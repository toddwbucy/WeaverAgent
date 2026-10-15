//! The shell execution's suite. **Every test that runs a shell is a cgroup
//! instrument** (the Planner's ruling of 2026-10-15 on #111's PR C): the
//! shell's child moves itself into `tools/` before its exec, so each runs in
//! a scratch `tools/` of its own beneath the user manager's delegated scope,
//! `#[ignore]`d and run as
//! `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-gate --locked -- --ignored tools::`,
//! and outside a delegated scope it fails naming why, never passing by skip.
//! A refusal runs no shell, so those stay plain tests, handed a plain
//! directory as their `tools/`: a regression that spawned would meet a
//! `cgroup.procs` that is not there and answer `Errored`.

use super::*;
use weaver_types::ToolName;

/// One execution in a scratch `tools/` of its own.
fn execute(execution: &ToolExecution) -> ToolOutcome {
    let scratch = ScratchTools::new();
    let (near, _far) = pair();
    super::execute_in(execution, &near, &exchange(), &scratch.tools).expect("execution channel")
}

/// One execution handed a plain directory as its `tools/`, for the refusals.
fn refuse(execution: &ToolExecution) -> ToolOutcome {
    let (near, _far) = pair();
    let plain = std::fs::File::open(std::env::temp_dir()).expect("a plain directory opens");
    let tools = Tools::from_owned(plain.into());
    super::execute_in(execution, &near, &exchange(), &tools).expect("execution channel")
}

/// A scratch `tools/` cgroup beneath the delegated scope this suite runs in,
/// one per test, since the suite's tests run in parallel and a kill of a
/// shared one would end another test's shell. Killed and removed when
/// dropped.
pub(crate) struct ScratchTools {
    pub(crate) path: std::path::PathBuf,
    pub(crate) tools: Tools,
}

impl ScratchTools {
    pub(crate) fn new() -> Self {
        use std::os::unix::fs::MetadataExt;
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let own = std::fs::read_to_string("/proc/self/cgroup")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix("0::"))
                    .map(|own| own.trim().trim_end_matches('/').to_string())
            })
            .expect("/proc/self/cgroup names a unified cgroup");
        let parent = std::path::PathBuf::from(format!("/sys/fs/cgroup{own}"));
        assert!(
            std::fs::metadata(&parent)
                .is_ok_and(|meta| meta.uid() == nix::unistd::getuid().as_raw())
                && parent.join("cgroup.procs").exists(),
            "the shell's tests need a delegated scope this account owns, and {own} is not one: run them as \
             `systemd-run --user --scope -p Delegate=yes cargo test -p weaver-gate --locked -- --ignored tools::`"
        );
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = parent.join(format!("weaver-gate-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir(&path);
        std::fs::create_dir(&path).expect("a scratch tools/ is made");
        let dir = std::fs::File::open(&path).expect("the scratch tools/ opens");
        Self {
            path,
            tools: Tools::from_owned(dir.into()),
        }
    }
}

impl Drop for ScratchTools {
    fn drop(&mut self) {
        let _ = self.tools.kill();
        let _ = self.tools.wait_empty(std::time::Duration::from_secs(5));
        let _ = std::fs::remove_dir(&self.path);
    }
}

fn pair() -> (Channel, Channel) {
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (
        crate::channel::from_owned(near),
        crate::channel::from_owned(far),
    )
}

fn exchange() -> ExchangeId {
    ExchangeId {
        opener: weaver_types::Opener::Harness,
        ordinal: 1,
    }
}

std::thread_local! {
    static WAIT_ENTERED: std::cell::RefCell<Option<std::sync::mpsc::SyncSender<()>>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn wait_entered() {
    WAIT_ENTERED.with(|slot| {
        if let Some(sender) = slot.borrow_mut().take() {
            sender.send(()).unwrap();
        }
    });
}

/// **A tool's command does not see the gate's environment.** The gate
/// inherits the agent's, and the command is handed `HOME` alone. The
/// probe is a variable cargo sets on the test process.
///
/// Perturbation: drop the `env_clear` and the command prints the value.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_command_sees_home_and_not_the_gates_environment() {
    assert!(
        std::env::var_os("CARGO_PKG_NAME").is_some(),
        "cargo sets it"
    );
    let outcome = execute(&ToolExecution {
        name: ToolName(SHELL_NAME.into()),
        arguments: r#"{"command":"printf '%s|%s' \"${CARGO_PKG_NAME-unset}\" \"${HOME:+home}\""}"#
            .into(),
        clock_ms: 2_000,
    });
    assert_eq!(
        outcome,
        ToolOutcome::Result {
            content: "unset|home".into()
        }
    );
}

