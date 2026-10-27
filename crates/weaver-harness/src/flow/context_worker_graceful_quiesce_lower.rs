//! A force moves the original Quiesce accounts into Lower, without asserting
//! GateQuiesced or sending another Quiesce. The exact prior author resumes.
use super::*;
use crate::flow::context::worker_startup::expired::gate::RollbackGateClosed;

impl Quiesce {
    #[cfg(test)]
    pub(in crate::flow::context) fn test_record_send(&self) -> Option<&OrganSend> {
        self.pending.as_ref()?.send.as_ref()
    }

    #[cfg(test)]
    pub(in crate::flow::context) fn test_accounts(&self) -> (bool, Vec<Sequence>) {
        (
            self.complete,
            self.completed
                .iter()
                .filter_map(|record| record.recorded)
                .collect(),
        )
    }

    pub(in crate::flow::context::worker_startup) fn control(
        &self,
    ) -> Option<(&OrganEnvelope, &OrganSend)> {
        if self.complete || self.reply.is_some() {
            return None;
        }
        self.send.as_ref().map(|send| (&self.request, send))
    }

    pub(in crate::flow::context::worker_startup) fn has_record(&self) -> bool {
        self.pending.is_some()
    }

    /// Retain complete sends and withdraw only the actual unsent packet. A
    /// disposed port is evidence of lost delivery, never of a successful send.
    pub(in crate::flow::context::worker_startup) fn prepare_lower(
        &mut self,
        context: &mut RecordingContext,
        mut gate: Option<&mut OrganPort>,
        lowered: Option<&RollbackGateClosed>,
        input: Option<&ReceivedOrgan>,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if let Some(send) = &self.send {
            validate_send(
                context,
                gate.as_deref(),
                lowered,
                send,
                &self.request.exchange,
                io,
            )?;
            if !send.sent()
                && !send.withdrawn()
                && let Some(port) = gate.as_deref_mut()
            {
                match port
                    .withdraw_pending(&self.request.exchange)
                    .map_err(gate_failure)?
                {
                    Some(request) => self.request = request,
                    None => return Ok(false),
                }
            }
        }
        for record in self.pending.iter_mut().chain(&mut self.completed) {
            let Some(send) = &record.send else { continue };
            let original = record
                .received
                .as_ref()
                .or(input.filter(|_| record.active_input))
                .ok_or_else(|| protocol(context, io, "Quiesce send lost its original input"))?;
            let exchange = &original.envelope().exchange;
            validate_send(context, gate.as_deref(), lowered, send, exchange, io)?;
            if send.sent() && record.active_input {
                if context
                    .input
                    .snapshot()
                    .is_some_and(|input| !input.response_sent)
                {
                    context.input_response_sent(io)?;
                }
                self.input_responded = true;
            } else if !send.sent()
                && !send.withdrawn()
                && let Some(port) = gate.as_deref_mut()
            {
                match port.withdraw_pending(exchange).map_err(gate_failure)? {
                    Some(close) => record.close = Some(close),
                    None => return Ok(false),
                }
            }
        }
        Ok(true)
    }

    /// Only the retained refusal/fault may author here. Every other queued
    /// frame stays in the serving FIFO for the actual Lower owner to refuse.
    pub(in crate::flow::context::worker_startup) fn advance_after_force<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        lowered: Option<&RollbackGateClosed>,
        input: Option<&ReceivedOrgan>,
        seen: &mut std::collections::HashSet<u64>,
    ) -> Result<bool, RunFailure> {
        let Some(record) = self.pending.as_mut() else {
            return Ok(true);
        };
        record.stopped.check()?;
        let received = record
            .received
            .as_ref()
            .or(input.filter(|_| record.active_input))
            .ok_or_else(|| {
                protocol(context, ports.io, "Quiesce refusal lost its original input")
            })?;
        // A newly held gate frame is deliberately excluded from an ordinary
        // author's pump loan. It does not change the original port identity.
        let owned = self.port.matches_received(received)
            && context
                .rollback_gate_port
                .as_ref()
                .is_some_and(|port| port.same(&self.port))
            && lowered.is_none_or(|proof| proof.allows(context) && proof.owns(received));
        if !owned {
            return Err(protocol(
                context,
                ports.io,
                "Quiesce refusal has no original gate disposition",
            ));
        }
        record.prepare_event(context, ports.io, input, seen)?;
        if record.recorded.is_none() {
            if !record.started && context.caller_author_held() {
                context.pump_once(ports)?;
                return Ok(false);
            }
            let result = if record.started {
                context.resume_author_and_wait(ports)
            } else {
                record.started = true;
                context.author_and_wait(ports, record.event.take().unwrap())
            };
            match result {
                Ok(sequence) => record.recorded = Some(sequence),
                Err(error)
                    if matches!(
                        error.reason,
                        RunFailure::Busy(_)
                            | RunFailure::RollbackGateHeld
                            | RunFailure::Wait {
                                key: crate::flow::pump::PumpDeadline::Lower
                                    | crate::flow::pump::PumpDeadline::AdminRead,
                                ..
                            }
                            | RunFailure::Transport {
                                source: PumpSource::Gate
                                    | PumpSource::LeavePeer
                                    | PumpSource::AdminDial,
                                ..
                            }
                    ) =>
                {
                    record.started = error.original.is_none();
                    record.event = error.original;
                    return Err(error.reason);
                }
                Err(error) => {
                    return Err(record.stopped.retain_started(error, &mut record.started));
                }
            }
        }
        if lowered.is_none() {
            context.pump_once(ports)?;
            return Ok(false);
        }
        // Keep the withdrawn/unsent close and its original ticket in the phase
        // account. The shared Lower proof disposes it; no send hook is called.
        self.completed.push(self.pending.take().unwrap());
        Ok(true)
    }
}

fn validate_send(
    context: &mut RecordingContext,
    gate: Option<&OrganPort>,
    lowered: Option<&RollbackGateClosed>,
    send: &OrganSend,
    exchange: &ExchangeId,
    io: &mut impl PumpIo,
) -> Result<(), RunFailure> {
    if !send.matches_exchange(exchange)
        || !(gate.is_some_and(|port| send.belongs_to(port))
            || lowered.is_some_and(|proof| {
                proof.allows(context) && proof.owns_port(&send.port_identity())
            }))
    {
        return Err(protocol(
            context,
            io,
            "Quiesce send has no matching gate or exchange",
        ));
    }
    if send.sent() && send.sent_at().is_none() {
        return Err(context.fail(
            io,
            FlowHaltReason::TraceFailed,
            HaltPoint::Author,
            "Quiesce send has no clock observation",
        ));
    }
    Ok(())
}
