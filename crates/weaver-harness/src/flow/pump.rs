//! The common transport pump. A step never authors, reduces, or takes a completion.
//!
//! Concrete owners retain all read cursors, bytes and descriptor custody. Lifecycle
//! gets a bounded probe before buffered work, full slots retain terminal monitoring,
//! and the remaining ready sources rotate. Every unit keeps the original deadlines.

use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::libc;
use nix::poll::PollFlags;
use nix::sys::socket::{SockFlag, accept4, getsockopt, sockopt};
use weaver_types::{MAX_ENVELOPE_BYTES, OrganEnvelope};

use crate::channel::{CoordinationListener, OrganChannel};
use crate::failure::ChannelFault;

use super::classifier::ClassifierState;
use super::clock::{
    AbsoluteBound, ClockOrigin, DeadlineRegistry, PumpIo, WaitInterest, WaitOutcome,
};
use super::coordinator::{
    AdmissionAdvance, FlowCoordinator, FlowFailure, FlowRecording, TransportSource,
};
use super::pump_transport::{
    DecodePort, Failure, LabelPort, OrganPort, ReceiveProgress, SendProgress, Stage, StageFailure,
};
use super::ste100_socket::{AdmissionPoll, SocketPort};

const SOURCE_COUNT: usize = 11;
const CONTROL_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PumpDeadline {
    Enter,
    Leave,
    Gate,
    Cut,
    Decision,
    StateControl,
    /// A silent secondary admin dial, owned by the accepted Leave.
    AdminRead,
    /// Snapshot/finished missing results require cleanup, not immediate halt.
    CaptureResult,
    /// The unchanged publication window, retained separately from leg waits.
    CaptureWindow,
    Label,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum PumpSource {
    Listener,
    Admin,
    AdminDial,
    LeavePeer,
    Residency,
    Gate,
    Decode,
    Label,
    State,
    Classifier,
    Trace,
}

const LIFECYCLE: [PumpSource; 5] = [
    PumpSource::Listener,
    PumpSource::Admin,
    PumpSource::AdminDial,
    PumpSource::LeavePeer,
    PumpSource::Residency,
];
const ROTATING: [PumpSource; 6] = [
    PumpSource::Gate,
    PumpSource::Decode,
    PumpSource::Label,
    PumpSource::State,
    PumpSource::Classifier,
    PumpSource::Trace,
];

#[derive(Debug)]
pub(super) enum PumpFailure {
    Clock(io::Error),
    Transport {
        source: PumpSource,
        failure: Failure,
    },
    Trace(weaver_trace::Failure),
    Flow(FlowFailure),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PumpStep {
    Progress {
        sources: Vec<PumpSource>,
    },
    /// The owner can reduce a measurement tick without expiring an operation.
    Observation,
    Deadline {
        key: PumpDeadline,
        bound: AbsoluteBound,
    },
    Idle,
}

/// Listener, current admin exchange and SPU residency are distinct endpoints.
/// The accepted connection remains in the listener's slot until its caller may
/// reduce it. No existing read handle is cloned by these conversions.
#[derive(Default)]
pub(super) struct LifecyclePorts<'a> {
    pub listener: Option<&'a mut LifecycleListener>,
    pub peer: Option<&'a mut LifecyclePeer>,
    /// A dial during an already accepted Leave. Its raw directives are not
    /// new boot/placement cancellation authority; the Leave owner judges them.
    pub dial: Option<&'a mut LifecyclePeer>,
    /// The already accepted Leave's connection. Extra packets stay held and
    /// EOF is a fact for its owner, never another accepted lifecycle directive.
    pub leave_peer: Option<&'a mut LifecyclePeer>,
    pub residency: Option<&'a mut OrganPort>,
}

pub(super) struct PumpPorts<'a, I: PumpIo> {
    pub lifecycle: LifecyclePorts<'a>,
    pub gate: Option<&'a mut OrganPort>,
    pub decode: Option<&'a mut DecodePort>,
    pub label: Option<&'a mut LabelPort>,
    pub io: &'a mut I,
}

/// An Enter's handles stay beside the exact envelope until one explicit take.
/// The caller, not transport, judges which directive owes each supplied handle.
#[derive(Debug)]
pub(super) struct LifecycleMessage<E = OrganEnvelope> {
    pub envelope: E,
    pub sink: Option<OwnedFd>,
    pub state: Option<OwnedFd>,
}

pub(super) struct LifecycleListener {
    listener: CoordinationListener,
    held: Option<LifecyclePeer>,
    accepted_ns: Option<u64>,
    failure: Option<Failure>,
    interrupted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListenProgress {
    Pending,
    Accepted,
    RefusedPeer,
}

impl LifecycleListener {
    pub(super) fn new(listener: CoordinationListener) -> Result<Self, Failure> {
        let flags = fcntl(listener.as_fd(), FcntlArg::F_GETFL).map_err(io_failure)?;
        fcntl(
            listener.as_fd(),
            FcntlArg::F_SETFL(OFlag::from_bits_retain(flags) | OFlag::O_NONBLOCK),
        )
        .map_err(io_failure)?;
        Ok(Self {
            listener,
            held: None,
            accepted_ns: None,
            failure: None,
            interrupted: false,
        })
    }

    pub(super) fn held(&self) -> Option<&LifecyclePeer> {
        self.held.as_ref()
    }
    pub(super) fn take_connection(&mut self) -> Option<LifecyclePeer> {
        self.accepted_ns = None;
        self.held.take()
    }
    pub(super) fn accepted_ns(&self) -> Option<u64> {
        self.accepted_ns
    }
    pub(super) fn take_observed_connection(&mut self) -> Option<(LifecyclePeer, u64)> {
        let at = self.accepted_ns?;
        let peer = self.held.take()?;
        self.accepted_ns = None;
        Some((peer, at))
    }
    pub(super) fn gate_socket(&self) -> std::path::PathBuf {
        self.listener.gate_socket()
    }
    pub(super) fn unlink(&self) {
        self.listener.unlink();
    }
    pub(super) fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }

