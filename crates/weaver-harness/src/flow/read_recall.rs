//! Faithful provenance for an already checked and released read.
use std::collections::{BTreeMap, BTreeSet};

use weaver_trace::serving::payloads as trace;
use weaver_types::state_flow as shared;

use super::{CompletedRead, PageReadError};
use crate::flow::mirror;

#[cfg(test)]
#[path = "read_recall_tests.rs"]
mod tests;

pub(super) fn convert(read: &CompletedRead) -> Result<trace::RecallRecord, PageReadError> {
    let selector = selector(read.selector())?;
    let mut walk = Walk::default();
    match read.result() {
        shared::QueryResult::Entries { entries }
        | shared::QueryResult::SpanPreview { entries, .. } => {
            for entry in entries {
                walk.entry(entry)?;
            }
        }
        shared::QueryResult::StepSuccessor {
            reference,
            instruction,
        } => {
            walk.entry(instruction)?;
            walk.statement(&reference.instruction);
        }
        shared::QueryResult::OpenPairs { pairs } => {
            for pair in pairs {
                walk.pair(pair);
            }
        }
        shared::QueryResult::Marks { open_scopes, .. } => {
            for scope in open_scopes {
                if let Some(source) = &scope.opened_source {
                    walk.event(source);
                }
                if let Some(source) = &scope.ended_source {
                    walk.event(source);
                }
            }
        }
        shared::QueryResult::PlanCurrent { current, .. } => {
            if let Some(snapshot) = current {
                walk.plan(snapshot);
            }
        }
        shared::QueryResult::Plan { snapshot } => walk.plan(snapshot),
        shared::QueryResult::PlanFlags { snapshot } => {
            walk.event(&snapshot.after);
            walk.event(&snapshot.through.block_id);
        }
        shared::QueryResult::Observations { .. } => {}
    }
    Ok(trace::RecallRecord::Read {
        selector,
        snapshot_id: number(read.snapshot_id()),
        sources: walk
            .sources
            .into_iter()
            .map(|(source, ordinals)| trace::RecallSource {
                source,
                binding_ordinals: ordinals.into_iter().map(trace::DecimalU64).collect(),
            })
            .collect(),
        spans: walk.spans,
    })
}

#[derive(Default)]
struct Walk {
    indices: BTreeMap<(u8, String, u64, u64), usize>,
    sources: Vec<(trace::ContentRef, BTreeSet<u64>)>,
    spans: Vec<trace::RecallSpan>,
}

impl Walk {
    fn source(&mut self, source: trace::ContentRef) -> &mut BTreeSet<u64> {
        let key = match &source {
            trace::ContentRef::Seed { seed_id, ordinal } => (0, seed_id.clone(), ordinal.0, 0),
            trace::ContentRef::Event { block_id } => (
                1,
                block_id.run.clone(),
                block_id.sequence.0,
                block_id.block.0,
            ),
        };
        let index = *self.indices.entry(key).or_insert_with(|| {
            let index = self.sources.len();
            self.sources.push((source, BTreeSet::new()));
            index
        });
        &mut self.sources[index].1
    }

    fn event(&mut self, source: &shared::SourceBlockId) {
        self.source(trace::ContentRef::Event {
            block_id: mirror::source(source),
        });
    }

    fn statement(&mut self, statement: &shared::StatementRef) {
        self.source(trace::ContentRef::Event {
            block_id: mirror::source(&statement.block_id),
        })
        .insert(statement.binding.0);
    }

    fn entry(&mut self, entry: &shared::ReadEntry) -> Result<(), PageReadError> {
        let source = match &entry.source {
            shared::ReadSource::Seed(source) => {
                seed(&source.seed_id)?;
                trace::ContentRef::Seed {
                    seed_id: source.seed_id.clone(),
                    ordinal: number(source.ordinal),
                }
            }
            shared::ReadSource::Event(source) => trace::ContentRef::Event {
                block_id: mirror::source(&source.block_id),
            },
        };
        self.source(source.clone())
            .extend(entry.bindings.iter().map(|binding| binding.ordinal.0));
        if let Some(reference) = &entry.step_query {
            self.statement(&reference.instruction);
        }
        for pair in &entry.pairs {
            self.pair(pair);
        }
        if let Some(selected) = &entry.selected_span {
            self.spans.push(trace::RecallSpan {
                source,
                selected_span: trace::RecallSelection {
                    span_ref: span(&selected.span_ref)?,
                    start: number(selected.start),
                    end: number(selected.end),
                },
            });
        }
        Ok(())
    }

    fn pair(&mut self, pair: &shared::ReadPair) {
        self.statement(&pair.statement);
        self.event(&pair.opening_source.source.block_id);
        if let Some(source) = &pair.terminal_source {
            self.event(&source.source.block_id);
        }
    }

