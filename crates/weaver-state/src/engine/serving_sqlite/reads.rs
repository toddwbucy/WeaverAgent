//! Read-only state-content/1 queries and bounded complete-result serialization.
//!
//! The encoded port streams SQLite rows into one bounded snapshot. The typed port
//! decodes only that already bounded result. Neither port exposes a partial result.

use super::ServingSqlite;
use crate::serving_bytes::BufferBytes;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::io::{self, Write};
use weaver_types::state_flow::*;

mod content;
mod marks;
mod plans;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(super) enum ReadFailure {
    Refused(ReadRefusalCode),
    ComponentCorruption(&'static str),
    Sqlite(rusqlite::Error),
}
impl From<rusqlite::Error> for ReadFailure {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
impl From<ReadRefusalCode> for ReadFailure {
    fn from(value: ReadRefusalCode) -> Self {
        Self::Refused(value)
    }
}
type Result<T> = std::result::Result<T, ReadFailure>;

macro_rules! value {
    ($out:expr, $value:expr) => {{
        let value = &$value;
        $out.serialized(|writer| serde_json::to_writer(writer, &value))
    }};
}
macro_rules! json_cell {
    ($out:expr, $row:expr, $name:expr, $ty:ty) => {
        decode_strict::<$ty>(text_cell($row, $name, $out)?)
            .map_err(|_| ReadFailure::ComponentCorruption("invalid typed stored JSON"))?
    };
}
use {json_cell, value};

pub(super) fn query(store: &ServingSqlite, selector: &ReadSelector) -> Result<QueryResult> {
    let bytes = query_encoded(store, selector, None)?;
    decode_strict(
        std::str::from_utf8(&bytes)
            .map_err(|_| ReadFailure::ComponentCorruption("invalid encoded result UTF-8"))?,
    )
    .map_err(|_| ReadFailure::ComponentCorruption("invalid encoded query result"))
}

pub(super) fn query_encoded(
    store: &ServingSqlite,
    selector: &ReadSelector,
    copied: Option<&mut BufferBytes>,
) -> Result<Vec<u8>> {
    selector.validate_shape()?;
    let mut out = Output::new(store.read_limits.max_result_bytes)?;
    out.copied = copied;
    let result = (|| -> Result<()> {
        let tx = store.connection.unchecked_transaction()?;
        match selector {
            ReadSelector::History {
                noun,
                verb,
                condition,
            } => content::history(&tx, &mut out, noun, verb.as_deref(), condition.as_deref())?,
            ReadSelector::Condition { condition } => content::condition(&tx, &mut out, condition)?,
            ReadSelector::Subjects { nouns } => content::subjects(&tx, &mut out, nouns)?,
            ReadSelector::Statement { statement } => content::statement(&tx, &mut out, statement)?,
            ReadSelector::Conversation { last_turns } => {
                content::conversation(&tx, &mut out, *last_turns)?
            }
            ReadSelector::OpenPairs {} => content::open_pairs(&tx, &mut out)?,
            ReadSelector::Span {
                seed_id,
                local_id,
                revision,
            } => marks::span(
                &tx,
                &store.context.seed_id,
                &mut out,
                &SpanRef {
                    seed_id: seed_id.clone(),
                    local_id: *local_id,
                    revision: *revision,
                },
            )?,
            ReadSelector::SystemPrompt {} => marks::span(
                &tx,
                &store.context.seed_id,
                &mut out,
                &SpanRef {
                    seed_id: store.context.seed_id.clone(),
                    local_id: 0.into(),
                    revision: 1.into(),
                },
            )?,
            ReadSelector::Marks {} => marks::marks(&tx, &store.context.seed_id, &mut out)?,
            ReadSelector::SpanPreview { span, through } => {
                marks::preview(&tx, &store.context.seed_id, &mut out, span, through)?
            }
            ReadSelector::PlanCurrent {} => plans::current(&tx, &store.context.seed_id, &mut out)?,
            ReadSelector::Plan { plan } => {
                plans::historical(&tx, &store.context.seed_id, &mut out, plan)?
            }
            ReadSelector::StepSuccessor {
                plan,
                previous_step,
            } => plans::successor(
                &tx,
                &store.context.seed_id,
                &mut out,
                plan,
                previous_step.as_ref(),
            )?,
            ReadSelector::PlanFlags { plan, after } => {
                plans::flags(&tx, &store.context.seed_id, &mut out, plan, after)?
            }
            ReadSelector::Observations {
                run,
                after,
                through,
            } => observations(&tx, &mut out, run, *after, *through)?,
        }
        tx.commit()?;
        Ok(())
    })();
    result.map(|()| out.bytes)
}

/// SQL keys never leave these helpers. Stored malformed counters are corruption.
fn counter(row: &Row<'_>, name: &str) -> Result<DecimalU64> {
    let value = row
        .get_ref(name)?
        .as_blob()
        .map_err(|_| ReadFailure::ComponentCorruption("counter is not a blob"))?;
    let bytes: [u8; 8] = value
        .try_into()
        .map_err(|_| ReadFailure::ComponentCorruption("counter is not eight bytes"))?;
    Ok(u64::from_be_bytes(bytes).into())
}
fn blob(value: DecimalU64) -> [u8; 8] {
    value.0.to_be_bytes()
}
fn nullable_counter(row: &Row<'_>, name: &str) -> Result<Option<DecimalU64>> {
    if matches!(row.get_ref(name)?, rusqlite::types::ValueRef::Null) {
        Ok(None)
    } else {
        counter(row, name).map(Some)
    }
}
fn nonnegative(value: i64) -> Result<DecimalU64> {
    u64::try_from(value)
        .map(Into::into)
        .map_err(|_| ReadFailure::ComponentCorruption("negative SQLite count"))
}
fn checked_next(connection: &Connection, sql: &str) -> Result<DecimalU64> {
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(ReadFailure::ComponentCorruption("aggregate has no row"))?;
    let previous = nullable_counter(row, "largest")?.unwrap_or(DecimalU64(0));
    previous
        .0
        .checked_add(1)
        .map(Into::into)
        .ok_or(ReadRefusalCode::CounterExhausted.into())
}
fn text_cell<'a>(row: &'a Row<'_>, name: &str, out: &Output) -> Result<&'a str> {
    let text = row
        .get_ref(name)?
        .as_str()
        .map_err(|_| ReadFailure::ComponentCorruption("stored text is not UTF-8"))?;
    if text.len() > out.remaining() {
        return Err(ReadRefusalCode::ResultTooLarge.into());
    }
    Ok(text)
}
fn source_id(connection: &Connection, id: i64) -> Result<SourceBlockId> {
    let mut statement = connection.prepare(
        "SELECT run,sequence,block FROM content_block WHERE id=?1 AND source_kind='event'",
    )?;
    let mut rows = statement.query([id])?;
    let row = rows
        .next()?
        .ok_or(ReadFailure::ComponentCorruption("missing event source"))?;
    Ok(SourceBlockId {
        run: row.get("run")?,
        sequence: counter(row, "sequence")?,
        block: counter(row, "block")?,
    })
}
fn resolve_source(connection: &Connection, source: &SourceBlockId) -> Result<Option<i64>> {
    Ok(connection.query_row("SELECT id FROM content_block WHERE source_kind='event' AND run=?1 AND sequence=?2 AND block=?3", params![source.run,blob(source.sequence),blob(source.block)], |row| row.get(0)).optional()?)
}
fn statement_ref(connection: &Connection, id: i64) -> Result<StatementRef> {
    let mut statement =
        connection.prepare("SELECT block_id,ordinal FROM statement_binding WHERE id=?1")?;
    let mut rows = statement.query([id])?;
    let row = rows.next()?.ok_or(ReadFailure::ComponentCorruption(
        "missing statement reference",
    ))?;
    Ok(StatementRef {
        block_id: source_id(connection, row.get("block_id")?)?,
        binding: counter(row, "ordinal")?,
    })
}
fn same_seed(expected: &str, actual: &str) -> Result<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(ReadRefusalCode::WrongSeed.into())
    }
}

