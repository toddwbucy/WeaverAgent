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
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    // **Judged and laid on the descriptor** (#98; the custody audit's G17):
    // a regular file this crate's uid owns, then `0600` whatever the umask
    // made or another act left, per Spec section 3.
    let stat = nix::sys::stat::fstat(&owned)
        .map_err(|_| refuse_path("cannot be judged as a lock file", path))?;
    if nix::sys::stat::SFlag::from_bits_truncate(stat.st_mode) & nix::sys::stat::SFlag::S_IFMT
        != nix::sys::stat::SFlag::S_IFREG
    {
        return Err(refuse_path("is not a regular file", path));
    }
    if stat.st_uid != nix::unistd::geteuid().as_raw() {
        return Err(refuse_path("is not this crate's own", path));
    }
    nix::sys::stat::fchmod(&owned, nix::sys::stat::Mode::from_bits_truncate(0o600))
        .map_err(|_| refuse_path("does not take mode 0600", path))?;
    Ok(owned)
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
///
/// **Taken, it records its verb** (section 3, the lock's verb, on the
/// operator's ruling of 2026-10-09 on #1): `<verb> <pid>`, the file's whole
/// content, written under the lock after truncating whatever the last holder
/// left, so a force meeting the lock reads which verb holds it
/// ([`read_holder`]). A record that cannot be written refuses, since a force
/// would otherwise wait on it forever.
pub fn take_invocation_lock(
    run_directory: &Path,
    verb: crate::verb_table::HolderVerb,
) -> Result<InvocationLock, LifecycleRefusal> {
    take_invocation_lock_judged(run_directory, verb).map_err(|refused| match refused {
        TakeRefused::Starved => LifecycleRefusal::InvocationInFlight,
        TakeRefused::Other(refusal) => refusal,
    })
}

/// Take under an invocation's existing deadline. The courtesy wait behind
/// `show` can consume only what remains, and expiry permits no holder record.
/// The caller distinguishes its deadline from contention after this returns.
pub(crate) fn take_invocation_lock_until(
    run_directory: &Path,
    verb: crate::verb_table::HolderVerb,
    deadline: Instant,
) -> Result<InvocationLock, LifecycleRefusal> {
    take_invocation_lock_with_deadline(run_directory, verb, Some(deadline)).map_err(|refused| {
        match refused {
            TakeRefused::Starved => LifecycleRefusal::InvocationInFlight,
            TakeRefused::Other(refusal) => refusal,
        }
    })
}

/// Why the exclusive take refused, for the one caller that acts on the
/// difference, the force (#125).
#[derive(Debug, PartialEq)]
pub enum TakeRefused {
    /// Readers held the lock past `show`'s bound, the take having waited it
    /// out: the starved case, `InvocationInFlight` with readers named.
    Starved,
    /// Any other refusal: an exclusive holder, an expired invocation deadline,
    /// a probe that did not read, or a lock file that refused.
    Other(LifecycleRefusal),
}

/// [`take_invocation_lock`], saying which refusal it met.
pub fn take_invocation_lock_judged(
    run_directory: &Path,
    verb: crate::verb_table::HolderVerb,
) -> Result<InvocationLock, TakeRefused> {
    take_invocation_lock_with_deadline(run_directory, verb, None)
}

fn take_invocation_lock_with_deadline(
    run_directory: &Path,
    verb: crate::verb_table::HolderVerb,
    invocation_deadline: Option<Instant>,
) -> Result<InvocationLock, TakeRefused> {
    take_invocation_lock_with_clock(run_directory, verb, invocation_deadline, Instant::now)
}

pub(crate) fn take_invocation_lock_for_enter(
    run_directory: &Path,
    window: &crate::serving_load::window::LoadWindow,
) -> Result<InvocationLock, LifecycleRefusal> {
    take_invocation_lock_with_limit(
        run_directory,
        crate::verb_table::HolderVerb::Load,
        None,
        Instant::now,
        Some(window),
    )
    .map_err(|refused| match refused {
        TakeRefused::Starved => LifecycleRefusal::InvocationInFlight,
        TakeRefused::Other(refusal) => refusal,
    })
}

