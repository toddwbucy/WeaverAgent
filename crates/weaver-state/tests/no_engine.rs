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

fn high_copy(fd: RawFd) -> OwnedFd {
    // SAFETY: duplicate a borrowed descriptor above the child's fixed slots.
    let raw = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(raw >= 100);
    // SAFETY: the successful duplicate is owned by no other value.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

#[test]
fn no_engine_refuses_every_start_mode_without_answering_or_touching_holdings() {
    for mode in ["legacy", "legacy-preload", "serving", "diagnostic", "seed"] {
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "weaver-state-no-engine-{}-{mode}",
            std::process::id()
        )));
        std::fs::create_dir(&scratch.0).unwrap();
        let sentinel = scratch.0.join(".part-held");
        std::fs::write(&sentinel, b"retained before startup").unwrap();
        let preload = scratch.0.join("preload.sock");
        let lock_file = std::fs::File::create(scratch.0.join("run.lock")).unwrap();
        let lock = high_copy(lock_file.as_raw_fd());
        let (mut ours, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
        let door = high_copy(theirs.as_raw_fd());
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
                if nix::libc::dup2(door_raw, 3) == -1 || nix::libc::dup2(lock_raw, 9) == -1 {
                    return Err(std::io::Error::last_os_error());
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
        let faults: Vec<serde_json::Value> = log
            .lines()
            .map(|line| serde_json::from_str(line).expect("structured startup fault"))
            .collect();
        assert_eq!(faults.len(), 1, "{mode}: {log}");
        assert_eq!(
            faults[0],
            serde_json::json!({"state_fault":
                "StoreUnavailable(\"sqlite engine is not compiled into this binary\")"}),
            "{mode}"
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
