//! conforms: gate-execution-cancel-brings-the-clock-forward
//! conforms: gate-execution-exit-heard-within-a-slice
//! conforms: gate-shell-the-one-held-tool
//! conforms: gate-execution-one-clock
//! conforms: gate-execution-four-contents
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

pub mod cgroup;

use crate::channel::{Channel, ChannelFault};
use cgroup::Tools;
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

/// Why an execution has no answer: the channel failed, or the gate could not
/// contain it (`weaver-gate-Spec` section 8), and then it exits with the
/// execution unanswered so the harness meets its death.
#[derive(Debug)]
pub enum Unanswered {
    Channel(ChannelFault),
    /// The write of `tools/cgroup.kill` failed, or `tools/` did not empty
    /// within the containment bound.
    Uncontained,
}

/// Execute one call in the `tools/` entry adopted, answering one of the
/// exchange's four contents.
pub fn execute(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
) -> Result<ToolOutcome, Unanswered> {
    match cgroup::adopted() {
        Some(tools) => execute_in(execution, channel, exchange, tools),
        // Entry refuses a gate with no handle, so no execution reaches here
        // without one; answered as the machinery's all the same.
        None => Ok(ToolOutcome::Errored {
            detail: "no tools/ cgroup was handed to the gate".to_string(),
        }),
    }
}

/// [`execute`] in a named `tools/`: the one adopted, or the suite's
/// scratch one. Not public: no caller outside this crate may run a shell in
/// any `tools/` but the one entry adopted at descriptor 4.
pub(crate) fn execute_in(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
    tools: &Tools,
) -> Result<ToolOutcome, Unanswered> {
    execute_inner(execution, channel, exchange, tools).or_else(|end| match end {
        ShellEnd::Channel(fault) => Err(Unanswered::Channel(fault)),
        ShellEnd::Uncontained => Err(Unanswered::Uncontained),
        other => Ok(other.into_outcome()),
    })
}

fn execute_inner(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
    tools: &Tools,
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
    // environment: the gate inherits the agent's environment, which may or
    // may not carry `HOME`, and the account database is where the uid's home
    // is a fact rather than an inheritance.
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
        tools,
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
    /// The kill of `tools/` failed or did not empty it within the bound.
    Uncontained,
}

