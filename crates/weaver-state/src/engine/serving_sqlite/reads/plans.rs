use super::*;

const STEPS: &str = "SELECT s.*,g.group_id FROM plan_step s LEFT JOIN plan_group_step g ON g.plan_id=s.plan_id AND g.revision=s.revision AND g.step_id=s.step_id WHERE s.plan_id=?1 AND s.revision=?2 ORDER BY s.ordinal";
fn plan_ref(seed: &str, row: &Row<'_>) -> Result<PlanRef> {
    Ok(PlanRef {
        plan_id: PlanId {
            seed_id: seed.into(),
            local_id: counter(row, "plan_id")?,
        },
        revision: counter(row, "revision")?,
    })
}
fn plan_status(row: &Row<'_>) -> Result<PlanStatus> {
    match row.get::<_, String>("status")?.as_str() {
        "authorized" => Ok(PlanStatus::Authorized),
        "locked" => Ok(PlanStatus::Locked),
        "broken" => Ok(PlanStatus::Broken),
        "complete" => Ok(PlanStatus::Complete),
        "superseded" => Ok(PlanStatus::Superseded),
        _ => Err(ReadFailure::ComponentCorruption("unknown plan status")),
    }
}
fn step_status(row: &Row<'_>) -> Result<StepStatus> {
    match row.get::<_, String>("status")?.as_str() {
        "waiting" => Ok(StepStatus::Waiting),
        "active" => Ok(StepStatus::Active),
        "closed" => Ok(StepStatus::Closed),
        "retired" => Ok(StepStatus::Retired),
        _ => Err(ReadFailure::ComponentCorruption("unknown step status")),
    }
}

