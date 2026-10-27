//! Actual gate intake joined to the entered worker's one recording context.
//! The loop chooses a turn only after checked acceptance. No caller can mint a
//! gate source, a send receipt, an accepted token or a selected content copy.
use super::super::{InputProgress, RunFailure, SeatedInput, StoppedAuthor};
use super::FoundationServing;
use crate::flow::author::{AuthoredEvent, AuthoredSource};
use crate::flow::clock::{MonotonicIo, PumpIo};
use crate::flow::input::AcceptedInput;
use crate::flow::pump::{LifecyclePeer, LifecyclePorts, PumpPorts, PumpSource};
use crate::flow::pump_transport::{OrganSend, ReceivedOrgan, TrackedStage};
use crate::flow::report::{FlowHaltReason, HaltPoint};
use weaver_trace::{Subsystem, TurnRef, serving};
use weaver_types::{Opener, OrganEnvelope, Payload, Position, TurnKey};

#[path = "context_worker_generation.rs"]
pub(crate) mod generation;
#[path = "context_worker_management.rs"]
pub(crate) mod management;

#[path = "context_worker_wind_down.rs"]
pub(crate) mod wind_down;

#[path = "context_worker_graceful_leave.rs"]
pub(in crate::flow::context::worker_startup) mod graceful;

#[path = "context_worker_save.rs"]
pub(in crate::flow::context::worker_startup) mod save;

#[path = "context_worker_halt.rs"]
pub(crate) mod halt;

#[path = "context_worker_close.rs"]
mod close;

#[path = "context_worker_report.rs"]
mod report;

#[path = "context_worker_input_rollback.rs"]
pub(in crate::flow::context::worker_startup) mod rollback;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationInputProgress {
    Pending,
    Accepted,
    Turn,
    Task,
    Placed,
    Generated,
    Restatement,
    CorrectionPlaced,
    Effective,
    Sorted,
    /// The next selected request has both actual opening receipts. The worker
    /// must explicitly elect its operation; readiness does not dispatch.
    PairReady,
    /// Every conforming request has its actual terminal filing proof.
    PairsComplete,
    /// The clean turn close is Recorded. Its exact world reply was sent or
    /// retained as lost delivery on this run's observed closed gate.
    Completed,
    /// An operation refused. Its full account remains with this owner.
    OperationRefused,
    Refused,
    /// The stopped turn's close was carried or retained as checked lost delivery.
    Aborted,
    /// The actual root connection is retained for management judgment. Mere
    /// acceptance is not a Leave and does not revoke input or grant cleanup.
    ManagementPending,
    /// The accepted graceful Leave retains this seat. Its drain has ended or
    /// needs forced continuation; no new turn or serving seat is available.
    LeavePending,
    /// Quiesce was answered on its exact exchange, all unserved frames have
    /// Recorded refusals and sent replies, and any input check has settled.
    /// Wind-down and the Leave/save tail still require their own elections.
    Quiesced,
    /// The lifecycle wind-down owns a real turn and no world dialer.
    WindDown,
    Stopped,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Waiting,
    Checking,
    Accepted,
    Source,
    Closing,
    Refused,
    Aborted,
    Turn,
    Engine,
    Completed,
    Stopped,
}

/// Owns the serving resources and original input through checking, trace waits,
/// an actual world close, or source authoring. A stopped owner remains owed
/// to lifecycle teardown; dropping it is not completed cleanup.
///
/// ```compile_fail,E0507
/// fn steal(owner: &mut Box<weaver_harness::FoundationInput<'_>>) {
///     let _ = owner.finish();
/// }
/// ```
pub struct FoundationInput<'a> {
    serving: FoundationServing<'a>,
    stage: Stage,
    received: Option<ReceivedOrgan>,
    token: Option<AcceptedInput>,
    turn: Option<TurnKey>,
    seated: Option<SeatedInput>,
    close: Option<OrganEnvelope>,
    send: Option<OrganSend>,
    report: Option<report::Report>,
    failure: Option<RunFailure>,
    engine: Option<Box<generation::EngineWork>>,
}

