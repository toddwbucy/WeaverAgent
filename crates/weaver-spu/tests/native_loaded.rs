//! The native backend against a real safetensors artifact, end to end:
//! resolve, header, admit, session, generation. The GGUF peer's suite is
//! `loaded.rs` and this file mirrors its discipline: real artifact, skipped
//! where the workshop lacks it, failing where an operator named one that is
//! not there.
#![cfg(feature = "cuda")]

use std::path::PathBuf;

use weaver_spu::artifact;
use weaver_spu::decoder::session::{NeverCancels, PositionedSinks, SamplerBuild, StopCondition};
use weaver_spu::family::FamilyName;
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, Residency};
use weaver_spu::sampling::EffectiveKnobs;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

/// The safetensors artifact, a directory the resolution walks into. The
/// override follows the per-fixture pattern of `selection.rs`: unset and
/// absent skips, named and absent fails.
fn artifact_dir() -> Option<PathBuf> {
    match std::env::var_os("WEAVER_ARTIFACT_QWEN25_SAFETENSORS") {
        Some(named) => {
            let path = PathBuf::from(named);
            assert!(
                path.is_dir(),
                "WEAVER_ARTIFACT_QWEN25_SAFETENSORS names {}, which is not a directory",
                path.display()
            );
            Some(path)
        }
        None => {
            let path = PathBuf::from("/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct");
            path.is_dir().then_some(path)
        }
    }
}

/// The header reads the sidecars: a stock export's `__metadata__` carries
/// only its format, so the family and the template arrive from `config.json`
/// and `tokenizer_config.json` beside the weights.
#[test]
fn the_header_reads_the_sidecars() {
    let Some(dir) = artifact_dir() else {
        eprintln!("SKIP the_header_reads_the_sidecars: no safetensors artifact");
        return;
    };
    let resolved = artifact::resolve(&ArtifactRef(dir.to_string_lossy().into_owned()))
        .expect("a directory artifact resolves to its container");
    let mut pinned = artifact::pin(&resolved).expect("the container pins");
    let header = artifact::read_header(&mut pinned).expect("the header reads");
    assert_eq!(header.family, FamilyName("qwen2".into()));
    assert_eq!(header.hidden_size, Some(896));
    assert_eq!(header.layer_count, Some(24));
    assert!(
        header
            .chat_template
            .as_deref()
            .is_some_and(|t| t.contains("<|im_start|>")),
        "the template arrives from tokenizer_config.json"
    );
}

/// One full pass: admit onto the device, open a session, decode a rendered
/// prompt, and generate. What this proves is the seam, not the model: the
/// native engine serves the same five primitives the GGUF peer serves, under
/// the same session loop.
#[test]
fn a_safetensors_artifact_generates_through_the_native_engine() {
    let Some(dir) = artifact_dir() else {
        eprintln!("SKIP a_safetensors_artifact_generates: no safetensors artifact");
        return;
    };
    if !cuda_present() {
        eprintln!("SKIP a_safetensors_artifact_generates: no CUDA device");
        return;
    }

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0)],
    };
    let resident = residency
        .admit(
            &binding,
            Headroom(64 * 1024 * 1024),
            ReadoutElection(false),
            false,
        )
        .expect("the admit succeeds against a real artifact");

    // The prompt through the family's own rendering, tokenized against the
    // artifact's own vocabulary.
    let prompt =
        "<|im_start|>user\nReply with exactly one word: hello<|im_end|>\n<|im_start|>assistant\n";
    let prefix = resident.tokenize(prompt).expect("the prompt tokenizes");
    assert!(!prefix.is_empty(), "a rendered prompt yields tokens");

    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let mut session = resident
        .open_session(&knobs, 512)
        .expect("a session opens over the native residency");
    session.open(&prefix).expect("the prefix decodes");

    // `<|im_end|>` is the family's turn close, promoted against this
    // vocabulary at a known id; asserting the promotion here keeps the stop
    // set honest rather than assumed.
    let close = resident
        .tokenize("<|im_end|>")
        .expect("the close tokenizes");
    let [terminator] = close.as_slice() else {
        panic!("the turn close promotes to one token, got {close:?}");
    };

    let generated = session
        .append_and_generate(
            &[],
            &StopCondition {
                stop_tokens: vec![*terminator],
                terminator: *terminator,
                max_tokens: 16,
            },
            &mut NeverCancels,
            &mut |_token| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the generation completes");

    assert!(
        !generated.tokens.is_empty(),
        "the model generated at least one token"
    );
    let text = resident
        .detokenize(&generated.tokens)
        .expect("the emission detokenizes");
    assert!(
        !text.trim().is_empty(),
        "the emission renders to text: {text:?}"
    );
    eprintln!("native emission: {text:?}");
}

