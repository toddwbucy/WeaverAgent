use super::*;
use crate::flow::clock::{MonotonicIo, PumpIo, WaitInterest, WaitOutcome};
use nix::sys::socket::{AddressFamily, ControlMessage, SockFlag, recv, sendmsg, socketpair};
use std::cell::Cell;
use std::io::IoSlice;
use weaver_types::{
    ExchangeId, LabelAnswer, LabelRefusal, LifecycleAnswer, Opener, Position, TokenAnswer,
    TokenRefusal,
};

impl<T, E> PacketPort<T, E> {
    pub(in crate::flow) fn test_outgoing_packet(&self) -> Option<(&[u8], usize)> {
        self.outgoing
            .as_ref()
            .map(|outgoing| (outgoing.body.as_slice(), outgoing.offset))
    }
}

// A copied owner would permit a second read cursor on the same channel.
const _: fn() = || {
    trait NoClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <OrganPort as NoClone<_>>::check;
    let _ = <DecodePort as NoClone<_>>::check;
    let _ = <LabelPort as NoClone<_>>::check;
};

fn pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap()
}

fn envelope() -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal: 7,
        },
        position: Position::Close,
        payload: weaver_types::Payload::Answer(LifecycleAnswer::Ready),
    }
}

fn send_bytes(peer: &OwnedFd, bytes: &[u8]) {
    assert_eq!(
        send(
            peer.as_raw_fd(),
            bytes,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        ),
        Ok(bytes.len())
    );
}

fn receive_bytes(peer: &OwnedFd) -> Option<Vec<u8>> {
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
        other => panic!("fixture receive: {other:?}"),
    }
}

fn assert_frozen<T>(port: &mut PacketPort<T>, failure: &Failure) {
    assert_eq!(port.failure(), Some(failure));
    assert_eq!(port.interest().events(), PollFlags::empty());
    assert!(!port.needs_progress());
    assert_eq!(port.try_send(), Err(failure.clone()));
    assert_eq!(port.try_receive(), Err(failure.clone()));
}

#[test]
fn organ_reset_keeps_both_queued_packets_without_reopening_sends() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    port.stage(envelope()).unwrap();
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
    let first = format!(
        " \n{}\n ",
        serde_json::to_string_pretty(&envelope()).unwrap()
    );
    let mut second_envelope = envelope();
    second_envelope.exchange.ordinal += 1;
    let second = serde_json::to_vec(&second_envelope).unwrap();
    send_bytes(&peer, first.as_bytes());
    send_bytes(&peer, &second);
    // The peer exits without reading the sent packet. Linux reports reset
    // before making its two final queued packets available to this owner.
    drop(peer);
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.errno, Some(libc::ECONNRESET));
    assert_eq!(port.stage(envelope()).unwrap_err().failure, failure);
    assert_frozen(&mut port, &failure);
    assert!(port.closed_receive_pending());
    for expected in [first.as_bytes(), second.as_slice()] {
        assert_eq!(port.receive_closed(), Ok(ReceiveProgress::Held));
        assert_eq!(
            port.receive_closed_with(|_| panic!("one retained completion blocks another read")),
            Ok(ReceiveProgress::Held)
        );
        let packet = port.take_received().unwrap().unwrap();
        assert!(packet.belongs_to(&port));
        assert_eq!(packet.raw_bytes(), expected);
        assert_eq!(
            packet.envelope(),
            &serde_json::from_slice(expected).unwrap()
        );
        assert_frozen(&mut port, &failure);
    }
    assert_eq!(port.receive_closed(), Ok(ReceiveProgress::Pending));
    assert!(!port.closed_receive_pending());
    assert_eq!(
        port.receive_closed_with(|_| panic!("drained closure is final")),
        Ok(ReceiveProgress::Pending)
    );
    assert_frozen(&mut port, &failure);
}

