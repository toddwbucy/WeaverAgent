//! Actual checked world input and SQLite activation, interrupted while its
//! committed receipt is held by the byte-preserving member proxy.
use super::*;
use crate::flow::context::worker_startup::input::plan::tests as root_plan;

fn step(input: &mut FoundationInput<'_>, member: &mut Member) {
    assert_ne!(
        input.advance_with_io(&mut NonblockingIo { waits: 0 }),
        FoundationInputProgress::Stopped,
        "{:?}",
        input.detail()
    );
    member.step_live("ready");
    std::thread::sleep(Duration::from_millis(1));
}

fn complete(input: &mut FoundationInput<'_>, member: &mut Member) -> FoundationPlanOperation {
    let end = Instant::now() + Duration::from_secs(5);
    assert_eq!(input.progress(), FoundationInputProgress::Pending);
    while input.plan_operation_pending() {
        step(input, member);
        assert!(Instant::now() < end, "actual query did not settle");
    }
    input.take_plan_operation().unwrap()
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    mode: &str,
) -> Option<Cleanup> {
    let mut guard = root_plan::sqlite_update(serving, member, reader, socket, false);
    root_plan::flags::prepare_checked_turn(&mut guard, member, socket);
    let input = guard.input.as_mut().unwrap();
    let wire = input.received.as_ref().unwrap().raw_bytes().to_vec();
    let turn = input.turn.clone().unwrap();
    let source = input.seated.as_ref().unwrap().source.clone();
    let plan = input
        .serving
        .owner
        .context
        .committed_plan_summary()
        .unwrap()
        .unwrap()
        .plan
        .clone();
    input
        .step_successor(plan, None, Some(turn.clone()))
        .unwrap();
    let checked = complete(input, member).into_successor().unwrap();
    let successor = checked.reference().clone();
    member.hold_next_normal_filing();
    input
        .activate(
            successor.plan.clone(),
            successor.step_id.clone(),
            checked,
            turn.clone(),
        )
        .unwrap();
    let bound = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while !member.normal_filing_held() {
        step(input, member);
        assert!(
            Instant::now() < end,
            "activation receipt never reached the proxy"
        );
    }
    let context = &mut input.serving.owner.context;
    let request = format!("{:?}", context.plan_port_work().unwrap().test_request());
    let effect = context.test_activation_effect().unwrap().clone();
    assert!(matches!(effect, s::PlanEffect::Activate { .. }));
    assert!(!context.native_place_cache_ready());
    assert!(input.take_plan_operation().is_none());
    assert!(input.plan_operation_pending());
    if mode.ends_with("-failed") {
        return Some(Cleanup::Failed(failed::begin_activation(
            guard, member, reader, request, bound, effect,
        )));
    }

    let leave = mode.ends_with("-leave");
    let peer = dial(socket);
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 413,
        },
        position: Position::Open,
        payload: Payload::Directive(if leave {
            LifecycleDirective::Leave {
                cause: Cause { uid: 0 },
                forced: false,
                rollback: true,
            }
        } else {
            LifecycleDirective::Stop {
                cause: Cause { uid: 0 },
            }
        }),
    };
    let bytes = serde_json::to_vec(&envelope).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    while guard
        .input
        .as_mut()
        .unwrap()
        .advance_with_io(&mut NonblockingIo { waits: 0 })
        != FoundationInputProgress::ManagementPending
    {
        assert!(
            Instant::now() < end,
            "held activation must yield management"
        );
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    loop {
        let owner = guard.management.as_mut().unwrap();
        assert_eq!(
            owner.advance_with_io(&mut NonblockingIo { waits: 0 }),
            FoundationManagementProgress::Pending,
            "{:?}",
            owner.detail()
        );
        let context = &owner.input.serving.owner.context;
        assert_eq!(context.deadlines.get(PumpDeadline::Decision), Some(bound));
        assert!(
            owner
                .input
                .serving
                .plan_operation
                .as_ref()
                .unwrap()
                .result
                .is_none()
        );
        if context.plan_port_work().unwrap().test_cancelled() {
            break;
        }
        assert!(
            Instant::now() < end,
            "management did not cancel the held activation"
        );
    }
    assert!(
        read_packet(&peer).is_none(),
        "no answer before the actual filing receipt"
    );
    assert!(
        !socket
            .parent()
            .unwrap()
            .join("flags-turn-close.json")
            .exists()
    );
    member.release_normal_filing();
    let wanted = if leave {
        FoundationManagementProgress::DirectivePending
    } else {
        FoundationManagementProgress::Answered
    };
    loop {
        let owner = guard.management.as_mut().unwrap();
        let progress = owner.advance_with_io(&mut NonblockingIo { waits: 0 });
        assert_ne!(
            progress,
            FoundationManagementProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        if progress == wanted {
            break;
        }
        member.step_live("ready");
        assert!(
            Instant::now() < end,
            "management did not settle the admitted effect"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let input = &mut guard.management.as_mut().unwrap().input;
    assert_eq!(input.received.as_ref().unwrap().raw_bytes(), wire);
    assert_eq!(input.turn.as_ref(), Some(&turn));
    assert_eq!(input.seated.as_ref().unwrap().source, source);
    let result = input
        .serving
        .plan_operation
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .unwrap();
    assert_eq!(format!("{:?}", result.request()), request);
    assert!(matches!(result.outcome(), FoundationPlanOutcome::Cancelled));
    let context = &mut input.serving.owner.context;
    assert!(context.failure().is_none());
    assert!(context.plan_port_work().is_none());
    assert!(context.filing.is_none() && context.plan_summary.is_none() && context.read.is_none());
    assert!(context.plan_cache.required.is_none());
    assert!(
        context.native_place_cache_ready(),
        "cancelled committed activation must adopt scopes"
    );
    let s::QueryResult::PlanCurrent {
        current: Some(current),
        ..
    } = context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .result()
    else {
        panic!("actual complete plan snapshot")
    };
    let active = current
        .steps
        .iter()
        .find(|step| step.step_id == successor.step_id)
        .unwrap();
    assert_eq!(active.status, s::StepStatus::Active);
    let s::PlanEffect::Activate { step_span, .. } = &effect else {
        unreachable!()
    };
    assert_eq!(active.span.as_ref(), Some(step_span));
    let effect_sequence = current.last_source.block_id.sequence.0;
    let through = context.barrier.latest_recorded().unwrap();
    let rows = trace_through(
        reader,
        vec![],
        through,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "plan.event")
            .count(),
        1
    );
    assert!(
        rows.iter()
            .all(|row| row["kind"] != "model.request" && row["kind"] != "refusal")
    );
    if leave {
        assert!(read_packet(&peer).is_none());
        assert!(rows.iter().all(|row| row["kind"] != "turn.closed"));
        let handoff = guard
            .management
            .take()
            .unwrap()
            .rollback_handoff()
            .ok()
            .unwrap();
        cleanup::check_account(&handoff, &request);
        return Some(Cleanup::Leave(cleanup::Leave {
            handoff,
            peer,
            request,
            root: socket.parent().unwrap().to_path_buf(),
        }));
    }
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
    assert_eq!(answer.exchange, envelope.exchange);
    assert_eq!(
        answer.payload,
        Payload::Answer(LifecycleAnswer::TurnAborted { turn: turn.clone() })
    );
    let closes: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(closes.len(), 1);
    assert_eq!(closes[0]["turn"], turn.0);
    assert!(
        closes[0]["sequence"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > effect_sequence
    );
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    let input = guard.input.as_mut().unwrap();
    assert!(matches!(
        input.take_plan_operation().unwrap().outcome(),
        FoundationPlanOutcome::Cancelled
    ));
    assert!(input.take_plan_operation().is_none());
    assert!(input.serving.owner.context.open_turn.is_none());
    let close_path = socket.parent().unwrap().join("flags-turn-close.json");
    let end = Instant::now() + Duration::from_secs(3);
    while !close_path.exists() {
        assert!(
            Instant::now() < end,
            "actual gate did not receive the stopped world reply"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let close: OrganEnvelope = serde_json::from_slice(&std::fs::read(close_path).unwrap()).unwrap();
    let original: OrganEnvelope = serde_json::from_slice(&wire).unwrap();
    assert_eq!(close.exchange, original.exchange);
    assert_eq!(close.position, Position::Close);
    assert!(matches!(close.payload, Payload::Frame(_)));
    None
}
