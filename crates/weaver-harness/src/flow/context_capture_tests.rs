//! Capture transactions through the actual context, state socket and recorder.
//! Synthetic member answers test the harness mechanics, not image publication.

#[path = "context_capture_edge_tests.rs"]
mod edge_tests;

use super::super::clock::ReadyFd;
use super::super::coordinator::{CapturePhase, ControlOperation, OperationStart};
use super::super::pump::LifecyclePorts;
use super::tests::{Fixture, config, event};
use super::*;
use nix::poll::PollFlags;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::time::{Duration, Instant};
use weaver_trace::serving::payloads as p;
use weaver_trace::{Subsystem, TurnRef};
use weaver_types::state_flow as s;

const SAVE_NS: u64 = 10_000_000_000;
const SNAPSHOT_NS: u64 = 1000;
const SMALL_CONTROL_NS: u64 = 2_000_000_000;

#[test]
fn held_boundary_capture_returns_busy_without_freezing_a_frontier_or_new_window() {
    use super::held_input_tests::{HOLDS, hold};
    for held in HOLDS {
        for expired in [false, true] {
            let mut run = active();
            let window = Member::new(&mut run).requested(42, SAVE_NS);
            let before = run.context.barrier.recorder().structure().len();
            let waits = format!("{:?}", run.context.deadlines);
            let originals = hold(&mut run.context, held);
            assert_eq!(
                run.context
                    .prepare_member_capture(42.into(), window, &mut run.clock),
                Err(RunFailure::Busy(held))
            );
            assert!(run.context.member_capture.is_none());
            assert!(run.context.flow.state().outstanding().is_empty());
            assert!(run.context.failure().is_none());
            assert_eq!(run.context.phase(), RecordingPhase::Entered);
            assert_eq!(format!("{:?}", run.context.deadlines), waits);
            assert_eq!(run.context.barrier.recorder().structure().len(), before);
            originals.assert_held(&run.context);
            originals.take(&mut run.context);
            if expired {
                run.clock.advance_to(window.deadline_ns.0).unwrap();
                assert_eq!(
                    run.context
                        .prepare_member_capture(42.into(), window, &mut run.clock),
                    Err(RunFailure::Wait {
                        key: PumpDeadline::CaptureWindow,
                        bound: AbsoluteBound::from_absolute(
                            window.began_ns.0,
                            window.deadline_ns.0
                        )
                        .unwrap()
                    })
                );
                assert!(run.context.member_capture.is_none());
                assert_eq!(format!("{:?}", run.context.deadlines), waits);
            } else {
                run.context
                    .prepare_member_capture(42.into(), window, &mut run.clock)
                    .unwrap();
                assert!(run.context.member_capture.is_some());
                assert_eq!(
                    run.context
                        .deadlines
                        .get(PumpDeadline::CaptureWindow)
                        .unwrap()
                        .deadline_ns(),
                    window.deadline_ns.0
                );
            }
        }
    }
}

#[test]
fn held_boundary_capture_observes_failure_before_busy_and_take_does_not_revive_it() {
    use super::held_input_tests::{HOLDS, hold};
    for held in HOLDS {
        for already_failed in [false, true] {
            let mut run = active();
            let window = Member::new(&mut run).requested(42, SAVE_NS);
            let originals = hold(&mut run.context, held);
            let first = already_failed.then(|| {
                run.context.fail(
                    &mut run.clock,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Barrier,
                    "first capture cause",
                )
            });
            if !already_failed {
                run.clock = crate::flow::clock::testing::FakePumpIo::new(0);
            }
            let failure = run
                .context
                .prepare_member_capture(42.into(), window, &mut run.clock)
                .unwrap_err();
            assert!(matches!(failure, RunFailure::Flow { .. }));
            if let Some(first) = first {
                assert_eq!(failure, first);
            }
            assert_eq!(run.context.failure(), Some(&failure));
            originals.assert_held(&run.context);
            originals.take(&mut run.context);
            assert_eq!(
                run.context
                    .prepare_member_capture(42.into(), window, &mut run.clock),
                Err(failure)
            );
            assert_eq!(run.context.phase(), RecordingPhase::Failed);
            assert!(run.context.member_capture.is_none());
        }
    }
}

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn active() -> Fixture {
    let mut run = Fixture::new(config());
    run.opened();
    let mut authored = run.load_event();
    let Some(Payload::Elections(load)) = &mut authored.payload else {
        unreachable!();
    };
    load.state_flow.persistence.save_wait.elapsed_ns = SAVE_NS.into();
    load.state_flow.persistence.snapshot_wait.elapsed_ns = SNAPSHOT_NS.into();
    run.author(authored).unwrap();
    run.context.activate(&mut run.clock).unwrap();
    for _ in 0..32 {
        run.context.reduce_flow(&mut run.clock).unwrap();
        if run.context.flow.operation_completion().is_some() {
            break;
        }
    }
    assert!(matches!(
        run.context
            .take_operation_completion(&mut run.clock)
            .unwrap(),
        Some(OperationCompletion::Opened(_))
    ));
    run.context.complete_wait(PumpDeadline::Enter).unwrap();
    run
}

struct Member<'a> {
    run: &'a mut Fixture,
    next_operation: u64,
}

impl<'a> Member<'a> {
    fn new(run: &'a mut Fixture) -> Self {
        Self {
            run,
            next_operation: 2,
        }
    }

