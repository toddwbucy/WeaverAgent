use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, socket, socketpair};

fn pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("SEQPACKET pair")
}

fn packet(tag: Tag, id: u64, total: u64, offset: u64, data: &[u8]) -> Vec<u8> {
    let mut bytes = vec![1, tag as u8];
    bytes.extend_from_slice(&id.to_be_bytes());
    bytes.extend_from_slice(&total.to_be_bytes());
    bytes.extend_from_slice(&offset.to_be_bytes());
    bytes.extend_from_slice(data);
    bytes
}

fn raw_send(end: &OwnedFd, bytes: &[u8]) {
    assert_eq!(
        send(
            end.as_raw_fd(),
            bytes,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        )
        .unwrap(),
        bytes.len()
    );
}

fn raw_receive(end: &OwnedFd) -> Vec<u8> {
    let mut bytes = vec![0; DATAGRAM_BYTES + 1];
    let count = recv(end.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    bytes.truncate(count);
    bytes
}

fn assert_retired(channel: &mut Channel) {
    assert_eq!(channel.try_receive(), Err(Fault::Failed));
    assert_eq!(channel.try_send(), Err(Fault::Failed));
    assert_eq!(
        channel.begin_send(Tag::Request, 9, b"later".to_vec()),
        Err(Fault::Failed)
    );
}

#[test]
fn exact_header_and_all_tags_use_the_declared_wire_and_id_domain() {
    assert_eq!(DATAGRAM_BYTES, 65_536);
    assert_eq!(HEADER_BYTES, 26);
    for (tag, value, id) in [
        (Tag::Bootstrap, 1, 0),
        (Tag::Request, 2, u64::MAX),
        (Tag::Reply, 3, 1),
        (Tag::Ready, 4, 0),
        (Tag::AdmissionRefused, 5, 0),
    ] {
        assert_eq!(tag as u8, value);
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(writer, 3).unwrap();
        channel.begin_send(tag, id, b"abc".to_vec()).unwrap();
        assert_eq!(channel.try_send(), Ok(SendProgress::Complete));
        assert_eq!(raw_receive(&reader), packet(tag, id, 3, 0, b"abc"));
        assert!(!channel.has_pending_send());
        assert_eq!(channel.try_send(), Ok(SendProgress::Complete));
        let mut byte = [0];
        assert_eq!(
            recv(reader.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
            Err(Errno::EAGAIN)
        );
    }
}

#[test]
fn exact_datagram_and_logical_bounds_return_only_one_whole_message() {
    for length in [1, 65_510, 65_511, 2 * 65_510 + 17] {
        let payload: Vec<_> = (0..length).map(|i| (i % 251) as u8).collect();
        let (writer, reader) = pair();
        let mut sender = Channel::from_owned(writer, length).unwrap();
        let mut receiver = Channel::from_owned(reader, length).unwrap();
        assert_eq!(receiver.try_receive(), Ok(Receive::Pending));
        sender.begin_send(Tag::Reply, 37, payload.clone()).unwrap();
        let fragments = length.div_ceil(65_510);
        for index in 0..fragments {
            let sent = sender.try_send().unwrap();
            if index + 1 == fragments {
                assert_eq!(sent, SendProgress::Complete);
                assert_eq!(
                    receiver.try_receive(),
                    Ok(Receive::Complete(Message {
                        tag: Tag::Reply,
                        id: 37,
                        payload: payload.clone(),
                    }))
                );
                assert!(!receiver.has_partial_receive());
            } else {
                assert_eq!(sent, SendProgress::Progress);
                assert_eq!(receiver.try_receive(), Ok(Receive::Fragment));
                assert!(receiver.has_partial_receive());
            }
            assert_eq!(receiver.try_receive(), Ok(Receive::Pending));
        }
        sender.begin_send(Tag::Reply, 38, vec![b'z']).unwrap();
        assert_eq!(sender.try_send(), Ok(SendProgress::Complete));
        assert_eq!(
            receiver.try_receive(),
            Ok(Receive::Complete(Message {
                tag: Tag::Reply,
                id: 38,
                payload: vec![b'z'],
            }))
        );
    }
}

#[test]
fn one_call_never_sends_or_consumes_a_second_datagram() {
    let (writer, reader) = pair();
    let mut sender = Channel::from_owned(writer, 65_511).unwrap();
    sender
        .begin_send(Tag::Request, 1, vec![b'x'; 65_511])
        .unwrap();
    assert_eq!(sender.try_send(), Ok(SendProgress::Progress));
    let first = raw_receive(&reader);
    assert_eq!(first.len(), 65_536);
    assert_eq!(&first[..26], &packet(Tag::Request, 1, 65_511, 0, &[]));
    let mut byte = [0];
    assert_eq!(
        recv(reader.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
        Err(Errno::EAGAIN)
    );
    assert_eq!(sender.try_send(), Ok(SendProgress::Complete));
    assert_eq!(
        raw_receive(&reader),
        packet(Tag::Request, 1, 65_511, 65_510, b"x")
    );

    let (writer, reader) = pair();
    let mut receiver = Channel::from_owned(reader, 6).unwrap();
    raw_send(&writer, &packet(Tag::Reply, 1, 6, 0, b"abc"));
    raw_send(&writer, &packet(Tag::Reply, 1, 6, 3, b"def"));
    raw_send(&writer, &packet(Tag::Reply, 2, 1, 0, b"z"));
    assert_eq!(receiver.try_receive(), Ok(Receive::Fragment));
    assert_eq!(receiver.partial.as_ref().unwrap().bytes, b"abc");
    assert_eq!(
        receiver.try_receive(),
        Ok(Receive::Complete(Message {
            tag: Tag::Reply,
            id: 1,
            payload: b"abcdef".to_vec()
        }))
    );
    assert_eq!(
        receiver.try_receive(),
        Ok(Receive::Complete(Message {
            tag: Tag::Reply,
            id: 2,
            payload: b"z".to_vec()
        }))
    );
}

#[test]
fn malformed_first_fragments_refuse_before_reserving_a_payload() {
    let valid = packet(Tag::Reply, 1, 3, 0, b"abc");
    let mut bad_version = valid.clone();
    bad_version[0] = 2;
    let mut bad_tag = valid;
    bad_tag[1] = 6;
    for bytes in [
        vec![1; 25],
        packet(Tag::Reply, 1, 1, 0, b""),
        bad_version,
        bad_tag,
        packet(Tag::Bootstrap, 1, 3, 0, b"abc"),
        packet(Tag::Ready, 1, 3, 0, b"abc"),
        packet(Tag::AdmissionRefused, 1, 3, 0, b"abc"),
        packet(Tag::Request, 0, 3, 0, b"abc"),
        packet(Tag::Reply, 0, 3, 0, b"abc"),
        packet(Tag::Reply, 1, 0, 0, b"abc"),
        packet(Tag::Reply, 1, 9, 0, b"abc"),
        packet(Tag::Reply, 1, u64::MAX, 0, b"abc"),
        packet(Tag::Reply, 1, 2, 0, b"abc"),
        packet(Tag::Reply, 1, 4, 1, b"abc"),
        packet(Tag::Reply, 1, 4, u64::MAX, b"abc"),
    ] {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, 8).unwrap();
        raw_send(&writer, &bytes);
        assert!(
            matches!(channel.try_receive(), Err(Fault::Framing(_))),
            "{bytes:?}"
        );
        assert!(
            channel.partial.is_none(),
            "untrusted total reserved a payload"
        );
        assert_retired(&mut channel);
    }
}

#[test]
fn continuation_faults_retain_the_exact_partial_message_and_retire_both_directions() {
    for continuation in [
        packet(Tag::Reply, 2, 6, 3, b"def"),
        packet(Tag::Request, 1, 6, 3, b"def"),
        packet(Tag::Reply, 1, 7, 3, b"def"),
        packet(Tag::Reply, 1, 6, 2, b"def"),
        packet(Tag::Reply, 1, 6, 4, b"de"),
        packet(Tag::Reply, 1, 6, 0, b"abc"),
        packet(Tag::Reply, 1, 6, 3, b"defg"),
    ] {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, 8).unwrap();
        raw_send(&writer, &packet(Tag::Reply, 1, 6, 0, b"abc"));
        assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
        channel
            .begin_send(Tag::Request, 9, b"queued".to_vec())
            .unwrap();
        raw_send(&writer, &continuation);
        assert!(matches!(channel.try_receive(), Err(Fault::Framing(_))));
        let held = channel.partial.as_ref().unwrap();
        assert_eq!((held.tag, held.id, held.total), (Tag::Reply, 1, 6));
        assert_eq!(held.bytes, b"abc");
        assert_eq!(channel.outbound.as_ref().unwrap().bytes, b"queued");
        assert!(channel.has_partial_receive() && channel.has_pending_send());
        assert_retired(&mut channel);
        assert_eq!(raw_receive(&writer), b"");
    }
}

#[test]
fn a_truncated_datagram_cannot_complete_from_its_copied_prefix() {
    let (writer, reader) = pair();
    let mut channel = Channel::from_owned(reader, 65_510).unwrap();
    let mut bytes = packet(Tag::Reply, 1, 65_510, 0, &vec![b'x'; 65_510]);
    bytes.push(b'y');
    raw_send(&writer, &bytes);
    assert_eq!(
        channel.try_receive(),
        Err(Fault::Framing("truncated datagram"))
    );
    assert!(channel.partial.is_none());
    assert_retired(&mut channel);
}

#[test]
fn eof_retains_a_partial_but_a_complete_reply_preceding_close_is_delivered_once() {
    for partial in [false, true] {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, 8).unwrap();
        if partial {
            raw_send(&writer, &packet(Tag::Reply, 1, 6, 0, b"abc"));
            assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
        }
        drop(writer);
        assert_eq!(channel.try_receive(), Err(Fault::Closed));
        assert_eq!(channel.has_partial_receive(), partial);
        if partial {
            assert_eq!(channel.partial.as_ref().unwrap().bytes, b"abc");
        }
        assert_retired(&mut channel);
    }
    let (writer, reader) = pair();
    let mut channel = Channel::from_owned(reader, 8).unwrap();
    raw_send(&writer, &packet(Tag::Reply, 1, 6, 0, b"abc"));
    raw_send(&writer, &packet(Tag::Reply, 1, 6, 3, b"def"));
    drop(writer);
    assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
    assert_eq!(
        channel.try_receive(),
        Ok(Receive::Complete(Message {
            tag: Tag::Reply,
            id: 1,
            payload: b"abcdef".to_vec()
        }))
    );
    assert_eq!(channel.try_receive(), Err(Fault::Closed));
    assert_retired(&mut channel);
}

