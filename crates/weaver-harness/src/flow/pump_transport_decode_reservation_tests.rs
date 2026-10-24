//! Reserved measurement transport uses actual socket bytes for every reply.

use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, recv, socketpair};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{FaultCase, FaultReport, TokenRefusal};

const _: fn() = || {
    trait NoClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <DecodeReservation as NoClone<_>>::check;
    let _ = <ReceivedDecode as NoClone<_>>::check;
    trait NoDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NoDefault<()> for T {}
    impl<T: Default> NoDefault<u8> for T {}
    let _ = <DecodeReservation as NoDefault<_>>::check;
    let _ = <ReceivedDecode as NoDefault<_>>::check;
};

fn pair() -> (DecodePort, OwnedFd) {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    (DecodePort::from_owned(near).unwrap(), far)
}

fn request(size: usize) -> TokenDirective {
    TokenDirective::MeasureDelta {
        keep: 90,
        delta: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "r".repeat(size),
            }],
        }],
        tool_schemas: vec!["schema".into()],
    }
}

fn reserve(port: &mut DecodePort, input: TokenDirective) -> DecodeReservation {
    match port.stage_reserved(input).unwrap() {
        DecodeStage::Queued(ticket) => ticket,
        DecodeStage::Busy(_) => panic!("fixture requires an empty port"),
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

fn recv_bytes(peer: &OwnedFd) -> Option<Vec<u8>> {
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    match recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(count) => {
            bytes.truncate(count);
            Some(bytes)
        }
        Err(Errno::EAGAIN) => None,
        Err(error) => panic!("fixture receive: {error}"),
    }
}

fn drain_send(port: &mut DecodePort, peer: &OwnedFd) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    for _ in 0..16 {
        if !port.send_pending() {
            return packets;
        }
        assert_ne!(port.try_send().unwrap(), SendProgress::Pending);
        packets.push(recv_bytes(peer).expect("successful send reaches peer"));
    }
    panic!("fixture send did not finish")
}

fn measured() -> Vec<u8> {
    b" \n{\"kind\":\"delta_measured\", \"body\":{\"resident\":90,\"prefix_tokens\":20,\"keep\":90,\"input_token\\u0073\":7,\"capacity\":4096}}\t".to_vec()
}

fn reply(port: &mut DecodePort, peer: &OwnedFd, bytes: &[u8]) {
    send_bytes(peer, bytes);
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Held));
}

fn preamble(total: usize) -> Vec<u8> {
    serde_json::to_vec(&SegmentPreamble {
        segments: total.div_ceil(MAX_ENVELOPE_BYTES) as u64,
        bytes: total as u64,
    })
    .unwrap()
}

#[test]
fn reservation_keeps_exact_reply_bytes_and_only_finishes_after_terminal_transfer() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(9));
    assert!(ticket.belongs_to(&port));
    assert!(!ticket.sent());
    let packets = drain_send(&mut port, &peer);
    assert_eq!(
        serde_json::from_slice::<TokenDirective>(&packets[0]).unwrap(),
        request(9)
    );
    assert!(ticket.sent());
    let bytes = measured();
    reply(&mut port, &peer, &bytes);
    let ticket = port.finish_reserved(ticket).unwrap_err().reservation;
    let result = port.take_reserved(&ticket).unwrap().unwrap();
    assert!(result.belongs_to(&port));
    assert!(result.matches(&ticket));
    assert!(result.after_send());
    assert_eq!(result.raw_bytes(), bytes);
    assert!(matches!(
        result.reply(),
        DecodeReply::Answer(TokenAnswer::DeltaMeasured {
            input_tokens: 7,
            ..
        })
    ));
    assert!(port.take_reserved(&ticket).unwrap().is_none());
    port.finish_reserved(ticket).unwrap();
    let (typed, raw) = result.into_parts();
    assert!(matches!(
        typed,
        DecodeReply::Answer(TokenAnswer::DeltaMeasured { .. })
    ));
    assert_eq!(raw, bytes);
    assert!(port.take_completion().unwrap().is_none());
}