    fn now(&mut self) -> u64 {
        self.run.clock.now().unwrap()
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

    fn requested(&mut self, id: u64, duration: u64) -> s::SaveWindow {
        let now = self.now();
        let window = s::SaveWindow {
            began_ns: now.into(),
            deadline_ns: (now + duration).into(),
        };
        self.run
            .author(event(
                Kind::SavePoint,
                Some(Payload::SavePointRecord(Box::new(
                    p::SavePointRecord::Requested {
                        capture_id: id.into(),
                        window: p::SaveWindow {
                            began_ns: now.into(),
                            deadline_ns: (now + duration).into(),
                        },
                        tail_window: None,
                        cause: p::PersistenceCause::Requested,
                    },
                ))),
            ))
            .unwrap();
        window
    }

    fn prepare(&mut self, id: u64, duration: u64) -> s::SaveWindow {
        let window = self.requested(id, duration);
        // The parent lifecycle has carried Beginning before this member seam.
        self.run
            .context
            .prepare_member_capture(id.into(), window, &mut self.run.clock)
            .unwrap();
        window
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

    fn advance(&mut self) -> Result<CaptureProgress, RunFailure> {
        self.run.context.advance_member_capture(&mut self.run.clock)
    }

    fn until(&mut self, expected: CaptureProgress) {
        for _ in 0..256 {
            if self.advance().unwrap() == expected {
                return;
            }
        }
        panic!("capture did not reach {expected:?}");
    }

    fn snapshot(&mut self, id: u64, duration: u64) -> s::StateFrame {
        self.prepare(id, duration);
        for _ in 0..256 {
            self.advance().unwrap();
            if !self.run.context.flow.state().outstanding().is_empty() {
                break;
            }
        }
        let request = self.sent();
        assert!(matches!(&request, s::StateFrame::Control {
            frame: s::StateControl::Snapshot { capture_id }, ..
        } if *capture_id == id.into()));
        request
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
        let mut line = String::new();
        BufReader::new(&mut self.run.state)
            .read_line(&mut line)
            .unwrap();
        s::decode_state_line(line.as_bytes(), s::FrameDirection::Request).unwrap()
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

    fn receive(&mut self, response: &s::StateFrame) -> Result<(), RunFailure> {
        self.run
            .state
            .write_all(&s::encode_state_line(response, s::FrameDirection::Response).unwrap())
            .unwrap();
        let now = self.now() + 3;
        self.pump_at(now, PollFlags::POLLIN)
    }

    fn image(&mut self, request: &s::StateFrame, id: u64) -> s::StateFrame {
        self.answer(
            request,
            s::StateControlAnswer::Snapshot {
                capture_id: id.into(),
                room_name: format!("{id}.part"),
                size_bytes: 10.into(),
                position: None,
            },
        )
    }

    fn result(&mut self, expected: CaptureProgress, answer: &s::StateFrame) -> CaptureAnswer {
        self.receive(answer).unwrap();
        let measurement = self
            .run
            .context
            .flow
            .state()
            .held_measurement()
            .unwrap()
            .clone();
        assert!(
            self.run
                .context
                .take_capture_answer(&mut self.run.clock)
                .unwrap()
                .is_none()
        );
        self.until(expected);
        let result = self
            .run
            .context
            .take_capture_answer(&mut self.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(
            s::encode_state_line(&result.answer, s::FrameDirection::Response).unwrap(),
            s::encode_state_line(answer, s::FrameDirection::Response).unwrap()
        );
        let expected = serde_json::to_value(measurement).unwrap();
        assert!(
            self.records(Kind::StateStoreMeasurement)
                .iter()
                .any(|record| record["payload"] == expected)
        );
        assert!(
            self.run
                .context
                .take_capture_answer(&mut self.run.clock)
                .unwrap()
                .is_none()
        );
        result
    }

    fn expire_leg(&mut self) -> (s::StateFrame, AbsoluteBound) {
        let phase = self.run.context.capture_snapshot().unwrap().phase;
        let bound = self
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureResult)
            .unwrap();
        self.run.clock.advance_to(bound.deadline_ns()).unwrap();
        self.advance().unwrap();
        let timeout = self.run.context.capture_timeout().unwrap();
        assert_eq!(timeout.phase, phase);
        assert_eq!(timeout.bound, bound);
        assert!(
            self.run.context.halt().is_none(),
            "missing original result is recoverable"
        );
        let cleanup = self
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        assert_eq!(cleanup.began_ns(), bound.deadline_ns());
        assert_eq!(cleanup.deadline_ns() - cleanup.began_ns(), SMALL_CONTROL_NS);
        assert_eq!(
            self.run.context.capture_snapshot().unwrap().phase,
            CapturePhase::Aborting
        );
        (self.sent(), cleanup)
    }

    fn ordinary_is_blocked(&mut self) {
        let request = ControlOperation::try_from((s::StateControl::Held {}, None)).unwrap();
        let result = self
            .run
            .context
            .flow
            .begin_control(
                &mut FlowRecording {
                    barrier: &mut self.run.context.barrier,
                    inbox: &mut self.run.context.inbox,
                    budget: &mut self.run.context.budget,
                    author: &self.run.context.author,
                },
                &mut self.run.clock,
                request,
            )
            .unwrap();
        assert!(matches!(result, OperationStart::Busy(_)));
        self.no_request();
    }

    fn no_request(&mut self) {
        self.run.state.set_nonblocking(true).unwrap();
        let mut bytes = [0u8; 1];
        let result = self.run.state.read(&mut bytes);
        self.run.state.set_nonblocking(false).unwrap();
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    }

    fn discard_late(&mut self, answer: &s::StateFrame, cleanup: AbsoluteBound) {
        let before = self.records(Kind::StateStoreMeasurement).len();
        self.receive(answer).unwrap();
        for _ in 0..256 {
            self.advance().unwrap();
            if self.records(Kind::StateStoreMeasurement).len() > before {
                break;
            }
        }
        assert_eq!(self.records(Kind::StateStoreMeasurement).len(), before + 1);
        assert!(
            self.run
                .context
                .take_capture_answer(&mut self.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            self.run.context.deadlines.get(PumpDeadline::StateControl),
            Some(cleanup)
        );
        assert_eq!(
            self.run.context.capture_snapshot().unwrap().phase,
            CapturePhase::Aborting
        );
        self.ordinary_is_blocked();
    }

    fn aborted(&mut self, request: &s::StateFrame, id: u64) {
        let answer = self.answer(
            request,
            s::StateControlAnswer::Aborted {
                capture_id: id.into(),
            },
        );
        self.result(CaptureProgress::Complete, &answer);
        self.run
            .context
            .finish_member_capture(id.into(), &mut self.run.clock)
            .unwrap();
        assert!(self.run.context.capture_snapshot().is_none());
        assert!(
            self.run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
        assert!(self.run.context.halt().is_none());
    }

    fn finished_capture(&mut self, id: u64) -> s::StateFrame {
        let snapshot = self.snapshot(id, SAVE_NS);
        let image = self.image(&snapshot, id);
        self.result(CaptureProgress::ImageReady, &image);
        self.run
            .context
            .acknowledge_member_capture(
                id.into(),
                s::Digest::new("9".repeat(64)).unwrap(),
                &mut self.run.clock,
            )
            .unwrap();
        let acknowledge = self.sent();
        let answer = self.answer(
            &acknowledge,
            s::StateControlAnswer::Finished {
                capture_id: id.into(),
                room_name: format!("{id}.sqlite"),
            },
        );
        self.result(CaptureProgress::FinishedReady, &answer);
        snapshot
    }

    fn fill_trace(&mut self) {
        let payload =
            serde_json::value::to_raw_value(&serde_json::json!({ "fixture": "x".repeat(32_768) }))
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
                        payload: Some(Payload::ModelMeasurement(payload.clone()))
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
        panic!("trace did not reach actual ordinary queue capacity");
    }

    fn drain_trace(&mut self) {
        self.run._reader.set_nonblocking(true).unwrap();
        let watchdog = Instant::now() + Duration::from_secs(3);
        let mut bytes = [0u8; 65536];
        while self.run.context.barrier.recorder().boundary().queued != 0 {
            assert!(Instant::now() < watchdog, "trace did not drain");
            match self.run._reader.read(&mut bytes) {
                Ok(0) => panic!("trace peer closed"),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::yield_now()
                }
                Err(error) => panic!("trace drain: {error}"),
            }
        }
    }
}

#[test]
fn successful_capture_records_each_leg_and_needs_explicit_release() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(7, SAVE_NS);
    let bound = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureResult)
        .unwrap();
    assert_eq!(bound.deadline_ns() - bound.began_ns(), SNAPSHOT_NS);
    let answer = member.image(&snapshot, 7);
    member.result(CaptureProgress::ImageReady, &answer);
    member.ordinary_is_blocked();
    member
        .run
        .context
        .acknowledge_member_capture(
            7.into(),
            s::Digest::new("3".repeat(64)).unwrap(),
            &mut member.run.clock,
        )
        .unwrap();
    let acknowledge = member.sent();
    assert_eq!(acknowledge.common(), snapshot.common());
    let bound = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureResult)
        .unwrap();
    assert_eq!(bound.deadline_ns() - bound.began_ns(), SMALL_CONTROL_NS);
    let answer = member.answer(
        &acknowledge,
        s::StateControlAnswer::Finished {
            capture_id: 7.into(),
            room_name: "7.sqlite".into(),
        },
    );
    member.result(CaptureProgress::FinishedReady, &answer);
    member.ordinary_is_blocked();
    assert_eq!(
        member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Finished
    );
    member
        .run
        .context
        .release_member_capture(7.into(), &mut member.run.clock)
        .unwrap();
    let release = member.sent();
    assert_eq!(
        release.common().request_id.0,
        snapshot.common().request_id.0 + 1
    );
    let answer = member.answer(
        &release,
        s::StateControlAnswer::CaptureReleased {
            capture_id: 7.into(),
        },
    );
    member.result(CaptureProgress::Complete, &answer);
    member
        .run
        .context
        .finish_member_capture(7.into(), &mut member.run.clock)
        .unwrap();
    assert!(member.run.context.capture_snapshot().is_none());
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), 4);
}