    /// Before a recording context exists, accept through this same credential
    /// check. Transfer the untouched peer to its sole Enter recvmsg owner.
    pub(super) fn accept_enter(&mut self) -> Result<Option<LifecyclePeer>, Failure> {
        self.advance()?;
        Ok(self.take_connection())
    }

    /// Observe before the actual nonblocking accept. A queued connection keeps
    /// that first anchor even when its owner reduces it later.
    pub(super) fn accept_observed(&mut self, before: u64) -> Result<bool, Failure> {
        if self.held.is_some() {
            return Ok(true);
        }
        let progress = self.advance()?;
        if self.held.is_some() {
            self.accepted_ns = Some(before);
        }
        Ok(progress != ListenProgress::Pending || self.interrupted)
    }

    pub(super) fn interest(&self) -> (BorrowedFd<'_>, PollFlags) {
        if let Some(peer) = &self.held {
            (peer.fd(), terminal_flags())
        } else {
            (self.listener.as_fd(), PollFlags::POLLIN)
        }
    }

    fn advance(&mut self) -> Result<ListenProgress, Failure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        if let Some(peer) = &mut self.held {
            // The accept itself occupies this slot. Do not consume Enter until
            // the caller transfers the peer into the current-exchange owner.
            return peer.check_terminal().map(|_| ListenProgress::Pending);
        }
        self.interrupted = false;
        let raw = match accept4(
            self.listener.as_fd().as_raw_fd(),
            SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
        ) {
            Ok(raw) => raw,
            Err(Errno::EAGAIN) => return Ok(ListenProgress::Pending),
            Err(Errno::EINTR) => {
                self.interrupted = true;
                return Ok(ListenProgress::Pending);
            }
            Err(error) => return self.fail(io_failure(error)),
        };
        // SAFETY: accept4 just created the sole owned descriptor.
        let end = unsafe { OwnedFd::from_raw_fd(raw) };
        let credentials = match getsockopt(&end, sockopt::PeerCredentials) {
            Ok(credentials) => credentials,
            Err(error) => return self.fail(io_failure(error)),
        };
        if credentials.uid() != 0 {
            // Refusal of an unauthenticated dial does not end serving operation.
            return Ok(ListenProgress::RefusedPeer);
        }
        let peer = LifecyclePeer::from_owned(end).inspect_err(|failure| {
            self.failure = Some(failure.clone());
        })?;
        self.held = Some(peer);
        Ok(ListenProgress::Accepted)
    }

    fn fail<T>(&mut self, failure: Failure) -> Result<T, Failure> {
        self.failure = Some(failure.clone());
        Err(failure)
    }
}

/// This owner reuses the ordinary bounded sender, but never invokes that port's
/// reader. Its one recvmsg reader alone may receive Enter's ancillary handles.
pub(super) struct LifecyclePeer {
    sender: OrganPort,
    held: Option<LifecycleMessage>,
    wire: Option<Vec<u8>>,
    failure: Option<Failure>,
    interrupted: bool,
    zero_received: bool,
    enter_ready_guard: bool,
}

