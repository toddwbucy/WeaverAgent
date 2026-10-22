//! Measurement exchange through the actual decode socket and recording context.
//! Scripted counts test custody and validation, not tokenizer or fit qualification.

use super::super::clock::{ReadyFd, WaitInterest, WaitOutcome, testing::FakePumpIo};
use super::super::pump::LifecyclePorts;
use super::super::pump_transport::{DecodePort, Stage};
use super::measure_driver::{MeasureCompletion, MeasurePosition, MeasureProgress, MeasureRequest};
use super::tests::{Fixture, config, event};
use super::*;
use crate::channel::DecodeChannel;
use nix::errno::Errno;
use nix::poll::PollFlags;
use nix::sys::socket::{MsgFlags, recv, send};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, BorrowedFd, RawFd};
use std::time::{Duration, Instant};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{TokenAnswer, TokenDirective, TokenRefusal, TurnKey};

const WAIT_NS: u64 = 1_000_000;

fn request() -> MeasureRequest {
    MeasureRequest {
        assembly_id: 9.into(),
        revision: 3.into(),
        keep: 45,
        delta: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Read the record.\nKeep the spelling: e\u{301}.".into(),
            }],
        }],
        tool_schemas: vec!["{\"name\":\"second\"}".into(), "first\nlast".into()],
        turn: None,
        // These are expected journal facts supplied by the caller. This
        // fixture creates neither a BootHead nor a placement grant.
        position: MeasurePosition {
            resident: 45,
            prefix_tokens: 12,
            capacity: 256,
        },
    }
}

fn answer(input_tokens: u64) -> TokenAnswer {
    TokenAnswer::DeltaMeasured {
        resident: 45,
        prefix_tokens: 12,
        keep: 45,
        input_tokens,
        capacity: 256,
    }
}

fn send_bytes(peer: BorrowedFd<'_>, bytes: &[u8]) {
    assert_eq!(
        send(
            peer.as_raw_fd(),
            bytes,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        ),
        Ok(bytes.len())
    );
}

fn receive_bytes(peer: BorrowedFd<'_>) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(
        peer.as_raw_fd(),
        &mut bytes,
        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC,
    ) {
        Ok(count) => {
            assert!(count > 0 && count <= bytes.len());
            bytes.truncate(count);
            Some(bytes)
        }
        Err(Errno::EAGAIN) => None,
        other => panic!("scripted decode receive failed: {other:?}"),
    }
}

/// Schedule readiness by the actual fd in the pump's current interest list.
/// Other ports may add descriptors without changing this fixture's meaning.
struct ReadyClock<'a> {
    clock: &'a mut FakePumpIo,
    target: RawFd,
    flags: PollFlags,
}

impl PumpIo for ReadyClock<'_> {
    fn now(&mut self) -> io::Result<u64> {
        self.clock.now()
    }

    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> io::Result<WaitOutcome> {
        let index = interests
            .iter()
            .position(|interest| interest.fd.as_raw_fd() == self.target)
            .expect("the active decode owner remains in the common pump");
        let at = self.clock.now()? + 1;
        self.clock.schedule(
            at,
            vec![ReadyFd {
                index,
                events: self.flags,
            }],
        )?;
        self.clock.wait(interests, deadline)
    }
}

fn ports<'a, I: PumpIo>(decode: &'a mut DecodePort, io: &'a mut I) -> PumpPorts<'a, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: Some(decode),
        label: None,
        io,
    }
}

struct Peer {
    run: Fixture,
    decode: DecodePort,
    peer: crate::channel::ChildEnd,
}

impl Peer {
    fn new() -> Self {
        let mut declaration = config();
        declaration.observation_interval.elapsed_ns = (WAIT_NS * 100).into();
        let mut run = Fixture::new(declaration);
        run.activate();
        run.context.complete_wait(PumpDeadline::Enter).unwrap();
        let (decode, peer) = DecodeChannel::pair().unwrap();
        Self {
            run,
            decode: DecodePort::from_channel(decode).unwrap(),
            peer,
        }
    }

    fn arm(&mut self, key: PumpDeadline) -> AbsoluteBound {
        let now = self.run.clock.now().unwrap();
        let bound = AbsoluteBound::after(now, WAIT_NS).unwrap();
        self.run
            .context
            .arm(key, bound, &mut self.run.clock)
            .unwrap();
        bound
    }

    fn begin(&mut self, request: MeasureRequest, key: PumpDeadline) -> AbsoluteBound {
        let bound = self.arm(key);
        self.run
            .context
            .begin_measurement(request, key, &mut self.decode, &mut self.run.clock)
            .unwrap();
        bound
    }

    fn advance(&mut self, flags: PollFlags) -> Result<MeasureProgress, RunFailure> {
        let target = self.decode.fd().as_raw_fd();
        let mut clock = ReadyClock {
            clock: &mut self.run.clock,
            target,
            flags,
        };
        self.run
            .context
            .advance_measurement(&mut ports(&mut self.decode, &mut clock))
    }

    fn sent(&mut self) -> TokenDirective {
        serde_json::from_slice(&self.sent_bytes(Vec::new())).unwrap()
    }

    fn sent_bytes(&mut self, mut packets: Vec<Vec<u8>>) -> Vec<u8> {
        for _ in 0..128 {
            while let Some(bytes) = receive_bytes(self.peer.as_fd()) {
                packets.push(bytes);
            }
            if !packets.is_empty() {
                if let Ok(preamble) =
                    serde_json::from_slice::<weaver_types::SegmentPreamble>(&packets[0])
                {
                    if packets.len() == preamble.segments as usize + 1 {
                        let bytes = packets[1..].concat();
                        assert_eq!(bytes.len() as u64, preamble.bytes);
                        return bytes;
                    }
                } else {
                    assert_eq!(packets.len(), 1);
                    return packets.pop().unwrap();
                }
            }
            self.advance(PollFlags::POLLOUT).unwrap();
            while let Some(bytes) = receive_bytes(self.peer.as_fd()) {
                packets.push(bytes);
            }
        }
        panic!("the measure request did not reach its actual peer");
    }

