//! **The observed division matches the computed one**, the device pair
//! amendment PAIR-5 instrument: on a device pair, under the LORA-3 log
//! capture at debug level, the engine's assignment record for every layer and
//! for the output, and the cache records the artifact's PAIR-4 classification
//! predicts, each name the device PAIR-3 gives that layer. The judgment is
//! `lora::observation::judge_division`, whose deliberate changes are watched
//! without a device in its unit tests. This file is where it meets the
//! engine.
//!
//! An explicit accelerator test, like the LORA-7 fixture, run with
//! `--ignored` under one variable: `WEAVER_ARTIFACT_DIVISION` names the GGUF
//! artifact (its first shard, for a split set). A missing artifact fails by
//! name, and so does a missing device pair: this test is never a pass for
//! what it did not observe. Run it with the pair leased,
//! `CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=0,1` or no mask at all.
//!
//! **It runs in an isolated process with the observer.** The observer is
//! installed before any admission and never removed, and production serving
//! installs no log callback, so the parent here judges only the lease and
//! the child, a process of this binary under the binding's PAIR-1 mask
//! (`common::run_masked`), installs the observer, admits, opens one session
//! so the KV cache is allocated, and judges the records.
//!
//! **It also makes PAIR-5's non-member check**: with the model loaded, the
//! driver's process table lists this process on the binding's devices and on
//! no other. Read through `nvidia-smi`, which ignores the process mask and
//! sees every device, so a box with a device outside the binding (olympus
//! holds three) shows a context the mask failed to keep out.
#![cfg(all(feature = "cuda", feature = "gguf"))]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use weaver_spu::artifact;
use weaver_spu::lora::Fixture;
use weaver_spu::lora::observation::judge_division;
use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::Headroom;
use weaver_spu::room::Layout;
use weaver_spu::sampling::EffectiveKnobs;
use weaver_types::{ArtifactRef, ModelBinding};

/// The pair, by ordinal.
const PAIR: &[u32] = &[0, 1];

/// The context positions the room judgment charges and the session opens.
const POSITIONS: u32 = 4096;

fn required_artifact() -> PathBuf {
    let value = std::env::var_os("WEAVER_ARTIFACT_DIVISION").unwrap_or_else(|| {
        panic!("WEAVER_ARTIFACT_DIVISION is not set, so no division was observed")
    });
    let path = PathBuf::from(value);
    assert!(
        path.is_file(),
        "WEAVER_ARTIFACT_DIVISION names {}, which is not a file",
        path.display()
    );
    path
}

