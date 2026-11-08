//! SPU-owned fixture input and preflight, per LORA-1 and LORA-2.
//! No production declaration or wire member selects this mechanic.
//! The fixture accepts ordinary F32, F16 and BF16 matrix pairs. Other adapter
//! features refuse. Artifact selection and real application proof remain LORA-8 cells.

use crate::artifact::directory as gguf;
#[cfg(feature = "gguf")]
pub mod observation;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use weaver_types::LifecycleRefusal;

/// The four fixture members of LORA-1, beside the unchanged base instruction.
#[derive(Debug, Clone)]
pub struct Request {
    pub artifact: PathBuf,
    pub sha256: String,
    pub expected_base_weights_hash: String,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailureKind {
    Missing,
    Unreadable,
    Incompatible,
    Load,
}

/// LORA-2's artifact, stage and effective identity account.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub kind: FailureKind,
    pub path: PathBuf,
    pub stage: &'static str,
    pub detail: String,
    pub uid: u32,
    pub gid: u32,
}

impl Failure {
    pub(crate) fn new(
        request: &Request,
        kind: FailureKind,
        stage: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            path: request.artifact.clone(),
            stage,
            detail: detail.into(),
            uid: nix::unistd::geteuid().as_raw(),
            gid: nix::unistd::getegid().as_raw(),
        }
    }

    pub fn lifecycle(&self) -> LifecycleRefusal {
        match self.kind {
            FailureKind::Missing => LifecycleRefusal::ArtifactUnresolvable,
            FailureKind::Unreadable => LifecycleRefusal::ArtifactUnreadable,
            FailureKind::Incompatible | FailureKind::Load => LifecycleRefusal::DeviceCannotAdmit,
        }
    }
}

impl Request {
    pub(crate) fn validate(&self) -> Result<(), Failure> {
        let digest = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.artifact.as_os_str().is_empty()
            || !digest(&self.sha256)
            || !digest(&self.expected_base_weights_hash)
            || !self.scale.is_finite()
        {
            return Err(Failure::new(
                self,
                FailureKind::Unreadable,
                "adapter-input",
                "explicit path, lowercase digests and finite scale are required",
            ));
        }
        Ok(())
    }
}

/// LORA-4 and LORA-5: a failed attachment destroys the new context.
#[cfg(any(feature = "gguf", test))]
pub(crate) fn attach_before_use<C, A, E>(
    context: C,
    adapter: Option<&A>,
    attach: impl FnOnce(&C, &A) -> Result<(), E>,
) -> Result<C, E> {
    if let Some(adapter) = adapter {
        attach(&context, adapter)?;
    }
    Ok(context)
}

/// LORA-6 facts retained with the resident adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub sha256: String,
    pub base_weights_hash: String,
    pub scale: f32,
    /// Stored alpha, absent when the artifact leaves the backend default in use.
    pub alpha: Option<f32>,
    pub targets: BTreeMap<String, u64>,
    /// Each target's adapter bytes (its `lora_a` and `lora_b` together), which
    /// the room judgment charges to the device holding the target's base
    /// (device pair amendment PAIR-4).
    pub target_bytes: BTreeMap<String, u64>,
    pub tensor_bytes: u64,
}

/// Isolated fixture driver for LORA-1, LORA-3 and LORA-6.
/// Normal serving calls Residency::admit and cannot supply an adapter.
#[cfg(feature = "gguf")]
pub struct Fixture {
    residency: crate::residency::Residency,
}

#[cfg(feature = "gguf")]
impl Fixture {
    /// Call once in the fixture process, before any backend admission.
    pub fn new() -> Result<Self, String> {
        observation::install()?;
        Ok(Self {
            residency: crate::residency::Residency::new(),
        })
    }

    pub fn admit(
        &mut self,
        binding: &weaver_types::ModelBinding,
        headroom: crate::residency::Headroom,
        positions: u32,
        readout: crate::readout::ReadoutElection,
        surprisal: bool,
        request: Option<&Request>,
    ) -> Result<&crate::residency::Resident, crate::residency::AdmitRefusal> {
        self.residency
            .admit_with_adapter(binding, headroom, positions, readout, surprisal, request)
    }

    pub fn records(&self) -> Result<Vec<observation::Record>, String> {
        observation::records()
    }

    pub fn release(&mut self) -> Result<(), LifecycleRefusal> {
        self.residency.release()
    }
}

