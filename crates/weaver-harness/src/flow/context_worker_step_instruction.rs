//! The successor's step instruction: author the checked step loop.instruction,
//! file it, place it as Spec 08's instruction unit after any elected results,
//! and keep the Recorded placement as a private receipt. It chooses no step,
//! starts no generation and is not a strategy.
use super::*;
use crate::flow::context::step_instruction::StepMeta;
use crate::flow::context::{FilingCompletion, FilingProgress};
use crate::flow::ledger::SourceDisposition;
use crate::flow::material::SelectedUnit;
use crate::flow::templates::TemplateExpansion;
use std::sync::Arc;
use weaver_trace::{Sequence, TurnRef};
use weaver_types::state_flow::{
    CallId, DecimalU64, InstructionRefusal, PlanRef, SourceBlockId, SpanRef, StatementRef, StepId,
};
use weaver_types::{FaultCase, FaultReport};

/// One elected step instruction, from its check through its placement.
pub(super) struct StepWork {
    expected: PlanRef,
    step_id: StepId,
    turn: TurnRef,
    /// The progress the operation returns to when it refuses.
    back: EngineStage,
    event: Option<AuthoredEvent>,
    expansion: Option<TemplateExpansion>,
    meta: Option<StepMeta>,
    filing: Option<FilingCompletion>,
    refusal: Option<AuthoredEvent>,
    case: Option<InstructionRefusal>,
    refusal_started: bool,
    returned: crate::flow::context::StoppedAuthor,
    pub(super) source: Option<SourceBlockId>,
    pub(super) unit: Option<SelectedUnit>,
}

/// The step operation's refusal, after its record is Recorded. The engine
/// returns to the progress it had before the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundationStepRefusal {
    /// None for a correctly rejected expansion, recorded as its template fault.
    pub case: Option<InstructionRefusal>,
    pub sequence: Sequence,
}

/// The successor placement's Recorded receipt: the actual state.placement
/// that placed this run's filed step instruction for the named step. It is
/// taken once and grants no generation, mutation or progression.
///
/// ```compile_fail,E0277
/// fn cloned<T: Clone>() {}
/// cloned::<weaver_harness::StepPlacementReceipt>();
/// ```
///
/// ```compile_fail,E0451
/// fn forged() -> weaver_harness::StepPlacementReceipt {
///     weaver_harness::StepPlacementReceipt { origin: todo!() }
/// }
/// ```
pub struct StepPlacementReceipt {
    pub(in crate::flow) origin: Arc<()>,
    pub(in crate::flow) meta: StepMeta,
    pub(in crate::flow) source: SourceBlockId,
    pub(in crate::flow) placement: Sequence,
    pub(in crate::flow) assembly_id: DecimalU64,
    pub(in crate::flow) ended_ns: u64,
}
impl StepPlacementReceipt {
    pub fn plan(&self) -> &PlanRef {
        &self.meta.plan
    }
    pub fn step_id(&self) -> &StepId {
        &self.meta.step_id
    }
    pub fn instruction(&self) -> &StatementRef {
        &self.meta.instruction
    }
    pub fn span(&self) -> &SpanRef {
        &self.meta.span
    }
    /// The filed step loop.instruction the placement carried.
    pub fn source(&self) -> &SourceBlockId {
        &self.source
    }
    pub fn placement(&self) -> Sequence {
        self.placement
    }
    pub fn assembly_id(&self) -> DecimalU64 {
        self.assembly_id
    }
}
impl std::fmt::Debug for StepPlacementReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepPlacementReceipt")
            .field("plan", self.plan())
            .field("step_id", self.step_id())
            .field("source", &self.source)
            .field("placement", &self.placement)
            .field("assembly_id", &self.assembly_id)
            .finish()
    }
}

/// The placement holds this run's instruction unit for exactly this filed
/// step instruction, as its last unit. Another unit kind, another source or a
/// foreign unit is no successor placement.
pub(in crate::flow) fn placed_step_unit(
    units: &[Arc<SelectedUnit>],
    source: &SourceBlockId,
    context: &RecordingContext,
) -> bool {
    units
        .last()
        .is_some_and(|unit| crate::flow::material::is_instruction_unit(unit, source, context))
}