#[test]
fn both_original_timeouts_discard_recorded_late_results_before_abort_releases() {
    for finished in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let snapshot = member.snapshot(8, SAVE_NS);
        let pending = if finished {
            let answer = member.image(&snapshot, 8);
            member.result(CaptureProgress::ImageReady, &answer);
            member
                .run
                .context
                .acknowledge_member_capture(
                    8.into(),
                    s::Digest::new("4".repeat(64)).unwrap(),
                    &mut member.run.clock,
                )
                .unwrap();
            member.sent()
        } else {
            snapshot.clone()
        };
        let (abort, cleanup) = member.expire_leg();
        assert_eq!(abort.common(), pending.common());
        assert!(
            matches!(&abort, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if *capture_id == 8.into())
        );
        member.ordinary_is_blocked();
        let late = if finished {
            member.answer(
                &pending,
                s::StateControlAnswer::Finished {
                    capture_id: 8.into(),
                    room_name: "8.sqlite".into(),
                },
            )
        } else {
            member.image(&pending, 8)
        };
        member.discard_late(&late, cleanup);
        member.aborted(&abort, 8);
        let retry = member.snapshot(9, SAVE_NS);
        assert_eq!(
            retry.common().request_id.0,
            snapshot.common().request_id.0 + 1
        );
        let image = member.image(&retry, 9);
        member.result(CaptureProgress::ImageReady, &image);
    }
}

#[test]
fn late_explicit_capture_refusal_is_measured_without_replacing_the_missing_result() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(10, SAVE_NS);
    let (abort, cleanup) = member.expire_leg();
    let late = member.answer(
        &snapshot,
        s::StateControlAnswer::ControlRefused {
            code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::SnapshotFailed),
            detail: "late fixture refusal".into(),
        },
    );
    member.discard_late(&late, cleanup);
    assert!(member.run.context.capture_refusal().is_none());
    member.aborted(&abort, 10);
}

#[test]
fn timely_explicit_refusal_remains_distinct_and_hidden_until_abort_is_recorded() {
    for finished in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let snapshot = member.snapshot(30, SAVE_NS);
        let pending = if finished {
            let image = member.image(&snapshot, 30);
            member.result(CaptureProgress::ImageReady, &image);
            member
                .run
                .context
                .acknowledge_member_capture(
                    30.into(),
                    s::Digest::new("6".repeat(64)).unwrap(),
                    &mut member.run.clock,
                )
                .unwrap();
            member.sent()
        } else {
            snapshot
        };
        let original = member.answer(
            &pending,
            s::StateControlAnswer::ControlRefused {
                code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::SnapshotFailed),
                detail: "timely explicit fixture refusal".into(),
            },
        );
        member.receive(&original).unwrap();
        let measurement = member
            .run
            .context
            .flow
            .state()
            .held_measurement()
            .unwrap()
            .clone();
        for _ in 0..256 {
            member.advance().unwrap();
            if member.run.context.capture_snapshot().unwrap().phase == CapturePhase::Aborting {
                break;
            }
        }
        assert_eq!(
            member.run.context.capture_snapshot().unwrap().phase,
            CapturePhase::Aborting
        );
        assert!(member.run.context.capture_timeout().is_none());
        assert!(member.run.context.capture_refusal().is_none());
        assert!(
            member
                .run
                .context
                .take_capture_answer(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            member
                .records(Kind::StateStoreMeasurement)
                .iter()
                .any(|record| record["payload"] == serde_json::to_value(&measurement).unwrap())
        );
        let abort = member.sent();
        assert_eq!(abort.common(), pending.common());
        member.ordinary_is_blocked();
        let answer = member.answer(
            &abort,
            s::StateControlAnswer::Aborted {
                capture_id: 30.into(),
            },
        );
        member.result(CaptureProgress::Complete, &answer);
        let refusal = member.run.context.capture_refusal().unwrap();
        assert_eq!(
            s::encode_state_line(&refusal.answer, s::FrameDirection::Response).unwrap(),
            s::encode_state_line(&original, s::FrameDirection::Response).unwrap()
        );
        assert!(member.run.context.capture_timeout().is_none());
        member
            .run
            .context
            .finish_member_capture(30.into(), &mut member.run.clock)
            .unwrap();
        assert!(member.run.context.halt().is_none());
    }
}

