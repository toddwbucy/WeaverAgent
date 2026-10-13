//! Test-only: the `undecodable_tests` suite of `channel.rs`,
//! moved out of the production file by #118.

use super::undecodable_detail;

/// **The operator line names the member the builds disagree about**, per
/// `weaver-harness-Spec` section 7 and issue #404: an hour of diagnosis
/// against `no_residency` and a bare `Undecodable`, where one line naming
/// `unknown field: column-permission` costs a second.
///
/// Perturbation: return a constant from `undecodable_detail` and this
/// fails on the member name. Watched under exactly that removal,
/// 2026-09-02.
///
/// conforms: harness-undecodable-names-its-detail
#[test]
fn the_undecodable_line_names_the_field() {
    // The mixed-version shape itself: an instruction carrying a member
    // this build never registered, refused by deny_unknown_fields.
    let alien = r#"{"model-binding":{"artifact":"a","devices":[0]},
            "residual-readout-election":false,"surprisal-election":false,
            "tunable-values":{},
            "a-member-from-a-newer-floor":{"x":1}}"#;
    let error = serde_json::from_str::<weaver_types::DecoderInstruction>(alien)
        .expect_err("the alien member refuses");
    let line = undecodable_detail("organ", &error);
    assert!(
        line.contains("a-member-from-a-newer-floor"),
        "the line names the member: {line}"
    );
    assert!(
        line.contains("unknown field"),
        "and the refusal's kind: {line}"
    );
    assert!(
        line.contains("\"channel\":\"organ\""),
        "and which channel refused: {line}"
    );
    // The line itself is typed, so the journal grep that finds it can
    // also parse it.
    serde_json::from_str::<serde_json::Value>(&line).expect("a typed line");
}
