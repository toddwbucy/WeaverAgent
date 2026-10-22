use super::*;
use rusqlite::params;

pub(super) fn verify(db: &Connection, start: &StartIdentity) -> Result<(), OpenFault> {
    let orphans:i64=db.query_row(
        "SELECT COUNT(*) FROM pair_event e JOIN content_block b ON b.id=e.id
         WHERE b.content_type!='pair' OR
           (SELECT COUNT(*) FROM tool_pair p WHERE p.opened_event=e.id OR p.terminal_event=e.id)!=1",
        [],|r|r.get(0))?;
    if orphans != 0 {
        return Err(invalid("orphan or multiply consumed pair event"));
    }
    let mut stmt = db.prepare("SELECT * FROM tool_pair ORDER BY opened_event")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let opened: i64 = row.get("opened_event")?;
        let filing = pair_file(db, opened)?;
        let PairRecord::Started(send) = &filing.pair else {
            return Err(invalid("pair opens with terminal"));
        };
        let identity = CallId {
            run: row.get("run")?,
            sequence: counter(row, "sequence")?,
            call: counter(row, "call")?,
        };
        let request: PairRequest = decode!(row, "request", PairRequest);
        let retry: Option<String> = row.get("retry_of")?;
        let retry = retry
            .map(|value| {
                decode_strict::<CallId>(&value).map_err(|_| invalid("invalid retry identity"))
            })
            .transpose()?;
        let lane: String = row.get("lane")?;
        let lane_name = match send.lane {
            PairLane::External => "external",
            PairLane::Recall => "recall",
            PairLane::Review => "review",
        };
        if send.call_id != identity
            || send.call_block != counter(row, "call_block")?
            || send.request != request
            || send.retry_of != retry
            || lane != lane_name
            || send.request.lane() != send.lane
            || send.call_id.run != filing.block_id.run
            || send.call_id.sequence >= filing.block_id.sequence
            || filing.source.turn.is_none()
        {
            return Err(invalid(
                "pair opening differs from indexed identity or request",
            ));
        }

        let statement_id: i64 = row.get("statement_id")?;
        let (block, binding): (i64, Vec<u8>) = db.query_row(
            "SELECT block_id,ordinal FROM statement_binding WHERE id=?1",
            [statement_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut source_stmt = db.prepare("SELECT * FROM content_block WHERE id=?1")?;
        let mut source_rows = source_stmt.query([block])?;
        let statement = source_rows
            .next()?
            .ok_or_else(|| invalid("pair statement missing"))?;
        let (block_id, envelope) = source(statement)?;
        let binding = u64::from_be_bytes(
            binding
                .try_into()
                .map_err(|_| invalid("invalid binding counter"))?,
        );
        if send.statement
            != (StatementRef {
                block_id,
                binding: binding.into(),
            })
            || block >= opened
            || envelope.run != send.call_id.run
            || envelope.turn != filing.source.turn
        {
            return Err(invalid("pair statement attribution mismatch"));
        }
        let review = send.lane == PairLane::Review;
        if envelope.kind
            != if review {
                "loop.instruction"
            } else {
                "message.assistant"
            }
            || envelope.origin != if review { "loop" } else { "model" }
            || filing.source.origin != if review { "loop" } else { "model" }
            || envelope.sequence >= filing.source.sequence
        {
            return Err(invalid("pair send source is not licensed"));
        }
        if review
            && (send.call_id.sequence != send.statement.block_id.sequence
                || send.call_id.call.0 != 0
                || send.call_block.0 != 0
                || send.statement.block_id.block.0 != 0)
        {
            return Err(invalid("review pair does not identify its instruction"));
        }
        if let PairRequest::Recall { subject } = &send.request {
            let matched: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM statement_binding b JOIN binding_noun n ON n.binding_id=b.id
                 WHERE b.id=?1 AND b.condition IS NULL AND n.value=?2 COLLATE BINARY
                   AND (SELECT COUNT(*) FROM binding_noun WHERE binding_id=b.id)=1)",
                params![statement_id, subject], |row| row.get(0))?;
            if subject.is_empty() || !matched {
                return Err(invalid(
                    "recall requires its exact single held noun and no condition",
                ));
            }
        }
        if let Some(previous) = retry {
            if !matches!(send.lane, PairLane::External)
                || previous == identity
                || previous.run != identity.run
            {
                return Err(invalid("invalid retry lineage"));
            }
            let (prior_lane,prior_request,terminal):(String,String,Option<i64>)=db.query_row(
                "SELECT lane,request,terminal_event FROM tool_pair WHERE run=?1 AND sequence=?2 AND call=?3",
                params![previous.run,previous.sequence.0.to_be_bytes(),previous.call.0.to_be_bytes()],
                |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            if prior_lane != "external"
                || terminal.is_none_or(|id| id >= opened)
                || decode_strict::<PairRequest>(&prior_request)
                    .map_err(|_| invalid("invalid prior request"))?
                    != request
            {
                return Err(invalid(
                    "retry predecessor was not terminal with same request",
                ));
            }
        }
        let terminal: Option<i64> = row.get("terminal_event")?;
        let fate: String = row.get("fate")?;
        let reason: Option<String> = row.get("reason")?;
        if let Some(terminal) = terminal {
            if terminal <= opened {
                return Err(invalid("pair terminal precedes opening"));
            }
            let ending = pair_file(db, terminal)?;
            let PairRecord::Ended(end) = &ending.pair else {
                return Err(invalid("pair closes with opening"));
            };
            let reason = reason
                .map(|value| {
                    decode_strict::<PairFailure>(&value).map_err(|_| invalid("invalid pair reason"))
                })
                .transpose()?;
            let expected_fate = match end.fate {
                TerminalPairFate::Completed => "completed",
                TerminalPairFate::Failed => "failed",
            };
            if end.call_id != identity || reason != end.reason || fate != expected_fate {
                return Err(invalid("terminal disagrees with indexed pair"));
            }
            // The terminal retains historical load attribution. This checks that
            // structure, not a past configuration unavailable to this member.
            let historical_start = end.loaded_from.as_ref().unwrap_or(start);
            if historical_start.seed_id != start.seed_id {
                return Err(invalid("terminal loaded from another seed"));
            }
            let context = ServingContext {
                session_id: ending.source.session.clone(),
                run_id: ending.source.run.clone(),
                scheme_id: ending.scheme_id.clone(),
                seed_id: start.seed_id.clone(),
                start: historical_start.clone(),
            };
            filing::pairs::validate_terminal(&context, &ending, end, &lane, &filing.source)
                .map_err(|error| invalid(error.to_string()))?;
        } else if fate != "opened" || reason.is_some() {
            return Err(invalid("open pair has terminal metadata"));
        }
    }
    Ok(())
}

