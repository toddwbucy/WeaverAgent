//! Completed boot reads keep their past disposition and their rollback custody.
//! This is a primitive composition, not the still-held serving Enter adapter.
use super::*;
use crate::flow::context::plan_summary_driver::PlanSummaryDisposition;

/// Scalar and pointer witnesses only. No test rebuilds or copies a capability.
struct CompletedBootEvidence {
    open_address: usize,
    open_description: String,
    initial_sequence: Sequence,
    initial_bytes: Vec<u8>,
    recheck_sequence: Sequence,
    recheck_bytes: Vec<u8>,
    read_address: usize,
    recall_sequence: Sequence,
    raw: Vec<u8>,
    enter: AbsoluteBound,
}

fn completed_boot(populated: bool) -> (SummaryFixture, CompletedBootEvidence) {
    let mut fx = SummaryFixture::new(vec![]);
    let repaired = fx.repaired.as_ref().unwrap();
    let open = repaired.checked_open().unwrap();
    let mut evidence = CompletedBootEvidence {
        open_address: std::ptr::from_ref(open) as usize,
        open_description: format!("{open:?}"),
        initial_sequence: repaired.initial().recall_sequence(),
        initial_bytes: repaired.initial().raw_bytes().to_vec(),
        recheck_sequence: repaired.recheck().recall_sequence(),
        recheck_bytes: repaired.recheck().raw_bytes().to_vec(),
        read_address: 0,
        recall_sequence: Sequence(0),
        raw: vec![],
        enter: open.enter(),
    };
    let snapshot = populated.then(|| fx.snapshot());
    fx.begin_boot();
    evidence.raw = fx.release(snapshot);
    finish_summary_without_take(&mut fx);
    let run = fx.run();
    let read = run.context.plan_cache.committed.as_ref().unwrap().read();
    evidence.read_address = std::ptr::from_ref(read) as usize;
    evidence.recall_sequence = read.recall_sequence();
    assert_eq!(read.raw_bytes(), evidence.raw);
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
            account_deadline: evidence.enter.deadline_ns()
        }
    );
    (fx, evidence)
}

fn revoke_completed_boot(fx: &mut SummaryFixture, cause: &str) {
    if cause == "cancel" {
        let run = fx.run();
        run.context.cancel_plan_summary(&mut run.clock).unwrap();
        // Explicit cancellation must work through cache revocation without
        // manufacturing a lifecycle observation.
        assert!(!run.context.boot_enter_cancelled);
        return;
    }
    let cause_id = weaver_types::Cause { uid: 0 };
    let value = match cause {
        "stop" => weaver_types::LifecycleDirective::Stop { cause: cause_id },
        "leave" => weaver_types::LifecycleDirective::Leave {
            cause: cause_id,
            forced: true,
            rollback: true,
        },
        "join_leave" => weaver_types::LifecycleDirective::JoinLeave { cause: cause_id },
        _ => panic!("unknown completed-boot cancellation fixture"),
    };
    let envelope = directive(value);
    let (channel, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(channel).unwrap();
    send_bytes(peer.as_fd(), &serde_json::to_vec(&envelope).unwrap());
    let run = fx.run();
    let mut clock = SummaryReadyClock(&mut run.clock);
    run.context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: None,
                peer: Some(&mut admin),
                residency: None,
                dial: None,
                leave_peer: None,
            },
            gate: None,
            decode: None,
            label: None,
            io: &mut clock,
        })
        .unwrap();
    assert!(run.context.boot_enter_cancelled);
    assert_eq!(admin.take_completion().unwrap().unwrap().envelope, envelope);
    // Consumption of the observed envelope cannot restore Enter authority.
    assert!(admin.held().is_none());
}

fn completed_boot_leave(
    fx: &mut SummaryFixture,
    evidence: &CompletedBootEvidence,
    longer: bool,
) -> AbsoluteBound {
    let now = fx.run().clock.now().unwrap();
    let duration = if longer {
        evidence.enter.deadline_ns().checked_sub(now).unwrap() + 1_000
    } else {
        1_000
    };
    let leave = AbsoluteBound::after(now, duration).unwrap();
    assert_eq!(leave.deadline_ns() > evidence.enter.deadline_ns(), longer);
    leave
}

