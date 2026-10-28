//! Preserve an expired startup for the actual root rollback. Expiry grants no
//! failed-run authority, successful Enter, or renewed budget for owed boot work.
use super::failed::{DetachedStartup, HeldStartup, StartupGate};
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::{RecordingPhase, failed_enter_driver::ResidentBootResources};
use crate::flow::pump::PumpDeadline;
use crate::flow::pump_transport::{DecodePort, PortIdentity};
use std::sync::Arc;

#[path = "context_worker_startup_expired_active.rs"]
mod active;
#[path = "context_worker_startup_expired_failure.rs"]
pub(super) mod failure;
pub use failure::FoundationExpiredRollbackExit;
#[path = "context_worker_startup_expired_gate.rs"]
pub(in crate::flow) mod gate;
#[path = "context_worker_startup_expired_preflight.rs"]
pub(in crate::flow) mod preflight;

#[cfg(test)]
#[path = "context_worker_startup_expired_tests.rs"]
pub(super) mod tests;
use crate::flow::clock::WaitInterest;
use crate::flow::context::{FlowHaltReason, HaltPoint, RunFailure};
use crate::flow::pump::{
    LifecycleMessage, LifecyclePeer, RetiredLifecycleListener, RetiredLifecyclePeer,
    enter::RetiredEnter,
};
use crate::flow::pump_transport::{OrganSend, PacketPort, TrackedStage};
use nix::poll::PollFlags;
use weaver_types::{
    LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Opener, OrganEnvelope, Payload, Position,
};

pub(in crate::flow::context) struct ExpiredEnterIdentity {
    origin: Arc<()>,
    enter: AbsoluteBound,
    decode: Option<PortIdentity>,
    residency: Option<PortIdentity>,
}

/// Issued after the actual rollback seal. Its Leave permits only release of
/// this run's original resources, never another startup write. The caller still
/// completes classifier reap before releasing the SPU or member.
pub(in crate::flow::context) struct ExpiredReleasePermit {
    origin: Arc<()>,
    enter: AbsoluteBound,
    leave: AbsoluteBound,
}
impl ExpiredReleasePermit {
    pub(in crate::flow::context) fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.phase == RecordingPhase::Releasing
            && context.retired_enter == Some(self.enter)
            && context.deadlines.get(PumpDeadline::Enter).is_none()
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && self.enter.expired(context.last_ns)
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

/// The actual rollback Leave permits its Grants comparison after Enter retires.
/// It grants no ordinary inspection or boot continuation.
pub(in crate::flow::context) struct ExpiredStartupGrantsPermit {
    origin: Arc<()>,
    enter: AbsoluteBound,
    leave: AbsoluteBound,
}
impl ExpiredStartupGrantsPermit {
    pub(in crate::flow::context) fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.phase == RecordingPhase::Entered
            && context.flow.phase() == crate::flow::coordinator::CoordinatorPhase::Active
            && context.retired_enter == Some(self.enter)
            && context.deadlines.get(PumpDeadline::Enter).is_none()
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && self.enter.expired(context.last_ns)
            && !self.leave.expired(context.last_ns)
            && context.boot_enter_cancelled
            && context.flow.preinput_custody()
            && context.failure.is_none()
            && context.flow.halt().is_none()
    }
}

/// Only the consuming rollback may dispose its original expired startup account.
/// This grants no query, decode, record or startup work.
pub(in crate::flow::context) struct ExpiredStartupAccountPermit {
    origin: Arc<()>,
    enter: AbsoluteBound,
    leave: AbsoluteBound,
}
impl ExpiredStartupAccountPermit {
    pub(in crate::flow::context) fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && context.phase == RecordingPhase::Entered
            && context.flow.phase() == crate::flow::coordinator::CoordinatorPhase::Active
            && context.deadlines.get(PumpDeadline::Enter) == Some(self.enter)
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && self.enter.expired(context.last_ns)
            && !self.leave.expired(context.last_ns)
            && context.boot_enter_cancelled
            && context.flow.preinput_custody()
            && context.failure.is_none()
            && context.flow.halt().is_none()
    }
}

