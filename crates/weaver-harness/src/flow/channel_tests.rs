//! Synthetic transport peers exercise custody, not STE100 qualification or SQLite.

use super::*;
use std::io::{Read, Write};
use weaver_types::state_flow::*;

fn assert_same_answer(actual: &StateFrame, expected: &StateFrame) {
    assert_eq!(
        encode_state_line(actual, FrameDirection::Response).unwrap(),
        encode_state_line(expected, FrameDirection::Response).unwrap()
    );
}

struct Peer {
    channel: StateChannel,
    stream: UnixStream,
    now: u64,
}

impl Peer {
    fn new() -> Self {
        let (stream, peer) = UnixStream::pair().unwrap();
        nix::sys::socket::setsockopt(&stream, nix::sys::socket::sockopt::SndBuf, &4096).unwrap();
        peer.set_nonblocking(true).unwrap();
        Self {
            channel: StateChannel::new(stream, "session".into(), "run".into()).unwrap(),
            stream: peer,
            now: 10,
        }
    }

    fn step(&mut self) -> Result<(), ChannelFailure> {
        self.now += 10;
        self.channel.progress(&mut || Ok(self.now))
    }

    fn drain(&mut self, bytes: &mut Vec<u8>) {
        let mut chunk = [0; 8192];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => panic!("channel unexpectedly closed"),
                Ok(count) => bytes.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
                Err(error) => panic!("peer read: {error}"),
            }
        }
    }

    fn send_request(&mut self, body: StateRequest) -> StateFrame {
        self.channel.begin(body, self.now).unwrap();
        self.finish_send()
    }

    fn finish_send(&mut self) -> StateFrame {
        let mut bytes = Vec::new();
        for _ in 0..4096 {
            self.step().unwrap();
            self.drain(&mut bytes);
            if self
                .channel
                .outstanding()
                .iter()
                .all(|entry| entry.bytes_sent == entry.frame_bytes)
            {
                return decode_state_line(&bytes, FrameDirection::Request).unwrap();
            }
        }
        panic!("fixture send did not finish");
    }

    fn write_answer(&mut self, answer: &StateFrame) {
        let bytes = encode_state_line(answer, FrameDirection::Response).unwrap();
        self.stream.write_all(&bytes).unwrap();
        self.step().unwrap();
        assert!(self.channel.held_measurement().is_some());
    }

    fn release(&mut self) -> StateCompletion {
        let proof = RecordedStateMeasurement::for_test(self.channel.held_measurement().unwrap());
        self.channel
            .release(proof)
            .unwrap()
            .expect("release is ready")
    }

    fn open(&mut self) {
        let request = self.send_request(open_request());
        let answer = self.answer(&request, None);
        self.write_answer(&answer);
        assert!(self.channel.opened_identity().is_some());
        assert!(matches!(self.release().answer, StateFrame::Opened { .. }));
    }

    fn observation(&self, request: &StateFrame) -> StateOperationObservation {
        StateOperationObservation {
            operation_id: self.channel.next_operation.unwrap().into(),
            parent_id: None,
            request_id: request.common().request_id,
            operation: request.request_operation().unwrap(),
            began_ns: (self.now + 1).into(),
            ended_ns: (self.now + 2).into(),
            phases: vec![
                OperationPhase {
                    name: PhaseName::Validate,
                    began_ns: (self.now + 1).into(),
                    ended_ns: (self.now + 1).into(),
                },
                OperationPhase {
                    name: PhaseName::Encode,
                    began_ns: (self.now + 2).into(),
                    ended_ns: (self.now + 2).into(),
                },
            ],
            resource: OperationResource {
                cpu_ns: None,
                copied_bytes: Some(0.into()),
                held_bytes: Some(1.into()),
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

    fn answer(&self, request: &StateFrame, control: Option<StateControlAnswer>) -> StateFrame {
        let common = request.common().clone();
        let observation = self.observation(request);
        match request {
            StateFrame::Open {
                schema_id,
                scheme_id,
                start,
                read_limits,
                ..
            } => StateFrame::Opened {
                common,
                schema_id: schema_id.clone(),
                scheme_id: scheme_id.clone(),
                start: start.clone(),
                read_limits: read_limits.clone(),
                observation,
                member_observation: MemberObservationIdentity {
                    instrument_version: "state-observation/2".into(),
                    pid: 7,
                    start_ticks: Some(10.into()),
                    ticks_per_second: Some(100.into()),
                    unavailable_reason: None,
                },
            },
            StateFrame::File { filing, .. } => StateFrame::Filed {
                common,
                block_id: filing.block_id.clone(),
                observation,
            },
            StateFrame::FilePair { filing, .. } => StateFrame::Filed {
                common,
                block_id: filing.block_id.clone(),
                observation,
            },
            StateFrame::Control { .. } => StateFrame::ControlAnswer {
                common,
                frame: control.expect("typed control answer"),
                observation,
            },
            _ => panic!("response is not a request"),
        }
    }
}

fn open_request() -> StateRequest {
    StateRequest::Open {
        schema_id: STATE_CONTENT_SCHEMA.into(),
        scheme_id: "synthetic-scheme".into(),
        start: StartIdentity {
            seed_id: "seed".into(),
            artifact_id: "artifact".into(),
            dump_sha256: Digest::new("1".repeat(64)).unwrap(),
            config_sha256: Digest::new("2".repeat(64)).unwrap(),
            parent_artifact_id: None,
            position: None,
        },
        read_limits: ReadLimits {
            max_result_bytes: 1_000_000.into(),
            measurement_ref: "synthetic test limit".into(),
        },
    }
}

fn statement(text: &str) -> StatementFile {
    StatementFile {
        block_id: SourceBlockId {
            run: "run".into(),
            sequence: 3.into(),
            block: 0.into(),
        },
        source: SourceEnvelope {
            session: "session".into(),
            run: "run".into(),
            turn: Some("turn".into()),
            sequence: 3.into(),
            kind: "message.assistant".into(),
            channel: "output".into(),
            origin: "model".into(),
            subsystem: "harness".into(),
            causal_parent: None,
            wall_ms: 1,
            monotonic_ns: 2.into(),
        },
        partition_rule_id: "statement-rule".into(),
        source_path: vec![
            SourcePathPart::Key("content".into()),
            SourcePathPart::Index { index: 0.into() },
        ],
        admission: Admission::Admitted,
        // Spaces in RawValue deliberately survive the carrier unchanged.
        raw_block: serde_json::value::RawValue::from_string(format!(
            "{{ \"type\": \"text\", \"text\": {} }}",
            serde_json::to_string(text).unwrap()
        ))
        .unwrap(),
        text: text.into(),
        scheme_id: "synthetic-scheme".into(),
        bindings: vec![],
        context: None,
        effect: None,
    }
}

#[test]
fn fragmented_answer_stays_held_until_its_exact_recorded_measurement() {
    let mut peer = Peer::new();
    peer.open();
    let request = peer.send_request(StateRequest::File(statement("Read item.")));
    assert_eq!(request.common().request_id.0, 2);
    let answer = peer.answer(&request, None);
    let bytes = encode_state_line(&answer, FrameDirection::Response).unwrap();
    for fragment in bytes.chunks(7) {
        peer.stream.write_all(fragment).unwrap();
        peer.step().unwrap();
        if !fragment.ends_with(b"\n") {
            assert!(peer.channel.held_measurement().is_none());
        }
    }
    let held = peer.channel.held_measurement().unwrap().clone();
    assert_eq!(held.request_source, Some(statement("Read item.").block_id));
    assert!(!peer.channel.interest().readable);
    assert_eq!(
        peer.channel.outstanding()[0].commit,
        CommitEvidence::Committed
    );
    // Additional pump calls cannot publish or consume the held answer.
    peer.step().unwrap();
    assert_eq!(peer.channel.held_measurement(), Some(&held));
    let complete = peer.release();
    assert!(matches!(complete.answer, StateFrame::Filed { .. }));
    assert!(!complete.discard_capture_result);
    assert!(peer.channel.outstanding().is_empty());
    let next = peer.send_request(StateRequest::Control(StateControl::Held {}));
    assert_eq!(next.common().request_id.0, 3);
}

#[test]
fn recording_proof_cannot_release_another_measurement_or_run() {
    for wrong_run in [false, true] {
        let mut peer = Peer::new();
        let request = peer.send_request(open_request());
        let answer = peer.answer(&request, None);
        peer.write_answer(&answer);
        let mut measurement = peer.channel.held_measurement().unwrap().clone();
        let proof = if wrong_run {
            RecordedStateMeasurement::for_test_in("other-run", &measurement)
        } else {
            measurement.harness_received_ns.0 += 1;
            RecordedStateMeasurement::for_test(&measurement)
        };
        assert_eq!(
            peer.channel.release(proof).unwrap_err().kind,
            ChannelFailureKind::Protocol
        );
        assert!(peer.channel.held_measurement().is_some());
    }
}

#[test]
fn new_exchange_and_duplicate_recording_ticket_cannot_pass_held_slot() {
    let mut peer = Peer::new();
    let request = peer.send_request(open_request());
    let answer = peer.answer(&request, None);
    peer.write_answer(&answer);
    let failure = peer
        .channel
        .begin(StateRequest::Control(StateControl::Held {}), peer.now)
        .unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Protocol);
    assert_eq!(failure.outstanding.unwrap().common.request_id.0, 1);

    let mut peer = Peer::new();
    let request = peer.send_request(open_request());
    peer.write_answer(&peer.answer(&request, None));
    let duplicate = RecordedStateMeasurement::for_test(peer.channel.held_measurement().unwrap());
    peer.release();
    assert_eq!(
        peer.channel.release(duplicate).unwrap_err().kind,
        ChannelFailureKind::Protocol
    );
}

#[test]
fn partial_writes_keep_exact_bytes_and_original_send_time_without_resend() {
    let mut peer = Peer::new();
    peer.open();
    let body = StateRequest::File(statement(&"a".repeat(150_000)));
    peer.channel.begin(body, peer.now).unwrap();
    let expected = peer.channel.exchanges[0].bytes.clone();
    peer.step().unwrap();
    let first = peer.channel.outstanding()[0].clone();
    assert!(first.bytes_sent > 0 && first.bytes_sent < first.frame_bytes);
    assert_eq!(first.commit, CommitEvidence::NotCommitted);
    let first_sent = peer.channel.exchanges[0].sent_ns;
    let offset = first.bytes_sent;
    // Saturate the actual socket: progress must preserve the exact offset on EAGAIN.
    for _ in 0..8 {
        peer.step().unwrap();
    }
    assert_eq!(peer.channel.outstanding()[0].bytes_sent, offset);
    assert_eq!(peer.channel.exchanges[0].sent_ns, first_sent);
    let mut bytes = Vec::new();
    for _ in 0..4096 {
        peer.drain(&mut bytes);
        peer.step().unwrap();
        if peer.channel.outstanding()[0].bytes_sent == expected.len() {
            break;
        }
    }
    peer.drain(&mut bytes);
    assert_eq!(bytes, expected);
    assert_eq!(
        peer.channel.outstanding()[0].commit,
        CommitEvidence::Unknown
    );
    let request = decode_state_line(&bytes, FrameDirection::Request).unwrap();
    peer.write_answer(&peer.answer(&request, None));
    assert_eq!(
        peer.channel.held_measurement().unwrap().harness_sent_ns.0,
        first_sent.unwrap()
    );
    peer.release();
}

#[test]
fn premature_unsolicited_and_duplicate_answers_are_terminal() {
    let mut unsolicited = Peer::new();
    unsolicited.stream.write_all(b"x").unwrap();
    assert_eq!(
        unsolicited.step().unwrap_err().kind,
        ChannelFailureKind::Protocol
    );

    let mut early = Peer::new();
    early.open();
    early
        .channel
        .begin(
            StateRequest::File(statement(&"a".repeat(100_000))),
            early.now,
        )
        .unwrap();
    early.step().unwrap();
    let request = early.channel.exchanges[0].request.clone();
    assert!(!early.channel.exchanges[0].fully_sent());
    let answer = early.answer(&request, None);
    early
        .stream
        .write_all(&encode_state_line(&answer, FrameDirection::Response).unwrap())
        .unwrap();
    let failure = early.step().unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Protocol);
    assert_eq!(
        failure.outstanding.unwrap().commit,
        CommitEvidence::NotCommitted
    );

    for duplicate_after_held in [false, true] {
        let mut peer = Peer::new();
        let request = peer.send_request(open_request());
        let bytes =
            encode_state_line(&peer.answer(&request, None), FrameDirection::Response).unwrap();
        peer.stream.write_all(&bytes).unwrap();
        if duplicate_after_held {
            peer.step().unwrap();
        }
        peer.stream.write_all(&bytes).unwrap();
        let failure = if duplicate_after_held {
            let proof =
                RecordedStateMeasurement::for_test(peer.channel.held_measurement().unwrap());
            peer.channel.release(proof).unwrap_err()
        } else {
            peer.step().unwrap_err()
        };
        assert_eq!(failure.kind, ChannelFailureKind::Protocol);
    }
}

