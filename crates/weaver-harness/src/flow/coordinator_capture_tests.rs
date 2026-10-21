//! Capture results cross the real state socket and exact trace recording barrier.
//! The peer supplies protocol fixtures, not a persistence or publication claim.

use super::tests::{Fixture, operation};
use super::*;
use std::io::Write;
use weaver_types::state_flow as s;

macro_rules! cx {
    ($fx:ident) => {
        FlowRecording {
            barrier: &mut $fx.barrier,
            inbox: &mut $fx.inbox,
            budget: &mut $fx.budget,
            author: &$fx.author,
        }
    };
}

fn register(fx: &mut Fixture, frontier: u64) {
    fx.flow
        .register_capture_barrier(&mut cx!(fx), 7.into(), frontier)
        .unwrap();
}

fn begin(fx: &mut Fixture) -> DecimalU64 {
    let OperationStart::Started { request_id } = fx
        .flow
        .begin_capture_snapshot(&mut cx!(fx), &mut fx.clock, 7.into())
        .unwrap()
    else {
        panic!("settled registered barrier");
    };
    fx.transport().unwrap();
    fx.collect_requests();
    request_id
}

fn snapshot() -> StateControlAnswer {
    StateControlAnswer::Snapshot {
        capture_id: 7.into(),
        room_name: "7.part".into(),
        size_bytes: 41.into(),
        position: None,
    }
}

fn finished() -> StateControlAnswer {
    StateControlAnswer::Finished {
        capture_id: 7.into(),
        room_name: "7.sqlite".into(),
    }
}

fn refused() -> StateControlAnswer {
    StateControlAnswer::ControlRefused {
        code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::SnapshotFailed),
        detail: "fixture explicit capture refusal".into(),
    }
}

fn answer(fx: &mut Fixture, index: usize, frame: StateControlAnswer) -> StateFrame {
    let now = fx.time();
    let request = &fx.requests[index];
    let result = StateFrame::ControlAnswer {
        common: request.common().clone(),
        frame,
        observation: operation(request, index as u64 + 2, now + 1),
    };
    fx.state_peer
        .write_all(&s::encode_state_line(&result, s::FrameDirection::Response).unwrap())
        .unwrap();
    fx.elapse(3);
    fx.transport().unwrap();
    result
}

fn take(fx: &mut Fixture) -> (StateControl, StateCompletion, u64) {
    for _ in 0..64 {
        fx.advance().unwrap();
        if let Some(completion) = fx.flow.take_operation_completion(&mut cx!(fx)).unwrap() {
            let OperationCompletion::Capture {
                request,
                completion,
                recorded_ns,
            } = completion
            else {
                panic!("capture uses its own completion route");
            };
            assert!(!completion.discard_capture_result);
            return (request, completion, recorded_ns);
        }
    }
    panic!("capture did not release after its actual recording proof");
}

fn ack(fx: &mut Fixture) -> DecimalU64 {
    let OperationStart::Started { request_id } = fx
        .flow
        .acknowledge_capture(
            &mut cx!(fx),
            &mut fx.clock,
            7.into(),
            Digest::new("a".repeat(64)).unwrap(),
        )
        .unwrap()
    else {
        panic!("recorded snapshot permits acknowledgement");
    };
    fx.transport().unwrap();
    fx.collect_requests();
    request_id
}

fn abort(fx: &mut Fixture) -> DecimalU64 {
    let OperationStart::Started { request_id } = fx
        .flow
        .abort_capture(&mut cx!(fx), &mut fx.clock, 7.into())
        .unwrap()
    else {
        panic!("owned capture permits one cleanup continuation");
    };
    fx.transport().unwrap();
    fx.collect_requests();
    request_id
}

