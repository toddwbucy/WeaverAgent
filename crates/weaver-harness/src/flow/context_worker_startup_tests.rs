//! One worker owner crosses all actual sockets and forks. Named SQLite cases
//! use the normal member. Other member and all classifier/model replies are
//! declared doubles, not production language or model qualification.
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

#[path = "context_worker_leave_joint_tests.rs"]
mod leave_joint;
#[path = "../../tests/state_flow/member.rs"]
pub(super) mod normal_member;

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
        normal: None,
        fixture_root: None,
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
        boot_pairs: vec![],
        summary: None,
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

struct Files(PathBuf, bool);
impl Files {
    fn new() -> Self {
        if let Some(root) = std::env::var_os("WEAVER_LEAVE_JOINT_ROOT") {
            return Self(root.into(), false);
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "weaver-worker-startup-{}-{serial}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path, true),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("worker fixture directory: {error}"),
            }
        }
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
                    if mode == "expired-startup-classifying-held" {
                        "admission-silent-held"
                    } else if mode == "expired-startup-admission-partial-complete" {
                        "admission-partial"
                    } else if mode == "expired-startup-admission-refused-complete" {
                        "refuse"
                    } else if mode.starts_with("expired-startup-classifying")
                        || mode == "expired-startup-admission-unsent-complete"
                    {
                        "admission-silent"
                    } else if mode.contains("-template-component-") {
                        "template-component"
                    } else if mode == "expired-startup-template-held" {
                        "template-silent-held"
                    } else if mode.contains("-template-record-")
                        || mode.contains("-template-between-")
                        || mode.contains("-template-completion-")
                        || mode.contains("-template-recorded-")
                        || mode.contains("-template-foreign-")
                    {
                        "ready"
                    } else if mode.starts_with("expired-startup-template-") {
                        "template-silent"
                    } else if mode == "expired-startup-classifier-held"
                        || mode == "expired-startup-summary-read-held"
                        || mode == "expired-startup-repair-read-held"
                        || mode == "expired-startup-repair-filing-pending-classifier-held"
                        || mode == "expired-startup-summary-account-retained-classifier-held"
                    {
                        "rollback-held-exit"
                    } else if mode.starts_with("input-") {
                        mode
                    } else if mode == "classifier-refusal"
                        || mode == "failed-startup-refusal"
                        || mode == "expired-startup-refusal"
                        || mode.starts_with("rollback-")
                    {
                        "refuse"
                    } else {
                        "ready"
                    },
                    if super::input::generation::session_tests::serving_rollback::is_mode(mode) {
                        "flow::context::worker_startup::input::generation::session_tests::serving_rollback::classifier_child"
                    } else {
                        "flow::context::startup_composition::tests::startup_composition_child"
                    },
                )),
                spu: self.child(
                    "spu",
                    "WEAVER_RESIDENCY_COMPOSITION_CHILD",
                    if mode.contains("-session-ready-rollback") || mode.contains("-session-forced-save") {
                        "cleanup-ready"
                    } else if mode.contains("generation") {
                        mode
                    } else if mode.starts_with("failed-startup-") {
                        "failed-startup"
                    } else if mode == "expired-startup-admit-sent-complete" {
                        "expired-admit-held"
                    } else if mode == "expired-startup-admit-pending-complete" {
                        "expired-admit-unsent"
                    } else if mode == "expired-startup-open-complete" {
                        "expired-open-unanswered"
                    } else if mode == "expired-startup-open-unsent-complete" {
                        "expired-open-unsent"
                    } else if mode == "expired-startup-open-release-wrong" {
                        "expired-open-unanswered-wrong"
                    } else if mode.contains("-release-") && mode.starts_with("expired-startup-") {
                        match mode.rsplit('-').next().unwrap() {
                            "wrong" => "expired-release-wrong",
                            "arm" => "expired-release-arm",
                            "position" => "expired-release-position",
                            "early" => "expired-release-early",
                            "silent" => "expired-release-silent",
                            "held" => "expired-release-held",
                            "recorded" => "cleanup-ready",
                            _ => panic!("unknown expired Release fixture"),
                        }
                    } else if mode.starts_with("expired-startup-") {
                        "cleanup-ready"
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
                    if mode.starts_with("input-")
                        || mode.starts_with("failed-startup-")
                        || mode.starts_with("expired-startup-raised")
                    {
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
        if self.1 {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
pub(super) fn dial(path: &std::path::Path) -> OwnedFd {
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
pub(super) fn packet(peer: &OwnedFd) -> Option<weaver_types::OrganEnvelope> {
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
struct RunGuard(Option<FoundationRun>);
impl RunGuard {
    // Keep the large move out of the circuit's debug stack frame. Every branch
    // consumes the same actual owner; no evidence is reconstructed.
    fn take_boxed(&mut self) -> Box<FoundationRun> {
        Box::new(self.0.take().unwrap())
    }
}
impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Some(run) = &mut self.0 {
            run.resources.reap_startup_test_children();
        }
    }
}
pub(super) struct EnterGuard<'a>(pub(super) Option<FoundationEnter<'a>>);
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
    normal: Option<normal_member::NormalMember>,
    fixture_root: Option<PathBuf>,
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
    pub(super) boot_pairs: Vec<s::ReadPair>,
    pub(super) summary: Option<s::QueryResult>,
}
impl Member {
    pub(super) fn hold_next_normal_filing(&mut self) {
        self.normal
            .as_mut()
            .unwrap()
            .treat_first_filing(normal_member::ReceiptTreatment::Hold);
    }

    pub(super) fn normal_filing_held(&self) -> bool {
        self.normal
            .as_ref()
            .is_some_and(normal_member::NormalMember::filing_held)
    }

    pub(super) fn release_normal_filing(&mut self) {
        self.normal.as_mut().unwrap().release_held_filing();
    }

    pub(super) fn write_normal_evidence(&self) {
        self.normal.as_ref().unwrap().write_evidence(&self.filings);
    }

    pub(super) fn finish_normal(&mut self) {
        if let Some(normal) = &mut self.normal {
            normal.finish(&mut self.peer, &mut self.requests, &mut self.filings);
        }
    }

    pub(super) fn assert_normal_capture(&self, report: &s::SavePointReport) {
        if let Some(normal) = &self.normal {
            normal.assert_capture(report, &self.filings);
        }
    }

    pub(super) fn assert_closed_without_request(&mut self) {
        let mut byte = [0];
        assert_eq!(
            self.peer.read(&mut byte).unwrap(),
            0,
            "failed run must close the actual member without another request"
        );
    }
    pub(super) fn send_held_filing_prefix(&mut self) -> Vec<u8> {
        let raw = s::encode_state_line(
            self.held_file.as_ref().unwrap(),
            s::FrameDirection::Response,
        )
        .unwrap();
        let prefix = raw[..raw.len() / 2].to_vec();
        self.peer.write_all(&prefix).unwrap();
        prefix
    }

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
        if let Some(normal) = &mut self.normal {
            normal.step(&mut self.peer, &mut self.requests, &mut self.filings);
            return;
        }
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
                            let rollback_read = (mode.starts_with("rollback-")
                                || super::input::generation::session_tests::forced_leave::is_mode(mode)
                                || super::input::generation::session_tests::serving_rollback::is_mode(mode))
                                && self.requests.len() > 1;
                            if rollback_read {
                                assert_eq!(&self.requests[..2], ["open", "grants"]);
                                assert_eq!(
                                    self.requests
                                        .iter()
                                        .filter(|request| *request == "grants")
                                        .count(),
                                    1
                                );
                            } else {
                                assert_eq!(self.requests, ["open"]);
                            }
                            self.requests.push("grants".into());
                            if mode == "rollback-grants-refused" && rollback_read {
                                s::StateControlAnswer::ControlRefused {
                                    code: s::ControlRefusalCode::Read(
                                        s::ReadRefusalCode::AnswerTooLarge,
                                    ),
                                    detail: "exact rollback grants refusal".into(),
                                }
                            } else {
                                s::StateControlAnswer::Grants {
                                    surface: vec![
                                        if mode == "rollback-varied" && rollback_read {
                                            "owner:changed"
                                        } else {
                                            "owner:actual"
                                        }
                                        .into(),
                                    ],
                                }
                            }
                        }
                        s::StateControl::Snapshot { capture_id }
                            if super::input::generation::session_tests::forced_leave::is_mode(
                                mode,
                            ) =>
                        {
                            let root = self.fixture_root.as_ref().expect("snapshot fixture root");
                            assert!(
                                root.join("ready-rollback-lower.json").exists(),
                                "snapshot precedes actual Lower"
                            );
                            self.requests.push("snapshot".into());
                            if mode.ends_with("-refused") {
                                s::StateControlAnswer::ControlRefused {
                                    code: s::ControlRefusalCode::Capture(
                                        s::CaptureRefusalCode::SnapshotFailed,
                                    ),
                                    detail: "original explicit snapshot refusal".into(),
                                }
                            } else {
                                s::StateControlAnswer::Snapshot {
                                    capture_id: *capture_id,
                                    room_name: format!("capture-{}.sqlite.part", capture_id.0),
                                    size_bytes: 10.into(),
                                    position: None,
                                }
                            }
                        }
                        s::StateControl::Acknowledge { capture_id, .. }
                            if super::input::generation::session_tests::forced_leave::is_mode(
                                mode,
                            ) =>
                        {
                            self.requests.push("acknowledge".into());
                            s::StateControlAnswer::Finished {
                                capture_id: *capture_id,
                                room_name: format!("capture-{}.sqlite", capture_id.0),
                            }
                        }
                        s::StateControl::ReleaseCapture { capture_id }
                            if super::input::generation::session_tests::forced_leave::is_mode(
                                mode,
                            ) =>
                        {
                            self.requests.push("release_capture".into());
                            s::StateControlAnswer::CaptureReleased {
                                capture_id: *capture_id,
                            }
                        }
                        s::StateControl::Abort { capture_id }
                            if super::input::generation::session_tests::forced_leave::is_mode(
                                mode,
                            ) =>
                        {
                            self.requests.push("abort".into());
                            if matches!(self.hold_control, Some("snapshot" | "finished")) {
                                // The original result crosses its bound and
                                // arrives before the correlated abort answer.
                                let late = self.held_control.take().unwrap();
                                assert_eq!(late.common(), request.common());
                                self.peer
                                    .write_all(
                                        &s::encode_state_line(&late, s::FrameDirection::Response)
                                            .unwrap(),
                                    )
                                    .unwrap();
                                self.hold_control = None;
                            }
                            s::StateControlAnswer::Aborted {
                                capture_id: *capture_id,
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
                                    let pairs = self.boot_pairs.iter().filter(|pair| !self.filings.iter().any(|file| matches!(file, s::StateFrame::FilePair { filing, .. } if matches!(&filing.pair, s::PairRecord::Ended(ended) if ended.call_id == pair.call_id)))).cloned().collect();
                                    serde_json::to_vec(&s::QueryResult::OpenPairs { pairs })
                                        .unwrap()
                                }
                                s::ReadSelector::PlanCurrent {} => {
                                    self.requests.push("summary".into());
                                    self.summary.as_ref().map(|result| serde_json::to_vec(result).unwrap()).unwrap_or_else(|| br#"{"plan_current":{"current":null,"next_plan_id":"40","next_step_id":"50"}}"#.to_vec())
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
                            } else if mode == "summary-refused"
                                && matches!(selector, s::ReadSelector::PlanCurrent {})
                            {
                                s::StateControlAnswer::ControlRefused {
                                    code: s::ControlRefusalCode::Read(
                                        s::ReadRefusalCode::ResultTooLarge,
                                    ),
                                    detail: "original boot summary refusal".into(),
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
                    frame: s::StateControl::Snapshot { .. },
                    ..
                } => self.hold_control == Some("snapshot"),
                s::StateFrame::Control {
                    frame: s::StateControl::Acknowledge { .. },
                    ..
                } => self.hold_control == Some("finished"),
                s::StateFrame::Control {
                    frame:
                        s::StateControl::Read {
                            selector: s::ReadSelector::OpenPairs {},
                        },
                    ..
                } => {
                    self.hold_control == Some("pairs")
                        || (self.hold_control == Some("pairs-recheck")
                            && self
                                .requests
                                .iter()
                                .filter(|request| request.as_str() == "pairs")
                                .count()
                                == 2)
                }
                s::StateFrame::Control {
                    frame: s::StateControl::Grants {},
                    ..
                } => self.hold_control == Some("grants"),
                s::StateFrame::Control {
                    frame:
                        s::StateControl::Read {
                            selector: s::ReadSelector::History { .. },
                        },
                    ..
                } => self.hold_control == Some("read"),
                s::StateFrame::Control {
                    frame:
                        s::StateControl::Read {
                            selector: s::ReadSelector::SystemPrompt {},
                        },
                    ..
                } => self.hold_control == Some("prompt"),
                s::StateFrame::Control {
                    frame:
                        s::StateControl::Read {
                            selector: s::ReadSelector::PlanCurrent {},
                        },
                    ..
                } => self.hold_control == Some("summary"),
                s::StateFrame::Control {
                    frame:
                        s::StateControl::Read {
                            selector: s::ReadSelector::Marks {},
                        },
                    ..
                } => self.hold_control == Some("marks"),
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

struct ForeignRun {
    run: Box<FoundationRun>,
    peer: OwnedFd,
    reader: UnixStream,
    member: UnixStream,
}

fn foreign_run(original: &[u8], root: &std::path::Path) -> ForeignRun {
    // A second actual root Enter owns independent SCM_RIGHTS descriptors.
    // Its organs are not started; it must remain untouched by the foreign join.
    let path = root.join("foreign-admin.sock");
    let mut admission =
        FoundationAdmission::listen(crate::bind_coordination(&path).unwrap()).unwrap();
    let peer = dial(&path);
    let (sink, reader) = UnixStream::pair().unwrap();
    let (state, member) = UnixStream::pair().unwrap();
    let mut envelope: weaver_types::state_flow::enter::EnterEnvelope =
        serde_json::from_slice(original).unwrap();
    let weaver_types::state_flow::enter::EnterContent::Directive(
        weaver_types::state_flow::enter::EnterDirective::Enter { payload },
    ) = &mut envelope.payload;
    let weaver_types::state_flow::enter::EnterBinding::Serving { enter_window, .. } =
        &mut payload.binding
    else {
        panic!()
    };
    let now = crate::flow::clock::monotonic_ns().unwrap();
    *enter_window = weaver_types::state_flow::EnterWindow {
        began_ns: now.into(),
        deadline_ns: (now + 5_000_000_000).into(),
    };
    let bytes = envelope.encode().unwrap();
    assert_eq!(
        sendmsg::<()>(
            peer.as_raw_fd(),
            &[std::io::IoSlice::new(&bytes)],
            &[ControlMessage::ScmRights(&[
                sink.as_raw_fd(),
                state.as_raw_fd()
            ])],
            MsgFlags::MSG_NOSIGNAL,
            None
        )
        .unwrap(),
        bytes.len()
    );
    drop((sink, state));
    assert_eq!(admission.advance(), FoundationAdmissionProgress::Received);
    assert_eq!(admission.received.as_ref().unwrap().wire(), bytes);
    let run = Box::new(
        admission
            .adopt()
            .ok()
            .expect("actual foreign Enter adoption"),
    );
    ForeignRun {
        run,
        peer,
        reader,
        member,
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
    let mut joint = leave_joint::is_mode(mode).then(|| leave_joint::Worker::begin(&files.0));
    let path = joint
        .as_ref()
        .map_or_else(|| files.0.join("admin.sock"), |joint| joint.socket.clone());
    let mut admission =
        FoundationAdmission::listen(crate::bind_coordination(&path).unwrap()).unwrap();
    assert_eq!(admission.advance(), FoundationAdmissionProgress::Pending);
    if mode == "unprivileged" {
        refuse_unprivileged(&mut admission, &path);
    }
    let peer = dial(&path);
    let (sink, mut reader) = UnixStream::pair().unwrap();
    if mode == "expired-startup-activation-trace" {
        // A large actual Load blocks its writer on this small real socket.
        // Nothing consumes the reader until the rollback deadline is checked.
        nix::sys::socket::setsockopt(&sink, nix::sys::socket::sockopt::SndBuf, &1024).unwrap();
        program.composer.binary = "worker".repeat(65536);
    }
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
    if super::input::generation::session_tests::forced_leave::is_mode(mode)
        || super::input::generation::session_tests::demand_save::is_mode(mode)
    {
        // Capture must file the wind-down's actual content through the normal
        // member. The fake-clock fixture's 1 ms cut allowance is not this
        // real-process circuit's declared test budget.
        config.cut_wait.elapsed_ns = 5_000_000_000.into();
    }
    let fixture = Fixture::new(config.clone());
    let event = fixture.load_event();
    let Some(weaver_trace::serving::Payload::Elections(mut load)) = event.payload else {
        panic!()
    };
    load.ste100_scheme = bootstrap.scheme_text.clone();
    load.state_flow.gate_wait.elapsed_ns = config.gate_wait.elapsed_ns.0.into();
    load.state_flow.cut_wait.elapsed_ns = config.cut_wait.elapsed_ns.0.into();
    if super::input::generation::session_tests::forced_leave::is_mode(mode)
        || super::input::generation::session_tests::demand_save::is_mode(mode)
    {
        load.state_flow.persistence.save_wait.elapsed_ns = 10_000_000_000.into();
        load.state_flow.persistence.snapshot_wait.elapsed_ns = 2_000_000_000.into();
        if mode.ends_with("-window-timeout")
            || mode.ends_with("-published-timeout")
            || mode.ends_with("-window-timeout-retry")
        {
            load.state_flow.persistence.save_wait.elapsed_ns = 1_000_000_000.into();
        }
        if mode.contains("-organ-loss-expiry-") {
            // The actual writer queue takes several seconds to fill in this
            // debug fixture. Leave time to retain the fault at trace admission
            // before the declared window expires; never replace its deadline.
            load.state_flow.persistence.save_wait.elapsed_ns = 6_000_000_000.into();
        }
        if super::input::generation::session_tests::demand_save::is_mode(mode)
            && mode.ends_with("-expires")
        {
            load.state_flow.persistence.save_wait.elapsed_ns = 250_000_000.into();
            load.state_flow.persistence.snapshot_wait.elapsed_ns = 50_000_000.into();
        }
    }
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
    let normal_image = joint
        .as_ref()
        .map(|_| normal_member::NormalMember::seed_image(&files.0, &bootstrap.scheme_text));
    if let Some(image) = &normal_image {
        load.state_start.dump_sha256 = normal_member::digest(image).as_str().into();
    }
    let mut normal = (mode.contains("-save-sqlite") && joint.is_none()).then(|| {
        let mut normal = normal_member::NormalMember::seeded(
            &files.0,
            &bootstrap.scheme_text,
            mode.ends_with("-held-filing"),
        );
        if mode.ends_with("-snapshot-timeout-retry") || mode.ends_with("-bad-aborted") {
            normal.delay_first_capture_result(normal_member::CaptureDelay::Snapshot);
        } else if mode.ends_with("-finished-timeout-retry") {
            normal.delay_first_capture_result(normal_member::CaptureDelay::Finished);
        }
        normal.corrupt_abort = mode.ends_with("-bad-aborted");
        load.state_start.dump_sha256 = normal_member::digest(&normal.image).as_str().into();
        normal
    });
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
    let mut envelope = envelope;
    if mode.contains("graceful") {
        let weaver_types::state_flow::enter::EnterContent::Directive(
            weaver_types::state_flow::enter::EnterDirective::Enter { payload },
        ) = &mut envelope.payload;
        payload.drain_bound = (mode.contains("graceful-expiry")
            || mode.contains("-graceful-save-sqlite-drain-expiry")
            || mode.contains("-graceful-save-sqlite-quiesce-expiry")
            || (mode.contains("-quiesce-secondary-") && mode.ends_with("-expiry"))
            || mode.ends_with("quiesce-expiry")
            || mode.ends_with("quiesce-bounded"))
        .then_some(1);
        payload.wind_down_bound = (mode.contains("-wind-down-") && !mode.ends_with("unbounded"))
            .then_some(
                if mode.ends_with("expiry") || mode.ends_with("join-delayed") {
                    1
                } else if mode.ends_with("-wind-down-author-wait") {
                    // Filling the real trace queue consumes this allowance too.
                    // Leave time for Observe and the retained author to finish.
                    15
                } else {
                    5
                },
            );
        if mode.contains("-wind-convert-expiry") {
            payload.wind_down_bound = Some(1);
        }
    }
    if let Some(joint) = &mut joint {
        let weaver_types::state_flow::enter::EnterContent::Directive(
            weaver_types::state_flow::enter::EnterDirective::Enter { payload },
        ) = &mut envelope.payload;
        let image = normal_image.unwrap();
        joint.prepare(payload, &image);
        normal = Some(normal_member::NormalMember::from_image(
            &files.0,
            image,
            mode.ends_with("-held-filing"),
            Some((joint.lock.as_ref().unwrap(), &joint.room)),
        ));
    }
    if mode.ends_with("-bad-release") {
        normal.as_mut().unwrap().corrupt_release = true;
    }
    let now = if mode == "worker"
        || mode.starts_with("input-worker")
        || mode == "failed-startup-worker"
        || mode == "rollback-failed-worker"
    {
        crate::flow::clock::monotonic_ns().unwrap()
    } else {
        100
    };
    let weaver_types::state_flow::enter::EnterContent::Directive(
        weaver_types::state_flow::enter::EnterDirective::Enter { payload },
    ) = &mut envelope.payload;
    let weaver_types::state_flow::enter::EnterBinding::Serving { enter_window, .. } =
        &mut payload.binding
    else {
        panic!()
    };
    *enter_window = weaver_types::state_flow::EnterWindow {
        began_ns: now.into(),
        deadline_ns: (now
            + if mode == "worker" || mode.starts_with("input-worker") {
                5_000_000_000
            } else {
                1_000_000_000
            })
        .into(),
    };
    if mode == "window-future" {
        enter_window.began_ns = 101.into();
    } else if mode == "window-expired" {
        enter_window.began_ns = 1.into();
        enter_window.deadline_ns = 100.into();
    }
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
    let mut clock = FakePumpIo::new(
        if mode == "failed-startup-worker" || mode == "rollback-failed-worker" {
            now
        } else {
            100
        },
    );
    let received = if mode == "worker" || mode.starts_with("input-worker") {
        admission.advance()
    } else {
        admission.advance_with_io(&mut clock)
    };
    if mode == "window-future" || mode == "window-expired" {
        assert_eq!(received, FoundationAdmissionProgress::Stopped);
        assert_eq!(admission.received.as_ref().unwrap().wire(), bytes);
        let response = packet(&peer);
        if mode == "window-future" {
            let response = response.expect("before-load Malformed on actual valid exchange");
            assert_eq!(response.exchange, envelope.exchange);
            assert_eq!(response.position, weaver_types::Position::Close);
            assert!(matches!(
                response.payload,
                weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::Malformed)
            ));
        } else {
            assert!(response.is_none(), "expiry produces no substitute answer");
        }
        reader.set_nonblocking(true).unwrap();
        member.set_nonblocking(true).unwrap();
        assert_eq!(reader.read(&mut [0]).unwrap(), 0, "no load/refusal trace");
        assert_eq!(
            (&member).read(&mut [0]).unwrap(),
            0,
            "actual unadmitted descriptors closed, no state Open"
        );
        assert!(admission.adopt_with_io(&mut clock).is_err());
        return;
    }
    if mode == "malformed" {
        assert_eq!(received, FoundationAdmissionProgress::Stopped);
        let detail = admission.detail().unwrap().to_owned();
        assert_eq!(admission.advance(), FoundationAdmissionProgress::Stopped);
        assert_eq!(admission.detail(), Some(detail.as_str()));
        assert!(admission.adopt().is_err());
        assert!(packet(&peer).is_none());
        return;
    }
    assert_eq!(received, FoundationAdmissionProgress::Received);
    assert_eq!(admission.received.as_ref().unwrap().wire(), bytes);
    assert_eq!(admission.advance(), FoundationAdmissionProgress::Received);
    let adopted = if mode == "worker" || mode.starts_with("input-worker") {
        admission.adopt()
    } else {
        admission.adopt_with_io(&mut clock)
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
    let mut run = RunGuard(Some(adopted.map_err(|a| a.detail).ok().unwrap()));
    if matches!(mode, "missing-checker" | "bad-checker-path") {
        program.organs.classify = if mode == "missing-checker" {
            None
        } else {
            Some(PathBuf::from("/synthetic/not-in-enter-stack"))
        };
        assert!(run.0.as_mut().unwrap().begin(program).is_err());
        assert_eq!(
            run.0.as_mut().unwrap().original.as_ref().unwrap().wire(),
            bytes
        );
        assert!(
            run.0
                .as_mut()
                .unwrap()
                .resources
                .classifier_child()
                .is_none()
        );
        program = files.program("ready");
    }
    let bound = run
        .0
        .as_mut()
        .unwrap()
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
        normal,
        fixture_root: Some(files.0.clone()),
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
        boot_pairs: vec![],
        summary: None,
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
        let result = unsafe { worker::enter(run.0.as_mut().unwrap(), program) }.unwrap();
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        member = responder.join().unwrap();
        let mut serving = match result {
            Ok(serving) => serving,
            Err(owner) => {
                let guard = EnterGuard(Some(owner));
                panic!("worker refused: {:?}", guard.0.as_ref().unwrap().detail());
            }
        };
        if mode.ends_with("-session-ready-rollback-late") {
            // The worker's Ready is wholly sent and its seat is already
            // granted. Root deliberately observes that actual datagram only
            // after its original Enter window has ended.
            while crate::flow::clock::monotonic_ns().unwrap() <= bound.deadline_ns() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert_serving(&mut serving, &peer, &bytes, &member, bound);
        if mode.starts_with("input-worker") {
            drop(peer);
            if mode.ends_with("-demand-receive-clock") {
                super::input::generation::session_tests::demand_save::receive_clock::exercise(
                    serving, &path,
                );
                return;
            }
            if super::input::generation::session_tests::demand_save::organ_loss::is_mode(mode) {
                let handoff =
                    super::input::generation::session_tests::demand_save::organ_loss::handoff(
                        serving,
                        &mut member,
                        &mut reader,
                        &path,
                        &files.0,
                        mode,
                    );
                let actual = run.take_boxed();
                super::input::generation::session_tests::demand_save::organ_loss::finish(
                    actual,
                    handoff,
                    &mut member,
                    &mut reader,
                    &files.0,
                    mode,
                );
                return;
            }
            if super::input::generation::session_tests::forced_leave::is_mode(mode) {
                let handoff = super::input::generation::session_tests::forced_leave::handoff(
                    serving,
                    &path,
                    &files.0,
                    mode,
                    &mut member,
                    &mut reader,
                );
                let actual = run.take_boxed();
                super::input::generation::session_tests::forced_leave::finish(
                    actual,
                    handoff,
                    &mut member,
                    &mut reader,
                    &files.0,
                    mode,
                );
                return;
            }
            if super::input::generation::session_tests::serving_rollback::is_mode(mode) {
                let handoff = super::input::generation::session_tests::serving_rollback::handoff(
                    serving,
                    &path,
                    &files.0,
                    mode,
                    &mut member,
                    &mut reader,
                );
                let actual = run.take_boxed();
                super::input::generation::session_tests::serving_rollback::finish(
                    actual,
                    handoff,
                    &mut member,
                    &mut reader,
                    &files.0,
                    mode,
                );
                return;
            }
            if super::input::generation::session_tests::ready_rollback::is_mode(mode) {
                let Some(mut handoff) =
                    super::input::generation::session_tests::ready_rollback::handoff(
                        serving, &path, &files.0, mode,
                    )
                else {
                    return;
                };
                if mode.ends_with("-foreign") {
                    let mut foreign = foreign_run(&bytes, &files.0);
                    let (retained, checked) =
                        super::input::generation::session_tests::ready_rollback::reject_foreign(
                            foreign.run,
                            handoff,
                        );
                    foreign.run = retained;
                    handoff = checked;
                    assert!(packet(&foreign.peer).is_none());
                    foreign.reader.set_nonblocking(true).unwrap();
                    foreign.member.set_nonblocking(true).unwrap();
                    for stream in [&mut foreign.reader, &mut foreign.member] {
                        assert!(matches!(stream.read(&mut [0]), Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock));
                    }
                }
                let actual = run.take_boxed();
                super::input::generation::session_tests::ready_rollback::finish(
                    actual,
                    handoff,
                    &mut member,
                    &mut reader,
                    &files.0,
                    mode,
                );
                return;
            }
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
                if super::input::generation::session_tests::demand_save::failed::is_mode(mode) {
                    let actual = run.take_boxed();
                    super::input::generation::session_tests::demand_save::failed::finish(
                        actual,
                        &mut member,
                        &mut reader,
                        &files.0,
                        mode,
                    );
                } else if mode.contains("-exit") {
                    let actual = run.take_boxed();
                    super::input::halt::exit::tests::finish(
                        actual,
                        &mut member,
                        &mut reader,
                        &mut clock,
                        &files.0,
                        mode,
                    );
                }
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
    let mut guard = EnterGuard(Some(run.0.as_mut().unwrap().begin(program).unwrap()));
    let mut owner = guard.0.take().unwrap();
    owner = owner
        .finish()
        .err()
        .expect("unfinished Enter cannot grant a seat");
    guard.0 = Some(owner);
    if mode.starts_with("failed-startup-") {
        let handoff = super::failed::tests::enter(
            guard.0.take().unwrap(),
            &mut member,
            &mut reader,
            &mut clock,
            mode,
        );
        drop(guard);
        if let Some(handoff) = handoff {
            super::failed::tests::finish(
                run.take_boxed(),
                handoff,
                &peer,
                &mut member,
                &mut reader,
                &mut clock,
                &path,
                &bytes,
                mode,
            );
        }
        return;
    }
    if mode.starts_with("expired-startup-") {
        if mode == "expired-startup-refusal" {
            super::expired::tests::refusal(guard.0.take().unwrap(), &mut member, &mut clock);
            return;
        }
        let handoff = super::expired::tests::enter(
            guard.0.take().unwrap(),
            &mut member,
            &mut reader,
            &mut clock,
            mode,
            &files.0,
        );
        drop(guard);
        super::expired::tests::finish(
            run.take_boxed(),
            handoff,
            &peer,
            &mut member,
            &mut reader,
            &mut clock,
            &path,
            &bytes,
            mode,
        );
        return;
    }
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
    if mode.starts_with("rollback-failed-") {
        let case = super::rollback::failed::tests::enter(
            guard.0.take().unwrap(),
            &peer,
            &mut member,
            &mut reader,
            &mut clock,
            &path,
            mode,
        );
        drop(guard);
        if let Some(case) = case {
            super::rollback::failed::tests::finish(
                run.take_boxed(),
                case,
                &peer,
                &mut member,
                &mut reader,
                &mut clock,
                &path,
                &bytes,
                envelope.enter().cause,
                mode,
            );
        }
        return;
    }
    if mode.starts_with("rollback-") {
        assert_eq!(
            guard.0.as_ref().unwrap().progress(),
            FoundationEnterProgress::Stopped
        );
        super::rollback::tests::exercise(
            guard.0.take().unwrap(),
            &peer,
            &mut member,
            &mut reader,
            &mut clock,
            &path,
            &envelope,
            mode,
        );
        return;
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
        drop(guard);
        assert_serving(&mut serving, &peer, &bytes, &member, bound);
        if mode.starts_with("input-") {
            drop(peer); // Normal admin departure does not revoke serving.
            if mode.contains("-lower-organ-") {
                let (handoff, rows, observed_at) = super::input::halt::exit::organ_tests::start(
                    serving,
                    &mut member,
                    &mut reader,
                    &mut clock,
                    &files.0,
                );
                super::input::halt::exit::organ_tests::finish(
                    run.take_boxed(),
                    handoff,
                    rows,
                    observed_at,
                    &mut member,
                    &mut reader,
                    &mut clock,
                    &files.0,
                    mode,
                );
                return;
            }
            if mode.contains("start-pending") {
                super::input::halt::exit::tests::start_pending(
                    serving,
                    &mut member,
                    &mut reader,
                    &mut clock,
                );
                let actual = run.take_boxed();
                super::input::halt::exit::tests::finish(
                    actual,
                    &mut member,
                    &mut reader,
                    &mut clock,
                    &files.0,
                    mode,
                );
                return;
            }
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
                if super::input::generation::session_tests::demand_save::failed::is_mode(mode) {
                    let actual = run.take_boxed();
                    super::input::generation::session_tests::demand_save::failed::finish(
                        actual,
                        &mut member,
                        &mut reader,
                        &files.0,
                        mode,
                    );
                } else if mode.contains("-exit") {
                    let actual = run.take_boxed();
                    super::input::halt::exit::tests::finish(
                        actual,
                        &mut member,
                        &mut reader,
                        &mut clock,
                        &files.0,
                        mode,
                    );
                }
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
    if leave_joint::is_mode(mode) {
        leave_joint::scenario(name, mode);
        return;
    }
    let layout = crate::tools_cgroup::tests::ScratchLayout::new(&format!("worker-{mode}"));
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", name, "--ignored", "--nocapture"])
        .env("WEAVER_WORKER_STARTUP_CASE", mode)
        .env("WEAVER_GATE_STARTUP_RUN", &layout.run);
    let status = if super::input::generation::session_tests::serving_rollback::is_mode(mode)
        || super::input::generation::session_tests::forced_leave::is_mode(mode)
        || super::input::generation::session_tests::demand_save::is_mode(mode)
        || mode.contains("-graceful-quiesce-")
        || mode.contains("-graceful-wind-down-")
    {
        // The public driver can block inside a real transport call. A parent
        // watchdog still fails that fixture and reaps its owned process tree.
        let mut child = command.spawn().unwrap();
        let limit = Instant::now() + Duration::from_secs(35);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= limit {
                let _ = child.kill();
                let _ = child.wait();
                drop(layout);
                panic!("serving rollback circuit {mode} exceeded its outer watchdog");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    } else {
        command.status().unwrap()
    };
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
scenario!(
    worker_session_demand_save_receive_clock,
    "input-worker-generation-emission-session-demand-receive-clock"
);
scenario!(
    worker_session_demand_save_sqlite_organ_loss_gate,
    "input-worker-generation-emission-session-demand-save-sqlite-organ-loss-gate"
);
scenario!(
    worker_session_demand_save_sqlite_organ_loss_spu,
    "input-worker-generation-emission-session-demand-save-sqlite-organ-loss-spu"
);
scenario!(
    worker_session_demand_save_sqlite_organ_loss_expiry_gate,
    "input-worker-generation-emission-session-demand-save-sqlite-organ-loss-expiry-gate"
);
scenario!(
    worker_session_demand_save_sqlite_idle,
    "input-worker-generation-emission-session-demand-save-sqlite-idle"
);
scenario!(
    worker_session_demand_save_sqlite_queued,
    "input-worker-generation-emission-session-demand-save-sqlite-queued"
);
scenario!(
    worker_session_demand_save_sqlite_expired,
    "input-worker-generation-emission-session-demand-save-sqlite-expired"
);
scenario!(
    worker_session_demand_save_sqlite_expires_during_generation,
    "input-worker-generation-emission-session-demand-save-sqlite-expires"
);
scenario!(
    worker_session_demand_save_sqlite_held_filing,
    "input-worker-generation-emission-session-demand-save-sqlite-held-filing"
);
scenario!(
    worker_session_demand_save_sqlite_joint,
    "input-worker-generation-emission-session-demand-save-sqlite-joint"
);
scenario!(
    worker_session_demand_save_sqlite_bad_published,
    "input-worker-generation-emission-session-demand-save-sqlite-bad-published"
);
scenario!(
    worker_session_demand_save_sqlite_bad_aborted,
    "input-worker-generation-emission-session-demand-save-sqlite-bad-aborted"
);
scenario!(
    worker_session_demand_save_sqlite_secondary_management,
    "input-worker-generation-emission-session-demand-save-sqlite-secondary"
);
scenario!(
    worker_session_demand_save_sqlite_snapshot_timeout_retry,
    "input-worker-generation-emission-session-demand-save-sqlite-snapshot-timeout-retry"
);
scenario!(
    worker_session_demand_save_sqlite_finished_timeout_retry,
    "input-worker-generation-emission-session-demand-save-sqlite-finished-timeout-retry"
);
scenario!(
    worker_session_demand_save_sqlite_image_window_timeout_retry,
    "input-worker-generation-emission-session-demand-save-sqlite-image-window-timeout-retry"
);
scenario!(
    worker_session_demand_save_sqlite_prepare_window_timeout_retry,
    "input-worker-generation-emission-session-demand-save-sqlite-prepare-window-timeout-retry"
);
scenario!(
    worker_session_forced_leave_joint_sqlite_idle,
    "input-worker-generation-emission-session-forced-save-sqlite-joint"
);
scenario!(
    worker_session_forced_leave_joint_sqlite_held_filing,
    "input-worker-generation-emission-session-forced-save-sqlite-joint-held-filing"
);
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
    worker_external_composition_gate_death_mixed,
    "input-generation-emission-pairs-external-gate-death-mixed"
);
scenario!(
    worker_external_composition_gate_death_crossed,
    "input-generation-emission-pairs-external-gate-death-crossed"
);
scenario!(
    worker_external_composition_gate_death_unsent,
    "input-generation-emission-pairs-external-gate-death-unsent"
);

scenario!(
    worker_external_composition_gate_death_repeat,
    "input-generation-emission-pairs-external-gate-death-repeat"
);
scenario!(
    worker_external_composition_gate_death_pressure,
    "input-generation-emission-pairs-external-gate-death-pressure"
);
scenario!(
    worker_external_composition_gate_death_fit_refusal,
    "input-generation-emission-pairs-external-gate-death-fit-refusal"
);

scenario!(
    worker_external_composition_gate_death_stop,
    "input-generation-emission-pairs-external-gate-death-stop"
);
scenario!(
    worker_external_composition_gate_death_stop_crossed,
    "input-generation-emission-pairs-external-gate-death-stop-crossed"
);
scenario!(
    worker_external_composition_gate_death_stop_peer_eof,
    "input-generation-emission-pairs-external-gate-death-stop-peer-eof"
);

scenario!(
    worker_external_composition_gate_death_report,
    "input-generation-emission-pairs-external-gate-death-report"
);
scenario!(
    worker_external_composition_gate_death_report_pressure,
    "input-generation-emission-pairs-external-gate-death-report-pressure"
);
scenario!(
    worker_external_composition_gate_death_stop_report,
    "input-generation-emission-pairs-external-gate-death-stop-report"
);
scenario!(
    worker_external_composition_gate_death_stop_report_unread,
    "input-generation-emission-pairs-external-gate-death-stop-report-unread"
);
scenario!(
    worker_external_composition_gate_death_stop_report_crossed_unread,
    "input-generation-emission-pairs-external-gate-death-stop-report-crossed-unread"
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
    worker_session_composition_serving_rollback_idle,
    "input-worker-generation-emission-session-serving-rollback-idle"
);
scenario!(
    worker_session_composition_serving_rollback_accepted,
    "input-worker-generation-emission-session-serving-rollback-accepted"
);
scenario!(
    worker_session_composition_serving_rollback_checking,
    "input-worker-generation-emission-session-serving-rollback-checking"
);
scenario!(
    worker_session_composition_serving_rollback_source_elected,
    "input-worker-generation-emission-session-serving-rollback-source-elected"
);
scenario!(
    worker_session_composition_serving_rollback_source_pending,
    "input-worker-generation-emission-session-serving-rollback-source-pending"
);
scenario!(
    worker_session_composition_serving_rollback_source_file,
    "input-worker-generation-emission-session-serving-rollback-source-file"
);
scenario!(
    worker_session_composition_serving_rollback_task_read,
    "input-worker-generation-emission-session-serving-rollback-task-read"
);
scenario!(
    worker_session_composition_serving_rollback_mark_file,
    "input-worker-generation-emission-session-serving-rollback-mark-file"
);
scenario!(
    worker_session_composition_serving_rollback_placement,
    "input-worker-generation-emission-session-serving-rollback-placement"
);
scenario!(
    worker_session_composition_serving_rollback_placement_decision_bound,
    "input-worker-generation-emission-session-serving-rollback-placement-decision-bound"
);
scenario!(
    worker_session_composition_serving_rollback_active_generation,
    "input-worker-generation-emission-session-serving-rollback-active-generation"
);
scenario!(
    worker_session_composition_serving_rollback_correction_placement,
    "input-worker-generation-emission-session-serving-rollback-correction-placement"
);
scenario!(
    worker_session_composition_serving_rollback_correction_generation,
    "input-worker-generation-emission-session-serving-rollback-correction-generation"
);
scenario!(
    worker_session_composition_serving_rollback_active_generation_queued,
    "input-worker-generation-emission-session-serving-rollback-active-generation-queued"
);
scenario!(
    worker_session_composition_serving_rollback_external_unload,
    "input-worker-generation-emission-session-serving-rollback-external-unload"
);
scenario!(
    worker_session_composition_serving_rollback_external_crossed,
    "input-worker-generation-emission-session-serving-rollback-external-crossed"
);
scenario!(
    worker_session_composition_serving_rollback_external_silent,
    "input-worker-generation-emission-session-serving-rollback-external-silent"
);
scenario!(
    worker_session_composition_serving_rollback_external_unload_queued,
    "input-worker-generation-emission-session-serving-rollback-external-unload-queued"
);
scenario!(
    worker_session_composition_serving_rollback_external_interrupt_blocked,
    "input-worker-generation-emission-session-serving-rollback-external-interrupt-blocked"
);
scenario!(
    worker_session_composition_serving_rollback_external_rejection_pending,
    "input-worker-generation-emission-session-serving-rollback-external-rejection-pending"
);
scenario!(
    worker_session_composition_serving_rollback_final_close,
    "input-worker-generation-emission-session-serving-rollback-final-close"
);
scenario!(
    worker_session_composition_serving_rollback_world_blocked,
    "input-worker-generation-emission-session-serving-rollback-world-blocked"
);
scenario!(
    worker_session_composition_serving_rollback_placed_clock_failure,
    "input-worker-generation-emission-session-serving-rollback-placed-clock-failure"
);
scenario!(
    worker_session_composition_serving_rollback_active_generation_cap,
    "input-worker-generation-emission-session-serving-rollback-active-generation-cap"
);
scenario!(
    worker_session_composition_serving_rollback_active_generation_clock_failure,
    "input-worker-generation-emission-session-serving-rollback-active-generation-clock-failure"
);
scenario!(
    worker_session_composition_serving_rollback_active_generation_trace_failure,
    "input-worker-generation-emission-session-serving-rollback-active-generation-trace-failure"
);
scenario!(
    worker_session_composition_ready_rollback,
    "input-worker-generation-emission-session-ready-rollback"
);
scenario!(
    worker_session_composition_ready_rollback_late,
    "input-worker-generation-emission-session-ready-rollback-late"
);
scenario!(
    worker_session_composition_ready_rollback_queued,
    "input-worker-generation-emission-session-ready-rollback-queued"
);
scenario!(
    worker_session_composition_ready_rollback_held,
    "input-worker-generation-emission-session-ready-rollback-held"
);
scenario!(
    worker_session_composition_ready_rollback_foreign,
    "input-worker-generation-emission-session-ready-rollback-foreign"
);
scenario!(
    worker_session_composition_ready_rollback_nonrollback,
    "input-worker-generation-emission-session-ready-rollback-nonrollback"
);
scenario!(
    worker_session_composition_ready_rollback_edge_leave_cap,
    "input-worker-generation-emission-session-ready-rollback-edge-leave-cap"
);
scenario!(
    worker_session_composition_ready_rollback_edge_clock_failure,
    "input-worker-generation-emission-session-ready-rollback-edge-clock-failure"
);
scenario!(
    worker_session_composition_ready_rollback_edge_recorded_clock,
    "input-worker-generation-emission-session-ready-rollback-edge-recorded-clock"
);
scenario!(
    worker_session_composition_ready_rollback_edge_unload_cap,
    "input-worker-generation-emission-session-ready-rollback-edge-unload-cap"
);
scenario!(
    worker_session_composition_ready_rollback_edge_drained_clock,
    "input-worker-generation-emission-session-ready-rollback-edge-drained-clock"
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

scenario!(
    worker_external_composition_failed_exit,
    "input-generation-emission-external-halt-gate-exit"
);

scenario!(
    worker_external_composition_failed_exit_eof,
    "input-generation-emission-external-halt-gate-eof-exit"
);

scenario!(
    worker_external_composition_failed_exit_control,
    "input-generation-emission-external-halt-gate-control-exit"
);

scenario!(
    worker_external_composition_failed_exit_deadline,
    "input-generation-emission-external-halt-gate-deadline-exit"
);

scenario!(
    worker_external_composition_failed_exit_late,
    "input-generation-emission-external-halt-gate-exit-late"
);

scenario!(
    worker_external_composition_failed_exit_pressure_hold_late,
    "input-generation-emission-external-halt-gate-exit-pressure-hold-late"
);

scenario!(
    worker_external_composition_failed_exit_trace_loss,
    "input-generation-emission-external-halt-gate-exit-trace-loss"
);

scenario!(
    worker_external_composition_failed_exit_management,
    "input-generation-emission-external-halt-gate-exit-management"
);

scenario!(
    worker_external_composition_failed_exit_lower_silent,
    "input-generation-emission-external-halt-gate-exit-lower-silent"
);

scenario!(
    worker_external_composition_failed_exit_lower_wrong,
    "input-generation-emission-external-halt-gate-exit-lower-wrong"
);

scenario!(
    worker_external_composition_failed_exit_release_silent,
    "input-generation-emission-external-halt-gate-exit-release-silent"
);

scenario!(
    worker_external_composition_failed_exit_release_wrong,
    "input-generation-emission-external-halt-gate-exit-release-wrong"
);

scenario!(
    worker_external_composition_failed_exit_clock_error,
    "input-generation-emission-external-halt-gate-exit-clock-error"
);

scenario!(
    worker_external_composition_failed_exit_clock_reverse,
    "input-generation-emission-external-halt-gate-exit-clock-reverse"
);

scenario!(
    worker_external_composition_failed_exit_worker,
    "input-generation-emission-external-halt-gate-worker-exit"
);

scenario!(
    worker_external_composition_failed_exit_pending_world,
    "input-generation-emission-external-halt-gate-exit-pending-world"
);
scenario!(
    worker_external_composition_failed_exit_sent_world,
    "input-generation-emission-external-halt-gate-exit-sent-world"
);
scenario!(
    worker_external_composition_failed_exit_world_silent,
    "input-generation-emission-external-halt-gate-exit-world-silent"
);
scenario!(
    worker_external_composition_failed_exit_live_tools,
    "input-generation-emission-external-halt-gate-eof-exit-live-tools"
);
scenario!(
    worker_external_composition_failed_exit_live_tools_after_lower,
    "input-generation-emission-external-halt-gate-exit-live-tools"
);
scenario!(
    worker_external_composition_failed_exit_lower_late,
    "input-generation-emission-external-halt-gate-exit-lower-late"
);
scenario!(
    worker_external_composition_failed_exit_lower_pressure,
    "input-generation-emission-external-halt-gate-exit-lower-late-lower-backlog"
);
scenario!(
    worker_external_composition_failed_exit_lower_pressure_unload,
    "input-generation-emission-external-halt-gate-exit-lower-late-lower-backlog-terminal"
);
scenario!(
    worker_external_composition_failed_exit_lower_pressure_wake,
    "input-generation-emission-external-halt-gate-exit-lower-late-lower-backlog-wake"
);
scenario!(
    worker_external_composition_failed_exit_start_pending,
    "input-generation-external-halt-gate-exit-start-pending"
);
scenario!(
    worker_external_composition_failed_exit_lower_organ_wake,
    "input-generation-external-halt-gate-exit-lower-organ-wake"
);
scenario!(
    worker_external_composition_failed_exit_lower_organ_return,
    "input-generation-external-halt-gate-exit-lower-organ-return"
);
scenario!(
    worker_external_composition_failed_exit_lower_organ_cap,
    "input-generation-external-halt-gate-exit-lower-organ-cap"
);
scenario!(
    worker_external_composition_failed_exit_start_ready,
    "input-generation-external-halt-gate-exit-start-pending-capacity-ready"
);
scenario!(
    worker_external_composition_failed_exit_start_pending_expiry,
    "input-generation-external-halt-gate-exit-start-pending-start-expiry"
);

scenario!(
    worker_external_composition_failed_exit_pending_turn,
    "input-generation-emission-external-halt-gate-exit-pressure-pending-turn"
);
scenario!(
    worker_external_composition_failed_exit_pending_session,
    "input-generation-emission-external-halt-gate-exit-pressure-pending-session"
);
scenario!(
    worker_external_composition_failed_exit_placement_pending,
    "input-generation-external-halt-gate-exit-placement-pending"
);
scenario!(
    worker_external_composition_failed_exit_placement_ready,
    "input-generation-external-halt-gate-exit-placement-pending-capacity-ready"
);
scenario!(
    worker_external_composition_failed_exit_placement_trace_loss,
    "input-generation-external-halt-gate-exit-placement-pending-trace-loss"
);
scenario!(
    worker_external_composition_failed_exit_rejection_unavailable,
    "input-generation-emission-external-halt-gate-exit-rejection-unavailable-pressure-hold-late"
);
scenario!(
    worker_external_composition_failed_exit_rejection_unavailable_lower_late,
    "input-generation-emission-external-halt-gate-exit-lower-late-rejection-unavailable-pressure-hold-late"
);

scenario!(
    worker_startup_composition_original_window_future_refused,
    "window-future"
);
scenario!(
    worker_startup_composition_original_window_expiry_unadmitted,
    "window-expired"
);

scenario!(
    worker_startup_composition_refusal_rollback_left,
    "rollback-left"
);
scenario!(
    worker_startup_composition_refusal_rollback_late,
    "rollback-late"
);
scenario!(
    worker_startup_composition_refusal_rollback_expired,
    "rollback-expired"
);

scenario!(
    worker_startup_composition_refusal_rollback_varied,
    "rollback-varied"
);

scenario!(
    worker_startup_composition_refusal_rollback_secondary,
    "rollback-secondary"
);

scenario!(
    worker_startup_composition_refusal_rollback_enter_lost,
    "rollback-enter-lost"
);

scenario!(
    worker_startup_composition_refusal_rollback_leave_lost,
    "rollback-leave-lost"
);

scenario!(
    worker_startup_composition_refusal_rollback_observer,
    "rollback-observer"
);

scenario!(
    worker_startup_composition_refusal_rollback_send_pressure,
    "rollback-send-pressure"
);

scenario!(
    worker_startup_composition_refusal_rollback_await_eof,
    "rollback-await-eof"
);
scenario!(
    worker_startup_composition_refusal_rollback_worker,
    "rollback-worker"
);

scenario!(
    worker_startup_composition_refusal_rollback_seal_silent,
    "rollback-seal-silent"
);
scenario!(
    worker_startup_composition_refusal_rollback_seal_backlog,
    "rollback-seal-backlog"
);
scenario!(
    worker_startup_composition_refusal_rollback_left_expired,
    "rollback-left-expired"
);

scenario!(
    worker_startup_composition_refusal_rollback_drain_expired,
    "rollback-drain-expired"
);

scenario!(
    worker_startup_composition_failed_exit_preopen,
    "failed-startup-preopen"
);

scenario!(
    worker_startup_composition_failed_exit_loaded,
    "failed-startup-loaded"
);

scenario!(
    worker_startup_composition_failed_exit_classifier,
    "failed-startup-classifier"
);

scenario!(
    worker_startup_composition_failed_exit_admit,
    "failed-startup-admit"
);

scenario!(
    worker_startup_composition_failed_exit_boot,
    "failed-startup-boot"
);

scenario!(
    worker_startup_composition_failed_exit_open,
    "failed-startup-open"
);

scenario!(
    worker_startup_composition_failed_exit_repair,
    "failed-startup-repair"
);

scenario!(
    worker_startup_composition_failed_exit_summary,
    "failed-startup-summary"
);

scenario!(
    worker_startup_composition_failed_exit_installed,
    "failed-startup-installed"
);

scenario!(
    worker_startup_composition_failed_exit_raised,
    "failed-startup-raised"
);

scenario!(
    worker_startup_composition_failed_exit_worker,
    "failed-startup-worker"
);

scenario!(
    worker_startup_composition_failed_exit_cap,
    "failed-startup-cap"
);

scenario!(
    worker_startup_composition_failed_exit_wait,
    "failed-startup-wait"
);

scenario!(
    worker_startup_composition_failed_exit_trace,
    "failed-startup-trace"
);

scenario!(
    worker_startup_composition_failed_exit_foreign,
    "failed-startup-foreign"
);

scenario!(
    worker_startup_composition_failed_exit_expiry,
    "failed-startup-expiry"
);

scenario!(
    worker_startup_composition_failed_exit_refusal,
    "failed-startup-refusal"
);

scenario!(
    worker_startup_composition_failed_exit_opened,
    "failed-startup-opened"
);

scenario!(
    worker_startup_composition_failed_exit_activation,
    "failed-startup-activation"
);

scenario!(
    worker_startup_composition_failed_exit_lower_wrong,
    "failed-startup-lower-wrong"
);
scenario!(
    worker_startup_composition_failed_exit_lower_early,
    "failed-startup-lower-early"
);
scenario!(
    worker_startup_composition_failed_exit_lower_silent,
    "failed-startup-lower-silent"
);

scenario!(
    worker_startup_composition_failed_rollback_record,
    "rollback-failed-record"
);

scenario!(
    worker_startup_composition_failed_rollback_recorded,
    "rollback-failed-recorded"
);

scenario!(
    worker_startup_composition_failed_rollback_response,
    "rollback-failed-response"
);

scenario!(
    worker_startup_composition_failed_rollback_await,
    "rollback-failed-await"
);

scenario!(
    worker_startup_composition_failed_rollback_transition,
    "rollback-failed-transition"
);

scenario!(
    worker_startup_composition_failed_rollback_grants,
    "rollback-failed-grants"
);

scenario!(
    worker_startup_composition_failed_rollback_prepared,
    "rollback-failed-prepared"
);

scenario!(
    worker_startup_composition_failed_rollback_seal,
    "rollback-failed-seal"
);

scenario!(
    worker_startup_composition_failed_rollback_state,
    "rollback-failed-state"
);

scenario!(
    worker_startup_composition_failed_rollback_unload,
    "rollback-failed-unload"
);

scenario!(
    worker_startup_composition_failed_rollback_observer,
    "rollback-failed-observer"
);

scenario!(
    worker_startup_composition_failed_rollback_drain_cap,
    "rollback-failed-drain-cap"
);

scenario!(
    worker_startup_composition_failed_rollback_left_peer,
    "rollback-failed-left-peer"
);

scenario!(
    worker_startup_composition_failed_rollback_leave_cap,
    "rollback-failed-leave-cap"
);

scenario!(
    worker_startup_composition_failed_rollback_trace,
    "rollback-failed-trace"
);

scenario!(
    worker_startup_composition_failed_rollback_foreign,
    "rollback-failed-foreign"
);

scenario!(
    worker_startup_composition_failed_rollback_worker,
    "rollback-failed-worker"
);

scenario!(
    worker_startup_composition_failed_rollback_wait,
    "rollback-failed-wait"
);

scenario!(
    worker_startup_composition_failed_rollback_healthy,
    "rollback-failed-healthy"
);

scenario!(
    worker_startup_composition_failed_rollback_grant_pressure,
    "rollback-failed-grant-pressure"
);

scenario!(
    worker_startup_composition_failed_rollback_grant_recorded,
    "rollback-failed-grant-recorded"
);

scenario!(
    worker_startup_composition_failed_rollback_recorded_cap,
    "rollback-failed-recorded-cap"
);

scenario!(
    worker_startup_composition_expired_preopen,
    "expired-startup-preopen"
);
scenario!(
    worker_startup_composition_expired_opened,
    "expired-startup-opened"
);
scenario!(
    worker_startup_composition_expired_activation,
    "expired-startup-activation"
);
scenario!(
    worker_startup_composition_expired_loaded,
    "expired-startup-loaded"
);
scenario!(
    worker_startup_composition_expired_classifier,
    "expired-startup-classifier"
);
scenario!(
    worker_startup_composition_expired_classifying,
    "expired-startup-classifying"
);
scenario!(
    worker_startup_composition_expired_classifying_complete,
    "expired-startup-classifying-complete"
);
scenario!(
    worker_startup_composition_expired_repair_read_complete,
    "expired-startup-repair-read-complete"
);
scenario!(
    worker_startup_composition_expired_repair_recheck_complete,
    "expired-startup-repair-recheck-complete"
);
scenario!(
    worker_startup_composition_expired_admission_before_fork_complete,
    "expired-startup-admission-before-fork-complete"
);
scenario!(
    worker_startup_composition_expired_admission_unsent_complete,
    "expired-startup-admission-unsent-complete"
);
scenario!(
    worker_startup_composition_expired_admission_unsent_failure_complete,
    "expired-startup-admission-unsent-failure-complete"
);
scenario!(
    worker_startup_composition_expired_admission_ready_complete,
    "expired-startup-admission-ready-complete"
);
scenario!(
    worker_startup_composition_expired_admission_refused_complete,
    "expired-startup-admission-refused-complete"
);
scenario!(
    worker_startup_composition_expired_admission_partial_complete,
    "expired-startup-admission-partial-complete"
);
scenario!(
    worker_startup_composition_expired_classifying_held,
    "expired-startup-classifying-held"
);
scenario!(
    worker_startup_composition_expired_classifying_cap,
    "expired-startup-classifying-cap"
);
scenario!(
    worker_startup_composition_expired_classifier_complete,
    "expired-startup-classifier-complete"
);
scenario!(
    worker_startup_composition_expired_classifier_varied,
    "expired-startup-classifier-varied"
);
scenario!(
    worker_startup_composition_expired_classifier_unreadable,
    "expired-startup-classifier-unreadable"
);
scenario!(
    worker_startup_composition_expired_classifier_held,
    "expired-startup-classifier-held"
);
scenario!(
    worker_startup_composition_expired_admit,
    "expired-startup-admit"
);
scenario!(
    worker_startup_composition_expired_admit_complete,
    "expired-startup-admit-complete"
);
scenario!(
    worker_startup_composition_expired_admit_sent_complete,
    "expired-startup-admit-sent-complete"
);
scenario!(
    worker_startup_composition_expired_admit_pending_complete,
    "expired-startup-admit-pending-complete"
);
scenario!(
    worker_startup_composition_expired_boot_complete,
    "expired-startup-boot-complete"
);
scenario!(
    worker_startup_composition_expired_release_wrong,
    "expired-startup-boot-release-wrong"
);
scenario!(
    worker_startup_composition_expired_release_arm,
    "expired-startup-boot-release-arm"
);
scenario!(
    worker_startup_composition_expired_release_position,
    "expired-startup-boot-release-position"
);
scenario!(
    worker_startup_composition_expired_release_early,
    "expired-startup-boot-release-early"
);
scenario!(
    worker_startup_composition_expired_release_silent,
    "expired-startup-boot-release-silent"
);
scenario!(
    worker_startup_composition_expired_release_held,
    "expired-startup-boot-release-held"
);
scenario!(
    worker_startup_composition_expired_release_recorded,
    "expired-startup-boot-release-recorded"
);
scenario!(
    worker_startup_composition_expired_repair_complete,
    "expired-startup-repair-complete"
);
scenario!(
    worker_startup_composition_expired_boot,
    "expired-startup-boot"
);
scenario!(
    worker_startup_composition_expired_open,
    "expired-startup-open"
);
scenario!(
    worker_startup_composition_expired_open_complete,
    "expired-startup-open-complete"
);
scenario!(
    worker_startup_composition_expired_open_unsent_complete,
    "expired-startup-open-unsent-complete"
);
scenario!(
    worker_startup_composition_expired_open_record_complete,
    "expired-startup-open-record-complete"
);
scenario!(
    worker_startup_composition_expired_open_release_wrong,
    "expired-startup-open-release-wrong"
);
scenario!(
    worker_startup_composition_expired_prompt_read,
    "expired-startup-prompt-complete"
);
scenario!(
    worker_startup_composition_expired_template,
    "expired-startup-template-complete"
);
scenario!(
    worker_startup_composition_expired_template_unsent,
    "expired-startup-template-unsent-complete"
);
scenario!(
    worker_startup_composition_expired_template_between,
    "expired-startup-template-between-complete"
);
scenario!(
    worker_startup_composition_expired_template_record,
    "expired-startup-template-record-complete"
);
scenario!(
    worker_startup_composition_expired_template_held,
    "expired-startup-template-held"
);
scenario!(
    worker_startup_composition_expired_template_release_wrong,
    "expired-startup-template-release-wrong"
);
scenario!(
    worker_startup_composition_expired_template_cap,
    "expired-startup-template-cap"
);
scenario!(
    worker_startup_composition_expired_template_component,
    "expired-startup-template-component-complete"
);
scenario!(
    worker_startup_composition_expired_template_foreign,
    "expired-startup-template-foreign-complete"
);
scenario!(
    worker_startup_composition_expired_template_completion,
    "expired-startup-template-completion-complete"
);
scenario!(
    worker_startup_composition_expired_template_recorded,
    "expired-startup-template-recorded-complete"
);
scenario!(
    worker_startup_composition_expired_summary_read,
    "expired-startup-summary-read-complete"
);
scenario!(
    worker_startup_composition_expired_prompt_cursor,
    "expired-startup-prompt-cursor-complete"
);
scenario!(
    worker_startup_composition_expired_summary_cursor,
    "expired-startup-summary-read-cursor-complete"
);
scenario!(
    worker_startup_composition_expired_read_classifier_held,
    "expired-startup-summary-read-held"
);
scenario!(
    worker_startup_composition_expired_read_release_wrong,
    "expired-startup-summary-read-release-wrong"
);
scenario!(
    worker_startup_composition_expired_prompt_measurement,
    "expired-startup-prompt-measurement-complete"
);
scenario!(
    worker_startup_composition_expired_summary_measurement,
    "expired-startup-summary-read-measurement-complete"
);
scenario!(
    worker_startup_composition_expired_repair,
    "expired-startup-repair"
);
scenario!(
    worker_startup_composition_expired_summary,
    "expired-startup-summary"
);
scenario!(
    worker_startup_composition_expired_installed,
    "expired-startup-installed"
);
scenario!(
    worker_startup_composition_expired_raised,
    "expired-startup-raised"
);
scenario!(
    worker_startup_composition_expired_installed_complete,
    "expired-startup-installed-complete"
);
scenario!(
    worker_startup_composition_expired_installed_unsent_complete,
    "expired-startup-installed-unsent-complete"
);
scenario!(
    worker_startup_composition_expired_raised_complete,
    "expired-startup-raised-complete"
);
scenario!(
    worker_startup_composition_expired_raised_lower_frames,
    "expired-startup-raised-lower-frames"
);
scenario!(
    worker_startup_composition_expired_raised_lower_pressure,
    "expired-startup-raised-lower-pressure"
);
scenario!(
    worker_startup_composition_expired_raised_lower_silent,
    "expired-startup-raised-lower-silent"
);
scenario!(
    worker_startup_composition_expired_raised_lower_held,
    "expired-startup-raised-lower-held"
);
scenario!(
    worker_startup_composition_expired_raised_lower_transport_loss,
    "expired-startup-raised-lower-transport-loss"
);
scenario!(
    worker_startup_composition_expired_raised_lower_last_word_delayed,
    "expired-startup-raised-lower-last-word-delayed"
);
scenario!(
    worker_startup_composition_expired_raised_lower_last_word_timeout,
    "expired-startup-raised-lower-last-word-timeout"
);
scenario!(
    worker_startup_composition_expired_raised_lower_last_word_lower_cap,
    "expired-startup-raised-lower-last-word-lower-cap"
);
scenario!(
    worker_startup_composition_expired_raised_lower_last_word_leave_cap,
    "expired-startup-raised-lower-last-word-leave-cap"
);
scenario!(
    worker_startup_composition_expired_raised_lower_last_word_failure,
    "expired-startup-raised-lower-last-word-failure"
);
scenario!(
    worker_startup_composition_expired_raised_lower_early,
    "expired-startup-raised-lower-early"
);
scenario!(
    worker_startup_composition_expired_raised_lower_wrong,
    "expired-startup-raised-lower-wrong"
);
scenario!(
    worker_startup_composition_expired_raised_lower_refused,
    "expired-startup-raised-lower-refused"
);
scenario!(
    worker_startup_composition_expired_raised_lower_unreplied,
    "expired-startup-raised-lower-unreplied"
);
scenario!(
    worker_startup_composition_expired_raised_lower_silent_admin,
    "expired-startup-raised-lower-silent-admin"
);
scenario!(
    worker_startup_composition_expired_raised_lower_cap,
    "expired-startup-raised-lower-cap"
);
scenario!(
    worker_startup_composition_expired_gate_failure_before_leave,
    "expired-startup-raised-lower-failure-before"
);
scenario!(
    worker_startup_composition_expired_gate_failure_pending_frame,
    "expired-startup-raised-lower-failure-lower-pressure"
);
scenario!(
    worker_startup_composition_expired_gate_failure_pending_ticket,
    "expired-startup-raised-lower-failure-pending-ticket-lower-pressure"
);
scenario!(
    worker_startup_composition_expired_gate_failure_stopped_child,
    "expired-startup-raised-lower-failure-lower-held"
);
scenario!(
    worker_startup_composition_expired_gate_failure_original_cap,
    "expired-startup-raised-lower-failure-lower-cap"
);
scenario!(
    worker_startup_composition_expired_gate_failure_release,
    "expired-startup-raised-lower-failure-release-wrong"
);
scenario!(
    worker_startup_composition_expired_gate_failure_recorded,
    "expired-startup-raised-lower-failure-recorded"
);
scenario!(
    worker_startup_composition_expired_foreign,
    "expired-startup-foreign"
);
scenario!(
    worker_startup_composition_expired_observe,
    "expired-startup-observe"
);
scenario!(
    worker_startup_composition_expired_silent_peer,
    "expired-startup-silent"
);
scenario!(
    worker_startup_composition_expired_refusal,
    "expired-startup-refusal"
);
scenario!(
    worker_startup_composition_expired_blocking_wait,
    "expired-startup-wait"
);
scenario!(
    worker_startup_composition_expired_activation_cap,
    "expired-startup-activation-cap"
);
scenario!(
    worker_startup_composition_expired_activation_trace,
    "expired-startup-activation-trace"
);

scenario!(
    worker_startup_composition_expired_repair_unstarted_complete,
    "expired-startup-repair-unstarted-complete"
);
scenario!(
    worker_startup_composition_expired_repair_retained_complete,
    "expired-startup-repair-retained-complete"
);
scenario!(
    worker_startup_composition_expired_repair_filed_complete,
    "expired-startup-repair-filed-complete"
);
scenario!(
    worker_startup_composition_expired_repair_proposed_complete,
    "expired-startup-repair-proposed-complete"
);
scenario!(
    worker_startup_composition_expired_repair_filing_unstarted_complete,
    "expired-startup-repair-filing-unstarted-complete"
);
scenario!(
    worker_startup_composition_expired_repair_filing_pending_complete,
    "expired-startup-repair-filing-pending-complete"
);
scenario!(
    worker_startup_composition_expired_repair_read_measurement_complete,
    "expired-startup-repair-read-measurement-complete"
);
scenario!(
    worker_startup_composition_expired_repair_read_cursor_complete,
    "expired-startup-repair-read-cursor-complete"
);
scenario!(
    worker_startup_composition_expired_repair_read_cap,
    "expired-startup-repair-read-cap"
);
scenario!(
    worker_startup_composition_expired_repair_read_held,
    "expired-startup-repair-read-held"
);
scenario!(
    worker_startup_composition_expired_repair_read_release_wrong,
    "expired-startup-repair-read-release-wrong"
);

scenario!(
    worker_startup_composition_expired_repair_filing_queued_complete,
    "expired-startup-repair-filing-queued-complete"
);

scenario!(
    worker_startup_composition_expired_repair_filing_unsent_complete,
    "expired-startup-repair-filing-unsent-complete"
);

scenario!(
    worker_startup_composition_expired_repair_filing_settled_complete,
    "expired-startup-repair-filing-settled-complete"
);

scenario!(
    worker_startup_composition_expired_repair_filing_pending_cap,
    "expired-startup-repair-filing-pending-cap"
);

scenario!(
    worker_startup_composition_expired_repair_filing_pending_classifier_held,
    "expired-startup-repair-filing-pending-classifier-held"
);

scenario!(
    worker_startup_composition_expired_repair_filing_pending_release_wrong,
    "expired-startup-repair-filing-pending-release-wrong"
);

scenario!(
    worker_startup_composition_expired_repair_filing_author_complete,
    "expired-startup-repair-filing-author-complete"
);

scenario!(
    worker_startup_composition_expired_repair_filing_flow_complete,
    "expired-startup-repair-filing-flow-complete"
);

scenario!(
    worker_startup_composition_expired_repair_filing_measurement_complete,
    "expired-startup-repair-filing-measurement-complete"
);

scenario!(
    worker_startup_composition_expired_repair_filing_pending_partial_complete,
    "expired-startup-repair-filing-pending-partial-complete"
);

scenario!(
    worker_startup_composition_expired_summary_account_unstarted_complete,
    "expired-startup-summary-account-unstarted-complete"
);

scenario!(
    worker_startup_composition_expired_summary_account_retained_complete,
    "expired-startup-summary-account-retained-complete"
);

scenario!(
    worker_startup_composition_expired_summary_account_populated_complete,
    "expired-startup-summary-account-populated-complete"
);

scenario!(
    worker_startup_composition_expired_summary_complete,
    "expired-startup-summary-complete"
);

scenario!(
    worker_startup_composition_expired_summary_account_refused_complete,
    "expired-startup-summary-account-refused-complete"
);

scenario!(
    worker_startup_composition_expired_summary_account_retained_cap,
    "expired-startup-summary-account-retained-cap"
);

scenario!(
    worker_startup_composition_expired_summary_account_retained_release_wrong,
    "expired-startup-summary-account-retained-release-wrong"
);

scenario!(
    worker_startup_composition_expired_summary_account_retained_classifier_held,
    "expired-startup-summary-account-retained-classifier-held"
);

scenario!(
    worker_startup_composition_expired_summary_boundary_recorded,
    "expired-startup-summary-boundary-recorded-populated-complete"
);

scenario!(
    worker_startup_composition_expired_summary_boundary_read_complete,
    "expired-startup-summary-boundary-read-complete-populated-complete"
);

scenario!(
    worker_startup_composition_expired_summary_boundary_read_settled,
    "expired-startup-summary-boundary-read-settled-populated-complete"
);

scenario!(
    worker_startup_composition_expired_summary_boundary_refused,
    "expired-startup-summary-boundary-refused-complete"
);

scenario!(
    worker_session_composition_forced_save,
    "input-worker-generation-emission-session-forced-save"
);
scenario!(
    worker_session_composition_forced_save_refused,
    "input-worker-generation-emission-session-forced-save-refused"
);
scenario!(
    worker_session_composition_forced_save_snapshot_timeout,
    "input-worker-generation-emission-session-forced-save-snapshot-timeout"
);
scenario!(
    worker_session_composition_graceful_save_snapshot_timeout,
    "input-worker-generation-emission-session-graceful-save-snapshot-timeout"
);
scenario!(
    worker_session_composition_forced_save_finished_timeout,
    "input-worker-generation-emission-session-forced-save-finished-timeout"
);
scenario!(
    worker_session_composition_forced_save_window_timeout,
    "input-worker-generation-emission-session-forced-save-window-timeout"
);
scenario!(
    worker_session_composition_forced_save_published_timeout,
    "input-worker-generation-emission-session-forced-save-published-timeout"
);
scenario!(
    worker_session_composition_forced_save_wrong_published,
    "input-worker-generation-emission-session-forced-save-wrong-published"
);
scenario!(
    worker_session_composition_forced_save_secondary,
    "input-worker-generation-emission-session-forced-save-secondary"
);
scenario!(
    worker_session_composition_forced_save_sqlite,
    "input-worker-generation-emission-session-forced-save-sqlite"
);
scenario!(
    worker_session_composition_forced_save_sqlite_held_filing,
    "input-worker-generation-emission-session-forced-save-sqlite-held-filing"
);

scenario!(
    worker_session_forced_leave_joint_sqlite_bad_release,
    "input-worker-generation-emission-session-forced-save-sqlite-joint-bad-release"
);

scenario!(
    worker_graceful_drain_running,
    "input-worker-generation-emission-pairs-external-graceful-running"
);
scenario!(
    worker_graceful_drain_crossed,
    "input-worker-generation-emission-pairs-external-graceful-crossed"
);
scenario!(
    worker_graceful_drain_new_call,
    "input-worker-generation-emission-pairs-external-graceful-new"
);
scenario!(
    worker_graceful_drain_completed_result,
    "input-worker-generation-emission-pairs-external-graceful-result"
);
scenario!(
    worker_graceful_drain_held_filing,
    "input-worker-generation-emission-pairs-external-graceful-held"
);
scenario!(
    worker_graceful_drain_expiry,
    "input-worker-generation-emission-pairs-external-graceful-expiry"
);
scenario!(
    worker_graceful_drain_expiry_during_handoff,
    "input-worker-generation-emission-pairs-external-graceful-expiry-delayed"
);

scenario!(
    worker_graceful_drain_session_idle,
    "input-worker-generation-emission-session-graceful-idle"
);
scenario!(
    worker_graceful_drain_session_active,
    "input-worker-generation-emission-session-graceful-active"
);

scenario!(
    worker_graceful_drain_join_before_expiry_delayed,
    "input-worker-generation-emission-pairs-external-graceful-expiry-early-join"
);

scenario!(
    worker_graceful_quiesce_accepted,
    "input-worker-generation-emission-session-graceful-quiesce-accepted"
);

scenario!(
    worker_graceful_quiesce_checking,
    "input-worker-generation-emission-session-graceful-quiesce-checking"
);

scenario!(
    worker_graceful_quiesce_queued,
    "input-worker-generation-emission-session-graceful-quiesce-queued"
);

scenario!(
    worker_graceful_quiesce_active,
    "input-worker-generation-emission-session-graceful-quiesce-active"
);

scenario!(
    worker_graceful_quiesce_expiry,
    "input-worker-generation-emission-session-graceful-quiesce-expiry"
);

scenario!(
    worker_graceful_quiesce_join,
    "input-worker-generation-emission-session-graceful-quiesce-join"
);

scenario!(
    worker_graceful_quiesce_wrong,
    "input-worker-generation-emission-session-graceful-quiesce-wrong"
);

scenario!(
    worker_graceful_quiesce_duplicate,
    "input-worker-generation-emission-session-graceful-quiesce-duplicate"
);

scenario!(
    worker_graceful_quiesce_empty,
    "input-worker-generation-emission-session-graceful-quiesce-empty"
);

scenario!(
    worker_graceful_quiesce_dead,
    "input-worker-generation-emission-session-graceful-quiesce-dead"
);

scenario!(
    worker_graceful_quiesce_bounded,
    "input-worker-generation-emission-session-graceful-quiesce-bounded"
);

scenario!(
    worker_graceful_quiesce_listener,
    "input-worker-generation-emission-session-graceful-quiesce-listener"
);

scenario!(
    worker_graceful_wind_down_clean,
    "input-worker-generation-emission-session-graceful-wind-down-clean"
);

scenario!(
    worker_graceful_wind_down_unbounded,
    "input-worker-generation-emission-session-graceful-wind-down-unbounded"
);

scenario!(
    worker_graceful_wind_down_multiple,
    "input-worker-generation-emission-session-graceful-wind-down-multiple"
);

scenario!(
    worker_graceful_wind_down_correction,
    "input-worker-generation-emission-session-graceful-wind-down-correction"
);

scenario!(
    worker_graceful_wind_down_exhausted,
    "input-worker-generation-emission-session-graceful-wind-down-exhausted"
);

scenario!(
    worker_graceful_wind_down_calls,
    "input-worker-generation-emission-session-graceful-wind-down-calls"
);

scenario!(
    worker_graceful_wind_down_template_absent,
    "input-worker-generation-emission-session-graceful-wind-down-template-absent"
);

scenario!(
    worker_graceful_wind_down_template_refused,
    "input-worker-generation-emission-session-graceful-wind-down-template-refused"
);

scenario!(
    worker_graceful_wind_down_placement_refused,
    "input-worker-generation-emission-session-graceful-wind-down-placement-refused"
);

scenario!(
    worker_graceful_wind_down_decode_refused,
    "input-worker-generation-emission-session-graceful-wind-down-decode-refused"
);

scenario!(
    worker_graceful_wind_down_correction_refused,
    "input-worker-generation-emission-session-graceful-wind-down-correction-refused"
);

scenario!(
    worker_graceful_wind_down_held_filing,
    "input-worker-generation-emission-session-graceful-wind-down-held-filing"
);

scenario!(
    worker_graceful_wind_down_expiry,
    "input-worker-generation-emission-session-graceful-wind-down-expiry"
);

scenario!(
    worker_graceful_wind_down_join,
    "input-worker-generation-emission-session-graceful-wind-down-join"
);

scenario!(
    worker_graceful_wind_down_component,
    "input-worker-generation-emission-session-graceful-wind-down-component"
);

scenario!(
    worker_graceful_wind_down_active,
    "input-worker-generation-emission-session-graceful-wind-down-active"
);

scenario!(
    worker_graceful_wind_down_join_delayed,
    "input-worker-generation-emission-session-graceful-wind-down-join-delayed"
);

scenario!(
    worker_graceful_wind_down_gate_dead,
    "input-worker-generation-emission-session-graceful-wind-down-gate-dead"
);

scenario!(
    worker_graceful_wind_down_author_wait,
    "input-worker-generation-emission-session-graceful-wind-down-author-wait"
);

scenario!(
    worker_session_graceful_save_sqlite,
    "input-worker-generation-emission-session-graceful-save-sqlite"
);
scenario!(
    worker_session_graceful_save_sqlite_join_before,
    "input-worker-generation-emission-session-graceful-save-sqlite-join-before"
);
scenario!(
    worker_session_graceful_save_sqlite_join_sealed,
    "input-worker-generation-emission-session-graceful-save-sqlite-join-sealed"
);
scenario!(
    worker_session_graceful_save_sqlite_joint,
    "input-worker-generation-emission-session-graceful-save-sqlite-joint"
);
scenario!(
    worker_session_graceful_save_sqlite_join_handoff,
    "input-worker-generation-emission-session-graceful-save-sqlite-join-handoff"
);
scenario!(
    worker_session_graceful_save_sqlite_wrong_published,
    "input-worker-generation-emission-session-graceful-save-sqlite-wrong-published"
);
scenario!(
    worker_session_graceful_save_sqlite_joint_bad_release,
    "input-worker-generation-emission-session-graceful-save-sqlite-joint-bad-release"
);
scenario!(
    worker_session_graceful_save_sqlite_peer_after_capture,
    "input-worker-generation-emission-session-graceful-save-sqlite-peer-after-capture"
);
scenario!(
    worker_session_graceful_save_sqlite_tail_expired,
    "input-worker-generation-emission-session-graceful-save-sqlite-tail-expired"
);

scenario!(
    worker_session_graceful_save_sqlite_drain_join_idle,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-join-idle"
);
scenario!(
    worker_session_graceful_save_sqlite_drain_join_accepted,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-join-accepted"
);
scenario!(
    worker_session_graceful_save_sqlite_drain_join_held_filing,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-join-held-filing"
);
scenario!(
    worker_session_graceful_save_sqlite_drain_expiry_held_filing,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-expiry-held-filing"
);
scenario!(
    worker_session_graceful_save_sqlite_drain_expiry_join_held_filing,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-expiry-join-held-filing"
);

scenario!(
    worker_session_graceful_save_sqlite_drain_join_generation,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-join-generation"
);
scenario!(
    worker_session_graceful_save_sqlite_drain_expiry_generation,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-expiry-generation"
);

scenario!(
    worker_session_graceful_save_sqlite_quiesce_prelower,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-prelower"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_afterlower,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-afterlower"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_expiry,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-expiry"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_pending_record,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-pending-record"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_pending_author,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-pending-author"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_accepted,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-accepted"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_delivered,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-delivered"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_wrong_ack,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-wrong-ack"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_duplicate_ack,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-duplicate-ack"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_queued_control,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-queued-control"
);
scenario!(
    worker_session_graceful_save_sqlite_quiesce_queued_close,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-queued-close"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_unstarted,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-unstarted"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_held_start,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-held-start"
);
scenario!(
    worker_session_graceful_save_sqlite_wind_convert_held_start_resume,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-held-start-resume"
);
scenario!(
    worker_session_graceful_save_sqlite_wind_convert_held_close,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-held-close"
);
scenario!(
    worker_session_graceful_save_sqlite_wind_convert_held_close_resume,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-held-close-resume"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_held_refusal,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-held-refusal"
);
scenario!(
    worker_session_graceful_save_sqlite_wind_convert_held_refusal_resume,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-held-refusal-resume"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_start_author,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-start-author"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_instruction,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-instruction"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_held_filing,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-held-filing"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_measure,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-measure"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_placed,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-placed"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_generation,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-generation"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_generated,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-generated"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_repair,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-repair"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_restatement,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-restatement"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_correction_place,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-correction-place"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_correction_generation,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-correction-generation"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_pair_filing,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-pair-filing"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_closing,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-closing"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_close_author,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-close-author"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_expiry_generation,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-expiry-generation"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_prior_generation,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-prior-generation"
);

scenario!(
    worker_session_graceful_save_sqlite_wind_convert_expiry_close_recorded,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-expiry-close-recorded"
);

scenario!(
    worker_session_graceful_secondary_drain_observe,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-expiry-secondary-observe-held-filing"
);

scenario!(
    worker_session_graceful_secondary_drain_joiner,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-expiry-secondary-joiner-held-filing"
);

scenario!(
    worker_session_graceful_secondary_drain_silent,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-expiry-secondary-silent-held-filing"
);

scenario!(
    worker_session_graceful_secondary_drain_refused,
    "input-worker-generation-emission-session-graceful-save-sqlite-drain-expiry-secondary-outoforder-held-filing"
);

scenario!(
    worker_session_graceful_secondary_wind_observe,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-expiry-secondary-observe"
);

scenario!(
    worker_session_graceful_secondary_wind_joiner,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-expiry-secondary-joiner"
);

scenario!(
    worker_session_graceful_secondary_wind_silent,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-expiry-secondary-silent"
);

scenario!(
    worker_session_graceful_secondary_wind_refused,
    "input-worker-generation-emission-session-graceful-save-sqlite-wind-convert-expiry-secondary-outoforder"
);

scenario!(
    worker_session_graceful_secondary_quiesce_observe,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-observe-expiry"
);

scenario!(
    worker_session_graceful_secondary_quiesce_joiner,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-joiner-expiry"
);

scenario!(
    worker_session_graceful_secondary_quiesce_silent,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-silent-expiry"
);

scenario!(
    worker_session_graceful_secondary_quiesce_refused,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-outoforder-expiry"
);

scenario!(
    worker_session_graceful_secondary_gate_observe,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-observe-gate-end"
);

scenario!(
    worker_session_graceful_secondary_gate_joiner,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-joiner-gate-end"
);

scenario!(
    worker_session_graceful_secondary_gate_silent,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-silent-gate-end"
);

scenario!(
    worker_session_graceful_secondary_gate_refused,
    "input-worker-generation-emission-session-graceful-save-sqlite-quiesce-secondary-outoforder-gate-end"
);
