//! Descriptor-held publication under foundation Spec section 5.
//!
//! This is a filesystem primitive, not the lifecycle driver. The caller owns
//! root inventory, invocation/run locks, Observe/Beginning and exchange checks,
//! seed-member containment/cleanup, and live deadline/peer observations. Its
//! checkpoint must refuse observed failure or expiry. No serialized protocol,
//! caller-chosen path, new clock, or success inferred from file presence lives here.
//!
//! A failed broken-guard replacement can retain its .<id>.pending.broken
//! temporary file. A converted Leave can also retain .<id>.pending.cause.part
//! from finalizing its pending index entry. Failure evidence reports those files
//! and .<id>.pending without granting retry or load.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use nix::fcntl::{AtFlags, OFlag};
use nix::sys::stat::Mode;
use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::{
    Confirmation, DecimalU64, Digest, PersistenceCase, PersistenceCause, PersistencePhase,
    PublicationState, RetainedArtifact, RetainedArtifactKind, SavePointReport, SeedCreated,
    SourcePosition, StartIdentity,
};
use weaver_types::{RunId, ServingDeclaration, ServingDraft, SessionId};

use super::codec::{self, Arrival, ArtifactId, PublicationGuard, StateIndexEntry};
use super::declaration;
use super::fs_ops::{self, PartDirectory};
use super::repository::{LoadScope, Repository};
use crate::save_points::{MANIFEST, Owner, PUBLISH_RESERVE, SAVE_POINT_BOUND};

/// A failure describes confirmed extant evidence. It never grants retry or load.
#[derive(Clone, Debug)]
pub(crate) struct Failure {
    pub(crate) case: PersistenceCase,
    pub(crate) phase: PersistencePhase,
    pub(crate) published: Option<Box<StartIdentity>>,
    pub(crate) artifacts: Vec<RetainedArtifact>,
    pub(crate) detail: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} at {:?}: {}", self.case, self.phase, self.detail)
    }
}

impl std::error::Error for Failure {}

impl From<super::Fault> for Failure {
    fn from(fault: super::Fault) -> Self {
        Self::before_work(fault.case, fault.detail)
    }
}

impl Failure {
    fn before_work(case: PersistenceCase, detail: impl Into<String>) -> Self {
        Self {
            case,
            phase: PersistencePhase::Preparing,
            published: None,
            artifacts: Vec::new(),
            detail: detail.into(),
        }
    }
}

/// Already correlated SeedCreated and the held, judged member room.
pub(crate) struct SeedImage<'a> {
    pub(crate) expected_seed_id: &'a str,
    pub(crate) created: &'a SeedCreated,
    pub(crate) room: BorrowedFd<'a>,
    pub(crate) member_uid: u32,
}

/// Image fields after the caller checked Observe, Beginning, run/exchange,
/// capture identity and the unchanged windows. Parent is the loaded source,
/// which need not be the newest index entry after another save in that run.
pub(crate) struct ServingImage<'a> {
    pub(crate) loaded: &'a StartIdentity,
    pub(crate) run: RunId,
    pub(crate) cause: PersistenceCause,
    pub(crate) capture_id: DecimalU64,
    pub(crate) room_name: &'a str,
    pub(crate) size_bytes: DecimalU64,
    pub(crate) position: Option<SourcePosition>,
    pub(crate) room: BorrowedFd<'a>,
    pub(crate) member_uid: u32,
}

/// Only actual final outcomes can complete a serving publication. In particular,
/// CaptureComplete has no variant here. It only changes the caller's deadline set.
pub(crate) enum FinalOutcome<'a> {
    SavePointTaken(&'a SavePointReport),
    Left {
        report: &'a SavePointReport,
        forced: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    Renamed,
    Verified,
    /// Caller may send Published, but the pending guard still prevents loading.
    Published(StartIdentity),
    /// Seed or rebind has no worker outcome. All publication writes completed.
    Complete(StartIdentity),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Copied,
    Prepared,
    Authorized,
    Renamed,
    Verified,
    Published,
    Complete,
    Failed,
}

#[derive(Clone)]
enum Origin {
    Seed {
        seed_id: String,
    },
    Rebind {
        parent: ArtifactId,
        seed_id: String,
        position: Option<SourcePosition>,
    },
    Serving {
        parent: ArtifactId,
        seed_id: String,
        run: RunId,
        cause: PersistenceCause,
        capture_id: DecimalU64,
        position: Option<SourcePosition>,
    },
}

/// One publication under the caller's held locks. Each advance performs one
/// publication step, leaving a checkpoint for lifecycle service between steps.
/// Dropping this handle never deletes evidence or completes a save.
pub(crate) struct Publication<'repo> {
    repository: &'repo mut Repository,
    session: SessionId,
    trace_path: PathBuf,
    prefix: Vec<u8>,
    ordinal: DecimalU64,
    origin: Origin,
    declaration: ServingDeclaration,
    part: Option<PartDirectory>,
    work_name: String,
    dump_sha256: Option<Digest>,
    entry: Option<StateIndexEntry>,
    guard_name: Option<String>,
    guard_bytes: Option<Vec<u8>>,
    owns_guard: bool,
    guard_durable: bool,
    known_files: BTreeMap<&'static str, KnownFile>,
    indexed: bool,
    stage: Stage,
    phase: PersistencePhase,
    failure: Option<Failure>,
}

