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
