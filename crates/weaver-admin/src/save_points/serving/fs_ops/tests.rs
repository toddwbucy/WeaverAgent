use super::*;
use crate::save_points::open_directory;
use std::fs;
use std::io::{Seek, SeekFrom};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    path: PathBuf,
    directory: OwnedFd,
    owner: Owner,
}

impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "publication-fs-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        let directory = open_directory(&path).unwrap();
        let owner = Owner {
            uid: nix::unistd::geteuid().as_raw(),
            gid: nix::unistd::getegid().as_raw(),
        };
        Self {
            path,
            directory,
            owner,
        }
    }

    fn fd(&self) -> BorrowedFd<'_> {
        self.directory.as_fd()
    }

    fn write(&self, name: &str, bytes: &[u8], mode: u32) -> File {
        write_new(self.fd(), name, bytes, self.owner, mode).unwrap()
    }

    fn bytes(&self, name: &str) -> Vec<u8> {
        fs::read(self.path.join(name)).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        FAILURE.with(|value| *value.borrow_mut() = None);
        PARTIAL_WRITE.with(|value| value.set(None));
        COPY_HOOK.with(|value| *value.borrow_mut() = None);
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn fail(point: Point, matching_calls_before_failure: usize) {
    FAILURE.with(|value| *value.borrow_mut() = Some((point, matching_calls_before_failure)));
}

#[test]
fn held_part_finishes_renames_and_removes_only_its_exact_files() {
    let f = Fixture::new();
    let mut part = create_part(f.fd(), "part", f.owner).unwrap();
    assert_ne!(
        nix::fcntl::fcntl(part.root.as_fd(), nix::fcntl::FcntlArg::F_GETFD).unwrap()
            & nix::libc::FD_CLOEXEC,
        0
    );
    assert_ne!(
        nix::fcntl::fcntl(part.fd(), nix::fcntl::FcntlArg::F_GETFD).unwrap()
            & nix::libc::FD_CLOEXEC,
        0
    );
    assert_eq!(
        nix::sys::stat::fstat(part.fd()).unwrap().st_mode & 0o7777,
        0o700
    );
    for name in FILES {
        let file = write_new(part.fd(), name, name.as_bytes(), f.owner, 0o640).unwrap();
        let metadata = file.metadata().unwrap();
        assert_ne!(
            nix::fcntl::fcntl(file.as_fd(), nix::fcntl::FcntlArg::F_GETFD).unwrap()
                & nix::libc::FD_CLOEXEC,
            0
        );
        assert_eq!(metadata.mode() & 0o7777, 0o640);
        assert_eq!((metadata.uid(), metadata.gid()), (f.owner.uid, f.owner.gid));
    }
    let identity = Identity::read(part.fd()).unwrap();
    assert!(part.rename_no_replace("published").is_err());
    part.finish_mode(f.owner).unwrap();
    part.rename_no_replace("published").unwrap();
    assert_eq!(part.name(), "published");
    assert_eq!(Identity::at(f.fd(), "published").unwrap(), identity);
    assert!(!f.path.join("part").exists());
    part.remove().unwrap();
    assert!(!f.path.join("published").exists());
}

#[test]
fn exclusive_creation_refuses_existing_names_paths_symlinks_and_wrong_modes() {
    let f = Fixture::new();
    f.write("existing", b"first", 0o640);
    assert!(write_new(f.fd(), "existing", b"second", f.owner, 0o640).is_err());
    symlink("existing", f.path.join("link")).unwrap();
    assert!(write_new(f.fd(), "link", b"second", f.owner, 0o640).is_err());
    for name in ["", ".", "..", "sub/file", "a\0b"] {
        assert!(write_new(f.fd(), name, b"x", f.owner, 0o640).is_err());
        assert!(create_part(f.fd(), name, f.owner).is_err());
    }
    assert!(write_new(f.fd(), "wrong-mode", b"x", f.owner, 0o666).is_err());
    assert_eq!(f.bytes("existing"), b"first");
    assert!(!f.path.join("wrong-mode").exists());
}

#[test]
fn copy_hashes_exact_bytes_without_moving_the_source_offset() {
    let f = Fixture::new();
    let bytes: Vec<_> = (0..140_001).map(|n| (n % 251) as u8).collect();
    let mut source = f.write("source", &bytes, 0o640);
    source.seek(SeekFrom::Start(12_345)).unwrap();
    let copy = copy_new(f.fd(), "copy", &source, bytes.len() as u64, f.owner, 0o640).unwrap();
    assert_eq!(source.stream_position().unwrap(), 12_345);
    assert_eq!(copy.size_bytes.0, bytes.len() as u64);
    assert_eq!(copy.file.metadata().unwrap().len(), bytes.len() as u64);
    assert_eq!(
        Identity::read(copy.file.as_fd()).unwrap(),
        Identity::at(f.fd(), "copy").unwrap()
    );
    assert_eq!(
        copy.sha256.as_str(),
        format!("{:x}", Sha256::digest(&bytes))
    );
    assert_eq!(f.bytes("copy"), bytes);
}

#[test]
fn copy_refuses_false_size_oversize_and_source_growth_without_success() {
    let f = Fixture::new();
    let source = f.write("source", b"abcdef", 0o640);
    assert!(copy_new(f.fd(), "wrong-size", &source, 5, f.owner, 0o640).is_err());
    assert!(!f.path.join("wrong-size").exists());
    source.set_len(SAVE_POINT_BOUND + 1).unwrap();
    assert!(
        copy_new(
            f.fd(),
            "oversize",
            &source,
            SAVE_POINT_BOUND + 1,
            f.owner,
            0o640
        )
        .is_err()
    );
    assert!(!f.path.join("oversize").exists());
    source.set_len(6).unwrap();
    let mutate = source.try_clone().unwrap();
    COPY_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            mutate.write_all_at(b"extra", 6).unwrap();
        }))
    });
    assert!(copy_new(f.fd(), "growing", &source, 6, f.owner, 0o640).is_err());
    assert_eq!(f.bytes("growing"), b"abcdef");
}

