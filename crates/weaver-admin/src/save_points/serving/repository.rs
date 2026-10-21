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
use weaver_types::state_flow::{
    DecimalU64, Digest, PersistenceCase, PersistenceCause, PersistencePhase, StartIdentity,
};
use weaver_types::{ServingDeclaration, SessionId, parse_serving_declaration};

use super::Fault;
use super::codec::{Arrival, ArtifactId, PublicationGuard, StateBinding, StateIndexEntry};
use crate::save_points::{MANIFEST, Owner, SAVE_POINT_BOUND};

const DUMP: &str = "state.sqlite";
const CONFIGURATION: &str = "agent.toml";
const BINDING: &str = "binding.json";

/// Identity constraints from the independently judged territory declaration.
/// Never derive these expectations from the candidate artifact being checked.
/// Inventory still judges binaries, model artifacts and serving port elections.
pub(crate) struct LoadScope<'a> {
    pub(crate) session: &'a SessionId,
    pub(crate) trace_path: &'a Path,
}

/// Root's published-artifact directory. A path is never resolved after entry.
/// The caller must retain admin's invocation lock for the lifetime of any use.
pub(crate) struct Repository {
    pub(super) directory: OwnedFd,
    pub(super) root_owner: Owner,
    pub(super) access_gid: u32,
}

/// Proof of file custody, agreement with the expected entry, the three hashes
/// and parseability of the declaration copy. Index membership and guard absence
/// depend on the verifying route. This does not certify SQLite, STE100
/// qualification, peer liveness or the full inventory admission.
#[derive(Debug)]
pub(crate) struct VerifiedArtifact {
    entry: StateIndexEntry,
    declaration: ServingDeclaration,
    configuration: Vec<u8>,
    dump: File,
}

/// A restore failure never claims that an attempted append left the index alone.
/// The caller must inspect that index before deciding whether another act is safe.
#[derive(Debug)]
pub(crate) struct RestoreFailure {
    pub(crate) fault: Fault,
    pub(crate) selection_may_be_appended: bool,
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

    /// Seed preparation runs under both admin locks and admits no earlier chain.
    /// The same strict index reader is used again at publication.
    pub(crate) fn require_unseeded(&self) -> Result<(), Fault> {
        self.require_unseeded_with(&mut seed_directory_names)
    }