pub(crate) struct Prepared {
    request: Request,
    file: File,
    pub identity: Identity,
}

impl Prepared {
    #[cfg(any(feature = "gguf", test))]
    pub fn path(&self) -> PathBuf {
        PathBuf::from(format!("/proc/self/fd/{}", self.file.as_raw_fd()))
    }

    pub fn failure(
        &self,
        kind: FailureKind,
        stage: &'static str,
        detail: impl Into<String>,
    ) -> Failure {
        Failure::new(&self.request, kind, stage, detail)
    }

    pub fn verify(&mut self, base_hash: &str) -> Result<(), Failure> {
        // LORA-2: hash and load use the same retained descriptor.
        let hash = (|| -> std::io::Result<String> {
            self.file.seek(SeekFrom::Start(0))?;
            let mut hasher = Sha256::new();
            let mut buffer = [0; 65536];
            loop {
                let count = self.file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
            Ok(format!("{:x}", hasher.finalize()))
        })()
        .map_err(|e| self.failure(FailureKind::Unreadable, "adapter-hash", e.to_string()))?;
        if hash != self.request.sha256 {
            return Err(self.failure(
                FailureKind::Unreadable,
                "adapter-hash",
                "adapter SHA-256 mismatch",
            ));
        }
        if base_hash != self.request.expected_base_weights_hash {
            return Err(self.failure(
                FailureKind::Incompatible,
                "adapter-base-identity",
                "runtime base BLAKE3 mismatch",
            ));
        }
        Ok(())
    }
}

pub(crate) fn prepare(
    request: &Request,
    base_paths: &[PathBuf],
    architecture: &str,
) -> Result<Prepared, Failure> {
    request.validate()?;
    let file = File::options()
        .read(true)
        .custom_flags(nix::libc::O_NONBLOCK)
        .open(&request.artifact)
        .map_err(|e| {
            let kind = if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) {
                FailureKind::Missing
            } else {
                FailureKind::Unreadable
            };
            Failure::new(request, kind, "adapter-open", e.to_string())
        })?;
    judge_custody(&file)
        .map_err(|e| Failure::new(request, FailureKind::Unreadable, "adapter-custody", e))?;
    let path = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
    let identity = inspect(&path, base_paths, architecture, request)
        .map_err(|e| Failure::new(request, e.kind, "adapter-metadata", e.detail))?;
    Ok(Prepared {
        request: request.clone(),
        file,
        identity,
    })
}

fn judge_custody(file: &File) -> Result<(), String> {
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.uid() == nix::unistd::geteuid().as_raw() {
        return Err("adapter must be a regular file owned outside the agent uid".into());
    }
    // LORA-2: the kernel judges ACLs and effective credentials on this inode.
    let result = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_faccessat2,
            file.as_raw_fd(),
            c"".as_ptr(),
            nix::libc::W_OK,
            nix::libc::AT_EMPTY_PATH | nix::libc::AT_EACCESS,
        )
    };
    if result == 0 {
        return Err("adapter is writable by the agent uid".into());
    }
    judge_write_denial(std::io::Error::last_os_error().raw_os_error())
}

fn judge_write_denial(errno: Option<i32>) -> Result<(), String> {
    if !matches!(errno, Some(nix::libc::EACCES | nix::libc::EROFS)) {
        return Err("cannot establish adapter write denial".into());
    }
    Ok(())
}

struct InspectionFault {
    kind: FailureKind,
    detail: String,
}

impl std::fmt::Debug for InspectionFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

impl From<String> for InspectionFault {
    fn from(detail: String) -> Self {
        Self {
            kind: FailureKind::Unreadable,
            detail,
        }
    }
}

impl From<&str> for InspectionFault {
    fn from(detail: &str) -> Self {
        detail.to_owned().into()
    }
}

fn incompatible(detail: impl Into<String>) -> InspectionFault {
    InspectionFault {
        kind: FailureKind::Incompatible,
        detail: detail.into(),
    }
}

