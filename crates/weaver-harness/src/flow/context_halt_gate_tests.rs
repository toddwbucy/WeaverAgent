//! Real execution/read/recorder owners. Scripted gate replies prove protocol
//! custody, not that a host cgroup or device has been cleaned up.
use super::*;
use crate::flow::context::{HaltGateProgress as Progress, HaltGateRefusal as Refusal};
use crate::flow::halt_snapshot::ExternalFacts;
use std::net::Shutdown;
use weaver_types::KillCause;

const _: fn() = || {
    trait NotClone<M> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <super::super::super::HaltGate as NotClone<_>>::check;
    trait NotDefault<M> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <super::super::super::HaltGate as NotDefault<_>>::check;
    trait NotExchange<M> {
        fn check() {}
    }
    impl<T: ?Sized> NotExchange<()> for T {}
    impl<T: From<ExchangeId>> NotExchange<u8> for T {}
    let _ = <super::super::super::HaltGate as NotExchange<_>>::check;
};

fn fail_state(fx: &mut PairFixture) -> RunFailure {
    let run = &mut fx.output.input.run;
    run.state.shutdown(Shutdown::Write).unwrap();
    let context = &mut run.context;
    let failed = context.flow.transport_one(
        &mut FlowRecording {
            barrier: &mut context.barrier,
            inbox: &mut context.inbox,
            budget: &mut context.budget,
            author: &context.author,
        },
        &mut run.clock,
        TransportSource::State,
    );
    assert!(failed.is_err(), "real state EOF must be observed");
    let failure = context.reduce_flow(&mut run.clock).unwrap_err();
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::CarryFailed,
            ..
        }
    ));
    failure
}
fn install_report(fx: &mut PairFixture) {
    let run = &mut fx.output.input.run;
    let context = &mut run.context;
    context
        .freeze_halt_account(ExternalFacts {
            outstanding_pairs: context.outstanding_pairs(),
            capture: None,
            wait: context.halt_wait().cloned(),
            input_id: context.input.snapshot().map(|input| input.input_id),
        })
        .unwrap();
    context.begin_halt_report().unwrap();
}
fn report(fx: &mut PairFixture) {
    install_report(fx);
    let run = &mut fx.output.input.run;
    assert!(matches!(
        run.context.advance_halt_trace(&mut run.clock).unwrap(),
        super::super::super::HaltTraceProgress::FaultRecorded(_)
    ));
}
fn start(fx: &mut PairFixture, gate: &OrganPort) {
    fail_state(fx);
    report(fx);
    fx.output.input.run.context.begin_halt_gate(gate).unwrap();
}
fn advance(fx: &mut PairFixture, gate: &mut OrganPort) -> Progress {
    fx.output
        .input
        .run
        .context
        .advance_halt_gate(gate, &mut fx.output.input.run.clock)
        .unwrap()
}
fn packet(peer: &OwnedFd) -> Option<OrganEnvelope> {
    let mut bytes = vec![0; 8192];
    match recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(count) => Some(serde_json::from_slice(&bytes[..count]).unwrap()),
        Err(nix::errno::Errno::EAGAIN) => None,
        Err(error) => panic!("gate packet: {error}"),
    }
}
fn reply(peer: &OwnedFd, frame: &OrganEnvelope) -> Vec<u8> {
    let raw = serde_json::to_vec(frame).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
        raw.len()
    );
    raw
}
fn killed() -> OrganEnvelope {
    OrganEnvelope {
        exchange: exchange(71),
        position: Position::Close,
        payload: OrganPayload::ToolAnswer(ToolOutcome::Killed {
            by: KillCause::FlowFailure,
            partial: None,
        }),
    }
}
fn assert_interrupt(peer: &OwnedFd) {
    let message = packet(peer).expect("interrupt crossed the real socket");
    assert_eq!(message.exchange, exchange(71));
    assert_eq!(message.position, Position::Continue);
    assert!(matches!(
        message.payload,
        OrganPayload::ToolInterrupt {
            by: KillCause::FlowFailure
        }
    ));
}

