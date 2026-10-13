//! Test-only: the `series_tests` suite of `channel.rs`, moved out of the production file by #118.

use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};

/// **A segmented answer arrives whole**, per the decode contract's
/// checkable: the far end sends the preamble and its counted slices,
/// and `recv_reply` hands up the one parsed frame with no layer above
/// the channel seeing a segment.
#[test]
fn a_segmented_answer_arrives_whole() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::empty(),
    )
    .expect("pair");
    let channel = DecodeChannel { end: near };
    let answer = weaver_types::TokenAnswer::Token {
        token: 7,
        piece: "x".repeat(100_000),
    };
    let body = serde_json::to_vec(&answer).expect("renders");
    assert!(
        body.len() > weaver_types::MAX_ENVELOPE_BYTES,
        "the fixture is oversized"
    );
    let preamble = format!(
        "{{\"segments\":{},\"bytes\":{}}}",
        body.len().div_ceil(weaver_types::MAX_ENVELOPE_BYTES),
        body.len()
    );
    send_octets(far.as_fd(), preamble.as_bytes()).expect("preamble");
    for slice in body.chunks(weaver_types::MAX_ENVELOPE_BYTES) {
        send_octets(far.as_fd(), slice).expect("slice");
    }
    match channel.recv_reply().expect("reassembles") {
        DecodeReply::Answer(weaver_types::TokenAnswer::Token { token, piece }) => {
            assert_eq!(token, 7);
            assert_eq!(piece.len(), 100_000, "byte-identical through the series");
        }
        other => panic!("the one frame, parsed: {other:?}"),
    }
}

/// A kindless frame that is not the exact preamble refuses as
/// undecodable rather than being read around, the reserved shape's own
/// rule on this side of the seam.
#[test]
fn a_malformed_preamble_shape_refuses() {
    assert!(matches!(
        reassemble_if_series(b"{\"segments\":2,\"extra\":true}".to_vec(), || panic!()),
        Err(ChannelFault::Undecodable)
    ));
    let through =
        reassemble_if_series(b"{\"kind\":\"not_open\"}".to_vec(), || panic!()).expect("passes");
    assert_eq!(through, b"{\"kind\":\"not_open\"}");
}
