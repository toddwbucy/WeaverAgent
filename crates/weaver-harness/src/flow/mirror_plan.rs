//! Exhaustive conversion of root input metadata into the serialize-only mirror.
use super::*;

pub(in crate::flow) fn execution_effect(
    value: &shared::PlanEffect,
) -> Result<trace::PlanEffect, &'static str> {
    if let shared::PlanEffect::Break {
        expected,
        step_id: step,
        case,
        proposed_source,
    } = value
    {
        return Ok(trace::PlanEffect::Break {
            expected: plan_ref(expected),
            step_id: step.as_ref().map(step_id),
            case: plan_break_case(*case),
            proposed_source: proposed_source.as_ref().map(source),
        });
    }
    if let shared::PlanEffect::Close {
        expected,
        step_id: step,
        review_call,
        work,
        outcome_span,
    } = value
    {
        return Ok(trace::PlanEffect::Close {
            expected: plan_ref(expected),
            step_id: step_id(step),
            review_call: review_call.as_ref().map(call_id),
            work: work_ref(work),
            outcome_span: span_ref(outcome_span),
        });
    }
    let shared::PlanEffect::Activate {
        expected,
        step_id: step,
        step_span,
        group_span,
        successor,
    } = value
    else {
        return Err("execution grant has another effect");
    };
    Ok(trace::PlanEffect::Activate {
        expected: plan_ref(expected),
        step_id: step_id(step),
        step_span: span_ref(step_span),
        group_span: group_span.as_ref().map(span_ref),
        successor: trace::StepQueryRef {
            plan: plan_ref(&successor.plan),
            previous_step: successor.previous_step.as_ref().map(step_id),
            step_id: step_id(&successor.step_id),
            instruction: statement(&successor.instruction),
        },
    })
}

/// PLAN-TRACE's closed step.outcome. Its outer identities are copied from
/// the exact close effect, so the record cannot name another step or work.
pub(in crate::flow) fn closed_outcome(
    value: &shared::PlanEffect,
    message: Box<serde_json::value::RawValue>,
    template_id: String,
) -> Result<trace::StepOutcomeRecord, &'static str> {
    let shared::PlanEffect::Close {
        expected,
        step_id: step,
        review_call,
        work,
        ..
    } = value
    else {
        return Err("closed outcome requires its close effect");
    };
    Ok(trace::StepOutcomeRecord {
        message,
        template_id,
        context: Some(execution_effect(value)?),
        plan: plan_ref(expected),
        step_id: step_id(step),
        review_call: review_call.as_ref().map(call_id),
        work: work_ref(work),
        outcome: trace::ReviewOutcome::Closed {},
    })
}

pub(in crate::flow) fn root_plan_effect(
    value: &shared::PlanEffect,
) -> Result<trace::PlanEffect, &'static str> {
    let task = |value: &shared::TaskScopeRef| trace::TaskScopeRef {
        task: span_ref(&value.task),
        prompt: span_ref(&value.prompt),
    };
    let steps = |values: &[shared::PlanStepInit]| {
        values
            .iter()
            .map(|value| trace::PlanStepInit {
                step_id: step_id(&value.step_id),
                instruction: statement(&value.instruction),
            })
            .collect()
    };
    Ok(match value {
        shared::PlanEffect::Authorize {
            plan,
            source: input,
            task: scope,
            review,
        } => trace::PlanEffect::Authorize {
            plan: plan_ref(plan),
            source: source(input),
            task: task(scope),
            review: setup(review),
        },
        shared::PlanEffect::Lock {
            plan,
            source: shared::PlanLockSource::Operator { input },
            task: scope,
            steps: list,
            review,
        } => trace::PlanEffect::Lock {
            plan: plan_ref(plan),
            source: trace::PlanLockSource::Operator {
                input: source(input),
            },
            task: task(scope),
            steps: steps(list),
            review: setup(review),
        },
        shared::PlanEffect::Replace {
            expected,
            next,
            source: input,
            task: scope,
            steps: list,
            review,
        } => trace::PlanEffect::Replace {
            expected: plan_ref(expected),
            next: plan_ref(next),
            source: source(input),
            task: task(scope),
            steps: steps(list),
            review: setup(review),
        },
        shared::PlanEffect::Lock {
            source: shared::PlanLockSource::Model { .. },
            ..
        }
        | shared::PlanEffect::Activate { .. }
        | shared::PlanEffect::Close { .. }
        | shared::PlanEffect::Break { .. } => {
            return Err("root effect cannot author a model or execution effect");
        }
    })
}

