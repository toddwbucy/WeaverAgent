use super::super::author::{AuthoredEvent, AuthoredSource, ServingAuthor};
use super::super::clock::{MonotonicIo, ReadyFd, testing::FakePumpIo};
use super::super::intake::{FlowInbox, RetentionBudget};
use super::super::recording::{RecordBarrier, RecordProgress, RecordPurpose};
use super::super::recording_fixture as fixture;
use super::*;
use crate::channel::{ClassifyReply, DecodeReply, bind_coordination};
use nix::fcntl::FdFlag;
use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockType, UnixAddr, connect, send, sendmsg, socket,
    socketpair,
};
use std::io::{IoSlice, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use weaver_trace::serving::{
    Channel, Envelope, Event, Kind, ORDINARY_CAPACITY, Origin, Payload, PreflightAttempt,
    PreflightRecorder, SubmitAttempt, validate_partition,
};
use weaver_trace::{MonotonicNs, RunRef, Sequence, SessionRef, Subsystem, TurnRef};
use weaver_types::state_flow::FlowBacklog;
use weaver_types::{ExchangeId, LabelAnswer, LifecycleAnswer, Opener, Position, TokenAnswer};

const _: fn() = || {
    trait NoClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <LifecyclePeer as NoClone<_>>::check;
    let _ = <LifecycleListener as NoClone<_>>::check;
    let _ = <LifecycleMessage as NoClone<_>>::check;
};

fn capture_message(id: u64) -> weaver_types::state_flow::capture::CaptureEnvelope {
    use weaver_types::state_flow::capture::*;
    CaptureEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 19,
        },
        position: Position::Continue,
        payload: CapturePayload::SavePointImageChecked(CaptureImageChecked {
            capture_id: id.into(),
            dump_sha256: weaver_types::state_flow::Digest::new("a".repeat(64)).unwrap(),
        }),
    }
}

#[test]
fn capture_transport_preserves_one_receive_slot_raw_bytes_and_handles_through_eof() {
    use weaver_types::state_flow::capture::CaptureSender;
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    send_envelope(&peer, 1);
    owner.try_receive().unwrap();
    assert!(owner.enable_capture().is_err());
    assert_eq!(owner.held().unwrap().envelope.exchange.ordinal, 1);
    owner.take_completion().unwrap().unwrap();
    owner.enable_capture().unwrap();
    let wire = capture_message(7).encode(CaptureSender::Admin).unwrap();
    let (handle, mut watcher) = UnixStream::pair().unwrap();
    watcher.set_nonblocking(true).unwrap();
    send_handles(&peer, &wire, &[handle.as_raw_fd()]);
    drop(handle);
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    assert!(owner.held().is_none());
    assert!(!owner.interest().contains(PollFlags::POLLIN));
    assert!(owner.interest().contains(terminal_flags()));
    send_bytes(
        &peer,
        &capture_message(8).encode(CaptureSender::Admin).unwrap(),
    );
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Pending);
    assert!(owner.take_completion().unwrap().is_none());
    let held = owner.take_capture().unwrap().unwrap();
    assert_eq!(held.message.envelope, capture_message(7));
    assert_eq!(held.wire, wire);
    assert!(held.message.sink.is_some());
    assert!(
        matches!(watcher.read(&mut [0]), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
    );
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    drop(peer);
    assert!(owner.try_receive().is_err());
    let retired = owner.retire_failed();
    assert_eq!(retired.capture_held.unwrap().envelope, capture_message(8));
    assert_eq!(
        retired.wire.unwrap(),
        capture_message(8).encode(CaptureSender::Admin).unwrap()
    );
    assert!(retired.failure.is_some());
    drop(held);
    assert_eq!(watcher.read(&mut [0]).unwrap(), 0);
}

#[test]
fn capture_transport_keeps_decoder_failure_and_closes_unusable_ancillary_handles() {
    for bytes in [
        b"not-json".to_vec(),
        serde_json::to_vec(&envelope(99)).unwrap(),
    ] {
        let (end, peer) = pair();
        let mut owner = LifecyclePeer::from_owned(end).unwrap();
        owner.enable_capture().unwrap();
        let (handle, mut watcher) = UnixStream::pair().unwrap();
        send_handles(&peer, &bytes, &[handle.as_raw_fd()]);
        drop(handle);
        assert!(owner.try_receive().is_err());
        let retired = owner.retire_failed();
        assert_eq!(retired.wire.unwrap(), bytes);
        assert!(retired.capture_held.is_none());
        assert!(retired.failure.is_some());
        assert_eq!(watcher.read(&mut [0]).unwrap(), 0);
    }
}

#[test]
fn capture_transport_tracks_actual_whole_send_and_returns_the_busy_original() {
    use super::super::pump_transport::TrackedStage;
    use weaver_types::state_flow::capture::*;
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    let mut value = capture_message(7);
    value.payload = CapturePayload::SavePointBeginning(CaptureBeginning {
        capture_id: 7.into(),
        window: weaver_types::state_flow::SaveWindow {
            began_ns: 100.into(),
            deadline_ns: 200.into(),
        },
        tail_window: Some(weaver_types::state_flow::SaveWindow {
            began_ns: 90.into(),
            deadline_ns: 210.into(),
        }),
    });
    owner.enable_capture().unwrap();
    let expected = value.encode(CaptureSender::Harness).unwrap();
    let TrackedStage::Queued(ticket) = owner.stage_capture(value.clone()).unwrap() else {
        panic!("free sender")
    };
    assert!(owner.owns_send(&ticket));
    assert!(ticket.matches_exchange(&value.exchange));
    assert!(!ticket.sent());
    assert_eq!(ticket.sent_at(), None);
    let TrackedStage::Busy(original) = owner.stage_capture(value.clone()).unwrap() else {
        panic!("occupied sender")
    };
    assert_eq!(original, value);
    let mut clock = FakePumpIo::new(110);
    assert_ne!(
        owner.try_send_observed(&mut clock).unwrap(),
        SendProgress::Pending
    );
    assert!(ticket.sent());
    assert_eq!(ticket.sent_at(), Some(110));
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    let count =
        nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    assert_eq!(&bytes[..count], expected);
    let _retired = owner.retire_failed();
    assert!(ticket.sent());
}

