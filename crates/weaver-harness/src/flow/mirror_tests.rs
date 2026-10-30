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

fn pair_id() -> shared::CallId {
    shared::CallId {
        run: "pair-run-\u{96ea}".into(),
        sequence: (u64::MAX - 1).into(),
        call: 9_007_199_254_740_993.into(),
    }
}

fn review_work() -> shared::WorkRef {
    shared::WorkRef {
        plan: shared::PlanRef {
            plan_id: shared::PlanId {
                seed_id: "plan-seed".into(),
                local_id: u64::MAX.into(),
            },
            revision: 9_007_199_254_740_993.into(),
        },
        step_id: shared::StepId {
            seed_id: "step-seed".into(),
            local_id: (u64::MAX - 2).into(),
        },
        span: shared::SpanRef {
            seed_id: "span-seed".into(),
            local_id: (u64::MAX - 3).into(),
            revision: (u64::MAX - 4).into(),
        },
        through: shared::SourcePosition {
            block_id: source_id(),
            turn: Some("work-turn-\u{3a9}".into()),
        },
        content_sha256: shared::Digest::new("3".repeat(64)).unwrap(),
    }
}

fn assert_started(value: &shared::PairStarted) {
    assert_eq!(
        serde_json::to_vec(&pair_started(value)).unwrap(),
        serde_json::to_vec(value).unwrap(),
        "the opening mirror must preserve exact typed bytes and field order",
    );
}

fn assert_ended(value: &shared::PairEnded) {
    assert_eq!(
        serde_json::to_vec(&pair_ended(value)).unwrap(),
        serde_json::to_vec(value).unwrap(),
        "the terminal mirror must preserve exact typed bytes and field order",
    );
}

#[test]
fn every_opening_lane_keeps_request_identity_review_refs_and_nullable_retry() {
    let work = review_work();
    let requests = [
        shared::PairRequest::External {
            name: "sh".into(),
            arguments: "printf '%s\\n' '\u{3a9} {raw}'\n".into(),
        },
        shared::PairRequest::Recall {
            subject: "exact \"subject\" \\ \u{96ea}".into(),
        },
        shared::PairRequest::Review {
            setup: shared::ReviewSetupRef {
                setup_id: "setup-id".into(),
                sha256: shared::Digest::new("4".repeat(64)).unwrap(),
            },
            work: Box::new(work),
            command: "exact review\ncommand".into(),
        },
    ];
    for request in requests {
        for retry_of in [None, Some(pair_id())] {
            let value = shared::PairStarted {
                call_id: pair_id(),
                call_block: u64::MAX.into(),
                statement: shared::StatementRef {
                    block_id: source_id(),
                    binding: 9_007_199_254_740_995.into(),
                },
                lane: request.lane(),
                request: request.clone(),
                retry_of,
            };
            value.validate_structure().unwrap();
            assert_started(&value);
        }
    }
}

#[test]
fn review_work_without_a_turn_keeps_its_null_and_distinct_identity_counters() {
    let mut work = review_work();
    work.through.turn = None;
    let value = shared::PairStarted {
        call_id: pair_id(),
        call_block: 7.into(),
        statement: shared::StatementRef {
            block_id: source_id(),
            binding: 8.into(),
        },
        lane: shared::PairLane::Review,
        request: shared::PairRequest::Review {
            setup: shared::ReviewSetupRef {
                setup_id: "review-setup".into(),
                sha256: shared::Digest::new("5".repeat(64)).unwrap(),
            },
            work: Box::new(work),
            command: "review unchanged".into(),
        },
        retry_of: None,
    };
    assert_started(&value);
    // A conversion is not the lane validator. It must not repair this supplied
    // evidence into another request or silently replace the caller's lane.
    let mut mismatched = value;
    mismatched.lane = shared::PairLane::Recall;
    assert_started(&mismatched);
}

fn terminal() -> shared::PairEnded {
    shared::PairEnded {
        call_id: pair_id(),
        execution: None,
        fate: shared::TerminalPairFate::Completed,
        reason: None,
        outcome: None,
        loaded_from: None,
    }
}

