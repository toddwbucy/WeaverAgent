//! Actual packet custody for PAIR-MATCH. No fixture manufactures ReceivedOrgan.

use super::*;
use crate::flow::clock::testing::FakePumpIo;
use nix::sys::socket::{AddressFamily, ControlMessage, SockFlag, recv, sendmsg, socketpair};
use std::io::IoSlice;
use weaver_types::{ExchangeId, Opener, Payload, Position, ToolOutcome};

// A copied or default owner would counterfeit a second received datagram.
const _: fn() = || {
    trait NoClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <ReceivedOrgan as NoClone<_>>::check;
    let _ = <OrganSend as NoClone<_>>::check;

    trait NoDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoDefault<()> for T {}
    impl<T: Default> NoDefault<u8> for T {}
    let _ = <ReceivedOrgan as NoDefault<_>>::check;
    let _ = <OrganSend as NoDefault<_>>::check;
    let _ = <PortIdentity as NoDefault<_>>::check;
};

fn pair() -> (OrganPort, OwnedFd) {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (OrganPort::from_owned(near).unwrap(), far)
}

fn send_bytes(peer: &OwnedFd, bytes: &[u8]) {
    assert_eq!(
        send(
            peer.as_raw_fd(),
            bytes,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        ),
        Ok(bytes.len()),
    );
}

fn tracked(port: &mut OrganPort, input: OrganEnvelope) -> OrganSend {
    match port.stage_tracked(input).unwrap() {
        TrackedStage::Queued(ticket) => ticket,
        TrackedStage::Busy(_) => panic!("fixture expected an empty outgoing slot"),
    }
}

fn envelope(ordinal: u64) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal,
        },
        position: Position::Close,
        payload: Payload::ToolAnswer(ToolOutcome::Result {
            content: "ab".into(),
        }),
    }
}

fn raw(ordinal: u64) -> Vec<u8> {
    format!(
        " \n{{ \"payload\": {{ \"kind\": \"tool_answer\", \"body\": {{ \"outcome\": \"result\", \"content\": \"a\\u0062\", \"opaque\": [1, 2] }} }}, \"position\": \"close\", \"exchange\": {{ \"ordinal\": {ordinal}, \"opener\": \"harness\", \"extra\": true }}, \"transport_note\": \"kept\" }}\t"
    ).into_bytes()
}

#[test]
fn received_organ_keeps_original_whitespace_order_escapes_and_unknown_fields() {
    let (mut port, peer) = pair();
    let bytes = raw(7);
    send_bytes(&peer, &bytes);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.held(), Some(&envelope(7)));
    assert!(port.failed_wire().is_none());
    assert!(port.take_failed_wire().is_none());
    let received = port.take_received().unwrap().unwrap();
    assert_eq!(received.envelope(), &envelope(7));
    assert_eq!(received.raw_bytes(), bytes);
    assert_ne!(serde_json::to_vec(received.envelope()).unwrap(), bytes);
    let (typed, original) = received.into_parts();
    assert_eq!(typed, envelope(7));
    assert_eq!(original, bytes);
    assert!(port.take_received().unwrap().is_none());
    assert!(port.take_completion().unwrap().is_none());
}

/// RawValue preserves the JSON value spelling, but its documented parser trims
/// surrounding whitespace. The transport owner must therefore keep all octets
/// independently, not replace them with a RawValue or a normalized rendering.
#[test]
fn raw_value_trimming_does_not_rewrite_received_datagram_custody() {
    let (mut port, peer) = pair();
    let bytes = raw(7);
    send_bytes(&peer, &bytes);
    port.try_receive().unwrap();
    let received = port.take_received().unwrap().unwrap();
    let value = serde_json::value::RawValue::from_string(
        std::str::from_utf8(received.raw_bytes())
            .unwrap()
            .to_owned(),
    )
    .unwrap();
    assert_eq!(
        value.get().as_bytes(),
        std::str::from_utf8(&bytes).unwrap().trim().as_bytes()
    );
    assert_ne!(value.get().as_bytes(), received.raw_bytes());
    assert_eq!(received.raw_bytes(), bytes);
}