impl<'a> FoundationServing<'a> {
    /// A later world frame belongs to the serving owner, not the active pair.
    /// The gate admits one unanswered input per retained connection. Keep its
    /// exact receipt here until serialized intake, with no new check or clock.
    fn retain_queued_input(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        let owner = &mut self.owner;
        let Some(frame) = owner.gate.held() else {
            return Ok(());
        };
        if frame.exchange.opener != Opener::Gate || !matches!(frame.payload, Payload::Frame(_)) {
            return Ok(());
        }
        if frame.position != Position::Open
            || self.gate_exchanges.contains(&frame.exchange.ordinal)
            || self
                .queued_inputs
                .iter()
                .any(|queued| queued.envelope().exchange == frame.exchange)
        {
            return Err(owner.context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "queued gate frame is not a fresh opening exchange",
            ));
        }
        let received = owner
            .gate
            .take_received()
            .map_err(|failure| RunFailure::Transport {
                source: PumpSource::Gate,
                failure,
            })?
            .expect("held original gate input");
        self.queued_inputs.push_back(received);
        Ok(())
    }

    /// Consume the unique serving seat. Frames behind this owner remain queued
    /// without an input ID or a new gate deadline.
    pub fn input(self) -> Box<FoundationInput<'a>> {
        Box::new(FoundationInput {
            serving: self,
            stage: Stage::Waiting,
            received: None,
            token: None,
            turn: None,
            seated: None,
            close: None,
            send: None,
            report: None,
            failure: None,
            engine: None,
        })
    }
}

impl<'a> FoundationInput<'a> {
    pub fn progress(&self) -> FoundationInputProgress {
        if self.stage == Stage::Stopped {
            return FoundationInputProgress::Stopped;
        }
        if self.serving.owner.listener.held().is_some() {
            return FoundationInputProgress::ManagementPending;
        }
        if self.save_blocks_turn() {
            return FoundationInputProgress::Pending;
        }
        if let Some(progress) = self.wind_down_input_progress() {
            return progress;
        }
        if let Some(progress) = self.quiesce_progress() {
            return progress;
        }
        if self.graceful_waiting() {
            return FoundationInputProgress::LeavePending;
        }
        if self.graceful_pairs_need_step() {
            return FoundationInputProgress::Pending;
        }
        if self
            .engine
            .as_ref()
            .is_some_and(|work| work.lost_gate_pairs_need_step(self.serving.owner.context))
        {
            return FoundationInputProgress::Pending;
        }
        if self.report.is_some() {
            return FoundationInputProgress::Pending;
        }
        match self.stage {
            Stage::Accepted => FoundationInputProgress::Accepted,
            Stage::Turn => FoundationInputProgress::Turn,
            Stage::Engine => self.engine.as_ref().unwrap().progress(),
            Stage::Refused => FoundationInputProgress::Refused,
            Stage::Aborted => FoundationInputProgress::Aborted,
            Stage::Completed => FoundationInputProgress::Completed,
            _ => FoundationInputProgress::Pending,
        }
    }

    pub fn detail(&self) -> Option<String> {
        self.failure.as_ref().map(|failure| format!("{failure:?}"))
    }

    pub fn advance(&mut self) -> FoundationInputProgress {
        self.advance_with_io(&mut MonotonicIo)
    }

    /// Wait for lifecycle or transport readiness without electing or advancing
    /// an input operation. Existing absolute bounds still apply. Completions
    /// stay with their owners, and a terminal input returns without waiting.
    pub fn wait_for_wake(&mut self) -> FoundationInputProgress {
        self.drive_with_io(&mut MonotonicIo, true)
    }

    fn advance_with_io(&mut self, io: &mut impl PumpIo) -> FoundationInputProgress {
        self.drive_with_io(io, false)
    }

