//! A test that drops its own socket end must wait for the socket's release
//! before it observes the close from the far end. A concurrent test's fork
//! holds a copy of every descriptor in the process until its exec, so a drop
//! alone can leave the socket open, and one nonblocking observation then reads
//! Pending (#482). This is the harness's ruled form, `await_release_until` in
//! weaver-harness's `flow/fixture_eof.rs`, which an organ cannot link. The two
//! copies keep the same form, each with its own tests: an edit to one is made
//! to both.

use nix::errno::Errno;
use nix::libc;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::time::{Duration, Instant};

/// The bound on one release wait. A fork's copy lasts until its exec, so this
/// bounds a hang only.
const RELEASE_WAIT: Duration = Duration::from_secs(5);

/// Wait, without reading, until `end` shows that its far end was released:
/// the hangup, or the error a release raises. A pending receive is not a
/// release, nor is a half-close, and no byte is consumed. The poll asks for no
/// event, since the hangup and the error are always reported, so a half-closed
/// peer's read hangup cannot turn the wait into a spin. The caller supplies one
/// absolute fixture deadline, and the wait panics at it.
pub(super) fn await_release_until(end: BorrowedFd<'_>, deadline: Instant) {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("fixture peer was not released by its deadline");
        let mut poll = libc::pollfd {
            fd: end.as_raw_fd(),
            events: 0,
            revents: 0,
        };
        // SAFETY: the caller's borrow keeps the descriptor open through this
        // bounded observation, and poll receives one writable pollfd.
        let result =
            unsafe { libc::poll(&mut poll, 1, remaining.as_millis().clamp(1, 100) as i32) };
        assert!(
            result >= 0 || Errno::last() == Errno::EINTR,
            "fixture release poll failed"
        );
        assert_eq!(
            poll.revents & libc::POLLNVAL,
            0,
            "fixture release poll on a descriptor that is not open"
        );
        if poll.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            return;
        }
    }
}

/// The deadline of one release wait from now.
pub(super) fn release_deadline() -> Instant {
    Instant::now() + RELEASE_WAIT
}

mod tests;