    fn send_answer(&self, answer: &TokenAnswer) {
        send_bytes(self.peer.as_fd(), &serde_json::to_vec(answer).unwrap());
    }

    fn send_refusal(&self, refusal: &TokenRefusal) {
        send_bytes(self.peer.as_fd(), &serde_json::to_vec(refusal).unwrap());
    }

    fn finish(&mut self) -> MeasureCompletion {
        for _ in 0..128 {
            self.advance(PollFlags::POLLIN).unwrap();
            if let Some(completion) = self
                .run
                .context
                .take_measurement(3.into(), &mut self.decode, &mut self.run.clock)
                .unwrap()
            {
                return completion;
            }
        }
        panic!("the whole answer did not close the measure exchange");
    }

    fn records(&self, kind: Kind) -> Vec<serde_json::Value> {
        self.run
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(kind)
            .map(|record| serde_json::from_str(&record.line).unwrap())
            .collect()
    }

    fn fill_trace(&mut self) {
        let payload = serde_json::value::to_raw_value(&serde_json::json!({
            "fixture": "x".repeat(32_768)
        }))
        .unwrap();
        for _ in 0..weaver_trace::serving::ORDINARY_CAPACITY + 64 {
            if self.run.context.barrier.recorder().boundary().queued
                == weaver_trace::serving::ORDINARY_CAPACITY
            {
                return;
            }
            let now = self.run.clock.now().unwrap();
            assert!(matches!(
                self.run.context.barrier.submit_authored_with_clock(
                    AuthoredEvent {
                        kind: Kind::ModelMeasurement,
                        subsystem: weaver_trace::Subsystem::Harness,
                        turn: Some(weaver_trace::TurnRef("pressure".into())),
                        source: super::super::author::AuthoredSource::Runtime,
                        causal_parent: None,
                        payload: Some(Payload::ModelMeasurement(payload.clone())),
                    },
                    RecordPurpose::Observation,
                    &self.run.context.author,
                    1,
                    &mut self.run.context.inbox,
                    &mut self.run.context.budget,
                    &mut || Ok(now),
                ),
                RecordProgress::Recorded { .. }
            ));
        }
        panic!("actual writer did not fill its ordinary queue");
    }

    fn drain_trace(&mut self) {
        self.run._reader.set_nonblocking(true).unwrap();
        let watchdog = Instant::now() + Duration::from_secs(3);
        let mut bytes = [0; 65_536];
        while self.run.context.barrier.recorder().boundary().queued != 0 {
            assert!(Instant::now() < watchdog, "trace writer did not drain");
            match self.run._reader.read(&mut bytes) {
                Ok(0) => panic!("trace closed during pressure fixture"),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(error) => panic!("trace pressure drain failed: {error}"),
            }
        }
    }
}

#[test]
fn exact_measure_proposal_and_oversized_count_cross_without_placement_authority() {
    let mut peer = Peer::new();
    let original = request();
    let expected = TokenDirective::MeasureDelta {
        keep: original.keep,
        delta: original.delta.clone(),
        tool_schemas: original.tool_schemas.clone(),
    };
    let bound = peer.begin(original, PumpDeadline::Decision);
    assert_eq!(peer.sent(), expected);
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        peer.run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    peer.send_answer(&answer(u64::MAX));
    let completed = peer.finish();
    assert_eq!(completed.request().assembly_id.0, 9);
    assert_eq!(completed.request().revision.0, 3);
    assert_eq!(completed.request().delta, request().delta);
    assert_eq!(completed.request().tool_schemas, request().tool_schemas);
    let MeasureCompletion::Measured(measured) = completed else {
        panic!("an oversized count remains a count, not a placement refusal");
    };
    assert_eq!(measured.facts().input_tokens, u64::MAX);
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(peer.records(Kind::StatePlacement).is_empty());
    assert_eq!(
        peer.run.context.complete_wait(PumpDeadline::Decision),
        Some(bound)
    );
}

#[test]
fn measure_keeps_one_absolute_wait_through_partial_reply_and_exact_expiry() {
    for key in [PumpDeadline::Decision, PumpDeadline::Cut] {
        let mut peer = Peer::new();
        let bound = peer.begin(request(), key);
        peer.sent();
        let mut bytes = serde_json::to_vec(&answer(17)).unwrap();
        bytes.resize(weaver_types::MAX_ENVELOPE_BYTES + 100, b' ');
        let preamble = weaver_types::SegmentPreamble {
            segments: 2,
            bytes: bytes.len() as u64,
        };
        send_bytes(peer.peer.as_fd(), &serde_json::to_vec(&preamble).unwrap());
        peer.advance(PollFlags::POLLIN).unwrap();
        send_bytes(
            peer.peer.as_fd(),
            &bytes[..weaver_types::MAX_ENVELOPE_BYTES],
        );
        peer.advance(PollFlags::POLLIN).unwrap();
        assert_eq!(peer.run.context.deadlines.get(key), Some(bound));
        assert!(peer.run.context.complete_wait(key).is_none());
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .unwrap()
                .is_none()
        );
        peer.run.clock.advance_to(bound.deadline_ns()).unwrap();
        send_bytes(
            peer.peer.as_fd(),
            &bytes[weaver_types::MAX_ENVELOPE_BYTES..],
        );
        assert!(
            matches!(peer.advance(PollFlags::POLLIN), Err(RunFailure::Flow { reason, .. })
            if reason == if key == PumpDeadline::Decision { FlowHaltReason::DecisionBound } else { FlowHaltReason::CutBound })
        );
        assert_eq!(
            peer.run.context.halt_wait().unwrap().deadline_ns.0,
            bound.deadline_ns()
        );
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .is_err()
        );
    }
}