#[test]
fn a_full_completion_slot_keeps_each_datagrams_typed_and_raw_views_together() {
    let (mut port, peer) = pair();
    let first = raw(7);
    let second = raw(8);
    send_bytes(&peer, &first);
    send_bytes(&peer, &second);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    let held = port.take_received().unwrap().unwrap();
    assert_eq!(held.envelope(), &envelope(7));
    assert_eq!(held.raw_bytes(), first);
    assert!(port.take_received().unwrap().is_none());
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    let next = port.take_received().unwrap().unwrap();
    assert_eq!(next.envelope(), &envelope(8));
    assert_eq!(next.raw_bytes(), second);
    assert_eq!(held.raw_bytes(), first);
}

#[test]
fn legacy_and_raw_completion_takes_share_one_transfer() {
    let (mut port, peer) = pair();
    send_bytes(&peer, &raw(7));
    port.try_receive().unwrap();
    assert_eq!(port.take_completion().unwrap(), Some(envelope(7)));
    assert!(port.take_received().unwrap().is_none());
    send_bytes(&peer, &raw(8));
    port.try_receive().unwrap();
    assert_eq!(
        port.take_received().unwrap().unwrap().envelope(),
        &envelope(8)
    );
    assert!(port.take_completion().unwrap().is_none());
}

#[test]
fn a_received_final_answer_survives_later_eof_with_its_exact_bytes() {
    let (mut port, peer) = pair();
    let bytes = raw(7);
    send_bytes(&peer, &bytes);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    shutdown(peer.as_raw_fd(), Shutdown::Write).unwrap();
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert!(port.failed_wire().is_none());
    assert!(
        port.take_failed_wire().is_none(),
        "fault evidence cannot steal a final answer"
    );
    let received = port.take_received().unwrap().unwrap();
    assert_eq!(received.envelope(), &envelope(7));
    assert_eq!(received.raw_bytes(), bytes);
    assert_eq!(port.failure(), Some(&failure));
    assert!(matches!(port.take_received(), Err(ref actual) if actual == &failure));
    assert_eq!(port.try_send(), Err(failure.clone()));
    assert_eq!(port.try_receive(), Err(failure));
    assert!(port.failed_wire().is_none());
}

#[test]
fn malformed_packets_retain_evidence_once_without_clearing_the_fault() {
    for bytes in [
        b" { \"exchange\": " as &[u8],
        b"{\"position\": 19}",
        b"\xff\xfe\x00",
        b"",
    ] {
        let (mut port, peer) = pair();
        send_bytes(&peer, bytes);
        let failure = port.try_receive().unwrap_err();
        assert_eq!(failure.fault, ChannelFault::Undecodable);
        assert_eq!(port.failed_wire(), Some(bytes));
        assert!(matches!(port.take_received(), Err(ref actual) if actual == &failure));
        assert_eq!(port.take_failed_wire().as_deref(), Some(bytes));
        assert!(port.failed_wire().is_none());
        assert!(port.take_failed_wire().is_none());
        assert_eq!(port.failure(), Some(&failure));
        assert_eq!(port.try_receive(), Err(failure.clone()));
        assert_eq!(port.try_send(), Err(failure.clone()));
        assert_eq!(port.take_completion(), Err(failure));
    }
}

#[test]
fn truncation_keeps_only_the_available_prefix_and_never_an_answer() {
    let (mut port, peer) = pair();
    let bytes = vec![b'x'; MAX_ENVELOPE_BYTES + 17];
    send_bytes(&peer, &bytes);
    let failure = port.try_receive().unwrap_err();
    assert_eq!(
        failure.fault,
        ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES
        }
    );
    assert_eq!(port.failed_wire(), Some(&bytes[..MAX_ENVELOPE_BYTES]));
    assert!(port.take_received().is_err());
    assert_eq!(
        port.take_failed_wire().unwrap(),
        bytes[..MAX_ENVELOPE_BYTES]
    );
    assert!(port.take_failed_wire().is_none());
    assert_eq!(port.failure(), Some(&failure));
}

