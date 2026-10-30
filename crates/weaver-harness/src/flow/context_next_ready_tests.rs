//! Spec 07 PLAN-OBSERVATION next_ready and Spec 09 PLAN-MEASURES: one interval
//! from a consumed closure receipt to the successor's Recorded placement, in
//! one run, with no bridged replacement, reload or missing endpoint.
use super::*;
use crate::flow::context::next_ready::{NextReadyRefusal, NextReadyStart};
use crate::flow::context::plan_port_driver::ClosureReceipt;
use crate::flow::context::step_instruction::StepMeta;
use crate::flow::context::worker_startup::input::generation::step_instruction::StepPlacementReceipt;
use crate::flow::ledger::SourceDisposition;
use std::sync::Arc;
use weaver_trace::Sequence;

/// Close the pinned step and take its receipt; the "closure" turn stays open.
fn closed(fx: &mut SummaryFixture, certificate: &TemplateCertificate) -> ClosureReceipt {
    let evidence = evidence(fx);
    begin_close(fx, evidence);
    serve(fx, PREVIEW, PREVIEW.len());
    let outcome = serve_marks(fx, 4);
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

fn successor() -> s::StepId {
    s::StepId {
        local_id: 2.into(),
        ..pinned().step_id
    }
}

/// The successor's activation, standing in for activate in the same revision.
fn activate_successor(fx: &mut SummaryFixture) -> s::SpanRef {
    let mut span = pinned().span;
    span.local_id = (span.local_id.0 + 7).into();
    let held = span.clone();
    swap_current(fx, move |plan| {
        plan.steps[1].status = s::StepStatus::Active;
        plan.steps[1].span = Some(held);
    });
    span
}

/// Author, file and place-stand-in the successor's step instruction. The
/// placement sequence is a real Recorded event after the filed instruction.
fn placed(
    fx: &mut SummaryFixture,
    certificate: &TemplateCertificate,
    block_unit: bool,
) -> StepPlacementReceipt {
    let turn = TurnRef("closure".into());
    let run = fx.run();
    let draft = run
        .context
        .step_instruction(&pinned().plan, &successor(), &turn, certificate)
        .unwrap();
    let meta: StepMeta = draft.step.clone();
    let expansion = draft.expansion;
    let proof = fx.file(draft.event, false);
    let Some(SourceDisposition::Committed(source)) =
        proof.into_receipts().into_dispositions().unwrap().next()
    else {
        panic!("a committed step instruction")
    };
    let block = source.source().clone();
    let run = fx.run();
    let origin = Arc::clone(run.context.material_origin());
    let unit = if block_unit {
        crate::flow::material::from_filed(Arc::clone(&origin), source).unwrap()
    } else {
        crate::flow::material::from_filed_instruction(Arc::clone(&origin), source, expansion)
            .unwrap()
    };
    // Any later Recorded event stands in for the placement's receipt.
    let wrong = run
        .context
        .step_instruction(
            &pinned().plan,
            &successor(),
            &TurnRef("x".into()),
            certificate,
        )
        .unwrap_err();
    let placement = run
        .context
        .author_and_wait(&mut ports(&mut run.clock), wrong.event)
        .unwrap();
    let ended_ns = run.clock.now().unwrap();
    StepPlacementReceipt {
        origin,
        meta,
        source: block,
        placement,
        assembly_id: 5.into(),
        ended_ns,
        unit: Arc::new(unit),
    }
}

struct Chain {
    fx: SummaryFixture,
    start: NextReadyStart,
    placed: StepPlacementReceipt,
}

fn chain() -> Chain {
    let (mut fx, certificate) = closure_fixture(false, true);
    let receipt = closed(&mut fx, &certificate);
    let run = fx.run();
    let start = run
        .context
        .begin_next_ready(receipt, &mut run.clock)
        .unwrap();
    activate_successor(&mut fx);
    let placed = placed(&mut fx, &certificate, false);
    Chain { fx, start, placed }
}

fn refusal(chain: &mut Chain) -> NextReadyRefusal {
    chain
        .fx
        .run()
        .context
        .next_ready_event(&chain.start, &chain.placed)
        .unwrap_err()
}

#[test]
fn next_ready_measures_closure_consumption_to_the_successors_recorded_placement() {
    let mut chain = chain();
    let began = chain.start.began_ns;
    let ended = chain.placed.ended_ns;
    assert!(began > 0 && ended >= began);
    let closed = chain.start.receipt.closed().clone();
    let event = chain
        .fx
        .run()
        .context
        .next_ready_event(&chain.start, &chain.placed)
        .unwrap();
    assert_eq!(event.kind, Kind::StatePlanMeasurement);
    assert_eq!(event.turn, Some(TurnRef("closure".into())));
    assert_eq!(event.causal_parent, Some(chain.placed.placement));
    // Building the event twice reads no clock: a retry keeps both endpoints.
    let again = chain
        .fx
        .run()
        .context
        .next_ready_event(&chain.start, &chain.placed)
        .unwrap();
    assert_eq!(
        format!("{:?}", again.payload),
        format!("{:?}", event.payload)
    );
    let run = chain.fx.run();
    run.context
        .author_and_wait(&mut ports(&mut run.clock), event)
        .unwrap();
    let rows = chain.fx.records(Kind::StatePlanMeasurement);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["turn"], "closure");
    assert_eq!(
        rows[0]["payload"],
        serde_json::json!({
            "instrument_version": "plan-observation/1",
            "plan": serde_json::to_value(pinned().plan).unwrap(),
            "event": {"next_ready": {
                "closed": serde_json::to_value(&closed).unwrap(),
                "previous_step": serde_json::to_value(pinned().step_id).unwrap(),
                "step_id": serde_json::to_value(successor()).unwrap(),
                "assembly_id": "5",
                "began_ns": began.to_string(),
                "ended_ns": ended.to_string(),
                "closure_to_next_ns": (ended - began).to_string(),
            }},
        })
    );
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
fn next_ready_refuses_a_placement_that_does_not_follow_its_filed_instruction() {
    let mut chain = chain();
    let source = chain.placed.source.sequence.0;
    chain.placed.placement = Sequence(source);
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
    swap_current(&mut chain.fx, |plan| {
        plan.plan.revision = (plan.plan.revision.0 + 1).into()
    });
    assert_eq!(refusal(&mut chain), NextReadyRefusal::Replaced);
    let mut chain = self::chain();
    chain.placed.meta.plan.revision = (chain.placed.meta.plan.revision.0 + 1).into();
    assert_eq!(refusal(&mut chain), NextReadyRefusal::Replaced);
}

#[test]
fn next_ready_does_not_bridge_a_missing_endpoint() {
    // The closure is gone from the committed revision.
    let mut chain = chain();
    swap_current(&mut chain.fx, |plan| plan.steps[0].closure_source = None);
    assert_eq!(refusal(&mut chain), NextReadyRefusal::MissingEndpoint);
    // The successor is not active.
    let mut chain = self::chain();
    swap_current(&mut chain.fx, |plan| {
        plan.steps[1].status = s::StepStatus::Waiting
    });
    assert_eq!(refusal(&mut chain), NextReadyRefusal::MissingEndpoint);
}

#[test]
fn next_ready_refuses_a_placement_without_the_successors_instruction_unit() {
    // The same filed instruction placed as a block unit.
    let (mut fx, certificate) = closure_fixture(false, true);
    let receipt = closed(&mut fx, &certificate);
    let run = fx.run();
    let start = run
        .context
        .begin_next_ready(receipt, &mut run.clock)
        .unwrap();
    activate_successor(&mut fx);
    let placed = placed(&mut fx, &certificate, true);
    let mut block = Chain { fx, start, placed };
    assert_eq!(refusal(&mut block), NextReadyRefusal::NotTheSuccessor);
    // A placement naming the closed step instead of its successor.
    let mut chain = chain();
    chain.placed.meta.step_id = pinned().step_id;
    assert_eq!(refusal(&mut chain), NextReadyRefusal::NotTheSuccessor);
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
