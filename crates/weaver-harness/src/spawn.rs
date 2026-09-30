//! conforms: harness-fork-to-exec-three-calls
//! conforms: harness-organ-argv-carries-construction-parameters
//! conforms: harness-organ-ends-from-descriptor-three
//! conforms: harness-child-flag-clear-unconditional
//!
//! The organ fork, per `weaver-harness-Spec` section 2.2. Between fork and
//! exec the child performs three calls, `dup2`, `fcntl`, and `execve`, and
//! nothing else: all three are async-signal-safe, and the bound is the safety
//! argument rather than a style, because the worker holds the writer's thread
//! at every fork and a child of a multithreaded process may safely run only
//! async-signal-safe calls before its exec.

use std::ffi::CString;
use std::path::Path;

use crate::channel::{ChildEnd, place_child_ends};

/// Forks an organ binary, handing it the ends it is owed at descriptor 3
/// upward in the channels' own order: the lifecycle channel every organ holds
/// takes 3, and an organ's further channel takes the next number, so the
/// gate's single end sits where the SPU's first does.
///
/// # Safety
///
/// The caller must be the serving thread, whose lifetime is the worker's: the
/// gate's parent-death backing fires on the forking thread's termination
/// rather than the process's, so a fork from a short-lived thread would kill
/// the gate spuriously while the interior it guards is healthy.
pub unsafe fn fork_organ(
    binary: &Path,
    ends: &[&ChildEnd],
    arguments: &[String],
    stderr: Option<std::os::fd::RawFd>,
) -> Result<nix::unistd::Pid, nix::Error> {
    let program =
        CString::new(binary.as_os_str().as_encoded_bytes()).map_err(|_| nix::Error::EINVAL)?;
    // **Built here, in the parent, before the fork.** The child's three calls
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
    // SAFETY: the caller's contract above, and the child body below runs only
    // async-signal-safe calls.
    match unsafe { nix::unistd::fork()? } {
        nix::unistd::ForkResult::Parent { child } => Ok(child),
        nix::unistd::ForkResult::Child => {
            // Call one and two, per the enumeration: dup2 each end into place
            // and clear the flag on each unconditionally.
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
            // `dup2`, inside the three-kind bound the module doc states.
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
            // Call three.
            let envp = [std::ptr::null()];
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

/// The retained line and whether the reader has seen end-of-stream. Until
/// `eof`, the reader thread may still be carrying a line from the pipe to
/// the cell; after it, the cell is final.
#[derive(Debug, Default)]
struct WordCell {
    word: Option<String>,
    eof: bool,
}

impl LastWord {
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
        let bound = std::time::Duration::from_millis(200);
        let deadline = std::time::Instant::now() + bound;
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

#[cfg(test)]
mod last_word_tests {
    use super::*;

    /// **The pipe carries the dying organ's last typed line**, per
    /// `weaver-harness-Spec` section 2.2, through a real fork and exec.
    ///
    /// Perturbation: fork with `stderr: None` and the word is absent, which
    /// is the second half below rather than a separate run.
    #[test]
    fn the_last_word_survives_a_real_death() {
        let (word, write_end) = LastWord::stand().expect("a pipe");
        let pid = unsafe {
            fork_organ(
                std::path::Path::new("/bin/sh"),
                &[],
                &[
                    "-c".to_string(),
                    r#"echo not json >&2; echo '{"died":"typed reason"}' >&2; exit 3"#.to_string(),
                ],
                Some(std::os::fd::AsRawFd::as_raw_fd(&write_end)),
            )
        }
        .expect("the fork");
        drop(write_end);
        let _ = nix::sys::wait::waitpid(pid, None);
        // EOF has landed with the wait done; the reader may need an instant
        // to drain the final buffer, bounded rather than raced.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(line) = word.take() {
                assert!(line.contains("typed reason"), "the typed line: {line}");
                assert!(
                    !line.contains("not json"),
                    "the unparsable line is teed, never retained: {line}"
                );
                break;
            }
            assert!(std::time::Instant::now() < deadline, "no word arrived");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        // The perturbation half: no pipe handed, no word held.
        let (quiet_word, unused) = LastWord::stand().expect("a pipe");
        let pid = unsafe {
            fork_organ(
                std::path::Path::new("/bin/sh"),
                &[],
                &["-c".to_string(), "echo '{\"x\":1}' >&2".to_string()],
                None,
            )
        }
        .expect("the fork");
        drop(unused);
        let _ = nix::sys::wait::waitpid(pid, None);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            quiet_word.take().is_none(),
            "with no descriptor placed the word never arrives"
        );
    }

    /// **The write end never travels as descriptor 2**, which is the
    /// aliasing corner of the placement: `dup2(2, 2)` is defined as a no-op
    /// returning the number, so it makes no copy, the flag the pipe carries
    /// stays set, and the exec closes the organ's stderr.
    ///
    /// **The corner is built rather than asserted about.** `pipe2` answers
    /// the lowest free numbers, so closing 0 and 2 puts the read end at 0
    /// and the write end at exactly 2. The arrangement runs in a forked
    /// child because closing a standard stream in the test process would
    /// reach every test sharing it, and the child does no allocation, so it
    /// stays inside what a fork of a threaded process may run. The child
    /// reports two facts, that the corner was reached and that the
    /// relocation cleared it, so a run where the arrangement failed cannot
    /// pass as a repair.
    ///
    /// The child asks twice: the plain call for the numbering, to establish
    /// that the arrangement reached the corner, and then `last_word_pipe`,
    /// which is the one place `stand` takes its pipe from.
    ///
    /// Perturbation: reduce `last_word_pipe` to its bare `pipe2` and the
    /// second bit reads zero.
    #[test]
    fn the_write_end_never_travels_as_descriptor_two() {
        use std::os::fd::{AsFd, AsRawFd};
        let (report_r, report_w) = nix::unistd::pipe().expect("the report pipe");
        // SAFETY: the child runs no allocation and no unwind, and leaves by
        // `_exit`, which is what a fork of a threaded process may do.
        match unsafe { nix::unistd::fork() }.expect("fork") {
            nix::unistd::ForkResult::Child => {
                // Close 0 and 2, keeping 1, so the next pipe takes 0 for its
                // reader and 2 for its writer.
                unsafe {
                    nix::libc::close(0);
                    nix::libc::close(2);
                }
                let mut report = 0u8;
                // The corner, established: the plain call takes 0 and 2.
                if let Ok((plain_read, plain_write)) =
                    nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
                {
                    if plain_write.as_raw_fd() == 2 {
                        report |= 1;
                    }
                    drop(plain_read);
                    drop(plain_write);
                }
                // The same table, asked of the constructor `stand` uses.
                if let Ok((read_end, write_end)) = last_word_pipe() {
                    if write_end.as_raw_fd() > 2 {
                        report |= 2;
                    }
                    drop(read_end);
                    drop(write_end);
                }
                let _ = nix::unistd::write(report_w.as_fd(), &[report]);
                // SAFETY: async-signal-safe and does not unwind.
                unsafe { nix::libc::_exit(0) };
            }
            nix::unistd::ForkResult::Parent { child } => {
                drop(report_w);
                let mut byte = [0u8; 1];
                let _ = nix::unistd::read(report_r.as_fd(), &mut byte);
                let _ = nix::sys::wait::waitpid(child, None);
                assert_eq!(
                    byte[0] & 1,
                    1,
                    "the arrangement did not reach the corner, so the run pins nothing"
                );
                assert_eq!(
                    byte[0] & 2,
                    2,
                    "the write end stayed at descriptor 2, where the placement makes no copy"
                );
            }
        }
    }

    /// **The last word's pipe closes on exec**, per
    /// `harness-organ-ends-from-descriptor-three`: an organ finds its ends
    /// at 3 and, where it has a second, at 4, and finds nothing else, so a
    /// pipe the parent holds at every fork must not ride in beside them.
    /// The organ entry counts what it inherited and refuses a count it did
    /// not expect, which is why a leak here is not a tidiness question but
    /// a refused load. The watch reads the flag on the end the caller
    /// receives, and `pipe2` sets both ends or neither, so the reader is
    /// bought by the same figure. The child's stderr is untouched, because
    /// `dup2` produces a copy without the flag.
    ///
    /// Perturbation: `pipe2(O_CLOEXEC)` back to the plain `pipe` and the
    /// flag reads clear here, which is the defect that refused every load
    /// with `descriptors_unusable` and four descriptors held.
    #[test]
    fn the_last_words_pipe_closes_on_exec() {
        let (_word, write_end) = LastWord::stand().expect("a pipe");
        // SAFETY: reading F_GETFD on a descriptor this frame owns.
        let flags = unsafe {
            nix::libc::fcntl(
                std::os::fd::AsRawFd::as_raw_fd(&write_end),
                nix::libc::F_GETFD,
            )
        };
        assert!(flags != -1, "the descriptor answers its flags");
        assert!(
            (flags & nix::libc::FD_CLOEXEC) != 0,
            "the last word's pipe must close on exec, or it rides into every organ"
        );
    }
}
