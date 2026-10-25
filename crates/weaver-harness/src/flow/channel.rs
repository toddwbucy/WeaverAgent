//! Sole nonblocking owner of state-flow/1, including its observation barrier.
//!
//! The coordinator elects waits and authors measurements. This owner only carries
//! exact frames, judges correlated answers and holds them until that authoring ends.

use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::os::unix::net::UnixStream;

use nix::sys::socket::{MsgFlags, recv, send};
use nix::{libc, poll::PollFlags};
use weaver_types::state_flow::{
    Common, DecimalU64, FrameDirection, MemberObservationIdentity, PairFile, ReadLimits,
    STATE_ANSWER_LINE_BOUND, SourceBlockId, StartIdentity, StateControl, StateControlAnswer,
    StateFrame, StateOperation, StateStoreMeasurement, StatementFile, decode_state_line,
    encode_state_line,
};

use super::RecordedStateMeasurement;

/// Bodies only. The channel, not its callers, assigns exchange identities.
#[derive(Debug)]
pub(super) enum StateRequest {
    Open {
        schema_id: String,
        scheme_id: String,
        start: StartIdentity,
        read_limits: ReadLimits,
    },
    File(StatementFile),
    FilePair(PairFile),
    Control(StateControl),
}

impl StateRequest {
    fn frame(self, common: Common) -> StateFrame {
        match self {
            Self::Open {
                schema_id,
                scheme_id,
                start,
                read_limits,
            } => StateFrame::Open {
                common,
                schema_id,
                scheme_id,
                start,
                read_limits,
            },
            Self::File(filing) => StateFrame::File { common, filing },
            Self::FilePair(filing) => StateFrame::FilePair { common, filing },
            Self::Control(frame) => StateFrame::Control { common, frame },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChannelFailureKind {
    Carry,
    Protocol,
    FilingRefused,
    OpenRefused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChannelPoint {
    Send,
    Reply,
    Counter,
}

/// Evidence only. Even Committed cannot release a decision before recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitEvidence {
    NotRequested,
    NotCommitted,
    Unknown,
    Committed,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExchangeProgress {
    pub common: Common,
    pub operation: StateOperation,
    pub source: Option<SourceBlockId>,
    pub bytes_sent: usize,
    pub frame_bytes: usize,
    pub commit: CommitEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChannelFailure {
    pub kind: ChannelFailureKind,
    pub point: ChannelPoint,
    pub detail: String,
    pub outstanding: Option<Box<ExchangeProgress>>,
    pub received: Option<Common>,
}

impl std::fmt::Display for ChannelFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} at {:?}: {}", self.kind, self.point, self.detail)
    }
}

impl std::error::Error for ChannelFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StateInterest {
    pub readable: bool,
    pub writable: bool,
    live: bool,
}

impl StateInterest {
    /// A full answer slot still observes write-half closure, without requesting
    /// POLLIN for another answer that this owner cannot receive yet.
    pub(super) fn events(self) -> PollFlags {
        if !self.live {
            return PollFlags::empty();
        }
        let mut events = PollFlags::from_bits_retain(libc::POLLRDHUP);
        if self.readable {
            events |= PollFlags::POLLIN;
        }
        if self.writable {
            events |= PollFlags::POLLOUT;
        }
        events
    }
}

/// Only a consumed RecordedStateMeasurement can produce this value.
#[derive(Debug)]
pub(super) struct StateCompletion {
    pub answer: StateFrame,
    /// A timeout cleanup was elected before the original answer was recorded.
    pub discard_capture_result: bool,
}

/// Evidence of an Open that was never released for boot. An enter rollback
/// consumes it without inventing a serving measurement proof or a read grant.
#[derive(Debug)]
pub(super) struct UnusedOpen {
    answer: StateFrame,
    measurement: StateStoreMeasurement,
}

impl UnusedOpen {
    pub(super) fn answer(&self) -> &StateFrame {
        &self.answer
    }
    pub(super) fn measurement(&self) -> &StateStoreMeasurement {
        &self.measurement
    }
}

struct Exchange {
    request: StateFrame,
    bytes: Vec<u8>,
    offset: usize,
    queued_ns: u64,
    sent_ns: Option<u64>,
    commit: CommitEvidence,
}

impl Exchange {
    fn source(&self) -> Option<SourceBlockId> {
        match &self.request {
            StateFrame::File { filing, .. } => Some(filing.block_id.clone()),
            StateFrame::FilePair { filing, .. } => Some(filing.block_id.clone()),
            _ => None,
        }
    }

    fn progress(&self) -> ExchangeProgress {
        ExchangeProgress {
            common: self.request.common().clone(),
            operation: self.request.request_operation().expect("request body"),
            source: self.source(),
            bytes_sent: self.offset,
            frame_bytes: self.bytes.len(),
            commit: self.commit,
        }
    }

    fn fully_sent(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

struct HeldCompletion {
    answer: StateFrame,
    measurement: StateStoreMeasurement,
    recorded: Option<RecordedStateMeasurement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapturePhase {
    Snapshot,
    Held,
    Acknowledging,
    Finished,
    Refused,
    Aborting,
}

#[derive(Debug, Clone, Copy)]
struct Capture {
    request: DecimalU64,
    id: DecimalU64,
    phase: CapturePhase,
}

/// A single fd, writer, read cursor and held answer. It is deliberately not Clone.
pub(super) struct StateChannel<S = UnixStream> {
    stream: S,
    session: String,
    run: String,
    next_request: Option<u64>,
    next_operation: Option<u64>,
    opened: bool,
    opened_scheme: Option<String>,
    // The second entry exists only for abort cleanup of a late capture answer.
    exchanges: VecDeque<Exchange>,
    received: Vec<u8>,
    received_ns: Option<u64>,
    held: Option<HeldCompletion>,
    capture: Option<Capture>,
    failure: Option<ChannelFailure>,
    receive_interrupted: bool,
    send_interrupted: bool,
    terminal_interrupted: bool,
    #[cfg(test)]
    interrupt_next_release: bool,
}

impl<S> StateChannel<S> {
    pub(super) fn outstanding(&self) -> Vec<ExchangeProgress> {
        self.exchanges.iter().map(Exchange::progress).collect()
    }

    pub(super) fn held_measurement(&self) -> Option<&StateStoreMeasurement> {
        self.held.as_ref().map(|held| &held.measurement)
    }

    /// Read-only frozen framing evidence; no descriptor is required or restored.
    pub(super) fn retained_received_bytes(&self) -> &[u8] {
        &self.received
    }
}

impl StateChannel {
    /// Consume the descriptor while moving every original request, answer and
    /// partial buffer into a carrier with no transport or live methods.
    pub(super) fn retire_failed(self) -> StateChannel<()> {
        let Self {
            stream,
            session,
            run,
            next_request,
            next_operation,
            opened,
            opened_scheme,
            exchanges,
            received,
            received_ns,
            held,
            capture,
            failure,
            receive_interrupted,
            send_interrupted,
            terminal_interrupted,
            #[cfg(test)]
            interrupt_next_release,
        } = self;
        drop(stream);
        StateChannel {
            stream: (),
            session,
            run,
            next_request,
            next_operation,
            opened,
            opened_scheme,
            exchanges,
            received,
            received_ns,
            held,
            capture,
            failure,
            receive_interrupted,
            send_interrupted,
            terminal_interrupted,
            #[cfg(test)]
            interrupt_next_release,
        }
    }

    pub(super) fn new(stream: UnixStream, session: String, run: String) -> io::Result<Self> {
        if session.is_empty() || run.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty state identity",
            ));
        }
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            session,
            run,
            next_request: Some(1),
            next_operation: Some(1),
            opened: false,
            opened_scheme: None,
            exchanges: VecDeque::new(),
            received: Vec::new(),
            received_ns: None,
            held: None,
            capture: None,
            failure: None,
            receive_interrupted: false,
            send_interrupted: false,
            terminal_interrupted: false,
            #[cfg(test)]
            interrupt_next_release: false,
        })
    }