#[test]
fn each_inconsistent_serial_count_is_a_protocol_failure_not_zero_or_empty() {
    for bad in [
        TokenAnswer::DeltaMeasured {
            resident: 46,
            prefix_tokens: 12,
            keep: 45,
            input_tokens: 17,
            capacity: 256,
        },
        TokenAnswer::DeltaMeasured {
            resident: 45,
            prefix_tokens: 11,
            keep: 45,
            input_tokens: 17,
            capacity: 256,
        },
        TokenAnswer::DeltaMeasured {
            resident: 45,
            prefix_tokens: 12,
            keep: 44,
            input_tokens: 17,
            capacity: 256,
        },
        TokenAnswer::DeltaMeasured {
            resident: 45,
            prefix_tokens: 12,
            keep: 45,
            input_tokens: 17,
            capacity: 257,
        },
        TokenAnswer::AtRest,
        TokenAnswer::Token {
            token: 7,
            piece: "unasked".into(),
        },
    ] {
        let mut peer = Peer::new();
        peer.begin(request(), PumpDeadline::Decision);
        peer.sent();
        peer.send_answer(&bad);
        let mut failed = false;
        for _ in 0..128 {
            if let Err(failure) = peer.advance(PollFlags::POLLIN) {
                assert!(
                    matches!(
                        failure,
                        RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            ..
                        }
                    ),
                    "{failure:?}"
                );
                failed = true;
                break;
            }
        }
        assert!(failed, "an inconsistent answer remained live: {bad:?}");
        assert!(peer.run.context.halt().is_some());
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .is_err()
        );
    }
}

#[test]
fn out_of_order_and_not_open_refuse_the_measure_protocol() {
    for refusal in [TokenRefusal::NotOpen, TokenRefusal::OutOfOrder] {
        let mut peer = Peer::new();
        peer.begin(request(), PumpDeadline::Decision);
        peer.sent();
        peer.send_refusal(&refusal);
        let mut failed = false;
        for _ in 0..128 {
            if let Err(failure) = peer.advance(PollFlags::POLLIN) {
                assert!(
                    matches!(
                        failure,
                        RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            ..
                        }
                    ),
                    "{failure:?}"
                );
                failed = true;
                break;
            }
        }
        assert!(failed);
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .is_err()
        );
    }
}

#[test]
fn cancellation_drains_the_same_answer_without_a_measurement_grant() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    peer.run
        .context
        .cancel_measurement(&mut peer.run.clock)
        .unwrap();
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        peer.run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    let next = TokenDirective::Cancel {
        turn: TurnKey("not-a-second-exchange".into()),
    };
    assert!(
        matches!(peer.decode.stage(next.clone()).unwrap(), Stage::Busy(returned) if returned == next)
    );
    peer.send_answer(&answer(17));
    assert!(matches!(peer.finish(), MeasureCompletion::Cancelled(_)));
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(peer.run.context.halt().is_none());
    assert!(receive_bytes(peer.peer.as_fd()).is_none());
    assert!(peer.records(Kind::StatePlacement).is_empty());
}

#[test]
fn measurement_owner_rejects_generic_authors_and_preserves_its_wait() {
    let mut peer = Peer::new();
    peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    assert!(matches!(
        peer.run.context.author_and_wait(
            &mut ports(&mut peer.decode, &mut peer.run.clock),
            event(Kind::SessionClosed, None)
        ),
        Err(RunFailure::MeasureOwned)
    ));
    assert!(matches!(
        peer.run
            .context
            .resume_author_and_wait(&mut ports(&mut peer.decode, &mut peer.run.clock)),
        Err(RunFailure::MeasureOwned)
    ));
    assert!(
        peer.run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert!(peer.run.context.halt().is_none());
    peer.send_answer(&answer(0));
    assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
}

#[test]
fn malformed_delta_is_recorded_once_before_the_typed_refusal_is_released() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    peer.send_refusal(&TokenRefusal::MalformedDelta);
    let MeasureCompletion::Refused(refused) = peer.finish() else {
        panic!("malformed family rendering is a typed refusal");
    };
    assert_eq!(
        refused.code(),
        weaver_types::state_flow::PlacementCode::InvalidSpan
    );
    let (decode, placement) = refused.sources();
    assert!(decode.is_some_and(|source| source.0 < placement.0));
    let records = peer.records(Kind::Refusal);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["subsystem"], "spu_decoder");
    assert_eq!(records[1]["subsystem"], "harness");
    assert!(
        records
            .iter()
            .any(|record| record.to_string().contains("malformed_delta"))
    );
    assert!(
        records
            .iter()
            .any(|record| record.to_string().contains("invalid_span"))
    );
    assert!(peer.run.context.halt().is_none());
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn decoder_fault_is_recorded_without_closing_the_measurement_exchange() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    let report = weaver_types::FaultReport {
        case: weaver_types::FaultCase::DeviceFaultDuringGeneration,
        account: serde_json::value::RawValue::from_string(
            "{\"device\":\"fixture\",\"detail\":\"exact bytes\"}".into(),
        )
        .unwrap(),
    };
    peer.send_answer(&TokenAnswer::Fault(report));
    for _ in 0..64 {
        peer.advance(PollFlags::POLLIN).unwrap();
        if !peer.records(Kind::Fault).is_empty() {
            break;
        }
    }
    let records = peer.records(Kind::Fault);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["subsystem"], "spu_decoder");
    let line = records[0].to_string();
    assert!(line.contains("spu_decoder") && line.contains("device_fault_during_generation"));
    assert!(line.contains("exact bytes"));
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        peer.run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    peer.send_answer(&answer(17));
    assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
    assert_eq!(peer.records(Kind::Fault).len(), 1);
}

#[test]
fn a_foreign_port_and_second_begin_cannot_steal_the_owned_decode_reply() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    let mut second = request();
    second.assembly_id = 10.into();
    let error = peer
        .run
        .context
        .begin_measurement(
            second,
            PumpDeadline::Decision,
            &mut peer.decode,
            &mut peer.run.clock,
        )
        .unwrap_err();
    assert_eq!(error.request.assembly_id.0, 10);
    assert_eq!(
        error.refusal,
        super::measure_driver::MeasureBeginRefusal::Busy
    );
    assert!(error.failure.is_none());
    let (other, _other_peer) = DecodeChannel::pair().unwrap();
    let mut other = DecodePort::from_channel(other).unwrap();
    assert!(matches!(
        peer.run
            .context
            .take_measurement(3.into(), &mut other, &mut peer.run.clock),
        Err(RunFailure::MeasureOwned)
    ));
    assert!(peer.run.context.halt().is_none());
    peer.send_answer(&answer(17));
    assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
}

