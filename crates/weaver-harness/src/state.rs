//! conforms: harness-restoring-open-seats-the-record
//! conforms: harness-session-opens-at-enter
//!
//! The state seam's ask end, per `weaver-harness-Spec` section 6: a clone
//! of the standing state channel held on the run beside the tee the enter
//! attaches, the ask written and the answer awaited on the serving thread
//! inside a bound this crate elects. Serialization on the shared channel is
//! the serving thread itself - the tee's feed and this ask both run on it,
//! so no ask can interleave a distillate's octets - and the member's
//! answers are the only traffic that ever flows toward this crate, so
//! whatever the wait reads is the answer or is malformed, with no third
//! case to disambiguate.
//!
//! **The open itself is `lifecycle.rs`'s**, which cites `harness-session-
//! opens-at-enter` for the enter's fan-out. This unit holds the ask the open
//! makes of the member, the identity material's one rule over its two sources
//! and the parked ask's bound, and no more of that clause than those.

use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;

/// The bound on the answer wait, per the contract's missing-answer clause:
/// generous against a member whose answer is one pass over its own
/// holdings, and an expiry is the dead peer converted into the same absence
/// a missing leg serves.
pub(crate) const ANSWER_BOUND_MS: u64 = 2_000;

/// The bound on an ask that parks at the member until the driver seals,
/// per `weaver-harness-state-contract` section 2: the enter's identity and
/// recall asks under a diagnostic binding or a restoring load wait on a
/// preload the operator runs beside the load, so the bound is the replay
/// ask's generous one and not the two seconds a member answering from
/// holdings at rest takes.
pub(crate) const PARKED_ASK_BOUND_MS: u64 = 600_000;

/// The bound on the answer's size, matched by the member's own cap on
/// what it renders: an answer still unframed past this many octets is not
/// the seam's traffic, and the seam retires rather than growing the turn
/// path's memory on a peer's behavior.
const ANSWER_BOUND_BYTES: usize = 1024 * 1024;

/// What one `poll` can be armed for, the system call taking milliseconds in
/// a `u16`. **It bounds one poll and never the wait**: a bound past it is
/// served by re-arming against what remains, the deadline check being the
/// only thing that ends the wait, so the replay ask's generous bound is
/// waited out rather than answered `None` at the ceiling by an arithmetic
/// the caller never asked for. Named rather than inlined so a test can
/// lower it and watch the re-arm in milliseconds instead of minutes.
#[cfg(not(test))]
const POLL_CEILING_MS: u16 = u16::MAX;
#[cfg(test)]
const POLL_CEILING_MS: u16 = 50;

/// The session's shape as the member answered it, per
/// `weaver-harness-state-contract` section 2: the runs in the order custody
/// first saw them, each with its held event counts by kind. The counts are
/// organized envelope fact and carry no judgment - what a count means to a
/// turn is the asking loop's business.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionShape {
    pub runs: Vec<RunShape>,
}

/// One run's shape: its reference and its event counts by kind, spelled as
/// the envelope spelled them.
#[derive(Debug, Clone, PartialEq)]
pub struct RunShape {
    pub run: String,
    pub kinds: Vec<(String, u64)>,
}

/// One event off either serving answer, per the contract: the envelope's
/// facts the asking loop composes with, and the elected pairs as custody
/// kept them - the distillate's own shape served back.
///
/// **The run travels beside the turn**, per `diagnostic-replay-loop`
/// section 3, whose walk groups the answer's events by run and turn from
/// their envelopes: two events carrying one turn label in different runs
/// are different events, and a shape that dropped the run would pair them
/// into a generation that never ran. The session does not travel, being
/// constant across any answer by the contract's session-bounded clause and
/// already held by the asker that declared it.
#[derive(Debug, Clone, PartialEq)]
pub struct Recalled {
    pub kind: String,
    pub run: String,
    pub turn: Option<String>,
    pub sequence: String,
    pub pairs: Vec<(String, String)>,
}

/// The harness's end of the serve direction: the ask, the bounded wait, and
/// the parse. Held on the run, granted to the seat, mintable nowhere else.
pub struct StateSeam {
    channel: UnixStream,
    /// The serve direction retires on its first failure: a late answer
    /// after a timed-out ask would be read as the next ask's answer,
    /// mis-attributing its position in the stream, so a seam that failed
    /// one ask answers no later one. The ingest direction is the tee's
    /// and is unaffected.
    dead: bool,
}

