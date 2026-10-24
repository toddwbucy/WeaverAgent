use super::*;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use std::os::fd::AsRawFd;
use weaver_types::{OrganEnvelope, TurnKey};

#[test]
fn stop_exchange_checks_original_identity_position_and_answer_kind() {
    for case in [
        "at-rest",
        "aborted",
        "wrong-id",
        "wrong-opener",
        "wrong-position",
        "wrong-answer",
        "refusal",
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
            let request: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
            assert_eq!(
                request.payload,
                Payload::Directive(LifecycleDirective::Stop {
                    cause: Cause { uid: 73 }
                })
            );
            let mut answer = OrganEnvelope {
                exchange: request.exchange,
                position: Position::Close,
                payload: Payload::Answer(LifecycleAnswer::AtRest),
            };
            match case {
                "aborted" => {
                    answer.payload = Payload::Answer(LifecycleAnswer::TurnAborted {
                        turn: TurnKey("t-real".into()),
                    })
                }
                "wrong-id" => answer.exchange.ordinal += 1,
                "wrong-opener" => answer.exchange.opener = Opener::Harness,
                "wrong-position" => answer.position = Position::Continue,
                "wrong-answer" => answer.payload = Payload::Answer(LifecycleAnswer::Ready),
                "refusal" => answer.payload = Payload::Refusal(LifecycleRefusal::OutOfOrder),
                _ => (),
            }
            let bytes = serde_json::to_vec(&answer).unwrap();
            assert_eq!(
                send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
                bytes.len()
            );
        });
        let result = ask(&mut Coordination::adopt(end), Cause { uid: 73 });
        match case {
            "at-rest" => assert_eq!(result, Ok(LifecycleAnswer::AtRest)),
            "aborted" => assert_eq!(
                result,
                Ok(LifecycleAnswer::TurnAborted {
                    turn: TurnKey("t-real".into())
                })
            ),
            "refusal" => assert_eq!(result, Err(LifecycleRefusal::OutOfOrder)),
            _ => assert_eq!(result, Err(LifecycleRefusal::Malformed)),
        }
        worker.join().unwrap();
    }
}
#[test]
fn stop_exchange_actual_admin_process() {
    let Ok(root) = std::env::var("WEAVER_STOP_CIRCUIT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let mut channel = crate::channel::dial(&root.join("admin.sock"), 0, root).unwrap();
    let answer = ask(&mut channel, Cause { uid: 73 }).unwrap();
    std::fs::write(
        root.join("admin-stopped.json"),
        serde_json::to_vec(&answer).unwrap(),
    )
    .unwrap();
}