#[test]
fn wrong_identity_source_operation_or_observation_never_establishes_commit() {
    for defect in 0..7 {
        let mut peer = Peer::new();
        peer.open();
        let request = peer.send_request(StateRequest::File(statement("Read item.")));
        let mut answer = peer.answer(&request, None);
        let StateFrame::Filed {
            common,
            block_id,
            observation,
        } = &mut answer
        else {
            unreachable!()
        };
        match defect {
            0 => common.request_id.0 += 1,
            1 => common.run_id = "other".into(),
            2 => block_id.sequence.0 += 1,
            3 => observation.operation_id.0 += 1,
            4 => observation.operation = StateOperation::Read,
            5 => observation.phases.clear(),
            6 => {
                observation.began_ns = 1.into();
                observation.ended_ns = 2.into();
                observation.phases[0].began_ns = 1.into();
                observation.phases[0].ended_ns = 1.into();
                observation.phases[1].began_ns = 2.into();
                observation.phases[1].ended_ns = 2.into();
            }
            _ => unreachable!(),
        }
        peer.stream
            .write_all(&encode_state_line(&answer, FrameDirection::Response).unwrap())
            .unwrap();
        let failure = peer.step().unwrap_err();
        assert_eq!(
            failure.kind,
            ChannelFailureKind::Protocol,
            "defect {defect}"
        );
        assert_eq!(
            failure.outstanding.as_ref().unwrap().commit,
            CommitEvidence::Unknown
        );
        assert!(peer.channel.held_measurement().is_none());
        assert_eq!(peer.step().unwrap_err(), failure);
    }
}

