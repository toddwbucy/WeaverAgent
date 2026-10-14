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
//! whatever the wait reads is the answer, a snapshot-protocol frame owed to
//! a missed save-point leg and dropped by its kind, or malformed.
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

/// **The bound on the snapshot's answer leg**, per `weaver-harness-Spec`
/// section 6 on the operator's ruling of 2026-10-08: the member writes the
/// image before it answers, so the leg is waited on for 120 seconds, enough
/// to write a 1 GiB image. The ask leg and the finished leg, and the small
/// asks, keep `ANSWER_BOUND_MS`; `restored` and the parked asks have their
/// own bounds below.
pub(crate) const SNAPSHOT_ANSWER_BOUND_MS: u64 = 120_000;

/// **The bound on the `restored` answer**, per `weaver-harness-Spec` section
/// 6.1 on the operator's ruling of 2026-10-08 on #99 (N1): the member
/// restores the save point admin handed only after the opener reaches it,
/// judging and seating an image of up to the save point's 1 GiB bound, and
/// answers `restored` after that, so the ask is waited on as the snapshot's
/// answer leg is, 120 seconds, and never on the small asks' two.
pub(crate) const RESTORED_ANSWER_BOUND_MS: u64 = 120_000;

/// The bound on an ask that parks at the member until the driver seals,
/// per `weaver-harness-state-contract` section 2: the enter's asks under a
/// diagnostic binding wait on a preload the operator runs beside the load,
/// so the bound is the replay ask's generous one and not the two seconds a
/// member answering from holdings at rest takes. A restoring load parks
/// nothing: its save point arrives on a descriptor at the member's start,
/// and its `restored` answer has its own bound above.
pub(crate) const PARKED_ASK_BOUND_MS: u64 = 600_000;

/// The bound on the answer's size, one mebibyte per
/// `weaver-harness-state-contract` section 3, the one number both ends
/// hold: the member renders no answer frame past it and this end reads none
/// past it, an answer still unframed past this many octets being not the
/// seam's traffic, so the seam retires rather than growing the turn path's
/// memory on a peer's behavior. **The seeding path spends it at the
/// sender**, per `weaver-harness-Spec` section 6.1: a seeding whose addition
/// to the residency's prefix would carry the identity answer past it refuses
/// as a turn, since a prefix the member cannot answer is a prefix the next
/// load cannot seat.
pub(crate) const ANSWER_BOUND_BYTES: usize = 1024 * 1024;

/// What one seeded message costs in the identity answer as the member
/// renders it, per `weaver-harness-state-contract` section 2: the message's
/// canonical rendering as the `pairs`, the envelope naming the session, the
/// run, the kind and the sequence, and the frame's own text around the
/// list. **Every member the envelope carries is costed as its canonical
/// rendering**, the names as the JSON strings the member writes, escapes
/// included, so the sum is never under the custodian's count, the variable
/// members exact; the kind and the sequence are fixed ASCII and ride in
/// `ENVELOPE_BYTES` with the envelope's own text, a sequence of up to twenty
/// digits inside it, and `ANSWER_FRAME_BYTES` covers
/// `{"answer":{"identity":{"messages":[]}}}` and the delimiter, each allowed
/// for above so the bound refuses before the seam would and never after
/// (#93 holds the exact measure as a cost with no correctness bearing).
pub(crate) const ANSWER_FRAME_BYTES: usize = 64;
pub(crate) const ENVELOPE_BYTES: usize = 128;

pub(crate) fn identity_entry_cost(rendered_message: usize, session: &str, run: &str) -> usize {
    rendered_message + rendered_name(session) + rendered_name(run) + ENVELOPE_BYTES
}

/// A name's length as the member renders it inside its quotes: the JSON
/// string's escapes counted, the two quotes themselves being the envelope's
/// text.
fn rendered_name(name: &str) -> usize {
    serde_json::to_string(name).map_or(name.len(), |json| json.len() - 2)
}

/// What one `poll` can be armed for, the system call taking milliseconds in
/// a `u16`. **It bounds one poll and never the wait**: a bound past it is
/// served by re-arming against what remains, the deadline check being the
/// only thing that ends the wait, so the replay ask's generous bound is
/// waited out rather than answered `None` at the ceiling by an arithmetic
/// the caller never asked for. Named rather than inlined so a test can
/// lower it and watch the re-arm in milliseconds instead of minutes.
#[cfg(not(test))]
const POLL_CEILING_MS: u16 = u16::MAX;
// Test-only hook kept here (#118): the test value of a ceiling the production wait reads.
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

