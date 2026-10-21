//! Behavioral publication tests using real descriptor-backed repositories.

use super::*;
use crate::save_points::open_directory;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use weaver_types::state_flow::SourceBlockId;
use weaver_types::{TraceSink, parse_serving_draft};

mod failures;

const SEED_BYTES: &[u8] = b"the member, not admin, judges this image";
const CAPTURE_BYTES: &[u8] = b"a complete captured state image";

#[test]
fn seed_publication_itself_refuses_prior_artifact_evidence_without_an_index() {
    let fixture = Fixture::new();
    let orphan = format!("{:020}-{}", 1, "d".repeat(64));
    fs::create_dir(fixture.root.join(&orphan)).unwrap();
    let mut repository = fixture.repository();
    let created = fixture.seed_image();
    let room = open_directory(&fixture.room).unwrap();
    let declaration = declaration::render(&declaration::tests::fixture()).unwrap();
    let result = Publication::seed(
        &mut repository,
        &fixture.scope(),
        parse_serving_draft(&declaration).unwrap(),
        SeedImage {
            expected_seed_id: &created.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: fixture.owner.uid,
        },
        &mut alive,
    );
    match result {
        Err(failure) => {
            assert_eq!(failure.case, PersistenceCase::UnsupportedLayout);
            assert!(failure.detail.contains(&orphan));
        }
        Ok(_) => panic!("publisher bypassed the seed-only prior-chain check"),
    }
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
    assert!(fixture.root.join(&orphan).is_dir());
    assert!(!fixture.root.join(MANIFEST).exists());
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    room: PathBuf,
    owner: Owner,
    session: SessionId,
    trace_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "publication-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("save-points");
        let room = base.join("member-room");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir(&room).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();
        fs::set_permissions(&room, fs::Permissions::from_mode(0o700)).unwrap();
        let declaration = declaration::tests::fixture();
        let trace_path = match &declaration.agent().trace_sink {
            TraceSink::File { path, .. }
            | TraceSink::Pipe { path, .. }
            | TraceSink::Socket { path } => path.clone(),
        };
        Self {
            base,
            root,
            room,
            owner: Owner {
                uid: nix::unistd::geteuid().as_raw(),
                gid: nix::unistd::getegid().as_raw(),
            },
            session: declaration.agent().session.clone(),
            trace_path,
        }
    }

    fn repository(&self) -> Repository {
        Repository::from_judged_directory(
            open_directory(&self.root).unwrap(),
            self.owner,
            self.owner.gid,
        )
        .unwrap()
    }

    fn scope(&self) -> LoadScope<'_> {
        LoadScope {
            session: &self.session,
            trace_path: &self.trace_path,
        }
    }

    fn write_room(&self, name: &str, bytes: &[u8]) {
        let path = self.room.join(name);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn seed_image(&self) -> SeedCreated {
        let seed_id = "1".repeat(32);
        let room_name = format!("seed-{seed_id}.sqlite");
        self.write_room(&room_name, SEED_BYTES);
        SeedCreated {
            schema_id: codec::SCHEMA_ID.into(),
            seed_id,
            room_name,
            size_bytes: DecimalU64(SEED_BYTES.len() as u64),
        }
    }

    fn seed<'a>(&self, repo: &'a mut Repository) -> Publication<'a> {
        let created = self.seed_image();
        let room = open_directory(&self.room).unwrap();
        Publication::seed(
            repo,
            &self.scope(),
            draft(),
            SeedImage {
                expected_seed_id: &created.seed_id,
                created: &created,
                room: room.as_fd(),
                member_uid: self.owner.uid,
            },
            &mut alive,
        )
        .unwrap()
    }

    fn seed_complete(&self, repo: &mut Repository) -> StartIdentity {
        let mut publication = self.seed(repo);
        publication.seed_cleanup_complete(&mut alive).unwrap();
        assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Renamed);
        assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Verified);
        let Progress::Complete(identity) = publication.advance(&mut alive).unwrap() else {
            panic!("seed requires completed publication");
        };
        identity
    }

    fn capture<'a>(
        &self,
        repo: &'a mut Repository,
        loaded: &StartIdentity,
        capture: u64,
        cause: PersistenceCause,
    ) -> Publication<'a> {
        let name = format!("capture-{capture}.sqlite.part");
        self.write_room(&name, CAPTURE_BYTES);
        let room = open_directory(&self.room).unwrap();
        Publication::serving(
            repo,
            &self.scope(),
            ServingImage {
                loaded,
                run: run(),
                cause,
                capture_id: DecimalU64(capture),
                room_name: &name,
                size_bytes: DecimalU64(CAPTURE_BYTES.len() as u64),
                position: Some(position(capture)),
                room: room.as_fd(),
                member_uid: self.owner.uid,
            },
            &mut alive,
        )
        .unwrap()
    }

    fn index_bytes(&self) -> Vec<u8> {
        match fs::read(self.root.join(MANIFEST)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("{error}"),
        }
    }

    fn index(&self) -> Vec<StateIndexEntry> {
        codec::decode_index(&self.index_bytes()).unwrap()
    }

    fn names(&self) -> Vec<String> {
        let mut names = fs::read_dir(&self.root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn assert_guarded(&self, identity: &StartIdentity, state: PublicationState) {
        let bytes = fs::read(self.root.join(format!(".{}.pending", identity.artifact_id))).unwrap();
        let guard = PublicationGuard::from_json(&bytes).unwrap();
        assert_eq!(guard.artifact_id.to_string(), identity.artifact_id);
        assert_eq!(guard.state, state);
        let id = ArtifactId::parse(&identity.artifact_id).unwrap();
        assert!(self.repository().load(Some(&id), &self.scope()).is_err());
        if codec::decode_index(&self.index_bytes())
            .is_ok_and(|index| index.last().is_some_and(|entry| entry.artifact_id == id))
        {
            assert!(self.repository().load(None, &self.scope()).is_err());
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn alive(_: PersistencePhase) -> Result<(), String> {
    Ok(())
}

fn run() -> RunId {
    RunId("loaded-serving-run".into())
}

fn draft() -> ServingDraft {
    parse_serving_draft(&declaration::render(&declaration::tests::fixture()).unwrap()).unwrap()
}

fn position(capture: u64) -> SourcePosition {
    SourcePosition {
        block_id: SourceBlockId {
            run: run().0,
            sequence: DecimalU64(capture),
            block: DecimalU64(1),
        },
        turn: None,
    }
}

fn prepare(publication: &mut Publication<'_>, capture: u64) -> StartIdentity {
    publication
        .image_finished(
            &run(),
            DecimalU64(capture),
            &format!("capture-{capture}.sqlite"),
            &mut alive,
        )
        .unwrap()
}

fn publish(publication: &mut Publication<'_>, capture: u64) -> StartIdentity {
    let identity = prepare(publication, capture);
    publication
        .commit(&run(), DecimalU64(capture), &mut alive)
        .unwrap();
    assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Renamed);
    assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Verified);
    assert_eq!(
        publication.advance(&mut alive).unwrap(),
        Progress::Published(identity.clone())
    );
    identity
}

fn report(identity: &StartIdentity, capture: u64) -> SavePointReport {
    SavePointReport {
        start: identity.clone(),
        capture_id: DecimalU64(capture),
        event_run: run(),
        event_sequence: DecimalU64(81),
    }
}

#[test]
fn seed_requires_cleanup_then_publishes_exact_bytes_and_identity() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let mut publication = fixture.seed(&mut repo);
    assert_eq!(publication.dump_sha256(), &hash(SEED_BYTES));
    assert!(fixture.index().is_empty());
    assert!(fixture.repository().load(None, &fixture.scope()).is_err());
    assert_eq!(fixture.names().len(), 1);
    publication.seed_cleanup_complete(&mut alive).unwrap();
    assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Renamed);
    assert!(fixture.index().is_empty());
    assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Verified);
    let Progress::Complete(identity) = publication.advance(&mut alive).unwrap() else {
        panic!("seed did not complete");
    };
    drop(publication);
    let loaded = repo.load(None, &fixture.scope()).unwrap();
    assert_eq!(loaded.identity(), identity);
    assert_eq!(identity.dump_sha256, hash(SEED_BYTES));
    assert_eq!(identity.parent_artifact_id, None);
    assert_eq!(identity.position, None);
    assert_eq!(fixture.index()[0].cause, PersistenceCause::Seed);
    assert_eq!(
        fs::read(
            fixture
                .root
                .join(&identity.artifact_id)
                .join("state.sqlite")
        )
        .unwrap(),
        SEED_BYTES
    );
    assert_eq!(hash(loaded.configuration_bytes()), identity.config_sha256);
    assert!(fixture.names().iter().all(|name| !name.starts_with('.')));
}

