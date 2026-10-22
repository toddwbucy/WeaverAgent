//! Closed recall provenance. Admission checks structure, never store contents.
use super::*;
use crate::serving::record::Object;
use std::collections::BTreeSet;

/// The harness supplies a complete checked read or the first seeded prompt read.
/// This record grants no material, placement or boot authority.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallRecord {
    Read {
        selector: ReadSelector,
        snapshot_id: DecimalU64,
        sources: Vec<RecallSource>,
        spans: Vec<RecallSpan>,
    },
    Boot {
        selector: ReadSelector,
        snapshot_id: DecimalU64,
        sources: Vec<RecallSource>,
        spans: Vec<RecallSpan>,
        message: Box<RawValue>,
        end_mark: RecallEndMark,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecallSource {
    pub source: ContentRef,
    pub binding_ordinals: Vec<DecimalU64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecallSpan {
    pub source: ContentRef,
    pub selected_span: RecallSelection,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecallSelection {
    pub span_ref: SpanRef,
    pub start: DecimalU64,
    pub end: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecallEndMark {
    pub span: SpanRef,
    pub end: DecimalU64,
}

impl RecallRecord {
    pub(in crate::serving) fn valid(&self) -> bool {
        let (selector, snapshot_id, sources, spans) = match self {
            Self::Read {
                selector,
                snapshot_id,
                sources,
                spans,
            }
            | Self::Boot {
                selector,
                snapshot_id,
                sources,
                spans,
                ..
            } => (selector, snapshot_id, sources, spans),
        };
        if snapshot_id.0 == 0 || !valid_selector(selector) {
            return false;
        }
        let mut cited = BTreeSet::new();
        for source in sources {
            if !valid_content(&source.source)
                || !cited.insert(content_key(&source.source))
                || source
                    .binding_ordinals
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
            {
                return false;
            }
        }
        for span in spans {
            if !cited.contains(&content_key(&span.source))
                || !valid_span(&span.selected_span.span_ref)
                || span.selected_span.start >= span.selected_span.end
            {
                return false;
            }
        }
        match self {
            Self::Read { .. } => true,
            Self::Boot {
                message, end_mark, ..
            } => valid_boot(selector, sources, spans, message, end_mark),
        }
    }
}

fn content_key(source: &ContentRef) -> (u8, &str, u64, u64) {
    match source {
        ContentRef::Seed { seed_id, ordinal } => (0, seed_id, ordinal.0, 0),
        ContentRef::Event { block_id } => (1, &block_id.run, block_id.sequence.0, block_id.block.0),
    }
}

fn valid_content(source: &ContentRef) -> bool {
    match source {
        ContentRef::Seed { seed_id, .. } => valid_seed(seed_id),
        // Run and turn identities are opaque strings under the base trace Spec.
        ContentRef::Event { .. } => true,
    }
}

fn valid_seed(seed: &str) -> bool {
    seed.len() == 32
        && seed
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_span(span: &SpanRef) -> bool {
    valid_seed(&span.seed_id) && span.revision.0 == 1
}

fn valid_plan(plan: &PlanRef) -> bool {
    valid_seed(&plan.plan_id.seed_id) && plan.plan_id.local_id.0 > 0 && plan.revision.0 > 0
}

fn valid_selector(selector: &ReadSelector) -> bool {
    match selector {
        ReadSelector::Subjects { nouns } => {
            !nouns.is_empty()
                && nouns.iter().all(|noun| !noun.is_empty())
                && nouns
                    .windows(2)
                    .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
        }
        ReadSelector::Span {
            seed_id, revision, ..
        } => valid_seed(seed_id) && revision.0 == 1,
        ReadSelector::Conversation { last_turns } => last_turns.0 > 0,
        ReadSelector::SpanPreview { span, .. } => valid_span(span),
        ReadSelector::Plan { plan } | ReadSelector::PlanFlags { plan, .. } => valid_plan(plan),
        ReadSelector::StepSuccessor {
            plan,
            previous_step,
        } => {
            valid_plan(plan)
                && previous_step
                    .as_ref()
                    .is_none_or(|step| valid_seed(&step.seed_id) && step.local_id.0 > 0)
        }
        ReadSelector::Observations { after, through, .. } => {
            through.is_none_or(|through| after <= &through)
        }
        ReadSelector::History { .. }
        | ReadSelector::Statement { .. }
        | ReadSelector::Condition { .. }
        | ReadSelector::SystemPrompt {}
        | ReadSelector::OpenPairs {}
        | ReadSelector::Marks {}
        | ReadSelector::PlanCurrent {} => true,
    }
}

fn valid_boot(
    selector: &ReadSelector,
    sources: &[RecallSource],
    spans: &[RecallSpan],
    message: &RawValue,
    end_mark: &RecallEndMark,
) -> bool {
    let ([source], [span]) = (sources, spans) else {
        return false;
    };
    let ContentRef::Seed { seed_id, ordinal } = &source.source else {
        return false;
    };
    let selection = &span.selected_span;
    matches!(selector, ReadSelector::SystemPrompt {})
        && ordinal.0 == 0
        && span.source == source.source
        && selection.span_ref.seed_id == *seed_id
        && selection.span_ref.local_id.0 == 0
        && selection.start.0 == 0
        && end_mark.span == selection.span_ref
        && end_mark.end == selection.end
        && canonical_prompt_bytes(message) == Some(selection.end.0)
}

/// Decode only the closed boot Message, then compare its typed canonical bytes.
/// Unicode escapes and whitespace cannot alter the stored UTF-8 byte boundary.
fn canonical_prompt_bytes(raw: &RawValue) -> Option<u64> {
    let message = Object::parse(raw.get()).ok()?;
    if !message.exact(&["role", "content"]) || message.string("role").ok()? != "system" {
        return None;
    }
    let blocks: Vec<&RawValue> = serde_json::from_str(message.get("content")?.get()).ok()?;
    let [block] = blocks.as_slice() else {
        return None;
    };
    let block = Object::parse(block.get()).ok()?;
    if !block.exact(&["type", "text"]) || block.string("type").ok()? != "text" {
        return None;
    }
    let text = block.string("text").ok()?;
    #[derive(Serialize)]
    struct Message<'a> {
        role: &'a str,
        content: [Text<'a>; 1],
    }
    #[derive(Serialize)]
    struct Text<'a> {
        #[serde(rename = "type")]
        kind: &'a str,
        text: &'a str,
    }
    let canonical = serde_json::to_string(&Message {
        role: "system",
        content: [Text {
            kind: "text",
            text: &text,
        }],
    })
    .ok()?;
    if canonical != raw.get() {
        return None;
    }
    u64::try_from(text.len()).ok()
}