#[test]
fn generic_stage_and_take_cannot_steal_the_reserved_wait_or_held_reply() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(9));
    drain_send(&mut port, &peer);
    let next = request(10);
    assert_eq!(port.stage(next.clone()).unwrap(), Stage::Busy(next.clone()));
    assert!(
        matches!(port.stage_reserved(next.clone()).unwrap(), DecodeStage::Busy(got) if got == next)
    );
    reply(&mut port, &peer, &measured());
    assert!(port.take_completion().is_err());
    assert!(
        port.failure().is_none(),
        "ownership refusal is not a transport fault"
    );
    assert!(port.held().is_some());
    port.take_reserved(&ticket).unwrap().unwrap();
    port.finish_reserved(ticket).unwrap();
    let next_ticket = reserve(&mut port, next);
    assert!(!next_ticket.sent());
}

#[test]
fn foreign_ticket_neither_consumes_nor_releases_either_socket() {
    let (mut a, peer_a) = pair();
    let (mut b, peer_b) = pair();
    let ta = reserve(&mut a, request(1));
    let tb = reserve(&mut b, request(2));
    drain_send(&mut a, &peer_a);
    drain_send(&mut b, &peer_b);
    reply(&mut a, &peer_a, &measured());
    reply(&mut b, &peer_b, &measured());
    assert!(!ta.belongs_to(&b));
    assert!(b.take_reserved(&ta).is_err());
    let ta = b.finish_reserved(ta).unwrap_err().reservation;
    assert!(a.held().is_some() && b.held().is_some());
    assert!(a.failure().is_none() && b.failure().is_none());
    let ra = a.take_reserved(&ta).unwrap().unwrap();
    assert!(!ra.matches(&tb));
    assert!(!ra.belongs_to(&b));
    b.take_reserved(&tb).unwrap().unwrap();
    a.finish_reserved(ta).unwrap();
    b.finish_reserved(tb).unwrap();
}

#[test]
fn previous_reply_cannot_name_a_later_reservation_on_the_same_socket() {
    let (mut port, peer) = pair();
    let first = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    reply(&mut port, &peer, &measured());
    let old = port.take_reserved(&first).unwrap().unwrap();
    port.finish_reserved(first).unwrap();
    let next = reserve(&mut port, request(2));
    assert!(old.belongs_to(&port));
    assert!(!old.matches(&next));
}

#[test]
fn partial_request_never_proves_sent_and_cannot_be_cancelled_by_finish() {
    let (mut port, peer) = pair();
    let input = request(MAX_ENVELOPE_BYTES * 2 + 47);
    let expected = serde_json::to_vec(&input).unwrap();
    let mut ticket = reserve(&mut port, input);
    ticket = port.finish_reserved(ticket).unwrap_err().reservation;
    assert_eq!(port.try_send(), Ok(SendProgress::Progress));
    let head: SegmentPreamble = serde_json::from_slice(&recv_bytes(&peer).unwrap()).unwrap();
    assert_eq!(head.bytes, expected.len() as u64);
    assert!(!ticket.sent());
    ticket = port.finish_reserved(ticket).unwrap_err().reservation;
    let mut body = Vec::new();
    for part in 0..head.segments {
        let progress = port.try_send().unwrap();
        body.extend(recv_bytes(&peer).unwrap());
        if part + 1 < head.segments {
            assert_eq!(progress, SendProgress::Progress);
            assert!(!ticket.sent());
            ticket = port.finish_reserved(ticket).unwrap_err().reservation;
        } else {
            assert_eq!(progress, SendProgress::Complete);
            assert!(ticket.sent());
        }
    }
    assert_eq!(body, expected);
    ticket = port.finish_reserved(ticket).unwrap_err().reservation;
    reply(&mut port, &peer, &measured());
    port.take_reserved(&ticket).unwrap().unwrap();
    port.finish_reserved(ticket).unwrap();
}

