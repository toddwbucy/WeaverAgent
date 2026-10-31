//! Composed transport fixtures, using the existing owned channel constructors.
//! These prove bounded byte progress and held slots, not the future context's
//! lifecycle policy or its permission to release a semantic completion.

use std::io::Read;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::socket::{MsgFlags, recv, send, setsockopt, sockopt};
use weaver_trace::serving::{
    Channel, Envelope, Event, Kind, ORDINARY_CAPACITY, Origin, Payload, PreflightAttempt,
    PreflightRecorder, Recorder, SubmitAttempt, validate_partition,
};
use weaver_trace::{MonotonicNs, RunRef, Sequence, SessionRef, Subsystem, TurnRef};
use weaver_types::state_flow::FlowBacklog;
use weaver_types::{
    ExchangeId, LabelAnswer, LifecycleAnswer, Opener, OrganEnvelope, Position, SegmentPreamble,
    TokenAnswer, TokenDirective, TurnKey,
};

use crate::channel::{ClassifyChannel, ClassifyReply, DecodeChannel, DecodeReply, OrganChannel};

use super::intake::{FlowInbox, RetentionBudget};
use super::pump_transport::{
    DecodePort, LabelPort, OrganPort, ReceiveProgress, SendProgress, Stage,
};
use super::recording::{RecordBarrier, RecordProgress, RecordPurpose};
use super::recording_fixture as fixture;

fn send_bytes(fd: BorrowedFd<'_>, body: &[u8]) {
    assert_eq!(
        send(
            fd.as_raw_fd(),
            body,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        ),
        Ok(body.len()),
        "bounded fixture packet must fit before the receiver advances"
    );
}

fn receive_bytes(fd: BorrowedFd<'_>) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(
        fd.as_raw_fd(),
        &mut bytes,
        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC,
    ) {
        Ok(length) => {
            assert!(length > 0 && length <= bytes.len());
            bytes.truncate(length);
            Some(bytes)
        }
        Err(Errno::EAGAIN) => None,
        other => panic!("fixture receive failed: {other:?}"),
    }
}

fn assert_next_packet(fd: BorrowedFd<'_>, expected: &[u8]) {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let length = recv(
        fd.as_raw_fd(),
        &mut bytes,
        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK | MsgFlags::MSG_TRUNC,
    )
    .unwrap();
    assert_eq!(
        &bytes[..length],
        expected,
        "a full slot must leave the next packet unread"
    );
}

fn organ_answer(ordinal: u64) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal,
        },
        position: Position::Close,
        payload: weaver_types::Payload::Answer(LifecycleAnswer::Ready),
    }
}

fn assert_organ_answer(answer: OrganEnvelope, ordinal: u64) {
    assert_eq!(answer.exchange, organ_answer(ordinal).exchange);
    assert_eq!(answer.position, Position::Close);
    assert!(matches!(
        answer.payload,
        weaver_types::Payload::Answer(LifecycleAnswer::Ready)
    ));
}

fn assert_token(answer: DecodeReply, token: u32, expected: &str) {
    let DecodeReply::Answer(TokenAnswer::Token {
        token: actual,
        piece,
    }) = answer
    else {
        panic!("token answer was changed: {answer:?}");
    };
    assert_eq!(actual, token);
    assert_eq!(piece, expected);
}

fn assert_label_ready(answer: ClassifyReply) {
    assert!(matches!(answer, ClassifyReply::Answer(LabelAnswer::Ready)));
}

