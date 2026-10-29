//! Test peers must observe actual EOF, including when another fork temporarily
//! retains a descriptor. A pending receive neither proves closure nor permits
//! skipping bytes. The caller supplies one absolute fixture deadline.

use nix::errno::Errno;
use nix::libc;
use nix::sys::socket::{MsgFlags, recv};
use std::os::fd::RawFd;
use std::time::Instant;

pub(super) fn await_eof_until(fd: RawFd, deadline: Instant, on_pending: impl FnOnce()) {
    let mut on_pending = Some(on_pending);
    loop {
        assert!(
            Instant::now() < deadline,
            "fixture socket did not reach EOF by its deadline"
        );
        match recv(fd, &mut [0; 1], MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return,
            Ok(_) => panic!("fixture socket carried bytes after required retirement"),
            Err(Errno::EINTR) => continue,
            Err(Errno::EAGAIN) => {
                if let Some(observe) = on_pending.take() {
                    observe();
                }
            }
            Err(error) => panic!("fixture EOF receive: {error}"),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: callers retain fd through this bounded observation, and poll
        // receives one writable pollfd. No wait renews the caller's deadline.
        let result = unsafe { libc::poll(&mut poll, 1, remaining.as_millis().min(1) as i32) };
        assert!(
            result >= 0 || Errno::last() == Errno::EINTR,
            "fixture EOF poll failed"
        );
    }
}

#[test]
fn a_live_peer_does_not_become_eof_at_the_deadline() {
    use std::os::fd::AsRawFd;
    let (near, _far) = std::os::unix::net::UnixStream::pair().unwrap();
    assert!(
        std::panic::catch_unwind(|| await_eof_until(near.as_raw_fd(), Instant::now(), || {}))
            .is_err()
    );
}

#[test]
fn bytes_arriving_after_a_pending_receive_are_rejected() {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::time::Duration;
    let (near, mut far) = std::os::unix::net::UnixStream::pair().unwrap();
    let (pending, observed) = std::sync::mpsc::channel();
    let child = std::thread::spawn(move || {
        await_eof_until(
            near.as_raw_fd(),
            Instant::now() + Duration::from_secs(3),
            || pending.send(()).unwrap(),
        );
    });
    observed.recv_timeout(Duration::from_secs(3)).unwrap();
    far.write_all(b"unexpected work").unwrap();
    drop(far);
    assert!(
        child.join().is_err(),
        "later bytes cannot be discarded as EOF"
    );
}
