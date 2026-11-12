//! conforms: spu-disposition-compels-election
//! conforms: spu-knob-set-includes-the-seed
//! conforms: spu-effective-values-recorded
//! conforms: spu-seed-derives-per-generation
//!
//! Sampling, the carrier and the session parameters' dispositions, per
//! `weaver-spu-Spec` section 8.
//!
//! **Every generation carries the values it samples with**, per the
//! operator's ruling of 2026-11-11. `AppendAndGenerate` carries a
//! [`Sampling`] and a token bound, and `ReFeed` carries the recorded
//! generation's [`Sampling`]. [`judge`] refuses a value outside its range
//! before the generation touches the session, and both engines build the
//! whole sampler from the carried values at each generation. No sampling
//! value reaches this crate at admit.
//!
//! **The session parameters keep a [`Disposition`], elected at the worker's
//! composition root.** A parameter is `Frozen` with its value compiled into
//! the binary, or `OperatorTunable` and routed from the agent's configuration
//! at load. The machinery beneath takes a plain value and never learns which
//! side supplied it, which is what makes the two dispositions cost the same
//! and a change between them one line and a recompile.
//!
//! **A parameter left without an election does not compile.** The type
//! carries no third case and no default, so a bare value where a disposition
//! belongs is a type error rather than a silent choice:
//!
//! Perturbation for the first pin: change a field to its bare type, as in
//! `pub context_capacity: u32`, and this stops failing, because a value with
//! no election then means something. Watched under exactly that change. A
//! `From` impl does not defeat it, since a struct field takes no implicit
//! conversion, which is why the field's own type is what carries the
//! compulsion.
//!
//! ```compile_fail
//! use weaver_spu::sampling::{Disposition, SessionParameters};
//! // A value with no election. There is no `From<u32>` and no `Default`, so
//! // this cannot mean anything and does not compile.
//! let parameters = SessionParameters {
//!     context_capacity: 4096,
//!     max_tokens_per_turn: Disposition::OperatorTunable,
//! };
//! ```
//!
//! ```compile_fail
//! use weaver_spu::sampling::SessionParameters;
//! // Nor is there a default to fall back on.
//! let parameters = SessionParameters::default();
//! ```
//!
//! Perturbation for the second pin: derive `Default` on [`SessionParameters`]
//! and on [`Disposition`], which needs a third case to default to, and this
//! stops failing. Watched under exactly that change, and the third case it
//! forces is the point: a default parameter set cannot be built without
//! inventing the unelected state the type refuses to carry.
//!
//! **The carried set is a struct literal a doctest reads, and the seed is
//! among it.** Every member is named, so a member added to or dropped from
//! `weaver-types`' [`Sampling`] stops the build rather than being noticed
//! later. Perturbation: drop the `seed` initializer and this doctest fails on
//! the missing field, which is the membership check working. Watched under
//! exactly that removal. The seed is a member for the first time in this
//! code's lineage: the archived tree carried it as a hardcoded default and a
//! determinism test and never made it configurable.
//!
//! ```
//! use weaver_spu::sampling::Sampling;
//! let carried = Sampling {
//!     temperature: 0.7,
//!     top_k: 40,
//!     top_p: 0.95,
//!     repetition_penalty: 1.1,
//!     repetition_window: 64,
//!     seed: 11,
//! };
//! assert_eq!(carried.out_of_range(), None);
//! ```

use std::collections::BTreeMap;

pub use weaver_types::Sampling;
use weaver_types::{SamplingMember, TokenRefusal};

/// How a session parameter's value is supplied, elected at the composition
/// root.
///
/// Two cases and no third. There is no `Default` and no conversion from a bare
/// value, which is what makes the election compulsory rather than encouraged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Disposition<T> {
    /// Compiled into the binary. The operator cannot move it and the wire never
    /// carries it.
    Frozen(T),
    /// Routed from the agent's configuration at load.
    OperatorTunable,
}

impl<T> Disposition<T> {
    pub fn is_frozen(&self) -> bool {
        matches!(self, Disposition::Frozen(_))
    }
}

/// What a caller must supply for the session parameters this binary left
/// tunable, keyed by name. A tunable parameter with no value supplied is a
/// refused load rather than a default applied, since a default here would be
/// an election this crate made on the operator's behalf.
pub type TunableValues = BTreeMap<String, f64>;

/// Judge one generation's carried values before it touches the session, per
/// `weaver-spu-Spec` sections 7.5 and 8.
///
/// `bound` is the generation's `max_tokens` beside the session's ceiling, and
/// is `None` for a re-feed, which carries no bound. `field_depth` is the
/// elected field's depth, where the field stands.
///
/// **The ranges are the floor's [`Sampling::out_of_range`]**, the rule the
/// harness judges the declaration's defaults by at load, so the early refusal
/// and this one cannot drift apart. A zero bound is not refused: the grammar
/// refuses no zero ceiling, and the generation it bounds reaches its limit
/// before its first draw.
pub fn judge(
    sampling: &Sampling,
    bound: Option<(u64, u64)>,
    field_depth: Option<u32>,
) -> Result<(), TokenRefusal> {
    if let Some(member) = sampling.out_of_range() {
        return Err(TokenRefusal::SamplingOutOfRange { member });
    }
    if let Some((max_tokens, ceiling)) = bound
        && max_tokens > ceiling
    {
        return Err(TokenRefusal::SamplingOutOfRange {
            member: SamplingMember::MaxTokens,
        });
    }
    if let Some(depth) = field_depth
        && weaver_types::field_below_cutoff(depth, sampling.top_k)
    {
        return Err(TokenRefusal::FieldBelowCutoff {
            depth,
            top_k: sampling.top_k,
        });
    }
    Ok(())
}