#[test]
fn organ_closed_receive_keeps_malformed_tail_and_first_reset() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    port.stage(envelope()).unwrap();
    port.try_send().unwrap();
    let original = b"{\"malformed\": [  ";
    send_bytes(&peer, original);
    send_bytes(&peer, &serde_json::to_vec(&envelope()).unwrap());
    drop(peer);
    let first = port.try_receive().unwrap_err();
    assert_eq!(first.errno, Some(libc::ECONNRESET));
    let refusal = port.receive_closed().unwrap_err();
    assert_eq!(refusal.fault, ChannelFault::Undecodable);
    assert_eq!(port.failure(), Some(&first));
    assert!(port.held().is_none());
    assert_eq!(port.failed_wire(), Some(original.as_slice()));
    assert_eq!(
        port.receive_closed_with(|_| panic!("a refused tail cannot be skipped")),
        Err(refusal)
    );
    assert_eq!(port.take_failed_wire(), Some(original.to_vec()));
    assert_frozen(&mut port, &first);
}

#[test]
fn organ_closed_receive_yields_each_interruption_and_reset() {
    let (end, _peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    assert!(
        port.receive_closed_with(|_| panic!("live read is forbidden"))
            .is_err()
    );
    assert!(port.failure().is_none());
    let first = port.fail(Failure::io(Errno::EPIPE));
    let calls = Cell::new(0);
    for fault in [
        ReadFault::Interrupted,
        ReadFault::Failed {
            failure: Failure::io(Errno::ECONNRESET),
            received_wire: None,
        },
    ] {
        let before = calls.get();
        assert_eq!(
            port.receive_closed_with(|_| {
                calls.set(calls.get() + 1);
                Err(fault)
            }),
            Ok(ReceiveProgress::Pending)
        );
        assert_eq!(calls.get(), before + 1);
        assert!(port.closed_receive_pending());
        assert_frozen(&mut port, &first);
    }
    assert_eq!(
        port.receive_closed_with(|_| Err(ReadFault::Pending)),
        Ok(ReceiveProgress::Pending)
    );
    assert!(!port.closed_receive_pending());
    assert_frozen(&mut port, &first);
}

#[test]
fn closed_withdrawal_requires_the_original_ticket_and_preserves_received_bytes() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    let original = envelope();
    let TrackedStage::Queued(earlier) = port.stage_tracked(original.clone()).unwrap() else {
        panic!()
    };
    assert_eq!(
        port.try_send_observed(&mut MonotonicIo),
        Ok(SendProgress::Complete)
    );
    assert!(receive_bytes(&peer).is_some());
    let TrackedStage::Queued(current) = port.stage_tracked(original.clone()).unwrap() else {
        panic!()
    };
    assert!(port.withdraw_closed_pending(&current).is_err());
    assert!(port.send_pending() && !current.withdrawn());
    let (other, _peer) = pair();
    let mut other = OrganPort::from_owned(other).unwrap();
    let TrackedStage::Queued(foreign) = other.stage_tracked(original.clone()).unwrap() else {
        panic!()
    };
    let raw = format!(" \n{}\n ", serde_json::to_string_pretty(&original).unwrap());
    send_bytes(&peer, raw.as_bytes());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    drop(peer);
    let failure = port.try_send_observed(&mut MonotonicIo).unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    for wrong in [&earlier, &foreign] {
        assert!(port.withdraw_closed_pending(wrong).is_err());
        assert!(port.send_pending() && !current.withdrawn());
        assert_eq!(port.failure(), Some(&failure));
    }
    assert_eq!(
        port.withdraw_closed_pending(&current).unwrap(),
        Some(original.clone())
    );
    assert!(current.withdrawn() && !current.sent());
    assert!(earlier.sent() && !earlier.withdrawn());
    assert!(!foreign.sent() && !foreign.withdrawn());
    assert_eq!(port.failure(), Some(&failure));
    let held = port.take_received().unwrap().unwrap();
    assert_eq!(held.envelope(), &original);
    assert_eq!(held.raw_bytes(), raw.as_bytes());
    assert_eq!(port.withdraw_closed_pending(&current).unwrap(), None);
    assert_frozen(&mut port, &failure);
}