/// The PCI-bus ordinals of the devices whose driver process table lists
/// `pid`. The ordinal is the device's place among all devices sorted by bus
/// id, the numbering PAIR-1 gives a binding, read from `nvidia-smi`, which
/// answers for every device whatever this process's mask.
fn devices_holding(pid: u32) -> BTreeSet<u32> {
    let query = |args: &[&str]| -> String {
        let output = Command::new("nvidia-smi")
            .args(args)
            .output()
            .unwrap_or_else(|error| {
                panic!("nvidia-smi did not run, so the non-member check was not made: {error}")
            });
        assert!(
            output.status.success(),
            "nvidia-smi {args:?} failed, so the non-member check was not made: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("nvidia-smi answers UTF-8")
    };
    let mut cards: Vec<(String, String)> =
        query(&["--query-gpu=pci.bus_id,uuid", "--format=csv,noheader"])
            .lines()
            .map(|line| {
                let (bus, uuid) = line.split_once(',').expect("a bus id and a uuid");
                (bus.trim().to_string(), uuid.trim().to_string())
            })
            .collect();
    cards.sort();
    let ordinal: BTreeMap<String, u32> = cards
        .into_iter()
        .enumerate()
        .map(|(index, (_, uuid))| (uuid, index as u32))
        .collect();
    query(&["--query-compute-apps=pid,gpu_uuid", "--format=csv,noheader"])
        .lines()
        .filter_map(|line| {
            let (holder, uuid) = line.split_once(',')?;
            (holder.trim().parse::<u32>().ok()? == pid).then(|| {
                *ordinal
                    .get(uuid.trim())
                    .unwrap_or_else(|| panic!("a process on an unlisted device {uuid}"))
            })
        })
        .collect()
}

#[test]
#[ignore = "an accelerator instrument: needs WEAVER_ARTIFACT_DIVISION and a leased device pair"]
fn the_observed_division_matches_the_computed_one_on_the_pair() {
    required_artifact();
    if let Err(why) = common::lease(PAIR) {
        panic!("the device pair is not leased, so no division was observed: {why}");
    }
    let reported = common::run_masked(
        "the_observed_division_matches_the_computed_one_on_the_pair_child",
        PAIR,
    );
    eprintln!("observed division: {reported:?}");
}

/// The masked child of
/// [`the_observed_division_matches_the_computed_one_on_the_pair`]: observe,
/// admit, open, judge.
#[test]
#[ignore = "a masked child, run by its parent"]
fn the_observed_division_matches_the_computed_one_on_the_pair_child() {
    let Some(devices) =
        common::masked_child("the_observed_division_matches_the_computed_one_on_the_pair_child")
    else {
        return;
    };
    let path = required_artifact();
    let reference = ArtifactRef(path.to_string_lossy().into_owned());

    // The prediction, from the artifact's own directory through the same pin
    // the admission takes.
    let resolved = artifact::resolve(&reference).expect("the artifact resolves");
    let pinned = artifact::pin(&resolved).expect("the artifact pins");
    let layout = Layout::read(&pinned.paths()).expect("the layout reads");
    let caches = layout.caches().unwrap_or_else(|| {
        panic!(
            "{} is recurrent or hybrid, whose cache classification is the device pair \
             work's second child act, so its records are not predicted here",
            layout.architecture()
        )
    });
    drop(pinned);

    let mut fixture = Fixture::new().expect("the observer installs before any admission");
    let binding = ModelBinding {
        artifact: reference,
        devices,
    };
    let (placement, holding) = {
        let resident = fixture
            .admit(
                &binding,
                Headroom(2 * 1024 * 1024 * 1024),
                POSITIONS,
                ReadoutElection(false),
                false,
                None,
            )
            .unwrap_or_else(|refusal| panic!("the pair admits: {refusal:?}"));
        let knobs = EffectiveKnobs {
            temperature: 0.0,
            top_k: 1,
            top_p: 1.0,
            repetition_penalty: 1.0,
            repetition_window: 0,
            seed: 11,
        };
        // One session, so the KV cache is allocated once and records once.
        let session = resident
            .open_session(&knobs, POSITIONS)
            .expect("a session opens, allocating the cache");
        drop(session);
        // **No context on a non-member device**, read while the model is
        // resident.
        (
            resident.placement.clone(),
            devices_holding(std::process::id()),
        )
    };
    let bound: BTreeSet<u32> = binding.devices.iter().map(|d| d.0).collect();
    assert_eq!(
        holding, bound,
        "this process holds a context on {holding:?}, where the binding is {bound:?}"
    );
    let division = placement
        .division
        .as_ref()
        .expect("a GGUF residency carries its division");
    let records = fixture.records().expect("the capture holds every record");

    // The evidence the fixture report carries: the computed division and the
    // observed records, raw.
    eprintln!(
        "computed division {:?} over {} layers, needs {:?}, input layer {:?} bytes on the host",
        division.counts(),
        division.layers(),
        placement.needs,
        placement.input_bytes
    );
    for record in &records {
        let text = String::from_utf8_lossy(&record.bytes);
        for line in text.lines().filter(|line| {
            line.starts_with("load_tensors: layer ")
                || line.starts_with("llama_kv_cache: layer ")
                || line.starts_with("llama_memory_recurrent, layer ")
        }) {
            eprintln!("record (level {}): {line}", record.level);
        }
    }

    let observed = judge_division(&records, &binding.devices, division, &caches)
        .unwrap_or_else(|faults| panic!("the observed division departs from PAIR-3: {faults}"));
    common::report(&serde_json::json!({
        "counts": division.counts(),
        "slots": observed.slots.iter().map(|d| d.0).collect::<Vec<_>>(),
        "attention": observed.attention.iter().map(|d| d.as_ref().map(|d| d.0)).collect::<Vec<_>>(),
        "visible": placement.visible,
        "holding": holding,
    }));
    fixture.release().expect("the release succeeds");
}
