//! TYPES-PERSIST's lifecycle stage/case matrix and exact nested wire carrier.

use serde::Deserialize;
use serde_json::json;
use weaver_types::state_flow::{PersistenceCase, PersistenceStage, decode_strict};
use weaver_types::{
    ExchangeId, LifecycleRefusal, Opener, OrganEnvelope, Payload, Position, RefusingOrgan,
};

// These masks are the Spec's independent five-column table, not permits_stage.
const STAGES: [(PersistenceStage, &str, u8); 5] = [
    (PersistenceStage::Seed, "seed", 1),
    (PersistenceStage::Bind, "bind", 2),
    (PersistenceStage::Load, "load", 4),
    (PersistenceStage::Publish, "publish", 8),
    (PersistenceStage::Boot, "boot", 16),
];
const CASES: [(PersistenceCase, &str, u8); 11] = [
    (PersistenceCase::UnsupportedLayout, "unsupported_layout", 6),
    (PersistenceCase::SeedExists, "seed_exists", 1),
    (
        PersistenceCase::SeedRequiresUnloaded,
        "seed_requires_unloaded",
        1,
    ),
    (
        PersistenceCase::SeedCertificateInvalid,
        "seed_certificate_invalid",
        1,
    ),
    (PersistenceCase::SeedFailed, "seed_failed", 1),
    (PersistenceCase::BindingMismatch, "binding_mismatch", 6),
    (
        PersistenceCase::ConfigurationMismatch,
        "configuration_mismatch",
        6,
    ),
    (PersistenceCase::SchemeMismatch, "scheme_mismatch", 2),
    (PersistenceCase::BrokenSavePoint, "broken_save_point", 6),
    (PersistenceCase::PublicationFailed, "publication_failed", 8),
    (PersistenceCase::BootFailed, "boot_failed", 16),
];

fn assert_decode(wire: &str, expected: Option<&LifecycleRefusal>) {
    let mut deserializer = serde_json::Deserializer::from_str(wire);
    for result in [
        serde_json::from_str::<LifecycleRefusal>(wire),
        decode_strict::<LifecycleRefusal>(wire),
        LifecycleRefusal::deserialize(&mut deserializer),
    ] {
        match expected {
            Some(expected) => assert_eq!(result.unwrap(), *expected, "{wire}"),
            None => assert!(result.is_err(), "invalid refusal decoded: {wire}"),
        }
    }
}

#[test]
fn persistence_refusal_checks_the_complete_stage_matrix_even_when_nested() {
    let mut admitted = 0;
    for (stage, stage_name, stage_bit) in STAGES {
        for (case, case_name, allowed) in CASES {
            let valid = allowed & stage_bit != 0;
            admitted += usize::from(valid);
            for artifact_id in [None, Some("exact-artifact".to_owned())] {
                let mut refusal = LifecycleRefusal::StatePersistenceInvalid {
                    stage,
                    case,
                    artifact_id: artifact_id.clone(),
                    detail: "exact reason".into(),
                };
                let mut value = json!({
                    "kind":"state_persistence_invalid", "stage":stage_name,
                    "case":case_name, "detail":"exact reason",
                });
                if let Some(artifact_id) = artifact_id {
                    value["artifact_id"] = json!(artifact_id);
                }
                for _ in 0..3 {
                    assert_eq!(serde_json::to_value(&refusal).unwrap(), value);
                    assert_decode(&value.to_string(), valid.then_some(&refusal));
                    let payload = json!({"kind":"refusal", "body":value});
                    let decoded = serde_json::from_value::<Payload>(payload.clone());
                    if valid {
                        assert_eq!(decoded.unwrap(), Payload::Refusal(refusal.clone()));
                    } else {
                        assert!(decoded.is_err(), "{payload}");
                    }
                    let envelope = json!({
                        "exchange":{"opener":"harness","ordinal":7},
                        "position":"close", "payload":payload,
                    });
                    let decoded = serde_json::from_value::<OrganEnvelope>(envelope.clone());
                    if valid {
                        assert_eq!(
                            decoded.unwrap(),
                            OrganEnvelope {
                                exchange: ExchangeId {
                                    opener: Opener::Harness,
                                    ordinal: 7
                                },
                                position: Position::Close,
                                payload: Payload::Refusal(refusal.clone()),
                            }
                        );
                    } else {
                        assert!(decoded.is_err(), "{envelope}");
                    }
                    refusal = LifecycleRefusal::OrganRefused {
                        organ: RefusingOrgan::Spu,
                        reason: Box::new(refusal),
                    };
                    value = json!({"kind":"organ_refused", "organ":"spu", "reason":value});
                }
            }
        }
    }
    assert_eq!(admitted, 15);
}

