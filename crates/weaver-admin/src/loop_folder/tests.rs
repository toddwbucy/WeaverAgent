use super::*;
use std::os::unix::fs::PermissionsExt;

/// sha256 of `abc`, the standard's own test vector, so the digest admin
/// carries is checked against a value this crate did not compute.
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn ids() -> (u32, u32) {
    (
        nix::unistd::geteuid().as_raw(),
        nix::unistd::getegid().as_raw(),
    )
}

/// The agent's folder `0755` holding `loop/` at `0750` and `wires.py` at
/// `0640` holding `content`, all the test's own and its group's.
fn laid(tag: &str, content: &[u8]) -> crate::scratch::Scratch {
    let base = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-loop-folder-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let folder = base.join("a1");
    std::fs::create_dir_all(folder.join(LOOP_FOLDER)).expect("the folders");
    std::fs::set_permissions(&*base, std::fs::Permissions::from_mode(0o755)).expect("base");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).expect("folder");
    std::fs::set_permissions(
        folder.join(LOOP_FOLDER),
        std::fs::Permissions::from_mode(0o750),
    )
    .expect("loop");
    let wire = folder.join(LOOP_FOLDER).join(WIRE_FILE);
    std::fs::write(&wire, content).expect("the wire file");
    std::fs::set_permissions(&wire, std::fs::Permissions::from_mode(0o640)).expect("wire");
    base
}

/// **The bound is 256 KiB**, as `weaver-admin-Spec` section 4 states it, so
/// a moved constant is caught here and not followed by the rows that use it.
#[test]
fn the_wire_files_bound_is_256_kib() {
    assert_eq!(WIRE_FILE_BOUND, 262_144);
}

/// **The agent's folder is the config base's parent joined with the name.**
#[test]
fn the_agents_folder_stands_beside_the_config_base() {
    assert_eq!(
        agent_folder(Path::new("/etc/weaver/admin/a1"), "a1"),
        Some(PathBuf::from("/etc/weaver/a1"))
    );
    assert_eq!(
        agent_folder(Path::new("/srv/x/admin-test/a1"), "a1"),
        Some(PathBuf::from("/srv/x/a1"))
    );
    assert_eq!(agent_folder(Path::new("a1"), "a1"), None);
}

/// **Where no loop folder stands, the compiled loop runs**: an absent
/// agent's folder and an agent's folder with no `loop/` both answer none.
/// Perturbation: refuse an absent folder, and every agent laid before the
/// loop folder refuses its load.
#[test]
fn no_loop_folder_is_no_election() {
    let (owner, group) = ids();
    let base = laid("none", b"abc");
    assert_eq!(judge(&base.join("absent"), owner, group), Ok(None));
    std::fs::remove_dir_all(base.join("a1").join(LOOP_FOLDER)).expect("remove loop");
    assert_eq!(judge(&base.join("a1"), owner, group), Ok(None));
}

/// **Admin's half of the two digests**: the election names the wire file
/// and carries the sha256 of the bytes admin read, which the relay compares
/// with the runner's `hello`. Perturbation: digest anything but the bytes
/// read, or name another path, and this fails.
#[test]
fn the_election_names_the_file_and_the_digest_of_its_bytes() {
    let (owner, group) = ids();
    let base = laid("digest", b"abc");
    let folder = base.join("a1");
    let election = judge(&folder, owner, group)
        .expect("judged")
        .expect("an election");
    assert_eq!(
        election.file,
        folder.join("loop/wires.py").to_str().expect("utf-8")
    );
    assert_eq!(election.sha256, ABC);
}

/// **A laid change takes effect at the next load alone**: the judgment
/// reads the file at each call and keeps nothing, so an election already
/// made carries what was read, and the next one carries the change.
/// Perturbation: cache the first read, and the second election is stale.
#[test]
fn a_change_is_read_at_the_next_judgment_alone() {
    let (owner, group) = ids();
    let base = laid("change", b"abc");
    let folder = base.join("a1");
    let first = judge(&folder, owner, group)
        .expect("judged")
        .expect("an election");
    std::fs::write(folder.join("loop/wires.py"), b"abd").expect("the change");
    let second = judge(&folder, owner, group)
        .expect("judged")
        .expect("an election");
    assert_eq!(first.sha256, ABC);
    assert_ne!(second.sha256, ABC);
    assert_eq!(second.file, first.file);
}

