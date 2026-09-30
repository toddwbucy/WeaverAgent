//! A pair binding, admitted and served: the first exercise of the two-device
//! load path since it was written. Measured 2026-08-17: the artifact splits
//! by layer, 18.2 GiB on ordinal 0 beside 17.0 GiB on ordinal 1, and both
//! cards return to idle at release.
//!
//! **A split GGUF rides this path since #200, through the pin rather than
//! around it.** llama.cpp finds `-00002-of-00002` siblings by the filename
//! pattern, and the descriptor's `/proc/self/fd/N` carries none, so the pin
//! reaches every shard and the loader names the whole set through the
//! fork's explicit splits door. Handing the real path back would have undone
//! what the pin is for.
#![cfg(all(feature = "gguf", feature = "cuda"))]

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use weaver_spu::decoder::backend::TokenId;
use weaver_spu::decoder::session::{NeverCancels, PositionedSinks, SamplerBuild, StopCondition};
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, Residency};
use weaver_spu::sampling::EffectiveKnobs;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

/// Every test in this binary takes the lock, so no two-card measurement
/// races another for the same devices, which is `loaded.rs`'s discipline
/// carried over.
fn device_lock() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE: OnceLock<Mutex<()>> = OnceLock::new();
    DEVICE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn free_bytes(context: &cudarc::driver::CudaContext) -> u64 {
    let (free, _total) = context.mem_get_info().expect("the driver answers");
    free as u64
}

