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
    let refused = dial(&path, nix::unistd::getuid().as_raw(), &dir.0);
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
        match dial(&path, nix::unistd::getuid().as_raw(), &dir.0) {
            Ok(c) => held.push(c),
            Err(_) => break,
        }
    }
    let started = std::time::Instant::now();
    let refused = dial(&path, nix::unistd::getuid().as_raw(), &dir.0);
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
    let connected = dial(&path, nix::unistd::getuid().as_raw(), &dir.0);
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
    let admin = dial(&path, nix::unistd::getuid().as_raw(), &dir.0).expect("dial");
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
    let admin = dial(&path, nix::unistd::getuid().as_raw(), &dir.0).expect("dial");
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
    let admin = dial(&path, nix::unistd::getuid().as_raw(), &dir.0).expect("dial");
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
    let admin = dial(&path, nix::unistd::getuid().as_raw(), &dir.0).expect("dial");
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
    let admin = dial(&path, nix::unistd::getuid().as_raw(), &dir.0).expect("dial");
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

/// **The dial refuses a listener that is not the agent's worker, at once**
/// (#98; the custody audit's G3): the socket's directory is the agent's, so
/// a name replaced there could aim the verb at another agent's worker. The
/// peer's uid is read after the connect, and a stranger's refuses
/// `WrongPeer` without the ceiling's retries; the worker's own connects.
/// Perturbation: drop the comparison, and the stranger is dialed.
#[test]
fn the_dial_refuses_a_listener_that_is_not_the_worker() {
    let dir = scratch("peer");
    let (listener, path) = harness_listener(&dir);
    let me = nix::unistd::getuid().as_raw();
    let started = std::time::Instant::now();
    let refused = dial(&path, me.wrapping_add(1), &dir.0);
    assert!(
        matches!(refused, Err(ChannelFault::WrongPeer { uid, .. }) if uid == me),
        "{refused:?}"
    );
    assert!(
        started.elapsed() < DIAL_CEILING,
        "a wrong peer is not retried to the ceiling"
    );
    assert!(
        dial(&path, me, &dir.0).is_ok(),
        "the worker's own listener connects"
    );
    drop(listener);
}

/// A forked child of the suite's own uid that listens at `socket`, holding a
/// run lock's description on `run_lock` first where it is given one, and
/// answering once it listens. **A real second process**, as the gate, the
/// SPU or a tool is to the worker: the uid alone cannot tell them apart.
/// The ready byte says whether the child stands as asked, so a failed open,
/// lock or bind fails the test here rather than reading as a refusal.
fn forked_listener(socket: &std::path::Path, run_lock: Option<&std::path::Path>) -> Forked {
    // Built before the fork, so the child only calls what a fork of a
    // threaded process may.
    let address = UnixAddr::new(socket).expect("address");
    let lock_path =
        run_lock.map(|path| std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap());
    let lock = nix::libc::flock {
        l_type: nix::libc::F_WRLCK as i16,
        l_whence: nix::libc::SEEK_SET as i16,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    let (ready_read, ready_write) = nix::unistd::pipe().unwrap();
    // SAFETY: the child calls only async-signal-safe functions.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => {
            crate::start::close_inherited_except(Some(ready_write.as_raw_fd()));
            // SAFETY: open, fcntl, socket, bind, listen, write, pause and
            // _exit are async-signal-safe.
            unsafe {
                let mut stands = true;
                if let Some(path) = &lock_path {
                    let fd = nix::libc::open(
                        path.as_ptr(),
                        nix::libc::O_RDWR | nix::libc::O_CREAT,
                        0o600,
                    );
                    stands &= fd >= 0 && nix::libc::fcntl(fd, nix::libc::F_OFD_SETLK, &lock) == 0;
                }
                let listener = nix::libc::socket(nix::libc::AF_UNIX, nix::libc::SOCK_SEQPACKET, 0);
                stands &= listener >= 0
                    && nix::libc::bind(
                        listener,
                        nix::sys::socket::SockaddrLike::as_ptr(&address),
                        nix::sys::socket::SockaddrLike::len(&address),
                    ) == 0
                    && nix::libc::listen(listener, 8) == 0;
                nix::libc::write(
                    ready_write.as_raw_fd(),
                    [u8::from(stands)].as_ptr().cast(),
                    1,
                );
                nix::libc::pause();
                nix::libc::_exit(0);
            }
        }
        nix::unistd::ForkResult::Parent { child } => {
            drop(ready_write);
            let mut byte = [0u8; 1];
            nix::unistd::read(&ready_read, &mut byte).unwrap();
            let forked = Forked(child);
            assert_eq!(byte[0], 1, "the forked listener did not stand as asked");
            forked
        }
    }
}

