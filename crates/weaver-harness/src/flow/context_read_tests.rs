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

const CONTROL_NS: u64 = 2_000_000_000;
const EMPTY: &[u8] = br#"{"entries":{"entries":[]}}"#;

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

fn page(snapshot: u64, offset: usize, total: usize, bytes: &[u8]) -> s::StateControlAnswer {
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

    fn finish(
        &mut self,
    ) -> Result<super::super::read::CompletedRead, super::super::read::ReadFailure> {
        for _ in 0..128 {
            self.advance().unwrap();
            if let Some(result) = self.run.context.take_read(&mut self.run.clock).unwrap() {
                return result;
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
            seed_id: "seed".into(),
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
                seed_id: "seed".into(),
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
