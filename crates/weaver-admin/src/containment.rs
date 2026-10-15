//! **The agent's cgroup**, per `weaver-admin-Spec` section 6.1 (#111): the
//! state table's rows that admin owns, as pure functions transcribed row for
//! row, and the primitives that act on a cgroup, each beneath a directory
//! descriptor and never by path.
//!
//! The rows are the start step's judgment, each step of the run end with its
//! success and its failure, the record's judgment at a later verb, and the
//! ownership rule (a cgroup is the record's run's, never the marker's). The
//! worker's and the gate's rows live in their own crates, which carry their
//! own small copy of the kill and the wait: no organ links another, and the
//! floor does no OS work (the Planner's ruling of 2026-10-14 on the plan).
//!
//! Uncalled until the wiring act (PR B of the #111 code act).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired into the start step and the run end by #111's PR B"
    )
)]

use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag, openat};
use nix::sys::stat::{Mode, fchmod, fstat, mkdirat};
use nix::unistd::{Gid, Uid, UnlinkatFlags, fchownat, unlinkat};

// ------------------------------------------------------------- the election

/// **The containment bound**, 6.1's election, provisional and measured: how
/// long a waiter waits for `populated 0` after a `cgroup.kill`, each waiter
/// from its own first kill.
pub const CONTAINMENT_BOUND: Duration = Duration::from_secs(30);

/// How often a wait reads `cgroup.events`.
const WAIT_SLICE: Duration = Duration::from_millis(10);

/// The mount of the unified hierarchy.
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// Every directory the start step makes is this mode, set by `fchmod`
/// whatever the umask (6.1, the modes).
pub const DIRECTORY_MODE: u32 = 0o755;

/// The leaves and the inner cgroup beneath the agent's, deepest first, the
/// order the run end removes them in.
pub const LAYOUT_DEEPEST_FIRST: [&str; 4] =
    ["worker/run", "worker/tools", "worker", "constituents"];

/// The three files delegated to the agent's account (6.1, C3).
pub const DELEGATED_FILES: [&str; 3] = [
    "worker/cgroup.procs",
    "worker/tools/cgroup.procs",
    "worker/tools/cgroup.kill",
];

/// The record's name in the run directory, and the temporary it is written
/// through.
pub const RECORD: &str = "cgroup";
pub const RECORD_TEMPORARY: &str = ".cgroup.tmp";

// ------------------------------------------------------------- the states

/// What stands where the start step would make the agent's cgroup, or where
/// a record names one: its `mkdir` meeting `EEXIST`, or the record's path
/// opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Absent,
    /// Root's and `populated 0`.
    Empty,
    /// Root's and populated.
    Populated,
    /// Not root's.
    NotRoots,
}

// ------------------------------------------------------------- the start step

/// The facts the start step judges, in the order it reads them (6.1, the
/// floor and the start's rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartFacts {
    pub run_lock_held: bool,
    pub cgroup2: bool,
    pub delegated: bool,
    /// At the deterministic path, the invoker's cgroup plus `weaver-<agent>/`.
    pub at_path: Standing,
    /// Where a record names a cgroup elsewhere (an earlier load from another
    /// containment), `Absent` where no record names one.
    pub recorded: Standing,
}

/// The start step's answer before it forks anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartAction {
    /// Make the agent's cgroup afresh (A0).
    Make,
    /// Remove the empty cgroup standing, with any record, then make afresh
    /// (A1).
    RemoveThenMake,
    Refuse(StartRefusal),
}

/// Why the start step refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRefusal {
    /// `AgentRunning`, at the run lock, before the cgroup is looked at (A2,
    /// or A4 with the lock held).
    RunLockHeld,
    /// `BoundaryUnverified`: no `cgroup2` mount.
    NoCgroup2,
    /// `BoundaryUnverified`: the invoker's cgroup carries no delegate mark.
    NotDelegated,
    /// `BoundaryUnverified`: a cgroup standing that is not root's.
    NotRoots,
    /// `AgentRunning`: an ended run's leftovers (A3, or A4 with the lock
    /// free), killing nothing (I8).
    Leftovers,
}

