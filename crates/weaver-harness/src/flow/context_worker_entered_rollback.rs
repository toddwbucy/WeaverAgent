//! An actual Leave consumes the successful Ready seat and its run.
//! Rollback and accepted Leave share private Lower, settlement and release steps.
//! Non-rollback Leave inserts capture; this owner grants no wind-down,
//! renewed Enter or replacement input.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::{RecordingPhase, RunFailure};
use crate::flow::coordinator::FailedFlowResources;
use crate::flow::pump::PumpDeadline;
use crate::flow::pump_transport::OrganSend;
use std::sync::Arc;

/// Only checked successful-Ready cleanup issues this release seal. Forced Leave
/// additionally requires its completed capture before reaching this step.
/// The original Enter may still be live in wall time; Ready retired it already.
pub(in crate::flow::context) struct EnteredRollbackReleasePermit {
    origin: Arc<()>,
    enter: AbsoluteBound,
    leave: AbsoluteBound,
    ready: OrganSend,
    exchange: weaver_types::ExchangeId,
}

impl EnteredRollbackReleasePermit {
    pub(in crate::flow::context) fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.phase == RecordingPhase::Releasing
            && context.retired_enter == Some(self.enter)
            && context.deadlines.get(PumpDeadline::Enter).is_none()
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && self.ready.sent()
            && self.ready.matches_exchange(&self.exchange)
            && self
                .ready
                .sent_at()
                .is_some_and(|at| self.original_send(at))
            && !self.leave.expired(context.last_ns)
            && context.failure.is_none()
            && context.flow.halt().is_none()
    }
    pub(in crate::flow::context) fn original_send(&self, at: u64) -> bool {
        at >= self.enter.began_ns() && !self.enter.expired(at)
    }
    pub(in crate::flow::context) fn release_send(&self, at: u64) -> bool {
        at >= self.leave.began_ns() && !self.leave.expired(at)
    }
    pub(in crate::flow::context) fn deadline(&self) -> u64 {
        self.leave.deadline_ns()
    }
}

use super::input::rollback::RollbackInputWork;
use crate::flow::context::failed_enter_driver::expired_residency::RollbackResidencyRelease;
use crate::flow::context::failed_enter_driver::{FailedEnterProgress, FailedRunCleanup};
use crate::flow::context::startup_composition::refusal::response::listener::{
    RetiredRollbackDials, RollbackDials,
};
use crate::flow::context::worker_startup::expired::gate::{
    GateRollbackEvidence, GateRollbackHost, RollbackGate,
};
use crate::flow::context::worker_startup::failed::StartupGate;
use crate::flow::context::*;
use crate::flow::pump::{
    LifecycleMessage, LifecyclePeer, PumpPorts, PumpSource, RetiredLifecyclePeer,
};
use crate::flow::pump_transport::{PortIdentity, ReceivedOrgan};
use crate::flow::templates::TemplateCertificate;
use std::collections::{HashSet, VecDeque};
use weaver_trace::GrantSurface;
use weaver_types::OrganEnvelope;

#[path = "context_worker_entered_rollback_terminal.rs"]
mod terminal;
use crate::flow::context::leave_capture::{ServingCapture, ServingCaptureProgress};
use terminal::EnteredRollbackTerminal;

#[path = "context_worker_resting.rs"]
mod resting;
use resting::RestingWork;

#[path = "context_worker_forced_leave.rs"]
pub mod forced;

#[path = "context_worker_graceful_exit.rs"]
pub mod graceful_exit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LeaveKind {
    Rollback,
    Forced,
    Graceful,
}
impl LeaveKind {
    fn captures(self) -> bool {
        match self {
            Self::Rollback => false,
            Self::Forced | Self::Graceful => true,
        }
    }
    pub(super) fn accepts(self, payload: &weaver_types::Payload) -> bool {
        matches!(
            (self, payload),
            (
                Self::Rollback,
                weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                    rollback: true,
                    ..
                })
            ) | (
                Self::Forced,
                weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                    forced: true,
                    rollback: false,
                    ..
                })
            ) | (
                Self::Graceful,
                weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                    forced: false,
                    rollback: false,
                    ..
                })
            )
        )
    }
}

/// A successful Ready and one actual rollback directive, after ending the run
/// loans. This is neither a release receipt nor permission to use another run.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationEnteredRollbackHandoff>();
/// ```
pub struct FoundationEnteredRollbackHandoff {
    pub(super) kind: LeaveKind,
    pub(super) _plan: Option<Box<super::input::plan::PendingPlan>>,
    pub(super) _flags: Option<Box<super::input::flags::FoundationFlags>>,
    pub(super) graceful_tail: Option<Box<super::input::graceful::GracefulTail>>,
    pub(super) origin: Arc<()>,
    pub(super) decode: PortIdentity,
    pub(super) residency: PortIdentity,
    pub(super) enter: AbsoluteBound,
    pub(super) leave: AbsoluteBound,
    pub(super) resident_origin: Arc<()>,
    pub(super) resident_enter: AbsoluteBound,
    pub(super) startup: crate::flow::context::startup_composition::StartupEvidence,
    pub(super) residency_request: crate::flow::context::residency_composition::ResidencyRequest,
    pub(super) admit_reply: Option<ReceivedOrgan>,
    pub(super) certificate: Option<TemplateCertificate>,
    pub(super) listener: Option<LifecycleListener>,
    pub(super) arms: StartupGate,
    pub(super) gate_request: GateStartupRequest,
    pub(super) gate_raise: OrganEnvelope,
    pub(super) gate_send: OrganSend,
    pub(super) gate_reply: ReceivedOrgan,
    pub(super) ready: Option<OrganSend>,
    pub(super) answer: OrganEnvelope,
    pub(super) gate_exchanges: HashSet<u64>,
    pub(super) queued_inputs: VecDeque<ReceivedOrgan>,
    pub(super) gate_outgoing: u64,
    pub(super) input: RollbackInputWork,
    pub(super) peer: Option<LifecyclePeer>,
    pub(super) message: LifecycleMessage,
    pub(super) peer_unavailable: bool,
    pub(super) detail: Option<String>,
}

