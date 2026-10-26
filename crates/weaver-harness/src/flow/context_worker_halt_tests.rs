//! Failed worker ownership through the actual gate socket and state EOF.
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::input::halt::{FoundationHalt, FoundationHaltProgress};
use crate::flow::context::worker_startup::input::management::FoundationManagementProgress;
use crate::flow::context::worker_startup::input::management::stop::tests::fill_trace;
use crate::flow::context::worker_startup::input::management::tests::{
    Guard, halt_facts, hold_for_halt,
};
use crate::flow::context::worker_startup::tests::Member;
use crate::flow::context::worker_startup::tests::{trace_through, worker};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use std::path::Path;
use std::time::{Duration, Instant};

#[allow(clippy::too_many_arguments)]
pub(super) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.take().unwrap();
    let address = &*input as *const _;
    guard.input = Some(input.halt().err().expect("healthy input cannot enter halt"));
    assert_eq!(&**guard.input.as_ref().unwrap() as *const _, address);
    let input = guard.input.as_mut().unwrap();
    for _ in 0..1000 {
        input.advance_with_io(&mut CircuitIo(clock));
        member.step(clock.now().unwrap(), mode);
        if if mode.contains("-unsent") {
            input
                .serving
                .owner
                .context
                .pairs
                .executing()
                .is_some_and(|(_, _, sent)| !sent)
        } else {
            root.join("external-2.json").exists()
        } {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if mode.contains("-unsent") {
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
    } else {
        assert!(root.join("external-2.json").exists());
    }
    reader.set_nonblocking(true).unwrap();
    let mut records = drain(reader, input.serving.owner.context);
    if mode.contains("pressure") {
        assert!(fill_trace(input.serving.owner.context, clock) > 0);
    }
    if mode.contains("halt-gate-worker") {
        clock.advance_to(MonotonicIo.now().unwrap()).unwrap();
    }
    let peer = mode
        .contains("-management")
        .then(|| hold_for_halt(&mut guard, member, clock, socket, mode));
    let management = guard.management.as_deref().map(halt_facts);
    member.stop_responses();
    for _ in 0..1000 {
        let stopped = if let Some(input) = &mut guard.input {
            input.advance_with_io(&mut CircuitIo(clock)) == FoundationInputProgress::Stopped
        } else {
            guard
                .management
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
                == FoundationManagementProgress::Stopped
        };
        if stopped {
            break;
        }
    }
    let input = guard.owned_input();
    assert_eq!(input.progress(), FoundationInputProgress::Stopped);
    let owner = &input.serving.owner;
    assert!(matches!(
        owner.context.failure(),
        Some(RunFailure::Flow {
            reason: FlowHaltReason::CarryFailed,
            ..
        })
    ));
    let first = owner.context.failure().unwrap().clone();
    let deadline = owner.context.halt().unwrap().deadline().unwrap();
    let original = input.received.as_ref().unwrap().raw_bytes().to_vec();
    if mode.contains("trace-loss") {
        reader.shutdown(std::net::Shutdown::Both).unwrap();
    }
    std::fs::write(root.join("halt-failed"), b"failed").unwrap();
    let mut halt = if mode.contains("halt-gate-worker") {
        HaltGuard(Some(
            worker::halt(Ok(guard.input.take().unwrap())).ok().unwrap(),
        ))
    } else {
        HaltGuard(Some(if let Some(input) = guard.input.take() {
            input.halt().ok().unwrap()
        } else {
            guard.management.take().unwrap().halt().ok().unwrap()
        }))
    };
    let mut pressure_released = false;
    let mut expired = false;
    for _ in 0..1000 {
        let owned = halt.0.as_mut().unwrap();
        let progress = owned.advance_with_io(clock);
        if matches!(
            progress,
            FoundationHaltProgress::Prepared | FoundationHaltProgress::NeedsContainment
        ) {
            break;
        }
        assert!(matches!(
            progress,
            FoundationHaltProgress::Pending | FoundationHaltProgress::Progress
        ));
        let input = owned.test_input();
        let blocked_return = input
            .serving
            .owner
            .context
            .pairs
            .pending_rejection()
            .is_some()
            && input
                .serving
                .owner
                .gate
                .held()
                .is_some_and(|frame| matches!(frame.payload, Payload::ToolAnswer(_)));
        let silent = mode.contains("-deadline") && root.join("halt-interrupt.json").exists();
        if mode.contains("halt-gate-clock")
            && root.join("halt-interrupt.json").exists()
            && progress == FoundationHaltProgress::Pending
        {
            let previous = clock.now().unwrap();
            let observed = mode.contains("-reverse").then_some(previous - 1);
            assert_eq!(
                owned.wait_with_io(&mut BrokenClock(observed)),
                FoundationHaltProgress::Expired
            );
            let context = &owned.test_input().serving.owner.context;
            assert_eq!(
                context
                    .halt()
                    .unwrap()
                    .secondary()
                    .last()
                    .unwrap()
                    .observed_ns,
                observed.map(Into::into)
            );
            expired = true;
            break;
        }
        if (silent || (mode.contains("trace-loss-late") && blocked_return))
            && progress == FoundationHaltProgress::Pending
        {
            let fd = owned.test_input().serving.owner.gate.fd().as_raw_fd();
            assert_eq!(owned.wait_with_io(clock), FoundationHaltProgress::Expired);
            let wait = clock.waits().last().unwrap();
            assert_eq!(wait.absolute_deadline, Some(deadline));
            if blocked_return {
                assert!(
                    wait.descriptors.iter().all(|(seen, flags)| *seen != fd
                        || !flags.contains(nix::poll::PollFlags::POLLIN))
                );
            }
            expired = true;
            break;
        }
        if mode.contains("pressure-late") && blocked_return && !pressure_released {
            assert!(
                root.join("halt-interrupt.json").exists(),
                "interrupt precedes ordinary capacity"
            );
            assert!(
                owned
                    .test_input()
                    .serving
                    .owner
                    .context
                    .barrier
                    .pending()
                    .is_some()
            );
            records.extend(drain(reader, owned.test_input().serving.owner.context));
            pressure_released = true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let owned = halt.0.as_mut().unwrap();
    let unavailable = mode.contains("-eof") || mode.contains("-control");
    assert_eq!(
        owned.progress(),
        if expired {
            FoundationHaltProgress::Expired
        } else if unavailable {
            FoundationHaltProgress::NeedsContainment
        } else {
            FoundationHaltProgress::Prepared
        },
        "actual failed tool terminal must not remain behind a world frame"
    );
    if !mode.contains("-unsent") && !mode.contains("-eof") {
        assert_eq!(owned.test_received().len(), 1);
        assert_eq!(
            owned.test_received()[0].raw_bytes(),
            std::fs::read(root.join("halt-queued.json")).unwrap()
        );
    }
    let input = owned.test_input();
    assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
    let context = &input.serving.owner.context;
    assert_eq!(context.failure(), Some(&first));
    assert_eq!(
        context.halt_report().unwrap().last_trace_sequence(),
        context.barrier.last_trace_only_sequence(),
        "the report retains the latest actual stopped-tap admission"
    );
    assert_eq!(context.halt().unwrap().deadline().unwrap(), deadline);
    if mode.contains("-unsent") {
        assert!(context.halt_gate().unwrap().withdrawn().is_some());
        assert!(!context.halt_gate().unwrap().interrupt_sent());
        assert!(!root.join("external-2.json").exists());
    } else if !unavailable {
        assert!(root.join("halt-interrupt.json").exists());
        if !expired {
            assert_eq!(
                context.halt_gate_terminal().unwrap().1,
                std::fs::read(root.join("halt-terminal.json")).unwrap()
            );
        }
    }
    if expired || unavailable {
        assert!(context.halt_gate_terminal().is_none());
    }
    if mode.contains("pressure-hold") || mode.contains("trace-loss-late") {
        assert_eq!(
            context.pairs.pending_rejection().unwrap().raw_bytes(),
            std::fs::read(root.join("halt-rejected-0.json")).unwrap()
        );
    }
    assert!(!root.join("external-3.json").exists());
    assert!(!root.join("closed.json").exists());
    member.assert_no_request();
    if !mode.contains("trace-loss") {
        let through = context.barrier.recorder().boundary().admitted.unwrap();
        if records
            .last()
            .and_then(|row| row["sequence"].as_str())
            .and_then(|n| n.parse::<u64>().ok())
            .is_none_or(|last| last < through.0)
        {
            records.extend(drain(reader, context));
        }
        let rejects: Vec<_> = records
            .iter()
            .filter(|row| row["kind"] == "tool.return_rejected")
            .collect();
        if mode.contains("-late") && !mode.contains("pressure-hold") {
            assert_eq!(rejects.len(), 2);
            for (index, row) in rejects.iter().enumerate() {
                assert_eq!(
                    row["payload"]["raw_envelope"].as_str().unwrap().as_bytes(),
                    std::fs::read(root.join(format!("halt-rejected-{index}.json"))).unwrap()
                );
                assert!(row["payload"]["call_id"].is_null());
            }
        } else {
            assert!(rejects.is_empty());
        }
    }
    if mode.contains("pressure-late") {
        assert!(pressure_released);
    }
    assert_eq!(owned.test_management().map(halt_facts), management);
    if let Some(peer) = &peer {
        assert!(nix::fcntl::fcntl(peer, nix::fcntl::FcntlArg::F_GETFD).is_ok());
    }
    match halt.0.take().unwrap().into_test_owner() {
        Ok(input) => guard.input = Some(input),
        Err(management) => guard.management = Some(management),
    }
}

fn drain(
    reader: &mut UnixStream,
    context: &crate::flow::context::RecordingContext,
) -> Vec<serde_json::Value> {
    trace_through(
        reader,
        vec![],
        context.barrier.recorder().boundary().admitted.unwrap(),
        Instant::now() + Duration::from_secs(10),
    )
    .unwrap()
}

struct HaltGuard<'a>(Option<Box<FoundationHalt<'a>>>);
impl Drop for HaltGuard<'_> {
    fn drop(&mut self) {
        if let Some(owner) = &mut self.0 {
            crate::flow::context::worker_startup::tests::reap(
                &mut owner.test_input().serving.owner.gate_child,
            );
        }
    }
}

struct BrokenClock(Option<u64>);
impl PumpIo for BrokenClock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0
            .ok_or_else(|| std::io::Error::other("fixture monotonic read failure"))
    }
    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("a failed or reversed halt clock permits no wait")
    }
}