#[test]
fn fault_emission_does_not_close_the_reserved_exchange() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    let fault = serde_json::to_vec(&TokenAnswer::Fault(FaultReport {
        case: FaultCase::ResidencyDegraded,
        account: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
    }))
    .unwrap();
    reply(&mut port, &peer, &fault);
    let received = port.take_reserved(&ticket).unwrap().unwrap();
    assert_eq!(received.raw_bytes(), fault);
    let ticket = port.finish_reserved(ticket).unwrap_err().reservation;
    reply(&mut port, &peer, &measured());
    port.take_reserved(&ticket).unwrap().unwrap();
    port.finish_reserved(ticket).unwrap();
}

#[test]
fn wrong_answer_is_retained_and_a_later_right_answer_cannot_erase_it() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    let wrong = serde_json::to_vec(&TokenAnswer::AtRest).unwrap();
    reply(&mut port, &peer, &wrong);
    let received = port.take_reserved(&ticket).unwrap().unwrap();
    assert_eq!(received.raw_bytes(), wrong);
    let ticket = port.finish_reserved(ticket).unwrap_err().reservation;
    reply(&mut port, &peer, &measured());
    port.take_reserved(&ticket).unwrap().unwrap();
    assert!(port.finish_reserved(ticket).is_err());
}

#[test]
fn duplicate_terminal_cannot_become_a_second_settlement() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    for _ in 0..2 {
        reply(&mut port, &peer, &measured());
        port.take_reserved(&ticket).unwrap().unwrap();
    }
    assert!(port.finish_reserved(ticket).is_err());
}

#[test]
fn premature_whole_reply_stays_premature_after_the_request_finishes() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    reply(&mut port, &peer, &measured());
    drain_send(&mut port, &peer);
    let result = port.take_reserved(&ticket).unwrap().unwrap();
    assert!(ticket.sent());
    assert!(!result.after_send());
    assert!(port.finish_reserved(ticket).is_err());
}

#[test]
fn reply_preamble_before_whole_send_cannot_be_laundered_by_late_segments() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    let mut large = measured();
    large.extend(vec![b' '; MAX_ENVELOPE_BYTES]);
    send_bytes(&peer, &preamble(large.len()));
    assert_eq!(port.try_receive(), Ok(ReceiveProgress::Progress));
    drain_send(&mut port, &peer);
    for part in large.chunks(MAX_ENVELOPE_BYTES) {
        send_bytes(&peer, part);
        port.try_receive().unwrap();
    }
    let received = port.take_reserved(&ticket).unwrap().unwrap();
    assert_eq!(received.raw_bytes(), large);
    assert!(!received.after_send());
    assert!(port.finish_reserved(ticket).is_err());
}

#[test]
fn complete_reply_and_exact_bytes_survive_later_eof_without_thawing_port() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    let bytes = measured();
    reply(&mut port, &peer, &bytes);
    shutdown(peer.as_raw_fd(), Shutdown::Write).unwrap();
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Closed);
    assert!(port.failed_wire_reserved(&ticket).unwrap().is_none());
    let received = port.take_reserved(&ticket).unwrap().unwrap();
    assert_eq!(received.raw_bytes(), bytes);
    port.finish_reserved(ticket).unwrap();
    assert_eq!(port.failure(), Some(&failure));
    assert!(port.stage_reserved(request(2)).is_err());
}

#[test]
fn malformed_reply_evidence_is_owned_once_and_cannot_finish_the_exchange() {
    let (mut port, peer) = pair();
    let (other, _other_peer) = pair();
    let ticket = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    let bytes = b" {\"kind\": ";
    send_bytes(&peer, bytes);
    let failure = port.try_receive().unwrap_err();
    assert_eq!(failure.fault, ChannelFault::Undecodable);
    assert!(other.failed_wire_reserved(&ticket).is_err());
    assert_eq!(
        port.failed_wire_reserved(&ticket).unwrap(),
        Some(bytes.as_slice())
    );
    assert_eq!(
        port.take_failed_wire_reserved(&ticket).unwrap().unwrap(),
        bytes
    );
    assert!(port.take_failed_wire_reserved(&ticket).unwrap().is_none());
    assert!(port.take_reserved(&ticket).is_err());
    assert!(port.finish_reserved(ticket).is_err());
    assert_eq!(port.failure(), Some(&failure));
}

