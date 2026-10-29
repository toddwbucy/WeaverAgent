//! Real failed-exit custody of either the in-flight read or its Recorded sample.
use super::*;
use crate::flow::context::worker_startup::input::management::tests::Guard;
use crate::flow::context::worker_startup::tests::{Member, reap, worker};
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(in crate::flow::context::worker_startup) struct Failure {
    handoff: Box<FoundationFailedHandoff>,
    first: RunFailure,
    deadline: u64,
    recorded: bool,
}

pub(in crate::flow::context::worker_startup) fn begin(
    mut guard: Guard<'_>,
    member: &mut Member,
    recorded: bool,
) -> Failure {
    if !recorded {
        assert!(
            guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .owner
                .context
                .read
                .as_ref()
                .unwrap()
                .plan_expected()
                .is_some(),
            "the actual flag request precedes member EOF"
        );
    }
    member.stop_responses();
    let input = guard.input.as_mut().unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while input.advance() != FoundationInputProgress::Stopped {
        assert!(Instant::now() < end, "member EOF must stop flags owner");
    }
    assert!(
        input.take_observed_flags().is_none(),
        "failed owner cannot grant an untaken sample"
    );
    assert_eq!(input.flags_pending(), !recorded);
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
    let halt = match worker::halt(Ok(guard.input.take().unwrap())) {
        Ok(halt) => halt,
        Err(Ok(input)) => {
            guard.input = Some(input);
            panic!("failed flags owner refused halt")
        }
        Err(Err(_)) => unreachable!(),
    };
    let handoff = halt.handoff().ok().expect("failed flags handoff");
    assert_eq!(handoff._flags.is_some(), recorded);
    Failure {
        handoff,
        first,
        deadline,
        recorded,
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

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    failure: Failure,
    reader: &mut UnixStream,
) {
    let Failure {
        handoff,
        first,
        deadline,
        recorded,
    } = failure;
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
        struct Done<'a>(&'a AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _done = Done(&done);
        let owner = match worker::failed_exit(run, handoff) {
            Ok(owner) => owner,
            Err((mut run, mut handoff)) => {
                reap(&mut handoff.gate_child);
                run.resources.reap_startup_test_children();
                panic!("original run refused flags failed handoff")
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
    assert_eq!(owner.tail.context.flags_pending(), !recorded);
    assert_eq!(owner.handoff._flags.is_some(), recorded);
    assert!(owner.lower_confirmed);
    assert!(!owner.abandoned());
    let rows: Vec<serde_json::Value> = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "fault").count(), 1);
    assert_eq!(rows.iter().filter(|r| r["kind"] == "unload").count(), 1);
    assert!(
        rows.iter().all(|r| r["kind"] != "state.plan.measurement"),
        "failed cleanup must not invent a completed sample"
    );
}
