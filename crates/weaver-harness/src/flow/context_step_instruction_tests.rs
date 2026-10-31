//! Spec 02's step loop.instruction against the committed active step, filed
//! through the scripted classifier and member, and Spec 08's instruction unit.
use super::*;
use crate::flow::context::step_instruction::{StepInstructionError, StepInstructionRefused};
use crate::flow::ledger::SourceDisposition;
use std::sync::Arc;

fn open_turn(fx: &mut SummaryFixture, turn: &str) -> TurnRef {
    let run = fx.run();
    let turn = TurnRef(turn.into());
    run.context
        .author_and_wait(
            &mut ports(&mut run.clock),
            AuthoredEvent {
                kind: Kind::TurnStarted,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(turn.clone()),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: None,
            },
        )
        .unwrap();
    turn
}

fn refused(result: Result<impl std::fmt::Debug, StepInstructionError>) -> StepInstructionRefused {
    match result {
        Err(StepInstructionError::Refused(refused)) => refused,
        other => panic!("step instruction must refuse: {other:?}"),
    }
}

fn record(refused: &StepInstructionRefused) -> serde_json::Value {
    let Some(Payload::Refusal(raw)) = &refused.event.payload else {
        panic!("a refusal record")
    };
    let value: serde_json::Value = serde_json::from_str(raw.get()).unwrap();
    assert_eq!(value["seam"], "instruction");
    value
}

#[test]
fn step_instruction_names_the_active_step_and_files_one_instruction_unit() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let turn = open_turn(&mut fx, "closure");
    let work = pinned();
    let run = fx.run();
    let draft = run
        .context
        .step_instruction(&work.plan, &work.step_id, &turn, &certificate)
        .unwrap();
    assert_eq!(draft.step.plan, work.plan);
    assert_eq!(draft.step.step_id, work.step_id);
    assert_eq!(draft.step.span, work.span);
    assert_eq!(draft.step.turn, turn);
    let instruction = s::StatementRef {
        block_id: s::SourceBlockId {
            run: "run".into(),
            sequence: 1.into(),
            block: 1.into(),
        },
        binding: 0.into(),
    };
    assert_eq!(draft.step.instruction, instruction);
    let expansion = draft.expansion;
    let proof = fx.file(draft.event, false);
    let rows = fx.records(Kind::LoopInstruction);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["turn"], "closure");
    assert_eq!(rows[0]["payload"]["template_id"], "step.instruction");
    assert_eq!(
        rows[0]["payload"]["instruction"],
        serde_json::json!({"step": {
            "plan": serde_json::to_value(&work.plan).unwrap(),
            "step_id": serde_json::to_value(&work.step_id).unwrap(),
            "instruction": serde_json::to_value(&instruction).unwrap(),
            "span": serde_json::to_value(&work.span).unwrap(),
        }})
    );
    let Some(SourceDisposition::Committed(source)) =
        proof.into_receipts().into_dispositions().unwrap().next()
    else {
        panic!("a committed step instruction")
    };
    let block = source.source().clone();
    let run = fx.run();
    let unit = crate::flow::material::from_filed_instruction(
        Arc::clone(run.context.material_origin()),
        source,
        expansion,
    )
    .unwrap();
    assert_eq!(
        unit.unit_id(),
        &s::UnitId::Instruction {
            source: s::ContentRef::Event {
                block_id: block.clone()
            }
        }
    );
    assert_eq!(unit.messages().len(), 1);
    assert_eq!(unit.messages()[0].role, weaver_traits::Role::User);
    assert!(crate::flow::material::is_instruction_unit(
        &unit,
        &block,
        &run.context
    ));
    let other = s::SourceBlockId {
        block: 9.into(),
        ..block
    };
    assert!(!crate::flow::material::is_instruction_unit(
        &unit,
        &other,
        &run.context
    ));
}

#[test]
fn step_instruction_refuses_a_wrong_turn_before_its_step_context() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let open = open_turn(&mut fx, "closure");
    let work = pinned();
    let waiting = s::StepId {
        local_id: 2.into(),
        ..work.step_id.clone()
    };
    let run = fx.run();
    for turn in [TurnRef("other".into()), TurnRef(String::new())] {
        // A waiting step and a foreign turn: the turn is checked first.
        let refusal =
            refused(
                run.context
                    .step_instruction(&work.plan, &waiting, &turn, &certificate),
            );
        assert_eq!(refusal.case, s::InstructionRefusal::WrongTurn);
        assert_eq!(record(&refusal)["case"], "wrong_turn");
        assert_eq!(record(&refusal)["template_id"], "step.instruction");
        assert_eq!(refusal.event.turn.as_ref(), Some(&open));
        assert_eq!(refusal.event.kind, Kind::Refusal);
    }
}

