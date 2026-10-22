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
pub mod confine;

use crate::channel::{Channel, ChannelFault};
use cgroup::Tools;
use confine::Shell;
use std::collections::BTreeSet;
use weaver_types::{
    ExchangeId, KillCause, LifecycleRefusal, OrganEnvelope, Payload, Position, ToolExecution,
    ToolOutcome,
};

/// The one held tool's name, `sh` (the operator's ruling of 2026-10-16 on
/// #29), read from the floor: the program the call reaches is busybox's POSIX
/// `sh`, never bash, so the name says what the model gets.
pub const SHELL_NAME: &str = weaver_types::SHELL_TOOL;

/// The shell the raise granted, built once there and read by every
/// execution. Unset, this gate holds no tool (`weaver-gate-Spec` section 3).
static GRANTED: std::sync::OnceLock<Shell> = std::sync::OnceLock::new();

/// **Holds the granted shell for the gate's life**, from the raise that built
/// it. A second grant cannot arrive: the raise is answered once per process
/// and a second refuses out of order.
pub fn grant(shell: Shell) {
    let _ = GRANTED.set(shell);
}

/// The maximum clock the shell declares, in milliseconds. A caller may ask
/// for less and never more, per the one-clock rule.
pub use weaver_types::state_flow::SHELL_MAX_CLOCK_MS;

/// The capture bound per pipe. Output past it is drained and discarded so
/// the pipe empties, and the truncation marks itself in the answer.
const SHELL_OUTPUT_BOUND: usize = 32 * 1024;

/// Why an execution has no answer: the channel failed, bounded delivery failed,
/// or the gate could not contain it (`weaver-gate-Spec` section 8). The gate exits
/// with the execution unanswered so the harness meets its death.
#[derive(Debug)]
pub enum Unanswered {
    Channel(ChannelFault),
    /// The existing containment deadline expired before this answer was sent.
    /// This is non-delivery, not evidence that the peer closed its channel.
    DeliveryBound {
        exchange: ExchangeId,
        answer: UndeliveredAnswer,
    },
    /// The write of `tools/cgroup.kill` failed, or `tools/` did not empty
    /// within the containment bound.
    Uncontained,
}

/// Which retained answer could not cross the channel within the original bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndeliveredAnswer {
    ToolAnswer,
    OutOfOrder,
}

/// An execution has finished containment, but its answer has not crossed the
/// channel yet. Only send_answer consumes this owner, so a pending unload may
/// still become flow_failure before the actual send. The existing containment
/// deadline travels with it and is never renewed by channel pressure.
#[derive(Debug)]
pub struct ExecutionAnswer {
    outcome: ToolOutcome,
    containment_deadline: Option<std::time::Instant>,
}

/// Execute one call in the `tools/` entry adopted, answering one of the
/// exchange's four contents.
pub fn execute(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
    answered_tools: &BTreeSet<u64>,
) -> Result<ExecutionAnswer, Unanswered> {
    // **An absent grant is no shell** (`weaver-gate-Spec` section 3, #29):
    // every execution refuses by name with nothing run.
    let Some(shell) = GRANTED.get() else {
        return Ok(ExecutionAnswer {
            outcome: ToolOutcome::Refused {
                reason: format!(
                    "no tool is granted to this agent - its declaration elects none, so {} is not held",
                    execution.name.0
                ),
            },
            containment_deadline: None,
        });
    };
    match cgroup::adopted() {
        Some(tools) => execute_answer_in(
            execution,
            channel,
            exchange,
            tools,
            shell,
            Some(answered_tools),
        ),
        // Entry refuses a gate with no handle, so no execution reaches here
        // without one; answered as the machinery's all the same.
        None => Ok(ExecutionAnswer {
            outcome: ToolOutcome::Errored {
                detail: "no tools/ cgroup was handed to the gate".to_string(),
            },
            containment_deadline: None,
        }),
    }
}

