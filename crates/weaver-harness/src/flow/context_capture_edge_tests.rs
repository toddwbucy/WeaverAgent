//! Exact clock boundaries reuse the real capture context and member socket.

fn original_leg(member: &mut Member<'_>, id: u64, finished: bool) -> s::StateFrame {
    let snapshot = member.snapshot(id, SAVE_NS);
    if !finished {
        return snapshot;
    }
    let image = member.image(&snapshot, id);
    member.result(CaptureProgress::ImageReady, &image);
    member
        .run
        .context
        .acknowledge_member_capture(
            id.into(),
            s::Digest::new("6".repeat(64)).unwrap(),
            &mut member.run.clock,
        )
        .unwrap();
    member.sent()
}

fn hold_recorded_original(member: &mut Member<'_>) {
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
            return;
        }
    }
    panic!("original capture did not cross its real recording barrier");
}

fn original_answer(
    member: &mut Member<'_>,
    request: &s::StateFrame,
    id: u64,
    finished: bool,
) -> s::StateFrame {
    if finished {
        member.answer(
            request,
            s::StateControlAnswer::Finished {
                capture_id: id.into(),
                room_name: format!("{id}.sqlite"),
            },
        )
    } else {
        member.image(request, id)
    }
}

#[test]
fn original_timeout_keeps_its_phase_and_abort_obligation_across_leave() {
    for finished in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let request = original_leg(&mut member, 42, finished);
        let original_phase = member.run.context.capture_snapshot().unwrap().phase;
        let original_bound = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureResult)
            .unwrap();
        let now = member.now();
        let leave = AbsoluteBound::from_absolute(now, original_bound.deadline_ns() + 10).unwrap();
        member
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
            .unwrap();
        member
            .run
            .clock
            .advance_to(original_bound.deadline_ns())
            .unwrap();
        // Detect the original timeout, then cross Leave at cleanup admission.
        let mut clock = JumpAtSample {
            clock: &mut member.run.clock,
            samples: 0,
            at: 2,
            to: leave.deadline_ns(),
        };
        assert_eq!(
            member
                .run
                .context
                .advance_member_capture(&mut clock)
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave
            }
        );
        assert_eq!(
            member.run.context.capture_snapshot().unwrap().phase,
            original_phase
        );
        assert_eq!(
            member.run.context.capture_timeout(),
            Some(CaptureTimeout {
                phase: original_phase,
                bound: original_bound,
            })
        );
        assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
        assert_eq!(
            member
                .run
                .context
                .deadlines
                .get(PumpDeadline::CaptureResult),
            Some(original_bound)
        );
        for _ in 0..2 {
            assert_eq!(
                member.advance().unwrap_err(),
                RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound: leave
                }
            );
            assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
        }
        member
            .run
            .context
            .complete_wait(PumpDeadline::Leave)
            .unwrap();
        member.advance().unwrap();
        let cleanup = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        assert_eq!(cleanup.began_ns(), leave.deadline_ns());
        assert_eq!(cleanup.deadline_ns() - cleanup.began_ns(), SMALL_CONTROL_NS);
        for _ in 0..2 {
            member.advance().unwrap();
            assert_eq!(member.run.context.flow.state().outstanding().len(), 2);
            assert_eq!(
                member.run.context.deadlines.get(PumpDeadline::StateControl),
                Some(cleanup)
            );
        }
        let abort = member.sent();
        assert_eq!(abort.common(), request.common());
        let late = original_answer(&mut member, &request, 42, finished);
        member.discard_late(&late, cleanup);
        member.aborted(&abort, 42);
    }
}

