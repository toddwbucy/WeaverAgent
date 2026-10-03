//! The start step, the run lock and the invocation lock, per
//! `weaver-admin-Spec` sections 3 and 6, on the operator's ruling of
//! 2026-10-03 on #50 that the agent leaves systemd.
//!
//! **Two locks the kernel holds answer the two questions a per-invocation
//! crate cannot keep across verbs.** The invocation lock, `admin.lock`, is a
//! classic POSIX record lock, owned by one process and never inherited by a
//! fork, so no child of the start step ever holds it. The run lock,
//! `run.lock`, is an open file description lock, taken before the first fork
//! and inherited by the worker, the state member and the trace relay, so the
//! agent counts as running while any of them holds it. Both stand in the
//! agent's root-owned run directory, `<coordination-root>/weaver.run/<agent>/`.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use weaver_types::LifecycleRefusal;

/// The run lock's number in every constituent, per `weaver-admin-Spec`
/// section 3.
pub const RUN_LOCK_FD: RawFd = 9;

/// The relay's lifetime pipe's write end in the worker, per section 6.
pub const RELAY_LIFETIME_FD: RawFd = 8;

/// How long an exclusive taker waits behind `show`'s shared holds before it
/// refuses, per section 3: `show`'s own bound, the dial's.
const SHARED_HOLD_WAIT: Duration = Duration::from_secs(1);

/// How often the waits below look again.
const POLL: Duration = Duration::from_millis(20);

/// The escalation's three fixed waits, per section 3: after `left`, from
/// `SIGTERM` to `SIGKILL`, and the last read after `SIGKILL`.
pub const AFTER_LEFT: Duration = Duration::from_secs(30);
pub const TERM_GRACE: Duration = Duration::from_secs(10);
pub const KILL_GRACE: Duration = Duration::from_secs(5);

/// The agent's run directory, `<coordination-root>/weaver.run/<agent>/`, in a
/// namespace no agent name can produce, a name carrying no `.`.
pub fn run_directory(coordination_root: &Path, agent: &str) -> PathBuf {
    coordination_root.join("weaver.run").join(agent)
}

/// The agent's runtime directory, `<coordination-root>/weaver-<agent>/`,
/// where the worker binds its coordination and gate sockets.
pub fn runtime_directory(coordination_root: &Path, agent: &str) -> PathBuf {
    coordination_root.join(format!("weaver-{agent}"))
}

/// **Makes and judges the run directory**, every verb's first act after the
/// root's admission, per section 3: `weaver.run/` and `weaver.run/<agent>/`
/// made where absent, opened without following a link, and set to root's
/// ownership and `0755` where they stand. Answers the agent's run directory.
pub fn prepare_run_directory(
    coordination_root: &Path,
    agent: &str,
    owner: u32,
) -> Result<PathBuf, LifecycleRefusal> {
    let parent = coordination_root.join("weaver.run");
    hold_directory(&parent, owner, owner, 0o755)?;
    let directory = parent.join(agent);
    hold_directory(&directory, owner, owner, 0o755)?;
    Ok(directory)
}

/// Makes `path` if absent and sets its owner, group and mode, refusing a
/// link or anything that is not a directory. The judgment and the repair go
/// through one descriptor opened without following a link, so a name swapped
/// between them is never the one repaired.
pub fn hold_directory(path: &Path, uid: u32, gid: u32, mode: u32) -> Result<(), LifecycleRefusal> {
    match std::fs::symlink_metadata(path) {
        // **Made at its final mode, never wider**: the creating call carries
        // the mode, which a umask can only narrow, so no inherited umask leaves
        // a window in which another principal could plant an entry before the
        // repair below.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            use std::os::unix::fs::DirBuilderExt;
            let made = std::fs::DirBuilder::new().mode(mode).create(path);
            match made {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(refuse_path("cannot be made", path)),
            }
        }
        Err(_) => return Err(refuse_path("cannot be read", path)),
        Ok(_) => {}
    }
    let directory = open_directory(path)?;
    let fd = directory.as_raw_fd();
    // SAFETY: fchown and fchmod on a descriptor this frame owns.
    let held = unsafe { nix::libc::fchown(fd, uid, gid) == 0 && nix::libc::fchmod(fd, mode) == 0 };
    if !held {
        return Err(refuse_path("cannot be set to its owner and mode", path));
    }
    Ok(())
}

