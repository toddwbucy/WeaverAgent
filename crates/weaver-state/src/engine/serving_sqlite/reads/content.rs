use super::*;
use serde_json::value::RawValue;

const HISTORY: &str = "SELECT b.id AS block_id,s.id AS binding_id FROM content_block b JOIN statement_binding s ON s.block_id=b.id WHERE s.id IN (SELECT n.binding_id FROM binding_noun n INDEXED BY noun_value WHERE n.value=?1 COLLATE BINARY) AND (?2 IS NULL OR s.verb=?2 COLLATE BINARY) AND (?3 IS NULL OR s.condition=?3 COLLATE BINARY) ORDER BY b.id,s.ordinal";
const SUBJECTS: &str = "SELECT b.id AS block_id,NULL AS binding_id FROM content_block b WHERE b.source_kind='event' AND b.content_type='statement' AND b.kind IN ('message.user','message.assistant') AND b.id IN (SELECT s.block_id FROM json_each(?1) q JOIN binding_noun n INDEXED BY noun_value ON n.value=q.value COLLATE BINARY JOIN statement_binding s ON s.id=n.binding_id) ORDER BY b.id";

// STATE-SUBJECTS needs one global pair order across every selected binding.
// Specialize PAIR-READ's indexed ownership join for a whole block so rows can
// stream under STATE-PAGES without a separate pair-ID vector or merge buffer.
pub(super) const WHOLE_PAIRS: &str = "SELECT p.*,o.envelope AS opening_envelope,o.raw_block AS opening_block,t.envelope AS terminal_envelope,t.raw_block AS terminal_block FROM tool_pair p INDEXED BY pair_statement JOIN statement_binding s ON s.id=p.statement_id JOIN content_block o ON o.id=p.opened_event LEFT JOIN content_block t ON t.id=p.terminal_event WHERE s.block_id=?1 AND (?2 IS NULL OR s.id=?2) AND (?3 IS NULL OR (s.start>=?3 AND s.end<=?4)) AND (?5 IS NULL OR p.opened_event<=?5) ORDER BY p.opened_event";

pub(super) fn history(
    db: &Connection,
    out: &mut Output,
    noun: &str,
    verb: Option<&str>,
    condition: Option<&str>,
) -> Result<()> {
    entries(db, out, HISTORY, params![noun, verb, condition])
}
pub(super) fn condition(db: &Connection, out: &mut Output, condition: &str) -> Result<()> {
    entries(
        db,
        out,
        "SELECT b.id AS block_id,s.id AS binding_id FROM content_block b JOIN statement_binding s ON s.block_id=b.id WHERE s.condition=?1 COLLATE BINARY ORDER BY b.id,s.ordinal",
        [condition],
    )
}
pub(super) fn subjects(db: &Connection, out: &mut Output, nouns: &[String]) -> Result<()> {
    // The selector has already enforced sorted uniqueness. Keep its array bounded
    // separately, since it is SQL input rather than part of the returned content.
    let nouns = serde_json::to_string(nouns)
        .map_err(|_| ReadFailure::ComponentCorruption("noun parameter encoding"))?;
    entries(db, out, SUBJECTS, [nouns])
}
pub(super) fn conversation(
    db: &Connection,
    out: &mut Output,
    last_turns: DecimalU64,
) -> Result<()> {
    let count:i64=db.query_row("SELECT COUNT(*) FROM (SELECT run,turn FROM content_block WHERE source_kind='event' AND content_type='statement' AND turn IS NOT NULL AND kind IN ('message.user','message.assistant') GROUP BY run,turn)",[],|row|row.get(0))?;
    let limit = last_turns.0.min(nonnegative(count)?.0) as i64;
    entries(
        db,
        out,
        "WITH chosen AS (SELECT run,turn,MAX(id) AS last_id FROM content_block WHERE source_kind='event' AND content_type='statement' AND turn IS NOT NULL AND kind IN ('message.user','message.assistant') GROUP BY run,turn ORDER BY last_id DESC LIMIT ?1) SELECT b.id AS block_id,NULL AS binding_id FROM content_block b JOIN chosen c ON b.run=c.run AND b.turn=c.turn WHERE b.content_type='statement' AND b.kind IN ('message.user','message.assistant') ORDER BY b.id",
        [limit],
    )
}
fn entries(
    db: &Connection,
    out: &mut Output,
    sql: &str,
    parameters: impl rusqlite::Params,
) -> Result<()> {
    out.raw("{\"entries\":{\"entries\":[")?;
    let mut statement = db.prepare(sql)?;
    let mut rows = statement.query(parameters)?;
    let mut first = true;
    while let Some(row) = rows.next()? {
        out.comma(&mut first)?;
        entry(
            db,
            out,
            row.get("block_id")?,
            row.get("binding_id")?,
            None,
            None,
        )?;
    }
    out.raw("]}}")
}
pub(super) fn resolve_statement(
    db: &Connection,
    statement: &StatementRef,
) -> Result<Option<(i64, i64)>> {
    Ok(db.query_row("SELECT b.id,s.id FROM content_block b JOIN statement_binding s ON s.block_id=b.id WHERE b.source_kind='event' AND b.content_type='statement' AND b.run=?1 AND b.sequence=?2 AND b.block=?3 AND s.ordinal=?4",params![statement.block_id.run,blob(statement.block_id.sequence),blob(statement.block_id.block),blob(statement.binding)],|row|Ok((row.get(0)?,row.get(1)?))).optional()?)
}
pub(super) fn statement(db: &Connection, out: &mut Output, statement: &StatementRef) -> Result<()> {
    let (block, binding) = resolve_statement(db, statement)?.ok_or(ReadRefusalCode::NotFound)?;
    out.raw("{\"entries\":{\"entries\":[")?;
    entry(db, out, block, Some(binding), None, None)?;
    out.raw("]}}")
}