#[test]
fn an_empty_live_packet_is_framing_but_full_and_half_close_are_closed() {
    let (writer, reader) = pair();
    let mut channel = Channel::from_owned(reader, 8).unwrap();
    raw_send(&writer, &[]);
    assert_eq!(
        channel.try_receive(),
        Err(Fault::Framing("empty datagram from live peer"))
    );
    assert_retired(&mut channel);

    for half in [false, true] {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, 8).unwrap();
        if half {
            shutdown(writer.as_raw_fd(), Shutdown::Write).unwrap();
        } else {
            shutdown(writer.as_raw_fd(), Shutdown::Both).unwrap();
        }
        assert_eq!(channel.try_receive(), Err(Fault::Closed));
        assert_retired(&mut channel);
    }
}

#[test]
fn an_interrupted_zero_read_probe_retains_the_consumed_observation() {
    for terminal in [0, nix::libc::POLLRDHUP] {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, 8).unwrap();
        channel
            .begin_send(Tag::Request, 1, b"request".to_vec())
            .unwrap();
        raw_send(&writer, &[]);
        assert_eq!(
            channel.receive_with_status(recv, |_| Err(Errno::EINTR)),
            Ok(Receive::Pending)
        );
        assert!(channel.zero_received);
        assert!(!channel.failed);
        assert!(channel.has_pending_receive_status());
        let mut status = nix::libc::pollfd {
            fd: channel.readiness().as_raw_fd(),
            events: nix::libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one writable pollfd whose descriptor stays owned by channel.
        assert_eq!(unsafe { nix::libc::poll(&mut status, 1, 0) }, 0);
        assert_eq!(
            status.revents, 0,
            "the consumed empty packet is no longer readable"
        );
        assert_eq!(
            channel.send_with(|_, _, _| panic!("a consumed zero read must resolve before sending")),
            Ok(SendProgress::Pending)
        );
        assert_eq!(channel.outbound.as_ref().unwrap().offset, 0);
        assert!(channel.outbound.as_ref().unwrap().datagram.is_empty());
        assert_eq!(
            channel.receive_with_status(
                |_, _, _| panic!("the consumed zero read must resolve before another receive"),
                |_| Ok(terminal)
            ),
            Err(if terminal == 0 {
                Fault::Framing("empty datagram from live peer")
            } else {
                Fault::Closed
            })
        );
        assert!(!channel.has_pending_receive_status());
        assert_retired(&mut channel);
    }
}

