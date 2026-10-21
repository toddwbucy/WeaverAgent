use super::*;

fn source_id() -> shared::SourceBlockId {
    shared::SourceBlockId {
        run: "run-雪".into(),
        sequence: (u64::MAX - 1).into(),
        block: 9_007_199_254_740_993.into(),
    }
}

fn at(text: &str, part: &str) -> shared::Span {
    let start = text.find(part).expect("fixture substring");
    shared::Span {
        start: (start as u64).into(),
        end: ((start + part.len()) as u64).into(),
    }
}

fn reply(target: shared::Ste100Target, result: shared::Ste100Result) -> shared::Ste100Reply {
    shared::Ste100Reply {
        request_id: u64::MAX.into(),
        target,
        scheme_id: format!("ste100:{}", "9".repeat(64)),
        result,
    }
}

fn assert_reply(value: &shared::Ste100Reply) {
    assert_eq!(
        serde_json::to_vec(&ste100(value)).unwrap(),
        serde_json::to_vec(value).unwrap(),
        "the trace mirror must preserve every field and decimal spelling",
    );
}

#[test]
fn accepted_compound_bindings_keep_exact_utf8_spans_conditions_and_target_counters() {
    let text = "If ready, read café. Open valve α and valve β.";
    let first = "If ready, read café.";
    let second = "Open valve α and valve β.";
    let bindings = vec![
        shared::Binding {
            statement_span: at(text, first),
            verb: "read".into(),
            verb_span: at(text, "read"),
            condition: Some("If ready".into()),
            condition_span: Some(at(text, "If ready")),
            nouns: vec![shared::NounBinding {
                value: "café".into(),
                span: at(text, "café"),
            }],
        },
        shared::Binding {
            statement_span: at(text, second),
            verb: "open".into(),
            verb_span: at(text, "Open"),
            condition: None,
            condition_span: None,
            nouns: vec![
                shared::NounBinding {
                    value: "valve α".into(),
                    span: at(text, "valve α"),
                },
                shared::NounBinding {
                    value: "valve β".into(),
                    span: at(text, "valve β"),
                },
            ],
        },
    ];
    for target in [
        shared::Ste100Target::Source(source_id()),
        shared::Ste100Target::Input {
            input_id: u64::MAX.into(),
            block: (u64::MAX - 2).into(),
        },
    ] {
        let value = reply(
            target,
            shared::Ste100Result::Accepted {
                bindings: bindings.clone(),
            },
        );
        value
            .validate_for(
                &shared::Ste100Request {
                    request_id: value.request_id,
                    target: value.target.clone(),
                    text: text.into(),
                    scheme_id: value.scheme_id.clone(),
                },
                &["read".into(), "open".into()],
            )
            .unwrap();
        assert_reply(&value);
    }
    let mut step = bindings;
    step[0].condition = Some("step:3".into());
    assert_reply(&reply(
        shared::Ste100Target::Source(source_id()),
        shared::Ste100Result::Accepted { bindings: step },
    ));
}

#[test]
fn empty_template_acceptance_rejection_and_every_component_result_keep_their_shapes() {
    let template = shared::TemplateSpec {
        template_id: "fixture.template".into(),
        kind: "loop.instruction".into(),
        parts: vec![shared::TemplatePart::Literal("Read valve.".into())],
    };
    let value = reply(
        shared::Ste100Target::Template {
            template_id: template.template_id.clone(),
        },
        shared::Ste100Result::Accepted {
            bindings: Vec::new(),
        },
    );
    value
        .validate_for(
            &shared::Ste100Request {
                request_id: value.request_id,
                target: value.target.clone(),
                text: serde_json::to_string(&template).unwrap(),
                scheme_id: value.scheme_id.clone(),
            },
            &["read".into()],
        )
        .unwrap();
    assert_reply(&value);
    assert_reply(&reply(
        shared::Ste100Target::Source(source_id()),
        shared::Ste100Result::NotSte100 {
            code: shared::Ste100DefectCode::NonSte100,
            detail: "Exact café\n\"detail\"\\".into(),
            spans: vec![
                shared::Span {
                    start: 0.into(),
                    end: 5.into(),
                },
                shared::Span {
                    start: 9_007_199_254_740_993.into(),
                    end: u64::MAX.into(),
                },
            ],
        },
    ));
    for code in [
        shared::Ste100ComponentCode::Crashed,
        shared::Ste100ComponentCode::NoResult,
        shared::Ste100ComponentCode::InvalidResult,
        shared::Ste100ComponentCode::TransportFailed,
    ] {
        assert_reply(&reply(
            shared::Ste100Target::Input {
                input_id: 11.into(),
                block: 0.into(),
            },
            shared::Ste100Result::ComponentFailed {
                code,
                detail: "unchanged Ω evidence".into(),
            },
        ));
    }
}