fn blocked_control(fx: &mut Fixture) {
    let request =
        ControlOperation::try_from((StateControl::Held {}, Some(TurnRef("held".into())))).unwrap();
    let OperationStart::Busy(returned) = fx
        .flow
        .begin_control(&mut cx!(fx), &mut fx.clock, request)
        .unwrap()
    else {
        panic!("capture must retain the ordinary state lane");
    };
    assert_eq!(
        returned.into_parts(),
        (StateControl::Held {}, Some(TurnRef("held".into())))
    );
}

fn assert_same_answer(actual: &StateFrame, expected: &StateFrame) {
    assert_eq!(
        s::encode_state_line(actual, s::FrameDirection::Response).unwrap(),
        s::encode_state_line(expected, s::FrameDirection::Response).unwrap(),
    );
}

#[test]
fn capture_success_keeps_one_owner_through_recorded_release_and_explicit_finish() {
    let mut fx = Fixture::new(4);
    register(&mut fx, 0);
    let original = begin(&mut fx);
    blocked_control(&mut fx);
    let expected = answer(&mut fx, 0, snapshot());
    assert!(
        fx.flow
            .take_operation_completion(&mut cx!(fx))
            .unwrap()
            .is_none()
    );
    assert!(fx.flow.ledger.facts().next().is_none());
    let (request, completion, recorded_ns) = take(&mut fx);
    assert_eq!(
        request,
        StateControl::Snapshot {
            capture_id: 7.into()
        }
    );
    assert_same_answer(&completion.answer, &expected);
    assert_eq!(recorded_ns, fx.time());
    blocked_control(&mut fx);
    assert_eq!(ack(&mut fx), original);
    answer(&mut fx, 1, finished());
    let (request, _, _) = take(&mut fx);
    assert_eq!(
        request,
        StateControl::Acknowledge {
            capture_id: 7.into(),
            dump_sha256: Digest::new("a".repeat(64)).unwrap(),
        }
    );
    blocked_control(&mut fx);
    let OperationStart::Started { request_id } = fx
        .flow
        .release_capture(&mut cx!(fx), &mut fx.clock, 7.into())
        .unwrap()
    else {
        panic!("finished capture can be released");
    };
    assert_eq!(request_id.0, original.0 + 1);
    fx.transport().unwrap();
    fx.collect_requests();
    answer(
        &mut fx,
        2,
        StateControlAnswer::CaptureReleased {
            capture_id: 7.into(),
        },
    );
    take(&mut fx);
    blocked_control(&mut fx);
    assert_eq!(
        fx.flow.capture_snapshot().unwrap().phase,
        CapturePhase::Complete
    );
    fx.flow.finish_capture(&mut cx!(fx), 7.into()).unwrap();
    assert!(fx.flow.capture_snapshot().is_none());
    let request = ControlOperation::try_from((StateControl::Held {}, None)).unwrap();
    assert!(matches!(
        fx.flow
            .begin_control(&mut cx!(fx), &mut fx.clock, request)
            .unwrap(),
        OperationStart::Started { .. }
    ));
}

#[test]
fn capture_barrier_drains_queued_waiting_and_terminal_proofs_before_snapshot() {
    let mut fx = Fixture::new(1);
    let first = fx.admit(&["Read first."]);
    let second = fx.admit(&["Read second."]);
    assert!(fx.budget.waiting_copy().is_some());
    register(&mut fx, second[0].sequence.0);
    assert!(!fx.flow.capture_barrier_ready(&mut cx!(fx)).unwrap());
    assert!(matches!(
        fx.flow
            .begin_capture_snapshot(&mut cx!(fx), &mut fx.clock, 7.into())
            .unwrap(),
        OperationStart::Busy(_)
    ));
    let mut answered = 0;
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        fx.answer_classifier(false);
        fx.collect_requests();
        while answered < fx.requests.len() {
            assert!(matches!(fx.requests[answered], StateFrame::File { .. }));
            fx.answer_file(answered, false);
            answered += 1;
        }
        if fx.flow.capture_barrier_ready(&mut cx!(fx)).unwrap() {
            break;
        }
    }
    assert_eq!(answered, 2);
    assert!(
        fx.flow
            .ledger
            .barrier(&[first[0].clone(), second[0].clone()])
            .unwrap()
    );
    assert_eq!(fx.budget.gauges(), (0, 0));
    assert!(fx.flow.capture_barrier_ready(&mut cx!(fx)).unwrap());
    begin(&mut fx);
    assert!(matches!(
        fx.requests[2],
        StateFrame::Control {
            frame: StateControl::Snapshot { .. },
            ..
        }
    ));
}