#[test]
fn closed_withdrawal_never_relabels_an_undecodable_port_as_closed() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    let TrackedStage::Queued(send) = port.stage_tracked(envelope()).unwrap() else {
        panic!()
    };
    send_bytes(&peer, b"not JSON");
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Undecodable);
    assert!(port.withdraw_closed_pending(&send).is_err());
    assert!(port.send_pending() && !send.withdrawn() && !send.sent());
    assert_eq!(port.failed_wire(), Some(b"not JSON".as_slice()));
    assert_frozen(&mut port, &failure);
}

#[test]
fn conversions_move_the_actual_socket_and_a_retired_label_cannot_reopen() {
    let (channel, _) = OrganChannel::pair().unwrap();
    let fd = channel.as_fd().as_raw_fd();
    let port = OrganPort::from_channel(channel).unwrap();
    assert_eq!(port.fd().as_raw_fd(), fd);

    let (channel, _) = DecodeChannel::pair().unwrap();
    let fd = channel.as_fd().as_raw_fd();
    let port = DecodePort::from_channel(channel).unwrap();
    assert_eq!(port.fd().as_raw_fd(), fd);

    let (channel, _) = ClassifyChannel::pair().unwrap();
    channel.retire();
    assert!(LabelPort::from_channel(channel).is_err());

    let (end, peer) = pair();
    fcntl(&end, FcntlArg::F_SETFD(FdFlag::empty())).unwrap();
    let status = fcntl(&end, FcntlArg::F_GETFL).unwrap();
    let port = LabelPort::from_owned(end).unwrap();
    assert!(
        FdFlag::from_bits_retain(fcntl(port.fd(), FcntlArg::F_GETFD).unwrap())
            .contains(FdFlag::FD_CLOEXEC)
    );
    assert_eq!(fcntl(port.fd(), FcntlArg::F_GETFL).unwrap(), status);
    drop(peer);

    let (stream, _) = std::os::unix::net::UnixStream::pair().unwrap();
    assert!(OrganPort::from_owned(stream.into()).is_err());
}

#[test]
fn typed_completion_preserves_identity_and_refusal_is_not_a_transport_fault() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    send_bytes(&peer, &serde_json::to_vec(&envelope()).unwrap());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.held(), Some(&envelope()));
    assert_eq!(port.take_completion().unwrap(), Some(envelope()));
    assert_eq!(port.take_completion().unwrap(), None);

    let (end, peer) = pair();
    let mut port = DecodePort::from_owned(end).unwrap();
    let refusal = TokenRefusal::NotOpen;
    send_bytes(&peer, &serde_json::to_vec(&refusal).unwrap());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert!(matches!(
        port.take_completion().unwrap(),
        Some(DecodeReply::Refusal(TokenRefusal::NotOpen))
    ));
    assert!(port.failure().is_none());

    let (end, peer) = pair();
    let mut port = LabelPort::from_owned(end).unwrap();
    send_bytes(
        &peer,
        &serde_json::to_vec(&LabelRefusal::MalformedContent).unwrap(),
    );
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert!(matches!(
        port.take_completion().unwrap(),
        Some(ClassifyReply::Refusal(LabelRefusal::MalformedContent))
    ));
    assert!(port.failure().is_none());
}

#[test]
fn each_receive_and_send_performs_one_packet_and_busy_returns_exact_input() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    let input = envelope();
    assert!(matches!(port.stage(input.clone()).unwrap(), Stage::Queued));
    let mut second = input.clone();
    second.exchange.ordinal = 99;
    assert_eq!(port.stage(second.clone()).unwrap(), Stage::Busy(second));
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
    assert_eq!(
        receive_bytes(&peer).unwrap(),
        serde_json::to_vec(&input).unwrap()
    );
    assert_eq!(receive_bytes(&peer), None);
    send_bytes(&peer, &serde_json::to_vec(&input).unwrap());
    send_bytes(&peer, &serde_json::to_vec(&input).unwrap());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.take_completion().unwrap(), Some(input.clone()));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.take_completion().unwrap(), Some(input));
}

