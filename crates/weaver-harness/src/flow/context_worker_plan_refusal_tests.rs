use super::*;
use crate::flow::context::input_tests::InputFixture;
use crate::flow::context::output_tests::{OutputFixture, original, text};

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn refusal(fx: &mut InputFixture) -> PlanRefusal {
    let now = fx.run.clock.now().unwrap();
    PlanRefusal::new(
        &mut fx.run.context,
        &LifecycleRefusal::OutOfOrder,
        now,
        &mut fx.run.clock,
    )
    .unwrap()
}

#[test]
fn plan_refusal_retries_its_returned_original_after_an_earlier_wait() {
    let mut fx = InputFixture::new();
    let mut record = refusal(&mut fx);
    let run = &mut fx.run;
    let now = run.clock.now().unwrap();
    run.context
        .arm(PumpDeadline::PlanRefusal, record.bound, &mut run.clock)
        .unwrap();
    let earlier = AbsoluteBound::after(now, 1).unwrap();
    run.context
        .arm(PumpDeadline::Label, earlier, &mut run.clock)
        .unwrap();
    run.clock.advance_to(earlier.deadline_ns()).unwrap();
    let original = format!("{:?}", record.original.as_ref().unwrap());
    assert!(matches!(
        record.advance(&mut run.context, &mut ports(&mut run.clock)),
        Err(RunFailure::Wait { key: PumpDeadline::Label, bound }) if bound == earlier
    ));
    assert!(record.author.ticket().is_none());
    assert!(run.context.pending_author().is_none());
    run.context.complete_wait(PumpDeadline::Label).unwrap();
    assert_eq!(
        format!(
            "{:?}",
            record
                .original
                .as_ref()
                .expect("unaccepted original remains retryable")
        ),
        original
    );
    assert!(
        record
            .advance(&mut run.context, &mut ports(&mut run.clock))
            .unwrap()
    );
    assert_eq!(record.sequence, record.author.sequence());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::PlanRefusal),
        Some(record.bound)
    );
    assert_eq!(fx.records(serving::Kind::Refusal).len(), 1);
}

#[test]
fn plan_refusal_yields_to_the_original_output_author_and_keeps_its_bound() {
    let mut fx = OutputFixture::new();
    let content = vec![text("Read original.")];
    fx.begin(original(content.clone()));
    let mut record = refusal(&mut fx.input);
    let context = &mut fx.input.run.context;
    let earlier = format!("{:?}", context.waiting_content.as_ref().unwrap().event);
    assert!(
        !record
            .advance(context, &mut ports(&mut fx.input.run.clock))
            .unwrap()
    );
    assert_eq!(
        format!("{:?}", context.waiting_content.as_ref().unwrap().event),
        earlier
    );
    assert!(record.author.ticket().is_none());
    assert!(!record.started);
    assert_eq!(
        context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    assert_eq!(
        context.deadlines.get(PumpDeadline::PlanRefusal),
        Some(record.bound)
    );
    fx.complete(None, false);
    let run = &mut fx.input.run;
    assert!(
        record
            .advance(&mut run.context, &mut ports(&mut run.clock))
            .unwrap()
    );
    let checked = run.context.take_output(&mut run.clock).unwrap().unwrap();
    assert_eq!(checked.original().message.content, content);
    assert_ne!(
        checked.source().block_id.sequence.0,
        record.sequence.unwrap().0
    );
    assert_eq!(fx.input.records(serving::Kind::MessageAssistant).len(), 1);
    assert_eq!(fx.input.records(serving::Kind::Refusal).len(), 1);
}

#[test]
fn plan_refusal_rejects_a_foreign_interval_without_losing_the_original() {
    let mut fx = InputFixture::new();
    let mut record = refusal(&mut fx);
    let foreign = AbsoluteBound::after(record.bound.began_ns(), 20_000).unwrap();
    let run = &mut fx.run;
    run.context
        .arm(PumpDeadline::PlanRefusal, foreign, &mut run.clock)
        .unwrap();
    assert!(matches!(
        record.advance(&mut run.context, &mut ports(&mut run.clock)),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert!(record.original.is_some());
    assert!(record.author.ticket().is_none());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::PlanRefusal),
        Some(foreign)
    );
}

#[test]
fn plan_refusal_resumes_only_its_exact_ticket_after_trace_pressure() {
    let mut fx = InputFixture::new();
    let mut record = refusal(&mut fx);
    fx.fill_trace();
    let run = &mut fx.run;
    run.context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(104, 1).unwrap(),
            &mut run.clock,
        )
        .unwrap();
    assert!(matches!(
        record.advance(&mut run.context, &mut ports(&mut run.clock)),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    assert!(record.started);
    let ticket = record.author.ticket().unwrap();
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    assert!(!record.complete());
    let bound = record.bound;
    run.context.complete_wait(PumpDeadline::Label).unwrap();
    fx.drain_trace();
    let run = &mut fx.run;
    assert!(
        record
            .advance(&mut run.context, &mut ports(&mut run.clock))
            .unwrap()
    );
    assert_eq!(record.author.ticket(), Some(ticket));
    assert_eq!(record.sequence, record.author.sequence());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::PlanRefusal),
        Some(bound)
    );
    assert!(run.context.serving_observation_author.is_none());
    assert!(
        record
            .advance(&mut run.context, &mut ports(&mut run.clock))
            .unwrap()
    );
    let rows = fx.records(serving::Kind::Refusal);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]["payload"],
        serde_json::to_value(RefusalRecord::Lifecycle {
            asked: LifecycleAsk::PlanUpdate,
            refusal: LifecycleRefusal::OutOfOrder,
        })
        .unwrap()
    );
}

