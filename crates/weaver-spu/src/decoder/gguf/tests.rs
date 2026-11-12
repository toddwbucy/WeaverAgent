use super::*;
use crate::decoder::session::{
    NeverCancels, PositionedSinks, SamplerBuild, Session, StopCondition,
};
use crate::sampling::Sampling;

/// A small instruct model that loads on the host in about a second.
const MODEL: &str = "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf";

/// The values a generation carries in these tests: the constants the
/// deploy scripts write into a new declaration.
fn frozen() -> Sampling {
    Sampling {
        temperature: 0.7,
        top_k: 40,
        top_p: 0.95,
        repetition_penalty: 1.1,
        repetition_window: 64,
        seed: 11,
    }
}

fn engine(model: &ResidentModel) -> GgufEngine<'_> {
    GgufEngine::open(model, 512, false).expect("the context opens")
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
    let mut engine = GgufEngine::open(&model, 512, true).expect("the context opens");

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
    let mut engine = GgufEngine::open(&model, capacity, true).expect("the context opens");

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

    let mut batched = GgufEngine::open(&model, 512, true).expect("the context opens");
    batched.decode_at(&prompt, 0).expect("the prompt decodes");
    let batched = batched
        .take_reduction()
        .expect("elected")
        .per_layer_norm()
        .to_vec();

    // The same last position, reached one token at a time, so the tap has
    // one column to take and cannot have taken the wrong one.
    let mut singly = GgufEngine::open(&model, 512, true).expect("the context opens");
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
        let mut engine = GgufEngine::open(&model, 512, readout).expect("the context opens");
        engine.decode_at(&prompt, 0).expect("the prompt decodes");
        engine.reseed(11, &frozen(), &[]).expect("the reseed");
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
    let mut elected = GgufEngine::open(&model, 512, true).expect("the context opens");
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

    engine.reseed(11, &frozen(), &[]).expect("the reseed");
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
                sampling: frozen(),
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

/// **Nothing may be drawn before a generation has built the sampler**, per
/// `weaver-spu-Spec` section 8: the engine opens with no sampler, since no
/// sampling value reaches it at open, and the first reseed builds the whole
/// chain from the values the generation carried.
///
/// Perturbation: build a chain at open from any fixed values and this
/// fails, the draw succeeding with values no generation carried.
#[test]
fn a_draw_before_any_reseed_refuses() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    engine
        .decode_at(&[TokenId(9707), TokenId(11)], 0)
        .expect("decode");
    assert!(engine.distribution().is_ok(), "a distribution stands");
    assert!(
        engine.sample().is_err(),
        "no generation has carried the values to draw with"
    );
    engine.reseed(11, &frozen(), &[]).expect("the reseed");
    assert!(engine.sample().is_ok(), "the carried values build the draw");
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
        GgufEngine::open(&model, 0, false).is_err(),
        "a session that can hold nothing is refused at the open"
    );
    assert!(
        GgufEngine::open(&model, 1, false).is_ok(),
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
    engine
        .reseed(4242, &frozen(), &window)
        .expect("the first reseed");
    let first = engine.sample().expect("a token is drawn");

    // Rewrite the cache and come back to the same resident state.
    engine.reestablish().expect("the rewrite runs");
    engine
        .decode_at(&prefix, 0)
        .expect("the prefix decodes again");
    engine
        .reseed(4242, &frozen(), &window)
        .expect("the second reseed");
    let after = engine.sample().expect("a token is drawn");

    assert_eq!(
        first, after,
        "one seed and one window draw one token, whatever happened to \
         the cache between them"
    );
}

/// Values that make the penalty window decide the draw: a two-slot window,
/// a penalty that sinks any token in it, and a top-k of one, so each draw
/// is the best token the window leaves standing and no luck enters.
fn window_decides() -> Sampling {
    Sampling {
        temperature: 0.7,
        top_k: 1,
        top_p: 0.95,
        repetition_penalty: 1.0e4,
        repetition_window: 2,
        seed: 11,
    }
}

