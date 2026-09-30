//! conforms: traits-provider-dyn-compatible
//! conforms: traits-provider-no-futures-dep
//!
//! The provider contract, deferred. This module exists as a placement, per
//! `weaver-traits-Spec` section 6: the provider's vocabulary is the request, the
//! streamed response, and the error, and every one of those is decode-seam
//! material that `weaver-spu-PRD` section 8 defers to the token workflow by name.
//! No vocabulary clause draws `provider-trait` - its consumer is the composition
//! root's injection, inside the body, so no contract has a clause to draw it with.
//!
//! Three constraints the token workflow inherits when it settles the signature:
//!
//! 1. The trait is dyn-compatible - the concrete transport is constructed at the
//!    worker composition root and injected, which is what lets the composition
//!    root be the only place a wire format is named.
//! 2. Its futures carry `+ Send`, for the reason the tool contract gives.
//! 3. The streamed response is expressed without a `futures` dependency on the
//!    floor, the candidate being a boxed future yielding a receiver the caller
//!    drains. If the token workflow finds that shape costs measurable latency on
//!    the decode path, the dependency is a decision that pass makes with the
//!    measurement in hand.