/// Issued only by this consuming owner at its actual accepted Leave cap.
/// It permits disposal of retained work, never an account retry or a record.
pub(in crate::flow) struct ExpiredStartupDisposal {
    origin: Arc<()>,
    barrier: Arc<()>,
    enter: AbsoluteBound,
    leave: AbsoluteBound,
}
impl ExpiredStartupDisposal {
    pub(in crate::flow::context) fn allows(&self, context: &RecordingContext) -> bool {
        Arc::ptr_eq(&self.origin, &context.read_origin)
            && self.enter.expired(context.last_ns)
            && self.leave.expired(context.last_ns)
            && match context.deadlines.get(PumpDeadline::Enter) {
                Some(enter) => enter == self.enter,
                None => context.retired_enter == Some(self.enter),
            }
            && context.deadlines.get(PumpDeadline::Leave) == Some(self.leave)
            && context.failure.is_none()
            && context.flow.halt().is_none()
            && context.flow.preinput_custody()
    }

    pub(in crate::flow) fn matches_record(&self, barrier: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.barrier, barrier)
    }
}

fn expired_context(context: &RecordingContext) -> Option<AbsoluteBound> {
    let enter = context.deadlines.get(PumpDeadline::Enter)?;
    (enter.expired(context.last_ns)
        && matches!(
            context.phase,
            RecordingPhase::Preflight | RecordingPhase::Entered
        )
        && context.failure.is_none()
        && context.flow.halt().is_none()
        && context.flow.preinput_custody()
        && !context.ever_started_turn
        && context.open_turn.is_none()
        && context.input.snapshot().is_none()
        && context.member_capture.is_none()
        && !context.place_leaving
        && context.deadlines.get(PumpDeadline::Leave).is_none())
    .then_some(enter)
}

impl ExpiredEnterIdentity {
    fn original(
        context: &RecordingContext,
        original: Option<&ReceivedEnter>,
    ) -> Option<AbsoluteBound> {
        let enter = expired_context(context)?;
        let original = original?;
        let EnterBinding::Serving { enter_window, .. } = &original.envelope().enter().binding
        else {
            return None;
        };
        (original.adopted_by(context)
            && enter_window.began_ns.0 == enter.began_ns()
            && enter_window.deadline_ns.0 == enter.deadline_ns())
        .then_some(enter)
    }
    pub(in crate::flow::context) fn from_resources(
        context: &RecordingContext,
        resources: &FailedEnterResources,
        original: Option<&ReceivedEnter>,
    ) -> Option<Self> {
        let enter = Self::original(context, original)?;
        if !resources.matches(context) || !resources.original_ports() {
            return None;
        }
        Some(Self {
            origin: Arc::clone(&context.read_origin),
            enter,
            decode: resources
                .decode_present_for_serving()
                .map(|port| port.port_identity()),
            residency: resources.residency_port().map(|port| port.port_identity()),
        })
    }
    pub(in crate::flow::context) fn from_boot(
        context: &RecordingContext,
        decode: &DecodePort,
        resident: Option<&ResidentBootResources<'_>>,
    ) -> Option<Self> {
        let resident = resident?;
        let enter = Self::original(context, resident.evidence.startup.original_enter.as_ref())?;
        if !Arc::ptr_eq(&context.read_origin, &resident.evidence.origin)
            || resident.evidence.enter != enter
        {
            return None;
        }
        Some(Self {
            origin: Arc::clone(&context.read_origin),
            enter,
            decode: Some(decode.port_identity()),
            residency: Some(resident.residency.port_identity()),
        })
    }
    fn matches(&self, run: &FoundationRun) -> bool {
        Arc::ptr_eq(&self.origin, &run.context.read_origin)
            && expired_context(&run.context) == Some(self.enter)
            && run.resources.matches(&run.context)
            && run.resources.original_ports()
            && match (&self.decode, run.resources.decode_present_for_serving()) {
                (Some(identity), Some(port)) => identity.matches_decode(port),
                (None, None) => true,
                _ => false,
            }
            && match (&self.residency, run.resources.residency_port()) {
                (Some(identity), Some(port)) => identity.matches(port),
                (None, None) => true,
                _ => false,
            }
            && run.original.is_none()
            && run.listener.is_none()
    }
}

