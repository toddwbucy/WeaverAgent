//! Test-only: the `series_tests` suite of `channel.rs`, moved out of the production file by #118.

use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use std::os::fd::OwnedFd;

fn seqpacket_pair() -> (OwnedFd, OwnedFd) {
    let (a, b) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::empty(),
    )
    .expect("pair");
    (a, b)
}

/// **A close past the envelope arrives byte-identical**, the contract's
/// checkable: the series is the channel's fact and the caller sees the
/// one frame, watched to fail when the carriage is removed.
#[test]
fn an_oversized_frame_round_trips_byte_identical() {
    let (a, b) = seqpacket_pair();
    let sender = DecodeSocket { end: a };
    let receiver = DecodeSocket { end: b };
    let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    sender.send_octets(&body).expect("segments");
    let back = receiver.recv_octets().expect("reassembles");
    assert_eq!(back, body, "byte-identical through the series");
    // And a small frame still crosses as one write.
    sender
        .send_octets(b"{\"kind\":\"at_rest\"}")
        .expect("plain");
    assert_eq!(
        receiver.recv_octets().expect("plain"),
        b"{\"kind\":\"at_rest\"}"
    );
}

/// **An interrupted series faults whole**: a peer closing before its
/// declared count leaves no partial frame, the contract's checkable.
#[test]
fn an_interrupted_series_faults_whole() {
    let (a, b) = seqpacket_pair();
    let receiver = DecodeSocket { end: b };
    {
        let sender = DecodeSocket { end: a };
        let preamble = format!("{{\"segments\":4,\"bytes\":{}}}", 200_000);
        super::send_octets(sender.end.as_fd(), preamble.as_bytes()).expect("preamble");
        super::send_octets(sender.end.as_fd(), &[7u8; MAX_ENVELOPE_BYTES]).expect("one");
        // the sender dies here, two slices short
    }
    assert!(matches!(receiver.recv_octets(), Err(ChannelFault::Closed)));
}

/// The preamble validates whole before any slice is read: the kindless
/// shape is reserved, and every inconsistent spelling refuses.
#[test]
fn the_preamble_validates_before_any_slice() {
    let fault = |first: &str| {
        matches!(
            reassemble_if_series(first.as_bytes().to_vec(), || panic!("no slice read")),
            Err(ChannelFault::Undecodable)
        )
    };
    // a kindful frame passes through untouched
    let through = reassemble_if_series(b"{\"kind\":\"at_rest\"}".to_vec(), || {
        panic!("no slice read")
    })
    .expect("passes");
    assert_eq!(through, b"{\"kind\":\"at_rest\"}");
    // non-JSON and non-object pass through for the caller to judge
    assert!(reassemble_if_series(b"not json".to_vec(), || panic!()).is_ok());
    assert!(reassemble_if_series(b"[1,2]".to_vec(), || panic!()).is_ok());
    // kindless and not the exact preamble: refused, never guessed
    assert!(fault("{\"segments\":2}"), "one member");
    assert!(
        fault("{\"segments\":2,\"bytes\":200000,\"extra\":1}"),
        "three members"
    );
    assert!(
        fault("{\"segments\":-2,\"bytes\":200000}"),
        "negative count"
    );
    // bytes within one envelope: the sender had no series to send
    assert!(fault("{\"segments\":1,\"bytes\":100}"));
    // bytes past the total bound
    assert!(fault(&format!(
        "{{\"segments\":129,\"bytes\":{}}}",
        DECODE_MESSAGE_BOUND + 1
    )));
    // a count inconsistent with the length
    assert!(fault("{\"segments\":5,\"bytes\":200000}"), "too many");
    assert!(fault("{\"segments\":2,\"bytes\":200000}"), "too few");
}

/// A series whose slices exceed or fall short of the declared bytes is
/// a channel fault, never a partial frame.
#[test]
fn a_series_totals_exactly_its_declared_bytes() {
    let slices = vec![vec![1u8; MAX_ENVELOPE_BYTES], vec![2u8; 10]];
    let mut feed = slices.clone().into_iter();
    let preamble = format!("{{\"segments\":2,\"bytes\":{}}}", MAX_ENVELOPE_BYTES + 10);
    let whole = reassemble_if_series(preamble.into_bytes(), || Ok(feed.next().expect("scripted")))
        .expect("exact total reassembles");
    assert_eq!(whole.len(), MAX_ENVELOPE_BYTES + 10);
    let mut feed = vec![vec![1u8; MAX_ENVELOPE_BYTES], vec![2u8; 11]].into_iter();
    let preamble = format!("{{\"segments\":2,\"bytes\":{}}}", MAX_ENVELOPE_BYTES + 10);
    assert!(matches!(
        reassemble_if_series(preamble.into_bytes(), || Ok(feed.next().expect("scripted"))),
        Err(ChannelFault::Undecodable)
    ));
}
