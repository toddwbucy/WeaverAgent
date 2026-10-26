//! Paged reads through the actual state stream, recorder and common pump.
//! Scripted member frames exercise the consumer contract, not SQL semantics.

use super::super::clock::{ReadyFd, WaitInterest, WaitOutcome};
use super::super::coordinator::{ControlOperation, OperationStart};
use super::super::pump::LifecyclePorts;
use super::tests::{Fixture, config};
use super::*;
use nix::poll::PollFlags;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use weaver_trace::{Subsystem, TurnRef};
use weaver_types::state_flow as s;

#[path = "context_inspection_tests.rs"]
mod inspection_tests;

const CONTROL_NS: u64 = 2_000_000_000;
const EMPTY: &[u8] = br#"{"entries":{"entries":[]}}"#;

#[path = "context_material_tests.rs"]
mod material_tests;

#[path = "context_read_recall_tests.rs"]
mod recall_tests;

#[path = "context_read_resume_tests.rs"]
mod resume_tests;

#[path = "context_boot_read_tests.rs"]
mod boot_tests;

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn selector() -> s::ReadSelector {
    s::ReadSelector::History {
        noun: "record".into(),
        verb: None,
        condition: None,
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let word = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        encoded.push(ALPHABET[((word >> 18) & 63) as usize] as char);
        encoded.push(ALPHABET[((word >> 12) & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[((word >> 6) & 63) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(word & 63) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

pub(super) fn page(
    snapshot: u64,
    offset: usize,
    total: usize,
    bytes: &[u8],
) -> s::StateControlAnswer {
    s::StateControlAnswer::ReadPage(s::ReadPage {
        snapshot_id: snapshot.into(),
        offset: (offset as u64).into(),
        total_bytes: (total as u64).into(),
        octets: base64(bytes),
        done: offset + bytes.len() == total,
    })
}

fn refused(code: s::ReadRefusalCode) -> s::StateControlAnswer {
    s::StateControlAnswer::ControlRefused {
        code: s::ControlRefusalCode::Read(code),
        detail: "scripted read refusal".into(),
    }
}

struct Member {
    run: Fixture,
    next_operation: u64,
    requests: Vec<s::StateControl>,
}

/// Interleave a caller with actual inspection requests and recorded replies.
/// The callback cannot take the inspection's answer; its original owner does.
pub(super) fn with_three_inspection_holds(
    mut caller: impl FnMut(&mut RecordingContext, &mut crate::flow::clock::testing::FakePumpIo, bool),
) {
    let mut member = Member::new();
    for _ in 0..3 {
        member
            .run
            .context
            .begin_inspection(InspectionKind::Shape, None, &mut member.run.clock)
            .unwrap();
        let request = member.sent();
        let answer = member.answer(&request, s::StateControlAnswer::Shape(vec![]));
        member.receive(&answer).unwrap();
        caller(&mut member.run.context, &mut member.run.clock, false);
        assert!(member.run.context.inspection.is_some());
        let mut completed = false;
        for _ in 0..128 {
            let progress = member
                .run
                .context
                .advance_inspection(&mut ports(&mut member.run.clock))
                .unwrap();
            if progress == InspectionProgress::Complete {
                completed = true;
                break;
            }
        }
        assert!(
            completed,
            "original inspection owner must take its own answer"
        );
        assert_eq!(
            member
                .run
                .context
                .reduce_flow(&mut member.run.clock)
                .unwrap(),
            Advance::Idle
        );
        let before = member.run.clock.waits().len();
        let wake = member.run.clock.now().unwrap() + 1;
        member
            .run
            .clock
            .schedule(
                wake,
                vec![ReadyFd {
                    index: 0,
                    events: PollFlags::POLLIN,
                }],
            )
            .unwrap();
        caller(&mut member.run.context, &mut member.run.clock, false);
        assert!(
            member.run.clock.waits()[before..].iter().any(|wait| {
                wait.absolute_deadline
                    .is_some_and(|end| end > wait.began_ns)
            }),
            "an idle inspection owner must wait before another author attempt"
        );
        let done = member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
            .unwrap();
        assert!(matches!(done, InspectionCompletion::Shape { .. }));
    }
    caller(&mut member.run.context, &mut member.run.clock, true);
}

impl Member {
    fn new() -> Self {
        let mut run = Fixture::new(config());
        run.activate();
        run.context.complete_wait(PumpDeadline::Enter).unwrap();
        Self {
            run,
            next_operation: 2,
            requests: Vec::new(),
        }
    }

    fn now(&mut self) -> u64 {
        self.run.clock.now().unwrap()
    }

    fn begin(&mut self) -> s::StateFrame {
        self.run
            .context
            .begin_read(selector(), None, &mut self.run.clock)
            .unwrap();
        self.next_request()
    }

    fn advance(&mut self) -> Result<ReadProgress, RunFailure> {
        self.run
            .context
            .advance_read(&mut ports(&mut self.run.clock))
    }

    fn pump_at(&mut self, now: u64, events: PollFlags) -> Result<(), RunFailure> {
        self.run
            .clock
            .schedule(now, vec![ReadyFd { index: 0, events }])
            .unwrap();
        self.run
            .context
            .pump_once(&mut ports(&mut self.run.clock))
            .map(|_| ())
    }

    fn next_request(&mut self) -> s::StateFrame {
        for _ in 0..128 {
            self.advance().unwrap();
            if !self.run.context.flow.state().outstanding().is_empty() {
                return self.sent();
            }
        }
        panic!("read did not issue its next request");
    }

    fn sent(&mut self) -> s::StateFrame {
        assert!(!self.run.context.flow.state().outstanding().is_empty());
        for _ in 0..128 {
            let now = self.now() + 1;
            self.pump_at(now, PollFlags::POLLOUT).unwrap();
            if self
                .run
                .context
                .flow
                .state()
                .outstanding()
                .iter()
                .all(|request| request.bytes_sent == request.frame_bytes)
            {
                break;
            }
        }
        assert!(
            self.run
                .context
                .flow
                .state()
                .outstanding()
                .iter()
                .all(|request| request.bytes_sent == request.frame_bytes)
        );
        let mut line = Vec::new();
        BufReader::new(&mut self.run.state)
            .read_until(b'\n', &mut line)
            .unwrap();
        let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
        let s::StateFrame::Control { frame, .. } = &request else {
            panic!("read sent a filing or lifecycle request");
        };
        self.requests.push(frame.clone());
        request
    }

    fn answer(&mut self, request: &s::StateFrame, frame: s::StateControlAnswer) -> s::StateFrame {
        let began = self.now() + 1;
        let observation = s::StateOperationObservation {
            operation_id: self.next_operation.into(),
            parent_id: None,
            request_id: request.common().request_id,
            operation: request.request_operation().unwrap(),
            began_ns: began.into(),
            ended_ns: (began + 1).into(),
            phases: vec![
                s::OperationPhase {
                    name: s::PhaseName::Validate,
                    began_ns: began.into(),
                    ended_ns: began.into(),
                },
                s::OperationPhase {
                    name: s::PhaseName::Encode,
                    began_ns: (began + 1).into(),
                    ended_ns: (began + 1).into(),
                },
            ],
            resource: s::OperationResource {
                cpu_ns: None,
                copied_bytes: Some(0.into()),
                held_bytes: Some(1.into()),
                unavailable: vec![s::ResourceUnavailable {
                    field: s::ResourceField::CpuNs,
                    reason: s::ObservationUnavailable::NotSampled,
                }],
            },
            disk: s::OperationDisk {
                enabled: None,
                before: None,
                after: None,
                unavailable_reason: Some(s::ObservationUnavailable::NotSampled),
            },
        };
        self.next_operation += 1;
        s::StateFrame::ControlAnswer {
            common: request.common().clone(),
            frame,
            observation,
        }
    }

    fn receive(&mut self, answer: &s::StateFrame) -> Result<(), RunFailure> {
        self.run
            .state
            .write_all(&s::encode_state_line(answer, s::FrameDirection::Response).unwrap())
            .unwrap();
        let now = self.now() + 3;
        self.pump_at(now, PollFlags::POLLIN)
    }

    fn respond(&mut self, request: &s::StateFrame, frame: s::StateControlAnswer) {
        let answer = self.answer(request, frame);
        self.receive(&answer).unwrap();
    }

    fn respond_protocol_failure(&mut self, request: &s::StateFrame, frame: s::StateControlAnswer) {
        let answer = self.answer(request, frame);
        match self.receive(&answer) {
            Ok(()) => self.assert_component_failure(),
            Err(failure) => self.assert_protocol_failure(failure),
        }
    }

    fn finish(&mut self) -> Result<RecordedRead, super::super::read::ReadFailure> {
        for _ in 0..128 {
            self.advance().unwrap();
            if let Some(result) = self.run.context.take_read(&mut self.run.clock).unwrap() {
                return match result {
                    ReadCompletion::Recorded(read) => Ok(read),
                    ReadCompletion::Refused(refusal) => Err(refusal),
                    ReadCompletion::Cancelled(_) => panic!("unexpected cancelled read"),
                };
            }
        }
        panic!("read did not finish");
    }

    fn no_result(&mut self) {
        assert!(
            self.run
                .context
                .take_read(&mut self.run.clock)
                .unwrap()
                .is_none()
        );
    }

    fn no_request(&mut self) {
        self.run.state.set_nonblocking(true).unwrap();
        let mut byte = [0];
        let result = self.run.state.read(&mut byte);
        self.run.state.set_nonblocking(false).unwrap();
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    }

    fn bound(&self) -> AbsoluteBound {
        self.run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap()
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

    fn record_completion_only(&mut self) {
        for _ in 0..32 {
            // Hold the real coordinator proof before the context read owner
            // reduces it. No receipt or recorded sequence is manufactured.
            self.run
                .context
                .flow
                .advance(
                    &mut FlowRecording {
                        barrier: &mut self.run.context.barrier,
                        inbox: &mut self.run.context.inbox,
                        budget: &mut self.run.context.budget,
                        author: &self.run.context.author,
                    },
                    &mut self.run.clock,
                    1,
                )
                .unwrap();
            if self.run.context.flow.operation_completion().is_some() {
                return;
            }
        }
        panic!("coordinator did not retain a recorded completion");
    }

    fn assert_component_failure(&mut self) {
        for _ in 0..128 {
            if let Err(failure) = self.advance() {
                self.assert_protocol_failure(failure);
                return;
            }
        }
        panic!("invalid member response did not fail the read");
    }

    fn assert_protocol_failure(&mut self, failure: RunFailure) {
        assert!(
            matches!(
                failure,
                RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    point: HaltPoint::StateReply,
                    ..
                }
            ),
            "unexpected refusal: {failure:?}"
        );
        assert!(self.run.context.flow.halt().is_some());
        assert!(self.run.context.take_read(&mut self.run.clock).is_err());
        let mut byte = [0];
        assert_eq!(
            self.run.state.read(&mut byte).unwrap(),
            0,
            "failed read did not close its snapshot channel"
        );
    }

    fn fill_trace(&mut self) {
        let payload =
            serde_json::value::to_raw_value(&serde_json::json!({"fixture":"x".repeat(32_768)}))
                .unwrap();
        for _ in 0..weaver_trace::serving::ORDINARY_CAPACITY + 64 {
            if self.run.context.barrier.recorder().boundary().queued
                == weaver_trace::serving::ORDINARY_CAPACITY
            {
                return;
            }
            let now = self.now();
            assert!(matches!(
                self.run.context.barrier.submit_authored_with_clock(
                    AuthoredEvent {
                        kind: Kind::ModelMeasurement,
                        subsystem: Subsystem::Harness,
                        turn: Some(TurnRef("pressure".into())),
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
        panic!("trace did not reach ordinary queue capacity");
    }
}

#[test]
fn read_pages_are_private_until_one_exact_release_is_recorded() {
    let mut member = Member::new();
    let initial = member.begin();
    let bound = member.bound();
    assert_eq!(bound.deadline_ns() - bound.began_ns(), CONTROL_NS);
    assert!(
        matches!(&member.requests[0], s::StateControl::Read { selector: got } if got == &selector())
    );
    member.respond(&initial, page(7, 0, EMPTY.len(), &EMPTY[..8]));
    member.no_result();
    let next = member.next_request();
    assert_eq!(member.bound(), bound);
    assert_eq!(
        member.requests[1],
        s::StateControl::Next {
            snapshot_id: 7.into(),
            offset: 8.into()
        }
    );
    assert_eq!(
        next.common().request_id.0,
        initial.common().request_id.0 + 1
    );
    member.respond(&next, page(7, 8, EMPTY.len(), &EMPTY[8..]));
    member.no_result();
    let release = member.next_request();
    assert_eq!(
        member.requests[2],
        s::StateControl::Release {
            snapshot_id: 7.into()
        }
    );
    assert_eq!(member.bound(), bound);
    member.no_result();
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 7.into(),
        },
    );
    member.no_result();
    let completed = member.finish().unwrap();
    assert_eq!(completed.selector(), &selector());
    assert_eq!(completed.snapshot_id(), 7.into());
    assert_eq!(completed.raw_bytes(), EMPTY);
    assert!(
        matches!(completed.result(), s::QueryResult::Entries { entries } if entries.is_empty())
    );
    assert_eq!(member.requests.len(), 3);
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), 4);
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    member.no_result();
    member.no_request();
    assert!(member.run.context.flow.ledger().facts().next().is_none());
}

#[test]
fn initial_read_refusal_needs_no_snapshot_release_and_allows_a_new_read() {
    let mut member = Member::new();
    let initial = member.begin();
    member.respond(&initial, refused(s::ReadRefusalCode::NotFound));
    let result = member.finish();
    assert!(matches!(
        result,
        Err(super::super::read::ReadFailure::Refused {
            code: s::ReadRefusalCode::NotFound,
            ..
        })
    ));
    assert_eq!(member.requests.len(), 1);
    member.no_request();
    assert!(member.run.context.halt().is_none());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    let second = member.begin();
    assert!(second.common().request_id > initial.common().request_id);
}

#[test]
fn a_later_page_refusal_discards_partial_data_only_after_snapshot_release() {
    let mut member = Member::new();
    let initial = member.begin();
    let bound = member.bound();
    member.respond(&initial, page(11, 0, EMPTY.len(), &EMPTY[..8]));
    let next = member.next_request();
    member.respond(&next, refused(s::ReadRefusalCode::ResultUnavailable));
    member.no_result();
    let release = member.next_request();
    assert_eq!(
        member.requests.last().unwrap(),
        &s::StateControl::Release {
            snapshot_id: 11.into()
        }
    );
    assert_eq!(member.bound(), bound);
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 11.into(),
        },
    );
    assert!(matches!(
        member.finish(),
        Err(super::super::read::ReadFailure::Refused {
            code: s::ReadRefusalCode::ResultUnavailable,
            ..
        })
    ));
    assert!(member.run.context.halt().is_none());
    assert_eq!(member.requests.len(), 3);
}

#[test]
fn refusal_codes_from_other_legs_close_the_read_channel_without_a_guessed_release() {
    for code in [
        s::ReadRefusalCode::ReadBusy,
        s::ReadRefusalCode::UnknownSnapshot,
        s::ReadRefusalCode::WrongOffset,
    ] {
        let mut member = Member::new();
        let initial = member.begin();
        member.respond_protocol_failure(&initial, refused(code));
        assert_eq!(member.requests.len(), 1);
    }
    for code in [
        s::ReadRefusalCode::InvalidSelector,
        s::ReadRefusalCode::ReadBusy,
        s::ReadRefusalCode::ResultTooLarge,
    ] {
        let mut member = Member::new();
        let initial = member.begin();
        member.respond(&initial, page(7, 0, EMPTY.len(), &EMPTY[..3]));
        let next = member.next_request();
        member.respond_protocol_failure(&next, refused(code));
        assert_eq!(member.requests.len(), 2);
        assert!(matches!(member.requests[1], s::StateControl::Next { .. }));
    }
}

#[test]
fn read_admission_preserves_pending_pair_and_author_owners() {
    for pending in 0..5 {
        let mut member = Member::new();
        let mut event = super::tests::event(
            Kind::ModelMeasurement,
            Some(Payload::ModelMeasurement(
                serde_json::value::to_raw_value(&serde_json::json!({"pending":"owner"})).unwrap(),
            )),
        );
        event.turn = Some(TurnRef("pending-owner".into()));
        match pending {
            0 => {
                member.run.context.pair_work = Some(super::pair_driver::PairTransaction::new(
                    AbsoluteBound::after(member.now(), 1_000_000).unwrap(),
                ));
            }
            1 => member.run.context.waiting_content = Some(WaitingAuthor::ordinary(event)),
            2 => {
                let sequence = member.run.author(event).unwrap();
                // A completed author remains owned across a lifecycle yield.
                member.run.context.completed_author = Some(sequence);
            }
            3 => {
                member.fill_trace();
                let now = member.now();
                assert!(matches!(
                    member.run.context.barrier.submit_authored_with_clock(
                        event,
                        RecordPurpose::Observation,
                        &member.run.context.author,
                        1,
                        &mut member.run.context.inbox,
                        &mut member.run.context.budget,
                        &mut || Ok(now),
                    ),
                    RecordProgress::Pending
                ));
                assert!(member.run.context.barrier.pending().is_some());
                assert!(!member.run.context.flow.has_pending_record());
            }
            _ => {
                let now = member.now();
                member
                    .run
                    .context
                    .flow
                    .input_started(
                        &mut FlowRecording {
                            barrier: &mut member.run.context.barrier,
                            inbox: &mut member.run.context.inbox,
                            budget: &mut member.run.context.budget,
                            author: &member.run.context.author,
                        },
                        &mut member.run.clock,
                        1.into(),
                        now,
                    )
                    .unwrap();
                member.fill_trace();
                member
                    .run
                    .clock
                    .advance_to(
                        now + member
                            .run
                            .context
                            .declaration
                            .observation_interval
                            .elapsed_ns
                            .0,
                    )
                    .unwrap();
                for _ in 0..16 {
                    member
                        .run
                        .context
                        .reduce_flow(&mut member.run.clock)
                        .unwrap();
                    if member.run.context.flow.has_pending_record() {
                        break;
                    }
                }
                assert!(member.run.context.flow.has_pending_record());
            }
        }
        let pending_author = format!("{:?}", member.run.context.pending_author());
        let waiting = member.run.context.waiting_content.is_some();
        let completed = member.run.context.completed_author;
        let pending_record = member.run.context.flow.has_pending_record();
        let pair = member.run.context.pair_work.is_some();
        assert!(member.run.context.flow.state().outstanding().is_empty());
        let refused = member
            .run
            .context
            .begin_read(
                selector(),
                Some(TurnRef("read-owner".into())),
                &mut member.run.clock,
            )
            .unwrap_err();
        assert_eq!(
            refused.refusal,
            read_driver::ReadBeginRefusal::Busy,
            "owner {pending}"
        );
        assert_eq!(refused.selector, selector());
        assert_eq!(refused.turn, Some(TurnRef("read-owner".into())));
        assert!(refused.failure.is_none());
        assert!(member.run.context.read.is_none());
        assert!(
            member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
        assert!(member.run.context.flow.state().outstanding().is_empty());
        assert_eq!(
            format!("{:?}", member.run.context.pending_author()),
            pending_author
        );
        assert_eq!(member.run.context.waiting_content.is_some(), waiting);
        assert_eq!(member.run.context.completed_author, completed);
        assert_eq!(member.run.context.flow.has_pending_record(), pending_record);
        assert_eq!(member.run.context.pair_work.is_some(), pair);
        assert!(member.run.context.failure().is_none());
        member.no_request();
    }
}

#[test]
fn read_admission_preserves_gate_and_cut_barriers_without_allocating_a_request() {
    for key in [PumpDeadline::Gate, PumpDeadline::Cut] {
        let mut member = Member::new();
        let bound = AbsoluteBound::after(member.now(), 1_000_000).unwrap();
        member
            .run
            .context
            .arm(key, bound, &mut member.run.clock)
            .unwrap();
        let refused = member
            .run
            .context
            .begin_read(selector(), None, &mut member.run.clock)
            .unwrap_err();
        assert_eq!(refused.refusal, read_driver::ReadBeginRefusal::Busy);
        assert!(refused.failure.is_none());
        assert!(member.run.context.read.is_none());
        assert!(member.run.context.flow.pending_control().is_none());
        assert!(member.run.context.flow.state().outstanding().is_empty());
        assert!(
            member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
        assert_eq!(member.run.context.deadlines.get(key), Some(bound));
        member.no_request();
        assert_eq!(member.run.context.complete_wait(key), Some(bound));
        let admitted = member.begin();
        assert_eq!(admitted.common().request_id, 2.into());
        assert!(member.run.context.failure().is_none());
    }
}

#[test]
fn read_admission_preserves_recorded_classifier_release_with_an_empty_author_barrier() {
    use super::super::ste100_channel::{Receive, Tag};

    let mut input = super::input_tests::InputFixture::new();
    input
        .run
        .author(AuthoredEvent {
            kind: Kind::MessageAssistant,
            subsystem: Subsystem::Harness,
            turn: Some(TurnRef("source-turn".into())),
            source: super::super::author::AuthoredSource::Model,
            causal_parent: Some(Sequence(0)),
            payload: Some(Payload::Message(
                serde_json::value::to_raw_value(&serde_json::json!({
                    "role": "assistant", "content": [{"type": "text", "text": "Read file."}]
                }))
                .unwrap(),
            )),
        })
        .unwrap();
    let mut request = None;
    for _ in 0..128 {
        input.run.context.reduce_flow(&mut input.run.clock).unwrap();
        input.classifier_transport();
        if let Receive::Complete(message) = input.peer.try_receive().unwrap() {
            assert_eq!(message.tag, Tag::Request);
            request = Some(
                s::decode_strict::<s::Ste100Request>(
                    std::str::from_utf8(&message.payload).unwrap(),
                )
                .unwrap(),
            );
            break;
        }
    }
    let request = request.expect("selected source crosses the real classifier socket");
    let reply = s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "synthetic source rejection".into(),
            spans: vec![],
        },
    };
    input.reply(&reply);
    input
        .run
        .context
        .flow
        .interrupt_classifier_terminal_probes_for_test(1);
    for _ in 0..128 {
        input.run.context.reduce_flow(&mut input.run.clock).unwrap();
        if input
            .run
            .context
            .flow
            .pending_classifier_recorded_request()
            .is_some()
        {
            break;
        }
    }
    let context = &mut input.run.context;
    assert!(context.flow.pending_classifier_recorded_request().is_some());
    assert!(context.flow.has_pending_record());
    assert!(context.barrier.pending().is_none());
    assert!(context.waiting_content.is_none());
    assert!(context.completed_author.is_none());
    assert!(context.output.is_none());
    assert!(context.input.snapshot().is_none());
    assert!(context.flow.pending_check().is_none());
    assert!(context.flow.pending_control().is_none());
    assert!(context.flow.operation_completion().is_none());
    assert!(context.flow.state().outstanding().is_empty());
    assert!(context.flow.state().held_measurement().is_none());
    let refused = context
        .begin_read(selector(), None, &mut input.run.clock)
        .unwrap_err();
    assert_eq!(refused.refusal, read_driver::ReadBeginRefusal::Busy);
    assert!(refused.failure.is_none());
    assert!(context.read.is_none());
    assert!(context.deadlines.get(PumpDeadline::StateControl).is_none());
    assert!(context.flow.pending_control().is_none());
    assert!(context.flow.has_pending_record());
    assert_eq!(context.flow.classifier().outstanding(), Some(&request));
    assert_eq!(context.flow.classifier().held(), Some(&reply));
    assert!(context.failure().is_none());
    assert_eq!(input.records(Kind::Ste100Result).len(), 1);
    input.run.context.reduce_flow(&mut input.run.clock).unwrap();
    assert!(
        input
            .run
            .context
            .flow
            .pending_classifier_recorded_request()
            .is_none()
    );
    assert_eq!(input.records(Kind::Ste100Result).len(), 1);
}

struct InputHandoffClock {
    reads: usize,
}

impl PumpIo for InputHandoffClock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.reads += 1;
        Ok(if self.reads <= 4 { 104 } else { 109 })
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("input handoff records before its lifecycle interruption");
    }
}

#[test]
fn read_admission_preserves_an_interrupted_input_source_handoff() {
    let mut input = super::input_tests::InputFixture::new();
    input.begin(weaver_types::TurnFrame::carry_from(
        br#"{"text":"Read file."}"#,
        42,
    ));
    let (_, _, token) = input.accepted_token();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    input
        .run
        .context
        .arm(PumpDeadline::Leave, bound, &mut input.run.clock)
        .unwrap();
    let mut clock = InputHandoffClock { reads: 0 };
    let interrupted = input
        .run
        .context
        .consume_input(token, TurnRef("input-owner".into()), &mut ports(&mut clock))
        .unwrap_err();
    assert_eq!(
        interrupted.failure,
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound
        }
    );
    assert!(input.run.context.input_turn.is_some());
    let snapshot = input.run.context.input.snapshot();
    let completed = input.run.context.completed_author;
    input
        .run
        .context
        .complete_wait(PumpDeadline::Leave)
        .unwrap();
    input.run.clock.advance_to(109).unwrap();
    let refused = input
        .run
        .context
        .begin_read(selector(), None, &mut input.run.clock)
        .unwrap_err();
    assert_eq!(refused.refusal, read_driver::ReadBeginRefusal::Busy);
    assert!(refused.failure.is_none());
    assert!(input.run.context.input_turn.is_some());
    assert_eq!(input.run.context.input.snapshot(), snapshot);
    assert_eq!(input.run.context.completed_author, completed);
    assert!(
        input
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert!(input.run.context.read.is_none());
    assert!(input.run.context.failure().is_none());
    assert!(
        input
            .run
            .context
            .resume_input_turn(&mut ports(&mut input.run.clock))
            .unwrap()
            .is_some()
    );
}

#[test]
fn read_refuses_another_begin_and_generic_consumers_cannot_steal_its_completion() {
    let mut member = Member::new();
    let initial = member.begin();
    let other = s::ReadSelector::OpenPairs {};
    let refusal = member
        .run
        .context
        .begin_read(other.clone(), None, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(refusal.selector, other);
    assert!(refusal.failure.is_none());
    member.respond(&initial, page(1, 0, EMPTY.len(), EMPTY));
    member.record_completion_only();
    assert!(member.run.context.flow.operation_completion().is_some());
    assert!(
        member
            .run
            .context
            .take_operation_completion(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    let release = member.next_request();
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 1.into(),
        },
    );
    assert!(member.finish().is_ok());
    assert_eq!(member.requests.len(), 2);
}

#[test]
fn read_cannot_consume_an_existing_unrelated_control_completion() {
    let mut member = Member::new();
    let operation = ControlOperation::try_from((s::StateControl::Held {}, None)).unwrap();
    let start = member
        .run
        .context
        .flow
        .begin_control(
            &mut FlowRecording {
                barrier: &mut member.run.context.barrier,
                inbox: &mut member.run.context.inbox,
                budget: &mut member.run.context.budget,
                author: &member.run.context.author,
            },
            &mut member.run.clock,
            operation,
        )
        .unwrap();
    assert!(matches!(start, OperationStart::Started { .. }));
    let held = member.sent();
    member.respond(&held, s::StateControlAnswer::Held { count: 9.into() });
    for _ in 0..32 {
        member
            .run
            .context
            .reduce_flow(&mut member.run.clock)
            .unwrap();
        if member.run.context.flow.operation_completion().is_some() {
            break;
        }
    }
    let refused = member
        .run
        .context
        .begin_read(selector(), None, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(refused.selector, selector());
    assert!(refused.failure.is_none());
    assert!(
        matches!(member.run.context.take_operation_completion(&mut member.run.clock).unwrap(), Some(OperationCompletion::State { request: s::StateControl::Held {}, completion, .. }) if matches!(completion.answer, s::StateFrame::ControlAnswer { frame:s::StateControlAnswer::Held{count},.. } if count == 9.into()))
    );
    assert_eq!(member.requests, vec![s::StateControl::Held {}]);
    member.no_request();
    assert!(member.run.context.halt().is_none());
}

#[test]
fn invalid_page_identity_offset_and_total_are_component_failures() {
    for case in 0..3 {
        let mut member = Member::new();
        let initial = member.begin();
        member.respond(&initial, page(2, 0, EMPTY.len(), &EMPTY[..8]));
        let next = member.next_request();
        let mut frame = page(2, 8, EMPTY.len(), &EMPTY[8..]);
        let s::StateControlAnswer::ReadPage(ref mut value) = frame else {
            unreachable!()
        };
        match case {
            0 => value.snapshot_id = 3.into(),
            1 => {
                value.offset = 7.into();
                value.done = false;
            }
            _ => {
                value.total_bytes = (EMPTY.len() as u64 + 1).into();
                value.done = false;
            }
        }
        member.respond_protocol_failure(&next, frame);
        assert_eq!(member.requests.len(), 2);
    }
}

#[test]
fn a_wrong_result_tag_or_invalid_json_never_becomes_read_content() {
    for bytes in [
        br#"{"open_pairs":{"pairs":[]}}"#.as_slice(),
        b"not JSON".as_slice(),
    ] {
        let mut member = Member::new();
        let initial = member.begin();
        member.respond(&initial, page(5, 0, bytes.len(), bytes));
        member.assert_component_failure();
        assert_eq!(member.requests.len(), 1);
    }
}

#[test]
fn a_different_control_answer_cannot_close_the_read() {
    let mut member = Member::new();
    let initial = member.begin();
    member.respond_protocol_failure(&initial, s::StateControlAnswer::Held { count: 0.into() });
    assert_eq!(member.requests.len(), 1);
}

#[test]
fn release_must_acknowledge_the_exact_snapshot() {
    let mut member = Member::new();
    let initial = member.begin();
    member.respond(&initial, page(9, 0, EMPTY.len(), EMPTY));
    let release = member.next_request();
    member.respond_protocol_failure(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 10.into(),
        },
    );
    assert_eq!(member.requests.len(), 2);
}

#[test]
fn a_refused_release_is_a_component_failure_and_never_a_successful_read() {
    let mut member = Member::new();
    let initial = member.begin();
    member.respond(&initial, page(9, 0, EMPTY.len(), EMPTY));
    let release = member.next_request();
    member.respond(&release, refused(s::ReadRefusalCode::UnknownSnapshot));
    member.assert_component_failure();
    assert_eq!(member.requests.len(), 2);
}

#[test]
fn pages_may_split_utf8_without_changing_raw_block_spelling() {
    let mut member = Member::new();
    let text = "Read caf\u{e9}.";
    let raw = format!(r#"{{"type":"text","text":"Read\u0020caf{}."}}"#, '\u{e9}');
    let entry = s::ReadEntry {
        source: s::ReadSource::Seed(s::SeedSource {
            seed_id: "0123456789abcdef0123456789abcdef".into(),
            ordinal: 0.into(),
        }),
        raw_block: serde_json::value::RawValue::from_string(raw.clone()).unwrap(),
        text: text.into(),
        bindings: vec![s::ReadBinding {
            ordinal: 0.into(),
            value: serde_json::from_value(serde_json::json!({
                "statement_span":{"start":"0","end":"11"},
                "verb":"read","verb_span":{"start":"0","end":"4"},
                "condition":null,"condition_span":null,
                "nouns":[{"value":"caf\u{e9}","span":{"start":"5","end":"10"}}]
            }))
            .unwrap(),
        }],
        selected_span: Some(s::SelectedSpan {
            span_ref: s::SpanRef {
                seed_id: "0123456789abcdef0123456789abcdef".into(),
                local_id: 0.into(),
                revision: 1.into(),
            },
            start: 0.into(),
            end: 11.into(),
        }),
        step_query: None,
        pairs: Vec::new(),
    };
    let bytes = serde_json::to_vec(&s::QueryResult::Entries {
        entries: vec![entry],
    })
    .unwrap();
    let split = bytes.iter().position(|byte| *byte == 0xc3).unwrap() + 1;
    assert!(std::str::from_utf8(&bytes[..split]).is_err());
    member
        .run
        .context
        .begin_read(
            s::ReadSelector::SystemPrompt {},
            None,
            &mut member.run.clock,
        )
        .unwrap();
    let initial = member.next_request();
    member.respond(&initial, page(12, 0, bytes.len(), &bytes[..split]));
    member.no_result();
    let next = member.next_request();
    assert_eq!(
        member.requests[1],
        s::StateControl::Next {
            snapshot_id: 12.into(),
            offset: (split as u64).into()
        }
    );
    member.respond(&next, page(12, split, bytes.len(), &bytes[split..]));
    let release = member.next_request();
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 12.into(),
        },
    );
    let completed = member.finish().unwrap();
    assert_eq!(completed.raw_bytes(), bytes);
    let s::QueryResult::Entries { entries } = completed.result() else {
        panic!("entries")
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, text);
    assert_eq!(entries[0].raw_block.get(), raw);
}

#[test]
fn read_uses_one_absolute_bound_through_next_and_release() {
    let mut member = Member::new();
    let initial = member.begin();
    let bound = member.bound();
    member
        .run
        .clock
        .advance_to(bound.began_ns() + CONTROL_NS / 2)
        .unwrap();
    member.respond(&initial, page(2, 0, EMPTY.len(), &EMPTY[..8]));
    let next = member.next_request();
    assert_eq!(member.bound(), bound);
    member
        .run
        .clock
        .advance_to(bound.deadline_ns() - 20)
        .unwrap();
    member.respond(&next, page(2, 8, EMPTY.len(), &EMPTY[8..]));
    let _release = member.next_request();
    assert_eq!(member.bound(), bound);
    member.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        member.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        })
    ));
    assert_eq!(
        member.run.context.halt_wait().unwrap().deadline_ns.0,
        bound.deadline_ns()
    );
    assert!(member.run.context.take_read(&mut member.run.clock).is_err());
}