impl StateSeam {
    /// Crate-private, the port discipline: the enter constructs it from the
    /// standing channel's clone and nothing outside loop 0 can.
    pub(crate) fn new(channel: UnixStream) -> StateSeam {
        StateSeam {
            channel,
            dead: false,
        }
    }

    /// The shape ask: write the frame, await the answer inside the bound,
    /// parse. `None` is the contract's dead peer at the seat, whether the
    /// leg is down, the write refused, the bound expired, or the answer
    /// malformed, converted into the same absence a missing leg serves,
    /// and the serve direction retires on it.
    pub(crate) fn ask_shape(&mut self) -> Option<SessionShape> {
        if self.dead {
            return None;
        }
        let answered = self.exchange();
        if answered.is_none() {
            self.dead = true;
        }
        answered
    }

    fn exchange(&mut self) -> Option<SessionShape> {
        if !self.send(b"{\"ask\":{\"shape\":{}}}\n") {
            return None;
        }
        let line = self.await_line(ANSWER_BOUND_MS)?;
        parse_shape_answer(&line)
    }

    /// The recall ask, per the contract: the conversation as custody holds
    /// it, bounded to the most recent turns where a bound is given. The
    /// same one-strike economics as the shape ask, for the same
    /// mis-attribution reason.
    /// The boundary as the store states it, per the contract's `grants` ask
    /// of 2026-09-04: asked at the enter and the leave by the seat itself,
    /// never granted to a loop, and missing on the same three grounds as
    /// every other ask.
    pub(crate) fn ask_grants(&mut self) -> Option<Vec<String>> {
        if self.dead {
            return None;
        }
        let answered = self.grants_exchange();
        if answered.is_none() {
            self.dead = true;
        }
        answered
    }

    fn grants_exchange(&mut self) -> Option<Vec<String>> {
        if !self.send(b"{\"ask\":{\"grants\":{}}}\n") {
            return None;
        }
        let line = self.await_line(ANSWER_BOUND_MS)?;
        parse_grants_answer(&line)
    }

    /// The session's seated prefix as custody holds it, per the contract's
    /// `identity` ask of 2026-09-04: asked once at the enter before the
    /// decode open, inside the caller's bound, the parked one where the
    /// member's door stands per the contract's parking clause. An empty
    /// answer is an answer, the first load, and a miss is the one the enter
    /// does not convert.
    pub(crate) fn ask_identity_within(&mut self, bound_ms: u64) -> Option<Vec<Recalled>> {
        if self.dead {
            return None;
        }
        let answered = self.identity_exchange(bound_ms);
        if answered.is_none() {
            self.dead = true;
        }
        answered
    }

    fn identity_exchange(&mut self, bound_ms: u64) -> Option<Vec<Recalled>> {
        if !self.send(b"{\"ask\":{\"identity\":{}}}\n") {
            return None;
        }
        let line = self.await_line(bound_ms)?;
        parse_identity_answer(&line)
    }

    pub(crate) fn ask_recall(&mut self, last_turns: Option<u64>) -> Option<Vec<Recalled>> {
        self.ask_recall_within(last_turns, ANSWER_BOUND_MS)
    }

    /// The recall ask inside a caller's bound, the parked one at the enter
    /// of a restoring load, per the contract's parking clause.
    pub(crate) fn ask_recall_within(
        &mut self,
        last_turns: Option<u64>,
        bound_ms: u64,
    ) -> Option<Vec<Recalled>> {
        if self.dead {
            return None;
        }
        let answered = self.recall_exchange(last_turns, bound_ms);
        if answered.is_none() {
            self.dead = true;
        }
        answered
    }

    /// The replay ask, per the contract and `weaver-harness-Spec` section
    /// 6's 2026-08-24 clause: the session's elected events whole, in
    /// landing order, as the member answered them. **The bound is the
    /// caller's to pass**, because the replay ask is the one whose answer
    /// lawfully waits, parked at an open preload until the seal, and only
    /// the asking loop knows how long a preload is worth waiting on. The
    /// same one-strike economics as the other asks: a bound that expires
    /// retires the serve direction, a late answer otherwise reading as the
    /// next ask's.
    pub(crate) fn ask_replay(&mut self, bound_ms: u64) -> Option<Vec<Recalled>> {
        if self.dead {
            return None;
        }
        let answered = self.replay_exchange(bound_ms);
        if answered.is_none() {
            self.dead = true;
        }
        answered
    }

