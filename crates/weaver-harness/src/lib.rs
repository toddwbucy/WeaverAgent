//! conforms: harness-one-constructor
//! conforms: harness-path-shapes-pinned-by-doctest
//! conforms: harness-outcome-one-case
//! conforms: harness-descriptors-owned-types
//! conforms: harness-loop-mints-no-port
//! conforms: harness-no-runtime-no-logging-no-http
//!
//! The hub: loop 0, the channels and their custody, trace authorship, prompt
//! assembly's deterministic floor, and loop 1's seat.
//!
//! The doctests below are the compile-time instruments of
//! `weaver-harness-Spec` section 8.
//!
//! Descriptors are owned types end to end
//! (`harness-descriptors-owned-types`): a handle that escapes its owner is a
//! move the borrow checker sees, not an integer copied silently. `listen`
//! consumes the listener it is handed, so this compiles:
//!
//! ```
//! use weaver_harness::{
//!     AdoptionFault, CoordinationListener, Harness, OrganBinaries, OrganParameters,
//! };
//! fn shape(listener: CoordinationListener, organs: OrganBinaries) {
//!     let _served: Result<Harness, AdoptionFault> =
//!         Harness::listen(listener, organs, OrganParameters::default());
//! }
//! ```
//!
//! ...and reusing the moved descriptor does not:
//!
//! ```compile_fail
//! use std::os::fd::{AsRawFd, OwnedFd};
//! use weaver_harness::{Harness, OrganBinaries};
//! fn f(end: OwnedFd, organs: OrganBinaries) {
//!     let _ = Harness::adopt(end, organs);
//!     let _ = end.as_raw_fd();
//! }
//! ```
//!
//! One constructor (`harness-one-constructor`): the fields are private and no
//! second path exists, so a struct literal fails to compile.
//!
//! ```compile_fail
//! let _ = weaver_harness::Harness { coordination: todo!(), organs: todo!() };
//! ```
//!
//! No path-taking surface (`harness-path-shapes-pinned-by-doctest`): the three
//! named shapes each fail to compile where the channel takes owned
//! descriptors. The general prohibition stays review's - these three reach the
//! shapes they name and not the open set of every way a path becomes a call
//! argument.
//!
//! ```compile_fail
//! use weaver_harness::{Harness, OrganBinaries};
//! let _ = Harness::adopt("/run/weaver/alpha/coordination.sock", OrganBinaries {
//!     spu: "/opt/weaver/bin/spu".into(), gate: "/opt/weaver/bin/gate".into(),
//! });
//! ```
//!
//! ```compile_fail
//! use weaver_harness::{Harness, OrganBinaries};
//! let path = String::from("/run/weaver/alpha/coordination.sock");
//! let _ = Harness::adopt(path, OrganBinaries {
//!     spu: "/opt/weaver/bin/spu".into(), gate: "/opt/weaver/bin/gate".into(),
//! });
//! ```
//!
//! ```compile_fail
//! use std::path::PathBuf;
//! use weaver_harness::{Harness, OrganBinaries};
//! let _ = Harness::adopt(PathBuf::from("/run/weaver/alpha/coordination.sock"),
//!     OrganBinaries { spu: "/opt/weaver/bin/spu".into(), gate: "/opt/weaver/bin/gate".into() });
//! ```
//!
//! The outcome enum is exhaustive (`harness-outcome-one-case`), so a second
//! case reaches every caller loudly:
//!
//! ```
//! use weaver_harness::Outcome;
//! fn name(outcome: Outcome) -> &'static str {
//!     match outcome {
//!         Outcome::Left => "left",
//!     }
//! }
//! ```
//!
//! A loop mints no port (`harness-loop-mints-no-port`): `Ports` has private
//! fields and no reachable constructor, so a builder's loop composes the
//! granted surface or does not compile.
//!
//! ```compile_fail,E0624
//! // `grant` is crate-private (E0624), so a builder's loop cannot name it.
//! let _ = weaver_harness::Ports::grant;
//! ```
//!
//! The channel state's three positions and the floor's exhaustive wire enums
//! together make every out-of-order directive reach a match arm rather than a
//! flag check, which `serve` holds structurally.

