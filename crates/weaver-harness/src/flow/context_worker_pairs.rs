//! Worker custody from recorded outbound selection through actual pair/read
//! completion. The loop elects each lane. Readiness itself never runs a request.
use super::*;
use crate::flow::context::{InwardProgress, PairProgress};
use crate::flow::pump_transport::OrganPort;
use weaver_types::state_flow::{CallId, PairEnded, PairLane, PairStarted};

#[path = "context_worker_external.rs"]
mod external;
use external::ExternalWork;

#[cfg(test)]
#[path = "context_worker_external_tests.rs"]
pub(in crate::flow::context) mod external_tests;

#[cfg(test)]
#[path = "context_worker_pairs_tests.rs"]
pub(in crate::flow::context) mod tests;

enum Stage {
    Begin,
    Opening,
    Ready,
    Recall(CallId),
    Reading,
    External,
    Complete,
}

pub(super) struct WorkerPairs {
    stage: Stage,
    effective: Option<EffectiveEmission>,
    calls: Vec<CallId>,
    external: Vec<ExternalWork>,
    cancelled: Option<NonDispatchCause>,
    barred: Option<NonDispatchCause>,
    returned_filing: crate::flow::context::StoppedAuthor,
}
impl WorkerPairs {
    pub(super) fn completed_effective(&self) -> Option<&EffectiveEmission> {
        (matches!(self.stage, Stage::Complete) && self.cancelled.is_none())
            .then_some(self.effective.as_ref())
            .flatten()
    }
    fn new(effective: EffectiveEmission, barred: Option<NonDispatchCause>) -> Self {
        Self {
            stage: Stage::Begin,
            calls: effective
                .requests()
                .iter()
                .map(|p| p.call_id.clone())
                .collect(),
            effective: Some(effective),
            external: Vec::new(),
            cancelled: None,
            barred,
            returned_filing: crate::flow::context::StoppedAuthor::default(),
        }
    }
    pub(super) fn progress(&self) -> FoundationInputProgress {
        match self.stage {
            Stage::Ready => FoundationInputProgress::PairReady,
            Stage::Complete => FoundationInputProgress::PairsComplete,
            _ => FoundationInputProgress::Pending,
        }
    }
    pub(super) fn cancel(
        &mut self,
        cause: NonDispatchCause,
        context: &mut RecordingContext,
        gate: Option<&mut OrganPort>,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        let previous = self.cancelled;
        retain_cancellation(&mut self.cancelled, cause);
        let cause = self.cancelled.unwrap();
        if context.pair_work.is_some() {
            if context.pairs.executing().is_some() {
                if let Some(gate) = gate {
                    context.bar_executing_pairs(cause, gate, io)?;
                } else if previous != Some(cause) && !context.pairs.barred_for(cause) {
                    // The first election must reconcile the actual send on
                    // its port. Once barred, retained gate replies can settle
                    // after that port retires without renewing the election.
                    return Err(RunFailure::PairOwned);
                }
            } else {
                context.bar_pending_pairs(cause, io)?;
            }
        }
        if matches!(self.stage, Stage::Ready | Stage::Recall(_)) {
            self.stage = Stage::Opening;
        }
        Ok(())
    }
    pub(super) fn cancel_control(
        &self,
    ) -> Option<(&OrganEnvelope, &crate::flow::pump_transport::OrganSend)> {
        self.external.last()?.cancel_control()
    }

    pub(super) fn advance_cancel_control(
        &mut self,
        context: &mut RecordingContext,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !matches!(self.stage, Stage::External) {
            return Ok(true);
        }
        let cause = self.cancelled.ok_or(RunFailure::PairOwned)?;
        self.external
            .last_mut()
            .unwrap()
            .advance_cancel_control(cause, context, gate, io)
    }

    pub(super) fn route_cancel_reply(
        &mut self,
        context: &mut RecordingContext,
        gate: &mut OrganPort,
        io: &mut impl PumpIo,
    ) -> Result<bool, RunFailure> {
        if !matches!(self.stage, Stage::External) {
            return Ok(false);
        }
        self.external
            .last_mut()
            .unwrap()
            .route_cancel_reply(context, gate, io)
    }