#[test]
fn capture_transport_socket_pressure_never_grants_a_whole_send_receipt() {
    use super::super::pump_transport::TrackedStage;
    use weaver_types::state_flow::capture::*;
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    owner.enable_capture().unwrap();
    loop {
        match send(
            owner.fd().as_raw_fd(),
            b"pressure",
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(_) => (),
            Err(Errno::EAGAIN) => break,
            Err(error) => panic!("{error}"),
        }
    }
    let mut value = capture_message(7);
    value.payload = CapturePayload::SavePointCaptureComplete(CaptureIdentity {
        capture_id: 7.into(),
    });
    let expected = value.encode(CaptureSender::Harness).unwrap();
    let TrackedStage::Queued(ticket) = owner.stage_capture(value).unwrap() else {
        panic!("empty sender")
    };
    let mut clock = FakePumpIo::new(110);
    for _ in 0..3 {
        assert_eq!(
            owner.try_send_observed(&mut clock).unwrap(),
            SendProgress::Pending
        );
        assert!(!ticket.sent());
        assert_eq!(ticket.sent_at(), None);
    }
    assert!(owner.interest().contains(PollFlags::POLLOUT));
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    loop {
        match nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(count) => assert_eq!(&bytes[..count], b"pressure"),
            Err(Errno::EAGAIN) => break,
            Err(error) => panic!("{error}"),
        }
    }
    clock.advance_to(150).unwrap();
    assert_ne!(
        owner.try_send_observed(&mut clock).unwrap(),
        SendProgress::Pending
    );
    assert_eq!(ticket.sent_at(), Some(150));
    let count =
        nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    assert_eq!(&bytes[..count], expected);
}

fn pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap()
}
fn envelope(ordinal: u64) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Answer(LifecycleAnswer::Ready),
    }
}
fn send_bytes(peer: &OwnedFd, bytes: &[u8]) {
    assert_eq!(
        send(
            peer.as_raw_fd(),
            bytes,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        )
        .unwrap(),
        bytes.len()
    );
}
fn send_envelope(peer: &OwnedFd, ordinal: u64) {
    send_bytes(peer, &serde_json::to_vec(&envelope(ordinal)).unwrap());
}
fn send_handles(peer: &OwnedFd, bytes: &[u8], handles: &[RawFd]) {
    assert_eq!(
        sendmsg::<()>(
            peer.as_raw_fd(),
            &[IoSlice::new(bytes)],
            &[ControlMessage::ScmRights(handles)],
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            None
        )
        .unwrap(),
        bytes.len()
    );
}
fn event(kind: Kind, payload: Payload) -> Event {
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
struct Fixture {
    barrier: RecordBarrier,
    inbox: FlowInbox,
    budget: RetentionBudget,
    author: ServingAuthor,
    reader: Option<UnixStream>,
}

#[test]
fn an_observation_wake_keeps_real_interests_and_terminal_deadline_precedence() {
    for terminal in [None, Some(104), Some(105), Some(106)] {
        let mut fixture = Fixture::new(false);
        let (end, _peer) = pair();
        let mut admin = LifecyclePeer::from_owned(end).unwrap();
        let fd = admin.fd().as_raw_fd();
        let mut io = FakePumpIo::new(100);
        let mut pump = CommonPump::new(ClockOrigin::at(100));
        let mut deadlines = DeadlineRegistry::new();
        if let Some(end) = terminal {
            deadlines
                .arm(
                    PumpDeadline::Leave,
                    AbsoluteBound::from_absolute(100, end).unwrap(),
                )
                .unwrap();
        }
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut io,
        };
        ports.lifecycle.peer = Some(&mut admin);
        assert!(matches!(
            pump.step_with_observation(
                &mut ports,
                None,
                &mut fixture.cx(),
                &deadlines,
                false,
                Some(105)
            )
            .unwrap(),
            PumpStep::Idle
        ));
        assert!(ports.io.waits().is_empty(), "advance must not sleep");
        let result = pump
            .step_with_observation(
                &mut ports,
                None,
                &mut fixture.cx(),
                &deadlines,
                true,
                Some(105),
            )
            .unwrap();
        if terminal.is_some_and(|end| end <= 105) {
            assert!(matches!(
                result,
                PumpStep::Deadline {
                    key: PumpDeadline::Leave,
                    ..
                }
            ));
        } else {
            assert!(matches!(result, PumpStep::Observation));
        }
        let wait = ports.io.waits().last().unwrap();
        assert_eq!(
            wait.absolute_deadline,
            Some(terminal.unwrap_or(105).min(105))
        );
        assert!(
            wait.descriptors
                .iter()
                .any(|(actual, flags)| *actual == fd && flags.contains(PollFlags::POLLIN))
        );
        assert_eq!(
            deadlines.nearest(),
            terminal,
            "the wake cannot arm or renew a protocol bound"
        );
    }
}
impl Fixture {
    fn new(pressure: bool) -> Self {
        let (sink, reader) = UnixStream::pair().unwrap();
        sink.set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let preflight = PreflightRecorder::receive(
            sink.into(),
            RunRef("run".into()),
            SessionRef("session".into()),
        )
        .unwrap();
        let PreflightAttempt::Recorded { recorder, .. } = preflight
            .record_load(event(
                Kind::Load,
                Payload::Elections(Box::new(fixture::load_record())),
            ))
            .unwrap_or_else(|failure| panic!("fixture load failed: {:?}", failure.failure))
        else {
            panic!("empty recorder must admit load")
        };
        let mut recorder = recorder.activate(validate_partition(&fixture::partition()).unwrap());
        if pressure {
            let payload = serde_json::value::to_raw_value(
                &serde_json::json!({"pressure":"x".repeat(32_768)}),
            )
            .unwrap();
            let mut full = false;
            for _ in 0..ORDINARY_CAPACITY + 64 {
                if matches!(
                    recorder
                        .submit(event(
                            Kind::ModelMeasurement,
                            Payload::ModelMeasurement(payload.clone())
                        ))
                        .unwrap(),
                    SubmitAttempt::CapacityPending(_)
                ) {
                    full = true;
                    break;
                }
            }
            assert!(full, "real writer queue must be full");
        }
        Self {
            barrier: RecordBarrier::new(recorder, "run".into()),
            inbox: FlowInbox::new(),
            budget: RetentionBudget::new(FlowBacklog {
                max_pending_blocks: 4.into(),
                max_pending_bytes: 1_000_000.into(),
                lag_alarm_turns: 2.into(),
                measurement_ref: "test-only declared budget".into(),
            })
            .unwrap(),
            author: ServingAuthor::new("session".into(), "run".into(), 42, 100).unwrap(),
            reader: Some(reader),
        }
    }
    fn cx(&mut self) -> FlowRecording<'_> {
        FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.reader.take());
    }
}
fn empty_ports(io: &mut impl PumpIo) -> PumpPorts<'_, impl PumpIo> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}
fn progress(result: PumpStep) -> Vec<PumpSource> {
    let PumpStep::Progress { sources } = result else {
        panic!("expected transport progress: {result:?}")
    };
    sources
}

