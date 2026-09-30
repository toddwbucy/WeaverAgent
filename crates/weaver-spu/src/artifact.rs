//! conforms: spu-weights-hash-at-admit
//! conforms: spu-hash-failure-sentinel
//! conforms: spu-artifact-refusal-names-path-and-identity
//!
//! The artifact: resolution, the header read, and the weights hash.
//!
//! These are the free steps of the admit, per `weaver-spu-Spec` section 3.
//! Reading what an artifact declares about itself answers what family this is
//! and what its dimensions are **without touching tensor data or the device**,
//! which is the salvaged mechanic the survey names: it converts the common shape
//! of a bad binding, an artifact present and wrong, into a refusal costing no
//! device work.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use weaver_types::{ArtifactRef, LifecycleRefusal};

use crate::family::FamilyName;

/// What an artifact declares about itself, read from its header alone.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactHeader {
    /// Which container the bytes are in, which selects the backend.
    pub container: Container,
    /// The family this artifact declares, which keys the family registry.
    pub family: FamilyName,
    /// The hidden width, where the header declares one.
    pub hidden_size: Option<u64>,
    /// The layer count, where the header declares one.
    pub layer_count: Option<u64>,
    /// **The chat template the artifact's builder trained it on**, where the
    /// header declares one, per Spec section 5.
    ///
    /// Read but not parsed here. It selects among registry entries sharing an
    /// architecture, and the selection renders it rather than reading it,
    /// because a template is a program and a marker it emits need not appear
    /// in its source.
    pub chat_template: Option<String>,
}

/// The two containers this crate reads, which are peers rather than a default
/// and a fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// A GGUF file, served by the llama.cpp backend.
    Gguf,
    /// Safetensors, served by the candle-native backend.
    Safetensors,
}

/// Step one: resolve the binding to an artifact file.
///
/// Resolution is a path question and nothing more. Fetching one would make this
/// crate a provisioner of the operator's own artifact, per charter section 4.1,
/// which is why no HTTP client is in the dependency set. A directory holding
/// exactly one container file resolves to that file, which is the shape a
/// safetensors export takes, and a directory holding several is refused as
/// ambiguous. **What resolves is a regular file and nothing else**, so a
/// named pipe cannot turn a bad binding into an admission that blocks forever
/// on an open waiting for a writer.
pub fn resolve(reference: &ArtifactRef) -> Result<PathBuf, LifecycleRefusal> {
    let path = PathBuf::from(&reference.0);
    // **Absent and denied are told apart here**, per Spec section 3. A lookup
    // the kernel refuses is a present artifact this identity cannot reach,
    // which is the operator's likeliest fault and the one admin's root-side
    // look could never see, so it refuses unreadable rather than unresolvable,
    // the reading the pin gives an open that fails for a reason other than
    // absence. Nothing at the path, or a path through a non-directory, is the
    // artifact failing to resolve.
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Err(LifecycleRefusal::ArtifactUnresolvable);
        }
        Err(_) => return Err(LifecycleRefusal::ArtifactUnreadable),
    };
    // A directory resolves here, at step one, to the single container file it
    // holds. Resolving it later would leave the admission carrying the
    // directory while the header read blessed the file inside, and the loader
    // would then be handed a path it cannot open, refusing as a device
    // condition three steps after the fact that caused it.
    if metadata.is_dir() {
        return container_within(&path);
    }
    // Only a regular file resolves. A FIFO with a container's name would open
    // and then block until a writer connects, which turns a bad binding into
    // an admission that never returns, where every other malformation on this
    // path is a refusal costing no device work.
    if !metadata.is_file() {
        return Err(LifecycleRefusal::ArtifactUnresolvable);
    }
    Ok(path)
}

/// An artifact opened once and held open for the whole admit.
///
/// **The open is the check.** A path tested with `is_file` and opened later is
/// two different files if the name is replaced in between, so this type opens
/// first and asks the descriptor what it got. Everything after, the header
/// read, the load, and the hash, goes through this one descriptor, so the three
/// cannot observe three different files.
///
/// The open takes `O_NONBLOCK` so a named pipe cannot block it: a FIFO opens
/// immediately and is then refused by the kind check, where a blocking open
/// would hold the admit forever before any check could run.
pub struct PinnedArtifact {
    file: File,
    /// The split's later shards, pinned beside the first, empty for the
    /// single-file case. **The pin reaches the artifact's whole extent**, per
    /// the ruling on the split collision: judging and loading different
    /// bytes is the swap the descriptor discipline exists to prevent, and a
    /// split artifact's bytes are every shard's.
    siblings: Vec<File>,
}

