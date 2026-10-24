//! Actual append reservation/refusal and Cancel ordering controls for #269 Q6/Q24.
use super::*;
use crate::flow::pump_transport::DecodeStage;
use weaver_types::TokenRefusal;

fn append_refusal(fx: &PlaceFixture, refusal: &TokenRefusal) -> Vec<u8> {
    let raw = serde_json::to_vec(refusal).unwrap();
    send_bytes(fx.peer.as_fd(), &raw);
    raw
}

#[test]
fn place_append_review_not_open_and_out_of_order_latch_before_trace_only_refusal_account() {
    for refusal in [TokenRefusal::NotOpen, TokenRefusal::OutOfOrder] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("typed-fatal-append");
        let grant = fx.grant(units, 3);
        let before = fx.journal();
        let prior_refusals = fx.records(Kind::Refusal).len();
        fx.start_generation(grant);
        assert!(matches!(
            fx.sent_generation(),
            TokenDirective::AppendAndGenerate { .. }
        ));
        let raw = append_refusal(&fx, &refusal);
        let first = fx.generation_failure();
        assert_eq!(
            first,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Barrier,
                detail: "decode refused a reserved append outside its admitted state".into(),
            }
        );
        assert_eq!(fx.run().context.failure(), Some(&first));
        assert_eq!(
            fx.records(Kind::Refusal).len(),
            prior_refusals,
            "first failure must latch before its owed refusal is trace-only recorded"
        );
        assert_eq!(
            fx.journal(),
            before,
            "fatal refusal confirms no logical or physical mutation"
        );
        assert!(receive(fx.peer.as_fd()).is_none());
        fx.first_fault_attempt();
        let mut account = None;
        for _ in 0..128 {
            fx.account_step().unwrap();
            assert!(
                receive(fx.peer.as_fd()).is_none(),
                "the consumed terminal refusal must not start a new Cancel exchange"
            );
            account = fx.run().context.take_place_account();
            if account.is_some() {
                break;
            }
        }
        let account = account.expect("the complete fatal refusal must settle its original account");
        assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
        assert_eq!(account.refusal(), Some(&refusal));
        assert!(account.confirmed_generation().is_none());
        assert!(account.incomplete().is_none());
        assert_eq!(fx.records(Kind::Refusal).len(), prior_refusals + 1);
        assert_eq!(fx.journal(), before);
        assert_eq!(fx.run().context.failure(), Some(&first));
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_append_review_malformed_render_refusal_stays_nonfatal_and_allows_explicit_retry() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    let later = fx.text(9, "retry");
    fx.turn("malformed-append");
    let grant = fx.grant(units, 3);
    let before = fx.journal();
    let prior_refusals = fx.records(Kind::Refusal).len();
    fx.start_generation(grant);
    fx.sent_generation();
    // MalformedDelta is also the existing family-rendering refusal; no new
    // render-refusal variant or interpretation is invented by this fixture.
    let raw = append_refusal(&fx, &TokenRefusal::MalformedDelta);
    let GenerationCompletion::Refused(account) = fx.finish_generation() else {
        panic!("malformed rendering must remain the original nonfatal refused outcome");
    };
    assert!(fx.run().context.failure().is_none());
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert_eq!(account.refusal(), Some(&TokenRefusal::MalformedDelta));
    assert!(account.confirmed_generation().is_none());
    assert!(account.incomplete().is_none());
    assert_eq!(fx.journal(), before);
    assert_eq!(fx.records(Kind::Refusal).len(), prior_refusals + 1);
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.close_turn();
    fx.turn("malformed-retry");
    fx.generation(later, 2, 0, "", Finish::Completed);
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn place_append_review_unsent_cancel_returns_same_staged_carry_and_next_turn_remeasures() {
    let mut fx = PlaceFixture::new();
    let carry = fx.text(8, "carry");
    let expected = carry[0].messages().to_vec();
    let prefix = fx.prefix(None, s::CutMethod::Elide);
    let staged = fx.staged(prefix, ReloadTail::held(carry), 3);
    let identity = staged.identity.clone();
    let original_record = staged.recorded;
    fx.turn("unsent-staged-cancel");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        panic!("measure")
    };
    assert_eq!(delta, expected);
    fx.answer(&fx.measured(keep, 3));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("grant")
    };
    let burned_id = grant.assistant_id();
    let before = fx.journal();
    fx.start_generation(grant);
    assert!(fx.decode.placement_not_started());
    assert!(fx.decode.send_pending());
    assert!(receive(fx.peer.as_fd()).is_none());
    let run = &mut fx.summary.boot.member.run;
    run.context
        .cancel_generation(&mut fx.decode, &mut run.clock)
        .unwrap();
    assert!(
        !fx.decode.send_pending(),
        "exact unsent reservation must be withdrawn immediately"
    );
    assert!(!fx.decode.placement_not_started());
    assert!(fx.decode.failure().is_none());
    let GenerationCompletion::Cancelled(mut account) = fx.finish_generation() else {
        panic!("cancelled")
    };
    assert!(account.raw_bytes().is_none());
    assert!(account.confirmed_generation().is_none());
    assert!(account.incomplete().is_none());
    assert!(account.recorded_sequences().is_empty());
    assert_eq!(fx.journal(), before);
    assert!(
        receive(fx.peer.as_fd()).is_none(),
        "neither Append nor Cancel may have sent"
    );
    let staged = account
        .take_staged()
        .expect("unsent cancellation returns its exact reserved carry");
    assert!(std::sync::Arc::ptr_eq(&staged.identity, &identity));
    assert_eq!(staged.recorded, original_record);
    assert!(account.take_staged().is_none());
    fx.close_turn();
    fx.turn("staged-cancel-next");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        panic!("fresh measure")
    };
    assert_eq!(delta, expected);
    fx.answer(&fx.measured(keep, 3));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("next grant")
    };
    assert!(grant.assistant_id().0 > burned_id.0);
    fx.start_generation(grant);
    let sent = fx.sent_generation();
    assert!(
        matches!(sent, TokenDirective::AppendAndGenerate { turn, delta, .. }
        if turn == TurnKey("staged-cancel-next".into()) && delta == expected)
    );
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
        3,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    assert!(
        fx.run()
            .context
            .place_engine
            .as_ref()
            .unwrap()
            .staged
            .is_none()
    );
}

