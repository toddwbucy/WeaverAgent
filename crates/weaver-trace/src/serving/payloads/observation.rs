//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateOperationObservation {
    pub operation_id: DecimalU64,
    pub parent_id: Option<DecimalU64>,
    pub request_id: DecimalU64,
    pub operation: StateOperation,
    pub began_ns: DecimalU64,
    pub ended_ns: DecimalU64,
    pub phases: Vec<OperationPhase>,
    pub resource: OperationResource,
    pub disk: OperationDisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StateOperation {
    Open,
    File,
    FilePair,
    Read,
    Next,
    Release,
    Shape,
    Held,
    Position,
    Grants,
    Snapshot,
    Acknowledge,
    Abort,
    ReleaseCapture,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationPhase {
    pub name: PhaseName,
    pub began_ns: DecimalU64,
    pub ended_ns: DecimalU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseName {
    Validate,
    Query,
    Transaction,
    Capture,
    Filesystem,
    Encode,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationResource {
    pub cpu_ns: Option<DecimalU64>,
    pub copied_bytes: Option<DecimalU64>,
    pub held_bytes: Option<DecimalU64>,
    pub unavailable: Vec<ResourceUnavailable>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResourceUnavailable {
    pub field: ResourceField,
    pub reason: ObservationUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceField {
    CpuNs,
    CopiedBytes,
    HeldBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationUnavailable {
    Unsupported,
    Denied,
    Disabled,
    ReadFailed,
    CounterReset,
    Overflow,
    Uninstrumented,
    NotSampled,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationDisk {
    pub enabled: Option<bool>,
    pub before: Option<DiskSample>,
    pub after: Option<DiskSample>,
    pub unavailable_reason: Option<ObservationUnavailable>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiskSample {
    pub major_faults: DecimalU64,
    pub read_bytes: DecimalU64,
    pub write_bytes: DecimalU64,
    pub blkio_delay_ticks: DecimalU64,
    pub ticks_per_second: DecimalU64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemberObservationIdentity {
    pub instrument_version: String,
    pub pid: u32,
    pub start_ticks: Option<DecimalU64>,
    pub ticks_per_second: Option<DecimalU64>,
    pub unavailable_reason: Option<ObservationUnavailable>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredOperationObservation {
    pub request_source: Option<SourceBlockId>,
    pub observation: StateOperationObservation,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateStoreMeasurement {
    pub observation: StateOperationObservation,
    pub request_source: Option<SourceBlockId>,
    pub harness_queued_ns: DecimalU64,
    pub harness_sent_ns: DecimalU64,
    pub harness_received_ns: DecimalU64,
}
