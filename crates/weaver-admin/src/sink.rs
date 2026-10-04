//! conforms: admin-sink-file-append-only
//! conforms: admin-fifo-open-nonblocking-refuses
//! conforms: admin-sink-path-dies-at-open-site
//! conforms: admin-cloexec-atomic-at-creation
//!
//! The sink, per `weaver-admin-Spec` section 5: opened by the discriminant the
//! config carries, under admin's own principal, **every descriptor
//! close-on-exec in the opening call itself**.
//!
//! **One open site, and the path dies at it.** The sink is resolved and opened
//! here and the resulting `OwnedFd` is what travels, so no other module holds
//! a sink path and the worker never sees one at all.

use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;

use weaver_types::{LifecycleRefusal, TraceSink};

/// **What a file sink's custody is judged against**, supplied by the caller so
/// a test can present it without provisioning: the uids that may hold the
/// trace's directory and the trace itself (root and the admin principal), and
/// the name of the agent's trace group, `weaver-<agent>-trace`, which the
/// territory's layout groups the trace to (#56).
#[derive(Debug, Clone)]
pub struct FileCustody {
    pub owners: Vec<u32>,
    pub trace_group: String,
}

/// The trace's mode as the territory lays it out: the owner writes, the trace
/// group reads, and no one else has anything.
const TRACE_MODE: u32 = 0o640;

/// Opens the sink the config names and yields the descriptor that travels.
///
/// `File` is judged before it is opened and after, per Spec section 5 and #62:
/// its directory is root's (or the admin principal's) and writable by no group
/// or other, every directory above it held by section 9's rule; the open is
/// write-only with `O_APPEND`, `O_CLOEXEC`, `O_NOFOLLOW` and `O_NONBLOCK`, so
/// a link is refused and a FIFO never blocks; the descriptor must be a regular
/// file; and an existing trace must stand as the territory lays it out, owned
/// by root, grouped to the trace group, 0640. Where the flag is set and no
/// trace stands, admin creates it exclusively and gives it that layout.
/// Append-only rides the open file description, so the worker's duplicate
/// appends wherever it writes, which is what the recorder relies on from the
/// far side, and the relay's read-only reopen inherits the judged descriptor.
///
/// `Pipe` is created with `mkfifo` at 0640 when the flag is set, then opened
/// **write-only and nonblocking**, because a blocking open of a reader-less
/// FIFO hangs the load and the nonblocking form fails loudly instead: the open
/// returns `ENXIO` when nothing holds the read end, which refuses the load
/// with the truth, that the operator's tooling is not listening. On success
/// the nonblocking flag is cleared so the worker's writer sees ordinary
/// blocking semantics.
///
/// `Socket` is a stream connection to the operator's listener, close-on-exec
/// at the socket call, with no creation flag: something of the operator's must
/// already be listening, and a connection refused refuses the load.
pub fn open(sink: &TraceSink, custody: &FileCustody) -> Result<OwnedFd, LifecycleRefusal> {
    match sink {
        TraceSink::File { path, create } => open_file(path, *create, custody),
        TraceSink::Pipe { path, create } => open_fifo(path, *create),
        TraceSink::Socket { path } => open_socket(path),
    }
}

