//! Recorded semantic custody, using real output, recorder and member ports.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Purpose {
    Empty,
    Association,
    Protocol,
    Recall,
}

struct ReceiptClock {
    before: u64,
    after: u64,
    remaining: usize,
}
impl PumpIo for ReceiptClock {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.remaining == 0 {
            Ok(self.after)
        } else {
            self.remaining -= 1;
            Ok(self.before)
        }
    }
    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("unpressured receipt progresses locally");
    }
}

fn setup(purpose: Purpose) -> RepairFixture {
    let mut fx = RepairFixture::new();
    let original = match purpose {
        Purpose::Recall => {
            recall_refusal_tests::generation(&mut fx, vec![text("Recall first and second.")])
        }
        Purpose::Association => fx.generation(vec![call(), call()], true),
        _ => fx.generation(vec![text("bad original")], true),
    };
    fx.begin(original);
    if matches!(purpose, Purpose::Empty | Purpose::Protocol) {
        let request = fx.restatement(true);
        let correction = fx.generation(
            if matches!(purpose, Purpose::Empty) {
                vec![]
            } else {
                vec![call()]
            },
            true,
        );
        fx.observe(request.source(), correction);
    }
    fx.step(true); // stage the actual refusal; no receipt is invented
    fx
}

fn post_recorded(purpose: Purpose) -> (RepairFixture, serde_json::Value) {
    for remaining in 0..24 {
        let mut fx = setup(purpose);
        let before_count = fx.input.records(Kind::Refusal).len();
        let now = fx.input.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        fx.input
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
            .unwrap();
        let mut clock = ReceiptClock {
            before: now,
            after: leave.deadline_ns(),
            remaining,
        };
        let result = fx.input.run.context.advance_repair(&mut ports(&mut clock));
        let records = fx.input.records(Kind::Refusal);
        if matches!(
            result,
            Err(RunFailure::Wait {
                key: PumpDeadline::Leave,
                ..
            })
        ) && records.len() == before_count + 1
            && fx.input.run.context.completed_author
                == Some(Sequence(sequence(records.last().unwrap())))
        {
            let event = records.last().unwrap().clone();
            assert_eq!(
                fx.input
                    .run
                    .context
                    .repair_refusal_inspection()
                    .1
                    .unwrap()
                    .block_id
                    .sequence
                    .0,
                sequence(&event)
            );
            fx.input.run.context.complete_wait(PumpDeadline::Leave);
            fx.input.run.clock.advance_to(leave.deadline_ns()).unwrap();
            return (fx, event);
        }
        assert!(
            result.is_ok()
                || matches!(
                    result,
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        ..
                    })
                ),
            "{purpose:?}: {result:?}"
        );
    }
    panic!("must witness actual {purpose:?} Recorded and unfinished completion");
}

fn cancelled(fx: &mut RepairFixture, cause: s::NonDispatchCause) -> RepairCancelled {
    fx.input
        .run
        .context
        .cancel_repair(cause, &mut fx.input.run.clock)
        .unwrap();
    for _ in 0..512 {
        fx.step(true);
        if let Some(account) = fx
            .input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap()
        {
            return account;
        }
    }
    panic!("cancelled author must settle without a new successful grant");
}

