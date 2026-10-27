use super::*;
use serde_json::{Value, json};

fn envelope(kind: &str, body: Value) -> Value {
    json!({
        "exchange":{"opener":"admin","ordinal":19},
        "position":"continue",
        "payload":{"kind":kind,"body":body}
    })
}

fn continuations() -> Vec<(CaptureSender, Value)> {
    vec![
        (
            CaptureSender::Harness,
            envelope(
                "save_point_beginning",
                json!({
                    "capture_id":"7", "window":{"began_ns":"100","deadline_ns":"200"},
                    "tail_window":null
                }),
            ),
        ),
        (
            CaptureSender::Harness,
            envelope(
                "save_point_image",
                json!({
                    "capture_id":"7", "room_name":"capture-7.sqlite.part", "size_bytes":"9007199254740993",
                    "position":null
                }),
            ),
        ),
        (
            CaptureSender::Admin,
            envelope(
                "save_point_image_checked",
                json!({
                    "capture_id":"7", "dump_sha256":"a".repeat(64)
                }),
            ),
        ),
        (
            CaptureSender::Harness,
            envelope(
                "save_point_image_finished",
                json!({
                    "capture_id":"7", "room_name":"capture-7.sqlite"
                }),
            ),
        ),
        (
            CaptureSender::Admin,
            envelope(
                "save_point_prepared",
                json!({
                    "capture_id":"7", "artifact_id":"capture-fixture", "dump_sha256":"a".repeat(64),
                    "config_sha256":"b".repeat(64)
                }),
            ),
        ),
        (
            CaptureSender::Harness,
            envelope("save_point_commit", json!({"capture_id":"7"})),
        ),
        (
            CaptureSender::Admin,
            envelope(
                "save_point_published",
                json!({
                    "capture_id":"7", "artifact_id":"capture-fixture", "dump_sha256":"a".repeat(64),
                    "config_sha256":"b".repeat(64)
                }),
            ),
        ),
        (
            CaptureSender::Harness,
            envelope("save_point_capture_complete", json!({"capture_id":"7"})),
        ),
        (
            CaptureSender::Harness,
            envelope(
                "save_point_aborted",
                json!({
                    "capture_id":"7", "code":"snapshot_failed", "detail":"exact refusal\nwith detail"
                }),
            ),
        ),
    ]
}

fn decode(value: &Value, sender: CaptureSender) -> Result<CaptureEnvelope, CaptureCodecError> {
    CaptureEnvelope::decode(&serde_json::to_vec(value).unwrap(), sender)
}

#[test]
fn capture_codec_uses_the_original_exchange_and_exact_paired_continuation_shapes() {
    for (sender, value) in continuations() {
        let decoded = decode(&value, sender).unwrap();
        assert_eq!(decoded.capture_id(), Some(7.into()));
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        assert_eq!(
            CaptureEnvelope::decode(&decoded.encode(sender).unwrap(), sender).unwrap(),
            decoded
        );
        assert!(serde_json::from_value::<crate::OrganEnvelope>(value.clone()).is_err());
        let other = match sender {
            CaptureSender::Harness => CaptureSender::Admin,
            CaptureSender::Admin => CaptureSender::Harness,
        };
        if value["payload"]["kind"] == "save_point_aborted" {
            assert!(decode(&value, other).is_ok());
        } else {
            assert!(decode(&value, other).is_err());
            assert!(decoded.encode(other).is_err());
        }
        for position in ["open", "close"] {
            let mut wrong = value.clone();
            wrong["position"] = json!(position);
            assert!(decode(&wrong, sender).is_err());
        }
        for opener in ["harness", "spu", "gate"] {
            let mut wrong = value.clone();
            wrong["exchange"]["opener"] = json!(opener);
            assert!(decode(&wrong, sender).is_err());
        }
    }
}

