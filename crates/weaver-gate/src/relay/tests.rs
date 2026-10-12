use super::*;
use weaver_types::PeerIdentity;

/// A served connection over a socketpair, the accept bypassed: the
/// predicate's own tests live with the hook, and the gate denies its
/// own uid by construction, so relay tests build the admitted state
/// directly.
fn served_pair() -> (Served, UnixStream) {
    let (near, far) = UnixStream::pair().expect("pair");
    let served = Served::admit(Admitted {
        stream: near,
        peer: PeerIdentity {
            uid: 12345,
            gid: 12345,
            pid: 1,
        },
    })
    .expect("admit");
    (served, far)
}

/// **A quiesce closes every connection owed nothing and reads no more**,
/// per `weaver-gate-Spec` section 4 (draining): a connection with no open
/// exchange closes, its client reading the end; one owed a response stays,
/// reads nothing, opens no exchange for a line already in its residual or
/// sent after, delivers what it is owed, and then owes nothing.
/// Perturbation: keep the idle connection, or let a draining one frame its
/// residual, and this fails.
#[test]
fn a_quiesce_closes_what_is_owed_nothing_and_reads_no_more() {
    use std::io::Read as _;
    let (owed, mut owed_client) = served_pair();
    let (idle, mut idle_client) = served_pair();
    let mut relay = Relay::new();
    relay.served.push(owed);
    relay.served.push(idle);
    owed_client
        .write_all(b"the request\nthe next line\n")
        .expect("writes");
    let Framed::Opened(opened) = relay.read_one(0).expect("reads") else {
        panic!("the request frames");
    };

    relay.quiesce();

    assert_eq!(relay.served.len(), 1, "the idle connection closed");
    let mut got = [0u8; 8];
    assert_eq!(idle_client.read(&mut got).expect("the end"), 0);
    let draining = &mut relay.served[0];
    assert!(draining.owes(), "the open exchange is owed");
    assert!(
        !draining.wants_read(),
        "a draining connection is never read"
    );
    let mut ordinal = opened.exchange.ordinal;
    draining
        .on_response(&TurnFrame::carry(b"the answer"))
        .expect("routes");
    draining.on_writable().expect("drains");
    let n = owed_client.read(&mut [0u8; 32]).expect("the answer");
    assert_eq!(n, b"the answer\n".len());
    assert!(
        matches!(
            draining.frame_one(&mut ordinal).expect("scans"),
            Framed::Waiting
        ),
        "the residual's line never opens an exchange while draining"
    );
    assert!(!draining.owes(), "delivered, it owes nothing");
    assert!(!draining.wants_read(), "and is still never read");
}

/// **At the lower, a request never answered closes at once, and only a
/// response the harness sent is waited on** (`weaver-harness-gate-contract`
/// section 2: "every response the harness sent has been written"): the
/// channel is ordered, so an exchange still open when the `Lower` is read
/// will never be answered, and its connection closes unanswered, the
/// harness having recorded the request refused. Perturbation: keep the
/// open exchange, and its client never reads the end.
#[test]
fn at_the_lower_a_request_never_answered_closes_at_once() {
    use std::io::Read as _;
    let (open, mut client) = served_pair();
    let mut relay = Relay::new();
    relay.served.push(open);
    client.write_all(b"never answered\n").expect("writes");
    let Framed::Opened(_) = relay.read_one(0).expect("reads") else {
        panic!("the request frames");
    };
    relay.quiesce();
    assert_eq!(relay.served.len(), 1, "the quiesce keeps an open exchange");
    relay.lower();
    assert!(relay.served.is_empty(), "the lower closes it");
    assert_eq!(client.read(&mut [0u8; 8]).expect("the end"), 0);
    assert_eq!(relay.settle(false), Settled::Done { lost: vec![] });
}

