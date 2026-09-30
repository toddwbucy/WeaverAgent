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

/// Opens the sink the config names and yields the descriptor that travels.
///
/// `File` is write-only with `O_APPEND` and `O_CLOEXEC`, creating at 0640 when
/// the flag is set. Append-only rides the open file description, so the
/// worker's duplicate appends wherever it writes, which is what the recorder
/// relies on from the far side.
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
pub fn open(sink: &TraceSink) -> Result<OwnedFd, LifecycleRefusal> {
    match sink {
        TraceSink::File { path, create } => open_file(path, *create),
        TraceSink::Pipe { path, create } => open_fifo(path, *create),
        TraceSink::Socket { path } => open_socket(path),
    }
}

fn open_file(path: &Path, create: bool) -> Result<OwnedFd, LifecycleRefusal> {
    let mut flags = nix::libc::O_WRONLY | nix::libc::O_APPEND | nix::libc::O_CLOEXEC;
    if create {
        flags |= nix::libc::O_CREAT;
    }
    let c_path = c_string(path)?;
    // SAFETY: open with a NUL-terminated path this frame owns; the returned
    // descriptor is adopted immediately.
    let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags, 0o640 as nix::libc::c_uint) };
    if fd == -1 {
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    // SAFETY: the kernel just created this descriptor and no other owner
    // exists.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
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
        let refused = open(&TraceSink::Pipe {
            path: path.clone(),
            create: true,
        });
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

        let opened = open(&TraceSink::Pipe {
            path: path.clone(),
            create: false,
        })
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
        let opened = open(&TraceSink::File {
            path: path.clone(),
            create: true,
        })
        .expect("opens");
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
}