#[test]
fn late_recorded_result_keeps_abort_obligation_across_leave_without_publication() {
    for finished in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let request = original_leg(&mut member, 43, finished);
        let original_phase = member.run.context.capture_snapshot().unwrap().phase;
        let bound = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureResult)
            .unwrap();
        let now = member.now();
        let leave = AbsoluteBound::from_absolute(now, bound.deadline_ns() + 10).unwrap();
        member
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
            .unwrap();
        let late = original_answer(&mut member, &request, 43, finished);
        member.receive(&late).unwrap();
        // The frame arrived in time, but its actual Recorded boundary is late.
        member.run.clock.advance_to(bound.deadline_ns()).unwrap();
        hold_recorded_original(&mut member);
        assert!(matches!(member.run.context.flow.operation_completion(),
            Some(OperationCompletion::Capture { recorded_ns, .. }) if *recorded_ns == bound.deadline_ns()));
        let recorded = member.records(Kind::StateStoreMeasurement).len();
        let mut clock = JumpAtSample {
            clock: &mut member.run.clock,
            samples: 0,
            at: 1,
            to: leave.deadline_ns(),
        };
        assert_eq!(
            member
                .run
                .context
                .settle_recorded_capture(&mut clock)
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave
            }
        );
        assert!(member.run.context.flow.operation_completion().is_none());
        assert_eq!(
            member.run.context.capture_timeout(),
            Some(CaptureTimeout {
                phase: original_phase,
                bound,
            })
        );
        assert!(member.run.context.flow.state().outstanding().is_empty());
        assert!(member.run.context.capture_refusal().is_none());
        assert_eq!(
            member.advance().unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave
            }
        );
        member
            .run
            .context
            .complete_wait(PumpDeadline::Leave)
            .unwrap();
        member.advance().unwrap();
        let cleanup = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        assert_eq!(cleanup.began_ns(), leave.deadline_ns());
        assert_eq!(cleanup.deadline_ns() - cleanup.began_ns(), SMALL_CONTROL_NS);
        assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
        assert_eq!(member.records(Kind::StateStoreMeasurement).len(), recorded);
        assert!(
            member
                .run
                .context
                .take_capture_answer(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        let abort = member.sent();
        assert_eq!(abort.common(), request.common());
        member.aborted(&abort, 43);
    }
}

#[test]
fn capture_cancellation_has_one_recoverable_admission_boundary() {
    for finished in [false, true] {
        for at in [1, 2] {
            let mut run = active();
            let mut member = Member::new(&mut run);
            let request = original_leg(&mut member, 44, finished);
            let original_phase = member.run.context.capture_snapshot().unwrap().phase;
            let now = member.now();
            let leave = AbsoluteBound::after(now, 10).unwrap();
            member
                .run
                .context
                .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
                .unwrap();
            let mut clock = JumpAtSample {
                clock: &mut member.run.clock,
                samples: 0,
                at,
                to: leave.deadline_ns(),
            };
            let outcome = member
                .run
                .context
                .cancel_member_capture(44.into(), &mut clock);
            if at == 1 {
                assert_eq!(
                    outcome,
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound: leave,
                    })
                );
                assert_eq!(
                    member.run.context.capture_snapshot().unwrap().phase,
                    original_phase
                );
                assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
                member
                    .run
                    .context
                    .complete_wait(PumpDeadline::Leave)
                    .unwrap();
                // This cancellation was not admitted. Its existing contract
                // requires the parent to re-issue it after handling Leave.
                member.advance().unwrap();
                assert_eq!(
                    member.run.context.capture_snapshot().unwrap().phase,
                    original_phase
                );
                assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
                assert_eq!(
                    member
                        .run
                        .context
                        .cancel_member_capture(44.into(), &mut member.run.clock)
                        .unwrap(),
                    Advance::Progress
                );
            } else {
                // Once admitted, cancellation queues its continuation even if
                // the enqueue clock crosses Leave. No intent is lost in a
                // second recoverable check; transport still honors Leave.
                assert_eq!(outcome, Ok(Advance::Progress));
                assert_eq!(
                    member.run.context.capture_snapshot().unwrap().phase,
                    CapturePhase::Aborting
                );
                assert_eq!(member.run.context.flow.state().outstanding().len(), 2);
                assert_eq!(
                    member.advance().unwrap_err(),
                    RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound: leave,
                    }
                );
                member
                    .run
                    .context
                    .complete_wait(PumpDeadline::Leave)
                    .unwrap();
            }
            let cleanup = member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .unwrap();
            assert_eq!(cleanup.began_ns(), leave.deadline_ns());
            assert_eq!(cleanup.deadline_ns() - cleanup.began_ns(), SMALL_CONTROL_NS);
            assert!(member.run.context.capture_timeout().is_none());
            assert_eq!(
                member
                    .run
                    .context
                    .cancel_member_capture(44.into(), &mut member.run.clock)
                    .unwrap(),
                Advance::Pending
            );
            assert_eq!(
                member.run.context.deadlines.get(PumpDeadline::StateControl),
                Some(cleanup)
            );
            let abort = member.sent();
            assert_eq!(abort.common(), request.common());
            let late = original_answer(&mut member, &request, 44, finished);
            member.discard_late(&late, cleanup);
            member.aborted(&abort, 44);
        }
    }
}

