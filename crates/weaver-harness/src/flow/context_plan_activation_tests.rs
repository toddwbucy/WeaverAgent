//! Released queries supply private activation proofs. Scripted replies here
//! test the harness guards, while worker cases use the actual SQLite member.
use super::*;
use crate::flow::plan::ports::CheckedStepSuccessor;

#[path = "context_plan_activation_effect_tests.rs"]
mod effects;

const _: fn() = || {
    trait NotClone<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <CheckedStepSuccessor as NotClone<_>>::pin;
};

fn successor_result(seed: &str) -> s::QueryResult {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    let mut result: s::QueryResult = serde_json::from_value(
        cases
            .iter()
            .find(|case| case["case"] == "step_successor_exact_instruction")
            .unwrap()["result"]
            .clone(),
    )
    .unwrap();
    let s::QueryResult::StepSuccessor {
        reference,
        instruction,
    } = &mut result
    else {
        unreachable!()
    };
    reference.plan.plan_id.seed_id = seed.into();
    reference.plan.revision = 1.into();
    reference.previous_step = None;
    reference.step_id.seed_id = seed.into();
    reference.step_id.local_id = 4.into();
    instruction.step_query = Some(reference.clone());
    result
}
fn query_request(fx: &SummaryFixture) -> PlanRequest {
    PlanRequest::StepSuccessor {
        expected: request(fx.seed()).expected().clone(),
        previous_step: None,
    }
}
fn release_result(fx: &mut SummaryFixture, result: &s::QueryResult) {
    let read = next(fx);
    let bytes = serde_json::to_vec(result).unwrap();
    fx.boot
        .member
        .respond(&read, page(61, 0, bytes.len(), &bytes));
    let release = next(fx);
    assert!(
        matches!(&release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 61)
    );
    assert!(take(fx).is_none(), "unreleased query grants no result");
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 61.into(),
        },
    );
}
fn query(fx: &mut SummaryFixture) -> CheckedStepSuccessor {
    query_with_previous(fx, None)
}
fn query_with_previous(
    fx: &mut SummaryFixture,
    previous: Option<s::StepId>,
) -> CheckedStepSuccessor {
    let request = PlanRequest::StepSuccessor {
        expected: query_request(fx).expected().clone(),
        previous_step: previous.clone(),
    };
    let mut result = successor_result(fx.seed());
    let s::QueryResult::StepSuccessor {
        reference,
        instruction,
    } = &mut result
    else {
        unreachable!()
    };
    reference.previous_step = previous;
    instruction.step_query = Some(reference.clone());
    begin(fx, request);
    release_result(fx, &result);
    match finish(fx).into_successor() {
        Ok(proof) => proof,
        Err(_) => panic!("query must succeed"),
    }
}

#[test]
fn plan_activation_successor_requires_release_and_keeps_standalone_read_outcome() {
    let mut fx = ready(false);
    let bound = fx.run().context.deadlines.get(PumpDeadline::Decision);
    let proof = query(&mut fx);
    assert_eq!(&proof.reference().plan, query_request(&fx).expected());
    assert!(fx.records(Kind::PlanEvent).is_empty());
    assert!(fx.records(Kind::Refusal).is_empty());
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        bound
    );
}

#[test]
fn plan_activation_successor_refusals_do_not_become_plan_operation_refusals() {
    for code in [
        s::ReadRefusalCode::NotFound,
        s::ReadRefusalCode::InvalidSelector,
        s::ReadRefusalCode::CounterExhausted,
        s::ReadRefusalCode::ResultUnavailable,
    ] {
        let mut fx = ready(false);
        let request = query_request(&fx);
        begin(&mut fx, request);
        let read = next(&mut fx);
        fx.boot.member.respond(&read, refuse(code));
        let result = finish(&mut fx);
        assert_eq!(
            result.query_refusal(),
            Some((code, "retained member detail"))
        );
        assert!(result.into_successor().is_err());
        assert!(fx.records(Kind::Refusal).is_empty());
        assert!(fx.records(Kind::PlanEvent).is_empty());
    }
}

