//! PR 261 refusal-account custody. Defensive overlap fixtures name their scope.
use super::*;

fn finish_refused(fx: &mut RepairFixture) -> RepairRefused {
    for _ in 0..512 {
        if fx.step(true) == RepairProgress::Refused {
            assert!(
                fx.input
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .is_none()
            );
            // Readiness is stable; another poll cannot renew the original wait.
            assert_eq!(fx.step(true), RepairProgress::Refused);
            assert!(
                fx.input
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .is_none()
            );
            return fx
                .input
                .run
                .context
                .take_refused_repair(&mut fx.input.run.clock)
                .unwrap()
                .unwrap();
        }
    }
    panic!("refusal did not settle its admitted obligations");
}

#[test]
fn repair_review_refused_correction_waits_for_actual_pass_purpose_terminal_receipt() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = fx.generation(vec![text("Read forbidden."), call()], false);
    let opening = correction.requests()[0].opening().clone();
    let correction_source = correction.generation().source().clone();
    fx.observe(request.source(), correction);
    // No production caller currently selects this overlap: keep the true
    // correction and stage only the operation-refusal author ahead of its debt.
    fx.input
        .run
        .context
        .stage_repair_test_refusal(&mut fx.input.run.clock);
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let mut opening_pending = false;
    for _ in 0..512 {
        fx.step(false);
        assert!(
            fx.input
                .run
                .context
                .take_refused_repair(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        opening_pending = fx.filings.iter().zip(&fx.answered).any(|(frame, answered)| !answered && matches!(frame,
            s::StateFrame::FilePair { filing, .. } if matches!(&filing.pair, s::PairRecord::Started(start) if start.call_id == opening.call_id)));
        if opening_pending {
            break;
        }
        if fx
            .filings
            .iter()
            .zip(&fx.answered)
            .any(|(frame, answered)| !answered && matches!(frame, s::StateFrame::File { .. }))
        {
            fx.member(true);
        }
    }
    assert!(
        opening_pending,
        "accepted forbidden send must reach its real pair opening"
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .completed_pair(&opening.call_id, &mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.member(true);
    let mut terminal_pending = false;
    for _ in 0..512 {
        fx.step(false);
        terminal_pending = fx.filings.iter().zip(&fx.answered).any(|(frame, answered)| !answered && matches!(frame,
            s::StateFrame::FilePair { filing, .. } if matches!(&filing.pair, s::PairRecord::Ended(end) if end.call_id == opening.call_id)));
        if terminal_pending {
            break;
        }
    }
    assert!(
        terminal_pending,
        "actual terminal must wait for its own member receipt"
    );
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
        Some(bound)
    );
    let retained = finish_refused(&mut fx);
    let pending = retained
        .pending()
        .expect("completed correction remains exact owned evidence");
    assert_eq!(pending.generation().source(), &correction_source);
    assert_eq!(pending.requests()[0].opening(), &opening);
    let completed = fx
        .input
        .run
        .context
        .completed_pair(&opening.call_id, &mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(completed.started(), &opening);
    assert_eq!(completed.ended().fate, s::TerminalPairFate::Failed);
    assert_eq!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::PassPurpose
        })
    );
    assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    assert_eq!(fx.input.records(Kind::Refusal).len(), 2);
    assert!(fx.input.run.context.completed_author.is_none());
    assert!(fx.input.run.context.failure().is_none());
    let next = fx.generation(vec![text("Read next.")], true);
    assert!(!next.generation().blocks().is_empty());
}

#[test]
fn repair_review_structural_refusal_keeps_exact_evidence_through_stop_and_unload() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for after_recorded in [false, true] {
            let mut fx = RepairFixture::new();
            let original =
                recall_refusal_tests::generation(&mut fx, vec![text("Recall first and second.")]);
            let original_source = original.generation().source().clone();
            fx.begin(original);
            // Defensive ordering only; the real invalid recall remains owed.
            fx.input
                .run
                .context
                .stage_repair_test_refusal(&mut fx.input.run.clock);
            if after_recorded {
                for _ in 0..64 {
                    fx.step(true);
                    if fx.input.run.context.repair_refused() {
                        break;
                    }
                }
                assert!(fx.input.run.context.repair_refused());
                assert_eq!(fx.input.records(Kind::Refusal).len(), 1);
            }
            let retained = cancelled(&mut fx, cause);
            let RepairRefusalEvidence::Instruction { source, record } = retained
                .refusal_evidence()
                .expect("cancellation must retain the operation refusal")
            else {
                panic!("structural refusal changed category");
            };
            let records = fx.input.records(Kind::Refusal);
            assert_eq!(records.len(), 2);
            assert_eq!(source.block_id.sequence.0, sequence(&records[0]));
            assert_eq!(serde_json::to_value(record).unwrap(), records[0]["payload"]);
            assert_eq!(
                retained.account().original().generation().source(),
                &original_source
            );
            assert_eq!(retained.account().recall_refusals().len(), 1);
            assert_eq!(
                retained.account().recall_refusals()[0].sequence().0,
                sequence(&records[1])
            );
            assert!(
                fx.input
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .is_none()
            );
            assert!(
                fx.input
                    .run
                    .context
                    .take_refused_repair(&mut fx.input.run.clock)
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
            assert!(fx.input.run.context.completed_author.is_none());
            assert!(fx.input.run.context.failure().is_none());
            let next = fx.generation(vec![text("Read next.")], true);
            assert!(!next.generation().blocks().is_empty());
        }
    }
}

