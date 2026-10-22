//! Review refinements use actual read/filing ownership. Only receipt-predicate
//! inspection copies and the named prior-Open prerequisite are synthetic.

use super::*;

fn answered_terminal() -> (RepairFixture, AbsoluteBound) {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.begin();
    fx.until_held(Hold::Terminal(1));
    let bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    fx.answer(index).unwrap();
    (fx, bound)
}

fn record_terminal(fx: &mut RepairFixture) -> usize {
    for steps in 1..=64 {
        let run = fx.run();
        run.context.reduce_flow(&mut run.clock).unwrap();
        if run.context.flow.filing_recorded_ns().is_some() {
            return steps;
        }
    }
    panic!("actual terminal receipt never became Recorded");
}

#[test]
fn boot_pairs_receipt_association_defenses_inspect_real_committed_values() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    let context = &fx.run().context;
    let session = context.flow.state().identity().0.to_owned();
    let run = context.flow.run().to_owned();
    let scheme = context.declaration.scheme.scheme_id.clone();
    fx.begin();
    let BootPairsCompletion::Repaired(proof) = fx.complete() else {
        panic!("no actual repair proof");
    };
    boot_pair_driver::validation_tests::receipt_associations(
        &proof.terminals()[0],
        &session,
        &run,
        &scheme,
    );
    assert!(proof.terminals()[0].receipts().is_complete());
    assert_eq!(fx.terminals().len(), 1);
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn boot_pairs_terminal_window_keeps_exact_enter_cap_and_expiry_attribution() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.begin();
    fx.until_held(Hold::Release(1));
    let enter = fx.run().context.deadlines.get(PumpDeadline::Enter).unwrap();
    fx.run()
        .clock
        .advance_to(enter.deadline_ns() - 100)
        .unwrap();
    let release = fx.answered.iter().position(|answered| !answered).unwrap();
    fx.answer(release).unwrap();
    // Stop as soon as the initial read installs the terminal's actual wait.
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx
            .run()
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_some()
        {
            break;
        }
    }
    let bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    assert_eq!(bound.deadline_ns(), enter.deadline_ns());
    assert!(bound.began_ns() >= enter.deadline_ns() - 100);
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Enter),
        Some(enter)
    );
    assert!(fx.terminals().is_empty());
    fx.run().clock.advance_to(enter.deadline_ns()).unwrap();
    assert_eq!(
        fx.advance().unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Enter,
            bound: enter,
        }
    );
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(fx.run().context.failure().is_none());
    assert!(fx.records(Kind::ToolCallCompleted).is_empty());
}