#[test]
fn repair_custody_all_four_receipts_survive_post_recorded_stop_and_unload() {
    for purpose in [
        Purpose::Empty,
        Purpose::Association,
        Purpose::Protocol,
        Purpose::Recall,
    ] {
        for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
            let (mut fx, event) = post_recorded(purpose);
            let seq = sequence(&event);
            // These checks happen before completion, so delaying semantics to
            // completion cannot satisfy the test even without cancellation.
            let account = fx.input.run.context.repair_recorded_account();
            match purpose {
                Purpose::Empty => {
                    assert_eq!(account.empty_corrections().len(), 1);
                    assert_eq!(
                        account.empty_corrections()[0].source().block_id.sequence.0,
                        seq
                    );
                    assert_eq!(account.empty_corrections()[0].failed_corrections(), 1);
                    assert_eq!(account.corrections().len(), 1);
                }
                Purpose::Association => {
                    assert_eq!(account.association_refusals().len(), 1);
                    assert_eq!(
                        account.association_refusals()[0]
                            .source()
                            .block_id
                            .sequence
                            .0,
                        seq
                    );
                }
                Purpose::Recall => assert_eq!(account.recall_refusals()[0].sequence().0, seq),
                Purpose::Protocol => assert_eq!(event["payload"]["case"], "correction_call"),
            }
            let retained = cancelled(&mut fx, cause);
            assert!(
                fx.input
                    .run
                    .context
                    .take_restatement(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none()
            );
            assert!(
                fx.input
                    .run
                    .context
                    .take_effective_emission(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none()
            );
            match purpose {
                Purpose::Empty => {
                    assert!(retained.pending().is_none());
                    assert_eq!(retained.account().corrections().len(), 1);
                    let receipt = &retained.account().empty_corrections()[0];
                    assert_eq!(receipt.source().block_id.sequence.0, seq);
                    assert_eq!(receipt.failed_corrections(), 1);
                    assert_eq!(
                        receipt.correction().block_id.sequence.0.to_string(),
                        event["causal_parent"]
                    );
                }
                Purpose::Association => {
                    // Only the first of two obligations was admitted. Preserve
                    // that exact partial batch, never claim or re-author two.
                    assert_eq!(retained.account().association_refusals().len(), 1);
                    let receipt = &retained.account().association_refusals()[0];
                    assert_eq!(receipt.source().block_id.sequence.0, seq);
                    assert_eq!(
                        serde_json::to_value(receipt.refusal()).unwrap(),
                        event["payload"]
                    );
                }
                Purpose::Protocol => assert!(retained.pending().unwrap().attempted_call()),
                Purpose::Recall => {
                    assert_eq!(retained.account().recall_refusals()[0].sequence().0, seq)
                }
            }
            assert_eq!(
                fx.input
                    .records(Kind::Refusal)
                    .iter()
                    .filter(
                        |record| record["payload"]["seam"] == event["payload"]["seam"]
                            && record["payload"]["case"] == event["payload"]["case"]
                    )
                    .count(),
                1
            );
            assert!(fx.input.run.context.completed_author.is_none());
            assert!(fx.input.run.context.pending_author().is_none());
            let next = fx.generation(vec![text("Read next.")], true);
            assert!(next.generation().source().block_id.sequence.0 > seq);
            assert!(fx.input.run.context.failure().is_none());
        }
    }
}

#[test]
fn repair_custody_empty_and_association_never_consume_an_unrelated_completion() {
    for purpose in [Purpose::Empty, Purpose::Association] {
        let (mut fx, event) = post_recorded(purpose);
        let foreign = Sequence(sequence(&event) + 1);
        fx.input.run.context.completed_author = Some(foreign);
        assert!(
            fx.input
                .run
                .context
                .advance_repair(&mut ports(&mut fx.input.run.clock))
                .is_err()
        );
        assert_eq!(fx.input.run.context.completed_author, Some(foreign));
        assert!(fx.input.run.context.failure().is_some());
        assert!(fx.input.run.context.repair.is_some());
        let retained = fx.input.run.context.repair_recorded_account();
        if matches!(purpose, Purpose::Empty) {
            assert_eq!(retained.empty_corrections().len(), 1);
        } else {
            assert_eq!(retained.association_refusals().len(), 1);
        }
    }
}

#[test]
fn repair_custody_staged_recall_mismatch_preserves_actual_record_without_retiring_another_id() {
    let mut fx = RepairFixture::new();
    let original = recall_refusal_tests::generation(
        &mut fx,
        vec![
            text("Recall first and second."),
            text("Recall item if ready."),
        ],
    );
    let expected = recall_refusal_tests::expected(&original);
    fx.begin(original);
    fx.step(true);
    // Deliberately reorder only the private staged identity after normal
    // construction. The real event still records the first exact refusal.
    fx.input
        .run
        .context
        .replace_staged_recall_for_test(expected[1].clone());
    assert!(
        fx.input
            .run
            .context
            .advance_repair(&mut ports(&mut fx.input.run.clock))
            .is_err()
    );
    let records = fx.input.records(Kind::Refusal);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0]["payload"],
        serde_json::to_value(weaver_types::RefusalRecord::RecallRequest(
            expected[0].clone()
        ))
        .unwrap()
    );
    let (_, source, retained) = fx.input.run.context.repair_refusal_inspection();
    assert_eq!(source.unwrap().block_id.sequence.0, sequence(&records[0]));
    assert!(
        retained.is_empty(),
        "a different staged identity cannot retire the next owed refusal"
    );
    assert!(fx.input.run.context.failure().is_some());
}

