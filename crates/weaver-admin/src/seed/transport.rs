//! The one state-seed/1 exchange, under ADMIN-SEED and TYPES-SEED.
//!
//! The caller supplies the invocation's original deadline and retains the
//! channel after a created answer, so the member keeps its image through copy.

use std::fmt;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::time::Instant;

use weaver_types::state_flow::{
    FrameDirection, STATE_ANSWER_LINE_BOUND, SeedCreated, SeedFrame, SeedRefusal, SeedRequest,
    decode_seed_line, encode_seed_line,
};

#[derive(Debug)]
pub(super) enum ExchangeFailure {
    Refused { code: SeedRefusal, detail: String },
    Io(io::Error),
    Malformed(String),
    Deadline,
}

impl fmt::Display for ExchangeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { code, detail } => write!(formatter, "seed refused {code:?}: {detail}"),
            Self::Io(error) => write!(formatter, "seed channel failed: {error}"),
            Self::Malformed(detail) => write!(formatter, "invalid seed exchange: {detail}"),
            Self::Deadline => formatter.write_str("seed invocation deadline elapsed"),
        }
    }
}

impl std::error::Error for ExchangeFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// A complete correlated answer grants copying only, never publication.
///
/// Per-call nonblocking flags leave the borrowed descriptor's state unchanged.
/// The frame bounds belong to the shared codec, not this invocation's policy.
pub(super) fn exchange(
    channel: BorrowedFd<'_>,
    request: &SeedRequest,
    deadline: Instant,
) -> Result<SeedCreated, ExchangeFailure> {
    check_deadline(deadline)?;
    let line = encode_seed_line(
        &SeedFrame::Seed {
            request: Box::new(request.clone()),
        },
        FrameDirection::Request,
    )
    .map_err(|error| ExchangeFailure::Malformed(error.to_string()))?;
    send_line(channel, &line, deadline)?;
    let line = receive_line(channel, deadline)?;
    let answer = decode_seed_line(&line, FrameDirection::Response)
        .map_err(|error| ExchangeFailure::Malformed(error.to_string()))?;
    answer
        .validate_response_to(request)
        .map_err(|error| ExchangeFailure::Malformed(error.to_string()))?;
    // Reject bytes queued behind the first complete frame, including when its LF
    // was exactly the last byte of a receive. A later unsolicited write cannot be
    // predicted here. The member's exit and cleanup remain the caller's judgment.
    let peer_closed = reject_buffered_tail(channel, deadline)?;
    check_deadline(deadline)?;
    match answer {
        SeedFrame::Created { created } if !peer_closed => Ok(created),
        SeedFrame::Created { .. } => Err(closed("member closed before its image was copied")),
        SeedFrame::Refused { code, detail } => Err(ExchangeFailure::Refused { code, detail }),
        SeedFrame::Seed { .. } => Err(ExchangeFailure::Malformed("request used as answer".into())),
    }
}

fn check_deadline(deadline: Instant) -> Result<(), ExchangeFailure> {
    if Instant::now() >= deadline {
        Err(ExchangeFailure::Deadline)
    } else {
        Ok(())
    }
}

fn closed(detail: &str) -> ExchangeFailure {
    ExchangeFailure::Io(io::Error::new(io::ErrorKind::UnexpectedEof, detail))
}

fn send_line(
    channel: BorrowedFd<'_>,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), ExchangeFailure> {
    let mut sent = 0;
    while sent < bytes.len() {
        check_deadline(deadline)?;
        let pending = &bytes[sent..];
        // SAFETY: the borrowed socket and pending byte slice live for this call.
        let count = unsafe {
            nix::libc::send(
                channel.as_raw_fd(),
                pending.as_ptr().cast(),
                pending.len(),
                nix::libc::MSG_DONTWAIT | nix::libc::MSG_NOSIGNAL,
            )
        };
        if count > 0 {
            sent += count as usize;
        } else if count == 0 {
            return Err(ExchangeFailure::Io(io::Error::new(
                io::ErrorKind::WriteZero,
                "seed channel made no write progress",
            )));
        } else {
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => wait(channel, nix::libc::POLLOUT, deadline)?,
                _ => return Err(ExchangeFailure::Io(error)),
            }
        }
    }
    Ok(())
}

