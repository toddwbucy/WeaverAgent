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
/// binding amendment ADP-3: an Open measurement without the member refuses,
/// as does an adapter member that is not `null` or the closed `artifact`,
/// `sha256`, `scale` object, while `null` and the object pass.
///
/// Perturbation: accept a measurement without the member, or accept any
/// value as the member, and this fails.
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
        "adapter_absent",
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

/// **The theta blocks are stated either way and carried opaque**, per
/// SPU-OPEN and HARNESS-OPEN: a measurement in the five-member shape that
/// predates them refuses, as does one with either block absent, duplicated,
/// or neither an object nor `null`. `null` passes, for a backend that
/// renders none yet (`weaver-spu-Spec` section 3.1.5), and any object
/// passes, its members unread.
///
/// Perturbation: accept the five-member shape, accept any value as a block,
/// or close the blocks' members here, and this fails.
#[test]
fn the_open_measurement_states_both_theta_blocks_and_carries_them_opaque() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../weaver-types/tests/fixtures/decode-open-evidence.json"
    ))
    .unwrap();
    let judged = |name: &str| {
        let case = cases
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("the corpus holds {name}"));
        match serde_json::from_str::<weaver_types::TokenAnswer>(case["wire"].as_str().unwrap()) {
            Ok(weaver_types::TokenAnswer::Opened(opened)) => {
                validate(&opened, &SessionId("s".into()))
            }
            _ => panic!("{name} is an Opened answer"),
        }
    };
    for passed in [
        "escaped_exact_boxes",
        "theta_objects_carried_opaque",
        "theta_empty_objects_carried_opaque",
    ] {
        assert_eq!(judged(passed), Ok(()), "{passed}");
    }
    for refused in [
        "theta_absent_five_member_shape",
        "engine_absent",
        "device_absent",
        "engine_string",
        "device_array",
        "engine_number",
        "duplicate_engine",
    ] {
        assert_eq!(
            judged(refused),
            Err(OpenEvidenceError::Measurement),
            "{refused}"
        );
    }
}
