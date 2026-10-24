//! Cleanup-only return order and static successful-Boot evidence after review #271.
use super::*;

#[path = "context_plan_summary_admission_review_tests.rs"]
mod admission_review_tests;

fn cleanup_leave(fx: &mut SummaryFixture) -> AbsoluteBound {
    AbsoluteBound::after(fx.run().clock.now().unwrap(), 100_000).unwrap()
}
fn assert_cleanup_account(
    account: &PlanSummaryAccount,
    evidence: &CompletedBootEvidence,
    cap: u64,
) {
    assert_eq!(
        account.disposition_for_test(),
        PlanSummaryDisposition {
            complete: true,
            cancelled: false,
            key: PumpDeadline::Enter,
            bound: evidence.enter,
            account_deadline: cap,
        }
    );
    assert_eq!(
        std::ptr::from_ref(account.recorded_read().unwrap()) as usize,
        evidence.read_address
    );
    assert_eq!(
        std::ptr::from_ref(account.repaired().unwrap().checked_open().unwrap()) as usize,
        evidence.open_address
    );
    assert_eq!(account.recorded_read().unwrap().raw_bytes(), evidence.raw);
    assert!(account.refusal().is_none());
    assert!(account.incomplete().is_none());
}

#[test]
fn boot_cleanup_review_failed_orphan_waits_do_not_block_exact_completed_custody() {
    for key in [
        PumpDeadline::Decision,
        PumpDeadline::Cut,
        PumpDeadline::Gate,
        PumpDeadline::StateControl,
    ] {
        let (mut fx, evidence, proof) = take_completed_boot(true);
        let leave = cleanup_leave(&mut fx);
        let run = fx.run();
        let orphan = AbsoluteBound::after(run.clock.now().unwrap(), 50_000).unwrap();
        run.context.arm(key, orphan, &mut run.clock).unwrap();
        run.context
            .arm(PumpDeadline::Leave, leave, &mut run.clock)
            .unwrap();
        let first = run.context.fail(
            &mut run.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "actual first failure with a registered wait but no work owner",
        );
        let halt = run.context.halt().unwrap().deadline().unwrap();
        let now = run.clock.now().unwrap();
        run.context
            .restore_boot_plan_summary_for_rollback(proof)
            .unwrap();
        assert_eq!(
            run.clock.now().unwrap(),
            now,
            "custody return neither samples nor advances time"
        );
        assert_eq!(run.context.phase(), RecordingPhase::Failed);
        assert_eq!(run.context.failure(), Some(&first));
        assert_eq!(run.context.deadlines.get(key), Some(orphan));
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Enter),
            Some(evidence.enter)
        );
        assert_eq!(
            run.context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Failed)
        );
        let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
        assert_cleanup_account(
            &account,
            &evidence,
            evidence
                .enter
                .deadline_ns()
                .min(leave.deadline_ns())
                .min(halt),
        );
        assert!(run.context.take_stopped_plan_summary().unwrap().is_none());
        assert_eq!(run.context.failure(), Some(&first));
        assert!(fx.records(Kind::SavePoint).is_empty());
        assert!(fx.records(Kind::TurnStarted).is_empty());
    }
}