#[test]
fn a_stalled_decode_series_does_not_hold_lifecycle_or_label_progress() {
    let (decode, decode_peer) = DecodeChannel::pair().unwrap();
    let (organ, organ_peer) = OrganChannel::pair().unwrap();
    let (admin, admin_peer) = OrganChannel::pair().unwrap();
    let (label, label_peer) = ClassifyChannel::pair().unwrap();
    let mut decode = DecodePort::from_channel(decode).unwrap();
    let mut organ = OrganPort::from_channel(organ).unwrap();
    let mut admin = OrganPort::from_channel(admin).unwrap();
    let mut label = LabelPort::from_channel(label).unwrap();
    let text = "x".repeat(weaver_types::MAX_ENVELOPE_BYTES + 100);
    let body = serde_json::to_vec(&TokenAnswer::Token {
        token: 41,
        piece: text.clone(),
    })
    .unwrap();
    let segments: Vec<_> = body.chunks(weaver_types::MAX_ENVELOPE_BYTES).collect();
    assert_eq!(segments.len(), 2);
    let preamble = SegmentPreamble {
        segments: 2,
        bytes: body.len() as u64,
    };
    send_bytes(decode_peer.as_fd(), &serde_json::to_vec(&preamble).unwrap());
    send_bytes(decode_peer.as_fd(), segments[0]);
    send_bytes(
        organ_peer.as_fd(),
        &serde_json::to_vec(&organ_answer(17)).unwrap(),
    );
    let stop = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 71,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        }),
    };
    send_bytes(admin_peer.as_fd(), &serde_json::to_vec(&stop).unwrap());
    send_bytes(
        label_peer.as_fd(),
        &serde_json::to_vec(&LabelAnswer::Ready).unwrap(),
    );

    // The external watchdog drops the peer on failure, waking an accidental
    // blocking read without introducing a deadline into the adapter itself.
    let (finished, completion) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        assert!(matches!(
            decode.try_receive().unwrap(),
            ReceiveProgress::Progress
        ));
        assert!(matches!(
            decode.try_receive().unwrap(),
            ReceiveProgress::Progress
        ));
        assert!(matches!(
            decode.try_receive().unwrap(),
            ReceiveProgress::Pending
        ));
        assert!(decode.held().is_none());
        assert!(matches!(
            organ.try_receive().unwrap(),
            ReceiveProgress::Held
        ));
        assert!(matches!(
            admin.try_receive().unwrap(),
            ReceiveProgress::Held
        ));
        assert!(matches!(
            label.try_receive().unwrap(),
            ReceiveProgress::Held
        ));
        finished.send(()).unwrap();
        (decode, organ, admin, label)
    });
    if completion.recv_timeout(Duration::from_secs(2)).is_err() {
        drop(decode_peer);
        let _ = worker.join();
        panic!("a partial decode series blocked independent source progress");
    }
    let (mut decode, mut organ, mut admin, mut label) = worker.join().unwrap();
    assert_organ_answer(organ.take_completion().unwrap().unwrap(), 17);
    let stopped = admin.take_completion().unwrap().unwrap();
    assert_eq!(stopped.exchange, stop.exchange);
    assert_eq!(stopped.position, Position::Open);
    assert!(matches!(
        stopped.payload,
        weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 }
        })
    ));
    assert_label_ready(label.take_completion().unwrap().unwrap());
    assert!(decode.take_completion().unwrap().is_none());
    send_bytes(decode_peer.as_fd(), segments[1]);
    assert!(matches!(
        decode.try_receive().unwrap(),
        ReceiveProgress::Held
    ));
    assert_token(decode.take_completion().unwrap().unwrap(), 41, &text);
    assert!(decode.take_completion().unwrap().is_none());
}

fn trace_event(kind: Kind, payload: Payload) -> Event {
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: (kind != Kind::Load).then(|| TurnRef("turn".into())),
            sequence: Sequence(0),
            kind,
            channel: if kind == Kind::Load {
                Channel::Internal
            } else {
                Channel::Measurement
            },
            origin: Origin::Runtime,
            subsystem: Subsystem::Harness,
            causal_parent: None,
            wall_ms: 1,
            monotonic_ns: MonotonicNs(1),
        },
        payload: Some(payload),
    }
}

