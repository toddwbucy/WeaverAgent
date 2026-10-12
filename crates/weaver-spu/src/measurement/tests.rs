/// **The election governs the vector and never the mean.** Charter
/// section 13.12 has the unelected posture be a smaller computation
/// rather than a withheld reading, and the way that stays true is that
/// the perplexity is identical either way: the terms are the same
/// terms, and only whether they are kept differs.
///
/// Perturbation: make `record` push the surprisal whatever the
/// election, and the unelected run reports a vector. Watched under
/// exactly that removal.
#[test]
fn the_election_keeps_the_vector_and_never_moves_the_mean() {
    let steps = [(1.0_f32, 2.0_f32), (3.0, 4.0), (5.0, 6.0)];

    let mut elected = Accumulator::new(true);
    let mut plain = Accumulator::new(false);
    for (entropy, surprisal) in steps {
        elected.record(entropy, surprisal);
        plain.record(entropy, surprisal);
    }
    let elected = elected.finish();
    let plain = plain.finish();

    assert!(
        elected.surprisal_bits.is_some(),
        "the elected run keeps the vector"
    );
    assert!(
        plain.surprisal_bits.is_none(),
        "and the unelected run keeps none, absent rather than empty"
    );
    assert_eq!(
        elected.entropy_bits, plain.entropy_bits,
        "the entropy carries no election and does not move"
    );
    assert_eq!(
        elected.perplexity, plain.perplexity,
        "and the mean is the same mean, the election having governed \
         what was kept rather than what was computed"
    );
    // 2 ** ((2 + 4 + 6) / 3) is 2 ** 4.
    assert_eq!(plain.perplexity, Some(16.0));
}

/// **A mean the wire cannot carry is absent rather than rendered.**
/// `serde_json` gives a non-finite float as `null`, so a member present
/// and carrying nothing is what an unguarded overflow would put on the
/// wire. An infinite surprisal is reachable by arithmetic rather than by
/// any path this crate can be talked into, which is why it is asserted
/// here rather than left to a reading of the code.
///
/// Perturbation: drop the `is_finite` filter in `finish` and the
/// infinite case answers `Some(inf)`. Watched under exactly that.
#[test]
fn an_unrepresentable_mean_is_absent() {
    let mut overflowed = Accumulator::new(false);
    overflowed.record(1.0, f32::INFINITY);
    overflowed.record(1.0, 2.0);
    assert!(
        overflowed.finish().perplexity.is_none(),
        "an infinite mean is not a number this crate hands the record"
    );

    let mut ordinary = Accumulator::new(false);
    ordinary.record(1.0, 2.0);
    let perplexity = ordinary.finish().perplexity.expect("a finite mean stands");
    assert!(perplexity.is_finite());
    assert_eq!(perplexity, 4.0);
}

/// **A generation that measured nothing renders no perplexity**, per
/// charter section 13.12: a mean needs terms, and a backend serving no
/// distribution supplies none. The scalar is absent on the one ground
/// the vectors are absent on rather than on a ground of its own.
#[test]
fn nothing_measured_reports_no_perplexity() {
    let nothing = Accumulator::new(true).finish();
    assert!(nothing.entropy_bits.is_none());
    assert!(nothing.surprisal_bits.is_none());
    assert!(
        nothing.perplexity.is_none(),
        "a mean of no terms is not a number this crate has"
    );

    let mut abandoned = Accumulator::new(true);
    abandoned.record(1.0, 2.0);
    abandoned.abandon();
    assert!(
        abandoned.finish().perplexity.is_none(),
        "and a broken pairing takes the mean with it, the vector it \
         summarizes being the one the record will not carry"
    );
}

use super::*;

