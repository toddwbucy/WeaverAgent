//! conforms: spu-marker-promotion
//!
//! The inbound direction of `weaver-spu-Spec` section 5, bought as section 10
//! asks: every control marker a family renders tokenizes to exactly one token.
//!
//! **A marker that degrades to sub-word text is structure the model reads as
//! prose.** The renderer emits its markers as literal text, and that rendering
//! is faithful only where the tokenizer promotes each literal back to the one
//! control token it stands for. `str_to_token` calls `llama_tokenize` with
//! `parse_special` set, which promotes a literal **iff** the GGUF's tokenizer
//! marks that string special, so the property is a fact about the pairing of a
//! family's renderer with a family's tokenizer and not about either alone.
//!
//! **The load is vocab-only and reaches no device.** `with_vocab_only` reads
//! the header and the tokenizer and stops, so this needs the `gguf` feature and
//! no GPU, which is what the feature comment in `Cargo.toml` says that build
//! exists for. The archived tree's version of this test loaded full weights
//! onto `gpu_id: 0`; nothing here needs weights, and the salvage improves on
//! that rather than carrying it across.
//!
//! **Only the rendered set is asserted.** Promotion is the inbound claim, and a
//! family's parsed markers are matched as text against an emission this crate
//! never tokenizes. Where a parsed marker does not promote, the family module
//! says so at its declaration rather than this test asserting a property the
//! corpus does not claim.
//!
//! **The file also holds the flush declaration's check**, per Spec section 4.4,
//! because it is the other claim a family makes that only an artifact can
//! settle and the probe table is where the artifacts are named. Both tests ask
//! the same question in the same way: the family declares, the artifact
//! answers, and the two must agree.
#![cfg(feature = "gguf")]

use std::path::{Path, PathBuf};

use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};

use weaver_spu::family::{FamilyName, gemma4, gpt_oss, llama, mistral3, phi, qwen2};

/// A family, its rendered markers, and where a tokenizer for it is found.
///
/// The paths are defaults an operator overrides with `WEAVER_VOCAB_<FAMILY>`,
/// because a workshop's model layout is not a fact this crate should hold.
struct Probe {
    family: &'static str,
    env: &'static str,
    default_path: &'static str,
    rendered: &'static [&'static str],
}

