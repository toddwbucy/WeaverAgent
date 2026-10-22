//! Execution crosses real trace, checker, member and gate sockets. Scripted
//! checker/member replies test custody, not production model qualification.
use super::super::input_tests::InputFixture;
use super::*;
use crate::flow::pump_transport::{OrganPort, Stage};
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use std::os::fd::{AsRawFd, OwnedFd};
use weaver_types::{
    ExchangeId, Opener, OrganEnvelope, Payload as OrganPayload, Position, ToolOutcome,
};

fn gate() -> (OrganPort, OwnedFd) {
    let (owner, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
    )
    .unwrap();
    (OrganPort::from_owned(owner).unwrap(), peer)
}
fn exchange(ordinal: u64) -> ExchangeId {
    ExchangeId {
        opener: Opener::Harness,
        ordinal,
    }
}
fn fixture(granted: bool) -> PairFixture {
    let (scheme_text, declaration, scheme) = scheme();
    let input = InputFixture::with_declaration_scheme_tools(
        None,
        super::super::tests::config(),
        Some((scheme_text, declaration)),
        granted.then(|| serving::payloads::ToolPolicy {
            clock_ms: 1234,
            measurement_ref: "synthetic-pair-policy".into(),
        }),
    );
    PairFixture {
        output: OutputFixture::from_input(input),
        scheme,
        bytes: vec![],
        requests: vec![],
        answered: vec![],
        operation: 2,
    }
}
fn begin(fx: &mut PairFixture) -> s::CallId {
    let original = fx.generation(vec![text("Read files."), call("ls")]);
    let effective = super::super::repair::effective_fixture(original);
    let call = effective.requests()[0].call_id.clone();
    fx.output
        .input
        .run
        .context
        .begin_pair_execution(effective, &mut fx.output.input.run.clock)
        .unwrap();
    call
}
fn ready(fx: &mut PairFixture) -> s::CallId {
    let call = begin(fx);
    for _ in 0..512 {
        fx.step(Hold::None);
        if fx.output.input.run.context.next_pair_execution().is_some() {
            return call;
        }
    }
    panic!("opening never filed");
}
fn stage(fx: &mut PairFixture, gate: &mut OrganPort, call: &s::CallId) {
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .stage_pair_execution(
                call,
                exchange(71),
                Some(gate),
                &mut fx.output.input.run.clock
            )
            .unwrap(),
        Ok(super::super::ExecutionProgress::Pending)
    );
}
fn sent(
    fx: &mut PairFixture,
    gate: &mut OrganPort,
    peer: &OwnedFd,
    call: &s::CallId,
) -> OrganEnvelope {
    stage(fx, gate, call);
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .advance_pair_send(gate, &mut fx.output.input.run.clock)
            .unwrap(),
        super::super::ExecutionProgress::Sent
    );
    let mut raw = vec![0; 8192];
    let count = recv(peer.as_raw_fd(), &mut raw, MsgFlags::MSG_DONTWAIT).unwrap();
    serde_json::from_slice(&raw[..count]).unwrap()
}
fn answer(gate: &mut OrganPort, peer: &OwnedFd, ordinal: u64) {
    let frame = OrganEnvelope {
        exchange: exchange(ordinal),
        position: Position::Close,
        payload: OrganPayload::ToolAnswer(ToolOutcome::Result {
            content: "whole result".into(),
        }),
    };
    let raw = serde_json::to_vec(&frame).unwrap();
    send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap();
    gate.try_receive().unwrap();
}
fn observe(fx: &mut PairFixture, gate: &mut OrganPort) -> super::super::ExecutionProgress {
    fx.output
        .input
        .run
        .context
        .observe_pair_reply(gate, &mut fx.output.input.run.clock)
        .unwrap()
}