impl PinnedArtifact {
    /// The path that reaches this descriptor's file whatever the original name
    /// now points at. The descriptor holds the inode, so a replacement of the
    /// name does not move it.
    pub fn path(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.file.as_raw_fd()))
    }

    /// The artifact's size, from the descriptors rather than from the names:
    /// the sum over every shard, which is what the room judgment must see.
    pub fn len(&self) -> Result<u64, LifecycleRefusal> {
        let mut total = self
            .file
            .metadata()
            .map(|m| m.len())
            .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
        for sibling in &self.siblings {
            total += sibling
                .metadata()
                .map(|m| m.len())
                .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
        }
        Ok(total)
    }

    /// Every shard's descriptor path, in split order, the first shard first.
    /// One entry for a single-file artifact.
    pub fn paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.path()];
        paths.extend(
            self.siblings
                .iter()
                .map(|f| PathBuf::from(format!("/proc/self/fd/{}", f.as_raw_fd()))),
        );
        paths
    }

    pub fn is_empty(&self) -> bool {
        self.len().map(|n| n == 0).unwrap_or(true)
    }

    fn rewind(&mut self) -> Result<(), LifecycleRefusal> {
        self.file
            .seek(SeekFrom::Start(0))
            .map(|_| ())
            .map_err(|_| LifecycleRefusal::ArtifactUnreadable)
    }
}

/// Open the resolved artifact and hold it, refusing anything that is not a
/// regular file once opened.
///
/// **A split artifact pins whole.** A name carrying llama.cpp's
/// `-NNNNN-of-NNNNN` pattern is one shard of a set, so every shard is pinned
/// here, in split order, with the first shard becoming the header's file
/// whichever shard the operator named. A shard that is missing refuses as
/// unresolvable: the artifact the binding names is the set, and a partial
/// set is not a smaller artifact but an absent one.
pub fn pin(path: &Path) -> Result<PinnedArtifact, LifecycleRefusal> {
    let Some((first, count, suffix)) = split_pattern(path) else {
        return Ok(PinnedArtifact {
            file: pin_one(path)?,
            siblings: Vec::new(),
        });
    };
    let file = pin_one(&shard_name(&first, 1, count, suffix))?;
    let mut siblings = Vec::with_capacity(count as usize - 1);
    for index in 2..=count {
        siblings.push(pin_one(&shard_name(&first, index, count, suffix))?);
    }
    Ok(PinnedArtifact { file, siblings })
}

fn pin_one(path: &Path) -> Result<File, LifecycleRefusal> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| {
            // A shard that is not there is the artifact failing to resolve,
            // which is what the pin's own doc promises. A shard that is
            // there and will not open is a different fact and keeps the
            // unreadable name.
            if error.kind() == std::io::ErrorKind::NotFound {
                LifecycleRefusal::ArtifactUnresolvable
            } else {
                LifecycleRefusal::ArtifactUnreadable
            }
        })?;
    let kind = file
        .metadata()
        .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
    if !kind.is_file() {
        return Err(LifecycleRefusal::ArtifactUnresolvable);
    }
    Ok(file)
}

/// The split pattern in a name: `<stem>-NNNNN-of-NNNNN.<container>`, both
/// containers spelling it the same way, llama.cpp's convention for GGUF and
/// the HuggingFace exporter's for safetensors. Answers the stem, the count,
/// and the suffix, or nothing for an unsplit name. Parsed by hand because
/// the shape is fixed and a pattern dependency for one fixed shape buys
/// nothing.
fn split_pattern(path: &Path) -> Option<(PathBuf, u32, &'static str)> {
    let name = path.file_name()?.to_str()?;
    let (rest, suffix) = if let Some(rest) = name.strip_suffix(".gguf") {
        (rest, ".gguf")
    } else if let Some(rest) = name.strip_suffix(".safetensors") {
        (rest, ".safetensors")
    } else {
        return None;
    };
    // <stem>-NNNNN-of-NNNNN
    if rest.len() < 15 {
        return None;
    }
    let (head, of_part) = rest.split_at(rest.len() - 9);
    let count_digits = of_part.strip_prefix("-of-")?;
    // **Every character of both fields is a digit, checked rather than left
    // to the integer parse**, which accepts a leading sign the pattern does
    // not.
    if !count_digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let count: u32 = count_digits.parse().ok()?;
    if head.len() < 6 {
        return None;
    }
    let (stem_dash, index_digits) = head.split_at(head.len() - 5);
    if !index_digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let stem = stem_dash.strip_suffix('-')?;
    if stem.is_empty() || count == 0 {
        return None;
    }
    // **The named index must cohere with the count.** An existing file named
    // index zero, or an index past the count, is a name outside the set it
    // claims membership of, and deriving the set from an incoherent name
    // would honor a reference the set does not contain.
    let index: u32 = index_digits.parse().ok()?;
    if index == 0 || index > count {
        return None;
    }
    Some((path.with_file_name(stem), count, suffix))
}