fn take_invocation_lock_with_clock(
    run_directory: &Path,
    verb: crate::verb_table::HolderVerb,
    invocation_deadline: Option<Instant>,
    now: impl Fn() -> Instant,
) -> Result<InvocationLock, TakeRefused> {
    take_invocation_lock_with_limit(run_directory, verb, invocation_deadline, now, None)
}

fn take_invocation_lock_with_limit(
    run_directory: &Path,
    verb: crate::verb_table::HolderVerb,
    invocation_deadline: Option<Instant>,
    now: impl Fn() -> Instant,
    enter: Option<&crate::serving_load::window::LoadWindow>,
) -> Result<InvocationLock, TakeRefused> {
    let check_invocation = || {
        if let Some(window) = enter {
            window.check().map_err(TakeRefused::Other)?;
        }
        if invocation_deadline.is_some_and(|deadline| now() >= deadline) {
            Err(TakeRefused::Other(LifecycleRefusal::InvocationInFlight))
        } else {
            Ok(())
        }
    };
    check_invocation()?;
    let path = run_directory.join("admin.lock");
    let file = open_lock_file(&path).map_err(TakeRefused::Other)?;
    let courtesy_deadline = now() + SHARED_HOLD_WAIT;
    let deadline = invocation_deadline.map_or(courtesy_deadline, |invocation| {
        invocation.min(courtesy_deadline)
    });
    loop {
        check_invocation()?;
        let request = whole_file(nix::libc::F_WRLCK);
        // SAFETY: fcntl on a descriptor this frame owns, with a lock it built.
        if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_SETLK, &request) } == 0 {
            check_invocation()?;
            return recorded(file, verb, &path).map_err(TakeRefused::Other);
        }
        contended(&run_directory.join("admin.lock")).map_err(TakeRefused::Other)?;
        let mut probe = whole_file(nix::libc::F_WRLCK);
        // SAFETY: as above.
        if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_GETLK, &mut probe) } != 0 {
            return Err(TakeRefused::Other(LifecycleRefusal::InvocationInFlight));
        }
        match probe.l_type as i32 {
            nix::libc::F_WRLCK => {
                return Err(TakeRefused::Other(LifecycleRefusal::InvocationInFlight));
            }
            nix::libc::F_RDLCK if now() < deadline => {
                let pause = if invocation_deadline.is_some() {
                    POLL.min(deadline.saturating_duration_since(now()))
                } else {
                    POLL
                };
                let pause = if let Some(window) = enter {
                    pause.min(window.remaining().map_err(TakeRefused::Other)?)
                } else {
                    pause
                };
                std::thread::sleep(pause);
            }
            nix::libc::F_RDLCK => {
                // The invocation may have expired during the lock attempt or
                // holder probe, after the check at the top of this iteration.
                check_invocation()?;
                diag!(
                    "weaver-admin: readers held the invocation lock past show's bound; \
                     retry once the poller pauses"
                );
                return Err(TakeRefused::Starved);
            }
            // Released between the take and the read: try again at once.
            _ => {}
        }
    }
}

/// The lock just taken, its verb recorded as `<verb> <pid>`.
fn recorded(
    file: OwnedFd,
    verb: crate::verb_table::HolderVerb,
    path: &Path,
) -> Result<InvocationLock, LifecycleRefusal> {
    recorded_with(file, verb, path, |file, record| {
        nix::unistd::ftruncate(file, 0)
            .and_then(|()| nix::sys::uio::pwrite(file, record, 0))
            .map(|_| ())
    })
}