fn open_file(
    path: &Path,
    create: bool,
    custody: &FileCustody,
) -> Result<OwnedFd, LifecycleRefusal> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    let refuse = |why: &str| {
        diag!("weaver-admin: the trace {}: {why}", path.display());
        LifecycleRefusal::BoundaryUnverified
    };
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(refuse("names no directory and file"));
    };
    let trace_gid = nix::unistd::Group::from_name(&custody.trace_group)
        .ok()
        .flatten()
        .map(|group| group.gid.as_raw())
        .ok_or_else(|| {
            refuse(&format!(
                "its group {} is not provisioned",
                custody.trace_group
            ))
        })?;
    let directory = judge_trace_directory(parent, &custody.owners)?;
    let c_path = c_string(&directory.join(name))?;
    let flags = nix::libc::O_WRONLY
        | nix::libc::O_APPEND
        | nix::libc::O_CLOEXEC
        | nix::libc::O_NOFOLLOW
        | nix::libc::O_NONBLOCK;
    // SAFETY: open with a NUL-terminated path this frame owns; the returned
    // descriptor is adopted immediately.
    let mut fd = unsafe { nix::libc::open(c_path.as_ptr(), flags) };
    let mut created = false;
    if fd == -1 {
        match std::io::Error::last_os_error().raw_os_error() {
            Some(nix::libc::ENOENT) if create => {
                // SAFETY: as above; O_EXCL makes this open the creation, so a
                // name claimed since the first look refuses rather than opens.
                fd = unsafe {
                    nix::libc::open(
                        c_path.as_ptr(),
                        flags | nix::libc::O_CREAT | nix::libc::O_EXCL,
                        TRACE_MODE as nix::libc::c_uint,
                    )
                };
                created = fd != -1;
            }
            Some(nix::libc::ELOOP) => {
                return Err(refuse("is a link, and admin opens no link as the trace"));
            }
            Some(nix::libc::ENXIO) => return Err(refuse("is not a regular file")),
            _ => {}
        }
    }
    if fd == -1 {
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    // SAFETY: the kernel just created this descriptor and no other owner
    // exists.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let file = std::fs::File::from(owned);
    let metadata = file
        .metadata()
        .map_err(|_| LifecycleRefusal::DescriptorsUnusable)?;
    if !metadata.file_type().is_file() {
        return Err(refuse("is not a regular file"));
    }
    if created {
        // **A trace admin creates takes the territory's layout**, so the next
        // load finds it as the operator's provisioning laid it out.
        // SAFETY: fchown and fchmod on the descriptor this frame owns.
        let laid_out = unsafe {
            nix::libc::fchown(file.as_raw_fd(), u32::MAX, trace_gid) == 0
                && nix::libc::fchmod(file.as_raw_fd(), TRACE_MODE as nix::libc::mode_t) == 0
        };
        if !laid_out {
            return Err(refuse("could not be given the territory's layout"));
        }
    } else if !custody.owners.contains(&metadata.uid())
        || metadata.gid() != trace_gid
        || metadata.mode() & 0o7777 != TRACE_MODE
    {
        return Err(refuse(&format!(
            "stands as uid {} gid {} mode {:o}, not root's, grouped {} and 0640 as the territory \
             lays it out, so it was replaced or never provisioned and is not appended to",
            metadata.uid(),
            metadata.gid(),
            metadata.mode() & 0o7777,
            custody.trace_group
        )));
    }
    let owned = OwnedFd::from(file);
    clear_nonblocking(&owned)?;
    Ok(owned)
}

/// **The trace's directory, judged before the open**: resolved once, held by
/// one of the custody's owners and writable by no group or other (the
/// territory's 0710 pass-through for the member's group is no write), and
/// every directory above it held by section 9's rule. Answers the resolved
/// directory, through which the open goes.
fn judge_trace_directory(
    parent: &Path,
    owners: &[u32],
) -> Result<std::path::PathBuf, LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let mut held: Vec<u32> = owners.to_vec();
    held.push(0);
    let directory = crate::judge_ancestors(parent, &held)?;
    let metadata =
        std::fs::symlink_metadata(&directory).map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    if !metadata.is_dir() || !owners.contains(&metadata.uid()) || metadata.mode() & 0o022 != 0 {
        diag!(
            "weaver-admin: the trace's directory {} is not held closed: owner {}, mode {:o}",
            directory.display(),
            metadata.uid(),
            metadata.mode() & 0o7777
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    Ok(directory)
}

fn open_fifo(path: &Path, create: bool) -> Result<OwnedFd, LifecycleRefusal> {
    let c_path = c_string(path)?;
    if create && !path.exists() {
        // SAFETY: mkfifo with a NUL-terminated path this frame owns.
        let rc = unsafe { nix::libc::mkfifo(c_path.as_ptr(), 0o640) };
        if rc == -1 {
            return Err(LifecycleRefusal::DescriptorsUnusable);
        }
    }
    let flags = nix::libc::O_WRONLY | nix::libc::O_NONBLOCK | nix::libc::O_CLOEXEC;
    // SAFETY: as above.
    let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags) };
    if fd == -1 {
        // ENXIO is the reader-less case, and it maps to the same refusal as
        // any other unusable descriptor: the load refuses rather than hangs.
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    // SAFETY: the kernel just created this descriptor.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    clear_nonblocking(&owned)?;
    Ok(owned)
}

