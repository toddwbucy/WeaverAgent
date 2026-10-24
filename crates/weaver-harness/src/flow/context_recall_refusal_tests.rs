//! RECALL-REFUSAL through actual checker, member and recorder ports.
use super::*;

fn checked_reply(request: &s::Ste100Request) -> s::Ste100Reply {
    if !request.text.starts_with("Recall ") {
        return reply(
            request,
            request.text.starts_with("bad") || request.text.is_empty(),
        );
    }
    let span = |start: u64, end: u64| s::Span {
        start: start.into(),
        end: end.into(),
    };
    let (nouns, condition, condition_span) = match request.text.as_str() {
        "Recall first and second." => (
            vec![
                s::NounBinding {
                    value: "first".into(),
                    span: span(7, 12),
                },
                s::NounBinding {
                    value: "second".into(),
                    span: span(17, 23),
                },
            ],
            None,
            None,
        ),
        "Recall item if ready." => (
            vec![s::NounBinding {
                value: "item".into(),
                span: span(7, 11),
            }],
            Some("if ready".into()),
            Some(span(12, 20)),
        ),
        "Recall valid." => (
            vec![s::NounBinding {
                value: "valid".into(),
                span: span(7, 12),
            }],
            None,
            None,
        ),
        _ => panic!("unknown synthetic recall fixture"),
    };
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::Accepted {
            bindings: vec![s::Binding {
                statement_span: span(0, request.text.len() as u64),
                verb: "recall".into(),
                verb_span: span(0, 6),
                condition,
                condition_span,
                nouns,
            }],
        },
    }
}