#[test]
fn two_calls_keep_separate_receipt_windows_when_the_pump_sends_the_first() {
    struct GateWritable<'a> {
        clock: &'a mut crate::flow::clock::testing::FakePumpIo,
        gate: i32,
    }
    impl PumpIo for GateWritable<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            self.clock.now()
        }
        fn wait(
            &mut self,
            interests: &[crate::flow::clock::WaitInterest<'_>],
            _: Option<u64>,
        ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
            let index = interests
                .iter()
                .position(|interest| interest.fd.as_raw_fd() == self.gate)
                .expect("the staged gate send has a wait interest");
            assert!(
                interests[index]
                    .events
                    .contains(nix::poll::PollFlags::POLLOUT)
            );
            Ok(crate::flow::clock::WaitOutcome::Ready(vec![
                crate::flow::clock::ReadyFd {
                    index,
                    events: nix::poll::PollFlags::POLLOUT,
                },
            ]))
        }
    }

    let mut fx = fixture(true);
    let original = fx.generation(vec![
        text("Read files."),
        call("ls"),
        text("Read names."),
        call("pwd"),
    ]);
    let effective = super::super::repair::effective_fixture(original);
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
    let initial = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let (mut gate, peer) = gate();
    stage(&mut fx, &mut gate, &calls[0]);
    // This formerly reserved B's opening under A's send bound. The common
    // pump then sent A and retired the only bound while B still held a leg.
    fx.advance().unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        Some(initial)
    );
    let mut clock = GateWritable {
        clock: &mut fx.output.input.run.clock,
        gate: gate.fd().as_raw_fd(),
    };
    for _ in 0..16 {
        let mut active = ports(&mut clock);
        active.gate = Some(&mut gate);
        fx.output.input.run.context.pump_once(&mut active).unwrap();
        if fx.output.input.run.context.pairs.executing().unwrap().2 {
            break;
        }
    }
    assert!(fx.output.input.run.context.pairs.executing().unwrap().2);
    assert!(
        fx.output
            .input
            .run
            .context
            .pair_work
            .as_ref()
            .unwrap()
            .leg
            .is_none()
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    let mut bytes = vec![0; 8192];
    let count = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    let first: OrganEnvelope = serde_json::from_slice(&bytes[..count]).unwrap();
    assert_eq!(first.exchange, exchange(71));
    fx.output
        .input
        .run
        .clock
        .advance_to(initial.deadline_ns() + 20)
        .unwrap();
    assert_eq!(fx.advance().unwrap(), PairProgress::Pending);
    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    let terminal = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    fx.until_held(Hold::Terminal);
    assert!(fx.output.input.run.context.next_pair_execution().is_none());
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        Some(terminal)
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
    let second = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let elapsed = fx
        .output
        .input
        .run
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .instruction
        .decision_wait
        .elapsed_ns
        .0;
    assert!(second.began_ns() > terminal.began_ns());
    assert_eq!(second.deadline_ns() - second.began_ns(), elapsed);
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&calls[0], &mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
    // Finishing A retires its receipt wait. B must not inherit the short tail
    // of that completed barrier, nor renew its own bound while being staged.
    fx.output
        .input
        .run
        .clock
        .advance_to(terminal.deadline_ns() + 1)
        .unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .stage_pair_execution(
                &calls[1],
                exchange(72),
                Some(&mut gate),
                &mut fx.output.input.run.clock,
            )
            .unwrap(),
        Ok(super::super::ExecutionProgress::Pending)
    );
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        Some(second)
    );
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .advance_pair_send(&mut gate, &mut fx.output.input.run.clock)
            .unwrap(),
        super::super::ExecutionProgress::Sent
    );
    let count = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    let later: OrganEnvelope = serde_json::from_slice(&bytes[..count]).unwrap();
    assert_eq!(later.exchange, exchange(72));
    assert!(matches!(later.payload, OrganPayload::Tool(tool) if tool.arguments == "pwd"));
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    fx.output
        .input
        .run
        .clock
        .advance_to(second.deadline_ns() + 20)
        .unwrap();
    answer(&mut gate, &peer, 72);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    fx.complete();
    for call in &calls {
        let completed = fx
            .output
            .input
            .run
            .context
            .completed_pair(call, &mut fx.output.input.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(completed.ended().fate, s::TerminalPairFate::Completed);
        assert!(completed.raw_answer().is_some());
    }
    let pair_records: Vec<_> = fx
        .requests
        .iter()
        .filter_map(|frame| match frame {
            s::StateFrame::FilePair { filing, .. } => Some(&filing.pair),
            _ => None,
        })
        .collect();
    assert_eq!(pair_records.len(), 4);
    assert!(
        matches!(pair_records[0], s::PairRecord::Started(started) if started.call_id == calls[0])
    );
    assert!(matches!(pair_records[1], s::PairRecord::Ended(ended) if ended.call_id == calls[0]));
    assert!(
        matches!(pair_records[2], s::PairRecord::Started(started) if started.call_id == calls[1])
    );
    assert!(matches!(pair_records[3], s::PairRecord::Ended(ended) if ended.call_id == calls[1]));
    assert!(
        fx.output
            .input
            .run
            .context
            .take_executed_pairs(&mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
}

#[test]
fn terminal_handoff_keeps_the_next_wait_across_each_leave_boundary() {
    use crate::flow::clock::testing::FakePumpIo;
    use crate::flow::clock::{WaitInterest, WaitOutcome};

    struct LeaveAtSample<'a> {
        clock: &'a mut FakePumpIo,
        samples: usize,
        at: usize,
        deadline: u64,
    }
    impl PumpIo for LeaveAtSample<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            self.samples += 1;
            if self.samples == self.at {
                self.clock.advance_to(self.deadline)?;
            }
            self.clock.now()
        }
        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            deadline: Option<u64>,
        ) -> std::io::Result<WaitOutcome> {
            self.clock.wait(interests, deadline)
        }
    }

    fn handoff(at: usize) -> usize {
        let mut fx = fixture(true);
        let original = fx.generation(vec![
            text("Read files."),
            call("ls"),
            text("Read names."),
            call("pwd"),
        ]);
        let effective = super::super::repair::effective_fixture(original);
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
        let (mut gate, peer) = gate();
        sent(&mut fx, &mut gate, &peer, &calls[0]);
        answer(&mut gate, &peer, 71);
        observe(&mut fx, &mut gate);
        let terminal = fx
            .output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .unwrap();
        fx.until_held(Hold::Terminal);
        fx.member(Hold::None);
        // Complete the real member receipt while the pair owner still owns
        // its terminal leg. The next advance collects exactly this receipt.
        for _ in 0..512 {
            let progress = fx
                .output
                .input
                .run
                .context
                .advance_owned_filing(
                    super::super::receipt_driver::FilingOwner::Pair,
                    &mut ports(&mut fx.output.input.run.clock),
                )
                .unwrap();
            fx.checker();
            fx.member(Hold::None);
            if progress == FilingProgress::Complete {
                break;
            }
        }
        assert!(
            fx.output
                .input
                .run
                .context
                .flow
                .filing_receipts()
                .unwrap()
                .is_complete()
        );
        let now = fx.output.input.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        fx.output
            .input
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut fx.output.input.run.clock)
            .unwrap();
        let mut clock = LeaveAtSample {
            clock: &mut fx.output.input.run.clock,
            samples: 0,
            at,
            deadline: leave.deadline_ns(),
        };
        let progress = fx
            .output
            .input
            .run
            .context
            .advance_pairs(&mut ports(&mut clock));
        let samples = clock.samples;
        if at == usize::MAX {
            assert_eq!(progress.unwrap(), PairProgress::Progress);
            fx.output
                .input
                .run
                .clock
                .advance_to(leave.deadline_ns())
                .unwrap();
            assert_eq!(
                fx.advance().unwrap_err(),
                RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound: leave,
                }
            );
        } else {
            assert_eq!(
                progress.unwrap_err(),
                RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    bound: leave,
                },
                "Leave at clock sample {at}"
            );
        }
        let context = &mut fx.output.input.run.context;
        let work = context.pair_work.as_ref().unwrap();
        assert_eq!(work.bound, context.deadlines.get(PumpDeadline::Decision));
        if context.pairs.completed(&calls[0]).unwrap().is_some() {
            assert!(work.leg.is_none());
            assert!(work.bound.unwrap().began_ns() > terminal.began_ns());
        } else {
            assert!(work.leg.is_some());
            assert_eq!(work.bound, Some(terminal));
        }
        assert!(context.halt().is_none());
        assert_eq!(context.complete_wait(PumpDeadline::Leave), Some(leave));
        for _ in 0..512 {
            fx.step(Hold::None);
            if fx.output.input.run.context.next_pair_execution().is_some() {
                break;
            }
        }
        let context = &fx.output.input.run.context;
        assert_eq!(context.next_pair_execution().unwrap().call_id, calls[1]);
        assert!(
            context
                .pairs
                .completed(&calls[0])
                .unwrap()
                .unwrap()
                .raw_answer()
                .is_some()
        );
        assert!(
            context
                .deadlines
                .get(PumpDeadline::Decision)
                .unwrap()
                .began_ns()
                > terminal.began_ns()
        );
        samples
    }

    // Count the live boundaries instead of pinning the test to an incidental
    // number of samples. Each possible Leave yield must retain a continuation.
    let samples = handoff(usize::MAX);
    assert!(samples > 0);
    for at in 1..=samples {
        handoff(at);
    }
}

