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

struct Guard(Option<Box<FoundationExpiredRollback>>);
impl Drop for Guard {
    fn drop(&mut self) {
        let Some(owner) = self.0.as_mut() else {
            return;
        };
        if let Some(child) = owner
            .handoff
            .gate
            .as_mut()
            .and_then(|arms| arms.child.as_mut())
        {
            reap(child);
        }
        owner.run.resources.reap_startup_test_children();
        owner.run.context.barrier.abandon();
    }
}
struct ExitGuard(Box<super::failure::FoundationExpiredRollbackExit>);
impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.reap_composition_test_children();
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
    let suffix = suffix.split("-release-").next().unwrap();
    let suffix = suffix.strip_suffix("-cap").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-trace").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-complete").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-record").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-pending").unwrap_or(suffix);
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
    if mode == "expired-startup-admit-pending-complete" {
        let EnterState::Residency(owner) = guard.0.as_mut().unwrap().state.as_mut().unwrap() else {
            panic!("actual Admit owner");
        };
        owner.failure_test_stage_admit();
    }
    if mode == "expired-startup-open-unsent-complete" {
        let EnterState::Boot(owner) = guard.0.as_mut().unwrap().state.as_mut().unwrap() else {
            panic!("actual Open owner");
        };
        owner.failure_test_stage_open(clock);
    }
    if mode == "expired-startup-open-record-complete" {
        super::super::failed::tests::fill_trace(context(guard.0.as_mut().unwrap()), clock);
        while !context(guard.0.as_mut().unwrap()).boot_owns_pending_author() {
            assert!(
                Instant::now() < limit,
                "Open placement did not reach writer pressure"
            );
            // SAFETY: all original child owners remain in the fixture guard.
            assert_eq!(
                unsafe {
                    guard
                        .0
                        .as_mut()
                        .unwrap()
                        .advance_with_io(&mut CircuitIo(clock))
                },
                FoundationEnterProgress::Pending
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(context(guard.0.as_mut().unwrap()).boot_open_raw().is_some());
    }
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
    if mode.ends_with("-release-early") {
        let until = Instant::now() + Duration::from_secs(2);
        while !path.parent().unwrap().join("release-early").exists() {
            assert!(Instant::now() < until, "early Release fixture did not send");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
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
    let mut guard = Guard(Some(run.expired_rollback(handoff).ok().expect("exact run")));
    let owner = guard.0.as_mut().unwrap();
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
    let open_request = owner.run.context.boot_open_request().cloned();
    let open_raw = owner.run.context.boot_open_raw().map(<[u8]>::to_vec);
    let pending_open = owner.run.context.boot_owns_pending_author().then(|| {
        (
            serde_json::to_vec(owner.run.context.barrier.pending().unwrap()).unwrap(),
            owner.run.context.barrier.pending_author().unwrap().ticket,
        )
    });
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
    let original_enter = owner.handoff.identity.enter;
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
    if mode.ends_with("-classifying") {
        assert!(owner.run.resources.classifier_child().is_some());
        for _ in 0..6 {
            assert_eq!(
                owner.advance_with_io(&mut CircuitIo(clock)),
                FoundationExpiredRollbackProgress::Accepted
            );
            assert!(owner.run.context.failure.is_none());
            assert!(owner.run.context.flow.halt().is_none());
            assert_eq!(
                owner.run.context.deadlines.get(PumpDeadline::Enter),
                Some(owner.handoff.identity.enter)
            );
            member.step(clock.now().unwrap(), "ready");
            assert_eq!(
                member.requests, requests,
                "unfinished classifier cannot start Grants or release"
            );
        }
    }
    if mode == "expired-startup-admit-sent-complete" {
        assert!(
            matches!(owner.handoff.held.as_ref(), HeldStartup::Residency(held) if held.reply.is_none())
        );
        std::fs::write(path.parent().unwrap().join("allow-admit-reply"), b"allowed").unwrap();
    }
    if mode.starts_with("expired-startup-open-") {
        let enter = owner.handoff.identity.enter;
        let context = &owner.run.context;
        let decode = owner.run.resources.decode_present_for_serving().unwrap();
        for (origin, enter, leave) in [
            (Arc::new(()), enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(enter.began_ns() + 1, enter.deadline_ns()).unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            let permit = ExpiredBootAccountPermit {
                origin,
                enter,
                leave,
            };
            assert!(
                !context.can_detach_expired_boot_open(&permit, decode),
                "foreign rollback cannot detach the boot account"
            );
        }
        let permit = ExpiredBootAccountPermit {
            origin: Arc::clone(&context.read_origin),
            enter,
            leave,
        };
        assert!(context.can_detach_expired_boot_open(&permit, decode));
        let (foreign, _peer) = crate::channel::DecodeChannel::pair().unwrap();
        let foreign = DecodePort::from_channel(foreign).unwrap();
        assert!(
            !context.can_detach_expired_boot_open(&permit, &foreign),
            "retain the exact decode port"
        );
        assert!(context.failure.is_none() && context.flow.halt().is_none());
    }
    if mode.ends_with("-complete")
        || mode.contains("-release-")
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
        let mut release_spu = !mode.ends_with("-release-held");
        let unsent_open = mode == "expired-startup-open-unsent-complete";
        let incomplete_open = mode.starts_with("expired-startup-open-") && !unsent_open;
        // Every child is already forked. Drain the real writer concurrently:
        // close_after_drain is itself a blocking operation under its bound.
        let trace_reader = pending_open.is_some().then(|| {
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
                panic!("pressure fixture reached EOF before Unload");
            })
        });
        let invalid_release = [
            "-release-wrong",
            "-release-arm",
            "-release-position",
            "-release-early",
        ]
        .iter()
        .any(|suffix| mode.ends_with(suffix));
        loop {
            assert!(
                Instant::now() < limit,
                "classifier rollback: {:?}",
                owner.detail()
            );
            let progress = owner.advance_with_io(&mut CleanupIo(clock));
            if invalid_release && progress == FoundationExpiredRollbackProgress::Stopped {
                assert!(owner.run.context.flow.halt().is_some());
                assert!(owner.unload_sequence().is_none());
                assert!(packet(&peer).is_none(), "invalid Release granted Left");
                let release = owner.active.as_ref().unwrap().release.as_ref().unwrap();
                let reply = release
                    .received_reply()
                    .expect("retain actual invalid reply");
                assert!(release.outcome().is_none());
                assert!(
                    matches!(
                        owner.run.context.failure.as_ref(),
                        Some(RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            ..
                        })
                    ),
                    "{:?}",
                    owner.run.context.failure
                );
                if mode.ends_with("-early") {
                    assert!(!release.release_sent(), "reply preceded every Release byte");
                    assert!(matches!(
                        reply.payload,
                        Payload::Answer(LifecycleAnswer::Released)
                    ));
                } else if mode.ends_with("-arm") {
                    assert!(matches!(
                        reply.payload,
                        Payload::Answer(LifecycleAnswer::Admitted)
                    ));
                } else if mode.ends_with("-position") {
                    assert_eq!(reply.position, Position::Open);
                } else {
                    assert_eq!(reply.exchange.ordinal, 3);
                }
                let first = owner.run.context.failure.clone().unwrap();
                let leave = owner.leave.unwrap();
                let cause = owner.original.as_ref().unwrap().payload().cause.uid;
                let mut exit = ExitGuard(
                    guard
                        .0
                        .take()
                        .unwrap()
                        .failed_exit()
                        .ok()
                        .expect("actual stopped rollback"),
                );
                use super::super::rollback::failed::FoundationRollbackExitProgress as ExitProgress;
                loop {
                    assert!(
                        Instant::now() < limit,
                        "failed expired rollback did not close"
                    );
                    assert_eq!(exit.0.failure(), Some(&first));
                    assert!(exit.0.deadline().unwrap() <= leave.deadline_ns());
                    if exit.0.advance_with_io(&mut CleanupIo(clock)) == ExitProgress::Closed {
                        break;
                    }
                    if exit.0.progress() == ExitProgress::Pending {
                        exit.0.wait_with_io(&mut CleanupIo(clock));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(!exit.0.abandoned());
                if incomplete_open {
                    let account = exit
                        .0
                        .test_open()
                        .expect("failed exit retains Open account");
                    assert!(account.incomplete());
                    assert_eq!(account.request(), open_request.as_ref().unwrap());
                    assert_eq!(account.test_bound(), original_enter.deadline_ns());
                }
                assert_eq!(exit.0.channel_fault(), Some(crate::ChannelFault::Closed));
                assert!(matches!(
                    exit.0.test_spu_child(),
                    Some(
                        crate::flow::context::failed_enter_driver::ChildState::Reaped(
                            nix::sys::wait::WaitStatus::Exited(_, 0)
                        )
                    )
                ));
                assert!(packet(&peer).is_none(), "failed exit cannot grant Left");
                let sequence = exit.0.unload_sequence().expect("bounded failure Unload");
                reader.set_nonblocking(true).unwrap();
                let rows = super::super::tests::trace_through(
                    reader,
                    vec![],
                    weaver_trace::Sequence(sequence),
                    limit,
                )
                .unwrap();
                assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
                assert!(rows.iter().any(|row| row["kind"] == "fault"));
                let terminal = rows.last().unwrap();
                assert_eq!(terminal["kind"], "unload");
                assert_eq!(terminal["payload"]["cause"]["uid"], cause);
                assert_eq!(terminal["payload"]["release"]["spu"], "unconfirmed");
                assert_eq!(terminal["payload"]["end_reason"], "component_failure");
                return;
            }
            assert_ne!(
                progress,
                FoundationExpiredRollbackProgress::Stopped,
                "{:?}",
                owner.detail()
            );
            if progress == FoundationExpiredRollbackProgress::Closed {
                assert!(
                    release_child && release_spu && !invalid_release,
                    "Left cannot precede the actual classifier reap"
                );
                break;
            }
            if mode.ends_with("-release-recorded") && owner.unload_sequence().is_some() {
                struct BrokenOnce<'a> {
                    clock: &'a mut FakePumpIo,
                    broken: bool,
                }
                impl PumpIo for BrokenOnce<'_> {
                    fn now(&mut self) -> std::io::Result<u64> {
                        if !self.broken {
                            self.broken = true;
                            Err(std::io::Error::other("post-Unload clock failed"))
                        } else {
                            self.clock.now()
                        }
                    }
                    fn wait(
                        &mut self,
                        _: &[WaitInterest<'_>],
                        _: Option<u64>,
                    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                        panic!("post-Unload clock failure cannot wait")
                    }
                }
                let sequence = owner.unload_sequence().unwrap();
                assert_eq!(
                    owner.advance_with_io(&mut BrokenOnce {
                        clock,
                        broken: false
                    }),
                    FoundationExpiredRollbackProgress::Stopped
                );
                let mut exit = ExitGuard(
                    guard
                        .0
                        .take()
                        .unwrap()
                        .failed_exit()
                        .ok()
                        .expect("actual Recorded terminal"),
                );
                use super::super::rollback::failed::FoundationRollbackExitProgress as ExitProgress;
                loop {
                    assert!(Instant::now() < limit);
                    assert_eq!(exit.0.unload_sequence(), Some(sequence));
                    if exit.0.advance_with_io(&mut CleanupIo(clock)) == ExitProgress::Closed {
                        break;
                    }
                    exit.0.wait_with_io(&mut CleanupIo(clock));
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(!exit.0.abandoned());
                assert!(packet(&peer).is_none());
                reader.set_nonblocking(true).unwrap();
                let rows = super::super::tests::trace_through(
                    reader,
                    vec![],
                    weaver_trace::Sequence(sequence),
                    limit,
                )
                .unwrap();
                assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
                assert!(!rows.iter().any(|row| row["kind"] == "fault"));
                let terminal = rows.last().unwrap();
                assert_eq!(terminal["payload"]["release"]["spu"], "confirmed");
                assert!(
                    terminal["payload"]
                        .get("end_reason")
                        .is_none_or(serde_json::Value::is_null)
                );
                return;
            }
            if mode.ends_with("-release-silent")
                && path.parent().unwrap().join("release-received").exists()
            {
                let leave = owner.leave.unwrap();
                assert!(clock.now().unwrap() < leave.deadline_ns());
                // No ready descriptor: the actual release wait must consume the
                // original Leave, then retain and abandon without a terminal.
                assert_eq!(
                    owner.advance_with_io(clock),
                    FoundationExpiredRollbackProgress::Closed
                );
                assert_eq!(clock.now().unwrap(), leave.deadline_ns());
                assert!(owner.abandoned());
                assert!(owner.run.context.failure.is_none());
                assert!(owner.unload_sequence().is_none());
                assert!(packet(&peer).is_none());
                assert!(
                    owner
                        .active
                        .as_ref()
                        .unwrap()
                        .release
                        .as_ref()
                        .unwrap()
                        .outcome()
                        .is_none()
                );
                return;
            }
            if !release_spu && owner.unload_sequence().is_some() {
                assert!(path.parent().unwrap().join("release-replied").exists());
                assert!(matches!(
                    owner.run.resources.spu_child(),
                    Some(crate::flow::context::failed_enter_driver::ChildState::Running)
                ));
                for _ in 0..4 {
                    assert_eq!(
                        owner.advance_with_io(&mut CleanupIo(clock)),
                        FoundationExpiredRollbackProgress::Accepted
                    );
                    assert!(packet(&peer).is_none(), "Left requires actual SPU reap");
                }
                std::fs::write(path.parent().unwrap().join("spu-exit"), b"exit").unwrap();
                release_spu = true;
            }
            if !release_child && path.parent().unwrap().join("classifier-retired").exists() {
                assert!(
                    !path.exists(),
                    "the lifecycle seal must precede classifier release"
                );
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
                    assert!(
                        owner.unload_sequence().is_none(),
                        "healthy Unload requires classifier reap first"
                    );
                    assert!(
                        owner.run.context.flow.standing_state().is_some(),
                        "healthy member release follows classifier reap"
                    );
                }
                let root = path.parent().unwrap();
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
        let spu = !mode.contains("-classifier-");
        if spu {
            assert!(matches!(
                owner.run.resources.spu_child(),
                Some(
                    crate::flow::context::failed_enter_driver::ChildState::Reaped(
                        nix::sys::wait::WaitStatus::Exited(_, 0)
                    )
                )
            ));
        } else {
            assert!(owner.run.resources.spu_child().is_none());
        }
        assert!(owner.run.context.flow.standing_state().is_none());
        assert!(owner.active.as_ref().unwrap().current.is_some());
        if mode == "expired-startup-admit-sent-complete" {
            let release = owner.active.as_ref().unwrap().release.as_ref().unwrap();
            let prior = release
                .prior_reply()
                .expect("retain actual late Admit reply");
            assert_eq!(prior.exchange.ordinal, 1);
            assert!(matches!(
                prior.payload,
                Payload::Answer(LifecycleAnswer::Admitted)
            ));
            assert_eq!(release.received_reply().unwrap().exchange.ordinal, 2);
        }
        let mut expected_requests = requests.clone();
        expected_requests.push("grants".into());
        assert_eq!(member.requests, expected_requests);
        member.assert_closed_without_request();
        if incomplete_open {
            let account = owner.active.as_ref().unwrap().open.as_ref().unwrap();
            assert!(
                account.incomplete(),
                "unanswered Open has no successful account"
            );
            assert_eq!(account.request(), open_request.as_ref().unwrap());
            assert_eq!(
                account.test_bound(),
                owner.handoff.identity.enter.deadline_ns()
            );
            if let Some((event, ticket)) = pending_open.as_ref() {
                let (retained, retained_ticket) = account
                    .test_record()
                    .expect("retain exact pending placement");
                assert_eq!(&serde_json::to_vec(retained).unwrap(), event);
                assert_eq!(&retained_ticket, ticket);
                assert_eq!(account.test_raw(), open_raw.as_deref());
            } else {
                assert!(path.parent().unwrap().join("open-unanswered").exists());
            }
            assert!(packet(&peer).is_none(), "incomplete Open cannot grant Left");
            assert!(owner.retired_dial.as_ref().unwrap()._send.is_none());
            assert!(
                !leave.expired(clock.now().unwrap()),
                "cleanup did not need the Leave cap"
            );
        } else {
            if unsent_open {
                let account = owner.active.as_ref().unwrap().open.as_ref().unwrap();
                assert!(
                    !account.incomplete(),
                    "unsent cancellation needs no Open answer"
                );
                assert_eq!(account.request(), open_request.as_ref().unwrap());
                assert_eq!(account.test_bound(), original_enter.deadline_ns());
                assert!(account.test_raw().is_none());
                assert!(account.test_record().is_none());
            }
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
        }
        let sequence = owner
            .unload_sequence()
            .expect("retained real Unload sequence");
        let rows = if let Some(trace_reader) = trace_reader {
            trace_reader.join().unwrap()
        } else {
            reader.set_nonblocking(true).unwrap();
            super::super::tests::trace_through(
                reader,
                vec![],
                weaver_trace::Sequence(sequence),
                limit,
            )
            .unwrap()
        };
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
        if spu {
            assert_eq!(
                terminal["payload"]["release"]["spu"],
                if mode == "expired-startup-admit-complete"
                    || mode == "expired-startup-admit-pending-complete"
                {
                    "unconfirmed"
                } else {
                    "confirmed"
                }
            );
        } else {
            assert!(terminal["payload"]["release"].get("spu").is_none());
        }
        assert!(rows.iter().all(|row| row["kind"] != "fault"
            && (row["kind"] != "refusal" || row["payload"].get("fixture").is_some())));
        if incomplete_open || unsent_open {
            assert!(rows.iter().all(|row| row["kind"] != "state.placement"));
        }
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
        owner.retired_listener.is_some() || owner.preflight.is_some() || owner.active.is_some()
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
        assert!(
            owner.run.context.boot_open_incomplete().is_some()
                || owner
                    .active
                    .as_ref()
                    .and_then(|active| active.open.as_ref())
                    .is_some_and(|open| open.incomplete())
        );
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
