//! The concrete socket adapter crosses the same recording barrier as serving.
//! Synthetic scheme and replies establish byte mechanics, not language quality.

use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use nix::sys::socket::{AddressFamily, Shutdown, SockFlag, SockType, shutdown, socketpair};
use weaver_trace::serving::{
    Channel as TraceChannel, Envelope, Event, Kind, ORDINARY_CAPACITY, Origin, Payload,
    PreflightAttempt, PreflightRecorder, Recorder, SubmitAttempt, TapOutcome, validate_partition,
};
use weaver_trace::{MonotonicNs, RunRef, Sequence, SessionRef, Subsystem, TurnRef};
use weaver_types::state_flow::{
    Digest, FlowBacklog, MeasuredWait, RepairCap, SchemeDeclaration, SeedRequest, Ste100Bootstrap,
    Ste100ComponentCode, Ste100Instruction, Ste100Ready, Ste100Reply, Ste100Request, Ste100Result,
    Ste100Target, decode_strict,
};
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

use super::classifier::{Classifier, ClassifierPoll, ClassifierState, PortFailureKind, Submit};
use super::intake::{FlowInbox, RetentionBudget};
use super::recording::{RecordBarrier, RecordProgress, RecordProof, RecordPurpose};
use super::recording_fixture as fixture;
use super::ste100_channel::{Channel, Receive, SendProgress, Tag};
use super::ste100_socket::{AdmissionPoll, SocketPort};

const TEST_WIRE_BOUND: usize = 262_144;

fn seed() -> SeedRequest {
    decode_strict(include_str!("fixtures/seed.json")).unwrap()
}

fn ready_classifier() -> (Classifier<SocketPort>, Channel, Ste100Request) {
    let seed = seed();
    let scheme = SchemeDeclaration {
        artifact_sha256: Digest::new(seed.scheme_id.strip_prefix("ste100:").unwrap()).unwrap(),
        scheme_id: seed.scheme_id.clone(),
        verbs: vec!["read".into()],
    };
    let instruction = Ste100Instruction {
        model_binding: ModelBinding {
            artifact: ArtifactRef("synthetic-wire-fixture".into()),
            devices: vec![DeviceOrdinal(0)],
        },
        model_artifact_sha256: seed.checker_artifact_sha256.clone(),
        instances: 1,
        max_wire_bytes: (TEST_WIRE_BOUND as u64).into(),
        measurement_ref: "synthetic integration fixture, not qualification".into(),
        repair_cap: RepairCap {
            attempts: 2,
            measurement_ref: "fixture".into(),
        },
        decision_wait: MeasuredWait {
            elapsed_ns: 1_000_000.into(),
            measurement_ref: "fixture".into(),
        },
    };
    let bootstrap = Ste100Bootstrap {
        protocol: "state-ste100/1".into(),
        scheme_text: seed.ste100_scheme,
        instruction: instruction.clone(),
    };
    let ready = Ste100Ready {
        protocol: bootstrap.protocol.clone(),
        scheme_id: scheme.scheme_id.clone(),
        artifact_sha256: scheme.artifact_sha256.clone(),
        checker_artifact_sha256: instruction.model_artifact_sha256,
        instances: instruction.instances,
        max_wire_bytes: instruction.max_wire_bytes,
    };
    let (ours, theirs) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let mut port = SocketPort::starting(ours, bootstrap.clone(), scheme.clone()).unwrap();
    let mut peer = Channel::from_owned(theirs, TEST_WIRE_BOUND).unwrap();
    let mut received = false;
    for _ in 0..64 {
        assert!(matches!(
            port.advance_admission().unwrap(),
            AdmissionPoll::Pending
        ));
        match peer.try_receive().unwrap() {
            Receive::Pending | Receive::Fragment => {}
            Receive::Complete(message) => {
                assert_eq!(message.tag, Tag::Bootstrap);
                assert_eq!(message.id, 0);
                let decoded: Ste100Bootstrap =
                    decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                assert_eq!(decoded, bootstrap);
                received = true;
                break;
            }
        }
    }
    assert!(received, "bootstrap never crossed the concrete socket");
    peer.begin_send(Tag::Ready, 0, serde_json::to_vec(&ready).unwrap())
        .unwrap();
    flush_peer(&mut peer);
    let mut admitted = false;
    for _ in 0..64 {
        match port.advance_admission().unwrap() {
            AdmissionPoll::Pending => {}
            AdmissionPoll::Ready(observed) => {
                assert_eq!(observed, ready);
                admitted = true;
                break;
            }
            AdmissionPoll::Refused(refusal) => panic!("fixture admission refused: {refusal:?}"),
        }
    }
    assert!(admitted, "ready echo never released admission");
    let request = Ste100Request {
        request_id: 1.into(),
        target: Ste100Target::Input {
            input_id: 7.into(),
            block: 0.into(),
        },
        text: seed.prompt_text,
        scheme_id: scheme.scheme_id.clone(),
    };
    (
        Classifier::new(port, scheme, "run".into()).unwrap(),
        peer,
        request,
    )
}

