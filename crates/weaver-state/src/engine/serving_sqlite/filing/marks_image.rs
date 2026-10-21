//! Structural mark adoption. No image is repaired or re-labelled here.

use rusqlite::{Connection, OptionalExtension, named_params, params};
use weaver_types::state_flow::{
    MarkEffect, PlanEffect, PlanLockSource, SpanRef, StatementEffect, StepQueryRef, decode_strict,
};

use super::{
    MEMBERS_SQL, blob, decode_counter, invalid_effect, member_from_row, resolve_statement,
    validate_interval,
};
use crate::FileFault;

struct SpanRow {
    id: u64,
    revision: u64,
    kind: String,
    parent: Option<u64>,
    parent_revision: Option<u64>,
    status: String,
    opened: Option<i64>,
    ended: Option<i64>,
    replacement: Option<u64>,
    replacement_revision: Option<u64>,
}

pub(in crate::engine::serving_sqlite) fn verify_image_marks(
    connection: &Connection,
    seed: &str,
) -> Result<(), FileFault> {
    let mut query = connection.prepare("SELECT local_id,revision,kind,parent_local_id,parent_revision,status,opened_content_id,ended_content_id,replacement_local_id,replacement_revision FROM span ORDER BY local_id")?;
    let mut rows = query.query([])?;
    let mut expected = 0_u64;
    let mut root_seen = false;
    while let Some(row) = rows.next()? {
        let span = SpanRow {
            id: decode_counter(row.get(0)?)?,
            revision: decode_counter(row.get(1)?)?,
            kind: row.get(2)?,
            parent: row
                .get::<_, Option<Vec<u8>>>(3)?
                .map(decode_counter)
                .transpose()?,
            parent_revision: row
                .get::<_, Option<Vec<u8>>>(4)?
                .map(decode_counter)
                .transpose()?,
            status: row.get(5)?,
            opened: row.get(6)?,
            ended: row.get(7)?,
            replacement: row
                .get::<_, Option<Vec<u8>>>(8)?
                .map(decode_counter)
                .transpose()?,
            replacement_revision: row
                .get::<_, Option<Vec<u8>>>(9)?
                .map(decode_counter)
                .transpose()?,
        };
        if span.id != expected || span.revision != 1 {
            return Err(invalid_effect(
                "image span allocations are not consecutive revision one",
            ));
        }
        expected = expected
            .checked_add(1)
            .ok_or_else(|| invalid_effect("image span allocation exhausted"))?;
        if span.id == 0 {
            verify_seed_span(connection, &span)?;
            root_seen = true;
            continue;
        }
        verify_parent(connection, &span)?;
        let opened = span
            .opened
            .ok_or_else(|| invalid_effect("runtime span lacks opening source"))?;
        let opening = source_effect(connection, opened)?;
        check_opening(seed, &span, &opening)?;
        verify_prefix(connection, seed, &span, &opening)?;
        if let Some(replacement) = span.replacement {
            if span.replacement_revision != Some(1)
                || replacement == span.id
                || span.status != "retired"
            {
                return Err(invalid_effect("invalid image replacement address"));
            }
            let kind: Option<String> = connection
                .query_row(
                    "SELECT kind FROM span WHERE local_id=?1 AND revision=?2",
                    params![blob(replacement), blob(1)],
                    |row| row.get(0),
                )
                .optional()?;
            if kind.as_deref() != Some(span.kind.as_str()) {
                return Err(invalid_effect(
                    "image replacement changes kind or is absent",
                ));
            }
        } else if span.replacement_revision.is_some() {
            return Err(invalid_effect("partial image replacement address"));
        }
        if span.kind == "task" {
            let prompts:i64=connection.query_row("SELECT COUNT(*) FROM span WHERE parent_local_id=?1 AND parent_revision=?2 AND kind='task_prompt'",params![blob(span.id),blob(1)],|row|row.get(0))?;
            if prompts != 1 {
                return Err(invalid_effect("task lacks its unique prompt child"));
            }
        }
        if span.status == "open" {
            let members: i64 = connection.query_row(
                "SELECT COUNT(*) FROM span_member WHERE local_id=?1 AND revision=?2",
                params![blob(span.id), blob(1)],
                |row| row.get(0),
            )?;
            if span.ended.is_some() || members != 0 {
                return Err(invalid_effect("open image span has finalized membership"));
            }
        } else {
            let ended = span
                .ended
                .ok_or_else(|| invalid_effect("terminal image span lacks its end"))?;
            if ended < opened {
                return Err(invalid_effect("terminal end precedes its opening"));
            }
            if span.kind == "task_prompt" {
                if ended != opened || span.status != "closed" {
                    return Err(invalid_effect("task prompt must end at its opening"));
                }
                compare_prefix_members(connection, &span)?;
            } else {
                let ending = source_effect(connection, ended)?;
                check_ending(connection, seed, &span, &ending)?;
                compare_canonical_members(connection, &span, ended)?;
            }
        }
    }
    if !root_seen {
        return Err(invalid_effect("image has no seeded system span"));
    }
    verify_effect_spans(connection, seed)?;
    let roots: i64 = connection.query_row(
        "SELECT COUNT(*) FROM span WHERE status='open' AND parent_local_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    if roots > 1 {
        return Err(invalid_effect("image has crossed open paths"));
    }
    let crossed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM span a JOIN span b
         ON a.parent_local_id IS b.parent_local_id AND a.parent_revision IS b.parent_revision
         WHERE a.local_id < b.local_id AND a.local_id != ?1
         AND a.kind != 'task_prompt' AND b.kind != 'task_prompt'
         AND (a.ended_content_id IS NULL OR a.ended_content_id > b.opened_content_id))",
        [blob(0)],
        |row| row.get(0),
    )?;
    if crossed {
        return Err(invalid_effect(
            "image contains crossed sibling execution intervals",
        ));
    }
    Ok(())
}

