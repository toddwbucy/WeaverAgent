//! The store's send/terminal ledger, with no dispatch or timeout policy.

use rusqlite::{OptionalExtension, Transaction, params};
use weaver_types::state_flow::{
    PairEnded, PairFailure, PairFile, PairLane, PairRecord, PairRequest, PairStarted,
    SourceEnvelope, StateRefusalCode, TerminalPairFate, decode_strict,
};
use weaver_types::{KillCause, ToolOutcome};

use super::super::ServingContext;
use super::{blob, fault, marks};
use crate::FileFault;

pub(super) fn apply(
    tx: &Transaction<'_>,
    context: &ServingContext,
    filing: &PairFile,
    source_id: i64,
) -> Result<(), FileFault> {
    if filing.source.channel != "tool" {
        return Err(invalid("pair is not tool-channel metadata"));
    }
    match &filing.pair {
        PairRecord::Started(started) if filing.source.kind == "tool.call.started" => {
            start(tx, context, filing, source_id, started)
        }
        PairRecord::Ended(ended) if filing.source.kind == "tool.call.completed" => {
            end(tx, context, filing, source_id, ended)
        }
        _ => Err(invalid("pair payload and source event kind disagree")),
    }
}

fn start(
    tx: &Transaction<'_>,
    context: &ServingContext,
    filing: &PairFile,
    source_id: i64,
    started: &PairStarted,
) -> Result<(), FileFault> {
    if started.lane != started.request.lane()
        || started.call_id.run != context.run_id
        || started.statement.block_id.run != context.run_id
        || filing.source.turn.is_none()
        || started.call_id.sequence >= filing.block_id.sequence
    {
        return Err(invalid(
            "opening lane or source identity differs from its send",
        ));
    }
    let statement = marks::resolve_statement(tx, &started.statement).map_err(|error| {
        if error.code == StateRefusalCode::InvalidEffect {
            fault(StateRefusalCode::StatementMissing, error.detail)
        } else {
            error
        }
    })?;
    let envelope: String = tx.query_row(
        "SELECT envelope FROM content_block WHERE id=?1",
        [statement.block_id],
        |row| row.get(0),
    )?;
    let envelope: SourceEnvelope =
        decode_strict(&envelope).map_err(|_| invalid("held send envelope is malformed"))?;
    let review = started.lane == PairLane::Review;
    let (source_kind, source_origin, opening_origin) = if review {
        ("loop.instruction", "loop", "loop")
    } else {
        ("message.assistant", "model", "model")
    };
    if envelope.kind != source_kind
        || envelope.origin != source_origin
        || filing.source.origin != opening_origin
        || envelope.turn != filing.source.turn
        || envelope.sequence >= filing.source.sequence
    {
        return Err(invalid("pair statement is not an eligible held send"));
    }
    if review
        && (started.call_id.sequence != started.statement.block_id.sequence
            || started.call_id.call.0 != 0
            || started.call_block.0 != 0
            || started.statement.block_id.block.0 != 0)
    {
        return Err(invalid("review identity does not name its instruction"));
    }
    if let PairRequest::Recall { subject } = &started.request {
        let matched: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM binding_noun WHERE binding_id=?1 AND value=?2 COLLATE BINARY)",params![statement.id,subject],|row|row.get(0))?;
        if subject.is_empty() || !matched {
            return Err(invalid("recall subject is not a held noun"));
        }
    }
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM tool_pair WHERE run=?1 AND sequence=?2 AND call=?3)",
        params![
            started.call_id.run,
            blob(started.call_id.sequence.0),
            blob(started.call_id.call.0)
        ],
        |row| row.get(0),
    )?;
    if exists {
        return Err(fault(
            StateRefusalCode::DuplicatePair,
            "call identity already exists",
        ));
    }
    if let Some(previous) = &started.retry_of {
        if started.lane != PairLane::External
            || previous.run != context.run_id
            || *previous == started.call_id
        {
            return Err(invalid(
                "retry lineage is not a distinct current-run external pair",
            ));
        }
        let prior: Option<(String, String, String)> = tx
            .query_row(
                "SELECT lane,fate,request FROM tool_pair WHERE run=?1 AND sequence=?2 AND call=?3",
                params![
                    previous.run,
                    blob(previous.sequence.0),
                    blob(previous.call.0)
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (lane, fate, request) =
            prior.ok_or_else(|| invalid("retry predecessor is not held"))?;
        let request: PairRequest = decode_strict(&request)
            .map_err(|_| invalid("retry predecessor request is malformed"))?;
        if lane != "external" || fate == "opened" || request != started.request {
            return Err(invalid(
                "retry changed its request or names an open predecessor",
            ));
        }
    }
    tx.execute(
        "INSERT INTO pair_event(id,record_type) VALUES (?1,'started')",
        [source_id],
    )?;
    let lane = match started.lane {
        PairLane::External => "external",
        PairLane::Recall => "recall",
        PairLane::Review => "review",
    };
    tx.execute("INSERT INTO tool_pair(run,sequence,call,call_block,statement_id,opened_event,lane,request,retry_of,fate)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'opened')",params![started.call_id.run,blob(started.call_id.sequence.0),blob(started.call_id.call.0),blob(started.call_block.0),statement.id,source_id,lane,
        serde_json::to_string(&started.request).map_err(super::json_fault)?,started.retry_of.as_ref().map(serde_json::to_string).transpose().map_err(super::json_fault)?])?;
    Ok(())
}

fn end(
    tx: &Transaction<'_>,
    context: &ServingContext,
    filing: &PairFile,
    source_id: i64,
    ended: &PairEnded,
) -> Result<(), FileFault> {
    let prior: Option<(String,String,String)> = tx.query_row("SELECT lane,fate,o.envelope FROM tool_pair p JOIN content_block o ON o.id=p.opened_event
        WHERE p.run=?1 AND p.sequence=?2 AND p.call=?3",params![ended.call_id.run,blob(ended.call_id.sequence.0),blob(ended.call_id.call.0)],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    let (lane, fate, opening) =
        prior.ok_or_else(|| fault(StateRefusalCode::UnknownPair, "terminal has no opened send"))?;
    if fate != "opened" {
        return Err(fault(
            StateRefusalCode::AlreadyTerminal,
            "pair already has a terminal",
        ));
    }
    let opening: SourceEnvelope =
        decode_strict(&opening).map_err(|_| invalid("opening envelope is malformed"))?;
    validate_terminal(context, filing, ended, &lane, &opening)?;
    tx.execute(
        "INSERT INTO pair_event(id,record_type) VALUES (?1,'ended')",
        [source_id],
    )?;
    let fate = match ended.fate {
        TerminalPairFate::Completed => "completed",
        TerminalPairFate::Failed => "failed",
    };
    let reason = ended
        .reason
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(super::json_fault)?;
    let changed = tx.execute("UPDATE tool_pair SET fate=?1,reason=?2,terminal_event=?3 WHERE run=?4 AND sequence=?5 AND call=?6 AND fate='opened'",
        params![fate,reason,source_id,ended.call_id.run,blob(ended.call_id.sequence.0),blob(ended.call_id.call.0)])?;
    if changed != 1 {
        return Err(fault(
            StateRefusalCode::AlreadyTerminal,
            "terminal transition changed no opened pair",
        ));
    }
    Ok(())
}

pub(in crate::engine::serving_sqlite) fn validate_terminal(
    context: &ServingContext,
    filing: &PairFile,
    ended: &PairEnded,
    lane: &str,
    opening: &SourceEnvelope,
) -> Result<(), FileFault> {
    use PairFailure::*;
    if ended
        .execution
        .as_ref()
        .is_some_and(|execution| execution.opener != weaver_types::Opener::Harness)
    {
        return Err(invalid("tool execution was not opened by the harness"));
    }
    let abandoned = matches!(ended.reason, Some(AbandonedAtLoad {}));
    if abandoned {
        if ended.fate != TerminalPairFate::Failed
            || ended.execution.is_some()
            || ended.outcome.is_some()
            || ended.loaded_from.as_ref() != Some(&context.start)
            || filing.source.turn.is_some()
            || filing.source.origin != "loop"
        {
            return Err(invalid(
                "abandoned pair lacks its exact loaded starting state",
            ));
        }
        return Ok(());
    }
    if ended.call_id.run != context.run_id
        || ended.loaded_from.is_some()
        || filing.source.turn.is_none()
        || filing.source.turn != opening.turn
    {
        return Err(invalid("terminal turn or load attribution is inconsistent"));
    }
    if matches!(ended.reason, Some(FlowFailure {})) {
        return Err(invalid(
            "flow-failure terminal is trace-only after the broken seam",
        ));
    }
    match (&ended.fate, &ended.reason, &ended.outcome) {
        (
            TerminalPairFate::Completed,
            None,
            Some(
                ToolOutcome::Result { .. }
                | ToolOutcome::Refused { .. }
                | ToolOutcome::Errored { .. },
            ),
        ) => {
            let eligible = if lane == "recall" {
                ended.execution.is_none()
                    && filing.source.origin == "loop"
                    && !matches!(ended.outcome, Some(ToolOutcome::Errored { .. }))
            } else {
                ended.execution.is_some() && filing.source.origin == "tool"
            };
            if !eligible {
                return Err(invalid("completed outcome has the wrong lane or execution"));
            }
        }
        (TerminalPairFate::Failed, Some(reason), Some(ToolOutcome::Killed { by, .. })) => {
            let mapped = matches!(
                (reason, by),
                (Timeout {}, KillCause::Clock)
                    | (Cancelled {}, KillCause::Cancel)
                    | (Unload {}, KillCause::Unload)
                    | (GateFault {}, KillCause::Fault)
            );
            let origin = if matches!(reason, GateFault {}) {
                "loop"
            } else {
                "tool"
            };
            if !mapped
                || lane == "recall"
                || ended.execution.is_none()
                || filing.source.origin != origin
            {
                return Err(invalid("killed outcome does not match its terminal reason"));
            }
        }
        (TerminalPairFate::Failed, Some(NotGranted {}), Some(ToolOutcome::Refused { .. })) => {
            if ended.execution.is_some() || filing.source.origin != "loop" || lane == "recall" {
                return Err(invalid("not-granted outcome has a gate execution"));
            }
        }
        (TerminalPairFate::Failed, Some(NotDispatched { .. }), None) => {
            if ended.execution.is_some() || filing.source.origin != "loop" {
                return Err(invalid("not-dispatched outcome fabricates execution"));
            }
        }
        _ => return Err(invalid("terminal fate, reason and outcome do not match")),
    }
    Ok(())
}

fn invalid(detail: impl Into<String>) -> FileFault {
    fault(StateRefusalCode::InvalidPair, detail)
}
