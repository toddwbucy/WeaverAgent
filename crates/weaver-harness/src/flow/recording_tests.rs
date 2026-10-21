use super::*;
use crate::flow::classifier::scripted::{ScriptStep, ScriptedPort};
use crate::flow::classifier::{Classifier, ClassifierPoll, ClassifierState, Submit};
use crate::flow::intake::{IntakeError, IntakeFailure};
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};
use weaver_trace::serving::{
    Channel, Envelope, ORDINARY_CAPACITY, Origin, PreflightAttempt, PreflightRecorder,
    payloads as p, validate_partition,
};
use weaver_trace::{MonotonicNs, RunRef, SessionRef, Subsystem, TurnRef};
use weaver_types::state_flow::{
    FlowBacklog, Ste100DefectCode, Ste100Request, Ste100Result, Ste100Target,
};

use crate::flow::recording_fixture as fixture;

fn event(kind: Kind, payload: Payload) -> Event {
    let (channel, origin) = match kind {
        Kind::MessageUser => (Channel::Input, Origin::Operator),
        Kind::ModelMeasurement | Kind::StateFlow | Kind::StateStoreMeasurement => {
            (Channel::Measurement, Origin::Runtime)
        }
        _ => (Channel::Internal, Origin::Runtime),
    };
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: (kind != Kind::Load).then(|| TurnRef("turn".into())),
            sequence: Sequence(999),
            kind,
            channel,
            origin,
            subsystem: Subsystem::Harness,
            causal_parent: None,
            wall_ms: 1,
            monotonic_ns: MonotonicNs(7),
        },
        payload: Some(payload),
    }
}

fn recorder() -> (Recorder, UnixStream) {
    let (sink, reader) = UnixStream::pair().unwrap();
    sink.set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let preflight = PreflightRecorder::receive(
        sink.into(),
        RunRef("run".into()),
        SessionRef("session".into()),
    )
    .unwrap();
    let attempt = preflight
        .record_load(event(
            Kind::Load,
            Payload::Elections(Box::new(fixture::load_record())),
        ))
        .unwrap_or_else(|error| panic!("load record failed: {:?}", error.failure));
    let PreflightAttempt::Recorded {
        recorder,
        submission,
    } = attempt
    else {
        panic!("an empty recorder has capacity for its load");
    };
    assert_eq!(submission.sequence, Sequence(0));
    assert_eq!(submission.tap, TapOutcome::HeadUntapped);
    (
        recorder.activate(validate_partition(&fixture::partition()).unwrap()),
        reader,
    )
}

fn budget(blocks: u64) -> RetentionBudget {
    RetentionBudget::new(FlowBacklog {
        max_pending_blocks: blocks.into(),
        max_pending_bytes: 100_000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "test-only declared backlog".into(),
    })
    .unwrap()
}

fn message(text: &str) -> Event {
    event(
        Kind::MessageUser,
        Payload::Message(
            serde_json::value::to_raw_value(
                &serde_json::json!({"role":"user","content":[{"type":"text","text":text}]}),
            )
            .unwrap(),
        ),
    )
}

fn measurement(text: &str) -> Event {
    event(
        Kind::ModelMeasurement,
        Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({"fixture":text})).unwrap(),
        ),
    )
}

fn source(sequence: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: 0.into(),
    }
}

fn terminal(id: &SourceBlockId) -> Event {
    event(
        Kind::StateFlow,
        Payload::FlowObservation(Box::new(p::FlowObservation {
            phase: p::FlowPhase::Filed,
            source_id: Some(p::SourceBlockId {
                run: id.run.clone(),
                sequence: id.sequence.0.into(),
                block: id.block.0.into(),
            }),
            input_id: None,
            request_id: None,
            pending_count: 0.into(),
            oldest_age_ns: 0.into(),
            elapsed_ns: None,
            blocked_stage: None,
            sample_start_ns: 1.into(),
            sampled_ns: 2.into(),
            pending_bytes: 0.into(),
            filed_count: 1.into(),
            filed_bytes: 0.into(),
            rejected_count: 0.into(),
            input_pending: 0.into(),
            input_started: 0.into(),
            input_completed: 0.into(),
            retained_blocks: 0.into(),
            retained_bytes: 0.into(),
            pending_turns: 0.into(),
            backlog_waiting: false,
            lag_alarm: false,
            waiting_copy: None,
            failed_corrections: None,
            retry_cap: None,
        })),
    )
}

