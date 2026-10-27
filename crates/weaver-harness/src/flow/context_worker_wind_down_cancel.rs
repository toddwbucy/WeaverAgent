//! Settle the original internal turn after force. No world response exists,
//! and no new instruction, placement, generation or dispatch is elected.
use super::*;
use crate::flow::pump_transport::{DecodePort, OrganPort};
use crate::flow::templates::TemplateCertificate;
use weaver_types::state_flow::NonDispatchCause;

impl WindDown {
    #[cfg(test)]
    pub(in crate::flow::context) fn test_stage(&self) -> String {
        format!("{:?}", self.stage)
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_records(&self) -> (Option<Sequence>, Option<Sequence>) {
        (self.started, self.closed)
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_cancelled_placement(&self) -> bool {
        matches!(self.pass.placement, Some(PlacementCompletion::Cancelled(_)))
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_unadmitted_author(&self) -> Option<(usize, String)> {
        self.turn_event.as_ref().map(|event| {
            assert!(!self.author_started);
            (
                event.turn.as_ref().unwrap().0.as_ptr() as usize,
                format!("{event:?}"),
            )
        })
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_unadmitted_close(&self) -> Option<(usize, String)> {
        self.unadmitted_close.as_ref().map(|event| {
            (
                event.turn.as_ref().unwrap().0.as_ptr() as usize,
                format!("{event:?}"),
            )
        })
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_unadmitted_refusal(&self) -> Option<(usize, String)> {
        self.pass.refusal.as_ref().map(|event| {
            assert!(!self.pass.refusal_started);
            let Some(serving::Payload::Refusal(raw) | serving::Payload::Fault(raw)) =
                &event.payload
            else {
                panic!("wind-down refusal keeps its exact payload");
            };
            (raw.get().as_ptr() as usize, format!("{event:?}"))
        })
    }
    #[cfg(test)]
    pub(in crate::flow::context) fn test_refusal_record(&self) -> Option<Sequence> {
        self.pass.refusal_record
    }
    pub(in crate::flow::context::worker_startup) fn owns_context(
        &self,
        context: &RecordingContext,
    ) -> bool {
        context
            .wind_down
            .as_ref()
            .is_some_and(|(turn, _)| *turn == self.turn)
            && context
                .open_turn
                .as_ref()
                .is_none_or(|turn| *turn == self.turn)
    }

    pub(in crate::flow::context::worker_startup) fn settled_for_leave(&self) -> bool {
        self.settled() || self.cancel_settled
    }

    pub(in crate::flow::context::worker_startup) fn retire_failed_placement_grants(
        &mut self,
        context: &mut RecordingContext,
    ) -> Result<(), RunFailure> {
        for pass in self
            .previous
            .iter_mut()
            .chain(std::iter::once(&mut self.pass))
        {
            generation::retire_failed_placement_grant(&mut pass.placement, context)?;
            if let Some(emission) = &mut pass.emission {
                emission.retire_failed_placement_grants(context)?;
            }
        }
        Ok(())
    }

    pub(in crate::flow::context::worker_startup) fn cancel(
        &mut self,
        context: &mut RecordingContext,
        decode: Option<&mut DecodePort>,
        gate: Option<&mut OrganPort>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        if self.settled_for_leave() {
            return Ok(());
        }
        if !self.owns_context(context) {
            return Err(fault(
                context,
                io,
                "forced wind-down lost its original turn".into(),
            ));
        }
        self.cancelled = true;
        if self.stage == WindStage::Generated {
            self.pass.emission = Some(Box::new(EmissionWork::cancelled(NonDispatchCause::Unload)));
            self.stage = WindStage::Emission;
        }
        if matches!(self.stage, WindStage::Emission | WindStage::Account) {
            return self.pass.emission.as_mut().unwrap().cancel(
                NonDispatchCause::Unload,
                context,
                decode,
                gate,
                io,
            );
        }
        context.revoke_place();
        if context.measure.is_some() {
            context.cancel_measurement(io)?;
        }
        if matches!(self.pass.placement, Some(PlacementCompletion::Granted(_))) {
            let Some(PlacementCompletion::Granted(grant)) = self.pass.placement.take() else {
                unreachable!()
            };
            match context.cancel_placement_grant(grant) {
                Ok(account) => self.pass.placement = Some(PlacementCompletion::Cancelled(account)),
                Err(error) => {
                    self.pass.placement = Some(PlacementCompletion::Granted(error.input));
                    return Err(error.failure.unwrap_or(RunFailure::PlaceOwned));
                }
            }
        }
        if context.generation.is_some() {
            context.cancel_generation(decode.ok_or(RunFailure::PlaceOwned)?, io)?;
        }
        Ok(())
    }

    pub(in crate::flow::context::worker_startup) fn advance_cancelled<I: PumpIo>(
        &mut self,
        certificate: &mut Option<TemplateCertificate>,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
        cause: weaver_types::Cause,
    ) -> Result<bool, RunFailure> {
        context.live(ports.io)?;
        self.cancel(
            context,
            ports.decode.as_deref_mut(),
            ports.gate.as_deref_mut(),
            ports.io,
        )?;
        if self.settled_for_leave() {
            return Ok(true);
        }
        // A start or close already admitted keeps its own event and ticket.
        // In particular a Recorded clean close is never replaced by a stop.
        if self.author_started {
            if !matches!(self.stage, WindStage::Start | WindStage::Closing) {
                return Err(fault(
                    context,
                    ports.io,
                    "wind-down author has no owning phase".into(),
                ));
            }
            self.record_turn(context, ports)?;
            return Ok(self.settled_for_leave());
        }
        if (self.pass.refusal_started && self.pass.refusal_record.is_none())
            || self.pass.refusal.is_some()
        {
            record_refusal(self, context, ports)?;
            return Ok(false);
        }
        match self.stage {
            WindStage::Filing => {
                let progress = context.advance_filing(ports)?;
                if let Some(done) = context.take_filing(ports.io)? {
                    self.pass.filing = Some(done);
                    self.stage = WindStage::PassComplete;
                    if rejected_filing(self) {
                        record_refusal(self, context, ports)?;
                    }
                } else if progress == FilingProgress::Pending {
                    context.pump_once(ports)?;
                }
                return Ok(false);
            }
            WindStage::Place => {
                let progress = context.advance_place(ports)?;
                if let Some(done) = context.take_placement(ports.io)? {
                    self.stage = if matches!(done, PlacementCompletion::Granted(_)) {
                        WindStage::Placed
                    } else {
                        WindStage::PassComplete
                    };
                    self.pass.placement = Some(done);
                } else if progress == crate::flow::context::PlaceProgress::Pending {
                    context.pump_once(ports)?;
                }
                return Ok(false);
            }
            WindStage::Generation => {
                let progress = context.advance_generation(ports)?;
                if let Some(done) = context.take_placed_generation(ports.io)? {
                    self.stage = if matches!(done, GenerationCompletion::Generated(_)) {
                        WindStage::Generated
                    } else {
                        WindStage::PassComplete
                    };
                    self.pass.generation = Some(done);
                } else if progress == crate::flow::context::PlaceProgress::Pending {
                    context.pump_once(ports)?;
                }
                return Ok(false);
            }
            WindStage::Emission | WindStage::Account => {
                let emission = self.pass.emission.as_mut().unwrap();
                emission.advance(&mut self.pass.generation, certificate, context, ports)?;
                if emission.settled() {
                    self.stage = WindStage::PassComplete;
                }
                return Ok(false);
            }
            _ => {}
        }
        if self.started.is_none() {
            if context.open_turn.is_some() {
                return Err(fault(
                    context,
                    ports.io,
                    "unstarted wind-down has an open turn".into(),
                ));
            }
            // The unadmitted start and instruction stay in this exact account.
            // Neither a turn.started nor a turn.closed is invented by force.
            context.wind_down = None;
            context.complete_wait(PumpDeadline::Decision);
            self.cancel_settled = true;
            return Ok(true);
        }
        if context.open_turn.as_ref() != Some(&self.turn) {
            return Err(fault(
                context,
                ports.io,
                "wind-down close lost its recorded start".into(),
            ));
        }
        if !matches!(
            self.outcome,
            TurnClose::Stopped {
                reason: StopReason::Unload,
                ..
            }
        ) && let Some(original) = self.turn_event.take()
        {
            // Force elects a stopped close only while the old close has no
            // ticket. Preserve that exact unadmitted event as caller evidence.
            assert!(self.unadmitted_close.is_none());
            self.unadmitted_close = Some(original);
        }
        self.outcome = TurnClose::Stopped {
            reason: StopReason::Unload,
            cause: Some(weaver_trace::Cause { uid: cause.uid }),
        };
        self.stage = WindStage::Closing;
        self.record_turn(context, ports)?;
        Ok(self.settled_for_leave())
    }

    pub(super) fn record_turn<I: PumpIo>(
        &mut self,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        generation::decision(context, ports.io)?;
        self.returned_author.check()?;
        if !self.author_started && context.caller_author_held() {
            context.pump_once(ports)?;
            return Ok(());
        }
        let closing = self.stage == WindStage::Closing;
        let result = if self.author_started {
            context.resume_author_and_wait(ports)
        } else {
            if self.turn_event.is_none() {
                self.turn_event = Some(AuthoredEvent {
                    kind: if closing {
                        serving::Kind::TurnClosed
                    } else {
                        serving::Kind::TurnStarted
                    },
                    subsystem: Subsystem::Harness,
                    turn: Some(self.turn.clone()),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: closing.then(|| serving::Payload::TurnClosed(self.outcome.clone())),
                });
            }
            self.author_started = true;
            context.author_and_wait(ports, self.turn_event.take().unwrap())
        };
        let sequence = match result {
            Ok(sequence) => sequence,
            Err(mut failure) => {
                if failure.reclaim_held(context) {
                    self.author_started = false;
                    self.turn_event = failure.original.take();
                    // The hold granted no author ticket. Only this caller
                    // regains its event; the original owner still owes work.
                    // Reduce admitted local work and wait only while Pending.
                    // Idle returns to the owner of an already completed answer.
                    if context.reduce_flow(ports.io)? == crate::flow::coordinator::Advance::Pending
                    {
                        context.pump_once(ports)?;
                    }
                    return Ok(());
                }
                return Err(self
                    .returned_author
                    .retain_started(failure, &mut self.author_started));
            }
        };
        self.author_started = false;
        if closing {
            self.closed = Some(sequence);
            context.wind_down = None;
            context.complete_wait(PumpDeadline::Decision);
            context.complete_wait(PumpDeadline::WindDown);
            self.stage = WindStage::Complete;
            self.cancel_settled = self.cancelled;
        } else {
            self.started = Some(sequence);
            self.stage = WindStage::Instruction;
        }
        Ok(())
    }
}
