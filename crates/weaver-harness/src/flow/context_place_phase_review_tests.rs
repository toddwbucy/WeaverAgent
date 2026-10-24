//! Real owner/account phase boundaries; no fabricated done or phase fields.
use super::*;
use std::sync::Arc;

fn unrelated_failure(fx: &mut PlaceFixture) -> RunFailure {
    let run = fx.run();
    // Exercise the actual foundation latch, as the existing boot account
    // fixture does. This injects an unrelated failure, not a transport claim.
    let first = run.context.fail(
        &mut run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        "phase review unrelated first failure",
    );
    assert_eq!(run.context.phase(), RecordingPhase::Failed);
    first
}

#[test]
fn place_phase_review_abandoned_reload_keeps_inputs_as_evidence_only() {
    for expired in [false, true] {
        let mut fx = PlaceFixture::new();
        let carry = fx.text(8, "retained carry");
        let prefix = fx.prefix(None, s::CutMethod::Flush);
        let assembly = prefix.assembly_id();
        let tail = ReloadTail::held(carry);
        let ReloadItem::Held(unit) = &tail.items[0] else {
            unreachable!()
        };
        let held = Arc::as_ptr(unit);
        fx.begin_reload(prefix, tail);
        let TokenDirective::MeasureDelta { keep, .. } = fx.reload_packet() else {
            panic!("real pending measure")
        };
        let run = fx.run();
        let cut = run.context.deadlines.get(PumpDeadline::Cut).unwrap();
        let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
        run.context
            .begin_place_release(leave, &mut run.clock)
            .unwrap();
        assert_eq!(run.context.phase(), RecordingPhase::Releasing);
        if expired {
            run.clock.advance_to(leave.deadline_ns()).unwrap();
        } else {
            fx.answer(&fx.measured(keep, 3));
        }
        let mut account = fx.settled_account();
        assert_eq!(account.incomplete().is_some(), expired);
        assert_eq!(account.reload_prefix().unwrap().assembly_id(), assembly);
        let ReloadItem::Held(unit) = &account.reload_tail().unwrap().items[0] else {
            unreachable!()
        };
        assert_eq!(Arc::as_ptr(unit), held);
        assert!(account.take_staged().is_none());
        if expired {
            assert!(account.take_reload_inputs().is_none());
            assert!(account.reload_prefix().is_some());
        } else {
            let returned = account
                .take_reload_inputs()
                .expect("completed no-mutation Leave returns inputs");
            assert_eq!(returned.prefix().assembly_id(), assembly);
            let ReloadItem::Held(unit) = &returned.original_tail().items[0] else {
                unreachable!()
            };
            assert_eq!(Arc::as_ptr(unit), held);
            assert!(account.take_reload_inputs().is_none());
        }
        assert!(fx.run().context.failure().is_none());
        assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Cut), Some(cut));
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_phase_review_failed_completed_reload_returns_neither_inputs_nor_staged() {
    for confirmed in [false, true] {
        let mut fx = PlaceFixture::new();
        let carry = fx.text(8, "retained carry");
        let prefix = fx.prefix(None, s::CutMethod::Elide);
        let assembly = prefix.assembly_id();
        let n = prefix.n();
        fx.begin_reload(prefix, ReloadTail::held(carry));
        if confirmed {
            assert!(matches!(
                fx.reload_packet(),
                TokenDirective::MeasureDelta { .. }
            ));
            fx.answer(&fx.measured(n, 3));
        } else {
            let run = fx.run();
            run.context.cancel_reload(&mut run.clock).unwrap();
        }
        for _ in 0..128 {
            if fx.advance_reload().unwrap() == ReloadProgress::Complete {
                break;
            }
        }
        // Completion is real, but custody has not been extracted. A later
        // actual latch changes the meaning of this account, not its facts.
        let first = unrelated_failure(&mut fx);
        fx.first_fault_attempt();
        let mut account = fx.settled_account();
        assert!(account.incomplete().is_none());
        assert_eq!(account.reload_prefix().unwrap().assembly_id(), assembly);
        if confirmed {
            assert!(account.staged_evidence().unwrap().recorded.is_some());
            assert!(!account.recorded_sequences().is_empty());
            assert!(
                fx.run()
                    .context
                    .place_engine
                    .as_ref()
                    .unwrap()
                    .staged
                    .is_none()
            );
        } else {
            assert!(account.staged_evidence().is_none());
        }
        assert!(account.take_reload_inputs().is_none());
        assert!(account.take_staged().is_none());
        assert!(account.reload_tail().is_some());
        assert_eq!(fx.run().context.failure(), Some(&first));
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

fn native_recorded_before_finish() -> (PlaceFixture, String, Sequence) {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.text(8, "record");
    fx.turn("native-phase-review");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let filing = proof.filing().sequence();
    let old = format!(
        "{:?}",
        fx.run().context.place_engine.as_ref().unwrap().native
    );
    let before = fx.records(Kind::StateMarkPosition).len();
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    for _ in 0..128 {
        let run = fx.run();
        let progress = run
            .context
            .advance_place_marks(&mut ports(&mut run.clock))
            .unwrap();
        if fx.records(Kind::StateMarkPosition).len() == before + 2 {
            assert_eq!(
                progress,
                PlaceProgress::Progress,
                "both real positions are Recorded before the separate finish step"
            );
            break;
        }
    }
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), before + 2);
    let run = fx.run();
    assert!(
        run.context
            .take_place_marks(&mut run.clock)
            .unwrap()
            .is_none()
    );
    assert!(run.context.barrier.place_owed_for_test());
    (fx, old, filing)
}

