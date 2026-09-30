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
mod tests {
    use super::*;

    fn all_frozen() -> Knobs {
        Knobs {
            temperature: Disposition::Frozen(0.7),
            top_k: Disposition::Frozen(40),
            top_p: Disposition::Frozen(0.95),
            repetition_penalty: Disposition::Frozen(1.1),
            repetition_window: Disposition::Frozen(64),
            seed: Disposition::Frozen(11),
        }
    }

    /// **Frozen values never cross the wire.** Only the operator-tunable
    /// remainder travels on the token seam, and the list that says which is
    /// derived from the elections rather than maintained beside them.
    ///
    /// Perturbation: make `tunable_names` push a name unconditionally for any
    /// knob, or return every name, and this test fails on the all-frozen case.
    /// Watched under exactly that change.
    #[test]
    fn a_frozen_knob_is_not_on_the_wire() {
        assert_eq!(all_frozen().tunable_names(), Vec::<&str>::new());

        let mut knobs = all_frozen();
        knobs.seed = Disposition::OperatorTunable;
        knobs.top_p = Disposition::OperatorTunable;
        assert_eq!(knobs.tunable_names(), vec!["top-p", "seed"]);
    }

    /// **The effective values are what the record holds, whichever side set
    /// them.** A frozen knob resolves to its compiled value and a tunable one
    /// to what was supplied, and the result carries all six either way, so a
    /// replay reads one list rather than joining two.
    #[test]
    fn the_effective_values_carry_every_knob_from_either_side() {
        let mut knobs = all_frozen();
        knobs.temperature = Disposition::OperatorTunable;
        let mut supplied = TunableValues::new();
        supplied.insert("temperature".into(), 0.2);

        let effective = knobs.resolve(&supplied).expect("every tunable supplied");
        assert_eq!(effective.temperature, 0.2, "the tunable came from the wire");
        assert_eq!(effective.seed, 11, "and the frozen one from the binary");
        assert_eq!(effective.top_k, 40);
    }

    /// A frozen knob ignores a value supplied under its name. Frozen means the
    /// wire never carried it, so a value arriving anyway is a fact with no
    /// effect rather than a conflict to adjudicate.
    #[test]
    fn a_supplied_value_cannot_move_a_frozen_knob() {
        let mut supplied = TunableValues::new();
        supplied.insert("seed".into(), 999.0);
        let effective = all_frozen().resolve(&supplied).expect("nothing is tunable");
        assert_eq!(effective.seed, 11, "the compiled value stands");
    }

    /// **A tunable knob with no value refuses rather than defaulting**, naming
    /// the knob. A default here would be an election this crate made on the
    /// operator's behalf, which is the thing the disposition type exists to
    /// prevent.
    ///
    /// Perturbation: give the `OperatorTunable` arm of `take` a fallback value
    /// and this test fails. Watched under exactly that change.
    #[test]
    fn a_tunable_knob_with_no_value_refuses_by_name() {
        let mut knobs = all_frozen();
        knobs.repetition_window = Disposition::OperatorTunable;
        assert_eq!(
            knobs.resolve(&TunableValues::new()),
            Err(KnobRefusal::Unsupplied {
                knob: "repetition-window"
            })
        );
    }

    /// **A value supplied in the declaration reaches the engine's inputs**,
    /// per `weaver-spu-Spec` section 8, which is the point of the route: the
    /// values arrive with the instruction at admit and the engine takes them
    /// when the session opens. The frozen half is asserted beside it, because
    /// a route that moved a frozen value would take a deployment's lock away.
    ///
    /// Perturbation: have `Knobs::resolve` read its own frozen value for an
    /// `OperatorTunable` disposition and this test fails, the temperature
    /// reading 0.7 where the declaration said 0.2.
    #[test]
    fn a_declared_value_resolves_and_a_frozen_one_does_not_move() {
        let elections = Knobs {
            temperature: Disposition::OperatorTunable,
            top_k: Disposition::Frozen(40),
            top_p: Disposition::Frozen(0.95),
            repetition_penalty: Disposition::Frozen(1.1),
            repetition_window: Disposition::Frozen(64),
            seed: Disposition::Frozen(11),
        };
        let supplied: TunableValues = [
            ("temperature".to_string(), 0.2f64),
            // A name this binary froze, ignored where it appears.
            ("seed".to_string(), 99f64),
        ]
        .into_iter()
        .collect();
        let effective = elections.resolve(&supplied).expect("resolves");
        assert_eq!(
            effective.temperature, 0.2,
            "the declaration's value resolved"
        );
        assert_eq!(
            effective.seed, 11,
            "and a frozen parameter is not moved by a declaration naming it"
        );
    }

