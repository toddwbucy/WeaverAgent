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
    gguf_with_alpha(path, adapter, tensors, extra, None);
}

fn gguf_with_alpha(
    path: &Path,
    adapter: bool,
    tensors: &[(&str, [u64; 2])],
    extra: &[(&str, &str)],
    alpha: Option<f32>,
) {
    let mut metadata = vec![("general.architecture", "llama")];
    if adapter {
        metadata.extend([("general.type", "adapter"), ("adapter.type", "lora")]);
    }
    metadata.extend_from_slice(extra);
    let mut out = b"GGUF".to_vec();
    out.extend(3u32.to_le_bytes());
    out.extend((tensors.len() as u64).to_le_bytes());
    out.extend((metadata.len() as u64 + u64::from(alpha.is_some())).to_le_bytes());
    for (key, value) in metadata {
        string(&mut out, key);
        out.extend(8u32.to_le_bytes());
        string(&mut out, value);
    }
    if let Some(alpha) = alpha {
        string(&mut out, "adapter.lora.alpha");
        out.extend(6u32.to_le_bytes());
        out.extend(alpha.to_le_bytes());
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
        normal.admit(&binding, Headroom(0), 4096, ReadoutElection(false), false),
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
            4096,
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
fn stored_alpha_distinguishes_absence_from_zero_and_nonzero() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    let tensors = [
        ("blk.0.attn_q.weight.lora_a", [8, 2]),
        ("blk.0.attn_q.weight.lora_b", [2, 16]),
    ];
    for alpha in [None, Some(0.0), Some(16.0)] {
        gguf_with_alpha(&adapter, true, &tensors, &[], alpha);
        let identity = inspect(
            &adapter,
            std::slice::from_ref(&base),
            "llama",
            &request(&adapter),
        )
        .unwrap();
        assert_eq!(identity.alpha, alpha);
    }
    for alpha in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        gguf_with_alpha(&adapter, true, &tensors, &[], Some(alpha));
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
fn overlapping_tensor_ranges_refuse_even_when_each_tensor_fits() {
    let files = Files::new();
    let (base, adapter) = artifacts(&files);
    inspect(
        &adapter,
        std::slice::from_ref(&base),
        "llama",
        &request(&adapter),
    )
    .unwrap();
    let mut bytes = std::fs::read(&adapter).unwrap();
    let name = b"blk.0.attn_q.weight.lora_b";
    let name_start = bytes.windows(name.len()).position(|b| b == name).unwrap();
    // The offset follows the name, dimension count, two dimensions and type.
    let offset_start = name_start + name.len() + 4 + 2 * 8 + 4;
    bytes[offset_start..offset_start + 8].copy_from_slice(&0u64.to_le_bytes());
    std::fs::write(&adapter, bytes).unwrap();
    let error = inspect(&adapter, &[base], "llama", &request(&adapter)).unwrap_err();
    assert_eq!(error.kind, FailureKind::Unreadable);
    assert_eq!(error.detail, "empty or overlapping adapter tensor data");
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

#[test]
fn only_kernel_write_denials_establish_custody() {
    for errno in [nix::libc::EACCES, nix::libc::EROFS] {
        judge_write_denial(Some(errno)).unwrap();
    }
    for errno in [
        None,
        Some(nix::libc::ENOSYS),
        Some(nix::libc::EBADF),
        Some(nix::libc::EIO),
    ] {
        assert!(judge_write_denial(errno).is_err(), "accepted {errno:?}");
    }
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
    let division = crate::division::Division::even(1, 1).unwrap();
    let record = |text: &str| Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
        bytes: text.as_bytes().into(),
    };
    let good = record("llama_adapter_lora_init_impl: lora for 'blk.0.attn_q.weight' -> 'CUDA0'\n");
    let mut wrong_level = good.clone();
    wrong_level.level = llama_cpp_sys_2::GGML_LOG_LEVEL_WARN as i32;
    assert_eq!(
        judge(std::slice::from_ref(&good), &targets, &devices, &division)
            .unwrap()
            .len(),
        1
    );
    for records in [
        vec![],
        vec![wrong_level],
        vec![record(
            "llama_adapter_lora_init_impl: lora for 'blk.0.attn_v.weight' -> 'CUDA0'\n",
        )],
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
        assert!(judge(&records, &targets, &devices, &division).is_err());
    }
}

/// **Each target is matched to the device its layer is divided onto**, per
/// LORA-3 and PAIR-5: over a pair bound to ordinals 4 and 6, layer 0's
/// target on `CUDA0` and layer 1's on `CUDA1` pass and report the binding's
/// ordinals; layer 1's target on `CUDA0` is a layer on the wrong device and
/// refuses, as does a device outside the set.
///
/// Perturbation: accept any member device for a layer's target, or report
/// the position, and this test fails.
#[cfg(feature = "gguf")]
#[test]
fn each_target_is_matched_to_its_layer_device() {
    use observation::{Record, judge};
    let targets = BTreeMap::from([
        ("blk.0.attn_q.weight".into(), 2),
        ("blk.1.attn_q.weight".into(), 2),
    ]);
    let devices = [
        weaver_types::DeviceOrdinal(4),
        weaver_types::DeviceOrdinal(6),
    ];
    let division = crate::division::Division::even(2, 2).unwrap();
    let record = |target: &str, buffer: &str| Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
        bytes: format!("llama_adapter_lora_init_impl: lora for '{target}' -> '{buffer}'\n")
            .into_bytes(),
    };
    let placement = judge(
        &[
            record("blk.0.attn_q.weight", "CUDA0"),
            record("blk.1.attn_q.weight", "CUDA1"),
        ],
        &targets,
        &devices,
        &division,
    )
    .unwrap();
    assert_eq!(placement["blk.0.attn_q.weight"], devices[0]);
    assert_eq!(placement["blk.1.attn_q.weight"], devices[1]);
    for wrong in ["CUDA0", "CUDA2"] {
        assert!(
            judge(
                &[
                    record("blk.0.attn_q.weight", "CUDA0"),
                    record("blk.1.attn_q.weight", wrong),
                ],
                &targets,
                &devices,
                &division,
            )
            .is_err(),
            "layer 1 on {wrong}"
        );
    }
}

/// **The observer refuses an adapter placement line that is not UTF-8, and
/// only that**: a metadata value the loader cut at 37 bytes inside a
/// character is in the same capture and is no placement evidence.
///
/// Perturbation: skip an invalid record, or refuse every invalid record,
/// and this test fails.
#[cfg(feature = "gguf")]
#[test]
fn the_observer_refuses_only_an_invalid_placement_line() {
    use observation::{Record, judge};
    let targets = BTreeMap::from([("blk.0.attn_q.weight".into(), 2)]);
    let devices = [weaver_types::DeviceOrdinal(4)];
    let division = crate::division::Division::even(1, 1).unwrap();
    let placed = Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
        bytes: b"llama_adapter_lora_init_impl: lora for 'blk.0.attn_q.weight' -> 'CUDA0'\n"
            .to_vec(),
    };
    let cut = Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_INFO as i32,
        bytes: b"llama_model_loader: - kv  19: tokenizer.ggml.merges arr[str,3] = [\"\xc4\xa0 \xc4...\n"
            .to_vec(),
    };
    let placement = judge(
        &[cut.clone(), placed.clone()],
        &targets,
        &devices,
        &division,
    )
    .expect("a cut metadata value is no placement evidence");
    assert_eq!(placement["blk.0.attn_q.weight"], devices[0]);
    let mut invalid = placed.clone();
    invalid.bytes =
        b"llama_adapter_lora_init_impl: lora for 'blk.0.attn_q.weight' -> 'CUDA0'\xff\n".to_vec();
    let refused = judge(&[cut, invalid], &targets, &devices, &division)
        .expect_err("an invalid placement line refuses");
    assert!(refused.contains("non-UTF-8"), "{refused}");
}

