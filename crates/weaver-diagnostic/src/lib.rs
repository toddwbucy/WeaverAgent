//! conforms: diagnostic-no-path-taking-constructor
//! conforms: diagnostic-holds-no-working-structure
//!
//! The diagnostic-trace writer: the counterpart record a replay pass is
//! authored into, per `weaver-diagnostic-Spec`. Re-exports and the
//! compile-fail set, and nothing else.
//!
//! **No path-taking constructor**, per section 5: the receive takes a
//! descriptor and this crate holds no name at any point. The three named
//! shapes each fail to compile:
//!
//! ```compile_fail
//! use weaver_diagnostic::{Recorder, RunRef, SessionRef};
//! let _ = Recorder::receive("/var/lib/weaver/diag.ndjson",
//!     RunRef("r-1".into()), SessionRef("s".into()));
//! ```
//!
//! ```compile_fail
//! use weaver_diagnostic::{Recorder, RunRef, SessionRef};
//! let path = String::from("/var/lib/weaver/diag.ndjson");
//! let _ = Recorder::receive(path, RunRef("r-1".into()), SessionRef("s".into()));
//! ```
//!
//! ```compile_fail
//! use weaver_diagnostic::{Recorder, RunRef, SessionRef};
//! let path = std::path::PathBuf::from("/var/lib/weaver/diag.ndjson");
//! let _ = Recorder::receive(path, RunRef("r-1".into()), SessionRef("s".into()));
//! ```
//!
//! **No working structure**, per section 5 and the contract's section 2:
//! this crate holds no RAM copy of the record it writes, a replay's present
//! being the holdings `weaver-state` serves, and the instrument is the
//! compile-fail absence of any accessor - no method on [`Recorder`] yields
//! a held event:
//!
//! ```compile_fail
//! fn f(r: &weaver_diagnostic::Recorder) -> &weaver_diagnostic::Event {
//!     r.structure()
//! }
//! ```
//!
//! The receive shape is read by a doctest, so an argument added to the one
//! constructor stops the build loudly, which is the mirror's compile pin:
//!
//! ```no_run
//! use std::os::fd::OwnedFd;
//! use weaver_diagnostic::{Recorder, RunRef, SessionRef};
//! fn shape(sink: OwnedFd) -> Result<Recorder, weaver_diagnostic::Failure> {
//!     Recorder::receive(sink, RunRef("r-1".into()), SessionRef("s-d".into()))
//! }
//! ```

mod event;
mod failure;
mod recorder;

pub use event::{
    AbandonReason, Divergence, Envelope, Event, Kind, ModelId, MonotonicNs, Payload, ReplayClosed,
    ReplayIdentity, ReplayOpened, ReplayOutcome, ResidualColumn, RunRef, Sequence, SessionRef,
    Subsystem, TemplateId, TokenId, TurnRef, WeightsHash,
};
pub use failure::{Failure, FieldName, SubmitRefusal, WriteError};
pub use recorder::Recorder;
