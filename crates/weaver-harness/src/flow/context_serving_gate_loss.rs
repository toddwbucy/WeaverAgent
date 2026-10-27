//! S3 gate loss settles only after this run's actual containment is empty.
//! A received result keeps its fate; a wholly unsent request keeps its packet.
use super::*;
use weaver_types::state_flow::NonDispatchCause;

impl Observation {
    pub(super) fn settle_gate_pairs<I: PumpIo>(
        &self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<bool, RunFailure> {
        if context.pair_work.is_none() {
            return Ok(true);
        }
        let gate = ports.gate.as_deref_mut().ok_or(RunFailure::PairOwned)?;
        if !context.serving_organs.gate_matches(gate)
            || !gate
                .failure()
                .is_some_and(|failure| failure.fault == crate::failure::ChannelFault::Closed)
            || self.observed_ns.is_none()
            || self.containment != Some(Containment::Emptied)
        {
            return Err(RunFailure::PairOwned);
        }
        context.reconcile_pair_send(ports.io)?;
        if context.pairs.executing().is_some()
            && context
                .pair_gate_identity
                .as_ref()
                .is_none_or(|port| !port.matches(gate))
        {
            return Err(RunFailure::PairOwned);
        }
        context
            .pairs
            .set_not_dispatched(NonDispatchCause::NoGate)
            .map_err(|error| {
                context.pair_fault(ports.io, format!("lost gate dispatch bar: {error:?}"))
            })?;
        // Containment ends execution. Its remaining receive/filing work uses
        // the same terminal Decision bound, never a fresh bound per packet.
        context.arm_pair_terminal_wait(ports.io)?;
        // A valid answer can be held, or queued behind the reset that reported
        // closure. The original caller must route it before an absent result
        // is elected. The port never admits another send on this path.
        if gate.held().is_some()
            || gate.closed_receive_pending()
            || context.pairs.pending_rejection().is_some()
        {
            return Ok(false);
        }
        let Some((call, exchange, sent)) = context.pairs.executing() else {
            return Ok(true);
        };
        let (call, exchange) = (call.clone(), exchange.clone());
        if sent {
            context.pairs.close_gate_fault(&call).map_err(|error| {
                context.pair_fault(ports.io, format!("contained gate disposition: {error:?}"))
            })?;
        } else {
            let work = context.pair_work.as_mut().unwrap();
            let send = work.send.as_ref().ok_or(RunFailure::PairOwned)?;
            if !send.matches_exchange(&exchange) || work.refused_send.is_some() {
                return Err(RunFailure::PairOwned);
            }
            let original = gate
                .withdraw_closed_pending(send)
                .map_err(|failure| RunFailure::Transport {
                    source: PumpSource::Gate,
                    failure,
                })?
                .ok_or(RunFailure::PairOwned)?;
            work.refused_send = Some(original);
            context
                .pairs
                .cancel_unsent(&call, &exchange)
                .map_err(|error| {
                    context.pair_fault(ports.io, format!("lost gate withdrawal: {error:?}"))
                })?;
        }
        Ok(true)
    }
}
