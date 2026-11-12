//! **The `engine` and `device` blocks on GGUF**, per `weaver-spu-Spec`
//! sections 3.1.1 and 3.1.2: what the engine resolved, read back where the
//! wrapper exposes it, set where it does not, and rendered once where a
//! session opens, after its context is made (section 6).
//!
//! **Two moments, one rendering.** What the residency resolved (the engine's
//! build, the model's read-backs and metadata, the placement it was loaded
//! with, the device list) is gathered once at load and held by the
//! residency. What a context resolved (its sizes, and the values it was
//! handed) is read where the session opens. The two meet in [`render`],
//! which writes both blocks to bytes once, so every measurement of the
//! session splices the same bytes.
//!
//! **Each member is written in the order section 3.1.2's shapes give**, its
//! value by `serde_json`, which escapes each string and writes each number
//! typed. The `f32` split is written as the `f32` itself, so a share of
//! `0.1` reads `0.1`. The crate takes no `serde` dependency for the derive,
//! the way `placement.rs` writes the Open evidence.

use std::collections::BTreeSet;
use std::ffi::CStr;
use std::path::Path;
use std::sync::OnceLock;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};
use llama_cpp_2::model::{LlamaModel, RopeType};
use sha2::{Digest, Sha256};

use crate::decoder::backend::{Blocks, DecodeFault, FlushMechanism};

/// The sampler mechanism this build carries, per section 3.1.3: the draw
/// accepts nothing, and each decode accepts what it made resident.
pub(crate) const MECHANISM: &str = "gguf-sampler-2";

/// The engine's build: its version and commit, and a digest of each engine
/// library this process mapped. One process maps one set, so it is read once.
#[derive(Debug)]
pub(crate) struct Build {
    ggml_version: String,
    ggml_commit: String,
    libraries: Vec<Library>,
}

#[derive(Debug)]
struct Library {
    path: String,
    sha256: String,
}

#[derive(Debug)]
struct Meta {
    key: String,
    value: String,
}

#[derive(Debug)]
struct BackendDevice {
    name: String,
    description: String,
    memory_total: u64,
}

/// **What a residency's load resolved**, gathered once and held by the
/// residency (sections 3.1.1 and 3.1.2).
#[derive(Debug)]
pub(crate) struct ResidencyFacts {
    build: &'static Build,
    n_layer: u64,
    n_ctx_train: u64,
    rope_type: Option<&'static str>,
    file_type: String,
    n_gpu_layers: u32,
    split_mode: &'static str,
    main_gpu: u32,
    tensor_split: Vec<f32>,
    use_mmap: bool,
    use_mlock: bool,
    metadata: Vec<Meta>,
    mmap_supported: bool,
    mlock_supported: bool,
    headroom: u64,
    cuda_device_order: Option<String>,
    devices: Vec<BackendDevice>,
}

/// **What a context was handed**, read from the parameters the context was
/// made with rather than from the constants that built them, so the record
/// states what the engine received.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextSet {
    n_ubatch: u32,
    type_k: KvCacheType,
    type_v: KvCacheType,
    flash_attn: llama_cpp_sys_2::llama_flash_attn_type,
    offload_kqv: bool,
    op_offload: bool,
    swa_full: bool,
    kv_unified: bool,
    n_threads: i32,
    n_threads_batch: i32,
}

impl ContextSet {
    pub(crate) fn of(params: &LlamaContextParams) -> ContextSet {
        ContextSet {
            n_ubatch: params.n_ubatch(),
            type_k: params.type_k(),
            type_v: params.type_v(),
            flash_attn: params.flash_attention_policy(),
            offload_kqv: params.offload_kqv(),
            op_offload: params.op_offload(),
            swa_full: params.swa_full(),
            kv_unified: params.kv_unified(),
            n_threads: params.n_threads(),
            n_threads_batch: params.n_threads_batch(),
        }
    }
}