/// One raw generation with no family assumption: a plain prompt, the
/// artifact's own declared end-of-sequence as the stop, greedy sampling.
/// The subject of this file is the pair load, not any template, so an
/// operator overriding a fixture with another family's split reaches no
/// marker this helper never promised.
fn generate_plain(resident: &weaver_spu::residency::Resident) -> (Vec<TokenId>, String) {
    let prefix = resident
        .tokenize("The capital of France is")
        .expect("a plain prompt tokenizes");
    let knobs = EffectiveKnobs {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let mut session = resident.open_session(&knobs, 4096).expect("session opens");
    session.open(&prefix).expect("prefix decodes");
    let eos = resident
        .declared_eos()
        .expect("the artifact declares an end-of-sequence");
    let generated = session
        .append_and_generate(
            &[],
            &StopCondition {
                stop_tokens: vec![eos],
                terminator: eos,
                max_tokens: 12,
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
    (generated.tokens, text)
}

fn artifact() -> Option<PathBuf> {
    match std::env::var_os("WEAVER_ARTIFACT_TWO_CARD") {
        Some(named) => {
            let path = PathBuf::from(named);
            assert!(
                path.is_file(),
                "WEAVER_ARTIFACT_TWO_CARD names a missing file"
            );
            Some(path)
        }
        None => {
            let path = PathBuf::from(
                "/bulk-store/models/Qwen--Qwen-AgentWorld-35B-A3B-GGUF/Qwen-AgentWorld-35B-A3B-Q8_0.gguf",
            );
            path.is_file().then_some(path)
        }
    }
}

/// The split artifact, the class the ruling on the collision reopened: a
/// 64.6 GiB set across two files, larger than any single card this box
/// holds, admitted whole through the pinned set and the fork's explicit
/// splits door.
#[test]
fn a_split_artifact_larger_than_any_card_admits_across_the_pair() {
    let path = match std::env::var_os("WEAVER_ARTIFACT_SPLIT") {
        Some(named) => {
            let path = PathBuf::from(named);
            assert!(path.is_file(), "WEAVER_ARTIFACT_SPLIT names a missing file");
            // The same classifier the pin answers to: a single-file override
            // would pass this test through the single-file door while the
            // test's name promises the splits door was exercised.
            assert!(
                weaver_spu::artifact::names_a_split(&path),
                "WEAVER_ARTIFACT_SPLIT must name a split shard: {}",
                path.display()
            );
            path
        }
        None => {
            let path = PathBuf::from(
                "/bulk-store/models/unsloth--Qwen3.6-35B-A3B-GGUF/BF16/Qwen3.6-35B-A3B-BF16-00001-of-00002.gguf",
            );
            if !path.is_file() {
                eprintln!("SKIP: no split artifact");
                return;
            }
            path
        }
    };
    let (Some(card0), Some(card1)) = (
        cudarc::driver::CudaContext::new(0).ok(),
        cudarc::driver::CudaContext::new(1).ok(),
    ) else {
        eprintln!("SKIP: fewer than two CUDA devices");
        return;
    };
    let _device = device_lock();
    let before = (free_bytes(&card0), free_bytes(&card1));

    // **Larger than any card is asserted, not narrated**: the same artifact
    // on either card alone refuses at the room judgment, which is free, so
    // the claim the pair admission rests on is exercised before it is
    // relied on. Fresh residencies per probe, because a residency admits
    // once and a probe must not consume the one the pair uses.
    for ordinal in [0u32, 1] {
        let mut probe = Residency::new();
        let single = ModelBinding {
            artifact: ArtifactRef(path.to_string_lossy().into_owned()),
            devices: vec![DeviceOrdinal(ordinal)],
        };
        let refused = probe.admit(
            &single,
            Headroom(2 * 1024 * 1024 * 1024),
            ReadoutElection(false),
            false,
        );
        // The refusal must be the room refusal on the probed card, or the
        // probe proves nothing: any other refusal - a family miss, a bad
        // path - would satisfy a bare is_err while the size claim went
        // unexercised.
        match refused.as_ref().err() {
            Some(weaver_spu::residency::AdmitRefusal::DeviceRefused(
                weaver_spu::gpu::DeviceRefusal::NoRoom {
                    ordinal: refused_at,
                    free,
                    needed,
                    total,
                },
            )) => {
                assert_eq!(*refused_at, ordinal, "the refusal names the probed card");
                assert!(
                    needed > free,
                    "the refusal carries the inequality: needed {needed} against free {free}"
                );
                // **The capacity is what makes the inequality readable**, per
                // Spec section 5. Needed against free says the load did not
                // fit. Only the capacity says whether the card is too small or
                // held by something else, and this probe is the first case: an
                // idle card whose whole capacity is under the need.
                assert!(
                    *total >= *free,
                    "a card cannot have more free than it holds: \
                     free {free}, total {total}"
                );
                assert!(
                    needed > total,
                    "this probe is the too-small reading, so the need exceeds \
                     the whole card rather than merely what was free: \
                     needed {needed}, total {total}"
                );
            }
            other => panic!(
                "the split fits no single card, so ordinal {ordinal} refuses for room, got {other:?}"
            ),
        }
    }

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(path.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0), DeviceOrdinal(1)],
    };
    let resident = residency
        .admit(
            &binding,
            Headroom(2 * 1024 * 1024 * 1024),
            ReadoutElection(false),
            false,
        )
        .expect("the split admits across the pair");

    // Larger than either card alone: each holds a share no one card could
    // spare beside the other's, which is the fact this artifact class is for.
    let floor = 24u64 * 1024 * 1024 * 1024;
    let held = (free_bytes(&card0), free_bytes(&card1));
    assert!(before.0 - held.0 > floor, "ordinal 0 holds a major share");
    assert!(before.1 - held.1 > floor, "ordinal 1 holds a major share");

    let (generated, text) = generate_plain(resident);
    eprintln!("split emission: {text:?}");
    assert!(!generated.is_empty());

    let _ = resident;
    residency.release().expect("the release succeeds");
    let after = (free_bytes(&card0), free_bytes(&card1));
    let tolerance: u64 = 1024 * 1024 * 1024;
    assert!(
        after.0.abs_diff(before.0) < tolerance,
        "ordinal 0 returns to baseline in both directions: {} -> {}",
        before.0,
        after.0
    );
    assert!(
        after.1.abs_diff(before.1) < tolerance,
        "ordinal 1 returns to baseline in both directions: {} -> {}",
        before.1,
        after.1
    );
}

#[test]
fn a_model_larger_than_one_card_admits_across_a_pair() {
    let Some(path) = artifact() else {
        eprintln!("SKIP: no two-card artifact");
        return;
    };
    let (Some(card0), Some(card1)) = (
        cudarc::driver::CudaContext::new(0).ok(),
        cudarc::driver::CudaContext::new(1).ok(),
    ) else {
        eprintln!("SKIP: fewer than two CUDA devices");
        return;
    };
    let _device = device_lock();
    let before = (free_bytes(&card0), free_bytes(&card1));

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(path.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0), DeviceOrdinal(1)],
    };
    let resident = match residency.admit(
        &binding,
        Headroom(2 * 1024 * 1024 * 1024),
        ReadoutElection(false),
        false,
    ) {
        Ok(resident) => resident,
        Err(refusal) => panic!("the pair admit: {refusal:?}"),
    };
    // **Both cards hold a substantial share, asserted with thresholds well
    // under an even split**: the artifact is 34.4 GiB and a layer split is
    // not exactly even, so each card owing 8 GiB catches a one-card load
    // masquerading as a pair without flaking on allocator granularity.
    let floor = 8u64 * 1024 * 1024 * 1024;
    let held = (free_bytes(&card0), free_bytes(&card1));
    assert!(
        before.0 - held.0 > floor,
        "ordinal 0 holds a share: {} -> {}",
        before.0,
        held.0
    );
    assert!(
        before.1 - held.1 > floor,
        "ordinal 1 holds a share: {} -> {}",
        before.1,
        held.1
    );
    eprintln!("ADMITTED across two devices");

    let (generated, text) = generate_plain(resident);
    eprintln!("two-card emission: {text:?}");
    assert!(!generated.is_empty());

    // **The release frees both cards.** The session and the resident borrow
    // the residency, so both end before it releases, and the assertion reads
    // the devices rather than trusting the drop order.
    let _ = resident;
    residency.release().expect("the release succeeds");
    let after = (free_bytes(&card0), free_bytes(&card1));
    let tolerance: u64 = 1024 * 1024 * 1024;
    assert!(
        after.0.abs_diff(before.0) < tolerance,
        "ordinal 0 returns to baseline in both directions: {} -> {}",
        before.0,
        after.0
    );
    assert!(
        after.1.abs_diff(before.1) < tolerance,
        "ordinal 1 returns to baseline in both directions: {} -> {}",
        before.1,
        after.1
    );
}
