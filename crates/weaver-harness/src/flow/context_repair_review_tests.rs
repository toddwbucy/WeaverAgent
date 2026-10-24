//! PR 261 refusal-account custody. Defensive overlap fixtures name their scope.
use super::*;

#[path = "context_repair_accounting_review_tests.rs"]
mod accounting_review_tests;

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

#[test]
fn repair_followup_refused_correction_retains_actual_pass_stop_unload_prefixes() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = fx.generation(
        vec![
            text("Read first."),
            call(),
            text("Read second."),
            call(),
            text("Read third."),
            call(),
        ],
        true,
    );
    let openings = correction
        .requests()
        .iter()
        .map(|request| request.opening().clone())
        .collect::<Vec<_>>();
    assert_eq!(openings.len(), 3);
    let correction_source = correction.generation().source().clone();
    fx.observe(request.source(), correction);
    // This future refused-with-correction overlap is deliberately staged; every
    // pair, terminal admission, member receipt and cancellation below is real.
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
    let mut prefixes = Vec::new();
    for (expected, cause) in [
        (1, s::NonDispatchCause::Stop),
        (2, s::NonDispatchCause::Unload),
    ] {
        for _ in 0..512 {
            fx.step(true);
            if fx.input.records(Kind::ToolCallCompleted).len() == expected {
                break;
            }
        }
        let records = fx.input.records(Kind::ToolCallCompleted);
        assert_eq!(records.len(), expected);
        assert_eq!(
            fx.input.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
        assert_eq!(&records[..prefixes.len()], prefixes.as_slice());
        prefixes = records;
        fx.input
            .run
            .context
            .cancel_repair(cause, &mut fx.input.run.clock)
            .unwrap();
    }
    let retained = cancelled(&mut fx, s::NonDispatchCause::Unload);
    let pending = retained.pending().unwrap();
    assert_eq!(pending.generation().source(), &correction_source);
    assert!(matches!(
        retained.refusal_evidence(),
        Some(RepairRefusalEvidence::Instruction { .. })
    ));
    let expected = [
        s::NonDispatchCause::PassPurpose,
        s::NonDispatchCause::Stop,
        s::NonDispatchCause::Unload,
    ];
    for ((opening, cause), retained_request) in
        openings.iter().zip(expected).zip(pending.requests())
    {
        assert_eq!(retained_request.opening(), opening);
        let proof = fx
            .input
            .run
            .context
            .completed_pair(&opening.call_id, &mut fx.input.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(proof.started(), opening);
        assert_eq!(
            proof.ended().reason,
            Some(s::PairFailure::NotDispatched { cause })
        );
    }
    let records = fx.input.records(Kind::ToolCallCompleted);
    assert_eq!(records.len(), 3);
    assert_eq!(&records[..prefixes.len()], prefixes.as_slice());
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
}

fn completed_cause_fixture(cause: s::NonDispatchCause) -> (RepairFixture, s::CallId) {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("Read request."), call()], true);
    let call = original.requests()[0].opening().call_id.clone();
    fx.input
        .run
        .context
        .begin_not_dispatched(original, cause, &mut fx.input.run.clock)
        .unwrap();
    let mut complete = false;
    for _ in 0..512 {
        let progress = fx
            .input
            .run
            .context
            .advance_pairs(&mut ports(&mut fx.input.run.clock))
            .unwrap();
        fx.checker();
        fx.member(true);
        if progress == PairProgress::Complete {
            let original = fx
                .input
                .run
                .context
                .take_pair_accounting(&mut fx.input.run.clock)
                .unwrap()
                .unwrap();
            assert_eq!(original.requests()[0].opening().call_id, call);
            complete = true;
            break;
        }
    }
    assert!(complete, "actual pair account must return its original");
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    (fx, call)
}