impl<'a> FoundationInput<'a> {
    /// Place the active step's certified instruction for the next pass. From
    /// Task it follows the task input; from PairsComplete it follows the
    /// elected complete results, as place_results would place them. A refused
    /// check records its RefusalRecord::Instruction and returns the engine to
    /// its prior progress. Nothing is generated until generate().
    #[allow(clippy::result_large_err)]
    pub fn place_step_instruction(
        &mut self,
        calls: Vec<CallId>,
        expected: PlanRef,
        step_id: StepId,
        turn: TurnKey,
        label: String,
        method: CutMethod,
    ) -> Result<(), Vec<CallId>> {
        let progress = self.progress();
        if label.is_empty()
            || !matches!(
                progress,
                FoundationInputProgress::Task | FoundationInputProgress::PairsComplete
            )
            || (progress == FoundationInputProgress::Task && !calls.is_empty())
        {
            return Err(calls);
        }
        let mut units = vec![];
        if progress == FoundationInputProgress::PairsComplete {
            for (index, call) in calls.iter().enumerate() {
                if calls[..index].contains(call) {
                    return Err(calls);
                }
                let Ok(unit) = self.serving.owner.context.hold_place_return(call) else {
                    return Err(calls);
                };
                units.push(unit);
            }
            let Some(journal) = self.serving.owner.context.boot_journal.as_ref() else {
                return Err(calls);
            };
            if !journal
                .project(None)
                .is_ok_and(|prefix| journal.validate_selection(&prefix, &units).is_ok())
            {
                return Err(calls);
            }
        }
        let work = self.engine.as_mut().unwrap();
        let back = work.stage;
        work.step = Some(Box::new(StepWork {
            expected,
            step_id,
            turn: TurnRef(turn.0),
            back,
            event: None,
            expansion: None,
            meta: None,
            filing: None,
            refusal: None,
            case: None,
            refusal_started: false,
            returned: Default::default(),
            source: None,
            unit: None,
        }));
        work.step_refusal = None;
        if back == EngineStage::Emission {
            work.step_calls = calls;
            work.step_units = Some(units);
        }
        work.selection = Some((label, method));
        work.stage = EngineStage::StepInstruction;
        Ok(())
    }

    /// Take the step operation's Recorded refusal, if it refused.
    pub fn take_step_refusal(&mut self) -> Option<FoundationStepRefusal> {
        self.engine.as_mut()?.step_refusal.take()
    }

    /// Take the successor placement's Recorded receipt once.
    pub fn take_step_placement(&mut self) -> Option<StepPlacementReceipt> {
        self.engine.as_mut()?.step_receipt.take()
    }
}

