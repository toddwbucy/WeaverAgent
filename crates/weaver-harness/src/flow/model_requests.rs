//! Original-generation request identity, from OUTPUT-UNIT, TOOL-STATEMENT,
//! RECALL and PAIR-IDENTITY. This is evidence for later repair and pair accounting.
//! It neither substitutes corrections nor opens a pair, files, dispatches or reads.

use std::collections::BTreeSet;

use sha2::{Digest as _, Sha256};
use weaver_traits::{ContentBlock, Role};
use weaver_types::state_flow::{
    CallId, Digest, PairLane, PairRequest, PairStarted, ProtocolRepairCase, RecallRequestRefusal,
    SchemeDeclaration, SourceBlockId, StatementRef, Ste100Result, Ste100Scheme, Ste100Target,
    decode_strict,
};

use super::{CheckedGeneration, RecordedOutputBlock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum AssociationError {
    InvalidScheme,
    WrongScheme,
    InvalidGeneration,
    MissingCheck,
    InvalidCheck,
    CounterExhausted,
}

/// Recall meaning comes from the exact loaded artifact, not a caller's key list.
/// Digest/shape verification here makes no language-qualification claim.
#[derive(Debug)]
pub(in crate::flow) struct RequestScheme {
    declaration: SchemeDeclaration,
    recall_keys: BTreeSet<String>,
}

impl RequestScheme {
    pub(in crate::flow) fn from_exact_scheme(
        text: &str,
        declaration: &SchemeDeclaration,
    ) -> Result<Self, AssociationError> {
        let artifact: Ste100Scheme =
            decode_strict(text).map_err(|_| AssociationError::InvalidScheme)?;
        let digest = hash(text.as_bytes());
        artifact
            .validate(&digest, declaration)
            .and_then(|()| artifact.verify_blobs(hash))
            .map_err(|_| AssociationError::InvalidScheme)?;
        Ok(Self {
            declaration: declaration.clone(),
            recall_keys: artifact.recall_keys.into_iter().collect(),
        })
    }

    pub(in crate::flow) fn scheme_id(&self) -> &str {
        &self.declaration.scheme_id
    }

    fn is_recall(&self, verb: &str) -> bool {
        self.recall_keys.contains(verb)
    }
}

fn hash(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256's hexadecimal rendering is a Digest")
}

/// An associated original request, never an opening receipt or execution grant.
#[derive(Debug)]
pub(in crate::flow) struct ModelRequestCandidate {
    opening: PairStarted,
}

impl ModelRequestCandidate {
    pub(in crate::flow) fn opening(&self) -> &PairStarted {
        &self.opening
    }

    pub(in crate::flow) fn dependency(&self) -> &StatementRef {
        &self.opening.statement
    }
}

/// A rejected adjacent Text can be repaired. Keep its original executable bytes
/// and stable call identity without claiming a successful statement association.
#[derive(Debug)]
pub(in crate::flow) struct PendingAssociation {
    call_id: CallId,
    call_block: u64,
    rejected_source: SourceBlockId,
    request: PairRequest,
}

impl PendingAssociation {
    pub(in crate::flow) fn call_id(&self) -> &CallId {
        &self.call_id
    }

    pub(in crate::flow) fn call_block(&self) -> u64 {
        self.call_block
    }

    pub(in crate::flow) fn rejected_source(&self) -> &SourceBlockId {
        &self.rejected_source
    }

    pub(in crate::flow) fn request(&self) -> &PairRequest {
        &self.request
    }
}

/// The caller records external association refusals with ProtocolRepair.
/// InvalidRecall preserves the checked facts for the trace-only RECALL-REFUSAL.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::flow) enum AssociationRefusal {
    External {
        call_id: CallId,
        call_block: u64,
        source: SourceBlockId,
        case: ProtocolRepairCase,
        detail: &'static str,
    },
    InvalidRecall {
        call_id: CallId,
        call_block: u64,
        statement: StatementRef,
        noun_count: usize,
        has_condition: bool,
    },
}

impl AssociationRefusal {
    pub(in crate::flow) fn recall_refusal(&self) -> Option<RecallRequestRefusal> {
        let Self::InvalidRecall {
            call_id,
            call_block,
            statement,
            noun_count,
            has_condition,
        } = self
        else {
            return None;
        };
        Some(RecallRequestRefusal {
            call_id: call_id.clone(),
            call_block: (*call_block).into(),
            statement: statement.clone(),
            noun_count: u64::try_from(*noun_count)
                .expect("checked binding count fits u64")
                .into(),
            has_condition: *has_condition,
            detail: RecallRequestRefusal::DETAIL.into(),
        })
    }
}

#[derive(Debug)]
pub(in crate::flow) struct AssociationFailure {
    pub generation: CheckedGeneration,
    pub error: AssociationError,
}

