use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files(PathBuf);

impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "weaver-lora-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn string(out: &mut Vec<u8>, text: &str) {
    out.extend((text.len() as u64).to_le_bytes());
    out.extend(text.as_bytes());
}

fn gguf(path: &Path, adapter: bool, tensors: &[(&str, [u64; 2])], extra: &[(&str, &str)]) {
    let mut metadata = vec![("general.architecture", "llama")];
    if adapter {
        metadata.extend([("general.type", "adapter"), ("adapter.type", "lora")]);
    }
    metadata.extend_from_slice(extra);
    let mut out = b"GGUF".to_vec();
    out.extend(3u32.to_le_bytes());
    out.extend((tensors.len() as u64).to_le_bytes());
    out.extend((metadata.len() as u64).to_le_bytes());
    for (key, value) in metadata {
        string(&mut out, key);
        out.extend(8u32.to_le_bytes());
        string(&mut out, value);
    }
    let mut bytes = 0u64;
    for (name, dims) in tensors {
        string(&mut out, name);
        out.extend(2u32.to_le_bytes());
        out.extend(dims[0].to_le_bytes());
        out.extend(dims[1].to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(bytes.to_le_bytes());
        bytes += dims.iter().product::<u64>() * 4;
        bytes = bytes.div_ceil(32) * 32;
    }
    out.resize(out.len().div_ceil(32) * 32 + bytes as usize, 0);
    std::fs::write(path, out).unwrap();
}

fn request(path: &Path) -> Request {
    Request {
        artifact: path.to_owned(),
        sha256: "a".repeat(64),
        expected_base_weights_hash: "b".repeat(64),
        scale: 1.0,
    }
}

fn artifacts(files: &Files) -> (PathBuf, PathBuf) {
    let base = files.path("base.gguf");
    let adapter = files.path("adapter.gguf");
    gguf(&base, false, &[("blk.0.attn_q.weight", [8, 16])], &[]);
    gguf(
        &adapter,
        true,
        &[
            ("blk.0.attn_q.weight.lora_a", [8, 2]),
            ("blk.0.attn_q.weight.lora_b", [2, 16]),
        ],
        &[],
    );
    (base, adapter)
}

#[test]
fn input_digests_and_scale_are_explicit() {
    let mut r = request(Path::new("adapter.gguf"));
    r.validate().unwrap();
    r.scale = 0.0;
    r.validate().unwrap();
    for scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        r.scale = scale;
        assert_eq!(r.validate().unwrap_err().stage, "adapter-input");
    }
    r.scale = 1.0;
    for digest in ["".into(), "A".repeat(64), "g".repeat(64), "a".repeat(63)] {
        r.sha256 = digest;
        assert!(r.validate().is_err());
    }
}

#[test]
fn attachment_failure_releases_context_before_control_returns() {
    use std::cell::RefCell;
    let events = RefCell::new(Vec::new());
    struct Context<'a>(&'a RefCell<Vec<&'static str>>);
    impl Drop for Context<'_> {
        fn drop(&mut self) {
            self.0.borrow_mut().push("context freed");
        }
    }
    let context = Context(&events);
    let failed = attach_before_use(context, Some(&()), |_, _| {
        events.borrow_mut().push("attach failed");
        Err("injected failure")
    });
    assert!(failed.is_err());
    events.borrow_mut().push("caller cleanup");
    assert_eq!(
        *events.borrow(),
        ["attach failed", "context freed", "caller cleanup"]
    );

    events.borrow_mut().clear();
    let context = attach_before_use(Context(&events), Some(&()), |_, _| {
        events.borrow_mut().push("attached");
        Ok::<_, &str>(())
    })
    .unwrap();
    events.borrow_mut().push("prefix decode");
    drop(context);
    assert_eq!(
        *events.borrow(),
        ["attached", "prefix decode", "context freed"]
    );

    let absent = attach_before_use((), None::<&()>, |_, _| -> Result<(), &str> {
        panic!("absence must not call the adapter backend")
    });
    assert!(absent.is_ok());
}

#[test]
fn normal_admission_does_not_validate_or_acquire_an_adapter() {
    use crate::readout::ReadoutElection;
    use crate::residency::{AdmitRefusal, Headroom, Residency};
    use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};
    let files = Files::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(files.path("absent-base").to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0)],
    };
    let mut normal = Residency::new();
    assert!(matches!(
        normal.admit(&binding, Headroom(0), ReadoutElection(false), false),
        Err(AdmitRefusal::Unresolvable {
            step: "resolve",
            ..
        })
    ));
    let mut supplied = Residency::new();
    let mut r = request(&files.path("absent-adapter"));
    r.scale = f32::NAN;
    assert!(matches!(
        supplied.admit_with_adapter(
            &binding,
            Headroom(0),
            ReadoutElection(false),
            false,
            Some(&r)
        ),
        Err(AdmitRefusal::Adapter(Failure {
            stage: "adapter-input",
            ..
        }))
    ));
}