    fn replay_exchange(&mut self, bound_ms: u64) -> Option<Vec<Recalled>> {
        if !self.send(b"{\"ask\":{\"replay\":{}}}\n") {
            return None;
        }
        let line = self.await_line(bound_ms)?;
        parse_replay_answer(&line)
    }

    fn recall_exchange(&mut self, last_turns: Option<u64>, bound_ms: u64) -> Option<Vec<Recalled>> {
        let ask = match last_turns {
            Some(bound) => format!("{{\"ask\":{{\"recall\":{{\"last-turns\":{bound}}}}}}}\n"),
            None => "{\"ask\":{\"recall\":{}}}\n".to_string(),
        };
        if !self.send(ask.as_bytes()) {
            return None;
        }
        let line = self.await_line(bound_ms)?;
        parse_recall_answer(&line)
    }

    /// One frame whole or nothing, the tee's own economics: the channel is
    /// nonblocking - the flag rides the shared open file description the
    /// tee set - and a peer that cannot take a frame now is not waited on.
    fn send(&mut self, mut bytes: &[u8]) -> bool {
        while !bytes.is_empty() {
            match self.channel.write(bytes) {
                Ok(0) => return false,
                Ok(written) => bytes = &bytes[written..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
        true
    }

    /// One poll's timeout out of what remains of the wait, clamped to what
    /// the system call can be armed for, per `POLL_CEILING_MS`.
    fn poll_slice(remaining: std::time::Duration) -> u16 {
        remaining.as_millis().min(u128::from(POLL_CEILING_MS)) as u16
    }

    /// Await one line inside the bound. The channel shares the tee's
    /// nonblocking flag, so the wait is a poll deadline rather than a read
    /// timeout.
    fn await_line(&mut self, bound_ms: u64) -> Option<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(bound_ms);
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            if let Some(position) = buffer.iter().position(|&b| b == b'\n') {
                return Some(String::from_utf8_lossy(&buffer[..position]).into_owned());
            }
            if buffer.len() > ANSWER_BOUND_BYTES {
                return None;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let wait = Self::poll_slice(remaining);
            let mut fds = [nix::poll::PollFd::new(
                self.channel.as_fd(),
                nix::poll::PollFlags::POLLIN,
            )];
            match nix::poll::poll(&mut fds, wait) {
                Ok(0) => continue,
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => return None,
            }
            let mut chunk = [0u8; 65536];
            match self.channel.read(&mut chunk) {
                Ok(0) => return None,
                Ok(count) => buffer.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
    }
}

/// Parse the shape answer, per the contract:
/// `{"answer":{"shape":{"runs":[{"run":...,"kinds":{...}}]}}}`. A frame
/// that does not carry the whole shape is malformed and answers nothing.
fn parse_shape_answer(line: &str) -> Option<SessionShape> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let runs = value.get("answer")?.get("shape")?.get("runs")?.as_array()?;
    let mut shaped = Vec::with_capacity(runs.len());
    for entry in runs {
        let run = entry.get("run")?.as_str()?.to_string();
        let kinds = entry
            .get("kinds")?
            .as_object()?
            .iter()
            .map(|(kind, count)| Some((kind.clone(), count.as_u64()?)))
            .collect::<Option<Vec<_>>>()?;
        shaped.push(RunShape { run, kinds });
    }
    Some(SessionShape { runs: shaped })
}

/// Parse the replay answer, per the contract:
/// `{"answer":{"replay":{"events":[{"envelope":{...},"pairs":{...}}]}}}`.
/// The recall's shape under the replay's name, and a frame that does not
/// carry the whole shape answers nothing.
fn parse_replay_answer(line: &str) -> Option<Vec<Recalled>> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let events = value
        .get("answer")?
        .get("replay")?
        .get("events")?
        .as_array()?;
    parse_recalled_events(events)
}

/// The identity answer's messages, or nothing where the frame is not one:
/// `{"answer":{"identity":{"messages":[...]}}}`, each event the
/// distillate's shape.
fn parse_identity_answer(line: &str) -> Option<Vec<Recalled>> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let events = value
        .get("answer")?
        .get("identity")?
        .get("messages")?
        .as_array()?;
    parse_recalled_events(events)
}

/// **The open's identity material, two sources and one rule**, per
/// `weaver-harness-Spec` section 6.1 as of 2026-09-04. `None` for the answer
/// is the ask missed: the enter refuses rather than opening a run with no
/// bounding. An empty answer is the first load of the session, so the
/// declaration's field seeds the open. A prefix answered is the open's
/// messages, each rebuilt from the pairs the tee carried whole, `role` and
/// `content`, and a prefix that does not rebuild refuses the same way a
/// miss does, because a half-read bounding is no bounding.
pub(crate) fn identity_material(
    answer: Option<Vec<Recalled>>,
    seed: &[weaver_traits::Message],
) -> Option<Vec<weaver_traits::Message>> {
    let held = answer?;
    if held.is_empty() {
        return Some(seed.to_vec());
    }
    held.iter().map(prefix_message).collect()
}

/// **The restored conversation, rebuilt from the recall answer**, per
/// `weaver-harness-Spec` section 6.1. `None` for the answer is the ask
/// missed and refuses the enter. The turned message events rebuild as
/// canonical messages in landing order, and so does a `message.restored`
/// row, the conversation a branch inherited and recorded turnless at its own
/// open, so a restore from a branch reopens with it (#697). The other
/// turnless rows are the identity ask's, seated once by it and skipped here,
/// so a prefix does not reach the open twice. A message that does not
/// rebuild refuses the same way a miss does.
pub(crate) fn restored_conversation(
    answer: Option<Vec<Recalled>>,
) -> Option<Vec<weaver_traits::Message>> {
    let held = answer?;
    held.iter()
        .filter(|event| event.turn.is_some() || event.kind == "message.restored")
        .map(prefix_message)
        .collect()
}

fn prefix_message(event: &Recalled) -> Option<weaver_traits::Message> {
    let role = event
        .pairs
        .iter()
        .find(|(key, _)| key == "role")?
        .1
        .as_str();
    let content = event
        .pairs
        .iter()
        .find(|(key, _)| key == "content")?
        .1
        .as_str();
    serde_json::from_str(&format!("{{\"role\":{role},\"content\":{content}}}")).ok()
}

/// The grants answer's surface, or nothing where the frame is not one:
/// `{"answer":{"grants":{"surface":["...", ...]}}}`, every entry a string.
fn parse_grants_answer(line: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let surface = value
        .get("answer")?
        .get("grants")?
        .get("surface")?
        .as_array()?;
    surface
        .iter()
        .map(|entry| entry.as_str().map(str::to_string))
        .collect()
}

/// Parse the recall answer, per the contract:
/// `{"answer":{"recall":{"events":[{"envelope":{...},"pairs":{...}}]}}}`.
/// A frame that does not carry the whole shape answers nothing.
fn parse_recall_answer(line: &str) -> Option<Vec<Recalled>> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let events = value
        .get("answer")?
        .get("recall")?
        .get("events")?
        .as_array()?;
    parse_recalled_events(events)
}