#[test]
fn refused_ancillary_packet_keeps_its_bytes_after_closing_supplied_handles() {
    let (mut port, peer) = pair();
    let bytes = raw(7);
    let (read, write) = nix::unistd::pipe().unwrap();
    fcntl(&read, FcntlArg::F_SETFL(nix::fcntl::OFlag::O_NONBLOCK)).unwrap();
    let descriptors = [write.as_raw_fd()];
    sendmsg::<()>(
        peer.as_raw_fd(),
        &[IoSlice::new(&bytes)],
        &[ControlMessage::ScmRights(&descriptors)],
        MsgFlags::MSG_NOSIGNAL,
        None,
    )
    .unwrap();
    drop(write);
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Undecodable);
    assert_eq!(port.failed_wire(), Some(bytes.as_slice()));
    let mut probe = [0u8; 1];
    assert_eq!(
        nix::unistd::read(&read, &mut probe),
        Ok(0),
        "received handle was closed"
    );
    assert_eq!(port.take_failed_wire().unwrap(), bytes);
    assert!(port.take_received().is_err());
}

fn receive_bytes(peer: &OwnedFd) -> Vec<u8> {
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    let count = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
    bytes.truncate(count);
    bytes
}

#[test]
fn send_pending_tracks_the_exact_staged_operation_through_busy_and_pressure() {
    let (mut port, peer) = pair();
    assert!(!port.send_pending());
    let filler = b"queued";
    let mut filled = 0;
    loop {
        match send(
            port.fd().as_raw_fd(),
            filler,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(count) => {
                assert_eq!(count, filler.len());
                filled += 1;
            }
            Err(Errno::EAGAIN) => break,
            other => panic!("socket fill: {other:?}"),
        }
        assert!(filled < 100_000);
    }
    let ticket = tracked(&mut port, envelope(7));
    let mut clock = FakePumpIo::new(100);
    assert!(port.send_pending());
    assert_eq!(
        port.try_send_observed(&mut clock),
        Ok(SendProgress::Pending)
    );
    assert!(!ticket.sent());
    assert_eq!(ticket.sent_at(), None);
    assert!(!ticket.withdrawn());
    assert!(port.send_pending());
    assert_eq!(port.stage(envelope(8)).unwrap(), Stage::Busy(envelope(8)));
    for _ in 0..filled {
        assert_eq!(receive_bytes(&peer), filler);
    }
    clock.advance_to(200).unwrap();
    assert_eq!(
        port.try_send_observed(&mut clock),
        Ok(SendProgress::Complete)
    );
    assert!(ticket.sent());
    assert_eq!(ticket.sent_at(), Some(200));
    assert!(!ticket.withdrawn());
    assert!(!port.send_pending());
    assert_eq!(
        receive_bytes(&peer),
        serde_json::to_vec(&envelope(7)).unwrap()
    );
}

#[test]
fn a_failed_send_remains_pending_without_claiming_transport_success() {
    let (mut port, peer) = pair();
    assert!(matches!(port.stage(envelope(7)).unwrap(), Stage::Queued));
    drop(peer);
    let failure = port.try_send().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert!(port.send_pending(), "the queued operation never completed");
    assert_eq!(port.try_send(), Err(failure));
}

#[test]
fn eof_without_a_packet_does_not_invent_malformed_datagram_evidence() {
    let (mut port, peer) = pair();
    drop(peer);
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert!(port.failed_wire().is_none());
    assert!(port.take_failed_wire().is_none());
    assert!(port.take_received().is_err());
}