#[test]
fn valid_refusal_is_measured_before_terminal_filing_failure() {
    let mut peer = Peer::new();
    peer.open();
    let request = peer.send_request(StateRequest::File(statement("Read item.")));
    let answer = StateFrame::Refused {
        common: request.common().clone(),
        code: StateRefusalCode::WriteFailed,
        detail: "synthetic transaction refusal".into(),
        block_id: Some(statement("Read item.").block_id),
        observation: peer.observation(&request),
    };
    peer.write_answer(&answer);
    assert!(peer.channel.failure().is_none());
    assert_eq!(
        peer.channel.outstanding()[0].commit,
        CommitEvidence::Refused
    );
    let held = peer.channel.held_measurement().unwrap().clone();
    // The real member closes after refusing. Preserve that exact failure reason.
    peer.stream.shutdown(std::net::Shutdown::Both).unwrap();
    peer.step().unwrap();
    assert!(peer.channel.failure().is_none());
    assert!(peer.channel.interest().events().is_empty());
    let proof = RecordedStateMeasurement::for_test(&held);
    let failure = peer.channel.release(proof).unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::FilingRefused);
    assert_eq!(failure.outstanding.unwrap().commit, CommitEvidence::Refused);
    assert_eq!(peer.channel.held_measurement(), Some(&held));
}

