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
