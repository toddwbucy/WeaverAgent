//! Atomic source, index and effect commits. A receipt follows SQLite commit.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use weaver_types::state_flow::{
    PairFile, SourceBlockId, SourceEnvelope, StateRefusalCode, StatementEffect, StatementFile,
};

use super::{ServingContext, ServingSqlite};
use crate::{FileCommit, FileFault};

pub(super) mod marks;
pub(super) mod pairs;
pub(super) mod plan;

#[cfg(test)]
pub(super) use super::tests::filing as tests;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FilingCheckpoint {
    AfterSource,
    AfterBinding(u64),
    AfterNoun(u64, u64),
    AfterEffect,
    BeforeCommit,
}

#[cfg(test)]
#[derive(Debug)]
pub(super) struct InjectedWriteFailure;

#[cfg(test)]
pub(super) trait FilingTestHooks {
    fn checkpoint(
        &self,
        point: FilingCheckpoint,
        transaction: &Transaction<'_>,
    ) -> Result<(), InjectedWriteFailure>;
    fn committed(&self, connection: &Connection);
}

macro_rules! checkpoint {
    ($hooks:expr, $point:expr, $transaction:expr) => {
        #[cfg(test)]
        if let Some(hooks) = $hooks {
            hooks.checkpoint($point, $transaction).map_err(|_| {
                fault(
                    StateRefusalCode::WriteFailed,
                    "injected transaction failure",
                )
            })?;
        }
    };
}

pub(super) fn file(
    store: &mut ServingSqlite,
    filing: &StatementFile,
) -> Result<FileCommit, FileFault> {
    let context = store.context.clone();
    check_identity(
        &store.connection,
        &context,
        &filing.block_id,
        &filing.source,
        &filing.scheme_id,
    )?;
    validate_effect_license(filing)?;
    // Open supplies the scheme identity, not another dictionary. The classifier
    // owns language judgment. The member verifies only nonempty keys and spans.
    let keys: Vec<String> = filing
        .bindings
        .iter()
        .map(|binding| binding.verb.clone())
        .collect();
    filing
        .validate_structure(&keys)
        .map_err(|error| fault(StateRefusalCode::InvalidBinding, error.to_string()))?;
    #[cfg(test)]
    let hooks = store.filing_hooks.as_deref();
    let tx = store.connection.transaction()?;
    let source_id = insert_statement(&tx, filing)?;
    checkpoint!(hooks, FilingCheckpoint::AfterSource, &tx);
    let mut binding_id: i64 = tx.query_row(
        "SELECT COALESCE(MAX(id),0) FROM statement_binding",
        [],
        |row| row.get(0),
    )?;
    for (ordinal, binding) in filing.bindings.iter().enumerate() {
        let ordinal = ordinal as u64;
        binding_id = binding_id.checked_add(1).ok_or_else(|| {
            fault(
                StateRefusalCode::WriteFailed,
                "binding identifier exhausted",
            )
        })?;
        positive_id(binding_id)?;
        tx.execute(
            "INSERT INTO statement_binding(id,block_id,ordinal,start,end,verb,verb_start,verb_end,
             condition,condition_start,condition_end) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                binding_id,
                source_id,
                blob(ordinal),
                blob(binding.statement_span.start.0),
                blob(binding.statement_span.end.0),
                binding.verb,
                blob(binding.verb_span.start.0),
                blob(binding.verb_span.end.0),
                binding.condition,
                binding.condition_span.map(|span| blob(span.start.0)),
                binding.condition_span.map(|span| blob(span.end.0))
            ],
        )?;
        checkpoint!(hooks, FilingCheckpoint::AfterBinding(ordinal), &tx);
        for (noun_ordinal, noun) in binding.nouns.iter().enumerate() {
            tx.execute(
                "INSERT INTO binding_noun(binding_id,ordinal,value,start,end) VALUES (?1,?2,?3,?4,?5)",
                params![binding_id, blob(noun_ordinal as u64), noun.value,
                    blob(noun.span.start.0), blob(noun.span.end.0)],
            )?;
            checkpoint!(
                hooks,
                FilingCheckpoint::AfterNoun(ordinal, noun_ordinal as u64),
                &tx
            );
        }
    }
    match &filing.effect {
        None => {}
        Some(StatementEffect::Mark(effect)) => {
            marks::apply_direct(&tx, &context, filing, source_id, effect)?
        }
        Some(StatementEffect::Plan(effect)) => {
            plan::apply(&tx, &context, filing, source_id, effect, &store.read_limits)?
        }
    }
    if filing.effect.is_some() {
        let invalid: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM span s LEFT JOIN span r
            ON r.local_id=s.replacement_local_id AND r.revision=s.replacement_revision
            WHERE s.replacement_local_id IS NOT NULL AND (r.local_id IS NULL OR r.kind != s.kind))",
            [],
            |row| row.get(0),
        )?;
        if invalid {
            return Err(invalid_effect(
                "replacement must resolve to a span of the same kind",
            ));
        }
    }
    checkpoint!(hooks, FilingCheckpoint::AfterEffect, &tx);
    checkpoint!(hooks, FilingCheckpoint::BeforeCommit, &tx);
    tx.commit()?;
    #[cfg(test)]
    if let Some(hooks) = hooks {
        hooks.committed(&store.connection);
    }
    Ok(FileCommit {
        block_id: filing.block_id.clone(),
    })
}