#[test]
fn outgoing_validation_sends_nothing_and_busy_keeps_the_first_whole_message() {
    for (tag, id, payload) in [
        (Tag::Request, 1, vec![]),
        (Tag::Request, 1, vec![0; 4]),
        (Tag::Request, 0, vec![0]),
        (Tag::Bootstrap, 1, vec![0]),
    ] {
        let (writer, reader) = pair();
        let mut sender = Channel::from_owned(writer, 3).unwrap();
        assert!(matches!(
            sender.begin_send(tag, id, payload),
            Err(Fault::Framing(_))
        ));
        assert_eq!(raw_receive(&reader), b"");
        assert_retired(&mut sender);
    }
    let (writer, reader) = pair();
    let mut sender = Channel::from_owned(writer, 8).unwrap();
    sender
        .begin_send(Tag::Bootstrap, 0, b"first".to_vec())
        .unwrap();
    assert_eq!(
        sender.begin_send(Tag::Request, 1, b"second".to_vec()),
        Err(Fault::Busy)
    );
    assert_eq!(sender.outbound.as_ref().unwrap().bytes, b"first");
    assert_eq!(sender.try_send(), Ok(SendProgress::Complete));
    assert_eq!(
        raw_receive(&reader),
        packet(Tag::Bootstrap, 0, 5, 0, b"first")
    );
}