#[test]
fn capture_codec_refuses_missing_extra_duplicate_array_and_noncanonical_fields() {
    for (sender, value) in continuations() {
        // This is serde's supported array alias in declaration order, not just
        // an arbitrary permutation that an open decoder might also reject.
        let mut adjacent_array = value.clone();
        adjacent_array["payload"] = json!([value["payload"]["kind"], value["payload"]["body"]]);
        assert!(decode(&adjacent_array, sender).is_err());
        let mut exchange_array = value.clone();
        exchange_array["exchange"] = json!(["admin", 19]);
        assert!(decode(&exchange_array, sender).is_err());
        for pointer in ["", "/exchange", "/payload", "/payload/body"] {
            let mut extra = value.clone();
            extra
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("extra".into(), json!(true));
            assert!(
                decode(&extra, sender).is_err(),
                "extra at {pointer}: {extra}"
            );
            let mut array = value.clone();
            let slot = array.pointer_mut(pointer).unwrap();
            *slot = Value::Array(slot.as_object().unwrap().values().cloned().collect());
            assert!(
                decode(&array, sender).is_err(),
                "array at {pointer}: {array}"
            );
        }
        for field in value["payload"]["body"].as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing["payload"]["body"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                decode(&missing, sender).is_err(),
                "missing {field}: {missing}"
            );
        }
        for bad in [
            json!(0),
            json!(7),
            json!("0"),
            json!("07"),
            json!("-1"),
            json!("18446744073709551616"),
        ] {
            let mut wrong = value.clone();
            wrong["payload"]["body"]["capture_id"] = bad;
            assert!(decode(&wrong, sender).is_err(), "{wrong}");
        }
        let bytes = value.to_string();
        let duplicate = bytes.replacen(
            "\"capture_id\":\"7\"",
            "\"capture_id\":\"7\",\"capture_id\":\"7\"",
            1,
        );
        assert_ne!(bytes, duplicate);
        assert!(CaptureEnvelope::decode(duplicate.as_bytes(), sender).is_err());
    }
}

#[test]
fn capture_codec_keeps_nullable_tail_and_absolute_windows_without_renewing_expiry() {
    let (sender, mut value) = continuations().remove(0);
    value["payload"]["body"]["tail_window"] = json!({"began_ns":"90","deadline_ns":"210"});
    let decoded = decode(&value, sender).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    for field in ["window", "tail_window"] {
        for bad in [
            json!({"began_ns":"100","deadline_ns":"100"}),
            json!({"began_ns":"101","deadline_ns":"100"}),
            json!({"began_ns":100,"deadline_ns":"200"}),
            json!({"began_ns":"100","deadline_ns":"200","duration":"100"}),
            json!(["100", "200"]),
        ] {
            let mut wrong = value.clone();
            wrong["payload"]["body"][field] = bad;
            assert!(decode(&wrong, sender).is_err(), "{wrong}");
        }
    }
    let mut invalid = decode(&value, sender).unwrap();
    let CapturePayload::SavePointBeginning(beginning) = &mut invalid.payload else {
        unreachable!()
    };
    beginning.window.deadline_ns = beginning.window.began_ns;
    assert!(invalid.encode(sender).is_err());
}

fn report() -> Value {
    json!({"start":{
        "seed_id":"a".repeat(32), "artifact_id":"capture-fixture", "dump_sha256":"a".repeat(64),
        "config_sha256":"b".repeat(64), "parent_artifact_id":"seed-fixture", "position":null
    }, "capture_id":"7", "event_run":"run-fixture", "event_sequence":"9007199254740993"})
}

fn final_answer(kind: &str, body: Value) -> Value {
    let mut value = envelope(kind, body);
    value["position"] = json!("close");
    value
}

#[test]
fn capture_codec_final_left_carries_only_the_replacement_success_report() {
    for answer in [
        json!({"kind":"left","save_point":report(),"forced":true,"no_state":false}),
        json!({"kind":"save_point_taken","report":report()}),
    ] {
        let value = final_answer("answer", answer);
        let decoded = decode(&value, CaptureSender::Harness).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        assert!(decode(&value, CaptureSender::Admin).is_err());
        assert!(serde_json::from_value::<crate::OrganEnvelope>(value.clone()).is_err());
        let mut continued = value.clone();
        continued["position"] = json!("continue");
        assert!(decode(&continued, CaptureSender::Harness).is_err());
        let key = if value["payload"]["body"]["kind"] == "left" {
            "save_point"
        } else {
            "report"
        };
        for path in [
            format!("/payload/body/{key}"),
            format!("/payload/body/{key}/start"),
        ] {
            let mut wrong = value.clone();
            let slot = wrong.pointer_mut(&path).unwrap();
            *slot = Value::Array(slot.as_object().unwrap().values().cloned().collect());
            assert!(decode(&wrong, CaptureSender::Harness).is_err());
        }
        let mut legacy = value;
        legacy["payload"]["body"][key] = json!({
            "save_point":"a".repeat(64),"name":"old","run":"run-fixture", "sequence":1,
            "turn":1,"event_run":"run-fixture","position":2
        });
        assert!(decode(&legacy, CaptureSender::Harness).is_err());
    }
    let none = final_answer(
        "answer",
        json!({"kind":"left","forced":true,"no_state":false}),
    );
    assert!(decode(&none, CaptureSender::Harness).is_ok());
    let mut impossible = none;
    impossible["payload"]["body"]["no_state"] = json!(true);
    impossible["payload"]["body"]["save_point"] = report();
    assert!(decode(&impossible, CaptureSender::Harness).is_err());
}

