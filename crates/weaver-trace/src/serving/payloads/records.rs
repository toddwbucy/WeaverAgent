//! Payload records that compose the closed shared mirrors at the trace boundary.
use super::*;
use crate::event::{Cause, GrantSurface, Lineage, LoopIdentity, Release, Reset, StoreIdentity};

pub type StateStartRecord = StartIdentity;
pub type Ste100ResultRecord = Ste100Reply;

#[derive(Debug, Clone, Serialize)]
pub struct LoopInstructionRecord {
    pub template_id: String,
    pub message: Box<RawValue>,
    pub instruction: LoopInstructionMeta,
}
#[derive(Debug, Clone, Serialize)]
pub struct SortDecisionRecord {
    pub template_id: String,
    pub message: Box<RawValue>,
    pub decision: SortDecision,
}
#[derive(Debug, Clone, Serialize)]
pub struct PlanInputRecord {
    pub message: Box<RawValue>,
    pub context: PlanInputContext,
    pub document: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct PlanEffectRecord {
    pub message: Box<RawValue>,
    pub context: PlanEffect,
    pub template_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct StepOutcomeRecord {
    pub message: Box<RawValue>,
    pub template_id: String,
    pub context: Option<PlanEffect>,
    pub plan: PlanRef,
    pub step_id: StepId,
    pub review_call: Option<CallId>,
    pub work: WorkRef,
    pub outcome: ReviewOutcome,
}
#[derive(Debug, Clone, Serialize)]
pub struct StateMarkRecord {
    pub template_id: String,
    pub message: Box<RawValue>,
    pub context: MarkEffect,
}
#[derive(Debug, Clone, Serialize)]
pub struct InputOfferedRecord {
    pub input_id: DecimalU64,
    pub octets: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RejectedReturn {
    pub call_id: Option<CallId>,
    pub execution: ExchangeId,
    pub reason: String,
    pub raw_envelope: Box<RawValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedSource {
    pub material: MaterialRef,
    pub message_id: DecimalU64,
    pub block: DecimalU64,
    pub start: Option<DecimalU64>,
    pub end: Option<DecimalU64>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitMapEntry {
    pub unit_id: UnitId,
    pub occurrence_id: UnitOccurrenceId,
    pub message_start: DecimalU64,
    pub message_end: DecimalU64,
    pub spans: Vec<SpanRef>,
    pub source_map: Vec<PlacedSource>,
    pub step_queries: Vec<PlacedStepQuery>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementPhase {
    Boot,
    Cut,
    Append,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacementRecord {
    pub assembly_id: DecimalU64,
    pub phase: PlacementPhase,
    pub context: Option<PlacementContext>,
    pub cut: Option<CutReference>,
    pub method: Option<CutMethod>,
    #[serde(rename = "N")]
    pub n: u64,
    pub prefix_tokens: u64,
    pub resident_before: u64,
    pub input_tokens: u64,
    pub capacity: u64,
    pub generation_limit: u64,
    pub terminal_tokens: u64,
    pub unit_map: Vec<UnitMapEntry>,
    pub message_ids: Vec<DecimalU64>,
    pub assistant_message_id: Option<DecimalU64>,
    pub recomputed_messages: DecimalU64,
    pub wait_ns: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TemplateInventory {
    pub scheme_id: String,
    pub inventory_sha256: Digest,
    pub checker_artifact_sha256: Digest,
    pub templates: Vec<TemplateSpec>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlowDeclarationRecord {
    pub registry_id: String,
    pub version: u32,
    pub partition: Vec<PartitionRule>,
    pub gate_wait: MeasuredWait,
    pub cut_wait: MeasuredWait,
    pub observation_interval: MeasuredInterval,
    pub backlog: FlowBacklog,
    pub scheme: SchemeDeclaration,
    pub persistence: PersistenceConfig,
    pub reads: ReadLimits,
    pub ste100: Ste100Instruction,
    pub tools: ToolPolicies,
    pub placement: PlacementConfig,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ClockSource {
    #[serde(rename = "CLOCK_MONOTONIC")]
    ClockMonotonic,
}
/// The former load elections with the tee election removed and explicit proof inputs.
#[derive(Debug, Clone, Serialize)]
pub struct LoadRecord {
    pub residual_readout: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<u32>,
    pub surprisal: bool,
    pub state_member: bool,
    pub state_store: StoreIdentity,
    pub declaration: String,
    pub composer: LoopIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Box<Lineage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<Reset>,
    pub stack: std::collections::BTreeMap<String, String>,
    pub boundary: String,
    pub cause: Cause,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_path: Option<String>,
    pub state_flow: FlowDeclarationRecord,
    pub state_start: StateStartRecord,
    pub clock_source: ClockSource,
    pub origin_ns: DecimalU64,
    pub state_member_observation: MemberObservationIdentity,
    pub template_inventory: TemplateInventory,
    /// Exact scheme artifact text. JSON string escaping preserves its whitespace
    /// without putting physical newlines inside the canonical trace record.
    pub ste100_scheme: String,
    pub ste100_binary_sha256: Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowPhase {
    Copied,
    ClassificationStarted,
    Accepted,
    CorrectlyRejected,
    FileStarted,
    Filed,
    WaitFinished,
    RepairRequested,
    RepairExhausted,
    Sampled,
    BacklogWait,
    BacklogResumed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedStage {
    Gate,
    Cut,
    Decision,
    Control,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyContentType {
    Statement,
    Pair,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopySourceEvent {
    pub run: String,
    pub sequence: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopyAtHalt {
    pub source_event: CopySourceEvent,
    pub partition_rule_id: String,
    pub content_type: CopyContentType,
    pub sources: Vec<SourceBlockId>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlowObservation {
    pub phase: FlowPhase,
    pub source_id: Option<SourceBlockId>,
    pub input_id: Option<DecimalU64>,
    pub request_id: Option<DecimalU64>,
    pub pending_count: DecimalU64,
    pub oldest_age_ns: DecimalU64,
    pub elapsed_ns: Option<DecimalU64>,
    pub blocked_stage: Option<BlockedStage>,
    pub sample_start_ns: DecimalU64,
    pub sampled_ns: DecimalU64,
    pub pending_bytes: DecimalU64,
    pub filed_count: DecimalU64,
    pub filed_bytes: DecimalU64,
    pub rejected_count: DecimalU64,
    pub input_pending: DecimalU64,
    pub input_started: DecimalU64,
    pub input_completed: DecimalU64,
    pub retained_blocks: DecimalU64,
    pub retained_bytes: DecimalU64,
    pub pending_turns: DecimalU64,
    pub backlog_waiting: bool,
    pub lag_alarm: bool,
    pub waiting_copy: Option<CopyAtHalt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_corrections: Option<DecimalU64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_cap: Option<DecimalU64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    ComponentFailure,
}
#[derive(Debug, Clone, Serialize)]
pub struct StateFlowFailure {
    pub fault_sequence: Option<DecimalU64>,
    /// HARNESS-HALT-ACCOUNT is deliberately opaque to the recorder.
    pub account: Box<RawValue>,
}
#[derive(Debug, Clone, Serialize)]
pub struct UnloadClose {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_surface: Option<GrantSurface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<Cause>,
    pub forced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forced_by: Option<Cause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<Release>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<EndReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_flow_failure: Option<StateFlowFailure>,
}

/// Phase-specific members cannot accidentally leak artifact details into a request
/// or claim that a refusal published a state image.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum SavePointRecord {
    Requested {
        capture_id: DecimalU64,
        window: SaveWindow,
        tail_window: Option<SaveWindow>,
        cause: PersistenceCause,
    },
    Confirmed {
        capture_id: DecimalU64,
        window: SaveWindow,
        tail_window: Option<SaveWindow>,
        cause: PersistenceCause,
        artifact_id: String,
        dump_sha256: Digest,
        config_sha256: Digest,
        location: String,
        position: Option<SourcePosition>,
    },
    Published {
        capture_id: DecimalU64,
        window: SaveWindow,
        tail_window: Option<SaveWindow>,
        cause: PersistenceCause,
        artifact_id: String,
        dump_sha256: Digest,
        config_sha256: Digest,
        location: String,
        position: Option<SourcePosition>,
    },
    Refused {
        capture_id: DecimalU64,
        window: SaveWindow,
        tail_window: Option<SaveWindow>,
        cause: PersistenceCause,
        refusal_code: String,
    },
}