    pub(super) fn advance<I: PumpIo>(
        &mut self,
        acts: &[SortAct],
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        match &self.stage {
            Stage::Begin => {
                decision(context, ports.io)?;
                match context.begin_pair_execution(self.effective.take().unwrap(), ports.io) {
                    Ok(()) => {
                        if let Some(cause) = self.cancelled.or(self.barred) {
                            context.bar_pending_pairs(cause, ports.io)?;
                        }
                        self.stage = Stage::Opening;
                    }
                    Err(error) => {
                        self.effective = Some(error.effective);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if error.refusal != PairBeginRefusal::Busy {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("worker pair admission: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(ports)?;
                    }
                }
            }
            Stage::Opening => {
                self.returned_filing.check()?;
                let progress = context
                    .advance_pairs(ports)
                    .map_err(|failure| self.returned_filing.retain(failure))?;
                if let Some(effective) = context.take_executed_pairs(ports.io)? {
                    self.effective = Some(effective);
                    context.complete_wait(PumpDeadline::Decision);
                    self.stage = Stage::Complete;
                } else if let Some(ready) = context.next_pair_execution() {
                    let selected = acts.iter().any(|act| match act {
                        SortAct::Tool { call_id, statement }
                        | SortAct::Recall { call_id, statement } => {
                            call_id == &ready.call_id && statement == &ready.statement
                        }
                        SortAct::Answer { .. } => false,
                    });
                    if selected && self.cancelled.is_none() {
                        self.stage = Stage::Ready;
                    } else {
                        let call = ready.call_id.clone();
                        context.refuse_pair_dispatch(
                            &call,
                            self.cancelled.unwrap_or(NonDispatchCause::PlanBreak),
                            ports.io,
                        )?;
                    }
                } else if progress == PairProgress::Pending {
                    context.pump_once(ports)?;
                }
            }
            Stage::Recall(call) => {
                if context.next_pair_execution().map(|p| &p.call_id) != Some(call) {
                    self.stage = Stage::Opening;
                    return Ok(());
                }
                match context.begin_inward_recall(call, ports.io)? {
                    Ok(()) => self.stage = Stage::Reading,
                    Err(refusal) => {
                        return Err(fault(
                            context,
                            ports.io,
                            format!("worker recall admission: {refusal:?}"),
                        ));
                    }
                }
            }
            Stage::Reading => {
                if context.advance_inward_recall(ports)? == InwardProgress::Terminal {
                    self.stage = Stage::Opening;
                } else {
                    context.pump_once(ports)?;
                }
            }
            Stage::External => {
                if self
                    .external
                    .last_mut()
                    .unwrap()
                    .advance(self.cancelled, context, ports)?
                {
                    self.stage = Stage::Opening;
                }
            }
            Stage::Ready | Stage::Complete => {}
        }
        Ok(())
    }
}

impl FoundationInput<'_> {
    /// Elect opening the recorded sort's conforming request batch. Omitted
    /// requests receive plan_break terminals; they never disappear. Selection
    /// order does not replace the request source order required by Spec 05.
    pub fn begin_pairs(&mut self) -> Result<(), FoundationInputProgress> {
        self.begin_selected_pairs(None)
    }

    /// File every original conforming pair with a round_limit terminal. The
    /// worker elects this only after exhausting its execution-round budget.
    pub fn bar_round_limit(&mut self) -> Result<(), FoundationInputProgress> {
        self.begin_selected_pairs(Some(NonDispatchCause::RoundLimit))
    }

    fn begin_selected_pairs(
        &mut self,
        barred: Option<NonDispatchCause>,
    ) -> Result<(), FoundationInputProgress> {
        if self.progress() != FoundationInputProgress::Sorted {
            return Err(self.progress());
        }
        let emission = self.engine.as_mut().unwrap().emission.as_mut().unwrap();
        let Some(OutboundCompletion::Sorted(sorted)) = emission.result.take() else {
            unreachable!()
        };
        let (input, record) = sorted.into_parts();
        emission.selected = Some((input.source, input.acts));
        emission.sort_record = Some(record);
        emission.execution = Some(WorkerPairs::new(input.effective, barred));
        emission.stage = EmissionStage::Execution;
        Ok(())
    }

    pub fn ready_pair(&self) -> Option<&PairStarted> {
        if self.progress() != FoundationInputProgress::PairReady {
            return None;
        }
        self.serving.owner.context.next_pair_execution()
    }

    /// Elect exactly the next ready inward request. A foreign ID or external
    /// request is returned without changing the batch or starting a read.
    pub fn recall(&mut self, call: CallId) -> Result<(), CallId> {
        if !self
            .ready_pair()
            .is_some_and(|ready| ready.call_id == call && ready.lane == PairLane::Recall)
        {
            return Err(call);
        }
        let work = self
            .engine
            .as_mut()
            .unwrap()
            .emission
            .as_mut()
            .unwrap()
            .execution
            .as_mut()
            .unwrap();
        work.stage = Stage::Recall(call);
        Ok(())
    }

    /// Elect the next selected external request, retaining an unused harness
    /// exchange across backpressure. No request is executed by becoming ready.
    pub fn execute(&mut self, call: CallId) -> Result<(), CallId> {
        if !self
            .ready_pair()
            .is_some_and(|ready| ready.call_id == call && ready.lane == PairLane::External)
        {
            return Err(call);
        }
        let Some(ordinal) = self.serving.gate_outgoing.checked_add(1) else {
            return Err(call);
        };
        self.serving.gate_outgoing = ordinal;
        let work = self
            .engine
            .as_mut()
            .unwrap()
            .emission
            .as_mut()
            .unwrap()
            .execution
            .as_mut()
            .unwrap();
        work.external.push(ExternalWork::new(
            call,
            weaver_types::ExchangeId {
                opener: weaver_types::Opener::Harness,
                ordinal,
            },
        ));
        work.stage = Stage::External;
        Ok(())
    }

    /// Read-only terminal facts, released only from an actual CompletedPair.
    /// This borrowed record cannot construct a model-result or placement grant.
    pub fn pair_outcome(&self, call: &CallId) -> Option<&PairEnded> {
        let work = self
            .engine
            .as_ref()?
            .emission
            .as_ref()?
            .execution
            .as_ref()?;
        if !work.calls.contains(call) {
            return None;
        }
        self.serving
            .owner
            .context
            .pairs
            .completed(call)
            .ok()
            .flatten()
            .map(|p| p.ended())
    }
}
