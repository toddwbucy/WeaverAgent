use super::*;

fn raw(value: &str) -> Box<RawValue> {
    RawValue::from_string(value.into()).unwrap()
}
fn source() -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: 9.into(),
        block: 0.into(),
    }
}
fn span() -> SpanRef {
    SpanRef {
        seed_id: "seed".into(),
        local_id: 3.into(),
        revision: 1.into(),
    }
}
fn plan() -> PlanRef {
    PlanRef {
        plan_id: PlanId {
            seed_id: "seed".into(),
            local_id: 1.into(),
        },
        revision: 2.into(),
    }
}
fn step() -> StepId {
    StepId {
        seed_id: "seed".into(),
        local_id: 4.into(),
    }
}
fn statement() -> StatementRef {
    StatementRef {
        block_id: source(),
        binding: 0.into(),
    }
}
fn setup() -> ReviewSetupRef {
    ReviewSetupRef {
        setup_id: "tests".into(),
        sha256: "d".repeat(64),
    }
}
fn work() -> WorkRef {
    WorkRef {
        plan: plan(),
        step_id: step(),
        span: span(),
        through: SourcePosition {
            block_id: source(),
            turn: Some("turn".into()),
        },
        content_sha256: "c".repeat(64),
    }
}
fn message() -> Box<RawValue> {
    raw(r#"{"role":"user","content":[{"type":"text","text":"Read file."}]}"#)
}
fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn classifier_result_keeps_every_compound_binding_and_decimal_position() {
    let record = Ste100ResultRecord {
        request_id: u64::MAX.into(),
        target: Ste100Target::Source(source()),
        scheme_id: "scheme".into(),
        result: Ste100Result::Accepted {
            bindings: vec![Binding {
                statement_span: Span {
                    start: 0.into(),
                    end: 30.into(),
                },
                verb: "read".into(),
                verb_span: Span {
                    start: 0.into(),
                    end: 4.into(),
                },
                condition: Some("ready".into()),
                condition_span: Some(Span {
                    start: 24.into(),
                    end: 29.into(),
                }),
                nouns: vec![
                    NounBinding {
                        value: "file".into(),
                        span: Span {
                            start: 5.into(),
                            end: 9.into(),
                        },
                    },
                    NounBinding {
                        value: "log".into(),
                        span: Span {
                            start: 14.into(),
                            end: 17.into(),
                        },
                    },
                ],
            }],
        },
    };
    assert_eq!(
        json(&record),
        r#"{"request_id":"18446744073709551615","target":{"source":{"run":"run","sequence":"9","block":"0"}},"scheme_id":"scheme","result":{"accepted":{"bindings":[{"statement_span":{"start":"0","end":"30"},"verb":"read","verb_span":{"start":"0","end":"4"},"condition":"ready","condition_span":{"start":"24","end":"29"},"nouns":[{"value":"file","span":{"start":"5","end":"9"}},{"value":"log","span":{"start":"14","end":"17"}}]}]}}}"#
    );
    assert_eq!(
        json(&Ste100Target::Input {
            input_id: 1.into(),
            block: 0.into()
        }),
        r#"{"input":{"input_id":"1","block":"0"}}"#
    );
    assert_eq!(
        json(&Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "defect".into(),
            spans: vec![]
        }),
        r#"{"not_ste100":{"code":"non_ste100","detail":"defect","spans":[]}}"#
    );
    assert_eq!(
        json(&Ste100Result::ComponentFailed {
            code: Ste100ComponentCode::NoResult,
            detail: "missing".into()
        }),
        r#"{"component_failed":{"code":"no_result","detail":"missing"}}"#
    );
}

#[test]
fn template_inventory_records_the_symbolic_parts_without_claiming_a_pass() {
    let record = TemplateInventory {
        scheme_id: "scheme".into(),
        inventory_sha256: "inventory".into(),
        checker_artifact_sha256: "checker".into(),
        templates: vec![TemplateSpec {
            template_id: "step".into(),
            kind: "loop.instruction".into(),
            parts: vec![
                TemplatePart::Literal("Read ".into()),
                TemplatePart::Slot {
                    name: "step".into(),
                    class: SlotClass::StepId,
                },
                TemplatePart::Literal(".".into()),
            ],
        }],
    };
    assert_eq!(
        json(&record),
        r#"{"scheme_id":"scheme","inventory_sha256":"inventory","checker_artifact_sha256":"checker","templates":[{"template_id":"step","kind":"loop.instruction","parts":[{"literal":"Read "},{"slot":{"name":"step","class":"step_id"}},{"literal":"."}]}]}"#
    );
    assert_eq!(
        json(&Ste100Target::Template {
            template_id: "step".into()
        }),
        r#"{"template":{"template_id":"step"}}"#
    );
}