#[test]
fn lost_commit_receipt_is_unknown_and_never_reconnected_or_resent() {
    let mut peer = Peer::new();
    peer.open();
    let request = peer.send_request(StateRequest::File(statement("Read item.")));
    // The peer has the complete frame and could have committed. No receipt crossed.
    let common = request.common().clone();
    peer.stream.shutdown(std::net::Shutdown::Both).unwrap();
    let failure = peer.step().unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Carry);
    let progress = failure.outstanding.as_ref().unwrap();
    assert_eq!(progress.common, common);
    assert_eq!(progress.bytes_sent, progress.frame_bytes);
    assert_eq!(progress.commit, CommitEvidence::Unknown);
    assert_eq!(
        peer.channel
            .begin(StateRequest::Control(StateControl::Held {}), peer.now)
            .unwrap_err(),
        failure
    );
    assert!(!peer.channel.interest().readable && !peer.channel.interest().writable);
}

#[test]
fn peer_death_after_valid_receipt_retains_committed_evidence_without_release() {
    let mut peer = Peer::new();
    peer.open();
    let request = peer.send_request(StateRequest::File(statement("Read item.")));
    peer.write_answer(&peer.answer(&request, None));
    let held = peer.channel.held_measurement().unwrap().clone();
    let proof = RecordedStateMeasurement::for_test(&held);
    peer.stream.shutdown(std::net::Shutdown::Both).unwrap();
    let failure = peer.channel.release(proof).unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Carry);
    assert_eq!(
        failure.outstanding.unwrap().commit,
        CommitEvidence::Committed
    );
    assert_eq!(peer.channel.held_measurement(), Some(&held));
    assert_eq!(peer.channel.outstanding().len(), 1);
}

#[test]
fn held_answer_half_close_wakes_common_poll_without_reading_another_answer() {
    use crate::flow::clock::{MonotonicIo, PumpIo, WaitInterest, WaitOutcome};

    let mut peer = Peer::new();
    peer.open();
    let request = peer.send_request(StateRequest::File(statement("Read item.")));
    let answer = peer.answer(&request, None);
    peer.write_answer(&answer);
    let held = peer.channel.held_measurement().unwrap().clone();
    let extra = encode_state_line(&answer, FrameDirection::Response).unwrap();
    peer.stream.write_all(&extra).unwrap();

    let events = peer.channel.interest().events();
    assert!(!events.contains(PollFlags::POLLIN));
    assert_ne!(events.bits() & libc::POLLRDHUP, 0);
    let mut status = libc::pollfd {
        fd: peer.channel.fd().as_raw_fd(),
        events: events.bits(),
        revents: 0,
    };
    // SAFETY: status describes one live fixture fd and the call never waits.
    assert_eq!(unsafe { libc::poll(&mut status, 1, 0) }, 0);
    peer.step().unwrap();
    assert!(peer.channel.received.is_empty());
    assert_eq!(peer.channel.held_measurement(), Some(&held));

    peer.stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut io = MonotonicIo;
    let deadline = io.now().unwrap().checked_add(1_000_000_000).unwrap();
    let wake = io
        .wait(
            &[WaitInterest {
                fd: peer.channel.fd(),
                events,
            }],
            Some(deadline),
        )
        .unwrap();
    let WaitOutcome::Ready(ready) = wake else {
        panic!("a held state's half-close must wake the common wait");
    };
    assert_eq!(ready.len(), 1);
    assert_ne!(ready[0].events.bits() & libc::POLLRDHUP, 0);
    let failure = peer.step().unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Carry);
    assert_eq!(peer.channel.held_measurement(), Some(&held));
    assert_same_answer(&peer.channel.held.as_ref().unwrap().answer, &answer);
    assert!(peer.channel.received.is_empty());
    let mut untouched = vec![0; extra.len()];
    assert_eq!(
        recv(
            peer.channel.fd().as_raw_fd(),
            &mut untouched,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK,
        ),
        Ok(extra.len())
    );
    assert_eq!(untouched, extra);
    let proof = RecordedStateMeasurement::for_test(&held);
    assert_eq!(peer.channel.release(proof).unwrap_err(), failure);
    assert_eq!(peer.channel.failure(), Some(&failure));
    assert_eq!(peer.channel.outstanding().len(), 1);
    assert!(peer.channel.interest().events().is_empty());
    assert!(!peer.channel.needs_progress());
}