pub(super) struct Interval<'a> {
    pub span: &'a SpanRef,
    pub start: DecimalU64,
    pub end: DecimalU64,
    pub frontier: Option<i64>,
}

pub(super) fn event_source(db: &Connection, id: i64, out: &Output) -> Result<EventSource> {
    let mut statement =
        db.prepare("SELECT * FROM content_block WHERE id=?1 AND source_kind='event'")?;
    let mut rows = statement.query([id])?;
    let row = rows
        .next()?
        .ok_or(ReadFailure::ComponentCorruption("missing event content"))?;
    let block_id = SourceBlockId {
        run: text_cell(row, "run", out)?.into(),
        sequence: counter(row, "sequence")?,
        block: counter(row, "block")?,
    };
    let envelope: SourceEnvelope = json_cell!(out, row, "envelope", SourceEnvelope);
    let turn: Option<String> = row.get("turn")?;
    if envelope.run != block_id.run
        || envelope.sequence != block_id.sequence
        || envelope.turn != turn
        || envelope.session != text_cell(row, "session", out)?
        || envelope.kind != text_cell(row, "kind", out)?
        || envelope.channel != text_cell(row, "channel", out)?
        || envelope.origin != text_cell(row, "origin", out)?
    {
        return Err(ReadFailure::ComponentCorruption(
            "source envelope disagrees with indexed attribution",
        ));
    }
    Ok(EventSource {
        block_id,
        envelope,
        source_path: json_cell!(out, row, "source_path", Vec<SourcePathPart>),
        partition_rule_id: text_cell(row, "rule_id", out)?.into(),
    })
}

