//! Fresh material consumes a real completed begin_task, not an inspection copy.
use super::*;
use crate::flow::material::{HeldMaterial, MaterialFailure, SelectedUnit};
use std::sync::Arc;
use weaver_traits::{ContentBlock, Message, Role};

const _: fn() = || {
    trait MustNotFromUnfiled<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotFromUnfiled<()> for T {}
    impl<T: From<SeatedInput>> MustNotFromUnfiled<u8> for T {}
    impl<T: From<AcceptedInput>> MustNotFromUnfiled<u16> for T {}
    impl<T: From<marks_driver::TaskAccount>> MustNotFromUnfiled<u32> for T {}
    impl<T: From<Message>> MustNotFromUnfiled<u64> for T {}
    let _ = <HeldMaterial as MustNotFromUnfiled<_>>::check;
    let _ = <SelectedUnit as MustNotFromUnfiled<_>>::check;
};

fn begin_text(fx: &mut TaskFixture, text: &str) {
    let frame = serde_json::to_vec(&serde_json::json!({"text": text})).unwrap();
    fx.input.begin(TurnFrame::carry_from(&frame, 42));
    let token = (0..128)
        .find_map(|_| {
            fx.input
                .run
                .context
                .advance_input(&mut ports(&mut fx.input.run.clock))
                .unwrap();
            fx.checker();
            fx.input
                .run
                .context
                .take_accepted_input(&mut fx.input.run.clock)
                .unwrap()
        })
        .expect("real accepted input");
    fx.input
        .run
        .context
        .begin_task(
            token,
            TurnRef("task-turn".into()),
            &fx.certificate,
            &mut fx.input.run.clock,
        )
        .unwrap();
}

fn begun(fx: &mut TaskFixture) -> BegunTask {
    let TaskCompletion::Begun(task) = fx.finish() else {
        panic!("fixture did not return the real begun task");
    };
    task
}

fn hold(fx: &mut TaskFixture, task: BegunTask) -> HeldMaterial {
    fx.input
        .run
        .context
        .hold_task_material(task, &mut fx.input.run.clock)
        .unwrap()
}

fn existing(fx: &mut TaskFixture) -> s::TaskScopeRef {
    let scope = s::TaskScopeRef {
        task: fx.span(1),
        prompt: fx.span(2),
    };
    fx.result = s::QueryResult::Marks {
        next_span_id: 5.into(),
        current_task: Some(scope.clone()),
        open_scopes: vec![s::SpanInfo {
            span: scope.task.clone(),
            parent: None,
            kind: s::SpanKind::Task,
            status: s::SpanStatus::Open,
            opened_source: Some(s::SourceBlockId {
                run: "prior-run".into(),
                sequence: 4.into(),
                block: 0.into(),
            }),
            ended_source: None,
            replacement: None,
        }],
    };
    scope
}

#[test]
fn task_material_preserves_exact_filed_user_text_and_complete_task_evidence() {
    let mut fx = TaskFixture::new();
    let text = "Read caf\u{e9}  file.";
    begin_text(&mut fx, text);
    let task = begun(&mut fx);
    let source = task.input().source().clone();
    let scope = task.scope().clone();
    let mark = task.mark().unwrap().sequence();
    let origin = task.origin().clone();
    let evidence = marks_driver::validation_tests::task_material_evidence(&task);
    let request_count = fx.requests.len();
    let record_count = fx.input.records(Kind::Recall).len();
    let held = hold(&mut fx, task);
    assert!(held.belongs_to(&fx.input.run.context));
    assert_eq!(held.units().len(), 1);
    let unit = &held.units()[0];
    assert!(Arc::ptr_eq(unit.origin(), &origin));
    assert_eq!(
        unit.messages(),
        &[Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        }]
    );
    let content = s::ContentRef::Event { block_id: source };
    assert_eq!(
        unit.unit_id(),
        &s::UnitId::Block {
            source: content.clone()
        }
    );
    let material = s::MaterialRef::Text {
        source: content,
        span: s::Span {
            start: 0.into(),
            end: (text.len() as u64).into(),
        },
    };
    assert_eq!(unit.sources(), std::slice::from_ref(&material));
    assert_eq!(
        unit.source_map(),
        &[crate::flow::material::SourceMapEntry {
            material,
            message: 0.into(),
            block: 0.into(),
            start: Some(0.into()),
            end: Some((text.len() as u64).into()),
        }]
    );
    assert!(unit.spans().is_empty());
    assert!(unit.step_queries().is_empty());
    assert_eq!(held.task().unwrap().scope(), &scope);
    assert_eq!(unit.task().unwrap().mark().unwrap().sequence(), mark);
    assert_eq!(
        marks_driver::validation_tests::task_material_evidence(unit.task().unwrap()),
        evidence
    );
    marks_driver::validation_tests::input_associations(unit.task().unwrap());
    marks_driver::validation_tests::mark_associations(unit.task().unwrap());
    assert_eq!(fx.requests.len(), request_count);
    assert_eq!(fx.input.records(Kind::Recall).len(), record_count);
    assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);
}

