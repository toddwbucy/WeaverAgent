use super::super::payloads::{self, PersistenceCause, SavePointRecord, SaveWindow};
use super::super::{AdmittedCopy, Channel, Envelope, Origin, validate_partition};
use super::*;
use crate::{MonotonicNs, Subsystem, TurnRef};
use serde_json::value::RawValue;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::time::Duration;

// Assertion unwinding still drops and joins the writer. Bound the actual socket
// I/O so a deliberately broken terminal permit cannot leave that join blocked
// behind a reader which the failing test has not started yet.
fn bounded_pair() -> (UnixStream, UnixStream) {
    let (sink, reader) = UnixStream::pair().unwrap();
    sink.set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    (sink, reader)
}

fn drained(recorder: &mut Recorder) {
    assert_eq!(
        recorder
            .drain_by(Instant::now() + Duration::from_secs(3))
            .unwrap(),
        Drained::Emptied
    );
}

fn raw(text: &str) -> Box<RawValue> {
    RawValue::from_string(text.into()).unwrap()
}
fn event(
    kind: Kind,
    channel: Channel,
    origin: Origin,
    turn: bool,
    payload: Option<Payload>,
) -> Event {
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: turn.then(|| TurnRef("turn".into())),
            sequence: Sequence(999),
            kind,
            channel,
            origin,
            subsystem: Subsystem::Harness,
            causal_parent: None,
            wall_ms: 42,
            monotonic_ns: MonotonicNs(9007199254740993),
        },
        payload,
    }
}
fn load() -> Event {
    event(
        Kind::Load,
        Channel::Internal,
        Origin::Runtime,
        false,
        Some(Payload::Elections(Box::new(payloads::tests::load_record()))),
    )
}
fn message(text: &str) -> Event {
    event(
        Kind::MessageUser,
        Channel::Input,
        Origin::Operator,
        true,
        Some(Payload::Message(raw(&format!(
            r#"{{"role":"user","content":[{{"type":"text","text":{}}}]}}"#,
            serde_json::to_string(text).unwrap()
        )))),
    )
}
fn fault() -> Event {
    event(
        Kind::Fault,
        Channel::Internal,
        Origin::Runtime,
        false,
        Some(Payload::Fault(raw(
            r#"{"case":"state_flow_failed","account":{"first":"copy_failed"}}"#,
        ))),
    )
}
fn loaded() -> (PreflightRecorder<Loaded>, UnixStream) {
    let (sink, reader) = bounded_pair();
    let recorder = PreflightRecorder::receive(
        sink.into(),
        RunRef("run".into()),
        SessionRef("session".into()),
    )
    .unwrap();
    match recorder
        .record_load(load())
        .unwrap_or_else(|failed| panic!("{:?}", failed.failure))
    {
        PreflightAttempt::Recorded {
            recorder,
            submission,
        } => {
            assert_eq!(submission.sequence, Sequence(0));
            assert_eq!(submission.tap, TapOutcome::HeadUntapped);
            (recorder, reader)
        }
        _ => panic!("empty queue refused load"),
    }
}
fn recorder() -> (Recorder, UnixStream) {
    let (preflight, reader) = loaded();
    (
        preflight.activate(validate_partition(&super::super::tests::partition()).unwrap()),
        reader,
    )
}
fn recorded(result: Result<SubmitAttempt, Failure>) -> Submission {
    match result.unwrap() {
        SubmitAttempt::Recorded(record) => record,
        SubmitAttempt::CapacityPending(_) => panic!("unexpected pending"),
    }
}
fn bytes(mut reader: UnixStream) -> String {
    let mut bytes = String::new();
    reader.read_to_string(&mut bytes).unwrap();
    bytes
}
fn fixture(kind: Kind) -> Event {
    let (k, c, o, t, p) = payloads::tests::payload_fixtures()
        .into_iter()
        .find(|x| x.0 == kind)
        .unwrap();
    event(k, c, o, t, Some(p))
}