#[test]
fn enter_handles_remain_ordered_cloexec_and_owned_until_one_take() {
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    let (sink, mut sink_peer) = UnixStream::pair().unwrap();
    let (state, mut state_peer) = UnixStream::pair().unwrap();
    send_handles(
        &peer,
        &serde_json::to_vec(&envelope(11)).unwrap(),
        &[sink.as_raw_fd(), state.as_raw_fd()],
    );
    drop((sink, state));
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    send_envelope(&peer, 12);
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Pending);
    let first = owner.held().unwrap();
    assert_eq!(first.envelope.exchange.ordinal, 11);
    for fd in [&first.sink, &first.state] {
        assert!(
            FdFlag::from_bits_retain(fcntl(fd.as_ref().unwrap(), FcntlArg::F_GETFD).unwrap())
                .contains(FdFlag::FD_CLOEXEC)
        );
    }
    let first = owner.take_completion().unwrap().unwrap();
    let mut sink = UnixStream::from(first.sink.unwrap());
    let mut state = UnixStream::from(first.state.unwrap());
    sink.write_all(b"trace").unwrap();
    state.write_all(b"state").unwrap();
    let mut bytes = [0; 5];
    sink_peer.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"trace");
    state_peer.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"state");
    assert!(owner.take_completion().unwrap().is_none());
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    assert_eq!(
        owner
            .take_completion()
            .unwrap()
            .unwrap()
            .envelope
            .exchange
            .ordinal,
        12
    );
}

#[test]
fn malformed_and_truncated_enter_packets_close_every_received_handle() {
    for bytes in [b"not-json".to_vec(), vec![b'x'; MAX_ENVELOPE_BYTES + 1]] {
        let (end, peer) = pair();
        let mut owner = LifecyclePeer::from_owned(end).unwrap();
        let (sent, mut witness) = UnixStream::pair().unwrap();
        witness
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        send_handles(&peer, &bytes, &[sent.as_raw_fd()]);
        drop(sent);
        assert!(owner.try_receive().is_err());
        assert_eq!(
            witness.read(&mut [0]).unwrap(),
            0,
            "refusal must close the received descriptor"
        );
        assert!(owner.failure().is_some());
        assert!(owner.take_completion().is_err());
    }
}

#[test]
fn held_admin_directive_survives_peer_death_without_reading_a_second_one() {
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    send_envelope(&peer, 21);
    send_envelope(&peer, 22);
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    drop(peer);
    assert!(owner.try_receive().is_err());
    assert_eq!(
        owner
            .take_completion()
            .unwrap()
            .unwrap()
            .envelope
            .exchange
            .ordinal,
        21
    );
    assert!(owner.take_completion().is_err());
    assert!(owner.try_receive().is_err());
}

#[test]
fn listener_has_one_accept_slot_and_checks_root_before_any_payload() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "weaver-pump-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = bind_coordination(&path).unwrap();
    let mut owner = LifecycleListener::new(listener).unwrap();
    assert_eq!(owner.advance().unwrap(), ListenProgress::Pending);
    let peer = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .unwrap();
    connect(peer.as_raw_fd(), &UnixAddr::new(&path).unwrap()).unwrap();
    send_envelope(&peer, 31);
    if nix::unistd::geteuid().is_root() {
        assert_eq!(owner.advance().unwrap(), ListenProgress::Accepted);
        assert!(owner.held().unwrap().held().is_none());
        assert_eq!(owner.advance().unwrap(), ListenProgress::Pending);
        let mut connection = owner.take_connection().unwrap();
        assert_eq!(connection.try_receive().unwrap(), ReceiveProgress::Held);
        assert_eq!(connection.held().unwrap().envelope.exchange.ordinal, 31);
    } else {
        assert_eq!(owner.advance().unwrap(), ListenProgress::RefusedPeer);
        assert!(owner.held().is_none());
        assert!(
            owner.failure().is_none(),
            "a refused nonroot dial must not stop the listener"
        );
    }
    owner.unlink();
}

