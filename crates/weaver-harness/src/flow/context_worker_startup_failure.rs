//! Move the failed startup's actual custody back into its one outer run.
use super::*;
use crate::flow::clock::{AbsoluteBound, WaitInterest};
use crate::flow::context::failed_enter_driver::{
    FailedEnterProgress, FailedRunCleanup, OwnedChild, ResidentBootResources,
};
use crate::flow::context::*;
use crate::flow::pump::{RetiredLifecycleListener, enter::RetiredEnter};
use crate::flow::pump_transport::{OrganPort, OrganSend, PacketPort, PortIdentity, ReceivedOrgan};
use crate::flow::templates::TemplateCertificate;
use crate::tools_cgroup::ToolsCgroup;
use nix::poll::PollFlags;
use std::sync::Arc;
use weaver_types::{
    ExchangeId, LifecycleAnswer, LifecycleDirective, Opener, OrganEnvelope, Position,
};

/// This identity is sampled from the borrowed actual ports, never caller input.
pub(in crate::flow::context) struct StartupIdentity {
    origin: Arc<()>,
    decode: Option<PortIdentity>,
    residency: Option<PortIdentity>,
    deadline: Option<u64>,
}
impl StartupIdentity {
    pub(in crate::flow::context) fn from_resources(
        context: &RecordingContext,
        resources: &FailedEnterResources,
    ) -> Option<Self> {
        if !startup_failed(context) || !resources.matches(context) || !resources.original_ports() {
            return None;
        }
        Some(Self {
            origin: Arc::clone(&context.read_origin),
            decode: resources
                .decode_present_for_serving()
                .map(|port| port.port_identity()),
            residency: resources.residency_port().map(|port| port.port_identity()),
            deadline: context.flow.halt()?.deadline().ok(),
        })
    }
    pub(in crate::flow::context) fn from_boot(
        context: &RecordingContext,
        decode: &crate::flow::pump_transport::DecodePort,
        resident: Option<&ResidentBootResources<'_>>,
    ) -> Option<Self> {
        let resident = resident?;
        if !startup_failed(context) || !Arc::ptr_eq(&context.read_origin, &resident.evidence.origin)
        {
            return None;
        }
        Some(Self {
            origin: Arc::clone(&context.read_origin),
            decode: Some(decode.port_identity()),
            residency: Some(resident.residency.port_identity()),
            deadline: context.flow.halt()?.deadline().ok(),
        })
    }
    fn matches(&self, run: &FoundationRun) -> bool {
        Arc::ptr_eq(&self.origin, &run.context.read_origin)
            && startup_failed(&run.context)
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
            && run
                .context
                .flow
                .halt()
                .and_then(|halt| halt.deadline().ok())
                == self.deadline
            && run.original.is_none()
            && run.listener.is_none()
    }
}

fn startup_failed(context: &RecordingContext) -> bool {
    context.phase == RecordingPhase::Failed
        && context.failure.is_some()
        && context.flow.preinput_failed_owner()
        && !context.ever_started_turn
        && context.open_turn.is_none()
        && context.input.snapshot().is_none()
        && context.member_capture.is_none()
        && !context.place_leaving
        && context
            .deadlines
            .get(crate::flow::pump::PumpDeadline::Leave)
            .is_none()
}