#[test]
fn every_terminal_failure_and_all_seven_non_dispatch_causes_keep_their_tags() {
    let mut reasons = vec![
        shared::PairFailure::Timeout {},
        shared::PairFailure::Cancelled {},
        shared::PairFailure::Unload {},
        shared::PairFailure::FlowFailure {},
        shared::PairFailure::GateFault {},
        shared::PairFailure::NotGranted {},
        shared::PairFailure::AbandonedAtLoad {},
    ];
    reasons.extend(
        [
            shared::NonDispatchCause::PassPurpose,
            shared::NonDispatchCause::RepairExhausted,
            shared::NonDispatchCause::PlanBreak,
            shared::NonDispatchCause::RoundLimit,
            shared::NonDispatchCause::Stop,
            shared::NonDispatchCause::Unload,
            shared::NonDispatchCause::NoGate,
        ]
        .into_iter()
        .map(|cause| shared::PairFailure::NotDispatched { cause }),
    );
    assert_eq!(reasons.len(), 14);
    for reason in reasons {
        let mut value = terminal();
        value.fate = shared::TerminalPairFate::Failed;
        value.reason = Some(reason);
        assert_ended(&value);
    }
    assert_ended(&terminal());
    assert_eq!(
        serde_json::to_string(&pair_ended(&terminal())).unwrap(),
        concat!(
            "{\"call_id\":{\"run\":\"pair-run-\u{96ea}\",",
            "\"sequence\":\"18446744073709551614\",\"call\":\"9007199254740993\"},",
            "\"execution\":null,\"fate\":\"completed\",\"reason\":null,",
            "\"outcome\":null,\"loaded_from\":null}"
        ),
    );
}

#[test]
fn every_tool_outcome_kill_cause_and_execution_opener_preserves_exact_wire_shape() {
    let mut outcomes = vec![
        weaver_types::ToolOutcome::Result {
            content: "unchanged result\n\u{96ea}".into(),
        },
        weaver_types::ToolOutcome::Refused {
            reason: "exact refused \"reason\"".into(),
        },
        weaver_types::ToolOutcome::Errored {
            detail: "exact infrastructure\\detail".into(),
        },
    ];
    for by in [
        weaver_types::KillCause::Clock,
        weaver_types::KillCause::Cancel,
        weaver_types::KillCause::Unload,
        weaver_types::KillCause::FlowFailure,
        weaver_types::KillCause::Fault,
    ] {
        for partial in [None, Some(String::new()), Some("partial\n\u{3a9}".into())] {
            outcomes.push(weaver_types::ToolOutcome::Killed { partial, by });
        }
    }
    assert_eq!(outcomes.len(), 18);
    for outcome in outcomes {
        for opener in [
            weaver_types::Opener::Admin,
            weaver_types::Opener::Harness,
            weaver_types::Opener::Spu,
            weaver_types::Opener::Gate,
        ] {
            let mut value = terminal();
            value.execution = Some(weaver_types::ExchangeId {
                opener,
                ordinal: u64::MAX,
            });
            if matches!(outcome, weaver_types::ToolOutcome::Killed { .. }) {
                value.fate = shared::TerminalPairFate::Failed;
            }
            value.outcome = Some(outcome.clone());
            assert_ended(&value);
        }
    }
}

#[test]
fn abandoned_at_load_preserves_the_complete_start_identity_and_its_optional_position() {
    let identity = shared::StartIdentity {
        seed_id: "seed-\u{3a9}".into(),
        artifact_id: "artifact-1".into(),
        dump_sha256: shared::Digest::new("1".repeat(64)).unwrap(),
        config_sha256: shared::Digest::new("2".repeat(64)).unwrap(),
        parent_artifact_id: Some("parent-artifact".into()),
        position: Some(shared::SourcePosition {
            block_id: source_id(),
            turn: Some("prior-turn".into()),
        }),
    };
    for position in [None, identity.position.clone()] {
        for parent in [None, identity.parent_artifact_id.clone()] {
            let mut identity = identity.clone();
            identity.position = position.clone();
            identity.parent_artifact_id = parent;
            let mut value = terminal();
            value.fate = shared::TerminalPairFate::Failed;
            value.reason = Some(shared::PairFailure::AbandonedAtLoad {});
            value.loaded_from = Some(identity);
            assert_ended(&value);
        }
    }
}

