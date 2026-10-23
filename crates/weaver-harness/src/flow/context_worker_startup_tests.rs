//! One worker owner crosses all actual sockets and forks. Member/classifier/model
//! replies are declared doubles, not production language or model qualification.
use super::*;
use crate::flow::clock::{PumpIo, testing::FakePumpIo};
use crate::flow::context::failed_enter_driver::{ChildState, OwnedChild};
use crate::flow::context::{edge_tests::classifier_fixture, template_tests, tests::Fixture};
use crate::flow::coordinator::tests::operation;
use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, UnixAddr, connect, recv, sendmsg,
    socket,
};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;

#[path = "../bin/worker/dev_loop/foundation.rs"]
pub(super) mod worker;

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("weaver-worker-startup-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn child(&self, name: &str, variable: &str, mode: &str, test: &str) -> PathBuf {
        let path = self.0.join(name);
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nexport WEAVER_WORKER_INPUT_ROOT={}\nexport {}={}\nexec {} --exact {} --nocapture\n",
                quote(self.0.to_str().unwrap()),
                variable,
                quote(mode),
                quote(std::env::current_exe().unwrap().to_str().unwrap()),
                quote(test),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn program(&self, mode: &str) -> FoundationProgram {
        FoundationProgram {
            organs: OrganBinaries {
                classify: Some(self.child(
                    "checker",
                    "WEAVER_STARTUP_COMPOSITION_CHILD",
                    if mode.starts_with("input-") {
                        mode
                    } else if mode == "classifier-refusal" {
                        "refuse"
                    } else {
                        "ready"
                    },
                    "flow::context::startup_composition::tests::startup_composition_child",
                )),
                spu: self.child(
                    "spu",
                    "WEAVER_RESIDENCY_COMPOSITION_CHILD",
                    if mode == "spu-refusal" {
                        "refuse"
                    } else {
                        "full"
                    },
                    "flow::context::residency_composition::tests::residency_composition_child",
                ),
                gate: self.child(
                    "gate",
                    "WEAVER_GATE_STARTUP_CHILD",
                    if mode.starts_with("input-") {
                        mode
                    } else if mode == "gate-refusal" {
                        "refuse"
                    } else {
                        "ready"
                    },
                    if mode.starts_with("input-") {
                        "flow::context::worker_startup::input::tests::worker_input_gate_child"
                    } else {
                        "flow::context::boot_composition::gate_startup::tests::gate_startup_child"
                    },
                ),
            },
            parameters: OrganParameters {
                headroom_bytes: Some("123456".into()),
            },
            composer: LoopIdentity::compiled("worker"),
            templates: template_tests::templates(),
        }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn dial(path: &std::path::Path) -> OwnedFd {
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
fn packet(peer: &OwnedFd) -> Option<weaver_types::OrganEnvelope> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(n) if n > 0 => Some(serde_json::from_slice(&bytes[..n]).unwrap()),
        Ok(_) | Err(nix::errno::Errno::EAGAIN) => None,
        Err(error) => panic!("admin receive: {error}"),
    }
}

#[test]
fn worker_startup_unprivileged_dial() {
    let Ok(path) = std::env::var("WEAVER_WORKER_UNPRIVILEGED_DIAL") else {
        return;
    };
    assert_eq!(nix::unistd::getuid().as_raw(), 65534);
    let peer = dial(std::path::Path::new(&path));
    let mut byte = [0];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut byte, MsgFlags::empty()).unwrap(),
        0
    );
}

