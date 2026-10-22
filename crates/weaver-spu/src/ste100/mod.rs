//! The STE100 duty's protocol and binding mechanics.
//!
//! This is an internal SPU library surface, not a link for another organ. The
//! production grammar compiler, model adapter and executable admission path are
//! separate dependencies of Spec 02 O02-1 and O02-2. None of these mechanics
//! qualifies an artifact or substitutes the sequence-label scorer for a binder.

pub mod admission;
pub mod binder;
pub mod channel;
pub mod dispatcher;

// The merged TEMPLATES compiler consumes this private algorithm at cutover.
#[allow(dead_code)]
mod inclusion;
