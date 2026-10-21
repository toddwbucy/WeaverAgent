use super::*;
use std::sync::Arc;
use weaver_trace::Sequence;
use weaver_trace::serving::{ContextBlock, PairCopy, SourceBlock, StatementCopy};
use weaver_types::state_flow::*;

fn ledger() -> Ledger {
    Ledger::fixture("run".into(), "session".into())
}

fn id(sequence: u64, block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: block.into(),
    }
}

fn source(sequence: u64, kind: &str, channel: &str, origin: &str) -> Arc<str> {
    serde_json::json!({
        "session":"session", "run":"run", "turn":"turn", "sequence":sequence.to_string(),
        "kind":kind, "channel":channel, "origin":origin, "subsystem":"harness",
        "wall_ms":1, "monotonic_ns":"7", "causal_parent":"1"
    })
    .to_string()
    .into()
}

fn statement(sequence: u64, ordinals: &[u64]) -> AdmittedCopy {
    AdmittedCopy::Statement(StatementCopy {
        source_envelope: source(sequence, "message.assistant", "output", "model"),
        partition_rule_id: "statement".into(),
        blocks: ordinals
            .iter()
            .map(|index| SelectedBlock {
                source: SourceBlock {
                    run: "run".into(),
                    sequence: Sequence(sequence),
                    block: *index,
                },
                source_path: vec![PathPart::Key("content".into()), PathPart::Index(*index)],
                raw: Arc::from("{ \"type\" : \"text\", \"text\" : \"Read caf\\u00e9.\" }"),
            })
            .collect(),
        context: None,
    })
}

fn mechanical(kind: &str, context: &str) -> AdmittedCopy {
    let AdmittedCopy::Statement(mut copy) = statement(3, &[0]) else {
        unreachable!()
    };
    copy.source_envelope = source(3, kind, "internal", "loop");
    copy.blocks[0]
        .source_path
        .insert(0, PathPart::Key("message".into()));
    copy.context = Some(ContextBlock {
        source_path: vec![PathPart::Key("context".into())],
        raw: context.into(),
    });
    AdmittedCopy::Statement(copy)
}

fn binding() -> Binding {
    Binding {
        statement_span: Span {
            start: 0.into(),
            end: 11.into(),
        },
        verb: "read".into(),
        verb_span: Span {
            start: 0.into(),
            end: 4.into(),
        },
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: "caf\u{e9}".into(),
            span: Span {
                start: 5.into(),
                end: 10.into(),
            },
        }],
    }
}

fn check(
    ledger: &mut Ledger,
    source: SourceBlockId,
    request_id: u64,
    accepted: bool,
) -> Classified {
    let request = Ste100Request {
        request_id: request_id.into(),
        target: Ste100Target::Source(source.clone()),
        text: "Read caf\u{e9}.".into(),
        scheme_id: "fixture-read/1".into(),
    };
    ledger.classification_submitted(&source, &request).unwrap();
    ledger
        .classify_recorded(&Ste100Reply {
            request_id: request.request_id,
            target: request.target,
            scheme_id: request.scheme_id,
            result: if accepted {
                Ste100Result::Accepted {
                    bindings: vec![binding()],
                }
            } else {
                Ste100Result::NotSte100 {
                    code: Ste100DefectCode::NonSte100,
                    detail: "fixture defect".into(),
                    spans: vec![],
                }
            },
        })
        .unwrap()
}

fn rejected_proof(source: SourceBlockId) -> RecordedTerminal {
    RecordedTerminal::for_test_phase(
        source,
        weaver_trace::serving::payloads::FlowPhase::CorrectlyRejected,
    )
}

fn budget() -> RetentionBudget {
    RetentionBudget::new(FlowBacklog {
        max_pending_blocks: 20.into(),
        max_pending_bytes: 50_000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "fixture".into(),
    })
    .unwrap()
}

fn admit(ledger: &mut Ledger, budget: &mut RetentionBudget, copy: AdmittedCopy) -> InstallReport {
    let mut inbox = super::super::intake::FlowInbox::new();
    assert!(matches!(
        inbox.accept(copy, budget),
        super::super::intake::IntakeOutcome::Accepted
    ));
    ledger
        .install(inbox.pop_front(budget).unwrap(), 10, None)
        .unwrap()
}

