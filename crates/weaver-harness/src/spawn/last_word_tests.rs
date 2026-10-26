//! Test-only: the `last_word_tests` suite of `spawn.rs`, moved out of the production file by #118.

use super::*;

fn wait_snapshot(word: &LastWord, ready: impl Fn(&LastWordSnapshot) -> bool) -> LastWordSnapshot {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(snapshot) = word.snapshot().filter(&ready) {
            return snapshot;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "stderr snapshot did not arrive"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn last_word_snapshot_retains_typed_lines_without_consuming_or_waiting_for_eof() {
    use std::io::Write;
    let (word, write_end) = LastWord::stand().unwrap();
    let mut writer = std::fs::File::from(write_end);
    let empty = wait_snapshot(&word, |_| true);
    assert!(!empty.eof);
    assert!(empty.word.is_none());
    writer.write_all(b"not json\n{\"stage\":1}\n").unwrap();
    let first = wait_snapshot(&word, |snapshot| snapshot.word.is_some());
    assert!(!first.eof, "the actual pipe writer remains open");
    assert_eq!(first.word, Some(serde_json::json!({"stage": 1})));
    assert_eq!(wait_snapshot(&word, |_| true), first);

    writer
        .write_all(b"[\"final\",2]\nstill not json\n")
        .unwrap();
    drop(writer);
    let final_word = wait_snapshot(&word, |snapshot| snapshot.eof);
    assert_eq!(final_word.word, Some(serde_json::json!(["final", 2])));
    assert_eq!(wait_snapshot(&word, |_| true), final_word);
    assert_eq!(word.take().as_deref(), Some("[\"final\",2]"));
    assert_eq!(wait_snapshot(&word, |_| true), final_word);
    assert_eq!(word.take().as_deref(), Some("[\"final\",2]"));
}

#[test]
fn last_word_snapshot_eof_without_json_keeps_the_word_absent() {
    use std::io::Write;
    for bytes in [b"".as_slice(), b"garble only\n".as_slice()] {
        let (word, write_end) = LastWord::stand().unwrap();
        let mut writer = std::fs::File::from(write_end);
        writer.write_all(bytes).unwrap();
        drop(writer);
        let snapshot = wait_snapshot(&word, |snapshot| snapshot.eof);
        assert!(snapshot.word.is_none());
        assert_eq!(wait_snapshot(&word, |_| true), snapshot);
        assert!(word.take().is_none());
    }
}

#[test]
fn last_word_snapshot_does_not_wait_for_the_reader_mutex() {
    let (word, write_end) = LastWord::stand().unwrap();
    std::thread::scope(|scope| {
        let guard = word.cell.0.lock().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let observed = &word;
        let worker = scope.spawn(move || sender.send(observed.snapshot()).unwrap());
        let received = receiver.recv_timeout(std::time::Duration::from_secs(5));
        // Release the lock before failing or joining, so a blocking regression
        // fails this assertion instead of trapping the scoped worker forever.
        drop(guard);
        worker.join().unwrap();
        assert_eq!(received.expect("snapshot waited on the held mutex"), None);
    });
    drop(write_end);
    assert!(wait_snapshot(&word, |snapshot| snapshot.eof).word.is_none());
}

#[test]
fn last_word_snapshot_preserves_the_pipe_evidence_after_mutex_poisoning() {
    use std::io::Write;
    let (word, write_end) = LastWord::stand().unwrap();
    let mut writer = std::fs::File::from(write_end);
    writer.write_all(b"{\"died\":\"typed reason\"}\n").unwrap();
    drop(writer);
    let expected = wait_snapshot(&word, |snapshot| snapshot.eof);
    let cell = word.cell.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = cell.0.lock().unwrap();
            panic!("poison only after actual stderr evidence settled");
        })
        .join()
        .is_err()
    );
    assert_eq!(word.snapshot(), Some(expected));
    assert_eq!(word.take().as_deref(), Some("{\"died\":\"typed reason\"}"));
}

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

/// **The organ starts with the worker's environment**, per
/// `weaver-harness-Spec` section 2.2 and the operator's ruling of
/// 2026-10-01, so a variable the agent's unit sets reaches every organ.
/// The probe is a variable cargo sets on the test process and no shell
/// sets for itself, read back as the organ's last typed line.
///
/// Perturbation: hand `execve` an empty `envp` and the organ reports the
/// variable empty.
#[test]
fn an_organ_inherits_the_workers_environment() {
    let expected = std::env::var("CARGO_PKG_NAME").expect("cargo sets it on a test run");
    let (word, write_end) = LastWord::stand().expect("a pipe");
    let pid = unsafe {
        fork_organ(
            std::path::Path::new("/bin/sh"),
            &[],
            &[
                "-c".to_string(),
                r#"printf '{"probe":"%s"}\n' "$CARGO_PKG_NAME" >&2"#.to_string(),
            ],
            Some(std::os::fd::AsRawFd::as_raw_fd(&write_end)),
        )
    }
    .expect("the fork");
    drop(write_end);
    let _ = nix::sys::wait::waitpid(pid, None);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let line = loop {
        if let Some(line) = word.take() {
            break line;
        }
        assert!(std::time::Instant::now() < deadline, "no word arrived");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(
        line,
        format!(r#"{{"probe":"{expected}"}}"#),
        "the organ saw the worker's variable"
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
            if let Ok((plain_read, plain_write)) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
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