fn pair_file(db: &Connection, id: i64) -> Result<PairFile, OpenFault> {
    let mut stmt=db.prepare("SELECT b.*,p.record_type FROM content_block b JOIN pair_event p ON p.id=b.id WHERE b.id=?1 AND b.content_type='pair'")?;
    let mut rows = stmt.query([id])?;
    let row = rows.next()?.ok_or_else(|| invalid("pair event missing"))?;
    let (block_id, source) = source(row)?;
    let raw: Box<RawValue> = decode!(row, "raw_block", Box<RawValue>);
    let kind: String = row.get("record_type")?;
    let expected_kind = if kind == "started" {
        "tool.call.started"
    } else {
        "tool.call.completed"
    };
    if source.kind != expected_kind {
        return Err(invalid("pair event kind disagrees with record"));
    }
    let pair = match kind.as_str() {
        "started" => {
            PairRecord::Started(decode_strict(raw.get()).map_err(|_| invalid("invalid opening"))?)
        }
        "ended" => {
            PairRecord::Ended(decode_strict(raw.get()).map_err(|_| invalid("invalid terminal"))?)
        }
        _ => return Err(invalid("invalid pair type")),
    };
    let filing = PairFile {
        block_id,
        source,
        partition_rule_id: row.get("rule_id")?,
        source_path: decode!(row, "source_path", Vec<SourcePathPart>),
        admission: Admission::Admitted,
        raw_block: raw,
        scheme_id: row.get("scheme_id")?,
        pair,
    };
    if filing.source.channel != "tool" {
        return Err(invalid("pair is not on tool channel"));
    }
    filing
        .validate_structure()
        .map_err(|error| invalid(error.to_string()))?;
    Ok(filing)
}
