//! Checked plan predicates consume recorded proofs. They choose no workflow,
//! create no effect, and never classify a held statement again.
use super::CommittedPlanSummary;
use crate::flow::context::{RecordedRead, RecordedReadRefusal, RecordingContext};
use sha2::{Digest as _, Sha256};
use weaver_types::state_flow as s;

#[derive(Debug)]
pub enum PlanRequest {
    AssessAct {
        expected: s::PlanRef,
        step_id: s::StepId,
        statement: s::StatementRef,
        act: s::PlanAct,
    },
    WorkReference {
        expected: s::PlanRef,
        step_id: s::StepId,
        span: s::SpanRef,
        through: s::SourcePosition,
    },
}

impl PlanRequest {
    pub(in crate::flow) fn expected(&self) -> &s::PlanRef {
        match self {
            Self::AssessAct { expected, .. } | Self::WorkReference { expected, .. } => expected,
        }
    }
    pub(in crate::flow) fn step_id(&self) -> &s::StepId {
        match self {
            Self::AssessAct { step_id, .. } | Self::WorkReference { step_id, .. } => step_id,
        }
    }
    pub(in crate::flow) fn operation(&self) -> s::PlanPortOperation {
        match self {
            Self::AssessAct { .. } => s::PlanPortOperation::AssessAct,
            Self::WorkReference { .. } => s::PlanPortOperation::WorkReference,
        }
    }
    pub(in crate::flow) fn local_refusal(&self, seed: &str) -> Option<s::PlanOperationRefused> {
        (self.expected().plan_id.seed_id != seed
            || self.step_id().seed_id != seed
            || matches!(self, Self::WorkReference { span, .. } if span.seed_id != seed))
        .then(|| {
            refusal(
                s::PlanOperationCase::InvalidTarget,
                "plan request belongs to another seed",
            )
        })
    }
    pub(in crate::flow) fn selector(&self) -> s::ReadSelector {
        match self {
            Self::AssessAct { statement, .. } => s::ReadSelector::Statement {
                statement: statement.clone(),
            },
            Self::WorkReference { span, through, .. } => s::ReadSelector::SpanPreview {
                span: span.clone(),
                through: through.clone(),
            },
        }
    }
}

pub(in crate::flow) enum PlanCheckFailure {
    Refused(s::PlanOperationRefused),
    Protocol(&'static str),
}

fn refusal(case: s::PlanOperationCase, detail: &str) -> s::PlanOperationRefused {
    s::PlanOperationRefused {
        case,
        detail: detail.into(),
        read_code: None,
    }
}
fn refuse(case: s::PlanOperationCase, detail: &str) -> PlanCheckFailure {
    PlanCheckFailure::Refused(refusal(case, detail))
}

pub(in crate::flow) fn active<'a>(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &'a CommittedPlanSummary,
) -> Result<(&'a s::PlanSnapshot, &'a s::PlanStep), PlanCheckFailure> {
    use s::PlanOperationCase as Case;
    if !current.read().belongs_to(context)
        || current.read().selector() != &(s::ReadSelector::PlanCurrent {})
    {
        return Err(PlanCheckFailure::Protocol(
            "plan predicate has a foreign current-plan proof",
        ));
    }
    let s::QueryResult::PlanCurrent { current, .. } = current.read().result() else {
        return Err(PlanCheckFailure::Protocol(
            "plan predicate has no current-plan result",
        ));
    };
    // The operation validates its requested seed before consuming this proof.
    // A foreign cached answer is therefore component failure, not a different
    // current revision that can be turned into an ordinary target refusal.
    if current
        .as_ref()
        .is_some_and(|plan| plan.plan.plan_id.seed_id != request.expected().plan_id.seed_id)
    {
        return Err(PlanCheckFailure::Protocol(
            "current-plan proof belongs to another loaded seed",
        ));
    }
    let plan = current
        .as_ref()
        .filter(|p| p.status == s::PlanStatus::Locked)
        .ok_or_else(|| refuse(Case::NoLockedPlan, "no current locked plan"))?;
    if &plan.plan != request.expected() {
        return Err(refuse(
            Case::InvalidTarget,
            "expected plan is not the current revision",
        ));
    }
    let step = plan
        .steps
        .iter()
        .find(|step| &step.step_id == request.step_id() && step.status == s::StepStatus::Active)
        .ok_or_else(|| refuse(Case::InvalidTarget, "requested step is not active"))?;
    if let PlanRequest::WorkReference { span, .. } = request
        && step.span.as_ref() != Some(span)
    {
        return Err(refuse(
            Case::InvalidTarget,
            "requested work span is not the active step's span",
        ));
    }
    Ok((plan, step))
}

