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
use weaver_spu::lora::observation::Record;
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
/// Where the load's records end, and each session's.
const LOAD: usize = 3;
const FIRST: std::ops::Range<usize> = 3..5;
const SECOND: std::ops::Range<usize> = 5..7;

fn session(records: &[Record], range: std::ops::Range<usize>) -> Result<Vec<u32>, String> {
    let devices = [DeviceOrdinal(0), DeviceOrdinal(1)];
    let division = Division::even(2, 2).expect("two layers over two devices");
    let caches = [Caches {
        attention: true,
        recurrent: false,
    }; 2];
    fixture::judge_session(records, LOAD, range, &devices, &division, &caches)
        .map(|observed| observed.slots.iter().map(|d| d.0).collect())
}

/// **Each session is judged with the load's records and its own**, so the
/// SPU's judgment passes on every session of a capture of several.
#[test]
fn each_session_judges_with_the_load_records() {
    let records = records(TWO_SESSIONS);
    assert_eq!(session(&records, FIRST), Ok(vec![0, 1, 1]));
    assert_eq!(session(&records, SECOND), Ok(vec![0, 1, 1]));
}

/// Perturbation of the above: judged whole, the second session's records are
/// repeats, and the judgment refuses them.
#[test]
fn a_capture_judged_whole_refuses_its_repeats() {
    let records = records(TWO_SESSIONS);
    let faults = session(&records, FIRST.start..SECOND.end).expect_err("repeats refuse");
    assert!(faults.contains("recorded 2 times"), "{faults}");
}

/// A later session's cache on the wrong device refuses, which a judgment of
/// the first session alone would not see.
#[test]
fn a_later_session_off_its_layer_refuses() {
    let mut lines = TWO_SESSIONS.to_vec();
    lines[6] = "llama_kv_cache: layer   1: dev = CUDA0";
    let records = records(&lines);
    assert!(session(&records, FIRST).is_ok());
    let faults = session(&records, SECOND).expect_err("the misplaced cache refuses");
    assert!(faults.contains("layer 1"), "{faults}");
}

/// The boundaries are counts, not a reading of the log: a session whose cache
/// also prints a filtered layer 0 line, as a sliding-window model's second
/// cache does, is judged whole.
#[test]
fn a_filtered_cache_line_does_not_move_the_boundaries() {
    let mut lines = TWO_SESSIONS[..5].to_vec();
    lines.insert(4, "llama_kv_cache: layer   0: filtered");
    let records = records(&lines);
    assert_eq!(session(&records, 3..6), Ok(vec![0, 1, 1]));
}
