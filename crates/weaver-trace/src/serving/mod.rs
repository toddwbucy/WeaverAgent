//! The inactive serving tap, under the merged dependable-flow amendment.
//!
//! This module supplies the complete projection boundary. It does not select the
//! serving path, submit trace events, transport copies, or authorize store writes.
//! Recorder activation, typed event authoring, capacity reservation and their
//! harness integration are separate work before the foundation cutover.
//!
//! A validated partition cannot be constructed without its load checks.
//!
//! ```compile_fail
//! use weaver_trace::serving::ValidatedPartition;
//! let partition = ValidatedPartition { rules: Vec::new() };
//! ```
//!
//! Vocabulary is serialize-only. The tap's private parser reads canonical bytes.
//!
//! ```compile_fail
//! fn reads<T: for<'a> serde::Deserialize<'a>>() {}
//! reads::<weaver_trace::serving::Kind>();
//! ```

mod partition;
mod record;
mod registry;
mod tee;

pub use partition::{
    Partition, PartitionError, PartitionErrorCode, PartitionRule, ValidatedPartition,
    validate_partition,
};
pub use registry::{
    Channel, ContentSchema, Kind, Origin, REGISTRY_ID, TurnRule, content_schema, turn_rule,
};
pub use tee::{
    AdmittedCopy, ContextBlock, PairCopy, PathPart, SelectedBlock, SourceBlock, SourceEvent,
    StatementCopy, TapFailure, TapFailureCode, TapOutcome, Tee,
};

#[cfg(test)]
mod tests;