#[test]
fn plan_activation_successor_next_refusal_releases_before_return() {
    let mut fx = ready(false);
    let request = query_request(&fx);
    begin(&mut fx, request);
    let read = next(&mut fx);
    fx.boot.member.respond(&read, page(61, 0, 20, b"{"));
    let continuation = next(&mut fx);
    fx.boot
        .member
        .respond(&continuation, refuse(s::ReadRefusalCode::WrongOffset));
    let release = next(&mut fx);
    assert!(
        matches!(&release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 61)
    );
    assert!(take(&mut fx).is_none());
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 61.into(),
        },
    );
    assert_eq!(
        finish(&mut fx).query_refusal(),
        Some((s::ReadRefusalCode::WrongOffset, "retained member detail"))
    );
    assert!(fx.records(Kind::Refusal).is_empty());
}

fn activation_fixture(group: u8) -> (SummaryFixture, s::QueryResult) {
    activation_fixture_with(group, |_| {})
}
fn activation_fixture_with(
    group: u8,
    edit: impl FnOnce(&mut s::PlanSnapshot),
) -> (SummaryFixture, s::QueryResult) {
    let (fixture, marks, _certificate) = activation_fixture_with_certificate(group, edit);
    (fixture, marks)
}
fn activation_fixture_with_certificate(
    group: u8,
    edit: impl FnOnce(&mut s::PlanSnapshot),
) -> (
    SummaryFixture,
    s::QueryResult,
    crate::flow::templates::TemplateCertificate,
) {
    activation_fixture_from(SummaryFixture::new(vec![]), group, edit)
}

fn activation_fixture_from(
    mut fx: SummaryFixture,
    group: u8,
    edit: impl FnOnce(&mut s::PlanSnapshot),
) -> (
    SummaryFixture,
    s::QueryResult,
    crate::flow::templates::TemplateCertificate,
) {
    let mut plan = fx.snapshot();
    let s::QueryResult::StepSuccessor { reference, .. } = successor_result(fx.seed()) else {
        unreachable!()
    };
    plan.status = s::PlanStatus::Locked;
    plan.lock_source = Some(plan.operator_source.clone());
    plan.categories = vec![s::Category {
        id: "work".into(),
        verbs: vec!["read".into()],
        acts: vec![s::PlanAct::Statement],
        review_required: false,
    }];
    plan.steps = vec![s::PlanStep {
        step_id: reference.step_id.clone(),
        ordinal: 1.into(),
        instruction: reference.instruction,
        category: "work".into(),
        group: (group != 0).then(|| "group".into()),
        status: s::StepStatus::Waiting,
        span: None,
        outcome_span: None,
        review_call: None,
        work: None,
        closure_source: None,
    }];
    let span = |id| s::SpanRef {
        seed_id: fx.seed().into(),
        local_id: id,
        revision: 1.into(),
    };
    plan.group_drafts = if group == 0 {
        vec![]
    } else {
        vec![s::GroupDraft {
            id: "group".into(),
            step_indices: vec![0.into()],
        }]
    };
    plan.groups = if group == 0 {
        vec![]
    } else {
        vec![s::PlanGroup {
            id: "group".into(),
            step_ids: vec![reference.step_id],
            span: (group == 2).then(|| span(7.into())),
        }]
    };
    let opened = Some(plan.operator_source.clone());
    let mut scopes = vec![s::SpanInfo {
        span: plan.task.task.clone(),
        parent: None,
        kind: s::SpanKind::Task,
        status: s::SpanStatus::Open,
        opened_source: opened.clone(),
        ended_source: None,
        replacement: None,
    }];
    if group == 2 {
        scopes.push(s::SpanInfo {
            span: span(7.into()),
            parent: Some(plan.task.task.clone()),
            kind: s::SpanKind::Subtask,
            status: s::SpanStatus::Open,
            opened_source: opened,
            ended_source: None,
            replacement: None,
        });
    }
    let marks = s::QueryResult::Marks {
        next_span_id: 10.into(),
        current_task: Some(plan.task.clone()),
        open_scopes: scopes,
    };
    edit(&mut plan);
    let boot = fx.boot(Some(plan));
    let run = fx.run();
    run.context.install_place(boot, &mut run.clock).unwrap();
    let certificate = run
        .context
        .take_template_certificate(&mut run.clock)
        .unwrap()
        .unwrap();
    run.context.complete_wait(PumpDeadline::Enter).unwrap();
    run.context
        .arm(
            PumpDeadline::Decision,
            AbsoluteBound::after(run.clock.now().unwrap(), 5_000_000_000).unwrap(),
            &mut run.clock,
        )
        .unwrap();
    (fx, marks, certificate)
}
fn begin_activation(fx: &mut SummaryFixture) {
    let successor = query(fx);
    begin_with_successor(fx, successor);
}
fn begin_with_successor(fx: &mut SummaryFixture, successor: CheckedStepSuccessor) {
    let request = PlanRequest::Activate {
        expected: successor.reference().plan.clone(),
        step_id: successor.reference().step_id.clone(),
        successor,
    };
    let run = fx.run();
    let turn = TurnRef("activation".into());
    run.context
        .author_and_wait(
            &mut ports(&mut run.clock),
            AuthoredEvent {
                kind: Kind::TurnStarted,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(turn.clone()),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: None,
            },
        )
        .unwrap();
    run.context
        .begin_plan_operation(request, Some(turn), &mut run.clock)
        .unwrap();
}

