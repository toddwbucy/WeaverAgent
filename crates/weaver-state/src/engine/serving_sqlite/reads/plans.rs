use super::*;

const STEPS: &str = "SELECT s.*,g.group_id FROM plan_step s LEFT JOIN plan_group_step g ON g.plan_id=s.plan_id AND g.revision=s.revision AND g.step_id=s.step_id WHERE s.plan_id=?1 AND s.revision=?2 ORDER BY s.ordinal";
const CURRENT: &str =
    "SELECT * FROM plan_revision WHERE status!='superseded' ORDER BY last_source DESC LIMIT 1";
const REVISION: &str = "SELECT * FROM plan_revision WHERE plan_id=?1 AND revision=?2";
const CATEGORIES: &str = "SELECT category_id,category FROM plan_category WHERE plan_id=?1 AND revision=?2 ORDER BY ordinal";
const GROUPS: &str = "SELECT * FROM plan_group WHERE plan_id=?1 AND revision=?2 ORDER BY ordinal";
const GROUP_STEPS: &str = "SELECT s.step_id FROM plan_group_step g JOIN plan_step s ON s.plan_id=g.plan_id AND s.revision=g.revision AND s.step_id=g.step_id WHERE g.plan_id=?1 AND g.revision=?2 AND g.group_id=?3 ORDER BY s.ordinal";

// Both the public streaming serializer and the transaction-local typed guard
// use these row decoders. Only a public result supplies a remaining-byte bound.
fn plan_text<'a>(row: &'a Row<'_>, name: &str, out: Option<&Output>) -> Result<&'a str> {
    match out {
        Some(out) => text_cell(row, name, out),
        None => row
            .get_ref(name)?
            .as_str()
            .map_err(|_| ReadFailure::ComponentCorruption("stored text is not UTF-8")),
    }
}
macro_rules! plan_json {
    ($row:expr, $name:expr, $out:expr, $ty:ty) => {
        decode_strict::<$ty>(plan_text($row, $name, $out)?)
            .map_err(|_| ReadFailure::ComponentCorruption("invalid typed stored JSON"))?
    };
}
fn task_scope(row: &Row<'_>, seed: &str) -> Result<TaskScopeRef> {
    Ok(TaskScopeRef {
        task: marks::span_ref(
            seed,
            counter(row, "task_id")?,
            counter(row, "task_revision")?,
        ),
        prompt: marks::span_ref(
            seed,
            counter(row, "prompt_id")?,
            counter(row, "prompt_revision")?,
        ),
    })
}
fn lock_source(db: &Connection, row: &Row<'_>) -> Result<Option<SourceBlockId>> {
    row.get::<_, Option<i64>>("lock_source")?
        .map(|id| source_id(db, id))
        .transpose()
}
fn category(row: &Row<'_>, out: Option<&Output>) -> Result<Category> {
    let category = plan_json!(row, "category", out, Category);
    if category.id != plan_text(row, "category_id", out)? {
        return Err(ReadFailure::ComponentCorruption(
            "category identity differs from index",
        ));
    }
    Ok(category)
}
fn step_id(row: &Row<'_>, seed: &str) -> Result<StepId> {
    Ok(StepId {
        seed_id: seed.into(),
        local_id: counter(row, "step_id")?,
    })
}
fn broken(row: &Row<'_>, out: Option<&Output>) -> Result<Option<PlanBreakRecord>> {
    if matches!(row.get_ref("broken")?, rusqlite::types::ValueRef::Null) {
        Ok(None)
    } else {
        Ok(Some(plan_json!(row, "broken", out, PlanBreakRecord)))
    }
}
fn push<T>(values: &mut Vec<T>, value: T) -> Result<()> {
    values
        .try_reserve(1)
        .map_err(|_| ReadRefusalCode::ResultUnavailable)?;
    values.push(value);
    Ok(())
}