#[test]
fn withdrawing_the_exact_unsent_exchange_sends_nothing_and_keeps_incoming_custody() {
    let (mut port, peer) = pair();
    let outgoing = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal: 19,
        },
        position: Position::Open,
        payload: Payload::Tool(weaver_types::ToolExecution {
            name: weaver_types::ToolName("sh".into()),
            arguments: r#"{"command":"true"}"#.into(),
            clock_ms: 1_000,
        }),
    };
    let incoming = raw(7);
    send_bytes(&peer, &incoming);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    assert!(matches!(
        port.stage(outgoing.clone()).unwrap(),
        Stage::Queued
    ));
    assert!(port.send_pending());
    assert_eq!(port.withdraw_pending(&envelope(7).exchange).unwrap(), None);
    assert!(port.send_pending(), "the mismatched exchange stays queued");
    assert_eq!(
        port.withdraw_pending(&outgoing.exchange).unwrap(),
        Some(outgoing.clone())
    );
    assert!(!port.send_pending());
    assert_eq!(port.withdraw_pending(&outgoing.exchange).unwrap(), None);
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
    let mut probe = [0u8; 1];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut probe, MsgFlags::MSG_DONTWAIT),
        Err(Errno::EAGAIN)
    );
    let held = port.take_received().unwrap().unwrap();
    assert_eq!(held.envelope(), &envelope(7));
    assert_eq!(held.raw_bytes(), incoming);
}

#[test]
fn an_already_sent_exchange_cannot_be_withdrawn_as_not_dispatched() {
    let (mut port, peer) = pair();
    let outgoing = envelope(7);
    assert!(matches!(
        port.stage(outgoing.clone()).unwrap(),
        Stage::Queued
    ));
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
    assert_eq!(port.withdraw_pending(&outgoing.exchange).unwrap(), None);
    assert_eq!(receive_bytes(&peer), serde_json::to_vec(&outgoing).unwrap());
}

#[test]
fn a_failed_send_refuses_withdrawal_and_retains_the_original_queued_bytes() {
    let (mut port, peer) = pair();
    let outgoing = envelope(7);
    assert!(matches!(
        port.stage(outgoing.clone()).unwrap(),
        Stage::Queued
    ));
    drop(peer);
    let failure = port.try_send().unwrap_err();
    assert_eq!(
        port.withdraw_pending(&outgoing.exchange),
        Err(failure.clone())
    );
    assert!(port.send_pending());
    assert_eq!(
        port.outgoing.as_ref().unwrap().body,
        serde_json::to_vec(&outgoing).unwrap()
    );
    assert_eq!(port.failure(), Some(&failure));
}

#[test]
fn withdrawing_an_interrupted_send_removes_only_its_retry_interest() {
    let (mut port, _peer) = pair();
    let outgoing = envelope(7);
    port.stage(outgoing.clone()).unwrap();
    assert_eq!(
        port.send_with(|_, _, _| Err(Errno::EINTR)),
        Ok(SendProgress::Pending)
    );
    assert!(port.needs_progress());
    assert_eq!(
        port.withdraw_pending(&outgoing.exchange).unwrap(),
        Some(outgoing)
    );
    assert!(!port.send_pending());
    assert!(!port.needs_progress());
    assert!(!port.interest().writable);
    assert!(port.interest().readable);
}

#[test]
fn a_different_empty_port_cannot_prove_a_staged_packet_was_sent() {
    let (mut first, first_peer) = pair();
    let (mut other, other_peer) = pair();
    let ticket = tracked(&mut first, envelope(7));
    let identity = ticket.port_identity();
    assert!(ticket.belongs_to(&first));
    assert!(identity.matches(&first));
    assert!(!ticket.belongs_to(&other));
    assert!(!identity.matches(&other));
    assert_eq!(other.try_send(), Ok(SendProgress::Complete));
    assert!(!ticket.sent(), "an unrelated empty slot proves nothing");
    assert!(!ticket.withdrawn());
    assert!(first.send_pending());
    send_bytes(&other_peer, &raw(7));
    other.try_receive().unwrap();
    let foreign = other.take_received().unwrap().unwrap();
    assert!(foreign.belongs_to(&other));
    assert!(!foreign.belongs_to(&first));
    assert!(foreign.port_identity().matches(&other));
    assert!(!foreign.port_identity().matches(&first));
    assert_eq!(
        first.try_send_observed(&mut FakePumpIo::new(100)),
        Ok(SendProgress::Complete)
    );
    assert!(ticket.sent());
    assert_eq!(ticket.sent_at(), Some(100));
    assert_eq!(
        receive_bytes(&first_peer),
        serde_json::to_vec(&envelope(7)).unwrap()
    );
}

