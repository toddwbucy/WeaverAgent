//! Real worker Ready, original Leave, Lower and capture on actual sockets.
//! SQLite modes use the normal member and reread its image through typed queries.
//! Model/classifier and admin replies remain scripted, not qualified deployment.
use super::*;
use crate::flow::context::worker_startup::FoundationRun;
use crate::flow::pump::PumpDeadline;
use std::os::fd::OwnedFd;
use weaver_harness::{
    FoundationForcedLeave, FoundationForcedLeaveHandoff, FoundationForcedLeaveProgress,
    FoundationGracefulLeave, FoundationGracefulLeaveHandoff, FoundationGracefulLeaveProgress,
};
use weaver_types::state_flow::capture::*;

#[path = "context_worker_graceful_conversion_tests.rs"]
mod conversion;

#[path = "context_worker_quiesce_conversion_tests.rs"]
pub(super) mod quiesce_conversion;

#[path = "context_worker_wind_down_conversion_tests.rs"]
pub(super) mod wind_conversion;

#[path = "context_worker_capture_wait_tests.rs"]
mod capture_wait;

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-forced-save") || mode.contains("-session-graceful-save")
}

pub(in crate::flow::context::worker_startup) struct Handoff {
    actual: LeaveHandoff,
    peer: Option<OwnedFd>,
    socket: std::path::PathBuf,
    before: Vec<u8>,
    joins: Vec<JoinReply>,
    conversion: Option<conversion::Evidence>,
    quiesce: Option<quiesce_conversion::Evidence>,
    wind_down: Option<wind_conversion::Evidence>,
}

