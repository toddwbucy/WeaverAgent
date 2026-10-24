//! PR275 defenses around genuine owner handoffs. The named pause/failure and
//! foreign-completion instruments model private corruption, not public paths.
use super::*;

fn pending_measure(fx: &mut PlaceFixture) -> u64 {
    let units = fx.text(8, "record");
    fx.turn("measurement-ticket");
    let request = fx.request(units);
    fx.begin(request);
    fx.next_measure();
    send_bytes(
        fx.peer.as_fd(),
        &serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap(),
    );
    fx.summary.boot.member.fill_trace();
    for _ in 0..128 {
        fx.advance_placement_pressure().unwrap();
        if let Some(ticket) = fx.run().context.place_measure_author_ticket() {
            return ticket;
        }
    }
    panic!("actual measurement refusal did not own a pending ticket")
}

fn waiting_place(fx: &mut PlaceFixture) -> AuthorClaim {
    let units = fx.text(8, "record");
    fx.turn("waiting-owner");
    let request = fx.request(units);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, 3));
    fx.run().context.pause_claim_queue_for_test = true;
    for _ in 0..128 {
        match fx.advance() {
            Err(RunFailure::PlaceOwned) if fx.run().context.waiting_content.is_some() => break,
            Ok(_) => {}
            other => panic!("pause before actual queue: {other:?}"),
        }
    }
    let run = fx.run();
    assert!(run.context.failure().is_none());
    assert!(run.context.pending_author().is_none());
    assert!(run.context.waiting_content.is_some());
    AuthorClaim::Placement(run.context.place_author_identity_for_test().unwrap())
}

fn pending_close(fx: &mut PlaceFixture) -> u64 {
    let units = fx.text(8, "record");
    fx.turn("admitted-refusal-close");
    let request = fx.request(units);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, u64::MAX));
    for _ in 0..128 {
        fx.advance().unwrap();
        if !fx.records(Kind::Refusal).is_empty() {
            break;
        }
    }
    assert_eq!(fx.records(Kind::Refusal).len(), 1);
    fx.summary.boot.member.fill_trace();
    for _ in 0..128 {
        fx.advance_placement_pressure().unwrap();
        if let Some(author) = fx.run().context.pending_author() {
            assert_eq!(author.event.kind, Kind::TurnClosed);
            return author.ticket;
        }
    }
    panic!("actual refusal TurnClosed did not own its pending ticket")
}

#[test]
fn author_claim_review_same_actual_ticket_cannot_borrow_another_placement_identity() {
    let mut own = PlaceFixture::new();
    let ticket = own.pending_placement();
    let mut other = PlaceFixture::new();
    other.pending_placement();
    let foreign = other
        .run()
        .context
        .place_author_identity_for_test()
        .unwrap();
    let local = own.run().context.place_author_identity_for_test().unwrap();
    let run = own.run();
    let before = run.context.place_author_record_state();
    assert!(!run.context.claim_place_author_ticket(&foreign, ticket));
    assert_eq!(run.context.place_author_record_state(), before);
    assert!(run.context.claim_place_author_ticket(&local, ticket));
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    own.summary.drain();
    other.summary.drain();
}

#[test]
fn author_claim_review_same_actual_ticket_cannot_borrow_another_measurement_identity() {
    let mut own = PlaceFixture::new();
    let ticket = pending_measure(&mut own);
    let mut other = PlaceFixture::new();
    pending_measure(&mut other);
    let foreign = other
        .run()
        .context
        .measure_author_identity_for_test()
        .unwrap();
    let local = own
        .run()
        .context
        .measure_author_identity_for_test()
        .unwrap();
    let run = own.run();
    let before = run.context.measure_author_record_state();
    assert!(!run.context.claim_measure_author_ticket(&foreign, ticket));
    assert_eq!(run.context.measure_author_record_state(), before);
    assert!(run.context.claim_measure_author_ticket(&local, ticket));
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    own.summary.drain();
    other.summary.drain();
}

#[test]
fn author_claim_review_releasing_rejects_before_consuming_the_original_waiting_owner() {
    let mut fx = PlaceFixture::new();
    let claim = waiting_place(&mut fx);
    let original = format!("{:?}", fx.run().context.waiting_content.as_ref().unwrap());
    let run = fx.run();
    let decision = run.context.deadlines.get(PumpDeadline::Decision);
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    let result = run
        .context
        .continue_claimed_content(
            &mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: None,
                label: None,
                io: &mut run.clock,
            },
            Some(claim),
        )
        .map_err(|failure| failure.retained_reason());
    assert_eq!(result, Err(RunFailure::PlaceOwned));
    assert_eq!(
        format!("{:?}", run.context.waiting_content.as_ref().unwrap()),
        original
    );
    assert!(run.context.pending_author().is_none());
    assert!(run.context.failure().is_none());
    assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), decision);
    assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
}

