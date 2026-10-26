//! conforms: harness-fork-to-exec-bounded-calls
//! conforms: harness-organ-argv-carries-construction-parameters
//! conforms: harness-organ-ends-from-descriptor-three
//! conforms: harness-child-flag-clear-unconditional
//!
//! The organ fork, per `weaver-harness-Spec` section 2.2. Between fork and
//! exec the child performs six calls, `prctl`, `getppid`, `_exit` where the
//! parent is gone, `dup2`, `fcntl`, and `execve`, and nothing else: all six
//! are async-signal-safe, and the bound is the safety argument rather than a
//! style, because the worker holds the writer's thread at every fork and a
//! child of a multithreaded process may safely run only async-signal-safe
//! calls before its exec.
//!
//! **The organs die with the worker**, on the operator's ruling of 2026-10-03
//! on #50 that the agent leaves systemd, whose cgroup ended every process of
//! the unit: the child arms `PR_SET_PDEATHSIG` to `SIGKILL` first, so the
//! kernel kills the organ the instant the worker dies, and then compares
//! `getppid` with the worker's pid read before the fork, because a worker
//! that died between the fork and the arming would otherwise leave a child
//! the signal was armed too late to reach.

use std::ffi::CString;
use std::path::Path;

use std::os::fd::BorrowedFd;

use crate::channel::place_child_ends;

/// Forks an organ binary, handing it the ends it is owed at descriptor 3
/// upward in the channels' own order: the lifecycle channel every organ holds
/// takes 3, and an organ's further channel takes the next number, so the
/// gate's single end sits where the SPU's first does.
///
/// # Safety
///
/// The caller must be the serving thread, whose lifetime is the worker's: the
/// parent-death signal armed here, like the gate's own backing, fires on the
/// forking thread's termination rather than the process's, so a fork from a
/// short-lived thread would kill the organ spuriously while the worker is
/// healthy. The worker forks from its main thread, which lives for the
/// process.
pub unsafe fn fork_organ(
    binary: &Path,
    ends: &[BorrowedFd<'_>],
    arguments: &[String],
    stderr: Option<std::os::fd::RawFd>,
) -> Result<nix::unistd::Pid, nix::Error> {
    let program =
        CString::new(binary.as_os_str().as_encoded_bytes()).map_err(|_| nix::Error::EINVAL)?;
    // **Built here, in the parent, before the fork.** The child's six calls
    // are the safety bound of the doc comment above, so a vector assembled
    // after the fork would not be expressible: allocation is not
    // async-signal-safe. The pointers below borrow these, so both outlive the
    // exec that reads them.
    let arguments: Vec<CString> = arguments
        .iter()
        .map(|argument| CString::new(argument.as_bytes()).map_err(|_| nix::Error::EINVAL))
        .collect::<Result<_, _>>()?;
    let mut argv: Vec<*const nix::libc::c_char> = Vec::with_capacity(arguments.len() + 2);
    argv.push(program.as_ptr());
    argv.extend(arguments.iter().map(|argument| argument.as_ptr()));
    argv.push(std::ptr::null());
    // **The organ starts as the agent is constituted**, per `weaver-harness-Spec`
    // section 2.2 and the operator's ruling of 2026-10-01: the worker's
    // environment, which is the agent's unit as configured, `Environment=`
    // included. Read and built here in the parent for the reason the vector
    // above is, so the child reads a finished array rather than the process's
    // `environ`, which another thread of the worker could be changing.
    let environment: Vec<CString> = std::env::vars_os()
        .filter_map(|(name, value)| {
            let mut pair = name.into_encoded_bytes();
            pair.push(b'=');
            pair.extend(value.into_encoded_bytes());
            CString::new(pair).ok()
        })
        .collect();
    let mut envp: Vec<*const nix::libc::c_char> = Vec::with_capacity(environment.len() + 1);
    envp.extend(environment.iter().map(|pair| pair.as_ptr()));
    envp.push(std::ptr::null());
    // Read before the fork, so the child compares against the worker and not
    // against whatever adopted it.
    let worker = nix::unistd::getpid().as_raw();
    // SAFETY: the caller's contract above, and the child body below runs only
    // async-signal-safe calls.
    match unsafe { nix::unistd::fork()? } {
        nix::unistd::ForkResult::Parent { child } => Ok(child),
        nix::unistd::ForkResult::Child => {
            // Calls one to three: arm the death signal, then confirm the
            // worker still stands, so no window leaves an organ orphaned.
            // SAFETY: prctl, getppid and _exit are async-signal-safe.
            // **A refused arming refuses the organ**: an organ that runs
            // without its death signal could outlive the worker, so a
            // rejection, a seccomp policy's among them, ends the child before
            // its exec rather than letting the invariant lapse silently.
            unsafe {
                if nix::libc::prctl(
                    nix::libc::PR_SET_PDEATHSIG,
                    nix::libc::SIGKILL as nix::libc::c_ulong,
                    0,
                    0,
                    0,
                ) == -1
                {
                    nix::libc::_exit(DEATH_SIGNAL_REFUSED);
                }
                if nix::libc::getppid() != worker {
                    nix::libc::_exit(WORKER_GONE);
                }
            }
            // Calls four and five, per the enumeration: dup2 each end into
            // place and clear the flag on each unconditionally.
            // SAFETY: in the child, before exec, with ends this process made.
            //
            // **A failed placement does not reach the exec.** The child cannot
            // return an error to the parent, but it can refuse to run the
            // organ: an organ started with no channel at descriptor 3 would
            // report as a residency problem at the parent's first exchange
            // rather than as the placement fault it is. The distinct status
            // is what lets the parent tell the two apart.
            // **Descriptor 2 first, and by the same call kind.** The pipe's
            // write end lands where stderr lives, so the organ's last typed
            // line has somewhere to go that the parent holds - one more
            // `dup2`, inside the six-kind bound the module doc states.
            // Placed before the ends because 2 sits below their range and
            // an end landing on the pipe's original number afterwards only
            // closes a copy already duplicated home.
            if let Some(write_end) = stderr {
                // SAFETY: dup2 is async-signal-safe; a failure leaves the
                // inherited stderr standing, which is the pre-pipe world and
                // costs the last word, never the organ.
                unsafe { nix::libc::dup2(write_end, 2) };
            }
            if unsafe { place_child_ends(ends) }.is_err() {
                // SAFETY: _exit is async-signal-safe and does not unwind.
                unsafe { nix::libc::_exit(PLACEMENT_FAILED) };
            }
            // Call six.
            // SAFETY: execve is async-signal-safe; on success it does not
            // return, and on failure the child exits without unwinding.
            unsafe {
                nix::libc::execve(program.as_ptr(), argv.as_ptr(), envp.as_ptr());
                nix::libc::_exit(EXEC_FAILED);
            }
        }
    }
}

/// The worker's end of an organ's stderr: a tee that loses the journal
/// nothing, and the retained last typed line, per `weaver-harness-Spec`
/// section 2.2 and issue #360.
///
/// The reader thread copies every line through to the worker's own stderr,
/// so what reached the journal before this type reaches it still, and
/// retains the last line that parses as JSON - the SPU dies printing a
/// typed reason, and the death's account carries it as `last_word` rather
/// than the reason dying with the journal. A line that parses is retained
/// parsed-shape-agnostic: the account carries what was said, not what this
/// crate hoped was said.
///
/// conforms: harness-death-carries-the-last-word
#[derive(Debug)]
pub struct LastWord {
    cell: std::sync::Arc<(std::sync::Mutex<WordCell>, std::sync::Condvar)>,
}

/// The existing bound on settling the dying organ's final stderr line.
pub(crate) const LAST_WORD_SETTLE_BOUND: std::time::Duration =
    std::time::Duration::from_millis(200);

/// One nonconsuming observation of the reader's retained typed line and EOF.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LastWordSnapshot {
    pub(crate) word: Option<serde_json::Value>,
    pub(crate) eof: bool,
}