/// conforms: gate-execution-cancel-brings-the-clock-forward
/// Removing the channel from supervision makes this reach the clock.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_cancel_brings_the_clock_forward() {
    let scratch = ScratchTools::new();
    use std::time::{Duration, Instant};
    let (gate, harness) = pair();
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    WAIT_ENTERED.with(|slot| *slot.borrow_mut() = Some(entered));
    let peer = std::thread::spawn(move || {
        waiting
            .recv_timeout(Duration::from_secs(2))
            .expect("supervision entered");
        std::thread::sleep(Duration::from_millis(50));
        harness
            .send(&OrganEnvelope {
                exchange: exchange(),
                position: Position::Continue,
                payload: Payload::ToolCancel,
            })
            .unwrap();
        // Keep the channel open until the result returns.
        harness
    });
    let started = Instant::now();
    let outcome = super::execute_in(
        &ToolExecution {
            name: ToolName(SHELL_NAME.into()),
            arguments: r#"{"command":"echo partial; sleep 5"}"#.into(),
            clock_ms: 2_000,
        },
        &gate,
        &exchange(),
        &scratch.tools,
    )
    .unwrap();
    let _harness = peer.join().unwrap();
    assert!(
        matches!(outcome, ToolOutcome::Killed { by: KillCause::Cancel, ref partial } if partial.as_deref().is_some_and(|text| text.contains("partial"))),
        "{outcome:?}"
    );
    assert!(
        started.elapsed() < Duration::from_millis(800),
        "cancel spent the clock: {:?}",
        started.elapsed()
    );
}

/// **The unload's interrupt ends an open execution by the cancel's path**,
/// per `weaver-harness-gate-contract` section 2 and `weaver-admin-Spec`
/// 3.0 S4 x tool return: answered `Killed { by: unload }`, its partial
/// riding, with no clock added, so the record reads the call as never
/// finished and re-runnable at the reload. Perturbation: answer the
/// interrupt as a misplaced message, and the clock kills it instead.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn an_interrupt_kills_an_open_execution_by_unload() {
    let scratch = ScratchTools::new();
    use std::time::{Duration, Instant};
    let (gate, harness) = pair();
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    WAIT_ENTERED.with(|slot| *slot.borrow_mut() = Some(entered));
    let peer = std::thread::spawn(move || {
        waiting
            .recv_timeout(Duration::from_secs(2))
            .expect("supervision entered");
        std::thread::sleep(Duration::from_millis(50));
        harness
            .send(&OrganEnvelope {
                exchange: exchange(),
                position: Position::Continue,
                payload: Payload::ToolInterrupt,
            })
            .unwrap();
        harness
    });
    let started = Instant::now();
    let outcome = super::execute_in(
        &ToolExecution {
            name: ToolName(SHELL_NAME.into()),
            arguments: r#"{"command":"echo partial; sleep 5"}"#.into(),
            clock_ms: 2_000,
        },
        &gate,
        &exchange(),
        &scratch.tools,
    )
    .unwrap();
    let _harness = peer.join().unwrap();
    assert!(
        matches!(outcome, ToolOutcome::Killed { by: KillCause::Unload, ref partial } if partial.as_deref().is_some_and(|text| text.contains("partial"))),
        "{outcome:?}"
    );
    assert!(
        started.elapsed() < Duration::from_millis(800),
        "the interrupt spent the clock: {:?}",
        started.elapsed()
    );
}