/// **Each piece out of its shape refuses, naming it**, the folder read
/// through the descriptor already judged. Perturbation: drop any one
/// check, and its row is admitted.
#[test]
fn each_piece_out_of_its_shape_refuses() {
    let (owner, group) = ids();
    type Spoil = fn(&Path);
    let rows: &[(&str, Spoil, &str)] = &[
        (
            "folder-link",
            |f| {
                std::fs::rename(f, f.with_file_name("real")).expect("move");
                std::os::unix::fs::symlink(f.with_file_name("real"), f).expect("link");
            },
            "the agent's folder",
        ),
        (
            "folder-writable",
            |f| {
                std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o775)).expect("mode");
            },
            "writable by its group or by others",
        ),
        (
            "loop-link",
            |f| {
                std::fs::rename(f.join("loop"), f.join("real")).expect("move");
                std::os::unix::fs::symlink(f.join("real"), f.join("loop")).expect("link");
            },
            "loop folder",
        ),
        (
            "loop-file",
            |f| {
                std::fs::remove_dir_all(f.join("loop")).expect("remove");
                std::fs::write(f.join("loop"), b"").expect("file");
            },
            "is not a directory",
        ),
        (
            "loop-mode",
            |f| {
                std::fs::set_permissions(f.join("loop"), std::fs::Permissions::from_mode(0o755))
                    .expect("mode");
            },
            "is not mode 0750",
        ),
        (
            "wires-absent",
            |f| {
                std::fs::remove_file(f.join("loop/wires.py")).expect("remove");
            },
            "does not stand in the loop folder",
        ),
        (
            "wires-link",
            |f| {
                std::fs::rename(f.join("loop/wires.py"), f.join("loop/real.py")).expect("move");
                std::os::unix::fs::symlink(f.join("loop/real.py"), f.join("loop/wires.py"))
                    .expect("link");
            },
            "is a link",
        ),
        (
            "wires-directory",
            |f| {
                std::fs::remove_file(f.join("loop/wires.py")).expect("remove");
                std::fs::create_dir(f.join("loop/wires.py")).expect("dir");
                std::fs::set_permissions(
                    f.join("loop/wires.py"),
                    std::fs::Permissions::from_mode(0o640),
                )
                .expect("mode");
            },
            "is not a regular file",
        ),
        (
            "wires-fifo",
            |f| {
                std::fs::remove_file(f.join("loop/wires.py")).expect("remove");
                nix::unistd::mkfifo(
                    &f.join("loop/wires.py"),
                    nix::sys::stat::Mode::from_bits_truncate(0o640),
                )
                .expect("fifo");
            },
            "is not a regular file",
        ),
        (
            "wires-mode",
            |f| {
                std::fs::set_permissions(
                    f.join("loop/wires.py"),
                    std::fs::Permissions::from_mode(0o644),
                )
                .expect("mode");
            },
            "is not mode 0640",
        ),
        (
            "wires-bound",
            |f| {
                std::fs::write(
                    f.join("loop/wires.py"),
                    vec![b'#'; WIRE_FILE_BOUND as usize + 1],
                )
                .expect("grow");
            },
            "holds more than 256 KiB",
        ),
    ];
    for (tag, spoil, said) in rows {
        let base = laid(tag, b"abc");
        let folder = base.join("a1");
        spoil(&folder);
        let refused = judge(&folder, owner, group).expect_err(tag);
        assert!(refused.contains(said), "{tag}: {refused}");
    }
    // A wire file of exactly the bound is admitted.
    let base = laid("at-bound", &vec![b'#'; WIRE_FILE_BOUND as usize]);
    assert!(
        judge(&base.join("a1"), owner, group)
            .expect("judged")
            .is_some()
    );
}

/// **Owned or grouped by another refuses**: every piece is root's, and the
/// loop folder and the wire file the agent's group's. Asked here as an
/// owner or a group the fixture's files do not carry. Perturbation: drop
/// the owner or the group check, and its row is admitted.
#[test]
fn another_owner_or_group_refuses() {
    let (owner, group) = ids();
    let base = laid("owner", b"abc");
    let folder = base.join("a1");
    let refused = judge(&folder, owner.wrapping_add(1), group).expect_err("owner");
    assert!(
        refused.contains("the agent's folder") && refused.contains("is not root's"),
        "{refused}"
    );
    let refused = judge(&folder, owner, group.wrapping_add(1)).expect_err("group");
    assert!(
        refused.contains("loop folder") && refused.contains("is not grouped to the agent's group"),
        "{refused}"
    );
}

/// **The wire file grouped to another group refuses**, the loop folder
/// standing in its shape: set to another of this uid's own groups, and
/// skipped, naming why, where it holds none (as the declaration's group case
/// in `src/tests.rs` is). Perturbation: drop the wire file's group check, and
/// it is admitted.
#[test]
fn a_wire_file_of_another_group_refuses() {
    let (owner, group) = ids();
    let egid = nix::unistd::getegid();
    let Some(other) = nix::unistd::getgroups()
        .unwrap_or_default()
        .into_iter()
        .find(|g| *g != egid)
    else {
        eprintln!(
            "skipped: this uid holds one group, so the wire file's group cannot be set apart"
        );
        return;
    };
    let base = laid("wire-group", b"abc");
    let wire = base.join("a1/loop/wires.py");
    nix::unistd::chown(&wire, None, Some(other)).expect("chgrp to another of this uid's groups");
    let refused = judge(&base.join("a1"), owner, group).expect_err("another group");
    assert!(
        refused.contains("the wire file")
            && refused.contains("is not grouped to the agent's group"),
        "{refused}"
    );
}

/// **An access-control entry beyond the mode refuses, on each piece**, a row
/// each: the agent's folder, the loop folder and the wire file, each given a
/// named user's read entry that leaves its mode as it was, so the judgment
/// reaches the entry's check. Skipped, naming why, where setfacl cannot set
/// an entry here. Perturbation: drop any piece's entry check, and its row is
/// admitted.
#[test]
fn an_access_entry_on_any_piece_refuses() {
    let (owner, group) = ids();
    for (row, piece, named) in [
        ("the agent's folder", "a1", "the agent's folder"),
        ("the loop folder", "a1/loop", "the loop folder"),
        ("the wire file", "a1/loop/wires.py", "the wire file"),
    ] {
        let base = laid("acl", b"abc");
        let path = base.join(piece);
        let set = std::process::Command::new("setfacl")
            .args(["-m", "u:nobody:r"])
            .arg(&path)
            .status();
        if !set.is_ok_and(|status| status.success()) {
            eprintln!(
                "SKIP an_access_entry_on_any_piece_refuses: setfacl could not set an entry here"
            );
            return;
        }
        let refused = judge(&base.join("a1"), owner, group).expect_err(row);
        assert_eq!(
            refused,
            format!(
                "{named} {} carries an access-control entry beyond its mode",
                path.display()
            ),
            "{row}"
        );
    }
    // The control: the same layout with no entry is elected.
    let base = laid("acl-control", b"abc");
    assert!(judge(&base.join("a1"), owner, group).unwrap().is_some());
}
