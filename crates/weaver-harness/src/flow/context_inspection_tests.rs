//! Actual state frames and the existing bounded trace writer, with no forged
//! Recorded proof or production permission/placement constructor.
use super::*;
use crate::flow::read::ReadFailure;

fn begin(member: &mut Member, kind: InspectionKind) -> s::StateFrame {
    member
        .run
        .context
        .begin_inspection(kind, None, &mut member.run.clock)
        .unwrap();
    member.sent()
}
fn advance(member: &mut Member) -> Result<InspectionProgress, RunFailure> {
    member
        .run
        .context
        .advance_inspection(&mut ports(&mut member.run.clock))
}
fn finish(member: &mut Member) -> InspectionCompletion {
    for _ in 0..128 {
        advance(member).unwrap();
        if let Some(result) = member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
        {
            return result;
        }
    }
    panic!("inspection did not finish");
}
fn encoded(answer: &s::StateFrame) -> Vec<u8> {
    s::encode_state_line(answer, s::FrameDirection::Response).unwrap()
}

#[test]
fn inspection_shape_preserves_first_run_kind_order_full_counts_and_exact_answer() {
    let mut member = Member::new();
    let turn = Some(TurnRef("inspect".into()));
    member
        .run
        .context
        .begin_inspection(InspectionKind::Shape, turn.clone(), &mut member.run.clock)
        .unwrap();
    let request = member.sent();
    let bound = member.bound();
    let before = member.records(Kind::StateStoreMeasurement).len();
    let rows = [
        ("second", "message.user", u64::MAX),
        ("first", "message.assistant", 0),
        ("second", "message.tool_result", 7),
        ("first", "message.user", 9),
    ]
    .into_iter()
    .map(|(run, kind, count)| s::ShapeRow {
        run: run.into(),
        kind: kind.into(),
        count: count.into(),
    })
    .collect();
    let answer = member.answer(&request, s::StateControlAnswer::Shape(rows));
    member.receive(&answer).unwrap();
    assert!(
        member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    let InspectionCompletion::Shape { shape, evidence } = finish(&mut member) else {
        panic!("shape");
    };
    assert_eq!(
        shape.runs,
        vec![
            crate::state::RunShape {
                run: "second".into(),
                kinds: vec![
                    ("message.user".into(), u64::MAX),
                    ("message.tool_result".into(), 7)
                ]
            },
            crate::state::RunShape {
                run: "first".into(),
                kinds: vec![("message.assistant".into(), 0), ("message.user".into(), 9)]
            },
        ]
    );
    assert_eq!(evidence.request(), &s::StateControl::Shape {});
    assert_eq!(encoded(evidence.answer()), encoded(&answer));
    assert_eq!(evidence.turn(), turn.as_ref());
    assert_eq!(evidence.bound(), bound);
    assert!(!bound.expired(evidence.recorded_ns()));
    assert_eq!(
        member.records(Kind::StateStoreMeasurement).len(),
        before + 1
    );
    assert!(member.records(Kind::Recall).is_empty());
    assert!(
        member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(member.run.context.flow.ledger().facts().next().is_none());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    member.no_request();
}

#[test]
fn inspection_empty_success_and_exact_typed_refusal_remain_distinct() {
    for kind in [InspectionKind::Shape, InspectionKind::Grants] {
        for refusal in [false, true] {
            let mut member = Member::new();
            let request = begin(&mut member, kind);
            let frame = if refusal {
                s::StateControlAnswer::ControlRefused {
                    code: s::ControlRefusalCode::Read(s::ReadRefusalCode::AnswerTooLarge),
                    detail: "exact store detail".into(),
                }
            } else {
                match kind {
                    InspectionKind::Shape => s::StateControlAnswer::Shape(vec![]),
                    InspectionKind::Grants => s::StateControlAnswer::Grants { surface: vec![] },
                }
            };
            let answer = member.answer(&request, frame);
            member.receive(&answer).unwrap();
            match finish(&mut member) {
                InspectionCompletion::Shape { shape, .. }
                    if !refusal && kind == InspectionKind::Shape =>
                {
                    assert!(shape.runs.is_empty())
                }
                InspectionCompletion::Grants { surface, .. }
                    if !refusal && kind == InspectionKind::Grants =>
                {
                    assert!(surface.is_empty())
                }
                InspectionCompletion::Refused {
                    failure: ReadFailure::Refused { code, detail },
                    evidence,
                } if refusal => {
                    assert_eq!(code, s::ReadRefusalCode::AnswerTooLarge);
                    assert_eq!(detail, "exact store detail");
                    assert_eq!(encoded(evidence.answer()), encoded(&answer));
                }
                other => panic!("unexpected inspection: {other:?}"),
            }
            assert!(member.run.context.failure().is_none());
            assert_eq!(member.requests.len(), 1);
            member.no_request();
        }
    }
}

#[test]
fn inspection_grants_preserve_the_observed_surface_without_a_fresh_grant() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Grants);
    let surface = vec!["shell:/tmp/work".to_owned(), "unknown:exact".into()];
    let answer = member.answer(
        &request,
        s::StateControlAnswer::Grants {
            surface: surface.clone(),
        },
    );
    member.receive(&answer).unwrap();
    let InspectionCompletion::Grants {
        surface: got,
        evidence,
    } = finish(&mut member)
    else {
        panic!("grants");
    };
    assert_eq!(got, surface);
    assert_eq!(encoded(evidence.answer()), encoded(&answer));
    assert_eq!(member.requests, vec![s::StateControl::Grants {}]);
    assert!(member.records(Kind::Recall).is_empty());
    assert!(member.run.context.flow.ledger().facts().next().is_none());
}