#[test]
fn lifecycle_precedes_other_ready_sources_and_each_completion_stays_held() {
    let mut fixture = Fixture::new(false);
    let (admin, admin_peer) = pair();
    let mut admin = LifecyclePeer::from_owned(admin).unwrap();
    let (residency, residency_peer) = pair();
    let mut residency = OrganPort::from_owned(residency).unwrap();
    let (gate, gate_peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    let (decode, decode_peer) = pair();
    let mut decode = DecodePort::from_owned(decode).unwrap();
    let (label, label_peer) = pair();
    let mut label = LabelPort::from_owned(label).unwrap();
    send_envelope(&admin_peer, 1);
    send_envelope(&residency_peer, 2);
    send_envelope(&gate_peer, 3);
    send_bytes(
        &decode_peer,
        &serde_json::to_vec(&TokenAnswer::Token {
            token: 4,
            piece: "four".into(),
        })
        .unwrap(),
    );
    send_bytes(
        &label_peer,
        &serde_json::to_vec(&LabelAnswer::Ready).unwrap(),
    );
    let mut io = FakePumpIo::new(100);
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = PumpPorts {
        lifecycle: LifecyclePorts {
            listener: None,
            peer: Some(&mut admin),
            residency: Some(&mut residency),
            dial: None,
            leave_peer: None,
        },
        gate: Some(&mut gate),
        decode: Some(&mut decode),
        label: Some(&mut label),
        io: &mut io,
    };
    let sources = progress(
        pump.step(
            &mut ports,
            None,
            &mut fixture.cx(),
            &DeadlineRegistry::new(),
        )
        .unwrap(),
    );
    assert_eq!(
        &sources[..5],
        &[
            PumpSource::Admin,
            PumpSource::Residency,
            PumpSource::Gate,
            PumpSource::Decode,
            PumpSource::Label
        ]
    );
    assert!(
        io.waits().is_empty(),
        "lifecycle and buffered work must precede blocking poll"
    );
    assert_eq!(admin.held().unwrap().envelope.exchange.ordinal, 1);
    assert_eq!(residency.held().unwrap().exchange.ordinal, 2);
    assert_eq!(gate.held().unwrap().exchange.ordinal, 3);
    assert!(matches!(
        decode.held(),
        Some(DecodeReply::Answer(TokenAnswer::Token { token: 4, .. }))
    ));
    assert!(matches!(
        label.held(),
        Some(ClassifyReply::Answer(LabelAnswer::Ready))
    ));
    assert_eq!(
        fixture.barrier.recorder().structure().len(),
        1,
        "transport must never author a result"
    );
}

#[test]
fn ordinary_ready_sources_rotate_and_partial_readiness_does_not_renew_deadlines() {
    let mut fixture = Fixture::new(false);
    let (gate, gate_peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    let (decode, decode_peer) = pair();
    let mut decode = DecodePort::from_owned(decode).unwrap();
    let (label, label_peer) = pair();
    let mut label = LabelPort::from_owned(label).unwrap();
    let mut io = FakePumpIo::new(100);
    for at in [101, 102] {
        io.schedule(
            at,
            (0..3)
                .map(|index| ReadyFd {
                    index,
                    events: PollFlags::POLLIN,
                })
                .collect(),
        )
        .unwrap();
    }
    let mut deadlines = DeadlineRegistry::new();
    let cut = AbsoluteBound::after(100, 5).unwrap();
    deadlines.arm(PumpDeadline::Cut, cut).unwrap();
    deadlines
        .arm(
            PumpDeadline::Decision,
            AbsoluteBound::after(100, 10).unwrap(),
        )
        .unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    for round in 0..2 {
        send_envelope(&gate_peer, round);
        send_bytes(
            &decode_peer,
            &serde_json::to_vec(&TokenAnswer::Token {
                token: 4,
                piece: "four".into(),
            })
            .unwrap(),
        );
        send_bytes(
            &label_peer,
            &serde_json::to_vec(&LabelAnswer::Ready).unwrap(),
        );
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: Some(&mut gate),
            decode: Some(&mut decode),
            label: Some(&mut label),
            io: &mut io,
        };
        let sources = progress(
            pump.step(&mut ports, None, &mut fixture.cx(), &deadlines)
                .unwrap(),
        );
        assert_eq!(
            sources,
            if round == 0 {
                vec![PumpSource::Gate, PumpSource::Decode, PumpSource::Label]
            } else {
                vec![PumpSource::Decode, PumpSource::Label, PumpSource::Gate]
            }
        );
        assert_eq!(
            gate.take_completion().unwrap().unwrap().exchange.ordinal,
            round
        );
        assert!(decode.take_completion().unwrap().is_some());
        assert!(label.take_completion().unwrap().is_some());
    }
    assert!(
        io.waits()
            .iter()
            .all(|wait| wait.absolute_deadline == Some(105))
    );
    let mut ports = PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: Some(&mut gate),
        decode: None,
        label: None,
        io: &mut io,
    };
    assert_eq!(
        pump.step(&mut ports, None, &mut fixture.cx(), &deadlines)
            .unwrap(),
        PumpStep::Deadline {
            key: PumpDeadline::Cut,
            bound: cut,
            observed_ns: 105,
        }
    );
    assert_eq!(deadlines.get(PumpDeadline::Cut), Some(cut));
}

#[test]
fn expired_bound_prevents_transport_even_when_lifecycle_is_ready() {
    let mut fixture = Fixture::new(false);
    let (admin, peer) = pair();
    let mut admin = LifecyclePeer::from_owned(admin).unwrap();
    send_envelope(&peer, 41);
    let mut io = FakePumpIo::new(107);
    let mut deadlines = DeadlineRegistry::new();
    let bound = AbsoluteBound::after(100, 5).unwrap();
    deadlines.arm(PumpDeadline::Enter, bound).unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = empty_ports(&mut io);
    ports.lifecycle.peer = Some(&mut admin);
    assert_eq!(
        pump.step(&mut ports, None, &mut fixture.cx(), &deadlines)
            .unwrap(),
        PumpStep::Deadline {
            key: PumpDeadline::Enter,
            bound,
            observed_ns: 107,
        }
    );
    assert!(admin.held().is_none());
    assert_eq!(
        admin.try_receive().unwrap(),
        ReceiveProgress::Held,
        "deadline must leave the incoming bytes with their owner"
    );
}

struct TickingIo {
    now: u64,
    reads: usize,
}
impl PumpIo for TickingIo {
    fn now(&mut self) -> io::Result<u64> {
        self.now += 1;
        self.reads += 1;
        Ok(self.now)
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("fixture has a queued lifecycle packet")
    }
}
#[test]
fn bound_is_checked_immediately_after_a_lifecycle_unit() {
    let mut fixture = Fixture::new(false);
    let (admin, peer) = pair();
    let mut admin = LifecyclePeer::from_owned(admin).unwrap();
    send_envelope(&peer, 51);
    let mut io = TickingIo { now: 100, reads: 0 };
    let bound = AbsoluteBound::after(100, 3).unwrap();
    let mut deadlines = DeadlineRegistry::new();
    deadlines.arm(PumpDeadline::Leave, bound).unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = empty_ports(&mut io);
    ports.lifecycle.peer = Some(&mut admin);
    assert_eq!(
        pump.step(&mut ports, None, &mut fixture.cx(), &deadlines)
            .unwrap(),
        PumpStep::Deadline {
            key: PumpDeadline::Leave,
            bound,
            observed_ns: 103,
        }
    );
    assert_eq!(
        io.reads, 3,
        "the post-unit check must report expiry before another unit or wait"
    );
    assert_eq!(admin.held().unwrap().envelope.exchange.ordinal, 51);
}

