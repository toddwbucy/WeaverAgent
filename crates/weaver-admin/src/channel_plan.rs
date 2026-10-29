//! One original decision deadline across send, readiness, receive and decoding.
use super::{ChannelFault, Coordination, MAX_ENVELOPE_BYTES, poll_cap_ms};
use nix::sys::socket::MsgFlags;
use std::os::fd::AsRawFd;
use std::time::Instant;
use weaver_types::OrganEnvelope;

impl Coordination {
    pub(crate) fn send_plan_until(&self, bytes: &[u8], end: Instant) -> Result<(), ChannelFault> {
        #[cfg(test)]
        observe_plan_deadline(true, end);
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(ChannelFault::Truncated {
                bound: MAX_ENVELOPE_BYTES,
            });
        }
        loop {
            remaining(end)?;
            match nix::sys::socket::send(
                self.fd.as_raw_fd(),
                bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(count) if count == bytes.len() => return remaining(end).map(|_| ()),
                Ok(_) => return Err(ChannelFault::Closed),
                Err(nix::errno::Errno::EINTR) => (),
                Err(nix::errno::Errno::EAGAIN) => self.wait_plan_until(nix::libc::POLLOUT, end)?,
                Err(_) => return Err(ChannelFault::Closed),
            }
        }
    }

    pub(crate) fn recv_plan_until(&self, end: Instant) -> Result<OrganEnvelope, ChannelFault> {
        #[cfg(test)]
        observe_plan_deadline(false, end);
        loop {
            self.wait_plan_until(nix::libc::POLLIN, end)?;
            let Some(bytes) = self.packet_bytes()? else {
                continue;
            };
            remaining(end)?;
            let wire = std::str::from_utf8(&bytes).map_err(|_| ChannelFault::Undecodable)?;
            let envelope = weaver_types::state_flow::decode_strict(wire)
                .map_err(|_| ChannelFault::Undecodable)?;
            remaining(end)?;
            return Ok(envelope);
        }
    }

    fn wait_plan_until(&self, events: i16, end: Instant) -> Result<(), ChannelFault> {
        loop {
            let wait = remaining(end)?;
            let millis = wait.as_nanos().div_ceil(1_000_000).min(poll_cap_ms()) as i32;
            let mut fd = nix::libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events,
                revents: 0,
            };
            // SAFETY: this sole channel owner retains the descriptor throughout poll.
            let result = unsafe { nix::libc::poll(&mut fd, 1, millis) };
            if result < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return Err(ChannelFault::Closed);
            }
            remaining(end)?;
            if result > 0 {
                return Ok(());
            }
        }
    }
}

fn remaining(end: Instant) -> Result<std::time::Duration, ChannelFault> {
    end.checked_duration_since(Instant::now())
        .filter(|wait| !wait.is_zero())
        .ok_or(ChannelFault::Unanswered)
}

// Test-only observation of the bounds supplied by the actual exchange owner.
// Neither this probe nor its consumer changes a socket or clock result.
#[cfg(test)]
type PlanDeadlines = Vec<(bool, Instant, Instant)>;
#[cfg(test)]
type PlanDeadlineProbe = (PlanDeadlines, std::sync::mpsc::Sender<()>);
#[cfg(test)]
thread_local! {
    static PLAN_DEADLINES: std::cell::RefCell<Option<PlanDeadlineProbe>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn observe_plan_deadline(send: bool, end: Instant) {
    PLAN_DEADLINES.with(|slot| {
        if let Some((values, started)) = &mut *slot.borrow_mut() {
            values.push((send, end, Instant::now()));
            if send {
                started.send(()).unwrap();
            }
        }
    });
}
#[cfg(test)]
pub(crate) fn with_plan_deadlines<T>(
    started: std::sync::mpsc::Sender<()>,
    action: impl FnOnce() -> T,
) -> (T, PlanDeadlines) {
    struct Restore(Option<PlanDeadlineProbe>);
    impl Drop for Restore {
        fn drop(&mut self) {
            PLAN_DEADLINES.with(|slot| slot.replace(self.0.take()));
        }
    }
    let _restore = Restore(PLAN_DEADLINES.with(|slot| slot.replace(Some((Vec::new(), started)))));
    let result = action();
    let deadlines = PLAN_DEADLINES.with(|slot| slot.take().unwrap().0);
    (result, deadlines)
}
