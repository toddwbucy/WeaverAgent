//! conforms: spu-two-taps-one-shape
//! conforms: spu-elected-readout-changes-no-token
//!
//! **Charter section 13.7's bar, taken as a measurement.** An elected
//! readout changes no token: the same declaration and the same seed produce
//! the same token sequence with the election on and off. The clause says
//! shown before a tap ships and shown per tap rather than once for the
//! election, so this is the GGUF tap's demonstration and it says nothing
//! about the native one.
//!
//! **Why this could not be argued from the source.** The tap installs an
//! eval callback on the ggml scheduler, which changes how the graph is
//! computed rather than only what is read from it: one compute over a split
//! becomes a walk of windows with a synchronize after each, and a fusion
//! candidate straddling a window boundary is not applied. A fused kernel and
//! its unfused equivalent are not guaranteed bit-identical in floating
//! point, and `l_out-<il>` sits next to a normalise that is a fusion
//! candidate. `weaver-spu-Spec` section 7 says so in as many words and calls
//! the tap plausibly observational rather than provably so. **This file is
//! where that plausibility is retired**, on the real artifact and a real
//! device, which is the only place the hazard exists.
//!
//! The host-side reading in `decoder/gguf.rs` covers everything but this: it
//! runs on a backend whose scheduler has one backend and no fusion to lose.
//! Both are kept, because a failure here and a pass there would localise the
//! defect to the device path immediately.
#![cfg(all(feature = "gguf", feature = "cuda"))]

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use weaver_spu::decoder::backend::TokenId;
use weaver_spu::decoder::session::{NeverCancels, PositionedSinks, SamplerBuild, StopCondition};
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, Residency};
use weaver_spu::sampling::EffectiveKnobs;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

/// The device tests across this crate bind a card and read device-global
/// free memory, so they take a lock and run one at a time. This file's is
/// its own binary's, which is why the discipline is repeated rather than
/// shared.
fn device_lock() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE: OnceLock<Mutex<()>> = OnceLock::new();
    DEVICE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The artifact this workshop deploys, which is the one the claim is about.
///
/// **The demonstration is per family and this file names which.** Charter
/// section 13.7 has the bar shown per tap, and the registry entry it
/// authorises is `qwen35moe`, so pointing this at another artifact would
/// leave that entry's flag bought by a measurement of something else.
const ARTIFACT: &str = "/bulk-store/models/Qwen--Qwen-AgentWorld-35B-A3B-GGUF/\
                        Qwen-AgentWorld-35B-A3B-Q8_0.gguf";

/// The artifact's own `qwen35moe.block_count`, read from its GGUF metadata.
///
/// The reduction folds one figure per layer per forward, so a generation's
/// figures come back as a multiple of this. Naming it here rather than
/// deriving it from the reduction is deliberate: a count derived from the
/// thing under test would agree with itself whatever the tap did.
const LAYERS: usize = 40;

/// What this run's artifact owes per forward, named by the operator where
/// the artifact is not the default's.
///
/// **The count is named rather than derived**, for the reason the constant
/// above gives: a count taken from the thing under test would agree with
/// itself whatever the tap did. It moves with `WEAVER_ARTIFACT_READOUT`
/// because a layer count carried over from another family agrees with
/// nothing.
fn layers() -> usize {
    // `NotPresent` is the one arm that means nobody asked. A value that is
    // set and not Unicode is a named count this run cannot read, and taking
    // the default for it would run the demonstration against another
    // artifact's layer count.
    match std::env::var("WEAVER_ARTIFACT_READOUT_LAYERS") {
        Ok(named) => named
            .parse()
            .expect("WEAVER_ARTIFACT_READOUT_LAYERS names a layer count"),
        Err(std::env::VarError::NotPresent) => LAYERS,
        Err(other) => {
            panic!("WEAVER_ARTIFACT_READOUT_LAYERS is set and unreadable: {other}")
        }
    }
}