fn pair_open(sequence: u64) -> AdmittedCopy {
    let pair = PairStarted {
        call_id: CallId {
            run: "run".into(),
            sequence: 2.into(),
            call: 0.into(),
        },
        call_block: 1.into(),
        statement: StatementRef {
            block_id: id(2, 0),
            binding: 0.into(),
        },
        lane: PairLane::External,
        request: PairRequest::External {
            name: "inspect".into(),
            arguments: "{ \"path\" : \"caf\u{e9}\" }".into(),
        },
        retry_of: None,
    };
    AdmittedCopy::Pair(PairCopy {
        source_envelope: source(sequence, "tool.call.started", "tool", "model"),
        partition_rule_id: "pair".into(),
        block: SelectedBlock {
            source: SourceBlock {
                run: "run".into(),
                sequence: Sequence(sequence),
                block: 0,
            },
            source_path: vec![],
            raw: serde_json::to_string_pretty(&pair).unwrap().into(),
        },
    })
}

fn common() -> Common {
    Common {
        session_id: "session".into(),
        run_id: "run".into(),
        request_id: 2.into(),
    }
}

fn progress(bytes: usize, commit: CommitEvidence) -> ExchangeProgress {
    ExchangeProgress {
        common: common(),
        operation: StateOperation::File,
        source: Some(id(2, 0)),
        bytes_sent: bytes,
        frame_bytes: 100,
        commit,
    }
}

