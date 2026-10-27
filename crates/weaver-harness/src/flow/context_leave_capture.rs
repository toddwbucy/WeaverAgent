//! The shared capture interval of a demand save or an accepted Leave after Lower.
//! It retains every lifecycle carry and member result through final accounting.
//! A refusal ends only this capture interval. Its lifecycle disposition belongs
//! to the parent and is distinct from successful publication or actual Left.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::CapturePhase;
use crate::flow::pump::{LifecyclePeer, ReceivedCapture, RetiredLifecyclePeer};
use crate::flow::pump_transport::{OrganSend, PortIdentity, TrackedStage};
use crate::flow::report::{CaptureAtHalt, CaptureState};
use std::sync::Arc;
use weaver_trace::Subsystem;
use weaver_trace::serving::payloads as trace;
use weaver_types::state_flow as shared;
use weaver_types::state_flow::capture::CaptureAnswer as LifecycleCaptureAnswer;
use weaver_types::state_flow::capture::*;
use weaver_types::{ExchangeId, Opener, Position, RunId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Requested,
    QueueBoundary,
    QueueRefused,
    Beginning,
    Barrier,
    Image,
    AwaitChecked,
    Acknowledge,
    Finished,
    AwaitPrepared,
    Confirmed,
    Commit,
    AwaitPublished,
    Published,
    Release,
    CompleteCarry,
    Complete,
    Refused,
    RefusalReady,
    CancelRecord,
    CancelMember,
}

#[derive(Clone, Copy)]
enum CaptureOrigin {
    Demand { received_ns: u64 },
    Leave { bound: AbsoluteBound, forced: bool },
}
impl CaptureOrigin {
    fn matches(self, context: &RecordingContext) -> bool {
        match self {
            Self::Demand { received_ns } => context.last_ns >= received_ns,
            Self::Leave { bound, .. } => context.deadlines.get(PumpDeadline::Leave) == Some(bound),
        }
    }
    fn cause(self) -> trace::PersistenceCause {
        match self {
            Self::Demand { .. } => trace::PersistenceCause::Requested,
            Self::Leave { forced: true, .. } => trace::PersistenceCause::ForcedUnload,
            Self::Leave { forced: false, .. } => trace::PersistenceCause::GracefulUnload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServingCaptureProgress {
    Progress,
    Pending,
    Published,
    Refused,
}

struct Carry {
    envelope: CaptureEnvelope,
    send: Option<OrganSend>,
    returned: Option<CaptureEnvelope>,
}

impl Carry {
    fn new(exchange: &ExchangeId, payload: CapturePayload) -> Self {
        Self {
            envelope: CaptureEnvelope {
                exchange: exchange.clone(),
                position: if matches!(
                    payload,
                    CapturePayload::Answer(_) | CapturePayload::Refusal(_)
                ) {
                    Position::Close
                } else {
                    Position::Continue
                },
                payload,
            },
            send: None,
            returned: None,
        }
    }
    fn sent(&self) -> bool {
        self.send.as_ref().is_some_and(OrganSend::sent)
    }
    fn advance(
        &mut self,
        peer: &mut LifecyclePeer,
        context: &mut RecordingContext,
        window: AbsoluteBound,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        context.live(io)?;
        if self.send.is_none() {
            match peer.stage_capture(self.envelope.clone()) {
                Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                Ok(TrackedStage::Busy(original)) => {
                    self.returned = Some(original);
                    return Err(fault(context, io, "capture has a competing lifecycle send"));
                }
                Err(failure) => {
                    self.returned = Some(failure.input);
                    if failure.failure.fault == crate::ChannelFault::Closed {
                        return Err(RunFailure::Transport {
                            source: PumpSource::LeavePeer,
                            failure: failure.failure,
                        });
                    }
                    return Err(context.fail(
                        io,
                        FlowHaltReason::CarryFailed,
                        HaltPoint::Barrier,
                        format!("capture lifecycle stage: {:?}", failure.failure),
                    ));
                }
            }
        }
        // The transport records a whole send before its fallible clock. Its
        // witness remains in this owner if either this or the final check fails.
        peer.try_send_observed(io).map_err(|error| {
            if error.fault == crate::ChannelFault::Closed {
                RunFailure::Transport {
                    source: PumpSource::LeavePeer,
                    failure: error,
                }
            } else {
                context.fail(
                    io,
                    FlowHaltReason::CarryFailed,
                    HaltPoint::Barrier,
                    format!("capture lifecycle send: {error:?}"),
                )
            }
        })?;
        let ticket = self.send.as_ref().unwrap();
        if !peer.owns_send(ticket) || !ticket.matches_exchange(&self.envelope.exchange) {
            return Err(fault(
                context,
                io,
                "capture send changed its original port or exchange",
            ));
        }
        if ticket.sent()
            && ticket
                .sent_at()
                .is_none_or(|at| at < window.began_ns() || window.expired(at))
        {
            return Err(fault(
                context,
                io,
                "capture continuation was not carried in its original window",
            ));
        }
        // Complete ends this wait on its actual timely send. Its parent retires
        // only CaptureWindow before checking the unchanged Leave tail.
        if !matches!(
            self.envelope.payload,
            CapturePayload::SavePointCaptureComplete(_)
                | CapturePayload::Answer(LifecycleCaptureAnswer::SavePointTaken { .. })
        ) {
            context.live(io)?;
        }
        Ok(ticket.sent())
    }
}

#[derive(Default)]
struct Record {
    started: bool,
    settled: bool,
    returned: StoppedAuthor,
    sequence: Option<Sequence>,
    queued: Option<Arc<super::serving_observation::ServingObservationAuthor>>,
}
impl Record {
    fn advance_queued<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        payload: trace::SavePointRecord,
    ) -> Result<(), RunFailure> {
        if self.queued.is_none() {
            self.queued = Some(Arc::new(
                super::serving_observation::ServingObservationAuthor::new(context, &payload),
            ));
        }
        self.advance(context, ports, payload)
    }

    fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        payload: trace::SavePointRecord,
    ) -> Result<(), RunFailure> {
        if self.settled {
            return Ok(());
        }
        self.returned.check()?;
        let result = if self.started {
            if let Some(owner) = &self.queued {
                context.resume_serving_observation(ports, owner)
            } else {
                context.resume_author_and_wait(ports)
            }
        } else {
            self.started = true;
            let event = AuthoredEvent {
                kind: Kind::SavePoint,
                subsystem: Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(Payload::SavePointRecord(Box::new(payload))),
            };
            if let Some(owner) = &self.queued {
                context.author_serving_observation(ports, owner, event)
            } else {
                context.author_and_wait(ports, event)
            }
        };
        match result {
            Ok(sequence) => {
                self.sequence = Some(sequence);
                self.settled = true;
                Ok(())
            }
            Err(error) => {
                // A post-Recorded observer failure cannot erase this fact and
                // still cannot grant Commit because the failure is returned.
                self.sequence = if let Some(owner) = &self.queued {
                    owner.sequence()
                } else {
                    context.completed_author
                };
                Err(self.returned.retain_started(error, &mut self.started))
            }
        }
    }
}

pub(super) struct ServingCapture {
    origin: Arc<()>,
    peer: PortIdentity,
    exchange: ExchangeId,
    run: RunId,
    loaded: StartIdentity,
    beginning: CaptureBeginning,
    window: AbsoluteBound,
    capture_origin: CaptureOrigin,
    stage: Stage,
    frontier: Option<u64>,
    image: Option<CaptureImage>,
    member_results: Vec<super::CaptureAnswer>,
    member_refusal: Option<super::CaptureAnswer>,
    incoming: [Option<ReceivedCapture>; 3],
    aborted: Option<ReceivedCapture>,
    unexpected: Option<ReceivedCapture>,
    carries: [Option<Carry>; 5],
    requested: Record,
    confirmed: Record,
    published_record: Record,
    refused_record: Record,
    artifact: Option<CaptureArtifact>,
    published: Option<StartIdentity>,
    refusal_code: Option<String>,
    refusal_leg: Option<SavePointLeg>,
    queued_expired: bool,
    expired_at: Option<Stage>,
    release_started: bool,
    cancelling: bool,
    disconnected: bool,
}

/// A latched capture failure with its exact lifecycle descriptor retired. No
/// caller can update a send ticket after these phase facts are frozen.
pub(super) struct FailedCapture {
    owner: ServingCapture,
    _peer: RetiredLifecyclePeer,
    facts: CaptureAtHalt,
}
/// A failed capture's retained facts permit closing only its own transports.
/// An available halt account must agree. Account allocation failure cannot keep
/// descriptors open past the cap. All ledgers stay owned, with no healthy release.
pub(in crate::flow) struct FailedCaptureDisposal {
    barrier: Arc<()>,
    facts: CaptureAtHalt,
}
impl FailedCaptureDisposal {
    pub(in crate::flow) fn allows(
        &self,
        barrier: &crate::flow::recording::RecordBarrier,
        account: Option<&crate::flow::report::FlowHaltAccount>,
        capture: Option<crate::flow::coordinator::CaptureSnapshot>,
    ) -> bool {
        Arc::ptr_eq(&self.barrier, &barrier.author_identity())
            && account.is_none_or(|account| account.capture.as_ref() == Some(&self.facts))
            && capture.is_none_or(|capture| capture.capture_id == self.facts.capture_id)
    }
}

impl FailedCapture {
    pub(super) fn disposal(&self, context: &RecordingContext) -> Option<FailedCaptureDisposal> {
        (self.matches(context)
            && context.failure.is_some()
            && context.flow.halt().is_some_and(|halt| {
                halt.account()
                    .is_none_or(|account| account.capture.as_ref() == Some(&self.facts))
            }))
        .then(|| FailedCaptureDisposal {
            barrier: context.barrier.author_identity(),
            facts: self.facts.clone(),
        })
    }
    pub(super) fn matches(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.owner.origin, &context.read_origin)
            && context.barrier.run() == self.owner.run.0
            && self.owner.capture_origin.matches(context)
            && context
                .member_capture
                .as_ref()
                .is_none_or(|capture| capture.id == self.facts.capture_id)
    }
    pub(super) fn facts(&self) -> &CaptureAtHalt {
        &self.facts
    }
}

