//! Input owns the original request and author debt under INPUT-NO-RESIDENCY.
use super::*;
use crate::flow::context::serving_organ_loss::tests::retain_spu_for_input;

#[test]
fn spu_loss_between_offered_and_check_submission_keeps_the_original_offer() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    assert_eq!(fx.advance().unwrap(), InputProgress::Progress);
    let before = fx.run.context.input.snapshot().unwrap();
    assert_eq!(before.state, InputState::ReadyToCheck);
    assert!(before.request_id.is_none());
    retain_spu_for_input(&mut fx.run.context);
    fx.until(InputProgress::CloseReady);
    assert_eq!(
        fx.run.context.input.snapshot().unwrap().input_id,
        before.input_id
    );
    assert_eq!(fx.records(Kind::InputOffered).len(), 1);
    assert_eq!(fx.records(Kind::Refusal).len(), 1);
    let refusal = &fx.records(Kind::Refusal)[0];
    assert_eq!(refusal["payload"]["asked"]["ask"], "world_request");
    assert_eq!(
        refusal["causal_parent"],
        before.offered_sequence.unwrap().0.to_string()
    );
    fx.no_turn();
    fx.no_request();
}

#[test]
fn world_request_refusals_preserve_each_exact_offer_and_never_submit_a_check() {
    let mut fx = InputFixture::new();
    retain_spu_for_input(&mut fx.run.context);
    for (index, bytes) in [b"\xffnot JSON".as_slice(), br#"{"text":"Read file."}"#]
        .into_iter()
        .enumerate()
    {
        let frame = TurnFrame::carry_from(bytes, 42);
        fx.begin(frame.clone());
        fx.until(InputProgress::CloseReady);
        let offers = fx.records(Kind::InputOffered);
        let refusals = fx.records(Kind::Refusal);
        assert_eq!(offers.len(), index + 1);
        assert_eq!(refusals.len(), index + 1);
        let offer = &offers[index];
        let refusal = &refusals[index];
        assert_eq!(offer["payload"]["octets"], frame.octets);
        assert_eq!(offer["payload"]["input_id"], (index + 1).to_string());
        assert_eq!(refusal["causal_parent"], offer["sequence"]);
        assert_eq!(refusal["run"], offer["run"]);
        assert!(refusal["turn"].is_null());
        assert_eq!(refusal["channel"], "internal");
        assert_eq!(refusal["origin"], "runtime");
        assert_eq!(refusal["subsystem"], "harness");
        assert_eq!(
            refusal["payload"],
            serde_json::json!({
                "seam":"lifecycle", "asked":{"ask":"world_request"}, "refusal":{"kind":"no_residency"}
            })
        );
        assert!(sequence(offer) < sequence(refusal));
        assert!(
            fx.run
                .context
                .take_accepted_input(&mut fx.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fx.run.context.input_close().unwrap(),
            TurnFrame::carry(br#"{"kind":"refused","reason":"no model is resident"}"#)
        );
        fx.run
            .context
            .input_response_sent(&mut fx.run.clock)
            .unwrap();
        fx.run.context.finish_input(&mut fx.run.clock).unwrap();
        fx.no_turn();
        fx.no_request();
        assert!(fx.records(Kind::Ste100Result).is_empty());
    }
}

#[test]
fn world_request_offer_and_refusal_pressure_expire_at_the_original_gate_bound() {
    for at_refusal in [false, true] {
        let mut fx = InputFixture::new();
        retain_spu_for_input(&mut fx.run.context);
        let frame = TurnFrame::carry_from(b"not JSON", 42);
        fx.begin(frame.clone());
        if at_refusal {
            assert_eq!(fx.advance().unwrap(), InputProgress::Progress);
        }
        let bound = fx.run.context.deadlines.get(PumpDeadline::Gate).unwrap();
        fx.fill_trace();
        assert!(matches!(
            fx.advance().unwrap_err(),
            RunFailure::Flow {
                reason: FlowHaltReason::GateBound,
                ..
            }
        ));
        let waited = fx.run.context.halt_wait().unwrap();
        assert_eq!(
            (waited.began_ns.0, waited.deadline_ns.0),
            (bound.began_ns(), bound.deadline_ns())
        );
        assert_eq!(fx.run.context.input.frame(), Some(&frame));
        let pending = fx
            .run
            .context
            .pending_author()
            .expect("original author debt");
        assert_eq!(
            pending.event.kind,
            if at_refusal {
                Kind::Refusal
            } else {
                Kind::InputOffered
            }
        );
        if at_refusal {
            assert_eq!(
                pending.event.causal_parent.unwrap().0,
                sequence(&fx.records(Kind::InputOffered)[0])
            );
        }
        assert!(fx.records(Kind::Refusal).is_empty());
        assert_eq!(
            fx.run.context.input_close().unwrap(),
            TurnFrame::carry(
                br#"{"kind":"stopped","reason":"The agent stopped because state flow failed."}"#
            )
        );
        assert!(
            fx.run
                .context
                .input
                .snapshot()
                .unwrap()
                .request_id
                .is_none()
        );
        fx.no_turn();
        fx.no_request();
    }
}

#[test]
fn world_request_refusal_resumes_its_original_author_after_a_lifecycle_wait() {
    let mut fx = InputFixture::new();
    retain_spu_for_input(&mut fx.run.context);
    fx.begin(TurnFrame::carry_from(b"not JSON", 42));
    assert_eq!(fx.advance().unwrap(), InputProgress::Progress);
    let gate = fx.run.context.deadlines.get(PumpDeadline::Gate).unwrap();
    let now = fx.run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 5).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.run.clock)
        .unwrap();
    let mut io = RecordBoundaryClock {
        before_reads: 1,
        before: now,
        after: leave.deadline_ns(),
    };
    assert_eq!(
        fx.run
            .context
            .advance_input(&mut ports(&mut io))
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(fx.run.context.input_close().is_none());
    assert_eq!(
        fx.run.context.waiting_content.as_ref().unwrap().kind,
        Kind::Refusal
    );
    assert_eq!(fx.run.context.deadlines.get(PumpDeadline::Gate), Some(gate));
    fx.run.context.complete_wait(PumpDeadline::Leave);
    fx.run.clock.advance_to(leave.deadline_ns()).unwrap();
    fx.until(InputProgress::CloseReady);
    assert_eq!(fx.records(Kind::Refusal).len(), 1);
    assert_eq!(
        fx.records(Kind::Refusal)[0]["causal_parent"],
        fx.records(Kind::InputOffered)[0]["sequence"]
    );
    fx.no_turn();
    fx.no_request();
}

#[test]
fn recorded_world_request_refusal_bounds_its_remaining_measurements_and_send() {
    let mut fx = InputFixture::new();
    retain_spu_for_input(&mut fx.run.context);
    fx.begin(TurnFrame::carry_from(b"not JSON", 42));
    while fx.records(Kind::Refusal).is_empty() {
        assert_eq!(fx.advance().unwrap(), InputProgress::Progress);
    }
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    let bound = fx
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .expect("Recorded refusal starts the required-send bound");
    assert!(fx.run.context.flow.input_measurements_pending());
    fx.fill_trace();
    // The queued measurement cannot defer the close indefinitely. Repeated
    // reductions preserve the same original bound until the common pump waits.
    for _ in 0..3 {
        fx.advance().unwrap();
        assert_eq!(
            fx.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
    fx.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        fx.advance().unwrap_err(),
        RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        }
    ));
    assert_eq!(
        fx.run.context.input_close().unwrap(),
        TurnFrame::carry(
            br#"{"kind":"stopped","reason":"The agent stopped because state flow failed."}"#
        )
    );
    fx.no_turn();
    fx.no_request();
}
