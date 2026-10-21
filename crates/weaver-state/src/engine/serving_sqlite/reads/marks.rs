use super::super::filing::marks as members;
use super::*;
use rusqlite::named_params;

pub(super) fn span(
    db: &Connection,
    seed: &str,
    out: &mut Output,
    reference: &SpanRef,
) -> Result<()> {
    same_seed(seed, &reference.seed_id)?;
    let mut statement = db.prepare(
        "SELECT kind,status,ended_content_id FROM span WHERE local_id=?1 AND revision=?2",
    )?;
    let mut rows = statement.query(params![blob(reference.local_id), blob(reference.revision)])?;
    let row = rows.next()?.ok_or(ReadRefusalCode::UnknownSpan)?;
    if row.get::<_, String>("status")? == "open" {
        return Err(ReadRefusalCode::SpanOpen.into());
    }
    let frontier: Option<i64> = row.get("ended_content_id")?;
    let seed_prompt = row.get::<_, String>("kind")? == "system_prompt";
    if seed_prompt {
        if reference.local_id.0 != 0 || reference.revision.0 != 1 || frontier.is_some() {
            return Err(ReadFailure::ComponentCorruption(
                "invalid system prompt span",
            ));
        }
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM span_member WHERE local_id=?1 AND revision=?2",
            params![blob(reference.local_id), blob(reference.revision)],
            |row| row.get(0),
        )?;
        if count != 1 {
            return Err(ReadFailure::ComponentCorruption(
                "system prompt must have one member",
            ));
        }
    } else if frontier.is_none() {
        return Err(ReadFailure::ComponentCorruption(
            "terminal runtime span has no frontier",
        ));
    }
    out.raw("{\"entries\":{\"entries\":[")?;
    let mut statement=db.prepare("SELECT m.ordinal,b.id,m.start,m.end,m.step_query FROM span_member m JOIN content_block b ON b.id=m.block_id WHERE m.local_id=?1 AND m.revision=?2 AND b.content_type='statement' ORDER BY m.ordinal")?;
    let mut rows = statement.query(params![blob(reference.local_id), blob(reference.revision)])?;
    let mut first = true;
    let mut expected = 0_u64;
    while let Some(row) = rows.next()? {
        if counter(row, "ordinal")?.0 != expected {
            return Err(ReadFailure::ComponentCorruption("span member ordinal gap"));
        }
        expected = expected
            .checked_add(1)
            .ok_or(ReadRefusalCode::CounterExhausted)?;
        let id: i64 = row.get("id")?;
        let start = counter(row, "start")?;
        let end = counter(row, "end")?;
        let query = step_query(row, out)?;
        precheck_text(db, id, out)?;
        if let Some(frontier) = frontier {
            check_interval(db, id, start, end, frontier)?;
        }
        if seed_prompt {
            let valid:bool=db.query_row("SELECT source_kind='seed' AND seed_ordinal=X'0000000000000000' AND length(CAST(statement_text AS BLOB))=?2 FROM content_block WHERE id=?1",params![id,i64::try_from(end.0).map_err(|_|ReadFailure::ComponentCorruption("prompt endpoint overflow"))?],|row|row.get(0))?;
            if !valid || start.0 != 0 || query.is_some() {
                return Err(ReadFailure::ComponentCorruption(
                    "system prompt member invariant",
                ));
            }
        }
        out.comma(&mut first)?;
        content::entry(
            db,
            out,
            id,
            None,
            Some(content::Interval {
                span: reference,
                start,
                end,
                frontier,
            }),
            query.as_ref(),
        )?;
    }
    out.raw("]}}")
}
fn step_query(row: &Row<'_>, out: &Output) -> Result<Option<StepQueryRef>> {
    if matches!(row.get_ref("step_query")?, rusqlite::types::ValueRef::Null) {
        Ok(None)
    } else {
        Ok(Some(json_cell!(out, row, "step_query", StepQueryRef)))
    }
}
fn precheck_text(db: &Connection, id: i64, out: &Output) -> Result<()> {
    let length:i64=db.query_row("SELECT length(CAST(statement_text AS BLOB)) FROM content_block WHERE id=?1 AND content_type='statement'",[id],|row|row.get(0))?;
    if nonnegative(length)?.0 > out.remaining() as u64 {
        return Err(ReadRefusalCode::ResultTooLarge.into());
    }
    Ok(())
}
fn check_interval(
    db: &Connection,
    id: i64,
    start: DecimalU64,
    end: DecimalU64,
    frontier: i64,
) -> Result<()> {
    members::validate_interval(db, id, start.0, end.0, frontier).map_err(|error| match error {
        members::IntervalFault::PartialBinding => ReadRefusalCode::InvalidSelector.into(),
        members::IntervalFault::PartialPair => ReadRefusalCode::PartialPair.into(),
        members::IntervalFault::PendingPair => ReadRefusalCode::SpanPending.into(),
        members::IntervalFault::Malformed => {
            ReadFailure::ComponentCorruption("invalid stored span interval")
        }
        members::IntervalFault::Sqlite(error) => ReadFailure::Sqlite(error),
    })
}
pub(super) fn preview(
    db: &Connection,
    seed: &str,
    out: &mut Output,
    span: &SpanRef,
    through: &SourcePosition,
) -> Result<()> {
    preview_at(db, seed, out, span, through, false)
}