#[test]
fn matched_gate_answer_installs_its_terminal_wait_before_leave_can_yield() {
    use crate::flow::clock::testing::FakePumpIo;
    use crate::flow::clock::{WaitInterest, WaitOutcome};

    struct AnswerBoundaryClock<'a> {
        clock: &'a mut FakePumpIo,
        checked: bool,
        deadline: u64,
    }
    impl PumpIo for AnswerBoundaryClock<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            if self.checked {
                self.clock.advance_to(self.deadline)?;
            }
            self.checked = true;
            self.clock.now()
        }
        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            deadline: Option<u64>,
        ) -> std::io::Result<WaitOutcome> {
            self.clock.wait(interests, deadline)
        }
    }

    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    sent(&mut fx, &mut gate, &peer, &call);
    answer(&mut gate, &peer, 71);
    let now = fx.output.input.run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    fx.output
        .input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.output.input.run.clock)
        .unwrap();
    let mut clock = AnswerBoundaryClock {
        clock: &mut fx.output.input.run.clock,
        checked: false,
        deadline: leave.deadline_ns(),
    };
    let context = &mut fx.output.input.run.context;
    assert_eq!(
        context.observe_pair_reply(&mut gate, &mut clock).unwrap(),
        super::super::ExecutionProgress::Terminal
    );
    assert!(gate.held().is_none());
    let terminal = context.deadlines.get(PumpDeadline::Decision).unwrap();
    assert_eq!(terminal.began_ns(), now);
    assert_eq!(context.pair_work.as_ref().unwrap().bound, Some(terminal));
    assert!(context.pairs.next_terminal().is_some());
    assert_eq!(
        context.advance_pairs(&mut ports(&mut clock)).unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave,
        }
    );
    assert_eq!(
        context.deadlines.get(PumpDeadline::Decision),
        Some(terminal)
    );
    assert_eq!(context.complete_wait(PumpDeadline::Leave), Some(leave));
    fx.complete();
    let completed = fx
        .output
        .input
        .run
        .context
        .completed_pair(&call, &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(completed.raw_answer().is_some());
    assert_eq!(completed.ended().fate, s::TerminalPairFate::Completed);
}

