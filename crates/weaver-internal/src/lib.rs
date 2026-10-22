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
//! Calculator retains Spec section 2's interim pure-function surface. The
//! INTERNAL-QUERY amendment gives recall its pure constructor, borrowed query
//! accessor and empty-subject refusal. Neither member initiates an effect.

pub mod calculator;
pub mod recall;
