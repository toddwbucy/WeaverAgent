//! Ordering refusals retain the actual entered seat and its accepted input.
//! Synthetic language and organ peers do not qualify production deployment.
use super::tests::{Guard, dial, read_packet};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::tests::Member;
use nix::sys::socket::{ControlMessage, MsgFlags, sendmsg};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::{Cause, ExchangeId};

/// This is the actual reexecuted worker process. The controller gets the three
/// fixture endpoints; no recorder thread or adopted run crosses a fork.
pub(in crate::flow::context::worker_startup) fn run(
    run: Box<crate::FoundationRun>,
    program: crate::FoundationProgram,
    member: Member,
    peer: std::os::fd::OwnedFd,
    reader: UnixStream,
    root: &Path,
) -> ! {
    use crate::flow::context::worker_startup::tests::worker;
    let (member_peer, seed) = member.dispatch_parts();
    std::fs::write(root.join("dispatch-member-seed"), seed).unwrap();
    let enter = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
    std::fs::write(
        root.join("dispatch-enter-deadline"),
        enter.deadline_ns().to_string(),
    )
    .unwrap();
    // This is the original start-step description at fixed fd9, retained
    // before adoption. No local File is substituted for that process custody.
    // SAFETY: the start-step fixture installed and retained this descriptor.
    let inherited = unsafe { std::os::fd::BorrowedFd::borrow_raw(crate::RUN_LOCK_DESCRIPTOR) };
    let original = std::fs::File::open(root.join("dispatch-run.lock")).unwrap();
    let inherited_stat = nix::sys::stat::fstat(inherited).unwrap();
    let original_stat = nix::sys::stat::fstat(&original).unwrap();
    assert_eq!(
        (inherited_stat.st_dev, inherited_stat.st_ino),
        (original_stat.st_dev, original_stat.st_ino)
    );
    drop(original);
    let control = UnixStream::connect(root.join("dispatch-control.sock")).unwrap();
    let descriptors = [
        peer.as_raw_fd(),
        reader.as_raw_fd(),
        member_peer.as_raw_fd(),
    ];
    assert_eq!(
        sendmsg::<()>(
            control.as_raw_fd(),
            &[std::io::IoSlice::new(b"!")],
            &[ControlMessage::ScmRights(&descriptors)],
            MsgFlags::MSG_NOSIGNAL,
            None
        )
        .unwrap(),
        1
    );
    drop((control, peer, reader, member_peer));
    let mut input_turn = 0_u64;
    let mut wind_turn = 0_u64;
    let policy = worker::dispatch::Policy {
        family: &super::super::generation::emission::tests::FAMILY,
        method: weaver_types::state_flow::CutMethod::Flush,
        wind_down_passes: std::num::NonZeroUsize::new(1).unwrap(),
        next_turn: move |purpose| {
            let (prefix, ordinal) = match purpose {
                worker::dispatch::TurnPurpose::Input => ("session", &mut input_turn),
                worker::dispatch::TurnPurpose::WindDown => ("wind-down", &mut wind_turn),
            };
            let key = TurnKey(format!("{prefix}-{ordinal}"));
            *ordinal += 1;
            key
        },
    };
    // SAFETY: this isolated reexec holds only this agent. The driver owns the
    // serving thread until explicit process exit, with inherited fd9 retained.
    unsafe { worker::dispatch::drive(run, program, policy) }
}

