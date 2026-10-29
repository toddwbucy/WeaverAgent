//! Admitted activation settles actual filing and summary duties before transfer.
use super::*;
use crate::flow::templates::{CompiledInventory, TemplateCertificate};

fn ready_effect(group: u8) -> (SummaryFixture, TemplateCertificate) {
    static SPECS: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    let inventory = CompiledInventory::new(SPECS.get_or_init(|| {
        vec![s::TemplateSpec {
            template_id: "plan.activate".into(),
            kind: "plan.event".into(),
            parts: vec![s::TemplatePart::Literal("Read plan.".into())],
        }]
    }))
    .unwrap();
    let fx = super::super::super::root_update_tests::unbooted_with_inventory(inventory);
    let (mut fx, marks, certificate) = activation_fixture_from(fx, group, |_| {});
    begin_activation(&mut fx);
    release_result(&mut fx, &marks);
    let prepared = (0..128).any(|_| advance(&mut fx).unwrap() == PlanPortProgress::EffectReady);
    assert!(prepared);
    (fx, certificate)
}

fn start_effect(fx: &mut SummaryFixture, certificate: &TemplateCertificate) {
    let run = fx.run();
    run.context
        .begin_activation_effect(certificate, &mut run.clock)
        .unwrap();
}

fn committed(fx: &mut SummaryFixture, cancel_after_source: bool) -> s::SourcePosition {
    let bound = fx.run().context.plan_port_bound();
    let mut cancelled = false;
    for _ in 0..512 {
        advance(fx).unwrap();
        if cancel_after_source && !cancelled && !fx.records(Kind::PlanEvent).is_empty() {
            // The original source is already Recorded. Its classifier and
            // filed receipt have not been supplied, so cancellation must drain.
            assert!(fx.files.is_empty());
            fx.run().context.cancel_plan_operation();
            cancelled = true;
        }
        assert!(take(fx).is_none(), "no result before the mandatory summary");
        assert_eq!(fx.run().context.plan_port_bound(), bound);
        let context = &mut fx.run().context;
        if context.filing.is_none()
            && let Some(required) = context.plan_cache.required.as_ref()
        {
            assert_eq!(cancelled, cancel_after_source);
            return required.source.clone();
        }
        fx.classifier(false);
        fx.file_reply();
    }
    panic!("admitted activation did not retain its committed receipt");
}

fn updated_snapshot(fx: &mut SummaryFixture, source: s::SourcePosition) -> s::PlanSnapshot {
    let context = &fx.run().context;
    let s::QueryResult::PlanCurrent {
        current: Some(snapshot),
        ..
    } = context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .result()
    else {
        panic!()
    };
    let mut snapshot = snapshot.clone();
    let s::PlanEffect::Activate {
        step_id,
        step_span,
        group_span,
        ..
    } = context.test_activation_effect().unwrap()
    else {
        panic!()
    };
    let step = snapshot
        .steps
        .iter_mut()
        .find(|step| &step.step_id == step_id)
        .unwrap();
    step.status = s::StepStatus::Active;
    step.span = Some(step_span.clone());
    if let Some(group) = &step.group {
        snapshot
            .groups
            .iter_mut()
            .find(|g| &g.id == group)
            .unwrap()
            .span = group_span.clone();
    }
    snapshot.last_source = source;
    snapshot
}

fn release_summary(fx: &mut SummaryFixture, snapshot: s::PlanSnapshot) {
    let read = next(fx);
    assert!(matches!(
        &read,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::PlanCurrent {}
            },
            ..
        }
    ));
    let bytes = summary_bytes(Some(snapshot));
    fx.boot
        .member
        .respond(&read, page(91, 0, bytes.len(), &bytes));
    let release = next(fx);
    assert!(matches!(&release, s::StateFrame::Control {
        frame: s::StateControl::Release { snapshot_id }, ..
    } if snapshot_id.0 == 91));
    assert!(
        take(fx).is_none(),
        "unreleased summary grants no completion"
    );
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 91.into(),
        },
    );
}

#[test]
fn plan_activation_committed_effect_adopts_native_scopes_even_when_cancelled() {
    for (group, cancel_at) in (0..3).flat_map(|group| {
        ["none", "source", "receipt", "summary"]
            .into_iter()
            .map(move |at| (group, at))
    }) {
        let (mut fx, certificate) = ready_effect(group);
        let bound = fx.run().context.plan_port_bound();
        start_effect(&mut fx, &certificate);
        let source = committed(&mut fx, cancel_at == "source");
        assert_eq!(fx.files.len(), 1);
        assert!(!fx.run().context.native_place_cache_ready());
        let snapshot = updated_snapshot(&mut fx, source.clone());
        if cancel_at == "receipt" {
            fx.run().context.cancel_plan_operation();
        }
        release_summary(&mut fx, snapshot.clone());
        if cancel_at == "summary" {
            fx.run().context.cancel_plan_operation();
        }
        let result = finish(&mut fx);
        if cancel_at == "none" {
            assert!(matches!(result, PlanPortCompletion::Activated(_)));
        } else {
            assert!(matches!(result, PlanPortCompletion::Cancelled(_)));
        }
        assert!(
            fx.run().context.native_place_cache_ready(),
            "committed scopes must be adopted even when the caller grant is cancelled: {group}/{cancel_at}"
        );
        assert_eq!(
            fx.run()
                .context
                .committed_plan_summary()
                .unwrap()
                .unwrap()
                .last_source,
            source
        );
        assert!(fx.run().context.plan_cache.required.is_none());
        assert!(fx.run().context.plan_port_work().is_none());
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::Decision),
            bound
        );
        assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
        assert!(fx.records(Kind::Refusal).is_empty());
        assert!(fx.records(Kind::ModelRequest).is_empty());
    }
}

