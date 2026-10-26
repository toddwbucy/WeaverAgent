//! A refused loop election waits on the real pump without surrendering its
//! operation. The helper wakes it only after observing a blocking poll.
use super::*;
use crate::flow::clock::{AbsoluteBound, WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::tests::worker;
use crate::flow::context::worker_startup::{input::management::tests::Guard, tests::Member};
use nix::poll::PollFlags;
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use weaver_types::{ExchangeId, LifecycleDirective, Opener, OrganEnvelope, Payload, Position};

pub(super) fn wake_session(
    session: &mut worker::session::Session<'_>,
    socket: &Path,
    directive: LifecycleDirective,
) -> OwnedFd {
    let tid = nix::unistd::gettid();
    let issued = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let sender = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(2);
            let observed = loop {
                let syscall =
                    std::fs::read_to_string(format!("/proc/self/task/{}/syscall", tid.as_raw()))
                        .expect("read the fixture driver's current syscall");
                let fields: Vec<_> = syscall.split_whitespace().collect();
                if fields.first().and_then(|value| value.parse::<i64>().ok())
                    == Some(nix::libc::SYS_poll)
                    && fields.get(3).is_some_and(|timeout| {
                        u64::from_str_radix(timeout.trim_start_matches("0x"), 16)
                            .is_ok_and(|timeout| timeout != 0)
                    })
                {
                    break true;
                }
                if Instant::now() >= end {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            // Wake even the broken implementation, so a failed assertion can
            // return its owner to the fixture's ordinary cleanup.
            let peer = super::super::management::tests::dial(socket);
            let raw = serde_json::to_vec(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 99,
                },
                position: Position::Open,
                payload: Payload::Directive(directive),
            })
            .unwrap();
            assert_eq!(
                send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL),
                Ok(raw.len())
            );
            issued.store(true, Ordering::SeqCst);
            (peer, observed)
        });
        while !issued.load(Ordering::SeqCst) {
            assert!(matches!(
                session.advance(),
                worker::session::Progress::ElectionRefused | worker::session::Progress::Pending
            ));
        }
        let (peer, observed) = sender.join().unwrap();
        assert!(observed, "a refused election must reach a blocking poll");
        peer
    })
}

struct Wait<'a> {
    clock: &'a mut FakePumpIo,
    calls: usize,
    bound: Option<AbsoluteBound>,
    expire: bool,
    held_gate: Option<i32>,
}
impl PumpIo for Wait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        self.calls += 1;
        assert!(!interests.is_empty());
        if let Some(gate) = self.held_gate {
            let events = interests
                .iter()
                .find(|interest| interest.fd.as_raw_fd() == gate)
                .expect("a held gate retains terminal monitoring")
                .events;
            assert!(!events.contains(PollFlags::POLLIN));
        }
        if let Some(bound) = self.bound {
            assert!(
                deadline.is_some_and(|deadline| deadline <= bound.deadline_ns()),
                "wait cannot outlast the original bound"
            );
            if self.expire {
                self.clock.advance_to(bound.deadline_ns())?;
                return Ok(WaitOutcome::Timeout);
            }
        }
        let now = self.clock.now()?;
        self.clock.advance_to(now + 1)?;
        Ok(WaitOutcome::Ready(vec![]))
    }
}

fn proof(input: &FoundationInput<'_>) -> Vec<u8> {
    let Some(GenerationCompletion::Generated(proof)) = &input.engine.as_ref().unwrap().generation
    else {
        panic!("the original generation remains owned")
    };
    assert!(proof.output.is_some(), "the checked output remains owned");
    serde_json::to_vec(proof.generation()).unwrap()
}

