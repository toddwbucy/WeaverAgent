//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ste100Instruction {
    pub model_binding: ModelBinding,
    pub model_artifact_sha256: Digest,
    pub instances: u32,
    pub max_wire_bytes: DecimalU64,
    pub measurement_ref: String,
    pub repair_cap: RepairCap,
    pub decision_wait: MeasuredWait,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepairCap {
    pub attempts: u32,
    pub measurement_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ste100Target {
    Source(SourceBlockId),
    Input {
        input_id: DecimalU64,
        block: DecimalU64,
    },
    Template {
        template_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ste100Reply {
    pub request_id: DecimalU64,
    pub target: Ste100Target,
    pub scheme_id: String,
    pub result: Ste100Result,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ste100Result {
    Accepted {
        bindings: Vec<Binding>,
    },
    NotSte100 {
        code: Ste100DefectCode,
        detail: String,
        spans: Vec<Span>,
    },
    ComponentFailed {
        code: Ste100ComponentCode,
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ste100DefectCode {
    NonSte100,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ste100ComponentCode {
    Crashed,
    NoResult,
    InvalidResult,
    TransportFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TemplateSpec {
    pub template_id: String,
    pub kind: String,
    pub parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplatePart {
    Literal(String),
    Slot { name: String, class: SlotClass },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotClass {
    TechnicalName,
    StepId,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopInstructionMeta {
    Restatement {
        source: SourceBlockId,
        attempt: DecimalU64,
    },
    Retry {
        retry_of: CallId,
        attempt: DecimalU64,
    },
    PlanProposal {
        plan: PlanRef,
        authorization: SourceBlockId,
    },
    Step {
        plan: PlanRef,
        step_id: StepId,
        instruction: StatementRef,
        span: SpanRef,
    },
    Review {
        setup: ReviewSetupRef,
        work: WorkRef,
    },
    Compact {
        reservation: SpanRef,
        through: SourcePosition,
    },
    WindDown {},
}
