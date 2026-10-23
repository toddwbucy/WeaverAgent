//! Exact manifest acquisition for the separate STE100 duty.
//!
//! The root is the acquired parent directory, never a lexical prefix. Only named
//! regular files are read; their checked owned bytes survive pathname replacement.
//! This proves acquisition and identity, not provisioned immutability, language
//! qualification, model compatibility or readiness. No legacy binding is changed.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use nix::fcntl::{OFlag, OpenHow, ResolveFlag, open, openat, openat2};
use nix::sys::stat::Mode;
use weaver_types::state_flow::{
    STE100_PROTOCOL, Ste100AdmissionCode, Ste100AdmissionRefused, Ste100Instruction,
};

use super::admission::{VerifiedFiles, verify_file_digest, verify_manifest};

/// No public constructor can assert a path's custody from supplied bytes.
#[derive(Debug)]
pub struct AcquiredArtifacts {
    pub(super) manifest_bytes: Arc<[u8]>,
    pub(super) files: VerifiedFiles,
}

impl AcquiredArtifacts {
    pub fn acquire(instruction: &Ste100Instruction) -> Result<Self, Ste100AdmissionRefused> {
        acquire_with(instruction, |_| {})
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
    // '..' has filesystem semantics, and a trailing slash is not a file alias.
    let (parent, final_name) = locator
        .rsplit_once('/')
        .ok_or_else(|| failure(locator, "absolute manifest pathname has no final component"))?;
    if final_name.is_empty() || final_name == "." || final_name == ".." {
        return Err(failure(
            locator,
            "manifest final component is not a regular file",
        ));
    }
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
    let manifest_bytes = read_regular(File::from(manifest), locator)?;
    let manifest = verify_manifest(instruction, &manifest_bytes)?;
    acquired(Acquired::Manifest);
    let mut files = BTreeMap::new();
    for reference in std::iter::once(&manifest.encoder.config)
        .chain(manifest.encoder.weights.iter())
        .chain([&manifest.tokenizer, &manifest.span_head])
    {
        // validate() already checked all names before the first reference open.
        if !files.contains_key(&reference.name) {
            let how = OpenHow::new()
                .flags(OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK)
                .resolve(ResolveFlag::RESOLVE_BENEATH | ResolveFlag::RESOLVE_NO_MAGICLINKS);
            let descriptor = openat2(&root, reference.name.as_str(), how)
                .map_err(|error| failure(&reference.name, error))?;
            let bytes = read_regular(File::from(descriptor), &reference.name)?;
            files.insert(reference.name.clone(), bytes);
        }
        verify_file_digest(reference, &files[&reference.name])?;
        acquired(Acquired::Reference(&reference.name));
    }
    Ok(AcquiredArtifacts {
        manifest_bytes,
        files: VerifiedFiles { files },
    })
}

fn read_regular(mut file: File, name: &str) -> Result<Arc<[u8]>, Ste100AdmissionRefused> {
    let metadata = file.metadata().map_err(|error| failure(name, error))?;
    if !metadata.is_file() {
        return Err(failure(name, "artifact is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| failure(name, error))?;
    Ok(Arc::from(bytes))
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