#[test]
fn wrong_capture_id_cannot_enqueue_an_already_owed_abort() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let request = member.snapshot(45, SAVE_NS);
    let refusal = member.answer(
        &request,
        s::StateControlAnswer::ControlRefused {
            code: s::ControlRefusalCode::Capture(s::CaptureRefusalCode::SnapshotFailed),
            detail: "owned refusal".into(),
        },
    );
    member.receive(&refusal).unwrap();
    hold_recorded_original(&mut member);
    let now = member.now();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
        .unwrap();
    let mut clock = JumpAtSample {
        clock: &mut member.run.clock,
        samples: 0,
        at: 1,
        to: leave.deadline_ns(),
    };
    assert_eq!(
        member
            .run
            .context
            .settle_recorded_capture(&mut clock)
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    member
        .run
        .context
        .complete_wait(PumpDeadline::Leave)
        .unwrap();
    assert!(member.run.context.flow.state().outstanding().is_empty());
    assert!(matches!(
        member
            .run
            .context
            .cancel_member_capture(46.into(), &mut member.run.clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert!(
        member.run.context.flow.state().outstanding().is_empty(),
        "wrong capture identity must be refused before any owed cleanup is enqueued"
    );
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
}

#[test]
fn cancellation_cannot_replace_an_owned_release_at_either_clock_boundary() {
    for at in [1, 2] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        member.finished_capture(47);
        member
            .run
            .context
            .release_member_capture(47.into(), &mut member.run.clock)
            .unwrap();
        let release = member.sent();
        let bound = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        let now = member.now();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        member
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
            .unwrap();
        let mut clock = JumpAtSample {
            clock: &mut member.run.clock,
            samples: 0,
            at,
            to: leave.deadline_ns(),
        };
        let result = member
            .run
            .context
            .cancel_member_capture(47.into(), &mut clock);
        if at == 1 {
            assert_eq!(
                result,
                Err(RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound: leave,
                })
            );
            assert_eq!(
                member.run.context.deadlines.get(PumpDeadline::StateControl),
                Some(bound)
            );
            member
                .run
                .context
                .complete_wait(PumpDeadline::Leave)
                .unwrap();
            let answer = member.answer(
                &release,
                s::StateControlAnswer::CaptureReleased {
                    capture_id: 47.into(),
                },
            );
            member.result(CaptureProgress::Complete, &answer);
            member
                .run
                .context
                .finish_member_capture(47.into(), &mut member.run.clock)
                .unwrap();
            assert!(member.run.context.halt().is_none());
        } else {
            assert!(matches!(
                result,
                Err(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                })
            ));
            assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
            assert_eq!(
                member.run.context.flow.state().outstanding()[0].operation,
                s::StateOperation::ReleaseCapture
            );
        }
    }
}

use super::super::capture_driver::CaptureTimeout;
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::clock::{WaitInterest, WaitOutcome};

