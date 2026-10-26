use super::*;
use crate::flow::clock::{WaitOutcome, testing::FakePumpIo};
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::worker_startup::tests::{Member, reap, trace_through, worker};
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

pub(in crate::flow::context::worker_startup) fn start_pending(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
) {
    use crate::flow::context::startup_composition::tests::CircuitIo;
    use crate::flow::context::worker_startup::input::management::tests::Guard;
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let input = guard.input.as_mut().unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while input.advance_with_io(&mut CircuitIo(clock)) != FoundationInputProgress::Accepted {
        assert!(Instant::now() < end, "input acceptance watchdog");
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    let through = input
        .serving
        .owner
        .context
        .barrier
        .recorder()
        .boundary()
        .admitted
        .unwrap();
    reader.set_nonblocking(true).unwrap();
    let rows = trace_through(reader, vec![], through, end).unwrap();
    super::super::super::management::stop::tests::fill_trace(input.serving.owner.context, clock);
    input.start_turn(TurnKey("generation-turn".into())).unwrap();
    // The input author pumps while waiting for its receipt. Deliver real state
    // EOF from that capacity wait, after the exact TurnStarted is pending.
    let mut fail = StateEofOnWait {
        clock,
        member,
        fired: false,
        preceding_waits: 0,
    };
    assert_eq!(
        input.advance_with_io(&mut fail),
        FoundationInputProgress::Stopped
    );
    assert!(fail.fired);
    assert_eq!(
        input
            .serving
            .owner
            .context
            .pending_author()
            .unwrap()
            .event
            .kind,
        serving::Kind::TurnStarted
    );
    assert!(input.serving.owner.context.open_turn.is_none());
    let mut halt = guard.input.take().unwrap().halt().ok().unwrap();
    while halt.advance_with_io(clock) != FoundationHaltProgress::Prepared {
        assert!(Instant::now() < end, "early halt watchdog");
    }
    assert!(halt.owner.input().serving.owner.context.open_turn.is_none());
    retain(halt.handoff().ok().unwrap(), rows);
}

struct StateEofOnWait<'a> {
    clock: &'a mut FakePumpIo,
    member: &'a Member,
    fired: bool,
    preceding_waits: usize,
}
impl PumpIo for StateEofOnWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        if self.preceding_waits != 0 {
            self.preceding_waits -= 1;
        } else if !self.fired {
            self.member.stop_responses();
            self.fired = true;
        }
        crate::flow::context::startup_composition::tests::CircuitIo(self.clock)
            .wait(interests, deadline)
    }
}

pub(in crate::flow::context::worker_startup) fn pending_close(
    input: &mut FoundationInput<'_>,
    clock: &mut FakePumpIo,
    mode: &str,
) {
    if !mode.contains("pending-turn") && !mode.contains("pending-session") {
        return;
    }
    use crate::flow::recording::{RecordProgress, RecordPurpose};
    let context = &mut input.serving.owner.context;
    let turn_close = mode.contains("pending-turn");
    let event = AuthoredEvent {
        kind: if turn_close {
            serving::Kind::TurnClosed
        } else {
            serving::Kind::SessionClosed
        },
        subsystem: Subsystem::Harness,
        turn: turn_close.then(|| context.open_turn.clone().unwrap()),
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: turn_close.then_some(serving::Payload::TurnClosed(TurnClose::Stopped {
            reason: StopReason::Directive,
            cause: Some(weaver_trace::Cause { uid: 42 }),
        })),
    };
    context.author_turn = Some((event.kind, event.turn.clone()));
    assert!(matches!(
        context.barrier.submit_authored_with_clock(
            event,
            RecordPurpose::Observation,
            &context.author,
            1,
            &mut context.inbox,
            &mut context.budget,
            &mut || clock.now(),
        ),
        RecordProgress::Pending
    ));
}

