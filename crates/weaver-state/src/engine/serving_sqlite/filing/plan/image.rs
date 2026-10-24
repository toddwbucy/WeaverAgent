//! Read-only plan image verification. The deployment's read ceiling is not an
//! image validity limit. No image is repaired during adoption.

use super::*;
use sha2::{Digest as _, Sha256};

pub(in crate::engine::serving_sqlite) fn verify_image_plan(
    db: &Connection,
    seed: &str,
    _limits: &ReadLimits,
) -> Result<(), FileFault> {
    let mut counters = db.prepare("SELECT DISTINCT step_id FROM plan_step ORDER BY step_id")?;
    let mut counters = counters.query([])?;
    let mut expected = Some(1u64);
    while let Some(row) = counters.next()? {
        let actual = decode_counter(row.get(0)?)?;
        if expected != Some(actual) {
            return Err(invalid_effect("step history skips an allocated identity"));
        }
        expected = actual.checked_add(1);
    }
    let mut statement =
        db.prepare("SELECT plan_id,revision FROM plan_revision ORDER BY plan_id,revision")?;
    let mut rows = statement.query([])?;
    let mut previous: Option<PlanRef> = None;
    while let Some(row) = rows.next()? {
        let plan = PlanRef {
            plan_id: PlanId {
                seed_id: seed.into(),
                local_id: decode_counter(row.get(0)?)?.into(),
            },
            revision: decode_counter(row.get(1)?)?.into(),
        };
        let allocated = match &previous {
            None => plan.plan_id.local_id.0 == 1 && plan.revision.0 == 1,
            Some(old) if old.plan_id == plan.plan_id => {
                old.revision.0.checked_add(1) == Some(plan.revision.0)
            }
            Some(old) => {
                old.plan_id.local_id.0.checked_add(1) == Some(plan.plan_id.local_id.0)
                    && plan.revision.0 == 1
            }
        };
        if !allocated {
            return Err(invalid_effect("plan history skips an allocated identity"));
        }
        let snapshot = reads::plan_snapshot(db, seed, &plan).map_err(read_fault)?;
        verify_revision(db, seed, &snapshot)?;
        previous = Some(plan);
    }
    super::image_sources::verify_sources(db, seed)
}

fn held_id(db: &Connection, source: &SourceBlockId) -> Result<i64, FileFault> {
    source_id(db, source)?.ok_or_else(|| invalid_effect("plan image references an absent source"))
}

fn effect_at(db: &Connection, source: &SourceBlockId) -> Result<PlanEffect, FileFault> {
    let id = held_id(db, source)?;
    let raw: Option<String> = db.query_row(
        "SELECT effect FROM content_block WHERE id=?1",
        [id],
        |row| row.get(0),
    )?;
    match decode_strict(&raw.ok_or_else(|| invalid_effect("plan source has no effect"))?)
        .map_err(|_| invalid_effect("invalid plan source effect"))?
    {
        StatementEffect::Plan(effect) => Ok(*effect),
        _ => Err(invalid_effect("plan row names another effect type")),
    }
}

type SpanFact = (String, String, Option<Vec<u8>>, Option<Vec<u8>>);