#[test]
fn plan_activation_allocates_ungrouped_new_group_and_existing_group_from_marks() {
    for group in 0..3 {
        let (mut fx, marks) = activation_fixture(group);
        begin_activation(&mut fx);
        release_result(&mut fx, &marks);
        let mut prepared = false;
        for _ in 0..128 {
            if advance(&mut fx).unwrap() == PlanPortProgress::EffectReady {
                prepared = true;
                break;
            }
        }
        assert!(prepared);
        let s::PlanEffect::Activate {
            step_span,
            group_span,
            ..
        } = fx.run().context.test_plan_effect().unwrap()
        else {
            panic!()
        };
        assert_eq!(step_span.local_id.0, if group == 1 { 11 } else { 10 });
        assert_eq!(
            group_span.as_ref().map(|span| span.local_id.0),
            match group {
                0 => None,
                1 => Some(10),
                _ => Some(7),
            }
        );
        assert!(
            take(&mut fx).is_none(),
            "prepared metadata is not a committed active step"
        );
        assert!(fx.records(Kind::PlanEvent).is_empty());
    }
}

#[test]
fn plan_activation_marks_off_the_plan_task_or_group_path_fault_before_any_effect() {
    // "prompt": the served catalog is coherent and its open path is the
    // plan's own task, yet the plan's task names another prompt, so the
    // catalog's current task is not the plan's. "group": the open Subtask
    // span is not the plan's group span.
    for case in ["prompt", "group"] {
        let (mut fx, mut marks) = activation_fixture_with(2, |plan| {
            if case == "prompt" {
                plan.task.prompt.local_id = 7.into();
            }
        });
        if case == "group" {
            let s::QueryResult::Marks { open_scopes, .. } = &mut marks else {
                unreachable!()
            };
            open_scopes[1].span.local_id = 8.into();
        }
        begin_activation(&mut fx);
        release_result(&mut fx, &marks);
        let failure = (0..128)
            .find_map(|_| advance(&mut fx).err())
            .expect("activation off its marks path is no ordinary refusal");
        let detail = if case == "prompt" {
            "plan task disagrees with its open marks path"
        } else {
            "activation marks differ from its task/group scopes"
        };
        assert!(
            matches!(
                &failure,
                RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                }
            ) && format!("{failure:?}").contains(detail),
            "{case}: {failure:?}"
        );
        assert!(fx.run().context.test_plan_effect().is_none(), "{case}");
        assert!(fx.records(Kind::PlanEvent).is_empty(), "{case}");
        assert!(fx.records(Kind::Refusal).is_empty(), "{case}");
    }
}

#[test]
fn plan_activation_group_counter_exhaustion_records_refusal_without_effect() {
    let (mut fx, mut marks) = activation_fixture(1);
    let s::QueryResult::Marks { next_span_id, .. } = &mut marks else {
        unreachable!()
    };
    *next_span_id = u64::MAX.into();
    begin_activation(&mut fx);
    release_result(&mut fx, &marks);
    let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
        panic!()
    };
    assert_eq!(refusal.case, s::PlanOperationCase::CounterExhausted);
    assert_eq!(refusal.read_code, None);
    assert!(fx.records(Kind::PlanEvent).is_empty());
    let rows = fx.records(Kind::Refusal);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["payload"]["operation"], "activate");
    assert_eq!(
        rows[0]["causal_parent"],
        fx.records(Kind::Recall).last().unwrap()["sequence"]
    );
}

