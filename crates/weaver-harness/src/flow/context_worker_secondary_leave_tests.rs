//! Keep a real empty secondary dial across a forced conversion, then finish
//! its original read before the primary Leave resumes its normal SQLite tail.
use super::*;
use crate::flow::clock::PumpIo;
use crate::flow::pump::PumpDeadline;
use weaver_types::state_flow::enter::{ObservationContent, ObservationEnvelope};

fn receive_owner<'a>(guard: &mut Guard<'a>, socket: &Path) -> OwnedFd {
    let peer = dial(socket);
    let end = Instant::now() + Duration::from_secs(4);
    while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end, "secondary dial was not accepted");
        let input = guard.input.as_mut().unwrap();
        assert_ne!(
            input.wait_for_wake(),
            FoundationInputProgress::Stopped,
            "{:?}",
            input.detail()
        );
    }
    let accepted = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .owner
        .listener
        .accepted_ns()
        .unwrap();
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let owner = guard.management.as_mut().unwrap();
    let read = owner.read.unwrap();
    assert_eq!(read.began_ns(), accepted);
    assert_eq!(
        read.deadline_ns() - accepted,
        crate::state::ANSWER_BOUND_MS * 1_000_000
    );
    assert_eq!(owner.advance(), FoundationManagementProgress::Pending);
    assert!(owner.read_armed && owner.message.is_none() && owner.peer.held().is_none());
    peer
}

fn send_directive(peer: &OwnedFd, ordinal: u64, directive: LifecycleDirective) -> OrganEnvelope {
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    request
}

fn finish_answer(guard: &mut Guard<'_>, peer: &OwnedFd, request: &OrganEnvelope, observe: bool) {
    let end = Instant::now() + Duration::from_secs(4);
    loop {
        assert!(Instant::now() < end, "secondary answer did not finish");
        let owner = guard.management.as_mut().unwrap();
        let progress = owner.advance();
        if progress == FoundationManagementProgress::Answered {
            break;
        }
        assert_eq!(
            progress,
            FoundationManagementProgress::Pending,
            "secondary directive lost its answering owner: {:?}",
            owner.detail()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let raw = read_packet(peer).expect("actual correlated secondary answer");
    if observe {
        let answer: ObservationEnvelope = serde_json::from_slice(&raw).unwrap();
        assert_eq!(answer.exchange, request.exchange);
        assert_eq!(answer.position, Position::Close);
        assert!(matches!(
            answer.payload,
            ObservationContent::Answer(ObservationAnswer::InTransition { .. })
        ));
    } else {
        let answer: OrganEnvelope = serde_json::from_slice(&raw).unwrap();
        assert_eq!(answer.exchange, request.exchange);
        assert_eq!(answer.position, Position::Close);
        assert_eq!(
            answer.payload,
            Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
        );
    }
    let owner = guard.management.as_ref().unwrap();
    assert!(!owner.read_armed);
    assert!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::AdminRead)
            .is_none()
    );
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
}

fn refuse_directives(guard: &mut Guard<'_>, socket: &Path, peer: OwnedFd, ordinal: u64) {
    let original = std::ptr::from_ref(guard.owned_input()) as usize;
    let requests = [
        LifecycleDirective::Leave {
            cause: Cause { uid: 75 },
            forced: false,
            rollback: false,
        },
        LifecycleDirective::Leave {
            cause: Cause { uid: 75 },
            forced: true,
            rollback: false,
        },
        LifecycleDirective::Leave {
            cause: Cause { uid: 75 },
            forced: true,
            rollback: true,
        },
        LifecycleDirective::SavePoint {
            cause: Cause { uid: 75 },
        },
        LifecycleDirective::Stop {
            cause: Cause { uid: 75 },
        },
        LifecycleDirective::Release,
        LifecycleDirective::Quiesce,
        LifecycleDirective::Lower,
    ];
    let mut peer = peer;
    for (index, directive) in requests.into_iter().enumerate() {
        if index > 0 {
            peer = receive_owner(guard, socket);
        }
        let request = send_directive(&peer, ordinal + index as u64, directive);
        finish_answer(guard, &peer, &request, false);
        assert_eq!(
            std::ptr::from_ref(guard.input.as_deref().unwrap()) as usize,
            original
        );
    }
}

