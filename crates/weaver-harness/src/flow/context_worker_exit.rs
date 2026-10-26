//! Consume the failed serving seat into its original run, then close and release
//! its actual resources. No borrowed decoder is replaced or mistaken for closed.
use super::*;
use crate::flow::context::failed_enter_driver::{
    FailedEnterProgress, FailedRunCleanup, OwnedChild,
};
use crate::flow::context::worker_startup::FoundationRun;
use crate::flow::pump::{RetiredLifecycleListener, enter::RetiredEnter};
use crate::flow::pump_transport::{OrganPort, PacketPort, PortIdentity};
use crate::flow::templates::TemplateCertificate;
use crate::tools_cgroup::ToolsCgroup;
use std::sync::Arc;
use weaver_trace::{Sequence, StopReason, TurnClose};
use weaver_types::{ExchangeId, LifecycleAnswer, LifecycleDirective};

/// Move-only custody after ending the entered seat's loans. Only the original
/// FoundationRun can accept it. This is not a release or a failed-exit proof.
///
/// ```compile_fail,E0277
/// fn clone_required<T: Clone>() {}
/// clone_required::<weaver_harness::FoundationFailedHandoff>();
/// ```
pub struct FoundationFailedHandoff {
    origin: Arc<()>,
    decode: PortIdentity,
    residency: PortIdentity,
    deadline: Option<u64>,
    progress: FoundationHaltProgress,
    world_already_sent: bool,
    input: FailedInput,
    management: Option<super::super::management::FailedManagement>,
    gate: Option<OrganPort>,
    gate_child: OwnedChild,
    tools: Option<ToolsCgroup>,
    gate_outgoing: u64,
    received: Vec<ReceivedOrgan>,
    _listener: RetiredLifecycleListener,
    _enter_peer: Option<RetiredEnter>,
    _startup: crate::flow::context::residency_composition::ResidencyEvidence,
    _certificate: Option<TemplateCertificate>,
    _gate_request: crate::flow::context::boot_composition::gate_startup::GateStartupRequest,
    _gate_raise: OrganEnvelope,
    _gate_send: OrganSend,
    _gate_reply: ReceivedOrgan,
    _ready: OrganSend,
    _answer: OrganEnvelope,
    _enter: crate::flow::clock::AbsoluteBound,
    _gate_exchanges: std::collections::HashSet<u64>,
    _queued_inputs: std::collections::VecDeque<ReceivedOrgan>,
    _account_failure: Option<HaltAccountFailure>,
    _reporting: bool,
    _rejection_unavailable: bool,
    _capacity_unavailable: bool,
    _detail: Option<String>,
}

/// Every original input/engine field survives the end of its mutable run loans.
struct FailedInput {
    received: Option<ReceivedOrgan>,
    turn: Option<TurnKey>,
    _stage: Stage,
    _token: Option<AcceptedInput>,
    _seated: Option<SeatedInput>,
    _close: Option<OrganEnvelope>,
    _send: Option<OrganSend>,
    _report: Option<(ReceivedOrgan, Subsystem)>,
    _report_started: bool,
    _returned_report: StoppedAuthor,
    _failure: Option<RunFailure>,
    _engine: Option<Box<generation::EngineWork>>,
}