#[test]
fn an_earlier_enter_or_leave_deadline_keeps_lifecycle_ownership() {
    for key in [PumpDeadline::Enter, PumpDeadline::Leave] {
        let mut member = Member::new();
        let now = member.now();
        let bound = AbsoluteBound::after(now, 20).unwrap();
        member
            .run
            .context
            .arm(key, bound, &mut member.run.clock)
            .unwrap();
        let _initial = member.begin();
        member.run.clock.advance_to(bound.deadline_ns()).unwrap();
        assert_eq!(
            member.advance().unwrap_err(),
            RunFailure::Wait { key, bound }
        );
        assert!(member.run.context.halt().is_none());
    }
}

#[test]
fn trace_pressure_retains_the_exact_page_without_renewing_the_read_bound() {
    let mut member = Member::new();
    let initial = member.begin();
    let bound = member.bound();
    let recorded = member.records(Kind::StateStoreMeasurement).len();
    member.fill_trace();
    member.respond(&initial, page(4, 0, EMPTY.len(), EMPTY));
    let held = member
        .run
        .context
        .flow
        .state()
        .held_measurement()
        .unwrap()
        .clone();
    for _ in 0..32 {
        member.advance().unwrap();
    }
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), recorded);
    assert_eq!(
        member.run.context.flow.state().held_measurement(),
        Some(&held)
    );
    assert_eq!(member.bound(), bound);
    member.no_result();
    member.no_request();
    assert_eq!(
        member.run.context.pending_author().unwrap().event.kind,
        Kind::StateStoreMeasurement
    );
    member.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        member.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        })
    ));
    assert!(member.run.context.take_read(&mut member.run.clock).is_err());
}