pub(in crate::flow::context::worker_startup) fn placement_pending(
    mut guard: super::super::super::management::tests::Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_mut().unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("measured.json").exists() {
        assert!(Instant::now() < end, "actual MeasureDelta send watchdog");
        input.advance_with_io(
            &mut crate::flow::context::startup_composition::tests::CircuitIo(clock),
        );
        member.step(clock.now().unwrap(), mode);
        std::thread::sleep(Duration::from_millis(1));
    }
    reader.set_nonblocking(true).unwrap();
    let through = input
        .serving
        .owner
        .context
        .barrier
        .recorder()
        .boundary()
        .admitted
        .unwrap();
    let rows = trace_through(reader, vec![], through, end).unwrap();
    super::super::super::management::stop::tests::fill_trace(input.serving.owner.context, clock);
    std::fs::write(root.join("release-measure"), []).unwrap();
    // Wait for the actual decoder answer before injecting EOF at the author's
    // capacity wait, so the stopped record belongs to measured placement.
    let mut ready = [nix::poll::PollFd::new(
        input.serving.owner.decode.fd(),
        nix::poll::PollFlags::POLLIN,
    )];
    assert_eq!(nix::poll::poll(&mut ready, 1000u16).unwrap(), 1);
    let mut fail = StateEofOnWait {
        clock,
        member,
        fired: false,
        preceding_waits: 1,
    };
    while input.advance_with_io(&mut fail) != FoundationInputProgress::Stopped {
        assert!(Instant::now() < end, "placement author watchdog");
    }
    assert!(fail.fired);
    let context = &input.serving.owner.context;
    assert_eq!(
        context.pending_author().unwrap().event.kind,
        serving::Kind::StatePlacement
    );
    assert!(context.place_owed());
    assert!(context.open_turn.is_some());
    assert!(!root.join("appended.json").exists());
    let mut halt = guard.input.take().unwrap().halt().ok().unwrap();
    while halt.advance_with_io(clock) != FoundationHaltProgress::Prepared {
        assert!(Instant::now() < end, "placement halt watchdog");
    }
    retain(halt.handoff().ok().unwrap(), rows);
}

pub(in crate::flow::context::worker_startup) fn stage_prior_world(
    input: &mut FoundationInput<'_>,
    clock: &mut FakePumpIo,
    mode: &str,
) {
    if !(mode.contains("pending-world") || mode.contains("sent-world")) {
        return;
    }
    let run = weaver_types::RunId(input.serving.owner.context.flow.run().to_owned());
    let text = crate::lifecycle::render_close_with_finish(
        "stopped",
        "reason",
        "the prior close",
        input.turn.as_ref().map(|turn| (turn, &run)),
        false,
    );
    let frame = OrganEnvelope {
        exchange: input.received.as_ref().unwrap().envelope().exchange.clone(),
        position: Position::Close,
        payload: Payload::Frame(weaver_types::TurnFrame::carry(text.as_bytes())),
    };
    input.close = Some(frame.clone());
    let TrackedStage::Queued(send) = input.serving.owner.gate.stage_tracked(frame).unwrap() else {
        panic!()
    };
    input.send = Some(send);
    if mode.contains("sent-world") {
        input.serving.owner.gate.try_send_observed(clock).unwrap();
        assert!(input.send.as_ref().unwrap().sent());
    }
}

thread_local! {
    static HANDOFF: std::cell::RefCell<Option<(Box<FoundationFailedHandoff>, Vec<serde_json::Value>)>> = const { std::cell::RefCell::new(None) };
}
pub(in crate::flow::context::worker_startup) fn retain(
    handoff: Box<FoundationFailedHandoff>,
    rows: Vec<serde_json::Value>,
) {
    HANDOFF.with(|slot| assert!(slot.borrow_mut().replace((handoff, rows)).is_none()));
}

struct Guard(Option<Box<FoundationFailedExit>>);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(owner) = &mut self.0 {
            owner.abort();
            reap(&mut owner.handoff.gate_child);
            owner.tail.reap_composition_test_children();
        }
    }
}

