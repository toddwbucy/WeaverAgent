//! Stored span identities and mechanical effects, from Spec 06 and TYPES-READ.
//!
//! These values describe evidence. They cannot construct a harness placement,
//! cut, or closure grant.

use serde::{Deserialize, Serialize};

use super::{DecimalU64, PlanRef, SourceBlockId, StatementRef, StepId};

/// Address of immutable held content in one seeded chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanRef {
    pub seed_id: String,
    pub local_id: DecimalU64,
    pub revision: DecimalU64,
}

/// The content source, independent of its position in any argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentRef {
    Seed {
        seed_id: String,
        ordinal: DecimalU64,
    },
    Event {
        block_id: SourceBlockId,
    },
}

/// A task and the immutable prompt child that introduced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskScopeRef {
    pub task: SpanRef,
    pub prompt: SpanRef,
}

/// A read-produced successor address. This is not an activation grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepQueryRef {
    pub plan: PlanRef,
    #[serde(deserialize_with = "super::required_option")]
    pub previous_step: Option<StepId>,
    pub step_id: StepId,
    pub instruction: StatementRef,
}

/// Exact UTF-8 interval of a held statement and optional step-query context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanText {
    pub source: ContentRef,
    pub start: DecimalU64,
    pub end: DecimalU64,
    #[serde(deserialize_with = "super::required_option")]
    pub step_query: Option<StepQueryRef>,
}

/// The six stored scope kinds in the initial catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    SystemPrompt,
    Task,
    TaskPrompt,
    Subtask,
    Step,
    StepOutcome,
}

/// Administrative retirement never means reviewed closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Open,
    Closed,
    Retired,
}

/// Allocation and boundary facts returned by the marks selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanInfo {
    pub span: SpanRef,
    #[serde(deserialize_with = "super::required_option")]
    pub parent: Option<SpanRef>,
    pub kind: SpanKind,
    pub status: SpanStatus,
    #[serde(deserialize_with = "super::required_option")]
    pub opened_source: Option<SourceBlockId>,
    #[serde(deserialize_with = "super::required_option")]
    pub ended_source: Option<SourceBlockId>,
    #[serde(deserialize_with = "super::required_option")]
    pub replacement: Option<SpanRef>,
}

/// Atomic mark operations. Only BeginTask is directly licensed for filing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MarkEffect {
    BeginTask {
        task: SpanRef,
        prompt: SpanRef,
        source: ContentRef,
    },
    Open {
        span: SpanRef,
        parent: SpanRef,
        kind: SpanKind,
        prefix: Vec<SpanText>,
    },
    Close {
        span: SpanRef,
    },
    Retire {
        span: SpanRef,
        #[serde(deserialize_with = "super::required_option")]
        replacement: Option<SpanRef>,
    },
}

/// Caller attribution for placement, never rendered as content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementContext {
    pub label: String,
    #[serde(deserialize_with = "super::required_option")]
    pub instruction: Option<SourceBlockId>,
}

/// The two buffer cut mechanics. Neither is a strategy default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutMethod {
    Flush,
    Elide,
}

/// One placement occurrence, distinct from stored identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitOccurrenceId {
    pub run: String,
    pub assembly_id: DecimalU64,
    pub unit_index: DecimalU64,
}

/// One current argument occurrence of a stored end mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkOccurrenceId {
    pub run: String,
    pub ordinal: DecimalU64,
}

/// A requested cut address, never a token count or a cut permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutReference {
    pub span: SpanRef,
    pub occurrence_id: MarkOccurrenceId,
}

/// Canonical message endpoint. The SPU owns conversion into token facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ArgumentCursor {
    Head {},
    Message {
        message_id: DecimalU64,
        block_end: DecimalU64,
        #[serde(deserialize_with = "super::required_option")]
        byte_end: Option<DecimalU64>,
    },
}

/// Provenance of a step instruction placed at an actual message occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedStepQuery {
    pub reference: StepQueryRef,
    pub message_id: DecimalU64,
}

/// The source of one observed mark occurrence in the current argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MarkPositionSource {
    Boot {},
    Placed { unit: UnitOccurrenceId },
    Native { event: SourceBlockId },
}

/// The journal's attributed end mark, still independent of token position N.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkPosition {
    pub id: MarkOccurrenceId,
    pub span: SpanRef,
    pub end: ArgumentCursor,
    pub source: MarkPositionSource,
}

/// Refusal vocabulary for the placement port, not a component halt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementCode {
    InvalidSpan,
    InvalidPair,
    InvalidCut,
    HeadOverCapacity,
    TailOverCapacity,
    GenerationRoom,
    StalePosition,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_null_members_are_required_and_scope_operations_are_closed() {
        let span = r#"{"seed_id":"seed","local_id":"0","revision":"1"}"#;
        let valid = format!(r#"{{"retire":{{"span":{span},"replacement":null}}}}"#);
        assert!(serde_json::from_str::<MarkEffect>(&valid).is_ok());
        let missing = format!(r#"{{"retire":{{"span":{span}}}}}"#);
        assert!(serde_json::from_str::<MarkEffect>(&missing).is_err());
        let extra = format!(r#"{{"close":{{"span":{span},"members":[]}}}}"#);
        assert!(serde_json::from_str::<MarkEffect>(&extra).is_err());
        assert!(serde_json::from_str::<ArgumentCursor>(r#"{"head":{"N":0}}"#).is_err());
        assert!(
            serde_json::from_str::<ArgumentCursor>(
                r#"{"message":{"message_id":"1","block_end":"1"}}"#
            )
            .is_err()
        );
    }
}