#[test]
fn dispatch_uses_recorded_policy_and_needs_both_real_receipts() {
    let mut fx = fixture(true);
    let call = begin(&mut fx);
    let (mut gate, peer) = gate();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .stage_pair_execution(
                &call,
                exchange(71),
                Some(&mut gate),
                &mut fx.output.input.run.clock
            )
            .unwrap(),
        Err(super::super::ExecutionRefusal::NotReady)
    );
    fx.until_held(Hold::Opening);
    assert!(fx.output.input.run.context.next_pair_execution().is_none());
    for _ in 0..256 {
        fx.step(Hold::None);
        if fx.output.input.run.context.next_pair_execution().is_some() {
            break;
        }
    }
    let frame = sent(&mut fx, &mut gate, &peer, &call);
    let OrganPayload::Tool(tool) = frame.payload else {
        panic!("tool frame");
    };
    assert_eq!(tool.clock_ms, 1234);
    assert_eq!(tool.name.0, "sh");
    assert_eq!(tool.arguments, "ls");
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
}

#[test]
fn terminal_wait_is_new_and_grant_waits_for_exact_gate_attributed_commit() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let old_bound = fx.output.bound;
    let (mut gate, peer) = gate();
    sent(&mut fx, &mut gate, &peer, &call);
    fx.output
        .input
        .run
        .clock
        .advance_to(old_bound.deadline_ns() + 20)
        .unwrap();
    assert_eq!(fx.advance().unwrap(), PairProgress::Pending);
    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    let bound = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    assert!(bound.began_ns() > old_bound.deadline_ns());
    fx.until_held(Hold::Terminal);
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.complete();
    let completed = fx
        .output
        .input
        .run
        .context
        .completed_pair(&call, &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(completed.ended().fate, s::TerminalPairFate::Completed);
    assert!(completed.raw_answer().is_some());
    let terminal = fx
        .requests
        .iter()
        .find_map(|frame| match frame {
            s::StateFrame::FilePair { filing, .. }
                if matches!(filing.pair, s::PairRecord::Ended(_)) =>
            {
                Some(filing)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(terminal.source.subsystem, "gate");
    assert_eq!(terminal.source.origin, "tool");
    assert!(
        fx.output
            .input
            .run
            .context
            .take_executed_pairs(&mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
}

#[test]
fn ungranted_tool_is_filed_without_a_gate_exchange() {
    let mut fx = fixture(false);
    let call = ready(&mut fx);
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .stage_pair_execution(&call, exchange(71), None, &mut fx.output.input.run.clock)
            .unwrap(),
        Ok(super::super::ExecutionProgress::Terminal)
    );
    fx.complete();
    let pair = fx
        .output
        .input
        .run
        .context
        .completed_pair(&call, &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(pair.ended().reason, Some(s::PairFailure::NotGranted {}));
    assert!(pair.ended().execution.is_none());
}

#[test]
fn a_busy_gate_does_not_reserve_an_execution_or_retire_the_filing_wait() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, _) = gate();
    let other = OrganEnvelope {
        exchange: exchange(99),
        position: Position::Continue,
        payload: OrganPayload::ToolCancel,
    };
    assert!(matches!(gate.stage(other).unwrap(), Stage::Queued));
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .stage_pair_execution(
                &call,
                exchange(71),
                Some(&mut gate),
                &mut fx.output.input.run.clock
            )
            .unwrap(),
        Err(super::super::ExecutionRefusal::Busy)
    );
    assert!(fx.output.input.run.context.pairs.executing().is_none());
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_some()
    );
}

#[test]
fn stop_or_leave_withdraws_unsent_datagram_and_files_not_dispatched() {
    for cause in [s::NonDispatchCause::Stop, s::NonDispatchCause::Unload] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut gate, peer) = gate();
        stage(&mut fx, &mut gate, &call);
        fx.output
            .input
            .run
            .context
            .bar_executing_pairs(cause, &mut gate, &mut fx.output.input.run.clock)
            .unwrap();
        assert!(!gate.send_pending());
        let mut raw = [0; 8192];
        assert_eq!(
            recv(peer.as_raw_fd(), &mut raw, MsgFlags::MSG_DONTWAIT).unwrap_err(),
            nix::errno::Errno::EAGAIN
        );
        fx.complete();
        let pair = fx
            .output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(
            pair.ended().reason,
            Some(s::PairFailure::NotDispatched { cause })
        );
        assert!(pair.ended().execution.is_none());
    }
}