#[test]
fn a_timely_recorded_image_survives_delayed_caller_scheduling_past_its_leg_bound() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(31, SAVE_NS);
    let bound = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureResult)
        .unwrap();
    let image = member.image(&snapshot, 31);
    member.receive(&image).unwrap();
    member.until(CaptureProgress::ImageReady);
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureResult)
            .is_none()
    );
    member
        .run
        .clock
        .advance_to(bound.deadline_ns() + 1)
        .unwrap();
    let answer = member
        .run
        .context
        .take_capture_answer(&mut member.run.clock)
        .unwrap()
        .unwrap();
    assert!(answer.recorded_ns < bound.deadline_ns());
    assert!(matches!(
        answer.answer,
        s::StateFrame::ControlAnswer {
            frame: s::StateControlAnswer::Snapshot { .. },
            ..
        }
    ));
    assert!(member.run.context.capture_timeout().is_none());
    assert!(member.run.context.halt().is_none());
    member.no_request();
    member
        .run
        .context
        .acknowledge_member_capture(
            31.into(),
            s::Digest::new("7".repeat(64)).unwrap(),
            &mut member.run.clock,
        )
        .unwrap();
    let acknowledge = member.sent();
    assert_eq!(acknowledge.common(), snapshot.common());
}

#[test]
fn recorded_capture_completion_waits_behind_a_later_original_author_without_losing_time() {
    for late in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let snapshot = member.snapshot(33, SAVE_NS);
        let bound = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureResult)
            .unwrap();
        let image = member.image(&snapshot, 33);
        member.receive(&image).unwrap();
        if late {
            member.run.clock.advance_to(bound.deadline_ns()).unwrap();
        }
        // The coordinator produces a real Recorded result before the context
        // gets a scheduling turn. No private proof or completion is fabricated.
        for _ in 0..256 {
            member
                .run
                .context
                .flow
                .advance(
                    &mut FlowRecording {
                        barrier: &mut member.run.context.barrier,
                        inbox: &mut member.run.context.inbox,
                        budget: &mut member.run.context.budget,
                        author: &member.run.context.author,
                    },
                    &mut member.run.clock,
                    1,
                )
                .unwrap();
            if member.run.context.flow.operation_completion().is_some() {
                break;
            }
        }
        let Some(OperationCompletion::Capture { recorded_ns, .. }) =
            member.run.context.flow.operation_completion()
        else {
            panic!("actual capture measurement did not produce its completion");
        };
        assert_eq!(*recorded_ns >= bound.deadline_ns(), late);
        member.fill_trace();
        let now = member.now();
        let mut authored = event(
            Kind::ModelMeasurement,
            Some(Payload::ModelMeasurement(
                serde_json::value::to_raw_value(
                    &serde_json::json!({ "held": "later original author" }),
                )
                .unwrap(),
            )),
        );
        authored.turn = Some(TurnRef("pressure".into()));
        assert!(matches!(
            member.run.context.barrier.submit_authored_with_clock(
                authored,
                RecordPurpose::Observation,
                &member.run.context.author,
                1,
                &mut member.run.context.inbox,
                &mut member.run.context.budget,
                &mut || Ok(now),
            ),
            RecordProgress::Pending
        ));
        let ticket = member.run.context.pending_author().unwrap().ticket;
        member
            .run
            .clock
            .advance_to(bound.deadline_ns() + 1)
            .unwrap();
        assert!(
            member
                .run
                .context
                .take_capture_answer(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
        assert!(member.run.context.flow.operation_completion().is_some());
        assert!(member.run.context.halt().is_none());
        let abort = if late {
            assert_eq!(
                member.run.context.capture_snapshot().unwrap().phase,
                CapturePhase::Aborting
            );
            assert_eq!(member.run.context.capture_timeout().unwrap().bound, bound);
            let abort = member.sent();
            assert_eq!(abort.common(), snapshot.common());
            Some(abort)
        } else {
            assert!(
                member
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::CaptureResult)
                    .is_none()
            );
            assert!(member.run.context.capture_timeout().is_none());
            member.no_request();
            None
        };
        member.drain_trace();
        let now = member.now();
        assert!(matches!(
            member.run.context.barrier.resume_with_clock(
                &mut member.run.context.inbox,
                &mut member.run.context.budget,
                &mut || Ok(now),
            ),
            RecordProgress::Recorded { proof: None, .. }
        ));
        assert!(member.run.context.pending_author().is_none());
        let answer = member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap();
        if let Some(abort) = abort {
            assert!(
                answer.is_none(),
                "late original cannot escape as a save result"
            );
            member.aborted(&abort, 33);
        } else {
            let answer = answer.expect("timely recorded answer survives later author");
            assert_eq!(
                s::encode_state_line(&answer.answer, s::FrameDirection::Response).unwrap(),
                s::encode_state_line(&image, s::FrameDirection::Response).unwrap()
            );
            assert!(answer.recorded_ns < bound.deadline_ns());
        }
    }
}

#[test]
fn outer_window_expiry_does_not_relabel_a_timely_recorded_held_result_as_missing() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(35, SAVE_NS);
    let leg = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureResult)
        .unwrap();
    let window = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow)
        .unwrap();
    let image = member.image(&snapshot, 35);
    member.receive(&image).unwrap();
    for _ in 0..256 {
        member
            .run
            .context
            .flow
            .advance(
                &mut FlowRecording {
                    barrier: &mut member.run.context.barrier,
                    inbox: &mut member.run.context.inbox,
                    budget: &mut member.run.context.budget,
                    author: &member.run.context.author,
                },
                &mut member.run.clock,
                1,
            )
            .unwrap();
        if member.run.context.flow.operation_completion().is_some() {
            break;
        }
    }
    let Some(OperationCompletion::Capture { recorded_ns, .. }) =
        member.run.context.flow.operation_completion()
    else {
        panic!("actual capture measurement did not produce a completion");
    };
    assert!(*recorded_ns < leg.deadline_ns());
    member.fill_trace();
    let now = member.now();
    let mut authored = event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({ "held": "through outer expiry" }))
                .unwrap(),
        )),
    );
    authored.turn = Some(TurnRef("pressure".into()));
    assert!(matches!(
        member.run.context.barrier.submit_authored_with_clock(
            authored,
            RecordPurpose::Observation,
            &member.run.context.author,
            1,
            &mut member.run.context.inbox,
            &mut member.run.context.budget,
            &mut || Ok(now),
        ),
        RecordProgress::Pending
    ));
    let ticket = member.run.context.pending_author().unwrap().ticket;
    member.run.clock.advance_to(window.deadline_ns()).unwrap();
    assert_eq!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::CaptureWindow,
            bound: window
        }
    );
    assert!(member.run.context.capture_timeout().is_none());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureResult)
            .is_none()
    );
    assert_eq!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow),
        Some(window)
    );
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
    let Some(OperationCompletion::Capture { completion, .. }) =
        member.run.context.flow.operation_completion()
    else {
        panic!("outer expiry lost the recorded original result");
    };
    assert!(!completion.discard_capture_result);
    assert_eq!(
        s::encode_state_line(&completion.answer, s::FrameDirection::Response).unwrap(),
        s::encode_state_line(&image, s::FrameDirection::Response).unwrap()
    );
    assert!(member.run.context.halt().is_none());
    member.no_request();

    // The parent may now cancel its expired save. This explicit cancellation
    // retains the distinct cause and still owes a measured Abort result.
    member
        .run
        .context
        .cancel_member_capture(35.into(), &mut member.run.clock)
        .unwrap();
    let abort = member.sent();
    assert_eq!(abort.common(), snapshot.common());
    assert!(member.run.context.capture_timeout().is_none());
    member.drain_trace();
    let now = member.now();
    assert!(matches!(
        member.run.context.barrier.resume_with_clock(
            &mut member.run.context.inbox,
            &mut member.run.context.budget,
            &mut || Ok(now),
        ),
        RecordProgress::Recorded { proof: None, .. }
    ));
    assert!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    member.aborted(&abort, 35);
}

