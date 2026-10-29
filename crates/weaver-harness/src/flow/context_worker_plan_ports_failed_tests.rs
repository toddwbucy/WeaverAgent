//! Real state EOF keeps the pending refusal and its original operation account.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::worker_startup::input::management::plan_refusal::composition_tests::pressure;
use crate::flow::context::worker_startup::input::management::tests::Guard;
use crate::flow::context::worker_startup::input::plan_ports::tests::check_pending;
use crate::flow::context::worker_startup::tests::{Member, reap, worker};
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(in crate::flow::context::worker_startup) struct Failure {
    handoff: Box<FoundationFailedHandoff>,
    first: RunFailure,
    deadline: u64,
    request: String,
    decision: AbsoluteBound,
    ticket: u64,
    initial: Vec<u8>,
}

pub(in crate::flow::context::worker_startup) fn begin(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    request: String,
    decision: AbsoluteBound,
    ticket: u64,
) -> Failure {
    member.stop_responses();
    let input = guard.input.as_mut().unwrap();
    let watch = Instant::now() + Duration::from_secs(5);
    while input.advance() != FoundationInputProgress::Stopped {
        assert!(
            Instant::now() < watch,
            "state EOF must stop the original operation"
        );
    }
    assert!(input.take_plan_operation().is_none());
    assert!(input.plan_operation_pending());
    let context = &input.serving.owner.context;
    let first = context.failure().unwrap().clone();
    assert!(matches!(
        first,
        RunFailure::Flow {
            reason: FlowHaltReason::CarryFailed,
            ..
        }
    ));
    assert_eq!(context.pending_author().unwrap().ticket, ticket);
    let deadline = context.halt().unwrap().deadline().unwrap();
    let mut initial = pressure::drain(context, reader);
    let mut halt = match worker::halt(Ok(guard.input.take().unwrap())) {
        Ok(halt) => halt,
        Err(Ok(input)) => {
            guard.input = Some(input);
            panic!("failed plan operation refused its halt handoff");
        }
        Err(Err(_)) => unreachable!(),
    };
    let context = &halt.test_input().serving.owner.context;
    assert_eq!(context.failure(), Some(&first));
    check_context(context, &request, ticket, false);
    let _ = reader.read_to_end(&mut initial);
    let handoff = halt.handoff().ok().expect("terminal failed plan handoff");
    check_pending(handoff._plan_operation.as_ref().unwrap(), decision);
    Failure {
        handoff,
        first,
        deadline,
        request,
        decision,
        ticket,
        initial,
    }
}

fn check_context(
    context: &crate::flow::context::RecordingContext,
    request: &str,
    ticket: u64,
    settled: bool,
) {
    let (original, held_ticket, recorded, consumed) =
        context.plan_port_work().unwrap().test_refusal_account();
    assert_eq!(format!("{original:?}"), request);
    assert_eq!(held_ticket, Some(ticket));
    if settled {
        assert!(
            recorded.is_some(),
            "failed cleanup must record the exact owed refusal"
        );
    }
    assert!(!consumed, "failure grants no ordinary completion");
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
        request,
        decision,
        ticket,
        mut initial,
    } = failure;
    let done = AtomicBool::new(false);
    let (mut guard, bytes) = std::thread::scope(|scope| {
        let recorder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut initial);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut initial);
            initial
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
                panic!("original run refused plan operation failed handoff");
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
    check_pending(owner.handoff._plan_operation.as_ref().unwrap(), decision);
    check_context(&owner.tail.context, &request, ticket, true);
    assert!(owner.lower_confirmed);
    assert!(!owner.abandoned());
    let rows: Vec<serde_json::Value> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|row| row["kind"] == "fault").count(), 1);
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "refusal" && row["payload"]["seam"] == "plan_operation")
            .count(),
        1
    );
}