#[test]
fn capture_codec_keeps_failed_run_evidence_separate_from_success_and_retired_member_death() {
    for missed in ["answer", "finished", "published", "capture_not_ready"] {
        let value = final_answer(
            "refusal",
            json!({"kind":"save_point_not_taken","missed":missed}),
        );
        let decoded = decode(&value, CaptureSender::Harness).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        assert_eq!(
            CaptureEnvelope::decode(
                &decoded.encode(CaptureSender::Harness).unwrap(),
                CaptureSender::Harness
            )
            .unwrap(),
            decoded
        );
    }
    let retired = final_answer(
        "refusal",
        json!({"kind":"save_point_not_taken","missed":"member_dead"}),
    );
    assert!(decode(&retired, CaptureSender::Harness).is_err());
    let nested = final_answer(
        "refusal",
        json!({"kind":"organ_refused","organ":"state", "reason":{"kind":"save_point_not_taken","missed":"member_dead"}}),
    );
    assert!(decode(&nested, CaptureSender::Harness).is_err());
    let value = final_answer(
        "refusal",
        json!({
            "kind":"failed_run", "run":"run-fixture", "capture_id":"7", "phase":"publishing",
            "published":report()["start"], "artifacts":[{"location":"save-points/.capture-fixture.pending","kind":"guard","sha256":null}],
            "detail":"worker exited after actual Commit"
        }),
    );
    let decoded = decode(&value, CaptureSender::Harness).unwrap();
    assert_eq!(decoded.capture_id(), Some(7.into()));
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
    for pointer in ["/payload/body/published", "/payload/body/artifacts/0"] {
        let mut wrong = value.clone();
        let slot = wrong.pointer_mut(pointer).unwrap();
        *slot = Value::Array(slot.as_object().unwrap().values().cloned().collect());
        assert!(decode(&wrong, CaptureSender::Harness).is_err());
    }
    let common = final_answer("refusal", json!({"kind":"out_of_order"}));
    assert!(decode(&common, CaptureSender::Harness).is_ok());
}

#[test]
fn capture_codec_bounds_the_whole_escaped_packet_and_retains_unicode_and_newlines() {
    let (_, mut value) = continuations().pop().unwrap();
    value["payload"]["body"]["detail"] = json!("");
    let overhead = value.to_string().len();
    value["payload"]["body"]["detail"] = json!("x".repeat(crate::MAX_ENVELOPE_BYTES - overhead));
    let bytes = serde_json::to_vec(&value).unwrap();
    assert_eq!(bytes.len(), crate::MAX_ENVELOPE_BYTES);
    let decoded = CaptureEnvelope::decode(&bytes, CaptureSender::Admin).unwrap();
    assert_eq!(
        decoded.encode(CaptureSender::Admin).unwrap().len(),
        crate::MAX_ENVELOPE_BYTES
    );
    value["payload"]["body"]["detail"] = json!("\n".repeat(crate::MAX_ENVELOPE_BYTES / 2));
    assert!(matches!(
        decode(&value, CaptureSender::Admin),
        Err(CaptureCodecError::TooLarge)
    ));
    let oversized: CaptureEnvelope = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        oversized.encode(CaptureSender::Admin),
        Err(CaptureCodecError::TooLarge)
    ));
    value["payload"]["body"]["detail"] = json!("exact \"reply\"\n\u{00e9}");
    assert_eq!(
        serde_json::to_value(decode(&value, CaptureSender::Admin).unwrap()).unwrap(),
        value
    );
}