/// No live capability escapes this account. Its original Enter and listener
/// remain retained until explicit rollback, without a synthetic refusal answer.
pub struct FoundationExpiredEnter {
    identity: ExpiredEnterIdentity,
    held: Box<HeldStartup>,
    listener: Option<LifecycleListener>,
    gate: Option<StartupGate>,
    residency: Option<Box<ResidencyRequest>>,
    gate_request: Option<Box<GateStartupRequest>>,
    detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationExpiredRollbackProgress {
    AwaitRollback,
    Accepted,
    Stopped,
    Closed,
}

struct Dial {
    peer: LifecyclePeer,
    read: AbsoluteBound,
    message: Option<LifecycleMessage>,
    answer: Option<OrganEnvelope>,
    send: Option<OrganSend>,
}

struct RetiredDial {
    _peer: RetiredLifecyclePeer,
    _read: AbsoluteBound,
    _message: Option<LifecycleMessage>,
    _answer: Option<OrganEnvelope>,
    _send: Option<OrganSend>,
}

/// The exact run and expired startup stay owned while awaiting root. No
/// operation here may resume boot or change the expired Enter interval.
pub struct FoundationExpiredRollback {
    run: Box<FoundationRun>,
    handoff: Box<FoundationExpiredEnter>,
    dial: Option<Dial>,
    leave: Option<AbsoluteBound>,
    original: Option<RetiredEnter>,
    before_load: bool,
    retired: Option<crate::flow::coordinator::FailedFlowResources>,
    detail: Option<String>,
    abandoned: bool,
    stopped_record: Option<crate::flow::recording::StoppedStartupRecord>,
    retired_listener: Option<RetiredLifecycleListener>,
    retired_dial: Option<RetiredDial>,
    retired_gate: Option<PacketPort<OrganEnvelope, ()>>,
    preflight: Option<preflight::ExpiredPreflightRollback>,
    active: Option<active::ExpiredActiveRollback>,
}

impl FoundationRun {
    #[allow(clippy::result_large_err)]
    pub fn expired_rollback(
        self: Box<Self>,
        handoff: Box<FoundationExpiredEnter>,
    ) -> Result<Box<FoundationExpiredRollback>, (Box<Self>, Box<FoundationExpiredEnter>)> {
        if !handoff.identity.matches(&self) {
            return Err((self, handoff));
        }
        let before_load = !self.context.barrier.is_loaded();
        let mut owner = Box::new(FoundationExpiredRollback {
            run: self,
            handoff,
            dial: None,
            leave: None,
            original: None,
            before_load,
            retired: None,
            detail: None,
            abandoned: false,
            stopped_record: None,
            retired_listener: None,
            retired_dial: None,
            retired_gate: None,
            preflight: None,
            active: None,
        });
        if before_load {
            owner.retired = owner
                .run
                .context
                .flow
                .retire_unadmitted_ports(&owner.run.context.barrier);
            assert!(owner.retired.is_some(), "checked original unadmitted Enter");
            if let Err(error) = owner.run.context.barrier.close_unadmitted() {
                owner.detail = Some(format!("unadmitted trace close: {error:?}"));
            }
            owner.original = owner.handoff.held.retire_enter();
        }
        Ok(owner)
    }
}

impl FoundationExpiredRollback {
    fn gate_host(&mut self) -> gate::GateRollbackHost<'_> {
        let original = match self.handoff.held.as_ref() {
            HeldStartup::Gate(gate) | HeldStartup::Ready { gate, .. } => gate,
            _ => unreachable!("actual startup gate account"),
        };
        gate::GateRollbackHost {
            context: &mut self.run.context,
            arms: self.handoff.gate.as_mut().expect("actual gate resources"),
            original: gate::GateRollbackEvidence::from_held(original, self.handoff.identity.enter),
            leave: self.leave.expect("actual accepted rollback Leave"),
            peer: &mut self.dial.as_mut().expect("actual Leave peer").peer,
        }
    }

