//! Spec 07 PLAN-OBSERVATION next_ready and Spec 09 PLAN-MEASURES: one interval
//! from a consumed closure receipt to the Recorded placement of the step the
//! following activation actually activated, in one run, with no bridged
//! replacement, reload or missing endpoint. The close, the step_successor
//! query, the activation, the step instruction's filing and the placement are
//! the actual owners against the scripted member and decoder.
use super::*;
use crate::flow::context::PlacementCompletion;
use crate::flow::context::next_ready::{NextReadyRefusal, NextReadyStart};
use crate::flow::context::plan_port_driver::{ActivationReceipt, ClosureReceipt};
use crate::flow::context::step_instruction::StepMeta;
use crate::flow::context::worker_startup::input::generation::step_instruction::StepPlacementReceipt;
use crate::flow::ledger::SourceDisposition;
use crate::flow::material::SelectedUnit;
use crate::flow::place::{PlaceError, PlaceRequest, PlacementGrant};
use std::sync::Arc;
use weaver_trace::Sequence;
use weaver_types::{TokenAnswer, TokenDirective};

const TURN: &str = "closure";

/// Close the pinned step and take its receipt; the closure turn stays open.
fn closed(fx: &mut SummaryFixture, certificate: &TemplateCertificate) -> ClosureReceipt {
    closed_at(fx, certificate, 4)
}

/// Close with the catalog's next span id at `next_span_id`.
fn closed_at(
    fx: &mut SummaryFixture,
    certificate: &TemplateCertificate,
    next_span_id: u64,
) -> ClosureReceipt {
    let evidence = evidence(fx);
    begin_close(fx, evidence);
    serve(fx, PREVIEW, PREVIEW.len());
    let outcome = serve_marks(fx, next_span_id);
    effect_ready(fx);
    let run = fx.run();
    run.context
        .begin_plan_effect(certificate, &mut run.clock)
        .unwrap();
    let source = committed(fx, false);
    let snapshot = closed_snapshot(fx, &source, &outcome);
    release_summary(fx, snapshot);
    finish(fx)
        .into_closure_receipt()
        .unwrap_or_else(|_| panic!("a committed close grants its receipt"))
}

fn step_id(local: u64) -> s::StepId {
    s::StepId {
        local_id: local.into(),
        ..pinned().step_id
    }
}

fn current(fx: &mut SummaryFixture) -> s::PlanSnapshot {
    let s::QueryResult::PlanCurrent {
        current: Some(plan),
        ..
    } = fx
        .run()
        .context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .result()
    else {
        panic!("committed plan")
    };
    plan.clone()
}

/// Serve one released read of `result` under `snapshot_id`.
fn serve_result(fx: &mut SummaryFixture, result: &s::QueryResult, snapshot_id: u64) {
    let read = next(fx);
    let bytes = serde_json::to_vec(result).unwrap();
    fx.boot
        .member
        .respond(&read, page(snapshot_id, 0, bytes.len(), &bytes));
    let release = next(fx);
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: snapshot_id.into(),
        },
    );
}

/// The member's STEP-QUERY answer naming `successor` after `previous`.
fn successor_result(
    plan: &s::PlanSnapshot,
    successor: &s::StepId,
    previous: &s::StepId,
) -> s::QueryResult {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    let mut result: s::QueryResult = serde_json::from_value(
        cases
            .iter()
            .find(|case| case["case"] == "step_successor_exact_instruction")
            .unwrap()["result"]
            .clone(),
    )
    .unwrap();
    let s::QueryResult::StepSuccessor {
        reference,
        instruction,
    } = &mut result
    else {
        unreachable!()
    };
    let step = plan
        .steps
        .iter()
        .find(|step| &step.step_id == successor)
        .unwrap();
    *reference = s::StepQueryRef {
        plan: plan.plan.clone(),
        previous_step: Some(previous.clone()),
        step_id: successor.clone(),
        instruction: step.instruction.clone(),
    };
    instruction.step_query = Some(reference.clone());
    // The held statement is the step's own instruction, as the member reads it.
    let s::ReadSource::Event(source) = &mut instruction.source else {
        unreachable!("fixture successor reads an event statement")
    };
    source.block_id = step.instruction.block_id.clone();
    source.envelope.sequence = step.instruction.block_id.sequence;
    instruction.bindings[0].ordinal = step.instruction.binding;
    instruction.pairs.clear();
    result
}