fn flush_peer(peer: &mut Channel) {
    for _ in 0..64 {
        match peer.try_send().unwrap() {
            SendProgress::Complete => return,
            SendProgress::Pending | SendProgress::Progress => {}
        }
    }
    panic!("bounded fixture reply could not be sent before receive");
}

fn send_request(
    classifier: &mut Classifier<SocketPort>,
    peer: &mut Channel,
    request: &Ste100Request,
) {
    assert_eq!(
        classifier.submit(request.clone()).unwrap(),
        Submit::Submitted
    );
    assert!(
        classifier.wants_write(),
        "the queued request needs writable interest"
    );
    for _ in 0..64 {
        assert_eq!(classifier.try_result().unwrap(), ClassifierPoll::Pending);
        match peer.try_receive().unwrap() {
            Receive::Pending | Receive::Fragment => {}
            Receive::Complete(message) => {
                assert_eq!(message.tag, Tag::Request);
                assert_eq!(message.id, request.request_id.0);
                let decoded: Ste100Request =
                    decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                assert_eq!(&decoded, request);
                assert!(
                    !classifier.wants_write(),
                    "a fully sent request needs no writable wake"
                );
                return;
            }
        }
    }
    panic!("request never crossed the concrete socket");
}

fn send_reply(peer: &mut Channel, reply: &Ste100Reply) {
    peer.begin_send(
        Tag::Reply,
        reply.request_id.0,
        serde_json::to_vec(reply).unwrap(),
    )
    .unwrap();
    flush_peer(peer);
}

fn hold_reply(classifier: &mut Classifier<SocketPort>, expected: &Ste100Reply) {
    for _ in 0..64 {
        match classifier.try_result().unwrap() {
            ClassifierPoll::Pending => {}
            ClassifierPoll::ResultHeld => {
                assert_eq!(classifier.state(), ClassifierState::ResultHeld);
                assert_eq!(classifier.held(), Some(expected));
                assert!(
                    !classifier.wants_write(),
                    "a held result must not spin on POLLOUT"
                );
                return;
            }
        }
    }
    panic!("complete reply did not reach the held slot");
}

fn accepted(request: &Ste100Request) -> Ste100Reply {
    Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: Ste100Result::Accepted {
            bindings: seed().bindings,
        },
    }
}

fn event(kind: Kind, payload: Payload) -> Event {
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: (kind == Kind::ModelMeasurement).then(|| TurnRef("fixture-turn".into())),
            sequence: Sequence(999),
            kind,
            channel: match kind {
                Kind::ModelMeasurement => TraceChannel::Measurement,
                Kind::InputOffered => TraceChannel::Input,
                _ => TraceChannel::Internal,
            },
            origin: if kind == Kind::InputOffered {
                Origin::External
            } else {
                Origin::Runtime
            },
            subsystem: Subsystem::Harness,
            causal_parent: None,
            wall_ms: 1,
            monotonic_ns: MonotonicNs(7),
        },
        payload: Some(payload),
    }
}

fn classifier_event(reply: &Ste100Reply) -> Event {
    event(
        Kind::Ste100Result,
        Payload::Ste100Result(Box::new(super::mirror::ste100(reply))),
    )
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
        .unwrap_or_else(|error| panic!("load fixture refused: {:?}", error.failure));
    let PreflightAttempt::Recorded {
        recorder,
        submission,
    } = attempt
    else {
        panic!("empty recorder has capacity for load");
    };
    assert_eq!(submission.tap, TapOutcome::HeadUntapped);
    let mut recorder = recorder.activate(validate_partition(&fixture::partition()).unwrap());
    let offered = event(
        Kind::InputOffered,
        Payload::InputOffered(Box::new(
            weaver_trace::serving::payloads::InputOfferedRecord {
                input_id: 7.into(),
                octets: weaver_types::TurnFrame::carry(b"{\"text\":\"Read item.\"}\n").octets,
            },
        )),
    );
    let SubmitAttempt::Recorded(submission) = recorder.submit(offered).unwrap() else {
        panic!("empty recorder has capacity for the offered input");
    };
    assert_eq!(submission.sequence, Sequence(1));
    (recorder, reader)
}