fn clear_nonblocking(fd: &OwnedFd) -> Result<(), LifecycleRefusal> {
    use std::os::fd::AsRawFd;
    // SAFETY: fcntl on a descriptor this process owns.
    let flags = unsafe { nix::libc::fcntl(fd.as_raw_fd(), nix::libc::F_GETFL) };
    if flags == -1 {
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    // SAFETY: as above.
    let rc = unsafe {
        nix::libc::fcntl(
            fd.as_raw_fd(),
            nix::libc::F_SETFL,
            flags & !nix::libc::O_NONBLOCK,
        )
    };
    if rc == -1 {
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    Ok(())
}

fn open_socket(path: &Path) -> Result<OwnedFd, LifecycleRefusal> {
    let stream = UnixStream::connect(path).map_err(|_| LifecycleRefusal::DescriptorsUnusable)?;
    let owned = OwnedFd::from(stream);
    set_close_on_exec(&owned)?;
    Ok(owned)
}

/// **Close-on-exec is what keeps an exec'd tool process from inheriting the
/// sink.** If tool processes run as the agent uid, per the cell
/// `weaver-admin-PRD` section 7 files against the gate charter, this flag is
/// load-bearing for the custody claim rather than hygiene: an inherited sink
/// descriptor is a writable handle to the agent's own account, requiring no
/// path and passing no check.
///
/// The standard library's `UnixStream::connect` passes `SOCK_CLOEXEC` in the
/// `socket(2)` call on Linux, so the descriptor is born flagged and this call
/// is the backstop that keeps the property true on any platform or library
/// version that does not. An earlier form of this comment described a window
/// between creation and flag - measured on 2026-08-05, that window does not
/// exist, and the flag closes the gap rather than narrowing it.
fn set_close_on_exec(fd: &OwnedFd) -> Result<(), LifecycleRefusal> {
    use std::os::fd::AsRawFd;
    // SAFETY: fcntl on a descriptor this process owns.
    let rc = unsafe { nix::libc::fcntl(fd.as_raw_fd(), nix::libc::F_SETFD, nix::libc::FD_CLOEXEC) };
    if rc == -1 {
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    Ok(())
}

fn c_string(path: &Path) -> Result<std::ffi::CString, LifecycleRefusal> {
    std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| LifecycleRefusal::DescriptorsUnusable)
}

#[cfg(test)]
mod tests {
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
        let reader = unsafe {
            nix::libc::open(c_path.as_ptr(), nix::libc::O_RDONLY | nix::libc::O_NONBLOCK)
        };
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
    /// refused before the open, the territory's 0710 pass-through being no
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
        drop(open(&file(&path, false), &custody()).expect("the 0710 pass-through holds"));
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
        let reader = unsafe {
            nix::libc::open(c_path.as_ptr(), nix::libc::O_RDONLY | nix::libc::O_NONBLOCK)
        };
        assert!(reader >= 0);
        assert_eq!(
            open(&file(&path, false), &custody()).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a reader held"
        );
        // SAFETY: closing the read end this test opened.
        unsafe { nix::libc::close(reader) };
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
}
