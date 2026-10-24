//! Real settled-terminal and queued-Cancel account boundaries for #275 row 6.
use super::*;
use weaver_types::TokenRefusal;

fn cancel_again(fx: &mut PlaceFixture) {
    let run = &mut fx.summary.boot.member.run;
    run.context
        .cancel_generation(&mut fx.decode, &mut run.clock)
        .unwrap();
}

#[test]
fn place_refusal_boundary_repeated_settled_cancel_keeps_owed_refusal_until_recorded() {
    for refusal in [TokenRefusal::NotOpen, TokenRefusal::OutOfOrder] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("settled-refusal-custody");
        let grant = fx.grant(units, 3);
        let before = fx.journal();
        let prior_refusals = fx.records(Kind::Refusal).len();
        fx.start_generation(grant);
        fx.sent_generation();
        // Fill the actual ordinary recorder before failure stops new admission.
        // The fatal refusal consumes its terminal but returns before reservation
        // retirement, leaving a real settled reservation and an owed author.
        fx.summary.boot.member.fill_trace();
        let raw = serde_json::to_vec(&refusal).unwrap();
        send_bytes(fx.peer.as_fd(), &raw);
        let first = fx.generation_failure();
        assert!(matches!(
            &first,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Barrier,
                ..
            }
        ));
        let halt_bound = fx.run().context.halt().unwrap().deadline().unwrap();
        for _ in 0..4 {
            cancel_again(&mut fx);
            assert!(!fx.decode.send_pending());
            assert!(receive(fx.peer.as_fd()).is_none());
            assert!(
                fx.run().context.take_place_account().is_none(),
                "transport settlement cannot mark the still-owed refusal done"
            );
        }
        fx.first_fault_attempt();
        let mut ticket = None;
        for _ in 0..8 {
            fx.account_step().unwrap();
            if let Some(author) = fx.run().context.pending_author() {
                assert_eq!(author.event.kind, Kind::Refusal);
                ticket = Some(author.ticket);
                break;
            }
        }
        let ticket = ticket.expect("full trace must retain the actual refusal author");
        for _ in 0..4 {
            cancel_again(&mut fx);
            assert_eq!(fx.account_step().unwrap(), PlaceProgress::Pending);
            assert_eq!(fx.run().context.pending_author().unwrap().ticket, ticket);
            assert!(fx.run().context.take_place_account().is_none());
            assert_eq!(fx.records(Kind::Refusal).len(), prior_refusals);
            assert_eq!(fx.run().context.failure(), Some(&first));
            assert_eq!(
                fx.run().context.halt().unwrap().deadline().unwrap(),
                halt_bound
            );
            assert!(!fx.decode.send_pending());
            assert!(receive(fx.peer.as_fd()).is_none());
        }
        fx.summary.drain();
        let account = fx.settled_account();
        assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
        assert_eq!(account.refusal(), Some(&refusal));
        assert!(account.confirmed_generation().is_none());
        assert!(account.incomplete().is_none());
        assert_eq!(account.recorded_sequences().len(), 1);
        let sequence = account.recorded_sequences()[0];
        assert_eq!(
            fx.run().context.barrier.recorded_author_ticket(sequence),
            Some(ticket)
        );
        let records = fx.records(Kind::Refusal);
        assert_eq!(records.len(), prior_refusals + 1);
        assert_eq!(records.last().unwrap()["sequence"], sequence.0.to_string());
        assert_eq!(fx.journal(), before);
        assert_eq!(fx.run().context.failure(), Some(&first));
        assert_eq!(
            fx.run().context.halt().unwrap().deadline().unwrap(),
            halt_bound
        );
        assert!(fx.run().context.pending_author().is_none());
        assert!(!fx.decode.send_pending());
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_refusal_boundary_repeated_queued_cancel_keeps_its_at_rest_and_model_records() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("queued-cancel-custody");
    let before = fx.journal();
    let prior_request = fx.records(Kind::ModelRequest).len();
    let prior_output = fx.records(Kind::ModelOutput).len();
    let prior_measurement = fx.records(Kind::ModelMeasurement).len();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    fx.sent_generation();
    let terminal = generated(before.1, 3, 2, "Read result.", Finish::Completed, before.2);
    // A natural terminal on the wire has not yet settled the reservation. The
    // actual Cancel queued now must retain its own AtRest duty after Generated.
    let raw = fx.answer(&TokenAnswer::Generated(terminal.clone()));
    cancel_again(&mut fx);
    let mut sent = None;
    for _ in 0..32 {
        if let Some(bytes) = receive(fx.peer.as_fd()) {
            sent = Some(serde_json::from_slice::<TokenDirective>(&bytes).unwrap());
            break;
        }
        fx.advance_generation().unwrap();
    }
    assert_eq!(
        sent,
        Some(TokenDirective::Cancel {
            turn: TurnKey("queued-cancel-custody".into()),
        })
    );
    for _ in 0..32 {
        cancel_again(&mut fx);
        fx.advance_generation().unwrap();
        let run = fx.run();
        assert!(
            run.context
                .take_placed_generation(&mut run.clock)
                .unwrap()
                .is_none(),
            "Generated and repeated cancellation do not discharge the queued AtRest"
        );
        assert!(receive(fx.peer.as_fd()).is_none());
    }
    assert_eq!(fx.journal().1, terminal.resident);
    assert_eq!(fx.records(Kind::ModelRequest).len(), prior_request + 1);
    assert_eq!(fx.records(Kind::ModelOutput).len(), prior_output + 1);
    assert_eq!(
        fx.records(Kind::ModelMeasurement).len(),
        prior_measurement + 1
    );
    assert!(fx.run().context.failure().is_none());
    fx.answer(&TokenAnswer::AtRest);
    let GenerationCompletion::Cancelled(account) = fx.finish_generation() else {
        panic!("the complete exchange remains cancelled custody")
    };
    let proof = account.confirmed_generation().unwrap();
    assert_eq!(proof.generation(), &terminal);
    assert_eq!(proof.raw_bytes(), raw);
    assert!(account.incomplete().is_none());
    assert!(account.recorded_sequences().len() >= 3);
    assert_eq!(fx.records(Kind::ModelRequest).len(), prior_request + 1);
    assert_eq!(fx.records(Kind::ModelOutput).len(), prior_output + 1);
    assert_eq!(
        fx.records(Kind::ModelMeasurement).len(),
        prior_measurement + 1
    );
    assert!(receive(fx.peer.as_fd()).is_none());
    // Reuse the actual transport only after both terminal legs were consumed.
    let next = fx.text(9, "next");
    let next = fx.generation(next, 2, 1, "Read next.", Finish::Completed);
    assert_eq!(next.generation().resident, terminal.resident + 4);
    assert!(fx.run().context.failure().is_none());
}