fn ofd_lock(file: &std::fs::File) -> Result<(), std::io::Error> {
    let request = nix::libc::flock {
        l_type: nix::libc::F_WRLCK as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    // SAFETY: request is a complete flock and this fixture retains the fd.
    if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_OFD_SETLK, &request) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn send_lifecycle(peer: &std::os::fd::OwnedFd, ordinal: u64, directive: LifecycleDirective) {
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        nix::sys::socket::send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
}

fn receive_packet(
    peer: &std::os::fd::OwnedFd,
    seam: &str,
    mode: &str,
    trace: &[u8],
    member: &Member,
    expired_enter: bool,
) -> Option<Vec<u8>> {
    let mut packet = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match nix::sys::socket::recv(peer.as_raw_fd(), &mut packet, MsgFlags::MSG_DONTWAIT) {
        Ok(length) if length > 0 => {
            packet.truncate(length);
            Some(packet)
        }
        Ok(_) | Err(nix::errno::Errno::EAGAIN) => None,
        Err(error) => {
            evidence(mode, trace, member);
            if let Some(destination) = std::env::var_os("WEAVER_DISPATCH_EVIDENCE") {
                let destination = std::path::PathBuf::from(destination).join(mode);
                std::fs::write(
                    destination.join("receive-fault.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"seam": seam, "error": error.to_string(),
                        "expired_enter": expired_enter}),
                    )
                    .unwrap(),
                )
                .unwrap();
            }
            // Expiry grants no substitute answer on the original Enter. Its
            // retired peer can reset, independently of the actual rollback
            // connection whose exact Left remains mandatory below.
            if expired_enter && error == nix::errno::Errno::ECONNRESET {
                None
            } else {
                panic!("dispatcher {seam} receive: {error}")
            }
        }
    }
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The controller never calls worker primitives. It judges actual packets,
/// complete trace EOF, release receipts and the reexecuted driver's exit.
pub(in crate::flow::context::worker_startup) fn scenario(name: &str, mode: &str) {
    use nix::sys::socket::{ControlMessageOwned, recvmsg};
    use std::io::Read;
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use weaver_types::state_flow::capture::*;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = Directory(std::env::temp_dir().join(format!("wd-{}-{serial}", std::process::id())));
    std::fs::create_dir(&root.0).unwrap();
    let layout = crate::tools_cgroup::tests::ScratchLayout::new(&format!("dispatch-{serial}"));
    let listener =
        std::os::unix::net::UnixListener::bind(root.0.join("dispatch-control.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let original_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(root.0.join("dispatch-run.lock"))
        .unwrap();
    ofd_lock(&original_lock).unwrap();
    // SAFETY: duplicate this live description above every placed descriptor.
    let high =
        unsafe { nix::libc::fcntl(original_lock.as_raw_fd(), nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(high >= 100);
    // SAFETY: fcntl returned one fresh owned descriptor.
    let high = unsafe { OwnedFd::from_raw_fd(high) };
    let inherited = high.as_raw_fd();
    let admission_mode =
        crate::flow::context::worker_startup::tests::admission_process::is_mode(mode);
    let relay = admission_mode.then(|| {
        let (read, write) =
            nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC | nix::fcntl::OFlag::O_NONBLOCK)
                .unwrap();
        // Keep its source above fixed descriptors installed in pre_exec.
        let high = unsafe { nix::libc::fcntl(write.as_raw_fd(), nix::libc::F_DUPFD_CLOEXEC, 101) };
        assert!(high >= 101);
        (std::fs::File::from(read), unsafe {
            OwnedFd::from_raw_fd(high)
        })
    });
    let relay_fd = relay.as_ref().map(|(_, write)| write.as_raw_fd());
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", name, "--ignored", "--nocapture"])
        .env("WEAVER_WORKER_STARTUP_CASE", mode)
        .env("WEAVER_GATE_STARTUP_RUN", &layout.run)
        .env("WEAVER_FOUNDATION_DISPATCH_ROOT", &root.0);
    // SAFETY: only async-signal-safe descriptor installation runs after fork.
    unsafe {
        command.pre_exec(move || {
            if nix::libc::dup2(inherited, crate::RUN_LOCK_DESCRIPTOR) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(relay) = relay_fd
                && nix::libc::dup2(relay, crate::RELAY_LIFETIME_DESCRIPTOR) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = ChildGuard(command.spawn().unwrap());
    drop((original_lock, high));
    let mut relay = relay.map(|(read, write)| {
        drop(write);
        read
    });
    // These instruments spend actual lifecycle intervals. The outer fixture
    // watch permits that evidence and never shortens a production bound.
    let watchdog = if mode.contains("-real-model-")
        || mode.ends_with("-setup-arm-admit-deadline")
        || mode.ends_with("-ready-bystander-expiry")
    {
        200
    } else if mode.contains("-interrupt-")
        && (mode.ends_with("-silent") || mode.ends_with("-no-rest"))
    {
        180
    } else {
        35
    };
    let end = Instant::now() + Duration::from_secs(watchdog);
    let control = loop {
        assert!(
            Instant::now() < end,
            "dispatcher did not transfer controller endpoints"
        );
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(error) => panic!("controller accept: {error}"),
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "dispatcher exited before adoption"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    control
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut ancillary = nix::cmsg_space!([i32; 4]);
    let mut byte = [0];
    let mut slice = [std::io::IoSliceMut::new(&mut byte)];
    let packet = recvmsg::<()>(
        control.as_raw_fd(),
        &mut slice,
        Some(&mut ancillary),
        MsgFlags::MSG_CMSG_CLOEXEC,
    )
    .unwrap();
    assert_eq!(packet.bytes, 1);
    let mut descriptors = Vec::new();
    for ancillary in packet.cmsgs().unwrap() {
        let ControlMessageOwned::ScmRights(fds) = ancillary else {
            panic!("only actual fixture handles")
        };
        for fd in fds {
            // SAFETY: recvmsg transferred fresh owned descriptors exactly once.
            descriptors.push(unsafe { OwnedFd::from_raw_fd(fd) });
        }
    }
    assert_eq!(descriptors.len(), if admission_mode { 4 } else { 3 });
    let before_enter = admission_mode.then(|| {
        let state = descriptors.pop().unwrap();
        let sink = descriptors.pop().unwrap();
        (sink, state)
    });
    let mut member_peer = UnixStream::from(descriptors.pop().unwrap());
    member_peer.set_nonblocking(true).unwrap();
    let mut reader = UnixStream::from(descriptors.pop().unwrap());
    reader.set_nonblocking(true).unwrap();
    if let Some(relay) = relay.as_mut() {
        assert_eq!(
            relay.read(&mut [0]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "original fd8 remains held before admission"
        );
    }
    let mut enter_peer = if let Some((sink, state)) = before_enter {
        let result = crate::flow::context::worker_startup::tests::admission_process::before_enter(
            &root.0,
            mode,
            &mut child.0,
            sink,
            state,
            &mut reader,
            &mut member_peer,
            end,
        );
        if result.is_none() {
            assert_eq!(
                relay.as_mut().unwrap().read(&mut [0]).unwrap(),
                0,
                "actual failed worker exit releases original fd8"
            );
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(root.0.join("dispatch-run.lock"))
                .unwrap();
            ofd_lock(&lock).expect("actual failed worker exit releases original fd9");
            return;
        }
        result
    } else {
        Some(descriptors.pop().unwrap())
    };
    let seed = std::fs::read_to_string(root.0.join("dispatch-member-seed")).unwrap();
    let mut member = Member::dispatch_double(member_peer, seed, root.0.clone());
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.0.join("dispatch-run.lock"))
        .unwrap();
    assert_eq!(
        ofd_lock(&lock).unwrap_err().raw_os_error(),
        Some(nix::libc::EAGAIN)
    );
    let rollback = mode.contains("-ready-rollback-");
    let graceful = mode.contains("-graceful-save-");
    let startup_refusal = mode.ends_with("-startup-refusal");
    let startup_failed = mode.ends_with("-startup-failed");
    let expired = mode.ends_with("-enter-expired");
    let serving_failed = mode.ends_with("-serving-failed");
    let failed = startup_failed || serving_failed;
    let pending = mode.ends_with("-pending");
    let s9 = mode.ends_with("-s9");
    let demand = mode.ends_with("-demand-two");
    let two_turns = graceful && !s9;
    if startup_failed || expired {
        member.hold_control = Some("prompt");
    }
    if pending {
        member.hold_kind = Some("message.user");
    }
    let enter_deadline: u64 = std::fs::read_to_string(root.0.join("dispatch-enter-deadline"))
        .unwrap()
        .parse()
        .unwrap();
    let mut fault_injected = false;
    let mut held_source = None;
    let mut held_source_released = false;
    let mut ready = false;
    let mut refused_startup = false;
    let mut join = None;
    let mut join_refused = false;
    let mut leave = None;
    let mut leave_ordinal = 801;
    let mut artifact: Option<CaptureArtifact> = None;
    let mut capture = Vec::new();
    let mut capture_ids = Vec::new();
    let mut s9_refused = false;
    let mut s9_probe = None;
    let mut s9_observed = false;
    let mut s9_stopped = false;
    let mut requested = false;
    let mut saved = false;
    let mut left = false;
    let mut bytes = Vec::new();
    let mut exited = None;
    let mut exited_at = None;
    let status = loop {
        assert!(
            Instant::now() < end,
            "dispatcher {mode} exceeded controller watchdog"
        );
        member.step_live(if rollback {
            "rollback-ready"
        } else if s9 && !s9_refused {
            "input-worker-generation-emission-session-graceful-save-dispatch-refused"
        } else {
            mode
        });
        match reader.read_to_end(&mut bytes) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(error) => panic!("trace reader: {error}"),
        }
        if !fault_injected
            && ((startup_failed && member.held_control.is_some())
                || (serving_failed && ready && join_refused))
        {
            member.stop_responses();
            fault_injected = true;
        }
        if pending
            && held_source.is_none()
            && let Some(held) = &member.held_file
        {
            held_source = Some(held.clone());
            assert!(!root.0.join("ready-rollback-lower.json").exists());
        }
        if pending
            && held_source.is_some()
            && !held_source_released
            && root.0.join("ready-rollback-lower.json").exists()
        {
            assert_eq!(
                member
                    .held_file
                    .as_ref()
                    .map(|frame| weaver_types::state_flow::encode_state_line(
                        frame,
                        weaver_types::state_flow::FrameDirection::Response
                    )
                    .unwrap()),
                held_source
                    .as_ref()
                    .map(|frame| weaver_types::state_flow::encode_state_line(
                        frame,
                        weaver_types::state_flow::FrameDirection::Response
                    )
                    .unwrap()),
                "cleanup must retain the original filing until its response"
            );
            member.hold_kind = None;
            held_source_released = true;
        }
        if let Some(peer) = &enter_peer
            && let Some(packet) = receive_packet(
                peer,
                "original Enter",
                mode,
                &bytes,
                &member,
                expired && crate::flow::clock::monotonic_ns().unwrap() >= enter_deadline,
            )
        {
            let answer: OrganEnvelope = serde_json::from_slice(&packet).unwrap();
            assert_eq!(answer.exchange.ordinal, 7);
            assert_eq!(answer.position, Position::Close);
            if startup_refusal {
                assert!(matches!(
                    answer.payload,
                    Payload::Refusal(weaver_types::LifecycleRefusal::StateManagementInvalid { .. })
                ));
                refused_startup = true;
            } else {
                assert!(
                    !startup_failed && !expired,
                    "stopped startup grants no Ready"
                );
                assert_eq!(answer.payload, Payload::Answer(LifecycleAnswer::Ready));
                ready = true;
                let peer = dial(&root.0.join("admin.sock"));
                send_lifecycle(
                    &peer,
                    800,
                    LifecycleDirective::JoinLeave {
                        cause: Cause { uid: 75 },
                    },
                );
                join = Some(peer);
            }
            enter_peer.take();
        }
        if !join_refused
            && let Some(peer) = &join
            && let Some(packet) = receive_packet(peer, "JoinLeave", mode, &bytes, &member, false)
        {
            let answer: OrganEnvelope = serde_json::from_slice(&packet).unwrap();
            assert_eq!(answer.exchange.ordinal, 800);
            assert_eq!(
                answer.payload,
                Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
            );
            join_refused = true;
        }
        if demand && ready && join_refused && !requested {
            let peer = dial(&root.0.join("admin.sock"));
            let now = crate::flow::clock::monotonic_ns().unwrap();
            let request = CaptureEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 801,
                },
                position: Position::Open,
                payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                    cause: Cause { uid: 74 },
                    window: weaver_types::state_flow::SaveWindow {
                        began_ns: now.into(),
                        deadline_ns: (now + 10_000_000_000).into(),
                    },
                }),
            }
            .encode(CaptureSender::Admin)
            .unwrap();
            assert_eq!(
                nix::sys::socket::send(peer.as_raw_fd(), &request, MsgFlags::MSG_NOSIGNAL),
                Ok(request.len())
            );
            leave = Some(peer);
            requested = true;
        }
        if s9_refused
            && let Some(peer) = &s9_probe
            && let Some(packet) = receive_packet(peer, "S9 probe", mode, &bytes, &member, false)
        {
            if !s9_observed {
                let answer = ObservationEnvelope::decode(&packet).unwrap();
                assert_eq!(answer.exchange.ordinal, 810);
                assert!(matches!(
                    answer.payload,
                    ObservationContent::Answer(ObservationAnswer::State {
                        state: AgentState::Idle,
                        ..
                    })
                ));
                s9_observed = true;
                let peer = dial(&root.0.join("admin.sock"));
                send_lifecycle(
                    &peer,
                    811,
                    LifecycleDirective::Stop {
                        cause: Cause { uid: 75 },
                    },
                );
                s9_probe = Some(peer);
            } else {
                let answer: OrganEnvelope = serde_json::from_slice(&packet).unwrap();
                assert_eq!(answer.exchange.ordinal, 811);
                assert_eq!(answer.payload, Payload::Answer(LifecycleAnswer::AtRest));
                s9_stopped = true;
                s9_probe = None;
            }
        }
        let ordinary_leave = ready
            && join_refused
            && !failed
            && (!two_turns || root.0.join("session-close-1.json").exists())
            && (!pending || held_source.is_some())
            && (!demand || saved)
            && (!s9_refused || s9_stopped);
        let expired_leave = expired
            && member.held_control.is_some()
            && crate::flow::clock::monotonic_ns().unwrap() >= enter_deadline;
        if leave.is_none() && (ordinary_leave || refused_startup || expired_leave) {
            let peer = dial(&root.0.join("admin.sock"));
            if s9_refused || saved {
                leave_ordinal = 802;
            }
            send_lifecycle(
                &peer,
                leave_ordinal,
                LifecycleDirective::Leave {
                    cause: Cause { uid: 74 },
                    forced: !graceful || s9_refused,
                    rollback,
                },
            );
            leave = Some(peer);
        }
        let mut retire_exchange = false;
        if let Some(peer) = &leave
            && let Some(packet) =
                receive_packet(peer, "Leave or SavePoint", mode, &bytes, &member, false)
        {
            if rollback {
                let answer: OrganEnvelope = serde_json::from_slice(&packet).unwrap();
                assert_eq!(answer.exchange.ordinal, leave_ordinal);
                assert_eq!(answer.position, Position::Close);
                assert_eq!(
                    answer.payload,
                    Payload::Answer(LifecycleAnswer::Left {
                        save_point: None,
                        forced: true,
                        no_state: false
                    })
                );
                assert!(!left);
                left = true;
            } else {
                let message = CaptureEnvelope::decode(&packet, CaptureSender::Harness).unwrap();
                assert_eq!(message.exchange.ordinal, leave_ordinal);
                let response = match &message.payload {
                    CapturePayload::SavePointBeginning(beginning) => {
                        assert!(capture.is_empty());
                        assert_eq!(
                            root.0.join("ready-rollback-lower.json").exists(),
                            !demand || saved
                        );
                        assert_eq!(beginning.tail_window.is_none(), demand && !saved);
                        assert!(!capture_ids.contains(&beginning.capture_id));
                        capture_ids.push(beginning.capture_id);
                        None
                    }
                    CapturePayload::SavePointImage(image) => {
                        assert_eq!(capture.len(), 1);
                        let value = CaptureArtifact {
                            capture_id: image.capture_id,
                            artifact_id: format!("{:020}-{}", image.capture_id.0, "d".repeat(64)),
                            dump_sha256: weaver_types::state_flow::Digest::new("9".repeat(64))
                                .unwrap(),
                            config_sha256: weaver_types::state_flow::Digest::new("d".repeat(64))
                                .unwrap(),
                        };
                        let response = CapturePayload::SavePointImageChecked(CaptureImageChecked {
                            capture_id: image.capture_id,
                            dump_sha256: value.dump_sha256.clone(),
                        });
                        artifact = Some(value);
                        Some(response)
                    }
                    CapturePayload::SavePointImageFinished(_) => {
                        assert_eq!(capture.len(), 2);
                        Some(CapturePayload::SavePointPrepared(
                            artifact.as_ref().unwrap().clone(),
                        ))
                    }
                    CapturePayload::SavePointCommit(_) => {
                        assert_eq!(capture.len(), 3);
                        Some(CapturePayload::SavePointPublished(
                            artifact.as_ref().unwrap().clone(),
                        ))
                    }
                    CapturePayload::SavePointCaptureComplete(_) => {
                        assert!(
                            !demand || saved,
                            "requested saves have their own final reply"
                        );
                        assert_eq!(capture.len(), 4);
                        None
                    }
                    CapturePayload::SavePointCaptureRefused(ack) => {
                        assert!(s9 && !s9_refused);
                        assert_eq!(capture.len(), 1);
                        assert_eq!(ack.capture_id, capture_ids[0]);
                        None
                    }
                    CapturePayload::Refusal(CaptureRefusal::Save(
                        SaveRefusal::SavePointNotTaken { missed },
                    )) => {
                        assert!(s9 && !s9_refused);
                        assert!(
                            matches!(capture.last(), Some(CapturePayload::SavePointCaptureRefused(ack)) if ack.missed == *missed)
                        );
                        assert!(
                            child.0.try_wait().unwrap().is_none(),
                            "refused graceful capture must leave its worker standing"
                        );
                        assert_eq!(
                            ofd_lock(&lock).unwrap_err().raw_os_error(),
                            Some(nix::libc::EAGAIN)
                        );
                        s9_refused = true;
                        let probe = dial(&root.0.join("admin.sock"));
                        send_lifecycle(&probe, 810, LifecycleDirective::Observe);
                        s9_probe = Some(probe);
                        retire_exchange = true;
                        None
                    }
                    CapturePayload::Answer(CaptureAnswer::SavePointTaken { report }) => {
                        assert!(demand && !saved);
                        assert_eq!(capture.len(), 4);
                        assert_eq!(report.capture_id, artifact.as_ref().unwrap().capture_id);
                        assert!(!root.0.join("session-close-0.json").exists());
                        saved = true;
                        retire_exchange = true;
                        std::fs::write(
                            root.0.join("dispatch-save-complete"),
                            b"actual SavePointTaken received",
                        )
                        .unwrap();
                        None
                    }
                    CapturePayload::Answer(CaptureAnswer::Left {
                        save_point: Some(report),
                        forced,
                        no_state: false,
                    }) => {
                        assert_eq!(capture.len(), 5);
                        assert_eq!(*forced, !graceful || s9_refused);
                        assert_eq!(report.capture_id, artifact.as_ref().unwrap().capture_id);
                        assert!(!left);
                        left = true;
                        None
                    }
                    other => panic!("unexpected dispatcher capture: {other:?}"),
                };
                if let Some(payload) = response {
                    let response = CaptureEnvelope {
                        exchange: message.exchange.clone(),
                        position: Position::Continue,
                        payload,
                    }
                    .encode(CaptureSender::Admin)
                    .unwrap();
                    assert_eq!(
                        nix::sys::socket::send(peer.as_raw_fd(), &response, MsgFlags::MSG_NOSIGNAL),
                        Ok(response.len())
                    );
                }
                capture.push(message.payload);
            }
        }
        if retire_exchange {
            leave = None;
            capture.clear();
            artifact = None;
        }
        if exited.is_none() {
            exited = child.0.try_wait().unwrap();
            if exited.is_some() {
                exited_at = Some(Instant::now());
            }
        }
        if let Some(status) = exited {
            if left || failed {
                break status;
            }
            if exited_at.unwrap().elapsed() >= Duration::from_secs(1) {
                evidence(mode, &bytes, &member);
            }
            assert!(
                exited_at.unwrap().elapsed() < Duration::from_secs(1),
                "dispatcher exited {status} without its original Left"
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(
        status.code(),
        Some(i32::from(failed)),
        "dispatcher process status {status}"
    );
    assert_eq!(fault_injected, failed);
    if failed {
        assert!(
            !left && leave.is_none(),
            "failed run grants no lifecycle success"
        );
    }
    if startup_refusal {
        assert!(refused_startup && !ready);
    }
    if expired {
        assert!(!ready && member.held_control.is_some());
    }
    if pending {
        assert!(held_source.is_some() && held_source_released && member.held_file.is_none());
    }
    if s9 {
        assert!(s9_refused && s9_observed && s9_stopped);
    }
    if demand {
        assert!(saved);
    }
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    reader.read_to_end(&mut bytes).unwrap();
    member.assert_closed_without_request();
    ofd_lock(&lock).expect("actual driver death released the original inherited fd9 description");
    if let Some(relay) = relay.as_mut() {
        assert_eq!(
            relay.read(&mut [0]).unwrap(),
            0,
            "actual driver death releases original inherited fd8 and organ execs retain no copy"
        );
    }
    let rows: Vec<serde_json::Value> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(rows.iter().filter(|row| row["kind"] == "load").count(), 1);
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    assert_eq!(rows.iter().any(|row| row["kind"] == "fault"), failed);
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    if failed {
        assert_eq!(unload["payload"]["end_reason"], "component_failure");
    } else {
        assert_eq!(unload["payload"]["forced"], !graceful || s9_refused);
    }
    if startup_refusal {
        assert!(unload["payload"]["release"]["spu"].is_null());
    } else {
        assert_eq!(unload["payload"]["release"]["spu"], "confirmed");
    }
    assert_eq!(
        unload["payload"]["release"]["member"],
        if failed { "unconfirmed" } else { "closed" }
    );
    let turns = if two_turns {
        3
    } else {
        usize::from(s9 || pending)
    };
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "turn.closed")
            .count(),
        turns
    );
    let captures = if rollback || failed {
        0
    } else if demand {
        2
    } else {
        1
    };
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "confirmed")
            .count(),
        captures
    );
    if two_turns {
        for turn in ["session-0", "session-1", "wind-down-0"] {
            let closed = rows
                .iter()
                .find(|row| row["kind"] == "turn.closed" && row["turn"] == turn)
                .unwrap();
            assert_eq!(closed["payload"]["close"], "clean");
        }
        for index in 0..3 {
            assert!(
                root.0
                    .join(format!("dispatch-generation-{index}.json"))
                    .exists()
            );
        }
    }
    if pending {
        let user = rows
            .iter()
            .find(|row| row["kind"] == "message.user")
            .unwrap();
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == "message.user")
                .count(),
            1
        );
        assert!(member.filings.iter().any(|file| matches!(file,
            weaver_types::state_flow::StateFrame::File { filing, .. } if filing.source.sequence.0.to_string() == user["sequence"].as_str().unwrap())));
        assert!(!root.0.join("session-generation-0.json").exists());
        let close = rows
            .iter()
            .find(|row| row["kind"] == "turn.closed")
            .unwrap();
        assert_eq!(close["payload"]["close"], "stopped");
    }
    if s9 {
        assert_eq!(capture_ids.len(), 2);
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "refused")
                .count(),
            1
        );
    }
    assert_eq!(root.0.join("ready-rollback-lower.json").exists(), ready);
    evidence(mode, &bytes, &member);
}

