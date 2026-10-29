//! Bounded transport owners for HARNESS-PUMP's existing packet seams.
//!
//! These adapters consume a channel rather than cloning its read handle. Each
//! send or receive call attempts at most one datagram operation, retains partial
//! work, and holds at most one typed completion. The caller owns exchange order,
//! deadlines, rotation, trace authoring and completion reduction. In particular,
//! organ channels have two initiators: incoming traffic is legal while sending.
//!
//! Ordinary organ traffic admits no ancillary handles. The active lifecycle's
//! Enter reader and the dormant foundation Enter receiver each retain their
//! original sink and state descriptors before any organ-port conversion.
//! Decode retains the existing SegmentPreamble wire, not the STE100 framing.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::libc;
use nix::poll::PollFlags;
use nix::sys::socket::{
    MsgFlags, Shutdown, SockType, UnixAddr, getpeername, getsockname, getsockopt, send, shutdown,
    sockopt,
};
use weaver_types::{
    DECODE_MESSAGE_BOUND, LabelDirective, MAX_ENVELOPE_BYTES, OrganEnvelope, SegmentPreamble,
    TokenAnswer, TokenDirective,
};

use super::clock::PumpIo;
use crate::channel::{ClassifyChannel, ClassifyReply, DecodeChannel, DecodeReply, OrganChannel};
use crate::failure::ChannelFault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Failure {
    pub fault: ChannelFault,
    pub detail: String,
    pub errno: Option<i32>,
}