#[test]
fn interrupted_syscalls_yield_without_retry_or_changing_any_cursor() {
    let (writer, reader) = pair();
    let mut channel = Channel::from_owned(reader, 8).unwrap();
    raw_send(&writer, &packet(Tag::Reply, 1, 6, 0, b"abc"));
    assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
    channel
        .begin_send(Tag::Request, 2, b"request".to_vec())
        .unwrap();
    assert_eq!(
        channel.receive_with(|_, _, flags| {
            assert!(flags.contains(MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC));
            Err(Errno::EINTR)
        }),
        Ok(Receive::Pending)
    );
    assert_eq!(channel.partial.as_ref().unwrap().bytes, b"abc");
    assert!(channel.has_pending_receive_status());
    assert_eq!(channel.try_receive(), Ok(Receive::Pending));
    assert!(!channel.has_pending_receive_status());
    assert_eq!(
        channel.send_with(|_, _, flags| {
            assert!(flags.contains(MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL));
            Err(Errno::EINTR)
        }),
        Ok(SendProgress::Pending)
    );
    let outgoing = channel.outbound.as_ref().unwrap();
    assert_eq!(outgoing.offset, 0);
    assert_eq!(outgoing.datagram, packet(Tag::Request, 2, 7, 0, b"request"));
    assert_eq!(channel.try_send(), Ok(SendProgress::Complete));
    assert_eq!(
        raw_receive(&writer),
        packet(Tag::Request, 2, 7, 0, b"request")
    );
    raw_send(&writer, &packet(Tag::Reply, 1, 6, 3, b"def"));
    assert!(matches!(channel.try_receive(), Ok(Receive::Complete(_))));
}

