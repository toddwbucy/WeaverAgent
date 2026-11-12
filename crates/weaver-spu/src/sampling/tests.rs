use super::*;

/// The constants the composition root froze before the carrier, which the
/// identity of `weaver-spu-Spec` section 8 sets the defaults to.
fn carried() -> Sampling {
    Sampling {
        temperature: 0.7,
        top_k: 40,
        top_p: 0.95,
        repetition_penalty: 1.1,
        repetition_window: 64,
        seed: 11,
    }
}

/// **Each member outside its range refuses by name**, per `weaver-spu-Spec`
/// section 8's table, and each edge the table admits is admitted.
///
/// Perturbation: drop any one check from `Sampling::out_of_range` and its row
/// here fails, the out-of-range value judged in range. Watched under each.
#[test]
fn each_member_out_of_range_refuses_by_name_and_each_edge_is_served() {
    let refused = |change: fn(&mut Sampling), member: SamplingMember| {
        let mut sampling = carried();
        change(&mut sampling);
        assert_eq!(
            judge(&sampling, Some((512, 512)), None),
            Err(TokenRefusal::SamplingOutOfRange { member }),
            "{sampling:?} is out of range at {member:?}"
        );
    };
    refused(|s| s.temperature = -0.1, SamplingMember::Temperature);
    refused(|s| s.temperature = f32::NAN, SamplingMember::Temperature);
    refused(
        |s| s.temperature = f32::INFINITY,
        SamplingMember::Temperature,
    );
    refused(|s| s.top_k = 1 << 31, SamplingMember::TopK);
    refused(|s| s.top_p = 0.0, SamplingMember::TopP);
    refused(|s| s.top_p = 1.0001, SamplingMember::TopP);
    refused(|s| s.top_p = f32::NAN, SamplingMember::TopP);
    refused(
        |s| s.repetition_penalty = 0.0,
        SamplingMember::RepetitionPenalty,
    );
    refused(
        |s| s.repetition_penalty = f32::INFINITY,
        SamplingMember::RepetitionPenalty,
    );
    refused(
        |s| s.repetition_window = 1 << 31,
        SamplingMember::RepetitionWindow,
    );
    let served = |change: fn(&mut Sampling)| {
        let mut sampling = carried();
        change(&mut sampling);
        assert_eq!(
            judge(&sampling, Some((512, 512)), None),
            Ok(()),
            "{sampling:?} is in range"
        );
    };
    served(|s| s.temperature = 0.0);
    served(|s| s.top_k = 0);
    served(|s| s.top_k = (1 << 31) - 1);
    served(|s| s.top_p = 1.0);
    served(|s| s.repetition_penalty = 1.0);
    served(|s| s.repetition_window = 0);
    served(|s| s.seed = u64::MAX);
}

/// **The bound is the generation's and the ceiling the session's**: a bound
/// above the ceiling refuses, one at it or below is served, zero included,
/// and a re-feed carries none to judge.
///
/// Perturbation: compare with `>=` and the bound at the ceiling refuses.
/// Drop the comparison and the bound above it is served. Watched under both.
#[test]
fn a_bound_above_the_ceiling_refuses_and_one_at_it_is_served() {
    assert_eq!(
        judge(&carried(), Some((513, 512)), None),
        Err(TokenRefusal::SamplingOutOfRange {
            member: SamplingMember::MaxTokens
        })
    );
    assert_eq!(judge(&carried(), Some((512, 512)), None), Ok(()));
    assert_eq!(judge(&carried(), Some((0, 512)), None), Ok(()));
    assert_eq!(judge(&carried(), None, None), Ok(()));
}

/// **A field the cutoff hides refuses with both numbers**, per
/// `weaver-spu-Spec` section 7.5: a `top_k` above the depth, and a `top_k`
/// of zero, whose cutoff is the whole vocabulary. A depth at or past the
/// cutoff stands, and no election judges nothing.
///
/// Perturbation: compare the depth alone (`depth < top_k`) and the zero case
/// is served. Watched under exactly that change.
#[test]
fn a_field_below_the_cutoff_refuses_with_both_numbers_and_zero_among_them() {
    assert_eq!(
        judge(&carried(), Some((512, 512)), Some(20)),
        Err(TokenRefusal::FieldBelowCutoff {
            depth: 20,
            top_k: 40
        })
    );
    let mut zero = carried();
    zero.top_k = 0;
    assert_eq!(
        judge(&zero, Some((512, 512)), Some(20)),
        Err(TokenRefusal::FieldBelowCutoff {
            depth: 20,
            top_k: 0
        })
    );
    assert_eq!(judge(&zero, Some((512, 512)), None), Ok(()));
    assert_eq!(judge(&carried(), Some((512, 512)), Some(40)), Ok(()));
    assert_eq!(judge(&carried(), None, Some(64)), Ok(()));
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
