//! The real recorder, classifier socket and member socket carry every decision.
use super::*;
use crate::flow::sort::OutboundInput;

fn prepared(words: &'static str) -> (RepairFixture, OutboundInput, TemplateCertificate) {
    prepared_with_selection(words, true)
}

fn prepared_with_selection(
    words: &'static str,
    selected: bool,
) -> (RepairFixture, OutboundInput, TemplateCertificate) {
    let mut fx = RepairFixture::with_sort_partition("Read correction.", Some(words), selected);
    let original = fx.generation(
        vec![text("Read files."), call(), text("Read answer.")],
        true,
    );
    fx.begin(original);
    let effective = fx.effective(true);
    let certificate = fx
        .input
        .run
        .context
        .take_repair_certificate(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    let request = &effective.requests()[0];
    let input = OutboundInput {
        source: effective.source().clone(),
        acts: vec![
            s::SortAct::Tool {
                call_id: request.call_id.clone(),
                statement: request.statement.clone(),
            },
            s::SortAct::Answer {
                source: effective.statements().last().unwrap().block_id.clone(),
            },
        ],
        effective,
    };
    fx.arm();
    (fx, input, certificate)
}

fn advance(fx: &mut RepairFixture, file: bool) -> OutboundProgress {
    let progress = fx
        .input
        .run
        .context
        .advance_outbound(&mut ports(&mut fx.input.run.clock))
        .unwrap();
    fx.checker();
    fx.member(file);
    progress
}

fn complete(fx: &mut RepairFixture) -> OutboundCompletion {
    for _ in 0..512 {
        if advance(fx, true) == OutboundProgress::Complete {
            assert!(
                fx.input
                    .run
                    .context
                    .complete_wait(PumpDeadline::Decision)
                    .is_none()
            );
        }
        if let Some(result) = fx
            .input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .unwrap()
        {
            return result;
        }
    }
    panic!("outbound did not finish");
}

#[test]
fn outbound_unselected_decision_still_records_without_inventing_a_filing_receipt() {
    let (mut fx, input, certificate) = prepared_with_selection("Read ", false);
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let OutboundCompletion::Sorted(sorted) = complete(&mut fx) else {
        panic!("unselected decision did not complete");
    };
    let (_, record) = sorted.into_parts();
    assert_eq!(record.receipts().sources().count(), 0);
    let records = fx.input.records(Kind::SortDecision);
    assert_eq!(records.len(), 1);
    assert_eq!(sequence(&records[0]), record.sequence().0);
}

#[test]
fn outbound_records_exact_acts_before_use_and_never_opens_a_pair() {
    let (mut fx, input, certificate) = prepared("Read ");
    let acts = input.acts.clone();
    let source = input.source.clone();
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    for _ in 0..32 {
        advance(&mut fx, false);
    }
    assert!(
        fx.input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
    let OutboundCompletion::Sorted(sorted) = complete(&mut fx) else {
        panic!("no sort grant");
    };
    assert_eq!(sorted.acts(), acts);
    let (input, record) = sorted.into_parts();
    assert_eq!(input.source, source);
    let records = fx.input.records(Kind::SortDecision);
    assert_eq!(records.len(), 1);
    assert_eq!(sequence(&records[0]), record.sequence().0);
    assert_eq!(
        records[0]["payload"]["decision"]["outbound"]["acts"],
        serde_json::to_value(acts).unwrap()
    );
    assert_eq!(
        records[0]["payload"]["message"]["content"][0]["text"],
        "Read identity."
    );
    assert_eq!(records[0]["origin"], "loop");
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
    assert!(
        fx.input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn outbound_cancellation_drains_its_record_without_a_sort_grant() {
    for after_author in [false, true] {
        let (mut fx, input, certificate) = prepared("Read ");
        let acts = input.acts.clone();
        fx.input
            .run
            .context
            .begin_outbound(input, &certificate, &mut fx.input.run.clock)
            .unwrap();
        if after_author {
            advance(&mut fx, false);
        }
        fx.input.run.context.cancel_outbound();
        let OutboundCompletion::Cancelled {
            input,
            record,
            fault,
        } = complete(&mut fx)
        else {
            panic!("cancelled sort granted use");
        };
        assert_eq!(input.acts, acts);
        assert!(fault.is_none());
        assert_eq!(fx.input.records(Kind::SortDecision).len(), 1);
        assert!(record.sequence().0 > input.source.block_id.sequence.0);
    }
}

#[test]
fn outbound_runtime_template_rejection_records_fault_then_returns_no_grant() {
    let (mut fx, input, certificate) = prepared("bad ");
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    let OutboundCompletion::Refused {
        input,
        record,
        fault,
    } = complete(&mut fx)
    else {
        panic!("rejected template granted use");
    };
    assert_eq!(input.acts.len(), 2);
    assert!(fault.0 > record.sequence().0);
    let faults = fx.input.records(Kind::Fault);
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0]["payload"]["case"], "state_template_rejected");
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn outbound_owner_excludes_generic_filing_take_advance_and_cancel() {
    let (mut fx, input, certificate) = prepared("Read ");
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    assert_eq!(
        fx.input
            .run
            .context
            .take_filing(&mut fx.input.run.clock)
            .unwrap_err(),
        RunFailure::PairOwned
    );
    assert_eq!(
        fx.input
            .run
            .context
            .advance_filing(&mut ports(&mut fx.input.run.clock))
            .unwrap_err(),
        RunFailure::PairOwned
    );
    fx.input.run.context.cancel_filing();
    assert!(fx.input.run.context.take_cancelled_filing().is_none());
    assert!(matches!(complete(&mut fx), OutboundCompletion::Sorted(_)));
}

#[test]
fn outbound_foreign_certificate_returns_input_and_owed_instruction_refusal() {
    let (mut fx, input, _) = prepared("Read ");
    let (_, _, foreign) = prepared("Read foreign ");
    let acts = input.acts.clone();
    let error = fx
        .input
        .run
        .context
        .begin_outbound(input, &foreign, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(
        error.template,
        Some(crate::flow::templates::ExpansionRefusal::ForeignCertificate)
    );
    let (returned, refusal) = error.into_parts();
    assert_eq!(returned.acts, acts);
    assert!(matches!(
        refusal.unwrap().payload,
        Some(Payload::Refusal(_))
    ));
    assert!(fx.input.records(Kind::SortDecision).is_empty());
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn outbound_trace_pressure_keeps_one_ticket_and_cancellation_never_releases_use() {
    let (mut fx, input, certificate) = prepared("Read ");
    let acts = input.acts.clone();
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    fx.input.fill_trace();
    let now = fx.input.run.clock.now().unwrap();
    fx.input
        .run
        .context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(now, 1).unwrap(),
            &mut fx.input.run.clock,
        )
        .unwrap();
    assert!(matches!(
        fx.input
            .run
            .context
            .advance_outbound(&mut ports(&mut fx.input.run.clock)),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    assert!(matches!(
        fx.input.run.context.take_outbound(&mut fx.input.run.clock),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    assert!(
        fx.input
            .run
            .context
            .take_filing(&mut fx.input.run.clock)
            .is_err()
    );
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    fx.input.run.context.cancel_outbound();
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    fx.input.drain_trace();
    let OutboundCompletion::Cancelled { input, .. } = complete(&mut fx) else {
        panic!("cancelled ticket granted use");
    };
    assert_eq!(input.acts, acts);
    assert_eq!(fx.input.records(Kind::SortDecision).len(), 1);
    assert!(fx.input.records(Kind::ToolCallStarted).is_empty());
}

#[test]
fn outbound_retains_input_at_the_original_deadline_without_a_grant() {
    let (mut fx, input, certificate) = prepared("Read ");
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    fx.input.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(
        fx.input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .is_err()
    );
    assert!(
        fx.input
            .run
            .context
            .filing
            .as_ref()
            .unwrap()
            .outbound
            .is_some()
    );
    assert!(fx.input.records(Kind::SortDecision).is_empty());
}

#[test]
fn outbound_busy_returns_second_owner_and_foreign_source_leases_never_grant_use() {
    let (mut fx, first, certificate) = prepared("Read ");
    let (_, second, _) = prepared("Read ");
    let error = fx
        .input
        .run
        .context
        .begin_outbound(second, &certificate, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(
        error.invalid_candidate,
        Some(crate::flow::sort::OutboundRefusal::InvalidCandidate)
    );
    fx.input
        .run
        .context
        .begin_outbound(first, &certificate, &mut fx.input.run.clock)
        .unwrap();
    let acts = error.input.acts.clone();
    let error = fx
        .input
        .run
        .context
        .begin_outbound(error.input, &certificate, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, Some(PairBeginRefusal::Busy));
    assert_eq!(error.input.acts, acts);
    assert!(matches!(complete(&mut fx), OutboundCompletion::Sorted(_)));
}

#[test]
fn outbound_cancelled_runtime_rejection_keeps_its_owed_fault_under_pressure() {
    let (mut fx, input, certificate) = prepared("bad ");
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    advance(&mut fx, false); // The original decision is recorded.
    for _ in 0..128 {
        fx.input
            .run
            .context
            .reduce_flow(&mut fx.input.run.clock)
            .unwrap();
        fx.checker();
        fx.member(true);
        if fx
            .input
            .run
            .context
            .flow
            .filing_receipts()
            .is_some_and(|receipts| receipts.is_complete())
        {
            break;
        }
    }
    assert!(
        fx.input
            .run
            .context
            .flow
            .filing_receipts()
            .unwrap()
            .is_complete()
    );
    // Filing is complete, but the outbound fault is still owed under this bound.
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    fx.input.fill_trace();
    let now = fx.input.run.clock.now().unwrap();
    fx.input
        .run
        .context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(now, 1).unwrap(),
            &mut fx.input.run.clock,
        )
        .unwrap();
    assert!(matches!(
        fx.input
            .run
            .context
            .advance_outbound(&mut ports(&mut fx.input.run.clock)),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    assert!(fx.input.records(Kind::Fault).is_empty());
    fx.input.run.context.cancel_outbound();
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.input.drain_trace();
    let OutboundCompletion::Cancelled { fault, .. } = complete(&mut fx) else {
        panic!("cancelled rejection granted use");
    };
    assert!(fault.is_some());
    assert_eq!(fx.input.records(Kind::Fault).len(), 1);
    assert_eq!(fx.input.records(Kind::SortDecision).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn outbound_resumes_its_recorded_fault_completion_before_releasing_the_owner() {
    let (mut fx, input, certificate) = prepared("bad ");
    fx.input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap();
    for _ in 0..128 {
        if advance(&mut fx, true) == OutboundProgress::Complete {
            break;
        }
    }
    let faults = fx.input.records(Kind::Fault);
    assert_eq!(faults.len(), 1);
    // Reproduce the exact post-Recorded state left when wait_recorded yields a
    // lifecycle wait before transferring the completed author to its consumer.
    let recorded = Sequence(sequence(&faults[0]));
    fx.input.run.context.completed_author = Some(recorded);
    assert!(
        fx.input
            .run
            .context
            .take_outbound(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert_eq!(advance(&mut fx, true), OutboundProgress::Complete);
    let OutboundCompletion::Refused { fault, .. } = fx
        .input
        .run
        .context
        .take_outbound(&mut fx.input.run.clock)
        .unwrap()
        .unwrap()
    else {
        panic!("runtime rejection granted use");
    };
    assert_eq!(fault, recorded);
    assert!(fx.input.run.context.completed_author.is_none());
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_some()
    );
    assert_eq!(fx.input.records(Kind::Fault).len(), 1);
}

#[test]
fn outbound_invalid_source_returns_the_complete_input_without_record_or_halt() {
    let (mut fx, mut input, certificate) = prepared("Read ");
    input.source.turn = Some("foreign".into());
    let acts = input.acts.clone();
    let error = fx
        .input
        .run
        .context
        .begin_outbound(input, &certificate, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(
        error.invalid_candidate,
        Some(crate::flow::sort::OutboundRefusal::InvalidCandidate)
    );
    assert_eq!(error.input.acts, acts);
    assert!(error.failure.is_none());
    assert!(fx.input.records(Kind::SortDecision).is_empty());
    assert!(fx.input.run.context.failure().is_none());
}