/// [`execute`] in a named `tools/`: the one adopted, or the suite's
/// scratch one. Not public: no caller outside this crate may run a shell in
/// any `tools/` but the one entry adopted at descriptor 4.
#[cfg(test)]
pub(crate) fn execute_in(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
    tools: &Tools,
    shell: &Shell,
) -> Result<ToolOutcome, Unanswered> {
    execute_answer_in(execution, channel, exchange, tools, shell, None).map(|answer| answer.outcome)
}

fn execute_answer_in(
    execution: &ToolExecution,
    channel: &Channel,
    exchange: &ExchangeId,
    tools: &Tools,
    shell: &Shell,
    answered_tools: Option<&BTreeSet<u64>>,
) -> Result<ExecutionAnswer, Unanswered> {
    let mut containment_deadline = None;
    let outcome = execute_inner(
        execution,
        Supervision::with_answered(channel, exchange, answered_tools),
        tools,
        shell,
        &mut containment_deadline,
    )
    .or_else(|end| match end {
        ShellEnd::Channel(fault) => Err(Unanswered::Channel(fault)),
        ShellEnd::DeliveryBound { exchange, answer } => {
            Err(Unanswered::DeliveryBound { exchange, answer })
        }
        ShellEnd::Uncontained => Err(Unanswered::Uncontained),
        other => Ok(other.into_outcome()),
    })?;
    Ok(ExecutionAnswer {
        outcome,
        containment_deadline,
    })
}

fn execute_inner(
    execution: &ToolExecution,
    supervision: Supervision<'_>,
    tools: &Tools,
    shell: &Shell,
    containment_deadline: &mut Option<std::time::Instant>,
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
    match run_in_home(
        command,
        shell,
        std::time::Duration::from_millis(execution.clock_ms),
        supervision,
        tools,
        containment_deadline,
    ) {
        Ok(content) => Ok(ToolOutcome::Result { content }),
        Err(end) => Err(end),
    }
}

/// How the shell's run ended when it did not answer: the machinery failed,
/// or the clock/cancel killed the group, or the channel failed. The split is the
/// contract's - the speaker differs - and the conversion to the wire's
/// contents happens here so `run_in_home` stays a plain function.
#[derive(Debug)]
enum ShellEnd {
    Errored {
        detail: String,
    },
    Killed {
        partial: Option<String>,
        by: KillCause,
    },
    Channel(ChannelFault),
    DeliveryBound {
        exchange: ExchangeId,
        answer: UndeliveredAnswer,
    },
    /// The kill of `tools/` failed or did not empty it within the bound.
    Uncontained,
}

impl ShellEnd {
    fn into_outcome(self) -> ToolOutcome {
        match self {
            ShellEnd::Errored { detail } => ToolOutcome::Errored { detail },
            ShellEnd::Killed { partial, by } => ToolOutcome::Killed { partial, by },
            ShellEnd::Channel(_) | ShellEnd::DeliveryBound { .. } | ShellEnd::Uncontained => {
                unreachable!("an unanswered end stays below the exchange")
            }
        }
    }
}