/// These proofs remain retained evidence after failure, never live authority.
pub(in crate::flow::context) struct HeldBoot {
    pub resident: Option<HeldResident>,
    pub inventory: Option<CompiledInventory>,
    pub certificate: Option<TemplateCertificate>,
    pub prior: Option<PriorBootOpen>,
    pub repaired: Option<RepairedBootPairs>,
    pub summary: Option<BootPlanSummary>,
    pub stage: super::super::boot_composition::BootStage,
    pub started: bool,
    pub stop: Option<super::super::boot_composition::BootCompositionStop>,
}
pub(in crate::flow::context) struct HeldResident {
    origin: Arc<()>,
    enter: AbsoluteBound,
    startup: super::super::startup_composition::StartupEvidence,
    request: ResidencyRequest,
    reply: Option<ReceivedOrgan>,
}
impl From<super::super::residency_composition::ResidencyEvidence> for HeldResident {
    fn from(evidence: super::super::residency_composition::ResidencyEvidence) -> Self {
        let super::super::residency_composition::ResidencyEvidence {
            origin,
            enter,
            startup,
            request,
            reply,
        } = evidence;
        Self {
            origin,
            enter,
            startup,
            request,
            reply: Some(reply),
        }
    }
}
pub(in crate::flow::context) struct HeldGate {
    pub boot: HeldBoot,
    pub request: GateStartupRequest,
    pub enter: Option<AbsoluteBound>,
    pub stage: GateStartupStage,
    pub raise: OrganEnvelope,
    pub send: Option<OrganSend>,
    pub reply: Option<ReceivedOrgan>,
    pub returned: Option<OrganEnvelope>,
    pub stop: Option<super::super::boot_composition::gate_startup::GateStartupStop>,
}
// The whole account is boxed. Keep the original stage evidence together rather
// than adding another allocation on the failed handoff path.
#[allow(clippy::large_enum_variant)]
pub(in crate::flow::context) enum HeldStartup {
    Initial(super::super::startup_composition::StoppedStartupAccount),
    Residency(super::super::residency_composition::StoppedResidency),
    Admitted(CompiledInventory, HeldResident),
    Boot(HeldBoot),
    Gate(HeldGate),
    Ready {
        gate: HeldGate,
        answer: OrganEnvelope,
        enter: AbsoluteBound,
        stage: ReadyStage,
        send: Option<OrganSend>,
        returned: Option<OrganEnvelope>,
        stop: Option<super::super::boot_composition::gate_startup::ready::ReadyStop>,
    },
}
impl HeldStartup {
    pub(in crate::flow::context) fn original_enter(&self) -> Option<&ReceivedEnter> {
        match self {
            Self::Initial(account) => account.original_enter.as_ref(),
            Self::Residency(account) => account.startup.original_enter.as_ref(),
            Self::Admitted(_, resident) => resident.startup.original_enter.as_ref(),
            Self::Boot(boot) => boot.resident.as_ref()?.startup.original_enter.as_ref(),
            Self::Gate(gate) | Self::Ready { gate, .. } => {
                gate.boot.resident.as_ref()?.startup.original_enter.as_ref()
            }
        }
    }
    pub(in crate::flow::context) fn retire_enter(&mut self) -> Option<RetiredEnter> {
        let source = match self {
            Self::Initial(account) => &mut account.original_enter,
            Self::Residency(account) => &mut account.startup.original_enter,
            Self::Admitted(_, evidence) => &mut evidence.startup.original_enter,
            Self::Boot(boot) => &mut boot.resident.as_mut()?.startup.original_enter,
            Self::Gate(gate) | Self::Ready { gate, .. } => {
                &mut gate.boot.resident.as_mut()?.startup.original_enter
            }
        };
        source.take().map(|original| original.retire_failed())
    }
    pub(in crate::flow::context) fn initial_grants(&self) -> Option<&InspectionCompletion> {
        match self {
            Self::Initial(account) => account.initial_grants.as_ref(),
            Self::Residency(account) => Some(&account.startup.initial_grants),
            Self::Admitted(_, resident) => Some(&resident.startup.initial_grants),
            Self::Boot(boot) => Some(&boot.resident.as_ref()?.startup.initial_grants),
            Self::Gate(gate) | Self::Ready { gate, .. } => {
                Some(&gate.boot.resident.as_ref()?.startup.initial_grants)
            }
        }
    }
    pub(in crate::flow::context) fn take_admit_reply(&mut self) -> Option<ReceivedOrgan> {
        match self {
            Self::Residency(account) => account.reply.take(),
            Self::Initial(_) => None,
            Self::Admitted(_, resident) => resident.reply.take(),
            Self::Boot(boot) => boot.resident.as_mut()?.reply.take(),
            Self::Gate(gate) | Self::Ready { gate, .. } => {
                gate.boot.resident.as_mut()?.reply.take()
            }
        }
    }
}

pub(in crate::flow::context) struct StartupGate {
    pub port: Option<OrganPort>,
    pub child: Option<OwnedChild>,
    pub tools: Option<ToolsCgroup>,
}
pub(in crate::flow::context) struct DetachedStartup<I = StartupIdentity> {
    pub identity: I,
    pub held: Box<HeldStartup>,
    pub listener: Option<LifecycleListener>,
    pub gate: Option<StartupGate>,
}

