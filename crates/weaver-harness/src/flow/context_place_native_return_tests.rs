//! Entered cancellation settles committed native facts without granting a turn.
use super::*;

fn take_cancelled_native(fx: &mut PlaceFixture) -> PlaceAccount {
    for _ in 0..128 {
        let run = fx.run();
        run.context
            .advance_place_marks(&mut ports(&mut run.clock))
            .unwrap();
        assert!(
            run.context
                .take_place_marks(&mut run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            run.context.barrier.place_owed_for_test(),
            "the actual native owner still owes extraction"
        );
        if let Some(account) = run
            .context
            .take_cancelled_place_marks(&mut run.clock)
            .unwrap()
        {
            assert!(
                !run.context.barrier.place_owed_for_test(),
                "cancelled extraction releases the actual barrier duty"
            );
            return account;
        }
    }
    panic!("completed cancelled native job must leave Entered with its evidence");
}

#[test]
fn place_native_return_stop_settles_exact_recorded_scopes_and_allows_next_turn_generation() {
    for (at_rest, already_complete) in [(false, false), (false, true), (true, false), (true, true)]
    {
        let (mut fx, plan) = PlaceFixture::with_active_scope();
        let first = fx.text(8, "record");
        let later = fx.text(9, "later");
        fx.turn("native-stop-original");
        fx.generation(first, 3, 0, "", Finish::Completed);
        // The selected input and the empty assistant completion each occupy
        // one journal message; native mark settlement adds neither.
        let original_messages = fx.journal().6;
        assert_eq!(original_messages, 2);
        if at_rest {
            fx.close_turn();
        }
        let proof = fx.closed_effect(plan);
        let read = proof.read() as *const RecordedRead;
        let sequence = proof.filing().sequence();
        let bound = proof.filing().wait_bound();
        let before = fx.records(Kind::StateMarkPosition).len();
        let run = fx.run();
        run.context
            .begin_place_effect_marks(proof, &mut run.clock)
            .unwrap();
        // Exercise actual partial recording before cancellation, not owner bits.
        for _ in 0..128 {
            let run = fx.run();
            let progress = run
                .context
                .advance_place_marks(&mut ports(&mut run.clock))
                .unwrap();
            if already_complete && progress == PlaceProgress::Complete {
                break;
            }
            if !already_complete && fx.records(Kind::StateMarkPosition).len() == before + 1 {
                break;
            }
        }
        assert_eq!(
            fx.records(Kind::StateMarkPosition).len(),
            before + if already_complete { 2 } else { 1 }
        );
        fx.observe_place_command(weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        });
        let account = take_cancelled_native(&mut fx);
        assert!(account.incomplete().is_none());
        assert_eq!(
            account.native_prerequisite().unwrap().read() as *const RecordedRead,
            read
        );
        assert_eq!(
            account.native_prerequisite().unwrap().filing().sequence(),
            sequence
        );
        assert_eq!(
            account.native_prerequisite().unwrap().filing().wait_bound(),
            bound
        );
        assert_eq!(account.native_positions().len(), 2);
        assert_eq!(account.native_recorded_sequences().len(), 2);
        assert_eq!(fx.records(Kind::StateMarkPosition).len(), before + 2);
        assert!(
            account.native_proposal().is_some(),
            "cancelled account retains the exact proposal"
        );
        assert_eq!(
            format!("{:?}", account.native_proposal().unwrap()),
            format!(
                "{:?}",
                fx.run().context.place_engine.as_ref().unwrap().native
            )
        );
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert!(receive(fx.peer.as_fd()).is_none());
        let run = fx.run();
        assert!(
            run.context
                .take_cancelled_place_marks(&mut run.clock)
                .unwrap()
                .is_none()
        );
        if !at_rest {
            assert_eq!(
                run.context
                    .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
                    .unwrap_err(),
                PlaceError::WrongPhase
            );
            assert!(
                !run.context.native_place_cache_ready(),
                "current stopped turn remains barred"
            );
            fx.close_turn();
        }
        assert!(
            fx.run().context.native_place_cache_ready(),
            "settled committed scopes match the cache"
        );
        fx.turn("native-stop-next");
        fx.generation(later, 2, 0, "", Finish::Completed);
        assert!(fx.run().context.failure().is_none());
        assert_eq!(fx.journal().6, original_messages + 2);
    }
}