/// A forked listener, ended and reaped when dropped. **Ended on a failed
/// assertion as well**: the child holds the test harness's output pipe, so
/// one left paused would hold the run open after the test has failed.
struct Forked(nix::unistd::Pid);

impl Drop for Forked {
    fn drop(&mut self) {
        // SAFETY: kill on a child this test forked and has not yet reaped.
        unsafe { nix::libc::kill(self.0.as_raw(), nix::libc::SIGKILL) };
        let _ = nix::sys::wait::waitpid(self.0, None);
    }
}

/// **A listener of the worker's uid that is not the worker is refused**
/// (#140; Spec section 7): two forked children of the suite's uid, the
/// worker holding the run lock's description and listening at its name, an
/// impostor holding none and listening at the agent's name, as an organ or
/// tool that replaced it would. The impostor refuses `WrongPeer` naming its
/// pid, without the ceiling's retries; the worker's own listener connects.
/// Perturbation: drop the holder check from `listener_is_the_worker` and the
/// impostor is dialed; break the holder scan (`holds` looking for another
/// lock kind) and the worker is refused.
#[test]
fn the_dial_refuses_a_listener_of_the_workers_uid_that_holds_no_run_lock() {
    let dir = scratch("pid");
    let me = nix::unistd::getuid().as_raw();
    let worker_name = dir.0.join("worker.sock");
    let impostor_name = dir.0.join("alpha.sock");
    let worker = forked_listener(&worker_name, Some(&dir.0.join("run.lock")));
    let impostor = forked_listener(&impostor_name, None);
    assert!(
        crate::start::run_lock_held(&dir.0).unwrap(),
        "the worker holds the run lock, so the impostor stands beside a live run"
    );

    let started = std::time::Instant::now();
    let refused = dial(&impostor_name, me, &dir.0);
    assert!(
        matches!(refused, Err(ChannelFault::WrongPeer { uid, pid }) if uid == me && pid == impostor.0.as_raw()),
        "{refused:?}"
    );
    assert!(
        started.elapsed() < DIAL_CEILING,
        "a wrong peer is not retried to the ceiling"
    );
    let accepted = dial(&worker_name, me, &dir.0);
    assert!(
        accepted.is_ok(),
        "the process holding the run lock is the worker: {accepted:?}"
    );

    drop((impostor, worker));
}

#[test]
fn replacement_enter_sender_bounds_whole_packet_before_transferring_descriptors() {
    use nix::sys::socket::{recv, socketpair};
    use std::os::fd::AsFd;
    use weaver_types::state_flow::enter::EnterEnvelope;
    for case in ["exact", "over", "escaped", "missing-state"] {
        let (end, peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let owner = Coordination::adopt(end);
        let (sink, _sink_peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let (state, _state_peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../weaver-types/tests/fixtures/state-enter.json"
        ))
        .unwrap();
        value["payload"]["body"]["payload"]["binding"]["ste100_scheme"] = serde_json::json!("");
        let base = serde_json::to_vec(&value).unwrap().len();
        let text = match case {
            "exact" => "x".repeat(MAX_ENVELOPE_BYTES - base),
            "over" => "x".repeat(MAX_ENVELOPE_BYTES - base + 1),
            "escaped" => "\n".repeat(MAX_ENVELOPE_BYTES / 2),
            _ => String::new(),
        };
        value["payload"]["body"]["payload"]["binding"]["ste100_scheme"] = serde_json::json!(text);
        let envelope: EnterEnvelope = serde_json::from_value(value).unwrap();
        let result = owner.send_replacement_enter(
            &envelope,
            sink.as_fd(),
            (case != "missing-state").then(|| state.as_fd()),
        );
        let mut bytes = vec![0; MAX_ENVELOPE_BYTES + 1];
        if case == "exact" {
            result.unwrap();
            let count = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
            assert_eq!(count, MAX_ENVELOPE_BYTES);
            assert_eq!(EnterEnvelope::decode(&bytes[..count]).unwrap(), envelope);
        } else {
            if case == "missing-state" {
                assert_eq!(result, Err(ChannelFault::Undecodable));
            } else {
                assert_eq!(
                    result,
                    Err(ChannelFault::Truncated {
                        bound: MAX_ENVELOPE_BYTES
                    })
                );
            }
            assert_eq!(
                recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN),
                "no prefix or handles sent for {case}"
            );
        }
    }
}

