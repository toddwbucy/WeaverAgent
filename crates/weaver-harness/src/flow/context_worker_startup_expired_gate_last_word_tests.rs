//! Actual gate EOF and stderr are distinct observations under the original caps.
use super::*;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::worker_startup::rollback::failed::FoundationRollbackExitProgress as ExitProgress;
use weaver_trace::serving::Kind;

pub(super) fn is_mode(mode: &str) -> bool {
    mode.starts_with("expired-startup-raised-lower-last-word-")
}

struct CleanupIo<'a>(&'a mut FakePumpIo);
impl PumpIo for CleanupIo<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        end: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        if interests.is_empty() {
            self.0.wait(interests, end)
        } else {
            CircuitIo(self.0).wait(interests, end)
        }
    }
}

fn expected_word() -> serde_json::Value {
    serde_json::json!({"fixture": "gate-last-word", "answer": 42})
}

fn gate(owner: &FoundationExpiredRollback) -> &super::super::gate::ExpiredGateRollback {
    owner.active.as_ref().unwrap().gate.as_ref().unwrap()
}

fn child(
    owner: &FoundationExpiredRollback,
) -> &crate::flow::context::failed_enter_driver::OwnedChild {
    owner.handoff.gate.as_ref().unwrap().child.as_ref().unwrap()
}