#[test]
fn boot_cleanup_review_release_and_state_retirement_can_precede_completed_proof_return() {
    for fail_after_retirement in [false, true] {
        for retire_enter in [false, true] {
            let (mut fx, evidence, proof) = take_completed_boot(true);
            let leave = cleanup_leave(&mut fx);
            let run = fx.run();
            run.context.begin_releasing(leave, &mut run.clock).unwrap();
            assert_eq!(run.context.phase(), RecordingPhase::Releasing);
            if retire_enter {
                assert_eq!(
                    run.context.complete_wait(PumpDeadline::Enter),
                    Some(evidence.enter)
                );
                assert_eq!(run.context.retired_enter, Some(evidence.enter));
                assert_eq!(run.context.complete_wait(PumpDeadline::Enter), None);
                assert_eq!(
                    run.context.retired_enter,
                    Some(evidence.enter),
                    "no-op retirement preserves receipt"
                );
            }
            assert_eq!(
                run.context.retire_state(&mut run.clock).unwrap(),
                Advance::Progress
            );
            assert!(run.context.flow.standing_state().is_none());
            let first = fail_after_retirement.then(|| {
                run.context.fail(
                    &mut run.clock,
                    FlowHaltReason::ProtocolFailed,
                    HaltPoint::Teardown,
                    "failure after actual state retirement",
                )
            });
            if let Some(first) = first.as_ref()
                && retire_enter
            {
                let attempted = AbsoluteBound::after(
                    evidence.enter.began_ns(),
                    evidence.enter.deadline_ns() - evidence.enter.began_ns() + 1,
                )
                .unwrap();
                assert_eq!(
                    run.context
                        .arm(PumpDeadline::Enter, attempted, &mut run.clock),
                    Err(first.clone())
                );
                assert_eq!(
                    run.context.retired_enter,
                    Some(evidence.enter),
                    "failed arm cannot erase original retirement evidence"
                );
            }
            let phase = run.context.phase();
            let expected_cap = evidence.enter.deadline_ns().min(leave.deadline_ns()).min(
                run.context
                    .halt()
                    .map_or(u64::MAX, |halt| halt.deadline().unwrap()),
            );
            let now = run.clock.now().unwrap();
            run.context
                .restore_boot_plan_summary_for_rollback(proof)
                .unwrap();
            assert_eq!(run.clock.now().unwrap(), now);
            assert_eq!(run.context.phase(), phase);
            assert_eq!(run.context.failure(), first.as_ref());
            assert!(run.context.flow.standing_state().is_none());
            assert_eq!(
                run.context.deadlines.get(PumpDeadline::Enter),
                (!retire_enter).then_some(evidence.enter)
            );
            let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
            assert_cleanup_account(&account, &evidence, expected_cap);
            assert!(run.context.plan_summary.is_none());
            assert!(fx.records(Kind::SavePoint).is_empty());
            assert!(fx.records(Kind::TurnStarted).is_empty());
        }
    }
}

#[test]
fn boot_cleanup_review_failed_actual_filing_remains_a_competing_owner() {
    let (mut fx, evidence, proof) = take_completed_boot(true);
    let event = effect(fx.seed(), "authorize", None);
    let original = format!("{event:?}");
    fx.begin_file(event);
    let run = fx.run();
    let decision = run.context.deadlines.get(PumpDeadline::Decision).unwrap();
    let first = run.context.fail(
        &mut run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        "first failure while actual filing owns its original event",
    );
    let error = run
        .context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
    assert_eq!(error.failure, Some(first.clone()));
    assert_same_returned_boot(&error.prerequisite, &evidence);
    assert!(run.context.filing.is_some());
    assert!(run.context.plan_summary.is_none());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(decision)
    );
    run.context.cancel_filing();
    let cancelled = run.context.take_cancelled_filing().unwrap();
    assert_eq!(format!("{:?}", cancelled.event.unwrap()), original);
    assert!(cancelled.sequence.is_none());
    assert_eq!(run.context.failure(), Some(&first));
    // This is custody refusal, not a claim that every failed filing can be
    // discarded without its separate original accounting owner.
}

#[test]
fn boot_cleanup_review_current_or_retired_replacement_never_uses_an_older_enter_receipt() {
    for keep_replacement in [false, true] {
        let (mut fx, evidence, proof) = take_completed_boot(true);
        let replacement = AbsoluteBound::after(
            evidence.enter.began_ns(),
            evidence.enter.deadline_ns() - evidence.enter.began_ns() + 1,
        )
        .unwrap();
        let run = fx.run();
        assert_eq!(
            run.context.complete_wait(PumpDeadline::Enter),
            Some(evidence.enter)
        );
        assert_eq!(run.context.retired_enter, Some(evidence.enter));
        run.context
            .arm(PumpDeadline::Enter, replacement, &mut run.clock)
            .unwrap();
        assert!(
            run.context.retired_enter.is_none(),
            "successful re-arm invalidates prior retirement evidence"
        );
        if !keep_replacement {
            assert_eq!(
                run.context.complete_wait(PumpDeadline::Enter),
                Some(replacement)
            );
            assert_eq!(run.context.retired_enter, Some(replacement));
        }
        let leave = cleanup_leave(&mut fx);
        let run = fx.run();
        run.context.begin_releasing(leave, &mut run.clock).unwrap();
        let before = run.context.deadlines.get(PumpDeadline::Enter);
        let error = run
            .context
            .restore_boot_plan_summary_for_rollback(proof)
            .unwrap_err();
        assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
        assert!(error.failure.is_none());
        assert_same_returned_boot(&error.prerequisite, &evidence);
        assert_eq!(run.context.phase(), RecordingPhase::Releasing);
        assert_eq!(run.context.deadlines.get(PumpDeadline::Enter), before);
        assert!(run.context.plan_summary.is_none());
    }
}