/// conforms: gate-execution-exit-heard-within-a-slice
/// The child cannot exit before the first wait. Replacing the slice
/// with the remaining clock therefore fails even on a fast machine.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn an_exit_is_heard_within_a_slice() {
    let scratch = ScratchTools::new();
    use std::time::{Duration, Instant};
    let path = std::env::temp_dir().join(format!("weaver-exit-release-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    // Removed when the test ends, pass or fail: the closing unwrap below
    // is the pass's own removal and asserts the peer wrote the file, and
    // this guard answers a failure that never reaches it (#690 item
    // C2.9).
    struct Release(std::path::PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _release_guard = Release(path.clone());
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    WAIT_ENTERED.with(|slot| *slot.borrow_mut() = Some(entered));
    let release = path.clone();
    let peer = std::thread::spawn(move || {
        waiting
            .recv_timeout(Duration::from_secs(2))
            .expect("supervisor reached poll with live child");
        std::fs::write(release, "exit").unwrap();
    });
    let (gate, _harness) = pair();
    let started = Instant::now();
    let arguments = serde_json::json!({"command": format!("while [ ! -f '{}' ]; do sleep 0.01; done; echo finished", path.display())}).to_string();
    let outcome = super::execute_in(
        &ToolExecution {
            name: ToolName(SHELL_NAME.into()),
            arguments,
            clock_ms: 2_000,
        },
        &gate,
        &exchange(),
        &scratch.tools,
    )
    .unwrap();
    peer.join().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        matches!(outcome, ToolOutcome::Result { ref content } if content.contains("finished")),
        "{outcome:?}"
    );
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "exit waited {:?}",
        started.elapsed()
    );
}

fn shell(arguments: &str, clock_ms: u64) -> ToolOutcome {
    execute(&ToolExecution {
        name: ToolName(SHELL_NAME.into()),
        arguments: arguments.into(),
        clock_ms,
    })
}

/// [`shell`] for a call that refuses before anything runs.
fn refused(arguments: &str, clock_ms: u64) -> ToolOutcome {
    refuse(&ToolExecution {
        name: ToolName(SHELL_NAME.into()),
        arguments: arguments.into(),
        clock_ms,
    })
}

/// **A name that is not the shell's refuses by name, in this crate's
/// own voice, and nothing runs.** The one verb has no nearest match.
#[test]
fn a_name_that_is_not_the_shells_refuses_by_name() {
    let outcome = refuse(&ToolExecution {
        name: ToolName("calculator".into()),
        arguments: r#"{"command":"true"}"#.into(),
        clock_ms: 1_000,
    });
    let ToolOutcome::Refused { reason } = outcome else {
        panic!("a refusal answers: {outcome:?}");
    };
    assert!(
        reason.contains("no tool named calculator"),
        "the refusal names the name: {reason}"
    );
}