pub(super) fn position(connection: &Connection) -> Result<Option<SourcePosition>> {
    let mut statement = connection.prepare("SELECT run,sequence,block,turn FROM content_block WHERE source_kind='event' ORDER BY id DESC LIMIT 1")?;
    let mut rows = statement.query([])?;
    rows.next()?
        .map(|row| {
            Ok(SourcePosition {
                block_id: SourceBlockId {
                    run: row.get("run")?,
                    sequence: counter(row, "sequence")?,
                    block: counter(row, "block")?,
                },
                turn: row.get("turn")?,
            })
        })
        .transpose()
}
pub(super) fn held(connection: &Connection) -> Result<DecimalU64> {
    nonnegative(connection.query_row("SELECT COUNT(*) FROM content_block", [], |row| row.get(0))?)
}
pub(super) fn shape(
    connection: &Connection,
    max_bytes: DecimalU64,
    copied: Option<&mut BufferBytes>,
) -> Result<Vec<ShapeRow>> {
    let mut out = Output::new(max_bytes)?;
    out.copied = copied;
    let result = (|| -> Result<()> {
        out.raw("[")?;
        let mut statement=connection.prepare("SELECT run,kind,COUNT(*) AS count FROM content_block WHERE source_kind='event' GROUP BY run,kind ORDER BY MIN(id)")?;
        let mut rows = statement.query([])?;
        let mut first = true;
        while let Some(row) = rows.next()? {
            out.comma(&mut first)?;
            value!(
                out,
                ShapeRow {
                    run: text_cell(row, "run", &out)?.into(),
                    kind: text_cell(row, "kind", &out)?.into(),
                    count: nonnegative(row.get("count")?)?
                }
            )?;
        }
        out.raw("]")?;
        Ok(())
    })();
    result?;
    serde_json::from_slice(&out.bytes)
        .map_err(|_| ReadFailure::ComponentCorruption("invalid shape result"))
}