#[test]
fn task_material_is_mandatory_only_without_catalog_read_rank_or_fake_recall() {
    let mut fx = TaskFixture::new();
    fx.begin();
    let task = begun(&mut fx);
    assert_eq!(fx.input.records(Kind::Recall).len(), 1);
    let held = hold(&mut fx, task);
    // The genuine marks Recall still licenses the task. It does not rank its
    // new user Text as if that Text came from the catalog's content result.
    assert!(held.recall_sequences().next().is_none());
    assert!(held.units()[0].reads().is_empty());
    assert!(held.units()[0].task().is_some());
    assert_eq!(fx.input.records(Kind::Recall).len(), 1);
}

#[test]
fn task_material_existing_scope_keeps_new_input_without_inventing_a_mark() {
    let mut fx = TaskFixture::new();
    let scope = existing(&mut fx);
    fx.begin();
    let task = begun(&mut fx);
    assert!(task.mark().is_none());
    let source = task.input().source().clone();
    let evidence = marks_driver::validation_tests::task_material_evidence(&task);
    let held = hold(&mut fx, task);
    let task = held.task().unwrap();
    assert_eq!(task.scope(), &scope);
    assert_eq!(task.input().source(), &source);
    assert!(task.mark().is_none());
    assert!(held.units()[0].spans().is_empty());
    assert_eq!(
        marks_driver::validation_tests::task_material_evidence(task),
        evidence
    );
    assert!(fx.input.records(Kind::StateMark).is_empty());
}

#[test]
fn task_material_retains_receipts_and_catalog_after_container_and_context_drop() {
    let mut fx = TaskFixture::new();
    fx.begin();
    let task = begun(&mut fx);
    let evidence = marks_driver::validation_tests::task_material_evidence(&task);
    let origin = task.origin().clone();
    let unit = Arc::new(hold(&mut fx, task).into_units().pop().unwrap());
    let repeated = unit.clone();
    drop(unit);
    drop(fx);
    assert!(Arc::ptr_eq(repeated.origin(), &origin));
    assert_eq!(
        marks_driver::validation_tests::task_material_evidence(repeated.task().unwrap()),
        evidence
    );
    assert!(repeated.reads().is_empty());
    marks_driver::validation_tests::input_associations(repeated.task().unwrap());
    marks_driver::validation_tests::mark_associations(repeated.task().unwrap());
}

