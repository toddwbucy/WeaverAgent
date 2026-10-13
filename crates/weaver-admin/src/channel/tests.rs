//! The dial's bound and the channel's framing, run from inside the crate
//! because this crate publishes no library surface.
//!
//! **The harness is stood in for here.** Under the inversion the worker
//! binds and this crate dials, so every test below plays the harness's
//! part with a listener of its own and exercises `dial` as the code under
//! test.

use super::*;
use nix::sys::socket::{bind, listen};
use weaver_types::{ExchangeId, LifecycleDirective, Opener, Payload, Position};

fn scratch(tag: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-chan-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let _ = std::fs::remove_dir_all(&dir.0);
    std::fs::create_dir_all(&dir.0).expect("scratch");
    dir
}

/// The harness's part: bind the per-agent name and listen. The real one
/// does this inside the unit's runtime directory as its first act.
fn harness_listener(dir: &std::path::Path) -> (OwnedFd, std::path::PathBuf) {
    let path = dir.join("alpha.sock");
    let fd = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .expect("socket");
    let address = UnixAddr::new(&path).expect("address");
    bind(fd.as_raw_fd(), &address).expect("bind");
    listen(&fd, nix::sys::socket::Backlog::new(1).unwrap()).expect("listen");
    (fd, path)
}

fn harness_accepts(listener: &OwnedFd) -> OwnedFd {
    let fd =
        nix::sys::socket::accept4(listener.as_raw_fd(), SockFlag::SOCK_CLOEXEC).expect("accept");
    // SAFETY: the kernel just created this descriptor and no other owner
    // exists.
    unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) }
}

fn directive(ordinal: u64, agent: &str) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Load {
            agent: weaver_types::AgentName(agent.to_string()),
        }),
    }
}