pub(super) fn historical_preview(
    db: &Connection,
    seed: &str,
    out: &mut Output,
    span: &SpanRef,
    through: &SourcePosition,
) -> Result<()> {
    preview_at(db, seed, out, span, through, true)
}

fn preview_at(
    db: &Connection,
    seed: &str,
    out: &mut Output,
    span: &SpanRef,
    through: &SourcePosition,
    historical: bool,
) -> Result<()> {
    same_seed(seed, &span.seed_id)?;
    let mut statement = db.prepare(
        "SELECT status,opened_content_id,ended_content_id FROM span WHERE local_id=?1 AND revision=?2",
    )?;
    let mut rows = statement.query(params![blob(span.local_id), blob(span.revision)])?;
    let row = rows.next()?.ok_or(ReadRefusalCode::InvalidSelector)?;
    let status: String = row.get("status")?;
    if status != "open" && !historical {
        return Err(ReadRefusalCode::InvalidSelector.into());
    }
    let end_id: Option<i64> = row.get("ended_content_id")?;
    if (status == "open") != end_id.is_none() {
        return Err(ReadFailure::ComponentCorruption(
            "span status and frontier disagree",
        ));
    }
    let open_id: i64 = row.get("opened_content_id")?;
    let upper_id =
        resolve_source(db, &through.block_id)?.ok_or(ReadRefusalCode::InvalidSelector)?;
    let turn: Option<String> = db.query_row(
        "SELECT turn FROM content_block WHERE id=?1",
        [upper_id],
        |row| row.get(0),
    )?;
    if upper_id < open_id || end_id.is_some_and(|end| upper_id > end) || turn != through.turn {
        return Err(ReadRefusalCode::InvalidSelector.into());
    }
    out.raw("{\"span_preview\":{\"span\":")?;
    value!(out, span)?;
    out.raw(",\"through\":")?;
    value!(out, through)?;
    out.raw(",\"entries\":[")?;
    let mut statement = db.prepare(members::MEMBERS_SQL)?;
    let mut rows=statement.query(named_params!{":span_id":blob(span.local_id),":span_rev":blob(span.revision),":open_id":open_id,":upper_id":upper_id,":inclusive_work":1})?;
    let mut first = true;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get("block_id")?;
        precheck_text(db, id, out)?;
        if !matches!(row.get_ref("step_query")?, rusqlite::types::ValueRef::Null) {
            text_cell(row, "step_query", out)?;
        }
        let member = members::member_from_row(db, row)
            .map_err(|_| ReadFailure::ComponentCorruption("invalid canonical member"))?;
        check_interval(db, id, member.start.into(), member.end.into(), upper_id)?;
        out.comma(&mut first)?;
        content::entry(
            db,
            out,
            id,
            None,
            Some(content::Interval {
                span,
                start: member.start.into(),
                end: member.end.into(),
                frontier: Some(upper_id),
            }),
            member.step_query.as_ref(),
        )?;
    }
    out.raw("]}}")
}

