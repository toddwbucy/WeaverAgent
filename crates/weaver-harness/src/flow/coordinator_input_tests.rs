//! Input checks use the same real trace barrier and classifier proof as filing.
//! The scripted port is protocol evidence, not language qualification.

use super::tests::Fixture;
use super::*;
use crate::flow::classifier::scripted::ScriptStep;
use crate::flow::report::{FlowHaltReason, HaltPoint, SecondaryFailure};
use weaver_types::state_flow as s;

fn start(fx: &mut Fixture, id: u64, text: &str) -> (CheckOperation, u64) {
    let began = fx.time();
    // Registration and authoring follow the original serialized intake sample.
    fx.elapse(5);
    fx.flow
        .input_started(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            id.into(),
            began,
        )
        .unwrap();
    let frame = weaver_types::TurnFrame::carry_from(
        &serde_json::to_vec(&serde_json::json!({"text":text})).unwrap(),
        0,
    );
    let now = fx.time();
    let progress = fx.barrier.submit_authored_with_clock(
        AuthoredEvent {
            kind: Kind::InputOffered,
            subsystem: Subsystem::Harness,
            turn: None,
            source: AuthoredSource::BoundaryInput(0),
            causal_parent: None,
            payload: Some(Payload::InputOffered(Box::new(trace::InputOfferedRecord {
                input_id: id.into(),
                octets: frame.octets,
            }))),
        },
        RecordPurpose::Observation,
        &fx.author,
        1,
        &mut fx.inbox,
        &mut fx.budget,
        &mut || Ok(now),
    );
    let RecordProgress::Recorded {
        sequence,
        proof: None,
    } = progress
    else {
        panic!("real input offer: {progress:?}");
    };
    (
        CheckOperation::Input {
            input_id: id.into(),
            offered_sequence: sequence.0.into(),
            text: text.into(),
        },
        began,
    )
}

fn submit(fx: &mut Fixture, operation: CheckOperation) -> DecimalU64 {
    let result = fx
        .flow
        .begin_input_check(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            operation,
        )
        .unwrap();
    let OperationStart::Started { request_id } = result else {
        panic!("input request should start: {result:?}");
    };
    request_id
}

fn take(fx: &mut Fixture) -> Option<OperationCompletion> {
    fx.flow
        .take_operation_completion(&mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        })
        .unwrap()
}

fn complete(fx: &mut Fixture, reject: bool) -> OperationCompletion {
    fx.answer_classifier(reject);
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        if let Some(result) = take(fx) {
            return result;
        }
    }
    panic!("input completion did not cross the actual trace barrier");
}

fn finish(fx: &mut Fixture, id: u64) {
    fx.flow
        .input_finished(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            id.into(),
        )
        .unwrap();
}

fn settle_notices(fx: &mut Fixture) {
    for _ in 0..128 {
        if !fx.flow.input_measurements_pending() {
            return;
        }
        fx.advance().unwrap();
    }
    panic!("input measurements did not record");
}

fn records(fx: &Fixture) -> Vec<serde_json::Value> {
    fx.barrier
        .recorder()
        .structure()
        .iter()
        .map(|record| serde_json::from_str(&record.line).unwrap())
        .collect()
}

fn grant(fx: &mut Fixture, text: &str) -> (AcceptedReuse, weaver_trace::Sequence) {
    let (operation, _) = start(fx, 1, text);
    let CheckOperation::Input {
        offered_sequence, ..
    } = &operation
    else {
        unreachable!()
    };
    let offered = weaver_trace::Sequence(offered_sequence.0);
    submit(fx, operation);
    let OperationCompletion::Input {
        reuse: Some(reuse), ..
    } = complete(fx, false)
    else {
        panic!("accepted input must carry its recorded grant");
    };
    finish(fx, 1);
    settle_notices(fx);
    (reuse, offered)
}

fn user(fx: &mut Fixture, text: &str, offer: weaver_trace::Sequence) -> weaver_trace::Sequence {
    user_blocks(fx, serde_json::json!([{"type":"text","text":text}]), offer)
}

