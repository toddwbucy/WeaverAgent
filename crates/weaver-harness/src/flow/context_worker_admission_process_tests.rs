//! The actual dormant entry starts before the first lifecycle packet. The
//! controller owns all peer endpoints and judges packets, EOF and process exit.
use super::*;
use weaver_types::state_flow::enter::{EnterBinding, EnterContent, EnterDirective, EnterEnvelope};
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent, ObservationEnvelope};
use weaver_types::{
    AgentState, Cause, ExchangeId, LifecycleDirective, LifecycleRefusal, Opener, OrganEnvelope,
    Payload, Position,
};

#[path = "context_worker_diagnostic_tests.rs"]
mod diagnostic;

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    mode.contains("-admission-") && mode.contains("-dispatch-")
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn run(
    admission: FoundationAdmission,
    mut program: FoundationProgram,
    sink: UnixStream,
    reader: UnixStream,
    state: UnixStream,
    member: UnixStream,
    wire: &[u8],
    seed: &str,
    root: &std::path::Path,
    mode: &str,
) -> ! {
    if mode.contains("-admission-diagnostic-run-") {
        diagnostic::prepare(&mut program, root);
    }
    // Before any real Enter descriptor is accepted, the same entry hygiene as
    // the selected worker must already protect this process and its fd8/fd9.
    assert_eq!(unsafe { nix::libc::prctl(nix::libc::PR_GET_DUMPABLE) }, 0);
    for fd in [crate::RUN_LOCK_DESCRIPTOR, crate::RELAY_LIFETIME_DESCRIPTOR] {
        let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
        assert!(flags >= 0 && flags & nix::libc::FD_CLOEXEC != 0);
    }
    for binary in [
        program.organs.classify.as_ref().unwrap(),
        &program.organs.spu,
        &program.organs.gate,
    ] {
        let script = std::fs::read_to_string(binary).unwrap();
        let script = script.replacen(
            "#!/bin/sh\n",
            "#!/bin/sh\nif [ -e /proc/self/fd/8 ] || [ -e /proc/self/fd/9 ]; then exit 90; fi\n",
            1,
        );
        std::fs::write(binary, script).unwrap();
    }
    std::fs::write(
        root.join("admission-serving-tid"),
        nix::unistd::gettid().as_raw().to_string(),
    )
    .unwrap();
    std::fs::write(root.join("admission-enter.json"), wire).unwrap();
    std::fs::write(root.join("dispatch-member-seed"), seed).unwrap();
    let control = UnixStream::connect(root.join("dispatch-control.sock")).unwrap();
    let descriptors = [
        reader.as_raw_fd(),
        member.as_raw_fd(),
        sink.as_raw_fd(),
        state.as_raw_fd(),
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
    drop((control, reader, member, sink, state));
    let mut input_turn = 0_u64;
    let mut wind_turn = 0_u64;
    let policy = worker::dispatch::Policy {
        family: &super::super::input::generation::emission::tests::FAMILY,
        method: weaver_types::state_flow::CutMethod::Flush,
        wind_down_passes: std::num::NonZeroUsize::new(1).unwrap(),
        next_turn: move |purpose| {
            let (prefix, ordinal) = match purpose {
                worker::dispatch::TurnPurpose::Input => ("session", &mut input_turn),
                worker::dispatch::TurnPurpose::WindDown => ("wind-down", &mut wind_turn),
            };
            let key = weaver_types::TurnKey(format!("{prefix}-{ordinal}"));
            *ordinal += 1;
            key
        },
    };
    // SAFETY: the isolated reexec retains the start-step descriptors for its
    // entire lifetime. The supplied policy never blocks or unwinds.
    if mode.contains("-admission-diagnostic-run-") || mode.contains("-admission-unified-") {
        // SAFETY: the same isolated worker and custody as the Serving-only entry.
        unsafe { worker::admission::drive(admission, program, policy) }
    }
    let diagnostic = unsafe { worker::admission::drive_serving(admission, program, policy) };
    assert_eq!(
        diagnostic.binding_kind(),
        Some(weaver_types::BindingKind::Diagnostic)
    );
    assert_eq!(
        diagnostic.received.as_ref().unwrap().wire(),
        std::fs::read(root.join("admission-diagnostic.json")).unwrap()
    );
    assert!(diagnostic.process_exit().is_none());
    std::fs::write(
        root.join("admission-diagnostic-returned"),
        b"retained original",
    )
    .unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    while !root.join("admission-diagnostic-checked").exists() {
        assert!(
            Instant::now() < end,
            "Diagnostic controller did not check retained handles"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    // Fixture conclusion after proving the original remained live. This is not
    // a lifecycle Ready/Left or production Diagnostic implementation.
    std::process::exit(0);
}

fn send(peer: &OwnedFd, ordinal: u64, directive: LifecycleDirective) {
    let packet = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    };
    let bytes = serde_json::to_vec(&packet).unwrap();
    assert_eq!(
        nix::sys::socket::send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
}

fn receive(peer: &OwnedFd, end: Instant) -> Vec<u8> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    loop {
        match recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => panic!("before-Enter peer closed before its owed answer"),
            Ok(n) => {
                bytes.truncate(n);
                return bytes;
            }
            Err(nix::errno::Errno::EAGAIN) => (),
            Err(error) => panic!("before-Enter receive: {error}"),
        }
        assert!(Instant::now() < end, "before-Enter reply watchdog");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn ordinary(
    root: &std::path::Path,
    ordinal: u64,
    directive: LifecycleDirective,
    observe: bool,
    end: Instant,
) {
    let peer = dial(&root.join("admin.sock"));
    send(&peer, ordinal, directive);
    let bytes = receive(&peer, end);
    if observe {
        let answer = ObservationEnvelope::decode(&bytes).unwrap();
        assert_eq!(
            answer.exchange,
            ExchangeId {
                opener: Opener::Admin,
                ordinal
            }
        );
        assert_eq!(answer.position, Position::Close);
        assert!(
            matches!(answer.payload, ObservationContent::Answer(ObservationAnswer::State {
            plan: None,
            state: AgentState::Unloaded, load: None, ref constituents
        }) if constituents.is_empty())
        );
    } else {
        let answer: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            answer.exchange,
            ExchangeId {
                opener: Opener::Admin,
                ordinal
            }
        );
        assert_eq!(answer.position, Position::Close);
        assert_eq!(
            answer.payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
    }
}

fn no_bytes(stream: &mut UnixStream) {
    stream.set_nonblocking(true).unwrap();
    let mut byte = [0];
    assert_eq!(
        stream.read(&mut byte).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "no pre-load trace or state request"
    );
}

fn closed_empty(stream: &mut UnixStream, end: Instant) {
    stream.set_nonblocking(true).unwrap();
    let mut byte = [0];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return,
            Ok(_) => panic!("unadmitted descriptor carried invented work"),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(error) => panic!("unadmitted EOF: {error}"),
        }
        assert!(Instant::now() < end, "unadmitted handle was not closed");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let rest: Vec<_> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    rest[11].parse::<u64>().unwrap() + rest[12].parse::<u64>().unwrap()
}

fn blocks(
    child: &mut std::process::Child,
    root: &std::path::Path,
    phase: &str,
) -> serde_json::Value {
    let pid = child.id();
    let tid = std::fs::read_to_string(root.join("admission-serving-tid")).unwrap();
    let before = ticks(pid);
    let began = Instant::now();
    let mut observed_poll = false;
    while began.elapsed() < Duration::from_millis(180) {
        assert!(child.try_wait().unwrap().is_none());
        if std::fs::read_to_string(format!("/proc/{pid}/task/{tid}/wchan"))
            .is_ok_and(|s| s.contains("poll"))
        {
            observed_poll = true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let consumed = ticks(pid) - before;
    let hz = unsafe { nix::libc::sysconf(nix::libc::_SC_CLK_TCK) };
    assert!(hz > 0);
    let cpu_ms = consumed * 1000 / hz as u64;
    assert!(cpu_ms < 90, "{phase} consumed {cpu_ms} ms while idle");
    assert!(observed_poll, "{phase} never entered the real poll wait");
    serde_json::json!({"phase":phase, "wall_ms":began.elapsed().as_millis(), "cpu_ms":cpu_ms,
        "observed_poll":observed_poll})
}

fn window(envelope: &mut EnterEnvelope, began: u64, deadline: u64) {
    let EnterContent::Directive(EnterDirective::Enter { payload }) = &mut envelope.payload;
    let EnterBinding::Serving { enter_window, .. } = &mut payload.binding else {
        panic!()
    };
    enter_window.began_ns = began.into();
    enter_window.deadline_ns = deadline.into();
}

fn send_enter(peer: &OwnedFd, bytes: &[u8], handles: &[i32]) {
    assert_eq!(
        sendmsg::<()>(
            peer.as_raw_fd(),
            &[std::io::IoSlice::new(bytes)],
            &[ControlMessage::ScmRights(handles)],
            MsgFlags::MSG_NOSIGNAL,
            None
        )
        .unwrap(),
        bytes.len()
    );
}

/// Returns no peer only after an expected malformed-packet process failure.
#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn before_enter(
    root: &std::path::Path,
    mode: &str,
    child: &mut std::process::Child,
    sink: OwnedFd,
    state: OwnedFd,
    reader: &mut UnixStream,
    member: &mut UnixStream,
    end: Instant,
) -> Option<OwnedFd> {
    let mut observations = vec![blocks(child, root, "listener")];
    if mode.contains("-admission-ordinary-") {
        let socket = root.join("admin.sock");
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o666)).unwrap();
        let mut unprivileged = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "flow::context::worker_startup::tests::worker_startup_unprivileged_dial",
                "--nocapture",
            ])
            .env("WEAVER_WORKER_UNPRIVILEGED_DIAL", &socket)
            .uid(65534)
            .gid(65534)
            .spawn()
            .unwrap();
        loop {
            if let Some(status) = unprivileged.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "non-root peer must close before reading any bytes"
                );
                break;
            }
            if Instant::now() >= end {
                let _ = unprivileged.kill();
                let _ = unprivileged.wait();
                panic!("unprivileged admission watchdog");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mut envelope =
        EnterEnvelope::decode(&std::fs::read(root.join("admission-enter.json")).unwrap()).unwrap();
    // A silent accepted connection retains serial custody while the next dial
    // waits in the backlog. Its EOF must release that custody without a fault.
    let silent = dial(&root.join("admin.sock"));
    observations.push(blocks(child, root, "silent peer"));
    let queued = dial(&root.join("admin.sock"));
    send(&queued, 900, LifecycleDirective::Observe);
    std::thread::sleep(Duration::from_millis(20));
    let mut byte = [0];
    assert_eq!(
        recv(queued.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
    drop(silent);
    let answer = ObservationEnvelope::decode(&receive(&queued, end)).unwrap();
    assert_eq!(answer.exchange.ordinal, 900);
    assert!(matches!(
        answer.payload,
        ObservationContent::Answer(ObservationAnswer::State {
            state: AgentState::Unloaded,
            load: None,
            ..
        })
    ));
    drop(queued);
    ordinary(root, 901, LifecycleDirective::Observe, true, end);
    ordinary(
        root,
        902,
        LifecycleDirective::Stop {
            cause: Cause { uid: 75 },
        },
        false,
        end,
    );
    ordinary(
        root,
        903,
        LifecycleDirective::Leave {
            cause: Cause { uid: 75 },
            forced: false,
            rollback: false,
        },
        false,
        end,
    );
    ordinary(
        root,
        904,
        LifecycleDirective::Leave {
            cause: Cause { uid: 75 },
            forced: true,
            rollback: true,
        },
        false,
        end,
    );
    ordinary(
        root,
        905,
        LifecycleDirective::JoinLeave {
            cause: Cause { uid: 75 },
        },
        false,
        end,
    );
    let lost = dial(&root.join("admin.sock"));
    send(&lost, 906, LifecycleDirective::Observe);
    drop(lost);
    ordinary(root, 907, LifecycleDirective::Observe, true, end);
    no_bytes(reader);
    no_bytes(member);
    if mode.contains("-admission-diagnostic-run-") {
        diagnostic::control(
            root, mode, child, envelope, sink, state, reader, member, end,
        );
        return None;
    }
    if mode.contains("-admission-diagnostic-") {
        let EnterContent::Directive(EnterDirective::Enter { payload }) = &mut envelope.payload;
        payload.binding = EnterBinding::Diagnostic {
            state_election: weaver_types::StateElection::default(),
        };
        let wire = envelope.encode().unwrap();
        std::fs::write(root.join("admission-diagnostic.json"), &wire).unwrap();
        let peer = dial(&root.join("admin.sock"));
        send_enter(&peer, &wire, &[sink.as_raw_fd(), state.as_raw_fd()]);
        drop((sink, state));
        while !root.join("admission-diagnostic-returned").exists() {
            assert!(
                Instant::now() < end && child.try_wait().unwrap().is_none(),
                "Diagnostic owner did not return intact"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        no_bytes(reader);
        no_bytes(member);
        assert_eq!(
            recv(peer.as_raw_fd(), &mut [0], MsgFlags::MSG_DONTWAIT),
            Err(nix::errno::Errno::EAGAIN),
            "Serving entry authors no Diagnostic answer"
        );
        std::fs::write(
            root.join("admission-diagnostic-checked"),
            b"actual descriptors remain owned",
        )
        .unwrap();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < end,
                "Diagnostic fixture conclusion watchdog"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        closed_empty(reader, end);
        closed_empty(member, end);
        return None;
    }
    if mode.contains("-admission-malformed-") || mode.contains("-admission-truncated-") {
        let peer = dial(&root.join("admin.sock"));
        let wire = if mode.contains("-admission-truncated-") {
            vec![b' '; weaver_types::MAX_ENVELOPE_BYTES + 1]
        } else {
            b"{bad".to_vec()
        };
        send_enter(&peer, &wire, &[sink.as_raw_fd(), state.as_raw_fd()]);
        drop((sink, state));
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert_eq!(
                    status.code(),
                    Some(1),
                    "malformed service must exit through channel failure"
                );
                break;
            }
            assert!(Instant::now() < end, "malformed admission process watchdog");
            std::thread::sleep(Duration::from_millis(1));
        }
        closed_empty(reader, end);
        closed_empty(member, end);
        return None;
    }
    if !mode.contains("-admission-ordinary-") && !mode.contains("-admission-unified-") {
        let mut invalid = envelope.clone();
        invalid.exchange.ordinal = 910;
        let now = crate::flow::clock::monotonic_ns().unwrap();
        if mode.contains("-admission-future-") {
            window(&mut invalid, now + 20_000_000_000, now + 25_000_000_000);
        } else if mode.contains("-admission-expired-") {
            window(&mut invalid, now - 2_000_000, now - 1_000_000);
        } else {
            window(&mut invalid, now, now + 5_000_000_000);
        }
        let peer = dial(&root.join("admin.sock"));
        let (bad_sink, mut bad_reader) = UnixStream::pair().unwrap();
        let (bad_state, mut bad_member) = UnixStream::pair().unwrap();
        let handles = if mode.contains("-admission-missing-") {
            vec![bad_sink.as_raw_fd()]
        } else {
            vec![bad_sink.as_raw_fd(), bad_state.as_raw_fd()]
        };
        send_enter(&peer, &invalid.encode().unwrap(), &handles);
        drop((bad_sink, bad_state));
        if !mode.contains("-admission-expired-") {
            let answer: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
            assert_eq!(answer.exchange, invalid.exchange);
            assert_eq!(answer.position, Position::Close);
            assert_eq!(
                answer.payload,
                Payload::Refusal(if mode.contains("-admission-missing-") {
                    LifecycleRefusal::DescriptorsUnusable
                } else {
                    LifecycleRefusal::Malformed
                })
            );
        }
        closed_empty(&mut bad_reader, end);
        closed_empty(&mut bad_member, end);
        if mode.contains("-admission-expired-") {
            let mut bytes = [0; 512];
            match recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
                // The original peer may remain retained until the next root
                // dial. Only its sink/state handles must already be closed.
                Ok(0) | Err(nix::errno::Errno::ECONNRESET | nix::errno::Errno::EAGAIN) => (),
                other => panic!("expired Enter grants no substitute answer: {other:?}"),
            }
        }
        drop(peer);
        ordinary(root, 911, LifecycleDirective::Observe, true, end);
        ordinary(
            root,
            912,
            LifecycleDirective::Leave {
                cause: Cause { uid: 75 },
                forced: true,
                rollback: true,
            },
            false,
            end,
        );
        no_bytes(reader);
        no_bytes(member);
    }
    let now = crate::flow::clock::monotonic_ns().unwrap();
    window(&mut envelope, now, now + 5_000_000_000);
    std::fs::write(
        root.join("dispatch-enter-deadline"),
        (now + 5_000_000_000).to_string(),
    )
    .unwrap();
    if let Some(destination) = std::env::var_os("WEAVER_DISPATCH_EVIDENCE") {
        let destination = PathBuf::from(destination).join(mode);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(
            destination.join("admission-waits.json"),
            serde_json::to_vec(&observations).unwrap(),
        )
        .unwrap();
    }
    let peer = dial(&root.join("admin.sock"));
    send_enter(
        &peer,
        &envelope.encode().unwrap(),
        &[sink.as_raw_fd(), state.as_raw_fd()],
    );
    drop((sink, state));
    Some(peer)
}