impl FoundationHalt<'_> {
    /// End the resource loans only after interruption has reached a retained
    /// terminal, needs actual containment, or exhausted its original cap.
    pub fn handoff(self: Box<Self>) -> Result<Box<FoundationFailedHandoff>, Box<Self>> {
        if !self.terminal() {
            return Err(self);
        }
        let Self {
            owner,
            progress,
            deadline,
            reporting,
            account_failure,
            received,
            rejection_unavailable,
            capacity_unavailable,
            detail,
        } = *self;
        let (input, management) = match owner {
            Owner::Input(input) => (input, None),
            Owner::Management(owner) => {
                let (input, evidence) = owner.into_failed_parts();
                (input, Some(evidence))
            }
        };
        let world_already_sent = input.failed_world_was_sent()
            || management
                .as_ref()
                .is_some_and(|owner| owner.world_was_sent());
        let FoundationInput {
            serving,
            stage,
            received: original,
            token,
            turn,
            seated,
            close,
            send,
            report,
            report_started,
            returned_report,
            failure,
            engine,
        } = *input;
        let FoundationServing {
            owner,
            gate_exchanges,
            queued_inputs,
            gate_outgoing,
        } = serving;
        let crate::flow::context::boot_composition::gate_startup::ready::EnteredServing {
            context,
            decode,
            mut resident,
            certificate,
            listener,
            gate,
            gate_child,
            tools,
            gate_request,
            gate_raise,
            gate_send,
            gate_reply,
            ready,
            answer,
            enter,
        } = *owner;
        let origin = Arc::clone(&context.read_origin);
        let decode = decode.port_identity();
        let residency = resident.residency.port_identity();
        let enter_peer = resident
            .evidence
            .startup
            .original_enter
            .take()
            .map(|peer| peer.retire_failed());
        Ok(Box::new(FoundationFailedHandoff {
            origin,
            decode,
            residency,
            deadline,
            progress,
            world_already_sent,
            input: FailedInput {
                received: original,
                turn,
                _stage: stage,
                _token: token,
                _seated: seated,
                _close: close,
                _send: send,
                _report: report,
                _report_started: report_started,
                _returned_report: returned_report,
                _failure: failure,
                _engine: engine,
            },
            management,
            gate: Some(gate),
            gate_child,
            tools: Some(tools),
            gate_outgoing,
            received,
            _listener: listener.retire_failed(),
            _enter_peer: enter_peer,
            _startup: resident.evidence,
            _certificate: certificate,
            _gate_request: gate_request,
            _gate_raise: gate_raise,
            _gate_send: gate_send,
            _gate_reply: gate_reply,
            _ready: ready,
            _answer: answer,
            _enter: enter,
            _gate_exchanges: gate_exchanges,
            _queued_inputs: queued_inputs,
            _account_failure: account_failure,
            _reporting: reporting,
            _rejection_unavailable: rejection_unavailable,
            _capacity_unavailable: capacity_unavailable,
            _detail: detail,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationExitProgress {
    Progress,
    Pending,
    Closed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExitStage {
    Containment,
    Terminals,
    World,
    Lower,
    Release,
    Closed,
}

/// Owns the failed run, actual process/transport resources and all retained
/// evidence. It exposes neither a serving seat nor a successful Left result.
pub struct FoundationFailedExit {
    tail: FailedRunCleanup,
    handoff: Box<FoundationFailedHandoff>,
    stage: ExitStage,
    progress: FoundationExitProgress,
    retired_gate: Option<PacketPort<OrganEnvelope, ()>>,
    containment_deadline: Option<u64>,
    containment_started: bool,
    contained: bool,
    fallback: bool,
    lower: Option<OrganEnvelope>,
    lower_send: Option<OrganSend>,
    lower_reply: Option<ReceivedOrgan>,
    lower_confirmed: bool,
    world: Option<OrganEnvelope>,
    world_send: Option<OrganSend>,
    world_attempted: bool,
    world_sent: bool,
    withdrawn_world: Option<OrganEnvelope>,
    terminal_events: Vec<AuthoredEvent>,
    terminal_sequences: Vec<Sequence>,
    terminal_attempted: bool,
    pending_turn_close: Option<crate::flow::recording::DeferredHaltClose>,
    abandoned: bool,
}

impl FoundationRun {
    /// Match the unique source run and both actual SPU ports. Refusal returns
    /// both complete owners unchanged; a different run cannot release them.
    #[allow(clippy::result_large_err)]
    pub fn failed_exit(
        self: Box<Self>,
        handoff: Box<FoundationFailedHandoff>,
    ) -> Result<Box<FoundationFailedExit>, (Box<Self>, Box<FoundationFailedHandoff>)> {
        let valid = Arc::ptr_eq(&self.context.read_origin, &handoff.origin)
            && self.context.phase == RecordingPhase::Failed
            && self.context.failure.is_some()
            && self.context.flow.halt().is_some()
            && self.context.member_capture.is_none()
            && self.resources.matches(&self.context)
            && self.resources.original_ports()
            && self
                .resources
                .decode_present_for_serving()
                .is_some_and(|port| handoff.decode.matches_decode(port))
            && self
                .resources
                .residency_port()
                .is_some_and(|port| handoff.residency.matches(port))
            && self.context.flow.halt().unwrap().deadline().ok() == handoff.deadline
            && self.original.is_none()
            && self.listener.is_none();
        if !valid {
            return Err((self, handoff));
        }
        let FoundationRun {
            context,
            resources,
            listener: _,
            original: _,
        } = *self;
        let abandoned = handoff.progress == FoundationHaltProgress::Expired;
        let stage = if handoff.progress == FoundationHaltProgress::NeedsContainment {
            ExitStage::Containment
        } else {
            ExitStage::Terminals
        };
        Ok(Box::new(FoundationFailedExit {
            tail: FailedRunCleanup::after_serving(context, resources),
            handoff,
            stage,
            progress: FoundationExitProgress::Progress,
            retired_gate: None,
            containment_deadline: None,
            containment_started: false,
            contained: false,
            fallback: false,
            lower: None,
            lower_send: None,
            lower_reply: None,
            lower_confirmed: false,
            world: None,
            world_send: None,
            world_attempted: false,
            world_sent: false,
            withdrawn_world: None,
            terminal_events: vec![],
            terminal_sequences: vec![],
            terminal_attempted: false,
            pending_turn_close: None,
            abandoned,
        }))
    }
}

impl FoundationFailedExit {
    pub fn progress(&self) -> FoundationExitProgress {
        self.progress
    }
    pub fn advance(&mut self) -> FoundationExitProgress {
        self.advance_with_io(&mut MonotonicIo)
    }
    pub fn wait_for_progress(&mut self) -> FoundationExitProgress {
        self.wait_with_io(&mut MonotonicIo)
    }
    /// A closed failed owner always returns the existing failed channel result.
    /// Actual confirmations and unavailability remain inspectable evidence.
    pub fn channel_fault(&self) -> Option<crate::ChannelFault> {
        (self.stage == ExitStage::Closed).then_some(crate::ChannelFault::Closed)
    }
    pub fn unload_sequence(&self) -> Option<u64> {
        self.tail.unload_sequence().map(|s| s.0)
    }
    pub fn abandoned(&self) -> bool {
        self.abandoned || self.tail.abandoned()
    }

    fn retire_gate(&mut self) {
        if let Some(gate) = self.handoff.gate.take() {
            self.retired_gate = Some(gate.retire_failed());
        }
    }
    fn abort(&mut self) {
        self.abandoned = true;
        self.retire_gate();
        if let Some(tools) = self.handoff.tools.take() {
            let killed = tools.kill();
            if let Err(error) = killed {
                self.tail
                    .secondary(format!("failed serving tools kill: {error}"));
                if !tools.signal_pinned_pass() {
                    self.tail
                        .secondary("failed serving tools fallback delivered no confirmed signal");
                }
            }
            self.contained |= tools.populated() == Some(false);
        }
        self.handoff.gate_child.stop_at_cap();
        self.tail.abort();
        self.stage = ExitStage::Closed;
        self.progress = FoundationExitProgress::Closed;
    }
    fn clock(&mut self, io: &mut impl PumpIo) -> bool {
        if self.abandoned || !self.tail.clock(io) {
            self.abort();
            false
        } else {
            true
        }
    }
    pub(in crate::flow::context::worker_startup) fn advance_with_io(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationExitProgress {
        if self.stage == ExitStage::Closed {
            return FoundationExitProgress::Closed;
        }
        if !self.clock(io) {
            return self.progress;
        }
        self.progress = match self.stage {
            ExitStage::Containment => self.containment(),
            ExitStage::Terminals => self.terminals(io),
            ExitStage::World => self.world(io),
            ExitStage::Lower => self.lower(io),
            ExitStage::Release => {
                self.handoff.gate_child.poll();
                match self.tail.advance(io) {
                    FailedEnterProgress::Progress => FoundationExitProgress::Progress,
                    FailedEnterProgress::Pending => FoundationExitProgress::Pending,
                    FailedEnterProgress::Closed => {
                        // The common tail closes SPU/state/classifier. The gate
                        // still belongs here, even if it ignored descriptor EOF.
                        self.handoff.gate_child.stop_at_cap();
                        self.handoff.tools.take();
                        self.stage = ExitStage::Closed;
                        FoundationExitProgress::Closed
                    }
                }
            }
            ExitStage::Closed => FoundationExitProgress::Closed,
        };
        if self.stage != ExitStage::Closed {
            self.clock(io);
        }
        self.progress
    }

    fn containment(&mut self) -> FoundationExitProgress {
        let context = &mut self.tail.context;
        let tools = self
            .handoff
            .tools
            .as_ref()
            .expect("actual entered tools owner");
        if !self.containment_started {
            self.containment_started = true;
            self.containment_deadline = Some(
                context
                    .last_ns
                    .saturating_add(crate::tools_cgroup::CONTAINMENT_BOUND.as_nanos() as u64)
                    .min(self.handoff.deadline.unwrap()),
            );
            self.handoff.gate_child.stop_at_cap();
            if let Err(error) = tools.kill() {
                self.fallback = true;
                self.tail
                    .secondary(format!("failed serving tools kill: {error}"));
            }
        }
        if self.fallback {
            tools.signal_pinned_pass();
        }
        self.handoff.gate_child.poll();
        self.contained = tools.populated() == Some(false);
        if self.contained || self.tail.context.last_ns >= self.containment_deadline.unwrap() {
            if !self.contained {
                self.tail
                    .secondary("failed serving tools containment unavailable at its capped bound");
            }
            // A dead or invalid gate never supplies a fabricated contained ToolAnswer.
            self.retire_gate();
            self.stage = ExitStage::Terminals;
            FoundationExitProgress::Progress
        } else {
            FoundationExitProgress::Pending
        }
    }

    fn terminals(&mut self, io: &mut impl PumpIo) -> FoundationExitProgress {
        if self.terminal_attempted {
            self.stage = ExitStage::World;
            return FoundationExitProgress::Progress;
        }
        // Move the one prepared close into the consuming owner before driving
        // earlier obligations. Neither its ticket nor its payload is replaced.
        if !self.tail.context.place_owns_pending_author()
            && let Some(close) = self.tail.context.barrier.defer_halt_close()
        {
            if close.kind() == serving::Kind::SessionClosed {
                self.tail.pending_session_close = Some(close);
            } else {
                self.pending_turn_close = Some(close);
            }
        }
        let context = &mut self.tail.context;
        let existing_terminal = context.barrier.pending_author().is_some_and(|author| {
            matches!(
                author.event.kind,
                serving::Kind::ToolCallCompleted | serving::Kind::MessageToolResult
            )
        });
        if existing_terminal {
            match context.resume_halt_terminal_and_bind() {
                Ok(Some(sequence)) => self.terminal_sequences.push(sequence),
                Ok(None) => {}
                Err(error) => self
                    .tail
                    .secondary(format!("failed serving pending terminal: {error:?}")),
            }
        }
        if !self.tail.advance_accounts(io) {
            self.abort();
            return FoundationExitProgress::Closed;
        }
        if !self.clock(io) {
            return FoundationExitProgress::Closed;
        }
        // Every pre-existing ordinary author keeps its ordinary capacity and
        // earlier bound. The stopped account drivers grant no continuation.
        if self.tail.serving_accounts_pending() {
            return FoundationExitProgress::Pending;
        }
        let context = &mut self.tail.context;
        self.terminal_attempted = true;
        if let Some(event) = context.failed_gate_terminal_event() {
            self.terminal_events.push(event);
        }
        if self.pending_turn_close.is_none()
            && let Some(turn) = context.open_turn.clone()
        {
            self.terminal_events.push(AuthoredEvent {
                kind: serving::Kind::TurnClosed,
                subsystem: Subsystem::Harness,
                turn: Some(turn),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(serving::Payload::TurnClosed(TurnClose::Stopped {
                    reason: StopReason::Fault,
                    cause: None,
                })),
            });
        }
        for index in 0..self.terminal_events.len() {
            if !self.clock(io) {
                return FoundationExitProgress::Closed;
            }
            let wall = match crate::flow::context::wall_ms() {
                Ok(wall) => wall,
                Err(error) => {
                    self.tail.secondary(error);
                    continue;
                }
            };
            let context = &mut self.tail.context;
            let event = &self.terminal_events[index];
            match context.barrier.submit_terminal_authored_ref(
                event,
                &context.author,
                context.last_ns,
                wall,
            ) {
                Ok(sequence) => {
                    self.terminal_sequences.push(sequence);
                    if event.kind == serving::Kind::TurnClosed {
                        context.open_turn = None;
                    }
                }
                Err(error) => self
                    .tail
                    .secondary(format!("failed serving terminal {:?}: {error}", event.kind)),
            }
        }
        if self.pending_turn_close.is_some() {
            if !self.clock(io) {
                return FoundationExitProgress::Closed;
            }
            match self
                .tail
                .context
                .resume_deferred_halt_close(&mut self.pending_turn_close)
            {
                Ok(Some(sequence)) => self.terminal_sequences.push(sequence),
                Ok(None) => self
                    .tail
                    .secondary("failed serving turn close remains unrecorded"),
                Err(error) => self
                    .tail
                    .secondary(format!("failed serving turn close: {error:?}")),
            }
        }
        self.stage = ExitStage::World;
        FoundationExitProgress::Progress
    }

    fn world(&mut self, io: &mut impl PumpIo) -> FoundationExitProgress {
        if !self.world_attempted {
            self.world_attempted = true;
            if let Some(gate) = &mut self.handoff.gate
                && !self.handoff.world_already_sent
                && let Some(original) = &self.handoff.input.received
                && original.belongs_to(gate)
                && !matches!(
                    self.handoff.input._stage,
                    Stage::Completed | Stage::Refused | Stage::Aborted
                )
            {
                match gate.withdraw_pending(&original.envelope().exchange) {
                    Ok(original) => self.withdrawn_world = original,
                    Err(error) => {
                        self.tail
                            .secondary(format!("failed serving previous world send: {error:?}"));
                        self.stage = ExitStage::Containment;
                        return FoundationExitProgress::Progress;
                    }
                }
                let run = weaver_types::RunId(self.tail.context.flow.run().to_owned());
                let text = crate::lifecycle::render_close_with_finish(
                    "stopped",
                    "reason",
                    "the service stopped",
                    self.handoff.input.turn.as_ref().map(|turn| (turn, &run)),
                    false,
                );
                self.world = Some(OrganEnvelope {
                    exchange: original.envelope().exchange.clone(),
                    position: Position::Close,
                    payload: Payload::Frame(weaver_types::TurnFrame::carry(text.as_bytes())),
                });
            }
        }
        if self.handoff.gate.is_none() || (self.world.is_none() && self.world_send.is_none()) {
            self.stage = ExitStage::Lower;
            return FoundationExitProgress::Progress;
        }
        let gate = self.handoff.gate.as_mut().unwrap();
        if let Some(frame) = self.world.take() {
            match gate.stage_tracked(frame) {
                Ok(TrackedStage::Queued(send)) => self.world_send = Some(send),
                Ok(TrackedStage::Busy(frame)) => {
                    self.world = Some(frame);
                    return FoundationExitProgress::Pending;
                }
                Err(error) => {
                    self.world = Some(error.input);
                    self.tail.secondary(format!(
                        "failed serving stopped response: {:?}",
                        error.failure
                    ));
                    self.stage = ExitStage::Containment;
                    return FoundationExitProgress::Progress;
                }
            }
        }
        if let Err(error) = gate.try_send_observed(io) {
            self.tail
                .secondary(format!("failed serving stopped response send: {error:?}"));
            self.stage = ExitStage::Containment;
            return FoundationExitProgress::Progress;
        }
        if self
            .world_send
            .as_ref()
            .is_some_and(|send| send.sent() && send.sent_at().is_some() && send.belongs_to(gate))
        {
            self.world_sent = true;
            self.stage = ExitStage::Lower;
            FoundationExitProgress::Progress
        } else {
            FoundationExitProgress::Pending
        }
    }

    fn lower(&mut self, io: &mut impl PumpIo) -> FoundationExitProgress {
        if self.tail.context.pairs.pending_rejection().is_some() {
            if let Err(error) = self.tail.context.advance_halt_rejection() {
                self.tail.secondary(error);
            }
            if !self.clock(io) {
                return FoundationExitProgress::Closed;
            }
        }
        if self.handoff.gate.is_none() {
            self.stage = ExitStage::Release;
            return FoundationExitProgress::Progress;
        }
        if self.lower.is_none() {
            let Some(ordinal) = self.handoff.gate_outgoing.checked_add(1) else {
                self.tail
                    .secondary("failed serving Lower ordinal exhausted");
                self.stage = ExitStage::Containment;
                return FoundationExitProgress::Progress;
            };
            self.handoff.gate_outgoing = ordinal;
            self.lower = Some(OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Harness,
                    ordinal,
                },
                position: Position::Open,
                payload: Payload::Directive(LifecycleDirective::Lower),
            });
        }
        let gate = self.handoff.gate.as_mut().unwrap();
        // An answer received before the whole send can never confirm this Lower.
        let receive = if gate.held().is_none() {
            gate.try_receive().err()
        } else {
            None
        };
        if gate
            .held()
            .is_some_and(|frame| matches!(frame.payload, Payload::ToolAnswer(_)))
            && self.tail.context.pairs.pending_rejection().is_some()
        {
            return FoundationExitProgress::Pending;
        }
        if gate.held().is_some() && self.handoff.received.try_reserve(1).is_err() {
            self.tail
                .secondary("failed serving Lower cannot retain another received packet");
            self.stage = ExitStage::Containment;
            return FoundationExitProgress::Progress;
        }
        match gate.take_received() {
            Ok(Some(reply)) => {
                let frame = reply.envelope();
                if matches!(frame.payload, Payload::ToolAnswer(_)) {
                    match self.tail.context.pairs.observe_gate(reply) {
                        Ok(crate::flow::context::pairs::GateObservation::Rejected) => {
                            if let Err(error) = self.tail.context.advance_halt_rejection() {
                                self.tail.secondary(error);
                            }
                        }
                        Ok(_) => self
                            .tail
                            .secondary("failed serving Lower observed an additional tool terminal"),
                        Err((original, error)) => {
                            self.handoff.received.push(original);
                            self.tail
                                .secondary(format!("failed serving Lower tool reply: {error:?}"));
                            self.stage = ExitStage::Containment;
                        }
                    }
                    return FoundationExitProgress::Progress;
                }
                if frame.exchange.opener == Opener::Gate
                    && frame.position == Position::Open
                    && matches!(frame.payload, Payload::Frame(_))
                {
                    self.handoff.received.push(reply);
                    return FoundationExitProgress::Progress;
                }
                self.lower_confirmed = self.lower_send.as_ref().is_some_and(|send| {
                    send.sent()
                        && send.sent_at().is_some()
                        && send.belongs_to(gate)
                        && send.matches_exchange(&frame.exchange)
                }) && frame.position == Position::Close
                    && matches!(frame.payload, Payload::Answer(LifecycleAnswer::GateStopped));
                self.lower_reply = Some(reply);
                if self.lower_confirmed {
                    self.contained =
                        self.handoff.tools.as_ref().unwrap().populated() == Some(false);
                    if self.contained {
                        self.retire_gate();
                        self.stage = ExitStage::Release;
                    } else {
                        self.tail
                            .secondary("failed serving GateStopped lacks an empty tools group");
                        self.stage = ExitStage::Containment;
                    }
                } else {
                    self.tail.secondary(
                        "failed serving Lower received no exact sent-exchange GateStopped",
                    );
                    self.stage = ExitStage::Containment;
                }
                return FoundationExitProgress::Progress;
            }
            Err(error) => {
                self.tail
                    .secondary(format!("failed serving Lower receive: {error:?}"));
                self.stage = ExitStage::Containment;
                return FoundationExitProgress::Progress;
            }
            Ok(None) => {}
        }
        if let Some(error) = receive {
            self.tail
                .secondary(format!("failed serving Lower receive: {error:?}"));
            self.stage = ExitStage::Containment;
            return FoundationExitProgress::Progress;
        }
        if self.lower_send.is_none() {
            match gate.stage_tracked(self.lower.as_ref().unwrap().clone()) {
                Ok(TrackedStage::Queued(send)) => self.lower_send = Some(send),
                Ok(TrackedStage::Busy(_)) => return FoundationExitProgress::Pending,
                Err(error) => {
                    self.tail
                        .secondary(format!("failed serving Lower stage: {:?}", error.failure));
                    self.stage = ExitStage::Containment;
                    return FoundationExitProgress::Progress;
                }
            }
        }
        if let Err(error) = gate.try_send_observed(io) {
            self.tail
                .secondary(format!("failed serving Lower send: {error:?}"));
            self.stage = ExitStage::Containment;
            return FoundationExitProgress::Progress;
        }
        FoundationExitProgress::Pending
    }

    pub(in crate::flow::context::worker_startup) fn wait_with_io(
        &mut self,
        io: &mut impl PumpIo,
    ) -> FoundationExitProgress {
        if self.stage == ExitStage::Closed {
            return FoundationExitProgress::Closed;
        }
        if self.progress != FoundationExitProgress::Pending {
            return self.progress;
        }
        if !self.clock(io) {
            return self.progress;
        }
        if matches!(self.stage, ExitStage::Terminals | ExitStage::Release) {
            if self.tail.wait_for_progress(io) == FailedEnterProgress::Closed {
                self.abort();
            }
            return self.progress;
        }
        let now = self.tail.context.last_ns;
        let deadline = self.handoff.deadline.unwrap();
        let wake = if self.stage == ExitStage::Containment {
            now.saturating_add(10_000_000)
                .min(deadline)
                .min(self.containment_deadline.unwrap())
        } else {
            deadline
        };
        let mut interests = vec![];
        if self.stage == ExitStage::Lower
            && self.tail.context.pairs.pending_rejection().is_some()
            && !self.handoff._capacity_unavailable
        {
            if let Err(error) = self.tail.context.barrier.drain_capacity_wake() {
                self.handoff._capacity_unavailable = true;
                self.tail
                    .secondary(format!("failed serving Lower capacity wake: {error:?}"));
            } else {
                interests.push(WaitInterest {
                    fd: self.tail.context.barrier.capacity_fd(),
                    events: PollFlags::POLLIN,
                });
            }
        }
        if matches!(self.stage, ExitStage::World | ExitStage::Lower)
            && let Some(gate) = &self.handoff.gate
        {
            let mut events = gate.interest().events();
            if self.stage == ExitStage::World {
                events.remove(PollFlags::POLLIN);
            }
            if !events.is_empty() {
                interests.push(WaitInterest {
                    fd: gate.fd(),
                    events,
                });
            }
        }
        if let Err(error) = io.wait(&interests, Some(wake)) {
            self.tail
                .secondary(format!("failed serving close wait: {error}"));
            self.abort();
        } else {
            self.clock(io);
        }
        self.progress
    }
}

#[cfg(test)]
#[path = "context_worker_exit_tests.rs"]
pub(in crate::flow::context) mod tests;