pub(super) fn current(db: &Connection, seed: &str, out: &mut Output) -> Result<()> {
    let next_plan = checked_next(db, "SELECT MAX(plan_id) AS largest FROM plan_revision")?;
    let next_step = checked_next(db, "SELECT MAX(step_id) AS largest FROM plan_step")?;
    out.raw("{\"plan_current\":{\"current\":")?;
    let mut statement = db.prepare(
        "SELECT * FROM plan_revision WHERE status!='superseded' ORDER BY last_source DESC LIMIT 1",
    )?;
    let mut rows = statement.query([])?;
    if let Some(row) = rows.next()? {
        snapshot(db, seed, out, row)?;
    } else {
        out.raw("null")?;
    }
    out.raw(",\"next_plan_id\":")?;
    value!(out, next_plan)?;
    out.raw(",\"next_step_id\":")?;
    value!(out, next_step)?;
    out.raw("}}")
}
pub(super) fn historical(
    db: &Connection,
    seed: &str,
    out: &mut Output,
    plan: &PlanRef,
) -> Result<()> {
    same_seed(seed, &plan.plan_id.seed_id)?;
    let mut statement =
        db.prepare("SELECT * FROM plan_revision WHERE plan_id=?1 AND revision=?2")?;
    let mut rows = statement.query(params![blob(plan.plan_id.local_id), blob(plan.revision)])?;
    let row = rows.next()?.ok_or(ReadRefusalCode::NotFound)?;
    out.raw("{\"plan\":{\"snapshot\":")?;
    snapshot(db, seed, out, row)?;
    out.raw("}}")
}
pub(super) fn snapshot(db: &Connection, seed: &str, out: &mut Output, row: &Row<'_>) -> Result<()> {
    let plan = plan_ref(seed, row)?;
    let id = blob(plan.plan_id.local_id);
    let revision = blob(plan.revision);
    out.raw("{\"plan\":")?;
    value!(out, plan)?;
    out.raw(",\"task\":")?;
    value!(
        out,
        TaskScopeRef {
            task: marks::span_ref(
                seed,
                counter(row, "task_id")?,
                counter(row, "task_revision")?
            ),
            prompt: marks::span_ref(
                seed,
                counter(row, "prompt_id")?,
                counter(row, "prompt_revision")?
            )
        }
    )?;
    out.raw(",\"status\":")?;
    value!(out, plan_status(row)?)?;
    out.raw(",\"review\":")?;
    value!(out, json_cell!(out, row, "review_setup", ReviewSetupRef))?;
    out.raw(",\"operator_source\":")?;
    value!(out, source_id(db, row.get("operator_source")?)?)?;
    out.raw(",\"lock_source\":")?;
    value!(
        out,
        row.get::<_, Option<i64>>("lock_source")?
            .map(|id| source_id(db, id))
            .transpose()?
    )?;
    out.raw(",\"last_source\":")?;
    value!(out, source_id(db, row.get("last_source")?)?)?;
    out.raw(",\"categories\":[")?;
    let mut categories=db.prepare("SELECT category_id,category FROM plan_category WHERE plan_id=?1 AND revision=?2 ORDER BY ordinal")?;
    let mut categories = categories.query(params![id, revision])?;
    let mut first = true;
    while let Some(row) = categories.next()? {
        let category: Category = json_cell!(out, row, "category", Category);
        if category.id != text_cell(row, "category_id", out)? {
            return Err(ReadFailure::ComponentCorruption(
                "category identity differs from index",
            ));
        }
        out.comma(&mut first)?;
        value!(out, category)?;
    }
    out.raw("],\"group_drafts\":")?;
    value!(out, json_cell!(out, row, "group_drafts", Vec<GroupDraft>))?;
    out.raw(",\"groups\":[")?;
    let mut groups =
        db.prepare("SELECT * FROM plan_group WHERE plan_id=?1 AND revision=?2 ORDER BY ordinal")?;
    let mut groups = groups.query(params![id, revision])?;
    let mut first = true;
    while let Some(row) = groups.next()? {
        out.comma(&mut first)?;
        out.raw("{\"id\":")?;
        let group = text_cell(row, "group_id", out)?;
        value!(out, group)?;
        out.raw(",\"step_ids\":[")?;
        let mut steps=db.prepare("SELECT s.step_id FROM plan_group_step g JOIN plan_step s ON s.plan_id=g.plan_id AND s.revision=g.revision AND s.step_id=g.step_id WHERE g.plan_id=?1 AND g.revision=?2 AND g.group_id=?3 ORDER BY s.ordinal")?;
        let mut steps = steps.query(params![id, revision, group])?;
        let mut first = true;
        while let Some(row) = steps.next()? {
            out.comma(&mut first)?;
            value!(
                out,
                StepId {
                    seed_id: seed.into(),
                    local_id: counter(row, "step_id")?
                }
            )?;
        }
        out.raw("],\"span\":")?;
        value!(
            out,
            marks::optional_span(row, seed, "span_id", "span_revision")?
        )?;
        out.raw("}")?;
    }
    out.raw("],\"steps\":[")?;
    let mut steps = db.prepare(STEPS)?;
    let mut steps = steps.query(params![id, revision])?;
    let mut first = true;
    while let Some(row) = steps.next()? {
        out.comma(&mut first)?;
        value!(out, read_step(db, seed, out, row)?)?;
    }
    out.raw("],\"broken\":")?;
    if matches!(row.get_ref("broken")?, rusqlite::types::ValueRef::Null) {
        out.raw("null")?;
    } else {
        value!(out, json_cell!(out, row, "broken", PlanBreakRecord))?;
    }
    out.raw("}")
}
fn read_step(db: &Connection, seed: &str, out: &Output, row: &Row<'_>) -> Result<PlanStep> {
    let review_run: Option<String> = row.get("review_run")?;
    let review_call = match review_run {
        Some(run) => Some(CallId {
            run,
            sequence: counter(row, "review_sequence")?,
            call: counter(row, "review_call")?,
        }),
        None => {
            if nullable_counter(row, "review_sequence")?.is_some()
                || nullable_counter(row, "review_call")?.is_some()
            {
                return Err(ReadFailure::ComponentCorruption("partial review call"));
            }
            None
        }
    };
    let work = if matches!(row.get_ref("work")?, rusqlite::types::ValueRef::Null) {
        None
    } else {
        Some(json_cell!(out, row, "work", WorkRef))
    };
    Ok(PlanStep {
        step_id: StepId {
            seed_id: seed.into(),
            local_id: counter(row, "step_id")?,
        },
        ordinal: counter(row, "ordinal")?,
        instruction: statement_ref(db, row.get("instruction_id")?)?,
        category: text_cell(row, "category_id", out)?.into(),
        group: row.get("group_id")?,
        status: step_status(row)?,
        span: marks::optional_span(row, seed, "span_id", "span_revision")?,
        outcome_span: marks::optional_span(row, seed, "outcome_id", "outcome_revision")?,
        review_call,
        work,
        closure_source: row
            .get::<_, Option<i64>>("closure_source")?
            .map(|id| source_id(db, id))
            .transpose()?,
    })
}
pub(super) fn successor(
    db: &Connection,
    seed: &str,
    out: &mut Output,
    plan: &PlanRef,
    previous: Option<&StepId>,
) -> Result<()> {
    same_seed(seed, &plan.plan_id.seed_id)?;
    let id = blob(plan.plan_id.local_id);
    let revision = blob(plan.revision);
    let status: Option<String> = db
        .query_row(
            "SELECT status FROM plan_revision WHERE plan_id=?1 AND revision=?2",
            params![id, revision],
            |row| row.get(0),
        )
        .optional()?;
    let status = status.ok_or(ReadRefusalCode::InvalidSelector)?;
    if matches!(status.as_str(), "complete" | "superseded") {
        return Err(ReadRefusalCode::NotFound.into());
    }
    let active:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM plan_step WHERE plan_id=?1 AND revision=?2 AND status='active')",params![id,revision],|row|row.get(0))?;
    if active {
        return Err(ReadRefusalCode::InvalidSelector.into());
    }
    let mut statement=db.prepare("SELECT step_id,ordinal,instruction_id FROM plan_step WHERE plan_id=?1 AND revision=?2 AND status='waiting' ORDER BY ordinal LIMIT 1")?;
    let mut rows = statement.query(params![id, revision])?;
    let row = rows.next()?.ok_or(ReadRefusalCode::NotFound)?;
    let ordinal = counter(row, "ordinal")?;
    if ordinal.0 == 0 {
        return Err(ReadFailure::ComponentCorruption("zero plan ordinal"));
    }
    let earlier:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM plan_step WHERE plan_id=?1 AND revision=?2 AND ordinal<?3 AND status!='closed')",params![id,revision,blob(ordinal)],|row|row.get(0))?;
    if earlier {
        return Err(ReadRefusalCode::InvalidSelector.into());
    }
    let expected = if ordinal.0 == 1 {
        None
    } else {
        let mut predecessor=db.prepare("SELECT step_id FROM plan_step WHERE plan_id=?1 AND revision=?2 AND ordinal=?3 AND status='closed'")?;
        let mut predecessor =
            predecessor.query(params![id, revision, blob((ordinal.0 - 1).into())])?;
        let row = predecessor
            .next()?
            .ok_or(ReadRefusalCode::InvalidSelector)?;
        Some(StepId {
            seed_id: seed.into(),
            local_id: counter(row, "step_id")?,
        })
    };
    if previous != expected.as_ref() {
        return Err(ReadRefusalCode::InvalidSelector.into());
    }
    let instruction = statement_ref(db, row.get("instruction_id")?)?;
    let reference = StepQueryRef {
        plan: plan.clone(),
        previous_step: expected,
        step_id: StepId {
            seed_id: seed.into(),
            local_id: counter(row, "step_id")?,
        },
        instruction: instruction.clone(),
    };
    let (block, binding) = content::resolve_statement(db, &instruction)?.ok_or(
        ReadFailure::ComponentCorruption("missing successor instruction"),
    )?;
    out.raw("{\"step_successor\":{\"reference\":")?;
    value!(out, reference)?;
    out.raw(",\"instruction\":")?;
    content::entry(db, out, block, Some(binding), None, Some(&reference))?;
    out.raw("}}")
}
pub(super) fn flags(
    db: &Connection,
    seed: &str,
    out: &mut Output,
    plan: &PlanRef,
    after: &SourceBlockId,
) -> Result<()> {
    same_seed(seed, &plan.plan_id.seed_id)?;
    let requested: Option<i64> = db
        .query_row(
            "SELECT last_source FROM plan_revision WHERE plan_id=?1 AND revision=?2",
            params![blob(plan.plan_id.local_id), blob(plan.revision)],
            |row| row.get(0),
        )
        .optional()?;
    requested.ok_or(ReadRefusalCode::NotFound)?;
    let after_id = resolve_source(db, after)?.ok_or(ReadRefusalCode::NotFound)?;
    let mut current=db.prepare("SELECT plan_id,revision,last_source FROM plan_revision WHERE status!='superseded' ORDER BY last_source DESC LIMIT 1")?;
    let mut current = current.query([])?;
    let row = current.next()?.ok_or(ReadRefusalCode::InvalidSelector)?;
    if plan_ref(seed, row)? != *plan || row.get::<_, i64>("last_source")? != after_id {
        return Err(ReadRefusalCode::InvalidSelector.into());
    }
    let through = position(db)?.ok_or(ReadFailure::ComponentCorruption(
        "plan has no event frontier",
    ))?;
    // This parameter is the specified complete target set, not result rows. Grow it
    // fallibly, without a second row vector or a signed conversion of a step ID.
    let mut targets = String::from("[");
    let mut statement=db.prepare("SELECT step_id FROM plan_step WHERE plan_id=?1 AND revision=?2 AND status!='retired' ORDER BY ordinal")?;
    let mut rows = statement.query(params![blob(plan.plan_id.local_id), blob(plan.revision)])?;
    let mut first = true;
    while let Some(row) = rows.next()? {
        let id = counter(row, "step_id")?;
        if id.0 == 0 {
            return Err(ReadFailure::ComponentCorruption("zero step address"));
        }
        let target = format!("\"step:{}\"", id.0);
        targets
            .try_reserve(target.len() + 1)
            .map_err(|_| ReadRefusalCode::ResultUnavailable)?;
        if !first {
            targets.push(',');
        }
        first = false;
        targets.push_str(&target);
    }
    targets
        .try_reserve(1)
        .map_err(|_| ReadRefusalCode::ResultUnavailable)?;
    targets.push(']');
    let (total,matched):(i64,i64)=db.query_row("SELECT COUNT(*) AS total_flags,COALESCE(SUM(CASE WHEN t.value IS NULL THEN 0 ELSE 1 END),0) AS matched_flags FROM statement_binding b LEFT JOIN json_each(?1) t ON b.condition=t.value COLLATE BINARY WHERE b.condition GLOB 'step:[1-9]*' AND substr(b.condition,6) NOT GLOB '*[^0-9]*' AND instr(b.condition,char(0))=0 AND length(b.condition) BETWEEN 6 AND 25 AND (length(b.condition)<25 OR b.condition<='step:18446744073709551615' COLLATE BINARY)",[targets],|row|Ok((row.get(0)?,row.get(1)?)))?;
    let total = nonnegative(total)?;
    let matched = nonnegative(matched)?;
    let unmatched = total
        .0
        .checked_sub(matched.0)
        .ok_or(ReadFailure::ComponentCorruption("flag count underflow"))?;
    out.raw("{\"plan_flags\":{\"snapshot\":")?;
    value!(
        out,
        PlanFlagSnapshot {
            plan: plan.clone(),
            after: after.clone(),
            through,
            total_flags: total,
            matched_flags: matched,
            unmatched_flags: unmatched.into()
        }
    )?;
    out.raw("}}")
}
