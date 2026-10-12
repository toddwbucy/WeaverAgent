use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};

fn pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("a socketpair")
}

/// **A read returning `MSG_TRUNC` is a channel fault and never a message.**
///
/// Perturbation: delete the `read > buffer.len()` branch from `recv` and
/// this test fails, because the oversized message arrives as a truncated
/// body a parser then sees. Watched under exactly that removal.
#[test]
fn an_oversized_message_is_a_fault_rather_than_a_short_body() {
    let (writer, reader) = pair();
    let channel = from_owned(reader);
    let oversized = vec![b'x'; MAX_ENVELOPE_BYTES + 1];
    send(writer.as_raw_fd(), &oversized, MsgFlags::empty()).expect("the datagram is sent");
    assert_eq!(
        channel.recv(),
        Err(ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES
        })
    );
}

/// A closed peer reads as closure rather than as a message, which is what
/// the exit path of section 2 keys on.
#[test]
fn a_closed_peer_reads_as_closure() {
    let (writer, reader) = pair();
    let channel = from_owned(reader);
    drop(writer);
    assert_eq!(channel.recv(), Err(ChannelFault::Closed));
}

/// A well-formed envelope inside the bound sends clean.
///
/// **This does not exercise the refusal branch, and the name says so.** No
/// payload variant this floor carries can exceed the envelope bound today:
/// the frame and the fault report are both empty shapes, and the directive
/// and answer cases are small by construction. The send-side bound is
/// asserted in the code against the day a payload grows, and the receive
/// side, where a peer can produce an oversized message, carries the watch.
#[test]
fn a_write_within_the_bound_reaches_the_socket() {
    use weaver_types::{ExchangeId, Opener, Payload, Position, TurnFrame};
    let (writer, _reader) = pair();
    let channel = from_owned(writer);
    // A frame is the only payload that could grow; the bound is asserted
    // on the encoded envelope whatever carries it.
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal: 1,
        },
        position: Position::Open,
        payload: Payload::Frame(TurnFrame::carry(b"{\"text\":\"hello\"}")),
    };
    assert_eq!(channel.send(&envelope), Ok(()));
}
