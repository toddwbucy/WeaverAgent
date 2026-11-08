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
    Inputs, assert_covers, assert_division_on_binding, assert_same_division, first_difference,
    run_arm,
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

/// **The division parse reads the pinned engine's record wording.** The lines
/// are the shapes the pinned engine wrote on 2026-11-08 for a two-device
/// layer split (`load_tensors` and `llama_kv_cache`), so a reworded record
/// fails here on the host rather than as an empty division on the device.
#[test]
fn the_division_parse_reads_the_engine_records() {
    let (division, kv) = fixture::division(&records(&[
        "load_tensors: layer   0 assigned to device CUDA0, is_swa = 0",
        "load_tensors: layer   1 assigned to device CUDA1, is_swa = 0",
        "load_tensors: layer   2 assigned to device CUDA1, is_swa = 0",
        "load_tensors: layer   0 assigned to device CUDA0, is_swa = 0",
        "llama_kv_cache: layer   0: dev = CUDA0",
        "llama_kv_cache: layer   1: dev = CUDA1",
        "llama_kv_cache: layer   0: dev = CUDA0",
    ]));
    assert_eq!(division.len(), 3);
    assert_eq!(division[&2], "CUDA1");
    assert_eq!(kv.len(), 2);
    assert_eq!(kv[&1], "CUDA1");
}

/// Perturbation of the above: a capture missing a repeating layer's KV
/// record refuses, where a set comparison of two such captures would pass.
#[test]
#[should_panic(expected = "KV records are not one per repeating layer")]
fn a_partial_capture_refuses() {
    fixture::division(&records(&[
        "load_tensors: layer   0 assigned to device CUDA0, is_swa = 0",
        "load_tensors: layer   1 assigned to device CUDA1, is_swa = 0",
        "load_tensors: layer   2 assigned to device CUDA1, is_swa = 0",
        "llama_kv_cache: layer   0: dev = CUDA0",
    ]));
}