    pub fn progress(&self) -> FoundationExpiredRollbackProgress {
        if self.abandoned
            || self
                .preflight
                .as_ref()
                .is_some_and(|owner| owner.complete())
        {
            FoundationExpiredRollbackProgress::Closed
        } else if self.detail.is_some() {
            FoundationExpiredRollbackProgress::Stopped
        } else if self.leave.is_some() {
            FoundationExpiredRollbackProgress::Accepted
        } else {
            FoundationExpiredRollbackProgress::AwaitRollback
        }
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub fn abandoned(&self) -> bool {
        self.abandoned
    }

    pub fn process_exit(&self) -> Option<crate::FoundationProcessExit> {
        if self.progress() != FoundationExpiredRollbackProgress::Closed {
            return None;
        }
        Some(
            self.preflight
                .as_ref()
                .filter(|_| {
                    !self.abandoned
                        && self.run.context.failure.is_none()
                        && self.run.context.flow.halt().is_none()
                })
                .and_then(preflight::ExpiredPreflightRollback::process_exit)
                .unwrap_or(crate::FoundationProcessExit::Failure),
        )
    }

    pub fn channel_fault(&self) -> Option<crate::ChannelFault> {
        (self.progress() == FoundationExpiredRollbackProgress::Closed)
            .then_some(crate::ChannelFault::Closed)
    }

    pub fn unload_sequence(&self) -> Option<u64> {
        self.preflight.as_ref().and_then(|owner| owner.sequence())
    }

    pub fn advance(&mut self) -> FoundationExpiredRollbackProgress {
        self.advance_with_io(&mut MonotonicIo)
    }

    fn clock(&mut self, io: &mut impl PumpIo) -> Result<u64, String> {
        if self.before_load || self.run.context.phase == RecordingPhase::Closed {
            let now = io
                .now()
                .map_err(|error| format!("rollback clock: {error}"))?;
            if now < self.run.context.last_ns {
                return Err("rollback clock reversed".into());
            }
            self.run.context.last_ns = now;
            return Ok(now);
        }
        let clock = match self.run.context.live(io) {
            Err(
                error @ RunFailure::Wait {
                    key: PumpDeadline::AdminRead,
                    ..
                },
            ) => {
                if let Some(owner) = &mut self.preflight {
                    owner.recover_clock(&mut self.run.context, error, io)
                } else if let Some(owner) = &mut self.active {
                    owner.recover_clock(&mut self.run.context, error, io)
                } else {
                    Err(error)
                }
            }
            result => result,
        };
        match clock {
            Ok(()) => {}
            Err(RunFailure::Wait {
                key: PumpDeadline::Enter,
                bound,
            }) if bound == self.handoff.identity.enter => {}
            Err(RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound,
            }) if self.leave == Some(bound) => {}
            Err(RunFailure::Wait {
                key: PumpDeadline::Lower,
                bound,
            }) if self
                .active
                .as_ref()
                .and_then(|active| active.gate.as_ref())
                .is_some_and(|gate| gate.bound() == bound) => {}
            Err(error) => return Err(format!("expired Enter rollback: {error:?}")),
        }
        Ok(self.run.context.last_ns)
    }

    fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationExpiredRollbackProgress {
        if self.progress() == FoundationExpiredRollbackProgress::Accepted {
            match self.clock(io) {
                Ok(now) if self.leave.unwrap().expired(now) => self.dispose_expired(),
                Ok(_) => {
                    if let Err(detail) = self.advance_preflight(io) {
                        self.detail = Some(detail);
                    }
                    // An inner drain/author wait can observe the accepted cap.
                    // It still owes disposal even though that helper returned
                    // the retained work to this owner with a wait result.
                    if !self
                        .preflight
                        .as_ref()
                        .is_some_and(|owner| owner.complete())
                    {
                        match self.clock(io) {
                            Ok(now) if self.leave.unwrap().expired(now) => self.dispose_expired(),
                            Ok(_) => (),
                            Err(detail) => {
                                self.detail.get_or_insert(detail);
                            }
                        }
                    }
                }
                Err(detail) => self.detail = Some(detail),
            }
            return self.progress();
        }
        if self.progress() != FoundationExpiredRollbackProgress::AwaitRollback {
            return self.progress();
        }
        if let Err(detail) = self.await_step(io) {
            self.detail = Some(detail);
        }
        self.progress()
    }

    fn await_step(&mut self, io: &mut impl PumpIo) -> Result<(), String> {
        let now = self.clock(io)?;
        if self.dial.is_none() {
            let listener = self
                .handoff
                .listener
                .as_mut()
                .expect("live rollback listener");
            listener
                .accept_observed(now)
                .map_err(|error| format!("rollback accept: {error:?}"))?;
            let Some((peer, accepted)) = listener.take_observed_connection() else {
                return Ok(());
            };
            let read = AbsoluteBound::after(accepted, crate::state::ANSWER_BOUND_MS * 1_000_000)
                .map_err(|error| error.to_string())?;
            self.dial = Some(Dial {
                peer,
                read,
                message: None,
                answer: None,
                send: None,
            });
        }
        let dial = self.dial.as_mut().unwrap();
        if dial.read.expired(now) {
            self.dial = None;
            return Ok(());
        }
        if dial.message.is_none() && dial.peer.held().is_none() {
            let received = dial.peer.try_receive();
            if received.is_err() && dial.peer.held().is_none() {
                self.dial = None;
                return Ok(());
            }
        }
        let now = self.clock(io)?;
        let dial = self.dial.as_mut().unwrap();
        if dial.read.expired(now) {
            self.dial = None;
            return Ok(());
        }
        if dial.message.is_none()
            && let Some(message) = dial.peer.held()
        {
            if message.envelope.exchange.opener != Opener::Admin
                || message.envelope.position != Position::Open
                || message.sink.is_some()
                || message.state.is_some()
            {
                self.dial = None;
                return Ok(());
            }
            let rollback = matches!(
                message.envelope.payload,
                Payload::Directive(LifecycleDirective::Leave { rollback: true, .. })
            );
            dial.message = dial
                .peer
                .take_completion()
                .map_err(|error| format!("rollback directive: {error:?}"))?;
            let message = dial.message.as_ref().expect("actual held lifecycle frame");
            if rollback && !self.before_load {
                let leave = AbsoluteBound::after(now, weaver_types::LEAVE_BOUND_MS * 1_000_000)
                    .map_err(|error| error.to_string())?;
                self.run
                    .context
                    .deadlines
                    .arm(PumpDeadline::Leave, leave)
                    .map_err(|error| error.to_string())?;
                self.leave = Some(leave);
                self.run.context.boot_enter_cancelled = true;
                self.original = self.handoff.held.retire_enter();
                return Ok(());
            }
            let payload = match &message.envelope.payload {
                Payload::Directive(LifecycleDirective::Observe) if self.before_load => {
                    Payload::Answer(LifecycleAnswer::State {
                        state: weaver_types::AgentState::Unloaded,
                        load: None,
                        constituents: vec![],
                    })
                }
                Payload::Directive(LifecycleDirective::Observe) => {
                    Payload::Answer(LifecycleAnswer::InTransition {
                        constituents: vec![],
                    })
                }
                Payload::Directive(_) => Payload::Refusal(LifecycleRefusal::OutOfOrder),
                _ => {
                    self.dial = None;
                    return Ok(());
                }
            };
            dial.answer = Some(OrganEnvelope {
                exchange: message.envelope.exchange.clone(),
                position: Position::Close,
                payload,
            });
        }
        if let Some(answer) = dial.answer.take() {
            match dial.peer.stage_tracked(answer) {
                Ok(TrackedStage::Queued(send)) => dial.send = Some(send),
                Ok(TrackedStage::Busy(answer)) => dial.answer = Some(answer),
                Err(error) => {
                    dial.answer = Some(error.input);
                    self.dial = None;
                    return Ok(());
                }
            }
        }
        if dial.send.is_some() {
            let result = dial.peer.try_send_observed(io);
            if result.is_err() || dial.send.as_ref().is_some_and(OrganSend::sent) {
                if dial
                    .send
                    .as_ref()
                    .is_some_and(|send| send.sent() && send.sent_at().is_none())
                {
                    if self.before_load {
                        return Err("unadmitted rollback response lost its send clock".into());
                    }
                    let reason = self.run.context.fail(
                        io,
                        FlowHaltReason::TraceFailed,
                        HaltPoint::Author,
                        "rollback response lost its send clock",
                    );
                    return Err(format!("rollback response: {reason:?}"));
                }
                self.dial = None;
            }
        }
        self.clock(io).map(|_| ())
    }

