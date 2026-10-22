//! Keep a blocked recall author while observing the real state socket's failure.

use super::*;
use crate::flow::channel::{ChannelFailureKind, ChannelPoint};

struct PendingRecall {
    ticket: u64,
    payload: String,
    bound: AbsoluteBound,
}

fn pending(member: &mut Member) -> PendingRecall {
    released(member);
    member.fill_trace();
    assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
    let author = member.run.context.pending_author().unwrap();
    PendingRecall {
        ticket: author.ticket,
        payload: serde_json::to_string(&author.event.payload).unwrap(),
        bound: member.bound(),
    }
}

fn break_state(member: &mut Member, malformed: bool) {
    if malformed {
        // No state request remains after Released. The real parser rejects
        // these unsolicited bytes before decoding them as a new response.
        member.run.state.write_all(b"{}\n").unwrap();
    } else {
        // Shutdown is deterministic even if a concurrent fixture fork briefly
        // inherited another copy of this peer descriptor.
        member.run.state.shutdown(Shutdown::Both).unwrap();
    }
}

fn expected_reason(malformed: bool) -> FlowHaltReason {
    if malformed {
        FlowHaltReason::ProtocolFailed
    } else {
        FlowHaltReason::CarryFailed
    }
}

fn assert_failed(
    member: &mut Member,
    held: &PendingRecall,
    failure: RunFailure,
    reason: FlowHaltReason,
) {
    assert!(
        matches!(&failure, RunFailure::Flow { reason: actual, point: HaltPoint::StateReply, .. }
            if *actual == reason),
        "wrong state failure attribution: {failure:?}"
    );
    let first = member.run.context.halt().unwrap().first();
    assert_eq!(first.reason, reason);
    assert_eq!(first.point, HaltPoint::StateReply);
    let author = member.run.context.pending_author().unwrap();
    assert_eq!(author.ticket, held.ticket);
    assert_eq!(serde_json::to_string(&author.event.payload).unwrap(), held.payload);
    assert_eq!(member.bound(), held.bound);
    assert_eq!(member.requests.len(), 2);
    assert!(member.records(Kind::Recall).is_empty());
    assert!(member.run.context.flow.operation_completion().is_none());
    assert!(member.run.context.flow.ledger().facts().next().is_none());
    assert_eq!(
        member.run.context.take_read(&mut member.run.clock).unwrap_err(),
        failure
    );
}

#[test]
fn recall_common_pump_reports_real_state_failure_before_the_original_bound() {
    for malformed in [false, true] {
        let mut member = Member::new();
        let held = pending(&mut member);
        break_state(&mut member, malformed);
        assert!(member.run.context.flow.state().failure().is_none());
        let now = member.now() + 1;
        let failure = member.pump_at(now, PollFlags::POLLIN).unwrap_err();
        assert!(member.now() < held.bound.deadline_ns());
        assert_failed(&mut member, &held, failure, expected_reason(malformed));
    }
}

#[test]
fn recall_local_advance_observes_real_parser_failure_without_reducing_flow() {
    for malformed in [false, true] {
        let mut member = Member::new();
        let held = pending(&mut member);
        break_state(&mut member, malformed);
        // Deliberately stop between the real channel latch and its production
        // coordinator mapping. This is a passive-status fixture, not a claim
        // that transport_one normally drops this error.
        let channel = member.run.context.flow
            .probe_state_without_failure_mapping_for_test(&mut member.run.clock)
            .unwrap_err();
        assert_eq!(channel.point, ChannelPoint::Reply);
        assert_eq!(channel.kind, if malformed {
            ChannelFailureKind::Protocol
        } else {
            ChannelFailureKind::Carry
        });
        assert!(member.run.context.flow.failure().is_none());
        let failure = member.advance().unwrap_err();
        assert!(member.now() < held.bound.deadline_ns());
        assert_failed(&mut member, &held, failure, expected_reason(malformed));
    }
}

#[test]
fn recall_observed_state_failure_keeps_its_cause_after_the_read_bound_passes() {
    let mut member = Member::new();
    let held = pending(&mut member);
    break_state(&mut member, false);
    member.run.context.flow
        .probe_state_without_failure_mapping_for_test(&mut member.run.clock)
        .unwrap_err();
    member.run.clock.advance_to(held.bound.deadline_ns()).unwrap();
    let failure = member.advance().unwrap_err();
    assert_failed(&mut member, &held, failure, FlowHaltReason::CarryFailed);
}

#[test]
fn recall_later_channel_failure_cannot_replace_the_first_context_latch() {
    let mut member = Member::new();
    let held = pending(&mut member);
    let first = member.run.context.fail(
        &mut member.run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Classifier,
        "earlier classifier protocol failure",
    );
    let evidence = member.run.context.halt().unwrap().first().clone();
    break_state(&mut member, false);
    member.run.context.flow
        .probe_state_without_failure_mapping_for_test(&mut member.run.clock)
        .unwrap_err();
    assert_eq!(member.advance().unwrap_err(), first);
    assert_eq!(member.run.context.halt().unwrap().first(), &evidence);
    assert_eq!(member.run.context.pending_author().unwrap().ticket, held.ticket);
    assert_eq!(member.bound(), held.bound);
}

#[test]
fn recall_known_recorder_failure_keeps_priority_over_an_unmapped_state_failure() {
    let mut member = Member::new();
    let held = pending(&mut member);
    member.run._reader.shutdown(Shutdown::Both).unwrap();
    let watch = Instant::now() + Duration::from_secs(5);
    loop {
        let context = &mut member.run.context;
        if context.barrier.drain_status(&mut context.budget).is_err() {
            break;
        }
        assert!(Instant::now() < watch, "writer did not observe its closed sink");
        std::thread::yield_now();
    }
    break_state(&mut member, false);
    member.run.context.flow
        .probe_state_without_failure_mapping_for_test(&mut member.run.clock)
        .unwrap_err();
    let failure = member.advance().unwrap_err();
    assert!(matches!(failure, RunFailure::Flow {
        reason: FlowHaltReason::TraceFailed,
        point: HaltPoint::Author,
        ..
    }));
    assert_eq!(member.run.context.halt().unwrap().first().reason, FlowHaltReason::TraceFailed);
    assert_eq!(member.run.context.pending_author().unwrap().ticket, held.ticket);
    assert_eq!(member.bound(), held.bound);
}

#[test]
fn recall_healthy_channel_preserves_lifecycle_wait_and_original_control_expiry() {
    let mut member = Member::new();
    let held = pending(&mut member);
    let now = member.now();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    member.run.context.arm(PumpDeadline::Leave, leave, &mut member.run.clock).unwrap();
    member.run.clock.advance_to(leave.deadline_ns()).unwrap();
    for _ in 0..2 {
        assert_eq!(member.advance().unwrap_err(), RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave,
        });
        assert!(member.run.context.halt().is_none());
        assert_eq!(member.run.context.pending_author().unwrap().ticket, held.ticket);
        assert_eq!(member.bound(), held.bound);
    }
    member.run.context.complete_wait(PumpDeadline::Leave).unwrap();
    member.run.clock.advance_to(held.bound.deadline_ns()).unwrap();
    assert!(matches!(member.advance(), Err(RunFailure::Flow {
        reason: FlowHaltReason::ControlBound,
        point: HaltPoint::Barrier,
        ..
    })));
    assert!(member.run.context.flow.state().failure().is_none());
    assert_eq!(member.run.context.pending_author().unwrap().ticket, held.ticket);
    assert_eq!(member.bound(), held.bound);
}