impl LifecyclePeer {
    pub(super) fn from_channel(channel: OrganChannel) -> Result<Self, Failure> {
        Self::from_owned(channel.into_fd())
    }
    pub(super) fn from_owned(end: OwnedFd) -> Result<Self, Failure> {
        Ok(Self {
            sender: OrganPort::from_owned(end)?,
            held: None,
            wire: None,
            failure: None,
            interrupted: false,
            zero_received: false,
            enter_ready_guard: false,
        })
    }
    pub(super) fn fd(&self) -> BorrowedFd<'_> {
        self.sender.fd()
    }
    pub(super) fn held(&self) -> Option<&LifecycleMessage> {
        self.held.as_ref()
    }
    /// Only these held admin openers revoke an in-progress boot Open.
    pub(super) fn cancels_boot_open(&self) -> bool {
        self.held.as_ref().is_some_and(|held| {
            held.envelope.exchange.opener == weaver_types::Opener::Admin
                && held.envelope.position == weaver_types::Position::Open
                && matches!(
                    held.envelope.payload,
                    weaver_types::Payload::Directive(
                        weaver_types::LifecycleDirective::Stop { .. }
                            | weaver_types::LifecycleDirective::Leave { .. }
                            | weaver_types::LifecycleDirective::JoinLeave { .. }
                    )
                )
        })
    }
    pub(super) fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref().or(self.sender.failure())
    }
    pub(super) fn take_completion(&mut self) -> Result<Option<LifecycleMessage>, Failure> {
        // Closing after a complete directive does not destroy its evidence.
        if self.held.is_some() {
            self.wire = None;
            return Ok(self.held.take());
        }
        self.live()?;
        Ok(None)
    }
    // A refused send returns the original envelope without a new allocation.
    #[allow(clippy::result_large_err)]
    pub(super) fn stage(
        &mut self,
        envelope: OrganEnvelope,
    ) -> Result<Stage<OrganEnvelope>, StageFailure<OrganEnvelope>> {
        if let Err(failure) = self.live() {
            return Err(StageFailure {
                input: envelope,
                failure,
            });
        }
        let result = self.sender.stage(envelope)?;
        if matches!(result, Stage::Queued) {
            self.enter_ready_guard = false;
        }
        Ok(result)
    }
    #[allow(clippy::result_large_err)]
    pub(super) fn stage_tracked(
        &mut self,
        envelope: OrganEnvelope,
    ) -> Result<super::pump_transport::TrackedStage, StageFailure<OrganEnvelope>> {
        if let Err(failure) = self.live() {
            return Err(StageFailure {
                input: envelope,
                failure,
            });
        }
        let result = self.sender.stage_tracked(envelope)?;
        if matches!(result, super::pump_transport::TrackedStage::Queued(_)) {
            self.enter_ready_guard = false;
        }
        Ok(result)
    }
    #[allow(clippy::result_large_err)]
    pub(super) fn stage_enter_ready(
        &mut self,
        envelope: OrganEnvelope,
    ) -> Result<super::pump_transport::TrackedStage, StageFailure<OrganEnvelope>> {
        let result = self.stage_tracked(envelope)?;
        if matches!(result, super::pump_transport::TrackedStage::Queued(_)) {
            self.enter_ready_guard = true;
        }
        Ok(result)
    }
    #[allow(clippy::result_large_err)]
    pub(super) fn stage_observation(
        &mut self,
        envelope: weaver_types::state_flow::enter::ObservationEnvelope,
    ) -> Result<
        super::pump_transport::TrackedStage<weaver_types::state_flow::enter::ObservationEnvelope>,
        StageFailure<weaver_types::state_flow::enter::ObservationEnvelope>,
    > {
        if let Err(failure) = self.live() {
            return Err(StageFailure {
                input: envelope,
                failure,
            });
        }
        let result = self.sender.stage_observation(envelope)?;
        if matches!(result, super::pump_transport::TrackedStage::Queued(_)) {
            self.enter_ready_guard = false;
        }
        Ok(result)
    }
    fn enter_ready_pending(&self) -> bool {
        self.enter_ready_guard && self.sender.send_pending()
    }
    pub(super) fn owns_send(&self, send: &super::pump_transport::OrganSend) -> bool {
        send.belongs_to(&self.sender)
    }
    pub(super) fn try_send_observed(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<SendProgress, Failure> {
        self.live()?;
        if self.interrupted || self.zero_received {
            return Ok(SendProgress::Pending);
        }
        self.sender.try_send_observed(io)
    }
    pub(super) fn try_send(&mut self) -> Result<SendProgress, Failure> {
        self.live()?;
        if self.interrupted || self.zero_received {
            return Ok(SendProgress::Pending);
        }
        self.sender.try_send()
    }
    pub(super) fn interest(&self) -> PollFlags {
        if self.failure().is_some() {
            return PollFlags::empty();
        }
        let mut flags = terminal_flags();
        if self.held.is_none() {
            flags |= PollFlags::POLLIN;
        }
        if self.sender.interest().writable {
            flags |= PollFlags::POLLOUT;
        }
        flags
    }
    fn needs_progress(&self) -> bool {
        self.failure().is_none()
            && (self.interrupted || self.zero_received || self.sender.needs_progress())
    }
    fn live(&self) -> Result<(), Failure> {
        self.failure().cloned().map_or(Ok(()), Err)
    }
    fn fail<T>(&mut self, failure: Failure) -> Result<T, Failure> {
        if self.failure.is_none() {
            self.failure = Some(failure.clone());
        }
        Err(failure)
    }
    fn check_terminal(&mut self) -> Result<bool, Failure> {
        self.live()?;
        let mut descriptor = libc::pollfd {
            fd: self.fd().as_raw_fd(),
            events: terminal_flags().bits(),
            revents: 0,
        };
        // SAFETY: one valid borrowed descriptor and one writable poll entry.
        let result = unsafe { libc::poll(&mut descriptor, 1, 0) };
        if result < 0 {
            let error = Errno::last();
            if error == Errno::EINTR {
                self.interrupted = true;
                return Ok(false);
            }
            return self.fail(io_failure(error));
        }
        self.interrupted = false;
        if is_terminal(PollFlags::from_bits_retain(descriptor.revents)) {
            return self.fail(closed_failure("admin peer closed"));
        }
        Ok(true)
    }

    pub(super) fn try_receive(&mut self) -> Result<ReceiveProgress, Failure> {
        match self.receive_message(|bytes| serde_json::from_slice::<OrganEnvelope>(bytes))? {
            Some(message) => {
                self.held = Some(message);
                Ok(ReceiveProgress::Held)
            }
            None => Ok(ReceiveProgress::Pending),
        }
    }

    fn receive_message<E>(
        &mut self,
        decode: impl FnOnce(&[u8]) -> Result<E, serde_json::Error>,
    ) -> Result<Option<LifecycleMessage<E>>, Failure> {
        self.live()?;
        if self.held.is_some() {
            self.check_terminal()?;
            return Ok(None);
        }
        if self.zero_received {
            if !self.check_terminal()? {
                return Ok(None);
            }
            return self.fail(invalid_failure("empty live coordination packet"));
        }
        let mut bytes = vec![0u8; MAX_ENVELOPE_BYTES];
        // Align the ancillary buffer for cmsghdr without casting a byte-array
        // alignment assumption. The byte capacity matches the existing reader.
        let mut control = [0usize; CONTROL_BYTES / std::mem::size_of::<usize>()];
        let mut iovec = libc::iovec {
            iov_base: bytes.as_mut_ptr().cast(),
            iov_len: bytes.len(),
        };
        // SAFETY: zero is a valid empty msghdr, and every referenced buffer lives
        // through recvmsg and the subsequent bounded ancillary walk.
        let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
        header.msg_iov = &mut iovec;
        header.msg_iovlen = 1;
        header.msg_control = control.as_mut_ptr().cast();
        header.msg_controllen = CONTROL_BYTES;
        // SAFETY: the header owns valid writable buffers and the fd is borrowed.
        let count = unsafe {
            libc::recvmsg(
                self.fd().as_raw_fd(),
                &mut header,
                libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC,
            )
        };
        if count < 0 {
            let error = Errno::last();
            if error == Errno::EINTR {
                self.interrupted = true;
                return Ok(None);
            }
            if error == Errno::EAGAIN {
                self.interrupted = false;
                return Ok(None);
            }
            return self.fail(io_failure(error));
        }
        self.interrupted = false;
        let mut sink = None;
        let mut state = None;
        // SAFETY: only kernel-returned control headers within the reported
        // buffer are walked. Every installed descriptor is adopted before any
        // truncation, decode, zero-length or surplus refusal can return.
        unsafe {
            let mut message = libc::CMSG_FIRSTHDR(&header);
            while !message.is_null() {
                let offset = message
                    .cast::<u8>()
                    .offset_from(header.msg_control.cast::<u8>())
                    as usize;
                let length = (*message).cmsg_len;
                let body = libc::CMSG_LEN(0) as usize;
                if length < body
                    || offset
                        .checked_add(length)
                        .is_none_or(|end| end > header.msg_controllen)
                {
                    break;
                }
                if (*message).cmsg_level == libc::SOL_SOCKET
                    && (*message).cmsg_type == libc::SCM_RIGHTS
                {
                    let data = libc::CMSG_DATA(message);
                    for index in 0..(length - body) / std::mem::size_of::<RawFd>() {
                        let raw = std::ptr::read_unaligned(
                            data.add(index * std::mem::size_of::<RawFd>())
                                .cast::<RawFd>(),
                        );
                        if raw < 0 {
                            continue;
                        }
                        let owned = OwnedFd::from_raw_fd(raw);
                        if sink.is_none() {
                            sink = Some(owned);
                        } else if state.is_none() {
                            state = Some(owned);
                        }
                        // Surplus descriptors close immediately, as in the
                        // existing recv_with_descriptor custody contract.
                    }
                }
                message = libc::CMSG_NXTHDR(&header, message);
            }
        }
        if header.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0 {
            return self.fail(Failure {
                fault: ChannelFault::Truncated {
                    bound: MAX_ENVELOPE_BYTES,
                },
                detail: "coordination packet or ancillary data truncated".into(),
                errno: None,
            });
        }
        if count == 0 {
            self.zero_received = true;
            if !self.check_terminal()? {
                return Ok(None);
            }
            return self.fail(invalid_failure("empty live coordination packet"));
        }
        bytes.truncate(count as usize);
        let decoded = decode(&bytes);
        self.wire = Some(bytes);
        let envelope = decoded.map_err(|error| {
            let failure =
                invalid_failure(&crate::channel::undecodable_detail("coordination", &error));
            self.failure = Some(failure.clone());
            failure
        })?;
        Ok(Some(LifecycleMessage {
            envelope,
            sink,
            state,
        }))
    }
}

