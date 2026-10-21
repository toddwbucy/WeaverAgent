//! Descriptor-held verification. No unindexed file becomes a starting state.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use nix::fcntl::OFlag;
use nix::sys::stat::Mode;
use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::{Digest, PersistenceCase, PersistenceCause, StartIdentity};
use weaver_types::{ServingDeclaration, SessionId, TraceSink, parse_serving_declaration};

use super::Fault;
use super::codec::{Arrival, ArtifactId, PublicationGuard, StateBinding, StateIndexEntry};
use crate::save_points::{MANIFEST, Owner, SAVE_POINT_BOUND};

const DUMP: &str = "state.sqlite";
const CONFIGURATION: &str = "agent.toml";
const BINDING: &str = "binding.json";

/// Values from the already judged territory, not from the authoring draft.
/// Inventory still judges binaries, model artifacts and serving port elections.
pub(crate) struct LoadScope<'a> {
    pub(crate) session: &'a SessionId,
    pub(crate) trace_path: &'a Path,
}

/// Root's published-artifact directory. A path is never resolved after entry.
/// The caller must retain admin's invocation lock for the lifetime of any use.
pub(crate) struct Repository {
    directory: OwnedFd,
    root_owner: Owner,
    access_gid: u32,
}

/// Proof of file custody, exact index/binding agreement, the three hashes and
/// parseability of the actual declaration copy. This does not certify SQLite,
/// STE100 qualification, peer liveness or the full inventory admission.
#[derive(Debug)]
pub(crate) struct VerifiedArtifact {
    entry: StateIndexEntry,
    declaration: ServingDeclaration,
    configuration: Vec<u8>,
    dump: File,
}

impl VerifiedArtifact {
    pub(super) fn entry(&self) -> &StateIndexEntry {
        &self.entry
    }

    pub(crate) fn declaration(&self) -> &ServingDeclaration {
        &self.declaration
    }

    pub(crate) fn configuration_bytes(&self) -> &[u8] {
        &self.configuration
    }

    /// The same read-only descriptor whose complete bytes were hashed, at zero.
    pub(crate) fn dump(&self) -> BorrowedFd<'_> {
        self.dump.as_fd()
    }

    pub(crate) fn identity(&self) -> StartIdentity {
        StartIdentity {
            seed_id: self.entry.seed_id.clone(),
            artifact_id: self.entry.artifact_id.to_string(),
            dump_sha256: self.entry.dump_sha256.clone(),
            config_sha256: self.entry.config_sha256.clone(),
            parent_artifact_id: self
                .entry
                .parent_artifact_id
                .as_ref()
                .map(ToString::to_string),
            position: self.entry.position.clone(),
        }
    }
}

impl Repository {
    pub(crate) fn from_judged_directory(
        directory: OwnedFd,
        root_owner: Owner,
        access_gid: u32,
    ) -> Result<Self, Fault> {
        judge_directory(directory.as_fd(), root_owner.uid, access_gid)?;
        Ok(Self {
            directory,
            root_owner,
            access_gid,
        })
    }

    /// N6: a present root selector must name the latest index entry. In
    /// particular, a broken newest save never falls back to an earlier save.
    pub(crate) fn load(
        &self,
        selector: Option<&ArtifactId>,
        scope: &LoadScope<'_>,
    ) -> Result<VerifiedArtifact, Fault> {
        let index = self.read_index()?;
        let latest = index
            .last()
            .ok_or_else(|| layout("no indexed starting artifact"))?;
        if let Some(selector) = selector
            && *selector != latest.artifact_id
        {
            return Err(Fault::new(
                PersistenceCase::ConfigurationMismatch,
                format!(
                    "restore-artifact {selector} is not latest {}",
                    latest.artifact_id
                ),
            ));
        }
        self.verify(latest.clone(), scope)
    }

    /// The read half of restore. This checks an explicitly indexed historical
    /// artifact but does not append a selection or claim restore succeeded.
    /// The publisher must append/sync a selected entry under the same lock.
    pub(crate) fn verify_indexed(
        &self,
        selector: &ArtifactId,
        scope: &LoadScope<'_>,
    ) -> Result<VerifiedArtifact, Fault> {
        let index = self.read_index()?;
        let entry = index
            .iter()
            .rev()
            .find(|entry| entry.artifact_id == *selector)
            .ok_or_else(|| layout(format!("artifact {selector} has no index entry")))?;
        self.verify(entry.clone(), scope)
    }

