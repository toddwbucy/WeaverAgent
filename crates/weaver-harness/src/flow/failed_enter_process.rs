//! Actual fork custody. No scalar PID constructor or blocking wait is exposed.

use std::os::fd::{AsRawFd, BorrowedFd};
use std::path::Path;

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ChildState {
    Running,
    Reaped(WaitStatus),
    NotWaitable,
}

pub(in crate::flow::context) struct OwnedChild {
    pid: Pid,
    state: ChildState,
    kill_attempt: Option<Result<(), Errno>>,
    wait_error: Option<Errno>,
    // Do not call LastWord::take during cleanup: it has its own 200ms wait.
    _last_word: crate::spawn::LastWord,
}

impl OwnedChild {
    /// # Safety
    /// The caller is the serving thread and remains alive until this child's
    /// custody is returned/reaped or the whole worker exits failed. This is the
    /// same fork-thread obligation as spawn::fork_organ, not another runtime.
    pub(in crate::flow::context) unsafe fn spawn(
        binary: &Path,
        ends: &[BorrowedFd<'_>],
        arguments: &[String],
    ) -> Result<Self, String> {
        let (last_word, stderr) =
            crate::spawn::LastWord::stand().map_err(|error| format!("child stderr: {error}"))?;
        // SAFETY: the caller carries fork_organ's serving-thread obligation.
        let pid =
            unsafe { crate::spawn::fork_organ(binary, ends, arguments, Some(stderr.as_raw_fd())) }
                .map_err(|error| format!("child fork: {error}"))?;
        // Retain the PID before any further fallible action, and close the
        // parent's write copy so child exit can deliver stderr EOF.
        let child = Self {
            pid,
            state: ChildState::Running,
            kill_attempt: None,
            wait_error: None,
            _last_word: last_word,
        };
        drop(stderr);
        Ok(child)
    }

    pub(in crate::flow::context) fn state(&self) -> ChildState {
        self.state
    }
    /// Read the actual stderr owner without consuming its word or waiting on
    /// its reader. The lifecycle owner supplies the existing settlement cap.
    pub(in crate::flow::context) fn last_word_snapshot(
        &self,
    ) -> Option<crate::spawn::LastWordSnapshot> {
        self._last_word.snapshot()
    }
    pub(super) fn kill_attempt(&self) -> Option<Result<(), Errno>> {
        self.kill_attempt
    }
    pub(super) fn wait_error(&self) -> Option<Errno> {
        self.wait_error
    }

    /// One nonblocking syscall; Interrupted/StillAlive never mean reaped.
    pub(in crate::flow::context) fn poll(&mut self) -> ChildState {
        if self.state != ChildState::Running {
            return self.state;
        }
        match waitpid(self.pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(status @ (WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _))) => {
                self.state = ChildState::Reaped(status);
            }
            Ok(_) => {}
            Err(Errno::ECHILD) => {
                self.wait_error.get_or_insert(Errno::ECHILD);
                self.state = ChildState::NotWaitable;
            }
            Err(Errno::EINTR) => {}
            Err(error) => {
                self.wait_error.get_or_insert(error);
            }
        }
        self.state
    }

    /// At the shared cap, attempt kill once then one nonblocking reap. A live
    /// result retains this exact owner; neither kill success nor ECHILD is a
    /// successful child exit. Caller must exit failed if custody remains.
    pub(in crate::flow::context) fn stop_at_cap(&mut self) -> ChildState {
        self.poll();
        if self.state == ChildState::Running && self.kill_attempt.is_none() {
            self.kill_attempt = Some(kill(self.pid, Signal::SIGKILL));
        }
        self.poll()
    }
}

// No Drop wait: worker exit/PDEATHSIG and admin cgroup escalation keep their
// existing roles. A returned unreaped owner is not a fictitious reap result.
