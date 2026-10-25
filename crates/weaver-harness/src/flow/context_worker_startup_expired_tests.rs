use super::super::failed::tests::{context, reached};
use super::super::tests::{EnterGuard, Member, dial, packet, reap};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use nix::sys::socket::{MsgFlags, send};
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

struct Guard(Box<FoundationExpiredRollback>);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(child) = self
            .0
            .handoff
            .gate
            .as_mut()
            .and_then(|arms| arms.child.as_mut())
        {
            reap(child);
        }
        self.0.run.resources.reap_startup_test_children();
        self.0.run.context.barrier.abandon();
    }
}
struct PanicReader(UnixStream);
impl Drop for PanicReader {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ = self.0.shutdown(std::net::Shutdown::Both);
        }
    }
}

pub(in crate::flow::context::worker_startup) fn refusal(
    enter: FoundationEnter<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
) {
    let mut guard = EnterGuard(Some(enter));
    let limit = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < limit, "classifier refusal did not arrive");
        // SAFETY: the fixture retains its thread and reaps every actual child.
        if unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        } == FoundationEnterProgress::Stopped
        {
            break;
        }
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    let context = context(guard.0.as_mut().unwrap());
    assert!(context.failure().is_none());
    assert!(
        !context
            .deadlines
            .get(PumpDeadline::Enter)
            .unwrap()
            .expired(context.last_ns)
    );
    let expected = guard.0.as_ref().unwrap().detail().unwrap().to_owned();
    assert!(expected.contains("ClassifierRefused"));
    guard.0 = Some(
        guard
            .0
            .take()
            .unwrap()
            .expired_handoff()
            .err()
            .expect("an actual timely refusal is not expired Enter authority"),
    );
    assert_eq!(guard.0.as_ref().unwrap().detail(), Some(expected.as_str()));
}

