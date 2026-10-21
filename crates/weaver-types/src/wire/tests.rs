use super::*;

#[test]
fn state_fault_cases_keep_component_and_conformance_results_distinct() {
    for (case, spelling) in [
        (FaultCase::StateFlowFailed, "state_flow_failed"),
        (FaultCase::StateTemplateRejected, "state_template_rejected"),
    ] {
        let encoded = serde_json::to_value(case).unwrap();
        assert_eq!(encoded, spelling);
        assert_eq!(serde_json::from_value::<FaultCase>(encoded).unwrap(), case);
    }
    assert!(serde_json::from_str::<FaultCase>("\"state_rejected\"").is_err());
}

#[test]
fn input_refusal_keeps_exact_case_attribution_and_flat_seam_shape() {
    use crate::state_flow::InputRefusalCase;

    for (case, spelling) in [
        (InputRefusalCase::NonSte100, "non_ste100"),
        (InputRefusalCase::InvalidInputShape, "invalid_input_shape"),
    ] {
        let refusal = crate::InputRefusal {
            input_id: u64::MAX.into(),
            case,
            detail: "Read \"caf\u{e9}\".\n\\unchanged".into(),
        };
        let record = RefusalRecord::Input(refusal.clone());
        let expected = format!(
            "{{\"seam\":\"input\",\"input_id\":\"18446744073709551615\",\"case\":\"{spelling}\",\"detail\":\"Read \\\"caf\u{e9}\\\".\\n\\\\unchanged\"}}"
        );
        let encoded = serde_json::to_string(&record).unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(
            serde_json::from_str::<RefusalRecord>(&encoded).unwrap(),
            record
        );
        assert_eq!(
            crate::state_flow::decode_strict::<RefusalRecord>(&encoded).unwrap(),
            record
        );
        let body = serde_json::to_string(&refusal).unwrap();
        assert_eq!(
            serde_json::from_str::<InputRefusal>(&body).unwrap(),
            refusal
        );
        assert!(!body.contains("seam"));
    }
}

#[test]
fn input_refusal_rejects_missing_extra_duplicate_and_noncanonical_members() {
    let valid = r#"{"seam":"input","input_id":"1","case":"non_ste100","detail":"Read file."}"#;
    assert!(serde_json::from_str::<RefusalRecord>(valid).is_ok());

    for rejected in [
        valid.replace("\"seam\":\"input\",", ""),
        valid.replace("\"input_id\":\"1\",", ""),
        valid.replace("\"case\":\"non_ste100\",", ""),
        valid.replace(",\"detail\":\"Read file.\"", ""),
        valid.replace("\"input_id\":\"1\"", "\"input_id\":null"),
        valid.replace("\"case\":\"non_ste100\"", "\"case\":null"),
        valid.replace("\"detail\":\"Read file.\"", "\"detail\":null"),
        valid.replace("\"input_id\":\"1\"", "\"input_id\":1"),
        valid.replace("\"input_id\":\"1\"", "\"input_id\":\"01\""),
        valid.replace("\"input_id\":\"1\"", "\"input_id\":\"-1\""),
        valid.replace(
            "\"input_id\":\"1\"",
            "\"input_id\":\"18446744073709551616\"",
        ),
        valid.replace("non_ste100", "component_failed"),
        valid.replace(
            "\"seam\":\"input\"",
            "\"seam\":\"input\",\"seam\":\"input\"",
        ),
        valid.replace(
            "\"input_id\":\"1\"",
            "\"input_id\":\"1\",\"input_id\":\"2\"",
        ),
        valid.replace(
            "\"case\":\"non_ste100\"",
            "\"case\":\"non_ste100\",\"case\":\"invalid_input_shape\"",
        ),
        valid.replace(
            "\"detail\":\"Read file.\"",
            "\"detail\":\"Read file.\",\"detail\":\"other\"",
        ),
        valid.replace("}", ",\"source_id\":null}"),
        valid.replace("}", ",\"refusal\":{}}"),
        r#"{"seam":"input","refusal":{"input_id":"1","case":"non_ste100","detail":"Read file."}}"#
            .into(),
    ] {
        assert!(
            serde_json::from_str::<RefusalRecord>(&rejected).is_err(),
            "accepted {rejected}"
        );
        assert!(crate::state_flow::decode_strict::<RefusalRecord>(&rejected).is_err());
    }
    assert!(
        serde_json::from_str::<InputRefusal>(
            r#"{"input_id":"1","case":"non_ste100","detail":"Read file.","extra":false}"#
        )
        .is_err()
    );
}