fn reply() -> Ste100Reply {
    Ste100Reply {
        request_id: 1.into(),
        target: Ste100Target::Input {
            input_id: 1.into(),
            block: 0.into(),
        },
        scheme_id: "fixture-read/1".into(),
        result: Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "test-only conformance outcome".into(),
            spans: vec![],
        },
    }
}

fn classifier_event(reply: &Ste100Reply) -> Event {
    let Ste100Target::Input { input_id, block } = reply.target else {
        panic!("input fixture");
    };
    let Ste100Result::NotSte100 { detail, .. } = &reply.result else {
        panic!("defect fixture");
    };
    let mirror = p::Ste100Reply {
        request_id: reply.request_id.0.into(),
        target: p::Ste100Target::Input {
            input_id: input_id.0.into(),
            block: block.0.into(),
        },
        scheme_id: reply.scheme_id.clone(),
        result: p::Ste100Result::NotSte100 {
            code: p::Ste100DefectCode::NonSte100,
            detail: detail.clone(),
            spans: vec![],
        },
    };
    assert_eq!(
        serde_json::to_value(&mirror).unwrap(),
        serde_json::to_value(reply).unwrap()
    );
    event(Kind::Ste100Result, Payload::Ste100Result(Box::new(mirror)))
}

fn recorded(progress: RecordProgress) -> (Sequence, Option<RecordProof>) {
    match progress {
        RecordProgress::Recorded { sequence, proof } => (sequence, proof),
        other => panic!("expected real Recorded admission, got {other:?}"),
    }
}

#[test]
fn real_tap_queues_exact_copy_and_only_recorded_terminal_releases_it() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(2);
    let (sequence, proof) = recorded(barrier.submit(
        message("Read caf\u{e9}.\n"),
        RecordPurpose::Content,
        &mut inbox,
        &mut budget,
    ));
    assert_eq!(sequence, Sequence(1));
    assert!(proof.is_none());
    let copy = inbox.pop_front(&budget).unwrap();
    let weaver_trace::serving::AdmittedCopy::Statement(copy) = copy else {
        panic!("statement");
    };
    assert_eq!(copy.blocks.len(), 1);
    assert_eq!(copy.blocks[0].source.sequence, sequence);
    assert_eq!(copy.blocks[0].source.run, "run");
    assert_eq!(copy.blocks[0].source.block, 0);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&copy.blocks[0].raw).unwrap()["text"],
        "Read caf\u{e9}.\n"
    );
    let charged = budget.gauges();
    assert_eq!(charged.0, 1);
    assert!(charged.1 > 0);
    let id = source(1);
    let (_, proof) = recorded(barrier.submit(
        terminal(&id),
        RecordPurpose::Terminal(id),
        &mut inbox,
        &mut budget,
    ));
    assert_eq!(
        budget.gauges(),
        charged,
        "recording grants a proof but does not release intake itself"
    );
    let Some(RecordProof::Terminal(proof)) = proof else {
        panic!("terminal proof");
    };
    budget.release(proof).unwrap();
    assert_eq!(budget.gauges(), (0, 0));
    assert_eq!(barrier.recorder().structure().len(), 3);
    assert!(matches!(
        barrier.resume(&mut inbox, &mut budget),
        RecordProgress::Failed
    ));
    assert_eq!(
        barrier.recorder().structure().len(),
        3,
        "resume cannot duplicate a terminal"
    );
}

#[test]
fn copied_intake_uses_one_raw_clock_read_and_excluded_observations_use_none() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let mut reads = 0;
    recorded(barrier.submit_with_clock(
        message("Read file."),
        RecordPurpose::Content,
        &mut inbox,
        &mut budget,
        &mut || {
            reads += 1;
            Ok(1_234_567)
        },
    ));
    assert_eq!(reads, 1);
    let (copy, intake_ns) = inbox.pop_timed(&budget).unwrap();
    assert_eq!(intake_ns, Some(1_234_567));
    let weaver_trace::serving::AdmittedCopy::Statement(copy) = copy else {
        panic!("statement");
    };
    assert_eq!(copy.blocks[0].source.sequence, Sequence(1));
    recorded(barrier.submit_with_clock(
        measurement("excluded"),
        RecordPurpose::Observation,
        &mut inbox,
        &mut budget,
        &mut || panic!("excluded observations have no copy intake clock"),
    ));
    assert_eq!(inbox.queued().count(), 0);
}