#[test]
fn seed_cancellation_and_premature_publication_remove_only_unlisted_work() {
    for premature in [false, true] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let mut publication = fixture.seed(&mut repo);
        let failure = if premature {
            publication.advance(&mut alive).unwrap_err()
        } else {
            publication.cancel("seed member did not exit and empty its cgroup")
        };
        assert!(failure.published.is_none());
        assert!(failure.artifacts.is_empty());
        assert!(fixture.names().is_empty());
        assert!(
            fixture
                .room
                .join(format!("seed-{}.sqlite", "1".repeat(32)))
                .exists()
        );
        assert!(publication.seed_cleanup_complete(&mut alive).is_err());
        assert!(publication.advance(&mut alive).is_err());
        assert!(fixture.index().is_empty());
    }
}

#[test]
fn second_seed_refuses_without_touching_the_chain() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    fixture.seed_complete(&mut repo);
    let previous = fixture.index_bytes();
    let names = fixture.names();
    let created = fixture.seed_image();
    let room = open_directory(&fixture.room).unwrap();
    let error = Publication::seed(
        &mut repo,
        &fixture.scope(),
        draft(),
        SeedImage {
            expected_seed_id: &created.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: fixture.owner.uid,
        },
        &mut alive,
    )
    .err()
    .unwrap();
    assert_eq!(error.case, PersistenceCase::SeedExists);
    assert_eq!(fixture.index_bytes(), previous);
    assert_eq!(fixture.names(), names);
}