pub(in crate::flow) fn plan_input(value: &shared::PlanInputContext) -> trace::PlanInputContext {
    let shared::PlanInputContext {
        document_sha256,
        cause,
        expected,
        operation,
    } = value;
    trace::PlanInputContext {
        document_sha256: document_sha256.as_str().into(),
        cause: weaver_trace::Cause { uid: cause.uid },
        expected: expected.as_ref().map(plan_ref),
        operation: match operation {
            shared::PlanInputOperation::Create {
                task,
                steps,
                categories,
                groups,
                review,
            } => trace::PlanInputOperation::Create {
                task: number(*task),
                steps: steps.iter().map(step).collect(),
                categories: categories.iter().map(category).collect(),
                groups: groups.iter().map(group).collect(),
                review: setup(review),
            },
            shared::PlanInputOperation::Authorize {
                task,
                categories,
                groups,
                review,
            } => trace::PlanInputOperation::Authorize {
                task: number(*task),
                categories: categories.iter().map(category).collect(),
                groups: groups.iter().map(group).collect(),
                review: setup(review),
            },
            shared::PlanInputOperation::Replace(value) => {
                trace::PlanInputOperation::Replace(replacement(value))
            }
            shared::PlanInputOperation::AnswerBreak { replacement: value } => {
                trace::PlanInputOperation::AnswerBreak {
                    replacement: replacement(value),
                }
            }
        },
    }
}
pub(in crate::flow) fn plan_ref(value: &shared::PlanRef) -> trace::PlanRef {
    let shared::PlanRef {
        plan_id: shared::PlanId { seed_id, local_id },
        revision,
    } = value;
    trace::PlanRef {
        plan_id: trace::PlanId {
            seed_id: seed_id.clone(),
            local_id: number(*local_id),
        },
        revision: number(*revision),
    }
}
pub(in crate::flow) fn plan_flags(value: &shared::PlanFlagSnapshot) -> trace::PlanFlagSnapshot {
    let shared::PlanFlagSnapshot {
        plan,
        after,
        through,
        total_flags,
        matched_flags,
        unmatched_flags,
    } = value;
    trace::PlanFlagSnapshot {
        plan: plan_ref(plan),
        after: source(after),
        through: source_position(through),
        total_flags: number(*total_flags),
        matched_flags: number(*matched_flags),
        unmatched_flags: number(*unmatched_flags),
    }
}
pub(in crate::flow) fn read_refusal(value: shared::ReadRefusalCode) -> trace::ReadRefusalCode {
    use shared::ReadRefusalCode as S;
    use trace::ReadRefusalCode as T;
    match value {
        S::UnsupportedOperation => T::UnsupportedOperation,
        S::InvalidSelector => T::InvalidSelector,
        S::WrongSeed => T::WrongSeed,
        S::NotFound => T::NotFound,
        S::UnknownSpan => T::UnknownSpan,
        S::SpanOpen => T::SpanOpen,
        S::SpanPending => T::SpanPending,
        S::PartialPair => T::PartialPair,
        S::CounterExhausted => T::CounterExhausted,
        S::ReadBusy => T::ReadBusy,
        S::UnknownSnapshot => T::UnknownSnapshot,
        S::WrongOffset => T::WrongOffset,
        S::ResultTooLarge => T::ResultTooLarge,
        S::ResultUnavailable => T::ResultUnavailable,
        S::AnswerTooLarge => T::AnswerTooLarge,
    }
}
fn step_id(value: &shared::StepId) -> trace::StepId {
    let shared::StepId { seed_id, local_id } = value;
    trace::StepId {
        seed_id: seed_id.clone(),
        local_id: number(*local_id),
    }
}
fn step(value: &shared::PlanInputStep) -> trace::PlanInputStep {
    let shared::PlanInputStep { id, block } = value;
    trace::PlanInputStep {
        id: id.as_ref().map(step_id),
        block: number(*block),
    }
}
fn setup(value: &shared::ReviewSetupRef) -> trace::ReviewSetupRef {
    let shared::ReviewSetupRef { setup_id, sha256 } = value;
    trace::ReviewSetupRef {
        setup_id: setup_id.clone(),
        sha256: sha256.as_str().into(),
    }
}
fn category(value: &shared::Category) -> trace::Category {
    let shared::Category {
        id,
        verbs,
        acts,
        review_required,
    } = value;
    trace::Category {
        id: id.clone(),
        verbs: verbs.clone(),
        review_required: *review_required,
        acts: acts
            .iter()
            .map(|act| match act {
                shared::PlanAct::Statement => trace::PlanAct::Statement,
                shared::PlanAct::ExternalTool { name } => {
                    trace::PlanAct::ExternalTool { name: name.clone() }
                }
                shared::PlanAct::InwardRecall { verb } => {
                    trace::PlanAct::InwardRecall { verb: verb.clone() }
                }
            })
            .collect(),
    }
}
fn group(value: &shared::GroupDraft) -> trace::GroupDraft {
    let shared::GroupDraft { id, step_indices } = value;
    trace::GroupDraft {
        id: id.clone(),
        step_indices: step_indices.iter().copied().map(number).collect(),
    }
}
fn replacement(value: &shared::PlanInputReplacement) -> trace::PlanInputReplacement {
    let shared::PlanInputReplacement {
        task,
        steps,
        categories,
        groups,
        review,
        preserved_closures,
    } = value;
    trace::PlanInputReplacement {
        task: task.map(number),
        steps: steps.iter().map(step).collect(),
        categories: categories.iter().map(category).collect(),
        groups: groups.iter().map(group).collect(),
        review: setup(review),
        preserved_closures: preserved_closures.iter().map(step_id).collect(),
    }
}

