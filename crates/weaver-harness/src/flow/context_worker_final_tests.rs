//! Actual worker loop, sockets and filing receipts with scripted model/member
//! peers. These are composition checks, not language or deployment qualification.
use super::emission::tests::{FAMILY, sequence};
use super::tests::until;
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::worker_startup::input::management::tests::Guard;
use crate::flow::context::worker_startup::tests::{Member, worker};
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    if mode.contains("-manual-") {
        return manual(serving, member, reader, clock, socket, root, mode);
    }
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let input = guard.input.as_mut().unwrap();
    assert!(input.close_answer().is_err());
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Accepted,
    );
    let mut flow = worker::turn::Workflow::begin(
        guard.input.take().unwrap(),
        TurnKey("generation-turn".into()),
        &FAMILY,
        CutMethod::Flush,
    )
    .ok()
    .unwrap();
    let done = AtomicBool::new(false);
    let mut peer = None;
    reader.set_nonblocking(true).unwrap();
    let rows = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(45);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(45);
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
                .split(|b| *b == b'\n')
                .filter(|s| !s.is_empty())
                .map(|s| serde_json::from_slice::<serde_json::Value>(s).unwrap())
                .collect::<Vec<_>>()
        });
        let end = Instant::now() + Duration::from_secs(30);
        while flow.progress() == worker::turn::Progress::Pending && Instant::now() < end {
            if mode.contains("-management-") && flow.rounds() == 1 && peer.is_none() {
                use crate::flow::context::worker_startup::input::management::tests::dial;
                use nix::sys::socket::{MsgFlags, send};
                use std::os::fd::AsRawFd;
                let connection = dial(socket);
                let directive = if mode.ends_with("stop-before") {
                    weaver_types::LifecycleDirective::Stop {
                        cause: weaver_types::Cause { uid: 73 },
                    }
                } else if mode.ends_with("leave") {
                    weaver_types::LifecycleDirective::Leave {
                        cause: weaver_types::Cause { uid: 73 },
                        forced: false,
                        rollback: false,
                    }
                } else {
                    weaver_types::LifecycleDirective::Observe
                };
                let frame = OrganEnvelope {
                    exchange: weaver_types::ExchangeId {
                        opener: Opener::Admin,
                        ordinal: 98,
                    },
                    position: Position::Open,
                    payload: Payload::Directive(directive),
                };
                let bytes = serde_json::to_vec(&frame).unwrap();
                assert_eq!(
                    send(connection.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                    Ok(bytes.len())
                );
                peer = Some(connection);
            }
            flow.advance();
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    });
    let progress = flow.progress();
    let detail = flow.detail();
    let rounds = flow.rounds();
    if mode.ends_with("management-leave") {
        assert_eq!(progress, worker::turn::Progress::DirectivePending);
        assert_eq!(rounds, 1);
        let flow = flow
            .finish()
            .err()
            .expect("unsupported directive retains workflow");
        guard.management = Some(
            flow.into_owner()
                .err()
                .expect("actual management owner retained"),
        );
        assert!(!rows.iter().any(|r| r["kind"] == "turn.closed"));
        assert!(!root.join("closed.json").exists());
        return;
    }
    match flow.into_owner() {
        Ok(owner) => guard.input = Some(owner),
        Err(owner) => {
            let management_detail = owner.detail().map(str::to_owned);
            guard.management = Some(owner);
            panic!("workflow retained management: {progress:?}; {detail:?}; {management_detail:?}");
        }
    }
    let input = guard.input.as_mut().unwrap();
    if mode.ends_with("management-stop-before") {
        use crate::flow::context::worker_startup::input::management::tests::read_packet;
        assert_eq!(
            progress,
            worker::turn::Progress::Aborted,
            "{detail:?}; {:?}",
            input.detail()
        );
        let reply: OrganEnvelope =
            serde_json::from_slice(&read_packet(peer.as_ref().unwrap()).unwrap()).unwrap();
        assert!(matches!(
            reply.payload,
            Payload::Answer(weaver_types::LifecycleAnswer::TurnAborted { .. })
        ));
        let close = rows.iter().find(|r| r["kind"] == "turn.closed").unwrap();
        let terminals: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "tool.call.completed")
            .collect();
        assert_eq!(terminals.len(), 2);
        assert!(terminals.iter().all(|r| sequence(r) < sequence(close)));
        assert!(!root.join("final-pass-1.json").exists());
        return;
    }
    if let Some(peer) = peer {
        use crate::flow::context::worker_startup::input::management::tests::read_packet;
        use weaver_types::state_flow::enter::{
            ObservationAnswer, ObservationContent, ObservationEnvelope,
        };
        let reply: ObservationEnvelope =
            serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
        assert!(matches!(
            reply.payload,
            ObservationContent::Answer(ObservationAnswer::State {
                state: weaver_types::AgentState::Active,
                ..
            })
        ));
    }
    assert_eq!(
        progress,
        worker::turn::Progress::Completed,
        "{detail:?}; {:?}; {:?}",
        input.detail(),
        input.operation_detail()
    );
    assert!(input.serving.owner.context.open_turn.is_none());
    assert!(input.serving.owner.context.failure().is_none());
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    let end = Instant::now() + Duration::from_secs(2);
    while !root.join("closed.json").exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1));
    }
    let close: OrganEnvelope =
        serde_json::from_slice(&std::fs::read(root.join("closed.json")).unwrap()).unwrap();
    assert_eq!(
        close.exchange,
        input.received.as_ref().unwrap().envelope().exchange
    );
    let Payload::Frame(frame) = close.payload else {
        panic!("actual world frame")
    };
    let answer: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
    let limited = mode.contains("-round-limit");
    let round = limited || mode.contains("-round-once") || mode.contains("-external-");
    let expected = if limited {
        "Recall archive.\nRecall ledger."
    } else if round {
        "Read result."
    } else if mode.contains("empty") {
        ""
    } else if mode.contains("bindings") {
        "Read zeta.\nRead alpha."
    } else if mode.contains("correction") {
        "Read fixed.\nRead caf\u{e9}."
    } else {
        "Read file.\nRead caf\u{e9}."
    };
    assert_eq!(answer["kind"], "answered");
    assert_eq!(answer["text"], expected);
    assert_eq!(
        answer["finish"].as_str(),
        mode.ends_with("length").then_some("length")
    );
    assert_eq!(rounds, if limited { 32 } else { usize::from(round) });
    let closes: Vec<_> = rows.iter().filter(|r| r["kind"] == "turn.closed").collect();
    assert_eq!(closes.len(), 1);
    let terminals: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "tool.call.completed")
        .collect();
    assert_eq!(
        terminals.len(),
        if limited {
            66
        } else if round {
            2
        } else {
            0
        }
    );
    assert!(terminals.iter().all(|r| sequence(r) < sequence(closes[0])));
    if limited {
        for row in &terminals[64..] {
            assert_eq!(
                row["payload"]["reason"]["not_dispatched"]["cause"],
                "round_limit"
            );
        }
        assert_eq!(
            member
                .requests
                .iter()
                .filter(|r| r.starts_with("recall:"))
                .count(),
            64
        );
        assert!(root.join("final-pass-32.json").exists());
        assert!(!root.join("final-pass-33.json").exists());
        assert_eq!(input.engine.as_ref().unwrap().rounds.len(), 32);
    }
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "message.tool_result")
            .count(),
        if limited {
            64
        } else if round {
            2
        } else {
            0
        }
    );
    // Only the original begin_task mark exists; a turn close adds no task end.
    assert_eq!(rows.iter().filter(|r| r["kind"] == "state.mark").count(), 1);
    let FoundationInputResult::Completed(serving) =
        guard.input.take().unwrap().finish().ok().unwrap()
    else {
        panic!("completed handoff")
    };
    guard.input = Some(serving.input());
}