// Row-to-source checks cannot detect deleted output rows. Each retained effect
// must also still have its immutable materialized span outputs. Some plan
// operations reuse an existing scope, so those references are checked against
// their historical frontier rather than assumed to allocate a fresh span.
fn verify_effect_spans(connection: &Connection, seed: &str) -> Result<(), FileFault> {
    let mut query =
        connection.prepare("SELECT id FROM content_block WHERE effect IS NOT NULL ORDER BY id")?;
    let mut rows = query.query([])?;
    while let Some(row) = rows.next()? {
        let source: i64 = row.get(0)?;
        match source_effect(connection, source)? {
            StatementEffect::Mark(MarkEffect::BeginTask {
                task,
                prompt,
                source: input,
            }) => {
                require_task_scope(connection, seed, &task, &prompt, source, true)?;
                let input = super::resolve_content(connection, seed, &input)?;
                let eligible: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM content_block input
                     JOIN content_block effect ON effect.id=?1
                     WHERE input.id=?2 AND input.id < effect.id
                     AND input.kind='message.user' AND input.run=effect.run
                     AND input.turn IS NOT NULL AND input.turn=effect.turn)",
                    params![source, input],
                    |row| row.get(0),
                )?;
                if !eligible {
                    return Err(invalid_effect(
                        "begin-task source is not an earlier input of its turn",
                    ));
                }
            }
            StatementEffect::Mark(_) => {
                return Err(invalid_effect("image contains an unlicensed mark effect"));
            }
            StatementEffect::Plan(effect) => match effect.as_ref() {
                PlanEffect::Authorize { task, .. } => {
                    require_task_scope(connection, seed, &task.task, &task.prompt, source, true)?;
                }
                PlanEffect::Lock {
                    task, source: lock, ..
                } => {
                    let new = matches!(lock, PlanLockSource::Operator { .. });
                    let opened = require_task_scope(
                        connection,
                        seed,
                        &task.task,
                        &task.prompt,
                        source,
                        new,
                    )?;
                    if !new && opened >= source {
                        return Err(invalid_effect(
                            "model lock did not retain its authorized task scope",
                        ));
                    }
                }
                PlanEffect::Replace { task, .. } => {
                    require_task_scope(connection, seed, &task.task, &task.prompt, source, false)?;
                }
                PlanEffect::Activate {
                    step_span,
                    group_span,
                    ..
                } => {
                    require_effect_span(connection, seed, step_span, "step", source, true)?;
                    if let Some(group) = group_span {
                        require_effect_span(connection, seed, group, "subtask", source, false)?;
                        require_parent(connection, step_span, group)?;
                    }
                }
                PlanEffect::Close {
                    work, outcome_span, ..
                } => {
                    require_effect_span(
                        connection,
                        seed,
                        outcome_span,
                        "step_outcome",
                        source,
                        true,
                    )?;
                    require_effect_span(connection, seed, &work.span, "step", source, false)?;
                    require_parent(connection, outcome_span, &work.span)?;
                    for reference in [&work.span, outcome_span] {
                        let closed: bool = connection.query_row(
                            "SELECT status='closed' AND ended_content_id=?1 FROM span
                             WHERE local_id=?2 AND revision=?3",
                            params![source, blob(reference.local_id.0), blob(1)],
                            |row| row.get(0),
                        )?;
                        if !closed {
                            return Err(invalid_effect(
                                "close effect lost its terminal step or outcome",
                            ));
                        }
                    }
                }
                PlanEffect::Break { .. } => {}
            },
        }
    }
    Ok(())
}