pub(super) fn rollback_context(context: &RecordingContext) -> bool {
    context.phase == RecordingPhase::Entered
        && context.flow.phase() == CoordinatorPhase::Active
        && context.barrier.is_active()
        && context.failure.is_none()
        && context.flow.halt().is_none()
        && context.member_capture.is_none()
}

fn preceding_control<'a>(
    tail: &'a Option<Box<super::input::graceful::GracefulTail>>,
    input: &'a RollbackInputWork,
) -> Option<(&'a OrganEnvelope, &'a OrganSend)> {
    tail.as_ref()
        .and_then(|tail| tail.quiesce.as_ref())
        .and_then(|work| work.control())
        .or_else(|| input.cancel_control())
}

impl FoundationEnteredRollbackHandoff {
    fn original(&self) -> GateRollbackEvidence<'_> {
        GateRollbackEvidence {
            raise: &self.gate_raise,
            send: Some(&self.gate_send),
            reply: Some(&self.gate_reply),
            gate_enter: Some(self.enter),
            enter: self.enter,
            current_outgoing: self.gate_outgoing,
            control: preceding_control(&self.graceful_tail, &self.input),
        }
    }
    fn cause(&self) -> weaver_types::Cause {
        if self.kind.captures() {
            let weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                cause,
                ..
            }) = self.message.envelope.payload
            else {
                unreachable!("checked original Leave")
            };
            return cause;
        }
        self.startup
            .original_enter
            .as_ref()
            .expect("original successful Enter")
            .envelope()
            .enter()
            .cause
    }
    fn matches(&self, run: &FoundationRun) -> bool {
        self.kind.accepts(&self.message.envelope.payload)
            && (self.kind == LeaveKind::Graceful) == self.graceful_tail.is_some()
            && Arc::ptr_eq(&self.origin, &run.context.read_origin)
            && Arc::ptr_eq(&self.resident_origin, &self.origin)
            && self.resident_enter == self.enter
            && rollback_context(&run.context)
            && run.context.retired_enter == Some(self.enter)
            && run.context.deadlines.get(PumpDeadline::Enter).is_none()
            && run.context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && run.resources.matches(&run.context)
            && run.resources.original_ports()
            && run
                .resources
                .decode_present_for_serving()
                .is_some_and(|port| self.decode.matches_decode(port))
            && run
                .resources
                .residency_port()
                .is_some_and(|port| self.residency.matches(port))
            && run.original.is_none()
            && run.listener.is_none()
            && self.listener.is_some()
            && self.peer.is_some()
            && self.ready.as_ref().is_some_and(|ready| {
                ready.sent()
                    && ready
                        .sent_at()
                        .is_some_and(|at| at >= self.enter.began_ns() && !self.enter.expired(at))
                    && self
                        .startup
                        .original_enter
                        .as_ref()
                        .is_some_and(|original| {
                            original.adopted_by(&run.context)
                                && original.peer().owns_send(ready)
                                && ready.matches_exchange(&original.envelope().exchange)
                                && self.answer == original.ready_envelope()
                        })
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationEnteredRollbackProgress {
    Progress,
    Pending,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanupStage {
    Gate,
    Capture,
    RefusedAnswer,
    CaptureRefused,
    Grants,
    Seal,
    Release,
    Classifier,
    Child,
    Spu,
    State,
    Terminal,
    Closed,
}

/// Owns the original run and Ready through actual rollback and failed cleanup.
/// A closed owner retains its evidence; it never grants a replacement serving seat.
pub struct FoundationEnteredRollback {
    run: Option<Box<FoundationRun>>,
    handoff: Box<FoundationEnteredRollbackHandoff>,
    stage: CleanupStage,
    progress: FoundationEnteredRollbackProgress,
    dials: Option<RollbackDials>,
    retired_dials: Option<RetiredRollbackDials>,
    retired_peer: Option<RetiredLifecyclePeer>,
    gate: Option<RollbackGate>,
    inspecting: bool,
    current: Option<InspectionCompletion>,
    surface: Option<GrantSurface>,
    release: Option<RollbackResidencyRelease>,
    terminal: Option<EnteredRollbackTerminal>,
    capture: Option<ServingCapture>,
    resting: RestingWork,
    retained_failed_capture: Option<Box<crate::flow::context::leave_capture::FailedCapture>>,
    failed: Option<FailedRunCleanup>,
    reported: bool,
    failed_grants_attempted: bool,
    gate_done: bool,
    sweep_reported: bool,
    abandoned: bool,
    detail: Option<String>,
    retired: Option<FailedFlowResources>,
    stopped_record: Option<crate::flow::recording::StoppedStartupRecord>,
}

impl FoundationRun {
    #[allow(clippy::result_large_err)]
    pub fn entered_rollback(
        self: Box<Self>,
        handoff: Box<FoundationEnteredRollbackHandoff>,
    ) -> Result<Box<FoundationEnteredRollback>, (Box<Self>, Box<FoundationEnteredRollbackHandoff>)>
    {
        if handoff.kind != LeaveKind::Rollback || !handoff.matches(&self) {
            return Err((self, handoff));
        }
        Ok(self.begin_leave_cleanup(handoff))
    }

    fn begin_leave_cleanup(
        self: Box<Self>,
        mut handoff: Box<FoundationEnteredRollbackHandoff>,
    ) -> Box<FoundationEnteredRollback> {
        let mut dials = RollbackDials::new(handoff.listener.take().unwrap());
        if let Some(tail) = &mut handoff.graceful_tail {
            let (forced, forced_by) = tail.outcome();
            dials.adopt_graceful(forced, forced_by, tail.take_joins());
        }
        Box::new(FoundationEnteredRollback {
            run: Some(self),
            handoff,
            stage: CleanupStage::Gate,
            progress: FoundationEnteredRollbackProgress::Progress,
            dials: Some(dials),
            retired_dials: None,
            retired_peer: None,
            gate: None,
            inspecting: false,
            current: None,
            surface: None,
            release: None,
            terminal: None,
            capture: None,
            resting: RestingWork::default(),
            retained_failed_capture: None,
            failed: None,
            reported: false,
            failed_grants_attempted: false,
            gate_done: false,
            sweep_reported: false,
            abandoned: false,
            detail: None,
            retired: None,
            stopped_record: None,
        })
    }
}

/// A terminal custody transfer is issued only at the accepted Leave cap or
/// after the actual Unload receipt. It permits retaining records, never authoring.
pub(in crate::flow) struct EnteredRollbackDisposal {
    barrier: Arc<()>,
}
impl EnteredRollbackDisposal {
    pub(in crate::flow) fn matches_record(&self, barrier: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.barrier, barrier)
    }
}

impl FoundationEnteredRollback {
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_account(
        &self,
    ) -> (&FoundationRun, &FoundationEnteredRollbackHandoff) {
        assert!(
            matches!(self.stage, CleanupStage::Gate) || self.handoff.arms.port.is_none(),
            "a retired Gate must not be polled in later rollback stages"
        );
        (
            self.run.as_deref().expect("healthy retained run"),
            &self.handoff,
        )
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_input(&self) -> &RollbackInputWork {
        &self.handoff.input
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_gate(&self) -> Option<&RollbackGate> {
        self.gate.as_ref()
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_context(&self) -> &RecordingContext {
        self.run.as_ref().map_or_else(
            || self.failed.as_ref().unwrap().context(),
            |run| &run.context,
        )
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_end_resting_spu(&mut self) {
        assert_eq!(self.stage, CleanupStage::CaptureRefused);
        self.run.as_mut().unwrap().resources.end_spu_for_test();
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_failed(
        &self,
    ) -> Option<&FailedRunCleanup> {
        self.failed.as_ref()
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_before_unload(&self) -> bool {
        self.stage == CleanupStage::Terminal && self.unload_sequence().is_none()
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_at_grants(&self) -> bool {
        self.stage == CleanupStage::Grants
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_join_count(&self) -> usize {
        self.dials
            .as_ref()
            .map_or(0, RollbackDials::test_join_count)
    }
    #[cfg(test)]
    pub(in crate::flow::context::worker_startup) fn test_reap(&mut self) {
        if let Some(child) = &mut self.handoff.arms.child {
            super::tests::reap(child);
        }
        if let Some(run) = &mut self.run {
            run.resources.reap_startup_test_children();
            run.context.barrier.abandon();
        }
        if let Some(tail) = &mut self.failed {
            tail.reap_composition_test_children();
            tail.context.barrier.abandon();
        }
    }
    pub fn progress(&self) -> FoundationEnteredRollbackProgress {
        self.progress
    }
    /// Report process disposition only after the original cleanup owner closes.
    /// A terminal refusal, first failure or abandoned cleanup cannot claim the
    /// ordinary exit even when an Unload sequence was retained.
    pub fn process_exit(&self) -> Option<crate::FoundationProcessExit> {
        if self.progress != FoundationEnteredRollbackProgress::Closed {
            return None;
        }
        Some(
            self.terminal
                .as_ref()
                .filter(|_| {
                    self.failed.is_none()
                        && !self.abandoned
                        && self.run.as_ref().is_some_and(|run| {
                            run.context.failure.is_none() && run.context.flow.halt().is_none()
                        })
                })
                .and_then(EnteredRollbackTerminal::process_exit)
                .unwrap_or(crate::FoundationProcessExit::Failure),
        )
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn abandoned(&self) -> bool {
        self.abandoned
            || self
                .failed
                .as_ref()
                .is_some_and(FailedRunCleanup::abandoned)
    }
    pub fn unload_sequence(&self) -> Option<u64> {
        self.terminal
            .as_ref()
            .and_then(EnteredRollbackTerminal::sequence)
            .or_else(|| {
                self.failed
                    .as_ref()
                    .and_then(FailedRunCleanup::unload_sequence)
            })
            .map(|s| s.0)
    }
    pub fn channel_fault(&self) -> Option<crate::ChannelFault> {
        (self.progress == FoundationEnteredRollbackProgress::Closed)
            .then_some(crate::ChannelFault::Closed)
    }
    pub fn advance(&mut self) -> FoundationEnteredRollbackProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    pub fn wait_for_progress(&mut self) -> FoundationEnteredRollbackProgress {
        self.wait_with_io(&mut MonotonicIo)
    }

    fn close_dials(&mut self) {
        if let Some(command) = self.resting.command.take() {
            self.resting.retired_command = Some(command.retire());
        }
        if let Some(dials) = self.dials.take() {
            self.retired_dials = Some(dials.retire_failed());
        }
        if let Some(peer) = self.handoff.peer.take() {
            self.retired_peer = Some(peer.retire_failed());
        }
    }
    fn closed(&mut self) {
        self.close_dials();
        self.stage = CleanupStage::Closed;
        self.progress = FoundationEnteredRollbackProgress::Closed;
    }
    fn dispose(&mut self) {
        let run = self.run.as_mut().expect("healthy terminal custody");
        assert!(
            self.handoff.leave.expired(run.context.last_ns)
                || self
                    .terminal
                    .as_ref()
                    .and_then(EnteredRollbackTerminal::sequence)
                    .is_some()
        );
        let permit = EnteredRollbackDisposal {
            barrier: run.context.barrier.author_identity(),
        };
        self.stopped_record = Some(run.context.barrier.retain_entered_rollback_record(&permit));
        self.retired = run
            .context
            .flow
            .retire_abandoned_serving_rollback_ports(&permit, &run.context.barrier);
        assert!(self.retired.is_some(), "original retained rollback custody");
        run.resources.close_at_cap();
        if let Some(gate) = &mut self.gate {
            gate.dispose(&mut self.handoff.arms);
        } else {
            let mut gate = RollbackGate::failed_new(
                self.handoff.original(),
                &self.handoff.arms,
                Some(self.handoff.leave),
                None,
                run.context.last_ns,
                false,
            );
            gate.failed_dispose(&mut self.handoff.arms);
            self.gate = Some(gate);
        }
        run.context.barrier.abandon();
        run.context.phase = RecordingPhase::Closed;
        self.abandoned = true;
        self.closed();
    }

    fn clock(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let run = self.run.as_mut().unwrap();
        if run.context.phase == RecordingPhase::Closed {
            let now = io.now().map_err(|_| RunFailure::Closed)?;
            if now < run.context.last_ns {
                return Err(RunFailure::Closed);
            }
            run.context.last_ns = now;
            if self.handoff.leave.expired(now) {
                return Err(RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound: self.handoff.leave,
                });
            }
            return Ok(());
        }
        let clock = match run.context.live(io) {
            Err(
                error @ RunFailure::Wait {
                    key: PumpDeadline::AdminRead,
                    ..
                },
            ) => self
                .dials
                .as_mut()
                .unwrap()
                .recover(&mut run.context, error, io),
            other => other,
        };
        match clock {
            Err(RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            }) if self.gate.as_ref().is_some_and(|gate| gate.bound() == bound) => Ok(()),
            other => other,
        }
    }

    pub(in crate::flow::context) fn advance_with_io(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationEnteredRollbackProgress {
        if self.stage == CleanupStage::Closed {
            return self.progress;
        }
        if self.failed.is_some() {
            return self.advance_failed(io);
        }
        self.progress = FoundationEnteredRollbackProgress::Progress;
        // The capture owner first reduces any timely whole CaptureComplete.
        // A delayed reduction must not re-expire that completed capture wait.
        let result = if self.stage == CleanupStage::Capture
            || (self.stage == CleanupStage::CaptureRefused && self.resting.save.is_some())
        {
            self.step(io)
        } else {
            self.clock(io).and_then(|()| self.step(io))
        };
        if let Err(error) = result {
            if self.stage == CleanupStage::CaptureRefused {
                match self.retain_resting_interrupt(&error, io) {
                    Ok(true) => self.progress = FoundationEnteredRollbackProgress::Progress,
                    Ok(false) => self.handle_error(error, io),
                    Err(error) => self.handle_error(error, io),
                }
            } else {
                self.handle_error(error, io);
            }
        }
        self.progress
    }

    fn step(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if self.stage == CleanupStage::CaptureRefused {
            return self.step_resting(io);
        }
        let run = self.run.as_mut().unwrap();
        let handoff = &mut self.handoff;
        let leave = handoff.leave;
        let dials = self.dials.as_mut().unwrap();
        if self.stage == CleanupStage::Gate {
            if self.gate.is_none() {
                // Only this checked consuming owner changes nested waits into
                // a yield to its exact gate. Expired startup keeps its existing
                // pump semantics and cannot acquire this serving authority.
                if let Some(port) = &handoff.arms.port {
                    run.context.rollback_gate_port = Some(port.port_identity());
                }
                let original = GateRollbackEvidence {
                    raise: &handoff.gate_raise,
                    send: Some(&handoff.gate_send),
                    reply: Some(&handoff.gate_reply),
                    gate_enter: Some(handoff.enter),
                    enter: handoff.enter,
                    current_outgoing: handoff.gate_outgoing,
                    control: preceding_control(&handoff.graceful_tail, &handoff.input),
                };
                self.gate = Some(RollbackGate::new(
                    &mut GateRollbackHost {
                        context: &mut run.context,
                        arms: &mut handoff.arms,
                        original,
                        leave,
                        peer: handoff.peer.as_mut().unwrap(),
                    },
                    io,
                )?);
            }
            let gate = self.gate.as_mut().unwrap();
            let wind_down_pending = handoff
                .graceful_tail
                .as_ref()
                .and_then(|tail| tail.wind_down.as_ref())
                .is_some_and(|work| !work.settled_for_leave());
            if let Some(received) = handoff.input.take_unoffered() {
                handoff.queued_inputs.push_front(received);
            }
            if let Some(received) = handoff.queued_inputs.pop_front()
                && let Err(received) = gate.retain_queued_input(received)
            {
                handoff.queued_inputs.push_front(received);
            }
            // Lower's original clock and actual child disposal outrank every
            // account wait. No author runs in this transport-only reduction.
            gate.transport_step(
                &mut GateRollbackHost {
                    context: &mut run.context,
                    arms: &mut handoff.arms,
                    original: GateRollbackEvidence {
                        raise: &handoff.gate_raise,
                        send: Some(&handoff.gate_send),
                        reply: Some(&handoff.gate_reply),
                        gate_enter: Some(handoff.enter),
                        enter: handoff.enter,
                        current_outgoing: handoff.gate_outgoing,
                        control: preceding_control(&handoff.graceful_tail, &handoff.input),
                    },
                    leave,
                    peer: handoff.peer.as_mut().unwrap(),
                },
                dials,
                false,
                io,
            )?;
            if let Some(child) = &handoff.arms.child {
                run.context.serving_organs.refresh_gate(child);
            }
            run.resources.refresh_spu_observation(&mut run.context);
            {
                let (residency, decode) = run.resources.spu_ports_mut().unwrap();
                let mut lifecycle = dials.ports(handoff.peer.as_mut().unwrap());
                lifecycle.residency = Some(residency);
                let mut ports = PumpPorts {
                    lifecycle,
                    gate: handoff
                        .arms
                        .port
                        .as_mut()
                        .filter(|port| port.held().is_none()),
                    decode: Some(decode),
                    label: None,
                    io: &mut *io,
                };
                if matches!(
                    run.context
                        .advance_leaving_organ_faults(&mut ports, gate.world_closed())?,
                    crate::flow::context::serving_organ_loss::ObservationProgress::Pending { .. }
                ) {
                    self.progress = FoundationEnteredRollbackProgress::Pending;
                    return Ok(());
                }
            }
            if !wind_down_pending && let Some(port) = &mut handoff.arms.port {
                handoff.input.route_reply(&mut run.context, port, io)?;
            }
            gate.settle_retired_execution(&mut run.context, io)?;
            let quiesce_ready = if let Some(work) = handoff
                .graceful_tail
                .as_mut()
                .and_then(|tail| tail.quiesce.as_mut())
            {
                work.prepare_lower(
                    &mut run.context,
                    handoff.arms.port.as_mut(),
                    gate.world_closed(),
                    handoff.input.original_input(),
                    io,
                )?
            } else {
                true
            };
            if wind_down_pending {
                handoff
                    .graceful_tail
                    .as_mut()
                    .unwrap()
                    .wind_down
                    .as_mut()
                    .unwrap()
                    .cancel(
                        &mut run.context,
                        run.resources.decode_mut(),
                        handoff.arms.port.as_mut(),
                        io,
                    )?;
            } else {
                handoff.input.cancel(
                    &mut run.context,
                    run.resources.decode_mut(),
                    handoff.arms.port.as_mut(),
                    io,
                )?;
            }
            let control_ready = (if wind_down_pending {
                true
            } else if let Some(port) = &mut handoff.arms.port {
                handoff.input.prepare_control(&mut run.context, port, io)?
            } else {
                true
            }) && quiesce_ready;
            gate.transport_step(
                &mut GateRollbackHost {
                    context: &mut run.context,
                    arms: &mut handoff.arms,
                    original: GateRollbackEvidence {
                        raise: &handoff.gate_raise,
                        send: Some(&handoff.gate_send),
                        reply: Some(&handoff.gate_reply),
                        gate_enter: Some(handoff.enter),
                        enter: handoff.enter,
                        current_outgoing: handoff.gate_outgoing,
                        control: preceding_control(&handoff.graceful_tail, &handoff.input),
                    },
                    leave,
                    peer: handoff.peer.as_mut().unwrap(),
                },
                dials,
                control_ready,
                io,
            )?;
            if !wind_down_pending && let Some(port) = &mut handoff.arms.port {
                handoff.input.route_reply(&mut run.context, port, io)?;
            }
            gate.settle_retired_execution(&mut run.context, io)?;
            // Only the exact gate owner may resume its pending ticket. A
            // waiting execution or read alone is not ownership of the author.
            let quiesce_record = handoff
                .graceful_tail
                .as_ref()
                .and_then(|tail| tail.quiesce.as_ref())
                .is_some_and(|work| work.has_record());
            if !control_ready
                || gate.owns_author()
                || (!quiesce_record
                    && !wind_down_pending
                    && run.context.rollback_gate_author_available())
            {
                let done = gate.step(
                    &mut GateRollbackHost {
                        context: &mut run.context,
                        arms: &mut handoff.arms,
                        original: GateRollbackEvidence {
                            raise: &handoff.gate_raise,
                            send: Some(&handoff.gate_send),
                            reply: Some(&handoff.gate_reply),
                            gate_enter: Some(handoff.enter),
                            enter: handoff.enter,
                            current_outgoing: handoff.gate_outgoing,
                            control: preceding_control(&handoff.graceful_tail, &handoff.input),
                        },
                        leave,
                        peer: handoff.peer.as_mut().unwrap(),
                    },
                    dials,
                    control_ready,
                    io,
                )?;
                self.gate_done = done && handoff.queued_inputs.is_empty();
                if !control_ready || gate.owns_author() {
                    return Ok(());
                }
            }
            let cause = match &handoff.message.envelope.payload {
                weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                    cause,
                    ..
                }) => *cause,
                _ => unreachable!("accepted actual rollback Leave"),
            };
            let settled = {
                let (residency, decode) = run
                    .resources
                    .spu_ports_mut()
                    .expect("original serving SPU ports");
                let mut lifecycle = dials.ports(handoff.peer.as_mut().unwrap());
                lifecycle.residency = Some(residency);
                let mut ports = PumpPorts {
                    lifecycle,
                    gate: handoff
                        .arms
                        .port
                        .as_mut()
                        .filter(|port| port.held().is_none()),
                    decode: Some(decode),
                    label: None,
                    io: &mut *io,
                };
                if let Some(work) = handoff
                    .graceful_tail
                    .as_mut()
                    .and_then(|tail| tail.quiesce.as_mut())
                    && !work.advance_after_force(
                        &mut run.context,
                        &mut ports,
                        gate.world_closed(),
                        handoff.input.original_input(),
                        &mut handoff.gate_exchanges,
                    )?
                {
                    return Ok(());
                }
                if wind_down_pending {
                    handoff
                        .graceful_tail
                        .as_mut()
                        .unwrap()
                        .wind_down
                        .as_mut()
                        .unwrap()
                        .advance_cancelled(
                            &mut handoff.certificate,
                            &mut run.context,
                            &mut ports,
                            cause,
                        )?;
                    // The prior input is cancelled on the next reduction,
                    // after its separate internal successor has settled.
                    return Ok(());
                }
                handoff.input.advance(
                    &mut handoff.certificate,
                    &mut run.context,
                    &mut ports,
                    gate.world_closed(),
                    cause,
                )?
            };
            if settled
                && self.gate_done
                && handoff.queued_inputs.is_empty()
                && !run.context.has_owed_organ_observation()
            {
                self.stage = if handoff.kind.captures() {
                    CleanupStage::Capture
                } else {
                    CleanupStage::Grants
                };
            }
            return Ok(());
        }
        if self.stage == CleanupStage::Capture {
            if self.capture.is_none() {
                self.capture = Some(ServingCapture::begin(
                    &mut run.context,
                    handoff.peer.as_ref().unwrap(),
                    handoff.message.envelope.exchange.clone(),
                    leave,
                    dials.outcome().0,
                    io,
                )?);
            }
            self.capture
                .as_mut()
                .unwrap()
                .observe_leave_mode(dials.outcome().0);
            let progress = self.capture.as_mut().unwrap().advance(
                &mut run.context,
                &mut PumpPorts {
                    lifecycle: dials.capture_ports(handoff.peer.as_mut().unwrap()),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                },
            )?;
            match progress {
                ServingCaptureProgress::Published => {
                    dials.freeze_outcome();
                    self.stage = CleanupStage::Grants;
                }
                ServingCaptureProgress::Refused => self.stage = CleanupStage::RefusedAnswer,
                ServingCaptureProgress::Pending => {
                    self.progress = FoundationEnteredRollbackProgress::Pending
                }
                ServingCaptureProgress::Progress => (),
            }
            dials.prepare(&mut run.context, leave, io)?;
            return Ok(());
        }
        if self.stage == CleanupStage::RefusedAnswer {
            dials.prepare(&mut run.context, leave, io)?;
            dials.freeze_refusal();
            if dials.outcome().0 {
                self.stage = CleanupStage::Grants;
            } else if self.capture.as_mut().unwrap().finish_graceful_refusal(
                &mut run.context,
                &mut PumpPorts {
                    lifecycle: dials.capture_ports(handoff.peer.as_mut().unwrap()),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                },
            )? {
                // S9 retains the entered run, lowered gate and original capture
                // evidence. Only the completed refused Leave's wait retires.
                run.context.deadlines.disarm(PumpDeadline::Leave);
                self.resting.completed_capture = Some((
                    self.capture.take().unwrap(),
                    handoff.peer.take().unwrap().retire_failed(),
                ));
                self.stage = CleanupStage::CaptureRefused;
                self.progress = FoundationEnteredRollbackProgress::Pending;
            } else {
                self.progress = FoundationEnteredRollbackProgress::Pending;
            }
            return Ok(());
        }
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.refusal_leg().is_some())
        {
            // A first Aborted may cross any refused-Leave stage; the capture
            // retains it and rejects a duplicate without repeating cleanup.
            self.capture.as_mut().unwrap().advance(
                &mut run.context,
                &mut PumpPorts {
                    lifecycle: dials.capture_ports(handoff.peer.as_mut().unwrap()),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                },
            )?;
        }
        if self.stage != CleanupStage::Terminal {
            dials.prepare(&mut run.context, leave, io)?;
        }
        let context = &mut run.context;
        match self.stage {
            CleanupStage::Grants => {
                if !self.inspecting {
                    match context.begin_inspection(InspectionKind::Grants, None, io) {
                        Ok(()) => self.inspecting = true,
                        Err(error) => {
                            self.inspecting = error.retained;
                            return Err(error.failure.unwrap_or(RunFailure::InspectionOwned));
                        }
                    }
                }
                let mut ports = PumpPorts {
                    lifecycle: dials.ports(handoff.peer.as_mut().unwrap()),
                    gate: None,
                    decode: None,
                    label: None,
                    io,
                };
                let progress = context.advance_inspection(&mut ports)?;
                if let Some(reading) = context.take_inspection(ports.io)? {
                    self.surface = Some(match (&handoff.startup.initial_grants, &reading) {
                        (
                            InspectionCompletion::Grants { surface: first, .. },
                            InspectionCompletion::Grants { surface: last, .. },
                        ) => {
                            if first == last {
                                GrantSurface::Unchanged
                            } else {
                                GrantSurface::Varied
                            }
                        }
                        (
                            InspectionCompletion::Grants { .. }
                            | InspectionCompletion::Refused { .. },
                            InspectionCompletion::Refused { .. },
                        )
                        | (
                            InspectionCompletion::Refused { .. },
                            InspectionCompletion::Grants { .. },
                        ) => GrantSurface::Unreadable,
                        _ => return Err(RunFailure::InspectionOwned),
                    });
                    self.current = Some(reading);
                    self.stage = CleanupStage::Seal;
                } else if progress == InspectionProgress::Pending {
                    context.pump_once(&mut ports)?;
                }
            }
            CleanupStage::Seal => {
                if dials.seal_step(context, leave, io)? {
                    self.stage = CleanupStage::Release;
                }
            }
            CleanupStage::Release => {
                context.begin_releasing(leave, io)?;
                self.stage = CleanupStage::Classifier;
            }
            CleanupStage::Classifier => {
                if context.retire_classifier(io)? == Advance::Progress {
                    self.stage = CleanupStage::Child;
                }
            }
            CleanupStage::Child => match run.resources.poll_classifier_child() {
                Some(failed_enter_driver::ChildState::Reaped(_)) => self.stage = CleanupStage::Spu,
                Some(failed_enter_driver::ChildState::Running) => {
                    io.wait(
                        &[],
                        Some(
                            context
                                .last_ns
                                .saturating_add(10_000_000)
                                .min(leave.deadline_ns()),
                        ),
                    )
                    .map_err(|error| {
                        context.fail(
                            io,
                            FlowHaltReason::TraceFailed,
                            HaltPoint::Author,
                            format!("entered rollback classifier wait: {error}"),
                        )
                    })?;
                }
                _ => {
                    return Err(context.fail(
                        io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "entered rollback lost actual classifier reap",
                    ));
                }
            },
            CleanupStage::Spu => {
                if self.release.is_none() {
                    let permit = EnteredRollbackReleasePermit {
                        origin: Arc::clone(&handoff.origin),
                        enter: handoff.enter,
                        leave,
                        ready: handoff.ready.take().expect("original whole Ready"),
                        exchange: handoff.answer.exchange.clone(),
                    };
                    match run.resources.begin_entered_release(
                        context,
                        permit,
                        handoff.admit_reply.take(),
                    ) {
                        Ok(release) => self.release = Some(release),
                        Err((permit, reply, detail)) => {
                            handoff.ready = Some(permit.ready);
                            handoff.admit_reply = reply;
                            return Err(context.fail(
                                io,
                                FlowHaltReason::ProtocolFailed,
                                HaltPoint::Barrier,
                                detail,
                            ));
                        }
                    }
                }
                let release = self.release.as_mut().unwrap();
                if release.advance(&mut run.resources, context, io)? {
                    self.stage = CleanupStage::State;
                } else {
                    release.wait(&run.resources, context, io)?;
                }
            }
            CleanupStage::State => {
                if context.retire_state(io)? == Advance::Progress {
                    self.terminal = Some(EnteredRollbackTerminal::new(
                        handoff.cause(),
                        self.surface.unwrap(),
                        self.release
                            .as_ref()
                            .and_then(RollbackResidencyRelease::outcome),
                        weaver_trace::MemberRelease::Closed,
                        self.gate.as_ref().and_then(RollbackGate::refusal).cloned(),
                    ));
                    let (forced, forced_by) = dials.outcome();
                    self.terminal
                        .as_mut()
                        .unwrap()
                        .leave_outcome(forced, forced_by);
                    if handoff.kind.captures() {
                        let capture = self.capture.as_ref().expect("original Leave capture");
                        if let Some(report) = capture.report() {
                            self.terminal.as_mut().unwrap().captured(report);
                        } else {
                            assert!(capture.refusal_leg().is_some() && forced);
                            self.terminal.as_mut().unwrap().refused_capture();
                        }
                    }
                    self.stage = CleanupStage::Terminal;
                }
            }
            CleanupStage::Terminal => {
                let terminal = self.terminal.as_mut().unwrap();
                let result = terminal
                    .step(
                        context,
                        &mut run.resources,
                        &mut handoff.arms,
                        handoff.peer.as_mut().unwrap(),
                        &handoff.message,
                        leave,
                        io,
                    )
                    .and_then(|complete| {
                        if complete {
                            dials.finish_joins(
                                terminal.capture_answer(),
                                context.last_ns,
                                leave,
                                io,
                            )
                        } else {
                            Ok(false)
                        }
                    });
                match result {
                    Ok(true) => self.closed(),
                    Ok(false) => (),
                    Err(detail) => {
                        self.detail = Some(detail.clone());
                        return Err(context.failure.clone().unwrap_or_else(|| {
                            if self
                                .terminal
                                .as_ref()
                                .and_then(EnteredRollbackTerminal::sequence)
                                .is_some()
                            {
                                RunFailure::Closed
                            } else if leave.expired(context.last_ns) {
                                RunFailure::Wait {
                                    key: PumpDeadline::Leave,
                                    bound: leave,
                                }
                            } else {
                                context.fail(
                                    io,
                                    FlowHaltReason::ProtocolFailed,
                                    HaltPoint::Barrier,
                                    detail,
                                )
                            }
                        }));
                    }
                }
            }
            CleanupStage::Gate
            | CleanupStage::Capture
            | CleanupStage::RefusedAnswer
            | CleanupStage::CaptureRefused
            | CleanupStage::Closed => unreachable!(),
        }
        Ok(())
    }

    fn begin_failure(&mut self, error: RunFailure, io: &mut impl PumpIo) {
        self.detail
            .get_or_insert_with(|| format!("entered rollback: {error:?}"));
        if self
            .terminal
            .as_ref()
            .and_then(EnteredRollbackTerminal::sequence)
            .is_some()
        {
            self.dispose();
            return;
        }
        let context = &mut self.run.as_mut().unwrap().context;
        if context.failure.is_none() {
            context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                format!("entered rollback: {error:?}"),
            );
        }
        let FoundationRun {
            context, resources, ..
        } = *self.run.take().unwrap();
        let cause = self.handoff.cause();
        let admit_reply = if self.release.is_none() {
            self.handoff.admit_reply.take()
        } else {
            None
        };
        let mut tail = if self.stage == CleanupStage::CaptureRefused {
            FailedRunCleanup::after_resting(context, resources, admit_reply)
        } else {
            FailedRunCleanup::after_serving_rollback(
                context,
                resources,
                self.handoff.leave,
                cause,
                self.release.take(),
                admit_reply,
            )
        };
        if let Some(save) = self.resting.save.take() {
            let (capture, request) = save.into_failed(&tail.context);
            self.resting.failed_save = Some(request);
            if let Err(capture) = tail.retain_failed_capture(capture) {
                self.retained_failed_capture = Some(capture);
                tail.secondary("S9 save could not join its original failed report");
            }
        }
        if let Some(capture) = self.capture.take() {
            let peer = self
                .handoff
                .peer
                .take()
                .expect("capture retains original lifecycle peer");
            match capture.into_failed(&tail.context, peer) {
                Ok(capture) => {
                    if let Err(capture) = tail.retain_failed_capture(capture) {
                        self.retained_failed_capture = Some(capture);
                        tail.secondary("Leave capture could not join its original failed report");
                    }
                }
                Err((capture, peer)) => {
                    self.capture = Some(capture);
                    self.handoff.peer = Some(peer);
                    tail.secondary(
                        "Leave capture failure retained with unmatched original context",
                    );
                }
            }
        }
        if self.gate.is_none() {
            self.gate = Some(RollbackGate::failed_new(
                self.handoff.original(),
                &self.handoff.arms,
                Some(self.handoff.leave),
                tail.deadline(),
                tail.context.last_ns,
                true,
            ));
        }
        self.failed = Some(tail);
        self.close_dials();
        self.progress = FoundationEnteredRollbackProgress::Progress;
    }

    fn advance_failed(&mut self, io: &mut impl PumpIo) -> FoundationEnteredRollbackProgress {
        let tail = self.failed.as_mut().unwrap();
        let gate = self.gate.as_mut().unwrap();
        if self.reported && !self.failed_grants_attempted {
            self.failed_grants_attempted = true;
            if let Some(work) = self
                .handoff
                .graceful_tail
                .as_mut()
                .and_then(|graceful| graceful.wind_down.as_mut())
                && let Err(error) = work.retire_failed_placement_grants(&mut tail.context)
            {
                tail.secondary(format!(
                    "failed wind-down retained placement grant: {error:?}"
                ));
            }
            if let Err(error) = self
                .handoff
                .input
                .retire_failed_placement_grants(&mut tail.context)
            {
                tail.secondary(format!(
                    "failed rollback retained placement grant: {error:?}"
                ));
            }
        }
        let progress = if !self.reported {
            self.reported = true;
            tail.advance(io)
        } else if !self.gate_done {
            if !tail.clock(io) || !tail.advance_accounts(io) {
                gate.failed_dispose(&mut self.handoff.arms);
                tail.abort();
                FailedEnterProgress::Closed
            } else {
                let original = GateRollbackEvidence {
                    raise: &self.handoff.gate_raise,
                    send: Some(&self.handoff.gate_send),
                    reply: Some(&self.handoff.gate_reply),
                    gate_enter: Some(self.handoff.enter),
                    enter: self.handoff.enter,
                    current_outgoing: self.handoff.gate_outgoing,
                    control: preceding_control(&self.handoff.graceful_tail, &self.handoff.input),
                };
                match gate.failed_step(
                    original,
                    &mut self.handoff.arms,
                    &mut self.sweep_reported,
                    tail,
                    io,
                ) {
                    Ok(true) => {
                        self.gate_done = true;
                        FailedEnterProgress::Progress
                    }
                    Ok(false) => FailedEnterProgress::Pending,
                    Err(detail) => {
                        tail.secondary(detail);
                        gate.failed_dispose(&mut self.handoff.arms);
                        tail.abort();
                        FailedEnterProgress::Closed
                    }
                }
            }
        } else {
            tail.advance(io)
        };
        self.progress = match progress {
            FailedEnterProgress::Progress => FoundationEnteredRollbackProgress::Progress,
            FailedEnterProgress::Pending => FoundationEnteredRollbackProgress::Pending,
            FailedEnterProgress::Closed => {
                gate.failed_dispose(&mut self.handoff.arms);
                self.stage = CleanupStage::Closed;
                FoundationEnteredRollbackProgress::Closed
            }
        };
        self.progress
    }

    pub(in crate::flow::context) fn wait_with_io(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationEnteredRollbackProgress {
        if self.progress != FoundationEnteredRollbackProgress::Pending {
            return self.progress;
        }
        if self.stage == CleanupStage::CaptureRefused && self.failed.is_none() {
            self.wait_resting(io);
            return self.progress;
        }
        if let Some(tail) = &mut self.failed {
            let result = if self.gate_done {
                tail.wait_for_progress(io)
            } else {
                match self
                    .gate
                    .as_mut()
                    .unwrap()
                    .failed_wait(&self.handoff.arms, tail, io)
                {
                    true => FailedEnterProgress::Pending,
                    false => {
                        tail.abort();
                        FailedEnterProgress::Closed
                    }
                }
            };
            if result == FailedEnterProgress::Closed {
                self.gate
                    .as_mut()
                    .unwrap()
                    .failed_dispose(&mut self.handoff.arms);
                self.closed();
            }
            return self.progress;
        }
        let run = self.run.as_mut().unwrap();
        let result = run.context.pump_once(&mut PumpPorts {
            lifecycle: self
                .dials
                .as_mut()
                .unwrap()
                .ports(self.handoff.peer.as_mut().unwrap()),
            gate: self.handoff.arms.port.as_mut(),
            decode: None,
            label: None,
            io,
        });
        match result {
            Err(RunFailure::Wait {
                key: PumpDeadline::CaptureResult | PumpDeadline::CaptureWindow,
                ..
            }) if self.stage == CleanupStage::Capture => {
                // The member owner must reduce a missing snapshot/finished
                // result into its original abort and recorded refusal. Yield
                // only pump expiry: an error returned by capture.advance still
                // goes through handle_error, including post-Commit expiry.
                self.progress = FoundationEnteredRollbackProgress::Progress;
            }
            Err(error) => self.handle_error(error, io),
            Ok(_) => (),
        }
        self.progress
    }

    fn handle_error(&mut self, error: RunFailure, io: &mut impl PumpIo) {
        match error {
            RunFailure::RollbackGateHeld if self.stage == CleanupStage::Gate => {
                self.progress = FoundationEnteredRollbackProgress::Progress;
            }
            RunFailure::Busy(_) => self.progress = FoundationEnteredRollbackProgress::Pending,
            RunFailure::Transport {
                source: PumpSource::Gate,
                ..
            } if self.stage == CleanupStage::Gate
                && self
                    .run
                    .as_ref()
                    .is_some_and(|run| run.context.failure().is_none()) =>
            {
                // A nested input/engine wait only observes this transport.
                // RollbackGate still owns the exact held GateStopped, EOF and
                // raw framing; its next transport step judges that evidence.
                self.progress = FoundationEnteredRollbackProgress::Progress;
            }
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound,
            } if bound == self.handoff.leave => {
                if self.handoff.kind.captures() {
                    self.begin_failure(
                        RunFailure::Wait {
                            key: PumpDeadline::Leave,
                            bound,
                        },
                        io,
                    );
                } else {
                    self.dispose();
                }
            }
            RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            } if self.gate.as_ref().is_some_and(|gate| gate.bound() == bound) => (),
            error @ RunFailure::Transport {
                source: PumpSource::LeavePeer,
                ..
            } => {
                self.handoff.peer_unavailable = true;
                if self.handoff.kind.captures()
                    && self.capture.is_some()
                    && self.stage != CleanupStage::Capture
                {
                    self.begin_failure(error, io);
                }
            }
            error @ (RunFailure::Transport {
                source: PumpSource::AdminDial,
                ..
            }
            | RunFailure::Wait {
                key: PumpDeadline::AdminRead,
                ..
            }) => {
                let result = self.dials.as_mut().unwrap().recover(
                    &mut self.run.as_mut().unwrap().context,
                    error,
                    io,
                );
                if let Err(error) = result {
                    if matches!(
                        error,
                        RunFailure::Transport {
                            source: PumpSource::AdminDial,
                            ..
                        } | RunFailure::Wait {
                            key: PumpDeadline::AdminRead,
                            ..
                        }
                    ) {
                        self.begin_failure(error, io);
                    } else {
                        self.handle_error(error, io);
                    }
                }
            }
            error => self.begin_failure(error, io),
        }
    }
}

#[cfg(test)]
#[path = "context_worker_entered_rollback_exit_tests.rs"]
mod exit_tests;