    fn require_unseeded_with(
        &self,
        list: &mut dyn FnMut(BorrowedFd<'_>) -> nix::Result<BTreeSet<Vec<u8>>>,
    ) -> Result<(), Fault> {
        if !self.read_index()?.is_empty() {
            return Err(Fault::new(
                PersistenceCase::SeedExists,
                "chain already seeded",
            ));
        }
        // ADMIN-SEED must establish a new chain, not infer one from a missing
        // index. Spec 4's absent-manifest refusal and PERSIST-RECOVERY's manual
        // repair boundary also cover surviving artifacts and publication guards.
        // A part is not a publication and grants no load or recovery authority.
        let entries = list(self.directory.as_fd())
            .map_err(|error| layout(format!("list pre-seed repository: {error}")))?;
        for name in entries {
            let artifact =
                std::str::from_utf8(&name).is_ok_and(|name| ArtifactId::parse(name).is_ok());
            let guard = name.starts_with(b".")
                && (name.ends_with(b".pending") || name.ends_with(b".pending.broken"));
            let legacy = name.ends_with(crate::save_points::SUFFIX.as_bytes());
            if artifact || guard || legacy {
                return Err(layout(format!(
                    "cannot establish an unseeded chain: {} stands without its index",
                    String::from_utf8_lossy(&name),
                )));
            }
        }
        Ok(())
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

    /// Append an explicit selection of a verified indexed artifact. The caller
    /// owns root authorization and the invocation/run locks of an unloaded agent.
    /// Checkpoints judge whether that caller may continue, not remote liveness.
    /// Even a selection of the current latest appends the next ordinal.
    pub(crate) fn restore(
        &self,
        selector: &ArtifactId,
        scope: &LoadScope<'_>,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<VerifiedArtifact, RestoreFailure> {
        let before_append = |fault| RestoreFailure {
            fault,
            selection_may_be_appended: false,
        };
        checkpoint(PersistencePhase::Preparing)
            .map_err(|detail| before_append(restore_stopped(detail)))?;
        let (index, prefix) = self.read_index_snapshot().map_err(before_append)?;
        let entry = index
            .iter()
            .rev()
            .find(|entry| entry.artifact_id == *selector)
            .ok_or_else(|| {
                before_append(layout(format!("artifact {selector} has no index entry")))
            })?;
        let mut verified = self.verify(entry.clone(), scope).map_err(before_append)?;
        checkpoint(PersistencePhase::Preparing)
            .map_err(|detail| before_append(restore_stopped(detail)))?;
        let mut selection = verified.entry.clone();
        // Finding the selected entry above proves that this snapshot is nonempty.
        selection.ordinal = next_selection_ordinal(index.last().expect("indexed artifact").ordinal)
            .map_err(before_append)?;
        selection.arrival = Arrival::Selected;
        let record = selection
            .to_json()
            .map_err(|error| before_append(layout(format!("invalid selection: {error}"))))?;
        checkpoint(PersistencePhase::Publishing)
            .map_err(|detail| before_append(restore_stopped(detail)))?;
        let after_attempt = |fault| RestoreFailure {
            fault,
            selection_may_be_appended: true,
        };
        super::fs_ops::append_line(
            self.directory.as_fd(),
            MANIFEST,
            self.root_owner,
            &prefix,
            &record,
        )
        .map_err(|error| {
            after_attempt(Fault::new(
                PersistenceCase::PublicationFailed,
                format!("restore selection append failed: {error}"),
            ))
        })?;
        checkpoint(PersistencePhase::Reporting)
            .map_err(|detail| after_attempt(restore_stopped(detail)))?;
        verified.entry = selection;
        Ok(verified)
    }

    /// Empty is a valid pre-seed repository, never a valid serving load. A torn
    /// newest line is an error, including when every preceding line was valid.
    pub(super) fn read_index(&self) -> Result<Vec<StateIndexEntry>, Fault> {
        self.read_index_snapshot().map(|(index, _)| index)
    }

    /// Read and validate the whole index and retain the exact bytes from that
    /// same judged descriptor. An append must match this prefix without repair.
    pub(super) fn read_index_snapshot(&self) -> Result<(Vec<StateIndexEntry>, Vec<u8>), Fault> {
        let Some(mut file) = open_optional(self.directory.as_fd(), MANIFEST)? else {
            return Ok((Vec::new(), Vec::new()));
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
        Ok((index, bytes))
    }

    pub(super) fn verify(
        &self,
        entry: StateIndexEntry,
        scope: &LoadScope<'_>,
    ) -> Result<VerifiedArtifact, Fault> {
        self.refuse_guard(&entry.artifact_id)?;
        self.verify_unindexed_publication(entry, scope)
    }

    /// Publisher readback only. This judges exact artifact files, custody,
    /// hashes and declaration while its publication guard may still stand.
    /// It checks neither index membership nor guard absence and grants no load
    /// authority. Load and restore must go through the guarded verifier above.
    pub(in crate::save_points::serving) fn verify_unindexed_publication(
        &self,
        entry: StateIndexEntry,
        scope: &LoadScope<'_>,
    ) -> Result<VerifiedArtifact, Fault> {
        entry
            .validate()
            .map_err(|error| layout(format!("invalid expected publication entry: {error}")))?;
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
        let trace_path = super::trace_path(&agent.trace_sink);
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

fn next_selection_ordinal(last: DecimalU64) -> Result<DecimalU64, Fault> {
    last.0
        .checked_add(1)
        .map(DecimalU64)
        .ok_or_else(|| layout("restore selection ordinal exhausted"))
}

fn restore_stopped(detail: String) -> Fault {
    Fault::new(
        PersistenceCase::PublicationFailed,
        format!("restore checkpoint stopped the act: {detail}"),
    )
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

pub(super) fn judge_file(file: &File, owner: Owner, mode: u32, name: &str) -> Result<(), Fault> {
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

pub(super) fn judge_acl(descriptor: BorrowedFd<'_>, name: &str) -> Result<(), Fault> {
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
    let descriptor = directory
        .try_clone_to_owned()
        .map_err(|e| layout(format!("dup directory: {e}")))?;
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

fn seed_directory_names(directory: BorrowedFd<'_>) -> nix::Result<BTreeSet<Vec<u8>>> {
    // A new description is required: dup would share a previous listing's
    // offset and could make the second seed check observe an empty directory.
    let descriptor = nix::fcntl::openat(
        directory,
        ".",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    let mut directory = nix::dir::Dir::from_fd(descriptor)?;
    let mut names = BTreeSet::new();
    for entry in directory.iter() {
        let entry = entry?;
        let name = entry.file_name().to_bytes();
        if name != b"." && name != b".." {
            names.insert(name.to_vec());
        }
    }
    Ok(names)
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