fn budget() -> RetentionBudget {
    RetentionBudget::new(FlowBacklog {
        max_pending_blocks: 2.into(),
        max_pending_bytes: 100_000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "test-only backlog".into(),
    })
    .unwrap()
}

fn fill_capacity(recorder: &mut Recorder) {
    let payload = serde_json::value::to_raw_value(&serde_json::json!({
        "fixture": "x".repeat(32_768)
    }))
    .unwrap();
    for _ in 0..ORDINARY_CAPACITY + 64 {
        let progress = recorder
            .submit(event(
                Kind::ModelMeasurement,
                Payload::ModelMeasurement(payload.clone()),
            ))
            .unwrap();
        if matches!(progress, SubmitAttempt::CapacityPending(_)) {
            return;
        }
    }
    panic!("fixture failed to saturate the real trace queue");
}

fn drain(reader: UnixStream) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).expect("bounded trace drain");
        bytes
    })
}

fn assert_trace_reply(bytes: &[u8], expected: &Ste100Reply) {
    let lines: Vec<serde_json::Value> = std::str::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|line: &serde_json::Value| line["kind"] == "ste100.result")
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "one complete result must reach the trace socket"
    );
    assert_eq!(lines[0]["payload"], serde_json::to_value(expected).unwrap());
}

#[test]
fn classifier_writable_interest_survives_real_request_backpressure() {
    let (mut classifier, mut peer, mut request) = ready_classifier();
    nix::sys::socket::setsockopt(
        &classifier.readiness(),
        nix::sys::socket::sockopt::SndBuf,
        &65_536,
    )
    .unwrap();
    request.text = format!("Read {}.", "x".repeat(200_000));
    assert_eq!(
        classifier.submit(request.clone()).unwrap(),
        Submit::Submitted
    );
    // The peer deliberately does not read. The request is several datagrams,
    // larger than this socket's buffer, so progress must retain a send cursor.
    for _ in 0..16 {
        assert_eq!(classifier.try_result().unwrap(), ClassifierPoll::Pending);
    }
    assert!(
        classifier.wants_write(),
        "a backpressured request must retain writable interest"
    );
    let second = Ste100Request {
        request_id: 2.into(),
        ..request.clone()
    };
    assert_eq!(
        classifier.submit(second.clone()).unwrap(),
        Submit::Busy(second)
    );
    for _ in 0..64 {
        assert_eq!(classifier.try_result().unwrap(), ClassifierPoll::Pending);
        match peer.try_receive().unwrap() {
            Receive::Pending | Receive::Fragment => {}
            Receive::Complete(message) => {
                assert_eq!(message.tag, Tag::Request);
                let decoded: Ste100Request =
                    decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                assert_eq!(
                    decoded, request,
                    "backpressure must preserve the whole request"
                );
                assert!(!classifier.wants_write());
                assert!(matches!(peer.try_receive().unwrap(), Receive::Pending));
                return;
            }
        }
    }
    panic!("request did not finish after the peer drained backpressure");
}

#[test]
fn socket_acceptance_waits_for_trace_capacity_before_it_can_be_reused() {
    let (mut classifier, mut peer, request) = ready_classifier();
    send_request(&mut classifier, &mut peer, &request);
    let expected = accepted(&request);
    send_reply(&mut peer, &expected);
    hold_reply(&mut classifier, &expected);

    let (mut recorder, reader) = recorder();
    fill_capacity(&mut recorder);
    let count = recorder.structure().len();
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = budget();
    assert!(matches!(
        barrier.submit(
            classifier_event(&expected),
            RecordPurpose::Classifier(expected.clone()),
            &mut inbox,
            &mut budget,
        ),
        RecordProgress::Pending
    ));
    assert_eq!(barrier.recorder().structure().len(), count);
    assert_eq!(classifier.state(), ClassifierState::ResultHeld);
    assert_eq!(classifier.held(), Some(&expected));
    let next = Ste100Request {
        request_id: 2.into(),
        ..request.clone()
    };
    assert_eq!(
        classifier.submit(next.clone()).unwrap(),
        Submit::Busy(next.clone())
    );
    assert!(matches!(peer.try_receive().unwrap(), Receive::Pending));

    let drain = drain(reader);
    let deadline = Instant::now() + Duration::from_secs(8);
    let proof = loop {
        match barrier.resume(&mut inbox, &mut budget) {
            RecordProgress::Pending => {
                assert!(Instant::now() < deadline, "trace capacity did not return");
                std::thread::yield_now();
            }
            RecordProgress::Recorded {
                proof: Some(RecordProof::Classifier(proof)),
                ..
            } => break proof,
            other => panic!("classifier trace admission failed: {other:?}"),
        }
    };
    assert_eq!(classifier.state(), ClassifierState::ResultHeld);
    let grant = classifier.recorded_input(proof, 1.into()).unwrap();
    assert_eq!(grant.reply(), &expected);
    assert_eq!(grant.request(), &request);
    assert_eq!(classifier.state(), ClassifierState::Idle);
    assert!(classifier.held().is_none());
    assert_eq!(classifier.submit(next).unwrap(), Submit::Submitted);
    assert_eq!(barrier.recorder().structure().len(), count + 1);
    assert_eq!(budget.gauges(), (0, 0));
    assert_eq!(inbox.queued().count(), 0);
    drop(barrier);
    assert_trace_reply(&drain.join().unwrap(), &expected);
}