pub(super) fn span_ref(seed: &str, id: DecimalU64, revision: DecimalU64) -> SpanRef {
    SpanRef {
        seed_id: seed.into(),
        local_id: id,
        revision,
    }
}
pub(super) fn optional_span(
    row: &Row<'_>,
    seed: &str,
    id: &str,
    revision: &str,
) -> Result<Option<SpanRef>> {
    match (nullable_counter(row, id)?, nullable_counter(row, revision)?) {
        (Some(id), Some(revision)) => Ok(Some(span_ref(seed, id, revision))),
        (None, None) => Ok(None),
        _ => Err(ReadFailure::ComponentCorruption("partial span reference")),
    }
}
pub(super) fn task_ref(
    db: &Connection,
    seed: &str,
    id: DecimalU64,
    revision: DecimalU64,
) -> Result<TaskScopeRef> {
    let mut statement=db.prepare("SELECT local_id,revision FROM span WHERE parent_local_id=?1 AND parent_revision=?2 AND kind='task_prompt'")?;
    let mut rows = statement.query(params![blob(id), blob(revision)])?;
    let row = rows
        .next()?
        .ok_or(ReadFailure::ComponentCorruption("task has no prompt child"))?;
    let prompt = span_ref(seed, counter(row, "local_id")?, counter(row, "revision")?);
    if rows.next()?.is_some() {
        return Err(ReadFailure::ComponentCorruption(
            "task has multiple prompt children",
        ));
    }
    Ok(TaskScopeRef {
        task: span_ref(seed, id, revision),
        prompt,
    })
}
fn info(db: &Connection, seed: &str, row: &Row<'_>) -> Result<SpanInfo> {
    let kind = match row.get::<_, String>("kind")?.as_str() {
        "system_prompt" => SpanKind::SystemPrompt,
        "task" => SpanKind::Task,
        "task_prompt" => SpanKind::TaskPrompt,
        "subtask" => SpanKind::Subtask,
        "step" => SpanKind::Step,
        "step_outcome" => SpanKind::StepOutcome,
        _ => return Err(ReadFailure::ComponentCorruption("unknown span kind")),
    };
    let status = match row.get::<_, String>("status")?.as_str() {
        "open" => SpanStatus::Open,
        "closed" => SpanStatus::Closed,
        "retired" => SpanStatus::Retired,
        _ => return Err(ReadFailure::ComponentCorruption("unknown span status")),
    };
    Ok(SpanInfo {
        span: span_ref(seed, counter(row, "local_id")?, counter(row, "revision")?),
        parent: optional_span(row, seed, "parent_local_id", "parent_revision")?,
        kind,
        status,
        opened_source: row
            .get::<_, Option<i64>>("opened_content_id")?
            .map(|id| source_id(db, id))
            .transpose()?,
        ended_source: row
            .get::<_, Option<i64>>("ended_content_id")?
            .map(|id| source_id(db, id))
            .transpose()?,
        replacement: optional_span(row, seed, "replacement_local_id", "replacement_revision")?,
    })
}
pub(super) fn marks(db: &Connection, seed: &str, out: &mut Output) -> Result<()> {
    let next = checked_next(db, "SELECT MAX(local_id) AS largest FROM span")?;
    let mut statement = db.prepare(
        "SELECT local_id,revision FROM span WHERE kind='task' ORDER BY local_id DESC LIMIT 1",
    )?;
    let mut rows = statement.query([])?;
    let task = rows
        .next()?
        .map(|row| {
            task_ref(
                db,
                seed,
                counter(row, "local_id")?,
                counter(row, "revision")?,
            )
        })
        .transpose()?;
    out.raw("{\"marks\":{\"next_span_id\":")?;
    value!(out, next)?;
    out.raw(",\"current_task\":")?;
    value!(out, task)?;
    out.raw(",\"open_scopes\":[")?;
    let mut statement = db.prepare("SELECT * FROM span WHERE status='open' ORDER BY local_id")?;
    let mut rows = statement.query([])?;
    let mut first = true;
    let mut previous: Option<SpanRef> = None;
    while let Some(row) = rows.next()? {
        let value = info(db, seed, row)?;
        if value.parent != previous
            || (previous.is_none() && value.kind != SpanKind::Task)
            || value.opened_source.is_none()
        {
            return Err(ReadFailure::ComponentCorruption(
                "impossible open span path",
            ));
        }
        if let Some(parent) = &previous
            && parent.local_id >= value.span.local_id
        {
            return Err(ReadFailure::ComponentCorruption(
                "span parent does not precede child",
            ));
        }
        if previous.is_none() && task.as_ref().is_none_or(|task| task.task != value.span) {
            return Err(ReadFailure::ComponentCorruption(
                "open root is not current task",
            ));
        }
        previous = Some(value.span.clone());
        out.comma(&mut first)?;
        value!(out, value)?;
    }
    out.raw("]}}")
}