/// Derive the actual typed metadata inside the caller's transaction. No public
/// response is encoded and no result-byte limit is invented for an atomic guard.
/// Keep this materializing traversal separate from `snapshot`'s bounded stream:
/// building a full PlanSnapshot for a public query would allocate before that
/// query's byte ceiling can refuse it. Shared row decoders above and the typed /
/// public parity tests pin the mapping without imposing that allocation policy.
fn typed_snapshot(db: &Connection, seed: &str, row: &Row<'_>) -> Result<PlanSnapshot> {
    let plan = plan_ref(seed, row)?;
    let id = blob(plan.plan_id.local_id);
    let revision = blob(plan.revision);
    let mut snapshot = PlanSnapshot {
        plan,
        task: task_scope(row, seed)?,
        status: plan_status(row)?,
        review: plan_json!(row, "review_setup", None, ReviewSetupRef),
        operator_source: source_id(db, row.get("operator_source")?)?,
        lock_source: lock_source(db, row)?,
        document_sha256: operator_context(db, row.get("operator_source")?)?.document_sha256,
        last_source: transition_position(db, row.get("last_source")?)?,
        categories: vec![],
        group_drafts: plan_json!(row, "group_drafts", None, Vec<GroupDraft>),
        groups: vec![],
        steps: vec![],
        broken: broken(row, None)?,
    };
    let mut categories = db.prepare(CATEGORIES)?;
    let mut categories = categories.query(params![id, revision])?;
    while let Some(row) = categories.next()? {
        push(&mut snapshot.categories, category(row, None)?)?;
    }
    let mut groups = db.prepare(GROUPS)?;
    let mut groups = groups.query(params![id, revision])?;
    while let Some(row) = groups.next()? {
        let group = plan_text(row, "group_id", None)?;
        let mut value = PlanGroup {
            id: group.into(),
            step_ids: vec![],
            span: marks::optional_span(row, seed, "span_id", "span_revision")?,
        };
        let mut steps = db.prepare(GROUP_STEPS)?;
        let mut steps = steps.query(params![id, revision, group])?;
        while let Some(row) = steps.next()? {
            push(&mut value.step_ids, step_id(row, seed)?)?;
        }
        push(&mut snapshot.groups, value)?;
    }
    let mut steps = db.prepare(STEPS)?;
    let mut steps = steps.query(params![id, revision])?;
    while let Some(row) = steps.next()? {
        push(&mut snapshot.steps, read_step(db, seed, None, row)?)?;
    }
    Ok(snapshot)
}

pub(super) fn current_snapshot(db: &Connection, seed: &str) -> Result<Option<PlanSnapshot>> {
    let mut statement = db.prepare(CURRENT)?;
    let mut rows = statement.query([])?;
    rows.next()?
        .map(|row| typed_snapshot(db, seed, row))
        .transpose()
}
pub(super) fn historical_snapshot(
    db: &Connection,
    seed: &str,
    plan: &PlanRef,
) -> Result<PlanSnapshot> {
    same_seed(seed, &plan.plan_id.seed_id)?;
    let mut statement = db.prepare(REVISION)?;
    let mut rows = statement.query(params![blob(plan.plan_id.local_id), blob(plan.revision)])?;
    let row = rows.next()?.ok_or(ReadRefusalCode::NotFound)?;
    typed_snapshot(db, seed, row)
}

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
    let mut statement = db.prepare(CURRENT)?;
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
    let mut statement = db.prepare(REVISION)?;
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
    value!(out, task_scope(row, seed)?)?;
    out.raw(",\"status\":")?;
    value!(out, plan_status(row)?)?;
    out.raw(",\"review\":")?;
    value!(
        out,
        plan_json!(row, "review_setup", Some(out), ReviewSetupRef)
    )?;
    out.raw(",\"operator_source\":")?;
    value!(out, source_id(db, row.get("operator_source")?)?)?;
    out.raw(",\"lock_source\":")?;
    value!(out, lock_source(db, row)?)?;
    out.raw(",\"document_sha256\":")?;
    value!(
        out,
        operator_context(db, row.get("operator_source")?)?.document_sha256
    )?;
    out.raw(",\"last_source\":")?;
    value!(out, transition_position(db, row.get("last_source")?)?)?;
    out.raw(",\"categories\":[")?;
    let mut categories = db.prepare(CATEGORIES)?;
    let mut categories = categories.query(params![id, revision])?;
    let mut first = true;
    while let Some(row) = categories.next()? {
        let category = category(row, Some(out))?;
        out.comma(&mut first)?;
        value!(out, category)?;
    }
    out.raw("],\"group_drafts\":")?;
    value!(
        out,
        plan_json!(row, "group_drafts", Some(out), Vec<GroupDraft>)
    )?;
    out.raw(",\"groups\":[")?;
    let mut groups = db.prepare(GROUPS)?;
    let mut groups = groups.query(params![id, revision])?;
    let mut first = true;
    while let Some(row) = groups.next()? {
        out.comma(&mut first)?;
        out.raw("{\"id\":")?;
        let group = plan_text(row, "group_id", Some(out))?;
        value!(out, group)?;
        out.raw(",\"step_ids\":[")?;
        let mut steps = db.prepare(GROUP_STEPS)?;
        let mut steps = steps.query(params![id, revision, group])?;
        let mut first = true;
        while let Some(row) = steps.next()? {
            out.comma(&mut first)?;
            value!(out, step_id(row, seed)?)?;
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
        value!(out, read_step(db, seed, Some(out), row)?)?;
    }
    out.raw("],\"broken\":")?;
    value!(out, broken(row, Some(out))?)?;
    out.raw("}")
}