/// A save point's stamp as the member answers it, per
/// `weaver-harness-state-contract` section 2: the save point's digest, and
/// the trace position it covers, the run and sequence of the last distillate
/// it holds and the last turn that run holds in it. The four members the
/// enter compares with its lineage, and the load event names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePointStamp {
    pub digest: String,
    pub run: String,
    pub sequence: u64,
    pub turn: u64,
}

/// The `snapshot` answer: the name the member wrote the save point under in
/// its own room, and its stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePointTaken {
    pub name: String,
    pub stamp: SavePointStamp,
}

/// The `restored` answer, per the contract's `restored` ask: the stamp of the
/// save point the load restored, nothing where the member stood empty, or a
/// refusal naming its reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoredAnswer {
    Lineage(SavePointStamp),
    Empty,
    Refused(String),
}

/// The harness's end of the serve direction: the ask, the bounded wait, and
/// the parse. Held on the run, granted to the seat, mintable nowhere else.
pub struct StateSeam {
    channel: UnixStream,
    /// The serve direction retires on its first failure: a late answer
    /// after a timed-out ask would be read as the next ask's answer,
    /// mis-attributing its position in the stream, so a seam that failed
    /// one ask answers no later one. The ingest direction is the tee's
    /// and is unaffected. **The snapshot's answer and finished legs are
    /// the exception**, per `weaver-harness-Spec` section 6: a miss there
    /// leaves the run entered for the operator's retry, so the seam stays
    /// alive and settles instead, below.
    dead: bool,
    /// A snapshot leg missed its bound with the member alive: whatever the
    /// member sends late, a whole line or part of one, is owed to no ask,
    /// so the next ask drains it before it is sent, per
    /// `weaver-harness-Spec` section 6, and the retry's answer is read as
    /// the retry's. A frame landing after the drain is dropped by the wait:
    /// by its number at a snapshot ask, by its kind at every other.
    unsettled: bool,
    /// **The drain ended inside a line** (#140): the rest of that late frame
    /// is still owed to no ask, so the next line read is discarded where it
    /// is that rest, and the retry's answer is never read behind a suffix
    /// that would miss it. A line that parses whole is no rest: every frame
    /// is one JSON object, and a proper suffix of one never parses, so the
    /// part drained was never finished and the line is the next answer.
    discarding: bool,
    /// **The next snapshot ask's ordinal**, per residency from 1 (Codex on
    /// #94, round 10): the ask carries it, the member echoes it on the answer
    /// and the finished answer, and an answer carrying another number is a
    /// late one and is dropped by its number rather than by timing, so a
    /// slow member's answer to the ask before never passes as the retry's.
    snapshot_ordinal: u64,
    /// Bytes read past the last answered line, kept for the next await: an
    /// answer that arrived in the same read as the one before it is the next
    /// exchange's, never dropped, which the four-leg save point of A3.2
    /// depends on where the member answers ahead.
    residual: Vec<u8>,
    /// The snapshot answer leg's bound, `SNAPSHOT_ANSWER_BOUND_MS` on every
    /// seam the crate builds; a test that needs the leg to miss shortens it
    /// rather than waiting two minutes out.
    pub(crate) snapshot_answer_bound_ms: u64,
    /// The `restored` ask's bound, `RESTORED_ANSWER_BOUND_MS` on every seam
    /// the crate builds; a test that needs it to miss shortens it.
    restored_answer_bound_ms: u64,
}

impl StateSeam {
    /// Crate-private, the port discipline: the enter constructs it from the
    /// standing channel's clone and nothing outside loop 0 can.
    pub(crate) fn new(channel: UnixStream) -> StateSeam {
        StateSeam {
            channel,
            dead: false,
            unsettled: false,
            discarding: false,
            snapshot_ordinal: 1,
            residual: Vec::new(),
            snapshot_answer_bound_ms: SNAPSHOT_ANSWER_BOUND_MS,
            restored_answer_bound_ms: RESTORED_ANSWER_BOUND_MS,
        }
    }