fn fault(
    context: &mut RecordingContext,
    io: &mut impl PumpIo,
    detail: impl Into<String>,
) -> RunFailure {
    context.fail(
        io,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        detail,
    )
}
fn trace_window(value: shared::SaveWindow) -> trace::SaveWindow {
    trace::SaveWindow {
        began_ns: value.began_ns.0.into(),
        deadline_ns: value.deadline_ns.0.into(),
    }
}

fn artifact_identity(artifact: &CaptureArtifact) -> bool {
    // Foundation 5.1: this is the publisher's positive creation ordinal and
    // digest B, not the 32-hex seed ID. The harness hashes no configuration.
    artifact
        .artifact_id
        .split_once('-')
        .is_some_and(|(ordinal, digest)| {
            ordinal.len() == 20
                && ordinal.bytes().all(|byte| byte.is_ascii_digit())
                && ordinal.parse::<u64>().is_ok_and(|value| value > 0)
                && digest == artifact.config_sha256.as_str()
        })
}

impl ServingCapture {
    /// Allocate the identity at acceptance, even while the turn owns the
    /// recorder. Beginning and the filing barrier still require an explicit
    /// between-turn handoff after the requested record settles.
    pub(super) fn queue_demand(
        context: &mut RecordingContext,
        peer: &LifecyclePeer,
        exchange: ExchangeId,
        window: shared::SaveWindow,
        received_ns: u64,
        io: &mut impl PumpIo,
    ) -> Result<Self, RunFailure> {
        context.live(io)?;
        let bound = AbsoluteBound::from_absolute(window.began_ns.0, window.deadline_ns.0)
            .map_err(|error| fault(context, io, error.to_string()))?;
        let peer = peer.capture_identity().ok_or_else(|| {
            fault(
                context,
                io,
                "demand capture projection not selected on original peer",
            )
        })?;
        let save_wait = context
            .load_duty
            .as_ref()
            .map(|duty| duty.persistence.save_wait.elapsed_ns.0);
        if context.phase != RecordingPhase::Entered
            || context.member_capture.is_some()
            || context.deadlines.get(PumpDeadline::Leave).is_some()
            || context.deadlines.get(PumpDeadline::SaveQueued).is_some()
            || context.deadlines.get(PumpDeadline::CaptureWindow).is_some()
            || exchange.opener != Opener::Admin
            || bound.began_ns() > received_ns
            || received_ns > context.last_ns
            || save_wait != Some(bound.deadline_ns() - bound.began_ns())
        {
            return Err(fault(
                context,
                io,
                "demand capture changed its phase, original window or loaded duration",
            ));
        }
        let loaded = context
            .open_start
            .clone()
            .ok_or_else(|| fault(context, io, "capture has no original start"))?;
        let id = context
            .next_capture_id
            .filter(|id| *id != 0)
            .ok_or_else(|| fault(context, io, "capture ID exhausted"))?;
        // arm preserves an already-expired but valid window. Its next live
        // check returns the queued-save outcome to the enclosing owner.
        context.arm(PumpDeadline::SaveQueued, bound, io)?;
        context.next_capture_id = id.checked_add(1);
        Ok(Self::new(
            context,
            peer,
            exchange,
            loaded,
            CaptureBeginning {
                capture_id: id.into(),
                window,
                tail_window: None,
            },
            CaptureOrigin::Demand { received_ns },
        ))
    }