#[test]
fn state_eof_during_a_read_is_not_an_empty_success() {
    let mut member = Member::new();
    let _initial = member.begin();
    member.run.state.shutdown(Shutdown::Both).unwrap();
    let now = member.now() + 1;
    assert!(member.pump_at(now, PollFlags::POLLIN).is_err());
    assert!(member.run.context.halt().is_some());
    assert!(member.run.context.take_read(&mut member.run.clock).is_err());
}

#[test]
fn a_read_wait_cannot_be_disarmed_during_pages_or_release() {
    let mut member = Member::new();
    let initial = member.begin();
    let bound = member.bound();
    assert!(
        member
            .run
            .context
            .complete_wait(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(member.bound(), bound);
    member.respond(&initial, page(3, 0, EMPTY.len(), &EMPTY[..8]));
    let next = member.next_request();
    assert!(
        member
            .run
            .context
            .complete_wait(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(member.bound(), bound);
    member.respond(&next, page(3, 8, EMPTY.len(), &EMPTY[8..]));
    let release = member.next_request();
    assert!(
        member
            .run
            .context
            .complete_wait(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(member.bound(), bound);
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 3.into(),
        },
    );
    assert!(member.finish().is_ok());
}

#[test]
fn release_cannot_cross_the_gap_between_a_page_and_its_next_request() {
    let mut member = Member::new();
    let initial = member.begin();
    member.respond(&initial, page(3, 0, EMPTY.len(), &EMPTY[..8]));
    for _ in 0..32 {
        member
            .run
            .context
            .reduce_flow(&mut member.run.clock)
            .unwrap();
        if member.run.context.flow.state().held_measurement().is_none()
            && member.run.context.flow.pending_control().is_none()
        {
            break;
        }
    }
    assert!(member.run.context.flow.pending_control().is_none());
    assert!(member.run.context.flow.state().outstanding().is_empty());
    assert!(member.run.context.read.is_some());
    let leave = AbsoluteBound::after(member.now(), 1000).unwrap();
    assert!(matches!(
        member
            .run
            .context
            .begin_releasing(leave, &mut member.run.clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_ne!(member.run.context.phase(), RecordingPhase::Releasing);
    assert!(member.run.context.take_read(&mut member.run.clock).is_err());
    assert_eq!(member.requests.len(), 1);
}

struct ValidationClock {
    before: u64,
    deadline: u64,
    reads: usize,
}

impl PumpIo for ValidationClock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.reads += 1;
        Ok(if self.reads == 1 {
            self.before
        } else {
            self.deadline
        })
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("the actual answer is recorded before validation starts");
    }
}

#[test]
fn read_validation_retains_page_and_release_continuations_across_leave() {
    for at_release in [false, true] {
        let mut member = Member::new();
        let initial = member.begin();
        let bound = member.bound();
        if at_release {
            member.respond(&initial, page(3, 0, EMPTY.len(), EMPTY));
            let release = member.next_request();
            member.respond(
                &release,
                s::StateControlAnswer::Released {
                    snapshot_id: 3.into(),
                },
            );
        } else {
            member.respond(&initial, page(3, 0, EMPTY.len(), &EMPTY[..8]));
        }
        member.record_completion_only();
        let recorded = member.records(Kind::StateStoreMeasurement).len();
        let now = member.now();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        member
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
            .unwrap();
        let mut clock = ValidationClock {
            before: now,
            deadline: leave.deadline_ns(),
            reads: 0,
        };
        assert_eq!(
            member
                .run
                .context
                .settle_recorded_read(&mut clock)
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave,
            }
        );
        assert!(
            clock.reads >= 2,
            "Leave must interrupt the post-validation check"
        );
        assert!(member.run.context.flow.operation_completion().is_none());
        assert!(member.run.context.flow.state().outstanding().is_empty());
        assert!(member.run.context.flow.state().held_measurement().is_none());
        assert_eq!(member.bound(), bound);
        assert!(member.run.context.halt().is_none());
        assert!(
            member
                .run
                .context
                .complete_wait(PumpDeadline::StateControl)
                .is_none()
        );
        // No partial content or completed release bypasses the lifecycle wait.
        assert_eq!(
            member.run.context.take_read(&mut clock).unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave,
            }
        );
        member.no_request();
        assert_eq!(member.records(Kind::StateStoreMeasurement).len(), recorded);
        assert_eq!(
            member.run.context.complete_wait(PumpDeadline::Leave),
            Some(leave)
        );
        member.run.clock.advance_to(leave.deadline_ns()).unwrap();
        // Resume through the public owner. It must continue the consumed answer,
        // without waiting for a second copy or issuing the same request again.
        let progress = member.advance().unwrap();
        assert_eq!(member.records(Kind::StateStoreMeasurement).len(), recorded);
        if at_release {
            assert_eq!(progress, ReadProgress::Complete);
        } else {
            assert_eq!(progress, ReadProgress::Progress);
            assert_eq!(member.bound(), bound);
            let next = member.sent();
            assert!(matches!(
                &next,
                s::StateFrame::Control {
                    frame: s::StateControl::Next { snapshot_id, offset },
                    ..
                } if *snapshot_id == 3.into() && *offset == 8.into()
            ));
            assert_eq!(
                next.common().request_id.0,
                initial.common().request_id.0 + 1
            );
            member.respond(&next, page(3, 8, EMPTY.len(), &EMPTY[8..]));
            let release = member.next_request();
            assert_eq!(member.bound(), bound);
            member.respond(
                &release,
                s::StateControlAnswer::Released {
                    snapshot_id: 3.into(),
                },
            );
        }
        let result = member.finish().unwrap();
        assert_eq!(result.raw_bytes(), EMPTY);
        assert_eq!(
            member
                .requests
                .iter()
                .filter(|request| matches!(request, s::StateControl::Release { .. }))
                .count(),
            1
        );
        assert_eq!(member.requests.len(), if at_release { 2 } else { 3 });
        assert!(
            member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
        member.no_result();
        member.no_request();
        assert!(member.run.context.halt().is_none());
    }
}

#[test]
fn validation_crossing_the_deadline_cannot_publish_a_page_or_released_result() {
    for at_release in [false, true] {
        let mut member = Member::new();
        let initial = member.begin();
        let bound = member.bound();
        member.respond(&initial, page(3, 0, EMPTY.len(), EMPTY));
        if at_release {
            let release = member.next_request();
            member.respond(
                &release,
                s::StateControlAnswer::Released {
                    snapshot_id: 3.into(),
                },
            );
        }
        member.record_completion_only();
        let mut clock = ValidationClock {
            before: bound.deadline_ns() - 1,
            deadline: bound.deadline_ns(),
            reads: 0,
        };
        assert!(matches!(
            member.run.context.settle_recorded_read(&mut clock),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ControlBound,
                ..
            })
        ));
        assert!(clock.reads >= 2, "validation did not recheck the clock");
        assert!(member.run.context.take_read(&mut clock).is_err());
        assert_eq!(
            member.run.context.halt_wait().unwrap().deadline_ns.0,
            bound.deadline_ns()
        );
        assert_eq!(member.requests.len(), if at_release { 2 } else { 1 });
    }
}

#[path = "context_halt_tests.rs"]
mod halt_tests;