#[test]
fn halt_gate_requires_failure_report_and_the_original_socket() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    assert_eq!(
        fx.output.input.run.context.begin_halt_gate(&port),
        Err(Refusal::NotFailed)
    );
    fail_state(&mut fx);
    assert_eq!(
        fx.output.input.run.context.begin_halt_gate(&port),
        Err(Refusal::ReportNotAttempted)
    );
    report(&mut fx);
    let (foreign, _) = gate();
    assert_eq!(
        fx.output.input.run.context.begin_halt_gate(&foreign),
        Err(Refusal::ForeignPort)
    );
    fx.output.input.run.context.begin_halt_gate(&port).unwrap();
    assert_eq!(
        fx.output.input.run.context.begin_halt_gate(&port),
        Err(Refusal::AlreadyStarted)
    );
    assert!(packet(&peer).is_none());
}

#[test]
fn halt_gate_interrupt_retains_exact_terminal_without_filing_or_healthy_grant() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    start(&mut fx, &port);
    let account = fx
        .output
        .input
        .run
        .context
        .halt()
        .unwrap()
        .fault_account()
        .unwrap()
        .get()
        .to_owned();
    let first = fx.output.input.run.context.failure().unwrap().clone();
    let requests = fx.requests.len();
    assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
    assert_interrupt(&peer);
    let raw = reply(&peer, &killed());
    assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
    let context = &fx.output.input.run.context;
    let (terminal, retained) = context.halt_gate_terminal().unwrap();
    assert_eq!(terminal.call_id, call);
    assert_eq!(retained, raw);
    assert!(matches!(
        terminal.reason,
        Some(s::PairFailure::FlowFailure {})
    ));
    assert_eq!(context.halt_gate().unwrap().call(), &call);
    assert_eq!(context.halt_gate().unwrap().exchange(), &exchange(71));
    assert!(context.halt_gate().unwrap().interrupt_sent());
    assert_eq!(
        context.halt().unwrap().fault_account().unwrap().get(),
        account
    );
    assert!(context.next_pair_execution().is_none());
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .take_executed_pairs(&mut fx.output.input.run.clock)
            .unwrap_err(),
        first
    );
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .reduce_flow(&mut fx.output.input.run.clock)
            .unwrap_err(),
        first
    );
    assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
    assert!(packet(&peer).is_none());
    assert_eq!(fx.requests.len(), requests);
    assert!(fx.output.input.records(Kind::ToolCallCompleted).is_empty());
    assert!(fx.output.input.records(Kind::MessageToolResult).is_empty());
}

#[test]
fn halt_gate_reconciles_the_whole_send_before_the_healthy_reducer() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    stage(&mut fx, &mut port, &call);
    port.try_send_observed(&mut fx.output.input.run.clock)
        .unwrap();
    assert!(matches!(
        packet(&peer).unwrap().payload,
        OrganPayload::Tool(_)
    ));
    assert!(!fx.output.input.run.context.pairs.executing().unwrap().2);
    // State failure is noticed before live() can run the normal send reducer.
    start(&mut fx, &port);
    assert!(!fx.output.input.run.context.pairs.executing().unwrap().2);
    assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
    assert!(fx.output.input.run.context.pairs.executing().unwrap().2);
    assert_interrupt(&peer);
    reply(&peer, &killed());
    assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
}

#[test]
fn halt_gate_withdraws_only_its_wholly_unsent_execution() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    stage(&mut fx, &mut port, &call);
    start(&mut fx, &port);
    assert_eq!(advance(&mut fx, &mut port), Progress::Withdrawn);
    let owner = fx.output.input.run.context.halt_gate().unwrap();
    assert!(matches!(
        owner.withdrawn().unwrap().payload,
        OrganPayload::Tool(_)
    ));
    assert_eq!(owner.withdrawn().unwrap().exchange, exchange(71));
    assert!(owner.interrupt().is_none());
    assert!(
        fx.output
            .input
            .run
            .context
            .pair_work
            .as_ref()
            .unwrap()
            .send
            .as_ref()
            .unwrap()
            .withdrawn()
    );
    assert!(fx.output.input.run.context.halt_gate_terminal().is_none());
    assert!(!port.send_pending());
    assert!(packet(&peer).is_none());
    assert_eq!(advance(&mut fx, &mut port), Progress::Withdrawn);
}

