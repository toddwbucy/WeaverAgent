use super::*;
use nix::sys::socket::{AddressFamily, Shutdown, SockFlag, SockType, shutdown, socketpair};
use std::os::fd::AsRawFd;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

/// The CPU time this thread has used.
fn thread_cpu() -> Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes one timespec, which this frame owns.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) },
        0
    );
    Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

/// A half-close is no release: a peer that shut down its write half and still
/// holds the socket leaves the release wait to its deadline, and the wait
/// sleeps in poll rather than spinning on the read hangup.
#[test]
fn a_half_closed_peer_is_not_released_and_is_not_spun_on() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    shutdown(far.as_raw_fd(), Shutdown::Write).unwrap();
    let spent = thread_cpu();
    let deadline = Instant::now() + Duration::from_millis(300);
    assert!(
        catch_unwind(AssertUnwindSafe(|| await_release_until(
            near.as_raw_fd(),
            deadline
        )))
        .is_err()
    );
    let spent = thread_cpu() - spent;
    assert!(
        spent < Duration::from_millis(100),
        "the wait spun for {spent:?} of CPU over a 300 ms deadline"
    );
    drop(far);
}
