use super::*;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use serde_json::json;
use std::os::fd::AsRawFd;

#[test]
fn replacement_observe_correlates_and_retains_refusals() {
    for case in [
        "unloaded",
        "wrong-exchange",
        "wrong-position",
        "refusal",
        "bad",
        "oversize",
    ] {
        let (end, peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let worker = std::thread::spawn(move || {
            let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
            let n = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap();
            let request: weaver_types::OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
            assert_eq!(
                request.payload,
                weaver_types::Payload::Directive(LifecycleDirective::Observe)
            );
            assert_eq!(request.position, weaver_types::Position::Open);
            let mut value = json!({"exchange":request.exchange,"position":"close",
                "payload":{"kind":"answer","body":{"kind":"state","state":"unloaded"}}});
            if case == "wrong-exchange" {
                value["exchange"]["ordinal"] = json!(77);
            }
            if case == "wrong-position" {
                value["position"] = json!("open");
            }
            if case == "refusal" {
                value["payload"] = json!({"kind":"refusal","body":{"kind":"out_of_order"}});
            }
            let bytes = match case {
                "bad" => b"{".to_vec(),
                "oversize" => vec![b'x'; weaver_types::MAX_ENVELOPE_BYTES + 1],
                _ => serde_json::to_vec(&value).unwrap(),
            };
            assert_eq!(
                send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
                bytes.len()
            );
        });
        let result = ask(&mut Coordination::adopt(end));
        match (case, result) {
            (
                "unloaded",
                Ok(ObservationAnswer::State {
                    state: weaver_types::AgentState::Unloaded,
                    load: None,
                    ..
                }),
            )
            | ("refusal", Err(Failure::Refused(LifecycleRefusal::OutOfOrder)))
            | ("oversize", Err(Failure::Channel(ChannelFault::Truncated { .. }))) => (),
            (
                "wrong-exchange" | "wrong-position" | "bad",
                Err(Failure::Channel(ChannelFault::Undecodable)),
            ) => (),
            other => panic!("{other:?}"),
        }
        worker.join().unwrap();
    }
}
#[test]
fn replacement_observe_read_uses_its_original_bound() {
    let (end, _peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let channel = Coordination::adopt(end);
    assert_eq!(
        channel
            .recv_observation_within(std::time::Duration::ZERO)
            .unwrap_err(),
        ChannelFault::Unanswered
    );
}

/// Invoked by the harness circuit as a separate process. The actual dial checks
/// both listener UID and its held run.lock; no crate dependency or adopt bypass.
#[test]
fn replacement_observe_actual_admin_process() {
    let Ok(root) = std::env::var("WEAVER_OBSERVE_CIRCUIT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let mut channel = crate::channel::dial(&root.join("admin.sock"), 0, root).unwrap();
    let answer = ask(&mut channel).unwrap();
    std::fs::write(
        root.join("admin-observed.json"),
        serde_json::to_vec(&answer).unwrap(),
    )
    .unwrap();
}