/// Opens a directory without following a link at its last component.
fn open_directory(path: &Path) -> Result<OwnedFd, LifecycleRefusal> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let flags =
        nix::libc::O_RDONLY | nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC;
    // SAFETY: open with a NUL-terminated path and no mode.
    let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags) };
    if fd == -1 {
        return Err(refuse_path(
            "is not a directory opened without a link",
            path,
        ));
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn refuse_path(what: &str, path: &Path) -> LifecycleRefusal {
    diag!("weaver-admin: {} {what}", path.display());
    LifecycleRefusal::BoundaryUnverified
}

/// Opens a lock file in the run directory, root's and `0600`, without
/// following a link, close-on-exec at creation.
fn open_lock_file(path: &Path) -> Result<OwnedFd, LifecycleRefusal> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
    let flags =
        nix::libc::O_RDWR | nix::libc::O_CREAT | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC;
    // SAFETY: open with a NUL-terminated path and a mode for O_CREAT.
    let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags, 0o600 as nix::libc::c_uint) };
    if fd == -1 {
        return Err(refuse_path("cannot be opened as a lock file", path));
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// A whole-file lock request of the given kind.
fn whole_file(kind: i32) -> nix::libc::flock {
    nix::libc::flock {
        l_type: kind as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    }
}

/// The invocation lock this process holds until it exits. A classic record
/// lock, released by the kernel when this process ends and never held by a
/// child.
#[derive(Debug)]
pub struct InvocationLock {
    _file: OwnedFd,
}

/// What the shared take answered: held for `show`'s observation, or another
/// invocation is changing the agent.
#[derive(Debug)]
pub enum Shared {
    Held(InvocationLock),
    InTransition,
}

/// **Takes the invocation lock exclusively**, every verb but `show`, per
/// section 3. Refused, it reads the holder's kind: an exclusive holder is
/// another transition and refuses `InvocationInFlight` at once, and a shared
/// holder is a `show` and is retried within `show`'s bound, judged afresh on
/// every attempt, the starved case refusing with readers named as the cause.
pub fn take_invocation_lock(run_directory: &Path) -> Result<InvocationLock, LifecycleRefusal> {
    let file = open_lock_file(&run_directory.join("admin.lock"))?;
    let deadline = Instant::now() + SHARED_HOLD_WAIT;
    loop {
        let request = whole_file(nix::libc::F_WRLCK);
        // SAFETY: fcntl on a descriptor this frame owns, with a lock it built.
        if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_SETLK, &request) } == 0 {
            return Ok(InvocationLock { _file: file });
        }
        let mut probe = whole_file(nix::libc::F_WRLCK);
        // SAFETY: as above.
        if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_GETLK, &mut probe) } != 0 {
            return Err(LifecycleRefusal::InvocationInFlight);
        }
        match probe.l_type as i32 {
            nix::libc::F_WRLCK => return Err(LifecycleRefusal::InvocationInFlight),
            nix::libc::F_RDLCK if Instant::now() < deadline => std::thread::sleep(POLL),
            nix::libc::F_RDLCK => {
                diag!(
                    "weaver-admin: readers held the invocation lock past show's bound; \
                     retry once the poller pauses"
                );
                return Err(LifecycleRefusal::InvocationInFlight);
            }
            // Released between the take and the read: try again at once.
            _ => {}
        }
    }
}

/// **Takes the invocation lock shared, without waiting**, `show`'s take, per
/// section 3: refused where an exclusive holder stands, which is a
/// transition in flight.
pub fn take_shared_invocation_lock(run_directory: &Path) -> Result<Shared, LifecycleRefusal> {
    let file = open_lock_file(&run_directory.join("admin.lock"))?;
    let request = whole_file(nix::libc::F_RDLCK);
    // SAFETY: fcntl on a descriptor this frame owns, with a lock it built.
    if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_SETLK, &request) } == 0 {
        Ok(Shared::Held(InvocationLock { _file: file }))
    } else {
        Ok(Shared::InTransition)
    }
}

/// The run lock this invocation took: an open file description lock, which
/// every constituent the start step forks inherits.
#[derive(Debug)]
pub struct RunLock {
    file: OwnedFd,
}

impl RunLock {
    /// The descriptor a spawn path re-arms onto `RUN_LOCK_FD` in its child.
    pub fn raw(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

/// **Takes the run lock, without waiting**, per section 3: answers the lock
/// where no run holds it, and `None` where one does. Taken by the invocation
/// itself before it forks anything, so no interval leaves a constituent
/// alive with the lock free.
pub fn take_run_lock(run_directory: &Path) -> Result<Option<RunLock>, LifecycleRefusal> {
    let file = open_lock_file(&run_directory.join("run.lock"))?;
    let request = whole_file(nix::libc::F_WRLCK);
    // SAFETY: fcntl on a descriptor this frame owns, with a lock it built.
    if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_OFD_SETLK, &request) } == 0 {
        Ok(Some(RunLock { file }))
    } else {
        Ok(None)
    }
}