#[test]
fn a_full_slot_keeps_half_close_interest_without_reading_the_next_packet() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    let input = envelope();
    send_bytes(&peer, &serde_json::to_vec(&input).unwrap());
    send_bytes(&peer, &serde_json::to_vec(&input).unwrap());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    let events = port.interest().events();
    assert!(!events.contains(PollFlags::POLLIN));
    assert_ne!(events.bits() & libc::POLLRDHUP, 0);
    let mut observed = libc::pollfd {
        fd: port.fd().as_raw_fd(),
        events: events.bits(),
        revents: 0,
    };
    // SAFETY: one nonblocking probe of the live fixture descriptor.
    assert_eq!(unsafe { libc::poll(&mut observed, 1, 0) }, 0);
    shutdown(peer.as_raw_fd(), Shutdown::Write).unwrap();
    // SAFETY: same live pollfd.
    assert_eq!(unsafe { libc::poll(&mut observed, 1, 0) }, 1);
    assert_ne!(observed.revents & libc::POLLRDHUP, 0);
    let mut io = MonotonicIo;
    let deadline = io.now().unwrap().checked_add(1_000_000_000).unwrap();
    let wake = io
        .wait(
            &[WaitInterest {
                fd: port.fd(),
                events,
            }],
            Some(deadline),
        )
        .unwrap();
    let WaitOutcome::Ready(ready) = wake else {
        panic!("half-close must wake the common clock wait")
    };
    assert_eq!(ready.len(), 1);
    assert_ne!(ready[0].events.bits() & libc::POLLRDHUP, 0);
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert_eq!(port.held(), Some(&input));
    assert_frozen(&mut port, &failure);
    // Transfer evidence once after observed closure, without reviving the port.
    assert_eq!(port.take_completion().unwrap(), Some(input));
    assert_eq!(port.take_completion(), Err(failure.clone()));
    assert_frozen(&mut port, &failure);
}

#[test]
fn complete_final_reply_before_orderly_exit_is_preserved_for_the_caller() {
    let (end, peer) = pair();
    let mut port = LabelPort::from_owned(end).unwrap();
    let reply = LabelRefusal::NotAdmitted {
        reason: "synthetic refusal".into(),
    };
    send_bytes(&peer, &serde_json::to_vec(&reply).unwrap());
    drop(peer);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    let Some(ClassifyReply::Refusal(actual)) = port.take_completion().unwrap() else {
        panic!("refusal retained")
    };
    assert_eq!(actual, reply);
    assert_eq!(port.failure(), Some(&failure));
}

#[test]
fn interrupted_operations_yield_without_a_second_syscall_or_lost_cursor() {
    let (end, _) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    port.stage(envelope()).unwrap();
    let calls = Cell::new(0);
    assert_eq!(
        port.send_with(|_, _, _| {
            calls.set(calls.get() + 1);
            Err(Errno::EINTR)
        }),
        Ok(SendProgress::Pending)
    );
    assert_eq!(calls.get(), 1);
    assert!(port.needs_progress());
    assert_eq!(port.outgoing.as_ref().unwrap().offset, 0);
    assert_eq!(
        port.send_with(|_, _, _| Err(Errno::EAGAIN)),
        Ok(SendProgress::Pending)
    );
    assert!(!port.needs_progress());
    assert_eq!(
        port.receive_with(|_| Err(ReadFault::Interrupted), |_| unreachable!()),
        Ok(ReceiveProgress::Pending)
    );
    assert!(port.needs_progress());
    assert_eq!(
        port.receive_with(|_| Err(ReadFault::Pending), |_| unreachable!()),
        Ok(ReceiveProgress::Pending)
    );
    assert!(!port.needs_progress());
}

