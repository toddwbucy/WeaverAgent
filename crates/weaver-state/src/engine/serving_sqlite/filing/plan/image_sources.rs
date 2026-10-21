//! Reverse provenance: every retained plan effect still has its durable outputs.

use super::*;

pub(super) fn verify_sources(db: &Connection, seed: &str) -> Result<(), FileFault> {
    let mut query = db.prepare(
        "SELECT run,sequence,block,effect FROM content_block WHERE effect IS NOT NULL ORDER BY id",
    )?;
    let mut rows = query.query([])?;
    let mut next_plan = Some(1u64);
    let mut next_step = Some(1u64);
    let mut authorizations = std::collections::BTreeMap::new();
    while let Some(row) = rows.next()? {
        let effect: String = row.get(3)?;
        let effect: StatementEffect =
            decode_strict(&effect).map_err(|_| invalid_effect("malformed held effect"))?;
        let StatementEffect::Plan(effect) = effect else {
            continue;
        };
        let source = SourceBlockId {
            run: row.get(0)?,
            sequence: decode_counter(row.get(1)?)?.into(),
            block: decode_counter(row.get(2)?)?.into(),
        };
        match effect.as_ref() {
            PlanEffect::Authorize {
                plan,
                source: operator,
                task,
                review,
            } => {
                let p = snapshot(db, seed, plan)?;
                if p.operator_source != *operator || p.task != *task || p.review != *review {
                    return Err(invalid_effect("authorization output is missing or changed"));
                }
                allocate_plan(plan, &mut next_plan)?;
                authorizations.insert(plan.plan_id.local_id.0, (plan.clone(), operator.clone()));
            }
            PlanEffect::Lock {
                plan,
                source: input,
                task,
                steps,
                review,
            } => {
                let p = snapshot(db, seed, plan)?;
                let operator = match input {
                    PlanLockSource::Operator { input } => {
                        allocate_plan(plan, &mut next_plan)?;
                        input
                    }
                    PlanLockSource::Model { authorization, .. } => {
                        if authorizations.remove(&plan.plan_id.local_id.0)
                            != Some((plan.clone(), authorization.clone()))
                        {
                            return Err(invalid_effect(
                                "model lock has no unconsumed earlier authorization",
                            ));
                        }
                        authorization
                    }
                };
                verify_locked_output(&p, &source, operator, task, steps, review)?;
                for step in steps {
                    allocate_step(&step.step_id, &mut next_step)?;
                }
            }
            PlanEffect::Replace {
                expected,
                next,
                source: operator,
                task,
                steps,
                review,
            } => {
                let old = snapshot(db, seed, expected)?;
                if old.status != PlanStatus::Superseded || old.last_source != source {
                    return Err(invalid_effect("replacement lost its retired predecessor"));
                }
                authorizations.remove(&expected.plan_id.local_id.0);
                let p = snapshot(db, seed, next)?;
                verify_locked_output(&p, &source, operator, task, steps, review)?;
                let id = source_id(db, operator)?
                    .ok_or_else(|| invalid_effect("replacement source is absent"))?;
                let context: String = db.query_row(
                    "SELECT context_raw FROM content_block WHERE id=?1",
                    [id],
                    |row| row.get(0),
                )?;
                let context: PlanInputContext = decode_strict(&context)
                    .map_err(|_| invalid_effect("invalid replacement source"))?;
                let draft = match context.operation {
                    PlanInputOperation::Replace(r)
                    | PlanInputOperation::AnswerBreak { replacement: r } => r,
                    _ => return Err(invalid_effect("replacement has another operator operation")),
                };
                if draft.steps.len() != steps.len() {
                    return Err(invalid_effect("replacement changed step count"));
                }
                for (draft, step) in draft.steps.iter().zip(steps) {
                    if draft.id.is_none() {
                        allocate_step(&step.step_id, &mut next_step)?;
                    }
                }
            }
            PlanEffect::Activate {
                expected,
                step_id,
                step_span,
                group_span,
                successor,
            } => {
                let p = snapshot(db, seed, expected)?;
                let step = held_step(&p, step_id)?;
                if step.span.as_ref() != Some(step_span)
                    || step.status == StepStatus::Waiting
                    || successor.plan != *expected
                    || successor.step_id != *step_id
                    || successor.instruction != step.instruction
                {
                    return Err(invalid_effect("activation output is missing or changed"));
                }
                let group = step
                    .group
                    .as_ref()
                    .and_then(|id| p.groups.iter().find(|g| g.id == *id))
                    .and_then(|g| g.span.as_ref());
                if group != group_span.as_ref() {
                    return Err(invalid_effect(
                        "activation group output is missing or changed",
                    ));
                }
                let prefix:Option<String>=db.query_row("SELECT step_query FROM span_prefix WHERE local_id=?1 AND revision=?2 AND ordinal=?3",params![blob(step_span.local_id.0),blob(step_span.revision.0),blob(0)],|row|row.get(0)).optional()?.flatten();
                if prefix
                    .as_deref()
                    .map(decode_strict::<StepQueryRef>)
                    .transpose()
                    .map_err(|_| invalid_effect("invalid activation prefix"))?
                    .as_ref()
                    != Some(successor)
                {
                    return Err(invalid_effect("activation prefix changed"));
                }
            }
            PlanEffect::Close {
                expected,
                step_id,
                review_call,
                work,
                outcome_span,
            } => {
                let p = snapshot(db, seed, expected)?;
                let step = held_step(&p, step_id)?;
                if step.status != StepStatus::Closed
                    || step.closure_source.as_ref() != Some(&source)
                    || step.outcome_span.as_ref() != Some(outcome_span)
                    || step.span.as_ref() != Some(&work.span)
                    || step.review_call != *review_call
                    || step.work.as_ref() != Some(work)
                {
                    return Err(invalid_effect("closure output is missing or changed"));
                }
            }
            PlanEffect::Break {
                expected,
                step_id,
                case,
                proposed_source,
            } => {
                let p = snapshot(db, seed, expected)?;
                let record = PlanBreakRecord {
                    case: *case,
                    step_id: step_id.clone(),
                    proposed_source: proposed_source.clone(),
                    source,
                };
                if !matches!(p.status, PlanStatus::Broken | PlanStatus::Superseded)
                    || p.broken.as_ref() != Some(&record)
                {
                    return Err(invalid_effect("break output is missing or changed"));
                }
            }
        }
    }
    Ok(())
}