impl EngineWork {
    /// Check, author and file the step instruction. Its unit then joins the
    /// ordinary Prepare and Place stages.
    pub(super) fn advance_step<I: PumpIo>(
        &mut self,
        certificate: &Option<TemplateCertificate>,
        context: &mut RecordingContext,
        ports: &mut PumpPorts<'_, I>,
    ) -> Result<(), RunFailure> {
        decision(context, ports.io)?;
        let step = self.step.as_mut().unwrap();
        match self.stage {
            EngineStage::StepInstruction => {
                if step.event.is_none() && step.refusal.is_none() {
                    let Some(certificate) = certificate.as_ref() else {
                        return Err(fault(
                            context,
                            ports.io,
                            "step instruction lost its template certificate".into(),
                        ));
                    };
                    match context.step_instruction(
                        &step.expected,
                        &step.step_id,
                        &step.turn,
                        certificate,
                    ) {
                        Ok(draft) => {
                            step.event = Some(draft.event);
                            step.expansion = Some(draft.expansion);
                            step.meta = Some(draft.step);
                        }
                        Err(refused) => {
                            step.case = Some(refused.case);
                            step.refusal = Some(refused.event);
                        }
                    }
                }
                if step.refusal.is_some() {
                    self.stage = EngineStage::StepRefusal;
                    return Ok(());
                }
                match context.begin_filing(
                    step.event.take().unwrap(),
                    PumpDeadline::Decision,
                    ports.io,
                ) {
                    Ok(()) => self.stage = EngineStage::StepFiling,
                    Err(error) => {
                        step.event = Some(error.event);
                        if let Some(failure) = error.failure {
                            return Err(failure);
                        }
                        if error.refusal != crate::flow::context::FilingBeginRefusal::Busy {
                            return Err(fault(
                                context,
                                ports.io,
                                format!("step instruction filing: {:?}", error.refusal),
                            ));
                        }
                        context.pump_once(ports)?;
                    }
                }
            }
            EngineStage::StepFiling => {
                if step.filing.is_none() {
                    let progress = context.advance_filing(ports)?;
                    step.filing = context.take_filing(ports.io)?;
                    if step.filing.is_none() {
                        if progress == FilingProgress::Pending {
                            context.pump_once(ports)?;
                        }
                        return Ok(());
                    }
                }
                if self.cancelled.is_some() {
                    self.stage = EngineStage::Cancelled;
                    context.complete_wait(PumpDeadline::Decision);
                    return Ok(());
                }
                let proof = step.filing.as_ref().unwrap();
                if let Some(rejected) = proof.receipts().dispositions().find_map(|d| match d {
                    SourceDisposition::CorrectlyRejected(rejected) => Some(rejected),
                    _ => None,
                }) {
                    // A correctly rejected expansion follows state_template_rejected.
                    let fault = FaultReport {
                        case: FaultCase::StateTemplateRejected,
                        account: serde_json::value::to_raw_value(&serde_json::json!({
                            "template_id": crate::flow::context::step_instruction::STEP_INSTRUCTION,
                            "source_id": rejected.source(), "result": rejected.reply()
                        }))
                        .unwrap(),
                    };
                    step.refusal = Some(AuthoredEvent {
                        kind: weaver_trace::serving::Kind::Fault,
                        subsystem: weaver_trace::Subsystem::Harness,
                        turn: Some(step.turn.clone()),
                        source: crate::flow::author::AuthoredSource::Runtime,
                        causal_parent: Some(proof.sequence()),
                        payload: Some(weaver_trace::serving::Payload::Fault(
                            serde_json::value::to_raw_value(&fault).unwrap(),
                        )),
                    });
                    self.stage = EngineStage::StepRefusal;
                    return Ok(());
                }
                let expansion = step.expansion.as_ref().unwrap();
                let sources: Vec<_> = proof.receipts().dispositions().collect();
                let valid = proof.belongs_to(context)
                    && sources.len() == 1
                    && matches!(sources[0], SourceDisposition::Committed(source)
                        if matches!(source.file(), crate::flow::ledger::LedgerFile::Statement(file)
                            if file.source.kind == "loop.instruction" && file.source.origin == "loop"
                                && file.block_id.run == context.flow.run()
                                && file.block_id.sequence.0 == proof.sequence().0
                                && file.text == expansion.text()));
                if !valid {
                    return Err(fault(
                        context,
                        ports.io,
                        "step instruction has no exact committed filing".into(),
                    ));
                }
                let proof = step.filing.take().unwrap();
                let Some(SourceDisposition::Committed(source)) =
                    proof.into_receipts().into_dispositions().unwrap().next()
                else {
                    unreachable!("checked one committed disposition")
                };
                step.source = Some(source.source().clone());
                let unit = crate::flow::material::from_filed_instruction(
                    Arc::clone(context.material_origin()),
                    source,
                    step.expansion.take().unwrap(),
                )
                .map_err(|(_, kind)| {
                    fault(
                        context,
                        ports.io,
                        format!("step instruction unit: {kind:?}"),
                    )
                })?;
                step.unit = Some(unit);
                if step.back == EngineStage::TaskReady {
                    self.step_calls.clear();
                } else {
                    self.rounds.push(RetainedRound {
                        _generation: self.generation.take(),
                        _emission: self.emission.take(),
                    });
                    self.return_calls = std::mem::take(&mut self.step_calls);
                    self.return_units = self.step_units.take();
                }
                self.stage = EngineStage::Prepare;
            }
            EngineStage::StepRefusal => {
                step.returned.check()?;
                let result = if step.refusal_started {
                    context.resume_author_and_wait(ports)
                } else {
                    step.refusal_started = true;
                    context.author_and_wait(ports, step.refusal.take().unwrap())
                };
                let sequence = match result {
                    Ok(sequence) => sequence,
                    Err(mut failure) => {
                        if failure.reclaim_held(context) {
                            step.refusal_started = false;
                            step.refusal = failure.original.take();
                            if context.reduce_flow(ports.io)?
                                == crate::flow::coordinator::Advance::Pending
                            {
                                context.pump_once(ports)?;
                            }
                            return Ok(());
                        }
                        return Err(step
                            .returned
                            .retain_started(failure, &mut step.refusal_started));
                    }
                };
                self.step_refusal = Some(FoundationStepRefusal {
                    case: step.case,
                    sequence,
                });
                self.stage = if self.cancelled.is_some() {
                    context.complete_wait(PumpDeadline::Decision);
                    EngineStage::Cancelled
                } else {
                    step.back
                };
                self.step = None;
                self.step_calls.clear();
                self.step_units = None;
                self.selection = None;
            }
            _ => unreachable!("step stages only"),
        }
        Ok(())
    }

    /// The Recorded placement of a step instruction becomes its receipt.
    pub(super) fn keep_step_placement(
        &mut self,
        context: &RecordingContext,
        ended_ns: u64,
    ) -> Result<(), &'static str> {
        let Some(step) = self.step.take() else {
            return Ok(());
        };
        let Some(PlacementCompletion::Granted(grant)) = self.placement.as_ref() else {
            return Ok(());
        };
        let (Some(source), Some(meta)) = (step.source, step.meta) else {
            return Err("step placement lost its filed instruction");
        };
        if !placed_step_unit(&grant.units, &source, context) || grant.turn != meta.turn {
            return Err("step placement does not hold its filed instruction unit");
        }
        self.step_receipt = Some(StepPlacementReceipt {
            origin: Arc::clone(&grant.origin),
            meta,
            source,
            placement: grant.placement_sequence(),
            assembly_id: grant.assembly_id(),
            ended_ns,
        });
        Ok(())
    }
}