#[test]
fn a_withdrawn_ticket_never_becomes_sent_when_the_same_port_sends_again() {
    let (mut port, peer) = pair();
    let first = tracked(&mut port, envelope(7));
    assert!(matches!(
        port.stage_tracked(envelope(8)).unwrap(),
        TrackedStage::Busy(frame) if frame == envelope(8)
    ));
    assert!(
        port.withdraw_pending(&envelope(8).exchange)
            .unwrap()
            .is_none()
    );
    assert!(!first.withdrawn());
    assert_eq!(
        port.withdraw_pending(&envelope(7).exchange).unwrap(),
        Some(envelope(7))
    );
    assert!(first.withdrawn());
    assert!(!first.sent());
    assert_eq!(first.sent_at(), None);
    assert_eq!(port.try_send(), Ok(SendProgress::Complete));
    assert!(!first.sent(), "an empty slot does not undo withdrawal");
    // Even the same exchange bytes staged again are a distinct operation.
    let second = tracked(&mut port, envelope(7));
    assert!(!second.sent());
    assert!(!second.withdrawn());
    assert_eq!(
        port.try_send_observed(&mut FakePumpIo::new(100)),
        Ok(SendProgress::Complete)
    );
    assert!(second.sent());
    assert_eq!(second.sent_at(), Some(100));
    assert!(!second.withdrawn());
    assert!(first.withdrawn());
    assert!(!first.sent());
    assert_eq!(
        receive_bytes(&peer),
        serde_json::to_vec(&envelope(7)).unwrap()
    );
}

#[test]
fn a_sent_ticket_and_its_received_answer_survive_later_eof() {
    let (mut port, peer) = pair();
    let ticket = tracked(&mut port, envelope(7));
    let identity = ticket.port_identity();
    let mut clock = FakePumpIo::new(100);
    assert_eq!(
        port.try_send_observed(&mut clock),
        Ok(SendProgress::Complete)
    );
    assert_eq!(
        receive_bytes(&peer),
        serde_json::to_vec(&envelope(7)).unwrap()
    );
    send_bytes(&peer, &raw(7));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
    shutdown(peer.as_raw_fd(), Shutdown::Write).unwrap();
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert!(ticket.sent());
    clock.advance_to(10_000).unwrap();
    assert_eq!(
        ticket.sent_at(),
        Some(100),
        "consumer delay cannot move send time"
    );
    assert!(!ticket.withdrawn());
    assert!(ticket.belongs_to(&port));
    assert!(identity.matches(&port));
    let received = port.take_received().unwrap().unwrap();
    assert_eq!(received.raw_bytes(), raw(7));
    assert!(received.belongs_to(&port));
    assert!(received.port_identity().matches(&port));
    assert_eq!(port.failure(), Some(&failure));
    assert!(ticket.sent());
}

#[test]
fn failed_or_short_sends_keep_the_ticket_pending_and_owed() {
    let (mut port, peer) = pair();
    let ticket = tracked(&mut port, envelope(7));
    drop(peer);
    let failure = port
        .try_send_observed(&mut FakePumpIo::new(100))
        .unwrap_err();
    assert!(!ticket.sent());
    assert_eq!(ticket.sent_at(), None);
    assert!(!ticket.withdrawn());
    assert!(port.send_pending());
    assert_eq!(port.withdraw_pending(&envelope(7).exchange), Err(failure));
    assert!(!ticket.sent());
    assert!(!ticket.withdrawn());

    let (mut port, _peer) = pair();
    let ticket = tracked(&mut port, envelope(7));
    let failure = port
        .send_with_observation(
            |_, bytes, _| Ok(bytes.len() - 1),
            Some(&mut FakePumpIo::new(100)),
        )
        .unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert!(!ticket.sent());
    assert!(!ticket.withdrawn());
    assert!(port.send_pending());
}

