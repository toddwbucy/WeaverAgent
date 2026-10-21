use super::super::payloads::{self, PersistenceCause, SavePointRecord, SaveWindow};
use super::super::{AdmittedCopy, Channel, Envelope, Origin, validate_partition};
use super::*;
use crate::{MonotonicNs, Subsystem, TurnRef};
use serde_json::value::RawValue;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::time::Duration;

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
    let (sink, reader) = UnixStream::pair().unwrap();
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
    recorder.drain().unwrap();
    assert_eq!(recorder.boundary().committed, Some(Sequence(1)));
    drop(recorder);
    assert_eq!(
        bytes(reader),
        held.iter().map(|x| x.as_ref()).collect::<String>()
    );
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
    recorder.drain().unwrap();
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
    closed.drain().unwrap();
    drop(closed);
    let text = bytes(reader);
    assert_eq!(text.lines().count(), 4);
    assert!(text.lines().last().unwrap().contains(r#""kind":"unload""#));
}

#[test]
fn malformed_render_and_append_failure_consume_no_admission_or_capacity() {
    let (mut recorder, reader) = recorder();
    recorder.drain().unwrap();
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
    recorder.drain().unwrap();
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
    recorder.drain().unwrap();
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
    recorder.drain().unwrap();
    drop(recorder);
    let data = reader.join().unwrap();
    assert_eq!(data.lines().count(), 1019);
}

#[test]
fn full_ordinary_queue_cannot_starve_the_seven_one_use_terminal_records() {
    let (mut recorder, mut reader) = recorder();
    recorder.drain().unwrap();
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
    recorder.drain().unwrap();
    drop(recorder);
    assert_eq!(reader.join().unwrap().lines().count(), 1025);
}

#[test]
fn pending_admission_neither_consumes_identity_nor_rewrites_the_returned_event() {
    let (mut recorder, reader) = recorder();
    recorder.drain().unwrap();
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
    recorder.drain().unwrap();
    drop(recorder);
    assert_eq!(bytes(reader).lines().count(), 2);
}