#[test]
fn held_success_observes_full_or_half_close_without_a_release_attempt() {
    for shutdown in [std::net::Shutdown::Both, std::net::Shutdown::Write] {
        let mut peer = Peer::new();
        let request = peer.send_request(open_request());
        let answer = peer.answer(&request, None);
        peer.write_answer(&answer);
        peer.stream.shutdown(shutdown).unwrap();
        let held = peer.channel.held_measurement().unwrap().clone();
        let failure = peer.channel.check_held_terminal().unwrap_err();
        assert_eq!(failure.kind, ChannelFailureKind::Carry);
        assert_eq!(peer.channel.held_measurement(), Some(&held));
        assert_same_answer(&peer.channel.held.as_ref().unwrap().answer, &answer);
        assert!(!peer.channel.opened);
        assert_eq!(peer.step().unwrap_err(), failure);
        assert_eq!(
            peer.channel
                .release(RecordedStateMeasurement::for_test(&held))
                .unwrap_err(),
            failure
        );
    }
}

#[test]
fn complete_open_refusal_survives_peer_close_until_its_measurement_is_recorded() {
    for shutdown in [std::net::Shutdown::Both, std::net::Shutdown::Write] {
        let mut peer = Peer::new();
        let request = peer.send_request(open_request());
        let answer = StateFrame::Refused {
            common: request.common().clone(),
            code: StateRefusalCode::OpenFailed,
            detail: "synthetic open refusal".into(),
            block_id: None,
            observation: peer.observation(&request),
        };
        peer.stream
            .write_all(&encode_state_line(&answer, FrameDirection::Response).unwrap())
            .unwrap();
        peer.stream.shutdown(shutdown).unwrap();
        peer.step().unwrap();
        let held = peer.channel.held_measurement().unwrap().clone();
        peer.step().unwrap();
        assert!(peer.channel.failure().is_none());
        assert_same_answer(&peer.channel.held.as_ref().unwrap().answer, &answer);
        assert!(peer.channel.interest().events().is_empty());
        let failure = peer
            .channel
            .release(RecordedStateMeasurement::for_test(&held))
            .unwrap_err();
        assert_eq!(failure.kind, ChannelFailureKind::OpenRefused);
        assert!(failure.detail.contains("synthetic open refusal"));
        assert_eq!(peer.channel.held_measurement(), Some(&held));
        assert_eq!(peer.channel.failure(), Some(&failure));
    }
}

#[test]
fn interrupted_receive_cannot_send_final_request_bytes_past_a_premature_answer() {
    let mut peer = Peer::new();
    peer.channel.begin(open_request(), peer.now).unwrap();
    let request = peer.channel.exchanges[0].request.clone();
    let answer = peer.answer(&request, None);
    let bytes = encode_state_line(&answer, FrameDirection::Response).unwrap();
    peer.stream.write_all(&bytes).unwrap();
    let original = peer.channel.outstanding();
    peer.channel
        .progress_with(
            &mut || Ok(peer.now),
            |_, _| Err(nix::errno::Errno::EINTR),
            |_, _| panic!("an interrupted receive cannot authorize a send"),
        )
        .unwrap();
    assert!(peer.channel.needs_progress());
    assert!(!peer.channel.interest().writable);
    assert_eq!(peer.channel.outstanding(), original);
    assert!(peer.channel.failure().is_none());
    let failure = peer.step().unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Protocol);
    assert!(failure.detail.contains("before the complete request"));
    assert_eq!(peer.channel.received, bytes);
    assert_eq!(peer.channel.outstanding(), original);
    assert!(!peer.channel.needs_progress());
}

#[test]
fn interrupted_receive_and_send_resume_once_without_restarting_queue_or_send_clocks() {
    let mut peer = Peer::new();
    peer.channel.begin(open_request(), peer.now).unwrap();
    let queued = peer.channel.exchanges[0].queued_ns;
    peer.channel
        .progress_with(
            &mut || Ok(20),
            |_, _| Err(nix::errno::Errno::EINTR),
            |_, _| panic!("receive EINTR must yield before sending"),
        )
        .unwrap();
    assert!(peer.channel.needs_progress());
    // The next bounded receive proves EAGAIN; an interrupted send keeps its own
    // local work but still has no first-byte timestamp or byte progress.
    peer.channel
        .progress_with(
            &mut || Ok(30),
            |fd, bytes| recv(fd, bytes, MsgFlags::MSG_DONTWAIT),
            |_, _| Err(nix::errno::Errno::EINTR),
        )
        .unwrap();
    assert!(peer.channel.needs_progress());
    assert!(peer.channel.interest().writable);
    assert_eq!(peer.channel.exchanges[0].queued_ns, queued);
    assert_eq!(peer.channel.exchanges[0].sent_ns, None);
    assert_eq!(peer.channel.exchanges[0].offset, 0);
    peer.channel.progress(&mut || Ok(40)).unwrap();
    assert!(!peer.channel.needs_progress());
    assert_eq!(peer.channel.exchanges[0].queued_ns, queued);
    assert_eq!(peer.channel.exchanges[0].sent_ns, Some(40));
    assert!(peer.channel.exchanges[0].fully_sent());
    let mut bytes = Vec::new();
    peer.drain(&mut bytes);
    assert_eq!(bytes, peer.channel.exchanges[0].bytes);
}

