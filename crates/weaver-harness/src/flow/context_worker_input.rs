//! Actual gate intake joined to the entered worker's one recording context.
//! The loop chooses a turn only after checked acceptance. No caller can mint a
//! gate source, a send receipt, an accepted token or a selected content copy.
use super::super::{InputProgress, RunFailure, SeatedInput, StoppedAuthor};
use super::FoundationServing;
use crate::flow::author::{AuthoredEvent, AuthoredSource};
use crate::flow::clock::{MonotonicIo, PumpIo};
use crate::flow::input::AcceptedInput;
use crate::flow::pump::{LifecyclePorts, PumpPorts, PumpSource};
use crate::flow::pump_transport::{OrganSend, ReceivedOrgan, TrackedStage};
use crate::flow::report::{FlowHaltReason, HaltPoint};
use weaver_trace::{Subsystem, TurnRef, serving};
use weaver_types::{Opener, OrganEnvelope, Payload, Position, TurnKey};

#[path = "context_worker_generation.rs"]
pub(crate) mod generation;
#[path = "context_worker_management.rs"]
pub(crate) mod management;

#[path = "context_worker_close.rs"]
mod close;

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
    /// The clean turn close is Recorded and its exact world reply fully sent.
    Completed,
    /// An operation refused. Its full account remains with this owner.
    OperationRefused,
    Refused,
    /// The stopped turn's world close has been completely carried.
    Aborted,
    /// The actual root connection is retained for management judgment. Mere
    /// acceptance is not a Leave and does not revoke input or grant cleanup.
    ManagementPending,
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
    report: Option<(ReceivedOrgan, Subsystem)>,
    report_started: bool,
    returned_report: StoppedAuthor,
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
            report_started: false,
            returned_report: StoppedAuthor::default(),
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
        let result = self.observe(io).and_then(|()| {
            if self.progress() == FoundationInputProgress::ManagementPending {
                Ok(())
            } else if wait {
                self.serving
                    .owner
                    .context
                    .reduce_flow(io)
                    .and_then(|_| self.observe_with_wait(io, true))
            } else if self.progress() == FoundationInputProgress::Pending {
                self.step(io)
            } else {
                Ok(())
            }
        });
        if let Err(mut failure) = result {
            if self.final_send_clock_failed() {
                failure = self.serving.owner.context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    format!("final reply send clock observation failed: {failure:?}"),
                );
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
        let owner = &mut self.serving.owner;
        owner.context.pump_with_wait(
            &mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: Some(&mut owner.listener),
                    residency: Some(owner.resident.residency),
                    ..Default::default()
                },
                gate: Some(&mut owner.gate),
                decode: Some(owner.decode),
                label: None,
                io,
            },
            wait,
        )?;
        Ok(())
    }

    fn step(&mut self, io: &mut impl PumpIo) -> Result<(), RunFailure> {
        if self.stage == Stage::Engine {
            return self.advance_engine(None, io);
        }
        let owner = &mut self.serving.owner;
        owner.context.live(io)?;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        if self.report.is_some() {
            self.returned_report.check()?;
            if self.report_started {
                owner
                    .context
                    .resume_author_and_wait(&mut ports)
                    .map_err(|failure| {
                        self.returned_report
                            .retain_started(failure, &mut self.report_started)
                    })?;
            } else {
                let (received, subsystem) = self.report.as_ref().unwrap();
                let Payload::Fault(report) = &received.envelope().payload else {
                    unreachable!()
                };
                let payload = serde_json::value::to_raw_value(report)
                    .expect("a received fault is serializable");
                self.report_started = true;
                owner
                    .context
                    .author_and_wait(
                        &mut ports,
                        AuthoredEvent {
                            kind: serving::Kind::Fault,
                            subsystem: *subsystem,
                            turn: None,
                            source: AuthoredSource::Runtime,
                            causal_parent: None,
                            payload: Some(serving::Payload::Fault(payload)),
                        },
                    )
                    .map_err(|failure| {
                        self.returned_report
                            .retain_started(failure, &mut self.report_started)
                    })?;
            }
            self.report = None;
            self.report_started = false;
            return Ok(());
        }
        match self.stage {
            Stage::Waiting => {
                owner.context.reduce_flow(ports.io)?;
                // Residency announcements remain on their actual seam. They
                // are opaque trace facts, not invented management decisions.
                if let Some(received) = ports
                    .lifecycle
                    .residency
                    .as_deref_mut()
                    .unwrap()
                    .take_received()
                    .map_err(|failure| RunFailure::Transport {
                        source: PumpSource::Residency,
                        failure,
                    })?
                {
                    self.report = Some((received, Subsystem::Spu));
                    let (received, _) = self.report.as_ref().unwrap();
                    if received.envelope().exchange.opener != Opener::Spu
                        || !matches!(received.envelope().payload, Payload::Fault(_))
                    {
                        return Err(owner.context.fail(
                            ports.io,
                            FlowHaltReason::ProtocolFailed,
                            HaltPoint::Barrier,
                            "unexpected serving residency reply",
                        ));
                    }
                    return Ok(());
                }
                if self.received.is_none() {
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
                    self.report = Some((self.received.take().unwrap(), Subsystem::Gate));
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
                if self.send.as_ref().is_some_and(|send| send.sent()) {
                    owner.context.input_response_sent(ports.io)?;
                    owner.context.finish_input(ports.io)?;
                    self.stage = Stage::Refused;
                } else {
                    owner.context.pump_once(&mut ports)?;
                }
            }
            _ => unreachable!("only pending stages advance"),
        }
        Ok(())
    }

    /// Only whole world-close delivery, Recorded checked source authoring or a
    /// confirmed/check-recorded generation grants a result. The generated arm
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
            report_started: false,
            returned_report: StoppedAuthor::default(),
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
