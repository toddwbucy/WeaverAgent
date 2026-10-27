use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::tests::{Member, reap, worker};
use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, UnixAddr, connect, recv, send,
    sendmsg, socket,
};
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::{Cause, ExchangeId};

#[path = "context_worker_secondary_leave_tests.rs"]
pub(in crate::flow::context::worker_startup) mod secondary;

pub(in crate::flow::context::worker_startup) struct Guard<'a> {
    pub(in crate::flow::context::worker_startup) input: Option<Box<FoundationInput<'a>>>,
    pub(in crate::flow::context::worker_startup) management: Option<Box<FoundationManagement<'a>>>,
}
impl Guard<'_> {
    pub(in crate::flow::context::worker_startup) fn owned_input(&self) -> &FoundationInput<'_> {
        self.input
            .as_deref()
            .or_else(|| self.management.as_deref().map(|m| &*m.input))
            .expect("fixture retains its input owner")
    }

    pub(in crate::flow::context::worker_startup) fn recorded_sequence(
        &self,
    ) -> weaver_trace::Sequence {
        self.input
            .as_deref()
            .or_else(|| self.management.as_deref().map(|m| &*m.input))
            .expect("fixture retains its input owner")
            .serving
            .owner
            .context
            .last_author_sequence
            .expect("fixture has Recorded events")
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let input = self
            .input
            .as_deref_mut()
            .or_else(|| self.management.as_deref_mut().map(|m| &mut *m.input));
        if let Some(input) = input {
            reap(&mut input.serving.owner.gate_child);
        }
    }
}
pub(in crate::flow::context::worker_startup) fn dial(path: &Path) -> OwnedFd {
    let fd = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .unwrap();
    connect(fd.as_raw_fd(), &UnixAddr::new(path).unwrap()).unwrap();
    fd
}
pub(in crate::flow::context::worker_startup) fn read_packet(fd: &OwnedFd) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(fd.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(n) if n > 0 => {
            bytes.truncate(n);
            Some(bytes)
        }
        Ok(_) | Err(nix::errno::Errno::EAGAIN) => None,
        Err(e) => panic!("{e}"),
    }
}
pub(in crate::flow::context::worker_startup) fn trace(
    reader: &mut UnixStream,
) -> Vec<serde_json::Value> {
    reader.set_nonblocking(true).unwrap();
    let mut bytes = vec![];
    let end = Instant::now() + Duration::from_millis(40);
    while Instant::now() < end {
        let _ = reader.read_to_end(&mut bytes);
        std::thread::sleep(Duration::from_millis(1));
    }
    bytes
        .split(|b| *b == b'\n')
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_slice(s).unwrap())
        .collect()
}