#[test]
fn held_terminal_failure_wins_over_a_new_ready_completion() {
    let mut fixture = Fixture::new(false);
    let (gate, gate_peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    let (decode, decode_peer) = pair();
    let mut decode = DecodePort::from_owned(decode).unwrap();
    send_envelope(&gate_peer, 61);
    assert_eq!(gate.try_receive().unwrap(), ReceiveProgress::Held);
    drop(gate_peer);
    send_bytes(
        &decode_peer,
        &serde_json::to_vec(&TokenAnswer::Token {
            token: 7,
            piece: "seven".into(),
        })
        .unwrap(),
    );
    let mut io = FakePumpIo::new(100);
    let mut ports = PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: Some(&mut gate),
        decode: Some(&mut decode),
        label: None,
        io: &mut io,
    };
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    assert!(matches!(
        pump.step(
            &mut ports,
            None,
            &mut fixture.cx(),
            &DeadlineRegistry::new()
        ),
        Err(PumpFailure::Transport {
            source: PumpSource::Gate,
            ..
        })
    ));
    assert!(decode.held().is_none());
    assert_eq!(
        gate.take_completion().unwrap().unwrap().exchange.ordinal,
        61
    );
    assert!(gate.take_completion().is_err());
    assert!(io.waits().is_empty());
}

#[test]
fn real_poll_preserves_the_actual_fd_and_held_slot_terminal_subscription() {
    let mut fixture = Fixture::new(false);
    let (gate, peer) = pair();
    let raw = gate.as_raw_fd();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    send_envelope(&peer, 71);
    let mut io = MonotonicIo;
    let origin = ClockOrigin::capture(&mut io).unwrap();
    let mut deadlines = DeadlineRegistry::new();
    deadlines
        .arm(
            PumpDeadline::Decision,
            AbsoluteBound::after(origin.raw(), 1_000_000_000).unwrap(),
        )
        .unwrap();
    let mut pump = CommonPump::new(origin);
    let mut ports = empty_ports(&mut io);
    ports.gate = Some(&mut gate);
    assert!(
        progress(
            pump.step(&mut ports, None, &mut fixture.cx(), &deadlines)
                .unwrap()
        )
        .contains(&PumpSource::Gate)
    );
    assert_eq!(gate.fd().as_raw_fd(), raw);
    assert!(!gate.interest().events().contains(PollFlags::POLLIN));
    assert!(gate.interest().events().contains(terminal_flags()));
    assert_eq!(gate.held().unwrap().exchange.ordinal, 71);
}