fn manual(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    use super::emission::tests::{selection, stop};
    use super::tests::step;
    use crate::flow::context::startup_composition::tests::CircuitIo;
    use crate::flow::context::worker_startup::input::management::{
        FoundationManagementProgress,
        stop::tests::{fill, fill_trace},
        tests::{dial, read_packet, trace},
    };
    use nix::sys::socket::{MsgFlags, send};
    use std::os::fd::AsRawFd;
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let input = guard.input.as_mut().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Accepted,
    );
    input.start_task(TurnKey("generation-turn".into())).unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Task);
    input.place_task("task".into(), CutMethod::Flush).unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Placed);
    input.generate().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Generated,
    );
    assert!(input.close_answer().is_err());
    input.repair(&FAMILY).unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Effective,
    );
    input
        .sort_outbound(selection(input.effective_emission().unwrap()))
        .unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Sorted);
    assert!(input.close_answer().is_err());
    if mode.contains("round-held") {
        member.hold_kind = Some("tool.call.completed");
        input.bar_round_limit().unwrap();
        for _ in 0..2000 {
            step(input, member, clock, mode);
            if member.held_file.is_some() {
                break;
            }
        }
        assert!(member.held_file.is_some());
        for _ in 0..8 {
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::Pending
            );
            assert!(input.close_answer().is_err());
        }
        assert!(!root.join("closed.json").exists());
        member.hold_kind = None;
    } else {
        input.begin_pairs().unwrap();
    }
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::PairsComplete,
    );
    assert!(!member.requests.iter().any(|r| r.starts_with("recall:")));
    let mut rows = trace(reader);
    let pressure = mode.ends_with("gate-pressure")
        || mode.ends_with("stop-after")
        || mode.ends_with("deadline")
        || mode.ends_with("send-clock");
    if pressure {
        fill(input.serving.owner.gate.fd().as_raw_fd());
    }
    input.close_answer().unwrap();
    assert!(input.close_answer().is_err());
    if mode.ends_with("stop-before") {
        stop(&mut guard, member, clock, mode, socket, root);
        rows.extend(trace(reader));
        assert_eq!(
            rows.iter().filter(|r| r["kind"] == "turn.closed").count(),
            1
        );
        assert_eq!(
            guard.input.as_ref().unwrap().progress(),
            FoundationInputProgress::Aborted
        );
        return;
    }
    if mode.ends_with("trace-failure") {
        input
            .serving
            .owner
            .context
            .barrier
            .fail_next_recorded_observer_for_test();
        assert_eq!(
            step(input, member, clock, mode),
            FoundationInputProgress::Stopped
        );
        let first = input.serving.owner.context.failure().cloned();
        assert!(first.is_some());
        assert!(input.engine.as_ref().unwrap().answer.is_some());
        assert!(!root.join("closed.json").exists());
        assert!(input.close_answer().is_err());
        assert_eq!(
            step(input, member, clock, mode),
            FoundationInputProgress::Stopped
        );
        assert_eq!(input.serving.owner.context.failure(), first.as_ref());
        return;
    }
    if mode.ends_with("trace-pressure") {
        let count = fill_trace(input.serving.owner.context, clock);
        struct Wait<'a> {
            clock: &'a mut FakePumpIo,
            reader: &'a mut UnixStream,
            root: &'a Path,
            count: usize,
            seen: bool,
        }
        impl PumpIo for Wait<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                self.clock.now()
            }
            fn wait(
                &mut self,
                interests: &[crate::flow::clock::WaitInterest<'_>],
                deadline: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                if !self.seen {
                    assert!(
                        !self.root.join("closed.json").exists(),
                        "no world reply before Recorded close"
                    );
                    let mut records = 0;
                    let mut bytes = [0; 65536];
                    let end = Instant::now() + Duration::from_secs(3);
                    while records < self.count {
                        assert!(Instant::now() < end);
                        match self.reader.read(&mut bytes) {
                            Ok(n) => records += bytes[..n].iter().filter(|b| **b == b'\n').count(),
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                            other => panic!("trace: {other:?}"),
                        }
                        std::thread::yield_now();
                    }
                    self.seen = true;
                }
                CircuitIo(self.clock).wait(interests, deadline)
            }
        }
        let mut io = Wait {
            clock,
            reader,
            root,
            count,
            seen: false,
        };
        input.advance_with_io(&mut io);
        assert!(io.seen, "actual trace backpressure exercised");
    } else {
        step(input, member, clock, mode);
    }
    if pressure {
        assert_eq!(input.progress(), FoundationInputProgress::Pending);
        assert!(
            input.serving.owner.context.open_turn.is_none(),
            "Recorded close already holds"
        );
        assert!(!root.join("closed.json").exists());
        assert!(input.engine.as_ref().unwrap().answer.is_some());
        let bound = input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .unwrap();
        if mode.ends_with("send-clock") {
            struct BrokenClock<'a> {
                clock: &'a mut FakePumpIo,
                sent: Box<dyn Fn() -> bool>,
            }
            impl PumpIo for BrokenClock<'_> {
                fn now(&mut self) -> std::io::Result<u64> {
                    if (self.sent)() {
                        Err(std::io::Error::other("fixture post-send clock"))
                    } else {
                        self.clock.now()
                    }
                }
                fn wait(
                    &mut self,
                    interests: &[crate::flow::clock::WaitInterest<'_>],
                    deadline: Option<u64>,
                ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                    CircuitIo(self.clock).wait(interests, deadline)
                }
            }
            let sent = input.final_send_probe_for_test();
            std::fs::write(root.join("drain"), b"yes").unwrap();
            let mut io = BrokenClock { clock, sent };
            for _ in 0..1000 {
                if input.advance_with_io(&mut io) == FoundationInputProgress::Stopped {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(
                (io.sent)(),
                "actual send happened before failing its observation"
            );
            assert_eq!(input.progress(), FoundationInputProgress::Stopped);
            assert!(matches!(
                input.serving.owner.context.failure(),
                Some(RunFailure::Flow {
                    reason: FlowHaltReason::TraceFailed,
                    point: HaltPoint::Author,
                    ..
                })
            ));
            assert!(input.final_send_clock_failed());
            return;
        }
        if mode.ends_with("deadline") {
            clock.advance_to(bound.deadline_ns() + 1).unwrap();
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::Stopped
            );
            assert!(matches!(
                input.serving.owner.context.failure(),
                Some(RunFailure::Flow {
                    reason: FlowHaltReason::DecisionBound,
                    ..
                })
            ));
            assert!(input.engine.as_ref().unwrap().answer.is_some());
            assert!(!root.join("closed.json").exists());
            std::fs::write(root.join("drain"), b"yes").unwrap();
            return;
        }
        if mode.ends_with("stop-after") {
            let peer = dial(socket);
            let request = OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 99,
                },
                position: Position::Open,
                payload: Payload::Directive(weaver_types::LifecycleDirective::Stop {
                    cause: weaver_types::Cause { uid: 73 },
                }),
            };
            let bytes = serde_json::to_vec(&request).unwrap();
            assert_eq!(
                send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
                Ok(bytes.len())
            );
            for _ in 0..100 {
                input.observe(&mut CircuitIo(clock)).unwrap();
                if input.progress() == FoundationInputProgress::ManagementPending {
                    break;
                }
            }
            guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
            for _ in 0..8 {
                guard
                    .management
                    .as_mut()
                    .unwrap()
                    .advance_with_io(&mut CircuitIo(clock));
            }
            assert!(read_packet(&peer).is_none());
            std::fs::write(root.join("drain"), b"yes").unwrap();
            for _ in 0..1000 {
                if guard
                    .management
                    .as_mut()
                    .unwrap()
                    .advance_with_io(&mut CircuitIo(clock))
                    == FoundationManagementProgress::Answered
                {
                    break;
                }
                member.step(clock.now().unwrap(), mode);
                std::thread::sleep(Duration::from_millis(1));
            }
            let answer: OrganEnvelope =
                serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
            assert!(matches!(
                answer.payload,
                Payload::Answer(weaver_types::LifecycleAnswer::AtRest)
            ));
            guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
        } else {
            std::fs::write(root.join("drain"), b"yes").unwrap();
        }
    }
    let input = guard.input.as_mut().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Completed,
    );
    rows.extend(trace(reader));
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "turn.closed").count(),
        1
    );
    if mode.contains("round-held") {
        let terminals: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "tool.call.completed")
            .collect();
        assert_eq!(terminals.len(), 2);
        for row in terminals {
            assert_eq!(
                row["payload"]["reason"]["not_dispatched"]["cause"],
                "round_limit"
            );
        }
    }
    let end = Instant::now() + Duration::from_secs(2);
    while !root.join("closed.json").exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1));
    }
    let reply: OrganEnvelope =
        serde_json::from_slice(&std::fs::read(root.join("closed.json")).unwrap()).unwrap();
    let Payload::Frame(frame) = reply.payload else {
        panic!("world frame")
    };
    let reply: serde_json::Value = serde_json::from_slice(&frame.octets().unwrap()).unwrap();
    assert_eq!(
        reply["text"],
        if mode.contains("round-held") {
            "Recall ledger."
        } else {
            "Read caf\u{e9}."
        }
    );
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
}