/// Gather what a load resolved. `tensor_split` is the split this crate
/// handed the loader, empty on one device (section 3.1.1's `set` row).
pub(crate) fn gather(
    model: &LlamaModel,
    params: &LlamaModelParams,
    tensor_split: Vec<f32>,
    headroom: u64,
) -> Result<ResidencyFacts, String> {
    let build = build()?;
    let arch = model
        .meta_val_str("general.architecture")
        .map_err(|error| format!("the model's general.architecture: {error}"))?;
    // **A model with no file type refuses rather than rendering a guess.**
    // The block's member is closed and required, and the converter writes
    // the key on every artifact it makes.
    let file_type = model
        .meta_val_str("general.file_type")
        .map_err(|error| format!("the model's general.file_type: {error}"))?;
    let rope = format!("{arch}.rope.");
    let context_length = format!("{arch}.context_length");
    let mut metadata = Vec::new();
    for index in 0..model.meta_count() {
        let key = model
            .meta_key_by_index(index)
            .map_err(|error| format!("metadata key {index}: {error}"))?;
        if key.starts_with(&rope) || key == context_length {
            let value = model
                .meta_val_str_by_index(index)
                .map_err(|error| format!("metadata value of {key}: {error}"))?;
            metadata.push(Meta { key, value });
        }
    }
    metadata.sort_by(|a, b| a.key.cmp(&b.key));
    let split_mode = match params.split_mode() {
        Ok(LlamaSplitMode::None) => "none",
        Ok(LlamaSplitMode::Layer) => "layer",
        other => return Err(format!("split mode {other:?} is not one the record names")),
    };
    let unsigned = |value: i32, what: &str| {
        u32::try_from(value).map_err(|_| format!("{what} {value} is negative"))
    };
    Ok(ResidencyFacts {
        build,
        n_layer: u64::from(model.n_layer()),
        n_ctx_train: u64::from(model.n_ctx_train()),
        rope_type: model.rope_type().map(|rope| match rope {
            RopeType::Norm => "norm",
            RopeType::NeoX => "neox",
            RopeType::MRope => "mrope",
            RopeType::Vision => "vision",
        }),
        file_type,
        n_gpu_layers: unsigned(params.n_gpu_layers(), "n_gpu_layers")?,
        split_mode,
        main_gpu: unsigned(params.main_gpu(), "main_gpu")?,
        tensor_split,
        use_mmap: params.use_mmap(),
        use_mlock: params.use_mlock(),
        metadata,
        mmap_supported: llama_cpp_2::mmap_supported(),
        mlock_supported: llama_cpp_2::mlock_supported(),
        headroom,
        cuda_device_order: device_order(std::env::var_os("CUDA_DEVICE_ORDER"))?,
        devices: llama_cpp_2::list_llama_ggml_backend_devices()
            .into_iter()
            .map(|device| BackendDevice {
                name: device.name,
                description: device.description,
                memory_total: device.memory_total as u64,
            })
            .collect(),
    })
}

/// **The process's `CUDA_DEVICE_ORDER` as the record states it**: `None`
/// where it is unset, and a refusal where it is not UTF-8, since a lossy
/// rendering would state a value the process never had.
fn device_order(raw: Option<std::ffi::OsString>) -> Result<Option<String>, String> {
    raw.map(|order| {
        order
            .into_string()
            .map_err(|_| "CUDA_DEVICE_ORDER is not UTF-8, so the record cannot state it".into())
    })
    .transpose()
}

/// Write one member: a comma unless it leads, the key, and the value as
/// `serde_json` writes that type.
macro_rules! member {
    ($out:expr, $lead:literal, $key:literal, $value:expr) => {{
        if !$lead {
            $out.push(b',');
        }
        $out.extend_from_slice(concat!("\"", $key, "\":").as_bytes());
        serde_json::to_writer(&mut *$out, &$value)?;
    }};
}

/// **Render both blocks once**, from the residency's facts, what the context
/// was handed, and what it resolved.
pub(crate) fn render(
    facts: &ResidencyFacts,
    set: &ContextSet,
    context: &LlamaContext<'_>,
    flush: FlushMechanism,
) -> Result<Blocks, DecodeFault> {
    let fault = |detail: String| DecodeFault::Engine { detail };
    let flash_attn = match set.flash_attn {
        llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_ENABLED => "enabled",
        llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_DISABLED => "disabled",
        other => {
            return Err(fault(format!(
                "flash attention {other} is not a value the record states, which is never auto"
            )));
        }
    };
    let unsigned = |value: i32, what: &str| {
        u32::try_from(value).map_err(|_| fault(format!("{what} {value} is negative")))
    };
    let type_k = kv_name(set.type_k).map_err(fault)?;
    let type_v = kv_name(set.type_v).map_err(fault)?;
    let n_threads = unsigned(set.n_threads, "n_threads")?;
    let n_threads_batch = unsigned(set.n_threads_batch, "n_threads_batch")?;
    let read_back = (
        u64::from(context.n_ctx()),
        u64::from(context.n_batch()),
        u64::from(context.n_ubatch()),
    );
    let engine = write_engine(facts, set, read_back, flush, (type_k, type_v, flash_attn));
    let device = write_device(facts, n_threads, n_threads_batch);
    let raw = |bytes: serde_json::Result<Vec<u8>>| {
        let bytes =
            bytes.map_err(|error| fault(format!("the theta blocks do not encode: {error}")))?;
        let text = String::from_utf8(bytes)
            .map_err(|error| fault(format!("the theta blocks are not UTF-8: {error}")))?;
        serde_json::value::RawValue::from_string(text)
            .map_err(|error| fault(format!("the theta blocks are not JSON: {error}")))
    };
    Ok(Blocks {
        engine: raw(engine)?,
        device: raw(device)?,
    })
}