#[test]
fn complete_component_reply_survives_peer_exit_until_its_exact_trace_record() {
    for half_close in [false, true] {
        let (mut classifier, mut peer, request) = ready_classifier();
        send_request(&mut classifier, &mut peer, &request);
        let expected = Ste100Reply {
            request_id: request.request_id,
            target: request.target.clone(),
            scheme_id: request.scheme_id.clone(),
            result: Ste100Result::ComponentFailed {
                code: Ste100ComponentCode::NoResult,
                // More than one datagram must be reassembled even with EOF ready.
                detail: format!("artifact_capacity:{}", "x".repeat(70_000)),
            },
        };
        send_reply(&mut peer, &expected);
        if half_close {
            shutdown(peer.readiness().as_raw_fd(), Shutdown::Write).unwrap();
        } else {
            drop(peer);
        }
        hold_reply(&mut classifier, &expected);
        assert!(classifier.failure().is_none());
        let (recorder, reader) = recorder();
        let drain = drain(reader);
        let mut barrier = RecordBarrier::new(recorder, "run".into());
        let mut inbox = FlowInbox::new();
        let mut budget = budget();
        let RecordProgress::Recorded {
            proof: Some(RecordProof::Classifier(proof)),
            ..
        } = barrier.submit(
            classifier_event(&expected),
            RecordPurpose::Classifier(expected.clone()),
            &mut inbox,
            &mut budget,
        )
        else {
            panic!("terminal result requires its real trace record");
        };
        assert_eq!(classifier.state(), ClassifierState::ResultHeld);
        assert_eq!(classifier.recorded(proof).unwrap(), expected);
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(
            classifier.failure().unwrap().kind,
            PortFailureKind::Component(Ste100ComponentCode::NoResult)
        );
        assert_eq!(classifier.outstanding(), Some(&request));
        assert!(classifier.submit(request).is_err());
        assert_eq!(budget.gauges(), (0, 0));
        drop(barrier);
        assert_trace_reply(&drain.join().unwrap(), &expected);
    }
}

#[test]
fn accepted_socket_reply_cannot_mint_reuse_after_the_peer_dies() {
    for half_close in [false, true] {
        let (mut classifier, mut peer, request) = ready_classifier();
        send_request(&mut classifier, &mut peer, &request);
        let expected = accepted(&request);
        send_reply(&mut peer, &expected);
        hold_reply(&mut classifier, &expected);
        let (recorder, reader) = recorder();
        let drain = drain(reader);
        let mut barrier = RecordBarrier::new(recorder, "run".into());
        let mut inbox = FlowInbox::new();
        let mut budget = budget();
        let RecordProgress::Recorded {
            proof: Some(RecordProof::Classifier(proof)),
            ..
        } = barrier.submit(
            classifier_event(&expected),
            RecordPurpose::Classifier(expected.clone()),
            &mut inbox,
            &mut budget,
        )
        else {
            panic!("acceptance requires its real trace record");
        };
        if half_close {
            shutdown(peer.readiness().as_raw_fd(), Shutdown::Write).unwrap();
        } else {
            drop(peer);
        }
        let failure = classifier.recorded_input(proof, 1.into()).unwrap_err();
        assert_eq!(
            failure.kind,
            PortFailureKind::Component(Ste100ComponentCode::Crashed)
        );
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.held(), Some(&expected));
        assert_eq!(classifier.outstanding(), Some(&request));
        drop(barrier);
        assert_trace_reply(&drain.join().unwrap(), &expected);
    }
}