pub(super) fn generation(fx: &mut RepairFixture, content: Vec<ContentBlock>) -> OriginalRequests {
    fx.arm();
    fx.input
        .run
        .context
        .begin_output(original(content), None, &mut fx.input.run.clock)
        .unwrap();
    for _ in 0..512 {
        let progress = fx
            .input
            .run
            .context
            .advance_output(&mut ports(&mut fx.input.run.clock))
            .unwrap();
        if let Some(request) = fx.receive_check() {
            fx.input.reply(&checked_reply(&request));
        }
        fx.member(true);
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
    panic!("recall generation did not finish its recorded checks");
}

fn refusal_records(fx: &RepairFixture) -> Vec<serde_json::Value> {
    fx.input
        .records(Kind::Refusal)
        .into_iter()
        .filter(|event| event["payload"]["seam"] == "recall_request")
        .collect()
}

fn assert_account(
    fx: &RepairFixture,
    account: &repair::RepairAccount,
    expected: &[s::RecallRequestRefusal],
) {
    let records = refusal_records(fx);
    assert_eq!(records.len(), expected.len());
    assert_eq!(account.recall_refusals().len(), expected.len());
    for ((event, retained), expected) in records.iter().zip(account.recall_refusals()).zip(expected)
    {
        assert_eq!(retained.refusal(), expected);
        assert_eq!(retained.sequence().0, sequence(event));
        assert_eq!(event["channel"], "internal");
        assert_eq!(event["origin"], "runtime");
        assert_eq!(event["subsystem"], "harness");
        assert_eq!(event["turn"], "output-turn");
        assert_eq!(
            event["causal_parent"],
            expected.statement.block_id.sequence.0.to_string()
        );
        assert_eq!(
            event["payload"]["detail"],
            "recall requires exactly one noun and no governing condition"
        );
        assert_eq!(
            s::decode_strict::<weaver_types::RefusalRecord>(&event["payload"].to_string()).unwrap(),
            weaver_types::RefusalRecord::RecallRequest(expected.clone())
        );
        assert!(
            fx.filings
                .iter()
                .all(|frame| frame.common().run_id != expected.call_id.run
                    || match frame {
                        s::StateFrame::File { filing, .. } =>
                            filing.block_id.sequence != retained.sequence(),
                        s::StateFrame::FilePair { filing, .. } =>
                            filing.block_id.sequence != retained.sequence(),
                        _ => false,
                    }),
            "trace-only refusal must never file"
        );
    }
}

pub(super) fn expected(original: &OriginalRequests) -> Vec<s::RecallRequestRefusal> {
    original
        .refusals()
        .iter()
        .filter_map(|refusal| refusal.recall_refusal())
        .collect()
}

#[test]
fn recall_refusal_alone_keeps_accepted_text_without_feedback_or_restatement() {
    let mut fx = RepairFixture::new();
    let original = generation(&mut fx, vec![text("Recall first and second.")]);
    let expected = expected(&original);
    fx.begin(original);
    let effective = fx.effective(true);
    assert!(effective.attempted_call());
    assert!(effective.requests().is_empty());
    assert_eq!(effective.text(), "Recall first and second.");
    assert_eq!(effective.statements(), &[expected[0].statement.clone()]);
    assert_account(&fx, effective.account(), &expected);
    assert!(fx.input.records(Kind::LoopInstruction).is_empty());
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
    assert!(fx.input.records(Kind::ToolCallCompleted).is_empty());
    assert!(fx.input.run.context.failure().is_none());
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn recall_refusal_preserves_mixed_identities_corrected_external_and_effective_text() {
    let mut fx = RepairFixture::new();
    let original = generation(
        &mut fx,
        vec![
            text("bad original"),
            call(),
            text("Recall first and second."),
            text("Recall item if ready."),
            text("Recall valid."),
        ],
    );
    let original_source = original.generation().source().clone();
    let expected = expected(&original);
    assert_eq!(
        expected
            .iter()
            .map(|r| r.call_id.call.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        expected.iter().map(|r| r.call_block.0).collect::<Vec<_>>(),
        vec![2, 3]
    );
    fx.begin(original);
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let restatement = fx.restatement(true);
    let correction = fx.generation(vec![text("Read corrected.")], true);
    let corrected_source = correction.generation().source().clone();
    fx.observe(restatement.source(), correction);
    let effective = fx.effective(true);
    assert_eq!(effective.source(), &original_source);
    assert!(effective.attempted_call());
    assert_eq!(effective.requests().len(), 2);
    assert_eq!(effective.requests()[0].call_id.call.0, 0);
    assert_eq!(
        effective.requests()[0].call_id.sequence,
        original_source.block_id.sequence
    );
    assert_eq!(
        effective.requests()[0].statement.block_id,
        corrected_source.block_id
    );
    assert_eq!(effective.requests()[1].call_id.call.0, 3);
    assert_eq!(effective.statements().len(), 4);
    assert_eq!(
        effective.text(),
        "Read corrected.\nRecall first and second.\nRecall item if ready.\nRecall valid."
    );
    assert_account(&fx, effective.account(), &expected);
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
    assert!(fx.input.records(Kind::ToolCallCompleted).is_empty());
    assert!(fx.input.run.context.failure().is_none());

    // Sort accepts only valid effective requests, even when the refused statement
    // remains accepted answer material. It cannot turn its reserved ID into a call.
    let refused = &expected[0];
    let input = crate::flow::sort::OutboundInput {
        source: effective.source().clone(),
        effective,
        acts: vec![s::SortAct::Recall {
            call_id: refused.call_id.clone(),
            statement: refused.statement.clone(),
        }],
    };
    let (mut input, reason) = crate::flow::sort::identity_sort(input).unwrap_err();
    assert_eq!(reason, crate::flow::sort::OutboundRefusal::InvalidCandidate);
    input.acts = vec![s::SortAct::Answer {
        source: refused.statement.block_id.clone(),
    }];
    assert!(crate::flow::sort::identity_sort(input).is_ok());
}

fn interrupt_pending_refusal(fx: &mut RepairFixture) {
    let leave = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 5).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
        .unwrap();
    let mut waited = false;
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
                waited = true;
                break;
            }
            Err(error) => panic!("unexpected refusal wait: {error:?}"),
        }
        fx.checker();
        fx.member(false);
    }
    assert!(waited);
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    let pending = fx.input.run.context.pending_author().unwrap();
    assert_eq!(pending.event.kind, Kind::Refusal);
    assert!(refusal_records(fx).is_empty());
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn recall_refusal_retains_ticket_bound_and_all_owed_records_across_stop_and_leave() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for before_admission in [false, true] {
            let mut fx = RepairFixture::new();
            let original = generation(
                &mut fx,
                vec![
                    text("Recall first and second."),
                    text("Recall item if ready."),
                ],
            );
            let expected = expected(&original);
            fx.begin(original);
            let bound = fx
                .input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Decision)
                .unwrap();
            if !before_admission {
                fx.input.fill_trace();
                interrupt_pending_refusal(&mut fx);
                let ticket = fx.input.run.context.pending_author().unwrap().ticket;
                interrupt_pending_refusal(&mut fx);
                assert_eq!(
                    fx.input.run.context.pending_author().unwrap().ticket,
                    ticket
                );
                assert_eq!(
                    fx.input.run.context.deadlines.get(PumpDeadline::Decision),
                    Some(bound)
                );
            }
            fx.input
                .run
                .context
                .cancel_repair(cause, &mut fx.input.run.clock)
                .unwrap();
            assert!(
                fx.input
                    .run
                    .context
                    .take_cancelled_repair(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none()
            );
            fx.input.drain_trace();
            let mut cancelled = None;
            for _ in 0..512 {
                fx.step(true);
                cancelled = fx
                    .input
                    .run
                    .context
                    .take_cancelled_repair(&mut fx.input.run.clock)
                    .unwrap();
                if cancelled.is_some() {
                    break;
                }
            }
            let cancelled = cancelled.expect("cancellation must settle every owed refusal");
            assert_account(&fx, cancelled.account(), &expected);
            assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
            assert!(fx.input.records(Kind::ToolCallCompleted).is_empty());
            assert!(
                fx.input
                    .run
                    .context
                    .take_effective_emission(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none()
            );
            assert!(fx.input.run.context.failure().is_none());
        }
    }
}