    /// **A count supplied as anything but a count refuses**, per
    /// `weaver-spu-Spec` section 8, judged before a cast that cannot fail.
    ///
    /// **The ceiling case is the one worth having.** `u64::MAX as f64` rounds
    /// up to `2^64`, so a bound written as the type's maximum would admit
    /// exactly `2^64` and then saturate it to `u64::MAX`, which is the
    /// substitution the check exists to refuse.
    ///
    /// Perturbation: relax `resolve_count`'s test to `value > ceiling` and the
    /// `2^64` case fails, the value resolving to `u64::MAX`. Drop the `fract`
    /// test and the fractional case fails. Watched under both.
    #[test]
    fn a_count_that_is_not_one_refuses() {
        let elections = SessionParameters {
            context_capacity: Disposition::OperatorTunable,
            max_tokens_per_turn: Disposition::Frozen(512),
        };
        for bad in [3.7f64, -1.0, 2f64.powi(32)] {
            let supplied: TunableValues = [("context-capacity".to_string(), bad)]
                .into_iter()
                .collect();
            assert_eq!(
                elections.resolve(&supplied),
                Err(KnobRefusal::NotACount {
                    knob: "context-capacity",
                    supplied: bad,
                }),
                "{bad} is not a count and the cast would have taken it"
            );
        }
        let good: TunableValues = [("context-capacity".to_string(), 8192.0)]
            .into_iter()
            .collect();
        assert_eq!(
            elections
                .resolve(&good)
                .expect("a count resolves")
                .context_capacity,
            8192,
            "and a count that is one is taken"
        );
    }

    /// The 64-bit ceiling is exclusive, which the 32-bit case cannot show.
    #[test]
    fn the_sixty_four_bit_ceiling_is_exclusive() {
        let elections = SessionParameters {
            context_capacity: Disposition::Frozen(4096),
            max_tokens_per_turn: Disposition::OperatorTunable,
        };
        let at_the_bound: TunableValues = [("max-tokens-per-turn".to_string(), 2f64.powi(64))]
            .into_iter()
            .collect();
        assert!(
            elections.resolve(&at_the_bound).is_err(),
            "2^64 is the value u64::MAX as f64 rounds to, and it is refused"
        );
    }

    /// **The Spec's own vectors**, per `weaver-spu-Spec` section 8.5. They
    /// are the specification's rather than this implementation's, computed
    /// independently of it, so a build disagreeing with one has a defect
    /// rather than a variation.
    ///
    /// Perturbation: change any constant, reorder the two mixes, or make
    /// the ordinal one-based, and every row fails.
    #[test]
    fn the_derivation_matches_the_specs_vectors() {
        let turn = |t: &str| weaver_types::TurnKey(t.to_string());
        assert_eq!(fnv1a_64(b"t-1"), 0x5627_0919_43b2_a601);
        assert_eq!(fnv1a_64(b"t-2"), 0x5627_0619_43b2_a0e8);
        for (declared, label, generation, expected) in [
            (0u64, "t-1", 0u64, 0x4587_63dd_2277_adccu64),
            (11, "t-1", 0, 0x6025_a13a_d2f4_a430),
            (11, "t-1", 1, 0x5144_c61d_6c67_c975),
            (11, "t-2", 0, 0x25f1_b675_aac3_c47d),
            (2_814_393_375, "t-2", 3, 0xcf4a_d3d2_e2e1_4630),
            (u64::MAX, "t-1", 0, 0x007b_e97c_ffce_ca04),
        ] {
            assert_eq!(
                derived_seed(declared, &turn(label), generation),
                expected,
                "declared {declared}, turn {label}, generation {generation}"
            );
        }
    }

    /// **Adjacent inputs draw distant streams**, which is why the finalizer
    /// is there at all: the ordinal is sequential and turn references
    /// differ by a character, so a derivation without avalanche would put
    /// neighbouring runs on neighbouring streams.
    #[test]
    fn adjacent_inputs_do_not_draw_adjacent_streams() {
        let turn = |t: &str| weaver_types::TurnKey(t.to_string());
        let base = derived_seed(11, &turn("t-1"), 0);
        let next_generation = derived_seed(11, &turn("t-1"), 1);
        let next_turn = derived_seed(11, &turn("t-2"), 0);
        assert_eq!((base ^ next_generation).count_ones(), 33);
        assert_eq!((base ^ next_turn).count_ones(), 31);
    }

    /// A second run of the same turn under the same declared seed derives
    /// the same value, which is the property the whole ruling exists for.
    #[test]
    fn the_same_three_inputs_derive_the_same_seed() {
        let turn = weaver_types::TurnKey("t-7".to_string());
        assert_eq!(
            derived_seed(2_814_393_375, &turn, 2),
            derived_seed(2_814_393_375, &turn, 2)
        );
        assert_ne!(
            derived_seed(2_814_393_375, &turn, 2),
            derived_seed(2_814_393_375, &turn, 3),
            "and a different generation of it does not"
        );
    }
}