impl<'repo> Publication<'repo> {
    /// Copy while the seed channel is still open. The caller must already have
    /// verified its certificate, unloaded state and isolated seed process.
    pub(crate) fn seed(
        repository: &'repo mut Repository,
        scope: &LoadScope<'_>,
        draft: ServingDraft,
        image: SeedImage<'_>,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<Self, Failure> {
        precheck(checkpoint, PersistencePhase::Preparing)?;
        repository.require_unseeded()?;
        let (index, prefix) = repository.read_index_snapshot()?;
        precheck(checkpoint, PersistencePhase::Preparing)?;
        if !index.is_empty() {
            return Err(Failure::before_work(
                PersistenceCase::SeedExists,
                "chain already seeded",
            ));
        }
        if !valid_seed_id(image.expected_seed_id)
            || image.created.seed_id != image.expected_seed_id
            || image.created.schema_id != codec::SCHEMA_ID
            || image.created.room_name != format!("seed-{}.sqlite", image.expected_seed_id)
        {
            return Err(Failure::before_work(
                PersistenceCase::SeedFailed,
                "uncorrelated seed image",
            ));
        }
        let declaration = draft.bind_start(zero_digest());
        let mut publication = Self::new(
            repository,
            scope,
            prefix,
            DecimalU64(1),
            Origin::Seed {
                seed_id: image.expected_seed_id.into(),
            },
            declaration,
        )?;
        publication.copy_room(
            image.room,
            &image.created.room_name,
            image.member_uid,
            image.created.size_bytes.0,
            checkpoint,
        )?;
        Ok(publication)
    }