#[test]
fn place_append_review_private_cancel_rejects_an_actual_unsent_reservation_before_busy() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("unsent-cancel-boundary");
    let grant = fx.grant(units, 3);
    let directive = TokenDirective::AppendAndGenerate {
        turn: TurnKey(grant.turn.0.clone()),
        delta: grant.delta.clone(),
        tool_schemas: grant.schemas.clone(),
    };
    // Defensive port-boundary call with a real measured grant and real reserved
    // request. The normal context owner withdraws unsent work before this call;
    // it never reaches this refusal arm by fabricating sent/owner bits.
    let DecodeStage::Queued(reservation) = fx.decode.stage_placement(directive).unwrap() else {
        panic!("reservation")
    };
    assert!(!reservation.sent());
    assert!(reservation.belongs_to(&fx.decode));
    let asked = TokenDirective::Cancel {
        turn: TurnKey("unsent-cancel-boundary".into()),
    };
    let error = fx
        .decode
        .cancel_reserved(&reservation, TurnKey("unsent-cancel-boundary".into()))
        .unwrap_err();
    assert_eq!(error.input, asked);
    assert_eq!(
        error.failure.fault,
        crate::failure::ChannelFault::Undecodable
    );
    assert_eq!(
        error.failure.detail,
        "Cancel does not own a sent generation"
    );
    assert!(error.failure.errno.is_none());
    assert!(fx.decode.placement_not_started());
    assert!(fx.decode.failure().is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.decode.withdraw_unsent(reservation).unwrap();
    assert!(!fx.decode.send_pending());
    // The original measured grant still belongs to its real context and can
    // enter the ordinary owner after the defensive port attempt is withdrawn.
    fx.start_generation(grant);
    fx.sent_generation();
    let run = &mut fx.summary.boot.member.run;
    run.context
        .cancel_generation(&mut fx.decode, &mut run.clock)
        .unwrap();
    let mut sent = None;
    for _ in 0..32 {
        if let Some(raw) = receive(fx.peer.as_fd()) {
            sent = Some(serde_json::from_slice::<TokenDirective>(&raw).unwrap());
            break;
        }
        fx.advance_generation().unwrap();
    }
    assert_eq!(sent, Some(asked));
    let before = fx.journal();
    let terminal = generated(before.1, 3, 0, "", Finish::Stopped, before.2);
    let raw = fx.answer(&TokenAnswer::Generated(terminal.clone()));
    for _ in 0..8 {
        fx.advance_generation().unwrap();
        let run = fx.run();
        assert!(
            run.context
                .take_placed_generation(&mut run.clock)
                .unwrap()
                .is_none(),
            "the actual sent Cancel still owes its separate AtRest"
        );
    }
    fx.answer(&TokenAnswer::AtRest);
    let GenerationCompletion::Cancelled(account) = fx.finish_generation() else {
        panic!("sent cancellation")
    };
    assert_eq!(
        account.confirmed_generation().unwrap().generation(),
        &terminal
    );
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert!(account.incomplete().is_none());
    assert_eq!(fx.journal().1, terminal.resident);
    assert!(receive(fx.peer.as_fd()).is_none());
}
