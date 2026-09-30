//! conforms: trace-structure-mutators-pinned-by-doctest
//! conforms: trace-path-shapes-pinned-by-doctest
//! conforms: trace-payload-serialize-only
//! conforms: trace-kind-enum-exhaustive
//! conforms: trace-failure-enum-exhaustive
//! conforms: trace-structure-no-mutation-surface
//! conforms: trace-receive-shape-pinned-by-doctest
//!
//! The recorder: canonical NDJSON rendering, the append-only working
//! structure, and the stream writer, per `weaver-trace-Spec`. The harness
//! authors every event and this crate records every event it admits; neither
//! party holds a recording level and neither drops an event for being one.
//!
//! The doctests below are the compile-time instruments of `weaver-trace-Spec`
//! section 10.
//!
//! The receive shape is read by a compiling call
//! (`trace-receive-shape-pinned-by-doctest`): an `OwnedFd`, a `RunRef`,
//! and a `SessionRef` binding a `Recorder`, so an argument added to the one
//! constructor stops the build loudly.
//!
//! ```
//! use std::os::fd::OwnedFd;
//! use weaver_trace::{Failure, Recorder, RunRef, SessionRef};
//! fn shape(sink: OwnedFd, run: RunRef, session: SessionRef) {
//!     let _recorder: Result<Recorder, Failure> = Recorder::receive(sink, run, session);
//! }
//! ```
//!
//! The kind enum is exhaustive (`trace-kind-enum-exhaustive`): a consumer's
//! match with no wildcard compiles, and a further kind breaks it loudly.
//!
//! ```
//! use weaver_trace::Kind;
//! fn wire_name(kind: Kind) -> &'static str {
//!     match kind {
//!         Kind::Load => "load",
//!         Kind::Unload => "unload",
//!         Kind::SessionClosed => "session.closed",
//!         Kind::TurnStarted => "turn.started",
//!         Kind::TurnClosed => "turn.closed",
//!         Kind::MessageSystem => "message.system",
//!         Kind::MessageUser => "message.user",
//!         Kind::MessageAssistant => "message.assistant",
//!         Kind::MessageToolResult => "message.tool_result",
//!         Kind::ToolCallStarted => "tool.call.started",
//!         Kind::ToolCallCompleted => "tool.call.completed",
//!         Kind::Fault => "fault",
//!         Kind::Flush => "flush",
//!         Kind::Elision => "elision",
//!         Kind::Refusal => "refusal",
//!         Kind::ModelRequest => "model.request",
//!         Kind::ModelOutput => "model.output",
//!         Kind::ModelMeasurement => "model.measurement",
//!         Kind::ModelField => "model.field",
//!         Kind::ClassifyRequest => "classify.request",
//!         Kind::ClassifyOutput => "classify.output",
//!         Kind::Recall => "recall",
//!         Kind::MessageRestored => "message.restored",
//!         Kind::Score => "score",
//!     }
//! }
//! ```
//!
//! The failure enum is exhaustive (`trace-failure-enum-exhaustive`), so a new
//! case reaches every caller.
//!
//! ```
//! use weaver_trace::Failure;
//! fn name(f: &Failure) -> &'static str {
//!     match f {
//!         Failure::RefusedOnSubmit { .. } => "refused",
//!         Failure::CommitFailed { .. } => "commit_failed",
//!         Failure::AppendFailed { .. } => "append_failed",
//!         Failure::WriteTargetUnusable { .. } => "write_target_unusable",
//!     }
//! }
//! ```
//!
//! The working structure exposes no mutation surface
//! (`trace-structure-no-mutation-surface`): every public accessor yields a
//! shared reference, which this doctest reads over the signatures.
//!
//! ```
//! use weaver_trace::{Kind, Record, WorkingStructure};
//! fn reads_only(structure: &WorkingStructure) -> usize {
//!     let _all: Vec<&Record> = structure.iter().collect();
//!     let _loads: Vec<&Record> = structure.by_kind(Kind::Load).collect();
//!     structure.len()
//! }
//! ```
//!
//! The named mutators fail to compile
//! (`trace-structure-mutators-pinned-by-doctest`), a finite set reaching
//! `remove`, `truncate`, and `get_mut` and not the open set of all the
//! mutators someone could write:
//!
//! ```compile_fail
//! fn f(s: &mut weaver_trace::WorkingStructure) { s.remove(0); }
//! ```
//!
//! ```compile_fail
//! fn f(s: &mut weaver_trace::WorkingStructure) { s.truncate(0); }
//! ```
//!
//! ```compile_fail
//! fn f(s: &mut weaver_trace::WorkingStructure) { let _ = s.get_mut(0); }
//! ```
//!
//! No path-taking write surface (`trace-path-shapes-pinned-by-doctest`): the
//! three named shapes each fail to compile, and the attack they defeat is a
//! tool that has read `/proc/self/fd` and wants a second handle, which fails
//! because there is no call that takes what it learned.
//!
//! ```compile_fail
//! use weaver_trace::{Recorder, RunRef, SessionRef};
//! let _ = Recorder::receive("/var/lib/weaver/trace.ndjson", RunRef("r-1".into()),
//!     SessionRef("s".into()));
//! ```
//!
//! ```compile_fail
//! use weaver_trace::{Recorder, RunRef, SessionRef};
//! let path = String::from("/var/lib/weaver/trace.ndjson");
//! let _ = Recorder::receive(path, RunRef("r-1".into()), SessionRef("s".into()));
//! ```
//!
//! ```compile_fail
//! use std::path::PathBuf;
//! use weaver_trace::{Recorder, RunRef, SessionRef};
//! let _ = Recorder::receive(PathBuf::from("/tmp/t"), RunRef("r-1".into()),
//!     SessionRef("s".into()));
//! ```
//!
//! `Payload` derives no `Deserialize` (`trace-payload-serialize-only`): this
//! crate never reads an event back, and the asymmetry is a compile property.
//!
//! ```compile_fail
//! fn de<T: for<'de> serde::Deserialize<'de>>() {}
//! de::<weaver_trace::Payload>();
//! ```

mod canonical;
mod event;
mod failure;
mod structure;
mod tee;
mod writer;

pub use canonical::{MonotonicNs, Sequence};
pub use event::{
    Candidate, ClassifyAsk, ClassifyScored, Elections, ElisionSpan, Envelope, Event, Finish,
    FlushCounts, GrantSurface, Kind, Line, Lineage, LoopIdentity, ModelField, ModelOutput, Payload,
    RecallAccount, RecallAsk, RecallVerb, RecalledIdentity, RunRef, ScoreRatio, SessionRef,
    StopReason, StoreIdentity, Subsystem, TaskScore, TurnClose, TurnRef, UnloadClose, raw_payload,
};
pub use failure::{Failure, FieldName, SubmitRefusal, WriteError};
pub use structure::{Record, WorkingStructure};
pub use tee::{ElectedKind, Election, Tee, distill, opener};
pub use writer::{Boundary, HIGH_WATER_MARK, Pressure, QUEUE_DEPTH, Recorder};