#[test]
fn plan_activation_classifier_rejection_records_fault_and_refusal_after_cancellation() {
    for cancelled in [false, true] {
        let (mut fx, certificate) = ready_effect(0);
        let before = fx.run().context.committed_plan_summary().unwrap().cloned();
        start_effect(&mut fx, &certificate);
        let mut cancelled_once = false;
        let result = (0..512)
            .find_map(|_| {
                advance(&mut fx).unwrap();
                if cancelled && !cancelled_once && !fx.records(Kind::PlanEvent).is_empty() {
                    fx.run().context.cancel_plan_operation();
                    cancelled_once = true;
                }
                fx.classifier(true);
                fx.file_reply();
                take(&mut fx)
            })
            .expect("rejected effect must settle its fault and refusal");
        assert_eq!(cancelled_once, cancelled);
        if cancelled {
            assert!(matches!(result, PlanPortCompletion::Cancelled(_)));
        } else {
            assert!(matches!(result, PlanPortCompletion::Refused { refusal, .. }
                if refusal.case == s::PlanOperationCase::TemplateRejected && refusal.read_code.is_none()));
        }
        assert!(
            fx.files.is_empty(),
            "correctly rejected source files no plan mutation"
        );
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            before.as_ref()
        );
        let events = fx.records(Kind::PlanEvent);
        let faults = fx.records(Kind::Fault);
        let refusals = fx.records(Kind::Refusal);
        assert_eq!((events.len(), faults.len(), refusals.len()), (1, 1, 1));
        assert_eq!(faults[0]["causal_parent"], events[0]["sequence"]);
        assert_eq!(refusals[0]["causal_parent"], faults[0]["sequence"]);
        assert_eq!(
            refusals[0]["payload"]["refusal"]["case"],
            "template_rejected"
        );
        assert!(fx.run().context.failure().is_none());
    }
}

#[test]
fn plan_activation_post_commit_summary_refusal_latches_and_keeps_receipt() {
    for cancelled in [false, true] {
        let (mut fx, certificate) = ready_effect(0);
        start_effect(&mut fx, &certificate);
        let source = committed(&mut fx, false);
        if cancelled {
            fx.run().context.cancel_plan_operation();
        }
        let read = next(&mut fx);
        fx.boot
            .member
            .respond(&read, refuse(s::ReadRefusalCode::ResultUnavailable));
        let failure = (0..128)
            .find_map(|_| advance(&mut fx).err())
            .expect("post-commit summary refusal must latch");
        assert!(format!("{failure:?}").contains("required post-effect summary read refused"));
        assert_eq!(fx.run().context.failure(), Some(&failure));
        assert!(fx.run().context.committed_plan_summary().is_err());
        assert_eq!(
            fx.run()
                .context
                .plan_cache
                .required
                .as_ref()
                .unwrap()
                .source,
            source
        );
        let run = fx.run();
        assert!(run.context.take_plan_operation(&mut run.clock).is_err());
        assert!(run.context.plan_port_work().is_some());
        assert!(
            fx.records(Kind::Refusal).is_empty(),
            "committed failure is no operation refusal"
        );
        assert_eq!(fx.files.len(), 1);
        fx.boot.member.no_request();
    }
}

#[test]
fn plan_activation_post_commit_changed_step_latches_before_native_adoption() {
    for cancelled in [false, true] {
        let (mut fx, certificate) = ready_effect(0);
        start_effect(&mut fx, &certificate);
        let source = committed(&mut fx, false);
        let mut snapshot = updated_snapshot(&mut fx, source);
        snapshot.steps[0].status = s::StepStatus::Waiting;
        snapshot.steps[0].span = None;
        if cancelled {
            fx.run().context.cancel_plan_operation();
        }
        release_summary(&mut fx, snapshot);
        let failure = (0..128)
            .find_map(|_| advance(&mut fx).err())
            .expect("a valid summary with no committed active step must fail");
        assert!(format!("{failure:?}").contains("activation refresh changed its committed step"));
        assert_eq!(fx.run().context.failure(), Some(&failure));
        assert!(fx.run().context.plan_port_work().is_some());
        let run = fx.run();
        assert!(run.context.take_plan_operation(&mut run.clock).is_err());
        assert!(fx.records(Kind::Refusal).is_empty());
        assert_eq!(fx.files.len(), 1);
        fx.boot.member.no_request();
    }
}