#[test]
fn inspection_trace_backpressure_retains_one_answer_and_original_bound_until_recorded() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Grants);
    let bound = member.bound();
    let before = member.records(Kind::StateStoreMeasurement).len();
    member.fill_trace();
    let answer = member.answer(
        &request,
        s::StateControlAnswer::Grants {
            surface: vec!["one".into()],
        },
    );
    member.receive(&answer).unwrap();
    let held = member
        .run
        .context
        .flow
        .state()
        .held_measurement()
        .unwrap()
        .clone();
    for _ in 0..8 {
        advance(&mut member).unwrap();
        assert!(
            member
                .run
                .context
                .take_inspection(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            member
                .run
                .context
                .take_operation_completion(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), before);
    assert_eq!(
        member.run.context.flow.state().held_measurement(),
        Some(&held)
    );
    assert_eq!(member.bound(), bound);
    assert_eq!(
        member.run.context.complete_wait(PumpDeadline::StateControl),
        None
    );
    member.no_request();
    super::recall_tests::drain_admitted(&mut member);
    let InspectionCompletion::Grants { evidence, .. } = finish(&mut member) else {
        panic!("recorded grants");
    };
    assert_eq!(encoded(evidence.answer()), encoded(&answer));
    assert_eq!(
        member.records(Kind::StateStoreMeasurement).len(),
        before + 1
    );
    assert_eq!(member.requests.len(), 1);
}

#[test]
fn inspection_cancellation_drains_before_send_and_under_trace_pressure_without_release() {
    for before_send in [false, true] {
        for refusal in [false, true] {
            let mut member = Member::new();
            member
                .run
                .context
                .begin_inspection(InspectionKind::Shape, None, &mut member.run.clock)
                .unwrap();
            let bound = member.bound();
            if before_send {
                assert_eq!(
                    member.run.context.flow.state().outstanding()[0].bytes_sent,
                    0
                );
                member
                    .run
                    .context
                    .cancel_inspection(&mut member.run.clock)
                    .unwrap();
            }
            let request = member.sent();
            member.fill_trace();
            let frame = if refusal {
                refused(s::ReadRefusalCode::ResultUnavailable)
            } else {
                s::StateControlAnswer::Shape(vec![])
            };
            let answer = member.answer(&request, frame);
            member.receive(&answer).unwrap();
            advance(&mut member).unwrap();
            member
                .run
                .context
                .cancel_inspection(&mut member.run.clock)
                .unwrap();
            assert!(
                member
                    .run
                    .context
                    .take_inspection(&mut member.run.clock)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(member.bound(), bound);
            super::recall_tests::drain_admitted(&mut member);
            let InspectionCompletion::Cancelled(evidence) = finish(&mut member) else {
                panic!("cancelled");
            };
            assert_eq!(encoded(evidence.answer()), encoded(&answer));
            assert_eq!(evidence.bound(), bound);
            assert_eq!(member.requests, vec![s::StateControl::Shape {}]);
            member.no_request();
        }
    }
}

#[test]
fn inspection_trace_pressure_cannot_renew_its_control_deadline() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Shape);
    let bound = member.bound();
    assert_eq!(bound.deadline_ns() - bound.began_ns(), CONTROL_NS);
    member.fill_trace();
    member.respond(&request, s::StateControlAnswer::Shape(vec![]));
    advance(&mut member).unwrap();
    member
        .run
        .context
        .cancel_inspection(&mut member.run.clock)
        .unwrap();
    member.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        advance(&mut member),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        })
    ));
    assert!(
        member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .is_err()
    );
    assert_eq!(
        member.run.context.halt_wait().unwrap().deadline_ns.0,
        bound.deadline_ns()
    );
}