#[test]
fn failed_intake_clock_preserves_the_whole_recorded_copy_and_stops_admission() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    assert!(matches!(
        barrier.submit_with_clock(
            message("Read file."),
            RecordPurpose::Content,
            &mut inbox,
            &mut budget,
            &mut || Err(std::io::Error::other("test clock failure")),
        ),
        RecordProgress::Failed
    ));
    assert_eq!(barrier.recorder().structure().len(), 2);
    assert!(budget.is_frozen());
    assert!(inbox.pop_timed(&budget).is_none());
    let failure = inbox.failure().expect("whole failed copy remains owned");
    assert_eq!(failure.detail, IntakeError::ClockReadFailed);
    let weaver_trace::serving::AdmittedCopy::Statement(copy) = &failure.copy else {
        panic!("statement");
    };
    assert_eq!(copy.blocks.len(), 1);
    assert_eq!(copy.blocks[0].source.sequence, Sequence(1));
    assert!(matches!(
        barrier.submit_with_clock(
            message("Read later."),
            RecordPurpose::Content,
            &mut inbox,
            &mut budget,
            &mut || panic!("failed flow cannot take another copy"),
        ),
        RecordProgress::Busy { .. }
    ));
    assert_eq!(barrier.recorder().structure().len(), 2);
}

#[test]
fn waiting_copy_blocks_content_before_admission_but_excluded_measurements_progress() {
    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    recorded(barrier.submit(
        message("Read first."),
        RecordPurpose::Content,
        &mut inbox,
        &mut budget,
    ));
    recorded(barrier.submit(
        message("Read second."),
        RecordPurpose::Content,
        &mut inbox,
        &mut budget,
    ));
    assert!(budget.waiting_copy().is_some());
    assert_eq!(inbox.queued().count(), 1);
    let next = message("Read third.");
    let expected = serde_json::to_string(&next).unwrap();
    match barrier.submit(next, RecordPurpose::Content, &mut inbox, &mut budget) {
        RecordProgress::Busy {
            event,
            purpose: RecordPurpose::Content,
        } => assert_eq!(serde_json::to_string(&event).unwrap(), expected),
        other => panic!("content must remain outside trace admission: {other:?}"),
    }
    assert_eq!(barrier.recorder().structure().len(), 3);
    let (sequence, proof) = recorded(barrier.submit(
        measurement("still observable"),
        RecordPurpose::Observation,
        &mut inbox,
        &mut budget,
    ));
    assert_eq!(sequence, Sequence(3));
    assert!(proof.is_none());
    let id = source(1);
    let (_, proof) = recorded(barrier.submit(
        terminal(&id),
        RecordPurpose::Terminal(id),
        &mut inbox,
        &mut budget,
    ));
    let Some(RecordProof::Terminal(proof)) = proof else {
        panic!("terminal proof");
    };
    budget.release(proof).unwrap();
    assert!(matches!(
        inbox.resume_waiting(&mut budget),
        Some(IntakeOutcome::Accepted)
    ));
    assert!(budget.waiting_copy().is_none());
    assert_eq!(inbox.queued().count(), 2);
}