pub(super) fn entry(
    db: &Connection,
    out: &mut Output,
    id: i64,
    binding: Option<i64>,
    interval: Option<Interval<'_>>,
    step_query: Option<&StepQueryRef>,
) -> Result<()> {
    let mut statement =
        db.prepare("SELECT * FROM content_block WHERE id=?1 AND content_type='statement'")?;
    let mut rows = statement.query([id])?;
    let row = rows.next()?.ok_or(ReadFailure::ComponentCorruption(
        "missing statement content",
    ))?;
    let source = match text_cell(row, "source_kind", out)? {
        "seed" => ReadSource::Seed(SeedSource {
            seed_id: db.query_row(
                "SELECT seed_id FROM store_meta WHERE singleton=1",
                [],
                |r| r.get(0),
            )?,
            ordinal: counter(row, "seed_ordinal")?,
        }),
        "event" => ReadSource::Event(Box::new(event_source(db, id, out)?)),
        _ => {
            return Err(ReadFailure::ComponentCorruption(
                "unknown content source kind",
            ));
        }
    };
    out.raw("{\"source\":")?;
    value!(out, source)?;
    let raw = text_cell(row, "raw_block", out)?;
    let raw: &RawValue = serde_json::from_str(raw)
        .map_err(|_| ReadFailure::ComponentCorruption("invalid raw content"))?;
    reject_duplicate_keys(raw.get())
        .map_err(|_| ReadFailure::ComponentCorruption("ambiguous raw content"))?;
    out.raw(",\"raw_block\":")?;
    value!(out, raw)?;
    let text = text_cell(row, "statement_text", out)?;
    if let Some(selected) = &interval {
        Span {
            start: selected.start,
            end: selected.end,
        }
        .slice(text)
        .map_err(|_| ReadFailure::ComponentCorruption("invalid selected UTF-8 interval"))?;
    }
    out.raw(",\"text\":")?;
    value!(out, text)?;
    out.raw(",\"bindings\":[")?;
    let mut bindings=db.prepare("SELECT * FROM statement_binding WHERE block_id=?1 AND (?2 IS NULL OR id=?2) ORDER BY ordinal")?;
    let mut bindings = bindings.query(params![id, binding])?;
    let mut first = true;
    while let Some(row) = bindings.next()? {
        let start = counter(row, "start")?;
        let end = counter(row, "end")?;
        if let Some(selected) = &interval
            && (start < selected.start || end > selected.end)
        {
            continue;
        }
        out.comma(&mut first)?;
        write_binding(db, out, row)?;
    }
    out.raw("],\"selected_span\":")?;
    match &interval {
        Some(selected) => value!(
            out,
            SelectedSpan {
                span_ref: selected.span.clone(),
                start: selected.start,
                end: selected.end
            }
        )?,
        None => out.raw("null")?,
    }
    out.raw(",\"step_query\":")?;
    value!(out, step_query)?;
    out.raw(",\"pairs\":[")?;
    // One pair belongs to exactly one binding, so this join deduplicates compound
    // nouns without allocating a second list of pair identities.
    if !matches!(&interval,Some(selected) if selected.frontier.is_none()) {
        let lower = interval.as_ref().map(|s| blob(s.start));
        let upper = interval.as_ref().map(|s| blob(s.end));
        let frontier = interval.as_ref().and_then(|s| s.frontier);
        let mut pairs = db.prepare(WHOLE_PAIRS)?;
        let mut pairs = pairs.query(params![id, binding, lower, upper, frontier])?;
        let mut first = true;
        while let Some(row) = pairs.next()? {
            let terminal: Option<i64> = row.get("terminal_event")?;
            if frontier.is_some_and(|cut| terminal.is_none_or(|end| end > cut)) {
                return Err(ReadRefusalCode::SpanPending.into());
            }
            out.comma(&mut first)?;
            value!(out, read_pair(db, row, out)?)?;
        }
    }
    out.raw("]}")
}

fn write_binding(db: &Connection, out: &mut Output, row: &Row<'_>) -> Result<()> {
    out.raw("{\"ordinal\":")?;
    value!(out, counter(row, "ordinal")?)?;
    out.raw(",\"value\":{\"statement_span\":")?;
    value!(
        out,
        Span {
            start: counter(row, "start")?,
            end: counter(row, "end")?
        }
    )?;
    out.raw(",\"verb\":")?;
    value!(out, text_cell(row, "verb", out)?)?;
    out.raw(",\"verb_span\":")?;
    value!(
        out,
        Span {
            start: counter(row, "verb_start")?,
            end: counter(row, "verb_end")?
        }
    )?;
    out.raw(",\"condition\":")?;
    let condition: Option<&str> = row
        .get_ref("condition")?
        .as_str_or_null()
        .map_err(|_| ReadFailure::ComponentCorruption("invalid condition"))?;
    value!(out, condition)?;
    out.raw(",\"condition_span\":")?;
    match (
        nullable_counter(row, "condition_start")?,
        nullable_counter(row, "condition_end")?,
    ) {
        (Some(start), Some(end)) => value!(out, Span { start, end })?,
        (None, None) => out.raw("null")?,
        _ => {
            return Err(ReadFailure::ComponentCorruption(
                "partial condition interval",
            ));
        }
    }
    out.raw(",\"nouns\":[")?;
    let mut nouns =
        db.prepare("SELECT * FROM binding_noun WHERE binding_id=?1 ORDER BY ordinal")?;
    let mut nouns = nouns.query([row.get::<_, i64>("id")?])?;
    let mut first = true;
    while let Some(row) = nouns.next()? {
        out.comma(&mut first)?;
        value!(
            out,
            NounBinding {
                value: text_cell(row, "value", out)?.into(),
                span: Span {
                    start: counter(row, "start")?,
                    end: counter(row, "end")?
                }
            }
        )?;
    }
    out.raw("]}}")
}

