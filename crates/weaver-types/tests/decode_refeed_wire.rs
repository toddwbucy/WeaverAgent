use serde_json::{Value, json};
use weaver_types::TokenAnswer;

// The Spec fixes this internal interval. Ownership and nonrenewal belong to
// the harness's cancellation owner, not to a second timer in the floor.
const _: () = assert!(weaver_types::DIAGNOSTIC_CANCEL_BOUND_MS == 60_000);

fn interrupted() -> Value {
    json!({"kind":"refeed_interrupted","body":{
        "turn":"original-turn","consumed":0,"resident":17,"capacity":1024
    }})
}

#[test]
fn interrupted_refeed_has_its_own_required_body_in_both_envelope_orders() {
    let original = interrupted();
    let kind_first = format!(
        r#"{{"kind":"refeed_interrupted","body":{}}}"#,
        original["body"]
    );
    let body_first = format!(
        r#"{{"body":{},"kind":"refeed_interrupted"}}"#,
        original["body"]
    );
    assert_ne!(kind_first, body_first);
    for bytes in [kind_first, body_first] {
        let answer: TokenAnswer =
            serde_json::from_str(&bytes).expect("interruption is a decode answer");
        let encoded = serde_json::to_string(&answer).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&encoded).unwrap(), original);
        assert_eq!(
            serde_json::from_reader::<_, TokenAnswer>(bytes.as_bytes()).unwrap(),
            answer
        );
    }
}

#[test]
fn interrupted_refeed_never_defaults_an_absent_or_invalid_fact() {
    for field in ["turn", "consumed", "resident", "capacity"] {
        let mut missing = interrupted();
        missing["body"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_str::<TokenAnswer>(&missing.to_string()).is_err(),
            "missing {field}"
        );
        let mut null = interrupted();
        null["body"][field] = Value::Null;
        assert!(
            serde_json::from_str::<TokenAnswer>(&null.to_string()).is_err(),
            "null {field}"
        );
    }
    for field in ["consumed", "resident", "capacity"] {
        for invalid in [json!(-1), json!(1.5), json!("1"), json!(true)] {
            let mut value = interrupted();
            value["body"][field] = invalid;
            assert!(
                serde_json::from_str::<TokenAnswer>(&value.to_string()).is_err(),
                "invalid {field}"
            );
        }
    }
    for extra in ["emission", "request", "measurement", "unknown"] {
        let mut value = interrupted();
        value["body"][extra] = json!({});
        assert!(
            serde_json::from_str::<TokenAnswer>(&value.to_string()).is_err(),
            "extra {extra}"
        );
    }
    let duplicate = r#"{"kind":"refeed_interrupted","body":{"turn":"t","consumed":0,"consumed":1,"resident":2,"capacity":3}}"#;
    assert!(serde_json::from_str::<TokenAnswer>(duplicate).is_err());
}