#[test]
fn place_native_return_leave_settles_facts_but_never_restores_append_admission() {
    for join in [false, true] {
        let (mut fx, plan) = PlaceFixture::with_active_scope();
        let units = fx.text(8, "record");
        let later = fx.text(9, "later");
        fx.turn("native-leave-original");
        fx.generation(units, 3, 0, "", Finish::Completed);
        let original_messages = fx.journal().6;
        assert_eq!(original_messages, 2);
        let prefix = fx.prefix(None, s::CutMethod::Elide);
        let proof = fx.closed_effect(plan);
        let run = fx.run();
        run.context
            .begin_place_effect_marks(proof, &mut run.clock)
            .unwrap();
        fx.observe_place_command(if join {
            weaver_types::LifecycleDirective::JoinLeave {
                cause: weaver_types::Cause { uid: 0 },
            }
        } else {
            weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 0 },
                forced: true,
                rollback: true,
            }
        });
        let account = take_cancelled_native(&mut fx);
        assert_eq!(account.native_positions().len(), 2);
        assert_eq!(account.native_recorded_sequences().len(), 2);
        assert!(account.native_proposal().is_some());
        fx.close_turn();
        assert!(!fx.run().context.native_place_cache_ready());
        let run = fx.run();
        assert_eq!(
            run.context
                .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
                .unwrap_err(),
            PlaceError::WrongPhase
        );
        let tail = ReloadTail::held(later);
        let ReloadItem::Held(unit) = &tail.items[0] else {
            unreachable!()
        };
        let exact = std::sync::Arc::as_ptr(unit);
        let run = fx.run();
        let error = run
            .context
            .begin_cut_and_reload(prefix, tail, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlaceError::InvalidTail);
        assert!(error.failure.is_none());
        let ReloadItem::Held(unit) = &error.input.1.items[0] else {
            unreachable!()
        };
        assert_eq!(std::sync::Arc::as_ptr(unit), exact);
        let mut started = crate::flow::context::tests::event(Kind::TurnStarted, None);
        started.turn = Some(weaver_trace::TurnRef("forbidden-after-leave".into()));
        assert_eq!(run.author(started).unwrap_err(), RunFailure::PlaceOwned);
        assert!(receive(fx.peer.as_fd()).is_none());
        assert_eq!(fx.journal().6, original_messages);
    }
}

#[test]
fn place_native_return_observe_keeps_success_and_permits_next_turn_generation() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let first = fx.text(8, "record");
    let later = fx.text(9, "later");
    fx.turn("native-observe");
    fx.generation(first, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    fx.observe_place_command(weaver_types::LifecycleDirective::Observe);
    let run = fx.run();
    assert!(
        run.context
            .take_cancelled_place_marks(&mut run.clock)
            .unwrap()
            .is_none()
    );
    let done = fx.finish_native();
    assert_eq!(done.positions().len(), 2);
    fx.close_turn();
    fx.turn("native-observe-next");
    fx.generation(later, 2, 0, "", Finish::Completed);
}

#[test]
fn place_native_return_under_recorder_pressure_waits_for_every_original_recorded_position() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let first = fx.text(8, "record");
    fx.turn("native-pressure-stop");
    fx.generation(first, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let bound = proof.filing().wait_bound();
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    fx.summary.boot.member.fill_trace();
    assert!(native_marks_pressure(&mut fx).is_some());
    native_marks_pressure(&mut fx);
    fx.observe_place_command(weaver_types::LifecycleDirective::Stop {
        cause: weaver_types::Cause { uid: 0 },
    });
    for _ in 0..3 {
        native_marks_pressure(&mut fx);
        let run = fx.run();
        assert!(
            run.context
                .take_cancelled_place_marks(&mut run.clock)
                .unwrap()
                .is_none()
        );
        assert!(run.context.native_marks.is_some());
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.summary.drain();
    let account = take_cancelled_native(&mut fx);
    assert_eq!(account.native_positions().len(), 2);
    assert_eq!(account.native_recorded_sequences().len(), 2);
    assert!(account.native_proposal().is_some());
}

#[path = "context_place_phase_review_tests.rs"]
mod phase_review;