mod assembly;
mod authorship;
mod channel;
mod engine;
mod failure;
// Additive state-flow mechanics. The qualified, coupled foundation cutover
// replaces the legacy serving owners together, never one seam at a time.
#[allow(dead_code)]
mod flow;
mod lifecycle;
mod lifecycle_table;
mod open_evidence;
mod record;
mod replay;
mod spawn;
mod state;
mod tools;
mod tools_cgroup;

// Exercise the actual external worker adapter in the composed unit instrument.
#[cfg(test)]
extern crate self as weaver_harness;

pub use assembly::{Prompt, assemble};
pub use authorship::{Author, licensed};
pub use channel::{
    ChildEnd, CoordinationListener, DecodeChannel, FIRST_ORGAN_DESCRIPTOR, OrganChannel,
    RELAY_LIFETIME_DESCRIPTOR, RUN_LOCK_DESCRIPTOR, bind_coordination, keep_close_on_exec,
    keep_start_step_descriptors, place_child_ends,
};
pub use engine::{Ports, TurnError, TurnOutcome};
pub use failure::{AdoptionFault, ChannelFault, Outcome, UnlicensedMessage};
pub use flow::context::worker_startup::entered_rollback::forced::{
    FoundationForcedLeave, FoundationForcedLeaveHandoff, FoundationForcedLeaveProgress,
};
pub use flow::context::worker_startup::entered_rollback::graceful_exit::{
    FoundationGracefulLeave, FoundationGracefulLeaveHandoff, FoundationGracefulLeaveProgress,
};
pub use flow::context::worker_startup::entered_rollback::{
    FoundationEnteredRollback, FoundationEnteredRollbackHandoff, FoundationEnteredRollbackProgress,
};
pub use flow::context::worker_startup::expired::{
    FoundationExpiredEnter, FoundationExpiredRollback, FoundationExpiredRollbackExit,
    FoundationExpiredRollbackProgress,
};
pub use flow::context::worker_startup::failed::{
    FoundationStartupExit, FoundationStartupExitProgress, FoundationStartupHandoff,
};
pub use flow::context::worker_startup::input::flags::{
    FoundationFlagObservation, FoundationFlags, FoundationFlagsError, FoundationFlagsRefusal,
};
pub use flow::context::worker_startup::input::generation::FoundationGeneration;
pub use flow::context::worker_startup::input::generation::emission::{
    FoundationEmission, FoundationOutbound, FoundationRepairAdmission, FoundationRestatement,
};
pub use flow::context::worker_startup::input::halt::exit::{
    FoundationExitProgress, FoundationFailedExit, FoundationFailedHandoff,
};
pub use flow::context::worker_startup::input::halt::{FoundationHalt, FoundationHaltProgress};
pub use flow::context::worker_startup::input::management::{
    FoundationManagement, FoundationManagementProgress,
};
pub use flow::context::worker_startup::input::plan_ports::{
    CheckedStepSuccessor, CheckedWork, ClosureEvidence, ClosureReceipt,
    FoundationPlanAdmissionRefusal, FoundationPlanOperation, FoundationPlanOperationError,
    FoundationPlanOutcome, FoundationPlanRequest,
};
pub use flow::context::worker_startup::input::wind_down::FoundationWindDownProgress;
pub use flow::context::worker_startup::input::{
    FoundationInput, FoundationInputProgress, FoundationInputResult, FoundationTurn,
};
pub use flow::context::worker_startup::rollback::{FoundationRollback, FoundationRollbackProgress};
pub use flow::context::worker_startup::{
    FoundationAdmission, FoundationAdmissionProgress, FoundationDiagnosticResult, FoundationEnter,
    FoundationEnterProgress, FoundationProcessExit, FoundationProgram, FoundationRun,
    FoundationServing,
};
pub use lifecycle::{Harness, OrganBinaries, OrganParameters};
pub use record::{Record, RecordFailure};
pub use spawn::fork_organ;
pub use state::{Recalled, RunShape, SessionShape, StateSeam};
pub use tools::ToolResult;
pub use weaver_trace::LoopIdentity;

/// A path under the temp directory for this crate's unit tests, removed when
/// the test ends, pass or fail: the guard drops on the unwind a failed
/// assertion takes as on a clean return, so no run leaves a socket, a sink or
/// a directory behind (#690 item C2.9).
#[cfg(test)]
mod scratch;

#[cfg(test)]
mod test_child;

pub use flow::context::worker_startup::rollback::failed::{
    FoundationRollbackExit, FoundationRollbackExitProgress, FoundationRollbackHandoff,
};
