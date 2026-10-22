//! EMPTY-CORRECTION through the actual output and single-author recording ports.
use super::*;

fn empty_refusals(fx: &RepairFixture) -> Vec<serde_json::Value> {
    fx.input
        .records(Kind::Refusal)
        .into_iter()
        .filter(|record| record["payload"]["case"] == "empty_correction")
        .collect()
}

fn assert_empty_refusal(record: &serde_json::Value, source: &s::SourceBlockId, correction: u64) {
    assert_eq!(record["turn"], "output-turn");
    assert_eq!(record["causal_parent"], correction.to_string());
    assert_eq!(
        record["payload"],
        serde_json::json!({
            "seam": "protocol_repair", "source_id": source,
            "case": "empty_correction", "detail": "correction contains no Text block or attempted call",
        })
    );
    let encoded = record["payload"].to_string();
    let decoded: weaver_types::RefusalRecord = s::decode_strict(&encoded).unwrap();
    assert!(
        matches!(decoded, weaver_types::RefusalRecord::ProtocolRepair {
        source_id, case: s::ProtocolRepairCase::EmptyCorrection, detail,
    } if source_id == *source && detail == "correction contains no Text block or attempted call")
    );
    assert!(
        s::decode_strict::<weaver_types::RefusalRecord>(
            &encoded.replace("empty_correction", "unknown_correction")
        )
        .is_err()
    );
}

/// Fill the recorder, then let the independent Leave window interrupt its wait.
/// A resumed author must retain the original ticket and enclosing Decision bound.
fn interrupt_refusal(fx: &mut RepairFixture) {
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
            Err(error) => panic!("unexpected refusal pressure failure: {error:?}"),
        }
        fx.checker();
        fx.member(false);
    }
    assert!(
        interrupted,
        "pending refusal must retain its lifecycle continuation"
    );
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    let pending = fx
        .input
        .run
        .context
        .pending_author()
        .expect("actual pending refusal ticket");
    assert_eq!(pending.event.kind, Kind::Refusal);
    assert!(empty_refusals(fx).is_empty());
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
}

#[test]
fn empty_correction_recording_preserves_original_attribution_and_counts_once_through_pressure() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("Read sibling."), text("bad original")], true);
    fx.begin(original);
    let first = fx.restatement(true);
    assert_eq!(first.source().block.0, 1);
    let checks = fx.input.records(Kind::Ste100Result).len();
    let empty = fx.generation(vec![], true);
    let empty_source = empty.generation().source().clone();
    assert_eq!(
        fx.input.records(Kind::Ste100Result).len(),
        checks,
        "no invented empty-output checker result"
    );
    fx.observe(first.source(), empty);
    assert!(empty_refusals(&fx).is_empty());
    fx.input.fill_trace();
    let decision = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    interrupt_refusal(&mut fx);
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    interrupt_refusal(&mut fx);
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(decision)
    );
    fx.input.drain_trace();
    let second = fx.restatement(true);
    assert_eq!(second.source(), first.source());
    assert_eq!(second.attempt().0, 2, "one recorded refusal counts once");
    let refusals = empty_refusals(&fx);
    assert_eq!(refusals.len(), 1);
    assert_empty_refusal(
        &refusals[0],
        first.source(),
        empty_source.block_id.sequence.0,
    );
    let checks = fx.input.records(Kind::Ste100Result).len();
    let empty = fx.generation(vec![], true);
    let second_source = empty.generation().source().clone();
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), checks);
    fx.observe(second.source(), empty);
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
    let exhausted = exhausted.expect("two empty corrections exhaust the existing cap");
    assert_eq!(exhausted.failed_corrections().0, 2);
    let account = exhausted.into_account();
    assert_eq!(account.corrections().len(), 2);
    assert!(
        account.corrections().iter().all(|pass| pass
            .generation()
            .original()
            .message
            .content
            .is_empty())
    );
    let refusals = empty_refusals(&fx);
    assert_eq!(refusals.len(), 2);
    assert_empty_refusal(
        &refusals[1],
        first.source(),
        second_source.block_id.sequence.0,
    );
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
    assert!(fx.input.records(Kind::ToolCallCompleted).is_empty());
    assert!(fx.filings.iter().all(|frame| match frame {
        s::StateFrame::File { filing, .. } =>
            filing.block_id.sequence != empty_source.block_id.sequence
                && filing.block_id.sequence != second_source.block_id.sequence,
        _ => false,
    }));
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
fn cancellation_finishes_a_pending_empty_refusal_without_retry_or_success() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        let mut fx = RepairFixture::new();
        let original = fx.generation(vec![text("bad original")], true);
        fx.begin(original);
        let request = fx.restatement(true);
        let empty = fx.generation(vec![], true);
        let empty_source = empty.generation().source().clone();
        fx.observe(request.source(), empty);
        fx.input.fill_trace();
        interrupt_refusal(&mut fx);
        let ticket = fx.input.run.context.pending_author().unwrap().ticket;
        fx.input
            .run
            .context
            .cancel_repair(cause, &mut fx.input.run.clock)
            .unwrap();
        assert_eq!(
            fx.input.run.context.pending_author().unwrap().ticket,
            ticket
        );
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
        let cancelled = cancelled.expect("recorded refusal returns retained cancellation custody");
        assert_eq!(
            cancelled.pending().unwrap().generation().source(),
            &empty_source
        );
        assert!(cancelled.account().corrections().is_empty());
        let refusals = empty_refusals(&fx);
        assert_eq!(refusals.len(), 1);
        assert_empty_refusal(
            &refusals[0],
            request.source(),
            empty_source.block_id.sequence.0,
        );
        assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 1);
        assert!(fx.repair_records("repair_exhausted").is_empty());
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
        assert!(fx.input.run.context.failure().is_none());
    }
}

#[test]
fn empty_original_empty_text_and_call_only_outputs_do_not_become_empty_correction_refusals() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![], true);
    fx.begin(original);
    assert!(fx.effective(true).statements().is_empty());
    assert!(empty_refusals(&fx).is_empty());
    assert!(fx.input.records(Kind::LoopInstruction).is_empty());
    for content in [
        vec![text("")],
        vec![call()],
        vec![ContentBlock::UnrecoveredCall {
            fragment: "<partial>".into(),
        }],
    ] {
        let mut fx = RepairFixture::new();
        let original = fx.generation(vec![text("bad original")], true);
        fx.begin(original);
        let first = fx.restatement(true);
        let check_count = fx.input.records(Kind::Ste100Result).len();
        let is_text = matches!(&content[0], ContentBlock::Text { .. });
        let correction = fx.generation(content, true);
        assert_eq!(
            fx.input.records(Kind::Ste100Result).len(),
            check_count + usize::from(is_text)
        );
        fx.observe(first.source(), correction);
        let second = fx.restatement(true);
        assert_eq!(second.attempt().0, 2);
        assert!(empty_refusals(&fx).is_empty());
        let calls = fx
            .input
            .records(Kind::Refusal)
            .into_iter()
            .filter(|event| event["payload"]["case"] == "correction_call")
            .count();
        assert_eq!(calls, usize::from(!is_text));
        let correction = fx.generation(vec![text("Read replacement.")], true);
        fx.observe(second.source(), correction);
        assert_eq!(fx.effective(true).text(), "Read replacement.");
        assert!(fx.input.run.context.failure().is_none());
    }
}
