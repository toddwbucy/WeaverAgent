use super::*;

/// **The type table is the engine's own.** Every type the pinned ggml
/// defines (a non-zero block size) is in the table with the engine's block
/// size and block bytes, and no type the engine left undefined is.
///
/// Perturbation: change any row's block bytes, or drop a row, and this test
/// fails.
#[cfg(feature = "gguf")]
#[test]
fn the_type_table_matches_the_engine() {
    for kind in 0..llama_cpp_sys_2::GGML_TYPE_COUNT {
        let block = unsafe { llama_cpp_sys_2::ggml_blck_size(kind) };
        let row = TYPE_SIZES.iter().find(|(k, ..)| *k == kind);
        if block == 0 {
            assert!(row.is_none(), "type {kind} is undefined in the engine");
            continue;
        }
        let size = unsafe { llama_cpp_sys_2::ggml_type_size(kind) };
        assert_eq!(
            row.map(|(_, b, s)| (*b, *s)),
            Some((block as u64, size as u64)),
            "type {kind}"
        );
    }
    assert!(
        TYPE_SIZES
            .iter()
            .all(|(kind, ..)| *kind < llama_cpp_sys_2::GGML_TYPE_COUNT)
    );
}

/// A tensor's bytes are whole blocks of its first dimension times the rest,
/// and a first dimension that is not whole blocks has no size.
#[test]
fn tensor_bytes_follow_blocks() {
    let tensor = |dims: [u64; 4], kind| Tensor {
        dims,
        kind,
        offset: 0,
    };
    assert_eq!(
        tensor([4096, 1024, 1, 1], 30).bytes(),
        Some(4096 * 1024 * 2)
    );
    assert_eq!(tensor([256, 3, 1, 1], 12).bytes(), Some(144 * 3));
    assert_eq!(tensor([100, 3, 1, 1], 12).bytes(), None);
    assert_eq!(tensor([1, 1, 1, 1], 4).bytes(), None);
}

fn write(tag: &str, bytes: &[u8]) -> (std::path::PathBuf, impl Drop) {
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "weaver-spu-directory-{tag}-{}.gguf",
        std::process::id()
    ));
    std::fs::write(&path, bytes).expect("a fixture");
    (path.clone(), Remove(path))
}

fn header(n_tensors: u64, n_values: u64) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&n_tensors.to_le_bytes());
    out.extend_from_slice(&n_values.to_le_bytes());
    out
}

fn key(out: &mut Vec<u8>, name: &str) {
    out.extend_from_slice(&(name.len() as u64).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
}

/// **A per-layer integer array is captured**, signed and unsigned, and a
/// longer one is skipped as `Other` without misaligning what follows.
#[test]
fn integer_arrays_are_captured_up_to_the_cap() {
    let mut out = header(0, 3);
    key(&mut out, "a.head_count_kv");
    out.extend_from_slice(&9u32.to_le_bytes());
    out.extend_from_slice(&5u32.to_le_bytes());
    out.extend_from_slice(&3u64.to_le_bytes());
    for n in [8i32, 0, -1] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    key(&mut out, "a.token_type");
    out.extend_from_slice(&9u32.to_le_bytes());
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(ARRAY_CAPTURE + 1).to_le_bytes());
    out.resize(out.len() + 4 * (ARRAY_CAPTURE as usize + 1), 0);
    key(&mut out, "a.after");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&7u32.to_le_bytes());
    let (path, _remove) = write("arrays", &out);
    let directory = read(&path).expect("the directory reads");
    assert!(matches!(
        directory.values.get("a.head_count_kv"),
        Some(Value::Integers(v)) if v == &[8, 0, -1]
    ));
    assert!(matches!(
        directory.values.get("a.token_type"),
        Some(Value::Other)
    ));
    assert!(matches!(
        directory.values.get("a.after"),
        Some(Value::Number(7, 4))
    ));
}

/// **A file with no tensor reads**, though its rounded data offset lies
/// past the end: there is no data section to be missing.
#[test]
fn a_directory_with_no_tensor_reads() {
    let mut out = header(0, 1);
    key(&mut out, "general.architecture");
    out.extend_from_slice(&8u32.to_le_bytes());
    key(&mut out, "qwen2");
    let (path, _remove) = write("empty", &out);
    let directory = read(&path).expect("the directory reads");
    assert!(directory.tensors.is_empty());
}