fn measured(operation: shared::StateOperation) -> shared::StateStoreMeasurement {
    let base = 9_007_199_254_740_993;
    shared::StateStoreMeasurement {
        observation: shared::StateOperationObservation {
            operation_id: u64::MAX.into(),
            parent_id: None,
            request_id: (u64::MAX - 1).into(),
            operation,
            began_ns: base.into(),
            ended_ns: (base + 20).into(),
            phases: [
                shared::PhaseName::Validate,
                shared::PhaseName::Query,
                shared::PhaseName::Transaction,
                shared::PhaseName::Capture,
                shared::PhaseName::Filesystem,
                shared::PhaseName::Encode,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, name)| shared::OperationPhase {
                name,
                began_ns: (base + index as u64 * 2).into(),
                ended_ns: (base + index as u64 * 2 + 1).into(),
            })
            .collect(),
            resource: shared::OperationResource {
                cpu_ns: None,
                copied_bytes: Some(u64::MAX.into()),
                held_bytes: Some(base.into()),
                unavailable: vec![shared::ResourceUnavailable {
                    field: shared::ResourceField::CpuNs,
                    reason: shared::ObservationUnavailable::NotSampled,
                }],
            },
            disk: shared::OperationDisk {
                enabled: None,
                before: None,
                after: None,
                unavailable_reason: Some(shared::ObservationUnavailable::NotSampled),
            },
        },
        request_source: operation.is_filing().then(source_id),
        harness_queued_ns: (base - 2).into(),
        harness_sent_ns: (base - 1).into(),
        harness_received_ns: (base + 21).into(),
    }
}

fn assert_measurement(value: &shared::StateStoreMeasurement) {
    assert_eq!(
        serde_json::to_vec(&measurement(value)).unwrap(),
        serde_json::to_vec(value).unwrap()
    );
}

#[test]
fn every_state_operation_and_phase_preserve_clocks_sources_and_required_nulls() {
    for operation in [
        shared::StateOperation::Open,
        shared::StateOperation::File,
        shared::StateOperation::FilePair,
        shared::StateOperation::Read,
        shared::StateOperation::Next,
        shared::StateOperation::Release,
        shared::StateOperation::Shape,
        shared::StateOperation::Held,
        shared::StateOperation::Position,
        shared::StateOperation::Grants,
        shared::StateOperation::Snapshot,
        shared::StateOperation::Acknowledge,
        shared::StateOperation::Abort,
        shared::StateOperation::ReleaseCapture,
    ] {
        let value = measured(operation);
        value.validate().unwrap();
        assert_measurement(&value);
    }
}

#[test]
fn every_resource_field_and_unavailable_reason_survive_the_same_conversion() {
    for reason in [
        shared::ObservationUnavailable::Unsupported,
        shared::ObservationUnavailable::Denied,
        shared::ObservationUnavailable::Disabled,
        shared::ObservationUnavailable::ReadFailed,
        shared::ObservationUnavailable::CounterReset,
        shared::ObservationUnavailable::Overflow,
        shared::ObservationUnavailable::Uninstrumented,
        shared::ObservationUnavailable::NotSampled,
    ] {
        let mut value = measured(shared::StateOperation::Snapshot);
        value.observation.resource = shared::OperationResource {
            cpu_ns: None,
            copied_bytes: None,
            held_bytes: None,
            unavailable: [
                shared::ResourceField::CpuNs,
                shared::ResourceField::CopiedBytes,
                shared::ResourceField::HeldBytes,
            ]
            .into_iter()
            .map(|field| shared::ResourceUnavailable { field, reason })
            .collect(),
        };
        value.observation.disk.unavailable_reason = Some(reason);
        value.validate().unwrap();
        assert_measurement(&value);
    }
}

#[test]
fn available_disk_resource_counters_and_optional_values_remain_exact() {
    let mut value = measured(shared::StateOperation::Snapshot);
    value.observation.resource = shared::OperationResource {
        cpu_ns: Some(9_007_199_254_740_993.into()),
        copied_bytes: Some(0.into()),
        held_bytes: Some(u64::MAX.into()),
        unavailable: Vec::new(),
    };
    let before = shared::DiskSample {
        major_faults: 9_007_199_254_740_993.into(),
        read_bytes: (u64::MAX - 3).into(),
        write_bytes: (u64::MAX - 2).into(),
        blkio_delay_ticks: (u64::MAX - 1).into(),
        ticks_per_second: 100.into(),
    };
    let after = shared::DiskSample {
        major_faults: u64::MAX.into(),
        read_bytes: u64::MAX.into(),
        write_bytes: u64::MAX.into(),
        blkio_delay_ticks: u64::MAX.into(),
        ticks_per_second: 100.into(),
    };
    for enabled in [true, false] {
        value.observation.disk = shared::OperationDisk {
            enabled: Some(enabled),
            before: Some(before.clone()),
            after: Some(after.clone()),
            unavailable_reason: None,
        };
        value.validate().unwrap();
        assert_measurement(&value);
    }
    // Conversion owns no validation policy. A future parent observation value
    // must not silently become null in a pure mirror of the supplied struct.
    value.observation.parent_id = Some(u64::MAX.into());
    assert_measurement(&value);
}