/// Whether any open file description holds the run lock, read through a
/// fresh one with `F_OFD_GETLK`. A run directory with no lock file has no
/// run.
pub fn run_lock_held(run_directory: &Path) -> Result<bool, LifecycleRefusal> {
    let path = run_directory.join("run.lock");
    // **Only an absent file is an absent run**: any other failure to look is
    // a refusal, never a report that the lock is free.
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(refuse_path("cannot be read", &path)),
        Ok(_) => {}
    }
    let file = open_lock_file(&path)?;
    let mut probe = whole_file(nix::libc::F_WRLCK);
    // SAFETY: fcntl on a descriptor this frame owns.
    if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_OFD_GETLK, &mut probe) } != 0 {
        return Err(refuse_path("cannot be queried", &path));
    }
    Ok(probe.l_type as i32 != nix::libc::F_UNLCK)
}

/// One process holding a descriptor to the run lock's file: its pid and the
/// number the descriptor stands at in its table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holder {
    pub pid: i32,
    pub fd: i32,
}

/// **The run lock's holders, from the kernel's descriptor tables**, per
/// section 3: a description lock names no pid, so every process's
/// `/proc/<pid>/fd` is scanned for a descriptor referring to the lock file's
/// device and inode, this process excluded. Root reads every table.
pub fn holders(run_directory: &Path) -> Vec<Holder> {
    let Ok(lock) = std::fs::metadata(run_directory.join("run.lock")) else {
        return Vec::new();
    };
    let target = (lock.dev(), lock.ino());
    let own = std::process::id() as i32;
    let mut found = Vec::new();
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return found;
    };
    for process in processes.flatten() {
        let Some(pid) = process
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<i32>().ok())
        else {
            continue;
        };
        if pid == own {
            continue;
        }
        let Ok(descriptors) = std::fs::read_dir(process.path().join("fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten() {
            let Some(fd) = descriptor
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<i32>().ok())
            else {
                continue;
            };
            if std::fs::metadata(descriptor.path()).is_ok_and(|m| (m.dev(), m.ino()) == target) {
                found.push(Holder { pid, fd });
            }
        }
    }
    found
}

/// A pidfd on one holder, confirmed after opening to hold the same file, so
/// a pid that exited, or was reused, since the scan is never signalled.
struct Pinned {
    pidfd: OwnedFd,
}

fn pin(holder: Holder, target: (u64, u64)) -> Option<Pinned> {
    // SAFETY: pidfd_open takes a pid and flags and answers a descriptor.
    let raw = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_open, holder.pid, 0) };
    if raw < 0 {
        return None;
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw as RawFd) };
    let path = format!("/proc/{}/fd/{}", holder.pid, holder.fd);
    std::fs::metadata(path)
        .ok()
        .filter(|m| (m.dev(), m.ino()) == target)
        .map(|_| Pinned { pidfd })
}

fn signal(pinned: &Pinned, signal: i32) {
    // SAFETY: pidfd_send_signal on a pidfd this frame owns, with no siginfo.
    unsafe {
        nix::libc::syscall(
            nix::libc::SYS_pidfd_send_signal,
            pinned.pidfd.as_raw_fd(),
            signal,
            std::ptr::null::<nix::libc::siginfo_t>(),
            0,
        );
    }
}

