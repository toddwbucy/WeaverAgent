//! Actual idle SPU EOF, then actual member EOF, through the worker's failed Lower.
use super::*;
use crate::flow::clock::{ReadyFd, WaitOutcome, testing::FakePumpIo};
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::input::management::tests::Guard;
use crate::flow::context::worker_startup::tests::{Member, trace_through};
use nix::poll::{PollFd, poll};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

type Start = (Box<FoundationFailedHandoff>, Vec<serde_json::Value>, u64);

pub(in crate::flow::context::worker_startup) fn start(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    root: &Path,
) -> Start {
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
    std::fs::write(root.join("lower-observe-spu"), []).unwrap();
    while !input.serving.owner.context.serving_organs.any() {
        assert!(Instant::now() < end, "actual idle SPU EOF was not observed");
        assert_ne!(
            input.advance_with_io(&mut CircuitIo(clock)),
            FoundationInputProgress::Stopped
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let observed_at = clock.now().unwrap();
    let context = &input.serving.owner.context;
    assert!(
        context
            .serving_organs
            .decode_matches(input.serving.owner.decode)
    );
    assert!(context.has_owed_organ_observation());
    assert!(context.failure().is_none() && context.open_turn.is_none());
    assert!(context.pending_author().is_none());
    member.stop_responses();
    while input.advance_with_io(&mut CircuitIo(clock)) != FoundationInputProgress::Stopped {
        assert!(Instant::now() < end, "actual member EOF did not halt");
    }
    let mut halt = guard.input.take().unwrap().halt().ok().unwrap();
    while halt.advance_with_io(clock) != FoundationHaltProgress::Prepared {
        assert!(Instant::now() < end, "organ debt blocked mandatory halt");
    }
    let context = &halt.owner.input().serving.owner.context;
    assert!(context.has_owed_organ_observation());
    assert!(clock.now().unwrap() < observed_at + 200_000_000);
    reader.set_nonblocking(true).unwrap();
    let through = context.barrier.recorder().boundary().admitted.unwrap();
    let rows = trace_through(reader, vec![], through, end).unwrap();
    assert!(
        rows.iter()
            .all(|row| row["payload"]["case"] != "organ_death_observed")
    );
    (halt.handoff().ok().unwrap(), rows, observed_at)
}

struct Owner(Box<FoundationFailedExit>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.abort();
        crate::flow::context::worker_startup::tests::reap(&mut self.0.handoff.gate_child);
        self.0.tail.reap_composition_test_children();
    }
}

/// Actual kernel readiness. Numeric time follows the retained test clock;
/// timeout advances only to the deadline the production wait supplied.
struct Wait<'a> {
    clock: &'a mut FakePumpIo,
    cap: u64,
    capacity: i32,
    settlement: bool,
    seen: bool,
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
        let now = self.clock.now()?;
        let deadline = deadline.expect("original failed-exit cap");
        assert!(deadline <= self.cap && deadline > now);
        if self.settlement {
            assert!(
                deadline <= now + 10_000_000,
                "Lower lost the bounded stderr settlement wake"
            );
        } else {
            assert_eq!(
                deadline, self.cap,
                "settled observation introduced a polling clock"
            );
        }
        assert!(
            interests
                .iter()
                .any(|i| i.fd.as_raw_fd() == self.capacity && i.events.contains(PollFlags::POLLIN)),
            "Lower omitted trace capacity for its owed organ observation"
        );
        let mut fds: Vec<_> = interests
            .iter()
            .map(|i| PollFd::new(i.fd, i.events))
            .collect();
        let began = Instant::now();
        let count = poll(&mut fds, if self.settlement { 5u16 } else { 500u16 })?;
        self.seen = true;
        if self.settlement {
            assert_eq!(
                count, 0,
                "unsettled stderr must block without stale readiness"
            );
            assert!(began.elapsed() >= Duration::from_millis(4));
            self.clock.advance_to(deadline)?;
            return Ok(WaitOutcome::Timeout);
        }
        assert!(count > 0, "released writer capacity did not wake Lower");
        let ready: Vec<_> = fds
            .iter()
            .enumerate()
            .filter_map(|(index, fd)| {
                let events = fd.revents().unwrap_or(PollFlags::empty());
                (!events.is_empty()).then_some(ReadyFd { index, events })
            })
            .collect();
        assert!(
            ready
                .iter()
                .any(|r| interests[r.index].fd.as_raw_fd() == self.capacity)
        );
        Ok(WaitOutcome::Ready(ready))
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    handoff: Box<FoundationFailedHandoff>,
    mut rows: Vec<serde_json::Value>,
    observed_at: u64,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    root: &Path,
    mode: &str,
) {
    let first = run.context.failure().unwrap().clone();
    let cap = run.context.halt().unwrap().deadline().unwrap();
    let mut guard = Owner(
        run.failed_exit(handoff)
            .ok()
            .expect("original failed owner"),
    );
    let owner = &mut guard.0;
    let end = Instant::now() + Duration::from_secs(8);
    while owner.stage != ExitStage::Lower {
        assert!(Instant::now() < end, "mandatory close did not reach Lower");
        owner.advance_with_io(clock);
    }
    assert!(owner.tail.context.has_owed_organ_observation());
    assert!(owner.tail.context.pending_author().is_none());
    super::tests::fill_lower_writer(owner, clock);
    while !root.join("halt-lower.json").exists() {
        assert!(
            Instant::now() < end,
            "owed observation delayed actual Lower"
        );
        assert_ne!(owner.advance_with_io(clock), FoundationExitProgress::Closed);
        assert!(owner.stage == ExitStage::Lower);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(owner.lower_send.as_ref().unwrap().sent());
    if !mode.ends_with("-return") {
        assert!(owner.tail.context.pending_author().is_none());
    }
    let rejection_ticket = if mode.ends_with("-return") {
        while owner.tail.context.pairs.pending_rejection().is_none() {
            assert!(Instant::now() < end, "actual late return was not retained");
            owner.advance_with_io(clock);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            owner
                .tail
                .context
                .pairs
                .pending_rejection()
                .unwrap()
                .raw_bytes(),
            std::fs::read(root.join("lower-organ-return.json")).unwrap()
        );
        Some(owner.tail.context.pending_author().unwrap().ticket)
    } else {
        None
    };
    // The receive step can yield Progress after retaining a return. Obtain the
    // production Pending wait before asking its transport to block.
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExitProgress::Pending
    );
    // No trace read and no gate response can wake this stage. Stderr is still
    // held by the actual child waiting for Release, after Lower completes.
    let mut waited = 0;
    while clock.now().unwrap() < observed_at + 200_000_000 {
        let mut io = Wait {
            clock,
            cap,
            capacity: owner.tail.context.barrier.capacity_fd().as_raw_fd(),
            settlement: true,
            seen: false,
        };
        owner.wait_with_io(&mut io);
        assert!(io.seen);
        waited += 1;
        owner.advance_with_io(clock);
    }
    assert!(waited > 0);
    let held = owner
        .tail
        .context
        .pending_author()
        .expect("Lower must admit its settled organ fault");
    let ticket = held.ticket;
    assert_eq!(
        held.event.kind,
        if rejection_ticket.is_some() {
            serving::Kind::ToolReturnRejected
        } else {
            serving::Kind::Fault
        }
    );
    if let Some(earlier) = rejection_ticket {
        assert_eq!(ticket, earlier);
    }
    assert!(held.event.turn.is_none());
    let event = held.event.clone();
    for _ in 0..3 {
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExitProgress::Pending
        );
        assert_eq!(owner.tail.context.pending_author().unwrap().ticket, ticket);
    }
    if mode.ends_with("-cap") {
        clock.advance_to(cap - 1).unwrap();
        assert_eq!(owner.wait_with_io(clock), FoundationExitProgress::Closed);
        assert!(owner.abandoned() && owner.unload_sequence().is_none());
        assert_eq!(owner.tail.context.failure(), Some(&first));
        assert_eq!(owner.tail.context.halt().unwrap().deadline().unwrap(), cap);
        assert!(!owner.tail.context.has_owed_organ_observation());
        assert!(owner.handoff.gate.is_none());
        // The original child stays in this closed owner for actual reap; the
        // exhausted halt cap grants no blocking waitpid or successful Unload.
        return;
    }
    let through = owner
        .tail
        .context
        .barrier
        .recorder()
        .boundary()
        .admitted
        .unwrap();
    rows.extend(trace_through(reader, vec![], through, end).unwrap());
    let mut io = Wait {
        clock,
        cap,
        capacity: owner.tail.context.barrier.capacity_fd().as_raw_fd(),
        settlement: false,
        seen: false,
    };
    owner.wait_with_io(&mut io);
    assert!(io.seen);
    while owner.tail.context.has_owed_organ_observation()
        || owner.tail.context.pairs.pending_rejection().is_some()
    {
        assert!(
            Instant::now() < end,
            "Lower did not record its retained observation"
        );
        owner.advance_with_io(clock);
    }
    assert!(owner.stage == ExitStage::Lower);
    let through = owner
        .tail
        .context
        .barrier
        .recorder()
        .boundary()
        .admitted
        .unwrap();
    rows.extend(trace_through(reader, vec![], through, end).unwrap());
    let faults: Vec<_> = rows
        .iter()
        .filter(|r| r["payload"]["case"] == "organ_death_observed")
        .collect();
    assert_eq!(faults.len(), 1);
    if let Some(serving::Payload::Fault(original)) = event.payload {
        assert_eq!(
            faults[0]["payload"],
            serde_json::from_str::<serde_json::Value>(original.get()).unwrap()
        );
    } else {
        let rejects: Vec<_> = rows
            .iter()
            .filter(|r| r["kind"] == "tool.return_rejected")
            .collect();
        assert_eq!(rejects.len(), 1);
        assert_eq!(
            rejects[0]["payload"]["raw_envelope"]
                .as_str()
                .unwrap()
                .as_bytes(),
            std::fs::read(root.join("lower-organ-return.json")).unwrap()
        );
        assert!(rejects[0].get("turn").is_none());
        let sequence =
            |row: &serde_json::Value| row["sequence"].as_str().unwrap().parse::<u64>().unwrap();
        assert!(sequence(rejects[0]) < sequence(faults[0]));
    }
    assert!(faults[0].get("turn").is_none());
    assert!(
        faults[0]["payload"]
            .to_string()
            .contains("idle SPU lost decode before failed Lower")
    );
    std::fs::write(root.join("lower-organ-recorded"), []).unwrap();
    while owner.progress() != FoundationExitProgress::Closed {
        assert!(Instant::now() < end, "failed Lower tail stalled");
        owner.advance_with_io(clock);
        let through = owner
            .tail
            .context
            .barrier
            .recorder()
            .boundary()
            .admitted
            .unwrap();
        if rows.last().unwrap()["sequence"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            < through.0
        {
            rows.extend(trace_through(reader, vec![], through, end).unwrap());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(owner.lower_confirmed && !owner.abandoned());
    assert!(owner.unload_sequence().is_some());
    assert_eq!(owner.tail.context.failure(), Some(&first));
    assert_eq!(owner.tail.context.halt().unwrap().deadline().unwrap(), cap);
    assert_eq!(rows.last().unwrap()["kind"], "unload");
    assert_eq!(
        rows.iter()
            .filter(|r| r["payload"]["case"] == "organ_death_observed")
            .count(),
        1
    );
    assert!(matches!(
        owner.handoff.gate_child.state(),
        crate::flow::context::failed_enter_driver::ChildState::Reaped(_)
    ));
    assert!(owner.tail.resources().spu_child().is_some_and(|s| matches!(
        s,
        crate::flow::context::failed_enter_driver::ChildState::Reaped(_)
    )));
    member.assert_closed_without_request();
}