#[test]
fn a_real_pending_trace_author_does_not_reduce_or_replace_packet_completions() {
    let mut fixture = Fixture::new(true);
    let authored = AuthoredEvent {
        kind: Kind::MessageUser, subsystem: Subsystem::Harness, turn: Some(TurnRef("turn".into())), source: AuthoredSource::BoundaryInput(42), causal_parent: None,
        payload: Some(Payload::Message(serde_json::value::to_raw_value(&serde_json::json!({"role":"user","content":[{"type":"text","text":"Read the retained item."}]})).unwrap())),
    };
    assert!(matches!(
        fixture.barrier.submit_authored_with_clock(
            authored,
            RecordPurpose::Content,
            &fixture.author,
            1,
            &mut fixture.inbox,
            &mut fixture.budget,
            &mut || Ok(100)
        ),
        RecordProgress::Pending
    ));
    let before = serde_json::to_string(fixture.barrier.pending().unwrap()).unwrap();
    let ticket = fixture.barrier.pending_author().unwrap().ticket;
    let (gate, peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    send_envelope(&peer, 81);
    send_envelope(&peer, 82);
    let mut io = FakePumpIo::new(100);
    io.schedule(
        101,
        vec![ReadyFd {
            index: 0,
            events: PollFlags::POLLIN,
        }],
    )
    .unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = empty_ports(&mut io);
    ports.gate = Some(&mut gate);
    assert_eq!(
        progress(
            pump.step(
                &mut ports,
                None,
                &mut fixture.cx(),
                &DeadlineRegistry::new()
            )
            .unwrap()
        ),
        vec![PumpSource::Gate]
    );
    assert_eq!(gate.held().unwrap().exchange.ordinal, 81);
    assert_eq!(gate.try_receive().unwrap(), ReceiveProgress::Held);
    assert_eq!(
        serde_json::to_string(fixture.barrier.pending().unwrap()).unwrap(),
        before
    );
    assert_eq!(fixture.barrier.pending_author().unwrap().ticket, ticket);
    let mut reader = fixture.reader.take().unwrap();
    let drain = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    // Only the caller reduces the held completion. The second stays in the fd
    // until that explicit transfer, even after trace capacity returns.
    assert_eq!(
        gate.take_completion().unwrap().unwrap().exchange.ordinal,
        81
    );
    assert_eq!(gate.try_receive().unwrap(), ReceiveProgress::Held);
    assert_eq!(
        gate.take_completion().unwrap().unwrap().exchange.ordinal,
        82
    );
    drop(fixture);
    assert!(!drain.join().unwrap().is_empty());
}

#[test]
fn common_origin_and_monotonic_order_are_checked_before_transport() {
    let mut fixture = Fixture::new(false);
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut io = FakePumpIo::new(99);
    assert!(matches!(
        pump.step(
            &mut empty_ports(&mut io),
            None,
            &mut fixture.cx(),
            &DeadlineRegistry::new()
        ),
        Err(PumpFailure::Clock(_))
    ));
    assert!(io.waits().is_empty());
}

#[test]
fn standing_packet_failure_cannot_disappear_with_its_empty_interest_set() {
    let mut fixture = Fixture::new(false);
    let (gate, peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    send_bytes(&peer, b"not an envelope");
    assert!(gate.try_receive().is_err());
    assert!(gate.held().is_none());
    assert!(gate.interest().events().is_empty());
    let mut io = FakePumpIo::new(100);
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = empty_ports(&mut io);
    ports.gate = Some(&mut gate);
    assert!(matches!(
        pump.step(
            &mut ports,
            None,
            &mut fixture.cx(),
            &DeadlineRegistry::new()
        ),
        Err(PumpFailure::Transport {
            source: PumpSource::Gate,
            ..
        })
    ));
    assert!(io.waits().is_empty());
}

#[test]
fn interrupted_lifecycle_work_is_local_before_any_wait() {
    let (admin, _peer) = pair();
    let mut admin = LifecyclePeer::from_owned(admin).unwrap();
    let mut io = FakePumpIo::new(100);
    // This is the exact retained state after a recvmsg EINTR. The following
    // real receive resolves EAGAIN, without requiring a new readable packet.
    admin.interrupted = true;
    let mut ports = empty_ports(&mut io);
    ports.lifecycle.peer = Some(&mut admin);
    assert!(local_work(PumpSource::Admin, &ports, None, true));
    assert_eq!(
        ports
            .lifecycle
            .peer
            .as_mut()
            .unwrap()
            .try_receive()
            .unwrap(),
        ReceiveProgress::Pending
    );
    assert!(!local_work(PumpSource::Admin, &ports, None, true));
}

#[test]
fn lifecycle_sends_progress_during_a_round_with_other_immediate_work() {
    let mut fixture = Fixture::new(false);
    let (admin, admin_peer) = pair();
    let mut admin = LifecyclePeer::from_owned(admin).unwrap();
    let (residency, residency_peer) = pair();
    let mut residency = OrganPort::from_owned(residency).unwrap();
    let (decode, decode_peer) = pair();
    let mut decode = DecodePort::from_owned(decode).unwrap();
    send_envelope(&admin_peer, 91);
    send_envelope(&residency_peer, 92);
    // A decode series has started but cannot complete this round.
    let body = serde_json::to_vec(&TokenAnswer::Token {
        token: 1,
        piece: "x".repeat(MAX_ENVELOPE_BYTES),
    })
    .unwrap();
    send_bytes(
        &decode_peer,
        &serde_json::to_vec(&weaver_types::SegmentPreamble {
            segments: 2,
            bytes: body.len() as u64,
        })
        .unwrap(),
    );
    send_bytes(&decode_peer, &body[..MAX_ENVELOPE_BYTES]);
    assert!(matches!(admin.stage(envelope(93)).unwrap(), Stage::Queued));
    assert!(matches!(
        residency.stage(envelope(94)).unwrap(),
        Stage::Queued
    ));
    let mut io = FakePumpIo::new(100);
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = PumpPorts {
        lifecycle: LifecyclePorts {
            listener: None,
            peer: Some(&mut admin),
            residency: Some(&mut residency),
            dial: None,
            leave_peer: None,
        },
        gate: None,
        decode: Some(&mut decode),
        label: None,
        io: &mut io,
    };
    for _ in 0..2 {
        progress(
            pump.step(
                &mut ports,
                None,
                &mut fixture.cx(),
                &DeadlineRegistry::new(),
            )
            .unwrap(),
        );
    }
    assert!(io.waits().is_empty());
    assert!(decode.held().is_none());
    assert_eq!(admin.held().unwrap().envelope.exchange.ordinal, 91);
    assert_eq!(residency.held().unwrap().exchange.ordinal, 92);
    for (peer, expected) in [(&admin_peer, 93), (&residency_peer, 94)] {
        let mut bytes = [0; MAX_ENVELOPE_BYTES];
        let count = nix::sys::socket::recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT)
            .expect("lifecycle send must not wait for buffered traffic to end");
        let received: OrganEnvelope = serde_json::from_slice(&bytes[..count]).unwrap();
        assert_eq!(received.exchange.ordinal, expected);
    }
}

#[test]
fn malformed_lifecycle_envelope_retains_the_actual_decode_detail() {
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    let bytes = br#"{"exchange": false}"#;
    let expected = serde_json::from_slice::<OrganEnvelope>(bytes).unwrap_err();
    send_bytes(&peer, bytes);
    let failure = owner.try_receive().unwrap_err();
    assert_eq!(
        failure.detail,
        crate::channel::undecodable_detail("coordination", &expected)
    );
    assert_eq!(owner.wire.as_deref(), Some(bytes.as_slice()));
}

#[test]
fn an_already_held_gate_completion_is_not_progress_while_other_sources_are_ready() {
    let mut fixture = Fixture::new(false);
    let (gate, gate_peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    send_envelope(&gate_peer, 101);
    assert_eq!(gate.try_receive().unwrap(), ReceiveProgress::Held);
    // Another answer in the real socket must stay unread while the first is held.
    send_envelope(&gate_peer, 102);
    let (admin, admin_peer) = pair();
    let mut admin = LifecyclePeer::from_owned(admin).unwrap();
    send_envelope(&admin_peer, 91);
    let mut io = FakePumpIo::new(100);
    let bound = AbsoluteBound::after(100, 5).unwrap();
    let mut deadlines = DeadlineRegistry::new();
    deadlines.arm(PumpDeadline::Leave, bound).unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = empty_ports(&mut io);
    ports.lifecycle.peer = Some(&mut admin);
    ports.gate = Some(&mut gate);
    // Real admin readiness causes the pump to probe every standing source.
    let sources = progress(
        pump.step(&mut ports, None, &mut fixture.cx(), &deadlines)
            .unwrap(),
    );
    assert!(sources.contains(&PumpSource::Admin));
    assert!(!sources.contains(&PumpSource::Gate));
    assert_eq!(
        ports
            .gate
            .as_ref()
            .unwrap()
            .held()
            .unwrap()
            .exchange
            .ordinal,
        101
    );
    // Once the other source is held too, neither held answer causes a spin.
    // A real trace-capacity notification may race the next blocking wait.
    let mut expired = false;
    for _ in 0..8 {
        match pump
            .step(&mut ports, None, &mut fixture.cx(), &deadlines)
            .unwrap()
        {
            PumpStep::Deadline {
                key,
                bound: observed,
                observed_ns,
            } => {
                assert_eq!(key, PumpDeadline::Leave);
                assert_eq!(observed, bound);
                assert_eq!(observed_ns, bound.deadline_ns());
                expired = true;
                break;
            }
            PumpStep::Progress { sources } => assert!(!sources.contains(&PumpSource::Gate)),
            other => panic!("held owners must wait under the original bound: {other:?}"),
        }
    }
    assert!(
        expired,
        "held completions must not spin before the real wait"
    );
    assert_eq!(io.waits().len(), 1);
    assert_eq!(
        gate.take_completion().unwrap().unwrap().exchange.ordinal,
        101
    );
    assert_eq!(gate.try_receive().unwrap(), ReceiveProgress::Held);
    assert_eq!(
        gate.take_completion().unwrap().unwrap().exchange.ordinal,
        102
    );
}

#[test]
fn an_already_held_residency_completion_does_not_spin_as_new_progress() {
    let mut fixture = Fixture::new(false);
    let (residency, peer) = pair();
    let mut residency = OrganPort::from_owned(residency).unwrap();
    send_envelope(&peer, 101);
    assert_eq!(residency.try_receive().unwrap(), ReceiveProgress::Held);
    let mut io = FakePumpIo::new(100);
    let bound = AbsoluteBound::after(100, 5).unwrap();
    let mut deadlines = DeadlineRegistry::new();
    deadlines.arm(PumpDeadline::Leave, bound).unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = empty_ports(&mut io);
    ports.lifecycle.residency = Some(&mut residency);
    assert_eq!(
        pump.step(&mut ports, None, &mut fixture.cx(), &deadlines)
            .unwrap(),
        PumpStep::Deadline {
            key: PumpDeadline::Leave,
            bound,
            observed_ns: 105,
        }
    );
    assert_eq!(io.waits().len(), 1);
    assert_eq!(residency.held().unwrap().exchange.ordinal, 101);
}

#[test]
fn unheld_terminal_owner_precedes_another_sources_buffered_work() {
    let mut fixture = Fixture::new(false);
    let (gate, gate_peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    let (decode, decode_peer) = pair();
    let mut decode = DecodePort::from_owned(decode).unwrap();
    drop(gate_peer);
    send_bytes(
        &decode_peer,
        &serde_json::to_vec(&TokenAnswer::Token {
            token: 9,
            piece: "nine".into(),
        })
        .unwrap(),
    );
    let mut io = FakePumpIo::new(100);
    io.schedule(
        101,
        vec![ReadyFd {
            index: 1,
            events: PollFlags::POLLIN,
        }],
    )
    .unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    pump.cursor = 1; // Decode would be first in the ordinary rotation.
    let mut ports = PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: Some(&mut gate),
        decode: Some(&mut decode),
        label: None,
        io: &mut io,
    };
    assert!(matches!(
        pump.step(
            &mut ports,
            None,
            &mut fixture.cx(),
            &DeadlineRegistry::new()
        ),
        Err(PumpFailure::Transport {
            source: PumpSource::Gate,
            ..
        })
    ));
    assert!(decode.held().is_none());
    assert!(
        io.waits().is_empty(),
        "terminal hint must be serviced before waiting for ordinary work"
    );
}

#[test]
fn terminal_hint_preserves_a_complete_answer_before_reporting_close() {
    let mut fixture = Fixture::new(false);
    let (gate, peer) = pair();
    let mut gate = OrganPort::from_owned(gate).unwrap();
    send_envelope(&peer, 111);
    drop(peer);
    let mut io = FakePumpIo::new(100);
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    let mut ports = empty_ports(&mut io);
    ports.gate = Some(&mut gate);
    assert!(
        progress(
            pump.step(
                &mut ports,
                None,
                &mut fixture.cx(),
                &DeadlineRegistry::new()
            )
            .unwrap()
        )
        .contains(&PumpSource::Gate)
    );
    assert!(matches!(
        pump.step(
            &mut ports,
            None,
            &mut fixture.cx(),
            &DeadlineRegistry::new()
        ),
        Err(PumpFailure::Transport {
            source: PumpSource::Gate,
            ..
        })
    ));
    assert_eq!(
        gate.take_completion().unwrap().unwrap().exchange.ordinal,
        111
    );
    assert!(gate.take_completion().is_err());
}

#[test]
fn real_silent_socket_wait_expires_at_the_original_bound_with_a_watchdog() {
    let (end, peer) = pair();
    let (finished, result) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let mut fixture = Fixture::new(false);
        let mut gate = OrganPort::from_owned(end).unwrap();
        let mut io = MonotonicIo;
        let origin = ClockOrigin::capture(&mut io).unwrap();
        // This duration is only an outer transport-test fixture, not a
        // deployment election or a replacement for measured state-flow values.
        let bound = AbsoluteBound::after(origin.raw(), 50_000_000).unwrap();
        let mut deadlines = DeadlineRegistry::new();
        deadlines.arm(PumpDeadline::Gate, bound).unwrap();
        let mut pump = CommonPump::new(origin);
        loop {
            let mut ports = empty_ports(&mut io);
            ports.gate = Some(&mut gate);
            match pump
                .step(&mut ports, None, &mut fixture.cx(), &deadlines)
                .unwrap()
            {
                PumpStep::Deadline {
                    key,
                    bound: observed,
                    observed_ns,
                } => {
                    assert_eq!(key, PumpDeadline::Gate);
                    assert_eq!(observed, bound);
                    assert!(observed_ns >= bound.deadline_ns());
                    assert!(io.now().unwrap() >= observed_ns);
                    assert!(gate.held().is_none());
                    finished.send(()).unwrap();
                    break;
                }
                PumpStep::Progress { .. } => {
                    // A trace-capacity wake can race this silent wait. It must
                    // not renew or replace the already armed absolute bound.
                    assert_eq!(deadlines.get(PumpDeadline::Gate), Some(bound));
                }
                PumpStep::Idle => panic!("the silent gate remains standing"),
                PumpStep::Observation => panic!("no flow owner has an observation tick"),
            }
        }
    });
    if result.recv_timeout(Duration::from_secs(2)).is_err() {
        // Wake an accidental unbounded poll before joining the test worker.
        drop(peer);
        let _ = worker.join();
        panic!("common pump failed its real absolute deadline before watchdog");
    }
    worker.join().unwrap();
}