impl ShellEnd {
    fn into_outcome(self) -> ToolOutcome {
        match self {
            ShellEnd::Errored { detail } => ToolOutcome::Errored { detail },
            ShellEnd::Killed { partial, by } => ToolOutcome::Killed { partial, by },
            ShellEnd::Channel(_) | ShellEnd::Uncontained => {
                unreachable!("an unanswered end stays below the exchange")
            }
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
    tools: &Tools,
) -> Result<String, ShellEnd> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    // **The child moves itself into `tools/` before its exec**
    // (`weaver-gate-Spec` section 8; `weaver-admin-Spec` 6.1, C4): the
    // handle's number and the gate's pid are read here, before the fork, so
    // the child's `pre_exec` allocates nothing. The spawn returns only once
    // the child has exec'd, so every kill below begins after the move, and a
    // move that fails fails the spawn: `Errored`, nothing run.
    let handle = tools.raw();
    let gate = nix::unistd::getpid();
    let mut shell = Command::new("bash");
    // SAFETY: `move_self` makes only async-signal-safe `nix` calls on
    // numbers and a path read before the fork.
    unsafe {
        shell.pre_exec(move || cgroup::move_self(handle, gate));
    }
    // The child still leads its own process group, as before the cgroup's
    // kill: the group no longer carries the kill, and keeping it changes
    // nothing else.
    let mut child = shell
        .arg("-c")
        .arg(command)
        .current_dir(home)
        // **The command starts with `HOME` and nothing else**, as it did when
        // the gate itself ran with an empty environment. The gate now carries
        // the agent's environment, per `weaver-harness-Spec` section 2.2, and
        // the operator's ruling of 2026-10-01 reaches organs, not the commands
        // a tool runs, so the clear keeps that surface where it stood.
        .env_clear()
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| ShellEnd::Errored {
            detail: format!("the fork, or the shell's move into tools/, failed: {error}"),
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
                    Ok(Some(by)) => break Ok(Some(by)),
                    Ok(None) => {}
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
    // **Every end kills `tools/` whole and waits for it to empty before the
    // answer** (`weaver-gate-Spec` section 8; `weaver-admin-Spec` 6.1): the
    // shell's exit, the clock, a cancel, an interrupt and the channel's
    // closure alike, a failed waitid included. The kill reaches a command
    // that left its group or its session, which a group signal did not.
    // Cleanup has no tool outcome election: only the supervisor decides
    // whether the clock killed the run.
    // **One containment bound per end** (Codex on #167): the kill's wait and
    // the leader's reap share a deadline taken here, so an execution's
    // exchange completes within the caller's number plus the bound, as 6.1's
    // table states, never plus twice it.
    let deadline = std::time::Instant::now() + cgroup::CONTAINMENT_BOUND;
    let contained = tools.contain(cgroup::CONTAINMENT_BOUND);
    // **The leader is signalled by its pid, then reaped within the bound**
    // (Codex on #167): the shell runs as the agent's account and may have
    // moved itself out of `tools/` (6.1, C1's first stated exception), where
    // the kill of `tools/` does not reach it and an unbounded reap would
    // hang the gate. It is this process's own unreaped child, so its pid
    // cannot have been reused. A leader not reaped within the bound is an
    // execution not contained.
    let reaped = if contained {
        let _ = nix::sys::signal::kill(group, nix::sys::signal::Signal::SIGKILL);
        reap_within(&mut child, deadline)
    } else {
        None
    };
    let reaped = if let Some(reaped) = reaped {
        reaped
    } else {
        // **An execution the gate cannot contain is never answered**: the
        // leader is signalled, the readers are released, and the gate exits
        // so the harness meets its death. The leader is not reaped: a
        // process that would not die is likely what kept `tools/` populated,
        // and a wait on it could hold the gate past its own bound. The gate's
        // exit reparents it.
        let _ = nix::sys::signal::kill(group, nix::sys::signal::Signal::SIGKILL);
        let _ = finish_draining(&finished, out_reader, err_reader);
        drop(child);
        return Err(ShellEnd::Uncontained);
    };
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

/// The leader's reap by `deadline`, by a non-blocking wait in slices:
/// `None` where it did not exit in time (an uninterruptible sleep), which the
/// caller takes as an execution not contained. One check is made even past
/// the deadline, so a leader already exited is reaped.
fn reap_within(
    child: &mut std::process::Child,
    deadline: std::time::Instant,
) -> Option<std::io::Result<std::process::ExitStatus>> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(Ok(status)),
            Ok(None) => {}
            Err(error) => return Some(Err(error)),
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Wait for a cancel or the unload's interrupt without postponing the next
/// unreaped exit check, answering which one ended the execution: the cancel,
/// or the interrupt, which ends it by the same path and names the unload
/// (`weaver-harness-gate-contract` section 2).
fn supervision_wait(
    channel: &Channel,
    exchange: &ExchangeId,
    slice: std::time::Duration,
) -> Result<Option<KillCause>, ChannelFault> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    let mut fds = [PollFd::new(channel.as_fd(), PollFlags::POLLIN)];
    let timeout = PollTimeout::try_from(slice).expect("the supervision slice fits poll");
    // Test-only hook kept here (#118): tells the supervision tests the poll wait is entered.
    #[cfg(test)]
    tests::wait_entered();
    match poll(&mut fds, timeout) {
        Ok(0) | Err(nix::errno::Errno::EINTR) => return Ok(None),
        Err(_) => return Err(ChannelFault::Closed),
        Ok(_) => {}
    }
    let envelope = channel.recv()?;
    if envelope.exchange == *exchange && envelope.position == Position::Continue {
        match envelope.payload {
            Payload::ToolCancel => return Ok(Some(KillCause::Cancel)),
            Payload::ToolInterrupt => return Ok(Some(KillCause::Unload)),
            _ => {}
        }
    }
    // A misplaced message is refused as at rest. Continue supervising the
    // live group regardless, and never let a bad sender bypass cleanup.
    channel.send(&OrganEnvelope {
        exchange: envelope.exchange,
        position: Position::Close,
        payload: Payload::Refusal(LifecycleRefusal::OutOfOrder),
    })?;
    Ok(None)
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
mod tests;
