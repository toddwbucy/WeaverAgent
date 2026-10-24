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
//! Binaries forked and exec'd by the harness during the enter fan-out. This
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
//! Semantic operations live beside `decoder/`, with their own process seams.
//! The `ste100` module implements the new duty's protocol and binding mechanics.
//! Its separate executable refuses unsupported artifacts; production grammar,
//! backend and measured limits remain qualification work.

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
pub mod ste100;

// One copy of shared helpers per unit-test binary, including isolated children.
#[cfg(test)]
#[path = "../tests/common/ste100_fixture.rs"]
mod ste100_test_fixture;
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;
