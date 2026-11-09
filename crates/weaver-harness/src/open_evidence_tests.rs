use super::*;

#[test]
fn shared_open_corpus_checks_closed_evidence_and_exact_counts_without_rewriting_boxes() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../weaver-types/tests/fixtures/decode-open-evidence.json"
    ))
    .unwrap();
    for case in cases {
        let answer =
            serde_json::from_str::<weaver_types::TokenAnswer>(case["wire"].as_str().unwrap());
        assert_eq!(
            answer.is_ok(),
            case["outer_valid"].as_bool().unwrap(),
            "outer: {}",
            case["name"]
        );
        let Ok(weaver_types::TokenAnswer::Opened(opened)) = answer else {
            continue;
        };
        let request = opened.request.get().to_owned();
        let measurement = opened.measurement.get().to_owned();
        assert_eq!(
            validate(&opened, &SessionId("s".into())).is_ok(),
            case["evidence_valid"].as_bool().unwrap(),
            "evidence: {}",
            case["name"]
        );
        assert_eq!(opened.request.get(), request);
        assert_eq!(opened.measurement.get(), measurement);
    }
}

/// **The shape stays closed with the adapter in it**, per the adapter
/// binding amendment ADP-3: an Open measurement in the four-member shape
/// that predates the member refuses, as does an adapter member that is not
/// `null` or the closed `artifact`, `sha256`, `scale` object, while `null`
/// and the object pass.
///
/// Perturbation: accept the four-member shape, or accept any value as the
/// member, and this fails.
#[test]
fn an_open_measurement_without_the_adapter_member_refuses() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../weaver-types/tests/fixtures/decode-open-evidence.json"
    ))
    .unwrap();
    let judged = |name: &str| {
        let case = cases
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("the corpus holds {name}"));
        let Ok(weaver_types::TokenAnswer::Opened(opened)) =
            serde_json::from_str::<weaver_types::TokenAnswer>(case["wire"].as_str().unwrap())
        else {
            panic!("{name} is an Opened answer");
        };
        validate(&opened, &SessionId("s".into()))
    };
    assert_eq!(judged("escaped_exact_boxes"), Ok(()));
    assert_eq!(judged("adapted_object"), Ok(()));
    for refused in [
        "adapter_absent_four_member_shape",
        "adapter_string",
        "adapter_unknown_member",
        "adapter_missing_scale",
        "adapter_string_scale",
        "adapter_scale_beyond_f32",
        "adapter_numeric_sha256",
    ] {
        assert_eq!(
            judged(refused),
            Err(OpenEvidenceError::Measurement),
            "{refused}"
        );
    }
}
