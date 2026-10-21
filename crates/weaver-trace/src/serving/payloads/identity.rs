//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceBlockId {
    pub run: String,
    pub sequence: DecimalU64,
    pub block: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourcePosition {
    pub block_id: SourceBlockId,
    pub turn: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Span {
    pub start: DecimalU64,
    pub end: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NounBinding {
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Binding {
    pub statement_span: Span,
    pub verb: String,
    pub verb_span: Span,
    pub condition: Option<String>,
    pub condition_span: Option<Span>,
    pub nouns: Vec<NounBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatementRef {
    pub block_id: SourceBlockId,
    pub binding: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpanRef {
    pub seed_id: String,
    pub local_id: DecimalU64,
    pub revision: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentRef {
    Seed {
        seed_id: String,
        ordinal: DecimalU64,
    },
    Event {
        block_id: SourceBlockId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskScopeRef {
    pub task: SpanRef,
    pub prompt: SpanRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepQueryRef {
    pub plan: PlanRef,
    pub previous_step: Option<StepId>,
    pub step_id: StepId,
    pub instruction: StatementRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpanText {
    pub source: ContentRef,
    pub start: DecimalU64,
    pub end: DecimalU64,
    pub step_query: Option<StepQueryRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    SystemPrompt,
    Task,
    TaskPrompt,
    Subtask,
    Step,
    StepOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Open,
    Closed,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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

        replacement: Option<SpanRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacementContext {
    pub label: String,
    pub instruction: Option<SourceBlockId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CutMethod {
    Flush,
    Elide,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitOccurrenceId {
    pub run: String,
    pub assembly_id: DecimalU64,
    pub unit_index: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarkOccurrenceId {
    pub run: String,
    pub ordinal: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CutReference {
    pub span: SpanRef,
    pub occurrence_id: MarkOccurrenceId,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentCursor {
    Head {},
    Message {
        message_id: DecimalU64,
        block_end: DecimalU64,

        byte_end: Option<DecimalU64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedStepQuery {
    pub reference: StepQueryRef,
    pub message_id: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkPositionSource {
    Boot {},
    Placed { unit: UnitOccurrenceId },
    Native { event: SourceBlockId },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarkPosition {
    pub id: MarkOccurrenceId,
    pub span: SpanRef,
    pub end: ArgumentCursor,
    pub source: MarkPositionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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