#[test]
fn copy_refuses_source_shrink_and_same_size_rewrite() {
    let f = Fixture::new();
    let source = f.write("source", &vec![b'a'; 70_000], 0o640);
    let mutate = source.try_clone().unwrap();
    COPY_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            mutate.set_len(65_536).unwrap();
        }))
    });
    assert!(copy_new(f.fd(), "shrinking", &source, 70_000, f.owner, 0o640).is_err());
    source.set_len(70_000).unwrap();
    let mutate = source.try_clone().unwrap();
    let before = source.metadata().unwrap();
    COPY_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            mutate.write_all_at(b"changed", 0).unwrap();
            // Pin a different modification time instead of relying on clock resolution.
            let times = std::fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH);
            mutate.set_times(times).unwrap();
            assert_ne!(mutate.metadata().unwrap().mtime(), before.mtime());
        }))
    });
    assert!(copy_new(f.fd(), "rewritten", &source, 70_000, f.owner, 0o640).is_err());
}

#[test]
fn ownership_mode_and_sync_failures_never_return_success() {
    let f = Fixture::new();
    for (index, point) in [
        Point::BeforeOwner,
        Point::BeforeMode,
        Point::FileSync,
        Point::DirectorySync,
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("failure-{index}");
        fail(point, 0);
        assert!(write_new(f.fd(), &name, b"retained evidence", f.owner, 0o640).is_err());
        assert!(f.path.join(name).exists());
    }
    fail(Point::BeforeMode, 0);
    assert!(create_part(f.fd(), "failed-part", f.owner).is_err());
    assert!(!f.path.join("failed-part").exists());
}

#[test]
fn rename_never_replaces_and_updates_held_name_before_postrename_failure() {
    let f = Fixture::new();
    let mut part = create_part(f.fd(), "part", f.owner).unwrap();
    part.finish_mode(f.owner).unwrap();
    fs::create_dir(f.path.join("occupied")).unwrap();
    let occupied = Identity::at(f.fd(), "occupied").unwrap();
    assert!(part.rename_no_replace("occupied").is_err());
    assert_eq!(part.name(), "part");
    assert_eq!(Identity::at(f.fd(), "occupied").unwrap(), occupied);
    fail(Point::AfterRename, 0);
    assert!(part.rename_no_replace("published").is_err());
    assert_eq!(part.name(), "published");
    assert!(f.path.join("published").exists());
    part.remove().unwrap();
}