#[test]
fn interrupted_held_status_keeps_local_work_without_consuming_or_failing_the_answer() {
    let mut peer = Peer::new();
    let request = peer.send_request(open_request());
    let answer = peer.answer(&request, None);
    peer.write_answer(&answer);
    let held = peer.channel.held_measurement().unwrap().clone();
    peer.channel
        .check_held_terminal_with(|_| Err(io::Error::from(io::ErrorKind::Interrupted)))
        .unwrap();
    assert!(peer.channel.needs_progress());
    assert!(peer.channel.failure().is_none());
    assert_eq!(peer.channel.held_measurement(), Some(&held));
    peer.channel.check_held_terminal().unwrap();
    assert!(!peer.channel.needs_progress());
    assert_same_answer(&peer.release().answer, &answer);
}

#[test]
fn interrupted_release_keeps_the_original_proof_until_one_successful_release() {
    let mut peer = Peer::new();
    let request = peer.send_request(open_request());
    let answer = peer.answer(&request, None);
    peer.write_answer(&answer);
    let held = peer.channel.held_measurement().unwrap().clone();
    let proof = RecordedStateMeasurement::for_test(&held);
    assert!(
        peer.channel
            .release_with(proof, |_| Err(nix::errno::Errno::EINTR))
            .unwrap()
            .is_none()
    );
    assert!(peer.channel.needs_progress());
    assert!(peer.channel.failure().is_none());
    assert_eq!(peer.channel.held_measurement(), Some(&held));
    assert!(!peer.channel.opened);
    assert_eq!(peer.channel.outstanding().len(), 1);
    assert!(
        peer.channel
            .resume_release_with(|_| Err(nix::errno::Errno::EINTR))
            .unwrap()
            .is_none()
    );
    let retained = peer
        .channel
        .held
        .as_ref()
        .unwrap()
        .recorded
        .as_ref()
        .unwrap();
    assert_eq!(retained.run(), "run");
    assert_eq!(retained.measurement(), &held);
    // Transport may observe terminal status but cannot release the caller's slot.
    peer.step().unwrap();
    assert!(peer.channel.held_measurement().is_some());
    let completion = peer.channel.resume_release().unwrap().unwrap();
    assert_same_answer(&completion.answer, &answer);
    assert!(peer.channel.opened);
    assert!(peer.channel.outstanding().is_empty());
    assert!(!peer.channel.needs_progress());
    assert_eq!(
        peer.channel.resume_release().unwrap_err().kind,
        ChannelFailureKind::Protocol
    );
}

#[test]
fn interrupted_release_does_not_hide_a_queued_duplicate_or_later_eof() {
    for duplicate in [false, true] {
        let mut peer = Peer::new();
        let request = peer.send_request(open_request());
        let answer = peer.answer(&request, None);
        peer.write_answer(&answer);
        let held = peer.channel.held_measurement().unwrap().clone();
        if duplicate {
            peer.stream
                .write_all(&encode_state_line(&answer, FrameDirection::Response).unwrap())
                .unwrap();
        }
        assert!(
            peer.channel
                .release_with(RecordedStateMeasurement::for_test(&held), |_| {
                    Err(nix::errno::Errno::EINTR)
                })
                .unwrap()
                .is_none()
        );
        if !duplicate {
            peer.stream.shutdown(std::net::Shutdown::Write).unwrap();
        }
        let failure = peer.channel.resume_release().unwrap_err();
        assert_eq!(
            failure.kind,
            if duplicate {
                ChannelFailureKind::Protocol
            } else {
                ChannelFailureKind::Carry
            }
        );
        assert_eq!(peer.channel.held_measurement(), Some(&held));
        assert_same_answer(&peer.channel.held.as_ref().unwrap().answer, &answer);
        assert!(peer.channel.held.as_ref().unwrap().recorded.is_some());
        assert_eq!(peer.channel.outstanding().len(), 1);
        assert!(!peer.channel.opened);
        assert!(!peer.channel.needs_progress());
    }
}

#[test]
fn unsent_oversize_and_unterminated_answer_bounds_are_enforced() {
    let mut peer = Peer::new();
    peer.open();
    let failure = peer
        .channel
        .begin(
            StateRequest::File(statement(&"a".repeat(STATE_REQUEST_LINE_BOUND))),
            peer.now,
        )
        .unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Carry);
    assert_eq!(
        failure.outstanding.unwrap().commit,
        CommitEvidence::NotRequested
    );
    let mut bytes = Vec::new();
    peer.drain(&mut bytes);
    assert!(bytes.is_empty());

    let mut peer = Peer::new();
    peer.send_request(open_request());
    let chunk = [b' '; 8192];
    let mut failure = None;
    for _ in 0..STATE_ANSWER_LINE_BOUND / chunk.len() {
        peer.stream.write_all(&chunk).unwrap();
        if let Err(error) = peer.step() {
            failure = Some(error);
            break;
        }
    }
    assert_eq!(failure.unwrap().kind, ChannelFailureKind::Protocol);
    assert!(peer.channel.held_measurement().is_none());
}