    /// Waiting for root neither polls the expired Enter deadline nor progresses
    /// any old boot send. A silent accepted peer retains its own read deadline.
    pub fn wait_for_progress(&mut self) -> FoundationExpiredRollbackProgress {
        self.wait_with_io(&mut MonotonicIo)
    }
    fn wait_with_io(&mut self, io: &mut impl PumpIo) -> FoundationExpiredRollbackProgress {
        if self.progress() != FoundationExpiredRollbackProgress::AwaitRollback {
            return self.progress();
        }
        if let Err(detail) = self.clock(io) {
            self.detail = Some(detail);
            return self.progress();
        }
        if !self.before_load
            && let Err(error) = self.run.context.barrier.drain_capacity_wake()
        {
            self.run.context.record_failed(io);
            self.detail = Some(format!("rollback trace wake: {error:?}"));
            return self.progress();
        }
        let (fd, events) = match &self.dial {
            Some(dial) => (
                dial.peer.fd(),
                if dial.answer.is_some() || dial.send.is_some() {
                    PollFlags::POLLOUT
                } else {
                    PollFlags::POLLIN
                },
            ),
            None => self.handoff.listener.as_ref().unwrap().interest(),
        };
        let mut interests = vec![WaitInterest { fd, events }];
        if !self.before_load {
            interests.push(WaitInterest {
                fd: self.run.context.barrier.capacity_fd(),
                events: PollFlags::POLLIN,
            });
        }
        let bound = self.dial.as_ref().map(|dial| dial.read.deadline_ns());
        if let Err(error) = io.wait(&interests, bound) {
            if self.before_load {
                self.detail = Some(format!("unadmitted rollback wait: {error}"));
                return self.progress();
            }
            let reason = self.run.context.fail(
                io,
                FlowHaltReason::TraceFailed,
                HaltPoint::Author,
                format!("expired Enter rollback wait: {error}"),
            );
            self.detail = Some(format!("rollback wait: {reason:?}"));
        } else if let Err(detail) = self.clock(io) {
            self.detail = Some(detail);
        }
        self.progress()
    }