/// [`recorded`] over a write, apart for its tests. **A record that does not
/// write never refuses the take** (section 3, the lock's verb; I3, on the
/// Planner's ruling of 2026-10-12 on #126): a full or failing run directory
/// must not stop a force ending the run, so the taker holds the lock and goes
/// on, naming the failure on standard error, and a reader that never sees
/// the holder named refuses `InvocationInFlight` past its window
/// ([`read_holder`]).
pub(crate) fn recorded_with(
    file: OwnedFd,
    verb: crate::verb_table::HolderVerb,
    path: &Path,
    write: impl FnOnce(&OwnedFd, &[u8]) -> nix::Result<()>,
) -> Result<InvocationLock, LifecycleRefusal> {
    let record = format!("{} {}", verb.spelled(), std::process::id());
    if let Err(e) = write(&file, record.as_bytes()) {
        diag!(
            "weaver-admin: {} cannot record its holder's verb ({e}); the {} goes on, and a force meeting it refuses",
            path.display(),
            verb.spelled()
        );
    }
    Ok(InvocationLock { _file: file })
}

/// **Takes the invocation lock exclusively, waiting with no deadline**
/// (section 3, the forced unload, joining): the force that joined an
/// `unload` takes the lock behind the concluding holder, after the run lock
/// has freed or after its escalation's phase (a), a publication being
/// bounded by its size and not by time. It records its verb as the
/// non-waiting take does.
pub fn take_invocation_lock_waiting(
    run_directory: &Path,
    verb: crate::verb_table::HolderVerb,
) -> Result<InvocationLock, LifecycleRefusal> {
    let path = run_directory.join("admin.lock");
    let file = open_lock_file(&path)?;
    let request = whole_file(nix::libc::F_WRLCK);
    loop {
        // SAFETY: fcntl on a descriptor this frame owns, with a lock it built.
        if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_SETLKW, &request) } == 0 {
            return recorded(file, verb, &path);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(nix::libc::EINTR) {
            return Err(refuse_path("cannot be locked", &path));
        }
    }
}

/// Who holds the invocation lock, as a reader holding nothing sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockHolder {
    Free,
    /// A `show`, which takes the lock shared and records nothing.
    Shared,
    /// An exclusive holder, its record read once it names the holder.
    Exclusive(crate::verb_table::HolderVerb),
}

/// What one `F_GETLK` probe found: the classic record lock names its
/// exclusive holder's pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldBy {
    Free,
    Shared,
    Exclusive(i32),
}

/// How often a reader reads a record that does not yet name its holder.
const HOLDER_REREAD: std::time::Duration = std::time::Duration::from_millis(100);

/// How long a reader waits for one holder to name itself: a holder's write
/// follows its lock by microseconds, so five seconds is a holder whose record
/// did not write, never one still writing.
const HOLDER_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);

/// **Reads the invocation lock's holder, holding nothing** (section 3, the
/// lock's verb): free, `show`'s shared hold, or an exclusive holder's verb,
/// taken only where the record's pid is the holder `F_GETLK` reports.
/// **A record that does not name the holder is read again** every 100 ms
/// (Codex on #109, rounds 45 and 46): empty, partial, or the last holder's,
/// it is the window between the holder's lock and its write; a changed
/// holder starts the read over. **One holder unnamed past five seconds is a
/// holder whose record did not write** (the Planner's ruling of 2026-10-12 on
/// #126), read as a verb that is no `unload`: the reader refuses
/// `InvocationInFlight`, the conservative outcome. A record that cannot be
/// read refuses `InvocationInFlight`.
pub fn read_holder(run_directory: &Path) -> Result<LockHolder, LifecycleRefusal> {
    let file = open_lock_file(&run_directory.join("admin.lock"))?;
    read_holder_using(
        || {
            let mut probe = whole_file(nix::libc::F_WRLCK);
            // SAFETY: fcntl on a descriptor this frame owns.
            if unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_GETLK, &mut probe) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(match probe.l_type as i32 {
                nix::libc::F_WRLCK => HeldBy::Exclusive(probe.l_pid),
                nix::libc::F_RDLCK => HeldBy::Shared,
                _ => HeldBy::Free,
            })
        },
        || {
            let mut record = vec![0u8; 64];
            let read = nix::sys::uio::pread(&file, &mut record, 0).map_err(std::io::Error::from)?;
            record.truncate(read);
            Ok(record)
        },
        HOLDER_WINDOW,
    )
}