pub(in crate::flow::context::worker_startup) fn handoff(
    serving: FoundationServing<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    member: &mut Member,
    reader: &mut UnixStream,
) -> Handoff {
    if wind_conversion::is_mode(mode) {
        return wind_conversion::handoff(serving, socket, root, mode, member, reader);
    }
    if quiesce_conversion::is_mode(mode) {
        return quiesce_conversion::handoff(serving, socket, root, mode, member, reader);
    }
    if conversion::is_mode(mode) {
        return conversion::handoff(serving, socket, root, mode, member, reader);
    }
    if mode.contains("-session-graceful-save") {
        return graceful_handoff(serving, socket, root, mode, member, reader);
    }
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let leave = || {
        management(
            socket,
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 74 },
                forced: true,
                rollback: false,
            },
        )
    };
    let (peer, before) = if mode.contains("-sqlite-joint") {
        (None, joint_handoff(&mut guard, root, mode, member, reader))
    } else if mode.ends_with("-held-filing") {
        let done = AtomicBool::new(false);
        reader.set_nonblocking(true).unwrap();
        std::thread::scope(|scope| {
            let responder = scope.spawn(|| {
                let mut peer = None;
                let end = Instant::now() + Duration::from_secs(10);
                while !done.load(Ordering::SeqCst) && Instant::now() < end {
                    member.step_live(mode);
                    if member.normal_filing_held() && peer.is_none() {
                        assert!(!root.join("ready-rollback-lower.json").exists());
                        peer = Some(leave());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                peer.expect("actual committed filing receipt triggered Leave")
            });
            let recorder = scope.spawn(|| {
                let mut bytes = vec![];
                let end = Instant::now() + Duration::from_secs(10);
                while !done.load(Ordering::SeqCst) && Instant::now() < end {
                    let _ = reader.read_to_end(&mut bytes);
                    std::thread::sleep(Duration::from_millis(1));
                }
                let _ = reader.read_to_end(&mut bytes);
                bytes
            });
            let _stop = StopThreads(&done);
            let session = guard.0.as_mut().unwrap();
            drive(session, Progress::Accepted);
            session.start_turn(TurnKey("session-0".into())).unwrap();
            drive(session, Progress::DirectivePending);
            done.store(true, Ordering::SeqCst);
            (Some(responder.join().unwrap()), recorder.join().unwrap())
        })
    } else {
        let peer = leave();
        drive(guard.0.as_mut().unwrap(), Progress::DirectivePending);
        (Some(peer), vec![])
    };
    let management = match guard.0.take().unwrap().into_owner() {
        Err(owner) => owner,
        Ok(input) => {
            let _guard = Guard {
                input: Some(input),
                management: None,
            };
            panic!("actual forced Leave must retain management");
        }
    };
    if mode.ends_with("-held-filing") {
        assert!(member.normal_filing_held());
        assert_eq!(member.filings.len(), 1);
        assert!(!root.join("ready-rollback-lower.json").exists());
    }
    let management = match management.rollback_handoff() {
        Err(owner) => owner,
        Ok(_) => panic!("forced Leave cannot obtain a rollback handoff"),
    };
    let actual = match management.forced_leave_handoff() {
        Ok(actual) => actual,
        Err(owner) => {
            let detail = owner.detail().map(str::to_owned);
            let _guard = Guard {
                input: None,
                management: Some(owner),
            };
            panic!("forced Leave handoff refused: {detail:?}");
        }
    };
    if let Some(peer) = &peer {
        assert!(read_packet(peer).is_none());
    }
    Handoff {
        actual: LeaveHandoff::Forced(actual),
        peer,
        socket: socket.into(),
        before,
        joins: Vec::new(),
        conversion: None,
        quiesce: None,
        wind_down: None,
    }
}

fn graceful_handoff(
    serving: FoundationServing<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    member: &mut Member,
    reader: &mut UnixStream,
) -> Handoff {
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let peer = (!mode.contains("-sqlite-joint")).then(|| {
        management(
            socket,
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 74 },
                forced: false,
                rollback: false,
            },
        )
    });
    let mut before = if peer.is_none() {
        joint_handoff(&mut guard, root, mode, member, reader)
    } else {
        Vec::new()
    };
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let phase_trace = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = Vec::new();
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let session = guard.0.as_mut().unwrap();
        drive(session, Progress::DirectivePending);
        assert!(session.defer_graceful_leave());
        assert!(session.begin_graceful_quiesce());
        drive(session, Progress::Quiesced);
        assert!(!root.join("ready-rollback-lower.json").exists());
        std::fs::write(
            root.join("wind-measure-release"),
            b"real wind-down may measure",
        )
        .unwrap();
        session
            .begin_graceful_wind_down(TurnKey("wind-down-0".into()), 1)
            .unwrap();
        drive(session, Progress::WindDownComplete);
        if let Some(peer) = &peer {
            assert!(read_packet(peer).is_none());
        }
        assert!(!root.join("ready-rollback-lower.json").exists());
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    });
    before.extend(phase_trace);
    let mut input = guard.0.take().unwrap().into_owner().ok().unwrap();
    let mut joins = Vec::new();
    if mode.ends_with("join-handoff") {
        for (uid, ordinal) in [(75, 100), (76, 101)] {
            let peer = joining(socket, uid, ordinal);
            while input.progress() != FoundationInputProgress::ManagementPending {
                assert_ne!(input.progress(), FoundationInputProgress::Stopped);
                input.wait_for_wake();
            }
            let mut management = input.management().ok().unwrap();
            while management.progress() == weaver_harness::FoundationManagementProgress::Pending {
                management.advance();
            }
            assert_eq!(
                management.progress(),
                weaver_harness::FoundationManagementProgress::DirectivePending
            );
            input = management.join_graceful_leave().ok().unwrap();
            assert!(read_packet(&peer).is_none());
            joins.push(JoinReply {
                peer,
                ordinal,
                answer: None,
            });
        }
        let original = input.serving.graceful.as_ref().unwrap();
        assert!(original.forced);
        assert_eq!(original.forced_by, Some(weaver_types::Cause { uid: 75 }));
        assert_eq!(original.joins.len(), 2);
    }
    assert!(input.serving.owner.context.open_turn.is_none());
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none()
    );
    let actual = match input.graceful_leave_handoff() {
        Ok(handoff) => handoff,
        Err(input) => {
            let detail = input.detail();
            let _guard = Guard {
                input: Some(input),
                management: None,
            };
            panic!("settled original graceful Leave handoff refused: {detail:?}");
        }
    };
    assert_eq!(
        actual.inner.message.envelope.payload,
        Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 74 },
            forced: false,
            rollback: false,
        })
    );
    assert!(actual.inner.graceful_tail.is_some());
    if let Some(peer) = &peer {
        assert!(read_packet(peer).is_none());
    }
    Handoff {
        actual: LeaveHandoff::Graceful(actual),
        peer,
        socket: socket.into(),
        before,
        joins,
        conversion: None,
        quiesce: None,
        wind_down: None,
    }
}

fn joint_handoff(
    guard: &mut SessionGuard<'_>,
    root: &Path,
    mode: &str,
    member: &mut Member,
    reader: &mut UnixStream,
) -> Vec<u8> {
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(25);
            let mut ready = false;
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                if !ready && (!mode.ends_with("-held-filing") || member.normal_filing_held()) {
                    std::fs::write(root.join("ready.json.part"), b"{}").unwrap();
                    std::fs::rename(root.join("ready.json.part"), root.join("ready.json")).unwrap();
                    ready = true;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(ready, "joint trigger was never reached");
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            let end = Instant::now() + Duration::from_secs(25);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let session = guard.0.as_mut().unwrap();
        if mode.ends_with("-held-filing") {
            drive(session, Progress::Accepted);
            session.start_turn(TurnKey("session-0".into())).unwrap();
        }
        // The actual Session answers Observe, retires that connection, and
        // retains the following Leave. No fixture fabricates load facts.
        drive(session, Progress::DirectivePending);
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    })
}

