//! Actual gate custody through failed expired rollback; no reconstructed proof.
use super::*;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::worker_startup::rollback::failed::FoundationRollbackExitProgress as ExitProgress;

pub(super) fn is_mode(mode: &str) -> bool {
    mode.starts_with("expired-startup-raised-lower-failure-")
}

pub(super) fn before_leave(mode: &str) -> bool {
    mode == "expired-startup-raised-lower-failure-before"
}

struct CleanupIo<'a>(&'a mut FakePumpIo);
impl PumpIo for CleanupIo<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        if interests.is_empty() {
            self.0.wait(interests, deadline)
        } else {
            CircuitIo(self.0).wait(interests, deadline)
        }
    }
}

struct BrokenOnce<'a> {
    clock: &'a mut FakePumpIo,
    broken: bool,
}
impl PumpIo for BrokenOnce<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if !self.broken {
            self.broken = true;
            Err(std::io::Error::other("expired gate rollback clock failed"))
        } else {
            self.clock.now()
        }
    }
    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("the injected first clock failure cannot wait")
    }
}

fn gate(owner: &FoundationExpiredRollback) -> &super::super::gate::ExpiredGateRollback {
    owner.active.as_ref().unwrap().gate.as_ref().unwrap()
}

fn lower_evidence(
    gate: &super::super::gate::ExpiredGateRollback,
) -> (OrganEnvelope, bool, Option<u64>) {
    let (request, sent, at) = gate.test_lower().unwrap();
    (request.clone(), sent, at)
}

fn child(exit: &super::super::failure::FoundationExpiredRollbackExit) -> ChildState {
    exit.test_gate_resources()
        .unwrap()
        .child
        .as_ref()
        .unwrap()
        .state()
}

fn assert_raw(actual: &[u8], expected: &(Vec<u8>, usize)) {
    assert_eq!(actual, expected.0);
    assert_eq!(actual.as_ptr() as usize, expected.1);
}

