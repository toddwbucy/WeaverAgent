//! Structural checks on a complete store-owned read snapshot.
//!
//! HARNESS-READ and HARNESS-MATERIAL require exact selector association and
//! original content references. These checks create no recall, placement, plan,
//! or boot authority and never reclassify a retained statement.

use std::collections::BTreeSet;

use weaver_types::state_flow::{
    EventSource, PairEnded, PairFate, PairStarted, PlanRef, PlanSnapshot, QueryResult, ReadEntry,
    ReadPair, ReadSelector, ReadSource, SourcePathPart, Span, SpanRef, StatementRef, StepId,
    StepQueryRef, TerminalPairFate, ValidationError, decode_strict,
};

pub(super) fn validate(
    selector: &ReadSelector,
    result: &QueryResult,
) -> Result<(), ValidationError> {
    validate_selector_identities(selector)?;
    if !result.matches_selector(selector) {
        return Err(ValidationError("read result tag differs from its selector"));
    }
    match (selector, result) {
        (selector, QueryResult::Entries { entries }) => validate_entries(selector, entries),
        (ReadSelector::OpenPairs {}, QueryResult::OpenPairs { pairs }) => {
            unique_pairs(pairs)?;
            for pair in pairs {
                validate_pair(pair)?;
                if pair.fate != PairFate::Opened {
                    return Err(ValidationError("open_pairs returned a terminal pair"));
                }
            }
            Ok(())
        }
        (
            ReadSelector::SpanPreview { span, through },
            QueryResult::SpanPreview {
                span: actual,
                through: actual_through,
                entries,
            },
        ) => {
            if actual != span || actual_through != through {
                return Err(ValidationError(
                    "span preview changed its requested frontier",
                ));
            }
            for entry in entries {
                validate_entry(entry)?;
                require_span(entry, span)?;
            }
            Ok(())
        }
        (ReadSelector::PlanCurrent {}, QueryResult::PlanCurrent { current, .. }) => {
            current.as_ref().map_or(Ok(()), validate_plan)
        }
        (ReadSelector::Plan { plan }, QueryResult::Plan { snapshot }) => {
            if &snapshot.plan != plan {
                return Err(ValidationError(
                    "plan result changed the requested revision",
                ));
            }
            validate_plan(snapshot)
        }
        (
            ReadSelector::StepSuccessor {
                plan,
                previous_step,
            },
            QueryResult::StepSuccessor {
                reference,
                instruction,
            },
        ) => {
            validate_step_query(reference)?;
            if &reference.plan != plan
                || &reference.previous_step != previous_step
                || reference.step_id.seed_id != plan.plan_id.seed_id
                || instruction.step_query.as_ref() != Some(reference)
            {
                return Err(ValidationError(
                    "successor result changed its query reference",
                ));
            }
            validate_entry(instruction)?;
            require_statement(instruction, &reference.instruction)
        }
        (ReadSelector::PlanFlags { plan, after }, QueryResult::PlanFlags { snapshot }) => {
            validate_plan_ref(&snapshot.plan)?;
            // through is a store commit frontier. Run strings do not order it.
            if &snapshot.plan != plan || &snapshot.after != after || !snapshot.counts_agree() {
                return Err(ValidationError(
                    "plan flags changed an echo or count partition",
                ));
            }
            Ok(())
        }
        (
            ReadSelector::Observations {
                run,
                after,
                through,
            },
            QueryResult::Observations {
                run: actual_run,
                after: actual_after,
                through: actual_through,
                member_observation,
                records,
            },
        ) => {
            if actual_run != run
                || actual_after != after
                || actual_through < after
                || through.is_some_and(|value| value != *actual_through)
            {
                return Err(ValidationError(
                    "observation read changed its requested frontier",
                ));
            }
            member_observation.validate_historical()?;
            let mut previous = *after;
            for record in records {
                record.validate()?;
                let id = record.observation.operation_id;
                if id <= previous
                    || id > *actual_through
                    || record
                        .request_source
                        .as_ref()
                        .is_some_and(|source| &source.run != run)
                {
                    return Err(ValidationError(
                        "observation order or source run is invalid",
                    ));
                }
                previous = id;
            }
            Ok(())
        }
        (
            ReadSelector::Marks {},
            QueryResult::Marks {
                current_task,
                open_scopes,
                ..
            },
        ) => {
            if let Some(task) = current_task {
                validate_span_ref(&task.task)?;
                validate_span_ref(&task.prompt)?;
            }
            for scope in open_scopes {
                validate_span_ref(&scope.span)?;
                for span in [&scope.parent, &scope.replacement].into_iter().flatten() {
                    validate_span_ref(span)?;
                }
            }
            Ok(())
        }
        _ => Err(ValidationError(
            "read result has no matching validation branch",
        )),
    }
}