#[test]
fn inspection_timely_recorded_completion_survives_later_consumer_scheduling() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Shape);
    let bound = member.bound();
    let answer = member.answer(&request, s::StateControlAnswer::Shape(vec![]));
    member.receive(&answer).unwrap();
    member.record_completion_only();
    let recorded_at = member.now();
    member
        .run
        .clock
        .advance_to(bound.deadline_ns() + 1)
        .unwrap();
    let InspectionCompletion::Shape { evidence, .. } = finish(&mut member) else {
        panic!("timely answer");
    };
    assert!(evidence.recorded_ns() < bound.deadline_ns());
    assert_eq!(evidence.recorded_ns(), recorded_at);
    assert_eq!(encoded(evidence.answer()), encoded(&answer));
    assert!(member.run.context.failure().is_none());
}

#[test]
fn inspection_interrupted_release_retires_only_its_timely_recorded_wait() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Grants);
    let bound = member.bound();
    member
        .run
        .context
        .flow
        .interrupt_next_state_release_for_test();
    member.respond(
        &request,
        s::StateControlAnswer::Grants {
            surface: vec!["observed".into()],
        },
    );
    advance(&mut member).unwrap();
    assert!(
        member
            .run
            .context
            .flow
            .control_recorded_boundary()
            .is_some()
    );
    assert!(member.run.context.flow.operation_completion().is_none());
    assert!(member.run.context.flow.state().held_measurement().is_some());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert!(
        member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    let before = member.records(Kind::StateStoreMeasurement).len();
    member
        .run
        .clock
        .advance_to(bound.deadline_ns() + 1)
        .unwrap();
    let InspectionCompletion::Grants { evidence, .. } = finish(&mut member) else {
        panic!("grants after final probe");
    };
    assert!(evidence.recorded_ns() < bound.deadline_ns());
    assert_eq!(member.records(Kind::StateStoreMeasurement).len(), before);
}

#[test]
fn inspection_busy_admission_preserves_gate_cut_and_other_state_control_owners() {
    for key in [
        PumpDeadline::Gate,
        PumpDeadline::Cut,
        PumpDeadline::StateControl,
    ] {
        let mut member = Member::new();
        let bound = AbsoluteBound::after(member.now(), 10_000).unwrap();
        member
            .run
            .context
            .arm(key, bound, &mut member.run.clock)
            .unwrap();
        let turn = Some(TurnRef("unchanged".into()));
        let refusal = member
            .run
            .context
            .begin_inspection(InspectionKind::Shape, turn.clone(), &mut member.run.clock)
            .unwrap_err();
        assert_eq!(refusal.kind, InspectionKind::Shape);
        assert_eq!(refusal.turn, turn);
        assert_eq!(refusal.refusal, InspectionBeginRefusal::Busy);
        assert!(refusal.failure.is_none());
        assert!(member.run.context.inspection.is_none());
        assert_eq!(member.run.context.deadlines.get(key), Some(bound));
        assert!(member.run.context.flow.state().outstanding().is_empty());
        member.no_request();
    }
}

#[test]
fn inspection_cannot_replace_a_pending_or_complete_untaken_owner() {
    for complete in [false, true] {
        let mut member = Member::new();
        let request = begin(&mut member, InspectionKind::Shape);
        if complete {
            member.respond(&request, s::StateControlAnswer::Shape(vec![]));
            assert_eq!(advance(&mut member).unwrap(), InspectionProgress::Complete);
        }
        let refusal = member
            .run
            .context
            .begin_inspection(InspectionKind::Grants, None, &mut member.run.clock)
            .unwrap_err();
        assert_eq!(refusal.refusal, InspectionBeginRefusal::Busy);
        assert!(refusal.failure.is_none());
        if !complete {
            member.respond(&request, s::StateControlAnswer::Shape(vec![]));
        }
        assert!(matches!(
            finish(&mut member),
            InspectionCompletion::Shape { .. }
        ));
        assert_eq!(member.requests.len(), 1);
    }
}