/// The retained line and whether the reader has seen end-of-stream. Until
/// `eof`, the reader thread may still be carrying a line from the pipe to
/// the cell; after it, the cell is final.
#[derive(Debug, Default)]
struct WordCell {
    word: Option<String>,
    eof: bool,
}

impl LastWord {
    /// Snapshot the retained line without waiting on the reader or its mutex.
    /// `None` means the cell is busy, not that EOF or an absent word was seen.
    /// The raw line stays with this owner for subsequent snapshots and `take`.
    pub(crate) fn snapshot(&self) -> Option<LastWordSnapshot> {
        let cell = match self.cell.0.try_lock() {
            Ok(cell) => cell,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return None,
        };
        let word = cell.word.clone();
        let eof = cell.eof;
        drop(cell);
        Some(LastWordSnapshot {
            word: word.and_then(|line| serde_json::from_str(&line).ok()),
            eof,
        })
    }

    /// The pipe and its reader, returning the child's write end raw for the
    /// fork to place. The parent must close its copy of the write end after
    /// the fork - the returned `OwnedFd` is exactly that copy - or the
    /// reader never sees EOF.
    pub fn stand() -> Result<(Self, std::os::fd::OwnedFd), nix::Error> {
        use std::io::BufRead as _;
        use std::io::Write as _;
        // **One constructor, so the pipe's properties have one home.** The
        // flag every organ's entry depends on and the numbering the child's
        // placement depends on are both argued at `last_word_pipe`.
        let (read_end, write_end) = last_word_pipe()?;
        let cell = std::sync::Arc::new((
            std::sync::Mutex::new(WordCell::default()),
            std::sync::Condvar::new(),
        ));
        let retained = cell.clone();
        let reader = std::fs::File::from(read_end);
        // The thread ends at EOF, which arrives when the organ dies and the
        // parent's write-end copy is closed. It is deliberately not joined:
        // an organ that closes stderr early or never dies must not hold a
        // leave hostage, and the tee writes through as lines arrive.
        std::thread::spawn(move || {
            let mut lines = std::io::BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match lines.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                // The tee: the journal keeps every line it kept before.
                let _ = std::io::stderr().write_all(line.as_bytes());
                let trimmed = line.trim();
                if !trimmed.is_empty() && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
                {
                    retained.0.lock().unwrap_or_else(|p| p.into_inner()).word =
                        Some(trimmed.to_string());
                }
            }
            retained.0.lock().unwrap_or_else(|p| p.into_inner()).eof = true;
            retained.1.notify_all();
        });
        Ok((LastWord { cell }, write_end))
    }
}