#[test]
fn capture_registration_rejects_stale_frontier_without_losing_queued_copy() {
    let mut fx = Fixture::new(2);
    let source = fx.admit(&["Read later."]);
    assert!(
        fx.flow
            .register_capture_barrier(&mut cx!(fx), 7.into(), source[0].sequence.0 - 1)
            .is_err()
    );
    assert!(fx.inbox.queued().next().is_some());
    assert!(fx.budget.is_frozen());
    assert!(fx.flow.capture_snapshot().is_none());
}

#[test]
fn capture_without_registration_and_clean_retirement_cannot_bypass_ownership() {
    let mut fx = Fixture::new(2);
    assert!(
        fx.flow
            .begin_capture_snapshot(&mut cx!(fx), &mut fx.clock, 7.into())
            .is_err()
    );
    assert!(fx.flow.state().outstanding().is_empty());
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    assert!(fx.flow.begin_releasing(&mut cx!(fx)).is_err());
    assert!(fx.flow.capture_snapshot().is_some());
}

#[test]
fn capture_barrier_cancellation_keeps_existing_filing_owned() {
    let mut fx = Fixture::new(2);
    let sources = fx.admit(&["Read existing."]);
    register(&mut fx, sources[0].sequence.0);
    fx.advance().unwrap();
    fx.flow
        .cancel_capture_barrier(&mut cx!(fx), 7.into())
        .unwrap();
    assert!(fx.flow.capture_snapshot().is_none());
    assert!(!fx.flow.ledger.barrier(&sources).unwrap());
    assert_eq!(fx.budget.gauges().0, 1);
    fx.until_file();
    assert!(matches!(fx.requests[0], StateFrame::File { .. }));
}

#[test]
fn abort_keeps_original_held_leg_and_records_late_result_without_export() {
    for acknowledgement in [false, true] {
        for late_refusal in [false, true] {
            let mut fx = Fixture::new(4);
            register(&mut fx, 0);
            let original = begin(&mut fx);
            let index = if acknowledgement {
                answer(&mut fx, 0, snapshot());
                take(&mut fx);
                ack(&mut fx);
                1
            } else {
                0
            };
            let late = if late_refusal {
                refused()
            } else if acknowledgement {
                finished()
            } else {
                snapshot()
            };
            answer(&mut fx, index, late);
            let held = fx.flow.state().held_measurement().unwrap().clone();
            assert_eq!(abort(&mut fx), original);
            assert_eq!(fx.flow.state().outstanding().len(), 2);
            assert_eq!(fx.flow.state().held_measurement().unwrap(), &held);
            assert_eq!(fx.requests[index + 1].common().request_id, original);
            blocked_control(&mut fx);
            fx.advance().unwrap();
            assert!(
                fx.flow
                    .take_operation_completion(&mut cx!(fx))
                    .unwrap()
                    .is_none()
            );
            assert_eq!(fx.flow.state().outstanding().len(), 1);
            assert_eq!(
                fx.flow.capture_snapshot().unwrap().phase,
                CapturePhase::Aborting
            );
            answer(
                &mut fx,
                index + 1,
                StateControlAnswer::Aborted {
                    capture_id: 7.into(),
                },
            );
            let (request, _, _) = take(&mut fx);
            assert_eq!(
                request,
                StateControl::Abort {
                    capture_id: 7.into()
                }
            );
            blocked_control(&mut fx);
            fx.flow.finish_capture(&mut cx!(fx), 7.into()).unwrap();
            assert!(fx.flow.failure().is_none());
            let records = fx.barrier.recorder().structure();
            let operations: Vec<_> = records
                .iter()
                .filter_map(|record| {
                    let value: serde_json::Value = serde_json::from_str(&record.line).unwrap();
                    (value["kind"] == "state.store.measurement").then_some(value)
                })
                .collect();
            // Open, optional successful Snapshot, then original late leg and Abort.
            assert_eq!(operations.len(), index + 3);
        }
    }
}