/// **The start step's judgment**, 6.1's start rows, in their order: the run
/// lock first (section 3), then the floor's mount and delegate mark before
/// any `mkdir`, then what stands at the path or the record.
pub fn start_judgment(facts: StartFacts) -> StartAction {
    if facts.run_lock_held {
        return StartAction::Refuse(StartRefusal::RunLockHeld);
    }
    if !facts.cgroup2 {
        return StartAction::Refuse(StartRefusal::NoCgroup2);
    }
    if !facts.delegated {
        return StartAction::Refuse(StartRefusal::NotDelegated);
    }
    match (facts.at_path, facts.recorded) {
        (Standing::NotRoots, _) | (_, Standing::NotRoots) => {
            StartAction::Refuse(StartRefusal::NotRoots)
        }
        (Standing::Populated, _) | (_, Standing::Populated) => {
            StartAction::Refuse(StartRefusal::Leftovers)
        }
        (Standing::Empty, _) | (_, Standing::Empty) => StartAction::RemoveThenMake,
        (Standing::Absent, Standing::Absent) => StartAction::Make,
    }
}

/// The step of the start that made the cgroup, and what it found: a
/// `cgroup.kill` in what it made (the floor's third question), and the
/// `fchmod`, owner and record writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadeStep {
    KillFilePresent(bool),
    ModesSet(bool),
    OwnersSet(bool),
    RecordWritten(bool),
}

/// After the `mkdir`: go on, or refuse `BoundaryUnverified`, removing what
/// was made and standing nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadeAction {
    GoOn,
    RemoveAndRefuse,
}

/// 6.1's rows for the floor's `cgroup.kill` and the start's `fchmod`, owner
/// and record writes: each failure removes what the start made and refuses.
pub fn made_judgment(step: MadeStep) -> MadeAction {
    match step {
        MadeStep::KillFilePresent(true)
        | MadeStep::ModesSet(true)
        | MadeStep::OwnersSet(true)
        | MadeStep::RecordWritten(true) => MadeAction::GoOn,
        MadeStep::KillFilePresent(false)
        | MadeStep::ModesSet(false)
        | MadeStep::OwnersSet(false)
        | MadeStep::RecordWritten(false) => MadeAction::RemoveAndRefuse,
    }
}

// ------------------------------------------------------------- ownership

/// **Ownership is the record's, never the marker's** (6.1): a run end ends
/// only the cgroup its record names as made for the run being concluded. A
/// rollback passes its own load's run, which made the cgroup.
pub fn owns(record_run: &str, concluded_run: &str) -> bool {
    record_run == concluded_run
}

// ------------------------------------------------------------- the record

/// What a later verb finds at the recorded path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedAt {
    /// The record does not read, or names anything but a `weaver-<agent>`
    /// directory on the `cgroup2` mount.
    Unreadable,
    /// Nothing at the recorded path.
    Absent,
    /// The cgroup the record names, its device and inode the recorded ones.
    Same,
    /// A cgroup with another device and inode.
    Other,
}

/// A later verb's answer to its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordAction {
    /// The recorded cgroup is the one to act on.
    Proceed,
    /// Absent is A0, already removed: remove the stale record and go on,
    /// acting on nothing there.
    RemoveStaleAndGoOn,
    /// `BoundaryUnverified`, naming the record or both identities, acting on
    /// nothing.
    Refuse,
}

/// 6.1's rows for a later verb's record.
pub fn record_judgment(found: RecordedAt) -> RecordAction {
    match found {
        RecordedAt::Same => RecordAction::Proceed,
        RecordedAt::Absent => RecordAction::RemoveStaleAndGoOn,
        RecordedAt::Unreadable | RecordedAt::Other => RecordAction::Refuse,
    }
}

// ------------------------------------------------------------- the run end

/// One step of the run end, with whether it succeeded (6.1, the run end and
/// its failures, a rollback's included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEndStep {
    /// The write of `cgroup.kill` on the agent's cgroup.
    Kill { written: bool },
    /// The pinned fallback over the agent's cgroup and its descendants,
    /// repeated within the bound, where the write failed.
    Fallback { delivered: bool },
    /// The wait for `populated 0` within the bound.
    Wait { emptied: bool },
    /// The removal, deepest first, `ENOENT` read as removed.
    Remove { removed: bool },
    /// The record's removal.
    RemoveRecord { removed: bool },
}