#[test]
fn repair_followup_defensive_composed_proofs_reject_wrong_cause_and_backward_prefix() {
    let (pass, pass_call) = completed_cause_fixture(s::NonDispatchCause::PassPurpose);
    let (stop, stop_call) = completed_cause_fixture(s::NonDispatchCause::Stop);
    let (unload, unload_call) = completed_cause_fixture(s::NonDispatchCause::Unload);
    let (wrong, wrong_call) = completed_cause_fixture(s::NonDispatchCause::RepairExhausted);
    let pass = pass
        .input
        .run
        .context
        .pairs
        .completed(&pass_call)
        .unwrap()
        .unwrap();
    let stop = stop
        .input
        .run
        .context
        .pairs
        .completed(&stop_call)
        .unwrap()
        .unwrap();
    let unload = unload
        .input
        .run
        .context
        .pairs
        .completed(&unload_call)
        .unwrap()
        .unwrap();
    let wrong = wrong
        .input
        .run
        .context
        .pairs
        .completed(&wrong_call)
        .unwrap()
        .unwrap();
    // These are actual completed capabilities deliberately composed in orders
    // that a single public pair owner cannot produce. This is a defensive
    // validator test, not evidence that normal cancellation can corrupt order.
    for proofs in [
        vec![stop, pass],
        vec![unload, stop],
        vec![wrong],
        vec![pass, wrong],
    ] {
        assert_eq!(
            repair::validate_refused_accounted(
                proofs.iter().map(|p| p.started()),
                &proofs,
                Some(s::NonDispatchCause::Unload)
            ),
            Err(RepairError::WrongAccounting)
        );
    }
    assert_eq!(
        repair::validate_refused_accounted(
            [unload.started()].into_iter(),
            &[unload],
            Some(s::NonDispatchCause::Stop)
        ),
        Err(RepairError::WrongAccounting)
    );
    assert_eq!(
        repair::validate_refused_accounted([stop.started()].into_iter(), &[stop], None),
        Err(RepairError::WrongAccounting)
    );
    assert_eq!(
        repair::validate_refused_accounted(
            [pass.started(), stop.started(), unload.started()].into_iter(),
            &[pass, stop, unload],
            Some(s::NonDispatchCause::Unload)
        ),
        Ok(())
    );
    assert_eq!(
        repair::validate_refused_accounted([pass.started()].into_iter(), &[], None),
        Err(RepairError::WrongAccounting)
    );
}

fn refused_with_correction() -> RepairFixture {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = fx.generation(vec![text("Read forbidden."), call()], true);
    fx.observe(request.source(), correction);
    fx
}

fn refusal_failure(fx: &mut RepairFixture) -> RunFailure {
    for _ in 0..512 {
        match fx
            .input
            .run
            .context
            .advance_repair(&mut ports(&mut fx.input.run.clock))
        {
            Ok(_) => {
                fx.checker();
                fx.member(true);
            }
            Err(failure) => return failure,
        }
    }
    panic!("defensive refused state must name its error, not wait for expiry");
}

#[test]
fn repair_followup_defensive_refusal_names_an_actual_outstanding_correction_loan() {
    let mut fx = refused_with_correction();
    let mut loan = None;
    for _ in 0..512 {
        fx.step(true);
        loan = fx.input.run.context.take_repair_correction_loan_for_test();
        if loan.is_some() {
            break;
        }
    }
    let loan = loan.expect("actual protocol refusal must release its correction loan");
    let source = loan.generation().source().clone();
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    // Public repair progress keeps this loan paired with its accounting owner.
    // Deliberately stage a refusal while the real loan is held by this fixture.
    fx.input
        .run
        .context
        .stage_repair_test_refusal(&mut fx.input.run.clock);
    let failure = refusal_failure(&mut fx);
    assert!(
        matches!(&failure, RunFailure::Flow { reason: FlowHaltReason::ProtocolFailed, detail, .. }
        if detail == "refused repair retains loan: OutstandingCorrectionLoan")
    );
    assert!(fx.input.run.clock.now().unwrap() < bound.deadline_ns());
    assert_eq!(loan.generation().source(), &source);
    assert_eq!(loan.requests().len(), 1);
    assert_eq!(fx.input.run.context.failure(), Some(&failure));
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
}

#[test]
fn repair_followup_defensive_refusal_names_an_actual_outstanding_account_loan() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(
        vec![text("Read request."), call(), text("bad original")],
        true,
    );
    fx.begin(original);
    for _ in 0..2 {
        let request = fx.restatement(true);
        let correction = fx.generation(vec![text("bad correction")], true);
        fx.observe(request.source(), correction);
    }
    let mut loan = None;
    for _ in 0..512 {
        fx.step(true);
        loan = fx.input.run.context.take_repair_account_loan_for_test();
        if loan.is_some() {
            break;
        }
    }
    let loan = loan.expect("actual exhaustion record must release its account loan");
    let source = loan.source().clone();
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    // An account loan removes the original from the ledger, so a new refusal
    // cannot even be authored through the ordinary path. Invoke the private
    // refusal continuation directly with that real loan held: this is an
    // unreachable defensive state, not a claimed production interleaving.
    let failure = fx
        .input
        .run
        .context
        .advance_refused_repair_for_test(&mut ports(&mut fx.input.run.clock))
        .unwrap_err();
    assert!(
        matches!(&failure, RunFailure::Flow { reason: FlowHaltReason::ProtocolFailed, detail, .. }
        if detail == "refused repair retains loan: OutstandingAccountLoan")
    );
    assert!(fx.input.run.clock.now().unwrap() < bound.deadline_ns());
    assert_eq!(loan.source(), &source);
    assert_eq!(loan.requests().len(), 1);
    assert_eq!(fx.input.run.context.failure(), Some(&failure));
}

