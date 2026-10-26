//! The capture interval of an already accepted forced Leave, after actual Lower.
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
use weaver_types::state_flow::capture::*;
use weaver_types::{ExchangeId, Opener, Position, RunId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Requested,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LeaveCaptureProgress {
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
                position: Position::Continue,
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
}
impl Record {
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
            context.resume_author_and_wait(ports)
        } else {
            self.started = true;
            context.author_and_wait(
                ports,
                AuthoredEvent {
                    kind: Kind::SavePoint,
                    subsystem: Subsystem::Harness,
                    turn: None,
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(Payload::SavePointRecord(Box::new(payload))),
                },
            )
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
                self.sequence = context.completed_author;
                Err(self.returned.retain_started(error, &mut self.started))
            }
        }
    }
}

pub(super) struct LeaveCapture {
    origin: Arc<()>,
    peer: PortIdentity,
    exchange: ExchangeId,
    run: RunId,
    loaded: StartIdentity,
    beginning: CaptureBeginning,
    window: AbsoluteBound,
    leave: AbsoluteBound,
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
    release_started: bool,
    cancelling: bool,
    disconnected: bool,
}

/// A latched capture failure with its exact lifecycle descriptor retired. No
/// caller can update a send ticket after these phase facts are frozen.
pub(super) struct FailedCapture {
    owner: LeaveCapture,
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
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.owner.leave)
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