fn observation() -> StateOperationObservation {
    StateOperationObservation {
        operation_id: 2.into(),
        parent_id: None,
        request_id: 2.into(),
        operation: StateOperation::File,
        began_ns: 15.into(),
        ended_ns: 20.into(),
        phases: vec![
            OperationPhase {
                name: PhaseName::Validate,
                began_ns: 15.into(),
                ended_ns: 16.into(),
            },
            OperationPhase {
                name: PhaseName::Encode,
                began_ns: 19.into(),
                ended_ns: 20.into(),
            },
        ],
        resource: OperationResource {
            cpu_ns: None,
            copied_bytes: Some(1.into()),
            held_bytes: Some(2.into()),
            unavailable: vec![ResourceUnavailable {
                field: ResourceField::CpuNs,
                reason: ObservationUnavailable::NotSampled,
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

fn receipt() -> StateFrame {
    StateFrame::Filed {
        common: common(),
        block_id: id(2, 0),
        observation: observation(),
    }
}

#[test]
fn whole_copy_installation_preserves_original_indices_and_decoded_utf8() {
    let mut ledger = ledger();
    let copy = statement(2, &[0, 3]);
    let original = copy.clone();
    let report = ledger.install(copy, 10, None).unwrap();
    assert_eq!(report.sources, vec![id(2, 0), id(2, 3)]);
    assert_eq!(
        ledger.next_classification().unwrap().text,
        "Read caf\u{e9}."
    );
    check(&mut ledger, id(2, 0), 1, true);
    let LedgerFile::Statement(file) = ledger.next_file().unwrap() else {
        panic!()
    };
    let AdmittedCopy::Statement(original) = original else {
        panic!()
    };
    assert_eq!(file.raw_block.get(), original.blocks[0].raw.as_ref());
    assert_eq!(
        file.bindings[0].nouns[0].span.slice(&file.text).unwrap(),
        "caf\u{e9}"
    );
    assert_eq!(
        file.source_path,
        vec![
            SourcePathPart::Key("content".into()),
            SourcePathPart::Index { index: 0.into() }
        ]
    );
    assert_eq!(ledger.next_classification().unwrap().source, id(2, 3));
}

#[test]
fn malformed_later_block_returns_the_whole_copy_without_installing_a_prefix() {
    let mut ledger = ledger();
    let AdmittedCopy::Statement(mut copy) = statement(2, &[0, 3]) else {
        unreachable!()
    };
    copy.blocks[1].raw = "{\"type\":\"tool_use\",\"name\":\"inspect\",\"arguments\":\"{}\"}".into();
    let copy = AdmittedCopy::Statement(copy);
    let error = ledger.install(copy.clone(), 10, None).unwrap_err();
    assert_eq!(error.copy, copy);
    assert_eq!(ledger.facts().count(), 0);
    assert!(ledger.history.is_empty());
    ledger.install(statement(2, &[0, 3]), 10, None).unwrap();
}

#[test]
fn duplicate_keys_paths_identities_and_foreign_sessions_refuse_atomically() {
    for defect in 0..5 {
        let mut ledger = ledger();
        let AdmittedCopy::Statement(mut copy) = statement(2, &[0, 3]) else {
            unreachable!()
        };
        match defect {
            0 => {
                copy.blocks[1].raw =
                    "{\"type\":\"text\",\"text\":\"Read caf\u{e9}.\",\"text\":\"Read caf\u{e9}.\"}"
                        .into()
            }
            1 => copy.blocks[1]
                .source_path
                .push(PathPart::Key("text".into())),
            2 => copy.blocks[1].source.block = 0,
            3 => copy.blocks[1].source.sequence = Sequence(9),
            4 => {
                copy.source_envelope = copy
                    .source_envelope
                    .replace("\"session\":\"session\"", "\"session\":\"other\"")
                    .into()
            }
            _ => unreachable!(),
        }
        assert!(
            ledger
                .install(AdmittedCopy::Statement(copy), 10, None)
                .is_err(),
            "defect {defect}"
        );
        assert_eq!(ledger.facts().count(), 0);
    }
}

#[test]
fn later_acceptance_cannot_overtake_earlier_classification_or_unrecorded_rejection() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0, 3]));
    admit(&mut ledger, &mut budget, statement(3, &[0]));
    check(&mut ledger, id(3, 0), 1, true);
    assert!(ledger.next_file().is_none());
    check(&mut ledger, id(2, 0), 2, false);
    assert!(ledger.next_file().is_none());
    assert!(!ledger.barrier(&[id(2, 0)]).unwrap());
    ledger
        .terminal_recorded(rejected_proof(id(2, 0)), &mut budget)
        .unwrap();
    assert!(ledger.next_file().is_none(), "block three is still owed");
    check(&mut ledger, id(2, 3), 3, false);
    ledger
        .terminal_recorded(rejected_proof(id(2, 3)), &mut budget)
        .unwrap();
    assert_eq!(ledger.next_file().unwrap().source(), &id(3, 0));
    assert!(ledger.barrier(&[id(2, 0), id(2, 3)]).unwrap());
    assert!(
        ledger.barrier(&[id(2, 1)]).is_err(),
        "a sequence watermark cannot invent a selected block"
    );
    assert!(ledger.barrier_through(2));
    assert!(!ledger.barrier_through(3));
}

#[test]
fn zero_block_copies_advance_replay_protection_without_inventing_obligations() {
    let mut ledger = ledger();
    let report = ledger.install(statement(5, &[]), 10, None).unwrap();
    assert_eq!(report.source_sequence, 5);
    assert!(report.sources.is_empty());
    assert_eq!(ledger.facts().count(), 0);
    assert!(ledger.barrier_through(5));
    assert!(ledger.barrier(&[id(5, 0)]).is_err());
    assert!(ledger.install(statement(5, &[]), 10, None).is_err());
    assert!(ledger.install(statement(4, &[0]), 10, None).is_err());
}

#[test]
fn replay_stays_refused_after_terminal_content_and_budget_are_released() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[3]));
    check(&mut ledger, id(2, 3), 1, false);
    ledger
        .terminal_recorded(rejected_proof(id(2, 3)), &mut budget)
        .unwrap();
    assert_eq!(budget.gauges(), (0, 0));
    assert!(ledger.barrier(&[id(2, 3)]).unwrap());
    assert!(ledger.install(statement(2, &[3]), 10, None).is_err());
}

#[test]
fn terminal_proof_must_name_the_exact_held_disposition_before_releasing_charges() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0]));
    check(&mut ledger, id(2, 0), 1, false);
    let charged = budget.gauges();
    assert!(
        ledger
            .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
            .is_err()
    );
    assert_eq!(budget.gauges(), charged);
    assert_eq!(
        ledger.facts().next().unwrap().state,
        LedgerState::CorrectlyRejectedHeld
    );
    ledger
        .terminal_recorded(rejected_proof(id(2, 0)), &mut budget)
        .unwrap();
    assert_eq!(budget.gauges(), (0, 0));
}

