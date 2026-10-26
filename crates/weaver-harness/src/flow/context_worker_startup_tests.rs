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

/// Recorded does not imply the sink reader has received the writer's bytes.
/// Keep partial lines and wait through the actual Recorded sequence under a
/// fixture deadline. A missing row is a failed instrument, never a partial pass.
pub(super) fn trace_through(
    reader: &mut impl Read,
    mut bytes: Vec<u8>,
    through: weaver_trace::Sequence,
    deadline: Instant,
) -> std::io::Result<Vec<serde_json::Value>> {
    let mut rows = Vec::<serde_json::Value>::new();
    loop {
        match reader.read_to_end(&mut bytes) {
            Ok(_) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        while let Some(end) = bytes.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = bytes.drain(..=end).collect();
            rows.push(serde_json::from_slice(&line).map_err(std::io::Error::other)?);
        }
        let last = rows
            .last()
            .and_then(|row| row["sequence"].as_str())
            .and_then(|s| s.parse::<u64>().ok());
        if last.is_some_and(|sequence| sequence >= through.0) {
            return Ok(rows);
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "trace fixture ended at {last:?}, before Recorded {}",
                    through.0
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn worker_fixture_trace_waits_for_delayed_complete_recorded_row() {
    struct Delayed(std::collections::VecDeque<Vec<u8>>);
    impl Read for Delayed {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            let Some(bytes) = self.0.front_mut() else {
                return Ok(0);
            };
            if bytes.is_empty() {
                self.0.pop_front();
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
            let count = bytes.len().min(output.len());
            output[..count].copy_from_slice(&bytes[..count]);
            bytes.drain(..count);
            if bytes.is_empty() {
                self.0.pop_front();
            }
            Ok(count)
        }
    }
    let mut reader = Delayed(std::collections::VecDeque::from([
        b"{\"sequence\":\"1\"}\n{\"sequence\":".to_vec(),
        vec![],
        b"\"2\",\"kind\":\"turn.closed\"}\n".to_vec(),
    ]));
    let rows = trace_through(
        &mut reader,
        vec![],
        weaver_trace::Sequence(2),
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["kind"], "turn.closed");
}

#[test]
fn worker_fixture_trace_missing_recorded_row_fails_at_deadline() {
    let mut reader = std::io::Cursor::new(b"{\"sequence\":\"1\"}\n");
    let error = trace_through(
        &mut reader,
        vec![],
        weaver_trace::Sequence(2),
        Instant::now(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("before Recorded 2"));
}

#[test]
fn worker_fixture_clock_waits_for_complete_request_receipt() {
    use std::io::BufRead;
    let (mut peer, member) = UnixStream::pair().unwrap();
    member.set_nonblocking(true).unwrap();
    let mut member = Member {
        peer: member,
        bytes: vec![],
        seed: "seed".into(),
        ordinal: 0,
        requests: vec![],
        hold_mark: false,
        hold_kind: None,
        filings: vec![],
        held_file: None,
        hold_control: None,
        held_control: None,
    };
    let samples = std::cell::Cell::new(0);
    let sample = || {
        samples.set(samples.get() + 1);
        30
    };
    // Arrival after an empty poll and then a partial line must not reuse an
    // earlier reading. No scheduling assumption or production validator changes.
    member.step_observed("ready", sample);
    assert_eq!(samples.get(), 0);
    let request = s::StateFrame::Control {
        common: s::Common {
            session_id: "session".into(),
            run_id: "run".into(),
            request_id: 2.into(),
        },
        frame: s::StateControl::Read {
            selector: s::ReadSelector::OpenPairs {},
        },
    };
    let bytes = s::encode_state_line(&request, s::FrameDirection::Request).unwrap();
    peer.write_all(&bytes[..bytes.len() - 1]).unwrap();
    member.step_observed("ready", sample);
    assert_eq!(samples.get(), 0);
    peer.write_all(&bytes[bytes.len() - 1..]).unwrap();
    member.step_observed("ready", sample);
    assert_eq!(samples.get(), 1);
    let mut bytes = vec![];
    std::io::BufReader::new(peer)
        .read_until(b'\n', &mut bytes)
        .unwrap();
    let answer = s::decode_state_line(&bytes, s::FrameDirection::Response).unwrap();
    let s::StateFrame::ControlAnswer { observation, .. } = answer else {
        panic!("actual read response")
    };
    assert_eq!(observation.began_ns.0, 30);
}

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
                    if mode.contains("generation") {
                        mode
                    } else if mode == "spu-refusal" {
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
            templates: if mode.contains("emission") {
                super::input::generation::emission::tests::templates(mode)
            } else if mode.contains("generation") {
                super::input::generation::tests::templates()
            } else {
                template_tests::templates()
            },
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
    pub(super) requests: Vec<String>,
    pub(super) hold_mark: bool,
    pub(super) hold_kind: Option<&'static str>,
    pub(super) filings: Vec<s::StateFrame>,
    pub(super) held_file: Option<s::StateFrame>,
    pub(super) hold_control: Option<&'static str>,
    pub(super) held_control: Option<s::StateFrame>,
}
impl Member {
    pub(super) fn stop_responses(&self) {
        self.peer.shutdown(std::net::Shutdown::Write).unwrap();
    }

    pub(super) fn assert_no_request(&mut self) {
        let mut byte = [0];
        assert!(
            matches!(self.peer.read(&mut byte), Err(error)
            if error.kind() == std::io::ErrorKind::WouldBlock),
            "failed serving must issue no new member request"
        );
    }

    pub(super) fn step(&mut self, now: u64, mode: &str) {
        self.step_observed(mode, || now);
    }

    /// The concurrent worker can send after this responder starts its step.
    /// Sample a fresh reading only after each complete request is received.
    pub(super) fn step_live(&mut self, mode: &str) {
        self.step_observed(mode, || MonotonicIo.now().unwrap());
    }

    fn step_observed(&mut self, mode: &str, mut now: impl FnMut() -> u64) {
        if self.hold_control.is_none()
            && let Some(response) = self.held_control.take()
        {
            self.peer
                .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
                .unwrap();
        }
        if !self.hold_mark
            && self.hold_kind.is_none()
            && let Some(response) = self.held_file.take()
        {
            self.peer
                .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
                .unwrap();
        }
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
            let observation = operation(&request, self.ordinal, now());
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
                                s::ReadSelector::Marks {} if mode.contains("generation") => {
                                    self.requests.push("marks".into());
                                    serde_json::to_vec(&if mode.contains("-session-") {
                                        super::input::generation::session_tests::marks(
                                            &self.filings,
                                        )
                                    } else {
                                        s::QueryResult::Marks {
                                            next_span_id: 1.into(),
                                            current_task: None,
                                            open_scopes: vec![],
                                        }
                                    })
                                    .unwrap()
                                }
                                s::ReadSelector::History {
                                    noun,
                                    verb: None,
                                    condition: None,
                                } if mode.contains("-pairs-")
                                    || mode.contains("-final-")
                                    || mode.ends_with("correction-inward") =>
                                {
                                    self.requests.push(format!("recall:{noun}"));
                                    br#"{ "entries" : { "entries" : [] } }"#.to_vec()
                                }
                                _ => panic!("unexpected read {selector:?}"),
                            };
                            if (mode.contains("-pairs-") && mode.contains("read-refused"))
                                && matches!(selector, s::ReadSelector::History { .. })
                            {
                                s::StateControlAnswer::ControlRefused {
                                    code: s::ControlRefusalCode::Read(
                                        s::ReadRefusalCode::ResultTooLarge,
                                    ),
                                    detail: "whole history exceeds the fixture bound".into(),
                                }
                            } else if mode == "prompt-refusal"
                                || (mode.ends_with("read-refusal")
                                    && matches!(selector, s::ReadSelector::Marks {}))
                            {
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
                    self.filings.push(request.clone());
                    s::StateFrame::Filed {
                        common: common.clone(),
                        block_id: filing.block_id.clone(),
                        observation,
                    }
                }
                s::StateFrame::FilePair { common, filing } => {
                    self.requests.push("pair-file".into());
                    self.filings.push(request.clone());
                    s::StateFrame::Filed {
                        common: common.clone(),
                        block_id: filing.block_id.clone(),
                        observation,
                    }
                }
                _ => panic!("unexpected request {request:?}"),
            };
            let hold_control = match &request {
                s::StateFrame::Control {
                    frame:
                        s::StateControl::Read {
                            selector: s::ReadSelector::History { .. },
                        },
                    ..
                } => self.hold_control == Some("read"),
                s::StateFrame::Control {
                    frame: s::StateControl::Release { .. },
                    ..
                } => self.hold_control == Some("release"),
                _ => false,
            };
            if hold_control {
                assert!(self.held_control.is_none());
                self.held_control = Some(response);
                continue;
            }
            let hold = match &request {
                s::StateFrame::File { filing, .. } => (self.hold_mark && filing.source.kind == "state.mark") || self.hold_kind == Some(filing.source.kind.as_str()),
                s::StateFrame::FilePair { filing, .. } => self.hold_kind == Some(filing.source.kind.as_str()) && (!mode.contains("second-pair") || self.filings.iter().filter(|f| matches!(f, s::StateFrame::FilePair { filing, .. } if filing.source.kind == "tool.call.completed")).count() >= 2),
                _ => false,
            };
            if hold {
                assert!(self.held_file.is_none());
                self.held_file = Some(response);
                continue;
            }
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
    let (mut bootstrap, mut scheme, _) = classifier_fixture();
    if mode.contains("emission") {
        super::input::generation::emission::tests::scheme(&mut bootstrap, &mut scheme);
    }
    let mut config = super::super::tests::config();
    config.scheme = scheme;
    if mode.contains("emission") && !mode.ends_with("unselected") {
        let mut rule = config
            .partition
            .iter()
            .find(|r| r.kind == "loop.instruction")
            .unwrap()
            .clone();
        rule.id = "outbound".into();
        rule.kind = "sort.decision".into();
        config.partition.push(rule);
    }
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
    load.ste100_scheme = bootstrap.scheme_text.clone();
    load.state_flow.gate_wait.elapsed_ns = config.gate_wait.elapsed_ns.0.into();
    if mode.starts_with("input-worker") {
        load.state_flow.ste100.decision_wait.elapsed_ns = 5_000_000_000.into();
        bootstrap.instruction.decision_wait.elapsed_ns = 5_000_000_000.into();
    }
    if mode.contains("-external-") && !mode.contains("no-grant") {
        load.state_flow.tools.sh = Some(weaver_trace::serving::payloads::ToolPolicy {
            clock_ms: 1234,
            measurement_ref: "synthetic-worker-external".into(),
        });
    }
    let envelope = super::super::startup_composition::tests::entered_tests::envelope(
        &config,
        &load,
        &bootstrap,
        program.organs.classify.as_ref().unwrap(),
    );
    let envelope = if mode.contains("-external-") && !mode.contains("no-grant") {
        let mut value = serde_json::to_value(envelope).unwrap();
        value["payload"]["body"]["payload"]["binding"]["shell"] = serde_json::json!({
            "binary": "/synthetic/sh", "commands": "/synthetic/commands"
        });
        serde_json::from_value(value).unwrap()
    } else {
        envelope
    };
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
        hold_mark: false,
        hold_kind: None,
        filings: vec![],
        held_file: None,
        hold_control: None,
        held_control: None,
    };
    if mode == "worker" || mode.starts_with("input-worker") {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = done.clone();
        let responder = std::thread::spawn(move || {
            let limit = Instant::now() + Duration::from_secs(8);
            while !flag.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < limit {
                member.step_live("ready");
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
            if mode.contains("generation") {
                super::input::generation::tests::exercise(
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
            if mode.contains("generation") {
                super::input::generation::tests::exercise(
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
    assert_eq!(
        owner.certificate.as_ref().unwrap().accepted().len(),
        owner
            .certificate
            .as_ref()
            .unwrap()
            .inventory()
            .templates()
            .len()
    );
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

scenario!(
    worker_generation_composition_filed_task,
    "input-generation-ready"
);
scenario!(
    worker_generation_composition_slow_decode,
    "input-generation-slow-decode"
);
scenario!(
    worker_generation_composition_elide_without_cut,
    "input-generation-elide"
);
scenario!(
    worker_generation_composition_input_receipt,
    "input-generation-held-input"
);
scenario!(
    worker_generation_composition_read_refusal,
    "input-generation-read-refusal"
);
scenario!(
    worker_generation_composition_fit_refusal,
    "input-generation-fit"
);
scenario!(
    worker_generation_composition_wrong_counts,
    "input-generation-wrong-counts"
);
scenario!(
    worker_generation_composition_decoder_eof,
    "input-generation-decoder-eof"
);
scenario!(
    worker_generation_composition_output_refusal,
    "input-generation-output-refusal"
);
scenario!(
    worker_generation_composition_stop_task,
    "input-generation-stop-task"
);
scenario!(
    worker_generation_composition_stop_measuring,
    "input-generation-stop-measure"
);
scenario!(
    worker_generation_composition_stop_grant,
    "input-generation-stop-grant"
);
scenario!(
    worker_generation_composition_stop_generation,
    "input-generation-stop-generation"
);
scenario!(
    worker_generation_composition_stop_unsent,
    "input-generation-stop-unsent"
);
scenario!(
    worker_generation_composition_stop_generated,
    "input-generation-stop-generated"
);
scenario!(
    worker_generation_composition_actual_worker,
    "input-worker-generation"
);

scenario!(
    worker_generation_composition_mark_receipt,
    "input-generation-held-mark"
);
scenario!(
    worker_generation_composition_stop_mark,
    "input-generation-stop-mark"
);
scenario!(
    worker_generation_composition_stop_refusal,
    "input-generation-stop-fit"
);
scenario!(
    worker_generation_composition_stop_measure_eof,
    "input-generation-stop-measure-eof"
);

scenario!(
    worker_generation_composition_observe_measurement,
    "input-generation-observe-measure"
);

scenario!(
    worker_emission_composition_ready,
    "input-generation-emission-ready"
);

scenario!(
    worker_emission_composition_no_answer,
    "input-generation-emission-no-answer"
);

scenario!(
    worker_emission_composition_unselected,
    "input-generation-emission-unselected"
);

scenario!(
    worker_emission_composition_empty,
    "input-generation-emission-empty"
);

scenario!(
    worker_emission_composition_invalid_recall,
    "input-generation-emission-invalid-recall"
);

scenario!(
    worker_emission_composition_duplicate,
    "input-generation-emission-duplicate"
);

scenario!(
    worker_emission_composition_foreign,
    "input-generation-emission-foreign"
);

scenario!(
    worker_emission_composition_wrong_statement,
    "input-generation-emission-wrong-statement"
);

scenario!(
    worker_emission_composition_wrong_lane,
    "input-generation-emission-wrong-lane"
);

scenario!(
    worker_emission_composition_held,
    "input-generation-emission-held"
);

scenario!(
    worker_emission_composition_held_observe,
    "input-generation-emission-held-observe"
);

scenario!(
    worker_emission_composition_stop_held,
    "input-generation-emission-stop-held"
);

scenario!(
    worker_emission_composition_stop_held_eof,
    "input-generation-emission-stop-held-eof"
);

scenario!(
    worker_emission_composition_stop_begin,
    "input-generation-emission-stop-begin"
);

scenario!(
    worker_emission_composition_stop_effective,
    "input-generation-emission-stop-effective"
);

scenario!(
    worker_emission_composition_stop_outbound_begin,
    "input-generation-emission-stop-outbound-begin"
);

scenario!(
    worker_emission_composition_stop_sorted,
    "input-generation-emission-stop-sorted"
);

scenario!(
    worker_emission_composition_restatement,
    "input-generation-emission-restatement"
);

scenario!(
    worker_emission_composition_repair_refusal,
    "input-generation-emission-repair-refusal"
);

scenario!(
    worker_emission_composition_sort_refusal,
    "input-generation-emission-sort-refusal"
);

scenario!(
    worker_emission_composition_no_sort,
    "input-generation-emission-no-sort"
);

scenario!(
    worker_emission_composition_actual_worker,
    "input-worker-generation-emission-actual-worker"
);

scenario!(
    worker_emission_composition_stop_generated,
    "input-generation-emission-stop-generated"
);
scenario!(
    worker_emission_composition_stop_generated_invalid_recall,
    "input-generation-emission-stop-generated-invalid-recall"
);

scenario!(
    worker_emission_composition_bindings,
    "input-generation-emission-bindings"
);

scenario!(
    worker_emission_composition_held_deadline,
    "input-generation-emission-held-deadline"
);

scenario!(
    worker_emission_composition_stop_pair_receipt,
    "input-generation-emission-stop-pair-receipt"
);

scenario!(
    worker_emission_composition_stop_second_pair_receipt,
    "input-generation-emission-stop-second-pair-receipt"
);

scenario!(
    worker_emission_composition_answer_order,
    "input-generation-emission-answer-order"
);

scenario!(
    worker_emission_composition_completed_observe_sorted,
    "input-generation-emission-completed-observe-sorted"
);
scenario!(
    worker_emission_composition_completed_observe_unselected,
    "input-generation-emission-completed-observe-unselected"
);
scenario!(
    worker_emission_composition_completed_observe_sort_refusal,
    "input-generation-emission-completed-observe-sort-refusal"
);
scenario!(
    worker_emission_composition_completed_observe_repair_refusal,
    "input-generation-emission-completed-observe-repair-refusal"
);

scenario!(
    worker_correction_composition_ready,
    "input-generation-emission-correction-ready"
);

scenario!(
    worker_correction_composition_held_filing,
    "input-generation-emission-correction-held-filing"
);

scenario!(
    worker_correction_composition_retry_text,
    "input-generation-emission-correction-retry-text"
);

scenario!(
    worker_correction_composition_retry_empty,
    "input-generation-emission-correction-retry-empty"
);

scenario!(
    worker_correction_composition_retry_call,
    "input-generation-emission-correction-retry-call"
);

scenario!(
    worker_correction_composition_stop_restatement,
    "input-generation-emission-correction-stop-restatement"
);

scenario!(
    worker_correction_composition_stop_placed,
    "input-generation-emission-correction-stop-placed"
);

scenario!(
    worker_correction_composition_stop_generation,
    "input-generation-emission-correction-stop-generation"
);

scenario!(
    worker_correction_composition_stop_checks,
    "input-generation-emission-correction-stop-checks"
);

scenario!(
    worker_correction_composition_retry_text_stop_checks,
    "input-generation-emission-correction-retry-text-stop-checks"
);

scenario!(
    worker_correction_composition_retry_empty_stop_checks,
    "input-generation-emission-correction-retry-empty-stop-checks"
);

scenario!(
    worker_correction_composition_retry_call_stop_checks,
    "input-generation-emission-correction-retry-call-stop-checks"
);

scenario!(
    worker_correction_composition_fit_refusal,
    "input-generation-emission-correction-fit-refusal"
);

scenario!(
    worker_correction_composition_measure_refusal,
    "input-generation-emission-correction-measure-refusal"
);

scenario!(
    worker_correction_composition_actual_worker,
    "input-worker-generation-emission-correction-actual-worker"
);

scenario!(
    worker_correction_composition_cap,
    "input-generation-emission-correction-cap"
);

scenario!(
    worker_correction_composition_held_decode,
    "input-generation-emission-correction-held-decode"
);

scenario!(
    worker_correction_composition_held_decode_deadline,
    "input-generation-emission-correction-held-decode-deadline"
);

scenario!(
    worker_correction_composition_held_decode_inherited,
    "input-generation-emission-correction-held-decode-inherited"
);

scenario!(
    worker_correction_composition_fit_refusal_close_stop,
    "input-generation-emission-correction-fit-refusal-close-stop"
);

scenario!(
    worker_correction_composition_actual_worker_fit_refusal,
    "input-worker-generation-emission-correction-actual-worker-fit-refusal"
);

scenario!(
    worker_correction_composition_refusal_trace_pressure,
    "input-generation-emission-correction-fit-refusal-boundary-trace-pressure"
);
scenario!(
    worker_correction_composition_refusal_trace_deadline,
    "input-generation-emission-correction-fit-refusal-boundary-trace-deadline"
);
scenario!(
    worker_correction_composition_refusal_gate_pressure,
    "input-generation-emission-correction-fit-refusal-boundary-gate-pressure"
);
scenario!(
    worker_correction_composition_refusal_gate_deadline,
    "input-generation-emission-correction-fit-refusal-boundary-gate-deadline"
);
scenario!(
    worker_correction_composition_refusal_ready_stop,
    "input-generation-emission-correction-fit-refusal-boundary-refusal-close-ready-stop"
);

scenario!(
    worker_inward_composition_ready,
    "input-generation-emission-pairs-ready"
);

scenario!(
    worker_inward_composition_identical,
    "input-generation-emission-pairs-identical"
);

scenario!(
    worker_inward_composition_reverse,
    "input-generation-emission-pairs-reverse"
);

scenario!(
    worker_inward_composition_empty,
    "input-generation-emission-pairs-empty"
);

scenario!(
    worker_inward_composition_barred,
    "input-generation-emission-pairs-barred"
);

scenario!(
    worker_inward_composition_omit_first,
    "input-generation-emission-pairs-omit-first"
);

scenario!(
    worker_inward_composition_mixed,
    "input-generation-emission-pairs-mixed"
);

scenario!(
    worker_inward_composition_external,
    "input-generation-emission-pairs-external"
);

scenario!(
    worker_inward_composition_held_statement,
    "input-generation-emission-pairs-held-statement"
);

scenario!(
    worker_inward_composition_held_opening,
    "input-generation-emission-pairs-held-opening"
);

scenario!(
    worker_inward_composition_held_read,
    "input-generation-emission-pairs-held-read"
);
scenario!(
    worker_inward_composition_read_expired,
    "input-generation-emission-pairs-held-read-expired"
);

scenario!(
    worker_inward_composition_held_release,
    "input-generation-emission-pairs-held-release"
);

scenario!(
    worker_inward_composition_held_terminal,
    "input-generation-emission-pairs-held-terminal"
);

scenario!(
    worker_inward_composition_read_refused,
    "input-generation-emission-pairs-read-refused"
);

scenario!(
    worker_inward_composition_stop_begin,
    "input-generation-emission-pairs-stop-begin"
);

scenario!(
    worker_inward_composition_stop_ready,
    "input-generation-emission-pairs-stop-ready"
);
scenario!(
    worker_inward_composition_stop_plan_break_prefix,
    "input-generation-emission-pairs-omit-first-stop-ready"
);
scenario!(
    worker_inward_composition_corrected,
    "input-generation-emission-correction-inward"
);
scenario!(
    worker_inward_composition_stop_elected,
    "input-generation-emission-pairs-stop-elected"
);

scenario!(
    worker_inward_composition_stop_read,
    "input-generation-emission-pairs-stop-read"
);

scenario!(
    worker_inward_composition_stop_terminal,
    "input-generation-emission-pairs-stop-terminal"
);

scenario!(
    worker_inward_composition_stop_complete,
    "input-generation-emission-pairs-stop-complete"
);

scenario!(
    worker_inward_composition_actual_worker,
    "input-worker-generation-emission-pairs-actual-worker"
);

scenario!(
    worker_return_composition_ready,
    "input-generation-emission-pairs-return-ready"
);
scenario!(
    worker_return_composition_slow_decode,
    "input-generation-emission-pairs-return-slow-decode"
);

scenario!(
    worker_return_composition_identical,
    "input-generation-emission-pairs-return-identical"
);

scenario!(
    worker_return_composition_reverse,
    "input-generation-emission-pairs-return-reverse"
);

scenario!(
    worker_return_composition_barred,
    "input-generation-emission-pairs-return-barred"
);

scenario!(
    worker_return_composition_read_refused,
    "input-generation-emission-pairs-return-read-refused"
);

scenario!(
    worker_return_composition_subset,
    "input-generation-emission-pairs-return-subset"
);

scenario!(
    worker_return_composition_fit_refusal,
    "input-generation-emission-pairs-return-fit-refusal"
);

scenario!(
    worker_return_composition_stop_election,
    "input-generation-emission-pairs-return-stop-election"
);

scenario!(
    worker_return_composition_stop_placed,
    "input-generation-emission-pairs-return-stop-placed"
);

scenario!(
    worker_return_composition_stop_generating,
    "input-generation-emission-pairs-return-stop-generating"
);

scenario!(
    worker_return_composition_stop_generated,
    "input-generation-emission-pairs-return-stop-generated"
);

scenario!(
    worker_return_composition_actual_worker,
    "input-worker-generation-emission-pairs-return-actual-worker"
);

scenario!(
    worker_external_composition_ready,
    "input-generation-emission-pairs-external-ready"
);

scenario!(
    worker_external_composition_identical,
    "input-generation-emission-pairs-external-identical"
);

scenario!(
    worker_external_composition_mixed,
    "input-generation-emission-pairs-external-mixed"
);

scenario!(
    worker_external_composition_reverse,
    "input-generation-emission-pairs-external-reverse"
);

scenario!(
    worker_external_composition_no_grant,
    "input-generation-emission-pairs-external-no-grant"
);

scenario!(
    worker_external_composition_refused,
    "input-generation-emission-pairs-external-refused"
);

scenario!(
    worker_external_composition_errored,
    "input-generation-emission-pairs-external-errored"
);

scenario!(
    worker_external_composition_killed,
    "input-generation-emission-pairs-external-killed"
);

scenario!(
    worker_external_composition_held_terminal,
    "input-generation-emission-pairs-external-held-terminal"
);

scenario!(
    worker_external_composition_stop_elected,
    "input-generation-emission-pairs-external-stop-elected"
);

scenario!(
    worker_external_composition_stop_unsent,
    "input-generation-emission-pairs-external-stop-unsent"
);

scenario!(
    worker_external_composition_stop_running,
    "input-generation-emission-pairs-external-stop-running"
);

scenario!(
    worker_external_composition_stop_crossed,
    "input-generation-emission-pairs-external-stop-crossed"
);

scenario!(
    worker_external_composition_stop_terminal,
    "input-generation-emission-pairs-external-stop-terminal"
);

scenario!(
    worker_external_composition_wrong_return,
    "input-generation-emission-pairs-external-wrong-return"
);

scenario!(
    worker_external_composition_duplicate,
    "input-generation-emission-pairs-external-duplicate"
);

scenario!(
    worker_external_composition_actual_worker,
    "input-worker-generation-emission-pairs-external-actual-worker"
);

scenario!(
    worker_external_composition_mixed_revoked,
    "input-generation-emission-pairs-external-mixed-revoked"
);

scenario!(
    worker_external_composition_gate_death,
    "input-generation-emission-pairs-external-gate-death"
);

scenario!(
    worker_external_composition_multiline,
    "input-generation-emission-pairs-external-multiline"
);

scenario!(
    worker_external_composition_queued_input,
    "input-generation-emission-pairs-external-queued-input"
);

scenario!(
    worker_external_composition_queued_input_stop_running,
    "input-generation-emission-pairs-external-queued-input-stop-running"
);
scenario!(
    worker_external_composition_queued_input_stop_running_late,
    "input-generation-emission-pairs-external-queued-input-stop-running-late"
);
scenario!(
    worker_external_composition_queued_input_leave,
    "input-generation-emission-pairs-external-queued-input-leave"
);
scenario!(
    worker_external_composition_queued_input_duplicate_current,
    "input-generation-emission-pairs-external-queued-input-duplicate-current"
);
scenario!(
    worker_external_composition_queued_input_duplicate_queued,
    "input-generation-emission-pairs-external-queued-input-duplicate-queued"
);
scenario!(
    worker_external_composition_queued_input_not_open,
    "input-generation-emission-pairs-external-queued-input-not-open"
);
scenario!(
    worker_final_composition_queued_input,
    "input-worker-generation-emission-final-return-external-queued-input"
);
scenario!(
    worker_final_composition_queued_input_capacity,
    "input-worker-generation-emission-final-return-external-queued-input-capacity"
);
scenario!(
    worker_final_composition_queued_input_held_terminal,
    "input-worker-generation-emission-final-return-external-queued-input-held-terminal"
);

scenario!(
    worker_final_composition_text,
    "input-worker-generation-emission-final-text"
);

scenario!(
    worker_emission_composition_operation_refusal_stop,
    "input-generation-emission-sort-refusal-operation-close-stop"
);

scenario!(
    worker_final_composition_task_admission_refusal,
    "input-worker-generation-emission-final-operation-refusal-no-task"
);
scenario!(
    worker_generation_composition_task_admission_busy,
    "input-generation-task-admission-busy"
);
scenario!(
    worker_generation_composition_task_admission_busy_deadline,
    "input-generation-task-admission-busy-deadline"
);
scenario!(
    worker_generation_composition_task_admission_refusal_proof,
    "input-generation-emission-task-admission-operation-refusal-proof-no-task"
);
scenario!(
    worker_generation_composition_task_admission_revoked_election,
    "input-generation-emission-task-admission-operation-refusal-revoked-election-no-task"
);
scenario!(
    worker_generation_composition_task_admission_revoked_use,
    "input-generation-emission-task-admission-operation-refusal-revoked-use-no-task"
);
scenario!(
    worker_generation_composition_task_admission_refusal_gate_pressure,
    "input-generation-emission-task-admission-operation-refusal-gate-pressure-no-task"
);
scenario!(
    worker_generation_composition_task_admission_refusal_gate_deadline,
    "input-generation-emission-task-admission-operation-refusal-gate-deadline-no-task"
);

scenario!(
    worker_final_composition_operation_repair_refusal,
    "input-worker-generation-emission-final-operation-refusal-repair-refusal"
);
scenario!(
    worker_final_composition_operation_outbound_refusal,
    "input-worker-generation-emission-final-operation-refusal-sort-refusal"
);
scenario!(
    worker_final_composition_operation_outbound_template_refusal,
    "input-worker-generation-emission-final-operation-refusal-no-sort"
);
scenario!(
    worker_final_composition_operation_outbound_refusal_held,
    "input-worker-generation-emission-final-operation-refusal-sort-refusal-held-terminal"
);
scenario!(
    worker_final_composition_operation_repair_exhausted,
    "input-worker-generation-emission-final-correction-operation-refusal-cap"
);
scenario!(
    worker_final_composition_operation_decode_refusal,
    "input-worker-generation-emission-final-correction-operation-refusal-decode-refusal"
);
scenario!(
    worker_correction_composition_decode_refusal_proof,
    "input-generation-emission-correction-decode-refusal-proof"
);

scenario!(
    worker_final_composition_empty,
    "input-worker-generation-emission-final-empty"
);

scenario!(
    worker_final_composition_bindings,
    "input-worker-generation-emission-final-bindings"
);

scenario!(
    worker_final_composition_length,
    "input-worker-generation-emission-final-length"
);

scenario!(
    worker_final_composition_correction,
    "input-worker-generation-emission-final-correction"
);

scenario!(
    worker_final_composition_round_once,
    "input-worker-generation-emission-final-return-round-once"
);

scenario!(
    worker_final_composition_round_limit,
    "input-worker-generation-emission-final-return-round-limit"
);

scenario!(
    worker_final_composition_external,
    "input-worker-generation-emission-final-return-external-healthy"
);

scenario!(
    worker_final_composition_subset,
    "input-generation-emission-final-manual-subset"
);

scenario!(
    worker_final_composition_round_held,
    "input-generation-emission-final-manual-round-held"
);

scenario!(
    worker_final_composition_gate_pressure,
    "input-generation-emission-final-manual-gate-pressure"
);

scenario!(
    worker_final_composition_stop_before,
    "input-generation-emission-final-manual-stop-before"
);

scenario!(
    worker_final_composition_stop_after,
    "input-generation-emission-final-manual-stop-after"
);

scenario!(
    worker_final_composition_trace_failure,
    "input-generation-emission-final-manual-trace-failure"
);

scenario!(
    worker_final_composition_trace_pressure,
    "input-generation-emission-final-manual-trace-pressure"
);

scenario!(
    worker_final_composition_deadline,
    "input-generation-emission-final-manual-deadline"
);
scenario!(
    worker_final_composition_send_clock,
    "input-generation-emission-final-manual-send-clock"
);

scenario!(
    worker_final_composition_management_observe,
    "input-worker-generation-emission-final-return-round-once-management-observe"
);

scenario!(
    worker_final_composition_management_stop_before,
    "input-worker-generation-emission-final-return-round-once-management-stop-before"
);

scenario!(
    worker_final_composition_management_leave,
    "input-worker-generation-emission-final-return-round-once-management-leave"
);

scenario!(
    worker_session_composition_two,
    "input-worker-generation-emission-session-two"
);

scenario!(
    worker_session_composition_three,
    "input-worker-generation-emission-session-three"
);

scenario!(
    worker_session_composition_refused,
    "input-worker-generation-emission-session-refused"
);

scenario!(
    worker_session_composition_queued,
    "input-worker-generation-emission-session-queued"
);

scenario!(
    worker_session_composition_observe,
    "input-worker-generation-emission-session-observe"
);

scenario!(
    worker_session_composition_stop_idle,
    "input-worker-generation-emission-session-stop-idle"
);

scenario!(
    worker_session_composition_stop_active,
    "input-worker-generation-emission-session-stop-active"
);

scenario!(
    worker_session_composition_leave,
    "input-worker-generation-emission-session-leave"
);

scenario!(
    worker_session_composition_duplicate,
    "input-worker-generation-emission-session-duplicate"
);

scenario!(
    worker_session_composition_observe_accepted,
    "input-worker-generation-emission-session-observe-accepted"
);

scenario!(
    worker_session_composition_stop_accepted,
    "input-worker-generation-emission-session-stop-accepted"
);

scenario!(
    worker_session_composition_decoder_failure,
    "input-worker-generation-emission-session-decoder-failure"
);

scenario!(
    worker_final_composition_model_stop,
    "input-worker-generation-emission-final-model-stop"
);
scenario!(
    worker_session_composition_model_stop,
    "input-worker-generation-emission-session-model-stop"
);
scenario!(
    worker_session_composition_read_refusal,
    "input-worker-generation-emission-session-read-refusal"
);
scenario!(
    worker_session_composition_generation_refusal,
    "input-worker-generation-emission-session-generation-refusal"
);
scenario!(
    worker_session_composition_election_management,
    "input-worker-generation-emission-session-election"
);

scenario!(
    worker_generation_composition_election_wait,
    "input-generation-election-wait"
);
scenario!(
    worker_generation_composition_election_wait_bound,
    "input-generation-election-wait-bound"
);
scenario!(
    worker_generation_composition_election_wait_held,
    "input-generation-election-wait-held"
);
scenario!(
    worker_generation_composition_election_wait_filing,
    "input-generation-election-wait-filing"
);
scenario!(
    worker_generation_composition_election_wait_first_failure,
    "input-generation-election-wait-first-failure"
);
scenario!(
    worker_generation_composition_election_wait_gate_failure,
    "input-generation-election-wait-gate-failure"
);

scenario!(
    worker_external_composition_halt_gate,
    "input-generation-emission-external-halt-gate"
);
scenario!(
    worker_external_composition_halt_gate_late,
    "input-generation-emission-external-halt-gate-late"
);
scenario!(
    worker_external_composition_halt_gate_pressure,
    "input-generation-emission-external-halt-gate-pressure-late"
);
scenario!(
    worker_external_composition_halt_gate_pressure_hold,
    "input-generation-emission-external-halt-gate-pressure-hold-late"
);
scenario!(
    worker_external_composition_halt_gate_trace_loss,
    "input-generation-emission-external-halt-gate-trace-loss"
);
scenario!(
    worker_external_composition_halt_gate_trace_loss_late,
    "input-generation-emission-external-halt-gate-trace-loss-late"
);
scenario!(
    worker_external_composition_halt_gate_deadline,
    "input-generation-emission-external-halt-gate-deadline"
);
scenario!(
    worker_external_composition_halt_gate_eof,
    "input-generation-emission-external-halt-gate-eof"
);
scenario!(
    worker_external_composition_halt_gate_control,
    "input-generation-emission-external-halt-gate-control"
);
scenario!(
    worker_external_composition_halt_gate_unsent,
    "input-generation-emission-external-halt-gate-unsent"
);
scenario!(
    worker_external_composition_halt_gate_worker,
    "input-generation-emission-external-halt-gate-worker"
);
scenario!(
    worker_external_composition_halt_gate_management,
    "input-generation-emission-external-halt-gate-management"
);

scenario!(
    worker_stop_composition_inherited_deadline,
    "input-stop-inherited-deadline"
);

scenario!(
    worker_external_composition_halt_gate_clock_error,
    "input-generation-emission-external-halt-gate-clock-error"
);
scenario!(
    worker_external_composition_halt_gate_clock_reverse,
    "input-generation-emission-external-halt-gate-clock-reverse"
);