#[test]
fn reply_and_run_mismatch_refuse_before_trace_and_preserve_pending_author() {
    for mismatch in ["reply", "run", "terminal", "terminal phase"] {
        let (recorder, _reader) = recorder();
        let mut barrier = RecordBarrier::new(recorder, "run".into());
        let mut inbox = FlowInbox::new();
        let mut budget = budget(2);
        let expected = reply();
        let (mut event, purpose) = if mismatch.starts_with("terminal") {
            let mut event = terminal(&source(1));
            if mismatch == "terminal phase" {
                let Some(Payload::FlowObservation(record)) = &mut event.payload else {
                    unreachable!()
                };
                record.phase = p::FlowPhase::Sampled;
            }
            (
                event,
                RecordPurpose::Terminal(source(if mismatch == "terminal" { 2 } else { 1 })),
            )
        } else {
            let mut authored = expected.clone();
            if mismatch == "reply" {
                authored.request_id = 2.into();
            }
            (
                classifier_event(&authored),
                RecordPurpose::Classifier(expected),
            )
        };
        if mismatch == "run" {
            event.envelope.run = RunRef("another run".into());
        }
        let authored = serde_json::to_string(&event).unwrap();
        assert!(
            matches!(
                barrier.submit(event, purpose, &mut inbox, &mut budget),
                RecordProgress::Failed
            ),
            "{mismatch}"
        );
        assert!(
            matches!(barrier.failure(), Some(RecordFailure::Protocol(_))),
            "{mismatch}"
        );
        assert_eq!(
            serde_json::to_string(barrier.pending().unwrap()).unwrap(),
            authored
        );
        assert_eq!(barrier.recorder().structure().len(), 1, "{mismatch}");
        assert!(budget.is_frozen());
    }
}

#[test]
fn first_failed_intake_keeps_the_whole_copy_and_prevents_a_second_admission() {
    let (recorder_a, _reader_a) = recorder();
    let (recorder_b, _reader_b) = recorder();
    let mut first = RecordBarrier::new(recorder_a, "run".into());
    let mut replay = RecordBarrier::new(recorder_b, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(3);
    recorded(first.submit(
        message("Read first."),
        RecordPurpose::Content,
        &mut inbox,
        &mut budget,
    ));
    // Two real recorders intentionally reuse a run/sequence to exercise the
    // intake identity guard. A serving coordinator must never create this pair.
    assert!(matches!(
        replay.submit(
            message("Read conflicting replay."),
            RecordPurpose::Content,
            &mut inbox,
            &mut budget
        ),
        RecordProgress::Failed
    ));
    assert!(matches!(
        replay.failure(),
        Some(RecordFailure::Intake(IntakeError::DuplicateReservation))
    ));
    let failure = inbox.failure().unwrap();
    let held = failure.copy.clone();
    assert_eq!(failure.detail, IntakeError::DuplicateReservation);
    assert!(budget.is_frozen());
    let weaver_trace::serving::AdmittedCopy::Statement(copy) = &held else {
        panic!("statement");
    };
    assert!(copy.blocks[0].raw.contains("conflicting replay"));
    assert!(matches!(
        replay.submit(
            message("Read later."),
            RecordPurpose::Content,
            &mut inbox,
            &mut budget
        ),
        RecordProgress::Busy { .. }
    ));
    assert!(matches!(
        first.submit(
            measurement("another owner"),
            RecordPurpose::Observation,
            &mut inbox,
            &mut budget
        ),
        RecordProgress::Busy { .. }
    ));
    assert_eq!(replay.recorder().structure().len(), 2);
    assert_eq!(first.recorder().structure().len(), 2);
    assert_eq!(inbox.failure().unwrap().copy, held);
}

fn fill_capacity(recorder: &mut Recorder) {
    let payload = "x".repeat(32_768);
    for _ in 0..ORDINARY_CAPACITY + 64 {
        match recorder.submit(measurement(&payload)).unwrap() {
            SubmitAttempt::Recorded(_) => {}
            SubmitAttempt::CapacityPending(_) => return,
        }
    }
    panic!("bounded socket must force ordinary queue capacity pressure");
}

fn drain(reader: UnixStream) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .expect("bounded trace socket drain");
        bytes
    })
}