fn closed_step(plan: &s::PlanSnapshot, id: u64, ordinal: u64) -> s::PlanStep {
    let mut step = plan.steps[0].clone();
    step.step_id.local_id = id.into();
    step.ordinal = ordinal.into();
    step.status = s::StepStatus::Closed;
    step.span = Some(s::SpanRef {
        seed_id: step.step_id.seed_id.clone(),
        local_id: 5.into(),
        revision: 1.into(),
    });
    step.outcome_span = Some(s::SpanRef {
        seed_id: step.step_id.seed_id.clone(),
        local_id: 6.into(),
        revision: 1.into(),
    });
    step.work = Some(s::WorkRef {
        plan: plan.plan.clone(),
        step_id: step.step_id.clone(),
        span: step.span.clone().unwrap(),
        through: plan.last_source.clone(),
        content_sha256: s::Digest::new("a".repeat(64)).unwrap(),
    });
    step.closure_source = Some(plan.operator_source.clone());
    step
}

#[test]
fn plan_activation_preserved_later_closures_do_not_require_a_closed_prefix() {
    for has_predecessor in [false, true] {
        let (mut fx, marks) = activation_fixture_with(0, |plan| {
            let mut later = closed_step(plan, 6, if has_predecessor { 3 } else { 2 });
            later.span.as_mut().unwrap().local_id = 7.into();
            later.outcome_span.as_mut().unwrap().local_id = 8.into();
            later.work.as_mut().unwrap().span = later.span.clone().unwrap();
            if has_predecessor {
                let previous = closed_step(plan, 5, 1);
                plan.steps[0].ordinal = 2.into();
                plan.steps.insert(0, previous);
            }
            plan.steps.push(later);
        });
        let previous = has_predecessor.then(|| s::StepId {
            seed_id: fx.seed().into(),
            local_id: 5.into(),
        });
        let successor = query_with_previous(&mut fx, previous);
        begin_with_successor(&mut fx, successor);
        release_result(&mut fx, &marks);
        let mut prepared = false;
        for _ in 0..128 {
            if advance(&mut fx).unwrap() == PlanPortProgress::EffectReady {
                prepared = true;
                break;
            }
        }
        assert!(
            prepared,
            "a preserved later closure must not hide the first waiting step"
        );
    }
}

#[test]
fn plan_activation_requires_the_exact_closed_immediate_predecessor() {
    let (mut fx, _) = activation_fixture_with(0, |plan| {
        let previous = closed_step(plan, 5, 1);
        plan.steps[0].ordinal = 2.into();
        plan.steps.insert(0, previous);
    });
    // This is a real released result for its selector. It cannot authorize a
    // different current predecessor, even if every public target ID matches.
    begin_activation(&mut fx);
    let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
        panic!()
    };
    assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
    assert!(fx.records(Kind::PlanEvent).is_empty());
    assert!(fx.run().context.flow.state().outstanding().is_empty());
    assert_eq!(
        fx.records(Kind::Refusal)[0]["causal_parent"],
        fx.records(Kind::Recall).last().unwrap()["sequence"],
        "predecessor refusal consumes the newer actual successor proof"
    );
}

#[test]
fn plan_activation_foreign_successor_proof_latches_without_read_or_effect() {
    let (mut original, _) = activation_fixture(0);
    let (mut other, _) = activation_fixture(0);
    assert_eq!(
        original.seed(),
        other.seed(),
        "public identities deliberately match"
    );
    let proof = query(&mut other);
    begin_with_successor(&mut original, proof);
    let failure = advance(&mut original).unwrap_err();
    assert!(format!("{failure:?}").contains("foreign successor proof"));
    assert_eq!(original.run().context.failure(), Some(&failure));
    original.boot.member.no_request();
    assert!(original.records(Kind::PlanEvent).is_empty());
    assert!(original.records(Kind::Refusal).is_empty());
    assert!(original.run().context.plan_port_work().is_some());
}

