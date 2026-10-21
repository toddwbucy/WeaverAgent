//! The additive serving trace primitive, under the dependable-flow amendment.
//!
//! Typed authoring, reserved writer capacity and the terminal tap share one
//! canonical record. The existing selected recorder remains in place until the
//! foundation cutover. This module neither transports copies nor writes state.
//! Serving recall authoring awaits its owning payload contract.
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

mod event;
mod partition;
pub mod payloads;
mod record;
mod recorder;
mod registry;
mod structure;
mod tee;
mod writer;

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

pub use event::{Envelope, Event, Payload};
pub use recorder::{
    AwaitingLoad, Closed, Loaded, PreflightAttempt, PreflightFailure, PreflightRecorder,
    PreflightTerminal, PreflightTerminalFailure, Recorder, Refused, Submission, SubmitAttempt,
};
pub use structure::{Record, WorkingStructure};
pub use writer::{CapacityWake, DrainStatus, ORDINARY_CAPACITY, QUEUE_DEPTH, TERMINAL_RESERVE};
