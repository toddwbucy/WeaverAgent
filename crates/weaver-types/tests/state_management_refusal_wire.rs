//! TYPES-ENTER's independent four-column matrix and recursive lifecycle carrier.
use serde::Deserialize;
use serde_json::json;
use weaver_types::state_flow::{
    StateManagementCode as Code, StateManagementStage as Stage, decode_strict,
};
use weaver_types::{LifecycleRefusal, OrganEnvelope, Payload, RefusingOrgan};

const STAGES: [(Stage, &str, u8); 4] = [
    (Stage::Partition, "partition", 1),
    (Stage::Template, "template", 2),
    (Stage::Open, "open", 4),
    (Stage::Serving, "serving", 8),
];
const CODES: [(Code, &str, u8); 9] = [
    (Code::UnknownVocabulary, "unknown_vocabulary", 1),
    (Code::InvalidProjection, "invalid_projection", 1),
    (Code::Overlap, "overlap", 1),
    (Code::ForbiddenProjection, "forbidden_projection", 1),
    (Code::TemplateRejected, "template_rejected", 2),
    (Code::TemplateFailed, "template_failed", 2),
    (Code::SchemeMismatch, "scheme_mismatch", 6),
    (Code::OpenFailed, "open_failed", 4),
    (Code::InvalidSpan, "invalid_span", 8),
];
fn assert_decode(wire: &str, expected: Option<&LifecycleRefusal>) {
    let mut decoder = serde_json::Deserializer::from_str(wire);
    for result in [
        serde_json::from_str::<LifecycleRefusal>(wire),
        decode_strict::<LifecycleRefusal>(wire),
        LifecycleRefusal::deserialize(&mut decoder),
    ] {
        match expected {
            Some(expected) => assert_eq!(&result.unwrap(), expected, "{wire}"),
            None => assert!(result.is_err(), "invalid refusal decoded: {wire}"),
        }
    }
}

#[test]
fn state_management_refusal_checks_all_stage_code_pairs_and_nested_carriers() {
    let mut legal = 0;
    for (stage, stage_name, bit) in STAGES {
        for (code, code_name, mask) in CODES {
            let valid = mask & bit != 0;
            legal += usize::from(valid);
            assert_eq!(code.permits_stage(stage), valid);
            let mut typed = LifecycleRefusal::StateManagementInvalid {
                stage,
                code,
                rule_id: None,
                path: None,
                detail: "exact reason".into(),
            };
            let mut value = json!({"kind":"state_management_invalid","stage":stage_name,"code":code_name,"detail":"exact reason"});
            for _ in 0..3 {
                assert_eq!(serde_json::to_value(&typed).unwrap(), value);
                assert_decode(&value.to_string(), valid.then_some(&typed));
                let payload = json!({"kind":"refusal","body":value});
                let decoded = serde_json::from_value::<Payload>(payload.clone());
                if valid {
                    assert_eq!(decoded.unwrap(), Payload::Refusal(typed.clone()));
                } else {
                    assert!(decoded.is_err());
                }
                let envelope = json!({"exchange":{"opener":"admin","ordinal":u64::MAX},"position":"close","payload":payload});
                let decoded = serde_json::from_value::<OrganEnvelope>(envelope);
                if valid {
                    assert_eq!(decoded.unwrap().payload, Payload::Refusal(typed.clone()));
                } else {
                    assert!(decoded.is_err());
                }
                typed = LifecycleRefusal::OrganRefused {
                    organ: RefusingOrgan::Spu,
                    reason: Box::new(typed),
                };
                value = json!({"kind":"organ_refused","organ":"spu","reason":value});
            }
        }
    }
    assert_eq!(legal, 10);
}

#[test]
fn state_management_refusal_preserves_optional_attribution_and_exact_strings() {
    for rule in [None, Some("rule-\"exact\"\n")] {
        for path in [None, Some("/payload/blocks/0/text\\source")] {
            let refusal = LifecycleRefusal::StateManagementInvalid {
                stage: Stage::Partition,
                code: Code::InvalidProjection,
                rule_id: rule.map(str::to_owned),
                path: path.map(str::to_owned),
                detail: "exact\tUnicode \u{e9}\nreason\\end".into(),
            };
            let value = serde_json::to_value(&refusal).unwrap();
            assert_eq!(
                value.as_object().unwrap().len(),
                4 + usize::from(rule.is_some()) + usize::from(path.is_some())
            );
            assert_eq!(value.get("rule_id").and_then(|v| v.as_str()), rule);
            assert_eq!(value.get("path").and_then(|v| v.as_str()), path);
            assert_decode(&value.to_string(), Some(&refusal));
            let mut explicit_null = value;
            if rule.is_none() {
                explicit_null["rule_id"] = serde_json::Value::Null;
            }
            if path.is_none() {
                explicit_null["path"] = serde_json::Value::Null;
            }
            assert_decode(&explicit_null.to_string(), Some(&refusal));
        }
    }
}

#[test]
fn state_management_refusal_rejects_unknown_missing_wrong_shaped_and_duplicate_fields() {
    let canonical = json!({"kind":"state_management_invalid","stage":"template","code":"template_failed","rule_id":"actual-rule","path":"actual-path","detail":"exact checker detail"});
    for field in ["kind", "stage", "code", "detail"] {
        let mut missing = canonical.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_decode(&missing.to_string(), None);
        let mut null = canonical.clone();
        null[field] = serde_json::Value::Null;
        assert_decode(&null.to_string(), None);
    }
    for (field, wrong) in [
        ("kind", json!("StateManagementInvalid")),
        ("stage", json!("boot")),
        ("stage", json!({"template":null})),
        ("code", json!({"template_failed":null})),
        ("code", json!("invalid_bound")),
        ("code", json!("TemplateFailed")),
        ("rule_id", json!(1)),
        ("path", json!(["invented"])),
        ("detail", json!(false)),
        ("unexpected", json!("not allowed")),
    ] {
        let mut bad = canonical.clone();
        bad[field] = wrong;
        let raw = bad.to_string();
        assert_decode(&raw, None);
        assert_decode(
            &format!(r#"{{"kind":"organ_refused","organ":"spu","reason":{raw}}}"#),
            None,
        );
    }
    let raw = canonical.to_string();
    for field in ["kind", "stage", "code", "rule_id", "path", "detail"] {
        let needle = format!("\"{field}\":");
        let value = serde_json::to_string(&canonical[field]).unwrap();
        let duplicate = raw.replace(&needle, &format!("\"{field}\":{value},\"{field}\":"));
        assert_decode(&duplicate, None);
        assert_decode(
            &format!(r#"{{"kind":"organ_refused","organ":"spu","reason":{duplicate}}}"#),
            None,
        );
    }
}
