use super::*;
use serde_json::{Value, json};
fn fixture() -> EnterEnvelope {
    EnterEnvelope::decode(include_bytes!("../../tests/fixtures/state-enter.json")).unwrap()
}
fn payload(value: &mut Value) -> &mut Value {
    &mut value["payload"]["body"]["payload"]
}
#[test]
fn replacement_enter_round_trip_preserves_exact_scheme_and_bound_settings() {
    let envelope = fixture();
    let bytes = envelope.encode().unwrap();
    assert_eq!(EnterEnvelope::decode(&bytes).unwrap(), envelope);
    let EnterBinding::Serving {
        state_management,
        start,
        ste100_scheme,
        ..
    } = &envelope.enter().binding
    else {
        panic!()
    };
    state_management.validate(&[]).unwrap();
    assert_eq!(start.config_sha256.as_str(), envelope.enter().declaration);
    assert!(ste100_scheme.contains("synthetic-process-test"));
    assert!(
        serde_json::from_slice::<crate::OrganEnvelope>(&bytes).is_err(),
        "legacy parser must not accept replacement Serving"
    );
}
#[test]
fn replacement_enter_closes_retired_mixed_unknown_and_incomplete_shapes() {
    for case in [
        "top-election",
        "old-serving",
        "extra-envelope",
        "extra-payload",
        "missing-state",
        "missing-start",
        "missing-scheme",
        "missing-null",
        "array",
        "wrong-opener",
        "wrong-position",
    ] {
        let mut value = serde_json::to_value(fixture()).unwrap();
        match case {
            "top-election" => payload(&mut value)["state_election"] = json!({"kind":"none"}),
            "old-serving" => {
                for k in ["state_management", "start", "ste100_scheme"] {
                    payload(&mut value)["binding"]
                        .as_object_mut()
                        .unwrap()
                        .remove(k);
                }
            }
            "extra-envelope" => value["unknown"] = json!(1),
            "extra-payload" => payload(&mut value)["unknown"] = json!(1),
            "missing-state" => {
                payload(&mut value)["binding"]
                    .as_object_mut()
                    .unwrap()
                    .remove("state_management");
            }
            "missing-start" => {
                payload(&mut value)["binding"]
                    .as_object_mut()
                    .unwrap()
                    .remove("start");
            }
            "missing-scheme" => {
                payload(&mut value)["binding"]
                    .as_object_mut()
                    .unwrap()
                    .remove("ste100_scheme");
            }
            "missing-null" => {
                payload(&mut value)["binding"]["start"]
                    .as_object_mut()
                    .unwrap()
                    .remove("position");
            }
            "array" => value = json!([]),
            "wrong-opener" => value["exchange"]["opener"] = json!("harness"),
            "wrong-position" => value["position"] = json!("close"),
            _ => unreachable!(),
        }
        assert!(
            EnterEnvelope::decode(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{case}"
        );
    }
    let bytes = String::from_utf8(fixture().encode().unwrap()).unwrap();
    let duplicate = bytes.replacen("\"ordinal\":7", "\"ordinal\":7,\"ordinal\":7", 1);
    assert_ne!(bytes, duplicate);
    assert!(EnterEnvelope::decode(duplicate.as_bytes()).is_err());
}
#[test]
fn diagnostic_binding_owns_its_election_and_cannot_mix_serving_fields() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    payload(&mut value)["binding"] =
        json!({"kind":"diagnostic", "state_election":crate::StateElection::default()});
    let diagnostic = EnterEnvelope::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        diagnostic.enter().binding,
        EnterBinding::Diagnostic { .. }
    ));
    payload(&mut value)["binding"]["ste100_scheme"] = json!("forbidden");
    assert!(EnterEnvelope::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn entire_encoded_envelope_is_bounded_including_escaping_and_siblings() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    payload(&mut value)["binding"]["ste100_scheme"] = json!("");
    let base = serde_json::to_vec(&value).unwrap().len();
    payload(&mut value)["binding"]["ste100_scheme"] =
        json!("x".repeat(crate::MAX_ENVELOPE_BYTES - base));
    let exact: EnterEnvelope = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(exact.encode().unwrap().len(), crate::MAX_ENVELOPE_BYTES);
    assert!(EnterEnvelope::decode(&exact.encode().unwrap()).is_ok());
    payload(&mut value)["binding"]["ste100_scheme"] =
        json!("x".repeat(crate::MAX_ENVELOPE_BYTES - base + 1));
    let over: EnterEnvelope = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(over.encode(), Err(CodecError::TooLarge)));
    assert!(matches!(
        EnterEnvelope::decode(&serde_json::to_vec(&over).unwrap()),
        Err(CodecError::TooLarge)
    ));
    payload(&mut value)["binding"]["ste100_scheme"] =
        json!("\n".repeat(crate::MAX_ENVELOPE_BYTES / 2));
    let escaped: EnterEnvelope = serde_json::from_value(value).unwrap();
    assert!(matches!(escaped.encode(), Err(CodecError::TooLarge)));
}