#[test]
fn a_stale_revision_is_recorded_as_refusal_without_releasing_the_old_measure() {
    let mut peer = Peer::new();
    peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    peer.send_answer(&answer(17));
    for _ in 0..64 {
        if peer.advance(PollFlags::POLLIN).unwrap() == MeasureProgress::Complete {
            break;
        }
    }
    assert!(
        peer.run
            .context
            .take_measurement(4.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    let mut completed = None;
    for _ in 0..64 {
        peer.advance(PollFlags::POLLIN).unwrap();
        completed = peer
            .run
            .context
            .take_measurement(4.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap();
        if completed.is_some() {
            break;
        }
    }
    let Some(MeasureCompletion::Refused(refused)) = completed else {
        panic!("revision change must close as a recorded refusal");
    };
    assert_eq!(
        refused.code(),
        weaver_types::state_flow::PlacementCode::StalePosition
    );
    assert!(refused.sources().0.is_none());
    assert_eq!(peer.records(Kind::Refusal).len(), 1);
    assert!(peer.run.context.halt().is_none());
}

#[test]
fn cut_clamps_keep_but_ordinary_measure_cannot_request_a_smaller_prefix() {
    let mut peer = Peer::new();
    peer.arm(PumpDeadline::Decision);
    let mut smaller = request();
    smaller.keep = 0;
    let error = peer
        .run
        .context
        .begin_measurement(
            smaller,
            PumpDeadline::Decision,
            &mut peer.decode,
            &mut peer.run.clock,
        )
        .unwrap_err();
    assert_eq!(error.request.keep, 0);
    assert!(error.failure.is_none());
    assert!(receive_bytes(peer.peer.as_fd()).is_none());
    peer.run
        .context
        .complete_wait(PumpDeadline::Decision)
        .unwrap();
    let mut smaller = request();
    smaller.keep = 0;
    peer.begin(smaller, PumpDeadline::Cut);
    assert!(matches!(
        peer.sent(),
        TokenDirective::MeasureDelta { keep: 0, .. }
    ));
    peer.send_answer(&TokenAnswer::DeltaMeasured {
        resident: 45,
        prefix_tokens: 12,
        keep: 12,
        input_tokens: 17,
        capacity: 256,
    });
    let MeasureCompletion::Measured(measured) = peer.finish() else {
        panic!("the exact clamped cut proposal must measure");
    };
    assert_eq!(measured.facts().keep, 12);
}

#[test]
fn a_queued_measure_spends_the_original_wait_before_its_first_socket_write() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    assert!(receive_bytes(peer.peer.as_fd()).is_none());
    peer.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        peer.advance(PollFlags::POLLOUT),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert!(receive_bytes(peer.peer.as_fd()).is_none());
    assert_eq!(
        peer.run.context.halt_wait().unwrap().began_ns.0,
        bound.began_ns()
    );
    assert_eq!(
        peer.run.context.halt_wait().unwrap().deadline_ns.0,
        bound.deadline_ns()
    );
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .is_err()
    );
}

#[test]
fn a_measure_requires_an_existing_applicable_wait_and_returns_unstarted_input() {
    let mut peer = Peer::new();
    let error = peer
        .run
        .context
        .begin_measurement(
            request(),
            PumpDeadline::Decision,
            &mut peer.decode,
            &mut peer.run.clock,
        )
        .unwrap_err();
    assert_eq!(
        error.refusal,
        super::measure_driver::MeasureBeginRefusal::MissingWait
    );
    assert_eq!(error.request.delta, request().delta);
    assert!(error.failure.is_none());
    assert!(receive_bytes(peer.peer.as_fd()).is_none());
    assert!(
        peer.run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    peer.arm(PumpDeadline::Label);
    let error = peer
        .run
        .context
        .begin_measurement(
            request(),
            PumpDeadline::Label,
            &mut peer.decode,
            &mut peer.run.clock,
        )
        .unwrap_err();
    assert_eq!(
        error.refusal,
        super::measure_driver::MeasureBeginRefusal::WrongWait
    );
    assert!(error.failure.is_none());
    assert!(peer.run.context.halt().is_none());
}

#[test]
fn cancellation_before_sending_keeps_the_original_request_owned_until_drained() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.run
        .context
        .cancel_measurement(&mut peer.run.clock)
        .unwrap();
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        peer.sent(),
        TokenDirective::MeasureDelta { keep: 45, .. }
    ));
    peer.send_answer(&answer(17));
    let completed = peer.finish();
    assert!(matches!(completed, MeasureCompletion::Cancelled(_)));
    assert_eq!(completed.request().assembly_id.0, 9);
    assert_eq!(completed.bound(), bound);
    assert_eq!(completed.wait(), PumpDeadline::Decision);
    assert_eq!(
        completed.raw_bytes(),
        serde_json::to_vec(&answer(17)).unwrap()
    );
    assert!(peer.run.context.halt().is_none());
}

#[test]
fn a_generic_decode_consumer_cannot_take_a_held_measurement_or_stage_after_send() {
    let mut peer = Peer::new();
    peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    let second = TokenDirective::AppendAndGenerate {
        turn: TurnKey("bypass".into()),
        delta: vec![],
        tool_schemas: vec![],
    };
    assert!(
        matches!(peer.decode.stage(second.clone()).unwrap(), Stage::Busy(returned) if returned == second)
    );
    peer.send_answer(&answer(17));
    assert_eq!(
        peer.decode.try_receive().unwrap(),
        super::super::pump_transport::ReceiveProgress::Held
    );
    assert!(peer.decode.take_completion().is_err());
    assert!(peer.decode.failure().is_none());
    assert!(peer.decode.held().is_some());
    assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
    assert!(receive_bytes(peer.peer.as_fd()).is_none());
    assert!(matches!(peer.decode.stage(second).unwrap(), Stage::Queued));
}