/// What the run end does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEndAction {
    Fallback,
    Wait,
    Remove,
    RemoveRecord,
    /// The run end is done: publish, write the marker and answer.
    Conclude,
    /// The record's removal failed: say it in `admin.log`, refuse nothing,
    /// and conclude. The next verb reads the absent path as A0.
    LogAndConclude,
    /// `WorkerWouldNotExit`, naming the pids: nothing published, no marker.
    RefuseWorkerWouldNotExit,
    /// `BoundaryUnverified`, naming the cgroup: its record kept, nothing
    /// published, no marker.
    RefuseBoundaryUnverified,
}

/// **The run end's rows**, each step's success and failure, 6.1.
pub fn run_end_next(step: RunEndStep) -> RunEndAction {
    match step {
        RunEndStep::Kill { written: true } => RunEndAction::Wait,
        RunEndStep::Kill { written: false } => RunEndAction::Fallback,
        RunEndStep::Fallback { delivered: true } => RunEndAction::Wait,
        RunEndStep::Fallback { delivered: false } => RunEndAction::RefuseWorkerWouldNotExit,
        RunEndStep::Wait { emptied: true } => RunEndAction::Remove,
        RunEndStep::Wait { emptied: false } => RunEndAction::RefuseWorkerWouldNotExit,
        RunEndStep::Remove { removed: true } => RunEndAction::RemoveRecord,
        RunEndStep::Remove { removed: false } => RunEndAction::RefuseBoundaryUnverified,
        RunEndStep::RemoveRecord { removed: true } => RunEndAction::Conclude,
        RunEndStep::RemoveRecord { removed: false } => RunEndAction::LogAndConclude,
    }
}

// ------------------------------------------------------------- the primitives

/// A cgroup held by its directory descriptor, with its path beneath the
/// mount (as `/proc/<pid>/cgroup` names it) for the pinned fallback's check.
#[derive(Debug)]
pub struct Cgroup {
    dir: OwnedFd,
    relative: String,
}

