//! Actual worker/gate sockets with scripted outcomes; no shell command runs.
use super::super::tests::{FAMILY, selection, sequence, stop};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::worker_startup::input::generation::queued_input_tests as queued;
use crate::flow::context::worker_startup::input::generation::tests::{step, until};
use crate::flow::context::worker_startup::input::management::tests::{Guard, trace};
use crate::flow::context::worker_startup::tests::{Member, trace_through, worker};
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::{ExchangeId, KillCause, ToolOutcome, state_flow as s};

fn expected(mode: &str, ordinal: u64) -> ToolOutcome {
    if mode.contains("refused") {
        ToolOutcome::Refused {
            reason: "fixture gate refusal".into(),
        }
    } else if mode.contains("errored") {
        ToolOutcome::Errored {
            detail: "fixture infrastructure".into(),
        }
    } else if mode.contains("killed") || mode.contains("stop-running") {
        ToolOutcome::Killed {
            partial: Some("fixture partial".into()),
            by: if mode.contains("stop-running") {
                KillCause::Cancel
            } else {
                KillCause::Clock
            },
        }
    } else {
        ToolOutcome::Result {
            content: format!("fixture result {ordinal}"),
        }
    }
}
fn packet(fd: i32, frame: &OrganEnvelope) {
    let bytes = serde_json::to_vec(frame).unwrap();
    assert_eq!(send(fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
}
pub(in crate::flow::context) fn gate(mode: &str, root: &Path, fd: i32) {
    let mut tool_child = None;
    let end = Instant::now() + Duration::from_secs(30);
    if mode.contains("stop-unsent") || mode.contains("halt-gate-unsent") {
        while !root.join("drain-external").exists() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let mut ordinal = 1;
    let mut pending = None;
    let mut previous = None;
    let mut cancels = 0;
    while Instant::now() < end {
        if mode.contains("halt-gate")
            && pending.is_some()
            && root.join("halt-failed").exists()
            && !root.join("halt-queued.json").exists()
        {
            if mode.contains("halt-gate-eof") {
                return;
            }
            let frame = OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal: 102,
                },
                position: Position::Open,
                payload: if mode.contains("halt-gate-control") {
                    Payload::Answer(weaver_types::LifecycleAnswer::GateStopped)
                } else {
                    Payload::Frame(weaver_types::TurnFrame::carry(b"Read the next task."))
                },
            };
            let raw = serde_json::to_vec(&frame).unwrap();
            std::fs::write(root.join("halt-queued.json"), &raw).unwrap();
            packet(fd, &frame);
            if mode.contains("-late") {
                let count = if mode.contains("pressure-hold") { 1 } else { 2 };
                for index in 0..count {
                    let rejected = OrganEnvelope {
                        exchange: ExchangeId {
                            opener: Opener::Harness,
                            ordinal: 999 + index,
                        },
                        position: Position::Close,
                        payload: Payload::ToolAnswer(ToolOutcome::Result {
                            content: "foreign \"result\" \\\n caf\u{e9}".into(),
                        }),
                    };
                    let raw = format!(
                        " \t\r\n{}\n\t ",
                        serde_json::to_string_pretty(&rejected)
                            .unwrap()
                            .replace('\n', "\r\n")
                            .replace("foreign", r"\u0066oreign")
                    );
                    std::fs::write(root.join(format!("halt-rejected-{index}.json")), &raw).unwrap();
                    assert_eq!(
                        send(fd, raw.as_bytes(), MsgFlags::MSG_NOSIGNAL),
                        Ok(raw.len())
                    );
                }
            }
        }
        match recv(fd, &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return,
            Ok(n) => {
                if bytes[0] == b'x' {
                    continue;
                }
                let frame: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
                match &frame.payload {
                    Payload::Directive(weaver_types::LifecycleDirective::Lower)
                        if mode.contains("-exit") =>
                    {
                        assert_eq!(frame.position, Position::Open);
                        assert_eq!(frame.exchange.opener, Opener::Harness);
                        assert_eq!(
                            frame.exchange.ordinal,
                            if mode.contains("start-pending") { 2 } else { 3 }
                        );
                        assert!(pending.is_none());
                        assert!(!root.join("halt-lower.json").exists());
                        std::fs::write(root.join("halt-lower.json"), &bytes[..n]).unwrap();
                        if mode.contains("lower-silent") {
                            continue;
                        }
                        if mode.contains("lower-late") {
                            let rejected = OrganEnvelope {
                                exchange: ExchangeId {
                                    opener: Opener::Harness,
                                    ordinal: 2,
                                },
                                position: Position::Close,
                                payload: Payload::ToolAnswer(ToolOutcome::Result {
                                    content: "late exact output".into(),
                                }),
                            };
                            let raw = format!(
                                " \n{}\n ",
                                serde_json::to_string_pretty(&rejected).unwrap()
                            );
                            std::fs::write(root.join("halt-lower-late.json"), &raw).unwrap();
                            assert_eq!(
                                send(fd, raw.as_bytes(), MsgFlags::MSG_NOSIGNAL),
                                Ok(raw.len())
                            );
                        }
                        let mut exchange = frame.exchange.clone();
                        if mode.contains("lower-wrong") {
                            exchange.ordinal += 100;
                        }
                        packet(
                            fd,
                            &OrganEnvelope {
                                exchange,
                                position: Position::Close,
                                payload: Payload::Answer(
                                    weaver_types::LifecycleAnswer::GateStopped,
                                ),
                            },
                        );
                        if !mode.contains("lower-wrong") {
                            return;
                        }
                    }
                    Payload::Tool(execution) => {
                        assert!(!mode.contains("no-grant"));
                        assert_eq!(frame.position, Position::Open);
                        ordinal += 1;
                        assert_eq!(
                            frame.exchange,
                            ExchangeId {
                                opener: Opener::Harness,
                                ordinal
                            }
                        );
                        assert_eq!(execution.clock_ms, 1234);
                        assert_eq!(execution.name.0, "sh");
                        assert_eq!(
                            execution.arguments,
                            if ordinal == 2 || mode.contains("identical") {
                                "opaque first command"
                            } else {
                                "opaque second command"
                            }
                        );
                        assert!(pending.is_none());
                        std::fs::write(root.join(format!("external-{ordinal}.json")), &bytes[..n])
                            .unwrap();
                        if mode.contains("gate-death") {
                            return;
                        }
                        let answer = OrganEnvelope {
                            exchange: frame.exchange.clone(),
                            position: Position::Close,
                            payload: Payload::ToolAnswer(expected(mode, ordinal)),
                        };
                        if mode.contains("halt-gate") {
                            if mode.contains("live-tools") {
                                assert!(tool_child.is_none());
                                use std::os::unix::process::CommandExt;
                                let mut command = std::process::Command::new("/usr/bin/sleep");
                                command.arg("60");
                                // SAFETY: only async-signal-safe closes in this
                                // fixture child; it must not inherit the gate
                                // socket and conceal the gate parent's death.
                                unsafe {
                                    command.pre_exec(|| {
                                        nix::unistd::close(3).map_err(std::io::Error::from)?;
                                        nix::unistd::close(4).map_err(std::io::Error::from)?;
                                        Ok(())
                                    });
                                }
                                let child = command.spawn().unwrap();
                                // SAFETY: this actual gate fixture was forked with
                                // its parent's verified tools directory at fd 4.
                                let tools = unsafe { std::os::fd::BorrowedFd::borrow_raw(4) };
                                let procs = nix::fcntl::openat(
                                    tools,
                                    "cgroup.procs",
                                    nix::fcntl::OFlag::O_WRONLY | nix::fcntl::OFlag::O_CLOEXEC,
                                    nix::sys::stat::Mode::empty(),
                                )
                                .unwrap();
                                let pid = child.id().to_string();
                                assert_eq!(
                                    nix::unistd::write(&procs, pid.as_bytes()).unwrap(),
                                    pid.len()
                                );
                                std::fs::write(root.join("halt-tool-pid"), &pid).unwrap();
                                tool_child = Some(child);
                            }
                            pending = Some(OrganEnvelope {
                                payload: Payload::ToolAnswer(ToolOutcome::Killed {
                                    by: KillCause::FlowFailure,
                                    partial: Some("fixture interrupted output".into()),
                                }),
                                ..answer
                            });
                            continue;
                        }
                        if mode.contains("queued-input") && !mode.contains("late") && ordinal == 2 {
                            queued::queue_packets(mode, root, fd);
                        }
                        if mode.contains("stop-running") || mode.contains("queued-input-leave") {
                            pending = Some(answer);
                            continue;
                        }
                        if ((mode.contains("wrong-return") || mode.contains("multiline"))
                            && ordinal == 2)
                            || (mode.contains("duplicate") && ordinal == 3)
                        {
                            let rejected =
                                previous.as_ref().cloned().unwrap_or_else(|| OrganEnvelope {
                                    exchange: ExchangeId {
                                        opener: Opener::Harness,
                                        ordinal: 999,
                                    },
                                    position: Position::Close,
                                    payload: Payload::ToolAnswer(ToolOutcome::Result {
                                        content: "unrelated \"result\" \\\n caf\u{e9}".into(),
                                    }),
                                });
                            let raw = if mode.contains("multiline") {
                                format!(
                                    " \t\r\n{}\n\t ",
                                    serde_json::to_string_pretty(&rejected)
                                        .unwrap()
                                        .replace('\n', "\r\n")
                                        .replace("unrelated", r"\u0075nrelated")
                                )
                            } else {
                                serde_json::to_string(&rejected)
                                    .unwrap()
                                    .replace("{", "{ ")
                                    .replace(":", " : ")
                            };
                            std::fs::write(root.join("rejected.json"), &raw).unwrap();
                            assert_eq!(
                                send(fd, raw.as_bytes(), MsgFlags::MSG_NOSIGNAL),
                                Ok(raw.len())
                            );
                        }
                        packet(fd, &answer);
                        previous = Some(answer);
                    }
                    Payload::ToolInterrupt {
                        by: KillCause::FlowFailure,
                    } if mode.contains("halt-gate") => {
                        assert_eq!(frame.position, Position::Continue);
                        assert_eq!(frame.exchange, pending.as_ref().unwrap().exchange);
                        assert!(!root.join("halt-interrupt.json").exists());
                        std::fs::write(root.join("halt-interrupt.json"), &bytes[..n]).unwrap();
                        if mode.contains("halt-gate-deadline") || mode.contains("halt-gate-clock") {
                            continue;
                        }
                        let answer = pending.take().unwrap();
                        let raw = serde_json::to_vec(&answer).unwrap();
                        std::fs::write(root.join("halt-terminal.json"), &raw).unwrap();
                        packet(fd, &answer);
                        if mode.contains("world-silent") {
                            packet(
                                fd,
                                &OrganEnvelope {
                                    exchange: ExchangeId {
                                        opener: Opener::Gate,
                                        ordinal: 103,
                                    },
                                    position: Position::Open,
                                    payload: Payload::Frame(weaver_types::TurnFrame::carry(
                                        b"queued behind close",
                                    )),
                                },
                            );
                            // Keep the actual receive queue undrained until the
                            // original halt cap closes/kills this fixture.
                            loop {
                                std::thread::sleep(Duration::from_millis(10));
                            }
                        }
                    }
                    Payload::ToolCancel => {
                        assert_eq!(frame.position, Position::Continue);
                        assert_eq!(
                            frame.exchange,
                            ExchangeId {
                                opener: Opener::Harness,
                                ordinal: 2
                            }
                        );
                        cancels += 1;
                        assert_eq!(cancels, 1, "one cancel on the original execution");
                        std::fs::write(root.join("cancel.json"), &bytes[..n]).unwrap();
                        if mode.contains("queued-input") && mode.contains("late") {
                            queued::queue_packets(mode, root, fd);
                        }
                        if let Some(answer) = pending.take() {
                            packet(fd, &answer);
                        }
                    }
                    Payload::Frame(close) => {
                        assert_eq!(frame.position, Position::Close);
                        if mode.contains("queued-input") && frame.exchange.ordinal >= 102 {
                            let value: serde_json::Value =
                                serde_json::from_slice(&close.octets().unwrap()).unwrap();
                            assert_eq!(frame.exchange.opener, Opener::Gate);
                            assert_eq!(value["kind"], "refused");
                            assert!(value.get("turn").is_none() && value.get("run").is_none());
                            std::fs::write(
                                root.join(format!("queued-close-{}.json", frame.exchange.ordinal)),
                                &bytes[..n],
                            )
                            .unwrap();
                            continue;
                        }
                        assert_eq!(
                            frame.exchange,
                            ExchangeId {
                                opener: Opener::Gate,
                                ordinal: 101
                            }
                        );
                        assert!(pending.is_none());
                        let value: serde_json::Value =
                            serde_json::from_slice(&close.octets().unwrap()).unwrap();
                        assert_eq!(
                            value["kind"],
                            if mode.contains("-final-") {
                                "answered"
                            } else {
                                "stopped"
                            }
                        );
                        assert_eq!(value["turn"], "generation-turn");
                        assert!(!root.join("closed.json").exists(), "duplicate world close");
                        std::fs::write(root.join("closed.json"), &bytes[..n]).unwrap();
                        if mode.contains("queued-input") {
                            queued::fresh_packet(mode, root, fd);
                        }
                    }
                    other => panic!("unexpected gate request: {other:?}"),
                }
            }
            Err(nix::errno::Errno::EAGAIN) => {}
            other => panic!("gate peer: {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("external gate watchdog");
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_mut().unwrap();
    input.repair(&FAMILY).unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Effective,
    );
    let requests = input.effective_emission().unwrap().requests().to_vec();
    let mut acts = selection(input.effective_emission().unwrap());
    if mode.contains("reverse") {
        acts.reverse();
    }
    input.sort_outbound(acts.clone()).unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Sorted);
    input.begin_pairs().unwrap();
    let mut ordinal = 1;
    for (index, request) in requests.iter().enumerate() {
        let input = guard.input.as_mut().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairReady,
        );
        assert_eq!(input.ready_pair(), Some(request));
        for _ in 0..4 {
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::PairReady
            );
        }
        if request.lane == PairLane::Recall {
            assert!(input.execute(request.call_id.clone()).is_err());
            input.recall(request.call_id.clone()).unwrap();
            if mode.contains("revoked") {
                input
                    .serving
                    .owner
                    .context
                    .refuse_pair_dispatch(&request.call_id, NonDispatchCause::PlanBreak, clock)
                    .unwrap();
            }
            continue;
        }
        ordinal += 1;
        assert!(!root.join(format!("external-{ordinal}.json")).exists());
        let mut foreign = request.call_id.clone();
        foreign.call.0 += 100;
        assert_eq!(input.execute(foreign.clone()), Err(foreign));
        assert!(input.recall(request.call_id.clone()).is_err());
        assert_eq!(input.ready_pair(), Some(request));
        if mode.contains("stop-unsent") || mode.contains("halt-gate-unsent") {
            loop {
                match send(
                    input.serving.owner.gate.fd().as_raw_fd(),
                    &[b'x'; 4096],
                    MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                ) {
                    Ok(4096) => {}
                    Err(nix::errno::Errno::EAGAIN) => break,
                    other => panic!("fill gate: {other:?}"),
                }
            }
        }
        if mode.contains("actual-worker") {
            let done = std::sync::atomic::AtomicBool::new(false);
            guard.input = Some(std::thread::scope(|scope| {
                let responder = scope.spawn(|| {
                    let end = Instant::now() + Duration::from_secs(8);
                    while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                        member.step_live(mode);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                });
                let input = worker::execute(guard.input.take().unwrap(), request.call_id.clone());
                done.store(true, std::sync::atomic::Ordering::SeqCst);
                responder.join().unwrap();
                input
            }));
            continue;
        }
        input.execute(request.call_id.clone()).unwrap();
        assert!(input.execute(request.call_id.clone()).is_err());
        if mode.contains("halt-gate") {
            return crate::flow::context::worker_startup::input::generation::halt_tests::exercise(
                guard, member, reader, clock, socket, root, mode,
            );
        }
        if index == 0 && mode.contains("queued-input") {
            if mode.contains("duplicate-") || mode.contains("not-open") {
                queued::invalid(input, member, clock, root, mode);
                return;
            }
            if mode.contains("queued-input-leave") {
                return queued::leave(guard, member, clock, socket, root, mode);
            }
            if mode.contains("stop-running") && !mode.contains("late") {
                queued::wait_queued(input, member, clock, mode);
            }
        }
        if mode.contains("gate-death") {
            for _ in 0..1000 {
                if step(input, member, clock, mode) == FoundationInputProgress::Stopped {
                    break;
                }
            }
            assert_eq!(input.progress(), FoundationInputProgress::Stopped);
            assert!(input.serving.owner.context.failure().is_some());
            assert!(input.pair_outcome(&request.call_id).is_none());
            assert!(input.serving.owner.context.pairs.executing().is_some());
            assert!(!root.join("external-3.json").exists());
            assert!(!root.join("closed.json").exists());
            return;
        }
        if ((mode.contains("wrong-return") || mode.contains("multiline")) && ordinal == 2)
            || (mode.contains("duplicate") && ordinal == 3)
        {
            for _ in 0..1000 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
                if input
                    .serving
                    .owner
                    .context
                    .pairs
                    .pending_rejection()
                    .is_some()
                {
                    break;
                }
            }
            let raw = std::fs::read_to_string(root.join("rejected.json")).unwrap();
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .pairs
                    .pending_rejection()
                    .unwrap()
                    .record()
                    .raw_envelope
                    .as_str(),
                raw
            );
            assert!(input.pair_outcome(&request.call_id).is_none());
        }
        if mode.contains("stop-elected") {
            stop(&mut guard, member, clock, mode, socket, root);
            break;
        }
        if mode.contains("stop-unsent") {
            for _ in 0..4 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
            }
            assert!(
                input
                    .serving
                    .owner
                    .context
                    .pairs
                    .executing()
                    .is_some_and(|(_, _, sent)| !sent)
            );
            assert!(!root.join("external-2.json").exists());
            stop(&mut guard, member, clock, mode, socket, root);
            break;
        }
        if mode.contains("stop-running") || mode.contains("stop-crossed") {
            // Drive through the send only. The next Stop may read the answer
            // into the socket owner but must preserve its first actual outcome.
            for _ in 0..1000 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
                if input
                    .serving
                    .owner
                    .context
                    .pairs
                    .executing()
                    .is_some_and(|(_, _, sent)| sent)
                {
                    break;
                }
            }
            assert!(
                input
                    .serving
                    .owner
                    .context
                    .pairs
                    .executing()
                    .is_some_and(|(_, _, sent)| sent)
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
            stop(&mut guard, member, clock, mode, socket, root);
            break;
        }
        if index == 0 && (mode.contains("held-terminal") || mode.contains("stop-terminal")) {
            member.hold_kind = Some("tool.call.completed");
            for _ in 0..1000 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
                if member.held_file.is_some() {
                    break;
                }
            }
            assert!(member.held_file.is_some());
            assert!(input.pair_outcome(&request.call_id).is_none());
            for _ in 0..8 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::Pending
                );
            }
            assert!(!root.join("external-3.json").exists());
            assert!(input.ready_pair().is_none());
            member.hold_kind = None;
            if mode.contains("stop-terminal") {
                stop(&mut guard, member, clock, mode, socket, root);
                break;
            }
        }
    }
    let input = guard.input.as_mut().unwrap();
    if mode.contains("stop-") {
        assert_eq!(input.progress(), FoundationInputProgress::Aborted);
        let sent = mode.contains("stop-running")
            || mode.contains("stop-crossed")
            || mode.contains("stop-terminal");
        for (index, request) in requests.iter().enumerate() {
            let end = input.pair_outcome(&request.call_id).unwrap();
            if index == 0 && sent {
                assert_eq!(end.outcome, Some(expected(mode, 2)));
            } else {
                assert_eq!(
                    end.reason,
                    Some(s::PairFailure::NotDispatched {
                        cause: NonDispatchCause::Stop
                    })
                );
            }
        }
        assert!(!root.join("external-3.json").exists());
        assert_eq!(
            root.join("cancel.json").exists(),
            mode.contains("stop-running") || mode.contains("stop-crossed")
        );
        let rows = trace(reader);
        assert_eq!(
            rows.iter()
                .filter(|r| r["kind"] == "tool.call.completed")
                .count(),
            requests.len()
        );
        let close = rows.iter().find(|r| r["kind"] == "turn.closed").unwrap();
        assert!(
            rows.iter()
                .filter(|r| r["kind"] == "tool.call.completed")
                .all(|r| sequence(r) < sequence(close))
        );
        if mode.contains("queued-input") {
            queued::handoff(&mut guard, member, reader, clock, root, mode);
        }
        return;
    }
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::PairsComplete,
    );
    let mut ordinal = 1;
    for request in &requests {
        let end = input.pair_outcome(&request.call_id).unwrap();
        if request.lane == PairLane::Recall {
            assert!(end.execution.is_none());
            continue;
        }
        ordinal += 1;
        if mode.contains("no-grant") {
            assert_eq!(end.reason, Some(s::PairFailure::NotGranted {}));
            assert!(end.execution.is_none());
            assert!(!root.join(format!("external-{ordinal}.json")).exists());
        } else {
            assert_eq!(
                end.execution,
                Some(ExchangeId {
                    opener: Opener::Harness,
                    ordinal
                })
            );
            assert_eq!(end.outcome, Some(expected(mode, ordinal)));
        }
    }
    assert_eq!(
        input
            .engine
            .as_ref()
            .unwrap()
            .emission
            .as_ref()
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .1,
        acts
    );
    // This reads the whole trace only once, in the shared return instrument.
    // Rejection evidence is also retained by the ledger's completed rejection.
    if mode.contains("wrong-return") || mode.contains("duplicate") || mode.contains("multiline") {
        reader.set_nonblocking(true).unwrap();
        let last = input.serving.owner.context.last_author_sequence.unwrap();
        let rows = trace_through(
            reader,
            Vec::new(),
            last,
            Instant::now() + Duration::from_secs(3),
        )
        .unwrap();
        let rejected: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "tool.return_rejected")
            .collect();
        assert_eq!(rejected.len(), 1);
        assert_eq!(
            rejected[0]["payload"]["reason"],
            if mode.contains("duplicate") {
                "duplicate_return"
            } else {
                "unexpected_exchange"
            }
        );
        // Decoding the outer string must recover every original payload byte.
        let raw = std::fs::read_to_string(root.join("rejected.json")).unwrap();
        assert_eq!(
            rejected[0]["payload"]["raw_envelope"]
                .as_str()
                .unwrap()
                .as_bytes(),
            raw.as_bytes()
        );
        return;
    }
    crate::flow::context::worker_startup::input::generation::return_tests::exercise(
        guard,
        member,
        reader,
        clock,
        socket,
        root,
        mode,
        requests.iter().map(|r| r.call_id.clone()).collect(),
    );
}
