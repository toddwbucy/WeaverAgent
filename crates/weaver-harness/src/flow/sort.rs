//! Pure outbound identity validation and exact noun extraction (Spec 08).
//! No port here can file, open a pair, dispatch, classify, or mutate an argument.

use std::collections::BTreeSet;

use weaver_types::state_flow::{Binding, PairLane, SortAct, SourcePosition};

use super::context::EffectiveEmission;

/// The caller supplies checked bindings. Preserve compound nouns and exact bytes.
pub(super) fn noun_keys(bindings: &[Binding]) -> Vec<String> {
    bindings
        .iter()
        .flat_map(|binding| binding.nouns.iter().map(|noun| noun.value.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Choice of acts belongs to the caller, including whether this is an answer.
#[derive(Debug)]
pub(super) struct OutboundInput {
    pub effective: EffectiveEmission,
    pub source: SourcePosition,
    pub acts: Vec<SortAct>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutboundRefusal {
    InvalidCandidate,
}

/// Checked identities only. The context must record before releasing these acts.
#[derive(Debug)]
pub(super) struct CheckedOutbound {
    input: OutboundInput,
}

impl CheckedOutbound {
    pub(super) fn input(&self) -> &OutboundInput {
        &self.input
    }
    pub(super) fn into_input(self) -> OutboundInput {
        self.input
    }
}

#[allow(clippy::result_large_err)]
pub(super) fn identity_sort(
    input: OutboundInput,
) -> Result<CheckedOutbound, (OutboundInput, OutboundRefusal)> {
    if !valid(&input) {
        return Err((input, OutboundRefusal::InvalidCandidate));
    }
    Ok(CheckedOutbound { input })
}

fn valid(input: &OutboundInput) -> bool {
    if &input.source != input.effective.source() {
        return false;
    }
    let mut calls = BTreeSet::new();
    let mut answers = BTreeSet::new();
    // The logical order includes corrected blocks at their original position.
    let mut answer_order = Vec::new();
    for statement in input.effective.statements() {
        if !answer_order.contains(&&statement.block_id) {
            answer_order.push(&statement.block_id);
        }
    }
    let mut last_answer = None;
    for act in &input.acts {
        match act {
            SortAct::Tool { call_id, statement } | SortAct::Recall { call_id, statement } => {
                let lane = match act {
                    SortAct::Tool { .. } => PairLane::External,
                    SortAct::Recall { .. } => PairLane::Recall,
                    SortAct::Answer { .. } => unreachable!(),
                };
                if !calls.insert((&call_id.run, call_id.sequence.0, call_id.call.0))
                    || !input.effective.statements().contains(statement)
                    || !input.effective.requests().iter().any(|request| {
                        &request.call_id == call_id
                            && &request.statement == statement
                            && request.lane == lane
                            && request.request.lane() == lane
                    })
                {
                    return false;
                }
            }
            SortAct::Answer { source } => {
                let Some(position) = answer_order
                    .iter()
                    .position(|candidate| *candidate == source)
                else {
                    return false;
                };
                if !answers.insert((&source.run, source.sequence.0, source.block.0))
                    || last_answer.is_some_and(|last| last >= position)
                {
                    return false;
                }
                last_answer = Some(position);
            }
        }
    }
    true
}
