use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};

fn seqpacket_pair() -> (OwnedFd, OwnedFd) {
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
/// A message larger than the envelope bound is written directly to the
/// socket, bypassing this crate's own send-side bound, which is exactly the
/// case a misbehaving or mismatched peer produces. The receiver must fault
/// rather than hand a silently shortened body to a parser.
///
/// Perturbation: delete the `read > buffer.len()` branch from `recv_octets`
/// and this test fails, because the oversized message then returns as a
/// truncated body instead of a fault. Watched under exactly that removal.
#[test]
fn an_oversized_message_is_a_fault_rather_than_a_short_body() {
    let (writer, reader) = seqpacket_pair();
    let socket = decode_from_owned(reader);

    let oversized = vec![b'x'; MAX_ENVELOPE_BYTES + 1];
    // Written past this crate's own send bound on purpose: the receive
    // obligation is what is under test, not the send assertion.
    send(writer.as_raw_fd(), &oversized, MsgFlags::empty()).expect("the datagram is sent");

    assert_eq!(
        socket.recv_octets(),
        Err(ChannelFault::Truncated {
            bound: MAX_ENVELOPE_BYTES
        }),
        "an oversized datagram faults rather than arriving short, naming the bound"
    );
}

/// A message of exactly the bound fits and reads clean, which is what makes
/// the buffer exactly the bound rather than the bound plus one.
#[test]
fn a_message_of_exactly_the_bound_reads_clean() {
    let (writer, reader) = seqpacket_pair();
    let socket = decode_from_owned(reader);

    let exact = vec![b'y'; MAX_ENVELOPE_BYTES];
    send(writer.as_raw_fd(), &exact, MsgFlags::empty()).expect("the datagram is sent");

    assert_eq!(socket.recv_octets(), Ok(exact));
}

/// **This crate asserts the bound on its own writes too**, and since the
/// segment series the bound it asserts is the total one: a write past
/// the envelope segments rather than refusing, per the decode
/// contract's amendment on issue #236, and a write past
/// `DECODE_MESSAGE_BOUND` is this side's own fault, refused before the
/// socket.
///
/// Perturbation: delete the total-bound branch from
/// `DecodeSocket::send_octets` and this test fails. Watched under
/// exactly that removal.
#[test]
fn a_write_past_the_bound_refuses_before_it_reaches_the_socket() {
    let (writer, reader) = seqpacket_pair();
    // A removed bound must fail this instrument, not fill the socket and hang.
    nix::sys::socket::setsockopt(
        &writer,
        nix::sys::socket::sockopt::SendTimeout,
        &nix::sys::time::TimeVal::new(5, 0),
    )
    .unwrap();
    let socket = decode_from_owned(writer);
    let oversized = vec![b'z'; DECODE_MESSAGE_BOUND + 1];
    assert_eq!(
        socket.send_octets(&oversized),
        Err(ChannelFault::Truncated {
            bound: DECODE_MESSAGE_BOUND
        })
    );
    assert_eq!(
        recv(reader.as_raw_fd(), &mut [0_u8; 128], MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "refusal must precede any segment preamble"
    );
}