#[test]
fn recall_refusal_expired_original_bound_halts_with_the_pending_event_retained() {
    let mut fx = RepairFixture::new();
    let original = generation(&mut fx, vec![text("Recall first and second.")]);
    fx.begin(original);
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    fx.input.fill_trace();
    interrupt_pending_refusal(&mut fx);
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    fx.input.run.clock.advance_to(bound.deadline_ns()).unwrap();
    let failed = fx
        .input
        .run
        .context
        .advance_repair(&mut ports(&mut fx.input.run.clock))
        .unwrap_err();
    assert!(matches!(
        failed,
        RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        }
    ));
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    assert_eq!(fx.input.run.context.repair_refusal_inspection().2.len(), 0);
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .is_err()
    );
    assert!(refusal_records(&fx).is_empty());
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
}

#[test]
fn recall_refusal_correction_counts_once_after_all_refusals_and_valid_pair_terminals() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let first = fx.restatement(true);
    let correction = generation(
        &mut fx,
        vec![
            text("Read command."),
            call(),
            text("Recall first and second."),
            text("Recall item if ready."),
            text("Recall valid."),
        ],
    );
    let correction_source = correction.generation().source().clone();
    let mut expected = expected(&correction);
    fx.observe(first.source(), correction);
    for _ in 0..64 {
        fx.step(false);
    }
    assert_eq!(fx.input.run.context.repair_refusal_inspection().0, Some(0));
    assert_eq!(refusal_records(&fx).len(), 2);
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let second = fx.restatement(true);
    assert_eq!(second.attempt().0, 2);
    let calls = fx.input.records(Kind::ToolCallStarted);
    let terminals = fx.input.records(Kind::ToolCallCompleted);
    assert_eq!(calls.len(), 2);
    assert_eq!(terminals.len(), 2);
    for record in &calls {
        assert_eq!(
            record["payload"]["call_id"]["sequence"],
            correction_source.block_id.sequence.0.to_string()
        );
        assert!(matches!(
            record["payload"]["call_id"]["call"].as_str(),
            Some("0" | "3")
        ));
    }
    for record in &terminals {
        assert_eq!(
            record["payload"]["reason"],
            serde_json::json!({"not_dispatched":{"cause":"pass_purpose"}})
        );
    }
    let correction = generation(
        &mut fx,
        vec![
            text("Recall first and second."),
            text("Recall item if ready."),
        ],
    );
    let second_source = correction.generation().source().clone();
    expected.extend(self::expected(&correction));
    fx.observe(second.source(), correction);
    let mut exhausted = None;
    for _ in 0..512 {
        fx.step(true);
        exhausted = fx
            .input
            .run
            .context
            .take_repair_exhausted(&mut fx.input.run.clock)
            .unwrap();
        if exhausted.is_some() {
            break;
        }
    }
    let exhausted = exhausted.expect("two forbidden passes exhaust exactly two attempts");
    assert_eq!(exhausted.failed_corrections().0, 2);
    let account = exhausted.into_account();
    assert_eq!(account.corrections().len(), 2);
    assert_account(&fx, &account, &expected);
    let protocol = fx
        .input
        .records(Kind::Refusal)
        .into_iter()
        .filter(|e| e["payload"]["case"] == "correction_call")
        .collect::<Vec<_>>();
    assert_eq!(protocol.len(), 2);
    assert_eq!(
        protocol[0]["causal_parent"],
        correction_source.block_id.sequence.0.to_string()
    );
    assert_eq!(
        protocol[1]["causal_parent"],
        second_source.block_id.sequence.0.to_string()
    );
    assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 2);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn recall_refusal_cancelled_correction_loan_keeps_its_own_identities_and_protocol_refusal() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = generation(
        &mut fx,
        vec![
            text("Recall first and second."),
            text("Recall item if ready."),
        ],
    );
    let source = correction.generation().source().clone();
    let expected = expected(&correction);
    fx.input
        .run
        .context
        .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
        .unwrap();
    fx.observe(request.source(), correction);
    let mut cancelled = None;
    for _ in 0..512 {
        fx.step(true);
        cancelled = fx
            .input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap();
        if cancelled.is_some() {
            break;
        }
    }
    let cancelled = cancelled.unwrap();
    assert_eq!(cancelled.pending().unwrap().generation().source(), &source);
    assert_account(&fx, cancelled.account(), &expected);
    assert!(cancelled.account().corrections().is_empty());
    let protocol = fx
        .input
        .records(Kind::Refusal)
        .into_iter()
        .filter(|e| e["payload"]["case"] == "correction_call")
        .collect::<Vec<_>>();
    assert_eq!(protocol.len(), 1);
    assert_eq!(
        protocol[0]["causal_parent"],
        source.block_id.sequence.0.to_string()
    );
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
    assert!(fx.repair_records("repair_exhausted").is_empty());
}