#[test]
fn port_identity_survives_moves_and_does_not_match_a_replacement_owner() {
    let (mut original, peer) = pair();
    let ticket = tracked(&mut original, envelope(7));
    let identity = ticket.port_identity();
    // A second owner of even the same underlying socket is not this owner.
    let duplicate = nix::unistd::dup(original.fd()).unwrap();
    let mut replacement = OrganPort::from_owned(duplicate).unwrap();
    let moved = original;
    assert!(identity.matches(&moved));
    assert!(!identity.matches(&replacement));
    drop(moved);
    assert_eq!(replacement.try_send(), Ok(SendProgress::Complete));
    assert!(!ticket.sent());
    assert!(!ticket.withdrawn());
    assert!(!ticket.belongs_to(&replacement));
    drop(replacement);
    drop(peer);
    let (fresh, _fresh_peer) = pair();
    assert!(!identity.matches(&fresh));
}

#[test]
fn an_unobserved_tracked_send_is_refused_before_any_syscall() {
    let (mut port, peer) = pair();
    let ticket = tracked(&mut port, envelope(7));
    let failure = port.try_send().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Undecodable);
    assert!(failure.detail.contains("common pump clock"));
    assert_eq!(
        port.send_with(|_, _, _| panic!("unobserved tracked send reached syscall")),
        Err(failure)
    );
    assert!(
        port.failure().is_none(),
        "API refusal does not break the port"
    );
    assert!(port.send_pending());
    assert!(!ticket.sent());
    assert!(!ticket.withdrawn());
    assert_eq!(ticket.sent_at(), None);
    let mut bytes = [0u8; 1];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(Errno::EAGAIN)
    );
    assert_eq!(
        port.try_send_observed(&mut FakePumpIo::new(0)),
        Ok(SendProgress::Complete)
    );
    assert!(ticket.sent());
    assert_eq!(ticket.sent_at(), Some(0), "zero is a real observed value");
    assert_eq!(
        receive_bytes(&peer),
        serde_json::to_vec(&envelope(7)).unwrap()
    );
}

#[test]
fn post_send_clock_failure_retains_actual_send_without_inventing_a_timestamp() {
    struct BrokenClock;
    impl PumpIo for BrokenClock {
        fn now(&mut self) -> std::io::Result<u64> {
            Err(std::io::Error::other("clock unavailable"))
        }
        fn wait(
            &mut self,
            _: &[crate::flow::clock::WaitInterest<'_>],
            _: Option<u64>,
        ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
            panic!("one send observation never waits")
        }
    }
    let (mut port, peer) = pair();
    let ticket = tracked(&mut port, envelope(7));
    let failure = port.try_send_observed(&mut BrokenClock).unwrap_err();
    assert!(failure.detail.contains("clock observation failed"));
    assert_eq!(port.failure(), Some(&failure));
    assert!(!port.send_pending(), "the syscall sent the whole packet");
    assert!(ticket.sent());
    assert!(!ticket.withdrawn());
    assert_eq!(ticket.sent_at(), None);
    assert_eq!(
        receive_bytes(&peer),
        serde_json::to_vec(&envelope(7)).unwrap()
    );
    assert_eq!(port.withdraw_pending(&envelope(7).exchange), Err(failure));
    assert!(ticket.sent());
    assert_eq!(ticket.sent_at(), None);
}

#[test]
fn tracked_completion_samples_the_clock_after_the_atomic_send() {
    struct AdvancingClock<'a>(&'a std::cell::Cell<u64>);
    impl PumpIo for AdvancingClock<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            Ok(self.0.get())
        }
        fn wait(
            &mut self,
            _: &[crate::flow::clock::WaitInterest<'_>],
            _: Option<u64>,
        ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
            panic!("one send observation never waits")
        }
    }
    let (mut port, peer) = pair();
    let ticket = tracked(&mut port, envelope(7));
    let raw_clock = std::cell::Cell::new(100);
    let mut clock = AdvancingClock(&raw_clock);
    assert_eq!(
        port.send_with_observation(
            |fd, bytes, flags| {
                let result = send(fd, bytes, flags);
                raw_clock.set(110);
                result
            },
            Some(&mut clock),
        ),
        Ok(SendProgress::Complete)
    );
    assert_eq!(ticket.sent_at(), Some(110));
    assert_eq!(
        receive_bytes(&peer),
        serde_json::to_vec(&envelope(7)).unwrap()
    );
}