#[test]
fn persistence_refusal_serializes_exact_fields_and_omits_absent_artifact() {
    let mut refusal = LifecycleRefusal::StatePersistenceInvalid {
        stage: PersistenceStage::Boot,
        case: PersistenceCase::BootFailed,
        artifact_id: None,
        detail: "head cannot fit".into(),
    };
    let absent = r#"{"kind":"state_persistence_invalid","stage":"boot","case":"boot_failed","detail":"head cannot fit"}"#;
    assert_eq!(serde_json::to_string(&refusal).unwrap(), absent);
    assert_decode(absent, Some(&refusal));
    // Option<String> accepts explicit null but serializes absence by omission.
    assert_decode(
        &absent.replace("\"detail\":", "\"artifact_id\":null,\"detail\":"),
        Some(&refusal),
    );
    let LifecycleRefusal::StatePersistenceInvalid {
        artifact_id,
        detail,
        ..
    } = &mut refusal
    else {
        unreachable!();
    };
    *artifact_id = Some("artifact-\"escaped\"".into());
    *detail = "exact\nreason".into();
    let present = r#"{"kind":"state_persistence_invalid","stage":"boot","case":"boot_failed","artifact_id":"artifact-\"escaped\"","detail":"exact\nreason"}"#;
    assert_eq!(serde_json::to_string(&refusal).unwrap(), present);
    assert_decode(present, Some(&refusal));
}

#[test]
fn persistence_refusal_requires_its_closed_named_fields_and_vocabulary() {
    let canonical = json!({
        "kind":"state_persistence_invalid", "stage":"boot", "case":"boot_failed",
        "artifact_id":"kept-artifact", "detail":"exact reason",
    });
    for field in ["kind", "stage", "case", "detail"] {
        let mut missing = canonical.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_decode(&missing.to_string(), None);
        let mut null = canonical.clone();
        null[field] = serde_json::Value::Null;
        assert_decode(&null.to_string(), None);
    }
    for (field, wrong) in [
        ("kind", json!("StatePersistenceInvalid")),
        ("stage", json!("startup")),
        ("stage", json!(0)),
        ("stage", json!(["boot"])),
        ("case", json!("component_failure")),
        ("case", json!({"boot_failed":{}})),
        ("artifact_id", json!(17)),
        ("detail", json!(false)),
        ("capture_id", json!("1")),
    ] {
        let mut wrong_value = canonical.clone();
        wrong_value[field] = wrong;
        let raw = wrong_value.to_string();
        assert_decode(&raw, None);
        assert_decode(
            &format!(r#"{{"kind":"organ_refused","organ":"spu","reason":{raw}}}"#),
            None,
        );
    }
    let wire = canonical.to_string();
    for field in ["kind", "stage", "case", "artifact_id", "detail"] {
        let key = format!("\"{field}\":");
        let duplicate = wire.replace(
            &key,
            &format!("\"{field}\":{},\"{field}\":", canonical[field]),
        );
        assert_decode(&duplicate, None);
    }
    for positional in [
        json!([
            "state_persistence_invalid",
            "boot",
            "boot_failed",
            "kept-artifact",
            "exact reason"
        ]),
        json!(["state_persistence_invalid", {"stage":"boot","case":"boot_failed","detail":"exact reason"}]),
        json!({"state_persistence_invalid":{"stage":"boot","case":"boot_failed","detail":"exact reason"}}),
    ] {
        assert_decode(&positional.to_string(), None);
    }
}

#[test]
fn persistence_refusal_codes_are_strings_even_inside_carriers() {
    for (_, stage_name, stage_bit) in STAGES {
        for (_, case_name, allowed) in CASES {
            if allowed & stage_bit == 0 {
                continue;
            }
            for (field, code) in [("stage", stage_name), ("case", case_name)] {
                for alias in [
                    json!([code]),
                    json!({(code):null}),
                    json!({(code):{}}),
                    json!({(code):[]}),
                ] {
                    let mut value = json!({
                        "kind":"state_persistence_invalid", "stage":stage_name,
                        "case":case_name, "detail":"exact reason",
                    });
                    value[field] = alias;
                    for _ in 0..3 {
                        assert_decode(&value.to_string(), None);
                        let payload = json!({"kind":"refusal", "body":value});
                        assert!(serde_json::from_value::<Payload>(payload.clone()).is_err());
                        let envelope = json!({
                            "exchange":{"opener":"harness","ordinal":7},
                            "position":"close", "payload":payload,
                        });
                        assert!(serde_json::from_value::<OrganEnvelope>(envelope).is_err());
                        value = json!({"kind":"organ_refused", "organ":"spu", "reason":value});
                    }
                }
            }
        }
    }
}

#[test]
fn persistence_variant_validation_keeps_legacy_refusal_wire_unchanged() {
    for (refusal, wire) in [
        (LifecycleRefusal::Unauthorized, r#"{"kind":"unauthorized"}"#),
        (
            LifecycleRefusal::ConfigInvalid { field: None },
            r#"{"kind":"config_invalid","field":null}"#,
        ),
        (
            LifecycleRefusal::OrganRefused {
                organ: RefusingOrgan::Spu,
                reason: Box::new(LifecycleRefusal::DeviceCannotAdmit),
            },
            r#"{"kind":"organ_refused","organ":"spu","reason":{"kind":"device_cannot_admit"}}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&refusal).unwrap(), wire);
        assert_decode(wire, Some(&refusal));
    }
}