#[test]
fn a_cached_capture_answer_cannot_pass_a_later_pending_author() {
    for finished in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let snapshot = member.snapshot(34, SAVE_NS);
        let (answer, progress) = if finished {
            let image = member.image(&snapshot, 34);
            member.result(CaptureProgress::ImageReady, &image);
            member
                .run
                .context
                .acknowledge_member_capture(
                    34.into(),
                    s::Digest::new("8".repeat(64)).unwrap(),
                    &mut member.run.clock,
                )
                .unwrap();
            let acknowledge = member.sent();
            (
                member.answer(
                    &acknowledge,
                    s::StateControlAnswer::Finished {
                        capture_id: 34.into(),
                        room_name: "34.sqlite".into(),
                    },
                ),
                CaptureProgress::FinishedReady,
            )
        } else {
            (member.image(&snapshot, 34), CaptureProgress::ImageReady)
        };
        member.receive(&answer).unwrap();
        member.until(progress);
        assert!(member.run.context.flow.operation_completion().is_none());
        assert!(member.run.context.flow.state().held_measurement().is_none());
        member.fill_trace();
        let now = member.now();
        let mut authored = event(
            Kind::ModelMeasurement,
            Some(Payload::ModelMeasurement(
                serde_json::value::to_raw_value(
                    &serde_json::json!({ "held": "after cached result" }),
                )
                .unwrap(),
            )),
        );
        authored.turn = Some(TurnRef("pressure".into()));
        assert!(matches!(
            member.run.context.barrier.submit_authored_with_clock(
                authored,
                RecordPurpose::Observation,
                &member.run.context.author,
                1,
                &mut member.run.context.inbox,
                &mut member.run.context.budget,
                &mut || Ok(now),
            ),
            RecordProgress::Pending
        ));
        let ticket = member.run.context.pending_author().unwrap().ticket;
        assert!(
            member
                .run
                .context
                .take_capture_answer(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
        assert!(member.run.context.halt().is_none());
        member.drain_trace();
        let now = member.now();
        assert!(matches!(
            member.run.context.barrier.resume_with_clock(
                &mut member.run.context.inbox,
                &mut member.run.context.budget,
                &mut || Ok(now),
            ),
            RecordProgress::Recorded { proof: None, .. }
        ));
        let actual = member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(
            s::encode_state_line(&actual.answer, s::FrameDirection::Response).unwrap(),
            s::encode_state_line(&answer, s::FrameDirection::Response).unwrap()
        );
        assert!(
            member
                .run
                .context
                .take_capture_answer(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn cancelling_after_timely_finished_uses_a_fresh_release_even_after_outer_expiry() {
    for expired in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let snapshot = member.finished_capture(36);
        let window = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .unwrap();
        if expired {
            member.run.clock.advance_to(window.deadline_ns()).unwrap();
        }
        let began = member.now();
        assert_eq!(
            member
                .run
                .context
                .cancel_member_capture(36.into(), &mut member.run.clock)
                .unwrap(),
            Advance::Progress
        );
        assert!(member.run.context.capture_timeout().is_none());
        let cleanup = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        assert_eq!(cleanup.began_ns(), began);
        assert_eq!(cleanup.deadline_ns() - cleanup.began_ns(), SMALL_CONTROL_NS);
        assert_eq!(
            member
                .run
                .context
                .cancel_member_capture(36.into(), &mut member.run.clock)
                .unwrap(),
            Advance::Pending,
        );
        assert_eq!(
            member.run.context.deadlines.get(PumpDeadline::StateControl),
            Some(cleanup)
        );
        assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
        let release = member.sent();
        assert!(matches!(&release, s::StateFrame::Control {
            frame: s::StateControl::ReleaseCapture { capture_id }, ..
        } if *capture_id == 36.into()));
        assert_eq!(release.common().session_id, snapshot.common().session_id);
        assert_eq!(release.common().run_id, snapshot.common().run_id);
        assert_eq!(
            release.common().request_id.0,
            snapshot.common().request_id.0 + 1
        );
        member.ordinary_is_blocked();
        let answer = member.answer(
            &release,
            s::StateControlAnswer::CaptureReleased {
                capture_id: 36.into(),
            },
        );
        member.result(CaptureProgress::Complete, &answer);
        assert_eq!(
            member
                .run
                .context
                .cancel_member_capture(36.into(), &mut member.run.clock)
                .unwrap(),
            Advance::Progress,
        );
        assert!(member.run.context.capture_snapshot().is_some());
        member
            .run
            .context
            .finish_member_capture(36.into(), &mut member.run.clock)
            .unwrap();
        assert!(member.run.context.capture_snapshot().is_none());
        assert!(member.run.context.halt().is_none());
        assert_eq!(member.records(Kind::StateStoreMeasurement).len(), 4);
    }
}

#[test]
fn refused_release_after_cancel_is_measured_then_halts_at_state_reply() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.finished_capture(37);
    assert_eq!(
        member
            .run
            .context
            .cancel_member_capture(37.into(), &mut member.run.clock)
            .unwrap(),
        Advance::Progress
    );
    let release = member.sent();
    assert_eq!(
        release.common().request_id.0,
        snapshot.common().request_id.0 + 1
    );
    let answer = member.answer(
        &release,
        s::StateControlAnswer::ControlRefused {
            code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::UnknownCapture),
            detail: "release cleanup refused".into(),
        },
    );
    member.receive(&answer).unwrap();
    let measurement = member
        .run
        .context
        .flow
        .state()
        .held_measurement()
        .unwrap()
        .clone();
    let mut failure = None;
    for _ in 0..256 {
        if let Err(error) = member.advance() {
            failure = Some(error);
            break;
        }
    }
    let failure = failure.expect("refused cleanup must halt");
    assert!(matches!(
        &failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            point: HaltPoint::StateReply,
            ..
        }
    ));
    assert!(
        member
            .records(Kind::StateStoreMeasurement)
            .iter()
            .any(|record| record["payload"] == serde_json::to_value(&measurement).unwrap())
    );
    assert_eq!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap_err(),
        failure
    );
    assert!(member.run.context.capture_snapshot().is_some());
    member.no_request();
}

