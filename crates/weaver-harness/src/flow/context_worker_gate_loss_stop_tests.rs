//! Root Stop owns an actual execution before the gate exits. A crossed result
//! can be held beside EOF and an unsent original ToolCancel.
use super::*;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::worker_startup::input::management::{
    FoundationManagementProgress,
    tests::{dial, read_packet},
};
use weaver_types::{Cause, LifecycleAnswer, LifecycleDirective};

pub(in super::super) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    (socket, root): (&Path, &Path),
    mode: &str,
    requests: Vec<s::PairStarted>,
) {
    let crossed = mode.contains("crossed");
    let unread = mode.ends_with("unread");
    let peer_eof = mode.ends_with("peer-eof");
    let input = guard.input.as_mut().unwrap();
    let original = input.received.as_ref().unwrap().raw_bytes().to_vec();
    let end = Instant::now() + Duration::from_secs(5);
    while !input
        .serving
        .owner
        .context
        .pairs
        .executing()
        .is_some_and(|(_, _, sent)| sent)
        || !root.join("external-2.json").exists()
    {
        assert!(
            Instant::now() < end,
            "actual original execution was not sent"
        );
        assert_eq!(
            step(input, member, clock, mode),
            FoundationInputProgress::Pending
        );
    }
    assert!(!input.serving.owner.context.serving_organs.any());
    if crossed && !unread {
        loop {
            match send(
                input.serving.owner.gate.fd().as_raw_fd(),
                &[b'x'; 4096],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(4096) => (),
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill original control path: {other:?}"),
            }
        }
    }
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 91,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Stop {
            cause: Cause { uid: 73 },
        }),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
        Ok(bytes.len())
    );
    while input.progress() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < end, "root Stop was not accepted");
        input.observe(&mut CircuitIo(clock)).unwrap();
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let management = guard.management.as_mut().unwrap();
    while management
        .input
        .engine
        .as_ref()
        .and_then(|engine| engine.cancel_control())
        .is_none()
    {
        assert!(
            Instant::now() < end,
            "Stop did not retain its original control"
        );
        assert_eq!(
            management.advance_with_io(&mut CircuitIo(clock)),
            FoundationManagementProgress::Pending
        );
        member.step(clock.now().unwrap(), mode);
    }
    let (control, ticket) = management
        .input
        .engine
        .as_ref()
        .unwrap()
        .cancel_control()
        .unwrap();
    assert_eq!(control.payload, Payload::ToolCancel);
    assert_eq!(control.exchange.ordinal, 2);
    assert_eq!(ticket.sent(), !crossed || unread);
    let original_control = serde_json::to_vec(control).unwrap();
    assert!(read_packet(&peer).is_none());
    if peer_eof {
        nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    }
    std::fs::write(
        root.join("gate-loss-stop-accepted"),
        b"actual root Stop owns control",
    )
    .unwrap();
    if mode.contains("-report") && !unread {
        hold_report(&mut management.input, root);
    }
    if crossed && !unread {
        let gate = &mut management.input.serving.owner.gate;
        while gate.held().is_none() {
            assert!(
                Instant::now() < end,
                "actual crossed ToolAnswer was not received"
            );
            gate.try_receive().unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(matches!(
            gate.held().unwrap().payload,
            Payload::ToolAnswer(_)
        ));
        assert_eq!(
            gate.held().unwrap(),
            &serde_json::from_slice::<OrganEnvelope>(
                &std::fs::read(root.join("gate-loss-return.json")).unwrap()
            )
            .unwrap()
        );
        assert!(
            gate.send_pending(),
            "the exact Stop control is still unsent"
        );
        std::fs::write(root.join("gate-loss-close"), b"EOF beside held result").unwrap();
    }
    while management.input.serving.owner.gate_child.poll() == ChildState::Running {
        assert!(Instant::now() < end, "actual gate did not exit");
        std::thread::sleep(Duration::from_millis(1));
    }
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let progress = management.advance_with_io(&mut CircuitIo(clock));
        member.step(clock.now().unwrap(), mode);
        assert_ne!(
            progress,
            FoundationManagementProgress::Stopped,
            "{:?}",
            management.detail()
        );
        if progress
            == if peer_eof {
                FoundationManagementProgress::Dropped
            } else {
                FoundationManagementProgress::Answered
            }
        {
            break;
        }
        assert!(
            Instant::now() < end,
            "Stop stranded after contained gate loss: {:?}",
            management.detail()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    guard.input = Some(
        guard
            .management
            .take()
            .unwrap()
            .finish()
            .ok()
            .expect("complete root Stop"),
    );
    let input = guard.input.as_ref().unwrap();
    assert_eq!(input.progress(), FoundationInputProgress::Aborted);
    assert_eq!(input.received.as_ref().unwrap().raw_bytes(), original);
    assert!(input.serving.owner.context.failure().is_none());
    assert!(input.serving.owner.context.open_turn.is_none());
    assert_eq!(input.serving.owner.tools.populated(), Some(false));
    assert!(!input.failed_world_was_sent());
    assert!(!root.join("closed.json").exists());
    assert!(!root.join("external-3.json").exists());
    let (control, ticket) = input
        .engine
        .as_ref()
        .unwrap()
        .cancel_control()
        .expect("original Stop control stays owned");
    assert_eq!(serde_json::to_vec(control).unwrap(), original_control);
    assert!(ticket.belongs_to(&input.serving.owner.gate));
    assert!(ticket.matches_exchange(&control.exchange));
    assert_eq!(ticket.sent(), !crossed || unread);
    assert_eq!(ticket.withdrawn(), crossed && !unread);
    if !peer_eof {
        let reply: OrganEnvelope =
            serde_json::from_slice(&read_packet(&peer).expect("actual root answer")).unwrap();
        assert_eq!(reply.exchange, request.exchange);
        assert_eq!(
            reply.payload,
            Payload::Answer(LifecycleAnswer::TurnAborted {
                turn: TurnKey("generation-turn".into())
            })
        );
    }
    assert!(read_packet(&peer).is_none());
    for (index, request) in requests.iter().enumerate() {
        let actual = input
            .pair_outcome(&request.call_id)
            .expect("original pair filing");
        assert_eq!(actual.call_id, request.call_id);
        if index == 0 {
            assert_eq!(actual.execution.as_ref().unwrap().ordinal, 2);
            if crossed {
                assert_eq!(actual.outcome, Some(expected(mode, 2)));
                assert!(actual.reason.is_none());
            } else {
                assert_eq!(
                    actual.outcome,
                    Some(ToolOutcome::Killed {
                        by: KillCause::Fault,
                        partial: None
                    })
                );
                assert_eq!(actual.reason, Some(s::PairFailure::GateFault {}));
            }
        } else {
            assert!(actual.execution.is_none());
            assert_eq!(
                actual.reason,
                Some(s::PairFailure::NotDispatched {
                    cause: NonDispatchCause::Stop
                })
            );
        }
    }
    let records = records(
        reader,
        input.serving.owner.context.last_author_sequence.unwrap(),
        root,
        mode,
    );
    let faults: Vec<_> = records
        .iter()
        .filter(|row| row["kind"] == "fault" && row["payload"]["case"] == "organ_death_observed")
        .collect();
    let closes: Vec<_> = records
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0]["payload"]["case"], "organ_death_observed");
    assert_eq!(closes.len(), 1);
    assert_eq!(closes[0]["turn"], "generation-turn");
    assert_eq!(closes[0]["payload"]["reason"], "directive");
    let ends: Vec<_> = records
        .iter()
        .filter(|row| row["kind"] == "tool.call.completed")
        .collect();
    assert_eq!(ends.len(), requests.len());
    let starts: Vec<_> = records
        .iter()
        .filter(|row| row["kind"] == "tool.call.started")
        .collect();
    assert_eq!(starts.len(), requests.len());
    for ((start, end), request) in starts.iter().zip(ends).zip(&requests) {
        assert_eq!(start["payload"], serde_json::to_value(request).unwrap());
        assert_eq!(
            end["payload"]["call_id"],
            serde_json::to_value(&request.call_id).unwrap()
        );
        assert!(sequence(start) < sequence(end));
        assert!(sequence(end) < sequence(closes[0]));
    }
}
