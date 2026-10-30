//! TYPES-PLAN-PORT-REFUSAL's table, independent of harness proof admission.
use serde_json::{Value, json};
use weaver_types::RefusalRecord;
use weaver_types::state_flow::{
    PlanBreakCase, PlanBreakRecord, PlanBreakReport, PlanEffect, PlanOperationRefused,
    decode_strict,
};

fn expected() -> Value {
    json!({"plan_id":{"seed_id":"requested-seed","local_id":"4"},"revision":"9"})
}
fn step() -> Value {
    json!({"seed_id":"requested-seed","local_id":"8"})
}
fn refusal(case: &str, code: Value) -> Value {
    json!({"case":case,"detail":"exact member detail\nwith spacing  ","read_code":code})
}
fn record(refusal: Value) -> Value {
    json!({"seam":"plan_operation","operation":"assess_act",
        "expected":expected(),"step_id":step(),"refusal":refusal})
}
fn accepts(value: Value, accepted: bool) {
    let outcome = decode_strict::<PlanOperationRefused>(&value.to_string());
    assert_eq!(outcome.is_ok(), accepted, "outcome: {value}");
    let envelope = record(value.clone());
    let decoded = decode_strict::<RefusalRecord>(&envelope.to_string());
    assert_eq!(decoded.is_ok(), accepted, "record: {envelope}");
    if let Ok(outcome) = outcome {
        assert_eq!(serde_json::to_value(outcome).unwrap(), value);
        assert_eq!(serde_json::to_value(decoded.unwrap()).unwrap(), envelope);
    }
}

#[test]
fn plan_operation_case_read_code_matrix_matches_the_spec() {
    let codes = [
        Value::Null,
        json!("unsupported_operation"),
        json!("invalid_selector"),
        json!("wrong_seed"),
        json!("not_found"),
        json!("unknown_span"),
        json!("span_open"),
        json!("span_pending"),
        json!("partial_pair"),
        json!("counter_exhausted"),
        json!("read_busy"),
        json!("unknown_snapshot"),
        json!("wrong_offset"),
        json!("result_too_large"),
        json!("result_unavailable"),
        json!("answer_too_large"),
        json!("invented"),
    ];
    // Each row is transcribed from TYPES-PLAN-PORT-REFUSAL, not obtained from
    // PlanOperationRefused::validate or another implementation helper.
    for (case, allowed) in [
        ("no_locked_plan", vec![Value::Null]),
        ("wrong_category", vec![Value::Null]),
        ("unknown_tool", vec![Value::Null]),
        ("self_edit", vec![Value::Null]),
        ("invalid_target", vec![Value::Null, json!("not_found")]),
        (
            "work_changed",
            vec![
                Value::Null,
                json!("invalid_selector"),
                json!("span_pending"),
                json!("partial_pair"),
            ],
        ),
        ("review_setup_changed", vec![Value::Null]),
        ("review_required", vec![Value::Null]),
        (
            "read_refused",
            vec![
                json!("counter_exhausted"),
                json!("result_too_large"),
                json!("result_unavailable"),
                json!("answer_too_large"),
                json!("unknown_snapshot"),
                json!("wrong_offset"),
            ],
        ),
        ("counter_exhausted", vec![Value::Null]),
        ("template_rejected", vec![Value::Null]),
    ] {
        for code in &codes {
            accepts(refusal(case, code.clone()), allowed.contains(code));
        }
    }
}

#[test]
fn plan_operation_outcome_requires_exact_object_fields_and_string_codes() {
    let valid = refusal("invalid_target", Value::Null);
    for field in ["case", "detail", "read_code"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        accepts(missing, false);
    }
    let mut extra = valid.clone();
    extra["rejected_blocks"] = json!([]);
    accepts(extra, false);
    let mut object_case = valid.clone();
    object_case["case"] = json!({"invalid_target":null});
    accepts(object_case, false);
    let mut object_code = valid;
    object_code["read_code"] = json!({"not_found":null});
    accepts(object_code, false);
    accepts(json!(["invalid_target", "detail", null]), false);
    assert!(
        decode_strict::<PlanOperationRefused>(
            r#"{"case":"invalid_target","detail":"first","detail":"second","read_code":null}"#
        )
        .is_err()
    );
}

#[test]
fn plan_operation_record_preserves_requested_identity_and_requires_its_step() {
    for operation in [
        "activate",
        "assess_act",
        "work_reference",
        "observe_review",
        "attest_closure",
        "close",
        "record_break",
    ] {
        let mut value = record(refusal("invalid_target", Value::Null));
        value["operation"] = json!(operation);
        let decoded = decode_strict::<RefusalRecord>(&value.to_string()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        value["step_id"] = Value::Null;
        assert_eq!(
            decode_strict::<RefusalRecord>(&value.to_string()).is_ok(),
            operation == "record_break",
            "{value}"
        );
        value.as_object_mut().unwrap().remove("step_id");
        assert!(decode_strict::<RefusalRecord>(&value.to_string()).is_err());
    }
}

#[test]
fn plan_operation_record_rejects_missing_foreign_and_non_object_fields() {
    let valid = record(refusal("work_changed", json!("partial_pair")));
    for field in ["operation", "expected", "step_id", "refusal"] {
        let mut value = valid.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(decode_strict::<RefusalRecord>(&value.to_string()).is_err());
    }
    for (field, value) in [
        ("operation", json!({"assess_act":null})),
        ("operation", json!("root_update")),
        ("expected", json!([{"seed_id":"s","local_id":"4"},"9"])),
        ("step_id", json!(["s", "8"])),
        ("refusal", json!(["work_changed", "detail", "partial_pair"])),
        ("asked", json!({"ask":"plan_update"})),
    ] {
        let mut wire = valid.clone();
        wire[field] = value;
        assert!(
            decode_strict::<RefusalRecord>(&wire.to_string()).is_err(),
            "{wire}"
        );
    }
    let duplicate = valid.to_string().replacen(
        "\"operation\":\"assess_act\"",
        "\"operation\":\"assess_act\",\"operation\":\"close\"",
        1,
    );
    assert_ne!(duplicate, valid.to_string());
    assert!(decode_strict::<RefusalRecord>(&duplicate).is_err());
}

#[test]
fn resource_and_template_refusals_cannot_enter_durable_break_carriers() {
    for case in [
        "invalid_target",
        "read_refused",
        "counter_exhausted",
        "template_rejected",
    ] {
        let allowed = case == "invalid_target";
        let effect = json!({"break":{"expected":expected(),"step_id":null,"case":case,"proposed_source":null}});
        let held = json!({"case":case,"step_id":null,"proposed_source":null,
            "source":{"run":"r","sequence":"7","block":"0"}});
        let report = json!({"plan":expected(),"step_id":null,"case":case,"proposed_source":null});
        assert_eq!(
            decode_strict::<PlanBreakCase>(&json!(case).to_string()).is_ok(),
            allowed
        );
        assert_eq!(
            decode_strict::<PlanEffect>(&effect.to_string()).is_ok(),
            allowed
        );
        assert_eq!(
            decode_strict::<PlanBreakRecord>(&held.to_string()).is_ok(),
            allowed
        );
        assert_eq!(
            decode_strict::<PlanBreakReport>(&report.to_string()).is_ok(),
            allowed
        );
    }
}