pub(super) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_mut().unwrap();
    static MISSING: weaver_harness::FoundationRestatement =
        weaver_harness::FoundationRestatement::new("fixture.missing");
    assert!(matches!(
        input.repair(&MISSING),
        Err(super::emission::FoundationRepairAdmission::UncertifiedFamily)
    ));
    let original = input.received.as_ref().unwrap().raw_bytes().to_vec();
    let generation = proof(input);
    let stopped = mode.ends_with("bound") || mode.ends_with("failure");
    if mode.ends_with("filing") {
        for _ in 0..1000 {
            assert_eq!(
                input.drive_with_io(&mut CircuitIo(clock), true),
                FoundationInputProgress::Generated
            );
            member.step(clock.now().unwrap(), mode);
            if member.held_file.is_some() {
                break;
            }
        }
        assert!(member.held_file.is_some(), "actual assistant filing waits");
        let source = member
            .filings
            .iter()
            .find_map(|frame| match frame {
                weaver_types::state_flow::StateFrame::File { filing, .. }
                    if filing.source.kind == "message.assistant" =>
                {
                    Some(filing.block_id.clone())
                }
                _ => None,
            })
            .unwrap();
        assert!(
            !input
                .serving
                .owner
                .context
                .flow
                .ledger()
                .barrier(std::slice::from_ref(&source))
                .unwrap()
        );
        member.hold_kind = None;
        for _ in 0..1000 {
            assert_eq!(
                input.drive_with_io(&mut CircuitIo(clock), true),
                FoundationInputProgress::Generated
            );
            member.step(clock.now().unwrap(), mode);
            if input
                .serving
                .owner
                .context
                .flow
                .ledger()
                .barrier(std::slice::from_ref(&source))
                .unwrap()
            {
                break;
            }
        }
        assert!(
            input
                .serving
                .owner
                .context
                .flow
                .ledger()
                .barrier(std::slice::from_ref(&source))
                .unwrap()
        );
        assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
        assert_eq!(proof(input), generation);
    }
    if mode.ends_with("held") || mode.ends_with("gate-failure") {
        std::fs::write(root.join("election-wake"), b"wake").unwrap();
        for _ in 0..1000 {
            let progress = input.drive_with_io(&mut CircuitIo(clock), true);
            member.step(clock.now().unwrap(), mode);
            if input.serving.owner.gate.held().is_some()
                || progress == FoundationInputProgress::Stopped
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if mode.ends_with("gate-failure") {
            assert!(matches!(
                input.failure,
                Some(RunFailure::Transport {
                    source: PumpSource::Gate,
                    ..
                })
            ));
            assert!(matches!(
                input.serving.owner.context.failure(),
                Some(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                })
            ));
        } else {
            assert_eq!(
                input.serving.owner.gate.held().unwrap().exchange.ordinal,
                102
            );
            assert_eq!(input.serving.gate_exchanges.len(), 1);
            assert!(input.serving.queued_inputs.is_empty());
        }
    }
    let bound = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Decision);
    if mode.ends_with("bound") {
        assert!(
            bound.is_some(),
            "generated output retains its existing barrier"
        );
    }
    if mode.ends_with("first-failure") {
        input.serving.owner.context.fail(
            clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "fixture election wait first failure",
        );
    }
    let mut wait = Wait {
        clock,
        calls: 0,
        bound,
        expire: false,
        held_gate: mode
            .ends_with("held")
            .then(|| input.serving.owner.gate.fd().as_raw_fd()),
    };
    if !stopped {
        assert_eq!(
            input.advance_with_io(&mut wait),
            FoundationInputProgress::Generated
        );
        assert_eq!(
            wait.calls, 0,
            "ordinary election observation stays nonblocking"
        );
    }
    for _ in 0..8 {
        let progress = input.drive_with_io(&mut wait, true);
        if mode.ends_with("failure") {
            assert_eq!(progress, FoundationInputProgress::Stopped);
        } else {
            assert_eq!(progress, FoundationInputProgress::Generated);
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision),
                bound
            );
        }
        assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
        assert_eq!(proof(input), generation);
    }
    if mode.ends_with("failure") {
        assert_eq!(wait.calls, 0);
        if mode.ends_with("first-failure") {
            assert!(
                input
                    .detail()
                    .unwrap()
                    .contains("fixture election wait first failure")
            );
        }
        return;
    }
    assert!(wait.calls > 0, "the explicit wait reaches PumpIo");
    if mode.ends_with("bound") {
        wait.expire = true;
        assert_eq!(
            input.drive_with_io(&mut wait, true),
            FoundationInputProgress::Stopped
        );
        assert!(matches!(
            input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
        assert_eq!(proof(input), generation);
        let calls = wait.calls;
        assert_eq!(
            input.drive_with_io(&mut wait, true),
            FoundationInputProgress::Stopped
        );
        assert_eq!(wait.calls, calls, "a stopped owner does not wait again");
    } else {
        super::tests::stop(&mut guard, member, wait.clock, mode, socket, root);
        let input = guard.input.as_mut().unwrap();
        let calls = wait.calls;
        assert_eq!(
            input.drive_with_io(&mut wait, true),
            FoundationInputProgress::Aborted
        );
        assert_eq!(wait.calls, calls, "completed Stop does not wait again");
        if mode.ends_with("held") {
            let original = input
                .serving
                .queued_inputs
                .pop_front()
                .or_else(|| input.serving.owner.gate.take_received().unwrap())
                .expect("Stop retains the later frame in its port or serving FIFO");
            assert_eq!(original.envelope().exchange.ordinal, 102);
            assert_eq!(
                original.raw_bytes(),
                std::fs::read(root.join("election-queued.json")).unwrap()
            );
            assert_eq!(input.serving.gate_exchanges.len(), 1);
            assert!(input.serving.queued_inputs.is_empty());
            assert!(input.serving.owner.gate.held().is_none());
        }
    }
}
