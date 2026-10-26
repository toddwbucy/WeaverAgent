use super::*;
use crate::save_points::open_directory;
use crate::save_points::serving::{codec, declaration};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use weaver_types::TraceSink;
use weaver_types::state_flow::{
    Confirmation, DecimalU64, PersistenceCause, PersistencePhase, PublicationState,
};

#[path = "restore_tests.rs"]
mod restore;

#[path = "seed_tests.rs"]
mod seed;

#[path = "loaded_tests.rs"]
mod loaded;

struct Fixture {
    path: PathBuf,
    owner: Owner,
    session: SessionId,
    trace_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "artifact-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        let owner = Owner {
            uid: nix::unistd::geteuid().as_raw(),
            gid: nix::unistd::getegid().as_raw(),
        };
        let declaration = declaration::tests::fixture();
        let trace_path = match &declaration.agent().trace_sink {
            TraceSink::File { path, .. }
            | TraceSink::Pipe { path, .. }
            | TraceSink::Socket { path } => path.clone(),
        };
        Self {
            path,
            owner,
            session: declaration.agent().session.clone(),
            trace_path,
        }
    }

    fn repository(&self) -> Repository {
        Repository::from_judged_directory(
            open_directory(&self.path).unwrap(),
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

    fn write(&self, path: impl AsRef<Path>, bytes: &[u8], mode: u32) {
        let path = self.path.join(path);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn seed(&self) -> StateIndexEntry {
        self.artifact(1, None, b"raw image: the member judges SQLite", None)
    }

    fn artifact(
        &self,
        ordinal: u64,
        parent: Option<&StateIndexEntry>,
        image: &[u8],
        config: Option<&str>,
    ) -> StateIndexEntry {
        let a = hash(image);
        let configuration = config.map(str::to_owned).unwrap_or_else(|| {
            declaration::render(&declaration::tests::fixture().with_start(a.clone())).unwrap()
        });
        let b = hash(configuration.as_bytes());
        let id = ArtifactId::from_parts(DecimalU64(ordinal), b.clone()).unwrap();
        let seed_id = "1".repeat(32);
        let (cause, confirmation) = if let Some(parent) = parent {
            (
                PersistenceCause::Rebind,
                Confirmation::Rebind {
                    parent_artifact_id: parent.artifact_id.to_string(),
                },
            )
        } else {
            (
                PersistenceCause::Seed,
                Confirmation::Seed {
                    seed_id: seed_id.clone(),
                },
            )
        };
        let entry = StateIndexEntry {
            ordinal: DecimalU64(ordinal),
            artifact_id: id.clone(),
            dump_sha256: a,
            config_sha256: b,
            seed_id,
            parent_artifact_id: parent.map(|e| e.artifact_id.clone()),
            cause,
            position: None,
            confirmation,
            arrival: Arrival::Published,
        };
        fs::create_dir(self.path.join(id.as_str())).unwrap();
        fs::set_permissions(
            self.path.join(id.as_str()),
            fs::Permissions::from_mode(0o750),
        )
        .unwrap();
        self.write(Path::new(id.as_str()).join(DUMP), image, 0o640);
        self.write(
            Path::new(id.as_str()).join(CONFIGURATION),
            configuration.as_bytes(),
            0o640,
        );
        self.write(
            Path::new(id.as_str()).join(BINDING),
            &entry.binding().to_json().unwrap(),
            0o640,
        );
        let manifest = self.path.join(MANIFEST);
        let mut bytes = fs::read(&manifest).unwrap_or_default();
        bytes.extend_from_slice(&entry.to_json().unwrap());
        bytes.push(b'\n');
        self.write(MANIFEST, &bytes, 0o644);
        entry
    }

    fn guard(&self, entry: &StateIndexEntry, state: PublicationState) {
        let guard = PublicationGuard {
            format: codec::GUARD_FORMAT.into(),
            artifact_id: entry.artifact_id.clone(),
            capture_id: DecimalU64(2),
            state,
            phase: PersistencePhase::Reporting,
            locations: vec![entry.artifact_id.to_string()],
        };
        self.write(
            format!(".{}.pending", entry.artifact_id),
            &guard.to_json().unwrap(),
            0o600,
        );
    }

    fn rewrite_index(&self, entries: &[StateIndexEntry]) {
        let mut bytes = Vec::new();
        for entry in entries {
            bytes.extend_from_slice(&entry.to_json().unwrap());
            bytes.push(b'\n');
        }
        self.write(MANIFEST, &bytes, 0o644);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn verified_read_holds_exact_copy_and_read_only_dump_at_zero() {
    let f = Fixture::new();
    let entry = f.seed();
    let repo = f.repository();
    let verified = repo.load(None, &f.scope()).unwrap();
    assert_eq!(verified.entry(), &entry);
    assert_eq!(
        verified.identity().artifact_id,
        entry.artifact_id.to_string()
    );
    assert_eq!(
        verified.declaration().state_management().start.dump_sha256,
        entry.dump_sha256
    );
    assert_eq!(hash(verified.configuration_bytes()), entry.config_sha256);
    let mut bytes = [0u8; 3];
    assert_eq!(nix::unistd::read(verified.dump(), &mut bytes).unwrap(), 3);
    assert_eq!(&bytes, b"raw");
    assert_eq!(
        nix::unistd::write(verified.dump(), b"change").unwrap_err(),
        nix::errno::Errno::EBADF
    );
    let flags = OFlag::from_bits_truncate(
        nix::fcntl::fcntl(verified.dump(), nix::fcntl::FcntlArg::F_GETFL).unwrap(),
    );
    assert_eq!(flags & OFlag::O_ACCMODE, OFlag::O_RDONLY);
    // No mutable authoring draft is opened by this primitive.
    f.write("agent.toml", b"invalid unrelated draft", 0o640);
    assert!(repo.load(None, &f.scope()).is_ok());
}

#[test]
fn no_index_and_empty_index_never_make_a_loadable_start() {
    let f = Fixture::new();
    assert!(f.repository().read_index().unwrap().is_empty());
    assert!(f.repository().load(None, &f.scope()).is_err());
    f.write(MANIFEST, b"", 0o644);
    assert!(f.repository().load(None, &f.scope()).is_err());
}

#[test]
fn malformed_latest_is_not_skipped_even_for_historical_verification() {
    for suffix in [b"{\"ordinal\":\"2\"}".as_slice(), b"{}\n", b"\n"] {
        let f = Fixture::new();
        let old = f.seed();
        let mut bytes = fs::read(f.path.join(MANIFEST)).unwrap();
        bytes.extend_from_slice(suffix);
        f.write(MANIFEST, &bytes, 0o644);
        assert!(f.repository().load(None, &f.scope()).is_err());
        assert!(
            f.repository()
                .verify_indexed(&old.artifact_id, &f.scope())
                .is_err()
        );
    }
}

#[test]
fn unindexed_directories_are_neither_loaded_nor_promoted() {
    let f = Fixture::new();
    let entry = f.seed();
    fs::remove_file(f.path.join(MANIFEST)).unwrap();
    assert!(
        f.repository()
            .verify_indexed(&entry.artifact_id, &f.scope())
            .is_err()
    );
    assert!(f.repository().load(None, &f.scope()).is_err());
    assert!(!f.path.join(MANIFEST).exists());
}

#[test]
fn load_selector_must_be_latest_but_historical_verification_does_not_select() {
    let f = Fixture::new();
    let first = f.seed();
    let second = f.artifact(
        2,
        Some(&first),
        b"raw image: the member judges SQLite",
        None,
    );
    let before = fs::read(f.path.join(MANIFEST)).unwrap();
    let fault = f
        .repository()
        .load(Some(&first.artifact_id), &f.scope())
        .unwrap_err();
    assert_eq!(fault.case, PersistenceCase::ConfigurationMismatch);
    assert!(fault.detail.contains(first.artifact_id.as_str()));
    assert!(fault.detail.contains(second.artifact_id.as_str()));
    assert_eq!(
        f.repository()
            .verify_indexed(&first.artifact_id, &f.scope())
            .unwrap()
            .entry(),
        &first
    );
    assert_eq!(fs::read(f.path.join(MANIFEST)).unwrap(), before);
    assert_eq!(
        f.repository()
            .load(Some(&second.artifact_id), &f.scope())
            .unwrap()
            .entry(),
        &second
    );
    let mut selected = first.clone();
    selected.ordinal = DecimalU64(3);
    selected.arrival = Arrival::Selected;
    f.rewrite_index(&[first, second, selected.clone()]);
    assert_eq!(
        f.repository().load(None, &f.scope()).unwrap().entry(),
        &selected
    );
}

#[test]
fn each_hash_agreement_is_required_independently() {
    let f = Fixture::new();
    let entry = f.seed();
    f.write(
        Path::new(entry.artifact_id.as_str()).join(DUMP),
        b"changed",
        0o640,
    );
    assert_eq!(
        f.repository().load(None, &f.scope()).unwrap_err().case,
        PersistenceCase::BindingMismatch
    );

    let f = Fixture::new();
    let entry = f.seed();
    let config_path = Path::new(entry.artifact_id.as_str()).join(CONFIGURATION);
    let mut config = fs::read(f.path.join(&config_path)).unwrap();
    config.extend_from_slice(b"\n# a different B\n");
    f.write(config_path, &config, 0o640);
    assert_eq!(
        f.repository().load(None, &f.scope()).unwrap_err().case,
        PersistenceCase::BindingMismatch
    );

    let f = Fixture::new();
    // A, B, the directory and binding agree. Only the A inside the copy is wrong.
    let config = declaration::render(&declaration::tests::fixture()).unwrap();
    f.artifact(1, None, b"a different raw image", Some(&config));
    assert_eq!(
        f.repository().load(None, &f.scope()).unwrap_err().case,
        PersistenceCase::BindingMismatch
    );
}

#[test]
fn binding_identity_must_match_index_even_when_both_are_well_formed() {
    let f = Fixture::new();
    let entry = f.seed();
    let mut binding = entry.binding();
    binding.seed_id = "2".repeat(32);
    binding.confirmation = Confirmation::Seed {
        seed_id: binding.seed_id.clone(),
    };
    f.write(
        Path::new(entry.artifact_id.as_str()).join(BINDING),
        &binding.to_json().unwrap(),
        0o640,
    );
    assert_eq!(
        f.repository().load(None, &f.scope()).unwrap_err().case,
        PersistenceCase::BindingMismatch
    );
}

#[test]
fn hash_correct_configuration_still_needs_real_parse_and_territory_match() {
    let f = Fixture::new();
    f.artifact(1, None, b"image", Some("spu-binary = '/caller/choice'\n"));
    assert_eq!(
        f.repository().load(None, &f.scope()).unwrap_err().case,
        PersistenceCase::ConfigurationMismatch
    );
    let f = Fixture::new();
    f.seed();
    let foreign = SessionId("another-agent".into());
    let scope = LoadScope {
        session: &foreign,
        trace_path: &f.trace_path,
    };
    assert_eq!(
        f.repository().load(None, &scope).unwrap_err().case,
        PersistenceCase::ConfigurationMismatch
    );
    let scope = LoadScope {
        session: &f.session,
        trace_path: Path::new("/wrong/trace"),
    };
    assert_eq!(
        f.repository().load(None, &scope).unwrap_err().case,
        PersistenceCase::ConfigurationMismatch
    );
}

#[test]
fn pending_or_broken_latest_refuses_without_fallback_but_old_explicit_read_works() {
    for state in [PublicationState::Pending, PublicationState::Broken] {
        let f = Fixture::new();
        let first = f.seed();
        let latest = f.artifact(
            2,
            Some(&first),
            b"raw image: the member judges SQLite",
            None,
        );
        f.guard(&latest, state);
        assert_eq!(
            f.repository().load(None, &f.scope()).unwrap_err().case,
            PersistenceCase::BrokenSavePoint
        );
        assert!(
            f.repository()
                .verify_indexed(&first.artifact_id, &f.scope())
                .is_ok()
        );
        assert!(
            f.repository()
                .verify_indexed(&latest.artifact_id, &f.scope())
                .is_err()
        );
    }
}

#[test]
fn malformed_guard_never_means_absent_guard() {
    let f = Fixture::new();
    let entry = f.seed();
    f.write(format!(".{}.pending", entry.artifact_id), b"{}", 0o600);
    assert!(f.repository().load(None, &f.scope()).is_err());
}

#[test]
fn artifact_has_exactly_three_files_including_non_utf8_entry_check() {
    for name in [
        std::ffi::OsString::from("fourth"),
        std::ffi::OsString::from_vec(vec![0xff]),
    ] {
        let f = Fixture::new();
        let entry = f.seed();
        f.write(
            Path::new(entry.artifact_id.as_str()).join(name),
            b"extra",
            0o640,
        );
        assert!(f.repository().load(None, &f.scope()).is_err());
    }
    for missing in [DUMP, CONFIGURATION, BINDING] {
        let f = Fixture::new();
        let entry = f.seed();
        fs::remove_file(f.path.join(entry.artifact_id.as_str()).join(missing)).unwrap();
        assert!(f.repository().load(None, &f.scope()).is_err());
    }
}

#[test]
fn file_symlinks_hardlinks_fifo_and_modes_refuse_before_reading() {
    for name in [DUMP, CONFIGURATION, BINDING] {
        for alteration in ["symlink", "hardlink", "mode", "fifo"] {
            let f = Fixture::new();
            let entry = f.seed();
            let path = f.path.join(entry.artifact_id.as_str()).join(name);
            match alteration {
                "symlink" => {
                    fs::rename(&path, f.path.join("outside")).unwrap();
                    symlink(f.path.join("outside"), &path).unwrap();
                }
                "hardlink" => fs::hard_link(&path, f.path.join("outside")).unwrap(),
                "mode" => fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap(),
                "fifo" => {
                    fs::remove_file(&path).unwrap();
                    nix::unistd::mkfifo(&path, Mode::from_bits_truncate(0o640)).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                f.repository().load(None, &f.scope()).is_err(),
                "{name}: {alteration}"
            );
        }
    }
}

#[test]
fn index_and_guard_custody_refuses_links_and_wrong_modes() {
    for kind in ["index", "guard"] {
        for alteration in ["symlink", "hardlink", "mode", "fifo"] {
            let f = Fixture::new();
            let entry = f.seed();
            let name = if kind == "index" {
                MANIFEST.to_string()
            } else {
                f.guard(&entry, PublicationState::Pending);
                format!(".{}.pending", entry.artifact_id)
            };
            let path = f.path.join(&name);
            match alteration {
                "symlink" => {
                    fs::rename(&path, f.path.join("outside")).unwrap();
                    symlink(f.path.join("outside"), &path).unwrap();
                }
                "hardlink" => fs::hard_link(&path, f.path.join("outside")).unwrap(),
                "mode" => fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap(),
                "fifo" => {
                    fs::remove_file(&path).unwrap();
                    nix::unistd::mkfifo(&path, Mode::from_bits_truncate(0o600)).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                f.repository().load(None, &f.scope()).is_err(),
                "{kind}: {alteration}"
            );
        }
    }
}

#[test]
fn wrong_owners_and_directory_modes_refuse() {
    let f = Fixture::new();
    let entry = f.seed();
    let owner = Owner {
        uid: f.owner.uid ^ 1,
        gid: f.owner.gid,
    };
    assert!(
        Repository::from_judged_directory(open_directory(&f.path).unwrap(), owner, f.owner.gid)
            .is_err()
    );
    assert!(
        Repository::from_judged_directory(
            open_directory(&f.path).unwrap(),
            f.owner,
            f.owner.gid ^ 1
        )
        .is_err()
    );
    let file = File::open(f.path.join(entry.artifact_id.as_str()).join(DUMP)).unwrap();
    assert!(judge_file(&file, owner, 0o640, DUMP).is_err());
    assert!(
        judge_file(
            &file,
            Owner {
                uid: f.owner.uid,
                gid: f.owner.gid ^ 1
            },
            0o640,
            DUMP
        )
        .is_err()
    );
    fs::set_permissions(
        f.path.join(entry.artifact_id.as_str()),
        fs::Permissions::from_mode(0o770),
    )
    .unwrap();
    assert!(f.repository().load(None, &f.scope()).is_err());
    fs::set_permissions(&f.path, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
        Repository::from_judged_directory(open_directory(&f.path).unwrap(), f.owner, f.owner.gid)
            .is_err()
    );
}

#[test]
fn directory_symlink_is_not_followed() {
    let f = Fixture::new();
    let entry = f.seed();
    let path = f.path.join(entry.artifact_id.as_str());
    fs::rename(&path, f.path.join("outside")).unwrap();
    symlink(f.path.join("outside"), &path).unwrap();
    assert!(f.repository().load(None, &f.scope()).is_err());
}

#[test]
fn held_root_is_not_reopened_when_its_path_changes() {
    let f = Fixture::new();
    let entry = f.seed();
    let repo = f.repository();
    let held_path = f.path.with_extension("held");
    fs::rename(&f.path, &held_path).unwrap();
    fs::create_dir(&f.path).unwrap();
    let result = repo.load(None, &f.scope());
    fs::remove_dir(&f.path).unwrap();
    fs::rename(&held_path, &f.path).unwrap();
    assert_eq!(result.unwrap().entry(), &entry);
}

#[test]
fn oversized_image_refuses_without_reading_it_into_memory() {
    let f = Fixture::new();
    let entry = f.seed();
    let file = fs::OpenOptions::new()
        .write(true)
        .open(f.path.join(entry.artifact_id.as_str()).join(DUMP))
        .unwrap();
    file.set_len(SAVE_POINT_BOUND + 1).unwrap();
    assert!(
        f.repository()
            .load(None, &f.scope())
            .unwrap_err()
            .detail
            .contains("bound")
    );
}

#[test]
fn publication_parent_must_be_an_earlier_indexed_artifact_of_the_same_seed() {
    let f = Fixture::new();
    let first = f.seed();
    let mut second = f.artifact(
        2,
        Some(&first),
        b"raw image: the member judges SQLite",
        None,
    );
    let missing = ArtifactId::from_parts(DecimalU64(99), hash(b"missing")).unwrap();
    second.parent_artifact_id = Some(missing.clone());
    second.confirmation = Confirmation::Rebind {
        parent_artifact_id: missing.to_string(),
    };
    f.rewrite_index(&[first.clone(), second.clone()]);
    assert!(
        f.repository()
            .read_index()
            .unwrap_err()
            .detail
            .contains("earlier indexed")
    );
    second.parent_artifact_id = Some(first.artifact_id.clone());
    second.confirmation = Confirmation::Rebind {
        parent_artifact_id: first.artifact_id.to_string(),
    };
    second.seed_id = "2".repeat(32);
    f.rewrite_index(&[first, second]);
    assert!(
        f.repository()
            .read_index()
            .unwrap_err()
            .detail
            .contains("another seed")
    );
}

#[test]
fn chain_requires_one_initial_seed_and_a_parent_for_every_later_publication() {
    let f = Fixture::new();
    let first = f.seed();
    let mut second = f.artifact(2, Some(&first), b"image", None);
    second.cause = PersistenceCause::Seed;
    second.parent_artifact_id = None;
    second.confirmation = Confirmation::Seed {
        seed_id: second.seed_id.clone(),
    };
    f.rewrite_index(&[first.clone(), second.clone()]);
    assert!(f.repository().read_index().is_err());
    second.cause = PersistenceCause::Requested;
    second.confirmation = Confirmation::Serving {
        run_id: "run".into(),
        capture_id: DecimalU64(1),
    };
    f.rewrite_index(&[first.clone(), second]);
    assert!(f.repository().read_index().is_err());
    let mut not_seed = first;
    not_seed.cause = PersistenceCause::Requested;
    not_seed.confirmation = Confirmation::Serving {
        run_id: "run".into(),
        capture_id: DecimalU64(1),
    };
    f.rewrite_index(&[not_seed]);
    assert!(f.repository().read_index().is_err());
}

#[test]
fn rebind_index_cannot_claim_a_changed_image() {
    let f = Fixture::new();
    let first = f.seed();
    f.artifact(2, Some(&first), b"changed image", None);
    assert!(
        f.repository()
            .read_index()
            .unwrap_err()
            .detail
            .contains("rebind")
    );
}

fn add_acl(path: &Path, default: bool) {
    use std::os::fd::AsRawFd;
    let file = File::open(path).unwrap();
    let mode = file.metadata().unwrap().mode();
    let mut acl = 2u32.to_le_bytes().to_vec();
    // Linux's POSIX ACL xattr format: header version, then tag/perms/id.
    for (tag, permissions, id) in [
        (1u16, ((mode >> 6) & 7) as u16, u32::MAX),
        (2u16, 4u16, nix::unistd::geteuid().as_raw() ^ 1),
        (4u16, ((mode >> 3) & 7) as u16, u32::MAX),
        (16u16, ((mode >> 3) & 7) as u16, u32::MAX),
        (32u16, (mode & 7) as u16, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&permissions.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    let name = if default {
        c"system.posix_acl_default"
    } else {
        c"system.posix_acl_access"
    };
    // SAFETY: the descriptor is held, the name is NUL terminated, and the
    // readable buffer has the length supplied for this synchronous call.
    let result = unsafe {
        nix::libc::fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    assert_eq!(
        result,
        0,
        "ACL fixture setup: {}",
        std::io::Error::last_os_error()
    );
    assert_eq!(
        file.metadata().unwrap().mode(),
        mode,
        "fixture must preserve the mode"
    );
}

#[test]
fn mode_correct_access_acl_never_passes_artifact_custody() {
    for component in [
        DUMP,
        CONFIGURATION,
        BINDING,
        "directory",
        "root",
        "index",
        "guard",
    ] {
        let f = Fixture::new();
        let entry = f.seed();
        let path = match component {
            "root" => f.path.clone(),
            "directory" => f.path.join(entry.artifact_id.as_str()),
            "index" => f.path.join(MANIFEST),
            "guard" => {
                f.guard(&entry, PublicationState::Pending);
                f.path.join(format!(".{}.pending", entry.artifact_id))
            }
            file => f.path.join(entry.artifact_id.as_str()).join(file),
        };
        add_acl(&path, false);
        let result = Repository::from_judged_directory(
            open_directory(&f.path).unwrap(),
            f.owner,
            f.owner.gid,
        )
        .and_then(|repository| repository.load(None, &f.scope()));
        let fault = result.unwrap_err();
        assert!(
            fault.detail.contains("access-control"),
            "{component}: {fault}"
        );
    }
}

#[test]
fn directory_default_acl_and_unreadable_acl_judgment_refuse() {
    for root in [true, false] {
        let f = Fixture::new();
        let entry = f.seed();
        let path = if root {
            f.path.clone()
        } else {
            f.path.join(entry.artifact_id.as_str())
        };
        add_acl(&path, true);
        let result = Repository::from_judged_directory(
            open_directory(&f.path).unwrap(),
            f.owner,
            f.owner.gid,
        )
        .and_then(|repository| repository.load(None, &f.scope()));
        assert!(result.unwrap_err().detail.contains("access-control"));
    }
    // O_PATH permits metadata but not fgetxattr. A failed check is not no ACL.
    let f = Fixture::new();
    let path_fd =
        nix::fcntl::open(&f.path, OFlag::O_PATH | OFlag::O_DIRECTORY, Mode::empty()).unwrap();
    assert!(Repository::from_judged_directory(path_fd, f.owner, f.owner.gid).is_err());
}

/// Real indexed/hash/custody verification for the paired Enter composition tests.
pub(crate) fn verified_enter_fixture(bound: weaver_types::ServingDeclaration) -> VerifiedArtifact {
    let mut fixture = Fixture::new();
    fixture.session = bound.agent().session.clone();
    fixture.trace_path = super::super::trace_path(&bound.agent().trace_sink).to_path_buf();
    let image = b"synthetic member image, not SQLite qualification";
    let bound = bound.with_start(hash(image));
    fixture.artifact(1, None, image, Some(&declaration::render(&bound).unwrap()));
    fixture.repository().load(None, &fixture.scope()).unwrap()
}
