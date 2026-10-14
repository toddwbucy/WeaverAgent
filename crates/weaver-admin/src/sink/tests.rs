//! The FIFO refusal and the third walk's custody half, run from inside the
//! crate because this crate publishes no library surface.

use super::*;
use std::os::fd::AsFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

/// This test's own custody: the uids it runs as, and its primary group
/// named as the trace group, so a trace laid out for it can pass.
fn custody() -> FileCustody {
    let gid = nix::unistd::getgid();
    FileCustody {
        owners: vec![nix::unistd::geteuid().as_raw()],
        trace_group: nix::unistd::Group::from_gid(gid)
            .ok()
            .flatten()
            .map(|group| group.name)
            .expect("the test's primary group has a name"),
    }
}

/// A trace laid out as the territory lays it out, for this test's custody.
fn laid_out_trace(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("trace.ndjson");
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    path
}

/// `super::open` beneath the sink's own directory, opened as the territory
/// is, with no link followed: the path-shaped calls below judge the sink as
/// production does, the directory standing where its path says.
fn open(sink: &TraceSink, custody: &FileCustody) -> Result<OwnedFd, LifecycleRefusal> {
    let path = match sink {
        TraceSink::File { path, .. } | TraceSink::Pipe { path, .. } => path,
        TraceSink::Socket { path } => path,
    };
    let directory = directory_of(path.parent().expect("the sink's directory"));
    super::open(sink, custody, directory.as_fd())
}

fn directory_of(path: &std::path::Path) -> OwnedFd {
    nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .expect("the sink's directory opens")
}

fn file(path: &std::path::Path, create: bool) -> TraceSink {
    TraceSink::File {
        path: path.to_path_buf(),
        create,
    }
}

fn scratch(tag: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-sink-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let _ = std::fs::remove_dir_all(&dir.0);
    std::fs::create_dir_all(&dir.0).expect("scratch");
    dir
}