    fn drive_with_io(&mut self, io: &mut impl PumpIo, wait: bool) -> FoundationInputProgress {
        if matches!(
            self.progress(),
            FoundationInputProgress::Stopped | FoundationInputProgress::ManagementPending
        ) || (wait
            && matches!(
                self.progress(),
                FoundationInputProgress::Completed
                    | FoundationInputProgress::Refused
                    | FoundationInputProgress::Aborted
            ))
        {
            return self.progress();
        }
        let listener = self.serving.owner.listener.identity();
        let previous_listener = self
            .serving
            .owner
            .context
            .management_listener
            .replace(listener.clone());
        let result = self
            .prepare_save_window_expiry(io)
            .and_then(|()| self.advance_organ_faults(None, io))
            .and_then(|ready| {
                if !ready {
                    // Pending settlement must wait even when the driver calls advance.
                    // The common pump still serves every socket and original bound.
                    return self.observe_with_wait(io, true);
                }
                self.advance_save(io)?;
                if self.save_blocks_turn() {
                    return Ok(());
                }
                self.observe(io)?;
                if self.progress() == FoundationInputProgress::ManagementPending {
                    Ok(())
                } else if wait {
                    self.serving
                        .owner
                        .context
                        .reduce_flow(io)
                        .and_then(|_| self.observe_with_wait(io, true))
                } else if matches!(
                    self.progress(),
                    FoundationInputProgress::Pending | FoundationInputProgress::WindDown
                ) {
                    self.step(io)
                } else {
                    Ok(())
                }
            });
        self.serving.owner.context.management_listener = previous_listener;
        if let Err(mut failure) = result {
            // A successful send without its clock witness remains fatal even
            // when an organ EOF is the next readiness observation.
            if self.final_send_clock_failed() {
                failure = self.serving.owner.context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("final reply send clock observation failed: {failure:?}"),
                );
            }
            match self.retain_organ_end(&failure, io) {
                Ok(true) => return self.progress(),
                Ok(false) => (),
                Err(error) => failure = error,
            }
            if self.retain_save_interrupt(&failure)
                || self.retain_graceful_expiry(&failure)
                || self.retain_quiesce_gate_end(&failure)
            {
                return self.progress();
            }
            if matches!(failure, RunFailure::ManagementPending)
                && listener.matches(&self.serving.owner.listener)
                && self.serving.owner.listener.held().is_some()
                && self.serving.owner.context.failure().is_none()
                && self.failure.is_none()
            {
                return self.progress();
            }
            if self.serving.save.is_some()
                && matches!(
                    failure,
                    RunFailure::Wait {
                        key: crate::flow::pump::PumpDeadline::CaptureWindow,
                        ..
                    }
                )
            {
                failure = self.serving.owner.context.fail(io, FlowHaltReason::ProtocolFailed, HaltPoint::Barrier, format!("serving save did not complete within its original capture window: {failure:?}"));
            }
            // The common pump reports external transport failure without
            // electing the enclosing owner's disposition. This owner stops
            // serving and revokes input before handing custody to teardown.
            if matches!(failure, RunFailure::Transport { .. }) {
                self.serving.owner.context.fail(
                    io,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    format!("serving input transport: {failure:?}"),
                );
            }
            self.failure = Some(failure);
            self.stage = Stage::Stopped;
        }
        self.progress()
    }

    fn retain_organ_end(
        &mut self,
        failure: &RunFailure,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        let RunFailure::Transport { source, failure } = failure else {
            return Ok(false);
        };
        let owner = &mut self.serving.owner;
        let retained = owner.context.observe_serving_organ_end(
            *source,
            failure,
            crate::flow::context::serving_organ_loss::ServingOrganPorts {
                gate: &owner.gate,
                decode: owner.decode,
                residency: owner.resident.residency,
                tools: &owner.tools,
            },
            self.serving.graceful.is_some(),
            io,
        )?;
        if retained {
            owner
                .context
                .serving_organs
                .refresh(&owner.gate_child, owner.resident.spu);
            if let Some(leave) = &mut self.serving.graceful {
                leave.forced = true;
                owner
                    .context
                    .complete_wait(crate::flow::pump::PumpDeadline::GracefulDrain);
                owner
                    .context
                    .complete_wait(crate::flow::pump::PumpDeadline::WindDown);
            }
        }
        Ok(retained)
    }

    fn advance_organ_faults(
        &mut self,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        // Intake only retains evidence. The existing operation owners reduce
        // due save/Leave waits; observation and report authoring check live
        // before their own work. Do not preempt a save owner's timeout here.
        self.retain_report()?;
        let owner = &mut self.serving.owner;
        owner
            .context
            .serving_organs
            .refresh(&owner.gate_child, owner.resident.spu);
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: peer,
                residency: Some(owner.resident.residency),
                leave_peer: self.serving.save.as_mut().and_then(|save| save.port()),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        // Containment and the terminal accounting bound precede a received
        // report's trace wait. Its exact claim keeps the observation author
        // from borrowing its ticket while the report remains pending.
        let result = owner
            .context
            .advance_serving_organ_faults(&mut ports, &owner.tools)?;
        if matches!(
            result,
            crate::flow::context::serving_organ_loss::ObservationProgress::Pending { .. }
        ) {
            return Ok(false);
        }
        if let Some(report) = &mut self.report
            && report.advance(owner.context, &mut ports)?
        {
            self.report = None;
        }
        Ok(true)
    }

    /// Workflow policy supplies the turn identity only after accepted input is
    /// available. This neither authors a turn nor grants an unchecked message.
    pub fn start_turn(&mut self, turn: TurnKey) -> Result<(), TurnKey> {
        if self.progress() != FoundationInputProgress::Accepted {
            return Err(turn);
        }
        self.turn = Some(turn);
        self.stage = Stage::Source;
        Ok(())
    }

    fn observe(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        self.observe_with_wait(io, false)
    }

    fn observe_with_wait(&mut self, io: &mut impl PumpIo, wait: bool) -> Result<(), RunFailure> {
        // The same pump owns both observation and an explicitly elected wait.
        // Neither changes the operation's clocks or consumes its completion.
        let retained_gate_end = self.retained_quiesce_gate_end();
        let owner = &mut self.serving.owner;
        owner.context.pump_with_wait(
            &mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    residency: Some(owner.resident.residency),
                    leave_peer: self.serving.save.as_mut().and_then(|save| save.port()),
                    ..Default::default()
                },
                gate: (!retained_gate_end).then_some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io,
            },
            wait,
        )?;
        Ok(())
    }

    fn retain_report(&mut self) -> Result<(), RunFailure> {
        if self.report.is_some() {
            return Ok(());
        }
        let owner = &mut self.serving.owner;
        for (port, subsystem, source, opener) in [
            (
                &mut owner.gate,
                Subsystem::Gate,
                PumpSource::Gate,
                Opener::Gate,
            ),
            (
                &mut *owner.resident.residency,
                Subsystem::Spu,
                PumpSource::Residency,
                Opener::Spu,
            ),
        ] {
            // A Fault and EOF can arrive together, including an unread final
            // packet when another seam first reports the same organ's loss.
            // Read once into the original port's sole slot; never reopen it.
            if port.held().is_none() {
                let received = if port.failure().is_none() {
                    port.try_receive()
                } else if port.closed_receive_pending() {
                    port.receive_closed()
                } else {
                    Ok(crate::flow::pump_transport::ReceiveProgress::Pending)
                };
                received.map_err(|failure| RunFailure::Transport { source, failure })?;
            }
            if port.held().is_some_and(|frame| {
                frame.exchange.opener == opener && matches!(frame.payload, Payload::Fault(_))
            }) {
                let received = port
                    .take_received()
                    .map_err(|failure| RunFailure::Transport { source, failure })?
                    .expect("held original organ report");
                self.report = Some(report::Report::new(owner.context, received, subsystem));
                break;
            }
        }
        Ok(())
    }

    fn step(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if self.progress() == FoundationInputProgress::WindDown {
            return self.advance_wind_down(io);
        }
        if self.quiesce_progress() == Some(FoundationInputProgress::Pending) {
            return self.advance_quiesce(io);
        }
        if self.stage == Stage::Engine {
            return self.advance_engine(None, io);
        }
        let owner = &mut self.serving.owner;
        owner.context.live(io)?;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                residency: Some(owner.resident.residency),
                leave_peer: self.serving.save.as_mut().and_then(|save| save.port()),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        match self.stage {
            Stage::Waiting => {
                owner.context.reduce_flow(ports.io)?;
                // The first report retains its exact received packet while an
                // earlier author owns the slot. A second Fault stays on its port.
                if self.report.is_some() {
                    return Ok(());
                }
                // Residency announcements remain on their actual seam. They
                // are opaque trace facts, not invented management decisions.
                if ports
                    .lifecycle
                    .residency
                    .as_deref()
                    .unwrap()
                    .held()
                    .is_some_and(|frame| {
                        frame.exchange.opener != Opener::Spu
                            || !matches!(frame.payload, Payload::Fault(_))
                    })
                {
                    return Err(owner.context.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "unexpected serving residency reply",
                    ));
                }
                if self.received.is_none() {
                    if owner
                        .context
                        .serving_organs
                        .gate_matches(ports.gate.as_deref().unwrap())
                    {
                        owner.context.pump_once(&mut ports)?;
                        return Ok(());
                    }
                    self.received = self.serving.queued_inputs.pop_front();
                    if self.received.is_none() {
                        self.received =
                            ports.gate.as_deref_mut().unwrap().take_received().map_err(
                                |failure| RunFailure::Transport {
                                    source: PumpSource::Gate,
                                    failure,
                                },
                            )?;
                    }
                }
                let Some(received) = self.received.as_ref() else {
                    owner.context.pump_once(&mut ports)?;
                    return Ok(());
                };
                let envelope = received.envelope();
                if !received.belongs_to(ports.gate.as_deref().unwrap())
                    || envelope.exchange.opener != Opener::Gate
                {
                    return Err(owner.context.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "input did not open on the standing gate",
                    ));
                }
                if let Payload::Fault(_) = &envelope.payload {
                    self.report = Some(report::Report::new(
                        owner.context,
                        self.received.take().unwrap(),
                        Subsystem::Gate,
                    ));
                    return Ok(());
                }
                let Payload::Frame(frame) = &envelope.payload else {
                    return Err(owner.context.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "unexpected gate payload at input intake",
                    ));
                };
                if envelope.position != Position::Open
                    || self
                        .serving
                        .gate_exchanges
                        .contains(&envelope.exchange.ordinal)
                {
                    return Err(owner.context.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "gate frame is not a fresh opening exchange",
                    ));
                }
                match owner.context.begin_input(frame.clone(), ports.io) {
                    Ok(()) => {
                        self.serving
                            .gate_exchanges
                            .insert(envelope.exchange.ordinal);
                        self.stage = Stage::Checking;
                    }
                    Err(error) => {
                        // begin_input returns the unchanged frame. Its received
                        // envelope is still retained here, including raw bytes.
                        debug_assert_eq!(&error.frame, frame);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        owner.context.pump_once(&mut ports)?;
                    }
                }
            }
            Stage::Checking => match owner.context.advance_input(&mut ports)? {
                InputProgress::Accepted => {
                    self.token = owner.context.take_accepted_input(ports.io)?;
                    if self.token.is_some() {
                        self.stage = Stage::Accepted;
                    }
                }
                InputProgress::CloseReady => {
                    self.close = Some(OrganEnvelope {
                        exchange: self.received.as_ref().unwrap().envelope().exchange.clone(),
                        position: Position::Close,
                        payload: Payload::Frame(owner.context.input_close().expect("owed close")),
                    });
                    self.stage = Stage::Closing;
                }
                InputProgress::Pending => {
                    owner.context.pump_once(&mut ports)?;
                }
                InputProgress::Progress => {}
                _ => {
                    return Err(owner.context.fail(
                        ports.io,
                        FlowHaltReason::ProtocolFailed,
                        HaltPoint::Barrier,
                        "input lost its active check",
                    ));
                }
            },
            Stage::Source => {
                let result = if let Some(token) = self.token.take() {
                    let turn = TurnRef(self.turn.as_ref().expect("loop supplied turn").0.clone());
                    match owner.context.consume_input(token, turn, &mut ports) {
                        Ok(result) => result,
                        Err(error) => {
                            self.token = error.token;
                            return Err(error.failure);
                        }
                    }
                } else {
                    owner.context.resume_input_turn(&mut ports)?
                };
                if let Some(seated) = result {
                    self.seated = Some(seated);
                    self.stage = Stage::Turn;
                } else {
                    owner.context.pump_once(&mut ports)?;
                }
            }
            Stage::Closing => {
                let gate = ports.gate.as_deref_mut().unwrap();
                if owner.context.serving_organs.gate_delivery_lost(gate) {
                    if close::retain_lost_world(
                        gate,
                        &self.received.as_ref().unwrap().envelope().exchange,
                        &mut self.close,
                        self.send.as_ref(),
                    )? {
                        owner.context.input_closed_by_gate_loss(gate, ports.io)?;
                        owner.context.finish_input(ports.io)?;
                        self.stage = Stage::Refused;
                    }
                    return Ok(());
                }
                if let Some(close) = self.close.take() {
                    match ports.gate.as_deref_mut().unwrap().stage_tracked(close) {
                        Ok(TrackedStage::Queued(send)) => self.send = Some(send),
                        Ok(TrackedStage::Busy(close)) => self.close = Some(close),
                        Err(error) => {
                            self.close = Some(error.input);
                            return Err(RunFailure::Transport {
                                source: PumpSource::Gate,
                                failure: error.failure,
                            });
                        }
                    }
                }
                if self.send.as_ref().is_some_and(|send| {
                    send.sent()
                        && send.sent_at().is_some()
                        && send.belongs_to(ports.gate.as_deref().unwrap())
                        && send
                            .matches_exchange(&self.received.as_ref().unwrap().envelope().exchange)
                }) {
                    owner.context.input_response_sent(ports.io)?;
                    owner.context.finish_input(ports.io)?;
                    self.stage = Stage::Refused;
                } else {
                    owner.context.pump_once(&mut ports)?;
                }
            }
            Stage::Accepted | Stage::Turn | Stage::Refused | Stage::Aborted | Stage::Completed
                if self.report.is_some() =>
            {
                // Only the retained report is pending. The stage has no input
                // work to reduce, and cannot finish until its report records.
            }
            _ => unreachable!("only pending stages advance"),
        }
        Ok(())
    }

    /// Only settled world-close delivery (including checked gate loss), Recorded
    /// checked source authoring or a confirmed/check-recorded generation grants
    /// a result. The generated arm
    /// retains custody for repair/sort; it grants no final answer or dispatch.
    /// Every other attempt returns the full original owner.
    pub fn finish(mut self: Box<Self>) -> Result<FoundationInputResult<'a>, Box<Self>> {
        match self.progress() {
            FoundationInputProgress::Refused => Ok(FoundationInputResult::Refused(self.serving)),
            FoundationInputProgress::Aborted => Ok(FoundationInputResult::Aborted(self.serving)),
            FoundationInputProgress::Completed => {
                Ok(FoundationInputResult::Completed(self.serving))
            }
            FoundationInputProgress::Turn => {
                Ok(FoundationInputResult::Turn(Box::new(FoundationTurn {
                    serving: self.serving,
                    received: self.received.take().expect("original gate exchange"),
                    input: self.seated.take().expect("recorded source"),
                })))
            }
            FoundationInputProgress::Generated => Ok(FoundationInputResult::Generated(
                generation::FoundationGeneration { input: self },
            )),
            FoundationInputProgress::Sorted => Ok(FoundationInputResult::Sorted(
                generation::emission::FoundationOutbound { input: self },
            )),
            _ => Err(self),
        }
    }
}