#[test]
fn sent_packet_is_accounted_before_accepting_the_stop_bar() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    stage(&mut fx, &mut gate, &call);
    gate.try_send_observed(&mut fx.output.input.run.clock)
        .unwrap(); // The common pump may send before the owner resumes.
    fx.output
        .input
        .run
        .context
        .bar_executing_pairs(
            s::NonDispatchCause::Stop,
            &mut gate,
            &mut fx.output.input.run.clock,
        )
        .unwrap();
    assert!(fx.output.input.run.context.pairs.executing().unwrap().2);
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    fx.complete();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .unwrap()
            .ended()
            .fate,
        s::TerminalPairFate::Completed
    );
}

#[test]
fn unexpected_reply_is_recorded_before_raw_custody_is_released() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    sent(&mut fx, &mut gate, &peer, &call);
    answer(&mut gate, &peer, 99);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Rejected
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .pairs
            .pending_rejection()
            .is_some()
    );
    assert_eq!(fx.advance().unwrap(), PairProgress::Pending);
    let sequence = fx
        .output
        .input
        .run
        .context
        .record_pair_rejection(&mut ports(&mut fx.output.input.run.clock))
        .unwrap();
    assert!(
        fx.output
            .input
            .run
            .context
            .pairs
            .pending_rejection()
            .is_none()
    );
    assert_eq!(
        fx.output.input.run.context.last_author_sequence,
        Some(sequence)
    );
    assert!(fx.output.input.run.context.pairs.executing().is_some());
    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    fx.complete();
}

#[test]
fn matching_final_answer_after_eof_keeps_successful_send_and_terminal_custody() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    stage(&mut fx, &mut gate, &call);
    gate.try_send_observed(&mut fx.output.input.run.clock)
        .unwrap();
    answer(&mut gate, &peer, 71);
    // Model the common pump observing EOF while the complete answer is held.
    // Keep the peer's read side open so the close does not erase queued bytes.
    nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Write).unwrap();
    let failure = gate.try_receive().unwrap_err();
    assert_eq!(failure.fault, crate::failure::ChannelFault::Closed);
    assert!(gate.failure().is_some());
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    fx.complete();
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
}

