//! conforms: spu-one-binary
//! conforms: spu-two-floor-links-types-without-config
//! conforms: spu-fork-pins-resolve-as-pinned
//! conforms: spu-cudarc-caret-pinned
//! conforms: spu-no-runtime-no-logging-no-http
//! conforms: spu-two-feature-gates
//! conforms: spu-kernels-cross-with-their-fixtures
//!
//! The organ that holds the model, per `weaver-spu-Spec`.
//!
//! One binary, forked and exec'd by the harness during the enter fan-out. This
//! lib target is not an API for a consumer: nothing links this crate, the seam
//! being a socket rather than a link. It exists so that the Spec's doctest
//! instruments execute, which they cannot do under a bin-only crate.
//!
//! **The kernels cross verbatim and their fixtures cross with them,** per Spec
//! section 10. That claim is review's by reach rather than by election: no
//! suite can watch a comparison that was left behind, because a test cannot
//! detect its own absence, so whether the carry is complete is a fact a reader
//! establishes. `kernels/PROVENANCE.md` is what the reader reads, and it names
//! both what crossed and what has not.
//!
//! **One submodule exists.** A later operation type is a sibling of `decoder/`,
//! in its own process, and nothing is laid in for it: no trait, no variant, no
//! feature, no socket bound early. That adding one is adding a directory beside
//! `decoder/` is the reversibility test passing on this crate's own future.

pub mod artifact;
pub mod channel;
pub mod decoder;
pub mod family;
/// The device queries the admission judgment reads. Compiled only under the
/// `cuda` feature, because it is the one module that requires a driver.
#[cfg(feature = "cuda")]
pub mod gpu;
pub mod measurement;
pub mod readout;
pub mod residency;
pub mod sampling;
