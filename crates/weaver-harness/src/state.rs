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
/// to write a 1 GiB image. The ask leg and the finished leg, and every
/// other ask, keep `ANSWER_BOUND_MS`.
pub(crate) const SNAPSHOT_ANSWER_BOUND_MS: u64 = 120_000;

/// **The bound on the `restored` answer**, per `weaver-harness-Spec` section
/// 6.1 on the operator's ruling of 2026-10-08 on #99 (N1): the member
/// restores the save point admin handed only after the opener reaches it,
/// judging and seating an image of up to the save point's 1 GiB bound, and
/// answers `restored` after that, so the ask is waited on as the snapshot's
/// answer leg is, 120 seconds, and never on the small asks' two.
pub(crate) const RESTORED_ANSWER_BOUND_MS: u64 = 120_000;

/// The bound on an ask that parks at the member until the driver seals,
/// per `weaver-harness-state-contract` section 2: the enter's identity and
/// recall asks under a diagnostic binding or a restoring load wait on a
/// preload the operator runs beside the load, so the bound is the replay
/// ask's generous one and not the two seconds a member answering from
/// holdings at rest takes.
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

/// The `restore` answer: what the member restored, named and stamped, and
/// the prefix the restored holdings carry, served as the identity ask serves
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct SavePointRestored {
    pub taken: SavePointTaken,
    pub identity: Vec<Recalled>,
}