fn wait_file(path: &Path, until: Instant) {
    while !path.exists() {
        assert!(
            Instant::now() < until,
            "missing actual child marker: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finish(
    guard: &mut Guard,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    peer: &OwnedFd,
    requests: &[String],
    mode: &str,
) {
    let until = Instant::now() + Duration::from_secs(12);
    let root = path.parent().unwrap();
    let before = before_leave(mode);
    let pending = mode.ends_with("-lower-pressure");
    let pending_ticket = mode.contains("-pending-ticket-");
    let held = mode.ends_with("-lower-held");
    let cap = mode.ends_with("-lower-cap");
    let recorded = mode.ends_with("-recorded");
    let release_failure = mode.ends_with("-release-wrong");
    let owner = guard.0.as_mut().unwrap();
    let enter = owner.handoff.identity.enter;
    let leave = owner.leave;
    assert_eq!(leave.is_none(), before);
    let original = match owner.handoff.held.as_ref() {
        HeldStartup::Gate(gate) | HeldStartup::Ready { gate, .. } => gate,
        _ => panic!("the actual raised startup gate"),
    };
    let raise = original.raise.clone();
    let reply = original.reply.as_ref().unwrap().raw_bytes();
    let raise_reply = (reply.to_vec(), reply.as_ptr() as usize);
    assert!(original.send.as_ref().is_some_and(|send| {
        send.sent()
            && send
                .sent_at()
                .is_some_and(|at| at >= enter.began_ns() && !enter.expired(at))
    }));
    if before {
        assert!(owner.active.is_none());
        assert!(owner.original.is_none());
        assert!(!root.join("gate-lower-received").exists());
    } else {
        if pending {
            wait_file(&root.join("gate-frames-sent"), until);
        }
        loop {
            let reached = if release_failure {
                owner.progress() == FoundationExpiredRollbackProgress::Stopped
            } else if recorded {
                owner.unload_sequence().is_some()
            } else {
                owner
                    .active
                    .as_ref()
                    .and_then(|active| active.gate.as_ref())
                    .is_some_and(|gate| {
                        if pending {
                            gate.test_pending_frame().is_some() && gate.test_lower().unwrap().1
                        } else if held {
                            gate.test_replies().1.is_some()
                        } else {
                            gate.test_lower().unwrap().1
                        }
                    })
            };
            if reached {
                break;
            }
            assert!(
                Instant::now() < until,
                "gate failure boundary: {:?}",
                owner.detail()
            );
            let progress = owner.advance_with_io(&mut CleanupIo(clock));
            assert_ne!(progress, FoundationExpiredRollbackProgress::Closed);
            if !release_failure {
                assert_ne!(
                    progress,
                    FoundationExpiredRollbackProgress::Stopped,
                    "{:?}",
                    owner.detail()
                );
                assert!(owner.run.context.flow.halt().is_none());
            }
            if recorded || release_failure {
                member.step(clock.now().unwrap(), "rollback-ready");
            } else {
                member.assert_no_request();
                assert_eq!(member.requests, requests);
                assert!(
                    owner
                        .active
                        .as_ref()
                        .is_none_or(|active| active.release.is_none())
                );
            }
            assert!(packet(peer).is_none());
            std::thread::sleep(Duration::from_millis(1));
        }
        wait_file(&root.join("gate-lower-received"), until);
    }
    let prior_lower = (!before).then(|| lower_evidence(gate(owner)));
    let pending_raw = pending.then(|| {
        let raw = gate(owner).test_pending_frame().unwrap();
        (raw.to_vec(), raw.as_ptr() as usize)
    });
    let stopped_raw = held.then(|| {
        let raw = gate(owner).test_replies().1.unwrap().raw_bytes();
        (raw.to_vec(), raw.as_ptr() as usize)
    });
    let original_lower = (!before).then(|| gate(owner).bound());
    let pending_author = if pending_ticket {
        let lower = original_lower.unwrap();
        assert!(owner.run.context.barrier.pending_author().is_none());
        super::super::super::failed::tests::fill_trace(&mut owner.run.context, clock);
        // No readiness is scheduled: the real refusal author owns an actual
        // capacity-pending ticket when the original Lower interrupts its wait.
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::Accepted
        );
        assert_eq!(clock.now().unwrap(), lower.deadline_ns());
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(gate(owner).swept());
        assert!(gate(owner).test_retired());
        assert!(
            owner
                .run
                .context
                .deadlines
                .get(PumpDeadline::Lower)
                .is_none()
        );
        assert_eq!(owner.run.context.deadlines.get(PumpDeadline::Leave), leave);
        assert_raw(
            gate(owner).test_pending_frame().unwrap(),
            pending_raw.as_ref().unwrap(),
        );
        let ticket = owner.run.context.barrier.pending_author().unwrap().ticket;
        let prepared = owner
            .run
            .context
            .barrier
            .pending()
            .expect("actual pending gate refusal");
        assert_eq!(prepared.envelope.kind, weaver_trace::serving::Kind::Refusal);
        assert!(owner.unload_sequence().is_none());
        assert!(packet(peer).is_none());
        Some((ticket, serde_json::to_value(prepared).unwrap()))
    } else {
        None
    };
    if held {
        wait_file(&root.join("gate-stopped-sent"), until);
        assert_eq!(
            owner
                .handoff
                .gate
                .as_ref()
                .unwrap()
                .child
                .as_ref()
                .unwrap()
                .state(),
            ChildState::Running
        );
        assert!(gate(owner).swept());
        assert!(!gate(owner).test_disposition().1);
    }
    let sequence = owner.unload_sequence();
    assert_eq!(sequence.is_some(), recorded);
    if release_failure {
        let release = owner.active.as_ref().unwrap().release.as_ref().unwrap();
        assert!(release.release_sent());
        assert_eq!(release.received_reply().unwrap().exchange.ordinal, 3);
        assert!(release.outcome().is_none());
        assert!(matches!(
            owner.run.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            })
        ));
    } else {
        assert_eq!(
            owner.advance_with_io(&mut BrokenOnce {
                clock,
                broken: false
            }),
            FoundationExpiredRollbackProgress::Stopped
        );
        assert!(matches!(
            owner.run.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            })
        ));
    }
    let first = owner.run.context.failure.clone().unwrap();
    let halt_cap = owner.run.context.flow.halt().unwrap().deadline().unwrap();
    let expected_cap = leave.map_or(halt_cap, |leave| halt_cap.min(leave.deadline_ns()));
    let mut exit = ExitGuard(
        guard
            .0
            .take()
            .unwrap()
            .failed_exit()
            .ok()
            .expect("actual failed gate owner"),
    );
    assert_raw(
        exit.0
            .test_original_gate()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .raw_bytes(),
        &raise_reply,
    );
    assert!(exit.0.test_gate().is_some());
    if let Some(expected) = &prior_lower {
        assert_eq!(lower_evidence(exit.0.test_gate().unwrap()), *expected);
    }
    if let Some(expected) = &pending_raw {
        assert_raw(
            exit.0.test_gate().unwrap().test_pending_frame().unwrap(),
            expected,
        );
    }
    if let Some((ticket, event)) = &pending_author {
        let context = exit.0.test_context().unwrap();
        assert_eq!(context.barrier.pending_author().unwrap().ticket, *ticket);
        assert_eq!(
            serde_json::to_value(context.barrier.pending().unwrap()).unwrap(),
            *event
        );
    }
    if let Some(expected) = &stopped_raw {
        assert_raw(
            exit.0
                .test_gate()
                .unwrap()
                .test_replies()
                .1
                .unwrap()
                .raw_bytes(),
            expected,
        );
    }
    if recorded {
        assert!(exit.0.test_context().is_none());
        assert_eq!(exit.0.unload_sequence(), sequence);
    } else {
        assert_eq!(exit.0.failure(), Some(&first));
        assert_eq!(exit.0.deadline(), Some(expected_cap));
        assert_eq!(
            exit.0
                .test_context()
                .unwrap()
                .deadlines
                .get(PumpDeadline::Leave),
            leave
        );
        // The first call makes the reserved fault attempt while retaining the
        // exact gate state; no Lower or sweep can run ahead of that attempt.
        let prior = lower_evidence(exit.0.test_gate().unwrap());
        let disposition = exit.0.test_gate().unwrap().test_disposition();
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
        assert_eq!(lower_evidence(exit.0.test_gate().unwrap()), prior);
        assert_eq!(exit.0.test_gate().unwrap().test_disposition(), disposition);
        if let Some((ticket, event)) = &pending_author {
            let context = exit.0.test_context().unwrap();
            assert_eq!(context.barrier.pending_author().unwrap().ticket, *ticket);
            assert_eq!(
                serde_json::to_value(context.barrier.pending().unwrap()).unwrap(),
                *event
            );
            assert_raw(
                exit.0.test_gate().unwrap().test_pending_frame().unwrap(),
                pending_raw.as_ref().unwrap(),
            );
        }
        if before {
            assert!(!root.join("gate-lower-received").exists());
            assert!(!exit.0.test_gate().unwrap().test_lower().unwrap().1);
        }
    }
    // The reserved fault attempt above must not consume the pending ordinary
    // owner. Only now make capacity available to bounded trace-only servicing.
    let trace_reader = pending_author.as_ref().map(|_| {
        use std::io::BufRead;
        let input = reader.try_clone().unwrap();
        input
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        std::thread::spawn(move || {
            let mut rows = vec![];
            for line in std::io::BufReader::new(input).lines() {
                let row: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
                let terminal = row["kind"] == "unload";
                rows.push(row);
                if terminal {
                    return rows;
                }
            }
            panic!("failed gate pressure trace ended before Unload");
        })
    });
    if let Some((ticket, _)) = &pending_author {
        // Observe real writer progress before advancing accounts, so the
        // original ticket's drain is independent of thread scheduling speed.
        while exit
            .0
            .test_context()
            .unwrap()
            .barrier
            .recorder()
            .boundary()
            .queued
            >= weaver_trace::serving::ORDINARY_CAPACITY
        {
            assert!(
                Instant::now() < until,
                "the actual trace sink did not free capacity"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            exit.0
                .test_context()
                .unwrap()
                .barrier
                .pending_author()
                .unwrap()
                .ticket,
            *ticket
        );
    }
    if cap {
        clock.advance_to(expected_cap).unwrap();
        assert_eq!(
            exit.0.advance_with_io(&mut CleanupIo(clock)),
            ExitProgress::Closed
        );
        assert!(exit.0.abandoned());
        assert!(exit.0.unload_sequence().is_none());
        assert!(exit.0.test_gate().unwrap().swept());
        assert!(exit.0.test_gate().unwrap().test_retired());
        assert!(exit.0.test_gate_resources().unwrap().port.is_none());
        assert_eq!(exit.0.failure(), Some(&first));
        assert_eq!(member.requests, requests);
        assert!(packet(peer).is_none());
        let disposition = exit.0.test_gate().unwrap().test_disposition();
        assert_eq!(
            exit.0.advance_with_io(&mut CleanupIo(clock)),
            ExitProgress::Closed
        );
        assert_eq!(exit.0.test_gate().unwrap().test_disposition(), disposition);
        return;
    }
    if held {
        let lower = original_lower.unwrap();
        assert!(lower.deadline_ns() < expected_cap);
        clock.advance_to(lower.deadline_ns() - 1).unwrap();
        assert_eq!(
            exit.0.advance_with_io(&mut CleanupIo(clock)),
            ExitProgress::Pending
        );
        assert_eq!(child(&exit.0), ChildState::Running);
        assert!(exit.0.unload_sequence().is_none());
        assert_eq!(member.requests, requests);
        assert_eq!(
            exit.0.wait_with_io(&mut CleanupIo(clock)),
            ExitProgress::Pending
        );
        assert_eq!(
            clock.waits().last().unwrap().absolute_deadline,
            Some(lower.deadline_ns())
        );
        assert_eq!(clock.now().unwrap(), lower.deadline_ns());
        assert_eq!(child(&exit.0), ChildState::Running);
    } else if pending {
        clock
            .advance_to(original_lower.unwrap().deadline_ns())
            .unwrap();
    }
    loop {
        assert!(Instant::now() < until, "failed gate cleanup did not finish");
        if !recorded {
            assert_eq!(exit.0.failure(), Some(&first));
            assert_eq!(exit.0.deadline(), Some(expected_cap));
            assert_eq!(
                exit.0
                    .test_context()
                    .unwrap()
                    .deadlines
                    .get(PumpDeadline::Leave),
                leave
            );
        }
        if let Some((ticket, _)) = &pending_author {
            let context = exit.0.test_context().unwrap();
            if let Some(pending) = context.barrier.pending_author() {
                assert_eq!(pending.ticket, *ticket);
            } else {
                let sequence = context
                    .barrier
                    .last_trace_only_sequence()
                    .expect("the original ticket Recorded through trace-only servicing");
                assert_eq!(
                    context.barrier.recorded_author_ticket(sequence),
                    Some(*ticket)
                );
            }
            assert_raw(
                exit.0.test_gate().unwrap().test_pending_frame().unwrap(),
                pending_raw.as_ref().unwrap(),
            );
            assert!(
                exit.0.test_gate().unwrap().test_recorded().is_none(),
                "the ordinary gate author is never resumed after halt"
            );
        }
        if !exit.0.test_gate().unwrap().test_disposition().1 {
            assert!(exit.0.unload_sequence().is_none());
            assert_eq!(member.requests, requests);
        }
        let progress = exit.0.advance_with_io(&mut CleanupIo(clock));
        if progress == ExitProgress::Closed {
            break;
        }
        if progress == ExitProgress::Pending {
            exit.0.wait_with_io(&mut CleanupIo(clock));
        }
        assert!(packet(peer).is_none(), "failed gate cleanup grants no Left");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!exit.0.abandoned());
    assert!(exit.0.test_gate().unwrap().swept());
    assert!(exit.0.test_gate().unwrap().test_retired());
    assert!(matches!(child(&exit.0), ChildState::Reaped(_)));
    if held {
        assert!(matches!(
            child(&exit.0),
            ChildState::Reaped(nix::sys::wait::WaitStatus::Signaled(
                _,
                nix::sys::signal::Signal::SIGKILL,
                _
            ))
        ));
        assert!(!root.join("gate-exit").exists());
        assert!(clock.now().unwrap() < expected_cap);
    }
    if let Some(expected) = &pending_raw {
        assert_raw(
            exit.0.test_gate().unwrap().test_pending_frame().unwrap(),
            expected,
        );
        assert!(exit.0.test_gate().unwrap().test_recorded().is_none());
    }
    if let Some(expected) = &stopped_raw {
        assert_raw(
            exit.0
                .test_gate()
                .unwrap()
                .test_replies()
                .1
                .unwrap()
                .raw_bytes(),
            expected,
        );
    }
    assert_raw(
        exit.0
            .test_original_gate()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .raw_bytes(),
        &raise_reply,
    );
    let lower = lower_evidence(exit.0.test_gate().unwrap());
    assert!(lower.1);
    assert_eq!(lower.0.exchange.ordinal, raise.exchange.ordinal + 1);
    assert_eq!(
        lower.0.payload,
        Payload::Directive(LifecycleDirective::Lower)
    );
    assert_eq!(
        serde_json::from_slice::<OrganEnvelope>(
            &std::fs::read(root.join("gate-lower-received")).unwrap()
        )
        .unwrap(),
        lower.0
    );
    if let Some(expected) = prior_lower {
        assert_eq!(lower, expected);
    }
    assert!(packet(peer).is_none());
    if recorded || release_failure {
        let mut expected = requests.to_vec();
        expected.push("grants".into());
        assert_eq!(member.requests, expected);
    } else {
        assert_eq!(member.requests, requests);
    }
    let terminal = exit.0.unload_sequence().unwrap();
    let rows = if let Some(reader) = trace_reader {
        reader.join().unwrap()
    } else {
        reader.set_nonblocking(true).unwrap();
        super::super::super::tests::trace_through(
            reader,
            vec![],
            weaver_trace::Sequence(terminal),
            until,
        )
        .unwrap()
    };
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    assert_eq!(rows.last().unwrap()["kind"], "unload");
    assert!(
        rows.iter()
            .all(|row| !matches!(row["kind"].as_str(), Some("turn.started" | "save_point")))
    );
    assert_eq!(rows.iter().any(|row| row["kind"] == "fault"), !recorded);
    assert_eq!(
        rows.last().unwrap()["payload"]["release"]["spu"],
        if release_failure {
            "unconfirmed"
        } else {
            "confirmed"
        }
    );
    if recorded {
        assert_eq!(Some(terminal), sequence);
    } else {
        assert_eq!(
            rows.last().unwrap()["payload"]["end_reason"],
            "component_failure"
        );
    }
    if pending {
        let refusal = serde_json::to_value(weaver_types::RefusalRecord::Lifecycle {
            asked: weaver_types::LifecycleAsk::Leave,
            refusal: LifecycleRefusal::Unloading,
        })
        .unwrap();
        let refusals = rows
            .iter()
            .filter(|row| row["kind"] == "refusal" && row["payload"] == refusal)
            .collect::<Vec<_>>();
        if let Some((ticket, mut event)) = pending_author {
            assert_eq!(
                refusals.len(),
                1,
                "only the original ordinary ticket can drain"
            );
            let context = exit.0.test_context().unwrap();
            assert!(context.barrier.pending_author().is_none());
            let sequence = context
                .barrier
                .last_trace_only_sequence()
                .expect("actual trace-only Recorded proof");
            assert_eq!(
                context.barrier.recorded_author_ticket(sequence),
                Some(ticket)
            );
            event["sequence"] = serde_json::to_value(sequence).unwrap();
            assert_eq!(*refusals[0], event);
            let fault = rows.iter().find(|row| row["kind"] == "fault").unwrap();
            assert_eq!(
                fault["payload"]["account"]["pending_author"]["ticket"],
                ticket.to_string()
            );
            assert_eq!(rows.last().unwrap()["sequence"], terminal.to_string());
        } else {
            assert!(refusals.is_empty());
        }
    }
    assert_eq!(
        exit.0.advance_with_io(&mut CleanupIo(clock)),
        ExitProgress::Closed
    );
    assert_eq!(exit.0.unload_sequence(), Some(terminal));
    assert!(packet(peer).is_none());
}
