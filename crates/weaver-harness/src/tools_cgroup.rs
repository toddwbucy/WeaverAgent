//! The worker's part of the agent's cgroup (`weaver-admin-Spec` section 6.1,
//! `weaver-harness-Spec` section 2.2, #111): the handle on `tools/` the
//! worker opens once at the enter and hands the gate at descriptor 4, and the
//! kill of `tools/` it makes when it reads the gate's death and at its unwind.
//!
//! The primitives are this crate's own, by the plan of 2026-10-14 (each organ
//! carries what it calls; no organ links another): the kill, the wait on
//! `cgroup.events`, and the pidfd-pinned fallback, all beneath the handle's
//! descriptor and never by path after the one open. `pidfd_open` and
//! `pidfd_send_signal` are the one stated exception to "no raw `libc`"
//! (`weaver-harness-Spec` section 1), made by number through `nix`'s
//! re-exported `libc` and wrapped at once as an owned descriptor.

use std::os::fd::{AsFd, BorrowedFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

use nix::fcntl::{OFlag, openat};
use nix::sys::stat::{Mode, SFlag, fstat};

/// **The containment bound**: 30 seconds for `populated 0` after a
/// `cgroup.kill`, `weaver-admin-Spec` section 6.1's election, stated there
/// once and mirrored here.
pub const CONTAINMENT_BOUND: Duration = Duration::from_secs(30);

/// How often a wait reads `cgroup.events`.
const WAIT_SLICE: Duration = Duration::from_millis(10);

/// The unified hierarchy's mount.
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// The leaf the worker starts in, beneath `worker/` (6.1, the layout).
const RUN_LEAF: &str = "/worker/run";

/// The shell executions' leaf, beside `run/`.
const TOOLS_LEAF: &str = "tools";

/// Why the worker could not open or judge `tools/` at the enter: each refuses
/// the gate's arm `BoundaryUnverified` (6.1; `weaver-harness-Spec` section
/// 2.2).
#[derive(Debug)]
pub enum OpenRefused {
    /// `/proc/self/cgroup` did not read or carries no unified line.
    OwnCgroup,
    /// The worker's own cgroup is not a `worker/run` leaf, so no `tools/`
    /// stands beside it.
    NotInRun(String),
    /// `tools/` did not open.
    Open(nix::Error),
    /// What opened is not a directory.
    NotADirectory,
    /// A file the worker must write beneath `tools/` did not open for write.
    File(&'static str, nix::Error),
}

impl std::fmt::Display for OpenRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OwnCgroup => write!(f, "/proc/self/cgroup names no unified cgroup"),
            Self::NotInRun(own) => write!(f, "the worker's cgroup {own} is not a worker/run leaf"),
            Self::Open(errno) => write!(f, "tools/ does not open: {errno}"),
            Self::NotADirectory => write!(f, "tools/ is not a directory"),
            Self::File(name, errno) => write!(f, "tools/{name} does not open for write: {errno}"),
        }
    }
}

/// The worker's handle on `tools/`, opened `O_DIRECTORY` once and never
/// resolved by path again.
#[derive(Debug)]
pub struct ToolsCgroup {
    dir: OwnedFd,
    /// The path beneath the mount, `/` first, as `/proc/<pid>/cgroup` names
    /// it: the pinned fallback's confirmation.
    relative: String,
}

impl ToolsCgroup {
    /// **Opened from the cgroup the worker starts in**: `/proc/self/cgroup`'s
    /// unified line names `.../worker/run`, and `tools/` is its sibling.
    pub fn open_from_own() -> Result<Self, OpenRefused> {
        let own = own_cgroup().ok_or(OpenRefused::OwnCgroup)?;
        Self::open_beside_run(&own)
    }