#[test]
fn input_refusal_does_not_change_existing_refusal_shapes() {
    let cases = [
        (
            RefusalRecord::Decode {
                asked: TokenAsk::Open,
                refusal: TokenRefusal::NotOpen,
            },
            r#"{"seam":"decode","asked":{"ask":"open"},"refusal":{"kind":"not_open"}}"#,
        ),
        (
            RefusalRecord::Lifecycle {
                asked: LifecycleAsk::Observe,
                refusal: LifecycleRefusal::OutOfOrder,
            },
            r#"{"seam":"lifecycle","asked":{"ask":"observe"},"refusal":{"kind":"out_of_order"}}"#,
        ),
        (
            RefusalRecord::Classify {
                refusal: LabelRefusal::NotReady,
            },
            r#"{"seam":"classify","refusal":{"kind":"not_ready"}}"#,
        ),
    ];
    for (record, expected) in cases {
        assert_eq!(serde_json::to_string(&record).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<RefusalRecord>(expected).unwrap(),
            record
        );
    }
}

#[test]
fn protocol_repair_refusal_keeps_exact_source_case_and_flat_seam_shape() {
    use crate::state_flow::{ProtocolRepairCase, SourceBlockId};

    for (case, spelling) in [
        (ProtocolRepairCase::CorrectionCall, "correction_call"),
        (
            ProtocolRepairCase::MissingToolStatement,
            "missing_tool_statement",
        ),
        (
            ProtocolRepairCase::AmbiguousToolStatement,
            "ambiguous_tool_statement",
        ),
    ] {
        let record = RefusalRecord::ProtocolRepair {
            source_id: SourceBlockId {
                run: "run-7".into(),
                sequence: u64::MAX.into(),
                block: 42.into(),
            },
            case,
            detail: "Read \"caf\u{e9}\".\n\\unchanged".into(),
        };
        let expected = format!(
            "{{\"seam\":\"protocol_repair\",\"source_id\":{{\"run\":\"run-7\",\"sequence\":\"18446744073709551615\",\"block\":\"42\"}},\"case\":\"{spelling}\",\"detail\":\"Read \\\"caf\u{e9}\\\".\\n\\\\unchanged\"}}"
        );
        assert_eq!(serde_json::to_string(&record).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<RefusalRecord>(&expected).unwrap(),
            record
        );
        assert_eq!(
            crate::state_flow::decode_strict::<RefusalRecord>(&expected).unwrap(),
            record
        );
    }
}

#[test]
fn instruction_refusal_keeps_exact_template_case_and_flat_seam_shape() {
    use crate::state_flow::InstructionRefusal;

    for (case, spelling) in [
        (InstructionRefusal::InvalidTemplate, "invalid_template"),
        (InstructionRefusal::InvalidContext, "invalid_context"),
        (InstructionRefusal::StaleSource, "stale_source"),
        (InstructionRefusal::WrongTurn, "wrong_turn"),
    ] {
        let record = RefusalRecord::Instruction {
            template_id: "loop.restatement".into(),
            case,
            detail: "Read \"caf\u{e9}\".\n\\unchanged".into(),
        };
        let expected = format!(
            "{{\"seam\":\"instruction\",\"template_id\":\"loop.restatement\",\"case\":\"{spelling}\",\"detail\":\"Read \\\"caf\u{e9}\\\".\\n\\\\unchanged\"}}"
        );
        assert_eq!(serde_json::to_string(&record).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<RefusalRecord>(&expected).unwrap(),
            record
        );
        assert_eq!(
            crate::state_flow::decode_strict::<RefusalRecord>(&expected).unwrap(),
            record
        );
    }
}

