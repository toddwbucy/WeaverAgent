//! Scoped observations interleave with real output/check owners and trace pressure.
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::context::output_tests::{OutputFixture, original, reply, text};
use crate::flow::pump::LifecyclePorts;

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn observation(
    context: &RecordingContext,
    save: bool,
) -> (Arc<ServingObservationAuthor>, AuthoredEvent) {
    if save {
        let payload = trace::SavePointRecord::Requested {
            capture_id: 51.into(),
            window: trace::SaveWindow {
                began_ns: 100.into(),
                deadline_ns: 1_000_100.into(),
            },
            tail_window: None,
            cause: trace::PersistenceCause::Requested,
        };
        return (
            Arc::new(ServingObservationAuthor::new(context, &payload)),
            AuthoredEvent {
                kind: Kind::SavePoint,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: None,
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(Payload::SavePointRecord(Box::new(payload))),
            },
        );
    }
    let report = crate::authorship::harness_report(
        weaver_types::FaultCase::OrganDeathObserved,
        "scoped observation while an output remains owned",
    );
    (
        Arc::new(ServingObservationAuthor::organ_loss(context, &report)),
        AuthoredEvent {
            kind: Kind::Fault,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: None,
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::Fault(
                serde_json::value::to_raw_value(&report).unwrap(),
            )),
        },
    )
}

fn interleave_output(stage: u8, save: bool) {
    let mut fx = OutputFixture::new();
    let content = vec![text("Read original.")];
    fx.begin(original(content.clone()));
    let pending = if stage == 1 { Some(fx.request()) } else { None };
    if stage == 2 {
        fx.complete(None, false);
    }
    let (claim, mut event) = observation(&fx.input.run.context, save);
    let kind = event.kind;
    if stage == 0 {
        let run = &mut fx.input.run;
        let earlier = format!("{:?}", run.context.waiting_content.as_ref().unwrap().event);
        let offered = format!("{event:?}");
        assert!(!run.context.serving_observation_author_available());
        let failure = run
            .context
            .author_serving_observation(&mut ports(&mut run.clock), &claim, event)
            .unwrap_err();
        assert_eq!(failure.reason, RunFailure::Busy(HeldInput::Caller));
        event = failure.original.unwrap();
        assert_eq!(format!("{event:?}"), offered);
        assert_eq!(
            format!("{:?}", run.context.waiting_content.as_ref().unwrap().event),
            earlier
        );
        assert!(claim.ticket().is_none());
        assert!(claim.sequence().is_none());
        assert!(run.context.failure().is_none());
        fx.complete(None, false);
    }
    let prior_output = fx.input.records(Kind::MessageAssistant);
    let run = &mut fx.input.run;
    assert!(run.context.serving_observation_author_available());
    let sequence = run
        .context
        .author_serving_observation(&mut ports(&mut run.clock), &claim, event)
        .unwrap();
    assert_eq!(claim.sequence(), Some(sequence));
    assert!(run.context.serving_observation_author.is_none());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    assert!(run.context.failure().is_none());
    assert_eq!(fx.input.records(Kind::MessageAssistant), prior_output);
    if let Some(request) = pending {
        fx.input.reply(&reply(&request, false));
    }
    fx.complete(None, false);
    let checked = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(checked.original().message.content, content);
    assert_ne!(checked.source().block_id.sequence.0, sequence.0);
    let model = fx.input.records(Kind::MessageAssistant);
    assert_eq!(model.len(), 1);
    assert_eq!(
        model[0]["sequence"],
        checked.source().block_id.sequence.0.to_string()
    );
    assert_eq!(fx.requests.len(), 1);
    assert_eq!(checked.blocks()[0].request().text, "Read original.");
    assert_eq!(fx.input.records(kind).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn observation_waits_behind_the_unsubmitted_output_original() {
    interleave_output(0, false);
}

#[test]
fn observation_during_output_check_keeps_its_original_source() {
    interleave_output(1, false);
}

#[test]
fn observation_before_output_retrieval_keeps_its_completed_account() {
    interleave_output(2, false);
}

#[test]
fn queued_save_during_output_check_keeps_its_original_source() {
    interleave_output(1, true);
}

#[test]
fn queued_save_before_output_retrieval_keeps_its_completed_account() {
    interleave_output(2, true);
}

#[test]
fn observation_waits_behind_the_exact_pending_output_author() {
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
    fx.begin(original(vec![text("Read original.")]));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let pending = fx.input.run.context.pending_author().unwrap();
    let ticket = pending.ticket;
    let exact_original = format!("{:?}", pending.event);
    let run = &mut fx.input.run;
    let (claim, event) = observation(&run.context, false);
    let exact_observation = format!("{event:?}");
    let failure = run
        .context
        .author_serving_observation(&mut ports(&mut run.clock), &claim, event)
        .unwrap_err();
    assert_eq!(failure.reason, RunFailure::Busy(HeldInput::Caller));
    assert_eq!(
        format!("{:?}", failure.original.as_ref().unwrap()),
        exact_observation
    );
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(
        format!("{:?}", run.context.pending_author().unwrap().event),
        exact_original
    );
    assert!(claim.ticket().is_none());
    assert!(claim.sequence().is_none());
    assert!(run.context.serving_observation_author.is_none());
    assert!(run.context.failure().is_none());
    run.context.complete_wait(PumpDeadline::Label).unwrap();
    fx.input.drain_trace();
    fx.complete(None, false);
    let run = &mut fx.input.run;
    let sequence = run
        .context
        .author_serving_observation(
            &mut ports(&mut run.clock),
            &claim,
            failure.original.unwrap(),
        )
        .unwrap();
    let checked = run.context.take_output(&mut run.clock).unwrap().unwrap();
    assert_ne!(checked.source().block_id.sequence.0, sequence.0);
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert_eq!(fx.input.records(Kind::Fault).len(), 1);
}