fn user_blocks(
    fx: &mut Fixture,
    content: serde_json::Value,
    offer: weaver_trace::Sequence,
) -> weaver_trace::Sequence {
    let now = fx.time();
    let progress = fx.barrier.submit_authored_with_clock(
        AuthoredEvent {
            kind: Kind::MessageUser,
            subsystem: Subsystem::Harness,
            turn: Some(TurnRef("input-turn".into())),
            source: AuthoredSource::BoundaryInput(0),
            causal_parent: Some(offer),
            payload: Some(Payload::Message(
                serde_json::value::to_raw_value(&serde_json::json!({
                    "role":"user", "content":content
                }))
                .unwrap(),
            )),
        },
        RecordPurpose::Content,
        &fx.author,
        1,
        &mut fx.inbox,
        &mut fx.budget,
        &mut || Ok(now),
    );
    let RecordProgress::Recorded {
        sequence,
        proof: None,
    } = progress
    else {
        panic!("accepted source: {progress:?}");
    };
    sequence
}

fn drain(fx: &mut Fixture) -> Result<(), FlowFailure> {
    fx.flow.drain(
        &mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        },
        &mut fx.clock,
    )
}

fn finish_author(fx: &mut Fixture, sequence: weaver_trace::Sequence) {
    fx.flow
        .finish_input_author(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            sequence,
        )
        .unwrap();
}

#[test]
fn accepted_input_uses_its_real_recording_proof_and_counts_the_original_intake_once() {
    let mut fx = Fixture::new(4);
    let (operation, began) = start(&mut fx, 1, "Read alpha.");
    fx.elapse(7);
    let id = submit(&mut fx, operation.clone());
    let OperationCompletion::Input {
        operation: actual,
        reply,
        reuse: Some(reuse),
        recorded_ns,
    } = complete(&mut fx, false)
    else {
        panic!("accepted input completion");
    };
    assert_eq!(actual, operation);
    assert_eq!(recorded_ns, fx.time());
    assert_eq!(reply.request_id, id);
    assert_eq!(reuse.reply(), &reply);
    assert_eq!(reuse.text(), "Read alpha.");
    assert!(fx.flow.ledger().facts().next().is_none());
    assert_eq!(fx.flow.pending_input_id(), Some(1.into()));
    let trace = records(&fx);
    assert!(trace.iter().any(|r| r["kind"] == "ste100.result"));
    assert!(!trace.iter().any(|r| r["kind"] == "message.user"));
    let result_at = trace
        .iter()
        .position(|r| r["kind"] == "ste100.result")
        .unwrap();
    let started_at = trace
        .iter()
        .position(|r| {
            r["kind"] == "state.flow" && r["payload"]["phase"] == "classification_started"
        })
        .unwrap();
    assert!(started_at < result_at);
    fx.elapse(13);
    let elapsed = fx.time() - began;
    finish(&mut fx, 1);
    assert!(fx.flow.pending_input_id().is_none());
    assert!(fx.flow.input_measurements_pending());
    settle_notices(&mut fx);
    let trace = records(&fx);
    let samples: Vec<_> = trace.iter().filter(|r| r["kind"] == "state.flow").collect();
    let completed = samples
        .iter()
        .find(|r| r["payload"]["phase"] == "wait_finished")
        .unwrap();
    assert_eq!(
        completed["payload"]["elapsed_ns"].as_str(),
        Some(elapsed.to_string().as_str())
    );
    assert_eq!(completed["payload"]["input_pending"], "0");
    assert_eq!(completed["payload"]["input_completed"], "1");
    assert_eq!(
        samples
            .iter()
            .filter_map(|r| r["payload"]["input_started"].as_str())
            .map(|v| v.parse::<u64>().unwrap())
            .sum::<u64>(),
        1
    );
    assert_eq!(
        samples
            .iter()
            .filter_map(|r| r["payload"]["input_completed"].as_str())
            .map(|v| v.parse::<u64>().unwrap())
            .sum::<u64>(),
        1
    );
    assert!(fx.flow.failure().is_none());
}

#[test]
fn correct_input_rejection_has_no_grant_or_ledger_and_leaves_the_port_usable() {
    let mut fx = Fixture::new(4);
    let (operation, _) = start(&mut fx, 1, "Read rejected.");
    submit(&mut fx, operation.clone());
    let OperationCompletion::Input {
        operation: actual,
        reply,
        reuse,
        ..
    } = complete(&mut fx, true)
    else {
        panic!("correct refusal stays an input result");
    };
    assert_eq!(actual, operation);
    assert!(matches!(reply.result, Ste100Result::NotSte100 { .. }));
    assert!(reuse.is_none());
    assert!(fx.flow.ledger().facts().next().is_none());
    assert!(fx.flow.failure().is_none());
    finish(&mut fx, 1);
    settle_notices(&mut fx);
    let (next, _) = start(&mut fx, 2, "Read next.");
    submit(&mut fx, next);
    assert!(matches!(
        complete(&mut fx, false),
        OperationCompletion::Input { reuse: Some(_), .. }
    ));
}

