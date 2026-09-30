//! conforms: spu-backends-are-peers
//! conforms: spu-backend-from-artifact
//!
//! The backend seam, per `weaver-spu-Spec` section 4.1.
//!
//! **One trait, two backends, peers rather than a legacy and a target.** The
//! archived tree's ruling of 2026-06-11 made the GGUF and native paths
//! first-class peers and the survey carries the reasoning forward: GGUF owns
//! quantized artifacts on consumer devices, and the native path owns what a
//! tensor-parallel forward and a fine-tunable artifact need, since a GGUF
//! cannot be fine-tuned and a program that intends training as a continuation
//! cannot let that path decay.
//!
//! **Which backend serves is a property of the artifact, decided at admit.**
//! The header read of section 3 already answers it, so nothing elects a backend
//! separately and no configuration field names one. [`for_container`] is that
//! derivation, and it is total over [`Container`]: a container this build
//! cannot serve refuses rather than falling back to the other, which would be
//! the silent substitution the family registry also forbids.
//!
//! **Both engines stand.** [`for_container`]
//! answers whether this build can serve a container at all, and construction is
//! [`crate::residency::Resident::open_session`]'s, because an engine borrows the
//! model and only the residency holds one. The derivation and the construction
//! are two things for that reason rather than by preference: a function that
//! answered with a backend would need a model it has no way to be given.
//!
//! ## Why the generation loop is not behind this seam
//!
//! The Spec names the seam as one trait over open, append-and-generate, cancel,
//! flush, and close. That is the seam's vocabulary. **This trait carries the
//! primitives and `session.rs` carries the loop,** which is a representation
//! choice this document makes and states rather than assumes.
//!
//! The ground is that two of section 4's properties bind both backends: the
//! turn terminator is made resident on every path including the cancelled one,
//! and the cancel is checked between sampled tokens. Written inside each
//! backend, both properties would hold only as long as each implementer
//! remembered them, and a third backend would arrive owing two obligations no
//! type states. Written once above the seam, they hold for every backend by
//! construction, and the instruments that watch them watch one loop rather than
//! one loop per backend.

use crate::artifact::Container;

/// One token, as the family's tokenizer numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TokenId(pub u32);

/// What a decode operation refuses on.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodeFault {
    /// The delta would exceed the session's capacity. Carries the session's own
    /// account of itself, because the harness decides what a full context means
    /// and cannot decide it without the numbers.
    Overflow {
        resident: usize,
        requested: usize,
        capacity: usize,
    },
    /// The elision's span describes no removable region, per charter
    /// section 13.13: it overlaps the identity prefix, runs past the
    /// resident count, ends before it starts, or is empty.
    ///
    /// **A refusal rather than a fault**, typed on the seam, because the
    /// ask was answerable and wrong rather than the session being unwell.
    /// It carries the span it refused and the two bounds it was judged
    /// against, so the loop reads why rather than guessing which edge it
    /// crossed.
    UnremovableSpan {
        from: u64,
        to: u64,
        prefix: u64,
        resident: u64,
    },
    /// The device or the engine failed underneath.
    Engine { detail: String },
    /// This build was not compiled to serve this artifact's container.
    ContainerNotBuilt { container: Container },
    /// The session has not been opened, so there is no prefix to append to.
    /// A session an engine fault or a failed flush left unusable answers this
    /// too: what it says is that no serviceable prefix stands.
    NotOpen,
    /// Open is first and happens once, per the decode contract's ordering. A
    /// second open would silently rewind the resident length over accumulated
    /// turns, which is the exact failure the append-only discipline forbids.
    AlreadyOpen,
}

/// The primitives a backend supplies. The loop above them is `session.rs`'s.
///
/// The two implementations are peers. Neither is a default and neither is a
/// fallback, which is why this trait names no preferred one and why
/// [`for_container`] refuses rather than substituting.
pub trait Backend {
    /// Decode tokens at an absolute position, extending the resident state.
    ///
    /// **The position is absolute and supplied by the caller** rather than
    /// tracked here, so the session holds the one account of what is resident
    /// and a backend cannot disagree with it.
    fn decode_at(&mut self, tokens: &[TokenId], position: usize) -> Result<(), DecodeFault>;

    /// The distribution over the vocabulary at the current state, as logits.
    ///
    /// **This exists so the signals of Spec section 6 are computed where the
    /// distribution is, before the sampler.** The sampler consumes the
    /// distribution and answers one token, so a measurement taken after it has
    /// no distribution left to read and would have to reconstruct one. Reading
    /// it here does not advance the state: a caller may take it and then
    /// sample, which is exactly what the generation loop does.
    fn distribution(&self) -> Result<&[f32], DecodeFault>;