#[test]
fn metadata_accounts_for_every_pair_and_its_storage() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    let identity = inspect(&adapter, &[base], "llama", &request(&adapter)).unwrap();
    assert_eq!(
        identity.targets,
        BTreeMap::from([("blk.0.attn_q.weight".into(), 2)])
    );
    assert_eq!(identity.tensor_bytes, (8 * 2 + 2 * 16) * 4);
}

#[test]
fn incomplete_unknown_incompatible_and_skipped_targets_refuse() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    for tensors in [
        vec![("blk.0.attn_q.weight.lora_a", [8, 2])],
        vec![
            ("unknown.weight.lora_a", [8, 2]),
            ("unknown.weight.lora_b", [2, 16]),
        ],
        vec![
            ("blk.0.attn_q.weight.lora_a", [7, 2]),
            ("blk.0.attn_q.weight.lora_b", [2, 16]),
        ],
        vec![
            ("blk.0.attn_q.weight.lora_a", [8, 2]),
            ("blk.0.attn_q.weight.lora_b", [3, 16]),
        ],
        vec![("blk.0.attn_norm.weight", [8, 1])],
        vec![
            ("blk.0.attn_q.weight.lora_a", [8, 2]),
            ("blk.0.attn_q.weight.lora_b", [2, 16]),
            ("blk.0.attn_norm.weight", [8, 1]),
        ],
    ] {
        gguf(&adapter, true, &tensors, &[]);
        assert!(
            inspect(
                &adapter,
                std::slice::from_ref(&base),
                "llama",
                &request(&adapter)
            )
            .is_err()
        );
    }
}

#[test]
fn wrong_architecture_alora_and_truncated_data_refuse() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    assert!(
        inspect(
            &adapter,
            std::slice::from_ref(&base),
            "qwen2",
            &request(&adapter)
        )
        .is_err()
    );
    gguf(
        &adapter,
        true,
        &[
            ("blk.0.attn_q.weight.lora_a", [8, 2]),
            ("blk.0.attn_q.weight.lora_b", [2, 16]),
        ],
        &[("adapter.alora.invocation_tokens", "1")],
    );
    assert!(
        inspect(
            &adapter,
            std::slice::from_ref(&base),
            "llama",
            &request(&adapter)
        )
        .is_err()
    );
    let (_, adapter) = artifacts(&files);
    let bytes = std::fs::read(&adapter).unwrap();
    std::fs::write(&adapter, &bytes[..bytes.len() - 1]).unwrap();
    assert!(inspect(&adapter, &[base], "llama", &request(&adapter)).is_err());
}

#[test]
fn malformed_gguf_metadata_refuses_before_backend_use() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    let tensors = [
        ("blk.0.attn_q.weight.lora_a", [8, 2]),
        ("blk.0.attn_q.weight.lora_b", [2, 16]),
    ];
    for extra in [
        vec![("general.type", "adapter")],
        vec![("general.alignment", "32")],
        vec![("adapter.lora.alpha", "1.0")],
        vec![("extra", "bad\0text")],
    ] {
        gguf(&adapter, true, &tensors, &extra);
        let error = inspect(
            &adapter,
            std::slice::from_ref(&base),
            "llama",
            &request(&adapter),
        )
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::Unreadable);
    }
    let (_, adapter) = artifacts(&files);
    let mut bytes = std::fs::read(&adapter).unwrap();
    bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
    std::fs::write(&adapter, bytes).unwrap();
    assert!(inspect(&adapter, &[base], "llama", &request(&adapter)).is_err());
}

#[test]
fn compatible_bytes_and_incompatible_pair_have_distinct_refusals() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    let error = inspect(&adapter, &[base], "qwen2", &request(&adapter)).unwrap_err();
    assert_eq!(error.kind, FailureKind::Incompatible);
    assert_eq!(
        Failure::new(
            &request(&adapter),
            error.kind,
            "adapter-metadata",
            error.detail
        )
        .lifecycle(),
        LifecycleRefusal::DeviceCannotAdmit
    );
}