pub(in crate::flow::context::worker_startup) fn convert(
    guard: &mut Guard<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    phase_key: Option<PumpDeadline>,
) -> Option<OwnedFd> {
    let action = mode
        .split("-secondary-")
        .nth(1)
        .unwrap()
        .split('-')
        .next()
        .unwrap();
    let original = std::ptr::from_ref(guard.input.as_deref().unwrap()) as usize;
    let leave = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .graceful
        .as_ref()
        .unwrap();
    assert!(!leave.forced);
    let original_heard = leave.heard_ns;
    let phase = phase_key.map(|key| {
        guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .owner
            .context
            .deadlines
            .get(key)
            .unwrap()
    });
    // This gate stays alive until the explicit EOF trigger below. Exercise
    // OutOfOrder while the original Leave is still graceful as well as after
    // conversion, without spending a declared phase's scheduling margin.
    if phase_key.is_none() && action == "outoforder" {
        let peer = receive_owner(guard, socket);
        refuse_directives(guard, socket, peer, 200);
        let leave = guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .graceful
            .as_ref()
            .unwrap();
        assert!(!leave.forced && leave.forced_by.is_none());
        assert_eq!(leave.heard_ns, original_heard);
    }
    let peer = receive_owner(guard, socket);
    assert!(
        !guard
            .management
            .as_ref()
            .unwrap()
            .input
            .serving
            .graceful
            .as_ref()
            .unwrap()
            .forced,
        "the original graceful phase must survive acceptance of the secondary dial"
    );
    let read = guard.management.as_ref().unwrap().read.unwrap();
    if let Some(phase) = phase {
        assert!(
            read.deadline_ns() > phase.deadline_ns(),
            "the declared phase must expire before this original read"
        );
    } else {
        std::fs::write(
            root.join("secondary-leave-read-armed"),
            b"actual empty dial owns its read",
        )
        .unwrap();
    }
    let end = Instant::now() + Duration::from_secs(4);
    while !guard
        .owned_input()
        .serving
        .graceful
        .as_ref()
        .unwrap()
        .forced
    {
        assert!(Instant::now() < end, "the original Leave did not convert");
        let owner = guard.management.as_mut().unwrap();
        assert_eq!(
            owner.advance(),
            FoundationManagementProgress::Pending,
            "forced conversion must retain the empty dial's readable owner: {:?}",
            owner.detail()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let owner = guard.management.as_ref().unwrap();
    assert!(owner.read_armed && owner.message.is_none() && owner.peer.held().is_none());
    assert_eq!(owner.read, Some(read));
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::AdminRead),
        Some(read)
    );
    assert!(owner.input.serving.owner.context.failure().is_none());
    assert!(
        owner
            .input
            .serving
            .graceful
            .as_ref()
            .unwrap()
            .forced_by
            .is_none()
    );
    if let (Some(key), Some(phase)) = (phase_key, phase) {
        assert!(phase.expired(MonotonicIo.now().unwrap()));
        assert!(
            owner
                .input
                .serving
                .owner
                .context
                .deadlines
                .get(key)
                .is_none()
        );
    }
    assert!(read_packet(&peer).is_none());
    let joined = match action {
        "silent" => {
            loop {
                assert!(
                    Instant::now() < end,
                    "original secondary read did not expire"
                );
                let owner = guard.management.as_mut().unwrap();
                let progress = owner.advance();
                if progress == FoundationManagementProgress::Dropped {
                    break;
                }
                assert_eq!(progress, FoundationManagementProgress::Pending);
                assert_eq!(owner.read, Some(read));
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(read.expired(MonotonicIo.now().unwrap()));
            assert!(read_packet(&peer).is_none());
            guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
            None
        }
        "observe" => {
            let request = send_directive(&peer, 100, LifecycleDirective::Observe);
            finish_answer(guard, &peer, &request, true);
            None
        }
        "joiner" => {
            let request = send_directive(
                &peer,
                100,
                LifecycleDirective::JoinLeave {
                    cause: Cause { uid: 75 },
                },
            );
            loop {
                assert!(Instant::now() < end, "late JoinLeave was not read");
                let owner = guard.management.as_mut().unwrap();
                let progress = owner.advance();
                if progress == FoundationManagementProgress::DirectivePending {
                    assert!(!owner.read_armed);
                    assert_eq!(owner.peer.held().unwrap().envelope, request);
                    break;
                }
                assert_eq!(progress, FoundationManagementProgress::Pending);
                std::thread::sleep(Duration::from_millis(1));
            }
            guard.input = Some(
                match guard.management.take().unwrap().join_graceful_leave() {
                    Ok(input) => input,
                    Err(owner) => {
                        guard.management = Some(owner);
                        panic!("late actual JoinLeave lost its owner")
                    }
                },
            );
            assert!(read_packet(&peer).is_none());
            Some(peer)
        }
        "outoforder" => {
            refuse_directives(guard, socket, peer, 100);
            None
        }
        other => panic!("unknown secondary action {other}"),
    };
    assert_eq!(
        std::ptr::from_ref(guard.input.as_deref().unwrap()) as usize,
        original
    );
    let input = guard.input.as_ref().unwrap();
    let leave = input.serving.graceful.as_ref().unwrap();
    assert!(leave.forced && leave.forced_by.is_none());
    assert_eq!(leave.heard_ns, original_heard);
    assert!(input.serving.owner.context.failure().is_none());
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::AdminRead)
            .is_none()
    );
    joined
}
