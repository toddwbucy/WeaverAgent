//! Worker custody from recorded outbound selection through actual pair/read
//! completion. The loop elects recall. Readiness itself never runs a request.
use super::*;
use crate::flow::context::{InwardProgress, PairProgress};
use weaver_types::state_flow::{CallId, PairEnded, PairLane, PairStarted};

#[cfg(test)]
#[path = "context_worker_pairs_tests.rs"]
pub(in crate::flow::context) mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Begin,
    Opening,
    Ready,
    Recall,
    Reading,
    Complete,
}

pub(super) struct WorkerPairs {
    stage: Stage,
    effective: Option<EffectiveEmission>,
    calls: Vec<CallId>,
    elected: Option<CallId>,
    cancelled: bool,
}
impl WorkerPairs {
    fn new(effective: EffectiveEmission) -> Self {
        Self {
            stage: Stage::Begin,
            calls: effective
                .requests()
                .iter()
                .map(|p| p.call_id.clone())
                .collect(),
            effective: Some(effective),
            elected: None,
            cancelled: false,
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
        context: &mut RecordingContext,
        io: &mut impl PumpIo,
    ) -> Result<(), RunFailure> {
        self.cancelled = true;
        if context.pair_work.is_some() {
            context.bar_pending_pairs(NonDispatchCause::Stop, io)?;
        }
        if matches!(self.stage, Stage::Ready | Stage::Recall) {
            self.stage = Stage::Opening;
        }
        Ok(())
    }
    pub(super) fn advance<I: PumpIo>(
        &mut self,
        acts: &[SortAct],
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        match self.stage {
            Stage::Begin => {
                decision(context, ports.io)?;
                match context.begin_pair_execution(self.effective.take().unwrap(), ports.io) {
                    Ok(()) => {
                        if self.cancelled {
                            context.bar_pending_pairs(NonDispatchCause::Stop, ports.io)?;
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
                let progress = context.advance_pairs(ports)?;
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
                    if selected && !self.cancelled {
                        self.stage = Stage::Ready;
                    } else {
                        let call = ready.call_id.clone();
                        context.refuse_pair_dispatch(
                            &call,
                            if self.cancelled {
                                NonDispatchCause::Stop
                            } else {
                                NonDispatchCause::PlanBreak
                            },
                            ports.io,
                        )?;
                    }
                } else if progress == PairProgress::Pending {
                    context.pump_once(ports)?;
                }
            }
            Stage::Recall => {
                let call = self.elected.as_ref().expect("explicit recall election");
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
                    self.elected = None;
                    self.stage = Stage::Opening;
                } else {
                    context.pump_once(ports)?;
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
        emission.execution = Some(WorkerPairs::new(input.effective));
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
        work.elected = Some(call);
        work.stage = Stage::Recall;
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