fn io_failure(error: Errno) -> Failure {
    Failure {
        fault: ChannelFault::Closed,
        detail: error.to_string(),
        errno: Some(error as i32),
    }
}
fn closed_failure(detail: &str) -> Failure {
    Failure {
        fault: ChannelFault::Closed,
        detail: detail.into(),
        errno: None,
    }
}
fn invalid_failure(detail: &str) -> Failure {
    Failure {
        fault: ChannelFault::Undecodable,
        detail: detail.into(),
        errno: None,
    }
}
fn terminal_flags() -> PollFlags {
    PollFlags::from_bits_retain(libc::POLLRDHUP)
}
fn is_terminal(flags: PollFlags) -> bool {
    flags.intersects(
        PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL | terminal_flags(),
    )
}

pub(super) struct CommonPump {
    origin: ClockOrigin,
    previous: u64,
    cursor: usize,
    send_next: [bool; SOURCE_COUNT],
}

impl CommonPump {
    pub(super) fn new(origin: ClockOrigin) -> Self {
        Self {
            origin,
            previous: origin.raw(),
            cursor: 0,
            send_next: [false; SOURCE_COUNT],
        }
    }

    fn deadline<I: PumpIo>(
        &mut self,
        io: &mut I,
        deadlines: &DeadlineRegistry<PumpDeadline>,
    ) -> Result<Option<PumpStep>, PumpFailure> {
        let now = io.now().map_err(PumpFailure::Clock)?;
        if now < self.origin.raw() || now < self.previous {
            return Err(PumpFailure::Clock(io::Error::new(
                io::ErrorKind::InvalidData,
                "pump clock precedes its common origin or prior reading",
            )));
        }
        self.previous = now;
        Ok(deadlines
            .expired(now)
            .min_by_key(|(_, bound)| bound.deadline_ns())
            .map(|(key, bound)| PumpStep::Deadline { key, bound }))
    }