#[test]
fn task_material_foreign_context_returns_exact_owner_even_with_equal_source_ids() {
    let mut first = TaskFixture::new();
    let mut second = TaskFixture::new();
    first.begin();
    second.begin();
    let first_task = begun(&mut first);
    let second_task = begun(&mut second);
    assert_eq!(first_task.input().source(), second_task.input().source());
    let evidence = marks_driver::validation_tests::task_material_evidence(&first_task);
    let error = second
        .input
        .run
        .context
        .hold_task_material(first_task, &mut second.input.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::ForeignContext);
    assert!(matches!(
        error.failure,
        Some(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_eq!(
        marks_driver::validation_tests::task_material_evidence(&error.task),
        evidence
    );
    let held = hold(&mut first, *error.task);
    assert!(held.belongs_to(&first.input.run.context));
    assert!(!held.belongs_to(&second.input.run.context));
    assert!(held.units()[0].belongs_to(&first.input.run.context));
    assert!(!held.units()[0].belongs_to(&second.input.run.context));
    assert!(!Arc::ptr_eq(held.units()[0].origin(), second_task.origin()));
}

#[test]
fn task_material_expired_leave_returns_owner_for_retry_without_refiling() {
    let mut fx = TaskFixture::new();
    fx.begin();
    let task = begun(&mut fx);
    let evidence = marks_driver::validation_tests::task_material_evidence(&task);
    let request_count = fx.requests.len();
    let now = fx.input.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(now, 10).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.input.run.clock)
        .unwrap();
    fx.input.run.clock.advance_to(bound.deadline_ns()).unwrap();
    let error = fx
        .input
        .run
        .context
        .hold_task_material(task, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(error.kind, MaterialFailure::Unavailable);
    assert_eq!(
        error.failure,
        Some(RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound
        })
    );
    assert_eq!(
        marks_driver::validation_tests::task_material_evidence(&error.task),
        evidence
    );
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Leave),
        Some(bound)
    );
    let held = hold(&mut fx, *error.task);
    assert_eq!(
        marks_driver::validation_tests::task_material_evidence(held.task().unwrap()),
        evidence
    );
    assert_eq!(fx.requests.len(), request_count);
    assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);
    assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    assert_eq!(fx.input.records(Kind::Recall).len(), 1);
}

#[test]
fn task_material_transfer_has_no_post_consumption_recoverable_yield() {
    let mut fx = TaskFixture::new();
    fx.begin();
    let task = begun(&mut fx);
    let evidence = marks_driver::validation_tests::task_material_evidence(&task);
    let now = fx.input.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(now, 10).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.input.run.clock)
        .unwrap();
    let mut clock = HandoffClock {
        clock: &mut fx.input.run.clock,
        before_reads: 1,
        deadline: bound.deadline_ns(),
    };
    let held = fx
        .input
        .run
        .context
        .hold_task_material(task, &mut clock)
        .unwrap();
    assert_eq!(
        marks_driver::validation_tests::task_material_evidence(held.task().unwrap()),
        evidence
    );
    assert_eq!(
        fx.input.run.context.live(&mut clock),
        Err(RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound
        })
    );
    assert!(held.belongs_to(&fx.input.run.context));
}

#[test]
fn task_material_waits_for_real_recording_release_and_both_source_receipts() {
    let mut fx = TaskFixture::new();
    fx.hold_input = true;
    fx.begin();
    fx.input.fill_trace();
    // Author admission waits on actual writer capacity. Yield under an earlier
    // lifecycle bound so the fixture can inspect custody and drain the writer.
    let now = fx.input.run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 5).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
        .unwrap();
    assert_eq!(
        fx.step(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave,
        })
    );
    assert!(fx.input.run.context.pending_author().is_some());
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    assert!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.input.drain_trace();
    fx.steps(32);
    assert!(fx.requests.iter().any(|request| matches!(request,
        s::StateFrame::File { filing, .. } if filing.source.kind == "message.user")));
    assert!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.hold_input = false;
    fx.hold_release = true;
    fx.steps(32);
    assert!(fx.input.records(Kind::Recall).is_empty());
    assert!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.hold_release = false;
    fx.hold_mark = true;
    fx.steps(32);
    assert_eq!(fx.input.records(Kind::Recall).len(), 1);
    assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.hold_mark = false;
    let task = begun(&mut fx);
    let request_count = fx.requests.len();
    let held = hold(&mut fx, task);
    assert_eq!(held.units().len(), 1);
    assert_eq!(fx.requests.len(), request_count);
    assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);
    assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn task_material_has_no_success_owner_after_cancellation_or_catalog_refusal() {
    for refused in [false, true] {
        let mut fx = TaskFixture::new();
        fx.refuse_read = refused;
        fx.begin();
        if !refused {
            fx.input.run.context.cancel_task();
        }
        let completion = fx.finish();
        assert!(matches!(
            (completion, refused),
            (TaskCompletion::Refused { .. }, true) | (TaskCompletion::Cancelled(_), false)
        ));
        assert!(
            fx.input
                .run
                .context
                .take_task(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(fx.input.records(Kind::StateMark).is_empty());
    }
}