    pub(super) fn fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }

    pub(super) fn interest(&self) -> StateInterest {
        // A validated terminal refusal already requires recording then halt.
        // Its expected peer closure must not spin a full slot under trace pressure.
        let live = self.failure.is_none() && !self.held_terminal_refusal();
        StateInterest {
            readable: live && self.held.is_none(),
            writable: live
                && !self.receive_interrupted
                && !self.terminal_interrupted
                && self.exchanges.iter().any(|exchange| !exchange.fully_sent()),
            live,
        }
    }

    /// EINTR leaves local work even when no descriptor readiness remains. The
    /// pump retries one bounded unit before waiting; it never resets a deadline.
    /// A pending release is resumed by its original author, not by transport.
    pub(super) fn needs_progress(&self) -> bool {
        self.failure.is_none()
            && !self.held_terminal_refusal()
            && (self.receive_interrupted
                || self.send_interrupted
                || self.terminal_interrupted
                || self
                    .held
                    .as_ref()
                    .is_some_and(|held| held.recorded.is_some()))
    }

    /// The pump must reduce buffered frames before polling the borrowed fd.
    /// Capture cleanup can deliver both ordered answers in one kernel read.
    pub(super) fn ready_buffered(&self) -> bool {
        self.failure.is_none() && self.held.is_none() && self.received.contains(&b'\n')
    }

    pub(super) fn failure(&self) -> Option<&ChannelFailure> {
        self.failure.as_ref()
    }

    /// A failed paged read has no valid cleanup exchange. Closing the seam
    /// makes the member drop its snapshot without inventing a release receipt.
    /// Keep all local request and answer evidence for the halt account.
    pub(super) fn close_failed_read(&self) -> io::Result<()> {
        self.stream.shutdown(std::net::Shutdown::Both)
    }

    /// A cancellation may queue fresh release behind this already-recorded
    /// Finished. This inspection grants no completion or mutable channel slot.
    pub(super) fn recorded_finished(&self, capture_id: DecimalU64) -> bool {
        self.held.as_ref().is_some_and(|held| {
            held.recorded.is_some()
                && matches!(&held.answer, StateFrame::ControlAnswer {
                    frame: StateControlAnswer::Finished { capture_id: id, .. }, ..
                } if *id == capture_id)
        })
    }

    /// Only the coordinator's recorded preactivation refusal or its sealed
    /// expired-Enter rollback may elect this disposal. Ordinary opened channels
    /// still require the recording barrier.
    pub(super) fn abandon_unactivated_open(
        &mut self,
    ) -> Result<Option<UnusedOpen>, ChannelFailure> {
        self.check_live()?;
        if self.opened
            || self.opened_scheme.is_some()
            || self.capture.is_some()
            || self.exchanges.len() != 1
            || !self.exchanges.front().is_some_and(|exchange| {
                matches!(exchange.request, StateFrame::Open { .. }) && exchange.fully_sent()
            })
            || !self.held.as_ref().is_some_and(|held| {
                matches!(held.answer, StateFrame::Opened { .. }) && held.recorded.is_none()
            })
            || !self.received.is_empty()
        {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "preactivation disposal requires one unused matching Open",
                None,
            ));
        }
        // EOF before intentional closure remains a component failure. EINTR
        // retains the exact held answer and never grants a successful disposal.
        if !self.reject_extra_answer()? {
            return Ok(None);
        }
        let held = self.held.take().expect("checked unused Open");
        self.exchanges.pop_front();
        Ok(Some(UnusedOpen {
            answer: held.answer,
            measurement: held.measurement,
        }))
    }

    pub(super) fn first_sent_ns(&self) -> Option<u64> {
        self.exchanges.front().and_then(|exchange| exchange.sent_ns)
    }

    pub(super) fn can_begin_filing(&self) -> bool {
        self.failure.is_none()
            && self.opened
            && self.exchanges.is_empty()
            && self.held.is_none()
            && self
                .capture
                .is_none_or(|capture| capture.phase == CapturePhase::Refused)
    }

    pub(super) fn identity(&self) -> (&str, &str, Option<&str>) {
        (&self.session, &self.run, self.opened_scheme.as_deref())
    }

    /// Load needs the member facts before activation permits the measurement event.
    /// No content, receipt or control result is exposed by this preflight accessor.
    pub(super) fn opened_identity(&self) -> Option<&MemberObservationIdentity> {
        match &self.held.as_ref()?.answer {
            StateFrame::Opened {
                member_observation, ..
            } => Some(member_observation),
            _ => None,
        }
    }

    /// Structural refusal before load is evidence for the existing enter
    /// refusal path. It creates no completion proof or successful open grant.
    pub(super) fn open_refusal(
        &self,
    ) -> Option<(weaver_types::state_flow::StateRefusalCode, &str)> {
        if self.opened_scheme.is_some() {
            return None;
        }
        match &self.held.as_ref()?.answer {
            StateFrame::Refused { code, detail, .. } => Some((*code, detail.as_str())),
            _ => None,
        }
    }

    pub(super) fn begin(
        &mut self,
        body: StateRequest,
        queued_ns: u64,
    ) -> Result<Common, ChannelFailure> {
        self.check_live()?;
        if !self.exchanges.is_empty() || self.held.is_some() {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Send,
                "state exchange is already outstanding",
                None,
            ));
        }
        self.reject_unsolicited()?;
        let continuation = self.check_body(&body)?;
        let id = match continuation {
            Some(id) => id,
            None => {
                let Some(id) = self.next_request else {
                    return Err(self.fail(
                        ChannelFailureKind::Protocol,
                        ChannelPoint::Counter,
                        "state request counter exhausted",
                        None,
                    ));
                };
                self.next_request = id.checked_add(1);
                id.into()
            }
        };
        let common = self.common(id);
        let request = body.frame(common.clone());
        self.push(request, queued_ns)?;
        self.note_request();
        Ok(common)
    }

    /// The coordinator, not this owner, decides that the original capture wait ended.
    /// This queues exactly one abort under that exchange's original identity.
    pub(super) fn abort_capture(&mut self, queued_ns: u64) -> Result<Common, ChannelFailure> {
        self.check_live()?;
        let Some(capture) = self.capture else {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Send,
                "abort has no capture",
                None,
            ));
        };
        if capture.phase == CapturePhase::Aborting
            || self.exchanges.len() > 1
            || self.exchanges.front().is_some_and(|exchange| {
                !matches!(
                    exchange.request.request_operation(),
                    Some(StateOperation::Snapshot | StateOperation::Acknowledge)
                )
            })
        {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Send,
                "capture cleanup is already active or has an incompatible operation",
                None,
            ));
        }
        let common = self.common(capture.request);
        self.push(
            StateFrame::Control {
                common: common.clone(),
                frame: StateControl::Abort {
                    capture_id: capture.id,
                },
            },
            queued_ns,
        )?;
        self.capture.as_mut().expect("capture exists").phase = CapturePhase::Aborting;
        Ok(common)
    }

    /// One bounded transport unit. Clock reads use the caller's common raw origin.
    /// No deadline is created, reset or elected here.
    pub(super) fn progress(
        &mut self,
        now: &mut impl FnMut() -> io::Result<u64>,
    ) -> Result<(), ChannelFailure> {
        self.progress_with(
            now,
            |fd, bytes| recv(fd, bytes, MsgFlags::MSG_DONTWAIT),
            |fd, bytes| send(fd, bytes, MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL),
        )
    }

    fn progress_with(
        &mut self,
        now: &mut impl FnMut() -> io::Result<u64>,
        receive: impl FnOnce(RawFd, &mut [u8]) -> nix::Result<usize>,
        transmit: impl FnOnce(RawFd, &[u8]) -> nix::Result<usize>,
    ) -> Result<(), ChannelFailure> {
        self.check_live()?;
        // Read first: an answer waiting before the request's final byte is premature.
        if self.held.is_none() {
            self.receive_with(now, receive)?;
        } else {
            self.check_held_terminal()?;
            if self.held_terminal_refusal() {
                return Ok(());
            }
            // Only capture cleanup may send while the original answer is held.
            // A non-consuming check still has to rule out an early cleanup reply.
            if self.exchanges.iter().any(|exchange| !exchange.fully_sent())
                && !self.reject_extra_answer()?
            {
                return Ok(());
            }
        }
        // EINTR is not evidence of an empty receive queue. In particular, it
        // cannot authorize the request's final bytes past an already queued reply.
        if self.receive_interrupted || self.terminal_interrupted || self.held_terminal_refusal() {
            return Ok(());
        }
        if let Some(index) = self
            .exchanges
            .iter()
            .position(|exchange| !exchange.fully_sent())
        {
            let observed = now().map_err(|e| {
                self.fail_at(
                    index,
                    ChannelFailureKind::Carry,
                    ChannelPoint::Send,
                    format!("monotonic clock: {e}"),
                    None,
                )
            })?;
            if observed < self.exchanges[index].queued_ns {
                return Err(self.fail_at(
                    index,
                    ChannelFailureKind::Protocol,
                    ChannelPoint::Send,
                    "send clock precedes queue clock",
                    None,
                ));
            }
            let exchange = &mut self.exchanges[index];
            let result = transmit(self.stream.as_raw_fd(), &exchange.bytes[exchange.offset..]);
            match result {
                Ok(0) => {
                    return Err(self.fail_at(
                        index,
                        ChannelFailureKind::Carry,
                        ChannelPoint::Send,
                        "zero state write",
                        None,
                    ));
                }
                Ok(count) => {
                    self.send_interrupted = false;
                    exchange.sent_ns.get_or_insert(observed);
                    exchange.offset += count;
                    exchange.commit = if exchange.fully_sent() {
                        CommitEvidence::Unknown
                    } else {
                        CommitEvidence::NotCommitted
                    };
                }
                Err(nix::errno::Errno::EAGAIN) => self.send_interrupted = false,
                Err(nix::errno::Errno::EINTR) => self.send_interrupted = true,
                Err(error) => {
                    return Err(self.fail_at(
                        index,
                        ChannelFailureKind::Carry,
                        ChannelPoint::Send,
                        error.to_string(),
                        None,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Observe terminal status without receiving into a full completion slot.
    /// A terminal refusal is already complete evidence and must be recorded before
    /// its own failure is latched; the member normally closes after that refusal.
    pub(super) fn check_held_terminal(&mut self) -> Result<(), ChannelFailure> {
        self.check_held_terminal_with(Self::terminal_status)
    }

    fn check_held_terminal_with(
        &mut self,
        status: impl FnOnce(RawFd) -> io::Result<i16>,
    ) -> Result<(), ChannelFailure> {
        self.check_live()?;
        if self.held.is_none() || self.held_terminal_refusal() {
            return Ok(());
        }
        let events = match status(self.stream.as_raw_fd()) {
            Ok(events) => {
                self.terminal_interrupted = false;
                events
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                self.terminal_interrupted = true;
                return Ok(());
            }
            Err(error) => {
                return Err(self.fail(
                    ChannelFailureKind::Carry,
                    ChannelPoint::Reply,
                    format!("state terminal-status probe: {error}"),
                    None,
                ));
            }
        };
        if events & (libc::POLLERR | libc::POLLNVAL) != 0 {
            return Err(self.fail(
                ChannelFailureKind::Carry,
                ChannelPoint::Reply,
                "state descriptor failed while an answer was held",
                None,
            ));
        }
        if events & (libc::POLLHUP | libc::POLLRDHUP) != 0 {
            return Err(self.fail(
                ChannelFailureKind::Carry,
                ChannelPoint::Reply,
                "state EOF while an answer was held",
                None,
            ));
        }
        Ok(())
    }

    fn terminal_status(fd: RawFd) -> io::Result<i16> {
        let mut status = libc::pollfd {
            fd,
            events: libc::POLLRDHUP,
            revents: 0,
        };
        // SAFETY: status is one initialized writable pollfd, and the socket's
        // owner keeps fd live for this zero-time, non-consuming call.
        if unsafe { libc::poll(&mut status, 1, 0) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(status.revents)
    }

    /// The private coordinator proof binds the entire measurement, not an asserted
    /// boolean or a caller-chosen trace sequence. The answer is taken exactly once.
    pub(super) fn release(
        &mut self,
        recorded: RecordedStateMeasurement,
    ) -> Result<Option<StateCompletion>, ChannelFailure> {
        #[cfg(test)]
        if std::mem::take(&mut self.interrupt_next_release) {
            return self.release_interrupted_for_test(recorded);
        }
        self.release_with(recorded, Self::peek)
    }

    #[cfg(test)]
    pub(super) fn interrupt_next_release_for_test(&mut self) {
        self.interrupt_next_release = true;
    }

    /// Exercise the coordinator's continuation with a real recording proof.
    /// Only the syscall outcome changes, after the ordinary proof checks.
    #[cfg(test)]
    pub(super) fn release_interrupted_for_test(
        &mut self,
        recorded: RecordedStateMeasurement,
    ) -> Result<Option<StateCompletion>, ChannelFailure> {
        self.release_with(recorded, |_| Err(nix::errno::Errno::EINTR))
    }

    fn release_with(
        &mut self,
        recorded: RecordedStateMeasurement,
        peek: impl FnOnce(RawFd) -> nix::Result<usize>,
    ) -> Result<Option<StateCompletion>, ChannelFailure> {
        self.check_live()?;
        let Some(held) = &self.held else {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "recorded measurement has no held state answer",
                None,
            ));
        };
        if held.recorded.is_some()
            || recorded.run() != self.run
            || recorded.measurement() != &held.measurement
        {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "recorded measurement differs from held state answer",
                None,
            ));
        }
        self.held
            .as_mut()
            .expect("checked held completion")
            .recorded = Some(recorded);
        self.resume_release_with(peek)
    }

    /// Continue a release whose non-consuming extra-answer check was interrupted.
    /// The exact original proof remains owned here; no second trace author is owed.
    pub(super) fn resume_release(&mut self) -> Result<Option<StateCompletion>, ChannelFailure> {
        self.resume_release_with(Self::peek)
    }

    fn resume_release_with(
        &mut self,
        peek: impl FnOnce(RawFd) -> nix::Result<usize>,
    ) -> Result<Option<StateCompletion>, ChannelFailure> {
        self.check_live()?;
        if self
            .held
            .as_ref()
            .is_none_or(|held| held.recorded.is_none())
        {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "state release has no recorded measurement proof",
                None,
            ));
        }
        let held = self.held.as_ref().expect("checked held completion");
        let discard_capture_result = self.discard_capture_result();
        if let StateFrame::Refused { code, detail, .. } = &held.answer {
            let kind = if self.opened {
                ChannelFailureKind::FilingRefused
            } else {
                ChannelFailureKind::OpenRefused
            };
            return Err(self.fail(
                kind,
                ChannelPoint::Reply,
                format!("{code:?}: {detail}"),
                Some(held.answer.common().clone()),
            ));
        }
        if !discard_capture_result
            && self
                .capture
                .is_some_and(|capture| capture.phase == CapturePhase::Aborting)
            && let StateFrame::ControlAnswer {
                frame: StateControlAnswer::ControlRefused { code, detail },
                ..
            } = &held.answer
        {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                format!("capture cleanup refused: {code:?}: {detail}"),
                Some(held.answer.common().clone()),
            ));
        }
        // A terminal refusal wins before probing for EOF: the member closes after
        // refusing a filing. Successful answers still cannot release past death.
        // A second answer is legal only for the one queued capture cleanup.
        if self.exchanges.len() == 1 {
            if !self.reject_extra_answer_with(peek)? {
                return Ok(None);
            }
            self.terminal_interrupted = false;
        } else {
            self.check_held_terminal()?;
            if self.terminal_interrupted {
                return Ok(None);
            }
        }
        let held = self.held.take().expect("checked held completion");
        if !discard_capture_result {
            self.note_answer(&held.answer);
        }
        self.exchanges.pop_front();
        Ok(Some(StateCompletion {
            answer: held.answer,
            discard_capture_result,
        }))
    }

    fn discard_capture_result(&self) -> bool {
        self.capture
            .is_some_and(|capture| capture.phase == CapturePhase::Aborting)
            && self.exchanges.front().is_some_and(|exchange| {
                matches!(
                    exchange.request.request_operation(),
                    Some(StateOperation::Snapshot | StateOperation::Acknowledge)
                )
            })
    }

    fn held_terminal_refusal(&self) -> bool {
        match self.held.as_ref().map(|held| &held.answer) {
            Some(StateFrame::Refused { .. }) => true,
            Some(StateFrame::ControlAnswer {
                frame: StateControlAnswer::ControlRefused { .. },
                ..
            }) => {
                !self.discard_capture_result()
                    && self
                        .capture
                        .is_some_and(|capture| capture.phase == CapturePhase::Aborting)
            }
            _ => false,
        }
    }

    fn common(&self, request_id: DecimalU64) -> Common {
        Common {
            session_id: self.session.clone(),
            run_id: self.run.clone(),
            request_id,
        }
    }

    fn check_body(&mut self, body: &StateRequest) -> Result<Option<DecimalU64>, ChannelFailure> {
        let allowed = match (self.opened, body) {
            (false, StateRequest::Open { .. }) => self.next_request == Some(1),
            (false, _) | (true, StateRequest::Open { .. }) => false,
            (true, _) => true,
        };
        if !allowed {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Send,
                "state operation is outside the opened phase",
                None,
            ));
        }
        match (self.capture, body) {
            (
                Some(capture),
                StateRequest::Control(StateControl::Acknowledge { capture_id, .. }),
            ) if capture.id == *capture_id && capture.phase == CapturePhase::Held => {
                Ok(Some(capture.request))
            }
            (Some(capture), StateRequest::Control(StateControl::Abort { capture_id }))
                if capture.id == *capture_id
                    && matches!(
                        capture.phase,
                        CapturePhase::Held | CapturePhase::Finished | CapturePhase::Refused
                    ) =>
            {
                Ok(Some(capture.request))
            }
            (Some(capture), StateRequest::Control(StateControl::ReleaseCapture { capture_id }))
                if capture.id == *capture_id && capture.phase == CapturePhase::Finished =>
            {
                Ok(None)
            }
            (Some(capture), _) if capture.phase != CapturePhase::Refused => Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Send,
                "capture holds the state exchange",
                None,
            )),
            (
                _,
                StateRequest::Control(
                    StateControl::Acknowledge { .. }
                    | StateControl::Abort { .. }
                    | StateControl::ReleaseCapture { .. },
                ),
            ) => Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Send,
                "capture continuation does not match held capture",
                None,
            )),
            (_, StateRequest::Control(StateControl::Snapshot { capture_id }))
                if capture_id.0 == 0 =>
            {
                Err(self.fail(
                    ChannelFailureKind::Protocol,
                    ChannelPoint::Send,
                    "zero capture identity",
                    None,
                ))
            }
            _ => Ok(None),
        }
    }

    fn push(&mut self, request: StateFrame, queued_ns: u64) -> Result<(), ChannelFailure> {
        if self.next_operation.is_none() {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Counter,
                "state observation counter exhausted",
                None,
            ));
        }
        let bytes = match encode_state_line(&request, FrameDirection::Request) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.exchanges.push_back(Exchange {
                    request,
                    bytes: Vec::new(),
                    offset: 0,
                    queued_ns,
                    sent_ns: None,
                    commit: CommitEvidence::NotRequested,
                });
                return Err(self.fail(
                    ChannelFailureKind::Carry,
                    ChannelPoint::Send,
                    format!("cannot encode state request: {error}"),
                    None,
                ));
            }
        };
        self.exchanges.push_back(Exchange {
            request,
            bytes,
            offset: 0,
            queued_ns,
            sent_ns: None,
            commit: CommitEvidence::NotRequested,
        });
        Ok(())
    }

    fn note_request(&mut self) {
        let request = &self.exchanges.back().expect("just queued").request;
        match request {
            StateFrame::Control {
                common,
                frame: StateControl::Snapshot { capture_id },
            } => {
                self.capture = Some(Capture {
                    request: common.request_id,
                    id: *capture_id,
                    phase: CapturePhase::Snapshot,
                });
            }
            StateFrame::Control {
                frame: StateControl::Acknowledge { .. },
                ..
            } => {
                self.capture.as_mut().expect("checked capture").phase = CapturePhase::Acknowledging
            }
            StateFrame::Control {
                frame: StateControl::Abort { .. },
                ..
            } => self.capture.as_mut().expect("checked capture").phase = CapturePhase::Aborting,
            _ => {
                if self
                    .capture
                    .is_some_and(|capture| capture.phase == CapturePhase::Refused)
                {
                    self.capture = None;
                }
            }
        }
    }

    fn note_answer(&mut self, answer: &StateFrame) {
        match answer {
            StateFrame::Opened { scheme_id, .. } => {
                self.opened = true;
                self.opened_scheme = Some(scheme_id.clone());
            }
            StateFrame::ControlAnswer { frame, .. } => match frame {
                StateControlAnswer::Snapshot { .. } => {
                    self.capture.as_mut().expect("snapshot capture").phase = CapturePhase::Held
                }
                StateControlAnswer::Finished { .. } => {
                    self.capture.as_mut().expect("acknowledged capture").phase =
                        CapturePhase::Finished
                }
                StateControlAnswer::Aborted { .. } | StateControlAnswer::CaptureReleased { .. } => {
                    self.capture = None
                }
                StateControlAnswer::ControlRefused { .. } => {
                    match self.capture.as_mut().map(|capture| &mut capture.phase) {
                        Some(phase @ CapturePhase::Snapshot) => *phase = CapturePhase::Refused,
                        Some(phase @ CapturePhase::Acknowledging) => *phase = CapturePhase::Held,
                        _ => {}
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn receive_with(
        &mut self,
        now: &mut impl FnMut() -> io::Result<u64>,
        receive: impl FnOnce(RawFd, &mut [u8]) -> nix::Result<usize>,
    ) -> Result<(), ChannelFailure> {
        if !self.received.contains(&b'\n') {
            let mut buffer = [0u8; 16 * 1024];
            // One extra byte proves that a missing LF cannot fit the bounded line.
            let limit = (STATE_ANSWER_LINE_BOUND + 1 - self.received.len()).min(buffer.len());
            match receive(self.stream.as_raw_fd(), &mut buffer[..limit]) {
                Ok(0) => {
                    return Err(self.fail(
                        ChannelFailureKind::Carry,
                        ChannelPoint::Reply,
                        "state EOF",
                        None,
                    ));
                }
                Ok(count) => {
                    self.receive_interrupted = false;
                    self.received.extend_from_slice(&buffer[..count]);
                    self.received_ns = Some(now().map_err(|error| {
                        self.fail(
                            ChannelFailureKind::Carry,
                            ChannelPoint::Reply,
                            format!("monotonic clock: {error}"),
                            None,
                        )
                    })?);
                }
                Err(nix::errno::Errno::EAGAIN) => {
                    self.receive_interrupted = false;
                    return Ok(());
                }
                Err(nix::errno::Errno::EINTR) => {
                    self.receive_interrupted = true;
                    return Ok(());
                }
                Err(error) => {
                    return Err(self.fail(
                        ChannelFailureKind::Carry,
                        ChannelPoint::Reply,
                        error.to_string(),
                        None,
                    ));
                }
            }
        }
        if self.exchanges.is_empty() {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "unsolicited state bytes",
                None,
            ));
        }
        if !self.exchanges.front().expect("outstanding").fully_sent() {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "state answered before the complete request was sent",
                None,
            ));
        }
        let Some(end) = self.received.iter().position(|byte| *byte == b'\n') else {
            if self.received.len() >= STATE_ANSWER_LINE_BOUND {
                return Err(self.fail(
                    ChannelFailureKind::Protocol,
                    ChannelPoint::Reply,
                    "state answer exceeds its line bound",
                    None,
                ));
            }
            return Ok(());
        };
        let received_ns = self
            .received_ns
            .expect("received bytes have a clock sample");
        let answer = decode_state_line(&self.received[..=end], FrameDirection::Response).map_err(
            |error| {
                self.fail(
                    ChannelFailureKind::Protocol,
                    ChannelPoint::Reply,
                    error.to_string(),
                    None,
                )
            },
        )?;
        let operation_id = self.next_operation.ok_or_else(|| {
            self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Counter,
                "state observation counter exhausted",
                None,
            )
        })?;
        let exchange = self.exchanges.front().expect("outstanding");
        if let Err(error) = answer.validate_response_to(&exchange.request, operation_id.into()) {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                error.to_string(),
                Some(answer.common().clone()),
            ));
        }
        let measurement = StateStoreMeasurement {
            observation: answer.observation().expect("validated response").clone(),
            request_source: exchange.source(),
            harness_queued_ns: exchange.queued_ns.into(),
            harness_sent_ns: exchange.sent_ns.expect("fully sent").into(),
            harness_received_ns: received_ns.into(),
        };
        if let Err(error) = measurement.validate() {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                error.to_string(),
                Some(answer.common().clone()),
            ));
        }
        if self.received.len() > end + 1
            && (self.exchanges.len() != 2 || !self.exchanges[1].fully_sent())
        {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "duplicate or premature state answer after complete response",
                None,
            ));
        }
        self.received.drain(..=end);
        if !self.received.contains(&b'\n') {
            self.received_ns = None;
        }
        self.next_operation = operation_id.checked_add(1);
        let exchange = self.exchanges.front_mut().expect("outstanding");
        match answer {
            StateFrame::Filed { .. } => exchange.commit = CommitEvidence::Committed,
            StateFrame::Refused { .. } => exchange.commit = CommitEvidence::Refused,
            _ => {}
        }
        self.held = Some(HeldCompletion {
            answer,
            measurement,
            recorded: None,
        });
        Ok(())
    }

    fn reject_unsolicited(&mut self) -> Result<(), ChannelFailure> {
        if !self.received.is_empty() {
            return Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "buffered unsolicited state bytes",
                None,
            ));
        }
        // An interrupted preflight check can stage the request, but leaves the
        // receive probe pending. progress must resolve it before sending bytes.
        self.reject_extra_answer().map(|_| ())
    }

    fn peek(fd: RawFd) -> nix::Result<usize> {
        let mut byte = [0];
        recv(fd, &mut byte, MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK)
    }

    fn reject_extra_answer(&mut self) -> Result<bool, ChannelFailure> {
        self.reject_extra_answer_with(Self::peek)
    }

    /// true means EAGAIN proved no extra answer; false means the single probe
    /// was interrupted and neither sending nor releasing is authorized by it.
    fn reject_extra_answer_with(
        &mut self,
        peek: impl FnOnce(RawFd) -> nix::Result<usize>,
    ) -> Result<bool, ChannelFailure> {
        match peek(self.stream.as_raw_fd()) {
            Ok(0) => Err(self.fail(
                ChannelFailureKind::Carry,
                ChannelPoint::Reply,
                "state EOF before exchange release",
                None,
            )),
            Err(nix::errno::Errno::EAGAIN) => {
                self.receive_interrupted = false;
                Ok(true)
            }
            Err(nix::errno::Errno::EINTR) => {
                self.receive_interrupted = true;
                Ok(false)
            }
            Ok(_) => Err(self.fail(
                ChannelFailureKind::Protocol,
                ChannelPoint::Reply,
                "unsolicited or duplicate state answer",
                None,
            )),
            Err(error) => Err(self.fail(
                ChannelFailureKind::Carry,
                ChannelPoint::Reply,
                error.to_string(),
                None,
            )),
        }
    }

    fn check_live(&self) -> Result<(), ChannelFailure> {
        self.failure.clone().map_or(Ok(()), Err)
    }

    fn fail(
        &mut self,
        kind: ChannelFailureKind,
        point: ChannelPoint,
        detail: impl Into<String>,
        received: Option<Common>,
    ) -> ChannelFailure {
        self.fail_at(0, kind, point, detail, received)
    }

    fn fail_at(
        &mut self,
        exchange: usize,
        kind: ChannelFailureKind,
        point: ChannelPoint,
        detail: impl Into<String>,
        received: Option<Common>,
    ) -> ChannelFailure {
        self.failure
            .get_or_insert_with(|| ChannelFailure {
                kind,
                point,
                detail: detail.into(),
                received,
                outstanding: self
                    .exchanges
                    .get(exchange)
                    .map(|exchange| Box::new(exchange.progress())),
            })
            .clone()
    }
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