fn receive_line(channel: BorrowedFd<'_>, deadline: Instant) -> Result<Vec<u8>, ExchangeFailure> {
    let mut line = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        check_deadline(deadline)?;
        // SAFETY: the borrowed socket and writable buffer live for this call.
        let count = unsafe {
            nix::libc::recv(
                channel.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                nix::libc::MSG_DONTWAIT,
            )
        };
        if count == 0 {
            return Err(closed("seed answer ended before its complete frame"));
        }
        if count < 0 {
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => {
                    wait(channel, nix::libc::POLLIN, deadline)?;
                    continue;
                }
                _ => return Err(ExchangeFailure::Io(error)),
            }
        }
        let received = &buffer[..count as usize];
        if line.len() + received.len() > STATE_ANSWER_LINE_BOUND {
            return Err(ExchangeFailure::Malformed(
                "seed answer exceeds its line bound".into(),
            ));
        }
        line.extend_from_slice(received);
        if let Some(end) = line.iter().position(|byte| *byte == b'\n') {
            if end + 1 != line.len() {
                return Err(ExchangeFailure::Malformed(
                    "bytes follow the seed answer".into(),
                ));
            }
            return Ok(line);
        }
        if line.len() == STATE_ANSWER_LINE_BOUND {
            return Err(ExchangeFailure::Malformed(
                "seed answer lacks its bounded LF".into(),
            ));
        }
    }
}

fn reject_buffered_tail(
    channel: BorrowedFd<'_>,
    deadline: Instant,
) -> Result<bool, ExchangeFailure> {
    let mut byte = 0u8;
    loop {
        check_deadline(deadline)?;
        // SAFETY: the borrowed socket and one writable byte live for this call.
        let count = unsafe {
            nix::libc::recv(
                channel.as_raw_fd(),
                (&mut byte as *mut u8).cast(),
                1,
                nix::libc::MSG_DONTWAIT | nix::libc::MSG_PEEK,
            )
        };
        if count > 0 {
            return Err(ExchangeFailure::Malformed(
                "bytes follow the seed answer".into(),
            ));
        }
        if count == 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        match error.kind() {
            io::ErrorKind::Interrupted => continue,
            io::ErrorKind::WouldBlock => return Ok(false),
            _ => return Err(ExchangeFailure::Io(error)),
        }
    }
}

fn wait(
    channel: BorrowedFd<'_>,
    events: nix::libc::c_short,
    deadline: Instant,
) -> Result<(), ExchangeFailure> {
    loop {
        check_deadline(deadline)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = nix::libc::timespec {
            tv_sec: remaining
                .as_secs()
                .try_into()
                .unwrap_or(nix::libc::time_t::MAX),
            tv_nsec: remaining.subsec_nanos().into(),
        };
        let mut descriptor = nix::libc::pollfd {
            fd: channel.as_raw_fd(),
            events,
            revents: 0,
        };
        // SAFETY: one live descriptor and timespec are passed for this call. No
        // signal mask is changed. Every retry derives time from the same deadline.
        let result = unsafe { nix::libc::ppoll(&mut descriptor, 1, &timeout, std::ptr::null()) };
        if result > 0 {
            check_deadline(deadline)?;
            if descriptor.revents & nix::libc::POLLNVAL != 0 {
                return Err(ExchangeFailure::Io(io::Error::from_raw_os_error(
                    nix::libc::EBADF,
                )));
            }
            if descriptor.revents & (events | nix::libc::POLLHUP | nix::libc::POLLERR) != 0 {
                return Ok(());
            }
        } else if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(ExchangeFailure::Io(error));
            }
        }
    }
}

#[cfg(test)]
mod tests;
