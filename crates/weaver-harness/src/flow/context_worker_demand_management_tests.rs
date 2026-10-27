//! A pending demand keeps its original peer while unrelated root directives
//! get their own correlated disposition, both queued and holding an image.
use super::*;
use weaver_types::state_flow::enter::{ObservationAnswer, ObservationContent, ObservationEnvelope};

pub(super) fn check(socket: &Path, window: s::SaveWindow) {
    let cause = weaver_types::Cause { uid: 75 };
    let directives = [
        LifecycleDirective::Observe,
        LifecycleDirective::Stop { cause },
        LifecycleDirective::Leave {
            cause,
            forced: false,
            rollback: false,
        },
        LifecycleDirective::Leave {
            cause,
            forced: true,
            rollback: false,
        },
        LifecycleDirective::JoinLeave { cause },
    ];
    for (index, directive) in directives.into_iter().enumerate() {
        let peer = dial(socket);
        let request = OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: Opener::Admin,
                ordinal: 500 + index as u64,
            },
            position: Position::Open,
            payload: Payload::Directive(directive),
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
            Ok(bytes.len())
        );
        let bytes = answer(&peer);
        if index == 0 {
            let reply: ObservationEnvelope = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(reply.exchange, request.exchange);
            assert_eq!(reply.position, Position::Close);
            assert!(matches!(
                reply.payload,
                ObservationContent::Answer(ObservationAnswer::InTransition { .. })
            ));
        } else {
            let reply: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(reply.exchange, request.exchange);
            assert_eq!(reply.position, Position::Close);
            assert_eq!(
                reply.payload,
                Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
            );
        }
    }
    let peer = dial(socket);
    let request = CaptureEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 600,
        },
        position: Position::Open,
        payload: CapturePayload::Directive(CaptureDirective::SavePoint { cause, window }),
    };
    let bytes = request.encode(CaptureSender::Admin).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    let reply = CaptureEnvelope::decode(&answer(&peer), CaptureSender::Harness).unwrap();
    assert_eq!(reply.exchange, request.exchange);
    assert_eq!(reply.position, Position::Close);
    assert_eq!(
        reply.payload,
        CapturePayload::Refusal(CaptureRefusal::Lifecycle(
            weaver_types::LifecycleRefusal::OutOfOrder
        ))
    );
}

fn answer(peer: &std::os::fd::OwnedFd) -> Vec<u8> {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(bytes) = read_packet(peer) {
            return bytes;
        }
        assert!(
            Instant::now() < end,
            "secondary demand directive was not answered"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
