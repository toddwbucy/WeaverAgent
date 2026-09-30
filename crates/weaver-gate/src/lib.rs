//! conforms: gate-one-binary
//! conforms: gate-floor-link-types-without-config
//! conforms: gate-no-runtime-no-logging-no-yaml
//!
//! The loop's membrane, per `weaver-gate-Spec` and the ratified
//! reasoning-loop boundary of 2026-08-11: the agent's membrane is the network
//! boundary, and this crate is the loop's.
//!
//! One binary, forked and exec'd by the harness during the enter fan-out, and
//! no other crate links it. This lib target is not an API for a consumer, and
//! why it exists is `weaver-gate-Spec` section 1's to say rather than this
//! header's, per gate G5 - `tests/manifest.rs` pins its presence and cites
//! the same clause.
//!
//! One module per obligation, per `weaver-gate-Spec` section 1: `channel.rs`
//! holds the seam end, `hook.rs` the instruction's resolution and the bind
//! and the predicate, `relay.rs` the pass-through of section 4, and
//! `tools.rs` the shell execution of section 8. The exchange service that
//! drives the seam end is the serve loop in `main.rs`.

pub mod channel;
pub mod hook;
pub mod relay;
pub mod tools;
