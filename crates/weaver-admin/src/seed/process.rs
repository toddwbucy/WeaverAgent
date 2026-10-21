//! The seed operation's one contained child. The coordinator keeps both locks
//! and performs the copy; this module closes, reaps and removes containment
//! before it can report a successful finish.

use std::fmt;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use super::preparation::PreparedSeed;
use crate::{AgentAccount, ServiceConfig, agent_cgroup, containment, inventory, start};

#[derive(Debug)]
pub(super) struct ProcessFailure {
    pub detail: String,
    pub cleanup: Option<String>,
}

impl ProcessFailure {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            cleanup: None,
        }
    }

    fn with_cleanup(mut self, cleanup: Result<(), Self>) -> Self {
        if let Err(error) = cleanup {
            self.cleanup = Some(match self.cleanup {
                Some(previous) => format!("{previous}; {error}"),
                None => error.to_string(),
            });
        }
        self
    }
}

impl fmt::Display for ProcessFailure {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(&self.detail)?;
        if let Some(cleanup) = &self.cleanup {
            write!(output, "; seed cleanup failed: {cleanup}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ProcessFailure {}

/// Must be consumed by `finish` or `rollback`. Neither dropping a Child nor
/// dropping a socket establishes the cleanup required for publication.
#[must_use = "the seed child must be finished or rolled back before releasing its locks"]
pub(super) struct SeedChild<'lock> {
    child: Child,
    channel: OwnedFd,
    room: OwnedFd,
    group: OperationGroup,
    _run_lock: &'lock start::RunLock,
}

struct OperationGroup {
    directory: OwnedFd,
    agent: String,
    record: containment::Record,
}

struct Launch<'a> {
    binary: &'a Path,
    room_path: &'a Path,
    room: OwnedFd,
    member: inventory::MemberAccount,
}

impl OperationGroup {
    fn end(self) -> Result<(), ProcessFailure> {
        // `run_end` judges its current record. This operation additionally
        // requires that it is still the exact record made before its fork.
        match containment::read_record(self.directory.as_fd(), &self.agent) {
            Ok(Some(record)) if record == self.record => {}
            _ => {
                return Err(ProcessFailure::new(
                    "the seed operation's cgroup record changed",
                ));
            }
        }
        // ADMIN-SEED holds the parent's run lock through publication. No child
        // may remain when we accept this result; the parent is outside this
        // cgroup. Do not use the serving run-lock-holder escalation here.
        strict_end(agent_cgroup::run_end(
            self.directory.as_fd(),
            &self.agent,
            agent_cgroup::Concluding::Run(&self.record.run),
            containment::CONTAINMENT_BOUND,
        ))
    }
}

fn strict_end(
    result: Result<agent_cgroup::Ended, agent_cgroup::EndRefused>,
) -> Result<(), ProcessFailure> {
    match result {
        Ok(agent_cgroup::Ended::Removed {
            record_removed: true,
            ..
        }) => Ok(()),
        other => Err(ProcessFailure::new(format!(
            "the seed operation's cgroup was not completely removed: {other:?}"
        ))),
    }
}

impl<'lock> SeedChild<'lock> {
    pub(super) fn start(
        config: &ServiceConfig,
        agent: &weaver_types::AgentName,
        prepared: &PreparedSeed,
        run_lock: &'lock start::RunLock,
        deadline: Instant,
    ) -> Result<Self, ProcessFailure> {
        check_deadline(deadline)?;
        if config.agent != agent.0 {
            return Err(ProcessFailure::new(
                "the seed agent does not match its service configuration",
            ));
        }
        let inventory = prepared.inventory();
        let member = inventory
            .member_account
            .ok_or_else(|| ProcessFailure::new("the seed inventory has no state member account"))?;
        let owner = AgentAccount::resolve(&inventory.identity)
            .map_err(|_| ProcessFailure::new("the agent's account does not resolve"))?;
        let binary = config
            .worker
            .parent()
            .ok_or_else(|| ProcessFailure::new("the worker binary has no parent directory"))?
            .join("weaver-state");
        let territory = config
            .territory_fd()
            .map_err(|_| ProcessFailure::new("the seed territory is not held by its judgment"))?;
        let room_path = crate::prepare_territory(territory, &config.territory, member)
            .ok_or_else(|| ProcessFailure::new("the state member's room could not be prepared"))?;
        let room = nix::fcntl::openat(
            territory,
            crate::save_points::ROOM,
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_DIRECTORY
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .map_err(|error| {
            ProcessFailure::new(format!("the state member's room does not open: {error}"))
        })?;
        let directory =
            agent_cgroup::open_run_directory(&config.run_directory()).map_err(|error| {
                ProcessFailure::new(format!("the run directory does not open: {error}"))
            })?;
        let run = crate::channel::mint_run_reference(&agent.0).ok_or_else(|| {
            ProcessFailure::new("the seed containment reference could not be minted")
        })?;
        check_deadline(deadline)?;
        let made = agent_cgroup::make(directory.as_fd(), &agent.0, &run.0, (owner.uid, owner.gid))
            .map_err(|error| {
                ProcessFailure::new(format!("the seed cgroup could not be made: {error:?}"))
            })?;
        Self::in_group(
            Launch {
                binary: &binary,
                room_path: &room_path,
                room,
                member,
            },
            run_lock,
            directory,
            &agent.0,
            made,
            deadline,
        )
    }

    fn in_group(
        launch: Launch<'_>,
        run_lock: &'lock start::RunLock,
        directory: OwnedFd,
        agent: &str,
        made: agent_cgroup::Made,
        deadline: Instant,
    ) -> Result<Self, ProcessFailure> {
        let group = OperationGroup {
            directory,
            agent: agent.to_owned(),
            record: made.record.clone(),
        };
        let Launch {
            binary,
            room_path,
            room,
            member,
        } = launch;
        let launch = check_deadline(deadline).and_then(|()| {
            spawn_member(
                binary,
                room_path,
                member,
                run_lock,
                made.constituents_procs(),
            )
            .map_err(|error| {
                ProcessFailure::new(format!("the seed member could not be started: {error}"))
            })
        });
        drop(made);
        let (child, channel) = match launch {
            Ok(launched) => launched,
            Err(error) => return Err(error.with_cleanup(group.end())),
        };
        let child = Self {
            child,
            channel,
            room,
            group,
            _run_lock: run_lock,
        };
        if let Err(error) = check_deadline(deadline) {
            return Err(error.with_cleanup(child.rollback()));
        }
        Ok(child)
    }

    pub(super) fn channel(&self) -> BorrowedFd<'_> {
        self.channel.as_fd()
    }