#[test]
fn an_interrupted_receive_cannot_turn_a_queued_reply_into_post_send_traffic() {
    for queued in [false, true] {
        let (writer, reader) = pair();
        let mut channel = Channel::from_owned(reader, 8).unwrap();
        channel
            .begin_send(Tag::Request, 1, b"request".to_vec())
            .unwrap();
        if queued {
            raw_send(&writer, &packet(Tag::Reply, 1, 6, 0, b"abc"));
        }
        assert_eq!(
            channel.receive_with(|_, _, _| Err(Errno::EINTR)),
            Ok(Receive::Pending)
        );
        assert!(channel.has_pending_receive_status());
        assert_eq!(
            channel.send_with(|_, _, _| panic!(
                "an interrupted receive did not prove an empty socket"
            )),
            Ok(SendProgress::Pending)
        );
        assert_eq!(channel.outbound.as_ref().unwrap().offset, 0);
        assert!(channel.outbound.as_ref().unwrap().datagram.is_empty());
        let mut byte = [0];
        assert_eq!(
            recv(writer.as_raw_fd(), &mut byte, MsgFlags::MSG_DONTWAIT),
            Err(Errno::EAGAIN)
        );
        assert_eq!(
            channel.try_receive(),
            Ok(if queued {
                Receive::Fragment
            } else {
                Receive::Pending
            })
        );
        assert!(!channel.has_pending_receive_status());
        if queued {
            // The port can now refuse the first premature fragment while the
            // entire request remains unsent. The channel owns no such policy.
            assert_eq!(channel.partial.as_ref().unwrap().bytes, b"abc");
            assert_eq!(channel.outbound.as_ref().unwrap().offset, 0);
        } else {
            assert_eq!(channel.try_send(), Ok(SendProgress::Complete));
            assert_eq!(
                raw_receive(&writer),
                packet(Tag::Request, 1, 7, 0, b"request")
            );
        }
    }
}

#[test]
fn short_atomic_send_is_terminal_and_keeps_its_exact_outbound_evidence() {
    let (writer, _reader) = pair();
    let mut channel = Channel::from_owned(writer, 8).unwrap();
    channel
        .begin_send(Tag::Request, 1, b"request".to_vec())
        .unwrap();
    assert_eq!(
        channel.send_with(|_, bytes, _| Ok(bytes.len() - 1)),
        Err(Fault::Framing("short SEQPACKET send"))
    );
    let outgoing = channel.outbound.as_ref().unwrap();
    assert_eq!(outgoing.offset, 0);
    assert_eq!(outgoing.bytes, b"request");
    assert_eq!(outgoing.datagram, packet(Tag::Request, 1, 7, 0, b"request"));
    assert_retired(&mut channel);
}

