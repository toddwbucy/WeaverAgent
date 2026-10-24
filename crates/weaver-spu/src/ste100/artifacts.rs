//! Exact manifest acquisition for the separate STE100 duty.
//!
//! The root is the acquired parent directory, never a lexical prefix. Only named
//! regular files are read; their checked owned bytes survive pathname replacement.
//! This proves acquisition and identity, not provisioned immutability, language
//! qualification, model compatibility or readiness. No legacy binding is changed.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::sync::Arc;

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, OpenHow, ResolveFlag, fcntl, open, openat, openat2};
use nix::sys::stat::Mode;
use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::{
    Digest, STE100_PROTOCOL, Ste100AdmissionCode, Ste100AdmissionRefused, Ste100Instruction,
    Ste100ModelManifest,
};

use super::admission::{VerifiedFiles, parse_manifest};

/// No public constructor can assert a path's custody from supplied bytes.
#[derive(Debug)]
pub struct AcquiredArtifacts {
    #[cfg(test)]
    pub(super) instruction: Ste100Instruction,
    pub(super) manifest: Ste100ModelManifest,
    pub(super) manifest_bytes: Arc<Vec<u8>>,
    pub(super) files: VerifiedFiles,
}

impl AcquiredArtifacts {
    pub fn acquire(instruction: &Ste100Instruction) -> Result<Self, Ste100AdmissionRefused> {
        acquire_with(instruction, |_| {})
    }

    // Admission checks compatibility before any referenced model input is opened.
    // Every route still checks the exact instruction before any path is opened.
    pub(super) fn acquire_compatible(
        instruction: &Ste100Instruction,
        compatible: impl FnOnce(&Ste100ModelManifest) -> Result<(), Ste100AdmissionRefused>,
    ) -> Result<Self, Ste100AdmissionRefused> {
        acquire_compatible_with(instruction, compatible, |_| {})
    }

    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }
    pub fn files(&self) -> &VerifiedFiles {
        &self.files
    }
}