/// step_successor after the closed step, then activate its answer: the
/// actual query, Marks allocation, plan.activate filing and summary refresh.
fn activate(
    fx: &mut SummaryFixture,
    certificate: &TemplateCertificate,
    successor: &s::StepId,
) -> ActivationReceipt {
    activate_after(fx, certificate, successor, &pinned().step_id, 5)
}

/// Activate `successor`, whose immediate predecessor is `previous`, with the
/// catalog's next span id at `next_span_id`.
fn activate_after(
    fx: &mut SummaryFixture,
    certificate: &TemplateCertificate,
    successor: &s::StepId,
    previous: &s::StepId,
    next_span_id: u64,
) -> ActivationReceipt {
    let plan = current(fx);
    begin(
        fx,
        PlanRequest::StepSuccessor {
            expected: plan.plan.clone(),
            previous_step: Some(previous.clone()),
        },
    );
    serve_result(fx, &successor_result(&plan, successor, previous), 61);
    let Ok(checked) = finish(fx).into_successor() else {
        panic!("the member's successor answer is a checked successor")
    };
    let run = fx.run();
    run.context
        .begin_plan_operation(
            PlanRequest::Activate {
                expected: plan.plan.clone(),
                step_id: successor.clone(),
                successor: checked,
            },
            Some(TurnRef(TURN.into())),
            &mut run.clock,
        )
        .unwrap();
    // After the close only the task remains open, below next_span_id.
    let marks = s::QueryResult::Marks {
        next_span_id: next_span_id.into(),
        open_scopes: vec![s::SpanInfo {
            span: plan.task.task.clone(),
            parent: None,
            kind: s::SpanKind::Task,
            status: s::SpanStatus::Open,
            opened_source: Some(s::SourceBlockId {
                run: "run".into(),
                sequence: 1.into(),
                block: 0.into(),
            }),
            ended_source: None,
            replacement: None,
        }],
        current_task: Some(plan.task.clone()),
    };
    serve_result(fx, &marks, 62);
    let prepared = (0..128).any(|_| advance(fx).unwrap() == PlanPortProgress::EffectReady);
    assert!(prepared, "activation must prepare its effect");
    let s::PlanEffect::Activate { step_span, .. } = fx.run().context.test_plan_effect().unwrap()
    else {
        panic!("an activation effect")
    };
    let step_span = step_span.clone();
    let run = fx.run();
    run.context
        .begin_plan_effect(certificate, &mut run.clock)
        .unwrap();
    let source = committed(fx, false);
    let mut snapshot = current(fx);
    let step = snapshot
        .steps
        .iter_mut()
        .find(|step| &step.step_id == successor)
        .unwrap();
    step.status = s::StepStatus::Active;
    step.span = Some(step_span);
    snapshot.last_source = source;
    release_summary(fx, snapshot);
    finish(fx)
        .into_activation_receipt()
        .unwrap_or_else(|_| panic!("a committed activation grants its receipt"))
}

/// The filed step instruction for the activated step, and its unit: Spec 08's
/// instruction unit, or the same source converted as a block unit.
fn filed_instruction(
    fx: &mut SummaryFixture,
    certificate: &TemplateCertificate,
    activation: &ActivationReceipt,
    block_unit: bool,
) -> (StepMeta, s::SourceBlockId, SelectedUnit) {
    let run = fx.run();
    let draft = run
        .context
        .step_instruction(
            activation.plan(),
            &activation.step().step_id,
            &TurnRef(TURN.into()),
            certificate,
        )
        .unwrap();
    let meta = draft.step.clone();
    let expansion = draft.expansion;
    let proof = fx.file(draft.event, false);
    let Some(SourceDisposition::Committed(source)) =
        proof.into_receipts().into_dispositions().unwrap().next()
    else {
        panic!("a committed step instruction")
    };
    let block = source.source().clone();
    let origin = Arc::clone(fx.run().context.material_origin());
    let unit = if block_unit {
        crate::flow::material::from_filed(origin, source).unwrap()
    } else {
        crate::flow::material::from_filed_instruction(origin, source, expansion).unwrap()
    };
    (meta, block, unit)
}