fn assert_completed_boot_account(
    fx: &mut SummaryFixture,
    evidence: &CompletedBootEvidence,
    account: &PlanSummaryAccount,
    leave: AbsoluteBound,
) {
    let run = fx.run();
    assert_eq!(
        account.disposition_for_test(),
        PlanSummaryDisposition {
            complete: true,
            cancelled: false,
            key: PumpDeadline::Enter,
            bound: evidence.enter,
            account_deadline: evidence.enter.deadline_ns().min(leave.deadline_ns())
        }
    );
    assert!(account.refusal().is_none());
    assert!(account.incomplete().is_none());
    assert!(
        account.read_evidence().is_none(),
        "completed read already left its transport owner"
    );
    assert!(account.filing().is_none());
    let read = account.recorded_read().unwrap();
    assert_eq!(std::ptr::from_ref(read) as usize, evidence.read_address);
    assert!(read.belongs_to(&run.context));
    assert_eq!(read.raw_bytes(), evidence.raw);
    assert_eq!(read.recall_sequence(), evidence.recall_sequence);
    assert_eq!(read.snapshot_id().0, 41);
    assert!(matches!(read.selector(), s::ReadSelector::PlanCurrent {}));
    let s::QueryResult::PlanCurrent { current, .. } = read.result() else {
        panic!("stopped account lost its actual plan_current result");
    };
    if let Some(snapshot) = current {
        assert_eq!(snapshot.last_source.block_id.run, "prior-run");
        assert_eq!(snapshot.last_source.block_id.sequence.0, 2);
        assert!(snapshot.last_source.turn.is_none());
    }
    let repaired = account.repaired().unwrap();
    assert!(repaired.belongs_to(&run.context));
    let open = repaired.checked_open().unwrap();
    assert_eq!(std::ptr::from_ref(open) as usize, evidence.open_address);
    assert_eq!(format!("{open:?}"), evidence.open_description);
    assert!(std::sync::Arc::ptr_eq(
        open.origin(),
        &run.context.read_origin
    ));
    assert_eq!(open.enter(), evidence.enter);
    assert_eq!(
        repaired.initial().recall_sequence(),
        evidence.initial_sequence
    );
    assert_eq!(repaired.initial().raw_bytes(), evidence.initial_bytes);
    assert_eq!(
        repaired.recheck().recall_sequence(),
        evidence.recheck_sequence
    );
    assert_eq!(repaired.recheck().raw_bytes(), evidence.recheck_bytes);
    assert!(repaired.terminals().is_empty());
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    assert!(run.context.failure().is_none());
    assert_eq!(
        run.context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Revoked)
    );
    assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
    assert!(run.context.deadlines.get(PumpDeadline::Enter).is_none());
    assert!(
        run.context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
}

#[test]
fn completed_boot_rollback_revocation_retains_exact_evidence_in_both_handoff_orders() {
    for cause in ["stop", "leave", "join_leave", "cancel"] {
        for take_first in [false, true] {
            for longer_leave in [false, true] {
                let (mut fx, evidence) = completed_boot(true);
                let recalls = fx.records(Kind::Recall).len();
                let measurements = fx.records(Kind::StateStoreMeasurement).len();
                revoke_completed_boot(&mut fx, cause);
                let run = fx.run();
                let PlanSummaryDisposition {
                    complete,
                    cancelled,
                    ..
                } = run
                    .context
                    .plan_summary
                    .as_ref()
                    .unwrap()
                    .disposition_for_test();
                assert!(complete);
                assert!(!cancelled);
                if take_first {
                    assert_eq!(
                        run.context.take_plan_summary(&mut run.clock).unwrap_err(),
                        RunFailure::PlanSummaryOwned
                    );
                }
                let frame = weaver_types::TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
                let error = run
                    .context
                    .begin_input(frame.clone(), &mut run.clock)
                    .unwrap_err();
                assert_eq!(error.frame, frame);
                assert!(error.failure.is_none());
                assert!(run.context.input.snapshot().is_none());
                assert!(run.context.deadlines.get(PumpDeadline::Gate).is_none());
                let leave = completed_boot_leave(&mut fx, &evidence, longer_leave);
                let run = fx.run();
                run.context
                    .begin_boot_plan_summary_release(leave, &mut run.clock)
                    .unwrap();
                assert_eq!(
                    run.context.take_plan_summary(&mut run.clock).unwrap_err(),
                    RunFailure::PlanSummaryOwned
                );
                let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
                assert!(run.context.take_stopped_plan_summary().unwrap().is_none());
                assert_completed_boot_account(&mut fx, &evidence, &account, leave);
                assert_eq!(fx.records(Kind::Recall).len(), recalls);
                assert_eq!(fx.records(Kind::StateStoreMeasurement).len(), measurements);
                assert!(fx.records(Kind::TurnStarted).is_empty());
                assert!(fx.records(Kind::MessageUser).is_empty());
                assert!(fx.records(Kind::SavePoint).is_empty());
            }
        }
    }
}

#[test]
fn completed_boot_rollback_release_first_preserves_success_without_a_prior_cancel() {
    let (mut fx, evidence) = completed_boot(true);
    // Completion remains owned until transfer or rollback. Its retained read
    // is populated, but the public cache cannot grant use through that owner.
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Pending)
    );
    assert!(
        fx.run()
            .context
            .plan_cache
            .committed
            .as_ref()
            .unwrap()
            .summary()
            .is_some()
    );
    let leave = completed_boot_leave(&mut fx, &evidence, false);
    let run = fx.run();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(
        run.context.take_plan_summary(&mut run.clock).unwrap_err(),
        RunFailure::PlanSummaryOwned
    );
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert_completed_boot_account(&mut fx, &evidence, &account, leave);
}