struct JumpAtSample<'a> {
    clock: &'a mut FakePumpIo,
    samples: usize,
    at: usize,
    to: u64,
}

impl PumpIo for JumpAtSample<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.samples += 1;
        if self.samples == self.at {
            self.clock.advance_to(self.to)?;
        }
        self.clock.now()
    }

    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        self.clock.wait(interests, deadline)
    }
}

#[test]
fn recorded_refusal_resumes_abort_after_leave_interrupts_cleanup_admission() {
    for finished in [false, true] {
        let mut run = active();
        let mut member = Member::new(&mut run);
        let snapshot = member.snapshot(41, SAVE_NS);
        let pending = if finished {
            let image = member.image(&snapshot, 41);
            member.result(CaptureProgress::ImageReady, &image);
            member
                .run
                .context
                .acknowledge_member_capture(
                    41.into(),
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
                detail: "retain this refusal through cleanup admission".into(),
            },
        );
        member.receive(&original).unwrap();
        // Obtain the coordinator's real Recorded proof without letting the
        // context consume it before the lifecycle interruption is installed.
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
        assert!(matches!(
            member.run.context.flow.operation_completion(),
            Some(OperationCompletion::Capture { .. })
        ));
        let now = member.now();
        let leave = AbsoluteBound::from_absolute(now, now + 10).unwrap();
        member
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
            .unwrap();
        let mut jumping = JumpAtSample {
            clock: &mut member.run.clock,
            samples: 0,
            at: 1,
            to: leave.deadline_ns(),
        };
        assert!(matches!(
            member.run.context.settle_recorded_capture(&mut jumping),
            Err(RunFailure::Wait { key: PumpDeadline::Leave, bound }) if bound == leave
        ));
        assert!(member.run.context.flow.operation_completion().is_none());
        assert!(member.run.context.capture_timeout().is_none());
        assert!(member.run.context.capture_refusal().is_none());
        assert!(member.run.context.flow.state().outstanding().is_empty());
        for _ in 0..2 {
            assert!(matches!(
                member.advance(),
                Err(RunFailure::Wait { key: PumpDeadline::Leave, bound }) if bound == leave
            ));
            assert!(member.run.context.flow.state().outstanding().is_empty());
        }
        // Lifecycle handles its own wait. Ordinary capture progress must now
        // find the retained cleanup obligation without a second cancellation.
        assert_eq!(
            member.run.context.complete_wait(PumpDeadline::Leave),
            Some(leave)
        );
        member.advance().unwrap();
        let requests = member.run.context.flow.state().outstanding();
        assert_eq!(
            requests.len(),
            1,
            "the original refusal still owes one Abort"
        );
        assert_eq!(requests[0].operation, s::StateOperation::Abort);
        assert_eq!(requests[0].common, *pending.common());
        let cleanup = member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        for _ in 0..3 {
            member.advance().unwrap();
            assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
            assert_eq!(
                member.run.context.deadlines.get(PumpDeadline::StateControl),
                Some(cleanup),
                "retrying cleanup must not renew its deadline"
            );
        }
        let abort = member.sent();
        assert!(member.run.context.capture_refusal().is_none());
        let answer = member.answer(
            &abort,
            s::StateControlAnswer::Aborted {
                capture_id: 41.into(),
            },
        );
        member.result(CaptureProgress::Complete, &answer);
        let refusal = member.run.context.capture_refusal().unwrap();
        assert_eq!(
            s::encode_state_line(&refusal.answer, s::FrameDirection::Response).unwrap(),
            s::encode_state_line(&original, s::FrameDirection::Response).unwrap()
        );
        assert!(member.run.context.capture_timeout().is_none());
        assert!(member.run.context.flow.state().outstanding().is_empty());
        member
            .run
            .context
            .finish_member_capture(41.into(), &mut member.run.clock)
            .unwrap();
        assert!(member.run.context.halt().is_none());
    }
}

