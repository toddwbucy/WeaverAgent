//! Real owner evidence with explicitly defensive missing-resource compositions.
use super::*;

#[test]
fn summary_admission_review_held_original_does_not_stall_admitted_refresh() {
    for effect_refresh in [false, true] {
        let mut fx = SummaryFixture::new(vec![]);
        let snapshot = if effect_refresh {
            let mut snapshot = fx.snapshot();
            fx.boot(Some(snapshot.clone()));
            let filing = fx.file(effect(fx.seed(), "authorize", None), false);
            snapshot.last_source = committed_position(&filing);
            fx.begin_effect(filing);
            Some(snapshot)
        } else {
            fx.begin_boot();
            None
        };
        let original = unload();
        let Some(Payload::Unload(payload)) = &original.payload else {
            unreachable!()
        };
        let pointer = std::ptr::from_ref(payload.as_ref()) as usize;
        let expected = format!("{original:?}");
        let run = fx.run();
        let key = if effect_refresh {
            PumpDeadline::Decision
        } else {
            PumpDeadline::Enter
        };
        let bound = run.context.deadlines.get(key).unwrap();
        assert!(run.context.read.is_none());
        assert_eq!(
            run.context
                .author_and_wait(&mut ports(&mut run.clock), original)
                .map_err(|failure| failure.retained_reason()),
            Err(RunFailure::PlanSummaryOwned)
        );
        assert!(run.context.pending_author().is_none());
        if !effect_refresh {
            let leave = AbsoluteBound::after(run.clock.now().unwrap(), 100_000).unwrap();
            assert_eq!(
                run.context
                    .begin_boot_plan_summary_release(leave, &mut run.clock),
                Err(RunFailure::PlanSummaryOwned)
            );
            assert_eq!(run.context.phase(), RecordingPhase::Entered);
            assert!(run.context.deadlines.get(PumpDeadline::Leave).is_none());
            assert_eq!(run.context.deadlines.get(key), Some(bound));
        }
        // The real first internal read, Released response and cache adoption
        // must complete while the competing caller still owns its exact input.
        fx.release(snapshot);
        assert!(matches!(
            (effect_refresh, fx.finish()),
            (false, PlanSummaryCompletion::Boot(_)) | (true, PlanSummaryCompletion::Effect(_))
        ));
        let run = fx.run();
        assert!(run.context.failure().is_none());
        assert!(run.clock.now().unwrap() < bound.deadline_ns());
        let held = run.context.held_author().unwrap();
        let Some(Payload::Unload(payload)) = &held.payload else {
            unreachable!()
        };
        assert_eq!(std::ptr::from_ref(payload.as_ref()) as usize, pointer);
        assert_eq!(format!("{held:?}"), expected);
        assert_eq!(
            format!("{:?}", run.context.take_held_author().unwrap()),
            expected
        );
    }
}

#[test]
fn summary_admission_review_defensive_absent_state_cannot_start_boot_read() {
    let mut fx = SummaryFixture::new(vec![]);
    let repaired = fx.repaired.take().unwrap();
    let open = std::ptr::from_ref(repaired.checked_open().unwrap()) as usize;
    let enter = repaired.checked_open().unwrap().enter();
    let run = fx.run();
    // Ordinary retirement also changes the lifecycle phase. Detach only this
    // genuine channel to isolate absence under the otherwise lawful Entered
    // boundary; no public retirement sequence creates this composition.
    let state = run.context.flow.take_state_for_admission_test();
    let error = run
        .context
        .begin_boot_plan_summary(repaired, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert_eq!(
        std::ptr::from_ref(error.prerequisite.checked_open().unwrap()) as usize,
        open
    );
    assert_eq!(run.context.deadlines.get(PumpDeadline::Enter), Some(enter));
    assert!(!run.context.plan_cache.boot_started);
    assert!(run.context.plan_summary.is_none());
    run.context.flow.restore_state_after_admission_test(state);
    fx.repaired = Some(error.prerequisite);
    let proof = fx.boot(None);
    assert_eq!(
        std::ptr::from_ref(proof.repaired().checked_open().unwrap()) as usize,
        open
    );
    assert_eq!(proof.repaired().checked_open().unwrap().enter(), enter);
}

#[test]
fn summary_admission_review_defensive_live_engine_returns_exact_boot_custody() {
    let (mut fx, evidence, proof) = take_completed_boot(true);
    let mut other = SummaryFixture::new(vec![]);
    let installed = other.boot(None);
    let run = other.run();
    run.context
        .install_place(installed, &mut run.clock)
        .unwrap();
    let engine = run.context.place_engine.take().unwrap();
    // Installation consumes its proof, so a matching returned proof cannot
    // coexist with that engine through the production API. Transplant one
    // actual foreign engine solely to isolate the retained defense.
    let run = fx.run();
    assert!(run.context.place_engine.is_none());
    run.context.place_engine = Some(engine);
    let before = std::ptr::from_ref(run.context.place_engine.as_ref().unwrap()) as usize;
    let error = run
        .context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert_same_returned_boot(&error.prerequisite, &evidence);
    assert_eq!(
        std::ptr::from_ref(run.context.place_engine.as_ref().unwrap()) as usize,
        before
    );
    assert!(run.context.plan_summary.is_none());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Enter),
        Some(evidence.enter)
    );
    // Return the genuine engine to its original owner; neither context gets
    // a decode operation or an invented successful proof from this fixture.
    let engine = run.context.place_engine.take().unwrap();
    other.run().context.place_engine = Some(engine);
    fx.run()
        .context
        .restore_boot_plan_summary_for_rollback(error.prerequisite)
        .unwrap();
    let leave = cleanup_leave(&mut fx);
    let run = fx.run();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    let account = fx
        .run()
        .context
        .take_stopped_plan_summary()
        .unwrap()
        .unwrap();
    assert_cleanup_account(
        &account,
        &evidence,
        evidence.enter.deadline_ns().min(leave.deadline_ns()),
    );
}