fn wait_file(path: &Path, until: Instant) {
    while !path.exists() {
        assert!(
            Instant::now() < until,
            "missing real gate marker {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_snapshot(
    owner: &FoundationExpiredRollback,
    eof: bool,
    word: &serde_json::Value,
    until: Instant,
) {
    loop {
        if child(owner)
            .last_word_snapshot()
            .is_some_and(|snapshot| snapshot.eof == eof && snapshot.word.as_ref() == Some(word))
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "actual stderr reader did not publish its word/EOF"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn advance(
    owner: &mut FoundationExpiredRollback,
    clock: &mut FakePumpIo,
    until: Instant,
) -> FoundationExpiredRollbackProgress {
    assert!(
        Instant::now() < until,
        "last-word cleanup: {:?}",
        owner.detail()
    );
    let result = owner.advance_with_io(&mut CleanupIo(clock));
    assert_ne!(
        result,
        FoundationExpiredRollbackProgress::Stopped,
        "{:?}",
        owner.detail()
    );
    assert!(owner.run.context.failure().is_none());
    assert!(owner.run.context.flow.halt().is_none());
    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finish(
    guard: &mut Guard,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    peer: &OwnedFd,
    directive: &OrganEnvelope,
    requests: &[String],
    mode: &str,
) {
    let until = Instant::now() + Duration::from_secs(12);
    let root = path.parent().unwrap();
    let delayed = mode.ends_with("-delayed");
    let timeout = mode.ends_with("-timeout");
    let lower_cap = mode.ends_with("-lower-cap");
    let leave_cap = mode.ends_with("-leave-cap");
    let failed = mode.ends_with("-failure");
    let owner = guard.0.as_mut().unwrap();
    let leave = owner.leave.unwrap();
    let enter = owner.handoff.identity.enter;
    let original = match owner.handoff.held.as_ref() {
        HeldStartup::Ready { gate, .. } => gate,
        _ => panic!("original actually raised startup gate"),
    };
    assert!(
        original
            .send
            .as_ref()
            .is_some_and(|send| send.sent() && send.sent_at().is_some_and(|at| !enter.expired(at)))
    );
    assert!(original.reply.is_some());
    clock.advance_to(leave.began_ns() + 1_000_000).unwrap();
    while !root.join("gate-lower-received").exists() {
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Accepted
        );
        member.assert_no_request();
        assert_eq!(member.requests, requests);
        assert!(packet(peer).is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
    let lower = gate(owner).bound();
    assert_eq!(lower.began_ns(), leave.began_ns());
    let (request, sent, sent_at) = gate(owner).test_lower().unwrap();
    assert!(sent && sent_at.is_some_and(|at| !lower.expired(at)));
    let lower_evidence = (request.clone(), sent, sent_at);
    assert_eq!(
        serde_json::from_slice::<OrganEnvelope>(
            &std::fs::read(root.join("gate-lower-received")).unwrap()
        )
        .unwrap(),
        lower_evidence.0
    );
    assert!(gate(owner).test_death().is_none());
    if !delayed {
        wait_file(&root.join("gate-word-written"), until);
        wait_snapshot(owner, false, &expected_word(), until);
    }
    if lower_cap {
        clock.advance_to(lower.deadline_ns() - 50_000_000).unwrap();
    }
    std::fs::write(root.join("gate-word-close"), b"close").unwrap();
    wait_file(&root.join("gate-word-closed"), until);
    while gate(owner).test_death().is_none() {
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Accepted
        );
        member.assert_no_request();
        assert!(packet(peer).is_none());
    }
    let death = gate(owner).test_death().unwrap();
    let observed = death.0.expect("actual channel-loss clock");
    let end = death.1.expect("one stderr settlement deadline");
    assert_eq!(end, (observed + 200_000_000).min(leave.deadline_ns()));
    assert!(!death.3 && !death.2.as_ref().unwrap().eof);
    assert_eq!(
        death.2.as_ref().unwrap().word,
        (!delayed).then(expected_word)
    );
    assert_eq!(child(owner).state(), ChildState::Running);
    assert!(gate(owner).swept() && gate(owner).test_retired());
    assert!(gate(owner).test_recorded().is_none());
    assert!(owner.unload_sequence().is_none());
    assert!(owner.active.as_ref().unwrap().release.is_none());
    assert_eq!(member.requests, requests);

    if leave_cap {
        clock.advance_to(leave.deadline_ns()).unwrap();
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Closed
        );
        assert!(owner.abandoned());
        assert_eq!(gate(owner).test_death(), Some(death));
        assert!(gate(owner).test_recorded().is_none());
        assert!(owner.unload_sequence().is_none());
        assert_eq!(
            owner
                .run
                .context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::Fault)
                .count(),
            0
        );
        assert_eq!(
            owner.run.context.deadlines.get(PumpDeadline::Leave),
            Some(leave)
        );
        assert_eq!(member.requests, requests);
        assert!(
            packet(peer).is_none(),
            "Leave cap cannot invent a death record or Left"
        );
        return;
    }
    if failed {
        struct BrokenOnce<'a> {
            clock: &'a mut FakePumpIo,
            broken: bool,
        }
        impl PumpIo for BrokenOnce<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                if !self.broken {
                    self.broken = true;
                    Err(std::io::Error::other("last-word fixture clock failure"))
                } else {
                    self.clock.now()
                }
            }
            fn wait(
                &mut self,
                _: &[WaitInterest<'_>],
                _: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                panic!("failed first clock cannot wait");
            }
        }
        assert_eq!(
            owner.advance_with_io(&mut BrokenOnce {
                clock,
                broken: false
            }),
            FoundationExpiredRollbackProgress::Stopped
        );
        let first = owner.run.context.failure.clone().unwrap();
        assert!(matches!(
            first,
            RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            }
        ));
        let mut exit = ExitGuard(
            guard
                .0
                .take()
                .unwrap()
                .failed_exit()
                .ok()
                .expect("actual failed death settlement"),
        );
        assert_eq!(
            exit.0.test_gate().unwrap().test_death(),
            Some(death.clone())
        );
        assert_eq!(
            exit.0.advance_with_io(&mut CleanupIo(clock)),
            ExitProgress::Progress
        );
        assert!(
            exit.0
                .test_context()
                .unwrap()
                .halt_report()
                .unwrap()
                .attempted()
        );
        assert_eq!(
            exit.0.test_gate().unwrap().test_death(),
            Some(death.clone())
        );
        assert!(exit.0.test_gate().unwrap().test_recorded().is_none());
        std::fs::write(root.join("gate-word-exit"), b"exit").unwrap();
        loop {
            assert!(Instant::now() < until, "failed last-word cleanup watchdog");
            assert_eq!(exit.0.failure(), Some(&first));
            assert_eq!(
                exit.0.test_gate().unwrap().test_death(),
                Some(death.clone())
            );
            assert!(
                exit.0.test_gate().unwrap().test_recorded().is_none(),
                "no ordinary death author after halt"
            );
            let progress = exit.0.advance_with_io(&mut CleanupIo(clock));
            if progress == ExitProgress::Closed {
                break;
            }
            if progress == ExitProgress::Pending {
                exit.0.wait_with_io(&mut CleanupIo(clock));
            }
            assert!(packet(peer).is_none());
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!exit.0.abandoned());
        assert!(matches!(
            exit.0
                .test_gate_resources()
                .unwrap()
                .child
                .as_ref()
                .unwrap()
                .state(),
            ChildState::Reaped(_)
        ));
        assert_eq!(member.requests, requests);
        assert!(packet(peer).is_none(), "failed settlement grants no Left");
        reader.set_nonblocking(true).unwrap();
        let rows = super::super::super::tests::trace_through(
            reader,
            vec![],
            weaver_trace::Sequence(exit.0.unload_sequence().unwrap()),
            until,
        )
        .unwrap();
        let faults: Vec<_> = rows.iter().filter(|row| row["kind"] == "fault").collect();
        assert_eq!(
            faults.len(),
            1,
            "only the actual first flow failure authors a fault"
        );
        let report: weaver_types::FaultReport =
            serde_json::from_value(faults[0]["payload"].clone()).unwrap();
        assert_eq!(report.case, weaver_types::FaultCase::StreamWriteFailed);
        assert_eq!(rows.last().unwrap()["kind"], "unload");
        return;
    }
    if delayed {
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Accepted
        );
        assert!(!gate(owner).test_death().unwrap().3);
        assert!(gate(owner).test_recorded().is_none());
        assert!(clock.now().unwrap() < end);
        std::fs::write(root.join("gate-word-write"), b"write").unwrap();
        wait_file(&root.join("gate-word-written"), until);
        wait_snapshot(owner, true, &expected_word(), until);
    } else if timeout {
        clock.advance_to(end).unwrap();
        let began = Instant::now();
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Accepted
        );
        assert!(
            began.elapsed() < Duration::from_millis(150),
            "snapshot must not call the blocking200ms take()"
        );
        assert_eq!(child(owner).state(), ChildState::Running);
        assert!(gate(owner).test_death().unwrap().3);
        assert!(!gate(owner).test_death().unwrap().2.unwrap().eof);
    } else if lower_cap {
        assert!(lower.deadline_ns() < end);
        clock.advance_to(lower.deadline_ns()).unwrap();
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Accepted
        );
        assert!(
            owner
                .run
                .context
                .deadlines
                .get(PumpDeadline::Lower)
                .is_none()
        );
        assert_eq!(
            owner.run.context.deadlines.get(PumpDeadline::Leave),
            Some(leave)
        );
        assert_eq!(gate(owner).test_death().unwrap().1, Some(end));
        assert!(!root.join("gate-word-exit").exists());
    }
    while !gate(owner).test_death().unwrap().3 || gate(owner).test_recorded().is_none() {
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Accepted
        );
        assert_eq!(gate(owner).test_death().unwrap().1, Some(end));
        assert_eq!(member.requests, requests);
        member.assert_no_request();
        assert!(packet(peer).is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
    let settled = gate(owner).test_death().unwrap();
    assert_eq!(settled.2.as_ref().unwrap().word, Some(expected_word()));
    if delayed {
        assert!(settled.2.as_ref().unwrap().eof && clock.now().unwrap() < end);
    }
    if timeout {
        assert!(!settled.2.as_ref().unwrap().eof);
        assert_eq!(child(owner).state(), ChildState::Running);
        assert!(owner.unload_sequence().is_none());
        assert!(owner.active.as_ref().unwrap().release.is_none());
        std::fs::write(root.join("gate-word-exit"), b"exit").unwrap();
    }
    loop {
        let progress = advance(owner, clock, until);
        assert_eq!(
            gate(owner).test_death(),
            Some(settled.clone()),
            "record preparation freezes the selected word"
        );
        if progress == FoundationExpiredRollbackProgress::Closed {
            break;
        }
        if !gate(owner).test_disposition().1 {
            assert_eq!(member.requests, requests);
            assert!(owner.active.as_ref().unwrap().release.is_none());
            assert!(owner.unload_sequence().is_none());
        }
        member.step(clock.now().unwrap(), "rollback-ready");
        assert!(packet(peer).is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!owner.abandoned());
    assert!(matches!(child(owner).state(), ChildState::Reaped(_)));
    if lower_cap {
        assert!(matches!(
            child(owner).state(),
            ChildState::Reaped(nix::sys::wait::WaitStatus::Signaled(
                _,
                nix::sys::signal::Signal::SIGKILL,
                _
            ))
        ));
    } else {
        assert!(matches!(
            child(owner).state(),
            ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0))
        ));
    }
    if timeout {
        wait_snapshot(
            owner,
            true,
            &serde_json::json!({"fixture":"word after frozen death"}),
            until,
        );
        assert_eq!(gate(owner).test_death(), Some(settled));
    }
    let (request, sent, sent_at) = gate(owner).test_lower().unwrap();
    assert_eq!((request.clone(), sent, sent_at), lower_evidence);
    let answer = packet(peer).expect("actual Leave answer after death record, reap and Unload");
    assert_eq!(answer.exchange, directive.exchange);
    assert!(matches!(
        answer.payload,
        Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            forced: true,
            no_state: false
        })
    ));
    let mut expected = requests.to_vec();
    expected.push("grants".into());
    assert_eq!(member.requests, expected);
    member.assert_closed_without_request();
    reader.set_nonblocking(true).unwrap();
    let rows = super::super::super::tests::trace_through(
        reader,
        vec![],
        weaver_trace::Sequence(owner.unload_sequence().unwrap()),
        until,
    )
    .unwrap();
    let faults: Vec<_> = rows.iter().filter(|row| row["kind"] == "fault").collect();
    assert_eq!(faults.len(), 1);
    let report: weaver_types::FaultReport =
        serde_json::from_value(faults[0]["payload"].clone()).unwrap();
    assert_eq!(report.case, weaver_types::FaultCase::OrganDeathObserved);
    let account: serde_json::Value = serde_json::from_str(report.account.get()).unwrap();
    assert_eq!(account["organ"], "gate");
    assert_eq!(account["last_word"], expected_word());
    assert_eq!(
        faults[0]["sequence"],
        serde_json::to_value(gate(owner).test_recorded().unwrap()).unwrap()
    );
    assert_eq!(rows.last().unwrap()["kind"], "unload");
    assert!(
        rows.iter()
            .all(|row| !matches!(row["kind"].as_str(), Some("turn.started" | "save_point")))
    );
}