#[test]
fn capture_refusals_require_correlated_abort_and_never_latch_component_failure() {
    for acknowledgement in [false, true] {
        let mut fx = Fixture::new(4);
        register(&mut fx, 0);
        let original = begin(&mut fx);
        let index = if acknowledgement {
            answer(&mut fx, 0, snapshot());
            take(&mut fx);
            ack(&mut fx);
            1
        } else {
            0
        };
        answer(&mut fx, index, refused());
        let (_, completion, _) = take(&mut fx);
        assert!(matches!(
            completion.answer,
            StateFrame::ControlAnswer {
                frame: StateControlAnswer::ControlRefused { .. },
                ..
            }
        ));
        assert!(fx.flow.failure().is_none());
        blocked_control(&mut fx);
        assert_eq!(abort(&mut fx), original);
        answer(
            &mut fx,
            index + 1,
            StateControlAnswer::Aborted {
                capture_id: 7.into(),
            },
        );
        take(&mut fx);
        fx.flow.finish_capture(&mut cx!(fx), 7.into()).unwrap();
        assert!(fx.flow.failure().is_none());
    }
}

#[test]
fn snapshot_refusal_cannot_finish_without_a_recorded_cleanup() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    begin(&mut fx);
    answer(&mut fx, 0, refused());
    take(&mut fx);
    assert!(fx.flow.finish_capture(&mut cx!(fx), 7.into()).is_err());
    assert!(fx.flow.capture_snapshot().is_some());
}

#[test]
fn capture_waits_for_real_trace_capacity_and_keeps_original_abort_evidence() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    begin(&mut fx);
    answer(&mut fx, 0, snapshot());
    let exact = fx.flow.state().held_measurement().unwrap().clone();
    let pending = fx.fill_trace_capacity();
    abort(&mut fx);
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    assert_eq!(fx.flow.state().held_measurement().unwrap(), &exact);
    assert_eq!(
        serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
        pending
    );
    assert!(
        fx.flow
            .take_operation_completion(&mut cx!(fx))
            .unwrap()
            .is_none()
    );
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    fx.advance().unwrap();
    assert!(
        fx.flow
            .take_operation_completion(&mut cx!(fx))
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.flow.state().outstanding().len(), 1);
    answer(
        &mut fx,
        1,
        StateControlAnswer::Aborted {
            capture_id: 7.into(),
        },
    );
    take(&mut fx);
    fx.flow.finish_capture(&mut cx!(fx), 7.into()).unwrap();
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn capture_recorded_time_survives_later_completion_consumption() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    begin(&mut fx);
    let expected = answer(&mut fx, 0, snapshot());
    fx.advance().unwrap();
    let recorded_at = fx.time();
    fx.elapse(500);
    let Some(OperationCompletion::Capture {
        completion,
        recorded_ns,
        ..
    }) = fx.flow.take_operation_completion(&mut cx!(fx)).unwrap()
    else {
        panic!("recorded capture is held");
    };
    assert_eq!(recorded_ns, recorded_at);
    assert_same_answer(&completion.answer, &expected);
}

