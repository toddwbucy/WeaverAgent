//! Establish pressure on the real worker reply socket without another writer.
use super::*;

const COUNT: u64 = 1024;
const FIRST: u64 = 10_000;

pub(super) fn fill(peer: &OwnedFd, root: &std::path::Path, end: Instant) {
    for ordinal in FIRST..FIRST + COUNT {
        let bytes = serde_json::to_vec(&OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: Opener::Admin,
                ordinal,
            },
            position: Position::Open,
            payload: Payload::Directive(LifecycleDirective::Observe),
        })
        .unwrap();
        loop {
            match nix::sys::socket::send(
                peer.as_raw_fd(),
                &bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(n) => {
                    assert_eq!(n, bytes.len());
                    break;
                }
                Err(nix::errno::Errno::EAGAIN) => {
                    assert!(
                        Instant::now() < end,
                        "worker stopped receiving under reply pressure"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => panic!("pressure request send: {other:?}"),
            }
        }
    }
    loop {
        let mut outstanding = 0_i32;
        // SAFETY: TIOCOUTQ writes one integer. On this Unix socket the sender
        // retains its allocation until the receiver consumes each packet.
        assert_eq!(
            unsafe { nix::libc::ioctl(peer.as_raw_fd(), nix::libc::TIOCOUTQ, &mut outstanding) },
            0
        );
        if outstanding == 0 {
            break;
        }
        assert!(Instant::now() < end, "worker left pressure requests unread");
        std::thread::sleep(Duration::from_millis(1));
    }
    let probe = dial(&root.join("admin.sock"));
    let mut queued = count(peer);
    loop {
        // Each completed probe is another real management cycle with the
        // pressured peer in the same poll set. A stable partial queue then
        // proves that it cannot flush while the client consumes no replies.
        transition(&probe, 980, end);
        transition(&probe, 981, end);
        let after = count(peer);
        if after == queued {
            break;
        }
        queued = after;
        assert!(Instant::now() < end);
    }
    assert!(
        queued > 0 && queued < COUNT,
        "all requests were consumed but replies must remain pending"
    );
    eprintln!(
        "Diagnostic reply pressure: {COUNT} requests consumed, {queued} replies queued, no reply consumed"
    );
}

fn count(peer: &OwnedFd) -> u64 {
    peek_offset(peer, 0);
    let mut queued = 0;
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    loop {
        match recv(
            peer.as_raw_fd(),
            &mut bytes,
            MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
        ) {
            Ok(n) => {
                assert!(n > 0);
                let answer = ObservationEnvelope::decode(&bytes[..n]).unwrap();
                assert_eq!(answer.exchange.ordinal, FIRST + queued);
                assert!(matches!(
                    answer.payload,
                    ObservationContent::Answer(ObservationAnswer::InTransition { .. })
                ));
                queued += 1;
            }
            Err(nix::errno::Errno::EAGAIN) => break,
            other => panic!("pressure peek: {other:?}"),
        }
    }
    peek_offset(peer, -1);
    queued
}

fn peek_offset(peer: &OwnedFd, value: i32) {
    // SAFETY: Linux SO_PEEK_OFF reads one integer and does not consume packets.
    assert_eq!(
        unsafe {
            nix::libc::setsockopt(
                peer.as_raw_fd(),
                nix::libc::SOL_SOCKET,
                nix::libc::SO_PEEK_OFF,
                (&value as *const i32).cast(),
                std::mem::size_of::<i32>() as nix::libc::socklen_t,
            )
        },
        0
    );
}

pub(super) fn drain(peer: &OwnedFd, source: &EnterEnvelope, end: Instant) {
    let mut seen = std::collections::BTreeSet::new();
    let mut enter = false;
    for _ in 0..=COUNT {
        let bytes = receive(peer, end);
        let reply: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reply.position, Position::Close);
        assert_eq!(reply.exchange.opener, Opener::Admin);
        if reply.exchange == source.exchange {
            assert!(!enter);
            assert_eq!(
                reply.payload,
                Payload::Refusal(LifecycleRefusal::NoResidency)
            );
            enter = true;
        } else {
            let reply = ObservationEnvelope::decode(&bytes).unwrap();
            assert!((FIRST..FIRST + COUNT).contains(&reply.exchange.ordinal));
            assert!(
                seen.insert(reply.exchange.ordinal),
                "duplicate pressure reply"
            );
            assert!(matches!(
                reply.payload,
                ObservationContent::Answer(ObservationAnswer::InTransition { .. })
            ));
        }
    }
    assert!(enter);
    assert_eq!(seen.len(), COUNT as usize);
}