/// The original proof remains owned. Accepted requests stay discoverable even
/// when a sibling needs repair, including inside a forbidden correction pass.
/// Only the repair owner can later construct EffectiveEmission.
#[derive(Debug)]
pub(in crate::flow) struct OriginalRequests {
    generation: CheckedGeneration,
    requests: Vec<ModelRequestCandidate>,
    unresolved: Vec<PendingAssociation>,
    refusals: Vec<AssociationRefusal>,
    attempted_call: bool,
}

impl OriginalRequests {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn from_checked(
        generation: CheckedGeneration,
        scheme: &RequestScheme,
    ) -> Result<Self, AssociationFailure> {
        let associated = associate(&generation, scheme);
        match associated {
            Ok((requests, unresolved, refusals, attempted_call)) => Ok(Self {
                generation,
                requests,
                unresolved,
                refusals,
                attempted_call,
            }),
            Err(error) => Err(AssociationFailure { generation, error }),
        }
    }

    pub(in crate::flow) fn generation(&self) -> &CheckedGeneration {
        &self.generation
    }

    /// Source-content order, with binding order breaking an inward tie.
    pub(in crate::flow) fn requests(&self) -> &[ModelRequestCandidate] {
        &self.requests
    }

    pub(in crate::flow) fn dependencies(&self) -> impl Iterator<Item = &StatementRef> {
        self.requests.iter().map(ModelRequestCandidate::dependency)
    }

    pub(in crate::flow) fn unresolved(&self) -> &[PendingAssociation] {
        &self.unresolved
    }

    pub(in crate::flow) fn refusals(&self) -> &[AssociationRefusal] {
        &self.refusals
    }

    pub(in crate::flow) fn attempted_call(&self) -> bool {
        self.attempted_call
    }

    pub(in crate::flow) fn into_generation(self) -> CheckedGeneration {
        self.generation
    }
}

type Associations = (
    Vec<ModelRequestCandidate>,
    Vec<PendingAssociation>,
    Vec<AssociationRefusal>,
    bool,
);

fn associate(
    generation: &CheckedGeneration,
    scheme: &RequestScheme,
) -> Result<Associations, AssociationError> {
    validate_generation(generation, scheme)?;
    let message = &generation.original().message;
    let source = &generation.source().block_id;
    let external_count = u64::try_from(
        message
            .content
            .iter()
            .filter(|block| matches!(block, ContentBlock::ToolCall(_)))
            .count(),
    )
    .map_err(|_| AssociationError::CounterExhausted)?;
    let mut external = 0;
    let mut recall = external_count;
    let mut requests = Vec::new();
    let mut unresolved = Vec::new();
    let mut refusals = Vec::new();
    let mut attempted = false;
    let mut check_index = 0;
    for (index, block) in message.content.iter().enumerate() {
        let index = u64::try_from(index).map_err(|_| AssociationError::CounterExhausted)?;
        let block_source = SourceBlockId {
            block: index.into(),
            ..source.clone()
        };
        match block {
            ContentBlock::Text { .. } => {
                let check = &generation.blocks()[check_index];
                check_index += 1;
                if let Ste100Result::Accepted { bindings } = &check.reply().result {
                    for (binding_index, binding) in bindings.iter().enumerate() {
                        if !scheme.is_recall(&binding.verb) {
                            continue;
                        }
                        attempted = true;
                        let call_id = call_id(source, recall);
                        recall = recall
                            .checked_add(1)
                            .ok_or(AssociationError::CounterExhausted)?;
                        let statement = StatementRef {
                            block_id: block_source.clone(),
                            binding: u64::try_from(binding_index)
                                .map_err(|_| AssociationError::CounterExhausted)?
                                .into(),
                        };
                        if binding.nouns.len() != 1 || binding.condition.is_some() {
                            refusals.push(AssociationRefusal::InvalidRecall {
                                call_id,
                                call_block: index,
                                statement,
                                noun_count: binding.nouns.len(),
                                has_condition: binding.condition.is_some(),
                            });
                            continue;
                        }
                        requests.push(ModelRequestCandidate {
                            opening: PairStarted {
                                call_id,
                                call_block: index.into(),
                                statement,
                                lane: PairLane::Recall,
                                request: PairRequest::Recall {
                                    subject: binding.nouns[0].value.clone(),
                                },
                                retry_of: None,
                            },
                        });
                    }
                }
            }
            ContentBlock::ToolCall(tool) => {
                attempted = true;
                let call_id = call_id(source, external);
                external = external
                    .checked_add(1)
                    .ok_or(AssociationError::CounterExhausted)?;
                let request = PairRequest::External {
                    name: tool.name.clone(),
                    arguments: tool.arguments.clone(),
                };
                let prior = index.checked_sub(1).and_then(|previous| {
                    match &message.content[previous as usize] {
                        ContentBlock::Text { .. } => generation.blocks().get(check_index - 1),
                        _ => None,
                    }
                });
                let Some(prior) = prior else {
                    refusals.push(AssociationRefusal::External {
                        call_id,
                        call_block: index,
                        source: block_source,
                        case: ProtocolRepairCase::MissingToolStatement,
                        detail: "tool call has no immediately preceding Text block",
                    });
                    continue;
                };
                let Ste100Target::Source(statement_source) = &prior.request().target else {
                    return Err(AssociationError::InvalidCheck);
                };
                match &prior.reply().result {
                    Ste100Result::NotSte100 { .. } => unresolved.push(PendingAssociation {
                        call_id,
                        call_block: index,
                        rejected_source: statement_source.clone(),
                        request,
                    }),
                    Ste100Result::Accepted { bindings }
                        if bindings.len() == 1 && !scheme.is_recall(&bindings[0].verb) =>
                    {
                        requests.push(ModelRequestCandidate {
                            opening: PairStarted {
                                call_id,
                                call_block: index.into(),
                                statement: StatementRef {
                                    block_id: statement_source.clone(),
                                    binding: 0.into(),
                                },
                                lane: PairLane::External,
                                request,
                                retry_of: None,
                            },
                        });
                    }
                    Ste100Result::Accepted { bindings } => {
                        refusals.push(AssociationRefusal::External {
                            call_id,
                            call_block: index,
                            source: block_source,
                            case: ProtocolRepairCase::AmbiguousToolStatement,
                            detail: if bindings.len() != 1 {
                                "tool statement does not contain exactly one accepted binding"
                            } else {
                                "recall statement cannot also supply an external send"
                            },
                        });
                    }
                    Ste100Result::ComponentFailed { .. } => {
                        return Err(AssociationError::InvalidCheck);
                    }
                }
            }
            ContentBlock::UnrecoveredCall { .. } => attempted = true,
            _ => return Err(AssociationError::InvalidGeneration),
        }
    }
    Ok((requests, unresolved, refusals, attempted))
}

