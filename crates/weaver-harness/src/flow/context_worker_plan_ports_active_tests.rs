//! Public successor, activation and predicate ports against the actual member.
//! Every activation below consumes the public query proof and exact effect owner.
use super::*;
use crate::flow::context::worker_startup::input::plan::tests as root_plan;
use crate::flow::context::{ReadCompletion, RecordedRead, RecordingContext};
use sha2::{Digest as _, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use weaver_trace::serving::{Kind, Payload as TracePayload};

const TURN: &str = "active-plan-port";

fn pump<T>(
    input: &mut FoundationInput<'_>,
    step: impl FnOnce(&mut RecordingContext, &mut PumpPorts<'_, MonotonicIo>) -> T,
) -> T {
    let owner = &mut input.serving.owner;
    step(
        owner.context,
        &mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: Some(&mut owner.listener),
                residency: Some(owner.resident.residency),
                ..Default::default()
            },
            gate: Some(&mut owner.gate),
            decode: Some(owner.decode),
            label: None,
            io: &mut MonotonicIo,
        },
    )
}

fn read(input: &mut FoundationInput<'_>, selector: s::ReadSelector) -> RecordedRead {
    input
        .serving
        .owner
        .context
        .begin_read(selector, None, &mut MonotonicIo)
        .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let done = pump(input, |context, ports| {
            context.advance_read(ports).unwrap();
            let done = context.take_read(ports.io).unwrap();
            if done.is_none() {
                context.pump_once(ports).unwrap();
            }
            done
        });
        if let Some(ReadCompletion::Recorded(read)) = done {
            return read;
        }
        assert!(done.is_none(), "fixture read refused: {done:?}");
        assert!(Instant::now() < end, "fixture read did not settle");
    }
}

fn complete_operation(input: &mut FoundationInput<'_>) -> FoundationPlanOperation {
    let end = Instant::now() + Duration::from_secs(5);
    assert_eq!(
        input.progress(),
        FoundationInputProgress::Pending,
        "an admitted plan owner excludes input before its first reduction"
    );
    while input.plan_operation_pending() {
        let stage = input
            .serving
            .owner
            .context
            .plan_port_work()
            .map(|work| work.test_stage());
        assert_ne!(
            input.advance(),
            FoundationInputProgress::Stopped,
            "{stage:?}: {:?}",
            input.detail()
        );
        assert!(Instant::now() < end, "public plan operation did not settle");
    }
    input
        .take_plan_operation()
        .expect("one completed operation")
}

