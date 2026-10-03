//! What the worker does with the descriptors admin's start step hands across
//! its exec, per `weaver-harness-Spec` section 2.2 and `weaver-admin-Spec`
//! sections 3 and 6: the run lock's open file description and the relay's
//! lifetime pipe, each marked close-on-exec as the worker's first act and
//! kept for its life. The worker binary itself is started, the way the start
//! step starts it, and read from outside.
#![cfg(target_os = "linux")]

use std::io::Read;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;

use weaver_harness::{RELAY_LIFETIME_DESCRIPTOR, RUN_LOCK_DESCRIPTOR};

/// A scratch directory removed when the test ends, pass or fail.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

/// The child killed when the test ends, pass or fail.
struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        drop(self.0.kill());
        drop(self.0.wait());
    }
}

/// An exclusive open file description lock on the whole file, without
/// waiting, as the start step takes it.
fn ofd_lock(fd: RawFd) {
    let lock = nix::libc::flock {
        l_type: nix::libc::F_WRLCK as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    // SAFETY: fcntl on a descriptor this test owns, with a lock it built.
    let rc = unsafe { nix::libc::fcntl(fd, nix::libc::F_OFD_SETLK, &lock) };
    assert_eq!(rc, 0, "the lock is taken");
}

/// Whether some open file description holds a write lock on the file, read with `F_OFD_GETLK` through a fresh description.
fn held_elsewhere(path: &std::path::Path) -> bool {
    let probe = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("a fresh description");
    let mut lock = nix::libc::flock {
        l_type: nix::libc::F_WRLCK as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    // SAFETY: fcntl on a descriptor this test owns.
    let rc = unsafe { nix::libc::fcntl(probe.as_raw_fd(), nix::libc::F_OFD_GETLK, &mut lock) };
    assert_eq!(rc, 0, "the query runs");
    lock.l_type != nix::libc::F_UNLCK as i16
}

/// A close-on-exec copy of `fd` at 100 or above, owned by the caller. The
/// source is left to its own owner.
fn high_copy(fd: RawFd) -> std::os::fd::OwnedFd {
    // SAFETY: F_DUPFD_CLOEXEC on a descriptor this test owns answers a fresh
    // number this test then owns.
    let raw = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(raw >= 100, "a high copy");
    // SAFETY: the number was just made and is owned by nothing else.
    unsafe { std::os::fd::FromRawFd::from_raw_fd(raw) }
}

/// The descriptor's open flags as the kernel reports them for another
/// process, from `/proc/<pid>/fdinfo/<fd>`.
fn fdinfo_flags(pid: u32, fd: RawFd) -> u32 {
    let info = std::fs::read_to_string(format!("/proc/{pid}/fdinfo/{fd}")).expect("fdinfo");
    let line = info
        .lines()
        .find(|line| line.starts_with("flags:"))
        .expect("a flags line");
    u32::from_str_radix(line.trim_start_matches("flags:").trim(), 8).expect("octal flags")
}

/// **The worker holds the run lock's description and the relay's write end,
/// close-on-exec, for its life**, per `weaver-harness-Spec` section 2.2.
///
/// The test takes an open file description lock as the start step does,
/// hands it to the worker at the run lock's number beside a pipe's write end
/// at the relay's, and closes its own copies, so the worker is the only
/// holder of either. While the worker serves, its descriptor table shows the
/// lock file and both numbers carry the close-on-exec flag, a fresh
/// description's query finds the lock held, and the pipe's read end sees no
/// end-of-file. Killed, the worker releases both: the lock reads free and the
/// read end reads end-of-file.
///
/// The table and the flags are read only under root, the worker having
/// cleared its dumpable flag; the flag itself is pinned in-process by the
/// channel's own test of `keep_close_on_exec`.
///
/// Perturbation: have `keep_start_step_descriptors` close the run lock's
/// number and the lock reads free while the worker serves.
#[test]
fn the_worker_keeps_the_run_lock_and_the_relay_write_end() {
    keeps_both(env!("CARGO_BIN_EXE_worker"), "worker");
}

/// The same first act in the Python-loop worker, built only with its feature.
/// Perturbation: drop `keep_start_step_descriptors` from the pyworker's main
/// and the flags assertion fails.
#[cfg(feature = "pyworker")]
#[test]
fn the_pyworker_keeps_the_run_lock_and_the_relay_write_end() {
    keeps_both(env!("CARGO_BIN_EXE_pyworker"), "pyworker");
}

fn keeps_both(binary: &str, tag: &str) {
    let dir = std::env::temp_dir().join(format!("weaver-start-step-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scratch = Scratch(dir.clone());
    let lock_path = scratch.0.join("run.lock");
    let lock_file = std::fs::File::create(&lock_path).unwrap();
    ofd_lock(lock_file.as_raw_fd());
    let (read_end, write_end) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).unwrap();

    let socket = scratch.0.join("coordination.sock");
    // **Moved above every number the child places**, so neither source can
    // be 8 or 9 itself: a `dup2` onto its own number is a no-op that leaves
    // the close-on-exec flag standing, and a concurrent test's descriptors
    // make the low numbers likely.
    // The low originals close at the end of each block, the copies sharing
    // their open file descriptions.
    let lock_file = {
        let low = lock_file;
        high_copy(low.as_raw_fd())
    };
    let write_end = {
        let low = write_end;
        high_copy(low.as_raw_fd())
    };
    let lock_raw = lock_file.as_raw_fd();
    let write_raw = write_end.as_raw_fd();
    // Built before the fork, so the child only writes it.
    let uid_map = format!("0 {} 1", nix::unistd::getuid().as_raw());
    let mut command = std::process::Command::new(binary);
    command
        .arg(&socket)
        .arg("/bin/false")
        .arg("/bin/false")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: dup2 is async-signal-safe, and dup2 onto a new number clears the
    // close-on-exec flag there, so each arrives as the start step hands it.
    unsafe {
        command.pre_exec(move || {
            if nix::libc::dup2(lock_raw, RUN_LOCK_DESCRIPTOR) == -1
                || nix::libc::dup2(write_raw, RELAY_LIFETIME_DESCRIPTOR) == -1
            {
                return Err(std::io::Error::last_os_error());
            }
            // **A user namespace this test owns**, its root mapped to the
            // test's own uid, so a non-dumpable worker's descriptor table is
            // the test's to read, without root. The worker's credentials are
            // unchanged, so it binds in the scratch directory as before.
            if nix::libc::unshare(nix::libc::CLONE_NEWUSER) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            let map = nix::libc::open(c"/proc/self/uid_map".as_ptr(), nix::libc::O_WRONLY);
            if map == -1
                || nix::libc::write(map, uid_map.as_ptr().cast(), uid_map.len())
                    != uid_map.len() as isize
            {
                return Err(std::io::Error::last_os_error());
            }
            nix::libc::close(map);
            Ok(())
        });
    }
    let worker = Child(command.spawn().expect("the worker starts"));
    let pid = worker.0.id();
    // The worker is now the only holder of the description and the write end.
    drop(lock_file);
    drop(write_end);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !socket.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the worker never bound"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // The worker clears its dumpable flag, and the test owns the worker's
    // user namespace, so it reads the table and the flags as any owner may.
    let held = std::fs::metadata(format!("/proc/{pid}/fd/{RUN_LOCK_DESCRIPTOR}"))
        .expect("the run lock is in the worker's table");
    let file = std::fs::metadata(&lock_path).unwrap();
    assert_eq!((held.dev(), held.ino()), (file.dev(), file.ino()));
    let cloexec = nix::libc::O_CLOEXEC as u32;
    assert_ne!(
        fdinfo_flags(pid, RUN_LOCK_DESCRIPTOR) & cloexec,
        0,
        "the run lock is close-on-exec, the worker's first act"
    );
    assert_ne!(
        fdinfo_flags(pid, RELAY_LIFETIME_DESCRIPTOR) & cloexec,
        0,
        "and so is the relay's write end"
    );
    assert!(held_elsewhere(&lock_path), "the worker holds the lock");
    nix::fcntl::fcntl(
        &read_end,
        nix::fcntl::FcntlArg::F_SETFL(nix::fcntl::OFlag::O_NONBLOCK),
    )
    .unwrap();
    let mut reader = std::fs::File::from(read_end);
    let mut byte = [0u8; 1];
    assert!(
        matches!(reader.read(&mut byte), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "the relay's read end sees no end-of-file while the worker lives"
    );

    drop(worker);
    assert!(
        !held_elsewhere(&lock_path),
        "the lock is free once the worker is gone"
    );
    assert_eq!(
        reader.read(&mut byte).unwrap(),
        0,
        "and the read end reads end-of-file"
    );
}
