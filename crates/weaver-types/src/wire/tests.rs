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
