//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitId {
    Block { source: ContentRef },
    Statement { statement: StatementRef },
    Pair { call_id: CallId },
    Result { call_id: CallId },
    Span { span: SpanRef },
    Instruction { source: ContentRef },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialRef {
    Text { source: ContentRef, span: Span },
    Request { call_id: CallId },
    Returned { call_id: CallId },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SortCriterion {
    pub order: CandidateOrder,
    pub injection_share: Rational,
    pub external_drop: ExternalDrop,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateOrder {
    Provided {},
    Recency {},
    CurrentStepThenRecency { condition: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalDrop {
    Refuse,
    WholeOverLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundCriterion {
    Identity,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectionRead {
    pub selector: ReadSelector,
    pub snapshot_id: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectionDrop {
    pub unit: UnitId,
    pub reason: DropReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    AlreadyCovered,
    InjectionBudget,
    TailOverCapacity,
    GenerationRoom,
    InvalidCandidate,
    BoundaryUnavailable,
    NotEvaluated,
    WholeOverLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionCase {
    IncompleteCandidates,
    InvalidCriterion,
    MandatoryOverCapacity,
    BoundaryUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortAct {
    Tool {
        call_id: CallId,
        statement: StatementRef,
    },
    Recall {
        call_id: CallId,
        statement: StatementRef,
    },
    Answer {
        source: SourceBlockId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDecision {
    Serve {
        assembly_id: DecimalU64,
        criterion: SortCriterion,
        selected_refs: Vec<UnitId>,
        dropped_refs: Vec<SelectionDrop>,
        reads: Vec<SelectionRead>,

        refusal: Option<SelectionCase>,
    },
    Outbound {
        source: SourcePosition,
        criterion: OutboundCriterion,
        acts: Vec<SortAct>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SortMeasurement {
    pub assembly_id: DecimalU64,
    pub candidate_count: DecimalU64,
    pub selected_count: DecimalU64,
    pub mandatory_tokens: Option<u64>,
    pub optional_marginal_tokens: Option<u64>,
    pub budget: u64,
    pub fit_attempts: DecimalU64,
    pub wait_ns: DecimalU64,
    pub refusal: Option<SelectionCase>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadSelector {
    History {
        noun: String,

        verb: Option<String>,

        condition: Option<String>,
    },
    Subjects {
        nouns: Vec<String>,
    },
    Statement {
        statement: StatementRef,
    },
    Condition {
        condition: String,
    },
    Span {
        seed_id: String,
        local_id: DecimalU64,
        revision: DecimalU64,
    },
    SystemPrompt {},
    Conversation {
        last_turns: DecimalU64,
    },
    OpenPairs {},
    Marks {},
    SpanPreview {
        span: SpanRef,
        through: SourcePosition,
    },
    PlanCurrent {},
    Plan {
        plan: PlanRef,
    },
    StepSuccessor {
        plan: PlanRef,

        previous_step: Option<StepId>,
    },
    PlanFlags {
        plan: PlanRef,
        after: SourceBlockId,
    },
    Observations {
        run: String,
        after: DecimalU64,

        through: Option<DecimalU64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadRefusalCode {
    UnsupportedOperation,
    InvalidSelector,
    WrongSeed,
    NotFound,
    UnknownSpan,
    SpanOpen,
    SpanPending,
    PartialPair,
    CounterExhausted,
    ReadBusy,
    UnknownSnapshot,
    WrongOffset,
    ResultTooLarge,
    ResultUnavailable,
    AnswerTooLarge,
}