fn call_id(source: &SourceBlockId, ordinal: u64) -> CallId {
    CallId {
        run: source.run.clone(),
        sequence: source.sequence,
        call: ordinal.into(),
    }
}

fn validate_generation(
    generation: &CheckedGeneration,
    scheme: &RequestScheme,
) -> Result<(), AssociationError> {
    if generation.scheme() != &scheme.declaration {
        return Err(AssociationError::WrongScheme);
    }
    let original = generation.original();
    let source = generation.source();
    if original.message.role != Role::Assistant
        || original.turn.0.is_empty()
        || source.block_id.run.is_empty()
        || source.block_id.sequence.0 == 0
        || source.block_id.block.0 != 0
        || source.turn.as_ref() != Some(&original.turn.0)
    {
        return Err(AssociationError::InvalidGeneration);
    }
    let mut next = generation.blocks().iter();
    for (index, block) in original.message.content.iter().enumerate() {
        let ContentBlock::Text { text } = block else {
            continue;
        };
        let check = next.next().ok_or(AssociationError::MissingCheck)?;
        validate_check(generation, scheme, check, index, text)?;
    }
    if next.next().is_some() {
        return Err(AssociationError::InvalidCheck);
    }
    Ok(())
}

fn validate_check(
    generation: &CheckedGeneration,
    scheme: &RequestScheme,
    check: &RecordedOutputBlock,
    index: usize,
    text: &str,
) -> Result<(), AssociationError> {
    if check.request().scheme_id != scheme.scheme_id()
        || check.reply().scheme_id != scheme.scheme_id()
    {
        return Err(AssociationError::WrongScheme);
    }
    let expected = SourceBlockId {
        block: u64::try_from(index)
            .map_err(|_| AssociationError::CounterExhausted)?
            .into(),
        ..generation.source().block_id.clone()
    };
    let result_source = check.result_source();
    if check.request().target != Ste100Target::Source(expected)
        || check.request().text != text
        || result_source.block_id.run != generation.source().block_id.run
        || result_source.block_id.sequence <= generation.source().block_id.sequence
        || result_source.block_id.block.0 != 0
        || result_source.turn != generation.source().turn
        || check.recorded_ns() > generation.recorded_ns()
        || matches!(check.reply().result, Ste100Result::ComponentFailed { .. })
        || check
            .reply()
            .validate_for(check.request(), &scheme.declaration.verbs)
            .is_err()
    {
        return Err(AssociationError::InvalidCheck);
    }
    Ok(())
}

#[cfg(test)]
#[path = "model_requests_tests.rs"]
pub(in crate::flow) mod tests;
