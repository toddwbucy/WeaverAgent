//! Real control/recorder ownership, with explicitly defensive association probes.

use super::*;

fn recorded_control(member: &mut Member) {
    for _ in 0..128 {
        let context = &mut member.run.context;
        if context.flow.operation_completion().is_some() {
            return;
        }
        context
            .flow
            .advance(
                &mut FlowRecording {
                    barrier: &mut context.barrier,
                    inbox: &mut context.inbox,
                    budget: &mut context.budget,
                    author: &context.author,
                },
                &mut member.run.clock,
                1,
            )
            .unwrap();
    }
    panic!("actual control did not become Recorded");
}

#[test]
fn inspection_defensive_associations_inspect_actual_recorded_completion() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Shape);
    let answer = member.answer(&request, s::StateControlAnswer::Shape(vec![]));
    member.receive(&answer).unwrap();
    recorded_control(&mut member);
    inspection_driver::validation_tests::associations(
        member.run.context.flow.operation_completion().unwrap(),
    );
    let InspectionCompletion::Shape { evidence, .. } = finish(&mut member) else {
        panic!("original proof must still complete");
    };
    assert_eq!(encoded(evidence.answer()), encoded(&answer));
    assert_eq!(member.requests.len(), 1);
}

#[test]
fn inspection_retire_and_settle_reject_changed_expected_identity_without_taking_proof() {
    use inspection_driver::validation_tests::{OwnerMismatch, changed_owner};

    for settle in [false, true] {
        for mismatch in [OwnerMismatch::RequestId, OwnerMismatch::Turn] {
            let mut member = Member::new();
            let request = begin(&mut member, InspectionKind::Shape);
            let bound = member.bound();
            let answer = member.answer(&request, s::StateControlAnswer::Shape(vec![]));
            member.receive(&answer).unwrap();
            recorded_control(&mut member);
            let error = changed_owner(
                &mut member.run.context,
                &mut member.run.clock,
                mismatch,
                settle,
            )
            .unwrap_err();
            assert!(
                matches!(
                    error,
                    RunFailure::Flow {
                        reason: FlowHaltReason::ProtocolFailed,
                        ..
                    }
                ),
                "{mismatch:?}, settle={settle}: {error:?}"
            );
            assert_eq!(member.run.context.failure(), Some(&error));
            assert!(member.run.context.inspection.is_some());
            let Some(OperationCompletion::State { completion, .. }) =
                member.run.context.flow.operation_completion()
            else {
                panic!("mismatched ownership must not consume the original proof");
            };
            assert_eq!(encoded(&completion.answer), encoded(&answer));
            assert_eq!(
                member.run.context.deadlines.get(PumpDeadline::StateControl),
                (!settle).then_some(bound)
            );
            assert_eq!(member.requests.len(), 1);
            member.no_request();
        }
    }
}