fn observations(
    connection: &Connection,
    out: &mut Output,
    run: &str,
    after: DecimalU64,
    through: Option<DecimalU64>,
) -> Result<()> {
    let mut member_statement =
        connection.prepare("SELECT member_observation FROM observation_run WHERE run=?1")?;
    let mut member_rows = member_statement.query([run])?;
    let row = member_rows.next()?.ok_or(ReadRefusalCode::NotFound)?;
    let member: MemberObservationIdentity =
        json_cell!(out, row, "member_observation", MemberObservationIdentity);
    member
        .validate_historical()
        .map_err(|_| ReadFailure::ComponentCorruption("invalid member observation"))?;
    let mut max_statement = connection
        .prepare("SELECT MAX(operation_id) AS largest FROM operation_observation WHERE run=?1")?;
    let mut max_rows = max_statement.query([run])?;
    let maximum = nullable_counter(
        max_rows
            .next()?
            .ok_or(ReadFailure::ComponentCorruption("missing aggregate"))?,
        "largest",
    )?
    .unwrap_or(0.into());
    let through = through.unwrap_or(maximum);
    if through > maximum || after > through {
        return Err(ReadRefusalCode::InvalidSelector.into());
    }
    out.raw("{\"observations\":{\"run\":")?;
    value!(out, run)?;
    out.raw(",\"after\":")?;
    value!(out, after)?;
    out.raw(",\"through\":")?;
    value!(out, through)?;
    out.raw(",\"member_observation\":")?;
    value!(out, member)?;
    out.raw(",\"records\":[")?;
    let mut statement=connection.prepare("SELECT operation_id,request_id,operation,source_run,source_sequence,source_block,observation FROM operation_observation WHERE run=?1 AND operation_id>?2 AND operation_id<=?3 ORDER BY operation_id")?;
    let mut rows = statement.query(params![run, blob(after), blob(through)])?;
    let mut first = true;
    let mut previous = after;
    while let Some(row) = rows.next()? {
        let record = observation_record(row, out)?;
        let id = record.observation.operation_id;
        if id <= previous || id > through {
            return Err(ReadFailure::ComponentCorruption("observation order"));
        }
        previous = id;
        out.comma(&mut first)?;
        value!(out, record)?;
    }
    out.raw("]}}")
}