fn write_engine(
    facts: &ResidencyFacts,
    set: &ContextSet,
    (n_ctx, n_batch, n_ubatch): (u64, u64, u64),
    flush: FlushMechanism,
    (type_k, type_v, flash_attn): (&str, &str, &str),
) -> serde_json::Result<Vec<u8>> {
    let out = &mut b"{".to_vec();
    member!(out, true, "backend", "gguf");
    member!(out, false, "mechanism", MECHANISM);
    member!(out, false, "flush", flush.name());
    out.extend_from_slice(b",\"build\":{");
    member!(out, true, "ggml_version", facts.build.ggml_version);
    member!(out, false, "ggml_commit", facts.build.ggml_commit);
    out.extend_from_slice(b",\"libraries\":[");
    for (index, library) in facts.build.libraries.iter().enumerate() {
        if index != 0 {
            out.push(b',');
        }
        out.push(b'{');
        member!(out, true, "path", library.path);
        member!(out, false, "sha256", library.sha256);
        out.push(b'}');
    }
    out.extend_from_slice(b"]},\"read_back\":{");
    member!(out, true, "n_ctx", n_ctx);
    member!(out, false, "n_batch", n_batch);
    member!(out, false, "n_ubatch", n_ubatch);
    member!(out, false, "n_layer", facts.n_layer);
    member!(out, false, "n_ctx_train", facts.n_ctx_train);
    member!(out, false, "rope_type", facts.rope_type);
    member!(out, false, "file_type", facts.file_type);
    out.extend_from_slice(b"},\"set\":{");
    member!(out, true, "n_ubatch", u64::from(set.n_ubatch));
    member!(out, false, "type_k", type_k);
    member!(out, false, "type_v", type_v);
    member!(out, false, "flash_attn", flash_attn);
    member!(out, false, "offload_kqv", set.offload_kqv);
    member!(out, false, "op_offload", set.op_offload);
    member!(out, false, "swa_full", set.swa_full);
    member!(out, false, "kv_unified", set.kv_unified);
    member!(out, false, "n_gpu_layers", facts.n_gpu_layers);
    member!(out, false, "split_mode", facts.split_mode);
    member!(out, false, "main_gpu", facts.main_gpu);
    // Each share is the `f32` the loader was handed, written as that `f32`.
    member!(out, false, "tensor_split", facts.tensor_split);
    out.extend_from_slice(b"},\"metadata\":[");
    for (index, meta) in facts.metadata.iter().enumerate() {
        if index != 0 {
            out.push(b',');
        }
        out.push(b'{');
        member!(out, true, "key", meta.key);
        member!(out, false, "value", meta.value);
        out.push(b'}');
    }
    out.extend_from_slice(b"]}");
    Ok(std::mem::take(out))
}

fn write_device(
    facts: &ResidencyFacts,
    n_threads: u32,
    n_threads_batch: u32,
) -> serde_json::Result<Vec<u8>> {
    let out = &mut b"{".to_vec();
    member!(out, true, "n_threads", n_threads);
    member!(out, false, "n_threads_batch", n_threads_batch);
    member!(out, false, "use_mmap", facts.use_mmap);
    member!(out, false, "use_mlock", facts.use_mlock);
    member!(out, false, "mmap_supported", facts.mmap_supported);
    member!(out, false, "mlock_supported", facts.mlock_supported);
    member!(out, false, "headroom", facts.headroom);
    member!(out, false, "cuda_device_order", facts.cuda_device_order);
    out.extend_from_slice(b",\"devices\":[");
    for (index, device) in facts.devices.iter().enumerate() {
        if index != 0 {
            out.push(b',');
        }
        out.push(b'{');
        member!(out, true, "name", device.name);
        member!(out, false, "description", device.description);
        member!(out, false, "memory_total", device.memory_total);
        out.push(b'}');
    }
    out.extend_from_slice(b"]}");
    Ok(std::mem::take(out))
}