/// [`read_holder`]'s loop over a probe and a read, apart for its tests.
pub(crate) fn read_holder_using(
    mut probe: impl FnMut() -> std::io::Result<HeldBy>,
    mut read: impl FnMut() -> std::io::Result<Vec<u8>>,
    window: std::time::Duration,
) -> Result<LockHolder, LifecycleRefusal> {
    let mut unnamed: Option<(i32, std::time::Instant)> = None;
    loop {
        let pid = match probe().map_err(|_| LifecycleRefusal::InvocationInFlight)? {
            HeldBy::Free => return Ok(LockHolder::Free),
            HeldBy::Shared => return Ok(LockHolder::Shared),
            HeldBy::Exclusive(pid) => pid,
        };
        let since = match unnamed {
            Some((held, since)) if held == pid => since,
            _ => {
                let now = std::time::Instant::now();
                unnamed = Some((pid, now));
                now
            }
        };
        let record = read().map_err(|_| LifecycleRefusal::InvocationInFlight)?;
        // The record names this holder where its pid is the one `F_GETLK`
        // reports; its verb is then read, and **a verb this admin does not
        // know refuses** (an admin of another version holding the lock, on
        // the Planner's ruling of 2026-10-12 on #126), the conservative
        // outcome, rather than being read again for ever.
        let named = std::str::from_utf8(&record).ok().and_then(|text| {
            let (verb, recorded) = text.split_once(' ')?;
            (recorded.parse::<i32>().ok()? == pid).then_some(verb)
        });
        if let Some(verb) = named {
            return crate::verb_table::HolderVerb::from_spelled(verb)
                .map(LockHolder::Exclusive)
                .ok_or(LifecycleRefusal::InvocationInFlight);
        }
        if since.elapsed() >= window {
            return Err(LifecycleRefusal::InvocationInFlight);
        }
        std::thread::sleep(HOLDER_REREAD);
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
        return Ok(Shared::Held(InvocationLock { _file: file }));
    }
    contended(&run_directory.join("admin.lock")).map(|()| Shared::InTransition)
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
        return Ok(Some(RunLock { file }));
    }
    contended(&run_directory.join("run.lock")).map(|()| None)
}

/// **Only contention is a holder**: a lock take that failed with `EACCES` or
/// `EAGAIN` met another holder, and any other failure, a filesystem that
/// refuses the lock among them, is a refusal and never evidence of a run or a
/// transition. Read from `errno` right after the failed take.
fn contended(path: &Path) -> Result<(), LifecycleRefusal> {
    match std::io::Error::last_os_error().raw_os_error() {
        Some(nix::libc::EACCES | nix::libc::EAGAIN) => Ok(()),
        _ => Err(refuse_path("cannot be locked", path)),
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

/// One process holding the run lock's description: its pid and the number
/// the descriptor stands at in its table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holder {
    pub pid: i32,
    pub fd: i32,
}

/// **The run lock's holders, from the kernel's descriptor tables**, per
/// section 3: a description lock names no pid, so every process's
/// `/proc/<pid>/fd` is scanned for a descriptor referring to the lock file's
/// device and inode whose description holds the lock, this process
/// excluded. Root reads every table.
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
        if let Ok(held) = holding_descriptors(pid, target) {
            found.extend(held.map(|fd| Holder { pid, fd }));
        }
    }
    found
}

/// **Whether one process holds the run lock's description**, per section 7,
/// the dial's peer: the holder scan above, for one pid. **A lock file that
/// does not stat, or a descriptor table that does not read, is no answer**
/// (`Err`), told apart from a table read that holds no lock, so the dial
/// names a peer it could not judge as such and never as an impostor.
pub fn holds_run_lock(run_directory: &Path, pid: i32) -> Result<bool, ()> {
    let lock = std::fs::metadata(run_directory.join("run.lock")).map_err(|_| ())?;
    Ok(holding_descriptors(pid, (lock.dev(), lock.ino()))
        .map_err(|_| ())?
        .next()
        .is_some())
}

