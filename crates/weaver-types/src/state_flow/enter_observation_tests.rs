use super::*;
use serde_json::{Value, json};

fn fixture() -> ObservationEnvelope {
    let enter: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/state-enter.json")).unwrap();
    let p = &enter["payload"]["body"]["payload"];
    let b = &p["binding"];
    ObservationEnvelope::decode(
        &serde_json::to_vec(&json!({
            "exchange":{"opener":"admin","ordinal":71},"position":"close",
            "payload":{"kind":"answer","body":{"kind":"state","state":"idle","load":{
                "session":p["session"],"run":p["run"],"declaration":p["declaration"],
                "artifact":"/synthetic/decoder.gguf","residual_readout":false,"surprisal":false,
                "state":{"serving":{"state_management":b["state_management"],"start":b["start"],
                    "ste100_scheme":b["ste100_scheme"],"ste100_binary_sha256":"e".repeat(64)}},
                "state_store":{"engine":"sqlite"},"state_member":true,"composer":{"binary":"worker"}
            }}}
        }))
        .unwrap(),
    )
    .unwrap()
}
fn load(value: &mut Value) -> &mut Value {
    &mut value["payload"]["body"]["load"]
}

#[test]
fn replacement_observation_preserves_committed_plan_and_rejects_it_without_serving() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["payload"]["body"]["plan"] = json!({
        "plan":{"plan_id":{"seed_id":"seed","local_id":"3"},"revision":"8"},
        "status":"locked","document_sha256":"1".repeat(64),
        "last_source":{"block_id":{"run":"prior","sequence":"7","block":"0"},"turn":null}
    });
    let wire = serde_json::to_vec(&value).unwrap();
    let parsed = ObservationEnvelope::decode(&wire).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&parsed.encode().unwrap()).unwrap(),
        value
    );
    for case in [
        "no-load",
        "unknown",
        "missing-turn",
        "status-object",
        "in-transition",
    ] {
        let mut invalid = value.clone();
        match case {
            "no-load" => {
                invalid["payload"]["body"]
                    .as_object_mut()
                    .unwrap()
                    .remove("load");
                invalid["payload"]["body"]["state"] = json!("unloaded");
            }
            "unknown" => invalid["payload"]["body"]["plan"]["extra"] = json!(0),
            "missing-turn" => {
                invalid["payload"]["body"]["plan"]["last_source"]
                    .as_object_mut()
                    .unwrap()
                    .remove("turn");
            }
            "status-object" => {
                invalid["payload"]["body"]["plan"]["status"] = json!({"locked":null})
            }
            "in-transition" => {
                let body = invalid["payload"]["body"].as_object_mut().unwrap();
                body.remove("load");
                body.remove("state");
                body.insert("kind".into(), json!("in_transition"));
            }
            _ => unreachable!(),
        }
        assert!(
            ObservationEnvelope::decode(&serde_json::to_vec(&invalid).unwrap()).is_err(),
            "{case}"
        );
    }
}
#[test]
fn replacement_observation_preserves_exact_state_and_rejects_legacy_parser() {
    let value = fixture();
    let bytes = value.encode().unwrap();
    assert_eq!(ObservationEnvelope::decode(&bytes).unwrap(), value);
    assert!(serde_json::from_slice::<crate::OrganEnvelope>(&bytes).is_err());
    let mut legacy: Value = serde_json::from_slice(&bytes).unwrap();
    load(&mut legacy).as_object_mut().unwrap().remove("state");
    load(&mut legacy)["state_election"] = json!({"kind":"none"});
    assert!(ObservationEnvelope::decode(&serde_json::to_vec(&legacy).unwrap()).is_err());
}
#[test]
fn replacement_observation_rejects_missing_mixed_or_impossible_facts() {
    for case in [
        "missing",
        "old",
        "extra",
        "no-member",
        "no-store",
        "no-load",
        "unloaded-load",
        "opener",
        "position",
        "unknown-envelope",
    ] {
        let mut v = serde_json::to_value(fixture()).unwrap();
        match case {
            "missing" => {
                load(&mut v).as_object_mut().unwrap().remove("state");
            }
            "old" => load(&mut v)["state_election"] = json!({"kind":"none"}),
            "extra" => load(&mut v)["state"]["serving"]["extra"] = json!(1),
            "no-member" => load(&mut v)["state_member"] = json!(false),
            "no-store" => load(&mut v)["state_store"]["engine"] = json!("none"),
            "no-load" => {
                v["payload"]["body"].as_object_mut().unwrap().remove("load");
            }
            "unloaded-load" => v["payload"]["body"]["state"] = json!("unloaded"),
            "opener" => v["exchange"]["opener"] = json!("harness"),
            "position" => v["position"] = json!("open"),
            "unknown-envelope" => v["extra"] = json!(false),
            _ => unreachable!(),
        }
        assert!(
            ObservationEnvelope::decode(&serde_json::to_vec(&v).unwrap()).is_err(),
            "{case}"
        );
    }
    let bytes = String::from_utf8(fixture().encode().unwrap()).unwrap();
    let duplicate = bytes.replacen(
        "\"state_member\":true",
        "\"state_member\":true,\"state_member\":true",
        1,
    );
    assert_ne!(duplicate, bytes);
    assert!(ObservationEnvelope::decode(duplicate.as_bytes()).is_err());
}
#[test]
fn replacement_observation_bounds_whole_escaped_envelope() {
    let mut v = serde_json::to_value(fixture()).unwrap();
    load(&mut v)["state"]["serving"]["ste100_scheme"] = json!("");
    let overhead = serde_json::to_vec(&v).unwrap().len();
    load(&mut v)["state"]["serving"]["ste100_scheme"] =
        json!("x".repeat(crate::MAX_ENVELOPE_BYTES - overhead));
    let bytes = serde_json::to_vec(&v).unwrap();
    assert_eq!(bytes.len(), crate::MAX_ENVELOPE_BYTES);
    let mut exact = ObservationEnvelope::decode(&bytes).unwrap();
    assert_eq!(exact.encode().unwrap().len(), crate::MAX_ENVELOPE_BYTES);
    let ObservationContent::Answer(ObservationAnswer::State {
        load: Some(facts), ..
    }) = &mut exact.payload
    else {
        panic!()
    };
    let LoadedStateFacts::Serving { ste100_scheme, .. } = &mut facts.state else {
        panic!()
    };
    ste100_scheme.push('x');
    assert!(matches!(exact.encode(), Err(CodecError::TooLarge)));
    assert!(matches!(
        ObservationEnvelope::decode(&serde_json::to_vec(&exact).unwrap()),
        Err(CodecError::TooLarge)
    ));
    load(&mut v)["state"]["serving"]["ste100_scheme"] =
        json!("\n".repeat(crate::MAX_ENVELOPE_BYTES / 2));
    assert!(matches!(
        ObservationEnvelope::decode(&serde_json::to_vec(&v).unwrap()),
        Err(CodecError::TooLarge)
    ));
}