#[test]
fn a_measure_owner_refuses_a_read_without_starting_another_control_wait() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    let selector = weaver_types::state_flow::ReadSelector::History {
        noun: "record".into(),
        verb: None,
        condition: None,
    };
    let error = peer
        .run
        .context
        .begin_read(selector.clone(), None, &mut peer.run.clock)
        .unwrap_err();
    assert_eq!(error.selector, selector);
    assert_eq!(error.refusal, super::read_driver::ReadBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert!(
        peer.run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(peer.run.context.halt().is_none());
    peer.sent();
    peer.send_answer(&answer(17));
    assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
}

#[test]
fn a_fault_before_the_whole_request_is_a_recorded_emission_not_a_reply() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    assert!(receive_bytes(peer.peer.as_fd()).is_none());
    peer.send_answer(&TokenAnswer::Fault(weaver_types::FaultReport {
        case: weaver_types::FaultCase::ReadoutFaultWhileElected,
        account: serde_json::value::RawValue::from_string("{\"at\":\"before request\"}".into())
            .unwrap(),
    }));
    // The real transport observes the unsolicited emission before sending the
    // queued request, so its reservation receipt cannot claim after_send.
    assert_eq!(
        peer.decode.try_receive().unwrap(),
        super::super::pump_transport::ReceiveProgress::Held
    );
    for _ in 0..64 {
        peer.advance(PollFlags::POLLOUT).unwrap();
        if !peer.records(Kind::Fault).is_empty() {
            break;
        }
    }
    assert_eq!(peer.records(Kind::Fault).len(), 1);
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    peer.sent();
    peer.send_answer(&answer(17));
    assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
}

#[test]
fn cancellation_in_a_segmented_send_drains_exact_bytes_before_a_later_proposal() {
    let mut peer = Peer::new();
    let mut large = request();
    large.delta[0].content = vec![ContentBlock::Text {
        text: "retain exact text\n".repeat(weaver_types::MAX_ENVELOPE_BYTES / 8),
    }];
    let expected = serde_json::to_vec(&TokenDirective::MeasureDelta {
        keep: large.keep,
        delta: large.delta.clone(),
        tool_schemas: large.tool_schemas.clone(),
    })
    .unwrap();
    let bound = peer.begin(large, PumpDeadline::Decision);
    let first = loop {
        peer.advance(PollFlags::POLLOUT).unwrap();
        if let Some(bytes) = receive_bytes(peer.peer.as_fd()) {
            break bytes;
        }
    };
    let preamble: weaver_types::SegmentPreamble = serde_json::from_slice(&first).unwrap();
    assert_eq!(preamble.bytes, expected.len() as u64);
    assert!(preamble.segments > 1);
    peer.run
        .context
        .cancel_measurement(&mut peer.run.clock)
        .unwrap();
    assert!(
        peer.run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert_eq!(peer.sent_bytes(vec![first]), expected);
    peer.send_answer(&answer(99));
    assert!(matches!(peer.finish(), MeasureCompletion::Cancelled(_)));
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    peer.run
        .context
        .complete_wait(PumpDeadline::Decision)
        .unwrap();
    let mut next = request();
    next.assembly_id = 10.into();
    next.delta[0].content = vec![ContentBlock::Text {
        text: "a different large proposal\n".repeat(weaver_types::MAX_ENVELOPE_BYTES / 8),
    }];
    let expected = serde_json::to_vec(&TokenDirective::MeasureDelta {
        keep: next.keep,
        delta: next.delta.clone(),
        tool_schemas: next.tool_schemas.clone(),
    })
    .unwrap();
    peer.begin(next, PumpDeadline::Decision);
    assert_eq!(peer.sent_bytes(Vec::new()), expected);
    peer.send_answer(&answer(123));
    let completed = peer.finish();
    assert_eq!(completed.request().assembly_id.0, 10);
    let MeasureCompletion::Measured(measured) = completed else {
        panic!("the second proposal must receive only its own count");
    };
    assert_eq!(measured.facts().input_tokens, 123);
}

/// The held completion needs no I/O. The first four clock reads cover the
/// entry guard and ordinary flow reduction. The fifth is after count validation.
struct ValidationClock {
    before: u64,
    deadline: u64,
    reads: usize,
}

/// Expire a lifecycle bound at a specific owner handoff without advancing a
/// second clock. These checkpoints have complete wire/trace evidence and wait
/// for no descriptor.
struct HandoffClock<'a> {
    clock: &'a mut FakePumpIo,
    before_reads: usize,
    deadline: u64,
}

impl PumpIo for HandoffClock<'_> {
    fn now(&mut self) -> io::Result<u64> {
        if self.before_reads == 0 {
            self.clock.advance_to(self.deadline)?;
        } else {
            self.before_reads -= 1;
        }
        self.clock.now()
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("the measurement handoff already owns its complete evidence");
    }
}

impl PumpIo for ValidationClock {
    fn now(&mut self) -> io::Result<u64> {
        self.reads += 1;
        Ok(if self.reads < 5 {
            self.before
        } else {
            self.deadline
        })
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("the actual whole answer is already held by the decode owner");
    }
}

#[test]
fn validation_crossing_the_original_bound_cannot_publish_measurement_facts() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    peer.send_answer(&answer(17));
    assert_eq!(
        peer.decode.try_receive().unwrap(),
        super::super::pump_transport::ReceiveProgress::Held
    );
    let mut io = ValidationClock {
        before: bound.deadline_ns() - 1,
        deadline: bound.deadline_ns(),
        reads: 0,
    };
    assert!(matches!(
        peer.run
            .context
            .advance_measurement(&mut ports(&mut peer.decode, &mut io)),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert!(
        io.reads >= 5,
        "the validation boundary did not read the clock"
    );
    assert_eq!(
        peer.run.context.halt_wait().unwrap().deadline_ns.0,
        bound.deadline_ns()
    );
    assert!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut io)
            .is_err()
    );
}