/// **The clock is validated before anything runs, refused and never
/// clamped**: zero asks for nothing and past-maximum asks for more than
/// the shell declares, and both refuse naming the bound.
#[test]
fn a_clock_outside_the_maximum_refuses_rather_than_clamps() {
    for clock in [0, SHELL_MAX_CLOCK_MS + 1] {
        let outcome = refused(r#"{"command":"true"}"#, clock);
        let ToolOutcome::Refused { reason } = outcome else {
            panic!("a refusal answers for {clock}: {outcome:?}");
        };
        assert!(
            reason.contains("declared maximum"),
            "the refusal names the bound: {reason}"
        );
    }
}

/// Malformed arguments are the gate's refusal - nothing ran, so no
/// account is the tool's.
#[test]
fn malformed_arguments_refuse_in_the_gates_voice() {
    for (arguments, expected) in [
        ("not json", "the arguments are not one JSON object"),
        (
            r#"{"expr":"true"}"#,
            "the arguments carry no command string",
        ),
    ] {
        let outcome = refused(arguments, 1_000);
        let ToolOutcome::Refused { reason } = outcome else {
            panic!("a refusal answers: {outcome:?}");
        };
        assert!(reason.contains(expected), "for {arguments}: {reason}");
    }
}

/// **The shell runs where it promises and accounts for every ending**:
/// output crosses, stderr is named, and a nonzero exit is a result -
/// the shell's own answer - never an error.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn the_shell_runs_where_it_promises() {
    let outcome = shell(r#"{"command":"pwd"}"#, 10_000);
    let ToolOutcome::Result { content } = outcome else {
        panic!("pwd answers: {outcome:?}");
    };
    let home = std::env::var("HOME").expect("the suite has a HOME");
    let expected = std::fs::canonicalize(&home).expect("the home resolves");
    let reported = std::fs::canonicalize(content.trim()).expect("the pwd resolves");
    assert_eq!(reported, expected, "the command runs in the home directory");

    let outcome = shell(r#"{"command":"echo out; echo err 1>&2; exit 3"}"#, 10_000);
    let ToolOutcome::Result { content } = outcome else {
        panic!("a nonzero exit is still a result: {outcome:?}");
    };
    assert!(content.contains("out"), "stdout crosses: {content}");
    assert!(
        content.contains("stderr: err"),
        "stderr is named: {content}"
    );
    assert!(
        content.contains("exit status: 3"),
        "the exit is named: {content}"
    );
}

/// **A chatty command drains while it runs and a straggler does not
/// hold the answer.** The first writes past the kernel's pipe buffer
/// and must complete promptly with the bound's marker - a drain that
/// waited for the exit would block the child's writes and convert it
/// into a false kill. The second leaves a background child holding the
/// pipe's write end, and the group kill at the exit is what lets the
/// readers finish.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_chatty_command_and_a_straggler_both_answer_promptly() {
    let started = std::time::Instant::now();
    let outcome = shell(
        r#"{"command":"head -c 200000 /dev/zero | tr '\\0' 'x'"}"#,
        10_000,
    );
    let ToolOutcome::Result { content } = outcome else {
        panic!("a chatty command still answers: {outcome:?}");
    };
    assert!(
        content.contains("[output truncated at 32 KiB]"),
        "the bound marks itself: {} octets",
        content.len()
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the drain rode along, no clock was consumed"
    );

    let started = std::time::Instant::now();
    let outcome = shell(r#"{"command":"echo done; sleep 30 &"}"#, 10_000);
    let ToolOutcome::Result { content } = outcome else {
        panic!("the foreground command answers: {outcome:?}");
    };
    assert!(content.contains("done"), "the answer crossed: {content}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the straggler did not hold the answer open"
    );
}

/// Fail clearly if the fixture's util-linux command is unavailable.
fn require_setsid() {
    assert!(
        std::process::Command::new("setsid")
            .arg("true")
            .status()
            .expect("these tests require the util-linux setsid executable on PATH")
            .success(),
        "the setsid fixture could not start a new session"
    );
}

/// Wait for the detached child to announce readiness before the leader
/// exits. FD 3 then puts its stdout back on the executor's pipe, and its
/// stderr was inherited throughout. Neither pipe can reach EOF for 2 s.
fn with_detached_descendant(command: &str) -> String {
    format!(
        "exec 3>&1; read -r ready < <(setsid sh -c 'echo ready; exec 1>&3 3>&-; sleep 2'); exec 3>&-; {command}"
    )
}

/// The handoff's 50 ms clock checks latency alone. Output capture has a
/// separate 500 ms test, so process startup need not win a 50 ms echo race.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_detached_descendant_cannot_hold_the_answer_past_the_clock() {
    require_setsid();
    const CLOCK_MS: u64 = 50;
    const SCHEDULING_TOLERANCE: std::time::Duration = std::time::Duration::from_millis(200);
    let started = std::time::Instant::now();
    let outcome = shell(r#"{"command":"setsid sleep 2 & wait"}"#, CLOCK_MS);
    let elapsed = started.elapsed();
    assert!(
        matches!(outcome, ToolOutcome::Killed { .. }),
        "the waiting leader should time out, fixture requires setsid: {outcome:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(CLOCK_MS) + SCHEDULING_TOLERANCE,
        "the caller's {CLOCK_MS}ms clock governs, elapsed: {elapsed:?}"
    );
}

/// A completed leader keeps its account and does not spend the remaining
/// caller budget waiting for a detached descendant's inherited pipes.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_detached_descendant_cannot_hold_the_answer_after_the_leader_exits() {
    require_setsid();
    for code in [0, 7] {
        let command = with_detached_descendant(&format!("echo done; echo err >&2; exit {code}"));
        let started = std::time::Instant::now();
        let outcome = shell(&serde_json::json!({"command": command}).to_string(), 3_000);
        let elapsed = started.elapsed();
        let ToolOutcome::Result { content } = outcome else {
            panic!("a completed leader retains its account: {outcome:?}");
        };
        assert!(
            content.contains("done") && content.contains("stderr: err"),
            "{content}"
        );
        if code != 0 {
            assert!(content.contains("exit status: 7"), "{content}");
        }
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "cleanup must not spend the 3000 ms clock: {elapsed:?}"
        );
    }
}