fn inspect(
    path: &Path,
    base_paths: &[PathBuf],
    architecture: &str,
    request: &Request,
) -> Result<Identity, InspectionFault> {
    use gguf::Value;
    let adapter = gguf::read(path)?;
    for (key, expected) in [
        ("general.type", "adapter"),
        ("adapter.type", "lora"),
        ("general.architecture", architecture),
    ] {
        if !matches!(adapter.values.get(key), Some(Value::Text(value)) if value == expected) {
            return Err(if key == "general.architecture" {
                incompatible(format!("{key} must match {expected}"))
            } else {
                format!("{key} must be {expected}").into()
            });
        }
    }
    if adapter.values.keys().any(|key| {
        key.starts_with("adapter.")
            && !matches!(key.as_str(), "adapter.type" | "adapter.lora.alpha")
    }) {
        return Err(incompatible(
            "unsupported adapter feature, including token-triggered aLoRA",
        ));
    }
    let alpha = match adapter.values.get("adapter.lora.alpha") {
        None => None,
        Some(Value::Float(value)) if value.is_finite() => Some(*value),
        _ => return Err("adapter alpha must be finite f32".into()),
    };
    let mut base = BTreeMap::new();
    for path in base_paths {
        for (name, tensor) in gguf::read(path)?.tensors {
            if base.insert(name, tensor).is_some() {
                return Err("duplicate base tensor across shards".into());
            }
        }
    }
    let mut pairs: BTreeMap<String, [Option<&gguf::Tensor>; 2]> = BTreeMap::new();
    let mut ranges = Vec::new();
    let mut tensor_bytes = 0u64;
    let mut target_bytes: BTreeMap<String, u64> = BTreeMap::new();
    for (name, tensor) in &adapter.tensors {
        let (target, side) = if let Some(target) = name.strip_suffix(".lora_a") {
            (target, 0)
        } else if let Some(target) = name.strip_suffix(".lora_b") {
            (target, 1)
        } else {
            return Err(incompatible(format!(
                "unsupported or skipped adapter tensor {name}"
            )));
        };
        if tensor.dims[2..] != [1, 1] {
            return Err(incompatible(format!(
                "unsupported non-matrix adapter tensor {name}"
            )));
        }
        let width = match tensor.kind {
            0 => 4,
            1 | 30 => 2,
            _ => {
                return Err(incompatible(format!(
                    "unsupported adapter tensor type for {name}"
                )));
            }
        };
        let size = tensor
            .dims
            .iter()
            .try_fold(width, |n: u64, dim| n.checked_mul(*dim))
            .ok_or("adapter tensor byte count overflow")?;
        let end = tensor
            .offset
            .checked_add(size)
            .ok_or("adapter tensor extent overflow")?;
        if end > adapter.file_bytes - adapter.data_offset {
            return Err(format!("adapter tensor {name} exceeds its file").into());
        }
        ranges.push((tensor.offset, end));
        tensor_bytes = tensor_bytes
            .checked_add(size)
            .ok_or("adapter storage overflow")?;
        let per_target = target_bytes.entry(target.into()).or_default();
        *per_target = per_target
            .checked_add(size)
            .ok_or("adapter storage overflow")?;
        pairs.entry(target.into()).or_default()[side] = Some(tensor);
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|r| r[0].1 > r[1].0) || pairs.is_empty() {
        return Err("empty or overlapping adapter tensor data".into());
    }
    let mut targets = BTreeMap::new();
    for (name, pair) in pairs {
        let [Some(a), Some(b)] = pair else {
            return Err(format!("missing adapter pair for {name}").into());
        };
        let tensor = base
            .get(&name)
            .ok_or_else(|| incompatible(format!("unknown base target {name}")))?;
        if tensor.dims[2..] != [1, 1] {
            return Err(incompatible(format!(
                "unsupported non-matrix base target {name}"
            )));
        }
        let rank = if name.ends_with("token_embd.weight") {
            if tensor.dims[0] != b.dims[1] || tensor.dims[1] != a.dims[1] || a.dims[0] != b.dims[0]
            {
                return Err(incompatible(format!(
                    "incompatible embedding shapes for {name}"
                )));
            }
            a.dims[0]
        } else {
            if tensor.dims[0] != a.dims[0] || tensor.dims[1] != b.dims[1] || a.dims[1] != b.dims[0]
            {
                return Err(incompatible(format!(
                    "incompatible matrix shapes for {name}"
                )));
            }
            a.dims[1]
        };
        targets.insert(name, rank);
    }
    Ok(Identity {
        sha256: request.sha256.clone(),
        base_weights_hash: request.expected_base_weights_hash.clone(),
        scale: request.scale,
        alpha,
        targets,
        target_bytes,
        tensor_bytes,
    })
}

#[cfg(test)]
mod tests;