#[test]
fn capture_recorded_time_survives_interrupted_release_and_pending_author() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    begin(&mut fx);
    answer(&mut fx, 0, snapshot());
    let measurement = fx.flow.state().held_measurement().unwrap().clone();
    let event = AuthoredEvent {
        kind: Kind::StateStoreMeasurement,
        subsystem: Subsystem::Harness,
        turn: None,
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::StateStoreMeasurement(Box::new(
            super::super::mirror::measurement(&measurement),
        ))),
    };
    let RecordProgress::Recorded {
        proof: Some(RecordProof::State(proof)),
        ..
    } = fx.barrier.submit_authored_with_clock(
        event,
        RecordPurpose::State(measurement.clone()),
        &fx.author,
        1,
        &mut fx.inbox,
        &mut fx.budget,
        &mut || fx.clock.now(),
    )
    else {
        panic!("exact state measurement records");
    };
    let recorded_ns = fx.time();
    fx.flow.state_recorded_ns = Some(recorded_ns);
    fx.flow.state_release_timing = Some((
        measurement.harness_received_ns.0,
        measurement.harness_sent_ns.0,
    ));
    let completion = fx
        .flow
        .state
        .as_mut()
        .unwrap()
        .release_interrupted_for_test(proof)
        .unwrap();
    assert!(completion.is_none());
    assert_eq!(
        fx.flow
            .finish_state_release(&mut cx!(fx), &mut fx.clock, completion)
            .unwrap(),
        Advance::Pending
    );
    let pending = fx.fill_trace_capacity();
    fx.elapse(500);
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    assert!(
        fx.flow
            .take_operation_completion(&mut cx!(fx))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
        pending
    );
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    let (_, _, actual) = take(&mut fx);
    assert_eq!(actual, recorded_ns);
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn abort_cleanup_refusal_records_then_fails_without_releasing_capture() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    begin(&mut fx);
    answer(&mut fx, 0, snapshot());
    take(&mut fx);
    abort(&mut fx);
    answer(&mut fx, 1, refused());
    assert!(fx.flow.failure().is_none());
    assert!(fx.advance().is_err());
    assert!(fx.flow.capture_snapshot().is_some());
    assert!(fx.flow.state().held_measurement().is_some());
    assert!(fx.flow.operation_completion().is_none());
    assert!(fx.budget.is_frozen());
    let records = fx.barrier.recorder().structure();
    let count = records
        .iter()
        .filter(|record| {
            let value: serde_json::Value = serde_json::from_str(&record.line).unwrap();
            value["kind"] == "state.store.measurement"
        })
        .count();
    assert_eq!(
        count, 3,
        "Open, Snapshot and refused Abort are all recorded"
    );
}

#[test]
fn abort_can_start_while_recorded_original_waits_behind_an_external_author() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    let original = begin(&mut fx);
    let expected = answer(&mut fx, 0, snapshot());
    fx.advance().unwrap();
    let recorded_ns = fx.time();
    assert!(matches!(
        fx.flow.operation_completion(),
        Some(OperationCompletion::Capture { .. })
    ));
    let pending = fx.fill_trace_capacity();
    fx.elapse(500);
    assert!(
        fx.flow
            .take_operation_completion(&mut cx!(fx))
            .unwrap()
            .is_none()
    );
    assert_eq!(abort(&mut fx), original);
    assert_eq!(fx.flow.state().outstanding().len(), 1);
    assert_eq!(
        fx.flow.capture_snapshot().unwrap().phase,
        CapturePhase::Aborting
    );
    let Some(OperationCompletion::Capture {
        completion,
        recorded_ns: retained,
        ..
    }) = fx.flow.operation_completion()
    else {
        panic!("recorded original must remain owned");
    };
    assert!(completion.discard_capture_result);
    assert_eq!(*retained, recorded_ns);
    assert_same_answer(&completion.answer, &expected);
    assert_eq!(
        serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
        pending
    );
    answer(
        &mut fx,
        1,
        StateControlAnswer::Aborted {
            capture_id: 7.into(),
        },
    );
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    assert!(
        fx.flow
            .take_operation_completion(&mut cx!(fx))
            .unwrap()
            .is_none()
    );
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    let Some(OperationCompletion::Capture {
        request,
        completion,
        recorded_ns: retained,
    }) = fx.flow.take_operation_completion(&mut cx!(fx)).unwrap()
    else {
        panic!("recorded original must transfer exactly once as discard");
    };
    assert_eq!(
        request,
        StateControl::Snapshot {
            capture_id: 7.into()
        }
    );
    assert!(completion.discard_capture_result);
    assert_eq!(retained, recorded_ns);
    assert_same_answer(&completion.answer, &expected);
    let (request, completion, _) = take(&mut fx);
    assert_eq!(
        request,
        StateControl::Abort {
            capture_id: 7.into()
        }
    );
    assert!(!completion.discard_capture_result);
    fx.flow.finish_capture(&mut cx!(fx), 7.into()).unwrap();
    drop(fx);
    reader.join().unwrap();
}