    /// Whether the seam has retired on a failure, the member found dead.
    pub(crate) fn dead(&self) -> bool {
        self.dead
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
        self.await_answer(ANSWER_BOUND_MS, parse_shape_answer)
    }

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
        self.await_answer(ANSWER_BOUND_MS, parse_grants_answer)
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
        self.await_answer(bound_ms, parse_identity_answer)
    }

    /// The recall ask, per the contract: the conversation as custody holds
    /// it, bounded to the most recent turns where a bound is given. The
    /// same one-strike economics as the shape ask, for the same
    /// mis-attribution reason.
    pub(crate) fn ask_recall(&mut self, last_turns: Option<u64>) -> Option<Vec<Recalled>> {
        self.ask_recall_within(last_turns, ANSWER_BOUND_MS)
    }

    /// The `restored` ask, per the contract's `restored` ask of 2026-10-02: what
    /// the load restored, asked at every serving enter after the opener and
    /// before `load` is authored. A miss is `None`, the one ask beside the
    /// diagnostic identity's that the dead-peer clause does not convert: the
    /// enter refuses on it.
    pub(crate) fn ask_restored(&mut self) -> Option<RestoredAnswer> {
        if self.dead {
            return None;
        }
        let answered = self.restored_exchange();
        if answered.is_none() {
            self.dead = true;
        }
        answered
    }

    fn restored_exchange(&mut self) -> Option<RestoredAnswer> {
        if !self.send(b"{\"ask\":{\"restored\":{}}}\n") {
            return None;
        }
        self.await_answer(self.restored_answer_bound_ms, parse_restored_answer)
    }

    /// **The `snapshot` ask's four legs**, per the contract's sixth ask of
    /// 2026-10-02 as amended on the operator's ruling of 2026-10-06 on #1
    /// (A3.0 item 4): the ask, the answer naming the finished name the part
    /// will take and its stamp, this end's `acknowledge` of the digest, and
    /// the member's `finished` answer, on which the caller authors the
    /// `save_point` event. Sent at every serving leave before `unload`, and
    /// on the operator's demand. **A missed leg names itself**, since the
    /// leave does not complete without its save point and the operator reads
    /// which leg missed: the member dead before the ask, the answer, or the
    /// `finished` answer; the write's own failure reaches this end as the
    /// answer missing, the member answering nothing for it. **A missed
    /// answer or finished leg retires nothing** (Codex on #94, round 4):
    /// the member is alive and the operator retries, per the Spec, so the
    /// seam marks itself unsettled and the next ask drains what arrived
    /// late before it is sent; a late frame landing after that drain is
    /// dropped by its number at a snapshot ask and by its kind at every
    /// other. **The answer leg waits `SNAPSHOT_ANSWER_BOUND_MS`**, the
    /// image being written before it, and the finished leg the small
    /// asks' bound. A write that does not send is the dead peer as every
    /// send failure is.
    pub(crate) fn ask_snapshot(&mut self) -> Result<SavePointTaken, weaver_types::SavePointLeg> {
        use weaver_types::SavePointLeg;
        if self.dead {
            return Err(SavePointLeg::MemberDead);
        }
        let ordinal = self.snapshot_ordinal;
        self.snapshot_ordinal += 1;
        let ask = format!(
            "{}\n",
            serde_json::json!({"ask": {"snapshot": {"ask": ordinal}}})
        );
        if !self.send(ask.as_bytes()) {
            self.dead = true;
            return Err(SavePointLeg::MemberDead);
        }
        // **An answer is read by its number**: one carrying another ordinal
        // is a late answer to an ask before, dropped and said, and the wait
        // goes on inside the one bound; a line that is no snapshot answer at
        // all misses the leg as before. The leg is the image's write, so it
        // has its own bound.
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(self.snapshot_answer_bound_ms);
        let Some(answered) = self.await_numbered(deadline, ordinal, parse_snapshot_answer) else {
            self.unsettled = true;
            return Err(SavePointLeg::Answer);
        };
        let acknowledge = format!(
            "{}\n",
            serde_json::json!({"acknowledge": {"snapshot": {"ask": ordinal, "digest": answered.stamp.digest}}})
        );
        if !self.send(acknowledge.as_bytes()) {
            self.dead = true;
            return Err(SavePointLeg::MemberDead);
        }
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(ANSWER_BOUND_MS);
        let Some(finished) = self.await_numbered(deadline, ordinal, parse_finished_answer) else {
            self.unsettled = true;
            return Err(SavePointLeg::Finished);
        };
        if finished != answered.name {
            self.unsettled = true;
            return Err(SavePointLeg::Finished);
        }
        Ok(answered)
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
        self.await_answer(bound_ms, parse_replay_answer)
    }

    fn recall_exchange(&mut self, last_turns: Option<u64>, bound_ms: u64) -> Option<Vec<Recalled>> {
        let ask = match last_turns {
            Some(bound) => format!("{{\"ask\":{{\"recall\":{{\"last-turns\":{bound}}}}}}}\n"),
            None => "{\"ask\":{\"recall\":{}}}\n".to_string(),
        };
        if !self.send(ask.as_bytes()) {
            return None;
        }
        self.await_answer(bound_ms, parse_recall_answer)
    }

    /// One frame whole or nothing, the tee's own economics: the channel is
    /// nonblocking - the flag rides the shared open file description the
    /// tee set - and a peer that cannot take a frame now is not waited on.
    /// **Settle the seam before an ask**, per `weaver-harness-Spec` section
    /// 6: after a missed snapshot leg, whatever the member sent late is
    /// read without blocking and discarded, the residual with it, and one
    /// line says how much, so the ask that follows reads its own answer.
    fn settle(&mut self) {
        if !self.unsettled {
            return;
        }
        self.unsettled = false;
        let mut discarded = std::mem::take(&mut self.residual);
        let mut chunk = [0u8; 65536];
        loop {
            match self.channel.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => discarded.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        if !discarded.is_empty() {
            let lines = discarded.iter().filter(|&&b| b == b'\n').count();
            let partial = usize::from(discarded.last().is_some_and(|&b| b != b'\n'));
            // A part of a line drained leaves its remainder owed to no ask.
            self.discarding = partial == 1;
            eprintln!(
                "weaver-harness: the state seam drained {} bytes the member sent late after a missed save-point leg, {lines} whole lines and {partial} partial, owed to no ask",
                discarded.len()
            );
        }
    }

    fn send(&mut self, mut bytes: &[u8]) -> bool {
        self.settle();
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

    /// **Await a non-snapshot ask's answer inside the bound**, per
    /// `weaver-harness-Spec` section 6: a snapshot-protocol frame is never
    /// another ask's answer, so one landing late after a missed save-point
    /// leg, past the drain, is dropped and said and the wait goes on inside
    /// the same deadline; any other line is the answer or the ask missed.
    /// The channel shares the tee's nonblocking flag, so the wait is a poll
    /// deadline rather than a read timeout.
    fn await_answer<T>(&mut self, bound_ms: u64, parse: fn(&str) -> Option<T>) -> Option<T> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(bound_ms);
        loop {
            let line = self.await_line_until(deadline)?;
            let Some(carried) = protocol_ordinal(&line) else {
                return parse(&line);
            };
            eprintln!(
                "weaver-harness: the state seam dropped a snapshot-protocol answer carrying ask {carried} while waiting for another ask's answer, a late answer to a missed save-point leg"
            );
        }
    }

    /// **Await the answer numbered `ordinal`** inside the deadline: a line
    /// that is a numbered answer of the snapshot protocol, of either kind,
    /// carrying another ordinal is dropped and said, the wait going on
    /// (Codex on #94, rounds 10 and 11: a late `finished` frame of the ask
    /// before, landing after the drain while the retry's answer is awaited,
    /// is dropped by its number like a late answer); a line that is the
    /// kind awaited with this ordinal is the answer; a line that is no
    /// numbered answer of the protocol at all is the leg missed, as a
    /// malformed answer always was.
    fn await_numbered<T>(
        &mut self,
        deadline: std::time::Instant,
        ordinal: u64,
        parse: fn(&str) -> Option<(u64, T)>,
    ) -> Option<T> {
        loop {
            let line = self.await_line_until(deadline)?;
            if let Some((carried, answer)) = parse(&line)
                && carried == ordinal
            {
                return Some(answer);
            }
            let carried = protocol_ordinal(&line)?;
            if carried == ordinal {
                // The right number on the other kind: the member answered
                // out of order for this very exchange, which is the leg
                // missed, never a frame to wait past.
                return None;
            }
            eprintln!(
                "weaver-harness: the state seam dropped a snapshot-protocol answer carrying ask {carried} while waiting for ask {ordinal}, a late answer to an ask before"
            );
        }
    }

    fn await_line_until(&mut self, deadline: std::time::Instant) -> Option<String> {
        let mut buffer: Vec<u8> = std::mem::take(&mut self.residual);
        loop {
            if let Some(position) = buffer.iter().position(|&b| b == b'\n') {
                if std::mem::take(&mut self.discarding)
                    && serde_json::from_slice::<serde_json::Value>(&buffer[..position]).is_err()
                {
                    // The remainder of a late frame the drain cut (#140).
                    eprintln!(
                        "weaver-harness: the state seam discarded the {position} bytes ending a line the member sent late after a missed save-point leg, owed to no ask"
                    );
                    buffer.drain(..=position);
                    continue;
                }
                let line = String::from_utf8_lossy(&buffer[..position]).into_owned();
                self.residual = buffer[position + 1..].to_vec();
                return Some(line);
            }
            if buffer.len() > ANSWER_BOUND_BYTES {
                return None;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                // **What arrived of a line is kept** (#140): a part of a late
                // frame read inside a missed wait reaches the drain, which
                // then knows the rest is owed to no ask.
                self.residual = buffer;
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

/// The five stamp members off a `snapshot` answer, whole or
/// nothing: `save-point`, `run`, `sequence`, `turn`, `digest`.
fn parse_stamped(body: &serde_json::Value) -> Option<SavePointTaken> {
    Some(SavePointTaken {
        name: body.get("save-point")?.as_str()?.to_string(),
        stamp: SavePointStamp {
            digest: body.get("digest")?.as_str()?.to_string(),
            run: body.get("run")?.as_str()?.to_string(),
            sequence: body.get("sequence")?.as_u64()?,
            turn: body.get("turn")?.as_u64()?,
        },
    })
}

/// Parse the snapshot answer, per the contract:
/// `{"answer":{"snapshot":{"ask":N,"save-point":..,"run":..,"sequence":..,"turn":..,"digest":..}}}`,
/// answering the ordinal it carries beside the stamp.
fn parse_snapshot_answer(line: &str) -> Option<(u64, SavePointTaken)> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let body = value.get("answer")?.get("snapshot")?;
    Some((body.get("ask")?.as_u64()?, parse_stamped(body)?))
}
/// The ordinal any snapshot-protocol answer carries, the `snapshot` answer's
/// or the `finished` answer's, or none where the line is neither.
fn protocol_ordinal(line: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let answer = value.get("answer")?;
    answer
        .get("snapshot")
        .or_else(|| answer.get("finished"))?
        .get("ask")?
        .as_u64()
}
/// Parse the finished answer, `{"answer":{"finished":{"ask":N,"save-point":..}}}`,
/// answering the ordinal it carries beside the name.
fn parse_finished_answer(line: &str) -> Option<(u64, String)> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let body = value.get("answer")?.get("finished")?;
    Some((
        body.get("ask")?.as_u64()?,
        body.get("save-point")?.as_str()?.to_string(),
    ))
}

/// Parse the restored answer: `{"answer":{"restored":{"lineage":{...}}}}`
/// with `digest`, `run`, `sequence` and `turn`, `{"answer":{"restored":{}}}`
/// where the member stood empty, or `{"answer":{"restored":{"refused":..}}}`.
/// A body that is none of the three is malformed and answers nothing.
fn parse_restored_answer(line: &str) -> Option<RestoredAnswer> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let body = value.get("answer")?.get("restored")?.as_object()?;
    if let Some(reason) = body.get("refused") {
        return Some(RestoredAnswer::Refused(reason.as_str()?.to_string()));
    }
    if let Some(lineage) = body.get("lineage") {
        return Some(RestoredAnswer::Lineage(SavePointStamp {
            digest: lineage.get("digest")?.as_str()?.to_string(),
            run: lineage.get("run")?.as_str()?.to_string(),
            sequence: lineage.get("sequence")?.as_u64()?,
            turn: lineage.get("turn")?.as_u64()?,
        }));
    }
    if body.is_empty() {
        return Some(RestoredAnswer::Empty);
    }
    None
}

/// **The open's identity material, one source and one rule**, per
/// `weaver-harness-Spec` section 6.1 on the operator's ruling of 2026-10-06
/// that the system prompt is state. The member's answer is the open's
/// messages, each rebuilt from the pairs the tee carried whole, `role` and
/// `content`; an empty answer is an agent not yet seeded and opens with no
/// prefix; and a prefix that does not rebuild refuses the same way a miss
/// does, because a half-read bounding is no bounding. The miss itself is
/// the caller's to refuse, before this is reached.
pub(crate) fn identity_material(held: &[Recalled]) -> Option<Vec<weaver_traits::Message>> {
    held.iter().map(prefix_message).collect()
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
mod tests;