#[test]
fn recorded_measure_authors_resume_after_leave_consumes_the_shared_sequence() {
    // Fault, decode refusal and placement refusal each have a distinct
    // continuation after their original author is Recorded.
    for (malformed, placement) in [(false, false), (true, false), (true, true)] {
        let mut peer = Peer::new();
        let bound = peer.begin(request(), PumpDeadline::Decision);
        peer.sent();
        let raw = if malformed {
            serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap()
        } else {
            serde_json::to_vec(&TokenAnswer::Fault(weaver_types::FaultReport {
                case: weaver_types::FaultCase::ReadoutFaultWhileElected,
                account: serde_json::value::RawValue::from_string(
                    "{ \"retained\" : \"original fault account\" }".into(),
                )
                .unwrap(),
            }))
            .unwrap()
        };
        send_bytes(peer.peer.as_fd(), &raw);
        for _ in 0..64 {
            peer.advance(PollFlags::POLLIN).unwrap();
            if peer.run.context.waiting_content.is_some() {
                break;
            }
        }
        assert!(peer.run.context.waiting_content.is_some());
        if placement {
            peer.advance(PollFlags::POLLIN).unwrap();
            assert_eq!(peer.records(Kind::Refusal).len(), 1);
            assert!(peer.run.context.waiting_content.is_some());
        }
        let now = peer.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        peer.run
            .context
            .arm(PumpDeadline::Leave, leave, &mut peer.run.clock)
            .unwrap();
        // Entry and the original envelope sample precede the shared author's
        // Recorded check, which retains completed_author when Leave expires.
        let mut clock = HandoffClock {
            clock: &mut peer.run.clock,
            before_reads: 2,
            deadline: leave.deadline_ns(),
        };
        assert_eq!(
            peer.run
                .context
                .advance_measurement(&mut ports(&mut peer.decode, &mut clock))
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave,
            }
        );
        let recorded = peer.run.context.completed_author.unwrap();
        assert!(peer.run.context.waiting_content.is_none());
        assert!(peer.run.context.pending_author().is_none());
        assert_eq!(peer.run.context.pending_measure_raw(), Some(raw.as_slice()));
        assert_eq!(
            peer.run.context.complete_wait(PumpDeadline::Leave),
            Some(leave)
        );

        let now = peer.run.clock.now().unwrap();
        let later_leave = AbsoluteBound::after(now, 10).unwrap();
        peer.run
            .context
            .arm(PumpDeadline::Leave, later_leave, &mut peer.run.clock)
            .unwrap();
        // The entry check passes; the owner's post-work check expires after
        // resume_owned_author_and_wait transfers that same recorded sequence.
        let mut clock = HandoffClock {
            clock: &mut peer.run.clock,
            before_reads: 1,
            deadline: later_leave.deadline_ns(),
        };
        assert_eq!(
            peer.run
                .context
                .advance_measurement(&mut ports(&mut peer.decode, &mut clock))
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: later_leave,
            }
        );
        assert!(peer.run.context.completed_author.is_none());
        assert!(peer.run.context.halt().is_none());
        assert_eq!(
            peer.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert_eq!(
            peer.run.context.complete_wait(PumpDeadline::Leave),
            Some(later_leave)
        );
        let completed = if malformed {
            peer.finish()
        } else {
            peer.send_answer(&answer(17));
            peer.finish()
        };
        if let MeasureCompletion::Refused(refused) = &completed {
            assert!(malformed);
            let (decode, placement_source) = refused.sources();
            assert_eq!(
                if placement {
                    Some(placement_source)
                } else {
                    decode
                },
                Some(recorded)
            );
            assert_eq!(completed.raw_bytes(), raw);
            assert_eq!(peer.records(Kind::Refusal).len(), 2);
        } else {
            assert!(!malformed);
            assert!(matches!(completed, MeasureCompletion::Measured(_)));
            assert_eq!(peer.records(Kind::Fault).len(), 1);
            assert!(
                peer.run
                    .context
                    .barrier
                    .recorder()
                    .structure()
                    .by_kind(Kind::Fault)
                    .next()
                    .unwrap()
                    .line
                    .contains("{ \"retained\" : \"original fault account\" }")
            );
        }
        assert!(peer.run.context.halt().is_none());
        assert_eq!(completed.request().assembly_id, request().assembly_id);
        assert_eq!(completed.bound(), bound);
        assert_eq!(
            peer.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
}

#[test]
fn measurement_take_keeps_its_reservation_when_the_final_check_yields_leave() {
    for disposition in 0..3 {
        let mut peer = Peer::new();
        let bound = peer.begin(request(), PumpDeadline::Decision);
        peer.sent();
        if disposition == 2 {
            peer.run
                .context
                .cancel_measurement(&mut peer.run.clock)
                .unwrap();
        }
        let raw = if disposition == 1 {
            serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap()
        } else {
            serde_json::to_vec(&answer(17)).unwrap()
        };
        send_bytes(peer.peer.as_fd(), &raw);
        let mut complete = false;
        for _ in 0..64 {
            if matches!(
                peer.advance(PollFlags::POLLIN).unwrap(),
                MeasureProgress::Complete | MeasureProgress::Cancelled
            ) {
                complete = true;
                break;
            }
        }
        assert!(complete);
        let now = peer.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        peer.run
            .context
            .arm(PumpDeadline::Leave, leave, &mut peer.run.clock)
            .unwrap();
        let mut clock = HandoffClock {
            clock: &mut peer.run.clock,
            before_reads: 1,
            deadline: leave.deadline_ns(),
        };
        assert_eq!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut clock)
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave,
            }
        );
        assert_eq!(peer.run.context.pending_measure_raw(), Some(raw.as_slice()));
        let next = TokenDirective::Cancel {
            turn: TurnKey("still-owned".into()),
        };
        assert!(matches!(
            peer.decode.stage(next.clone()).unwrap(),
            Stage::Busy(returned) if returned == next
        ));
        assert_eq!(
            peer.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert_eq!(
            peer.run.context.complete_wait(PumpDeadline::Leave),
            Some(leave)
        );
        let completed = peer
            .run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap()
            .unwrap();
        assert!(matches!(
            (&completed, disposition),
            (MeasureCompletion::Measured(_), 0)
                | (MeasureCompletion::Refused(_), 1)
                | (MeasureCompletion::Cancelled(_), 2)
        ));
        assert_eq!(completed.raw_bytes(), raw);
        assert_eq!(completed.request().delta, request().delta);
        assert_eq!(completed.request().tool_schemas, request().tool_schemas);
        assert_eq!(completed.bound(), bound);
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(matches!(peer.decode.stage(next).unwrap(), Stage::Queued));
        assert!(peer.run.context.halt().is_none());
        assert_eq!(
            peer.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
}

#[test]
fn measurement_take_transfers_its_owner_without_yield_after_reservation_settlement() {
    for disposition in 0..3 {
        let mut peer = Peer::new();
        let bound = peer.begin(request(), PumpDeadline::Decision);
        peer.sent();
        if disposition == 2 {
            peer.run
                .context
                .cancel_measurement(&mut peer.run.clock)
                .unwrap();
        }
        let raw = if disposition == 1 {
            serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap()
        } else {
            serde_json::to_vec(&answer(17)).unwrap()
        };
        send_bytes(peer.peer.as_fd(), &raw);
        let mut complete = false;
        for _ in 0..64 {
            if matches!(
                peer.advance(PollFlags::POLLIN).unwrap(),
                MeasureProgress::Complete | MeasureProgress::Cancelled
            ) {
                complete = true;
                break;
            }
        }
        assert!(complete);
        let now = peer.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        peer.run
            .context
            .arm(PumpDeadline::Leave, leave, &mut peer.run.clock)
            .unwrap();
        let mut clock = HandoffClock {
            clock: &mut peer.run.clock,
            // Permit both pre-settlement guards. A third guard would yield
            // Leave after consuming the reservation but before its owner moves.
            before_reads: 2,
            deadline: leave.deadline_ns(),
        };
        let completed = peer
            .run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut clock)
            .unwrap()
            .unwrap();
        assert!(matches!(
            (&completed, disposition),
            (MeasureCompletion::Measured(_), 0)
                | (MeasureCompletion::Refused(_), 1)
                | (MeasureCompletion::Cancelled(_), 2)
        ));
        assert_eq!(completed.raw_bytes(), raw);
        assert_eq!(completed.request().delta, request().delta);
        assert_eq!(completed.request().tool_schemas, request().tool_schemas);
        assert_eq!(completed.bound(), bound);
        assert!(peer.run.context.pending_measure_raw().is_none());
        // The pending Leave remains effective at the next recoverable boundary.
        assert_eq!(
            peer.run.context.live(&mut clock).unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave,
            }
        );
        assert_eq!(
            peer.run.context.complete_wait(PumpDeadline::Leave),
            Some(leave)
        );
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            peer.decode
                .stage(TokenDirective::Cancel {
                    turn: TurnKey("after-transfer".into()),
                })
                .unwrap(),
            Stage::Queued
        ));
        assert!(peer.run.context.halt().is_none());
        assert_eq!(
            peer.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
}