// Hooks observe completed acquisitions; they never replace path resolution,
// object-kind checks or hashing. Production supplies the empty observer.
#[derive(Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))] // The race observer reads these fields only in tests.
enum Acquired<'a> {
    Root,
    Manifest,
    Materialize(&'a str),
    Reference(&'a str),
}

fn acquire_with(
    instruction: &Ste100Instruction,
    acquired: impl FnMut(Acquired<'_>),
) -> Result<AcquiredArtifacts, Ste100AdmissionRefused> {
    acquire_compatible_with(instruction, |_| Ok(()), acquired)
}

fn acquire_compatible_with(
    instruction: &Ste100Instruction,
    compatible: impl FnOnce(&Ste100ModelManifest) -> Result<(), Ste100AdmissionRefused>,
    mut acquired: impl FnMut(Acquired<'_>),
) -> Result<AcquiredArtifacts, Ste100AdmissionRefused> {
    instruction
        .validate()
        .map_err(|error| Ste100AdmissionRefused {
            protocol: STE100_PROTOCOL.into(),
            code: Ste100AdmissionCode::InvalidInstruction,
            detail: error.to_string(),
        })?;
    let locator = instruction.model_binding.artifact.0.as_str();
    // Split bytes without Path::components normalization: a symlink followed by
    // '..' has filesystem semantics. Validation already rejected a trailing slash.
    let (parent, final_name) = locator
        .rsplit_once('/')
        .ok_or_else(|| failure(locator, "absolute manifest pathname has no final component"))?;
    let parent = if parent.is_empty() { "/" } else { parent };
    // O_PATH pins a directory without imposing a read-directory permission that
    // ordinary pathname lookup does not require. Ancestors resolve normally.
    let root = open(
        parent,
        OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| failure(locator, error))?;
    acquired(Acquired::Root);
    let manifest = openat(
        &root,
        final_name,
        OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| failure(locator, error))?;
    let mut manifest_file = regular_file(File::from(manifest), locator)?;
    let (manifest_bytes, manifest) =
        read_matching_manifest(&mut manifest_file, instruction, || {
            acquired(Acquired::Materialize(locator))
        })?;
    compatible(&manifest)?;
    acquired(Acquired::Manifest);
    let mut files: BTreeMap<String, HashedBytes> = BTreeMap::new();
    for reference in std::iter::once(&manifest.encoder.config)
        .chain(manifest.encoder.weights.iter())
        .chain([&manifest.tokenizer, &manifest.span_head])
    {
        // validate() already checked all names before the first reference open.
        if let Some(held) = files.get(&reference.name) {
            // A repeated declaration shares one allocation, but must name its digest.
            if held.digest != reference.sha256 {
                return Err(failure(&reference.name, "manifest file digest differs"));
            }
        } else {
            let descriptor = retry_confined_open(|| {
                // BENEATH currently also disables magic links on Linux. Keep the
                // explicit NO_MAGICLINKS requirement if that kernel behavior changes;
                // the two flags cannot be independently perturbed on this kernel.
                let how = OpenHow::new()
                    .flags(OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK)
                    .resolve(ResolveFlag::RESOLVE_BENEATH | ResolveFlag::RESOLVE_NO_MAGICLINKS);
                openat2(&root, reference.name.as_str(), how)
            })
            .map_err(|error| failure(&reference.name, error))?;
            let mut file = regular_file(File::from(descriptor), &reference.name)?;
            let held = read_matching_bytes(&mut file, &reference.sha256, &reference.name, || {
                acquired(Acquired::Materialize(&reference.name))
            })?;
            files.insert(reference.name.clone(), held);
        }
        acquired(Acquired::Reference(&reference.name));
    }
    Ok(AcquiredArtifacts {
        #[cfg(test)]
        instruction: instruction.clone(),
        manifest,
        manifest_bytes,
        files: VerifiedFiles {
            files: files
                .into_iter()
                .map(|(name, held)| (name, held.bytes))
                .collect(),
        },
    })
}

// A rename/mount race may prevent BENEATH from proving confinement. Retry only
// that transient result, with the same pinned root, name and resolution flags.
fn retry_confined_open<T>(mut open: impl FnMut() -> Result<T, Errno>) -> Result<T, Errno> {
    const ATTEMPTS: usize = 3;
    for attempt in 0..ATTEMPTS {
        match open() {
            Err(Errno::EAGAIN) if attempt + 1 < ATTEMPTS => {}
            result => return result,
        }
    }
    unreachable!("the final bounded attempt always returns")
}

fn regular_file(file: File, name: &str) -> Result<File, Ste100AdmissionRefused> {
    let metadata = file.metadata().map_err(|error| failure(name, error))?;
    if !metadata.is_file() {
        return Err(failure(name, "artifact is not a regular file"));
    }
    // Nonblocking open prevents FIFO acquisition from waiting before the kind
    // check. A verified regular descriptor uses ordinary blocking file reads.
    let flags = fcntl(&file, FcntlArg::F_GETFL).map_err(|error| failure(name, error))?;
    let flags = blocking_regular_flags(flags);
    fcntl(&file, FcntlArg::F_SETFL(flags)).map_err(|error| failure(name, error))?;
    Ok(file)
}

// Retain status bits even when this version of nix does not name them.
fn blocking_regular_flags(flags: i32) -> OFlag {
    OFlag::from_bits_retain(flags & !OFlag::O_NONBLOCK.bits())
}

const READ_CHUNK: usize = 16 * 1024;

#[derive(Debug)]
struct HashedBytes {
    bytes: Arc<Vec<u8>>,
    digest: Digest,
}

fn read_matching_manifest(
    reader: &mut (impl Read + Seek),
    instruction: &Ste100Instruction,
    after_precheck: impl FnOnce(),
) -> Result<(Arc<Vec<u8>>, Ste100ModelManifest), Ste100AdmissionRefused> {
    let name = instruction.model_binding.artifact.0.as_str();
    let held = read_matching_bytes(
        reader,
        &instruction.model_artifact_sha256,
        name,
        after_precheck,
    )?;
    // This owner has checked the retained digest while reading. Parse once, with
    // no additional whole-buffer hash pass and no pathname reopen.
    let manifest = parse_manifest(instruction, &held.bytes)?;
    Ok((held.bytes, manifest))
}

fn read_matching_bytes(
    reader: &mut (impl Read + Seek),
    expected: &Digest,
    name: &str,
    after_precheck: impl FnOnce(),
) -> Result<HashedBytes, Ste100AdmissionRefused> {
    let mut hash = Sha256::new();
    let mut chunk = [0u8; READ_CHUNK];
    let mut len = 0u64;
    loop {
        let size = read_chunk(reader, &mut chunk, name)?;
        if size == 0 {
            break;
        }
        hash.update(&chunk[..size]);
        len = len
            .checked_add(size as u64)
            .ok_or_else(|| failure(name, "artifact byte count overflow"))?;
    }
    if finish_digest(hash) != *expected {
        return Err(failure(
            name,
            "artifact digest differs from the declaration",
        ));
    }
    // Wrong large or sparse inputs reach no materialization. A matching input is
    // still an allocation/qualification obligation, without an invented size cap.
    // Reserve the observed length rather than unchecked metadata from a pathname.
    after_precheck();
    reader.rewind().map_err(|error| failure(name, error))?;
    let held = read_owned(reader, len, name)?;
    if held.digest != *expected {
        return Err(failure(
            name,
            "retained artifact digest differs from the declaration",
        ));
    }
    Ok(held)
}

fn finish_digest(hash: Sha256) -> Digest {
    Digest::new(format!("{:x}", hash.finalize())).expect("SHA-256 lower hex is a valid Digest")
}

fn read_chunk(
    reader: &mut impl Read,
    chunk: &mut [u8],
    name: &str,
) -> Result<usize, Ste100AdmissionRefused> {
    loop {
        match reader.read(chunk) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => return result.map_err(|error| failure(name, error)),
        }
    }
}

fn read_owned(
    reader: &mut impl Read,
    len: u64,
    name: &str,
) -> Result<HashedBytes, Ste100AdmissionRefused> {
    let len = usize::try_from(len).map_err(|error| failure(name, error))?;
    let mut bytes = Vec::new();
    let mut hash = Sha256::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|error| failure(name, error))?;
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        // The observed precheck length is the admitted input size, not a policy cap.
        // Read at most its remaining bytes plus one sentinel to detect growth.
        let remaining = len - bytes.len();
        let read_limit = remaining.saturating_add(1).min(chunk.len());
        let size = read_chunk(reader, &mut chunk[..read_limit], name)?;
        if size == 0 {
            if bytes.len() != len {
                return Err(failure(name, "artifact size changed after precheck"));
            }
            break;
        }
        if size > remaining {
            return Err(failure(name, "artifact size changed after precheck"));
        }
        hash.update(&chunk[..size]);
        bytes.extend_from_slice(&chunk[..size]);
    }
    Ok(HashedBytes {
        bytes: share_bytes(bytes),
        digest: finish_digest(hash),
    })
}

fn share_bytes(bytes: Vec<u8>) -> Arc<Vec<u8>> {
    // Arc owns the Vec header; its payload allocation is moved, never copied.
    Arc::new(bytes)
}

fn failure(name: &str, detail: impl std::fmt::Display) -> Ste100AdmissionRefused {
    Ste100AdmissionRefused {
        protocol: STE100_PROTOCOL.into(),
        code: Ste100AdmissionCode::UnsupportedArtifact,
        detail: format!("artifact {name}: {detail}"),
    }
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