/// Activate the first waiting step after `previous`, the closed step the
/// successor query follows, or the plan's first step when it is None.
fn activate(
    input: &mut FoundationInput<'_>,
    previous: Option<s::StepId>,
) -> (s::StepQueryRef, s::SpanRef, s::SourcePosition) {
    let context = &mut input.serving.owner.context;
    let bound = AbsoluteBound::after(MonotonicIo.now().unwrap(), 5_000_000_000).unwrap();
    context
        .arm(PumpDeadline::Decision, bound, &mut MonotonicIo)
        .unwrap();
    pump(input, |context, ports| {
        context
            .author_and_wait(
                ports,
                AuthoredEvent {
                    kind: Kind::TurnStarted,
                    subsystem: Subsystem::Harness,
                    turn: Some(TurnRef(TURN.into())),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: None,
                },
            )
            .unwrap();
    });
    let context = &mut input.serving.owner.context;
    let plan = context
        .committed_plan_summary()
        .unwrap()
        .unwrap()
        .plan
        .clone();
    input
        .step_successor(plan, previous, Some(TurnKey(TURN.into())))
        .unwrap();
    let checked = complete_operation(input).into_successor().unwrap();
    let successor = checked.reference().clone();
    input
        .activate(
            successor.plan.clone(),
            successor.step_id.clone(),
            checked,
            TurnKey(TURN.into()),
        )
        .unwrap();
    let result = complete_operation(input);
    let FoundationPlanOutcome::Activated(active) = result.outcome() else {
        panic!("activation must return its committed active step: {result:?}");
    };
    assert!(
        input.serving.owner.context.native_place_cache_ready(),
        "activation must adopt native scopes before granting the active step"
    );
    assert_eq!(active.step_id, successor.step_id);
    assert_eq!(active.status, s::StepStatus::Active);
    assert_eq!(active.instruction, successor.instruction);
    let span = active.span.clone().expect("committed activation span");
    let through = input
        .serving
        .owner
        .context
        .committed_plan_summary()
        .unwrap()
        .unwrap()
        .last_source
        .clone();
    assert_eq!(
        input
            .serving
            .owner
            .context
            .complete_wait(PumpDeadline::Decision),
        Some(bound)
    );
    (successor, span, through)
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    mode: &str,
) {
    let mut guard = root_plan::sqlite_update(serving, member, reader, socket, false);
    let done = AtomicBool::new(false);
    struct Done<'a>(&'a AtomicBool);
    impl Drop for Done<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let limit = Instant::now() + Duration::from_secs(15);
            while !done.load(Ordering::SeqCst) && Instant::now() < limit {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let _done = Done(&done);
        let input = guard.input.as_mut().unwrap();
        let (successor, span, through) = activate(input, None);
        let before = input
            .serving
            .owner
            .context
            .committed_plan_summary()
            .unwrap()
            .cloned();
        let s::QueryResult::PlanCurrent {
            current: Some(before_snapshot),
            ..
        } = input
            .serving
            .owner
            .context
            .plan_cache
            .committed
            .as_ref()
            .unwrap()
            .read()
            .result()
        else {
            panic!("actual committed plan snapshot")
        };
        let before_steps = before_snapshot.steps.clone();
        if mode.ends_with("-close") {
            close(input, &successor, span, through);
            return;
        }
        if mode.ends_with("-close-next") {
            // The second step closes behind the first one's closure: the
            // receipt must name the second step and its own closure source.
            close(input, &successor, span, through);
            let (next, span, through) = activate(input, Some(successor.step_id.clone()));
            assert_ne!(next.step_id, successor.step_id);
            close(input, &next, span, through);
            return;
        }
        let expected_digest = if mode.ends_with("-work") {
            let preview = read(
                input,
                s::ReadSelector::SpanPreview {
                    span: span.clone(),
                    through: through.clone(),
                },
            );
            let s::QueryResult::SpanPreview { entries, .. } = preview.result() else {
                panic!("preview required")
            };
            assert!(
                !entries.is_empty(),
                "real step preview retains its instruction prefix"
            );
            Some(format!("{:x}", Sha256::digest(preview.raw_bytes())))
        } else {
            None
        };
        if mode.ends_with("-break") {
            input
                .record_break(
                    successor.plan.clone(),
                    Some(successor.step_id.clone()),
                    s::PlanBreakCase::WrongCategory,
                    Some(successor.instruction.block_id.clone()),
                    Some(TurnKey(TURN.into())),
                )
                .unwrap();
        } else if mode.ends_with("-work") {
            input
                .work_reference(
                    successor.plan.clone(),
                    successor.step_id.clone(),
                    span.clone(),
                    through.clone(),
                    Some(TurnKey(TURN.into())),
                )
                .unwrap();
        } else {
            let act = if mode.ends_with("-category") {
                s::PlanAct::InwardRecall {
                    verb: "read".into(),
                }
            } else if mode.ends_with("-tool") {
                s::PlanAct::ExternalTool {
                    name: "unloaded".into(),
                }
            } else {
                s::PlanAct::Statement
            };
            input
                .assess_act(
                    successor.plan.clone(),
                    successor.step_id.clone(),
                    successor.instruction.clone(),
                    act,
                    Some(TurnKey(TURN.into())),
                )
                .unwrap();
        }
        let end = Instant::now() + Duration::from_secs(5);
        while input.plan_operation_pending() {
            assert_ne!(
                input.advance(),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            assert!(Instant::now() < end, "public plan operation did not settle");
        }
        if mode.ends_with("-completed-leave") || mode.ends_with("-completed-stop") {
            interrupt_completed(&mut guard, socket, mode.ends_with("-completed-leave"));
        }
        let input = guard.input.as_mut().unwrap();
        let result = input
            .take_plan_operation()
            .expect("one completed operation");
        assert!(result.completion.belongs_to(input.serving.owner.context));
        match result.outcome() {
            FoundationPlanOutcome::BreakRecorded { current } => {
                assert!(mode.ends_with("-break"));
                let current = current.unwrap();
                assert_eq!(current.plan, successor.plan);
                assert_eq!(current.status, s::PlanStatus::Broken);
                let broken = current.broken.as_ref().unwrap();
                assert_eq!(broken.step_id.as_ref(), Some(&successor.step_id));
                assert_eq!(broken.case, s::PlanBreakCase::WrongCategory);
                assert_eq!(
                    broken.proposed_source.as_ref(),
                    Some(&successor.instruction.block_id)
                );
                assert_eq!(broken.source, current.last_source.block_id);
                assert_eq!(current.steps, before_steps);
                assert!(input.serving.owner.context.native_place_cache_ready());
                let marks = read(input, s::ReadSelector::Marks {});
                let s::QueryResult::Marks { open_scopes, .. } = marks.result() else {
                    panic!()
                };
                assert!(
                    open_scopes
                        .iter()
                        .any(|scope| scope.span == span && scope.status == s::SpanStatus::Open)
                );
            }
            FoundationPlanOutcome::Cancelled => {
                assert!(mode.ends_with("-completed-leave") || mode.ends_with("-completed-stop"))
            }
            FoundationPlanOutcome::Allowed => assert!(mode.ends_with("-assess")),
            FoundationPlanOutcome::Work(work) => {
                assert!(mode.ends_with("-work"));
                assert_eq!(work.plan, successor.plan);
                assert_eq!(work.step_id, successor.step_id);
                assert_eq!(work.span, span);
                assert_eq!(work.through, through);
                assert_eq!(
                    work.content_sha256.as_str(),
                    expected_digest.as_deref().unwrap()
                );
            }
            FoundationPlanOutcome::Refused(refusal) => {
                assert!(mode.ends_with("-category") || mode.ends_with("-tool"));
                assert_eq!(
                    refusal.case,
                    if mode.ends_with("-category") {
                        s::PlanOperationCase::WrongCategory
                    } else {
                        s::PlanOperationCase::UnknownTool
                    }
                );
                assert_eq!(refusal.read_code, None);
            }
            other => panic!("unexpected operation outcome: {other:?}"),
        }
        assert!(input.take_plan_operation().is_none());
        if !mode.ends_with("-break") {
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .committed_plan_summary()
                    .unwrap(),
                before.as_ref()
            );
        }
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision)
                .is_none()
        );
    });
    let rows = management::tests::trace(reader);
    if mode.ends_with("-close-next") {
        let outcomes = rows
            .iter()
            .filter(|row| row["kind"] == "step.outcome")
            .count();
        assert_eq!(outcomes, 2, "each close certifies its own closed outcome");
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == "plan.event")
                .count(),
            2,
            "only the two activations write plan events"
        );
        assert!(rows.iter().all(|row| row["kind"] != "refusal"));
        return;
    }
    if mode.ends_with("-close") {
        let outcomes: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "step.outcome")
            .collect();
        assert_eq!(outcomes.len(), 1, "one certified closed outcome");
        assert_eq!(outcomes[0]["payload"]["template_id"], "step.closed");
        assert_eq!(
            outcomes[0]["payload"]["outcome"],
            serde_json::json!({"closed": {}})
        );
        assert_eq!(outcomes[0]["turn"], TURN);
        assert!(rows.iter().all(|row| row["kind"] != "refusal"));
        assert!(
            rows.iter()
                .all(|row| row["kind"] != "tool.call.started" && row["kind"] != "model.request"),
            "closure invokes no reviewer and starts no generation"
        );
        return;
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "plan.event")
            .count(),
        if mode.ends_with("-break") { 2 } else { 1 },
        "only admitted activation and break effects change the plan"
    );
    let breaks: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "plan.break")
        .collect();
    assert_eq!(breaks.len(), usize::from(mode.ends_with("-break")));
    if mode.ends_with("-break") {
        let effect = rows
            .iter()
            .rev()
            .find(|row| row["kind"] == "plan.event")
            .unwrap();
        assert_eq!(breaks[0]["causal_parent"], effect["sequence"]);
        assert_eq!(breaks[0]["turn"], TURN);
        assert_eq!(breaks[0]["payload"]["case"], "wrong_category");
    }
    assert!(
        rows.iter()
            .all(|row| row["kind"] != "tool.call.started" && row["kind"] != "model.request"),
        "a plan check dispatches nothing and starts no generation"
    );
    let refusals: Vec<_> = rows.iter().filter(|row| row["kind"] == "refusal").collect();
    if mode.ends_with("-category") || mode.ends_with("-tool") {
        assert_eq!(refusals.len(), 1);
        assert_eq!(refusals[0]["turn"], TURN);
        let read = rows
            .iter()
            .rev()
            .find(|row| row["kind"] == "recall")
            .unwrap();
        assert_eq!(refusals[0]["causal_parent"], read["sequence"]);
    } else {
        assert!(refusals.is_empty());
    }
}

