//! Merged #263's Text-only/empty boundary through real recorder/checker owners.
//! Scripted language replies are mechanics evidence, not artifact qualification.
use super::*;

#[path = "context_cancelled_call_count_tests.rs"]
mod call_count_tests;
use crate::flow::clock::{WaitInterest, WaitOutcome};

fn count(fx: &RepairFixture) -> u32 {
    fx.input.run.context.repair_refusal_inspection().0.unwrap()
}

fn begin_pass(fx: &mut RepairFixture, content: Vec<ContentBlock>) {
    fx.input
        .run
        .context
        .begin_output(original(content), None, &mut fx.input.run.clock)
        .unwrap();
}

fn output_step(fx: &mut RepairFixture) -> OutputProgress {
    fx.input
        .run
        .context
        .advance_output(&mut ports(&mut fx.input.run.clock))
        .unwrap()
}

fn output_loan(fx: &mut RepairFixture, file: bool) -> OriginalRequests {
    for _ in 0..512 {
        let progress = output_step(fx);
        fx.checker();
        fx.member(file);
        if progress == OutputProgress::Complete {
            let checked = fx
                .input
                .run
                .context
                .take_output(&mut fx.input.run.clock)
                .unwrap()
                .unwrap();
            return OriginalRequests::from_checked(checked, &fx.scheme).unwrap();
        }
    }
    panic!("whole output checks did not complete");
}

fn cancel(fx: &mut RepairFixture, cause: s::NonDispatchCause) {
    fx.input
        .run
        .context
        .cancel_repair(cause, &mut fx.input.run.clock)
        .unwrap();
}

fn cancelled(fx: &mut RepairFixture, file: bool) -> RepairCancelled {
    for _ in 0..512 {
        fx.step(file);
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
    panic!("cancelled repair did not return its account");
}

fn no_continuation(fx: &mut RepairFixture) {
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
    assert!(
        fx.input
            .run
            .context
            .take_repair_exhausted(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.repair_records("repair_exhausted").is_empty());
}

fn at_attempt(prior: u32) -> (RepairFixture, repair::RestatementRequest) {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let mut request = fx.restatement(true);
    if prior != 0 {
        let first = fx.generation(vec![text("bad first correction")], true);
        fx.observe(request.source(), first);
        request = fx.restatement(true);
    }
    assert_eq!(count(&fx), prior);
    assert_eq!(request.attempt().0, u64::from(prior) + 1);
    fx.arm(); // The next correction starts its actual Decision window before cancellation.
    (fx, request)
}

#[test]
fn cancelled_text_count_precedes_classifier_release_and_output_loan_below_and_at_cap() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for prior in [0, 1] {
            let (mut fx, request) = at_attempt(prior);
            let decision = fx
                .input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Decision)
                .unwrap();
            let leave = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 9000).unwrap();
            fx.input
                .run
                .context
                .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
                .unwrap();
            let before = fx.input.records(Kind::Ste100Result).len();
            begin_pass(&mut fx, vec![text("bad first"), text("bad last")]);
            cancel(&mut fx, cause);
            let mut final_recorded = false;
            let mut saw_partial = false;
            for _ in 0..512 {
                output_step(&mut fx);
                if let Some(check) = fx.receive_check() {
                    if check.text == "bad last" {
                        fx.input
                            .run
                            .context
                            .flow
                            .interrupt_classifier_terminal_probes_for_test(128);
                    }
                    fx.input.reply(&reply(&check, true));
                }
                fx.member(true);
                let recorded = fx.input.records(Kind::Ste100Result).len() - before;
                if recorded == 1 {
                    saw_partial = true;
                    assert_eq!(
                        count(&fx),
                        prior,
                        "a checked prefix is not a complete attempt"
                    );
                }
                if recorded == 2 {
                    final_recorded = true;
                    break;
                }
            }
            assert!(saw_partial && final_recorded);
            assert!(
                fx.input
                    .run
                    .context
                    .flow
                    .pending_classifier_recorded_request()
                    .is_some()
            );
            assert!(
                !fx.input
                    .run
                    .context
                    .flow
                    .output_checks()
                    .unwrap()
                    .is_complete()
            );
            assert_eq!(
                count(&fx),
                prior + 1,
                "the last Recorded check charges before terminal release"
            );
            for _ in 0..3 {
                no_continuation(&mut fx);
                assert!(
                    fx.input
                        .run
                        .context
                        .take_output(&mut fx.input.run.clock)
                        .unwrap()
                        .is_none()
                );
                cancel(&mut fx, cause);
                assert_eq!(count(&fx), prior + 1);
            }
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Decision),
                Some(decision)
            );
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Leave),
                Some(leave)
            );
            fx.input
                .run
                .context
                .flow
                .interrupt_classifier_terminal_probes_for_test(0);
            let correction = output_loan(&mut fx, true);
            let identity = correction.generation().source().clone();
            assert_eq!(
                count(&fx),
                prior + 1,
                "a caller holding the output loan cannot defer the count"
            );
            fx.observe(request.source(), correction);
            assert_eq!(
                count(&fx),
                prior + 1,
                "returning the loan must not charge again"
            );
            let stopped = cancelled(&mut fx, true);
            let records = stopped.account().failed_corrections();
            assert_eq!(records.len(), (prior + 1) as usize);
            let last = records.last().unwrap();
            assert_eq!(last.source(), request.source());
            assert_eq!(last.correction(), &identity);
            assert_eq!(last.failed_corrections(), prior + 1);
            assert_eq!(
                fx.input.records(Kind::LoopInstruction).len(),
                (prior + 1) as usize
            );
            no_continuation(&mut fx);
            assert!(fx.input.run.context.failure().is_none());
        }
    }
}