fn interrupt_actual_capture_release(fx: &mut Fixture) -> u64 {
    let measurement = fx.flow.state().held_measurement().unwrap().clone();
    let event = AuthoredEvent {
        kind: Kind::StateStoreMeasurement,
        subsystem: Subsystem::Harness,
        turn: None,
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::StateStoreMeasurement(Box::new(
            super::super::mirror::measurement(&measurement),
        ))),
    };
    let RecordProgress::Recorded {
        proof: Some(RecordProof::State(proof)),
        ..
    } = fx.barrier.submit_authored_with_clock(
        event,
        RecordPurpose::State(measurement.clone()),
        &fx.author,
        1,
        &mut fx.inbox,
        &mut fx.budget,
        &mut || fx.clock.now(),
    )
    else {
        panic!("actual capture measurement records");
    };
    let now = fx.time();
    fx.flow.state_recorded_ns = Some(now);
    fx.flow.state_release_timing = Some((
        measurement.harness_received_ns.0,
        measurement.harness_sent_ns.0,
    ));
    let result = fx
        .flow
        .state
        .as_mut()
        .unwrap()
        .release_interrupted_for_test(proof)
        .unwrap();
    assert!(result.is_none());
    assert_eq!(
        fx.flow
            .finish_state_release(&mut cx!(fx), &mut fx.clock, result)
            .unwrap(),
        Advance::Pending
    );
    now
}

