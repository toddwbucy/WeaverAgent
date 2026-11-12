use super::*;

fn facts(tensor_split: Vec<f32>) -> ResidencyFacts {
    ResidencyFacts {
        build: Box::leak(Box::new(Build {
            ggml_version: "v\"1".into(),
            ggml_commit: "c".into(),
            libraries: vec![
                Library {
                    path: "/lib/libggml.so".into(),
                    sha256: "a".repeat(64),
                },
                Library {
                    path: "/lib/libllama.so".into(),
                    sha256: "b".repeat(64),
                },
            ],
        })),
        n_layer: 24,
        n_ctx_train: 32768,
        rope_type: None,
        file_type: "18".into(),
        n_gpu_layers: 2147483647,
        split_mode: "layer",
        main_gpu: 0,
        tensor_split,
        use_mmap: true,
        use_mlock: false,
        metadata: vec![
            Meta {
                key: "qwen2.context_length".into(),
                value: "32768".into(),
            },
            Meta {
                key: "qwen2.rope.freq_base".into(),
                value: "1000000.000000".into(),
            },
        ],
        mmap_supported: true,
        mlock_supported: false,
        headroom: 1 << 30,
        cuda_device_order: None,
        devices: vec![BackendDevice {
            name: "CUDA0".into(),
            description: "card".into(),
            memory_total: 1,
        }],
    }
}

fn set() -> ContextSet {
    ContextSet {
        n_ubatch: 512,
        type_k: KvCacheType::F16,
        type_v: KvCacheType::F16,
        flash_attn: llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_ENABLED,
        offload_kqv: true,
        op_offload: true,
        swa_full: true,
        kv_unified: false,
        n_threads: 4,
        n_threads_batch: 4,
    }
}

fn engine_text(facts: &ResidencyFacts, flush: FlushMechanism) -> String {
    let bytes = write_engine(
        facts,
        &set(),
        (4096, 4096, 512),
        flush,
        ("f16", "q8_0", "enabled"),
    )
    .expect("the block writes");
    String::from_utf8(bytes).expect("UTF-8")
}

/// **Both blocks are written closed, member by member, in section 3.1.2's
/// order**, every count a JSON number and every string escaped. The whole
/// text is compared, so a member out of order, an extra one or a missing one
/// fails here, and so does a member written under another name.
///
/// Perturbation: swap `n_batch` and `n_ubatch` in `write_engine`, or drop
/// `cuda_device_order` from `write_device`, and this fails.
#[test]
fn both_blocks_write_every_member_in_the_spec_order() {
    let facts = facts(vec![31.5, 33.5]);
    assert_eq!(
        engine_text(&facts, FlushMechanism::ReestablishAndReprefill),
        format!(
            concat!(
                r#"{{"backend":"gguf","mechanism":"gguf-sampler-2","flush":"reestablish-and-reprefill","#,
                r#""build":{{"ggml_version":"v\"1","ggml_commit":"c","libraries":["#,
                r#"{{"path":"/lib/libggml.so","sha256":"{a}"}},"#,
                r#"{{"path":"/lib/libllama.so","sha256":"{b}"}}]}},"#,
                r#""read_back":{{"n_ctx":4096,"n_batch":4096,"n_ubatch":512,"n_layer":24,"#,
                r#""n_ctx_train":32768,"rope_type":null,"file_type":"18"}},"#,
                r#""set":{{"n_ubatch":512,"type_k":"f16","type_v":"q8_0","flash_attn":"enabled","#,
                r#""offload_kqv":true,"op_offload":true,"swa_full":true,"kv_unified":false,"#,
                r#""n_gpu_layers":2147483647,"split_mode":"layer","main_gpu":0,"#,
                r#""tensor_split":[31.5,33.5]}},"#,
                r#""metadata":[{{"key":"qwen2.context_length","value":"32768"}},"#,
                r#"{{"key":"qwen2.rope.freq_base","value":"1000000.000000"}}]}}"#,
            ),
            a = "a".repeat(64),
            b = "b".repeat(64),
        )
    );
    let device =
        String::from_utf8(write_device(&facts, 4, 4).expect("the block writes")).expect("UTF-8");
    assert_eq!(
        device,
        concat!(
            r#"{"n_threads":4,"n_threads_batch":4,"use_mmap":true,"use_mlock":false,"#,
            r#""mmap_supported":true,"mlock_supported":false,"headroom":1073741824,"#,
            r#""cuda_device_order":null,"devices":[{"name":"CUDA0","description":"card","#,
            r#""memory_total":1}]}"#,
        )
    );
}