/// Fork, supervise to the caller's clock, and account for the exit - every
/// path one of the contract's contents, never a hung exchange.
fn run_in_home(
    command: &str,
    shell_program: &Shell,
    deadline: std::time::Duration,
    mut supervision: Supervision<'_>,
    tools: &Tools,
    containment_deadline: &mut Option<std::time::Instant>,
) -> Result<String, ShellEnd> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    // **The child moves itself into `tools/` before its exec**
    // (`weaver-gate-Spec` section 8; `weaver-admin-Spec` 6.1, C4): the
    // handle's number and the gate's pid are read here, before the fork, so
    // the child's `pre_exec` allocates nothing. The spawn returns only once
    // the child has exec'd, so every kill below begins after the move, and a
    // move that fails fails the spawn: `Errored`, nothing run.
    let handle = tools.raw();
    let gate = nix::unistd::getpid();
    // **The confinement's layers follow the move** (`weaver-gate-Spec`
    // section 8.1): `command` registers the move first and the layers after
    // it, no-new-privs first, and builds no shell command without them.
    // SAFETY: `move_self` makes only async-signal-safe `nix` calls on
    // numbers and a path read before the fork.
    let mut shell =
        unsafe { shell_program.command(command, move || cgroup::move_self(handle, gate)) };
    // The child still leads its own process group, as before the cgroup's
    // kill: the group no longer carries the kill, and keeping it changes
    // nothing else.
    let mut child = shell
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| ShellEnd::Errored {
            detail: format!(
                "the fork, the shell's move into tools/, or its confinement failed: {error}"
            ),
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
                match supervision.wait(slice) {
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
    *containment_deadline = Some(deadline);
    let mut supervision = ContainmentSupervisor::new(supervision, ending);
    let killed = tools.kill().is_ok();
    #[cfg(test)]
    if killed {
        interrupt_tests::containment_entered();
    }
    let contained = killed
        && tools.wait_empty_until_with(deadline, |slice| supervision.wait(slice, Some(deadline)));
    // **The leader is signalled by its pid, then reaped within the bound**
    // (Codex on #167): the shell runs as the agent's account and may have
    // moved itself out of `tools/` (6.1, C1's first stated exception), where
    // the kill of `tools/` does not reach it and an unbounded reap would
    // hang the gate. It is this process's own unreaped child, so its pid
    // cannot have been reused. A leader not reaped within the bound is an
    // execution not contained.
    let reaped = if contained {
        let _ = nix::sys::signal::kill(group, nix::sys::signal::Signal::SIGKILL);
        reap_supervised(&mut child, deadline, &mut supervision)
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
    let killed_by = supervision.finish(deadline)?;
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
    mut progress: impl FnMut(std::time::Duration),
) -> Option<std::io::Result<std::process::ExitStatus>> {
    loop {
        progress(std::time::Duration::ZERO);
        match child.try_wait() {
            Ok(Some(status)) => return Some(Ok(status)),
            Ok(None) => {}
            Err(error) => return Some(Err(error)),
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        progress(remaining.min(std::time::Duration::from_millis(10)));
    }
}

/// Keep the actual leader-reap path and its channel progress under one bound.
/// The helper is also the test boundary: fixtures exercise this exact forwarding
/// call instead of copying the closure and leaving production unprotected.
fn reap_supervised(
    child: &mut std::process::Child,
    deadline: std::time::Instant,
    supervision: &mut ContainmentSupervisor<'_>,
) -> Option<std::io::Result<std::process::ExitStatus>> {
    reap_within(child, deadline, |slice| {
        supervision.wait(slice, Some(deadline))
    })
}

/// One bounded channel operation per slice. A misplaced frame retains one
/// refusal until writable instead of blocking containment in Channel::send.
struct Supervision<'a> {
    channel: &'a Channel,
    exchange: &'a ExchangeId,
    answered_tools: Option<&'a BTreeSet<u64>>,
    pending_refusal: Option<OrganEnvelope>,
    progressed: bool,
}

impl<'a> Supervision<'a> {
    #[cfg(test)]
    fn new(channel: &'a Channel, exchange: &'a ExchangeId) -> Self {
        Self::with_answered(channel, exchange, None)
    }

    fn with_answered(
        channel: &'a Channel,
        exchange: &'a ExchangeId,
        answered_tools: Option<&'a BTreeSet<u64>>,
    ) -> Self {
        Self {
            channel,
            exchange,
            answered_tools,
            pending_refusal: None,
            progressed: false,
        }
    }

    fn wait(&mut self, slice: std::time::Duration) -> Result<Option<KillCause>, ChannelFault> {
        self.wait_for_delivery(slice, None)
    }

    fn wait_for_delivery(
        &mut self,
        slice: std::time::Duration,
        delivery_deadline: Option<std::time::Instant>,
    ) -> Result<Option<KillCause>, ChannelFault> {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        self.progressed = false;
        let interest = if self.pending_refusal.is_some() {
            PollFlags::POLLOUT
        } else {
            PollFlags::POLLIN
        };
        let mut fds = [PollFd::new(self.channel.as_fd(), interest)];
        let timeout = PollTimeout::try_from(slice).expect("the supervision slice fits poll");
        #[cfg(test)]
        tests::wait_entered();
        match poll(&mut fds, timeout) {
            Ok(0) | Err(nix::errno::Errno::EINTR) => return Ok(None),
            Err(_) => return Err(ChannelFault::Closed),
            Ok(_) => {}
        }
        if let Some(refusal) = &self.pending_refusal {
            // Poll may return after the original bound. Preserve a transport
            // failure already observed, but never start a late refusal send.
            if fds[0].revents().is_some_and(|events| {
                events.intersects(PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL)
            }) {
                return Err(ChannelFault::Closed);
            }
            if delivery_deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                return Ok(None);
            }
            if self.channel.try_send(refusal)? {
                self.pending_refusal = None;
                self.progressed = true;
            }
            return Ok(None);
        }
        let envelope = self.channel.recv()?;
        self.progressed = true;
        // The cause is closed independently of position and correlation. A
        // known but invalid kill cause is a protocol fault, never a refusal.
        if let Payload::ToolInterrupt { by } = &envelope.payload
            && !matches!(by, KillCause::Unload | KillCause::FlowFailure)
        {
            return Err(ChannelFault::Undecodable);
        }
        // A control can cross an answer and arrive during the next execution.
        // The successful-send set survives that boundary. Discard only a valid
        // control for that exact answered harness exchange, never a new close.
        if envelope.exchange.opener == weaver_types::Opener::Harness
            && envelope.position == Position::Continue
            && matches!(
                &envelope.payload,
                Payload::ToolCancel | Payload::ToolInterrupt { .. }
            )
            && self
                .answered_tools
                .is_some_and(|answered| answered.contains(&envelope.exchange.ordinal))
        {
            return Ok(None);
        }
        if envelope.exchange == *self.exchange && envelope.position == Position::Continue {
            match envelope.payload {
                Payload::ToolCancel => return Ok(Some(KillCause::Cancel)),
                Payload::ToolInterrupt { by } => return Ok(Some(by)),
                _ => {}
            }
        }
        self.pending_refusal = Some(OrganEnvelope {
            exchange: envelope.exchange,
            position: Position::Close,
            payload: Payload::Refusal(LifecycleRefusal::OutOfOrder),
        });
        Ok(None)
    }
}

/// Containment does not elect another outcome. Only an unsent unload kill can
/// be upgraded, and transport failure must still allow the cleanup to finish.
struct ContainmentSupervisor<'a> {
    supervision: Supervision<'a>,
    ending: Result<Option<KillCause>, ShellEnd>,
    channel_failed: bool,
}

