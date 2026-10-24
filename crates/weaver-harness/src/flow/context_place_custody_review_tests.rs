//! PR 269 assembly attribution and exact real author-ticket completion.
use super::*;
use crate::flow::pump_transport::DecodeStage;
use weaver_types::TokenRefusal;

#[path = "context_author_claim_review_tests.rs"]
mod claim_review_tests;

#[test]
fn place_refusal_after_failed_measure_admission_names_the_actual_retry_reservation() {
    for malformed in [false, true] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("retry-reservation");
        let request = fx.request(units);
        let old_assembly = request.prefix.assembly;
        let original_prefix = request.prefix.revision();
        let before = fx.journal();
        let blocker = TokenDirective::MeasureDelta {
            keep: before.1,
            delta: vec![],
            tool_schemas: fx.schemas(),
        };
        let DecodeStage::Queued(blocker) = fx.decode.stage_reserved(blocker).unwrap() else {
            panic!("real foreign exchange reservation")
        };
        let run = &mut fx.summary.boot.member.run;
        let error = run
            .context
            .begin_place(request, &mut fx.decode, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlaceError::Busy);
        assert!(error.failure.is_none());
        assert_eq!(error.input.prefix.assembly, old_assembly);
        assert_eq!(error.input.prefix.revision(), original_prefix);
        assert!(run.context.measure_request_for_test().is_none());
        assert!(receive(fx.peer.as_fd()).is_none());
        fx.decode.withdraw_unsent(blocker).unwrap();
        fx.begin(error.input);
        let actual = fx
            .run()
            .context
            .measure_request_for_test()
            .unwrap()
            .assembly_id;
        assert_ne!(
            actual, old_assembly,
            "failed measure admission burned the first private reservation"
        );
        let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
            unreachable!()
        };
        let raw = if malformed {
            let bytes = serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap();
            send_bytes(fx.peer.as_fd(), &bytes);
            bytes
        } else {
            fx.answer(&fx.measured(keep, u64::MAX))
        };
        let PlacementCompletion::Refused(refused) = fx.take() else {
            panic!("typed measured refusal")
        };
        assert_eq!(refused.assembly_id, actual);
        assert_eq!(
            refused.code,
            if malformed {
                s::PlacementCode::InvalidSpan
            } else {
                s::PlacementCode::TailOverCapacity
            }
        );
        let account = refused.account().unwrap();
        assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
        let records = fx.records(Kind::Refusal);
        let actual_record: weaver_types::RefusalRecord = records
            .iter()
            .filter_map(|r| serde_json::from_value(r["payload"].clone()).ok())
            .find(|r| matches!(r, weaver_types::RefusalRecord::Placement { .. }))
            .expect("actual placement refusal record");
        assert!(
            matches!(actual_record, weaver_types::RefusalRecord::Placement { assembly_id, .. } if assembly_id == actual)
        );
        assert_eq!(fx.journal().1, before.1);
        assert_eq!(fx.journal().6, before.6);
        assert!(fx.run().context.failure().is_none());
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

fn record_replacement_author(fx: &mut PlaceFixture, ticket: u64) -> (u64, Sequence) {
    let run = fx.run();
    // The normal single-author facade cannot replace this slot. Deliberately
    // remove its real pending ticket, then admit a separate genuine author to
    // isolate the Recorded binding defense from the earlier resume guard.
    assert!(run.context.barrier.abandon_boot_author(ticket));
    fx.summary.drain();
    let run = fx.run();
    let mut event = crate::flow::context::tests::event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(
                &serde_json::json!({"fixture":"different genuine author"}),
            )
            .unwrap(),
        )),
    );
    event.turn = run.context.open_turn.clone();
    assert!(
        event.turn.is_some(),
        "different author carries the real current turn"
    );
    let progress = run.context.barrier.queue_authored_with_clock(
        event,
        RecordPurpose::Observation,
        &run.context.author,
        1,
        &mut run.context.inbox,
        &mut run.context.budget,
        &mut || run.clock.now(),
    );
    assert!(matches!(progress.progress, RecordProgress::Pending));
    let foreign = run.context.pending_author().unwrap().ticket;
    assert_ne!(foreign, ticket);
    let progress = run.context.barrier.resume_with_clock(
        &mut run.context.inbox,
        &mut run.context.budget,
        &mut || run.clock.now(),
    );
    let RecordProgress::Recorded {
        sequence,
        proof: None,
    } = progress
    else {
        panic!("different genuine author must record")
    };
    assert_eq!(
        run.context.barrier.recorded_author_ticket(sequence),
        Some(foreign)
    );
    (foreign, sequence)
}