#[test]
fn exact_classifier_request_is_required_and_bad_reply_keeps_prior_evidence() {
    let mut ledger = ledger();
    ledger.install(statement(2, &[0]), 10, None).unwrap();
    let request = Ste100Request {
        request_id: 7.into(),
        target: Ste100Target::Source(id(2, 0)),
        text: "Read caf\u{e9}.".into(),
        scheme_id: "fixture-read/1".into(),
    };
    ledger
        .classification_submitted(&id(2, 0), &request)
        .unwrap();
    let mut reply = Ste100Reply {
        request_id: 8.into(),
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: Ste100Result::Accepted {
            bindings: vec![binding()],
        },
    };
    assert!(ledger.classify_recorded(&reply).is_err());
    assert_eq!(
        ledger.facts().next().unwrap().state,
        LedgerState::AwaitingClassification
    );
    assert!(ledger.next_classification().is_none());
    reply.request_id = request.request_id;
    assert!(matches!(
        ledger.classify_recorded(&reply).unwrap(),
        Classified::Accepted(_)
    ));
    assert!(ledger.classify_recorded(&reply).is_err());
}

#[test]
fn accepted_input_reuse_requires_its_original_offer_bytes_scheme_and_source() {
    for defect in 0..6 {
        let mut ledger = ledger();
        let request = Ste100Request {
            request_id: 1.into(),
            target: Ste100Target::Input {
                input_id: 4.into(),
                block: 0.into(),
            },
            text: "Read caf\u{e9}.".into(),
            scheme_id: "fixture-read/1".into(),
        };
        let reply = Ste100Reply {
            request_id: request.request_id,
            target: request.target.clone(),
            scheme_id: request.scheme_id.clone(),
            result: Ste100Result::Accepted {
                bindings: vec![binding()],
            },
        };
        let (port, mut peer) = super::super::classifier::scripted::ScriptedPort::fixture([
            super::super::classifier::scripted::ScriptStep::Reply(reply.clone()),
        ])
        .unwrap();
        let mut classifier = super::super::classifier::Classifier::fixture(port, "run".into());
        classifier.submit(request.clone()).unwrap();
        peer.advance().unwrap();
        classifier.try_result().unwrap();
        let reuse = classifier
            .recorded_input(super::super::RecordedSte100::for_test(&reply), 1.into())
            .unwrap();
        let AdmittedCopy::Statement(mut copy) = statement(2, &[0]) else {
            unreachable!()
        };
        copy.source_envelope = source(2, "message.user", "input", "operator");
        match defect {
            0 => {}
            1 => copy.blocks[0].raw = "{\"type\":\"text\",\"text\":\"Read other.\"}".into(),
            2 => copy.source_envelope = source(2, "message.assistant", "output", "model"),
            3 => {
                copy.source_envelope = copy
                    .source_envelope
                    .replace("\"causal_parent\":\"1\"", "\"causal_parent\":\"9\"")
                    .into()
            }
            4 => copy.blocks[0].source.block = 1,
            5 => ledger.scheme.scheme_id = "another".into(),
            _ => unreachable!(),
        }
        let result = ledger.install(AdmittedCopy::Statement(copy), 10, Some(reuse));
        if defect == 0 {
            result.unwrap();
            assert!(ledger.next_classification().is_none());
            assert!(ledger.next_file().is_some());
        } else {
            let failure = result.unwrap_err();
            assert!(
                failure.reuse.is_some(),
                "a refused transfer retains the acceptance token"
            );
            assert_eq!(ledger.facts().count(), 0);
        }
    }
}

#[test]
fn whole_pair_copy_bypasses_classification_and_preserves_request_bytes() {
    let mut ledger = ledger();
    let copy = pair_open(3);
    let AdmittedCopy::Pair(original) = &copy else {
        unreachable!()
    };
    let original_raw = original.block.raw.clone();
    ledger.install(copy, 10, None).unwrap();
    assert!(ledger.next_classification().is_none());
    let LedgerFile::Pair(file) = ledger.next_file().unwrap() else {
        panic!()
    };
    assert_eq!(file.raw_block.get(), original_raw.as_ref());
    assert_eq!(file.block_id, id(3, 0));
    assert!(file.source_path.is_empty());
    assert!(matches!(
        file.pair,
        PairRecord::Started(PairStarted {
            lane: PairLane::External,
            ..
        })
    ));
}