#[test]
fn cancelling_a_timely_finished_proof_queues_release_and_public_author_resume_progresses() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(38, SAVE_NS);
    let window = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow)
        .unwrap();
    let image = member.image(&snapshot, 38);
    member.result(CaptureProgress::ImageReady, &image);
    member
        .run
        .context
        .acknowledge_member_capture(
            38.into(),
            s::Digest::new("a".repeat(64)).unwrap(),
            &mut member.run.clock,
        )
        .unwrap();
    let acknowledge = member.sent();
    let answer = member.answer(
        &acknowledge,
        s::StateControlAnswer::Finished {
            capture_id: 38.into(),
            room_name: "38.sqlite".into(),
        },
    );
    member.receive(&answer).unwrap();
    let bound = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureResult)
        .unwrap();
    for _ in 0..256 {
        member
            .run
            .context
            .flow
            .advance(
                &mut FlowRecording {
                    barrier: &mut member.run.context.barrier,
                    inbox: &mut member.run.context.inbox,
                    budget: &mut member.run.context.budget,
                    author: &member.run.context.author,
                },
                &mut member.run.clock,
                1,
            )
            .unwrap();
        if member.run.context.flow.operation_completion().is_some() {
            break;
        }
    }
    let Some(OperationCompletion::Capture { recorded_ns, .. }) =
        member.run.context.flow.operation_completion()
    else {
        panic!("Finished did not cross actual recording barrier");
    };
    assert!(*recorded_ns < bound.deadline_ns());
    member.fill_trace();
    let now = member.now();
    let mut authored = event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(
                &serde_json::json!({ "held": "finished cancellation" }),
            )
            .unwrap(),
        )),
    );
    authored.turn = Some(TurnRef("pressure".into()));
    assert!(matches!(
        member.run.context.barrier.submit_authored_with_clock(
            authored,
            RecordPurpose::Observation,
            &member.run.context.author,
            1,
            &mut member.run.context.inbox,
            &mut member.run.context.budget,
            &mut || Ok(now),
        ),
        RecordProgress::Pending
    ));
    let ticket = member.run.context.pending_author().unwrap().ticket;
    member.run.clock.advance_to(window.deadline_ns()).unwrap();
    assert_eq!(
        member
            .run
            .context
            .cancel_member_capture(38.into(), &mut member.run.clock)
            .unwrap(),
        Advance::Progress
    );
    assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow),
        None
    );
    let cleanup = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert_eq!(cleanup.began_ns(), window.deadline_ns());
    assert_eq!(cleanup.deadline_ns() - cleanup.began_ns(), SMALL_CONTROL_NS);
    assert!(member.run.context.flow.state().outstanding().is_empty());
    assert!(member.run.context.flow.operation_completion().is_some());
    assert!(member.run.context.capture_timeout().is_none());
    assert!(member.run.context.halt().is_none());
    member.no_request();
    member.drain_trace();
    member
        .run
        .context
        .resume_author_and_wait(&mut ports(&mut member.run.clock))
        .unwrap();
    assert!(member.run.context.pending_author().is_none());
    member
        .run
        .clock
        .advance_to(cleanup.began_ns() + 50)
        .unwrap();
    for _ in 0..256 {
        member.advance().unwrap();
        if !member.run.context.flow.state().outstanding().is_empty() {
            break;
        }
    }
    assert_eq!(
        member.run.context.deadlines.get(PumpDeadline::StateControl),
        Some(cleanup)
    );
    let release = member.sent();
    assert!(
        matches!(&release, s::StateFrame::Control { frame: s::StateControl::ReleaseCapture { capture_id }, .. } if *capture_id == 38.into())
    );
    assert_eq!(
        release.common().request_id.0,
        snapshot.common().request_id.0 + 1
    );
    let answer = member.answer(
        &release,
        s::StateControlAnswer::CaptureReleased {
            capture_id: 38.into(),
        },
    );
    member.result(CaptureProgress::Complete, &answer);
    let measurements = member.records(Kind::StateStoreMeasurement);
    assert_eq!(
        measurements.last().unwrap()["payload"]["harness_queued_ns"],
        cleanup.began_ns().to_string()
    );
    member
        .run
        .context
        .finish_member_capture(38.into(), &mut member.run.clock)
        .unwrap();
}

#[test]
fn a_recorded_acknowledge_refusal_behind_an_author_cancels_with_abort_not_release() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(39, SAVE_NS);
    let window = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow)
        .unwrap();
    let image = member.image(&snapshot, 39);
    member.result(CaptureProgress::ImageReady, &image);
    member
        .run
        .context
        .acknowledge_member_capture(
            39.into(),
            s::Digest::new("b".repeat(64)).unwrap(),
            &mut member.run.clock,
        )
        .unwrap();
    let acknowledge = member.sent();
    let answer = member.answer(
        &acknowledge,
        s::StateControlAnswer::ControlRefused {
            code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::SnapshotFailed),
            detail: "recorded refusal is not a Finished proof".into(),
        },
    );
    member.receive(&answer).unwrap();
    let bound = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureResult)
        .unwrap();
    for _ in 0..256 {
        member
            .run
            .context
            .flow
            .advance(
                &mut FlowRecording {
                    barrier: &mut member.run.context.barrier,
                    inbox: &mut member.run.context.inbox,
                    budget: &mut member.run.context.budget,
                    author: &member.run.context.author,
                },
                &mut member.run.clock,
                1,
            )
            .unwrap();
        if member.run.context.flow.operation_completion().is_some() {
            break;
        }
    }
    let Some(OperationCompletion::Capture { recorded_ns, .. }) =
        member.run.context.flow.operation_completion()
    else {
        panic!("refusal did not cross actual recording barrier");
    };
    assert!(*recorded_ns < bound.deadline_ns());
    member.fill_trace();
    let now = member.now();
    let mut authored = event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(
                &serde_json::json!({ "held": "refused finish cancellation" }),
            )
            .unwrap(),
        )),
    );
    authored.turn = Some(TurnRef("pressure".into()));
    assert!(matches!(
        member.run.context.barrier.submit_authored_with_clock(
            authored,
            RecordPurpose::Observation,
            &member.run.context.author,
            1,
            &mut member.run.context.inbox,
            &mut member.run.context.budget,
            &mut || Ok(now),
        ),
        RecordProgress::Pending
    ));
    member.run.clock.advance_to(window.deadline_ns()).unwrap();
    assert_eq!(
        member
            .run
            .context
            .cancel_member_capture(39.into(), &mut member.run.clock)
            .unwrap(),
        Advance::Progress
    );
    assert!(member.run.context.capture_timeout().is_none());
    let abort = member.sent();
    assert!(
        matches!(&abort, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if *capture_id == 39.into())
    );
    assert_eq!(abort.common(), snapshot.common());
    member.drain_trace();
    member
        .run
        .context
        .resume_author_and_wait(&mut ports(&mut member.run.clock))
        .unwrap();
    assert!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    member.aborted(&abort, 39);
}

