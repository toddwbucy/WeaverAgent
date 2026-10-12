use super::*;
use crate::decoder::session::{
    NeverCancels, PositionedSinks, SamplerBuild, Session, StopCondition,
};
use crate::sampling::{Disposition, Knobs, TunableValues};

/// A small instruct model that loads on the host in about a second.
const MODEL: &str = "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf";

fn frozen() -> EffectiveKnobs {
    Knobs {
        temperature: Disposition::Frozen(0.7),
        top_k: Disposition::Frozen(40),
        top_p: Disposition::Frozen(0.95),
        repetition_penalty: Disposition::Frozen(1.1),
        repetition_window: Disposition::Frozen(64),
        seed: Disposition::Frozen(11),
    }
    .resolve(&TunableValues::new())
    .expect("nothing is tunable")
}

fn engine(model: &ResidentModel) -> GgufEngine<'_> {
    GgufEngine::open(model, &frozen(), 512, false).expect("the context opens")
}

/// The fixture these tests decode against.
///
/// **An explicit request that cannot be met is a failure rather than a
/// skip.** A skip is a pass to the harness, so a green run on a machine
/// without the fixture reports that the engine was watched decoding when
/// nothing was decoded. The implicit skip survives only for the machine
/// that simply does not hold the fixture. This is the convention
/// `tests/loaded.rs` states and follows, and this file owed it.
///
/// **Its own variable rather than `WEAVER_TEST_GGUF`.** That one points the
/// load path at any small artifact, because that path is about loading and
/// not about the model. These tests read a Qwen vocabulary: the terminator
/// below is `<|im_end|>` at 151645, past the end of a smaller family's
/// vocabulary, so one variable serving both would answer a load-path
/// request by failing here as an engine error rather than as the mismatch
/// it is.
fn model_or_skip() -> Option<ResidentModel> {
    let path = match std::env::var_os("WEAVER_DECODE_GGUF") {
        Some(named) => {
            let path = std::path::PathBuf::from(named);
            // Both branches demand a regular file, since a directory at the
            // path would fail later and blame the loader.
            assert!(
                path.is_file(),
                "WEAVER_DECODE_GGUF names {}, which is not a regular file",
                path.display()
            );
            path
        }
        None => {
            let path = std::path::Path::new(MODEL);
            if !path.is_file() {
                eprintln!("SKIP: no model at {MODEL}");
                return None;
            }
            path.to_path_buf()
        }
    };
    Some(host_only(&path).expect("the model loads on the host"))
}

/// **An unelected engine taps nothing**, which is the absence the
/// backend's default `None` states and this is the GGUF engine honoring
/// it rather than answering an empty reduction. An empty reduction and no
/// reduction read the same to a consumer that only counts figures, and
/// they mean different things.
#[test]
fn an_unelected_engine_taps_nothing() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    engine
        .decode_at(&[TokenId(9707), TokenId(11), TokenId(1879)], 0)
        .expect("the prompt decodes");
    assert!(
        engine.take_reduction().is_none(),
        "an unelected load answers no reduction at all"
    );
}