    pub(super) fn begin_demand_boundary(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        context.live(io)?;
        if !Arc::ptr_eq(&self.origin, &context.read_origin)
            || context.barrier.run() != self.run.0
            || !matches!(self.capture_origin, CaptureOrigin::Demand { .. })
            || !self.capture_origin.matches(context)
            || context.deadlines.get(PumpDeadline::SaveQueued) != Some(self.window)
            || context.deadlines.get(PumpDeadline::CaptureWindow).is_some()
        {
            return Err(fault(
                context,
                io,
                "demand boundary lost its original run or window",
            ));
        }
        if self.stage != Stage::QueueBoundary
            || !self.requested.settled
            || context.open_turn.is_some()
            || context.input.snapshot().is_some()
        {
            return Ok(false);
        }
        context.check_held_input()?;
        // Transfer the same registered timestamps. No clock is sampled to
        // construct another duration at this boundary.
        context.arm(PumpDeadline::CaptureWindow, self.window, io)?;
        assert_eq!(
            context.deadlines.disarm(PumpDeadline::SaveQueued),
            Some(self.window)
        );
        self.stage = Stage::Beginning;
        Ok(true)
    }

    pub(super) fn failed_handoff_ready(
        &self,
        context: &RecordingContext,
        peer: &LifecyclePeer,
    ) -> bool {
        context.failure.is_some()
            && context.flow.halt().is_some()
            && Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.barrier.run() == self.run.0
            && self.capture_origin.matches(context)
            && peer.owns_capture_identity(&self.peer)
    }

    #[allow(clippy::result_large_err)] // A refusal returns both exact move-owned resources.
    pub(super) fn into_failed(
        self,
        context: &RecordingContext,
        peer: LifecyclePeer,
    ) -> Result<Box<FailedCapture>, (Self, LifecyclePeer)> {
        if !self.failed_handoff_ready(context, &peer) {
            return Err((self, peer));
        }
        let facts = self.at_halt(context);
        Ok(Box::new(FailedCapture {
            owner: self,
            _peer: peer.retire_failed(),
            facts,
        }))
    }
    /// The containing lifecycle owner proves Ready, the accepted Leave,
    /// settled input and actual Lower/reap. This internal constructor cannot
    /// return a serving seat, a release seal, or a successful Leave outcome.
    pub(super) fn begin(
        context: &mut RecordingContext,
        peer: &LifecyclePeer,
        exchange: ExchangeId,
        leave: AbsoluteBound,
        forced: bool,
        io: &mut impl PumpIo,
    ) -> Result<Self, RunFailure> {
        context.live(io)?;
        context.check_held_input()?;
        let peer = peer.capture_identity().ok_or_else(|| {
            fault(
                context,
                io,
                "capture projection not selected on original peer",
            )
        })?;
        if context.phase != RecordingPhase::Entered
            || context.member_capture.is_some()
            || context.deadlines.get(PumpDeadline::Leave) != Some(leave)
            || context.deadlines.get(PumpDeadline::CaptureWindow).is_some()
            || context.deadlines.get(PumpDeadline::SaveQueued).is_some()
            || exchange.opener != Opener::Admin
            || context.open_turn.is_some()
            || context.input.snapshot().is_some()
        {
            return Err(fault(
                context,
                io,
                "capture requires its settled Leave boundary",
            ));
        }
        let loaded = context
            .open_start
            .clone()
            .ok_or_else(|| fault(context, io, "capture has no original start"))?;
        let id = context
            .next_capture_id
            .ok_or_else(|| fault(context, io, "capture ID exhausted"))?;
        let save_wait = context
            .load_duty
            .as_ref()
            .map(|duty| duty.persistence.save_wait.elapsed_ns.0)
            .ok_or_else(|| fault(context, io, "capture has no load"))?;
        let began = context.last_ns;
        let end = began
            .checked_add(save_wait)
            .ok_or_else(|| fault(context, io, "capture window overflow"))?
            .min(leave.deadline_ns());
        if end <= began || id == 0 {
            return Err(fault(context, io, "capture has no live save window or ID"));
        }
        let window = AbsoluteBound::from_absolute(began, end)
            .map_err(|error| fault(context, io, error.to_string()))?;
        context.arm(PumpDeadline::CaptureWindow, window, io)?;
        context.next_capture_id = id.checked_add(1);
        let beginning = CaptureBeginning {
            capture_id: id.into(),
            window: shared::SaveWindow {
                began_ns: began.into(),
                deadline_ns: end.into(),
            },
            tail_window: Some(shared::SaveWindow {
                began_ns: leave.began_ns().into(),
                deadline_ns: leave.deadline_ns().into(),
            }),
        };
        Ok(Self::new(
            context,
            peer,
            exchange,
            loaded,
            beginning,
            CaptureOrigin::Leave {
                bound: leave,
                forced,
            },
        ))
    }

    /// S6 can still convert a graceful Leave. Each new event uses the current
    /// mode; an author already holding an event keeps that exact original.
    pub(super) fn observe_leave_mode(&mut self, forced: bool) {
        if let CaptureOrigin::Leave {
            forced: current, ..
        } = &mut self.capture_origin
        {
            *current |= forced;
        }
    }

    fn new(
        context: &RecordingContext,
        peer: PortIdentity,
        exchange: ExchangeId,
        loaded: StartIdentity,
        beginning: CaptureBeginning,
        capture_origin: CaptureOrigin,
    ) -> Self {
        let window = AbsoluteBound::from_absolute(
            beginning.window.began_ns.0,
            beginning.window.deadline_ns.0,
        )
        .expect("validated original capture window");
        let mut carries = std::array::from_fn(|_| None);
        carries[0] = Some(Carry::new(
            &exchange,
            CapturePayload::SavePointBeginning(beginning.clone()),
        ));
        Self {
            origin: Arc::clone(&context.read_origin),
            peer,
            run: RunId(context.barrier.run().into()),
            exchange,
            loaded,
            beginning,
            window,
            capture_origin,
            stage: Stage::Requested,
            frontier: None,
            image: None,
            member_results: Vec::new(),
            member_refusal: None,
            incoming: std::array::from_fn(|_| None),
            aborted: None,
            unexpected: None,
            carries,
            requested: Record::default(),
            confirmed: Record::default(),
            published_record: Record::default(),
            refused_record: Record::default(),
            artifact: None,
            published: None,
            refusal_code: None,
            refusal_leg: None,
            queued_expired: false,
            expired_at: None,
            release_started: false,
            cancelling: false,
            disconnected: false,
        }
    }

    pub(super) fn report(&self) -> Option<shared::SavePointReport> {
        (self.stage == Stage::Complete).then(|| self.published_report())
    }