pub(super) fn validate_seed(seed: &str) -> Result<(), ValidationError> {
    if seed.len() == 32
        && seed
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ValidationError("read source has invalid seed identity"))
    }
}

pub(super) fn validate_span_ref(span: &SpanRef) -> Result<(), ValidationError> {
    validate_seed(&span.seed_id)?;
    if span.revision.0 != 1 {
        return Err(ValidationError("read span has invalid revision"));
    }
    Ok(())
}

pub(super) fn validate_plan_ref(plan: &PlanRef) -> Result<(), ValidationError> {
    validate_seed(&plan.plan_id.seed_id)?;
    if plan.plan_id.local_id.0 == 0 || plan.revision.0 == 0 {
        return Err(ValidationError("read plan has invalid identity"));
    }
    Ok(())
}

pub(super) fn validate_step_id(step: &StepId) -> Result<(), ValidationError> {
    validate_seed(&step.seed_id)?;
    if step.local_id.0 == 0 {
        return Err(ValidationError("read step has invalid identity"));
    }
    Ok(())
}

fn validate_step_query(reference: &StepQueryRef) -> Result<(), ValidationError> {
    validate_plan_ref(&reference.plan)?;
    validate_step_id(&reference.step_id)?;
    if let Some(previous) = &reference.previous_step {
        validate_step_id(previous)?;
    }
    Ok(())
}