/// **A response written but unread is waited on, and named lost at the
/// settle instant** (`weaver-gate-Spec` section 4): with the gate's send
/// buffer smaller than the response, the write leaves part of it owed; the
/// lower keeps the connection, settle is open inside the instant and names
/// the dialer past it. At default buffers a single response always fits
/// (a 64 KiB envelope against about 176 KiB the kernel takes unread), so
/// this is reachable only where the gate's send buffer is small; the test
/// shrinks it. Perturbation: settle while a response is unwritten inside
/// the instant, or name no dialer past it, and this fails.
#[test]
fn a_response_written_but_unread_is_named_lost_at_the_settle_instant() {
    use std::os::fd::AsFd as _;
    let (near, mut client) = UnixStream::pair().expect("pair");
    nix::sys::socket::setsockopt(&near.as_fd(), nix::sys::socket::sockopt::SndBuf, &4096)
        .expect("a small send buffer");
    let mut served = Served::admit(Admitted {
        stream: near,
        peer: PeerIdentity {
            uid: 12345,
            gid: 12345,
            pid: 1,
        },
    })
    .expect("admit");
    let mut ordinal = 0u64;
    client.write_all(b"the request\n").expect("writes");
    let Framed::Opened(_) = served.on_readable(&mut ordinal).expect("reads") else {
        panic!("the request frames");
    };
    let mut relay = Relay::new();
    relay.served.push(served);
    relay.quiesce();
    relay.served[0]
        .on_response(&TurnFrame::carry(&vec![b'x'; 60 * 1024]))
        .expect("routes");
    relay.served[0].on_writable().expect("writes what fits");
    assert!(
        relay.served[0].wants_write(),
        "part of the response is unwritten"
    );
    relay.lower();
    assert_eq!(
        relay.served.len(),
        1,
        "a response the harness sent is waited on"
    );
    assert_eq!(relay.settle(false), Settled::Open);
    assert_eq!(relay.settle(true), Settled::Done { lost: vec![12345] });
    drop(client);
}

/// **The lower's flush stops at the settle instant** (Codex on #114): the
/// harness reading the channel slowly, or not at all, cannot hold the
/// gate past the instant; the frames still waiting are dropped and their
/// dialers named, as any delivery the bound outruns. Perturbation: flush
/// without the instant, and the call never returns while the channel is
/// full.
#[test]
fn the_lowers_flush_stops_at_the_settle_instant() {
    use std::time::{Duration, Instant};
    let (near, far) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::SeqPacket,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .expect("pair");
    let gate = crate::channel::from_owned(near);
    let _unread_harness = crate::channel::from_owned(far);
    let frame = |ordinal| OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Frame(TurnFrame::carry_from(&[b'x'; 30 * 1024], 777)),
    };
    // Fill the channel the harness is not reading.
    let mut ordinal = 0;
    while gate.try_send(&frame(ordinal)).expect("sends") {
        ordinal += 1;
    }
    let mut relay = Relay::new();
    relay.pending.push_back(frame(ordinal + 1));
    relay.pending.push_back(frame(ordinal + 2));
    let (done, finished) = std::sync::mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        let lost = relay
            .flush_pending_until(&gate, Instant::now() + Duration::from_millis(200))
            .expect("a full channel is no fault");
        done.send((lost, relay.pending.len())).ok();
        drop(gate);
    });
    let (lost, left) = finished
        .recv_timeout(Duration::from_secs(3))
        .expect("the flush returns at the instant");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        lost,
        vec![777, 777],
        "each frame left is named by its dialer"
    );
    assert_eq!(left, 0, "and dropped");
}