    pub(super) fn queued(&self) -> bool {
        matches!(self.capture_origin, CaptureOrigin::Demand { .. })
            && matches!(
                self.stage,
                Stage::Requested | Stage::QueueBoundary | Stage::QueueRefused
            )
    }

    pub(super) fn blocks_turn(&self) -> bool {
        (self.requested.started && !self.requested.settled)
            || (self.refused_record.started && !self.refused_record.settled)
            || (!self.queued() && !matches!(self.stage, Stage::Complete | Stage::RefusalReady))
    }

    pub(super) fn boundary_pending(&self) -> bool {
        self.stage == Stage::QueueBoundary && !self.queued_expired && !self.cancelling
    }

    pub(super) fn peer_available(&self) -> bool {
        !self.disconnected
    }

    pub(super) fn refusal_leg(&self) -> Option<SavePointLeg> {
        (self.stage == Stage::RefusalReady)
            .then(|| self.refusal_leg.expect("retained capture refusal leg"))
    }

    /// The enclosing current-turn owner may encounter this wait deep inside
    /// its own author or model operation. Retire only the queued save's exact
    /// registered deadline; its refusal is still owed and its ID is unchanged.
    pub(super) fn retain_queued_expiry(
        &mut self,
        context: &mut RecordingContext,
        failure: &RunFailure,
    ) -> bool {
        if !self.queued()
            || self.queued_expired
            || context.failure.is_some()
            || !Arc::ptr_eq(&self.origin, &context.read_origin)
            || !matches!(failure, RunFailure::Wait { key: PumpDeadline::SaveQueued, bound } if *bound == self.window)
            || context.deadlines.get(PumpDeadline::SaveQueued) != Some(self.window)
        {
            return false;
        }
        context.deadlines.disarm(PumpDeadline::SaveQueued);
        self.queued_expired = true;
        self.refusal_code
            .get_or_insert_with(|| "capture_not_ready".into());
        self.refusal_leg = Some(SavePointLeg::CaptureNotReady);
        true
    }

    pub(super) fn retain_peer_end(&mut self, failure: &RunFailure) -> bool {
        if !self.commit_sent()
            && !self.disconnected
            && matches!(failure, RunFailure::Transport { source: PumpSource::LeavePeer, failure } if failure.fault == crate::ChannelFault::Closed)
        {
            self.disconnected = true;
            self.cancelling = true;
            self.refusal_code
                .get_or_insert_with(|| "admin_disconnected".into());
            self.refusal_leg.get_or_insert(SavePointLeg::Published);
            true
        } else {
            false
        }
    }

    /// An expired on-demand publication window revokes Commit, not the run.
    /// Retain its exact phase before the member's bounded cleanup begins.
    pub(super) fn retain_window_expiry(
        &mut self,
        context: &RecordingContext,
        failure: &RunFailure,
    ) -> bool {
        if !matches!(self.capture_origin, CaptureOrigin::Demand { .. })
            || self.queued()
            || self.commit_sent()
            || self.expired_at.is_some()
            || context.failure.is_some()
            || !Arc::ptr_eq(&self.origin, &context.read_origin)
            || !matches!(failure, RunFailure::Wait { key: PumpDeadline::CaptureWindow, bound } if *bound == self.window)
            || context.deadlines.get(PumpDeadline::CaptureWindow) != Some(self.window)
        {
            return false;
        }
        self.expired_at = Some(self.stage);
        self.cancelling = true;
        self.refusal_code
            .get_or_insert_with(|| "capture_window_expired".into());
        self.refusal_leg.get_or_insert(match self.stage {
            Stage::Beginning | Stage::Barrier => SavePointLeg::Answer,
            Stage::Image | Stage::AwaitChecked | Stage::Acknowledge => SavePointLeg::Finished,
            _ => SavePointLeg::Published,
        });
        true
    }

    fn published_report(&self) -> shared::SavePointReport {
        shared::SavePointReport {
            start: self.published.clone().expect("actual Published"),
            capture_id: self.beginning.capture_id,
            event_run: self.run.clone(),
            event_sequence: self
                .confirmed
                .sequence
                .expect("confirmed Recorded")
                .0
                .into(),
        }
    }

    fn commit_sent(&self) -> bool {
        self.carries[3].as_ref().is_some_and(Carry::sent)
    }

    pub(super) fn at_halt(&self, context: &RecordingContext) -> CaptureAtHalt {
        CaptureAtHalt {
            capture_id: self.beginning.capture_id,
            state: match self.stage {
                Stage::Requested if matches!(self.capture_origin, CaptureOrigin::Demand { .. }) => {
                    CaptureState::QueueBoundary
                }
                Stage::QueueBoundary => CaptureState::QueueBoundary,
                Stage::Requested | Stage::Beginning => CaptureState::Beginning,
                Stage::Barrier => match context.capture_snapshot().map(|snapshot| snapshot.phase) {
                    None | Some(CapturePhase::Barrier) => CaptureState::Barrier,
                    Some(CapturePhase::Aborting | CapturePhase::Complete) => CaptureState::Aborting,
                    _ => CaptureState::AwaitImage,
                },
                Stage::Image | Stage::AwaitChecked => CaptureState::AwaitChecked,
                Stage::Acknowledge => CaptureState::AwaitFinished,
                Stage::Finished | Stage::AwaitPrepared => CaptureState::AwaitPrepared,
                Stage::Confirmed => CaptureState::AwaitRecorded,
                Stage::Commit | Stage::AwaitPublished => CaptureState::AwaitPublished,
                Stage::Published | Stage::Release | Stage::CompleteCarry | Stage::Complete => {
                    CaptureState::Completing
                }
                Stage::QueueRefused
                | Stage::Refused
                | Stage::RefusalReady
                | Stage::CancelRecord
                | Stage::CancelMember => CaptureState::Aborting,
            },
            commit_sent: self.commit_sent(),
            published: self.published.clone(),
        }
    }