#[test]
fn capture_requires_a_closed_turn_and_preserves_an_existing_pending_author() {
    for pending in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let window = member.requested(32, SAVE_NS);
        let ticket = if pending {
            member.fill_trace();
            let now = member.now();
            let mut authored = event(
                Kind::ModelMeasurement,
                Some(Payload::ModelMeasurement(
                    serde_json::value::to_raw_value(
                        &serde_json::json!({ "held": "original capture guard" }),
                    )
                    .unwrap(),
                )),
            );
            authored.turn = Some(TurnRef("pressure".into()));
            assert!(matches!(
                member.run.context.barrier.submit_authored_with_clock(
                    authored,
                    RecordPurpose::Observation,
                    &member.run.context.author,
                    1,
                    &mut member.run.context.inbox,
                    &mut member.run.context.budget,
                    &mut || Ok(now),
                ),
                RecordProgress::Pending
            ));
            Some(member.run.context.pending_author().unwrap().ticket)
        } else {
            member
                .run
                .author(AuthoredEvent {
                    kind: Kind::TurnStarted,
                    subsystem: Subsystem::Harness,
                    turn: Some(TurnRef("still-active".into())),
                    source: super::super::author::AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: None,
                })
                .unwrap();
            None
        };
        assert!(matches!(
            member
                .run
                .context
                .prepare_member_capture(32.into(), window, &mut member.run.clock),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Barrier,
                ..
            })
        ));
        assert!(member.run.context.capture_snapshot().is_none());
        if let Some(ticket) = ticket {
            let retained = member.run.context.pending_author().unwrap();
            assert_eq!(retained.ticket, ticket);
            let Some(Payload::ModelMeasurement(payload)) = &retained.event.payload else {
                panic!("original author changed");
            };
            assert!(payload.get().contains("original capture guard"));
        } else {
            assert_eq!(
                member.run.context.open_turn,
                Some(TurnRef("still-active".into()))
            );
        }
        member.no_request();
    }
}

#[test]
fn cleanup_bound_retains_its_anchor_through_late_answer_and_halts_without_retry() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(11, SAVE_NS);
    let (_, cleanup) = member.expire_leg();
    let late = member.image(&snapshot, 11);
    member.discard_late(&late, cleanup);
    member.run.clock.advance_to(cleanup.deadline_ns()).unwrap();
    assert!(matches!(
        member.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            point: HaltPoint::Barrier,
            ..
        })
    ));
    assert_eq!(
        member.run.context.halt_wait().unwrap().began_ns.0,
        cleanup.began_ns()
    );
    assert_eq!(
        member.run.context.halt_wait().unwrap().deadline_ns.0,
        cleanup.deadline_ns()
    );
    assert_eq!(
        member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Aborting
    );
    member.no_request();
}

#[test]
fn an_earlier_leave_bound_stops_cleanup_without_restarting_either_window() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(12, SAVE_NS);
    let (_, cleanup) = member.expire_leg();
    let began = member.now();
    let leave = AbsoluteBound::after(began, 100).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
        .unwrap();
    let late = member.image(&snapshot, 12);
    member.discard_late(&late, cleanup);
    member.run.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(
        member.advance().unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert_eq!(
        member.run.context.deadlines.get(PumpDeadline::StateControl),
        Some(cleanup)
    );
    assert_eq!(
        member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Aborting
    );
    member.no_request();
}

#[test]
fn cleanup_wrong_identity_refusal_malformed_bytes_and_eof_are_terminal() {
    for case in ["wrong_id", "refusal", "malformed", "eof"] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let snapshot = member.snapshot(13, SAVE_NS);
        let (abort, cleanup) = member.expire_leg();
        let late = member.image(&snapshot, 13);
        member.discard_late(&late, cleanup);
        let before = member.records(Kind::StateStoreMeasurement).len();
        let first = match case {
            "wrong_id" => {
                let answer = member.answer(
                    &abort,
                    s::StateControlAnswer::Aborted {
                        capture_id: 99.into(),
                    },
                );
                member.receive(&answer)
            }
            "refusal" => {
                let answer = member.answer(
                    &abort,
                    s::StateControlAnswer::ControlRefused {
                        code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::UnknownCapture),
                        detail: "cleanup refused".into(),
                    },
                );
                member.receive(&answer)
            }
            "malformed" => {
                member.run.state.write_all(b"{not-json}\n").unwrap();
                let now = member.now() + 1;
                member.pump_at(now, PollFlags::POLLIN)
            }
            "eof" => {
                member.run.state.shutdown(Shutdown::Write).unwrap();
                let now = member.now() + 1;
                member.pump_at(now, PollFlags::POLLIN)
            }
            _ => unreachable!(),
        };
        let failure = match first {
            Err(error) => error,
            Ok(()) => {
                let mut error = None;
                for _ in 0..256 {
                    if let Err(failure) = member.advance() {
                        error = Some(failure);
                        break;
                    }
                }
                error.expect("invalid cleanup must halt")
            }
        };
        let reason = if case == "eof" {
            FlowHaltReason::CarryFailed
        } else {
            FlowHaltReason::ProtocolFailed
        };
        assert!(
            matches!(failure, RunFailure::Flow { reason: actual, point: HaltPoint::StateReply, .. } if actual == reason),
            "{case}: {failure:?}"
        );
        if case == "refusal" {
            assert_eq!(
                member.records(Kind::StateStoreMeasurement).len(),
                before + 1,
                "valid cleanup refusal is recorded before halting"
            );
        }
        assert_eq!(
            member.run.context.capture_snapshot().unwrap().phase,
            CapturePhase::Aborting
        );
    }
}