    fn plan(&mut self, plan: &shared::PlanSnapshot) {
        self.event(&plan.operator_source);
        if let Some(source) = &plan.lock_source {
            self.event(source);
        }
        self.event(&plan.last_source.block_id);
        for step in &plan.steps {
            self.statement(&step.instruction);
            if let Some(work) = &step.work {
                self.event(&work.through.block_id);
            }
            if let Some(source) = &step.closure_source {
                self.event(source);
            }
        }
        if let Some(broken) = &plan.broken {
            if let Some(source) = &broken.proposed_source {
                self.event(source);
            }
            self.event(&broken.source);
        }
    }
}

fn number(value: shared::DecimalU64) -> trace::DecimalU64 {
    trace::DecimalU64(value.0)
}

fn seed(seed: &str) -> Result<(), PageReadError> {
    crate::flow::read_validate::validate_seed(seed)
        .map_err(|_| PageReadError("recall source has invalid seed identity"))
}

fn span(value: &shared::SpanRef) -> Result<trace::SpanRef, PageReadError> {
    crate::flow::read_validate::validate_span_ref(value)
        .map_err(|_| PageReadError("recall span has invalid identity"))?;
    Ok(trace::SpanRef {
        seed_id: value.seed_id.clone(),
        local_id: number(value.local_id),
        revision: number(value.revision),
    })
}

fn plan(value: &shared::PlanRef) -> Result<trace::PlanRef, PageReadError> {
    crate::flow::read_validate::validate_plan_ref(value)
        .map_err(|_| PageReadError("recall plan has invalid identity"))?;
    Ok(trace::PlanRef {
        plan_id: trace::PlanId {
            seed_id: value.plan_id.seed_id.clone(),
            local_id: number(value.plan_id.local_id),
        },
        revision: number(value.revision),
    })
}

fn step(value: &shared::StepId) -> Result<trace::StepId, PageReadError> {
    crate::flow::read_validate::validate_step_id(value)
        .map_err(|_| PageReadError("recall step has invalid identity"))?;
    Ok(trace::StepId {
        seed_id: value.seed_id.clone(),
        local_id: number(value.local_id),
    })
}

fn selector(value: &shared::ReadSelector) -> Result<trace::ReadSelector, PageReadError> {
    use shared::ReadSelector as S;
    use trace::ReadSelector as T;
    value
        .validate_shape()
        .map_err(|_| PageReadError("recall selector has invalid shape"))?;
    Ok(match value {
        S::History {
            noun,
            verb,
            condition,
        } => T::History {
            noun: noun.clone(),
            verb: verb.clone(),
            condition: condition.clone(),
        },
        S::Subjects { nouns } => T::Subjects {
            nouns: nouns.clone(),
        },
        S::Statement { statement } => T::Statement {
            statement: trace::StatementRef {
                block_id: mirror::source(&statement.block_id),
                binding: number(statement.binding),
            },
        },
        S::Condition { condition } => T::Condition {
            condition: condition.clone(),
        },
        S::Span {
            seed_id,
            local_id,
            revision,
        } => {
            let checked = span(&shared::SpanRef {
                seed_id: seed_id.clone(),
                local_id: *local_id,
                revision: *revision,
            })?;
            T::Span {
                seed_id: checked.seed_id,
                local_id: checked.local_id,
                revision: checked.revision,
            }
        }
        S::SystemPrompt {} => T::SystemPrompt {},
        S::Conversation { last_turns } => T::Conversation {
            last_turns: number(*last_turns),
        },
        S::OpenPairs {} => T::OpenPairs {},
        S::Marks {} => T::Marks {},
        S::SpanPreview {
            span: selected,
            through,
        } => T::SpanPreview {
            span: span(selected)?,
            through: trace::SourcePosition {
                block_id: mirror::source(&through.block_id),
                turn: through.turn.clone(),
            },
        },
        S::PlanCurrent {} => T::PlanCurrent {},
        S::Plan { plan: reference } => T::Plan {
            plan: plan(reference)?,
        },
        S::StepSuccessor {
            plan: reference,
            previous_step,
        } => T::StepSuccessor {
            plan: plan(reference)?,
            previous_step: previous_step.as_ref().map(step).transpose()?,
        },
        S::PlanFlags {
            plan: reference,
            after,
        } => T::PlanFlags {
            plan: plan(reference)?,
            after: mirror::source(after),
        },
        S::Observations {
            run,
            after,
            through,
        } => T::Observations {
            run: run.clone(),
            after: number(*after),
            through: through.map(number),
        },
    })
}