pub(super) fn file_pair(
    store: &mut ServingSqlite,
    filing: &PairFile,
) -> Result<FileCommit, FileFault> {
    let context = store.context.clone();
    check_identity(
        &store.connection,
        &context,
        &filing.block_id,
        &filing.source,
        &filing.scheme_id,
    )?;
    filing
        .validate_structure()
        .map_err(|error| fault(StateRefusalCode::InvalidPair, error.to_string()))?;
    #[cfg(test)]
    let hooks = store.filing_hooks.as_deref();
    let tx = store.connection.transaction()?;
    let source_id = insert_pair(&tx, filing)?;
    checkpoint!(hooks, FilingCheckpoint::AfterSource, &tx);
    pairs::apply(&tx, &context, filing, source_id)?;
    checkpoint!(hooks, FilingCheckpoint::AfterEffect, &tx);
    checkpoint!(hooks, FilingCheckpoint::BeforeCommit, &tx);
    tx.commit()?;
    #[cfg(test)]
    if let Some(hooks) = hooks {
        hooks.committed(&store.connection);
    }
    Ok(FileCommit {
        block_id: filing.block_id.clone(),
    })
}

fn check_identity(
    connection: &Connection,
    context: &ServingContext,
    block: &SourceBlockId,
    source: &SourceEnvelope,
    scheme: &str,
) -> Result<(), FileFault> {
    let held_scheme: String = connection.query_row(
        "SELECT scheme_id FROM store_meta WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    if scheme != context.scheme_id || scheme != held_scheme {
        return Err(fault(
            StateRefusalCode::SchemeMismatch,
            "filing differs from opened store scheme",
        ));
    }
    if source.run != context.run_id
        || block.run != context.run_id
        || source.session != context.session_id
    {
        return Err(fault(
            StateRefusalCode::WrongRun,
            "filing differs from opened run or session",
        ));
    }
    if source.run != block.run || source.sequence != block.sequence {
        return Err(fault(
            StateRefusalCode::MalformedFiling,
            "source envelope differs from block identity",
        ));
    }
    if source_id(connection, block)?.is_some() {
        return Err(fault(
            StateRefusalCode::DuplicateSource,
            "source already committed",
        ));
    }
    Ok(())
}

pub(in crate::engine::serving_sqlite) fn insert_statement(
    tx: &Transaction<'_>,
    filing: &StatementFile,
) -> Result<i64, FileFault> {
    let envelope = serde_json::to_string(&filing.source).map_err(json_fault)?;
    let path = serde_json::to_string(&filing.source_path).map_err(json_fault)?;
    let context_path = filing
        .context
        .as_ref()
        .map(|context| serde_json::to_string(&context.source_path))
        .transpose()
        .map_err(json_fault)?;
    let context_raw = filing.context.as_ref().map(|context| context.raw.get());
    let effect = filing
        .effect
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(json_fault)?;
    tx.execute(
        "INSERT INTO content_block(source_kind,run,sequence,block,session,turn,kind,channel,origin,
         envelope,rule_id,source_path,context_path,context_raw,effect,admission,raw_block,
         statement_text,scheme_id,content_type)
         VALUES ('event',?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'admitted',?15,?16,?17,'statement')",
        params![filing.block_id.run, blob(filing.block_id.sequence.0), blob(filing.block_id.block.0),
            filing.source.session, filing.source.turn, filing.source.kind, filing.source.channel,
            filing.source.origin, envelope, filing.partition_rule_id, path, context_path,
            context_raw, effect, filing.raw_block.get(), filing.text, filing.scheme_id],
    )?;
    positive_id(tx.last_insert_rowid())
}

