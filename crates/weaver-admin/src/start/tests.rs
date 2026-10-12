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

/// **A process with the lock file merely open is no holder**, per Spec
/// section 3: beside a constituent holding the description, a process
/// holding a fresh descriptor of the same file, as a concurrent `show`
/// does while it reads the lock, is neither named by the scan nor pinned.
/// Perturbation: match on the device and inode alone and the opener is
/// named at its standard input.
#[test]
fn a_process_with_the_lock_file_merely_open_is_no_holder() {
    let dir = scratch("merely-open");
    let lock = take_run_lock(dir.0.as_path()).unwrap().unwrap();
    let mut holder = holding_child(&lock);
    drop(lock);
    let mut opener = std::process::Command::new("/bin/sleep")
        .arg("30")
        .stdin(std::fs::File::open(dir.0.join("run.lock")).unwrap())
        .spawn()
        .unwrap();
    let found: Vec<i32> = holders(dir.0.as_path()).iter().map(|h| h.pid).collect();
    assert!(found.contains(&(holder.id() as i32)), "{found:?}");
    assert!(
        !found.contains(&(opener.id() as i32)),
        "the opener is no holder: {found:?}"
    );
    let meta = std::fs::metadata(dir.0.join("run.lock")).unwrap();
    let opened = Holder {
        pid: opener.id() as i32,
        fd: 0,
    };
    assert!(pin(opened, (meta.dev(), meta.ino())).is_none());
    for child in [&mut holder, &mut opener] {
        let _ = child.kill();
        let _ = child.wait();
    }
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
            close_inherited_except(None);
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
            close_inherited_except(Some(ready_write.as_raw_fd()));
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

/// **Only contention is a holder**: a lock take refused for another reason,
/// here a write lock asked on a descriptor open for reading, refuses rather
/// than reading as a run or a transition. Perturbation: read every failed
/// take as contention and the bad take answers `Ok`.
#[test]
fn a_lock_failure_that_is_not_contention_refuses() {
    let dir = scratch("not-contention");
    let path = dir.0.join("run.lock");
    std::fs::write(&path, "").unwrap();
    let read_only = std::fs::File::open(&path).unwrap();
    let request = whole_file(nix::libc::F_WRLCK);
    // SAFETY: fcntl on a descriptor this test owns; EBADF is the point.
    let rc = unsafe { nix::libc::fcntl(read_only.as_raw_fd(), nix::libc::F_OFD_SETLK, &request) };
    assert_eq!(rc, -1);
    assert_eq!(contended(&path), Err(LifecycleRefusal::BoundaryUnverified));
    // Real contention reads as a holder.
    let held = take_run_lock(dir.0.as_path()).unwrap().unwrap();
    assert!(take_run_lock(dir.0.as_path()).unwrap().is_none());
    drop(held);
}

/// **The relay serves the run's own file, read-only**, per section 6: the
/// reopen through `/proc/self/fd` answers a read-only descriptor of the
/// same device and inode, never the path's. Perturbation: reopen by the
/// path after renaming another file over it and the identity differs.
#[test]
fn the_sink_is_reopened_read_only_and_the_same_file() {
    let dir = scratch("reopen");
    let path = dir.0.join("trace.ndjson");
    let write: OwnedFd = std::fs::File::create(&path).unwrap().into();
    // Another file now stands at the declaration's path.
    std::fs::rename(&path, dir.0.join("rotated")).unwrap();
    std::fs::write(&path, "another").unwrap();
    let read = reopen_read_only(&write).unwrap();
    // SAFETY: F_GETFL on the descriptor just reopened.
    let flags = unsafe { nix::libc::fcntl(read.as_raw_fd(), nix::libc::F_GETFL) };
    assert_eq!(
        flags & nix::libc::O_ACCMODE,
        nix::libc::O_RDONLY,
        "read-only"
    );
    let a = nix::sys::stat::fstat(&write).unwrap();
    let b = nix::sys::stat::fstat(&read).unwrap();
    assert_eq!(
        (a.st_dev, a.st_ino),
        (b.st_dev, b.st_ino),
        "the run's own file"
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
    std::fs::set_permissions(&made, std::os::unix::fs::PermissionsExt::from_mode(0o777)).unwrap();
    hold_directory(&made, uid, gid, 0o755).expect("repaired where it stands");
    assert_eq!(std::fs::metadata(&made).unwrap().mode() & 0o7777, 0o755);
    let link = dir.0.as_path().join("link");
    std::os::unix::fs::symlink(&made, &link).unwrap();
    assert_eq!(
        hold_directory(&link, uid, gid, 0o755),
        Err(LifecycleRefusal::BoundaryUnverified)
    );
}
