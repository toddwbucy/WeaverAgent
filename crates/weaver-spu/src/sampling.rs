//! conforms: spu-disposition-compels-election
//! conforms: spu-knob-set-includes-the-seed
//! conforms: spu-frozen-values-never-cross
//! conforms: spu-effective-values-recorded
//! conforms: spu-seed-derives-per-generation
//!
//! Sampling and the dispositions, per `weaver-spu-Spec` section 8.
//!
//! **Every knob carries a [`Disposition`], elected at the worker's composition
//! root.** A knob is `Frozen` with its value compiled into the binary, or
//! `OperatorTunable` and routed from the agent's configuration at load. The
//! machinery beneath takes a plain value and never learns which side supplied
//! it, which is what makes the two dispositions cost the same and a change
//! between them one line and a recompile.
//!
//! **A knob left without an election does not compile.** The type carries no
//! third case and no default, so a bare value where a disposition belongs is a
//! type error rather than a silent choice:
//!
//! Perturbation for the first pin: change a field to its bare type, as in
//! `pub temperature: f32`, and this stops failing, because a value with no
//! election then means something. Watched under exactly that change. A `From`
//! impl does not defeat it, since a struct field takes no implicit conversion,
//! which is why the field's own type is what carries the compulsion.
//!
//! ```compile_fail
//! use weaver_spu::sampling::{Disposition, Knobs};
//! // A value with no election. There is no `From<f32>` and no `Default`, so
//! // this cannot mean anything and does not compile.
//! let knobs = Knobs {
//!     temperature: 0.7,
//!     top_k: Disposition::OperatorTunable,
//!     top_p: Disposition::OperatorTunable,
//!     repetition_penalty: Disposition::OperatorTunable,
//!     repetition_window: Disposition::OperatorTunable,
//!     seed: Disposition::OperatorTunable,
//! };
//! ```
//!
//! ```compile_fail
//! use weaver_spu::sampling::Knobs;
//! // Nor is there a default to fall back on.
//! let knobs = Knobs::default();
//! ```
//!
//! Perturbation for the second pin: derive `Default` on [`Knobs`] and on
//! [`Disposition`], which needs a third case to default to, and this stops
//! failing. Watched under exactly that change, and the third case it forces is
//! the point: a default knob set cannot be built without inventing the
//! unelected state the type refuses to carry.
//!
//! **The knob set is a struct literal a doctest reads, and the seed is among
//! it.** Every member rides its own disposition, so a knob added or dropped
//! stops the build rather than being noticed later. Perturbation: drop the
//! `seed` field and this doctest fails on the missing initializer, which is the
//! membership check working. Watched under exactly that removal. The seed is a
//! knob for the
//! first time in this code's lineage: the archived tree carried it as a
//! hardcoded default and a determinism test and never made it configurable, and
//! a frozen seed beside a frozen sampling surface is what makes a binary's
//! declared starting field re-enterable, per apex section 8.
//!
//! ```
//! use weaver_spu::sampling::{Disposition, Knobs};
//! let knobs = Knobs {
//!     temperature: Disposition::Frozen(0.7),
//!     top_k: Disposition::Frozen(40),
//!     top_p: Disposition::Frozen(0.95),
//!     repetition_penalty: Disposition::Frozen(1.1),
//!     repetition_window: Disposition::Frozen(64),
//!     seed: Disposition::Frozen(11),
//! };
//! assert_eq!(knobs.tunable_names(), Vec::<&str>::new());
//! ```
//! conforms: spu-tunables-arrive-in-the-declaration

use std::collections::BTreeMap;

/// How a knob's value is supplied, elected at the composition root.
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

/// The knob set: temperature, top-k, top-p, the repetition penalty and its
/// window, and the seed.
///
/// **Membership is what the build checks.** The doctest at the module head
/// reads this literal with every member named, so adding a knob or dropping one
/// stops that test compiling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Knobs {
    pub temperature: Disposition<f32>,
    pub top_k: Disposition<u32>,
    pub top_p: Disposition<f32>,
    pub repetition_penalty: Disposition<f32>,
    pub repetition_window: Disposition<u32>,
    pub seed: Disposition<u64>,
}

