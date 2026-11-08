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

mod common;
#[path = "lora/fixture.rs"]
mod fixture;

use fixture::{Inputs, assert_covers, assert_same_division, first_difference, run_arm};
use weaver_spu::division::Division;
use weaver_spu::lora::observation::{Record, judge_division};
use weaver_spu::room::Caches;
use weaver_types::DeviceOrdinal;

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
    if let Err(why) = common::lease(&inputs.ordinals()) {
        panic!("the binding's devices are not leased, so the fixture did not run: {why}");
    }
    fixture::fresh_report(&inputs.report);
    let ids = vec![pinned.clone()];

    let control = run_arm(TEST, &inputs, "control", false, Some(&ids));
    let repeat = run_arm(TEST, &inputs, "control-repeat", false, Some(&ids));
    let adapted = run_arm(TEST, &inputs, "adapted", true, Some(&ids));

    assert_covers(&control, &ids);
    assert_covers(&repeat, &ids);
    assert_covers(&adapted, &ids);
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

fn records(lines: &[&str]) -> Vec<Record> {
    lines
        .iter()
        .map(|line| Record {
            level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
            bytes: format!("{line}\n").into_bytes(),
        })
        .collect()
}

/// The shapes the pinned engine writes for a two-device layer split of a
/// model with two repeating layers, opened in two sessions. The assignment
/// records come from the load, once: layers 0 and 1 and the output layer 2.
/// Each session's context writes the KV records of layers 0 and 1 again.
const TWO_SESSIONS: &[&str] = &[
    "load_tensors: layer   0 assigned to device CUDA0, is_swa = 0",
    "load_tensors: layer   1 assigned to device CUDA1, is_swa = 0",
    "load_tensors: layer   2 assigned to device CUDA1, is_swa = 0",
    "llama_kv_cache: layer   0: dev = CUDA0",
    "llama_kv_cache: layer   1: dev = CUDA1",
    "llama_kv_cache: layer   0: dev = CUDA0",
    "llama_kv_cache: layer   1: dev = CUDA1",
];

fn judge(records: &[Record]) -> Result<(), String> {
    let devices = [DeviceOrdinal(0), DeviceOrdinal(1)];
    let division = Division::even(2, 2).expect("two layers over two devices");
    let caches = [Caches {
        attention: true,
        recurrent: false,
    }; 2];
    judge_division(records, &devices, &division, &caches).map(|_| ())
}

/// **The held-out arm's cut keeps the load and the first session**, so the
/// SPU's own judgment passes on a capture of several sessions.
#[test]
fn the_first_session_cut_judges_a_capture_of_two_sessions() {
    let records = records(TWO_SESSIONS);
    let first = fixture::first_session(&records);
    assert_eq!(first.len(), 5);
    judge(first).expect("the load and the first session judge clean");
}

/// Perturbation of the above: without the cut, the second session's records
/// are repeats, and the judgment refuses them.
#[test]
fn an_uncut_capture_of_two_sessions_refuses() {
    let faults = judge(&records(TWO_SESSIONS)).expect_err("repeats refuse");
    assert!(faults.contains("recorded 2 times"), "{faults}");
}

/// The cut does not hide a fault inside the first session: a layer 1 record
/// repeated there still refuses.
#[test]
fn a_repeat_inside_the_first_session_still_refuses() {
    let mut lines = TWO_SESSIONS[..5].to_vec();
    lines.push("llama_kv_cache: layer   1: dev = CUDA1");
    lines.extend_from_slice(&TWO_SESSIONS[5..]);
    let records = records(&lines);
    let faults = judge(fixture::first_session(&records)).expect_err("the repeat refuses");
    assert!(faults.contains("layer 1"), "{faults}");
}

/// A capture of one session is judged whole.
#[test]
fn a_capture_of_one_session_is_not_cut() {
    let records = records(&TWO_SESSIONS[..5]);
    assert_eq!(fixture::first_session(&records).len(), 5);
}

/// The cut reads a record lossily: a second session's layer 0 cache record
/// that is not UTF-8 still begins the second session, where a skip would
/// leave the second session's records in the judged slice.
#[test]
fn a_cache_record_that_is_not_utf8_still_opens_a_session() {
    let mut records = records(TWO_SESSIONS);
    records[5].bytes = b"llama_kv_cache: layer   0: dev = CUDA0 \xE2\x82\n".to_vec();
    assert_eq!(fixture::first_session(&records).len(), 5);
}