#[test]
fn input_component_failure_records_its_exact_reply_then_freezes_the_pending_account() {
    let mut fx = Fixture::new(4);
    let (operation, _) = start(&mut fx, 1, "Read alpha.");
    submit(&mut fx, operation);
    let request = fx.classifier.submitted()[0].clone();
    let reply = Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: Ste100Result::ComponentFailed {
            code: s::Ste100ComponentCode::NoResult,
            detail: "terminal fixture result".into(),
        },
    };
    fx.classifier.push(ScriptStep::Reply(reply.clone()));
    fx.classifier.advance().unwrap();
    let mut failure = None;
    for _ in 0..128 {
        fx.transport().unwrap();
        match fx.advance() {
            Ok(_) => {}
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    assert_eq!(failure.unwrap().point, "classifier");
    assert!(fx.flow.operation_completion().is_none());
    assert_eq!(fx.flow.pending_input_id(), Some(1.into()));
    let trace = records(&fx);
    let recorded = trace.iter().find(|r| r["kind"] == "ste100.result").unwrap();
    assert_eq!(recorded["payload"], serde_json::to_value(reply).unwrap());
    assert!(fx.budget.is_frozen());
}

#[test]
fn offered_input_is_in_the_frozen_sample_before_any_check_is_submitted() {
    let mut fx = Fixture::new(4);
    let began = fx.time();
    fx.flow
        .input_started(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            1.into(),
            began,
        )
        .unwrap();
    let now = fx.time();
    fx.flow.latch(
        SecondaryFailure {
            reason: FlowHaltReason::TraceFailed,
            point: HaltPoint::Author,
            observed_ns: Some(now.into()),
            detail: "failed before offer recorded".into(),
        },
        None,
        &mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        },
    );
    let sample = fx.flow.final_sample().unwrap().unwrap();
    assert_eq!(sample.input_pending.0, 1);
    assert_eq!(sample.input_started.0, 1);
    assert_eq!(sample.input_completed.0, 0);
    assert!(fx.classifier.submitted().is_empty());
}

#[test]
fn missing_or_repeated_input_identity_cannot_submit_or_finish_an_exchange() {
    for case in 0..4 {
        let mut fx = Fixture::new(4);
        let (mut operation, _) = start(&mut fx, 1, "Read alpha.");
        if let CheckOperation::Input {
            input_id,
            offered_sequence,
            ..
        } = &mut operation
        {
            match case {
                0 => *input_id = 0.into(),
                1 => *input_id = 2.into(),
                2 => *offered_sequence = 0.into(),
                _ => {}
            }
        }
        let result = if case == 3 {
            fx.flow.begin_check(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                operation,
            )
        } else {
            fx.flow.begin_input_check(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                &mut fx.clock,
                operation,
            )
        };
        assert!(result.is_err(), "invalid input route {case}");
        assert!(fx.classifier.submitted().is_empty());
    }
    let mut fx = Fixture::new(4);
    let (operation, _) = start(&mut fx, 1, "Read alpha.");
    submit(&mut fx, operation);
    assert!(
        fx.flow
            .input_finished(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                &mut fx.clock,
                1.into()
            )
            .is_err()
    );
    assert!(fx.flow.classifier().outstanding().is_some());
    assert_eq!(fx.flow.pending_input_id(), Some(1.into()));
}

#[test]
fn reuse_skips_an_unrelated_copy_and_reaches_only_its_correlated_input() {
    let mut fx = Fixture::new(4);
    let (reuse, offer) = grant(&mut fx, "Read input.");
    fx.flow.reuse_next_input(reuse).unwrap();
    let unrelated = fx.admit(&["Read earlier."])[0].clone();
    let source = user(&mut fx, "Read input.", offer);
    drain(&mut fx).unwrap();
    finish_author(&mut fx, source);
    let facts: Vec<_> = fx.flow.ledger().facts().collect();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].source, &unrelated);
    assert_eq!(facts[0].state, LedgerState::AwaitingClassification);
    assert_eq!(facts[1].source.sequence.0, source.0);
    assert_eq!(facts[1].state, LedgerState::ReadyToFile);
    assert!(fx.flow.reuse.is_none());
    assert_eq!(fx.classifier.submitted().len(), 1);
}