fn demand_request() -> Value {
    json!({
        "exchange":{"opener":"admin","ordinal":31},
        "position":"open",
        "payload":{"kind":"directive","body":{
            "kind":"save_point","cause":{"uid":74},
            "window":{"began_ns":"100","deadline_ns":"200"}
        }}
    })
}

#[test]
fn capture_save_point_request_retains_the_required_original_window() {
    let value = demand_request();
    let raw = serde_json::to_vec(&value).unwrap();
    let request = CaptureEnvelope::decode(&raw, CaptureSender::Admin).unwrap();
    assert_eq!(
        request.capture_id(),
        None,
        "capture identity begins at the boundary"
    );
    assert_eq!(serde_json::to_value(&request).unwrap(), value);
    let carried = request.encode(CaptureSender::Admin).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&carried).unwrap(), value);
    assert!(request.encode(CaptureSender::Harness).is_err());
    assert!(CaptureEnvelope::decode(&raw, CaptureSender::Harness).is_err());
    let mut malformed = request.clone();
    let CapturePayload::Directive(CaptureDirective::SavePoint { window, .. }) =
        &mut malformed.payload
    else {
        panic!("the accepted opener must retain its typed window")
    };
    window.deadline_ns = window.began_ns;
    assert!(malformed.encode(CaptureSender::Admin).is_err());
    for position in ["continue", "close"] {
        let mut wrong = value.clone();
        wrong["position"] = json!(position);
        assert!(
            CaptureEnvelope::decode(&serde_json::to_vec(&wrong).unwrap(), CaptureSender::Admin)
                .is_err()
        );
    }
}

#[test]
fn capture_save_point_request_keeps_expired_values_distinct_from_malformed_windows() {
    let mut value = demand_request();
    value["payload"]["body"]["window"] = json!({"began_ns":"0","deadline_ns":"1"});
    let request =
        CaptureEnvelope::decode(&serde_json::to_vec(&value).unwrap(), CaptureSender::Admin)
            .unwrap();
    assert_eq!(serde_json::to_value(&request).unwrap(), value);
    for window in [
        json!(null),
        json!({"began_ns":"1","deadline_ns":"1"}),
        json!({"began_ns":"2","deadline_ns":"1"}),
        json!({"began_ns":0,"deadline_ns":"1"}),
        json!({"began_ns":"00","deadline_ns":"1"}),
        json!({"began_ns":"0","deadline_ns":"18446744073709551616"}),
    ] {
        let mut wrong = value.clone();
        wrong["payload"]["body"]["window"] = window;
        assert!(
            CaptureEnvelope::decode(&serde_json::to_vec(&wrong).unwrap(), CaptureSender::Admin)
                .is_err()
        );
    }
}

#[test]
fn capture_save_point_request_refuses_missing_duplicate_and_extra_fields() {
    let value = demand_request();
    for key in ["cause", "window"] {
        let mut wrong = value.clone();
        wrong["payload"]["body"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            CaptureEnvelope::decode(&serde_json::to_vec(&wrong).unwrap(), CaptureSender::Admin)
                .is_err()
        );
    }
    for path in [
        vec![],
        vec!["exchange"],
        vec!["payload"],
        vec!["payload", "body"],
        vec!["payload", "body", "window"],
        vec!["payload", "body", "cause"],
    ] {
        let mut wrong = value.clone();
        let mut target = &mut wrong;
        for key in path {
            target = &mut target[key];
        }
        target["extra"] = json!(true);
        assert!(
            CaptureEnvelope::decode(&serde_json::to_vec(&wrong).unwrap(), CaptureSender::Admin)
                .is_err(),
            "{wrong}"
        );
    }
    let raw = serde_json::to_string(&value).unwrap();
    for key in ["began_ns", "deadline_ns"] {
        let token = format!(
            "\"{key}\":\"{}\"",
            if key == "began_ns" { "100" } else { "200" }
        );
        let duplicate = raw.replace(&token, &format!("{token},{token}"));
        assert_ne!(duplicate, raw);
        assert!(CaptureEnvelope::decode(duplicate.as_bytes(), CaptureSender::Admin).is_err());
    }
    for kind in ["leave", "join_leave", "observe", "stop"] {
        let mut wrong = value.clone();
        wrong["payload"]["body"]["kind"] = json!(kind);
        assert!(
            CaptureEnvelope::decode(&serde_json::to_vec(&wrong).unwrap(), CaptureSender::Admin)
                .is_err()
        );
    }
}