const PROBES: &[Probe] = &[
    Probe {
        family: "llama",
        env: "WEAVER_VOCAB_LLAMA",
        default_path: "/fastpool/scratch/llama.cpp/models/ggml-vocab-llama-bpe.gguf",
        rendered: llama::RENDERED_MARKERS,
    },
    Probe {
        family: "qwen2",
        env: "WEAVER_VOCAB_QWEN2",
        default_path: "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf",
        rendered: qwen2::RENDERED_MARKERS,
    },
    Probe {
        // **Qwen3 is a separate registry key served by the qwen2 module**, so
        // its tokenizer is probed separately. Two keys citing one module is a
        // claim about their markers agreeing, and a claim is worth measuring.
        family: "qwen3",
        env: "WEAVER_VOCAB_QWEN3",
        default_path: "/opt/weaver/models/h-dist/Qwen3-8B-Q5_K_M.gguf",
        rendered: qwen2::RENDERED_MARKERS,
    },
    Probe {
        // **Qwen3's sparse sibling is its own architecture key**, carried for
        // the reason the dense one is and probed against its own tokenizer.
        // Its rendered set is qwen2's exactly, with not even the vision
        // markers the qwen35 artifacts carry.
        family: "qwen3moe",
        env: "WEAVER_VOCAB_QWEN3MOE",
        default_path: "/opt/weaver/models/h-dist/Qwen3-30B-A3B-Q5_K_M.gguf",
        rendered: qwen2::RENDERED_MARKERS,
    },
    Probe {
        // **Qwen3.5 declares its own architecture and renders the same ChatML
        // scaffolding**, so it is probed separately for the reason qwen3 is:
        // two keys citing one module is a claim about their markers agreeing,
        // and a claim is worth measuring. The artifacts on this workshop are
        // community quantizations of Qwen3.6, which declare `qwen35`.
        family: "qwen35",
        env: "WEAVER_VOCAB_QWEN35",
        default_path: "/bulk-store/models/DavidAU--Qwen3.6-40B-Claude-4.6-Opus-Deckard-Heretic-Uncensored-Thinking-NEO-CODE-Di-IMatrix-MAX-GGUF/Qwen3.6-40B-Deck-Opus-NEO-CODE-HERE-2T-OT-IQ2_M.gguf",
        rendered: qwen2::RENDERED_MARKERS,
    },
    Probe {
        // The sparse sibling declares its own architecture again, so the same
        // claim is measured against its own tokenizer rather than assumed from
        // the dense one.
        family: "qwen35moe",
        env: "WEAVER_VOCAB_QWEN35MOE",
        default_path: "/bulk-store/books/models/unsloth--Qwen3.6-35B-A3B-GGUF/Qwen3.6-35B-A3B-UD-IQ1_M.gguf",
        rendered: qwen2::RENDERED_MARKERS,
    },
    Probe {
        family: "gpt-oss",
        env: "WEAVER_VOCAB_GPT_OSS",
        default_path: "/opt/weaver/models/h-dist/gpt-oss-20b-mxfp4.gguf",
        rendered: gpt_oss::RENDERED_MARKERS,
    },
    Probe {
        // **The first family whose rendered set is not only turn markers.** Its
        // generation opener carries a channel pair and its identity prefix
        // carries a BOS, so five strings reach a prompt and all five are
        // measured. Two of them, the channel pair, are `USER_DEFINED` rather
        // than `CONTROL` in this artifact's token table, which is exactly the
        // kind of difference a probe settles and a reading of the template
        // does not.
        //
        // **The measurement was taken on a workshop that does not lay its
        // models out this way**, through the override, against
        // `unsloth--gemma-4-26B-A4B-it-qat-GGUF/` under a home directory. The
        // default follows this table's `/opt/weaver/models` convention rather
        // than naming that box, a default being a convenience and the override
        // being what a run actually rests on:
        //
        //     WEAVER_VOCAB_GEMMA4=<artifact> cargo test -p weaver-spu \
        //       --features gguf --test markers
        //
        // A workshop holding none of these artifacts fails the `probed > 0`
        // guard below rather than reporting a green run over nothing, so the
        // absence is loud wherever it lands.
        family: "gemma4",
        env: "WEAVER_VOCAB_GEMMA4",
        default_path: "/opt/weaver/models/gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf",
        rendered: gemma4::RENDERED_MARKERS,
    },
    Probe {
        // **The census's warning made concrete.** Most Mistral artifacts
        // declare `llama` rather than this architecture: Mistral-Small 3.1,
        // Mistral-Small 3.2 and Magistral-Small all do, and their headers were
        // read before this one was chosen. Devstral-Small-2 declares
        // `mistral3`, so it is what this family can be measured against.
        family: "mistral3",
        env: "WEAVER_VOCAB_MISTRAL3",
        default_path: "/opt/weaver/models/Devstral-Small-2-24B-Instruct-2512-Q4_K_M.gguf",
        rendered: mistral3::RENDERED_MARKERS,
    },
    Probe {
        // **The tag half of the contested phi3 pair**, serving Phi-3.5-mini
        // and Phi-4-mini artifacts. The role tag is built by substitution, so
        // what promotes is the built marker, and the mini's vocabulary holds
        // all three as single CONTROL tokens - and holds no `<|im_start|>` at
        // all, which is what makes the pair unambiguous.
        family: "phi3",
        env: "WEAVER_VOCAB_PHI3_TAG",
        default_path: "/opt/weaver/models/microsoft_Phi-4-mini-instruct-Q4_K_M.gguf",
        rendered: phi::TAG_RENDERED_MARKERS,
    },
    Probe {
        // The separator half, serving Phi-4 14B artifacts. Same architecture
        // string, disjoint markers.
        family: "phi3",
        env: "WEAVER_VOCAB_PHI3_SEP",
        default_path: "/opt/weaver/models/phi-4-Q4_K_S.gguf",
        rendered: phi::SEP_RENDERED_MARKERS,
    },
    Probe {
        // **A hybrid family rendering ChatML.** Its template is
        // `<|im_start|>role\n ... <|im_end|>\n`, qwen2's scaffolding exactly,
        // so it is another key citing that module and the claim that their
        // markers agree is measured here rather than inherited.
        //
        // Its `<think>` pair is `USER_DEFINED` rather than `CONTROL`, as
        // gemma4's channel pair is, but it is outbound: the model emits it and
        // this crate matches it as text, so it is not in the rendered set.
        family: "nemotron_h_moe",
        env: "WEAVER_VOCAB_NEMOTRON_H_MOE",
        default_path: "/opt/weaver/models/NVIDIA-Nemotron-3-Nano-30B-A3B-BF16-heretic.i1-IQ4_XS.gguf",
        rendered: qwen2::RENDERED_MARKERS,
    },
];