#[test]
fn queued_release_keeps_finished_and_original_anchor_until_later_author_records() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    let original = begin(&mut fx);
    answer(&mut fx, 0, snapshot());
    take(&mut fx);
    ack(&mut fx);
    let expected = answer(&mut fx, 1, finished());
    fx.advance().unwrap();
    let recorded_ns = fx.time();
    let pending = fx.fill_trace_capacity();
    let queued_ns = fx.time();
    assert!(
        fx.flow
            .enqueue_capture_release(&mut cx!(fx), &mut fx.clock, 7.into())
            .unwrap()
    );
    assert_eq!(
        fx.flow.capture_snapshot().unwrap().phase,
        CapturePhase::ReleaseQueued
    );
    assert_eq!(fx.flow.capture_queued_ns(), Some(queued_ns));
    assert!(fx.flow.state().outstanding().is_empty());
    fx.elapse(500);
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    assert!(
        fx.flow
            .take_operation_completion(&mut cx!(fx))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
        pending
    );
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    let Some(OperationCompletion::Capture {
        request,
        completion,
        recorded_ns: preserved,
    }) = fx.flow.take_operation_completion(&mut cx!(fx)).unwrap()
    else {
        panic!("the original Finished remains owned until author records");
    };
    assert!(matches!(request, StateControl::Acknowledge { .. }));
    assert!(completion.discard_capture_result);
    assert_same_answer(&completion.answer, &expected);
    assert_eq!(preserved, recorded_ns);
    fx.advance().unwrap();
    fx.transport().unwrap();
    fx.collect_requests();
    assert_eq!(fx.requests.len(), 3);
    assert_eq!(fx.requests[2].common().request_id.0, original.0 + 1);
    assert_eq!(fx.flow.capture_queued_ns(), Some(queued_ns));
    answer(
        &mut fx,
        2,
        StateControlAnswer::CaptureReleased {
            capture_id: 7.into(),
        },
    );
    assert_eq!(
        fx.flow
            .state()
            .held_measurement()
            .unwrap()
            .harness_queued_ns
            .0,
        queued_ns
    );
    let (request, completion, _) = take(&mut fx);
    assert_eq!(
        request,
        StateControl::ReleaseCapture {
            capture_id: 7.into()
        }
    );
    assert!(!completion.discard_capture_result);
    fx.flow.finish_capture(&mut cx!(fx), 7.into()).unwrap();
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn queued_release_waits_for_interrupted_finished_proof_and_keeps_enqueue_time() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    let original = begin(&mut fx);
    answer(&mut fx, 0, snapshot());
    take(&mut fx);
    ack(&mut fx);
    let expected = answer(&mut fx, 1, finished());
    let recorded_ns = interrupt_actual_capture_release(&mut fx);
    let pending = fx.fill_trace_capacity();
    let queued_ns = fx.time();
    assert!(
        fx.flow
            .enqueue_capture_release(&mut cx!(fx), &mut fx.clock, 7.into())
            .unwrap()
    );
    assert_eq!(
        fx.flow.capture_snapshot().unwrap().phase,
        CapturePhase::ReleaseQueued
    );
    assert!(
        fx.flow.capture_recorded_ns().is_none(),
        "original proof cannot end the new release wait"
    );
    fx.elapse(500);
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    assert_eq!(fx.flow.state().outstanding().len(), 1);
    assert_eq!(
        serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
        pending
    );
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    fx.advance().unwrap();
    let Some(OperationCompletion::Capture {
        completion,
        recorded_ns: preserved,
        ..
    }) = fx.flow.take_operation_completion(&mut cx!(fx)).unwrap()
    else {
        panic!("interrupted original releases only through its retained proof");
    };
    assert!(completion.discard_capture_result);
    assert_same_answer(&completion.answer, &expected);
    assert_eq!(preserved, recorded_ns);
    assert!(fx.flow.state().outstanding().is_empty());
    fx.advance().unwrap();
    fx.transport().unwrap();
    fx.collect_requests();
    assert_eq!(fx.requests[2].common().request_id.0, original.0 + 1);
    assert_eq!(fx.flow.capture_queued_ns(), Some(queued_ns));
    answer(
        &mut fx,
        2,
        StateControlAnswer::CaptureReleased {
            capture_id: 7.into(),
        },
    );
    assert_eq!(
        fx.flow
            .state()
            .held_measurement()
            .unwrap()
            .harness_queued_ns
            .0,
        queued_ns
    );
    take(&mut fx);
    fx.flow.finish_capture(&mut cx!(fx), 7.into()).unwrap();
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn recorded_acknowledgement_refusal_cannot_authorize_a_fresh_release() {
    let mut fx = Fixture::new(2);
    register(&mut fx, 0);
    begin(&mut fx);
    answer(&mut fx, 0, snapshot());
    take(&mut fx);
    ack(&mut fx);
    answer(&mut fx, 1, refused());
    assert!(
        !fx.flow
            .enqueue_capture_release(&mut cx!(fx), &mut fx.clock, 7.into())
            .unwrap()
    );
    let recorded_ns = interrupt_actual_capture_release(&mut fx);
    assert!(
        !fx.flow
            .enqueue_capture_release(&mut cx!(fx), &mut fx.clock, 7.into())
            .unwrap()
    );
    assert_eq!(
        fx.flow.capture_snapshot().unwrap().phase,
        CapturePhase::AcknowledgePending
    );
    assert_eq!(fx.flow.capture_recorded_ns(), Some(recorded_ns));
    assert_eq!(fx.flow.state().outstanding().len(), 1);
    assert!(fx.flow.failure().is_none());
}