fn refuse_unprivileged(admission: &mut FoundationAdmission, path: &std::path::Path) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666)).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "flow::context::worker_startup::tests::worker_startup_unprivileged_dial",
            "--nocapture",
        ])
        .env("WEAVER_WORKER_UNPRIVILEGED_DIAL", path)
        .uid(65534)
        .gid(65534)
        .spawn()
        .unwrap();
    let limit = Instant::now() + Duration::from_secs(3);
    let mut status = None;
    while Instant::now() < limit {
        admission.advance();
        status = child.try_wait().unwrap();
        if status.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if status.is_none() {
        child.kill().unwrap();
    }
    assert!(child.wait().unwrap().success());
    assert!(admission.receiver.is_none() && admission.received.is_none());
    assert!(admission.detail().is_none());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

pub(super) fn reap(child: &mut OwnedChild) {
    child.stop_at_cap();
    let limit = Instant::now() + Duration::from_secs(3);
    while child.poll() == ChildState::Running && Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_ne!(child.state(), ChildState::Running);
}
struct RunGuard(FoundationRun);
impl Drop for RunGuard {
    fn drop(&mut self) {
        self.0.resources.reap_startup_test_children();
    }
}
struct EnterGuard<'a>(Option<FoundationEnter<'a>>);
impl Drop for EnterGuard<'_> {
    fn drop(&mut self) {
        let child = match self.0.as_mut().and_then(|owner| owner.state.as_mut()) {
            Some(EnterState::Gate(owner)) => owner.test_gate_child(),
            Some(EnterState::Raised(owner)) => owner.test_gate_child(),
            Some(EnterState::Ready(owner)) => owner.test_gate_child(),
            Some(EnterState::Entered(owner)) => Some(&mut owner.gate_child),
            _ => None,
        };
        if let Some(child) = child {
            reap(child);
        }
    }
}
struct ServingGuard<'a>(FoundationServing<'a>);
impl Drop for ServingGuard<'_> {
    fn drop(&mut self) {
        reap(&mut self.0.owner.gate_child);
    }
}

