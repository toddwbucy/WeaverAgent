//! Spec 02 INSTRUCTION-METADATA's step variant. The committed revision's
//! active step supplies every reference; the certified literal family supplies
//! the only Text. A mismatch refuses before any instruction is authored.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::mirror;
use crate::flow::templates::{
    ExpansionIdentity, TemplateCertificate, TemplateExpansion, expand_step_instruction,
};
use weaver_trace::TurnRef;
use weaver_trace::serving::payloads as trace;
use weaver_types::RefusalRecord;
use weaver_types::state_flow as s;

/// The step variant's family. Kind loop.instruction, literal parts only.
pub(in crate::flow) const STEP_INSTRUCTION: &str = "step.instruction";

/// The exact references a step instruction carries as metadata.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::flow) struct StepMeta {
    pub plan: s::PlanRef,
    pub step_id: s::StepId,
    pub instruction: s::StatementRef,
    pub span: s::SpanRef,
    pub turn: TurnRef,
}

/// A checked, unauthored step instruction and its certified expansion.
#[derive(Debug)]
pub(in crate::flow) struct StepInstructionDraft {
    pub event: AuthoredEvent,
    pub expansion: TemplateExpansion,
    pub step: StepMeta,
}

/// The refusal case and its unauthored RefusalRecord::Instruction.
#[derive(Debug)]
pub(in crate::flow) struct StepInstructionRefused {
    pub case: s::InstructionRefusal,
    pub event: AuthoredEvent,
}

/// Why no draft: the committed summary is settling (retry, nothing is
/// decided), the run has failed, or a recorded refusal.
#[derive(Debug)]
pub(in crate::flow) enum StepInstructionError {
    Pending,
    Failed,
    Refused(StepInstructionRefused),
}

impl From<StepInstructionRefused> for StepInstructionError {
    fn from(refused: StepInstructionRefused) -> Self {
        Self::Refused(refused)
    }
}

impl RecordingContext {
    /// Check the requested step against the committed revision and the
    /// caller's turn, then expand the certified family. It authors nothing.
    /// wrong_turn precedes invalid_context, which precedes invalid_template.
    pub(in crate::flow) fn step_instruction(
        &mut self,
        expected: &s::PlanRef,
        step_id: &s::StepId,
        turn: &TurnRef,
        certificate: &TemplateCertificate,
    ) -> Result<StepInstructionDraft, StepInstructionError> {
        if turn.0.is_empty() || self.open_turn.as_ref() != Some(turn) {
            return Err(self
                .step_instruction_refusal(
                    s::InstructionRefusal::WrongTurn,
                    "step instruction is not for the current open turn",
                )
                .into());
        }
        let active = match self.committed_active_step(expected, step_id) {
            Ok(Some(active)) => active,
            Err(PlanSummaryUnavailable::Pending) => return Err(StepInstructionError::Pending),
            Err(PlanSummaryUnavailable::Failed) => return Err(StepInstructionError::Failed),
            // No committed revision, or not this step active in it.
            Ok(None)
            | Err(PlanSummaryUnavailable::Uninitialized | PlanSummaryUnavailable::Revoked) => {
                return Err(self
                    .step_instruction_refusal(
                        s::InstructionRefusal::InvalidContext,
                        "step is not the active step of the expected committed revision",
                    )
                    .into());
            }
        };
        let expansion = self.load_duty.as_ref().and_then(|load| {
            if !self.repair_certificate_matches(certificate) {
                return None;
            }
            let expansion = expand_step_instruction(
                certificate,
                ExpansionIdentity {
                    run_id: self.flow.run(),
                    scheme: &self.declaration.scheme,
                    checker_artifact_sha256: certificate.checker_artifact_sha256(),
                    inventory_sha256: certificate.inventory_sha256(),
                },
            )
            .ok()?;
            (expansion.inventory_sha256().as_str() == load.template_inventory.inventory_sha256)
                .then_some(expansion)
        });
        let Some(expansion) = expansion else {
            return Err(self
                .step_instruction_refusal(
                    s::InstructionRefusal::InvalidTemplate,
                    "step family is not in this run's certificate",
                )
                .into());
        };
        let step = StepMeta {
            plan: expected.clone(),
            step_id: step_id.clone(),
            instruction: active.0,
            span: active.1,
            turn: turn.clone(),
        };
        let event = AuthoredEvent {
            kind: Kind::LoopInstruction,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(turn.clone()),
            source: AuthoredSource::Loop,
            causal_parent: None,
            payload: Some(Payload::LoopInstruction(Box::new(
                trace::LoopInstructionRecord {
                    template_id: expansion.template_id().into(),
                    message: serde_json::value::to_raw_value(expansion.message())
                        .expect("certified Message"),
                    instruction: mirror::step_instruction(
                        &step.plan,
                        &step.step_id,
                        &step.instruction,
                        &step.span,
                    ),
                },
            ))),
        };
        Ok(StepInstructionDraft {
            event,
            expansion,
            step,
        })
    }

    /// The held instruction and span of the requested step, only when it is
    /// the active step of the expected locked revision in the committed cache.
    /// A settling or failed summary is its own answer, never "not active".
    fn committed_active_step(
        &mut self,
        expected: &s::PlanRef,
        step_id: &s::StepId,
    ) -> Result<Option<(s::StatementRef, s::SpanRef)>, PlanSummaryUnavailable> {
        if self.committed_plan_summary()?.is_none() {
            return Ok(None);
        }
        let Some(s::QueryResult::PlanCurrent {
            current: Some(plan),
            ..
        }) = self
            .plan_cache
            .committed
            .as_ref()
            .map(|cache| cache.read().result())
        else {
            return Ok(None);
        };
        if &plan.plan != expected || plan.status != s::PlanStatus::Locked {
            return Ok(None);
        }
        Ok(plan
            .steps
            .iter()
            .find(|step| &step.step_id == step_id && step.status == s::StepStatus::Active)
            .and_then(|step| Some((step.instruction.clone(), step.span.clone()?))))
    }

    pub(in crate::flow) fn step_instruction_refusal(
        &self,
        case: s::InstructionRefusal,
        detail: &str,
    ) -> StepInstructionRefused {
        let record = RefusalRecord::Instruction {
            template_id: STEP_INSTRUCTION.into(),
            case,
            detail: detail.into(),
        };
        StepInstructionRefused {
            case,
            event: AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: self.open_turn.clone(),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(Payload::Refusal(
                    serde_json::value::to_raw_value(&record).expect("refusal serializes"),
                )),
            },
        }
    }
}