/// The engine the window tests read, the prompt decoded, and the three best
/// tokens of the distribution it leaves standing, best first.
///
/// **The ranking is checked before it is trusted.** The four best logits
/// must be strictly ordered, since the engine's top-k breaks a tie in an
/// order this ranking does not know, and the third must outrank a
/// penalised best: a positive logit is divided by the penalty and a
/// negative one multiplied, so a penalised token lands at or below
/// `best / 1e4`, and the window rather than the logits then decides which
/// of the three a draw takes.
fn window_fixture(model: &ResidentModel) -> (GgufEngine<'_>, [TokenId; 3]) {
    let mut engine = GgufEngine::open(model, 512, false).expect("the context opens");
    engine
        .decode_at(&[TokenId(9707), TokenId(11), TokenId(1879)], 0)
        .expect("the prompt decodes");
    let best = {
        let logits = engine.distribution().expect("a distribution stands");
        let mut ranked: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        assert!(
            ranked[0].1 > ranked[1].1 && ranked[1].1 > ranked[2].1 && ranked[2].1 > ranked[3].1,
            "the four best logits must be strictly ordered: {:?}",
            &ranked[..4]
        );
        assert!(
            ranked[2].1 > ranked[0].1.max(0.0) / 1.0e4,
            "the third logit {} must outrank a penalised best {}",
            ranked[2].1,
            ranked[0].1
        );
        [0, 1, 2].map(|i| TokenId(ranked[i].0 as u32))
    };
    (engine, best)
}

impl GgufEngine<'_> {
    /// Which of three tokens the chain draws from logits that rank them
    /// first, second and third, every other token at zero, which is a
    /// reading of the penalty window the model's distribution cannot
    /// decide.
    ///
    /// Under [`window_decides`] a token in the window falls from its logit
    /// to a ten-thousandth of it, still above every zero, and the top-k of
    /// one takes the best that stands. So the answer is the first of the
    /// three outside the window, or the first of them where all three are
    /// in it. The probe goes through the draw `sample` itself makes, and
    /// accepts nothing, so it does not move the window it reads.
    fn probe(&mut self, ranked: [TokenId; 3]) -> TokenId {
        let width = self.distribution().expect("a distribution stands").len();
        let mut logits = vec![0.0_f32; width];
        for (token, logit) in ranked.iter().zip([3.0, 2.0, 1.0]) {
            logits[token.0 as usize] = logit;
        }
        let sampler = self.sampler.as_ref().expect("a reseed built the sampler");
        GgufEngine::draw(sampler, &mut self.candidates, &logits).expect("the probe draws")
    }
}

/// **Each decoded token enters the penalty window once**, per
/// `weaver-spu-Spec` section 8.5: the window is the last
/// `repetition_window` resident positions, and on GGUF the decode is
/// what accepts a token into the chain.
///
/// The reseed puts the best token in the window, so the draw takes the
/// second best, and the draw is decoded as a generation decodes it. The
/// probe then ranks the best, the second and the third. Counted once, the
/// two-slot window holds the best and the second, and the probe answers
/// the third. Counted twice, the draw fills both slots and pushes the best
/// out, and the probe answers the best. Not counted, only the best stands
/// in the window, and the probe answers the second.
///
/// Perturbation: draw through the engine's own sample in
/// `GgufEngine::sample` (`self.sampler.sample(&self.context, at)`, which
/// accepts the draw before the decode accepts it again) and the probe
/// answers the best. Drop the accept from `GgufEngine::decode_at` and it
/// answers the second.
#[test]
fn each_decoded_token_enters_the_penalty_window_once() {
    let Some(model) = model_or_skip() else { return };
    let (mut engine, [best, second, third]) = window_fixture(&model);

    engine
        .reseed(4242, &window_decides(), &[best])
        .expect("the window holds the best");
    let draw = engine.sample().expect("a token is drawn");
    assert_eq!(
        draw, second,
        "the window sinks the best, so the second is drawn"
    );
    engine.decode_at(&[draw], 3).expect("the draw decodes");
    assert_eq!(
        engine.probe([best, second, third]),
        third,
        "counted once, the window holds the best and the draw, so the probe \
         answers the third. The best ({best:?}) means the draw was counted \
         twice, and the second ({second:?}) that it was not counted"
    );
}