#[test]
fn repair_custody_refused_extraction_advances_owed_recall_and_protocol_records() {
    for protocol in [false, true] {
        let mut fx = RepairFixture::new();
        let original = if protocol {
            fx.generation(vec![text("bad original")], true)
        } else {
            recall_refusal_tests::generation(&mut fx, vec![text("Recall first and second.")])
        };
        fx.begin(original);
        if protocol {
            let request = fx.restatement(true);
            let correction = fx.generation(vec![call()], true);
            fx.observe(request.source(), correction);
        }
        // A defensive ordering fixture: stage a genuine operation-refusal
        // event before the already checked generation's owed refusal. There is
        // no current production caller choosing this ordering.
        fx.input
            .run
            .context
            .stage_repair_test_refusal(&mut fx.input.run.clock);
        let bound = fx.input.run.context.deadlines.get(PumpDeadline::Decision);
        for _ in 0..32 {
            fx.step(true);
            if fx.input.run.context.repair_refused() {
                break;
            }
        }
        assert!(fx.input.run.context.repair_refused());
        assert!(
            fx.input
                .run
                .context
                .take_refused_repair(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fx.input.run.context.deadlines.get(PumpDeadline::Decision),
            bound
        );
        let mut retained = None;
        for _ in 0..64 {
            let progress = fx.step(true);
            if progress == RepairProgress::Refused {
                assert_eq!(
                    fx.input.run.context.deadlines.get(PumpDeadline::Decision),
                    bound,
                    "refusal readiness retains the original wait for its caller"
                );
            }
            retained = fx
                .input
                .run
                .context
                .take_refused_repair(&mut fx.input.run.clock)
                .unwrap();
            if retained.is_some() {
                break;
            }
        }
        let retained =
            retained.expect("advance must settle owed refusal and return original Refused custody");
        assert_eq!(
            fx.input.run.context.deadlines.get(PumpDeadline::Decision),
            bound
        );
        assert!(matches!(
            retained.evidence(),
            RepairRefusalEvidence::Instruction { .. }
        ));
        if protocol {
            assert!(retained.pending().unwrap().attempted_call());
        } else {
            assert_eq!(retained.account().recall_refusals().len(), 1);
        }
        assert_eq!(fx.input.records(Kind::Refusal).len(), 2);
        assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
        assert!(
            fx.input
                .run
                .context
                .take_refused_repair(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(fx.input.run.context.completed_author.is_none());
        let next = fx.generation(vec![text("Read next.")], true);
        assert!(!next.generation().blocks().is_empty());
        assert!(fx.input.run.context.failure().is_none());
    }
}

#[test]
fn repair_custody_queued_association_keeps_ticket_and_records_one_partial_receipt_after_cancel() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for count in [1, 2] {
            let mut fx = RepairFixture::new();
            let original = fx.generation((0..count).map(|_| call()).collect(), true);
            fx.begin(original);
            fx.step(true);
            fx.input.fill_trace();
            let leave = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 5).unwrap();
            fx.input
                .run
                .context
                .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
                .unwrap();
            let mut interrupted = false;
            for _ in 0..128 {
                match fx
                    .input
                    .run
                    .context
                    .advance_repair(&mut ports(&mut fx.input.run.clock))
                {
                    Ok(_) => {}
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound,
                    }) => {
                        assert_eq!(bound, leave);
                        interrupted = true;
                        break;
                    }
                    Err(error) => panic!("association pressure: {error:?}"),
                }
                fx.checker();
                fx.member(false);
            }
            assert!(interrupted);
            fx.input.run.context.complete_wait(PumpDeadline::Leave);
            let ticket = fx.input.run.context.pending_author().unwrap().ticket;
            assert!(fx.input.records(Kind::Refusal).is_empty());
            assert!(
                fx.input
                    .run
                    .context
                    .repair_recorded_account()
                    .association_refusals()
                    .is_empty()
            );
            let bound = fx.input.run.context.deadlines.get(PumpDeadline::Decision);
            fx.input
                .run
                .context
                .cancel_repair(cause, &mut fx.input.run.clock)
                .unwrap();
            assert_eq!(
                fx.input.run.context.pending_author().unwrap().ticket,
                ticket
            );
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Decision),
                bound
            );
            fx.input.drain_trace();
            let retained = cancelled(&mut fx, cause);
            let records = fx.input.records(Kind::Refusal);
            assert_eq!(records.len(), 1);
            assert_eq!(retained.account().association_refusals().len(), 1);
            let receipt = &retained.account().association_refusals()[0];
            assert_eq!(receipt.source().block_id.sequence.0, sequence(&records[0]));
            assert_eq!(
                serde_json::to_value(receipt.refusal()).unwrap(),
                records[0]["payload"]
            );
            assert!(fx.input.run.context.completed_author.is_none());
            assert!(fx.input.run.context.pending_author().is_none());
            let next = fx.generation(vec![text("Read next.")], true);
            assert!(next.generation().source().block_id.sequence.0 > sequence(&records[0]));
            assert!(fx.input.run.context.failure().is_none());
        }
    }
}

