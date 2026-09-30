//! conforms: gate-execution-cancel-brings-the-clock-forward
//! conforms: gate-execution-exit-heard-within-a-slice
//! conforms: gate-shell-the-one-held-tool
//! conforms: gate-execution-one-clock
//! conforms: gate-execution-four-contents
//! conforms: gate-execution-group-kill
//! conforms: gate-execution-drain-rides-the-run
//!
//! The shell execution, per `weaver-gate-Spec` section 8 and the tool
//! boundary ruling of 2026-08-18: this crate holds one tool, the shell, and
//! holds it as its own outbound verb rather than as a table's member. The
//! agent's effect on the world crosses this membrane, the shell is that
//! crossing's general form, and the uid it runs under is the agent's outer
//! protective shell. A name that is not the shell's refuses by name, never
//! a nearest match, and there is no dyn table to search: one verb,
//! dispatched directly.
//!
//! **One clock, the caller's.** Every invocation carries the caller's
//! timeout, validated here against the declared maximum and adopted as the
//! kill clock, refused past the maximum rather than clamped. The answer is
//! one of the contract's four contents, told apart by who speaks: a result
//! is the shell's own words - a nonzero exit included - a refusal is this
//! crate's voice with nothing run, an error is the machinery's, and a kill
//! carries no tool voice by construction.

use crate::channel::{Channel, ChannelFault};
use weaver_types::{
    ExchangeId, KillCause, LifecycleRefusal, OrganEnvelope, Payload, Position, ToolExecution,
    ToolOutcome,
};

/// The one held tool's name. The model calls the shell by the name the
/// shell answers to everywhere else.
pub const SHELL_NAME: &str = "bash";

/// The maximum clock the shell declares, in milliseconds. A caller may ask
/// for less and never more, per the one-clock rule.
pub const SHELL_MAX_CLOCK_MS: u64 = 60_000;

/// The capture bound per pipe. Output past it is drained and discarded so
/// the pipe empties, and the truncation marks itself in the answer.
const SHELL_OUTPUT_BOUND: usize = 32 * 1024;

/// Execute one call, answering one of the exchange's four contents.
pub fn execute(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
) -> Result<ToolOutcome, ChannelFault> {
    execute_inner(execution, channel, exchange).or_else(|end| match end {
        ShellEnd::Channel(fault) => Err(fault),
        other => Ok(other.into_outcome()),
    })
}

fn execute_inner(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
) -> Result<ToolOutcome, ShellEnd> {
    if execution.name.0 != SHELL_NAME {
        return Ok(ToolOutcome::Refused {
            reason: format!(
                "no tool named {} is held - the shell, {SHELL_NAME}, is the one tool",
                execution.name.0
            ),
        });
    }
    if execution.clock_ms == 0 || execution.clock_ms > SHELL_MAX_CLOCK_MS {
        return Ok(ToolOutcome::Refused {
            reason: format!(
                "the caller's clock of {}ms is outside the shell's declared maximum of \
                 {SHELL_MAX_CLOCK_MS}ms",
                execution.clock_ms
            ),
        });
    }
    let parsed: serde_json::Value = match serde_json::from_str(&execution.arguments) {
        Ok(value) => value,
        Err(_) => {
            return Ok(ToolOutcome::Refused {
                reason: "the arguments are not one JSON object".to_string(),
            });
        }
    };
    let Some(command) = parsed.get("command").and_then(|value| value.as_str()) else {
        return Ok(ToolOutcome::Refused {
            reason: "the arguments carry no command string".to_string(),
        });
    };
    // The home comes from the account database rather than from the
    // environment: the organ fan-out execs with an empty environment on
    // purpose, and the account database is where the uid's home is a fact
    // rather than an inheritance.
    let Some(home) = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|user| user.dir)
    else {
        return Ok(ToolOutcome::Errored {
            detail: "this uid has no home in the account database".to_string(),
        });
    };
    match run_in_home(
        command,
        &home.to_string_lossy(),
        std::time::Duration::from_millis(execution.clock_ms),
        channel,
        exchange,
    ) {
        Ok(content) => Ok(ToolOutcome::Result { content }),
        Err(end) => Err(end),
    }
}

/// How the shell's run ended when it did not answer: the machinery failed,
/// or the clock/cancel killed the group, or the channel failed. The split is the
/// contract's - the speaker differs - and the conversion to the wire's
/// contents happens here so `run_in_home` stays a plain function.
enum ShellEnd {
    Errored {
        detail: String,
    },
    Killed {
        partial: Option<String>,
        by: KillCause,
    },
    Channel(ChannelFault),
}

impl ShellEnd {
    fn into_outcome(self) -> ToolOutcome {
        match self {
            ShellEnd::Errored { detail } => ToolOutcome::Errored { detail },
            ShellEnd::Killed { partial, by } => ToolOutcome::Killed { partial, by },
            ShellEnd::Channel(_) => unreachable!("channel faults stay below the exchange"),
        }
    }
}