    /// No completion is consumed here, including a complete result that arrives
    /// while an ordinary trace author waits. The owner reduces it after return.
    pub(super) fn step<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        coordinator: Option<&mut FlowCoordinator<SocketPort>>,
        cx: &mut FlowRecording<'_>,
        deadlines: &DeadlineRegistry<PumpDeadline>,
    ) -> Result<PumpStep, PumpFailure> {
        self.step_with_wait(ports, coordinator, cx, deadlines, true)
    }

    /// The same bounded observation and transport work, with an optional wait
    /// when nothing is ready. A local decision may observe without inventing a
    /// protocol deadline or substituting a clock/wait provider.
    pub(super) fn step_with_wait<I: PumpIo>(
        &mut self,
        ports: &mut PumpPorts<'_, I>,
        mut coordinator: Option<&mut FlowCoordinator<SocketPort>>,
        cx: &mut FlowRecording<'_>,
        deadlines: &DeadlineRegistry<PumpDeadline>,
        wait: bool,
    ) -> Result<PumpStep, PumpFailure> {
        if let Some(expired) = self.deadline(ports.io, deadlines)? {
            return Ok(expired);
        }
        cx.barrier
            .drain_status(cx.budget)
            .map_err(PumpFailure::Trace)?;
        if let Some(failure) = coordinator.as_ref().and_then(|owner| owner.failure()) {
            return Err(PumpFailure::Flow(failure.clone()));
        }
        if let Some(failure) = standing_transport_failure(ports) {
            return Err(failure);
        }
        let mut progressed = Vec::new();
        // Readiness is only a hint for ordering. Never turn HUP into a
        // synthesized result: the owning parser may still hold a complete
        // refusal or component-failure reply before that close.
        let mut ready = readiness_hints(ports, coordinator.as_deref(), cx)?;
        for source in LIFECYCLE.into_iter().chain(ROTATING) {
            if local_work(
                source,
                ports,
                coordinator.as_deref(),
                cx.barrier.pending().is_none(),
            ) {
                ready[source as usize] |= PollFlags::POLLIN | PollFlags::POLLOUT;
            }
        }
        if ready.iter().any(|flags| !flags.is_empty()) || !progressed.is_empty() {
            // Local work must not starve another ready descriptor. A bounded
            // nonblocking probe uses its existing owner and never a second
            // poll clock. EAGAIN is simply no progress for that source.
            for source in LIFECYCLE.into_iter().chain(ROTATING) {
                if present(source, ports, coordinator.as_deref()) {
                    ready[source as usize] |= PollFlags::POLLIN;
                    if interest(
                        source,
                        &ports.lifecycle,
                        &ports.gate,
                        &ports.decode,
                        &ports.label,
                        coordinator.as_deref(),
                        cx,
                    )
                    .is_some_and(|(_, events)| events.contains(PollFlags::POLLOUT))
                    {
                        ready[source as usize] |= PollFlags::POLLOUT;
                    }
                }
            }
        }
        if ready.iter().all(PollFlags::is_empty) && progressed.is_empty() {
            let observation = coordinator
                .as_deref_mut()
                .map(|owner| owner.observation_wake(cx))
                .transpose()
                .map_err(PumpFailure::Flow)?
                .flatten();
            if !wait {
                return Ok(if observation.is_some_and(|tick| self.previous >= tick) {
                    PumpStep::Observation
                } else {
                    PumpStep::Idle
                });
            }
            let wake = deadlines.nearest().into_iter().chain(observation).min();
            let mut interests = Vec::new();
            let mut sources = Vec::new();
            for source in LIFECYCLE.into_iter().chain(ROTATING) {
                if let Some((fd, events)) = interest(
                    source,
                    &ports.lifecycle,
                    &ports.gate,
                    &ports.decode,
                    &ports.label,
                    coordinator.as_deref(),
                    cx,
                ) && !events.is_empty()
                {
                    interests.push(WaitInterest { fd, events });
                    sources.push(source);
                }
            }
            // The recorder contributes a descriptor throughout a live context.
            // No-descriptor operation is still useful at the final closed edge.
            if interests.is_empty() && wake.is_none() {
                return Ok(PumpStep::Idle);
            }
            match ports
                .io
                .wait(&interests, wake)
                .map_err(PumpFailure::Clock)?
            {
                WaitOutcome::Timeout => {
                    // Terminal bounds win even when their expiry shares the
                    // tick, or the clock advanced past both during the wait.
                    if let Some(expired) = self.deadline(ports.io, deadlines)? {
                        return Ok(expired);
                    }
                    if observation.is_some_and(|tick| self.previous >= tick) {
                        return Ok(PumpStep::Observation);
                    }
                    return Err(PumpFailure::Clock(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "wait returned timeout before any armed wake",
                    )));
                }
                WaitOutcome::Ready(wakes) => {
                    for wake in wakes {
                        let Some(source) = sources.get(wake.index) else {
                            return Err(PumpFailure::Clock(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "wait returned an unknown descriptor index",
                            )));
                        };
                        ready[*source as usize] |= wake.events;
                    }
                }
            }
        }
        let mut order = Vec::new();
        order.extend(
            LIFECYCLE
                .into_iter()
                .filter(|source| !ready[*source as usize].is_empty()),
        );
        order.extend(
            ROTATING
                .into_iter()
                .filter(|source| is_terminal(ready[*source as usize])),
        );
        for offset in 0..ROTATING.len() {
            let source = ROTATING[(self.cursor + offset) % ROTATING.len()];
            if !ready[source as usize].is_empty() && !order.contains(&source) {
                order.push(source);
            }
        }
        self.cursor = (self.cursor + 1) % ROTATING.len();
        // The final Enter answer observes every standing component before its
        // write. This does not reduce a completion or grant readiness itself.
        if ports
            .lifecycle
            .peer
            .as_deref()
            .is_some_and(LifecyclePeer::enter_ready_pending)
            && order.contains(&PumpSource::Admin)
        {
            order.retain(|source| *source != PumpSource::Admin);
            order.push(PumpSource::Admin);
        }
        for source in order {
            // Withhold only the first Open/placement write for the same held
            // cancellation that the owner observes. Keep receive and terminal
            // observation on the sole socket owner, including a final packet
            // before HUP. Unrelated lifecycle traffic cannot suppress the send.
            if source == PumpSource::Residency
                && ports
                    .lifecycle
                    .residency
                    .as_deref()
                    .is_some_and(|port| port.admit_not_started())
                && ports
                    .lifecycle
                    .peer
                    .as_deref()
                    .is_some_and(|peer| peer.cancels_boot_open())
            {
                ready[source as usize].remove(PollFlags::POLLOUT);
            }
            if source == PumpSource::Decode
                && ports.decode.as_deref().is_some_and(|decode| {
                    decode.open_not_started() || decode.placement_not_started()
                })
                && ports
                    .lifecycle
                    .peer
                    .as_deref()
                    .is_some_and(|peer| peer.cancels_boot_open())
            {
                ready[source as usize].remove(PollFlags::POLLOUT);
            }
            if source == PumpSource::Admin
                && ports
                    .lifecycle
                    .peer
                    .as_deref()
                    .is_some_and(LifecyclePeer::enter_ready_pending)
            {
                if let Some(expired) = self.deadline(ports.io, deadlines)? {
                    return Ok(expired);
                }
                let peer = ports
                    .lifecycle
                    .peer
                    .as_deref_mut()
                    .expect("present original Enter");
                // One bounded receive by the original owner precedes this
                // irrevocable answer. A held cancellation grants no Ready write.
                peer.try_receive()
                    .map_err(|failure| PumpFailure::Transport { source, failure })?;
                let cancelled = peer.cancels_boot_open();
                if let Some(expired) = self.deadline(ports.io, deadlines)? {
                    return Ok(expired);
                }
                if cancelled || !ready[source as usize].contains(PollFlags::POLLOUT) {
                    progressed.push(source);
                    continue;
                }
                // The receive above is this source's read unit. Preserve the
                // actual writable hint; EAGAIN never means a completed answer.
                ready[source as usize].remove(PollFlags::POLLIN);
            }
            if let Some(expired) = self.deadline(ports.io, deadlines)? {
                return Ok(expired);
            }
            if self.unit(
                source,
                ready[source as usize],
                ports,
                coordinator.as_deref_mut(),
                cx,
            )? && !progressed.contains(&source)
            {
                progressed.push(source);
            }
            if let Some(expired) = self.deadline(ports.io, deadlines)? {
                return Ok(expired);
            }
        }
        Ok(PumpStep::Progress {
            sources: progressed,
        })
    }

    /// One nonblocking transport unit for an already-owned stopped Open. Policy,
    /// source identity and the earlier enter/teardown bound remain with its owner.
    pub(super) fn stopped_decode_step(
        &mut self,
        decode: &mut DecodePort,
    ) -> Result<(), super::pump_transport::Failure> {
        let send = self.send_next[PumpSource::Decode as usize] && decode.send_pending();
        self.send_next[PumpSource::Decode as usize] = !send;
        if send {
            decode.try_send()?;
        } else if decode.held().is_none() {
            decode.try_receive()?;
        }
        Ok(())
    }

    fn unit<I: PumpIo>(
        &mut self,
        source: PumpSource,
        flags: PollFlags,
        ports: &mut PumpPorts<'_, I>,
        coordinator: Option<&mut FlowCoordinator<SocketPort>>,
        cx: &mut FlowRecording<'_>,
    ) -> Result<bool, PumpFailure> {
        let send = flags.contains(PollFlags::POLLOUT)
            && !is_terminal(flags)
            && (!flags.contains(PollFlags::POLLIN) || self.send_next[source as usize]);
        self.send_next[source as usize] = !send;
        let transport = |failure| PumpFailure::Transport { source, failure };
        match source {
            PumpSource::Listener => {
                if ports.lifecycle.peer.is_some() || ports.lifecycle.dial.is_some() {
                    return Ok(false);
                }
                let Some(listener) = ports.lifecycle.listener.as_mut() else {
                    return Ok(false);
                };
                if listener.held.is_none() && is_terminal(flags) {
                    return listener
                        .fail(closed_failure("coordination listener closed"))
                        .map_err(transport);
                }
                listener
                    .advance()
                    .map(|progress| {
                        if progress == ListenProgress::Accepted {
                            // The common clock was checked immediately before
                            // this nonblocking accept. Delayed reduction cannot
                            // restart the dial's original receive allowance.
                            listener.accepted_ns = Some(self.previous);
                        }
                        progress != ListenProgress::Pending
                    })
                    .map_err(transport)
            }
            PumpSource::Admin | PumpSource::AdminDial | PumpSource::LeavePeer => {
                let slot = match source {
                    PumpSource::Admin => &mut ports.lifecycle.peer,
                    PumpSource::AdminDial => &mut ports.lifecycle.dial,
                    _ => &mut ports.lifecycle.leave_peer,
                };
                let Some(peer) = slot.as_mut() else {
                    return Ok(false);
                };
                if send {
                    peer.try_send_observed(ports.io)
                        .map(|progress| progress != SendProgress::Pending)
                        .map_err(transport)
                } else {
                    peer.try_receive()
                        .map(|progress| progress != ReceiveProgress::Pending)
                        .map_err(transport)
                }
            }
            PumpSource::Residency => {
                let Some(port) = ports.lifecycle.residency.as_deref_mut() else {
                    return Ok(false);
                };
                if send {
                    port.try_send_observed(ports.io)
                        .map(|progress| progress != SendProgress::Pending)
                        .map_err(transport)
                } else {
                    let was_held = port.held().is_some();
                    port.try_receive()
                        .map(|progress| !was_held && progress != ReceiveProgress::Pending)
                        .map_err(transport)
                }
            }
            PumpSource::Gate => {
                let Some(port) = ports.gate.as_deref_mut() else {
                    return Ok(false);
                };
                if send {
                    port.try_send_observed(ports.io)
                        .map(|progress| progress != super::pump_transport::SendProgress::Pending)
                        .map_err(transport)
                } else {
                    let was_held = port.held().is_some();
                    port.try_receive()
                        .map(|progress| {
                            !was_held && progress != super::pump_transport::ReceiveProgress::Pending
                        })
                        .map_err(transport)
                }
            }
            PumpSource::Decode => packet_unit(ports.decode.as_deref_mut(), send).map_err(transport),
            PumpSource::Label => packet_unit(ports.label.as_deref_mut(), send).map_err(transport),
            PumpSource::State => {
                let Some(owner) = coordinator else {
                    return Ok(false);
                };
                owner
                    .transport_one(cx, ports.io, TransportSource::State)
                    .map_err(PumpFailure::Flow)?;
                Ok(true)
            }
            PumpSource::Classifier => {
                let Some(owner) = coordinator else {
                    return Ok(false);
                };
                if owner.starting_classifier().is_some() {
                    owner
                        .advance_classifier_admission(cx)
                        .map(|result| result != AdmissionAdvance::Pending)
                        .map_err(PumpFailure::Flow)
                } else {
                    owner
                        .transport_one(cx, ports.io, TransportSource::Classifier)
                        .map_err(PumpFailure::Flow)?;
                    Ok(true)
                }
            }
            PumpSource::Trace => cx
                .barrier
                .try_capacity_wake(cx.budget)
                .map(|wake| wake != weaver_trace::serving::CapacityWake::Empty)
                .map_err(PumpFailure::Trace),
        }
    }
}

