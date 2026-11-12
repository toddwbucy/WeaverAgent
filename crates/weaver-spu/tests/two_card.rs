//! A pair binding, admitted and served, split by layer across two cards by
//! the computed even division of the device pair amendment PAIR-3: twenty of
//! the forty layers on each card for both default artifacts, the output
//! layer on the second, and both cards back to idle at release.
//!
//! **Each binding runs in a masked child.** PAIR-1 has the SPU refuse a
//! binding whose ordinals its process mask does not name, in order, and one
//! process holds one mask, so the single-card probes and the pair each run in
//! their own process of this binary under that binding's
//! `CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=<ordinals>`
//! (`common::run_masked`). The parent reads the lease and serializes the
//! children. Run the binary with the pair leased,
//! `CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=0,1` or no mask at all.
//! With the cards hidden both tests skip.
//!
//! **A split GGUF rides this path since #200, through the pin rather than
//! around it.** llama.cpp finds `-00002-of-00002` siblings by the filename
//! pattern, and the descriptor's `/proc/self/fd/N` carries none, so the pin
//! reaches every shard and the loader names the whole set through the
//! fork's explicit splits door. Handing the real path back would have undone
//! what the pin is for.
#![cfg(all(feature = "gguf", feature = "cuda"))]

mod common;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use weaver_spu::decoder::backend::TokenId;
use weaver_spu::decoder::session::{NeverCancels, PositionedSinks, SamplerBuild, StopCondition};
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, Residency};
use weaver_spu::sampling::Sampling;
use weaver_types::{ArtifactRef, ModelBinding};

/// The pair, by ordinal.
const PAIR: &[u32] = &[0, 1];

/// Every parent in this binary takes the lock around its children, so no
/// two-card measurement races another for the same devices, which is
/// `loaded.rs`'s discipline carried over.
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

/// **Each card holds most of the share the room judgment asked of it**, the
/// floor restated against the per-device share (device pair amendment
/// PAIR-4, section 5). For the default qwen35moe artifacts that share is
/// the even fraction of the file, which over-asks the first card by half the
/// input layer the engine keeps in host memory. A floor at three quarters of
/// the share catches a one-card load masquerading as a pair, and a division
/// far from the one computed, without flaking on allocator granularity.
fn floor(need: u64) -> u64 {
    need / 4 * 3
}

/// **The division is the computed even one**: two counts that differ by at
/// most one layer, carried on the resident.
fn assert_even_division(resident: &weaver_spu::residency::Resident) {
    let division = resident
        .placement
        .division
        .as_ref()
        .expect("a GGUF residency carries its division");
    let counts = division.counts();
    assert_eq!(counts.len(), 2, "one count per card: {counts:?}");
    assert!(
        counts[0].abs_diff(counts[1]) <= 1,
        "the division is even: {counts:?}"
    );
    eprintln!("division {counts:?}, needs {:?}", resident.placement.needs);
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
    let sampling = Sampling {
        temperature: 0.0,
        top_k: 1,
        top_p: 1.0,
        repetition_penalty: 1.0,
        repetition_window: 0,
        seed: 11,
    };
    let mut session = resident.open_session(4096).expect("session opens");
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
            SamplerBuild { seed: 11, sampling },
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
    if split_artifact().is_none() {
        eprintln!("SKIP: no split artifact");
        return;
    }
    if let Err(why) = common::lease(PAIR) {
        eprintln!("SKIP: {why}");
        return;
    }
    let _device = device_lock();
    // **Larger than any card is asserted, not narrated**: the same artifact
    // on either card alone refuses at the room judgment, which is free, so
    // the claim the pair admission rests on is exercised before it is
    // relied on. A child per probe, each under its own card's mask.
    for ordinal in [0u32, 1] {
        common::run_masked("a_split_artifact_probe_child", &[ordinal]);
    }
    common::run_masked(
        "a_split_artifact_larger_than_any_card_admits_across_the_pair_child",
        PAIR,
    );
}

/// The split artifact, `None` where the default is absent.
fn split_artifact() -> Option<PathBuf> {
    match std::env::var_os("WEAVER_ARTIFACT_SPLIT") {
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
            Some(path)
        }
        None => {
            let path = PathBuf::from(
                "/bulk-store/models/unsloth--Qwen3.6-35B-A3B-GGUF/BF16/Qwen3.6-35B-A3B-BF16-00001-of-00002.gguf",
            );
            path.is_file().then_some(path)
        }
    }
}