enum LeaveHandoff {
    Forced(Box<FoundationForcedLeaveHandoff>),
    Graceful(Box<FoundationGracefulLeaveHandoff>),
}
enum LeaveOwner {
    Forced(Box<FoundationForcedLeave>),
    Graceful(Box<FoundationGracefulLeave>),
}
impl LeaveOwner {
    fn test_inner(
        &mut self,
    ) -> &mut crate::flow::context::worker_startup::entered_rollback::FoundationEnteredRollback
    {
        match self {
            Self::Forced(owner) => owner.test_inner(),
            Self::Graceful(owner) => owner.test_inner(),
        }
    }
    fn progress(&self) -> FoundationForcedLeaveProgress {
        match self {
            Self::Forced(owner) => owner.progress(),
            Self::Graceful(owner) => match owner.progress() {
                FoundationGracefulLeaveProgress::Progress => {
                    FoundationForcedLeaveProgress::Progress
                }
                FoundationGracefulLeaveProgress::Pending => FoundationForcedLeaveProgress::Pending,
                FoundationGracefulLeaveProgress::CaptureRefused => {
                    FoundationForcedLeaveProgress::CaptureRefused
                }
                FoundationGracefulLeaveProgress::Closed => FoundationForcedLeaveProgress::Closed,
            },
        }
    }
    fn advance(&mut self) -> FoundationForcedLeaveProgress {
        match self {
            Self::Forced(owner) => {
                owner.advance();
            }
            Self::Graceful(owner) => {
                owner.advance();
            }
        }
        self.progress()
    }
    fn wait_for_progress(&mut self) {
        match self {
            Self::Forced(owner) => {
                owner.wait_for_progress();
            }
            Self::Graceful(owner) => {
                owner.wait_for_progress();
            }
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Forced(owner) => owner.detail(),
            Self::Graceful(owner) => owner.detail(),
        }
    }
    fn unload_sequence(&self) -> Option<u64> {
        match self {
            Self::Forced(owner) => owner.unload_sequence(),
            Self::Graceful(owner) => owner.unload_sequence(),
        }
    }
    fn abandoned(&self) -> bool {
        match self {
            Self::Forced(owner) => owner.abandoned(),
            Self::Graceful(owner) => owner.abandoned(),
        }
    }
}
struct ForcedGuard(LeaveOwner);
impl Drop for ForcedGuard {
    fn drop(&mut self) {
        self.0.test_inner().test_reap();
    }
}

fn reply(peer: &OwnedFd, exchange: &weaver_types::ExchangeId, payload: CapturePayload) {
    let bytes = CaptureEnvelope {
        exchange: exchange.clone(),
        position: Position::Continue,
        payload,
    }
    .encode(CaptureSender::Admin)
    .unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
}

fn joining(socket: &Path, uid: u32, ordinal: u64) -> OwnedFd {
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::JoinLeave {
            cause: weaver_types::Cause { uid },
        }),
    };
    packet!(peer.as_raw_fd(), &request);
    peer
}

struct JoinReply {
    peer: OwnedFd,
    ordinal: u64,
    answer: Option<CaptureEnvelope>,
}

fn join_pair(socket: &Path) -> Vec<JoinReply> {
    [(75, 100), (76, 101)]
        .into_iter()
        .map(|(uid, ordinal)| JoinReply {
            peer: joining(socket, uid, ordinal),
            ordinal,
            answer: None,
        })
        .collect()
}