#[test]
fn seed_refuses_uncorrelated_or_untrusted_room_images() {
    for case in 0..9 {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let mut created = fixture.seed_image();
        let expected_seed_id = created.seed_id.clone();
        let mut uid = fixture.owner.uid;
        match case {
            0 => created.seed_id = "2".repeat(32),
            1 => created.schema_id = "state-content/2".into(),
            2 => created.room_name.push_str(".part"),
            3 => created.room_name = "../seed.sqlite".into(),
            4 => created.size_bytes.0 += 1,
            5 => uid = uid.wrapping_add(1),
            6 => {
                let path = fixture.room.join(&created.room_name);
                fs::remove_file(&path).unwrap();
                let outside = fixture.base.join("outside-image");
                fs::write(&outside, SEED_BYTES).unwrap();
                symlink(outside, path).unwrap();
            }
            7 => {
                let path = fixture.room.join(&created.room_name);
                fs::remove_file(&path).unwrap();
                nix::unistd::mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
                created.size_bytes = DecimalU64(0);
            }
            8 => {
                File::options()
                    .write(true)
                    .open(fixture.room.join(&created.room_name))
                    .unwrap()
                    .set_len(SAVE_POINT_BOUND + 1)
                    .unwrap();
                created.size_bytes = DecimalU64(SAVE_POINT_BOUND + 1);
            }
            _ => unreachable!(),
        }
        let room = open_directory(&fixture.room).unwrap();
        let error = Publication::seed(
            &mut repo,
            &fixture.scope(),
            draft(),
            SeedImage {
                expected_seed_id: &expected_seed_id,
                created: &created,
                room: room.as_fd(),
                member_uid: uid,
            },
            &mut alive,
        )
        .err()
        .unwrap();
        assert!(error.published.is_none(), "case {case}");
        assert!(
            fixture.names().is_empty(),
            "case {case}: {:?}",
            fixture.names()
        );
    }
}

#[test]
fn rebind_preserves_image_and_position_while_binding_operational_changes() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let seed = fixture.seed_complete(&mut repo);
    let mut capture = fixture.capture(&mut repo, &seed, 1, PersistenceCause::Requested);
    let saved = publish(&mut capture, 1);
    capture
        .finish(FinalOutcome::SavePointTaken(&report(&saved, 1)), &mut alive)
        .unwrap();
    drop(capture);
    let text = declaration::render(&declaration::tests::fixture())
        .unwrap()
        .replace("max-result-bytes = \"18\"", "max-result-bytes = \"29\"");
    let new_draft = parse_serving_draft(&text).unwrap();
    let mut publication =
        Publication::rebind(&mut repo, &fixture.scope(), None, new_draft, &mut alive).unwrap();
    assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Renamed);
    assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Verified);
    let Progress::Complete(rebound) = publication.advance(&mut alive).unwrap() else {
        panic!("rebind incomplete")
    };
    drop(publication);
    assert_eq!(rebound.dump_sha256, saved.dump_sha256);
    assert_eq!(rebound.position, saved.position);
    assert_eq!(rebound.seed_id, saved.seed_id);
    assert_eq!(
        rebound.parent_artifact_id.as_deref(),
        Some(saved.artifact_id.as_str())
    );
    assert_ne!(rebound.config_sha256, saved.config_sha256);
    assert_eq!(
        repo.load(None, &fixture.scope()).unwrap().identity(),
        rebound
    );
    assert_eq!(fixture.index()[2].cause, PersistenceCause::Rebind);
    assert!(fixture.names().iter().all(|name| !name.starts_with('.')));
}