    /// `tools/` beside the `run/` leaf `own` names: opened `O_DIRECTORY |
    /// O_NOFOLLOW`, judged a directory by `fstat`, and judged writable where
    /// the worker writes, `cgroup.kill` and `cgroup.procs`, so a handle the
    /// worker could not kill through refuses at the enter and not at the
    /// first death.
    pub fn open_beside_run(own: &str) -> Result<Self, OpenRefused> {
        let worker = own
            .strip_suffix("/run")
            .filter(|_| own.ends_with(RUN_LEAF))
            .ok_or_else(|| OpenRefused::NotInRun(own.to_string()))?;
        let relative = format!("{worker}/{TOOLS_LEAF}");
        let path = format!("{CGROUP_ROOT}{relative}");
        let dir = nix::fcntl::open(
            path.as_str(),
            OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_RDONLY,
            Mode::empty(),
        )
        .map_err(OpenRefused::Open)?;
        let stat = fstat(&dir).map_err(OpenRefused::Open)?;
        if SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT != SFlag::S_IFDIR {
            return Err(OpenRefused::NotADirectory);
        }
        let tools = Self { dir, relative };
        for name in ["cgroup.kill", "cgroup.procs"] {
            tools
                .open_for_write(name)
                .map_err(|errno| OpenRefused::File(name, errno))?;
        }
        Ok(tools)
    }

    /// The path beneath the mount, `/` first.
    #[cfg(test)]
    pub fn relative(&self) -> &str {
        &self.relative
    }

    /// **The kill**: `1` written to `cgroup.kill` beneath the handle, which
    /// ends every process in `tools/`, one that left its group or session
    /// included.
    pub fn kill(&self) -> nix::Result<()> {
        let kill = self.open_for_write("cgroup.kill")?;
        match nix::unistd::write(&kill, b"1")? {
            1 => Ok(()),
            _ => Err(nix::Error::EIO),
        }
    }

    /// `cgroup.events`' `populated` line: `None` where it does not read.
    pub fn populated(&self) -> Option<bool> {
        let text = self.read("cgroup.events")?;
        text.lines()
            .find_map(|line| line.strip_prefix("populated "))
            .and_then(|value| match value.trim() {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            })
    }

    /// **The wait**: `true` once `tools/` reads `populated 0` within `bound`,
    /// a read that fails read as not yet empty (6.1).
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

