//! Complete forbidden-call accounting through actual pair and filing owners.
use super::*;

fn answer(fx: &mut RepairFixture, index: usize) {
    assert!(!fx.answered[index]);
    let request = &fx.filings[index];
    let block_id = match request {
        s::StateFrame::File { filing, .. } => filing.block_id.clone(),
        s::StateFrame::FilePair { filing, .. } => filing.block_id.clone(),
        _ => unreachable!(),
    };
    let began = fx.input.run.clock.now().unwrap() + 1;
    let response = s::StateFrame::Filed {
        common: request.common().clone(),
        block_id,
        observation: operation(request, fx.operation, began),
    };
    fx.operation += 1;
    fx.input.run.clock.advance_to(began + 1).unwrap();
    fx.input
        .run
        .state
        .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
        .unwrap();
    fx.answered[index] = true;
}

fn member_except(fx: &mut RepairFixture, held: &s::CallId) -> Option<usize> {
    fx.member(false);
    let mut found = None;
    for index in 0..fx.filings.len() {
        if fx.answered[index] {
            continue;
        }
        if matches!(&fx.filings[index], s::StateFrame::FilePair { filing, .. }
            if matches!(&filing.pair, s::PairRecord::Ended(end) if &end.call_id == held))
        {
            found = Some(index);
        } else {
            answer(fx, index);
        }
    }
    fx.transport(TransportSource::State);
    found
}

fn hold_terminal(fx: &mut RepairFixture, held: &s::CallId, prior: u32) -> usize {
    for _ in 0..512 {
        fx.step(false);
        let index = member_except(fx, held);
        assert_eq!(
            count(fx),
            prior,
            "a terminal without its actual Filed observation is incomplete"
        );
        if let Some(index) = index {
            return index;
        }
    }
    panic!("actual terminal filing must reach the state socket");
}

fn forbidden(
    fx: &mut RepairFixture,
    request: &repair::RestatementRequest,
    calls: usize,
) -> (s::SourcePosition, Vec<s::PairStarted>) {
    let content = (0..calls)
        .flat_map(|index| [text(&format!("Read item{index}.")), call()])
        .collect();
    let correction = fx.generation(content, true);
    assert_eq!(correction.requests().len(), calls);
    let source = correction.generation().source().clone();
    let openings = correction
        .requests()
        .iter()
        .map(|request| request.opening().clone())
        .collect();
    fx.observe(request.source(), correction);
    (source, openings)
}

fn finish_at_receipt(fx: &mut RepairFixture, prior: u32) {
    for _ in 0..64 {
        fx.input
            .run
            .context
            .reduce_flow(&mut fx.input.run.clock)
            .unwrap();
        fx.transport(TransportSource::State);
        if count(fx) == prior + 1 {
            return;
        }
    }
    panic!("actual final observation must charge before repair/pair extraction");
}

fn assert_count(
    fx: &RepairFixture,
    source: &s::SourceBlockId,
    correction: &s::SourcePosition,
    expected: u32,
) {
    let account = fx.input.run.context.repair_recorded_account();
    let recorded = account.failed_corrections().last().unwrap();
    assert_eq!(recorded.source(), source);
    assert_eq!(recorded.correction(), correction);
    assert_eq!(recorded.failed_corrections(), expected);
    assert_eq!(account.failed_corrections().len(), expected as usize);
}

