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