/// What a caller must supply for the knobs this binary left tunable, keyed by
/// name. A tunable knob with no value supplied is a refused decode rather than
/// a default applied, since a default here would be an election this crate made
/// on the operator's behalf.
pub type TunableValues = BTreeMap<String, f64>;

/// The values sampling ran with, whichever side set them.
///
/// **This is what the record holds.** Reported into `model.request`'s payload,
/// so a frozen knob is as visible in the record as a tunable one and a replay
/// reads one list rather than joining two.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectiveKnobs {
    pub temperature: f32,
    pub top_k: u32,
    pub top_p: f32,
    pub repetition_penalty: f32,
    pub repetition_window: u32,
    pub seed: u64,
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

/// The session parameters, elected the same way the knobs are.
///
/// **A sibling of [`Knobs`] rather than a member of it**, because [`Knobs`] is
/// the sampling set and its doctest pins that membership: a capacity is not a
/// sampling knob and adding it there would make the pinned list mean something
/// else. What the two share is [`Disposition`], which is the mechanism rather
/// than the set, so both resolve against one supplied map.
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
    /// dispositions for the reason [`Knobs::tunable_names`] gives.
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
    /// A knob this binary left tunable was not supplied. Named, because an
    /// operator meeting it needs to know which one.
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

impl Knobs {
    /// The names of the knobs this binary left tunable.
    ///
    /// **This is the whole of what crosses the token seam**, per Spec section 8
    /// and the decode contract's conformance list. A frozen knob's value is
    /// compiled in and has no reason to travel, and the list being derived from
    /// the dispositions rather than written beside them is what keeps the two
    /// from drifting.
    pub fn tunable_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if !self.temperature.is_frozen() {
            names.push("temperature");
        }
        if !self.top_k.is_frozen() {
            names.push("top-k");
        }
        if !self.top_p.is_frozen() {
            names.push("top-p");
        }
        if !self.repetition_penalty.is_frozen() {
            names.push("repetition-penalty");
        }
        if !self.repetition_window.is_frozen() {
            names.push("repetition-window");
        }
        if !self.seed.is_frozen() {
            names.push("seed");
        }
        names
    }

    /// Resolve the elections against what the operator supplied.
    ///
    /// A frozen knob takes its compiled value and ignores anything supplied
    /// under its name, which is what frozen means: a supplied value for a
    /// frozen knob is not an error to report but a fact with no effect, because
    /// the wire never carried it in the first place.
    pub fn resolve(&self, supplied: &TunableValues) -> Result<EffectiveKnobs, KnobRefusal> {
        fn take<T: Copy>(
            disposition: &Disposition<T>,
            name: &'static str,
            supplied: &TunableValues,
            convert: impl Fn(f64) -> T,
        ) -> Result<T, KnobRefusal> {
            match disposition {
                Disposition::Frozen(value) => Ok(*value),
                Disposition::OperatorTunable => supplied
                    .get(name)
                    .map(|value| convert(*value))
                    .ok_or(KnobRefusal::Unsupplied { knob: name }),
            }
        }
        Ok(EffectiveKnobs {
            temperature: take(&self.temperature, "temperature", supplied, |v| v as f32)?,
            top_k: resolve_count(&self.top_k, "top-k", supplied, 2f64.powi(32), |v| v as u32)?,
            top_p: take(&self.top_p, "top-p", supplied, |v| v as f32)?,
            repetition_penalty: take(
                &self.repetition_penalty,
                "repetition-penalty",
                supplied,
                |v| v as f32,
            )?,
            repetition_window: resolve_count(
                &self.repetition_window,
                "repetition-window",
                supplied,
                2f64.powi(32),
                |v| v as u32,
            )?,
            seed: resolve_count(&self.seed, "seed", supplied, 2f64.powi(64), |v| v as u64)?,
        })
    }
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