#[test]
fn inspection_external_take_cannot_consume_actual_ready_control_completion() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Grants);
    let answer = member.answer(
        &request,
        s::StateControlAnswer::Grants {
            surface: vec!["retained".into()],
        },
    );
    member.receive(&answer).unwrap();
    recorded_control(&mut member);
    for _ in 0..2 {
        assert!(member.run.context.flow.operation_completion().is_some());
        assert_eq!(
            member
                .run
                .context
                .take_operation_completion(&mut member.run.clock)
                .unwrap_err(),
            RunFailure::InspectionOwned
        );
        assert!(member.run.context.flow.operation_completion().is_some());
    }
    let InspectionCompletion::Grants { surface, evidence } = finish(&mut member) else {
        panic!("inspection owns the control");
    };
    assert_eq!(surface, vec!["retained"]);
    assert_eq!(encoded(evidence.answer()), encoded(&answer));
    assert!(member.run.context.flow.operation_completion().is_none());
    assert!(
        member
            .run
            .context
            .take_operation_completion(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn inspection_first_control_busy_returns_inputs_without_owner_or_wait() {
    let mut member = Member::new();
    let turn = Some(TurnRef("original-turn".into()));
    // Admission excludes every ordinary synchronous Busy cause. This hook
    // exercises the defensive returned-Busy boundary without a fake operation.
    member.run.context.flow.defer_control_once_for_test(
        (s::StateControl::Grants {}, turn.clone())
            .try_into()
            .unwrap(),
    );
    let error = member
        .run
        .context
        .begin_inspection(InspectionKind::Grants, turn.clone(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, InspectionKind::Grants);
    assert_eq!(error.turn, turn);
    assert_eq!(error.refusal, InspectionBeginRefusal::Busy);
    assert!(error.failure.is_none() && !error.retained);
    assert!(member.run.context.inspection.is_none());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert!(member.run.context.flow.pending_control().is_none());
    assert!(member.run.context.flow.state().outstanding().is_empty());
    member.no_request();
    member
        .run
        .context
        .begin_inspection(error.kind, error.turn, &mut member.run.clock)
        .unwrap();
    let request = member.sent();
    member.respond(&request, s::StateControlAnswer::Grants { surface: vec![] });
    assert!(matches!(
        finish(&mut member),
        InspectionCompletion::Grants { .. }
    ));
    assert_eq!(member.requests.len(), 1);
}

fn suspended_before_allocation() -> (Member, AbsoluteBound, Option<TurnRef>) {
    let mut member = Member::new();
    let now = member.now();
    let label = AbsoluteBound::after(now, 20).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Label, label, &mut member.run.clock)
        .unwrap();
    let turn = Some(TurnRef("withdraw-original".into()));
    let mut clock = JumpClock {
        before: now,
        after: label.deadline_ns(),
        allowed: 1,
        reads: 0,
    };
    let error = member
        .run
        .context
        .begin_inspection(InspectionKind::Grants, turn.clone(), &mut clock)
        .unwrap_err();
    assert!(error.retained);
    assert_eq!(error.kind, InspectionKind::Grants);
    assert_eq!(error.turn, turn);
    assert_eq!(
        error.failure,
        Some(RunFailure::Wait {
            key: PumpDeadline::Label,
            bound: label
        })
    );
    assert_eq!(clock.reads, 2, "yield before begin_control allocates an id");
    assert!(
        member
            .run
            .context
            .inspection
            .as_ref()
            .unwrap()
            .request_id_for_test()
            .is_none()
    );
    assert!(member.run.context.flow.state().outstanding().is_empty());
    assert!(member.run.context.flow.pending_control().is_none());
    let bound = member.bound();
    assert_eq!(bound.began_ns(), now);
    assert_eq!(bound.deadline_ns(), now + CONTROL_NS);
    member.run.clock.advance_to(label.deadline_ns()).unwrap();
    assert_eq!(
        member.run.context.complete_wait(PumpDeadline::Label),
        Some(label)
    );
    (member, bound, turn)
}

#[test]
fn inspection_cancel_after_preallocation_label_wait_withdraws_without_control_or_proof() {
    let (mut member, bound, turn) = suspended_before_allocation();
    let before = member.records(Kind::StateStoreMeasurement).len();
    for _ in 0..2 {
        member
            .run
            .context
            .cancel_inspection(&mut member.run.clock)
            .unwrap();
        assert_eq!(advance(&mut member).unwrap(), InspectionProgress::Complete);
        assert!(
            member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
    }
    let Some(InspectionCompletion::Withdrawn {
        request,
        turn: retained_turn,
        bound: retained_bound,
    }) = member
        .run
        .context
        .take_inspection(&mut member.run.clock)
        .unwrap()
    else {
        panic!("unallocated cancellation must not manufacture evidence");
    };
    assert_eq!(request, s::StateControl::Grants {});
    assert_eq!(retained_turn, turn);
    assert_eq!(retained_bound, bound);
    assert!(member.run.context.inspection.is_none());
    assert!(member.run.context.flow.pending_control().is_none());
    assert!(member.run.context.flow.state().outstanding().is_empty());
    assert!(member.run.context.flow.operation_completion().is_none());
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), before);
    assert!(
        member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    member.no_request();
    let next = AbsoluteBound::after(member.now(), 100).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::StateControl, next, &mut member.run.clock)
        .unwrap();
    assert!(
        member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        member.run.context.deadlines.get(PumpDeadline::StateControl),
        Some(next)
    );
}

#[test]
fn inspection_cancel_before_allocation_cannot_disarm_a_replaced_wait() {
    let (mut member, original, _) = suspended_before_allocation();
    // Ordinary complete_wait refuses to steal the owned interval. Deliberately
    // replace it below to pin the private invariant check, not a healthy caller.
    assert!(
        member
            .run
            .context
            .complete_wait(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(
        member
            .run
            .context
            .deadlines
            .disarm(PumpDeadline::StateControl),
        Some(original)
    );
    let replacement = AbsoluteBound::after(member.now(), 100).unwrap();
    member
        .run
        .context
        .deadlines
        .arm(PumpDeadline::StateControl, replacement)
        .unwrap();
    assert!(matches!(
        member.run.context.cancel_inspection(&mut member.run.clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_eq!(
        member.run.context.deadlines.get(PumpDeadline::StateControl),
        Some(replacement)
    );
    assert!(member.run.context.inspection.is_some());
    assert!(member.run.context.flow.state().outstanding().is_empty());
}

#[test]
fn inspection_withdrawn_original_blocks_replacement_until_atomic_final_take() {
    let (mut member, bound, turn) = suspended_before_allocation();
    let before = member.records(Kind::StateStoreMeasurement).len();
    member
        .run
        .context
        .cancel_inspection(&mut member.run.clock)
        .unwrap();
    let replacement_turn = Some(TurnRef("replacement".into()));
    let refused = member
        .run
        .context
        .begin_inspection(
            InspectionKind::Shape,
            replacement_turn.clone(),
            &mut member.run.clock,
        )
        .unwrap_err();
    assert_eq!(refused.kind, InspectionKind::Shape);
    assert_eq!(refused.turn, replacement_turn);
    assert_eq!(refused.refusal, InspectionBeginRefusal::Busy);
    assert!(!refused.retained && refused.failure.is_none());
    let read = member
        .run
        .context
        .begin_read(selector(), replacement_turn.clone(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(read.refusal, ReadBeginRefusal::Busy);
    assert_eq!(read.selector, selector());
    assert_eq!(read.turn, replacement_turn);
    assert_eq!(
        member
            .run
            .context
            .take_operation_completion(&mut member.run.clock)
            .unwrap_err(),
        RunFailure::InspectionOwned
    );
    assert_eq!(advance(&mut member).unwrap(), InspectionProgress::Complete);
    let now = member.now();
    let outer = AbsoluteBound::after(now, 20).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, outer, &mut member.run.clock)
        .unwrap();
    let mut clock = JumpClock {
        before: now,
        after: outer.deadline_ns(),
        allowed: 1,
        reads: 0,
    };
    let Some(InspectionCompletion::Withdrawn {
        request,
        turn: retained_turn,
        bound: retained_bound,
    }) = member.run.context.take_inspection(&mut clock).unwrap()
    else {
        panic!("the original unallocated inspection must remain owned until take");
    };
    assert_eq!(request, s::StateControl::Grants {});
    assert_eq!(retained_turn, turn);
    assert_eq!(retained_bound, bound);
    assert_eq!(
        clock.reads, 1,
        "no fallible work after transferring custody"
    );
    assert!(member.run.context.inspection.is_none());
    assert!(member.run.context.flow.state().outstanding().is_empty());
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), before);
    assert_eq!(
        member.run.context.take_inspection(&mut clock).unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: outer
        }
    );
    member.no_request();
}

#[test]
fn inspection_and_read_refuse_a_real_pressured_author_without_consuming_its_ticket() {
    let mut member = Member::new();
    member.fill_trace();
    let label = AbsoluteBound::after(member.now(), 100).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Label, label, &mut member.run.clock)
        .unwrap();
    let mut event = super::super::super::tests::event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({"held":true})).unwrap(),
        )),
    );
    event.turn = Some(TurnRef("held-author".into()));
    assert_eq!(
        member
            .run
            .context
            .author_and_wait(&mut ports(&mut member.run.clock), event)
            .map_err(|failure| failure.retained_reason())
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Label,
            bound: label
        }
    );
    let ticket = member.run.context.pending_author().unwrap().ticket;
    assert!(member.run.context.barrier.pending().is_some());
    assert_eq!(
        member.run.context.complete_wait(PumpDeadline::Label),
        Some(label)
    );
    let turn = Some(TurnRef("other-reader".into()));
    let error = member
        .run
        .context
        .begin_inspection(InspectionKind::Shape, turn.clone(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, InspectionBeginRefusal::Busy);
    assert_eq!(error.turn, turn);
    assert!(!error.retained && error.failure.is_none());
    let error = member
        .run
        .context
        .begin_read(selector(), turn.clone(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, ReadBeginRefusal::Busy);
    assert_eq!(error.selector, selector());
    assert_eq!(error.turn, turn);
    assert!(error.failure.is_none());
    assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
    assert!(member.run.context.inspection.is_none() && member.run.context.read.is_none());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    member.no_request();
    super::super::recall_tests::drain_admitted(&mut member);
    member
        .run
        .context
        .resume_author_and_wait(&mut ports(&mut member.run.clock))
        .map_err(|failure| failure.retained_reason())
        .unwrap();
    assert!(member.run.context.pending_author().is_none());
}

#[test]
fn inspection_and_read_refuse_a_reserved_filing_consumer_before_its_author() {
    let mut member = Member::new();
    let context = &mut member.run.context;
    context
        .flow
        .reserve_filing_receipts(&mut FlowRecording {
            barrier: &mut context.barrier,
            inbox: &mut context.inbox,
            budget: &mut context.budget,
            author: &context.author,
        })
        .unwrap();
    assert!(context.flow.has_filing_consumer());
    assert!(context.filing.is_none());
    assert!(context.barrier.pending().is_none());
    assert!(context.barrier.pending_author().is_none());
    assert!(!context.flow.has_pending_record());
    let turn = Some(TurnRef("reserved-filing".into()));
    let error = context
        .begin_inspection(InspectionKind::Grants, turn.clone(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, InspectionBeginRefusal::Busy);
    assert_eq!(error.turn, turn);
    assert!(!error.retained && error.failure.is_none());
    let error = context
        .begin_read(selector(), turn.clone(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, ReadBeginRefusal::Busy);
    assert_eq!(error.turn, turn);
    assert_eq!(error.selector, selector());
    assert!(error.failure.is_none());
    assert!(context.flow.has_filing_consumer());
    assert!(context.inspection.is_none() && context.read.is_none());
    assert!(context.flow.state().outstanding().is_empty());
    assert!(context.deadlines.get(PumpDeadline::StateControl).is_none());
    member.no_request();
}