pub enum FoundationInputResult<'a> {
    Refused(FoundationServing<'a>),
    Aborted(FoundationServing<'a>),
    Completed(FoundationServing<'a>),
    Turn(Box<FoundationTurn<'a>>),
    Generated(generation::FoundationGeneration<'a>),
    Sorted(generation::emission::FoundationOutbound<'a>),
}

/// Actual checked source and the still-open gate exchange, beside the original
/// serving resources. There is no constructor, Clone or raw author access.
///
/// ```compile_fail,E0616
/// fn raw(turn: weaver_harness::FoundationTurn<'_>) { let _ = turn.serving; }
/// ```
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::FoundationTurn<'static>>();
/// ```
pub struct FoundationTurn<'a> {
    serving: FoundationServing<'a>,
    received: ReceivedOrgan,
    input: SeatedInput,
}
impl<'a> FoundationTurn<'a> {
    /// Return this still-owned checked source to the common service pump. Its
    /// original gate request and source remain intact; no input token, event or
    /// turn identity is minted again. The loop can hear management before it
    /// transfers this turn to a subsequent operation owner.
    pub fn into_service(self: Box<Self>) -> Box<FoundationInput<'a>> {
        Box::new(FoundationInput {
            turn: Some(TurnKey(self.input.turn.0.clone())),
            serving: self.serving,
            stage: Stage::Turn,
            received: Some(self.received),
            token: None,
            seated: Some(self.input),
            close: None,
            send: None,
            report: None,
            failure: None,
            engine: None,
        })
    }
    pub fn message(&self) -> &weaver_traits::Message {
        &self.input.message
    }
    pub fn source(&self) -> &weaver_types::state_flow::SourceBlockId {
        &self.input.source
    }
    pub fn turn(&self) -> TurnKey {
        TurnKey(self.input.turn.0.clone())
    }
}

#[cfg(test)]
#[path = "context_worker_input_tests.rs"]
pub(super) mod tests;
