//! Store-owned span construction and immutable canonical membership.

use rusqlite::{Connection, OptionalExtension, Row, Transaction, named_params, params};
use weaver_types::state_flow::{
    ContentRef, MarkEffect, SpanKind, SpanRef, SpanStatus, SpanText, StatementFile, StatementRef,
    StepQueryRef,
};

use super::super::ServingContext;

#[path = "marks_image.rs"]
mod image;
use super::{blob, decode_counter, invalid_effect, source_id};
use crate::FileFault;
pub(in crate::engine::serving_sqlite) use image::verify_image_marks;

/// The exact STATE-MARK-MEMBERS constructor. Readers stream this query into their
/// bounded result writer instead of collecting a second unbounded member array.
pub(in crate::engine::serving_sqlite) const MEMBERS_SQL: &str = r#"
WITH RECURSIVE descendants(local_id, revision, opened_content_id) AS (
  SELECT local_id, revision, opened_content_id FROM span
  WHERE local_id = :span_id AND revision = :span_rev
  UNION ALL
  SELECT s.local_id, s.revision, s.opened_content_id FROM span s
  JOIN descendants d ON s.parent_local_id = d.local_id
    AND s.parent_revision = d.revision
), members AS (
  SELECT d.opened_content_id AS anchor, d.local_id AS prefix_span,
         p.ordinal AS prefix_ordinal, p.block_id, p.start, p.end, p.step_query
  FROM descendants d JOIN span_prefix p
    ON p.local_id = d.local_id AND p.revision = d.revision
  WHERE d.opened_content_id >= :open_id AND d.opened_content_id <= :upper_id
  UNION ALL
  SELECT b.id, NULL, NULL, b.id, NULL, NULL, NULL FROM content_block b
  WHERE b.id > :open_id
    AND (b.id < :upper_id OR (:inclusive_work = 1 AND b.id = :upper_id))
    AND b.content_type = 'statement'
    AND b.kind IN ('message.user','message.assistant','loop.instruction')
)
SELECT anchor, prefix_span, prefix_ordinal, block_id, start, end, step_query
FROM members
ORDER BY anchor, CASE WHEN prefix_span IS NULL THEN 1 ELSE 0 END,
         prefix_span, prefix_ordinal
"#;

#[derive(Clone, Debug)]
pub(in crate::engine::serving_sqlite) struct MemberRow {
    pub block_id: i64,
    pub start: u64,
    pub end: u64,
    pub step_query: Option<StepQueryRef>,
}

#[derive(Debug)]
pub(in crate::engine::serving_sqlite) enum IntervalFault {
    PartialBinding,
    PartialPair,
    PendingPair,
    Malformed,
    Sqlite(rusqlite::Error),
}

