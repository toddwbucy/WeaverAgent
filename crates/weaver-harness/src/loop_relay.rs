//! **The loop relay**, per `weaver-harness-loop-contract`: the harness's second
//! function of the entry's type, beside the compiled loop. It answers the
//! entry by sending the turn to a loop on the far end of one `SOCK_SEQPACKET`
//! pair, and calls, for each ask the loop makes, the same [`Seat`] method the
//! compiled loop calls. The harness authors every event either way: the relay
//! authors nothing, and the loop reaches nothing but its end of the pair.
//!
//! **The relay names only what it can see** (the contract's section 0). It
//! answers asks one at a time in arrival order and counts nothing outstanding.
//! Every break of the contract's section 4 is one it observes on its own end,
//! named in one line on the writer it is given (the worker's standard error
//! when deployed), and answered `Err(TurnError::ChannelLost)`.
//!
//! The relay's states are the contract's section 3.1. `Unannounced` lasts only
//! while [`LoopRelay::make`] reads the `hello`. `Open(N, a)` lasts only inside
//! one call of [`LoopRelay::pass`], so between calls the relay is `Idle` or
//! `Broken`.

use std::collections::HashSet;
use std::fmt;
use std::io::{IoSlice, IoSliceMut, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::poll::{PollFd, PollFlags, poll};
use nix::sys::socket::{MsgFlags, recvmsg, sendmsg};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Value, json};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{LOOP_PASS_ASKS, LOOP_PASS_BOUND_MS, MAX_ENVELOPE_BYTES, TurnContext};

use crate::channel::{reassemble_if_series, send_segmented};
use crate::engine::{TurnError, TurnOutcome};
use crate::failure::ChannelFault;
use crate::seat::Seat;
use crate::state::{Recalled, SessionShape};

/// The `hello`'s `version`, the contract's section 2.1.
pub const LOOP_VERSION: &str = "weaver-loop/1";

/// The relay's bounds: the `hello`'s own wait, the pass's summed waits, and
/// the asks a pass may make (the contract's sections 3 and 5). Deployed, they
/// are the floor's constants. A test sets shorter waits so a bound expires in
/// milliseconds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bounds {
    pub(crate) hello: Duration,
    pub(crate) pass: Duration,
    pub(crate) asks: u64,
}

impl Bounds {
    pub(crate) fn contract() -> Self {
        Bounds {
            hello: Duration::from_millis(LOOP_PASS_BOUND_MS),
            pass: Duration::from_millis(LOOP_PASS_BOUND_MS),
            asks: LOOP_PASS_ASKS,
        }
    }
}

/// Why making the relay failed: the `hello` was not read, or not the one the
/// relay was given to expect. What a failed `hello` does to a load is act 2's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloRefused {
    pub reason: String,
}

/// The relay over one end of the loop's pair, `Idle` or `Broken` between
/// passes.
pub struct LoopRelay<W: Write> {
    end: OwnedFd,
    bounds: Bounds,
    reasons: W,
    pass: u64,
    broken: bool,
}

impl<W: Write> LoopRelay<W> {
    /// Make the relay: read the loop's `hello` under its own bound and compare
    /// its version and its digest with `wires_sha256`, the digest of the wire
    /// file the loop was handed (the contract's section 3.1). The relay is
    /// `Idle` when this returns, and the harness is never handed one that has
    /// not read its `hello`.
    pub fn make(end: OwnedFd, wires_sha256: &str, reasons: W) -> Result<Self, HelloRefused> {
        Self::make_bounded(end, wires_sha256, reasons, Bounds::contract())
    }

    pub(crate) fn make_bounded(
        end: OwnedFd,
        wires_sha256: &str,
        mut reasons: W,
        bounds: Bounds,
    ) -> Result<Self, HelloRefused> {
        let mut budget = Budget { left: bounds.hello };
        let heard = recv_frame(end.as_fd(), &mut budget).and_then(|raw| hello(&raw, wires_sha256));
        if let Err(cause) = heard {
            let _ = writeln!(
                reasons,
                "weaver-harness: the loop relay refused the loop's hello: {cause}"
            );
            return Err(HelloRefused {
                reason: cause.to_string(),
            });
        }
        Ok(LoopRelay {
            end,
            bounds,
            reasons,
            pass: 0,
            broken: false,
        })
    }

