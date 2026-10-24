//! Genuine pair-owner returns at both refusal-validation continuations.
use super::*;

#[test]
fn repair_accounting_review_held_original_allows_actual_correction_pair_drain() {
    let mut fx = refused_with_correction();
    fx.input
        .run
        .context
        .stage_repair_test_refusal(&mut fx.input.run.clock);
    let original = crate::flow::context::tests::event(Kind::Fault, None);
    let expected = format!("{original:?}");
    let run = &mut fx.input.run;
    let bound = run.context.deadlines.get(PumpDeadline::Decision).unwrap();
    assert_eq!(
        run.context
            .author_and_wait(&mut ports(&mut run.clock), original)
            .map_err(|failure| failure.retained_reason()),
        Err(RunFailure::RepairOwned)
    );
    let refused = finish_refused(&mut fx);
    assert!(refused.pending().is_some());
    assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    let run = &mut fx.input.run;
    assert!(run.clock.now().unwrap() < bound.deadline_ns());
    assert!(run.context.failure().is_none());
    assert_eq!(
        format!("{:?}", run.context.take_held_author().unwrap()),
        expected
    );
}

fn actual_refused_pair_return() -> (RepairFixture, OriginalRequests) {
    let mut fx = refused_with_correction();
    fx.input
        .run
        .context
        .stage_repair_test_refusal(&mut fx.input.run.clock);
    for _ in 0..512 {
        if fx.input.run.context.refused_pair_return_started_for_test() {
            break;
        }
        fx.step(true);
    }
    assert!(fx.input.run.context.refused_pair_return_started_for_test());
    assert!(!fx.input.run.context.repair_refused_pairs_settled_for_test());
    for _ in 0..512 {
        let run = &mut fx.input.run;
        if let Some(original) = run
            .context
            .advance_refused_pair_return_for_test(&mut ports(&mut run.clock))
            .unwrap()
        {
            assert_eq!(original.requests().len(), 1);
            let call = &original.requests()[0].opening().call_id;
            assert!(
                fx.input
                    .run
                    .context
                    .pairs
                    .completed(call)
                    .unwrap()
                    .is_some()
            );
            assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
            assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
            return (fx, original);
        }
        fx.checker();
        fx.member(true);
    }
    panic!("actual refused pair owner must return its completed correction");
}

#[test]
fn repair_accounting_review_immediately_complete_refused_proofs_are_validated_once() {
    let (mut fx, original) = actual_refused_pair_return();
    let source = original.generation().source().clone();
    let opening = original.requests()[0].opening().clone();
    let terminal = fx.input.records(Kind::ToolCallCompleted);
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    // Defensive scheduling of a real, fully completed owner return. The normal
    // driver uses finish_accounting immediately; this isolates the alternate
    // already-complete scan entry without setting its settled flag by hand.
    let run = &mut fx.input.run;
    assert!(
        !run.context
            .restore_refused_return_before_scan_for_test(original, &mut run.clock)
            .unwrap()
    );
    assert!(run.context.repair_refused_pairs_settled_for_test());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    let retained = run.context.repair_retained_custody_for_test().1.unwrap();
    assert_eq!(retained.generation().source(), &source);
    assert_eq!(retained.requests()[0].opening(), &opening);
    // A second lookup would now fail. The validated terminal is immutable;
    // readiness must reuse that fact, not reacquire it from a frozen ledger.
    run.context.pairs.freeze();
    let refused = finish_refused(&mut fx);
    assert_eq!(refused.pending().unwrap().generation().source(), &source);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted), terminal);
}

#[test]
fn repair_accounting_review_failed_refused_return_validation_keeps_actual_correction() {
    let (foreign, foreign_call) = completed_cause_fixture(s::NonDispatchCause::RepairExhausted);
    let wrong = foreign
        .input
        .run
        .context
        .pairs
        .completed(&foreign_call)
        .unwrap()
        .unwrap();
    let (mut fx, original) = actual_refused_pair_return();
    let source = original.generation().source().clone();
    let opening = original.requests()[0].opening().clone();
    let account = fx
        .input
        .run
        .context
        .repair_retained_custody_for_test()
        .0
        .unwrap()
        .source()
        .clone();
    let terminals = fx.input.records(Kind::ToolCallCompleted);
    let run = &mut fx.input.run;
    let bound = run.context.deadlines.get(PumpDeadline::Decision).unwrap();
    // A lawful call-id lookup cannot produce this foreign proof. Borrow a real
    // capability to exercise the exact production accounting finisher's error
    // path after the genuine pair owner has returned its original.
    let failure = run
        .context
        .finish_refused_pair_return_for_test(original, &[wrong], &mut run.clock)
        .unwrap_err();
    assert!(
        matches!(&failure, RunFailure::Flow { reason: FlowHaltReason::ProtocolFailed, detail, .. }
        if detail == "completed repair accounting: WrongAccounting")
    );
    assert_eq!(run.context.failure(), Some(&failure));
    assert!(!run.context.repair_refused_pairs_settled_for_test());
    assert!(run.clock.now().unwrap() < bound.deadline_ns());
    let (retained_account, retained_correction) = run.context.repair_retained_custody_for_test();
    assert_eq!(retained_account.unwrap().source(), &account);
    let retained =
        retained_correction.expect("validation failure cannot drop the actual returned correction");
    assert_eq!(retained.generation().source(), &source);
    assert_eq!(retained.requests()[0].opening(), &opening);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted), terminals);
}