pub(in crate::flow::context) fn release_peer(mode: &str, residency: i32, decode: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let end = Instant::now() + Duration::from_secs(10);
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    while Instant::now() < end {
        match recv(residency, &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return,
            Ok(n) => {
                let mut request: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
                assert!(matches!(
                    request.payload,
                    Payload::Directive(LifecycleDirective::Release)
                ));
                assert_eq!(
                    request.exchange,
                    ExchangeId {
                        opener: Opener::Harness,
                        ordinal: 2
                    }
                );
                assert_eq!(request.position, Position::Open);
                assert_eq!(recv(decode, &mut bytes, MsgFlags::MSG_DONTWAIT), Ok(0));
                assert!(!root.join("halt-release.json").exists());
                std::fs::write(
                    root.join("halt-release.json"),
                    serde_json::to_vec(&request).unwrap(),
                )
                .unwrap();
                if mode.contains("release-silent") {
                    continue;
                }
                if mode.contains("release-wrong") {
                    request.exchange.ordinal += 1;
                }
                request.position = Position::Close;
                request.payload = Payload::Answer(LifecycleAnswer::Released);
                let raw = serde_json::to_vec(&request).unwrap();
                assert_eq!(send(residency, &raw, MsgFlags::MSG_NOSIGNAL), Ok(raw.len()));
                return;
            }
            Err(nix::errno::Errno::EAGAIN) => {}
            other => panic!("serving Release fixture: {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("serving Release fixture watchdog");
}

pub(in crate::flow::context::worker_startup) fn finish(
    mut run: Box<FoundationRun>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    let (handoff, mut rows) =
        HANDOFF.with(|slot| slot.borrow_mut().take().expect("actual halt handoff"));
    let first = run.context.failure().unwrap().clone();
    let deadline = run.context.halt().unwrap().deadline().unwrap();
    let original = handoff
        .input
        .received
        .as_ref()
        .unwrap()
        .raw_bytes()
        .to_vec();
    let fd = run.resources.decode_mut().unwrap().fd().as_raw_fd();
    let residency = run.resources.residency_port().unwrap().fd().as_raw_fd();
    let state_fd = run.context.flow.state().fd().as_raw_fd();
    let gate_fd = handoff.gate.as_ref().unwrap().fd().as_raw_fd();
    assert!(std::fs::read_link(format!("/proc/self/fd/{fd}")).is_ok());
    assert_eq!(handoff.management.is_some(), mode.contains("management"));
    // The checked public join must return both exact owners unchanged on a
    // foreign-run identity, without closing any resource or changing its cap.
    let origin = Arc::clone(&run.context.read_origin);
    run.context.read_origin = Arc::new(());
    let run_address = &*run as *const _;
    let handoff_address = &*handoff as *const _;
    let (mut run, handoff) = run
        .failed_exit(handoff)
        .err()
        .expect("foreign source refused");
    assert_eq!(&*run as *const _, run_address);
    assert_eq!(&*handoff as *const _, handoff_address);
    assert_eq!(run.context.halt().unwrap().deadline().unwrap(), deadline);
    assert!(std::fs::read_link(format!("/proc/self/fd/{fd}")).is_ok());
    run.context.read_origin = origin;
    let mut guard = Guard(Some(if mode.contains("-worker") {
        worker::failed_exit(run, handoff)
            .ok()
            .expect("actual worker failed exit")
    } else {
        run.failed_exit(handoff).ok().expect("same original run")
    }));
    let owner = guard.0.as_mut().unwrap();
    if mode.contains("live-tools") {
        assert!(root.join("halt-tool-pid").exists());
        assert_eq!(
            owner.handoff.tools.as_ref().unwrap().populated(),
            Some(true)
        );
    }
    if mode.contains("world-silent") {
        let gate = owner.handoff.gate.as_ref().unwrap();
        let bytes = [b'x'; 65536];
        loop {
            match send(
                gate.fd().as_raw_fd(),
                &bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(n) => assert_eq!(n, bytes.len()),
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill actual world send: {other:?}"),
            }
        }
    }
    if mode.contains("clock-error") || mode.contains("clock-reverse") {
        let previous = clock.now().unwrap();
        let seen = mode.contains("reverse").then_some(previous - 1);
        assert_eq!(
            owner.advance_with_io(&mut BadClock(seen)),
            FoundationExitProgress::Closed
        );
        assert_eq!(
            owner
                .tail
                .context
                .halt()
                .unwrap()
                .secondary()
                .last()
                .unwrap()
                .observed_ns,
            seen.map(Into::into)
        );
    }
    let end = Instant::now() + Duration::from_secs(8);
    let mut triggered = false;
    let mut trace_released = !mode.contains("pressure-hold");
    if mode.contains("start-pending") {
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExitProgress::Pending
        );
        assert!(owner.stage == ExitStage::Terminals);
        assert!(!owner.terminal_attempted);
        assert!(owner.tail.context.open_turn.is_none());
        assert!(!root.join("closed.json").exists());
        if mode.contains("start-expiry") {
            clock.advance_to(deadline - 1).unwrap();
            owner.wait_with_io(clock);
            assert_eq!(owner.progress(), FoundationExitProgress::Closed);
            assert_eq!(
                owner.tail.context.pending_author().unwrap().event.kind,
                serving::Kind::TurnStarted
            );
            assert!(owner.tail.context.open_turn.is_none());
            triggered = true;
        }
    }
    if mode.contains("placement-pending") {
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExitProgress::Pending
        );
        assert!(owner.stage == ExitStage::Terminals);
        assert!(!owner.terminal_attempted);
        assert_eq!(
            owner.tail.context.pending_author().unwrap().event.kind,
            serving::Kind::StatePlacement
        );
        assert!(owner.tail.context.open_turn.is_some());
        assert!(!root.join("closed.json").exists());
    }
    while owner.progress() != FoundationExitProgress::Closed && Instant::now() < end {
        owner.advance_with_io(clock);
        if mode.contains("world-silent")
            && owner.stage == ExitStage::World
            && owner.world_send.is_some()
            && owner.progress() == FoundationExitProgress::Pending
        {
            // A queued unread packet must not make a send-only wait spin.
            let mut poll = RealSlice {
                now: clock.now().unwrap(),
                elapsed: Duration::ZERO,
            };
            owner.wait_with_io(&mut poll);
            assert!(
                poll.elapsed >= Duration::from_millis(20),
                "unrelated input woke the blocked world send"
            );
            clock.advance_to(deadline - 1).unwrap();
            owner.wait_with_io(clock);
            assert_eq!(
                owner.progress(),
                FoundationExitProgress::Closed,
                "original cap must stop this wait"
            );
            triggered = true;
        }
        if (mode.contains("lower-silent") && root.join("halt-lower.json").exists()
            || mode.contains("release-silent") && root.join("halt-release.json").exists())
            && owner.progress() == FoundationExitProgress::Pending
        {
            // Real non-ready transport with no synthetic readiness. The
            // inherited cap is the only deadline this road may consume.
            clock.advance_to(deadline - 1).unwrap();
            owner.wait_with_io(clock);
            assert_eq!(
                owner.progress(),
                FoundationExitProgress::Closed,
                "original cap must stop this wait"
            );
            triggered = true;
        }
        if !trace_released && owner.stage == ExitStage::Terminals {
            assert!(
                owner.tail.context.pairs.pending_rejection().is_some(),
                "raw return must remain owed while trace capacity is held"
            );
            assert!(!owner.terminal_attempted);
            assert!(std::fs::read_link(format!("/proc/self/fd/{state_fd}")).is_ok());
            assert!(!root.join("closed.json").exists());
            trace_released = true;
        }
        if !mode.contains("trace-loss") && trace_released {
            let through = owner
                .tail
                .context
                .barrier
                .recorder()
                .boundary()
                .admitted
                .unwrap();
            if rows
                .last()
                .and_then(|r| r["sequence"].as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .is_none_or(|s| s < through.0)
            {
                rows.extend(trace_through(reader, vec![], through, end).unwrap());
            }
        }
        if owner.progress() != FoundationExitProgress::Closed {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert_eq!(
        owner.progress(),
        FoundationExitProgress::Closed,
        "failed cleanup did not complete: stage {} release {:?} cause {:?}",
        owner.stage as u8,
        owner.tail.release_outcome(),
        owner.tail.context.halt().unwrap().secondary()
    );
    assert!(matches!(
        owner.channel_fault(),
        Some(crate::ChannelFault::Closed)
    ));
    assert_eq!(owner.tail.context.failure(), Some(&first));
    assert_eq!(
        owner.tail.context.halt().unwrap().deadline().unwrap(),
        deadline
    );
    assert_eq!(
        owner.handoff.input.received.as_ref().unwrap().raw_bytes(),
        original
    );
    assert!(owner.handoff.gate.is_none() && owner.retired_gate.is_some());
    assert!(
        owner
            .tail
            .resources()
            .decode_present_for_serving()
            .is_none()
    );
    assert!(owner.tail.resources().residency_port().is_none());
    for fd in [fd, residency, gate_fd, state_fd] {
        assert!(
            std::fs::read_link(format!("/proc/self/fd/{fd}")).is_err(),
            "actual run fd remains open"
        );
    }
    member.assert_closed_without_request();
    assert!(trace_released);
    assert!(!root.join("external-3.json").exists());
    let expired = mode.contains("deadline")
        || mode.contains("clock-")
        || mode.contains("silent")
        || mode.contains("start-expiry");
    if mode.contains("silent") || mode.contains("start-expiry") {
        assert!(triggered);
    }
    if !expired && !mode.contains("trace-loss") {
        let through = owner
            .tail
            .context
            .barrier
            .recorder()
            .boundary()
            .admitted
            .unwrap();
        if rows
            .last()
            .and_then(|r| r["sequence"].as_str())
            .and_then(|s| s.parse::<u64>().ok())
            != Some(through.0)
        {
            rows.extend(trace_through(reader, vec![], through, end).unwrap());
        }
        let by_kind = |kind: &str| {
            rows.iter()
                .filter(|row| row["kind"] == kind)
                .collect::<Vec<_>>()
        };
        let mut closed_turns = std::collections::HashSet::new();
        let mut session_closed = false;
        for row in &rows {
            let kind = row["kind"].as_str().unwrap();
            assert!(
                !session_closed || kind == "unload",
                "{kind} followed session.closed: {row}"
            );
            if let Some(turn) = row["turn"].as_str() {
                assert!(
                    !closed_turns.contains(turn),
                    "{kind} followed turn.closed({turn}): {row}"
                );
                if kind == "turn.closed" {
                    closed_turns.insert(turn);
                }
            }
            session_closed |= kind == "session.closed";
        }
        assert_eq!(by_kind("fault").len(), 1);
        let closes = by_kind("turn.closed");
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["turn"], "generation-turn");
        assert_eq!(
            closes[0]["payload"]["reason"],
            if mode.contains("pending-turn") {
                "directive"
            } else {
                "fault"
            }
        );
        if mode.contains("pending-turn") {
            assert_eq!(closes[0]["payload"]["cause"]["uid"], 42);
        }
        if mode.contains("placement-pending") {
            let placements: Vec<_> = by_kind("state.placement")
                .into_iter()
                .filter(|row| row["turn"] == "generation-turn")
                .collect();
            assert_eq!(placements.len(), 1);
            assert!(!owner.tail.context.place_owed());
            assert!(!root.join("appended.json").exists());
        }
        assert_eq!(by_kind("session.closed").len(), 1);
        if mode.contains("start-pending") {
            let starts = by_kind("turn.started");
            assert_eq!(starts.len(), 1);
            let sequence =
                |row: &serde_json::Value| row["sequence"].as_str().unwrap().parse::<u64>().unwrap();
            assert!(sequence(starts[0]) < sequence(closes[0]));
            assert!(sequence(closes[0]) < sequence(by_kind("session.closed")[0]));
        }
        assert_eq!(by_kind("unload").len(), 1);
        let unload = by_kind("unload")[0];
        assert_eq!(unload["payload"]["end_reason"], "component_failure");
        assert_eq!(unload["payload"]["grant_surface"], "unreadable");
        assert_eq!(
            unload["payload"]["release"]["spu"],
            if mode.contains("release-wrong") {
                "unconfirmed"
            } else {
                "confirmed"
            }
        );
        assert_eq!(unload["payload"]["release"]["member"], "unconfirmed");
        assert!(owner.unload_sequence().is_some());
        assert_eq!(rows.last().unwrap()["kind"], "unload");
        assert!(root.join("halt-release.json").exists());
        if mode.contains("-eof") || mode.contains("-control") {
            assert!(by_kind("tool.call.completed").is_empty());
            assert!(!root.join("closed.json").exists());
            assert!(owner.contained);
        } else {
            assert_eq!(
                by_kind("tool.call.completed").len(),
                usize::from(!mode.contains("start-pending") && !mode.contains("placement-pending"))
            );
            assert!(root.join("closed.json").exists());
            let wire: OrganEnvelope =
                serde_json::from_slice(&std::fs::read(root.join("closed.json")).unwrap()).unwrap();
            let Payload::Frame(frame) = wire.payload else {
                panic!()
            };
            let answer: serde_json::Value =
                serde_json::from_slice(&frame.octets().unwrap()).unwrap();
            assert_eq!(
                answer["reason"],
                if mode.contains("sent-world") {
                    "the prior close"
                } else {
                    "the service stopped"
                }
            );
            assert_eq!(
                wire.exchange,
                ExchangeId {
                    opener: Opener::Gate,
                    ordinal: 101
                }
            );
            assert!(root.join("halt-lower.json").exists());
        }
        assert_eq!(
            owner.lower_confirmed,
            !(mode.contains("lower-wrong") || mode.contains("-eof") || mode.contains("-control"))
        );
        assert_eq!(
            owner.withdrawn_world.is_some(),
            mode.contains("pending-world")
        );
        if mode.contains("live-tools") {
            assert!(owner.contained);
        }
        if mode.contains("lower-late") {
            let rejects = by_kind("tool.return_rejected");
            assert_eq!(rejects.len(), 3);
            let last = rejects.last().unwrap();
            assert_eq!(
                last["payload"]["raw_envelope"].as_str().unwrap().as_bytes(),
                std::fs::read(root.join("halt-lower-late.json")).unwrap()
            );
            assert!(last.get("turn").is_none());
            assert!(
                rejects[..2]
                    .iter()
                    .all(|row| row["turn"] == "generation-turn")
            );
            assert_eq!(last["payload"]["reason"], "duplicate_return");
            assert!(!last["payload"]["call_id"].is_null());
        }
        if mode.contains("pressure-hold") {
            let rejects = by_kind("tool.return_rejected");
            assert_eq!(rejects.len(), 1);
            assert_eq!(rejects[0]["turn"], "generation-turn");
            assert_eq!(
                rejects[0]["payload"]["raw_envelope"]
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                std::fs::read(root.join("halt-rejected-0.json")).unwrap()
            );
        }
    }
    if expired {
        assert!(owner.abandoned());
        assert!(owner.unload_sequence().is_none());
    }
    // Closed is terminal: it neither waits nor repeats a Release or trace record.
    let boundary = owner.tail.context.barrier.recorder().boundary();
    let waits = clock.waits().len();
    for _ in 0..3 {
        assert_eq!(owner.advance_with_io(clock), FoundationExitProgress::Closed);
        assert_eq!(owner.wait_with_io(clock), FoundationExitProgress::Closed);
    }
    assert_eq!(clock.waits().len(), waits);
    assert_eq!(owner.tail.context.barrier.recorder().boundary(), boundary);
    // A nonblocking cap kill is not a claimed reap. The fixture, which retains
    // the serving thread, reaps any actual remaining child before it ends.
    reap(&mut owner.handoff.gate_child);
    owner.tail.reap_composition_test_children();
    assert_ne!(owner.handoff.gate_child.state(), ChildState::Running);
}

struct BadClock(Option<u64>);
impl PumpIo for BadClock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0
            .ok_or_else(|| std::io::Error::other("failed exit fixture clock"))
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("no wait after failed clock")
    }
}

struct RealSlice {
    now: u64,
    elapsed: Duration,
}
impl PumpIo for RealSlice {
    fn now(&mut self) -> std::io::Result<u64> {
        Ok(self.now)
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        let mut fds = interests
            .iter()
            .map(|i| nix::poll::PollFd::new(i.fd, i.events))
            .collect::<Vec<_>>();
        let start = Instant::now();
        let _ = nix::poll::poll(&mut fds, 40u16).unwrap();
        self.elapsed = start.elapsed();
        Ok(WaitOutcome::Timeout)
    }
}