fn place_request(
    fx: &mut SummaryFixture,
    unit: SelectedUnit,
    instruction: &s::SourceBlockId,
) -> PlaceRequest {
    let run = fx.run();
    let schemas = run.context.loaded_place_schemas().unwrap().to_vec();
    let prefix = run
        .context
        .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
        .unwrap();
    PlaceRequest {
        prefix,
        selection: vec![unit],
        tool_schemas: schemas,
        context: s::PlacementContext {
            label: "next ready".into(),
            instruction: Some(instruction.clone()),
        },
    }
}

/// Place the instruction unit through the actual owner: one measured delta,
/// then its Recorded grant. ended_ns is read as the grant is taken.
fn place(
    fx: &mut SummaryFixture,
    port: &mut DecodePort,
    peer: &crate::channel::ChildEnd,
    request: PlaceRequest,
) -> (PlacementGrant, u64) {
    let run = &mut fx.boot.member.run;
    run.context
        .begin_place(request, port, &mut run.clock)
        .unwrap();
    let mut measured = false;
    for _ in 0..256 {
        if !measured && let Some(raw) = receive(peer.as_fd()) {
            let TokenDirective::MeasureDelta { keep, delta, .. } =
                serde_json::from_slice(&raw).unwrap()
            else {
                panic!("placement only measures")
            };
            assert_eq!(delta.len(), 1, "the instruction unit's one message");
            let snapshot = fx
                .run()
                .context
                .boot_journal
                .as_ref()
                .unwrap()
                .boot_snapshot();
            let (prefix, resident, capacity) = (snapshot.0, snapshot.1, snapshot.2);
            let answer = TokenAnswer::DeltaMeasured {
                resident,
                prefix_tokens: prefix,
                keep,
                input_tokens: 3,
                capacity,
            };
            send_bytes(peer.as_fd(), &serde_json::to_vec(&answer).unwrap());
            measured = true;
        }
        let run = &mut fx.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: port.fd().as_raw_fd(),
        };
        run.context
            .advance_place(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(port),
                label: None,
                io: &mut io,
            })
            .unwrap();
        let run = fx.run();
        if let Some(done) = run.context.take_placement(&mut run.clock).unwrap() {
            let PlacementCompletion::Granted(grant) = done else {
                panic!("the step instruction placement must be granted")
            };
            let ended = run.clock.now().unwrap();
            return (grant, ended);
        }
    }
    panic!("placement did not finish");
}

struct Chain {
    fx: SummaryFixture,
    port: DecodePort,
    _peer: crate::channel::ChildEnd,
    grant: Option<PlacementGrant>,
    start: NextReadyStart,
    activation: ActivationReceipt,
    placed: StepPlacementReceipt,
}

/// Release the placement owner the way a caller does before its first byte:
/// begin the grant's generation and cancel it. Until then the outstanding
/// grant holds external authorship, the measurement's included.
fn settle(chain: &mut Chain) {
    let grant = chain.grant.take().unwrap();
    let run = &mut chain.fx.boot.member.run;
    run.context
        .begin_generation(grant, &mut chain.port, &mut run.clock)
        .unwrap();
    run.context
        .cancel_generation(&mut chain.port, &mut run.clock)
        .unwrap();
    for _ in 0..128 {
        let run = &mut chain.fx.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: chain.port.fd().as_raw_fd(),
        };
        run.context
            .advance_generation(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut chain.port),
                label: None,
                io: &mut io,
            })
            .unwrap();
        chain.fx.classifier(false);
        chain.fx.file_reply();
        let run = chain.fx.run();
        if let Some(done) = run.context.take_placed_generation(&mut run.clock).unwrap() {
            assert!(matches!(done, GenerationCompletion::Cancelled(_)));
            return;
        }
    }
    panic!("cancelled generation did not settle");
}

