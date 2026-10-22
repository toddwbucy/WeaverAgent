//! A build without an engine remains executable and refuses before touching state.
#![cfg(all(target_os = "linux", not(feature = "sqlite")))]

use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

fn duplicate_above(fd: RawFd, minimum: RawFd) -> OwnedFd {
    // SAFETY: duplicate a borrowed descriptor without replacing an owned slot.
    let raw = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, minimum) };
    assert!(raw >= minimum);
    // SAFETY: the successful duplicate is owned by no other value.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

#[test]
fn no_engine_refuses_every_start_mode_without_answering_or_touching_holdings() {
    for (mode, has_run_lock) in ["legacy", "legacy-preload", "serving", "diagnostic", "seed"]
        .into_iter()
        .flat_map(|mode| [true, false].map(|has_run_lock| (mode, has_run_lock)))
    {
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "weaver-state-no-engine-{}-{mode}-{has_run_lock}",
            std::process::id()
        )));
        std::fs::create_dir(&scratch.0).unwrap();
        let sentinel = scratch.0.join(".part-held");
        std::fs::write(&sentinel, b"retained before startup").unwrap();
        let preload = scratch.0.join("preload.sock");
        let lock_file = std::fs::File::create(scratch.0.join("run.lock")).unwrap();
        let lock = duplicate_above(lock_file.as_raw_fd(), 100);
        // Keep descriptor 9 occupied in the parent while std creates its exec
        // status pipe. The child can then close that slot deliberately without
        // accidentally closing the pipe used to report an exec failure.
        let _reserved_fd9 = duplicate_above(lock_file.as_raw_fd(), 9);
        let (mut ours, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
        let door = duplicate_above(theirs.as_raw_fd(), 100);
        drop(theirs);
        let door_raw = door.as_raw_fd();
        let lock_raw = lock.as_raw_fd();
        let log_path = scratch.0.join("member.log");
        let log = std::fs::File::create(&log_path).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-state"));
        match mode {
            "legacy" => {
                command.arg(&scratch.0);
            }
            "legacy-preload" => {
                command.arg(&scratch.0).arg(&preload);
            }
            "diagnostic" => {
                command
                    .arg("--mode=diagnostic")
                    .arg(&scratch.0)
                    .arg(&preload);
            }
            _ => {
                command.arg(format!("--mode={mode}")).arg(&scratch.0);
            }
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log);
        // SAFETY: only async-signal-safe descriptor operations run after fork.
        // Sources are above the fixed slots. fd4 is deliberately absent, as on
        // a legacy first load, and each dup2 clears CLOEXEC on its destination.
        unsafe {
            command.pre_exec(move || {
                if nix::libc::dup2(door_raw, 3) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if has_run_lock {
                    if nix::libc::dup2(lock_raw, 9) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                } else if nix::libc::close(9) == -1 {
                    let error = std::io::Error::last_os_error();
                    if error.raw_os_error() != Some(nix::libc::EBADF) {
                        return Err(error);
                    }
                }
                if nix::libc::close(4) == -1 {
                    let error = std::io::Error::last_os_error();
                    if error.raw_os_error() != Some(nix::libc::EBADF) {
                        return Err(error);
                    }
                }
                Ok(())
            });
        }
        let mut child = Child(command.spawn().unwrap());
        drop((door, lock));
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "{mode}: startup did not refuse");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(1), "{mode}: refusal is not success");
        let log = std::fs::read_to_string(&log_path).unwrap();
        let records: Vec<serde_json::Value> = log
            .lines()
            .map(|line| serde_json::from_str(line).expect("structured startup record"))
            .collect();
        let mut expected = Vec::new();
        if !has_run_lock {
            expected.push(serde_json::json!({"state_notice":
                "no run lock at descriptor 9: not started by admin's start step"}));
        }
        expected.push(serde_json::json!({"state_fault":
            "StoreUnavailable(\"sqlite engine is not compiled into this binary\")"}));
        assert_eq!(
            records, expected,
            "{mode}, run lock present={has_run_lock}: run-lock handling precedes refusal"
        );
        let mut stdout = Vec::new();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut stdout)
            .unwrap();
        assert!(stdout.is_empty(), "{mode}: no stdout answer");
        ours.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut byte = [0];
        assert_eq!(ours.read(&mut byte).unwrap(), 0, "{mode}: no seam answer");
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            b"retained before startup",
            "{mode}: no room cleanup"
        );
        assert!(!preload.exists(), "{mode}: no preload door");
    }
}