#[test]
fn duplicate_after_batch_release_is_recorded_without_another_terminal_or_filing() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    sent(&mut fx, &mut gate, &peer, &call);
    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    fx.complete();
    let count = fx.requests.len();
    fx.output
        .input
        .run
        .context
        .take_executed_pairs(&mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(fx.output.input.run.context.pair_work.is_none());

    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Rejected
    );
    assert!(fx.output.input.records(Kind::ToolReturnRejected).is_empty());
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .pairs
            .pending_rejection()
            .unwrap()
            .record()
            .reason,
        "duplicate_return"
    );
    fx.output
        .input
        .run
        .context
        .record_pair_rejection(&mut ports(&mut fx.output.input.run.clock))
        .unwrap();
    let records = fx.output.input.records(Kind::ToolReturnRejected);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["payload"]["reason"], "duplicate_return");
    assert_eq!(
        records[0]["payload"]["call_id"],
        serde_json::to_value(&call).unwrap()
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .pairs
            .pending_rejection()
            .is_none()
    );
    assert_eq!(fx.output.input.records(Kind::ToolCallCompleted).len(), 1);
    fx.member(Hold::None);
    assert_eq!(fx.requests.len(), count, "rejection is trace only");
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .unwrap()
            .ended()
            .fate,
        s::TerminalPairFate::Completed
    );
}

#[test]
fn a_foreign_gate_cannot_send_observe_or_withdraw_the_reserved_execution() {
    for action in 0..3 {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut actual, actual_peer) = gate();
        let (mut foreign, foreign_peer) = gate();
        stage(&mut fx, &mut actual, &call);
        let unrelated = OrganEnvelope {
            exchange: exchange(99),
            position: Position::Continue,
            payload: OrganPayload::ToolCancel,
        };
        assert!(matches!(foreign.stage(unrelated).unwrap(), Stage::Queued));
        answer(&mut foreign, &foreign_peer, 71);
        let held = foreign.held().unwrap().clone();
        let before_bound = fx
            .output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision);
        let result = match action {
            0 => fx
                .output
                .input
                .run
                .context
                .advance_pair_send(&mut foreign, &mut fx.output.input.run.clock)
                .map(|_| ()),
            1 => fx
                .output
                .input
                .run
                .context
                .observe_pair_reply(&mut foreign, &mut fx.output.input.run.clock)
                .map(|_| ()),
            _ => fx.output.input.run.context.bar_executing_pairs(
                s::NonDispatchCause::Stop,
                &mut foreign,
                &mut fx.output.input.run.clock,
            ),
        };
        assert_eq!(result.unwrap_err(), RunFailure::PairOwned);
        assert!(actual.send_pending());
        assert!(foreign.send_pending());
        assert_eq!(foreign.held(), Some(&held));
        assert!(!fx.output.input.run.context.pairs.executing().unwrap().2);
        assert!(fx.output.input.run.context.pairs.next_terminal().is_none());
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            before_bound
        );
        assert!(fx.output.input.run.context.halt().is_none());
        for peer in [&actual_peer, &foreign_peer] {
            let mut raw = [0; 8192];
            assert_eq!(
                recv(peer.as_raw_fd(), &mut raw, MsgFlags::MSG_DONTWAIT).unwrap_err(),
                nix::errno::Errno::EAGAIN
            );
        }
    }
}

#[test]
fn existing_leave_deadline_still_wins_during_execution_without_a_decision_clock() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    sent(&mut fx, &mut gate, &peer, &call);
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    let now = fx.output.input.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(now, 10).unwrap();
    fx.output
        .input
        .run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.output.input.run.clock)
        .unwrap();
    fx.output
        .input
        .run
        .clock
        .advance_to(bound.deadline_ns() - 1)
        .unwrap();
    assert_eq!(fx.advance().unwrap(), PairProgress::Pending);
    answer(&mut gate, &peer, 71);
    fx.output
        .input
        .run
        .clock
        .advance_to(bound.deadline_ns())
        .unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .observe_pair_reply(&mut gate, &mut fx.output.input.run.clock)
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound
        }
    );
    assert!(gate.held().is_some());
    assert!(fx.output.input.run.context.halt().is_none());
    assert!(fx.output.input.run.context.pairs.next_terminal().is_none());
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave),
        Some(bound)
    );
}

