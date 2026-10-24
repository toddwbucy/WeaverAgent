//! Serving's fixed fd3/fd4/fd9 launch, fed by the retained verified image.
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use crate::{ServiceConfig, serving_enter::PreparedEnter, start};
use weaver_types::LifecycleRefusal;

pub(super) fn spawn(
    config: &ServiceConfig,
    prepared: &PreparedEnter,
    run_lock: &start::RunLock,
    placement: BorrowedFd<'_>,
) -> Result<(Child, OwnedFd), LifecycleRefusal> {
    let member = prepared
        .inventory()
        .member_account
        .ok_or(LifecycleRefusal::BindFailed)?;
    let room = crate::prepare_territory(config.territory_fd()?, &config.territory, member)
        .ok_or(LifecycleRefusal::BoundaryUnverified)?;
    let binary = config.worker.with_file_name("weaver-state");
    let (harness, channel) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::Stream,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .map_err(|_| LifecycleRefusal::DescriptorsUnusable)?;
    let lifted = (|| -> io::Result<_> {
        Ok((
            start::high(channel.as_raw_fd())?,
            start::high(prepared.dump().as_raw_fd())?,
            start::high(run_lock.raw())?,
        ))
    })()
    .map_err(|_| LifecycleRefusal::DescriptorsUnusable)?;
    let (channel_fd, dump_fd, lock_fd) = (
        lifted.0.as_raw_fd(),
        lifted.1.as_raw_fd(),
        lifted.2.as_raw_fd(),
    );
    let placement = placement.as_raw_fd();
    let mut command = Command::new(binary);
    command
        .arg("--mode=serving")
        .arg(&room)
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
            diag!("weaver-admin: serving member log refused: {error}");
            command.stderr(Stdio::null());
        }
    }
    // SAFETY: async-signal-safe syscalls/helpers only, all source fds lifted
    // beyond the target numbers before fork. Placement precedes identity drop.
    unsafe {
        command.pre_exec(move || {
            crate::containment::place_self(placement)?;
            start::place(lock_fd, start::RUN_LOCK_FD)?;
            start::place(dump_fd, crate::SAVE_POINT_FD)?;
            start::seal_except(&[3, crate::SAVE_POINT_FD, start::RUN_LOCK_FD])?;
            start::detach_and_reset()?;
            if nix::libc::chdir(c"/".as_ptr()) == -1 {
                return Err(io::Error::last_os_error());
            }
            nix::libc::umask(0o027);
            if nix::libc::prctl(nix::libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            crate::become_member(member)?;
            crate::arm_member_end(channel_fd)
        });
    }
    let child = command.spawn().map_err(|_| LifecycleRefusal::BindFailed)?;
    // No admin copy of the member end survives; its only peer is the endpoint
    // passed in the same Enter packet as the trace sink. Admin never reads it.
    drop(channel);
    drop(lifted);
    Ok((child, harness))
}
