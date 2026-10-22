//! Failure ordering and final-drain checks through the run's actual owners.

use super::super::clock::{WaitInterest, WaitOutcome};
use super::super::coordinator::TransportSource;
use super::super::pump::LifecyclePorts;
use super::tests::{Fixture, config, event};
use super::*;
use std::io;
use std::net::Shutdown;
use std::time::{Duration, Instant};

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn await_real_drain(fx: &mut Fixture) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if matches!(
            fx.context
                .barrier
                .drain_status(&mut fx.context.budget)
                .unwrap(),
            serving::DrainStatus::Emptied
        ) {
            return;
        }
        assert!(Instant::now() < deadline, "real trace writer did not drain");
        std::thread::yield_now();
    }
}

fn assert_state_failure_precedes_later_writer_failure(through_live: bool) {
    let mut fx = Fixture::new(config());
    fx.activate();
    await_real_drain(&mut fx);

    // Observe real state EOF before breaking the independent trace socket.
    // The coordinator has frozen its owners, but the context has not latched.
    fx.state.shutdown(Shutdown::Write).unwrap();
    let state_failure = fx
        .context
        .flow
        .transport_one(
            &mut FlowRecording {
                barrier: &mut fx.context.barrier,
                inbox: &mut fx.context.inbox,
                budget: &mut fx.context.budget,
                author: &fx.context.author,
            },
            &mut fx.clock,
            TransportSource::State,
        )
        .unwrap_err();
    assert_eq!(state_failure.point, "state_transport");
    assert_eq!(fx.context.flow.failure(), Some(&state_failure));
    assert!(fx.context.halt().is_none());
    assert!(fx.context.barrier.failure().is_none());
    assert!(fx.context.barrier.standing_failure().is_none());
    let state = fx.context.flow.standing_state().unwrap().failure().unwrap();
    assert_eq!(state.kind, ChannelFailureKind::Carry);
    assert_eq!(state.point, ChannelPoint::Reply);
    assert!(fx.context.budget.is_frozen());
    assert!(fx.context.barrier.recorder().tap_stopped());

    fx.clock.advance_to(105).unwrap();
    fx._reader.shutdown(Shutdown::Both).unwrap();
    // A valid terminal record drives the real writer after the state failure.
    // This exercises the lower owners' ordering, not a synthetic trace error
    // or a claimed implementation of the full halt teardown sequence.
    fx.context
        .barrier
        .submit_terminal_authored(event(Kind::SessionClosed, None), &fx.context.author, 105, 1)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let writer_failure = loop {
        if let Some(failure) = fx.context.barrier.standing_failure() {
            break failure;
        }
        assert!(Instant::now() < deadline, "real trace writer did not fail");
        std::thread::yield_now();
    };
    assert!(fx.context.barrier.failure().is_none());

    let result = if through_live {
        fx.context.live(&mut fx.clock).unwrap_err()
    } else {
        fx.context.flow_failed(&mut fx.clock, state_failure.clone())
    };
    let expected = RunFailure::Flow {
        reason: FlowHaltReason::CarryFailed,
        point: HaltPoint::StateReply,
        detail: state_failure.detail.clone(),
    };
    assert_eq!(result, expected);
    assert_eq!(fx.context.phase(), RecordingPhase::Failed);
    assert_eq!(fx.context.flow.failure(), Some(&state_failure));
    let halt = fx.context.halt().unwrap();
    assert_eq!(halt.first().reason, FlowHaltReason::CarryFailed);
    assert_eq!(halt.first().point, HaltPoint::StateReply);
    assert_eq!(halt.first().detail, state_failure.detail);
    assert_eq!(halt.first().observed_ns, Some(105.into()));
    assert_eq!(halt.secondary().len(), 1);
    assert_eq!(halt.secondary()[0].reason, FlowHaltReason::TraceFailed);
    assert_eq!(halt.secondary()[0].point, HaltPoint::Author);
    assert!(
        halt.secondary()[0]
            .detail
            .contains(&format!("{writer_failure:?}"))
    );
    let first = halt.first().clone();
    let deadline = halt.deadline();

    // Reobserving the failed context cannot replace the cause or restart time.
    fx.clock.advance_to(106).unwrap();
    assert_eq!(fx.context.live(&mut fx.clock).unwrap_err(), expected);
    let halt = fx.context.halt().unwrap();
    assert_eq!(halt.first(), &first);
    assert_eq!(halt.deadline(), deadline);
    assert_eq!(halt.secondary().len(), 1);
}

#[test]
fn flow_failure_mapping_preserves_state_eof_before_a_later_writer_failure() {
    assert_state_failure_precedes_later_writer_failure(false);
}

#[test]
fn live_entry_preserves_known_state_eof_before_a_later_writer_failure() {
    assert_state_failure_precedes_later_writer_failure(true);
}

struct ExpireAfterEntry {
    reads: usize,
    before: u64,
    expired: u64,
}

impl PumpIo for ExpireAfterEntry {
    fn now(&mut self) -> io::Result<u64> {
        let now = if self.reads == 0 {
            self.before
        } else {
            self.expired
        };
        self.reads += 1;
        Ok(now)
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("the real writer was already drained before this clock began");
    }
}

#[test]
fn final_drain_rechecks_leave_deadline_and_resumes_the_same_closed_coordinator() {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let leave = AbsoluteBound::after(104, 5).unwrap();
    fx.context.begin_releasing(leave, &mut fx.clock).unwrap();
    assert_eq!(
        fx.context.retire_state(&mut fx.clock).unwrap(),
        Advance::Progress
    );
    assert_eq!(
        fx.context.retire_classifier(&mut fx.clock).unwrap(),
        Advance::Progress
    );
    assert_eq!(
        fx.author(event(
            Kind::Unload,
            Some(Payload::Unload(Box::new(serving::payloads::UnloadClose {
                grant_surface: None,
                cause: None,
                forced: false,
                forced_by: None,
                release: None,
                end_reason: None,
                state_flow_failure: None,
            }))),
        ))
        .unwrap(),
        Sequence(2)
    );
    await_real_drain(&mut fx);
    let origin = fx.context.origin_ns();
    let records = fx.context.barrier.recorder().structure().len();
    let mut io = ExpireAfterEntry {
        reads: 0,
        before: 104,
        expired: leave.deadline_ns(),
    };

    assert_eq!(
        fx.context
            .close_after_drain(&mut ports(&mut io))
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave,
        }
    );
    assert_eq!(
        io.reads, 2,
        "entry and completed drain each check the clock"
    );
    assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
    assert_eq!(fx.context.flow.phase(), CoordinatorPhase::Closed);
    assert!(fx.context.failure().is_none());
    assert!(fx.context.halt().is_none());
    assert!(fx.context.barrier.is_closed());
    assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));

    // The lifecycle owner acknowledges its own expired wait. Final drain then
    // resumes the same owners, without calling close twice on the coordinator.
    assert_eq!(fx.context.complete_wait(PumpDeadline::Leave), Some(leave));
    fx.clock.advance_to(leave.deadline_ns()).unwrap();
    fx.context
        .close_after_drain(&mut ports(&mut fx.clock))
        .unwrap();
    assert_eq!(fx.context.phase(), RecordingPhase::Closed);
    assert_eq!(fx.context.flow.phase(), CoordinatorPhase::Closed);
    assert_eq!(fx.context.origin_ns(), origin);
    assert_eq!(fx.context.barrier.recorder().structure().len(), records);
    assert!(fx.context.halt().is_none());
}
