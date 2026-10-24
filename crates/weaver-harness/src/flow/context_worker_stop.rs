//! Stop retains the original source/task/generation owner through settlement.
use super::*;
use weaver_trace::{Sequence, StopReason, TurnClose};
use weaver_types::{Cause, LifecycleAnswer, TurnFrame};

pub(super) struct StopWork {
    turn: TurnRef,
    cause: Cause,
    author_started: bool,
    returned_author: StoppedAuthor,
    pub(super) close_sequence: Option<Sequence>,
    gate_close: Option<OrganEnvelope>,
    pub(super) gate_send: Option<OrganSend>,
    pub(super) gate_done: bool,
}

#[cfg(test)]
#[path = "context_worker_stop_tests.rs"]
pub(in crate::flow::context::worker_startup) mod tests;
impl StopWork {
    pub(super) fn begin(
        input: &mut FoundationInput<'_>,
        cause: Cause,
        io: &mut impl PumpIo,
    ) -> Result<Self, RunFailure> {
        let valid = ((input.stage == Stage::Turn
            && input.seated.as_ref().is_some_and(|source| {
                input.serving.owner.context.open_turn.as_ref() == Some(&source.turn)
            }))
            || (input.stage == Stage::Engine
                && input.turn.as_ref().is_some_and(|turn| {
                    input
                        .serving
                        .owner
                        .context
                        .open_turn
                        .as_ref()
                        .is_some_and(|open| open.0 == turn.0)
                })))
            && input
                .received
                .as_ref()
                .is_some_and(|request| request.belongs_to(&input.serving.owner.gate));
        if !valid {
            return Err(input.serving.owner.context.fail(
                io,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "Stop requires the actual turn owner and original gate request",
            ));
        }
        Ok(Self {
            turn: input
                .serving
                .owner
                .context
                .open_turn
                .as_ref()
                .unwrap()
                .clone(),
            cause,
            author_started: false,
            returned_author: StoppedAuthor::default(),
            close_sequence: None,
            gate_close: None,
            gate_send: None,
            gate_done: false,
        })
    }
    pub(super) fn answer(&self) -> Option<LifecycleAnswer> {
        self.close_sequence.map(|_| LifecycleAnswer::TurnAborted {
            turn: TurnKey(self.turn.0.clone()),
        })
    }
    pub(super) fn advance(
        &mut self,
        input: &mut FoundationInput<'_>,
        peer: Option<&mut LifecyclePeer>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        input.cancel_engine(io)?;
        if !input.engine_settled() {
            input.advance_engine(peer, io)?;
            return Ok(());
        }
        let owner = &mut input.serving.owner;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                dial: peer,
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io,
        };
        if self.close_sequence.is_none() {
            self.returned_author.check()?;
            let sequence = if self.author_started {
                owner
                    .context
                    .resume_author_and_wait(&mut ports)
                    .map_err(|failure| {
                        self.returned_author
                            .retain_started(failure, &mut self.author_started)
                    })?
            } else {
                self.author_started = true;
                owner
                    .context
                    .author_and_wait(
                        &mut ports,
                        AuthoredEvent {
                            kind: serving::Kind::TurnClosed,
                            subsystem: Subsystem::Harness,
                            turn: Some(self.turn.clone()),
                            source: AuthoredSource::Runtime,
                            causal_parent: None,
                            payload: Some(serving::Payload::TurnClosed(TurnClose::Stopped {
                                reason: StopReason::Directive,
                                cause: Some(weaver_trace::Cause {
                                    uid: self.cause.uid,
                                }),
                            })),
                        },
                    )
                    .map_err(|failure| {
                        self.returned_author
                            .retain_started(failure, &mut self.author_started)
                    })?
            };
            self.close_sequence = Some(sequence);
            let close = crate::lifecycle::render_close(
                "stopped",
                "reason",
                "the operator stopped the turn",
                Some((
                    &TurnKey(self.turn.0.clone()),
                    &weaver_types::RunId(owner.context.flow.run().to_owned()),
                )),
            );
            self.gate_close = Some(OrganEnvelope {
                exchange: input.received.as_ref().unwrap().envelope().exchange.clone(),
                position: Position::Close,
                payload: Payload::Frame(TurnFrame::carry(close.as_bytes())),
            });
        }
        if let Some(close) = self.gate_close.take() {
            match ports.gate.as_deref_mut().unwrap().stage_tracked(close) {
                Ok(TrackedStage::Queued(send)) => self.gate_send = Some(send),
                Ok(TrackedStage::Busy(close)) => self.gate_close = Some(close),
                Err(error) => {
                    self.gate_close = Some(error.input);
                    return Err(RunFailure::Transport {
                        source: PumpSource::Gate,
                        failure: error.failure,
                    });
                }
            }
        }
        if self.gate_send.as_ref().is_some_and(|send| {
            send.sent()
                && send.belongs_to(ports.gate.as_deref().unwrap())
                && send.matches_exchange(&input.received.as_ref().unwrap().envelope().exchange)
        }) {
            self.gate_done = true;
            input.stage = Stage::Aborted;
        }
        Ok(())
    }
}