struct RefusalClock {
    before: u64,
    after: u64,
    remaining: usize,
}
impl PumpIo for RefusalClock {
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
        panic!("unpressured refusal makes bounded local progress");
    }
}

#[test]
fn recall_refusal_retains_recorded_receipt_before_a_recoverable_clock_wait() {
    let mut witnessed = false;
    for remaining in 0..16 {
        let mut fx = RepairFixture::new();
        let original = generation(&mut fx, vec![text("Recall first and second.")]);
        let expected = expected(&original);
        fx.begin(original);
        fx.step(true); // stage the exact event without admitting it
        let now = fx.input.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        fx.input
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
            .unwrap();
        let mut clock = RefusalClock {
            before: now,
            after: leave.deadline_ns(),
            remaining,
        };
        let result = fx.input.run.context.advance_repair(&mut ports(&mut clock));
        if matches!(
            result,
            Err(RunFailure::Wait {
                key: PumpDeadline::Leave,
                ..
            })
        ) && refusal_records(&fx).len() == 1
        {
            // Identify this fixture's actual Recorded transition, never an
            // assumed number of polls from a separate baseline execution.
            let (_, source, recorded) = fx.input.run.context.repair_refusal_inspection();
            assert!(source.is_some());
            assert_eq!(recorded.len(), 1);
            assert_eq!(recorded[0].refusal(), &expected[0]);
            assert_eq!(recorded[0].sequence().0, sequence(&refusal_records(&fx)[0]));
            fx.input.run.context.complete_wait(PumpDeadline::Leave);
            fx.input.run.clock.advance_to(leave.deadline_ns()).unwrap();
            fx.input
                .run
                .context
                .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
                .unwrap();
            let mut cancelled = None;
            for _ in 0..32 {
                fx.step(true);
                cancelled = fx
                    .input
                    .run
                    .context
                    .take_cancelled_repair(&mut fx.input.run.clock)
                    .unwrap();
                if cancelled.is_some() {
                    break;
                }
            }
            assert_account(&fx, cancelled.unwrap().account(), &expected);
            witnessed = true;
            break;
        }
        assert!(
            result.is_ok()
                || matches!(
                    result,
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        ..
                    })
                )
        );
    }
    assert!(
        witnessed,
        "clock injection must reach Recorded before the recoverable wait"
    );
}