#[test]
fn inspection_excludes_read_input_author_and_freshness_until_final_take() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Shape);
    member.respond(&request, s::StateControlAnswer::Shape(vec![]));
    assert_eq!(advance(&mut member).unwrap(), InspectionProgress::Complete);
    let refusal = member
        .run
        .context
        .begin_read(selector(), None, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(refusal.refusal, ReadBeginRefusal::Busy);
    let frame = weaver_types::TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
    let error = member
        .run
        .context
        .begin_input(frame, &mut member.run.clock)
        .unwrap_err();
    assert!(error.failure.is_none());
    assert!(member.run.context.input.snapshot().is_none());
    let error = member
        .run
        .context
        .prepare_cut(vec![], &mut member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, FreshnessBeginRefusal::Busy);
    let event = super::super::tests::event(Kind::TurnStarted, None);
    assert_eq!(
        member
            .run
            .context
            .author_and_wait(&mut ports(&mut member.run.clock), event)
            .unwrap_err(),
        RunFailure::InspectionOwned
    );
    assert_eq!(
        member.run.context.refused_author().unwrap().kind,
        Kind::TurnStarted
    );
    assert!(
        member
            .run
            .context
            .take_operation_completion(&mut member.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        finish(&mut member),
        InspectionCompletion::Shape { .. }
    ));
    assert!(member.run.context.failure().is_none());
}

#[test]
fn inspection_cannot_enter_during_a_real_read() {
    let mut member = Member::new();
    let _request = member.begin();
    let bound = member.bound();
    let refusal = member
        .run
        .context
        .begin_inspection(InspectionKind::Grants, None, &mut member.run.clock)
        .unwrap_err();
    assert_eq!(refusal.refusal, InspectionBeginRefusal::Busy);
    assert!(refusal.failure.is_none());
    assert!(member.run.context.read.is_some());
    assert!(member.run.context.inspection.is_none());
    assert_eq!(member.bound(), bound);
    assert_eq!(member.requests.len(), 1);
}

#[test]
fn inspection_wrong_answer_or_eof_latches_failure_instead_of_empty_success() {
    for eof in [false, true] {
        let mut member = Member::new();
        let request = begin(&mut member, InspectionKind::Shape);
        let result = if eof {
            member.run.state.shutdown(Shutdown::Both).unwrap();
            let now = member.now() + 1;
            member.pump_at(now, PollFlags::POLLHUP)
        } else {
            let answer = member.answer(&request, s::StateControlAnswer::Grants { surface: vec![] });
            member.receive(&answer)
        };
        let failure = result
            .err()
            .unwrap_or_else(|| advance(&mut member).unwrap_err());
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::CarryFailed | FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        assert!(
            member
                .run
                .context
                .take_inspection(&mut member.run.clock)
                .is_err()
        );
        assert!(member.run.context.failure().is_some());
    }
}

#[test]
fn inspection_retains_original_enter_or_leave_cap_without_extending_the_control() {
    for key in [PumpDeadline::Enter, PumpDeadline::Leave] {
        let mut member = Member::new();
        let outer = AbsoluteBound::after(member.now(), 20).unwrap();
        member
            .run
            .context
            .arm(key, outer, &mut member.run.clock)
            .unwrap();
        let _request = begin(&mut member, InspectionKind::Shape);
        let bound = member.bound();
        assert_eq!(bound.deadline_ns(), outer.deadline_ns());
        member.run.clock.advance_to(outer.deadline_ns()).unwrap();
        assert_eq!(
            advance(&mut member).unwrap_err(),
            RunFailure::Wait { key, bound: outer }
        );
        assert!(member.run.context.inspection.is_some());
        assert_eq!(member.run.context.complete_wait(key), Some(outer));
        assert!(matches!(
            advance(&mut member),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ControlBound,
                ..
            })
        ));
        assert_eq!(member.requests.len(), 1);
    }
}

