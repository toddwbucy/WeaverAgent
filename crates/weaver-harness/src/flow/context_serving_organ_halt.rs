//! Owed run-scoped organ observations use ordinary trace capacity after halt.
//! Every step is local work under the caller's original halt clock.
use super::*;

impl Observation {
    fn owns_pending(&self, context: &RecordingContext) -> bool {
        self.author.as_ref().is_some_and(|owner| {
            owner.ticket().is_some()
                && owner.ticket() == context.barrier.pending_author().map(|held| held.ticket)
        })
    }

    fn owns_claim(&self, context: &RecordingContext) -> bool {
        self.author.as_ref().is_some_and(|owner| {
            context
                .serving_observation_author
                .as_ref()
                .is_some_and(|actual| Arc::ptr_eq(actual, owner))
        })
    }

    fn retain_waiting(&mut self, context: &mut RecordingContext) {
        if let Some(owner) = &self.author
            && context.waiting_content.as_ref().is_some_and(|waiting| {
                waiting.matches(Some(&AuthorClaim::ServingObservation(Arc::clone(owner))))
            })
        {
            assert!(self.event.is_none(), "one original observed event");
            self.event = Some(context.waiting_content.take().unwrap().event);
        }
    }

    fn abandon(&mut self, context: &mut RecordingContext, detail: &str) {
        if self.sequence.is_some() || self.unavailable.is_some() {
            return;
        }
        self.sequence = self.author.as_ref().and_then(|owner| owner.sequence());
        if self.owns_pending(context) {
            self.stopped_record = context.barrier.retain_halt_record();
        }
        self.retain_waiting(context);
        if self.owns_claim(context) {
            context.serving_observation_author.take();
        }
        if self.sequence.is_none() {
            self.unavailable = Some(detail.into());
            context.halt_report_secondary(
                Some(context.last_ns),
                format!("{} observation: {detail}", self.organ),
            );
        }
    }

    fn advance_stopped(
        &mut self,
        context: &mut RecordingContext,
    ) -> Result<Option<Sequence>, String> {
        if let Some(sequence) = self.author.as_ref().and_then(|owner| owner.sequence()) {
            self.sequence = Some(sequence);
            if self.owns_claim(context) {
                context.serving_observation_author.take();
            }
            return Ok(None);
        }
        if self.returned.failure.is_some() {
            return Err("returned attempt remains with its stopped author".into());
        }
        let Some(settle_by) = self.settle_by else {
            return Err("original observation clock is unavailable".into());
        };
        if self.author.is_none()
            && ((!self.last_word.as_ref().is_some_and(|word| word.eof)
                && context.last_ns < settle_by)
                || (self.organ == "gate" && self.containment.is_none()))
        {
            // Do not poll or hold mandatory shutdown behind stderr/containment.
            return Ok(None);
        }
        self.prepare(context);
        let owner = Arc::clone(self.author.as_ref().unwrap());
        if owner.ticket().is_none() {
            if context.barrier.pending().is_some()
                || context.barrier.pending_author().is_some()
                || (context.serving_observation_author.is_some() && !self.owns_claim(context))
            {
                return Ok(None);
            }
            self.retain_waiting(context);
            if context.waiting_content.is_some() {
                return Ok(None);
            }
            let wall = wall_ms()?;
            let event = self
                .event
                .take()
                .ok_or("observed event has no exact original")?;
            if let Err(error) = context.barrier.queue_halt_organ_observation(
                OwedOrganObservation(event),
                &context.author,
                context.last_ns,
                wall,
            ) {
                self.event = Some(error.event);
                return Err(error.detail);
            }
            assert!(owner.claim(context.barrier.pending_author().unwrap().ticket));
            context.serving_observation_author = Some(Arc::clone(&owner));
        }
        if !self.owns_pending(context) || !self.owns_claim(context) {
            return Err("observed event lost its exact pending owner".into());
        }
        context
            .resume_trace_only_and_bind()
            .map_err(|error| format!("owed organ trace: {error:?}"))?;
        self.sequence = owner.sequence();
        if self.sequence.is_some() {
            context.serving_observation_author.take();
        }
        Ok(self.sequence)
    }
}