#[test]
fn preexisting_unreserved_work_is_never_adopted_by_a_reservation() {
    let (mut port, peer) = pair();
    let old = TokenDirective::Open {
        session: weaver_types::SessionId("session".into()),
        messages: Vec::new(),
        column_ask: false,
    };
    assert_eq!(port.stage(old.clone()).unwrap(), Stage::Queued);
    assert!(matches!(
        port.stage_reserved(request(1)).unwrap(),
        DecodeStage::Busy(_)
    ));
    drain_send(&mut port, &peer);
    let opened = serde_json::to_vec(&TokenAnswer::Opened(weaver_types::Opened {
        session: weaver_types::SessionId("session".into()),
        prefix_tokens: 20,
        capacity: 4096,
        generation_limit: 10,
        terminal_tokens: 1,
        request: serde_json::value::to_raw_value(&serde_json::json!({})).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({})).unwrap(),
    }))
    .unwrap();
    reply(&mut port, &peer, &opened);
    assert!(matches!(
        port.stage_reserved(request(1)).unwrap(),
        DecodeStage::Busy(_)
    ));
    assert!(matches!(
        port.take_completion().unwrap(),
        Some(DecodeReply::Answer(TokenAnswer::Opened(_)))
    ));
    let large = serde_json::to_vec(&TokenAnswer::Fault(FaultReport {
        case: FaultCase::ResidencyDegraded,
        account: serde_json::value::to_raw_value(&"r".repeat(MAX_ENVELOPE_BYTES)).unwrap(),
    }))
    .unwrap();
    send_bytes(&peer, &preamble(large.len()));
    port.try_receive().unwrap();
    assert!(matches!(
        port.stage_reserved(request(1)).unwrap(),
        DecodeStage::Busy(_)
    ));
    for part in large.chunks(MAX_ENVELOPE_BYTES) {
        send_bytes(&peer, part);
        port.try_receive().unwrap();
    }
    assert!(port.take_completion().unwrap().is_some());
    reserve(&mut port, request(1));
}

#[test]
fn every_measurement_requires_the_reserved_entry_point() {
    let (mut port, peer) = pair();
    let input = request(37);
    let error = port.stage(input.clone()).unwrap_err();
    assert_eq!(error.input, input);
    assert_eq!(error.failure.fault, ChannelFault::Undecodable);
    assert!(port.failure().is_none());
    assert!(!port.send_pending());
    assert!(recv_bytes(&peer).is_none());
    let ticket = reserve(&mut port, error.input);
    drain_send(&mut port, &peer);
    assert_eq!(
        port.stage(input.clone()).unwrap(),
        Stage::Busy(input.clone())
    );
    reply(&mut port, &peer, &measured());
    port.take_reserved(&ticket).unwrap().unwrap();
    port.finish_reserved(ticket).unwrap();
    let error = port.stage(input.clone()).unwrap_err();
    assert_eq!(error.input, input);
    assert!(port.failure().is_none());
    assert!(recv_bytes(&peer).is_none());
    let second = reserve(&mut port, error.input);
    assert!(!second.sent());
}

#[test]
fn dropping_unsettled_ticket_exposes_failure_and_never_unlocks_or_erases_bytes() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    reply(&mut port, &peer, &measured());
    drop(ticket);
    assert!(port.failure().is_some());
    assert!(!port.interest().live);
    assert!(port.held().is_some());
    assert_eq!(port.received_wire.as_deref(), Some(measured().as_slice()));
    assert!(port.take_completion().is_err());
    assert!(port.stage_reserved(request(2)).is_err());
    assert!(port.decode_reservation.is_some());
}