/// Whether a path names a shard of a split artifact.
///
/// Public so a caller preparing a binding, and the device tests exercising
/// the splits door, ask the same classifier the pin answers to rather than
/// growing a second parse of the same pattern.
pub fn names_a_split(path: &Path) -> bool {
    split_pattern(path).is_some()
}

/// One shard's name from the split's stem and suffix.
fn shard_name(stem: &Path, index: u32, count: u32, suffix: &str) -> PathBuf {
    let mut name = stem.as_os_str().to_os_string();
    name.push(format!("-{index:05}-of-{count:05}{suffix}"));
    PathBuf::from(name)
}

/// Step two: read what the artifact declares about itself.
///
/// **This function opens the artifact file and reads its header. It touches no
/// device and loads no tensor data.**
pub fn read_header(pinned: &mut PinnedArtifact) -> Result<ArtifactHeader, LifecycleRefusal> {
    pinned.rewind()?;
    let mut reader = BufReader::new(&mut pinned.file);

    let mut magic = [0u8; 4];
    reader
        .read_exact(&mut magic)
        .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;

    if &magic == b"GGUF" {
        read_gguf_header(&mut reader)
    } else {
        drop(reader);
        let sidecars = sidecar_dir_of(pinned);
        pinned.rewind()?;
        let mut reader = BufReader::new(&mut pinned.file);
        read_safetensors_header(&mut reader, sidecars.as_deref())
    }
}

/// The single container file inside a directory artifact.
fn container_within(dir: &Path) -> Result<PathBuf, LifecycleRefusal> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
    for entry in entries.flatten() {
        let path = entry.path();
        // Regular files only, on the same ground: a FIFO inside a directory
        // artifact would be selected by name and block the open.
        let is_container = path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "gguf" || e == "safetensors");
        if is_container {
            found.push(path);
        }
    }
    match found.len() {
        0 => Err(LifecycleRefusal::ArtifactUnresolvable),
        1 => Ok(found.remove(0)),
        // **Several containers resolve as one artifact exactly where they
        // are one split's shards.** Every file must carry the split pattern
        // with one shared stem and count, and the resolution answers the
        // first shard, the pin collecting the rest. Anything else is the
        // ambiguity this arm has always refused: two artifacts in one
        // directory is a binding that names neither.
        _ => {
            let mut parsed = Vec::with_capacity(found.len());
            for path in &found {
                match split_pattern(path) {
                    Some(parts) => parsed.push(parts),
                    None => return Err(LifecycleRefusal::ArtifactUnresolvable),
                }
            }
            let (stem, count, suffix) = parsed[0].clone();
            if !parsed
                .iter()
                .all(|(s, c, x)| *s == stem && *c == count && *x == suffix)
            {
                return Err(LifecycleRefusal::ArtifactUnresolvable);
            }
            if found.len() != count as usize {
                return Err(LifecycleRefusal::ArtifactUnresolvable);
            }
            Ok(shard_name(&stem, 1, count, suffix))
        }
    }
}

/// The GGUF header: magic already consumed, then version, tensor count, and the
/// key-value metadata block. Only the metadata is walked, never the tensor data.
fn read_gguf_header<R: Read>(reader: &mut R) -> Result<ArtifactHeader, LifecycleRefusal> {
    let _version = read_u32(reader)?;
    let _tensor_count = read_u64(reader)?;
    let kv_count = read_u64(reader)?;
    // The caps refuse rather than truncate. A walk cut short mid-structure
    // leaves the reader misaligned, so an oversized-but-valid header would
    // parse garbage instead of refusing, and a header claiming more metadata
    // than any real artifact carries is not an artifact this crate reads.
    if kv_count > 4096 {
        return Err(LifecycleRefusal::ArtifactUnreadable);
    }

    let mut family = None;
    let mut hidden_size = None;
    let mut layer_count = None;
    let mut chat_template = None;

    for _ in 0..kv_count {
        let key = read_gguf_string(reader)?;
        let value = read_gguf_value(reader)?;
        match (key.as_str(), &value) {
            ("general.architecture", GgufValue::Text(text)) => family = Some(text.clone()),
            (k, GgufValue::Number(n)) if k.ends_with(".embedding_length") => {
                hidden_size = Some(*n);
            }
            (k, GgufValue::Number(n)) if k.ends_with(".block_count") => layer_count = Some(*n),
            ("tokenizer.chat_template", GgufValue::Text(text)) => {
                chat_template = Some(text.clone());
            }
            _ => {}
        }
    }

    Ok(ArtifactHeader {
        container: Container::Gguf,
        family: FamilyName(family.ok_or(LifecycleRefusal::ArtifactUnreadable)?),
        hidden_size,
        layer_count,
        chat_template,
    })
}