pub(in crate::flow) fn assess(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
    statement_read: &RecordedRead,
    loaded_tools: &weaver_trace::serving::payloads::ToolPolicies,
) -> Result<(), PlanCheckFailure> {
    let (plan, step) = active(request, context, current)?;
    let PlanRequest::AssessAct { statement, act, .. } = request else {
        return Err(PlanCheckFailure::Protocol(
            "act predicate received another request",
        ));
    };
    if !statement_read.belongs_to(context) || statement_read.selector() != &request.selector() {
        return Err(PlanCheckFailure::Protocol(
            "act predicate has a foreign statement read",
        ));
    }
    let s::QueryResult::Entries { entries } = statement_read.result() else {
        return Err(PlanCheckFailure::Protocol(
            "statement proof has another result family",
        ));
    };
    let [entry] = entries.as_slice() else {
        return Err(PlanCheckFailure::Protocol(
            "statement proof does not retain one entry",
        ));
    };
    let [binding] = entry.bindings.as_slice() else {
        return Err(PlanCheckFailure::Protocol(
            "statement proof does not retain one binding",
        ));
    };
    if binding.ordinal != statement.binding
        || !matches!(&entry.source, s::ReadSource::Event(source) if source.block_id == statement.block_id)
    {
        return Err(PlanCheckFailure::Protocol(
            "statement proof changed its requested identity",
        ));
    }
    let category = plan
        .categories
        .iter()
        .find(|category| category.id == step.category)
        .ok_or(PlanCheckFailure::Protocol("active step category is absent"))?;
    if let s::PlanAct::ExternalTool { name } = act
        && (name != "sh" || loaded_tools.sh.is_none())
    {
        return Err(refuse(
            s::PlanOperationCase::UnknownTool,
            "external tool is not granted",
        ));
    }
    if !category.verbs.contains(&binding.value.verb) || !category.acts.contains(act) {
        return Err(refuse(
            s::PlanOperationCase::WrongCategory,
            "statement or act is outside the step category",
        ));
    }
    Ok(())
}

pub(in crate::flow) fn work_reference(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
    preview: &RecordedRead,
) -> Result<s::WorkRef, PlanCheckFailure> {
    active(request, context, current)?;
    let PlanRequest::WorkReference {
        expected,
        step_id,
        span,
        through,
    } = request
    else {
        return Err(PlanCheckFailure::Protocol(
            "work predicate received another request",
        ));
    };
    if !preview.belongs_to(context) || preview.selector() != &request.selector() {
        return Err(PlanCheckFailure::Protocol(
            "work predicate has a foreign preview read",
        ));
    }
    let s::QueryResult::SpanPreview {
        span: actual,
        through: frontier,
        entries,
    } = preview.result()
    else {
        return Err(PlanCheckFailure::Protocol(
            "work preview has another result family",
        ));
    };
    if actual != span
        || frontier != through
        || entries
            .iter()
            .flat_map(|entry| &entry.pairs)
            .any(|pair| pair.fate == s::PairFate::Opened)
    {
        return Err(PlanCheckFailure::Protocol(
            "work preview changed its frontier or retained an open pair",
        ));
    }
    Ok(s::WorkRef {
        plan: expected.clone(),
        step_id: step_id.clone(),
        span: span.clone(),
        through: through.clone(),
        content_sha256: s::Digest::new(format!("{:x}", Sha256::digest(preview.raw_bytes())))
            .expect("SHA-256 has the digest shape"),
    })
}

/// Only finalized proof reaches here, after any acquired snapshot was released.
/// The owner retains this proof and its measurement parent with the returned case.
pub(in crate::flow) fn read_refusal(
    context: &RecordingContext,
    expected: &s::ReadSelector,
    proof: &RecordedReadRefusal,
) -> Result<s::PlanOperationRefused, &'static str> {
    use s::{PlanOperationCase as Case, ReadRefusalCode as Code};
    if !proof.belongs_to(context) || proof.selector() != expected {
        return Err("plan operation refusal belongs to another read");
    }
    if !matches!(
        expected,
        s::ReadSelector::PlanCurrent {}
            | s::ReadSelector::Plan { .. }
            | s::ReadSelector::Marks {}
            | s::ReadSelector::Statement { .. }
            | s::ReadSelector::SpanPreview { .. }
    ) {
        return Err("plan operation refusal has an unlicensed selector");
    }
    let s::StateFrame::ControlAnswer {
        frame:
            s::StateControlAnswer::ControlRefused {
                code: s::ControlRefusalCode::Read(code),
                detail,
            },
        ..
    } = proof.answer()
    else {
        return Err("plan operation lacks a finalized read refusal");
    };
    let case = match proof.request() {
        s::StateControl::Read { selector } if selector == expected => match code {
            Code::CounterExhausted
            | Code::ResultTooLarge
            | Code::ResultUnavailable
            | Code::AnswerTooLarge => Case::ReadRefused,
            Code::NotFound
                if matches!(
                    expected,
                    s::ReadSelector::Plan { .. } | s::ReadSelector::Statement { .. }
                ) =>
            {
                Case::InvalidTarget
            }
            Code::InvalidSelector | Code::SpanPending | Code::PartialPair
                if matches!(expected, s::ReadSelector::SpanPreview { .. }) =>
            {
                Case::WorkChanged
            }
            _ => return Err("read refusal is impossible for this initial plan selector"),
        },
        s::StateControl::Next { .. }
            if matches!(
                code,
                Code::UnknownSnapshot
                    | Code::WrongOffset
                    | Code::ResultUnavailable
                    | Code::AnswerTooLarge
            ) =>
        {
            Case::ReadRefused
        }
        _ => return Err("read refusal is impossible for this plan read leg"),
    };
    Ok(s::PlanOperationRefused {
        case,
        detail: detail.clone(),
        read_code: Some(*code),
    })
}
