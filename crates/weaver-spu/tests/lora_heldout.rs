//! **The held-out set through the LORA-7 fixture, base-only and adapted.**
//!
//! `weaver-spu-lora-loading-Spec-amendment` section 7 asks for the Python
//! comparison with matched input tokens and recorded settings, and leaves the
//! accepted drift to the operator (LORA-8). This target produces the SPU side
//! of that comparison: every entry of `WEAVER_LORA_ENTRIES`, decoded greedily
//! from its frozen input ids under the reference's reply budget, once
//! base-only and once adapted, each arm in its own process. Its outputs carry
//! the reference's row shape, so the reference's own checker and a token
//! comparison read them unchanged. It asserts only what LORA-7 fixes: the two
//! arms show the same division, and the adapter changes at least one output.
//! The drift is measured outside it and ruled by the operator.
//!
//! An explicit accelerator instrument, run with `--ignored` under the
//! variables `lora/fixture.rs` reads. Run `lora_proof.rs` first: it is the
//! gate, and this one is the long run.
//!
//! Perturbation: run the adapted arm without its request and the
//! output-change assertion fails.
#![cfg(all(feature = "gguf", feature = "cuda"))]

#[path = "lora/fixture.rs"]
mod fixture;

use fixture::{
    Inputs, assert_adapter_on_its_layers, assert_covers, assert_division_on_binding,
    assert_same_division, first_difference, run_arm,
};

const TEST: &str = "the_held_out_set_decodes_base_only_and_adapted_on_one_division";

#[test]
#[ignore = "needs the base, the adapter, the entries and the binding's devices"]
fn the_held_out_set_decodes_base_only_and_adapted_on_one_division() {
    if fixture::child() {
        return;
    }
    let inputs = Inputs::from_env();
    fixture::fresh_report(&inputs.report);
    let ids: Vec<String> = fixture::entries(&inputs.entries)
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert!(!ids.is_empty(), "WEAVER_LORA_ENTRIES holds no entry");

    let base = run_arm(TEST, &inputs, "base", false, None);
    let adapted = run_arm(TEST, &inputs, "adapted", true, None);

    assert_covers(&base, &ids);
    assert_covers(&adapted, &ids);
    assert_division_on_binding(&base, &inputs.devices);
    assert_division_on_binding(&adapted, &inputs.devices);
    assert_same_division(&base, &adapted);
    assert_adapter_on_its_layers(&adapted);
    let changed = ids
        .iter()
        .filter(|id| first_difference(&base.outputs[*id], &adapted.outputs[*id]).is_some())
        .count();
    let summary = serde_json::json!({
        "entries": ids.len(),
        "changed_by_adapter": changed,
        "same_division": true,
    });
    std::fs::write(
        inputs.report.join("summary.json"),
        serde_json::to_string_pretty(&summary).expect("the summary serializes"),
    )
    .expect("the summary writes");
    assert!(
        changed > 0,
        "the adapter changed no output on the held-out set"
    );
    eprintln!("held-out run: {summary}");
}