struct LastTraceIo {
    ticking: TickingIo,
    trace_fd: RawFd,
    waits: usize,
}

impl PumpIo for LastTraceIo {
    fn now(&mut self) -> io::Result<u64> {
        self.ticking.now()
    }

    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        absolute_deadline: Option<u64>,
    ) -> io::Result<WaitOutcome> {
        self.waits += 1;
        assert_eq!(interests.len(), 1, "trace must be the last and only source");
        assert_eq!(interests[0].fd.as_raw_fd(), self.trace_fd);
        assert_eq!(interests[0].events, PollFlags::POLLIN);
        assert_eq!(absolute_deadline, Some(103));
        Ok(WaitOutcome::Ready(vec![ReadyFd {
            index: 0,
            events: PollFlags::POLLIN,
        }]))
    }
}

#[test]
fn final_trace_unit_cannot_return_progress_after_the_absolute_bound() {
    let mut fixture = Fixture::new(false);
    // The real writer emits a queue-to-empty wake after this fixture's load.
    // Observe it without consuming it so the pump must do real work below.
    let watchdog = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let mut descriptor = [nix::poll::PollFd::new(
            fixture.barrier.capacity_fd(),
            PollFlags::POLLIN,
        )];
        let count = nix::poll::poll(&mut descriptor, 0u8).unwrap();
        if count == 1 {
            assert_eq!(descriptor[0].revents(), Some(PollFlags::POLLIN));
            break;
        }
        assert!(
            std::time::Instant::now() < watchdog,
            "trace drain did not wake"
        );
        std::thread::yield_now();
    }
    let mut io = LastTraceIo {
        ticking: TickingIo { now: 100, reads: 0 },
        trace_fd: fixture.barrier.capacity_fd().as_raw_fd(),
        waits: 0,
    };
    let bound = AbsoluteBound::after(100, 3).unwrap();
    let mut deadlines = DeadlineRegistry::new();
    deadlines.arm(PumpDeadline::Leave, bound).unwrap();
    let mut pump = CommonPump::new(ClockOrigin::at(100));
    assert_eq!(
        pump.step(
            &mut empty_ports(&mut io),
            None,
            &mut fixture.cx(),
            &deadlines
        )
        .unwrap(),
        PumpStep::Deadline {
            key: PumpDeadline::Leave,
            bound,
            observed_ns: 103,
        },
        "there is no next unit whose precheck can hide a missing post-unit check"
    );
    assert_eq!(io.waits, 1);
    assert_eq!(io.ticking.reads, 3);
    assert_eq!(
        fixture
            .barrier
            .try_capacity_wake(&mut fixture.budget)
            .unwrap(),
        weaver_trace::serving::CapacityWake::Empty,
        "the final unit must have consumed the real wake before reporting expiry"
    );
}

