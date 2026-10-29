//! Positive public ports against a plan filed and activated in the real member.
//! Activation here is fixture setup through existing filing/summary owners,
//! not evidence that the future public activation port is implemented.
use super::*;
use crate::flow::context::worker_startup::input::plan::tests as root_plan;
use crate::flow::context::{PlanSummaryCompletion, ReadCompletion, RecordedRead, RecordingContext};
use sha2::{Digest as _, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use weaver_trace::serving::{Kind, Payload as TracePayload, payloads as t};
use weaver_traits::{ContentBlock, Message, Role};

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

fn activation(successor: &s::StepQueryRef, span: &s::SpanRef) -> AuthoredEvent {
    let step = t::StepId {
        seed_id: successor.step_id.seed_id.clone(),
        local_id: successor.step_id.local_id.0.into(),
    };
    let plan = crate::flow::mirror::plan_ref(&successor.plan);
    let source = &successor.instruction.block_id;
    AuthoredEvent {
        kind: Kind::PlanEvent,
        subsystem: Subsystem::Harness,
        source: AuthoredSource::Loop,
        turn: Some(TurnRef(TURN.into())),
        causal_parent: None,
        payload: Some(TracePayload::PlanEffect(Box::new(t::PlanEffectRecord {
            message: serde_json::value::to_raw_value(&Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "Read plan.".into(),
                }],
            })
            .unwrap(),
            template_id: "plan.activate".into(),
            context: t::PlanEffect::Activate {
                expected: plan.clone(),
                step_id: step.clone(),
                step_span: crate::flow::mirror::span_ref(span),
                group_span: None,
                successor: t::StepQueryRef {
                    plan,
                    previous_step: None,
                    step_id: step,
                    instruction: t::StatementRef {
                        block_id: t::SourceBlockId {
                            run: source.run.clone(),
                            sequence: source.sequence.0.into(),
                            block: source.block.0.into(),
                        },
                        binding: successor.instruction.binding.0.into(),
                    },
                },
            },
        }))),
    }
}

fn activate(input: &mut FoundationInput<'_>) -> (s::StepQueryRef, s::SpanRef, s::SourcePosition) {
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
    let successor_read = read(
        input,
        s::ReadSelector::StepSuccessor {
            plan,
            previous_step: None,
        },
    );
    let s::QueryResult::StepSuccessor {
        reference,
        instruction,
    } = successor_read.result()
    else {
        panic!("actual member must supply the successor");
    };
    assert_eq!(instruction.bindings.len(), 1);
    let successor = reference.clone();
    let marks = read(input, s::ReadSelector::Marks {});
    let s::QueryResult::Marks { next_span_id, .. } = marks.result() else {
        panic!("actual marks allocation required");
    };
    let span = s::SpanRef {
        seed_id: successor.plan.plan_id.seed_id.clone(),
        local_id: *next_span_id,
        revision: 1.into(),
    };
    input
        .serving
        .owner
        .context
        .begin_filing(
            activation(&successor, &span),
            PumpDeadline::Decision,
            &mut MonotonicIo,
        )
        .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let filing = loop {
        if let Some(done) = pump(input, |context, ports| {
            context.advance_filing(ports).unwrap();
            let done = context.take_filing(ports.io).unwrap();
            if done.is_none() {
                context.pump_once(ports).unwrap();
            }
            done
        }) {
            break done;
        }
        assert!(Instant::now() < end, "activation filing did not settle");
    };
    input
        .serving
        .owner
        .context
        .begin_effect_plan_summary(filing, &mut MonotonicIo)
        .unwrap();
    let summary = loop {
        if let Some(done) = pump(input, |context, ports| {
            context.advance_plan_summary(ports).unwrap();
            let done = context.take_plan_summary(ports.io).unwrap();
            if done.is_none() {
                context.pump_once(ports).unwrap();
            }
            done
        }) {
            let PlanSummaryCompletion::Effect(proof) = done else {
                panic!("activation must retain its refreshed summary");
            };
            break proof;
        }
        assert!(Instant::now() < end, "activation summary did not settle");
    };
    let s::QueryResult::PlanCurrent {
        current: Some(plan),
        ..
    } = summary.read().result()
    else {
        panic!("activation must retain the actual current plan");
    };
    let active = plan
        .steps
        .iter()
        .find(|step| step.step_id == successor.step_id)
        .unwrap();
    assert_eq!(active.status, s::StepStatus::Active);
    assert_eq!(active.span.as_ref(), Some(&span));
    let through = plan.last_source.clone();
    input
        .serving
        .owner
        .context
        .begin_place_effect_marks(summary, &mut MonotonicIo)
        .unwrap();
    let native = loop {
        if let Some(done) = pump(input, |context, ports| {
            context.advance_place_marks(ports).unwrap();
            let done = context.take_place_marks(ports.io).unwrap();
            if done.is_none() {
                context.pump_once(ports).unwrap();
            }
            done
        }) {
            break done;
        }
        assert!(Instant::now() < end, "native activation did not settle");
    };
    assert!(
        native.positions().is_empty(),
        "activation grants no observed endpoint"
    );
    assert_eq!(
        native.prerequisite().summary().unwrap().last_source,
        through
    );
    assert!(successor_read.belongs_to(input.serving.owner.context));
    assert!(marks.belongs_to(input.serving.owner.context));
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
        let (successor, span, through) = activate(input);
        let before = input
            .serving
            .owner
            .context
            .committed_plan_summary()
            .unwrap()
            .cloned();
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
        if mode.ends_with("-work") {
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
        }
        assert!(input.take_plan_operation().is_none());
        assert_eq!(
            input
                .serving
                .owner
                .context
                .committed_plan_summary()
                .unwrap(),
            before.as_ref()
        );
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
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "plan.event")
            .count(),
        1,
        "only fixture activation changes the plan"
    );
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