#[test]
fn completed_boot_rollback_keeps_the_original_held_leave_and_account_minimum() {
    let (mut fx, evidence) = completed_boot(true);
    revoke_completed_boot(&mut fx, "stop");
    let leave = completed_boot_leave(&mut fx, &evidence, false);
    let replacement = completed_boot_leave(&mut fx, &evidence, true);
    let run = fx.run();
    run.context
        .arm(PumpDeadline::Leave, leave, &mut run.clock)
        .unwrap();
    assert_eq!(
        run.context
            .begin_boot_plan_summary_release(replacement, &mut run.clock)
            .unwrap_err(),
        RunFailure::PlanSummaryOwned
    );
    assert_eq!(run.context.phase(), RecordingPhase::Entered);
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Enter),
        Some(evidence.enter)
    );
    assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert_completed_boot_account(&mut fx, &evidence, &account, leave);
}

#[test]
fn completed_boot_rollback_actual_stop_retires_transports_then_records_unload_and_closes() {
    let (mut fx, evidence) = completed_boot(true);
    revoke_completed_boot(&mut fx, "stop");
    let leave = completed_boot_leave(&mut fx, &evidence, false);
    let run = fx.run();
    assert_eq!(
        run.context.take_plan_summary(&mut run.clock).unwrap_err(),
        RunFailure::PlanSummaryOwned
    );
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert_completed_boot_account(&mut fx, &evidence, &account, leave);
    let (decode, _peer) = fx._decode.take().unwrap();
    assert!(fx.run().context.retire_boot_decode(decode).is_ok());
    let run = fx.run();
    // The summary rollback retired the actual state channel under its seal.
    run.context.retire_classifier(&mut run.clock).unwrap();
    let unload_sequence = run
        .context
        .author_and_wait(&mut ports(&mut run.clock), unload())
        .unwrap();
    assert!(unload_sequence.0 > evidence.recall_sequence.0);
    assert!(run.context.barrier.is_closed());
    assert_eq!(fx.records(Kind::Unload).len(), 1);
    assert!(fx.records(Kind::SavePoint).is_empty());
    fx.drain();
    let run = fx.run();
    run.context
        .close_after_drain(&mut ports(&mut run.clock))
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Closed);
    assert!(run.context.failure().is_none());
    assert_eq!(account.recorded_read().unwrap().raw_bytes(), evidence.raw);
    assert!(!account.disposition_for_test().cancelled);
    // This fixture owns no real lifecycle/residency aggregate and makes no
    // claim about Ready/Left responses or the returned-proof teardown adapter.
}

#[test]
fn completed_boot_rollback_unrevoked_control_still_transfers_the_exact_success() {
    for populated in [false, true] {
        let (mut fx, evidence) = completed_boot(populated);
        let run = fx.run();
        let Some(PlanSummaryCompletion::Boot(proof)) =
            run.context.take_plan_summary(&mut run.clock).unwrap()
        else {
            panic!("unrevoked completed boot lost its existing success handoff");
        };
        assert_eq!(
            std::ptr::from_ref(proof.read()) as usize,
            evidence.read_address
        );
        assert_eq!(
            std::ptr::from_ref(proof.repaired().checked_open().unwrap()) as usize,
            evidence.open_address
        );
        assert_eq!(proof.read().raw_bytes(), evidence.raw);
        assert_eq!(proof.summary().is_some(), populated);
        assert_eq!(
            run.context.committed_plan_summary().unwrap().is_some(),
            populated
        );
        assert!(run.context.plan_summary.is_none());
        assert_eq!(
            run.context.take_stopped_plan_summary().unwrap_err(),
            RunFailure::PlanSummaryOwned
        );
    }
}

#[test]
fn completed_boot_rollback_unfinished_control_keeps_its_actual_cancelled_disposition() {
    for after_release in [false, true] {
        let mut fx = SummaryFixture::new(vec![]);
        fx.begin_boot();
        let raw = after_release.then(|| fx.release(Some(fx.snapshot())));
        let run = fx.run();
        run.context.cancel_plan_summary(&mut run.clock).unwrap();
        finish_summary_without_take(&mut fx);
        let account = rollback_boot_summary(&mut fx);
        assert!(account.disposition_for_test().cancelled);
        assert_eq!(
            account.recorded_read().map(|read| read.raw_bytes()),
            raw.as_deref()
        );
        assert!(account.refusal().is_none());
        assert!(account.incomplete().is_none());
    }
}

#[path = "context_plan_summary_returned_boot_tests.rs"]
mod returned_boot_tests;