#[test]
fn loop_metadata_is_closed_and_independent_of_the_spliced_message() {
    let record = LoopInstructionRecord {
        template_id: "wind_down".into(),
        message: message(),
        instruction: LoopInstructionMeta::WindDown {},
    };
    assert_eq!(
        json(&record),
        r#"{"template_id":"wind_down","message":{"role":"user","content":[{"type":"text","text":"Read file."}]},"instruction":{"wind_down":{}}}"#
    );
    let variants = [
        (
            LoopInstructionMeta::Restatement {
                source: source(),
                attempt: 1.into(),
            },
            "restatement",
        ),
        (
            LoopInstructionMeta::Retry {
                retry_of: CallId {
                    run: "run".into(),
                    sequence: 1.into(),
                    call: 0.into(),
                },
                attempt: 2.into(),
            },
            "retry",
        ),
        (
            LoopInstructionMeta::PlanProposal {
                plan: plan(),
                authorization: source(),
            },
            "plan_proposal",
        ),
        (
            LoopInstructionMeta::Step {
                plan: plan(),
                step_id: step(),
                instruction: statement(),
                span: span(),
            },
            "step",
        ),
        (
            LoopInstructionMeta::Review {
                setup: setup(),
                work: work(),
            },
            "review",
        ),
        (
            LoopInstructionMeta::Compact {
                reservation: span(),
                through: SourcePosition {
                    block_id: source(),
                    turn: None,
                },
            },
            "compact",
        ),
    ];
    for (value, tag) in variants {
        assert!(json(&value).starts_with(&format!("{{\"{tag}\":")));
    }
}