/// **Each share of the split is written as the `f32` it is**, per section
/// 3.1.2's `tensor_split: [f32]`: a share of `0.1` reads `0.1`. Widened
/// through `f64` or a generic value it would read `0.10000000149011612`, a
/// number the loader was never handed. A share on one device is the empty
/// list.
///
/// Perturbation: write `facts.tensor_split` mapped through `f64::from`, and
/// this fails.
#[test]
fn each_share_of_the_split_is_written_as_the_f32_it_is() {
    let text = engine_text(&facts(vec![0.1, 0.9]), FlushMechanism::TruncateToPosition);
    assert!(
        text.contains(r#""main_gpu":0,"tensor_split":[0.1,0.9]},"#),
        "{text}"
    );
    let text = engine_text(&facts(Vec::new()), FlushMechanism::TruncateToPosition);
    assert!(text.contains(r#""tensor_split":[]},"#), "{text}");
    assert!(
        text.contains(r#""flush":"truncate-to-position","#),
        "{text}"
    );
}

/// **A maps line names an engine library only by its file name**, and a
/// mapping replaced on disk refuses rather than digesting other bytes
/// (`mapped_libraries`).
///
/// Perturbation: accept a ` (deleted)` path, or match `llama` anywhere in
/// the path, and this fails.
#[test]
fn a_maps_line_names_an_engine_library_by_its_file_name() {
    let line =
        |path: &str| format!("7f00-7f10 r-xp 00000000 103:02 1234567                   {path}");
    assert_eq!(
        engine_library(&line("/opt/engine/lib/libllama.so.0.0.1")),
        Ok(Some("/opt/engine/lib/libllama.so.0.0.1"))
    );
    assert_eq!(
        engine_library(&line("/opt/engine/lib/libggml-cuda.so")),
        Ok(Some("/opt/engine/lib/libggml-cuda.so"))
    );
    assert_eq!(
        engine_library(&line("/opt/engine dir/lib/libggml-base.so")),
        Ok(Some("/opt/engine dir/lib/libggml-base.so"))
    );
    for other in [
        "/usr/lib/libc.so.6",
        "/opt/libllama/lib/libother.so",
        "[heap]",
        "libllama.so",
    ] {
        assert_eq!(engine_library(&line(other)), Ok(None), "{other}");
    }
    assert_eq!(
        engine_library("7f00-7f10 rw-p 00000000 00:00 0 "),
        Ok(None),
        "an anonymous mapping"
    );
    assert_eq!(engine_library("7f00-7f10 rw-p"), Ok(None), "a short line");
    let replaced = line("/opt/engine/lib/libllama.so (deleted)");
    let deleted = engine_library(&replaced);
    assert!(
        deleted
            .as_ref()
            .is_err_and(|detail| detail.contains("replaced on disk")),
        "{deleted:?}"
    );
}

/// **A device order that is not UTF-8 refuses**, rather than a lossy
/// rendering stating a value the process never had. Unset is `None`.
///
/// Perturbation: render it through `to_string_lossy`, and this fails.
#[test]
fn a_device_order_that_is_not_utf8_refuses() {
    use std::os::unix::ffi::OsStringExt;
    assert_eq!(device_order(None), Ok(None));
    assert_eq!(
        device_order(Some("PCI_BUS_ID".into())),
        Ok(Some("PCI_BUS_ID".into()))
    );
    let refused = device_order(Some(std::ffi::OsString::from_vec(vec![b'P', 0xff])));
    assert!(
        refused
            .as_ref()
            .is_err_and(|detail| detail.contains("not UTF-8")),
        "{refused:?}"
    );
}

/// **A library's digest is SHA-256 over the file's bytes, in lowercase
/// hex**, the spelling section 3.1.1's `build.libraries` row gives. Read
/// against the standard test vector for `abc`.
///
/// Perturbation: hash none of the file's bytes, or spell the digest in
/// uppercase, and this fails.
#[test]
fn a_library_digest_is_the_sha256_of_its_bytes() {
    use std::io::Write as _;
    let path = std::env::temp_dir().join(format!("theta-digest-{}", std::process::id()));
    // Made exclusively, so a path planted there, a link included, refuses
    // rather than being written through. A leftover from an earlier run that
    // panicked under the same pid is removed first, and removing a link
    // removes the link and not its target.
    let _ = std::fs::remove_file(&path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut file| file.write_all(b"abc"))
        .expect("the file is made afresh and writes");
    let digest = file_sha256(&path);
    std::fs::remove_file(&path).expect("the file is removed");
    assert_eq!(
        digest.as_deref(),
        Ok("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

unsafe extern "C" fn named_build() -> *const std::os::raw::c_char {
    c"b1234".as_ptr()
}

unsafe extern "C" fn mangled_build() -> *const std::os::raw::c_char {
    c"b\xff".as_ptr()
}

unsafe extern "C" fn no_build() -> *const std::os::raw::c_char {
    std::ptr::null()
}

/// **An engine build string that is not UTF-8 refuses**, as the device
/// order does, and a null answer refuses. A UTF-8 one is carried as read.
///
/// Perturbation: render it through `to_string_lossy`, and this fails.
#[test]
fn an_engine_string_that_is_not_utf8_refuses() {
    assert_eq!(
        engine_string(named_build, "ggml_version"),
        Ok("b1234".into())
    );
    let mangled = engine_string(mangled_build, "ggml_version");
    assert!(
        mangled
            .as_ref()
            .is_err_and(|detail| detail.contains("ggml_version is not UTF-8")),
        "{mangled:?}"
    );
    let absent = engine_string(no_build, "ggml_commit");
    assert_eq!(absent, Err("ggml_commit answered nothing".into()));
}
