//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAct {
    Statement,
    ExternalTool { name: String },
    InwardRecall { verb: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanId {
    pub seed_id: String,
    pub local_id: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepId {
    pub seed_id: String,
    pub local_id: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanRef {
    pub plan_id: PlanId,
    pub revision: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewSetupRef {
    pub setup_id: String,
    pub sha256: Digest,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkRef {
    pub plan: PlanRef,
    pub step_id: StepId,
    pub span: SpanRef,
    pub through: SourcePosition,
    pub content_sha256: Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanBreakCase {
    NoLockedPlan,
    WrongCategory,
    UnknownTool,
    SelfEdit,
    InvalidTarget,
    WorkChanged,
    ReviewSetupChanged,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewFailure {
    MismatchedWork,
    MismatchedSetup,
    AssessmentFailed,
    Refused,
    Errored,
    Killed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Category {
    pub id: String,
    pub verbs: Vec<String>,
    pub acts: Vec<PlanAct>,
    pub review_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupDraft {
    pub id: String,
    pub step_indices: Vec<DecimalU64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanInputStep {
    pub id: Option<StepId>,
    pub block: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanInputReplacement {
    pub task: Option<DecimalU64>,
    pub steps: Vec<PlanInputStep>,
    pub categories: Vec<Category>,
    pub groups: Vec<GroupDraft>,
    pub review: ReviewSetupRef,
    pub preserved_closures: Vec<StepId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanInputOperation {
    Create {
        task: DecimalU64,
        steps: Vec<PlanInputStep>,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    },
    Authorize {
        task: DecimalU64,
        categories: Vec<Category>,
        groups: Vec<GroupDraft>,
        review: ReviewSetupRef,
    },
    Replace(PlanInputReplacement),
    AnswerBreak {
        replacement: PlanInputReplacement,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanInputContext {
    pub document_sha256: Digest,
    pub cause: crate::event::Cause,
    pub expected: Option<PlanRef>,
    pub operation: PlanInputOperation,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanLockSource {
    Operator {
        input: SourceBlockId,
    },
    Model {
        authorization: SourceBlockId,
        candidate: Vec<StatementRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanStepInit {
    pub step_id: StepId,
    pub instruction: StatementRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanEffect {
    Authorize {
        plan: PlanRef,
        source: SourceBlockId,
        task: TaskScopeRef,
        review: ReviewSetupRef,
    },
    Lock {
        plan: PlanRef,
        source: PlanLockSource,
        task: TaskScopeRef,
        steps: Vec<PlanStepInit>,
        review: ReviewSetupRef,
    },
    Replace {
        expected: PlanRef,
        next: PlanRef,
        source: SourceBlockId,
        task: TaskScopeRef,
        steps: Vec<PlanStepInit>,
        review: ReviewSetupRef,
    },
    Activate {
        expected: PlanRef,
        step_id: StepId,
        step_span: SpanRef,

        group_span: Option<SpanRef>,
        successor: StepQueryRef,
    },
    Close {
        expected: PlanRef,
        step_id: StepId,

        review_call: Option<CallId>,
        work: WorkRef,
        outcome_span: SpanRef,
    },
    Break {
        expected: PlanRef,

        step_id: Option<StepId>,
        case: PlanBreakCase,

        proposed_source: Option<SourceBlockId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanFlagSnapshot {
    pub plan: PlanRef,
    pub after: SourceBlockId,
    pub through: SourcePosition,
    pub total_flags: DecimalU64,
    pub matched_flags: DecimalU64,
    pub unmatched_flags: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewOutcome {
    Closed {},
    Failed { case: ReviewFailure },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanBreakReport {
    pub plan: Option<PlanRef>,
    pub step_id: Option<StepId>,
    pub proposed_source: Option<SourceBlockId>,
    pub case: PlanBreakCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFitStage {
    MandatoryProbe,
    PlaceBackstop,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFitCode {
    TailOverCapacity,
    GenerationRoom,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFlagObservation {
    Measured { snapshot: PlanFlagSnapshot },
    Unavailable { code: ReadRefusalCode },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanMeasureEvent {
    Flags {
        after: SourceBlockId,
        request_id: DecimalU64,
        observation: PlanFlagObservation,
    },
    NextReady {
        closed: SourceBlockId,
        previous_step: StepId,
        step_id: StepId,
        assembly_id: DecimalU64,
        began_ns: DecimalU64,
        ended_ns: DecimalU64,
        closure_to_next_ns: DecimalU64,
    },
    FitRefused {
        step_id: StepId,
        assembly_id: DecimalU64,
        stage: PlanFitStage,
        code: PlanFitCode,
        mandatory_fit_refusals: DecimalU64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanMeasurement {
    pub instrument_version: String,
    pub plan: PlanRef,
    pub event: PlanMeasureEvent,
}
