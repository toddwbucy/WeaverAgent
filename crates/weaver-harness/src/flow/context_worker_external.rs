//! Worker election and custody of one gate execution. The gate keeps its
//! loaded kill clock; this owner only advances the existing pair ports.
use super::*;
use crate::flow::context::{ExecutionProgress, ExecutionRefusal};
use crate::flow::pump_transport::{OrganSend, TrackedStage};
use weaver_types::{ExchangeId, OrganEnvelope, Payload, Position};

enum Stage {
    Admission,
    Sending,
    Waiting,
    Rejection,
    Terminal,
}

pub(super) struct ExternalWork {
    call: CallId,
    exchange: ExchangeId,
    stage: Stage,
    cancel: Option<OrganEnvelope>,
    cancel_send: Option<OrganSend>,
}

impl ExternalWork {
    pub(super) fn new(call: CallId, exchange: ExchangeId) -> Self {
        Self {
            call,
            exchange,
            stage: Stage::Admission,
            cancel: None,
            cancel_send: None,
        }
    }

    /// Returning true permits the pair driver to file the actual terminal.
    /// Stop is idempotent, including a cancel that crosses a gate answer.
    pub(super) fn advance<I: PumpIo>(
        &mut self,
        cancelled: bool,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        match self.stage {
            Stage::Admission => {
                if cancelled {
                    return Ok(true);
                }
                match context.stage_pair_execution(
                    &self.call,
                    self.exchange.clone(),
                    ports.gate.as_deref_mut(),
                    ports.io,
                )? {
                    Ok(ExecutionProgress::Pending) => self.stage = Stage::Sending,
                    Ok(ExecutionProgress::Terminal) | Err(ExecutionRefusal::NotReady) => {
                        self.stage = Stage::Terminal;
                        return Ok(true);
                    }
                    Err(ExecutionRefusal::Busy) => {
                        context.pump_once(ports)?;
                    }
                    other => {
                        return Err(fault(
                            context,
                            ports.io,
                            format!("worker external admission: {other:?}"),
                        ));
                    }
                }
            }
            Stage::Sending => {
                // The Stop owner reconciled the actual ticket before withdrawing
                // an unsent packet. Its not_dispatched terminal still owes filing.
                if cancelled && context.pairs.executing().is_none() {
                    self.stage = Stage::Terminal;
                    return Ok(true);
                }
                match context.advance_pair_send(ports.gate.as_deref_mut().unwrap(), ports.io)? {
                    ExecutionProgress::Sent => self.stage = Stage::Waiting,
                    _ => {
                        context.pump_once(ports)?;
                    }
                }
            }
            Stage::Waiting => {
                if cancelled {
                    if self.cancel.is_none() && self.cancel_send.is_none() {
                        self.cancel = Some(OrganEnvelope {
                            exchange: self.exchange.clone(),
                            position: Position::Continue,
                            payload: Payload::ToolCancel,
                        });
                    }
                    if let Some(frame) = self.cancel.take() {
                        match ports.gate.as_deref_mut().unwrap().stage_tracked(frame) {
                            Ok(TrackedStage::Queued(send)) => self.cancel_send = Some(send),
                            Ok(TrackedStage::Busy(frame)) => self.cancel = Some(frame),
                            Err(error) => {
                                self.cancel = Some(error.input);
                                return Err(RunFailure::Transport {
                                    source: PumpSource::Gate,
                                    failure: error.failure,
                                });
                            }
                        }
                    }
                    if !self.cancel_send.as_ref().is_some_and(|send| {
                        send.sent()
                            && send.belongs_to(ports.gate.as_deref().unwrap())
                            && send.matches_exchange(&self.exchange)
                    }) {
                        context.pump_once(ports)?;
                        return Ok(false);
                    }
                }
                match context.observe_pair_reply(ports.gate.as_deref_mut().unwrap(), ports.io)? {
                    ExecutionProgress::Terminal => {
                        self.stage = Stage::Terminal;
                        return Ok(true);
                    }
                    ExecutionProgress::Rejected => self.stage = Stage::Rejection,
                    _ => {
                        context.pump_once(ports)?;
                    }
                }
            }
            Stage::Rejection => {
                context.record_pair_rejection(ports)?;
                self.stage = Stage::Waiting;
            }
            Stage::Terminal => return Ok(true),
        }
        Ok(false)
    }
}