fn pressured_recorder() -> (Recorder, UnixStream) {
    // No write timeout on the sink, as admin's blocking sink carries none: a
    // filled queue blocks the writer until the test drains, and a timeout
    // raced that wait into a commit failure under load (#416).
    let (sink, reader) = UnixStream::pair().unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let preflight = PreflightRecorder::receive(
        sink.into(),
        RunRef("run".into()),
        SessionRef("session".into()),
    )
    .unwrap();
    let PreflightAttempt::Recorded { recorder, .. } = preflight
        .record_load(trace_event(
            Kind::Load,
            Payload::Elections(Box::new(fixture::load_record())),
        ))
        .unwrap_or_else(|failure| panic!("fixture load failed: {:?}", failure.failure))
    else {
        panic!("empty recorder must admit load");
    };
    let mut recorder = recorder.activate(validate_partition(&fixture::partition()).unwrap());
    // Rebound after the recorder so a panic in the fill drops the reader
    // first: the blocked write then fails and the writer's join returns.
    let reader = reader;
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({ "pressure": "x".repeat(32_768) }))
            .unwrap();
    for _ in 0..ORDINARY_CAPACITY + 64 {
        if matches!(
            recorder
                .submit(trace_event(
                    Kind::ModelMeasurement,
                    Payload::ModelMeasurement(payload.clone())
                ))
                .unwrap(),
            SubmitAttempt::CapacityPending(_)
        ) {
            return (recorder, reader);
        }
    }
    panic!("fixture did not fill the actual writer queue");
}

#[test]
fn typed_completions_stay_whole_while_a_real_trace_ticket_waits() {
    let (decode, decode_peer) = DecodeChannel::pair().unwrap();
    let (organ, organ_peer) = OrganChannel::pair().unwrap();
    let (label, label_peer) = ClassifyChannel::pair().unwrap();
    let mut decode = DecodePort::from_channel(decode).unwrap();
    let mut organ = OrganPort::from_channel(organ).unwrap();
    let mut label = LabelPort::from_channel(label).unwrap();
    let first_decode = serde_json::to_vec(&TokenAnswer::Token {
        token: 1,
        piece: "first".into(),
    })
    .unwrap();
    let second_decode = serde_json::to_vec(&TokenAnswer::Token {
        token: 2,
        piece: "second".into(),
    })
    .unwrap();
    let first_organ = serde_json::to_vec(&organ_answer(1)).unwrap();
    let second_organ = serde_json::to_vec(&organ_answer(2)).unwrap();
    let first_label = serde_json::to_vec(&LabelAnswer::Ready).unwrap();
    let second_label = serde_json::to_vec(&weaver_types::LabelRefusal::NotReady).unwrap();
    for (fd, first, second) in [
        (decode_peer.as_fd(), &first_decode, &second_decode),
        (organ_peer.as_fd(), &first_organ, &second_organ),
        (label_peer.as_fd(), &first_label, &second_label),
    ] {
        send_bytes(fd, first);
        send_bytes(fd, second);
    }

    let (recorder, reader) = pressured_recorder();
    let prior = recorder.structure().len();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    // Rebound after the barrier so a panic drops the reader first (#416).
    let mut reader = reader;
    let mut inbox = FlowInbox::new();
    let mut budget = RetentionBudget::new(FlowBacklog {
        max_pending_blocks: 2.into(),
        max_pending_bytes: 100_000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "transport fixture only".into(),
    })
    .unwrap();
    let pending = trace_event(
        Kind::ModelMeasurement,
        Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({ "held_transport_fixture": true }))
                .unwrap(),
        ),
    );
    assert!(matches!(
        barrier.submit(pending, RecordPurpose::Observation, &mut inbox, &mut budget),
        RecordProgress::Pending
    ));
    assert_eq!(barrier.recorder().structure().len(), prior);

    for _ in 0..3 {
        assert!(matches!(
            organ.try_receive().unwrap(),
            ReceiveProgress::Held
        ));
        assert!(matches!(
            decode.try_receive().unwrap(),
            ReceiveProgress::Held
        ));
        assert!(matches!(
            label.try_receive().unwrap(),
            ReceiveProgress::Held
        ));
        assert!(barrier.pending().is_some());
        assert!(
            !organ.interest().readable && !decode.interest().readable && !label.interest().readable
        );
        assert_next_packet(organ.fd(), &second_organ);
        assert_next_packet(decode.fd(), &second_decode);
        assert_next_packet(label.fd(), &second_label);
    }

    let drained = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match barrier.resume(&mut inbox, &mut budget) {
            RecordProgress::Pending => {
                assert!(Instant::now() < deadline, "trace capacity did not return");
                std::thread::yield_now();
            }
            RecordProgress::Recorded { .. } => break,
            other => panic!("trace ticket failed: {other:?}"),
        }
    }
    assert_eq!(barrier.recorder().structure().len(), prior + 1);
    assert_organ_answer(organ.take_completion().unwrap().unwrap(), 1);
    assert_token(decode.take_completion().unwrap().unwrap(), 1, "first");
    assert_label_ready(label.take_completion().unwrap().unwrap());
    assert!(matches!(
        organ.try_receive().unwrap(),
        ReceiveProgress::Held
    ));
    assert!(matches!(
        decode.try_receive().unwrap(),
        ReceiveProgress::Held
    ));
    assert!(matches!(
        label.try_receive().unwrap(),
        ReceiveProgress::Held
    ));
    assert_organ_answer(organ.take_completion().unwrap().unwrap(), 2);
    assert_token(decode.take_completion().unwrap().unwrap(), 2, "second");
    assert!(matches!(
        label.take_completion().unwrap().unwrap(),
        ClassifyReply::Refusal(weaver_types::LabelRefusal::NotReady)
    ));
    assert!(organ.take_completion().unwrap().is_none());
    assert!(decode.take_completion().unwrap().is_none());
    assert!(label.take_completion().unwrap().is_none());
    drop(barrier);
    let bytes = drained.join().unwrap();
    let lines: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line["payload"]["held_transport_fixture"] == true)
            .count(),
        1
    );
}