    fn payload(&self, phase: Stage) -> trace::SavePointRecord {
        let capture_id = self.beginning.capture_id.0.into();
        let window = trace_window(self.beginning.window);
        let tail_window = self.beginning.tail_window.map(trace_window);
        let cause = self.capture_origin.cause();
        match phase {
            Stage::Requested => trace::SavePointRecord::Requested {
                capture_id,
                window,
                tail_window,
                cause,
            },
            Stage::Refused => trace::SavePointRecord::Refused {
                capture_id,
                window,
                tail_window,
                cause,
                refusal_code: self.refusal_code.clone().expect("exact refusal"),
            },
            Stage::Confirmed | Stage::Published => {
                let artifact = self.artifact.as_ref().expect("Prepared");
                let artifact_id = artifact.artifact_id.clone();
                let dump_sha256 = artifact.dump_sha256.as_str().to_owned();
                let config_sha256 = artifact.config_sha256.as_str().to_owned();
                let location = format!("save-points/{artifact_id}");
                let position = self
                    .image
                    .as_ref()
                    .and_then(|image| image.position.as_ref())
                    .map(super::super::mirror::source_position);
                if phase == Stage::Confirmed {
                    trace::SavePointRecord::Confirmed {
                        capture_id,
                        window,
                        tail_window,
                        cause,
                        artifact_id,
                        dump_sha256,
                        config_sha256,
                        location,
                        position,
                    }
                } else {
                    trace::SavePointRecord::Published {
                        capture_id,
                        window,
                        tail_window,
                        cause,
                        artifact_id,
                        dump_sha256,
                        config_sha256,
                        location,
                        position,
                    }
                }
            }
            _ => unreachable!("only capture author stages have records"),
        }
    }