#[test]
fn rebind_rejects_changed_scheme_and_wrong_territory() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    fixture.seed_complete(&mut repo);
    let original = fixture.index_bytes();
    let text = declaration::render(&declaration::tests::fixture())
        .unwrap()
        .replace(&"b".repeat(64), &"d".repeat(64));
    let error = Publication::rebind(
        &mut repo,
        &fixture.scope(),
        None,
        parse_serving_draft(&text).unwrap(),
        &mut alive,
    )
    .err();
    assert!(
        error.is_some(),
        "changed scheme must refuse before publication"
    );
    let error = error.unwrap();
    assert_eq!(error.case, PersistenceCase::SchemeMismatch);
    for bad_session in [false, true] {
        let other = SessionId("different-territory".into());
        let scope = LoadScope {
            session: if bad_session {
                &other
            } else {
                &fixture.session
            },
            trace_path: if bad_session {
                &fixture.trace_path
            } else {
                Path::new("/another/trace")
            },
        };
        assert!(Publication::rebind(&mut repo, &scope, None, draft(), &mut alive).is_err());
    }
    assert_eq!(fixture.index_bytes(), original);
    assert_eq!(fixture.names().len(), 2);
}

#[test]
fn serving_guard_precedes_prepared_and_requires_commit_and_real_final_outcome() {
    for cause in [
        PersistenceCause::Requested,
        PersistenceCause::GracefulUnload,
        PersistenceCause::ForcedUnload,
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let seed = fixture.seed_complete(&mut repo);
        let mut publication = fixture.capture(&mut repo, &seed, 9, cause);
        assert_eq!(publication.dump_sha256(), &hash(CAPTURE_BYTES));
        assert_eq!(fixture.index().len(), 1);
        let identity = prepare(&mut publication, 9);
        fixture.assert_guarded(&identity, PublicationState::Pending);
        assert!(!fixture.root.join(&identity.artifact_id).exists());
        assert_eq!(fixture.index().len(), 1);
        publication
            .commit(&run(), DecimalU64(9), &mut alive)
            .unwrap();
        assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Renamed);
        assert_eq!(publication.advance(&mut alive).unwrap(), Progress::Verified);
        assert_eq!(
            publication.advance(&mut alive).unwrap(),
            Progress::Published(identity.clone())
        );
        fixture.assert_guarded(&identity, PublicationState::Pending);
        assert_eq!(fixture.index().len(), 2);
        let report = report(&identity, 9);
        let outcome = if cause == PersistenceCause::Requested {
            FinalOutcome::SavePointTaken(&report)
        } else {
            FinalOutcome::Left(&report)
        };
        assert_eq!(publication.finish(outcome, &mut alive).unwrap(), identity);
        drop(publication);
        assert_eq!(
            repo.load(None, &fixture.scope()).unwrap().identity(),
            identity
        );
        assert_eq!(fixture.index()[1].cause, cause);
        assert_eq!(
            identity.parent_artifact_id.as_deref(),
            Some(seed.artifact_id.as_str())
        );
        assert_eq!(identity.position, Some(position(9)));
        assert!(fixture.names().iter().all(|name| !name.starts_with('.')));
    }
}

#[test]
fn later_capture_in_one_run_keeps_the_loaded_source_as_parent() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut last = None;
    for capture in [1, 2] {
        let mut publication =
            fixture.capture(&mut repo, &loaded, capture, PersistenceCause::Requested);
        let identity = publish(&mut publication, capture);
        publication
            .finish(
                FinalOutcome::SavePointTaken(&report(&identity, capture)),
                &mut alive,
            )
            .unwrap();
        assert_eq!(
            identity.parent_artifact_id.as_deref(),
            Some(loaded.artifact_id.as_str())
        );
        assert_ne!(identity.artifact_id, loaded.artifact_id);
        last = Some(identity);
    }
    let entries = fixture.index();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1].parent_artifact_id, entries[2].parent_artifact_id);
    assert_eq!(
        repo.load(None, &fixture.scope()).unwrap().identity(),
        last.unwrap()
    );
}
