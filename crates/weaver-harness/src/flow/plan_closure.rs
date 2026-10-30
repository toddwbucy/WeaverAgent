//! Unreviewed closure consumes moved checked work and a private grant. Neither
//! public WorkRef metadata nor model text can construct the evidence it needs.
use super::activation::SpanAllocator;
use super::effect::PreparedPortEffect;
use super::*;
use crate::flow::clock::AbsoluteBound;
use sha2::{Digest as _, Sha256};
use std::sync::Arc;
use weaver_trace::TurnRef;

pub(super) fn category<'a>(
    plan: &'a s::PlanSnapshot,
    step: &s::PlanStep,
) -> Result<&'a s::Category, PlanCheckFailure> {
    plan.categories
        .iter()
        .find(|category| category.id == step.category)
        .ok_or(PlanCheckFailure::Protocol("active step category is absent"))
}

/// The held WorkRef must name the current revision, the active step and its
/// span. A reference to another target is not changed work for this one.
fn held_target(
    work: &s::WorkRef,
    plan: &s::PlanSnapshot,
    step: &s::PlanStep,
) -> Result<(), PlanCheckFailure> {
    if work.plan != plan.plan
        || work.step_id != step.step_id
        || step.span.as_ref() != Some(&work.span)
    {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "held work names another plan revision, step or span",
        ));
    }
    Ok(())
}

pub(super) fn digest(bytes: &[u8]) -> s::Digest {
    s::Digest::new(format!("{:x}", Sha256::digest(bytes))).expect("SHA-256 has the digest shape")
}

/// Defence after read validation, which already faults a successful preview
/// that retains an Opened pair (the open-pair perturbation never bites).
pub(super) fn open_pair(entries: &[s::ReadEntry]) -> bool {
    entries
        .iter()
        .flat_map(|entry| &entry.pairs)
        .any(|pair| pair.fate == s::PairFate::Opened)
}

/// CLOSURE-EVIDENCE's attest_closure. It reads nothing: the committed cache
/// supplies the plan and category, and the moved CheckedWork its released bytes.
/// Run-binding and proof-consistency faults come before any ordinary refusal.
pub(in crate::flow) fn attest(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
) -> Result<s::ReviewSetupRef, PlanCheckFailure> {
    let PlanRequest::AttestClosure { work, .. } = request else {
        return Err(PlanCheckFailure::Protocol(
            "attestation received another request",
        ));
    };
    if !work.belongs_to(context) {
        return Err(PlanCheckFailure::Protocol(
            "attestation has foreign checked work",
        ));
    }
    let reference = work.reference();
    let preview = work.preview();
    let s::QueryResult::SpanPreview {
        span,
        through,
        entries,
    } = preview.result()
    else {
        return Err(PlanCheckFailure::Protocol(
            "checked work lost its preview result",
        ));
    };
    if span != &reference.span
        || through != &reference.through
        || digest(preview.raw_bytes()) != reference.content_sha256
    {
        return Err(PlanCheckFailure::Protocol(
            "checked work differs from its released preview",
        ));
    }
    let (plan, step) = active(request, context, current)?;
    held_target(reference, plan, step)?;
    if open_pair(entries) {
        return Err(refuse(
            s::PlanOperationCase::WorkChanged,
            "held work retains an open pair",
        ));
    }
    if category(plan, step)?.review_required {
        return Err(refuse(
            s::PlanOperationCase::ReviewRequired,
            "the step category requires review before closure",
        ));
    }
    Ok(plan.review.clone())
}