impl Cgroup {
    /// The descriptor every act goes through.
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.dir.as_fd()
    }

    /// The path beneath the mount, `/` first.
    pub fn relative(&self) -> &str {
        &self.relative
    }

    /// The device and inode `fstat` reads on the directory.
    pub fn identity(&self) -> std::io::Result<(u64, u64)> {
        let stat = fstat(self.dir.as_fd()).map_err(std::io::Error::from)?;
        Ok((stat.st_dev, stat.st_ino))
    }

    /// Opens a cgroup by its path beneath the mount, the one open by path:
    /// the invoker's own cgroup, or the record's.
    pub fn open(relative: &str) -> std::io::Result<Self> {
        let path = PathBuf::from(CGROUP_ROOT).join(relative.trim_start_matches('/'));
        let dir = nix::fcntl::open(
            &path,
            OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_RDONLY,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        Ok(Cgroup {
            dir,
            relative: normalized(relative),
        })
    }

    /// Opens a child cgroup beneath this one's descriptor.
    pub fn child(&self, name: &str) -> std::io::Result<Self> {
        let dir = openat(
            self.dir.as_fd(),
            name,
            OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_RDONLY,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        Ok(Cgroup {
            dir,
            relative: format!("{}/{}", self.relative.trim_end_matches('/'), name),
        })
    }

    /// **Makes a child cgroup and sets its mode through its descriptor**,
    /// whatever the umask (6.1, the modes). `EEXIST` comes back as
    /// `Errno::EEXIST` for the start step's judgment.
    pub fn make_child(&self, name: &str) -> nix::Result<Self> {
        mkdirat(
            self.dir.as_fd(),
            name,
            Mode::from_bits_truncate(DIRECTORY_MODE),
        )?;
        let made = self.child(name).map_err(io_errno)?;
        fchmod(made.dir.as_fd(), Mode::from_bits_truncate(DIRECTORY_MODE))?;
        Ok(made)
    }

    /// Whether the interface file stands, read beneath the descriptor.
    pub fn has_file(&self, name: &str) -> bool {
        nix::sys::stat::fstatat(self.dir.as_fd(), name, AtFlags::AT_SYMLINK_NOFOLLOW).is_ok()
    }

    /// Sets one interface file's owner, beneath the descriptor.
    pub fn chown_file(&self, name: &str, uid: u32, gid: u32) -> nix::Result<()> {
        fchownat(
            self.dir.as_fd(),
            name,
            Some(Uid::from_raw(uid)),
            Some(Gid::from_raw(gid)),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )
    }

    /// Writes an interface file, opened beneath the descriptor.
    fn write_file(&self, name: &str, bytes: &[u8]) -> std::io::Result<()> {
        let fd = openat(
            self.dir.as_fd(),
            name,
            OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        std::fs::File::from(fd).write_all(bytes)
    }

    /// Reads an interface file, opened beneath the descriptor.
    fn read_file(&self, name: &str) -> std::io::Result<String> {
        let fd = openat(
            self.dir.as_fd(),
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        let mut text = String::new();
        std::fs::File::from(fd).read_to_string(&mut text)?;
        Ok(text)
    }

    /// **The kill**: `1` written to `cgroup.kill`, which signals every process
    /// of the cgroup and its descendants with `SIGKILL`.
    pub fn kill(&self) -> std::io::Result<()> {
        self.write_file("cgroup.kill", b"1")
    }

    /// **`cgroup.procs` opened for a child's own move**, before the fork: the
    /// child writes `0` to it in `pre_exec` (see `place_self`), so it moves
    /// itself in before its exec and no instruction of its own runs outside
    /// (6.1, C4).
    pub fn procs_for_placement(&self) -> std::io::Result<OwnedFd> {
        openat(
            self.dir.as_fd(),
            "cgroup.procs",
            OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)
    }

    /// `populated` from `cgroup.events`: `None` where the read fails, which
    /// every waiter reads as not yet empty (6.1).
    pub fn populated(&self) -> Option<bool> {
        let events = self.read_file("cgroup.events").ok()?;
        events
            .lines()
            .find_map(|line| line.strip_prefix("populated "))
            .map(|value| value.trim() != "0")
    }

    /// **The wait**: `populated 0` within `bound`, a failed read being read as
    /// not yet empty. Answers whether the cgroup emptied.
    pub fn wait_empty(&self, bound: Duration) -> bool {
        let deadline = Instant::now() + bound;
        loop {
            if self.populated() == Some(false) {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            std::thread::sleep(WAIT_SLICE.min(remaining));
        }
    }

    /// The pids `cgroup.procs` lists for this cgroup alone.
    pub fn procs(&self) -> std::io::Result<Vec<i32>> {
        Ok(self
            .read_file("cgroup.procs")?
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect())
    }

    /// This cgroup and every descendant, by descriptor, for the fallback.
    pub fn subtree(&self) -> std::io::Result<Vec<Cgroup>> {
        let mut found = vec![self.child(".")?];
        found[0].relative = self.relative.clone();
        let mut at = 0;
        while at < found.len() {
            let dir = nix::dir::Dir::openat(
                found[at].dir.as_fd(),
                ".",
                OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?;
            let mut names = Vec::new();
            for entry in dir.into_iter().flatten() {
                if entry.file_type() == Some(nix::dir::Type::Directory) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name != "." && name != ".." {
                        names.push(name);
                    }
                }
            }
            for name in names {
                let child = found[at].child(&name)?;
                found.push(child);
            }
            at += 1;
        }
        Ok(found)
    }

    /// **The pinned fallback**, one pass over this cgroup and its
    /// descendants: each pid read from `cgroup.procs` is pinned by a pidfd,
    /// confirmed through `/proc/<pid>/cgroup`, read after the open, to stand
    /// in the cgroup it was read from, then sent `SIGKILL` through the pidfd.
    /// A pid failing any step is skipped. Answers whether every pid read was
    /// signalled and every listing read.
    pub fn signal_pinned_pass(&self) -> bool {
        let Ok(cgroups) = self.subtree() else {
            return false;
        };
        let mut all = true;
        for cgroup in &cgroups {
            let Ok(pids) = cgroup.procs() else {
                all = false;
                continue;
            };
            for pid in pids {
                all &= signal_pinned(pid, &cgroup.relative);
            }
        }
        all
    }

    /// The fallback repeated within `bound` until the cgroup empties.
    /// Answers whether it emptied.
    pub fn fallback_until_empty(&self, bound: Duration) -> bool {
        let deadline = Instant::now() + bound;
        loop {
            self.signal_pinned_pass();
            if self.populated() == Some(false) {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            std::thread::sleep(WAIT_SLICE.min(remaining));
        }
    }
}

/// **The child's own move**, for `pre_exec`: `0` written to the
/// `cgroup.procs` descriptor `procs_for_placement` opened before the fork.
/// Async-signal-safe: one `write` on a static buffer, allocating nothing.
pub fn place_self(procs: RawFd) -> std::io::Result<()> {
    // SAFETY: write is async-signal-safe, on a descriptor opened before the
    // fork, with a static buffer.
    if unsafe { nix::libc::write(procs, b"0".as_ptr().cast(), 1) } == 1 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// A pid pinned by a pidfd and confirmed in `relative` after the open, then
/// signalled through the pidfd. The two pidfd calls are the stated exception
/// to the election (`weaver-admin-Spec` section 1), by number through `nix`'s
/// re-exported `libc`.
fn signal_pinned(pid: i32, relative: &str) -> bool {
    // SAFETY: pidfd_open takes a pid and flags and answers a descriptor.
    let raw = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_open, pid, 0) };
    if raw < 0 {
        return false;
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw as RawFd) };
    if process_cgroup(pid).as_deref() != Some(relative) {
        return false;
    }
    // SAFETY: pidfd_send_signal on a pidfd this frame owns, with no siginfo.
    let sent = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_pidfd_send_signal,
            pidfd.as_raw_fd(),
            nix::libc::SIGKILL,
            std::ptr::null::<nix::libc::siginfo_t>(),
            0,
        )
    };
    sent == 0
}

/// The unified hierarchy's path `/proc/<pid>/cgroup` names, `/` first.
fn process_cgroup(pid: i32) -> Option<String> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(normalized)
}