#[test]
fn author_claim_review_substituted_waiting_original_keeps_its_real_foreign_claim() {
    let mut own = PlaceFixture::new();
    let claim = waiting_place(&mut own);
    let mut other = PlaceFixture::new();
    waiting_place(&mut other);
    let foreign = other.run().context.waiting_content.take().unwrap();
    let original = own.run().context.waiting_content.replace(foreign).unwrap();
    let foreign_bytes = format!("{:?}", own.run().context.waiting_content.as_ref().unwrap());
    let run = own.run();
    let failure = run
        .context
        .continue_claimed_content(
            &mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: None,
                label: None,
                io: &mut run.clock,
            },
            Some(claim),
        )
        .map_err(|failure| failure.retained_reason())
        .unwrap_err();
    assert!(
        matches!(failure, RunFailure::Flow { point: HaltPoint::Author, ref detail, .. }
        if detail == "waiting author does not belong to the exact private claim")
    );
    assert_eq!(
        format!("{:?}", run.context.waiting_content.as_ref().unwrap()),
        foreign_bytes
    );
    assert!(run.context.pending_author().is_none());
    assert!(
        original.claim.is_some(),
        "test still owns the exact displaced carrier"
    );
}

#[test]
fn author_claim_review_placement_completion_backstop_rejects_actual_foreign_record() {
    let mut fx = PlaceFixture::new();
    let ticket = fx.pending_placement();
    let before = fx.run().context.place_author_record_state();
    let (_, foreign) = record_replacement_author(&mut fx, ticket);
    let run = fx.run();
    let failure = run
        .context
        .complete_place_author(foreign, &mut run.clock)
        .unwrap_err();
    assert!(matches!(failure, RunFailure::Flow { ref detail, .. }
        if detail == "placement completion has another author's ticket"));
    assert_eq!(run.context.place_author_record_state(), before);
    assert_eq!(run.context.failure(), Some(&failure));
}

#[test]
fn author_claim_review_measurement_completion_backstop_rejects_actual_foreign_record() {
    let mut fx = PlaceFixture::new();
    let ticket = pending_measure(&mut fx);
    let before = fx.run().context.measure_author_record_state();
    let (_, foreign) = record_replacement_author(&mut fx, ticket);
    let run = fx.run();
    let failure = run
        .context
        .complete_measure_author(foreign, &mut run.clock)
        .unwrap_err();
    assert!(matches!(failure, RunFailure::Flow { ref detail, .. }
        if detail == "measurement completion has another author's ticket"));
    assert_eq!(run.context.measure_author_record_state(), before);
    assert_eq!(run.context.failure(), Some(&failure));
}

#[test]
fn author_claim_review_lost_claim_after_real_allocation_preserves_queued_original_and_faults() {
    let mut fx = PlaceFixture::new();
    let prior_placements = fx.records(Kind::StatePlacement).len();
    let _claim = waiting_place(&mut fx);
    let original = format!(
        "{:?}",
        fx.run().context.waiting_content.as_ref().unwrap().event
    );
    fx.run().context.reject_queued_claim_for_test = true;
    let failure = fx.advance().unwrap_err();
    assert!(
        matches!(failure, RunFailure::Flow { point: HaltPoint::Author, ref detail, .. }
        if detail == "queued author lost its exact private owner")
    );
    let run = fx.run();
    let pending = run.context.pending_author().unwrap();
    assert_eq!(format!("{:?}", pending.event), original);
    assert_eq!(run.context.place_author_record_state().unwrap().0, None);
    assert_eq!(run.context.place_author_record_state().unwrap().1, None);
    assert!(run.context.waiting_content.is_none());
    assert_eq!(fx.records(Kind::StatePlacement).len(), prior_placements);
}