#[test]
fn repair_review_runtime_template_check_and_fault_survive_cancel_before_and_after_recorded() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for after_recorded in [false, true] {
            let mut fx = RepairFixture::new();
            let original = fx.generation(vec![text("bad original")], true);
            fx.begin(original);
            let mut instruction = None;
            for _ in 0..512 {
                fx.input
                    .run
                    .context
                    .advance_repair(&mut ports(&mut fx.input.run.clock))
                    .unwrap();
                if let Some(request) = fx.receive_check() {
                    if request.text == "Read correction." {
                        instruction = Some(request);
                        break;
                    }
                    fx.input
                        .reply(&reply(&request, request.text.starts_with("bad")));
                }
                fx.member(true);
            }
            let instruction =
                instruction.expect("actual selected instruction must reach the classifier");
            if !after_recorded {
                fx.input
                    .run
                    .context
                    .cancel_repair(cause, &mut fx.input.run.clock)
                    .unwrap();
            }
            let rejection = reply(&instruction, true);
            fx.input.reply(&rejection);
            if after_recorded {
                for _ in 0..64 {
                    fx.step(true);
                    if !fx.input.records(Kind::Fault).is_empty() {
                        break;
                    }
                }
                assert_eq!(fx.input.records(Kind::Fault).len(), 1);
            }
            let retained = cancelled(&mut fx, cause);
            let RepairRefusalEvidence::RuntimeTemplate { fault, check } = retained
                .refusal_evidence()
                .expect("run account must own its actual classifier rejection")
            else {
                panic!("template refusal changed category");
            };
            let faults = fx.input.records(Kind::Fault);
            assert_eq!(faults.len(), 1);
            assert_eq!(fault.block_id.sequence.0, sequence(&faults[0]));
            assert_eq!(check.reply(), &rejection);
            assert_eq!(
                serde_json::to_value(check.source()).unwrap(),
                faults[0]["payload"]["account"]["result_source"]
            );
            assert!(check.source().block_id.sequence < fault.block_id.sequence);
            assert!(
                fx.input
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .is_none()
            );
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
                    .take_refused_repair(&mut fx.input.run.clock)
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
            assert!(fx.input.run.context.completed_author.is_none());
            assert!(fx.input.run.context.failure().is_none());
            let next = fx.generation(vec![text("Read next.")], true);
            assert!(!next.generation().blocks().is_empty());
        }
    }
}

#[test]
fn repair_review_stop_during_refused_correction_retains_the_immutable_terminal_prefix() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        let mut fx = RepairFixture::new();
        let original = fx.generation(vec![text("bad original")], true);
        fx.begin(original);
        let request = fx.restatement(true);
        let correction = fx.generation(
            vec![text("Read first."), call(), text("Read second."), call()],
            true,
        );
        let openings = correction
            .requests()
            .iter()
            .map(|request| request.opening().clone())
            .collect::<Vec<_>>();
        assert_eq!(openings.len(), 2);
        let correction_source = correction.generation().source().clone();
        fx.observe(request.source(), correction);
        // Defensive refused-with-pending ordering, with actual correction and
        // real pair receipts. No current production caller elects this overlap.
        fx.input
            .run
            .context
            .stage_repair_test_refusal(&mut fx.input.run.clock);
        for _ in 0..512 {
            fx.step(true);
            if !fx.input.records(Kind::ToolCallCompleted).is_empty() {
                break;
            }
        }
        let prefix = fx.input.records(Kind::ToolCallCompleted);
        assert_eq!(
            prefix.len(),
            1,
            "cancel before the second terminal is admitted"
        );
        assert_eq!(
            prefix[0]["payload"]["reason"]["not_dispatched"]["cause"],
            "pass_purpose"
        );
        fx.input
            .run
            .context
            .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
            .unwrap();
        let retained = cancelled(&mut fx, cause);
        let pending = retained
            .pending()
            .expect("cancel must retain the accounted correction");
        assert_eq!(pending.generation().source(), &correction_source);
        assert_eq!(pending.requests().len(), openings.len());
        assert!(matches!(
            retained.refusal_evidence(),
            Some(RepairRefusalEvidence::Instruction { .. })
        ));
        for (index, opening) in openings.iter().enumerate() {
            assert_eq!(pending.requests()[index].opening(), opening);
            let completed = fx
                .input
                .run
                .context
                .completed_pair(&opening.call_id, &mut fx.input.run.clock)
                .unwrap()
                .unwrap();
            assert_eq!(completed.started(), opening);
            assert_eq!(completed.ended().fate, s::TerminalPairFate::Failed);
            assert_eq!(
                completed.ended().reason,
                Some(s::PairFailure::NotDispatched {
                    cause: if index == 0 {
                        s::NonDispatchCause::PassPurpose
                    } else {
                        cause
                    },
                })
            );
        }
        let terminals = fx.input.records(Kind::ToolCallCompleted);
        assert_eq!(terminals.len(), 2);
        assert_eq!(terminals[0], prefix[0]);
        assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 2);
        assert!(
            fx.input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Decision)
                .is_none()
        );
        assert!(fx.input.run.context.completed_author.is_none());
        assert!(fx.input.run.context.failure().is_none());
        assert!(
            fx.input
                .run
                .context
                .take_refused_repair(&mut fx.input.run.clock)
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
        let next = fx.generation(vec![text("Read next.")], true);
        assert!(!next.generation().blocks().is_empty());
    }
}