#[test]
fn classifier_completion_waits_for_real_capacity_then_releases_exactly_once() {
    let expected = reply();
    let request = Ste100Request {
        request_id: expected.request_id,
        target: expected.target.clone(),
        text: "This fixture asks for an invalid sentence.".into(),
        scheme_id: expected.scheme_id.clone(),
    };
    let (port, mut peer) = ScriptedPort::fixture([ScriptStep::Reply(expected.clone())]).unwrap();
    let mut classifier = Classifier::fixture(port, "run".into());
    assert_eq!(classifier.submit(request).unwrap(), Submit::Submitted);
    assert!(peer.advance().unwrap());
    assert_eq!(classifier.try_result().unwrap(), ClassifierPoll::ResultHeld);
    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    let count = recorder.structure().len();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    assert!(matches!(
        barrier.submit(
            classifier_event(&expected),
            RecordPurpose::Classifier(expected.clone()),
            &mut inbox,
            &mut budget
        ),
        RecordProgress::Pending
    ));
    assert_eq!(classifier.state(), ClassifierState::ResultHeld);
    assert_eq!(classifier.held(), Some(&expected));
    assert_eq!(barrier.recorder().structure().len(), count);
    let reader = drain(reader);
    let deadline = Instant::now() + Duration::from_secs(8);
    let proof = loop {
        match barrier.resume(&mut inbox, &mut budget) {
            RecordProgress::Pending => {
                assert!(
                    Instant::now() < deadline,
                    "trace capacity did not wake before test bound"
                );
                std::thread::yield_now();
            }
            RecordProgress::Recorded {
                sequence,
                proof: Some(RecordProof::Classifier(proof)),
            } => {
                assert_eq!(sequence.0, count as u64);
                assert_eq!(
                    proof.source(),
                    &SourcePosition {
                        block_id: SourceBlockId {
                            run: "run".into(),
                            sequence: sequence.0.into(),
                            block: 0.into(),
                        },
                        turn: Some("turn".into()),
                    },
                    "proof must use assigned trace identity, never input sequence 999",
                );
                break proof;
            }
            other => panic!("classifier record failed: {other:?}"),
        }
    };
    assert_eq!(classifier.state(), ClassifierState::ResultHeld);
    assert_eq!(classifier.recorded(proof).unwrap(), Some(expected));
    assert_eq!(classifier.state(), ClassifierState::Idle);
    assert!(classifier.held().is_none());
    assert!(matches!(
        barrier.resume(&mut inbox, &mut budget),
        RecordProgress::Failed
    ));
    assert_eq!(barrier.recorder().structure().len(), count + 1);
    drop(barrier);
    let bytes = reader.join().unwrap();
    let lines = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .filter(|line| line.contains("\"kind\":\"ste100.result\""))
        .count();
    assert_eq!(
        lines, 1,
        "exactly one actual classifier result reached the trace socket"
    );
}

#[test]
fn intake_failure_arriving_during_capacity_wait_prevents_pending_admission() {
    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    let count = recorder.structure().len();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let pending = message("Read pending.");
    let exact = serde_json::to_string(&pending).unwrap();
    assert!(matches!(
        barrier.submit(pending, RecordPurpose::Content, &mut inbox, &mut budget),
        RecordProgress::Pending
    ));
    let (mut source_recorder, _source_reader) = self::recorder();
    let SubmitAttempt::Recorded(submission) = source_recorder
        .submit(message("Read first failure."))
        .unwrap()
    else {
        panic!("available capacity");
    };
    let TapOutcome::Selected(copy) = submission.tap else {
        panic!("real admitted copy");
    };
    inbox
        .preserve_failure(IntakeFailure {
            copy,
            detail: IntakeError::DuplicateReservation,
        })
        .unwrap();
    let reader = drain(reader);
    assert!(matches!(
        barrier.resume(&mut inbox, &mut budget),
        RecordProgress::Failed
    ));
    assert!(matches!(
        barrier.failure(),
        Some(RecordFailure::Intake(IntakeError::DuplicateReservation))
    ));
    assert_eq!(barrier.recorder().structure().len(), count);
    assert_eq!(
        serde_json::to_string(barrier.pending().unwrap()).unwrap(),
        exact
    );
    assert!(budget.is_frozen());
    drop(barrier);
    reader.join().unwrap();
}