/// Fork, supervise to the caller's clock, and account for the exit - every
/// path one of the contract's contents, never a hung exchange.
fn run_in_home(
    command: &str,
    home: &str,
    deadline: std::time::Duration,
    channel: &Channel,
    exchange: &ExchangeId,
) -> Result<String, ShellEnd> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    // The child leads its own process group, so the kill below reaches
    // descendants still in `bash -c`'s group: a background child would otherwise
    // inherit the pipe's write end and hold the readers open after the
    // shell itself exited.
    let mut child = Command::new("bash")
        .arg("-c")
        .arg(command)
        .current_dir(home)
        // The gate's own environment is deliberately empty; the command
        // still deserves to know where it lives.
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| ShellEnd::Errored {
            detail: format!("the fork failed: {error}"),
        })?;
    let group = nix::unistd::Pid::from_raw(child.id() as i32);

    // Each pipe drains concurrently so a chatty command cannot block on
    // a full pipe. After group termination, readers stop waiting for writers
    // and take a bounded final drain of bytes already available.
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let out_finished = finished.clone();
    let err_finished = finished.clone();
    let out_reader =
        std::thread::spawn(move || drain_bounded(stdout, SHELL_OUTPUT_BOUND, &out_finished));
    let err_reader =
        std::thread::spawn(move || drain_bounded(stderr, SHELL_OUTPUT_BOUND, &err_finished));

    // Preserve main's clock origin: supervision starts after the shell and
    // readers are spawned. WNOWAIT holds the leader's pid until the group is
    // signaled, so the kill cannot target a reissued process-group id.
    let until = std::time::Instant::now() + deadline;
    let ending = loop {
        use nix::sys::wait::{Id, WaitPidFlag, WaitStatus, waitid};
        match waitid(
            Id::Pid(group),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        ) {
            Ok(WaitStatus::StillAlive) => {
                let remaining = until.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break Ok(Some(KillCause::Clock));
                }
                let slice = remaining.min(std::time::Duration::from_millis(25));
                match supervision_wait(channel, exchange, slice) {
                    Ok(true) => break Ok(Some(KillCause::Cancel)),
                    Ok(false) => {}
                    Err(fault) => break Err(ShellEnd::Channel(fault)),
                }
            }
            Ok(_) => break Ok(None),
            Err(error) => {
                break Err(ShellEnd::Errored {
                    detail: format!("the supervision failed: {error}"),
                });
            }
        }
    };
    // All endings kill the same group before reaping and release both
    // readers, including a failed waitid or wait. Cleanup has no tool outcome
    // election: only the supervisor decides whether the clock killed the run.
    let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
    // Also signal the unreaped leader in case it left the original group.
    let _ = nix::sys::signal::kill(group, nix::sys::signal::Signal::SIGKILL);
    let reaped = child.wait();
    let drained = finish_draining(&finished, out_reader, err_reader);
    let killed_by = ending?;
    let status = reaped.map_err(|error| ShellEnd::Errored {
        detail: format!("the supervision failed: {error}"),
    })?;
    let (out, err) = drained?;
    let mut output = joined(&out, &err);
    if let Some(by) = killed_by {
        return Err(ShellEnd::Killed {
            partial: (!output.is_empty()).then_some(output),
            by,
        });
    }
    // The exit account is metadata, appended after capture truncation so
    // even a noisy command cannot lose its nonzero status.
    if !status.success() {
        let code = status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "killed by signal".to_string());
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!("exit status: {code}"));
    }
    if output.is_empty() {
        output.push_str("(no output)");
    }
    Ok(output)
}

/// Wait for a cancel without postponing the next unreaped exit check.
fn supervision_wait(
    channel: &Channel,
    exchange: &ExchangeId,
    slice: std::time::Duration,
) -> Result<bool, ChannelFault> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    let mut fds = [PollFd::new(channel.as_fd(), PollFlags::POLLIN)];
    let timeout = PollTimeout::try_from(slice).expect("the supervision slice fits poll");
    #[cfg(test)]
    tests::wait_entered();
    match poll(&mut fds, timeout) {
        Ok(0) | Err(nix::errno::Errno::EINTR) => return Ok(false),
        Err(_) => return Err(ChannelFault::Closed),
        Ok(_) => {}
    }
    let envelope = channel.recv()?;
    if envelope.exchange == *exchange
        && envelope.position == Position::Continue
        && matches!(envelope.payload, Payload::ToolCancel)
    {
        return Ok(true);
    }
    // A misplaced message is refused as at rest. Continue supervising the
    // live group regardless, and never let a bad sender bypass cleanup.
    channel.send(&OrganEnvelope {
        exchange: envelope.exchange,
        position: Position::Close,
        payload: Payload::Refusal(LifecycleRefusal::OutOfOrder),
    })?;
    Ok(false)
}

/// Captured bytes from one pipe. I/O failure travels as an error, never as
/// an apparently complete capture.
#[derive(Default)]
struct Drained {
    /// At most the requested capture bound, while excess bytes are discarded.
    bytes: Vec<u8>,
    /// At least one byte was discarded because the capture bound was reached.
    truncated: bool,
}

/// A reader owns its pipe until it returns its capture or an I/O error.
type DrainReader = std::thread::JoinHandle<std::io::Result<Drained>>;