fn pair_source(db: &Connection, id: i64, out: &Output) -> Result<PairReadSource> {
    let source = event_source(db, id, out)?;
    let mut statement =
        db.prepare("SELECT raw_block FROM content_block WHERE id=?1 AND content_type='pair'")?;
    let mut rows = statement.query([id])?;
    let row = rows
        .next()?
        .ok_or(ReadFailure::ComponentCorruption("missing pair payload"))?;
    let raw = text_cell(row, "raw_block", out)?;
    reject_duplicate_keys(raw)
        .map_err(|_| ReadFailure::ComponentCorruption("ambiguous pair payload"))?;
    let raw_block = RawValue::from_string(raw.into())
        .map_err(|_| ReadFailure::ComponentCorruption("invalid pair payload"))?;
    Ok(PairReadSource { source, raw_block })
}
fn read_pair(db: &Connection, row: &Row<'_>, out: &Output) -> Result<ReadPair> {
    let opening_source = pair_source(db, row.get("opened_event")?, out)?;
    let opening: PairStarted = decode_strict(opening_source.raw_block.get())
        .map_err(|_| ReadFailure::ComponentCorruption("invalid pair opening"))?;
    let call_id = CallId {
        run: text_cell(row, "run", out)?.into(),
        sequence: counter(row, "sequence")?,
        call: counter(row, "call")?,
    };
    let statement = statement_ref(db, row.get("statement_id")?)?;
    let request: PairRequest = json_cell!(out, row, "request", PairRequest);
    let lane = match text_cell(row, "lane", out)? {
        "external" => PairLane::External,
        "recall" => PairLane::Recall,
        "review" => PairLane::Review,
        _ => return Err(ReadFailure::ComponentCorruption("unknown pair lane")),
    };
    if opening.call_id != call_id
        || opening.statement != statement
        || opening.request != request
        || opening.lane != lane
        || opening.call_block != counter(row, "call_block")?
    {
        return Err(ReadFailure::ComponentCorruption(
            "pair index differs from opening",
        ));
    }
    let retry: Option<&str> = row
        .get_ref("retry_of")?
        .as_str_or_null()
        .map_err(|_| ReadFailure::ComponentCorruption("invalid retry"))?;
    let retry = retry
        .map(decode_strict::<CallId>)
        .transpose()
        .map_err(|_| ReadFailure::ComponentCorruption("invalid retry identity"))?;
    if retry != opening.retry_of {
        return Err(ReadFailure::ComponentCorruption(
            "pair retry differs from opening",
        ));
    }
    let fate = match text_cell(row, "fate", out)? {
        "opened" => PairFate::Opened,
        "completed" => PairFate::Completed,
        "failed" => PairFate::Failed,
        _ => return Err(ReadFailure::ComponentCorruption("unknown pair fate")),
    };
    let reason: Option<&str> = row
        .get_ref("reason")?
        .as_str_or_null()
        .map_err(|_| ReadFailure::ComponentCorruption("invalid pair reason"))?;
    let reason = reason
        .map(decode_strict::<PairFailure>)
        .transpose()
        .map_err(|_| ReadFailure::ComponentCorruption("invalid pair reason"))?;
    let terminal_source = match row.get::<_, Option<i64>>("terminal_event")? {
        Some(id) => Some(pair_source(db, id, out)?),
        None => None,
    };
    match &terminal_source {
        Some(source) => {
            let ended: PairEnded = decode_strict(source.raw_block.get())
                .map_err(|_| ReadFailure::ComponentCorruption("invalid pair terminal"))?;
            let expected = match ended.fate {
                TerminalPairFate::Completed => PairFate::Completed,
                TerminalPairFate::Failed => PairFate::Failed,
            };
            if ended.call_id != call_id || ended.reason != reason || expected != fate {
                return Err(ReadFailure::ComponentCorruption(
                    "pair terminal differs from index",
                ));
            }
        }
        None => {
            if fate != PairFate::Opened || reason.is_some() {
                return Err(ReadFailure::ComponentCorruption(
                    "terminal pair has no source",
                ));
            }
        }
    }
    Ok(ReadPair {
        call_id,
        statement,
        lane,
        request,
        fate,
        reason,
        opening_source,
        terminal_source,
    })
}
pub(super) fn open_pairs(db: &Connection, out: &mut Output) -> Result<()> {
    out.raw("{\"open_pairs\":{\"pairs\":[")?;
    let mut statement=db.prepare("SELECT p.* FROM tool_pair p INDEXED BY pair_open JOIN content_block o ON o.id=p.opened_event WHERE p.fate='opened' ORDER BY p.opened_event")?;
    let mut rows = statement.query([])?;
    let mut first = true;
    while let Some(row) = rows.next()? {
        out.comma(&mut first)?;
        value!(out, read_pair(db, row, out)?)?;
    }
    out.raw("]}}")
}