#[test]
fn malformed_duplicate_json_keys_and_wrong_direction_are_terminal() {
    for bytes in [
        b"{bad}\n".as_slice(),
        b"{\"protocol\":\"state-flow/1\",\"protocol\":\"state-flow/1\"}\n".as_slice(),
    ] {
        let mut peer = Peer::new();
        peer.send_request(open_request());
        peer.stream.write_all(bytes).unwrap();
        assert_eq!(peer.step().unwrap_err().kind, ChannelFailureKind::Protocol);
    }
    let mut peer = Peer::new();
    let request = peer.send_request(open_request());
    peer.stream
        .write_all(&encode_state_line(&request, FrameDirection::Request).unwrap())
        .unwrap();
    assert_eq!(peer.step().unwrap_err().kind, ChannelFailureKind::Protocol);
}

#[test]
fn ordinary_controls_share_ids_and_valid_control_refusal_does_not_break_channel() {
    let mut peer = Peer::new();
    peer.open();
    let cases = [
        (StateControl::Shape {}, StateControlAnswer::Shape(vec![])),
        (
            StateControl::Held {},
            StateControlAnswer::Held { count: 4.into() },
        ),
        (
            StateControl::Position {},
            StateControlAnswer::Position(None),
        ),
        (
            StateControl::Grants {},
            StateControlAnswer::Grants {
                surface: vec!["sh".into()],
            },
        ),
        (
            StateControl::Read {
                selector: ReadSelector::SystemPrompt {},
            },
            StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Read(ReadRefusalCode::AnswerTooLarge),
                detail: "synthetic limit".into(),
            },
        ),
        (
            StateControl::Next {
                snapshot_id: 7.into(),
                offset: 3.into(),
            },
            StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Read(ReadRefusalCode::UnknownSnapshot),
                detail: "synthetic absent cursor".into(),
            },
        ),
        (
            StateControl::Release {
                snapshot_id: 7.into(),
            },
            StateControlAnswer::Released {
                snapshot_id: 7.into(),
            },
        ),
    ];
    for (index, (control, result)) in cases.into_iter().enumerate() {
        let request = peer.send_request(StateRequest::Control(control));
        assert_eq!(request.common().request_id.0, index as u64 + 2);
        let answer = peer.answer(&request, Some(result));
        peer.write_answer(&answer);
        peer.release();
        assert!(peer.channel.failure().is_none());
    }
}

#[test]
fn capture_continuations_keep_exchange_id_and_advance_observation_id() {
    let mut peer = Peer::new();
    peer.open();
    let snapshot = peer.send_request(StateRequest::Control(StateControl::Snapshot {
        capture_id: 9.into(),
    }));
    peer.write_answer(&peer.answer(
        &snapshot,
        Some(StateControlAnswer::Snapshot {
            capture_id: 9.into(),
            room_name: "9.part".into(),
            size_bytes: 11.into(),
            position: None,
        }),
    ));
    peer.release();
    let acknowledge = peer.send_request(StateRequest::Control(StateControl::Acknowledge {
        capture_id: 9.into(),
        dump_sha256: Digest::new("3".repeat(64)).unwrap(),
    }));
    assert_eq!(acknowledge.common(), snapshot.common());
    peer.write_answer(&peer.answer(
        &acknowledge,
        Some(StateControlAnswer::Finished {
            capture_id: 9.into(),
            room_name: "9.sqlite".into(),
        }),
    ));
    assert_eq!(
        peer.channel
            .held_measurement()
            .unwrap()
            .observation
            .operation_id
            .0,
        3
    );
    peer.release();
    let release = peer.send_request(StateRequest::Control(StateControl::ReleaseCapture {
        capture_id: 9.into(),
    }));
    assert_eq!(
        release.common().request_id.0,
        snapshot.common().request_id.0 + 1
    );
    peer.write_answer(&peer.answer(
        &release,
        Some(StateControlAnswer::CaptureReleased {
            capture_id: 9.into(),
        }),
    ));
    peer.release();
    assert!(peer.channel.capture.is_none());
}

#[test]
fn late_capture_answer_and_abort_each_require_recording_before_reuse() {
    for late_refusal in [false, true] {
        let mut peer = Peer::new();
        peer.open();
        let snapshot = peer.send_request(StateRequest::Control(StateControl::Snapshot {
            capture_id: 8.into(),
        }));
        peer.channel.abort_capture(peer.now).unwrap();
        let abort = peer.finish_send();
        assert_eq!(abort.common(), snapshot.common());
        let result = if late_refusal {
            StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Capture(CaptureRefusalCode::SnapshotFailed),
                detail: "late explicit refusal".into(),
            }
        } else {
            StateControlAnswer::Snapshot {
                capture_id: 8.into(),
                room_name: "8.part".into(),
                size_bytes: 50.into(),
                position: None,
            }
        };
        peer.write_answer(&peer.answer(&snapshot, Some(result)));
        assert!(peer.release().discard_capture_result);
        assert_eq!(peer.channel.outstanding().len(), 1);
        assert_eq!(peer.channel.capture.unwrap().phase, CapturePhase::Aborting);
        peer.write_answer(&peer.answer(
            &abort,
            Some(StateControlAnswer::Aborted {
                capture_id: 8.into(),
            }),
        ));
        assert!(!peer.release().discard_capture_result);
        let next = peer.send_request(StateRequest::Control(StateControl::Held {}));
        assert_eq!(next.common().request_id.0, 3);
    }
}