/// The numbers in one process's table whose description holds the lock on
/// `target`, or the error reading the table.
fn holding_descriptors(pid: i32, target: (u64, u64)) -> std::io::Result<impl Iterator<Item = i32>> {
    Ok(std::fs::read_dir(format!("/proc/{pid}/fd"))?
        .flatten()
        .filter_map(|descriptor| {
            descriptor
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<i32>().ok())
        })
        .filter(move |&fd| holds(pid, fd, target)))
}

/// **Whether a descriptor holds the run lock**: it refers to the lock file,
/// and its description carries the lock. A descriptor merely open on the same
/// file, as `show`'s own read of the lock is while a concurrent `show` scans,
/// is a fresh description the kernel lists no lock on, so it is no holder.
/// `/proc/<pid>/fdinfo/<n>` lists a description lock only on the descriptors
/// sharing the description that took it.
fn holds(pid: i32, fd: i32, target: (u64, u64)) -> bool {
    std::fs::metadata(format!("/proc/{pid}/fd/{fd}")).is_ok_and(|m| (m.dev(), m.ino()) == target)
        && std::fs::read_to_string(format!("/proc/{pid}/fdinfo/{fd}")).is_ok_and(|info| {
            info.lines()
                .any(|line| line.starts_with("lock:") && line.contains("OFDLCK"))
        })
}

/// A pidfd on one holder, confirmed after opening to hold the lock still, so
/// a pid that exited, or was reused, since the scan is never signalled.
struct Pinned {
    pidfd: OwnedFd,
}

fn pin(holder: Holder, target: (u64, u64)) -> Option<Pinned> {
    let pidfd = pidfd_open(holder.pid)?;
    holds(holder.pid, holder.fd, target).then_some(Pinned { pidfd })
}

fn signal(pinned: &Pinned, signal: i32) {
    pidfd_signal(&pinned.pidfd, signal);
}

/// **A pidfd on a pid**, the pinning the escalation and the run end's
/// fallback share. `pidfd_open` is one of the stated exceptions to the
/// election (`weaver-admin-Spec` section 1), by number through `nix`'s
/// re-exported `libc`, and the descriptor is owned at once.
pub(crate) fn pidfd_open(pid: i32) -> Option<OwnedFd> {
    // SAFETY: pidfd_open takes a pid and flags and answers a descriptor.
    let raw = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_open, pid, 0) };
    if raw < 0 {
        return None;
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    Some(unsafe { OwnedFd::from_raw_fd(raw as RawFd) })
}