fn packet_unit<T>(
    port: Option<&mut super::pump_transport::PacketPort<T>>,
    send: bool,
) -> Result<bool, Failure> {
    let Some(port) = port else {
        return Ok(false);
    };
    if send {
        port.try_send()
            .map(|progress| progress != SendProgress::Pending)
    } else {
        let was_held = port.held().is_some();
        port.try_receive()
            .map(|progress| !was_held && progress != ReceiveProgress::Pending)
    }
}

fn present<I: PumpIo>(
    source: PumpSource,
    ports: &PumpPorts<'_, I>,
    coordinator: Option<&FlowCoordinator<SocketPort>>,
) -> bool {
    match source {
        PumpSource::Listener => {
            ports.lifecycle.peer.is_none()
                && ports.lifecycle.dial.is_none()
                && ports.lifecycle.listener.is_some()
        }
        PumpSource::Admin => ports.lifecycle.peer.is_some(),
        PumpSource::AdminDial => ports.lifecycle.dial.is_some(),
        PumpSource::LeavePeer => ports.lifecycle.leave_peer.is_some(),
        PumpSource::Residency => ports.lifecycle.residency.is_some(),
        PumpSource::Gate => ports.gate.is_some(),
        PumpSource::Decode => ports.decode.is_some(),
        PumpSource::Label => ports.label.is_some(),
        PumpSource::State => coordinator.is_some_and(|owner| owner.standing_state().is_some()),
        PumpSource::Classifier => coordinator.is_some_and(|owner| {
            !owner.admission_refused()
                && (owner.standing_classifier().is_some() || owner.starting_classifier().is_some())
        }),
        PumpSource::Trace => true,
    }
}

