//! The explicit admin-owned mode vector, additive until foundation cutover.

use std::io;
use std::os::fd::{BorrowedFd, FromRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;

use super::{
    channel::{Channel, invalid},
    room::{Room, read_image_fd},
};

pub(crate) fn dispatch(
    arguments: &[String],
    first_door: RawFd,
    image_fd: RawFd,
) -> Option<ExitCode> {
    let mode = arguments.first()?.strip_prefix("--mode=")?;
    if mode == "diagnostic" {
        if arguments.len() != 3 || arguments[1..].iter().any(|part| part.starts_with('-')) {
            eprintln!(
                "{}",
                serde_json::json!({"state_fault":"diagnostic mode requires territory and preload socket"})
            );
            return Some(ExitCode::FAILURE);
        }
        return Some(crate::member_entry(
            arguments[1..].iter().cloned(),
            first_door,
            image_fd,
        ));
    }
    let result = (|| {
        if arguments.len() != 2
            || !matches!(mode, "serving" | "seed")
            || arguments[1].starts_with('-')
        {
            return Err(invalid(
                "usage: weaver-state --mode=serving|seed <territory>",
            ));
        }
        // Probe all inherited numbers before a room open can reuse a missing one.
        // SAFETY: borrowed descriptor reads do not transfer ownership.
        let borrowed = unsafe { BorrowedFd::borrow_raw(first_door) };
        if nix::sys::socket::getsockopt(&borrowed, nix::sys::socket::sockopt::SockType)?
            != nix::sys::socket::SockType::Stream
        {
            return Err(invalid("fd3 is not the inherited stream"));
        }
        nix::sys::socket::getsockname::<nix::sys::socket::UnixAddr>(first_door)?;
        // SAFETY: F_GETFD probes the inherited lock without adopting it.
        if unsafe { nix::libc::fcntl(crate::RUN_LOCK_FD, nix::libc::F_GETFD) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let image = read_image_fd(image_fd, mode == "serving")?;
        // SAFETY: admin owns the launch vector and the inherited stream has been
        // checked before this one ownership transfer.
        let stream = unsafe { UnixStream::from_raw_fd(first_door) };
        let mut channel = Channel::new(stream)?;
        let room = Room::open(std::path::Path::new(&arguments[1]))?;
        let result = match image {
            Some(image) => super::run(&mut channel, &room, image),
            None => super::seed::run(&mut channel, &room),
        };
        // This runs after both ordinary EOF and failure. A failed cleanup is
        // visible and cannot become a retryable successful shutdown.
        let cleanup = room.clear_remnants();
        result.and(cleanup)
    })();
    Some(match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", serde_json::json!({"state_fault":error.to_string()}));
            ExitCode::FAILURE
        }
    })
}