fn artifact() -> Option<PathBuf> {
    match std::env::var_os("WEAVER_ARTIFACT_READOUT") {
        // An explicit request that cannot be met is a failure rather than a
        // skip, per the convention `loaded.rs` states: a skip is a pass to
        // the harness, so a green run on a machine without the artifact
        // would report that the bar was cleared when nothing was measured.
        Some(named) => {
            let path = PathBuf::from(named);
            assert!(
                path.is_file(),
                "WEAVER_ARTIFACT_READOUT names {}, which is not a regular file",
                path.display()
            );
            Some(path)
        }
        None => {
            let path = PathBuf::from(ARTIFACT);
            path.is_file().then_some(path)
        }
    }
}

/// One generation under a declaration, answering what was drawn and what the
/// tap folded.
///
/// **The knobs are a real declaration rather than greedy.** A greedy draw
/// takes an argmax, which a small perturbation has to overturn to be seen,
/// and the charter's clause is about the declaration an operator writes.
/// Sampling at a fixed seed reads the distribution's values and not only
/// their order, so it is the more sensitive of the two probes.
fn draw(resident: &weaver_spu::residency::Resident, seed: u64) -> (Vec<TokenId>, Option<Vec<f32>>) {
    let prefix = resident
        .tokenize("Explain, in a short paragraph, why the sky looks blue.")
        .expect("the prompt tokenizes");
    let knobs = EffectiveKnobs {
        temperature: 0.7,
        top_k: 40,
        top_p: 0.95,
        repetition_penalty: 1.1,
        repetition_window: 64,
        seed,
    };
    let mut session = resident
        .open_session(&knobs, 4096)
        .expect("the session opens");
    session.open(&prefix).expect("the prefix decodes");
    let eos = resident
        .declared_eos()
        .expect("the artifact declares an end-of-sequence");
    let generated = session
        .append_and_generate(
            &[],
            &StopCondition {
                stop_tokens: vec![eos],
                terminator: eos,
                max_tokens: 64,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed,
                penalty_window: 64,
            },
        )
        .expect("the generation runs");
    (
        generated.tokens,
        generated.residual.map(|r| r.per_layer_norm().to_vec()),
    )
}

/// A fresh residency and its binding. **Fresh per election**, because a
/// residency admits once and the two halves of this comparison are two
/// admissions: the election is fixed for a residency at admit, which is the
/// property that makes it judgeable there.
fn binding_for(path: &Path) -> (Residency, ModelBinding) {
    (
        Residency::new(),
        ModelBinding {
            artifact: ArtifactRef(path.to_string_lossy().into_owned()),
            devices: vec![DeviceOrdinal(0)],
        },
    )
}

