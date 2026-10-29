//! Break validates the held plan and active step, without choosing a recovery.
use super::effect::PreparedPortEffect;
use super::*;
use crate::flow::clock::AbsoluteBound;
use std::sync::Arc;
use weaver_trace::TurnRef;

pub(in crate::flow) fn prepare(
    request: &PlanRequest,
    context: &RecordingContext,
    current: &CommittedPlanSummary,
    turn: Option<TurnRef>,
    bound: AbsoluteBound,
) -> Result<Option<PreparedPortEffect>, PlanCheckFailure> {
    let PlanRequest::RecordBreak {
        expected,
        step_id,
        case,
        proposed_source,
    } = request
    else {
        return Err(PlanCheckFailure::Protocol("break has another request"));
    };
    let plan = match locked(request, context, current) {
        Ok(plan) => plan,
        Err(PlanCheckFailure::Refused(refusal))
            if refusal.case == s::PlanOperationCase::NoLockedPlan =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if plan
        .steps
        .iter()
        .find(|step| step.status == s::StepStatus::Active)
        .map(|step| &step.step_id)
        != step_id.as_ref()
    {
        return Err(refuse(
            s::PlanOperationCase::InvalidTarget,
            "break does not name the active step",
        ));
    }
    if turn.as_ref().is_none_or(|turn| turn.0.is_empty()) {
        return Err(PlanCheckFailure::Protocol(
            "break effect has no admitted turn",
        ));
    }
    Ok(Some(PreparedPortEffect {
        origin: Arc::clone(current.read().origin()),
        effect: s::PlanEffect::Break {
            expected: expected.clone(),
            step_id: step_id.clone(),
            case: *case,
            proposed_source: proposed_source.clone(),
        },
        parent: current.read().recall_sequence(),
        turn,
        bound,
    }))
}