/// Release both readers after every supervision outcome and join both even
/// when one failed. A panic or a pipe failure is the machinery's account.
fn finish_draining(
    finished: &std::sync::atomic::AtomicBool,
    out: DrainReader,
    err: DrainReader,
) -> Result<(Drained, Drained), ShellEnd> {
    finished.store(true, std::sync::atomic::Ordering::Relaxed);
    let out = out.join();
    let err = err.join();
    let capture = |name, result: std::thread::Result<std::io::Result<Drained>>| {
        result
            .map_err(|_| ShellEnd::Errored {
                detail: format!("the {name} reader panicked"),
            })?
            .map_err(|error| ShellEnd::Errored {
                detail: format!("the {name} capture failed: {error}"),
            })
    };
    Ok((capture("stdout", out)?, capture("stderr", err)?))
}

/// Assemble stdout and stderr once for both results and killed partials.
/// The marker describes discarded capture bytes, independently of the exit.
fn joined(out: &Drained, err: &Drained) -> String {
    let mut output = String::from_utf8_lossy(&out.bytes).into_owned();
    if !err.bytes.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("stderr: ");
        output.push_str(&String::from_utf8_lossy(&err.bytes));
    }
    if out.truncated || err.truncated || output.len() > SHELL_OUTPUT_BOUND {
        let mut cut = output.len().min(SHELL_OUTPUT_BOUND);
        while !output.is_char_boundary(cut) {
            cut -= 1;
        }
        output.truncate(cut);
        output.push_str("\n[output truncated at 32 KiB]");
    }
    output
}

/// Drain concurrently until EOF or supervision finishes. Poll in 10 ms
/// steps so a writer holding a silent pipe cannot delay cleanup for the
/// caller's remaining budget. After the stop, poll without waiting and take
/// at most one capture's worth of reads plus one overflow read. This keeps
/// buffered partial output without adopting a continuously writing descendant.
fn drain_bounded(
    mut pipe: impl std::io::Read + std::os::fd::AsFd,
    bound: usize,
    finished: &std::sync::atomic::AtomicBool,
) -> std::io::Result<Drained> {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::sync::atomic::Ordering;

    const DRAIN_POLL_MS: u16 = 10;
    let mut drained = Drained::default();
    let mut buffer = [0u8; 8192];
    let mut final_reads = None;
    loop {
        if finished.load(Ordering::Relaxed) && final_reads.is_none() {
            final_reads = Some(bound.div_ceil(buffer.len()) + 1);
        }
        let timeout = if let Some(left) = &mut final_reads {
            if *left == 0 {
                break;
            }
            *left -= 1;
            0
        } else {
            DRAIN_POLL_MS
        };
        let mut waiting = [PollFd::new(pipe.as_fd(), PollFlags::POLLIN)];
        match poll(&mut waiting, timeout) {
            Ok(0) if final_reads.is_some() => break,
            Ok(0) | Err(nix::errno::Errno::EINTR) => continue,
            Err(error) => return Err(std::io::Error::from_raw_os_error(error as i32)),
            Ok(_) => {}
        }
        // This is the pipe's sole reader: readiness means a read can take
        // available bytes or EOF without waiting for another write.
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let take = (bound - drained.bytes.len()).min(read);
                drained.bytes.extend_from_slice(&buffer[..take]);
                drained.truncated |= take < read;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(drained)
}

#[cfg(test)]
mod tests {
    use super::*;
    use weaver_types::ToolName;

    fn execute(execution: &ToolExecution) -> ToolOutcome {
        let (near, _far) = pair();
        super::execute(execution, &near, &exchange()).expect("execution channel")
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

    /// conforms: gate-execution-cancel-brings-the-clock-forward
    /// Removing the channel from supervision makes this reach the clock.
    #[test]
    fn a_cancel_brings_the_clock_forward() {
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
        let outcome = super::execute(
            &ToolExecution {
                name: ToolName(SHELL_NAME.into()),
                arguments: r#"{"command":"echo partial; sleep 5"}"#.into(),
                clock_ms: 2_000,
            },
            &gate,
            &exchange(),
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

    /// conforms: gate-execution-exit-heard-within-a-slice
    /// The child cannot exit before the first wait. Replacing the slice
    /// with the remaining clock therefore fails even on a fast machine.
    #[test]
    fn an_exit_is_heard_within_a_slice() {
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
        let outcome = super::execute(
            &ToolExecution {
                name: ToolName(SHELL_NAME.into()),
                arguments,
                clock_ms: 2_000,
            },
            &gate,
            &exchange(),
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

    /// **A name that is not the shell's refuses by name, in this crate's
    /// own voice, and nothing runs.** The one verb has no nearest match.
    #[test]
    fn a_name_that_is_not_the_shells_refuses_by_name() {
        let outcome = execute(&ToolExecution {
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
            let outcome = shell(r#"{"command":"true"}"#, clock);
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
            let outcome = shell(arguments, 1_000);
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
    fn a_detached_descendant_cannot_hold_the_answer_after_the_leader_exits() {
        require_setsid();
        for code in [0, 7] {
            let command =
                with_detached_descendant(&format!("echo done; echo err >&2; exit {code}"));
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
}
