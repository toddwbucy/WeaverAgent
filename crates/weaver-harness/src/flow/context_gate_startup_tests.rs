use super::*;
use crate::flow::clock::{ReadyFd, WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::pump::LifecyclePeer;
use nix::poll::PollFlags;
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

const CHILD: &str = "flow::context::boot_composition::gate_startup::tests::gate_startup_child";
fn instruction() -> GateInstruction {
    GateInstruction {
        access_rule: weaver_types::AccessRule {
            allowed_uids: [111].into(),
            allowed_gids: [222].into(),
            denied_uids: [333].into(),
        },
    }
}
fn shell() -> weaver_types::ShellGrant {
    weaver_types::ShellGrant {
        binary: "/fixture/bin/shell".into(),
        commands: "/fixture/commands".into(),
    }
}
fn packet(fd: i32, value: &OrganEnvelope) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert_eq!(send(fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
}
#[test]
fn gate_startup_child() {
    let Ok(mode) = std::env::var("WEAVER_GATE_STARTUP_CHILD") else {
        return;
    };
    // SAFETY: these are this child's two ends placed by the actual producer.
    let port = unsafe { OwnedFd::from_raw_fd(3) };
    let tools = unsafe { OwnedFd::from_raw_fd(4) };
    assert_eq!(
        nix::sys::statfs::fstatfs(&tools).unwrap().filesystem_type(),
        nix::sys::statfs::CGROUP2_SUPER_MAGIC
    );
    let expected = std::fs::metadata(format!(
        "/sys/fs/cgroup{}/../tools",
        std::env::var("WEAVER_GATE_STARTUP_RUN").unwrap()
    ))
    .unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        nix::sys::stat::fstat(&tools).unwrap().st_ino,
        expected.ino()
    );
    if mode == "eof" {
        return;
    }
    if mode == "early" {
        packet(
            3,
            &OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 1,
                },
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::GateReady),
            },
        );
    }
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    if mode == "pressure" {
        std::thread::sleep(Duration::from_millis(200));
    }
    let n = loop {
        let n = recv(port.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap();
        if mode != "pressure" || n == 0 || bytes[0] != b'x' {
            break n;
        }
    };
    if n == 0 {
        return;
    }
    let request: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    assert_eq!(request.position, Position::Open);
    assert_eq!(request.exchange.opener, Opener::Harness);
    let weaver_types::Payload::Directive(LifecycleDirective::Raise {
        instruction: actual,
        socket,
        shell: grant,
    }) = &request.payload
    else {
        panic!("actual Raise");
    };
    assert_eq!(*actual, instruction());
    assert_eq!(*grant, if mode == "shell" { Some(shell()) } else { None });
    assert_eq!(socket.file_name().unwrap(), "gate.sock");
    let _bound = if matches!(
        mode.as_str(),
        "ready" | "shell" | "delay" | "finish-expiry" | "pressure"
    ) || mode.starts_with("expired-startup-")
    {
        Some(std::os::unix::net::UnixListener::bind(socket).unwrap())
    } else {
        None
    };
    if mode == "delay" {
        std::thread::sleep(Duration::from_millis(30));
    }
    if mode != "silent" && mode != "early" && !mode.ends_with("-lower-unreplied") {
        let mut answer = OrganEnvelope {
            exchange: request.exchange.clone(),
            position: Position::Close,
            payload: if mode == "refuse" {
                weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::BindFailed)
            } else if mode == "wrong-answer" {
                weaver_types::Payload::Answer(LifecycleAnswer::Ready)
            } else {
                weaver_types::Payload::Answer(LifecycleAnswer::GateReady)
            },
        };
        if mode == "wrong-id" {
            answer.exchange.ordinal += 1;
        }
        packet(3, &answer);
        if std::env::var("WEAVER_GATE_STARTUP_DRIVER").ok().as_deref()
            == Some("entered-aggregate-plain-ready")
        {
            // The aggregate admission probe replaces only the retained reply,
            // using a second real datagram on this same gate socket.
            assert_eq!(
                recv(port.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap(),
                1
            );
            assert_eq!(bytes[0], b'g');
            answer.payload = weaver_types::Payload::Answer(LifecycleAnswer::Ready);
            packet(3, &answer);
        }
    }
    if mode.starts_with("failed-startup-") || mode.starts_with("expired-startup-") {
        let root = socket.parent().unwrap();
        if mode.ends_with("-lower-frames") || mode.ends_with("-lower-pressure") {
            let count = if mode.ends_with("-lower-pressure") {
                1
            } else {
                3
            };
            for ordinal in 101..101 + count {
                packet(
                    3,
                    &OrganEnvelope {
                        exchange: weaver_types::ExchangeId {
                            opener: Opener::Gate,
                            ordinal,
                        },
                        position: Position::Open,
                        payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry_from(
                            br#"{"text":"queued during expired Enter"}"#,
                            42,
                        )),
                    },
                );
            }
            std::fs::write(root.join("gate-frames-sent"), count.to_string()).unwrap();
        }
        if mode.ends_with("-lower-early") {
            packet(
                3,
                &OrganEnvelope {
                    exchange: weaver_types::ExchangeId {
                        opener: Opener::Harness,
                        ordinal: request.exchange.ordinal + 1,
                    },
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
                },
            );
            std::fs::write(root.join("gate-early-sent"), b"sent").unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            match recv(port.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
                Ok(0) => return,
                Ok(n) => {
                    let lower: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
                    assert_eq!(
                        lower.payload,
                        weaver_types::Payload::Directive(LifecycleDirective::Lower)
                    );
                    let marker = root.join("gate-lower-received");
                    assert!(!marker.exists(), "a second Lower is not cleanup authority");
                    std::fs::write(&marker, &bytes[..n]).unwrap();
                    if mode.contains("-lower-last-word-") {
                        last_word_fixture(port, root, &mode, deadline);
                        return;
                    }
                    if mode.ends_with("-lower-transport-loss") {
                        // Exit with the actual Lower unanswered. The owner
                        // must observe EOF and reap this exact child.
                        return;
                    }
                    if mode.ends_with("-lower-silent")
                        || mode.ends_with("-lower-silent-admin")
                        || mode.ends_with("-lower-cap")
                        || mode.ends_with("-lower-pressure")
                    {
                        continue;
                    }
                    if mode.ends_with("-lower-unreplied") {
                        packet(
                            3,
                            &OrganEnvelope {
                                exchange: request.exchange.clone(),
                                position: Position::Close,
                                payload: weaver_types::Payload::Answer(LifecycleAnswer::GateReady),
                            },
                        );
                    }
                    let mut exchange = lower.exchange;
                    if mode.ends_with("-lower-wrong") {
                        exchange.ordinal += 1;
                    }
                    packet(
                        3,
                        &OrganEnvelope {
                            exchange,
                            position: Position::Close,
                            payload: if mode.ends_with("-lower-refused") {
                                weaver_types::Payload::Refusal(
                                    weaver_types::LifecycleRefusal::OutOfOrder,
                                )
                            } else {
                                weaver_types::Payload::Answer(LifecycleAnswer::GateStopped)
                            },
                        },
                    );
                    if mode.ends_with("-lower-held") {
                        std::fs::write(root.join("gate-stopped-sent"), b"sent").unwrap();
                        loop {
                            match recv(port.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
                                Ok(0) | Err(nix::errno::Errno::EAGAIN) => (),
                                Ok(n) => panic!(
                                    "unexpected second request after GateStopped: {:?}",
                                    serde_json::from_slice::<OrganEnvelope>(&bytes[..n])
                                ),
                                other => panic!("held gate probe: {other:?}"),
                            }
                            if root.join("gate-exit").exists() {
                                break;
                            }
                            assert!(Instant::now() < deadline, "held gate exit watchdog");
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    }
                    return;
                }
                Err(nix::errno::Errno::EAGAIN) => std::thread::sleep(Duration::from_millis(1)),
                other => panic!("failed startup Lower: {other:?}"),
            }
        }
        panic!("failed startup gate watchdog");
    }
    // The test owner closes the gate port before reaping; no live deployment.
    while recv(port.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap_or(0) > 0 {}
}

fn last_word_fixture(port: OwnedFd, root: &std::path::Path, mode: &str, deadline: Instant) {
    let wait = |name: &str| {
        while !root.join(name).exists() {
            assert!(
                Instant::now() < deadline,
                "last-word child waiting for {name}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    let speak = || {
        eprintln!(r#"{{"fixture":"earlier gate word"}}"#);
        eprintln!("untyped gate stderr before final JSON");
        eprintln!(r#"{{"fixture":"gate-last-word","answer":42}}"#);
        eprintln!("untyped gate stderr after final JSON");
        std::fs::write(root.join("gate-word-written"), b"written").unwrap();
    };
    let delayed = mode.ends_with("-delayed");
    if !delayed {
        speak();
    }
    wait("gate-word-close");
    drop(port);
    std::fs::write(root.join("gate-word-closed"), b"closed").unwrap();
    if delayed {
        wait("gate-word-write");
        speak();
    } else {
        // Descriptor 3 is closed while this real child's descriptor 2 remains
        // open. The snapshot must never block on this held stderr stream.
        wait("gate-word-exit");
        if mode.ends_with("-timeout") {
            eprintln!(r#"{{"fixture":"word after frozen death"}}"#);
        }
    }
}

impl GateStartup<'_> {
    pub(in crate::flow::context) fn expired_test_raise_sent_without_reply(&self) -> bool {
        self.stage == GateStartupStage::Raise
            && self
                .send
                .as_ref()
                .is_some_and(|send| send.sent() && send.sent_at().is_some())
            && self.reply.is_none()
    }
}

pub(super) struct CircuitIo<'a>(pub(super) &'a mut FakePumpIo);
impl PumpIo for CircuitIo<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        let at = self.0.now()? + 1;
        self.0.schedule(
            at,
            interests
                .iter()
                .enumerate()
                .map(|(index, x)| ReadyFd {
                    index,
                    events: x.events & (PollFlags::POLLIN | PollFlags::POLLOUT),
                })
                .collect(),
        )?;
        self.0.wait(interests, deadline)
    }
}
struct Guard<'a>(Option<GateStartup<'a>>);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if let Some(owner) = &mut self.0 {
            owner.port.take();
            if let Some(child) = &mut owner.child {
                for _ in 0..3000 {
                    if child.poll()
                        != crate::flow::context::failed_enter_driver::ChildState::Running
                    {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                child.stop_at_cap();
            }
        }
    }
}

pub(in crate::flow::context) fn drive(
    boot: ComposedBoot<'_>,
    clock: &mut FakePumpIo,
    mut residency: Option<&mut OrganPort>,
    mode: &str,
) {
    let resident_mode = mode.to_owned();
    let mode = if mode.starts_with("resident-") || mode.starts_with("entered-") {
        "ready"
    } else {
        mode
    };
    let path = std::env::temp_dir().join(format!("weaver-gate-composition-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    let coordination = crate::channel::bind_coordination(&path.join("admin.sock")).unwrap();
    let listener = LifecycleListener::new(coordination).unwrap();
    let binary = path.join("gate-child");
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    std::fs::write(
        &binary,
        format!(
            "#!/bin/sh\nexport WEAVER_GATE_STARTUP_CHILD={}\nexec {} --exact {} --nocapture\n",
            quote(mode),
            quote(std::env::current_exe().unwrap().to_str().unwrap()),
            CHILD
        ),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let enter = boot.context.deadlines.get(PumpDeadline::Enter).unwrap();
    let (mut gate_instruction, gate_shell) = boot
        .resident
        .as_ref()
        .and_then(|arms| arms.evidence.startup.original_enter.as_ref())
        .map(|original| {
            let weaver_types::state_flow::enter::EnterBinding::Serving {
                gate_instruction,
                shell,
                ..
            } = &original.envelope().enter().binding
            else {
                panic!()
            };
            (gate_instruction.clone(), shell.clone())
        })
        .unwrap_or_else(|| (instruction(), (mode == "shell").then(shell)));
    if resident_mode == "entered-wrong-gate" {
        gate_instruction.access_rule.allowed_uids.insert(999);
    }
    let mut guard = Guard(Some(boot.into_gate(
        GateStartupRequest {
            binary,
            instruction: gate_instruction,
            shell: gate_shell,
        },
        listener,
    )));
    if resident_mode == "entered-wrong-gate" {
        let owner = guard.0.as_ref().unwrap();
        assert_eq!(owner.stage(), GateStartupStage::Stopped);
        assert!(matches!(owner.stop(), Some(GateStartupStop::InvalidEntry)));
        assert!(owner.child.is_none());
        assert!(
            owner
                .boot
                .resident
                .as_ref()
                .unwrap()
                .evidence
                .startup
                .original_enter
                .is_some()
        );
        return;
    }
    if resident_mode == "entered-substitute-gate" {
        let (near, _far) = crate::channel::OrganChannel::pair().unwrap();
        let mut peer = LifecyclePeer::from_channel(near).unwrap();
        let owner = guard.0.as_mut().unwrap();
        // SAFETY: the checked rejection precedes any fork.
        assert_eq!(
            unsafe {
                owner.advance(
                    LifecyclePorts {
                        peer: Some(&mut peer),
                        ..Default::default()
                    },
                    clock,
                )
            },
            GateStartupStage::Stopped
        );
        assert!(owner.child.is_none());
        assert!(
            owner
                .boot
                .resident
                .as_ref()
                .unwrap()
                .evidence
                .startup
                .original_enter
                .is_some()
        );
        return;
    }
    if resident_mode == "resident-gate-substitute" {
        let (near, _far) = crate::channel::OrganChannel::pair().unwrap();
        let mut substitute = OrganPort::from_channel(near).unwrap();
        let owner = guard.0.as_mut().unwrap();
        // SAFETY: no fork is allowed by this refusal, and the guard retains custody.
        assert_eq!(
            unsafe {
                owner.advance(
                    LifecyclePorts {
                        residency: Some(&mut substitute),
                        ..Default::default()
                    },
                    clock,
                )
            },
            GateStartupStage::Stopped
        );
        assert!(owner.child.is_none());
        let arms = owner
            .boot
            .resident
            .as_ref()
            .expect("stopped gate retained resident arms");
        assert!(arms.evidence.reply.belongs_to(arms.residency));
        assert_eq!(arms.evidence.enter, enter);
        return;
    }
    let (admin, far) = crate::channel::OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    let start = Instant::now();
    let mut before_fork = true;
    let mut did_stop = false;
    let mut staged_at = None;
    let mut filled = false;
    let mut pressure_seen = false;
    loop {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "gate did not terminate"
        );
        let owner = guard.0.as_mut().unwrap();
        if owner.stage() == GateStartupStage::Fork {
            assert!(owner.child.is_none());
            before_fork = false;
            if mode == "pre-fork-expiry" {
                clock.advance_to(enter.deadline_ns()).unwrap();
            }
        }
        if mode == "foreign-window" && owner.stage() == GateStartupStage::Boundary {
            owner.boot.context.deadlines.disarm(PumpDeadline::Enter);
            owner
                .boot
                .context
                .deadlines
                .arm(
                    PumpDeadline::Enter,
                    AbsoluteBound::after(
                        enter.began_ns(),
                        enter.deadline_ns() - enter.began_ns() + 1,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        if owner.stage() == GateStartupStage::Raise {
            if resident_mode == "entered-eof-gate" && !did_stop {
                crate::flow::context::startup_composition::tests::entered_tests::close_original_peer();
                did_stop = true;
            }
            if resident_mode == "resident-spu-eof-gate" && !did_stop {
                owner.boot.resident.as_mut().unwrap().spu.stop_at_cap();
                did_stop = true;
                // Make terminal residency visible before the next common pump.
                std::thread::sleep(Duration::from_millis(20));
            }
            if mode == "pressure" && !filled {
                let mut count = 0;
                loop {
                    match send(
                        owner.port.as_ref().unwrap().fd().as_raw_fd(),
                        &[b'x'; 4096],
                        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                    ) {
                        Ok(4096) => count += 1,
                        Err(nix::errno::Errno::EAGAIN) => break,
                        other => panic!("fill actual gate sender: {other:?}"),
                    }
                }
                assert!(count > 0);
                filled = true;
            }
            if mode == "pressure" && owner.send.as_ref().is_some_and(|send| !send.sent()) {
                pressure_seen = true;
            }
            if staged_at.is_none() {
                staged_at = Some(clock.now().unwrap());
            }
            if mode == "early" && owner.send.is_none() {
                for _ in 0..3000 {
                    owner.port.as_mut().unwrap().try_receive().unwrap();
                    if owner.port.as_ref().unwrap().held().is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(owner.port.as_ref().unwrap().held().is_some());
            }
            if mode == "stop" && !did_stop {
                packet(
                    far.as_fd().as_raw_fd(),
                    &OrganEnvelope {
                        exchange: weaver_types::ExchangeId {
                            opener: Opener::Admin,
                            ordinal: 17,
                        },
                        position: Position::Open,
                        payload: weaver_types::Payload::Directive(LifecycleDirective::Stop {
                            cause: weaver_types::Cause { uid: 0 },
                        }),
                    },
                );
                admin.try_receive().unwrap();
                did_stop = true;
            }
            if mode == "silent" && owner.send.as_ref().is_some_and(OrganSend::sent) {
                clock.advance_to(enter.deadline_ns()).unwrap();
            }
        }
        // SAFETY: this guard closes/reaps the gate, while the outer fixture owns
        // the actual classifier and SPU children for the same serving thread.
        let stage = unsafe {
            owner.advance(
                LifecyclePorts {
                    peer: (!resident_mode.starts_with("entered-")).then_some(&mut admin),
                    residency: residency.as_deref_mut(),
                    ..Default::default()
                },
                &mut CircuitIo(clock),
            )
        };
        if mode != "foreign-window" {
            assert_eq!(
                owner.boot.context.deadlines.get(PumpDeadline::Enter),
                Some(enter)
            );
        }
        if stage == GateStartupStage::Complete || stage == GateStartupStage::Stopped {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if !matches!(mode, "boundary" | "foreign-window") {
        assert!(!before_fork, "the gate was never reached after actual boot");
    }
    let mut owner = guard.0.take().unwrap();
    if resident_mode == "resident-spu-eof-gate" {
        assert_eq!(owner.stage(), GateStartupStage::Stopped);
        assert!(matches!(
            owner.stop(),
            Some(GateStartupStop::Run(RunFailure::Transport {
                source: PumpSource::Residency,
                ..
            }))
        ));
        assert!(owner.boot.resident.is_some());
        guard.0 = Some(owner);
        return;
    }
    if matches!(mode, "boundary" | "pre-fork-expiry" | "foreign-window") {
        assert_eq!(owner.stage(), GateStartupStage::Stopped);
        assert!(owner.child.is_none());
        assert!(owner.port.is_none());
        assert!(owner.send.is_none());
        if mode == "boundary" {
            assert!(matches!(owner.stop(), Some(GateStartupStop::Boundary(_))));
        }
        if mode == "pre-fork-expiry" {
            assert!(matches!(
                owner.stop(),
                Some(GateStartupStop::Run(RunFailure::Wait {
                    key: PumpDeadline::Enter,
                    ..
                }))
            ));
        }
        if mode == "foreign-window" {
            assert!(matches!(
                owner.stop(),
                Some(GateStartupStop::Run(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    ..
                }))
            ));
        }
        guard.0 = Some(owner);
        drop(guard);
        let _ = std::fs::remove_dir_all(path);
        return;
    }
    if mode == "pressure" {
        assert!(pressure_seen, "send never faced backpressure");
    }
    if resident_mode == "entered-eof-gate" {
        assert_eq!(owner.stage(), GateStartupStage::Stopped);
        assert!(matches!(
            owner.stop(),
            Some(GateStartupStop::Run(RunFailure::Transport {
                source: PumpSource::Admin,
                ..
            }))
        ));
        assert!(
            owner
                .boot
                .resident
                .as_ref()
                .unwrap()
                .evidence
                .startup
                .original_enter
                .is_some()
        );
        guard.0 = Some(owner);
        return;
    }

    if matches!(
        mode,
        "ready" | "shell" | "delay" | "finish-expiry" | "pressure"
    ) {
        assert_eq!(
            owner.stage(),
            GateStartupStage::Complete,
            "{:?}",
            owner.stop()
        );
        assert!(
            path.join("gate.sock").exists(),
            "gate confirms only after binding"
        );
        let sent = owner.send.as_ref().unwrap();
        assert!(sent.sent());
        assert!(sent.sent_at().unwrap() >= staged_at.unwrap());
        assert!(
            owner
                .reply
                .as_ref()
                .unwrap()
                .belongs_to(owner.port.as_ref().unwrap())
        );
        if mode == "finish-expiry" || resident_mode == "entered-finish-expiry-eof" {
            clock.advance_to(enter.deadline_ns()).unwrap();
        }
        if matches!(
            resident_mode.as_str(),
            "entered-finish-eof" | "entered-finish-expiry-eof"
        ) {
            crate::flow::context::startup_composition::tests::entered_tests::close_original_peer();
        }
        match owner.finish(
            LifecyclePorts {
                peer: (!resident_mode.starts_with("entered-")).then_some(&mut admin),
                residency,
                ..Default::default()
            },
            clock,
        ) {
            Ok(ready) => {
                if resident_mode.starts_with("entered-aggregate-")
                    || resident_mode == "resident-aggregate-missing"
                {
                    guard.0 = Some(super::ready::tests::drive(ready, clock, &resident_mode));
                    drop(guard);
                    let _ = std::fs::remove_dir_all(path);
                    return;
                }
                assert_ne!(resident_mode, "entered-finish-eof");
                assert_ne!(resident_mode, "entered-finish-expiry-eof");
                assert_ne!(mode, "finish-expiry");
                let arms = ready
                    .owner
                    .boot
                    .resident
                    .as_ref()
                    .expect("raised gate retains actual resident arms");
                assert!(arms.evidence.reply.belongs_to(arms.residency));
                assert_eq!(arms.evidence.enter, enter);
                if resident_mode.starts_with("entered-") {
                    let source = arms
                        .evidence
                        .startup
                        .original_enter
                        .as_ref()
                        .expect("original Enter survives gate Ready");
                    assert_eq!(source.envelope().exchange.ordinal, 7);
                    assert!(source.adopted_by(ready.owner.boot.context));
                }
                assert!(arms.request.send().sent());
                assert_eq!(
                    arms.spu.state(),
                    crate::flow::context::failed_enter_driver::ChildState::Running
                );
                assert_eq!(
                    arms.classifier.state(),
                    crate::flow::context::failed_enter_driver::ChildState::Running
                );
                assert_eq!(ready.enter(), enter);
                assert_eq!(ready.shell_granted(), mode == "shell");
                assert!(matches!(
                    ready.reply().envelope().payload,
                    weaver_types::Payload::Answer(LifecycleAnswer::GateReady)
                ));
                guard.0 = Some(ready.owner);
            }
            Err(stopped) => {
                if resident_mode == "entered-finish-eof" {
                    assert!(matches!(
                        stopped.stop(),
                        Some(GateStartupStop::Run(RunFailure::Transport {
                            source: PumpSource::Admin,
                            ..
                        }))
                    ));
                    assert!(
                        stopped
                            .boot
                            .resident
                            .as_ref()
                            .unwrap()
                            .evidence
                            .startup
                            .original_enter
                            .is_some()
                    );
                    guard.0 = Some(stopped);
                    return;
                }
                assert!(mode == "finish-expiry" || resident_mode == "entered-finish-expiry-eof");
                assert!(matches!(
                    stopped.stop(),
                    Some(GateStartupStop::Run(RunFailure::Wait {
                        key: PumpDeadline::Enter,
                        ..
                    }))
                ));
                guard.0 = Some(stopped);
            }
        }
    } else {
        assert_eq!(owner.stage(), GateStartupStage::Stopped);
        if mode == "refuse" {
            assert!(matches!(owner.stop(), Some(GateStartupStop::Refused)));
            assert!(matches!(
                owner.reply.as_ref().unwrap().envelope().payload,
                weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::BindFailed)
            ));
        }
        if mode == "early" {
            assert!(!owner.send.as_ref().unwrap().sent());
            assert!(owner.reply.is_some());
        }
        if mode == "stop" {
            assert!(matches!(owner.stop(), Some(GateStartupStop::Interrupted)));
            assert!(owner.send.is_none());
        }
        if mode == "silent" {
            assert!(matches!(
                owner.stop(),
                Some(GateStartupStop::Run(RunFailure::Wait {
                    key: PumpDeadline::Enter,
                    ..
                }))
            ));
        }
        assert!(
            owner.child.is_some(),
            "actual fork remains owned on failure"
        );
        owner = match owner.finish(LifecyclePorts::default(), clock) {
            Err(owner) => owner,
            Ok(_) => panic!("stopped gate granted Ready"),
        };
        guard.0 = Some(owner);
    }
    drop(guard);
    let _ = std::fs::remove_dir_all(path);
}

fn scenario(name: &str, mode: &str) {
    if std::env::var("WEAVER_GATE_STARTUP_DRIVER").ok().as_deref() == Some(mode) {
        let run = std::env::var("WEAVER_GATE_STARTUP_RUN").unwrap();
        if mode != "boundary" {
            std::fs::write(
                format!("/sys/fs/cgroup{run}/cgroup.procs"),
                std::process::id().to_string(),
            )
            .unwrap();
        }
        if mode.starts_with("entered-") {
            crate::flow::context::startup_composition::tests::entered_full();
        } else {
            crate::flow::context::startup_composition::tests::residency("full");
        }
        return;
    }
    let layout = crate::tools_cgroup::tests::ScratchLayout::new(mode);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--ignored", "--nocapture"])
        .env("WEAVER_GATE_STARTUP_DRIVER", mode)
        .env("WEAVER_GATE_STARTUP_RUN", &layout.run)
        .status()
        .unwrap();
    assert!(status.success(), "joined gate instrument failed");
}
macro_rules! scenario {
    ($name:ident,$mode:literal) => {
        #[test]
        #[ignore = "requires an owned delegated cgroup scope"]
        fn $name() {
            scenario(
                concat!(module_path!(), "::", stringify!($name))
                    .trim_start_matches("weaver_harness::"),
                $mode,
            );
        }
    };
}
scenario!(gate_startup_composition_ready, "ready");
scenario!(gate_startup_composition_shell, "shell");
scenario!(gate_startup_composition_delayed_ready, "delay");
scenario!(gate_startup_composition_early_ready, "early");
scenario!(gate_startup_composition_wrong_id, "wrong-id");
scenario!(gate_startup_composition_wrong_answer, "wrong-answer");
scenario!(gate_startup_composition_refusal, "refuse");
scenario!(gate_startup_composition_eof, "eof");
scenario!(gate_startup_composition_stop, "stop");
scenario!(gate_startup_composition_timeout, "silent");
scenario!(gate_startup_composition_finish_expiry, "finish-expiry");

scenario!(gate_startup_composition_unverified_boundary, "boundary");
scenario!(
    gate_startup_composition_expiry_before_fork,
    "pre-fork-expiry"
);
scenario!(gate_startup_composition_foreign_window, "foreign-window");
scenario!(gate_startup_composition_send_pressure, "pressure");

scenario!(
    gate_startup_composition_resident_foreign_context,
    "resident-foreign-context"
);
scenario!(
    gate_startup_composition_resident_wrong_instruction,
    "resident-wrong-instruction"
);
scenario!(
    gate_startup_composition_resident_foreign_bound,
    "resident-foreign-bound"
);
scenario!(
    gate_startup_composition_resident_boot_substitute,
    "resident-boot-substitute"
);
scenario!(
    gate_startup_composition_resident_boot_window,
    "resident-boot-window"
);
scenario!(
    gate_startup_composition_resident_stopped_boot,
    "resident-stopped-boot"
);
scenario!(
    gate_startup_composition_resident_gate_substitute,
    "resident-gate-substitute"
);

scenario!(
    gate_startup_composition_resident_spu_eof_boot,
    "resident-spu-eof-boot"
);
scenario!(
    gate_startup_composition_resident_spu_eof_gate,
    "resident-spu-eof-gate"
);

scenario!(gate_startup_composition_entered_ready, "entered-ready");

scenario!(
    gate_startup_composition_entered_eof_boot,
    "entered-eof-boot"
);
scenario!(
    gate_startup_composition_entered_substitute_boot,
    "entered-substitute-boot"
);
scenario!(
    gate_startup_composition_entered_eof_gate,
    "entered-eof-gate"
);
scenario!(
    gate_startup_composition_entered_substitute_gate,
    "entered-substitute-gate"
);
scenario!(
    gate_startup_composition_entered_wrong_gate,
    "entered-wrong-gate"
);
scenario!(
    gate_startup_composition_entered_finish_eof,
    "entered-finish-eof"
);

scenario!(
    gate_startup_composition_entered_finish_expiry_eof,
    "entered-finish-expiry-eof"
);

scenario!(
    gate_startup_composition_aggregate_ready,
    "entered-aggregate-ready"
);

scenario!(
    gate_startup_composition_aggregate_pressure,
    "entered-aggregate-pressure"
);

scenario!(
    gate_startup_composition_aggregate_stop,
    "entered-aggregate-stop"
);

scenario!(
    gate_startup_composition_aggregate_stop_pressure,
    "entered-aggregate-stop-pressure"
);

scenario!(
    gate_startup_composition_aggregate_eof,
    "entered-aggregate-eof"
);

scenario!(
    gate_startup_composition_aggregate_expiry,
    "entered-aggregate-expiry"
);

scenario!(
    gate_startup_composition_aggregate_gate_dead,
    "entered-aggregate-gate-dead"
);

scenario!(
    gate_startup_composition_aggregate_spu_dead,
    "entered-aggregate-spu-dead"
);

scenario!(
    gate_startup_composition_aggregate_clock_after_send,
    "entered-aggregate-clock-after-send"
);

scenario!(
    gate_startup_composition_aggregate_expiry_after_send,
    "entered-aggregate-expiry-after-send"
);

scenario!(
    gate_startup_composition_aggregate_busy,
    "entered-aggregate-busy"
);

scenario!(
    gate_startup_composition_aggregate_missing,
    "resident-aggregate-missing"
);

scenario!(
    gate_startup_composition_aggregate_gate_terminal,
    "entered-aggregate-gate-terminal"
);

scenario!(
    gate_startup_composition_aggregate_rejects_plain_ready,
    "entered-aggregate-plain-ready"
);