#[test]
fn step_instruction_refuses_a_step_that_is_not_active_in_the_expected_revision() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let turn = open_turn(&mut fx, "closure");
    let work = pinned();
    let waiting = s::StepId {
        local_id: 2.into(),
        ..work.step_id.clone()
    };
    let replaced = s::PlanRef {
        revision: (work.plan.revision.0 + 1).into(),
        ..work.plan.clone()
    };
    let run = fx.run();
    for (plan, step) in [
        (&work.plan, &waiting),
        (&replaced, &work.step_id),
        (
            &work.plan,
            &s::StepId {
                seed_id: "other".into(),
                ..work.step_id.clone()
            },
        ),
    ] {
        let refusal = refused(
            run.context
                .step_instruction(plan, step, &turn, &certificate),
        );
        assert_eq!(refusal.case, s::InstructionRefusal::InvalidContext);
        assert_eq!(record(&refusal)["case"], "invalid_context");
    }
    // The pinned step, still holding its span, once it is no longer active.
    swap_current(&mut fx, |plan| plan.steps[0].status = s::StepStatus::Closed);
    let run = fx.run();
    let refusal =
        refused(
            run.context
                .step_instruction(&work.plan, &work.step_id, &turn, &certificate),
        );
    assert_eq!(refusal.case, s::InstructionRefusal::InvalidContext);
    assert!(fx.records(Kind::LoopInstruction).is_empty());
}

#[test]
fn step_instruction_refuses_a_certificate_without_the_step_family() {
    let (mut fx, certificate) = closure_fixture(false, false);
    let turn = open_turn(&mut fx, "closure");
    let work = pinned();
    let run = fx.run();
    let refusal =
        refused(
            run.context
                .step_instruction(&work.plan, &work.step_id, &turn, &certificate),
        );
    assert_eq!(refusal.case, s::InstructionRefusal::InvalidTemplate);
    assert_eq!(record(&refusal)["case"], "invalid_template");
}

#[test]
fn a_block_unit_of_the_same_filed_instruction_is_not_its_instruction_unit() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let turn = open_turn(&mut fx, "closure");
    let work = pinned();
    let run = fx.run();
    let draft = run
        .context
        .step_instruction(&work.plan, &work.step_id, &turn, &certificate)
        .unwrap();
    let proof = fx.file(draft.event, false);
    let Some(SourceDisposition::Committed(source)) =
        proof.into_receipts().into_dispositions().unwrap().next()
    else {
        panic!("a committed step instruction")
    };
    let block = source.source().clone();
    let run = fx.run();
    // The wind-down conversion of the same source: a block unit, no grant.
    let unit = Arc::new(
        crate::flow::material::from_filed(Arc::clone(run.context.material_origin()), source)
            .unwrap(),
    );
    assert!(matches!(unit.unit_id(), s::UnitId::Block { .. }));
    assert!(!crate::flow::material::is_instruction_unit(
        &unit,
        &block,
        &run.context
    ));
    assert!(!crate::flow::context::worker_startup::input::generation::step_instruction::placed_step_unit(
        &[unit],
        &block,
        &run.context
    ));
}

#[test]
fn step_instruction_decides_nothing_while_the_committed_summary_settles() {
    // A close's plan effect is in flight: the committed summary is pending,
    // which is neither "not the active step" nor a recorded refusal.
    let (mut fx, certificate) = closure_fixture(false, true);
    let evidence = evidence(&mut fx);
    begin_close(&mut fx, evidence);
    serve(&mut fx, PREVIEW, PREVIEW.len());
    serve_marks(&mut fx, 4);
    effect_ready(&mut fx);
    let run = fx.run();
    run.context
        .begin_plan_effect(&certificate, &mut run.clock)
        .unwrap();
    // Its effect is committed; the summary refresh is not yet released.
    let _source = committed(&mut fx, false);
    let run = fx.run();
    assert_eq!(
        run.context.committed_plan_summary().err(),
        Some(crate::flow::plan::PlanSummaryUnavailable::Pending)
    );
    let work = pinned();
    let result = run.context.step_instruction(
        &work.plan,
        &work.step_id,
        &TurnRef("closure".into()),
        &certificate,
    );
    assert!(
        matches!(result, Err(StepInstructionError::Pending)),
        "{result:?}"
    );
    assert!(fx.records(Kind::Refusal).is_empty());
}