/// **The dial refuses within its bound rather than waiting.** A name
/// nothing bound is the worker that never came up, and section 7's ceiling
/// is what turns that into an answer.
///
/// Perturbation: remove the deadline test from `dial`'s loop and this test
/// never returns. Watched under exactly that removal.
#[test]
fn the_dial_refuses_within_its_bound() {
    let dir = scratch("bound");
    let path = dir.join("nothing-listens-here.sock");
    let started = std::time::Instant::now();
    let refused = dial(&path);
    let elapsed = started.elapsed();
    assert!(
        matches!(refused, Err(ChannelFault::NoListener)),
        "a dial against an unbound name refuses as no listener, the positive \
             evidence section 3 reads, got {refused:?}"
    );
    assert!(
        elapsed >= DIAL_CEILING,
        "the dial retried for the whole bound, took {elapsed:?}"
    );
    assert!(
        elapsed < DIAL_CEILING * 4,
        "the dial stopped at its ceiling rather than waiting on, took {elapsed:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A full backlog does not defeat the ceiling.** The harness serves one
/// connection at a time, so a verb arriving while another is in flight
/// meets a full backlog, and a blocking connect on an `AF_UNIX` socket
/// blocks there without bound: measured 2026-08-06, still blocked after
/// three seconds against a one second ceiling. The nonblocking connect is
/// what turns that into a refusal.
///
/// Perturbation: drop `SOCK_NONBLOCK` from `dial_once` and this test
/// blocks past its own assertion rather than failing an equality, which is
/// the hang the flag prevents. Watched under exactly that removal.
#[test]
fn a_full_backlog_refuses_within_the_bound() {
    let dir = scratch("backlog");
    let (listener, path) = harness_listener(&dir);
    // Fill the backlog: the listener never accepts, so these queue and
    // then the queue closes.
    let mut held = Vec::new();
    for _ in 0..8 {
        match dial(&path) {
            Ok(c) => held.push(c),
            Err(_) => break,
        }
    }
    let started = std::time::Instant::now();
    let refused = dial(&path);
    let elapsed = started.elapsed();
    assert!(
        matches!(refused, Err(ChannelFault::NotDialable)),
        "a full backlog refuses rather than blocking, got {refused:?}"
    );
    assert!(
        elapsed < DIAL_CEILING * 3,
        "and it refuses within the bound, took {elapsed:?}"
    );
    drop(held);
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The dial succeeds once the harness binds, which is the race the
/// bound covers.** The listener appears after the dial has begun.
#[test]
fn the_dial_wins_the_race_when_the_bind_is_late() {
    let dir = scratch("race");
    let path = dir.join("alpha.sock");
    let bind_dir = dir.to_path_buf();
    let late = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        harness_listener(&bind_dir)
    });
    let connected = dial(&path);
    let (listener, _) = late.join().expect("thread");
    assert!(
        connected.is_ok(),
        "a bind inside the bound is reached, got {connected:?}"
    );
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}

/// **One write is one read**, the boundary the socket type buys, tested
/// where the connection is made.
///
/// Perturbation: create the socket as `SOCK_STREAM` in `dial_once` and the
/// first read returns both envelopes. **Two messages are what make the
/// watch reachable** - a single small envelope crosses a stream socket
/// whole, so a one-message test would pass under the substitution.
#[test]
fn one_write_is_one_read() {
    let dir = scratch("framing");
    let (listener, path) = harness_listener(&dir);
    let admin = dial(&path).expect("dial");
    let worker = harness_accepts(&listener);
    let worker = Coordination::adopt(worker);
    worker.send(&directive(1, "alpha")).expect("first write");
    worker.send(&directive(2, "beta")).expect("second write");
    let first = admin.recv().expect("first read decodes on its own");
    let second = admin.recv().expect("second read decodes on its own");
    assert_eq!(first.exchange.ordinal, 1);
    assert_eq!(second.exchange.ordinal, 2);
    drop(worker);
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A bound past the clock never panics**: `recv_within` given the
/// longest duration reads the answer its peer sends, rather than
/// overflowing the deadline it adds. Perturbation: add the bound to the
/// clock unchecked and the call panics.
#[test]
fn a_bound_past_the_clock_waits_and_reads() {
    let dir = scratch("unbounded");
    let (listener, path) = harness_listener(&dir);
    let admin = dial(&path).expect("dial");
    let worker = Coordination::adopt(harness_accepts(&listener));
    worker.send(&directive(7, "alpha")).expect("write");
    let read = admin
        .recv_within(Duration::MAX)
        .expect("the answer is read within an unreachable bound");
    assert_eq!(read.exchange.ordinal, 7);
    drop(worker);
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A timeout of poll's capped wait with time left waits again**: a
/// declared drain and wind-down can make the leave's window longer than
/// poll's longest wait (about 24.8 days), and the read answers `Unanswered`
/// only once the window is spent. Here the cap is 20 ms and the answer
/// comes at 150 ms within a 2 s bound. Perturbation: answer `Unanswered`
/// at the first timeout, and the read gives up at 20 ms.
#[test]
fn a_capped_poll_timeout_with_time_left_waits_again() {
    let dir = scratch("poll-cap");
    let (listener, path) = harness_listener(&dir);
    let admin = dial(&path).expect("dial");
    let worker = Coordination::adopt(harness_accepts(&listener));
    let answering = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        worker.send(&directive(9, "alpha")).expect("write");
        worker
    });
    TEST_POLL_CAP_MS.with(|cap| cap.set(Some(20)));
    let read = admin.recv_within(Duration::from_secs(2));
    TEST_POLL_CAP_MS.with(|cap| cap.set(None));
    let worker = answering.join().expect("the peer");
    assert_eq!(
        read.expect("read past the capped waits").exchange.ordinal,
        9
    );
    drop(worker);
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}

/// **Truncation is a fault**, never a message: a read returning with
/// `MSG_TRUNC` set carries a prefix the kernel shortened.
///
/// Perturbation: remove the `MSG_TRUNC` check from `recv` and the
/// shortened octets reach the decoder.
#[test]
fn truncation_is_a_channel_fault() {
    let dir = scratch("truncate");
    let (listener, path) = harness_listener(&dir);
    let admin = dial(&path).expect("dial");
    let worker = harness_accepts(&listener);
    let oversized = vec![b'x'; MAX_ENVELOPE_BYTES + 4096];
    // SAFETY: a raw send is how the test produces a message larger than
    // the receiver's buffer; this crate's own send refuses to author one.
    let written = unsafe {
        nix::libc::send(
            worker.as_raw_fd(),
            oversized.as_ptr().cast(),
            oversized.len(),
            0,
        )
    };
    if written < 0 {
        diag!(
            "SKIP truncation_is_a_channel_fault: the kernel refused the oversized \
                 datagram (errno {}), so the MSG_TRUNC branch was not exercised here",
            std::io::Error::last_os_error()
        );
    } else {
        match admin.recv() {
            Err(ChannelFault::Truncated { bound }) => assert_eq!(bound, MAX_ENVELOPE_BYTES),
            other => panic!("a truncated read must be a fault, got {other:?}"),
        }
    }
    drop(worker);
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The enter directive and its ancillary payload are one message: the
/// descriptor crosses once, in the enter exchange, with no separate
/// delivery to order against anything.
#[test]
fn the_enter_carries_its_descriptor_in_one_message() {
    use std::os::fd::AsFd;
    let dir = scratch("ancillary");
    let (listener, path) = harness_listener(&dir);
    let admin = dial(&path).expect("dial");
    let worker_fd = harness_accepts(&listener);

    let sink = std::fs::File::create(dir.join("sink")).expect("sink");
    let sink = OwnedFd::from(sink);
    admin
        .send_with_sink(&directive(7, "alpha"), sink.as_fd(), None)
        .expect("one message");

    // The worker reads one message and finds the descriptor on it.
    let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
    let mut control = nix::cmsg_space!([std::os::fd::RawFd; 4]);
    let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
    let message = nix::sys::socket::recvmsg::<()>(
        worker_fd.as_raw_fd(),
        &mut slices,
        Some(&mut control),
        MsgFlags::MSG_CMSG_CLOEXEC,
    )
    .expect("one message read");
    let mut received = 0;
    for cmsg in message.cmsgs().expect("control data") {
        if let nix::sys::socket::ControlMessageOwned::ScmRights(fds) = cmsg {
            received += fds.len();
            for fd in fds {
                // SAFETY: adopting the descriptors the kernel installed so
                // the test closes them.
                drop(unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) });
            }
        }
    }
    assert_eq!(
        received, 1,
        "the sink crosses on the directive's own message"
    );
    assert!(message.bytes > 0, "the envelope crossed with it");
    drop(worker_fd);
    drop(listener);
    let _ = std::fs::remove_dir_all(&dir);
}