/// **The envelopes waiting on the channel are flushed in order**, so a
/// quiesce's or a forced lower's answer follows every frame this crate
/// admitted (`weaver-harness-gate-contract` section 2). Perturbation:
/// flush only the first, and the second never arrives before the end.
#[test]
fn the_pending_envelopes_flush_in_order() {
    let (near, far) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::SeqPacket,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .expect("pair");
    let (gate, harness) = (
        crate::channel::from_owned(near),
        crate::channel::from_owned(far),
    );
    let mut relay = Relay::new();
    for ordinal in [7, 8] {
        relay.pending.push_back(OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal,
            },
            position: Position::Open,
            payload: Payload::Frame(TurnFrame::carry(b"admitted")),
        });
    }
    relay.flush_pending(&gate).expect("flushes");
    assert!(relay.pending.is_empty());
    for ordinal in [7, 8] {
        assert_eq!(harness.recv().expect("arrives").exchange.ordinal, ordinal);
    }
}

/// **Two lines in one write open two exchanges in order, one at a
/// time.** The scan bounds frames at the delimiter, the cap holds the
/// second line in the residual while the first exchange is open, and
/// the scan resumes when the response returns. The lines are not JSON,
/// because the relay does not care.
#[test]
fn two_lines_frame_in_order_one_exchange_at_a_time() {
    let (mut served, mut client) = served_pair();
    let mut ordinal = 0u64;
    client
        .write_all(b"first line, plain text\nsecond line, also plain\n")
        .expect("client writes");

    let Framed::Opened(first) = served.on_readable(&mut ordinal).expect("reads") else {
        panic!("the first line frames");
    };
    assert_eq!(first.exchange.ordinal, 1);
    let Payload::Frame(frame) = &first.payload else {
        panic!("a frame");
    };
    assert_eq!(
        frame.octets().expect("canonical"),
        b"first line, plain text",
        "the frame carries the line's octets, unread"
    );
    // The dialer rides the frame, the uid the accept read, per
    // `weaver-harness-gate-contract` section 2. Perturbation: carry the
    // frame without it and every seeding line refuses at the harness,
    // whoever dialed.
    assert_eq!(frame.dialer, Some(12345), "the frame names its dialer");

    // The cap: the second line stands in the residual and no second
    // exchange opens while the first is unanswered.
    assert!(!served.wants_read(), "an open exchange leaves the read set");
    assert!(matches!(
        served.frame_one(&mut ordinal).expect("scans"),
        Framed::Waiting
    ));

    // The response returns and queues, and the scan still waits: the
    // cap admits the next line only after the drain, the outbound
    // buffer holding at most one response.
    served
        .on_response(&TurnFrame::carry(b"the first answer"))
        .expect("routes");
    assert!(matches!(
        served.frame_one(&mut ordinal).expect("scans"),
        Framed::Waiting
    ));

    // The queued response drains to the client, delimiter appended,
    // and only then does the residual's line frame in its turn.
    served.on_writable().expect("drains");
    let mut got = [0u8; 64];
    use std::io::Read as _;
    let n = client.read(&mut got).expect("client reads");
    assert_eq!(&got[..n], b"the first answer\n");
    let Framed::Opened(second) = served.frame_one(&mut ordinal).expect("scans") else {
        panic!("the residual's line frames after the drain");
    };
    assert_eq!(second.exchange.ordinal, 2);
}

/// **The response routes by the exchange's identity**, two clients
/// speaking at once and each answered on its own connection, in
/// whatever order the answers return.
#[test]
fn responses_route_by_the_exchange_identity() {
    let (served_a, mut client_a) = served_pair();
    let (served_b, mut client_b) = served_pair();
    let mut relay = Relay::new();
    relay.served.push(served_a);
    relay.served.push(served_b);

    client_a.write_all(b"from a\n").expect("a writes");
    client_b.write_all(b"from b\n").expect("b writes");
    let mut next = 0u64;
    let Framed::Opened(env_a) = relay.served[0].on_readable(&mut next).expect("a") else {
        panic!("a frames");
    };
    let Framed::Opened(env_b) = relay.served[1].on_readable(&mut next).expect("b") else {
        panic!("b frames");
    };

    // Answered in reverse order, routed by identity alone.
    relay
        .route(env_b.exchange.ordinal)
        .expect("b is owed")
        .on_response(&TurnFrame::carry(b"answer b"))
        .expect("routes");
    relay
        .route(env_a.exchange.ordinal)
        .expect("a is owed")
        .on_response(&TurnFrame::carry(b"answer a"))
        .expect("routes");
    for served in &mut relay.served {
        served.on_writable().expect("drains");
    }

    use std::io::Read as _;
    let mut got = [0u8; 32];
    let n = client_a.read(&mut got).expect("a reads");
    assert_eq!(&got[..n], b"answer a\n", "a receives a's answer");
    let n = client_b.read(&mut got).expect("b reads");
    assert_eq!(&got[..n], b"answer b\n", "b receives b's answer");
}