/// The whole chain: close step 1, consume its receipt, activate `successor`
/// after `previous`, file its step instruction, then `before_place` (a change
/// between closure and placement), then the actual placement.
fn chain_over(
    edit: impl FnOnce(&mut s::PlanSnapshot),
    successor: u64,
    previous: u64,
    next_span_id: u64,
    before_place: impl FnOnce(&mut SummaryFixture),
) -> Chain {
    let (mut fx, certificate) = closure_fixture_with(false, true, edit);
    let (mut port, peer) = fx._decode.take().unwrap();
    let receipt = closed_at(&mut fx, &certificate, next_span_id);
    let run = fx.run();
    let start = run
        .context
        .begin_next_ready(receipt, &mut run.clock)
        .unwrap();
    let activation = activate_after(
        &mut fx,
        &certificate,
        &step_id(successor),
        &step_id(previous),
        next_span_id + 1,
    );
    let (meta, source, unit) = filed_instruction(&mut fx, &certificate, &activation, false);
    before_place(&mut fx);
    let request = place_request(&mut fx, unit, &source);
    let (grant, ended_ns) = place(&mut fx, &mut port, &peer, request);
    let placed =
        StepPlacementReceipt::from_grant(&grant, meta, source, &fx.run().context, ended_ns)
            .unwrap();
    Chain {
        fx,
        port,
        _peer: peer,
        grant: Some(grant),
        start,
        activation,
        placed,
    }
}

fn chain_with(before_place: impl FnOnce(&mut SummaryFixture)) -> Chain {
    chain_over(|_| {}, 2, 1, 4, before_place)
}

fn chain() -> Chain {
    chain_with(|_| {})
}

fn refusal(chain: &mut Chain) -> NextReadyRefusal {
    chain
        .fx
        .run()
        .context
        .next_ready_event(&chain.start, &chain.activation, &chain.placed)
        .unwrap_err()
}

#[test]
fn next_ready_measures_closure_consumption_to_the_activated_successors_recorded_placement() {
    let mut chain = chain();
    let began = chain.start.began_ns;
    let ended = chain.placed.ended_ns;
    assert!(began > 0 && ended >= began);
    let closed = chain.start.receipt.closed().clone();
    // The placement receipt is the actual state.placement record, of the
    // unit's message, after the filed step instruction.
    let placements = chain.fx.records(Kind::StatePlacement);
    let placement = placements
        .iter()
        .find(|row| row["sequence"] == serde_json::to_value(chain.placed.placement).unwrap())
        .expect("the receipt names a Recorded state.placement");
    assert_eq!(placement["turn"], TURN);
    let instructions = chain.fx.records(Kind::LoopInstruction);
    assert_eq!(instructions.len(), 1);
    assert_eq!(
        instructions[0]["payload"]["instruction"]["step"]["step_id"],
        serde_json::to_value(step_id(2)).unwrap()
    );
    let event = chain
        .fx
        .run()
        .context
        .next_ready_event(&chain.start, &chain.activation, &chain.placed)
        .unwrap();
    assert_eq!(event.kind, Kind::StatePlanMeasurement);
    assert_eq!(event.turn, Some(TurnRef(TURN.into())));
    assert_eq!(event.causal_parent, Some(chain.placed.placement));
    // Building the event twice reads no clock: a retry keeps both endpoints.
    let again = chain
        .fx
        .run()
        .context
        .next_ready_event(&chain.start, &chain.activation, &chain.placed)
        .unwrap();
    assert_eq!(
        format!("{:?}", again.payload),
        format!("{:?}", event.payload)
    );
    settle(&mut chain);
    let run = chain.fx.run();
    run.context
        .author_and_wait(&mut ports(&mut run.clock), event)
        .unwrap();
    let rows = chain.fx.records(Kind::StatePlanMeasurement);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["turn"], TURN);
    assert_eq!(
        rows[0]["payload"],
        serde_json::json!({
            "instrument_version": "plan-observation/1",
            "plan": serde_json::to_value(pinned().plan).unwrap(),
            "event": {"next_ready": {
                "closed": serde_json::to_value(&closed).unwrap(),
                "previous_step": serde_json::to_value(pinned().step_id).unwrap(),
                "step_id": serde_json::to_value(step_id(2)).unwrap(),
                "assembly_id": serde_json::to_value(chain.placed.assembly_id()).unwrap(),
                "began_ns": began.to_string(),
                "ended_ns": ended.to_string(),
                "closure_to_next_ns": (ended - began).to_string(),
            }},
        })
    );
}