    /// Empty is a valid pre-seed repository, never a valid serving load. A torn
    /// newest line is an error, including when every preceding line was valid.
    pub(super) fn read_index(&self) -> Result<Vec<StateIndexEntry>, Fault> {
        let Some(mut file) = open_optional(self.directory.as_fd(), MANIFEST)? else {
            return Ok(Vec::new());
        };
        judge_file(&file, self.root_owner, 0o644, MANIFEST)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| layout(format!("read index: {e}")))?;
        let index = super::codec::decode_index(&bytes)
            .map_err(|e| layout(format!("invalid index: {e}")))?;
        let mut published: std::collections::BTreeMap<&ArtifactId, &StateIndexEntry> =
            std::collections::BTreeMap::new();
        for entry in &index {
            if entry.arrival == Arrival::Published {
                if published.is_empty() != (entry.cause == PersistenceCause::Seed) {
                    return Err(layout("a chain begins with exactly one seed publication"));
                }
                if entry.cause != PersistenceCause::Seed && entry.parent_artifact_id.is_none() {
                    return Err(layout(
                        "a later publication must name its earlier starting artifact",
                    ));
                }
                if let Some(parent) = &entry.parent_artifact_id {
                    let parent_entry = published.get(parent).ok_or_else(|| {
                        layout("publication parent is not an earlier indexed artifact")
                    })?;
                    if parent_entry.seed_id != entry.seed_id {
                        return Err(layout("publication parent belongs to another seed"));
                    }
                    if entry.cause == PersistenceCause::Rebind
                        && (entry.dump_sha256 != parent_entry.dump_sha256
                            || entry.position != parent_entry.position)
                    {
                        return Err(layout(
                            "rebind changes its parent's dump or source frontier",
                        ));
                    }
                }
                published.insert(&entry.artifact_id, entry);
            }
        }
        Ok(index)
    }

    fn verify(
        &self,
        entry: StateIndexEntry,
        scope: &LoadScope<'_>,
    ) -> Result<VerifiedArtifact, Fault> {
        self.refuse_guard(&entry.artifact_id)?;
        let directory = nix::fcntl::openat(
            self.directory.as_fd(),
            entry.artifact_id.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| layout(format!("open artifact {}: {e}", entry.artifact_id)))?;
        judge_directory(directory.as_fd(), self.root_owner.uid, self.access_gid)?;
        exact_files(directory.as_fd())?;
        let owner = Owner {
            uid: self.root_owner.uid,
            gid: self.access_gid,
        };
        let binding_bytes = read_judged(directory.as_fd(), BINDING, owner)?;
        let binding = StateBinding::from_json(&binding_bytes)
            .map_err(|e| binding_fault(format!("invalid binding: {e}")))?;
        if !entry.matches_binding(&binding)
            || entry.artifact_id.config_digest() != &binding.config_sha256
        {
            return Err(binding_fault(
                "selected index, directory and binding disagree",
            ));
        }
        let mut dump = open_required(directory.as_fd(), DUMP)?;
        judge_file(&dump, owner, 0o640, DUMP)?;
        let actual_a = hash_dump(&mut dump)?;
        if actual_a != binding.dump_sha256 {
            return Err(binding_fault("state.sqlite does not match dump_sha256"));
        }
        let configuration = read_judged(directory.as_fd(), CONFIGURATION, owner)?;
        if hash(&configuration) != binding.config_sha256 {
            return Err(binding_fault("agent.toml does not match config_sha256"));
        }
        let text = std::str::from_utf8(&configuration)
            .map_err(|e| configuration_fault(format!("agent.toml is not UTF-8: {e}")))?;
        let declaration = parse_serving_declaration(text)
            .map_err(|e| configuration_fault(format!("agent.toml does not parse: {e:?}")))?;
        if declaration.state_management().start.dump_sha256 != binding.dump_sha256 {
            return Err(binding_fault("agent.toml names a different dump_sha256"));
        }
        let agent = declaration.agent();
        let trace_path = match &agent.trace_sink {
            TraceSink::File { path, .. }
            | TraceSink::Pipe { path, .. }
            | TraceSink::Socket { path } => path,
        };
        if agent.session != *scope.session || trace_path != scope.trace_path {
            return Err(configuration_fault(
                "agent.toml identity or trace path disagrees with judged territory",
            ));
        }
        Ok(VerifiedArtifact {
            entry,
            declaration,
            configuration,
            dump,
        })
    }

    fn refuse_guard(&self, id: &ArtifactId) -> Result<(), Fault> {
        let name = format!(".{id}.pending");
        let Some(mut file) = open_optional(self.directory.as_fd(), &name)? else {
            return Ok(());
        };
        // A malformed guard is still a refusal, never evidence it can be ignored.
        judge_file(&file, self.root_owner, 0o600, &name)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| layout(format!("read guard: {e}")))?;
        let guard = PublicationGuard::from_json(&bytes)
            .map_err(|e| layout(format!("invalid publication guard: {e}")))?;
        if guard.artifact_id != *id {
            return Err(layout("publication guard names a different artifact"));
        }
        Err(Fault::new(
            PersistenceCase::BrokenSavePoint,
            format!(
                "artifact {id} has {:?} publication at {:?}",
                guard.state, guard.phase
            ),
        ))
    }
}