#[test]
fn plan_refusal_claim_rejects_a_different_refusal_and_context() {
    let mut fx = InputFixture::new();
    let record = refusal(&mut fx);
    let mut different = refusal(&mut fx).original.unwrap();
    different.causal_parent = Some(weaver_trace::Sequence(9));
    let run = &mut fx.run;
    let failure = run
        .context
        .author_serving_observation(&mut ports(&mut run.clock), &record.author, different)
        .unwrap_err();
    assert_eq!(failure.reason, RunFailure::Closed);
    assert_eq!(
        failure.original.unwrap().causal_parent,
        Some(weaver_trace::Sequence(9))
    );
    let mut other = InputFixture::new();
    let failure = other
        .run
        .context
        .author_serving_observation(
            &mut ports(&mut other.run.clock),
            &record.author,
            record.original.unwrap(),
        )
        .unwrap_err();
    assert_eq!(failure.reason, RunFailure::Closed);
    assert!(failure.original.is_some());
    assert!(record.author.ticket().is_none());
    assert!(other.run.context.pending_author().is_none());
    assert!(fx.records(serving::Kind::Refusal).is_empty());
    assert!(other.records(serving::Kind::Refusal).is_empty());
}

#[test]
fn plan_refusal_yields_to_the_exact_admitted_output_ticket() {
    let mut fx = OutputFixture::new();
    fx.input.fill_trace();
    fx.input
        .run
        .context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(104, 1).unwrap(),
            &mut fx.input.run.clock,
        )
        .unwrap();
    let content = vec![text("Read original.")];
    fx.begin(original(content.clone()));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let pending = fx.input.run.context.pending_author().unwrap();
    let ticket = pending.ticket;
    let exact = format!("{:?}", pending.event);
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    let mut record = refusal(&mut fx.input);
    let run = &mut fx.input.run;
    assert!(
        !record
            .advance(&mut run.context, &mut ports(&mut run.clock))
            .unwrap()
    );
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(
        format!("{:?}", run.context.pending_author().unwrap().event),
        exact
    );
    assert!(record.author.ticket().is_none());
    assert!(record.original.is_some());
    fx.input.drain_trace();
    fx.complete(None, false);
    let run = &mut fx.input.run;
    assert!(
        record
            .advance(&mut run.context, &mut ports(&mut run.clock))
            .unwrap()
    );
    let checked = run.context.take_output(&mut run.clock).unwrap().unwrap();
    assert_eq!(checked.original().message.content, content);
    assert!(checked.source().block_id.sequence.0 < record.sequence.unwrap().0);
    assert_eq!(fx.input.records(serving::Kind::MessageAssistant).len(), 1);
    assert_eq!(fx.input.records(serving::Kind::Refusal).len(), 1);
}