impl<'a> ContainmentSupervisor<'a> {
    fn new(supervision: Supervision<'a>, ending: Result<Option<KillCause>, ShellEnd>) -> Self {
        let channel_failed = matches!(ending, Err(ShellEnd::Channel(_)));
        Self {
            supervision,
            ending,
            channel_failed,
        }
    }

    fn wait(&mut self, slice: std::time::Duration, delivery_deadline: Option<std::time::Instant>) {
        if self.channel_failed {
            std::thread::sleep(slice);
            return;
        }
        match self.supervision.wait_for_delivery(slice, delivery_deadline) {
            Ok(Some(KillCause::FlowFailure))
                if matches!(self.ending, Ok(Some(KillCause::Unload))) =>
            {
                self.ending = Ok(Some(KillCause::FlowFailure));
            }
            Ok(_) => {}
            Err(fault) => {
                self.ending = Err(ShellEnd::Channel(fault));
                self.channel_failed = true;
            }
        }
    }

    fn finish(mut self, deadline: std::time::Instant) -> Result<Option<KillCause>, ShellEnd> {
        // Drain controls already readable before choosing the unsent answer.
        // An endless stream cannot move the one original containment deadline.
        while !self.channel_failed && std::time::Instant::now() < deadline {
            self.wait(std::time::Duration::ZERO, Some(deadline));
            if !self.supervision.progressed {
                if self.supervision.pending_refusal.is_none() {
                    break;
                }
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                self.wait(
                    remaining.min(std::time::Duration::from_millis(10)),
                    Some(deadline),
                );
            }
        }
        if !self.channel_failed
            && let Some(refusal) = &self.supervision.pending_refusal
        {
            return Err(ShellEnd::DeliveryBound {
                exchange: refusal.exchange.clone(),
                answer: UndeliveredAnswer::OutOfOrder,
            });
        }
        self.ending
    }
}