#[test]
fn boot_cleanup_review_unknown_enter_absence_is_not_a_retirement_receipt() {
    let (mut fx, evidence, proof) = take_completed_boot(true);
    let leave = cleanup_leave(&mut fx);
    let run = fx.run();
    run.context.begin_releasing(leave, &mut run.clock).unwrap();
    // Defensive registry corruption only: all real production Enter removals
    // use disarm_wait/complete_wait and retain their actual returned bound.
    assert_eq!(
        run.context.deadlines.disarm(PumpDeadline::Enter),
        Some(evidence.enter)
    );
    assert!(run.context.retired_enter.is_none());
    let error = run
        .context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
    assert_same_returned_boot(&error.prerequisite, &evidence);
    assert!(run.context.plan_summary.is_none());
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
}

#[test]
fn boot_cleanup_review_summary_release_records_only_its_original_enter_retirement() {
    let (mut fx, evidence) = completed_boot(true);
    let leave = cleanup_leave(&mut fx);
    let run = fx.run();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    assert_eq!(run.context.deadlines.get(PumpDeadline::Enter), None);
    assert_eq!(run.context.retired_enter, Some(evidence.enter));
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert_cleanup_account(
        &account,
        &evidence,
        evidence.enter.deadline_ns().min(leave.deadline_ns()),
    );
}

#[test]
fn boot_cleanup_review_closed_context_returns_original_proof_and_never_reopens() {
    let (mut fx, evidence, proof) = take_completed_boot(true);
    let leave = cleanup_leave(&mut fx);
    let run = fx.run();
    run.context.begin_releasing(leave, &mut run.clock).unwrap();
    assert_eq!(
        run.context.complete_wait(PumpDeadline::Enter),
        Some(evidence.enter)
    );
    assert_eq!(
        run.context.retire_state(&mut run.clock).unwrap(),
        Advance::Progress
    );
    let (decode, _peer) = fx._decode.take().unwrap();
    assert!(fx.run().context.retire_boot_decode(decode).is_ok());
    let run = fx.run();
    assert_eq!(
        run.context.retire_classifier(&mut run.clock).unwrap(),
        Advance::Progress
    );
    run.context
        .author_and_wait(&mut ports(&mut run.clock), unload())
        .map_err(|failure| failure.retained_reason())
        .unwrap();
    fx.drain();
    let run = fx.run();
    run.context
        .close_after_drain(&mut ports(&mut run.clock))
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Closed);
    let error = run
        .context
        .restore_boot_plan_summary_for_rollback(proof)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongPhase);
    assert!(error.failure.is_none());
    assert_same_returned_boot(&error.prerequisite, &evidence);
    assert!(run.context.plan_summary.is_none());
    assert_eq!(run.context.phase(), RecordingPhase::Closed);
    // The caller must return retained cleanup proofs before final Closed.
}

#[test]
fn boot_cleanup_review_completed_summary_cannot_reenter_taken_open_cancellation() {
    let (mut fx, evidence) = completed_boot(true);
    let leave = cleanup_leave(&mut fx);
    let run = fx.run();
    assert_eq!(
        run.context.cancel_boot_open(),
        Err(RunFailure::BootOpenOwned)
    );
    assert_eq!(
        run.context.begin_boot_open_release(leave, &mut run.clock),
        Err(RunFailure::BootOpenOwned)
    );
    assert!(!run.context.boot_enter_cancelled);
    assert_eq!(run.context.phase(), RecordingPhase::Entered);
    assert_eq!(
        run.context
            .plan_summary
            .as_ref()
            .unwrap()
            .disposition_for_test(),
        PlanSummaryDisposition {
            complete: true,
            cancelled: false,
            key: PumpDeadline::Enter,
            bound: evidence.enter,
            account_deadline: evidence.enter.deadline_ns(),
        }
    );
    let Some(PlanSummaryCompletion::Boot(proof)) =
        run.context.take_plan_summary(&mut run.clock).unwrap()
    else {
        panic!("refused duplicate cancellation cannot alter the actual successful Boot");
    };
    assert_same_returned_boot(&proof, &evidence);
}