fn insert_pair(tx: &Transaction<'_>, filing: &PairFile) -> Result<i64, FileFault> {
    tx.execute(
        "INSERT INTO content_block(source_kind,run,sequence,block,session,turn,kind,channel,origin,
         envelope,rule_id,source_path,admission,raw_block,scheme_id,content_type)
         VALUES ('event',?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'[]','admitted',?11,?12,'pair')",
        params![
            filing.block_id.run,
            blob(filing.block_id.sequence.0),
            blob(filing.block_id.block.0),
            filing.source.session,
            filing.source.turn,
            filing.source.kind,
            filing.source.channel,
            filing.source.origin,
            serde_json::to_string(&filing.source).map_err(json_fault)?,
            filing.partition_rule_id,
            filing.raw_block.get(),
            filing.scheme_id
        ],
    )?;
    positive_id(tx.last_insert_rowid())
}

pub(in crate::engine::serving_sqlite) fn validate_effect_license(
    filing: &StatementFile,
) -> Result<(), FileFault> {
    use weaver_types::state_flow::{MarkEffect, PlanEffect, PlanInputContext, decode_strict};
    if let Some(context) = &filing.context
        && (filing.block_id.block.0 != 0
            || context.source_path
                != [weaver_types::state_flow::SourcePathPart::Key(
                    "context".into(),
                )])
    {
        return Err(invalid_effect(
            "context must name the original block-zero sidecar",
        ));
    }
    let operator = filing.source.kind == "plan.event"
        && filing.source.channel == "input"
        && filing.source.origin == "operator"
        && filing.block_id.block.0 == 0;
    if operator {
        if filing.effect.is_some() {
            return Err(invalid_effect("operator input cannot apply its own effect"));
        }
        let context = filing
            .context
            .as_ref()
            .ok_or_else(|| invalid_effect("operator plan input lacks context"))?;
        let _: PlanInputContext = decode_strict(context.raw.get())
            .map_err(|_| invalid_effect("invalid operator plan context"))?;
        return Ok(());
    }
    match (&filing.context, &filing.effect) {
        (None, None) => {
            if filing.block_id.block.0 == 0
                && matches!(filing.source.kind.as_str(), "state.mark" | "plan.event")
            {
                return Err(invalid_effect(
                    "mechanical event lacks its licensed context",
                ));
            }
            Ok(())
        }
        (Some(context), Some(effect))
            if filing.block_id.block.0 == 0
                && filing.source.channel == "internal"
                && filing.source.origin == "loop" =>
        {
            match effect {
                StatementEffect::Mark(mark)
                    if filing.source.kind == "state.mark"
                        && matches!(mark, MarkEffect::BeginTask { .. }) =>
                {
                    let copied: MarkEffect = decode_strict(context.raw.get())
                        .map_err(|_| invalid_effect("invalid mark context"))?;
                    if copied != *mark {
                        return Err(invalid_effect("mark differs from copied context"));
                    }
                }
                StatementEffect::Plan(plan)
                    if (filing.source.kind == "plan.event"
                        && !matches!(plan.as_ref(), PlanEffect::Close { .. }))
                        || (filing.source.kind == "step.outcome"
                            && matches!(plan.as_ref(), PlanEffect::Close { .. })) =>
                {
                    let copied: PlanEffect = decode_strict(context.raw.get())
                        .map_err(|_| invalid_effect("invalid plan context"))?;
                    if copied != **plan {
                        return Err(invalid_effect("plan differs from copied context"));
                    }
                }
                _ => return Err(invalid_effect("effect is not licensed for its event")),
            }
            Ok(())
        }
        _ => Err(invalid_effect("unexpected or incomplete copied context")),
    }
}

pub(super) fn source_id(
    connection: &Connection,
    source: &SourceBlockId,
) -> Result<Option<i64>, FileFault> {
    Ok(connection.query_row(
        "SELECT id FROM content_block WHERE source_kind='event' AND run=?1 AND sequence=?2 AND block=?3",
        params![source.run, blob(source.sequence.0), blob(source.block.0)], |row| row.get(0),
    ).optional()?)
}

pub(super) fn blob(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

pub(super) fn decode_counter(value: Vec<u8>) -> Result<u64, FileFault> {
    let bytes: [u8; 8] = value
        .try_into()
        .map_err(|_| invalid_effect("invalid stored unsigned counter"))?;
    Ok(u64::from_be_bytes(bytes))
}

pub(super) fn positive_id(value: i64) -> Result<i64, FileFault> {
    if value <= 0 {
        return Err(fault(
            StateRefusalCode::WriteFailed,
            "internal identifier exhausted",
        ));
    }
    Ok(value)
}

pub(super) fn fault(code: StateRefusalCode, detail: impl Into<String>) -> FileFault {
    FileFault::new(code, detail)
}
pub(super) fn invalid_effect(detail: impl Into<String>) -> FileFault {
    fault(StateRefusalCode::InvalidEffect, detail)
}
pub(super) fn json_fault(error: serde_json::Error) -> FileFault {
    fault(StateRefusalCode::MalformedFiling, error.to_string())
}