#[test]
fn boot_pairs_receipt_recorded_at_or_after_bound_does_not_retire_decision() {
    for late_by in [0, 1] {
        let (mut fx, bound) = answered_terminal();
        let source = fx.terminals()[0].block_id.clone();
        // Wait for this actual source's member receipt to produce a terminal
        // candidate. Its Filed observation has not yet been Recorded. No step
        // count or relative scheduling from another fixture chooses this edge.
        for _ in 0..64 {
            let run = fx.run();
            if run
                .context
                .flow
                .ledger()
                .terminal_candidate(&source)
                .is_some()
            {
                break;
            }
            run.context.reduce_flow(&mut run.clock).unwrap();
        }
        let context = &fx.run().context;
        let candidate = context.flow.ledger().terminal_candidate(&source).unwrap();
        assert_eq!(
            candidate.outcome,
            crate::flow::ledger::TerminalOutcome::Filed
        );
        assert!(!context.flow.has_pending_record());
        assert!(context.barrier.pending().is_none());
        assert!(fx.run().context.flow.filing_recorded_ns().is_none());
        // The outer admission sample passes. The bounded recorder work then
        // crosses the deadline and produces a real, late Recorded receipt.
        let run = fx.run();
        let mut clock = CrossingClock {
            clock: &mut run.clock,
            remaining: 1,
            deadline: bound.deadline_ns() + late_by,
        };
        assert!(matches!(
            run.context.reduce_flow(&mut clock),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        assert_eq!(
            run.context.flow.filing_recorded_ns(),
            Some(bound.deadline_ns() + late_by)
        );
        assert!(run.context.flow.filing_receipts().unwrap().is_complete());
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert!(
            !run.context
                .owned_filing_recorded_before(receipt_driver::FilingOwner::BootPairs, bound)
        );
        assert!(run.context.filing.is_some());
        assert!(run.context.boot_pairs.is_some());
        assert!(run.context.take_boot_pairs(&mut run.clock).is_err());
        assert_eq!(fx.terminals().len(), 1);
        assert_eq!(fx.queries(), 1);
    }
}

#[test]
fn boot_pairs_cancel_between_terminals_settles_without_a_later_terminal_wait() {
    let mut fx = RepairFixture::new(vec![old_pair("old-a"), old_pair("old-b")]);
    fx.begin();
    fx.until_held(Hold::Terminal(1));
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    fx.answer(index).unwrap();
    record_terminal(&mut fx);
    let run = fx.run();
    run.context.cancel_boot_pairs(&mut run.clock).unwrap();
    assert_eq!(fx.advance().unwrap(), BootPairsProgress::Complete);
    let run = fx.run();
    assert!(run.context.deadlines.get(PumpDeadline::Decision).is_none());
    assert!(run.context.filing.is_none());
    assert!(run.context.read.is_none());
    let Some(BootPairsCompletion::Cancelled(account)) =
        run.context.take_boot_pairs(&mut run.clock).unwrap()
    else {
        panic!("cancelled batch granted repair");
    };
    assert!(matches!(
        account.initial(),
        Some(ReadCompletion::Recorded(_))
    ));
    assert!(account.recheck().is_none());
    assert!(account.cancelled_event().is_none());
    assert_eq!(account.terminals().len(), 1);
    assert!(account.terminals()[0].receipts().is_complete());
    assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
    assert_eq!(fx.terminals().len(), 1);
    assert_eq!(fx.queries(), 1);
}

#[test]
fn boot_pairs_mixed_snapshot_keeps_committed_old_terminal_before_current_run_refusal() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run"), old_pair("run")]);
    fx.begin();
    let failure = (0..256)
        .find_map(|_| fx.step().err())
        .expect("current-run call must halt");
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        }
    ));
    assert_eq!(fx.terminals().len(), 1);
    let s::PairRecord::Ended(ended) = &fx.terminals()[0].pair else {
        panic!("terminal")
    };
    assert_eq!(ended.call_id.run, "old-run");
    assert_eq!(ended.reason, Some(s::PairFailure::AbandonedAtLoad {}));
    assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
    assert_eq!(fx.queries(), 1);
    let run = fx.run();
    assert!(run.context.filing.is_none());
    assert!(run.context.boot_pairs.is_some());
    let retained = boot_pair_driver::validation_tests::retained_terminals(&run.context);
    assert_eq!(retained.len(), 1);
    assert!(retained[0].receipts().is_complete());
    assert!(run.context.take_boot_pairs(&mut run.clock).is_err());
}

#[test]
fn boot_pairs_release_refuses_initial_between_leg_and_completed_ownership() {
    for checkpoint in 0..3 {
        let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
        fx.begin();
        if checkpoint == 1 {
            fx.until_held(Hold::Terminal(1));
            let index = fx.answered.iter().position(|answered| !answered).unwrap();
            fx.answer(index).unwrap();
            record_terminal(&mut fx);
            assert_eq!(fx.advance().unwrap(), BootPairsProgress::Pending);
        } else if checkpoint == 2 {
            fx.held = Hold::None;
            assert!((0..256).any(|_| fx.step().unwrap() == BootPairsProgress::Complete));
        }
        let requests = fx.requests.len();
        let run = fx.run();
        assert!(run.context.read.is_none() && run.context.filing.is_none());
        let enter = run.context.deadlines.get(PumpDeadline::Enter);
        let leave = AbsoluteBound::after(run.clock.now().unwrap(), 100_000).unwrap();
        assert!(
            matches!(
                run.context.begin_releasing(leave, &mut run.clock),
                Err(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                })
            ),
            "checkpoint {checkpoint}"
        );
        assert!(run.context.boot_pairs.is_some());
        assert_eq!(run.context.deadlines.get(PumpDeadline::Enter), enter);
        assert!(run.context.deadlines.get(PumpDeadline::Leave).is_none());
        assert_eq!(fx.requests.len(), requests);
    }
}