/// A short clock does not turn an already observed exit into a kill.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_completed_nonzero_exit_keeps_its_account_with_a_short_clock() {
    require_setsid();
    let command = with_detached_descendant("echo done; exit 7");
    let outcome = shell(&serde_json::json!({"command": command}).to_string(), 500);
    let ToolOutcome::Result { content } = outcome else {
        panic!("the leader's exit must survive capture cleanup: {outcome:?}");
    };
    assert!(
        content.contains("done") && content.contains("exit status: 7"),
        "{content}"
    );
}

/// Capture truncation is visible on both timeout and normal-exit paths,
/// and an exit status is not part of the output that gets truncated.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_detached_descendant_does_not_hide_truncation_or_exit_status() {
    require_setsid();
    for redirect in ["", " >&2"] {
        let command = with_detached_descendant(&format!(
            "head -c 200000 /dev/zero | tr '\\0' x{redirect}; sleep 5"
        ));
        let outcome = shell(&serde_json::json!({"command": command}).to_string(), 500);
        let ToolOutcome::Killed {
            by: KillCause::Clock,
            partial: Some(partial),
        } = outcome
        else {
            panic!("expected captured output before the 500 ms timeout: {outcome:?}");
        };
        assert!(
            partial.contains("[output truncated at 32 KiB]"),
            "discarded output must be marked, got {} bytes",
            partial.len()
        );
    }
    let command = with_detached_descendant("head -c 200000 /dev/zero | tr '\\0' x; exit 7");
    let started = std::time::Instant::now();
    let outcome = shell(&serde_json::json!({"command": command}).to_string(), 3_000);
    let ToolOutcome::Result { content } = outcome else {
        panic!("a completed noisy command keeps its account: {outcome:?}");
    };
    assert!(
        content.contains("[output truncated at 32 KiB]"),
        "capture must mark truncation"
    );
    assert!(
        content.contains("exit status: 7"),
        "capture must retain the exit account"
    );
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
}

/// A held write end must not hide bytes already buffered at shutdown.
#[test]
fn a_stopped_reader_keeps_ready_bytes_without_waiting_for_eof() {
    use std::io::Write;
    let (read, write) = nix::unistd::pipe().expect("pipe");
    let mut writer = std::fs::File::from(write);
    writer.write_all(b"buffered before stop").expect("write");
    let finished = std::sync::atomic::AtomicBool::new(true);
    let capture =
        drain_bounded(std::fs::File::from(read), SHELL_OUTPUT_BOUND, &finished).expect("drain");
    assert_eq!(capture.bytes, b"buffered before stop");
    assert!(!capture.truncated);
    drop(writer);
}

/// The descriptor stays readable while reads emulate a continuous
/// writer, or an I/O failure. The read cap makes a broken test fail
/// instead of hanging if the final-drain budget is accidentally removed.
struct ReadProbe {
    ready: std::fs::File,
    reads: usize,
    fail: bool,
}

impl std::os::fd::AsFd for ReadProbe {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.ready.as_fd()
    }
}

impl std::io::Read for ReadProbe {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reads += 1;
        if self.fail {
            return Err(std::io::Error::other("injected read failure"));
        }
        if self.reads > 8 {
            return Err(std::io::Error::other("adopted continuous output"));
        }
        buffer.fill(b'x');
        Ok(buffer.len())
    }
}

/// Shutdown is bounded even if a detached writer never becomes quiet.
#[test]
fn a_stopped_reader_does_not_adopt_a_continuous_writer() {
    use std::io::Write;
    let (read, write) = nix::unistd::pipe().expect("pipe");
    let mut writer = std::fs::File::from(write);
    writer.write_all(b"ready").expect("write");
    let reader = ReadProbe {
        ready: read.into(),
        reads: 0,
        fail: false,
    };
    let capture = drain_bounded(
        reader,
        SHELL_OUTPUT_BOUND,
        &std::sync::atomic::AtomicBool::new(true),
    )
    .expect("the final drain must stop adopting output");
    assert_eq!(capture.bytes.len(), SHELL_OUTPUT_BOUND);
    assert!(capture.truncated);
}