impl RecordingContext {
    /// Once turn accounts retire, only this run-scoped debt may progress here.
    /// Reusing the report's clock preserves its first failure and absolute cap.
    pub(in crate::flow::context) fn advance_halt_organ_trace(
        &mut self,
        io: &mut impl PumpIo,
    ) -> HaltTraceProgress {
        if self.halt_report.is_none() {
            return HaltTraceProgress::Unavailable;
        }
        if !self.halt_report_clock(io) {
            return HaltTraceProgress::AbortRequired;
        }
        if self.barrier.standing_failure().is_some()
            || self.halt_report.as_ref().unwrap().trace_unavailable()
        {
            self.abandon_halt_organ_observations("trace unavailable during cleanup");
            return HaltTraceProgress::Unavailable;
        }
        let progress = self.advance_halt_organ_observation();
        if let Some(sequence) = self.barrier.last_trace_only_sequence() {
            self.halt_report
                .as_mut()
                .unwrap()
                .retain_trace_sequence(sequence);
        }
        if !self.halt_report_clock(io) {
            return HaltTraceProgress::AbortRequired;
        }
        progress
    }

    pub(in crate::flow::context) fn owns_halt_organ_record(&self) -> bool {
        self.serving_organs.order().into_iter().any(|gate| {
            self.serving_organs
                .observation(gate)
                .is_some_and(|observation| observation.owns_pending(self))
        })
    }

    pub(in crate::flow::context) fn has_owed_organ_observation(&self) -> bool {
        self.serving_organs.order().into_iter().any(|gate| {
            self.serving_organs
                .observation(gate)
                .is_some_and(|observation| {
                    observation.sequence.is_none() && observation.unavailable.is_none()
                })
        })
    }

    /// Stderr has no pollable notification here. Observe its snapshot on the
    /// existing short wake, ending at the original observation's settlement
    /// bound. A prepared author waits on writer capacity instead of polling.
    pub(in crate::flow::context) fn halt_organ_settle_wake(&self) -> Option<u64> {
        for gate in self.serving_organs.order() {
            let Some(observation) = self.serving_organs.observation(gate) else {
                continue;
            };
            if observation.sequence.is_some() || observation.unavailable.is_some() {
                continue;
            }
            return observation
                .settle_by
                .filter(|bound| {
                    observation.author.is_none()
                        && !observation.last_word.as_ref().is_some_and(|word| word.eof)
                        && self.last_ns < *bound
                })
                .map(|bound| self.last_ns.saturating_add(10_000_000).min(bound));
        }
        None
    }

    pub(in crate::flow::context) fn advance_halt_organ_observation(&mut self) -> HaltTraceProgress {
        assert_eq!(self.phase, RecordingPhase::Failed);
        for gate in self.serving_organs.order() {
            let Some(mut observation) = self.serving_organs.take(gate) else {
                continue;
            };
            let result = if observation.sequence.is_some() || observation.unavailable.is_some() {
                HaltTraceProgress::Idle
            } else {
                match observation.advance_stopped(self) {
                    Ok(Some(sequence)) => HaltTraceProgress::TraceRecorded(sequence),
                    Ok(None) if observation.sequence.is_some() => HaltTraceProgress::Idle,
                    Ok(None) => HaltTraceProgress::Pending,
                    Err(detail) => {
                        observation.abandon(self, &detail);
                        HaltTraceProgress::Unavailable
                    }
                }
            };
            self.serving_organs.restore(gate, observation);
            if result != HaltTraceProgress::Idle {
                return result;
            }
        }
        HaltTraceProgress::Idle
    }

    pub(in crate::flow::context) fn abandon_halt_organ_observations(&mut self, detail: &str) {
        for gate in self.serving_organs.order() {
            if let Some(mut observation) = self.serving_organs.take(gate) {
                observation.abandon(self, detail);
                self.serving_organs.restore(gate, observation);
            }
        }
    }
}