fn demand_opener() -> weaver_types::state_flow::capture::CaptureEnvelope {
    use weaver_types::state_flow::capture::*;
    CaptureEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 19,
        },
        position: Position::Open,
        payload: CapturePayload::Directive(CaptureDirective::SavePoint {
            cause: weaver_types::Cause { uid: 74 },
            window: weaver_types::state_flow::SaveWindow {
                began_ns: 100.into(),
                deadline_ns: 200.into(),
            },
        }),
    }
}

#[test]
fn serving_opener_keeps_the_original_save_bytes_and_the_single_receive_slot() {
    use weaver_types::state_flow::capture::*;
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    owner.enable_serving_openers().unwrap();
    assert!(owner.enable_serving_openers().is_err());
    let request = demand_opener();
    let bytes = serde_json::to_vec_pretty(&request).unwrap();
    send_bytes(&peer, &bytes);
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    assert!(owner.held().is_none());
    assert!(owner.capture_identity().is_none());
    assert!(owner.enable_capture().is_err());
    send_bytes(
        &peer,
        &capture_message(7).encode(CaptureSender::Admin).unwrap(),
    );
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Pending);
    let retained = owner.take_capture().unwrap().unwrap();
    assert_eq!(retained.message.envelope, request);
    assert_eq!(retained.wire, bytes);
    owner.enable_capture().unwrap();
    assert!(owner.capture_identity().is_some());
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    assert_eq!(
        owner.take_capture().unwrap().unwrap().message.envelope,
        capture_message(7)
    );
}

#[test]
fn serving_opener_keeps_ordinary_directives_and_refuses_legacy_save_downgrade() {
    for save in [false, true] {
        let (end, peer) = pair();
        let mut owner = LifecyclePeer::from_owned(end).unwrap();
        owner.enable_serving_openers().unwrap();
        let message = weaver_types::OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 19,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(if save {
                weaver_types::LifecycleDirective::SavePoint {
                    cause: weaver_types::Cause { uid: 74 },
                }
            } else {
                weaver_types::LifecycleDirective::Observe
            }),
        };
        let wire = serde_json::to_vec(&message).unwrap();
        send_bytes(&peer, &wire);
        if save {
            assert!(owner.try_receive().is_err());
            let failed = owner.retire_failed();
            assert_eq!(failed.wire.as_ref(), Some(&wire));
            assert!(failed.held.is_none() && failed.capture_held.is_none());
        } else {
            assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
            assert_eq!(owner.held().unwrap().envelope, message);
            assert!(owner.capture_held().is_none());
        }
    }
}

#[test]
fn serving_opener_selection_never_reinterprets_an_already_received_legacy_packet() {
    let (end, peer) = pair();
    let mut owner = LifecyclePeer::from_owned(end).unwrap();
    send_envelope(&peer, 1);
    assert_eq!(owner.try_receive().unwrap(), ReceiveProgress::Held);
    assert!(owner.enable_serving_openers().is_err());
    assert_eq!(owner.held().unwrap().envelope.exchange.ordinal, 1);
    assert!(owner.capture_held().is_none());
}
