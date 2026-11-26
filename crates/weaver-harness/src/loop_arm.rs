//! **The loop arm**, per `weaver-harness-Spec` section 2.5: the run's one
//! field for a loop admin found standing, holding the runner's pid, the relay
//! over the near end of the loop's pair, and the last word's reader. The
//! worker starts it at the enter, last, and stops it at every leave, after the
//! gate is lowered and before the save point, so a leave tried again finds
//! none to stop twice.
//!
//! **The start.** The pair is made with `SOCK_CLOEXEC` in the creating call,
//! the launcher is forked by the organ fork as it stands with the far end at 3
//! and the last word's pipe at 2, the worker drops its copy of the far end and
//! of the pipe's write end in the same step, and making the relay reads the
//! runner's `hello` under its own bound. A launcher that refuses, a runner that
//! does not start, and a `hello` that is not the one expected each refuse the
//! start: the child is killed and reaped at once, the near end closes, and the
//! launcher's or the runner's last line, which the last word copies into the
//! worker's standard error, names why.
//!
//! **The stop.** The relay shuts its end down both ways and hands it back, the
//! end closes, the runner reads its end of file and exits, and the worker reaps
//! it by the leave's shared deadline, killing it there if it has not gone. The
//! stop leans on no signal before that deadline.

use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::Instant;

use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;
use weaver_types::WiresElection;

use crate::engine::{TurnError, TurnOutcome};
use crate::loop_relay::{Bounds, LoopRelay};
use crate::seat::Seat;
use crate::spawn::LastWord;

/// **Where the loop's launcher and its grant file stand**, a deployment fact
/// admin hands the worker as two named arguments where the enter will carry a
/// loop (`weaver-admin-Spec` section 6): the launcher beside the worker, the
/// grant file in the runner's folder. Neither is read from the enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopLaunch {
    /// `weaver-loop-launch`, beside the worker in `<prefix>/bin`.
    pub launcher: PathBuf,
    /// The grant file, `<prefix>/lib/weaver-loop/grant`.
    pub grant: PathBuf,
}

/// Why a loop did not start: the fork failed, or the relay refused the
/// `hello`, the reason being the relay's and the last word the launcher's or
/// the runner's own line where one was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRefused {
    /// The pair or the pipe could not be made, an argument held a NUL or was
    /// not UTF-8, or the fork failed.
    Fork(String),
    /// The child started and no `hello` the relay accepts arrived.
    Hello {
        reason: String,
        last_word: Option<String>,
    },
}

/// The standing loop: the runner's pid, the relay `Idle` or `Broken` between
/// passes, and the last word's reader.
pub struct LoopArm {
    pid: Pid,
    relay: LoopRelay<std::io::Stderr>,
    last_word: LastWord,
}

impl LoopArm {
    /// **Start the loop**: fork the launcher on `wires`, under the grant file
    /// `launch` names, and read the runner's `hello` against the digest admin
    /// read. The arm returned is `Idle`.
    ///
    /// # Safety
    ///
    /// As [`crate::spawn::fork_organ`]: the caller is the serving thread, whose
    /// lifetime is the worker's, so the parent-death signal the fork arms is
    /// not fired by a short-lived thread's end.
    pub unsafe fn start(
        launch: &LoopLaunch,
        wires: &WiresElection,
    ) -> Result<LoopArm, StartRefused> {
        // SAFETY: the caller's contract above.
        unsafe { Self::start_bounded(launch, wires, Bounds::contract()) }
    }

    /// [`LoopArm::start`] under the bounds a test sets.
    ///
    /// # Safety
    ///
    /// As [`LoopArm::start`].
    pub(crate) unsafe fn start_bounded(
        launch: &LoopLaunch,
        wires: &WiresElection,
        bounds: Bounds,
    ) -> Result<LoopArm, StartRefused> {
        let utf8 = |path: &std::path::Path, what: &str| {
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| StartRefused::Fork(format!("the {what} is not named in UTF-8")))
        };
        let arguments = [utf8(&launch.grant, "grant file")?, wires.file.clone()];
        // **Both ends close on exec from their creation**, as every organ
        // pair's do (`weaver-harness-Spec` section 2.2): the far end placed at
        // 3 by `dup2` is the one descriptor born without the flag.
        let (near, far): (OwnedFd, OwnedFd) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .map_err(|e| StartRefused::Fork(format!("the loop's pair: {e}")))?;
        let (last_word, stderr) = LastWord::stand()
            .map_err(|e| StartRefused::Fork(format!("the last word's pipe: {e}")))?;
        // SAFETY: the caller's contract, and `fork_organ`'s child runs only its
        // six async-signal-safe calls.
        let forked = unsafe {
            crate::spawn::fork_organ(
                &launch.launcher,
                &[far.as_fd()],
                &arguments,
                &[],
                Some(stderr.as_raw_fd()),
            )
        };
        // **The worker's copies of the far end and the pipe's write end drop
        // in the same step as the fork**, before any further fork, so no later
        // child holds the loop's end and the last word meets its end of file
        // when the runner dies.
        drop(far);
        drop(stderr);
        let pid = forked.map_err(|e| StartRefused::Fork(format!("the launcher's fork: {e}")))?;
        match LoopRelay::make_bounded(near, &wires.sha256, std::io::stderr(), bounds) {
            Ok(relay) => Ok(LoopArm {
                pid,
                relay,
                last_word,
            }),
            Err(refused) => {
                // **A failed start is killed and reaped at once**: the near end
                // closed with the relay that was never made, and the child,
                // whatever it is doing, does not outlive the refusal.
                let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
                let _ = waitpid(pid, None);
                Err(StartRefused::Hello {
                    reason: refused.reason,
                    last_word: last_word.take(),
                })
            }
        }
    }

    /// **One pass**, the relay's: the loop composes the turn through `seat`.
    pub fn pass(&mut self, seat: &mut impl Seat, text: &str) -> Result<TurnOutcome, TurnError> {
        self.relay.pass(seat, text)
    }

    /// The runner's pid.
    pub fn pid(&self) -> Pid {
        self.pid
    }

    /// **Stop the loop**: shut the end down, close it, and reap the runner by
    /// `deadline`, killing it there (`SIGKILL`) if it has not gone, then reap
    /// it. Unbounded where no deadline stands. Then the last word settles, so
    /// the runner's end-of-file line has reached the worker's standard error
    /// when the stop returns. The runner's status where this worker reaped
    /// it, `None` where it was not this worker's to wait on.
    pub fn stop(self, deadline: Option<Instant>) -> Option<WaitStatus> {
        drop(self.relay.shut_down());
        let status = reap_by(self.pid, deadline);
        // Bounded by the last word's own settle bound: the runner is gone,
        // so its end of the pipe is closed and the end of file is queued.
        let _ = self.last_word.take();
        status
    }
}

fn reap_by(pid: Pid, deadline: Option<Instant>) -> Option<WaitStatus> {
    let Some(deadline) = deadline else {
        return waitpid(pid, None).ok();
    };
    loop {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {}
            Ok(status) => return Some(status),
            Err(_) => return None,
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
            return waitpid(pid, None).ok();
        }
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(5)));
    }
}