/// The engine's own name for a KV element type (`ggml_type_name`), so the
/// record spells it as the engine does: `f16`, `q8_0`.
fn kv_name(kind: KvCacheType) -> Result<&'static str, String> {
    let raw: llama_cpp_sys_2::ggml_type = kind.into();
    // Safety: ggml returns a pointer into its static type table, or null for
    // a type it does not know.
    let name = unsafe { llama_cpp_sys_2::ggml_type_name(raw) };
    if name.is_null() {
        return Err(format!("the KV type {kind:?} has no engine name"));
    }
    unsafe { CStr::from_ptr(name) }
        .to_str()
        .map_err(|_| format!("the KV type {kind:?} has a name that is not UTF-8"))
}

static BUILD: OnceLock<Build> = OnceLock::new();

/// The engine's build, read once per process.
///
/// **Only a build that was read is kept.** A failed read refuses this load
/// and leaves the next load to read again, so a passing fault (a descriptor
/// limit, a read cut short) does not refuse every later load of the process.
fn build() -> Result<&'static Build, String> {
    if let Some(build) = BUILD.get() {
        return Ok(build);
    }
    let read = Build {
        ggml_version: engine_string(llama_cpp_sys_2::ggml_version, "ggml_version")?,
        ggml_commit: engine_string(llama_cpp_sys_2::ggml_commit, "ggml_commit")?,
        libraries: mapped_libraries()?,
    };
    Ok(BUILD.get_or_init(|| read))
}

fn engine_string(
    read: unsafe extern "C" fn() -> *const std::os::raw::c_char,
    what: &str,
) -> Result<String, String> {
    // Safety: each is a free function of the engine's library returning a
    // pointer to a static NUL-terminated string.
    let text = unsafe { read() };
    if text.is_null() {
        return Err(format!("{what} answered nothing"));
    }
    // Not UTF-8 refuses, as `device_order` does: a lossy rendering would
    // state a build the engine never named.
    unsafe { CStr::from_ptr(text) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| format!("{what} is not UTF-8, so the record cannot state it"))
}

/// **Each libllama and libggml file this process mapped**, read from its own
/// `/proc/self/maps`, in path order, with the digest of the file's bytes
/// (section 3.1.1's `build.libraries` row).
///
/// **A file replaced on disk since it was mapped refuses.** The kernel marks
/// such a mapping ` (deleted)`, and the path then names other bytes than the
/// ones this process runs, so a digest of it would describe an engine the
/// process never loaded.
///
/// **No mapped engine library refuses as well.** The crate links the engine
/// dynamically (`Cargo.toml`, the wrapper's `dynamic-link` feature), so a
/// process that runs the engine maps its libraries, and an empty list would
/// state a build with no digest.
fn mapped_libraries() -> Result<Vec<Library>, String> {
    let maps = std::fs::read_to_string("/proc/self/maps")
        .map_err(|error| format!("/proc/self/maps: {error}"))?;
    let mut paths = BTreeSet::new();
    for line in maps.lines() {
        if let Some(path) = engine_library(line)? {
            paths.insert(path.to_owned());
        }
    }
    if paths.is_empty() {
        return Err("this process mapped no libllama or libggml file".into());
    }
    paths
        .into_iter()
        .map(|path| {
            let sha256 = file_sha256(Path::new(&path))?;
            Ok(Library { path, sha256 })
        })
        .collect()
}

/// The engine library a maps line names, if it names one. A line is the
/// address range, permissions, offset, device and inode, then the path after
/// padding.
pub(super) fn engine_library(line: &str) -> Result<Option<&str>, String> {
    let Some(path) = line.splitn(6, ' ').nth(5).map(str::trim_start) else {
        return Ok(None);
    };
    let (path, deleted) = match path.strip_suffix(" (deleted)") {
        Some(path) => (path, true),
        None => (path, false),
    };
    let named = path.starts_with('/')
        && Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("libllama") || name.starts_with("libggml"));
    if !named {
        return Ok(None);
    }
    if deleted {
        return Err(format!(
            "the engine library {path} was replaced on disk after this process mapped it"
        ));
    }
    Ok(Some(path))
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let failed = |error: std::io::Error| format!("{}: {error}", path.display());
    let mut file = std::fs::File::open(path).map_err(failed)?;
    let mut hasher = Sha256::new();
    // `io::copy` retries a read the kernel interrupted.
    std::io::copy(&mut file, &mut hasher).map_err(failed)?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests;