fn rejected_return_line(payload: &str) -> String {
    let (mut recorder, reader) = recorder();
    let sink = std::thread::spawn(move || bytes(reader));
    let mut source = event(
        Kind::ToolReturnRejected,
        Channel::Tool,
        Origin::Tool,
        true,
        Some(Payload::RejectedReturn(Box::new(
            payloads::RejectedReturn {
                call_id: None,
                execution: payloads::ExchangeId {
                    opener: payloads::Opener::Harness,
                    ordinal: 97,
                },
                reason: "unexpected_exchange".into(),
                raw_envelope: payload.to_owned(),
            },
        ))),
    );
    source.envelope.subsystem = Subsystem::Gate;
    let submitted = recorded(recorder.submit(source));
    assert_eq!(submitted.sequence, Sequence(1));
    assert_eq!(submitted.tap, TapOutcome::Unselected);
    let line = recorder.structure().iter().last().unwrap().line.to_string();
    assert!(line.ends_with('\n'));
    assert_eq!(line.bytes().filter(|b| *b == b'\n').count(), 1);
    assert!(!line.contains('\r'));
    let decoded: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        decoded["payload"]["raw_envelope"]
            .as_str()
            .unwrap()
            .as_bytes(),
        payload.as_bytes()
    );
    drained(&mut recorder);
    drop(recorder);
    let written = sink.join().unwrap();
    assert_eq!(written.lines().count(), 2);
    assert!(written.ends_with(&line));
    line
}

#[test]
fn rejected_return_string_records_exact_multiline_payload_in_one_line() {
    let expected = include_str!("../../tests/fixtures/rejected-return-string.jsonl");
    let fixture: serde_json::Value = serde_json::from_str(expected).unwrap();
    let payload = fixture["payload"]["raw_envelope"].as_str().unwrap();
    assert_eq!(rejected_return_line(payload), expected);

    let historical = include_str!("../../tests/fixtures/rejected-return-historical-object.jsonl");
    let old: serde_json::Value = serde_json::from_str(historical).unwrap();
    assert!(old["payload"]["raw_envelope"].is_object());
    assert_eq!(
        old["payload"]["raw_envelope"],
        serde_json::from_str::<serde_json::Value>(payload).unwrap()
    );
    // Compatibility corpus for readers; the new publisher emits only String.
    // An old object establishes JSON content, not original packet spelling.
    assert_ne!(
        serde_json::to_vec(&old["payload"]["raw_envelope"]).unwrap(),
        payload.as_bytes()
    );
}

#[test]
fn rejected_return_escaped_line_can_exceed_the_64_kib_packet_bound() {
    // The trace crate deliberately has no weaver-types dependency. The harness
    // companion test admits exactly its actual MAX_ENVELOPE_BYTES over a socket.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/rejected-return-string.jsonl"
    ))
    .unwrap();
    let mut payload = fixture["payload"]["raw_envelope"]
        .as_str()
        .unwrap()
        .to_owned();
    payload.extend(
        ['\r', '\n', '\t', ' ']
            .into_iter()
            .cycle()
            .take(64 * 1024 - payload.len()),
    );
    assert_eq!(payload.len(), 64 * 1024);
    serde_json::from_str::<serde_json::Value>(&payload).unwrap();
    let began = Instant::now();
    let line = rejected_return_line(&payload);
    assert!(line.len() > payload.len());
    eprintln!(
        "rejected-return sample: payload_bytes={} trace_line_bytes={} elapsed_ns={}",
        payload.len(),
        line.len(),
        began.elapsed().as_nanos()
    );
}