#[test]
fn a_recorded_zero_block_copy_retires_unused_reuse_before_the_next_copy() {
    // Serving partitions must select message.user. This directly exercises the
    // generic zero-projection boundary, not an allowed engine input shape.
    let mut fx = Fixture::new(4);
    let (reuse, offer) = grant(&mut fx, "Read omitted.");
    fx.flow.reuse_next_input(reuse).unwrap();
    let source = user_blocks(&mut fx, serde_json::json!([]), offer);
    assert!(
        fx.inbox
            .queued()
            .all(|copy| super::super::intake::selected_blocks(copy).is_empty())
    );
    finish_author(&mut fx, source);
    assert!(fx.flow.reuse.is_none());
    let unrelated = fx.admit(&["Read later."])[0].clone();
    drain(&mut fx).unwrap();
    let fact = fx.flow.ledger().facts().next().unwrap();
    assert_eq!(fact.source, &unrelated);
    assert_eq!(fact.state, LedgerState::AwaitingClassification);
    assert!(fx.flow.failure().is_none());
}

#[test]
fn a_waiting_input_copy_keeps_its_pinned_grant_until_the_reservation_can_move() {
    let mut fx = Fixture::new(1);
    let (reuse, offer) = grant(&mut fx, "Read waiting.");
    fx.flow.reuse_next_input(reuse).unwrap();
    fx.admit(&["Read occupied."]);
    let source = user(&mut fx, "Read waiting.", offer);
    assert!(fx.budget.waiting_copy().is_some());
    drain(&mut fx).unwrap();
    finish_author(&mut fx, source);
    assert_eq!(
        fx.flow.reuse.as_ref().unwrap().source_sequence,
        Some(source.0)
    );
    fx.until_file();
    fx.answer_file(0, false);
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        if fx.budget.waiting_copy().is_none() {
            break;
        }
    }
    assert!(fx.budget.waiting_copy().is_none());
    let fact = fx
        .flow
        .ledger()
        .facts()
        .find(|fact| fact.source.sequence.0 == source.0)
        .unwrap();
    assert_eq!(fact.state, LedgerState::ReadyToFile);
    assert!(fx.flow.reuse.is_none());
    assert_eq!(
        fx.classifier.submitted().len(),
        2,
        "input was checked only once"
    );
}

#[test]
fn busy_input_keeps_the_whole_operation_and_original_intake_account() {
    let mut fx = Fixture::new(4);
    let template_spec = s::TemplateSpec {
        template_id: "busy-template".into(),
        kind: "loop.instruction".into(),
        parts: vec![s::TemplatePart::Literal("Read template.".into())],
    };
    let template = CheckOperation::Template {
        template_id: template_spec.template_id.clone(),
        text: serde_json::to_string(&template_spec).unwrap(),
    };
    assert!(matches!(
        fx.flow
            .begin_check(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                template
            )
            .unwrap(),
        OperationStart::Started { .. }
    ));
    let (operation, began) = start(&mut fx, 1, "Read later.");
    fx.elapse(11);
    let busy = fx
        .flow
        .begin_input_check(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            operation.clone(),
        )
        .unwrap();
    assert_eq!(busy, OperationStart::Busy(operation.clone()));
    assert_eq!(fx.flow.input_account.as_ref().unwrap().began_ns, began);
    assert!(fx.flow.input_account.as_ref().unwrap().request_id.is_none());
    assert_eq!(fx.classifier.submitted().len(), 1);
    let request = fx.classifier.submitted()[0].clone();
    fx.classifier.push(ScriptStep::Reply(Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: Ste100Result::Accepted { bindings: vec![] },
    }));
    fx.classifier.advance().unwrap();
    let mut template_completion = None;
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        if let Some(result) = take(&mut fx) {
            template_completion = Some(result);
            break;
        }
    }
    assert!(matches!(
        template_completion,
        Some(OperationCompletion::Check(_))
    ));
    submit(&mut fx, operation);
    let request = fx.classifier.submitted()[1].clone();
    let len = request.text.len() as u64;
    fx.classifier.push(ScriptStep::Reply(Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: Ste100Result::Accepted {
            bindings: vec![s::Binding {
                statement_span: s::Span {
                    start: 0.into(),
                    end: len.into(),
                },
                verb: "read".into(),
                verb_span: s::Span {
                    start: 0.into(),
                    end: 4.into(),
                },
                condition: None,
                condition_span: None,
                nouns: vec![s::NounBinding {
                    value: "later".into(),
                    span: s::Span {
                        start: 5.into(),
                        end: (len - 1).into(),
                    },
                }],
            }],
        },
    }));
    fx.classifier.advance().unwrap();
    let mut input_completion = None;
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        if let Some(result) = take(&mut fx) {
            input_completion = Some(result);
            break;
        }
    }
    assert!(matches!(
        input_completion,
        Some(OperationCompletion::Input { reuse: Some(_), .. })
    ));
    let elapsed = fx.time() - began;
    finish(&mut fx, 1);
    settle_notices(&mut fx);
    let samples = records(&fx);
    let finished = samples
        .iter()
        .find(|r| r["kind"] == "state.flow" && r["payload"]["phase"] == "wait_finished")
        .unwrap();
    assert_eq!(finished["payload"]["elapsed_ns"], elapsed.to_string());
    assert_eq!(
        samples
            .iter()
            .filter_map(|r| r["payload"]["input_started"].as_str())
            .map(|v| v.parse::<u64>().unwrap())
            .sum::<u64>(),
        1
    );
}