/// Waits up to `bound` for the run lock to be free, answering whether it is.
pub fn wait_free(run_directory: &Path, bound: Duration) -> bool {
    let deadline = Instant::now() + bound;
    loop {
        if !run_lock_held(run_directory).unwrap_or(true) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}

/// **The escalation**, per section 3: every holder found and pinned is sent
/// `SIGTERM`, every holder still standing `SIGKILL` after the grace, and the
/// lock is read a last time after that. Refuses `LockHolderUnknown` where
/// the lock is held and no holder can be found, and `WorkerWouldNotExit`
/// where it is still held at the end. Answers once the lock is free.
pub fn escalate(run_directory: &Path) -> Result<(), LifecycleRefusal> {
    escalate_within(run_directory, TERM_GRACE, KILL_GRACE)
}

pub fn escalate_within(
    run_directory: &Path,
    term_grace: Duration,
    kill_grace: Duration,
) -> Result<(), LifecycleRefusal> {
    if !run_lock_held(run_directory)? {
        return Ok(());
    }
    // The lock reads held, so a file that cannot be looked at now is a fault,
    // never an ended run.
    let lock = std::fs::metadata(run_directory.join("run.lock"))
        .map_err(|_| refuse_path("cannot be read", &run_directory.join("run.lock")))?;
    let target = (lock.dev(), lock.ino());
    for (grace, sig) in [
        (term_grace, nix::libc::SIGTERM),
        (kill_grace, nix::libc::SIGKILL),
    ] {
        let found = holders(run_directory);
        if found.is_empty() {
            if !run_lock_held(run_directory)? {
                return Ok(());
            }
            diag!("weaver-admin: the run lock is held and no process holding it can be found");
            return Err(LifecycleRefusal::LockHolderUnknown);
        }
        for holder in found {
            if let Some(pinned) = pin(holder, target) {
                signal(&pinned, sig);
            }
        }
        if wait_free(run_directory, grace) {
            return Ok(());
        }
    }
    Err(LifecycleRefusal::WorkerWouldNotExit)
}

/// **The catchable signals that end a process by default and carry no fault**,
/// per `weaver-admin-Spec` section 2: the invocation ignores every one from its
/// first instruction, so neither a caller's hangup, interrupt or termination,
/// which sudo relays, nor a stray user signal ends a verb part way, and every
/// child the start step forks resets each to its default before its exec. The
/// synchronous faults (`SIGSEGV`, `SIGBUS`, `SIGFPE`, `SIGILL`, `SIGTRAP`,
/// `SIGSYS`) are not here, since ignoring a fault turns it into a loop, and
/// `SIGKILL` and `SIGSTOP` cannot be caught.
pub const IGNORED_SIGNALS: &[i32] = &[
    nix::libc::SIGHUP,
    nix::libc::SIGINT,
    nix::libc::SIGQUIT,
    nix::libc::SIGPIPE,
    nix::libc::SIGTERM,
    nix::libc::SIGUSR1,
    nix::libc::SIGUSR2,
    nix::libc::SIGALRM,
    nix::libc::SIGVTALRM,
    nix::libc::SIGPROF,
    nix::libc::SIGIO,
    nix::libc::SIGPWR,
    nix::libc::SIGSTKFLT,
    nix::libc::SIGXCPU,
    nix::libc::SIGXFSZ,
    nix::libc::SIGABRT,
];

/// Ignores every signal of `IGNORED_SIGNALS`, the invocation's first act.
pub fn ignore_terminating_signals() {
    for &sig in IGNORED_SIGNALS {
        // SAFETY: setting a disposition to SIG_IGN for a catchable signal.
        unsafe { nix::libc::signal(sig, nix::libc::SIG_IGN) };
    }
}

/// **A child's reset before its exec**: a new session first, so the child
/// belongs to no terminal's process group, then every ignored signal back at
/// its default and the signal mask cleared, since an ignored disposition
/// survives a fork and an exec. Async-signal-safe, for a pre-exec.
pub(crate) fn detach_and_reset() -> std::io::Result<()> {
    // SAFETY: setsid, signal and sigprocmask are async-signal-safe.
    unsafe {
        if nix::libc::setsid() == -1 {
            return Err(std::io::Error::last_os_error());
        }
        for &sig in IGNORED_SIGNALS {
            nix::libc::signal(sig, nix::libc::SIG_DFL);
        }
        let mut empty: nix::libc::sigset_t = std::mem::zeroed();
        nix::libc::sigemptyset(&mut empty);
        if nix::libc::sigprocmask(nix::libc::SIG_SETMASK, &empty, std::ptr::null_mut()) == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Places an inherited descriptor at a fixed number in a child, clearing
/// the close-on-exec flag there. The source must not be the target, which
/// `high` guarantees for every caller here. Async-signal-safe.
pub(crate) fn place(source: RawFd, target: RawFd) -> std::io::Result<()> {
    // SAFETY: dup2 is async-signal-safe; onto a new number it clears the flag.
    if unsafe { nix::libc::dup2(source, target) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// **Every descriptor above the standard streams but the kept numbers is
/// marked close-on-exec**, per Spec section 10's allowlist, so whatever the
/// caller left open, a root shell's descriptors among them, never crosses the
/// child's exec. Marked rather than closed, so the spawn's own close-on-exec
/// error pipe keeps working until the exec. `keep` is sorted, each at 3 or
/// above. Async-signal-safe, for a pre-exec.
pub(crate) fn seal_except(keep: &[RawFd]) -> std::io::Result<()> {
    let mut low: u32 = 3;
    for &kept in keep {
        let kept = kept as u32;
        if kept > low {
            close_on_exec_range(low, kept - 1)?;
        }
        low = kept + 1;
    }
    close_on_exec_range(low, u32::MAX)
}

fn close_on_exec_range(first: u32, last: u32) -> std::io::Result<()> {
    // SAFETY: close_range with CLOSE_RANGE_CLOEXEC marks a range of this
    // process's descriptors and is async-signal-safe.
    let rc = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_close_range,
            first,
            last,
            nix::libc::CLOSE_RANGE_CLOEXEC,
        )
    };
    if rc == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// A close-on-exec copy at 100 or above, so no source of a placement is a
/// number a child places, a `dup2` onto its own number leaving the flag set.
pub fn high(fd: RawFd) -> std::io::Result<OwnedFd> {
    // SAFETY: F_DUPFD_CLOEXEC on a descriptor the caller owns.
    let raw = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    if raw == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the number was just made and is owned by nothing else.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

/// **The agent's runtime directory, made and repaired**, per section 6:
/// owned by the agent's account and its own group, `0750`, a link at its
/// name refused, and a socket name a dead worker left removed, which is
/// safe only under both locks.
pub fn prepare_runtime_directory(
    coordination_root: &Path,
    agent: &str,
    uid: u32,
    gid: u32,
) -> Result<PathBuf, LifecycleRefusal> {
    let directory = runtime_directory(coordination_root, agent);
    hold_directory(&directory, uid, gid, 0o750)?;
    for name in ["coordination.sock", "gate.sock"] {
        let stale = directory.join(name);
        if std::fs::symlink_metadata(&stale).is_ok() && std::fs::remove_file(&stale).is_err() {
            return Err(refuse_path("cannot be cleared", &stale));
        }
    }
    Ok(directory)
}

/// What the start step hands the worker it forks, per section 6.
pub struct WorkerStart<'a> {
    pub binary: &'a Path,
    pub arguments: Vec<String>,
    pub uid: u32,
    pub gid: u32,
    pub home: &'a Path,
    pub library_path: Option<&'a Path>,
    /// The worker log, `worker.log` in the declaration directory, never the
    /// operations log.
    pub log: &'a std::fs::File,
    pub run_lock: &'a RunLock,
    /// The relay's lifetime pipe's write end, where a file sink stood a relay.
    pub relay_write: Option<RawFd>,
}

/// **Forks the worker**, per section 6: the child inherits the run lock's
/// description, takes a new session, points its standard input at
/// `/dev/null` and its output and error at the worker log, carries the fixed
/// environment and nothing else, resets the ignored signals, sets
/// `PR_SET_NO_NEW_PRIVS`, narrows its groups to the agent's own, then its
/// gid, then its uid, and executes. It carries exactly two descriptors past
/// its standard streams: the run lock at 9 and, where a relay stands, the
/// lifetime pipe's write end at 8.
pub fn spawn_worker(start: WorkerStart<'_>) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    let lock = high(start.run_lock.raw())?;
    let relay = start.relay_write.map(high).transpose()?;
    let lock_raw = lock.as_raw_fd();
    let relay_raw = relay.as_ref().map(|fd| fd.as_raw_fd());
    let (uid, gid) = (start.uid, start.gid);
    let mut command = std::process::Command::new(start.binary);
    command
        .args(&start.arguments)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", start.home)
        .env("LANG", "C.UTF-8")
        .stdin(std::process::Stdio::null())
        .stdout(start.log.try_clone()?)
        .stderr(start.log.try_clone()?);
    if let Some(libraries) = start.library_path {
        command.env("LD_LIBRARY_PATH", libraries);
    }
    // SAFETY: every call below is async-signal-safe, run in the child
    // between fork and exec.
    unsafe {
        command.pre_exec(move || {
            place(lock_raw, RUN_LOCK_FD)?;
            if let Some(raw) = relay_raw {
                place(raw, RELAY_LIFETIME_FD)?;
                seal_except(&[RELAY_LIFETIME_FD, RUN_LOCK_FD])?;
            } else {
                seal_except(&[RUN_LOCK_FD])?;
            }
            detach_and_reset()?;
            // **A fixed working directory and file-creation mask**, per Spec
            // section 6: the caller's directory and umask are not the
            // agent's, so the worker starts at `/` with `027`.
            if nix::libc::chdir(c"/".as_ptr()) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            nix::libc::umask(0o027);
            if nix::libc::prctl(nix::libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            crate::inventory::drop_to(uid, &[gid as nix::libc::gid_t])
        });
    }
    command.spawn()
}

/// The worker's argument vector, per section 6: the coordination socket,
/// the SPU and gate binaries, and the named flags for the headroom, the
/// loop file and the classify binary where each stands. Only the validated
/// name, the root's installed values and the validated declaration reach it.
pub fn worker_arguments(
    coordination_socket: &Path,
    spu: &Path,
    gate: &Path,
    headroom_bytes: Option<&str>,
    loop_file: Option<&Path>,
    classify: Option<&Path>,
) -> Vec<String> {
    let mut args = vec![
        coordination_socket.display().to_string(),
        spu.display().to_string(),
        gate.display().to_string(),
    ];
    if let Some(headroom) = headroom_bytes {
        args.push("--headroom-bytes".to_string());
        args.push(headroom.to_string());
    }
    if let Some(loop_file) = loop_file {
        args.push("--loop-file".to_string());
        args.push(loop_file.display().to_string());
    }
    if let Some(classify) = classify {
        args.push("--classify-binary".to_string());
        args.push(classify.display().to_string());
    }
    args
}

/// Opens a log the start step appends to, `worker.log` in the declaration
/// directory, owned by `owner`, through `log::open_append`.
pub fn open_log(path: &Path, owner: Option<(u32, u32)>) -> std::io::Result<std::fs::File> {
    crate::log::open_append(path, owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> crate::scratch::Scratch {
        let path =
            std::env::temp_dir().join(format!("weaver-admin-start-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        crate::scratch::Scratch(path)
    }

    /// A child holding the run lock's description, as a constituent does:
    /// `sleep` started with the description at the run lock's number.
    fn holding_child(lock: &RunLock) -> std::process::Child {
        use std::os::unix::process::CommandExt;
        // A high copy, so the source is never the target number itself.
        // SAFETY: F_DUPFD_CLOEXEC on a descriptor the lock owns.
        let high = unsafe { nix::libc::fcntl(lock.raw(), nix::libc::F_DUPFD_CLOEXEC, 100) };
        assert!(high >= 100);
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        // SAFETY: dup2 is async-signal-safe and clears the flag at the target.
        unsafe {
            command.pre_exec(move || {
                if nix::libc::dup2(high, RUN_LOCK_FD) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().expect("the stand-in constituent");
        // SAFETY: closing the parent's high copy.
        unsafe { nix::libc::close(high) };
        child
    }

    /// **The run lock is held while any constituent holds the description,
    /// and found by the scan**, per section 3: the invocation takes it,
    /// forks a holder and lets its own copy go, and the lock still reads held,
    /// a second take refuses, the scan names the holder, and the escalation
    /// ends it and answers once the lock is free. Perturbation: take the lock
    /// with `F_SETLK`, a classic lock, and the child never holds it, so the
    /// lock reads free the moment the invocation's copy closes.
    #[test]
    fn the_run_lock_outlives_its_taker_and_the_escalation_ends_its_holder() {
        let dir = scratch("run-lock");
        let lock = take_run_lock(dir.0.as_path())
            .unwrap()
            .expect("a free lock is taken");
        let mut child = holding_child(&lock);
        drop(lock);
        assert!(
            run_lock_held(dir.0.as_path()).unwrap(),
            "the child holds it"
        );
        assert!(
            take_run_lock(dir.0.as_path()).unwrap().is_none(),
            "a second take refuses while a constituent stands"
        );
        // The stand-in is found at the run lock's number; a concurrent test's
        // child between fork and exec may appear for an instant beside it.
        let found = holders(dir.0.as_path());
        assert!(
            found.contains(&Holder {
                pid: child.id() as i32,
                fd: RUN_LOCK_FD
            }),
            "{found:?}"
        );
        escalate_within(
            dir.0.as_path(),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .expect("the escalation ends the holder");
        assert!(!run_lock_held(dir.0.as_path()).unwrap());
        let _ = child.wait();
        assert!(
            take_run_lock(dir.0.as_path()).unwrap().is_some(),
            "and the lock is free"
        );
    }

    /// **The escalation reaches `SIGKILL` for a holder that ignores
    /// `SIGTERM`.** Perturbation: send `SIGTERM` in both rounds and the
    /// holder survives, refusing `WorkerWouldNotExit`.
    #[test]
    fn the_escalation_kills_a_holder_that_ignores_term() {
        use std::os::unix::process::CommandExt;
        let dir = scratch("ignores-term");
        let lock = take_run_lock(dir.0.as_path()).unwrap().unwrap();
        // SAFETY: as in `holding_child`.
        let high = unsafe { nix::libc::fcntl(lock.raw(), nix::libc::F_DUPFD_CLOEXEC, 100) };
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "trap '' TERM; while :; do sleep 1; done"]);
        // SAFETY: dup2 is async-signal-safe.
        unsafe {
            command.pre_exec(move || {
                if nix::libc::dup2(high, RUN_LOCK_FD) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        // SAFETY: closing the parent's copies.
        unsafe { nix::libc::close(high) };
        drop(lock);
        std::thread::sleep(Duration::from_millis(100));
        escalate_within(
            dir.0.as_path(),
            Duration::from_millis(500),
            Duration::from_secs(5),
        )
        .expect("SIGKILL ends it");
        let _ = child.wait();
    }

    /// **The escalation ends every holder**, per Spec section 3: two
    /// constituents holding the description, as a worker and a member do, are
    /// both signalled and the lock reads free. Perturbation: signal only the
    /// first holder the scan finds and the second keeps the lock, refusing
    /// `WorkerWouldNotExit`.
    #[test]
    fn the_escalation_ends_every_holder() {
        let dir = scratch("every-holder");
        let lock = take_run_lock(dir.0.as_path()).unwrap().unwrap();
        let mut first = holding_child(&lock);
        let mut second = holding_child(&lock);
        drop(lock);
        // Both stand-ins are found. A concurrent test's child, between its
        // fork and its exec, can hold this test's close-on-exec copy for an
        // instant, which single-threaded admin never meets, so the scan is
        // asked for both rather than for exactly two.
        let found: Vec<i32> = holders(dir.0.as_path()).iter().map(|h| h.pid).collect();
        assert!(found.contains(&(first.id() as i32)), "{found:?}");
        assert!(found.contains(&(second.id() as i32)), "{found:?}");
        escalate_within(
            dir.0.as_path(),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .expect("both end");
        let _ = first.wait();
        let _ = second.wait();
        assert!(!run_lock_held(dir.0.as_path()).unwrap());
    }

    /// **A pid is pinned only while it still holds the file**, per Spec
    /// section 3: a holder named at a descriptor that no longer refers to the
    /// lock file, a pid reused since the scan in effect, is never pinned and so
    /// never signalled. Perturbation: drop the re-check after `pidfd_open` and
    /// the stale holder is pinned.
    #[test]
    fn a_holder_no_longer_holding_the_file_is_never_pinned() {
        let dir = scratch("pin");
        let lock = take_run_lock(dir.0.as_path()).unwrap().unwrap();
        let mut child = holding_child(&lock);
        drop(lock);
        let meta = std::fs::metadata(dir.0.join("run.lock")).unwrap();
        let target = (meta.dev(), meta.ino());
        let pid = child.id() as i32;
        assert!(
            pin(
                Holder {
                    pid,
                    fd: RUN_LOCK_FD
                },
                target
            )
            .is_some()
        );
        // Its standard input is not the lock file: a stale scan entry.
        assert!(pin(Holder { pid, fd: 0 }, target).is_none());
        let _ = child.kill();
        let _ = child.wait();
    }

    /// **A child takes its own session and comes back to the default
    /// dispositions**, per Spec section 6: a child that inherited ignored
    /// signals, as every child of this invocation does, runs with no signal
    /// ignored and leads its own session. Perturbation: drop the reset from
    /// `detach_and_reset` and `SigIgn` is not zero; drop the `setsid` and the
    /// session is the test's.
    #[test]
    fn a_child_detaches_and_resets_its_signals() {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        // SAFETY: signal, setsid and sigprocmask are async-signal-safe.
        unsafe {
            command.pre_exec(|| {
                for &sig in IGNORED_SIGNALS {
                    nix::libc::signal(sig, nix::libc::SIG_IGN);
                }
                detach_and_reset()
            });
        }
        let mut child = command.spawn().unwrap();
        let pid = child.id();
        std::thread::sleep(Duration::from_millis(100));
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
        let ignored = status
            .lines()
            .find_map(|line| line.strip_prefix("SigIgn:"))
            .map(|mask| u64::from_str_radix(mask.trim(), 16).unwrap())
            .unwrap();
        assert_eq!(ignored, 0, "no signal stays ignored past the exec");
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let session: u32 = stat
            .rsplit(')')
            .next()
            .unwrap()
            .split_whitespace()
            .nth(3)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(session, pid, "the child leads its own session");
        let _ = child.kill();
        let _ = child.wait();
    }

    /// **A held exclusive invocation lock refuses every other take**, per
    /// section 3: from another process, both an exclusive take and a shared
    /// one are refused, the shared one being `show`'s `InTransition`. A child
    /// process asks, since a classic lock does not conflict within one
    /// process. The shared holder's side is the test below. Perturbation:
    /// take the exclusive lock as `F_RDLCK` and the child's shared take
    /// succeeds.
    #[test]
    fn an_exclusive_invocation_lock_refuses_every_other_take() {
        let dir = scratch("invocation");
        let path = dir.0.as_path().join("admin.lock");
        let held = take_invocation_lock(dir.0.as_path()).expect("the first take");
        // Built before the fork, so the child only calls what a fork of a
        // threaded process may.
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // A forked child asks, a second process being what the lock orders.
        // SAFETY: the child only calls async-signal-safe functions.
        match unsafe { nix::unistd::fork() }.unwrap() {
            nix::unistd::ForkResult::Child => {
                // SAFETY: open and fcntl are async-signal-safe.
                let code = unsafe {
                    let fd = nix::libc::open(c_path.as_ptr(), nix::libc::O_RDWR);
                    let mut write = whole_file(nix::libc::F_WRLCK);
                    let mut read = whole_file(nix::libc::F_RDLCK);
                    let write_refused = nix::libc::fcntl(fd, nix::libc::F_SETLK, &mut write) != 0;
                    let read_refused = nix::libc::fcntl(fd, nix::libc::F_SETLK, &mut read) != 0;
                    if write_refused && read_refused { 0 } else { 1 }
                };
                // SAFETY: _exit is async-signal-safe.
                unsafe { nix::libc::_exit(code) };
            }
            nix::unistd::ForkResult::Parent { child } => {
                let status = nix::sys::wait::waitpid(child, None).unwrap();
                assert_eq!(
                    status,
                    nix::sys::wait::WaitStatus::Exited(child, 0),
                    "an exclusive holder refuses both takes from another process"
                );
            }
        }
        drop(held);
    }

    /// A forked child holding the invocation lock shared for `hold`, as a
    /// `show` does for its observation, answering its pid once it holds it.
    fn shared_holder(path: &Path, hold: Duration) -> nix::unistd::Pid {
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let (ready_read, ready_write) = nix::unistd::pipe().unwrap();
        let pause = nix::libc::timespec {
            tv_sec: hold.as_secs() as nix::libc::time_t,
            tv_nsec: hold.subsec_nanos() as nix::libc::c_long,
        };
        // SAFETY: the child calls only async-signal-safe functions.
        match unsafe { nix::unistd::fork() }.unwrap() {
            nix::unistd::ForkResult::Child => {
                // SAFETY: open, fcntl, write, nanosleep and _exit are
                // async-signal-safe.
                unsafe {
                    let fd = nix::libc::open(c_path.as_ptr(), nix::libc::O_RDWR);
                    let read = whole_file(nix::libc::F_RDLCK);
                    nix::libc::fcntl(fd, nix::libc::F_SETLK, &read);
                    nix::libc::write(ready_write.as_raw_fd(), [1u8].as_ptr().cast(), 1);
                    nix::libc::nanosleep(&pause, std::ptr::null_mut());
                    nix::libc::_exit(0);
                }
            }
            nix::unistd::ForkResult::Parent { child } => {
                drop(ready_write);
                let mut byte = [0u8; 1];
                nix::unistd::read(&ready_read, &mut byte).unwrap();
                child
            }
        }
    }

    /// **An exclusive taker behind `show`'s shared hold waits within `show`'s
    /// bound, and is starved past it with readers named**, per section 3: a
    /// shared hold released inside the bound lets the take through on a
    /// retry, and one held past it refuses `InvocationInFlight` at about the
    /// bound. Perturbation: refuse on any holder without reading its kind and
    /// the first case refuses; retry without the bound and the second never
    /// returns.
    #[test]
    fn an_exclusive_take_waits_out_a_brief_reader_and_is_starved_by_a_long_one() {
        let dir = scratch("readers");
        let path = dir.0.join("admin.lock");
        drop(open_lock_file(&path).unwrap());
        let brief = shared_holder(&path, Duration::from_millis(300));
        let taken = take_invocation_lock(dir.0.as_path());
        assert!(taken.is_ok(), "a brief reader is waited out: {taken:?}");
        drop(taken);
        let _ = nix::sys::wait::waitpid(brief, None);
        let long = shared_holder(&path, Duration::from_secs(5));
        let started = Instant::now();
        assert_eq!(
            take_invocation_lock(dir.0.as_path()).err(),
            Some(LifecycleRefusal::InvocationInFlight),
            "a reader held past the bound starves the taker"
        );
        assert!(started.elapsed() < SHARED_HOLD_WAIT + Duration::from_secs(1));
        // SAFETY: kill on the child this test forked.
        unsafe { nix::libc::kill(long.as_raw(), nix::libc::SIGKILL) };
        let _ = nix::sys::wait::waitpid(long, None);
    }

    /// **Only an absent lock file is an absent run**: a run directory that
    /// cannot be looked into refuses rather than reporting the lock free.
    /// Perturbation: read every lookup failure as absent and the unreadable
    /// case answers `false`.
    #[test]
    fn a_lock_that_cannot_be_looked_at_is_never_free() {
        let dir = scratch("unreadable-lock");
        assert_eq!(run_lock_held(dir.0.as_path()), Ok(false), "no file, no run");
        // A path through a regular file cannot be looked into (ENOTDIR).
        let file = dir.0.join("plain");
        std::fs::write(&file, "").unwrap();
        assert_eq!(
            run_lock_held(&file),
            Err(LifecycleRefusal::BoundaryUnverified),
            "a lookup that fails for another reason refuses"
        );
    }

    /// **The run directory is made root's and `0755`, and a link at its name
    /// is refused**, per section 3. Run as the invoking user, so the owner
    /// set is that user's; the mode and the refusal are what is watched.
    /// Perturbation: drop `O_NOFOLLOW` from the open and a link to a
    /// directory is followed and repaired.
    #[test]
    fn the_run_directory_is_held_and_a_link_refused() {
        let dir = scratch("run-dir");
        let uid = nix::unistd::getuid().as_raw();
        let gid = nix::unistd::getgid().as_raw();
        let made = dir.0.as_path().join("made");
        hold_directory(&made, uid, gid, 0o755).expect("made where absent");
        let mode = std::fs::metadata(&made).unwrap().mode() & 0o7777;
        assert_eq!(mode, 0o755);
        std::fs::set_permissions(&made, std::os::unix::fs::PermissionsExt::from_mode(0o777))
            .unwrap();
        hold_directory(&made, uid, gid, 0o755).expect("repaired where it stands");
        assert_eq!(std::fs::metadata(&made).unwrap().mode() & 0o7777, 0o755);
        let link = dir.0.as_path().join("link");
        std::os::unix::fs::symlink(&made, &link).unwrap();
        assert_eq!(
            hold_directory(&link, uid, gid, 0o755),
            Err(LifecycleRefusal::BoundaryUnverified)
        );
    }
}