/// Exact startup custody after releasing its borrows. A foreign run cannot
/// accept it, and no caller can replace the first failure or its deadline.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationStartupHandoff>();
/// ```
pub struct FoundationStartupHandoff {
    identity: StartupIdentity,
    held: Box<HeldStartup>,
    listener: Option<RetiredLifecycleListener>,
    original: Option<RetiredEnter>,
    gate: Option<StartupGate>,
    residency: Option<Box<ResidencyRequest>>,
    gate_request: Option<Box<GateStartupRequest>>,
    detail: Option<String>,
}

impl FoundationEnter<'_> {
    /// Only an actual component failure may leave the borrowed startup road.
    /// Healthy refusal, interruption and Enter expiry return their whole owner.
    pub fn failed_handoff(mut self) -> Result<Box<FoundationStartupHandoff>, Self> {
        if self.progress() != FoundationEnterProgress::Stopped {
            return Err(self);
        }
        let identity = match self.state.as_ref().expect("retained startup") {
            EnterState::Startup(owner) => owner.failed_identity(),
            EnterState::Residency(owner) => owner.failed_identity(),
            EnterState::Admitted(owner) => owner.failed_identity(),
            EnterState::Boot(owner) => owner.failed_identity(),
            EnterState::Gate(owner) => owner.failed_identity(),
            EnterState::Raised(owner) => owner.failed_identity(),
            EnterState::Ready(owner) => owner.failed_identity(),
            EnterState::Entered(_) => None,
        };
        let Some(identity) = identity else {
            return Err(self);
        };
        let state = self.state.take().unwrap();
        let mut parts = match state {
            EnterState::Startup(owner) => owner.detach_owned(identity),
            EnterState::Residency(owner) => owner.detach_owned(identity),
            EnterState::Admitted(owner) => owner.detach_owned(identity),
            EnterState::Boot(owner) => owner.detach_owned(identity),
            EnterState::Gate(owner) => owner.detach_owned(identity),
            EnterState::Raised(owner) => owner.detach_owned(identity),
            EnterState::Ready(owner) => owner.detach_owned(identity),
            EnterState::Entered(_) => unreachable!(),
        };
        let original = parts.held.retire_enter();
        let listener = parts
            .listener
            .or_else(|| self.listener.take().map(|listener| *listener));
        Ok(Box::new(FoundationStartupHandoff {
            identity: parts.identity,
            held: parts.held,
            original,
            listener: listener.map(LifecycleListener::retire_failed),
            gate: parts.gate,
            residency: self.residency,
            gate_request: self.gate,
            detail: self.detail,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationStartupExitProgress {
    Progress,
    Pending,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitStage {
    Reporting,
    Gate,
    Containment,
    Release,
    Closed,
}

/// The outer run and all stopped startup evidence stay owned through failed
/// release and drain. Closed permits only the worker's existing failed exit.
pub struct FoundationStartupExit {
    tail: FailedRunCleanup,
    handoff: Box<FoundationStartupHandoff>,
    stage: ExitStage,
    progress: FoundationStartupExitProgress,
    lower: Option<OrganEnvelope>,
    lower_send: Option<OrganSend>,
    lower_reply: Option<ReceivedOrgan>,
    received: Vec<ReceivedOrgan>,
    retired_gate: Option<PacketPort<OrganEnvelope, ()>>,
    containment_deadline: Option<u64>,
    containment_started: bool,
    contained: bool,
    fallback: bool,
}
impl FoundationRun {
    /// Reunite the exact run with its consumed failed startup, including proofs
    /// transferred out of primitive owners. A refused join returns both whole.
    #[allow(clippy::result_large_err)]
    pub fn failed_startup_exit(
        self: Box<Self>,
        mut handoff: Box<FoundationStartupHandoff>,
    ) -> Result<Box<FoundationStartupExit>, (Box<Self>, Box<FoundationStartupHandoff>)> {
        if !handoff.identity.matches(&self) {
            return Err((self, handoff));
        }
        let FoundationRun {
            context,
            resources,
            listener: _,
            original: _,
        } = *self;
        let mut tail = FailedRunCleanup::after_startup(context, resources);
        if let Some(reply) = handoff.held.take_admit_reply() {
            tail.retain_residency_completion(reply);
        }
        Ok(Box::new(FoundationStartupExit {
            tail,
            handoff,
            stage: ExitStage::Reporting,
            progress: FoundationStartupExitProgress::Progress,
            lower: None,
            lower_send: None,
            lower_reply: None,
            received: vec![],
            retired_gate: None,
            containment_deadline: None,
            containment_started: false,
            contained: false,
            fallback: false,
        }))
    }
}
impl FoundationStartupExit {
    pub fn progress(&self) -> FoundationStartupExitProgress {
        self.progress
    }
    pub fn process_exit(&self) -> Option<crate::FoundationProcessExit> {
        (self.progress == FoundationStartupExitProgress::Closed)
            .then_some(crate::FoundationProcessExit::Failure)
    }
    pub fn advance(&mut self) -> FoundationStartupExitProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    pub fn wait_for_progress(&mut self) -> FoundationStartupExitProgress {
        self.wait_with_io(&mut MonotonicIo)
    }
    pub fn channel_fault(&self) -> Option<crate::ChannelFault> {
        (self.stage == ExitStage::Closed).then_some(crate::ChannelFault::Closed)
    }
    pub fn unload_sequence(&self) -> Option<u64> {
        self.tail.unload_sequence().map(|s| s.0)
    }
    pub fn abandoned(&self) -> bool {
        self.tail.abandoned()
    }
    fn gate(&self) -> Option<&HeldGate> {
        match self.handoff.held.as_ref() {
            HeldStartup::Gate(gate) | HeldStartup::Ready { gate, .. } => Some(gate),
            _ => None,
        }
    }
    fn raised(&self) -> bool {
        let Some(gate) = self.gate() else {
            return false;
        };
        let Some(port) = self
            .handoff
            .gate
            .as_ref()
            .and_then(|arms| arms.port.as_ref())
        else {
            return false;
        };
        gate.reply.as_ref().is_some_and(|reply| {
            reply.belongs_to(port)
                && reply.envelope().position == Position::Close
                && matches!(
                    reply.envelope().payload,
                    weaver_types::Payload::Answer(LifecycleAnswer::GateReady)
                )
                && gate.send.as_ref().is_some_and(|send| {
                    send.sent()
                        && send.belongs_to(port)
                        && send.matches_exchange(&reply.envelope().exchange)
                        && send.sent_at().is_some_and(|now| {
                            gate.enter
                                .is_some_and(|enter| now >= enter.began_ns() && !enter.expired(now))
                        })
                })
        })
    }
    fn retire_gate(&mut self) {
        if let Some(port) = self.handoff.gate.as_mut().and_then(|arms| arms.port.take()) {
            self.retired_gate = Some(port.retire_failed());
        }
    }
    fn abort(&mut self) {
        self.retire_gate();
        if let Some(arms) = &mut self.handoff.gate {
            if let Some(tools) = arms.tools.take() {
                if let Err(error) = tools.kill() {
                    self.tail
                        .secondary(format!("failed startup tools kill: {error}"));
                    tools.signal_pinned_pass();
                }
                self.contained |= tools.populated() == Some(false);
            }
            if let Some(child) = &mut arms.child {
                child.stop_at_cap();
            }
        }
        self.tail.abort();
        self.stage = ExitStage::Closed;
        self.progress = FoundationStartupExitProgress::Closed;
    }
    fn clock(&mut self, io: &mut impl PumpIo) -> bool {
        if !self.tail.clock(io) {
            self.abort();
            false
        } else {
            true
        }
    }
    fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationStartupExitProgress {
        use FoundationStartupExitProgress as Progress;
        if self.stage == ExitStage::Closed {
            return Progress::Closed;
        }
        if !self.clock(io) {
            return self.progress;
        }
        self.progress = match self.stage {
            ExitStage::Reporting => {
                // The tail's first step attempts the first fault, and no gate
                // action or trace-only Open work can precede that attempt.
                match self.tail.advance(io) {
                    FailedEnterProgress::Closed => {
                        self.abort();
                        Progress::Closed
                    }
                    _ => {
                        self.stage = if self.handoff.gate.is_none() {
                            ExitStage::Release
                        } else if self.raised() {
                            ExitStage::Gate
                        } else {
                            ExitStage::Containment
                        };
                        Progress::Progress
                    }
                }
            }
            ExitStage::Gate | ExitStage::Containment => {
                if !self.tail.advance_accounts(io) {
                    self.abort();
                    Progress::Closed
                } else if self.stage == ExitStage::Gate {
                    self.lower(io)
                } else {
                    self.containment()
                }
            }
            ExitStage::Release => {
                if let Some(child) = self
                    .handoff
                    .gate
                    .as_mut()
                    .and_then(|arms| arms.child.as_mut())
                {
                    child.poll();
                }
                match self.tail.advance(io) {
                    FailedEnterProgress::Progress => Progress::Progress,
                    FailedEnterProgress::Pending => Progress::Pending,
                    FailedEnterProgress::Closed => {
                        if let Some(child) = self
                            .handoff
                            .gate
                            .as_mut()
                            .and_then(|arms| arms.child.as_mut())
                        {
                            child.stop_at_cap();
                        }
                        if let Some(arms) = &mut self.handoff.gate {
                            arms.tools.take();
                        }
                        self.stage = ExitStage::Closed;
                        Progress::Closed
                    }
                }
            }
            ExitStage::Closed => Progress::Closed,
        };
        if self.stage != ExitStage::Closed {
            self.clock(io);
        }
        self.progress
    }
    fn containment(&mut self) -> FoundationStartupExitProgress {
        let arms = self.handoff.gate.as_mut().expect("retained gate arms");
        if !self.containment_started {
            self.containment_started = true;
            self.containment_deadline = Some(
                self.tail
                    .context
                    .last_ns
                    .saturating_add(crate::tools_cgroup::CONTAINMENT_BOUND.as_nanos() as u64)
                    .min(self.handoff.identity.deadline.unwrap()),
            );
            if let Some(child) = &mut arms.child {
                child.stop_at_cap();
            }
            if let Some(tools) = &arms.tools
                && let Err(error) = tools.kill()
            {
                self.fallback = true;
                self.tail
                    .secondary(format!("failed startup tools kill: {error}"));
            }
        }
        self.contained = arms.tools.as_ref().is_none_or(|tools| {
            if self.fallback {
                tools.signal_pinned_pass();
            }
            tools.populated() == Some(false)
        });
        if let Some(child) = &mut arms.child {
            child.poll();
        }
        if self.contained || self.tail.context.last_ns >= self.containment_deadline.unwrap() {
            if !self.contained {
                self.tail
                    .secondary("failed startup tools containment unavailable at its capped bound");
            }
            self.retire_gate();
            self.stage = ExitStage::Release;
            FoundationStartupExitProgress::Progress
        } else {
            FoundationStartupExitProgress::Pending
        }
    }
    fn lower(&mut self, io: &mut impl PumpIo) -> FoundationStartupExitProgress {
        use FoundationStartupExitProgress as Progress;
        if self.lower.is_none() {
            let Some(ordinal) = self.gate().unwrap().raise.exchange.ordinal.checked_add(1) else {
                self.tail
                    .secondary("failed startup Lower ordinal exhausted");
                self.stage = ExitStage::Containment;
                return Progress::Progress;
            };
            self.lower = Some(OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Harness,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Directive(LifecycleDirective::Lower),
            });
        }
        let port = self.handoff.gate.as_mut().unwrap().port.as_mut().unwrap();
        let receive_error = if port.held().is_none() {
            port.try_receive().err()
        } else {
            None
        };
        if port.held().is_some() && self.received.try_reserve(1).is_err() {
            self.tail
                .secondary("failed startup cannot retain another gate packet");
            self.stage = ExitStage::Containment;
            return Progress::Progress;
        }
        match port.take_received() {
            Ok(Some(reply)) => {
                let frame = reply.envelope();
                if frame.exchange.opener == Opener::Gate
                    && frame.position == Position::Open
                    && matches!(frame.payload, weaver_types::Payload::Frame(_))
                {
                    self.received.push(reply);
                    return Progress::Progress;
                }
                let confirmed = frame.position == Position::Close
                    && matches!(
                        frame.payload,
                        weaver_types::Payload::Answer(LifecycleAnswer::GateStopped)
                    )
                    && self.lower_send.as_ref().is_some_and(|send| {
                        send.sent()
                            && send.belongs_to(port)
                            && send.matches_exchange(&frame.exchange)
                            && send.sent_at().is_some()
                    });
                self.lower_reply = Some(reply);
                self.contained = confirmed
                    && self
                        .handoff
                        .gate
                        .as_ref()
                        .unwrap()
                        .tools
                        .as_ref()
                        .is_some_and(|tools| tools.populated() == Some(false));
                if self.contained {
                    self.retire_gate();
                    self.stage = ExitStage::Release;
                } else {
                    self.tail.secondary("failed startup Lower lacks an exact sent GateStopped and empty tools group");
                    self.stage = ExitStage::Containment;
                }
                return Progress::Progress;
            }
            Err(error) => {
                self.tail
                    .secondary(format!("failed startup Lower completion: {error:?}"));
                self.stage = ExitStage::Containment;
                return Progress::Progress;
            }
            Ok(None) => {}
        }
        if let Some(error) = receive_error {
            self.tail
                .secondary(format!("failed startup Lower receive: {error:?}"));
            self.stage = ExitStage::Containment;
            return Progress::Progress;
        }
        if self.lower_send.is_none() {
            match port.stage_tracked(self.lower.as_ref().unwrap().clone()) {
                Ok(crate::flow::pump_transport::TrackedStage::Queued(send)) => {
                    self.lower_send = Some(send)
                }
                Ok(crate::flow::pump_transport::TrackedStage::Busy(_)) => return Progress::Pending,
                Err(error) => {
                    self.tail
                        .secondary(format!("failed startup Lower stage: {:?}", error.failure));
                    self.stage = ExitStage::Containment;
                    return Progress::Progress;
                }
            }
        }
        if let Err(error) = port.try_send_observed(io) {
            self.tail
                .secondary(format!("failed startup Lower send: {error:?}"));
            self.stage = ExitStage::Containment;
            return Progress::Progress;
        }
        Progress::Pending
    }
    fn wait_with_io(&mut self, io: &mut impl PumpIo) -> FoundationStartupExitProgress {
        if self.stage == ExitStage::Closed
            || self.progress != FoundationStartupExitProgress::Pending
        {
            return self.progress;
        }
        if !self.clock(io) {
            return self.progress;
        }
        if self.stage == ExitStage::Release {
            if self.tail.wait_for_progress(io) == FailedEnterProgress::Closed {
                self.abort();
            }
            return self.progress;
        }
        // A raised gate follows completed boot, so Lower can await its socket
        // under the original halt cap. Containment also polls actual children
        // and any owed account; its short wake adds no new work deadline.
        let mut wake = self.handoff.identity.deadline.unwrap();
        if self.stage == ExitStage::Containment {
            wake = wake.min(self.tail.context.last_ns.saturating_add(10_000_000));
        }
        if let Some(cap) = self.containment_deadline {
            wake = wake.min(cap);
        }
        if let Err(error) = self.tail.context.barrier.drain_capacity_wake() {
            self.tail
                .secondary(format!("failed startup capacity wake: {error:?}"));
            self.abort();
            return self.progress;
        }
        let mut interests = vec![WaitInterest {
            fd: self.tail.context.barrier.capacity_fd(),
            events: PollFlags::POLLIN,
        }];
        if self.stage == ExitStage::Gate
            && let Some(port) = self
                .handoff
                .gate
                .as_ref()
                .and_then(|arms| arms.port.as_ref())
        {
            interests.push(WaitInterest {
                fd: port.fd(),
                events: port.interest().events(),
            });
        }
        if let Err(error) = io.wait(&interests, Some(wake)) {
            self.tail.secondary(format!("failed startup wait: {error}"));
            self.abort();
        } else {
            self.clock(io);
        }
        self.progress
    }
}

#[cfg(test)]
#[path = "context_worker_startup_failure_tests.rs"]
pub(super) mod tests;