/// **A re-feed's window holds the recorded token and not the draw**, per
/// `weaver-spu-Spec` sections 4.6 and 8.5: the drive draws at each
/// position and then decodes the recorded token, and what is decoded is
/// what becomes resident and what enters the window. The native path
/// reads its window from the resident tail, so this is also where the two
/// engines meet on a re-fed path.
///
/// The reseed leaves the window empty, so the draw takes the best token.
/// The record departs from it: the third best is decoded in its place.
/// The probe then ranks the recorded token, the draw and the second.
/// Holding the recorded token alone, the window sinks it, and the probe
/// answers the draw. Holding the draw alone, or nothing, the probe answers
/// the recorded token. Holding both, it answers the second.
///
/// Perturbation: draw through the engine's own sample in
/// `GgufEngine::sample` and drop the accept from `GgufEngine::decode_at`,
/// which is the engine before this ruling, and the probe answers the
/// recorded token.
#[test]
fn a_refeed_window_holds_the_recorded_token_and_not_the_draw() {
    let Some(model) = model_or_skip() else { return };
    let (mut engine, [best, second, third]) = window_fixture(&model);

    engine
        .reseed(4242, &window_decides(), &[])
        .expect("the window is empty");
    let draw = engine.sample().expect("a token is drawn");
    assert_eq!(
        draw, best,
        "nothing stands in the window, so the best is drawn"
    );
    let recorded = third;
    engine
        .decode_at(&[recorded], 3)
        .expect("the recorded token decodes");
    assert_eq!(
        engine.probe([recorded, draw, second]),
        draw,
        "the window holds the recorded token, so the probe answers the draw. \
         The recorded token ({recorded:?}) means the window holds the draw or \
         nothing, and the second ({second:?}) that it holds both"
    );
}

/// **The crate's draw is the engine's own draw** wherever the decoded
/// token is the draw, which is every generation. `sample` builds the
/// candidate array that `llama_sampler_sample` builds and leaves out only
/// its accept, and `decode_at` accepts the same token one call later, so
/// a generation draws what it drew before the re-feed ruling of
/// 2026-11-11. This is the watch that holds the two together if a later
/// engine changes how its sample builds the array.
///
/// One seed and one prompt drive two contexts of one model. The first
/// runs a generation's own calls: reseed with the tail, then draw and
/// decode. The twin draws through `llama_sampler_sample` on a chain built
/// from the same values and seed and restored from the same tail, decoding
/// the same token. The two answer the same token at every position, and
/// the same distribution bit for bit, which carries every signal the
/// session measures from it. The run is longer than the window.
///
/// **Two declarations, the frozen penalty on and the penalty off.** A
/// penalty that acts marks the array unsorted after it, so with the penalty
/// on the array's own sorted flag never reaches top-k. With the penalty at
/// 1.0 the penalty sampler returns at once and the flag the crate built
/// reaches top-k as built, so the second declaration is what reads it.
///
/// Perturbation: drop the accept from `GgufEngine::decode_at` and the
/// first declaration's draws part. Mark the candidate array sorted in
/// `GgufEngine::draw` and the second's do.
#[test]
fn the_crates_draw_is_the_engines_own_draw() {
    let Some(model) = model_or_skip() else { return };
    let on = frozen();
    let off = Sampling {
        repetition_penalty: 1.0,
        ..on
    };
    for (sampling, declaration) in [(on, "the penalty on"), (off, "the penalty off")] {
        lockstep_with_the_engine(&model, &sampling, declaration);
    }
}

/// One declaration of [`the_crates_draw_is_the_engines_own_draw`].
fn lockstep_with_the_engine(model: &ResidentModel, sampling: &Sampling, declaration: &str) {
    let steps = sampling.repetition_window as usize + 32;
    let prompt = [TokenId(9707), TokenId(11), TokenId(1879)];
    let seed = 4242;

    let mut ours = GgufEngine::open(model, 512, false).expect("the context opens");
    ours.decode_at(&prompt, 0).expect("the prompt decodes");
    ours.reseed(seed, sampling, &prompt).expect("the reseed");

    let mut twin = GgufEngine::open(model, 512, false).expect("the twin opens");
    twin.decode_at(&prompt, 0)
        .expect("the twin's prompt decodes");
    let mut theirs = GgufEngine::chain(sampling, seed);
    theirs.accept_many(prompt.iter().map(|token| LlamaToken(token.0 as i32)));

    for step in 0..steps {
        let same = {
            let a = ours.distribution().expect("a distribution stands");
            let b = twin.distribution().expect("the twin's distribution stands");
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
        };
        assert!(
            same,
            "with {declaration}, the two distributions differ at step {step}"
        );
        let at = twin.logits_at.expect("the twin's logits stand");
        let engines = theirs.sample(&twin.context, at);
        let crates = ours.sample().expect("a token is drawn");
        assert_eq!(
            crates.0 as i32, engines.0,
            "with {declaration}, the crate's draw departs from the engine's own at step {step}"
        );
        ours.decode_at(&[crates], prompt.len() + step)
            .expect("the draw decodes");
        twin.decode_at(&[crates], prompt.len() + step)
            .expect("the twin decodes the same token");
    }
}