fn allocate_step(id: &StepId, next: &mut Option<u64>) -> Result<(), FileFault> {
    if *next != Some(id.local_id.0) {
        return Err(invalid_effect(
            "new step reused or skipped a historical allocation",
        ));
    }
    *next = id.local_id.0.checked_add(1);
    Ok(())
}

fn allocate_plan(plan: &PlanRef, next: &mut Option<u64>) -> Result<(), FileFault> {
    if plan.revision.0 != 1 || *next != Some(plan.plan_id.local_id.0) {
        return Err(invalid_effect(
            "new plan reused or skipped a historical allocation",
        ));
    }
    *next = plan.plan_id.local_id.0.checked_add(1);
    Ok(())
}

fn snapshot(db: &Connection, seed: &str, plan: &PlanRef) -> Result<PlanSnapshot, FileFault> {
    reads::plan_snapshot(db, seed, plan, u64::MAX.into()).map_err(read_fault)
}

fn held_step<'a>(p: &'a PlanSnapshot, id: &StepId) -> Result<&'a PlanStep, FileFault> {
    p.steps
        .iter()
        .find(|step| step.step_id == *id)
        .ok_or_else(|| invalid_effect("plan effect has no retained step"))
}

fn verify_locked_output(
    p: &PlanSnapshot,
    source: &SourceBlockId,
    operator: &SourceBlockId,
    task: &TaskScopeRef,
    steps: &[PlanStepInit],
    review: &ReviewSetupRef,
) -> Result<(), FileFault> {
    if p.lock_source.as_ref() != Some(source)
        || p.operator_source != *operator
        || p.task != *task
        || p.review != *review
        || steps.len() != p.steps.len()
        || steps.iter().zip(&p.steps).any(|(initial, held)| {
            initial.step_id != held.step_id || initial.instruction != held.instruction
        })
    {
        return Err(invalid_effect("lock output is missing or changed"));
    }
    Ok(())
}