#[test]
fn plan_activation_refuses_when_another_step_is_active() {
    let (mut fx, _) = activation_fixture_with(0, |plan| {
        let mut active = plan.steps[0].clone();
        active.step_id.local_id = 6.into();
        active.ordinal = 2.into();
        active.status = s::StepStatus::Active;
        active.span = Some(s::SpanRef {
            seed_id: active.step_id.seed_id.clone(),
            local_id: 7.into(),
            revision: 1.into(),
        });
        plan.steps.push(active);
    });
    begin_activation(&mut fx);
    let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
        panic!()
    };
    assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
    assert_eq!(refusal.read_code, None);
    assert!(fx.records(Kind::PlanEvent).is_empty());
    fx.boot.member.no_request();
}

#[test]
fn plan_activation_cancelled_query_releases_without_a_successor() {
    let mut fx = ready(false);
    let request = query_request(&fx);
    begin(&mut fx, request);
    let read = next(&mut fx);
    let bytes = serde_json::to_vec(&successor_result(fx.seed())).unwrap();
    let split = bytes.len() / 2;
    fx.boot
        .member
        .respond(&read, page(61, 0, bytes.len(), &bytes[..split]));
    let continuation = next(&mut fx);
    assert!(matches!(
        &continuation,
        s::StateFrame::Control {
            frame: s::StateControl::Next { .. },
            ..
        }
    ));
    fx.run().context.cancel_plan_operation();
    fx.boot
        .member
        .respond(&continuation, page(61, split, bytes.len(), &bytes[split..]));
    let release = next(&mut fx);
    assert!(
        matches!(&release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 61)
    );
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 61.into(),
        },
    );
    let result = finish(&mut fx);
    assert!(matches!(result, PlanPortCompletion::Cancelled(_)));
    assert!(result.into_successor().is_err());
    assert!(fx.records(Kind::PlanEvent).is_empty());
}

#[test]
fn plan_activation_cancelled_preparation_releases_allocator_without_effect() {
    let (mut fx, marks) = activation_fixture(0);
    begin_activation(&mut fx);
    release_result(&mut fx, &marks);
    for _ in 0..128 {
        if advance(&mut fx).unwrap() == PlanPortProgress::EffectReady {
            break;
        }
    }
    assert!(fx.run().context.test_plan_effect().is_some());
    fx.run().context.cancel_plan_operation();
    assert!(matches!(finish(&mut fx), PlanPortCompletion::Cancelled(_)));
    assert!(fx.run().context.plan_port_work().is_none());
    assert!(fx.records(Kind::PlanEvent).is_empty());
    assert!(fx.records(Kind::Refusal).is_empty());
}

#[test]
fn plan_activation_template_fault_records_before_refusal_even_after_cancellation() {
    for cancelled in [false, true] {
        let (mut fx, marks, certificate) = activation_fixture_with_certificate(0, |_| {});
        begin_activation(&mut fx);
        release_result(&mut fx, &marks);
        let mut prepared = false;
        for _ in 0..128 {
            if advance(&mut fx).unwrap() == PlanPortProgress::EffectReady {
                prepared = true;
                break;
            }
        }
        assert!(prepared);
        let run = fx.run();
        // This real loaded certificate lacks plan.activate. Rejection must be
        // accounted without substituting another certified family.

        run.context
            .begin_plan_effect(&certificate, &mut run.clock)
            .unwrap();
        if cancelled {
            run.context.cancel_plan_operation();
        }
        let result = finish(&mut fx);
        if cancelled {
            assert!(matches!(result, PlanPortCompletion::Cancelled(_)));
        } else {
            let PlanPortCompletion::Refused { refusal, .. } = result else {
                panic!()
            };
            assert_eq!(refusal.case, s::PlanOperationCase::TemplateRejected);
            assert_eq!(refusal.read_code, None);
        }
        assert!(fx.records(Kind::PlanEvent).is_empty());
        let faults = fx.records(Kind::Fault);
        let refusals = fx.records(Kind::Refusal);
        assert_eq!(faults.len(), 1);
        assert_eq!(refusals.len(), 1);
        assert_eq!(faults[0]["payload"]["case"], "state_template_rejected");
        assert_eq!(refusals[0]["causal_parent"], faults[0]["sequence"]);
        assert_eq!(
            refusals[0]["payload"]["refusal"]["case"],
            "template_rejected"
        );
    }
}