#[test]
fn descriptor_hash_ignores_path_replacement_and_checks_both_identities() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    let mut r = request(&adapter);
    r.sha256 = format!("{:x}", Sha256::digest(std::fs::read(&adapter).unwrap()));
    let identity = inspect(&adapter, &[base], "llama", &r).unwrap();
    let mut prepared = Prepared {
        file: File::open(&adapter).unwrap(),
        request: r.clone(),
        identity,
    };
    std::fs::rename(&adapter, files.path("old.gguf")).unwrap();
    std::fs::write(&adapter, b"replacement").unwrap();
    prepared.verify(&r.expected_base_weights_hash).unwrap();
    assert_eq!(
        std::fs::read(prepared.path()).unwrap(),
        std::fs::read(files.path("old.gguf")).unwrap()
    );
    assert_eq!(
        prepared.verify(&"c".repeat(64)).unwrap_err().kind,
        FailureKind::Incompatible
    );
    prepared.request.sha256 = "c".repeat(64);
    assert_eq!(
        prepared
            .verify(&r.expected_base_weights_hash)
            .unwrap_err()
            .kind,
        FailureKind::Unreadable
    );
}

#[test]
fn agent_owned_artifact_refuses_before_metadata_is_read() {
    use std::os::unix::fs::PermissionsExt;
    let files = Files::new();
    let path = files.path("not-gguf");
    std::fs::write(&path, b"not a model").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let failure = prepare(&request(&path), &[], "llama").err().unwrap();
    assert_eq!(failure.stage, "adapter-custody");
    assert_eq!(failure.lifecycle(), LifecycleRefusal::ArtifactUnreadable);
    let missing = prepare(&request(&files.path("absent")), &[], "llama")
        .err()
        .unwrap();
    assert_eq!(missing.lifecycle(), LifecycleRefusal::ArtifactUnresolvable);
}

#[test]
fn non_agent_owned_read_only_file_passes_kernel_custody_check() {
    if nix::unistd::geteuid().is_root() {
        eprintln!("SKIP custody success: root cannot prove write denial");
        return;
    }
    // A platform file provides real ownership and kernel access facts.
    let file = File::open("/bin/sh").unwrap();
    judge_custody(&file).unwrap();
}

#[cfg(feature = "gguf")]
#[test]
fn callback_capture_survives_fixture_drop_in_an_isolated_process() {
    if std::env::var_os("WEAVER_LORA_CAPTURE_CHILD").is_some() {
        let fixture = Fixture::new().unwrap();
        let mut callback = None;
        let mut userdata = std::ptr::null_mut();
        unsafe { llama_cpp_sys_2::ggml_log_get(&mut callback, &mut userdata) };
        let callback = callback.expect("llama_log_set installs the shared GGML callback");
        unsafe {
            callback(
                llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG,
                c"placement".as_ptr(),
                userdata,
            )
        };
        drop(fixture);
        unsafe {
            callback(
                llama_cpp_sys_2::GGML_LOG_LEVEL_WARN,
                c"cleanup".as_ptr(),
                userdata,
            )
        };
        let records = observation::records().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].bytes, b"placement");
        assert_eq!(records[1].bytes, b"cleanup");
        assert!(Fixture::new().is_err(), "capture must never be replaced");
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "lora::tests::callback_capture_survives_fixture_drop_in_an_isolated_process",
            "--nocapture",
        ])
        .env("WEAVER_LORA_CAPTURE_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(feature = "gguf")]
#[test]
fn successful_load_still_fails_on_fallback_missing_or_ambiguous_placement() {
    use observation::{Record, judge};
    let targets = BTreeMap::from([("blk.0.attn_q.weight".into(), 2)]);
    let devices = [weaver_types::DeviceOrdinal(0)];
    let record = |text: &str| Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
        bytes: text.as_bytes().into(),
    };
    let good = record("llama_adapter_lora_init_impl: lora for 'blk.0.attn_q.weight' -> 'CUDA0'\n");
    assert_eq!(
        judge(std::slice::from_ref(&good), &targets, &devices)
            .unwrap()
            .len(),
        1
    );
    for records in [
        vec![],
        vec![good.clone(), good.clone()],
        vec![record(
            "llama_adapter_lora_init_impl: lora for 'blk.0.attn_q.weight' -> 'CPU'\n",
        )],
        vec![
            good,
            record(
                "llama_adapter_lora_init_impl: lora for 'blk.0.attn_q.weight' cannot use buft 'CPU_REPACK', fallback to CPU\n",
            ),
        ],
    ] {
        assert!(judge(&records, &targets, &devices).is_err());
    }
}