#[test]
fn bounded_replacement_enter_reports_oversize_without_transferring_descriptors() {
    use crate::serving_load::window::LoadWindow;
    use std::os::fd::AsFd;
    use weaver_types::state_flow::enter::{
        EnterBinding, EnterContent, EnterDirective, EnterEnvelope,
    };
    let (end, far) = nix::sys::socket::socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let owner = Coordination::adopt(end);
    let (sink, _) = std::os::unix::net::UnixStream::pair().unwrap();
    let (state, _) = std::os::unix::net::UnixStream::pair().unwrap();
    let window = LoadWindow::begin(Duration::from_secs(2)).unwrap();
    let mut envelope = EnterEnvelope::decode(include_bytes!(
        "../../../weaver-types/tests/fixtures/state-enter.json"
    ))
    .unwrap();
    let EnterContent::Directive(EnterDirective::Enter { payload }) = &mut envelope.payload;
    let EnterBinding::Serving {
        enter_window,
        ste100_scheme,
        ..
    } = &mut payload.binding
    else {
        panic!()
    };
    *enter_window = window.original();
    *ste100_scheme = "x".repeat(MAX_ENVELOPE_BYTES);
    assert_eq!(
        owner.send_replacement_enter_within(&envelope, sink.as_fd(), state.as_fd(), &window),
        Err(ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES
        })
    );
    assert_eq!(
        nix::sys::socket::recv(far.as_raw_fd(), &mut [0; 1], MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
}

#[test]
fn original_enter_window_bounds_dial_send_pressure_and_already_readable_late_ready() {
    use crate::serving_load::window::LoadWindow;
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;
    let dir = scratch("enter-window");
    let window = LoadWindow::begin(Duration::from_millis(20)).unwrap();
    let before = Instant::now();
    assert!(matches!(
        dial_for_enter(
            &dir.join("absent.sock"),
            nix::unistd::getuid().as_raw(),
            &dir.0,
            &window
        ),
        Err(ChannelFault::Unanswered)
    ));
    assert!(before.elapsed() < Duration::from_millis(500));
    let (end, far) = nix::sys::socket::socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let owner = Coordination::adopt(end);
    let (sink, _sink_reader) = UnixStream::pair().unwrap();
    let (state, _state_reader) = UnixStream::pair().unwrap();
    let filler = vec![b'x'; 1024];
    loop {
        match nix::sys::socket::send(
            owner.fd.as_raw_fd(),
            &filler,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(_) => {}
            Err(nix::errno::Errno::EAGAIN) => break,
            result => panic!("pressure fixture: {result:?}"),
        }
    }
    let window = LoadWindow::begin(Duration::from_millis(20)).unwrap();
    let mut envelope: weaver_types::state_flow::enter::EnterEnvelope =
        weaver_types::state_flow::enter::EnterEnvelope::decode(include_bytes!(
            "../../../weaver-types/tests/fixtures/state-enter.json"
        ))
        .unwrap();
    let weaver_types::state_flow::enter::EnterContent::Directive(
        weaver_types::state_flow::enter::EnterDirective::Enter { payload },
    ) = &mut envelope.payload;
    let weaver_types::state_flow::enter::EnterBinding::Serving { enter_window, .. } =
        &mut payload.binding
    else {
        panic!()
    };
    *enter_window = window.original();
    assert_eq!(
        owner.send_replacement_enter_within(&envelope, sink.as_fd(), state.as_fd(), &window),
        Err(ChannelFault::Unanswered)
    );
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    loop {
        match nix::sys::socket::recv(far.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(n) => assert_eq!(&bytes[..n], filler, "no Enter transferred after expiry"),
            Err(nix::errno::Errno::EAGAIN) => break,
            result => panic!("drain: {result:?}"),
        }
    }
    let ready = OrganEnvelope {
        exchange: envelope.exchange,
        position: Position::Close,
        payload: Payload::Answer(weaver_types::LifecycleAnswer::Ready),
    };
    Coordination::adopt(far).send(&ready).unwrap();
    assert_eq!(
        owner.recv_enter_answer(&window),
        Err(ChannelFault::Unanswered),
        "readiness cannot admit late Ready"
    );
}

#[test]
fn original_enter_window_and_both_descriptors_cross_once_before_timely_ready() {
    use crate::serving_load::window::LoadWindow;
    use std::os::fd::{AsFd, FromRawFd};
    use std::os::unix::net::UnixStream;
    use weaver_types::state_flow::enter::{
        EnterBinding, EnterContent, EnterDirective, EnterEnvelope,
    };
    let (end, far) = nix::sys::socket::socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let owner = Coordination::adopt(end);
    let (sink, _reader) = UnixStream::pair().unwrap();
    let (state, _member) = UnixStream::pair().unwrap();
    let window = LoadWindow::begin(Duration::from_secs(2)).unwrap();
    let mut envelope = EnterEnvelope::decode(include_bytes!(
        "../../../weaver-types/tests/fixtures/state-enter.json"
    ))
    .unwrap();
    assert_eq!(
        owner.send_replacement_enter_within(&envelope, sink.as_fd(), state.as_fd(), &window),
        Err(ChannelFault::Undecodable),
        "the producer cannot substitute a different wire window"
    );
    let EnterContent::Directive(EnterDirective::Enter { payload }) = &mut envelope.payload;
    let EnterBinding::Serving { enter_window, .. } = &mut payload.binding else {
        panic!()
    };
    *enter_window = window.original();
    owner
        .send_replacement_enter_within(&envelope, sink.as_fd(), state.as_fd(), &window)
        .unwrap();
    let mut buffer = vec![0; MAX_ENVELOPE_BYTES];
    let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
    let mut controls = nix::cmsg_space!([i32; 2]);
    let received = nix::sys::socket::recvmsg::<()>(
        far.as_raw_fd(),
        &mut slices,
        Some(&mut controls),
        MsgFlags::MSG_CMSG_CLOEXEC,
    )
    .unwrap();
    let count = received.bytes;
    let mut gifts = Vec::new();
    for control in received.cmsgs().unwrap() {
        if let nix::sys::socket::ControlMessageOwned::ScmRights(fds) = control {
            for fd in fds {
                // SAFETY: each returned SCM_RIGHTS descriptor has one owner here.
                gifts.push(unsafe { OwnedFd::from_raw_fd(fd) });
            }
        }
    }
    assert_eq!(gifts.len(), 2);
    assert_eq!(EnterEnvelope::decode(&buffer[..count]).unwrap(), envelope);
    let mut byte = [0];
    assert_eq!(
        nix::sys::socket::recv(far.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "no duplicated Enter"
    );
    let ready = OrganEnvelope {
        exchange: envelope.exchange,
        position: Position::Close,
        payload: Payload::Answer(weaver_types::LifecycleAnswer::Ready),
    };
    Coordination::adopt(far).send(&ready).unwrap();
    assert_eq!(owner.recv_enter_answer(&window).unwrap(), ready);
}

#[test]
fn original_enter_window_rechecks_after_actual_receipt_and_delayed_decode() {
    let (end, far) = nix::sys::socket::socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let owner = Coordination::adopt(end);
    let ready = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 1,
        },
        position: Position::Close,
        payload: Payload::Answer(weaver_types::LifecycleAnswer::Ready),
    };
    Coordination::adopt(far).send(&ready).unwrap();
    let window = crate::serving_load::window::LoadWindow::begin(Duration::from_millis(20)).unwrap();
    let decoded = std::cell::Cell::new(false);
    let result = owner.recv_enter_answer_with(&window, |bytes| {
        let answer = serde_json::from_slice(bytes).map_err(|_| ChannelFault::Undecodable)?;
        decoded.set(true);
        // The real packet was received in time. A paused decoder spends the same
        // real clock budget; readiness must not authorize the later Ready.
        std::thread::sleep(Duration::from_millis(30));
        Ok(answer)
    });
    assert!(decoded.get());
    assert_eq!(result, Err(ChannelFault::Unanswered));
}