#[test]
fn operator_context_and_failed_outcome_preserve_nulls_and_no_authority_tag() {
    let input = PlanInputRecord {
        message: message(),
        context: PlanInputContext {
            document_sha256: "document".into(),
            cause: crate::event::Cause { uid: 0 },
            expected: None,
            operation: PlanInputOperation::Authorize {
                task: 0.into(),
                categories: vec![],
                groups: vec![],
                review: setup(),
            },
        },
        document: "Read file.\n".into(),
    };
    let value: serde_json::Value = serde_json::from_str(&json(&input)).unwrap();
    assert_eq!(
        value["context"].get("expected"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(value["context"]["cause"]["uid"], 0);
    assert_eq!(value["context"]["operation"]["authorize"]["task"], "0");
    assert_eq!(value["document"], "Read file.\n");
    assert!(value.get("type").is_none());
    let outcome = StepOutcomeRecord {
        message: message(),
        template_id: "step.failed".into(),
        context: None,
        plan: plan(),
        step_id: step(),
        review_call: None,
        work: work(),
        outcome: ReviewOutcome::Failed {
            case: ReviewFailure::AssessmentFailed,
        },
    };
    let value: serde_json::Value = serde_json::from_str(&json(&outcome)).unwrap();
    assert_eq!(value.get("context"), Some(&serde_json::Value::Null));
    assert_eq!(value.get("review_call"), Some(&serde_json::Value::Null));
    assert_eq!(
        value["outcome"],
        serde_json::json!({"failed":{"case":"assessment_failed"}})
    );
}

#[test]
fn pair_terminal_keeps_existing_numeric_exchange_and_tool_outcome() {
    let ended = PairEnded {
        call_id: CallId {
            run: "run".into(),
            sequence: u64::MAX.into(),
            call: 2.into(),
        },
        execution: Some(ExchangeId {
            opener: Opener::Harness,
            ordinal: 7,
        }),
        fate: TerminalPairFate::Failed,
        reason: Some(PairFailure::Timeout {}),
        outcome: Some(ToolOutcome::Killed {
            partial: Some("whole\nreturn".into()),
            by: KillCause::Clock,
        }),
        loaded_from: None,
    };
    assert_eq!(
        json(&ended),
        r#"{"call_id":{"run":"run","sequence":"18446744073709551615","call":"2"},"execution":{"opener":"harness","ordinal":7},"fate":"failed","reason":{"timeout":{}},"outcome":{"outcome":"killed","partial":"whole\nreturn","by":"clock"},"loaded_from":null}"#
    );
    let rejected = RejectedReturn {
        call_id: None,
        execution: ExchangeId {
            opener: Opener::Harness,
            ordinal: 7,
        },
        reason: "duplicate_return".into(),
        raw_envelope: raw(r#"{ "opaque": "\u0061" }"#),
    };
    assert!(json(&rejected).ends_with(r#""raw_envelope":{ "opaque": "\u0061" }}"#));
}

#[test]
fn sort_records_preserve_refs_drop_reasons_and_unavailable_counts() {
    let record = SortDecisionRecord {
        template_id: "sort.serve".into(),
        message: message(),
        decision: SortDecision::Serve {
            assembly_id: 7.into(),
            criterion: SortCriterion {
                order: CandidateOrder::Recency {},
                injection_share: Rational {
                    numerator: 1,
                    denominator: 4,
                },
                external_drop: ExternalDrop::Refuse,
            },
            selected_refs: vec![UnitId::Statement {
                statement: statement(),
            }],
            dropped_refs: vec![SelectionDrop {
                unit: UnitId::Span { span: span() },
                reason: DropReason::InjectionBudget,
            }],
            reads: vec![SelectionRead {
                selector: ReadSelector::History {
                    noun: "file".into(),
                    verb: None,
                    condition: None,
                },
                snapshot_id: 3.into(),
            }],
            refusal: None,
        },
    };
    let value: serde_json::Value = serde_json::from_str(&json(&record)).unwrap();
    assert_eq!(
        value["decision"]["serve"]["criterion"]["injection_share"],
        serde_json::json!({"numerator":1,"denominator":4})
    );
    assert_eq!(
        value["decision"]["serve"]["dropped_refs"][0]["reason"],
        "injection_budget"
    );
    assert!(value["decision"]["serve"]["reads"][0]["selector"]["history"]["verb"].is_null());
    assert_eq!(
        json(&PlanFlagObservation::Unavailable {
            code: ReadRefusalCode::ResultTooLarge
        }),
        r#"{"unavailable":{"code":"result_too_large"}}"#
    );
    let measurement = SortMeasurement {
        assembly_id: 7.into(),
        candidate_count: 2.into(),
        selected_count: 1.into(),
        mandatory_tokens: None,
        optional_marginal_tokens: Some(23),
        budget: 50,
        fit_attempts: 1.into(),
        wait_ns: 30.into(),
        refusal: None,
    };
    assert_eq!(
        json(&measurement),
        r#"{"assembly_id":"7","candidate_count":"2","selected_count":"1","mandatory_tokens":null,"optional_marginal_tokens":23,"budget":50,"fit_attempts":"1","wait_ns":"30","refusal":null}"#
    );
}

#[test]
fn placement_keeps_token_numbers_separate_from_decimal_message_and_mark_ids() {
    let placement = PlacementRecord {
        assembly_id: 0.into(),
        phase: PlacementPhase::Boot,
        context: None,
        cut: None,
        method: None,
        n: 1024,
        prefix_tokens: 1024,
        resident_before: 1024,
        input_tokens: 0,
        capacity: 8192,
        generation_limit: 512,
        terminal_tokens: 1,
        unit_map: vec![],
        message_ids: vec![],
        assistant_message_id: None,
        recomputed_messages: 0.into(),
        wait_ns: u64::MAX.into(),
    };
    assert_eq!(
        json(&placement),
        r#"{"assembly_id":"0","phase":"boot","context":null,"cut":null,"method":null,"N":1024,"prefix_tokens":1024,"resident_before":1024,"input_tokens":0,"capacity":8192,"generation_limit":512,"terminal_tokens":1,"unit_map":[],"message_ids":[],"assistant_message_id":null,"recomputed_messages":"0","wait_ns":"18446744073709551615"}"#
    );
    let mark = MarkPosition {
        id: MarkOccurrenceId {
            run: "run".into(),
            ordinal: 0.into(),
        },
        span: span(),
        end: ArgumentCursor::Message {
            message_id: 8.into(),
            block_end: 1.into(),
            byte_end: Some(5.into()),
        },
        source: MarkPositionSource::Native { event: source() },
    };
    assert_eq!(
        json(&mark),
        r#"{"id":{"run":"run","ordinal":"0"},"span":{"seed_id":"seed","local_id":"3","revision":"1"},"end":{"message":{"message_id":"8","block_end":"1","byte_end":"5"}},"source":{"native":{"event":{"run":"run","sequence":"9","block":"0"}}}}"#
    );
}

fn observation() -> StateOperationObservation {
    StateOperationObservation {
        operation_id: 1.into(),
        parent_id: None,
        request_id: 2.into(),
        operation: StateOperation::File,
        began_ns: 10.into(),
        ended_ns: 20.into(),
        phases: vec![OperationPhase {
            name: PhaseName::Transaction,
            began_ns: 11.into(),
            ended_ns: 19.into(),
        }],
        resource: OperationResource {
            cpu_ns: None,
            copied_bytes: Some(30.into()),
            held_bytes: Some(40.into()),
            unavailable: vec![ResourceUnavailable {
                field: ResourceField::CpuNs,
                reason: ObservationUnavailable::Unsupported,
            }],
        },
        disk: OperationDisk {
            enabled: None,
            before: None,
            after: None,
            unavailable_reason: Some(ObservationUnavailable::NotSampled),
        },
    }
}
#[test]
fn store_measurement_keeps_raw_clock_readings_and_unavailability() {
    let record = StateStoreMeasurement {
        observation: observation(),
        request_source: Some(source()),
        harness_queued_ns: 7.into(),
        harness_sent_ns: 9.into(),
        harness_received_ns: 22.into(),
    };
    assert_eq!(
        json(&record),
        r#"{"observation":{"operation_id":"1","parent_id":null,"request_id":"2","operation":"file","began_ns":"10","ended_ns":"20","phases":[{"name":"transaction","began_ns":"11","ended_ns":"19"}],"resource":{"cpu_ns":null,"copied_bytes":"30","held_bytes":"40","unavailable":[{"field":"cpu_ns","reason":"unsupported"}]},"disk":{"enabled":null,"before":null,"after":null,"unavailable_reason":"not_sampled"}},"request_source":{"run":"run","sequence":"9","block":"0"},"harness_queued_ns":"7","harness_sent_ns":"9","harness_received_ns":"22"}"#
    );
}

#[test]
fn save_phases_and_halt_extensions_preserve_their_distinct_evidence() {
    let window = SaveWindow {
        began_ns: 10.into(),
        deadline_ns: 20.into(),
    };
    let requested = SavePointRecord::Requested {
        capture_id: 1.into(),
        window,
        tail_window: None,
        cause: PersistenceCause::Requested,
    };
    assert_eq!(
        json(&requested),
        r#"{"phase":"requested","capture_id":"1","window":{"began_ns":"10","deadline_ns":"20"},"tail_window":null,"cause":"requested"}"#
    );
    let refused = SavePointRecord::Refused {
        capture_id: 1.into(),
        window,
        tail_window: None,
        cause: PersistenceCause::Requested,
        refusal_code: "snapshot_timeout".into(),
    };
    assert!(!json(&refused).contains("dump_sha256"));
    let unload = UnloadClose {
        grant_surface: None,
        cause: None,
        forced: true,
        forced_by: None,
        release: None,
        end_reason: Some(EndReason::ComponentFailure),
        state_flow_failure: Some(StateFlowFailure {
            fault_sequence: None,
            account: raw(r#"{"first":"same","secondary":[]}"#),
        }),
    };
    assert_eq!(
        json(&unload),
        r#"{"forced":true,"end_reason":"component_failure","state_flow_failure":{"fault_sequence":null,"account":{"first":"same","secondary":[]}}}"#
    );
}

pub(crate) fn load_record() -> LoadRecord {
    let wait = MeasuredWait {
        elapsed_ns: 1_000_000.into(),
        measurement_ref: "fixture".into(),
    };
    LoadRecord {
        residual_readout: false,
        field: None,
        surprisal: false,
        state_member: true,
        state_store: crate::event::StoreIdentity {
            engine: "serving_sqlite".into(),
        },
        declaration: "declaration".into(),
        composer: crate::event::LoopIdentity::compiled("worker"),
        lineage: None,
        reset: None,
        stack: std::collections::BTreeMap::from([("worker".into(), "worker-hash".into())]),
        boundary: "boundary".into(),
        cause: crate::event::Cause { uid: 0 },
        library_path: None,
        state_flow: FlowDeclarationRecord {
            registry_id: "state-copy/1".into(),
            version: 1,
            partition: vec![],
            gate_wait: wait.clone(),
            cut_wait: wait.clone(),
            observation_interval: MeasuredInterval {
                elapsed_ns: 1000.into(),
                measurement_ref: "fixture".into(),
            },
            backlog: FlowBacklog {
                max_pending_blocks: 100.into(),
                max_pending_bytes: 10000.into(),
                lag_alarm_turns: 2.into(),
                measurement_ref: "fixture".into(),
            },
            scheme: SchemeDeclaration {
                scheme_id: "scheme".into(),
                artifact_sha256: "s".repeat(64),
                verbs: vec!["read".into()],
            },
            persistence: PersistenceConfig {
                save_wait: wait.clone(),
                snapshot_wait: wait.clone(),
            },
            reads: ReadLimits {
                max_result_bytes: 10000.into(),
                measurement_ref: "fixture".into(),
            },
            ste100: Ste100Instruction {
                model_binding: ModelBinding {
                    artifact: "checker".into(),
                    devices: vec![0],
                },
                model_artifact_sha256: "m".repeat(64),
                instances: 1,
                max_wire_bytes: 10000.into(),
                measurement_ref: "fixture".into(),
                repair_cap: RepairCap {
                    attempts: 3,
                    measurement_ref: "fixture".into(),
                },
                decision_wait: wait,
            },
            tools: ToolPolicies { sh: None },
            placement: PlacementConfig {
                c_head: 2048,
                c_tail: 6144,
                measurement_ref: "fixture".into(),
            },
        },
        state_start: StartIdentity {
            seed_id: "seed".into(),
            artifact_id: "seed-artifact".into(),
            dump_sha256: "a".repeat(64),
            config_sha256: "b".repeat(64),
            parent_artifact_id: None,
            position: None,
        },
        clock_source: ClockSource::ClockMonotonic,
        origin_ns: 9_007_199_254_740_993.into(),
        state_member_observation: MemberObservationIdentity {
            instrument_version: "state-observation/2".into(),
            pid: 123,
            start_ticks: Some(100.into()),
            ticks_per_second: Some(100.into()),
            unavailable_reason: None,
        },
        template_inventory: TemplateInventory {
            scheme_id: "scheme".into(),
            inventory_sha256: "i".repeat(64),
            checker_artifact_sha256: "c".repeat(64),
            templates: vec![TemplateSpec {
                template_id: "wind_down".into(),
                kind: "loop.instruction".into(),
                parts: vec![TemplatePart::Literal("Stop work.".into())],
            }],
        },
        ste100_scheme: "{\n  \"edition\": \"Issue 9\"\n}\n".into(),
        ste100_binary_sha256: "e".repeat(64),
    }
}

fn flow_record() -> FlowObservation {
    FlowObservation {
        phase: FlowPhase::Sampled,
        source_id: None,
        input_id: None,
        request_id: None,
        pending_count: 1.into(),
        oldest_age_ns: 2.into(),
        elapsed_ns: None,
        blocked_stage: None,
        sample_start_ns: 3.into(),
        sampled_ns: 4.into(),
        pending_bytes: 5.into(),
        filed_count: 6.into(),
        filed_bytes: 7.into(),
        rejected_count: 8.into(),
        input_pending: 9.into(),
        input_started: 10.into(),
        input_completed: 11.into(),
        retained_blocks: 12.into(),
        retained_bytes: 13.into(),
        pending_turns: 14.into(),
        backlog_waiting: true,
        lag_alarm: true,
        waiting_copy: Some(CopyAtHalt {
            source_event: CopySourceEvent {
                run: "run".into(),
                sequence: 15.into(),
            },
            partition_rule_id: "text".into(),
            content_type: CopyContentType::Statement,
            sources: vec![source()],
        }),
        failed_corrections: None,
        retry_cap: None,
    }
}

#[test]
fn load_records_exact_scheme_text_current_member_and_complete_declaration() {
    let load = load_record();
    let bytes = json(&load);
    assert!(!bytes.contains('\n'));
    let value: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(value["ste100_scheme"], load.ste100_scheme);
    assert_eq!(value["clock_source"], "CLOCK_MONOTONIC");
    assert_eq!(value["origin_ns"], "9007199254740993");
    assert_eq!(value["state_member_observation"]["pid"], 123);
    assert_eq!(value["state_member_observation"]["start_ticks"], "100");
    assert_eq!(
        value["state_flow"]["ste100"]["model_binding"]["devices"],
        serde_json::json!([0])
    );
    assert_eq!(value["state_flow"]["placement"]["c_tail"], 6144);
    assert!(value.get("tee").is_none());
    assert!(value.get("certificate").is_none());
    assert!(value["state_flow"]["tools"].get("sh").is_none());
    assert_eq!(
        value["state_start"].get("position"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(
        value["state_start"].get("parent_artifact_id"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(value["stack"]["worker"], "worker-hash");
}

#[test]
fn flow_gauges_intervals_and_whole_waiting_copy_have_one_encoding() {
    assert_eq!(
        json(&flow_record()),
        r#"{"phase":"sampled","source_id":null,"input_id":null,"request_id":null,"pending_count":"1","oldest_age_ns":"2","elapsed_ns":null,"blocked_stage":null,"sample_start_ns":"3","sampled_ns":"4","pending_bytes":"5","filed_count":"6","filed_bytes":"7","rejected_count":"8","input_pending":"9","input_started":"10","input_completed":"11","retained_blocks":"12","retained_bytes":"13","pending_turns":"14","backlog_waiting":true,"lag_alarm":true,"waiting_copy":{"source_event":{"run":"run","sequence":"15"},"partition_rule_id":"text","content_type":"statement","sources":[{"run":"run","sequence":"9","block":"0"}]}}"#
    );
    let mut repaired = flow_record();
    repaired.phase = FlowPhase::RepairExhausted;
    repaired.source_id = Some(source());
    repaired.failed_corrections = Some(3.into());
    repaired.retry_cap = Some(3.into());
    assert!(json(&repaired).ends_with(r#","failed_corrections":"3","retry_cap":"3"}"#));
}

/// Shared literal-shaped examples for the recorder's exhaustive envelope matrix.
pub(crate) fn payload_fixtures() -> Vec<(
    super::super::Kind,
    super::super::Channel,
    super::super::Origin,
    bool,
    super::super::Payload,
)> {
    use super::super::{Channel as C, Kind as K, Origin as O, Payload as P};
    let call = CallId {
        run: "run".into(),
        sequence: 9.into(),
        call: 0.into(),
    };
    let task = TaskScopeRef {
        task: span(),
        prompt: SpanRef {
            local_id: 4.into(),
            ..span()
        },
    };
    let window = SaveWindow {
        began_ns: 10.into(),
        deadline_ns: 20.into(),
    };
    vec![
        (
            K::Load,
            C::Internal,
            O::Runtime,
            false,
            P::Elections(Box::new(load_record())),
        ),
        (
            K::Unload,
            C::Internal,
            O::Runtime,
            false,
            P::Unload(Box::new(UnloadClose {
                grant_surface: None,
                cause: None,
                forced: false,
                forced_by: None,
                release: None,
                end_reason: None,
                state_flow_failure: None,
            })),
        ),
        (
            K::ToolCallStarted,
            C::Tool,
            O::Model,
            true,
            P::PairStarted(Box::new(PairStarted {
                call_id: call.clone(),
                call_block: 1.into(),
                statement: statement(),
                lane: PairLane::Recall,
                request: PairRequest::Recall {
                    subject: "file".into(),
                },
                retry_of: None,
            })),
        ),
        (
            K::ToolCallCompleted,
            C::Tool,
            O::Tool,
            true,
            P::PairEnded(Box::new(PairEnded {
                call_id: call.clone(),
                execution: Some(ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 1,
                }),
                fate: TerminalPairFate::Completed,
                reason: None,
                outcome: Some(ToolOutcome::Result {
                    content: "whole return".into(),
                }),
                loaded_from: None,
            })),
        ),
        (
            K::ToolReturnRejected,
            C::Tool,
            O::Tool,
            true,
            P::RejectedReturn(Box::new(RejectedReturn {
                call_id: Some(call),
                execution: ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 1,
                },
                reason: "duplicate_return".into(),
                raw_envelope: raw(r#"{"original":"whole"}"#),
            })),
        ),
        (
            K::LoopInstruction,
            C::Internal,
            O::Loop,
            true,
            P::LoopInstruction(Box::new(LoopInstructionRecord {
                template_id: "restatement".into(),
                message: message(),
                instruction: LoopInstructionMeta::Restatement {
                    source: source(),
                    attempt: 1.into(),
                },
            })),
        ),
        (
            K::SortDecision,
            C::Internal,
            O::Loop,
            true,
            P::SortDecision(Box::new(SortDecisionRecord {
                template_id: "sort.outbound".into(),
                message: message(),
                decision: SortDecision::Outbound {
                    source: SourcePosition {
                        block_id: source(),
                        turn: Some("turn".into()),
                    },
                    criterion: OutboundCriterion::Identity,
                    acts: vec![SortAct::Answer { source: source() }],
                },
            })),
        ),
        (
            K::StateSortMeasurement,
            C::Internal,
            O::Loop,
            true,
            P::SortMeasurement(Box::new(SortMeasurement {
                assembly_id: 1.into(),
                candidate_count: 0.into(),
                selected_count: 0.into(),
                mandatory_tokens: Some(0),
                optional_marginal_tokens: Some(0),
                budget: 1024,
                fit_attempts: 1.into(),
                wait_ns: 2.into(),
                refusal: None,
            })),
        ),
        (
            K::StepOutcome,
            C::Internal,
            O::Loop,
            true,
            P::StepOutcome(Box::new(StepOutcomeRecord {
                message: message(),
                template_id: "step.failed".into(),
                context: None,
                plan: plan(),
                step_id: step(),
                review_call: None,
                work: work(),
                outcome: ReviewOutcome::Failed {
                    case: ReviewFailure::AssessmentFailed,
                },
            })),
        ),
        (
            K::PlanEvent,
            C::Input,
            O::Operator,
            false,
            P::PlanInput(Box::new(PlanInputRecord {
                message: message(),
                context: PlanInputContext {
                    document_sha256: "d".repeat(64),
                    cause: crate::event::Cause { uid: 0 },
                    expected: None,
                    operation: PlanInputOperation::Authorize {
                        task: 0.into(),
                        categories: vec![],
                        groups: vec![],
                        review: setup(),
                    },
                },
                document: "Read file.".into(),
            })),
        ),
        (
            K::PlanEvent,
            C::Internal,
            O::Loop,
            false,
            P::PlanEffect(Box::new(PlanEffectRecord {
                message: message(),
                context: PlanEffect::Authorize {
                    plan: plan(),
                    source: source(),
                    task: task.clone(),
                    review: setup(),
                },
                template_id: "plan.authorize".into(),
            })),
        ),
        (
            K::PlanBreak,
            C::Internal,
            O::Loop,
            true,
            P::PlanBreak(Box::new(PlanBreakReport {
                plan: Some(plan()),
                step_id: Some(step()),
                proposed_source: Some(source()),
                case: PlanBreakCase::WorkChanged,
            })),
        ),
        (
            K::StatePlanMeasurement,
            C::Internal,
            O::Loop,
            false,
            P::PlanMeasurement(Box::new(PlanMeasurement {
                instrument_version: "plan-observation/1".into(),
                plan: plan(),
                event: PlanMeasureEvent::Flags {
                    after: source(),
                    request_id: 1.into(),
                    observation: PlanFlagObservation::Unavailable {
                        code: ReadRefusalCode::ResultUnavailable,
                    },
                },
            })),
        ),
        (
            K::StateMark,
            C::Internal,
            O::Loop,
            true,
            P::StateMark(Box::new(StateMarkRecord {
                template_id: "task".into(),
                message: message(),
                context: MarkEffect::BeginTask {
                    task: task.task,
                    prompt: task.prompt,
                    source: ContentRef::Event { block_id: source() },
                },
            })),
        ),
        (
            K::StateMarkPosition,
            C::Internal,
            O::Loop,
            false,
            P::MarkPosition(Box::new(MarkPosition {
                id: MarkOccurrenceId {
                    run: "run".into(),
                    ordinal: 0.into(),
                },
                span: span(),
                end: ArgumentCursor::Head {},
                source: MarkPositionSource::Boot {},
            })),
        ),
        (
            K::StatePlacement,
            C::Internal,
            O::Loop,
            false,
            P::Placement(Box::new(PlacementRecord {
                assembly_id: 0.into(),
                phase: PlacementPhase::Boot,
                context: None,
                cut: None,
                method: None,
                n: 10,
                prefix_tokens: 10,
                resident_before: 10,
                input_tokens: 0,
                capacity: 8192,
                generation_limit: 512,
                terminal_tokens: 1,
                unit_map: vec![],
                message_ids: vec![],
                assistant_message_id: None,
                recomputed_messages: 0.into(),
                wait_ns: 20.into(),
            })),
        ),
        (
            K::InputOffered,
            C::Input,
            O::External,
            false,
            P::InputOffered(Box::new(InputOfferedRecord {
                input_id: 1.into(),
                octets: "eyJ0ZXh0IjoiUmVhZCBmaWxlLiJ9Cg==".into(),
            })),
        ),
        (
            K::StateFlow,
            C::Measurement,
            O::Runtime,
            true,
            P::FlowObservation(Box::new(flow_record())),
        ),
        (
            K::StateStoreMeasurement,
            C::Measurement,
            O::Runtime,
            true,
            P::StateStoreMeasurement(Box::new(StateStoreMeasurement {
                observation: observation(),
                request_source: Some(source()),
                harness_queued_ns: 7.into(),
                harness_sent_ns: 9.into(),
                harness_received_ns: 22.into(),
            })),
        ),
        (
            K::Ste100Result,
            C::Internal,
            O::Runtime,
            false,
            P::Ste100Result(Box::new(Ste100ResultRecord {
                request_id: 1.into(),
                target: Ste100Target::Template {
                    template_id: "wind_down".into(),
                },
                scheme_id: "scheme".into(),
                result: Ste100Result::Accepted { bindings: vec![] },
            })),
        ),
        (
            K::SavePoint,
            C::Internal,
            O::Runtime,
            false,
            P::SavePointRecord(Box::new(SavePointRecord::Requested {
                capture_id: 1.into(),
                window,
                tail_window: None,
                cause: PersistenceCause::Requested,
            })),
        ),
    ]
}

#[test]
fn unit_map_keeps_each_occurrence_source_interval_and_step_query() {
    let map = UnitMapEntry {
        unit_id: UnitId::Statement {
            statement: statement(),
        },
        occurrence_id: UnitOccurrenceId {
            run: "run".into(),
            assembly_id: 7.into(),
            unit_index: 2.into(),
        },
        message_start: 1.into(),
        message_end: 2.into(),
        spans: vec![span()],
        source_map: vec![PlacedSource {
            material: MaterialRef::Text {
                source: ContentRef::Event { block_id: source() },
                span: Span {
                    start: 4.into(),
                    end: 9.into(),
                },
            },
            message_id: 99.into(),
            block: 0.into(),
            start: Some(0.into()),
            end: Some(5.into()),
        }],
        step_queries: vec![PlacedStepQuery {
            reference: StepQueryRef {
                plan: plan(),
                previous_step: None,
                step_id: step(),
                instruction: statement(),
            },
            message_id: 99.into(),
        }],
    };
    assert_eq!(
        json(&map),
        r#"{"unit_id":{"statement":{"statement":{"block_id":{"run":"run","sequence":"9","block":"0"},"binding":"0"}}},"occurrence_id":{"run":"run","assembly_id":"7","unit_index":"2"},"message_start":"1","message_end":"2","spans":[{"seed_id":"seed","local_id":"3","revision":"1"}],"source_map":[{"material":{"text":{"source":{"event":{"block_id":{"run":"run","sequence":"9","block":"0"}}},"span":{"start":"4","end":"9"}}},"message_id":"99","block":"0","start":"0","end":"5"}],"step_queries":[{"reference":{"plan":{"plan_id":{"seed_id":"seed","local_id":"1"},"revision":"2"},"previous_step":null,"step_id":{"seed_id":"seed","local_id":"4"},"instruction":{"block_id":{"run":"run","sequence":"9","block":"0"},"binding":"0"}},"message_id":"99"}]}"#
    );
}

#[test]
fn confirmed_and_published_saves_keep_hashes_position_and_both_absolute_windows() {
    let window = SaveWindow {
        began_ns: 10.into(),
        deadline_ns: 20.into(),
    };
    let tail = Some(SaveWindow {
        began_ns: 5.into(),
        deadline_ns: 25.into(),
    });
    let confirmed = SavePointRecord::Confirmed {
        capture_id: 1.into(),
        window,
        tail_window: tail,
        cause: PersistenceCause::GracefulUnload,
        artifact_id: "artifact".into(),
        dump_sha256: "dump".into(),
        config_sha256: "config".into(),
        location: "territory/artifact".into(),
        position: None,
    };
    let published = SavePointRecord::Published {
        capture_id: 1.into(),
        window,
        tail_window: tail,
        cause: PersistenceCause::GracefulUnload,
        artifact_id: "artifact".into(),
        dump_sha256: "dump".into(),
        config_sha256: "config".into(),
        location: "territory/artifact".into(),
        position: None,
    };
    assert_eq!(
        json(&confirmed),
        r#"{"phase":"confirmed","capture_id":"1","window":{"began_ns":"10","deadline_ns":"20"},"tail_window":{"began_ns":"5","deadline_ns":"25"},"cause":"graceful_unload","artifact_id":"artifact","dump_sha256":"dump","config_sha256":"config","location":"territory/artifact","position":null}"#
    );
    assert_eq!(
        json(&published),
        r#"{"phase":"published","capture_id":"1","window":{"began_ns":"10","deadline_ns":"20"},"tail_window":{"began_ns":"5","deadline_ns":"25"},"cause":"graceful_unload","artifact_id":"artifact","dump_sha256":"dump","config_sha256":"config","location":"territory/artifact","position":null}"#
    );
}