impl Probe {
    /// The vocab to probe, and whether an operator named it.
    ///
    /// **The two are not the same absence.** A default path that is not there
    /// is a workshop without that family's artifact, which leaves the family
    /// unverified and is reported. An overridden path that is not there is an
    /// operator who asked for a vocabulary and got no measurement, which must
    /// not pass quietly.
    fn path(&self) -> (PathBuf, bool) {
        match std::env::var_os(self.env) {
            Some(value) => (PathBuf::from(value), true),
            None => (PathBuf::from(self.default_path), false),
        }
    }
}

/// A string shaped like a marker that no family declares.
///
/// **The control that keeps the assertion from being vacuous.** A tokenizer
/// that returned one token for every input would satisfy the promotion check
/// without promoting anything, so each vocab is also asked for a lookalike it
/// cannot know, and that must come back as more than one token. Without this
/// the test would pass against a tokenizer that had lost its special set
/// entirely.
const LOOKALIKE: &str = "<|not_a_marker_any_family_declares|>";

/// **The backend is a process-global and is initialized once for this binary.**
///
/// `LlamaBackend::init` is a compare-exchange on a static and answers
/// `BackendAlreadyInitialized` to the second caller, so two tests in one binary
/// each initializing their own would make the later one panic on whichever
/// order the harness happened to run them in. The type is a zero-sized proof of
/// initialization, so sharing one costs nothing and both tests take a reference.
fn backend() -> &'static LlamaBackend {
    static BACKEND: std::sync::OnceLock<LlamaBackend> = std::sync::OnceLock::new();
    BACKEND.get_or_init(|| LlamaBackend::init().expect("the llama backend initialises"))
}

fn vocab_only(backend: &LlamaBackend, path: &Path) -> Result<LlamaModel, String> {
    let params = LlamaModelParams::default()
        .with_vocab_only(true)
        .with_n_gpu_layers(0);
    LlamaModel::load_from_file(backend, path, &params).map_err(|error| error.to_string())
}

