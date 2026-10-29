//! A real root cancellation passes through management into the same run's
//! Lower, terminal records, releases and final drain. The member is scripted.
use super::*;
use crate::flow::context::worker_startup::FoundationRun;
use crate::flow::context::worker_startup::tests::{reap, worker};
use crate::{
    FoundationEnteredRollback, FoundationEnteredRollbackHandoff, FoundationEnteredRollbackProgress,
    FoundationProcessExit,
};
use std::io::Read;

pub(in crate::flow::context::worker_startup) struct Handoff {
    owner: Box<FoundationEnteredRollbackHandoff>,
    peer: OwnedFd,
    document: String,
}

struct StopThreads<'a>(&'a AtomicBool);
impl Drop for StopThreads<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
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
    let original = request(socket, &document);
    accepted(&mut guard);
    assert_eq!(
        guard.input.as_mut().unwrap().advance(),
        FoundationInputProgress::Pending
    );
    assert!(
        guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .owner
            .context
            .read
            .is_some()
    );
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 47,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Leave {
            cause: Cause { uid: 0 },
            forced: false,
            rollback: true,
        }),
    };
    let raw = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
        raw.len()
    );
    let end = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let _stop = StopThreads(&done);
        loop {
            let owner = guard.management.as_mut().unwrap();
            match owner.advance() {
                FoundationManagementProgress::DirectivePending => break,
                FoundationManagementProgress::Pending => (),
                other => panic!(
                    "root Leave did not settle cancellation: {other:?}, {:?}",
                    owner.detail()
                ),
            }
            assert!(Instant::now() < end);
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
    });
    let input = &guard.management.as_ref().unwrap().input;
    assert!(!input.plan_pending());
    assert!(input.serving.owner.context.root_plan_work().is_none());
    assert!(input.serving.owner.context.read.is_none());
    let Some(RootPlanCompletion::Cancelled(account)) =
        &input.serving.plan.as_ref().unwrap().completion
    else {
        panic!("Leave must retain cancelled root work");
    };
    assert_eq!(account.packet().input().2, document);
    assert!(account.work().input_filing().is_none());
    assert!(
        read_packet(&original).is_none(),
        "cancelled root must not answer success"
    );
    let owner = match guard.management.take().unwrap().rollback_handoff() {
        Ok(owner) => owner,
        Err(owner) => {
            guard.management = Some(owner);
            panic!("cancelled root work blocked rollback handoff");
        }
    };
    assert!(owner._plan.is_some());
    Handoff {
        owner,
        peer,
        document,
    }
}

struct ExitGuard(Option<Box<FoundationEnteredRollback>>);
impl Drop for ExitGuard {
    fn drop(&mut self) {
        if let Some(owner) = &mut self.0 {
            owner.test_reap();
        }
    }
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    handoff: Handoff,
    member: &mut Member,
    reader: &mut UnixStream,
) {
    let Handoff {
        owner,
        peer,
        document,
    } = handoff;
    let exchange = owner.message.envelope.exchange.clone();
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (mut guard, bytes) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live("rollback-ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let owner = match worker::entered_rollback(run, owner) {
            Ok(owner) => owner,
            Err((mut run, mut handoff)) => {
                reap(handoff.arms.child.as_mut().unwrap());
                run.resources.reap_startup_test_children();
                panic!("original run refused root cancellation handoff");
            }
        };
        let guard = ExitGuard(Some(owner));
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (guard, recorder.join().unwrap())
    });
    let owner = guard.0.as_mut().unwrap();
    assert_eq!(
        owner.progress(),
        FoundationEnteredRollbackProgress::Closed,
        "{:?}",
        owner.detail()
    );
    assert_eq!(owner.process_exit(), Some(FoundationProcessExit::Success));
    let (run, retained) = owner.test_account();
    assert!(run.context.failure().is_none());
    assert!(run.context.read.is_none());
    assert!(run.context.root_plan_work().is_none());
    assert!(run.context.deadlines.get(PumpDeadline::Decision).is_none());
    assert!(
        run.context
            .deadlines
            .get(PumpDeadline::PlanAnswer)
            .is_none()
    );
    let Some(RootPlanCompletion::Cancelled(account)) = &retained._plan.as_ref().unwrap().completion
    else {
        panic!("cleanup discarded root cancellation custody");
    };
    assert_eq!(account.packet().input().2, document);
    assert!(account.work().input_filing().is_none());
    member.assert_closed_without_request();
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
    assert_eq!(answer.exchange, exchange);
    assert!(matches!(
        answer.payload,
        Payload::Answer(weaver_types::LifecycleAnswer::Left {
            save_point: None,
            ..
        })
    ));
    let rows: Vec<serde_json::Value> = bytes
        .split(|b| *b == b'\n')
        .filter(|row| !row.is_empty())
        .map(|row| serde_json::from_slice(row).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    assert!(!rows.iter().any(|row| row["kind"] == "plan.event"));
    assert!(!rows.iter().any(|row| row["kind"] == "turn.started"));
}