pub(in crate::flow::context::worker_startup) fn enter(
    enter: FoundationEnter<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    mode: &str,
) -> Box<FoundationExpiredEnter> {
    let _reader = PanicReader(reader.try_clone().unwrap());
    let mut guard = EnterGuard(Some(enter));
    let limit = Instant::now() + Duration::from_secs(8);
    let suffix = mode.strip_prefix("expired-startup-").unwrap();
    let suffix = suffix.strip_suffix("-cap").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-trace").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-complete").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-varied").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-unreadable").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-held").unwrap_or(suffix);
    let target = format!("failed-startup-{suffix}");
    while !reached(guard.0.as_ref().unwrap(), &target) {
        assert!(
            Instant::now() < limit,
            "target {target}: {:?}",
            guard.0.as_ref().unwrap().detail()
        );
        // SAFETY: the original fixture guards reap all actual children.
        let progress = unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        };
        assert_eq!(
            progress,
            FoundationEnterProgress::Pending,
            "{:?}",
            guard.0.as_ref().unwrap().detail()
        );
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    guard.0 = Some(
        guard
            .0
            .take()
            .unwrap()
            .expired_handoff()
            .err()
            .expect("not expired"),
    );
    let enter = context(guard.0.as_mut().unwrap())
        .deadlines
        .get(PumpDeadline::Enter)
        .unwrap();
    let origin = Arc::clone(&context(guard.0.as_mut().unwrap()).read_origin);
    clock.advance_to(enter.deadline_ns()).unwrap();
    // SAFETY: as above. The real stage observes exact-deadline expiry itself.
    assert_eq!(
        unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        },
        FoundationEnterProgress::Stopped
    );
    assert!(
        context(guard.0.as_mut().unwrap()).failure().is_none(),
        "expiry is not failure"
    );
    assert!(context(guard.0.as_mut().unwrap()).flow.halt().is_none());
    let handoff = guard
        .0
        .take()
        .unwrap()
        .expired_handoff()
        .ok()
        .expect("actual expired startup");
    assert!(Arc::ptr_eq(&handoff.identity.origin, &origin));
    assert_eq!(handoff.identity.enter, enter);
    handoff
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    mut handoff: Box<FoundationExpiredEnter>,
    enter_peer: &OwnedFd,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    original: &[u8],
    mode: &str,
) {
    let _reader = PanicReader(reader.try_clone().unwrap());
    assert_eq!(handoff.held.original_enter().unwrap().wire(), original);
    let address = handoff.held.original_enter().unwrap().wire().as_ptr();
    assert!(path.exists());
    assert!(
        packet(enter_peer).is_none(),
        "expiry produces no lifecycle answer"
    );
    if mode.ends_with("-foreign") {
        let mut foreign =
            crate::flow::context::tests::Fixture::new(crate::flow::context::tests::config());
        let mut context =
            crate::flow::context::failed_enter_driver::tests::take_context(&mut foreign);
        let resources = context.failed_enter_resources().unwrap();
        let foreign = Box::new(FoundationRun {
            context,
            resources,
            listener: None,
            original: None,
        });
        let (foreign, returned) = foreign
            .expired_rollback(handoff)
            .err()
            .expect("foreign run");
        assert!(foreign.context.failure().is_none());
        drop(foreign);
        handoff = returned;
        assert_eq!(
            handoff.held.original_enter().unwrap().wire().as_ptr(),
            address
        );
    }
    let mut guard = Guard(run.expired_rollback(handoff).ok().expect("exact run"));
    let owner = &mut guard.0;
    assert_eq!(
        owner.progress(),
        FoundationExpiredRollbackProgress::AwaitRollback
    );
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::AwaitRollback
    );
    assert!(
        owner
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none()
    );
    let requests = member.requests.clone();
    if mode.ends_with("-wait") {
        let path = path.to_owned();
        // Startup has already forked its children. This thread owns only the
        // root-side dial which wakes the real blocking listener wait.
        let wake = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            dial(&path)
        });
        let began = Instant::now();
        assert_eq!(
            owner.wait_with_io(&mut MonotonicIo),
            FoundationExpiredRollbackProgress::AwaitRollback
        );
        assert!(
            began.elapsed() >= Duration::from_millis(50),
            "await-root spun instead of blocking"
        );
        let peer = wake.join().unwrap();
        clock.advance_to(owner.run.context.last_ns).unwrap();
        owner.advance_with_io(clock);
        let read = owner.dial.as_ref().unwrap().read;
        clock.advance_to(read.deadline_ns()).unwrap();
        owner.advance_with_io(clock);
        drop(peer);
        assert!(owner.dial.is_none());
        assert!(owner.leave.is_none());
        assert_eq!(member.requests, requests);
    }
    if mode.ends_with("-observe") {
        for directive in [
            LifecycleDirective::Observe,
            LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 99 },
            },
        ] {
            let peer = dial(path);
            let envelope = OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 18,
                },
                position: Position::Open,
                payload: Payload::Directive(directive),
            };
            send(
                peer.as_raw_fd(),
                &serde_json::to_vec(&envelope).unwrap(),
                MsgFlags::MSG_NOSIGNAL,
            )
            .unwrap();
            let mut answered = false;
            for _ in 0..8 {
                assert_eq!(
                    owner.advance_with_io(clock),
                    FoundationExpiredRollbackProgress::AwaitRollback
                );
                if let Some(answer) = packet(&peer) {
                    assert!(matches!(
                        answer.payload,
                        Payload::Answer(LifecycleAnswer::InTransition { .. })
                            | Payload::Refusal(LifecycleRefusal::OutOfOrder)
                    ));
                    assert_eq!(answer.exchange, envelope.exchange);
                    answered = true;
                    break;
                }
            }
            assert!(answered, "secondary lifecycle peer has an actual answer");
            assert!(owner.leave.is_none());
            assert!(
                owner
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Leave)
                    .is_none()
            );
        }
    }
    if mode.ends_with("-silent") {
        let peer = dial(path);
        owner.advance_with_io(clock);
        let read = owner.dial.as_ref().unwrap().read;
        clock.advance_to(read.deadline_ns()).unwrap();
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::AwaitRollback
        );
        assert!(owner.dial.is_none());
        assert!(owner.leave.is_none());
        drop(peer);
    }
    let peer = dial(path);
    let directive = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 21,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 321 },
            forced: true,
            rollback: true,
        }),
    };
    send(
        peer.as_raw_fd(),
        &serde_json::to_vec(&directive).unwrap(),
        MsgFlags::MSG_NOSIGNAL,
    )
    .unwrap();
    if owner.before_load {
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::AwaitRollback
        );
        let answer = packet(&peer).expect("before-load rollback has an actual response");
        assert_eq!(answer.exchange, directive.exchange);
        assert_eq!(answer.position, Position::Close);
        assert!(matches!(
            answer.payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        ));
        assert!(owner.leave.is_none());
        assert!(
            owner
                .run
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_none()
        );
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(owner.run.resources.spu_child().is_none());
        assert!(owner.run.resources.classifier_child().is_none());
        assert_eq!(
            owner.original.as_ref().unwrap().retained_wire().as_ptr(),
            address
        );
        assert_eq!(owner.original.as_ref().unwrap().retained_wire(), original);
        assert!(owner.retired.as_ref().unwrap().state.is_some());
        assert!(owner.run.context.flow.standing_state().is_none());
        let mut bytes = vec![];
        reader.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match reader.read_to_end(&mut bytes) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < until,
                        "unadmitted trace descriptor did not close"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("unadmitted trace: {error}"),
            }
        }
        assert!(bytes.is_empty(), "no record is manufactured before Load");
        return;
    }
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Accepted,
        "{:?}",
        owner.detail()
    );
    assert_eq!(owner.original.as_ref().unwrap().retained_wire(), original);
    assert_eq!(
        owner.original.as_ref().unwrap().retained_wire().as_ptr(),
        address
    );
    assert_eq!(
        owner
            .dial
            .as_ref()
            .unwrap()
            .message
            .as_ref()
            .unwrap()
            .envelope,
        directive
    );
    let leave = owner.leave.unwrap();
    assert_eq!(leave.began_ns(), clock.now().unwrap());
    assert_eq!(
        leave.deadline_ns(),
        leave.began_ns() + weaver_types::LEAVE_BOUND_MS * 1_000_000
    );
    assert_eq!(
        owner.run.context.deadlines.get(PumpDeadline::Leave),
        Some(leave)
    );
    assert_eq!(
        owner.run.context.deadlines.get(PumpDeadline::Enter),
        Some(owner.handoff.identity.enter)
    );
    assert!(owner.run.context.failure().is_none());
    assert!(owner.run.context.flow.halt().is_none());
    assert!(packet(enter_peer).is_none());
    assert!(
        packet(&peer).is_none(),
        "acceptance alone cannot grant Left"
    );
    member.step(clock.now().unwrap(), "ready");
    assert_eq!(
        member.requests, requests,
        "no new boot work during rollback admission"
    );
    if mode.ends_with("-classifier-complete")
        || mode.ends_with("-classifier-varied")
        || mode.ends_with("-classifier-unreadable")
        || mode.ends_with("-classifier-held")
    {
        // This cleanup waits for child exit with no socket descriptors. The
        // general circuit fake only supplies readiness for actual interests.
        struct CleanupIo<'a>(&'a mut FakePumpIo);
        impl PumpIo for CleanupIo<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                self.0.now()
            }
            fn wait(
                &mut self,
                interests: &[WaitInterest<'_>],
                bound: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                if interests.is_empty() {
                    self.0.wait(interests, bound)
                } else {
                    CircuitIo(self.0).wait(interests, bound)
                }
            }
        }
        let limit = Instant::now() + Duration::from_secs(4);
        let mut release_child = !mode.ends_with("-classifier-held");
        loop {
            assert!(
                Instant::now() < limit,
                "classifier rollback: {:?}",
                owner.detail()
            );
            let progress = owner.advance_with_io(&mut CleanupIo(clock));
            assert_ne!(
                progress,
                FoundationExpiredRollbackProgress::Stopped,
                "{:?}",
                owner.detail()
            );
            if progress == FoundationExpiredRollbackProgress::Closed {
                assert!(
                    release_child,
                    "Left cannot precede the actual classifier reap"
                );
                break;
            }
            if !release_child && owner.unload_sequence().is_some() {
                assert!(matches!(
                    owner.run.resources.classifier_child(),
                    Some(crate::flow::context::failed_enter_driver::ChildState::Running)
                ));
                for _ in 0..4 {
                    assert_eq!(
                        owner.advance_with_io(&mut CleanupIo(clock)),
                        FoundationExpiredRollbackProgress::Accepted
                    );
                    assert!(packet(&peer).is_none(), "no Left before the actual reap");
                }
                let root = path.parent().unwrap();
                let until = Instant::now() + Duration::from_secs(1);
                while !root.join("classifier-retired").exists() {
                    assert!(
                        Instant::now() < until,
                        "classifier did not observe its socket close"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                std::fs::write(root.join("classifier-exit"), b"exit").unwrap();
                release_child = true;
            }
            member.step(
                clock.now().unwrap(),
                if mode.ends_with("-unreadable") {
                    "rollback-grants-refused"
                } else if mode.ends_with("-varied") {
                    "rollback-varied"
                } else {
                    "rollback-ready"
                },
            );
            assert!(packet(&peer).is_none(), "Left preceded completed rollback");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!owner.abandoned());
        assert!(owner.run.context.failure.is_none() && owner.run.context.flow.halt().is_none());
        assert_eq!(
            owner.run.context.retired_enter,
            Some(owner.handoff.identity.enter)
        );
        assert!(matches!(
            owner.run.resources.classifier_child(),
            Some(crate::flow::context::failed_enter_driver::ChildState::Reaped(_))
        ));
        assert!(owner.run.resources.spu_child().is_none());
        assert!(owner.run.context.flow.standing_state().is_none());
        assert!(owner.classifier.as_ref().unwrap().current.is_some());
        assert_eq!(member.requests, ["open", "grants", "grants"]);
        member.assert_closed_without_request();
        let answer = packet(&peer).expect("actual Left after the terminal drain");
        assert_eq!(answer.exchange, directive.exchange);
        assert!(matches!(
            answer.payload,
            Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false
            })
        ));
        let sequence = owner
            .unload_sequence()
            .expect("retained real Unload sequence");
        reader.set_nonblocking(true).unwrap();
        let rows = super::super::tests::trace_through(
            reader,
            vec![],
            weaver_trace::Sequence(sequence),
            limit,
        )
        .unwrap();
        let terminal = rows.last().unwrap();
        assert_eq!(terminal["kind"], "unload");
        assert_eq!(
            terminal["payload"]["cause"]["uid"],
            owner.original.as_ref().unwrap().payload().cause.uid
        );
        assert_eq!(
            terminal["payload"]["grant_surface"],
            if mode.ends_with("-unreadable") {
                "unreadable"
            } else if mode.ends_with("-varied") {
                "varied"
            } else {
                "unchanged"
            }
        );
        assert_eq!(terminal["payload"]["release"]["member"], "closed");
        assert!(terminal["payload"]["release"].get("spu").is_none());
        assert!(
            rows.iter()
                .all(|row| !matches!(row["kind"].as_str(), Some("fault" | "refusal")))
        );
        assert!(!path.exists());
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::Closed
        );
        assert!(packet(&peer).is_none(), "no duplicate Left");
        return;
    }
    if mode.ends_with("-activation-trace") {
        let limit = Instant::now() + Duration::from_secs(3);
        while owner.unload_sequence().is_none() {
            assert!(
                Instant::now() < limit,
                "preflight terminal did not reach its actual trace admission"
            );
            assert_eq!(
                owner.advance_with_io(&mut CircuitIo(clock)),
                FoundationExpiredRollbackProgress::Accepted,
                "{:?}",
                owner.detail()
            );
            assert!(packet(&peer).is_none());
        }
        assert!(matches!(
            owner
                .run
                .context
                .barrier
                .drain_status(&mut owner.run.context.budget)
                .unwrap(),
            weaver_trace::serving::DrainStatus::Pending { .. }
        ));
        struct ExpireOnWait<'a>(&'a mut FakePumpIo, u64);
        impl PumpIo for ExpireOnWait<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                self.0.now()
            }
            fn wait(
                &mut self,
                _: &[WaitInterest<'_>],
                bound: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                assert_eq!(bound, Some(self.1), "drain retains the original Leave cap");
                self.0.advance_to(self.1).unwrap();
                Ok(crate::flow::clock::WaitOutcome::Timeout)
            }
        }
        assert_eq!(
            owner.advance_with_io(&mut ExpireOnWait(clock, leave.deadline_ns())),
            FoundationExpiredRollbackProgress::Closed,
            "{:?}",
            owner.detail()
        );
        assert!(owner.abandoned());
        assert_eq!(
            owner.unload_sequence(),
            Some(1),
            "the original Recorded terminal stays evidence"
        );
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(owner.retired_dial.as_ref().unwrap()._send.is_none());
        assert!(packet(&peer).is_none(), "no Left from a blocked drain");
        assert!(!path.exists());
        assert_eq!(member.requests, requests);
        // Permit the abandoned asynchronous writer to leave its in-flight
        // syscall. A queued receipt need not survive a forced process exit.
        let _ = reader.shutdown(std::net::Shutdown::Both);
        return;
    }
    if mode.ends_with("-activation") {
        let limit = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(
                Instant::now() < limit,
                "preflight rollback did not complete: {:?}",
                owner.detail()
            );
            let progress = owner.advance_with_io(&mut CircuitIo(clock));
            assert_ne!(
                progress,
                FoundationExpiredRollbackProgress::Stopped,
                "{:?}",
                owner.detail()
            );
            if progress == FoundationExpiredRollbackProgress::Closed {
                break;
            }
            assert!(packet(&peer).is_none(), "Left precedes completed rollback");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!owner.abandoned());
        assert_eq!(owner.run.context.phase, RecordingPhase::Closed);
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(owner.run.context.flow.unused_open().is_some());
        assert!(owner.run.context.flow.standing_state().is_none());
        assert_eq!(
            owner.run.context.retired_enter,
            Some(owner.handoff.identity.enter)
        );
        assert_eq!(
            owner.run.context.deadlines.get(PumpDeadline::Leave),
            Some(leave)
        );
        assert_eq!(owner.unload_sequence(), Some(1));
        let answer = packet(&peer).expect("actual Left follows Unload and drain");
        assert_eq!(answer.exchange, directive.exchange);
        assert!(matches!(
            answer.payload,
            Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false
            })
        ));
        assert_eq!(member.requests, requests);
        member.assert_closed_without_request();
        reader.set_nonblocking(true).unwrap();
        let rows =
            super::super::tests::trace_through(reader, vec![], weaver_trace::Sequence(1), limit)
                .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["load", "unload"]
        );
        let unload = &rows[1]["payload"];
        assert_eq!(
            unload["cause"]["uid"],
            owner.original.as_ref().unwrap().payload().cause.uid
        );
        assert_eq!(unload["forced"], true);
        assert_eq!(unload["grant_surface"], "unreadable");
        assert!(unload["release"].get("spu").is_none());
        assert_eq!(unload["release"]["member"], "closed");
        assert!(!path.exists());
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::Closed
        );
        assert!(packet(&peer).is_none(), "no repeated Left");
        assert_eq!(
            owner.original.as_ref().unwrap().retained_wire().as_ptr(),
            address
        );
        return;
    }
    assert!(!owner.abandoned());
    assert!(owner.channel_fault().is_none());
    let had_gate_port = owner
        .handoff
        .gate
        .as_ref()
        .is_some_and(|gate| gate.port.is_some());
    clock.advance_to(leave.deadline_ns() - 1).unwrap();
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Accepted
    );
    assert!(
        owner.retired.is_none(),
        "the cap is not early cleanup authority"
    );
    clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Closed
    );
    assert!(owner.abandoned());
    assert_eq!(owner.channel_fault(), Some(crate::ChannelFault::Closed));
    assert!(owner.run.context.failure().is_none());
    assert!(owner.run.context.flow.halt().is_none());
    assert_eq!(owner.run.context.phase, RecordingPhase::Closed);
    assert!(owner.run.context.flow.standing_state().is_none());
    assert!(
        owner.retired.as_ref().unwrap().state.is_some()
            || owner.run.context.flow.unused_open().is_some()
    );
    assert!(owner.run.resources.decode_present_for_serving().is_none());
    assert!(owner.run.resources.residency_port().is_none());
    assert!(!path.exists());
    assert!(owner.handoff.listener.is_none());
    assert!(
        owner.retired_listener.is_some() || owner.preflight.is_some() || owner.classifier.is_some()
    );
    let retained = owner.retired_dial.as_ref().unwrap();
    assert_eq!(retained._message.as_ref().unwrap().envelope, directive);
    assert!(
        retained._send.is_none(),
        "no fabricated Left at the cleanup cap"
    );
    assert_eq!(owner.original.as_ref().unwrap().retained_wire(), original);
    assert_eq!(
        owner.original.as_ref().unwrap().retained_wire().as_ptr(),
        address
    );
    assert!(owner.run.context.boot_open_account_settled());
    if mode.ends_with("-open") {
        assert!(owner.run.context.boot_open_incomplete().is_some());
    }
    if let Some(gate) = &owner.handoff.gate {
        assert!(gate.port.is_none());
    }
    assert_eq!(owner.retired_gate.is_some(), had_gate_port);
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Closed
    );
    assert_eq!(
        member.requests, requests,
        "disposal does not resume expired boot work"
    );
}