/// **Every marker a family renders promotes to exactly one token.**
///
/// The failures are collected across every family before the assertion fires,
/// so one bad marker does not hide the rest. What a reader gets on a failure is
/// the family, the marker, and the token ids it degraded into.
///
/// Perturbation: add a marker to any family's `RENDERED_MARKERS` that its GGUF
/// does not mark special, and this fails naming it. Watched by adding
/// `<|python_tag|>` to llama's rendered set, which the reachable 3.0-era
/// tokenizer splits into six tokens.
#[test]
fn every_rendered_marker_promotes_to_one_token() {
    let backend = backend();

    let mut failures: Vec<String> = Vec::new();
    let mut absent: Vec<String> = Vec::new();
    let mut probed = 0usize;

    for probe in PROBES {
        let (path, overridden) = probe.path();
        // An operator who named a vocab and whose path is not a regular file
        // gets a failure rather than a skip: the ask was explicit, so silence
        // would report a measurement that never ran.
        assert!(
            !overridden || path.is_file(),
            "{} is set to {} which is not a regular file, so {} would have been \
             skipped and this test would have passed without probing it",
            probe.env,
            path.display(),
            probe.family
        );
        if !path.exists() {
            absent.push(format!("{} (no vocab at {})", probe.family, path.display()));
            continue;
        }
        let model = match vocab_only(backend, &path) {
            Ok(model) => model,
            Err(error) => {
                failures.push(format!("{}: vocab-only load failed: {error}", probe.family));
                continue;
            }
        };
        probed += 1;

        for marker in probe.rendered {
            match model.str_to_token(marker, AddBos::Never) {
                Ok(tokens) if tokens.len() == 1 => {}
                Ok(tokens) => failures.push(format!(
                    "{}: {marker} degraded to {} tokens {:?}",
                    probe.family,
                    tokens.len(),
                    tokens.iter().map(|t| t.0).collect::<Vec<_>>()
                )),
                Err(error) => failures.push(format!("{}: {marker}: {error}", probe.family)),
            }
        }

        // The control. A tokenizer answering one token for everything would
        // satisfy every assertion above without promoting anything.
        match model.str_to_token(LOOKALIKE, AddBos::Never) {
            Ok(tokens) if tokens.len() > 1 => {}
            Ok(tokens) => failures.push(format!(
                "{}: the lookalike promoted to {} token(s), so this vocab \
                 promotes indiscriminately and the assertions above prove nothing",
                probe.family,
                tokens.len()
            )),
            Err(error) => failures.push(format!("{}: lookalike: {error}", probe.family)),
        }
    }

    // **A run that probed nothing is a failure, not a pass.** The archived
    // tree's version returned early when no model was present, which reads as
    // green on a machine that tested nothing.
    //
    // The failures are reported here as well as below, because a vocab that
    // exists and will not load leaves `probed` at zero while `absent` stays
    // empty. Naming only the absent set would then report that nothing was
    // found when what happened is that everything failed to open.
    assert!(
        probed > 0,
        "no family vocab was reachable, so this test asserted nothing. Set one of \
         {}.\n  absent: {absent:?}\n  failed to load: {failures:?}",
        PROBES
            .iter()
            .map(|probe| probe.env)
            .collect::<Vec<_>>()
            .join(", ")
    );

    assert!(
        failures.is_empty(),
        "rendered markers that did not promote to a single token, which means the \
         literal-text rendering degrades to sub-word prose for them:\n  {}",
        failures.join("\n  ")
    );

    if !absent.is_empty() {
        // Named rather than silent: a family with no reachable vocab is
        // unverified here, and the run says which.
        eprintln!("marker promotion unverified for: {}", absent.join(", "));
    }
}