#[test]
fn rename_refuses_a_replaced_directory_entry() {
    let f = Fixture::new();
    let mut part = create_part(f.fd(), "part", f.owner).unwrap();
    part.finish_mode(f.owner).unwrap();
    fs::rename(f.path.join("part"), f.path.join("moved")).unwrap();
    fs::create_dir(f.path.join("part")).unwrap();
    assert!(part.rename_no_replace("published").is_err());
    assert!(!f.path.join("published").exists());
    assert!(f.path.join("part").is_dir());
    assert!(f.path.join("moved").is_dir());
}

#[test]
fn explicit_cleanup_refuses_unknown_or_unowned_children_before_removing_any() {
    for bad in ["unknown", "symlink", "hardlink", "directory"] {
        let f = Fixture::new();
        let part = create_part(f.fd(), "part", f.owner).unwrap();
        write_new(part.fd(), "state.sqlite", b"keep", f.owner, 0o640).unwrap();
        match bad {
            "unknown" => {
                fs::write(f.path.join("part/unknown"), b"keep").unwrap();
            }
            "symlink" => {
                symlink("state.sqlite", f.path.join("part/agent.toml")).unwrap();
            }
            "hardlink" => {
                fs::hard_link(
                    f.path.join("part/state.sqlite"),
                    f.path.join("part/agent.toml"),
                )
                .unwrap();
            }
            "directory" => {
                fs::create_dir(f.path.join("part/agent.toml")).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(part.remove().is_err(), "{bad}");
        assert_eq!(f.bytes("part/state.sqlite"), b"keep", "{bad}");
    }
}

#[test]
fn append_requires_the_exact_complete_prior_bytes_and_adds_one_lf() {
    let f = Fixture::new();
    append_line(f.fd(), "index", f.owner, b"", b"first").unwrap();
    append_line(f.fd(), "index", f.owner, b"first\n", b"second").unwrap();
    assert_eq!(f.bytes("index"), b"first\nsecond\n");
    let metadata = fs::metadata(f.path.join("index")).unwrap();
    assert_eq!(metadata.mode() & 0o7777, 0o644);
    assert_eq!((metadata.uid(), metadata.gid()), (f.owner.uid, f.owner.gid));
    for prior in [b"first\n".as_slice(), b"wrong\nsecond\n", b"first\nsecond"] {
        assert!(append_line(f.fd(), "index", f.owner, prior, b"third").is_err());
    }
    for record in [b"".as_slice(), b"third\nfourth"] {
        assert!(append_line(f.fd(), "index", f.owner, b"first\nsecond\n", record).is_err());
    }
    assert!(append_line(f.fd(), "missing", f.owner, b"prior\n", b"new").is_err());
    assert!(!f.path.join("missing").exists());
    assert_eq!(f.bytes("index"), b"first\nsecond\n");
}

#[test]
fn partial_append_is_never_truncated_or_repaired() {
    let f = Fixture::new();
    append_line(f.fd(), "index", f.owner, b"", b"first").unwrap();
    PARTIAL_WRITE.with(|value| value.set(Some(3)));
    assert!(append_line(f.fd(), "index", f.owner, b"first\n", b"second").is_err());
    assert_eq!(f.bytes("index"), b"first\nsec");
    assert!(append_line(f.fd(), "index", f.owner, b"first\n", b"retry").is_err());
    assert!(append_line(f.fd(), "index", f.owner, b"first\nsec", b"retry").is_err());
    assert_eq!(f.bytes("index"), b"first\nsec");
}

#[test]
fn uncertain_complete_append_and_new_index_sync_failure_leave_evidence() {
    let f = Fixture::new();
    append_line(f.fd(), "index", f.owner, b"", b"first").unwrap();
    fail(Point::FileSync, 0);
    assert!(append_line(f.fd(), "index", f.owner, b"first\n", b"second").is_err());
    assert_eq!(f.bytes("index"), b"first\nsecond\n");
    fail(Point::DirectorySync, 0);
    assert!(append_line(f.fd(), "new-index", f.owner, b"", b"first").is_err());
    assert_eq!(f.bytes("new-index"), b"");
    assert_eq!(
        fs::metadata(f.path.join("new-index")).unwrap().mode() & 0o7777,
        0o644
    );
}

#[test]
fn guard_replacement_always_retains_old_or_new_blocking_bytes() {
    for (index, point) in [Point::FileSync, Point::BeforeReplace, Point::AfterReplace]
        .into_iter()
        .enumerate()
    {
        let f = Fixture::new();
        f.write("guard", b"old", 0o640);
        fail(point, 0);
        assert!(
            replace_file(
                f.fd(),
                "guard",
                "replacement",
                b"old",
                b"new",
                f.owner,
                0o640
            )
            .is_err()
        );
        assert_eq!(f.bytes("guard"), if index == 2 { b"new" } else { b"old" });
    }
    let f = Fixture::new();
    f.write("guard", b"old", 0o640);
    fail(Point::DirectorySync, 1);
    assert!(
        replace_file(
            f.fd(),
            "guard",
            "replacement",
            b"old",
            b"new",
            f.owner,
            0o640
        )
        .is_err()
    );
    assert_eq!(f.bytes("guard"), b"new");
    assert!(!f.path.join("replacement").exists());
}

#[test]
fn guard_replacement_and_removal_require_exact_prior_bytes() {
    let f = Fixture::new();
    f.write("guard", b"old", 0o640);
    assert!(replace_file(f.fd(), "guard", "temporary", b"bad", b"new", f.owner, 0o640).is_err());
    assert!(!f.path.join("temporary").exists());
    replace_file(f.fd(), "guard", "temporary", b"old", b"new", f.owner, 0o640).unwrap();
    assert_eq!(f.bytes("guard"), b"new");
    assert!(remove_file(f.fd(), "guard", b"old", f.owner, 0o640).is_err());
    assert_eq!(f.bytes("guard"), b"new");
    remove_file(f.fd(), "guard", b"new", f.owner, 0o640).unwrap();
    assert!(!f.path.join("guard").exists());
}

#[test]
fn held_root_not_a_later_path_replacement_receives_the_write() {
    let mut f = Fixture::new();
    let original = f.path.clone();
    let moved = original.with_extension("moved");
    fs::rename(&original, &moved).unwrap();
    fs::create_dir(&original).unwrap();
    f.write("held", b"first root", 0o640);
    assert!(!original.join("held").exists());
    assert_eq!(fs::read(moved.join("held")).unwrap(), b"first root");
    fs::remove_dir(&original).unwrap();
    f.path = moved;
}

#[test]
fn fsync_retries_only_interrupted_and_space_count_is_available() {
    let mut calls = 0;
    sync_retry(|| {
        calls += 1;
        if calls <= 2 {
            Err(io::Error::from(io::ErrorKind::Interrupted))
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert_eq!(calls, 3);
    calls = 0;
    assert!(
        sync_retry(|| {
            calls += 1;
            Err(io::Error::other("disk failure"))
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    let f = Fixture::new();
    assert!(available_bytes(f.fd()).unwrap() > 0);
}

#[test]
fn inherited_access_entries_refuse_creation_even_with_exact_mode() {
    use std::os::fd::AsRawFd;
    let f = Fixture::new();
    let mut acl = 2u32.to_le_bytes().to_vec();
    for (tag, permissions, id) in [
        (1u16, 7u16, u32::MAX),
        (2u16, 4u16, f.owner.uid ^ 1),
        (4u16, 5u16, u32::MAX),
        (16u16, 5u16, u32::MAX),
        (32u16, 0u16, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&permissions.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    // SAFETY: the held descriptor, NUL-terminated name and readable ACL buffer
    // remain valid for this synchronous syscall.
    let result = unsafe {
        nix::libc::fsetxattr(
            f.fd().as_raw_fd(),
            c"system.posix_acl_default".as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    assert_eq!(result, 0, "{}", io::Error::last_os_error());
    let error = write_new(f.fd(), "inherited", b"x", f.owner, 0o640).unwrap_err();
    assert!(error.to_string().contains("access-control"));
    assert_eq!(
        fs::metadata(f.path.join("inherited")).unwrap().mode() & 0o7777,
        0o640
    );
    assert!(create_part(f.fd(), "inherited-part", f.owner).is_err());
    assert!(!f.path.join("inherited-part").exists());
}
