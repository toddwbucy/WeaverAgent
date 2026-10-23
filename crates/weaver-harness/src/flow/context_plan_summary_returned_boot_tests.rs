//! Consuming return of a previously extracted, still uninstalled boot owner.
use super::*;

fn take_completed_boot(
    populated: bool,
) -> (SummaryFixture, CompletedBootEvidence, BootPlanSummary) {
    let (mut fx, evidence) = completed_boot(populated);
    let run = fx.run();
    let Some(PlanSummaryCompletion::Boot(proof)) =
        run.context.take_plan_summary(&mut run.clock).unwrap()
    else {
        panic!("actual completed boot must transfer once before revocation");
    };
    assert!(run.context.plan_summary.is_none());
    assert_eq!(
        proof.disposition_for_test(),
        (
            true,
            false,
            PumpDeadline::Enter,
            evidence.enter,
            evidence.enter.deadline_ns()
        )
    );
    (fx, evidence, proof)
}

fn assert_same_returned_boot(proof: &BootPlanSummary, evidence: &CompletedBootEvidence) {
    assert_eq!(
        std::ptr::from_ref(proof.read()) as usize,
        evidence.read_address
    );
    assert_eq!(proof.read().raw_bytes(), evidence.raw);
    assert_eq!(proof.read().recall_sequence(), evidence.recall_sequence);
    assert_eq!(
        std::ptr::from_ref(proof.repaired().checked_open().unwrap()) as usize,
        evidence.open_address
    );
    assert_eq!(
        proof.repaired().checked_open().unwrap().enter(),
        evidence.enter
    );
}

#[test]
fn returned_boot_rollback_actual_revocation_restores_exact_owner_and_closes() {
    for cause in ["stop", "leave", "join_leave", "rollback"] {
        let (mut fx, evidence, proof) = take_completed_boot(true);
        let before = fx.records(Kind::Recall).len();
        if cause != "rollback" {
            revoke_completed_boot(&mut fx, cause);
        }
        let run = fx.run();
        run.context
            .restore_boot_plan_summary_for_rollback(proof)
            .unwrap();
        assert_eq!(
            run.context.take_plan_summary(&mut run.clock).unwrap_err(),
            RunFailure::PlanSummaryOwned
        );
        let frame = weaver_types::TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
        let refused = run
            .context
            .begin_input(frame.clone(), &mut run.clock)
            .unwrap_err();
        assert_eq!(refused.frame, frame);
        assert!(refused.failure.is_none());
        let leave = completed_boot_leave(&mut fx, &evidence, false);
        let run = fx.run();
        run.context
            .begin_boot_plan_summary_release(leave, &mut run.clock)
            .unwrap();
        let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
        assert_completed_boot_account(&mut fx, &evidence, &account, leave);
        assert_eq!(fx.records(Kind::Recall).len(), before);
        let (decode, _peer) = fx._decode.take().unwrap();
        assert!(fx.run().context.retire_boot_decode(decode).is_ok());
        let run = fx.run();
        run.context.retire_classifier(&mut run.clock).unwrap();
        run.context
            .author_and_wait(&mut ports(&mut run.clock), unload())
            .unwrap();
        fx.drain();
        let run = fx.run();
        run.context
            .close_after_drain(&mut ports(&mut run.clock))
            .unwrap();
        assert_eq!(run.context.phase(), RecordingPhase::Closed);
        assert_eq!(account.recorded_read().unwrap().raw_bytes(), evidence.raw);
        assert!(fx.records(Kind::SavePoint).is_empty());
        assert!(fx.records(Kind::TurnStarted).is_empty());
        assert!(fx.records(Kind::MessageUser).is_empty());
    }
}

#[test]
fn returned_boot_rollback_foreign_context_returns_the_exact_original_without_admission() {
    let (mut owner, evidence, proof) = take_completed_boot(true);
    let mut foreign = SummaryFixture::new(vec![]);
    foreign.boot(None);
    assert_eq!(
        owner.run().context.barrier.run(),
        foreign.run().context.barrier.run()
    );
    assert!(!proof.read().belongs_to(&foreign.run().context));
    let before = foreign.records(Kind::Recall).len();
    let error = foreign
        .run()
        .context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
    assert!(error.failure.is_none());
    assert_same_returned_boot(&error.prerequisite, &evidence);
    assert!(foreign.run().context.plan_summary.is_none());
    assert!(foreign.run().context.committed_plan_summary().is_ok());
    assert_eq!(foreign.records(Kind::Recall).len(), before);
    owner
        .run()
        .context
        .restore_boot_plan_summary_for_rollback(error.prerequisite)
        .unwrap();
    let account = rollback_boot_summary(&mut owner);
    assert_eq!(account.recorded_read().unwrap().raw_bytes(), evidence.raw);
}