    /// At the actual Leave cap there is no permission to finish old boot work,
    /// record an Unload or answer Left. Retain its incomplete account first,
    /// then close the actual endpoints and stop only owned children.
    fn dispose_expired(&mut self) {
        let context = &mut self.run.context;
        let permit = ExpiredStartupDisposal {
            origin: Arc::clone(&self.handoff.identity.origin),
            barrier: context.barrier.author_identity(),
            enter: self.handoff.identity.enter,
            leave: self.leave.expect("accepted actual Leave"),
        };
        assert!(
            permit.allows(context),
            "exact expired startup and Leave custody"
        );
        self.stopped_record = Some(context.barrier.retain_expired_startup_record(&permit));
        if let Some(decode) = self.run.resources.decode_mut() {
            context.dispose_expired_startup_open(&permit, decode);
        }
        context.dispose_expired_startup_summary(&permit);
        self.retired = context.flow.retire_abandoned_rollback_ports();
        assert!(self.retired.is_some(), "exact pre-input rollback owner");
        self.run.resources.close_at_cap();
        if let Some(gate) = &mut self.handoff.gate {
            if let Some(account) = self.active.as_mut().and_then(|active| active.gate.as_mut()) {
                account.dispose(gate);
            } else {
                self.retired_gate = gate.port.take().map(|port| port.retire_failed());
                if let Some(tools) = &gate.tools
                    && let Err(error) = tools.kill()
                {
                    self.detail
                        .get_or_insert_with(|| format!("rollback tools kill: {error}"));
                    tools.signal_pinned_pass();
                }
                if let Some(child) = &mut gate.child {
                    child.stop_at_cap();
                }
            }
        }
        self.retired_listener = self
            .handoff
            .listener
            .take()
            .map(LifecycleListener::retire_failed);
        if let Some(preflight) = &mut self.preflight {
            preflight.retire_dials();
        }
        if let Some(active) = &mut self.active {
            active.retire_accounts();
            active.retire_dials();
        }
        self.retired_dial = self.dial.take().map(|dial| RetiredDial {
            _peer: dial.peer.retire_failed(),
            _read: dial.read,
            _message: dial.message,
            _answer: dial.answer,
            _send: dial.send,
        });
        context.barrier.abandon();
        context.phase = RecordingPhase::Closed;
        self.abandoned = true;
    }
}

impl FoundationEnter<'_> {
    /// Consume only the stopped, healthy, expired original startup. In
    /// particular a successful Ready, a refusal before expiry and a component
    /// failure keep their existing owners and dispositions.
    pub fn expired_handoff(mut self) -> Result<Box<FoundationExpiredEnter>, Self> {
        if self.progress() != FoundationEnterProgress::Stopped {
            return Err(self);
        }
        let identity = match self.state.as_ref().expect("retained startup") {
            EnterState::Startup(owner) => owner.expired_identity(),
            EnterState::Residency(owner) => owner.expired_identity(),
            EnterState::Admitted(owner) => owner.expired_identity(),
            EnterState::Boot(owner) => owner.expired_identity(),
            EnterState::Gate(owner) => owner.expired_identity(),
            EnterState::Raised(owner) => owner.expired_identity(),
            EnterState::Ready(owner) => owner.expired_identity(),
            EnterState::Entered(_) => None,
        };
        let Some(identity) = identity else {
            return Err(self);
        };
        let parts: DetachedStartup<ExpiredEnterIdentity> = match self.state.take().unwrap() {
            EnterState::Startup(owner) => owner.detach_owned(identity),
            EnterState::Residency(owner) => owner.detach_owned(identity),
            EnterState::Admitted(owner) => owner.detach_owned(identity),
            EnterState::Boot(owner) => owner.detach_owned(identity),
            EnterState::Gate(owner) => owner.detach_owned(identity),
            EnterState::Raised(owner) => owner.detach_owned(identity),
            EnterState::Ready(owner) => owner.detach_owned(identity),
            EnterState::Entered(_) => unreachable!(),
        };
        Ok(Box::new(FoundationExpiredEnter {
            identity: parts.identity,
            held: parts.held,
            listener: Some(
                parts
                    .listener
                    .or_else(|| self.listener.take().map(|owner| *owner))
                    .expect("the original startup retains its listener"),
            ),
            gate: parts.gate,
            residency: self.residency,
            gate_request: self.gate,
            detail: self.detail,
        }))
    }
}