pub(in crate::engine::serving_sqlite) fn operator_context(
    db: &Connection,
    id: i64,
) -> Result<PlanInputContext> {
    let mut statement = db.prepare(
        "SELECT context_raw FROM content_block WHERE id=?1 AND source_kind='event' AND kind='plan.event' AND channel='input' AND origin='operator' AND turn IS NULL AND block=?2 AND effect IS NULL AND scheme_id=(SELECT scheme_id FROM store_meta WHERE singleton=1)",
    )?;
    let mut rows = statement.query(params![id, blob(0.into())])?;
    let row = rows.next()?.ok_or(ReadFailure::ComponentCorruption(
        "invalid plan operator source",
    ))?;
    // Borrow the already held context. Its full input shape is validated, while
    // the read-result byte ceiling applies to the emitted digest, not this input.
    let raw = row
        .get_ref("context_raw")?
        .as_str()
        .map_err(|_| ReadFailure::ComponentCorruption("plan operator context is not text"))?;
    let context: PlanInputContext = decode_strict(raw)
        .map_err(|_| ReadFailure::ComponentCorruption("invalid plan operator context"))?;
    if context.cause.uid != 0 {
        return Err(ReadFailure::ComponentCorruption(
            "plan operator context has no root provenance",
        ));
    }
    Ok(context)
}

fn transition_position(db: &Connection, id: i64) -> Result<SourcePosition> {
    let mut statement =
        db.prepare("SELECT run,sequence,block,turn,source_kind FROM content_block WHERE id=?1")?;
    let mut rows = statement.query([id])?;
    let row = rows.next()?.ok_or(ReadFailure::ComponentCorruption(
        "missing plan transition source",
    ))?;
    if row.get::<_, String>("source_kind")? != "event" {
        return Err(ReadFailure::ComponentCorruption(
            "plan transition source is not an event",
        ));
    }
    Ok(SourcePosition {
        block_id: SourceBlockId {
            run: row.get("run")?,
            sequence: counter(row, "sequence")?,
            block: counter(row, "block")?,
        },
        turn: row.get("turn")?,
    })
}

fn read_step(db: &Connection, seed: &str, out: Option<&Output>, row: &Row<'_>) -> Result<PlanStep> {
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
        Some(plan_json!(row, "work", out, WorkRef))
    };
    Ok(PlanStep {
        step_id: step_id(row, seed)?,
        ordinal: counter(row, "ordinal")?,
        instruction: statement_ref(db, row.get("instruction_id")?)?,
        category: plan_text(row, "category_id", out)?.into(),
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