/// A real readable descriptor paired with a failing Read must propagate
/// the I/O failure instead of returning an apparently complete capture.
#[test]
fn a_read_failure_is_not_a_complete_capture() {
    use std::io::Write;
    let (read, write) = nix::unistd::pipe().expect("pipe");
    let mut writer = std::fs::File::from(write);
    writer.write_all(b"ready").expect("write");
    let reader = ReadProbe {
        ready: read.into(),
        reads: 0,
        fail: true,
    };
    let capture = drain_bounded(
        reader,
        SHELL_OUTPUT_BOUND,
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(matches!(capture, Err(error) if error.to_string() == "injected read failure"));
}

/// Exercise the shared cleanup used after a supervision error with both
/// writers still open. The guard releases them after 2 s if stop is broken,
/// making the regression fail on latency instead of hanging the test.
#[test]
fn cleanup_of_held_pipes_does_not_wait_for_writers() {
    use std::sync::{Arc, atomic::AtomicBool, mpsc};
    let (out_read, out_write) = nix::unistd::pipe().expect("stdout pipe");
    let (err_read, err_write) = nix::unistd::pipe().expect("stderr pipe");
    let (release, released) = mpsc::channel::<()>();
    let writers = std::thread::spawn(move || {
        let _ = released.recv_timeout(std::time::Duration::from_secs(2));
        drop((out_write, err_write));
    });
    let finished = Arc::new(AtomicBool::new(false));
    let out_finished = finished.clone();
    let err_finished = finished.clone();
    let out = std::thread::spawn(move || {
        drain_bounded(
            std::fs::File::from(out_read),
            SHELL_OUTPUT_BOUND,
            &out_finished,
        )
    });
    let err = std::thread::spawn(move || {
        drain_bounded(
            std::fs::File::from(err_read),
            SHELL_OUTPUT_BOUND,
            &err_finished,
        )
    });
    let started = std::time::Instant::now();
    let capture = finish_draining(&finished, out, err);
    let elapsed = started.elapsed();
    let _ = release.send(());
    writers.join().expect("writer guard");
    assert!(capture.is_ok(), "normal shutdown must capture both pipes");
    assert!(
        elapsed < std::time::Duration::from_millis(250),
        "cleanup waited for a writer: {elapsed:?}"
    );
}

/// A capture failure reaches the machinery outcome only after both
/// readers have been joined, and shutdown is signaled even on failure.
#[test]
fn failed_capture_cleanup_stops_and_joins_both_readers() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let finished = AtomicBool::new(false);
    let other_finished = Arc::new(AtomicBool::new(false));
    let observed = other_finished.clone();
    let out = std::thread::spawn(|| Err(std::io::Error::other("injected pipe failure")));
    let err = std::thread::spawn(move || {
        observed.store(true, Ordering::Relaxed);
        Ok(Drained::default())
    });
    let result = finish_draining(&finished, out, err);
    assert!(
        matches!(result, Err(ShellEnd::Errored { detail }) if detail.contains("stdout") && detail.contains("injected pipe failure"))
    );
    assert!(finished.load(Ordering::Relaxed));
    assert!(other_finished.load(Ordering::Relaxed));
}