#[test]
fn cancelled_call_count_requires_every_actual_pair_filing_before_extract_below_and_at_cap() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for prior in [0, 1] {
            let (mut fx, request) = at_attempt(prior);
            let (correction, starts) = forbidden(&mut fx, &request, 2);
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
            cancel(&mut fx, cause);
            let final_index = hold_terminal(&mut fx, &starts[1].call_id, prior);
            assert!(
                fx.input
                    .run
                    .context
                    .pairs
                    .completed(&starts[0].call_id)
                    .unwrap()
                    .is_some()
            );
            assert!(
                fx.input
                    .run
                    .context
                    .pairs
                    .completed(&starts[1].call_id)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                fx.input.records(Kind::ToolCallCompleted).len(),
                2,
                "Recorded PairEnded alone does not count"
            );
            no_continuation(&mut fx);
            assert!(
                fx.input
                    .run
                    .context
                    .take_cancelled_repair(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none()
            );
            for _ in 0..4 {
                fx.step(false);
                assert_eq!(count(&fx), prior);
            }
            answer(&mut fx, final_index);
            fx.transport(TransportSource::State);
            finish_at_receipt(&mut fx, prior);
            assert_count(&fx, request.source(), &correction, prior + 1);
            // The pair owner still holds its original: no finish_accounting hook ran.
            assert_eq!(
                fx.input
                    .run
                    .context
                    .pairs
                    .original()
                    .unwrap()
                    .generation()
                    .source(),
                &correction
            );
            for started in &starts {
                let pair = fx
                    .input
                    .run
                    .context
                    .pairs
                    .completed(&started.call_id)
                    .unwrap()
                    .unwrap();
                assert_eq!(pair.started(), started);
                assert_eq!(
                    pair.ended().reason,
                    Some(s::PairFailure::NotDispatched { cause })
                );
                assert!(matches!(
                    pair.statement().receipt(),
                    s::StateFrame::Filed { .. }
                ));
                assert!(matches!(
                    pair.opening().receipt(),
                    s::StateFrame::Filed { .. }
                ));
                assert!(matches!(
                    pair.terminal().receipt(),
                    s::StateFrame::Filed { .. }
                ));
            }
            cancel(&mut fx, cause);
            no_continuation(&mut fx);
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Decision),
                Some(decision)
            );
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Leave),
                Some(leave)
            );
            let retained = cancelled(&mut fx, true);
            assert_eq!(
                retained.pending().unwrap().generation().source(),
                &correction
            );
            assert_eq!(
                retained.account().failed_corrections().len(),
                (prior + 1) as usize
            );
            assert!(fx.repair_records("repair_exhausted").is_empty());
            assert!(
                fx.input
                    .run
                    .context
                    .take_cancelled_repair(&mut fx.input.run.clock)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
fn cancelled_call_count_preserves_admitted_pass_purpose_stop_and_unload_prefixes() {
    let (mut fx, request) = at_attempt(0);
    let (correction, starts) = forbidden(&mut fx, &request, 3);
    let first = hold_terminal(&mut fx, &starts[0].call_id, 0);
    cancel(&mut fx, s::NonDispatchCause::Stop);
    answer(&mut fx, first);
    let second = hold_terminal(&mut fx, &starts[1].call_id, 0);
    cancel(&mut fx, s::NonDispatchCause::Unload);
    answer(&mut fx, second);
    let third = hold_terminal(&mut fx, &starts[2].call_id, 0);
    answer(&mut fx, third);
    fx.transport(TransportSource::State);
    finish_at_receipt(&mut fx, 0);
    assert_count(&fx, request.source(), &correction, 1);
    for (started, cause) in starts.iter().zip([
        s::NonDispatchCause::PassPurpose,
        s::NonDispatchCause::Stop,
        s::NonDispatchCause::Unload,
    ]) {
        let pair = fx
            .input
            .run
            .context
            .pairs
            .completed(&started.call_id)
            .unwrap()
            .unwrap();
        assert_eq!(pair.started(), started);
        assert_eq!(
            pair.ended().reason,
            Some(s::PairFailure::NotDispatched { cause })
        );
    }
    assert_eq!(
        cancelled(&mut fx, true)
            .account()
            .failed_corrections()
            .len(),
        1
    );
    assert!(fx.repair_records("repair_exhausted").is_empty());
}

#[test]
fn cancelled_call_count_keeps_a_complete_pass_when_cancellation_precedes_loan_return() {
    let (mut fx, request) = at_attempt(0);
    let (correction, starts) = forbidden(&mut fx, &request, 2);
    let final_index = hold_terminal(&mut fx, &starts[1].call_id, 0);
    answer(&mut fx, final_index);
    fx.transport(TransportSource::State);
    finish_at_receipt(&mut fx, 0);
    assert_count(&fx, request.source(), &correction, 1);
    assert!(fx.input.run.context.pairs.original().is_some());
    cancel(&mut fx, s::NonDispatchCause::Stop);
    let retained = cancelled(&mut fx, true);
    assert_eq!(retained.account().failed_corrections().len(), 1);
    assert_eq!(
        retained.pending().unwrap().generation().source(),
        &correction
    );
    assert!(fx.repair_records("repair_exhausted").is_empty());
}

#[test]
fn cancelled_call_count_survives_a_clock_failure_after_the_final_filed_observation() {
    let mut witnessed = false;
    for remaining in 0..64 {
        let (mut fx, request) = at_attempt(0);
        let (correction, starts) = forbidden(&mut fx, &request, 2);
        cancel(&mut fx, s::NonDispatchCause::Stop);
        let final_index = hold_terminal(&mut fx, &starts[1].call_id, 0);
        let terminal_source = match &fx.filings[final_index] {
            s::StateFrame::FilePair { filing, .. } => filing.block_id.clone(),
            _ => unreachable!(),
        };
        answer(&mut fx, final_index);
        fx.transport(TransportSource::State);
        let mut clock = BrokenAfter {
            now: fx.input.run.clock.now().unwrap(),
            remaining,
        };
        let mut failed = None;
        for _ in 0..32 {
            if let Err(error) = fx.input.run.context.reduce_flow(&mut clock) {
                failed = Some(error);
                break;
            }
        }
        // Observe the trace itself; a count mutation cannot hide the reached boundary.
        let actual = fx.input.records(Kind::StateFlow).iter().any(|record| {
            record["payload"]["phase"] == "filed"
                && record["payload"]["source_id"] == serde_json::to_value(&terminal_source).unwrap()
        });
        if !actual {
            continue;
        }
        let failure = failed.expect("persistent injected clock failure must freeze continuation");
        assert_eq!(
            count(&fx),
            1,
            "the actual final receipt survives later clock failure"
        );
        assert_count(&fx, request.source(), &correction, 1);
        assert_eq!(fx.input.run.context.failure(), Some(&failure));
        assert_eq!(
            fx.input
                .run
                .context
                .take_cancelled_repair(&mut fx.input.run.clock)
                .unwrap_err(),
            failure
        );
        assert!(fx.repair_records("repair_exhausted").is_empty());
        witnessed = true;
        break;
    }
    assert!(
        witnessed,
        "reach the actual final Filed-before-failure boundary"
    );
}

#[test]
fn cancelled_call_count_unassociated_rejected_unrecovered_and_malformed_calls_need_all_refusals() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        for prior in [0, 1] {
            for shape in 0..4 {
                let (mut fx, request) = at_attempt(prior);
                let correction = match shape {
                    0 => fx.generation(vec![call()], true),
                    1 => fx.generation(vec![text("bad call statement"), call()], true),
                    2 => fx.generation(
                        vec![ContentBlock::UnrecoveredCall {
                            fragment: "<call".into(),
                        }],
                        true,
                    ),
                    _ => recall_refusal_tests::generation(
                        &mut fx,
                        vec![
                            text("Recall first and second."),
                            text("Recall item if ready."),
                        ],
                    ),
                };
                assert!(correction.attempted_call());
                assert!(correction.requests().is_empty());
                let identity = correction.generation().source().clone();
                let refusals = if shape == 3 { 3 } else { 1 };
                cancel(&mut fx, cause);
                fx.observe(request.source(), correction);
                let decision = fx.input.run.context.deadlines.get(PumpDeadline::Decision);
                let mut reached = false;
                for _ in 0..128 {
                    fx.step(true);
                    let recorded = fx.input.records(Kind::Refusal);
                    if recorded.len() < refusals {
                        assert_eq!(count(&fx), prior);
                    } else {
                        assert_eq!(recorded.len(), refusals);
                        assert_count(&fx, request.source(), &identity, prior + 1);
                        assert_eq!(
                            recorded.last().unwrap()["payload"]["case"],
                            "correction_call"
                        );
                        assert_eq!(
                            recorded.last().unwrap()["causal_parent"],
                            identity.block_id.sequence.0.to_string()
                        );
                        reached = true;
                        break;
                    }
                }
                assert!(reached);
                assert_eq!(
                    fx.input.run.context.deadlines.get(PumpDeadline::Decision),
                    decision
                );
                assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
                assert!(fx.input.records(Kind::ToolCallCompleted).is_empty());
                no_continuation(&mut fx);
                let retained = cancelled(&mut fx, true);
                assert_eq!(retained.pending().unwrap().generation().source(), &identity);
                assert_eq!(
                    retained.account().failed_corrections().len(),
                    (prior + 1) as usize
                );
                assert_eq!(retained.account().recall_refusals().len(), refusals - 1);
            }
        }
    }
}