/// close's checks against the committed cache, before its reads. Foreign
/// evidence is a fault before any ordinary refusal. Correct refusals here
/// leave the plan and cache unchanged.
pub(in crate::flow) fn validate_close<'a>(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &'a CommittedPlanSummary,
) -> Result<(&'a s::PlanSnapshot, &'a s::PlanStep), PlanCheckFailure> {
    let PlanRequest::Close { evidence, .. } = request else {
        return Err(PlanCheckFailure::Protocol("close received another request"));
    };
    if !evidence.belongs_to(context) {
        return Err(PlanCheckFailure::Protocol(
            "close has foreign closure evidence",
        ));
    }
    let (plan, step) = active(request, context, current)?;
    if evidence.plan() != &plan.plan || evidence.step_id() != &step.step_id {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "closure evidence names another plan revision or step",
        ));
    }
    // Defence: attest checked the same target, and a step's span or category
    // changes only with a new revision, which the check above refuses first.
    held_target(evidence.work(), plan, step)?;
    if evidence.setup() != &plan.review {
        return Err(refuse(
            s::PlanOperationCase::ReviewSetupChanged,
            "closure evidence names another review setup",
        ));
    }
    if evidence.review_call().is_none() && category(plan, step)?.review_required {
        return Err(refuse(
            s::PlanOperationCase::ReviewRequired,
            "the step category requires review before closure",
        ));
    }
    Ok((plan, step))
}

/// The reread must return the exact held bytes, after every proof close
/// consumed. A successful preview always echoes its selector and holds only
/// terminal pairs (Spec 06 MEMBER), so a changed echo or an open pair is a
/// member protocol fault. Changed bytes for the same selector are
/// PLAN-PORT-REFUSAL's local work_changed with null code.
pub(in crate::flow) fn check_reread(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
    preview: &RecordedRead,
) -> Result<(), PlanCheckFailure> {
    let PlanRequest::Close { evidence, .. } = request else {
        return Err(PlanCheckFailure::Protocol(
            "reread received another request",
        ));
    };
    if !preview.belongs_to(context)
        || preview.selector() != &request.selector()
        || preview.recall_sequence().0 <= current.read().recall_sequence().0
        || preview.recall_sequence().0 <= evidence.sequence().0
    {
        return Err(PlanCheckFailure::Protocol(
            "close lost its ordered work reread",
        ));
    }
    let s::QueryResult::SpanPreview {
        span,
        through,
        entries,
    } = preview.result()
    else {
        return Err(PlanCheckFailure::Protocol(
            "work reread has another result family",
        ));
    };
    if span != &evidence.work().span || through != &evidence.work().through || open_pair(entries) {
        return Err(PlanCheckFailure::Protocol(
            "work reread changed its frontier or retained an open pair",
        ));
    }
    if digest(preview.raw_bytes()) != evidence.work().content_sha256 {
        return Err(refuse(
            s::PlanOperationCase::WorkChanged,
            "held work bytes changed before closure",
        ));
    }
    Ok(())
}

/// close's own Marks read follows its checked reread. outcome_span must be
/// that catalog's next allocation exactly, as activation's step span is, so
/// only an allocation between this read and the filing reaches the store.
pub(in crate::flow) fn prepare_close(
    request: &PlanRequest,
    context: &RecordingContext,
    preview: &RecordedRead,
    marks: &RecordedRead,
    turn: TurnRef,
    bound: AbsoluteBound,
) -> Result<PreparedPortEffect, PlanCheckFailure> {
    let PlanRequest::Close {
        expected,
        step_id,
        evidence,
        outcome_span,
    } = request
    else {
        return Err(PlanCheckFailure::Protocol("close received another request"));
    };
    if marks.recall_sequence().0 <= preview.recall_sequence().0 || turn.0.is_empty() {
        return Err(PlanCheckFailure::Protocol(
            "close lost its ordered marks read or turn",
        ));
    }
    let next = SpanAllocator::from_marks(context, marks, &expected.plan_id.seed_id)?.peek();
    if outcome_span != &next {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "outcome span is not the next allocation in close's marks read",
        ));
    }
    Ok(PreparedPortEffect {
        origin: Arc::clone(marks.origin()),
        effect: s::PlanEffect::Close {
            expected: expected.clone(),
            step_id: step_id.clone(),
            review_call: evidence.review_call().cloned(),
            work: evidence.work().clone(),
            outcome_span: outcome_span.clone(),
        },
        parent: marks.recall_sequence(),
        turn: Some(turn),
        bound,
    })
}