pub(in super::super::super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let real = mode == "input-worker-observe";
    let checking = mode.ends_with("checking") || mode.ends_with("expiry");
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let payload = guard
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
        .enter()
        .clone();
    let mut target = FoundationInputProgress::Accepted;
    for _ in 0..5000 {
        let input = guard.input.as_mut().unwrap();
        let progress = if real {
            input.advance()
        } else {
            input.advance_with_io(&mut CircuitIo(clock))
        };
        if (checking && input.stage == Stage::Checking) || progress == target {
            break;
        }
        assert_eq!(
            progress,
            FoundationInputProgress::Pending,
            "{:?}",
            input.detail()
        );
        if real {
            member.step_live("ready");
        } else {
            member.step(clock.now().unwrap(), "ready");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if checking {
        assert!(guard.input.as_ref().unwrap().stage == Stage::Checking);
    } else {
        assert_eq!(guard.input.as_ref().unwrap().progress(), target);
    }
    if mode.ends_with("active") {
        guard
            .input
            .as_mut()
            .unwrap()
            .start_turn(TurnKey("observed-turn".into()))
            .unwrap();
        target = FoundationInputProgress::Turn;
        for _ in 0..5000 {
            let progress = guard
                .input
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock));
            if progress != FoundationInputProgress::Pending {
                assert_eq!(progress, target);
                break;
            }
            member.step(clock.now().unwrap(), "ready");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(guard.input.as_ref().unwrap().progress(), target);
    }
    let stage = guard.input.as_ref().unwrap().stage;
    // A premature workflow call must return custody rather than panic/drop it.
    guard.input = Some(
        worker::management(guard.input.take().unwrap())
            .ok()
            .unwrap(),
    );
    assert!(guard.input.as_ref().unwrap().stage == stage);
    let deadline = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .owner
        .context
        .deadlines
        .get(super::super::super::super::PumpDeadline::Gate);
    let mut records = trace(reader);
    // A separate real admin process must pass the production UID/pidfd/OFD lock checks.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(root.join("run.lock"))
        .unwrap();
    let request = nix::libc::flock {
        l_type: nix::libc::F_WRLCK as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    assert_eq!(
        unsafe { nix::libc::fcntl(lock.as_raw_fd(), nix::libc::F_OFD_SETLK, &request) },
        0
    );
    let mut admin = if real {
        Some(
            std::process::Command::new(
                std::env::var("WEAVER_OBSERVE_ADMIN_TEST_BIN")
                    .expect("explicit independently built admin test binary"),
            )
            .args([
                "--exact",
                "serving_observe::tests::replacement_observe_actual_admin_process",
                "--nocapture",
            ])
            .env("WEAVER_OBSERVE_CIRCUIT", root)
            .spawn()
            .unwrap(),
        )
    } else {
        None
    };
    let peer = (!real).then(|| dial(socket));
    if let Some(peer) = &peer {
        let directive = if mode.ends_with("leave") {
            LifecycleDirective::Leave {
                cause: Cause { uid: 73 },
                forced: false,
                rollback: false,
            }
        } else if mode.ends_with("stop") {
            LifecycleDirective::Stop {
                cause: Cause { uid: 73 },
            }
        } else if mode.ends_with("save") {
            LifecycleDirective::SavePoint {
                cause: Cause { uid: 73 },
            }
        } else {
            LifecycleDirective::Observe
        };
        let request = OrganEnvelope {
            exchange: ExchangeId {
                opener: if mode.ends_with("opener") {
                    Opener::Harness
                } else {
                    Opener::Admin
                },
                ordinal: 73,
            },
            position: if mode.ends_with("position") {
                Position::Continue
            } else {
                Position::Open
            },
            payload: Payload::Directive(directive),
        };
        if !mode.ends_with("expiry") && !mode.ends_with("eof") {
            let bytes = if mode.ends_with("save") {
                let began = clock.now().unwrap();
                let wait = guard
                    .input
                    .as_ref()
                    .unwrap()
                    .serving
                    .owner
                    .context
                    .load_duty
                    .as_ref()
                    .unwrap()
                    .persistence
                    .save_wait
                    .elapsed_ns
                    .0;
                CaptureEnvelope {
                    exchange: request.exchange.clone(),
                    position: request.position,
                    payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                        cause: Cause { uid: 73 },
                        window: weaver_types::state_flow::SaveWindow {
                            began_ns: began.into(),
                            deadline_ns: began.checked_add(wait).unwrap().into(),
                        },
                    }),
                }
                .encode(weaver_types::state_flow::capture::CaptureSender::Admin)
                .unwrap()
            } else if mode.ends_with("bad") {
                b"{".to_vec()
            } else {
                serde_json::to_vec(&request).unwrap()
            };
            let fds = [lock.as_raw_fd()];
            let handles = if mode.ends_with("handles") {
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
        }
    }
    for _ in 0..5000 {
        let p = if real {
            guard.input.as_mut().unwrap().advance()
        } else {
            guard
                .input
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        };
        if p == FoundationInputProgress::ManagementPending {
            break;
        }
        assert_ne!(p, FoundationInputProgress::Stopped);
        if real {
            member.step_live("ready");
        } else {
            member.step(clock.now().unwrap(), "ready");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        guard.input.as_ref().unwrap().progress(),
        FoundationInputProgress::ManagementPending
    );
    if real {
        let done = std::sync::atomic::AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            let responder = scope.spawn(|| {
                let end = Instant::now() + Duration::from_secs(8);
                while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                    member.step_live("ready");
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let result = worker::management(guard.input.take().unwrap());
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            responder.join().unwrap();
            result
        });
        match result {
            Ok(input) => guard.input = Some(input),
            Err(m) => guard.management = Some(m),
        }
        assert!(
            guard.input.is_some(),
            "{:?}",
            guard.management.as_ref().and_then(|m| m.detail())
        );
    } else {
        guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
        let m = guard.management.as_mut().unwrap();
        if mode.ends_with("pressure") {
            let fd = m.peer.fd().as_raw_fd();
            let mut count = 0;
            loop {
                match send(
                    fd,
                    &[b'x'; 4096],
                    MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                ) {
                    Ok(4096) => count += 1,
                    Err(nix::errno::Errno::EAGAIN) => break,
                    other => panic!("{other:?}"),
                }
            }
            assert!(count > 0);
        }
        if mode.ends_with("eof") {
            nix::sys::socket::shutdown(
                peer.as_ref().unwrap().as_raw_fd(),
                nix::sys::socket::Shutdown::Both,
            )
            .unwrap();
        }
        if mode.ends_with("expiry") {
            clock.advance_to(deadline.unwrap().deadline_ns()).unwrap();
        }
        let mut pressure = false;
        for _ in 0..5000 {
            let m = guard.management.as_mut().unwrap();
            let progress = m.advance_with_io(&mut CircuitIo(clock));
            if mode.ends_with("pressure") && m.send.as_ref().is_some_and(|s| !s.sent()) {
                for _ in 0..3 {
                    assert_eq!(
                        m.advance_with_io(&mut CircuitIo(clock)),
                        FoundationManagementProgress::Pending
                    );
                }
                assert!(!m.send.as_ref().unwrap().sent());
                pressure = true;
                while let Some(bytes) = read_packet(peer.as_ref().unwrap()) {
                    assert!(bytes.iter().all(|b| *b == b'x'));
                }
            }
            if progress != FoundationManagementProgress::Pending {
                break;
            }
            member.step(clock.now().unwrap(), "ready");
            std::thread::sleep(Duration::from_millis(1));
        }
        let m = guard.management.as_mut().unwrap();
        if mode.ends_with("expiry") {
            assert_eq!(m.progress(), FoundationManagementProgress::Stopped);
            assert!(
                m.input
                    .serving
                    .owner
                    .context
                    .input
                    .snapshot()
                    .unwrap()
                    .request_id
                    .is_none()
            );
            return;
        }
        if ["leave", "save"].iter().any(|s| mode.ends_with(s)) {
            assert_eq!(m.progress(), FoundationManagementProgress::DirectivePending);
            let exchange = if mode.ends_with("save") {
                &m.peer.capture_held().unwrap().envelope.exchange
            } else {
                &m.peer.held().unwrap().envelope.exchange
            };
            assert_eq!(exchange.ordinal, 73);
            assert!(m.message.is_none() && m.save_message.is_none() && m.send.is_none());
            assert!(read_packet(peer.as_ref().unwrap()).is_none());
            return;
        }
        if mode.ends_with("stop") {
            assert_eq!(m.progress(), FoundationManagementProgress::Answered);
            assert!(m.stop.is_none());
            let bytes = read_packet(peer.as_ref().unwrap()).unwrap();
            let answer: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(answer.exchange.ordinal, 73);
            assert_eq!(answer.payload, Payload::Answer(LifecycleAnswer::AtRest));
            assert!(trace(reader).is_empty());
            guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
            assert!(guard.input.as_ref().unwrap().stage == stage);
            return;
        }
        let dropped = ["opener", "position", "handles", "bad", "eof"]
            .iter()
            .any(|s| mode.ends_with(s));
        assert_eq!(
            m.progress(),
            if dropped {
                FoundationManagementProgress::Dropped
            } else {
                FoundationManagementProgress::Answered
            },
            "{:?}",
            m.detail()
        );
        if mode.ends_with("pressure") {
            assert!(pressure);
        }
        if !dropped {
            let bytes =
                read_packet(peer.as_ref().unwrap()).expect("whole actual observation answer");
            let reply = ObservationEnvelope::decode(&bytes).unwrap();
            assert_eq!(reply.exchange.ordinal, 73);
            std::fs::write(
                root.join("admin-observed.json"),
                serde_json::to_vec(&reply.payload).unwrap(),
            )
            .unwrap();
        }
        guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
        if dropped {
            assert!(guard.input.as_ref().unwrap().stage == stage);
            return;
        }
    }
    if let Some(admin) = admin.as_mut() {
        assert!(admin.wait().unwrap().success());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("admin-observed.json")).unwrap()).unwrap();
    let answer = if real { value } else { value["body"].clone() };
    assert_eq!(answer["kind"], "state");
    assert_eq!(
        answer["state"],
        if mode.ends_with("active") {
            "active"
        } else {
            "idle"
        }
    );
    let loaded = &answer["load"];
    assert_eq!(loaded["declaration"], payload.declaration);
    assert_eq!(loaded["session"], payload.session.0);
    assert_eq!(loaded["run"], payload.run.0);
    assert_eq!(loaded["state_member"], true);
    assert_eq!(loaded["state_store"]["engine"], "sqlite");
    assert_eq!(loaded["composer"]["binary"], "worker");
    let weaver_types::state_flow::enter::EnterBinding::Serving {
        state_management,
        start,
        ste100_scheme,
        ..
    } = payload.binding
    else {
        panic!()
    };
    assert_eq!(
        loaded["state"]["serving"]["state_management"],
        serde_json::to_value(state_management).unwrap()
    );
    assert_eq!(
        loaded["state"]["serving"]["start"],
        serde_json::to_value(start).unwrap()
    );
    assert_eq!(loaded["state"]["serving"]["ste100_scheme"], ste100_scheme);
    assert_eq!(
        loaded["state"]["serving"]["ste100_binary_sha256"],
        payload.stack["checker"]
    );
    assert!(guard.input.as_ref().unwrap().stage == stage);
    assert_eq!(
        guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .owner
            .context
            .deadlines
            .get(super::super::super::super::PumpDeadline::Gate),
        deadline
    );
    let after = trace(reader);
    assert!(after.is_empty(), "Observe must author nothing: {after:?}");
    let load = records.iter().find(|r| r["kind"] == "load").unwrap();
    assert_eq!(
        load["payload"]["ste100_binary_sha256"],
        loaded["state"]["serving"]["ste100_binary_sha256"]
    );
    if !mode.ends_with("active") {
        assert!(
            records
                .iter()
                .all(|r| r["kind"] != "turn.started" && r["kind"] != "message.user")
        );
        // Resume original checked input through source authoring, with no second offer.
        for _ in 0..5000 {
            let input = guard.input.as_mut().unwrap();
            if input.progress() == FoundationInputProgress::Accepted {
                input.start_turn(TurnKey("resumed-turn".into())).unwrap();
            }
            let p = if real {
                input.advance()
            } else {
                input.advance_with_io(&mut CircuitIo(clock))
            };
            if p == FoundationInputProgress::Turn {
                break;
            }
            assert!(
                matches!(
                    p,
                    FoundationInputProgress::Pending | FoundationInputProgress::Accepted
                ),
                "{:?}",
                input.detail()
            );
            if real {
                member.step_live("ready");
            } else {
                member.step(clock.now().unwrap(), "ready");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            guard.input.as_ref().unwrap().progress(),
            FoundationInputProgress::Turn
        );
        records.extend(trace(reader));
        assert_eq!(
            records
                .iter()
                .filter(|r| r["kind"] == "input.offered")
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|r| r["kind"] == "message.user")
                .count(),
            1
        );
    }
}

pub(in crate::flow::context::worker_startup) fn halt_facts(
    owner: &FoundationManagement<'_>,
) -> (usize, i32, OrganEnvelope, bool) {
    (
        owner as *const _ as usize,
        owner.peer.fd().as_raw_fd(),
        owner.message.as_ref().unwrap().envelope.clone(),
        owner.send.as_ref().unwrap().sent(),
    )
}

pub(in crate::flow::context::worker_startup) fn hold_for_halt(
    guard: &mut Guard<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
    socket: &Path,
    mode: &str,
) -> OwnedFd {
    let peer = dial(socket);
    let frame = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 301,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Observe),
    };
    let bytes = serde_json::to_vec(&frame).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    for _ in 0..1000 {
        if guard
            .input
            .as_mut()
            .unwrap()
            .advance_with_io(&mut CircuitIo(clock))
            == FoundationInputProgress::ManagementPending
        {
            break;
        }
        member.step(clock.now().unwrap(), mode);
        std::thread::sleep(Duration::from_millis(1));
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let owner = guard.management.as_mut().unwrap();
    loop {
        match send(
            owner.peer.fd().as_raw_fd(),
            &[b'x'; 4096],
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(4096) => {}
            Err(nix::errno::Errno::EAGAIN) => break,
            other => panic!("fill retained Observe: {other:?}"),
        }
    }
    for _ in 0..1000 {
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationManagementProgress::Pending
        );
        if owner.send.is_some() {
            break;
        }
        member.step(clock.now().unwrap(), mode);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(owner.message.as_ref().unwrap().envelope, frame);
    assert!(!owner.send.as_ref().unwrap().sent());
    peer
}