#[test]
fn input_acceptance_waits_for_the_exact_result_record_but_not_its_later_notice() {
    let mut fx = Fixture::new(4);
    let (operation, _) = start(&mut fx, 1, "Read held.");
    submit(&mut fx, operation.clone());
    settle_notices(&mut fx);
    let pending = fx.fill_trace_capacity();
    fx.answer_classifier(false);
    fx.transport().unwrap();
    let held = fx.flow.classifier().held().unwrap().clone();
    for _ in 0..4 {
        assert_eq!(fx.advance().unwrap(), Advance::Pending);
        assert!(take(&mut fx).is_none());
        assert_eq!(fx.flow.classifier().held(), Some(&held));
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            pending
        );
        assert!(!records(&fx).iter().any(|r| r["kind"] == "ste100.result"));
        assert!(fx.flow.ledger().facts().next().is_none());
    }
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    let mut completion = None;
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        if let Some(value) = take(&mut fx) {
            completion = Some(value);
            break;
        }
    }
    let OperationCompletion::Input {
        operation: actual,
        reply,
        reuse: Some(reuse),
        recorded_ns,
    } = completion.unwrap()
    else {
        panic!("recorded input completion");
    };
    assert_eq!(actual, operation);
    assert_eq!(recorded_ns, fx.time());
    assert_eq!(reply, held);
    assert_eq!(reuse.reply(), &held);
    assert!(
        fx.flow.input_measurements_pending(),
        "the gate result is available before its later measurement"
    );
    assert!(take(&mut fx).is_none(), "recorded grant transfers once");
    let actual = records(&fx);
    let result = actual
        .iter()
        .find(|r| r["kind"] == "ste100.result")
        .unwrap();
    assert_eq!(result["payload"], serde_json::to_value(&held).unwrap());
    assert!(
        !actual
            .iter()
            .any(|r| r["kind"] == "state.flow" && r["payload"]["phase"] == "accepted")
    );
    finish(&mut fx, 1);
    settle_notices(&mut fx);
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn intake_rejects_future_clock_samples_and_duplicate_registration() {
    for duplicate in [false, true] {
        let mut fx = Fixture::new(4);
        let began = fx.time();
        if duplicate {
            fx.flow
                .input_started(
                    &mut FlowRecording {
                        barrier: &mut fx.barrier,
                        inbox: &mut fx.inbox,
                        budget: &mut fx.budget,
                        author: &fx.author,
                    },
                    &mut fx.clock,
                    1.into(),
                    began,
                )
                .unwrap();
        }
        let result = fx.flow.input_started(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            1.into(),
            if duplicate { began } else { began + 1 },
        );
        assert!(result.is_err());
        assert!(fx.classifier.submitted().is_empty());
        assert_eq!(
            fx.flow.pending_input_id(),
            Some(1.into()),
            "failed registration retains the transferred frame account"
        );
    }
}

#[test]
fn gate_wait_ends_at_the_supplied_recorded_boundary_before_later_reduction() {
    let mut fx = Fixture::new(4);
    let (operation, began) = start(&mut fx, 1, "Read boundary.");
    submit(&mut fx, operation);
    let OperationCompletion::Input { recorded_ns, .. } = complete(&mut fx, false) else {
        panic!("input completion");
    };
    fx.elapse(900);
    fx.flow
        .input_finished_at(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            1.into(),
            recorded_ns,
        )
        .unwrap();
    settle_notices(&mut fx);
    let trace = records(&fx);
    let wait = trace
        .iter()
        .find(|r| r["payload"]["phase"] == "wait_finished")
        .unwrap();
    assert_eq!(
        wait["payload"]["elapsed_ns"],
        (recorded_ns - began).to_string()
    );
    assert_eq!(wait["payload"]["input_pending"], "0");
}