/// One event list off either serving answer, the distillate's shape read
/// back: envelope fields by name, pairs as raw text.
fn parse_recalled_events(events: &[serde_json::Value]) -> Option<Vec<Recalled>> {
    let mut recalled = Vec::with_capacity(events.len());
    for event in events {
        let envelope = event.get("envelope")?;
        let pairs = event
            .get("pairs")
            .and_then(|p| p.as_object())
            .map(|object| {
                object
                    .iter()
                    .map(|(key, val)| (key.clone(), val.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        recalled.push(Recalled {
            kind: envelope.get("kind")?.as_str()?.to_string(),
            run: envelope.get("run")?.as_str()?.to_string(),
            turn: envelope
                .get("turn")
                .and_then(|t| t.as_str())
                .map(str::to_string),
            sequence: envelope.get("sequence")?.as_str()?.to_string(),
            pairs,
        });
    }
    Some(recalled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The answer's wire spelling parses to the shape, and a frame missing
    /// any member of it answers nothing.
    #[test]
    fn the_shape_answer_parses_whole_or_not_at_all() {
        let line = concat!(
            r#"{"answer":{"shape":{"runs":[{"run":"r-1","kinds":{"load":1,"#,
            r#""turn.closed":3}},{"run":"r-2","kinds":{"load":1}}]}}}"#
        );
        let shape = parse_shape_answer(line).expect("parses");
        assert_eq!(shape.runs.len(), 2);
        assert_eq!(shape.runs[0].run, "r-1");
        assert!(
            shape.runs[0]
                .kinds
                .contains(&("turn.closed".to_string(), 3))
        );
        for malformed in [
            r#"{"answer":{"shape":{}}}"#,
            r#"{"answer":{}}"#,
            r#"{"answer":{"shape":{"runs":[{"kinds":{}}]}}}"#,
            "not json",
        ] {
            assert!(parse_shape_answer(malformed).is_none(), "{malformed}");
        }
    }

    /// The bounded wait converts a silent peer into a missing answer: a
    /// member that never speaks costs the answer inside the bound, never a
    /// hang. The bound is the constant's, so the test waits it out once.
    #[test]
    fn a_silent_peer_costs_the_answer_inside_the_bound() {
        let (ours, _theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let started = std::time::Instant::now();
        assert!(seam.ask_shape().is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    /// The serve direction retires on its first failure: a malformed
    /// answer costs its ask, and a well-formed answer already waiting
    /// behind it is never read, because attributing it to a later ask
    /// would misplace its position in the stream.
    #[test]
    fn the_seam_retires_on_its_first_failure() {
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let mut peer = theirs;
        peer.write_all(b"garbage\n{\"answer\":{\"shape\":{\"runs\":[]}}}\n")
            .expect("answers in advance");
        assert!(seam.ask_shape().is_none(), "a malformed answer misses");
        assert!(
            seam.ask_shape().is_none(),
            "the retired seam never reads the late well-formed answer"
        );
    }

    /// **The grants ask reads the surface as strings and nothing else**, per
    /// the contract's fourth ask of 2026-09-04. Perturbation: accept a
    /// non-string entry and the second case answers a surface with a hole
    /// in it, which two readings would then compare as equal across.
    #[test]
    fn the_grants_ask_reads_the_surface_whole_or_not_at_all() {
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let mut peer = theirs;
        peer.write_all(b"{\"answer\":{\"grants\":{\"surface\":[\"owner 0:0\",\"mode 0640\"]}}}\n")
            .expect("answers in advance");
        let surface = seam.ask_grants().expect("answered");
        assert_eq!(
            surface,
            vec!["owner 0:0".to_string(), "mode 0640".to_string()]
        );
        let mut asked = [0u8; 64];
        let n = peer.read(&mut asked).expect("reads the ask");
        assert_eq!(
            &asked[..n],
            b"{\"ask\":{\"grants\":{}}}\n",
            "the ask carries no members"
        );

        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let mut peer = theirs;
        peer.write_all(b"{\"answer\":{\"grants\":{\"surface\":[\"owner 0:0\",7]}}}\n")
            .expect("answers in advance");
        assert!(
            seam.ask_grants().is_none(),
            "a non-string entry is a malformed answer"
        );
    }

    /// **The restored conversation is the turned messages and the prefix is
    /// seated once**, per `weaver-harness-Spec` section 6.1: the turnless
    /// system row is the identity ask's and is skipped, the turned user and
    /// assistant rows rebuild in landing order, and a miss refuses.
    ///
    /// Perturbation: drop the turn filter and the system row is seated a
    /// second time, the first assertion failing on the count. Watched under
    /// exactly that removal.
    #[test]
    fn the_restored_conversation_is_the_turned_messages_seated_once() {
        let recalled = |turn: Option<&str>, kind: &str, role: &str, text: &str| Recalled {
            kind: kind.into(),
            run: "r-1".into(),
            turn: turn.map(str::to_string),
            sequence: "1".into(),
            pairs: vec![
                ("role".into(), format!("\"{role}\"")),
                (
                    "content".into(),
                    format!("[{{\"type\":\"text\",\"text\":\"{text}\"}}]"),
                ),
            ],
        };
        let answer = vec![
            recalled(None, "message.system", "system", "You are Karl."),
            recalled(Some("t-1"), "message.user", "user", "hello"),
            recalled(Some("t-1"), "message.assistant", "assistant", "hi"),
        ];
        let messages = restored_conversation(Some(answer)).expect("rebuilds");
        assert_eq!(messages.len(), 2, "the turnless prefix is the identity's");
        assert!(matches!(messages[0].role, weaver_traits::Role::User));
        assert!(matches!(messages[1].role, weaver_traits::Role::Assistant));
        assert!(restored_conversation(None).is_none(), "a miss refuses");
    }

    /// **The identity ask's three arms**, per `weaver-harness-Spec` section
    /// 6.1 as of 2026-09-04: a miss is `None` and refuses, an empty answer
    /// seeds, and a prefix rebuilds from its pairs. Perturbation: return
    /// the seed on a miss and the first assertion fails; skip the rebuild's
    /// `content` and the third.
    #[test]
    fn the_identity_material_has_two_sources_and_one_rule() {
        let seed = vec![weaver_traits::Message {
            role: weaver_traits::Role::System,
            content: vec![weaver_traits::ContentBlock::Text {
                text: "seed".into(),
            }],
        }];
        assert!(identity_material(None, &seed).is_none(), "a miss refuses");
        assert_eq!(
            identity_material(Some(vec![]), &seed),
            Some(seed.clone()),
            "an empty answer is the first load and seeds"
        );
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let mut peer = theirs;
        peer.write_all(
            concat!(
                r#"{"answer":{"identity":{"messages":[{"envelope":{"session":"s","run":"r-1","#,
                r#""kind":"message.system","sequence":"3"},"pairs":{"role":"system","#,
                r#""content":[{"type":"text","text":"You are Karl."}]}}]}}}"#,
                "\n"
            )
            .as_bytes(),
        )
        .expect("answers in advance");
        let held = seam.ask_identity_within(ANSWER_BOUND_MS).expect("answered");
        let material = identity_material(Some(held), &seed).expect("rebuilds");
        assert_eq!(material.len(), 1);
        assert!(matches!(material[0].role, weaver_traits::Role::System));
        assert!(
            matches!(&material[0].content[0], weaver_traits::ContentBlock::Text { text } if text == "You are Karl.")
        );
        let mut asked = [0u8; 64];
        let n = peer.read(&mut asked).expect("reads the ask");
        assert_eq!(&asked[..n], b"{\"ask\":{\"identity\":{}}}\n");
        let half = vec![Recalled {
            kind: "message.system".into(),
            run: "r-1".into(),
            turn: None,
            sequence: "3".into(),
            pairs: vec![("role".into(), "\"system\"".into())],
        }];
        assert!(
            identity_material(Some(half), &seed).is_none(),
            "a prefix without content refuses"
        );
    }

    /// An answer still unframed past the byte bound retires the seam
    /// inside the bound rather than growing the turn path's memory.
    #[test]
    fn an_unframed_flood_retires_the_seam_inside_the_bound() {
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let responder = std::thread::spawn(move || {
            let mut peer = theirs;
            let mut taken = [0u8; 256];
            let _ = peer.read(&mut taken).expect("reads the ask");
            let flood = vec![b'x'; 2 * ANSWER_BOUND_BYTES];
            let _ = peer.write_all(&flood);
        });
        let started = std::time::Instant::now();
        assert!(seam.ask_shape().is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        drop(seam);
        responder.join().expect("responder");
    }

    /// A peer that answers is read whole across the seam.
    #[test]
    fn an_answering_peer_is_read() {
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let responder = std::thread::spawn(move || {
            let mut peer = theirs;
            let mut taken = [0u8; 256];
            let count = peer.read(&mut taken).expect("reads the ask");
            assert_eq!(&taken[..count], b"{\"ask\":{\"shape\":{}}}\n");
            peer.write_all(
                b"{\"answer\":{\"shape\":{\"runs\":[{\"run\":\"r-1\",\"kinds\":{\"load\":1}}]}}}\n",
            )
            .expect("answers");
        });
        let shape = seam.ask_shape().expect("answered");
        assert_eq!(shape.runs[0].run, "r-1");
        responder.join().expect("responder");
    }

    /// **A bound past the poll's ceiling is waited out, not answered at
    /// the ceiling**, per the replay port's clause: `poll` takes a `u16` of
    /// milliseconds, and a wait that ended when one poll expired would
    /// answer `None` early to a caller who asked for longer - on precisely
    /// the ask whose clause exists so the loop may wait as long as a
    /// preload is worth. Both halves are watched here, the arithmetic and
    /// the re-arm, the ceiling being lowered under `cfg(test)` so the
    /// behavioural half costs milliseconds rather than the minute the
    /// production ceiling would.
    ///
    /// Perturbation: read a clamped poll's expiry as the deadline and the
    /// silent-peer half fails, returning near the ceiling instead of near
    /// the bound. The defect this watch exists for was measured live at
    /// the review seat before the re-arm landed, an `ask_replay(120_000)`
    /// answering `None` at 65.551 seconds.
    #[test]
    fn a_bound_past_the_ceiling_is_waited_out() {
        use std::time::Duration;
        // The arithmetic: one poll is a slice of what remains.
        assert_eq!(
            StateSeam::poll_slice(Duration::from_millis(120_000)),
            POLL_CEILING_MS
        );
        assert_eq!(StateSeam::poll_slice(Duration::from_millis(10)), 10);
        assert_eq!(StateSeam::poll_slice(Duration::from_millis(0)), 0);

        // The re-arm: a silent peer and a bound many ceilings long is
        // waited out to the bound, the deadline ending the wait and not
        // the poll.
        let bound_ms = u64::from(POLL_CEILING_MS) * 8;
        let (ours, _theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let started = std::time::Instant::now();
        assert!(seam.ask_replay(bound_ms).is_none(), "the silence costs it");
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(bound_ms),
            "the wait reaches the caller's bound, not the ceiling: {waited:?}"
        );
    }

    /// **The recall answer parses under its own name**, the sibling of the
    /// replay parse and the other caller of the shared event read: the
    /// envelope's facts including the run, the pairs as custody kept them,
    /// and a frame under the replay's name is not a recall answer. Nothing
    /// watched this path before the audit of 2026-08-26 found the shared
    /// refactor unwatched on the recall side.
    #[test]
    fn the_recall_answer_parses_under_its_own_name() {
        let events = parse_recall_answer(concat!(
            r#"{"answer":{"recall":{"events":[{"envelope":{"session":"s","run":"r-2","#,
            r#""turn":"t-9","kind":"message.user","sequence":"11"},"#,
            r#""pairs":{"content":[{"type":"text","text":"the plan"}]}}]}}}"#
        ))
        .expect("parses");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].run, "r-2");
        assert_eq!(events[0].turn.as_deref(), Some("t-9"));
        assert_eq!(events[0].kind, "message.user");
        assert_eq!(events[0].pairs.len(), 1);
        assert_eq!(events[0].pairs[0].0, "content");
        assert!(
            events[0].pairs[0].1.contains("the plan"),
            "the pairs cross as custody kept them"
        );
        assert!(
            parse_recall_answer(r#"{"answer":{"replay":{"events":[]}}}"#).is_none(),
            "the two answers stay apart by name"
        );
        assert!(
            parse_recall_answer(concat!(
                r#"{"answer":{"recall":{"events":[{"envelope":{"session":"s",""#,
                r#"turn":"t-9","kind":"message.user","sequence":"11"},"pairs":{}}]}}}"#
            ))
            .is_none(),
            "an envelope missing the run fails the parse whole"
        );
    }

    /// **The replay ask crosses under its own name and reads the answer's
    /// shape back**, per `weaver-harness-Spec` section 6's 2026-08-24
    /// clause: the wire ask is the contract's, the caller's bound is the
    /// wait, and the answer parses as the distillate's shape served back,
    /// envelope and pairs, whole or not at all. A frame under the recall's
    /// name is not a replay answer, which is what pins the two parses
    /// apart.
    #[test]
    fn the_replay_ask_crosses_and_its_answer_parses() {
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let responder = std::thread::spawn(move || {
            let mut peer = theirs;
            let mut taken = [0u8; 256];
            let count = peer.read(&mut taken).expect("reads the ask");
            assert_eq!(&taken[..count], b"{\"ask\":{\"replay\":{}}}\n");
            peer.write_all(
                b"{\"answer\":{\"replay\":{\"events\":[{\"envelope\":{\"session\":\"s\",\
                \"run\":\"r-1\",\"kind\":\"model.output\",\"sequence\":\"4\"},\
                \"pairs\":{}}]}}}\n",
            )
            .expect("answers");
        });
        let events = seam.ask_replay(2_000).expect("answered");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "model.output");
        assert_eq!(events[0].run, "r-1");
        assert_eq!(events[0].sequence, "4");
        responder.join().expect("responder");

        // The recall's name does not parse as a replay answer.
        assert!(
            parse_replay_answer("{\"answer\":{\"recall\":{\"events\":[]}}}").is_none(),
            "the two answers stay apart by name"
        );
    }
}