#[test]
fn author_claim_review_failed_post_recorded_observer_keeps_actual_place_fact_without_live_result() {
    for closing in [false, true] {
        let mut fx = PlaceFixture::new();
        let prior_placements = fx.records(Kind::StatePlacement).len();
        let ticket = if closing {
            pending_close(&mut fx)
        } else {
            fx.pending_placement()
        };
        let before_turn = fx.run().context.open_turn.clone();
        assert!(before_turn.is_some());
        let decision = fx.run().context.deadlines.get(PumpDeadline::Decision);
        fx.summary.drain();
        fx.run()
            .context
            .barrier
            .fail_next_recorded_observer_for_test();
        let failure = fx.advance().unwrap_err();
        let run = fx.run();
        assert_eq!(run.context.phase(), RecordingPhase::Failed);
        let (kept_ticket, sequence, _) = run.context.place_author_record_state().unwrap();
        let sequence =
            sequence.expect("already admitted original must bind despite observer failure");
        assert_eq!(kept_ticket, Some(ticket));
        assert_eq!(
            run.context.barrier.recorded_author_ticket(sequence),
            Some(ticket)
        );
        assert_eq!(run.context.failure(), Some(&failure));
        assert_eq!(
            run.context.take_placement(&mut run.clock).unwrap_err(),
            failure
        );
        assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), decision);
        if closing {
            assert!(
                run.context.open_turn.is_none(),
                "actual Recorded TurnClosed closes only its original turn"
            );
            let frame = run
                .context
                .place_close
                .as_ref()
                .expect("the truthful stopped frame remains retained");
            let frame: serde_json::Value =
                serde_json::from_slice(&frame.octets().unwrap()).unwrap();
            assert_eq!(frame["kind"], "stopped");
            assert_eq!(frame["turn"], before_turn.as_ref().unwrap().0);
            let actual_run = run
                .context
                .place_engine
                .as_ref()
                .unwrap()
                .boot
                .repaired()
                .checked_open()
                .unwrap()
                .run();
            assert_eq!(frame["run"], serde_json::json!(actual_run));
            assert!(
                run.context.place_close().is_none(),
                "Failed cannot expose a successful caller response"
            );
            assert_eq!(fx.records(Kind::TurnClosed).len(), 1);
        } else {
            assert_eq!(run.context.open_turn, before_turn);
            assert_eq!(fx.records(Kind::StatePlacement).len(), prior_placements + 1);
        }
    }
}

#[test]
fn author_claim_review_failed_post_recorded_observer_keeps_actual_measure_fact_without_live_result()
{
    let mut fx = PlaceFixture::new();
    let ticket = pending_measure(&mut fx);
    let decision = fx.run().context.deadlines.get(PumpDeadline::Decision);
    fx.summary.drain();
    fx.run()
        .context
        .barrier
        .fail_next_recorded_observer_for_test();
    let failure = fx.advance().unwrap_err();
    let run = fx.run();
    assert_eq!(run.context.phase(), RecordingPhase::Failed);
    let (kept_ticket, sequence) = run.context.measure_author_record_state().unwrap();
    let sequence = sequence.expect("actual admitted measurement refusal must retain its record");
    assert_eq!(kept_ticket, Some(ticket));
    assert_eq!(
        run.context.barrier.recorded_author_ticket(sequence),
        Some(ticket)
    );
    assert_eq!(run.context.failure(), Some(&failure));
    assert_eq!(
        run.context.take_placement(&mut run.clock).unwrap_err(),
        failure
    );
    assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), decision);
    assert!(run.context.open_turn.is_some());
    assert!(run.context.place_close().is_none());
}

#[test]
fn author_claim_review_trace_only_binding_survives_a_later_real_author_witness() {
    for measurement in [false, true] {
        let mut fx = PlaceFixture::new();
        let ticket = if measurement {
            pending_measure(&mut fx)
        } else {
            fx.pending_placement()
        };
        let run = fx.run();
        let decision = run.context.deadlines.get(PumpDeadline::Decision);
        // A real failure latch, with a named private scheduling seam around the
        // actual pending author. Normal admission prevents a second live owner.
        let first = run.context.fail(
            &mut run.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "trace-only interleave fixture first failure",
        );
        fx.first_fault_attempt();
        fx.summary.drain();
        let run = fx.run();
        let sequence = run
            .context
            .resume_trace_only_and_bind()
            .unwrap()
            .expect("actual older ticket Recorded");
        assert_eq!(
            run.context.barrier.recorded_author_ticket(sequence),
            Some(ticket)
        );
        // A second typed refusal is the defensive scheduling instrument; its
        // actual recorder admission replaces the one-slot barrier witness.
        let mut later = crate::flow::context::tests::event(
            Kind::Refusal,
            Some(Payload::Refusal(
                serde_json::value::to_raw_value(&weaver_types::RefusalRecord::Lifecycle {
                    asked: weaver_types::LifecycleAsk::Enter,
                    refusal: weaver_types::LifecycleRefusal::OutOfOrder,
                })
                .unwrap(),
            )),
        );
        later.turn = run.context.open_turn.clone();
        run.context
            .barrier
            .queue_trace_only_authored(later, &run.context.author, run.clock.now().unwrap(), 1)
            .unwrap();
        let later = run.context.resume_trace_only_and_bind().unwrap().unwrap();
        assert_ne!(later, sequence);
        assert!(
            run.context
                .barrier
                .recorded_author_ticket(sequence)
                .is_none()
        );
        if measurement {
            assert_eq!(
                run.context.measure_author_record_state(),
                Some((Some(ticket), Some(sequence)))
            );
        } else {
            assert_eq!(
                run.context.place_author_record_state().unwrap().1,
                Some(sequence)
            );
        }
        assert_eq!(run.context.failure(), Some(&first));
        assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), decision);
        assert!(run.context.take_placement(&mut run.clock).is_err());
    }
}
