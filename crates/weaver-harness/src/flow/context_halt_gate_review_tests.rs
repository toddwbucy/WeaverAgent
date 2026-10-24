//! Review regressions use the actual batch, socket and first-fault owners.
use super::*;

fn second_ready(
    fx: &mut PairFixture,
    port: &mut OrganPort,
    peer: &OwnedFd,
    argument: &str,
) -> s::CallId {
    let original = fx.generation(vec![
        text("Read files."),
        call("ls"),
        text("Read names."),
        call(argument),
    ]);
    let effective = super::super::super::repair::effective_fixture(original);
    let calls: Vec<_> = effective
        .requests()
        .iter()
        .map(|request| request.call_id.clone())
        .collect();
    fx.output
        .input
        .run
        .context
        .begin_pair_execution(effective, &mut fx.output.input.run.clock)
        .unwrap();
    for _ in 0..512 {
        fx.step(Hold::None);
        if fx.output.input.run.context.next_pair_execution().is_some() {
            break;
        }
    }
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .next_pair_execution()
            .unwrap()
            .call_id,
        calls[0]
    );
    sent(fx, port, peer, &calls[0]);
    answer(port, peer, 71);
    assert_eq!(
        observe(fx, port),
        super::super::super::ExecutionProgress::Terminal
    );
    for _ in 0..512 {
        fx.step(Hold::None);
        if fx.output.input.run.context.next_pair_execution().is_some() {
            break;
        }
    }
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .next_pair_execution()
            .unwrap()
            .call_id,
        calls[1]
    );
    calls[1].clone()
}

#[test]
fn halt_gate_failed_second_stage_cannot_relabel_the_first_sent_ticket() {
    let mut fx = fixture(true);
    let (mut port, peer) = gate();
    let argument = "x".repeat(weaver_types::MAX_ENVELOPE_BYTES);
    let call = second_ready(&mut fx, &mut port, &peer, &argument);
    let run = &mut fx.output.input.run;
    assert!(matches!(
        run.context
            .stage_pair_execution(&call, exchange(72), Some(&mut port), &mut run.clock),
        Err(RunFailure::Transport { .. })
    ));
    let original_ticket = run
        .context
        .pair_work
        .as_ref()
        .unwrap()
        .send
        .as_ref()
        .unwrap();
    assert!(original_ticket.sent());
    assert!(original_ticket.matches_exchange(&exchange(71)));
    assert!(!original_ticket.matches_exchange(&exchange(72)));
    let failed = run
        .context
        .pair_work
        .as_ref()
        .unwrap()
        .refused_send
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(failed.exchange, exchange(72));
    assert!(serde_json::to_vec(&failed).unwrap().len() > weaver_types::MAX_ENVELOPE_BYTES);
    assert!(!run.context.reconcile_pair_send(&mut run.clock).unwrap());
    assert_eq!(
        run.context.pairs.executing(),
        Some((&call, &exchange(72), false))
    );
    fail_state(&mut fx);
    report(&mut fx);
    let run = &mut fx.output.input.run;
    let ticket = run
        .context
        .pair_work
        .as_ref()
        .unwrap()
        .send
        .as_ref()
        .unwrap();
    assert!(
        run.context
            .pairs
            .reconcile_halt_send(&port, ticket)
            .is_err()
    );
    assert_eq!(
        run.context.pairs.halt_execution(),
        Some((&call, &exchange(72), false))
    );
    assert_eq!(
        run.context.begin_halt_gate(&port),
        Err(Refusal::MismatchedSend)
    );
    assert!(run.context.halt_gate().is_none());
    let work = run.context.pair_work.as_ref().unwrap();
    assert_eq!(work.refused_send.as_ref(), Some(&failed));
    assert!(work.send.as_ref().unwrap().sent());
    assert!(work.send.as_ref().unwrap().matches_exchange(&exchange(71)));
    // Oversize failure closes the socket. Neither the second call nor an
    // interrupt crossed it, and no synthetic withdrawal account was installed.
    let mut bytes = [0; 8192];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap(),
        0
    );
}

#[test]
fn halt_gate_other_exchange_world_frame_stays_held_while_interrupt_crosses() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    start(&mut fx, &port);
    let frame = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 19,
        },
        position: Position::Open,
        payload: OrganPayload::Frame(weaver_types::TurnFrame::carry(b"Read the next task.")),
    };
    let raw = reply(&peer, &frame);
    assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
    assert_interrupt(&peer);
    assert_eq!(port.held(), Some(&frame));
    assert!(
        fx.output
            .input
            .run
            .context
            .halt_gate()
            .unwrap()
            .refused()
            .is_none()
    );
    assert_eq!(advance(&mut fx, &mut port), Progress::Pending);
    assert!(packet(&peer).is_none());
    // The separate owner takes its own traffic. No replacement input is fed to
    // the failed context and no healthy work is resumed.
    assert_eq!(port.take_received().unwrap().unwrap().raw_bytes(), raw);
    reply(&peer, &killed());
    assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
}