/// Step 5, ordinal 2, between the closed step 1 and step 2 (now ordinal 3)
/// holds a preserved closure: the operator's, which predates this one. The
/// member's step_successor answer and the activation name step 2, the first
/// waiting step; the snapshot's next step after the closed one is step 5.
fn preserved_chain(before_place: impl FnOnce(&mut SummaryFixture)) -> Chain {
    chain_over(
        |plan| {
            let mut preserved = plan.steps[0].clone();
            preserved.step_id.local_id = 5.into();
            preserved.ordinal = 2.into();
            preserved.instruction.block_id.block = 5.into();
            preserved.status = s::StepStatus::Closed;
            preserved.span = Some(s::SpanRef {
                local_id: 10.into(),
                ..pinned().span
            });
            preserved.outcome_span = Some(s::SpanRef {
                local_id: 11.into(),
                ..pinned().span
            });
            preserved.work = Some(s::WorkRef {
                step_id: preserved.step_id.clone(),
                span: preserved.span.clone().unwrap(),
                ..pinned()
            });
            preserved.closure_source = Some(plan.operator_source.clone());
            plan.steps[1].ordinal = 3.into();
            plan.steps.insert(1, preserved);
        },
        2,
        5,
        12,
        before_place,
    )
}

#[test]
fn next_ready_names_the_activated_successor_not_the_snapshot_order_next_step() {
    let mut chain = preserved_chain(|_| {});
    assert_eq!(chain.activation.step().step_id, step_id(2));
    let event = chain
        .fx
        .run()
        .context
        .next_ready_event(&chain.start, &chain.activation, &chain.placed)
        .unwrap();
    let Some(Payload::PlanMeasurement(measurement)) = &event.payload else {
        panic!("a plan measurement")
    };
    let value = serde_json::to_value(measurement).unwrap();
    assert_eq!(
        value["event"]["next_ready"]["step_id"],
        serde_json::to_value(step_id(2)).unwrap()
    );
    // A placement naming the snapshot-order next step is not the successor.
    chain.placed.meta.step_id = step_id(5);
    assert_eq!(refusal(&mut chain), NextReadyRefusal::NotTheSuccessor);
}

#[test]
fn next_ready_refuses_an_activation_that_does_not_follow_this_closure() {
    // A step between the closed step and the successor closed later in this
    // run: a later closure, not this one, preceded the activation.
    let mut chain = preserved_chain(|fx| {
        swap_current(fx, |plan| {
            let later = plan.last_source.block_id.clone();
            plan.steps[1].closure_source = Some(later);
        })
    });
    assert_eq!(refusal(&mut chain), NextReadyRefusal::NotTheSuccessor);
    // The successor's predecessor in the committed revision is not the step
    // its step_successor query named.
    let mut chain = preserved_chain(|fx| {
        swap_current(fx, |plan| {
            plan.steps[1].step_id.local_id = 9.into();
            if let Some(work) = plan.steps[1].work.as_mut() {
                work.step_id.local_id = 9.into();
            }
        })
    });
    assert_eq!(refusal(&mut chain), NextReadyRefusal::NotTheSuccessor);
}