#[test]
fn empty_live_datagram_differs_from_eof_and_its_interrupted_probe_is_held() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    send_bytes(&peer, &[]);
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Undecodable);
    assert_frozen(&mut port, &failure);

    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    drop(peer);
    assert_eq!(port.try_receive().unwrap_err().fault, ChannelFault::Closed);

    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    port.stage(envelope()).unwrap();
    assert_eq!(
        port.receive_with(|_| Ok(Packet { bytes: vec![] }), |_| Err(Errno::EINTR)),
        Ok(ReceiveProgress::Pending)
    );
    assert!(port.needs_progress());
    assert_eq!(
        port.send_with(|_, _, _| panic!("unresolved zero read cannot send")),
        Ok(SendProgress::Pending)
    );
    let failure = port
        .receive_with(|_| panic!("zero-read status is held"), |_| Ok(0))
        .unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Undecodable);
    assert_eq!(port.outgoing.as_ref().unwrap().offset, 0);
    assert_frozen(&mut port, &failure);
    drop(peer);
}

fn large_token() -> Vec<u8> {
    serde_json::to_vec(&TokenAnswer::Token {
        token: 1,
        piece: "p".repeat(MAX_ENVELOPE_BYTES + 100),
    })
    .unwrap()
}

fn preamble(total: usize) -> Vec<u8> {
    serde_json::to_vec(&SegmentPreamble {
        segments: total.div_ceil(MAX_ENVELOPE_BYTES) as u64,
        bytes: total as u64,
    })
    .unwrap()
}

#[test]
fn decode_series_is_incremental_and_accepts_unequal_contract_valid_segments() {
    let (end, peer) = pair();
    let mut port = DecodePort::from_owned(end).unwrap();
    let body = large_token();
    assert!(body.len() > MAX_ENVELOPE_BYTES && body.len() < MAX_ENVELOPE_BYTES + 1024);
    send_bytes(&peer, &preamble(body.len()));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Progress));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Pending));
    assert!(!port.needs_progress());
    send_bytes(&peer, &body[..1024]);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Progress));
    assert!(port.held().is_none());
    assert_eq!(port.series.as_ref().unwrap().bytes, body[..1024]);
    send_bytes(&peer, &body[1024..]);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.received_wire.as_deref(), Some(body.as_slice()));
    let Some(DecodeReply::Answer(TokenAnswer::Token { token, piece })) =
        port.take_completion().unwrap()
    else {
        panic!("whole token message")
    };
    assert_eq!(token, 1);
    assert_eq!(piece, "p".repeat(MAX_ENVELOPE_BYTES + 100));
}