/// **The tap folds one figure per layer, per decode, against real
/// weights**, per Spec section 7.
///
/// This is the end-to-end reading of the whole mechanism: the callback
/// installed at context creation, the name matched against what the
/// context formatted, the last column copied off a real buffer, and the
/// count checked against the model's own layer count rather than against
/// a number written here.
///
/// **The figures must vary.** A tap that read the wrong column, or read
/// an unwritten buffer, would answer the right count of the wrong thing,
/// and a run of identical or zero norms is what that looks like. The
/// residual grows through a transformer's depth, so figures that are
/// finite, positive, and not all equal is the cheapest reading that a
/// buffer of zeros and a constant both fail.
#[test]
fn the_elected_tap_folds_one_figure_per_layer() {
    let Some(model) = model_or_skip() else { return };
    let layers = model.model().n_layer() as usize;
    assert!(layers > 1, "the fixture must have layers to count");
    let mut engine = GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");

    engine
        .decode_at(&[TokenId(9707), TokenId(11), TokenId(1879)], 0)
        .expect("the prompt decodes");
    let first = engine.take_reduction().expect("an elected engine taps");
    assert_eq!(
        first.layers(),
        layers,
        "the reduction names the model's own layer count"
    );
    assert_eq!(first.forwards(), 1, "one decode, one forward");
    assert_eq!(first.figures(), layers, "and one figure per layer of it");
    let norms = first.per_layer_norm();
    assert!(
        norms.iter().all(|norm| norm.is_finite() && *norm > 0.0),
        "the figures are the model's: {norms:?}"
    );
    assert!(
        norms.iter().any(|norm| *norm != norms[0]),
        "a constant run means the wrong column: {norms:?}"
    );

    // **The drain empties.** A second decode folds its own set and the
    // reduction handed back holds that set alone, so a generation cannot
    // report figures belonging to the one before it.
    assert_eq!(
        engine.take_reduction().expect("still elected").figures(),
        0,
        "the drain left nothing behind"
    );
    engine
        .decode_at(&[TokenId(1879)], 3)
        .expect("a single token decodes");
    assert_eq!(
        engine.take_reduction().expect("still elected").figures(),
        layers,
        "the decode after the drain folds its own layers alone"
    );

    // **Two forwards into one reduction, which is where the old naming
    // lied.** With a single forward the figure count and the layer count
    // coincide, and every caller happened to be in that case. Draining
    // only after both decodes separates them.
    engine
        .decode_at(&[TokenId(1879)], 4)
        .expect("a token decodes");
    engine
        .decode_at(&[TokenId(1879)], 5)
        .expect("and another, undrained between");
    let both = engine.take_reduction().expect("still elected");
    assert_eq!(both.layers(), layers, "the layer count is the model's");
    assert_eq!(both.forwards(), 2, "two decodes, two forwards");
    assert_eq!(
        both.figures(),
        layers * 2,
        "and the product of the counts is the figure count"
    );
}

/// **A prefill longer than the micro-batch still folds one set.**
///
/// The callback fires per ubatch rather than per decode, so a prefill
/// that splits crosses it several times, and a tap that folded as it
/// went would answer a multiple of the layer count whose extra figures
/// belong to positions nothing sampled from. The staging discipline is
/// what makes this one set, and this is the test that reads it: the
/// default micro-batch is 512, so a prompt past that splits.
#[test]
fn a_split_prefill_folds_one_set_and_not_one_per_ubatch() {
    let Some(model) = model_or_skip() else { return };
    let layers = model.model().n_layer() as usize;
    let capacity = 2048;
    let mut engine =
        GgufEngine::open(&model, &frozen(), capacity, true).expect("the context opens");

    // Past the 512 micro-batch and inside the capacity above.
    let prompt: Vec<TokenId> = (0..600).map(|_| TokenId(1879)).collect();
    engine.decode_at(&prompt, 0).expect("the prompt decodes");

    assert_eq!(
        engine
            .take_reduction()
            .expect("an elected engine taps")
            .layers(),
        layers,
        "a split prefill folds the layer count once, not once per ubatch"
    );
}

/// **The figures are the last position's, whatever the batching.**
///
/// The count tests above would pass a tap reading any column, and the
/// column is the one thing about this tap that cannot be read off the
/// name. So this reads it by a route that has no column to choose: a
/// single-token decode has exactly one, and it must agree with the same
/// position taken as the tail of a batched prefill.
///
/// Perturbation: taking column zero, or `nb[1]` as the offset instead of
/// the stride, diverges the two here while every count stays right.
///
/// The tolerance is relative and loose because a batched matmul and a
/// single-row one accumulate in different orders on this backend. What
/// is being told apart is the same position from a different one, which
/// is a difference of tens of percent rather than of ulps.
#[test]
fn the_figures_belong_to_the_last_position_however_it_was_batched() {
    let Some(model) = model_or_skip() else { return };
    let prompt = [TokenId(9707), TokenId(11), TokenId(1879), TokenId(0)];

    let mut batched = GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");
    batched.decode_at(&prompt, 0).expect("the prompt decodes");
    let batched = batched
        .take_reduction()
        .expect("elected")
        .per_layer_norm()
        .to_vec();

    // The same last position, reached one token at a time, so the tap has
    // one column to take and cannot have taken the wrong one.
    let mut singly = GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");
    for (position, token) in prompt.iter().enumerate() {
        singly
            .decode_at(&[*token], position)
            .expect("a single token decodes");
        // Everything but the last is context, and its figures are not
        // what this compares.
        if position + 1 < prompt.len() {
            singly.take_reduction();
        }
    }
    let singly = singly
        .take_reduction()
        .expect("elected")
        .per_layer_norm()
        .to_vec();

    assert_eq!(
        batched.len(),
        singly.len(),
        "the same layer count either way"
    );
    for (layer, (a, b)) in batched.iter().zip(&singly).enumerate() {
        let apart = (a - b).abs() / a.max(*b);
        assert!(
            apart < 0.05,
            "layer {layer} reads {a} batched and {b} singly, \
             which is a different position rather than a different order"
        );
    }
}