/// **A signal through a pidfd**, the other stated exception. Answers whether
/// it was sent.
pub(crate) fn pidfd_signal(pidfd: &OwnedFd, signal: i32) -> bool {
    // SAFETY: pidfd_send_signal on a pidfd the caller owns, with no siginfo.
    unsafe {
        nix::libc::syscall(
            nix::libc::SYS_pidfd_send_signal,
            pidfd.as_raw_fd(),
            signal,
            std::ptr::null::<nix::libc::siginfo_t>(),
            0,
        ) == 0
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
    escalate_within_checked(run_directory, term_grace, kill_grace, || Ok(()))
}

/// Revalidate a serving caller's pinned containment before each signal. The
/// target remains a pidfd-pinned run-lock holder, never the socket listener.
pub(crate) fn escalate_within_checked(
    run_directory: &Path,
    term_grace: Duration,
    kill_grace: Duration,
    check: impl Fn() -> Result<(), LifecycleRefusal>,
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
                check()?;
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

/// **A test's forked child closes what it inherited**, every descriptor
/// above standard error but `keep`, as its first act: a fork without an exec
/// carries every descriptor the test process held at that instant, another
/// test's run-lock description among them, so the child would hold that
/// run's lock, and the escalation of that test, finding it among the
/// holders, would end it. Async-signal-safe: raw `close_range` calls only.
// #118: a test-only hook, kept beside the production code it observes.
#[cfg(test)]
pub(crate) fn close_inherited_except(keep: Option<RawFd>) {
    let close = |first: u32, last: u32| {
        if first <= last {
            // SAFETY: close_range closes a range of this process's
            // descriptors and is async-signal-safe.
            unsafe { nix::libc::syscall(nix::libc::SYS_close_range, first, last, 0u32) };
        }
    };
    match keep {
        Some(kept) if kept >= 3 => {
            close(3, kept as u32 - 1);
            close(kept as u32 + 1, u32::MAX);
        }
        _ => close(3, u32::MAX),
    }
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
    /// The worker log, `worker.log` in the territory, never the
    /// operations log.
    pub log: &'a std::fs::File,
    pub run_lock: &'a RunLock,
    /// The relay's lifetime pipe's write end, where a file sink stood a relay.
    pub relay_write: Option<RawFd>,
    /// `worker/run/cgroup.procs` of the agent's cgroup, opened before the
    /// fork: the child moves itself in first (`weaver-admin-Spec` 6.1).
    pub placement: RawFd,
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
    let placement = start.placement;
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
            // **The move into the agent's cgroup comes first** (6.1, C4),
            // before any descriptor is placed over its number and before the
            // privilege drop: a failure ends the child before its exec.
            crate::containment::place_self(placement)?;
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

/// The relay's descriptors at its fixed numbers, per section 6: the bound
/// listener, a read-only descriptor of the loaded run's sink, the operations
/// log, the lifetime pipe's read end, and the run lock's description.
pub const RELAY_LISTENER_FD: RawFd = 3;
pub const RELAY_SINK_FD: RawFd = 4;
pub const RELAY_LOG_FD: RawFd = 5;
pub const RELAY_LIFETIME_READ_FD: RawFd = 6;

/// **Binds the trace door**, per section 6: as root, in the agent's run
/// directory, a stale `trace.sock` a previous run's relay left removed first,
/// a stream socket bound, owned by root and the agent's access group, mode
/// `0660`, and listening. Called holding both locks.
pub fn bind_trace_door(run_directory: &Path, access_gid: u32) -> Result<OwnedFd, LifecycleRefusal> {
    let path = run_directory.join("trace.sock");
    clear_trace_door(run_directory)?;
    // **Bound at a mode no wider than root's own**: the socket file takes its
    // mode from the umask at the bind, so the umask is tightened around it and
    // restored, the invocation being single-threaded, and the file never
    // stands open to another principal before its owner and mode are set.
    // SAFETY: umask only changes this process's file-creation mask.
    let previous = unsafe { nix::libc::umask(0o177) };
    let bound = std::os::unix::net::UnixListener::bind(&path);
    // SAFETY: as above, restoring the mask.
    unsafe { nix::libc::umask(previous) };
    let listener = bound.map_err(|_| refuse_path("cannot be bound", &path))?;
    std::os::unix::fs::chown(&path, Some(0), Some(access_gid))
        .map_err(|_| refuse_path("cannot be given to the access group", &path))?;
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o660))
        .map_err(|_| refuse_path("cannot be set to 0660", &path))?;
    Ok(OwnedFd::from(listener))
}

/// Removes a `trace.sock` a previous run's relay left, which a relay cannot
/// unlink from root's directory and whose name outlives its listener. Every
/// load clears it, so a pipe or socket sink leaves no stale door standing.
pub fn clear_trace_door(run_directory: &Path) -> Result<(), LifecycleRefusal> {
    let path = run_directory.join("trace.sock");
    if std::fs::symlink_metadata(&path).is_ok() && std::fs::remove_file(&path).is_err() {
        return Err(refuse_path("cannot be cleared", &path));
    }
    Ok(())
}

/// **A read-only descriptor of the same open file the worker writes**, per
/// section 6: reopened through `/proc/self/fd/N` of the write descriptor,
/// never by the declaration's path, and confirmed by device and inode before
/// it is handed on, so the relay serves the loaded run's own file.
pub fn reopen_read_only(write: &OwnedFd) -> Result<OwnedFd, LifecycleRefusal> {
    let path = format!("/proc/self/fd/{}", write.as_raw_fd());
    let c_path = std::ffi::CString::new(path).map_err(|_| LifecycleRefusal::DescriptorsUnusable)?;
    // SAFETY: open with a NUL-terminated path and no mode.
    let fd =
        unsafe { nix::libc::open(c_path.as_ptr(), nix::libc::O_RDONLY | nix::libc::O_CLOEXEC) };
    if fd == -1 {
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    let read = unsafe { OwnedFd::from_raw_fd(fd) };
    let same = |a: &OwnedFd| -> Option<(u64, u64)> {
        let stat = nix::sys::stat::fstat(a).ok()?;
        Some((stat.st_dev, stat.st_ino))
    };
    match (same(write), same(&read)) {
        (Some(w), Some(r)) if w == r => Ok(read),
        _ => Err(LifecycleRefusal::DescriptorsUnusable),
    }
}

/// What the start step hands the relay it forks, per section 6.
pub struct RelayStart<'a> {
    pub binary: &'a Path,
    pub reader_uid: u32,
    pub agent: &'a str,
    pub boundary_digest: &'a str,
    pub uid: u32,
    pub gid: u32,
    pub listener: &'a OwnedFd,
    pub sink: &'a OwnedFd,
    pub log: &'a std::fs::File,
    pub lifetime_read: &'a OwnedFd,
    pub run_lock: &'a RunLock,
    /// `constituents/cgroup.procs` of the agent's cgroup, opened before the
    /// fork: the child moves itself in first (`weaver-admin-Spec` 6.1).
    pub placement: RawFd,
}

/// **Forks the trace relay**, per section 6: under the relay account and its
/// one group, the trace group, never root, the agent or the member; a new
/// session and the signals reset; `/` and umask `027`; no new privileges;
/// exactly its handed descriptors at their fixed numbers and its standard
/// streams at `/dev/null`, every other descriptor sealed.
pub fn spawn_relay(start: RelayStart<'_>) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    let listener = high(start.listener.as_raw_fd())?;
    let sink = high(start.sink.as_raw_fd())?;
    let log = high(start.log.as_raw_fd())?;
    let lifetime = high(start.lifetime_read.as_raw_fd())?;
    let lock = high(start.run_lock.raw())?;
    let gifts = [
        (listener.as_raw_fd(), RELAY_LISTENER_FD),
        (sink.as_raw_fd(), RELAY_SINK_FD),
        (log.as_raw_fd(), RELAY_LOG_FD),
        (lifetime.as_raw_fd(), RELAY_LIFETIME_READ_FD),
        (lock.as_raw_fd(), RUN_LOCK_FD),
    ];
    let (uid, gid) = (start.uid, start.gid);
    let placement = start.placement;
    let mut command = std::process::Command::new(start.binary);
    command
        .args([
            start.reader_uid.to_string(),
            start.agent.to_string(),
            start.boundary_digest.to_string(),
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: every call below is async-signal-safe, run in the child
    // between fork and exec.
    unsafe {
        command.pre_exec(move || {
            // The move first, as the worker's (6.1, C4).
            crate::containment::place_self(placement)?;
            for (source, target) in gifts {
                place(source, target)?;
            }
            seal_except(&[
                RELAY_LISTENER_FD,
                RELAY_SINK_FD,
                RELAY_LOG_FD,
                RELAY_LIFETIME_READ_FD,
                RUN_LOCK_FD,
            ])?;
            detach_and_reset()?;
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
/// the SPU and gate binaries, and the named flags for the headroom and
/// the classify binary where each stands. Only the validated
/// name, the root's installed values and the validated declaration reach it.
pub fn worker_arguments(
    coordination_socket: &Path,
    spu: &Path,
    gate: &Path,
    headroom_bytes: Option<&str>,
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
pub(crate) mod tests;
