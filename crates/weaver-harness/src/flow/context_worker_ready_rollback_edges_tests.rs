//! Deadline and real failure transitions from an actual successful Ready.
use super::ready_rollback::Handoff;
use super::*;
use crate::flow::clock::{PumpIo, WaitInterest, WaitOutcome};
use crate::flow::context::worker_startup::FoundationRun;
use crate::flow::context::worker_startup::entered_rollback::{
    FoundationEnteredRollback, FoundationEnteredRollbackProgress as RollbackProgress,
};
use crate::flow::pump::PumpDeadline;

struct Guard(Box<FoundationEnteredRollback>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.test_reap();
    }
}

struct BrokenOnce<'a> {
    clock: &'a mut FakePumpIo,
    broken: bool,
}

struct ExpireAtAdmission {
    before: u64,
    end: u64,
    samples: usize,
}
impl PumpIo for ExpireAtAdmission {
    fn now(&mut self) -> std::io::Result<u64> {
        self.samples += 1;
        Ok(if self.samples == 1 {
            self.before
        } else {
            self.end
        })
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("expired Unload admission cannot wait")
    }
}
impl PumpIo for BrokenOnce<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if !self.broken {
            self.broken = true;
            Err(std::io::Error::other("entered rollback test clock failure"))
        } else {
            self.clock.now()
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("the injected first clock failure cannot wait")
    }
}