/// Resolve one count-valued parameter, judging the value before the cast.
///
/// **The cast cannot fail, which is why this exists.** A float cast to an
/// integer answers for every input: it truncates toward zero, so `3.7` becomes
/// `3`, it maps `NaN` to zero, and it saturates at the target's bounds, so
/// `-1.0` becomes `0` and `1e20` becomes `u32::MAX`. Each substitutes a number
/// no operator wrote.
///
/// **`ceiling` is exclusive and is a power of two rather than the type's
/// maximum.** `u32::MAX as f64` is exact, but `u64::MAX as f64` rounds up to
/// `2^64`, so a bound written as the maximum would admit a value of exactly
/// `2^64`, which then saturates to `u64::MAX`. That is the substitution this
/// function exists to refuse, surviving inside the refusal. Passing the power
/// of two and rejecting at or above it closes that for every width.
fn resolve_count<T: Copy>(
    disposition: &Disposition<T>,
    name: &'static str,
    supplied: &TunableValues,
    ceiling: f64,
    convert: impl Fn(f64) -> T,
) -> Result<T, KnobRefusal> {
    match disposition {
        Disposition::Frozen(value) => Ok(*value),
        Disposition::OperatorTunable => {
            let value = *supplied
                .get(name)
                .ok_or(KnobRefusal::Unsupplied { knob: name })?;
            if value.fract() != 0.0 || value < 0.0 || value >= ceiling {
                return Err(KnobRefusal::NotACount {
                    knob: name,
                    supplied: value,
                });
            }
            Ok(convert(value))
        }
    }
}

/// The session parameters: the context capacity and the generation ceiling,
/// each elected by a [`Disposition`] (`weaver-spu-Spec` section 4.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionParameters {
    pub context_capacity: Disposition<u32>,
    pub max_tokens_per_turn: Disposition<usize>,
}

/// The session parameters a load ran with, whichever side set them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectiveSessionParameters {
    pub context_capacity: u32,
    pub max_tokens_per_turn: usize,
}

impl SessionParameters {
    /// The names of the parameters this binary left tunable, derived from the
    /// dispositions rather than written beside them, so the two cannot drift.
    pub fn tunable_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if !self.context_capacity.is_frozen() {
            names.push("context-capacity");
        }
        if !self.max_tokens_per_turn.is_frozen() {
            names.push("max-tokens-per-turn");
        }
        names
    }

    /// Resolve against what the declaration supplied. Both members are counts,
    /// so both are judged before the cast that cannot fail.
    pub fn resolve(
        &self,
        supplied: &TunableValues,
    ) -> Result<EffectiveSessionParameters, KnobRefusal> {
        Ok(EffectiveSessionParameters {
            context_capacity: resolve_count(
                &self.context_capacity,
                "context-capacity",
                supplied,
                2f64.powi(32),
                |v| v as u32,
            )?,
            max_tokens_per_turn: resolve_count(
                &self.max_tokens_per_turn,
                "max-tokens-per-turn",
                supplied,
                2f64.powi(64),
                |v| v as usize,
            )?,
        })
    }
}

/// Why a resolve refused.
#[derive(Debug, Clone, PartialEq)]
pub enum KnobRefusal {
    /// A parameter this binary left tunable was not supplied. Named, because
    /// an operator meeting it needs to know which one.
    Unsupplied { knob: &'static str },
    /// A value supplied for a count is not one: fractional, negative, or past
    /// what the count's type holds.
    ///
    /// **The refusal exists because the conversion cannot fail.** A float cast
    /// to an integer answers for every input: it truncates toward zero, so
    /// `3.7` becomes `3`, it maps `NaN` to zero, and it saturates at the
    /// target's bounds, so `-1.0` becomes `0` and `1e20` becomes `u32::MAX`.
    /// Each of those substitutes a number no operator wrote, silently, which is
    /// the substitution this crate refuses everywhere else. The floor judged
    /// finiteness at parse, per `weaver-types-Spec` section 2, and which names
    /// are counts is this crate's election so this is where it is judged.
    NotACount { knob: &'static str, supplied: f64 },
}

/// The seed one generation draws from, per `weaver-spu-Spec` section 8.5.
///
/// **Stated to the bit there and implemented here to match**, because two
/// builds differing by one constant produce runs that cannot be compared
/// and nothing says so. The Spec carries test vectors and the test below
/// is those vectors.
///
/// The three inputs fix the stream and nothing else: the declared seed the
/// operator wrote, the turn's reference, and which generation of that turn
/// this is, counted from zero. A turn runs as many generations as its tool
/// rounds, and two sharing a seed would draw one stream twice.
pub fn derived_seed(declared: u64, turn: &weaver_types::TurnKey, generation: u64) -> u64 {
    let state = splitmix64_finalize(declared ^ fnv1a_64(turn.0.as_bytes()));
    splitmix64_finalize(state ^ generation)
}

/// The turn's reference as a number, FNV-1a over its UTF-8 bytes. The
/// reference is a string admin minted and this crate needs a number from
/// it without caring what it spells.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `splitmix64`'s finalizer: short enough to state in full, standard
/// enough to be recognised, and well distributed on sequential inputs,
/// the last mattering because the generation ordinal is sequential and
/// adjacent turns must not draw adjacent streams.
fn splitmix64_finalize(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests;
