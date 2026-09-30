//! conforms: traits-tool-dyn-compatible
//! conforms: traits-tool-boxed-future-send
//!
//! The tool contract, chartered with the tool workflow's opening act of
//! 2026-08-17, per `weaver-traits-Spec` section 5.
//!
//! The block this module carried lifted on its own terms: it held because tool
//! dispatch was harness-internal and no seam crossed it, and the ratified loop
//! boundary of 2026-08-11 inverted that ground - every tool sits outside the
//! reasoning loop. The tool boundary ruling of 2026-08-18 then narrowed the
//! constituency to what `weaver-traits-Spec` section 5's foot always named:
//! the elected outward corner, the registered service the egress seam
//! awaits. The gate holds one tool, the shell, its own verb dispatched with
//! no table, and the inward callables of `weaver-internal` are reached by
//! their own surface - so today this trait has a chartered shape and no
//! dispatching consumer, a definition waiting on its corner.
//!
//! **The name and the schema are primitives on purpose.** `tool-name` is
//! `weaver-types` vocabulary and that crate names this one as its one
//! floor-link, so a trait naming that type would close a dependency cycle -
//! and the floor invariant is the reason this crate refuses internal
//! dependencies at all. What compares a name that crossed against what a tool
//! answers is the dispatching consumer this crate does not have, and no crate
//! carries both definitions today: `weaver-gate`'s manifest names
//! `weaver-types` as its one internal dependency, this crate having left it
//! with the tool boundary ruling of 2026-08-18. `weaver-traits-Spec` section 5
//! states it and this header does not carry a second copy.
//!
//! **No safety classification of any kind**, per `weaver-traits-PRD` section
//! 3.1, which the workflow did not weaken: a trait method asking a tool
//! whether it is dangerous is a heuristic standing where a boundary already
//! stands.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

/// A tool's own account of its failure: content the conversation carries and
/// the model reasons over, never a channel fault, per the layer split the
/// execution exchange states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolFailure {
    pub detail: String,
}

/// The tool contract, per `weaver-traits-Spec` section 5.
///
/// Dyn-compatible, because the eventual dispatcher of a registered service
/// does not know the tool's identity. The future is explicitly boxed rather
/// than `async fn` in trait, which is not dyn-compatible, and carries `Send`
/// so no executor election leaks onto the floor.
pub trait Tool {
    /// The name the model calls, as the gate compares it against the drawn
    /// `tool-name` that crossed the exchange.
    fn name(&self) -> &str;

    /// The schema this tool advertises to the model, the charter's own
    /// vocabulary item carried by the signature. How an advertisement reaches
    /// the prompt assembly that renders it is the schema act's.
    fn schema(&self) -> &str;

    /// Execute one call. The arguments arrive as the model spoke them,
    /// uninterpreted by any party between the parse and this method.
    fn execute(
        &self,
        arguments: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ToolFailure>> + Send + '_>>;
}