pub(super) struct Member {
    peer: UnixStream,
    bytes: Vec<u8>,
    seed: String,
    ordinal: u64,
    requests: Vec<String>,
}
impl Member {
    pub(super) fn step(&mut self, now: u64, mode: &str) {
        let mut part = [0; 65536];
        match self.peer.read(&mut part) {
            Ok(n) => self.bytes.extend_from_slice(&part[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(e) => panic!("member: {e}"),
        }
        while let Some(end) = self.bytes.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.bytes.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            self.ordinal += 1;
            let observation = operation(&request, self.ordinal, now);
            let response = match &request {
                s::StateFrame::Open {
                    schema_id,
                    scheme_id,
                    start,
                    read_limits,
                    ..
                } => {
                    assert!(self.requests.is_empty());
                    self.requests.push("open".into());
                    if mode == "open-refusal" {
                        s::StateFrame::Refused {
                            common: request.common().clone(),
                            code: s::StateRefusalCode::OpenFailed,
                            detail: "worker Open refusal".into(),
                            block_id: None,
                            observation,
                        }
                    } else {
                        s::StateFrame::Opened {
                            common: request.common().clone(),
                            schema_id: schema_id.clone(),
                            scheme_id: scheme_id.clone(),
                            start: start.clone(),
                            read_limits: read_limits.clone(),
                            observation,
                            member_observation: s::MemberObservationIdentity {
                                instrument_version: "state-observation/2".into(),
                                pid: 7,
                                start_ticks: Some(10.into()),
                                ticks_per_second: Some(100.into()),
                                unavailable_reason: None,
                            },
                        }
                    }
                }
                s::StateFrame::Control { frame, .. } => {
                    let frame = match frame {
                        s::StateControl::Grants {} => {
                            assert_eq!(self.requests, ["open"]);
                            self.requests.push("grants".into());
                            s::StateControlAnswer::Grants {
                                surface: vec!["owner:actual".into()],
                            }
                        }
                        s::StateControl::Read { selector } => {
                            let raw = match selector {
                                s::ReadSelector::SystemPrompt {} => {
                                    self.requests.push("prompt".into());
                                    super::super::boot_composition::tests::prompt(&self.seed)
                                }
                                s::ReadSelector::OpenPairs {} => {
                                    self.requests.push("pairs".into());
                                    br#"{"open_pairs":{"pairs":[]}}"#.to_vec()
                                }
                                s::ReadSelector::PlanCurrent {} => {
                                    self.requests.push("summary".into());
                                    br#"{"plan_current":{"current":null,"next_plan_id":"40","next_step_id":"50"}}"#.to_vec()
                                }
                                _ => panic!("unexpected read {selector:?}"),
                            };
                            if mode == "prompt-refusal" {
                                s::StateControlAnswer::ControlRefused {
                                    code: s::ControlRefusalCode::Read(
                                        s::ReadRefusalCode::InvalidSelector,
                                    ),
                                    detail: "worker prompt refusal".into(),
                                }
                            } else {
                                super::super::read_tests::page(
                                    self.ordinal + 10,
                                    0,
                                    raw.len(),
                                    &raw,
                                )
                            }
                        }
                        s::StateControl::Release { snapshot_id } => {
                            self.requests.push("release".into());
                            s::StateControlAnswer::Released {
                                snapshot_id: *snapshot_id,
                            }
                        }
                        _ => panic!("unexpected control {frame:?}"),
                    };
                    s::StateFrame::ControlAnswer {
                        common: request.common().clone(),
                        frame,
                        observation,
                    }
                }
                s::StateFrame::File { common, filing } => {
                    self.requests.push("file".into());
                    s::StateFrame::Filed {
                        common: common.clone(),
                        block_id: filing.block_id.clone(),
                        observation,
                    }
                }
                _ => panic!("unexpected request {request:?}"),
            };
            self.peer
                .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
                .unwrap();
        }
    }
}

fn circuit(mode: &str) {
    assert_eq!(
        nix::unistd::getuid().as_raw(),
        0,
        "actual root-peer fixture"
    );
    let files = Files::new();
    let mut program = files.program(mode);
    let path = files.0.join("admin.sock");
    let mut admission =
        FoundationAdmission::listen(crate::bind_coordination(&path).unwrap()).unwrap();
    assert_eq!(admission.advance(), FoundationAdmissionProgress::Pending);
    if mode == "unprivileged" {
        refuse_unprivileged(&mut admission, &path);
    }
    let peer = dial(&path);
    let (sink, mut reader) = UnixStream::pair().unwrap();
    let (state, member) = UnixStream::pair().unwrap();
    let (bootstrap, scheme, _) = classifier_fixture();
    let mut config = super::super::tests::config();
    config.scheme = scheme;
    if mode.starts_with("input-worker") {
        // The ordinary fixture's 1 ms bound is for paired fake I/O. This real
        // socket/clock circuit declares its own ample, unqualified test budget.
        config.gate_wait.elapsed_ns = 5_000_000_000.into();
        config.observation_interval.elapsed_ns = 100_000_000.into();
    }
    let fixture = Fixture::new(config.clone());
    let event = fixture.load_event();
    let Some(weaver_trace::serving::Payload::Elections(mut load)) = event.payload else {
        panic!()
    };
    load.state_flow.gate_wait.elapsed_ns = config.gate_wait.elapsed_ns.0.into();
    let envelope = super::super::startup_composition::tests::entered_tests::envelope(
        &config,
        &load,
        &bootstrap,
        program.organs.classify.as_ref().unwrap(),
    );
    let bytes = envelope.encode().unwrap();
    let wire = if mode == "malformed" {
        b"{bad".to_vec()
    } else {
        bytes.clone()
    };
    let handles = if mode == "missing-state" {
        vec![sink.as_raw_fd()]
    } else {
        vec![sink.as_raw_fd(), state.as_raw_fd()]
    };
    assert_eq!(
        sendmsg::<()>(
            peer.as_raw_fd(),
            &[std::io::IoSlice::new(&wire)],
            &[ControlMessage::ScmRights(&handles)],
            MsgFlags::MSG_NOSIGNAL,
            None
        )
        .unwrap(),
        wire.len()
    );
    drop((sink, state));
    let received = admission.advance();
    if mode == "malformed" {
        assert_eq!(received, FoundationAdmissionProgress::Stopped);
        let detail = admission.detail().unwrap().to_owned();
        assert_eq!(admission.advance(), FoundationAdmissionProgress::Stopped);
        assert_eq!(admission.detail(), Some(detail.as_str()));
        assert!(admission.adopt(1_000_000_000).is_err());
        assert!(packet(&peer).is_none());
        return;
    }
    assert_eq!(received, FoundationAdmissionProgress::Received);
    assert_eq!(admission.received.as_ref().unwrap().wire(), bytes);
    assert_eq!(admission.advance(), FoundationAdmissionProgress::Received);
    let mut clock = FakePumpIo::new(100);
    let adopted = if mode == "worker" || mode.starts_with("input-worker") {
        admission.adopt(5_000_000_000)
    } else {
        admission.adopt_with_io(1_000_000_000, &mut clock)
    };
    if mode == "missing-state" {
        let admission = adopted.err().expect("missing handle refused");
        assert_eq!(admission.received.as_ref().unwrap().wire(), bytes);
        assert!(
            admission
                .detail()
                .unwrap()
                .contains("requires trace and state")
        );
        assert!(packet(&peer).is_none());
        return;
    }
    let mut run = RunGuard(adopted.map_err(|a| a.detail).ok().unwrap());
    if matches!(mode, "missing-checker" | "bad-checker-path") {
        program.organs.classify = if mode == "missing-checker" {
            None
        } else {
            Some(PathBuf::from("/synthetic/not-in-enter-stack"))
        };
        assert!(run.0.begin(program).is_err());
        assert_eq!(run.0.original.as_ref().unwrap().wire(), bytes);
        assert!(run.0.resources.classifier_child().is_none());
        program = files.program("ready");
    }
    let bound = run
        .0
        .context
        .deadlines
        .get(super::super::PumpDeadline::Enter)
        .unwrap();
    assert_eq!(
        bound.deadline_ns() - bound.began_ns(),
        if mode == "worker" || mode.starts_with("input-worker") {
            5_000_000_000
        } else {
            1_000_000_000
        }
    );
    member.set_nonblocking(true).unwrap();
    let mut member = Member {
        peer: member,
        bytes: vec![],
        seed: load.state_start.seed_id.clone(),
        ordinal: 0,
        requests: vec![],
    };
    if mode == "worker" || mode.starts_with("input-worker") {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = done.clone();
        let responder = std::thread::spawn(move || {
            let limit = Instant::now() + Duration::from_secs(8);
            while !flag.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < limit {
                member.step(MonotonicIo.now().unwrap(), "ready");
                std::thread::sleep(Duration::from_millis(1));
            }
            member
        });
        // SAFETY: the fixture keeps this serving thread alive through actual reap.
        let result = unsafe { worker::enter(&mut run.0, program) }.unwrap();
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        member = responder.join().unwrap();
        let mut serving = match result {
            Ok(serving) => serving,
            Err(owner) => {
                let guard = EnterGuard(Some(owner));
                panic!("worker refused: {:?}", guard.0.as_ref().unwrap().detail());
            }
        };
        assert_serving(&mut serving, &peer, &bytes, &member, bound);
        if mode.starts_with("input-worker") {
            drop(peer);
            if mode.contains("stop-") {
                super::input::management::stop::tests::exercise(
                    serving,
                    &mut member,
                    &mut reader,
                    &mut clock,
                    &path,
                    &files.0,
                    mode,
                );
                return;
            }
            if mode.contains("observe") {
                super::input::management::tests::exercise(
                    serving,
                    &mut member,
                    &mut reader,
                    &mut clock,
                    &path,
                    &files.0,
                    mode,
                );
                return;
            }
            super::input::tests::exercise(
                serving,
                &mut member,
                &mut reader,
                &mut clock,
                &path,
                &files.0,
                mode,
            );
        } else {
            let _guard = ServingGuard(serving);
        }
        return;
    }
    let mut guard = EnterGuard(Some(run.0.begin(program).unwrap()));
    let mut owner = guard.0.take().unwrap();
    owner = owner
        .finish()
        .err()
        .expect("unfinished Enter cannot grant a seat");
    guard.0 = Some(owner);
    let mut management = None;
    if mode == "expiry" {
        clock.advance_to(bound.deadline_ns()).unwrap();
    }
    if mode == "original-eof" {
        nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Write).unwrap();
    }
    if mode == "original-stop" {
        let stop = weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 8,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 73 },
            }),
        };
        let bytes = serde_json::to_vec(&stop).unwrap();
        nix::sys::socket::send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap();
    }
    for _ in 0..5000 {
        let owner = guard.0.as_mut().unwrap();
        if management.is_none()
            && (mode == "management-startup"
                || mode == "management-gate" && matches!(owner.state, Some(EnterState::Gate(_))))
        {
            management = Some(dial(&path));
        }
        // SAFETY: the guard reaps the gate, then RunGuard reaps SPU/classifier.
        let progress = unsafe {
            owner.advance_with_io(&mut super::super::startup_composition::tests::CircuitIo(
                &mut clock,
            ))
        };
        if progress != FoundationEnterProgress::Pending {
            break;
        }
        member.step(clock.now().unwrap(), mode);
        std::thread::sleep(Duration::from_millis(1));
    }
    let owner = guard.0.as_mut().unwrap();
    let successful = matches!(
        mode,
        "ready" | "missing-checker" | "bad-checker-path" | "unprivileged"
    ) || mode.starts_with("management-")
        || mode.starts_with("input-");
    if successful {
        assert_eq!(
            owner.progress(),
            FoundationEnterProgress::Ready,
            "{:?}",
            owner.detail()
        );
        let mut serving = guard.0.take().unwrap().finish().ok().unwrap();
        assert_serving(&mut serving, &peer, &bytes, &member, bound);
        if mode.starts_with("input-") {
            drop(peer); // Normal admin departure does not revoke serving.
            if mode.contains("stop-") {
                super::input::management::stop::tests::exercise(
                    serving,
                    &mut member,
                    &mut reader,
                    &mut clock,
                    &path,
                    &files.0,
                    mode,
                );
                return;
            }
            if mode.contains("observe") {
                super::input::management::tests::exercise(
                    serving,
                    &mut member,
                    &mut reader,
                    &mut clock,
                    &path,
                    &files.0,
                    mode,
                );
                return;
            }
            super::input::tests::exercise(
                serving,
                &mut member,
                &mut reader,
                &mut clock,
                &path,
                &files.0,
                mode,
            );
            return;
        }
        let mut serving = ServingGuard(serving);
        if let Some(management) = management {
            // The pump serializes lifecycle peers. No second receiver ran while
            // original Enter owned the primary slot, even during gate Raise.
            assert!(packet(&management).is_none());
            assert!(serving.0.owner.listener.held().is_none());
            let queued = serving
                .0
                .owner
                .listener
                .accept_enter()
                .unwrap()
                .expect("queued root peer retained");
            assert!(queued.held().is_none());
        }
        // A later clock and normal admin close cannot revoke the completed seat.
        drop(peer);
        clock.advance_to(bound.deadline_ns() + 1).unwrap();
        assert!(
            serving
                .0
                .owner
                .context
                .deadlines
                .get(super::super::PumpDeadline::Enter)
                .is_none()
        );
    } else {
        assert_eq!(
            owner.progress(),
            FoundationEnterProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        let detail = owner.detail().unwrap().to_owned();
        assert!(
            matches!(
                (&owner.state, mode),
                (
                    Some(EnterState::Startup(_)),
                    "open-refusal"
                        | "classifier-refusal"
                        | "original-eof"
                        | "original-stop"
                        | "expiry"
                ) | (Some(EnterState::Residency(_)), "spu-refusal")
                    | (Some(EnterState::Boot(_)), "prompt-refusal")
                    | (Some(EnterState::Gate(_)), "gate-refusal")
            ),
            "stopped stage lost actual owner"
        );
        // SAFETY: stopped owner remains held and fixture reaps its children.
        assert_eq!(
            unsafe { owner.advance_with_io(&mut clock) },
            FoundationEnterProgress::Stopped
        );
        assert_eq!(owner.detail(), Some(detail.as_str()));
        assert!(packet(&peer).is_none());
        assert!(detail.contains(match mode {
            "open-refusal" | "classifier-refusal" | "original-eof" | "original-stop" | "expiry" =>
                "startup",
            "spu-refusal" => "residency",
            "prompt-refusal" => "boot",
            "gate-refusal" => "gate",
            _ => panic!("unknown case"),
        }));
    }
    reader.set_nonblocking(true).unwrap();
    let mut trace = vec![];
    let _ = reader.read_to_end(&mut trace);
    assert_eq!(
        trace
            .split(|b| *b == b'\n')
            .filter(|line| {
                serde_json::from_slice::<serde_json::Value>(line)
                    .is_ok_and(|row| row["kind"] == "load")
            })
            .count(),
        usize::from(!matches!(
            mode,
            "open-refusal" | "original-eof" | "original-stop" | "expiry"
        ))
    );
}