/// The records of a pair whose division is right: four layers over two
/// devices, two each, the output on the second, every layer one attention
/// cache. The iSWA cache's "filtered" lines and a multi-line record ride
/// along, as the engine prints them.
#[cfg(feature = "gguf")]
fn division_records() -> Vec<observation::Record> {
    let debug = |text: String| observation::Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
        bytes: text.into_bytes(),
    };
    let mut records: Vec<_> = (0..=4)
        .map(|slot| {
            debug(format!(
                "load_tensors: layer {slot:>3} assigned to device CUDA{}, is_swa = 0\n",
                usize::from(slot >= 2)
            ))
        })
        .collect();
    records.push(debug(
        "llama_kv_cache: layer   0: dev = CUDA0\nllama_kv_cache: layer   1: dev = CUDA0\n".into(),
    ));
    for layer in 2..4 {
        records.push(debug(format!(
            "llama_kv_cache: layer {layer:>3}: dev = CUDA1\n"
        )));
        records.push(debug(format!(
            "llama_kv_cache: layer {layer:>3}: filtered\n"
        )));
    }
    records
}

/// The pair the division records are judged against: binding ordinals 4 and
/// 6, so a report of the position rather than the ordinal reads wrong.
#[cfg(feature = "gguf")]
const PAIR_AT: [u32; 2] = [4, 6];

