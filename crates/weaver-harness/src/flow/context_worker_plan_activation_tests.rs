//! Actual checked world input and SQLite plan effects, interrupted while their
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
    let breaking = mode.contains("-port-break-");
    let active_break = breaking && mode.contains("-break-active-");
    let closing = mode.contains("-activation-close-");
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
    let mut active_step = None;
    if !breaking || active_break {
        input
            .step_successor(plan.clone(), None, Some(turn.clone()))
            .unwrap();
        let checked = complete(input, member).into_successor().unwrap();
        let successor = checked.reference().clone();
        if !breaking && !closing {
            member.hold_next_normal_filing();
        }
        input
            .activate(
                plan.clone(),
                successor.step_id.clone(),
                checked,
                turn.clone(),
            )
            .unwrap();
        if breaking || closing {
            let activated = complete(input, member);
            let FoundationPlanOutcome::Activated(step) = activated.outcome() else {
                panic!("activation must commit before its break or closure")
            };
            let span = step.span.clone().unwrap();
            active_step = Some(successor.step_id.clone());
            if closing {
                // Admit close against the actual member, then hold its
                // step.outcome receipt so Stop/Leave arrives after admission.
                let evidence = attest(input, member, &plan, &successor.step_id, span, &turn);
                let outcome = outcome_span(input, member, &plan, &turn);
                member.hold_next_normal_filing();
                input
                    .close(
                        plan.clone(),
                        successor.step_id.clone(),
                        evidence,
                        outcome,
                        turn.clone(),
                    )
                    .unwrap();
            }
        }
    }
    if breaking {
        member.hold_next_normal_filing();
        input
            .record_break(
                plan,
                active_step,
                s::PlanBreakCase::WorkChanged,
                Some(source.clone()),
                Some(turn.clone()),
            )
            .unwrap();
    }
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
            "plan effect receipt never reached the proxy"
        );
    }
    let context = &mut input.serving.owner.context;
    let request = format!("{:?}", context.plan_port_work().unwrap().test_request());
    let effect = context.test_plan_effect().unwrap().clone();
    assert_eq!(matches!(effect, s::PlanEffect::Break { .. }), breaking);
    assert_eq!(matches!(effect, s::PlanEffect::Close { .. }), closing);
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
        "cancelled committed effect must adopt its native cache"
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
    match &effect {
        s::PlanEffect::Activate {
            step_id, step_span, ..
        } => {
            let active = current
                .steps
                .iter()
                .find(|step| &step.step_id == step_id)
                .unwrap();
            assert_eq!(active.status, s::StepStatus::Active);
            assert_eq!(active.span.as_ref(), Some(step_span));
        }
        s::PlanEffect::Close {
            step_id,
            outcome_span,
            work,
            ..
        } => {
            let closed = current
                .steps
                .iter()
                .find(|step| &step.step_id == step_id)
                .unwrap();
            assert_eq!(closed.status, s::StepStatus::Closed);
            assert_eq!(closed.outcome_span.as_ref(), Some(outcome_span));
            assert_eq!(closed.work.as_ref(), Some(work));
            assert_eq!(
                closed.closure_source.as_ref(),
                Some(&current.last_source.block_id)
            );
        }
        s::PlanEffect::Break {
            expected,
            step_id,
            case,
            proposed_source,
        } => {
            assert_eq!(&current.plan, expected);
            assert_eq!(current.status, s::PlanStatus::Broken);
            let broken = current.broken.as_ref().unwrap();
            assert_eq!(&broken.step_id, step_id);
            assert_eq!(&broken.case, case);
            assert_eq!(&broken.proposed_source, proposed_source);
            assert_eq!(broken.source, current.last_source.block_id);
            assert_eq!(
                current
                    .steps
                    .iter()
                    .any(|step| step.status == s::StepStatus::Active),
                active_break
            );
        }
        _ => unreachable!(),
    }
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
        if active_break { 2 } else { 1 }
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "step.outcome")
            .count(),
        usize::from(closing)
    );
    let reports: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "plan.break")
        .collect();
    assert_eq!(reports.len(), usize::from(breaking));
    if breaking {
        assert_eq!(reports[0]["causal_parent"], effect_sequence.to_string());
        assert_eq!(reports[0]["turn"], turn.0);
        assert_eq!(reports[0]["payload"]["case"], "work_changed");
        assert_eq!(
            reports[0]["payload"]["proposed_source"],
            serde_json::to_value(&source).unwrap()
        );
    }
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
    if breaking {
        assert!(
            closes[0]["sequence"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                > reports[0]["sequence"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
        );
    }
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

fn attest(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    plan: &s::PlanRef,
    step_id: &s::StepId,
    span: s::SpanRef,
    turn: &TurnKey,
) -> ClosureEvidence {
    let through = input
        .serving
        .owner
        .context
        .committed_plan_summary()
        .unwrap()
        .unwrap()
        .last_source
        .clone();
    input
        .work_reference(
            plan.clone(),
            step_id.clone(),
            span,
            through,
            Some(turn.clone()),
        )
        .unwrap();
    let work = complete(input, member).into_checked_work().unwrap();
    input
        .attest_closure(plan.clone(), step_id.clone(), work, Some(turn.clone()))
        .unwrap();
    complete(input, member).into_closure_evidence().unwrap()
}

/// The caller's outcome span, through the public outcome_span operation.
/// The owner holds its read for the close that follows.
fn outcome_span(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    plan: &s::PlanRef,
    turn: &TurnKey,
) -> s::SpanRef {
    input
        .outcome_span(plan.clone(), Some(turn.clone()))
        .unwrap();
    let result = complete(input, member);
    let FoundationPlanOutcome::OutcomeSpan(span) = result.outcome() else {
        panic!("outcome_span must allocate: {result:?}")
    };
    span.clone()
}