#[test]
fn cancelled_call_count_with_requests_and_malformed_recall_waits_for_refusals_and_terminal() {
    let (mut fx, request) = at_attempt(0);
    let correction = recall_refusal_tests::generation(
        &mut fx,
        vec![
            text("Recall first and second."),
            text("Read item."),
            call(),
            text("Recall item if ready."),
        ],
    );
    assert_eq!(correction.requests().len(), 1);
    let started = correction.requests()[0].opening().clone();
    let identity = correction.generation().source().clone();
    fx.observe(request.source(), correction);
    cancel(&mut fx, s::NonDispatchCause::Unload);
    let terminal = hold_terminal(&mut fx, &started.call_id, 0);
    assert_eq!(fx.input.records(Kind::Refusal).len(), 3);
    assert_eq!(count(&fx), 0);
    answer(&mut fx, terminal);
    fx.transport(TransportSource::State);
    finish_at_receipt(&mut fx, 0);
    assert_count(&fx, request.source(), &identity, 1);
    let retained = cancelled(&mut fx, true);
    assert_eq!(retained.account().recall_refusals().len(), 2);
    assert_eq!(retained.account().failed_corrections().len(), 1);
    assert_eq!(
        retained.pending().unwrap().requests()[0].opening(),
        &started
    );
}