#[cfg(feature = "gguf")]
fn pair_at() -> Vec<weaver_types::DeviceOrdinal> {
    PAIR_AT.map(weaver_types::DeviceOrdinal).to_vec()
}

#[cfg(feature = "gguf")]
fn dense(layers: usize) -> Vec<crate::room::Caches> {
    vec![
        crate::room::Caches {
            attention: true,
            recurrent: false,
        };
        layers
    ]
}

/// **The observed division matches the computed one**, per PAIR-5: one
/// assignment record per slot, the output's included, and one attention
/// cache record per layer, each on the device PAIR-3 gives it.
#[cfg(feature = "gguf")]
#[test]
fn the_observed_division_matches_the_computed_one() {
    let division = crate::division::Division::even(4, 2).unwrap();
    let observed =
        observation::judge_division(&division_records(), &pair_at(), &division, &dense(4)).unwrap();
    let [first, second] = PAIR_AT;
    assert_eq!(
        observed.slots,
        [first, first, second, second, second].map(weaver_types::DeviceOrdinal)
    );
    assert_eq!(
        observed.attention,
        [first, first, second, second].map(|o| Some(weaver_types::DeviceOrdinal(o)))
    );
    assert_eq!(observed.recurrent, [const { None }; 4]);
}

/// **Every deliberate change PAIR-5 names fails**: a missing record (the
/// output's, or a cache record that reads "does not have KV cache" or
/// "filtered"), a duplicated record, a layer on the wrong device, a record
/// the classification does not predict, a record at another level, a layer
/// past the depth, and a classification of another depth.
///
/// Perturbation: accept a missing or duplicated record, or a layer on the
/// wrong device, and this test fails.
#[cfg(feature = "gguf")]
#[test]
fn the_observed_division_refuses_a_missing_duplicated_or_misplaced_record() {
    use observation::{Record, judge_division};
    let division = crate::division::Division::even(4, 2).unwrap();
    let devices = pair_at();
    let debug = |text: &str| Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
        bytes: text.as_bytes().into(),
    };
    let without = |needle: &str| -> Vec<Record> {
        division_records()
            .into_iter()
            .filter(|r| !String::from_utf8_lossy(&r.bytes).contains(needle))
            .collect()
    };
    let with = |extra: Record| -> Vec<Record> {
        let mut records = division_records();
        records.push(extra);
        records
    };
    let mut warned = debug("load_tensors: layer   4 assigned to device CUDA1, is_swa = 0\n");
    warned.level = llama_cpp_sys_2::GGML_LOG_LEVEL_WARN as i32;
    let cases: Vec<(&str, Vec<Record>)> = vec![
        (
            "the output's assignment missing",
            without("layer   4 assigned"),
        ),
        (
            "a cache that has none",
            [
                without("layer   3: dev"),
                vec![debug("llama_kv_cache: layer   3: does not have KV cache\n")],
            ]
            .concat(),
        ),
        ("a cache filtered only", without("layer   3: dev")),
        (
            "an assignment twice",
            with(debug(
                "load_tensors: layer   1 assigned to device CUDA0, is_swa = 0\n",
            )),
        ),
        (
            "a cache twice",
            with(debug("llama_kv_cache: layer   2: dev = CUDA1\n")),
        ),
        (
            "a layer on the wrong device",
            [
                without("layer   2 assigned"),
                vec![debug(
                    "load_tensors: layer   2 assigned to device CUDA0, is_swa = 0\n",
                )],
            ]
            .concat(),
        ),
        (
            "a cache on the host",
            [
                without("layer   2: dev"),
                vec![debug("llama_kv_cache: layer   2: dev = CPU\n")],
            ]
            .concat(),
        ),
        (
            "a recurrent cache not predicted",
            with(debug("llama_memory_recurrent, layer   0: dev = CUDA0\n")),
        ),
        ("a record at another level", with(warned)),
        (
            "a layer past the depth",
            with(debug("llama_kv_cache: layer   4: dev = CUDA1\n")),
        ),
        (
            "a placement record that is not UTF-8",
            [
                without("layer   1 assigned"),
                vec![Record {
                    level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
                    bytes: b"load_tensors: layer   1 assigned to device CUDA0, is_swa = 0\xff\n"
                        .to_vec(),
                }],
            ]
            .concat(),
        ),
    ];
    for (case, records) in cases {
        assert!(
            judge_division(&records, &devices, &division, &dense(4)).is_err(),
            "{case} passed"
        );
    }
    assert!(judge_division(&division_records(), &devices, &division, &dense(3)).is_err());
    // A record that is not UTF-8 and holds no placement line, here a
    // metadata value the loader cut at 37 bytes inside a character, is no
    // placement fault.
    let cut = Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_INFO as i32,
        bytes: b"llama_model_loader: - kv  19: tokenizer.ggml.merges arr[str,3] = [\"\xc4\xa0 \xc4...\n"
            .to_vec(),
    };
    assert!(judge_division(&with(cut), &devices, &division, &dense(4)).is_ok());
}

/// **A recurrent cache is judged like an attention cache**: a layer the
/// classification gives a recurrent cache needs exactly one recurrent
/// record on its device, a "skipped" record is missing, and a layer with
/// both caches needs one record from each.
#[cfg(feature = "gguf")]
#[test]
fn the_observed_division_judges_recurrent_caches() {
    use observation::{Record, judge_division};
    let division = crate::division::Division::even(4, 2).unwrap();
    let devices = pair_at();
    let debug = |text: &str| Record {
        level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
        bytes: text.as_bytes().into(),
    };
    let mut caches = dense(4);
    caches[3].recurrent = true;
    let mut records = division_records();
    assert!(judge_division(&records, &devices, &division, &caches).is_err());
    records.push(debug("llama_memory_recurrent, layer   3: skipped\n"));
    assert!(judge_division(&records, &devices, &division, &caches).is_err());
    records.push(debug("llama_memory_recurrent, layer   3: dev = CUDA1\n"));
    let observed = judge_division(&records, &devices, &division, &caches).unwrap();
    let six = Some(weaver_types::DeviceOrdinal(6));
    assert_eq!(observed.recurrent, [None, None, None, six.clone()]);
    assert_eq!(observed.attention[3], six);
}