fn validate_selector_identities(selector: &ReadSelector) -> Result<(), ValidationError> {
    match selector {
        ReadSelector::Span {
            seed_id,
            local_id,
            revision,
        } => validate_span_ref(&SpanRef {
            seed_id: seed_id.clone(),
            local_id: *local_id,
            revision: *revision,
        }),
        ReadSelector::SpanPreview { span, .. } => validate_span_ref(span),
        ReadSelector::Plan { plan } | ReadSelector::PlanFlags { plan, .. } => {
            validate_plan_ref(plan)
        }
        ReadSelector::StepSuccessor {
            plan,
            previous_step,
        } => {
            validate_plan_ref(plan)?;
            if let Some(step) = previous_step {
                validate_step_id(step)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_entries(selector: &ReadSelector, entries: &[ReadEntry]) -> Result<(), ValidationError> {
    let complete_bindings = matches!(
        selector,
        ReadSelector::Subjects { .. } | ReadSelector::Conversation { .. }
    );
    for entry in entries {
        validate_entry(entry)?;
        if complete_bindings
            && entry
                .bindings
                .iter()
                .enumerate()
                .any(|(index, binding)| binding.ordinal.0 != index as u64)
        {
            return Err(ValidationError(
                "whole-block read lost an original binding ordinal",
            ));
        }
        match selector {
            ReadSelector::History {
                noun,
                verb,
                condition,
            } => {
                let [binding] = entry.bindings.as_slice() else {
                    return Err(ValidationError("history must return one matched binding"));
                };
                if !binding.value.nouns.iter().any(|value| &value.value == noun)
                    || verb
                        .as_ref()
                        .is_some_and(|value| value != &binding.value.verb)
                    || condition
                        .as_ref()
                        .is_some_and(|value| Some(value) != binding.value.condition.as_ref())
                {
                    return Err(ValidationError(
                        "history binding does not match its selector",
                    ));
                }
                require_unfiltered_text(entry)?;
            }
            ReadSelector::Condition { condition } => {
                let [binding] = entry.bindings.as_slice() else {
                    return Err(ValidationError(
                        "condition read must return one matched binding",
                    ));
                };
                if binding.value.condition.as_ref() != Some(condition) {
                    return Err(ValidationError(
                        "condition binding does not match its selector",
                    ));
                }
                require_unfiltered_text(entry)?;
            }
            ReadSelector::Subjects { nouns } => {
                if !entry.bindings.iter().any(|binding| {
                    binding
                        .value
                        .nouns
                        .iter()
                        .any(|noun| nouns.contains(&noun.value))
                }) {
                    return Err(ValidationError("subject block has no matched binding"));
                }
                require_unfiltered_text(entry)?;
            }
            ReadSelector::Conversation { .. } => require_unfiltered_text(entry)?,
            ReadSelector::Span {
                seed_id,
                local_id,
                revision,
            } => require_span(
                entry,
                &SpanRef {
                    seed_id: seed_id.clone(),
                    local_id: *local_id,
                    revision: *revision,
                },
            )?,
            ReadSelector::Statement { statement } => {
                require_statement(entry, statement)?;
                require_unfiltered_text(entry)?;
            }
            ReadSelector::SystemPrompt {} => {
                let ReadSource::Seed(source) = &entry.source else {
                    return Err(ValidationError("system prompt is not the seeded source"));
                };
                if source.ordinal.0 != 0 {
                    return Err(ValidationError("system prompt changed the seed ordinal"));
                }
                if entry.bindings.is_empty()
                    || entry
                        .bindings
                        .iter()
                        .enumerate()
                        .any(|(index, binding)| binding.ordinal.0 != index as u64)
                {
                    return Err(ValidationError(
                        "system prompt lost its complete binding list",
                    ));
                }
                require_span(
                    entry,
                    &SpanRef {
                        seed_id: source.seed_id.clone(),
                        local_id: 0.into(),
                        revision: 1.into(),
                    },
                )?;
                let selected = entry
                    .selected_span
                    .as_ref()
                    .expect("require_span checked it");
                if selected.start.0 != 0 || selected.end.0 != entry.text.len() as u64 {
                    return Err(ValidationError(
                        "system prompt is not the complete seeded text",
                    ));
                }
            }
            _ => return Err(ValidationError("entries have a metadata selector")),
        }
    }
    if matches!(
        selector,
        ReadSelector::Statement { .. } | ReadSelector::SystemPrompt {}
    ) && entries.len() != 1
    {
        return Err(ValidationError(
            "single-source read did not return exactly one entry",
        ));
    }
    if complete_bindings {
        let mut sources = BTreeSet::new();
        for entry in entries {
            let ReadSource::Event(source) = &entry.source else {
                return Err(ValidationError(
                    "conversation or subjects returned seeded content",
                ));
            };
            if !matches!(
                source.envelope.kind.as_str(),
                "message.user" | "message.assistant"
            ) || (matches!(selector, ReadSelector::Conversation { .. })
                && source.envelope.turn.is_none())
                || !sources.insert(&source.block_id)
            {
                return Err(ValidationError(
                    "whole-block read repeats or changes a source",
                ));
            }
        }
    }
    Ok(())
}

fn require_unfiltered_text(entry: &ReadEntry) -> Result<(), ValidationError> {
    if entry.bindings.is_empty() || entry.selected_span.is_some() || entry.step_query.is_some() {
        return Err(ValidationError(
            "ordinary content read lost bindings or carries span metadata",
        ));
    }
    Ok(())
}

fn require_statement(entry: &ReadEntry, statement: &StatementRef) -> Result<(), ValidationError> {
    if !matches!(&entry.source, ReadSource::Event(source) if source.block_id == statement.block_id)
        || entry.bindings.len() != 1
        || entry.bindings[0].ordinal != statement.binding
        || entry.selected_span.is_some()
    {
        return Err(ValidationError(
            "statement read changed its source or binding",
        ));
    }
    Ok(())
}

fn require_span(entry: &ReadEntry, span: &SpanRef) -> Result<(), ValidationError> {
    if entry
        .selected_span
        .as_ref()
        .is_none_or(|selected| &selected.span_ref != span)
        || entry.pairs.iter().any(|pair| pair.fate == PairFate::Opened)
    {
        return Err(ValidationError(
            "span result lost its interval or contains an open pair",
        ));
    }
    Ok(())
}

fn validate_entry(entry: &ReadEntry) -> Result<(), ValidationError> {
    let raw: serde_json::Value = decode_strict(entry.raw_block.get())
        .map_err(|_| ValidationError("read raw block is not a complete Text block"))?;
    let object = raw
        .as_object()
        .ok_or(ValidationError("read raw block is not a Text object"))?;
    if object.len() != 2
        || object.get("type").and_then(serde_json::Value::as_str) != Some("text")
        || object.get("text").and_then(serde_json::Value::as_str) != Some(entry.text.as_str())
    {
        return Err(ValidationError(
            "read text differs from its complete raw block",
        ));
    }
    match &entry.source {
        ReadSource::Event(source) => {
            validate_event_source(source)?;
            use SourcePathPart::{Index, Key};
            let text_path = match source.source_path.as_slice() {
                [Key(content), Index { index }] => {
                    content == "content" && *index == source.block_id.block
                }
                [Key(message), Key(content), Index { index }] => {
                    message == "message" && content == "content" && *index == source.block_id.block
                }
                _ => false,
            };
            if !text_path {
                return Err(ValidationError(
                    "read source does not address its original Text index",
                ));
            }
        }
        ReadSource::Seed(source) => {
            validate_seed(&source.seed_id)?;
            if !entry.pairs.is_empty() {
                return Err(ValidationError("seed source has runtime pairs"));
            }
        }
    }
    if let Some(selected) = &entry.selected_span {
        validate_span_ref(&selected.span_ref)?;
    }
    if let Some(reference) = &entry.step_query {
        validate_step_query(reference)?;
    }
    let selected = entry.selected_span.as_ref().map(|selected| Span {
        start: selected.start,
        end: selected.end,
    });
    if let Some(selected) = selected {
        selected.slice(&entry.text)?;
    }
    let mut previous = None;
    let mut previous_start = None;
    for binding in &entry.bindings {
        if previous.is_some_and(|ordinal| ordinal >= binding.ordinal)
            || previous_start.is_some_and(|start| start > binding.value.statement_span.start)
        {
            return Err(ValidationError(
                "read bindings lost original order or unique ordinals",
            ));
        }
        // The held verb is already classified. Only validate its retained byte
        // references here, without inventing a second dictionary election.
        binding
            .value
            .validate(&entry.text, std::slice::from_ref(&binding.value.verb))?;
        if selected.is_some_and(|span| !span.contains(&binding.value.statement_span)) {
            return Err(ValidationError(
                "selected interval only partly contains a binding",
            ));
        }
        previous = Some(binding.ordinal);
        previous_start = Some(binding.value.statement_span.start);
    }
    unique_pairs(&entry.pairs)?;
    for pair in &entry.pairs {
        validate_pair(pair)?;
        if !matches!(&entry.source, ReadSource::Event(source) if source.block_id == pair.statement.block_id)
            || !entry
                .bindings
                .iter()
                .any(|binding| binding.ordinal == pair.statement.binding)
        {
            return Err(ValidationError(
                "pair attachment does not name a retained binding",
            ));
        }
    }
    Ok(())
}

fn validate_event_source(source: &EventSource) -> Result<(), ValidationError> {
    if source.block_id.run != source.envelope.run
        || source.block_id.sequence != source.envelope.sequence
        || source.partition_rule_id.is_empty()
    {
        return Err(ValidationError(
            "read source differs from its envelope or admission rule",
        ));
    }
    Ok(())
}

fn unique_pairs(pairs: &[ReadPair]) -> Result<(), ValidationError> {
    let mut calls = BTreeSet::new();
    for pair in pairs {
        if !calls.insert((&pair.call_id.run, pair.call_id.sequence, pair.call_id.call)) {
            return Err(ValidationError("read repeats one pair attachment"));
        }
    }
    Ok(())
}

fn validate_pair(pair: &ReadPair) -> Result<(), ValidationError> {
    pair.validate_structure()?;
    validate_pair_source(&pair.opening_source.source, "tool.call.started")?;
    let opening: PairStarted = decode_strict(pair.opening_source.raw_block.get())
        .map_err(|_| ValidationError("pair opening is not its complete typed payload"))?;
    opening.validate_structure()?;
    if opening.call_id != pair.call_id
        || opening.statement != pair.statement
        || opening.lane != pair.lane
        || opening.request != pair.request
    {
        return Err(ValidationError(
            "pair attachment disagrees with its opening payload",
        ));
    }
    match &pair.terminal_source {
        None if pair.fate == PairFate::Opened && pair.reason.is_none() => Ok(()),
        Some(source) if pair.fate != PairFate::Opened => {
            validate_pair_source(&source.source, "tool.call.completed")?;
            let terminal: PairEnded = decode_strict(source.raw_block.get())
                .map_err(|_| ValidationError("pair terminal is not its complete typed payload"))?;
            let fate = match terminal.fate {
                TerminalPairFate::Completed => PairFate::Completed,
                TerminalPairFate::Failed => PairFate::Failed,
            };
            if terminal.call_id != pair.call_id
                || terminal.reason != pair.reason
                || fate != pair.fate
            {
                return Err(ValidationError(
                    "pair attachment disagrees with its terminal payload",
                ));
            }
            Ok(())
        }
        _ => Err(ValidationError("pair fate and terminal source disagree")),
    }
}

fn validate_pair_source(source: &EventSource, kind: &str) -> Result<(), ValidationError> {
    validate_event_source(source)?;
    if source.block_id.block.0 != 0
        || !source.source_path.is_empty()
        || source.envelope.kind != kind
    {
        return Err(ValidationError(
            "pair source does not name the whole pair event",
        ));
    }
    Ok(())
}

fn validate_plan(snapshot: &PlanSnapshot) -> Result<(), ValidationError> {
    // The store owns transition legality. Here retain the read's documented step
    // ordering and seed qualification without turning a snapshot into authority.
    // Strict decoding requires the operator digest and complete last-source
    // position, including explicit null turn. Their provenance cannot be inferred
    // from the response itself; a plan owner must compare its own held expectations.
    validate_plan_ref(&snapshot.plan)?;
    validate_span_ref(&snapshot.task.task)?;
    validate_span_ref(&snapshot.task.prompt)?;
    let seed = &snapshot.plan.plan_id.seed_id;
    if snapshot.task.task.seed_id != *seed || snapshot.task.prompt.seed_id != *seed {
        return Err(ValidationError(
            "plan task spans belong to a different seed",
        ));
    }
    let mut previous = None;
    let mut identities = BTreeSet::new();
    for step in &snapshot.steps {
        validate_step_id(&step.step_id)?;
        for span in [&step.span, &step.outcome_span].into_iter().flatten() {
            validate_span_ref(span)?;
        }
        if let Some(work) = &step.work {
            validate_plan_ref(&work.plan)?;
            validate_step_id(&work.step_id)?;
            validate_span_ref(&work.span)?;
        }
        if step.step_id.seed_id != *seed
            || previous.is_some_and(|ordinal| ordinal >= step.ordinal)
            || !identities.insert(step.step_id.local_id)
        {
            return Err(ValidationError(
                "plan steps lost seed identity or ordinal order",
            ));
        }
        previous = Some(step.ordinal);
    }
    if let Some(step) = snapshot
        .broken
        .as_ref()
        .and_then(|broken| broken.step_id.as_ref())
    {
        validate_step_id(step)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "read_validate_tests.rs"]
mod tests;