fn observation_record(row: &Row<'_>, out: &Output) -> Result<StoredOperationObservation> {
    let observed: StateOperationObservation =
        json_cell!(out, row, "observation", StateOperationObservation);
    let operation: StateOperation = serde_json::from_str(
        &serde_json::to_string(text_cell(row, "operation", out)?)
            .map_err(|_| ReadFailure::ComponentCorruption("invalid operation"))?,
    )
    .map_err(|_| ReadFailure::ComponentCorruption("unknown stored operation"))?;
    let id = counter(row, "operation_id")?;
    observed
        .validate(counter(row, "request_id")?, operation, id)
        .map_err(|_| {
            ReadFailure::ComponentCorruption("stored observation disagrees with columns")
        })?;
    let source_run: Option<String> = row.get("source_run")?;
    let request_source = match source_run {
        Some(run) => Some(SourceBlockId {
            run,
            sequence: counter(row, "source_sequence")?,
            block: counter(row, "source_block")?,
        }),
        None => {
            if nullable_counter(row, "source_sequence")?.is_some()
                || nullable_counter(row, "source_block")?.is_some()
            {
                return Err(ReadFailure::ComponentCorruption(
                    "partial observation source",
                ));
            }
            None
        }
    };
    let record = StoredOperationObservation {
        request_source,
        observation: observed,
    };
    record
        .validate()
        .map_err(|_| ReadFailure::ComponentCorruption("invalid stored observation source"))?;
    Ok(record)
}

/// Adoption validates measurement metadata independently of a serving result cap.
/// These SELECTs stream one record at a time and do not create a new observation.
pub(super) fn verify_image_observations(connection: &Connection) -> Result<()> {
    let out = Output::new(u64::MAX.into())?;
    let mut runs = connection.prepare(
        "SELECT run,member_observation FROM observation_run ORDER BY run COLLATE BINARY",
    )?;
    let mut runs = runs.query([])?;
    while let Some(row) = runs.next()? {
        let run = text_cell(row, "run", &out)?;
        let member: MemberObservationIdentity =
            json_cell!(&out, row, "member_observation", MemberObservationIdentity);
        member
            .validate_historical()
            .map_err(|_| ReadFailure::ComponentCorruption("invalid member observation"))?;
        let mut records = connection.prepare(
            "SELECT operation_id,request_id,operation,source_run,source_sequence,
                    source_block,observation FROM operation_observation
             WHERE run=?1 ORDER BY operation_id",
        )?;
        let mut records = records.query([run])?;
        let mut previous = 0_u64;
        while let Some(row) = records.next()? {
            let record = observation_record(row, &out)?;
            let observation = record.observation;
            if previous.checked_add(1) != Some(observation.operation_id.0) {
                return Err(ReadFailure::ComponentCorruption(
                    "stored operation IDs do not form a complete run",
                ));
            }
            if previous == 0 {
                if observation.operation != StateOperation::Open || observation.request_id.0 != 1 {
                    return Err(ReadFailure::ComponentCorruption(
                        "stored run does not begin with open request one",
                    ));
                }
            } else if observation.operation == StateOperation::Open {
                return Err(ReadFailure::ComponentCorruption(
                    "stored run contains another open observation",
                ));
            }
            previous = observation.operation_id.0;
        }
        if previous == 0 {
            return Err(ReadFailure::ComponentCorruption(
                "stored member identity has no open observation",
            ));
        }
    }
    Ok(())
}