#[test]
fn trace_pressure_holds_both_late_image_and_abort_until_each_exact_measurement_records() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(14, SAVE_NS);
    let (abort, cleanup) = member.expire_leg();
    for _ in 0..32 {
        member.advance().unwrap();
    }
    let before = member.records(Kind::StateStoreMeasurement).len();
    member.fill_trace();
    let late = member.image(&snapshot, 14);
    member.receive(&late).unwrap();
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
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), before);
    assert_eq!(
        member.run.context.flow.state().held_measurement(),
        Some(&held)
    );
    assert_eq!(
        member.run.context.pending_author().unwrap().event.kind,
        Kind::StateStoreMeasurement
    );
    assert!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        member.run.context.deadlines.get(PumpDeadline::StateControl),
        Some(cleanup)
    );
    member.ordinary_is_blocked();
    member.drain_trace();
    for _ in 0..256 {
        member.advance().unwrap();
        if member.run.context.flow.state().held_measurement().is_none() {
            break;
        }
    }
    assert!(member.run.context.flow.state().held_measurement().is_none());
    assert_eq!(
        member.records(Kind::StateStoreMeasurement).len(),
        before + 1
    );
    assert!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Aborting
    );

    member.fill_trace();
    let answer = member.answer(
        &abort,
        s::StateControlAnswer::Aborted {
            capture_id: 14.into(),
        },
    );
    member.receive(&answer).unwrap();
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
    assert_eq!(
        member.records(Kind::StateStoreMeasurement).len(),
        before + 1
    );
    assert_eq!(
        member.run.context.flow.state().held_measurement(),
        Some(&held)
    );
    assert!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Aborting
    );
    member.ordinary_is_blocked();
    member.drain_trace();
    member.until(CaptureProgress::Complete);
    let completed = member
        .run
        .context
        .take_capture_answer(&mut member.run.clock)
        .unwrap()
        .unwrap();
    assert!(matches!(
        completed.answer,
        s::StateFrame::ControlAnswer {
            frame: s::StateControlAnswer::Aborted { .. },
            ..
        }
    ));
    assert_eq!(
        member.records(Kind::StateStoreMeasurement).len(),
        before + 2
    );
    member
        .run
        .context
        .finish_member_capture(14.into(), &mut member.run.clock)
        .unwrap();
    assert!(member.run.context.capture_snapshot().is_none());
}

#[test]
fn an_original_answer_waiting_for_trace_at_its_bound_enters_correlated_cleanup() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(15, SAVE_NS);
    for _ in 0..32 {
        member.advance().unwrap();
    }
    let before = member.records(Kind::StateStoreMeasurement).len();
    member.fill_trace();
    let answer = member.image(&snapshot, 15);
    member.receive(&answer).unwrap();
    for _ in 0..32 {
        member.advance().unwrap();
    }
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), before);
    let held = member
        .run
        .context
        .flow
        .state()
        .held_measurement()
        .unwrap()
        .clone();
    let (abort, cleanup) = member.expire_leg();
    assert_eq!(abort.common(), snapshot.common());
    assert_eq!(
        member.run.context.flow.state().held_measurement(),
        Some(&held)
    );
    assert!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    member.ordinary_is_blocked();
    member.drain_trace();
    for _ in 0..256 {
        member.advance().unwrap();
        if member.run.context.flow.state().held_measurement().is_none() {
            break;
        }
    }
    assert_eq!(
        member.records(Kind::StateStoreMeasurement).len(),
        before + 1
    );
    assert_eq!(
        member.run.context.deadlines.get(PumpDeadline::StateControl),
        Some(cleanup)
    );
    assert!(
        member
            .run
            .context
            .take_capture_answer(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    member.aborted(&abort, 15);
}

#[test]
fn cleanup_unblocks_a_real_selected_filing_then_a_new_capture() {
    let mut input = super::input_tests::InputFixture::new();
    let next_operation;
    let snapshot_id;
    {
        let mut member = Member::new(&mut input.run);
        // This shared fixture elected one millisecond in its actual load record.
        let snapshot = member.snapshot(20, 1_000_000);
        snapshot_id = snapshot.common().request_id;
        let (abort, cleanup) = member.expire_leg();
        let late = member.image(&snapshot, 20);
        member.discard_late(&late, cleanup);
        member.aborted(&abort, 20);
        next_operation = member.next_operation;
    }
    input.begin(weaver_types::TurnFrame::carry_from(
        br#"{"text":"Read next."}"#,
        43,
    ));
    let (_, _, token) = input.accepted_token();
    let turn = TurnRef("after-capture".into());
    let seated = input
        .run
        .context
        .consume_input(token, turn.clone(), &mut ports(&mut input.run.clock))
        .unwrap()
        .unwrap();
    let mut member = Member {
        run: &mut input.run,
        next_operation,
    };
    for _ in 0..256 {
        member
            .run
            .context
            .reduce_flow(&mut member.run.clock)
            .unwrap();
        if !member.run.context.flow.state().outstanding().is_empty() {
            break;
        }
    }
    let filing = member.sent();
    let s::StateFrame::File { filing: body, .. } = &filing else {
        panic!("selected input must cross actual File seam");
    };
    assert_eq!(body.block_id, seated.source);
    assert_eq!(filing.common().request_id.0, snapshot_id.0 + 1);
    let observation = match member.answer(&filing, s::StateControlAnswer::Held { count: 0.into() })
    {
        s::StateFrame::ControlAnswer { observation, .. } => observation,
        _ => unreachable!(),
    };
    let answer = s::StateFrame::Filed {
        common: filing.common().clone(),
        block_id: seated.source.clone(),
        observation,
    };
    member.receive(&answer).unwrap();
    for _ in 0..256 {
        member
            .run
            .context
            .reduce_flow(&mut member.run.clock)
            .unwrap();
        if member
            .run
            .context
            .flow
            .ledger()
            .barrier(std::slice::from_ref(&seated.source))
            == Ok(true)
        {
            break;
        }
    }
    assert_eq!(
        member
            .run
            .context
            .flow
            .ledger()
            .barrier(std::slice::from_ref(&seated.source)),
        Ok(true)
    );
    member
        .run
        .author(AuthoredEvent {
            kind: Kind::TurnClosed,
            subsystem: Subsystem::Harness,
            turn: Some(turn),
            source: super::super::author::AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::TurnClosed(weaver_trace::TurnClose::Clean)),
        })
        .unwrap();
    let retry = member.snapshot(21, 1_000_000);
    assert_eq!(retry.common().request_id.0, snapshot_id.0 + 2);
    let image = member.image(&retry, 21);
    member.result(CaptureProgress::ImageReady, &image);
    assert!(member.run.context.halt().is_none());
}