fn layout(detail: impl Into<String>) -> Fault {
    Fault::new(PersistenceCase::UnsupportedLayout, detail)
}

fn binding_fault(detail: impl Into<String>) -> Fault {
    Fault::new(PersistenceCase::BindingMismatch, detail)
}

fn configuration_fault(detail: impl Into<String>) -> Fault {
    Fault::new(PersistenceCase::ConfigurationMismatch, detail)
}

fn open_optional(directory: BorrowedFd<'_>, name: &str) -> Result<Option<File>, Fault> {
    match nix::fcntl::openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => Ok(Some(File::from(fd))),
        Err(nix::errno::Errno::ENOENT) => Ok(None),
        Err(e) => Err(layout(format!("open {name}: {e}"))),
    }
}

fn open_required(directory: BorrowedFd<'_>, name: &str) -> Result<File, Fault> {
    open_optional(directory, name)?.ok_or_else(|| layout(format!("missing {name}")))
}

fn read_judged(directory: BorrowedFd<'_>, name: &str, owner: Owner) -> Result<Vec<u8>, Fault> {
    let mut file = open_required(directory, name)?;
    judge_file(&file, owner, 0o640, name)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| layout(format!("read {name}: {e}")))?;
    Ok(bytes)
}

fn judge_file(file: &File, owner: Owner, mode: u32, name: &str) -> Result<(), Fault> {
    let metadata = file
        .metadata()
        .map_err(|e| layout(format!("stat {name}: {e}")))?;
    if !metadata.is_file()
        || metadata.uid() != owner.uid
        || metadata.gid() != owner.gid
        || metadata.mode() & 0o7777 != mode
        || metadata.nlink() != 1
    {
        return Err(layout(format!(
            "{name} is not a single-link regular file of the declared owner/mode"
        )));
    }
    judge_acl(file.as_fd(), name)?;
    Ok(())
}

fn judge_directory(directory: BorrowedFd<'_>, uid: u32, gid: u32) -> Result<(), Fault> {
    let metadata = nix::sys::stat::fstat(directory)
        .map_err(|e| layout(format!("stat artifact directory: {e}")))?;
    if metadata.st_mode & nix::libc::S_IFMT != nix::libc::S_IFDIR
        || metadata.st_mode & 0o7777 != 0o750
        || metadata.st_uid != uid
        || metadata.st_gid != gid
    {
        return Err(layout(
            "artifact directory has the wrong type, owner or mode",
        ));
    }
    judge_acl(directory, "artifact directory")?;
    Ok(())
}

fn judge_acl(descriptor: BorrowedFd<'_>, name: &str) -> Result<(), Fault> {
    match crate::carries_access_entries_fd(descriptor) {
        Ok(false) => Ok(()),
        Ok(true) => Err(layout(format!(
            "{name} carries an access-control entry beyond its mode"
        ))),
        Err(e) => Err(layout(format!(
            "{name} cannot be judged for access-control entries: {e}"
        ))),
    }
}

fn exact_files(directory: BorrowedFd<'_>) -> Result<(), Fault> {
    let descriptor =
        nix::unistd::dup(directory).map_err(|e| layout(format!("dup directory: {e}")))?;
    let mut directory =
        nix::dir::Dir::from_fd(descriptor).map_err(|e| layout(format!("read directory: {e}")))?;
    let mut names = BTreeSet::new();
    for entry in directory.iter() {
        let entry = entry.map_err(|e| layout(format!("read directory entry: {e}")))?;
        let name = entry.file_name().to_bytes();
        if name != b"." && name != b".." {
            names.insert(name.to_vec());
        }
    }
    let expected = BTreeSet::from([
        DUMP.as_bytes().to_vec(),
        CONFIGURATION.as_bytes().to_vec(),
        BINDING.as_bytes().to_vec(),
    ]);
    if names != expected {
        return Err(layout(
            "artifact directory does not contain exactly its three files",
        ));
    }
    Ok(())
}

fn hash(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes))).expect("SHA-256 renders a digest")
}

fn hash_dump(file: &mut File) -> Result<Digest, Fault> {
    if file
        .metadata()
        .map_err(|e| layout(format!("stat dump: {e}")))?
        .len()
        > SAVE_POINT_BOUND
    {
        return Err(layout("dump exceeds the existing save-point read bound"));
    }
    file.rewind()
        .map_err(|e| layout(format!("rewind dump: {e}")))?;
    let mut digest = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| layout(format!("read dump: {e}")))?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read as u64)
            .ok_or_else(|| layout("dump byte count overflows"))?;
        if count > SAVE_POINT_BOUND {
            return Err(layout("dump exceeds the existing save-point read bound"));
        }
        digest.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| layout(format!("rewind checked dump: {e}")))?;
    Ok(Digest::new(format!("{:x}", digest.finalize())).expect("SHA-256 renders a digest"))
}

#[cfg(test)]
mod tests;