/// **A reseed's tail enters the window once per position**, the same
/// count as a decode, so a window restored from the tail and a window built
/// by decoding hold the same positions.
///
/// The reseed hands a two-token tail to a two-slot window. Counted once,
/// both stay and the draw takes the third best. Counted twice, the second
/// token fills both slots and pushes the first out.
///
/// Perturbation: enter each token of the window twice in
/// `GgufEngine::reseed` and the draw answers the best token.
#[test]
fn a_reseed_tail_enters_the_window_once_per_position() {
    let Some(model) = model_or_skip() else { return };
    let (mut engine, [best, second, third]) = window_fixture(&model);

    engine
        .reseed(4242, &window_decides(), &[best, second])
        .expect("the window holds the best two");
    assert_eq!(
        engine.sample().expect("a token is drawn"),
        third,
        "both tail positions stand in the window, so the third is drawn"
    );
}

/// **Every value this crate sets but flash attention is the engine's own
/// default**, per `weaver-spu-Spec` sections 3.1.1 and 3.1.2, so setting it
/// leaves the parameters the engine is handed as they were, whatever a
/// fixture can observe of it. Flash attention is the one set that changes
/// them, from AUTO to enabled, and
/// [`the_engine_settings_move_no_draw`] reads that one in lockstep.
///
/// The defaults are read from the engine's own constructors at the pinned
/// engine, so an engine whose default moves fails here, and the act that
/// moves the pin measures the new value before it sets it.
///
/// Perturbation: set `n_ubatch` 256 in `engine_settings`, or `use_mmap` off
/// in `placement_params`, and this test fails.
#[test]
fn every_set_value_but_flash_attention_is_the_engines_default() {
    let engines = LlamaContextParams::default();
    let ours = engine_settings(LlamaContextParams::default());
    assert_eq!(ours.n_ubatch(), engines.n_ubatch(), "n_ubatch");
    assert_eq!(ours.type_k(), engines.type_k(), "type_k");
    assert_eq!(ours.type_v(), engines.type_v(), "type_v");
    assert_eq!(ours.offload_kqv(), engines.offload_kqv(), "offload_kqv");
    assert_eq!(ours.op_offload(), engines.op_offload(), "op_offload");
    assert_eq!(ours.swa_full(), engines.swa_full(), "swa_full");
    assert_eq!(ours.kv_unified(), engines.kv_unified(), "kv_unified");
    assert_eq!(ours.n_threads(), engines.n_threads(), "n_threads");
    assert_eq!(
        ours.n_threads_batch(),
        engines.n_threads_batch(),
        "n_threads_batch"
    );
    assert_eq!(
        engines.flash_attention_policy(),
        llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_AUTO,
        "the engine's default is AUTO, which the record cannot state"
    );
    assert_eq!(
        ours.flash_attention_policy(),
        llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_ENABLED,
        "flash attention is set enabled, never auto"
    );

    let models = LlamaModelParams::default();
    for devices in [1, 2] {
        let placed = placement_params(&Division::even(64, devices).expect("a division"))
            .expect("parameters");
        assert_eq!(placed.use_mmap(), models.use_mmap(), "use_mmap");
        assert_eq!(placed.use_mlock(), models.use_mlock(), "use_mlock");
    }
}