fn local_work<I: PumpIo>(
    source: PumpSource,
    ports: &PumpPorts<'_, I>,
    coordinator: Option<&FlowCoordinator<SocketPort>>,
    release_ready: bool,
) -> bool {
    match source {
        PumpSource::Listener => {
            ports.lifecycle.peer.is_none()
                && ports.lifecycle.dial.is_none()
                && ports.lifecycle.listener.as_ref().is_some_and(|port| {
                    port.interrupted || port.held.as_ref().is_some_and(|peer| peer.needs_progress())
                })
        }
        PumpSource::Admin => ports
            .lifecycle
            .peer
            .as_ref()
            .is_some_and(|port| port.needs_progress()),
        PumpSource::AdminDial => ports
            .lifecycle
            .dial
            .as_ref()
            .is_some_and(|port| port.needs_progress()),
        PumpSource::LeavePeer => ports
            .lifecycle
            .leave_peer
            .as_ref()
            .is_some_and(|port| port.needs_progress()),
        PumpSource::Residency => ports
            .lifecycle
            .residency
            .as_ref()
            .is_some_and(|port| port.needs_progress()),
        PumpSource::Gate => ports
            .gate
            .as_ref()
            .is_some_and(|port| port.needs_progress()),
        PumpSource::Decode => ports
            .decode
            .as_ref()
            .is_some_and(|port| port.needs_progress()),
        PumpSource::Label => ports
            .label
            .as_ref()
            .is_some_and(|port| port.needs_progress()),
        PumpSource::State => coordinator
            .and_then(|owner| owner.standing_state())
            .is_some_and(|port| port.needs_progress() || port.ready_buffered()),
        PumpSource::Classifier => coordinator.is_some_and(|owner| {
            owner.admission_needs_progress()
                || owner
                    .standing_classifier()
                    .is_some_and(|port| port.needs_progress(release_ready))
                || owner.starting_classifier().is_some_and(|port| {
                    port.needs_progress()
                        && !owner.admission_refused()
                        && owner.held_admission().is_none()
                })
        }),
        _ => false,
    }
}

/// This bounded zero-time observation consumes no byte and owns no timer.
/// Ordinary readiness remains PumpIo's. Only lifecycle readiness and terminal
/// hints bypass a blocking wait while another owner has buffered work.
fn readiness_hints<I: PumpIo>(
    ports: &PumpPorts<'_, I>,
    coordinator: Option<&FlowCoordinator<SocketPort>>,
    cx: &FlowRecording<'_>,
) -> Result<[PollFlags; SOURCE_COUNT], PumpFailure> {
    let mut descriptors = [libc::pollfd {
        fd: -1,
        events: 0,
        revents: 0,
    }; SOURCE_COUNT];
    let mut sources = [PumpSource::Trace; SOURCE_COUNT];
    let mut count = 0;
    for source in LIFECYCLE.into_iter().chain(ROTATING) {
        if let Some((fd, events)) = interest(
            source,
            &ports.lifecycle,
            &ports.gate,
            &ports.decode,
            &ports.label,
            coordinator,
            cx,
        ) {
            if events.is_empty() {
                continue;
            }
            descriptors[count] = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: if LIFECYCLE.contains(&source) {
                    events.bits()
                } else {
                    terminal_flags().bits()
                },
                revents: 0,
            };
            sources[count] = source;
            count += 1;
        }
    }
    let mut ready = [PollFlags::empty(); SOURCE_COUNT];
    // SAFETY: count never exceeds the fixed source set, each entry is writable,
    // every fd is borrowed from a standing owner, and timeout zero cannot wait.
    if unsafe { libc::poll(descriptors.as_mut_ptr(), count as libc::nfds_t, 0) } < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            return Ok(ready);
        }
        return Err(PumpFailure::Clock(error));
    }
    for index in 0..count {
        ready[sources[index] as usize] = PollFlags::from_bits_retain(descriptors[index].revents);
    }
    Ok(ready)
}