/// Deliver a contained outcome once. An unload remains revisable until the
/// actual datagram send succeeds, including while socket capacity is absent.
/// There is no new kill clock or fresh containment interval at this boundary.
/// The contract bounds completion, so even the first send is refused at expiry.
pub fn send_answer(
    channel: &Channel,
    exchange: &ExchangeId,
    mut answer: ExecutionAnswer,
    answered_tools: &BTreeSet<u64>,
) -> Result<(), Unanswered> {
    let ending = match &answer.outcome {
        ToolOutcome::Killed { by, .. } => Some(*by),
        _ => None,
    };
    let mut supervision = ContainmentSupervisor::new(
        Supervision::with_answered(channel, exchange, Some(answered_tools)),
        Ok(ending),
    );
    loop {
        // A fault already observed during the preceding bounded wait keeps
        // its identity, even when that wait returned at the deadline.
        if supervision.channel_failed {
            let Err(ShellEnd::Channel(fault)) = supervision.ending else {
                unreachable!("a failed channel retains its exact fault")
            };
            return Err(Unanswered::Channel(fault));
        }
        if answer
            .containment_deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            let (exchange, answer) = match &supervision.supervision.pending_refusal {
                Some(refusal) => (&refusal.exchange, UndeliveredAnswer::OutOfOrder),
                None => (exchange, UndeliveredAnswer::ToolAnswer),
            };
            return Err(Unanswered::DeliveryBound {
                exchange: exchange.clone(),
                answer,
            });
        }
        supervision.wait(std::time::Duration::ZERO, answer.containment_deadline);
        if supervision.channel_failed {
            continue;
        }
        if supervision.supervision.progressed {
            continue;
        }
        if supervision.supervision.pending_refusal.is_none() {
            if let (ToolOutcome::Killed { by, .. }, Ok(Some(current))) =
                (&mut answer.outcome, &supervision.ending)
            {
                *by = *current;
            }
            let envelope = OrganEnvelope {
                exchange: exchange.clone(),
                position: Position::Close,
                payload: Payload::ToolAnswer(answer.outcome.clone()),
            };
            // Even a zero-time poll, control parse or envelope clone can
            // cross the bound. Recheck immediately before the first write.
            if answer
                .containment_deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
            {
                continue;
            }
            if channel.try_send(&envelope).map_err(Unanswered::Channel)? {
                return Ok(());
            }
        }
        let slice = answer
            .containment_deadline
            .map(|deadline| deadline.saturating_duration_since(std::time::Instant::now()))
            .unwrap_or(std::time::Duration::from_millis(10))
            .min(std::time::Duration::from_millis(10));
        supervision.wait(slice, answer.containment_deadline);
    }
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

#[cfg(test)]
#[path = "tools/interrupt_tests.rs"]
mod interrupt_tests;

#[cfg(test)]
#[path = "tools/delivery_tests.rs"]
mod delivery_tests;