#[test]
fn repair_followup_defensive_frozen_pair_owner_preserves_its_exact_error_before_intake() {
    for cancel in [None, Some(s::NonDispatchCause::Stop)] {
        let mut fx = refused_with_correction();
        fx.input
            .run
            .context
            .stage_repair_test_refusal(&mut fx.input.run.clock);
        if let Some(cause) = cancel {
            fx.input
                .run
                .context
                .cancel_repair(cause, &mut fx.input.run.clock)
                .unwrap();
        }
        // A real halt freezes the entire context first. Deliberately freeze
        // only its pair owner to test exact internal error propagation.
        fx.input.run.context.pairs.freeze();
        let failure = refusal_failure(&mut fx);
        assert!(
            matches!(&failure, RunFailure::Flow { reason: FlowHaltReason::ProtocolFailed, detail, .. }
            if detail == "correction pair proof: Frozen")
        );
        assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
        assert!(fx.input.records(Kind::ToolCallCompleted).is_empty());
        assert_eq!(fx.input.run.context.failure(), Some(&failure));
        let (account, correction) = fx.input.run.context.repair_retained_custody_for_test();
        let account = account.expect("failure retains the actual original account");
        let correction = correction.expect("failure retains the actual correction");
        assert_eq!(correction.requests().len(), 1);
        assert_ne!(account.source(), correction.generation().source());
        assert!(!fx.input.run.context.repair_refused_pairs_settled_for_test());
    }
}

#[test]
fn repair_followup_refusal_readiness_reuses_its_settled_terminal_proof() {
    let mut fx = refused_with_correction();
    fx.input
        .run
        .context
        .stage_repair_test_refusal(&mut fx.input.run.clock);
    for _ in 0..512 {
        fx.step(true);
        if fx.input.run.context.repair_refused_pairs_settled_for_test() {
            break;
        }
    }
    assert!(fx.input.run.context.repair_refused_pairs_settled_for_test());
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_some()
    );
    assert!(
        fx.input
            .run
            .context
            .take_refused_repair(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    let terminals = fx.input.records(Kind::ToolCallCompleted);
    // Terminal capabilities cannot change after completion. This defensive
    // freeze after validation but before readiness proves the next transition
    // and later polls do not acquire a fresh pair proof.
    fx.input.run.context.pairs.freeze();
    for _ in 0..4 {
        assert_eq!(fx.step(true), RepairProgress::Refused);
    }
    assert_eq!(fx.input.records(Kind::ToolCallCompleted), terminals);
    let retained = fx
        .input
        .run
        .context
        .take_refused_repair(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(retained.pending().unwrap().requests().len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn repair_custody_shared_validation_rejects_foreign_actual_terminal_without_losing_originals() {
    let (foreign, call) = completed_cause_fixture(s::NonDispatchCause::RepairExhausted);
    let wrong = foreign
        .input
        .run
        .context
        .pairs
        .completed(&call)
        .unwrap()
        .unwrap();
    let mut fx = refused_with_correction();
    fx.input
        .run
        .context
        .stage_repair_test_refusal(&mut fx.input.run.clock);
    let mut ready = false;
    for _ in 0..512 {
        fx.step(true);
        if fx
            .input
            .run
            .context
            .repair_retained_custody_for_test()
            .1
            .is_some()
        {
            ready = true;
            break;
        }
    }
    assert!(
        ready,
        "refusal must retain the actual correction before pair intake"
    );
    assert!(!fx.input.run.context.repair_refused_pairs_settled_for_test());
    let (account, correction) = fx.input.run.context.repair_retained_custody_for_test();
    let original_source = account.unwrap().source().clone();
    let correction_source = correction.unwrap().generation().source().clone();
    let opening = correction.unwrap().requests()[0].opening().clone();
    // Borrow a real completed capability from another owner. Public accounting
    // cannot supply this composition; it isolates the shared validation gate.
    assert_eq!(
        fx.input
            .run
            .context
            .settle_refused_proofs_for_test(&[wrong]),
        Err(RepairError::WrongAccounting)
    );
    assert!(!fx.input.run.context.repair_refused_pairs_settled_for_test());
    let (account, correction) = fx.input.run.context.repair_retained_custody_for_test();
    assert_eq!(account.unwrap().source(), &original_source);
    assert_eq!(
        correction.unwrap().generation().source(),
        &correction_source
    );
    assert_eq!(correction.unwrap().requests()[0].opening(), &opening);
    assert!(fx.input.run.context.failure().is_none());
    let refused = finish_refused(&mut fx);
    assert_eq!(refused.account().source(), &original_source);
    assert_eq!(
        refused.pending().unwrap().generation().source(),
        &correction_source
    );
}