    pub(super) fn room(&self) -> BorrowedFd<'_> {
        self.room.as_fd()
    }

    /// Any exit before the coordinator closes the completed copy is a failed
    /// operation, even when the member returned exit status zero.
    pub(super) fn check_alive(&mut self) -> Result<(), ProcessFailure> {
        match self.child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(ProcessFailure::new(format!(
                "the seed member exited before channel close: {status}"
            ))),
            Err(error) => Err(ProcessFailure::new(format!(
                "the seed member's status could not be read: {error}"
            ))),
        }
    }

    pub(super) fn finish(self, deadline: Instant) -> Result<(), ProcessFailure> {
        self.conclude(Some(deadline))
    }

    pub(super) fn rollback(self) -> Result<(), ProcessFailure> {
        self.conclude(None)
    }

    fn conclude(mut self, deadline: Option<Instant>) -> Result<(), ProcessFailure> {
        // Observe a premature exit while our endpoint is still open. Closing
        // first would make an already exited zero-status child look like a
        // successful response to EOF.
        let before_close = if deadline.is_some() {
            self.check_alive()
        } else {
            Ok(())
        };
        let Self {
            mut child,
            channel,
            room: _,
            group,
            _run_lock,
        } = self;
        drop(channel);
        let result = match deadline {
            Some(deadline) => finish_member(&mut child, deadline),
            None => terminate_and_reap(&mut child, start::TERM_GRACE, start::KILL_GRACE)
                .map(|_| ())
                .map_err(|error| {
                    ProcessFailure::new(format!("the seed member could not be reaped: {error}"))
                }),
        };
        let result = match before_close {
            Ok(()) => result,
            Err(error) => Err(error.with_cleanup(result)),
        };
        // This must run even if a child would not reap. Cgroup kill reaches
        // descendants too, including one that moved to another process group.
        let cleanup = group.end();
        // A prior failed reap can be repaired by cgroup.kill, but cannot turn
        // a failed finish into publication authority.
        let reaped = wait_until(&mut child, Instant::now() + start::KILL_GRACE)
            .map_err(|error| {
                ProcessFailure::new(format!("the seed member's final reap failed: {error}"))
            })
            .and_then(|status| {
                status.ok_or_else(|| {
                    ProcessFailure::new("the seed member still stands after cgroup cleanup")
                })
            });
        let result = match (result, reaped) {
            (Ok(()), Ok(_)) => Ok(()),
            (Err(error), Ok(_)) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Err(error), Err(reap)) => Err(error.with_cleanup(Err(reap))),
        };
        match result {
            Ok(()) => cleanup,
            Err(error) => Err(error.with_cleanup(cleanup)),
        }
    }
}