/// **The surprisal's election reaches the native engine's session and
/// governs what it produces**, per `weaver-spu-PRD` section 13.12. The
/// election is carried at admit and held by the session for the residency's
/// life, so what this proves is the whole route on the engine the unit
/// tests do not exercise: declaration, admit, session, generation, reading.
///
/// **Both states run against the same artifact and the same seed.** A test
/// that only elected would pass with an election nothing consults, and one
/// that only declined would pass with a vector nothing ever builds. The
/// perplexity standing in both arms is the property the charter rests the
/// election's honesty on: the terms are computed either way and only their
/// keeping differs.
#[test]
fn the_surprisal_election_governs_the_native_session() {
    let Some(dir) = artifact_dir() else {
        eprintln!("SKIP the_surprisal_election_governs_the_native_session: no artifact");
        return;
    };
    if !cuda_present() {
        eprintln!("SKIP the_surprisal_election_governs_the_native_session: no CUDA device");
        return;
    }

    let prompt =
        "<|im_start|>user\nReply with exactly one word: hello<|im_end|>\n<|im_start|>assistant\n";
    let mut seen = Vec::new();
    for elected in [false, true] {
        let mut residency = Residency::new();
        let binding = ModelBinding {
            artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
            devices: vec![DeviceOrdinal(0)],
        };
        let resident = residency
            .admit(
                &binding,
                Headroom(64 * 1024 * 1024),
                ReadoutElection(false),
                elected,
            )
            .expect("the admit succeeds against a real artifact");
        let prefix = resident.tokenize(prompt).expect("the prompt tokenizes");
        let close = resident
            .tokenize("<|im_end|>")
            .expect("the close tokenizes");
        let [terminator] = close.as_slice() else {
            panic!("the turn close promotes to one token, got {close:?}");
        };
        // Greedy, so the two arms decode the same tokens and a difference
        // between them is the election's doing rather than the sampler's.
        let knobs = EffectiveKnobs {
            temperature: 0.0,
            top_k: 1,
            top_p: 1.0,
            repetition_penalty: 1.0,
            repetition_window: 0,
            seed: 11,
        };
        let mut session = resident
            .open_session(&knobs, 512)
            .expect("the session opens");
        session.open(&prefix).expect("the prefix decodes");
        let generated = session
            .append_and_generate(
                &[],
                &StopCondition {
                    stop_tokens: vec![*terminator],
                    terminator: *terminator,
                    max_tokens: 16,
                },
                &mut NeverCancels,
                &mut |_token| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("the generation completes");

        assert!(!generated.tokens.is_empty(), "the model answered");
        assert!(
            generated.signals.entropy_bits.is_some(),
            "the entropy carries no election and stands in both arms"
        );
        assert!(
            generated.signals.perplexity.is_some(),
            "and so does the perplexity, which is what stands in the \
             vector's place"
        );
        assert_eq!(
            generated.signals.surprisal_bits.is_some(),
            elected,
            "the vector is kept exactly where the election stands"
        );
        if elected {
            assert_eq!(
                generated.signals.surprisal_bits.as_ref().map(|b| b.len()),
                generated.signals.entropy_bits.as_ref().map(|b| b.len()),
                "and it is paired with the entropy position for position"
            );
        }
        seen.push((generated.tokens.clone(), generated.signals.perplexity));
    }

    let (declined_tokens, declined_perplexity) = &seen[0];
    let (elected_tokens, elected_perplexity) = &seen[1];
    assert_eq!(
        declined_tokens, elected_tokens,
        "the election changes no token, which is the behaviour-neutral bar \
         every diagnostic in this program is held to"
    );
    // **The mean is compared within a tolerance and the tokens are not.**
    // The two arms are separate residencies over the same weights, and apex
    // section 8 puts residual determinism "within GPU float tolerance"
    // rather than at the bit: a different allocation can select a different
    // kernel, and a last-bit difference in one logit moves a surprisal and
    // so the mean. The tokens are discrete and carry no such slack, which is
    // why the behaviour-neutral claim above is asserted exactly and this one
    // is not. A relative bound rather than an absolute one, perplexity being
    // a magnitude that varies with the vocabulary rather than a quantity
    // near one.
    let declined = declined_perplexity.expect("the declined arm reports a mean");
    let elected = elected_perplexity.expect("the elected arm reports one too");
    let spread = (declined - elected).abs() / declined.abs().max(elected.abs());
    assert!(
        spread < 1e-6,
        "the mean is the same mean within tolerance, the election having \
         governed what was kept rather than what was computed: {declined} \
         against {elected}, relative spread {spread}"
    );
}

/// The pair serves the same artifact the single card serves, and greedy
/// decoding answers the same first tokens: the sharded forward is the same
/// model cut in half, so the strongest cheap assertion is agreement with the
/// whole. Reduction order differs between the two, so low-order logit bits
/// may differ where two candidates tie; the first token is asserted and the
/// rest is reported.
#[test]
fn the_pair_agrees_with_the_single_card() {
    let Some(dir) = artifact_dir() else {
        eprintln!("SKIP the_pair_agrees: no safetensors artifact");
        return;
    };
    if cudarc::driver::CudaContext::new(1).is_err() {
        eprintln!("SKIP the_pair_agrees: fewer than two CUDA devices");
        return;
    }
    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let prompt =
        "<|im_start|>user\nReply with exactly one word: hello<|im_end|>\n<|im_start|>assistant\n";

    let mut emissions = Vec::new();
    for devices in [
        vec![DeviceOrdinal(0)],
        vec![DeviceOrdinal(0), DeviceOrdinal(1)],
    ] {
        let width = devices.len();
        let mut residency = Residency::new();
        let binding = ModelBinding {
            artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
            devices,
        };
        let resident = residency
            .admit(
                &binding,
                Headroom(64 * 1024 * 1024),
                ReadoutElection(false),
                false,
            )
            .unwrap_or_else(|refusal| panic!("the admit at width {width}: {refusal:?}"));
        let prefix = resident.tokenize(prompt).expect("tokenizes");
        let mut session = resident.open_session(&knobs, 512).expect("session opens");
        session.open(&prefix).expect("prefix decodes");
        let close = resident.tokenize("<|im_end|>").expect("close tokenizes");
        let [terminator] = close.as_slice() else {
            panic!("the turn close promotes to one token, got {close:?}");
        };
        let generated = session
            .append_and_generate(
                &[],
                &StopCondition {
                    stop_tokens: close.clone(),
                    terminator: *terminator,
                    max_tokens: 8,
                },
                &mut NeverCancels,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("generates");
        let text = resident.detokenize(&generated.tokens).expect("detokenizes");
        eprintln!(
            "width {width} emission: {text:?} tokens: {:?}",
            generated.tokens
        );
        emissions.push(generated.tokens.clone());
    }
    // Greedy at temperature zero over the same weights: the full sequences
    // agree, measured, and the whole sequence is the pin. A future flake
    // here would mean two candidates tied close enough for reduction order
    // to pick differently, and that is a fact to meet with evidence in hand
    // rather than pre-excused by a weaker assertion.
    assert!(
        !emissions[0].is_empty() && !emissions[1].is_empty(),
        "both widths generated"
    );
    assert_eq!(
        emissions[0], emissions[1],
        "the pair's greedy sequence agrees with the single card's"
    );
}

/// The single card keeps its rotary angle past the first position BF16
/// cannot represent. The fork's qwen2 builds its rotary tables in fp32 and
/// casts after the trigonometry, per Spec section 1.1, but that construction
/// is private to the fork: the rotary watch of #640 reads this crate's own
/// pair builder and says it cannot see the single card's. What the single
/// card does expose is its generation. BF16 carries eight significand bits,
/// so 257 is the first integer it cannot hold, and a BF16-first construction
/// rounds every position from there, 257 to 256, which in the highest
/// frequency lane turns cos(257) = 0.82 into cos(256) = -0.04. Past position
/// 256, then, the single card agrees with the pair, whose tables are pinned,
/// only while both keep the angle, and the agreement test above runs at
/// positions where both orderings agree.
///
/// **The prefix runs past that position before the first draw, and the test
/// refuses to run short.** A prefix under 257 tokens would pass on positions
/// no rounding reaches and read as a watch it is not.
///
/// Perturbation: build the fork's `RotaryEmbedding` BF16-first, casting the
/// inverse frequencies and the positions before their product, and the
/// single card's greedy sequence parts from the pair's within the first
/// draws. Recorded in the act that landed this test.
#[test]
fn the_single_card_keeps_its_rotary_angle_past_position_256() {
    let Some(dir) = artifact_dir() else {
        eprintln!("SKIP the_single_card_keeps_its_rotary_angle: no safetensors artifact");
        return;
    };
    if cudarc::driver::CudaContext::new(1).is_err() {
        eprintln!("SKIP the_single_card_keeps_its_rotary_angle: fewer than two CUDA devices");
        return;
    }
    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    // A user turn long enough that its last token sits past position 256:
    // the first draw then lands where BF16 first rounds a position.
    let mut prompt =
        String::from("<|im_start|>user\nHere is a numbered list. Reply with its last item only.\n");
    for item in 1..=40 {
        prompt.push_str(&format!("item {item}: a short line of ordinary text\n"));
    }
    prompt.push_str("<|im_end|>\n<|im_start|>assistant\n");

    let mut emissions = Vec::new();
    for devices in [
        vec![DeviceOrdinal(0)],
        vec![DeviceOrdinal(0), DeviceOrdinal(1)],
    ] {
        let width = devices.len();
        let mut residency = Residency::new();
        let binding = ModelBinding {
            artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
            devices,
        };
        let resident = residency
            .admit(
                &binding,
                Headroom(64 * 1024 * 1024),
                ReadoutElection(false),
                false,
            )
            .unwrap_or_else(|refusal| panic!("the admit at width {width}: {refusal:?}"));
        let prefix = resident.tokenize(&prompt).expect("tokenizes");
        assert!(
            prefix.len() > 256,
            "the prefix must run past position 256 for the watch to see a rounded \
             position, and it holds {} tokens",
            prefix.len()
        );
        let mut session = resident.open_session(&knobs, 1024).expect("session opens");
        session.open(&prefix).expect("prefix decodes");
        let close = resident.tokenize("<|im_end|>").expect("close tokenizes");
        let [terminator] = close.as_slice() else {
            panic!("the turn close promotes to one token, got {close:?}");
        };
        let generated = session
            .append_and_generate(
                &[],
                &StopCondition {
                    stop_tokens: close.clone(),
                    terminator: *terminator,
                    max_tokens: 8,
                },
                &mut NeverCancels,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("generates");
        let text = resident.detokenize(&generated.tokens).expect("detokenizes");
        eprintln!(
            "width {width} at prefix {}: emission {text:?} tokens: {:?}",
            prefix.len(),
            generated.tokens
        );
        emissions.push(generated.tokens.clone());
    }
    assert!(
        !emissions[0].is_empty() && !emissions[1].is_empty(),
        "both widths generated"
    );
    assert_eq!(
        emissions[0], emissions[1],
        "past position 256 the single card's greedy sequence agrees with the pair's, \
         whose rotary tables are pinned"
    );
}

/// An elected readout travels with the generation, at both widths: one
/// norm per layer per forward, in order, every figure finite, and the
/// prefix decode's figures drained rather than leaking into the turn's.
/// The unelected path stays absent rather than empty, the record's own
/// discipline, asserted against the same artifact in the same breath.
#[test]
fn an_elected_readout_travels_with_the_generation() {
    let Some(dir) = artifact_dir() else {
        eprintln!("SKIP an_elected_readout: no safetensors artifact");
        return;
    };
    let both = cudarc::driver::CudaContext::new(1).is_ok();
    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let prompt =
        "<|im_start|>user\nReply with exactly one word: hello<|im_end|>\n<|im_start|>assistant\n";
    let mut widths = vec![vec![DeviceOrdinal(0)]];
    if both {
        widths.push(vec![DeviceOrdinal(0), DeviceOrdinal(1)]);
    } else {
        eprintln!("SKIP an_elected_readout pair half: fewer than two CUDA devices");
    }
    for (devices, elected) in widths
        .into_iter()
        .flat_map(|devices| [(devices.clone(), true), (devices, false)])
    {
        let width = devices.len();
        let mut residency = Residency::new();
        let binding = ModelBinding {
            artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
            devices,
        };
        let resident = residency
            .admit(
                &binding,
                Headroom(64 * 1024 * 1024),
                ReadoutElection(elected),
                false,
            )
            .unwrap_or_else(|refusal| panic!("admit width {width}: {refusal:?}"));
        let prefix = resident.tokenize(prompt).expect("tokenizes");
        let mut session = resident.open_session(&knobs, 512).expect("session opens");
        session.open(&prefix).expect("prefix decodes");
        let close = resident.tokenize("<|im_end|>").expect("close tokenizes");
        let [terminator] = close.as_slice() else {
            panic!("one close token");
        };
        let generated = session
            .append_and_generate(
                &[],
                &StopCondition {
                    stop_tokens: close.clone(),
                    terminator: *terminator,
                    max_tokens: 4,
                },
                &mut NeverCancels,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("generates");
        match (elected, &generated.residual) {
            (false, None) => {}
            (false, Some(_)) => {
                panic!("width {width}: unelected but a reduction present")
            }
            (true, None) => panic!("width {width}: elected but no reduction"),
            (true, Some(reduction)) => {
                // **Read rather than divided.** This computed the layer count
                // by dividing the figure count by a hardcoded twenty-four
                // until 2026-08-24, which is the arithmetic #293 names: it
                // held only while every forward tapped every layer, and no
                // document said so.
                let norms = reduction.per_layer_norm();
                assert!(!norms.is_empty(), "width {width}: empty reduction");
                assert_eq!(
                    reduction.layers(),
                    24,
                    "width {width}: the artifact has twenty-four layers"
                );
                assert_eq!(
                    reduction.layers() * reduction.forwards(),
                    reduction.figures(),
                    "width {width}: the counts do not describe the figures"
                );
                // The ceiling is exact on purpose: an empty delta decodes
                // nothing, four sampled tokens decode once each, and the
                // terminator's landing is the fifth, so a leaked prefix
                // pass is the sixth and fails here rather than slipping
                // under a slack bound.
                assert!(
                    reduction.forwards() <= 5,
                    "width {width}: {} forwards claims more than the turn ran, \
                     the prefix leaked",
                    reduction.forwards()
                );
                assert!(
                    norms.iter().all(|n| n.is_finite() && *n > 0.0),
                    "width {width}: a figure is not a finite positive norm"
                );
                eprintln!(
                    "width {width} elected: {} figures, {} layers over {} forwards",
                    reduction.figures(),
                    reduction.layers(),
                    reduction.forwards()
                );
            }
        }
    }
}

/// An elected readout against a family that declares no tap refuses at
/// admit, naming the family, because a load that succeeded and failed at
/// the first turn would be the expensive lie the charter forbids.
///
/// **The container stopped being the ground on 2026-08-22.** This read the
/// refusal against a qwen2 GGUF while the GGUF tap did not exist and every
/// GGUF load refused an election whatever its family declared. Both engines
/// tap now, so the refusal is reached through SmolLM2, whose family selects
/// a `llama` entry declaring nothing. The contrast this test once drew,
/// native taps and GGUF does not, is retired rather than repaired: what it
/// watches now is the residency path reading the declaration, which is a
/// different and still live claim.
#[test]
fn an_elected_readout_refuses_a_gguf_residency_at_admit() {
    let gguf = PathBuf::from("/opt/weaver/models/smollm2-360m-instruct-q8_0.gguf");
    if !gguf.is_file() {
        eprintln!(
            "SKIP an_elected_readout_refuses_gguf: no GGUF artifact from a family \
             that declares no tap"
        );
        return;
    }
    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(gguf.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0)],
    };
    let refusal = residency
        .admit(
            &binding,
            Headroom(64 * 1024 * 1024),
            ReadoutElection(true),
            false,
        )
        .expect_err("an elected GGUF admit refuses");
    assert!(
        format!("{refusal:?}").contains("NotTappable"),
        "refused for the tap, got {refusal:?}"
    );
}

/// **An elected readout against a GGUF family that declares a tap is
/// admitted, and the reduction arrives.** The positive half of the rule
/// above, and the half that pins #284.
///
/// **Watched: without this, restoring the container ground fails nothing.**
/// Measured 2026-08-23 by putting that ground back in `residency.rs` and
/// running this crate's suite under both engines: every test passed. The
/// negative cases cannot catch it, because each reaches its refusal through
/// a family that declares no tap and would refuse under either rule, and
/// `readout::judge`'s own unit tests call the judgment directly and never
/// travel the admit path. So the substance of the narrowing had no watch on
/// this path at all, which is what this test is.
///
/// **It reads the engine and not only the judgment.** An admission that
/// succeeded while the election never reached the backend would satisfy a
/// bare `is_ok`, so the reduction is drawn from a real generation and
/// counted against the artifact's own layer count.
#[test]
fn an_elected_readout_is_admitted_by_a_gguf_family_that_declares_a_tap() {
    let gguf = PathBuf::from(
        "/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct-GGUF/qwen2.5-0.5b-instruct-q8_0.gguf",
    );
    if !gguf.is_file() {
        eprintln!(
            "SKIP an_elected_readout_is_admitted: no GGUF artifact from a family \
             that declares a tap"
        );
        return;
    }
    // The artifact's own `qwen2.block_count`, named rather than derived from
    // the reduction under test, which would agree with itself whatever the
    // tap did.
    const LAYERS: usize = 24;

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(gguf.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0)],
    };
    let resident = residency
        .admit(
            &binding,
            Headroom(64 * 1024 * 1024),
            ReadoutElection(true),
            false,
        )
        .expect("a family declaring a tap admits an elected readout");

    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let prompt =
        "<|im_start|>user\nReply with exactly one word: hello<|im_end|>\n<|im_start|>assistant\n";
    let prefix = resident.tokenize(prompt).expect("tokenizes");
    let mut session = resident.open_session(&knobs, 512).expect("session opens");
    session.open(&prefix).expect("prefix decodes");
    let close = resident.tokenize("<|im_end|>").expect("close tokenizes");
    let [terminator] = close.as_slice() else {
        panic!("one close token");
    };
    let generated = session
        .append_and_generate(
            &[],
            &StopCondition {
                stop_tokens: close.clone(),
                terminator: *terminator,
                max_tokens: 4,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("generates");

    let reduction = generated
        .residual
        .expect("elected across the admit, so the generation answers a reduction");
    let norms = reduction.per_layer_norm();
    assert!(!norms.is_empty(), "the reduction is empty");
    // **Read rather than divided**, per Spec section 6. The modulus below
    // stood in for a layer count the payload did not carry.
    assert_eq!(
        reduction.layers(),
        LAYERS,
        "the reduction names {} layers where the artifact has {LAYERS}",
        reduction.layers()
    );
    assert_eq!(
        reduction.layers() * reduction.forwards(),
        reduction.figures(),
        "the counts do not describe the figures"
    );
    assert!(
        norms.iter().all(|n| n.is_finite() && *n > 0.0),
        "the figures are the model's rather than an unwritten buffer"
    );
}

/// The measurement instrument for the reduction's cost, not a gate: run
/// with `WEAVER_MEASURE_PACE=1` and it reports the single card's pace and
/// the pair's on the hop-dominated 0.5B worst case, 256 greedy tokens
/// each, timing the generation alone. It asserts nothing about the
/// numbers, because a pace is a fact to read rather than a threshold to
/// flake on, and it skips by default so the suite's cost stays flat.
#[test]
fn the_pair_reports_its_pace() {
    if std::env::var_os("WEAVER_MEASURE_PACE").is_none() {
        eprintln!("SKIP the_pair_reports_its_pace: WEAVER_MEASURE_PACE unset");
        return;
    }
    let Some(dir) = artifact_dir() else {
        eprintln!("SKIP the_pair_reports_its_pace: no safetensors artifact");
        return;
    };
    if cudarc::driver::CudaContext::new(1).is_err() {
        eprintln!("SKIP the_pair_reports_its_pace: fewer than two CUDA devices");
        return;
    }
    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let prompt = "<|im_start|>user\nCount upward from one, comma separated, \
                  for as long as you can.<|im_end|>\n<|im_start|>assistant\n";
    for devices in [
        vec![DeviceOrdinal(0)],
        vec![DeviceOrdinal(0), DeviceOrdinal(1)],
    ] {
        let width = devices.len();
        let mut residency = Residency::new();
        let binding = ModelBinding {
            artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
            devices,
        };
        let resident = residency
            .admit(
                &binding,
                Headroom(64 * 1024 * 1024),
                ReadoutElection(false),
                false,
            )
            .unwrap_or_else(|refusal| panic!("the admit at width {width}: {refusal:?}"));
        let prefix = resident.tokenize(prompt).expect("tokenizes");
        let mut session = resident.open_session(&knobs, 2048).expect("session opens");
        session.open(&prefix).expect("prefix decodes");
        let close = resident.tokenize("<|im_end|>").expect("close tokenizes");
        let [terminator] = close.as_slice() else {
            panic!("the turn close promotes to one token, got {close:?}");
        };
        let started = std::time::Instant::now();
        let generated = session
            .append_and_generate(
                &[],
                &StopCondition {
                    stop_tokens: close.clone(),
                    terminator: *terminator,
                    max_tokens: 256,
                },
                &mut NeverCancels,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("generates");
        let elapsed = started.elapsed().as_secs_f64();
        let count = generated.tokens.len();
        eprintln!(
            "PACE width {width}: {count} tokens in {elapsed:.2}s = {:.1} tok/s",
            count as f64 / elapsed
        );
    }
}

/// The model the pair exists for: larger than any single card, sharded
/// safetensors, served across both devices through the native engine. The
/// deliberate mirror of `two_card.rs`'s split-GGUF proof in the second
/// container.
#[test]
fn a_large_sharded_safetensors_serves_across_the_pair() {
    let overridden = std::env::var_os("WEAVER_ARTIFACT_QWEN25_32B").is_some();
    let dir = match std::env::var_os("WEAVER_ARTIFACT_QWEN25_32B") {
        Some(named) => {
            let path = PathBuf::from(named);
            assert!(
                path.is_dir(),
                "WEAVER_ARTIFACT_QWEN25_32B names {}, which is not a directory",
                path.display()
            );
            path
        }
        None => {
            let path = PathBuf::from("/bulk-store/models/Qwen--Qwen2.5-32B-Instruct");
            if !path.is_dir() || !path.join("model-00001-of-00017.safetensors").is_file() {
                eprintln!("SKIP a_large_sharded_safetensors: artifact absent or incomplete");
                return;
            }
            path
        }
    };
    if cudarc::driver::CudaContext::new(1).is_err() {
        eprintln!("SKIP a_large_sharded_safetensors: fewer than two CUDA devices");
        return;
    }

    // The premise, asserted the way the GGUF split test asserts it: either
    // card alone refuses for room.
    for ordinal in [0u32, 1] {
        let mut probe = Residency::new();
        let single = ModelBinding {
            artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
            devices: vec![DeviceOrdinal(ordinal)],
        };
        let refused = probe.admit(
            &single,
            Headroom(2 * 1024 * 1024 * 1024),
            ReadoutElection(false),
            false,
        );
        assert!(
            refused.is_err(),
            "the 32B fits no single card, so ordinal {ordinal} alone refuses"
        );
    }

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(dir.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0), DeviceOrdinal(1)],
    };
    let resident = residency
        .admit(
            &binding,
            Headroom(2 * 1024 * 1024 * 1024),
            ReadoutElection(false),
            false,
        )
        .expect("the 32B admits across the pair");

    let prompt =
        "<|im_start|>user\nReply with exactly one word: hello<|im_end|>\n<|im_start|>assistant\n";
    let prefix = resident.tokenize(prompt).expect("tokenizes");
    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let mut session = resident.open_session(&knobs, 1024).expect("session opens");
    let opened = std::time::Instant::now();
    session.open(&prefix).expect("the prefix decodes");
    let close = resident.tokenize("<|im_end|>").expect("close tokenizes");
    let [terminator] = close.as_slice() else {
        panic!("the turn close promotes to one token, got {close:?}");
    };
    let generated = session
        .append_and_generate(
            &[],
            &StopCondition {
                stop_tokens: close.clone(),
                terminator: *terminator,
                max_tokens: 16,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the 32B generates");
    let secs = opened.elapsed().as_secs_f64();
    let text = resident.detokenize(&generated.tokens).expect("detokenizes");
    eprintln!(
        "32B pair emission: {text:?} ({} tokens, {secs:.1}s with prefill)",
        generated.tokens.len()
    );
    assert!(!generated.tokens.is_empty());
    // **The known fixture's answer is pinned exactly**: greedy at zero
    // temperature over fixed weights is deterministic, and the measured
    // emission is the one word the instruction asks for, in one token. An
    // operator's override may name a model whose obedience this test never
    // promised, so the content pin holds for the fixture the test knows and
    // the serving assertions above hold for any.
    if !overridden {
        assert_eq!(generated.tokens.len(), 1, "one word, one token");
        assert_eq!(text.trim(), "hello", "the instruction is followed exactly");
    }
}

fn cuda_present() -> bool {
    cudarc::driver::CudaContext::new(0).is_ok()
}