#[test]
fn invalid_semantic_answer_keeps_raw_evidence_without_halting_the_state_feed() {
    for invalid_position in [false, true] {
        let mut fx = fixture(true);
        let call = ready(&mut fx);
        let (mut gate, peer) = gate();
        sent(&mut fx, &mut gate, &peer, &call);
        let frame = OrganEnvelope {
            exchange: exchange(71),
            position: if invalid_position {
                Position::Continue
            } else {
                Position::Close
            },
            payload: OrganPayload::ToolAnswer(if invalid_position {
                ToolOutcome::Result {
                    content: "not a close".into(),
                }
            } else {
                ToolOutcome::Killed {
                    by: weaver_types::KillCause::Fault,
                    partial: None,
                }
            }),
        };
        let raw = format!(" {} ", serde_json::to_string(&frame).unwrap());
        send(peer.as_raw_fd(), raw.as_bytes(), MsgFlags::MSG_NOSIGNAL).unwrap();
        gate.try_receive().unwrap();
        let error = fx
            .output
            .input
            .run
            .context
            .observe_pair_reply(&mut gate, &mut fx.output.input.run.clock)
            .unwrap_err();
        assert!(
            matches!(error, RunFailure::Transport { source: PumpSource::Gate, failure } if failure.fault == crate::failure::ChannelFault::Undecodable)
        );
        assert!(fx.output.input.run.context.halt().is_none());
        assert!(fx.output.input.run.context.failure().is_none());
        let held = fx
            .output
            .input
            .run
            .context
            .pair_refused_gate
            .as_ref()
            .unwrap();
        assert_eq!(held.raw_bytes(), raw.as_bytes());
        assert_eq!(held.envelope(), &frame);
        assert!(fx.output.input.run.context.pairs.next_terminal().is_none());
        assert!(fx.output.input.records(Kind::ToolCallCompleted).is_empty());
        answer(&mut gate, &peer, 71);
        let next = gate.held().unwrap().clone();
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .observe_pair_reply(&mut gate, &mut fx.output.input.run.clock)
                .unwrap_err(),
            RunFailure::PairOwned
        );
        assert_eq!(gate.held(), Some(&next));
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .pair_refused_gate
                .as_ref()
                .unwrap()
                .raw_bytes(),
            raw.as_bytes()
        );
    }
}

#[test]
fn a_gate_frame_stays_with_its_owner_and_is_not_consumed_as_a_tool_answer() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    sent(&mut fx, &mut gate, &peer, &call);
    let frame = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 19,
        },
        position: Position::Open,
        payload: OrganPayload::Frame(weaver_types::TurnFrame::carry(b"Read the next task.")),
    };
    let raw = serde_json::to_vec(&frame).unwrap();
    send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap();
    gate.try_receive().unwrap();
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::OtherTraffic
    );
    assert_eq!(gate.held(), Some(&frame));
    assert!(fx.output.input.run.context.pairs.next_terminal().is_none());
    let held = gate.take_received().unwrap().unwrap();
    assert_eq!(held.raw_bytes(), raw);
    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    fx.complete();
}

#[test]
fn actual_send_ticket_retires_dispatch_wait_before_late_owner_observation() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    stage(&mut fx, &mut gate, &call);
    let bound = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    gate.try_send_observed(&mut fx.output.input.run.clock)
        .unwrap();
    fx.output
        .input
        .run
        .clock
        .advance_to(bound.deadline_ns() + 1)
        .unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .advance_pair_send(&mut gate, &mut fx.output.input.run.clock)
            .unwrap(),
        super::super::ExecutionProgress::Sent
    );
    assert!(fx.output.input.run.context.failure().is_none());
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    answer(&mut gate, &peer, 71);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Terminal
    );
    fx.complete();
}

#[test]
fn a_withdrawn_ticket_cannot_be_mistaken_for_a_successful_send() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, _) = gate();
    stage(&mut fx, &mut gate, &call);
    assert!(gate.withdraw_pending(&exchange(71)).unwrap().is_some());
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .advance_pair_send(&mut gate, &mut fx.output.input.run.clock)
            .unwrap_err(),
        RunFailure::PairOwned
    );
    assert!(!fx.output.input.run.context.pairs.executing().unwrap().2);
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_some()
    );
}

