//! The scaffolding the three integration suites share: the socket pair, the
//! descriptor placement, and the one-directive exchange. It lives once because
//! it existed three times, and the third copy had dropped the load-bearing
//! comment on why the lifted descriptors must be closed. A descriptor-protocol
//! fix applied to one copy and missed by the others is exactly the divergence a
//! shared module prevents.
//!
//! Each test binary compiles its own copy and uses its own subset, so the
//! unused-code lint is silenced here once rather than per-consumer: a helper
//! unused by one suite is not dead, it is another suite's.
#![allow(dead_code)]

use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use nix::libc;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use weaver_types::{ExchangeId, LifecycleDirective, Opener, OrganEnvelope, Payload, Position};

pub fn seqpacket_pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("a socketpair")
}

/// Arrange for the child to hold exactly the given descriptors at 3, 4, and
/// upward, in order, and nothing else.
///
/// The placement runs in two passes because a source can already sit on a
/// number about to be written, and writing targets directly would clobber a
/// source not yet placed. The first pass lifts every source above the target
/// range with `F_DUPFD`, the second writes the targets with `dup2`, which
/// clears close-on-exec on what it writes so exactly these survive the exec.
///
/// Nothing in the closure allocates: it runs in the child of a multithreaded
/// test process, where another thread may hold the allocator lock at the
/// moment of the fork.
pub fn place_inherited(command: &mut Command, child_ends: &[RawFd]) {
    let mut sources = [0 as RawFd; 8];
    let count = child_ends.len();
    assert!(
        count <= sources.len(),
        "place_inherited takes at most {} descriptors, got {count}",
        sources.len()
    );
    sources[..count].copy_from_slice(child_ends);
    unsafe {
        command.pre_exec(move || {
            // Pass one: lift every source clear of the target range.
            let mut lifted = [0 as RawFd; 8];
            for (slot, source) in lifted[..count].iter_mut().zip(&sources[..count]) {
                let high = libc::fcntl(*source, libc::F_DUPFD, 32);
                if high < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                *slot = high;
            }
            // Pass two: write the targets, which also clears close-on-exec.
            for (offset, high) in lifted[..count].iter().enumerate() {
                if libc::dup2(*high, 3 + offset as RawFd) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            // `F_DUPFD` CLEARS close-on-exec on the copy rather than
            // inheriting it, so the lifted copies would survive the exec and
            // inflate the very count the child checks. Closing them is
            // required, not tidiness.
            for high in &lifted[..count] {
                libc::close(*high);
            }
            // Mark everything above the placed range close-on-exec, which is
            // the fork discipline the harness performs in production. The test
            // process is not a clean parent: an in-process CUDA or llama.cpp
            // load in a sibling test leaves driver descriptors open without
            // close-on-exec, the child inherits them, and its own count check
            // then refuses to serve - correctly. Found live: a child spawned
            // after a sibling's in-process load reported 24 descriptors held.
            //
            // **Marked, never closed.** Rust reports a failed exec to the
            // parent through a close-on-exec pipe it opened above this range,
            // and closing that pipe makes the parent read EOF and conclude the
            // spawn succeeded. Measured: with the descriptors closed, spawning
            // a nonexistent binary returns Ok and the child dies on a signal.
            // Marking leaves the pipe alive until exec, which is exactly when
            // it should go.
            let first = 3 + count as libc::c_uint;
            if libc::syscall(
                libc::SYS_close_range,
                first,
                libc::c_uint::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            ) != 0
            {
                // The flag is Linux 5.11+; this workshop is far past it, but a
                // fallback keeps the helper honest elsewhere. It runs to the
                // process's own descriptor limit rather than a round number,
                // because a descriptor above that number is what the child
                // would then count and refuse on, and it sets the flag on top
                // of what each descriptor already carries rather than
                // replacing it.
                let mut limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                let ceiling = if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) == 0 {
                    limit.rlim_cur as RawFd
                } else {
                    4096
                };
                for fd in first as RawFd..ceiling {
                    let flags = libc::fcntl(fd, libc::F_GETFD);
                    if flags >= 0 {
                        libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
                    }
                }
            }
            Ok(())
        });
    }
}

/// Bound every receive on this end, so a child that wedges turns into a test
/// failure naming the stage rather than a hang the harness cannot end.
pub fn bound_receives(end: &OwnedFd, seconds: i64) {
    use nix::sys::socket::sockopt::ReceiveTimeout;
    use nix::sys::time::TimeVal;
    nix::sys::socket::setsockopt(end, ReceiveTimeout, &TimeVal::new(seconds, 0))
        .expect("the receive timeout sets");
}

/// Wait for a child to exit, bounded, killing and reaping it on expiry.
///
/// An unbounded wait on a child that never exits is a hang the harness cannot
/// end, and a hang reports nothing about which stage wedged.
pub fn wait_bounded(child: &mut std::process::Child, seconds: u64, what: &str) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        match child.try_wait().expect("a wait succeeds") {
            Some(status) => return status,
            None if Instant::now() > deadline => {
                child.kill().ok();
                child.wait().ok();
                panic!("{what}: the child did not exit within {seconds}s");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// Send one directive as the harness and read the one answer, panicking with
/// the stage name if the child never answers within the bound set by
/// [`bound_receives`].
pub fn ask(end: &OwnedFd, ordinal: u64, directive: LifecycleDirective) -> OrganEnvelope {
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    };
    let body = serde_json::to_vec(&envelope).expect("an envelope encodes");
    send(end.as_raw_fd(), &body, MsgFlags::empty()).expect("the directive is sent");

    let mut buffer = vec![0u8; 64 * 1024];
    let read = recv(end.as_raw_fd(), &mut buffer, MsgFlags::empty())
        .unwrap_or_else(|errno| panic!("no answer to ordinal {ordinal}: {errno}"));
    buffer.truncate(read);
    serde_json::from_slice(&buffer).expect("the answer is an envelope")
}