/// **The tap changes no token**, watched on this backend.
///
/// `weaver-spu-PRD` section 13.7 obliges an elected readout to change no
/// token: the same declaration and the same seed produce the same token
/// sequence with the election on and off. **What this test buys and what
/// it does not**: it reads the property on the host backend, where the
/// scheduler has one backend and the windowing the callback forces costs
/// synchronisation and nothing else. The hazard the Spec names is a
/// device one, a fusion candidate straddling a window boundary going
/// unapplied and the unfused kernel not being bit-identical to the
/// fused, and no CPU run can reach it. The charter's bar is therefore a
/// measurement against the real artifact on the real device pair, taken
/// in `tests/readout_neutral.rs` and kept beside this rather than
/// replacing it: a failure there against a pass here localises a defect
/// to the device path immediately. What this buys is the whole of the rest: that the
/// tap mutates no engine state, consumes nothing from the sampler, and
/// perturbs no arithmetic that the host path shares with the device one.
///
/// Perturbation: making the tap accept the token it observed, or drawing
/// from the sampler inside the callback, diverges the two runs here.
#[test]
fn the_tap_changes_no_token_on_this_backend() {
    let Some(model) = model_or_skip() else { return };
    let prompt = [TokenId(9707), TokenId(11), TokenId(1879)];

    let draw = |readout: bool| {
        let mut engine =
            GgufEngine::open(&model, &frozen(), 512, readout).expect("the context opens");
        engine.decode_at(&prompt, 0).expect("the prompt decodes");
        let mut drawn = Vec::new();
        for step in 0..16 {
            let token = engine.sample().expect("a token is drawn");
            drawn.push(token);
            engine
                .decode_at(&[token], prompt.len() + step)
                .expect("the draw decodes");
        }
        drawn
    };

    let without = draw(false);
    let with = draw(true);
    assert_eq!(
        with, without,
        "the elected run drew a different sequence, so the tap is not observational"
    );
    // The comparison is only worth something if the elected run tapped.
    let mut elected = GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");
    elected.decode_at(&prompt, 0).expect("the prompt decodes");
    assert!(
        elected.take_reduction().is_some_and(|r| r.layers() > 0),
        "the elected run observed nothing, so the comparison is vacuous"
    );
}

/// **The engine decodes a prompt and draws a token from it.** The first
/// end-to-end exercise of this seam in the program: real weights, a real
/// forward, a real draw.
///
/// What it reads is that the distribution is the model's rather than a
/// buffer of zeros. A vocabulary-wide slice of equal values would satisfy a
/// length check and mean nothing, so the assertion is that the logits vary
/// and that the token drawn is inside the vocabulary.
#[test]
fn the_engine_decodes_and_draws() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);

    engine
        .decode_at(&[TokenId(9707), TokenId(11), TokenId(1879)], 0)
        .expect("the prompt decodes");

    // **The borrow ends before the draw, and the compiler is what says
    // so.** `distribution` borrows and `sample` needs the engine mutably,
    // so a reading held across the draw does not compile. That is the
    // pre-sampler ordering of Spec section 6 enforced by the seam's own
    // signatures rather than by a rule the caller remembers.
    let (width, varies, finite) = {
        let logits = engine.distribution().expect("a distribution stands");
        let first = logits[0];
        (
            logits.len(),
            logits.iter().any(|l| (l - first).abs() > 1e-3),
            logits.iter().all(|l| l.is_finite()),
        )
    };
    assert!(width > 1000, "a vocabulary, got {width}");
    assert!(varies, "the distribution varies rather than being zeroed");
    assert!(finite, "no logit is NaN");

    let token = engine.sample().expect("a token is drawn");
    assert!(
        (token.0 as usize) < width,
        "the draw is inside the vocabulary, got {token:?}"
    );
}