#[test]
fn cancelled_text_count_preserves_completed_pass_before_cancellation_and_late_return() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        let (mut fx, request) = at_attempt(0);
        let correction = fx.generation(vec![text("bad complete correction")], true);
        assert_eq!(count(&fx), 1, "count exists before observe_correction");
        let identity = correction.generation().source().clone();
        cancel(&mut fx, cause);
        assert_eq!(fx.step(true), RepairProgress::Pending);
        assert!(
            fx.input
                .run
                .context
                .take_cancelled_repair(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        no_continuation(&mut fx);
        fx.observe(request.source(), correction);
        let stopped = cancelled(&mut fx, true);
        assert_eq!(stopped.account().failed_corrections().len(), 1);
        assert_eq!(
            stopped.account().failed_corrections()[0].correction(),
            &identity
        );
        no_continuation(&mut fx);
    }
}

fn recall_reply(check: &s::Ste100Request, malformed: bool) -> s::Ste100Reply {
    let mut response = reply(
        &s::Ste100Request {
            text: "Read item.".into(),
            ..check.clone()
        },
        false,
    );
    let s::Ste100Result::Accepted { bindings } = &mut response.result else {
        unreachable!()
    };
    bindings[0].verb = "recall".into();
    bindings[0].statement_span.end = (check.text.len() as u64).into();
    bindings[0].verb_span.end = 6.into();
    bindings[0].nouns[0].span.start = 7.into();
    bindings[0].nouns[0].span.end = 11.into();
    if malformed {
        bindings[0].condition = Some("if ready".into());
        bindings[0].condition_span = Some(s::Span {
            start: 12.into(),
            end: 20.into(),
        });
    }
    response
}