fn assert_serving(
    serving: &mut FoundationServing<'_>,
    peer: &OwnedFd,
    wire: &[u8],
    member: &Member,
    bound: crate::flow::clock::AbsoluteBound,
) {
    let owner = &mut serving.owner;
    assert_eq!(owner.enter, bound);
    assert!(owner.ready.sent() && owner.gate_send.sent());
    assert!(owner.gate_reply.belongs_to(&owner.gate));
    assert_eq!(
        owner
            .resident
            .evidence
            .startup
            .original_enter
            .as_ref()
            .unwrap()
            .wire(),
        wire
    );
    assert_eq!(
        owner.resident.evidence.request.arguments,
        ["--headroom-bytes", "123456"]
    );
    assert_eq!(owner.certificate.accepted().len(), 3);
    assert!(owner.context.place_engine.is_some());
    assert!(owner.context.committed_plan_summary().is_ok());
    assert_eq!(owner.resident.spu.state(), ChildState::Running);
    assert_eq!(owner.resident.classifier.state(), ChildState::Running);
    assert_eq!(owner.gate_child.state(), ChildState::Running);
    assert_eq!(
        member.requests,
        [
            "open", "grants", "prompt", "release", "pairs", "release", "pairs", "release",
            "summary", "release"
        ]
    );
    let answer = packet(peer).expect("aggregate answer arrived on original peer");
    assert_eq!(answer, owner.answer);
    assert_eq!(answer.exchange.ordinal, 7);
    assert_eq!(answer.position, weaver_types::Position::Close);
    assert!(matches!(
        answer.payload,
        weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::Ready)
    ));
    assert!(packet(peer).is_none(), "exactly one aggregate answer");
}