#[test]
fn boot_pairs_cancel_after_completion_keeps_read_and_terminal_evidence_without_grant() {
    for refusal in [
        None,
        Some(BootPairsRefusal::Read),
        Some(BootPairsRefusal::NonemptyRecheck),
    ] {
        let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
        if refusal == Some(BootPairsRefusal::Read) {
            fx.read_refusal = Some(2);
        } else if refusal == Some(BootPairsRefusal::NonemptyRecheck) {
            fx.final_read = fx.initial.clone();
        }
        fx.begin();
        assert!((0..256).any(|_| fx.step().unwrap() == BootPairsProgress::Complete));
        let before = fx.requests.len();
        let run = fx.run();
        run.context.cancel_boot_pairs(&mut run.clock).unwrap();
        let Some(BootPairsCompletion::Cancelled(account)) =
            run.context.take_boot_pairs(&mut run.clock).unwrap()
        else {
            panic!("cancellation returned an untaken grant");
        };
        let Some(ReadCompletion::Recorded(initial)) = account.initial() else {
            panic!("initial read")
        };
        assert_eq!(initial.raw_bytes(), fx.initial);
        assert_eq!(account.terminals().len(), 1);
        assert!(account.terminals()[0].receipts().is_complete());
        assert!(account.cancelled_event().is_none());
        assert_eq!(account.refusal(), refusal);
        if refusal == Some(BootPairsRefusal::Read) {
            assert!(matches!(
                account.recheck(),
                Some(ReadCompletion::Refused(_))
            ));
            assert!(account.read_refusal().is_some());
        } else {
            let Some(ReadCompletion::Recorded(recheck)) = account.recheck() else {
                panic!("recheck")
            };
            assert_eq!(recheck.raw_bytes(), fx.final_read);
            assert!(account.read_refusal().is_none());
        }
        assert_eq!(fx.requests.len(), before);
        assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
    }
}

#[test]
fn boot_pairs_cancelled_account_exposes_exact_unstarted_terminal() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.begin();
    for _ in 0..128 {
        fx.step().unwrap();
        if fx.run().context.filing.is_some() {
            break;
        }
    }
    let original = fx.run().context.waiting_content.as_ref().unwrap().clone();
    let run = fx.run();
    run.context.cancel_boot_pairs(&mut run.clock).unwrap();
    let BootPairsCompletion::Cancelled(account) = fx.complete() else {
        panic!("cancelled")
    };
    let retained = account.cancelled_event().expect("withdrawn original");
    assert_eq!(format!("{retained:?}"), format!("{original:?}"));
    assert!(matches!(
        account.initial(),
        Some(ReadCompletion::Recorded(_))
    ));
    assert!(account.recheck().is_none() && account.terminals().is_empty());
    assert!(fx.records(Kind::ToolCallCompleted).is_empty());
    assert!(fx.terminals().is_empty());
}

#[test]
fn boot_pairs_absent_owner_is_a_caller_defect_without_new_work() {
    for operation in 0..3 {
        let mut fx = RepairFixture::new(vec![]);
        let run = fx.run();
        let result = match operation {
            0 => run
                .context
                .advance_boot_pairs(&mut ports(&mut run.clock))
                .map(|_| ()),
            1 => run.context.cancel_boot_pairs(&mut run.clock),
            _ => run.context.take_boot_pairs(&mut run.clock).map(|_| ()),
        };
        assert!(matches!(
            result,
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            })
        ));
        assert!(run.context.boot_pairs.is_none());
        assert!(run.context.read.is_none() && run.context.filing.is_none());
        assert!(fx.requests.is_empty());
    }
}

#[test]
fn boot_pairs_first_control_busy_keeps_original_admitted_read_and_window() {
    let mut fx = RepairFixture::new(vec![]);
    fx.begin();
    let run = fx.run();
    // Normal admission checks the same synchronous Busy predicates. This
    // Two exact deferrals cover begin_read and the same step's advance_read.
    // They allocate no competing operation, ticket or request identity.
    for _ in 0..2 {
        run.context.flow.defer_control_once_for_test(
            (
                s::StateControl::Read {
                    selector: s::ReadSelector::OpenPairs {},
                },
                None,
            )
                .try_into()
                .unwrap(),
        );
    }
    assert_eq!(fx.advance().unwrap(), BootPairsProgress::Pending);
    let run = fx.run();
    let window = run
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert!(run.context.read.is_some());
    assert!(run.context.flow.pending_control().is_none());
    assert!(run.context.flow.state().outstanding().is_empty());
    assert!(run.context.deadlines.get(PumpDeadline::Decision).is_none());
    run.clock.advance_to(window.began_ns() + 17).unwrap();
    fx.until_held(Hold::Read(1));
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::StateControl),
        Some(window)
    );
    fx.held = Hold::None;
    assert!(matches!(fx.complete(), BootPairsCompletion::Repaired(_)));
    assert_eq!(fx.queries(), 2);
    assert_eq!(fx.records(Kind::Recall).len(), 3);
    assert!(fx.terminals().is_empty());
}