#[test]
fn trace_pressure_keeps_fault_and_malformed_authors_until_recorded_once() {
    for malformed in [false, true] {
        let mut peer = Peer::new();
        let bound = peer.begin(request(), PumpDeadline::Decision);
        peer.sent();
        peer.fill_trace();
        let raw = if malformed {
            serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap()
        } else {
            serde_json::to_vec(&TokenAnswer::Fault(weaver_types::FaultReport {
                case: weaver_types::FaultCase::ReadoutFaultWhileElected,
                account: serde_json::value::RawValue::from_string(
                    "{ \"preserve\" : \"the exact account\" }".into(),
                )
                .unwrap(),
            }))
            .unwrap()
        };
        send_bytes(peer.peer.as_fd(), &raw);
        peer.advance(PollFlags::POLLIN).unwrap();
        let now = peer.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        peer.run
            .context
            .arm(PumpDeadline::Leave, leave, &mut peer.run.clock)
            .unwrap();
        assert_eq!(
            peer.advance(PollFlags::POLLIN).unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave,
            }
        );
        let kind = if malformed {
            Kind::Refusal
        } else {
            Kind::Fault
        };
        assert_eq!(peer.run.context.pending_author().unwrap().event.kind, kind);
        assert!(peer.records(kind).is_empty());
        assert!(peer.run.context.halt().is_none());
        assert_eq!(
            peer.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert!(
            peer.run
                .context
                .complete_wait(PumpDeadline::Decision)
                .is_none()
        );
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .is_err()
        );
        peer.run.context.complete_wait(PumpDeadline::Leave).unwrap();
        assert!(
            peer.run
                .context
                .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                .unwrap()
                .is_none()
        );
        peer.drain_trace();
        if malformed {
            let completed = peer.finish();
            assert!(matches!(completed, MeasureCompletion::Refused(_)));
            assert_eq!(completed.raw_bytes(), raw);
            assert_eq!(peer.records(Kind::Refusal).len(), 2);
        } else {
            for _ in 0..64 {
                peer.advance(PollFlags::POLLIN).unwrap();
                if !peer.records(Kind::Fault).is_empty() {
                    break;
                }
            }
            assert_eq!(peer.records(Kind::Fault).len(), 1);
            assert!(
                peer.run
                    .context
                    .barrier
                    .recorder()
                    .structure()
                    .by_kind(Kind::Fault)
                    .next()
                    .unwrap()
                    .line
                    .contains("{ \"preserve\" : \"the exact account\" }")
            );
            assert!(
                peer.run
                    .context
                    .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
                    .unwrap()
                    .is_none()
            );
            peer.send_answer(&answer(17));
            assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
            assert_eq!(peer.records(Kind::Fault).len(), 1);
        }
        assert_eq!(
            peer.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert!(peer.run.context.halt().is_none());
    }
}