fn scenario(name: &str, mode: &str) {
    if std::env::var("WEAVER_WORKER_STARTUP_CASE").ok().as_deref() == Some(mode) {
        let run = std::env::var("WEAVER_GATE_STARTUP_RUN").unwrap();
        std::fs::write(
            format!("/sys/fs/cgroup{run}/cgroup.procs"),
            std::process::id().to_string(),
        )
        .unwrap();
        circuit(mode);
        return;
    }
    let layout = crate::tools_cgroup::tests::ScratchLayout::new(&format!("worker-{mode}"));
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--ignored", "--nocapture"])
        .env("WEAVER_WORKER_STARTUP_CASE", mode)
        .env("WEAVER_GATE_STARTUP_RUN", &layout.run)
        .status()
        .unwrap();
    assert!(status.success(), "worker circuit {mode} failed");
}
macro_rules! scenario {
    ($name:ident, $mode:literal) => {
        #[test]
        #[ignore = "requires root and an owned delegated cgroup scope"]
        fn $name() {
            scenario(
                concat!(module_path!(), "::", stringify!($name))
                    .trim_start_matches("weaver_harness::"),
                $mode,
            );
        }
    };
}
scenario!(worker_startup_composition_ready, "ready");
scenario!(worker_startup_composition_actual_dev_loop, "worker");
scenario!(worker_startup_composition_open_refusal, "open-refusal");
scenario!(
    worker_startup_composition_classifier_refusal,
    "classifier-refusal"
);
scenario!(worker_startup_composition_spu_refusal, "spu-refusal");
scenario!(worker_startup_composition_prompt_refusal, "prompt-refusal");
scenario!(worker_startup_composition_gate_refusal, "gate-refusal");
scenario!(worker_startup_composition_original_eof, "original-eof");
scenario!(worker_startup_composition_original_stop, "original-stop");
scenario!(worker_startup_composition_expiry, "expiry");
scenario!(
    worker_startup_composition_management_at_startup,
    "management-startup"
);
scenario!(
    worker_startup_composition_management_at_gate,
    "management-gate"
);
scenario!(
    worker_startup_composition_missing_checker_retains_enter,
    "missing-checker"
);
scenario!(
    worker_startup_composition_missing_state_retains_enter,
    "missing-state"
);
scenario!(worker_startup_composition_malformed_stops, "malformed");
scenario!(worker_startup_composition_root_peer_only, "unprivileged");
scenario!(
    worker_startup_composition_bad_checker_retains_enter,
    "bad-checker-path"
);