    /// **The entry**: one pass, from `pass` to the loop's `turn` or `refuse`
    /// (the contract's section 3.2). A `turn` answers `seat.turn`'s result
    /// unchanged. A `refuse` answers `ChannelLost` and leaves the relay `Idle`.
    /// Every break answers `ChannelLost`, names its reason once, and leaves the
    /// relay `Broken`, where every later call answers `ChannelLost` at once and
    /// sends nothing.
    pub fn pass(&mut self, seat: &mut impl Seat, text: &str) -> Result<TurnOutcome, TurnError> {
        if self.broken {
            return Err(TurnError::ChannelLost);
        }
        match self.run(seat, text) {
            Ok(Ended::Turn(result)) => result,
            Ok(Ended::Refused) => Err(TurnError::ChannelLost),
            Err(cause) => {
                self.broken = true;
                let _ = writeln!(
                    self.reasons,
                    "weaver-harness: the loop relay broke: {cause}"
                );
                Err(TurnError::ChannelLost)
            }
        }
    }

    fn run(&mut self, seat: &mut impl Seat, text: &str) -> Result<Ended, Break> {
        let end = self.end.as_fd();
        // The look before a pass (the contract's section 3.2): a frame or an
        // end of file already standing is a break, whatever pass it names.
        if standing(end) {
            return Err(Break::Standing);
        }
        self.pass += 1;
        let open = self.pass;
        let mut budget = Budget {
            left: self.bounds.pass,
        };
        send_frame(
            end,
            &mut budget,
            &json!({"kind": "pass", "pass": open, "text": text}),
        )?;
        let mut asked: u64 = 0;
        let mut tags: HashSet<u64> = HashSet::new();
        loop {
            let raw = recv_frame(end, &mut budget)?;
            match loop_frame(&raw)? {
                LoopFrame::Refuse { pass } => {
                    if pass != open {
                        return Err(Break::OtherPass { named: pass, open });
                    }
                    return Ok(Ended::Refused);
                }
                LoopFrame::Ask { pass, tag, ask } => {
                    if pass != open {
                        return Err(Break::OtherPass { named: pass, open });
                    }
                    if asked >= self.bounds.asks {
                        return Err(Break::TooManyAsks(self.bounds.asks));
                    }
                    if !tags.insert(tag) {
                        return Err(Break::TagTwice(tag));
                    }
                    asked += 1;
                    let value = match ask {
                        Ask::Turn(delta) => return Ok(Ended::Turn(seat.turn(delta))),
                        Ask::Seated => json!(seat.seated()),
                        Ask::TurnContext => json!(match seat.turn_context() {
                            TurnContext::Continuous => "continuous",
                            TurnContext::Independent => "independent",
                        }),
                        Ask::FirstTurn => json!(seat.first_turn()),
                        Ask::Fullness => match seat.fullness() {
                            Some((resident, capacity)) => {
                                json!({"resident": resident, "capacity": capacity})
                            }
                            None => Value::Null,
                        },
                        Ask::Flush(keep) => match seat.flush(keep) {
                            Some((before, after)) => {
                                json!({"resident_before": before, "resident_after": after})
                            }
                            None => Value::Null,
                        },
                        Ask::Recall(last_turns) => seat
                            .recall(last_turns)
                            .map_or(Value::Null, |events| recalled(&events)),
                        Ask::Shape => seat
                            .session_shape()
                            .map_or(Value::Null, |shape| shaped(&shape)),
                    };
                    send_frame(
                        end,
                        &mut budget,
                        &json!({"kind": "return", "pass": open, "tag": tag, "value": value}),
                    )?;
                }
            }
        }
    }
}

enum Ended {
    Turn(Result<TurnOutcome, TurnError>),
    Refused,
}

