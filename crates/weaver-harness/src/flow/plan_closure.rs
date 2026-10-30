//! Unreviewed closure consumes moved checked work and a private grant. Neither
//! public WorkRef metadata nor model text can construct the evidence it needs.
use super::activation::SpanAllocator;
use super::effect::PreparedPortEffect;
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::plan_port_driver::HeldOutcome;
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

pub(super) fn open_pair(entries: &[s::ReadEntry]) -> bool {
    entries
        .iter()
        .flat_map(|entry| &entry.pairs)
        .any(|pair| pair.fate == s::PairFate::Opened)
}

/// CLOSURE-EVIDENCE's attest_closure. It reads nothing: the committed cache
/// supplies the plan and category, and the moved CheckedWork its released bytes.
pub(in crate::flow) fn attest(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
) -> Result<s::ReviewSetupRef, PlanCheckFailure> {
    let (plan, step) = active(request, context, current)?;
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

/// close's checks against the committed cache, before and again after its
/// work reread. Correct refusals here leave the plan and cache unchanged.
pub(in crate::flow) fn validate_close<'a>(
    request: &PlanRequest,
    held: Option<&HeldOutcome>,
    context: &RecordingContext,
    current: &'a CommittedPlanSummary,
) -> Result<(&'a s::PlanSnapshot, &'a s::PlanStep), PlanCheckFailure> {
    let (plan, step) = active(request, context, current)?;
    let PlanRequest::Close {
        evidence,
        outcome_span,
        ..
    } = request
    else {
        return Err(PlanCheckFailure::Protocol("close received another request"));
    };
    // The owner's held outcome_span read, from this run and for this plan
    // revision. The loop cannot supply one, so its absence is a refusal.
    let Some(held) =
        held.filter(|held| held.belongs_to(context) && held.expected() == request.expected())
    else {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "close has no held outcome_span read for this run and plan revision",
        ));
    };
    if !evidence.belongs_to(context) {
        return Err(PlanCheckFailure::Protocol(
            "close has foreign closure evidence",
        ));
    }
    if evidence.plan() != &plan.plan || evidence.step_id() != &step.step_id {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "closure evidence names another plan revision or step",
        ));
    }
    held_target(evidence.work(), plan, step)?;
    // The caller's span must be an allocation from its held catalog. Its
    // local_id stays the store's: a span allocated since the read is a race
    // the store guards, after the effect is admitted.
    let held = SpanAllocator::from_marks(context, held.read(), &plan.plan.plan_id.seed_id)?.peek();
    if outcome_span.seed_id != held.seed_id || outcome_span.revision != held.revision {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "outcome span is not an allocation from its held marks catalog",
        ));
    }
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

/// The reread must return the exact held bytes. A successful preview always
/// echoes its selector and holds only terminal pairs (Spec 06 MEMBER), so a
/// changed echo or an open pair is a member protocol fault. Changed bytes for
/// the same selector are PLAN-PORT-REFUSAL's local work_changed with null code.
pub(in crate::flow) fn prepare_close(
    request: &PlanRequest,
    held: Option<&HeldOutcome>,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
    preview: &RecordedRead,
    turn: TurnRef,
    bound: AbsoluteBound,
) -> Result<PreparedPortEffect, PlanCheckFailure> {
    validate_close(request, held, context, current)?;
    let held = held.expect("validated close holds its outcome read");
    let PlanRequest::Close {
        expected,
        step_id,
        evidence,
        outcome_span,
    } = request
    else {
        unreachable!()
    };
    // The reread follows every proof close consumed: the committed cache,
    // the attestation (and through it the checked work) and the held marks.
    if !preview.belongs_to(context)
        || preview.selector() != &request.selector()
        || preview.recall_sequence().0 <= current.read().recall_sequence().0
        || preview.recall_sequence().0 <= evidence.sequence().0
        || preview.recall_sequence().0 <= held.sequence().0
        || turn.0.is_empty()
    {
        return Err(PlanCheckFailure::Protocol(
            "close lost its ordered work reread or turn",
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
    Ok(PreparedPortEffect {
        origin: Arc::clone(preview.origin()),
        effect: s::PlanEffect::Close {
            expected: expected.clone(),
            step_id: step_id.clone(),
            review_call: evidence.review_call().cloned(),
            work: evidence.work().clone(),
            outcome_span: outcome_span.clone(),
        },
        parent: preview.recall_sequence(),
        turn: Some(turn),
        bound,
    })
}