/// The `restored` answer, per the contract's eighth ask: the stamp of the
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
            snapshot_ordinal: 1,
            residual: Vec::new(),
            snapshot_answer_bound_ms: SNAPSHOT_ANSWER_BOUND_MS,
            restored_answer_bound_ms: RESTORED_ANSWER_BOUND_MS,
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
        self.await_answer(ANSWER_BOUND_MS, parse_shape_answer)
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

    pub(crate) fn ask_recall(&mut self, last_turns: Option<u64>) -> Option<Vec<Recalled>> {
        self.ask_recall_within(last_turns, ANSWER_BOUND_MS)
    }

    /// The `restored` ask, per the contract's eighth ask of 2026-10-02: what
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

    /// The `restore` ask, per the contract's seventh ask of 2026-10-02: the
    /// member replaces its holdings from a save point in its own room, named
    /// here, and answers the stamp and the restored prefix. **The flush and
    /// the reopen that follow are the loop's**, per `weaver-harness-Spec`
    /// section 6, and the channel that carries the operator's demand to the
    /// loop is the loop act's to name, so nothing in this crate sends this
    /// ask yet: the seam speaks the contract's vocabulary whole, and the
    /// caller arrives with that act.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn ask_restore(&mut self, save_point: &str) -> Option<SavePointRestored> {
        if self.dead {
            return None;
        }
        let answered = self.restore_exchange(save_point);
        if answered.is_none() {
            self.dead = true;
        }
        answered
    }

    fn restore_exchange(&mut self, save_point: &str) -> Option<SavePointRestored> {
        let ask = serde_json::json!({"ask": {"restore": {"save-point": save_point}}}).to_string();
        if !self.send(format!("{ask}\n").as_bytes()) {
            return None;
        }
        self.await_answer(ANSWER_BOUND_MS, parse_restore_answer)
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
                let line = String::from_utf8_lossy(&buffer[..position]).into_owned();
                self.residual = buffer[position + 1..].to_vec();
                return Some(line);
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

/// The five stamp members off a `snapshot` or `restore` answer, whole or
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

/// Parse the restore answer: the snapshot's members and `identity`.
fn parse_restore_answer(line: &str) -> Option<SavePointRestored> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let body = value.get("answer")?.get("restore")?;
    let taken = parse_stamped(body)?;
    let identity = parse_recalled_events(body.get("identity")?.as_array()?)?;
    Some(SavePointRestored { taken, identity })
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
mod tests {
    use super::*;

    /// **The answer bound is the contract's one number**, per
    /// `weaver-harness-state-contract` section 3: one mebibyte, which the
    /// member's `ANSWER_BOUND` in `weaver-state` holds as the same number
    /// under its own watch, the two crates seeing neither's constant. A
    /// change to either without the other is the defect this pins.
    #[test]
    fn the_answer_bound_is_the_contracts_mebibyte() {
        assert_eq!(ANSWER_BOUND_BYTES, 1024 * 1024);
        // One entry's cost is never under what the member renders for it:
        // the envelope's fixed text with the kind and a twenty-digit
        // sequence fits the allowance beside the names.
        let envelope = r#"{"envelope":{"session":"","run":"","kind":"message.system","sequence":"12345678901234567890"},"pairs":},"#;
        assert!(envelope.len() <= ENVELOPE_BYTES, "{}", envelope.len());
        // The frame's own text and its delimiter fit the allowance.
        assert!(r#"{"answer":{"identity":{"messages":[]}}}"#.len() < ANSWER_FRAME_BYTES);
        assert_eq!(
            identity_entry_cost(10, "s-1", "r-1"),
            10 + 3 + 3 + ENVELOPE_BYTES
        );
        // **The names cost what the member writes, escapes included**: a
        // quote and a backslash each render as two bytes, so a name of one
        // costs one more than its length (Codex on #92, round 6).
        // Perturbation: count `name.len()` again and this fails by two.
        assert_eq!(
            identity_entry_cost(10, "s\"1", "r\\1"),
            10 + 4 + 4 + ENVELOPE_BYTES
        );
        assert_eq!(rendered_name("s\"\\\n"), 7, "three escapes, two bytes each");
    }

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

    /// **The three save-point asks cross and their answers parse whole or
    /// not at all**, per `weaver-harness-state-contract` section 2 as of
    /// 2026-10-02: `restored` answers a stamp, nothing, or a refusal, and a
    /// body that is none of the three is malformed; `snapshot` answers the
    /// five stamp members; `restore` answers them and the restored prefix.
    /// Each ask is spelled as the contract spells it.
    ///
    /// Perturbation: drop `turn` from `parse_stamped` and the snapshot
    /// assertion on the turn fails; accept a restored body with an unknown
    /// member as `Empty` and the malformed case parses.
    #[test]
    fn the_save_point_asks_cross_and_parse() {
        let exchange = |answer: &str, ask: &dyn Fn(&mut StateSeam) -> Option<String>| {
            let (ours, theirs) = UnixStream::pair().expect("pair");
            ours.set_nonblocking(true).expect("nonblocking");
            let mut seam = StateSeam::new(ours);
            let mut peer = theirs;
            peer.write_all(format!("{answer}\n").as_bytes())
                .expect("answers in advance");
            let parsed = ask(&mut seam);
            let mut asked = [0u8; 256];
            let n = peer.read(&mut asked).expect("reads the ask");
            (parsed, String::from_utf8_lossy(&asked[..n]).into_owned())
        };
        let (parsed, asked) = exchange(
            r#"{"answer":{"restored":{"lineage":{"digest":"ab","run":"r-1","sequence":41,"turn":2}}}}"#,
            &|seam| seam.ask_restored().map(|a| format!("{a:?}")),
        );
        assert_eq!(asked, "{\"ask\":{\"restored\":{}}}\n");
        assert_eq!(
            parsed.as_deref(),
            Some(
                format!(
                    "{:?}",
                    RestoredAnswer::Lineage(SavePointStamp {
                        digest: "ab".into(),
                        run: "r-1".into(),
                        sequence: 41,
                        turn: 2
                    })
                )
                .as_str()
            )
        );
        for (answer, expected) in [
            (r#"{"answer":{"restored":{}}}"#, Some(RestoredAnswer::Empty)),
            (
                r#"{"answer":{"restored":{"refused":"schema-mismatch"}}}"#,
                Some(RestoredAnswer::Refused("schema-mismatch".into())),
            ),
            (r#"{"answer":{"restored":{"other":1}}}"#, None),
            (
                r#"{"answer":{"restored":{"lineage":{"digest":"ab"}}}}"#,
                None,
            ),
            (r#"{"answer":{"shape":{"runs":[]}}}"#, None),
        ] {
            assert_eq!(parse_restored_answer(answer), expected, "{answer}");
        }
        // The four legs: both answers stand in advance, and what was asked
        // is the snapshot ask followed by the acknowledgement of the digest.
        let (parsed, asked) = exchange(
            concat!(
                r#"{"answer":{"snapshot":{"ask":1,"save-point":"ab.save-point","run":"r-1","sequence":41,"turn":2,"digest":"ab"}}}"#,
                "\n",
                r#"{"answer":{"finished":{"ask":1,"save-point":"ab.save-point"}}}"#
            ),
            &|seam| seam.ask_snapshot().ok().map(|a| format!("{a:?}")),
        );
        assert_eq!(
            asked,
            concat!(
                "{\"ask\":{\"snapshot\":{\"ask\":1}}}\n",
                "{\"acknowledge\":{\"snapshot\":{\"ask\":1,\"digest\":\"ab\"}}}\n"
            )
        );
        // A finished answer naming another file, or none, is the finished
        // leg missed. Perturbation: skip the name's comparison and the first
        // case parses.
        let (parsed_other, _) = exchange(
            concat!(
                r#"{"answer":{"snapshot":{"ask":1,"save-point":"ab.save-point","run":"r-1","sequence":41,"turn":2,"digest":"ab"}}}"#,
                "\n",
                r#"{"answer":{"finished":{"ask":1,"save-point":"other.save-point"}}}"#
            ),
            &|seam| seam.ask_snapshot().err().map(|leg| format!("{leg:?}")),
        );
        assert_eq!(parsed_other.as_deref(), Some("Finished"));
        let (parsed_none, _) = exchange(
            r#"{"answer":{"snapshot":{"ask":1,"save-point":"ab.save-point","run":"r-1","sequence":41,"turn":2,"digest":"ab"}}}"#,
            &|seam| seam.ask_snapshot().err().map(|leg| format!("{leg:?}")),
        );
        assert_eq!(parsed_none.as_deref(), Some("Finished"));
        let taken = SavePointTaken {
            name: "ab.save-point".into(),
            stamp: SavePointStamp {
                digest: "ab".into(),
                run: "r-1".into(),
                sequence: 41,
                turn: 2,
            },
        };
        assert_eq!(parsed, Some(format!("{taken:?}")));
        assert!(
            parse_snapshot_answer(
                r#"{"answer":{"snapshot":{"ask":1,"save-point":"x","run":"r","sequence":1,"digest":"d"}}}"#
            )
            .is_none(),
            "a stamp without its turn is malformed"
        );
        assert!(
            parse_snapshot_answer(
                r#"{"answer":{"snapshot":{"save-point":"x","run":"r","sequence":1,"turn":1,"digest":"d"}}}"#
            )
            .is_none(),
            "an answer without its ordinal is malformed (Codex on #94, round 10)"
        );
        let (parsed, asked) = exchange(
            concat!(
                r#"{"answer":{"restore":{"save-point":"ab.save-point","run":"r-1","sequence":41,"turn":2,"digest":"ab","#,
                r#""identity":[{"envelope":{"session":"s","run":"r-1","kind":"message.system","sequence":"3"},"#,
                r#""pairs":{"role":"system","content":[]}}]}}}"#
            ),
            &|seam| seam.ask_restore("ab.save-point").map(|a| format!("{a:?}")),
        );
        assert_eq!(
            asked,
            "{\"ask\":{\"restore\":{\"save-point\":\"ab.save-point\"}}}\n"
        );
        let parsed = parsed.expect("parses");
        assert!(
            parsed.contains("ab.save-point") && parsed.contains("message.system"),
            "{parsed}"
        );
    }

    /// **The identity material's one source**, per `weaver-harness-Spec`
    /// section 6.1 on the ruling of 2026-10-06: an empty answer is an agent
    /// not yet seeded and opens with no prefix, a prefix rebuilds from its
    /// pairs, and a half-read one refuses. Perturbation: skip the rebuild's
    /// `content` and the rebuilt prefix carries no text.
    #[test]
    fn the_identity_material_has_one_source_and_one_rule() {
        assert_eq!(
            identity_material(&[]),
            Some(Vec::new()),
            "an empty answer is an unseeded agent, which opens with no prefix"
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
        let material = identity_material(&held).expect("rebuilds");
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
            identity_material(&half).is_none(),
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

    /// **A missed snapshot leg leaves the seam alive and the next ask drains
    /// what arrived late**, per `weaver-harness-Spec` section 6 (Codex on
    /// #94, round 4): the member answering past the bound costs the answer
    /// leg, the late answer and the part of a line behind it are drained at
    /// the retry, and the retry reads its own answer. Perturbation: keep the
    /// dead flag on the missed answer and the retry answers `MemberDead`;
    /// skip the drain and the retry acknowledges the late digest and misses
    /// its finished leg.
    #[test]
    fn a_missed_snapshot_leg_is_retried_over_a_drained_seam() {
        use std::io::BufRead;
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        seam.snapshot_answer_bound_ms = ANSWER_BOUND_MS;
        let (to_peer, at_peer) = std::sync::mpsc::channel::<()>();
        let (to_us, at_us) = std::sync::mpsc::channel::<()>();
        let peer = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
            let mut writer = theirs;
            let mut asked = Vec::new();
            let mut line = String::new();
            reader.read_line(&mut line).expect("the first ask");
            asked.push(line.clone());
            at_peer.recv().expect("told the bound expired");
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"snapshot":{"ask":1,"save-point":"late.save-point","run":"r-1","sequence":1,"turn":1,"digest":"late"}}}"#,
                        "\n",
                        "{\"part"
                    )
                    .as_bytes(),
                )
                .expect("the late answer and a part of a line");
            to_us.send(()).expect("says so");
            line.clear();
            reader.read_line(&mut line).expect("the retry");
            asked.push(line.clone());
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"snapshot":{"ask":2,"save-point":"fresh.save-point","run":"r-1","sequence":1,"turn":1,"digest":"fresh"}}}"#,
                        "\n"
                    )
                    .as_bytes(),
                )
                .expect("answers the retry");
            line.clear();
            reader.read_line(&mut line).expect("the acknowledgement");
            asked.push(line.clone());
            writer
                .write_all(
                    b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"fresh.save-point\"}}}\n",
                )
                .expect("finishes");
            asked
        });
        assert!(matches!(
            seam.ask_snapshot(),
            Err(weaver_types::SavePointLeg::Answer)
        ));
        to_peer.send(()).unwrap();
        at_us.recv().unwrap();
        let taken = seam
            .ask_snapshot()
            .expect("the retry is read as its own over the drained seam");
        assert_eq!(taken.stamp.digest, "fresh");
        let asked = peer.join().unwrap();
        assert_eq!(
            asked,
            vec![
                "{\"ask\":{\"snapshot\":{\"ask\":1}}}\n".to_string(),
                "{\"ask\":{\"snapshot\":{\"ask\":2}}}\n".to_string(),
                "{\"acknowledge\":{\"snapshot\":{\"ask\":2,\"digest\":\"fresh\"}}}\n".to_string(),
            ]
        );
    }

    /// **A late answer is dropped by its number, not by timing**, per
    /// `weaver-harness-Spec` section 6 (Codex on #94, round 10): the member
    /// answers the first ask only after the retry's ask has arrived, past
    /// the drain, then answers the retry; the harness reads past the answer
    /// carrying ask 1 to the one carrying ask 2 and acknowledges that one,
    /// so a slow member never leaves the retry one answer behind.
    /// Perturbation: ignore the ordinal and the stale answer is taken as the
    /// retry's, the acknowledgement names "late" and the finished leg misses.
    #[test]
    fn a_late_answer_is_dropped_by_its_number_and_the_retrys_is_read() {
        use std::io::BufRead;
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        seam.snapshot_answer_bound_ms = ANSWER_BOUND_MS;
        let peer = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
            let mut writer = theirs;
            let mut asked = Vec::new();
            let mut line = String::new();
            reader.read_line(&mut line).expect("the first ask");
            asked.push(line.clone());
            // Silent past the bound: the first ask misses.
            line.clear();
            reader.read_line(&mut line).expect("the retry");
            asked.push(line.clone());
            // The late answer to ask 1 lands after the retry's ask, past any
            // drain, then the retry's own.
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"snapshot":{"ask":1,"save-point":"late.save-point","run":"r-1","sequence":1,"turn":1,"digest":"late"}}}"#,
                        "\n",
                        r#"{"answer":{"snapshot":{"ask":2,"save-point":"fresh.save-point","run":"r-1","sequence":1,"turn":1,"digest":"fresh"}}}"#,
                        "\n"
                    )
                    .as_bytes(),
                )
                .expect("answers late and then the retry");
            line.clear();
            reader.read_line(&mut line).expect("the acknowledgement");
            asked.push(line.clone());
            writer
                .write_all(
                    b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"fresh.save-point\"}}}\n",
                )
                .expect("finishes the retry's");
            asked
        });
        assert!(matches!(
            seam.ask_snapshot(),
            Err(weaver_types::SavePointLeg::Answer)
        ));
        let taken = seam
            .ask_snapshot()
            .expect("the retry reads past the late answer to its own");
        assert_eq!(taken.stamp.digest, "fresh");
        let asked = peer.join().unwrap();
        assert_eq!(
            asked[2],
            "{\"acknowledge\":{\"snapshot\":{\"ask\":2,\"digest\":\"fresh\"}}}\n"
        );
    }

    /// **A late `finished` frame of the ask before is dropped by its number
    /// while the retry's answer is awaited**, per `weaver-harness-Spec`
    /// section 6 (Codex on #94, round 11): the member finishes ask 1 only
    /// after the retry's ask arrived, past the drain, then answers ask 2;
    /// the harness drops the finished frame carrying ask 1 and reads ask
    /// 2's answer. Perturbation: judge the awaited kind alone again and the
    /// retry misses its answer leg on the finished frame.
    #[test]
    fn a_late_finished_frame_is_dropped_by_its_number_while_the_retry_is_awaited() {
        use std::io::BufRead;
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let peer = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
            let mut writer = theirs;
            let mut line = String::new();
            reader.read_line(&mut line).expect("the first ask");
            // Ask 1 is answered at once; its finished frame comes only after
            // the retry's ask.
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"snapshot":{"ask":1,"save-point":"one.save-point","run":"r-1","sequence":1,"turn":1,"digest":"one"}}}"#,
                        "\n"
                    )
                    .as_bytes(),
                )
                .expect("answers ask 1");
            line.clear();
            reader
                .read_line(&mut line)
                .expect("the acknowledgement of ask 1");
            // Silent past the bound: the finished leg misses.
            line.clear();
            reader.read_line(&mut line).expect("the retry's ask");
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"finished":{"ask":1,"save-point":"one.save-point"}}}"#,
                        "\n",
                        r#"{"answer":{"snapshot":{"ask":2,"save-point":"two.save-point","run":"r-1","sequence":1,"turn":1,"digest":"two"}}}"#,
                        "\n"
                    )
                    .as_bytes(),
                )
                .expect("finishes ask 1 late, then answers ask 2");
            line.clear();
            reader
                .read_line(&mut line)
                .expect("the acknowledgement of ask 2");
            writer
                .write_all(
                    b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"two.save-point\"}}}\n",
                )
                .expect("finishes ask 2");
            line
        });
        assert!(matches!(
            seam.ask_snapshot(),
            Err(weaver_types::SavePointLeg::Finished)
        ));
        let taken = seam
            .ask_snapshot()
            .expect("the retry reads past the late finished frame to its own answer");
        assert_eq!(taken.stamp.digest, "two");
        assert_eq!(
            peer.join().unwrap(),
            "{\"acknowledge\":{\"snapshot\":{\"ask\":2,\"digest\":\"two\"}}}\n"
        );
    }

    /// **A late snapshot-protocol frame is never another ask's answer**,
    /// per `weaver-harness-Spec` section 6: the answer leg of snapshot ask
    /// 1 misses, the seam settles, and only after the recall ask has
    /// arrived does the member send the late answer for ask 1 and then the
    /// recall's; the recall reads past the late frame to its own answer
    /// inside the same bound and the seam stays alive for the snapshot ask
    /// that follows. Perturbation: take the first line as the answer again
    /// and the recall misses on the late frame, retiring the seam.
    #[test]
    fn a_late_snapshot_frame_is_dropped_by_a_later_ask_of_another_kind() {
        use std::io::BufRead;
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        seam.snapshot_answer_bound_ms = ANSWER_BOUND_MS;
        let peer = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
            let mut writer = theirs;
            let mut asked = Vec::new();
            let mut line = String::new();
            reader.read_line(&mut line).expect("the snapshot ask");
            asked.push(line.clone());
            // Silent past the bound: the answer leg misses.
            line.clear();
            reader.read_line(&mut line).expect("the recall ask");
            asked.push(line.clone());
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"snapshot":{"ask":1,"save-point":"late.save-point","run":"r-1","sequence":1,"turn":1,"digest":"late"}}}"#,
                        "\n",
                        r#"{"answer":{"recall":{"events":[]}}}"#,
                        "\n"
                    )
                    .as_bytes(),
                )
                .expect("answers ask 1 late, then the recall");
            line.clear();
            reader
                .read_line(&mut line)
                .expect("the second snapshot ask");
            asked.push(line.clone());
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"snapshot":{"ask":2,"save-point":"two.save-point","run":"r-1","sequence":1,"turn":1,"digest":"two"}}}"#,
                        "\n"
                    )
                    .as_bytes(),
                )
                .expect("answers ask 2");
            line.clear();
            reader.read_line(&mut line).expect("the acknowledgement");
            writer
                .write_all(
                    b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"two.save-point\"}}}\n",
                )
                .expect("finishes ask 2");
            asked
        });
        assert!(matches!(
            seam.ask_snapshot(),
            Err(weaver_types::SavePointLeg::Answer)
        ));
        let recalled = seam
            .ask_recall(None)
            .expect("the recall reads past the late snapshot frame to its own answer");
        assert!(recalled.is_empty());
        let taken = seam
            .ask_snapshot()
            .expect("the seam is alive for the snapshot ask that follows");
        assert_eq!(taken.stamp.digest, "two");
        let asked = peer.join().unwrap();
        assert_eq!(asked[1], "{\"ask\":{\"recall\":{}}}\n");
    }

    /// **The `restored` ask has its own bound** (the operator's ruling of
    /// 2026-10-08 on #99, N1): a member that answers `restored` two and a
    /// half seconds after the ask, past the small asks' two, as a member
    /// still seating a large image does, is read as answered. Perturbation:
    /// wait it on `ANSWER_BOUND_MS` again and the ask answers nothing.
    #[test]
    fn a_restored_answer_past_the_small_bound_is_read() {
        use std::io::BufRead;
        assert_eq!(RESTORED_ANSWER_BOUND_MS, 120_000);
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let peer = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
            let mut writer = theirs;
            let mut line = String::new();
            reader.read_line(&mut line).expect("the restored ask");
            std::thread::sleep(std::time::Duration::from_millis(ANSWER_BOUND_MS + 500));
            writer
                .write_all(b"{\"answer\":{\"restored\":{}}}\n")
                .expect("answers past the small bound");
        });
        assert_eq!(seam.ask_restored(), Some(RestoredAnswer::Empty));
        peer.join().unwrap();
    }

    /// **The snapshot's answer leg has its own bound**, per
    /// `weaver-harness-Spec` section 6 on the operator's ruling of
    /// 2026-10-08: a member that answers the snapshot two and a half seconds
    /// after the ask, past the small asks' two, is read as taken, the image
    /// being written before the answer. Perturbation: wait the answer leg
    /// on `ANSWER_BOUND_MS` again and the leg misses as `Answer`.
    #[test]
    fn a_snapshot_answered_past_the_small_bound_is_taken() {
        use std::io::BufRead;
        assert_eq!(SNAPSHOT_ANSWER_BOUND_MS, 120_000);
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let peer = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
            let mut writer = theirs;
            let mut line = String::new();
            reader.read_line(&mut line).expect("the snapshot ask");
            std::thread::sleep(std::time::Duration::from_millis(ANSWER_BOUND_MS + 500));
            writer
                .write_all(
                    concat!(
                        r#"{"answer":{"snapshot":{"ask":1,"save-point":"slow.save-point","run":"r-1","sequence":1,"turn":1,"digest":"slow"}}}"#,
                        "\n"
                    )
                    .as_bytes(),
                )
                .expect("answers past the small bound");
            line.clear();
            reader.read_line(&mut line).expect("the acknowledgement");
            writer
                .write_all(
                    b"{\"answer\":{\"finished\":{\"ask\":1,\"save-point\":\"slow.save-point\"}}}\n",
                )
                .expect("finishes");
        });
        let taken = seam
            .ask_snapshot()
            .expect("the slow answer is inside the answer leg's own bound");
        assert_eq!(taken.stamp.digest, "slow");
        peer.join().unwrap();
    }
}