    /// Sample one token from the current state.
    fn sample(&mut self) -> Result<TokenId, DecodeFault>;

    /// Build this generation's sampler from its derived seed, per
    /// `weaver-spu-Spec` section 8.5.
    ///
    /// **Called at the start of every generation and nowhere else.** The
    /// sampler holds nothing between generations: it is built here from
    /// the seed the derivation named, and the penalty window is restored
    /// from the resident tail the caller supplies rather than accumulated
    /// across generations. One stream standing for a residency is what
    /// made a generation's draws depend on every draw before it, and what
    /// let a flush reseed the draw while clearing the window.
    ///
    /// `window` is the resident tail the penalty knobs describe, newest
    /// last, already truncated to the window length by the caller. After a
    /// flush it is the truncated tail, so nothing needs clearing and the
    /// accident cannot recur.
    fn reseed(&mut self, seed: u64, window: &[TokenId]) -> Result<(), DecodeFault>;

    /// Truncate resident state back to a position.
    ///
    /// **Only a family declaring [`FlushMechanism::TruncateToPosition`] may
    /// have this called on it**, per Spec section 4.4. A family whose state
    /// cannot roll back is re-established instead, because a truncation that
    /// returns success while recurrent state stays is the silent failure the
    /// append-only discipline exists to prevent.
    fn truncate_to(&mut self, position: usize) -> Result<(), DecodeFault>;

    /// Discard all state and return to an empty session.
    ///
    /// The expensive half of the flush, and the correct one where the cheap
    /// path is silently wrong.
    fn reestablish(&mut self) -> Result<(), DecodeFault>;

    /// Drain the residual reduction the engine accumulated since the last
    /// drain, per `weaver-spu-Spec` section 7: the reduction returns by the
    /// same path as the generation, and `None` is an engine that taps
    /// nothing, which is every engine whose residency was not admitted with
    /// readout elected. Default `None` so an engine without a tap states
    /// nothing.
    fn take_reduction(&mut self) -> Option<crate::readout::Reduction> {
        None
    }

    /// Arm or disarm the tap's column hold, per `weaver-spu-PRD` section
    /// 13.7's cadence: called once at the open where the column ask stood.
    /// Default nothing, which is every engine whose tap holds no column.
    fn hold_columns(&mut self, _hold: bool) {}

    /// The last committed forward's columns where the hold stands, one
    /// `Vec` per layer in layer order at the artifact's width, taken at
    /// the draw site so what is taken is the forward that feeds the draw.
    /// Default `None`, the same absence an unarmed tap answers.
    fn take_columns(&mut self) -> Option<Vec<Vec<f32>>> {
        None
    }

    /// Release the engine's resources.
    fn close(&mut self);
}

/// How a family's flush reaches its fixed outcome, per Spec section 4.4.
///
/// **The family declares which it is** and the decode path reads the
/// declaration rather than inferring it from a version string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushMechanism {
    /// The family's state permits truncation to a position, so the outcome is
    /// reached by truncating to the prefix's recorded length.
    TruncateToPosition,
    /// The family cannot roll back, so the outcome is reached by re-establishing
    /// the session and decoding the prefix again. Expensive and correct.
    ReestablishAndReprefill,
}

/// Derive the backend from the artifact's container.
///
/// **Nothing elects a backend separately.** This is the whole derivation, and
/// it is a function of the header read at admit. A container this build was not
/// compiled to serve refuses by name rather than falling back to the peer.
pub fn for_container(container: Container) -> Result<(), DecodeFault> {
    // **What this answers is serviceability, not a backend.** Construction
    // needs a model and this is given a container, so the two are split. A
    // container this build carries answers Ok and the caller constructs; one it
    // does not answers by name, so a reader meeting the refusal is told what is
    // missing rather than what failed.
    match container {
        #[cfg(feature = "gguf")]
        Container::Gguf => Ok(()),
        #[cfg(feature = "cuda")]
        Container::Safetensors => Ok(()),
        // A container this build was not compiled to serve refuses by name
        // rather than falling back to the peer, which would be the silent
        // substitution the family registry also forbids.
        #[allow(unreachable_patterns)]
        other => Err(DecodeFault::ContainerNotBuilt { container: other }),
    }
}
