//! Interrupted terminal checks retain the recorded input and its original time.

use super::*;
use crate::flow::classifier::ClassifierState;
use crate::flow::pump::{PumpSource, PumpStep};
use nix::sys::socket::{Shutdown, shutdown};
use std::os::fd::AsRawFd;

fn pending_release(cancelled: bool, rejected: bool) -> (InputFixture, s::Ste100Reply) {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read item."}"#, 42));
    let request = fx.request();
    while fx.run.context.flow.input_measurements_pending() {
        fx.advance().unwrap();
    }
    if cancelled {
        fx.run.context.cancel_input(&mut fx.run.clock).unwrap();
        fx.run
            .context
            .input_response_sent(&mut fx.run.clock)
            .unwrap();
    }
    let mut reply = accepted(&request);
    if rejected {
        reply.result = s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "synthetic rejection".into(),
            spans: vec![],
        };
    }
    fx.reply(&reply);
    fx.run
        .context
        .flow
        .interrupt_classifier_terminal_probes_for_test(1);
    fx.advance().unwrap();
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    assert_eq!(
        fx.records(Kind::Ste100Result)[0]["payload"],
        serde_json::to_value(&reply).unwrap()
    );
    assert!(fx.run.context.flow.pending_classifier_recorded().is_some());
    assert!(fx.run.context.flow.operation_completion().is_none());
    assert_eq!(
        fx.run.context.flow.classifier().state(),
        ClassifierState::ResultHeld
    );
    assert_eq!(fx.run.context.flow.classifier().held(), Some(&reply));
    assert!(!fx.run.context.input.snapshot().unwrap().result_recorded);
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    fx.no_turn();
    (fx, reply)
}

#[test]
fn interrupted_input_release_preserves_recorded_time_and_single_acceptance() {
    let (mut fx, reply) = pending_release(false, false);
    let bound = fx.run.context.input.snapshot().unwrap().bound;
    let recorded_ns = fx.run.context.flow.pending_classifier_recorded().unwrap().2;
    assert!(recorded_ns < bound.deadline_ns());
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    fx.run.clock.advance_to(bound.deadline_ns() + 1).unwrap();
    fx.until(InputProgress::Accepted);
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_some()
    );
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    assert_eq!(
        fx.records(Kind::Ste100Result)[0]["payload"],
        serde_json::to_value(reply).unwrap()
    );
    let waits: Vec<_> = fx
        .records(Kind::StateFlow)
        .into_iter()
        .filter(|record| {
            record["payload"]["phase"] == "wait_finished"
                && record["payload"]["blocked_stage"] == "gate"
        })
        .collect();
    assert_eq!(waits.len(), 1);
    assert_eq!(
        waits[0]["payload"]["elapsed_ns"],
        (recorded_ns - bound.began_ns()).to_string()
    );
    assert!(fx.run.context.halt().is_none());
    fx.no_request();
    fx.no_turn();
}

#[test]
fn interrupted_cancelled_input_release_drains_exact_result_without_reviving_acceptance() {
    for rejected in [false, true] {
        let (mut fx, _) = pending_release(true, rejected);
        let bound = fx.run.context.input.snapshot().unwrap().bound;
        assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
        fx.run.clock.advance_to(bound.deadline_ns() + 1).unwrap();
        fx.until(InputProgress::Finished);
        assert!(fx.run.context.input.snapshot().unwrap().result_recorded);
        assert!(
            fx.run
                .context
                .take_accepted_input(&mut fx.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
        assert!(fx.records(Kind::Refusal).is_empty());
        fx.run.context.finish_input(&mut fx.run.clock).unwrap();
        fx.no_request();
        fx.no_turn();
    }
}

#[test]
fn interrupted_rejection_keeps_gate_bound_until_owed_refusal_records() {
    let (mut fx, _) = pending_release(false, true);
    let bound = fx.run.context.input.snapshot().unwrap().bound;
    assert_eq!(
        fx.run.context.deadlines.get(PumpDeadline::Gate),
        Some(bound)
    );
    fx.run
        .context
        .flow
        .interrupt_classifier_terminal_probes_for_test(2);
    assert_eq!(fx.advance().unwrap(), InputProgress::Pending);
    assert_eq!(fx.advance().unwrap(), InputProgress::Pending);
    assert!(fx.run.context.flow.operation_completion().is_none());
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    fx.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::GateBound,
            ..
        })
    ));
    assert!(fx.records(Kind::Refusal).is_empty());
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .is_err()
    );
    fx.no_turn();
}

#[test]
fn cancelling_an_interrupted_recorded_rejection_ends_gate_without_a_grant() {
    let (mut fx, _) = pending_release(false, true);
    let bound = fx.run.context.input.snapshot().unwrap().bound;
    fx.run.context.cancel_input(&mut fx.run.clock).unwrap();
    fx.run
        .context
        .input_response_sent(&mut fx.run.clock)
        .unwrap();
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    fx.run.clock.advance_to(bound.deadline_ns() + 1).unwrap();
    fx.until(InputProgress::Finished);
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    assert!(fx.records(Kind::Refusal).is_empty());
    fx.no_turn();
}