/// **A session over this engine generates.** The loop of `session.rs`
/// driving the engine of this module against real weights, which is the
/// whole of the decode path below the seam and the first time in this
/// program that it runs.
///
/// What it reads is that tokens come back, that the measurement is
/// positionally paired with them, and that the terminator landed: the
/// resident length exceeds what was decoded by the one slot the terminator
/// takes, which is the property section 4.2 holds on every path.
#[test]
fn a_session_over_the_engine_generates() {
    let Some(model) = model_or_skip() else { return };
    let mut session = Session::new(
        Box::new(engine(&model)),
        512,
        crate::decoder::backend::FlushMechanism::TruncateToPosition,
        false,
    );

    // A short identity prefix, then a turn's delta.
    session
        .open(&[TokenId(9707), TokenId(11)])
        .expect("the prefix goes resident");
    let before = session.resident_len();
    assert_eq!(before, 2, "the prefix is what was decoded");

    // The stream counter reads the real engine's callback: it fires once
    // per retained token, so the count equals the tokens the close carries.
    let mut streamed = 0usize;
    let generated = session
        .append_and_generate(
            &[TokenId(1879)],
            &StopCondition {
                stop_tokens: vec![],
                terminator: TokenId(151645),
                max_tokens: 8,
            },
            &mut NeverCancels,
            &mut |_| streamed += 1,
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the generation runs");

    assert_eq!(generated.tokens.len(), 8, "the ceiling bounds the run");
    assert_eq!(
        streamed,
        generated.tokens.len(),
        "the real engine streamed once per retained token"
    );
    assert_eq!(
        generated.signals.steps(),
        generated.tokens.len(),
        "one measurement per retained token, positionally paired"
    );

    // **The terminator landed.** Two prefix, one delta, eight generated,
    // one terminator.
    assert_eq!(
        session.resident_len(),
        2 + 1 + 8 + 1,
        "the terminator is resident beyond what was drawn"
    );

    let entropies = generated
        .signals
        .entropy_bits
        .as_ref()
        .expect("the run measured");
    assert!(
        entropies
            .iter()
            .all(|bits| bits.is_finite() && *bits >= 0.0),
        "every entropy is a finite non-negative number of bits"
    );
}

/// **Nothing may be read before a decode has left a distribution.**
///
/// Perturbation: return `get_logits_ith(0)` unconditionally rather than
/// keying on `logits_at` and this passes while reading a buffer no forward
/// filled, which measures as a uniform distribution over the vocabulary.
/// Watched under exactly that change.
#[test]
fn a_read_before_any_decode_refuses() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    assert!(engine.distribution().is_err(), "nothing has been decoded");
    assert!(engine.sample().is_err(), "and nothing may be drawn");
}

/// **A truncation takes the standing distribution with the positions it
/// removed**, so a draw afterwards cannot come from state that is gone.
///
/// Perturbation: leave `logits_at` set in `truncate_to` and this fails,
/// the sample succeeding against a position the cache no longer holds.
/// Watched under exactly that change.
#[test]
fn a_truncation_drops_the_standing_distribution() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    engine
        .decode_at(&[TokenId(9707), TokenId(11)], 0)
        .expect("decode");
    assert!(engine.distribution().is_ok(), "a distribution stands");

    engine.truncate_to(1).expect("the truncation runs");
    assert!(
        engine.distribution().is_err(),
        "the reading went with the positions"
    );
}

/// **A capacity of zero refuses rather than being substituted.**
///
/// `NonZeroU32::new(0)` is `None` and llama.cpp reads an absent `n_ctx` as
/// the model's own training context, so without the guard a caller asking
/// for nothing is served the whole window and the session above holds a
/// capacity the context does not have.
///
/// Perturbation: pass the capacity through unchecked and this fails, the
/// open succeeding and reporting a context far larger than was asked for.
/// Watched under exactly that change.
#[test]
fn a_zero_capacity_refuses() {
    let Some(model) = model_or_skip() else { return };
    assert!(
        GgufEngine::open(&model, &frozen(), 0, false).is_err(),
        "a session that can hold nothing is refused at the open"
    );
    assert!(
        GgufEngine::open(&model, &frozen(), 1, false).is_ok(),
        "a positive capacity still opens"
    );
}