#[test]
fn place_binding_ignores_a_different_actual_recorded_ticket_before_any_completion_or_turn_close() {
    for closing in [false, true] {
        let mut fx = PlaceFixture::new();
        let ticket = if !closing {
            fx.pending_placement()
        } else {
            let units = fx.text(8, "record");
            fx.turn("held-refusal-close");
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
            let mut held = None;
            for _ in 0..128 {
                fx.advance_placement_pressure().unwrap();
                held = fx
                    .run()
                    .context
                    .pending_author()
                    .map(|a| (a.ticket, a.event.kind));
                if held.is_some() {
                    break;
                }
            }
            let (ticket, kind) = held.expect("actual refusal TurnClosed ticket");
            assert_eq!(kind, Kind::TurnClosed);
            ticket
        };
        let owned = fx.run().context.place_author_record_state().unwrap();
        assert_eq!(owned.0, Some(ticket));
        assert_eq!(owned.1, None);
        let turn = fx.run().context.open_turn.clone();
        let (_, sequence) = record_replacement_author(&mut fx, ticket);
        let run = fx.run();
        run.context.bind_place_author(sequence);
        assert_eq!(run.context.place_author_record_state(), Some(owned));
        assert_eq!(run.context.open_turn, turn);
        assert!(
            run.context
                .take_placement(&mut run.clock)
                .unwrap()
                .is_none()
        );
        assert!(run.context.failure().is_none());
        assert!(run.context.place_close().is_none());
        assert_eq!(fx.records(Kind::TurnClosed).len(), 0);
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_measure_binding_ignores_a_different_actual_recorded_ticket() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("measurement-ticket");
    let request = fx.request(units);
    fx.begin(request);
    fx.next_measure();
    let raw = serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap();
    send_bytes(fx.peer.as_fd(), &raw);
    fx.summary.boot.member.fill_trace();
    let mut ticket = None;
    for _ in 0..128 {
        fx.advance_placement_pressure().unwrap();
        ticket = fx.run().context.place_measure_author_ticket();
        if ticket.is_some() {
            break;
        }
    }
    let ticket = ticket.expect("real measured decode-refusal ticket");
    assert_eq!(
        fx.run().context.measure_author_record_state(),
        Some((Some(ticket), None))
    );
    let (_, sequence) = record_replacement_author(&mut fx, ticket);
    let run = fx.run();
    run.context.bind_place_measure_author(sequence);
    assert_eq!(
        run.context.measure_author_record_state(),
        Some((Some(ticket), None))
    );
    assert_eq!(run.context.pending_measure_raw(), Some(raw.as_slice()));
    assert!(
        run.context
            .take_placement(&mut run.clock)
            .unwrap()
            .is_none()
    );
    assert!(run.context.failure().is_none());
    assert!(fx.records(Kind::Refusal).is_empty());
}

#[test]
fn place_immediate_and_pressured_recording_pin_the_actual_ticket_before_binding() {
    for pressure in [false, true] {
        let mut fx = PlaceFixture::new();
        let pending = if pressure {
            Some(fx.pending_placement())
        } else {
            let units = fx.text(8, "record");
            fx.turn("immediate-ticket");
            let request = fx.request(units);
            fx.begin(request);
            let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
                unreachable!()
            };
            fx.answer(&fx.measured(keep, 3));
            None
        };
        if pressure {
            fx.summary.drain();
        }
        for _ in 0..128 {
            fx.advance().unwrap();
            if fx
                .run()
                .context
                .place_author_record_state()
                .is_some_and(|(_, sequence, _)| sequence.is_some())
            {
                break;
            }
        }
        let (ticket, sequence, kind) = fx.run().context.place_author_record_state().unwrap();
        let ticket = ticket.expect("real ticket pinned even on immediate recording");
        let sequence = sequence.expect("actual placement record");
        assert_eq!(kind, Kind::StatePlacement);
        if let Some(pending) = pending {
            assert_eq!(pending, ticket);
        }
        assert_eq!(
            fx.run().context.barrier.recorded_author_ticket(sequence),
            Some(ticket)
        );
        let run = fx.run();
        let PlacementCompletion::Granted(grant) =
            run.context.take_placement(&mut run.clock).unwrap().unwrap()
        else {
            panic!("actual recorded grant")
        };
        assert_eq!(grant.placement_sequence(), sequence);
    }
}

#[test]
fn place_stopped_recording_retains_exact_ticket_witness_and_its_recorded_sequence() {
    let mut fx = PlaceFixture::new();
    let ticket = fx.pending_placement();
    let run = fx.run();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    fx.summary.drain();
    let account = fx.settled_account();
    assert!(account.incomplete().is_none());
    assert_eq!(account.recorded_sequences().len(), 1);
    let sequence = account.recorded_sequences()[0];
    assert_eq!(
        fx.run().context.barrier.recorded_author_ticket(sequence),
        Some(ticket)
    );
    let records = fx.records(Kind::StatePlacement);
    let actual = records.last().unwrap()["sequence"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert_eq!(sequence.0, actual);
    assert!(fx.run().context.pending_author().is_none());
    assert!(fx.run().context.completed_author.is_none());
    assert!(fx.run().context.failure().is_none());
    assert_eq!(
        fx.journal().6,
        0,
        "trace-only admission grants no prospective message occurrence"
    );
    assert!(receive(fx.peer.as_fd()).is_none());
}