    fn receive<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        slot: usize,
    ) -> Result<bool, RunFailure> {
        if self.incoming[slot].is_some() {
            return Ok(true);
        }
        let peer = ports
            .lifecycle
            .leave_peer
            .as_deref_mut()
            .expect("checked Leave peer");
        if peer.capture_held().is_none() {
            peer.try_receive().map_err(|error| {
                if error.fault == crate::ChannelFault::Closed {
                    return RunFailure::Transport {
                        source: PumpSource::LeavePeer,
                        failure: error,
                    };
                }
                context.fail(
                    ports.io,
                    FlowHaltReason::CarryFailed,
                    HaltPoint::Barrier,
                    format!("capture receive: {error:?}"),
                )
            })?;
        }
        let Some(received) = peer.take_capture().map_err(|error| {
            if error.fault == crate::ChannelFault::Closed {
                return RunFailure::Transport {
                    source: PumpSource::LeavePeer,
                    failure: error,
                };
            }
            context.fail(
                ports.io,
                FlowHaltReason::CarryFailed,
                HaltPoint::Barrier,
                format!("capture take: {error:?}"),
            )
        })?
        else {
            return Ok(false);
        };
        let message = &received.message;
        let matches = message.envelope.exchange == self.exchange
            && message.envelope.capture_id() == Some(self.beginning.capture_id)
            && message.sink.is_none()
            && message.state.is_none()
            && matches!(
                (slot, &message.envelope.payload),
                (0, CapturePayload::SavePointImageChecked(_))
                    | (1, CapturePayload::SavePointPrepared(_))
                    | (2, CapturePayload::SavePointPublished(_))
            );
        if !matches {
            self.unexpected = Some(received);
            return Err(fault(
                context,
                ports.io,
                "capture continuation changed exchange, capture, direction or phase",
            ));
        }
        assert!(self.incoming[slot].is_none());
        self.incoming[slot] = Some(received);
        context.live(ports.io)?;
        Ok(true)
    }

    fn carry<I: PumpIo>(
        &mut self,
        index: usize,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        self.carries[index]
            .as_mut()
            .expect("prepared continuation")
            .advance(
                ports
                    .lifecycle
                    .leave_peer
                    .as_deref_mut()
                    .expect("checked Leave peer"),
                context,
                self.window,
                ports.io,
            )
    }

    fn probe<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        if self.disconnected {
            return Ok(());
        }
        let peer = ports
            .lifecycle
            .leave_peer
            .as_deref_mut()
            .expect("checked peer");
        let result = if peer.capture_held().is_none() {
            peer.try_receive().map(|_| ())
        } else {
            Ok(())
        };
        if let Err(error) = result {
            if error.fault == crate::ChannelFault::Closed && !self.commit_sent() {
                self.disconnected = true;
                self.cancelling = true;
                self.refusal_code
                    .get_or_insert_with(|| "admin_disconnected".into());
                self.refusal_leg.get_or_insert(SavePointLeg::Published);
                return Ok(());
            }
            return Err(context.fail(
                ports.io,
                if error.fault == crate::ChannelFault::Closed {
                    FlowHaltReason::CarryFailed
                } else {
                    FlowHaltReason::ProtocolFailed
                },
                HaltPoint::Barrier,
                format!("capture receive: {error:?}"),
            ));
        }
        let Some(message) = peer.capture_held() else {
            return Ok(());
        };
        if self.cancelling && self.expired_at.is_none() {
            self.unexpected = peer.take_capture().expect("held complete packet");
            return Err(fault(
                context,
                ports.io,
                "capture cancellation received another continuation",
            ));
        }
        let identity = message.envelope.exchange == self.exchange
            && message.envelope.capture_id() == Some(self.beginning.capture_id)
            && message.sink.is_none()
            && message.state.is_none();
        if identity
            && let CapturePayload::SavePointAborted(aborted) = &message.envelope.payload
            && !self.commit_sent()
        {
            if self.aborted.is_some() {
                self.unexpected = peer.take_capture().expect("held duplicate abort");
                return Err(fault(
                    context,
                    ports.io,
                    "capture received duplicate admin abort",
                ));
            }
            self.refusal_code
                .get_or_insert_with(|| aborted.code.clone());
            self.refusal_leg.get_or_insert(SavePointLeg::Published);
            self.aborted = peer.take_capture().expect("held complete packet");
            self.cancelling = true;
            return Ok(());
        }
        let expected = match self.expired_at.unwrap_or(self.stage) {
            Stage::AwaitChecked => Some(0),
            Stage::Image if self.carries[1].as_ref().is_some_and(Carry::sent) => Some(0),
            Stage::AwaitPrepared => Some(1),
            Stage::Finished if self.carries[2].as_ref().is_some_and(Carry::sent) => Some(1),
            Stage::AwaitPublished => Some(2),
            Stage::Commit if self.carries[3].as_ref().is_some_and(Carry::sent) => Some(2),
            _ => None,
        };
        let allowed = identity
            && matches!(
                (expected, &message.envelope.payload),
                (Some(0), CapturePayload::SavePointImageChecked(_))
                    | (Some(1), CapturePayload::SavePointPrepared(_))
                    | (Some(2), CapturePayload::SavePointPublished(_))
            );
        if !allowed {
            self.unexpected = peer.take_capture().expect("held complete packet");
            return Err(fault(
                context,
                ports.io,
                "capture continuation changed exchange, capture, direction or phase",
            ));
        }
        if self.expired_at.is_some() {
            let slot = expected.expect("validated late continuation");
            if self.incoming[slot].is_some() {
                self.unexpected = peer.take_capture().expect("held duplicate packet");
                return Err(fault(
                    context,
                    ports.io,
                    "capture expiry received duplicate continuation",
                ));
            }
            // Keep one valid in-flight reply as evidence; it cannot advance
            // publication or reset the old window while Abort is outstanding.
            self.incoming[slot] = peer.take_capture().expect("held complete packet");
        }
        Ok(())
    }

    fn cancel<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ServingCaptureProgress, RunFailure> {
        if let Some(peer) = ports.lifecycle.leave_peer.as_deref_mut() {
            self.withdraw_pending_carries(context, peer, ports.io)?;
        }
        self.prepare_window_expiry(context, ports.io)?;
        self.cancel_after_window(context, ports)
    }

    /// The containing worker may encounter this expiry while recording an
    /// unrelated observation. Let the original capture start its existing
    /// cleanup before that observer checks the same expired window again.
    /// This step neither authors a refusal nor takes another author's ticket.
    pub(super) fn prepare_retained_window_expiry(
        &mut self,
        context: &mut RecordingContext,
        peer: &mut LifecyclePeer,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.expired_at.is_none() {
            return Ok(());
        }
        if !matches!(self.capture_origin, CaptureOrigin::Demand { .. })
            || !Arc::ptr_eq(&self.origin, &context.read_origin)
            || context.barrier.run() != self.run.0
            || !self.capture_origin.matches(context)
            || !peer.owns_capture_identity(&self.peer)
        {
            return Err(fault(
                context,
                io,
                "capture expiry lost its original run or peer",
            ));
        }
        self.withdraw_pending_carries(context, peer, io)?;
        self.prepare_window_expiry(context, io)
    }

    fn prepare_window_expiry(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.expired_at.is_some()
            && context.deadlines.get(PumpDeadline::CaptureWindow) == Some(self.window)
        {
            if context.member_capture.is_some() {
                context.expire_member_capture(self.beginning.capture_id, io)?;
            } else {
                // Beginning did not reach a member barrier. Only this exact
                // capture's wait retires; no publication allowance is renewed.
                context.deadlines.disarm(PumpDeadline::CaptureWindow);
            }
        }
        Ok(())
    }

    fn withdraw_pending_carries(
        &mut self,
        context: &mut RecordingContext,
        peer: &mut LifecyclePeer,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        // An expired or cancelled Commit must be unsendable before any
        // fallible cleanup step or any other owner gets the common pump.
        for carry in self.carries.iter_mut().flatten() {
            if let Some(ticket) = carry.send.as_ref()
                && !ticket.sent()
                && !ticket.withdrawn()
            {
                let returned = peer.withdraw_capture_pending(ticket).map_err(|error| {
                    fault(context, io, format!("capture cancellation: {error:?}"))
                })?;
                if returned.as_ref() != Some(&carry.envelope) {
                    return Err(fault(
                        context,
                        io,
                        "capture cancellation lost original unsent packet",
                    ));
                }
                carry.returned = returned;
            }
        }
        Ok(())
    }

    fn cancel_after_window<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ServingCaptureProgress, RunFailure> {
        // Finish the exact already-admitted author before starting its refusal.
        // A retained original is never replaced by a fresh equivalent event.
        if !self.requested.settled && (self.requested.started || self.queued()) {
            if matches!(self.capture_origin, CaptureOrigin::Demand { .. }) {
                self.requested
                    .advance_queued(context, ports, self.payload(Stage::Requested))?;
            } else {
                self.requested
                    .advance(context, ports, self.payload(Stage::Requested))?;
            }
        }
        if self.confirmed.started && !self.confirmed.settled {
            self.confirmed
                .advance(context, ports, self.payload(Stage::Confirmed))?;
        }
        if !matches!(self.stage, Stage::CancelRecord | Stage::CancelMember) {
            self.stage = Stage::CancelRecord;
        }
        if self.stage == Stage::CancelRecord {
            if matches!(self.capture_origin, CaptureOrigin::Demand { .. })
                && self.frontier.is_none()
            {
                self.refused_record
                    .advance_queued(context, ports, self.payload(Stage::Refused))?;
            } else {
                self.refused_record
                    .advance(context, ports, self.payload(Stage::Refused))?;
            }
            self.stage = Stage::CancelMember;
        }
        if context.member_capture.is_none() {
            context.deadlines.disarm(PumpDeadline::CaptureWindow);
            if matches!(self.capture_origin, CaptureOrigin::Demand { .. }) {
                context.deadlines.disarm(PumpDeadline::SaveQueued);
            }
            self.stage = Stage::RefusalReady;
            return Ok(ServingCaptureProgress::Refused);
        }
        context.cancel_member_capture(self.beginning.capture_id, ports.io)?;
        if context.member_capture.is_none() {
            self.stage = Stage::RefusalReady;
            return Ok(ServingCaptureProgress::Refused);
        }
        if context.advance_member_capture(ports.io)? == CaptureProgress::Complete
            && let Some(answer) = context.take_capture_answer(ports.io)?
        {
            self.member_results.push(answer);
            self.member_refusal = context.take_recorded_capture_refusal(self.beginning.capture_id);
            context.finish_member_capture(self.beginning.capture_id, ports.io)?;
            self.stage = Stage::RefusalReady;
            return Ok(ServingCaptureProgress::Refused);
        }
        Ok(ServingCaptureProgress::Pending)
    }

    fn complete_sent<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        if self.stage != Stage::CompleteCarry {
            return Ok(false);
        }
        let carry = self.carries[4].as_ref().expect("prepared CaptureComplete");
        let Some(ticket) = carry.send.as_ref().filter(|send| send.sent()) else {
            return Ok(false);
        };
        let peer = ports
            .lifecycle
            .leave_peer
            .as_deref()
            .expect("original peer");
        if !peer.owns_send(ticket)
            || !ticket.matches_exchange(&self.exchange)
            || ticket
                .sent_at()
                .is_none_or(|at| at < self.window.began_ns() || self.window.expired(at))
            || context.deadlines.get(PumpDeadline::CaptureWindow) != Some(self.window)
            || context.capture_snapshot().is_none_or(|snapshot| {
                snapshot.capture_id != self.beginning.capture_id
                    || snapshot.frontier != self.frontier.expect("original barrier")
                    || snapshot.phase != CapturePhase::Complete
            })
            || !self.confirmed.settled
            || !self.published_record.settled
            || self.published.is_none()
        {
            return Err(fault(
                context,
                ports.io,
                "CaptureComplete lacks its original timely carry and recorded cleanup",
            ));
        }
        // This private state is reached only after the matching release answer
        // has been Recorded and consumed. A delayed reduction cannot turn that
        // whole timely carry back into a waiting capture. All remaining bounds,
        // clock failures and component failures still apply in finish.
        context.deadlines.disarm(PumpDeadline::CaptureWindow);
        context.finish_member_capture(self.beginning.capture_id, ports.io)?;
        self.stage = Stage::Complete;
        Ok(true)
    }

    pub(super) fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<ServingCaptureProgress, RunFailure> {
        if !Arc::ptr_eq(&self.origin, &context.read_origin)
            || context.barrier.run() != self.run.0
            || !self.capture_origin.matches(context)
            || ports
                .lifecycle
                .leave_peer
                .as_deref()
                .is_none_or(|peer| !peer.owns_capture_identity(&self.peer))
        {
            return Err(fault(
                context,
                ports.io,
                "capture lost original run, Leave or peer",
            ));
        }
        // Preserve the first fatal cause before probing another socket. The
        // member alone handles its recoverable result and cleanup deadlines.
        if let Some(failure) = context.failure.as_ref() {
            return Err(failure.clone());
        }
        if self.stage == Stage::Complete {
            return Ok(ServingCaptureProgress::Published);
        }
        if self.stage == Stage::RefusalReady {
            return Ok(ServingCaptureProgress::Refused);
        }
        if self.complete_sent(context, ports)? {
            return Ok(ServingCaptureProgress::Published);
        }
        if self.queued()
            && let Err(failure) = context.live(ports.io)
            && !self.retain_queued_expiry(context, &failure)
        {
            return Err(failure);
        }
        self.probe(context, ports)?;
        // A queued save may share the running turn's context, but never its
        // author slot. Let that actual earlier owner continue instead of
        // installing a competing held original or taking its completion.
        if self.queued()
            && (!self.requested.started || self.requested.settled)
            && (!self.refused_record.started || self.refused_record.settled)
            && !context.serving_save_observation_author_available()
        {
            return Ok(ServingCaptureProgress::Pending);
        }
        if self.cancelling {
            // A disconnected admin is retained by the containing owner. It
            // cannot repeatedly fail the pump while cleanup drains its trace.
            let peer = if self.disconnected {
                ports.lifecycle.leave_peer.take()
            } else {
                None
            };
            let result = self.cancel(context, ports);
            if let Some(peer) = peer {
                ports.lifecycle.leave_peer = Some(peer);
            }
            return result;
        }
        // The member owner alone handles its recoverable result expiry and
        // cleanup. Do not convert that timeout into a component halt here.
        if !matches!(
            self.stage,
            Stage::Barrier | Stage::Acknowledge | Stage::Release
        ) && let Err(failure) = context.live(ports.io)
        {
            if self.retain_window_expiry(context, &failure) {
                return Ok(ServingCaptureProgress::Progress);
            }
            return Err(failure);
        }
        let id = self.beginning.capture_id;
        let result = self.step(context, ports, id);
        if let Err(failure) = &result
            && self.retain_window_expiry(context, failure)
        {
            return Ok(ServingCaptureProgress::Progress);
        }
        match result {
            Err(RunFailure::Transport {
                source: PumpSource::LeavePeer,
                failure,
            }) if failure.fault == crate::ChannelFault::Closed => {
                if self.commit_sent() {
                    Err(context.fail(
                        ports.io,
                        FlowHaltReason::CarryFailed,
                        HaltPoint::Barrier,
                        format!("capture after Commit: {failure:?}"),
                    ))
                } else {
                    self.disconnected = true;
                    self.cancelling = true;
                    self.refusal_code
                        .get_or_insert_with(|| "admin_disconnected".into());
                    self.refusal_leg.get_or_insert(SavePointLeg::Published);
                    Ok(ServingCaptureProgress::Progress)
                }
            }
            result => result,
        }
    }

    fn step<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        id: shared::DecimalU64,
    ) -> Result<ServingCaptureProgress, RunFailure> {
        match self.stage {
            Stage::Requested => {
                if matches!(self.capture_origin, CaptureOrigin::Demand { .. }) {
                    self.requested.advance_queued(
                        context,
                        ports,
                        self.payload(Stage::Requested),
                    )?;
                } else {
                    self.requested
                        .advance(context, ports, self.payload(Stage::Requested))?;
                }
                self.stage = match self.capture_origin {
                    CaptureOrigin::Demand { .. } if self.queued_expired => Stage::QueueRefused,
                    CaptureOrigin::Demand { .. } => Stage::QueueBoundary,
                    CaptureOrigin::Leave { .. } => Stage::Beginning,
                };
            }
            Stage::QueueBoundary => {
                if self.queued_expired {
                    self.stage = Stage::QueueRefused;
                } else {
                    return Ok(ServingCaptureProgress::Pending);
                }
            }
            Stage::QueueRefused => {
                self.refused_record
                    .advance_queued(context, ports, self.payload(Stage::Refused))?;
                self.stage = Stage::RefusalReady;
                return Ok(ServingCaptureProgress::Refused);
            }
            Stage::Beginning => {
                if !self.carry(0, context, ports)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                self.stage = Stage::Barrier;
            }
            Stage::Barrier => {
                if self.frontier.is_none() {
                    context.prepare_member_capture(id, self.beginning.window, ports.io)?;
                    self.frontier = Some(
                        context
                            .capture_snapshot()
                            .expect("registered barrier")
                            .frontier,
                    );
                }
                let progress = context.advance_member_capture(ports.io)?;
                if self.refused_member(context, ports.io)? {
                    return Ok(ServingCaptureProgress::Progress);
                }
                if progress == CaptureProgress::ImageReady {
                    if let Some(answer) = context.take_capture_answer(ports.io)? {
                        let shared::StateFrame::ControlAnswer {
                            frame:
                                shared::StateControlAnswer::Snapshot {
                                    capture_id,
                                    room_name,
                                    size_bytes,
                                    position,
                                },
                            ..
                        } = &answer.answer
                        else {
                            self.member_results.push(answer);
                            return Err(fault(context, ports.io, "capture expected snapshot"));
                        };
                        let image = CaptureImage {
                            capture_id: *capture_id,
                            room_name: room_name.clone(),
                            size_bytes: *size_bytes,
                            position: position.clone(),
                        };
                        self.carries[1] = Some(Carry::new(
                            &self.exchange,
                            CapturePayload::SavePointImage(image.clone()),
                        ));
                        self.image = Some(image);
                        self.member_results.push(answer);
                        self.stage = Stage::Image;
                    }
                } else if progress == CaptureProgress::Pending {
                    return Ok(ServingCaptureProgress::Pending);
                }
            }
            Stage::Image => {
                if !self.carry(1, context, ports)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                self.stage = Stage::AwaitChecked;
            }
            Stage::AwaitChecked => {
                if !self.receive(context, ports, 0)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                let CapturePayload::SavePointImageChecked(checked) =
                    &self.incoming[0].as_ref().unwrap().message.envelope.payload
                else {
                    unreachable!()
                };
                context.acknowledge_member_capture(id, checked.dump_sha256.clone(), ports.io)?;
                self.stage = Stage::Acknowledge;
            }
            Stage::Acknowledge => {
                let progress = context.advance_member_capture(ports.io)?;
                if self.refused_member(context, ports.io)? {
                    return Ok(ServingCaptureProgress::Progress);
                }
                if progress == CaptureProgress::FinishedReady {
                    if let Some(answer) = context.take_capture_answer(ports.io)? {
                        let shared::StateFrame::ControlAnswer {
                            frame:
                                shared::StateControlAnswer::Finished {
                                    capture_id,
                                    room_name,
                                },
                            ..
                        } = &answer.answer
                        else {
                            self.member_results.push(answer);
                            return Err(fault(context, ports.io, "capture expected finished"));
                        };
                        self.carries[2] = Some(Carry::new(
                            &self.exchange,
                            CapturePayload::SavePointImageFinished(CaptureImageFinished {
                                capture_id: *capture_id,
                                room_name: room_name.clone(),
                            }),
                        ));
                        self.member_results.push(answer);
                        self.stage = Stage::Finished;
                    }
                } else if progress == CaptureProgress::Pending {
                    return Ok(ServingCaptureProgress::Pending);
                }
            }
            Stage::Finished => {
                if !self.carry(2, context, ports)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                self.stage = Stage::AwaitPrepared;
            }
            Stage::AwaitPrepared => {
                if !self.receive(context, ports, 1)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                let CapturePayload::SavePointPrepared(artifact) =
                    &self.incoming[1].as_ref().unwrap().message.envelope.payload
                else {
                    unreachable!()
                };
                let CapturePayload::SavePointImageChecked(checked) =
                    &self.incoming[0].as_ref().unwrap().message.envelope.payload
                else {
                    unreachable!()
                };
                if artifact.dump_sha256 != checked.dump_sha256 || !artifact_identity(artifact) {
                    return Err(fault(
                        context,
                        ports.io,
                        "Prepared changed its image digest or artifact identity",
                    ));
                }
                self.artifact = Some(artifact.clone());
                self.stage = Stage::Confirmed;
            }
            Stage::Confirmed => {
                if !self.confirmed.started
                    && !context.confirmable_member_capture(id, self.frontier.unwrap(), ports.io)?
                {
                    return Ok(ServingCaptureProgress::Pending);
                }
                self.confirmed
                    .advance(context, ports, self.payload(Stage::Confirmed))?;
                self.carries[3] = Some(Carry::new(
                    &self.exchange,
                    CapturePayload::SavePointCommit(CaptureIdentity { capture_id: id }),
                ));
                self.stage = Stage::Commit;
            }
            Stage::Commit => {
                if !context.confirmable_member_capture(id, self.frontier.unwrap(), ports.io)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                if !self.carry(3, context, ports)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                self.stage = Stage::AwaitPublished;
            }
            Stage::AwaitPublished => {
                if !self.receive(context, ports, 2)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                let CapturePayload::SavePointPublished(published) =
                    &self.incoming[2].as_ref().unwrap().message.envelope.payload
                else {
                    unreachable!()
                };
                if Some(published) != self.artifact.as_ref() {
                    return Err(fault(
                        context,
                        ports.io,
                        "Published changed Prepared A/B or artifact",
                    ));
                }
                self.published = Some(StartIdentity {
                    seed_id: self.loaded.seed_id.clone(),
                    artifact_id: published.artifact_id.clone(),
                    dump_sha256: published.dump_sha256.clone(),
                    config_sha256: published.config_sha256.clone(),
                    parent_artifact_id: Some(self.loaded.artifact_id.clone()),
                    position: self.image.as_ref().unwrap().position.clone(),
                });
                self.stage = Stage::Published;
            }
            Stage::Published => {
                self.published_record
                    .advance(context, ports, self.payload(Stage::Published))?;
                self.stage = Stage::Release;
            }
            Stage::Release => {
                if !self.release_started {
                    context.release_member_capture(id, ports.io)?;
                    self.release_started = true;
                }
                if context.advance_member_capture(ports.io)? == CaptureProgress::Complete {
                    if let Some(answer) = context.take_capture_answer(ports.io)? {
                        self.member_results.push(answer);
                        let payload = match self.capture_origin {
                            CaptureOrigin::Leave { .. } => {
                                CapturePayload::SavePointCaptureComplete(CaptureIdentity {
                                    capture_id: id,
                                })
                            }
                            CaptureOrigin::Demand { .. } => {
                                CapturePayload::Answer(LifecycleCaptureAnswer::SavePointTaken {
                                    report: self.published_report(),
                                })
                            }
                        };
                        self.carries[4] = Some(Carry::new(&self.exchange, payload));
                        self.stage = Stage::CompleteCarry;
                    }
                } else {
                    return Ok(ServingCaptureProgress::Pending);
                }
            }
            Stage::CompleteCarry => {
                if !self.carry(4, context, ports)? {
                    return Ok(ServingCaptureProgress::Pending);
                }
                assert!(self.complete_sent(context, ports)?);
                return Ok(ServingCaptureProgress::Published);
            }
            Stage::Refused => {
                self.refused_record
                    .advance(context, ports, self.payload(Stage::Refused))?;
                context.finish_member_capture(id, ports.io)?;
                self.stage = Stage::RefusalReady;
                return Ok(ServingCaptureProgress::Refused);
            }
            Stage::Complete | Stage::RefusalReady | Stage::CancelRecord | Stage::CancelMember => {
                unreachable!()
            }
        }
        Ok(ServingCaptureProgress::Progress)
    }

    fn refused_member(
        &mut self,
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if context
            .capture_snapshot()
            .is_none_or(|value| value.phase != CapturePhase::Complete)
            || (context.capture_refusal().is_none() && context.capture_timeout().is_none())
        {
            return Ok(false);
        }
        let Some(answer) = context.take_capture_answer(io)? else {
            return Ok(false);
        };
        self.member_results.push(answer);
        self.member_refusal = context.take_recorded_capture_refusal(self.beginning.capture_id);
        self.refusal_leg = Some(if self.stage == Stage::Barrier {
            SavePointLeg::Answer
        } else {
            SavePointLeg::Finished
        });
        self.refusal_code = Some(if let Some(refusal) = self.member_refusal.as_ref() {
            match &refusal.answer {
                shared::StateFrame::ControlAnswer {
                    frame: shared::StateControlAnswer::ControlRefused { code, .. },
                    ..
                } => serde_json::to_value(code)
                    .expect("closed refusal code")
                    .as_str()
                    .unwrap()
                    .to_owned(),
                _ => {
                    return Err(fault(
                        context,
                        io,
                        "capture refusal changed its typed result",
                    ));
                }
            }
        } else if context.capture_timeout().unwrap().phase == CapturePhase::SnapshotPending {
            "snapshot_timeout".into()
        } else {
            "finished_timeout".into()
        });
        self.stage = Stage::Refused;
        Ok(true)
    }
}