/// These are exact closed carriers. Removing the strict-key or case checks must
/// admit a foreign refusal family and fail this fixture.
#[test]
fn repair_and_instruction_refusals_reject_missing_null_and_foreign_members() {
    let repair = r#"{"seam":"protocol_repair","source_id":{"run":"run","sequence":"1","block":"0"},"case":"correction_call","detail":"refused"}"#;
    let instruction = r#"{"seam":"instruction","template_id":"loop.restatement","case":"invalid_template","detail":"refused"}"#;
    for (valid, required) in [
        (repair, ["seam", "source_id", "case", "detail"]),
        (instruction, ["seam", "template_id", "case", "detail"]),
    ] {
        let original: serde_json::Value = serde_json::from_str(valid).unwrap();
        assert!(crate::state_flow::decode_strict::<RefusalRecord>(valid).is_ok());
        for field in required {
            let mut missing = original.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert_refusal_rejected(&missing.to_string());
            let mut null = original.clone();
            null[field] = serde_json::Value::Null;
            assert_refusal_rejected(&null.to_string());
        }
        for (field, value) in [
            ("refusal", serde_json::json!({})),
            ("asked", serde_json::json!({"ask":"open"})),
            ("input_id", serde_json::json!("1")),
            ("extra", serde_json::json!(true)),
        ] {
            let mut alien = original.clone();
            alien[field] = value;
            assert_refusal_rejected(&alien.to_string());
        }
        for case in [
            "non_ste100",
            "component_failed",
            "empty_correction",
            "CorrectionCall",
        ] {
            let mut alien = original.clone();
            alien["case"] = serde_json::json!(case);
            assert_refusal_rejected(&alien.to_string());
        }
    }
    assert_refusal_rejected(&repair.replace("correction_call", "wrong_turn"));
    assert_refusal_rejected(&instruction.replace("invalid_template", "correction_call"));
    assert_refusal_rejected(
        &repair.replace("\"detail\":", "\"template_id\":\"foreign\",\"detail\":"),
    );
    assert_refusal_rejected(&instruction.replace("\"detail\":", "\"source_id\":null,\"detail\":"));
    for bad in ["1", "null", "\"01\"", "\"-1\"", "\"18446744073709551616\""] {
        assert_refusal_rejected(
            &repair.replace("\"sequence\":\"1\"", &format!("\"sequence\":{bad}")),
        );
    }
}

#[test]
fn repair_and_instruction_refusals_reject_duplicate_attribution_and_nested_aliases() {
    for valid in [
        r#"{"seam":"protocol_repair","source_id":{"run":"run","sequence":"1","block":"0"},"case":"correction_call","detail":"refused"}"#,
        r#"{"seam":"instruction","template_id":"loop.restatement","case":"invalid_template","detail":"refused"}"#,
    ] {
        let fields: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(valid).unwrap();
        for (key, value) in fields {
            let duplicated = format!("{},\"{key}\":{value}}}", valid.strip_suffix('}').unwrap());
            assert_refusal_rejected(&duplicated);
        }
    }
    let duplicate_nested = r#"{"seam":"protocol_repair","source_id":{"run":"run","sequence":"1","\u0073equence":"2","block":"0"},"case":"correction_call","detail":"refused"}"#;
    assert_refusal_rejected(duplicate_nested);
}

fn assert_refusal_rejected(json: &str) {
    assert!(
        serde_json::from_str::<RefusalRecord>(json).is_err(),
        "accepted {json}"
    );
    assert!(
        crate::state_flow::decode_strict::<RefusalRecord>(json).is_err(),
        "strict codec accepted {json}"
    );
}

/// **A save point report carries its seven members and no other** (#99,
/// K9): the report admin writes into a root-owned manifest round-trips,
/// and the same report with one field more fails to deserialize.
/// Perturbation: remove `deny_unknown_fields` from `SavePointReport` and
/// the extended report reads.
#[test]
fn a_save_point_report_with_an_extra_field_is_refused() {
    let report = SavePointReport {
        save_point: "ab".into(),
        name: "ab.save-point".into(),
        run: RunId("r-0".into()),
        sequence: 5,
        turn: 1,
        event_run: RunId("r-1".into()),
        position: 7,
    };
    let mut value = serde_json::to_value(&report).expect("serializes");
    assert_eq!(
        serde_json::from_value::<SavePointReport>(value.clone()).expect("round-trips"),
        report
    );
    value
        .as_object_mut()
        .expect("an object")
        .insert("extra".into(), serde_json::json!(1));
    assert!(
        serde_json::from_value::<SavePointReport>(value).is_err(),
        "a report with an extra field is refused"
    );
}