fn standing_transport_failure<I: PumpIo>(ports: &PumpPorts<'_, I>) -> Option<PumpFailure> {
    let failures = [
        (
            PumpSource::Listener,
            ports.lifecycle.listener.as_ref().and_then(|port| {
                port.failure()
                    .or_else(|| port.held.as_ref().and_then(|peer| peer.failure()))
            }),
        ),
        (
            PumpSource::Admin,
            ports
                .lifecycle
                .peer
                .as_ref()
                .and_then(|port| port.failure()),
        ),
        (
            PumpSource::AdminDial,
            ports
                .lifecycle
                .dial
                .as_ref()
                .and_then(|port| port.failure()),
        ),
        (
            PumpSource::LeavePeer,
            ports
                .lifecycle
                .leave_peer
                .as_ref()
                .and_then(|port| port.failure()),
        ),
        (
            PumpSource::Residency,
            ports
                .lifecycle
                .residency
                .as_ref()
                .and_then(|port| port.failure()),
        ),
        (
            PumpSource::Gate,
            ports.gate.as_ref().and_then(|port| port.failure()),
        ),
        (
            PumpSource::Decode,
            ports.decode.as_ref().and_then(|port| port.failure()),
        ),
        (
            PumpSource::Label,
            ports.label.as_ref().and_then(|port| port.failure()),
        ),
    ];
    failures.into_iter().find_map(|(source, failure)| {
        failure
            .cloned()
            .map(|failure| PumpFailure::Transport { source, failure })
    })
}

fn interest<'a>(
    source: PumpSource,
    lifecycle: &'a LifecyclePorts<'_>,
    gate: &'a Option<&mut OrganPort>,
    decode: &'a Option<&mut DecodePort>,
    label: &'a Option<&mut LabelPort>,
    coordinator: Option<&'a FlowCoordinator<SocketPort>>,
    cx: &'a FlowRecording<'_>,
) -> Option<(BorrowedFd<'a>, PollFlags)> {
    match source {
        PumpSource::Listener if lifecycle.peer.is_none() && lifecycle.dial.is_none() => {
            lifecycle.listener.as_ref().map(|port| port.interest())
        }
        PumpSource::Admin => lifecycle
            .peer
            .as_ref()
            .map(|port| (port.fd(), port.interest())),
        PumpSource::AdminDial => lifecycle
            .dial
            .as_ref()
            .map(|port| (port.fd(), port.interest())),
        PumpSource::LeavePeer => lifecycle
            .leave_peer
            .as_ref()
            .map(|port| (port.fd(), port.interest())),
        PumpSource::Residency => lifecycle
            .residency
            .as_ref()
            .map(|port| (port.fd(), port.interest().events())),
        PumpSource::Gate => gate
            .as_ref()
            .map(|port| (port.fd(), port.interest().events())),
        PumpSource::Decode => decode
            .as_ref()
            .map(|port| (port.fd(), port.interest().events())),
        PumpSource::Label => label
            .as_ref()
            .map(|port| (port.fd(), port.interest().events())),
        PumpSource::State => coordinator
            .and_then(|owner| owner.standing_state())
            .map(|port| (port.fd(), port.interest().events())),
        PumpSource::Classifier => coordinator.and_then(|owner| {
            if owner.admission_refused()
                || matches!(owner.held_admission(), Some(AdmissionPoll::Refused(_)))
            {
                return None;
            }
            if let Some(port) = owner.starting_classifier() {
                if matches!(owner.held_admission(), Some(AdmissionPoll::Ready(_))) {
                    return Some((port.readiness(), terminal_flags()));
                }
                Some((
                    port.readiness(),
                    PollFlags::POLLIN
                        | terminal_flags()
                        | if port.wants_write() {
                            PollFlags::POLLOUT
                        } else {
                            PollFlags::empty()
                        },
                ))
            } else {
                owner.standing_classifier().map(|port| {
                    (
                        port.readiness(),
                        terminal_flags()
                            | if port.state() != ClassifierState::ResultHeld {
                                PollFlags::POLLIN
                            } else {
                                PollFlags::empty()
                            }
                            | if port.wants_write() {
                                PollFlags::POLLOUT
                            } else {
                                PollFlags::empty()
                            },
                    )
                })
            }
        }),
        PumpSource::Trace => Some((cx.barrier.capacity_fd(), PollFlags::POLLIN)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "pump_tests.rs"]
mod tests;

/// Descriptorless terminal evidence. Received handle roles remain explicit;
/// the actual descriptors close without becoming another run's inputs.
pub(super) struct RetiredLifecycleMessage {
    pub(super) envelope: OrganEnvelope,
    pub(super) had_sink: bool,
    pub(super) had_state: bool,
}
impl LifecycleMessage {
    pub(super) fn retire_failed(self) -> RetiredLifecycleMessage {
        RetiredLifecycleMessage {
            envelope: self.envelope,
            had_sink: self.sink.is_some(),
            had_state: self.state.is_some(),
        }
    }
}
pub(super) struct RetiredLifecyclePeer {
    pub(super) sender: super::pump_transport::PacketPort<OrganEnvelope, ()>,
    pub(super) held: Option<RetiredLifecycleMessage>,
    pub(super) wire: Option<Vec<u8>>,
    pub(super) failure: Option<Failure>,
    pub(super) interrupted: bool,
    pub(super) zero_received: bool,
}
impl LifecyclePeer {
    pub(super) fn retire_failed(self) -> RetiredLifecyclePeer {
        RetiredLifecyclePeer {
            sender: self.sender.retire_failed(),
            held: self.held.map(LifecycleMessage::retire_failed),
            wire: self.wire,
            failure: self.failure,
            interrupted: self.interrupted,
            zero_received: self.zero_received,
        }
    }
}
pub(super) struct RetiredLifecycleListener {
    pub(super) held: Option<RetiredLifecyclePeer>,
    pub(super) accepted_ns: Option<u64>,
    pub(super) failure: Option<Failure>,
    pub(super) interrupted: bool,
}
impl LifecycleListener {
    pub(super) fn retire_failed(self) -> RetiredLifecycleListener {
        self.listener.unlink();
        RetiredLifecycleListener {
            held: self.held.map(LifecyclePeer::retire_failed),
            accepted_ns: self.accepted_ns,
            failure: self.failure,
            interrupted: self.interrupted,
        }
    }
}

#[path = "pump_enter.rs"]
pub(super) mod enter;
