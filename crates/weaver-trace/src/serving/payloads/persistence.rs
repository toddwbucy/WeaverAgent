//! Serialize-only trace mirrors. The owning state-management Spec defines the wire.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StartIdentity {
    pub seed_id: String,
    pub artifact_id: String,
    pub dump_sha256: Digest,
    pub config_sha256: Digest,
    pub parent_artifact_id: Option<String>,
    pub position: Option<SourcePosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceCause {
    Requested,
    GracefulUnload,
    ForcedUnload,
    Seed,
    Rebind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceCase {
    UnsupportedLayout,
    SeedExists,
    SeedRequiresUnloaded,
    SeedCertificateInvalid,
    SeedFailed,
    BindingMismatch,
    ConfigurationMismatch,
    SchemeMismatch,
    BrokenSavePoint,
    PublicationFailed,
    BootFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SaveWindow {
    pub began_ns: DecimalU64,
    pub deadline_ns: DecimalU64,
}