fn evidence(mode: &str, bytes: &[u8], member: &Member) {
    // Save captured bytes for the ordinary evidence workflow when requested.
    if let Some(destination) = std::env::var_os("WEAVER_DISPATCH_EVIDENCE") {
        let destination = std::path::PathBuf::from(destination).join(mode);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("trace.jsonl"), bytes).unwrap();
        std::fs::write(
            destination.join("member-requests.json"),
            serde_json::to_vec(&member.requests).unwrap(),
        )
        .unwrap();
        std::fs::write(
            destination.join("member-filings.json"),
            serde_json::to_vec(&member.filings).unwrap(),
        )
        .unwrap();
        std::fs::write(
            destination.join("member-held-control.json"),
            serde_json::to_vec(&member.held_control).unwrap(),
        )
        .unwrap();
    }
}

pub(in crate::flow::context::worker_startup) fn ordering(
    serving: FoundationServing<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    socket: &Path,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let end = Instant::now() + Duration::from_secs(10);
    while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::Accepted {
        assert!(Instant::now() < end, "input was not accepted");
        let input = guard.input.as_mut().unwrap();
        assert_ne!(
            input.advance_with_io(&mut CircuitIo(clock)),
            FoundationInputProgress::Stopped
        );
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    let source = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .owner
        .resident
        .evidence
        .startup
        .original_enter
        .as_ref()
        .unwrap()
        .envelope()
        .enter();
    let weaver_types::state_flow::enter::EnterBinding::Serving {
        gate_instruction,
        shell,
        ..
    } = &source.binding
    else {
        panic!("serving fixture")
    };
    // The retained legacy lifecycle reader can decode this secondary Enter;
    // its payload and carried handles must never replace the existing run.
    let duplicate = LifecycleDirective::Enter {
        payload: Box::new(weaver_types::EnterPayload {
            session: source.session.clone(),
            run: source.run.clone(),
            spu_instruction: source.spu_instruction.clone(),
            binding: weaver_types::EnterBinding::Serving {
                gate_instruction: gate_instruction.clone(),
                shell: shell.clone(),
            },
            state_election: weaver_types::StateElection {
                all_kinds: false,
                keys: vec![],
            },
            state_store: source.state_store.clone(),
            declaration: source.declaration.clone(),
            restore: source.restore.clone(),
            reset: source.reset.clone(),
            stack: source.stack.clone(),
            boundary: source.boundary.clone(),
            cause: source.cause,
            operator: source.operator,
            library_path: source.library_path.clone(),
            drain_bound: source.drain_bound,
            wind_down_bound: source.wind_down_bound,
        }),
    };
    let original_input = std::ptr::from_ref(&**guard.input.as_ref().unwrap());
    let source_wire = guard
        .input
        .as_ref()
        .unwrap()
        .received
        .as_ref()
        .unwrap()
        .raw_bytes()
        .to_vec();
    for active in [false, true] {
        if active {
            guard
                .input
                .as_mut()
                .unwrap()
                .start_turn(TurnKey("ordering-turn".into()))
                .unwrap();
            while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::Turn {
                assert!(
                    Instant::now() < end,
                    "original input could not start its turn"
                );
                let input = guard.input.as_mut().unwrap();
                assert_ne!(
                    input.advance_with_io(&mut CircuitIo(clock)),
                    FoundationInputProgress::Stopped
                );
                member.step(clock.now().unwrap(), "ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        for (index, (directive, descriptors)) in [
            (
                LifecycleDirective::JoinLeave {
                    cause: Cause { uid: 74 },
                },
                false,
            ),
            (LifecycleDirective::Lower, false),
            (duplicate.clone(), false),
            (duplicate.clone(), true),
        ]
        .into_iter()
        .enumerate()
        {
            let stage = guard.input.as_ref().unwrap().stage;
            let recorded = guard.recorded_sequence();
            let gate_bound = guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Gate);
            let peer = dial(socket);
            let request = OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 900 + index as u64 + u64::from(active) * 10,
                },
                position: Position::Open,
                payload: Payload::Directive(directive),
            };
            let bytes = serde_json::to_vec(&request).unwrap();
            let (sink, _sink_reader) = UnixStream::pair().unwrap();
            let (state, _state_reader) = UnixStream::pair().unwrap();
            let fds = [sink.as_raw_fd(), state.as_raw_fd()];
            let handles = if descriptors {
                vec![ControlMessage::ScmRights(&fds)]
            } else {
                vec![]
            };
            assert_eq!(
                sendmsg::<()>(
                    peer.as_raw_fd(),
                    &[std::io::IoSlice::new(&bytes)],
                    &handles,
                    MsgFlags::MSG_NOSIGNAL,
                    None
                )
                .unwrap(),
                bytes.len()
            );
            while guard.input.as_ref().unwrap().progress()
                != FoundationInputProgress::ManagementPending
            {
                assert!(Instant::now() < end, "management was not accepted");
                let input = guard.input.as_mut().unwrap();
                assert_ne!(
                    input.advance_with_io(&mut CircuitIo(clock)),
                    FoundationInputProgress::Stopped
                );
                member.step(clock.now().unwrap(), "ready");
                std::thread::sleep(Duration::from_millis(1));
            }
            guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
            while guard.management.as_ref().unwrap().progress()
                == FoundationManagementProgress::Pending
            {
                assert!(Instant::now() < end, "management refusal did not complete");
                guard
                    .management
                    .as_mut()
                    .unwrap()
                    .advance_with_io(&mut CircuitIo(clock));
                member.step(clock.now().unwrap(), "ready");
                std::thread::sleep(Duration::from_millis(1));
            }
            let owner = guard.management.as_ref().unwrap();
            assert_eq!(
                owner.progress(),
                FoundationManagementProgress::Answered,
                "{:?}",
                owner.detail()
            );
            assert_eq!(owner.message.as_ref().unwrap().envelope, request);
            assert_eq!(owner.message.as_ref().unwrap().sink.is_some(), descriptors);
            assert_eq!(owner.message.as_ref().unwrap().state.is_some(), descriptors);
            let answer: OrganEnvelope =
                serde_json::from_slice(&read_packet(&peer).expect("actual refusal datagram"))
                    .unwrap();
            assert_eq!(answer.exchange, request.exchange);
            assert_eq!(answer.position, Position::Close);
            assert_eq!(
                answer.payload,
                Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
            );
            assert!(
                read_packet(&peer).is_none(),
                "one answer per original request"
            );
            assert_eq!(
                guard.recorded_sequence(),
                recorded,
                "ordering refusal authors no event"
            );
            guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
            let input = guard.input.as_ref().unwrap();
            assert_eq!(std::ptr::from_ref(&**input), original_input);
            assert!(input.stage == stage);
            assert_eq!(input.received.as_ref().unwrap().raw_bytes(), source_wire);
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::Gate),
                gate_bound
            );
            assert!(input.failure.is_none());
        }
    }
}