#[test]
fn pair_terminal_is_not_text_and_rejects_wrong_kind_lane_and_partial_payload() {
    let ended = PairEnded {
        call_id: CallId {
            run: "run".into(),
            sequence: 2.into(),
            call: 0.into(),
        },
        execution: None,
        fate: TerminalPairFate::Failed,
        reason: Some(PairFailure::Cancelled {}),
        outcome: None,
        loaded_from: None,
    };
    let raw = serde_json::to_string_pretty(&ended).unwrap();
    for defect in 0..5 {
        let mut ledger = ledger();
        let AdmittedCopy::Pair(mut copy) = pair_open(3) else {
            unreachable!()
        };
        copy.source_envelope = source(3, "tool.call.completed", "tool", "tool");
        copy.block.raw = raw.clone().into();
        match defect {
            0 => {}
            1 => copy.source_envelope = source(3, "message.assistant", "output", "model"),
            2 => copy.source_envelope = source(3, "tool.call.completed", "tool", "model"),
            3 => {
                copy.block.raw =
                    "{\"call_id\":{\"run\":\"run\",\"sequence\":\"2\",\"call\":\"0\"}}".into()
            }
            4 => copy.block.source_path = vec![PathPart::Key("outcome".into())],
            _ => unreachable!(),
        }
        let result = ledger.install(AdmittedCopy::Pair(copy), 10, None);
        if defect == 0 {
            result.unwrap();
            let LedgerFile::Pair(file) = ledger.next_file().unwrap() else {
                panic!()
            };
            assert_eq!(file.raw_block.get(), raw);
            assert_eq!(file.pair, PairRecord::Ended(ended.clone()));
        } else {
            assert!(result.is_err(), "defect {defect}");
        }
    }
}

fn span(local: u64) -> SpanRef {
    SpanRef {
        seed_id: "seed".into(),
        local_id: local.into(),
        revision: 1.into(),
    }
}
fn plan() -> PlanRef {
    PlanRef {
        plan_id: PlanId {
            seed_id: "seed".into(),
            local_id: 1.into(),
        },
        revision: 1.into(),
    }
}

#[test]
fn only_accepted_licensed_context_becomes_a_mechanical_effect() {
    let effect = MarkEffect::BeginTask {
        task: span(2),
        prompt: span(3),
        source: ContentRef::Event { block_id: id(2, 0) },
    };
    let raw = serde_json::to_string_pretty(&effect).unwrap();
    let mut ledger = ledger();
    ledger
        .install(mechanical("state.mark", &raw), 10, None)
        .unwrap();
    assert!(ledger.next_file().is_none());
    check(&mut ledger, id(3, 0), 1, true);
    let LedgerFile::Statement(file) = ledger.next_file().unwrap() else {
        panic!()
    };
    assert_eq!(file.effect, Some(StatementEffect::Mark(effect)));
    assert_eq!(file.context.unwrap().raw.get(), raw);

    let mut rejected = super::Ledger::fixture("run".into(), "session".into());
    rejected
        .install(mechanical("state.mark", "{\"bad\":true}"), 10, None)
        .unwrap();
    assert!(matches!(
        check(&mut rejected, id(3, 0), 1, false),
        Classified::CorrectlyRejected(_)
    ));
    assert!(
        rejected.next_file().is_none(),
        "rejected text grants no context effect"
    );
}