#[test]
fn reservation_allows_open_or_measurement_and_preserves_other_owners() {
    let (mut port, peer) = pair();
    let input = TokenDirective::Flush { keep: 9 };
    let failure = port.stage_reserved(input.clone()).unwrap_err();
    assert_eq!(failure.input, input);
    assert!(port.failure().is_none());
    assert!(!port.send_pending());
    assert!(recv_bytes(&peer).is_none());
    assert_eq!(port.stage(input).unwrap(), Stage::Queued);
}

#[test]
fn typed_refusal_after_whole_send_is_a_terminal_but_not_a_successful_measurement() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, request(1));
    drain_send(&mut port, &peer);
    let bytes = serde_json::to_vec(&TokenRefusal::MalformedDelta).unwrap();
    reply(&mut port, &peer, &bytes);
    let received = port.take_reserved(&ticket).unwrap().unwrap();
    assert!(matches!(
        received.reply(),
        DecodeReply::Refusal(TokenRefusal::MalformedDelta)
    ));
    assert_eq!(received.raw_bytes(), bytes);
    port.finish_reserved(ticket).unwrap();
}

fn boot_request(size: usize) -> TokenDirective {
    TokenDirective::Open {
        session: weaver_types::SessionId("session".into()),
        messages: vec![Message {
            role: Role::System,
            content: vec![ContentBlock::Text {
                text: "x".repeat(size),
            }],
        }],
        column_ask: false,
    }
}
fn opened_wire() -> Vec<u8> {
    serde_json::to_vec(&TokenAnswer::Opened(weaver_types::Opened {
        session: weaver_types::SessionId("session".into()),
        prefix_tokens: 0,
        capacity: 100,
        generation_limit: 1,
        terminal_tokens: 1,
        request: serde_json::value::to_raw_value(&serde_json::json!({})).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({})).unwrap(),
    }))
    .unwrap()
}
#[test]
fn boot_reservation_can_withdraw_only_before_any_preamble_or_payload_byte() {
    let (mut port, peer) = pair();
    let ticket = reserve(&mut port, boot_request(MAX_ENVELOPE_BYTES));
    assert!(port.open_not_started());
    port.withdraw_unsent(ticket).unwrap();
    assert!(recv_bytes(&peer).is_none());
    assert!(!port.send_pending());
    assert!(port.failure().is_none());
    let ticket = reserve(&mut port, boot_request(MAX_ENVELOPE_BYTES));
    port.try_send().unwrap();
    let preamble = recv_bytes(&peer).unwrap();
    assert!(serde_json::from_slice::<SegmentPreamble>(&preamble).is_ok());
    assert!(!ticket.sent());
    assert!(!port.open_not_started());
    let error = port.withdraw_unsent(ticket).unwrap_err();
    assert!(error.reservation.belongs_to(&port));
    let packets = drain_send(&mut port, &peer);
    assert_eq!(packets.len(), 2);
    assert!(error.reservation.sent());
    reply(&mut port, &peer, &opened_wire());
    assert!(matches!(
        port.take_reserved(&error.reservation)
            .unwrap()
            .unwrap()
            .reply(),
        DecodeReply::Answer(TokenAnswer::Opened(_))
    ));
    port.finish_reserved(error.reservation).unwrap();
}
#[test]
fn open_and_measurement_reservations_reject_each_others_terminal() {
    for open in [false, true] {
        let (mut port, peer) = pair();
        let ticket = reserve(&mut port, if open { boot_request(1) } else { request(1) });
        drain_send(&mut port, &peer);
        reply(
            &mut port,
            &peer,
            &if open { measured() } else { opened_wire() },
        );
        assert!(port.take_reserved(&ticket).unwrap().is_some());
        let error = port.finish_reserved(ticket).unwrap_err();
        assert!(error.reservation.belongs_to(&port));
        assert!(port.decode_reservation.is_some());
    }
}
