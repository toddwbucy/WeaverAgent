//! conforms: harness-tool-result-granted-not-minted
//!
//! The granted tool result, per `weaver-harness-Spec` section 6 and the tool
//! workflow's opening act: the loop closes at the type level here.
//!
//! **The grant is constructed at exactly one site**, the completion of the
//! execution exchange of `weaver-harness-gate-contract` section 2, whichever
//! of the exchange's three contents arrived - the tool's result, the tool's
//! failure, or the gate's refusal of an unheld name - because each is a fact
//! the model must learn, and a refusal that could not author a tool-result
//! turn would leave the conversation with a hole where the answer goes.
//!
//! **Three negatives, each a compile property**, the `PeerIdentity` pattern
//! one seam over: the gate exchange supplies it and no code asserts it.
//!
//! No `Deserialize` (`harness-tool-result-granted-not-minted`): bytes never
//! mint a grant, on any seam, in any test.
//!
//! ```compile_fail
//! fn de<T: for<'de> serde::Deserialize<'de>>() {}
//! de::<weaver_harness::ToolResult>();
//! ```
//!
//! No public construction: the fields are private and the one constructor is
//! `pub(crate)`, so the struct literal below does not compile and no public
//! function returns one from anything a consumer can supply.
//!
//! ```compile_fail
//! let fake = weaver_harness::ToolResult { content: String::new() };
//! ```
//!
//! No conversion from the record: the conversation's `ToolResultBlock` is a
//! serde record that grants nothing, and no `From` bridges it back.
//!
//! ```compile_fail
//! fn from<T: From<weaver_traits::ToolResultBlock>>() {}
//! from::<weaver_harness::ToolResult>();
//! ```

use weaver_traits::ToolResultBlock;
use weaver_types::ToolOutcome;

/// How many execution rounds one turn may run before the harness stops
/// re-generating, the composition root's bound like `HEADROOM_BYTES`: a model
/// that answers every result with another call would otherwise hold the turn
/// open without limit. The bound is a refusal of further calls rather than of
/// the turn: the final generation's emission stands.
///
/// **Raised from four on 2026-08-20, against a task that measured it.** Four
/// was set where a round trip was a demonstration, and it stops a staged
/// task at its fourth step: a problem whose stages depend on each other -
/// an area, then the counts it divides into, then the prices those counts
/// carry, then the total - reaches the bound mid-derivation and the model
/// finishes from what it holds, which is where a model starts inventing
/// numbers. A bound that turns a tool user into a fabricator is measuring
/// the bound rather than the model. The number is what the ordinary
/// long-chain task needs with room over it, not a ceiling anyone should
/// reach.
///
/// **What it does not bound stays worth knowing.** A round is one
/// generation and its calls, so this counts sequential dependence and never
/// the calls inside one round: the emission of 2026-08-20 that carried
/// thirty-three identical calls spent one round for all of them and was
/// held by the turn's token cap instead. Against a runaway the real stops
/// are that cap and the session's capacity, this one guarding the shape
/// they do not, a model that answers every result with one more call.
pub(crate) const MAX_TOOL_ROUNDS: usize = 32;

/// A tool result the gate exchange granted.
///
/// Obtainable from that exchange's completion alone, per the module note. The
/// interior is deliberately not public: nobody inspects a tool result's
/// fields to know it came through the gate - holding the value is the proof,
/// which is what box granularity means.
pub struct ToolResult {
    content: String,
}

impl ToolResult {
    /// The one construction site's door, crate-private: called where the
    /// execution exchange completes and nowhere else.
    ///
    /// Every outcome becomes renderable content, in words the model reasons
    /// over, whichever of the exchange's four contents arrived: a result is
    /// itself, a refusal and an error each name their speaker, and a kill
    /// names its cause and carries what drained before it - marked as
    /// partial, so a fragment cannot read as an answer.
    pub(crate) fn granted(outcome: &ToolOutcome) -> ToolResult {
        let content = match outcome {
            ToolOutcome::Result { content } => content.clone(),
            ToolOutcome::Refused { reason } => {
                format!("the call was refused: {reason}")
            }
            ToolOutcome::Errored { detail } => {
                format!("the tool machinery failed: {detail}")
            }
            ToolOutcome::Killed { partial, by } => {
                let ending = match by {
                    weaver_types::KillCause::Clock => {
                        "the command ran past the clock and was killed"
                    }
                    weaver_types::KillCause::Cancel => {
                        "the command was killed by the operator's stop"
                    }
                };
                match partial {
                    Some(partial) => {
                        format!("{ending}. Partial output before the kill:\n{partial}")
                    }
                    None => format!("{ending}, with no output before the kill"),
                }
            }
        };
        ToolResult { content }
    }

    /// The conversation's record of this grant, minted at the author door and
    /// nowhere else: the one direction the two types convert, grant to
    /// record, never back.
    pub(crate) fn block(&self) -> ToolResultBlock {
        ToolResultBlock {
            content: self.content.clone(),
        }
    }
}