#[test]
fn repair_custody_exhausted_extraction_cannot_retire_a_staged_author() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    for _ in 0..2 {
        let request = fx.restatement(true);
        let correction = fx.generation(vec![], true);
        fx.observe(request.source(), correction);
    }
    let mut exhausted = false;
    for _ in 0..128 {
        if fx.step(true) == RepairProgress::Exhausted {
            exhausted = true;
            break;
        }
    }
    assert!(exhausted);
    // Current ordering normally prevents this overlap. A real, not-yet-
    // admitted author pins the defensive consumer rule for future ordering.
    fx.input
        .run
        .context
        .stage_repair_test_refusal(&mut fx.input.run.clock);
    assert!(
        fx.input
            .run
            .context
            .take_repair_exhausted(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let mut refused = None;
    for _ in 0..128 {
        fx.step(true);
        refused = fx
            .input
            .run
            .context
            .take_refused_repair(&mut fx.input.run.clock)
            .unwrap();
        if refused.is_some() {
            break;
        }
    }
    assert_eq!(refused.unwrap().account().empty_corrections().len(), 2);
    assert!(fx.input.run.context.completed_author.is_none());
    let next = fx.generation(vec![text("Read next.")], true);
    assert!(!next.generation().blocks().is_empty());
}

#[test]
fn repair_custody_association_success_retains_each_exact_record_in_order() {
    let mut fx = setup(Purpose::Association);
    let effective = fx.effective(true);
    let records = fx.input.records(Kind::Refusal);
    assert_eq!(records.len(), 2);
    assert_eq!(effective.account().association_refusals().len(), 2);
    assert!(effective.requests().is_empty());
    for (receipt, event) in effective
        .account()
        .association_refusals()
        .iter()
        .zip(&records)
    {
        assert_eq!(receipt.source().block_id.sequence.0, sequence(event));
        assert_eq!(
            serde_json::to_value(receipt.refusal()).unwrap(),
            event["payload"]
        );
    }
    assert!(sequence(&records[0]) < sequence(&records[1]));
    assert!(fx.input.run.context.completed_author.is_none());
    let next = fx.generation(vec![text("Read next.")], true);
    assert!(next.generation().source().block_id.sequence.0 > sequence(&records[1]));
}

#[path = "context_repair_review_tests.rs"]
mod review_tests;