#[test]
fn cancelled_text_count_excludes_success_and_every_attempted_call_shape() {
    for shape in 0..5 {
        let (mut fx, request) = at_attempt(0);
        let content = match shape {
            0 => vec![text("Read accepted.")],
            1 => vec![text("bad correction"), call()],
            2 => vec![
                text("bad correction"),
                ContentBlock::UnrecoveredCall {
                    fragment: "<call".into(),
                },
            ],
            3 => vec![text("bad correction"), text("Recall item.")],
            _ => vec![text("bad correction"), text("Recall item if ready.")],
        };
        begin_pass(&mut fx, content);
        cancel(&mut fx, s::NonDispatchCause::Stop);
        let mut complete = false;
        for _ in 0..512 {
            let progress = output_step(&mut fx);
            if let Some(check) = fx.receive_check() {
                let response = if check.text.starts_with("Recall") {
                    recall_reply(&check, shape == 4)
                } else {
                    reply(&check, check.text.starts_with("bad"))
                };
                fx.input.reply(&response);
            }
            fx.member(true);
            assert_eq!(
                count(&fx),
                0,
                "success or an attempted call is not a Text-only failure"
            );
            if progress == OutputProgress::Complete {
                complete = true;
                break;
            }
        }
        assert!(complete);
        let checked = fx
            .input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .unwrap();
        let correction = OriginalRequests::from_checked(checked, &fx.scheme).unwrap();
        assert_eq!(correction.attempted_call(), shape != 0);
        fx.observe(request.source(), correction);
        assert_eq!(count(&fx), 0);
        if shape == 0 {
            assert!(
                cancelled(&mut fx, true)
                    .account()
                    .failed_corrections()
                    .is_empty()
            );
        }
        // Call cases deliberately stop before their separately owed refusal/pair
        // accounting, which the next stacked slice implements.
        no_continuation(&mut fx);
    }
}

#[test]
fn cancelled_text_count_does_not_wait_for_ordinary_accepted_text_filing() {
    let (mut fx, request) = at_attempt(0);
    // Finish the earlier instruction's ordinary filing before withholding the
    // correction's own receipt. Restatement admission itself does not wait for it.
    for _ in 0..128 {
        let advance = fx
            .input
            .run
            .context
            .reduce_flow(&mut fx.input.run.clock)
            .unwrap();
        fx.checker();
        fx.member(true);
        if advance == Advance::Idle {
            break;
        }
    }
    assert!(fx.answered.iter().all(|answered| *answered));
    begin_pass(
        &mut fx,
        vec![text("bad correction"), text("Read accepted sibling.")],
    );
    cancel(&mut fx, s::NonDispatchCause::Unload);
    let correction = output_loan(&mut fx, false);
    let identity = correction.generation().source().clone();
    assert_eq!(count(&fx), 1);
    fx.observe(request.source(), correction);
    for _ in 0..128 {
        fx.input
            .run
            .context
            .reduce_flow(&mut fx.input.run.clock)
            .unwrap();
        fx.member(false);
        if fx.filings.iter().any(|filing| {
            matches!(filing, s::StateFrame::File { filing, .. }
            if filing.block_id.sequence == identity.block_id.sequence)
        }) {
            break;
        }
    }
    let stopped = cancelled(&mut fx, false);
    assert_eq!(stopped.account().failed_corrections().len(), 1);
    assert!(
        fx.filings
            .iter()
            .zip(&fx.answered)
            .any(|(filing, answered)| {
                !answered
                    && matches!(filing, s::StateFrame::File { filing, .. }
            if filing.block_id.sequence == identity.block_id.sequence)
            }),
        "actual ordinary Text filing remains unanswered"
    );
    no_continuation(&mut fx);
}

#[test]
fn cancelled_empty_count_owes_refusal_when_cancellation_precedes_loan_return() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for prior in [0, 1] {
            let (mut fx, request) = at_attempt(prior);
            let correction = fx.generation(vec![], true);
            let identity = correction.generation().source().clone();
            let checks = fx.input.records(Kind::Ste100Result).len();
            cancel(&mut fx, cause);
            assert_eq!(count(&fx), prior);
            fx.observe(request.source(), correction);
            assert_eq!(count(&fx), prior);
            assert_eq!(fx.step(true), RepairProgress::Progress); // stages, never records the refusal
            assert_eq!(count(&fx), prior);
            assert!(
                fx.input
                    .records(Kind::Refusal)
                    .iter()
                    .all(|record| record["payload"]["case"] != "empty_correction")
            );
            let stopped = cancelled(&mut fx, true);
            let facts = stopped.account().failed_corrections();
            assert_eq!(facts.len(), (prior + 1) as usize);
            assert_eq!(facts.last().unwrap().source(), request.source());
            assert_eq!(facts.last().unwrap().correction(), &identity);
            assert_eq!(facts.last().unwrap().failed_corrections(), prior + 1);
            assert_eq!(stopped.account().empty_corrections().len(), 1);
            let refusals: Vec<_> = fx
                .input
                .records(Kind::Refusal)
                .into_iter()
                .filter(|record| record["payload"]["case"] == "empty_correction")
                .collect();
            assert_eq!(refusals.len(), 1);
            assert_eq!(
                refusals[0]["causal_parent"],
                identity.block_id.sequence.0.to_string()
            );
            assert_eq!(
                refusals[0]["payload"]["source_id"],
                serde_json::to_value(request.source()).unwrap()
            );
            assert_eq!(
                refusals[0]["payload"]["detail"],
                "correction contains no Text block or attempted call"
            );
            assert_eq!(fx.input.records(Kind::Ste100Result).len(), checks);
            no_continuation(&mut fx);
        }
    }
}

