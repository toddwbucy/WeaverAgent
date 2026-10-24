//! Original custody across multiple submissions, pending withdrawal and halt.
use super::*;
use crate::flow::context::{AuthorFailure, StoppedAuthor};

const _: fn() = || {
    trait NotClone<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <AuthorFailure as NotClone<_>>::pin;
    trait NotLossy<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotLossy<()> for T {}
    impl<T: From<AuthorFailure>> NotLossy<u8> for T {}
    let _ = <RunFailure as NotLossy<_>>::pin;
};

fn attempt(fx: &mut Fixture, event: AuthoredEvent) -> AuthorFailure {
    fx.context
        .author_and_wait(&mut ports(&mut fx.clock), event)
        .unwrap_err()
}

fn exact(original: &AuthoredEvent) -> (usize, String) {
    (original_pointer(original), format!("{original:?}"))
}

#[test]
fn author_failure_second_and_third_submit_preserve_first_held_original() {
    for (boot, place, expected) in [
        (true, false, RunFailure::BootOpenOwned),
        (false, true, RunFailure::PlaceOwned),
        (true, true, RunFailure::PlaceOwned),
    ] {
        let mut fx = releasing(true, false);
        fx.context.barrier.set_boot_open_owed(boot);
        fx.context.barrier.set_place_owed(place);
        let first = shutdown(Kind::Fault);
        let held = exact(&first);
        let before = fx.context.barrier.recorder().structure().len();
        let first_result = attempt(&mut fx, first);
        assert_eq!(first_result.reason, expected);
        assert!(first_result.original.is_none());
        assert!(fx.context.failure().is_none());
        let second = shutdown(Kind::Refusal);
        let original = exact(&second);
        let second = attempt(&mut fx, second);
        assert!(matches!(
            second.reason,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        assert_eq!(second.original.as_ref().map(exact), Some(original));
        let cap = fx.context.halt().unwrap().deadline();
        let third = shutdown(Kind::Fault);
        let original = exact(&third);
        let third = attempt(&mut fx, third);
        assert_eq!(third.reason, second.reason);
        assert_eq!(third.original.as_ref().map(exact), Some(original));
        assert_eq!(fx.context.held_author().map(exact), Some(held));
        assert!(fx.context.pending_author().is_none());
        assert!(fx.context.author_turn.is_none());
        assert!(fx.context.release_author.is_none());
        assert_eq!(fx.context.halt().unwrap().deadline(), cap);
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    }
}

#[test]
fn author_failure_first_fatal_original_and_cause_are_immutable() {
    let mut fx = releasing(true, false);
    let mut first = shutdown(Kind::Fault);
    first.kind = Kind::MessageSystem; // Illegal in Releasing, before any ticket.
    let original = exact(&first);
    let first = attempt(&mut fx, first);
    assert!(first.original.is_none());
    assert_eq!(
        fx.context.refused_author().map(exact),
        Some(original.clone())
    );
    let cap = fx.context.halt().unwrap().deadline();
    for kind in [Kind::Refusal, Kind::Fault] {
        let event = shutdown(kind);
        let expected = exact(&event);
        let later = attempt(&mut fx, event);
        assert_eq!(later.reason, first.reason);
        assert_eq!(later.original.as_ref().map(exact), Some(expected));
        assert_eq!(
            fx.context.refused_author().map(exact),
            Some(original.clone())
        );
        assert_eq!(fx.context.halt().unwrap().deadline(), cap);
        assert!(fx.context.pending_author().is_none());
    }
}

#[test]
fn author_failure_second_submit_does_not_change_pending_ticket_turn_or_release() {
    for kind in [Kind::Refusal, Kind::SessionClosed] {
        let mut fx = releasing(true, true);
        let wait = AbsoluteBound::after(fx.clock.now().unwrap(), 10).unwrap();
        fx.context
            .arm(PumpDeadline::Label, wait, &mut fx.clock)
            .unwrap();
        let first = shutdown(Kind::Fault);
        let original = exact(&first);
        let pending = attempt(&mut fx, first);
        assert_eq!(
            pending.reason,
            RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: wait
            }
        );
        assert!(pending.original.is_none());
        let ticket = fx.context.pending_author().unwrap().ticket;
        let turn = fx.context.author_turn.clone();
        let release = Arc::clone(&fx.context.release_author.as_ref().unwrap().identity);
        fx.context.complete_wait(PumpDeadline::Label).unwrap();
        let second = attempt(&mut fx, event(kind, None));
        assert!(matches!(
            second.reason,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        assert!(second.original.is_none()); // First fatal evidence is retained.
        assert_eq!(fx.context.refused_author().unwrap().kind, kind);
        assert_eq!(fx.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(exact(&fx.context.pending_author().unwrap().event), original);
        assert_eq!(fx.context.author_turn, turn);
        assert!(Arc::ptr_eq(
            &release,
            &fx.context.release_author.as_ref().unwrap().identity
        ));
    }
}

#[test]
fn author_failure_both_resumes_return_withdrawn_original_when_held_slot_is_full() {
    for claimed in [false, true] {
        for placement in [false, true] {
            let mut fx = releasing(true, true);
            let leave = fx.context.deadlines.get(PumpDeadline::Leave);
            let wait = AbsoluteBound::after(fx.clock.now().unwrap(), 10).unwrap();
            fx.context
                .arm(PumpDeadline::Label, wait, &mut fx.clock)
                .unwrap();
            let original = shutdown(Kind::Refusal);
            let expected = exact(&original);
            let failure = attempt(&mut fx, original);
            assert!(matches!(failure.reason, RunFailure::Wait { .. }));
            assert!(failure.original.is_none());
            let ticket = fx.context.pending_author().unwrap().ticket;
            let before = fx.context.barrier.recorder().structure().len();
            fx.context.complete_wait(PumpDeadline::Label).unwrap();
            // Defensive transition: public owner admission prevents this debt
            // rise. Exercise the real CapacityPending withdrawal with both the
            // older held allocation and the original release ticket present.
            let held = shutdown(Kind::Fault);
            let held_identity = exact(&held);
            fx.context.held_author = Some(held);
            fx.context.barrier.set_boot_open_owed(!placement);
            fx.context.barrier.set_place_owed(placement);
            let failure = if claimed {
                fx.context
                    .resume_claimed_author_and_wait(&mut ports(&mut fx.clock), None)
            } else {
                fx.context.resume_author_and_wait(&mut ports(&mut fx.clock))
            }
            .unwrap_err();
            assert_eq!(
                failure.reason,
                if placement {
                    RunFailure::PlaceOwned
                } else {
                    RunFailure::BootOpenOwned
                }
            );
            assert_eq!(failure.original.as_ref().map(exact), Some(expected));
            assert_eq!(fx.context.held_author().map(exact), Some(held_identity));
            assert!(fx.context.pending_author().is_none());
            assert!(fx.context.barrier.pending().is_none());
            assert!(fx.context.author_turn.is_none());
            assert!(fx.context.release_author.is_none());
            assert!(!fx.context.author_admission_pending);
            assert!(fx.context.failure().is_none());
            assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), leave);
            assert_eq!(fx.context.barrier.recorder().structure().len(), before);
            assert!(ticket > 0);
        }
    }
}

#[test]
fn author_failure_returned_attempt_stays_with_caller_through_halt_cap() {
    let mut fx = releasing(true, false);
    let held = shutdown(Kind::Fault);
    let held_identity = exact(&held);
    fx.context.held_author = Some(held);
    let event = shutdown(Kind::Refusal);
    let original = exact(&event);
    let failure = attempt(&mut fx, event);
    let first = failure.reason.clone();
    let mut caller = StoppedAuthor::default();
    let mut started = true;
    assert_eq!(caller.retain_started(failure, &mut started), first);
    assert!(!started);
    fx.context
        .freeze_halt_account(ExternalFacts {
            outstanding_pairs: vec![],
            capture: None,
            wait: None,
            input_id: None,
        })
        .unwrap();
    assert!(
        fx.context
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .pending_author
            .is_none()
    );
    fx.context.begin_halt_report().unwrap();
    let cap = fx.context.halt().unwrap().deadline().unwrap();
    fx.clock.advance_to(cap).unwrap();
    assert_eq!(
        fx.context.advance_halt_trace(&mut fx.clock).unwrap(),
        HaltTraceProgress::AbortRequired
    );
    assert_eq!(
        fx.context.close_after_drain(&mut ports(&mut fx.clock)),
        Err(first.clone())
    );
    assert_eq!(caller.check(), Err(first));
    assert_eq!(
        caller
            .failure
            .as_ref()
            .unwrap()
            .original
            .as_ref()
            .map(exact),
        Some(original)
    );
    assert_eq!(fx.context.held_author().map(exact), Some(held_identity));
    assert!(fx.context.pending_author().is_none());
    assert_eq!(fx.context.phase(), RecordingPhase::Failed);
}

#[test]
fn author_failure_private_resume_does_not_grant_a_recorded_result_after_failure() {
    let mut fx = releasing(true, false);
    let progress = submit(&mut fx, shutdown(Kind::Fault));
    let RecordProgress::Recorded { sequence, .. } = progress else {
        panic!("actual Recorded event")
    };
    let failure = fx
        .context
        .wait_recorded(&mut ports(&mut BrokenClock), progress)
        .unwrap_err();
    assert!(failure.original.is_none());
    assert_eq!(fx.context.completed_author, Some(sequence));
    assert_eq!(fx.context.failure(), Some(&failure.reason));
    let cap = fx.context.halt().unwrap().deadline();
    for claimed in [false, true] {
        let resumed = if claimed {
            fx.context
                .resume_claimed_author_and_wait(&mut ports(&mut fx.clock), None)
        } else {
            fx.context.resume_author_and_wait(&mut ports(&mut fx.clock))
        }
        .unwrap_err();
        assert_eq!(resumed.reason, failure.reason);
        assert!(resumed.original.is_none());
        assert_eq!(fx.context.completed_author, Some(sequence));
        assert_eq!(fx.context.halt().unwrap().deadline(), cap);
    }
}