#[test]
fn next_ready_takes_only_this_runs_closure_receipt_and_placed_unit() {
    let (mut other, certificate) = closure_fixture(false, true);
    let foreign = closed(&mut other, &certificate);
    let mut chain = chain();
    assert_eq!(foreign.closed(), chain.start.receipt.closed());
    chain.start.receipt = foreign;
    assert_eq!(refusal(&mut chain), NextReadyRefusal::CrossRun);
    // An identical chain's placed instruction unit, from another run.
    let activation = activate(&mut other, &certificate, &step_id(2));
    let (_, source, unit) = filed_instruction(&mut other, &certificate, &activation, false);
    let mut chain = self::chain();
    assert_eq!(&source, chain.placed.source());
    chain.placed.unit = Arc::new(unit);
    assert_eq!(refusal(&mut chain), NextReadyRefusal::CrossRun);
}

#[test]
fn next_ready_takes_only_this_runs_activation_receipt() {
    // An identical chain's activation, taken by another run's context.
    let mut chain = chain();
    let (mut other, certificate) = closure_fixture(false, true);
    let _ = closed(&mut other, &certificate);
    let foreign = activate(&mut other, &certificate, &step_id(2));
    assert_eq!(foreign.step(), chain.activation.step());
    assert_eq!(foreign.activated(), chain.activation.activated());
    chain.activation = foreign;
    assert_eq!(refusal(&mut chain), NextReadyRefusal::CrossRun);
}

#[test]
fn next_ready_interval_is_the_exact_difference_of_its_two_readings() {
    // Equal readings are a genuine zero interval, not clamped or invented.
    for gap in [0, 7] {
        let mut chain = chain();
        chain.placed.ended_ns = chain.start.began_ns + gap;
        let event = chain
            .fx
            .run()
            .context
            .next_ready_event(&chain.start, &chain.activation, &chain.placed)
            .unwrap();
        let Some(Payload::PlanMeasurement(measurement)) = &event.payload else {
            panic!("a plan measurement")
        };
        let value = serde_json::to_value(measurement).unwrap();
        assert_eq!(
            value["event"]["next_ready"]["closure_to_next_ns"],
            gap.to_string()
        );
    }
}

#[test]
fn next_ready_invents_no_zero_and_no_reversed_interval() {
    let mut chain = chain();
    let began = chain.start.began_ns;
    chain.placed.ended_ns = began - 1;
    assert_eq!(refusal(&mut chain), NextReadyRefusal::ClockOrder);
    chain.placed.ended_ns = began + 3;
    chain.start.began_ns = 0;
    assert_eq!(refusal(&mut chain), NextReadyRefusal::ClockOrder);
    assert!(chain.fx.records(Kind::StatePlanMeasurement).is_empty());
}

#[test]
fn next_ready_requires_one_runs_sources_in_chain_order() {
    // The placement must follow its filed instruction.
    let mut chain = chain();
    let source = chain.placed.source.sequence.0;
    chain.placed.placement = Sequence(source);
    assert_eq!(refusal(&mut chain), NextReadyRefusal::ClockOrder);
    // The filed instruction must follow the activation, so the closure.
    let mut chain = self::chain();
    chain.placed.source.sequence = chain.activation.activated().sequence;
    assert_eq!(refusal(&mut chain), NextReadyRefusal::ClockOrder);
    // The filed instruction must be this run's source.
    let mut chain = self::chain();
    chain.placed.source.run = "another-run".into();
    assert_eq!(refusal(&mut chain), NextReadyRefusal::ClockOrder);
}

#[test]
fn next_ready_refuses_another_runs_clock_and_so_a_reload() {
    for side in ["start", "placement"] {
        let mut chain = chain();
        // A save-point reload enters a new run: a new origin and clock.
        match side {
            "start" => chain.start.origin = Arc::new(()),
            _ => chain.placed.origin = Arc::new(()),
        }
        assert_eq!(refusal(&mut chain), NextReadyRefusal::CrossRun, "{side}");
    }
}