#[test]
fn returned_boot_rollback_changed_cache_rejects_old_actual_proof_without_reconstruction() {
    let (mut fx, evidence, proof) = take_completed_boot(true);
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    let mut snapshot = fx.snapshot();
    snapshot.last_source = committed_position(&filing);
    fx.begin_effect(filing);
    let new_raw = fx.release(Some(snapshot));
    let PlanSummaryCompletion::Effect(updated) = fx.finish() else {
        panic!("actual later effect must update the committed summary");
    };
    assert_eq!(updated.read().raw_bytes(), new_raw);
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_some()
    );
    let before = fx.records(Kind::Recall).len();
    let error = fx
        .run()
        .context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
    assert!(error.failure.is_none());
    assert_same_returned_boot(&error.prerequisite, &evidence);
    let run = fx.run();
    assert_eq!(
        run.context.committed_plan_summary().unwrap(),
        updated.summary()
    );
    assert!(run.context.plan_summary.is_none());
    assert_eq!(fx.records(Kind::Recall).len(), before);
    // This already-progressed context needs its serving/placement teardown
    // owner. Returning an old Boot proof cannot roll it back over later work.
}

#[test]
fn returned_boot_rollback_missing_or_replaced_enter_returns_original_then_accepts_exact_bound() {
    let (mut fx, evidence, mut proof) = take_completed_boot(true);
    let run = fx.run();
    assert_eq!(
        run.context.complete_wait(PumpDeadline::Enter),
        Some(evidence.enter)
    );
    for replacement in [
        None,
        Some(
            AbsoluteBound::after(
                evidence.enter.began_ns(),
                evidence.enter.deadline_ns() - evidence.enter.began_ns() + 1,
            )
            .unwrap(),
        ),
    ] {
        if let Some(bound) = replacement {
            run.context
                .arm(PumpDeadline::Enter, bound, &mut run.clock)
                .unwrap();
        }
        let error = run
            .context
            .restore_boot_plan_summary_for_rollback(proof)
            .unwrap_err();
        assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
        assert!(error.failure.is_none());
        assert_same_returned_boot(&error.prerequisite, &evidence);
        assert!(run.context.plan_summary.is_none());
        assert_eq!(run.context.deadlines.get(PumpDeadline::Enter), replacement);
        proof = error.prerequisite;
        if replacement.is_some() {
            assert_eq!(run.context.complete_wait(PumpDeadline::Enter), replacement);
        }
    }
    run.context
        .arm(PumpDeadline::Enter, evidence.enter, &mut run.clock)
        .unwrap();
    run.context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap();
    assert_eq!(
        run.context
            .plan_summary
            .as_ref()
            .unwrap()
            .disposition_for_test()
            .3,
        evidence.enter
    );
    let account = rollback_boot_summary(&mut fx);
    assert_eq!(account.recorded_read().unwrap().raw_bytes(), evidence.raw);
}

#[test]
fn returned_boot_rollback_competing_actual_filing_keeps_both_owned_inputs() {
    let (mut fx, evidence, proof) = take_completed_boot(true);
    let event = effect(fx.seed(), "authorize", None);
    let expected = format!("{event:?}");
    fx.begin_file(event);
    let run = fx.run();
    let decision = run.context.deadlines.get(PumpDeadline::Decision).unwrap();
    let error = run
        .context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert_same_returned_boot(&error.prerequisite, &evidence);
    assert!(run.context.filing.is_some());
    assert!(run.context.plan_summary.is_none());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(decision)
    );
    run.context.cancel_filing();
    let cancelled = run.context.take_cancelled_filing().unwrap();
    assert_eq!(format!("{:?}", cancelled.event.unwrap()), expected);
    assert!(cancelled.sequence.is_none());
    // Retained originals, not fabricated empty owner state, pin the refusal.
}

#[test]
fn returned_boot_rollback_expired_enter_retains_exact_completed_evidence_without_time_renewal() {
    let (mut fx, evidence, proof) = take_completed_boot(true);
    revoke_completed_boot(&mut fx, "stop");
    fx.run()
        .clock
        .advance_to(evidence.enter.deadline_ns())
        .unwrap();
    let run = fx.run();
    run.context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap();
    assert_eq!(
        run.context
            .plan_summary
            .as_ref()
            .unwrap()
            .disposition_for_test()
            .4,
        evidence.enter.deadline_ns()
    );
    let leave = completed_boot_leave(&mut fx, &evidence, true);
    let run = fx.run();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert_completed_boot_account(&mut fx, &evidence, &account, leave);
    assert_eq!(
        account.disposition_for_test().4,
        evidence.enter.deadline_ns()
    );
}