fn completed_refusal_wait(count: usize) -> (RepairFixture, Vec<s::RecallRequestRefusal>) {
    for remaining in 0..24 {
        let mut fx = RepairFixture::new();
        let content = ["Recall first and second.", "Recall item if ready."]
            .into_iter()
            .take(count)
            .map(text)
            .collect();
        let original = generation(&mut fx, content);
        let expected = expected(&original);
        fx.begin(original);
        fx.step(true);
        let now = fx.input.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        fx.input
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
            .unwrap();
        let mut clock = RefusalClock {
            before: now,
            after: leave.deadline_ns(),
            remaining,
        };
        let result = fx.input.run.context.advance_repair(&mut ports(&mut clock));
        let records = refusal_records(&fx);
        if matches!(
            result,
            Err(RunFailure::Wait {
                key: PumpDeadline::Leave,
                ..
            })
        ) && records.len() == 1
            && fx.input.run.context.completed_author == Some(Sequence(sequence(&records[0])))
        {
            // Match this execution's actual retained generic completion, rather
            // than assuming which clock read reaches the post-Recorded check.
            let (_, source, recorded) = fx.input.run.context.repair_refusal_inspection();
            assert_eq!(source.unwrap().block_id.sequence.0, sequence(&records[0]));
            assert_eq!(recorded.len(), 1);
            assert_eq!(recorded[0].sequence().0, sequence(&records[0]));
            assert_eq!(recorded[0].refusal(), &expected[0]);
            fx.input.run.context.complete_wait(PumpDeadline::Leave);
            fx.input.run.clock.advance_to(leave.deadline_ns()).unwrap();
            return (fx, expected);
        }
        assert!(
            result.is_ok()
                || matches!(
                    result,
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        ..
                    })
                )
        );
    }
    panic!("clock injection must retain the actual generic author completion");
}

#[test]
fn recall_refusal_post_recorded_wait_settles_all_authors_before_next_owner() {
    for count in [1, 2] {
        for cancel in [false, true] {
            let (mut fx, expected) = completed_refusal_wait(count);
            assert!(
                fx.input
                    .run
                    .context
                    .take_effective_emission(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none(),
                "even one Recorded refusal must retain its unfinished author continuation"
            );
            if cancel {
                fx.input
                    .run
                    .context
                    .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
                    .unwrap();
                let mut cancelled = None;
                for _ in 0..512 {
                    fx.step(true);
                    cancelled = fx
                        .input
                        .run
                        .context
                        .take_cancelled_repair(&mut fx.input.run.clock)
                        .unwrap();
                    if cancelled.is_some() {
                        break;
                    }
                }
                assert_account(
                    &fx,
                    cancelled.expect("all cancelled authors settle").account(),
                    &expected,
                );
            } else {
                let effective = fx.effective(true);
                assert_account(&fx, effective.account(), &expected);
                assert!(effective.requests().is_empty());
            }
            assert!(fx.input.run.context.completed_author.is_none());
            assert!(fx.input.run.context.pending_author().is_none());
            let last = sequence(refusal_records(&fx).last().unwrap());
            let next = fx.generation(vec![text("Read next.")], true);
            assert!(next.generation().source().block_id.sequence.0 > last);
            assert!(fx.input.run.context.failure().is_none());
            assert!(fx.input.run.context.completed_author.is_none());
        }
    }
}

#[test]
fn recall_refusal_post_recorded_wait_never_consumes_an_unrelated_completion() {
    let (mut fx, _) = completed_refusal_wait(1);
    let foreign = Sequence(fx.input.run.context.completed_author.unwrap().0 + 1);
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
    assert_eq!(refusal_records(&fx).len(), 1);
}