#[test]
fn inspection_taking_a_recorded_answer_preserves_a_later_outer_lifecycle_wait() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Shape);
    member.respond(&request, s::StateControlAnswer::Shape(vec![]));
    member.record_completion_only();
    let outer = AbsoluteBound::after(member.now(), 20).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, outer, &mut member.run.clock)
        .unwrap();
    member.run.clock.advance_to(outer.deadline_ns()).unwrap();
    assert_eq!(
        advance(&mut member).unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: outer
        }
    );
    assert!(member.run.context.flow.operation_completion().is_some());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert_eq!(
        member.run.context.complete_wait(PumpDeadline::Leave),
        Some(outer)
    );
    assert!(matches!(
        finish(&mut member),
        InspectionCompletion::Shape { .. }
    ));
    assert_eq!(member.requests.len(), 1);
}

struct JumpClock {
    before: u64,
    after: u64,
    allowed: usize,
    reads: usize,
}
impl PumpIo for JumpClock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.reads += 1;
        Ok(if self.reads <= self.allowed {
            self.before
        } else {
            self.after
        })
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("local ownership handoff never polls");
    }
}

#[test]
fn inspection_admission_retains_allocated_identity_before_post_enqueue_leave() {
    let mut member = Member::new();
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
        allowed: 3,
        reads: 0,
    };
    let error = member
        .run
        .context
        .begin_inspection(InspectionKind::Grants, None, &mut clock)
        .unwrap_err();
    assert_eq!(
        error.failure,
        Some(RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: outer
        })
    );
    assert_eq!(clock.reads, 4, "yield occurs immediately after enqueue");
    let expected = member.run.context.flow.state().outstanding()[0]
        .common
        .request_id;
    assert_eq!(
        member
            .run
            .context
            .inspection
            .as_ref()
            .unwrap()
            .request_id_for_test(),
        Some(expected)
    );
    assert_eq!(member.run.context.flow.state().outstanding().len(), 1);
    assert_eq!(
        member.run.context.flow.state().outstanding()[0].bytes_sent,
        0
    );
    assert_eq!(member.bound().deadline_ns(), outer.deadline_ns());
    member.no_request();
}

#[test]
fn inspection_take_does_not_yield_after_transferring_the_recorded_answer() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Grants);
    member.respond(
        &request,
        s::StateControlAnswer::Grants {
            surface: vec!["exact".into()],
        },
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
    assert!(matches!(
        member.run.context.take_inspection(&mut clock).unwrap(),
        Some(InspectionCompletion::Grants { .. })
    ));
    assert_eq!(clock.reads, 1);
    assert!(member.run.context.inspection.is_none());
    assert_eq!(
        member.run.context.take_inspection(&mut clock).unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: outer
        }
    );
}

#[test]
fn inspection_release_cannot_discard_a_completed_untaken_answer() {
    let mut member = Member::new();
    let request = begin(&mut member, InspectionKind::Grants);
    member.respond(&request, s::StateControlAnswer::Grants { surface: vec![] });
    assert_eq!(advance(&mut member).unwrap(), InspectionProgress::Complete);
    let leave = AbsoluteBound::after(member.now(), 1000).unwrap();
    assert!(matches!(
        member
            .run
            .context
            .begin_releasing(leave, &mut member.run.clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert!(member.run.context.inspection.is_some());
    assert_ne!(member.run.context.phase(), RecordingPhase::Releasing);
}

#[test]
fn inspection_late_recorded_proof_keeps_enclosing_lifecycle_priority() {
    for key in [PumpDeadline::Enter, PumpDeadline::Leave] {
        let mut member = Member::new();
        let outer = AbsoluteBound::after(member.now(), 20).unwrap();
        member
            .run
            .context
            .arm(key, outer, &mut member.run.clock)
            .unwrap();
        let request = begin(&mut member, InspectionKind::Shape);
        member.respond(&request, s::StateControlAnswer::Shape(vec![]));
        // Exercise the actual recorder completing at the bound inside the
        // coordinator, before the context gets its post-record clock check.
        member.run.clock.advance_to(outer.deadline_ns()).unwrap();
        member.record_completion_only();
        assert_eq!(
            advance(&mut member).unwrap_err(),
            RunFailure::Wait { key, bound: outer }
        );
        assert!(member.run.context.flow.operation_completion().is_some());
        assert!(member.run.context.failure().is_none());
        assert_eq!(member.run.context.complete_wait(key), Some(outer));
        assert!(matches!(
            advance(&mut member),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ControlBound,
                ..
            })
        ));
        assert!(
            member
                .run
                .context
                .take_inspection(&mut member.run.clock)
                .is_err()
        );
    }
}
