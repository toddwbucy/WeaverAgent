//! Exact clock boundaries reuse the real capture context and member socket.

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