#[test]
fn backpressure_retains_exact_datagram_while_incoming_traffic_remains_visible() {
    let length = 32 * 65_510 + 7;
    let payload: Vec<_> = (0..length).map(|i| (i % 251) as u8).collect();
    let (writer, reader) = pair();
    nix::sys::socket::setsockopt(&writer, sockopt::SndBuf, &131_072_usize).unwrap();
    let mut sender = Channel::from_owned(writer, length).unwrap();
    let mut receiver = Channel::from_owned(reader, length).unwrap();
    sender.begin_send(Tag::Request, 1, payload.clone()).unwrap();
    let mut blocked = false;
    for _ in 0..32 {
        match sender.try_send().unwrap() {
            SendProgress::Pending => {
                blocked = true;
                break;
            }
            SendProgress::Progress => {}
            SendProgress::Complete => panic!("fixture must fill real send capacity"),
        }
    }
    assert!(blocked);
    let held = sender.outbound.as_ref().unwrap();
    let (offset, datagram) = (held.offset, held.datagram.clone());
    assert_eq!(sender.try_send(), Ok(SendProgress::Pending));
    assert_eq!(sender.outbound.as_ref().unwrap().offset, offset);
    assert_eq!(sender.outbound.as_ref().unwrap().datagram, datagram);

    // The adapter can reject a premature reply after this first fragment; it
    // need not wait for a complete peer message while its request is blocked.
    raw_send(&receiver.end, &packet(Tag::Reply, 1, 6, 0, b"abc"));
    assert_eq!(sender.try_receive(), Ok(Receive::Fragment));
    assert_eq!(sender.partial.as_ref().unwrap().bytes, b"abc");
    assert_eq!(sender.outbound.as_ref().unwrap().offset, offset);
    for _ in 0..128 {
        match receiver.try_receive().unwrap() {
            Receive::Complete(message) => {
                assert_eq!(message.payload, payload);
                assert_eq!((message.tag, message.id), (Tag::Request, 1));
                assert!(!sender.has_pending_send());
                return;
            }
            Receive::Pending | Receive::Fragment => {}
        }
        sender.try_send().unwrap();
    }
    panic!("bounded drain did not complete");
}

#[test]
fn ownership_requires_connected_unix_seqpacket_and_sets_only_descriptor_hygiene() {
    let (end, _peer) = pair();
    fcntl(&end, FcntlArg::F_SETFD(FdFlag::empty())).unwrap();
    let status_before = fcntl(&end, FcntlArg::F_GETFL).unwrap();
    let channel = Channel::from_owned(end, 1).unwrap();
    let flags = fcntl(channel.readiness(), FcntlArg::F_GETFD).unwrap();
    assert!(FdFlag::from_bits_retain(flags).contains(FdFlag::FD_CLOEXEC));
    assert_eq!(
        fcntl(channel.readiness(), FcntlArg::F_GETFL).unwrap(),
        status_before
    );
    let (end, _peer) = pair();
    assert!(matches!(
        Channel::from_owned(end, 0),
        Err(Fault::InvalidLimit)
    ));
    let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    assert!(matches!(
        Channel::from_owned(stream.into(), 8),
        Err(Fault::WrongSocket)
    ));
    let unconnected = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .unwrap();
    assert!(matches!(
        Channel::from_owned(unconnected, 8),
        Err(Fault::WrongSocket)
    ));
}

#[test]
fn explicit_close_and_failed_send_keep_evidence_and_never_resume() {
    let (writer, reader) = pair();
    let mut channel = Channel::from_owned(reader, 8).unwrap();
    raw_send(&writer, &packet(Tag::Reply, 1, 6, 0, b"abc"));
    assert_eq!(channel.try_receive(), Ok(Receive::Fragment));
    channel
        .begin_send(Tag::Request, 2, b"request".to_vec())
        .unwrap();
    channel.close();
    channel.close();
    assert_eq!(channel.partial.as_ref().unwrap().bytes, b"abc");
    assert_eq!(channel.outbound.as_ref().unwrap().bytes, b"request");
    assert_retired(&mut channel);
    assert_eq!(raw_receive(&writer), b"");

    let (writer, reader) = pair();
    let mut channel = Channel::from_owned(writer, 8).unwrap();
    channel
        .begin_send(Tag::Request, 1, b"request".to_vec())
        .unwrap();
    drop(reader);
    assert!(matches!(channel.try_send(), Err(Fault::Io(_))));
    assert_eq!(channel.outbound.as_ref().unwrap().bytes, b"request");
    assert_eq!(channel.outbound.as_ref().unwrap().offset, 0);
    assert_retired(&mut channel);
}