#[test]
fn cancelled_text_count_is_zero_before_decode_or_with_an_unrecorded_check() {
    for decoded in [false, true] {
        let (mut fx, _) = at_attempt(0);
        if decoded {
            begin_pass(&mut fx, vec![text("bad first"), text("bad withheld")]);
            let mut withheld = None;
            for _ in 0..512 {
                output_step(&mut fx);
                if let Some(check) = fx.receive_check() {
                    if check.text == "bad withheld" {
                        withheld = Some(check);
                        break;
                    }
                    fx.input.reply(&reply(&check, true));
                }
                fx.member(true);
            }
            assert!(
                withheld.is_some(),
                "an actual checker request remains unanswered"
            );
        }
        cancel(&mut fx, s::NonDispatchCause::Stop);
        assert_eq!(count(&fx), 0);
        no_continuation(&mut fx);
        if decoded {
            assert!(
                fx.input
                    .run
                    .context
                    .take_cancelled_repair(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none()
            );
        } else {
            assert!(
                cancelled(&mut fx, true)
                    .account()
                    .failed_corrections()
                    .is_empty()
            );
        }
    }
}

struct BrokenAfter {
    now: u64,
    remaining: usize,
}
impl PumpIo for BrokenAfter {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.remaining == 0 {
            Err(std::io::Error::other(
                "cancelled correction post-Recorded clock",
            ))
        } else {
            self.remaining -= 1;
            Ok(self.now)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("unpressured final check needs no socket wait");
    }
}

#[test]
fn cancelled_text_count_survives_clock_failure_after_the_final_recorded_check() {
    let mut witnessed = false;
    for remaining in 0..48 {
        let (mut fx, _) = at_attempt(0);
        begin_pass(&mut fx, vec![text("bad final")]);
        cancel(&mut fx, s::NonDispatchCause::Stop);
        let mut ready = false;
        for _ in 0..512 {
            output_step(&mut fx);
            if let Some(check) = fx.receive_check() {
                fx.input.reply(&reply(&check, true));
                fx.input.classifier_transport();
                ready = true;
                break;
            }
            fx.member(true);
        }
        assert!(ready);
        let before = fx.input.records(Kind::Ste100Result).len();
        let mut clock = BrokenAfter {
            now: fx.input.run.clock.now().unwrap(),
            remaining,
        };
        let mut failure = None;
        for _ in 0..16 {
            match fx.input.run.context.advance_output(&mut ports(&mut clock)) {
                Ok(_) => {}
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if fx.input.records(Kind::Ste100Result).len() != before + 1 {
            continue;
        }
        let failure = failure.expect("persistent clock failure must end continuation");
        assert_eq!(
            count(&fx),
            1,
            "Recorded evidence survives the later failing clock"
        );
        assert_eq!(fx.input.run.context.failure(), Some(&failure));
        assert_eq!(
            fx.input
                .run
                .context
                .take_output(&mut fx.input.run.clock)
                .unwrap_err(),
            failure
        );
        assert_eq!(count(&fx), 1);
        assert!(fx.repair_records("repair_exhausted").is_empty());
        witnessed = true;
        break;
    }
    assert!(
        witnessed,
        "exercise the actual Recorded-before-failure boundary"
    );
}