/// **A command past the caller's clock is killed as its own case,
/// carrying no tool voice and attaching what drained before the
/// kill.** The clock here is tiny because the constant is what a
/// deployment tunes and the mechanism is what this watches.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn a_command_past_the_clock_is_killed_with_its_partial_attached() {
    let outcome = shell(r#"{"command":"echo early; sleep 5"}"#, 200);
    let ToolOutcome::Killed {
        partial,
        by: KillCause::Clock,
    } = outcome
    else {
        panic!("the kill is its own case: {outcome:?}");
    };
    let partial = partial.expect("the drained output rides the kill");
    assert!(
        partial.contains("early"),
        "what drained before the kill is attached: {partial}"
    );

    let outcome = shell(r#"{"command":"sleep 5"}"#, 200);
    let ToolOutcome::Killed {
        partial,
        by: KillCause::Clock,
    } = outcome
    else {
        panic!("the kill is its own case: {outcome:?}");
    };
    assert!(
        partial.is_none(),
        "a silent command attaches nothing: {partial:?}"
    );
}

// ------------------------------------------------------------- the agent's cgroup

/// The execution's answer, run in `scratch`'s `tools/`.
fn execute_in_scratch(
    scratch: &ScratchTools,
    arguments: &str,
    clock_ms: u64,
) -> Result<ToolOutcome, Unanswered> {
    let (near, _far) = pair();
    super::execute_in(
        &ToolExecution {
            name: ToolName(SHELL_NAME.into()),
            arguments: arguments.into(),
            clock_ms,
        },
        &near,
        &exchange(),
        &scratch.tools,
    )
}

/// **C1 at the gate: a command that left its session is ended at the clock**
/// (`weaver-admin-Spec` 6.1, C1's instrument): a shell that starts a new
/// session and double-forks a sleeper past the clock is killed whole, and
/// `tools/` reads `populated 0` when the answer comes. Perturbation: end the
/// execution by a process-group signal in place of the kill of `tools/`,
/// and the detached sleeper survives the answer.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn the_clock_ends_a_command_that_left_its_session() {
    require_setsid();
    let scratch = ScratchTools::new();
    let outcome = execute_in_scratch(
        &scratch,
        r#"{"command":"setsid sh -c '(sleep 30 &) ; sleep 30' & sleep 30"}"#,
        500,
    )
    .expect("answered");
    assert!(
        matches!(
            outcome,
            ToolOutcome::Killed {
                by: KillCause::Clock,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(
        scratch.tools.populated(),
        Some(false),
        "nothing of the execution outlives its answer"
    );
}

/// **The shell runs inside `tools/`** (`weaver-gate-Spec` section 8; 6.1,
/// C4): the shell's own `/proc/self/cgroup` names the `tools/` it was
/// handed. Perturbation: drop the move from `pre_exec`, and the shell names
/// the suite's own cgroup.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn the_shell_runs_inside_tools() {
    let scratch = ScratchTools::new();
    let outcome = execute_in_scratch(&scratch, r#"{"command":"cat /proc/self/cgroup"}"#, 2_000)
        .expect("answered");
    let ToolOutcome::Result { content } = outcome else {
        panic!("a result: {outcome:?}");
    };
    let name = scratch
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        content.trim().ends_with(&format!("/{name}")),
        "the shell's cgroup is the scratch tools/ {name}: {content}"
    );
}

/// **A move that fails is the machinery's: `Errored`, and nothing ran**
/// (`weaver-gate-Spec` section 8; 6.1, the gate's spawn failing): a handle
/// on a plain directory has no `cgroup.procs`, so the child ends before its
/// exec and a command that would leave a marker leaves none. Perturbation:
/// drop the move, and the command runs and the marker stands.
#[test]
fn a_move_that_fails_answers_errored_and_runs_nothing() {
    let marker = std::env::temp_dir().join(format!("weaver-gate-move-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let outcome = refuse(&ToolExecution {
        name: ToolName(SHELL_NAME.into()),
        arguments: serde_json::json!({"command": format!("touch {}", marker.display())})
            .to_string(),
        clock_ms: 2_000,
    });
    let ran = marker.exists();
    let _ = std::fs::remove_file(&marker);
    assert!(
        matches!(outcome, ToolOutcome::Errored { .. }),
        "{outcome:?}"
    );
    assert!(!ran, "nothing ran");
}

/// **An execution the gate cannot contain is never answered**
/// (`weaver-gate-Spec` section 8; 6.1, the gate's write of `cgroup.kill`
/// failing): `cgroup.kill` closed to the gate after the spawn's move, the
/// execution ends unanswered, `Uncontained`, which the gate's serve loop
/// turns into its exit (read, not run here: the binary's exit is the
/// harness's to meet). Perturbation: answer whatever the kill came to, and
/// the execution is answered.
#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn an_execution_the_gate_cannot_kill_is_unanswered() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = ScratchTools::new();
    let kill = scratch.path.join("cgroup.kill");
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o444)).unwrap();
    let answered = execute_in_scratch(&scratch, r#"{"command":"echo done"}"#, 2_000);
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        matches!(answered, Err(Unanswered::Uncontained)),
        "{answered:?}"
    );
}