pub(super) fn finish(
    run: Box<FoundationRun>,
    handoff: Handoff,
    member: &mut Member,
    reader: &mut UnixStream,
    _root: &Path,
    mode: &str,
) {
    let Handoff {
        actual,
        peer,
        origin,
        enter,
        cause,
    } = handoff;
    let leave = actual.leave;
    let raw_ready = actual.gate_reply.raw_bytes().to_vec();
    let mut clock = FakePumpIo::new(crate::flow::clock::monotonic_ns().unwrap());
    let mut guard = Guard(match run.entered_rollback(actual) {
        Ok(owner) => owner,
        Err((mut run, mut actual)) => {
            crate::flow::context::worker_startup::tests::reap(actual.arms.child.as_mut().unwrap());
            run.resources.reap_startup_test_children();
            panic!("actual Ready and original run must join");
        }
    });
    let owner = &mut guard.0;
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let bytes = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live("rollback-ready-edge");
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
        let until = Instant::now() + Duration::from_secs(10);
        let tick = |owner: &mut FoundationEnteredRollback| {
            assert!(
                Instant::now() < until,
                "rollback edge watchdog: {:?}",
                owner.detail()
            );
            let progress = owner.advance();
            std::thread::sleep(Duration::from_millis(1));
            progress
        };
        if mode.ends_with("-clock-failure") {
            assert!(owner.test_gate().is_none());
            assert_eq!(
                owner.advance_with_io(&mut BrokenOnce {
                    clock: &mut clock,
                    broken: false
                }),
                RollbackProgress::Progress
            );
            let first = owner.test_context().failure().unwrap().clone();
            assert!(matches!(
                first,
                RunFailure::Flow {
                    reason: FlowHaltReason::TraceFailed,
                    ..
                }
            ));
            let halt = owner
                .test_context()
                .flow
                .halt()
                .unwrap()
                .deadline()
                .unwrap();
            assert_eq!(
                owner.test_failed().unwrap().deadline(),
                Some(halt.min(leave.deadline_ns()))
            );
            assert!(
                owner
                    .test_gate()
                    .unwrap()
                    .test_lower()
                    .is_none_or(|(_, sent, _)| !sent)
            );
            tick(owner);
            assert!(owner.test_context().halt_report().unwrap().attempted());
            assert!(
                owner
                    .test_gate()
                    .unwrap()
                    .test_lower()
                    .is_none_or(|(_, sent, _)| !sent),
                "fault attempt precedes Lower"
            );
            while owner.progress() != RollbackProgress::Closed {
                tick(owner);
            }
            assert_eq!(owner.test_context().failure(), Some(&first));
            assert!(!owner.abandoned(), "{:?}", owner.detail());
            assert!(owner.unload_sequence().is_some());
        } else {
            while !owner.test_before_unload() {
                assert_ne!(
                    tick(owner),
                    RollbackProgress::Closed,
                    "{:?}",
                    owner.detail()
                );
            }
            let lower = owner
                .test_gate()
                .unwrap()
                .test_lower()
                .map(|(request, sent, at)| (request.clone(), sent, at));
            if mode.ends_with("-leave-cap") || mode.ends_with("-unload-cap") {
                let before = owner.test_context().last_author_sequence;
                if mode.ends_with("-unload-cap") {
                    let mut io = ExpireAtAdmission {
                        before: owner.test_context().last_ns,
                        end: leave.deadline_ns(),
                        samples: 0,
                    };
                    assert_eq!(owner.advance_with_io(&mut io), RollbackProgress::Closed);
                    assert!(
                        io.samples >= 2,
                        "expiry occurred inside actual Unload admission"
                    );
                } else {
                    clock.advance_to(leave.deadline_ns()).unwrap();
                    assert_eq!(owner.advance_with_io(&mut clock), RollbackProgress::Closed);
                }
                assert!(owner.test_context().failure().is_none());
                assert!(owner.test_context().flow.halt().is_none());
                assert_eq!(owner.test_context().last_author_sequence, before);
                assert!(owner.unload_sequence().is_none());
            } else {
                assert!(mode.ends_with("-recorded-clock") || mode.ends_with("-drained-clock"));
                while owner.unload_sequence().is_none() {
                    assert_ne!(tick(owner), RollbackProgress::Closed);
                }
                if mode.ends_with("-drained-clock") {
                    while owner.test_context().phase != crate::flow::context::RecordingPhase::Closed
                    {
                        assert_ne!(tick(owner), RollbackProgress::Closed);
                    }
                }
                let recorded = owner.unload_sequence();
                clock.advance_to(owner.test_context().last_ns).unwrap();
                assert_eq!(
                    owner.advance_with_io(&mut BrokenOnce {
                        clock: &mut clock,
                        broken: false
                    }),
                    RollbackProgress::Closed
                );
                assert_eq!(owner.unload_sequence(), recorded);
                assert!(
                    owner.test_failed().is_none(),
                    "Recorded Unload allows disposal only"
                );
            }
            assert!(owner.abandoned());
            assert_eq!(
                owner
                    .test_gate()
                    .unwrap()
                    .test_lower()
                    .map(|(request, sent, at)| (request.clone(), sent, at)),
                lower
            );
            let (_, account) = owner.test_account();
            assert_eq!(account.gate_reply.raw_bytes(), raw_ready);
            assert_eq!(account.enter, enter);
        }
        assert!(std::sync::Arc::ptr_eq(
            &owner.test_context().read_origin,
            &origin
        ));
        assert_eq!(owner.test_context().retired_enter, Some(enter));
        assert_eq!(
            owner.test_context().deadlines.get(PumpDeadline::Leave),
            Some(leave)
        );
        assert!(
            owner
                .test_context()
                .deadlines
                .get(PumpDeadline::Enter)
                .is_none()
        );
        let sequence = owner.unload_sequence();
        assert_eq!(owner.advance_with_io(&mut clock), RollbackProgress::Closed);
        assert_eq!(owner.unload_sequence(), sequence);
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    });
    assert!(
        read_packet(&peer).is_none(),
        "failed or capped cleanup grants no Left"
    );
    let rows: Vec<serde_json::Value> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
        .map(|row| serde_json::from_slice(row).unwrap())
        .collect();
    assert_eq!(
        rows.iter().filter(|row| row["kind"] == "unload").count(),
        usize::from(owner.unload_sequence().is_some())
    );
    if let Some(unload) = rows.iter().find(|row| row["kind"] == "unload") {
        assert_eq!(
            unload["payload"]["cause"],
            serde_json::to_value(cause).unwrap()
        );
        assert_eq!(rows.last().unwrap()["kind"], "unload");
    }
    assert_eq!(
        rows.iter().filter(|row| row["kind"] == "fault").count(),
        usize::from(mode.ends_with("-clock-failure"))
    );
    assert!(
        rows.iter()
            .all(|row| !matches!(row["kind"].as_str(), Some("turn.started" | "save_point")))
    );
}