#[test]
fn a_protocol_refusal_latches_before_pressure_clears_then_records_its_exact_ask() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    peer.fill_trace();
    let raw = serde_json::to_vec(&TokenRefusal::NotOpen).unwrap();
    send_bytes(peer.peer.as_fd(), &raw);
    let first_error = peer.advance(PollFlags::POLLIN).unwrap_err();
    assert!(matches!(
        first_error,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        }
    ));
    let first_cause = peer.run.context.halt().unwrap().first().clone();
    let teardown = peer.run.context.halt().unwrap().deadline().unwrap();
    assert!(peer.run.clock.now().unwrap() < bound.deadline_ns());
    assert!(peer.records(Kind::Refusal).is_empty());
    assert_eq!(
        peer.run.context.pending_author().unwrap().event.kind,
        Kind::Refusal
    );
    assert_eq!(peer.run.context.pending_measure_raw(), Some(raw.as_slice()));
    assert_eq!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap_err(),
        first_error
    );
    assert!(
        peer.run
            .context
            .failed_measure_refusal_progress(&mut peer.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(peer.run.context.halt().unwrap().first(), &first_cause);
    assert_eq!(
        peer.run.context.halt().unwrap().deadline().unwrap(),
        teardown
    );
    peer.drain_trace();
    let mut recorded = None;
    for _ in 0..64 {
        recorded = peer
            .run
            .context
            .failed_measure_refusal_progress(&mut peer.run.clock)
            .unwrap();
        if recorded.is_some() {
            break;
        }
    }
    assert!(
        recorded.is_some(),
        "failed trace-only progress lost the owed refusal"
    );
    let records = peer.records(Kind::Refusal);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["subsystem"], "spu_decoder");
    assert_eq!(records[0]["origin"], "runtime");
    let record = records[0].to_string();
    assert!(record.contains("not_open") && record.contains("measure_delta"));
    assert!(record.contains("\"keep\":45"));
    assert_eq!(peer.run.context.pending_measure_raw(), Some(raw.as_slice()));
    assert_eq!(peer.run.context.halt().unwrap().first(), &first_cause);
    assert_eq!(
        peer.run.context.halt().unwrap().deadline().unwrap(),
        teardown
    );
    peer.run
        .context
        .failed_measure_refusal_progress(&mut peer.run.clock)
        .unwrap();
    assert_eq!(peer.records(Kind::Refusal).len(), 1);
    assert_eq!(
        peer.run
            .context
            .take_measurement(3.into(), &mut peer.decode, &mut peer.run.clock)
            .unwrap_err(),
        first_error
    );
}

#[test]
fn an_earlier_leave_wait_keeps_its_identity_during_measurement() {
    let mut peer = Peer::new();
    let bound = peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    let now = peer.run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    peer.run
        .context
        .arm(PumpDeadline::Leave, leave, &mut peer.run.clock)
        .unwrap();
    peer.run.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(
        peer.advance(PollFlags::POLLIN).unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(peer.run.context.halt().is_none());
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(
        peer.run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    peer.run.context.complete_wait(PumpDeadline::Leave).unwrap();
    peer.send_answer(&answer(17));
    assert!(matches!(peer.finish(), MeasureCompletion::Measured(_)));
    assert_eq!(
        peer.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
}

#[test]
fn failed_refusal_reporting_ends_at_the_existing_halt_bound_without_changing_cause() {
    let mut peer = Peer::new();
    peer.begin(request(), PumpDeadline::Decision);
    peer.sent();
    peer.fill_trace();
    peer.send_refusal(&TokenRefusal::OutOfOrder);
    let first_error = peer.advance(PollFlags::POLLIN).unwrap_err();
    let first_cause = peer.run.context.halt().unwrap().first().clone();
    let deadline = peer.run.context.halt().unwrap().deadline().unwrap();
    let ticket = peer.run.context.pending_author().unwrap().ticket;
    peer.run.clock.advance_to(deadline).unwrap();
    assert_eq!(
        peer.run
            .context
            .failed_measure_refusal_progress(&mut peer.run.clock)
            .unwrap_err(),
        first_error
    );
    assert_eq!(peer.run.context.halt().unwrap().first(), &first_cause);
    assert_eq!(peer.run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(peer.run.context.halt().unwrap().secondary().len(), 1);
    assert!(peer.records(Kind::Refusal).is_empty());
    peer.drain_trace();
    assert_eq!(
        peer.run
            .context
            .failed_measure_refusal_progress(&mut peer.run.clock)
            .unwrap_err(),
        first_error
    );
    assert_eq!(peer.run.context.halt().unwrap().secondary().len(), 1);
    assert_eq!(
        peer.run.context.halt().unwrap().deadline().unwrap(),
        deadline
    );
    assert!(peer.records(Kind::Refusal).is_empty());
}

#[test]
fn invalid_expected_positions_return_the_proposal_without_any_decode_write() {
    for position in [
        MeasurePosition {
            resident: 0,
            prefix_tokens: 0,
            capacity: 0,
        },
        MeasurePosition {
            resident: 45,
            prefix_tokens: 46,
            capacity: 256,
        },
        MeasurePosition {
            resident: 257,
            prefix_tokens: 12,
            capacity: 256,
        },
    ] {
        let mut peer = Peer::new();
        let bound = peer.arm(PumpDeadline::Decision);
        let mut proposed = request();
        proposed.keep = position.resident;
        proposed.position = position;
        let refused = peer
            .run
            .context
            .begin_measurement(
                proposed,
                PumpDeadline::Decision,
                &mut peer.decode,
                &mut peer.run.clock,
            )
            .unwrap_err();
        assert_eq!(
            refused.refusal,
            super::measure_driver::MeasureBeginRefusal::InvalidPosition
        );
        assert_eq!(refused.request.position, position);
        assert_eq!(refused.request.delta, request().delta);
        assert!(refused.failure.is_none());
        assert!(peer.run.context.halt().is_none());
        assert!(receive_bytes(peer.peer.as_fd()).is_none());
        assert_eq!(
            peer.run.context.complete_wait(PumpDeadline::Decision),
            Some(bound)
        );
    }
}