/// **A token the engine cannot represent refuses by name.** `sample`
/// already refuses a value it cannot represent, and a `TokenId` is a
/// `u32`, so the inbound half owes the same posture.
///
/// **The refusal is read rather than the failure**, because both the guard
/// and its absence end in a refusal and only the reason separates them.
/// Wrapping to a negative id fails later inside llama.cpp's batch, so an
/// `is_err` assertion passes with the guard gone and watches nothing.
///
/// Perturbation: restore `token.0 as i32` and this fails, the refusal
/// arriving as `batch add` from the engine rather than naming the token.
/// Watched under exactly that change.
#[test]
fn an_unrepresentable_token_refuses_by_name() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    let refusal = engine
        .decode_at(&[TokenId(u32::MAX)], 0)
        .expect_err("a token above i32::MAX is refused");
    let DecodeFault::Engine { detail } = refusal else {
        panic!("the refusal is an engine fault, got {refusal:?}");
    };
    assert!(
        detail.contains("the token is outside the engine's range"),
        "the refusal names the token rather than the batch, got {detail:?}"
    );
}

/// **Two seeds differing only above bit 31 reach the draw as two
/// seeds.** This sampler takes a `u32` where the derivation answers a
/// `u64`, so a plain truncation would make `0x0000_0000_0000_0001` and
/// `0xFFFF_FFFF_0000_0001` one seed - and the native path, which
/// consumes the whole `u64`, would disagree with it. The fold is what
/// keeps the upper half in the value that draws.
///
/// Read against the arithmetic rather than against a device, so it
/// runs without a model: what is under test is the narrowing, and the
/// narrowing is a pure function.
///
/// Perturbation: write `seed as u32` and the two collapse.
#[test]
fn both_halves_of_the_derived_seed_reach_the_draw() {
    let narrow = |seed: u64| ((seed >> 32) ^ (seed & 0xFFFF_FFFF)) as u32;
    assert_ne!(
        narrow(0x0000_0000_0000_0001),
        narrow(0xFFFF_FFFF_0000_0001),
        "seeds differing only above bit 31 must not collapse"
    );
    // The derivation's own neighbours, which is where this matters.
    let turn = weaver_types::TurnKey("t-1".to_string());
    let a = crate::sampling::derived_seed(11, &turn, 0);
    let b = crate::sampling::derived_seed(11, &turn, 1);
    assert_ne!(narrow(a), narrow(b), "adjacent generations stay apart");
    // And the fold is not lossy in the ordinary direction: a value
    // whose upper half is zero passes through unchanged, so nothing
    // that worked before this act now narrows differently.
    assert_eq!(narrow(0x0000_0000_DEAD_BEEF), 0xDEAD_BEEF);
}

/// **A reseed restores the window from the tail it is handed**, per
/// `weaver-spu-Spec` section 8.5, which is what makes a flush harmless
/// to sampling rather than something a flush has to repair.
///
/// What this reads is that the seed still means something across a
/// rewrite: the same resident state, reseeded with the same seed and
/// the same window, draws the same token. It replaces a test that read
/// the same property through `forget_sampled`, which delivered it by
/// clearing the window and reseeding the draw together - the coupling
/// that ruling retires.
///
/// Perturbation: drop the `accept_many` from `reseed` and this fails,
/// the second draw seeing an empty window where the first saw a full
/// one.
#[test]
fn a_reseed_restores_the_window_it_is_handed() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    let prefix = [TokenId(9707), TokenId(11), TokenId(1879)];
    let window = [TokenId(9707), TokenId(11)];

    engine.decode_at(&prefix, 0).expect("the prefix decodes");
    engine.reseed(4242, &window).expect("the first reseed");
    let first = engine.sample().expect("a token is drawn");

    // Rewrite the cache and come back to the same resident state.
    engine.reestablish().expect("the rewrite runs");
    engine
        .decode_at(&prefix, 0)
        .expect("the prefix decodes again");
    engine.reseed(4242, &window).expect("the second reseed");
    let after = engine.sample().expect("a token is drawn");

    assert_eq!(
        first, after,
        "one seed and one window draw one token, whatever happened to \
         the cache between them"
    );
}

/// **A closed engine answers every ask with a refusal**, rather than a
/// stale value from the state it held.
#[test]
fn a_closed_engine_serves_nothing() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    engine.decode_at(&[TokenId(9707)], 0).expect("decode");
    engine.close();

    assert!(engine.distribution().is_err());
    assert!(engine.sample().is_err());
    assert!(engine.decode_at(&[TokenId(11)], 1).is_err());
    assert!(engine.truncate_to(0).is_err());
    assert!(engine.reestablish().is_err());
}