/// A masked child of
/// [`a_split_artifact_larger_than_any_card_admits_across_the_pair`]: the
/// split on one card, which must refuse for room.
#[test]
#[ignore = "a masked child, run by its parent"]
fn a_split_artifact_probe_child() {
    let Some(devices) = common::masked_child("a_split_artifact_probe_child") else {
        return;
    };
    let path = split_artifact().expect("the parent found the artifact");
    let ordinal = devices[0].0;
    let mut probe = Residency::new();
    let single = ModelBinding {
        artifact: ArtifactRef(path.to_string_lossy().into_owned()),
        devices,
        adapter: None,
    };
    let refused = probe.admit(
        &single,
        Headroom(2 * 1024 * 1024 * 1024),
        4096,
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
    common::report(&serde_json::json!("done"));
}

/// A masked child of
/// [`a_split_artifact_larger_than_any_card_admits_across_the_pair`]: the
/// split across the pair, served and released.
#[test]
#[ignore = "a masked child, run by its parent"]
fn a_split_artifact_larger_than_any_card_admits_across_the_pair_child() {
    let Some(devices) =
        common::masked_child("a_split_artifact_larger_than_any_card_admits_across_the_pair_child")
    else {
        return;
    };
    let path = split_artifact().expect("the parent found the artifact");
    let card0 = cudarc::driver::CudaContext::new(0).expect("the first card under the mask");
    let card1 = cudarc::driver::CudaContext::new(1).expect("the second card under the mask");
    let before = (free_bytes(&card0), free_bytes(&card1));

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(path.to_string_lossy().into_owned()),
        devices,
        adapter: None,
    };
    let resident = residency
        .admit(
            &binding,
            Headroom(2 * 1024 * 1024 * 1024),
            4096,
            ReadoutElection(false),
            false,
        )
        .expect("the split admits across the pair");
    assert_even_division(resident);

    // Larger than either card alone: each holds a share no one card could
    // spare beside the other's, which is the fact this artifact class is for.
    let needs = resident.placement.needs.clone();
    let held = (free_bytes(&card0), free_bytes(&card1));
    assert!(
        before.0 - held.0 > floor(needs[0]),
        "ordinal 0 holds a major share: {} -> {} against a share of {}",
        before.0,
        held.0,
        needs[0]
    );
    assert!(
        before.1 - held.1 > floor(needs[1]),
        "ordinal 1 holds a major share: {} -> {} against a share of {}",
        before.1,
        held.1,
        needs[1]
    );

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
    common::report(&serde_json::json!("done"));
}

/// **A pair binding admitted and served, the model split by layer across two
/// cards** (#23): what this measures, named for it. The default artifact,
/// 35 GiB, fits one 48 GiB card, as `weaver-spu-Spec` section 7 records of
/// the same artifact run on one card, so a model larger than any single card
/// is its neighbour's claim, `a_split_artifact_larger_than_any_card_admits_across_the_pair`.
#[test]
fn a_pair_binding_admits_split_by_layer_and_serves() {
    if artifact().is_none() {
        eprintln!("SKIP: no two-card artifact");
        return;
    }
    if let Err(why) = common::lease(PAIR) {
        eprintln!("SKIP: {why}");
        return;
    }
    let _device = device_lock();
    common::run_masked(
        "a_pair_binding_admits_split_by_layer_and_serves_child",
        PAIR,
    );
}

/// The masked child of [`a_pair_binding_admits_split_by_layer_and_serves`]:
/// the pair admitted, served and released.
#[test]
#[ignore = "a masked child, run by its parent"]
fn a_pair_binding_admits_split_by_layer_and_serves_child() {
    let Some(devices) =
        common::masked_child("a_pair_binding_admits_split_by_layer_and_serves_child")
    else {
        return;
    };
    let path = artifact().expect("the parent found the artifact");
    let card0 = cudarc::driver::CudaContext::new(0).expect("the first card under the mask");
    let card1 = cudarc::driver::CudaContext::new(1).expect("the second card under the mask");
    let before = (free_bytes(&card0), free_bytes(&card1));

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(path.to_string_lossy().into_owned()),
        devices,
        adapter: None,
    };
    let resident = match residency.admit(
        &binding,
        Headroom(2 * 1024 * 1024 * 1024),
        4096,
        ReadoutElection(false),
        false,
    ) {
        Ok(resident) => resident,
        Err(refusal) => panic!("the pair admit: {refusal:?}"),
    };
    assert_even_division(resident);
    // **Both cards hold a substantial share**, against the share the room
    // judgment asked of each.
    let needs = resident.placement.needs.clone();
    let held = (free_bytes(&card0), free_bytes(&card1));
    assert!(
        before.0 - held.0 > floor(needs[0]),
        "ordinal 0 holds a share: {} -> {} against a share of {}",
        before.0,
        held.0,
        needs[0]
    );
    assert!(
        before.1 - held.1 > floor(needs[1]),
        "ordinal 1 holds a share: {} -> {} against a share of {}",
        before.1,
        held.1,
        needs[1]
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
    common::report(&serde_json::json!("done"));
}
