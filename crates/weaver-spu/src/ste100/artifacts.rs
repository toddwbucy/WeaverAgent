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

use super::admission::{VerifiedFiles, verify_file_digest, verify_manifest};

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

    // Only admission, after validate_bootstrap, uses the prevalidated route. This
    // callback checks compatibility before any referenced model input is opened.
    pub(super) fn acquire_validated(
        instruction: &Ste100Instruction,
        compatible: impl FnOnce(&Ste100ModelManifest) -> Result<(), Ste100AdmissionRefused>,
    ) -> Result<Self, Ste100AdmissionRefused> {
        acquire_validated_with(instruction, compatible, |_| {})
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
    Reference(&'a str),
}

fn acquire_with(
    instruction: &Ste100Instruction,
    acquired: impl FnMut(Acquired<'_>),
) -> Result<AcquiredArtifacts, Ste100AdmissionRefused> {
    instruction
        .validate()
        .map_err(|error| Ste100AdmissionRefused {
            protocol: STE100_PROTOCOL.into(),
            code: Ste100AdmissionCode::InvalidInstruction,
            detail: error.to_string(),
        })?;
    acquire_validated_with(instruction, |_| Ok(()), acquired)
}

fn acquire_validated_with(
    instruction: &Ste100Instruction,
    compatible: impl FnOnce(&Ste100ModelManifest) -> Result<(), Ste100AdmissionRefused>,
    mut acquired: impl FnMut(Acquired<'_>),
) -> Result<AcquiredArtifacts, Ste100AdmissionRefused> {
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
    let (mut manifest_file, manifest_len) = regular_file(File::from(manifest), locator)?;
    let (manifest_bytes, manifest) =
        read_matching_manifest(&mut manifest_file, manifest_len, instruction, || {})?;
    compatible(&manifest)?;
    acquired(Acquired::Manifest);
    let mut files = BTreeMap::new();
    for reference in std::iter::once(&manifest.encoder.config)
        .chain(manifest.encoder.weights.iter())
        .chain([&manifest.tokenizer, &manifest.span_head])
    {
        // validate() already checked all names before the first reference open.
        if !files.contains_key(&reference.name) {
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
            let (mut file, len) = regular_file(File::from(descriptor), &reference.name)?;
            let bytes = read_owned(&mut file, len, &reference.name)?;
            files.insert(reference.name.clone(), bytes);
        }
        verify_file_digest(reference, &files[&reference.name])?;
        acquired(Acquired::Reference(&reference.name));
    }
    Ok(AcquiredArtifacts {
        #[cfg(test)]
        instruction: instruction.clone(),
        manifest,
        manifest_bytes,
        files: VerifiedFiles { files },
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

fn regular_file(file: File, name: &str) -> Result<(File, u64), Ste100AdmissionRefused> {
    let metadata = file.metadata().map_err(|error| failure(name, error))?;
    if !metadata.is_file() {
        return Err(failure(name, "artifact is not a regular file"));
    }
    // Nonblocking open prevents FIFO acquisition from waiting before the kind
    // check. A verified regular descriptor uses ordinary blocking file reads.
    let flags = fcntl(&file, FcntlArg::F_GETFL).map_err(|error| failure(name, error))?;
    let flags = OFlag::from_bits_truncate(flags) & !OFlag::O_NONBLOCK;
    fcntl(&file, FcntlArg::F_SETFL(flags)).map_err(|error| failure(name, error))?;
    Ok((file, metadata.len()))
}

const READ_CHUNK: usize = 16 * 1024;

fn read_matching_manifest(
    reader: &mut (impl Read + Seek),
    len: u64,
    instruction: &Ste100Instruction,
    after_precheck: impl FnOnce(),
) -> Result<(Arc<Vec<u8>>, Ste100ModelManifest), Ste100AdmissionRefused> {
    let name = instruction.model_binding.artifact.0.as_str();
    let mut hash = Sha256::new();
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        let size = read_chunk(reader, &mut chunk, name)?;
        if size == 0 {
            break;
        }
        hash.update(&chunk[..size]);
    }
    let actual =
        Digest::new(format!("{:x}", hash.finalize())).expect("SHA-256 lower hex is a valid Digest");
    if actual != instruction.model_artifact_sha256 {
        return Err(failure(
            name,
            "manifest digest differs from the loaded instruction",
        ));
    }
    // No manifest size policy is invented. A wrong large locator is rejected with
    // fixed scratch memory. A matching declared artifact is still an allocation
    // and qualification obligation. Both reads use this same regular descriptor.
    after_precheck();
    reader.rewind().map_err(|error| failure(name, error))?;
    let bytes = read_owned(reader, len, name)?;
    // Rehash retained bytes before the single closed parse: an in-place change
    // between precheck and materialization cannot gain admission. Downstream
    // admission consumes this parsed owner without hashing or decoding it again.
    let manifest = verify_manifest(instruction, &bytes)?;
    Ok((bytes, manifest))
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
) -> Result<Arc<Vec<u8>>, Ste100AdmissionRefused> {
    let len = usize::try_from(len).map_err(|error| failure(name, error))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|error| failure(name, error))?;
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        let size = read_chunk(reader, &mut chunk, name)?;
        if size == 0 {
            break;
        }
        bytes
            .try_reserve(size)
            .map_err(|error| failure(name, error))?;
        bytes.extend_from_slice(&chunk[..size]);
    }
    Ok(share_bytes(bytes))
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