fn plan_measured(event: shared::PlanMeasureEvent) -> shared::PlanMeasurement {
    shared::PlanMeasurement {
        instrument_version: "plan-observation/1".into(),
        plan: shared::PlanRef {
            plan_id: shared::PlanId {
                seed_id: "seed-plan".into(),
                local_id: (u64::MAX - 3).into(),
            },
            revision: 7.into(),
        },
        event,
    }
}

fn plan_step(local_id: u64) -> shared::StepId {
    shared::StepId {
        seed_id: "seed-plan".into(),
        local_id: local_id.into(),
    }
}

#[test]
fn next_ready_keeps_both_steps_its_assembly_and_every_distinct_clock_reading() {
    let value = plan_measured(shared::PlanMeasureEvent::NextReady {
        closed: source_id(),
        previous_step: plan_step(4),
        step_id: plan_step(5),
        assembly_id: 9.into(),
        began_ns: (u64::MAX - 40).into(),
        ended_ns: (u64::MAX - 12).into(),
        closure_to_next_ns: 28.into(),
    });
    let mirrored = serde_json::to_value(plan_measurement(&value)).unwrap();
    assert_eq!(mirrored, serde_json::to_value(&value).unwrap());
    assert_eq!(
        serde_json::to_vec(&plan_measurement(&value)).unwrap(),
        serde_json::to_vec(&value).unwrap(),
        "the trace mirror must preserve every field and decimal spelling",
    );
    assert_eq!(
        mirrored["event"]["next_ready"]["previous_step"]["local_id"],
        "4"
    );
    assert_eq!(
        mirrored["event"]["next_ready"]["began_ns"],
        (u64::MAX - 40).to_string()
    );
}

#[test]
fn fit_refused_keeps_every_stage_and_code_its_step_assembly_and_refusal_count() {
    for (stage, stage_name) in [
        (shared::PlanFitStage::MandatoryProbe, "mandatory_probe"),
        (shared::PlanFitStage::PlaceBackstop, "place_backstop"),
    ] {
        for (code, code_name) in [
            (shared::PlanFitCode::TailOverCapacity, "tail_over_capacity"),
            (shared::PlanFitCode::GenerationRoom, "generation_room"),
        ] {
            let value = plan_measured(shared::PlanMeasureEvent::FitRefused {
                step_id: plan_step(6),
                assembly_id: (u64::MAX - 2).into(),
                stage,
                code,
                mandatory_fit_refusals: 3.into(),
            });
            let mirrored = serde_json::to_value(plan_measurement(&value)).unwrap();
            assert_eq!(
                serde_json::to_vec(&plan_measurement(&value)).unwrap(),
                serde_json::to_vec(&value).unwrap(),
                "{stage_name}/{code_name}",
            );
            let event = &mirrored["event"]["fit_refused"];
            assert_eq!(event["stage"], stage_name);
            assert_eq!(event["code"], code_name);
            assert_eq!(event["step_id"]["local_id"], "6");
            assert_eq!(event["assembly_id"], (u64::MAX - 2).to_string());
            assert_eq!(event["mandatory_fit_refusals"], "3");
        }
    }
}

#[test]
fn flag_readings_keep_their_measured_snapshot_or_typed_unavailability() {
    let measured = shared::PlanFlagSnapshot {
        plan: plan_measured(shared::PlanMeasureEvent::Flags {
            after: source_id(),
            request_id: 1.into(),
            observation: shared::PlanFlagObservation::Unavailable {
                code: shared::ReadRefusalCode::ReadBusy,
            },
        })
        .plan,
        after: source_id(),
        through: shared::SourcePosition {
            block_id: source_id(),
            turn: None,
        },
        total_flags: 5.into(),
        matched_flags: 2.into(),
        unmatched_flags: 3.into(),
    };
    for observation in [
        shared::PlanFlagObservation::Measured { snapshot: measured },
        shared::PlanFlagObservation::Unavailable {
            code: shared::ReadRefusalCode::AnswerTooLarge,
        },
    ] {
        let value = plan_measured(shared::PlanMeasureEvent::Flags {
            after: source_id(),
            request_id: (u64::MAX - 8).into(),
            observation,
        });
        assert_eq!(
            serde_json::to_vec(&plan_measurement(&value)).unwrap(),
            serde_json::to_vec(&value).unwrap(),
        );
    }
}