#[test]
fn second_author_during_capacity_wait_fails_and_preserves_both_exact_events() {
    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    let count = recorder.structure().len();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let first = message("Read first pending author.");
    let first_exact = serde_json::to_string(&first).unwrap();
    assert!(matches!(
        barrier.submit(first, RecordPurpose::Content, &mut inbox, &mut budget),
        RecordProgress::Pending
    ));
    let second = message("Read second author.");
    let second_exact = serde_json::to_string(&second).unwrap();
    match barrier.submit(second, RecordPurpose::Content, &mut inbox, &mut budget) {
        RecordProgress::Busy {
            event,
            purpose: RecordPurpose::Content,
        } => assert_eq!(serde_json::to_string(&event).unwrap(), second_exact),
        other => panic!("second author ownership must return on failure: {other:?}"),
    }
    assert!(matches!(
        barrier.failure(),
        Some(RecordFailure::Protocol(_))
    ));
    assert!(budget.is_frozen());
    assert_eq!(
        serde_json::to_string(barrier.pending().unwrap()).unwrap(),
        first_exact
    );
    assert_eq!(barrier.recorder().structure().len(), count);
    assert_eq!(budget.gauges(), (0, 0));
    assert_eq!(inbox.queued().count(), 0);
    let reader = drain(reader);
    assert!(matches!(
        barrier.resume(&mut inbox, &mut budget),
        RecordProgress::Failed
    ));
    assert_eq!(barrier.recorder().structure().len(), count);
    assert_eq!(
        serde_json::to_string(barrier.pending().unwrap()).unwrap(),
        first_exact
    );
    drop(barrier);
    let bytes = reader.join().unwrap();
    let trace = std::str::from_utf8(&bytes).unwrap();
    assert!(!trace.contains("first pending author"));
    assert!(!trace.contains("second author"));
}

#[test]
fn real_classifier_proofs_cannot_release_a_different_reply_or_run() {
    for wrong_run in [false, true] {
        let expected = reply();
        let request = Ste100Request {
            request_id: expected.request_id,
            target: expected.target.clone(),
            text: "This is a test-only language defect.".into(),
            scheme_id: expected.scheme_id.clone(),
        };
        let (port, mut peer) =
            ScriptedPort::fixture([ScriptStep::Reply(expected.clone())]).unwrap();
        let mut classifier =
            Classifier::fixture(port, if wrong_run { "another run" } else { "run" }.into());
        assert_eq!(classifier.submit(request).unwrap(), Submit::Submitted);
        assert!(peer.advance().unwrap());
        assert_eq!(classifier.try_result().unwrap(), ClassifierPoll::ResultHeld);
        let (recorder, _reader) = recorder();
        let mut barrier = RecordBarrier::new(recorder, "run".into());
        let mut inbox = FlowInbox::new();
        let mut budget = budget(1);
        let mut authored = expected.clone();
        if !wrong_run {
            authored.request_id = 2.into();
        }
        let (_, proof) = recorded(barrier.submit(
            classifier_event(&authored),
            RecordPurpose::Classifier(authored),
            &mut inbox,
            &mut budget,
        ));
        let Some(RecordProof::Classifier(proof)) = proof else {
            panic!("actual recorded classifier proof");
        };
        assert!(classifier.recorded(proof).is_err());
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.held(), Some(&expected));
    }
}

fn open_measurement_mirror() -> p::StateStoreMeasurement {
    p::StateStoreMeasurement {
        observation: p::StateOperationObservation {
            operation_id: 1.into(),
            parent_id: None,
            request_id: 1.into(),
            operation: p::StateOperation::Open,
            began_ns: 21.into(),
            ended_ns: 22.into(),
            phases: vec![
                p::OperationPhase {
                    name: p::PhaseName::Validate,
                    began_ns: 21.into(),
                    ended_ns: 21.into(),
                },
                p::OperationPhase {
                    name: p::PhaseName::Encode,
                    began_ns: 22.into(),
                    ended_ns: 22.into(),
                },
            ],
            resource: p::OperationResource {
                cpu_ns: None,
                copied_bytes: Some(0.into()),
                held_bytes: Some(1.into()),
                unavailable: vec![p::ResourceUnavailable {
                    field: p::ResourceField::CpuNs,
                    reason: p::ObservationUnavailable::NotSampled,
                }],
            },
            disk: p::OperationDisk {
                enabled: None,
                before: None,
                after: None,
                unavailable_reason: Some(p::ObservationUnavailable::NotSampled),
            },
        },
        request_source: None,
        harness_queued_ns: 10.into(),
        harness_sent_ns: 20.into(),
        harness_received_ns: 30.into(),
    }
}