#[test]
fn capture_cleanup_refusal_is_measured_then_terminal() {
    let mut peer = Peer::new();
    peer.open();
    let snapshot = peer.send_request(StateRequest::Control(StateControl::Snapshot {
        capture_id: 8.into(),
    }));
    peer.write_answer(&peer.answer(
        &snapshot,
        Some(StateControlAnswer::Snapshot {
            capture_id: 8.into(),
            room_name: "8.part".into(),
            size_bytes: 1.into(),
            position: None,
        }),
    ));
    peer.release();
    peer.channel.abort_capture(peer.now).unwrap();
    let abort = peer.finish_send();
    peer.write_answer(&peer.answer(
        &abort,
        Some(StateControlAnswer::ControlRefused {
            code: ControlRefusalCode::Capture(CaptureRefusalCode::UnknownCapture),
            detail: "synthetic cleanup refusal".into(),
        }),
    ));
    let proof = RecordedStateMeasurement::for_test(peer.channel.held_measurement().unwrap());
    let failure = peer.channel.release(proof).unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Protocol);
    assert_eq!(
        failure.outstanding.unwrap().operation,
        StateOperation::Abort
    );
}

#[test]
fn coalesced_capture_answers_signal_buffered_work_and_keep_arrival_time() {
    let mut peer = Peer::new();
    peer.open();
    let snapshot = peer.send_request(StateRequest::Control(StateControl::Snapshot {
        capture_id: 8.into(),
    }));
    peer.channel.abort_capture(peer.now).unwrap();
    let abort = peer.finish_send();
    let first = peer.answer(
        &snapshot,
        Some(StateControlAnswer::Snapshot {
            capture_id: 8.into(),
            room_name: "8.part".into(),
            size_bytes: 50.into(),
            position: None,
        }),
    );
    let mut second = peer.answer(
        &abort,
        Some(StateControlAnswer::Aborted {
            capture_id: 8.into(),
        }),
    );
    let StateFrame::ControlAnswer { observation, .. } = &mut second else {
        unreachable!()
    };
    observation.operation_id.0 += 1;
    let mut bytes = encode_state_line(&first, FrameDirection::Response).unwrap();
    bytes.extend(encode_state_line(&second, FrameDirection::Response).unwrap());
    peer.stream.write_all(&bytes).unwrap();
    peer.step().unwrap();
    assert_eq!(
        recv(
            peer.channel.fd().as_raw_fd(),
            &mut [0u8],
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_PEEK
        ),
        Err(nix::errno::Errno::EAGAIN),
        "both frames have left the kernel buffer",
    );
    let first_arrival = peer.channel.held_measurement().unwrap().harness_received_ns;
    assert!(
        !peer.channel.ready_buffered(),
        "full completion slot blocks reduction"
    );
    assert!(peer.release().discard_capture_result);
    assert!(
        peer.channel.ready_buffered(),
        "no fd edge remains for the buffered abort"
    );
    peer.step().unwrap();
    assert_eq!(
        peer.channel.held_measurement().unwrap().harness_received_ns,
        first_arrival
    );
    let duplicate = RecordedStateMeasurement::for_test(peer.channel.held_measurement().unwrap());
    assert!(matches!(
        peer.release().answer,
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Aborted { .. },
            ..
        }
    ));
    assert!(!peer.channel.ready_buffered());
    assert!(peer.channel.outstanding().is_empty());
    assert_eq!(
        peer.channel.release(duplicate).unwrap_err().kind,
        ChannelFailureKind::Protocol
    );
}

#[test]
fn counters_do_not_wrap_and_clock_origin_must_match_the_member() {
    let mut peer = Peer::new();
    peer.open();
    peer.channel.next_request = None;
    assert_eq!(
        peer.channel
            .begin(StateRequest::Control(StateControl::Held {}), peer.now)
            .unwrap_err()
            .point,
        ChannelPoint::Counter
    );

    let mut peer = Peer::new();
    peer.open();
    peer.channel.next_operation = None;
    assert_eq!(
        peer.channel
            .begin(StateRequest::Control(StateControl::Held {}), peer.now)
            .unwrap_err()
            .point,
        ChannelPoint::Counter
    );

    let mut peer = Peer::new();
    peer.channel.begin(open_request(), 100).unwrap();
    let failure = peer.channel.progress(&mut || Ok(99)).unwrap_err();
    assert_eq!(failure.kind, ChannelFailureKind::Protocol);
    assert_eq!(
        failure.outstanding.unwrap().commit,
        CommitEvidence::NotRequested
    );
}