/// The safetensors header: an eight-byte little-endian length, then that many
/// bytes of JSON. The tensor data beyond it is never read.
fn read_safetensors_header<R: Read>(
    reader: &mut R,
    sidecars: Option<&Path>,
) -> Result<ArtifactHeader, LifecycleRefusal> {
    let length = read_u64(reader)?;
    if length == 0 || length > 100 * 1024 * 1024 {
        return Err(LifecycleRefusal::ArtifactUnreadable);
    }
    let mut body = vec![0u8; length as usize];
    reader
        .read_exact(&mut body)
        .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
    let parsed: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;

    // The family travels in the `__metadata__` block where an export wrote
    // one, and in the sibling `config.json` where it did not. **The sidecar
    // is the ordinary case, not the fallback's fallback**: a stock exporter
    // writes `__metadata__` as `{"format": "pt"}` and nothing else, measured
    // 2026-08-17 against the Qwen2.5 artifact this workshop holds, so a
    // reader trusting the embedded block alone refuses every real artifact
    // of this container. A safetensors export is a directory-shaped
    // artifact and its facts live beside the weights, which is the same
    // rule the native loader reads its config and vocabulary by.
    let metadata = parsed.get("__metadata__");
    let sidecar = read_sidecar_json(sidecars, "config.json")?;
    let read = |key: &str| {
        metadata
            .and_then(|m| m.get(key))
            .or_else(|| sidecar.as_ref().and_then(|c| c.get(key)))
            .cloned()
    };
    let family = metadata
        .and_then(|m| m.get("architecture").or_else(|| m.get("model_type")))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            sidecar
                .as_ref()
                .and_then(|c| c.get("model_type"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .ok_or(LifecycleRefusal::ArtifactUnreadable)?;

    // The chat template lives in `tokenizer_config.json` for this container,
    // read for the same reason the GGUF read carries it: where an
    // architecture is contested, the template is the second half of the key.
    let chat_template = read_sidecar_json(sidecars, "tokenizer_config.json")?.and_then(|c| {
        c.get("chat_template")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });

    Ok(ArtifactHeader {
        container: Container::Safetensors,
        family: FamilyName(family),
        hidden_size: read("hidden_size").as_ref().and_then(json_number),
        layer_count: read("num_hidden_layers").as_ref().and_then(json_number),
        chat_template,
    })
}

/// One sidecar as JSON: absent is `None`, present-but-unreadable refuses.
///
/// **The two absences are not the same.** A directory without the file is an
/// export that never wrote one, and the read proceeds on what stands. A file
/// that is there and does not parse is an artifact declaring something this
/// crate cannot read, and proceeding as though it were absent would select
/// against facts a corrupt file was about to supply, which is the silent
/// degradation the refusal vocabulary exists to prevent.
fn read_sidecar_json(
    sidecars: Option<&Path>,
    name: &str,
) -> Result<Option<serde_json::Value>, LifecycleRefusal> {
    let Some(path) = sidecars.map(|dir| dir.join(name)) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| LifecycleRefusal::ArtifactUnreadable)
}

/// The directory the pinned artifact's sidecar files live in.
///
/// The pin's path is `/proc/self/fd/N` on purpose, the descriptor being the
/// identity, so the sidecar directory is recovered by asking the kernel what
/// it currently calls the pinned inode. A sidecar read is an open by name
/// and cannot ride the descriptor, which is a named limit of the pin rather
/// than something this function can repair: what the descriptor discipline
/// protects is the weights, and a swapped sidecar changes what the artifact
/// declares rather than what the device holds.
fn sidecar_dir_of(pinned: &PinnedArtifact) -> Option<PathBuf> {
    let real = std::fs::read_link(pinned.path()).ok()?;
    real.parent().map(Path::to_path_buf)
}

fn json_number(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

/// One GGUF metadata value, reduced to the two shapes this header read uses.
enum GgufValue {
    Text(String),
    Number(u64),
    Other,
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32, LifecycleRefusal> {
    let mut buffer = [0u8; 4];
    reader
        .read_exact(&mut buffer)
        .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
    Ok(u32::from_le_bytes(buffer))
}

fn read_u64<R: Read>(reader: &mut R) -> Result<u64, LifecycleRefusal> {
    let mut buffer = [0u8; 8];
    reader
        .read_exact(&mut buffer)
        .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
    Ok(u64::from_le_bytes(buffer))
}

fn read_gguf_string<R: Read>(reader: &mut R) -> Result<String, LifecycleRefusal> {
    let length = read_u64(reader)?;
    if length > 64 * 1024 {
        return Err(LifecycleRefusal::ArtifactUnreadable);
    }
    let mut body = vec![0u8; length as usize];
    reader
        .read_exact(&mut body)
        .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
    String::from_utf8(body).map_err(|_| LifecycleRefusal::ArtifactUnreadable)
}

/// How deep a nested GGUF array may go before the read refuses. GGUF metadata
/// has no legitimate deep case, and without a bound a crafted file of repeated
/// array-of-array headers drives the recursion to a stack overflow, an abort
/// reachable from an untrusted artifact at step two of the admit. Every other
/// malformation on this path is a clean refusal, and this one must be too.
const MAX_ARRAY_DEPTH: u32 = 8;

/// Read one typed GGUF value, skipping what this header read does not use. The
/// type codes are GGUF's own.
fn read_gguf_value<R: Read>(reader: &mut R) -> Result<GgufValue, LifecycleRefusal> {
    let kind = read_u32(reader)?;
    read_gguf_typed(reader, kind, 0)
}

fn read_gguf_typed<R: Read>(
    reader: &mut R,
    kind: u32,
    depth: u32,
) -> Result<GgufValue, LifecycleRefusal> {
    let mut skip = |n: usize| -> Result<(), LifecycleRefusal> {
        let mut sink = vec![0u8; n];
        reader
            .read_exact(&mut sink)
            .map_err(|_| LifecycleRefusal::ArtifactUnreadable)
    };
    match kind {
        0 | 1 => {
            let mut b = [0u8; 1];
            reader
                .read_exact(&mut b)
                .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
            Ok(GgufValue::Number(b[0] as u64))
        }
        2 | 3 => {
            let mut b = [0u8; 2];
            reader
                .read_exact(&mut b)
                .map_err(|_| LifecycleRefusal::ArtifactUnreadable)?;
            Ok(GgufValue::Number(u16::from_le_bytes(b) as u64))
        }
        4 | 5 => Ok(GgufValue::Number(read_u32(reader)? as u64)),
        6 => {
            skip(4)?;
            Ok(GgufValue::Other)
        }
        7 => {
            skip(1)?;
            Ok(GgufValue::Other)
        }
        8 => Ok(GgufValue::Text(read_gguf_string(reader)?)),
        9 => {
            // An array: element type, count, then the elements. Walked rather
            // than skipped wholesale, because element widths vary. Depth and
            // count refuse rather than truncate, on the same ground the
            // kv-count cap does: a walk cut short leaves the reader misaligned.
            if depth >= MAX_ARRAY_DEPTH {
                return Err(LifecycleRefusal::ArtifactUnreadable);
            }
            let element = read_u32(reader)?;
            let count = read_u64(reader)?;
            if count > 1024 * 1024 {
                return Err(LifecycleRefusal::ArtifactUnreadable);
            }
            for _ in 0..count {
                read_gguf_typed(reader, element, depth + 1)?;
            }
            Ok(GgufValue::Other)
        }
        10 | 11 => Ok(GgufValue::Number(read_u64(reader)?)),
        12 => {
            skip(8)?;
            Ok(GgufValue::Other)
        }
        _ => Err(LifecycleRefusal::ArtifactUnreadable),
    }
}

/// The weights hash: BLAKE3 over a canonical manifest, a single file or a
/// walked directory.
///
/// **The empty-string sentinel on every failure path is the property worth
/// carrying verbatim.** A hash that cannot be computed reports that it could
/// not rather than reporting a wrong value, and apex section 8 rests replay on
/// the identity being right. This function therefore returns
/// [`WeightsHash::sentinel`] rather than an error: a caller cannot accidentally
/// treat a failure as a value, because the failure is a value it can test.
///
/// The hash is computed at admit by reading the artifact, never from a manifest
/// handed to this process, and it is computed fresh on each call with no cache
/// across an artifact change, which is what makes the third walk's alteration
/// visible. **For a single-file artifact it reads the descriptor the load
/// used**, so a name replaced during the load cannot change what is hashed:
/// the descriptor holds the inode. What remains open is a directory-shaped
/// artifact, whose members beyond the loaded container are named rather than
/// pinned, and that closes with the native path that reads them.
pub fn weights_hash(
    reference: &Path,
    pinned: &mut PinnedArtifact,
) -> crate::residency::WeightsHash {
    // A reference naming one file hashes the descriptor the load used, which
    // is what closes the swap window on the single-file case. A reference
    // naming a directory hashes the walked directory, per the Spec's canonical
    // manifest: its members beyond the container are not pinned, so that case
    // keeps the window and the native path is where it closes.
    let hashed = if reference.is_dir() {
        hash_canonical(reference)
    } else {
        pinned.rewind().map_err(|_| ()).and_then(|()| {
            // Every shard feeds the hash in split order: the identity is the
            // set's, and hashing the first shard alone would let a swapped
            // sibling ride under an unchanged identity.
            let mut hasher = blake3::Hasher::new();
            hash_reader_into(&mut pinned.file, &mut hasher)?;
            for sibling in &mut pinned.siblings {
                sibling.seek(SeekFrom::Start(0)).map_err(|_| ())?;
                hash_reader_into(sibling, &mut hasher)?;
            }
            Ok(hasher.finalize().to_hex().to_string())
        })
    };
    match hashed {
        Ok(value) => crate::residency::WeightsHash(value),
        Err(()) => crate::residency::WeightsHash::sentinel(),
    }
}

fn hash_canonical(path: &Path) -> Result<String, ()> {
    let mut hasher = blake3::Hasher::new();
    if path.is_dir() {
        // A walked directory, in sorted order, so the manifest is canonical
        // rather than dependent on readdir order. Each file contributes its
        // relative path and then its bytes, so a rename is a different hash.
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(path).sort_by_file_name() {
            let entry = entry.map_err(|_| ())?;
            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }
        for file in files {
            let relative = file.strip_prefix(path).map_err(|_| ())?;
            hasher.update(relative.to_string_lossy().as_bytes());
            hash_file_into(&file, &mut hasher)?;
        }
    } else {
        hash_file_into(path, &mut hasher)?;
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn hash_file_into(path: &Path, hasher: &mut blake3::Hasher) -> Result<(), ()> {
    let mut handle = File::open(path).map_err(|_| ())?;
    hash_reader_into(&mut handle, hasher)
}

fn hash_reader_into<R: Read>(reader: &mut R, hasher: &mut blake3::Hasher) -> Result<(), ()> {
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|_| ())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// A directory of one split's shards resolves as one artifact, to its
    /// first shard, and the pin collects the set. A directory mixing stems
    /// stays the ambiguity it always was.
    #[test]
    fn a_sharded_directory_resolves_to_its_first_shard() {
        let dir = std::env::temp_dir().join(format!("weaver-sharded-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        for index in 1..=3 {
            std::fs::write(
                dir.join(format!("model-{index:05}-of-00003.safetensors")),
                b"x",
            )
            .expect("a shard writes");
        }
        let resolved = super::container_within(&dir).expect("the set resolves");
        assert_eq!(
            resolved,
            dir.join("model-00001-of-00003.safetensors"),
            "the first shard is the container"
        );
        let pinned = super::pin(&resolved).expect("the set pins");
        assert_eq!(pinned.paths().len(), 3, "every shard is pinned");

        // A stranger in the directory restores the refusal.
        std::fs::write(dir.join("other.safetensors"), b"x").expect("writes");
        assert!(
            matches!(
                super::container_within(&dir),
                Err(LifecycleRefusal::ArtifactUnresolvable)
            ),
            "a mixed directory is ambiguous"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A split with a missing later shard refuses as unresolvable, which is
    /// the pin doc's own promise: a partial set is an absent artifact.
    #[test]
    fn a_split_missing_a_shard_is_unresolvable() {
        let dir = std::env::temp_dir().join(format!("weaver-split-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let first = dir.join("model-00001-of-00002.gguf");
        std::fs::write(&first, b"GGUF").expect("the first shard writes");
        // The sibling is deliberately absent.
        let outcome = super::pin(&first);
        assert!(
            matches!(outcome, Err(LifecycleRefusal::ArtifactUnresolvable)),
            "a missing sibling is the artifact failing to resolve, got {:?}",
            outcome.as_ref().err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The split pattern is exact: five digits, `-of-`, five digits, the
    /// `.gguf` suffix. A near miss is a single-file artifact rather than a
    /// malformed split, because the pattern is llama.cpp's convention and a
    /// name outside it is just a name.
    #[test]
    fn the_split_pattern_is_exact() {
        use std::path::Path;
        let hit = super::split_pattern(Path::new("/m/model-00001-of-00002.gguf"));
        let (stem, count, suffix) = hit.expect("the pattern parses");
        assert_eq!(stem, Path::new("/m/model"));
        assert_eq!(count, 2);
        assert_eq!(suffix, ".gguf");
        assert_eq!(
            super::shard_name(&stem, 2, count, suffix),
            Path::new("/m/model-00002-of-00002.gguf")
        );

        // Naming a later shard reaches the same set.
        let (stem2, _, _) = super::split_pattern(Path::new("/m/model-00002-of-00002.gguf"))
            .expect("a later shard parses");
        assert_eq!(stem2, stem);

        // The second container spells the same pattern, per the exporter's
        // own convention.
        let (_, count_st, suffix_st) =
            super::split_pattern(Path::new("/m/model-00001-of-00017.safetensors"))
                .expect("a safetensors shard parses");
        assert_eq!(count_st, 17);
        assert_eq!(suffix_st, ".safetensors");

        for miss in [
            "/m/model.gguf",
            "/m/model-1-of-2.gguf",
            "/m/model-00001-of-00000.gguf",
            "/m/model-00001-of-00002.bin",
            "/m/00001-of-00002.gguf",
            // The integer parse alone would take a signed count.
            "/m/model-00001-of-+1234.gguf",
            // An index outside the set it claims membership of.
            "/m/model-00000-of-00002.gguf",
            "/m/model-00003-of-00002.gguf",
        ] {
            assert!(
                super::split_pattern(Path::new(miss)).is_none(),
                "{miss} is not a split"
            );
        }
    }

    use super::*;
    use std::io::Write;

    /// A minimal GGUF whose one metadata value is an array nested past the
    /// depth bound: each level is `element_type=9` and `count=1`, so the file
    /// is small while the recursion it asks for is not.
    fn nested_gguf(depth: u32) -> Vec<u8> {
        let mut file = Vec::new();
        file.extend_from_slice(b"GGUF");
        file.extend_from_slice(&3u32.to_le_bytes()); // version
        file.extend_from_slice(&0u64.to_le_bytes()); // tensor count
        file.extend_from_slice(&1u64.to_le_bytes()); // kv count
        let key = b"a";
        file.extend_from_slice(&(key.len() as u64).to_le_bytes());
        file.extend_from_slice(key);
        file.extend_from_slice(&9u32.to_le_bytes()); // the value is an array
        for _ in 0..depth {
            file.extend_from_slice(&9u32.to_le_bytes()); // of arrays
            file.extend_from_slice(&1u64.to_le_bytes()); // one element each
        }
        file.extend_from_slice(&0u32.to_le_bytes()); // ending in u8
        file.extend_from_slice(&0u64.to_le_bytes()); // of zero elements
        file
    }

    /// **A crafted GGUF refuses instead of aborting.** Without the depth bound
    /// a file of repeated array-of-array headers drives the recursion to a
    /// stack overflow, an abort reachable from an untrusted artifact at step
    /// two of the admit. Every other malformation on this path is a clean
    /// refusal, and this one must be too.
    ///
    /// Perturbation: remove the `depth >= MAX_ARRAY_DEPTH` branch from the
    /// array arm and this test dies by stack overflow rather than failing an
    /// assertion, which is the abort the bound exists to prevent. Watched
    /// under exactly that removal.
    #[test]
    fn a_deeply_nested_gguf_refuses_rather_than_aborting() {
        let dir = std::env::temp_dir().join(format!(
            "weaver-spu-artifact-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let path = dir.join("nested.gguf");
        let mut handle = File::create(&path).expect("a fixture file");
        handle
            .write_all(&nested_gguf(200_000))
            .expect("the fixture is written");
        drop(handle);

        assert_eq!(
            read_header(&mut pin(&path).expect("the fixture pins")),
            Err(LifecycleRefusal::ArtifactUnreadable),
            "a nesting no real artifact carries is a refusal, never an abort"
        );

        // A shallow nesting inside the bound still reads to the family error,
        // which is what shows the bound refuses depth rather than arrays.
        let shallow = dir.join("shallow.gguf");
        let mut handle = File::create(&shallow).expect("a fixture file");
        handle
            .write_all(&nested_gguf(3))
            .expect("the fixture is written");
        drop(handle);
        assert_eq!(
            read_header(&mut pin(&shallow).expect("the fixture pins")),
            Err(LifecycleRefusal::ArtifactUnreadable),
            "no general.architecture key, so the refusal is the absent family"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A directory holding several containers refuses on ambiguity** rather
    /// than silently resolving to the alphabetically first.
    ///
    /// Perturbation: restore the sort-and-take-first and this test fails,
    /// because the header read succeeds against whichever file sorts first.
    /// Watched under exactly that restoration.
    #[test]
    fn an_ambiguous_directory_refuses() {
        let dir = std::env::temp_dir().join(format!(
            "weaver-spu-artifact-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        for name in ["a.gguf", "b.gguf"] {
            let mut handle = File::create(dir.join(name)).expect("a fixture file");
            handle
                .write_all(&super::tests::nested_gguf(0))
                .expect("written");
        }
        assert_eq!(
            resolve(&ArtifactRef(dir.to_string_lossy().into_owned())),
            Err(LifecycleRefusal::ArtifactUnresolvable),
            "two containers is an ambiguity, not a choice"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A directory the identity cannot traverse is a present artifact it
    /// cannot reach**, per Spec section 3, and refuses unreadable rather than
    /// unresolvable, so the operator's account says denied and not absent.
    /// Skips as root, a uid no directory denies, rather than passing on
    /// nothing.
    ///
    /// Perturbation: map every metadata error to unresolvable and this test
    /// fails. Watched under exactly that removal.
    #[test]
    fn a_denied_directory_is_unreadable_rather_than_unresolvable() {
        use std::os::unix::fs::PermissionsExt as _;
        // The suite skips loudly rather than attesting to nothing: a bare
        // return reports as a pass, and this is the one test that watches
        // the fault the ruling exists to catch.
        if nix::unistd::geteuid().is_root() {
            eprintln!(
                "SKIP a_denied_directory_is_unreadable_rather_than_unresolvable: \
                 no directory denies root, rerun as an unprivileged uid"
            );
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "weaver-spu-artifact-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let inside = dir.join("model.gguf");
        std::fs::write(&inside, b"GGUF").expect("a file inside");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("deny");
        let outcome = resolve(&ArtifactRef(inside.to_string_lossy().into_owned()));
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        assert_eq!(
            outcome,
            Err(LifecycleRefusal::ArtifactUnreadable),
            "a denied lookup is present and unreachable, not absent"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A named pipe does not resolve.** `File::open` on a FIFO blocks until a
    /// writer connects, so an artifact reference naming one would hold the
    /// admit open indefinitely where every other malformation refuses.
    ///
    /// Perturbation: drop the `is_file` guard from `resolve` and this test
    /// fails. Watched under exactly that removal.
    #[test]
    fn a_named_pipe_does_not_resolve() {
        let dir = std::env::temp_dir().join(format!(
            "weaver-spu-artifact-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let fifo = dir.join("pipe.gguf");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("a fifo");

        assert_eq!(
            resolve(&ArtifactRef(fifo.to_string_lossy().into_owned())),
            Err(LifecycleRefusal::ArtifactUnresolvable),
            "a pipe named like a container is not an artifact"
        );
        // And the same inside a directory, where it would be selected by name.
        assert_eq!(
            resolve(&ArtifactRef(dir.to_string_lossy().into_owned())),
            Err(LifecycleRefusal::ArtifactUnresolvable),
            "a directory holding only a pipe holds no container"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **The pin refuses what is not a regular file, judged on the descriptor
    /// rather than on the name.** A pipe opens without blocking because of
    /// `O_NONBLOCK` and is refused by the kind check, where a blocking open
    /// would hold the admit open before any check could run.
    ///
    /// Perturbation: drop the `is_file` branch from `pin` and this test fails.
    /// Watched under exactly that removal.
    #[test]
    fn the_pin_refuses_a_pipe_on_the_descriptor() {
        let dir = std::env::temp_dir().join(format!(
            "weaver-spu-artifact-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let fifo = dir.join("pipe.gguf");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("a fifo");
        assert!(
            matches!(pin(&fifo), Err(LifecycleRefusal::ArtifactUnresolvable)),
            "the descriptor says pipe, so the pin refuses"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A directory artifact resolves to the file inside it, at step one.**
    /// An earlier cut resolved the container inside the header read, so the
    /// admission carried the directory while the judgments blessed the file,
    /// and the loader was handed a path it cannot open, refusing as a device
    /// condition three steps after the cause.
    ///
    /// Perturbation: return the directory itself from `resolve` and this test
    /// fails on the extension assertion. Watched under exactly that change.
    #[test]
    fn a_directory_artifact_resolves_to_its_container_file() {
        let dir = std::env::temp_dir().join(format!(
            "weaver-spu-artifact-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let mut handle = File::create(dir.join("only.gguf")).expect("a fixture file");
        handle.write_all(&nested_gguf(0)).expect("written");
        drop(handle);

        let resolved = resolve(&ArtifactRef(dir.to_string_lossy().into_owned()))
            .expect("a one-container directory resolves");
        assert!(
            resolved.is_file(),
            "the admission carries the file, never the directory"
        );
        assert_eq!(resolved.extension().and_then(|e| e.to_str()), Some("gguf"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