#[test]
fn internal_plan_effect_operator_provenance_and_failed_review_have_distinct_shapes() {
    let effect = PlanEffect::Break {
        expected: plan(),
        step_id: None,
        case: PlanBreakCase::WrongCategory,
        proposed_source: None,
    };
    let mut ledger = ledger();
    ledger
        .install(
            mechanical("plan.event", &serde_json::to_string(&effect).unwrap()),
            10,
            None,
        )
        .unwrap();
    check(&mut ledger, id(3, 0), 1, true);
    let LedgerFile::Statement(file) = ledger.next_file().unwrap() else {
        panic!()
    };
    assert_eq!(file.effect, Some(StatementEffect::Plan(Box::new(effect))));

    let mut ledger = super::Ledger::fixture("run".into(), "session".into());
    let provenance = PlanInputContext {
        document_sha256: Digest::new("0".repeat(64)).unwrap(),
        cause: weaver_types::Cause { uid: 0 },
        expected: None,
        operation: PlanInputOperation::Create {
            task: 2.into(),
            steps: vec![],
            categories: vec![],
            groups: vec![],
            review: ReviewSetupRef {
                setup_id: "review".into(),
                sha256: Digest::new("1".repeat(64)).unwrap(),
            },
        },
    };
    let AdmittedCopy::Statement(mut copy) = mechanical(
        "plan.event",
        &serde_json::to_string_pretty(&provenance).unwrap(),
    ) else {
        unreachable!()
    };
    copy.source_envelope = source(3, "plan.event", "input", "operator");
    ledger
        .install(AdmittedCopy::Statement(copy), 10, None)
        .unwrap();
    check(&mut ledger, id(3, 0), 1, true);
    let LedgerFile::Statement(file) = ledger.next_file().unwrap() else {
        panic!()
    };
    assert!(file.context.is_some());
    assert!(
        file.effect.is_none(),
        "operator input never applies its own plan effect"
    );

    let mut ledger = super::Ledger::fixture("run".into(), "session".into());
    ledger
        .install(mechanical("step.outcome", " null "), 10, None)
        .unwrap();
    check(&mut ledger, id(3, 0), 1, true);
    let LedgerFile::Statement(file) = ledger.next_file().unwrap() else {
        panic!()
    };
    assert!(file.context.is_none() && file.effect.is_none());
    assert_eq!(
        ledger.facts().next().unwrap().context_bytes,
        6,
        "traffic still counts the copied literal null"
    );
}

#[test]
fn successful_review_carries_only_the_exact_close_effect_on_step_outcome() {
    let effect = PlanEffect::Close {
        expected: plan(),
        step_id: StepId {
            seed_id: "seed".into(),
            local_id: 1.into(),
        },
        review_call: None,
        work: WorkRef {
            plan: plan(),
            step_id: StepId {
                seed_id: "seed".into(),
                local_id: 1.into(),
            },
            span: span(4),
            through: SourcePosition {
                block_id: id(2, 0),
                turn: Some("turn".into()),
            },
            content_sha256: Digest::new("a".repeat(64)).unwrap(),
        },
        outcome_span: span(5),
    };
    let raw = serde_json::to_string_pretty(&effect).unwrap();
    let mut ledger = ledger();
    ledger
        .install(mechanical("step.outcome", &raw), 10, None)
        .unwrap();
    check(&mut ledger, id(3, 0), 1, true);
    let LedgerFile::Statement(file) = ledger.next_file().unwrap() else {
        panic!()
    };
    assert_eq!(file.effect, Some(StatementEffect::Plan(Box::new(effect))));
    assert_eq!(file.context.unwrap().raw.get(), raw);
}

#[test]
fn unlicensed_context_and_direct_mark_close_fail_without_losing_source_state() {
    for copy in [
        mechanical(
            "state.mark",
            &serde_json::to_string(&MarkEffect::Close { span: span(2) }).unwrap(),
        ),
        mechanical("loop.instruction", "{\"anything\":1}"),
        mechanical(
            "step.outcome",
            &serde_json::to_string(&PlanEffect::Break {
                expected: plan(),
                step_id: None,
                case: PlanBreakCase::SelfEdit,
                proposed_source: None,
            })
            .unwrap(),
        ),
    ] {
        let mut ledger = ledger();
        ledger.install(copy, 10, None).unwrap();
        let request = Ste100Request {
            request_id: 1.into(),
            target: Ste100Target::Source(id(3, 0)),
            text: "Read caf\u{e9}.".into(),
            scheme_id: "fixture-read/1".into(),
        };
        ledger
            .classification_submitted(&id(3, 0), &request)
            .unwrap();
        let reply = Ste100Reply {
            request_id: 1.into(),
            target: request.target,
            scheme_id: request.scheme_id,
            result: Ste100Result::Accepted {
                bindings: vec![binding()],
            },
        };
        assert!(ledger.classify_recorded(&reply).is_err());
        assert_eq!(
            ledger.facts().next().unwrap().state,
            LedgerState::AwaitingClassification
        );
        assert!(ledger.next_file().is_none());
    }
}

