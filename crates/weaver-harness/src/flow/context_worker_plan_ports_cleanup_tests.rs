//! The cancelled plan account stays owned through the real rollback driver.
use super::*;
use crate::flow::context::worker_startup::tests::{Member, reap, worker};
use std::sync::atomic::{AtomicBool, Ordering};

pub(in crate::flow::context::worker_startup) struct Leave {
    pub(super) handoff: Box<crate::FoundationEnteredRollbackHandoff>,
    pub(super) peer: OwnedFd,
    pub(super) request: String,
    pub(super) root: std::path::PathBuf,
}

pub(super) fn check_account(handoff: &crate::FoundationEnteredRollbackHandoff, request: &str) {
    let work = handoff
        ._plan_operation
        .as_ref()
        .expect("retained plan owner");
    assert!(work.cancelled);
    assert!(work.owned_decision.is_none());
    let result = work.result.as_ref().expect("completed original account");
    assert_eq!(format!("{:?}", result.request()), request);
    assert!(matches!(result.outcome(), FoundationPlanOutcome::Cancelled));
}

struct ExitGuard(Option<Box<crate::FoundationEnteredRollback>>);
impl Drop for ExitGuard {
    fn drop(&mut self) {
        if let Some(owner) = &mut self.0 {
            owner.test_reap();
        }
    }
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<crate::FoundationRun>,
    cleanup: Cleanup,
    member: &mut Member,
    reader: &mut UnixStream,
) {
    let Cleanup::Leave(cleanup) = cleanup else {
        let Cleanup::Failed(failure) = cleanup else {
            unreachable!()
        };
        return failed::finish(run, failure, reader);
    };
    let Leave {
        handoff,
        peer,
        request,
        root,
    } = cleanup;
    let done = AtomicBool::new(false);
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
        struct Done<'a>(&'a AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _done = Done(&done);
        let owner = match worker::entered_rollback(run, handoff) {
            Ok(owner) => owner,
            Err((mut run, mut handoff)) => {
                reap(handoff.arms.child.as_mut().unwrap());
                run.resources.reap_startup_test_children();
                panic!("original run refused plan operation rollback handoff");
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
        crate::FoundationEnteredRollbackProgress::Closed,
        "{:?}",
        owner.detail()
    );
    assert_eq!(
        owner.process_exit(),
        Some(crate::FoundationProcessExit::Success),
        "{}",
        owner.test_exit_detail()
    );
    let (run, retained) = owner.test_account();
    check_account(retained, &request);
    assert!(
        root.join("plan-port-lowered").exists(),
        "actual gate received Lower"
    );
    assert!(run.context.failure().is_none());
    assert!(run.context.plan_port_work().is_none());
    member.assert_closed_without_request();
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
    assert!(matches!(
        answer.payload,
        Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            ..
        })
    ));
    assert!(read_packet(&peer).is_none());
    let rows: Vec<serde_json::Value> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    assert!(
        rows.iter().all(|row| row["kind"] != "refusal"),
        "cleanup does not replay the settled refusal"
    );
}
