//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SchemeDeclaration {
    pub scheme_id: String,
    pub artifact_sha256: Digest,
    pub verbs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlowBacklog {
    pub max_pending_blocks: DecimalU64,
    pub max_pending_bytes: DecimalU64,
    pub lag_alarm_turns: DecimalU64,
    pub measurement_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartitionRule {
    pub id: String,
    pub kind: String,
    pub channels: Vec<String>,
    pub origins: Vec<String>,
    pub blocks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PersistenceConfig {
    pub save_wait: MeasuredWait,
    pub snapshot_wait: MeasuredWait,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadLimits {
    pub max_result_bytes: DecimalU64,
    pub measurement_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacementConfig {
    pub c_head: u64,
    pub c_tail: u64,
    pub measurement_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolPolicies {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sh: Option<ToolPolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolPolicy {
    pub clock_ms: u64,
    pub measurement_ref: String,
}