#[test]
fn recorded_classifier_release_stays_runnable_after_transport_clears_interruption() {
    let (mut fx, _) = pending_release(false, false);
    fx.classifier_transport();
    let waits = fx.run.clock.waits().len();
    let step = fx
        .run
        .context
        .pump_once(&mut ports(&mut fx.run.clock))
        .unwrap();
    assert!(
        matches!(step, PumpStep::Progress { sources } if sources.contains(&PumpSource::Classifier))
    );
    assert_eq!(
        fx.run.clock.waits().len(),
        waits,
        "a retained proof needs no fresh peer data"
    );
    assert!(fx.run.context.flow.operation_completion().is_none());
    fx.until(InputProgress::Accepted);
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
}

#[test]
fn recorded_classifier_release_waits_when_another_author_owns_the_barrier() {
    let (mut fx, _) = pending_release(false, false);
    fx.classifier_transport();
    fx.fill_trace();
    let context = &mut fx.run.context;
    let event = AuthoredEvent {
        kind: Kind::ModelMeasurement,
        subsystem: Subsystem::Harness,
        turn: Some(TurnRef("pressure-fixture".into())),
        source: super::super::super::author::AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(
                &serde_json::json!({"fixture": "pending external author"}),
            )
            .unwrap(),
        )),
    };
    assert!(matches!(
        context.barrier.submit_authored_with_clock(
            event,
            RecordPurpose::Observation,
            &context.author,
            1,
            &mut context.inbox,
            &mut context.budget,
            &mut || Ok(104),
        ),
        RecordProgress::Pending
    ));
    assert_eq!(fx.advance().unwrap(), InputProgress::Pending);
    let now = fx.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(now, 5).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.run.clock)
        .unwrap();
    let waits = fx.run.clock.waits().len();
    fx.run
        .context
        .flow
        .interrupt_classifier_terminal_probes_for_test(1);
    fx.classifier_transport();
    let step = fx
        .run
        .context
        .pump_once(&mut ports(&mut fx.run.clock))
        .unwrap();
    assert!(
        matches!(step, PumpStep::Progress { sources } if sources.contains(&PumpSource::Classifier))
    );
    assert_eq!(
        fx.run.clock.waits().len(),
        waits,
        "an interrupted status probe remains eligible behind another author"
    );
    assert!(matches!(
        fx.run.context.pump_once(&mut ports(&mut fx.run.clock)),
        Err(RunFailure::Wait { key: PumpDeadline::Leave, bound: expired }) if expired == bound
    ));
    assert_eq!(
        fx.run.clock.waits().len(),
        waits + 1,
        "a proof blocked by another author cannot make repeated local progress"
    );
    assert_eq!(
        fx.run.clock.waits().last().unwrap().absolute_deadline,
        Some(bound.deadline_ns())
    );
    assert!(fx.run.context.barrier.pending().is_some());
    assert!(fx.run.context.flow.operation_completion().is_none());
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    assert_eq!(
        fx.run.context.flow.classifier().state(),
        ClassifierState::ResultHeld
    );
}

#[test]
fn interrupted_classifier_release_keeps_outer_deadline_checks_between_attempts() {
    let (mut fx, _) = pending_release(false, false);
    let now = fx.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(now, 5).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.run.clock)
        .unwrap();
    fx.run
        .context
        .flow
        .interrupt_classifier_terminal_probes_for_test(4);
    assert_eq!(fx.advance().unwrap(), InputProgress::Pending);
    fx.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        fx.run.context.pump_once(&mut ports(&mut fx.run.clock)),
        Err(RunFailure::Wait { key: PumpDeadline::Leave, bound: expired }) if expired == bound
    ));
    assert_eq!(
        fx.run.context.deadlines.get(PumpDeadline::Leave),
        Some(bound)
    );
    assert_eq!(
        fx.run.context.flow.classifier().state(),
        ClassifierState::ResultHeld
    );
    assert!(fx.run.context.flow.operation_completion().is_none());
    assert!(!fx.run.context.input.snapshot().unwrap().result_recorded);
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    fx.no_turn();
}

#[test]
fn peer_death_during_interrupted_input_release_retains_record_without_acceptance() {
    let (mut fx, reply) = pending_release(false, false);
    shutdown(fx.peer.readiness().as_raw_fd(), Shutdown::Write).unwrap();
    assert!(fx.advance().is_err());
    assert_eq!(fx.run.context.flow.classifier().held(), Some(&reply));
    assert!(fx.run.context.flow.classifier().outstanding().is_some());
    assert!(fx.run.context.flow.operation_completion().is_none());
    assert!(fx.run.context.input.snapshot().unwrap().halted);
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .is_err()
    );
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    fx.no_turn();
}