struct Output<'a> {
    bytes: Vec<u8>,
    bound: usize,
    failure: Option<ReadRefusalCode>,
    copied: Option<&'a mut BufferBytes>,
}
impl Output<'_> {
    fn new(bound: DecimalU64) -> Result<Self> {
        let bound = usize::try_from(bound.0).map_err(|_| ReadRefusalCode::ResultTooLarge)?;
        Ok(Self {
            bytes: Vec::new(),
            bound,
            failure: None,
            copied: None,
        })
    }
    fn remaining(&self) -> usize {
        self.bound.saturating_sub(self.bytes.len())
    }
    fn raw(&mut self, value: &str) -> Result<()> {
        self.write_all(value.as_bytes()).map_err(|_| {
            self.failure
                .unwrap_or(ReadRefusalCode::ResultUnavailable)
                .into()
        })
    }
    fn comma(&mut self, first: &mut bool) -> Result<()> {
        if *first {
            *first = false;
            Ok(())
        } else {
            self.raw(",")
        }
    }
    fn serialized(
        &mut self,
        encode: impl FnOnce(&mut Self) -> serde_json::Result<()>,
    ) -> Result<()> {
        encode(self).map_err(|_| {
            self.failure
                .map(ReadFailure::Refused)
                .unwrap_or(ReadFailure::ComponentCorruption(
                    "result serialization failed",
                ))
        })
    }
}
impl Write for Output<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining() {
            self.failure = Some(ReadRefusalCode::ResultTooLarge);
            return Err(io::Error::other("result_too_large"));
        }
        let needed = self.bytes.len() + bytes.len();
        if needed > self.bytes.capacity() {
            let capacity = self
                .bound
                .min(needed.max(self.bytes.capacity().saturating_mul(2).max(64)));
            if self
                .bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .is_err()
            {
                self.failure = Some(ReadRefusalCode::ResultUnavailable);
                return Err(io::Error::other("result_unavailable"));
            }
        }
        self.bytes.extend_from_slice(bytes);
        if let Some(copied) = &mut self.copied {
            copied.add(bytes.len());
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Transaction-local views used by atomic plan guards. They neither begin a second
/// transaction nor mint a closure or activation grant.
pub(super) fn plan_snapshot(
    db: &Connection,
    seed: &str,
    plan: &PlanRef,
    max_bytes: DecimalU64,
) -> Result<PlanSnapshot> {
    let mut out = Output::new(max_bytes)?;
    plans::historical(db, seed, &mut out, plan)?;
    match serde_json::from_slice::<QueryResult>(&out.bytes)
        .map_err(|_| ReadFailure::ComponentCorruption("invalid plan encoding"))?
    {
        QueryResult::Plan { snapshot } => Ok(snapshot),
        _ => Err(ReadFailure::ComponentCorruption("wrong plan result")),
    }
}
pub(super) fn current_plan_snapshot(
    db: &Connection,
    seed: &str,
    max_bytes: DecimalU64,
) -> Result<Option<PlanSnapshot>> {
    let mut out = Output::new(max_bytes)?;
    let mut statement = db.prepare(
        "SELECT * FROM plan_revision WHERE status!='superseded' ORDER BY last_source DESC LIMIT 1",
    )?;
    let mut rows = statement.query([])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    plans::snapshot(db, seed, &mut out, row)?;
    serde_json::from_slice(&out.bytes)
        .map(Some)
        .map_err(|_| ReadFailure::ComponentCorruption("invalid current plan encoding"))
}
pub(super) fn span_preview_encoded(
    db: &Connection,
    seed: &str,
    span: &SpanRef,
    through: &SourcePosition,
    max_bytes: DecimalU64,
) -> Result<Vec<u8>> {
    let mut out = Output::new(max_bytes)?;
    marks::preview(db, seed, &mut out, span, through)?;
    Ok(out.bytes)
}
/// Adoption checks historical WorkRef digests against the same canonical view that
/// was available while the span was open. This has no serving-read deployment cap
/// and never grants the public preview selector access to a terminal span.
pub(super) fn historical_span_preview_encoded(
    db: &Connection,
    seed: &str,
    span: &SpanRef,
    through: &SourcePosition,
) -> Result<Vec<u8>> {
    let mut out = Output::new(u64::MAX.into())?;
    marks::historical_preview(db, seed, &mut out, span, through)?;
    Ok(out.bytes)
}
pub(super) fn step_successor(
    db: &Connection,
    seed: &str,
    plan: &PlanRef,
    previous: Option<&StepId>,
    max_bytes: DecimalU64,
) -> Result<(StepQueryRef, ReadEntry)> {
    let mut out = Output::new(max_bytes)?;
    plans::successor(db, seed, &mut out, plan, previous)?;
    match serde_json::from_slice::<QueryResult>(&out.bytes)
        .map_err(|_| ReadFailure::ComponentCorruption("invalid successor encoding"))?
    {
        QueryResult::StepSuccessor {
            reference,
            instruction,
        } => Ok((reference, *instruction)),
        _ => Err(ReadFailure::ComponentCorruption("wrong successor result")),
    }
}