/// **The FIFO refusal:** a pipe sink with no reader refuses the load with
/// `ENXIO` mapped to its case, rather than hanging on a blocking open.
///
/// Perturbation: drop `O_NONBLOCK` from the open and this test hangs
/// instead of returning - which is exactly the failure the election
/// prevents, and why the watch is a refusal rather than an error code.
#[test]
fn a_reader_less_fifo_refuses_rather_than_hanging() {
    let dir = scratch("fifo");
    let path = dir.join("trace.fifo");
    let refused = open(
        &TraceSink::Pipe {
            path: path.clone(),
            create: true,
        },
        &custody(),
    );
    assert!(
        matches!(refused, Err(LifecycleRefusal::DescriptorsUnusable)),
        "a reader-less FIFO refuses the load, got {refused:?}"
    );
    assert!(
        path.exists(),
        "the FIFO was created before the open refused"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A FIFO with a reader opens, and the nonblocking flag is cleared so the
/// worker's writer sees ordinary blocking semantics.
#[test]
fn a_read_held_fifo_opens_and_clears_nonblocking() {
    let dir = scratch("fifo-open");
    let path = dir.join("trace.fifo");
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).expect("path");
    // SAFETY: mkfifo with a NUL-terminated path this frame owns.
    assert_eq!(unsafe { nix::libc::mkfifo(c_path.as_ptr(), 0o640) }, 0);
    // Hold the read end so the write open succeeds.
    // SAFETY: open with a path this frame owns.
    let reader =
        unsafe { nix::libc::open(c_path.as_ptr(), nix::libc::O_RDONLY | nix::libc::O_NONBLOCK) };
    assert!(reader >= 0, "read end held");

    let opened = open(
        &TraceSink::Pipe {
            path: path.clone(),
            create: false,
        },
        &custody(),
    )
    .expect("opens with a reader present");
    assert!(
        crate::surface::is_close_on_exec(opened.as_fd()),
        "the sink is flagged in the opening call"
    );
    use std::os::fd::AsRawFd;
    // SAFETY: F_GETFL on a descriptor this process owns.
    let flags = unsafe { nix::libc::fcntl(opened.as_raw_fd(), nix::libc::F_GETFL) };
    assert_eq!(
        flags & nix::libc::O_NONBLOCK,
        0,
        "the nonblocking flag is cleared for the worker's writer"
    );
    // SAFETY: closing the raw read end this test opened.
    unsafe { nix::libc::close(reader) };
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The third walk's custody half:** a file sink is flagged in the
/// opening call itself, so no subprocess this crate spawns inherits it.
///
/// Perturbation: drop `O_CLOEXEC` from the open flags and the assertion
/// fails. Watched under exactly that removal.
#[test]
fn a_file_sink_is_flagged_in_its_opening_call() {
    let dir = scratch("file");
    let path = dir.join("trace.ndjson");
    let opened = open(&file(&path, true), &custody()).expect("opens");
    assert!(
        crate::surface::is_close_on_exec(opened.as_fd()),
        "the sink is flagged in the opening call, with no window"
    );
    use std::os::fd::AsRawFd;
    // SAFETY: F_GETFL on a descriptor this process owns.
    let flags = unsafe { nix::libc::fcntl(opened.as_raw_fd(), nix::libc::F_GETFL) };
    assert_ne!(
        flags & nix::libc::O_APPEND,
        0,
        "append-only rides the open file description the worker duplicates"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A trace admin creates takes the territory's layout**: absent and
/// elected for creation, the trace is made 0640 and grouped to the trace
/// group, so the next load finds it laid out. A trace that stands laid
/// out opens. Perturbation: drop the layout after creation and the mode
/// reads the umask's.
#[test]
fn a_created_trace_is_laid_out_and_a_laid_out_trace_opens() {
    let dir = scratch("created");
    let path = dir.join("trace.ndjson");
    // A supplementary group, not the primary one a new file is born in,
    // so only the layout can give the trace this group.
    let primary = nix::unistd::getgid();
    let Some(gid) = nix::unistd::getgroups()
        .unwrap_or_default()
        .into_iter()
        .find(|gid| *gid != primary)
    else {
        eprintln!("SKIP a_created_trace_is_laid_out: this test runs in no supplementary group");
        return;
    };
    let mut custody = custody();
    custody.trace_group = nix::unistd::Group::from_gid(gid)
        .ok()
        .flatten()
        .map(|group| group.name)
        .expect("the supplementary group has a name");
    let gid = gid.as_raw();
    drop(open(&file(&path, true), &custody).expect("created"));
    let meta = std::fs::metadata(&path).unwrap();
    assert_eq!((meta.mode() & 0o7777, meta.gid()), (0o640, gid));
    drop(open(&file(&path, false), &custody).expect("a laid-out trace opens"));
}

/// **A link at the trace's name is refused**, never followed: root would
/// otherwise open, and the relay read back out, whatever file the link
/// names. Perturbation: drop `O_NOFOLLOW` and the link opens its target.
#[test]
fn a_symlinked_trace_refuses() {
    let dir = scratch("link");
    let target = laid_out_trace(&dir);
    let elsewhere = dir.join("elsewhere.ndjson");
    std::fs::rename(&target, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &target).unwrap();
    assert_eq!(
        open(&file(&target, false), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
}

/// **The trace's directory is held closed**: one a group may write is
/// refused before the open, the territory's 0710 passage being no
/// write. Perturbation: drop the directory's mode test and the open
/// proceeds.
#[test]
fn a_group_writable_trace_directory_refuses() {
    let dir = scratch("open-dir");
    let territory = dir.join("territory");
    std::fs::create_dir(&territory).unwrap();
    let path = laid_out_trace(&territory);
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert_eq!(
        open(&file(&path, false), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
    std::fs::set_permissions(&territory, std::fs::Permissions::from_mode(0o710)).unwrap();
    drop(open(&file(&path, false), &custody()).expect("the 0710 passage holds"));
}

/// **A trace not laid out as the territory lays it out is not appended
/// to**: a mode wider than 0640, or a group other than the trace group,
/// is a trace replaced or never provisioned. Perturbation: drop the
/// layout check and both open.
#[test]
fn a_trace_of_another_group_or_mode_refuses() {
    let dir = scratch("layout");
    let path = laid_out_trace(&dir);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        open(&file(&path, false), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a world-readable trace"
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let mut other = custody();
    other.trace_group = "root".into();
    if nix::unistd::getgid().as_raw() != 0 {
        assert_eq!(
            open(&file(&path, false), &other).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a trace grouped to other than the trace group"
        );
    }
}

/// **A trace carrying an access entry beyond its mode refuses** (#99 area
/// 2, H5; #107): an entry granting another principal read hides under
/// the mask, which still shows 0640. Skips, naming why, where setfacl
/// cannot set the entry. Perturbation: drop the look from `open_file` and
/// the trace opens.
#[test]
fn a_trace_carrying_an_access_entry_refuses() {
    let dir = scratch("acl");
    let path = laid_out_trace(&dir);
    let set = std::process::Command::new("setfacl")
        .args(["-m", "u:nobody:r"])
        .arg(&path)
        .status();
    if !set.is_ok_and(|status| status.success()) {
        eprintln!("SKIP: setfacl could not set an entry here");
        return;
    }
    assert_eq!(
        open(&file(&path, false), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a trace the member could read through an entry"
    );
    let cleared = std::process::Command::new("setfacl")
        .arg("-b")
        .arg(&path)
        .status();
    assert!(cleared.is_ok_and(|status| status.success()));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(
        open(&file(&path, false), &custody()).is_ok(),
        "and without it the trace opens"
    );
}

/// **A FIFO at the trace's name refuses without blocking**: with no reader
/// the nonblocking open answers `ENXIO`, and with one the descriptor is no
/// regular file. Perturbation: drop `O_NONBLOCK` and the first case hangs,
/// or the regular-file check and the second opens.
#[test]
fn a_fifo_at_the_trace_path_refuses_without_blocking() {
    let dir = scratch("trace-fifo");
    let path = dir.join("trace.ndjson");
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: mkfifo with a NUL-terminated path this frame owns.
    assert_eq!(unsafe { nix::libc::mkfifo(c_path.as_ptr(), 0o640) }, 0);
    assert_eq!(
        open(&file(&path, false), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "no reader"
    );
    // SAFETY: open with a path this frame owns.
    let reader =
        unsafe { nix::libc::open(c_path.as_ptr(), nix::libc::O_RDONLY | nix::libc::O_NONBLOCK) };
    assert!(reader >= 0);
    assert_eq!(
        open(&file(&path, false), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a reader held"
    );
    // SAFETY: closing the read end this test opened.
    unsafe { nix::libc::close(reader) };
}

/// **A directory at the trace's name refuses by name**, not as an unusable
/// descriptor: the open fails `EISDIR` before any descriptor exists, and
/// only an absent trace the declaration does not create stays
/// `DescriptorsUnusable`. Perturbation: map every other open failure to
/// `DescriptorsUnusable` again and the directory case reads unusable.
#[test]
fn a_directory_at_the_trace_path_refuses_and_only_absence_is_unusable() {
    let dir = scratch("trace-dir");
    let path = dir.join("trace.ndjson");
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        open(&file(&path, false), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
    assert_eq!(
        open(&file(&path, true), &custody()).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
    let absent = dir.join("absent.ndjson");
    assert_eq!(
        open(&file(&absent, false), &custody()).err(),
        Some(LifecycleRefusal::DescriptorsUnusable)
    );
}

/// **A trace another uid owns refuses**, as root inside a user namespace
/// by the watch below, since only root can give a file another owner: a
/// trace laid out root:root 0640 opens, and chowned to uid 1 it refuses.
/// Perturbation: drop the owner from the layout check and the second
/// opens.
#[test]
#[ignore = "needs root; run inside a user namespace by the watch below"]
fn a_trace_another_uid_owns_refuses_as_root() {
    let dir = scratch("owner");
    let path = laid_out_trace(&dir);
    // The host's root is unmapped here and reads as the overflow uid, which
    // holds `/tmp` and `/`, so it joins the owners for the ancestors' sake.
    // Uid 1 stays outside them.
    let custody = FileCustody {
        owners: vec![0, 65534],
        trace_group: "root".into(),
    };
    std::os::unix::fs::chown(&path, Some(0), Some(0)).unwrap();
    drop(open(&file(&path, false), &custody).expect("root's trace opens"));
    std::os::unix::fs::chown(&path, Some(1), Some(0)).unwrap();
    assert_eq!(
        open(&file(&path, false), &custody).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
}

/// The watch for the owner instrument: re-executes this test binary inside
/// `unshare --map-auto --map-root-user` and requires exactly one test
/// passed. A box where the namespace cannot be entered prints a SKIP
/// naming why and passes.
#[test]
fn the_trace_owner_check_is_watched_inside_a_user_namespace() {
    if nix::unistd::geteuid().is_root() {
        return a_trace_another_uid_owns_refuses_as_root();
    }
    let exe = std::env::current_exe().expect("the test binary names itself");
    let ran = std::process::Command::new("unshare")
        .args(["--map-auto", "--map-root-user"])
        .arg(&exe)
        .args([
            "--exact",
            "sink::tests::a_trace_another_uid_owns_refuses_as_root",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            eprintln!("SKIP trace owner watch: unshare could not run: {e}");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with("unshare:") {
        eprintln!(
            "SKIP trace owner watch: no user namespace here: {}",
            stderr.trim()
        );
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "the owner check failed inside the namespace\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// **A pipe sink at a name that is a regular file refuses, the file
/// untouched** (#98; the custody audit's G6): the open would otherwise hand
/// the worker a write handle at offset 0 over the trace or any file the
/// territory holds. Perturbation: drop the FIFO check, and the file opens.
#[test]
fn a_pipe_sink_at_a_regular_file_refuses() {
    let dir = scratch("pipe-file");
    let path = dir.join("trace.ndjson");
    std::fs::write(&path, "the record\n").expect("a regular file");
    for create in [true, false] {
        let refused = open(
            &TraceSink::Pipe {
                path: path.clone(),
                create,
            },
            &custody(),
        );
        assert!(
            matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)),
            "create {create}: {refused:?}"
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "the record\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A link at the pipe's name is refused, never followed** (#98): even a
/// link to a FIFO with a reader. Perturbation: drop `O_NOFOLLOW`, and the
/// link is followed and the FIFO opens.
#[test]
fn a_link_at_the_pipe_sink_refuses() {
    let dir = scratch("pipe-link");
    let fifo = dir.join("real.fifo");
    let c_fifo = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).expect("path");
    // SAFETY: mkfifo with a NUL-terminated path this frame owns.
    assert_eq!(unsafe { nix::libc::mkfifo(c_fifo.as_ptr(), 0o640) }, 0);
    // SAFETY: open with a path this frame owns.
    let reader =
        unsafe { nix::libc::open(c_fifo.as_ptr(), nix::libc::O_RDONLY | nix::libc::O_NONBLOCK) };
    assert!(reader >= 0, "read end held");
    let link = dir.join("trace.fifo");
    std::os::unix::fs::symlink(&fifo, &link).expect("link");
    let refused = open(
        &TraceSink::Pipe {
            path: link,
            create: false,
        },
        &custody(),
    );
    assert!(
        matches!(refused, Err(LifecycleRefusal::BoundaryUnverified)),
        "{refused:?}"
    );
    // SAFETY: closing the raw read end this test opened.
    unsafe { nix::libc::close(reader) };
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The sink opens beneath the territory's descriptor, never the path
/// again** (#140, the custody audit's G18; Spec section 9): the directory
/// is judged and held, then moved aside and another made at its name, as a
/// principal that could rename within the parent might. The trace is created
/// in the directory judged, and the pipe sink is made there too; nothing
/// lands at the name's new directory. Perturbation: open the file sink by
/// its path again and the trace lands in the replacement.
#[test]
fn a_sink_opens_beneath_the_judged_directory_and_not_its_path() {
    let parent = scratch("descriptor");
    let judged = parent.join("territory");
    std::fs::create_dir(&judged).unwrap();
    std::fs::set_permissions(&judged, std::fs::Permissions::from_mode(0o710)).unwrap();
    let directory = directory_of(&judged);
    let moved = parent.join("moved");
    std::fs::rename(&judged, &moved).unwrap();
    std::fs::create_dir(&judged).unwrap();
    std::fs::set_permissions(&judged, std::fs::Permissions::from_mode(0o710)).unwrap();

    let trace = judged.join("trace.ndjson");
    drop(super::open(&file(&trace, true), &custody(), directory.as_fd()).expect("created"));
    assert!(
        moved.join("trace.ndjson").exists(),
        "created in the directory judged"
    );
    assert!(!trace.exists(), "nothing lands at the path's new directory");

    let fifo = judged.join("trace.fifo");
    // No reader holds it, so the open refuses; the FIFO is made all the same.
    let _ = super::open(
        &TraceSink::Pipe {
            path: fifo.clone(),
            create: true,
        },
        &custody(),
        directory.as_fd(),
    );
    assert!(
        moved.join("trace.fifo").exists(),
        "made in the directory judged"
    );
    assert!(!fifo.exists(), "nothing made at the path's new directory");
}