#[test]
fn rejected_reply_retains_its_exact_author_across_trace_pressure_and_leave_wait() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    sent(&mut fx, &mut gate, &peer, &call);
    answer(&mut gate, &peer, 99);
    assert_eq!(
        observe(&mut fx, &mut gate),
        super::super::ExecutionProgress::Rejected
    );
    fx.output.input.fill_trace();
    let now = fx.output.input.run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    fx.output
        .input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.output.input.run.clock)
        .unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .record_pair_rejection(&mut ports(&mut fx.output.input.run.clock))
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(fx.output.input.run.context.pair_rejected_authoring);
    assert!(
        fx.output
            .input
            .run
            .context
            .pairs
            .pending_rejection()
            .is_some()
    );
    let original_bound = fx.output.input.run.context.pair_rejection_wait.unwrap().0;
    assert!(
        fx.output
            .input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    fx.output
        .input
        .run
        .context
        .complete_wait(PumpDeadline::Leave)
        .unwrap();
    fx.output.input.drain_trace();
    fx.output
        .input
        .run
        .context
        .record_pair_rejection(&mut ports(&mut fx.output.input.run.clock))
        .unwrap();
    assert!(fx.output.input.run.context.pair_rejection_wait.is_none());
    assert!(
        fx.output
            .input
            .run
            .context
            .pairs
            .pending_rejection()
            .is_none()
    );
    assert_eq!(fx.output.input.records(Kind::ToolReturnRejected).len(), 1);
    assert!(fx.output.input.run.clock.now().unwrap() < original_bound.deadline_ns());
}

#[test]
fn actual_send_at_the_bound_is_retained_but_never_retires_the_failed_wait() {
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, _peer) = gate();
    stage(&mut fx, &mut gate, &call);
    let bound = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    fx.output
        .input
        .run
        .clock
        .advance_to(bound.deadline_ns())
        .unwrap();
    gate.try_send_observed(&mut fx.output.input.run.clock)
        .unwrap();
    assert!(matches!(
        fx.output
            .input
            .run
            .context
            .advance_pair_send(&mut gate, &mut fx.output.input.run.clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert!(fx.output.input.run.context.pairs.executing().unwrap().2);
    assert!(fx.output.input.run.context.failure().is_some());
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_some()
    );
}

#[test]
fn common_pump_reconciles_the_exact_send_before_reducing_its_expired_wait() {
    struct Clock {
        gate: i32,
        peer: i32,
        before: u64,
        after: u64,
        sampled_send: bool,
    }
    impl PumpIo for Clock {
        fn now(&mut self) -> std::io::Result<u64> {
            let mut byte = [0];
            if recv(
                self.peer,
                &mut byte,
                MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
            )
            .is_ok()
            {
                if self.sampled_send {
                    return Ok(self.after);
                }
                self.sampled_send = true;
            }
            Ok(self.before)
        }
        fn wait(
            &mut self,
            interests: &[crate::flow::clock::WaitInterest<'_>],
            _: Option<u64>,
        ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
            let index = interests
                .iter()
                .position(|interest| interest.fd.as_raw_fd() == self.gate)
                .unwrap();
            Ok(crate::flow::clock::WaitOutcome::Ready(vec![
                crate::flow::clock::ReadyFd {
                    index,
                    events: nix::poll::PollFlags::POLLOUT,
                },
            ]))
        }
    }
    let mut fx = fixture(true);
    let call = ready(&mut fx);
    let (mut gate, peer) = gate();
    stage(&mut fx, &mut gate, &call);
    let bound = fx
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let mut clock = Clock {
        gate: gate.fd().as_raw_fd(),
        peer: peer.as_raw_fd(),
        before: bound.deadline_ns() - 1,
        after: bound.deadline_ns() + 1,
        sampled_send: false,
    };
    for _ in 0..16 {
        fx.output
            .input
            .run
            .context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: Some(&mut gate),
                decode: None,
                label: None,
                io: &mut clock,
            })
            .unwrap();
        if fx.output.input.run.context.pairs.executing().unwrap().2 {
            break;
        }
    }
    assert!(clock.sampled_send);
    assert!(fx.output.input.run.context.pairs.executing().unwrap().2);
    assert!(
        fx.output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    assert!(fx.output.input.run.context.failure().is_none());
}