fn interrupt_completed(guard: &mut Guard<'_>, socket: &Path, leave: bool) {
    if !leave {
        // This fixture opens a trace turn to file activation, without a world
        // request. Close it through the real author before testing idle Stop.
        // A live-turn Stop requires a gate request and a turn owner instead.
        pump(guard.input.as_mut().unwrap(), |context, ports| {
            context
                .author_and_wait(
                    ports,
                    AuthoredEvent {
                        kind: Kind::TurnClosed,
                        subsystem: Subsystem::Harness,
                        source: AuthoredSource::Runtime,
                        turn: Some(TurnRef(TURN.into())),
                        causal_parent: None,
                        payload: Some(TracePayload::TurnClosed(weaver_trace::TurnClose::Clean)),
                    },
                )
                .unwrap();
        });
    }
    let input = guard.input.as_ref().unwrap();
    let result = input
        .serving
        .plan_operation
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .expect("assessment completed before the management packet");
    assert!(matches!(result.outcome(), FoundationPlanOutcome::Allowed));
    let request = format!("{:?}", result.request());
    assert!(!input.plan_operation_pending());
    assert!(input.serving.owner.context.plan_port_work().is_none());
    let input_address = std::ptr::from_ref(&**input);
    let peer = dial(socket);
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 412,
        },
        position: Position::Open,
        payload: Payload::Directive(if leave {
            LifecycleDirective::Leave {
                cause: Cause { uid: 0 },
                forced: false,
                rollback: false,
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
    let limit = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(
            Instant::now() < limit,
            "completed grant must allow management"
        );
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let owner = guard.management.as_mut().unwrap();
    let wanted = if leave {
        FoundationManagementProgress::DirectivePending
    } else {
        FoundationManagementProgress::Answered
    };
    while owner.progress() != wanted {
        assert_ne!(
            owner.advance(),
            FoundationManagementProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        assert!(
            Instant::now() < limit,
            "completed grant management must settle"
        );
    }
    if leave {
        assert!(
            read_packet(&peer).is_none(),
            "Leave still owns its later cleanup"
        );
        // A graceful Leave returns the same input via its real deferred handoff.
        // finish() instead returns the input for an answered Stop below.
        guard.input = Some(
            guard
                .management
                .take()
                .unwrap()
                .defer_graceful_leave()
                .ok()
                .unwrap(),
        );
        assert!(guard.input.as_ref().unwrap().has_graceful_leave());
        assert_eq!(
            guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .message
                .envelope,
            envelope
        );
    } else {
        let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
        assert_eq!(answer.payload, Payload::Answer(LifecycleAnswer::AtRest));
        guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    }
    let input = guard.input.as_mut().unwrap();
    assert_eq!(std::ptr::from_ref(&**input), input_address);
    assert!(input.serving.owner.context.failure().is_none());
    let result = input
        .serving
        .plan_operation
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .unwrap();
    assert_eq!(format!("{:?}", result.request()), request);
    // The caller must observe revocation through the public take below, after
    // management has returned the input. Do not cancel from this fixture.
}

/// D1 against the actual member: its served preview becomes the harness
/// WorkRef, attest_closure grants the evidence, close allocates its outcome
/// span from its own Marks read, and the store accepts the close.
fn close(
    input: &mut FoundationInput<'_>,
    successor: &s::StepQueryRef,
    span: s::SpanRef,
    through: s::SourcePosition,
) {
    let preview = read(
        input,
        s::ReadSelector::SpanPreview {
            span: span.clone(),
            through: through.clone(),
        },
    );
    let digest = format!("{:x}", Sha256::digest(preview.raw_bytes()));
    input
        .work_reference(
            successor.plan.clone(),
            successor.step_id.clone(),
            span.clone(),
            through,
            Some(TurnKey(TURN.into())),
        )
        .unwrap();
    let work = complete_operation(input).into_checked_work().unwrap();
    assert_eq!(work.reference().content_sha256.as_str(), digest);
    let reference = work.reference().clone();
    input
        .attest_closure(
            successor.plan.clone(),
            successor.step_id.clone(),
            work,
            Some(TurnKey(TURN.into())),
        )
        .unwrap();
    let evidence = complete_operation(input).into_closure_evidence().unwrap();
    // The span close must allocate, stated from the member's catalog
    // independently of the allocator: next_span_id at revision one.
    let catalog = read(input, s::ReadSelector::Marks {});
    let s::QueryResult::Marks { next_span_id, .. } = catalog.result() else {
        panic!("member marks")
    };
    let outcome = s::SpanRef {
        seed_id: successor.plan.plan_id.seed_id.clone(),
        local_id: *next_span_id,
        revision: 1.into(),
    };
    let bound = AbsoluteBound::after(MonotonicIo.now().unwrap(), 5_000_000_000).unwrap();
    input
        .serving
        .owner
        .context
        .arm(PumpDeadline::Decision, bound, &mut MonotonicIo)
        .unwrap();
    input
        .close(
            successor.plan.clone(),
            successor.step_id.clone(),
            evidence,
            TurnKey(TURN.into()),
        )
        .unwrap();
    let result = complete_operation(input);
    let FoundationPlanOutcome::Closed(current) = result.outcome() else {
        panic!("close must return its committed snapshot: {result:?}")
    };
    let closed = current
        .steps
        .iter()
        .find(|step| step.step_id == successor.step_id)
        .unwrap();
    assert_eq!(closed.status, s::StepStatus::Closed);
    assert_eq!(closed.outcome_span.as_ref(), Some(&outcome));
    assert_eq!(closed.work.as_ref(), Some(&reference));
    assert_eq!(closed.review_call, None);
    assert_eq!(
        closed.closure_source.as_ref(),
        Some(&current.last_source.block_id)
    );
    let receipt = result.into_closure_receipt().unwrap();
    assert!(receipt.belongs_to(input.serving.owner.context));
    // The receipt's closed source is the closure the member committed, read
    // afresh from the member rather than from the snapshot the receipt kept.
    let committed = read(input, s::ReadSelector::PlanCurrent {});
    let s::QueryResult::PlanCurrent {
        current: Some(committed),
        ..
    } = committed.result()
    else {
        panic!("member current plan")
    };
    let member_closure = committed
        .steps
        .iter()
        .find(|step| step.step_id == successor.step_id)
        .and_then(|step| step.closure_source.as_ref())
        .expect("the member holds the closure");
    assert_eq!(receipt.closed(), member_closure);
    assert_ne!(receipt.closed(), &reference.through.block_id);
    assert_eq!(receipt.plan(), &successor.plan);
    assert_eq!(receipt.previous_step(), &successor.step_id);
    assert!(input.serving.owner.context.native_place_cache_ready());
    // The member's own marks show the outcome opened and closed below the step.
    let marks = read(input, s::ReadSelector::Marks {});
    let s::QueryResult::Marks { open_scopes, .. } = marks.result() else {
        panic!()
    };
    assert!(
        open_scopes
            .iter()
            .all(|scope| scope.span != span && scope.span != outcome)
    );
    assert_eq!(
        input
            .serving
            .owner
            .context
            .complete_wait(PumpDeadline::Decision),
        Some(bound)
    );
}
