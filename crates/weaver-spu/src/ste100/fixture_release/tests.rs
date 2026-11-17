use super::*;
use nix::sys::socket::{AddressFamily, Shutdown, SockFlag, SockType, shutdown, socketpair};
use std::os::fd::{AsFd, OwnedFd};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("SEQPACKET pair")
}

fn soon(millis: u64) -> Instant {
    Instant::now() + Duration::from_millis(millis)
}

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

/// A dropped peer is a release.
#[test]
fn a_dropped_peer_is_released() {
    let (near, far) = pair();
    drop(far);
    await_release_until(near.as_fd(), release_deadline());
}

/// A live peer is no release: the wait panics at its deadline, not before it.
#[test]
fn a_live_peer_is_not_released_by_the_deadline() {
    let (near, _far) = pair();
    let started = Instant::now();
    let deadline = soon(100);
    assert!(
        catch_unwind(AssertUnwindSafe(|| await_release_until(
            near.as_fd(),
            deadline
        )))
        .is_err()
    );
    assert!(Instant::now() >= deadline, "the wait gave up early");
    assert!(started.elapsed() >= Duration::from_millis(100));
}

/// A copy of the dropped end holds the socket open until the copy closes. The
/// wait spans that window, a concurrent fork's between its fork and its exec,
/// and returns at the release (#482).
#[test]
fn a_held_copy_delays_the_release_until_it_closes() {
    let (near, far) = pair();
    let copy = nix::unistd::dup(far.as_fd()).unwrap();
    drop(far);
    let started = Instant::now();
    let holder = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        drop(copy);
    });
    await_release_until(near.as_fd(), release_deadline());
    assert!(
        started.elapsed() >= Duration::from_millis(200),
        "the wait returned while a copy held the socket open"
    );
    holder.join().unwrap();
}

/// A half-close is no release: a peer that shut down its write half and still
/// holds the socket leaves the wait to its deadline, and the wait sleeps in
/// poll rather than spinning on the read hangup.
#[test]
fn a_half_closed_peer_is_not_released_and_is_not_spun_on() {
    let (near, far) = pair();
    shutdown(far.as_raw_fd(), Shutdown::Write).unwrap();
    let spent = thread_cpu();
    assert!(
        catch_unwind(AssertUnwindSafe(|| await_release_until(
            near.as_fd(),
            soon(300)
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