#[test]
fn a_receipt_is_held_until_its_terminal_record_and_only_then_releases_budget() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0]));
    let charged = budget.gauges();
    check(&mut ledger, id(2, 0), 1, true);
    assert!(
        ledger
            .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
            .is_err()
    );
    ledger.begin_file(&id(2, 0), &common()).unwrap();
    ledger
        .observe_progress(&progress(35, CommitEvidence::NotCommitted), Some(11))
        .unwrap();
    assert!(ledger.receipt_recorded(&receipt()).is_err());
    ledger
        .observe_progress(&progress(100, CommitEvidence::Unknown), Some(11))
        .unwrap();
    assert!(ledger.receipt_recorded(&receipt()).is_err());
    ledger
        .observe_progress(&progress(100, CommitEvidence::Committed), Some(11))
        .unwrap();
    let terminal = ledger.receipt_recorded(&receipt()).unwrap();
    assert_eq!(terminal.first_sent_ns, Some(11));
    assert_eq!(terminal.outcome, TerminalOutcome::Filed);
    assert_eq!(budget.gauges(), charged);
    assert!(!ledger.barrier(&[id(2, 0)]).unwrap());
    ledger
        .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
        .unwrap();
    assert_eq!(budget.gauges(), (0, 0));
    assert!(ledger.barrier(&[id(2, 0)]).unwrap());
    assert!(ledger.receipt_recorded(&receipt()).is_err());
}

#[test]
fn transport_identity_offsets_and_original_first_write_time_are_immutable() {
    let mut ledger = ledger();
    ledger.install(statement(2, &[0]), 10, None).unwrap();
    check(&mut ledger, id(2, 0), 1, true);
    ledger.begin_file(&id(2, 0), &common()).unwrap();
    ledger
        .observe_progress(&progress(35, CommitEvidence::NotCommitted), Some(11))
        .unwrap();
    for defect in 0..5 {
        let mut bad = progress(40, CommitEvidence::NotCommitted);
        let mut sent = Some(11);
        match defect {
            0 => bad.common.request_id = 3.into(),
            1 => bad.source = Some(id(2, 3)),
            2 => bad.bytes_sent = 20,
            3 => bad.frame_bytes = 101,
            4 => sent = Some(12),
            _ => unreachable!(),
        }
        assert!(ledger.observe_progress(&bad, sent).is_err());
        assert_eq!(
            ledger.facts().next().unwrap().progress.unwrap().bytes_sent,
            35
        );
    }
    ledger.freeze();
    assert!(ledger.next_file().is_none());
    assert!(ledger.next_classification().is_none());
    let fact = ledger.facts().next().unwrap();
    assert_eq!(fact.state, LedgerState::Sending);
    assert_eq!(fact.progress.unwrap().commit, CommitEvidence::NotCommitted);
    assert!(ledger.barrier(&[id(2, 0)]).is_err());
    assert!(
        ledger
            .observe_progress(&progress(100, CommitEvidence::Unknown), Some(11))
            .is_err()
    );
}

#[test]
fn halt_preserves_committed_but_unrecorded_receipt_evidence() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0]));
    check(&mut ledger, id(2, 0), 1, true);
    ledger.begin_file(&id(2, 0), &common()).unwrap();
    ledger
        .observe_progress(&progress(100, CommitEvidence::Committed), Some(11))
        .unwrap();
    let charged = budget.gauges();
    ledger.freeze();
    budget.freeze();
    assert_eq!(
        ledger.facts().next().unwrap().state,
        LedgerState::AwaitingReceipt
    );
    assert_eq!(
        ledger.facts().next().unwrap().progress.unwrap().commit,
        CommitEvidence::Committed
    );
    assert!(ledger.receipt_recorded(&receipt()).is_err());
    assert_eq!(budget.gauges(), charged);
}
