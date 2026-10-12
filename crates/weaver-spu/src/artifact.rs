//! conforms: spu-weights-hash-at-admit
//! conforms: spu-hash-failure-refuses
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
    resolve_with_kind(reference).map(|(path, _)| path)
}

/// What the operator's reference named when it resolved: one file, or a
/// directory the container was resolved within.
///
/// **The kind is the resolution's, carried to the hash rather than asked
/// again**, per Spec section 3: a directory renamed, removed or replaced by a
/// file after the pin would otherwise read as a file reference at the hash,
/// which would then hash the pinned container alone and admit an identity
/// without the directory's names and sidecars. Carried, the walk of a
/// directory that is gone fails and refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference {
    File,
    Directory,
}

/// [`resolve`], answering the kind of reference beside the path, from the one
/// look the resolution already takes.
pub fn resolve_with_kind(
    reference: &ArtifactRef,
) -> Result<(PathBuf, Reference), LifecycleRefusal> {
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
        return container_within(&path).map(|container| (container, Reference::Directory));
    }
    // Only a regular file resolves. A FIFO with a container's name would open
    // and then block until a writer connects, which turns a bad binding into
    // an admission that never returns, where every other malformation on this
    // path is a refusal costing no device work.
    if !metadata.is_file() {
        return Err(LifecycleRefusal::ArtifactUnresolvable);
    }
    Ok((path, Reference::File))
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
    /// The names the pin opened, the first shard's first, in the order of the
    /// descriptors above. The directory walk of the weights hash reads a
    /// member it meets under one of these names through its descriptor, so
    /// the hash is of the bytes the load reads and not of whatever the name
    /// holds by the time the walk reaches it.
    names: Vec<PathBuf>,
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
            names: vec![path.to_path_buf()],
        });
    };
    let mut names = Vec::with_capacity(count as usize);
    let first_name = shard_name(&first, 1, count, suffix);
    let file = pin_one(&first_name)?;
    names.push(first_name);
    let mut siblings = Vec::with_capacity(count as usize - 1);
    for index in 2..=count {
        let name = shard_name(&first, index, count, suffix);
        siblings.push(pin_one(&name)?);
        names.push(name);
    }
    Ok(PinnedArtifact {
        file,
        siblings,
        names,
    })
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
        // A dot-entry is not part of the artifact, per the canonical walk of
        // section 3, so a container is never resolved from one: the load
        // would then serve bytes the identity leaves out.
        if is_dot_entry(&entry.file_name()) {
            continue;
        }
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
/// **A hash that cannot be computed refuses the admission**, on the operator's
/// ruling of 2026-10-01, per Spec section 3: an unreadable member, a walk that
/// cannot complete, or a pinned member the walk no longer meets answers
/// `ArtifactUnreadable`, so no agent serves under an identity that does not
/// name its weights.
///
/// The hash is computed at admit by reading the artifact, never from a manifest
/// handed to this process, and it is computed fresh on each call with no cache
/// across an artifact change, which is what makes the third walk's alteration
/// visible. **The members the load reads are hashed through the descriptors
/// the pin holds**: a single file, every shard of a split, and the container
/// met inside a walked directory. A name replaced during the admit therefore
/// cannot change what is hashed, the descriptor holding the inode. A
/// directory's other members are read by name, there being no load of them to
/// agree with.
pub fn weights_hash(
    reference: &Path,
    kind: Reference,
    pinned: &mut PinnedArtifact,
) -> Result<crate::residency::WeightsHash, LifecycleRefusal> {
    let hashed = if kind == Reference::Directory {
        hash_canonical(reference, pinned)
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
    hashed
        .map(crate::residency::WeightsHash)
        .map_err(|()| LifecycleRefusal::ArtifactUnreadable)
}

/// Whether a directory entry's name marks it as outside the artifact. A
/// downloader's bookkeeping, `.cache/` the standing case, sits beside the
/// model under such a name and is no property of the weights.
fn is_dot_entry(name: &std::ffi::OsStr) -> bool {
    name.as_encoded_bytes().first() == Some(&b'.')
}

/// The canonical walk of a directory artifact, per Spec section 3: every
/// regular file beneath it in sorted name order, dot-entries and everything
/// under them excluded, symbolic links followed and their targets hashed under
/// the link's name, each file contributing its path relative to the directory
/// and then its bytes. **Every failure refuses**: an entry the walk cannot
/// read, a link that leads nowhere or loops, and a pinned name the walk does
/// not meet.
fn hash_canonical(path: &Path, pinned: &mut PinnedArtifact) -> Result<String, ()> {
    let mut hasher = blake3::Hasher::new();
    let mut met = vec![false; pinned.names.len()];
    let walk = walkdir::WalkDir::new(path)
        .follow_links(true)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| !is_dot_entry(entry.file_name()));
    for entry in walk {
        let entry = entry.map_err(|_| ())?;
        // Regular files only, after the link is followed: a FIFO would block
        // the read the way it would block the resolution's open.
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(path).map_err(|_| ())?;
        hasher.update(relative.to_string_lossy().as_bytes());
        match pinned.names.iter().position(|name| name == entry.path()) {
            Some(index) => {
                met[index] = true;
                let file = if index == 0 {
                    &mut pinned.file
                } else {
                    &mut pinned.siblings[index - 1]
                };
                file.seek(SeekFrom::Start(0)).map_err(|_| ())?;
                hash_reader_into(file, &mut hasher)?;
            }
            None => {
                let mut handle = File::open(entry.path()).map_err(|_| ())?;
                hash_reader_into(&mut handle, &mut hasher)?;
            }
        }
    }
    // A pinned name the walk did not meet was removed or renamed after the
    // pin: the load reads its bytes and the identity would leave them out.
    if met.contains(&false) {
        return Err(());
    }
    Ok(hasher.finalize().to_hex().to_string())
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
mod tests;
