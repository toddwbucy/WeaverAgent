//! An actual member EOF during root preflight must keep the first failure and
//! the original root request through the worker's complete failed exit.
use super::*;
use crate::flow::context::plan_update_driver::RootPlanInput;
use crate::flow::context::worker_startup::input::management::tests::{Guard, read_packet};
use crate::flow::context::worker_startup::input::plan::tests::{accepted, request};
use crate::flow::context::worker_startup::tests::{Member, reap, worker};
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(in crate::flow::context::worker_startup) struct Handoff {
    owner: Box<FoundationFailedHandoff>,
    peer: OwnedFd,
    document: String,
    first: RunFailure,
    deadline: u64,
}

pub(in crate::flow::context::worker_startup) fn handoff(
    serving: FoundationServing<'_>,
    member: &mut Member,
    socket: &Path,
) -> Handoff {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let document = crate::flow::pump::plan::tests::document();
    let peer = request(socket, &document);
    accepted(&mut guard);
    let input = guard.input.as_mut().unwrap();
    assert_eq!(input.advance(), FoundationInputProgress::Pending);
    assert!(input.serving.owner.context.read.is_some());
    member.stop_responses();
    let end = Instant::now() + Duration::from_secs(5);
    while input.advance() != FoundationInputProgress::Stopped {
        assert!(Instant::now() < end, "member EOF must stop the root update");
    }
    let context = &input.serving.owner.context;
    let first = context.failure().unwrap().clone();
    assert!(matches!(
        first,
        RunFailure::Flow {
            reason: FlowHaltReason::CarryFailed,
            ..
        }
    ));
    let deadline = context.halt().unwrap().deadline().unwrap();
    assert!(context.root_plan_work().unwrap().input_filing().is_none());
    let halt = match worker::halt(Ok(guard.input.take().unwrap())) {
        Ok(halt) => halt,
        Err(Ok(input)) => {
            guard.input = Some(input);
            panic!("failed root input refused its halt owner");
        }
        Err(Err(_)) => unreachable!(),
    };
    assert_eq!(halt.progress(), FoundationHaltProgress::Prepared);
    let owner = halt.handoff().ok().expect("failed root handoff");
    assert!(owner._plan.is_some());
    assert_eq!(owner.deadline, Some(deadline));
    Handoff {
        owner,
        peer,
        document,
        first,
        deadline,
    }
}

struct ExitGuard(Option<Box<FoundationFailedExit>>);
impl Drop for ExitGuard {
    fn drop(&mut self) {
        if let Some(owner) = &mut self.0 {
            owner.abort();
            reap(&mut owner.handoff.gate_child);
            owner.tail.reap_composition_test_children();
        }
    }
}
struct StopThread<'a>(&'a AtomicBool);
impl Drop for StopThread<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    handoff: Handoff,
    reader: &mut UnixStream,
) {
    let Handoff {
        owner,
        peer,
        document,
        first,
        deadline,
    } = handoff;
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (mut guard, bytes) = std::thread::scope(|scope| {
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThread(&done);
        let owner = match worker::failed_exit(run, owner) {
            Ok(owner) => owner,
            Err((mut run, mut handoff)) => {
                reap(&mut handoff.gate_child);
                run.resources.reap_startup_test_children();
                panic!("original run refused failed root handoff");
            }
        };
        let guard = ExitGuard(Some(owner));
        done.store(true, Ordering::SeqCst);
        (guard, recorder.join().unwrap())
    });
    let owner = guard.0.as_mut().unwrap();
    assert_eq!(owner.progress(), FoundationExitProgress::Closed);
    assert_eq!(
        owner.process_exit(),
        Some(crate::FoundationProcessExit::Failure)
    );
    assert_eq!(owner.tail.context.failure(), Some(&first));
    assert_eq!(owner.handoff.deadline, Some(deadline));
    assert!(owner.handoff._plan.is_some());
    let work = owner
        .tail
        .context
        .root_plan_work()
        .expect("root account retained after failure");
    let RootPlanInput::Accepted(input) = work.input() else {
        panic!("accepted root account lost")
    };
    assert_eq!(input.packet().input().2, document);
    assert!(work.input_filing().is_none());
    assert!(
        read_packet(&peer).is_none(),
        "failed update must not answer success"
    );
    assert!(
        owner.lower_confirmed,
        "the actual gate must acknowledge Lower"
    );
    assert!(
        !owner.abandoned(),
        "healthy sink and children allow complete failed cleanup"
    );
    let rows: Vec<serde_json::Value> = bytes
        .split(|b| *b == b'\n')
        .filter(|row| !row.is_empty())
        .map(|row| serde_json::from_slice(row).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|row| row["kind"] == "fault").count(), 1);
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    assert!(
        !rows
            .iter()
            .any(|row| row["kind"] == "plan.event" || row["kind"] == "turn.started")
    );
}
