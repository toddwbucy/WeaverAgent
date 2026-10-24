//! Exact production entry loop in an isolated test-only backend process.
//! This cannot qualify production Idle: NoCompiledAdapter never admits.
use super::*;
use crate::ste100::admission::AdapterKind;
use crate::ste100::channel::{Receive, SendProgress, Tag};
use nix::libc;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use std::os::fd::AsRawFd;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use weaver_types::state_flow::{Ste100Ready, decode_strict};

use crate::ste100_test_fixture as fixture;
use crate::test_common as common;

struct IdleAdapter;
struct IdleBackend;
impl SealedAdapter for IdleAdapter {}
impl AdapterAdmission for IdleAdapter {
    type Backend = IdleBackend;
    fn kind(&self) -> AdapterKind {
        AdapterKind::TestOnly
    }
    fn admit(
        &self,
        _: &VerifiedArtifacts,
        _: &VerifiedFiles,
    ) -> Result<IdleBackend, Ste100AdmissionRefused> {
        Ok(IdleBackend)
    }
}
impl CheckBackend for IdleBackend {
    fn check(&mut self, _: &Ste100Request) -> Ste100Result {
        panic!("Idle exit instrument sends no check");
    }
}

const CHILD: &str = "WEAVER_STE100_ENTRY_REVIEW_IDLE_CHILD";

#[test]
fn ste100_entry_review_idle_child() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    // This exact test filter is the only fixture selector; it is absent from
    // the production executable. Adoption and the entry loop are production code.
    // SAFETY: the exact child was exec'd with one raw inherited seam and has
    // not created another descriptor owner or runtime worker.
    let fd =
        unsafe { crate::channel::adopt_ste100() }.expect("child inherits only its real duty seam");
    let channel = Channel::from_owned(fd, fixture::LIMIT).unwrap();
    let exit = drive(Runtime::new_fixture(channel, IdleAdapter));
    std::process::exit(if exit == ExitCode::SUCCESS { 0 } else { 1 });
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn ready_socket(channel: &Channel, events: i16) {
    let mut fd = libc::pollfd {
        fd: channel.readiness().as_raw_fd(),
        events,
        revents: 0,
    };
    assert!(
        unsafe { libc::poll(&mut fd, 1, 30_000) } > 0,
        "isolated entry child did not progress before watchdog"
    );
}

#[test]
fn ste100_entry_review_orderly_idle_close_returns_success_from_real_process() {
    let fixture = fixture::Fixture::new(true);
    let (peer, inherited) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let inherited_raw = inherited.as_raw_fd();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "ste100::entry::review_tests::ste100_entry_review_idle_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    common::place_inherited(&mut command, &[inherited_raw]);
    let mut child = ChildGuard(command.spawn().unwrap());
    drop(inherited);
    let mut peer = Channel::from_owned(peer, fixture::LIMIT).unwrap();
    peer.begin_send(
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    )
    .unwrap();
    while peer.has_pending_send() {
        if peer.try_send().unwrap() == SendProgress::Pending {
            ready_socket(&peer, libc::POLLOUT);
        }
    }
    let message = loop {
        match peer.try_receive().unwrap() {
            Receive::Complete(message) => break message,
            Receive::Fragment => {}
            Receive::Pending => ready_socket(&peer, libc::POLLIN),
        }
    };
    assert_eq!(
        message.tag,
        Tag::Ready,
        "the selected child actually reached admitted Idle"
    );
    assert_eq!(message.id, 0);
    let ready: Ste100Ready = decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
    assert_eq!(
        ready.artifact_sha256,
        fixture::sha(fixture.bootstrap.scheme_text.as_bytes())
    );
    drop(peer);
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "Idle close did not exit the actual child"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        status.success(),
        "orderly Idle close returned OS failure: {status}"
    );
}

#[test]
fn ste100_entry_review_static_absence_preserves_prepath_refusal_precedence() {
    use crate::ste100::admission::{admit_from_path, validate_bootstrap};
    let mut fixture = fixture::Fixture::new(false);
    fixture.bootstrap.instruction.model_binding.artifact.0 = fixture
        .root
        .join("missing-manifest")
        .to_str()
        .unwrap()
        .into();
    for defect in ["instruction", "scheme", "blob", "fixture", "none"] {
        let mut bootstrap = fixture.bootstrap.clone();
        match defect {
            "instruction" => bootstrap.instruction.instances = 0,
            "scheme" => bootstrap.scheme_text = "{}".into(),
            "blob" => {
                let mut scheme: serde_json::Value =
                    serde_json::from_str(&bootstrap.scheme_text).unwrap();
                scheme["grammar"]["sha256"] = serde_json::Value::String("0".repeat(64));
                bootstrap.scheme_text = serde_json::to_string(&scheme).unwrap();
            }
            "fixture" => {
                let mut scheme: serde_json::Value =
                    serde_json::from_str(&bootstrap.scheme_text).unwrap();
                scheme["reference"]["edition"] = "fixture-read/1".into();
                bootstrap.scheme_text = serde_json::to_string(&scheme).unwrap();
            }
            _ => {}
        }
        let result = validate_bootstrap(
            DecimalU64(fixture::LIMIT as u64),
            &serde_json::to_vec(&bootstrap).unwrap(),
        )
        .and_then(|checked| admit_from_path(checked, &NoCompiledAdapter));
        let refusal = match result {
            Err(refusal) => refusal,
            Ok(_) => panic!("absent adapter admitted"),
        };
        match defect {
            "instruction" => assert_eq!(refusal.code, Ste100AdmissionCode::InvalidInstruction),
            "scheme" | "blob" => assert_eq!(refusal.code, Ste100AdmissionCode::InvalidGrammar),
            "fixture" => {
                assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
                assert!(refusal.detail.contains("fixture-read/1"));
            }
            _ => {
                assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
                assert_eq!(
                    refusal.detail,
                    "no compatible STE100 adapter is compiled into this executable"
                );
            }
        }
    }
}

#[test]
fn ste100_entry_review_absent_adapter_keeps_one_refusal_for_supplied_and_path_routes() {
    use crate::ste100::admission::{SuppliedArtifacts, admit, admit_from_path, validate_bootstrap};
    let mut fixture = fixture::Fixture::new(false);
    fixture.bootstrap.instruction.model_binding.artifact.0 = fixture
        .root
        .join("missing-precheck-manifest")
        .to_str()
        .unwrap()
        .into();
    let bytes = serde_json::to_vec(&fixture.bootstrap).unwrap();
    let limit = DecimalU64(fixture::LIMIT as u64);
    let path = admit_from_path(
        validate_bootstrap(limit, &bytes).unwrap(),
        &NoCompiledAdapter,
    );
    let supplied = admit(
        limit,
        &bytes,
        SuppliedArtifacts {
            manifest: b"invalid manifest must remain untouched after static refusal",
            files: &[],
        },
        &NoCompiledAdapter,
    );
    let expected = NoCompiledAdapter.precheck().unwrap_err();
    for result in [path, supplied] {
        let refusal = match result {
            Err(refusal) => refusal,
            Ok(_) => panic!("the absent production adapter cannot produce Ready"),
        };
        assert_eq!(refusal.protocol, expected.protocol);
        assert_eq!(refusal.code, expected.code);
        assert_eq!(refusal.detail, expected.detail);
    }
}