#[test]
fn state_socket_completion_waits_for_real_recorded_measurement_and_releases_once() {
    use crate::flow::channel::{StateChannel, StateRequest};
    use std::io::{BufRead, BufReader, Write};
    use weaver_types::state_flow as s;

    let (socket, mut peer) = UnixStream::pair().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    peer.set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut channel = StateChannel::new(socket, "session".into(), "run".into()).unwrap();
    let load = fixture::load_record();
    channel
        .begin(
            StateRequest::Open {
                schema_id: s::STATE_CONTENT_SCHEMA.into(),
                scheme_id: "test-only scheme".into(),
                start: serde_json::from_value(serde_json::to_value(load.state_start).unwrap())
                    .unwrap(),
                read_limits: s::ReadLimits {
                    max_result_bytes: 10000.into(),
                    measurement_ref: "test-only read limit".into(),
                },
            },
            10,
        )
        .unwrap();
    channel.progress(&mut || Ok(20)).unwrap();
    let mut line = String::new();
    BufReader::new(&mut peer).read_line(&mut line).unwrap();
    let s::StateFrame::Open {
        common,
        schema_id,
        scheme_id,
        start,
        read_limits,
    } = s::decode_state_line(line.as_bytes(), s::FrameDirection::Request).unwrap()
    else {
        panic!("actual open frame");
    };
    let mirror = open_measurement_mirror();
    let answer = s::StateFrame::Opened {
        common,
        schema_id,
        scheme_id,
        start,
        read_limits,
        observation: serde_json::from_value(serde_json::to_value(&mirror.observation).unwrap())
            .unwrap(),
        member_observation: s::MemberObservationIdentity {
            instrument_version: "state-observation/1".into(),
            pid: 7,
            start_ticks: Some(10.into()),
            ticks_per_second: Some(100.into()),
            unavailable_reason: None,
        },
    };
    peer.write_all(&s::encode_state_line(&answer, s::FrameDirection::Response).unwrap())
        .unwrap();
    channel.progress(&mut || Ok(30)).unwrap();
    let held = channel.held_measurement().unwrap().clone();
    assert_eq!(
        serde_json::to_value(&held).unwrap(),
        serde_json::to_value(&mirror).unwrap()
    );
    assert_eq!(channel.outstanding().len(), 1);
    assert!(!channel.interest().readable);
    channel.progress(&mut || Ok(40)).unwrap();
    assert_eq!(channel.held_measurement(), Some(&held));

    let (recorder, _reader) = recorder();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget(1);
    let (_, proof) = recorded(barrier.submit(
        event(
            Kind::StateStoreMeasurement,
            Payload::StateStoreMeasurement(Box::new(mirror.clone())),
        ),
        RecordPurpose::State(held.clone()),
        &mut inbox,
        &mut budget,
    ));
    assert_eq!(
        channel.held_measurement(),
        Some(&held),
        "trace admission alone cannot consume the state answer"
    );
    let Some(RecordProof::State(proof)) = proof else {
        panic!("actual state measurement proof");
    };
    let completion = channel
        .release(proof)
        .unwrap()
        .expect("recorded completion");
    assert_eq!(
        s::encode_state_line(&completion.answer, s::FrameDirection::Response).unwrap(),
        s::encode_state_line(&answer, s::FrameDirection::Response).unwrap()
    );
    assert!(!completion.discard_capture_result);
    assert!(channel.held_measurement().is_none());
    assert!(channel.outstanding().is_empty());
    assert_eq!(
        budget.gauges(),
        (0, 0),
        "measurement payloads never become content"
    );
    assert_eq!(inbox.queued().count(), 0);

    // Even another real trace admission cannot release the consumed answer again.
    let (_, proof) = recorded(barrier.submit(
        event(
            Kind::StateStoreMeasurement,
            Payload::StateStoreMeasurement(Box::new(mirror)),
        ),
        RecordPurpose::State(held),
        &mut inbox,
        &mut budget,
    ));
    let Some(RecordProof::State(proof)) = proof else {
        panic!("actual second proof");
    };
    assert!(channel.release(proof).is_err());
}

#[path = "recording_owner_tests.rs"]
mod owner_tests;