scenario!(worker_input_composition_accepted, "input-accepted");
scenario!(worker_input_composition_external, "input-external");
scenario!(worker_input_composition_malformed, "input-malformed");
scenario!(worker_input_composition_role, "input-role");
scenario!(worker_input_composition_no_dialer, "input-no-dialer");
scenario!(worker_input_composition_language_refusal, "input-language");
scenario!(worker_input_composition_send_pressure, "input-pressure");
scenario!(worker_input_composition_queued_frame, "input-queued");
scenario!(worker_input_composition_wrong_position, "input-position");
scenario!(worker_input_composition_wrong_opener, "input-opener");
scenario!(
    worker_input_composition_duplicate_exchange,
    "input-duplicate"
);
scenario!(worker_input_composition_expiry, "input-silent");
scenario!(worker_input_composition_checker_eof, "input-check-eof");
scenario!(worker_input_composition_management, "input-management");
scenario!(worker_input_composition_fault_before_input, "input-fault");
scenario!(worker_input_composition_actual_dev_loop, "input-worker");

scenario!(
    worker_input_composition_eof_before_source,
    "input-source-eof"
);

scenario!(
    worker_input_composition_actual_dev_loop_accepted,
    "input-worker-accepted"
);

scenario!(worker_input_composition_refused_then_next, "input-two");