#[test]
fn halt_gate_closed_exchange_rejection_does_not_block_the_current_terminal() {
    let mut fx = fixture(true);
    let (mut port, peer) = gate();
    let call = second_ready(&mut fx, &mut port, &peer, "pwd");
    let run = &mut fx.output.input.run;
    run.context
        .stage_pair_execution(&call, exchange(72), Some(&mut port), &mut run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(
        run.context
            .advance_pair_send(&mut port, &mut run.clock)
            .unwrap(),
        super::super::super::ExecutionProgress::Sent
    );
    assert_eq!(packet(&peer).unwrap().exchange, exchange(72));
    start(&mut fx, &port);
    let late = OrganEnvelope {
        exchange: exchange(71),
        position: Position::Close,
        payload: OrganPayload::ToolAnswer(ToolOutcome::Result {
            content: "old answer again".into(),
        }),
    };
    let late_raw = reply(&peer, &late);
    assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
    let interrupt = packet(&peer).unwrap();
    assert_eq!(interrupt.exchange, exchange(72));
    assert!(matches!(
        interrupt.payload,
        OrganPayload::ToolInterrupt {
            by: KillCause::FlowFailure
        }
    ));
    let rejection = fx
        .output
        .input
        .run
        .context
        .pairs
        .pending_rejection()
        .unwrap()
        .record();
    assert_eq!(rejection.reason, "duplicate_return");
    assert_eq!(rejection.raw_envelope.as_bytes(), late_raw);
    let mut terminal = killed();
    terminal.exchange = exchange(72);
    let raw = reply(&peer, &terminal);
    assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
    let context = &fx.output.input.run.context;
    assert_eq!(context.halt_gate_terminal().unwrap().1, raw);
    assert_eq!(
        context
            .pairs
            .pending_rejection()
            .unwrap()
            .record()
            .raw_envelope
            .as_bytes(),
        late_raw
    );
    assert!(context.halt_gate().unwrap().refused().is_none());
    assert!(packet(&peer).is_none());
    assert!(fx.output.input.records(Kind::ToolReturnRejected).is_empty());
}

#[test]
fn halt_gate_foreign_answers_keep_one_rejection_and_hold_the_next_packet() {
    for changed_opener in [false, true] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut port, peer) = gate();
        sent(&mut fx, &mut port, &peer, &call);
        start(&mut fx, &port);
        let mut frame = killed();
        if changed_opener {
            frame.exchange.opener = Opener::Gate;
        } else {
            frame.exchange.ordinal += 1;
        }
        let raw = reply(&peer, &frame);
        assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
        assert_interrupt(&peer);
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .pairs
                .pending_rejection()
                .unwrap()
                .record()
                .raw_envelope
                .as_bytes(),
            raw
        );
        frame.exchange.ordinal += 1;
        reply(&peer, &frame);
        assert_eq!(advance(&mut fx, &mut port), Progress::Pending);
        assert_eq!(port.held(), Some(&frame));
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .pairs
                .pending_rejection()
                .unwrap()
                .record()
                .raw_envelope
                .as_bytes(),
            raw
        );
        assert!(
            fx.output
                .input
                .run
                .context
                .halt_gate()
                .unwrap()
                .refused()
                .is_none()
        );
        assert!(packet(&peer).is_none());
    }
}

#[test]
fn halt_gate_requires_an_attempted_report_and_refuses_an_aborted_report() {
    for abort in [false, true] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut port, peer) = gate();
        sent(&mut fx, &mut port, &peer, &call);
        fail_state(&mut fx);
        install_report(&mut fx);
        let run = &mut fx.output.input.run;
        if abort {
            assert!(matches!(
                run.context.advance_halt_trace(&mut run.clock).unwrap(),
                super::super::super::HaltTraceProgress::FaultRecorded(_)
            ));
            let deadline = run.context.halt().unwrap().deadline().unwrap();
            run.clock.advance_to(deadline).unwrap();
            assert_eq!(
                run.context.advance_halt_trace(&mut run.clock).unwrap(),
                super::super::super::HaltTraceProgress::AbortRequired
            );
            assert_eq!(
                run.context.begin_halt_gate(&port),
                Err(Refusal::ReportAborted)
            );
        } else {
            assert_eq!(
                run.context.begin_halt_gate(&port),
                Err(Refusal::ReportNotAttempted)
            );
            assert!(matches!(
                run.context.advance_halt_trace(&mut run.clock).unwrap(),
                super::super::super::HaltTraceProgress::FaultRecorded(_)
            ));
            run.context.begin_halt_gate(&port).unwrap();
        }
        assert!(packet(&peer).is_none());
    }
}

#[test]
fn halt_gate_stage_refusals_retain_unstaged_input_separately() {
    // The outer owner already checks busy/dead ports. Exercise its defensive
    // stage helper with actual port states, without claiming an intervening
    // race or minting a staged-send proof.
    for busy in [false, true] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut port, peer) = gate();
        sent(&mut fx, &mut port, &peer, &call);
        start(&mut fx, &port);
        if busy {
            let mut other = killed();
            other.exchange = exchange(900);
            assert!(matches!(port.stage(other).unwrap(), Stage::Queued));
        } else {
            drop(peer);
            assert!(port.try_receive().is_err());
        }
        let context = &mut fx.output.input.run.context;
        assert_eq!(
            context.stage_halt_interrupt_for_test(&mut port),
            Some(if busy {
                Progress::Pending
            } else {
                Progress::Unavailable
            })
        );
        let owner = context.halt_gate().unwrap();
        assert_eq!(owner.unstaged_interrupt().unwrap().exchange, exchange(71));
        assert!(matches!(
            owner.unstaged_interrupt().unwrap().payload,
            OrganPayload::ToolInterrupt {
                by: KillCause::FlowFailure
            }
        ));
        assert!(owner.interrupt().is_none());
        assert!(!owner.interrupt_sent());
    }
}