#[test]
fn place_phase_review_releasing_native_finish_does_not_adopt_or_extract_live_completion() {
    let (mut fx, old, filing) = native_recorded_before_finish();
    let run = fx.run();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    // Deliberately call the ordinary finish after lawful release, while its
    // actual recorded job remains owned. This reaches the phase guard itself.
    assert_eq!(
        run.context
            .advance_place_marks(&mut ports(&mut run.clock))
            .unwrap(),
        PlaceProgress::Complete
    );
    assert_eq!(
        format!("{:?}", run.context.place_engine.as_ref().unwrap().native),
        old
    );
    assert!(
        run.context
            .take_place_marks(&mut run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        run.context
            .take_cancelled_place_marks(&mut run.clock)
            .unwrap()
            .is_none()
    );
    assert!(run.context.native_marks.is_some());
    let account = run
        .context
        .take_place_account()
        .expect("the actual stopped account remains extractable");
    assert_eq!(
        account.native_prerequisite().unwrap().filing().sequence(),
        filing
    );
    assert_eq!(account.native_recorded_sequences().len(), 2);
    assert!(account.native_proposal().is_some());
    assert!(account.incomplete().is_none());
}

#[test]
fn place_phase_review_failed_native_account_keeps_first_failure_and_unadopted_scopes() {
    let (mut fx, old, filing) = native_recorded_before_finish();
    let first = unrelated_failure(&mut fx);
    let run = fx.run();
    // Failed reaches live()'s existing first-cause guard before the narrower
    // phase guard. This is an overlapping control, not an isolated M2/M4 probe.
    assert_eq!(
        run.context.take_place_marks(&mut run.clock).unwrap_err(),
        first
    );
    assert_eq!(
        run.context
            .take_cancelled_place_marks(&mut run.clock)
            .unwrap_err(),
        first
    );
    fx.first_fault_attempt();
    let account = fx.settled_account();
    assert_eq!(
        format!(
            "{:?}",
            fx.run().context.place_engine.as_ref().unwrap().native
        ),
        old
    );
    assert_eq!(
        account.native_prerequisite().unwrap().filing().sequence(),
        filing
    );
    assert_eq!(account.native_recorded_sequences().len(), 2);
    assert!(account.native_proposal().is_some());
    assert!(account.incomplete().is_none());
    assert_eq!(fx.run().context.failure(), Some(&first));
}

#[test]
fn place_phase_review_completed_releasing_reload_keeps_bounded_account_ownership() {
    let mut fx = PlaceFixture::new();
    let carry = fx.text(8, "retained carry");
    let prefix = fx.prefix(None, s::CutMethod::Flush);
    let n = prefix.n();
    let resident = prefix.resident_before();
    fx.begin_reload(prefix, ReloadTail::held(carry));
    let TokenDirective::MeasureDelta { keep, .. } = fx.reload_packet() else {
        panic!("actual measure")
    };
    assert_eq!(keep, n);
    fx.answer(&fx.measured(n, 3));
    assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep: n });
    let run = fx.run();
    let cut = run.context.deadlines.get(PumpDeadline::Cut).unwrap();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    let raw = fx.answer(&TokenAnswer::Flushed {
        resident_before: resident,
        resident_after: n,
    });
    let mut complete = false;
    for _ in 0..128 {
        if fx.account_step().unwrap() == PlaceProgress::Complete {
            complete = true;
            break;
        }
    }
    assert!(
        complete,
        "the real cut and every owed trace record must settle before extraction"
    );
    let records = fx.records(Kind::StatePlacement).len();
    let recorded = fx.records(Kind::StatePlacement).last().unwrap()["sequence"].clone();
    let run = fx.run();
    let owner = run.context.reload.as_ref().unwrap() as *const _;
    let identity = run
        .context
        .place_engine
        .as_ref()
        .unwrap()
        .staged
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
    assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
    assert!(
        run.context.take_reload(&mut run.clock).unwrap().is_none(),
        "ordinary live completion cannot extract a Releasing job"
    );
    assert_eq!(run.context.reload.as_ref().unwrap() as *const _, owner);
    assert!(Arc::ptr_eq(
        run.context
            .place_engine
            .as_ref()
            .unwrap()
            .staged
            .as_ref()
            .unwrap(),
        &identity
    ));
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
    assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    let account = run
        .context
        .take_place_account()
        .expect("bounded stopped extraction still owns the complete job");
    assert!(account.incomplete().is_none());
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert!(
        account.staged_evidence().unwrap().recorded.is_none(),
        "trace-only teardown retains actual records without stamping live carry authority"
    );
    assert_eq!(
        recorded,
        account.recorded_sequences().last().unwrap().0.to_string()
    );
    assert!(Arc::ptr_eq(
        &account.staged_evidence().unwrap().identity,
        &identity
    ));
    assert!(!account.recorded_sequences().is_empty());
    assert!(run.context.reload.is_none());
    assert!(run.context.place_engine.as_ref().unwrap().staged.is_none());
    assert!(run.context.failure().is_none());
    assert_eq!(fx.records(Kind::StatePlacement).len(), records);
    assert!(receive(fx.peer.as_fd()).is_none());
}
