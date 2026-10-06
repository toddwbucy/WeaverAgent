//! The member's side of the run lock, read off the binary itself, per
//! `weaver-state-Spec` sections 2 and 5: the descriptor admin's start step
//! hands across the exec at the run lock's number is marked close-on-exec as
//! the member's first act and kept while it serves.
#![cfg(target_os = "linux")]

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;

/// The run lock's number, the one admin's start step uses for every
/// constituent, per `weaver-admin-Spec` section 3.
const RUN_LOCK_FD: RawFd = 9;

/// The first door's number, per `weaver-state-Spec` section 2.
const FIRST_DOOR_FD: RawFd = 3;

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

struct Child(std::process::Child);

impl Drop for Child {
    fn drop(&mut self) {
        drop(self.0.kill());
        drop(self.0.wait());
    }
}

/// A close-on-exec copy of `fd` at 100 or above, so no source is a number
/// the child places, a `dup2` onto its own number leaving the flag standing.
fn high_copy(fd: RawFd) -> OwnedFd {
    // SAFETY: F_DUPFD_CLOEXEC on a descriptor this test owns answers a fresh
    // number this test then owns.
    let raw = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(raw >= 100, "a high copy");
    // SAFETY: the number was just made and is owned by nothing else.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

/// The descriptor's flags field from `/proc/<pid>/fdinfo/<fd>`, or `None`
/// while the number holds nothing.
fn fdinfo_flags(pid: u32, fd: RawFd) -> Option<u32> {
    let info = std::fs::read_to_string(format!("/proc/{pid}/fdinfo/{fd}")).ok()?;
    let line = info.lines().find(|line| line.starts_with("flags:"))?;
    u32::from_str_radix(line.trim_start_matches("flags:").trim(), 8).ok()
}

/// **The member marks the run lock close-on-exec first and keeps it**, per
/// `weaver-state-Spec` section 2: started the way admin starts it, with a
/// stream socket at the first door's number and a stand-in lock file at the
/// run lock's, the member's descriptor table shows the run lock with the flag
/// set, and still shows it while the member serves.
///
/// Perturbation: drop `keep_run_lock` from the member's `main` and the flag
/// never appears.
#[test]
fn the_member_keeps_the_run_lock_close_on_exec() {
    let dir = std::env::temp_dir().join(format!("weaver-member-run-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scratch = Scratch(dir.clone());
    let lock = {
        let low = std::fs::File::create(scratch.0.join("run.lock")).unwrap();
        high_copy(low.as_raw_fd())
    };
    let (ours, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
    let theirs = high_copy(theirs.as_raw_fd());
    let lock_raw = lock.as_raw_fd();
    let door_raw = theirs.as_raw_fd();
    let log_path = scratch.0.join("member.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_weaver-state"));
    command
        .arg(&scratch.0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log);
    // SAFETY: dup2 is async-signal-safe, and dup2 onto a new number clears the
    // close-on-exec flag there, so each arrives as the start step hands it.
    unsafe {
        command.pre_exec(move || {
            if nix::libc::dup2(door_raw, FIRST_DOOR_FD) == -1
                || nix::libc::dup2(lock_raw, RUN_LOCK_FD) == -1
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let member = Child(command.spawn().expect("the member starts"));
    let pid = member.0.id();
    drop((lock, theirs));

    let cloexec = nix::libc::O_CLOEXEC as u32;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if fdinfo_flags(pid, RUN_LOCK_FD).is_some_and(|flags| flags & cloexec != 0) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the member never marked the run lock close-on-exec: {}",
            std::fs::read_to_string(&log_path).unwrap_or_default()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Still held while the member serves its open door.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        fdinfo_flags(pid, RUN_LOCK_FD).is_some_and(|flags| flags & cloexec != 0),
        "the run lock is kept, never closed: {}",
        std::fs::read_to_string(&log_path).unwrap_or_default()
    );
    drop(ours);
}
