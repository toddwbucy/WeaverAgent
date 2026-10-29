//! Exact operator bytes and their indexed content projection, never a model grant.
use crate::flow::author::{AuthoredEvent, AuthoredSource};
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::{FilingCompletion, RecordingContext};
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::pump::PumpDeadline;
use crate::flow::pump::plan::RootPlanPacket;
use sha2::{Digest as _, Sha256};
use std::sync::Arc;
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::state_flow as s;

#[path = "plan_effect.rs"]
mod effect;
#[path = "plan_preflight.rs"]
mod preflight;
pub(in crate::flow) use effect::{PreparedRootEffect, RootPlanEvidence};

#[derive(Debug, Clone)]
pub(in crate::flow) struct ManagementInput {
    cause: weaver_trace::Cause,
}
impl ManagementInput {
    pub(in crate::flow) fn cause(&self) -> weaver_trace::Cause {
        self.cause
    }
    #[cfg(test)]
    pub(in crate::flow) fn for_test_root() -> Self {
        Self {
            cause: weaver_trace::Cause { uid: 0 },
        }
    }
}

/// Its sole production constructor consumes the root transport's private receipt.
/// It proves root admission and exact document shape, not classification or filing.
pub(in crate::flow) struct AcceptedPlanUpdate {
    packet: RootPlanPacket,
    document: s::PlanUpdateDocument,
    message: Message,
    context: s::PlanInputContext,
    source: ManagementInput,
    origin: Option<Arc<()>>,
}
impl AcceptedPlanUpdate {
    #[allow(clippy::result_large_err)]
    pub(in crate::flow) fn from_received(
        packet: RootPlanPacket,
    ) -> Result<Self, (RootPlanPacket, s::PlanUpdateRefused)> {
        let (cause, digest, raw) = packet.input();
        let refusal = |detail: &str| s::PlanUpdateRefused {
            case: s::PlanUpdateCase::MalformedPlan,
            detail: detail.into(),
            rejected_blocks: vec![],
            read_code: None,
        };
        if format!("{:x}", Sha256::digest(raw.as_bytes())) != digest.as_str() {
            return Err((
                packet,
                refusal("plan document digest does not match the received bytes"),
            ));
        }
        let document: s::PlanUpdateDocument = match s::decode_strict(raw) {
            Ok(document) => document,
            Err(_) => return Err((packet, refusal("plan document has an invalid shape"))),
        };
        if document.validate_shape().is_err() {
            return Err((
                packet,
                refusal("plan document has an invalid operation shape"),
            ));
        }
        let (message, operation) = project(&document.operation);
        let context = s::PlanInputContext {
            document_sha256: digest.clone(),
            cause,
            expected: document.expected.clone(),
            operation,
        };
        Ok(Self {
            packet,
            document,
            message,
            context,
            source: ManagementInput {
                cause: weaver_trace::Cause { uid: cause.uid },
            },
            origin: None,
        })
    }
    pub(in crate::flow) fn bind_to(&mut self, context: &RecordingContext) {
        assert!(self.origin.is_none(), "root input must not be rebound");
        self.origin = Some(context.enter_origin().clone());
    }
    pub(in crate::flow) fn belongs_to(&self, context: &RecordingContext) -> bool {
        self.origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, context.enter_origin()))
    }
    pub(in crate::flow) fn event(
        &self,
        context: &RecordingContext,
    ) -> Result<AuthoredEvent, &'static str> {
        if !self.belongs_to(context) {
            return Err("operator input belongs to another recording context");
        }
        Ok(AuthoredEvent {
            kind: weaver_trace::serving::Kind::PlanEvent,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: None,
            source: AuthoredSource::ManagementInput(self.source()),
            causal_parent: None,
            payload: Some(weaver_trace::serving::Payload::PlanInput(Box::new(
                weaver_trace::serving::payloads::PlanInputRecord {
                    message: serde_json::value::to_raw_value(&self.message)
                        .expect("Message serializes"),
                    context: crate::flow::mirror::plan_input(&self.context),
                    document: self.packet.input().2.into(),
                },
            ))),
        })
    }
    /// A full actual disposition set is necessary even when one block refuses.
    /// This check does not manufacture an effect or release the allocator.
    pub(in crate::flow) fn check_filing(
        &self,
        context: &RecordingContext,
        filing: &FilingCompletion,
        run: &str,
        scheme: &str,
        bound: AbsoluteBound,
    ) -> Result<Option<s::PlanUpdateRefused>, &'static str> {
        if !self.belongs_to(context)
            || !filing.belongs_to(context)
            || filing.wait_key() != PumpDeadline::Decision
            || filing.wait_bound() != bound
            || bound.expired(filing.recorded_ns())
            || !filing.receipts().is_complete()
            || filing.receipts().sources().count() != self.message.content.len()
            || filing.receipts().dispositions().count() != self.message.content.len()
        {
            return Err("operator filing lost its original context, bound or complete source set");
        }
        let mut rejected = Vec::new();
        let mut bad_step = false;
        let mut bad_category = false;
        let (steps, categories) = match &self.context.operation {
            s::PlanInputOperation::Create {
                steps, categories, ..
            } => (steps.as_slice(), categories),
            s::PlanInputOperation::Authorize { categories, .. } => (&[][..], categories),
            s::PlanInputOperation::Replace(value)
            | s::PlanInputOperation::AnswerBreak { replacement: value } => {
                (value.steps.as_slice(), &value.categories)
            }
        };
        for (index, (block, disposition)) in self
            .message
            .content
            .iter()
            .zip(filing.receipts().dispositions())
            .enumerate()
        {
            let ContentBlock::Text { text } = block else {
                return Err("operator input has a non-Text block");
            };
            let (file, is_rejected) = match disposition {
                SourceDisposition::Committed(proof) => {
                    let LedgerFile::Statement(file) = proof.file() else {
                        return Err("operator input has a pair receipt");
                    };
                    (file.as_ref(), false)
                }
                SourceDisposition::CorrectlyRejected(proof) => (proof.file(), true),
            };
            let metadata = file
                .context
                .as_ref()
                .map(|context| s::decode_strict::<s::PlanInputContext>(context.raw.get()));
            let valid_metadata = if index == 0 {
                matches!(metadata, Some(Ok(ref value)) if value == &self.context)
            } else {
                metadata.is_none()
            };
            if file.block_id.run != run
                || file.block_id.sequence.0 != filing.sequence().0
                || file.block_id.block.0 != index as u64
                || file.source.kind != "plan.event"
                || file.source.channel != "input"
                || file.source.origin != "operator"
                || file.source.turn.is_some()
                || file.source.causal_parent.is_some()
                || file.scheme_id != scheme
                || file.text != *text
                || file.effect.is_some()
                || !valid_metadata
            {
                return Err("operator filing differs from the retained original source");
            }
            if is_rejected {
                rejected.push((index as u64).into());
            } else if steps.iter().any(|step| step.block.0 == index as u64) {
                if file.bindings.len() != 1 {
                    bad_step = true;
                } else if categories
                    .iter()
                    .filter(|category| category.verbs.contains(&file.bindings[0].verb))
                    .count()
                    != 1
                {
                    bad_category = true;
                }
            }
        }
        Ok(if !rejected.is_empty() {
            Some(s::PlanUpdateRefused {
                case: s::PlanUpdateCase::NonSte100,
                detail: "operator plan contains rejected Text blocks".into(),
                rejected_blocks: rejected,
                read_code: None,
            })
        } else if bad_step {
            Some(s::PlanUpdateRefused {
                case: s::PlanUpdateCase::MalformedPlan,
                detail: "each plan instruction must contain exactly one statement".into(),
                rejected_blocks: vec![],
                read_code: None,
            })
        } else if bad_category {
            Some(s::PlanUpdateRefused {
                case: s::PlanUpdateCase::InvalidCategory,
                detail: "plan instruction does not select exactly one operator category".into(),
                rejected_blocks: vec![],
                read_code: None,
            })
        } else {
            None
        })
    }
    pub(in crate::flow) fn packet(&self) -> &RootPlanPacket {
        &self.packet
    }
    pub(in crate::flow) fn document(&self) -> &s::PlanUpdateDocument {
        &self.document
    }
    pub(in crate::flow) fn message(&self) -> &Message {
        &self.message
    }
    pub(in crate::flow) fn context(&self) -> &s::PlanInputContext {
        &self.context
    }
    pub(in crate::flow) fn source(&self) -> ManagementInput {
        self.source.clone()
    }
}