/// **The stable form survives logits a naive exponential cannot.**
///
/// At 100 a naive `exp` is already infinite in `f32`, so `ln(sum(exp(l)))`
/// answers infinity and every signal downstream is a NaN. The stable form
/// answers a finite number, which is what section 6 says it must.
#[test]
fn a_logit_past_the_overflow_threshold_still_measures() {
    // Above the roughly-88 threshold section 6 names.
    let logits = [100.0f32, 99.0, 98.0];

    assert!(
        (100.0f32).exp().is_infinite(),
        "the naive form really does overflow here, which is why this matters"
    );

    let lse = log_sum_exp(&logits);
    assert!(
        lse.is_finite(),
        "the stable normaliser is finite, got {lse}"
    );

    let entropy = entropy_bits(&logits);
    assert!(entropy.is_finite() && entropy > 0.0, "got {entropy}");

    let surprisal = surprisal_bits(&logits, 0).expect("a finite surprisal");
    assert!(surprisal.is_finite(), "got {surprisal}");
}

/// The arithmetic is right, not merely finite. A uniform distribution over
/// four outcomes has entropy of exactly two bits, and each outcome carries
/// two bits of surprisal.
#[test]
fn a_uniform_distribution_measures_two_bits_over_four_outcomes() {
    let logits = [1.0f32; 4];
    assert!((entropy_bits(&logits) - 2.0).abs() < 1e-4);
    assert!((surprisal_bits(&logits, 2).expect("present") - 2.0).abs() < 1e-4);
}

/// **Two equal logits carry one bit however large they are.** The magnitude
/// is a property of the distribution's position, not of its shape, so the
/// answer at `f32::MAX` is the answer at 1.0.
///
/// This is the regression. The earlier form took `lse` in unshifted space
/// and then computed `probability * logit` and `lse - logit`: the first
/// overflows to infinity at `f32::MAX` and the second cancels to zero, so
/// both signals reported nothing where they should report a bit. Measured
/// at `0` entropy and `0` surprisal before the fix.
#[test]
fn equal_logits_carry_one_bit_at_any_magnitude() {
    for logits in [
        vec![f32::MAX, f32::MAX],
        vec![1e30f32, 1e30],
        vec![100.0f32, 100.0],
        vec![1.0f32, 1.0],
    ] {
        let entropy = entropy_bits(&logits);
        let surprisal = surprisal_bits(&logits, 0).expect("a finite surprisal");
        assert!(
            (entropy - 1.0).abs() < 1e-4,
            "entropy at {:?} is one bit, got {entropy}",
            logits[0]
        );
        assert!(
            (surprisal - 1.0).abs() < 1e-4,
            "surprisal at {:?} is one bit, got {surprisal}",
            logits[0]
        );
    }
}

/// A distribution with all its mass in one place carries no entropy, and
/// the token that takes the mass carries no surprise.
#[test]
fn a_certain_distribution_measures_nothing() {
    let logits = [80.0f32, -80.0, -80.0];
    assert!(entropy_bits(&logits) < 1e-3);
    assert!(surprisal_bits(&logits, 0).expect("present") < 1e-3);
}

/// **Absent is not empty.** A generation that produced no token answers
/// `None` on both vectors rather than a zero-length one.
///
/// Perturbation: change `Accumulator::finish` to wrap the vectors in
/// `Some` unconditionally and this fails, because a run that measured
/// nothing then reports having measured an empty run. Watched under
/// exactly that change.
#[test]
fn a_generation_that_measured_nothing_is_absent_rather_than_empty() {
    let signals = Accumulator::new(true).finish();
    assert_eq!(signals.entropy_bits, None);
    assert_eq!(signals.surprisal_bits, None);
    assert_eq!(signals.steps(), 0);

    let mut one = Accumulator::new(true);
    one.record(1.0, 2.0);
    let measured = one.finish();
    assert_eq!(measured.steps(), 1, "and one step is present, not absent");
}

/// **The partition covers the prompt exactly**, the last offset reaching
/// the rendered text's end.
///
/// Perturbation: drop the `last != text_len` check from
/// `PromptPartition::new` and this fails, the short partition being
/// accepted. Watched under exactly that removal.
#[test]
fn a_partition_short_of_the_end_is_refused() {
    let text_len = 11;
    assert!(PromptPartition::new(vec![4, 8, 11], text_len).is_ok());

    assert_eq!(
        PromptPartition::new(vec![4, 8, 10], text_len),
        Err(PartitionDefect::ShortOfEnd {
            last: 10,
            text_len: 11
        }),
        "a gap at the end is a consumer's guess, so it is refused here"
    );

    assert_eq!(
        PromptPartition::new(vec![4, 2, 11], text_len),
        Err(PartitionDefect::NotAscending { at: 1 })
    );
}