impl Failure {
    fn closed(detail: impl Into<String>) -> Self {
        Self {
            fault: ChannelFault::Closed,
            detail: detail.into(),
            errno: None,
        }
    }
    fn invalid(detail: impl Into<String>) -> Self {
        Self {
            fault: ChannelFault::Undecodable,
            detail: detail.into(),
            errno: None,
        }
    }
    fn truncated(bound: usize) -> Self {
        Self {
            fault: ChannelFault::Truncated { bound },
            detail: "packet exceeds its wire bound".into(),
            errno: None,
        }
    }
    fn io(error: Errno) -> Self {
        Self {
            fault: ChannelFault::Closed,
            detail: error.to_string(),
            errno: Some(error as i32),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Interest {
    pub readable: bool,
    pub writable: bool,
    live: bool,
}

impl Interest {
    /// The pump registers every nonempty set. A full completion slot retains
    /// POLLRDHUP without POLLIN, so a queued second packet cannot spin the pump
    /// while a peer's write-half close still wakes its terminal observation.
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

#[derive(Debug, PartialEq)]
pub(super) enum Stage<T> {
    Queued,
    Busy(T),
}

/// Stable socket-owner identity, independent of its current numeric descriptor.
/// Retaining this identity prevents a later port from matching through fd reuse.
#[derive(Debug, Clone)]
pub(super) struct PortIdentity(Arc<()>);

impl PortIdentity {
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(super) fn matches_received(&self, received: &ReceivedOrgan) -> bool {
        Arc::ptr_eq(&self.0, &received.port_identity().0)
    }
    pub(super) fn matches_decode(&self, port: &DecodePort) -> bool {
        Arc::ptr_eq(&self.0, &port.identity.0)
    }
    pub(super) fn matches(&self, port: &OrganPort) -> bool {
        Arc::ptr_eq(&self.0, &port.identity.0)
    }
}

const SEND_PENDING: u8 = 0;
const SEND_SENT: u8 = 1;
const SEND_OBSERVED: u8 = 2;
const SEND_WITHDRAWN: u8 = 3;

#[derive(Debug)]
struct TrackedSend {
    status: AtomicU8,
    sent_at: AtomicU64,
}

/// One staged packet's transport proof. Only the owning port can change its
/// status, and only a successful whole send can establish Sent. A later socket
/// fault, another stage or an empty outgoing slot cannot manufacture that fact.
#[derive(Debug)]
pub(super) struct OrganSend {
    identity: PortIdentity,
    exchange: weaver_types::ExchangeId,
    tracked: Arc<TrackedSend>,
}

impl OrganSend {
    #[cfg(test)]
    pub(super) fn state_probe_for_test(&self) -> Box<dyn Fn() -> (bool, bool) + Send> {
        let tracked = Arc::clone(&self.tracked);
        Box::new(move || {
            let status = tracked.status.load(Ordering::Acquire);
            (
                matches!(status, SEND_SENT | SEND_OBSERVED),
                status == SEND_WITHDRAWN,
            )
        })
    }
    #[cfg(test)]
    pub(super) fn sent_probe_for_test(&self) -> Box<dyn Fn() -> bool> {
        let tracked = Arc::clone(&self.tracked);
        Box::new(move || {
            matches!(
                tracked.status.load(Ordering::Acquire),
                SEND_SENT | SEND_OBSERVED
            )
        })
    }
    pub(super) fn sent(&self) -> bool {
        matches!(
            self.tracked.status.load(Ordering::Acquire),
            SEND_SENT | SEND_OBSERVED
        )
    }

    /// Raw common-clock nanoseconds observed immediately after the whole send.
    /// Sent with no timestamp means the post-send clock observation failed.
    pub(super) fn sent_at(&self) -> Option<u64> {
        (self.tracked.status.load(Ordering::Acquire) == SEND_OBSERVED)
            .then(|| self.tracked.sent_at.load(Ordering::Relaxed))
    }

    pub(super) fn withdrawn(&self) -> bool {
        self.tracked.status.load(Ordering::Acquire) == SEND_WITHDRAWN
    }

    pub(super) fn belongs_to(&self, port: &OrganPort) -> bool {
        self.identity.matches(port)
    }

    pub(super) fn matches_exchange(&self, exchange: &weaver_types::ExchangeId) -> bool {
        self.exchange == *exchange
    }

    pub(super) fn port_identity(&self) -> PortIdentity {
        self.identity.clone()
    }
}

#[derive(Debug)]
pub(super) enum TrackedStage<T = OrganEnvelope> {
    Queued(OrganSend),
    Busy(T),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeRequestKind {
    Measurement,
    Open,
    Generation,
    Flush,
    Elide,
}

#[derive(Debug)]
struct DecodeReservationState {
    kind: DecodeRequestKind,
    turn: Option<weaver_types::TurnKey>,
    cancel_queued: AtomicBool,
    cancel_sent: AtomicBool,
    cancel_taken: AtomicBool,
    sent: AtomicBool,
    terminal_taken: AtomicBool,
    invalid_reply: AtomicBool,
    finished: AtomicBool,
    abandoned: AtomicBool,
    abandoned_failure: Failure,
}

/// Sole custody of one decode exchange, including its wait after the last
/// request segment. Dropping an unfinished ticket does not unlock the port.
#[derive(Debug)]
pub(super) struct DecodeReservation {
    identity: PortIdentity,
    state: Arc<DecodeReservationState>,
}

impl DecodeReservation {
    pub(super) fn sent(&self) -> bool {
        self.state.sent.load(Ordering::Acquire)
    }

    pub(super) fn belongs_to(&self, port: &DecodePort) -> bool {
        Arc::ptr_eq(&self.identity.0, &port.identity.0)
    }
}

impl Drop for DecodeReservation {
    fn drop(&mut self) {
        if !self.state.finished.load(Ordering::Acquire) {
            self.state.abandoned.store(true, Ordering::Release);
        }
    }
}

#[derive(Debug)]
pub(super) enum DecodeStage {
    Queued(DecodeReservation),
    Busy(TokenDirective),
}

#[derive(Debug)]
pub(super) struct DecodeFinishError {
    pub reservation: DecodeReservation,
    pub failure: Failure,
}

#[derive(Debug)]
struct DecodeReceipt {
    state: Arc<DecodeReservationState>,
    after_send: bool,
    after_cancel: bool,
}

/// A complete reply received by the reserved decoder socket. Its exact bytes
/// and receive-time send status remain attached to the typed view. This is
/// transport evidence, not a successful measurement or placement grant.
#[derive(Debug)]
pub(super) struct ReceivedDecode {
    reply: DecodeReply,
    raw: Vec<u8>,
    identity: PortIdentity,
    receipt: DecodeReceipt,
}

impl ReceivedDecode {
    pub(super) fn reply(&self) -> &DecodeReply {
        &self.reply
    }

    pub(super) fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }

    pub(super) fn belongs_to(&self, port: &DecodePort) -> bool {
        Arc::ptr_eq(&self.identity.0, &port.identity.0)
    }

    pub(super) fn matches(&self, reservation: &DecodeReservation) -> bool {
        Arc::ptr_eq(&self.identity.0, &reservation.identity.0)
            && Arc::ptr_eq(&self.receipt.state, &reservation.state)
    }

    pub(super) fn after_send(&self) -> bool {
        self.receipt.after_send
    }

    pub(super) fn into_parts(self) -> (DecodeReply, Vec<u8>) {
        (self.reply, self.raw)
    }
}

#[derive(Debug)]
pub(super) struct StageFailure<T> {
    pub input: T,
    pub failure: Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReceiveProgress {
    Pending,
    Progress,
    Held,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SendProgress {
    Pending,
    Progress,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    Organ,
    Packet,
    Decode,
}

#[derive(Debug)]
enum ClosedReceive {
    Finished,
    Pending,
    Refused(Failure),
}

#[derive(Debug)]
struct Outbound {
    body: Vec<u8>,
    preamble: Option<Vec<u8>>,
    preamble_sent: bool,
    offset: usize,
    tracked: Option<Arc<TrackedSend>>,
    decode_reservation: Option<Arc<DecodeReservationState>>,
    generation: bool,
    resident_mutation: bool,
    admit: bool,
    cancel: bool,
}

#[derive(Debug)]
struct Series {
    bytes: Vec<u8>,
    total: usize,
    segments: u64,
    received: u64,
}

/// One owner for an existing typed seam. The public aliases below fix the
/// codec and wire election, so callers cannot supply a different framing rule.
pub(super) struct PacketPort<T, E = OwnedFd> {
    end: E,
    identity: PortIdentity,
    wire: Wire,
    decode: fn(&[u8]) -> Result<T, Failure>,
    outgoing: Option<Outbound>,
    series: Option<Series>,
    // The last complete packet is retained on decode failure. A held whole
    // message retains its exact wire bytes until its typed completion is taken.
    received_wire: Option<Vec<u8>>,
    received_reservation: Option<DecodeReceipt>,
    held: Option<T>,
    decode_reservation: Option<Arc<DecodeReservationState>>,
    failure: Option<Failure>,
    receive_interrupted: bool,
    send_interrupted: bool,
    generation_send_held: bool,
    zero_received: bool,
    closed_receive: ClosedReceive,
}

pub(super) type OrganPort = PacketPort<OrganEnvelope>;
pub(super) type DecodePort = PacketPort<DecodeReply>;
pub(super) type LabelPort = PacketPort<ClassifyReply>;

impl PacketPort<OrganEnvelope, ()> {
    /// Retiring the descriptor does not erase an already received tool answer.
    /// No socket read or fabricated decoded envelope occurs on this path.
    pub(super) fn take_retired_tool_answer(&mut self) -> Option<ReceivedOrgan> {
        if !self.held.as_ref().is_some_and(|envelope| {
            matches!(envelope.payload, weaver_types::Payload::ToolAnswer(_))
        }) {
            return None;
        }
        Some(ReceivedOrgan {
            envelope: self.held.take().unwrap(),
            raw: self
                .received_wire
                .take()
                .expect("original decoded datagram"),
            identity: self.identity.clone(),
        })
    }
    /// A consumed port can dispose its exact wholly unsent datagram without
    /// regaining any transport operation. Its original ticket records withdrawal.
    pub(super) fn withdraw_retired_unsent(
        &mut self,
        exchange: &weaver_types::ExchangeId,
    ) -> Result<Option<OrganEnvelope>, Failure> {
        let Some(outbound) = self.outgoing.as_ref() else {
            return Ok(None);
        };
        if outbound.preamble.is_some() || outbound.preamble_sent || outbound.offset != 0 {
            return Err(Failure::invalid(
                "retired withdrawal requires a wholly unsent datagram",
            ));
        }
        let pending = (self.decode)(&outbound.body)?;
        if pending.exchange != *exchange {
            return Ok(None);
        }
        let Some(tracked) = &outbound.tracked else {
            return Err(Failure::invalid(
                "retired withdrawal requires its original send ticket",
            ));
        };
        if tracked.status.load(Ordering::Acquire) != SEND_PENDING {
            return Err(Failure::invalid(
                "retired withdrawal cannot replace a sent ticket",
            ));
        }
        let tracked = self.outgoing.take().unwrap().tracked.unwrap();
        tracked.status.store(SEND_WITHDRAWN, Ordering::Release);
        self.send_interrupted = false;
        Ok(Some(pending))
    }
}

/// One actual organ datagram, with its decoded view and original octets held
/// together. Only the owning socket port constructs this fact. It is not a
/// matched tool outcome, a filed pair, or permission to dispatch anything.
#[derive(Debug)]
pub(super) struct ReceivedOrgan {
    envelope: OrganEnvelope,
    raw: Vec<u8>,
    identity: PortIdentity,
}

impl ReceivedOrgan {
    pub(super) fn envelope(&self) -> &OrganEnvelope {
        &self.envelope
    }

    pub(super) fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }

    pub(super) fn port_identity(&self) -> PortIdentity {
        self.identity.clone()
    }

    pub(super) fn belongs_to<End>(&self, port: &PacketPort<OrganEnvelope, End>) -> bool {
        Arc::ptr_eq(&self.identity.0, &port.identity.0)
    }

    pub(super) fn into_parts(self) -> (OrganEnvelope, Vec<u8>) {
        (self.envelope, self.raw)
    }
}

impl OrganPort {
    pub(super) fn idle_for_diagnostic(&self) -> bool {
        self.outgoing.is_none()
            && self.series.is_none()
            && self.received_wire.is_none()
            && self.received_reservation.is_none()
            && self.held.is_none()
            && self.decode_reservation.is_none()
            && self.failure.is_none()
            && !self.zero_received
            && matches!(self.closed_receive, ClosedReceive::Finished)
    }

    pub(super) fn into_diagnostic_fd(self) -> OwnedFd {
        assert!(self.idle_for_diagnostic(), "prepared Diagnostic transport");
        self.end
    }

    pub(super) fn admit_not_started(&self) -> bool {
        self.outgoing
            .as_ref()
            .is_some_and(|out| out.admit && out.offset == 0 && !out.preamble_sent)
    }

    pub(super) fn port_identity(&self) -> PortIdentity {
        self.identity.clone()
    }

    pub(super) fn from_channel(channel: OrganChannel) -> Result<Self, Failure> {
        Self::from_owned(channel.into_fd())
    }
    pub(super) fn from_owned(end: OwnedFd) -> Result<Self, Failure> {
        Self::new(end, Wire::Organ, |bytes| {
            serde_json::from_slice(bytes).map_err(|error| Failure::invalid(error.to_string()))
        })
    }
    // Failure returns the intact owned envelope without a second allocation on
    // an already failing path. Boxing this 216-byte error would weaken that.
    #[allow(clippy::result_large_err)]
    pub(super) fn stage(
        &mut self,
        input: OrganEnvelope,
    ) -> Result<Stage<OrganEnvelope>, StageFailure<OrganEnvelope>> {
        let admit = matches!(
            &input.payload,
            weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Admit { .. })
        );
        let result = self.stage_encoded(input, serde_json::to_vec)?;
        if matches!(result, Stage::Queued) {
            self.outgoing
                .as_mut()
                .expect("queued organ directive")
                .admit = admit;
        }
        Ok(result)
    }

    /// Issue a sealed proof for exactly this stage, without changing the
    /// existing untracked API used by other seam owners.
    #[allow(clippy::result_large_err)]
    pub(super) fn stage_tracked(
        &mut self,
        input: OrganEnvelope,
    ) -> Result<TrackedStage, StageFailure<OrganEnvelope>> {
        let exchange = input.exchange.clone();
        match self.stage(input)? {
            Stage::Queued => Ok(TrackedStage::Queued(self.track_staged(exchange))),
            Stage::Busy(input) => Ok(TrackedStage::Busy(input)),
        }
    }

    fn track_staged(&mut self, exchange: weaver_types::ExchangeId) -> OrganSend {
        let tracked = Arc::new(TrackedSend {
            status: AtomicU8::new(SEND_PENDING),
            sent_at: AtomicU64::new(0),
        });
        self.outgoing.as_mut().expect("queued packet").tracked = Some(Arc::clone(&tracked));
        OrganSend {
            identity: self.identity.clone(),
            exchange,
            tracked,
        }
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn stage_observation(
        &mut self,
        input: weaver_types::state_flow::enter::ObservationEnvelope,
    ) -> Result<
        TrackedStage<weaver_types::state_flow::enter::ObservationEnvelope>,
        StageFailure<weaver_types::state_flow::enter::ObservationEnvelope>,
    > {
        let bytes = match input.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                let failure = match error {
                    weaver_types::state_flow::enter::CodecError::TooLarge => {
                        Failure::truncated(MAX_ENVELOPE_BYTES)
                    }
                    _ => Failure::invalid(format!("replacement observation: {error:?}")),
                };
                return Err(StageFailure {
                    input,
                    failure: self.fail(failure),
                });
            }
        };
        let exchange = input.exchange.clone();
        match self.stage_encoded(input, |_| Ok(bytes))? {
            Stage::Queued => Ok(TrackedStage::Queued(self.track_staged(exchange))),
            Stage::Busy(input) => Ok(TrackedStage::Busy(input)),
        }
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn stage_capture(
        &mut self,
        input: weaver_types::state_flow::capture::CaptureEnvelope,
    ) -> Result<
        TrackedStage<weaver_types::state_flow::capture::CaptureEnvelope>,
        StageFailure<weaver_types::state_flow::capture::CaptureEnvelope>,
    > {
        use weaver_types::state_flow::capture::{CaptureCodecError, CaptureSender};
        let bytes = match input.encode(CaptureSender::Harness) {
            Ok(bytes) => bytes,
            Err(error) => {
                let failure = match error {
                    CaptureCodecError::TooLarge => Failure::truncated(MAX_ENVELOPE_BYTES),
                    _ => Failure::invalid(format!("replacement capture: {error}")),
                };
                return Err(StageFailure {
                    input,
                    failure: self.fail(failure),
                });
            }
        };
        let exchange = input.exchange.clone();
        match self.stage_encoded(input, |_| Ok(bytes))? {
            Stage::Queued => Ok(TrackedStage::Queued(self.track_staged(exchange))),
            Stage::Busy(input) => Ok(TrackedStage::Busy(input)),
        }
    }

    /// Withdraw only this exact exchange's wholly unsent organ datagram. A
    /// successful SEQPACKET send removes it atomically, so absence after send
    /// never becomes a claim that the operation was not dispatched. A failed
    /// transport keeps all outgoing evidence and refuses withdrawal.
    pub(super) fn withdraw_pending(
        &mut self,
        exchange: &weaver_types::ExchangeId,
    ) -> Result<Option<OrganEnvelope>, Failure> {
        self.live()?;
        let Some(outbound) = self.outgoing.as_ref() else {
            return Ok(None);
        };
        if outbound.preamble.is_some() || outbound.preamble_sent || outbound.offset != 0 {
            return Err(self.fail(Failure::invalid(
                "organ withdrawal requires a wholly unsent datagram",
            )));
        }
        let pending = (self.decode)(&outbound.body).map_err(|failure| self.fail(failure))?;
        if pending.exchange != *exchange {
            return Ok(None);
        }
        if let Some(tracked) = self.outgoing.take().and_then(|outbound| outbound.tracked) {
            tracked.status.store(SEND_WITHDRAWN, Ordering::Release);
        }
        self.send_interrupted = false;
        Ok(Some(pending))
    }

    /// The paired projection keeps its exact port-minted send witness. It may
    /// withdraw only that still-unsent datagram, never a matching scalar alone.
    pub(super) fn withdraw_capture_pending(
        &mut self,
        send: &OrganSend,
    ) -> Result<Option<weaver_types::state_flow::capture::CaptureEnvelope>, Failure> {
        use weaver_types::state_flow::capture::{CaptureEnvelope, CaptureSender};
        // Closing the peer cannot authorize a Commit that is still wholly
        // unsent. Return that exact ticket's bytes while retaining the failure.
        if !self
            .failure
            .as_ref()
            .is_some_and(|failure| failure.fault == ChannelFault::Closed)
        {
            self.live()?;
        }
        let Some(outbound) = self.outgoing.as_ref() else {
            return Ok(None);
        };
        if !send.belongs_to(self)
            || outbound
                .tracked
                .as_ref()
                .is_none_or(|tracked| !Arc::ptr_eq(tracked, &send.tracked))
        {
            return Ok(None);
        }
        if outbound.preamble.is_some() || outbound.preamble_sent || outbound.offset != 0 {
            return Err(self.fail(Failure::invalid(
                "capture withdrawal requires a wholly unsent datagram",
            )));
        }
        let original = CaptureEnvelope::decode(&outbound.body, CaptureSender::Harness)
            .map_err(|error| self.fail(Failure::invalid(format!("capture withdrawal: {error}"))))?;
        if !send.matches_exchange(&original.exchange) || send.sent() {
            return Err(self.fail(Failure::invalid(
                "capture withdrawal changed its original ticket",
            )));
        }
        self.outgoing
            .take()
            .unwrap()
            .tracked
            .unwrap()
            .status
            .store(SEND_WITHDRAWN, Ordering::Release);
        self.send_interrupted = false;
        Ok(Some(original))
    }

    /// Closed serving transport can return its exact wholly unsent datagram.
    /// The port-minted ticket is required; its failure and received evidence stay.
    pub(super) fn withdraw_closed_pending(
        &mut self,
        send: &OrganSend,
    ) -> Result<Option<OrganEnvelope>, Failure> {
        if !self
            .failure
            .as_ref()
            .is_some_and(|failure| failure.fault == ChannelFault::Closed)
            || !send.belongs_to(self)
        {
            return Err(Failure::invalid(
                "closed withdrawal requires its actual port and ticket",
            ));
        }
        let Some(outbound) = self.outgoing.as_ref() else {
            return Ok(None);
        };
        if outbound
            .tracked
            .as_ref()
            .is_none_or(|tracked| !Arc::ptr_eq(tracked, &send.tracked))
        {
            return Err(Failure::invalid(
                "closed withdrawal cannot consume another send",
            ));
        }
        if outbound.preamble.is_some()
            || outbound.preamble_sent
            || outbound.offset != 0
            || send.sent()
            || send.withdrawn()
        {
            return Err(Failure::invalid(
                "closed withdrawal requires a wholly unsent datagram",
            ));
        }
        let original = (self.decode)(&outbound.body)?;
        if !send.matches_exchange(&original.exchange) {
            return Err(Failure::invalid(
                "closed withdrawal changed its original exchange",
            ));
        }
        self.outgoing
            .take()
            .unwrap()
            .tracked
            .unwrap()
            .status
            .store(SEND_WITHDRAWN, Ordering::Release);
        self.send_interrupted = false;
        Ok(Some(original))
    }

    /// Transfer the typed envelope and exact received datagram atomically.
    /// A later EOF does not erase a completion already held. Neither taking
    /// that completion nor its raw evidence clears the terminal transport fault.
    pub(super) fn take_received(&mut self) -> Result<Option<ReceivedOrgan>, Failure> {
        if let Some(envelope) = self.held.take() {
            let raw = self
                .received_wire
                .take()
                .expect("a decoded organ completion retains its exact datagram");
            return Ok(Some(ReceivedOrgan {
                envelope,
                raw,
                identity: self.identity.clone(),
            }));
        }
        self.live()?;
        Ok(None)
    }

    /// Closure freezes ordinary I/O, but a reset can precede packets already
    /// queued by this peer. Their original owner must reduce them before
    /// assigning a missing result. This never reopens admission or sending.
    pub(super) fn closed_receive_pending(&self) -> bool {
        matches!(self.closed_receive, ClosedReceive::Pending)
    }

    /// One bounded receive into the same single completion slot. The caller
    /// owns the applicable deadline and the disposition of each original.
    pub(super) fn receive_closed(&mut self) -> Result<ReceiveProgress, Failure> {
        self.receive_closed_with(receive_packet)
    }

    fn receive_closed_with(
        &mut self,
        receive: impl FnOnce(RawFd) -> Result<Packet, ReadFault>,
    ) -> Result<ReceiveProgress, Failure> {
        if !self
            .failure()
            .is_some_and(|failure| failure.fault == ChannelFault::Closed)
        {
            return Err(Failure::invalid(
                "queued receive requires a closed organ port",
            ));
        }
        if let ClosedReceive::Refused(failure) = &self.closed_receive {
            return Err(failure.clone());
        }
        if self.held.is_some() {
            return Ok(ReceiveProgress::Held);
        }
        if !self.closed_receive_pending() {
            return Ok(ReceiveProgress::Pending);
        }
        let packet = match receive(self.end.as_raw_fd()) {
            Ok(packet) => packet,
            Err(ReadFault::Interrupted) => return Ok(ReceiveProgress::Pending),
            Err(ReadFault::Pending) => {
                self.closed_receive = ClosedReceive::Finished;
                return Ok(ReceiveProgress::Pending);
            }
            Err(ReadFault::Failed {
                failure,
                received_wire,
            }) => {
                // A send or status probe can observe closure before recvmsg
                // consumes its pending reset. Retry on a later bounded step.
                if failure.errno == Some(libc::ECONNRESET) && received_wire.is_none() {
                    return Ok(ReceiveProgress::Pending);
                }
                self.received_wire = received_wire;
                if failure.fault == ChannelFault::Closed {
                    self.closed_receive = ClosedReceive::Finished;
                } else {
                    self.closed_receive = ClosedReceive::Refused(failure.clone());
                }
                return Err(failure);
            }
        };
        if packet.bytes.is_empty() {
            self.closed_receive = ClosedReceive::Finished;
            return Ok(ReceiveProgress::Pending);
        }
        self.received_wire = Some(packet.bytes);
        self.accept_packet().inspect_err(|failure| {
            // Keep the first transport failure and the malformed tail's raw
            // evidence. Its distinct refusal must reach the owning caller.
            self.closed_receive = ClosedReceive::Refused(failure.clone());
        })
    }

    /// Available malformed packet bytes, or a truncated packet's received
    /// prefix. A valid held completion keeps its bytes for take_received even
    /// if the peer later closes. The fault itself remains owned by this port.
    pub(super) fn failed_wire(&self) -> Option<&[u8]> {
        if self.failure.is_some() && self.held.is_none() {
            self.received_wire.as_deref()
        } else {
            None
        }
    }

    /// Take malformed evidence once without thawing the failed transport or
    /// claiming that arbitrary received octets are a JSON tool return.
    pub(super) fn take_failed_wire(&mut self) -> Option<Vec<u8>> {
        if self.failure.is_some() && self.held.is_none() {
            self.received_wire.take()
        } else {
            None
        }
    }

    /// A tracked send must use the same clock as its decision bound. The
    /// timestamp records syscall completion, never a later consumer's probe.
    pub(super) fn try_send_observed(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<SendProgress, Failure> {
        self.send_with_observation(send, Some(io))
    }
}

impl DecodePort {
    /// Lifecycle can retain an idle organ loss without discarding an owed
    /// exchange. A reservation, partial message or held reply still has an owner.
    pub(super) fn has_no_exchange(&self) -> bool {
        self.outgoing.is_none()
            && self.series.is_none()
            && self.received_wire.is_none()
            && self.held.is_none()
            && self.decode_reservation.is_none()
    }

    pub(super) fn port_identity(&self) -> PortIdentity {
        self.identity.clone()
    }
    pub(super) fn from_channel(channel: DecodeChannel) -> Result<Self, Failure> {
        Self::from_owned(channel.into_fd())
    }
    pub(super) fn from_owned(end: OwnedFd) -> Result<Self, Failure> {
        Self::new(end, Wire::Decode, |bytes| {
            if let Ok(answer) = serde_json::from_slice(bytes) {
                return Ok(DecodeReply::Answer(answer));
            }
            serde_json::from_slice(bytes)
                .map(DecodeReply::Refusal)
                .map_err(|error| Failure::invalid(error.to_string()))
        })
    }
    // Return the exact proposal without allocating on the failure path.
    #[allow(clippy::result_large_err)]
    pub(super) fn stage(
        &mut self,
        input: TokenDirective,
    ) -> Result<Stage<TokenDirective>, StageFailure<TokenDirective>> {
        if self.decode_reservation.is_none() && matches!(input, TokenDirective::MeasureDelta { .. })
        {
            return Err(StageFailure {
                input,
                failure: Failure::invalid("measurement requires an owned decode reservation"),
            });
        }
        let generation = matches!(input, TokenDirective::AppendAndGenerate { .. });
        let mutation = matches!(
            input,
            TokenDirective::Open { .. }
                | TokenDirective::AppendAndGenerate { .. }
                | TokenDirective::Flush { .. }
                | TokenDirective::Elide { .. }
                | TokenDirective::ReFeed { .. }
        );
        let result = self.stage_encoded(input, serde_json::to_vec)?;
        if matches!(result, Stage::Queued) {
            let outgoing = self.outgoing.as_mut().expect("queued decode directive");
            outgoing.generation = generation;
            outgoing.resident_mutation = mutation;
        }
        Ok(result)
    }

    /// Inspect only this staged directive's pre-first-byte state. Cancellation
    /// and draining an earlier exchange remain transport duties, not new work.
    pub(super) fn generation_not_started(&self) -> bool {
        self.outgoing
            .as_ref()
            .is_some_and(|out| out.generation && out.offset == 0 && !out.preamble_sent)
    }

    /// Scoped transport scheduling only. A hold affects no received evidence,
    /// failure or already-started exchange and grants no cancellation authority.
    /// Return the prior value so the common-pump caller restores it on all returns.
    pub(super) fn replace_generation_send_hold(&mut self, held: bool) -> bool {
        std::mem::replace(&mut self.generation_send_held, held)
    }

    /// Reserve and stage atomically. Existing buffered work belongs to its
    /// earlier caller and cannot become this exchange's evidence. The engine
    /// must first settle earlier exchanges. This transport does not certify their
    /// generation/cancellation protocol or quiescence. Measurement and the dormant
    /// boot Open owner both use this interface; existing serving Open stays held
    /// for the coupled runtime migration.
    #[allow(clippy::result_large_err)] // Refusal preserves the original owned proposal.
    pub(super) fn stage_reserved(
        &mut self,
        input: TokenDirective,
    ) -> Result<DecodeStage, StageFailure<TokenDirective>> {
        if !matches!(
            input,
            TokenDirective::MeasureDelta { .. } | TokenDirective::Open { .. }
        ) {
            return Err(StageFailure {
                input,
                failure: Failure::invalid(
                    "only measurement or boot Open has a reserved decode owner",
                ),
            });
        }
        self.stage_decode_owned(input)
    }

    #[allow(clippy::result_large_err)]
    pub(super) fn stage_placement(
        &mut self,
        input: TokenDirective,
    ) -> Result<DecodeStage, StageFailure<TokenDirective>> {
        if !matches!(
            input,
            TokenDirective::AppendAndGenerate { .. }
                | TokenDirective::Flush { .. }
                | TokenDirective::Elide { .. }
        ) {
            return Err(StageFailure {
                input,
                failure: Failure::invalid("placement reservation requires append or cut"),
            });
        }
        self.stage_decode_owned(input)
    }

    #[allow(clippy::result_large_err)]
    fn stage_decode_owned(
        &mut self,
        input: TokenDirective,
    ) -> Result<DecodeStage, StageFailure<TokenDirective>> {
        if let Err(failure) = self.live() {
            return Err(StageFailure { input, failure });
        }
        if self.decode_reservation.is_some()
            || self.outgoing.is_some()
            || self.held.is_some()
            || self.series.is_some()
            || self.received_wire.is_some()
            || self.received_reservation.is_some()
            || self.receive_interrupted
            || self.send_interrupted
            || self.zero_received
        {
            return Ok(DecodeStage::Busy(input));
        }
        let (kind, turn) = match &input {
            TokenDirective::Open { .. } => (DecodeRequestKind::Open, None),
            TokenDirective::MeasureDelta { .. } => (DecodeRequestKind::Measurement, None),
            TokenDirective::AppendAndGenerate { turn, .. } => {
                (DecodeRequestKind::Generation, Some(turn.clone()))
            }
            TokenDirective::Flush { .. } => (DecodeRequestKind::Flush, None),
            TokenDirective::Elide { .. } => (DecodeRequestKind::Elide, None),
            _ => unreachable!("private callers restrict directive kinds"),
        };
        match self.stage_encoded(input, serde_json::to_vec)? {
            Stage::Busy(input) => Ok(DecodeStage::Busy(input)),
            Stage::Queued => {
                let state = Arc::new(DecodeReservationState {
                    kind,
                    turn,
                    cancel_queued: AtomicBool::new(false),
                    cancel_sent: AtomicBool::new(false),
                    cancel_taken: AtomicBool::new(false),
                    sent: AtomicBool::new(false),
                    terminal_taken: AtomicBool::new(false),
                    invalid_reply: AtomicBool::new(false),
                    finished: AtomicBool::new(false),
                    abandoned: AtomicBool::new(false),
                    abandoned_failure: Failure::invalid("decode reservation was abandoned"),
                });
                self.outgoing
                    .as_mut()
                    .expect("reserved request was queued")
                    .decode_reservation = Some(Arc::clone(&state));
                self.outgoing.as_mut().unwrap().generation = kind == DecodeRequestKind::Generation;
                self.outgoing.as_mut().unwrap().resident_mutation = matches!(
                    kind,
                    DecodeRequestKind::Open
                        | DecodeRequestKind::Generation
                        | DecodeRequestKind::Flush
                        | DecodeRequestKind::Elide
                );
                self.decode_reservation = Some(Arc::clone(&state));
                Ok(DecodeStage::Queued(DecodeReservation {
                    identity: self.identity.clone(),
                    state,
                }))
            }
        }
    }

    pub(super) fn reservation_invalid(&self, reservation: &DecodeReservation) -> bool {
        self.check_reservation(reservation).is_err()
            || reservation.state.invalid_reply.load(Ordering::Acquire)
    }
    pub(super) fn reservation_terminal(&self, reservation: &DecodeReservation) -> bool {
        self.check_reservation(reservation).is_ok()
            && reservation.state.terminal_taken.load(Ordering::Acquire)
    }
    pub(super) fn reservation_settled(&self, reservation: &DecodeReservation) -> bool {
        self.reservation_terminal(reservation)
            && !self.reservation_invalid(reservation)
            && (!reservation.state.cancel_queued.load(Ordering::Acquire)
                || reservation.state.cancel_taken.load(Ordering::Acquire))
    }
    pub(super) fn send_started(&self) -> bool {
        self.outgoing
            .as_ref()
            .is_none_or(|out| out.offset != 0 || out.preamble_sent)
    }
    pub(super) fn unowned_mutation_not_started(&self) -> bool {
        self.decode_reservation.is_none()
            && self
                .outgoing
                .as_ref()
                .is_some_and(|out| out.resident_mutation && out.offset == 0 && !out.preamble_sent)
    }
    pub(super) fn placement_not_started(&self) -> bool {
        self.decode_reservation.as_ref().is_some_and(|state| {
            matches!(
                state.kind,
                DecodeRequestKind::Generation | DecodeRequestKind::Flush | DecodeRequestKind::Elide
            )
        }) && self
            .outgoing
            .as_ref()
            .is_some_and(|out| !out.cancel && out.offset == 0 && !out.preamble_sent)
    }
    /// The secondary Cancel shares the original generation reservation and may
    /// never become a new reader or be followed by a new request before AtRest.
    #[allow(clippy::result_large_err)]
    pub(super) fn cancel_reserved(
        &mut self,
        reservation: &DecodeReservation,
        turn: weaver_types::TurnKey,
    ) -> Result<Stage<TokenDirective>, StageFailure<TokenDirective>> {
        let input = TokenDirective::Cancel { turn: turn.clone() };
        if let Err(failure) = self.check_reservation(reservation) {
            return Err(StageFailure { input, failure });
        }
        if reservation.state.kind != DecodeRequestKind::Generation
            || reservation.state.turn.as_ref() != Some(&turn)
            || !reservation.sent()
            || reservation.state.cancel_queued.load(Ordering::Acquire)
        {
            return Err(StageFailure {
                input,
                failure: Failure::invalid("Cancel does not own a sent generation"),
            });
        }
        if self.outgoing.is_some() {
            return Ok(Stage::Busy(input));
        }
        let held = self.decode_reservation.take();
        let result = self.stage_encoded(input, serde_json::to_vec);
        self.decode_reservation = held;
        if matches!(result, Ok(Stage::Queued)) {
            let outgoing = self.outgoing.as_mut().unwrap();
            outgoing.cancel = true;
            outgoing.decode_reservation = Some(Arc::clone(&reservation.state));
            reservation
                .state
                .cancel_queued
                .store(true, Ordering::Release);
        }
        result
    }

    /// A queued boot request is held behind lifecycle reduction before its first byte.
    pub(super) fn open_not_started(&self) -> bool {
        self.decode_reservation
            .as_ref()
            .is_some_and(|state| state.kind == DecodeRequestKind::Open)
            && self
                .outgoing
                .as_ref()
                .is_some_and(|out| out.offset == 0 && !out.preamble_sent)
    }

    /// Cancellation may withdraw only a request for which no byte or preamble left.
    pub(super) fn withdraw_unsent(
        &mut self,
        reservation: DecodeReservation,
    ) -> Result<(), DecodeFinishError> {
        let failure = self.check_reservation(&reservation).err().or_else(|| {
            (!self
                .outgoing
                .as_ref()
                .is_some_and(|out| out.offset == 0 && !out.preamble_sent)
                || reservation.sent()
                || self.held.is_some()
                || self.series.is_some()
                || self.received_wire.is_some()
                || self.received_reservation.is_some()
                || self.receive_interrupted
                || self.zero_received)
                .then(|| Failure::invalid("decode request is not wholly unsent"))
        });
        if let Some(failure) = failure {
            return Err(DecodeFinishError {
                reservation,
                failure,
            });
        }
        self.outgoing = None;
        self.send_interrupted = false;
        reservation.state.finished.store(true, Ordering::Release);
        self.decode_reservation = None;
        Ok(())
    }

    fn check_reservation(&self, reservation: &DecodeReservation) -> Result<(), Failure> {
        if !reservation.belongs_to(self)
            || self
                .decode_reservation
                .as_ref()
                .is_none_or(|held| !Arc::ptr_eq(held, &reservation.state))
            || reservation.state.finished.load(Ordering::Acquire)
            || reservation.state.abandoned.load(Ordering::Acquire)
        {
            return Err(Failure::invalid(
                "decode reservation does not own this exchange",
            ));
        }
        Ok(())
    }

    /// Taking a fault emission retains the exchange. A matching terminal
    /// received after the whole request is the only proof that permits finish.
    pub(super) fn take_reserved(
        &mut self,
        reservation: &DecodeReservation,
    ) -> Result<Option<ReceivedDecode>, Failure> {
        self.check_reservation(reservation)?;
        if self.held.is_some() {
            let receipt = self
                .received_reservation
                .as_ref()
                .ok_or_else(|| Failure::invalid("decode reply has no reserved receive evidence"))?;
            if !Arc::ptr_eq(&receipt.state, &reservation.state) {
                return Err(Failure::invalid(
                    "decode reply belongs to another reservation",
                ));
            }
            let reply = self.held.take().expect("held reply checked");
            let receipt = self.received_reservation.take().expect("receipt checked");
            let raw = self
                .received_wire
                .take()
                .expect("complete decode reply retains its bytes");
            match &reply {
                DecodeReply::Answer(TokenAnswer::Fault(_))
                    if matches!(
                        reservation.state.kind,
                        DecodeRequestKind::Open | DecodeRequestKind::Measurement
                    ) => {}
                DecodeReply::Answer(
                    TokenAnswer::Token { .. }
                    | TokenAnswer::Field { .. }
                    | TokenAnswer::Column { .. },
                ) if reservation.state.kind == DecodeRequestKind::Generation
                    && receipt.after_send
                    && !reservation.state.terminal_taken.load(Ordering::Acquire) => {}
                DecodeReply::Answer(TokenAnswer::AtRest)
                    if reservation.state.kind == DecodeRequestKind::Generation
                        && receipt.after_cancel
                        && reservation.state.terminal_taken.load(Ordering::Acquire)
                        && reservation.state.cancel_queued.load(Ordering::Acquire)
                        && !reservation.state.cancel_taken.load(Ordering::Acquire) =>
                {
                    reservation
                        .state
                        .cancel_taken
                        .store(true, Ordering::Release);
                }
                reply
                    if receipt.after_send
                        && !reservation.state.terminal_taken.load(Ordering::Acquire)
                        && matches!(
                            (reservation.state.kind, reply),
                            (_, DecodeReply::Refusal(_))
                                | (
                                    DecodeRequestKind::Measurement,
                                    DecodeReply::Answer(TokenAnswer::DeltaMeasured { .. })
                                )
                                | (
                                    DecodeRequestKind::Open,
                                    DecodeReply::Answer(TokenAnswer::Opened(_))
                                )
                                | (
                                    DecodeRequestKind::Generation,
                                    DecodeReply::Answer(TokenAnswer::Generated(_))
                                )
                                | (
                                    DecodeRequestKind::Flush,
                                    DecodeReply::Answer(TokenAnswer::Flushed { .. })
                                )
                                | (
                                    DecodeRequestKind::Elide,
                                    DecodeReply::Answer(TokenAnswer::Elided { .. })
                                )
                        ) =>
                {
                    reservation
                        .state
                        .terminal_taken
                        .store(true, Ordering::Release);
                }
                _ => {
                    reservation
                        .state
                        .invalid_reply
                        .store(true, Ordering::Release);
                }
            }
            return Ok(Some(ReceivedDecode {
                reply,
                raw,
                identity: self.identity.clone(),
                receipt,
            }));
        }
        self.live()?;
        Ok(None)
    }

    /// Failure returns the still-live ticket. In particular, an empty send
    /// slot, a fault emission or a partially transmitted request cannot unlock
    /// the decoder. A later EOF remains latched after valid terminal cleanup.
    pub(super) fn finish_reserved(
        &mut self,
        reservation: DecodeReservation,
    ) -> Result<(), DecodeFinishError> {
        let refusal = self.check_reservation(&reservation).err().or_else(|| {
            (!reservation.sent()
                || !reservation.state.terminal_taken.load(Ordering::Acquire)
                || reservation.state.invalid_reply.load(Ordering::Acquire)
                || (reservation.state.cancel_queued.load(Ordering::Acquire)
                    && !reservation.state.cancel_taken.load(Ordering::Acquire))
                || self.outgoing.is_some()
                || self.held.is_some()
                || self.series.is_some()
                || self.received_wire.is_some()
                || self.received_reservation.is_some()
                || self.receive_interrupted
                || self.send_interrupted
                || (self.zero_received && self.failure.is_none()))
            .then(|| Failure::invalid("decode reservation has unsettled transport work"))
        });
        if let Some(failure) = refusal {
            return Err(DecodeFinishError {
                reservation,
                failure,
            });
        }
        reservation.state.finished.store(true, Ordering::Release);
        self.decode_reservation = None;
        Ok(())
    }

    pub(super) fn failed_wire_reserved(
        &self,
        reservation: &DecodeReservation,
    ) -> Result<Option<&[u8]>, Failure> {
        self.check_reservation(reservation)?;
        Ok(if self.failure().is_some() && self.held.is_none() {
            self.received_wire.as_deref()
        } else {
            None
        })
    }

    pub(super) fn take_failed_wire_reserved(
        &mut self,
        reservation: &DecodeReservation,
    ) -> Result<Option<Vec<u8>>, Failure> {
        self.check_reservation(reservation)?;
        Ok(if self.failure().is_some() && self.held.is_none() {
            self.received_wire.take()
        } else {
            None
        })
    }
}

impl LabelPort {
    pub(super) fn from_channel(channel: ClassifyChannel) -> Result<Self, Failure> {
        let end = channel.into_pump_fd().map_err(|_retired| {
            Failure::closed("retired label channel cannot acquire a new transport owner")
        })?;
        Self::from_owned(end)
    }
    pub(super) fn from_owned(end: OwnedFd) -> Result<Self, Failure> {
        Self::new(end, Wire::Packet, |bytes| {
            if let Ok(answer) = serde_json::from_slice(bytes) {
                return Ok(ClassifyReply::Answer(answer));
            }
            serde_json::from_slice(bytes)
                .map(ClassifyReply::Refusal)
                .map_err(|error| Failure::invalid(error.to_string()))
        })
    }
    pub(super) fn stage(
        &mut self,
        input: LabelDirective,
    ) -> Result<Stage<LabelDirective>, StageFailure<LabelDirective>> {
        self.stage_encoded(input, serde_json::to_vec)
    }
}

impl<T> PacketPort<T> {
    /// Consume only the fd. The original sent/partial/held/failure evidence
    /// remains move-owned in a carrier with no transport methods.
    pub(super) fn retire_failed(self) -> PacketPort<T, ()> {
        let Self {
            end,
            identity,
            wire,
            decode,
            outgoing,
            series,
            received_wire,
            received_reservation,
            held,
            decode_reservation,
            failure,
            receive_interrupted,
            send_interrupted,
            generation_send_held,
            zero_received,
            closed_receive,
        } = self;
        drop(end);
        PacketPort {
            end: (),
            identity,
            wire,
            decode,
            outgoing,
            series,
            received_wire,
            received_reservation,
            held,
            decode_reservation,
            failure,
            receive_interrupted,
            send_interrupted,
            generation_send_held,
            zero_received,
            closed_receive,
        }
    }

    fn new(
        end: OwnedFd,
        wire: Wire,
        decode: fn(&[u8]) -> Result<T, Failure>,
    ) -> Result<Self, Failure> {
        if getsockopt(&end, sockopt::SockType).map_err(Failure::io)? != SockType::SeqPacket
            || getsockname::<UnixAddr>(end.as_raw_fd()).is_err()
            || getpeername::<UnixAddr>(end.as_raw_fd()).is_err()
        {
            return Err(Failure::closed(
                "transport requires a connected Unix SEQPACKET end",
            ));
        }
        let flags = fcntl(&end, FcntlArg::F_GETFD).map_err(Failure::io)?;
        fcntl(
            &end,
            FcntlArg::F_SETFD(FdFlag::from_bits_retain(flags) | FdFlag::FD_CLOEXEC),
        )
        .map_err(Failure::io)?;
        Ok(Self {
            end,
            identity: PortIdentity(Arc::new(())),
            wire,
            decode,
            outgoing: None,
            series: None,
            received_wire: None,
            received_reservation: None,
            held: None,
            decode_reservation: None,
            failure: None,
            receive_interrupted: false,
            send_interrupted: false,
            generation_send_held: false,
            zero_received: false,
            closed_receive: ClosedReceive::Finished,
        })
    }

    pub(super) fn fd(&self) -> BorrowedFd<'_> {
        self.end.as_fd()
    }

    pub(super) fn interest(&self) -> Interest {
        Interest {
            readable: self.failure().is_none() && self.held.is_none(),
            writable: self.failure().is_none() && self.outgoing.is_some() && !self.send_is_held(),
            live: self.failure().is_none(),
        }
    }

    /// Interrupted work is revisited even if the consumed observation no longer
    /// makes the fd readable. A partial series alone waits for its next packet.
    pub(super) fn needs_progress(&self) -> bool {
        self.failure().is_none()
            && (self.receive_interrupted
                || (self.send_interrupted && !self.send_is_held())
                || self.zero_received)
    }

    fn send_is_held(&self) -> bool {
        self.generation_send_held
            && self
                .outgoing
                .as_ref()
                .is_some_and(|out| out.generation && out.offset == 0 && !out.preamble_sent)
    }

    pub(super) fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref().or_else(|| {
            self.decode_reservation.as_ref().and_then(|reservation| {
                reservation
                    .abandoned
                    .load(Ordering::Acquire)
                    .then_some(&reservation.abandoned_failure)
            })
        })
    }
    pub(super) fn held(&self) -> Option<&T> {
        self.held.as_ref()
    }

    /// Whether an owned send still owes transport work. False alone is no
    /// proof of sending. A tracked caller uses its exact OrganSend ticket.
    pub(super) fn send_pending(&self) -> bool {
        self.outgoing.is_some()
    }

    /// Transfer already received evidence even if a later probe observed EOF.
    /// A final release/refusal can precede an orderly exit. Only the caller can
    /// judge that order. Taking it grants no decision permission and never
    /// clears a latched fault or permits another read or send.
    pub(super) fn take_completion(&mut self) -> Result<Option<T>, Failure> {
        if self.decode_reservation.is_some() {
            return Err(Failure::invalid(
                "decode completion belongs to its reservation",
            ));
        }
        let completion = self.held.take();
        if completion.is_some() {
            self.received_wire = None;
            self.received_reservation = None;
            return Ok(completion);
        }
        self.live()?;
        Ok(None)
    }

    fn live(&self) -> Result<(), Failure> {
        self.failure().cloned().map_or(Ok(()), Err)
    }

    fn fail(&mut self, failure: Failure) -> Failure {
        if self.failure.is_none() {
            if self.wire == Wire::Organ
                && failure.fault == ChannelFault::Closed
                && !self.zero_received
            {
                self.closed_receive = ClosedReceive::Pending;
            }
            self.failure = Some(failure);
            let _ = shutdown(self.end.as_raw_fd(), Shutdown::Both);
        }
        self.failure.as_ref().expect("first failure held").clone()
    }

    fn stage_encoded<I>(
        &mut self,
        input: I,
        encode: impl FnOnce(&I) -> Result<Vec<u8>, serde_json::Error>,
    ) -> Result<Stage<I>, StageFailure<I>> {
        if let Err(failure) = self.live() {
            return Err(StageFailure { input, failure });
        }
        if self.outgoing.is_some() || self.decode_reservation.is_some() {
            return Ok(Stage::Busy(input));
        }
        let prepared = (|| {
            let body = encode(&input).map_err(|error| Failure::invalid(error.to_string()))?;
            let limit = match self.wire {
                Wire::Organ | Wire::Packet => MAX_ENVELOPE_BYTES,
                Wire::Decode => DECODE_MESSAGE_BOUND,
            };
            if body.is_empty() {
                return Err(Failure::invalid("empty outgoing message"));
            }
            if body.len() > limit {
                return Err(Failure::truncated(limit));
            }
            let preamble = if body.len() > MAX_ENVELOPE_BYTES {
                Some(
                    serde_json::to_vec(&SegmentPreamble {
                        segments: body.len().div_ceil(MAX_ENVELOPE_BYTES) as u64,
                        bytes: body.len() as u64,
                    })
                    .map_err(|error| Failure::invalid(error.to_string()))?,
                )
            } else {
                None
            };
            Ok(Outbound {
                body,
                preamble,
                preamble_sent: false,
                offset: 0,
                tracked: None,
                decode_reservation: None,
                generation: false,
                resident_mutation: false,
                admit: false,
                cancel: false,
            })
        })();
        match prepared {
            Ok(outbound) => {
                self.outgoing = Some(outbound);
                Ok(Stage::Queued)
            }
            Err(failure) => Err(StageFailure {
                input,
                failure: self.fail(failure),
            }),
        }
    }

    pub(super) fn try_send(&mut self) -> Result<SendProgress, Failure> {
        self.send_with(send)
    }

    fn send_with(
        &mut self,
        transmit: impl FnOnce(RawFd, &[u8], MsgFlags) -> nix::Result<usize>,
    ) -> Result<SendProgress, Failure> {
        self.send_with_observation(transmit, None)
    }

    fn send_with_observation(
        &mut self,
        transmit: impl FnOnce(RawFd, &[u8], MsgFlags) -> nix::Result<usize>,
        observe: Option<&mut dyn PumpIo>,
    ) -> Result<SendProgress, Failure> {
        self.live()?;
        if self.send_is_held() {
            return Ok(SendProgress::Pending);
        }
        if observe.is_none()
            && self
                .outgoing
                .as_ref()
                .is_some_and(|outbound| outbound.tracked.is_some())
        {
            return Err(Failure::invalid(
                "tracked send requires the common pump clock",
            ));
        }
        // An observed terminal/empty packet is unresolved, not permission to
        // send more bytes while its non-consuming status probe is interrupted.
        if self.zero_received {
            return Ok(SendProgress::Pending);
        }
        let Some(outbound) = self.outgoing.as_mut() else {
            return Ok(SendProgress::Complete);
        };
        let is_preamble = outbound.preamble.is_some() && !outbound.preamble_sent;
        let packet = if is_preamble {
            outbound
                .preamble
                .as_ref()
                .expect("preamble exists")
                .as_slice()
        } else {
            &outbound.body[outbound.offset
                ..outbound
                    .body
                    .len()
                    .min(outbound.offset + MAX_ENVELOPE_BYTES)]
        };
        let sent = match transmit(
            self.end.as_raw_fd(),
            packet,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(sent) => {
                self.send_interrupted = false;
                sent
            }
            Err(Errno::EAGAIN) => {
                self.send_interrupted = false;
                return Ok(SendProgress::Pending);
            }
            Err(Errno::EINTR) => {
                self.send_interrupted = true;
                return Ok(SendProgress::Pending);
            }
            Err(error) => return Err(self.fail(Failure::io(error))),
        };
        if sent != packet.len() {
            return Err(self.fail(Failure::closed("SEQPACKET send was not atomic")));
        }
        if is_preamble {
            outbound.preamble_sent = true;
        } else {
            outbound.offset += sent;
        }
        if outbound.offset == outbound.body.len() {
            let tracked = outbound.tracked.clone();
            if let Some(reservation) = &outbound.decode_reservation {
                if outbound.cancel {
                    reservation.cancel_sent.store(true, Ordering::Release);
                } else {
                    reservation.sent.store(true, Ordering::Release);
                }
            }
            if let Some(tracked) = &tracked {
                tracked.status.store(SEND_SENT, Ordering::Release);
            }
            self.outgoing = None;
            if let Some(tracked) = tracked {
                let observed = match observe.expect("tracked send requires a clock").now() {
                    Ok(observed) => observed,
                    Err(error) => {
                        return Err(self.fail(Failure {
                            fault: ChannelFault::Undecodable,
                            detail: format!("tracked send clock observation failed: {error}"),
                            errno: error.raw_os_error(),
                        }));
                    }
                };
                tracked.sent_at.store(observed, Ordering::Relaxed);
                tracked.status.store(SEND_OBSERVED, Ordering::Release);
            }
            Ok(SendProgress::Complete)
        } else {
            Ok(SendProgress::Progress)
        }
    }

    pub(super) fn try_receive(&mut self) -> Result<ReceiveProgress, Failure> {
        self.receive_with(receive_packet, peer_status)
    }

    fn receive_with(
        &mut self,
        receive: impl FnOnce(RawFd) -> Result<Packet, ReadFault>,
        status: impl FnOnce(RawFd) -> nix::Result<i16>,
    ) -> Result<ReceiveProgress, Failure> {
        self.live()?;
        if self.held.is_some() || self.zero_received {
            return self.resolve_status(status);
        }
        let packet = match receive(self.end.as_raw_fd()) {
            Ok(packet) => {
                self.receive_interrupted = false;
                packet
            }
            Err(ReadFault::Pending) => {
                self.receive_interrupted = false;
                return Ok(ReceiveProgress::Pending);
            }
            Err(ReadFault::Interrupted) => {
                self.receive_interrupted = true;
                return Ok(ReceiveProgress::Pending);
            }
            Err(ReadFault::Failed {
                failure,
                received_wire,
            }) => {
                if let Some(received_wire) = received_wire {
                    self.received_wire = Some(received_wire);
                }
                return Err(self.fail(failure));
            }
        };
        if packet.bytes.is_empty() {
            self.received_wire = Some(packet.bytes);
            self.zero_received = true;
            // No second datagram receive. This immediate status probe decides
            // whether recv(0) was an empty packet or a closed peer.
            return self.resolve_status(status);
        }
        self.received_wire = Some(packet.bytes);
        if self.series.is_none() {
            self.received_reservation =
                self.decode_reservation.as_ref().map(|state| DecodeReceipt {
                    state: Arc::clone(state),
                    after_send: state.sent.load(Ordering::Acquire),
                    after_cancel: state.cancel_sent.load(Ordering::Acquire),
                });
        }
        self.accept_packet().map_err(|failure| self.fail(failure))
    }

    fn resolve_status(
        &mut self,
        status: impl FnOnce(RawFd) -> nix::Result<i16>,
    ) -> Result<ReceiveProgress, Failure> {
        let events = match status(self.end.as_raw_fd()) {
            Ok(events) => {
                self.receive_interrupted = false;
                events
            }
            Err(Errno::EINTR) => {
                self.receive_interrupted = true;
                return Ok(ReceiveProgress::Pending);
            }
            Err(error) => return Err(self.fail(Failure::io(error))),
        };
        if events & libc::POLLNVAL != 0 {
            return Err(self.fail(Failure::io(Errno::EBADF)));
        }
        if events & libc::POLLERR != 0 {
            return Err(self.fail(Failure::io(Errno::EIO)));
        }
        if events & (libc::POLLHUP | libc::POLLRDHUP) != 0 {
            if self.zero_received {
                // recv(0) plus a closed peer is EOF, not an empty malformed
                // datagram. A previously decoded completion remains intact.
                self.received_wire = None;
            }
            return Err(self.fail(Failure::closed("packet peer closed")));
        }
        if self.zero_received {
            return Err(self.fail(Failure::invalid("empty datagram from live peer")));
        }
        Ok(ReceiveProgress::Held)
    }

    fn accept_packet(&mut self) -> Result<ReceiveProgress, Failure> {
        let packet = self.received_wire.as_ref().expect("one packet received");
        if let Some(series) = &mut self.series {
            let total = series
                .bytes
                .len()
                .checked_add(packet.len())
                .filter(|total| *total <= series.total)
                .ok_or_else(|| Failure::invalid("decode series exceeds declared total"))?;
            if series.received >= series.segments {
                return Err(Failure::invalid("decode series has surplus segments"));
            }
            series.bytes.extend_from_slice(packet);
            series.received += 1;
            self.received_wire = None;
            if series.received < series.segments {
                if total == series.total {
                    return Err(Failure::invalid(
                        "decode series ended before its segment count",
                    ));
                }
                return Ok(ReceiveProgress::Progress);
            }
            if total != series.total {
                return Err(Failure::invalid("decode series ended short"));
            }
            let series = self.series.take().expect("complete series held");
            self.received_wire = Some(series.bytes);
        } else if self.wire == Wire::Decode
            && let Ok(serde_json::Value::Object(object)) = serde_json::from_slice(packet)
            && !object.contains_key("kind")
        {
            let preamble: SegmentPreamble = serde_json::from_slice(packet)
                .map_err(|error| Failure::invalid(error.to_string()))?;
            let total = usize::try_from(preamble.bytes)
                .map_err(|_| Failure::invalid("decode byte total does not fit host"))?;
            if total <= MAX_ENVELOPE_BYTES
                || total > DECODE_MESSAGE_BOUND
                || preamble.segments != total.div_ceil(MAX_ENVELOPE_BYTES) as u64
            {
                return Err(Failure::invalid("invalid decode segment declaration"));
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(total)
                .map_err(|error| Failure::invalid(error.to_string()))?;
            self.series = Some(Series {
                bytes,
                total,
                segments: preamble.segments,
                received: 0,
            });
            self.received_wire = None;
            return Ok(ReceiveProgress::Progress);
        }
        let whole = self.received_wire.as_ref().expect("whole message retained");
        self.held = Some((self.decode)(whole)?);
        Ok(ReceiveProgress::Held)
    }
}

struct Packet {
    bytes: Vec<u8>,
}
enum ReadFault {
    Pending,
    Interrupted,
    Failed {
        failure: Failure,
        received_wire: Option<Vec<u8>>,
    },
}

/// Ancillary custody is established before any flag, payload or shape refusal.
/// Ordinary ports accept no handles, but every delivered SCM_RIGHTS descriptor
/// is adopted and closed even on MSG_CTRUNC. The kernel owns any undisclosed tail.
fn receive_packet(fd: RawFd) -> Result<Packet, ReadFault> {
    let mut body = [0u8; MAX_ENVELOPE_BYTES];
    // Machine-word storage satisfies cmsghdr alignment.
    let mut control = [0usize; 32];
    let mut iov = libc::iovec {
        iov_base: body.as_mut_ptr().cast(),
        iov_len: body.len(),
    };
    // SAFETY: All buffers remain live for recvmsg and its returned cmsg walk.
    let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
    header.msg_iov = &mut iov;
    header.msg_iovlen = 1;
    header.msg_control = control.as_mut_ptr().cast();
    header.msg_controllen = std::mem::size_of_val(&control);
    // SAFETY: fd is borrowed from the sole owning transport. The pointers above
    // name correctly aligned writable buffers of their advertised lengths.
    let count =
        unsafe { libc::recvmsg(fd, &mut header, libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC) };
    if count < 0 {
        return match Errno::last() {
            Errno::EAGAIN => Err(ReadFault::Pending),
            Errno::EINTR => Err(ReadFault::Interrupted),
            error => Err(ReadFault::Failed {
                failure: Failure::io(error),
                received_wire: None,
            }),
        };
    }
    let mut ancillary = false;
    // SAFETY: Walk only kernel-written headers within this bounded control area.
    unsafe {
        let mut item = libc::CMSG_FIRSTHDR(&header);
        while !item.is_null() {
            ancillary = true;
            let length = (*item).cmsg_len;
            let offset = libc::CMSG_LEN(0) as usize;
            if length < offset || length > header.msg_controllen {
                break;
            }
            if (*item).cmsg_level == libc::SOL_SOCKET && (*item).cmsg_type == libc::SCM_RIGHTS {
                let bytes = libc::CMSG_DATA(item);
                for index in 0..(length - offset) / std::mem::size_of::<RawFd>() {
                    let descriptor = std::ptr::read_unaligned(
                        bytes
                            .add(index * std::mem::size_of::<RawFd>())
                            .cast::<RawFd>(),
                    );
                    if descriptor >= 0 {
                        drop(OwnedFd::from_raw_fd(descriptor));
                    }
                }
            }
            item = libc::CMSG_NXTHDR(&header, item);
        }
    }
    let count = usize::try_from(count).map_err(|_| ReadFault::Failed {
        failure: Failure::invalid("negative packet size"),
        received_wire: None,
    })?;
    let copied = count.min(body.len());
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(copied)
        .map_err(|error| ReadFault::Failed {
            failure: Failure::invalid(error.to_string()),
            received_wire: None,
        })?;
    bytes.extend_from_slice(&body[..copied]);
    if header.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0 || count > body.len() {
        return Err(ReadFault::Failed {
            failure: Failure::truncated(MAX_ENVELOPE_BYTES),
            received_wire: Some(bytes),
        });
    }
    if ancillary {
        return Err(ReadFault::Failed {
            failure: Failure::invalid(
                "ancillary handles belong to preflight Enter, not a pump port",
            ),
            received_wire: Some(bytes),
        });
    }
    Ok(Packet { bytes })
}

fn peer_status(fd: RawFd) -> nix::Result<i16> {
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLRDHUP,
        revents: 0,
    };
    // SAFETY: This is one nonblocking probe of a valid stack pollfd.
    let result = unsafe { libc::poll(&mut descriptor, 1, 0) };
    if result < 0 {
        Err(Errno::last())
    } else {
        Ok(descriptor.revents)
    }
}

#[cfg(test)]
#[path = "pump_transport_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pump_transport_evidence_tests.rs"]
mod evidence_tests;

#[cfg(test)]
#[path = "pump_transport_decode_reservation_tests.rs"]
mod decode_reservation_tests;

#[cfg(test)]
#[path = "pump_transport_send_hold_tests.rs"]
mod send_hold_tests;
