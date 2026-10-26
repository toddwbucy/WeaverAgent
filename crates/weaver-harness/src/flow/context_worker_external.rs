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
    control: Option<OrganEnvelope>,
    cancel: Option<OrganEnvelope>,
    cancel_send: Option<OrganSend>,
}

impl ExternalWork {
    pub(super) fn new(call: CallId, exchange: ExchangeId) -> Self {
        Self {
            call,
            exchange,
            stage: Stage::Admission,
            control: None,
            cancel: None,
            cancel_send: None,
        }
    }

    /// The envelope and observed ticket stay together even after the gate
    /// retires. This is evidence of the one control, never a copied receipt.
    pub(super) fn cancel_control(&self) -> Option<(&OrganEnvelope, &OrganSend)> {
        Some((self.control.as_ref()?, self.cancel_send.as_ref()?))
    }

    /// Route only this execution's answer (or the pair ledger's rejected
    /// ToolAnswer). A Lower reply and queued world traffic remain on the port.
    pub(super) fn route_cancel_reply(
        &mut self,
        context: &mut RecordingContext,
        gate: &mut crate::flow::pump_transport::OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !matches!(self.stage, Stage::Sending | Stage::Waiting) {
            return Ok(false);
        }
        match context.observe_pair_reply(gate, io)? {
            ExecutionProgress::Terminal => {
                // A result that crosses an unsent control retains its fate.
                // Withdraw only our exact still-queued packet before Lower.
                if self.cancel_send.as_ref().is_some_and(|send| !send.sent())
                    && let Some(frame) =
                        gate.withdraw_pending(&self.exchange).map_err(|failure| {
                            RunFailure::Transport {
                                source: PumpSource::Gate,
                                failure,
                            }
                        })?
                {
                    self.cancel = Some(frame);
                }
                self.stage = Stage::Terminal;
                Ok(true)
            }
            ExecutionProgress::Rejected => {
                self.stage = Stage::Rejection;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// A bounded nonblocking control step lets rollback send Lower without
    /// waiting for the call's terminal or its trace filing.
    pub(super) fn advance_cancel_control(
        &mut self,
        cause: NonDispatchCause,
        context: &mut RecordingContext,
        gate: &mut crate::flow::pump_transport::OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        context.live(io)?;
        if context.pairs.executing().is_none() {
            return Ok(true);
        }
        if matches!(self.stage, Stage::Sending) {
            if context.pairs.executing().is_some_and(|(_, _, sent)| sent) {
                self.stage = Stage::Waiting;
            } else {
                return Ok(false);
            }
        }
        if cause == NonDispatchCause::Unload && matches!(self.stage, Stage::Waiting) {
            self.route_cancel_reply(context, gate, io)?;
            if matches!(self.stage, Stage::Terminal) {
                return Ok(true);
            }
        }
        if self.control.is_none() {
            let frame = OrganEnvelope {
                exchange: self.exchange.clone(),
                position: Position::Continue,
                payload: match cause {
                    NonDispatchCause::Stop => Payload::ToolCancel,
                    NonDispatchCause::Unload => Payload::ToolInterrupt {
                        by: weaver_types::KillCause::Unload,
                    },
                    _ => unreachable!("only Stop or Unload cancels worker execution"),
                },
            };
            self.control = Some(frame.clone());
            self.cancel = Some(frame);
        }
        if let Some(frame) = self.cancel.take() {
            match gate.stage_tracked(frame) {
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
        if self
            .cancel_send
            .as_ref()
            .is_some_and(|send| !send.sent() && !send.withdrawn())
            && let Err(failure) = gate.try_send_observed(io)
        {
            if self
                .cancel_send
                .as_ref()
                .is_some_and(|send| send.sent() && send.sent_at().is_none())
            {
                return Err(context.fail(
                    io,
                    FlowHaltReason::TraceFailed,
                    HaltPoint::Author,
                    "worker execution control send clock failed",
                ));
            }
            return Err(RunFailure::Transport {
                source: PumpSource::Gate,
                failure,
            });
        }
        Ok(self.cancel_send.as_ref().is_some_and(|send| {
            send.sent()
                && send.sent_at().is_some()
                && send.belongs_to(gate)
                && send.matches_exchange(&self.exchange)
        }))
    }

    /// Returning true permits the pair driver to file the actual terminal.
    /// Stop is idempotent, including a cancel that crosses a gate answer.
    pub(super) fn advance<I: PumpIo>(
        &mut self,
        cancelled: Option<NonDispatchCause>,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        match self.stage {
            Stage::Admission => {
                if cancelled.is_some() {
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
                if cancelled.is_some() && context.pairs.executing().is_none() {
                    self.stage = Stage::Terminal;
                    return Ok(true);
                }
                if cancelled.is_some() && ports.gate.is_none() {
                    return Ok(false);
                }
                match context.advance_pair_send(
                    ports.gate.as_deref_mut().ok_or(RunFailure::PairOwned)?,
                    ports.io,
                )? {
                    ExecutionProgress::Sent => self.stage = Stage::Waiting,
                    _ => {
                        context.pump_once(ports)?;
                    }
                }
            }
            Stage::Waiting => {
                if let Some(cause) = cancelled {
                    if context.pairs.executing().is_none() {
                        self.stage = Stage::Terminal;
                        return Ok(true);
                    }
                    let Some(gate) = ports.gate.as_deref_mut() else {
                        // The gate owner may still hold the real ToolAnswer
                        // behind an older rejection's filing. It routes that
                        // receipt before electing a gate-fault terminal.
                        return Ok(false);
                    };
                    if !self.advance_cancel_control(cause, context, gate, ports.io)? {
                        context.pump_once(ports)?;
                        return Ok(false);
                    }
                    if matches!(self.stage, Stage::Terminal) {
                        return Ok(true);
                    }
                    if matches!(self.stage, Stage::Rejection) {
                        return Ok(false);
                    }
                }
                match context.observe_pair_reply(
                    ports.gate.as_deref_mut().ok_or(RunFailure::PairOwned)?,
                    ports.io,
                )? {
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