/// **The field ranks by probability and names the realized rank**, per
/// `weaver-spu-Spec` section 7.5. A uniform vector plus one peak has a
/// known order, so the ranking is checkable without depending on the
/// arithmetic the entropy tests already buy.
///
/// Perturbation: rank ascending instead of descending and the leader
/// assertion fails; drop the realized lookup and the rank does.
#[test]
fn the_field_ranks_and_names_the_draw() {
    let mut logits = vec![0.0f32; 32];
    logits[7] = 10.0;
    logits[3] = 5.0;
    logits[19] = 1.0;
    let (ranked, realized) = field(&logits, 3, 4).expect("a field");
    assert_eq!(ranked.len(), 4, "the depth's worth of candidates");
    assert_eq!(ranked[0].0, 7, "the peak leads");
    assert_eq!(ranked[1].0, 3, "then the second");
    assert_eq!(ranked[2].0, 19, "then the third");
    assert!(
        ranked[0].1 > ranked[1].1 && ranked[1].1 > ranked[2].1,
        "descending probability: {ranked:?}"
    );
    assert_eq!(realized, 1, "the drawn token's own rank");
    let total: f32 = ranked.iter().map(|(_, p)| p).sum();
    assert!(
        total > 0.0 && total < 1.0 + 1e-4,
        "probabilities, not logits: {total}"
    );
}

/// **A draw outside the reported depth reads as past it**, never as a
/// rank that was observed. The alternative, reporting rank zero or
/// omitting the member, would put a token at the front of a field it
/// was not in.
#[test]
fn a_draw_below_the_depth_reads_as_past_it() {
    let mut logits = vec![0.0f32; 32];
    for (at, value) in [(7, 10.0), (3, 5.0), (19, 1.0)] {
        logits[at] = value;
    }
    let (_, realized) = field(&logits, 11, 3).expect("a field");
    assert_eq!(realized, 3, "the depth itself, meaning past the report");
}

/// **A `NaN` among real logits is an absence, not a `NaN` reading.**
/// `f32::max` skips it in the fold, so the maximum is finite while the
/// sum is not, and every reading taken from that sum would be `NaN`
/// without the normaliser's second guard.
///
/// Perturbation: drop the `sum.is_finite()` guard and the surprisal
/// answers `Some(NaN)` and the field answers `NaN` probabilities, both
/// of which a consumer records as though they meant something.
#[test]
fn a_broken_distribution_is_absent_rather_than_nan() {
    let logits = vec![1.0f32, f32::NAN, 3.0, 2.0];
    assert!(
        logits
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
            .is_finite(),
        "the fixture's maximum is finite, which is what makes it the case"
    );
    assert!(surprisal_bits(&logits, 2).is_none(), "no surprisal");
    assert!(field(&logits, 2, 2).is_none(), "no field");
    assert_eq!(entropy_bits(&logits), 0.0, "and the entropy reports none");
}

/// **A depth past the vocabulary answers the vocabulary**, without
/// reserving the declared number of slots. The depth carries a floor at
/// the sampling cutoff and no ceiling, so this is the shape a stray
/// digit in a declaration takes, and it must be an ordinary answer
/// rather than an allocation.
#[test]
fn a_depth_past_the_vocabulary_answers_the_vocabulary() {
    let logits = vec![0.0f32, 3.0, 1.0, 2.0];
    let (ranked, realized) = field(&logits, 1, 1_000_000).expect("a field");
    assert_eq!(ranked.len(), logits.len(), "no more leaders than logits");
    assert_eq!(ranked[0].0, 1, "still ranked by probability");
    assert_eq!(realized, 0, "and the draw keeps its rank");
}

/// A depth of zero and an index outside the vector are absences rather
/// than empty fields, the rule the signals follow.
#[test]
fn the_field_is_absent_rather_than_empty() {
    let logits = vec![0.0f32, 1.0, 2.0];
    assert!(field(&logits, 0, 0).is_none(), "no depth, no field");
    assert!(field(&logits, 9, 2).is_none(), "no such draw, no field");
}