fn plan_break_case(value: shared::PlanBreakCase) -> trace::PlanBreakCase {
    match value {
        shared::PlanBreakCase::NoLockedPlan => trace::PlanBreakCase::NoLockedPlan,
        shared::PlanBreakCase::WrongCategory => trace::PlanBreakCase::WrongCategory,
        shared::PlanBreakCase::UnknownTool => trace::PlanBreakCase::UnknownTool,
        shared::PlanBreakCase::SelfEdit => trace::PlanBreakCase::SelfEdit,
        shared::PlanBreakCase::InvalidTarget => trace::PlanBreakCase::InvalidTarget,
        shared::PlanBreakCase::WorkChanged => trace::PlanBreakCase::WorkChanged,
        shared::PlanBreakCase::ReviewSetupChanged => trace::PlanBreakCase::ReviewSetupChanged,
        shared::PlanBreakCase::ReviewRequired => trace::PlanBreakCase::ReviewRequired,
    }
}

pub(in crate::flow) fn plan_break(value: &shared::PlanBreakReport) -> trace::PlanBreakReport {
    let shared::PlanBreakReport {
        plan,
        step_id: step,
        proposed_source,
        case,
    } = value;
    trace::PlanBreakReport {
        plan: plan.as_ref().map(plan_ref),
        step_id: step.as_ref().map(step_id),
        proposed_source: proposed_source.as_ref().map(source),
        case: plan_break_case(*case),
    }
}