fn require_effect_span(
    connection: &Connection,
    seed: &str,
    reference: &SpanRef,
    kind: &str,
    source: i64,
    allocated_here: bool,
) -> Result<i64, FileFault> {
    if reference.seed_id != seed || reference.revision.0 != 1 {
        return Err(invalid_effect("effect names another seed or span revision"));
    }
    let opened: Option<i64> = connection
        .query_row(
            "SELECT opened_content_id FROM span
             WHERE local_id=?1 AND revision=?2 AND kind=?3",
            params![blob(reference.local_id.0), blob(1), kind],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let opened = opened.ok_or_else(|| invalid_effect("effect lost its required span"))?;
    if opened > source || (allocated_here && opened != source) {
        return Err(invalid_effect("effect span has a different opening source"));
    }
    Ok(opened)
}

fn require_task_scope(
    connection: &Connection,
    seed: &str,
    task: &SpanRef,
    prompt: &SpanRef,
    source: i64,
    allocated_here: bool,
) -> Result<i64, FileFault> {
    let opened = require_effect_span(connection, seed, task, "task", source, allocated_here)?;
    let prompt_opened = require_effect_span(
        connection,
        seed,
        prompt,
        "task_prompt",
        source,
        allocated_here,
    )?;
    if prompt_opened != opened || task.local_id.0.checked_add(1) != Some(prompt.local_id.0) {
        return Err(invalid_effect(
            "effect task scope lost its paired allocation",
        ));
    }
    require_parent(connection, prompt, task)?;
    Ok(opened)
}

fn require_parent(
    connection: &Connection,
    child: &SpanRef,
    parent: &SpanRef,
) -> Result<(), FileFault> {
    let matches: bool = connection.query_row(
        "SELECT parent_local_id=?1 AND parent_revision=?2 FROM span
         WHERE local_id=?3 AND revision=?4",
        params![
            blob(parent.local_id.0),
            blob(1),
            blob(child.local_id.0),
            blob(1)
        ],
        |row| row.get(0),
    )?;
    if !matches {
        return Err(invalid_effect("effect span lost its declared parent"));
    }
    Ok(())
}

fn verify_seed_span(connection: &Connection, span: &SpanRow) -> Result<(), FileFault> {
    if span.kind != "system_prompt"
        || span.status != "closed"
        || span.parent.is_some()
        || span.opened.is_some()
        || span.ended.is_some()
        || span.replacement.is_some()
    {
        return Err(invalid_effect(
            "span zero is not the sole source-free closed system span",
        ));
    }
    let source:Option<(i64,String)>=connection.query_row("SELECT id,statement_text FROM content_block WHERE source_kind='seed' AND seed_ordinal=?1",[blob(0)],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    let (id, text) = source.ok_or_else(|| invalid_effect("system span lacks seed content"))?;
    for table in ["span_prefix", "span_member"] {
        let mut statement = connection.prepare(&format!(
            "SELECT ordinal,block_id,start,end,step_query FROM {table}
             WHERE local_id=?1 AND revision=?2 ORDER BY ordinal"
        ))?;
        let mut rows = statement.query(params![blob(0), blob(1)])?;
        let Some(row) = rows.next()? else {
            return Err(invalid_effect("system span has no prompt member"));
        };
        let block_id: i64 = row.get(1)?;
        let step_query: Option<String> = row.get(4)?;
        if decode_counter(row.get(0)?)? != 0
            || block_id != id
            || decode_counter(row.get(2)?)? != 0
            || decode_counter(row.get(3)?)? != text.len() as u64
            || step_query.is_some()
            || rows.next()?.is_some()
        {
            return Err(invalid_effect(
                "system span does not name exactly its complete prompt",
            ));
        }
    }
    Ok(())
}

fn verify_parent(connection: &Connection, span: &SpanRow) -> Result<(), FileFault> {
    if span.kind == "task" {
        if span.parent.is_some() || span.parent_revision.is_some() {
            return Err(invalid_effect("task is not a root span"));
        }
        return Ok(());
    }
    let parent = span
        .parent
        .ok_or_else(|| invalid_effect("runtime child lacks parent"))?;
    if span.parent_revision != Some(1) || parent >= span.id {
        return Err(invalid_effect(
            "span parent is not an earlier revision-one span",
        ));
    }
    let (kind,status,opened,ended):(String,String,Option<i64>,Option<i64>)=connection.query_row("SELECT kind,status,opened_content_id,ended_content_id FROM span WHERE local_id=?1 AND revision=?2",params![blob(parent),blob(1)],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    let allowed = match span.kind.as_str() {
        "task_prompt" | "subtask" => kind == "task",
        "step" => kind == "task" || kind == "subtask",
        "step_outcome" => kind == "step",
        _ => false,
    };
    if !allowed
        || (span.status == "open" && status != "open")
        || opened.is_none()
        || span.opened < opened
        || ended.is_some_and(|end| span.ended.is_none_or(|child| child > end))
    {
        return Err(invalid_effect(
            "span nesting, source interval or open path is inconsistent",
        ));
    }
    Ok(())
}

fn source_effect(connection: &Connection, id: i64) -> Result<StatementEffect, FileFault> {
    let (effect,raw,kind,channel,origin,block):(Option<String>,Option<String>,String,String,String,Vec<u8>)=connection.query_row("SELECT effect,context_raw,kind,channel,origin,block FROM content_block WHERE id=?1 AND source_kind='event' AND content_type='statement'",[id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)))?;
    if channel != "internal" || origin != "loop" || decode_counter(block)? != 0 {
        return Err(invalid_effect("span source is not licensed loop metadata"));
    }
    let effect: StatementEffect =
        decode_strict(&effect.ok_or_else(|| invalid_effect("span source lacks effect"))?)
            .map_err(|_| invalid_effect("span source effect is malformed"))?;
    let raw = raw.ok_or_else(|| invalid_effect("span source lacks copied context"))?;
    match &effect {
        StatementEffect::Mark(mark)
            if kind == "state.mark" && matches!(mark, MarkEffect::BeginTask { .. }) =>
        {
            let context: MarkEffect =
                decode_strict(&raw).map_err(|_| invalid_effect("malformed mark source context"))?;
            if context != *mark {
                return Err(invalid_effect("mark source context differs from effect"));
            }
        }
        StatementEffect::Plan(plan)
            if (kind == "plan.event" && !matches!(plan.as_ref(), PlanEffect::Close { .. }))
                || (kind == "step.outcome"
                    && matches!(plan.as_ref(), PlanEffect::Close { .. })) =>
        {
            let context: PlanEffect =
                decode_strict(&raw).map_err(|_| invalid_effect("malformed plan source context"))?;
            if context != **plan {
                return Err(invalid_effect("plan source context differs from effect"));
            }
        }
        _ => return Err(invalid_effect("span source effect has an unlicensed kind")),
    }
    Ok(effect)
}

fn names(reference: &SpanRef, seed: &str, id: u64) -> bool {
    reference.seed_id == seed && reference.local_id.0 == id && reference.revision.0 == 1
}

fn check_opening(seed: &str, span: &SpanRow, effect: &StatementEffect) -> Result<(), FileFault> {
    let matches = match effect {
        StatementEffect::Mark(MarkEffect::BeginTask { task, prompt, .. }) => {
            (span.kind == "task" && names(task, seed, span.id))
                || (span.kind == "task_prompt"
                    && names(prompt, seed, span.id)
                    && span.parent == Some(task.local_id.0))
        }
        StatementEffect::Plan(effect) => match effect.as_ref() {
            PlanEffect::Authorize { task, .. }
            | PlanEffect::Lock { task, .. }
            | PlanEffect::Replace { task, .. } => {
                (span.kind == "task" && names(&task.task, seed, span.id))
                    || (span.kind == "task_prompt"
                        && names(&task.prompt, seed, span.id)
                        && span.parent == Some(task.task.local_id.0))
            }
            PlanEffect::Activate {
                step_span,
                group_span,
                ..
            } => {
                (span.kind == "step" && names(step_span, seed, span.id))
                    || (span.kind == "subtask"
                        && group_span
                            .as_ref()
                            .is_some_and(|group| names(group, seed, span.id)))
            }
            PlanEffect::Close { outcome_span, .. } => {
                span.kind == "step_outcome" && names(outcome_span, seed, span.id)
            }
            _ => false,
        },
        _ => false,
    };
    if !matches {
        return Err(invalid_effect(
            "span opening effect does not name its identity",
        ));
    }
    Ok(())
}

fn verify_prefix(
    connection: &Connection,
    seed: &str,
    span: &SpanRow,
    effect: &StatementEffect,
) -> Result<(), FileFault> {
    let mut query=connection.prepare("SELECT ordinal,block_id,start,end,step_query FROM span_prefix WHERE local_id=?1 AND revision=?2 ORDER BY ordinal")?;
    let mut rows = query.query(params![blob(span.id), blob(1)])?;
    let mut count = 0_u64;
    while let Some(row) = rows.next()? {
        let ordinal = decode_counter(row.get(0)?)?;
        let block: i64 = row.get(1)?;
        let start = decode_counter(row.get(2)?)?;
        let end = decode_counter(row.get(3)?)?;
        let raw: Option<String> = row.get(4)?;
        if ordinal != count || block > span.opened.unwrap_or(0) {
            return Err(invalid_effect(
                "span prefix is not consecutive held content",
            ));
        }
        validate_interval(connection, block, start, end, span.opened.unwrap_or(0))
            .map_err(super::IntervalFault::into_file)?;
        let step: Option<StepQueryRef> = raw
            .as_ref()
            .map(|raw| decode_strict(raw))
            .transpose()
            .map_err(|_| invalid_effect("prefix step reference is malformed"))?;
        if let Some(step) = &step
            && serde_json::to_string(step).map_err(super::super::json_fault)?
                != *raw.as_ref().unwrap()
        {
            return Err(invalid_effect("step prefix reference is not canonical"));
        }
        match span.kind.as_str() {
            "task_prompt" => {
                let length: i64 = connection.query_row(
                    "SELECT length(CAST(statement_text AS BLOB)) FROM content_block WHERE id=?1",
                    [block],
                    |row| row.get(0),
                )?;
                if start != 0 || end != length as u64 || step.is_some() {
                    return Err(invalid_effect("task prompt prefix is not whole text"));
                }
                if let StatementEffect::Mark(MarkEffect::BeginTask { source, .. }) = effect
                    && super::resolve_content(connection, seed, source)? != block
                {
                    return Err(invalid_effect(
                        "task prompt differs from its opening source",
                    ));
                }
            }
            "step" => {
                let StatementEffect::Plan(effect) = effect else {
                    return Err(invalid_effect("step prefix lacks activation"));
                };
                let PlanEffect::Activate { successor, .. } = effect.as_ref() else {
                    return Err(invalid_effect("step prefix lacks successor"));
                };
                if step.as_ref() != Some(successor) {
                    return Err(invalid_effect(
                        "step prefix differs from activation successor",
                    ));
                }
                let held = resolve_statement(connection, &successor.instruction)?;
                if held.block_id != block || held.start != start || held.end != end {
                    return Err(invalid_effect(
                        "step prefix changes its instruction interval",
                    ));
                }
            }
            "step_outcome" => {
                let length: i64 = connection.query_row(
                    "SELECT length(CAST(statement_text AS BLOB)) FROM content_block WHERE id=?1",
                    [block],
                    |row| row.get(0),
                )?;
                if Some(block) != span.opened
                    || start != 0
                    || end != length as u64
                    || step.is_some()
                {
                    return Err(invalid_effect(
                        "outcome prefix differs from closing statement",
                    ));
                }
            }
            _ => return Err(invalid_effect("unexpected span prefix")),
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| invalid_effect("prefix counter exhausted"))?;
    }
    let expected = if matches!(span.kind.as_str(), "task_prompt" | "step" | "step_outcome") {
        1
    } else {
        0
    };
    if count != expected {
        return Err(invalid_effect("span has the wrong prefix cardinality"));
    }
    Ok(())
}

fn check_ending(
    connection: &Connection,
    seed: &str,
    span: &SpanRow,
    effect: &StatementEffect,
) -> Result<(), FileFault> {
    let StatementEffect::Plan(effect) = effect else {
        return Err(invalid_effect("runtime end lacks its plan effect"));
    };
    let valid = if span.status == "retired" {
        matches!(
            effect.as_ref(),
            PlanEffect::Authorize { .. } | PlanEffect::Lock { .. } | PlanEffect::Replace { .. }
        )
    } else if span.status == "closed" {
        match effect.as_ref() {
            PlanEffect::Close {
                work, outcome_span, ..
            } => {
                if span.kind == "step_outcome" {
                    names(outcome_span, seed, span.id)
                } else if span.kind == "step" {
                    names(&work.span, seed, span.id)
                } else {
                    ancestor(connection, span.id, &work.span, seed)?
                }
            }
            PlanEffect::Replace { task, .. } => {
                span.kind == "task" && names(&task.task, seed, span.id)
            }
            _ => false,
        }
    } else {
        false
    };
    if !valid {
        return Err(invalid_effect(
            "terminal span does not match its ending effect",
        ));
    }
    Ok(())
}

fn ancestor(
    connection: &Connection,
    wanted: u64,
    descendant: &SpanRef,
    seed: &str,
) -> Result<bool, FileFault> {
    if descendant.seed_id != seed || descendant.revision.0 != 1 {
        return Ok(false);
    }
    let mut id = descendant.local_id.0;
    loop {
        if id == wanted {
            return Ok(true);
        }
        let parent: Option<Vec<u8>> = connection.query_row(
            "SELECT parent_local_id FROM span WHERE local_id=?1 AND revision=?2",
            params![blob(id), blob(1)],
            |row| row.get(0),
        )?;
        let Some(parent) = parent else {
            return Ok(false);
        };
        let parent = decode_counter(parent)?;
        if parent >= id {
            return Err(invalid_effect("cyclic span ancestry"));
        }
        id = parent;
    }
}

fn compare_prefix_members(connection: &Connection, span: &SpanRow) -> Result<(), FileFault> {
    let mismatch:bool=connection.query_row("SELECT EXISTS(SELECT ordinal,block_id,start,end,step_query FROM span_prefix WHERE local_id=?1 AND revision=?2 EXCEPT SELECT ordinal,block_id,start,end,step_query FROM span_member WHERE local_id=?1 AND revision=?2)
        OR EXISTS(SELECT ordinal,block_id,start,end,step_query FROM span_member WHERE local_id=?1 AND revision=?2 EXCEPT SELECT ordinal,block_id,start,end,step_query FROM span_prefix WHERE local_id=?1 AND revision=?2)",params![blob(span.id),blob(1)],|row|row.get(0))?;
    if mismatch {
        return Err(invalid_effect(
            "prompt finalized members differ from its prefix",
        ));
    }
    Ok(())
}

fn compare_canonical_members(
    connection: &Connection,
    span: &SpanRow,
    frontier: i64,
) -> Result<(), FileFault> {
    let mut expected = connection.prepare(MEMBERS_SQL)?;
    let mut expected=expected.query(named_params!{":span_id":blob(span.id),":span_rev":blob(1),":open_id":span.opened,":upper_id":frontier,":inclusive_work":0})?;
    let mut actual=connection.prepare("SELECT ordinal,block_id,start,end,step_query FROM span_member WHERE local_id=?1 AND revision=?2 ORDER BY ordinal")?;
    let mut actual = actual.query(params![blob(span.id), blob(1)])?;
    let mut ordinal = 0_u64;
    while let Some(row) = expected.next()? {
        let member = member_from_row(connection, row)?;
        validate_interval(
            connection,
            member.block_id,
            member.start,
            member.end,
            frontier,
        )
        .map_err(super::IntervalFault::into_file)?;
        let row = actual
            .next()?
            .ok_or_else(|| invalid_effect("terminal span omitted a canonical member"))?;
        let raw: Option<String> = row.get(4)?;
        let query = member
            .step_query
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(super::super::json_fault)?;
        if decode_counter(row.get(0)?)? != ordinal
            || row.get::<_, i64>(1)? != member.block_id
            || decode_counter(row.get(2)?)? != member.start
            || decode_counter(row.get(3)?)? != member.end
            || raw != query
        {
            return Err(invalid_effect(
                "terminal span membership differs from its canonical interval",
            ));
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| invalid_effect("member counter exhausted"))?;
    }
    if actual.next()?.is_some() {
        return Err(invalid_effect("terminal span contains extra members"));
    }
    Ok(())
}