#[test]
fn halt_gate_matching_result_before_interrupt_and_later_eof_keeps_its_outcome() {
    for reduced in [false, true] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut port, peer) = gate();
        sent(&mut fx, &mut port, &peer, &call);
        answer(&mut port, &peer, 71);
        if reduced {
            assert_eq!(
                observe(&mut fx, &mut port),
                super::super::super::ExecutionProgress::Terminal
            );
        }
        start(&mut fx, &port);
        drop(peer);
        let _ = port.try_receive();
        let _ = port.try_receive();
        assert!(port.failure().is_some());
        assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
        let context = &fx.output.input.run.context;
        let (terminal, _) = context.halt_gate_terminal().unwrap();
        assert!(matches!(terminal.outcome, Some(ToolOutcome::Result { .. })));
        assert!(terminal.reason.is_none());
        assert!(context.halt_gate().unwrap().interrupt().is_none());
    }
}

#[test]
fn halt_gate_foreign_port_cannot_supply_a_matching_execution_number() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    start(&mut fx, &port);
    let (mut foreign, other) = gate();
    reply(&other, &killed());
    foreign.try_receive().unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .advance_halt_gate(&mut foreign, &mut fx.output.input.run.clock),
        Err(Refusal::ForeignPort)
    );
    assert!(foreign.held().is_some());
    assert!(fx.output.input.run.context.halt_gate_terminal().is_none());
    assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
}

#[test]
fn halt_gate_wrong_position_and_forged_fault_retain_raw_without_a_terminal() {
    for forged_fault in [false, true] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut port, peer) = gate();
        sent(&mut fx, &mut port, &peer, &call);
        start(&mut fx, &port);
        let mut frame = killed();
        if forged_fault {
            frame.payload = OrganPayload::ToolAnswer(ToolOutcome::Killed {
                by: KillCause::Fault,
                partial: None,
            });
        } else {
            frame.position = Position::Continue;
        }
        let raw = reply(&peer, &frame);
        assert_eq!(advance(&mut fx, &mut port), Progress::Unavailable);
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .halt_gate()
                .unwrap()
                .refused()
                .unwrap()
                .raw_bytes(),
            raw
        );
        assert!(fx.output.input.run.context.halt_gate_terminal().is_none());
        let count = fx
            .output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .secondary()
            .len();
        assert_eq!(advance(&mut fx, &mut port), Progress::Unavailable);
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .halt()
                .unwrap()
                .secondary()
                .len(),
            count
        );
        assert!(packet(&peer).is_none());
    }
}

#[test]
fn halt_gate_real_child_exit_and_malformed_packet_require_separate_cleanup() {
    for malformed in [false, true] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut port, peer) = gate();
        sent(&mut fx, &mut port, &peer, &call);
        start(&mut fx, &port);
        if malformed {
            send(peer.as_raw_fd(), b"{broken", MsgFlags::MSG_NOSIGNAL).unwrap();
        } else {
            let status = std::process::Command::new("/bin/true")
                .stdin(std::process::Stdio::from(peer))
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
            assert!(status.success());
        }
        let mut progress = Progress::Pending;
        for _ in 0..4 {
            progress = advance(&mut fx, &mut port);
            if progress == Progress::Unavailable {
                break;
            }
        }
        assert_eq!(progress, Progress::Unavailable);
        let owner = fx.output.input.run.context.halt_gate().unwrap();
        assert!(owner.transport_failure().is_some());
        if malformed {
            assert_eq!(owner.failed_wire(), Some(b"{broken".as_slice()));
        }
        assert!(fx.output.input.run.context.halt_gate_terminal().is_none());
        assert!(fx.output.input.records(Kind::ToolCallCompleted).is_empty());
    }
}

#[test]
fn halt_gate_full_trace_cannot_block_interrupt_or_repeat_it() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    fx.output.input.fill_trace();
    start(&mut fx, &port);
    assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
    assert_interrupt(&peer);
    for _ in 0..32 {
        assert_eq!(advance(&mut fx, &mut port), Progress::Pending);
    }
    assert!(packet(&peer).is_none());
    reply(&peer, &killed());
    assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
}

#[test]
fn halt_gate_original_earlier_leave_deadline_aborts_without_a_new_anchor() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    let now = fx.output.input.run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 1000).unwrap();
    fx.output
        .input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.output.input.run.clock)
        .unwrap();
    start(&mut fx, &port);
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .deadline()
            .unwrap(),
        leave.deadline_ns()
    );
    fx.output
        .input
        .run
        .clock
        .advance_to(leave.deadline_ns())
        .unwrap();
    assert_eq!(advance(&mut fx, &mut port), Progress::AbortRequired);
    let count = fx
        .output
        .input
        .run
        .context
        .halt()
        .unwrap()
        .secondary()
        .len();
    assert_eq!(advance(&mut fx, &mut port), Progress::AbortRequired);
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .secondary()
            .len(),
        count
    );
    assert!(packet(&peer).is_none());
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .deadline()
            .unwrap(),
        leave.deadline_ns()
    );
}

