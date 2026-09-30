//! conforms: internal-no-advertisement
//! conforms: internal-member-initiates-nothing
//! conforms: internal-member-pure-function
//!
//! `weaver-internal`: the operator's promotion space, per the tool boundary
//! ruling of 2026-08-18 and `weaver-internal-PRD`. Members are loop-reachable
//! callables dispatched inward - never tools the agent knows. The agent's
//! innate tool surface is the shell, held by the gate as its own outbound
//! verb, and no member here is advertised to the model, carries a schema, or
//! enters any prompt assembly, per `weaver-internal-Spec` section 2.
//!
//! The three review absences this file cites hold crate-wide: nothing here
//! advertises, nothing initiates - no thread spawned, no process forked, no
//! socket dialed or bound - and every framework member is a function of its
//! arguments alone, no filesystem, no network, no clock, no randomness, and
//! no state between calls. The manifest half of the pure bar is the empty
//! normal dependency set, read by `tests/manifest.rs`.
//!
//! Until the charter's calling-surface cell settles, a member's surface is
//! the interim minimum of Spec section 2: one public pure function per
//! member on this library surface.

pub mod calculator;
