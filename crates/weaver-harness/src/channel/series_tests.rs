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

/// The new ask uses the existing segmented transport. No count or schema
/// bytes may be reconstructed from a partial request by its far end.
#[test]
fn a_measurement_proposal_crosses_the_decode_channel_byte_for_byte() {
    use weaver_traits::{ContentBlock, Message, Role};
    use weaver_types::{TokenAnswer, TokenDirective};

    let (channel, far) = DecodeChannel::pair().expect("pair");
    let directive = TokenDirective::MeasureDelta {
        keep: u64::MAX,
        delta: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Measure this input. ".repeat(12_000),
            }],
        }],
        tool_schemas: vec!["first schema".into(), "second schema".into()],
    };
    let expected = directive.clone();
    let answer = TokenAnswer::DeltaMeasured {
        resident: 90,
        prefix_tokens: 20,
        keep: 90,
        input_tokens: 200_000,
        capacity: 4096,
    };
    let expected_answer = answer.clone();
    let peer = std::thread::spawn(move || {
        let first = recv_octets(far.as_fd()).expect("preamble");
        let preamble: weaver_types::SegmentPreamble =
            serde_json::from_slice(&first).expect("the request uses segmentation");
        assert!(preamble.segments > 1);
        let body =
            reassemble_if_series(first, || recv_octets(far.as_fd())).expect("complete proposal");
        assert_eq!(body, serde_json::to_vec(&expected).unwrap());
        assert_eq!(
            serde_json::from_slice::<TokenDirective>(&body).unwrap(),
            expected
        );
        send_octets(far.as_fd(), &serde_json::to_vec(&answer).unwrap()).unwrap();
    });
    channel
        .send_directive(&directive)
        .expect("send complete proposal");
    let reply = channel.recv_reply().expect("decode exact count reply");
    assert!(matches!(reply, DecodeReply::Answer(answer) if answer == expected_answer));
    peer.join().unwrap();
}

/// A complete count body is required. A partial count or an old unit open
/// does not acquire a successful answer from the transport's refusal path.
#[test]
fn incomplete_placement_answers_are_channel_faults() {
    for body in [
        r#"{"kind":"opened"}"#,
        r#"{"kind":"opened","body":{"prefix_tokens":20,"capacity":4096}}"#,
        r#"{"kind":"delta_measured","body":{"resident":90,"prefix_tokens":20,"keep":90,"input_tokens":7}}"#,
    ] {
        let (channel, far) = DecodeChannel::pair().expect("pair");
        send_octets(far.as_fd(), body.as_bytes()).unwrap();
        assert!(matches!(
            channel.recv_reply(),
            Err(ChannelFault::Undecodable)
        ));
    }
}

#[test]
fn a_measurement_refusal_keeps_its_case() {
    for refusal in [
        weaver_types::TokenRefusal::NotOpen,
        weaver_types::TokenRefusal::OutOfOrder,
        weaver_types::TokenRefusal::MalformedDelta,
    ] {
        let (channel, far) = DecodeChannel::pair().expect("pair");
        send_octets(far.as_fd(), &serde_json::to_vec(&refusal).unwrap()).unwrap();
        assert!(
            matches!(channel.recv_reply(), Ok(DecodeReply::Refusal(actual)) if actual == refusal)
        );
    }
}
