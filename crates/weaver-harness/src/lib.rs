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
mod lifecycle;
mod record;
mod replay;
mod spawn;
mod state;
mod tools;

pub use assembly::{Prompt, assemble};
pub use authorship::{Author, licensed};
pub use channel::{
    ChildEnd, CoordinationListener, DecodeChannel, FIRST_ORGAN_DESCRIPTOR, OrganChannel,
    bind_coordination, place_child_ends,
};
pub use engine::{Ports, TurnError, TurnOutcome};
pub use failure::{AdoptionFault, ChannelFault, Outcome, UnlicensedMessage};
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
mod scratch {
    pub(crate) struct Scratch(pub(crate) std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            match std::fs::symlink_metadata(&self.0) {
                Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
                Ok(_) => drop(std::fs::remove_file(&self.0)),
                Err(_) => {}
            }
        }
    }

    impl std::ops::Deref for Scratch {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl AsRef<std::path::Path> for Scratch {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }

    /// A fresh directory under the temp directory, any earlier one of the
    /// same name removed first so a stale socket cannot refuse a bind.
    pub(crate) fn dir(name: String) -> Scratch {
        let dir = Scratch(std::env::temp_dir().join(name));
        let _ = std::fs::remove_dir_all(&dir.0);
        std::fs::create_dir_all(&dir.0).expect("scratch dir");
        dir
    }
}
