//! **The device half of "an explicit set moves no draw"**, per
//! `weaver-spu-Spec` section 3.1: an instrument, run by name, that records
//! one generation on a device lane so a run before an act that sets an
//! engine value explicitly and a run after it can be compared.
//!
//! The host half is a standing watch in `decoder/gguf/tests.rs`, which opens
//! the engine with its own defaults beside this crate's settings in one
//! process. A device load goes through the admission, which opens with this
//! crate's settings only, so the device comparison is two builds: the tree
//! before the act and the tree after it, each running this file on the same
//! lane, the artifact, the seed and the prompt held fixed. The record holds
//! the tokens and each token's entropy and surprisal as f32 bits, so a
//! comparison reads bit identity and not closeness.
//!
//! **The theta blocks ride beside the record.** Where the session rendered
//! its `engine` and `device` blocks (section 3.1), they are written as they
//! were rendered to `<out>.theta.json`, so a device run puts on file the
//! blocks a real placement rendered. The record itself stays comparable with
//! a tree that renders none, but the file reads `Session::blocks`, so it
//! copies into a tree from the act that renders the blocks onward. A tree
//! before that act takes this file as it stood at that act's base.
//!
//! **The tree before an act does not hold this file.** It is copied into that
//! checkout's `tests/` untracked for the run and removed after. Each record
//! carries the commit its tree stood at and that tree's `git status` lines,
//! so the copied file shows as untracked in the before record, and two
//! records from swapped or stale trees cannot pass as a before and an after.
//!
//! **The prompt is short and the generation decodes one token at a time**, so
//! a run never splits a batch into micro-batches and cannot observe
//! `n_ubatch`. That value, and every other set but flash attention, is the
//! engine's own default, which a host watch holds.
//!
//! Run with the lane's mask, as the harness launches the SPU (PAIR-1):
//!
//! ```text
//! CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=<ordinals> \
//! WEAVER_IDENTITY_ARTIFACT=<gguf> WEAVER_IDENTITY_OUT=<json> \
//! cargo test -p weaver-spu --features cuda,gguf --locked \
//!     --test engine_settings_identity -- --ignored --exact \
//!     record_one_generation_on_the_lane
//! ```
//!
//! The binding names the mask's ordinals, which is the lane: one ordinal is
//! the one-card lane and two are the layer split.

#![cfg(all(feature = "gguf", feature = "cuda"))]

use std::path::{Path, PathBuf};
use std::process::Command;

use weaver_spu::decoder::session::{NeverCancels, PositionedSinks, SamplerBuild, StopCondition};
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, Residency};
use weaver_spu::sampling::Sampling;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

/// Longer than the penalty window, so the window wraps inside the run.
const TOKENS: usize = 160;
const SEED: u64 = 4242;

fn named(variable: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| panic!("{variable} names this run's input"))
}

/// The commit the tree stood at and its `git status` lines, so a record
/// names the build that made it.
fn tree() -> serde_json::Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let git = |args: &[&str]| -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?} answers");
        String::from_utf8(out.stdout).expect("git answers in UTF-8")
    };
    serde_json::json!({
        "head": git(&["rev-parse", "HEAD"]).trim(),
        "status": git(&["status", "--porcelain"]).lines().collect::<Vec<_>>(),
    })
}

#[test]
#[ignore = "an instrument, run by name under a lane's mask"]
fn record_one_generation_on_the_lane() {
    let artifact = PathBuf::from(named("WEAVER_IDENTITY_ARTIFACT"));
    assert!(
        artifact.is_file(),
        "WEAVER_IDENTITY_ARTIFACT names {}, which is not a regular file",
        artifact.display()
    );
    let out = PathBuf::from(named("WEAVER_IDENTITY_OUT"));
    assert_eq!(
        std::env::var("CUDA_DEVICE_ORDER").as_deref(),
        Ok("PCI_BUS_ID"),
        "a mask names ordinals only under CUDA_DEVICE_ORDER=PCI_BUS_ID"
    );
    let devices: Vec<DeviceOrdinal> = named("CUDA_VISIBLE_DEVICES")
        .split(',')
        .map(|ordinal| DeviceOrdinal(ordinal.trim().parse().expect("a decimal ordinal")))
        .collect();

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(artifact.to_string_lossy().into_owned()),
        devices: devices.clone(),
        adapter: None,
    };
    let resident = residency
        .admit(
            &binding,
            Headroom(2 * 1024 * 1024 * 1024),
            4096,
            ReadoutElection(false),
            // Surprisal elected, so the record carries both signals.
            true,
        )
        .expect("the artifact admits");
    let prefix = resident
        .tokenize("Explain, in a short paragraph, why the sky looks blue.")
        .expect("the prompt tokenizes");
    // The frozen declaration the host watches use: a real declaration, so
    // the draw reads the distribution's values and not only their order.
    let sampling = Sampling {
        temperature: 0.7,
        top_k: 40,
        top_p: 0.95,
        repetition_penalty: 1.1,
        repetition_window: 64,
        seed: SEED,
    };
    let mut session = resident.open_session(4096).expect("the session opens");
    session.open(&prefix).expect("the prefix decodes");
    let terminator = resident
        .declared_eos()
        .expect("the artifact declares an end-of-sequence");
    // No stop token, so every run draws the same count and a stop drawn
    // early cannot shorten one side of the comparison.
    let generated = session
        .append_and_generate(
            &[],
            &StopCondition {
                stop_tokens: Vec::new(),
                terminator,
                max_tokens: TOKENS,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: SEED,
                sampling,
            },
        )
        .expect("the generation runs");

    let bits = |values: Option<&weaver_spu::measurement::NonEmpty<f32>>| -> Vec<u32> {
        values
            .map(|v| v.iter().map(|x| x.to_bits()).collect())
            .unwrap_or_default()
    };
    let record = serde_json::json!({
        "tree": tree(),
        "artifact": artifact.to_string_lossy(),
        "devices": devices.iter().map(|d| d.0).collect::<Vec<_>>(),
        "seed": SEED,
        "tokens": generated.tokens.iter().map(|t| t.0).collect::<Vec<_>>(),
        "entropy_bits": bits(generated.signals.entropy_bits.as_ref()),
        "surprisal_bits": bits(generated.signals.surprisal_bits.as_ref()),
    });
    assert_eq!(
        generated.tokens.len(),
        TOKENS,
        "the run drew every token it was asked for"
    );
    assert!(
        generated.signals.entropy_bits.is_some() && generated.signals.surprisal_bits.is_some(),
        "the run measured both signals, so the record compares more than the tokens"
    );
    std::fs::write(&out, format!("{record}\n")).expect("the record writes");
    if let Some(blocks) = session.blocks() {
        let theta = out.with_extension("theta.json");
        let text = format!(
            "{{\"engine\":{},\"device\":{}}}\n",
            blocks.engine.get(),
            blocks.device.get()
        );
        std::fs::write(&theta, text).expect("the theta blocks write");
        eprintln!("wrote the theta blocks to {}", theta.display());
    }
    eprintln!(
        "recorded {} tokens on {:?} to {}",
        generated.tokens.len(),
        devices,
        out.display()
    );
}