fn check_deadline(deadline: Instant) -> Result<(), ProcessFailure> {
    if Instant::now() >= deadline {
        Err(ProcessFailure::new(
            "the seed operation's load deadline expired",
        ))
    } else {
        Ok(())
    }
}

/// The sole exec path, fed only the held service configuration and judged
/// member identity by `start`. Tests exercise the same pre-exec against a
/// harmless placement descriptor; production always passes Made's descriptor.
fn spawn_member(
    binary: &Path,
    room: &Path,
    member: inventory::MemberAccount,
    run_lock: &start::RunLock,
    placement: BorrowedFd<'_>,
) -> io::Result<(Child, OwnedFd)> {
    let (admin_end, member_end) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::Stream,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )?;
    let member_gift = start::high(member_end.as_raw_fd())?;
    let lock_gift = start::high(run_lock.raw())?;
    let raw_member = member_gift.as_raw_fd();
    let raw_lock = lock_gift.as_raw_fd();
    let placement = placement.as_raw_fd();
    let mut command = Command::new(binary);
    command
        .arg("--mode=seed")
        .arg(room)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    match crate::log::open_append(
        &room.join("state.log"),
        Some(crate::state_log_owner(member)),
    ) {
        Ok(log) => {
            command.stderr(log);
        }
        Err(error) => {
            diag!("weaver-admin: the seed member's state.log is not opened: {error}");
            command.stderr(Stdio::null());
        }
    }
    // SAFETY: only async-signal-safe syscalls and existing pre-exec helpers.
    unsafe {
        command.pre_exec(move || {
            containment::place_self(placement)?;
            start::place(raw_lock, start::RUN_LOCK_FD)?;
            start::seal_except(&[3, start::RUN_LOCK_FD])?;
            start::detach_and_reset()?;
            if nix::libc::chdir(c"/".as_ptr()) == -1 {
                return Err(io::Error::last_os_error());
            }
            nix::libc::umask(0o027);
            if nix::libc::prctl(nix::libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            crate::become_member(member)?;
            crate::arm_member_end(raw_member)
        });
    }
    let child = command.spawn()?;
    // Neither the original nor lifted copy may keep the peer alive in admin.
    drop(member_end);
    drop(member_gift);
    drop(lock_gift);
    Ok((child, admin_end))
}

fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(None);
        };
        std::thread::sleep(left.min(Duration::from_millis(10)));
    }
}

fn finish_member(child: &mut Child, deadline: Instant) -> Result<(), ProcessFailure> {
    let normal = check_deadline(deadline).and_then(|()| {
        wait_until(child, deadline)
            .map_err(|error| {
                ProcessFailure::new(format!("the seed member's exit could not be read: {error}"))
            })?
            .ok_or_else(|| {
                ProcessFailure::new("the seed member did not exit within the finish bound")
            })
            .and_then(|status| {
                check_deadline(deadline)?;
                if status.success() {
                    Ok(())
                } else {
                    Err(ProcessFailure::new(format!(
                        "the seed member failed at close: {status}"
                    )))
                }
            })
    });
    match normal {
        Ok(()) => Ok(()),
        Err(error) => Err(error.with_cleanup(
            terminate_and_reap(child, start::TERM_GRACE, start::KILL_GRACE)
                .map(|_| ())
                .map_err(|error| {
                    ProcessFailure::new(format!("the seed member could not be reaped: {error}"))
                }),
        )),
    }
}

fn terminate_and_reap(child: &mut Child, term: Duration, kill: Duration) -> io::Result<ExitStatus> {
    if let Some(status) = child.try_wait()? {
        return Ok(status);
    }
    // The unreaped Child retains this pid; no unrelated process can reuse it.
    // SAFETY: the Child still owns the unreaped pid and SIGTERM is valid.
    if unsafe { nix::libc::kill(child.id() as i32, nix::libc::SIGTERM) } == -1 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(nix::libc::ESRCH) {
            return Err(error);
        }
    }
    if let Some(status) = wait_until(child, Instant::now() + term)? {
        return Ok(status);
    }
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(nix::libc::ESRCH) => {}
        Err(error) => return Err(error),
    }
    wait_until(child, Instant::now() + kill)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "the seed member did not exit after SIGKILL",
        )
    })
}

#[cfg(test)]
mod tests;