/// **The settings this crate sets move no draw**, per `weaver-spu-Spec`
/// section 3.1. Flash attention is the one set that hands the engine a
/// different value, enabled where its default is AUTO, so this reads that
/// set where it can bear on the logits. The others are the engine's defaults
/// ([`every_set_value_but_flash_attention_is_the_engines_default`]).
///
/// Read on the host lane, in lockstep: two contexts of one model, one opened
/// as every serving open is and one with the engine's defaults left in
/// place, each reseeded with one seed and the prompt as its tail, each draw
/// decoded into both. At every position the two distributions agree bit for
/// bit, which carries every signal the session measures from them, and the
/// two draws agree. The run is longer than the window, under the frozen
/// penalty. A device load goes through the admission, which opens with this
/// crate's settings only, so the device lanes are read by the instrument
/// `tests/engine_settings_identity.rs`, run on the tree before the act and
/// the tree after it.
///
/// Perturbation: set flash attention disabled in `engine_settings`, or the K
/// cache to q8_0, and the two distributions part at the first position.
#[test]
fn the_engine_settings_move_no_draw() {
    let Some(model) = model_or_skip() else { return };
    let sampling = frozen();
    let steps = sampling.repetition_window as usize + 32;
    let prompt = [TokenId(9707), TokenId(11), TokenId(1879)];

    let mut set = GgufEngine::open(&model, 512, false).expect("the context opens");
    let mut defaults = GgufEngine::open_with(&model, 512, false, |params| params)
        .expect("the context opens with the engine's defaults");
    for engine in [&mut set, &mut defaults] {
        engine.decode_at(&prompt, 0).expect("the prompt decodes");
        engine.reseed(4242, &sampling, &prompt).expect("the reseed");
    }

    for step in 0..steps {
        let same = {
            let a = set.distribution().expect("a distribution stands");
            let b = defaults.distribution().expect("a distribution stands");
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
        };
        assert!(same, "the two distributions differ at step {step}");
        let drawn = set.sample().expect("a token is drawn");
        assert_eq!(
            drawn,
            defaults.sample().expect("a token is drawn"),
            "the two draws differ at step {step}"
        );
        for engine in [&mut set, &mut defaults] {
            engine
                .decode_at(&[drawn], prompt.len() + step)
                .expect("the draw decodes");
        }
    }
}

/// **A closed engine answers every ask with a refusal**, rather than a
/// stale value from the state it held.
#[test]
fn a_closed_engine_serves_nothing() {
    let Some(model) = model_or_skip() else { return };
    let mut engine = engine(&model);
    engine.decode_at(&[TokenId(9707)], 0).expect("decode");
    engine.reseed(11, &frozen(), &[]).expect("the reseed");
    engine.close();

    assert!(engine.distribution().is_err());
    assert!(engine.sample().is_err());
    assert!(engine.decode_at(&[TokenId(11)], 1).is_err());
    assert!(engine.truncate_to(0).is_err());
    assert!(engine.reestablish().is_err());
}

/// **The split reaches the engine**, per the device pair amendment PAIR-3: the
/// parameters built for a pair carry layer split mode, the division's tensor
/// split zero-padded to the engine's device limit, and every layer offloaded
/// (at least L + 1), so the engine maps the split as the division computed.
/// A single device carries split mode none and no split.
///
/// Perturbation: leave the tensor split unset, or pass proportional halves,
/// and this test fails.
#[test]
fn the_parameters_for_a_pair_carry_the_division() {
    let pair = Division::even(64, 2).expect("a division");
    let params = placement_params(&pair).expect("parameters");
    assert_eq!(params.split_mode().ok(), Some(LlamaSplitMode::Layer));
    let split = params.tensor_split();
    assert_eq!(&split[..2], [31.5, 33.5]);
    assert!(split[2..].iter().all(|v| *v == 0.0));
    assert!(params.n_gpu_layers() >= 65);

    let single = placement_params(&Division::even(64, 1).expect("a division")).expect("parameters");
    assert_eq!(single.split_mode().ok(), Some(LlamaSplitMode::None));
    assert!(single.tensor_split().is_empty());
    assert_eq!(single.main_gpu(), 0);
}

/// **The record's split is the split the loader was handed**, per
/// `weaver-spu-Spec` section 3.1.1's `set.tensor_split`: the wrapper reads
/// no split back, so `gather` is handed [`handed_split`], the one function
/// [`placement_params`] hands the loader. It is the division's values across
/// several devices and the empty list on one.
///
/// Perturbation: hand the record the division's values on one device too,
/// and this fails.
#[test]
fn the_record_states_the_split_the_loader_was_handed() {
    for devices in [1, 2, 3] {
        let division = Division::even(64, devices).expect("a division");
        let handed = handed_split(&division);
        let params = placement_params(&division).expect("parameters");
        assert_eq!(&params.tensor_split()[..handed.len()], handed.as_slice());
        assert!(
            params.tensor_split()[handed.len()..]
                .iter()
                .all(|v| *v == 0.0)
        );
        assert_eq!(handed.is_empty(), devices == 1, "{devices} devices");
    }
}