struct FailAfter<'a> {
    clock: &'a mut crate::flow::clock::testing::FakePumpIo,
    remaining: usize,
}
impl PumpIo for FailAfter<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.remaining == 0 {
            return Err(std::io::Error::other("halt boundary clock failed"));
        }
        self.remaining -= 1;
        self.clock.now()
    }
    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("halt gate step must not block");
    }
}

#[test]
fn halt_gate_keeps_received_terminal_before_post_work_clock_failure() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    start(&mut fx, &port);
    let raw = reply(&peer, &killed());
    let mut clock = FailAfter {
        clock: &mut fx.output.input.run.clock,
        remaining: 1,
    };
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .advance_halt_gate(&mut port, &mut clock)
            .unwrap(),
        Progress::AbortRequired
    );
    assert_eq!(
        fx.output.input.run.context.halt_gate_terminal().unwrap().1,
        raw
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .halt_gate()
            .unwrap()
            .aborted()
            .is_some()
    );
    assert!(packet(&peer).is_none());
}

#[test]
fn halt_gate_keeps_successful_interrupt_when_its_post_send_clock_fails() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    start(&mut fx, &port);
    let mut clock = FailAfter {
        clock: &mut fx.output.input.run.clock,
        remaining: 1,
    };
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .advance_halt_gate(&mut port, &mut clock)
            .unwrap(),
        Progress::AbortRequired
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .halt_gate()
            .unwrap()
            .interrupt_sent()
    );
    assert_interrupt(&peer);
    assert_eq!(advance(&mut fx, &mut port), Progress::AbortRequired);
    // The successful interrupt crossed before the observation clock failed.
    // That transport failure shuts down the socket, so no repeat ends at EOF.
    let mut bytes = [0_u8; 8192];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap(),
        0
    );
}

#[test]
fn halt_gate_does_not_send_another_owners_pending_control() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    let other = OrganEnvelope {
        exchange: exchange(72),
        position: Position::Open,
        payload: OrganPayload::Directive(weaver_types::LifecycleDirective::Lower),
    };
    assert!(matches!(port.stage(other.clone()).unwrap(), Stage::Queued));
    start(&mut fx, &port);
    assert_eq!(advance(&mut fx, &mut port), Progress::Pending);
    assert!(packet(&peer).is_none());
    assert!(port.send_pending());
    assert!(
        fx.output
            .input
            .run
            .context
            .halt_gate()
            .unwrap()
            .interrupt()
            .is_none()
    );
    assert_eq!(port.withdraw_pending(&exchange(72)).unwrap(), Some(other));
    assert_eq!(advance(&mut fx, &mut port), Progress::InterruptSent);
    assert_interrupt(&peer);
}

#[test]
fn halt_gate_raced_result_withdraws_the_backpressured_interrupt() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut port, peer) = gate();
    sent(&mut fx, &mut port, &peer, &call);
    // Fill the real socket send queue without creating another owned packet.
    let filler = vec![b'x'; 4096];
    loop {
        match send(
            port.fd().as_raw_fd(),
            &filler,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(_) => {}
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(error) => panic!("fill gate socket: {error}"),
        }
    }
    start(&mut fx, &port);
    assert_eq!(advance(&mut fx, &mut port), Progress::Pending);
    assert!(port.send_pending());
    assert!(
        fx.output
            .input
            .run
            .context
            .halt_gate()
            .unwrap()
            .interrupt()
            .is_some()
    );
    let raw = reply(
        &peer,
        &OrganEnvelope {
            exchange: exchange(71),
            position: Position::Close,
            payload: OrganPayload::ToolAnswer(ToolOutcome::Result {
                content: "already done".into(),
            }),
        },
    );
    assert_eq!(advance(&mut fx, &mut port), Progress::Terminal);
    assert_eq!(
        fx.output.input.run.context.halt_gate_terminal().unwrap().1,
        raw
    );
    assert!(!port.send_pending());
    assert!(
        !fx.output
            .input
            .run
            .context
            .halt_gate()
            .unwrap()
            .interrupt_sent()
    );
}

#[path = "context_halt_gate_review_tests.rs"]
mod review;