impl LeaveCapture {
    #[allow(clippy::result_large_err)] // A refusal returns both exact move-owned resources.
    pub(super) fn into_failed(
        self,
        context: &RecordingContext,
        peer: LifecyclePeer,
    ) -> Result<Box<FailedCapture>, (Self, LifecyclePeer)> {
        if context.failure.is_none()
            || context.flow.halt().is_none()
            || !Arc::ptr_eq(&self.origin, &context.read_origin)
            || context.barrier.run() != self.run.0
            || context.deadlines.get(PumpDeadline::Leave) != Some(self.leave)
            || !peer.owns_capture_identity(&self.peer)
        {
            return Err((self, peer));
        }
        let facts = self.at_halt(context);
        Ok(Box::new(FailedCapture {
            owner: self,
            _peer: peer.retire_failed(),
            facts,
        }))
    }
    /// The containing lifecycle owner proves Ready, the real forced directive,
    /// settled input and actual Lower/reap. This internal constructor cannot
    /// return a serving seat, a release seal, or a successful Leave outcome.
    pub(super) fn begin(
        context: &mut RecordingContext,
        peer: &LifecyclePeer,
        exchange: ExchangeId,
        leave: AbsoluteBound,
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
            || exchange.opener != Opener::Admin
            || context.open_turn.is_some()
            || context.input.snapshot().is_some()
        {
            return Err(fault(
                context,
                io,
                "forced capture requires its settled Leave boundary",
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
        let mut carries = std::array::from_fn(|_| None);
        carries[0] = Some(Carry::new(
            &exchange,
            CapturePayload::SavePointBeginning(beginning.clone()),
        ));
        Ok(Self {
            origin: Arc::clone(&context.read_origin),
            peer,
            run: RunId(context.barrier.run().into()),
            exchange,
            loaded,
            beginning,
            window,
            leave,
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
            release_started: false,
            cancelling: false,
            disconnected: false,
        })
    }

    pub(super) fn report(&self) -> Option<shared::SavePointReport> {
        (self.stage == Stage::Complete).then(|| shared::SavePointReport {
            start: self.published.clone().expect("actual Published"),
            capture_id: self.beginning.capture_id,
            event_run: self.run.clone(),
            event_sequence: self
                .confirmed
                .sequence
                .expect("confirmed Recorded")
                .0
                .into(),
        })
    }

    fn commit_sent(&self) -> bool {
        self.carries[3].as_ref().is_some_and(Carry::sent)
    }

    pub(super) fn at_halt(&self, context: &RecordingContext) -> CaptureAtHalt {
        CaptureAtHalt {
            capture_id: self.beginning.capture_id,
            state: match self.stage {
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
                Stage::Refused
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
        let cause = trace::PersistenceCause::ForcedUnload;
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
        if self.cancelling {
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
            self.refusal_code
                .get_or_insert_with(|| aborted.code.clone());
            self.aborted = peer.take_capture().expect("held complete packet");
            self.cancelling = true;
            return Ok(());
        }
        let expected = match self.stage {
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
        Ok(())
    }

    fn cancel<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<LeaveCaptureProgress, RunFailure> {
        // Finish the exact already-admitted author before starting its refusal.
        // A retained original is never replaced by a fresh equivalent event.
        if self.requested.started && !self.requested.settled {
            self.requested
                .advance(context, ports, self.payload(Stage::Requested))?;
        }
        if self.confirmed.started && !self.confirmed.settled {
            self.confirmed
                .advance(context, ports, self.payload(Stage::Confirmed))?;
        }
        if let Some(peer) = ports.lifecycle.leave_peer.as_deref_mut() {
            for carry in self.carries.iter_mut().flatten() {
                if let Some(ticket) = carry.send.as_ref()
                    && !ticket.sent()
                    && !ticket.withdrawn()
                {
                    let returned = peer.withdraw_capture_pending(ticket).map_err(|error| {
                        fault(
                            context,
                            ports.io,
                            format!("capture cancellation: {error:?}"),
                        )
                    })?;
                    if returned.as_ref() != Some(&carry.envelope) {
                        return Err(fault(
                            context,
                            ports.io,
                            "capture cancellation lost original unsent packet",
                        ));
                    }
                    carry.returned = returned;
                }
            }
        }
        if !matches!(self.stage, Stage::CancelRecord | Stage::CancelMember) {
            self.stage = Stage::CancelRecord;
        }
        if self.stage == Stage::CancelRecord {
            self.refused_record
                .advance(context, ports, self.payload(Stage::Refused))?;
            self.stage = Stage::CancelMember;
        }
        if context.member_capture.is_none() {
            context.deadlines.disarm(PumpDeadline::CaptureWindow);
            self.stage = Stage::RefusalReady;
            return Ok(LeaveCaptureProgress::Refused);
        }
        context.cancel_member_capture(self.beginning.capture_id, ports.io)?;
        if context.member_capture.is_none() {
            self.stage = Stage::RefusalReady;
            return Ok(LeaveCaptureProgress::Refused);
        }
        if context.advance_member_capture(ports.io)? == CaptureProgress::Complete
            && let Some(answer) = context.take_capture_answer(ports.io)?
        {
            self.member_results.push(answer);
            self.member_refusal = context.take_recorded_capture_refusal(self.beginning.capture_id);
            context.finish_member_capture(self.beginning.capture_id, ports.io)?;
            self.stage = Stage::RefusalReady;
            return Ok(LeaveCaptureProgress::Refused);
        }
        Ok(LeaveCaptureProgress::Pending)
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
    ) -> Result<LeaveCaptureProgress, RunFailure> {
        if !Arc::ptr_eq(&self.origin, &context.read_origin)
            || context.barrier.run() != self.run.0
            || context.deadlines.get(PumpDeadline::Leave) != Some(self.leave)
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
            return Ok(LeaveCaptureProgress::Published);
        }
        if self.stage == Stage::RefusalReady {
            return Ok(LeaveCaptureProgress::Refused);
        }
        if self.complete_sent(context, ports)? {
            return Ok(LeaveCaptureProgress::Published);
        }
        self.probe(context, ports)?;
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
        ) {
            context.live(ports.io)?;
        }
        let id = self.beginning.capture_id;
        let result = self.step(context, ports, id);
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
                    Ok(LeaveCaptureProgress::Progress)
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
    ) -> Result<LeaveCaptureProgress, RunFailure> {
        match self.stage {
            Stage::Requested => {
                self.requested
                    .advance(context, ports, self.payload(Stage::Requested))?;
                self.stage = Stage::Beginning;
            }
            Stage::Beginning => {
                if !self.carry(0, context, ports)? {
                    return Ok(LeaveCaptureProgress::Pending);
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
                    return Ok(LeaveCaptureProgress::Progress);
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
                    return Ok(LeaveCaptureProgress::Pending);
                }
            }
            Stage::Image => {
                if !self.carry(1, context, ports)? {
                    return Ok(LeaveCaptureProgress::Pending);
                }
                self.stage = Stage::AwaitChecked;
            }
            Stage::AwaitChecked => {
                if !self.receive(context, ports, 0)? {
                    return Ok(LeaveCaptureProgress::Pending);
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
                    return Ok(LeaveCaptureProgress::Progress);
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
                    return Ok(LeaveCaptureProgress::Pending);
                }
            }
            Stage::Finished => {
                if !self.carry(2, context, ports)? {
                    return Ok(LeaveCaptureProgress::Pending);
                }
                self.stage = Stage::AwaitPrepared;
            }
            Stage::AwaitPrepared => {
                if !self.receive(context, ports, 1)? {
                    return Ok(LeaveCaptureProgress::Pending);
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
                    return Ok(LeaveCaptureProgress::Pending);
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
                    return Ok(LeaveCaptureProgress::Pending);
                }
                if !self.carry(3, context, ports)? {
                    return Ok(LeaveCaptureProgress::Pending);
                }
                self.stage = Stage::AwaitPublished;
            }
            Stage::AwaitPublished => {
                if !self.receive(context, ports, 2)? {
                    return Ok(LeaveCaptureProgress::Pending);
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
                        self.carries[4] = Some(Carry::new(
                            &self.exchange,
                            CapturePayload::SavePointCaptureComplete(CaptureIdentity {
                                capture_id: id,
                            }),
                        ));
                        self.stage = Stage::CompleteCarry;
                    }
                } else {
                    return Ok(LeaveCaptureProgress::Pending);
                }
            }
            Stage::CompleteCarry => {
                if !self.carry(4, context, ports)? {
                    return Ok(LeaveCaptureProgress::Pending);
                }
                assert!(self.complete_sent(context, ports)?);
                return Ok(LeaveCaptureProgress::Published);
            }
            Stage::Refused => {
                self.refused_record
                    .advance(context, ports, self.payload(Stage::Refused))?;
                context.finish_member_capture(id, ports.io)?;
                self.stage = Stage::RefusalReady;
                return Ok(LeaveCaptureProgress::Refused);
            }
            Stage::Complete | Stage::RefusalReady | Stage::CancelRecord | Stage::CancelMember => {
                unreachable!()
            }
        }
        Ok(LeaveCaptureProgress::Progress)
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