#[test]
fn canonical_record_is_shared_by_working_structure_tap_and_sink() {
    let (mut recorder, reader) = recorder();
    let source = message("Read file.\nKeep line two.");
    let submission = recorded(recorder.submit(source));
    assert_eq!(submission.sequence, Sequence(1));
    let TapOutcome::Selected(AdmittedCopy::Statement(copy)) = submission.tap else {
        panic!("must copy")
    };
    assert_eq!(copy.blocks[0].source.sequence, Sequence(1));
    assert_eq!(copy.blocks[0].source.run, "run");
    assert_eq!(
        copy.blocks[0].raw.as_ref(),
        r#"{"type":"text","text":"Read file.\nKeep line two."}"#
    );
    let held: Vec<_> = recorder
        .structure()
        .iter()
        .map(|record| record.line.clone())
        .collect();
    assert_eq!(
        recorder
            .structure()
            .in_turn(&TurnRef("turn".into()))
            .count(),
        1
    );
    assert_eq!(recorder.structure().by_kind(Kind::Load).count(), 1);
    assert!(held[1].starts_with(r#"{"session":"session","run":"run","turn":"turn","sequence":"1","kind":"message.user","channel":"input","origin":"operator","subsystem":"harness","wall_ms":42,"monotonic_ns":"9007199254740993","payload":"#));
    assert_eq!(held[1].bytes().filter(|&b| b == b'\n').count(), 1);
    drained(&mut recorder);
    assert_eq!(recorder.boundary().committed, Some(Sequence(1)));
    drop(recorder);
    assert_eq!(
        bytes(reader),
        held.iter().map(|x| x.as_ref()).collect::<String>()
    );
}

#[test]
fn recall_admission_preserves_sequence_and_records_without_a_state_copy() {
    use super::super::payloads::RecallRecord;
    use super::super::recall_tests::{boot, event as recall_event, read};

    let (mut recorder, reader) = recorder();
    let mut bad_read = read();
    let RecallRecord::Read { snapshot_id, .. } = &mut bad_read else {
        unreachable!()
    };
    *snapshot_id = 0.into();
    let mut bad_boot = boot();
    let RecallRecord::Boot { end_mark, .. } = &mut bad_boot else {
        unreachable!()
    };
    end_mark.end = 2.into();
    for invalid in [bad_read, bad_boot] {
        assert!(matches!(
            recorder.submit(recall_event(invalid)),
            Err(Failure::RefusedOnSubmit {
                reason: crate::SubmitRefusal::PayloadMalformed
            })
        ));
        assert_eq!(recorder.structure().len(), 1);
        assert!(!recorder.tap_stopped());
    }
    for (index, record) in [read(), boot()].into_iter().enumerate() {
        let mut source = recall_event(record);
        let submitted = recorded(recorder.submit(source.clone()));
        source.envelope.sequence = Sequence(index as u64 + 1);
        assert_eq!(submitted.sequence, source.envelope.sequence);
        assert_eq!(submitted.tap, TapOutcome::Unselected);
        let expected = serde_json::to_string(&source).unwrap() + "\n";
        assert_eq!(
            recorder.structure().iter().last().unwrap().line.as_ref(),
            expected
        );
    }
    assert_eq!(recorder.structure().by_kind(Kind::Recall).count(), 2);
    let expected: String = recorder
        .structure()
        .iter()
        .map(|record| record.line.as_ref())
        .collect();
    drained(&mut recorder);
    assert_eq!(recorder.boundary().committed, Some(Sequence(2)));
    drop(recorder);
    assert_eq!(bytes(reader), expected);
}

#[test]
fn boot_open_evidence_stays_in_the_canonical_line_without_a_copy_or_terminal_permit() {
    use super::super::recall_tests::{boot, event as recall_event};

    for stopped in [false, true] {
        let (mut recorder, reader) = recorder();
        let head = recorded(recorder.submit(recall_event(boot())));
        let mut placement = fixture(Kind::StatePlacement);
        // Author sequence values are placeholders, not trusted causal order.
        placement.envelope.sequence = Sequence(0);
        placement.envelope.causal_parent = Some(head.sequence);
        let Some(Payload::Placement(record)) = &mut placement.payload else {
            unreachable!()
        };
        let request = r#"{ "rendered":"head\n\u00e9", "template":"family" }"#;
        let measurement = r#"{"opaque":[1,2], "opaque":{"future":true}}"#;
        record.open_evidence = Some(payloads::OpenEvidenceRecord {
            request: raw(request),
            measurement: raw(measurement),
        });
        if stopped {
            recorder.stop_tap();
        }
        // The trace-only teardown account uses this same ordinary road.
        let submission = recorded(recorder.submit(placement.clone()));
        assert_eq!(submission.sequence, Sequence(2));
        assert_eq!(
            submission.tap,
            if stopped {
                TapOutcome::Stopped
            } else {
                TapOutcome::Unselected
            }
        );
        let line = recorder.structure().iter().last().unwrap().line.clone();
        assert!(line.contains(&format!(
            "\"open_evidence\":{{\"request\":{request},\"measurement\":{measurement}}}"
        )));
        assert!(line.contains(r#""causal_parent":"1""#));
        assert_eq!(line.bytes().filter(|&byte| byte == b'\n').count(), 1);
        assert!(line.ends_with('\n'));
        assert_eq!(recorder.structure().by_kind(Kind::ModelRequest).count(), 0);
        assert_eq!(
            recorder.structure().by_kind(Kind::ModelMeasurement).count(),
            0
        );

        recorder.stop_tap();
        assert!(recorder.submit_terminal(placement).is_err());
        assert_eq!(recorder.structure().len(), 3);
        let expected: String = recorder
            .structure()
            .iter()
            .map(|record| record.line.as_ref())
            .collect();
        drained(&mut recorder);
        drop(recorder);
        assert_eq!(bytes(reader), expected);
    }
}

#[test]
fn boot_open_evidence_requires_a_parent_before_the_assigned_sequence() {
    use super::super::recall_tests::{boot, event as recall_event};

    let (mut recorder, reader) = recorder();
    let head = recorded(recorder.submit(recall_event(boot())));
    for parent in [None, Some(Sequence(2)), Some(Sequence(u64::MAX))] {
        let mut placement = fixture(Kind::StatePlacement);
        placement.envelope.causal_parent = parent;
        assert!(matches!(
            recorder.submit(placement),
            Err(Failure::RefusedOnSubmit {
                reason: crate::SubmitRefusal::PayloadMalformed
            })
        ));
        assert_eq!(recorder.structure().len(), 2);
    }
    let mut placement = fixture(Kind::StatePlacement);
    placement.envelope.causal_parent = Some(head.sequence);
    assert_eq!(recorded(recorder.submit(placement)).sequence, Sequence(2));
    drained(&mut recorder);
    drop(recorder);
    assert_eq!(bytes(reader).lines().count(), 3);
}

#[test]
fn tap_failure_records_the_complete_source_then_stops_without_losing_fault() {
    let (mut recorder, reader) = recorder();
    let mut bad = message("Read file.");
    bad.payload = Some(Payload::Message(raw(r#"{"role":"user","content":"bad"}"#)));
    let submission = recorded(recorder.submit(bad));
    assert_eq!(submission.sequence, Sequence(1));
    assert!(matches!(submission.tap, TapOutcome::Failed(_)));
    assert!(recorder.tap_stopped());
    assert_eq!(recorder.structure().len(), 2);
    let report = recorder.submit_terminal(fault()).unwrap();
    assert_eq!(report.tap, TapOutcome::Stopped);
    assert_eq!(report.sequence, Sequence(2));
    assert!(recorder.submit_terminal(fault()).is_err());
    let another = recorded(recorder.submit(message("Keep fault.")));
    assert_eq!(another.tap, TapOutcome::Stopped);
    drained(&mut recorder);
    drop(recorder);
    let emitted = bytes(reader);
    let lines: Vec<_> = emitted.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[1].contains(r#""content":"bad""#));
    assert!(lines[2].contains(r#""case":"state_flow_failed""#));
}

#[test]
fn refusal_after_load_never_activates_and_preserves_trace_bracket() {
    let (preflight, reader) = loaded();
    let refused = event(
        Kind::Refusal,
        Channel::Internal,
        Origin::Runtime,
        false,
        Some(Payload::Refusal(raw(r#"{"case":"partition_invalid"}"#))),
    );
    let mut refused = match preflight
        .refuse(refused)
        .unwrap_or_else(|error| panic!("{:?}", error.failure))
    {
        PreflightAttempt::Recorded {
            recorder,
            submission,
        } => {
            assert_eq!(submission.tap, TapOutcome::Stopped);
            recorder
        }
        _ => panic!(),
    };
    let _ = recorded(refused.record_fault(fault()));
    assert!(refused.record_fault(fault()).is_err());
    let mut closed = match refused
        .record_unload(fixture(Kind::Unload))
        .unwrap_or_else(|error| panic!("{:?}", error.failure))
    {
        PreflightAttempt::Recorded { recorder, .. } => recorder,
        _ => panic!(),
    };
    assert_eq!(
        closed
            .drain_by(Instant::now() + Duration::from_secs(3))
            .unwrap(),
        Drained::Emptied
    );
    drop(closed);
    let text = bytes(reader);
    assert_eq!(text.lines().count(), 4);
    assert!(text.lines().last().unwrap().contains(r#""kind":"unload""#));
}

#[test]
fn malformed_render_and_append_failure_consume_no_admission_or_capacity() {
    let (mut recorder, reader) = recorder();
    drained(&mut recorder);
    let mut wrong = message("Read file.");
    wrong.envelope.origin = Origin::Runtime;
    assert!(recorder.submit(wrong).is_err());
    assert_eq!(recorder.structure().len(), 1);
    let mut multiline = fault();
    multiline.payload = Some(Payload::Fault(raw("{\n\"case\":\"fault\"\n}")));
    assert!(recorder.submit(multiline).is_err());
    assert_eq!(recorder.pressure().queued, 0);
    assert_eq!(recorder.core.next, Some(1));
    let good = recorded(recorder.submit(message("Read file.")));
    assert_eq!(good.sequence, Sequence(1));
    drained(&mut recorder);
    recorder.core.structure.fail_append = true;
    assert!(matches!(
        recorder.submit(message("Read again.")),
        Err(Failure::AppendFailed {
            sequence: Sequence(2)
        })
    ));
    recorder.core.structure.fail_append = false;
    assert!(
        matches!(
            recorder.submit(message("Read again.")),
            Err(Failure::AppendFailed {
                sequence: Sequence(2)
            })
        ),
        "append failure stays latched"
    );
    assert_eq!(recorder.structure().len(), 2);
    assert_eq!(recorder.boundary().admitted, Some(Sequence(1)));
    assert_eq!(recorder.pressure().queued, 0);
    drop(recorder);
    assert_eq!(bytes(reader).lines().count(), 2);
}

#[test]
fn capacity_pending_preserves_event_and_sequence_then_admits_after_real_wake() {
    let (mut recorder, mut reader) = recorder();
    drained(&mut recorder);
    let _ = recorded(recorder.submit(message(&"x".repeat(1_000_000))));
    for _ in 1..super::super::ORDINARY_CAPACITY {
        let _ = recorded(recorder.submit(message("Read file.")));
    }
    let waiting = message("Keep pending.");
    let before = serde_json::to_string(&waiting).unwrap();
    let waiting = match recorder.try_submit(waiting).unwrap() {
        SubmitAttempt::CapacityPending(event) => event,
        _ => panic!("queue must be full"),
    };
    assert_eq!(serde_json::to_string(&waiting).unwrap(), before);
    assert_eq!(recorder.core.next, Some(1018));
    assert_eq!(recorder.structure().len(), 1018);
    let reader = std::thread::spawn(move || {
        let mut data = String::new();
        reader.read_to_string(&mut data).unwrap();
        data
    });
    assert_eq!(
        recorder
            .drain_by(Instant::now() + Duration::from_secs(3))
            .unwrap(),
        Drained::Emptied
    );
    recorder.drain_capacity_wake().unwrap();
    assert_eq!(recorded(recorder.submit(waiting)).sequence, Sequence(1018));
    drained(&mut recorder);
    drop(recorder);
    let data = reader.join().unwrap();
    assert_eq!(data.lines().count(), 1019);
}

#[test]
fn full_ordinary_queue_cannot_starve_the_seven_one_use_terminal_records() {
    let (mut recorder, mut reader) = recorder();
    drained(&mut recorder);
    assert!(recorder.submit_terminal(fault()).is_err());
    let _ = recorded(recorder.submit(message(&"x".repeat(1_000_000))));
    for _ in 1..super::super::ORDINARY_CAPACITY {
        let _ = recorded(recorder.submit(message("Read file.")));
    }
    recorder.stop_tap();
    assert!(recorder.submit_terminal(message("Read file.")).is_err());
    assert!(recorder.submit_terminal(fixture(Kind::SavePoint)).is_err());
    let terminal = [
        fault(),
        fixture(Kind::ToolCallCompleted),
        event(
            Kind::MessageToolResult,
            Channel::Tool,
            Origin::Tool,
            true,
            Some(Payload::Message(raw(r#"{"role":"tool","content":[]}"#))),
        ),
        event(
            Kind::TurnClosed,
            Channel::Internal,
            Origin::Runtime,
            true,
            Some(Payload::TurnClosed(crate::TurnClose::Clean)),
        ),
        event(
            Kind::SessionClosed,
            Channel::Internal,
            Origin::Runtime,
            false,
            None,
        ),
        event(
            Kind::SavePoint,
            Channel::Internal,
            Origin::Runtime,
            false,
            Some(Payload::SavePointRecord(Box::new(
                SavePointRecord::Refused {
                    capture_id: 1.into(),
                    window: SaveWindow {
                        began_ns: 1.into(),
                        deadline_ns: 2.into(),
                    },
                    tail_window: None,
                    cause: PersistenceCause::ForcedUnload,
                    refusal_code: "component_failure".into(),
                },
            ))),
        ),
        fixture(Kind::Unload),
    ];
    for entry in terminal {
        let duplicate = entry.clone();
        let receipt = recorder.submit_terminal(entry).unwrap();
        assert_eq!(receipt.tap, TapOutcome::Stopped);
        assert!(recorder.submit_terminal(duplicate).is_err());
    }
    assert_eq!(recorder.pressure().queued, 1024);
    assert_eq!(recorder.pressure().peak, 1024);
    let reader = std::thread::spawn(move || {
        let mut data = String::new();
        reader.read_to_string(&mut data).unwrap();
        data
    });
    drained(&mut recorder);
    drop(recorder);
    assert_eq!(reader.join().unwrap().lines().count(), 1025);
}

#[test]
fn pending_admission_neither_consumes_identity_nor_rewrites_the_returned_event() {
    let (mut recorder, reader) = recorder();
    drained(&mut recorder);
    let held: Vec<_> = (0..1017)
        .map(|_| recorder.core.writer.reserve(None).unwrap().unwrap())
        .collect();
    let waiting = message("Keep pending.");
    let before = serde_json::to_string(&waiting).unwrap();
    let waiting = match recorder.submit(waiting).unwrap() {
        SubmitAttempt::CapacityPending(event) => event,
        _ => panic!("all ordinary slots are reserved"),
    };
    assert_eq!(serde_json::to_string(&waiting).unwrap(), before);
    assert_eq!(recorder.core.next, Some(1));
    assert_eq!(recorder.structure().len(), 1);
    drop(held);
    assert_eq!(recorded(recorder.submit(waiting)).sequence, Sequence(1));
    drained(&mut recorder);
    drop(recorder);
    assert_eq!(bytes(reader).lines().count(), 2);
}

#[test]
fn preflight_terminal_fault_and_unload_bypass_full_ordinary_capacity() {
    let (mut preflight, reader) = loaded();
    preflight.drain().unwrap();
    let held: Vec<_> = (0..super::super::ORDINARY_CAPACITY)
        .map(|_| preflight.core.writer.reserve(None).unwrap().unwrap())
        .collect();
    assert_eq!(
        preflight.drain_status().unwrap(),
        DrainStatus::Pending {
            queued: super::super::ORDINARY_CAPACITY
        }
    );
    let terminal = preflight
        .record_terminal_fault(fault())
        .unwrap_or_else(|error| panic!("{:?}", error.failure));
    assert_eq!(terminal.submission.sequence, Sequence(1));
    assert_eq!(terminal.submission.tap, TapOutcome::Stopped);
    let duplicate = fault();
    let original = serde_json::to_string(&duplicate).unwrap();
    let failure = match terminal.recorder.record_terminal_fault(duplicate) {
        Ok(_) => panic!("a terminal fault cannot spend its permit twice"),
        Err(failure) => failure,
    };
    assert_eq!(serde_json::to_string(&failure.event).unwrap(), original);
    assert_eq!(failure.recorder.boundary().admitted, Some(Sequence(1)));
    let terminal = failure
        .recorder
        .record_terminal_unload(fixture(Kind::Unload))
        .unwrap_or_else(|error| panic!("{:?}", error.failure));
    assert_eq!(terminal.submission.sequence, Sequence(2));
    assert_eq!(terminal.submission.tap, TapOutcome::Stopped);
    drop(held);
    let mut closed = terminal.recorder;
    closed.drain().unwrap();
    assert_eq!(closed.drain_status().unwrap(), DrainStatus::Emptied);
    assert_eq!(closed.standing_failure(), None);
    assert_eq!(closed.try_capacity_wake().unwrap(), CapacityWake::Consumed);
    assert_eq!(closed.try_capacity_wake().unwrap(), CapacityWake::Empty);
    drop(closed);
    let text = bytes(reader);
    assert_eq!(text.lines().count(), 3);
    assert!(text.lines().nth(1).unwrap().contains(r#""kind":"fault""#));
    assert!(text.lines().nth(2).unwrap().contains(r#""kind":"unload""#));
}

#[test]
fn terminal_preflight_refusal_preserves_original_event_and_prior_typestate() {
    let (preflight, reader) = loaded();
    let wrong = message("Read file.");
    let original = serde_json::to_string(&wrong).unwrap();
    let failure = match preflight.record_terminal_fault(wrong) {
        Ok(_) => panic!("content cannot spend a fault reservation"),
        Err(failure) => failure,
    };
    assert_eq!(serde_json::to_string(&failure.event).unwrap(), original);
    assert_eq!(failure.recorder.boundary().admitted, Some(Sequence(0)));
    let mut malformed = fault();
    malformed.envelope.origin = Origin::Operator;
    let original = serde_json::to_string(&malformed).unwrap();
    let failure = match failure.recorder.record_terminal_fault(malformed) {
        Ok(_) => panic!("an invalid envelope cannot spend the reservation"),
        Err(failure) => failure,
    };
    assert_eq!(serde_json::to_string(&failure.event).unwrap(), original);
    assert_eq!(failure.recorder.boundary().admitted, Some(Sequence(0)));
    let terminal = failure
        .recorder
        .record_terminal_fault(fault())
        .unwrap_or_else(|error| panic!("{:?}", error.failure));
    let wrong = fault();
    let original = serde_json::to_string(&wrong).unwrap();
    let failure = match terminal.recorder.record_terminal_unload(wrong) {
        Ok(_) => panic!("only unload can spend the unload reservation"),
        Err(failure) => failure,
    };
    assert_eq!(serde_json::to_string(&failure.event).unwrap(), original);
    let terminal = failure
        .recorder
        .record_terminal_unload(fixture(Kind::Unload))
        .unwrap_or_else(|error| panic!("{:?}", error.failure));
    let mut closed = terminal.recorder;
    closed.drain().unwrap();
    drop(closed);
    assert_eq!(bytes(reader).lines().count(), 3);
}

#[test]
fn an_ordinary_preflight_fault_cannot_be_repeated_using_the_terminal_reserve() {
    let (preflight, reader) = loaded();
    let refused = match preflight
        .refuse(fault())
        .unwrap_or_else(|error| panic!("{:?}", error.failure))
    {
        PreflightAttempt::Recorded { recorder, .. } => recorder,
        _ => panic!("ordinary fault must fit"),
    };
    let failure = match refused.record_terminal_fault(fault()) {
        Ok(_) => panic!("the already recorded first fault cannot repeat"),
        Err(failure) => failure,
    };
    assert_eq!(failure.recorder.structure().by_kind(Kind::Fault).count(), 1);
    let terminal = failure
        .recorder
        .record_terminal_unload(fixture(Kind::Unload))
        .unwrap_or_else(|error| panic!("{:?}", error.failure));
    let mut closed = terminal.recorder;
    closed.drain().unwrap();
    drop(closed);
    assert_eq!(bytes(reader).lines().count(), 3);
}

#[test]
fn component_failure_after_an_ordinary_refusal_can_use_the_fault_reserve() {
    let (preflight, reader) = loaded();
    let refusal = event(
        Kind::Refusal,
        Channel::Internal,
        Origin::Runtime,
        false,
        Some(Payload::Refusal(raw(r#"{"case":"partition_invalid"}"#))),
    );
    let refused = match preflight
        .refuse(refusal)
        .unwrap_or_else(|error| panic!("{:?}", error.failure))
    {
        PreflightAttempt::Recorded { recorder, .. } => recorder,
        _ => panic!("ordinary refusal must fit"),
    };
    let terminal = refused
        .record_terminal_fault(fault())
        .unwrap_or_else(|error| panic!("{:?}", error.failure));
    assert_eq!(terminal.submission.sequence, Sequence(2));
    assert_eq!(terminal.submission.tap, TapOutcome::Stopped);
    let terminal = terminal
        .recorder
        .record_terminal_unload(fixture(Kind::Unload))
        .unwrap_or_else(|error| panic!("{:?}", error.failure));
    let mut closed = terminal.recorder;
    closed.drain().unwrap();
    drop(closed);
    assert_eq!(bytes(reader).lines().count(), 4);
}

#[test]
fn terminal_preflight_append_failure_preserves_event_and_exact_status() {
    let (mut preflight, reader) = loaded();
    preflight.drain().unwrap();
    preflight.core.structure.fail_append = true;
    let report = fault();
    let original = serde_json::to_string(&report).unwrap();
    let failure = match preflight.record_terminal_fault(report) {
        Ok(_) => panic!("the append must fail"),
        Err(failure) => failure,
    };
    assert_eq!(serde_json::to_string(&failure.event).unwrap(), original);
    let expected = Failure::AppendFailed {
        sequence: Sequence(1),
    };
    assert_eq!(failure.failure, expected);
    let mut preflight = failure.recorder;
    assert_eq!(preflight.standing_failure(), Some(expected.clone()));
    assert_eq!(preflight.drain_status(), Err(expected.clone()));
    assert_eq!(preflight.try_capacity_wake(), Err(expected));
    assert_eq!(preflight.boundary().admitted, Some(Sequence(0)));
    assert_eq!(preflight.pressure().queued, 0);
    drop(preflight);
    assert_eq!(bytes(reader).lines().count(), 1);
}

#[test]
fn active_nonblocking_status_keeps_append_failure_after_queue_empty() {
    let (mut recorder, reader) = recorder();
    drained(&mut recorder);
    recorder.core.structure.fail_append = true;
    let expected = Failure::AppendFailed {
        sequence: Sequence(1),
    };
    assert!(matches!(
        recorder.submit(message("Read file.")),
        Err(Failure::AppendFailed {
            sequence: Sequence(1)
        })
    ));
    assert_eq!(recorder.standing_failure(), Some(expected.clone()));
    assert_eq!(recorder.drain_status(), Err(expected.clone()));
    assert_eq!(recorder.try_capacity_wake(), Err(expected));
    assert_eq!(recorder.pressure().queued, 0);
    drop(recorder);
    assert_eq!(bytes(reader).lines().count(), 1);
}