#[test]
fn decode_send_pressure_preserves_the_series_while_lifecycle_can_answer() {
    let (decode, decode_peer) = DecodeChannel::pair().unwrap();
    let (organ, organ_peer) = OrganChannel::pair().unwrap();
    let mut decode = DecodePort::from_channel(decode).unwrap();
    let mut organ = OrganPort::from_channel(organ).unwrap();
    setsockopt(&decode.fd(), sockopt::SndBuf, &65_536).unwrap();
    let request = TokenDirective::AppendAndGenerate {
        turn: TurnKey("fixture-turn".into()),
        tool_schemas: Vec::new(),
        delta: vec![weaver_traits::Message {
            role: weaver_traits::Role::User,
            content: vec![weaver_traits::ContentBlock::Text {
                text: "x".repeat(300_000),
            }],
        }],
    };
    let expected = serde_json::to_vec(&request).unwrap();
    assert!(matches!(decode.stage(request).unwrap(), Stage::Queued));
    let mut blocked = false;
    for _ in 0..16 {
        match decode.try_send().unwrap() {
            SendProgress::Pending => {
                blocked = true;
                break;
            }
            SendProgress::Progress => {}
            SendProgress::Complete => panic!("fixture did not reach send pressure"),
        }
    }
    assert!(blocked && decode.interest().writable);
    let second = TokenDirective::Cancel {
        turn: TurnKey("not-dispatched".into()),
    };
    let Stage::Busy(returned) = decode.stage(second.clone()).unwrap() else {
        panic!("second directive displaced a partial series");
    };
    assert_eq!(returned, second);
    send_bytes(
        organ_peer.as_fd(),
        &serde_json::to_vec(&organ_answer(55)).unwrap(),
    );
    assert!(matches!(
        organ.try_receive().unwrap(),
        ReceiveProgress::Held
    ));
    assert_organ_answer(organ.take_completion().unwrap().unwrap(), 55);

    let mut packets = Vec::new();
    let mut completed = false;
    for _ in 0..64 {
        if let Some(packet) = receive_bytes(decode_peer.as_fd()) {
            packets.push(packet);
        }
        match decode.try_send().unwrap() {
            SendProgress::Complete => completed = true,
            SendProgress::Pending | SendProgress::Progress => {}
        }
        if completed && !decode.interest().writable {
            while let Some(packet) = receive_bytes(decode_peer.as_fd()) {
                packets.push(packet);
            }
            break;
        }
    }
    assert!(
        completed,
        "series never resumed after the peer drained its queue"
    );
    let preamble: SegmentPreamble = serde_json::from_slice(&packets[0]).unwrap();
    assert_eq!(preamble.bytes, expected.len() as u64);
    assert_eq!(
        preamble.segments,
        expected.len().div_ceil(weaver_types::MAX_ENVELOPE_BYTES) as u64
    );
    assert_eq!(packets.len() as u64, preamble.segments + 1);
    assert_eq!(packets[1..].concat(), expected);
    assert!(receive_bytes(decode_peer.as_fd()).is_none());
}
