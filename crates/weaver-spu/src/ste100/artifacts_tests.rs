use super::*;
use std::cell::Cell;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Weak;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use weaver_types::state_flow::{
    ArtifactFile, EncoderArtifact, STE100_BINDER_ABI, STE100_LABELS, Ste100Bootstrap,
    Ste100ModelManifest,
};

use crate::ste100::admission::{
    AdapterAdmission, AdapterKind, SealedAdapter, VerifiedArtifacts, admit_acquired_fixture,
    admit_from_path, fixture_admitted, sha256,
};
use crate::ste100::channel::{Channel, Receive, SendProgress, Tag};
use crate::ste100::dispatcher::{Action, Dispatcher, Position};

static NEXT: AtomicU64 = AtomicU64::new(0);
const INPUTS: [(&str, &[u8]); 5] = [
    ("config.json", b"configuration checked once"),
    ("weights/a", b"first weight"),
    ("weights/b", b"second weight"),
    ("tokenizer.json", b"tokenizer checked once"),
    ("head.bin", b"seven labels"),
];

struct Fixture {
    root: PathBuf,
    bootstrap: Ste100Bootstrap,
    manifest: Ste100ModelManifest,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "weaver-ste100-artifacts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("weights")).unwrap();
        for (name, bytes) in INPUTS {
            std::fs::write(root.join(name), bytes).unwrap();
        }
        let reference = |index: usize| ArtifactFile {
            name: INPUTS[index].0.into(),
            sha256: sha256(INPUTS[index].1),
        };
        let manifest = Ste100ModelManifest {
            format: "ste100-model/1".into(),
            checker_abi: STE100_BINDER_ABI.into(),
            encoder: EncoderArtifact {
                family: "fixture-read/1".into(),
                config: reference(0),
                weights: vec![reference(1), reference(2)],
            },
            tokenizer: reference(3),
            span_head: reference(4),
            alignment: "fixture-read/1".into(),
            max_sentence_tokens: 128,
            labels: STE100_LABELS.into_iter().map(str::to_owned).collect(),
        };
        let (mut bootstrap, _) = fixture_admitted();
        bootstrap.instruction.model_binding.artifact.0 = root
            .join("arbitrary-manifest-name")
            .to_str()
            .unwrap()
            .into();
        let mut fixture = Self {
            root,
            bootstrap,
            manifest,
        };
        fixture.save();
        fixture
    }
    fn path(&self) -> &Path {
        Path::new(&self.bootstrap.instruction.model_binding.artifact.0)
    }
    fn save(&mut self) {
        let bytes = serde_json::to_vec(&self.manifest).unwrap();
        self.bootstrap.instruction.model_artifact_sha256 = sha256(&bytes);
        std::fs::write(self.path(), bytes).unwrap();
    }
    fn acquire(&self) -> Result<AcquiredArtifacts, Ste100AdmissionRefused> {
        AcquiredArtifacts::acquire(&self.bootstrap.instruction)
    }
    fn reference(&mut self, index: usize) -> &mut ArtifactFile {
        match index {
            0 => &mut self.manifest.encoder.config,
            1 | 2 => &mut self.manifest.encoder.weights[index - 1],
            3 => &mut self.manifest.tokenizer,
            4 => &mut self.manifest.span_head,
            _ => unreachable!(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn refused<T: std::fmt::Debug>(result: Result<T, Ste100AdmissionRefused>, reference: &str) {
    let refusal = result.unwrap_err();
    assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
    assert_eq!(refusal.protocol, STE100_PROTOCOL);
    assert!(
        refusal.detail.contains(reference),
        "{} does not identify {reference}",
        refusal.detail
    );
}

#[test]
fn exact_manifest_locator_rejects_relative_before_any_acquisition_or_adapter() {
    let mut fixture = Fixture::new();
    for relative in [
        "model.json",
        "./model.json",
        "../model.json",
        "",
        "/",
        "/models/",
        "/models/.",
        "/models/x/..",
    ] {
        fixture.bootstrap.instruction.model_binding.artifact.0 = relative.into();
        let mut touched = false;
        let result = acquire_with(&fixture.bootstrap.instruction, |_| touched = true);
        assert_eq!(
            result.unwrap_err().code,
            Ste100AdmissionCode::InvalidInstruction
        );
        assert!(!touched);
        let adapter = RejectingProduction {
            calls: Cell::new(0),
        };
        let result = admit_from_path(
            fixture.bootstrap.instruction.max_wire_bytes,
            &serde_json::to_vec(&fixture.bootstrap).unwrap(),
            &adapter,
        );
        assert_eq!(
            result.unwrap_err().code,
            Ste100AdmissionCode::InvalidInstruction
        );
        assert_eq!(adapter.calls.get(), 0);
    }
}

#[test]
fn exact_locator_has_no_directory_basename_or_trailing_component_fallback() {
    let mut fixture = Fixture::new();
    assert!(fixture.acquire().is_ok());
    let actual = fixture.path().to_owned();
    std::fs::copy(&actual, fixture.root.join("ste100-model.json")).unwrap();
    for locator in [
        fixture.root.to_str().unwrap().to_owned(),
        fixture
            .root
            .join("missing.json")
            .to_str()
            .unwrap()
            .to_owned(),
    ] {
        fixture.bootstrap.instruction.model_binding.artifact.0 = locator.clone();
        refused(fixture.acquire(), &locator);
    }
}

#[test]
fn parent_resolution_keeps_symlink_then_parent_semantics_and_the_pinned_instance() {
    let mut fixture = Fixture::new();
    let nested = fixture.root.join("actual/child");
    std::fs::create_dir_all(&nested).unwrap();
    let actual = fixture.root.join("actual");
    for (name, bytes) in INPUTS {
        std::fs::create_dir_all(actual.join(name).parent().unwrap()).unwrap();
        std::fs::write(actual.join(name), bytes).unwrap();
    }
    std::fs::copy(fixture.path(), actual.join("model")).unwrap();
    symlink("actual/child", fixture.root.join("link")).unwrap();
    fixture.bootstrap.instruction.model_binding.artifact.0 =
        fixture.root.join("link/../model").to_str().unwrap().into();
    assert!(fixture.acquire().is_ok());
    let pinned = fixture.root.join("held");
    let acquired = acquire_with(&fixture.bootstrap.instruction, |point| {
        if matches!(point, Acquired::Root) {
            std::fs::rename(&actual, &pinned).unwrap();
            std::fs::create_dir(&actual).unwrap();
            std::fs::write(actual.join("model"), b"replacement manifest").unwrap();
        }
    })
    .unwrap();
    assert_eq!(acquired.files().get("config.json"), Some(INPUTS[0].1));
}

#[test]
fn manifest_symlink_and_nonregular_reference_refuse_without_path_reinterpretation() {
    let fixture = Fixture::new();
    let original = fixture.path().to_owned();
    let held = fixture.root.join("original");
    std::fs::rename(&original, &held).unwrap();
    symlink(&held, &original).unwrap();
    refused(fixture.acquire(), original.to_str().unwrap());
    std::fs::remove_file(&original).unwrap();
    std::fs::rename(held, &original).unwrap();
    std::fs::remove_file(fixture.root.join("head.bin")).unwrap();
    std::fs::create_dir(fixture.root.join("head.bin")).unwrap();
    refused(fixture.acquire(), "head.bin");
}

#[test]
fn references_allow_only_links_whose_entire_resolution_stays_beneath_the_root() {
    let mut fixture = Fixture::new();
    std::fs::create_dir(fixture.root.join("nested")).unwrap();
    symlink("../config.json", fixture.root.join("nested/allowed")).unwrap();
    fixture.manifest.encoder.config.name = "nested/allowed".into();
    fixture.save();
    assert_eq!(
        fixture.acquire().unwrap().files().get("nested/allowed"),
        Some(INPUTS[0].1)
    );
    for (name, target) in [
        ("absolute", fixture.root.join("config.json")),
        (
            "reenter",
            PathBuf::from(format!(
                "../{}/config.json",
                fixture.root.file_name().unwrap().to_str().unwrap()
            )),
        ),
        ("magic", PathBuf::from("/proc/self/fd/0")),
        ("dangling", PathBuf::from("absent")),
        ("cycle", PathBuf::from("cycle")),
    ] {
        symlink(target, fixture.root.join(name)).unwrap();
        fixture.manifest.encoder.config.name = name.into();
        fixture.save();
        refused(fixture.acquire(), name);
    }
    // The root-prefix sibling is outside too, regardless of its equal prefix.
    let sibling = fixture.root.with_extension("lookalike");
    std::fs::create_dir(&sibling).unwrap();
    std::fs::write(sibling.join("file"), INPUTS[0].1).unwrap();
    symlink(&sibling, fixture.root.join("prefix")).unwrap();
    fixture.manifest.encoder.config.name = "prefix/file".into();
    fixture.save();
    refused(fixture.acquire(), "prefix/file");
    std::fs::remove_dir_all(sibling).unwrap();
}

#[test]
fn lexical_reference_defects_refuse_before_opening_any_reference() {
    for name in [
        "",
        "../head.bin",
        "/head.bin",
        "weights//a",
        "weights/./a",
        "weights/../a",
        "nul\0file",
    ] {
        let mut fixture = Fixture::new();
        fixture.manifest.span_head.name = name.into();
        fixture.save();
        let mut reference_opened = false;
        refused(
            acquire_with(&fixture.bootstrap.instruction, |point| {
                if let Acquired::Reference(_) = point {
                    reference_opened = true;
                }
            }),
            fixture.path().to_str().unwrap(),
        );
        assert!(!reference_opened);
    }
}

#[test]
fn exact_manifest_bytes_and_every_named_digest_are_verified_independently() {
    let mut fixture = Fixture::new();
    let mut bytes = std::fs::read(fixture.path()).unwrap();
    bytes.extend_from_slice(b" \n");
    std::fs::write(fixture.path(), &bytes).unwrap();
    refused(fixture.acquire(), fixture.path().to_str().unwrap());
    fixture.bootstrap.instruction.model_artifact_sha256 = sha256(&bytes);
    assert_eq!(
        fixture.acquire().unwrap().manifest_bytes(),
        bytes.as_slice()
    );
    for (index, (name, expected)) in INPUTS.into_iter().enumerate() {
        let mut fixture = Fixture::new();
        std::fs::write(fixture.root.join(name), b"wrong contents").unwrap();
        refused(fixture.acquire(), name);
        std::fs::remove_file(fixture.root.join(name)).unwrap();
        refused(fixture.acquire(), name);
        std::fs::write(fixture.root.join(name), expected).unwrap();
        assert!(fixture.acquire().is_ok());
        fixture.reference(index).sha256 = sha256(b"wrong declared digest");
        fixture.save();
        refused(fixture.acquire(), name);
    }
}

#[test]
fn malformed_manifest_bytes_refuse_with_the_exact_locator_before_reference_acquisition() {
    for bytes in [
        vec![0xff],
        b"not JSON".to_vec(),
        b"[]".to_vec(),
        br#"{"format":"ste100-model/1","format":"ste100-model/1"}"#.to_vec(),
    ] {
        let mut fixture = Fixture::new();
        std::fs::write(fixture.path(), &bytes).unwrap();
        fixture.bootstrap.instruction.model_artifact_sha256 = sha256(&bytes);
        let mut reference_opened = false;
        refused(
            acquire_with(&fixture.bootstrap.instruction, |point| {
                if let Acquired::Reference(_) = point {
                    reference_opened = true;
                }
            }),
            fixture.path().to_str().unwrap(),
        );
        assert!(!reference_opened);
    }
}

#[test]
fn repeated_reference_digests_and_unreferenced_sidecars_keep_the_closed_inventory() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.root.join("unreferenced.bin"), b"never an input").unwrap();
    fixture.manifest.tokenizer = fixture.manifest.encoder.config.clone();
    fixture.save();
    let mut config_acquisitions = 0;
    let acquired = acquire_with(&fixture.bootstrap.instruction, |point| {
        if let Acquired::Reference("config.json") = point {
            config_acquisitions += 1;
        }
    })
    .unwrap();
    assert_eq!(config_acquisitions, 2, "both declared digests are visited");
    assert_eq!(acquired.files.files.len(), 4);
    assert!(acquired.files().get("tokenizer.json").is_none());
    assert!(acquired.files().get("unreferenced.bin").is_none());
    assert!(Arc::ptr_eq(
        &acquired.files().retain("config.json").unwrap(),
        &acquired.files().retain("config.json").unwrap()
    ));
    fixture.manifest.tokenizer.sha256 = sha256(b"conflicting same-name digest");
    fixture.save();
    refused(fixture.acquire(), "config.json");
}

#[test]
fn replacement_after_each_acquisition_never_changes_checked_input_bytes() {
    let fixture = Fixture::new();
    let raw = std::fs::read(fixture.path()).unwrap();
    let acquired = acquire_with(&fixture.bootstrap.instruction, |point| match point {
        Acquired::Root => {}
        Acquired::Manifest => {
            std::fs::write(fixture.path(), b"replaced manifest").unwrap();
        }
        Acquired::Reference(name) => {
            let path = fixture.root.join(name);
            std::fs::rename(&path, path.with_extension("old")).unwrap();
            std::fs::write(path, b"replacement input").unwrap();
        }
    })
    .unwrap();
    assert_eq!(acquired.manifest_bytes(), raw);
    for (name, bytes) in INPUTS {
        assert_eq!(acquired.files().get(name), Some(bytes));
    }
}

#[derive(Debug)]
struct Deferred {
    inputs: Vec<Weak<Vec<u8>>>,
}
impl Deferred {
    fn read(&self) {
        for (input, (_, expected)) in self.inputs.iter().zip(INPUTS) {
            assert_eq!(
                input
                    .upgrade()
                    .expect("retained backend lost an admitted owner")
                    .as_slice(),
                expected
            );
        }
    }
}
struct DeferredAdapter;
impl SealedAdapter for DeferredAdapter {}
impl AdapterAdmission for DeferredAdapter {
    type Backend = Deferred;
    fn kind(&self) -> AdapterKind {
        AdapterKind::TestOnly
    }
    fn admit(
        &self,
        _: &VerifiedArtifacts,
        files: &VerifiedFiles,
    ) -> Result<Deferred, Ste100AdmissionRefused> {
        Ok(Deferred {
            inputs: INPUTS
                .iter()
                .map(|(name, _)| Arc::downgrade(&files.retain(name).unwrap()))
                .collect(),
        })
    }
}

#[test]
fn actual_ready_and_backend_transfer_keep_every_checked_owner_for_deferred_reads() {
    let fixture = Fixture::new();
    let raw = std::fs::read(fixture.path()).unwrap();
    let (peer, duty) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let limit = fixture.bootstrap.instruction.max_wire_bytes.0 as usize;
    let mut peer = Channel::from_owned(peer, limit).unwrap();
    let mut duty = Dispatcher::new(Channel::from_owned(duty, limit).unwrap());
    peer.begin_send(
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    )
    .unwrap();
    while peer.has_pending_send() {
        assert_ne!(peer.try_send().unwrap(), SendProgress::Pending);
    }
    assert_eq!(
        duty.advance().unwrap(),
        Action::Bootstrap(fixture.bootstrap.clone())
    );
    let admitted = {
        let acquired = fixture.acquire().unwrap();
        admit_acquired_fixture(fixture.bootstrap.clone(), acquired, &DeferredAdapter).unwrap()
    };
    duty.admit(&admitted).unwrap();
    assert_eq!(duty.advance().unwrap(), Action::Progress);
    assert_eq!(duty.position(), Position::Idle);
    let Receive::Complete(message) = peer.try_receive().unwrap() else {
        panic!("actual Ready expected");
    };
    assert_eq!(message.tag, Tag::Ready);
    assert_eq!(
        message.payload,
        serde_json::to_vec(admitted.ready()).unwrap()
    );
    for (name, _) in INPUTS {
        std::fs::write(fixture.root.join(name), b"after Ready replacement").unwrap();
    }
    std::fs::write(fixture.path(), b"after Ready manifest").unwrap();
    let retained = admitted.into_backend();
    drop(fixture);
    retained.backend().read();
    assert_eq!(retained.manifest_bytes(), raw);
    let witnesses = retained.backend().inputs.clone();
    drop(retained);
    assert!(witnesses.iter().all(|input| input.upgrade().is_none()));
}

struct RejectingProduction {
    calls: Cell<usize>,
}
impl SealedAdapter for RejectingProduction {}
impl AdapterAdmission for RejectingProduction {
    type Backend = ();
    fn kind(&self) -> AdapterKind {
        AdapterKind::Production
    }
    fn admit(
        &self,
        _: &VerifiedArtifacts,
        _: &VerifiedFiles,
    ) -> Result<(), Ste100AdmissionRefused> {
        self.calls.set(self.calls.get() + 1);
        Err(failure("compiled adapter", "unsupported test-only model"))
    }
}

#[test]
fn production_path_never_uses_fixture_adapter_or_unsupported_model_as_ready() {
    let fixture = Fixture::new();
    let bytes = serde_json::to_vec(&fixture.bootstrap).unwrap();
    refused(
        admit_from_path(
            fixture.bootstrap.instruction.max_wire_bytes,
            &bytes,
            &DeferredAdapter,
        ),
        "test adapter",
    );
    let rejecting = RejectingProduction {
        calls: Cell::new(0),
    };
    refused(
        admit_from_path(
            fixture.bootstrap.instruction.max_wire_bytes,
            &bytes,
            &rejecting,
        ),
        "fixture-read/1",
    );
    assert_eq!(rejecting.calls.get(), 0);
    let mut fixture = fixture;
    fixture.bootstrap.scheme_text = fixture
        .bootstrap
        .scheme_text
        .replace("fixture-read/1", "unsupported-production");
    fixture.manifest.encoder.family = "unsupported-production".into();
    fixture.manifest.alignment = "unsupported-production".into();
    fixture.save();
    let bytes = serde_json::to_vec(&fixture.bootstrap).unwrap();
    // A non-fixture manifest cannot make a test adapter a production adapter.
    refused(
        admit_from_path(
            fixture.bootstrap.instruction.max_wire_bytes,
            &bytes,
            &DeferredAdapter,
        ),
        "test adapter",
    );
    refused(
        admit_from_path(
            fixture.bootstrap.instruction.max_wire_bytes,
            &bytes,
            &rejecting,
        ),
        "compiled adapter",
    );
    assert_eq!(rejecting.calls.get(), 1);
}

#[test]
fn acquisition_failure_sends_only_the_exact_admission_refusal() {
    let fixture = Fixture::new();
    std::fs::remove_file(fixture.root.join("head.bin")).unwrap();
    let refusal = fixture.acquire().unwrap_err();
    let (peer, duty) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let limit = fixture.bootstrap.instruction.max_wire_bytes.0 as usize;
    let mut peer = Channel::from_owned(peer, limit).unwrap();
    let mut duty = Dispatcher::new(Channel::from_owned(duty, limit).unwrap());
    peer.begin_send(
        Tag::Bootstrap,
        0,
        serde_json::to_vec(&fixture.bootstrap).unwrap(),
    )
    .unwrap();
    while peer.has_pending_send() {
        assert_ne!(peer.try_send().unwrap(), SendProgress::Pending);
    }
    assert!(matches!(duty.advance().unwrap(), Action::Bootstrap(_)));
    duty.refuse(refusal.clone()).unwrap();
    duty.advance().unwrap();
    let Receive::Complete(message) = peer.try_receive().unwrap() else {
        panic!("refusal expected");
    };
    assert_eq!(message.tag, Tag::AdmissionRefused);
    assert_eq!(message.payload, serde_json::to_vec(&refusal).unwrap());
    assert_ne!(duty.position(), Position::Idle);
}

// Run access checks as the same uid/gid as the instrument, so ancestors and ACLs
// count as well as the final mode bits. An inaccessible Cargo target or TMPDIR is
// an unavailable environment, not evidence that the artifact permission guard ran.
fn unprivileged_environment_available(executable: &Path, temporary: &Path) -> bool {
    use std::os::unix::process::CommandExt;
    [
        ("-x", executable),
        ("-d", temporary),
        ("-w", temporary),
        ("-x", temporary),
    ]
    .into_iter()
    .all(|(mode, path)| {
        Command::new("/usr/bin/test")
            .args([std::ffi::OsStr::new(mode), path.as_os_str()])
            .uid(65534)
            .gid(65534)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

const CHILD: &str = "WEAVER_STE100_ARTIFACT_TEST_CHILD";
fn bounded_child(test: &str, case: &str, unprivileged: bool) {
    let executable = std::env::current_exe().unwrap();
    if unprivileged
        && nix::unistd::geteuid().is_root()
        && !unprivileged_environment_available(&executable, &std::env::temp_dir())
    {
        eprintln!(
            "SKIP unreadable-input instrument: uid 65534 cannot execute the test binary or use the fixture root; no permission-boundary result was obtained"
        );
        return;
    }
    let mut command = Command::new(executable);
    command
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, case)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if unprivileged && nix::unistd::geteuid().is_root() {
        use std::os::unix::process::CommandExt;
        command.uid(65534).gid(65534);
    }
    let mut child = command.spawn().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let output = child.wait_with_output().unwrap();
            assert!(
                status.success(),
                "child {case}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        if Instant::now() >= until {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("artifact acquisition blocked on {case}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn special_file_acquisition_never_waits_for_data_before_kind_checks() {
    if let Ok(case) = std::env::var(CHILD) {
        let mut fixture = Fixture::new();
        let path = if case == "manifest-fifo" {
            fixture.path().to_owned()
        } else {
            fixture.root.join("head.bin")
        };
        if case == "reference-fifo" {
            // Matching empty bytes isolate the kind guard from the hash guard.
            std::fs::write(&path, b"").unwrap();
            fixture.manifest.span_head.sha256 = sha256(b"");
            fixture.save();
            assert!(fixture.acquire().is_ok());
        }
        std::fs::remove_file(&path).unwrap();
        if case == "manifest-device" {
            fixture.bootstrap.instruction.model_binding.artifact.0 = "/dev/null".into();
        } else {
            nix::unistd::mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
        }
        refused(
            fixture.acquire(),
            if case == "manifest-fifo" {
                fixture.path().to_str().unwrap()
            } else if case == "manifest-device" {
                "/dev/null"
            } else {
                "head.bin"
            },
        );
        return;
    }
    for case in ["manifest-fifo", "reference-fifo", "manifest-device"] {
        bounded_child(
            "ste100::artifacts::tests::special_file_acquisition_never_waits_for_data_before_kind_checks",
            case,
            false,
        );
    }
}

#[test]
fn unreadable_input_is_judged_under_the_actual_unprivileged_identity() {
    if std::env::var_os(CHILD).is_some() || !nix::unistd::geteuid().is_root() {
        let fixture = Fixture::new();
        std::fs::set_permissions(
            fixture.root.join("head.bin"),
            std::fs::Permissions::from_mode(0o0),
        )
        .unwrap();
        refused(fixture.acquire(), "head.bin");
        return;
    }
    bounded_child(
        "ste100::artifacts::tests::unreadable_input_is_judged_under_the_actual_unprivileged_identity",
        "unreadable",
        true,
    );
}

#[test]
fn shared_file_bytes_keep_the_original_payload_allocation() {
    let bytes = (0..100_000)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let original = bytes.as_ptr();
    let retained = share_bytes(bytes);
    assert_eq!(
        retained.as_ptr(),
        original,
        "sharing copied the file payload"
    );
    let second = Arc::clone(&retained);
    drop(retained);
    assert_eq!(second.as_ptr(), original);
    assert_eq!(second.len(), 100_000);
}

#[test]
fn wrong_large_manifest_stream_never_enters_materialization() {
    struct LargeWrongFile {
        remaining: usize,
        max_read: usize,
        reads: usize,
        seeks: usize,
    }
    impl Read for LargeWrongFile {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.max_read = self.max_read.max(bytes.len());
            self.reads += 1;
            let length = bytes.len().min(self.remaining);
            bytes[..length].fill(b'x');
            self.remaining -= length;
            Ok(length)
        }
    }
    impl Seek for LargeWrongFile {
        fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
            self.seeks += 1;
            panic!("wrong digest reached retained-byte materialization")
        }
    }
    let fixture = Fixture::new();
    let mut wrong = LargeWrongFile {
        remaining: 64 * 1024 * 1024,
        max_read: 0,
        reads: 0,
        seeks: 0,
    };
    let refusal =
        read_matching_manifest(&mut wrong, u64::MAX, &fixture.bootstrap.instruction, || {
            panic!("wrong digest passed the streaming precheck")
        })
        .unwrap_err();
    assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
    assert!(refusal.detail.contains("manifest digest differs"));
    assert_eq!(wrong.remaining, 0);
    assert!(wrong.reads > 1);
    assert_eq!(wrong.max_read, READ_CHUNK);
    assert_eq!(wrong.seeks, 0);
}

#[test]
fn matching_manifest_reuses_the_descriptor_and_rechecks_materialized_bytes() {
    let fixture = Fixture::new();
    let raw = std::fs::read(fixture.path()).unwrap();
    let (mut file, len) = regular_file(File::open(fixture.path()).unwrap(), "manifest").unwrap();
    let held = fixture.root.join("held-manifest");
    let (bytes, manifest) =
        read_matching_manifest(&mut file, len, &fixture.bootstrap.instruction, || {
            std::fs::rename(fixture.path(), &held).unwrap();
            std::fs::write(fixture.path(), b"replacement pathname").unwrap();
        })
        .unwrap();
    assert_eq!(bytes.as_slice(), raw);
    assert_eq!(manifest, fixture.manifest);

    // In-place changes are different from pathname replacement. They must fail
    // the digest of the actual retained second-pass bytes before any parse/use.
    std::fs::write(fixture.path(), &raw).unwrap();
    let (mut file, len) = regular_file(File::open(fixture.path()).unwrap(), "manifest").unwrap();
    let mut changed = fixture.manifest.clone();
    changed.encoder.family = "different-but-valid-family".into();
    let changed = serde_json::to_vec(&changed).unwrap();
    let refusal = read_matching_manifest(&mut file, len, &fixture.bootstrap.instruction, || {
        std::fs::write(fixture.path(), &changed).unwrap();
    })
    .unwrap_err();
    assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
    assert!(refusal.detail.contains("digest differs"));
}

#[test]
fn regular_kind_check_clears_nonblocking_before_reading_data() {
    let fixture = Fixture::new();
    let descriptor = open(
        fixture.path(),
        OFlag::O_RDONLY | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .unwrap();
    assert!(
        OFlag::from_bits_truncate(fcntl(&descriptor, FcntlArg::F_GETFL).unwrap())
            .contains(OFlag::O_NONBLOCK)
    );
    let (mut file, len) = regular_file(File::from(descriptor), "manifest").unwrap();
    assert!(
        !OFlag::from_bits_truncate(fcntl(&file, FcntlArg::F_GETFL).unwrap())
            .contains(OFlag::O_NONBLOCK)
    );
    assert_eq!(
        read_owned(&mut file, len, "manifest").unwrap().as_slice(),
        std::fs::read(fixture.path()).unwrap()
    );
}

#[test]
fn confined_open_retries_only_eagain_and_stops_after_three_attempts() {
    let mut calls = 0;
    let result = retry_confined_open(|| {
        calls += 1;
        if calls < 3 {
            Err(Errno::EAGAIN)
        } else {
            Ok("same root success")
        }
    });
    assert_eq!(result, Ok("same root success"));
    assert_eq!(calls, 3);
    calls = 0;
    let result: Result<(), _> = retry_confined_open(|| {
        calls += 1;
        Err(Errno::EAGAIN)
    });
    assert_eq!(result, Err(Errno::EAGAIN));
    assert_eq!(calls, 3);
    for error in [
        Errno::EACCES,
        Errno::EXDEV,
        Errno::ELOOP,
        Errno::ENOENT,
        Errno::EINTR,
    ] {
        calls = 0;
        let result: Result<(), _> = retry_confined_open(|| {
            calls += 1;
            Err(error)
        });
        assert_eq!(result, Err(error));
        assert_eq!(calls, 1, "retried permanent/non-EAGAIN error {error}");
    }
}

#[test]
fn invalid_cheap_scheme_refuses_before_even_a_missing_manifest_is_opened() {
    let fixture = Fixture::new();
    let original: serde_json::Value = serde_json::from_str(&fixture.bootstrap.scheme_text).unwrap();
    let mut cases = vec!["not JSON".to_owned()];
    for field in ["grammar", "conditions"] {
        let mut value = original.clone();
        value[field]["bytes_base64"] = serde_json::json!("not base64@");
        cases.push(serde_json::to_string(&value).unwrap());
        let mut value = original.clone();
        value[field]["sha256"] = serde_json::json!("0".repeat(64));
        cases.push(serde_json::to_string(&value).unwrap());
    }
    // A valid control sees the missing file, proving these invalid cases are not
    // falling through to a later filesystem or adapter refusal.
    std::fs::remove_file(fixture.path()).unwrap();
    let adapter = RejectingProduction {
        calls: Cell::new(0),
    };
    let call = |bootstrap: &Ste100Bootstrap| {
        admit_from_path(
            bootstrap.instruction.max_wire_bytes,
            &serde_json::to_vec(bootstrap).unwrap(),
            &adapter,
        )
    };
    assert_eq!(
        call(&fixture.bootstrap).unwrap_err().code,
        Ste100AdmissionCode::UnsupportedArtifact
    );
    for scheme_text in cases {
        let mut bootstrap = fixture.bootstrap.clone();
        bootstrap.scheme_text = scheme_text;
        assert_eq!(
            call(&bootstrap).unwrap_err().code,
            Ste100AdmissionCode::InvalidGrammar
        );
    }
    assert_eq!(adapter.calls.get(), 0);
}

#[test]
fn manifest_compatibility_refuses_before_any_referenced_input_is_opened() {
    let fixture = Fixture::new();
    let mut bootstrap = fixture.bootstrap.clone();
    let mut scheme: serde_json::Value = serde_json::from_str(&bootstrap.scheme_text).unwrap();
    scheme["qualification"]["checker_abi"] = serde_json::json!("different-checker/1");
    bootstrap.scheme_text = serde_json::to_string(&scheme).unwrap();
    std::fs::remove_file(fixture.root.join("config.json")).unwrap();
    let adapter = RejectingProduction {
        calls: Cell::new(0),
    };
    let refusal = admit_from_path(
        bootstrap.instruction.max_wire_bytes,
        &serde_json::to_vec(&bootstrap).unwrap(),
        &adapter,
    )
    .unwrap_err();
    assert_eq!(refusal.code, Ste100AdmissionCode::SchemeMismatch);
    assert_eq!(refusal.detail, "scheme and model checker ABIs differ");
    assert_eq!(adapter.calls.get(), 0);
    // Fixture compatibility is also cheap and runs before the missing input.
    let refusal = admit_from_path(
        fixture.bootstrap.instruction.max_wire_bytes,
        &serde_json::to_vec(&fixture.bootstrap).unwrap(),
        &adapter,
    )
    .unwrap_err();
    assert!(refusal.detail.contains("fixture-read/1"));
}

#[test]
fn acquired_fixture_cannot_be_rebound_to_another_instruction_without_rehashing() {
    let fixture = Fixture::new();
    let acquired = fixture.acquire().unwrap();
    let mut substituted = fixture.bootstrap.clone();
    substituted.instruction.model_artifact_sha256 = sha256(b"another manifest");
    refused(
        admit_acquired_fixture(substituted, acquired, &DeferredAdapter),
        "different instruction",
    );
}

#[test]
fn unprivileged_permission_instrument_detects_inaccessible_ancestors_and_temp_roots() {
    if !nix::unistd::geteuid().is_root() {
        return;
    }
    let fixture = Fixture::new();
    std::fs::set_permissions(&fixture.root, std::fs::Permissions::from_mode(0o777)).unwrap();
    let executable = fixture.root.join("executable");
    std::fs::write(&executable, b"unused executable witness").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !unprivileged_environment_available(&executable, &fixture.root) {
        eprintln!(
            "SKIP environment preflight instrument: temporary ancestors are not traversable by uid 65534"
        );
        return;
    }
    let private = fixture.root.join("private");
    std::fs::create_dir(&private).unwrap();
    let private_executable = private.join("executable");
    std::fs::copy(&executable, &private_executable).unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!unprivileged_environment_available(
        &private_executable,
        &fixture.root
    ));
    assert!(!unprivileged_environment_available(&executable, &private));
    std::fs::set_permissions(&fixture.root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        !unprivileged_environment_available(&executable, &fixture.root),
        "read/search alone cannot create the test fixture"
    );
}

#[test]
fn acquired_scheme_rejects_all_object_array_aliases_before_adapter_admission() {
    let mut fixture = Fixture::new();
    fixture.bootstrap.scheme_text = fixture
        .bootstrap
        .scheme_text
        .replace("fixture-read/1", "unsupported-production");
    fixture.manifest.encoder.family = "unsupported-production".into();
    fixture.manifest.alignment = "unsupported-production".into();
    fixture.save();
    let valid: serde_json::Value = serde_json::from_str(&fixture.bootstrap.scheme_text).unwrap();
    let adapter = RejectingProduction {
        calls: Cell::new(0),
    };
    // A valid closed scheme reaches the real adapter boundary after acquiring
    // every declared manifest input. This double still refuses production Ready.
    refused(
        admit_from_path(
            fixture.bootstrap.instruction.max_wire_bytes,
            &serde_json::to_vec(&fixture.bootstrap).unwrap(),
            &adapter,
        ),
        "compiled adapter",
    );
    assert_eq!(adapter.calls.get(), 1);
    for (pointer, fields) in [
        (
            "",
            &[
                "format",
                "reference",
                "grammar",
                "verbs",
                "technical_names",
                "conditions",
                "recall_keys",
                "qualification",
            ][..],
        ),
        (
            "/reference",
            &[
                "edition",
                "document_sha256",
                "rule_ids",
                "dictionary_sha256",
            ][..],
        ),
        ("/grammar", &["format", "bytes_base64", "sha256"][..]),
        ("/conditions", &["format", "bytes_base64", "sha256"][..]),
        ("/verbs/0", &["key", "forms", "sense", "authority"][..]),
        ("/technical_names/0", &["text", "authority"][..]),
        ("/verbs/0/authority", &["rule_id", "disposition"][..]),
        (
            "/technical_names/0/authority",
            &["rule_id", "disposition"][..],
        ),
        (
            "/qualification",
            &["report_sha256", "corpus_sha256", "checker_abi"][..],
        ),
    ] {
        let mut malformed = valid.clone();
        let object = malformed.pointer_mut(pointer).unwrap();
        *object =
            serde_json::Value::Array(fields.iter().map(|field| object[*field].clone()).collect());
        fixture.bootstrap.scheme_text = serde_json::to_string(&malformed).unwrap();
        // The path entry computes the scheme hash from these exact changed
        // bytes, so an old caller-supplied digest cannot mask an open decoder.
        adapter.calls.set(0);
        let error = admit_from_path(
            fixture.bootstrap.instruction.max_wire_bytes,
            &serde_json::to_vec(&fixture.bootstrap).unwrap(),
            &adapter,
        )
        .unwrap_err();
        assert_eq!(error.code, Ste100AdmissionCode::InvalidGrammar, "{pointer}");
        assert!(
            error.detail.starts_with("invalid scheme:"),
            "{pointer}: {}",
            error.detail
        );
        assert_eq!(adapter.calls.get(), 0, "{pointer}");
    }

    for pointer in [
        "/verbs/0/authority/disposition",
        "/technical_names/0/authority/disposition",
    ] {
        for spelling in ["approved_word", "technical_use"] {
            let mut malformed = valid.clone();
            *malformed.pointer_mut(pointer).unwrap() = serde_json::json!({(spelling):null});
            fixture.bootstrap.scheme_text = serde_json::to_string(&malformed).unwrap();
            adapter.calls.set(0);
            let error = admit_from_path(
                fixture.bootstrap.instruction.max_wire_bytes,
                &serde_json::to_vec(&fixture.bootstrap).unwrap(),
                &adapter,
            )
            .unwrap_err();
            assert_eq!(
                error.code,
                Ste100AdmissionCode::InvalidGrammar,
                "{pointer}/{spelling}"
            );
            assert!(
                error.detail.starts_with("invalid scheme:"),
                "{pointer}: {}",
                error.detail
            );
            assert_eq!(adapter.calls.get(), 0, "{pointer}/{spelling}");
        }
    }
}