    /// The pids `cgroup.procs` lists, none where it does not read.
    pub fn pids(&self) -> Vec<i32> {
        self.read("cgroup.procs")
            .map(|text| {
                text.lines()
                    .filter_map(|line| line.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// **The pinned fallback's one pass** (6.1): each pid `cgroup.procs`
    /// lists is pinned by `pidfd_open`, confirmed still in `tools/` by
    /// `/proc/<pid>/cgroup` read after that open, and only then sent
    /// `SIGKILL` through the pidfd, so a pid reused since the read, standing
    /// outside `tools/`, is never signalled. Answers whether a kill was
    /// delivered to what stands: `true` where `tools/` listed nothing or at
    /// least one pid was signalled.
    pub fn signal_pinned_pass(&self) -> bool {
        let pids = self.pids();
        if pids.is_empty() {
            return self.populated() != Some(true);
        }
        let mut delivered = false;
        for pid in pids {
            delivered |= signal_pinned(pid, &self.relative);
        }
        delivered
    }

    /// The fallback repeated within `bound` until `tools/` empties. Answers
    /// whether any pass delivered a kill, and whether it emptied.
    pub fn fallback_until_empty(&self, bound: Duration) -> (bool, bool) {
        let deadline = Instant::now() + bound;
        let mut delivered = false;
        loop {
            if self.populated() == Some(false) {
                return (true, true);
            }
            delivered |= self.signal_pinned_pass();
            if Instant::now() >= deadline {
                return (delivered, false);
            }
            std::thread::sleep(WAIT_SLICE);
        }
    }

    fn open_for_write(&self, name: &str) -> nix::Result<OwnedFd> {
        openat(
            &self.dir,
            name,
            OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
    }

    fn read(&self, name: &str) -> Option<String> {
        let fd = openat(
            &self.dir,
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .ok()?;
        std::io::read_to_string(std::fs::File::from(fd)).ok()
    }
}

impl AsFd for ToolsCgroup {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.dir.as_fd()
    }
}

/// Where the worker meets `tools/`: while serving, it waits a full bound from
/// its own kill; inside a leave, and at its unwind, it kills and does not
/// wait (6.1, the worker's rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Serving,
    Leave,
}

/// What the worker's containment came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Containment {
    /// Serving: `tools/` read `populated 0` within the bound.
    Emptied,
    /// Inside a leave: a kill was delivered, by the write or the pass, and
    /// nothing waited on it.
    Delivered,
    /// Serving: a kill was delivered and `tools/` did not empty within the
    /// bound, so service ends.
    PastBound,
    /// The write and the fallback both failed: no kill was delivered, and
    /// the `fault`'s account names the failed containment.
    Undelivered,
}

/// **The worker's rows of 6.1's table as one judgment**: the context, whether
/// the write of `cgroup.kill` succeeded, whether the pinned fallback
/// delivered a kill, and whether `tools/` read empty within the bound.
pub fn step(context: Context, written: bool, fallback: bool, emptied: bool) -> Containment {
    let delivered = written || fallback;
    match context {
        // The unwind and a leave's gate death: the write or one pass, no
        // wait, the call closed at once (rows 2678, 2679, 2683, 2684).
        Context::Leave if delivered => Containment::Delivered,
        Context::Leave => Containment::Undelivered,
        // Serving: emptied within the bound from its own first kill (row
        // 2677), the bound passing with a kill delivered (row 2680), or no
        // kill delivered by either (row 2682).
        Context::Serving if emptied => Containment::Emptied,
        Context::Serving if delivered => Containment::PastBound,
        Context::Serving => Containment::Undelivered,
    }
}

/// **The worker's containment of `tools/`**, by the context's rows: the
/// write of `cgroup.kill` first, the pinned fallback where it fails, and
/// while serving the wait of `bound` from its own first kill.
pub fn contain(tools: &ToolsCgroup, context: Context, bound: Duration) -> Containment {
    let written = tools.kill().is_ok();
    let (fallback, emptied) = match (context, written) {
        (Context::Serving, true) => (false, tools.wait_empty(bound)),
        (Context::Serving, false) => tools.fallback_until_empty(bound),
        (Context::Leave, true) => (false, false),
        (Context::Leave, false) => (tools.signal_pinned_pass(), false),
    };
    step(context, written, fallback, emptied)
}

/// A pid pinned by a pidfd, confirmed in `relative` after the open, then
/// sent `SIGKILL` through the pidfd. Answers whether the signal went.
fn signal_pinned(pid: i32, relative: &str) -> bool {
    let Some(pidfd) = pidfd_open(pid) else {
        return false;
    };
    if process_cgroup(pid).as_deref() != Some(relative) {
        return false;
    }
    pidfd_send_kill(&pidfd)
}

/// `pidfd_open(pid, 0)`, by number, wrapped at once as an owned descriptor.
fn pidfd_open(pid: i32) -> Option<OwnedFd> {
    // SAFETY: a raw system call by number with integer arguments; the result
    // is a new descriptor this process owns, or -1.
    let fd = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        return None;
    }
    // SAFETY: the kernel returned a fresh descriptor, owned by nothing else.
    Some(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

/// `pidfd_send_signal(pidfd, SIGKILL, NULL, 0)`, by number.
fn pidfd_send_kill(pidfd: &OwnedFd) -> bool {
    use std::os::fd::AsRawFd;
    // SAFETY: a raw system call on a descriptor this frame borrows, with a
    // null siginfo, which the kernel reads as a plain kill.
    unsafe {
        nix::libc::syscall(
            nix::libc::SYS_pidfd_send_signal,
            pidfd.as_raw_fd(),
            nix::libc::SIGKILL,
            std::ptr::null::<nix::libc::siginfo_t>(),
            0,
        ) == 0
    }
}

/// The unified hierarchy's path `/proc/<pid>/cgroup` names, `/` first.
fn process_cgroup(pid: i32) -> Option<String> {
    unified_line(&std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?)
}

/// The worker's own cgroup, `/proc/self/cgroup`'s unified line.
fn own_cgroup() -> Option<String> {
    unified_line(&std::fs::read_to_string("/proc/self/cgroup").ok()?)
}

fn unified_line(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(|relative| {
            let trimmed = relative.trim().trim_end_matches('/');
            if trimmed.starts_with('/') {
                trimmed.to_string()
            } else {
                format!("/{trimmed}")
            }
        })
}

#[cfg(test)]
pub(crate) mod tests;
