//! A generation send hold keeps the same socket's health and receive owner live.
use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, recv, socketpair};

fn pair() -> (DecodePort, OwnedFd) {
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (DecodePort::from_owned(end).unwrap(), peer)
}
fn append(large: bool) -> TokenDirective {
    TokenDirective::AppendAndGenerate {
        turn: weaver_types::TurnKey("health-turn".into()),
        delta: if large {
            vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: "x".repeat(MAX_ENVELOPE_BYTES * 2),
                }],
            }]
        } else {
            vec![]
        },
        tool_schemas: vec![],
    }
}
fn received(peer: &OwnedFd) -> Option<Vec<u8>> {
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    match recv(
        peer.as_raw_fd(),
        &mut bytes,
        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC,
    ) {
        Ok(count) => {
            assert!(count <= bytes.len());
            bytes.truncate(count);
            Some(bytes)
        }
        Err(Errno::EAGAIN) => None,
        result => panic!("unexpected peer receive: {result:?}"),
    }
}

#[test]
fn generation_send_hold_keeps_reads_and_owed_packet_without_write_or_interrupt_spin() {
    let (mut port, peer) = pair();
    let input = append(false);
    let expected = serde_json::to_vec(&input).unwrap();
    assert!(matches!(port.stage(input).unwrap(), Stage::Queued));
    assert_eq!(
        port.send_with(|_, _, _| Err(Errno::EINTR)),
        Ok(SendProgress::Pending)
    );
    assert!(port.needs_progress());
    let previous = port.replace_generation_send_hold(true);
    assert!(!previous);
    assert!(
        port.send_pending(),
        "the held directive still owes its original send"
    );
    assert!(port.generation_not_started());
    assert!(!port.interest().writable);
    assert!(port.interest().readable);
    assert_ne!(port.interest().events().bits() & libc::POLLRDHUP, 0);
    assert!(
        !port.needs_progress(),
        "an interrupted held send cannot spin local work"
    );
    assert_eq!(
        port.send_with(|_, _, _| panic!("held first byte reached send syscall")),
        Ok(SendProgress::Pending)
    );
    assert_eq!(received(&peer), None);
    let raw = serde_json::to_vec(&TokenAnswer::AtRest).unwrap();
    assert_eq!(
        send(
            peer.as_raw_fd(),
            &raw,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        ),
        Ok(raw.len())
    );
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.received_wire.as_deref(), Some(raw.as_slice()));
    assert!(matches!(
        port.take_completion().unwrap(),
        Some(DecodeReply::Answer(TokenAnswer::AtRest))
    ));
    assert!(port.replace_generation_send_hold(previous));
    assert!(port.interest().writable);
    assert!(
        port.needs_progress(),
        "original interrupted send resumes after the scoped hold"
    );
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
    assert_eq!(received(&peer), Some(expected));
    assert!(!port.send_pending());
}

#[test]
fn generation_send_hold_preserves_actual_final_packet_and_half_close_failure() {
    let (mut port, peer) = pair();
    assert!(matches!(port.stage(append(false)).unwrap(), Stage::Queued));
    port.replace_generation_send_hold(true);
    let raw = serde_json::to_vec(&TokenAnswer::AtRest).unwrap();
    assert_eq!(
        send(
            peer.as_raw_fd(),
            &raw,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        ),
        Ok(raw.len())
    );
    shutdown(peer.as_raw_fd(), Shutdown::Write).unwrap();
    let mut poll = libc::pollfd {
        fd: port.fd().as_raw_fd(),
        events: port.interest().events().bits(),
        revents: 0,
    };
    // SAFETY: the live stack pollfd borrows this port for the bounded syscall.
    assert_eq!(unsafe { libc::poll(&mut poll, 1, 0) }, 1);
    assert_ne!(poll.revents & libc::POLLRDHUP, 0);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert!(matches!(
        port.held(),
        Some(DecodeReply::Answer(TokenAnswer::AtRest))
    ));
    assert_eq!(port.received_wire.as_deref(), Some(raw.as_slice()));
    assert!(port.send_pending());
    assert!(port.generation_not_started());
    assert_eq!(
        port.try_send(),
        Err(failure.clone()),
        "standing failure precedes the send hold"
    );
    assert!(matches!(
        port.take_completion().unwrap(),
        Some(DecodeReply::Answer(TokenAnswer::AtRest))
    ));
    assert_eq!(port.failure(), Some(&failure));
    // Latching the transport failure shuts this endpoint down. The peer
    // receives EOF, never the retained outgoing generation packet.
    assert_eq!(received(&peer), Some(vec![]));
}

#[test]
fn generation_send_hold_never_blocks_started_segments_or_cancel() {
    let (mut port, peer) = pair();
    let input = append(true);
    let expected = serde_json::to_vec(&input).unwrap();
    assert!(matches!(port.stage(input).unwrap(), Stage::Queued));
    assert_eq!(port.try_send(), Ok(SendProgress::Progress));
    let head: SegmentPreamble = serde_json::from_slice(&received(&peer).unwrap()).unwrap();
    assert!(!port.generation_not_started());
    port.replace_generation_send_hold(true);
    let mut actual = vec![];
    for _ in 0..head.segments {
        assert!(port.interest().writable);
        assert_ne!(port.try_send().unwrap(), SendProgress::Pending);
        actual.extend(received(&peer).unwrap());
    }
    assert_eq!(actual, expected);
    assert!(!port.send_pending());
    let cancel = TokenDirective::Cancel {
        turn: weaver_types::TurnKey("health-turn".into()),
    };
    let expected = serde_json::to_vec(&cancel).unwrap();
    assert!(matches!(port.stage(cancel).unwrap(), Stage::Queued));
    assert!(port.interest().writable);
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
    assert_eq!(received(&peer), Some(expected));
}