#[test]
fn malformed_preamble_refuses_before_allocating_the_claimed_series() {
    for declaration in [
        serde_json::json!({"segments":1,"bytes":MAX_ENVELOPE_BYTES}),
        serde_json::json!({"segments":2,"bytes":DECODE_MESSAGE_BOUND + 1}),
        serde_json::json!({"segments":3,"bytes":MAX_ENVELOPE_BYTES + 1}),
        serde_json::json!({"segments":2,"bytes":MAX_ENVELOPE_BYTES + 1,"extra":true}),
        serde_json::json!({"segments":0,"bytes":0}),
    ] {
        let (end, peer) = pair();
        let mut port = DecodePort::from_owned(end).unwrap();
        send_bytes(&peer, &serde_json::to_vec(&declaration).unwrap());
        let failure = port.try_receive().unwrap_err();
        assert_eq!(failure.fault, ChannelFault::Undecodable);
        assert!(port.series.is_none());
        assert!(port.received_wire.is_some());
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn short_long_and_closed_decode_series_never_expose_a_partial_completion() {
    for case in 0..3 {
        let (end, peer) = pair();
        let mut port = DecodePort::from_owned(end).unwrap();
        let body = large_token();
        // In the short case the actual segments still form valid complete
        // JSON. Only the declared-total check may reject that whole message.
        let declared = if case == 0 {
            body.len() + 1
        } else {
            body.len()
        };
        send_bytes(&peer, &preamble(declared));
        port.try_receive().unwrap();
        send_bytes(&peer, &body[..MAX_ENVELOPE_BYTES]);
        port.try_receive().unwrap();
        match case {
            0 => send_bytes(&peer, &body[MAX_ENVELOPE_BYTES..]),
            1 => {
                let mut extra = body[MAX_ENVELOPE_BYTES..].to_vec();
                extra.push(b'x');
                send_bytes(&peer, &extra);
            }
            2 => drop(peer),
            _ => unreachable!(),
        }
        let failure = port.try_receive().unwrap_err();
        assert!(port.held().is_none());
        assert!(port.series.is_some());
        assert!(
            port.series
                .as_ref()
                .unwrap()
                .bytes
                .starts_with(&body[..MAX_ENVELOPE_BYTES])
        );
        assert_eq!(
            failure.fault,
            if case == 2 {
                ChannelFault::Closed
            } else {
                ChannelFault::Undecodable
            }
        );
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn decode_send_retains_preamble_and_each_slice_without_looping() {
    let (end, peer) = pair();
    let mut port = DecodePort::from_owned(end).unwrap();
    let input = TokenDirective::AppendAndGenerate {
        turn: weaver_types::TurnKey("run:1".into()),
        tool_schemas: Vec::new(),
        delta: vec![weaver_traits::Message {
            role: weaver_traits::Role::User,
            content: vec![weaver_traits::ContentBlock::Text {
                text: "x".repeat(MAX_ENVELOPE_BYTES * 2),
            }],
        }],
    };
    let expected = serde_json::to_vec(&input).unwrap();
    assert!(matches!(port.stage(input).unwrap(), Stage::Queued));
    assert_eq!(port.try_send(), Ok(SendProgress::Progress));
    let head: SegmentPreamble = serde_json::from_slice(&receive_bytes(&peer).unwrap()).unwrap();
    assert_eq!(head.bytes, expected.len() as u64);
    assert_eq!(receive_bytes(&peer), None);
    let mut reconstructed = Vec::new();
    for index in 0..head.segments {
        let progress = port.try_send().unwrap();
        assert_eq!(
            progress,
            if index + 1 == head.segments {
                SendProgress::Complete
            } else {
                SendProgress::Progress
            }
        );
        reconstructed.extend(receive_bytes(&peer).unwrap());
        assert_eq!(receive_bytes(&peer), None);
    }
    assert_eq!(reconstructed, expected);
}

#[test]
fn encoding_and_size_failures_return_the_owned_input_and_freeze_the_port() {
    let (end, _) = pair();
    let mut port = LabelPort::from_owned(end).unwrap();
    let input = LabelDirective::Classify {
        turn: None,
        content: "x".repeat(MAX_ENVELOPE_BYTES),
    };
    let failure = port.stage(input.clone()).unwrap_err();
    assert_eq!(failure.input, input);
    assert_eq!(
        failure.failure.fault,
        ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES
        }
    );
    assert_frozen(&mut port, &failure.failure);
    let failure_again = port.stage(input.clone()).unwrap_err();
    assert_eq!(failure_again.input, input);
    assert_eq!(failure_again.failure, failure.failure);

    let (end, _) = pair();
    let mut port = LabelPort::from_owned(end).unwrap();
    let failure = port
        .stage_encoded(input.clone(), |_| {
            Err(serde_json::from_str::<serde_json::Value>("not JSON").unwrap_err())
        })
        .unwrap_err();
    assert_eq!(failure.input, input);
    assert_eq!(failure.failure.fault, ChannelFault::Undecodable);
}

#[test]
fn short_send_preserves_the_unsent_message_and_first_failure() {
    let (end, _) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    port.stage(envelope()).unwrap();
    let expected = port.outgoing.as_ref().unwrap().body.clone();
    let failure = port
        .send_with(|_, bytes, _| Ok(bytes.len() - 1))
        .unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert_eq!(port.outgoing.as_ref().unwrap().body, expected);
    assert_eq!(port.outgoing.as_ref().unwrap().offset, 0);
    assert_frozen(&mut port, &failure);
}

#[test]
fn incoming_organ_traffic_is_legal_while_an_outgoing_message_waits() {
    let (end, peer) = pair();
    let mut port = OrganPort::from_owned(end).unwrap();
    port.stage(envelope()).unwrap();
    let mut incoming = envelope();
    incoming.exchange.opener = Opener::Gate;
    send_bytes(&peer, &serde_json::to_vec(&incoming).unwrap());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.take_completion().unwrap(), Some(incoming));
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
}

#[test]
fn ancillary_and_truncated_ancillary_refuse_without_leaking_installed_descriptors() {
    for count in [1, 100] {
        let (end, peer) = pair();
        let mut port = OrganPort::from_owned(end).unwrap();
        let (read_end, write_end) = nix::unistd::pipe().unwrap();
        let flags =
            nix::fcntl::OFlag::from_bits_retain(fcntl(&read_end, FcntlArg::F_GETFL).unwrap());
        fcntl(
            &read_end,
            FcntlArg::F_SETFL(flags | nix::fcntl::OFlag::O_NONBLOCK),
        )
        .unwrap();
        let raw = vec![write_end.as_raw_fd(); count];
        let payload = serde_json::to_vec(&envelope()).unwrap();
        assert_eq!(
            sendmsg::<()>(
                peer.as_raw_fd(),
                &[IoSlice::new(&payload)],
                &[ControlMessage::ScmRights(&raw)],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                None
            ),
            Ok(payload.len())
        );
        drop(write_end);
        let failure = port.try_receive().unwrap_err();
        assert_eq!(
            failure.fault,
            if count == 1 {
                ChannelFault::Undecodable
            } else {
                ChannelFault::Truncated {
                    bound: MAX_ENVELOPE_BYTES,
                }
            }
        );
        // Every received duplicate of the write end must be closed, including
        // delivered descriptors on MSG_CTRUNC. No writer remains, so EOF wins.
        assert_eq!(nix::unistd::read(&read_end, &mut [0u8; 1]), Ok(0));
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn truncation_and_bad_json_retire_without_producing_a_typed_prefix() {
    let mut oversized = serde_json::to_vec(&LabelAnswer::Ready).unwrap();
    oversized.resize(MAX_ENVELOPE_BYTES + 1, b' ');
    for bytes in [oversized, b"not JSON".to_vec()] {
        let (end, peer) = pair();
        let mut port = LabelPort::from_owned(end).unwrap();
        send_bytes(&peer, &bytes);
        let failure = port.try_receive().unwrap_err();
        assert_eq!(
            failure.fault,
            if bytes.len() > MAX_ENVELOPE_BYTES {
                ChannelFault::Truncated {
                    bound: MAX_ENVELOPE_BYTES,
                }
            } else {
                ChannelFault::Undecodable
            }
        );
        assert!(port.held().is_none());
        assert_frozen(&mut port, &failure);
    }
}

#[test]
fn label_ready_is_an_ordinary_completion_and_the_adapter_has_no_wait_clock() {
    let (end, peer) = pair();
    let mut port = LabelPort::from_owned(end).unwrap();
    send_bytes(&peer, &serde_json::to_vec(&LabelAnswer::Ready).unwrap());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert!(matches!(
        port.take_completion().unwrap(),
        Some(ClassifyReply::Answer(LabelAnswer::Ready))
    ));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Pending));
}