#[test]
fn returned_boot_rollback_after_first_failure_freezes_original_leave_and_halt_minimum() {
    for short_leave in [false, true] {
        let mut fx = SummaryFixture::with_enter(300_000_000_000);
        let snapshot = fx.snapshot();
        let proof = fx.boot(Some(snapshot));
        let read_address = std::ptr::from_ref(proof.read()) as usize;
        let raw = proof.read().raw_bytes().to_vec();
        let original = proof.repaired().checked_open().unwrap().enter();
        let disposition = proof.disposition_for_test();
        let run = fx.run();
        let now = run.clock.now().unwrap();
        let leave =
            AbsoluteBound::after(now, if short_leave { 1_000 } else { 400_000_000_000 }).unwrap();
        run.context
            .arm(PumpDeadline::Leave, leave, &mut run.clock)
            .unwrap();
        let first = run.context.fail(
            &mut run.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "independent failed Enter prerequisite after Boot transfer",
        );
        let halt = run.context.halt().unwrap().deadline().unwrap();
        assert_eq!(halt, leave.deadline_ns().min(now + 150_000_000_000));
        run.context
            .restore_boot_plan_summary_for_rollback(proof)
            .unwrap();
        let expected = disposition.4.min(leave.deadline_ns()).min(halt);
        assert_eq!(
            run.context
                .plan_summary
                .as_ref()
                .unwrap()
                .disposition_for_test(),
            (true, false, PumpDeadline::Enter, original, expected)
        );
        assert_eq!(
            run.context.take_plan_summary(&mut run.clock).unwrap_err(),
            first
        );
        assert_eq!(
            run.context
                .advance_plan_summary_account(&mut run.clock)
                .unwrap(),
            PlanSummaryProgress::Complete
        );
        let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
        assert_eq!(
            account.disposition_for_test(),
            (true, false, PumpDeadline::Enter, original, expected)
        );
        assert_eq!(
            std::ptr::from_ref(account.recorded_read().unwrap()) as usize,
            read_address
        );
        assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
        assert!(account.incomplete().is_none());
        assert_eq!(run.context.failure(), Some(&first));
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Enter),
            Some(original)
        );
        assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
        assert_eq!(
            run.context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Failed)
        );
        // Only completed-account custody is claimed here; the existing halt
        // owner still owns its report and failed-run transport teardown.
    }
}

#[test]
fn returned_boot_rollback_extraction_keeps_a_retired_shorter_leave_in_the_original_owner() {
    let (mut fx, evidence) = completed_boot(true);
    let run = fx.run();
    let original_leave = AbsoluteBound::after(run.clock.now().unwrap(), 1_000).unwrap();
    run.context
        .arm(PumpDeadline::Leave, original_leave, &mut run.clock)
        .unwrap();
    let Some(PlanSummaryCompletion::Boot(proof)) =
        run.context.take_plan_summary(&mut run.clock).unwrap()
    else {
        panic!("held Leave alone does not invent an observed lifecycle Stop");
    };
    assert_eq!(proof.disposition_for_test().4, original_leave.deadline_ns());
    assert_eq!(
        run.context.complete_wait(PumpDeadline::Leave),
        Some(original_leave)
    );
    let replacement = AbsoluteBound::after(run.clock.now().unwrap(), 2_000).unwrap();
    run.context
        .arm(PumpDeadline::Leave, replacement, &mut run.clock)
        .unwrap();
    run.context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap();
    assert_eq!(
        run.context
            .plan_summary
            .as_ref()
            .unwrap()
            .disposition_for_test()
            .4,
        original_leave.deadline_ns()
    );
    run.context
        .begin_boot_plan_summary_release(replacement, &mut run.clock)
        .unwrap();
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert_eq!(
        account.disposition_for_test(),
        (
            true,
            false,
            PumpDeadline::Enter,
            evidence.enter,
            original_leave.deadline_ns()
        )
    );
    assert_eq!(
        std::ptr::from_ref(account.recorded_read().unwrap()) as usize,
        evidence.read_address
    );
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Leave),
        Some(replacement)
    );
}
