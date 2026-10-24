//! Final answer custody joins the checked selection, all pair filings, the
//! Recorded close and the exact original gate exchange. Selection is the loop's.
use super::*;
use weaver_trace::{Sequence, TurnClose};
use weaver_types::{ExchangeId, Finish, TurnFrame};

pub(super) struct AnswerWork {
    turn: TurnRef,
    exchange: ExchangeId,
    text: String,
    finish: Finish,
    author_started: bool,
    returned_author: StoppedAuthor,
    recorded: Option<Sequence>,
    close: Option<OrganEnvelope>,
    send: Option<OrganSend>,
}

impl FoundationInput<'_> {
    pub(in crate::flow::context::worker_startup::input) fn final_send_clock_failed(&self) -> bool {
        self.engine
            .as_ref()
            .and_then(|work| work.answer.as_ref())
            .and_then(|answer| answer.send.as_ref())
            .is_some_and(|send| send.sent() && send.sent_at().is_none())
    }

    #[cfg(test)]
    pub(super) fn final_send_probe_for_test(&self) -> Box<dyn Fn() -> bool> {
        self.engine
            .as_ref()
            .unwrap()
            .answer
            .as_ref()
            .unwrap()
            .send
            .as_ref()
            .unwrap()
            .sent_probe_for_test()
    }

    /// Close with the Answer blocks selected by the already recorded outbound
    /// sort. Every conforming request must first have a terminal filing receipt.
    /// No text argument or copied statement can substitute for those owners.
    pub fn close_answer(&mut self) -> Result<(), FoundationInputProgress> {
        if self.progress() != FoundationInputProgress::PairsComplete {
            return Err(self.progress());
        }
        let work = self.engine.as_mut().unwrap();
        let Some((text, finish)) = work.emission.as_ref().unwrap().selected_answer() else {
            return Err(self.progress());
        };
        let turn = TurnRef(self.turn.as_ref().unwrap().0.clone());
        if self.serving.owner.context.open_turn.as_ref() != Some(&turn)
            || self
                .received
                .as_ref()
                .is_none_or(|r| !r.belongs_to(&self.serving.owner.gate))
        {
            return Err(self.progress());
        }
        work.answer = Some(AnswerWork {
            turn,
            exchange: self.received.as_ref().unwrap().envelope().exchange.clone(),
            text,
            finish,
            author_started: false,
            returned_author: StoppedAuthor::default(),
            recorded: None,
            close: None,
            send: None,
        });
        work.stage = EngineStage::Answer;
        Ok(())
    }
}

impl AnswerWork {
    pub(super) fn started(&self) -> bool {
        self.author_started || self.recorded.is_some()
    }

    pub(super) fn advance<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        if self.recorded.is_none() {
            decision(context, ports.io)?;
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
                            payload: Some(serving::Payload::TurnClosed(TurnClose::Clean)),
                        },
                    )
                    .map_err(|failure| {
                        self.returned_author
                            .retain_started(failure, &mut self.author_started)
                    })?
            };
            self.recorded = Some(sequence);
            let close = crate::lifecycle::render_close_with_finish(
                "answered",
                "text",
                &self.text,
                Some((
                    &TurnKey(self.turn.0.clone()),
                    &weaver_types::RunId(context.flow.run().to_owned()),
                )),
                self.finish == Finish::Length,
            );
            self.close = Some(OrganEnvelope {
                exchange: self.exchange.clone(),
                position: Position::Close,
                payload: Payload::Frame(TurnFrame::carry(close.as_bytes())),
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
        context.pump_once(ports)?;
        Ok(false)
    }
}