#[test]
fn window_expiry_at_actual_snapshot_enqueue_retains_request_for_abort() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let window = member.prepare(7, SAVE_NS);
    let enqueue = window.deadline_ns.0 + 1;
    // The first sample is the context's live check. The second is the exact
    // sample retained by start_capture_leg and StateChannel::begin.
    let mut jumping = JumpAtSample {
        clock: &mut member.run.clock,
        samples: 0,
        at: 2,
        to: enqueue,
    };
    member
        .run
        .context
        .advance_member_capture(&mut jumping)
        .unwrap();
    assert!(jumping.samples >= 2);
    assert_eq!(member.run.context.flow.capture_queued_ns(), Some(enqueue));
    assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
    assert!(member.run.context.halt().is_none());
    member.advance().unwrap();
    assert_eq!(
        member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::Aborting
    );
    assert!(member.run.context.halt().is_none());
    let requests = member.run.context.flow.state().outstanding();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].operation, s::StateOperation::Snapshot);
    assert_eq!(requests[1].operation, s::StateOperation::Abort);
    assert_eq!(requests[0].common, requests[1].common);
    let cleanup = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert_eq!(cleanup.began_ns(), enqueue);
    assert_eq!(cleanup.deadline_ns() - cleanup.began_ns(), SMALL_CONTROL_NS);
}

#[test]
fn expired_save_bounds_cannot_hide_an_already_expired_leave_from_cleanup() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    member.snapshot(7, SAVE_NS);
    let save = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow)
        .unwrap();
    let now = member.now();
    // The original result and save windows expire first. Leave nevertheless
    // already expired before cleanup can begin, and remains the outer limit.
    let leave = AbsoluteBound::from_absolute(now, save.deadline_ns() + 10).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
        .unwrap();
    member
        .run
        .clock
        .advance_to(leave.deadline_ns() + 1)
        .unwrap();
    assert!(matches!(member.advance(), Err(RunFailure::Wait {
        key: PumpDeadline::Leave, bound,
    }) if bound == leave));
    let requests = member.run.context.flow.state().outstanding();
    assert_eq!(
        requests.len(),
        1,
        "expired containment cannot enqueue another request"
    );
    assert_eq!(requests[0].operation, s::StateOperation::Snapshot);
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(
        member.run.context.capture_snapshot().unwrap().phase,
        CapturePhase::SnapshotPending
    );
    assert!(
        member.run.context.halt().is_none(),
        "the existing lifecycle caller owns Leave expiry"
    );
}

#[test]
fn release_bound_tied_to_save_window_still_latches_missing_acknowledgement() {
    let mut run = active();
    let mut member = Member::new(&mut run);
    let snapshot = member.snapshot(7, SAVE_NS);
    let image = member.image(&snapshot, 7);
    member.result(CaptureProgress::ImageReady, &image);
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
    let finished = member.answer(
        &acknowledge,
        s::StateControlAnswer::Finished {
            capture_id: 7.into(),
            room_name: "7.sqlite".into(),
        },
    );
    member.result(CaptureProgress::FinishedReady, &finished);
    let window = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow)
        .unwrap();
    member
        .run
        .clock
        .advance_to(window.deadline_ns() - 1)
        .unwrap();
    member
        .run
        .context
        .release_member_capture(7.into(), &mut member.run.clock)
        .unwrap();
    let control = member
        .run
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert_eq!(control.deadline_ns(), window.deadline_ns());
    assert_eq!(control.began_ns(), window.deadline_ns() - 1);
    member.run.clock.advance_to(window.deadline_ns()).unwrap();
    assert!(matches!(
        member.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            point: HaltPoint::Barrier,
            ..
        })
    ));
    assert!(member.run.context.halt().is_some());
    let outstanding = member.run.context.flow.state().outstanding();
    assert_eq!(outstanding.len(), 1);
    assert_eq!(outstanding[0].operation, s::StateOperation::ReleaseCapture);
    assert!(member.run.context.capture_snapshot().is_some());
}
