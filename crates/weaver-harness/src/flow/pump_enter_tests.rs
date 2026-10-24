use super::super::super::clock::testing::FakePumpIo;
use super::*;
use nix::sys::socket::{AddressFamily, ControlMessage, SockType, sendmsg, socketpair};
use std::io::{IoSlice, Read, Write};
use std::os::unix::net::UnixStream;

const FIXTURE: &[u8] = include_bytes!("../../../weaver-types/tests/fixtures/state-enter.json");
fn pair() -> (OwnedFd, OwnedFd) {
    socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap()
}
fn receiver() -> (EnterReceiver, OwnedFd) {
    let (end, peer) = pair();
    let owner = EnterReceiver::from_peer(LifecyclePeer::from_owned(end).unwrap())
        .ok()
        .unwrap();
    (owner, peer)
}
fn send(peer: &OwnedFd, bytes: &[u8], fds: &[RawFd]) {
    sendmsg::<()>(
        peer.as_raw_fd(),
        &[IoSlice::new(bytes)],
        &[ControlMessage::ScmRights(fds)],
        nix::sys::socket::MsgFlags::MSG_NOSIGNAL,
        None,
    )
    .unwrap();
}
fn received(owner: EnterReceiver) -> ReceivedEnter {
    owner.into_received().ok().unwrap()
}
#[test]
fn replacement_enter_retains_actual_bytes_descriptor_order_and_original_peer() {
    let (mut owner, peer) = receiver();
    let (sink, mut sink_peer) = UnixStream::pair().unwrap();
    let (state, mut state_peer) = UnixStream::pair().unwrap();
    send(&peer, FIXTURE, &[sink.as_raw_fd(), state.as_raw_fd()]);
    drop((sink, state));
    assert_eq!(owner.advance().unwrap(), ReceiveProgress::Held);
    send(&peer, FIXTURE, &[]);
    assert_eq!(owner.advance().unwrap(), ReceiveProgress::Pending);
    let first = received(owner);
    assert_eq!(first.wire(), FIXTURE);
    assert_eq!(first.envelope().exchange.ordinal, 7);
    let (peer_owner, message, _) = first.into_parts();
    for fd in [&message.sink, &message.state] {
        assert!(
            nix::fcntl::FdFlag::from_bits_retain(
                fcntl(fd.as_ref().unwrap(), FcntlArg::F_GETFD).unwrap()
            )
            .contains(nix::fcntl::FdFlag::FD_CLOEXEC)
        );
    }
    let mut sink = UnixStream::from(message.sink.unwrap());
    let mut state = UnixStream::from(message.state.unwrap());
    sink.write_all(b"trace").unwrap();
    state.write_all(b"state").unwrap();
    let mut bytes = [0; 5];
    sink_peer.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"trace");
    state_peer.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"state");
    let mut second = EnterReceiver::from_peer(peer_owner).ok().unwrap();
    assert_eq!(second.advance().unwrap(), ReceiveProgress::Held);
}
#[test]
fn replacement_enter_adopts_one_recording_context_from_the_received_handles() {
    let (mut owner, peer) = receiver();
    let (sink, mut sink_peer) = UnixStream::pair().unwrap();
    let (state, mut state_peer) = UnixStream::pair().unwrap();
    send(&peer, FIXTURE, &[sink.as_raw_fd(), state.as_raw_fd()]);
    drop((sink, state));
    owner.advance().unwrap();
    let source = received(owner);
    let mut clock = FakePumpIo::new(123);
    let adopted = source
        .adopt(5000, &mut clock)
        .map_err(|f| f.detail)
        .unwrap();
    let (mut context, source) = adopted.into_parts();
    assert_eq!(context.origin_ns(), 123);
    assert_eq!(source.wire(), FIXTURE);
    assert!(source.message.sink.is_none() && source.message.state.is_none());
    let weaver_types::state_flow::enter::EnterBinding::Serving {
        start,
        state_management,
        ..
    } = &source.envelope().enter().binding
    else {
        panic!()
    };
    context
        .begin_open(start.clone(), state_management.reads.clone(), &mut clock)
        .unwrap();
    clock
        .schedule(
            124,
            vec![super::super::super::clock::ReadyFd {
                index: 0,
                events: PollFlags::POLLOUT,
            }],
        )
        .unwrap();
    context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut clock,
        })
        .unwrap();
    state_peer.set_nonblocking(true).unwrap();
    let mut bytes = [0; 65536];
    let count = state_peer.read(&mut bytes).unwrap();
    let text = std::str::from_utf8(&bytes[..count]).unwrap();
    assert!(text.contains("carrier-session") && text.contains("carrier-run"));
    assert!(text.contains(start.dump_sha256.as_str()));
    assert!(text.contains(&state_management.flow.scheme.scheme_id));
    sink_peer.set_nonblocking(true).unwrap();
    assert_eq!(
        sink_peer.read(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::WouldBlock,
        "adoption/open must not fabricate a trace load or Ready"
    );
}
#[test]
fn replacement_enter_adoption_refusals_retain_original_bytes_and_unconsumed_handles() {
    for case in [
        "missing-state",
        "wrong-store",
        "wrong-config",
        "wrong-dump",
        "diagnostic",
        "dead-peer",
    ] {
        let (mut owner, peer) = receiver();
        let (sink, _sink_peer) = UnixStream::pair().unwrap();
        let (state, _state_peer) = UnixStream::pair().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        let payload = &mut value["payload"]["body"]["payload"];
        match case {
            "wrong-store" => payload["state_store"]["engine"] = serde_json::json!("none"),
            "wrong-config" => payload["declaration"] = serde_json::json!("wrong"),
            "wrong-dump" => {
                payload["binding"]["start"]["dump_sha256"] = serde_json::json!("c".repeat(64))
            }
            "diagnostic" => {
                payload["binding"] = serde_json::json!({"kind":"diagnostic","state_election":weaver_types::StateElection::default()})
            }
            _ => (),
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        let handles = if case == "missing-state" {
            vec![sink.as_raw_fd()]
        } else {
            vec![sink.as_raw_fd(), state.as_raw_fd()]
        };
        send(&peer, &bytes, &handles);
        owner.advance().unwrap();
        if case == "dead-peer" {
            drop(peer);
        }
        let refusal = received(owner)
            .adopt(1000, &mut FakePumpIo::new(1))
            .err()
            .unwrap();
        assert!(!refusal.detail.is_empty());
        assert_eq!(refusal.source.wire(), bytes);
        assert!(refusal.source.message.sink.is_some(), "{case}");
    }
}
#[test]
fn replacement_enter_malformed_and_truncated_packets_close_received_handles() {
    for bytes in [b"malformed".to_vec(), vec![b'x'; MAX_ENVELOPE_BYTES + 1]] {
        let (mut owner, peer) = receiver();
        let (sink, mut sink_peer) = UnixStream::pair().unwrap();
        send(&peer, &bytes, &[sink.as_raw_fd()]);
        drop(sink);
        assert!(owner.advance().is_err());
        sink_peer.set_nonblocking(true).unwrap();
        assert_eq!(sink_peer.read(&mut [0]).unwrap(), 0);
        assert!(owner.into_received().is_err());
    }
}
const _: fn() = || {
    trait NoClone<M> {
        fn check() {}
    }
    impl<T: ?Sized> NoClone<()> for T {}
    impl<T: Clone> NoClone<u8> for T {}
    let _ = <ReceivedEnter as NoClone<_>>::check;
    let _ = <AdoptedEnter as NoClone<_>>::check;
};