/// **An elected readout changes no token, on the device.**
///
/// Perturbation: have the tap accept the token it observed into the sampler,
/// or fold on the ask pass as well as the data pass, and the two sequences
/// diverge here while every count in the host-side tests stays right.
#[test]
fn an_elected_readout_changes_no_token_on_the_device() {
    let Some(path) = artifact() else {
        eprintln!("SKIP an_elected_readout_changes_no_token: no artifact at {ARTIFACT}");
        return;
    };
    if cudarc::driver::CudaContext::new(0).is_err() {
        eprintln!("SKIP an_elected_readout_changes_no_token: no CUDA device 0");
        return;
    }
    let _device = device_lock();
    // **Two seeds, and the pair is what makes the comparison mean
    // something.** Each seed is its own neutrality reading, and the two
    // together are the sensitivity check: a comparison that cannot tell two
    // sequences apart would pass this test whatever the tap did, and the
    // only way to know it can is to hand it two sequences that must differ.
    // This was a perturbation run by hand and is now a standing assertion.
    const SEEDS: [u64; 2] = [4242, 4243];

    // Unelected first, so the comparison's reference is taken by the path
    // that has no tap in it at all rather than by a tap asked to stand down.
    let (mut residency, binding) = binding_for(&path);
    let resident = residency
        .admit(
            &binding,
            Headroom(2 * 1024 * 1024 * 1024),
            ReadoutElection(false),
            false,
        )
        .expect("the artifact admits unelected");
    let without: Vec<Vec<TokenId>> = SEEDS
        .iter()
        .map(|seed| {
            let (drawn, no_norms) = draw(resident, *seed);
            assert!(
                no_norms.is_none(),
                "an unelected load answered a reduction, so the election is not what \
                 gates the tap"
            );
            drawn
        })
        .collect();
    // **The residency is what holds the card**, `admit` answering a borrow
    // of a resident the residency owns, so dropping the borrow frees nothing
    // and this artifact would still be resident when the elected admit runs.
    // Two copies do not fit on one card, so getting this wrong fails loudly
    // rather than quietly, but it is worth naming: the owner is the thing to
    // drop.
    drop(residency);

    let (mut residency, binding) = binding_for(&path);
    let resident = residency
        .admit(
            &binding,
            Headroom(2 * 1024 * 1024 * 1024),
            ReadoutElection(true),
            false,
        )
        .expect("the artifact admits with readout elected, which is the flag's claim");
    let elected: Vec<(Vec<TokenId>, Vec<f32>)> = SEEDS
        .iter()
        .map(|seed| {
            let (drawn, norms) = draw(resident, *seed);
            (drawn, norms.expect("an elected load answers a reduction"))
        })
        .collect();

    // **The comparison is known to discriminate.** The two seeds must draw
    // different sequences, or an equality that holds for every input would
    // be reading nothing and the neutrality assertions below would pass on
    // a tap that rewrote every token.
    assert_ne!(
        without[0], without[1],
        "the two seeds drew the same sequence, so this comparison cannot tell \
         sequences apart and proves nothing about the tap"
    );

    for (index, seed) in SEEDS.iter().enumerate() {
        let (with, norms) = &elected[index];

        // **The elected run must have tapped**, or the comparison is between
        // two uninstrumented runs and proves nothing.
        assert!(
            norms.iter().all(|n| n.is_finite() && *n > 0.0),
            "seed {seed}: the figures are the model's rather than an unwritten buffer"
        );
        // **An elected load that folded nothing is named here**, or the
        // comparison below indexes an empty run and the panic blames the
        // harness for what the tap did.
        assert!(
            !norms.is_empty(),
            "seed {seed}: the elected load answered a reduction with no figures in it"
        );
        assert!(
            norms.iter().any(|n| *n != norms[0]),
            "seed {seed}: a constant run of figures means the wrong column"
        );

        // **A thin comparison proves little.** A run that stopped after one
        // token would satisfy an equality without exercising the tap across
        // a generation, so the reference has to be a real one.
        assert!(
            with.len() >= 32,
            "seed {seed}: the run drew {} tokens, too few to compare",
            with.len()
        );
        // **The count is the invariant rather than a constant.** One forward
        // for the prefix and one per token drawn, so the figures are fixed by
        // what the generation did rather than by a number written here, which
        // would pin the model's stopping behaviour instead of the tap's
        // arithmetic.
        assert_eq!(
            norms.len(),
            (with.len() + 1) * layers(),
            "seed {seed}: {} figures against {} forwards at {} layers",
            norms.len(),
            with.len() + 1,
            layers()
        );

        assert_eq!(
            *with, without[index],
            "seed {seed}: the elected run drew a different sequence, so this tap is not \
             observational on this device and the election cannot ship for this family"
        );
    }
    let norms = &elected[0].1;

    // **The measurement states itself.** A demonstration whose output is a
    // bare `ok` leaves a later reader to take on faith that it compared
    // anything, and this one exists precisely because the property could not
    // be taken on faith.
    println!(
        "  readout neutrality, {} on device 0, {} seeds: {} and {} tokens drawn, \
         each identical with the election on and off, {} figures folded over {} \
         forwards at {} layers",
        path.file_name().unwrap_or_default().to_string_lossy(),
        SEEDS.len(),
        elected[0].0.len(),
        elected[1].0.len(),
        norms.len(),
        norms.len() / layers(),
        layers(),
    );
}