fn expected_forced(mode: &str) -> bool {
    !mode.contains("-session-graceful-save")
        || conversion::is_mode(mode)
        || quiesce_conversion::is_mode(mode)
        || wind_conversion::is_mode(mode)
        || mode.ends_with("join-before")
        || mode.ends_with("join-handoff")
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    handoff: Handoff,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
) {
    let Handoff {
        actual,
        peer,
        socket,
        before,
        mut joins,
        conversion,
        quiesce,
        wind_down,
    } = handoff;
    if let Some(evidence) = conversion {
        evidence.assert_run(&run);
    }
    if let Some(evidence) = &quiesce {
        evidence.assert_run(&run);
    }
    let mut quiesce_probe = quiesce.and_then(|evidence| evidence.probe);
    if let Some(evidence) = &wind_down {
        evidence.assert_run(&run);
    }
    let original_start = run.context.open_start.clone().unwrap();
    let original_run = run.context.barrier.run().to_string();
    let (leave, owner) = match actual {
        LeaveHandoff::Forced(actual) => {
            let leave = actual.inner.leave;
            let owner = match run.forced_leave(actual) {
                Ok(owner) => LeaveOwner::Forced(owner),
                Err((mut run, mut handoff)) => {
                    crate::flow::context::worker_startup::tests::reap(
                        handoff.inner.arms.child.as_mut().unwrap(),
                    );
                    run.resources.reap_startup_test_children();
                    panic!("original run refused its forced Leave handoff");
                }
            };
            (leave, owner)
        }
        LeaveHandoff::Graceful(actual) => {
            let leave = actual.inner.leave;
            let owner = match run.graceful_leave(actual) {
                Ok(owner) => LeaveOwner::Graceful(owner),
                Err((mut run, mut handoff)) => {
                    crate::flow::context::worker_startup::tests::reap(
                        handoff.inner.arms.child.as_mut().unwrap(),
                    );
                    run.resources.reap_startup_test_children();
                    panic!("original run refused its graceful Leave handoff");
                }
            };
            (leave, owner)
        }
    };
    let mut owner = ForcedGuard(owner);
    let mut capture_wait = capture_wait::Probe::new(mode, member);
    let Some(peer) = peer else {
        finish_joint(owner, member, reader, root, mode, before, leave);
        return;
    };
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let mut packets = Vec::new();
    let mut artifact = None;
    let mut report = None;
    let quiesce_bad_ack = quiesce_conversion::is_mode(mode)
        && (mode.ends_with("wrong-ack") || mode.ends_with("duplicate-ack"));
    let mut final_refusal = false;
    let mut secondary = Vec::new();
    let mut held_image = None;
    let mut departed = false;
    let bytes = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(30);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                if let Some(probe) = &quiesce_probe {
                    let (sent, withdrawn) = probe();
                    assert!(
                        !sent,
                        "the original blocked Quiesce packet must remain unsent"
                    );
                    if withdrawn {
                        std::fs::write(root.join("quiesce-convert-drain"), b"actual withdrawal")
                            .unwrap();
                        quiesce_probe.take();
                    }
                }
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(30);
            let mut bytes = before;
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let end = Instant::now() + Duration::from_secs(25);
        loop {
            assert!(
                Instant::now() < end,
                "forced Leave exceeded fixture watch: {:?}",
                owner.0.detail()
            );
            let progress = owner.0.advance();
            if mode.ends_with("tail-expired") && owner.0.test_inner().test_at_grants() {
                // The actual capture is complete. Expiry must transfer that
                // original account to failed cleanup, never silent disposal.
                let mut clock = FakePumpIo::new(leave.deadline_ns());
                let inner = owner.0.test_inner();
                inner.advance_with_io(&mut clock);
                assert!(
                    inner.test_failed().is_some(),
                    "expired capture must retain its failed owner"
                );
                assert!(inner.test_context().failure().is_some());
                assert!(inner.unload_sequence().is_none());
                assert!(report.is_none());
                break;
            }
            if mode.ends_with("join-sealed")
                && joins.is_empty()
                && owner.0.test_inner().test_at_grants()
            {
                // The capture is complete and S7 has fixed the outcome. Both
                // actual connections enter its backlog before pathname seal.
                assert!(artifact.is_some());
                assert!(report.is_none());
                joins = join_pair(&socket);
            }
            if let Some(bytes) = read_packet(&peer) {
                let message = CaptureEnvelope::decode(&bytes, CaptureSender::Harness).unwrap();
                assert_eq!(message.exchange.ordinal, 99);
                match &message.payload {
                    CapturePayload::SavePointBeginning(beginning) => {
                        assert!(packets.is_empty());
                        assert!(root.join("ready-rollback-lower.json").exists());
                        let inner = owner.0.test_inner();
                        let (_, account) = inner.test_account();
                        assert!(matches!(
                            account.arms.child.as_ref().unwrap().state(),
                            crate::flow::context::failed_enter_driver::ChildState::Reaped(_)
                        ));
                        assert_eq!(beginning.tail_window.unwrap().began_ns.0, leave.began_ns());
                        assert_eq!(
                            beginning.tail_window.unwrap().deadline_ns.0,
                            leave.deadline_ns()
                        );
                        assert!(beginning.window.began_ns.0 >= leave.began_ns());
                        assert!(
                            inner.test_context().capture_snapshot().is_none(),
                            "whole Beginning precedes barrier"
                        );
                    }
                    CapturePayload::SavePointImage(image) => {
                        let dump_sha256 = if mode.contains("-save-sqlite") {
                            let bytes = std::fs::read(
                                root.join("normal-member/room").join(&image.room_name),
                            )
                            .unwrap();
                            assert_eq!(bytes.len() as u64, image.size_bytes.0);
                            assert_eq!(&bytes[..16], b"SQLite format 3\0");
                            std::fs::write(root.join("retained-capture.sqlite"), &bytes).unwrap();
                            if let Some(path) = std::env::var_os("WEAVER_NORMAL_MEMBER_EVIDENCE") {
                                std::fs::create_dir_all(&path).unwrap();
                                std::fs::write(
                                    std::path::Path::new(&path).join("capture.sqlite"),
                                    &bytes,
                                )
                                .unwrap();
                            }
                            crate::flow::context::worker_startup::tests::normal_member::digest(
                                &bytes,
                            )
                        } else {
                            s::Digest::new("9".repeat(64)).unwrap()
                        };
                        let value = CaptureArtifact {
                            capture_id: image.capture_id,
                            artifact_id: format!("{:020}-{}", 2, "d".repeat(64)),
                            dump_sha256,
                            config_sha256: s::Digest::new("d".repeat(64)).unwrap(),
                        };
                        let checked = CapturePayload::SavePointImageChecked(CaptureImageChecked {
                            capture_id: image.capture_id,
                            dump_sha256: value.dump_sha256.clone(),
                        });
                        if mode.ends_with("join-before") {
                            joins = join_pair(&socket);
                            held_image = Some((message.exchange.clone(), checked));
                        } else if mode.ends_with("-secondary") {
                            secondary.push((
                                management(
                                    &socket,
                                    LifecycleDirective::JoinLeave {
                                        cause: weaver_types::Cause { uid: 75 },
                                    },
                                ),
                                Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder),
                                false,
                            ));
                            secondary.push((
                                management(&socket, LifecycleDirective::Observe),
                                Payload::Answer(LifecycleAnswer::InTransition {
                                    constituents: vec![],
                                }),
                                false,
                            ));
                            held_image = Some((message.exchange.clone(), checked));
                        } else {
                            reply(&peer, &message.exchange, checked);
                        }
                        artifact = Some(value);
                    }
                    CapturePayload::SavePointImageFinished(finished) => {
                        if mode.contains("-save-sqlite") {
                            let room = root.join("normal-member/room");
                            assert!(
                                !room
                                    .join(format!("capture-{}.sqlite.part", finished.capture_id.0))
                                    .exists()
                            );
                            assert_eq!(
                                std::fs::read(room.join(&finished.room_name)).unwrap(),
                                std::fs::read(root.join("retained-capture.sqlite")).unwrap(),
                            );
                        }
                        reply(
                            &peer,
                            &message.exchange,
                            CapturePayload::SavePointPrepared(artifact.as_ref().unwrap().clone()),
                        );
                    }
                    CapturePayload::SavePointCommit(commit) => {
                        assert_eq!(commit.capture_id, artifact.as_ref().unwrap().capture_id);
                        assert!(owner.0.unload_sequence().is_none());
                        let context = owner.0.test_inner().test_context();
                        assert_eq!(
                            context
                                .barrier
                                .recorder()
                                .structure()
                                .by_kind(weaver_trace::serving::Kind::SavePoint)
                                .count(),
                            2
                        );
                        let mut value = artifact.as_ref().unwrap().clone();
                        if mode.ends_with("-wrong-published") {
                            value.dump_sha256 = s::Digest::new("8".repeat(64)).unwrap();
                        }
                        if !mode.ends_with("-published-timeout") {
                            reply(
                                &peer,
                                &message.exchange,
                                CapturePayload::SavePointPublished(value),
                            );
                        }
                    }
                    CapturePayload::SavePointCaptureComplete(_) => {
                        if mode.contains("-save-sqlite") {
                            assert_eq!(
                                std::fs::read_dir(root.join("normal-member/room"))
                                    .unwrap()
                                    .count(),
                                0,
                                "CaptureComplete requires the actual member image to be removed"
                            );
                        }
                        assert!(
                            owner.0.unload_sequence().is_none(),
                            "Complete is not Unload or Left"
                        );
                        assert!(report.is_none());
                    }
                    CapturePayload::Answer(CaptureAnswer::Left {
                        save_point: Some(value),
                        forced,
                        no_state: false,
                    }) => {
                        assert!(matches!(
                            packets.last(),
                            Some(CapturePayload::SavePointCaptureComplete(_))
                        ));
                        assert!(owner.0.unload_sequence().is_some());
                        assert_eq!(*forced, expected_forced(mode));
                        report = Some(value.clone());
                    }
                    CapturePayload::Refusal(CaptureRefusal::Lifecycle(
                        weaver_types::LifecycleRefusal::Malformed,
                    )) if quiesce_bad_ack => {
                        assert!(matches!(
                            packets.last(),
                            Some(CapturePayload::SavePointCaptureComplete(_))
                        ));
                        assert!(owner.0.unload_sequence().is_some());
                        final_refusal = true;
                    }
                    other => panic!("unexpected forced capture packet: {other:?}"),
                }
                packets.push(message.payload);
            }
            if mode.ends_with("peer-after-capture")
                && !departed
                && owner.0.test_inner().test_at_grants()
            {
                assert!(matches!(
                    packets.last(),
                    Some(CapturePayload::SavePointCaptureComplete(_))
                ));
                nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both)
                    .unwrap();
                departed = true;
            }
            for (dial, expected, complete) in &mut secondary {
                if !*complete && let Some(bytes) = read_packet(dial) {
                    let answer: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(answer.position, Position::Close);
                    assert_eq!(answer.exchange.ordinal, 99);
                    assert_eq!(&answer.payload, expected);
                    *complete = true;
                }
            }
            for join in &mut joins {
                if join.answer.is_none()
                    && let Some(bytes) = read_packet(&join.peer)
                {
                    assert!(owner.0.unload_sequence().is_some());
                    assert!(
                        report.is_some() || final_refusal,
                        "original holder receives its final answer first"
                    );
                    let answer = CaptureEnvelope::decode(&bytes, CaptureSender::Harness).unwrap();
                    assert_eq!(answer.exchange.opener, Opener::Admin);
                    assert_eq!(answer.exchange.ordinal, join.ordinal);
                    assert_eq!(answer.position, Position::Close);
                    assert_eq!(Some(&answer.payload), packets.last());
                    join.answer = Some(answer);
                }
            }
            if held_image.is_some()
                && secondary.iter().all(|(_, _, complete)| *complete)
                && (!mode.ends_with("join-before") || owner.0.test_inner().test_join_count() == 2)
            {
                assert!(joins.iter().all(|join| join.answer.is_none()));
                let (exchange, image) = held_image.take().unwrap();
                reply(&peer, &exchange, image);
            }
            if matches!(
                progress,
                FoundationForcedLeaveProgress::Closed
                    | FoundationForcedLeaveProgress::CaptureRefused
            ) {
                break;
            }
            if progress == FoundationForcedLeaveProgress::Pending {
                if let Some(probe) = &mut capture_wait {
                    probe.wait(&mut owner.0);
                } else {
                    owner.0.wait_for_progress();
                }
            }
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap_or_else(|_| {
            let detail = owner.0.detail().map(str::to_owned);
            panic!(
                "member fixture failed: owner {:?}, context {:?}",
                detail,
                owner.0.test_inner().test_context().failure()
            );
        });
        recorder.join().unwrap()
    });
    if mode.ends_with("tail-expired") {
        // This probe proves the failed ownership transfer at the old deadline.
        // The fixture guard reaps its processes; no complete teardown claim.
        assert!(owner.0.test_inner().test_failed().is_some());
        return;
    }
    if mode.contains("-save-sqlite") {
        member.finish_normal();
        if let Some(path) = std::env::var_os("WEAVER_NORMAL_MEMBER_EVIDENCE") {
            std::fs::write(std::path::Path::new(&path).join("trace.jsonl"), &bytes).unwrap();
        }
    }
    let rows: Vec<serde_json::Value> =
        if owner.0.progress() == FoundationForcedLeaveProgress::CaptureRefused {
            // CaptureRefused proves Recorded admission, not writer completion.
            // Read through that actual sequence for explicit refusals and timeouts.
            trace_through(
                reader,
                bytes,
                owner
                    .0
                    .test_inner()
                    .test_context()
                    .last_author_sequence
                    .expect("capture refusal has its Recorded sequence"),
                Instant::now() + Duration::from_secs(3),
            )
            .expect("the capture refusal's Recorded trace must reach its reader")
        } else {
            bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect()
        };
    if let Some(probe) = &capture_wait {
        probe.assert_finished(&mut owner.0, member, &packets, &rows, report.as_ref());
        return;
    }
    if mode.ends_with("-refused") {
        assert_eq!(
            owner.0.progress(),
            FoundationForcedLeaveProgress::CaptureRefused
        );
        assert!(report.is_none());
        assert!(owner.0.unload_sequence().is_none());
        assert!(owner.0.test_inner().test_context().failure().is_none());
        assert!(
            owner
                .0
                .test_inner()
                .test_context()
                .capture_snapshot()
                .is_none()
        );
        assert!(
            member
                .requests
                .ends_with(&["snapshot".into(), "abort".into()])
        );
        assert!(
            rows.iter()
                .any(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "refused")
        );
    } else if mode.ends_with("-wrong-published") || mode.ends_with("peer-after-capture") {
        assert_eq!(owner.0.progress(), FoundationForcedLeaveProgress::Closed);
        assert!(report.is_none());
        assert!(owner.0.test_inner().test_context().failure().is_some());
        let halt = rows
            .iter()
            .find(|row| row["kind"] == "fault" && row["payload"]["account"]["capture"].is_object())
            .expect("actual halt account");
        assert_eq!(halt["payload"]["account"]["capture"]["commit_sent"], true);
        assert!(!owner.0.abandoned(), "{:?}", owner.0.detail());
        assert!(
            owner
                .0
                .test_inner()
                .test_context()
                .flow
                .standing_state()
                .is_none()
        );
        assert!(
            owner
                .0
                .test_inner()
                .test_context()
                .flow
                .standing_classifier()
                .is_none()
        );
        member.assert_closed_without_request();
        for child in [
            owner
                .0
                .test_inner()
                .test_failed()
                .unwrap()
                .resources()
                .classifier_child(),
            owner
                .0
                .test_inner()
                .test_failed()
                .unwrap()
                .resources()
                .spu_child(),
        ] {
            assert!(matches!(
                child,
                Some(
                    crate::flow::context::failed_enter_driver::ChildState::Reaped(
                        nix::sys::wait::WaitStatus::Exited(_, 0)
                    )
                )
            ));
        }

        assert_eq!(
            packets
                .iter()
                .any(|p| matches!(p, CapturePayload::SavePointCaptureComplete(_))),
            departed
        );
    } else if quiesce_bad_ack {
        assert!(final_refusal && report.is_none() && artifact.is_some());
        assert_eq!(owner.0.progress(), FoundationForcedLeaveProgress::Closed);
        assert!(owner.0.detail().is_none(), "{:?}", owner.0.detail());
        assert!(!owner.0.abandoned());
        assert_eq!(rows.last().unwrap()["kind"], "unload");
        assert!(member.requests.ends_with(&[
            "snapshot".into(),
            "acknowledge".into(),
            "release_capture".into(),
            "grants".into()
        ]));
        quiesce_conversion::assert_tail(&mut owner, &rows, &joins, &member.filings, root, mode);
    } else {
        assert_eq!(owner.0.progress(), FoundationForcedLeaveProgress::Closed);
        assert!(owner.0.detail().is_none(), "{:?}", owner.0.detail());
        assert!(!owner.0.abandoned());
        let report = report.expect("actual final Left");
        member.assert_normal_capture(&report);
        let artifact = artifact.unwrap();
        assert_eq!(report.start.artifact_id, artifact.artifact_id);
        assert_eq!(report.start.dump_sha256, artifact.dump_sha256);
        assert_eq!(report.start.config_sha256, artifact.config_sha256);
        assert_eq!(
            report.start.parent_artifact_id,
            Some(original_start.artifact_id)
        );
        assert_eq!(report.event_run.0, original_run);
        let confirmed = rows
            .iter()
            .find(|r| r["kind"] == "save_point" && r["payload"]["phase"] == "confirmed")
            .unwrap();
        assert_eq!(confirmed["sequence"], report.event_sequence.0.to_string());
        assert!(member.requests.ends_with(&[
            "snapshot".into(),
            "acknowledge".into(),
            "release_capture".into(),
            "grants".into()
        ]));
        let unload = rows.iter().find(|r| r["kind"] == "unload").unwrap();
        assert_eq!(unload["payload"]["cause"]["uid"], 74);
        assert_eq!(unload["payload"]["forced"], expected_forced(mode));
        if wind_conversion::is_mode(mode) {
            wind_conversion::assert_tail(&mut owner, &rows, &joins, &member.filings, root, mode);
        } else if quiesce_conversion::is_mode(mode) {
            quiesce_conversion::assert_tail(&mut owner, &rows, &joins, &member.filings, root, mode);
        } else if conversion::is_mode(mode) {
            conversion::assert_tail(&rows, &joins, &member.filings, root, mode);
        } else if mode.contains("-session-graceful-save") {
            assert_wind_down_filings(&member.filings);
            assert_eq!(
                unload["payload"]["forced_by"],
                if expected_forced(mode) {
                    serde_json::json!({"uid": 75})
                } else {
                    serde_json::Value::Null
                }
            );
            if mode.contains("-join-") {
                assert_eq!(joins.len(), 2);
                assert!(joins.iter().all(|join| join.answer.is_some()));
                assert!(held_image.is_none());
            }
            assert_eq!(
                rows.iter()
                    .filter(|r| r["kind"] == "turn.started" && r["turn"] == "wind-down-0")
                    .count(),
                1
            );
            let close = rows
                .iter()
                .find(|r| r["kind"] == "turn.closed" && r["turn"] == "wind-down-0")
                .unwrap();
            assert_eq!(close["payload"]["close"], "clean");
            let instruction = rows
                .iter()
                .find(|r| {
                    r["kind"] == "loop.instruction" && r["payload"]["template_id"] == "wind_down"
                })
                .unwrap();
            let sequence =
                |r: &serde_json::Value| r["sequence"].as_str().unwrap().parse::<u64>().unwrap();
            assert!(sequence(instruction) < sequence(close));
            assert!(sequence(close) < sequence(confirmed));
            assert!(!rows.iter().any(|r| r["kind"] == "message.user"));
            let lower: OrganEnvelope = serde_json::from_slice(
                &std::fs::read(root.join("ready-rollback-lower.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                lower.exchange.ordinal, 3,
                "Lower follows the actual Raise and Quiesce"
            );
        }
        assert_eq!(rows.last().unwrap()["kind"], "unload");
        assert!(owner.0.test_inner().test_context().member_capture.is_none());
        if mode.ends_with("-secondary") {
            assert_eq!(secondary.len(), 2);
            assert!(secondary.iter().all(|(_, _, complete)| *complete));
            assert!(held_image.is_none());
            assert!(unload["payload"]["forced_by"].is_null());
        }
    }
    assert_eq!(
        owner
            .0
            .test_inner()
            .test_context()
            .deadlines
            .get(PumpDeadline::Leave),
        Some(leave)
    );
}

pub(in crate::flow::context::worker_startup) fn assert_wind_down_filings(
    filings: &[s::StateFrame],
) {
    for text in ["Read summary.", "Read result."] {
        assert_eq!(
            filings
                .iter()
                .filter(|frame| matches!(
                    frame, s::StateFrame::File { filing, .. } if filing.text == text
                ))
                .count(),
            1,
            "capture must retain the wind-down instruction and its actual output: {text}"
        );
    }
}

fn finish_joint(
    mut owner: ForcedGuard,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
    before: Vec<u8>,
    leave: crate::flow::clock::AbsoluteBound,
) {
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let bytes = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(30);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = before;
            let end = Instant::now() + Duration::from_secs(30);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let end = Instant::now() + Duration::from_secs(25);
        loop {
            assert!(
                Instant::now() < end,
                "joint forced Leave exceeded watchdog: {:?}",
                owner.0.detail()
            );
            match owner.0.advance() {
                FoundationForcedLeaveProgress::Closed => break,
                FoundationForcedLeaveProgress::CaptureRefused => {
                    panic!("unexpected capture refusal: {:?}", owner.0.detail())
                }
                FoundationForcedLeaveProgress::Pending => {
                    owner.0.wait_for_progress();
                }
                FoundationForcedLeaveProgress::Progress => (),
            }
        }
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        recorder.join().unwrap()
    });
    if mode.ends_with("-bad-release") {
        assert!(owner.0.test_inner().test_context().failure().is_some());
        assert!(owner.0.unload_sequence().is_some());
        member.finish_normal();
        assert_eq!(member.requests.last().unwrap(), "release_capture");
        std::fs::write(root.join("worker-trace.jsonl"), bytes).unwrap();
        return;
    }
    assert!(owner.0.detail().is_none(), "{:?}", owner.0.detail());
    assert!(!owner.0.abandoned());
    assert!(owner.0.unload_sequence().is_some());
    assert_eq!(
        owner
            .0
            .test_inner()
            .test_context()
            .deadlines
            .get(PumpDeadline::Leave),
        Some(leave)
    );
    assert!(owner.0.test_inner().test_context().member_capture.is_none());
    member.finish_normal();
    assert!(member.requests.ends_with(&[
        "snapshot".into(),
        "acknowledge".into(),
        "release_capture".into(),
        "grants".into()
    ]));
    std::fs::write(root.join("worker-trace.jsonl"), bytes).unwrap();
    // The parent waits for the actual admin after this worker releases fd9 and
    // exits. Waiting here would prevent admin's real run-end from completing.
}
