//! One retained turn close drives authoring and exact world delivery for answers,
//! settled operation refusals and operator Stop. The caller chooses the outcome.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::RecordingContext;
use crate::flow::pump::PumpDeadline;
use weaver_trace::{Sequence, StopReason, TurnClose};
use weaver_types::{Cause, ExchangeId, Finish, TurnFrame};

pub(super) struct WorldClose {
    turn: TurnRef,
    exchange: ExchangeId,
    outcome: TurnClose,
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
        Some(Self {
            turn,
            exchange: request.envelope().exchange.clone(),
            outcome,
            text,
            length,
            completed,
            bound: input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            author_started: false,
            returned_author: StoppedAuthor::default(),
            recorded: None,
            close: None,
            send: None,
        })
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
        self.author_started || self.recorded.is_some()
    }
    pub(super) fn recorded(&self) -> Option<Sequence> {
        self.recorded
    }
    pub(super) fn turn(&self) -> &TurnRef {
        &self.turn
    }
    pub(super) fn send(&self) -> Option<&OrganSend> {
        self.send.as_ref()
    }
    pub(super) fn completed(&self) -> Stage {
        self.completed
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
        let newly_recorded = self.recorded.is_none();
        if newly_recorded {
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
                            turn: Some(self.turn.clone()),
                            source: AuthoredSource::Runtime,
                            causal_parent: None,
                            payload: Some(serving::Payload::TurnClosed(self.outcome.clone())),
                        },
                    )
                    .map_err(|failure| {
                        self.returned_author
                            .retain_started(failure, &mut self.author_started)
                    })?
            };
            self.recorded = Some(sequence);
            let answered = self.completed == Stage::Completed;
            let frame = crate::lifecycle::render_close_with_finish(
                if answered { "answered" } else { "stopped" },
                if answered { "text" } else { "reason" },
                &self.text,
                Some((
                    &TurnKey(self.turn.0.clone()),
                    &weaver_types::RunId(context.flow.run().to_owned()),
                )),
                self.length,
            );
            self.close = Some(OrganEnvelope {
                exchange: self.exchange.clone(),
                position: Position::Close,
                payload: Payload::Frame(TurnFrame::carry(frame.as_bytes())),
            });
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
                && send.matches_exchange(&self.exchange)
        }) {
            context.complete_wait(PumpDeadline::Decision);
            return Ok(true);
        }
        if newly_recorded {
            // Let management queue the Stop answer after Recorded, before any
            // blocking wait on the world gate. The retained send stays owed.
            return Ok(false);
        }
        context.pump_once(ports)?;
        Ok(false)
    }
}