/// **A family declaring a truncating flush must not be served by an engine
/// that cannot roll back**, per `weaver-spu-Spec` section 4.4.
///
/// The declaration is the family's and the artifact is the only thing that can
/// contradict it. `llama_model_is_hybrid` and `llama_model_is_recurrent` are
/// the engine's own account of whether a state can be partially erased: a
/// recurrent state is a running summary rather than a per-position cache, and a
/// hybrid carries recurrent layers beside its attention, so `seq_rm` refuses a
/// partial erase on both. A family declaring `TruncateToPosition` against
/// either is asking for a rollback the engine will not perform.
///
/// **This is the test that was missing when qwen35 and qwen35moe were
/// carried.** Both took `TruncateToPosition` from the qwen2 entry along with
/// the template they legitimately share, and nothing read the two facts
/// together. A marker vocabulary says nothing about how state rolls back.
///
/// Perturbation: set either qwen35 entry's `flush` back to
/// `TruncateToPosition` and this fails naming it. Watched under exactly that.
///
/// **The guard counts declarations read, not vocabularies loaded.** A reachable
/// artifact for a family the registry does not carry yet leaves this test with
/// nothing to contradict, so counting the load would let a run pass having
/// checked no declaration at all - which is exactly what happened while
/// `mistral3` was probed and not yet carried. `verified` is what the assertion
/// reads, and `probed` stays only to tell the two absences apart in the
/// message.
#[test]
fn a_truncating_family_is_not_served_by_an_engine_that_cannot_roll_back() {
    let backend = backend();

    let mut failures: Vec<String> = Vec::new();
    let mut absent: Vec<String> = Vec::new();
    let mut probed = 0usize;
    let mut verified = 0usize;

    for probe in PROBES {
        let (path, overridden) = probe.path();
        assert!(
            !overridden || path.is_file(),
            "{} is set to {} which is not a regular file, so {} would have been \
             skipped and this test would have passed without probing it",
            probe.env,
            path.display(),
            probe.family
        );
        if !path.exists() {
            absent.push(format!("{} (no vocab)", probe.family));
            continue;
        }
        let model = match vocab_only(backend, &path) {
            Ok(model) => model,
            Err(error) => {
                failures.push(format!("{}: vocab-only load failed: {error}", probe.family));
                continue;
            }
        };
        probed += 1;

        // **Resolved the way admission resolves: the artifact's own header,
        // through `select`.** A by-name lookup stopped being enough when
        // architectures became contestable, because a contested name refuses
        // by design and this test would have skipped `llama` and `phi3`
        // forever while their flush claims went unchecked. The header carries
        // both facts selection needs, the architecture and the template, and
        // reading them here is what admission does, so the row this test
        // judges is the row a load would get.
        //
        // **A probe entry for a family the registry does not carry yet is not
        // a failure**, the ordinary mid-act state of probe-before-entry. A
        // contested architecture whose resolution refuses is also unverified
        // rather than failed, because admission would refuse that artifact
        // before any flush ran, and it is named with the refusal so a
        // detector gap reads as what it is.
        let header = {
            let mut pinned = weaver_spu::artifact::pin(&path).expect("a probed artifact pins");
            weaver_spu::artifact::read_header(&mut pinned).expect("a probed header reads")
        };
        assert_eq!(
            header.family,
            FamilyName(probe.family.to_string()),
            "the probe table says {} and the artifact declares {}, so this row \
             names the wrong family",
            probe.family,
            header.family.0,
        );
        let declaration =
            match weaver_spu::family::select(&header.family, header.chat_template.as_deref()) {
                Ok(declaration) => declaration,
                Err(weaver_spu::family::FamilyRefusal::UnknownFamily(_)) => {
                    absent.push(format!("{} (probed, not carried yet)", probe.family));
                    continue;
                }
                Err(refusal) => {
                    absent.push(format!(
                        "{} (carried, unresolvable here: {refusal:?})",
                        probe.family
                    ));
                    continue;
                }
            };

        verified += 1;
        let cannot_roll_back = model.is_hybrid() || model.is_recurrent();
        if declaration.permits_truncation() && cannot_roll_back {
            failures.push(format!(
                "{}: declares a truncating flush, but this artifact is {} and its \
                 engine refuses a partial erase, so the flush would report an \
                 outcome it did not reach",
                probe.family,
                if model.is_recurrent() {
                    "recurrent"
                } else {
                    "hybrid"
                },
            ));
        }
    }

    assert!(
        verified > 0,
        "no carried family's declaration was read against an artifact, so this test \
         asserted nothing. {probed} vocab(s) loaded and none belonged to a carried \
         family. Set one of {}.\n  absent: {absent:?}",
        PROBES
            .iter()
            .map(|probe| probe.env)
            .collect::<Vec<_>>()
            .join(", ")
    );

    assert!(
        failures.is_empty(),
        "flush declarations the artifact contradicts:\n  {}",
        failures.join("\n  ")
    );

    if !absent.is_empty() {
        eprintln!("flush declaration unverified for: {}", absent.join(", "));
    }
}
