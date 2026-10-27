//! A real closed gate, containment, pair filing, result delivery and elected
//! turn close. The decoder/member are scripted; no model is qualified here.
use super::*;
use weaver_traits::{ContentBlock, Message, Role};

#[path = "context_worker_gate_loss_stop_tests.rs"]
pub(super) mod stop;

#[expect(
    clippy::zombie_processes,
    reason = "The gate must exit with a live contained command; the worker owns containment and the fixture verifies tools are empty."
)]
pub(super) fn end(mode: &str, root: &Path, fd: i32, request: &OrganEnvelope, bound: Instant) {
    if mode.contains("gate-death-stop") {
        while !root.join("gate-loss-stop-accepted").exists() {
            assert!(Instant::now() < bound);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    if mode.contains("-report") {
        if !mode.ends_with("unread") {
            while !root.join("gate-loss-report-send").exists() {
                assert!(Instant::now() < bound);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let report = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 102,
            },
            position: Position::Close,
            payload: Payload::Fault(weaver_types::FaultReport {
                case: weaver_types::FaultCase::ListenerLost,
                account: serde_json::value::RawValue::from_string(
                    "{ \"listener\": \"lost before EOF\", \"original\": [1,  2] }".into(),
                )
                .unwrap(),
            }),
        };
        std::fs::write(
            root.join("gate-loss-report.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
        packet(fd, &report);
        if !mode.ends_with("unread") {
            while !root.join("gate-loss-report-held").exists() {
                assert!(Instant::now() < bound);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    if mode.ends_with("pressure") {
        while !root.join("gate-loss-close").exists() {
            assert!(Instant::now() < bound);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    if mode.contains("crossed") {
        let answer = OrganEnvelope {
            exchange: request.exchange.clone(),
            position: Position::Close,
            payload: Payload::ToolAnswer(expected(mode, request.exchange.ordinal)),
        };
        std::fs::write(
            root.join("gate-loss-return.json"),
            serde_json::to_vec(&answer).unwrap(),
        )
        .unwrap();
        packet(fd, &answer);
        if !mode.ends_with("unread") {
            while !root.join("gate-loss-close").exists() {
                assert!(Instant::now() < bound);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    } else {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("/usr/bin/sleep");
        command.arg("60");
        // SAFETY: these async-signal-safe closes keep the child from retaining
        // the gate socket or the tools directory passed to its parent.
        unsafe {
            command.pre_exec(|| {
                nix::unistd::close(3).map_err(std::io::Error::from)?;
                nix::unistd::close(4).map_err(std::io::Error::from)?;
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        // SAFETY: the gate fixture inherited the verified tools directory at 4.
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
        std::fs::write(root.join("gate-loss-tool-pid"), pid).unwrap();
        // The actual gate exits without answering or containing this process.
        // The harness must kill the verified tools cgroup before settling fate.
    }
}

pub(super) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
    mut requests: Vec<s::PairStarted>,
) {
    let input = guard.input.as_mut().unwrap();
    if mode.contains("-report") && !mode.ends_with("unread") {
        let end = Instant::now() + Duration::from_secs(5);
        while !root.join("external-2.json").exists() {
            assert!(Instant::now() < end, "actual call was not received");
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::Pending
            );
        }
        if mode.ends_with("pressure") {
            receive_report(input, root);
        } else {
            hold_report(input, root);
        }
    }
    if mode.ends_with("pressure") {
        return pressure(input, member, clock, root, mode);
    }
    if mode.ends_with("unsent") {
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
        std::fs::write(root.join("gate-loss-close"), b"close unsent gate").unwrap();
    } else if mode.ends_with("crossed") {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < end, "original answer was not received");
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::Pending
            );
            if input.serving.owner.context.pairs.next_terminal().is_some() {
                break;
            }
        }
        std::fs::write(root.join("gate-loss-close"), b"close after original result").unwrap();
        // Hold the elected pair continuation until the actual EOF is observed.
        // Otherwise the fixture can race the next PairReady before its child exits.
        let end = Instant::now() + Duration::from_secs(5);
        while !input
            .serving
            .owner
            .context
            .serving_organs
            .gate_matches(&input.serving.owner.gate)
        {
            assert!(Instant::now() < end, "crossed result gate did not close");
            input.drive_with_io(
                &mut crate::flow::context::startup_composition::tests::CircuitIo(clock),
                true,
            );
            member.step(clock.now().unwrap(), mode);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::PairsComplete,
    );
    let context = &input.serving.owner.context;
    assert!(context.failure().is_none());
    assert!(
        context
            .serving_organs
            .gate_matches(&input.serving.owner.gate)
    );
    assert_eq!(input.serving.owner.tools.populated(), Some(false));
    assert_eq!(
        root.join("gate-loss-tool-pid").exists(),
        !mode.ends_with("crossed") && !mode.ends_with("unsent")
    );
    assert!(!root.join("external-3.json").exists());
    assert!(!root.join("cancel.json").exists());
    for (index, request) in requests.iter().enumerate() {
        let end = input.pair_outcome(&request.call_id).unwrap();
        assert_eq!(end.call_id, request.call_id);
        if index == 0 && !mode.ends_with("unsent") {
            assert_eq!(
                end.execution,
                Some(ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 2
                })
            );
            if mode.ends_with("crossed") {
                assert_eq!(end.outcome, Some(expected(mode, 2)));
                assert!(end.reason.is_none());
            } else {
                assert_eq!(end.reason, Some(s::PairFailure::GateFault {}));
                assert_eq!(
                    end.outcome,
                    Some(ToolOutcome::Killed {
                        by: KillCause::Fault,
                        partial: None
                    })
                );
            }
        } else {
            assert!(end.execution.is_none());
            assert_eq!(
                end.reason,
                Some(s::PairFailure::NotDispatched {
                    cause: NonDispatchCause::NoGate
                })
            );
            assert!(end.outcome.is_none());
        }
    }
    let later = deliver_round(input, member, clock, root, mode, &requests);
    if mode.ends_with("repeat") {
        assert_eq!(later.len(), 2, "the model actually requested another batch");
        for request in &later {
            let end = input.pair_outcome(&request.call_id).unwrap();
            assert!(end.execution.is_none());
            assert_eq!(
                end.reason,
                Some(s::PairFailure::NotDispatched {
                    cause: NonDispatchCause::NoGate
                })
            );
            assert!(end.outcome.is_none());
        }
        assert!(deliver_round(input, member, clock, root, mode, &later).is_empty());
        requests.extend(later);
    } else {
        assert!(later.is_empty());
    }
    assert!(!root.join("external-3.json").exists());
    if mode.ends_with("fit-refusal") {
        assert_eq!(input.progress(), FoundationInputProgress::Refused);
        assert!(input.serving.owner.context.place_close().is_none());
        assert!(input.close.is_some() && input.send.is_none());
        assert!(!root.join("return-appended.json").exists());
    } else {
        assert!(input.serving.owner.context.open_turn.is_some());
        assert!(!root.join("closed.json").exists());
        input.close_answer().unwrap();
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::Completed,
        );
        assert!(
            input
                .engine
                .as_ref()
                .unwrap()
                .answer
                .as_ref()
                .unwrap()
                .send()
                .is_none()
        );
    }
    assert!(!input.failed_world_was_sent());
    assert!(!root.join("closed.json").exists());
    assert!(input.serving.owner.context.open_turn.is_none());
    assert!(input.serving.owner.context.failure().is_none());
    let rows = records(
        reader,
        input.serving.owner.context.last_author_sequence.unwrap(),
        root,
        mode,
    );
    let faults: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "fault" && row["payload"]["case"] == "organ_death_observed")
        .collect();
    assert_eq!(faults.len(), 1);
    assert!(
        faults[0]["payload"]
            .to_string()
            .contains("tools/ killed and emptied")
    );
    let closed: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["turn"], "generation-turn");
    assert_eq!(
        closed[0]["payload"]["close"],
        if mode.ends_with("fit-refusal") {
            "stopped"
        } else {
            "clean"
        }
    );
    if mode.ends_with("fit-refusal") {
        assert_eq!(closed[0]["payload"]["reason"], "refused");
    }
    let starts: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "tool.call.started")
        .collect();
    let ends: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "tool.call.completed")
        .collect();
    assert_eq!(starts.len(), requests.len());
    assert_eq!(ends.len(), requests.len());
    for ((start, end), request) in starts.iter().zip(ends).zip(&requests) {
        assert_eq!(start["payload"], serde_json::to_value(request).unwrap());
        assert_eq!(
            end["payload"]["call_id"],
            serde_json::to_value(&request.call_id).unwrap()
        );
        assert!(sequence(start) < sequence(end));
        assert!(sequence(end) < sequence(closed[0]));
    }
}

fn hold_report(input: &mut FoundationInput<'_>, root: &Path) {
    receive_report(input, root);
    let end = Instant::now() + Duration::from_secs(5);
    while input.serving.owner.gate_child.poll()
        == crate::flow::context::failed_enter_driver::ChildState::Running
    {
        assert!(Instant::now() < end, "actual reporting gate did not exit");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn receive_report(input: &mut FoundationInput<'_>, root: &Path) {
    std::fs::write(root.join("gate-loss-report-send"), b"send actual report").unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let gate = &mut input.serving.owner.gate;
    while gate.held().is_none() {
        assert!(Instant::now() < end, "actual fault packet was not received");
        gate.try_receive().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(gate.held().unwrap().payload, Payload::Fault(_)));
    assert_eq!(
        gate.held().unwrap(),
        &serde_json::from_slice::<OrganEnvelope>(
            &std::fs::read(root.join("gate-loss-report.json")).unwrap(),
        )
        .unwrap(),
    );
    std::fs::write(
        root.join("gate-loss-report-held"),
        b"EOF beside retained report",
    )
    .unwrap();
}

fn records(
    reader: &mut UnixStream,
    through: weaver_trace::Sequence,
    root: &Path,
    mode: &str,
) -> Vec<serde_json::Value> {
    if !mode.contains("-report") {
        return trace(reader);
    }
    struct RetainedRead<'a> {
        peer: &'a mut UnixStream,
        bytes: Vec<u8>,
    }
    impl std::io::Read for RetainedRead<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            let read = std::io::Read::read(self.peer, bytes)?;
            self.bytes.extend_from_slice(&bytes[..read]);
            Ok(read)
        }
    }
    reader.set_nonblocking(true).unwrap();
    let mut retained = RetainedRead {
        peer: reader,
        bytes: vec![],
    };
    let rows = trace_through(
        &mut retained,
        vec![],
        through,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let original: OrganEnvelope =
        serde_json::from_slice(&std::fs::read(root.join("gate-loss-report.json")).unwrap())
            .unwrap();
    let Payload::Fault(expected) = original.payload else {
        panic!("original gate fault")
    };
    let mut found = 0;
    for raw in retained
        .bytes
        .split(|byte| *byte == b'\n')
        .filter(|raw| !raw.is_empty())
    {
        let row: serde_json::Value = serde_json::from_slice(raw).unwrap();
        if row["kind"] == "fault" && row["payload"]["case"] == "listener_lost" {
            let fields: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
                serde_json::from_slice(raw).unwrap();
            let actual: weaver_types::FaultReport =
                serde_json::from_str(fields["payload"].get()).unwrap();
            assert_eq!(actual, expected, "retain the opaque account bytes");
            assert_eq!(row["subsystem"], "gate");
            found += 1;
        }
    }
    assert_eq!(found, 1, "the original gate report is recorded once");
    assert_eq!(rows.iter().filter(|row| row["kind"] == "fault").count(), 2);
    let close = rows
        .iter()
        .find(|row| row["kind"] == "turn.closed")
        .unwrap();
    assert!(
        rows.iter()
            .filter(|row| row["kind"] == "fault")
            .all(|row| sequence(row) < sequence(close))
    );
    rows
}

fn deliver_round(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
    requests: &[s::PairStarted],
) -> Vec<s::PairStarted> {
    let context = &input.serving.owner.context;
    let calls: Vec<_> = requests
        .iter()
        .map(|request| request.call_id.clone())
        .collect();
    let expected: Vec<_> = calls
        .iter()
        .map(|call| Message {
            role: Role::ToolResult,
            content: vec![ContentBlock::ToolResult(
                context.pairs.take_result(call).unwrap().result().block(),
            )],
        })
        .collect();
    input
        .place_results(calls, "results".into(), CutMethod::Flush)
        .unwrap();
    if mode.ends_with("fit-refusal") {
        until(input, member, clock, mode, FoundationInputProgress::Refused);
        return Vec::new();
    }
    until(input, member, clock, mode, FoundationInputProgress::Placed);
    input.generate().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Generated,
    );
    let request: weaver_types::TokenDirective =
        serde_json::from_slice(&std::fs::read(root.join("return-appended.json")).unwrap()).unwrap();
    let weaver_types::TokenDirective::AppendAndGenerate { delta, .. } = request else {
        panic!("actual result delivery")
    };
    assert_eq!(delta, expected);
    input.repair(&FAMILY).unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Effective,
    );
    let next = input.effective_emission().unwrap().requests().to_vec();
    input
        .sort_outbound(selection(input.effective_emission().unwrap()))
        .unwrap();
    until(input, member, clock, mode, FoundationInputProgress::Sorted);
    input.begin_pairs().unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::PairsComplete,
    );
    next
}

fn pressure(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    use crate::flow::clock::{WaitInterest, WaitOutcome};
    use crate::flow::context::worker_startup::input::management::stop::tests::fill_trace;
    use crate::flow::pump::PumpDeadline;
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("external-2.json").exists() {
        assert!(Instant::now() < end);
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
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    fill_trace(input.serving.owner.context, clock);
    std::fs::write(root.join("gate-loss-close"), b"close under trace pressure").unwrap();
    struct Expire<'a> {
        clock: &'a mut FakePumpIo,
        capacity: i32,
        gate: i32,
        bound: Option<u64>,
    }
    impl PumpIo for Expire<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            self.clock.now()
        }
        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            deadline: Option<u64>,
        ) -> std::io::Result<WaitOutcome> {
            if interests
                .iter()
                .any(|interest| interest.fd.as_raw_fd() == self.gate)
            {
                // Before observed EOF, the actual sent execution is waiting on
                // the gate's clock and has no harness Decision timer.
                return crate::flow::context::startup_composition::tests::CircuitIo(self.clock)
                    .wait(interests, deadline);
            }
            let deadline = deadline.expect("gate fault filing lost its Decision bound");
            if interests.is_empty() {
                // Let the actual cgroup/child stderr settle before selecting
                // the writer wait this instrument expires. Expiring an empty
                // settlement wake would never exercise trace backpressure.
                std::thread::sleep(Duration::from_millis(1));
                return crate::flow::context::startup_composition::tests::CircuitIo(self.clock)
                    .wait(interests, Some(deadline));
            }
            assert!(
                interests
                    .iter()
                    .any(|interest| interest.fd.as_raw_fd() == self.capacity)
            );
            let mut fds: Vec<_> = interests
                .iter()
                .map(|interest| nix::poll::PollFd::new(interest.fd, interest.events))
                .collect();
            assert_eq!(
                nix::poll::poll(&mut fds, 5u16)?,
                0,
                "full-writer wait must have no readiness"
            );
            assert!(self.bound.replace(deadline).is_none());
            self.clock.advance_to(deadline)?;
            Ok(WaitOutcome::Timeout)
        }
    }
    let mut io = Expire {
        clock,
        gate: input.serving.owner.gate.fd().as_raw_fd(),
        capacity: input
            .serving
            .owner
            .context
            .barrier
            .capacity_fd()
            .as_raw_fd(),
        bound: None,
    };
    let end = Instant::now() + Duration::from_secs(5);
    while input.progress() != FoundationInputProgress::Stopped {
        assert!(
            Instant::now() < end,
            "blocked gate-loss accounting did not stop"
        );
        input.advance_with_io(&mut io);
        member.step(io.now().unwrap(), mode);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        io.bound.is_some(),
        "the actual full writer must block: input={:?}, context={:?}",
        input.detail(),
        input.serving.owner.context.failure()
    );
    assert!(matches!(
        input.serving.owner.context.failure(),
        Some(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert_eq!(input.serving.owner.tools.populated(), Some(false));
    if mode.contains("-report") {
        assert!(
            input.report.is_some(),
            "the owed original remains owned at halt"
        );
        assert!(
            !input.serving.owner.gate.closed_receive_pending(),
            "the nested author observed actual EOF after retaining this report"
        );
    }
    let (_, terminal) = input.serving.owner.context.pairs.halted_terminal().unwrap();
    assert_eq!(terminal.reason, Some(s::PairFailure::GateFault {}));
    assert!(!root.join("external-3.json").exists());
    assert!(!root.join("closed.json").exists());
}