/// The last word's pipe, with both properties the placement depends on.
///
/// **Both ends close on exec**, per the descriptor hygiene the organ entry
/// enforces: an organ counts what it inherited and refuses a count it did not
/// expect, so a pipe created with the plain call - which sets no `O_CLOEXEC` -
/// leaks its reader and the parent's write end into every organ and refuses
/// the load with `descriptors_unusable`. The child's stderr survives the flag,
/// because `dup2` produces a descriptor without it and the copy onto 2 is what
/// the exec keeps.
///
/// **And the write end travels clear of descriptor 2**, because that copy is
/// only made where the call has two different numbers to work with. `dup2(2,
/// 2)` is defined as a no-op returning the number, so a write end that arrives
/// at 2 is never copied, the flag stays set, and the exec closes the organ's
/// stderr: the last word has nowhere to go and the journal loses what it used
/// to keep. `pipe2` answers the lowest free numbers, so the corner is reached
/// by a parent that has closed its own stderr. **Moving the end is the repair
/// rather than clearing the flag in the child**, because the same corner
/// leaves the parent's own stderr on this pipe and the tee would then write
/// its own input back.
///
/// `F_DUPFD_CLOEXEC` answers the lowest free number at or above the floor and
/// sets the flag on what it answers, so the property the pipe was created with
/// survives the move. The original is dropped, closing the low number.
/// Nothing here allocates, so the whole of it is usable where the fork's bound
/// applies.
fn last_word_pipe() -> Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd), nix::Error> {
    use std::os::fd::{AsFd, AsRawFd, FromRawFd, RawFd};
    const FLOOR: RawFd = 3;
    let (read_end, write_end) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)?;
    if write_end.as_raw_fd() >= FLOOR {
        return Ok((read_end, write_end));
    }
    let raw = nix::fcntl::fcntl(
        write_end.as_fd(),
        nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC(FLOOR),
    )?;
    // SAFETY: `F_DUPFD_CLOEXEC` answered a fresh descriptor this process owns,
    // and `write_end` is dropped here, so the low number is closed exactly
    // once and the new number has exactly one owner.
    Ok((read_end, unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) }))
}

impl LastWord {
    /// A word that never speaks, for scripted arms in tests: no pipe, no
    /// thread, `take` answering the same absence a silent organ leaves.
    // Test-only hook kept here (#118): a `LastWord` constructor the lifecycle suite uses.
    #[cfg(test)]
    pub fn quiet() -> Self {
        LastWord {
            cell: std::sync::Arc::new((
                std::sync::Mutex::new(WordCell {
                    word: None,
                    eof: true,
                }),
                std::sync::Condvar::new(),
            )),
        }
    }

    /// A scripted arm speaks: the test rig's route to a retained line
    /// without a pipe, so the account enrichment is watchable where no organ
    /// was forked.
    // Test-only hook kept here (#118): a `LastWord` method the lifecycle suite uses.
    #[cfg(test)]
    pub(crate) fn speak(&self, line: &str) {
        self.cell.0.lock().unwrap_or_else(|p| p.into_inner()).word = Some(line.to_string());
    }

    /// The last typed line the organ spoke, if it spoke one. The read
    /// settles on the reader's end-of-stream, bounded: a dead organ's EOF
    /// is already queued when the author site calls this, so the wait is
    /// microseconds there, while the bound keeps a descriptor that outlives
    /// its organ - or an arm that never died at all - from holding the
    /// death hostage. A timeout reads whatever the cell holds, which is the
    /// tail race the Spec names: it costs the member, never the death.
    pub fn take(&self) -> Option<String> {
        let deadline = std::time::Instant::now() + LAST_WORD_SETTLE_BOUND;
        let mut cell = self.cell.0.lock().unwrap_or_else(|p| p.into_inner());
        while !cell.eof {
            let now = std::time::Instant::now();
            if now >= deadline {
                break;
            }
            let (guard, _) = self
                .cell
                .1
                .wait_timeout(cell, deadline - now)
                .unwrap_or_else(|p| p.into_inner());
            cell = guard;
        }
        cell.word.clone()
    }
}

/// The child exited because it could not place the ends it was handed, which
/// is a placement fault and not a residency one.
pub const PLACEMENT_FAILED: i32 = 126;

/// The child exited because the organ binary could not be exec'd.
pub const EXEC_FAILED: i32 = 127;

/// The child exited because the worker died between the fork and the arming
/// of its death signal, so the organ never ran.
pub const WORKER_GONE: i32 = 125;

/// The child exited because the kernel refused its death signal, so the
/// organ never ran rather than running unbound to the worker.
pub const DEATH_SIGNAL_REFUSED: i32 = 124;

#[cfg(test)]
mod last_word_tests;

#[cfg(test)]
mod death_tests;