/// **The host residency renders both theta blocks, closed, from what the
/// engine resolved**, per `weaver-spu-Spec` sections 3.1.1 and 3.1.2. The
/// order and the writing are pinned by `theta`'s own tests. This reads the
/// values on a real context: the set values are `engine_settings`', the
/// read-backs the context's and the model's own, the metadata the model's
/// RoPE keys in key order, and each mapped engine library digested.
///
/// Perturbation: render `rope_type` as `null`, or gather the metadata of
/// every key, and this fails.
#[test]
fn the_host_residency_renders_both_theta_blocks_from_the_engine() {
    let Some(model) = model_or_skip() else { return };
    let engine = engine(&model);
    let blocks = render_blocks(&model, &engine, FlushMechanism::TruncateToPosition)
        .expect("the blocks render");
    let block: serde_json::Value = serde_json::from_str(blocks.engine.get()).expect("JSON");
    let device: serde_json::Value = serde_json::from_str(blocks.device.get()).expect("JSON");
    let closed = |value: &serde_json::Value, keys: &[&str], what: &str| {
        let mut have: Vec<&str> = value
            .as_object()
            .unwrap_or_else(|| panic!("{what} is an object"))
            .keys()
            .map(String::as_str)
            .collect();
        let mut want = keys.to_vec();
        have.sort_unstable();
        want.sort_unstable();
        assert_eq!(have, want, "{what}");
    };
    closed(
        &block,
        &[
            "backend",
            "mechanism",
            "flush",
            "build",
            "read_back",
            "set",
            "metadata",
        ],
        "engine",
    );
    closed(
        &block["build"],
        &["ggml_version", "ggml_commit", "libraries"],
        "build",
    );
    closed(
        &block["read_back"],
        &[
            "n_ctx",
            "n_batch",
            "n_ubatch",
            "n_layer",
            "n_ctx_train",
            "rope_type",
            "file_type",
        ],
        "read_back",
    );
    closed(
        &block["set"],
        &[
            "n_ubatch",
            "type_k",
            "type_v",
            "flash_attn",
            "offload_kqv",
            "op_offload",
            "swa_full",
            "kv_unified",
            "n_gpu_layers",
            "split_mode",
            "main_gpu",
            "tensor_split",
        ],
        "set",
    );
    closed(
        &device,
        &[
            "n_threads",
            "n_threads_batch",
            "use_mmap",
            "use_mlock",
            "mmap_supported",
            "mlock_supported",
            "headroom",
            "cuda_device_order",
            "devices",
        ],
        "device",
    );

    assert_eq!(block["backend"], "gguf");
    assert_eq!(block["mechanism"], "gguf-sampler-2");
    assert_eq!(block["flush"], "truncate-to-position");
    assert!(
        block["build"]["ggml_version"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    assert!(
        block["build"]["ggml_commit"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );
    let libraries = block["build"]["libraries"].as_array().expect("a list");
    assert!(!libraries.is_empty(), "the engine is linked dynamically");
    let mut previous = String::new();
    for library in libraries {
        closed(library, &["path", "sha256"], "a library");
        let path = library["path"].as_str().expect("a path");
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a file name");
        assert!(
            name.starts_with("libllama") || name.starts_with("libggml"),
            "{path}"
        );
        assert!(path > previous.as_str(), "path order: {path}");
        previous = path.to_owned();
        let digest = library["sha256"].as_str().expect("a digest");
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
    }

    let read_back = &block["read_back"];
    assert_eq!(read_back["n_ctx"], u64::from(engine.context.n_ctx()));
    assert_eq!(read_back["n_batch"], u64::from(engine.context.n_batch()));
    assert_eq!(read_back["n_ubatch"], 512);
    assert_eq!(read_back["n_layer"], u64::from(model.model.n_layer()));
    assert_eq!(
        read_back["n_ctx_train"],
        u64::from(model.model.n_ctx_train())
    );
    assert_eq!(read_back["rope_type"], "neox", "qwen2 rotates NeoX-style");
    assert_eq!(
        read_back["file_type"],
        model
            .model
            .meta_val_str("general.file_type")
            .expect("the key")
    );

    let set = &block["set"];
    assert_eq!(set["n_ubatch"], 512);
    assert_eq!(set["type_k"], "f16");
    assert_eq!(set["type_v"], "f16");
    assert_eq!(set["flash_attn"], "enabled");
    assert_eq!(set["offload_kqv"], true);
    assert_eq!(set["op_offload"], true);
    assert_eq!(set["swa_full"], true);
    assert_eq!(set["kv_unified"], false);
    // The host load offloads nothing and leaves the engine's split mode.
    assert_eq!(set["n_gpu_layers"], 0);
    assert_eq!(set["split_mode"], "layer");
    assert_eq!(set["main_gpu"], 0);
    assert_eq!(set["tensor_split"], serde_json::json!([]));

    let metadata = block["metadata"].as_array().expect("a list");
    let keys: Vec<&str> = metadata
        .iter()
        .map(|meta| {
            closed(meta, &["key", "value"], "a metadata pair");
            meta["key"].as_str().expect("a key")
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "key order");
    assert!(
        keys.iter()
            .all(|key| key.starts_with("qwen2.rope.") || *key == "qwen2.context_length"),
        "{keys:?}"
    );
    assert!(keys.contains(&"qwen2.rope.freq_base"), "{keys:?}");
    let context_length = metadata
        .iter()
        .find(|meta| meta["key"] == "qwen2.context_length")
        .expect("the context length");
    assert_eq!(
        context_length["value"],
        model.model.n_ctx_train().to_string()
    );

    assert_eq!(device["n_threads"], 4);
    assert_eq!(device["n_threads_batch"], 4);
    assert_eq!(device["use_mmap"], true);
    assert_eq!(device["use_mlock"], false);
    assert_eq!(device["mmap_supported"], llama_cpp_2::mmap_supported());
    assert_eq!(device["mlock_supported"], llama_cpp_2::mlock_supported());
    assert_eq!(device["headroom"], 0);
    assert_eq!(
        device["cuda_device_order"],
        std::env::var("CUDA_DEVICE_ORDER").map_or(serde_json::Value::Null, Into::into)
    );
    let devices = device["devices"].as_array().expect("a list");
    assert_eq!(
        devices.len(),
        llama_cpp_2::list_llama_ggml_backend_devices().len()
    );
    for listed in devices {
        closed(listed, &["name", "description", "memory_total"], "a device");
    }
}

/// **A context left at AUTO flash attention renders no blocks**, per
/// `weaver-spu-Spec` section 3.1.1: `set.flash_attn` is `"enabled"` or
/// `"disabled"`, never auto, because AUTO's choice reaches only the log. A
/// context opened with the engine's defaults is that context, and the
/// render refuses it as an engine fault rather than stating a value it does
/// not know.
///
/// Perturbation: render AUTO as `"enabled"`, and this fails.
#[test]
fn a_context_left_at_auto_flash_attention_renders_no_blocks() {
    let Some(model) = model_or_skip() else { return };
    let defaults = GgufEngine::open_with(&model, 512, false, |params| params)
        .expect("the context opens with the engine's defaults");
    let refused = render_blocks(&model, &defaults, FlushMechanism::TruncateToPosition);
    assert!(
        matches!(&refused, Err(DecodeFault::Engine { detail }) if detail.contains("never auto")),
        "{:?}",
        refused.map(|_| ())
    );
}

/// **The `set` members state what the context was handed**, per
/// `weaver-spu-Spec` section 3.1.1: they are read from the parameters the
/// context was made with (`theta::ContextSet::of`), not from the constants
/// that built them. A context handed a q8_0 K cache states q8_0, and the
/// members it was not handed differently stay as `engine_settings` set them.
///
/// Perturbation: read `type_k` in `ContextSet::of` as a constant f16, and
/// this fails.
#[test]
fn the_set_members_state_what_the_context_was_handed() {
    let Some(model) = model_or_skip() else { return };
    let handed = GgufEngine::open_with(&model, 512, false, |params| {
        engine_settings(params).with_type_k(KvCacheType::Q8_0)
    })
    .expect("the context opens with a q8_0 K cache");
    let blocks = render_blocks(&model, &handed, FlushMechanism::TruncateToPosition)
        .expect("the blocks render");
    let block: serde_json::Value = serde_json::from_str(blocks.engine.get()).expect("JSON");
    assert_eq!(block["set"]["type_k"], "q8_0");
    assert_eq!(block["set"]["type_v"], "f16");
    assert_eq!(block["set"]["flash_attn"], "enabled");
}
