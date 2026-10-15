//! The gate's part of the agent's cgroup (`weaver-admin-Spec` section 6.1,
//! `weaver-gate-Spec` sections 1, 2 and 8, #111): the handle on `tools/` it
//! receives at descriptor 4, beneath which each shell's child moves itself in
//! before its exec, and the kill and the wait at every end of an execution.
//!
//! The primitives are this crate's own, by the plan of 2026-10-14 (each organ
//! carries what it calls; no organ links another). Every file is opened by
//! `openat` beneath the handle, never by path.

use std::ffi::CStr;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl, openat};
use nix::sys::stat::{Mode, SFlag, fstat};

/// The inherited number of the handle on `tools/`, beside the channel's end
/// at 3 (`weaver-harness-Spec` section 2.2).
pub const TOOLS_FD: RawFd = 4;

/// **The containment bound**: 30 seconds for `populated 0` after a
/// `cgroup.kill`, `weaver-admin-Spec` section 6.1's election, mirrored here.
pub const CONTAINMENT_BOUND: Duration = Duration::from_secs(30);

/// How often the wait reads `cgroup.events`.
const WAIT_SLICE: Duration = Duration::from_millis(10);

/// The file the shell's child writes `0` to, built before any fork so the
/// child allocates nothing.
const PROCS: &CStr = c"cgroup.procs";

/// The handle adopted at entry, read by every execution.
static ADOPTED: OnceLock<Tools> = OnceLock::new();

/// The gate's handle on `tools/`.
#[derive(Debug)]
pub struct Tools {
    dir: OwnedFd,
}

/// Why entry refused the handle: each refuses every directive
/// `BoundaryUnverified`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleFault {
    /// Descriptor 4 is not open.
    Absent,
    /// It is open and not a directory.
    NotADirectory,
    /// It is a directory on no `cgroup2` filesystem: a plain directory
    /// holding files of the same names would let a shell run outside any
    /// cgroup while every kill read as contained.
    NotACgroup,
    /// Its close-on-exec flag would not set, or entry already ran.
    Hygiene,
}

/// **Entry takes descriptor 4** (`weaver-gate-Spec` section 2): open, a
/// directory by `fstat` on the `cgroup2` filesystem by `fstatfs`, and marked
/// close-on-exec at once, so no shell
/// inherits it past its exec; it stays open across the shell's fork for the
/// child's move. Kept for every execution after.
pub fn adopt() -> Result<(), HandleFault> {
    // SAFETY: borrowed only to read its flags; owned below once it verifies.
    let borrowed = unsafe { BorrowedFd::borrow_raw(TOOLS_FD) };
    fcntl(borrowed, FcntlArg::F_GETFD).map_err(|_| HandleFault::Absent)?;
    let stat = fstat(borrowed).map_err(|_| HandleFault::Absent)?;
    if SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT != SFlag::S_IFDIR {
        return Err(HandleFault::NotADirectory);
    }
    if !nix::sys::statfs::fstatfs(borrowed)
        .is_ok_and(|fs| fs.filesystem_type() == nix::sys::statfs::CGROUP2_SUPER_MAGIC)
    {
        return Err(HandleFault::NotACgroup);
    }
    if ADOPTED.get().is_some() {
        return Err(HandleFault::Hygiene);
    }
    // SAFETY: the number was confirmed open above, and this is the one place
    // in the crate that owns it.
    let dir = unsafe { OwnedFd::from_raw_fd(TOOLS_FD) };
    fcntl(dir.as_fd(), FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).map_err(|_| HandleFault::Hygiene)?;
    ADOPTED.set(Tools { dir }).map_err(|_| HandleFault::Hygiene)
}

/// The handle entry adopted, `None` before entry.
pub fn adopted() -> Option<&'static Tools> {
    ADOPTED.get()
}

impl Tools {
    /// A handle on a directory the caller opened: the suite's scratch
    /// `tools/`.
    #[cfg(test)]
    pub(crate) fn from_owned(dir: OwnedFd) -> Self {
        Self { dir }
    }

    /// The raw number the shell's child moves itself in through, valid while
    /// this handle lives.
    pub fn raw(&self) -> RawFd {
        self.dir.as_raw_fd()
    }

    /// **The kill**: `1` written to `cgroup.kill` beneath the handle.
    pub fn kill(&self) -> nix::Result<()> {
        let kill = openat(
            &self.dir,
            "cgroup.kill",
            OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?;
        match nix::unistd::write(&kill, b"1")? {
            1 => Ok(()),
            _ => Err(nix::Error::EIO),
        }
    }

    /// `cgroup.events`' `populated` line: `None` where it does not read.
    pub fn populated(&self) -> Option<bool> {
        let fd = openat(
            &self.dir,
            "cgroup.events",
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .ok()?;
        let text = std::io::read_to_string(std::fs::File::from(fd)).ok()?;
        text.lines()
            .find_map(|line| line.strip_prefix("populated "))
            .and_then(|value| match value.trim() {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            })
    }

    /// **The wait**: `true` once `tools/` reads `populated 0` within
    /// `bound`, a read that fails read as not yet empty (6.1).
    pub fn wait_empty(&self, bound: Duration) -> bool {
        let deadline = Instant::now() + bound;
        loop {
            if self.populated() == Some(false) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(WAIT_SLICE);
        }
    }

    /// **The kill and the wait at an execution's end** (6.1, the gate's
    /// rows): `false` where the write fails or `tools/` does not empty within
    /// `bound`, and the gate then exits with the execution unanswered.
    pub fn contain(&self, bound: Duration) -> bool {
        self.kill().is_ok() && self.wait_empty(bound)
    }
}

/// **The shell's child moves itself into `tools/` before its exec**
/// (`weaver-gate-Spec` section 8), run in `pre_exec`: the parent-death
/// signal set to `SIGKILL`, the gate confirmed still its parent, then `0`
/// written to `cgroup.procs` opened by `openat` beneath the handle. Every
/// call is `nix`'s and async-signal-safe, the path a `CStr` built before the
/// fork, so the child allocates nothing. A failure fails the spawn, and the
/// gate answers `Errored` with nothing run.
pub fn move_self(tools: RawFd, gate: nix::unistd::Pid) -> std::io::Result<()> {
    nix::sys::prctl::set_pdeathsig(Some(nix::sys::signal::Signal::SIGKILL))?;
    if nix::unistd::getppid() != gate {
        return Err(std::io::Error::from(nix::Error::ESRCH));
    }
    // SAFETY: the handle outlives the spawn, which returns only once the
    // child has exec'd or failed.
    let tools = unsafe { BorrowedFd::borrow_raw(tools) };
    let procs = openat(
        tools,
        PROCS,
        OFlag::O_WRONLY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    match nix::unistd::write(&procs, b"0")? {
        1 => Ok(()),
        _ => Err(std::io::Error::from(nix::Error::EIO)),
    }
}