impl From<rusqlite::Error> for IntervalFault {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl IntervalFault {
    pub(super) fn into_file(self) -> FileFault {
        match self {
            Self::Sqlite(error) => error.into(),
            other => invalid_effect(format!("invalid span interval: {other:?}")),
        }
    }
}

pub(in crate::engine::serving_sqlite) fn member_from_row(
    connection: &Connection,
    row: &Row<'_>,
) -> Result<MemberRow, FileFault> {
    let block_id = row.get(3)?;
    let prefix: Option<Vec<u8>> = row.get(1)?;
    let (start, end) = if prefix.is_some() {
        (decode_counter(row.get(4)?)?, decode_counter(row.get(5)?)?)
    } else {
        let text: String = connection.query_row(
            "SELECT statement_text FROM content_block WHERE id=?1 AND content_type='statement'",
            [block_id],
            |row| row.get(0),
        )?;
        (0, text.len() as u64)
    };
    let query: Option<String> = row.get(6)?;
    let step_query = query
        .map(|json| weaver_types::state_flow::decode_strict(&json))
        .transpose()
        .map_err(|_| invalid_effect("stored step query is malformed"))?;
    Ok(MemberRow {
        block_id,
        start,
        end,
        step_query,
    })
}

pub(in crate::engine::serving_sqlite) fn validate_interval(
    connection: &Connection,
    block_id: i64,
    start: u64,
    end: u64,
    frontier: i64,
) -> Result<(), IntervalFault> {
    let text: Option<String> = connection
        .query_row(
            "SELECT statement_text FROM content_block WHERE id=?1 AND content_type='statement'",
            [block_id],
            |row| row.get(0),
        )
        .optional()?;
    let text = text.ok_or(IntervalFault::Malformed)?;
    let start_index = usize::try_from(start).map_err(|_| IntervalFault::Malformed)?;
    let end_index = usize::try_from(end).map_err(|_| IntervalFault::Malformed)?;
    if start >= end || text.get(start_index..end_index).is_none() {
        return Err(IntervalFault::Malformed);
    }
    let mut statement = connection
        .prepare("SELECT id,start,end FROM statement_binding WHERE block_id=?1 ORDER BY ordinal")?;
    let mut rows = statement.query([block_id])?;
    while let Some(row) = rows.next()? {
        let binding_id: i64 = row.get(0)?;
        let lower = decode_counter(row.get(1)?).map_err(|_| IntervalFault::Malformed)?;
        let upper = decode_counter(row.get(2)?).map_err(|_| IntervalFault::Malformed)?;
        if upper <= start || lower >= end {
            continue;
        }
        let mut pairs = connection.prepare("SELECT terminal_event FROM tool_pair INDEXED BY pair_statement WHERE statement_id=?1 AND opened_event<=?2 ORDER BY opened_event")?;
        let terminals = pairs.query_map(params![binding_id, frontier], |row| {
            row.get::<_, Option<i64>>(0)
        })?;
        let mut has_pair = false;
        let mut pending = false;
        for terminal in terminals {
            has_pair = true;
            if terminal?.is_none_or(|terminal| terminal > frontier) {
                pending = true;
            }
        }
        if lower < start || upper > end {
            return Err(if has_pair {
                IntervalFault::PartialPair
            } else {
                IntervalFault::PartialBinding
            });
        }
        if pending {
            return Err(IntervalFault::PendingPair);
        }
    }
    Ok(())
}

pub(super) struct HeldStatement {
    pub id: i64,
    pub block_id: i64,
    pub text: String,
    pub start: u64,
    pub end: u64,
}

type HeldStatementRow = (i64, i64, String, Vec<u8>, Vec<u8>);

pub(super) fn resolve_statement(
    connection: &Connection,
    reference: &StatementRef,
) -> Result<HeldStatement, FileFault> {
    let row: Option<HeldStatementRow> = connection
        .query_row(
            "SELECT s.id,b.id,b.statement_text,s.start,s.end FROM statement_binding s
         JOIN content_block b ON b.id=s.block_id WHERE b.source_kind='event'
         AND b.run=?1 AND b.sequence=?2 AND b.block=?3 AND s.ordinal=?4",
            params![
                reference.block_id.run,
                blob(reference.block_id.sequence.0),
                blob(reference.block_id.block.0),
                blob(reference.binding.0)
            ],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let (id, block_id, text, start, end) =
        row.ok_or_else(|| invalid_effect("statement reference is not held"))?;
    Ok(HeldStatement {
        id,
        block_id,
        text,
        start: decode_counter(start)?,
        end: decode_counter(end)?,
    })
}

pub(super) fn resolve_content(
    connection: &Connection,
    seed: &str,
    reference: &ContentRef,
) -> Result<i64, FileFault> {
    let id = match reference {
        ContentRef::Seed { seed_id, ordinal } => {
            if seed_id != seed {
                return Err(invalid_effect("content names another seed"));
            }
            connection
                .query_row(
                    "SELECT id FROM content_block WHERE source_kind='seed' AND seed_ordinal=?1",
                    [blob(ordinal.0)],
                    |row| row.get(0),
                )
                .optional()?
        }
        ContentRef::Event { block_id } => source_id(connection, block_id)?,
    };
    id.ok_or_else(|| invalid_effect("content reference is not held"))
}

pub(super) fn next_span(connection: &Connection, seed: &str) -> Result<SpanRef, FileFault> {
    let max: Option<Vec<u8>> =
        connection.query_row("SELECT MAX(local_id) FROM span", [], |row| row.get(0))?;
    let next = max
        .map(decode_counter)
        .transpose()?
        .ok_or_else(|| invalid_effect("seed span missing"))?
        .checked_add(1)
        .ok_or_else(|| invalid_effect("span counter exhausted"))?;
    Ok(SpanRef {
        seed_id: seed.into(),
        local_id: next.into(),
        revision: 1.into(),
    })
}

fn check_span(context: &ServingContext, span: &SpanRef) -> Result<(), FileFault> {
    if span.seed_id != context.seed_id || span.revision.0 != 1 {
        return Err(invalid_effect("span names another seed or revision"));
    }
    Ok(())
}

fn check_next(
    connection: &Connection,
    context: &ServingContext,
    span: &SpanRef,
) -> Result<(), FileFault> {
    check_span(context, span)?;
    if *span != next_span(connection, &context.seed_id)? {
        return Err(invalid_effect("span allocation is not consecutive"));
    }
    Ok(())
}

pub(super) fn apply_direct(
    tx: &Transaction<'_>,
    context: &ServingContext,
    filing: &StatementFile,
    source_id: i64,
    effect: &MarkEffect,
) -> Result<(), FileFault> {
    let MarkEffect::BeginTask {
        task,
        prompt,
        source,
    } = effect
    else {
        return Err(invalid_effect("mark is not directly licensed"));
    };
    let held_id = resolve_content(tx, &context.seed_id, source)?;
    let (kind, turn, run): (Option<String>, Option<String>, Option<String>) = tx.query_row(
        "SELECT kind,turn,run FROM content_block WHERE id=?1",
        [held_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if held_id >= source_id
        || kind.as_deref() != Some("message.user")
        || turn.is_none()
        || turn != filing.source.turn
        || run.as_deref() != Some(context.run_id.as_str())
    {
        return Err(invalid_effect(
            "task source is not an earlier input of this turn",
        ));
    }
    begin_task(tx, context, source_id, task, prompt, source)
}

pub(super) fn begin_task(
    tx: &Transaction<'_>,
    context: &ServingContext,
    source_id: i64,
    task: &SpanRef,
    prompt: &SpanRef,
    source: &ContentRef,
) -> Result<(), FileFault> {
    check_next(tx, context, task)?;
    let open: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM span WHERE status='open')",
        [],
        |row| row.get(0),
    )?;
    if open {
        return Err(invalid_effect("a task path is still open"));
    }
    let content_id = resolve_content(tx, &context.seed_id, source)?;
    if content_id >= source_id {
        return Err(invalid_effect("task text must already be held"));
    }
    let text: String = tx
        .query_row(
            "SELECT statement_text FROM content_block WHERE id=?1 AND content_type='statement'",
            [content_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| invalid_effect("task prefix is not held statement text"))?;
    validate_interval(tx, content_id, 0, text.len() as u64, source_id)
        .map_err(IntervalFault::into_file)?;
    tx.execute("INSERT INTO span(local_id,revision,kind,status,opened_content_id) VALUES (?1,?2,'task','open',?3)", params![blob(task.local_id.0),blob(task.revision.0),source_id])?;
    check_next(tx, context, prompt)?;
    tx.execute("INSERT INTO span(local_id,revision,kind,parent_local_id,parent_revision,status,opened_content_id,ended_content_id)
        VALUES (?1,?2,'task_prompt',?3,?4,'closed',?5,?5)",
        params![blob(prompt.local_id.0),blob(prompt.revision.0),blob(task.local_id.0),blob(task.revision.0),source_id])?;
    for table in ["span_prefix", "span_member"] {
        tx.execute(&format!("INSERT INTO {table}(local_id,revision,ordinal,block_id,start,end,step_query) VALUES (?1,?2,?3,?4,?3,?5,NULL)"),
            params![blob(prompt.local_id.0),blob(prompt.revision.0),blob(0),content_id,blob(text.len() as u64)])?;
    }
    Ok(())
}

pub(super) fn open(
    tx: &Transaction<'_>,
    context: &ServingContext,
    source_id: i64,
    span: &SpanRef,
    parent: &SpanRef,
    kind: SpanKind,
    prefix: &[SpanText],
) -> Result<(), FileFault> {
    check_next(tx, context, span)?;
    check_span(context, parent)?;
    let (parent_kind, parent_status): (String, String) = tx
        .query_row(
            "SELECT kind,status FROM span WHERE local_id=?1 AND revision=?2",
            params![blob(parent.local_id.0), blob(parent.revision.0)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| invalid_effect("parent span is not held"))?;
    let allowed = match kind {
        SpanKind::Subtask => parent_kind == "task" && prefix.is_empty(),
        SpanKind::Step => {
            matches!(parent_kind.as_str(), "task" | "subtask")
                && prefix.len() == 1
                && prefix[0].step_query.is_some()
        }
        SpanKind::StepOutcome => {
            parent_kind == "step" && prefix.len() == 1 && prefix[0].step_query.is_none()
        }
        _ => false,
    };
    let child_open: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM span WHERE parent_local_id=?1 AND parent_revision=?2 AND status='open')",params![blob(parent.local_id.0),blob(parent.revision.0)],|row|row.get(0))?;
    if !allowed || parent_status != "open" || child_open || parent.local_id >= span.local_id {
        return Err(invalid_effect(
            "span kind, prefix or parent is not at the open leaf",
        ));
    }
    let kind = match kind {
        SpanKind::Subtask => "subtask",
        SpanKind::Step => "step",
        SpanKind::StepOutcome => "step_outcome",
        _ => return Err(invalid_effect("invalid runtime span kind")),
    };
    tx.execute("INSERT INTO span(local_id,revision,kind,parent_local_id,parent_revision,status,opened_content_id) VALUES (?1,?2,?3,?4,?5,'open',?6)",params![blob(span.local_id.0),blob(span.revision.0),kind,blob(parent.local_id.0),blob(parent.revision.0),source_id])?;
    for (ordinal, member) in prefix.iter().enumerate() {
        let content_id = resolve_content(tx, &context.seed_id, &member.source)?;
        if content_id > source_id {
            return Err(invalid_effect("prefix source lies after the open"));
        }
        validate_interval(tx, content_id, member.start.0, member.end.0, source_id)
            .map_err(IntervalFault::into_file)?;
        let query = member
            .step_query
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(super::json_fault)?;
        tx.execute("INSERT INTO span_prefix(local_id,revision,ordinal,block_id,start,end,step_query) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![blob(span.local_id.0),blob(span.revision.0),blob(ordinal as u64),content_id,blob(member.start.0),blob(member.end.0),query])?;
    }
    Ok(())
}

pub(in crate::engine::serving_sqlite) fn finish(
    tx: &Transaction<'_>,
    context: &ServingContext,
    source_id: i64,
    span: &SpanRef,
    status: SpanStatus,
    replacement: Option<&SpanRef>,
) -> Result<(), FileFault> {
    check_span(context, span)?;
    if status == SpanStatus::Open || (status == SpanStatus::Closed && replacement.is_some()) {
        return Err(invalid_effect("invalid terminal span disposition"));
    }
    let (kind, prior, opened): (String, String, Option<i64>) = tx
        .query_row(
            "SELECT kind,status,opened_content_id FROM span WHERE local_id=?1 AND revision=?2",
            params![blob(span.local_id.0), blob(span.revision.0)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| invalid_effect("span is not held"))?;
    let opened = opened.ok_or_else(|| invalid_effect("seed span is immutable"))?;
    let child_open: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM span WHERE parent_local_id=?1 AND parent_revision=?2 AND status='open')",params![blob(span.local_id.0),blob(span.revision.0)],|row|row.get(0))?;
    if prior != "open" || child_open || source_id < opened {
        return Err(invalid_effect("span cannot end before its open children"));
    }
    if let Some(replacement) = replacement {
        check_span(context, replacement)?;
        if replacement == span {
            return Err(invalid_effect("span cannot replace itself"));
        }
        let held_kind: Option<String> = tx
            .query_row(
                "SELECT kind FROM span WHERE local_id=?1 AND revision=?2",
                params![blob(replacement.local_id.0), blob(replacement.revision.0)],
                |row| row.get(0),
            )
            .optional()?;
        if held_kind
            .as_ref()
            .is_some_and(|replacement_kind| replacement_kind != &kind)
        {
            return Err(invalid_effect("replacement span has a different kind"));
        }
        // A forward reference is checked by the deferred FK at commit. Plan
        // replacement owns its same-kind construction within this transaction.
    }
    let mut statement = tx.prepare(MEMBERS_SQL)?;
    let mut rows = statement.query(named_params!{":span_id":blob(span.local_id.0),":span_rev":blob(span.revision.0),":open_id":opened,":upper_id":source_id,":inclusive_work":0})?;
    let mut ordinal = 0_u64;
    while let Some(row) = rows.next()? {
        let member = member_from_row(tx, row)?;
        validate_interval(tx, member.block_id, member.start, member.end, source_id)
            .map_err(IntervalFault::into_file)?;
        let query = member
            .step_query
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(super::json_fault)?;
        tx.execute("INSERT INTO span_member(local_id,revision,ordinal,block_id,start,end,step_query) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![blob(span.local_id.0),blob(span.revision.0),blob(ordinal),member.block_id,blob(member.start),blob(member.end),query])?;
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| invalid_effect("member ordinal exhausted"))?;
    }
    let status = if status == SpanStatus::Closed {
        "closed"
    } else {
        "retired"
    };
    let changed = tx.execute("UPDATE span AS s SET status=?1,ended_content_id=?2,replacement_local_id=?3,replacement_revision=?4
        WHERE s.local_id=?5 AND s.revision=?6 AND s.status='open'
        AND NOT EXISTS(SELECT 1 FROM span child WHERE child.parent_local_id=s.local_id AND child.parent_revision=s.revision AND child.status='open')",
        params![status,source_id,replacement.map(|span|blob(span.local_id.0)),replacement.map(|span|blob(span.revision.0)),blob(span.local_id.0),blob(span.revision.0)])?;
    if changed != 1 {
        return Err(invalid_effect("span terminal guard changed no row"));
    }
    Ok(())
}

pub(super) fn retire_open_path(
    tx: &Transaction<'_>,
    context: &ServingContext,
    source_id: i64,
) -> Result<(), FileFault> {
    loop {
        let next: Option<Vec<u8>> = tx
            .query_row(
                "SELECT local_id FROM span WHERE status='open' ORDER BY local_id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(local_id) = next else {
            return Ok(());
        };
        let span = SpanRef {
            seed_id: context.seed_id.clone(),
            local_id: decode_counter(local_id)?.into(),
            revision: 1.into(),
        };
        finish(tx, context, source_id, &span, SpanStatus::Retired, None)?;
    }
}
