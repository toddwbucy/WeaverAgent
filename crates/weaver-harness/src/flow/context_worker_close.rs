//! One retained close drives exact world delivery for answers, operation refusals
//! and operator Stop. Only an actual open turn authors a turn close.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::RecordingContext;
use crate::flow::pump::PumpDeadline;
use weaver_trace::{Sequence, StopReason, TurnClose};
use weaver_types::{Cause, ExchangeId, Finish, TurnFrame};

pub(super) struct WorldClose {
    target: CloseTarget,
    exchange: ExchangeId,
    text: String,
    length: bool,
    completed: Stage,
    bound: Option<AbsoluteBound>,
    author_started: bool,
    returned_author: StoppedAuthor,
    recorded: Option<Sequence>,
    close: Option<OrganEnvelope>,
    send: Option<OrganSend>,
}

enum CloseTarget {
    Turn { turn: TurnRef, outcome: TurnClose },
    Input(AcceptedInput),
}

impl WorldClose {
    fn begin(
        input: &FoundationInput<'_>,
        outcome: TurnClose,
        text: String,
        length: bool,
        completed: Stage,
    ) -> Option<Self> {
        let turn = TurnRef(input.turn.as_ref()?.0.clone());
        let request = input.received.as_ref()?;
        if input.serving.owner.context.open_turn.as_ref() != Some(&turn)
            || !request.belongs_to(&input.serving.owner.gate)
        {
            return None;
        }
        Some(Self::new(
            CloseTarget::Turn { turn, outcome },
            request.envelope().exchange.clone(),
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            text,
            length,
            completed,
        ))
    }
    fn new(
        target: CloseTarget,
        exchange: ExchangeId,
        bound: Option<AbsoluteBound>,
        text: String,
        length: bool,
        completed: Stage,
    ) -> Self {
        Self {
            target,
            exchange,
            text,
            length,
            completed,
            bound,
            author_started: false,
            returned_author: StoppedAuthor::default(),
            recorded: None,
            close: None,
            send: None,
        }
    }
    pub(super) fn answered(
        input: &FoundationInput<'_>,
        text: String,
        finish: Finish,
    ) -> Option<Self> {
        Self::begin(
            input,
            TurnClose::Clean,
            text,
            finish == Finish::Length,
            Stage::Completed,
        )
    }
    pub(super) fn refused(input: &FoundationInput<'_>) -> Option<Self> {
        Self::begin(
            input,
            TurnClose::Stopped {
                reason: StopReason::Refused,
                cause: None,
            },
            "the operation was refused".into(),
            false,
            Stage::Refused,
        )
    }
    pub(super) fn refused_admission(input: &mut FoundationInput<'_>) -> Option<Self> {
        let context = &input.serving.owner.context;
        let request = input.received.as_ref()?;
        if context.open_turn.is_some() || !request.belongs_to(&input.serving.owner.gate) {
            return None;
        }
        context
            .input
            .validate_accepted(
                input.token.as_ref()?,
                context.flow.run(),
                &context.declaration.scheme.scheme_id,
            )
            .ok()?;
        Some(Self::new(
            CloseTarget::Input(input.token.take()?),
            request.envelope().exchange.clone(),
            context.deadlines.get(PumpDeadline::Decision),
            "the operation was refused".into(),
            false,
            Stage::Refused,
        ))
    }
    pub(super) fn stopped(
        input: &mut FoundationInput<'_>,
        cause: Cause,
        io: &mut impl PumpIo,
    ) -> Result<Self, RunFailure> {
        // Capture an existing decision cap before cancellation retires it. Do
        // not install a decision wait while a read or exchange owns another
        // barrier: that operation must settle under its original wait first.
        Self::begin(
            input,
            TurnClose::Stopped {
                reason: StopReason::Directive,
                cause: Some(weaver_trace::Cause { uid: cause.uid }),
            },
            "the operator stopped the turn".into(),
            false,
            Stage::Aborted,
        )
        .ok_or_else(|| {
            input.serving.owner.context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "Stop requires the actual turn owner and original gate request",
            )
        })
    }
    pub(super) fn started(&self) -> bool {
        self.author_started
            || self.recorded.is_some()
            || self.close.is_some()
            || self.send.is_some()
    }
    pub(super) fn recorded(&self) -> Option<Sequence> {
        self.recorded
    }
    pub(super) fn turn(&self) -> Option<&TurnRef> {
        match &self.target {
            CloseTarget::Turn { turn, .. } => Some(turn),
            CloseTarget::Input(_) => None,
        }
    }
    pub(super) fn send(&self) -> Option<&OrganSend> {
        self.send.as_ref()
    }
    pub(super) fn completed(&self) -> Stage {
        self.completed
    }
    pub(super) fn rollback(turn: TurnRef, exchange: ExchangeId, cause: Cause) -> Self {
        Self::new(
            CloseTarget::Turn {
                turn,
                outcome: TurnClose::Stopped {
                    reason: StopReason::Unload,
                    cause: Some(weaver_trace::Cause { uid: cause.uid }),
                },
            },
            exchange,
            None,
            "The agent is leaving.".into(),
            false,
            Stage::Aborted,
        )
    }

    /// Lower owns world disposal. Keep an existing whole send or the exact
    /// withdrawn datagram, but never send another world frame after Lower.
    pub(super) fn prepare_rollback_transport(
        &mut self,
        gate: &mut crate::flow::pump_transport::OrganPort,
    ) -> Result<bool, RunFailure> {
        let Some(send) = &self.send else {
            return Ok(true);
        };
        if !send.belongs_to(gate) || !send.matches_exchange(&self.exchange) {
            return Err(RunFailure::PlaceOwned);
        }
        if send.sent() {
            return if send.sent_at().is_some() {
                Ok(true)
            } else {
                Err(RunFailure::Closed)
            };
        }
        if send.withdrawn() {
            return Ok(true);
        }
        let returned =
            gate.withdraw_pending(&self.exchange)
                .map_err(|failure| RunFailure::Transport {
                    source: PumpSource::Gate,
                    failure,
                })?;
        if let Some(returned) = returned {
            self.close = Some(returned);
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn advance_after_lower<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        proof: &crate::flow::context::worker_startup::expired::gate::RollbackGateClosed,
    ) -> Result<bool, RunFailure> {
        if !proof.allows(context) {
            return Err(RunFailure::PlaceOwned);
        }
        self.prepare_rollback_record(context, ports)?;
        if matches!(self.target, CloseTarget::Input(_)) && context.input.snapshot().is_some() {
            context.input_closed_by_lower(proof, ports.io)?;
            context.finish_input(ports.io)?;
        }
        context.complete_wait(PumpDeadline::Decision);
        Ok(true)
    }

    /// An already-started close keeps its one original record even while Lower
    /// is still reading queued frames. This continuation carries no world data.
    pub(super) fn prepare_rollback_record<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        // A close already begun before Leave keeps its existing absolute wait.
        if self.recorded.is_none()
            && context.deadlines.get(PumpDeadline::Decision).is_none()
            && let Some(bound) = self.bound
        {
            context.arm(PumpDeadline::Decision, bound, ports.io)?;
        }
        self.prepare(context, ports, true)?;
        Ok(())
    }

    fn prepare<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        lowered: bool,
    ) -> Result<bool, RunFailure> {
        let preparing = self.close.is_none() && self.send.is_none();
        if preparing {
            match &self.target {
                CloseTarget::Turn { turn, outcome } => {
                    self.returned_author.check()?;
                    let sequence = if self.author_started {
                        context.resume_author_and_wait(ports).map_err(|failure| {
                            self.returned_author
                                .retain_started(failure, &mut self.author_started)
                        })?
                    } else {
                        self.author_started = true;
                        context
                            .author_and_wait(
                                ports,
                                AuthoredEvent {
                                    kind: serving::Kind::TurnClosed,
                                    subsystem: Subsystem::Harness,
                                    turn: Some(turn.clone()),
                                    source: AuthoredSource::Runtime,
                                    causal_parent: None,
                                    payload: Some(serving::Payload::TurnClosed(outcome.clone())),
                                },
                            )
                            .map_err(|failure| {
                                self.returned_author
                                    .retain_started(failure, &mut self.author_started)
                            })?
                    };
                    self.recorded = Some(sequence);
                }
                CloseTarget::Input(token) => {
                    // The rejected admission never consumed this input or opened a
                    // turn. Revoke the exact accepted capability, then retain its
                    // owner until the original gate exchange has fully closed.
                    if !lowered {
                        context
                            .input
                            .validate_accepted(
                                token,
                                context.flow.run(),
                                &context.declaration.scheme.scheme_id,
                            )
                            .map_err(|error| {
                                context.fail(
                                    ports.io,
                                    FlowHaltReason::ProtocolFailed,
                                    HaltPoint::Barrier,
                                    format!("refused admission input: {error}"),
                                )
                            })?;
                    }
                    if !context
                        .input
                        .snapshot()
                        .is_some_and(|input| input.cancelled)
                    {
                        context.cancel_input(ports.io)?;
                    }
                }
            }
            let answered = self.completed == Stage::Completed;
            let run = weaver_types::RunId(context.flow.run().to_owned());
            let turn = self.turn().map(|turn| TurnKey(turn.0.clone()));
            let frame = crate::lifecycle::render_close_with_finish(
                if answered { "answered" } else { "stopped" },
                if answered { "text" } else { "reason" },
                &self.text,
                turn.as_ref().map(|turn| (turn, &run)),
                self.length,
            );
            self.close = Some(OrganEnvelope {
                exchange: self.exchange.clone(),
                position: Position::Close,
                payload: Payload::Frame(TurnFrame::carry(frame.as_bytes())),
            });
        }
        Ok(preparing)
    }

    pub(super) fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        if context.deadlines.get(PumpDeadline::Decision).is_none() {
            if let Some(bound) = self.bound {
                context.arm(PumpDeadline::Decision, bound, ports.io)?;
            } else {
                generation::decision(context, ports.io)?;
                self.bound = context.deadlines.get(PumpDeadline::Decision);
            }
        }
        let preparing = self.prepare(context, ports, false)?;
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
                && send.matches_exchange(&self.exchange)
        }) {
            if matches!(self.target, CloseTarget::Input(_)) {
                context.input_response_sent(ports.io)?;
                context.finish_input(ports.io)?;
            }
            context.complete_wait(PumpDeadline::Decision);
            return Ok(true);
        }
        if preparing {
            // Let management queue its answer after close preparation, before
            // any blocking gate wait. A turn close has reached Recorded here.
            return Ok(false);
        }
        context.pump_once(ports)?;
        Ok(false)
    }
}