/// A recall's events as the contract's section 2.2 carries them.
fn recalled(events: &[Recalled]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|event| {
                json!({
                    "kind": event.kind,
                    "run": event.run,
                    "turn": event.turn,
                    "sequence": event.sequence,
                    "pairs": event.pairs.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// A session's shape as the contract's section 2.2 carries it.
fn shaped(shape: &SessionShape) -> Value {
    json!({
        "runs": shape.runs.iter().map(|run| json!({
            "run": run.run,
            "kinds": run.kinds.iter().map(|(k, n)| json!([k, n])).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

// ---- The waits, summed on one budget per pass (the contract's section 5).

/// What is left of a pass's bound. Every wait on the socket, the poll and the
/// call after it, is taken from it, and nothing else is: the harness's time
/// inside a seat's call is not the loop's.
struct Budget {
    left: Duration,
}

impl Budget {
    fn spend(&mut self, since: Instant) {
        self.left = self.left.saturating_sub(since.elapsed());
    }

    /// One poll's timeout out of what is left, rounded up to a whole
    /// millisecond and clamped to what the call can be armed for.
    fn slice(&self) -> u16 {
        self.left
            .as_micros()
            .div_ceil(1000)
            .min(u128::from(u16::MAX)) as u16
    }
}

#[derive(Clone, Copy)]
enum Wait {
    Read,
    Send,
}

/// Wait for the end to be ready for `wait`, spending the wait from `budget`.
fn ready(end: BorrowedFd<'_>, budget: &mut Budget, wait: Wait) -> Result<(), Break> {
    let (flags, expired) = match wait {
        Wait::Read => (PollFlags::POLLIN, Break::BoundInRead),
        Wait::Send => (PollFlags::POLLOUT, Break::BoundInSend),
    };
    loop {
        if budget.left.is_zero() {
            return Err(expired);
        }
        let started = Instant::now();
        let mut fds = [PollFd::new(end, flags)];
        let polled = poll(&mut fds, budget.slice());
        budget.spend(started);
        match polled {
            Ok(0) | Err(Errno::EINTR) => continue,
            Ok(_) => return Ok(()),
            Err(_) => {
                return Err(match wait {
                    Wait::Read => Break::ReadFailed,
                    Wait::Send => Break::SendFailed,
                });
            }
        }
    }
}

/// One message off the end, under the budget.
fn recv_one(end: BorrowedFd<'_>, budget: &mut Budget) -> Result<Vec<u8>, Break> {
    let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
    loop {
        ready(end, budget, Wait::Read)?;
        let started = Instant::now();
        let mut slices = [IoSliceMut::new(&mut buffer)];
        let received = recvmsg::<()>(end.as_raw_fd(), &mut slices, None, MsgFlags::MSG_DONTWAIT)
            .map(|message| (message.bytes, message.flags));
        budget.spend(started);
        match received {
            Ok((_, flags)) if flags.contains(MsgFlags::MSG_TRUNC) => return Err(Break::Oversized),
            Ok((0, _)) => return Err(Break::EndOfFile),
            Ok((read, _)) => {
                buffer.truncate(read);
                return Ok(buffer);
            }
            Err(Errno::EAGAIN | Errno::EINTR) => continue,
            Err(_) => return Err(Break::ReadFailed),
        }
    }
}

/// One message onto the end, under the budget. A `SOCK_SEQPACKET` send is
/// whole or refused, so a send that reports fewer bytes is a break.
fn send_one(end: BorrowedFd<'_>, budget: &mut Budget, body: &[u8]) -> Result<(), Break> {
    loop {
        ready(end, budget, Wait::Send)?;
        let started = Instant::now();
        let slices = [IoSlice::new(body)];
        let sent = sendmsg::<()>(
            end.as_raw_fd(),
            &slices,
            &[],
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            None,
        );
        budget.spend(started);
        match sent {
            Ok(n) if n == body.len() => return Ok(()),
            Ok(_) => return Err(Break::SendUnfinished),
            Err(Errno::EAGAIN | Errno::EINTR) => continue,
            Err(_) => return Err(Break::SendFailed),
        }
    }
}

/// One frame off the end, reassembled where it crosses as a segment series
/// (the contract's section 2.4), every slice under the same budget.
fn recv_frame(end: BorrowedFd<'_>, budget: &mut Budget) -> Result<Vec<u8>, Break> {
    let first = recv_one(end, budget)?;
    let mut inner: Option<Break> = None;
    let whole = reassemble_if_series(first, || {
        recv_one(end, budget).map_err(|cause| {
            inner = Some(cause);
            ChannelFault::Closed
        })
    });
    whole.map_err(|_| inner.take().unwrap_or(Break::Series))
}

/// One frame onto the end, as one message or as a segment series.
fn send_frame(end: BorrowedFd<'_>, budget: &mut Budget, frame: &Value) -> Result<(), Break> {
    let body = serde_json::to_vec(frame).map_err(|_| Break::OwnFrame)?;
    let mut inner: Option<Break> = None;
    send_segmented(&body, |bytes| {
        send_one(end, budget, bytes).map_err(|cause| {
            inner = Some(cause);
            ChannelFault::Closed
        })
    })
    .map_err(|_| inner.take().unwrap_or(Break::OwnFrame))
}

/// The look before a pass, without waiting: true where a frame, an end of
/// file or an error stands on the end.
fn standing(end: BorrowedFd<'_>) -> bool {
    loop {
        let mut fds = [PollFd::new(end, PollFlags::POLLIN)];
        match poll(&mut fds, 0u16) {
            Ok(0) => return false,
            Err(Errno::EINTR) => continue,
            Ok(_) | Err(_) => return true,
        }
    }
}

// ---- The frames, read strictly (the contract's section 2).

/// A JSON value as the contract reads it: an integer only where it was
/// written as digits within 0 to 2^64 - 1, every other number kept apart, and
/// every object's members in order, refused where one is named twice.
#[derive(Debug, Clone, PartialEq)]
enum Strict {
    Null,
    Bool(bool),
    Int(u64),
    Number,
    Str(String),
    List(Vec<Strict>),
    Object(Vec<(String, Strict)>),
}

const TWICE: &str = "a member named twice in one object";

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Strict;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E>(self) -> Result<Strict, E> {
        Ok(Strict::Null)
    }
    fn visit_none<E>(self) -> Result<Strict, E> {
        Ok(Strict::Null)
    }
    fn visit_bool<E>(self, v: bool) -> Result<Strict, E> {
        Ok(Strict::Bool(v))
    }
    // serde_json hands a literal of digits alone to `visit_u64`, a negative
    // integer to `visit_i64`, and `-0`, a fraction, an exponent or a number
    // past 2^64 - 1 to `visit_f64`.
    fn visit_u64<E>(self, v: u64) -> Result<Strict, E> {
        Ok(Strict::Int(v))
    }
    fn visit_i64<E>(self, _: i64) -> Result<Strict, E> {
        Ok(Strict::Number)
    }
    fn visit_f64<E>(self, _: f64) -> Result<Strict, E> {
        Ok(Strict::Number)
    }
    fn visit_str<E>(self, v: &str) -> Result<Strict, E> {
        Ok(Strict::Str(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<Strict, E> {
        Ok(Strict::Str(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<Strict>()? {
            items.push(item);
        }
        Ok(Strict::List(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
        // A set beside the list keeps the check linear: an 8 MiB frame of short members
        // must not hold the harness's thread, whose time no bound counts.
        let mut members: Vec<(String, Strict)> = Vec::new();
        let mut names: HashSet<String> = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !names.insert(key.clone()) {
                return Err(de::Error::custom(TWICE));
            }
            let value = map.next_value::<Strict>()?;
            members.push((key, value));
        }
        Ok(Strict::Object(members))
    }
}

/// One frame's members, in order.
struct Frame {
    kind: &'static str,
    members: Vec<(String, Strict)>,
}

/// Parse one frame to its object and its known kind, or the break.
fn parse(raw: &[u8]) -> Result<(String, Vec<(String, Strict)>), Break> {
    let parsed: Strict = serde_json::from_slice(raw).map_err(|e| {
        let said = e.to_string();
        if said.contains(TWICE) {
            Break::Twice
        } else if said.contains("surrogate") || said.contains("end of hex escape") {
            // serde_json says "unexpected end of hex escape" for a leading
            // surrogate with no escape after it, and names the surrogate
            // otherwise (`read.rs`:900-960 at 1.0.151).
            Break::Surrogate
        } else {
            Break::NotJson
        }
    })?;
    let Strict::Object(members) = parsed else {
        return Err(Break::NotObject);
    };
    match members.iter().find(|(k, _)| k == "kind") {
        None => Err(Break::NoKind),
        Some((_, Strict::Str(kind))) => {
            let kind = kind.clone();
            Ok((kind, members))
        }
        Some(_) => Err(Break::KindNotString),
    }
}

impl Frame {
    /// The frame holds exactly `names`, each once: a member outside them is
    /// named first, then a member missing.
    fn exactly(&self, names: &[&str]) -> Result<(), Break> {
        if let Some((extra, _)) = self
            .members
            .iter()
            .find(|(k, _)| !names.contains(&k.as_str()))
        {
            return Err(Break::Extra {
                kind: self.kind,
                member: extra.clone(),
            });
        }
        if let Some(missing) = names
            .iter()
            .find(|name| !self.members.iter().any(|(k, _)| k == *name))
        {
            return Err(Break::Missing {
                kind: self.kind,
                member: (*missing).to_string(),
            });
        }
        Ok(())
    }

    fn get(&self, name: &str) -> &Strict {
        self.members
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
            .unwrap_or(&Strict::Null)
    }

    fn int(&self, name: &str) -> Result<u64, Break> {
        match self.get(name) {
            Strict::Int(n) => Ok(*n),
            Strict::Number => Err(Break::NotDigits {
                kind: self.kind,
                member: name.to_string(),
            }),
            _ => Err(Break::Type {
                kind: self.kind,
                member: name.to_string(),
                want: "an integer",
            }),
        }
    }

    fn string(&self, name: &str) -> Result<&str, Break> {
        match self.get(name) {
            Strict::Str(s) => Ok(s),
            _ => Err(Break::Type {
                kind: self.kind,
                member: name.to_string(),
                want: "a string",
            }),
        }
    }
}

/// The `hello`, read when the relay is made.
fn hello(raw: &[u8], wires_sha256: &str) -> Result<(), Break> {
    let (kind, members) = parse(raw)?;
    if kind != "hello" {
        return Err(Break::BeforeHello(kind));
    }
    let frame = Frame {
        kind: "hello",
        members,
    };
    frame.exactly(&["kind", "version", "wires_sha256"])?;
    if frame.string("version")? != LOOP_VERSION {
        return Err(Break::Version);
    }
    if frame.string("wires_sha256")? != wires_sha256 {
        return Err(Break::Digest);
    }
    Ok(())
}

/// The asks of the closed list (the contract's section 2.2), with their
/// arguments.
#[derive(Debug)]
enum Ask {
    Seated,
    TurnContext,
    FirstTurn,
    Fullness,
    Flush(u64),
    Recall(Option<u64>),
    Shape,
    Turn(Vec<Message>),
}

enum LoopFrame {
    Ask { pass: u64, tag: u64, ask: Ask },
    Refuse { pass: u64 },
}

/// One frame from the loop inside a pass.
fn loop_frame(raw: &[u8]) -> Result<LoopFrame, Break> {
    let (kind, members) = parse(raw)?;
    match kind.as_str() {
        "refuse" => {
            let frame = Frame {
                kind: "refuse",
                members,
            };
            frame.exactly(&["kind", "pass"])?;
            Ok(LoopFrame::Refuse {
                pass: frame.int("pass")?,
            })
        }
        "ask" => {
            let frame = Frame {
                kind: "ask",
                members,
            };
            let name = match frame.get("ask") {
                Strict::Str(name) => name.clone(),
                Strict::Null if !frame.members.iter().any(|(k, _)| k == "ask") => {
                    return Err(Break::Missing {
                        kind: "ask",
                        member: "ask".to_string(),
                    });
                }
                _ => {
                    return Err(Break::Type {
                        kind: "ask",
                        member: "ask".to_string(),
                        want: "a string",
                    });
                }
            };
            let arguments: &[&str] = match name.as_str() {
                "seated" | "turn_context" | "first_turn" | "fullness" | "shape" => &[],
                "flush" => &["keep"],
                "recall" => &["last_turns"],
                "turn" => &["delta"],
                _ => return Err(Break::NotListed(name)),
            };
            let mut names = vec!["kind", "pass", "tag", "ask"];
            names.extend_from_slice(arguments);
            frame.exactly(&names)?;
            let pass = frame.int("pass")?;
            let tag = frame.int("tag")?;
            let ask = match name.as_str() {
                "seated" => Ask::Seated,
                "turn_context" => Ask::TurnContext,
                "first_turn" => Ask::FirstTurn,
                "fullness" => Ask::Fullness,
                "shape" => Ask::Shape,
                "flush" => Ask::Flush(frame.int("keep")?),
                // Present either way (`exactly` above), null or an integer.
                "recall" => match frame.get("last_turns") {
                    Strict::Null => Ask::Recall(None),
                    _ => Ask::Recall(Some(frame.int("last_turns")?)),
                },
                _ => Ask::Turn(delta(frame.get("delta"))?),
            };
            Ok(LoopFrame::Ask { pass, tag, ask })
        }
        "hello" => Err(Break::SecondHello),
        "pass" | "return" => Err(Break::HarnessKind(kind)),
        _ => Err(Break::UnknownKind(kind)),
    }
}

/// A delta, checked by hand (the contract's section 2.2): at least one
/// message of exactly `role` and `content`, the role `system` or `user`, and
/// at least one block of exactly `type` `text` and a string `text`.
fn delta(value: &Strict) -> Result<Vec<Message>, Break> {
    let Strict::List(items) = value else {
        return Err(Break::DeltaList);
    };
    if items.is_empty() {
        return Err(Break::DeltaList);
    }
    let mut messages = Vec::with_capacity(items.len());
    for item in items {
        let Strict::Object(members) = item else {
            return Err(Break::DeltaMessage);
        };
        let has = |name: &str| members.iter().any(|(k, _)| k == name);
        if members.len() != 2 || !has("role") || !has("content") {
            return Err(Break::DeltaMessage);
        }
        let field = |name: &str| &members.iter().find(|(k, _)| k == name).expect("present").1;
        let role = match field("role") {
            Strict::Str(role) if role == "system" => Role::System,
            Strict::Str(role) if role == "user" => Role::User,
            Strict::Str(role) => return Err(Break::DeltaRole(role.clone())),
            _ => return Err(Break::DeltaRole("(not a string)".to_string())),
        };
        let Strict::List(blocks) = field("content") else {
            return Err(Break::DeltaContent);
        };
        if blocks.is_empty() {
            return Err(Break::DeltaContent);
        }
        let mut content = Vec::with_capacity(blocks.len());
        for block in blocks {
            let Strict::Object(members) = block else {
                return Err(Break::DeltaBlock);
            };
            let typed = members.len() == 2
                && members
                    .iter()
                    .any(|(k, v)| k == "type" && *v == Strict::Str("text".to_string()));
            let text = members.iter().find_map(|(k, v)| match (k.as_str(), v) {
                ("text", Strict::Str(text)) => Some(text.clone()),
                _ => None,
            });
            match (typed, text) {
                (true, Some(text)) => content.push(ContentBlock::Text { text }),
                _ => return Err(Break::DeltaBlock),
            }
        }
        messages.push(Message { role, content });
    }
    Ok(messages)
}

// ---- The breaks (the contract's section 4), each with its one line.

#[derive(Debug, Clone, PartialEq)]
enum Break {
    NotJson,
    Twice,
    Surrogate,
    NotObject,
    Series,
    NoKind,
    KindNotString,
    UnknownKind(String),
    HarnessKind(String),
    Extra {
        kind: &'static str,
        member: String,
    },
    Missing {
        kind: &'static str,
        member: String,
    },
    Type {
        kind: &'static str,
        member: String,
        want: &'static str,
    },
    NotDigits {
        kind: &'static str,
        member: String,
    },
    BeforeHello(String),
    Version,
    Digest,
    SecondHello,
    Standing,
    OtherPass {
        named: u64,
        open: u64,
    },
    TagTwice(u64),
    TooManyAsks(u64),
    NotListed(String),
    DeltaList,
    DeltaMessage,
    DeltaRole(String),
    DeltaContent,
    DeltaBlock,
    BoundInRead,
    BoundInSend,
    EndOfFile,
    ReadFailed,
    Oversized,
    SendFailed,
    SendUnfinished,
    OwnFrame,
}

impl fmt::Display for Break {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Break::NotJson => f.write_str("a frame that does not parse as JSON"),
            Break::Twice => f.write_str(TWICE),
            Break::Surrogate => f.write_str("a string holding a lone surrogate"),
            Break::NotObject => f.write_str("a frame that is not one JSON object"),
            Break::Series => f.write_str(
                "a frame with no kind that is not a segment preamble, or a series that breaks its rules",
            ),
            Break::NoKind => f.write_str("a frame with no kind"),
            Break::KindNotString => f.write_str("a frame whose kind is not a string"),
            Break::UnknownKind(kind) => write!(f, "an unknown kind {kind:?}"),
            Break::HarnessKind(kind) => write!(f, "the {kind} frame came from the loop, and only the harness sends it"),
            Break::Extra { kind, member } => write!(f, "a member outside the {kind} frame's: {member:?}"),
            Break::Missing { kind, member } => write!(f, "the {kind} frame lacks its member {member:?}"),
            Break::Type { kind, member, want } => write!(f, "the {kind} frame's {member:?} is not {want}"),
            Break::NotDigits { kind, member } => write!(
                f,
                "the {kind} frame's {member:?} is no integer of 0 to 2^64 - 1 written as digits"
            ),
            Break::BeforeHello(kind) => write!(f, "the {kind} frame came before the hello"),
            Break::Version => write!(f, "a hello whose version is not {LOOP_VERSION:?}"),
            Break::Digest => f.write_str("a hello whose digest is not the wire file's"),
            Break::SecondHello => f.write_str("a second hello"),
            Break::Standing => f.write_str("a frame or an end of file standing when a pass opens"),
            Break::OtherPass { named, open } => write!(f, "a frame naming pass {named}, not the open pass {open}"),
            Break::TagTwice(tag) => write!(f, "the tag {tag} used twice in one pass"),
            Break::TooManyAsks(bound) => write!(f, "an ask past the {bound} one pass may make"),
            Break::NotListed(name) => write!(f, "an ask outside the closed list: {name:?}"),
            Break::DeltaList => f.write_str("a delta that is not a list of at least one message"),
            Break::DeltaMessage => f.write_str("a delta message that is not exactly role and content"),
            Break::DeltaRole(role) => write!(f, "a delta role other than system or user: {role:?}"),
            Break::DeltaContent => {
                f.write_str("a delta message whose content is not a list of at least one block")
            }
            Break::DeltaBlock => {
                f.write_str("a delta block that is not exactly the type text and a string text")
            }
            Break::BoundInRead => f.write_str("the bound expired in a read"),
            Break::BoundInSend => f.write_str("the bound expired in a send"),
            Break::EndOfFile => f.write_str("the loop's end of the pair closed"),
            Break::ReadFailed => f.write_str("a read from the loop failed"),
            Break::Oversized => f.write_str("a message past the envelope's 64 KiB"),
            Break::SendFailed => f.write_str("a send to the loop failed"),
            Break::SendUnfinished => f.write_str("a send to the loop did not finish"),
            Break::OwnFrame => f.write_str("a frame of the relay's own past the total bound"),
        }
    }
}

#[cfg(test)]
mod tests;
