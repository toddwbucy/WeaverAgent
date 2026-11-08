//! **The real 30B application proof of LORA-7, on one pinned input.**
//!
//! `weaver-spu-lora-loading-Spec-amendment` section 7: a base-only control,
//! repeated, and an adapted run, on the fixed base, fixture, input and
//! settings, under the backend's layer split on the binding's devices. The
//! adapted run must change at least one output token, the control must repeat,
//! and all three arms must show the same division and the same KV cache
//! records. Each arm runs in its own process (see `lora/fixture.rs`).
//!
//! An explicit accelerator instrument, run with `--ignored` under the
//! variables `lora/fixture.rs` reads and `WEAVER_LORA_PINNED_ID`, the entry to
//! decode. A missing input fails by name.
//!
//! Perturbation: run the adapted arm without its request (`adapted` false at
//! the third `run_arm`) and the output-change assertion fails, because the
//! third arm decodes the control again.
#![cfg(all(feature = "gguf", feature = "cuda"))]

#[path = "lora/fixture.rs"]
mod fixture;

use fixture::{
    Inputs, assert_adapter_on_its_layers, assert_covers, assert_division_on_binding,
    assert_same_division, first_difference, run_arm,
};

const TEST: &str = "the_adapter_changes_the_pinned_greedy_output_on_the_binding";

#[test]
#[ignore = "needs the base, the adapter, the entries and the binding's devices"]
fn the_adapter_changes_the_pinned_greedy_output_on_the_binding() {
    if fixture::child() {
        return;
    }
    let inputs = Inputs::from_env();
    let pinned = std::env::var("WEAVER_LORA_PINNED_ID")
        .expect("WEAVER_LORA_PINNED_ID is not set, so the fixture did not run");
    fixture::fresh_report(&inputs.report);
    let ids = vec![pinned.clone()];

    let control = run_arm(TEST, &inputs, "control", false, Some(&ids));
    let repeat = run_arm(TEST, &inputs, "control-repeat", false, Some(&ids));
    let adapted = run_arm(TEST, &inputs, "adapted", true, Some(&ids));

    assert_covers(&control, &ids);
    assert_covers(&repeat, &ids);
    assert_covers(&adapted, &ids);
    for arm in [&control, &repeat, &adapted] {
        assert_division_on_binding(arm, &inputs.devices);
    }
    assert_same_division(&control, &repeat);
    assert_same_division(&control, &adapted);
    assert_adapter_on_its_layers(&adapted);

    let base = &control.outputs[&pinned];
    assert_eq!(
        first_difference(base, &repeat.outputs[&pinned]),
        None,
        "the base-only control did not repeat"
    );
    let changed = first_difference(base, &adapted.outputs[&pinned]);
    let summary = serde_json::json!({
        "pinned": pinned,
        "control_tokens": base.len(),
        "adapted_tokens": adapted.outputs[&pinned].len(),
        "first_changed_position": changed,
        "control_repeated": true,
        "same_division": true,
    });
    std::fs::write(
        inputs.report.join("summary.json"),
        serde_json::to_string_pretty(&summary).expect("the summary serializes"),
    )
    .expect("the summary writes");
    assert!(
        changed.is_some(),
        "the adapted run changed no output token on {pinned}"
    );
    eprintln!("LORA-7 proof: {summary}");
}

fn records(lines: &[&str]) -> Vec<weaver_spu::lora::observation::Record> {
    lines
        .iter()
        .map(|line| weaver_spu::lora::observation::Record {
            level: 1,
            bytes: format!("{line}\n").into_bytes(),
        })
        .collect()
}

/// The shapes the pinned engine wrote on 2026-11-08 for a two-device layer
/// split, for a model of two repeating layers: assignments for layers 0, 1
/// and the output layer 2, KV records for layers 0 and 1. A repeated record
/// naming the same device is a second session's, and is accepted.
const TWO_LAYERS: &[&str] = &[
    "load_tensors: layer   0 assigned to device CUDA0, is_swa = 0",
    "load_tensors: layer   1 assigned to device CUDA1, is_swa = 0",
    "load_tensors: layer   2 assigned to device CUDA1, is_swa = 0",
    "load_tensors: layer   0 assigned to device CUDA0, is_swa = 0",
    "llama_kv_cache: layer   0: dev = CUDA0",
    "llama_kv_cache: layer   1: dev = CUDA1",
    "llama_kv_cache: layer   0: dev = CUDA0",
];

/// **The division parse reads the pinned engine's record wording**, so a
/// reworded record fails here on the host rather than on the device.
#[test]
fn the_division_parse_reads_the_engine_records() {
    let (division, kv) = fixture::division(&records(TWO_LAYERS), 2);
    assert_eq!(division.len(), 3);
    assert_eq!(division[&2], "CUDA1");
    assert_eq!(kv.len(), 2);
    assert_eq!(kv[&1], "CUDA1");
}

/// Perturbation of the above: a capture missing a repeating layer's KV
/// record refuses, where a comparison of two such captures would pass.
#[test]
#[should_panic(expected = "KV records are not one per repeating layer")]
fn a_capture_missing_a_kv_record_refuses() {
    fixture::division(&records(&TWO_LAYERS[..5]), 2);
}

/// A capture that lost a suffix refuses against the artifact's own layer
/// count, which a count read back from the capture would have accepted.
#[test]
#[should_panic(expected = "assignment records are not one per layer")]
fn a_capture_missing_a_suffix_refuses_against_the_artifact_count() {
    fixture::division(&records(TWO_LAYERS), 64);
}

/// Adapter targets map to the layer whose assignment judges their buffer.
#[test]
fn adapter_targets_map_to_their_layers() {
    assert_eq!(fixture::adapter_layer("blk.12.attn_k.weight", 64), 12);
    assert_eq!(fixture::adapter_layer("output.weight", 64), 64);
}

/// A target outside every assigned layer refuses rather than passing unjudged.
#[test]
#[should_panic(expected = "outside every assigned layer")]
fn an_adapter_target_outside_every_layer_refuses() {
    fixture::adapter_layer("token_embd.weight", 64);
}

/// A layer's KV cache on another device than its layer refuses, though both
/// devices are in the binding.
#[test]
#[should_panic(expected = "KV cache is on CUDA0, and the layer is assigned to CUDA1")]
fn a_kv_cache_off_its_layer_refuses() {
    let mut lines = TWO_LAYERS[..5].to_vec();
    lines.push("llama_kv_cache: layer   1: dev = CUDA0");
    fixture::division(&records(&lines), 2);
}

fn adapted_arm(placement: serde_json::Value) -> fixture::Arm {
    fixture::Arm {
        name: "adapted".into(),
        outputs: Default::default(),
        observation: serde_json::json!({
            "n_layer": 2,
            "division": {"0": "CUDA0", "1": "CUDA1", "2": "CUDA1"},
            "adapter_placement": placement,
        }),
    }
}

/// An adapter target on its layer's device passes.
#[test]
fn an_adapter_target_on_its_layer_passes() {
    fixture::assert_adapter_on_its_layers(&adapted_arm(serde_json::json!({
        "blk.0.attn_q.weight": "CUDA0",
        "blk.1.ffn_up.weight": "CUDA1",
    })));
}

/// An adapter target on the binding's other device than its layer refuses.
#[test]
#[should_panic(expected = "adapter target blk.1.ffn_up.weight is on")]
fn an_adapter_target_off_its_layer_refuses() {
    fixture::assert_adapter_on_its_layers(&adapted_arm(serde_json::json!({
        "blk.0.attn_q.weight": "CUDA0",
        "blk.1.ffn_up.weight": "CUDA0",
    })));
}