#[test]
fn next_ready_does_not_bridge_a_replacement_between_closure_and_placement() {
    let mut chain = chain();
    settle(&mut chain);
    swap_current(&mut chain.fx, |plan| {
        plan.plan.revision = (plan.plan.revision.0 + 1).into()
    });
    assert_eq!(refusal(&mut chain), NextReadyRefusal::Replaced);
    // The same revision, no longer locked.
    let mut chain = self::chain();
    settle(&mut chain);
    swap_current(&mut chain.fx, |plan| plan.status = s::PlanStatus::Broken);
    assert_eq!(refusal(&mut chain), NextReadyRefusal::Replaced);
    let mut chain = self::chain();
    chain.placed.meta.plan.revision = (chain.placed.meta.plan.revision.0 + 1).into();
    assert_eq!(refusal(&mut chain), NextReadyRefusal::Replaced);
}

#[test]
fn next_ready_does_not_bridge_a_missing_endpoint() {
    // The closure is gone from the committed revision.
    let mut chain = chain_with(|fx| swap_current(fx, |plan| plan.steps[0].closure_source = None));
    assert_eq!(refusal(&mut chain), NextReadyRefusal::MissingEndpoint);
    // The receipt's closed step is gone, though its closure is held by
    // another step id.
    let mut chain = chain_with(|fx| {
        swap_current(fx, |plan| {
            plan.steps[0].step_id.local_id = 7.into();
            if let Some(work) = plan.steps[0].work.as_mut() {
                work.step_id.local_id = 7.into();
            }
        })
    });
    assert_eq!(refusal(&mut chain), NextReadyRefusal::MissingEndpoint);
    // The activated step is no longer active.
    let mut chain =
        chain_with(|fx| swap_current(fx, |plan| plan.steps[1].status = s::StepStatus::Waiting));
    assert_eq!(refusal(&mut chain), NextReadyRefusal::MissingEndpoint);
    // The activated step holds another span than its activation allocated.
    let mut chain = chain_with(|fx| {
        swap_current(fx, |plan| {
            let span = plan.steps[1].span.as_mut().unwrap();
            span.local_id = (span.local_id.0 + 1).into();
        })
    });
    assert_eq!(refusal(&mut chain), NextReadyRefusal::MissingEndpoint);
}

#[test]
fn next_ready_refuses_a_placement_that_is_not_the_activated_steps_instruction() {
    // A placement naming the closed step instead of the activated successor.
    let mut chain = chain();
    chain.placed.meta.step_id = pinned().step_id;
    assert_eq!(refusal(&mut chain), NextReadyRefusal::NotTheSuccessor);
    // A placed unit that is not the named filed instruction's unit.
    let mut chain = self::chain();
    chain.placed.source.block = (chain.placed.source.block.0 + 1).into();
    assert_eq!(refusal(&mut chain), NextReadyRefusal::NotTheSuccessor);
    // A placement naming the activated step under another span.
    let mut chain = self::chain();
    chain.placed.meta.span.local_id = (chain.placed.meta.span.local_id.0 + 1).into();
    assert_eq!(refusal(&mut chain), NextReadyRefusal::NotTheSuccessor);
}

#[test]
fn placement_admits_a_step_instruction_only_as_its_last_instruction_unit() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let (mut port, _peer) = fx._decode.take().unwrap();
    let _receipt = closed(&mut fx, &certificate);
    let activation = activate(&mut fx, &certificate, &step_id(2));
    let (_, source, block) = filed_instruction(&mut fx, &certificate, &activation, true);
    assert!(matches!(block.unit_id(), s::UnitId::Block { .. }));
    let request = place_request(&mut fx, block, &source);
    let placements = fx.records(Kind::StatePlacement).len();
    let run = &mut fx.boot.member.run;
    let refused = run
        .context
        .begin_place(request, &mut port, &mut run.clock)
        .unwrap_err();
    assert_eq!(refused.refusal, PlaceError::InvalidContext);
    assert!(refused.failure.is_none());
    assert_eq!(fx.records(Kind::StatePlacement).len(), placements);
}

#[test]
fn next_ready_takes_only_this_runs_closure_receipt() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let receipt = closed(&mut fx, &certificate);
    let (mut other, _) = closure_fixture(false, true);
    let run = other.run();
    let (back, failure) = run
        .context
        .begin_next_ready(receipt, &mut run.clock)
        .unwrap_err();
    assert!(failure.is_none());
    assert_eq!(back.previous_step(), &pinned().step_id);
}