/// The invoking process's own cgroup, `/proc/self/cgroup`'s unified line.
pub fn own_cgroup() -> std::io::Result<String> {
    let text = std::fs::read_to_string("/proc/self/cgroup")?;
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(normalized)
        .ok_or_else(|| std::io::Error::other("no unified hierarchy line in /proc/self/cgroup"))
}

fn normalized(relative: &str) -> String {
    let trimmed = relative.trim().trim_end_matches('/');
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn io_errno(error: std::io::Error) -> Errno {
    Errno::from_raw(error.raw_os_error().unwrap_or(nix::libc::EIO))
}

/// **The floor's mount**: a `cgroup2` filesystem at the mount, by `statfs`.
pub fn is_cgroup2() -> bool {
    nix::sys::statfs::statfs(CGROUP_ROOT)
        .is_ok_and(|fs| fs.filesystem_type() == nix::sys::statfs::CGROUP2_SUPER_MAGIC)
}

/// **systemd's delegate mark** on a cgroup, read as root: `user.delegate`
/// reading `1`, or `trusted.delegate` where set. `fgetxattr` is the stated
/// exception to the election (`weaver-admin-Spec` section 1), `nix` 0.31
/// having no extended attributes.
pub fn delegated(cgroup: &Cgroup) -> bool {
    ["user.delegate", "trusted.delegate"]
        .iter()
        .any(|name| xattr_reads_one(cgroup.fd(), name))
}

fn xattr_reads_one(fd: BorrowedFd<'_>, name: &str) -> bool {
    let Ok(name) = std::ffi::CString::new(name) else {
        return false;
    };
    let mut value = [0u8; 8];
    // SAFETY: fgetxattr on a descriptor this frame borrows, writing at most
    // the buffer's length into it.
    let read = unsafe {
        nix::libc::fgetxattr(
            fd.as_raw_fd(),
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            value.len(),
        )
    };
    read > 0 && value[..read as usize].trim_ascii() == b"1"
}

/// **Removes the agent's cgroup, deepest first**, `ENOENT` read as removed.
/// Answers the first other failure.
pub fn remove_layout(parent: &Cgroup, name: &str) -> nix::Result<()> {
    for inner in LAYOUT_DEEPEST_FIRST {
        let path = format!("{name}/{inner}");
        match unlinkat(parent.fd(), path.as_str(), UnlinkatFlags::RemoveDir) {
            Ok(()) | Err(Errno::ENOENT) => {}
            Err(error) => return Err(error),
        }
    }
    match unlinkat(parent.fd(), name, UnlinkatFlags::RemoveDir) {
        Ok(()) | Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(error),
    }
}

/// **Makes the layout beneath the agent's cgroup** (6.1): `constituents/`,
/// `worker/`, `worker/run/`, `worker/tools/`, each `0755` by `fchmod`, and the
/// three delegated files set to the agent's account. Answers the step that
/// failed, for `made_judgment`.
pub fn make_layout(agent: &Cgroup, uid: u32, gid: u32) -> Result<(), MadeStep> {
    let constituents = agent.make_child("constituents");
    let worker = agent.make_child("worker");
    let (Ok(_), Ok(worker)) = (constituents, worker) else {
        return Err(MadeStep::ModesSet(false));
    };
    let (Ok(_), Ok(_)) = (worker.make_child("run"), worker.make_child("tools")) else {
        return Err(MadeStep::ModesSet(false));
    };
    for file in DELEGATED_FILES {
        if agent.chown_file(file, uid, gid).is_err() {
            return Err(MadeStep::OwnersSet(false));
        }
    }
    Ok(())
}

// ------------------------------------------------------------- the record's file

/// **The record**: the agent's cgroup's path beneath the mount, the device and
/// inode `fstat` read on it, and the run its load made it for (6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub path: String,
    pub dev: u64,
    pub ino: u64,
    pub run: String,
}

