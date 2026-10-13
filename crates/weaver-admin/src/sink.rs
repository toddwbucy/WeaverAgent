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
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
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
                if fd == -1 {
                    // A name claimed between the first look and the creation.
                    return Err(refuse(&format!(
                        "was claimed while admin created it: {}",
                        std::io::Error::last_os_error()
                    )));
                }
                created = true;
            }
            // **Only an absent trace the declaration does not create is no
            // trace**; every other failure is the path being something it
            // must not be, a link, a FIFO, a directory, refused by name
            // (Codex on #82).
            Some(nix::libc::ENOENT) => return Err(LifecycleRefusal::DescriptorsUnusable),
            Some(nix::libc::ELOOP) => {
                return Err(refuse("is a link, and admin opens no link as the trace"));
            }
            Some(nix::libc::ENXIO) => return Err(refuse("is not a regular file")),
            _ => return Err(refuse(&format!("cannot be opened as the trace: {error}"))),
        }
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
    // **And no access entry beyond its mode** (#99 area 2, H5; #107): the
    // trace is the record the agent cannot reach, and an entry granting the
    // member read hides under the mask, which still shows 0640.
    match crate::carries_access_entries_fd(std::os::fd::AsFd::as_fd(&file)) {
        Ok(false) => {}
        Ok(true) => return Err(refuse("carries an access-control entry beyond its mode")),
        Err(_) => {
            return Err(refuse(
                "does not say whether it carries an access-control entry",
            ));
        }
    }
    let owned = OwnedFd::from(file);
    clear_nonblocking(&owned)?;
    Ok(owned)
}

/// **The trace's directory, judged before the open**: resolved once, held by
/// one of the custody's owners and writable by no group or other (the
/// territory's 0710 passage is no write), and
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
mod tests;