fn span_fact(db: &Connection, span: &SpanRef, seed: &str) -> Result<SpanFact, FileFault> {
    if span.seed_id != seed || span.revision.0 != 1 || span.local_id.0 == 0 {
        return Err(invalid_effect("invalid plan span identity"));
    }
    db.query_row("SELECT kind,status,parent_local_id,parent_revision FROM span WHERE local_id=?1 AND revision=?2",params![blob(span.local_id.0),blob(span.revision.0)],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?.ok_or_else(||invalid_effect("missing plan span"))
}

fn verify_revision(db: &Connection, seed: &str, p: &PlanSnapshot) -> Result<(), FileFault> {
    if p.plan.plan_id.local_id.0 == 0 || p.plan.revision.0 == 0 || p.review.setup_id.is_empty() {
        return Err(invalid_effect("invalid stored plan identity"));
    }
    super::revision::validate_categories(&p.categories)?;
    super::revision::validate_groups(
        &p.group_drafts,
        if p.lock_source.is_none() {
            None
        } else {
            Some(p.steps.len())
        },
    )?;
    let operator = held_id(db, &p.operator_source)?;
    // The shared snapshot read derives the digest and transition position from
    // these same held rows. Validate source authority here, not a tautological
    // comparison of a freshly derived carrier with its own inputs.
    let input = reads::operator_context(db, operator).map_err(read_fault)?;
    verify_input(db, &p.operator_source, &input)?;
    let (categories, groups, review, indices) = match &input.operation {
        PlanInputOperation::Create {
            categories,
            groups,
            review,
            steps,
            ..
        } => (categories, groups, review, Some(steps)),
        PlanInputOperation::Authorize {
            categories,
            groups,
            review,
            ..
        } => (categories, groups, review, None),
        PlanInputOperation::Replace(r) | PlanInputOperation::AnswerBreak { replacement: r } => {
            (&r.categories, &r.groups, &r.review, Some(&r.steps))
        }
    };
    if *categories != p.categories || *groups != p.group_drafts || *review != p.review {
        return Err(invalid_effect(
            "stored plan changed its operator declaration",
        ));
    }
    if p.plan.revision.0 == 1 {
        if input.expected.is_some()
            || !matches!(
                input.operation,
                PlanInputOperation::Create { .. } | PlanInputOperation::Authorize { .. }
            )
        {
            return Err(invalid_effect("initial plan has replacement provenance"));
        }
    } else if input.expected.as_ref()
        != Some(&PlanRef {
            plan_id: p.plan.plan_id.clone(),
            revision: (p.plan.revision.0 - 1).into(),
        })
    {
        return Err(invalid_effect(
            "replacement does not name preceding revision",
        ));
    }
    let (kind, task_status, parent, _) = span_fact(db, &p.task.task, seed)?;
    let (prompt_kind, prompt_status, prompt_parent, prompt_parent_revision) =
        span_fact(db, &p.task.prompt, seed)?;
    if kind != "task"
        || parent.is_some()
        || prompt_kind != "task_prompt"
        || prompt_status != "closed"
        || prompt_parent.as_deref() != Some(blob(p.task.task.local_id.0).as_slice())
        || prompt_parent_revision.as_deref() != Some(blob(p.task.task.revision.0).as_slice())
    {
        return Err(invalid_effect("stored task scope is not its prompt parent"));
    }
    let mut initial_steps = Vec::new();
    if let Some(lock) = &p.lock_source {
        if held_id(db, lock)? <= operator {
            return Err(invalid_effect("plan lock precedes its input"));
        }
        let task = match effect_at(db, lock)? {
            PlanEffect::Lock {
                plan,
                task,
                steps,
                review,
                source,
            } if plan == p.plan && review == p.review => {
                match source {
                    PlanLockSource::Operator { input: source }
                        if source == p.operator_source
                            && matches!(input.operation, PlanInputOperation::Create { .. }) => {}
                    PlanLockSource::Model {
                        authorization,
                        candidate,
                    } if authorization == p.operator_source
                        && matches!(input.operation, PlanInputOperation::Authorize { .. })
                        && candidate
                            == steps
                                .iter()
                                .map(|s| s.instruction.clone())
                                .collect::<Vec<_>>() =>
                    {
                        for (index, reference) in candidate.iter().enumerate() {
                            if candidate[..index].contains(reference) {
                                return Err(invalid_effect("duplicate stored model candidate"));
                            }
                        }
                    }
                    _ => return Err(invalid_effect("stored lock changed its authorization")),
                }
                initial_steps = steps;
                task
            }
            PlanEffect::Replace {
                next,
                task,
                steps,
                review,
                expected,
                source,
            } if next == p.plan
                && review == p.review
                && source == p.operator_source
                && Some(&expected) == input.expected.as_ref() =>
            {
                initial_steps = steps;
                task
            }
            _ => {
                return Err(invalid_effect(
                    "stored lock source does not create this revision",
                ));
            }
        };
        if task != p.task || initial_steps.len() != p.steps.len() || p.steps.is_empty() {
            return Err(invalid_effect("stored plan differs from its lock"));
        }
    } else if !p.steps.is_empty()
        || !p.groups.is_empty()
        || !matches!(p.status, PlanStatus::Authorized | PlanStatus::Superseded)
    {
        return Err(invalid_effect("unlocked revision contains execution rows"));
    }
    if indices.is_some_and(|indices| indices.len() != p.steps.len()) {
        return Err(invalid_effect("operator and stored step counts differ"));
    }
    verify_operator_steps(db, seed, p, &input)?;
    let last = held_id(db, &p.last_source.block_id)?;
    if last <= operator
        || p.lock_source
            .as_ref()
            .map(|source| held_id(db, source))
            .transpose()?
            .is_some_and(|lock| last < lock)
    {
        return Err(invalid_effect("plan transition order is invalid"));
    }
    match (p.status, effect_at(db, &p.last_source.block_id)?) {
        (
            PlanStatus::Authorized,
            PlanEffect::Authorize {
                plan,
                source,
                task,
                review,
            },
        ) if plan == p.plan
            && source == p.operator_source
            && task == p.task
            && review == p.review => {}
        (
            PlanStatus::Broken,
            PlanEffect::Break {
                expected,
                step_id,
                case,
                proposed_source,
            },
        ) if expected == p.plan => {
            if p.broken
                != Some(PlanBreakRecord {
                    case,
                    step_id,
                    proposed_source,
                    source: p.last_source.block_id.clone(),
                })
            {
                return Err(invalid_effect("broken plan differs from its effect"));
            }
        }
        (PlanStatus::Locked, PlanEffect::Lock { plan, .. }) if plan == p.plan => {}
        (PlanStatus::Locked | PlanStatus::Complete, PlanEffect::Replace { next, .. })
            if next == p.plan => {}
        (PlanStatus::Locked, PlanEffect::Activate { expected, .. }) if expected == p.plan => {}
        (PlanStatus::Locked | PlanStatus::Complete, PlanEffect::Close { expected, .. })
            if expected == p.plan => {}
        (PlanStatus::Superseded, PlanEffect::Replace { expected, .. }) if expected == p.plan => {}
        (PlanStatus::Superseded, PlanEffect::Authorize { .. } | PlanEffect::Lock { .. }) => {}
        _ => {
            return Err(invalid_effect(
                "plan status does not follow its last effect",
            ));
        }
    }
    verify_ordinals(db, &p.plan, "plan_category", 0)?;
    verify_ordinals(db, &p.plan, "plan_group", 0)?;
    for (index, step) in p.steps.iter().enumerate() {
        if step.ordinal.0 != index as u64 + 1
            || step.step_id.seed_id != seed
            || step.step_id.local_id.0 == 0
            || initial_steps.get(index)
                != Some(&PlanStepInit {
                    step_id: step.step_id.clone(),
                    instruction: step.instruction.clone(),
                })
        {
            return Err(invalid_effect(
                "stored step identity, order or instruction changed",
            ));
        }
        let held = marks::resolve_statement(db, &step.instruction)?;
        let verb: String = db.query_row(
            "SELECT verb FROM statement_binding WHERE id=?1",
            [held.id],
            |row| row.get(0),
        )?;
        let category = p
            .categories
            .iter()
            .find(|c| c.id == step.category)
            .ok_or_else(|| invalid_effect("stored step has no category"))?;
        if !category.verbs.contains(&verb) {
            return Err(invalid_effect("stored instruction has wrong category"));
        }
        match step.status {
            StepStatus::Waiting => {
                if step.span.is_some() {
                    return Err(invalid_effect("waiting step has a span"));
                }
            }
            StepStatus::Active | StepStatus::Retired => {
                let span = step
                    .span
                    .as_ref()
                    .ok_or_else(|| invalid_effect("executed step has no span"))?;
                let (kind, status, parent, revision) = span_fact(db, span, seed)?;
                let expected_parent = step
                    .group
                    .as_ref()
                    .and_then(|id| p.groups.iter().find(|g| g.id == *id))
                    .and_then(|g| g.span.as_ref())
                    .unwrap_or(&p.task.task);
                if kind != "step"
                    || status
                        != if step.status == StepStatus::Active {
                            "open"
                        } else {
                            "retired"
                        }
                    || parent.as_deref() != Some(blob(expected_parent.local_id.0).as_slice())
                    || revision.as_deref() != Some(blob(expected_parent.revision.0).as_slice())
                {
                    return Err(invalid_effect("step execution span changed"));
                }
                if step.status == StepStatus::Active
                    && !matches!(p.status, PlanStatus::Locked | PlanStatus::Broken)
                {
                    return Err(invalid_effect("inactive plan has an active step"));
                }
            }
            StepStatus::Closed => verify_closed(db, seed, p, step, category)?,
        }
    }
    for (index, group) in p.groups.iter().enumerate() {
        let draft = p
            .group_drafts
            .get(index)
            .ok_or_else(|| invalid_effect("extra stored group"))?;
        let expected = draft
            .step_indices
            .iter()
            .map(|i| {
                p.steps
                    .get(i.0 as usize)
                    .map(|s| s.step_id.clone())
                    .ok_or_else(|| invalid_effect("group index outside plan"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if group.id != draft.id || group.step_ids != expected {
            return Err(invalid_effect("stored group membership changed"));
        }
        if let Some(span) = &group.span {
            let (kind, _, parent, revision) = span_fact(db, span, seed)?;
            if kind != "subtask"
                || parent.as_deref() != Some(blob(p.task.task.local_id.0).as_slice())
                || revision.as_deref() != Some(blob(p.task.task.revision.0).as_slice())
            {
                return Err(invalid_effect("stored group span changed"));
            }
        }
    }
    if p.lock_source.is_some() && p.groups.len() != p.group_drafts.len() {
        return Err(invalid_effect("group rows are missing"));
    }
    if p.status == PlanStatus::Complete
        && (task_status != "closed" || p.steps.iter().any(|s| s.status != StepStatus::Closed))
    {
        return Err(invalid_effect("complete plan is not closed"));
    }
    if matches!(
        p.status,
        PlanStatus::Authorized | PlanStatus::Locked | PlanStatus::Broken
    ) && task_status != "open"
    {
        return Err(invalid_effect("unfinished plan has no open task"));
    }
    if matches!(p.status, PlanStatus::Locked | PlanStatus::Broken)
        && p.steps.iter().all(|step| step.status == StepStatus::Closed)
    {
        return Err(invalid_effect("closed plan was not marked complete"));
    }
    Ok(())
}

fn verify_input(
    db: &Connection,
    source: &SourceBlockId,
    input: &PlanInputContext,
) -> Result<(), FileFault> {
    let (task, steps) = match &input.operation {
        PlanInputOperation::Create { task, steps, .. } => (Some(*task), steps.as_slice()),
        PlanInputOperation::Authorize { task, .. } => (Some(*task), &[][..]),
        PlanInputOperation::Replace(r) | PlanInputOperation::AnswerBreak { replacement: r } => {
            (r.task, r.steps.as_slice())
        }
    };
    let authorize = matches!(input.operation, PlanInputOperation::Authorize { .. });
    if task.is_some_and(|index| index.0 != 0) || (!authorize && steps.is_empty()) {
        return Err(invalid_effect("invalid held plan input blocks"));
    }
    let offset = u64::from(task.is_some());
    let count = offset + steps.len() as u64;
    let actual: i64 = db.query_row(
        "SELECT COUNT(*) FROM content_block WHERE run=?1 AND sequence=?2",
        params![source.run, blob(source.sequence.0)],
        |row| row.get(0),
    )?;
    if u64::try_from(actual).ok() != Some(count) {
        return Err(invalid_effect("plan input has missing or unused blocks"));
    }
    for index in 0..count {
        let id = held_id(
            db,
            &SourceBlockId {
                block: index.into(),
                ..source.clone()
            },
        )?;
        let valid:bool=db.query_row("SELECT kind='plan.event' AND channel='input' AND origin='operator' AND turn IS NULL AND effect IS NULL AND content_type='statement' AND length(CAST(statement_text AS BLOB))>0 AND ((?2=0 AND context_raw IS NOT NULL) OR (?2!=0 AND context_raw IS NULL AND context_path IS NULL)) FROM content_block WHERE id=?1",params![id,index as i64],|row|row.get(0))?;
        if !valid {
            return Err(invalid_effect("held plan input provenance changed"));
        }
    }
    for (index, step) in steps.iter().enumerate() {
        if step.block.0 != offset + index as u64 {
            return Err(invalid_effect("plan input Text indices changed"));
        }
        let id = held_id(
            db,
            &SourceBlockId {
                block: step.block,
                ..source.clone()
            },
        )?;
        let bindings: i64 = db.query_row(
            "SELECT COUNT(*) FROM statement_binding WHERE block_id=?1",
            [id],
            |row| row.get(0),
        )?;
        if bindings != 1 {
            return Err(invalid_effect("plan instruction has multiple bindings"));
        }
    }
    Ok(())
}

fn verify_operator_steps(
    db: &Connection,
    seed: &str,
    p: &PlanSnapshot,
    input: &PlanInputContext,
) -> Result<(), FileFault> {
    let (steps, preserved, previous) = match &input.operation {
        PlanInputOperation::Authorize { .. } => return Ok(()),
        PlanInputOperation::Create { steps, .. } => (steps.as_slice(), &[][..], None),
        PlanInputOperation::Replace(r) | PlanInputOperation::AnswerBreak { replacement: r } => {
            let expected = input
                .expected
                .as_ref()
                .ok_or_else(|| invalid_effect("replacement has no previous revision"))?;
            (
                r.steps.as_slice(),
                r.preserved_closures.as_slice(),
                Some(reads::plan_snapshot(db, seed, expected).map_err(read_fault)?),
            )
        }
    };
    for (index, id) in preserved.iter().enumerate() {
        if preserved[..index].contains(id) || !p.steps.iter().any(|s| s.step_id == *id) {
            return Err(invalid_effect("invalid stored preservation list"));
        }
    }
    for (draft, step) in steps.iter().zip(&p.steps) {
        let proposed = StatementRef {
            block_id: SourceBlockId {
                block: draft.block,
                ..p.operator_source.clone()
            },
            binding: 0.into(),
        };
        if let Some(id) = &draft.id {
            if *id != step.step_id
                || previous
                    .as_ref()
                    .is_none_or(|old| !old.steps.iter().any(|s| s.step_id == *id))
            {
                return Err(invalid_effect(
                    "stored step identity is not in the preceding revision",
                ));
            }
        } else if previous
            .as_ref()
            .is_some_and(|old| old.steps.iter().any(|s| s.step_id == step.step_id))
        {
            return Err(invalid_effect("new step reused an existing identity"));
        }
        if preserved.contains(&step.step_id) {
            let old_plan = previous
                .as_ref()
                .ok_or_else(|| invalid_effect("no preceding preserved revision"))?;
            let old = old_plan
                .steps
                .iter()
                .find(|s| s.step_id == step.step_id)
                .ok_or_else(|| invalid_effect("missing preserved step"))?;
            let held = marks::resolve_statement(db, &step.instruction)?;
            let proposed = marks::resolve_statement(db, &proposed)?;
            let category = p.categories.iter().find(|c| c.id == step.category);
            let old_category = old_plan.categories.iter().find(|c| c.id == old.category);
            if old.status != StepStatus::Closed
                || step.status != StepStatus::Closed
                || step.instruction != old.instruction
                || super::revision::held_text(&held)? != super::revision::held_text(&proposed)?
                || category != old_category
                || p.review != old_plan.review
                || step.span != old.span
                || step.outcome_span != old.outcome_span
                || step.review_call != old.review_call
                || step.work != old.work
                || step.closure_source != old.closure_source
            {
                return Err(invalid_effect(
                    "stored preservation differs from its prior closure",
                ));
            }
        } else if step.instruction != proposed {
            return Err(invalid_effect(
                "stored instruction is not the operator's Text block",
            ));
        }
    }
    Ok(())
}

fn verify_ordinals(
    db: &Connection,
    plan: &PlanRef,
    table: &str,
    base: u64,
) -> Result<(), FileFault> {
    let mut query = db.prepare(&format!(
        "SELECT ordinal FROM {table} WHERE plan_id=?1 AND revision=?2 ORDER BY ordinal"
    ))?;
    let mut rows = query.query(params![
        blob(plan.plan_id.local_id.0),
        blob(plan.revision.0)
    ])?;
    let mut expected = base;
    while let Some(row) = rows.next()? {
        if decode_counter(row.get(0)?)? != expected {
            return Err(invalid_effect("plan ordinal sequence has a gap"));
        }
        expected = expected
            .checked_add(1)
            .ok_or_else(|| invalid_effect("ordinal overflow"))?;
    }
    Ok(())
}

fn verify_closed(
    db: &Connection,
    seed: &str,
    p: &PlanSnapshot,
    step: &PlanStep,
    category: &Category,
) -> Result<(), FileFault> {
    let work = step
        .work
        .as_ref()
        .ok_or_else(|| invalid_effect("closed step has no WorkRef"))?;
    let span = step
        .span
        .as_ref()
        .ok_or_else(|| invalid_effect("closed step has no span"))?;
    let outcome = step
        .outcome_span
        .as_ref()
        .ok_or_else(|| invalid_effect("closed step has no outcome"))?;
    let closure = step
        .closure_source
        .as_ref()
        .ok_or_else(|| invalid_effect("closed step has no closure source"))?;
    if work.step_id != step.step_id
        || work.span != *span
        || work.plan.plan_id != p.plan.plan_id
        || work.plan.revision > p.plan.revision
    {
        return Err(invalid_effect("closed WorkRef identity changed"));
    }
    let (kind, status, _, _) = span_fact(db, span, seed)?;
    let (outcome_kind, outcome_status, parent, revision) = span_fact(db, outcome, seed)?;
    if kind != "step"
        || status != "closed"
        || outcome_kind != "step_outcome"
        || outcome_status != "closed"
        || parent.as_deref() != Some(blob(span.local_id.0).as_slice())
        || revision.as_deref() != Some(blob(span.revision.0).as_slice())
    {
        return Err(invalid_effect("closed span or outcome changed"));
    }
    match effect_at(db, closure)? {
        PlanEffect::Close {
            expected,
            step_id,
            review_call,
            work: held,
            outcome_span,
        } if expected == work.plan
            && step_id == step.step_id
            && review_call == step.review_call
            && held == *work
            && outcome_span == *outcome => {}
        _ => {
            return Err(invalid_effect(
                "closure source does not match stored evidence",
            ));
        }
    }
    // A preserved closure must retain the complete category and review election of
    // its original revision, not merely a matching verb or category identifier.
    let (original_category,original_review):(String,String)=db.query_row("SELECT c.category,p.review_setup FROM plan_category c JOIN plan_revision p ON p.plan_id=c.plan_id AND p.revision=c.revision WHERE c.plan_id=?1 AND c.revision=?2 AND c.category_id=?3",params![blob(work.plan.plan_id.local_id.0),blob(work.plan.revision.0),category.id],|row|Ok((row.get(0)?,row.get(1)?)))?;
    if decode_strict::<Category>(&original_category)
        .map_err(|_| invalid_effect("invalid original category"))?
        != *category
        || decode_strict::<ReviewSetupRef>(&original_review)
            .map_err(|_| invalid_effect("invalid original review setup"))?
            != p.review
    {
        return Err(invalid_effect(
            "preserved closure changed its category or review",
        ));
    }
    let closure_id = held_id(db, closure)?;
    let frontier = held_id(db, &work.through.block_id)?;
    if frontier >= closure_id {
        return Err(invalid_effect("closure frontier is not earlier"));
    }
    let bytes = reads::historical_span_preview_encoded(db, seed, span, &work.through)
        .map_err(read_fault)?;
    if format!("{:x}", Sha256::digest(bytes)) != work.content_sha256.as_str() {
        return Err(invalid_effect("stored closure work digest changed"));
    }
    let mut allowed = vec![closure_id];
    match &step.review_call {
        None => {
            if category.review_required {
                return Err(invalid_effect("required review is missing"));
            }
        }
        Some(call) => {
            let (lane,fate,request,request_id,opened,terminal,raw):(String,String,String,i64,i64,i64,String)=db.query_row("SELECT t.lane,t.fate,t.request,s.block_id,t.opened_event,t.terminal_event,b.raw_block FROM tool_pair t JOIN statement_binding s ON s.id=t.statement_id JOIN content_block b ON b.id=t.terminal_event WHERE t.run=?1 AND t.sequence=?2 AND t.call=?3",params![call.run,blob(call.sequence.0),blob(call.call.0)],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)))?;
            let request: PairRequest = decode_strict(&request)
                .map_err(|_| invalid_effect("invalid stored review request"))?;
            let ended: PairEnded =
                decode_strict(&raw).map_err(|_| invalid_effect("invalid stored review outcome"))?;
            if lane != "review"
                || fate != "completed"
                || request_id <= frontier
                || terminal >= closure_id
                || ended.call_id != *call
                || !matches!(
                    ended.outcome,
                    Some(weaver_types::ToolOutcome::Result { .. })
                )
                || !matches!(request,PairRequest::Review{ref setup,work:ref requested,..} if setup==&p.review && requested.as_ref()==work)
            {
                return Err(invalid_effect("stored review evidence changed"));
            }
            allowed.extend([request_id, opened, terminal]);
        }
    }
    let mut statement =
        db.prepare("SELECT id FROM content_block WHERE source_kind='event' AND id>?1 AND id<=?2")?;
    let mut rows = statement.query(params![frontier, closure_id])?;
    while let Some(row) = rows.next()? {
        if !allowed.contains(&row.get::<_, i64>(0)?) {
            return Err(invalid_effect("stored work changed before closure"));
        }
    }
    Ok(())
}