/// **Writes the record atomically** in the run directory: a temporary,
/// `0600`, synced, renamed over `cgroup`, and the directory synced.
pub fn write_record(run_directory: BorrowedFd<'_>, record: &Record) -> std::io::Result<()> {
    let _ = unlinkat(run_directory, RECORD_TEMPORARY, UnlinkatFlags::NoRemoveDir);
    let fd = openat(
        run_directory,
        RECORD_TEMPORARY,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(std::io::Error::from)?;
    fchmod(fd.as_fd(), Mode::from_bits_truncate(0o600)).map_err(std::io::Error::from)?;
    let mut file = std::fs::File::from(fd);
    let text = serde_json::json!({
        "path": record.path,
        "dev": record.dev,
        "ino": record.ino,
        "run": record.run,
    })
    .to_string();
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    nix::fcntl::renameat(run_directory, RECORD_TEMPORARY, run_directory, RECORD)
        .map_err(std::io::Error::from)?;
    nix::unistd::fsync(run_directory).map_err(std::io::Error::from)
}

/// Reads the record: `Ok(None)` where none stands, `Err` where it does not
/// read or names anything but a `weaver-<agent>` directory.
pub fn read_record(run_directory: BorrowedFd<'_>, agent: &str) -> Result<Option<Record>, ()> {
    let fd = match openat(
        run_directory,
        RECORD,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::ENOENT) => return Ok(None),
        Err(_) => return Err(()),
    };
    let mut text = String::new();
    std::fs::File::from(fd)
        .read_to_string(&mut text)
        .map_err(|_| ())?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| ())?;
    let record = Record {
        path: value["path"].as_str().ok_or(())?.to_string(),
        dev: value["dev"].as_u64().ok_or(())?,
        ino: value["ino"].as_u64().ok_or(())?,
        run: value["run"].as_str().ok_or(())?.to_string(),
    };
    let named = Path::new(&record.path)
        .file_name()
        .and_then(|name| name.to_str())
        == Some(format!("weaver-{agent}").as_str());
    if !named || !record.path.starts_with('/') {
        return Err(());
    }
    Ok(Some(record))
}

/// Removes the record and a temporary left behind.
pub fn remove_record(run_directory: BorrowedFd<'_>) -> nix::Result<()> {
    let _ = unlinkat(run_directory, RECORD_TEMPORARY, UnlinkatFlags::NoRemoveDir);
    match unlinkat(run_directory, RECORD, UnlinkatFlags::NoRemoveDir) {
        Ok(()) | Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(error),
    }
}

/// **What a later verb finds at the record's path**, for `record_judgment`.
pub fn recorded_at(record: &Record) -> RecordedAt {
    match Cgroup::open(&record.path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => RecordedAt::Absent,
        Err(_) => RecordedAt::Unreadable,
        Ok(cgroup) => match cgroup.identity() {
            Ok(identity) if identity == (record.dev, record.ino) => RecordedAt::Same,
            Ok(_) => RecordedAt::Other,
            Err(_) => RecordedAt::Unreadable,
        },
    }
}

#[cfg(test)]
mod tests;