    /// Successful reaping and empty/removal of the seed cgroup are an explicit
    /// caller precondition. A cleanup failure must call cancel, not this method.
    pub(crate) fn seed_cleanup_complete(
        &mut self,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<(), Failure> {
        if self.stage != Stage::Copied || !matches!(self.origin, Origin::Seed { .. }) {
            return Err(self.fail("seed cleanup completion in the wrong phase"));
        }
        self.prepare(checkpoint)?;
        self.stage = Stage::Authorized;
        Ok(())
    }

    /// Bind an unchanged verified image to an operational draft. Locks and the
    /// unloaded-agent judgment remain the caller's. N6 selects the source.
    pub(crate) fn rebind(
        repository: &'repo mut Repository,
        scope: &LoadScope<'_>,
        selector: Option<&ArtifactId>,
        draft: ServingDraft,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<Self, Failure> {
        precheck(checkpoint, PersistencePhase::Preparing)?;
        let parent = repository.load(selector, scope)?;
        precheck(checkpoint, PersistencePhase::Preparing)?;
        let old_scheme = &parent.declaration().state_management().flow.scheme;
        let new_scheme = &draft.state_management().flow.scheme;
        if old_scheme.scheme_id != new_scheme.scheme_id
            || old_scheme.artifact_sha256 != new_scheme.artifact_sha256
        {
            return Err(Failure::before_work(
                PersistenceCase::SchemeMismatch,
                "rebind changes the scheme",
            ));
        }
        let (index, prefix) = repository.read_index_snapshot()?;
        let ordinal = next_ordinal(&index)?;
        let declaration = draft.bind_start(parent.identity().dump_sha256.clone());
        let origin = Origin::Rebind {
            parent: parent.entry().artifact_id.clone(),
            seed_id: parent.entry().seed_id.clone(),
            position: parent.entry().position.clone(),
        };
        let source = File::from(parent.dump().try_clone_to_owned().map_err(before_io)?);
        let expected_a = parent.entry().dump_sha256.clone();
        let size = source.metadata().map_err(before_io)?.len();
        let mut publication = Self::new(repository, scope, prefix, ordinal, origin, declaration)?;
        publication.copy(&source, size, checkpoint)?;
        if publication.dump_sha256.as_ref() != Some(&expected_a) {
            return Err(publication.fail("verified parent image changed during rebind"));
        }
        publication.prepare(checkpoint)?;
        publication.stage = Stage::Authorized;
        Ok(publication)
    }

    /// Copy an Image only. The A returned by dump_sha256 is ImageChecked's A.
    /// No configuration, Prepared, Commit, or successful save is inferred yet.
    pub(crate) fn serving(
        repository: &'repo mut Repository,
        scope: &LoadScope<'_>,
        image: ServingImage<'_>,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<Self, Failure> {
        if image.capture_id.0 == 0
            || image.run.0.is_empty()
            || !matches!(
                image.cause,
                PersistenceCause::Requested
                    | PersistenceCause::GracefulUnload
                    | PersistenceCause::ForcedUnload
            )
            || image.room_name != format!("capture-{}.sqlite.part", image.capture_id.0)
        {
            return Err(before_io("invalid serving image identity"));
        }
        let parent_id = ArtifactId::parse(&image.loaded.artifact_id).map_err(before_io)?;
        precheck(checkpoint, PersistencePhase::Hashing)?;
        let parent = repository.verify_indexed(&parent_id, scope)?;
        if parent.identity() != *image.loaded {
            return Err(before_io(
                "loaded facts differ from the verified source artifact",
            ));
        }
        precheck(checkpoint, PersistencePhase::Hashing)?;
        let (index, prefix) = repository.read_index_snapshot()?;
        let ordinal = next_ordinal(&index)?;
        let origin = Origin::Serving {
            parent: parent_id,
            seed_id: image.loaded.seed_id.clone(),
            run: image.run,
            cause: image.cause,
            capture_id: image.capture_id,
            position: image.position,
        };
        let mut publication = Self::new(
            repository,
            scope,
            prefix,
            ordinal,
            origin,
            parent.declaration().clone(),
        )?;
        publication.copy_room(
            image.room,
            image.room_name,
            image.member_uid,
            image.size_bytes.0,
            checkpoint,
        )?;
        Ok(publication)
    }

    pub(crate) fn dump_sha256(&self) -> &Digest {
        // Constructors return only after a complete copied and read-back image.
        self.dump_sha256
            .as_ref()
            .expect("constructed publication has an image")
    }

    /// Only the matching ImageFinished permits preparation and a pending guard.
    pub(crate) fn image_finished(
        &mut self,
        run: &RunId,
        capture_id: DecimalU64,
        room_name: &str,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<StartIdentity, Failure> {
        if self.stage != Stage::Copied
            || !self.matches_capture(run, capture_id)
            || room_name != format!("capture-{}.sqlite", capture_id.0)
        {
            return Err(self.fail("uncorrelated or out-of-phase ImageFinished"));
        }
        self.prepare(checkpoint)?;
        self.create_guard(checkpoint)?;
        self.stage = Stage::Prepared;
        self.phase = PersistencePhase::Confirming;
        Ok(self.identity())
    }

    pub(crate) fn commit(
        &mut self,
        run: &RunId,
        capture_id: DecimalU64,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<(), Failure> {
        if self.stage != Stage::Prepared || !self.matches_capture(run, capture_id) {
            return Err(self.fail("uncorrelated or out-of-phase Commit"));
        }
        self.check(PersistencePhase::Confirming, checkpoint)?;
        self.stage = Stage::Authorized;
        Ok(())
    }

    /// Filesystem work is deliberately split: rename, full read-back, index.
    /// No call starts another step after its post-operation checkpoint fails.
    pub(crate) fn advance(
        &mut self,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<Progress, Failure> {
        match self.stage {
            Stage::Authorized => {
                self.perform(PersistencePhase::Publishing, checkpoint, |this| {
                    let name = this.entry.as_ref().unwrap().artifact_id.to_string();
                    let result = this.part.as_mut().unwrap().rename_no_replace(&name);
                    // name() changes at rename, even when its following sync fails.
                    this.work_name = this.part.as_ref().unwrap().name().into();
                    result.map_err(|e| e.to_string())?;
                    this.stage = Stage::Renamed;
                    Ok(())
                })?;
                Ok(Progress::Renamed)
            }
            Stage::Renamed => {
                self.perform(PersistencePhase::Publishing, checkpoint, |this| {
                    let verified = this
                        .repository
                        .verify_unindexed_publication(
                            this.entry.clone().unwrap(),
                            &LoadScope {
                                session: &this.session,
                                trace_path: &this.trace_path,
                            },
                        )
                        .map_err(|e| e.to_string())?;
                    if verified.identity() != this.identity()
                        || verified.declaration() != &this.declaration
                    {
                        return Err("published read-back differs from prepared bytes".into());
                    }
                    this.stage = Stage::Verified;
                    Ok(())
                })?;
                Ok(Progress::Verified)
            }
            Stage::Verified => {
                self.perform(PersistencePhase::Publishing, checkpoint, |this| {
                    let record = this
                        .entry
                        .as_ref()
                        .unwrap()
                        .to_json()
                        .map_err(|e| e.to_string())?;
                    fs_ops::append_line(
                        this.repository.directory.as_fd(),
                        MANIFEST,
                        this.repository.root_owner,
                        &this.prefix,
                        &record,
                    )
                    .map_err(|e| e.to_string())?;
                    this.indexed = true;
                    this.stage = Stage::Published;
                    Ok(())
                })?;
                self.phase = PersistencePhase::Reporting;
                if matches!(self.origin, Origin::Serving { .. }) {
                    Ok(Progress::Published(self.identity()))
                } else {
                    self.stage = Stage::Complete;
                    Ok(Progress::Complete(self.identity()))
                }
            }
            _ => Err(self.fail("publication step without its required authority")),
        }
    }

    /// Caller supplies a real final lifecycle outcome, including all existing
    /// successful-Left duties, never an outcome synthesized from Published.
    pub(crate) fn finish(
        &mut self,
        outcome: FinalOutcome<'_>,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<StartIdentity, Failure> {
        let (report, final_cause) = match outcome {
            FinalOutcome::SavePointTaken(report)
                if matches!(
                    self.origin,
                    Origin::Serving {
                        cause: PersistenceCause::Requested,
                        ..
                    }
                ) =>
            {
                (report, PersistenceCause::Requested)
            }
            FinalOutcome::Left { report, forced }
                if matches!(
                    self.origin,
                    Origin::Serving {
                        cause: PersistenceCause::GracefulUnload | PersistenceCause::ForcedUnload,
                        ..
                    }
                ) =>
            {
                let cause = if forced {
                    PersistenceCause::ForcedUnload
                } else {
                    PersistenceCause::GracefulUnload
                };
                if matches!(
                    self.origin,
                    Origin::Serving {
                        cause: PersistenceCause::ForcedUnload,
                        ..
                    }
                ) && !forced
                {
                    return Err(self.fail("forced Leave answered as graceful"));
                }
                (report, cause)
            }
            _ => return Err(self.fail("wrong final outcome for the save cause")),
        };
        if self.stage != Stage::Published
            || !self.matches_capture(&report.event_run, report.capture_id)
            || report.start != self.identity()
        {
            return Err(self.fail("final outcome disagrees with the indexed capture"));
        }
        self.perform(PersistencePhase::Reporting, checkpoint, |this| {
            this.finalize_cause(final_cause)?;
            let name = this.guard_name.as_ref().unwrap();
            let expected = this.guard_bytes.as_ref().ok_or("no prepared guard bytes")?;
            fs_ops::remove_file(
                this.repository.directory.as_fd(),
                name,
                expected,
                this.repository.root_owner,
                0o600,
            )
            .map_err(|e| e.to_string())
        })?;
        self.owns_guard = false;
        self.stage = Stage::Complete;
        Ok(self.identity())
    }

    /// Only this still-guarded publication's exact last entry can change. A
    /// graceful Leave can become forced until S7; Left supplies the settled
    /// outcome. Binding bytes, ordinals and all earlier index bytes stay fixed.
    fn finalize_cause(&mut self, cause: PersistenceCause) -> Result<(), String> {
        let entry = self.entry.as_ref().ok_or("no indexed entry")?;
        if entry.cause == cause {
            return Ok(());
        }
        if entry.cause != PersistenceCause::GracefulUnload
            || cause != PersistenceCause::ForcedUnload
            || !self.indexed
            || !self.owns_guard
            || !self.guard_durable
            || self.stage != Stage::Published
            || self.read_guard()? != *self.guard_bytes.as_ref().ok_or("no prepared guard bytes")?
        {
            return Err("final save cause requires the owned pending publication".into());
        }
        let mut expected = self.prefix.clone();
        expected.extend(entry.to_json().map_err(|e| e.to_string())?);
        expected.push(b'\n');
        let mut final_entry = entry.clone();
        final_entry.cause = cause;
        let mut replacement = self.prefix.clone();
        replacement.extend(final_entry.to_json().map_err(|e| e.to_string())?);
        replacement.push(b'\n');
        fs_ops::replace_file(
            self.repository.directory.as_fd(),
            MANIFEST,
            &self.cause_part_name(),
            &expected,
            &replacement,
            self.repository.root_owner,
            0o644,
        )
        .map_err(|e| e.to_string())?;
        self.entry = Some(final_entry);
        Ok(())
    }

    fn cause_part_name(&self) -> String {
        format!(
            "{}.cause.part",
            self.guard_name.as_ref().expect("owned serving guard")
        )
    }

    /// Explicit cancellation before a guard exists removes owned unlisted work.
    /// In particular the seed driver must call this if member/cgroup cleanup
    /// failed. After a guard exists, failure retains guarded evidence for repair.
    pub(crate) fn cancel(&mut self, detail: impl Into<String>) -> Failure {
        self.fail(detail)
    }

    /// CaptureRefused proves that this original Leave cannot Commit. Remove
    /// only its unlisted part and exact owned guard, retaining any cleanup
    /// failure instead of claiming a normal refused-capture conclusion.
    pub(crate) fn cancel_unconfirmed_leave(
        &mut self,
        run: &RunId,
        capture_id: DecimalU64,
    ) -> Result<(), Failure> {
        let before_commit = matches!(self.stage, Stage::Copied | Stage::Prepared)
            || (self.stage == Stage::Failed
                && matches!(
                    self.phase,
                    PersistencePhase::Hashing | PersistencePhase::Preparing
                ));
        if !before_commit
            || self.indexed
            || !self.matches_capture(run, capture_id)
            || !matches!(
                self.origin,
                Origin::Serving {
                    cause: PersistenceCause::GracefulUnload | PersistenceCause::ForcedUnload,
                    ..
                }
            )
        {
            return Err(self.fail("capture refusal cannot cancel this publication"));
        }
        let prior = self.failure.take();
        let result = (|| -> Result<(), String> {
            let guard = if self.owns_guard {
                let actual = self.read_guard()?;
                let broken = self
                    .guard(PublicationState::Broken)
                    .and_then(|guard| guard.to_json().map_err(before_io))
                    .map_err(|error| error.to_string())?;
                if self.guard_bytes.as_ref() != Some(&actual) && actual != broken {
                    return Err("cancelled capture's guard changed".into());
                }
                Some(actual)
            } else {
                None
            };
            if let Some(part) = &self.part {
                if !part.name().ends_with(".part") {
                    return Err("cancelled capture is no longer an unlisted part".into());
                }
                match nix::sys::stat::fstatat(
                    self.repository.directory.as_fd(),
                    part.name(),
                    AtFlags::AT_SYMLINK_NOFOLLOW,
                ) {
                    Err(nix::errno::Errno::ENOENT) => (),
                    Err(error) => return Err(error.to_string()),
                    Ok(_) => part.remove().map_err(|error| error.to_string())?,
                }
            }
            if let Some(bytes) = guard {
                fs_ops::remove_file(
                    self.repository.directory.as_fd(),
                    self.guard_name.as_ref().unwrap(),
                    &bytes,
                    self.repository.root_owner,
                    0o600,
                )
                .map_err(|error| error.to_string())?;
            }
            Ok(())
        })();
        if let Err(detail) = result {
            let detail = prior.map_or_else(
                || detail.clone(),
                |prior| format!("{}; capture refusal cleanup: {detail}", prior.detail),
            );
            return Err(self.fail(detail));
        }
        self.part = None;
        self.owns_guard = false;
        self.guard_durable = false;
        self.stage = Stage::Failed;
        let mut retained = prior.unwrap_or_else(|| before_io("worker completed capture refusal"));
        retained.phase = self.phase;
        retained.artifacts.clear();
        self.failure = Some(retained);
        Ok(())
    }

    /// An observed peer failure or expiry permanently withdraws future authority.
    pub(crate) fn failed(&mut self, detail: impl Into<String>) -> Failure {
        self.fail(detail)
    }

    fn new(
        repository: &'repo mut Repository,
        scope: &LoadScope<'_>,
        prefix: Vec<u8>,
        ordinal: DecimalU64,
        origin: Origin,
        declaration: ServingDeclaration,
    ) -> Result<Self, Failure> {
        check_scope(&declaration, scope)?;
        Ok(Self {
            repository,
            session: scope.session.clone(),
            trace_path: scope.trace_path.into(),
            prefix,
            ordinal,
            origin,
            declaration,
            part: None,
            work_name: part_name()?,
            dump_sha256: None,
            entry: None,
            guard_name: None,
            guard_bytes: None,
            owns_guard: false,
            guard_durable: false,
            known_files: BTreeMap::new(),
            indexed: false,
            stage: Stage::Copied,
            phase: PersistencePhase::Hashing,
            failure: None,
        })
    }

    fn copy_room(
        &mut self,
        room: BorrowedFd<'_>,
        name: &str,
        member_uid: u32,
        size: u64,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<(), Failure> {
        let source = self.perform(PersistencePhase::Hashing, checkpoint, |_| {
            let fd = nix::fcntl::openat(
                room,
                name,
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?;
            let file = File::from(fd);
            let meta = file.metadata().map_err(|e| e.to_string())?;
            if !meta.is_file()
                || meta.uid() != member_uid
                || meta.len() != size
                || size > SAVE_POINT_BOUND
            {
                return Err("room image is not the reported regular member-owned file".into());
            }
            Ok(file)
        })?;
        self.copy(&source, size, checkpoint)
    }

    fn copy(
        &mut self,
        source: &File,
        size: u64,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<(), Failure> {
        self.perform(PersistencePhase::Hashing, checkpoint, |this| {
            let needed = size
                .checked_add(PUBLISH_RESERVE)
                .ok_or("required space overflows")?;
            if size > SAVE_POINT_BOUND
                || fs_ops::available_bytes(this.repository.directory.as_fd())
                    .map_err(|e| e.to_string())?
                    < needed
            {
                return Err("insufficient capacity for the image and publication reserve".into());
            }
            this.part = Some(
                fs_ops::create_part(
                    this.repository.directory.as_fd(),
                    &this.work_name,
                    this.repository.root_owner,
                )
                .map_err(|e| e.to_string())?,
            );
            Ok(())
        })?;
        self.perform(PersistencePhase::Hashing, checkpoint, |this| {
            let copied = fs_ops::copy_new(
                this.part.as_ref().unwrap().fd(),
                "state.sqlite",
                source,
                size,
                this.file_owner(),
                0o640,
            )
            .map_err(|e| e.to_string())?;
            this.remember("state.sqlite", &copied.file, copied.sha256.clone())?;
            this.dump_sha256 = Some(copied.sha256);
            Ok(())
        })
    }

    fn prepare(
        &mut self,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<(), Failure> {
        self.check(PersistencePhase::Preparing, checkpoint)?;
        self.declaration = self.declaration.with_start(self.dump_sha256().clone());
        let configuration =
            declaration::render(&self.declaration).map_err(|e| self.fail(e.to_string()))?;
        let b = hash(configuration.as_bytes());
        let id = ArtifactId::from_parts(self.ordinal, b.clone())
            .map_err(|e| self.fail(e.to_string()))?;
        let (seed_id, parent, position, cause, confirmation) = match &self.origin {
            Origin::Seed { seed_id } => (
                seed_id.clone(),
                None,
                None,
                PersistenceCause::Seed,
                Confirmation::Seed {
                    seed_id: seed_id.clone(),
                },
            ),
            Origin::Rebind {
                parent,
                seed_id,
                position,
            } => (
                seed_id.clone(),
                Some(parent.clone()),
                position.clone(),
                PersistenceCause::Rebind,
                Confirmation::Rebind {
                    parent_artifact_id: parent.to_string(),
                },
            ),
            Origin::Serving {
                parent,
                seed_id,
                run,
                cause,
                capture_id,
                position,
            } => (
                seed_id.clone(),
                Some(parent.clone()),
                position.clone(),
                *cause,
                Confirmation::Serving {
                    run_id: run.0.clone(),
                    capture_id: *capture_id,
                },
            ),
        };
        self.entry = Some(StateIndexEntry {
            ordinal: self.ordinal,
            artifact_id: id,
            dump_sha256: self.dump_sha256().clone(),
            config_sha256: b,
            seed_id,
            parent_artifact_id: parent,
            cause,
            position,
            confirmation,
            arrival: Arrival::Published,
        });
        let binding = self
            .entry
            .as_ref()
            .unwrap()
            .binding()
            .to_json()
            .map_err(|e| self.fail(e.to_string()))?;
        self.check(PersistencePhase::Preparing, checkpoint)?;
        for (name, bytes) in [
            ("agent.toml", configuration.as_bytes()),
            ("binding.json", binding.as_slice()),
        ] {
            self.perform(PersistencePhase::Preparing, checkpoint, |this| {
                let file = fs_ops::write_new(
                    this.part.as_ref().unwrap().fd(),
                    name,
                    bytes,
                    this.file_owner(),
                    0o640,
                )
                .map_err(|e| e.to_string())?;
                this.remember(name, &file, hash(bytes))?;
                Ok(())
            })?;
        }
        self.perform(PersistencePhase::Preparing, checkpoint, |this| {
            let owner = this.file_owner();
            this.part
                .as_mut()
                .unwrap()
                .finish_mode(owner)
                .map_err(|e| e.to_string())
        })
    }

    fn create_guard(
        &mut self,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<(), Failure> {
        let name = format!(".{}.pending", self.entry.as_ref().unwrap().artifact_id);
        let bytes = self
            .guard(PublicationState::Pending)?
            .to_json()
            .map_err(|e| self.fail(e.to_string()))?;
        self.guard_name = Some(name.clone());
        self.perform(PersistencePhase::Preparing, checkpoint, |this| {
            // Any partial newly-created guard is ours and remains a refusal.
            // EEXIST never permits overwriting a guard from another attempt.
            this.owns_guard = true;
            this.guard_bytes = Some(bytes.clone());
            if let Err(error) = fs_ops::write_new(
                this.repository.directory.as_fd(),
                &name,
                &bytes,
                this.repository.root_owner,
                0o600,
            ) {
                if error.raw_os_error() == Some(nix::libc::EEXIST) {
                    this.owns_guard = false;
                }
                return Err(error.to_string());
            }
            this.guard_durable = true;
            Ok(())
        })
    }

    fn guard(&self, state: PublicationState) -> Result<PublicationGuard, Failure> {
        let Origin::Serving { capture_id, .. } = self.origin else {
            return Err(before_io("non-serving publication has no guard"));
        };
        Ok(PublicationGuard {
            format: codec::GUARD_FORMAT.into(),
            artifact_id: self.entry.as_ref().unwrap().artifact_id.clone(),
            capture_id,
            state,
            phase: self.phase,
            locations: vec![self.work_name.clone()],
        })
    }

    fn read_guard(&self) -> Result<Vec<u8>, String> {
        let name = self.guard_name.as_ref().ok_or("no owned guard")?;
        let fd = nix::fcntl::openat(
            self.repository.directory.as_fd(),
            name.as_str(),
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?;
        let mut file = File::from(fd);
        super::repository::judge_file(&file, self.repository.root_owner, 0o600, name)
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }

    fn identity(&self) -> StartIdentity {
        let entry = self
            .entry
            .as_ref()
            .expect("prepared publication has an identity");
        StartIdentity {
            seed_id: entry.seed_id.clone(),
            artifact_id: entry.artifact_id.to_string(),
            dump_sha256: entry.dump_sha256.clone(),
            config_sha256: entry.config_sha256.clone(),
            parent_artifact_id: entry.parent_artifact_id.as_ref().map(ToString::to_string),
            position: entry.position.clone(),
        }
    }

    fn remember(&mut self, name: &'static str, file: &File, digest: Digest) -> Result<(), String> {
        let stat = nix::sys::stat::fstat(file.as_fd()).map_err(|e| e.to_string())?;
        self.known_files.insert(name, KnownFile { digest, stat });
        Ok(())
    }

    fn file_owner(&self) -> Owner {
        Owner {
            uid: self.repository.root_owner.uid,
            gid: self.repository.access_gid,
        }
    }

    fn matches_capture(&self, run: &RunId, capture: DecimalU64) -> bool {
        matches!(&self.origin, Origin::Serving { run: expected, capture_id, .. } if expected == run && *capture_id == capture)
    }

    fn check(
        &mut self,
        phase: PersistencePhase,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    ) -> Result<(), Failure> {
        self.phase = phase;
        checkpoint(phase).map_err(|detail| self.fail(detail))
    }

    fn perform<T>(
        &mut self,
        phase: PersistencePhase,
        checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
        operation: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, Failure> {
        self.check(phase, checkpoint)?;
        let result = operation(self).map_err(|detail| self.fail(detail))?;
        self.check(phase, checkpoint)?;
        Ok(result)
    }

    fn fail(&mut self, detail: impl Into<String>) -> Failure {
        if let Some(failure) = &self.failure {
            return failure.clone();
        }
        let mut detail = detail.into();
        if self.stage == Stage::Complete {
            // A later caller error cannot retroactively break a completed save.
            return Failure {
                case: PersistenceCase::PublicationFailed,
                phase: self.phase,
                published: Some(Box::new(self.identity())),
                artifacts: Vec::new(),
                detail,
            };
        }
        if !self.owns_guard
            && let Some(part) = &self.part
            && part.name().ends_with(".part")
            && let Err(error) = part.remove()
        {
            detail.push_str(&format!("; unlisted part cleanup failed: {error}"));
        }
        self.stage = Stage::Failed;
        if self.owns_guard {
            let replacement = self
                .guard(PublicationState::Broken)
                .and_then(|guard| guard.to_json().map_err(before_io));
            if let Ok(bytes) = replacement {
                let name = self.guard_name.as_ref().unwrap();
                let result = match self.read_guard() {
                    // A refused-capture cleanup may fail after the original
                    // failure already installed this exact Broken guard.
                    Ok(old) if old == bytes => {
                        fs_ops::sync_directory(self.repository.directory.as_fd())
                    }
                    Ok(old) => fs_ops::replace_file(
                        self.repository.directory.as_fd(),
                        name,
                        &format!("{name}.broken"),
                        if self.guard_durable {
                            self.guard_bytes.as_ref().unwrap()
                        } else {
                            &old
                        },
                        &bytes,
                        self.repository.root_owner,
                        0o600,
                    ),
                    Err(error) => {
                        // Final unlink may have succeeded while parent sync failed.
                        // Recreate a blocking guard, exclusively, or report the gap.
                        detail.push_str(&format!("; guard read: {error}"));
                        fs_ops::write_new(
                            self.repository.directory.as_fd(),
                            name,
                            &bytes,
                            self.repository.root_owner,
                            0o600,
                        )
                        .map(|_| ())
                    }
                };
                if let Err(error) = result {
                    detail.push_str(&format!("; broken guard update failed: {error}"));
                }
            }
        }
        let mut artifacts = Vec::new();
        let root = self.repository.directory.as_fd();
        if let Some(part) = &self.part {
            let held = nix::sys::stat::fstat(part.fd());
            match nix::sys::stat::fstatat(
                root,
                self.work_name.as_str(),
                AtFlags::AT_SYMLINK_NOFOLLOW,
            ) {
                Ok(named) => {
                    artifacts.push(RetainedArtifact {
                        location: format!("save-points/{}", self.work_name),
                        kind: RetainedArtifactKind::Part,
                        sha256: None,
                    });
                    match held {
                        Ok(held) if held.st_dev == named.st_dev && held.st_ino == named.st_ino => {
                            for (name, kind) in [
                                ("state.sqlite", RetainedArtifactKind::Dump),
                                ("agent.toml", RetainedArtifactKind::Configuration),
                                ("binding.json", RetainedArtifactKind::Binding),
                            ] {
                                probe(
                                    part.fd(),
                                    name,
                                    &format!("{}/{}", self.work_name, name),
                                    kind,
                                    self.known_files.get(name),
                                    &mut artifacts,
                                    &mut detail,
                                );
                            }
                        }
                        _ => detail.push_str("; evidence directory no longer names the held inode"),
                    }
                }
                Err(nix::errno::Errno::ENOENT) => {}
                Err(error) => {
                    detail.push_str(&format!("; evidence probe {}: {error}", self.work_name))
                }
            }
        } else {
            // mkdir may have succeeded before obtaining a held descriptor.
            probe(
                root,
                &self.work_name,
                &self.work_name,
                RetainedArtifactKind::Part,
                None,
                &mut artifacts,
                &mut detail,
            );
        }
        for name in self
            .guard_name
            .iter()
            .flat_map(|name| [name.clone(), format!("{name}.broken")])
        {
            probe(
                root,
                &name,
                &name,
                RetainedArtifactKind::Guard,
                None,
                &mut artifacts,
                &mut detail,
            );
        }
        if self.guard_name.is_some() {
            let name = self.cause_part_name();
            probe(
                root,
                &name,
                &name,
                RetainedArtifactKind::Part,
                None,
                &mut artifacts,
                &mut detail,
            );
        }
        let failure = Failure {
            case: PersistenceCase::PublicationFailed,
            phase: self.phase,
            published: self.indexed.then(|| Box::new(self.identity())),
            artifacts,
            detail,
        };
        self.failure = Some(failure.clone());
        failure
    }
}

fn next_ordinal(index: &[StateIndexEntry]) -> Result<DecimalU64, Failure> {
    index
        .last()
        .map_or(0, |entry| entry.ordinal.0)
        .checked_add(1)
        .map(DecimalU64)
        .ok_or_else(|| before_io("artifact index ordinal exhausted"))
}

fn precheck(
    checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
    phase: PersistencePhase,
) -> Result<(), Failure> {
    checkpoint(phase).map_err(|detail| Failure {
        phase,
        ..before_io(detail)
    })
}

fn before_io(error: impl std::fmt::Display) -> Failure {
    Failure::before_work(PersistenceCase::PublicationFailed, error.to_string())
}

fn check_scope(declaration: &ServingDeclaration, scope: &LoadScope<'_>) -> Result<(), Failure> {
    let path = super::trace_path(&declaration.agent().trace_sink);
    if declaration.agent().session != *scope.session || path != scope.trace_path {
        return Err(Failure::before_work(
            PersistenceCase::ConfigurationMismatch,
            "declaration differs from the judged territory",
        ));
    }
    Ok(())
}

fn valid_seed_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn part_name() -> Result<String, Failure> {
    let random = crate::random::random_hex(16).map_err(before_io)?;
    Ok(format!(".publication-{random}.part"))
}

fn hash(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes))).expect("SHA-256 renders as a digest")
}

fn zero_digest() -> Digest {
    Digest::new("0".repeat(64)).unwrap()
}

fn probe(
    directory: BorrowedFd<'_>,
    name: &str,
    location: &str,
    kind: RetainedArtifactKind,
    known: Option<&KnownFile>,
    artifacts: &mut Vec<RetainedArtifact>,
    detail: &mut String,
) {
    match nix::sys::stat::fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => artifacts.push(RetainedArtifact {
            location: format!("save-points/{location}"),
            kind,
            sha256: known
                .filter(|known| same_file(&known.stat, &stat))
                .map(|known| known.digest.clone()),
        }),
        Err(nix::errno::Errno::ENOENT) => {}
        Err(error) => detail.push_str(&format!("; evidence probe {location}: {error}")),
    }
}

/// Known digests describe completed writes only. Changed evidence is retained,
/// but its previous digest is not reported as the hash of the changed file.
struct KnownFile {
    digest: Digest,
    stat: nix::sys::stat::FileStat,
}

fn same_file(a: &nix::sys::stat::FileStat, b: &nix::sys::stat::FileStat) -> bool {
    a.st_dev == b.st_dev
        && a.st_ino == b.st_ino
        && a.st_size == b.st_size
        && a.st_mtime == b.st_mtime
        && a.st_mtime_nsec == b.st_mtime_nsec
        && a.st_ctime == b.st_ctime
        && a.st_ctime_nsec == b.st_ctime_nsec
}

#[cfg(test)]
mod tests;