fn text(blocks: &mut Vec<ContentBlock>, value: &str) -> s::DecimalU64 {
    let index = (blocks.len() as u64).into();
    blocks.push(ContentBlock::Text { text: value.into() });
    index
}
fn steps(blocks: &mut Vec<ContentBlock>, values: &[s::StepDraft]) -> Vec<s::PlanInputStep> {
    values
        .iter()
        .map(|step| s::PlanInputStep {
            id: step.id.clone(),
            block: text(blocks, &step.instruction),
        })
        .collect()
}
fn replacement(
    blocks: &mut Vec<ContentBlock>,
    value: &s::PlanReplacement,
) -> s::PlanInputReplacement {
    s::PlanInputReplacement {
        task: value.task.as_ref().map(|task| text(blocks, task)),
        steps: steps(blocks, &value.steps),
        categories: value.categories.clone(),
        groups: value.groups.clone(),
        review: value.review.clone(),
        preserved_closures: value.preserved_closures.clone(),
    }
}
fn project(operation: &s::PlanOperation) -> (Message, s::PlanInputOperation) {
    let mut content = Vec::new();
    let operation = match operation {
        s::PlanOperation::Create {
            task,
            steps: values,
            categories,
            groups,
            review,
        } => s::PlanInputOperation::Create {
            task: text(&mut content, task),
            steps: steps(&mut content, values),
            categories: categories.clone(),
            groups: groups.clone(),
            review: review.clone(),
        },
        s::PlanOperation::Authorize {
            task,
            categories,
            groups,
            review,
        } => s::PlanInputOperation::Authorize {
            task: text(&mut content, task),
            categories: categories.clone(),
            groups: groups.clone(),
            review: review.clone(),
        },
        s::PlanOperation::Replace(value) => {
            s::PlanInputOperation::Replace(replacement(&mut content, value))
        }
        s::PlanOperation::AnswerBreak { replacement: value } => {
            s::PlanInputOperation::AnswerBreak {
                replacement: replacement(&mut content, value),
            }
        }
    };
    (
        Message {
            role: Role::User,
            content,
        },
        operation,
    )
}