/// **A half-closed peer is a finished speaker, not a departed one.**
/// The read side ends, the line already sent still frames, the response
/// still delivers, and the connection is spent only when nothing
/// remains: the shape every piped client takes.
#[test]
fn a_half_closed_peer_still_receives_its_answer() {
    let (mut served, mut client) = served_pair();
    let mut ordinal = 0u64;
    client
        .write_all(b"one line, then silence\n")
        .expect("writes");
    client
        .shutdown(std::net::Shutdown::Write)
        .expect("half-close");

    let Framed::Opened(envelope) = served.on_readable(&mut ordinal).expect("reads") else {
        panic!("the line frames despite the half-close");
    };
    let Payload::Frame(frame) = &envelope.payload else {
        panic!("a frame");
    };
    assert_eq!(
        frame.octets().expect("canonical"),
        b"one line, then silence"
    );
    assert!(!served.spent(), "an open exchange is not spent");

    served
        .on_response(&TurnFrame::carry(b"the answer"))
        .expect("routes");
    served.on_writable().expect("drains");
    use std::io::Read as _;
    let mut got = [0u8; 32];
    let n = client.read(&mut got).expect("client reads");
    assert_eq!(&got[..n], b"the answer\n");

    // The end of the read side is its own wake: the poll re-wakes on
    // the pending close, the read records it, and with nothing left
    // the connection is spent.
    assert!(matches!(
        served.on_readable(&mut ordinal).expect("reads the close"),
        Framed::Waiting
    ));
    assert!(
        served.spent(),
        "nothing remains and the connection is spent"
    );
}

/// **The bound is inclusive, and one octet more closes the
/// connection.** A line of exactly the bound followed by its delimiter
/// opens its exchange, and a connection fed one undelimited octet past
/// the bound is gone with no exchange opened.
#[test]
fn the_bound_is_inclusive_and_one_more_closes() {
    let (mut served, mut client) = served_pair();
    let mut ordinal = 0u64;

    // Exactly the bound, then the delimiter: legal.
    let exact = vec![b'x'; LINE_BOUND];
    client.write_all(&exact).expect("writes");
    client.write_all(b"\n").expect("delimiter");
    let mut framed = false;
    for _ in 0..=(LINE_BOUND / 4096 + 2) {
        match served.on_readable(&mut ordinal).expect("reads") {
            Framed::Opened(envelope) => {
                let Payload::Frame(frame) = &envelope.payload else {
                    panic!("a frame");
                };
                assert_eq!(frame.octets().expect("canonical").len(), LINE_BOUND);
                framed = true;
                break;
            }
            Framed::Waiting => continue,
        }
    }
    assert!(framed, "a bound-exact line frames within its reads");

    // One undelimited octet past the bound: the framing layer closes
    // the connection, no exchange opened.
    let (mut served, mut client) = served_pair();
    let over = vec![b'x'; LINE_BOUND + 1];
    client.write_all(&over).expect("writes");
    let mut outcome = Ok(Framed::Waiting);
    for _ in 0..=(LINE_BOUND / 4096 + 1) {
        outcome = served.on_readable(&mut ordinal);
        if outcome.is_err() {
            break;
        }
    }
    assert_eq!(
        outcome.err(),
        Some(Gone::PastTheBound),
        "past the bound is below any turn"
    );
}