scenario!(
    worker_observe_composition_accepted,
    "input-observe-accepted"
);

scenario!(worker_observe_composition_active, "input-observe-active");

scenario!(
    worker_observe_composition_checking,
    "input-observe-checking"
);

scenario!(
    worker_observe_composition_pressure,
    "input-observe-pressure"
);

scenario!(
    worker_observe_composition_wrong_opener,
    "input-observe-opener"
);

scenario!(
    worker_observe_composition_wrong_position,
    "input-observe-position"
);

scenario!(worker_observe_composition_handles, "input-observe-handles");

scenario!(
    worker_observe_composition_leave_retained,
    "input-observe-leave"
);

scenario!(
    worker_observe_composition_stop_at_rest,
    "input-observe-stop"
);

scenario!(
    worker_observe_composition_save_retained,
    "input-observe-save"
);

scenario!(worker_observe_composition_malformed, "input-observe-bad");

scenario!(worker_observe_composition_eof, "input-observe-eof");

scenario!(
    worker_observe_composition_silent_input_expiry,
    "input-observe-expiry"
);

scenario!(
    worker_observe_composition_actual_admin_and_dev_loop,
    "input-worker-observe"
);

scenario!(worker_stop_composition_waiting, "input-stop-waiting");

scenario!(worker_stop_composition_checking, "input-stop-checking");

scenario!(worker_stop_composition_accepted, "input-stop-accepted");

scenario!(worker_stop_composition_source, "input-stop-source");

scenario!(worker_stop_composition_active, "input-stop-active");

scenario!(worker_stop_composition_transfer, "input-stop-transfer");

scenario!(
    worker_stop_composition_gate_pressure,
    "input-stop-gate-pressure"
);

scenario!(
    worker_stop_composition_admin_pressure,
    "input-stop-admin-pressure"
);

scenario!(worker_stop_composition_early_eof, "input-stop-early-eof");

scenario!(
    worker_stop_composition_accepted_eof,
    "input-stop-accepted-eof"
);

scenario!(worker_stop_composition_gate_eof, "input-stop-gate-eof");

scenario!(
    worker_stop_composition_trace_failure,
    "input-stop-trace-failure"
);

scenario!(worker_stop_composition_next, "input-stop-next");

scenario!(
    worker_stop_composition_actual_admin_and_worker,
    "input-worker-stop-active"
);
scenario!(
    worker_stop_composition_trace_pressure,
    "input-stop-trace-pressure"
);
scenario!(worker_stop_composition_trace_eof, "input-stop-trace-eof");
