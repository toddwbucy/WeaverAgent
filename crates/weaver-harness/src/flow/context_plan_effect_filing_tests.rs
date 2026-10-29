use super::*;
#[path = "context_plan_settlement_tests.rs"]
mod settlement;
use crate::flow::templates::{CompiledInventory, TemplateCertificate};

fn inventory() -> CompiledInventory {
    static VALUES: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    CompiledInventory::new(VALUES.get_or_init(|| {
        ["plan.authorize", "plan.lock", "plan.replace"]
            .into_iter()
            .map(|id| s::TemplateSpec {
                template_id: id.into(),
                kind: "plan.event".into(),
                parts: vec![s::TemplatePart::Literal("Read plan.".into())],
            })
            .collect()
    }))
    .unwrap()
}
fn start_effect(fx: &mut SummaryFixture, certificate: &TemplateCertificate) -> RootPlanProgress {
    let run = fx.run();
    run.context
        .begin_root_plan_effect(certificate, &mut run.clock)
        .unwrap()
}
fn finish_effect(fx: &mut SummaryFixture, reject: bool) -> RootPlanProgress {
    for _ in 0..512 {
        let progress = advance(fx);
        if matches!(
            progress,
            RootPlanProgress::EffectFiled
                | RootPlanProgress::RefusalReady
                | RootPlanProgress::Cancelled
        ) {
            return progress;
        }
        fx.classifier(reject);
        fx.file_reply();
    }
    panic!("root effect did not settle its exact filing");
}
fn prepare(fx: &mut SummaryFixture, operation: &str) {
    let current = (operation == "replace").then(|| previous(fx));
    let raw = if let Some(current) = &current {
        replacement(current, false)
    } else {
        let mut value: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
        if operation == "authorize" {
            let mut body = value["operation"]["create"].take();
            body.as_object_mut().unwrap().remove("steps");
            value["operation"] = serde_json::json!({"authorize":body});
        }
        value.to_string()
    };
    supplied(
        fx,
        &raw,
        catalog(current.clone(), 40, 50),
        marks(current.as_ref(), false, 20),
    );
    assert_eq!(advance(fx), RootPlanProgress::EffectReady);
}

fn updated_snapshot(fx: &mut SummaryFixture) -> s::PlanSnapshot {
    let mut snapshot = fx.snapshot();
    let work = fx.run().context.root_plan_work().unwrap();
    let s::PlanEffect::Lock {
        plan,
        task,
        steps,
        review,
        source: s::PlanLockSource::Operator { input },
    } = work.effect().unwrap().effect()
    else {
        panic!("create effect required")
    };
    snapshot.plan = plan.clone();
    snapshot.status = s::PlanStatus::Locked;
    snapshot.task = task.clone();
    snapshot.review = review.clone();
    snapshot.operator_source = input.clone();
    snapshot.last_source = committed_position(work.effect_filing().unwrap());
    snapshot.lock_source = Some(snapshot.last_source.block_id.clone());
    snapshot.categories = vec![s::Category {
        id: "review".into(),
        verbs: vec!["read".into()],
        acts: vec![s::PlanAct::Statement],
        review_required: false,
    }];
    snapshot.steps = steps
        .iter()
        .enumerate()
        .map(|(index, step)| s::PlanStep {
            step_id: step.step_id.clone(),
            ordinal: ((index + 1) as u64).into(),
            instruction: step.instruction.clone(),
            category: "review".into(),
            group: None,
            status: s::StepStatus::Waiting,
            span: None,
            outcome_span: None,
            review_call: None,
            work: None,
            closure_source: None,
        })
        .collect();
    use sha2::{Digest as _, Sha256};
    let submitted: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
    snapshot.document_sha256 = s::Digest::new(format!(
        "{:x}",
        Sha256::digest(submitted.to_string().as_bytes())
    ))
    .unwrap();
    snapshot
}

fn release_summary(fx: &mut SummaryFixture, snapshot: s::PlanSnapshot) -> Vec<u8> {
    let request = next(fx);
    assert!(matches!(
        request,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::PlanCurrent {}
            },
            ..
        }
    ));
    let raw = summary_bytes(Some(snapshot));
    fx.boot
        .member
        .respond(&request, page(91, 0, raw.len(), &raw));
    let release = next(fx);
    assert!(
        matches!(release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 91)
    );
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 91.into(),
        },
    );
    raw
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_summary_retains_the_allocator_and_actual_receipt_through_recorded() {
    for cancelled in [false, true] {
        let (mut fx, _boot, certificate) = ready_with_inventory(inventory());
        prepare(&mut fx, "create");
        start_effect(&mut fx, &certificate);
        assert_eq!(finish_effect(&mut fx, false), RootPlanProgress::EffectFiled);
        let snapshot = updated_snapshot(&mut fx);
        let bound = fx.run().context.root_plan_bound().unwrap();
        assert_eq!(
            fx.run()
                .context
                .plan_cache
                .required
                .as_ref()
                .unwrap()
                .digest
                .as_ref(),
            Some(&snapshot.document_sha256)
        );
        if cancelled {
            fx.run().context.cancel_root_plan_update();
        }
        let raw = release_summary(&mut fx, snapshot.clone());
        let mut complete = false;
        for _ in 0..128 {
            let progress = advance(&mut fx);
            if progress
                == if cancelled {
                    RootPlanProgress::Cancelled
                } else {
                    RootPlanProgress::SummaryReady
                }
            {
                complete = true;
                break;
            }
        }
        assert!(complete, "root summary never reached Recorded settlement");
        let work = fx.run().context.root_plan_work().unwrap();
        let proof = work.summary().unwrap();
        assert_eq!(proof.read().raw_bytes(), raw);
        assert_eq!(proof.filing().wait_bound(), bound);
        assert_eq!(
            proof.summary().unwrap().document_sha256,
            snapshot.document_sha256
        );
        assert_eq!(proof.summary().unwrap().last_source, snapshot.last_source);
        assert!(fx.run().context.plan_cache.required.is_none());
        assert!(
            fx.run().context.committed_plan_summary().is_err(),
            "root still owes operation settlement"
        );
        assert_eq!(fx.run().context.root_plan_bound(), Some(bound));
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_summary_refusal_or_wrong_operator_digest_latches_after_commit() {
    for refused_read in [false, true] {
        let (mut fx, _boot, certificate) = ready_with_inventory(inventory());
        prepare(&mut fx, "create");
        start_effect(&mut fx, &certificate);
        assert_eq!(finish_effect(&mut fx, false), RootPlanProgress::EffectFiled);
        let sequence = fx
            .run()
            .context
            .root_plan_work()
            .unwrap()
            .effect_filing()
            .unwrap()
            .sequence();
        if refused_read {
            let request = next(&mut fx);
            fx.boot
                .member
                .respond(&request, refused(s::ReadRefusalCode::ResultUnavailable));
        } else {
            let mut snapshot = updated_snapshot(&mut fx);
            snapshot.document_sha256 = s::Digest::new("f".repeat(64)).unwrap();
            release_summary(&mut fx, snapshot);
        }
        let mut failed = false;
        for _ in 0..128 {
            let run = fx.run();
            if run
                .context
                .advance_root_plan_update(&mut ports(&mut run.clock))
                .is_err()
            {
                failed = true;
                break;
            }
        }
        assert!(failed, "post-commit summary failure must latch");
        assert!(fx.run().context.halt().is_some());
        let expected_detail = if refused_read {
            "required post-effect summary read refused"
        } else {
            "held operator document digest"
        };
        assert!(
            fx.run()
                .context
                .halt()
                .unwrap()
                .first()
                .detail
                .contains(expected_detail),
            "unexpected failure: {:?}",
            fx.run().context.halt().unwrap().first()
        );
        assert!(fx.run().context.committed_plan_summary().is_err());
        assert!(
            fx.run()
                .context
                .root_plan_work()
                .unwrap()
                .summary()
                .is_none()
        );
        assert_eq!(
            fx.run()
                .context
                .plan_cache
                .required
                .as_ref()
                .unwrap()
                .source
                .block_id
                .sequence
                .0,
            sequence.0
        );
        assert!(
            fx.run().context.previous_plan_summary().is_none(),
            "prior committed null was not replaced by a proposed plan"
        );
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_filing_keeps_certified_text_metadata_parent_and_original_bound() {
    for operation in ["create", "authorize", "replace"] {
        let (mut fx, _boot, certificate) = ready_with_inventory(inventory());
        prepare(&mut fx, operation);
        let bound = fx.run().context.root_plan_bound().unwrap();
        let expected = fx
            .run()
            .context
            .root_plan_work()
            .unwrap()
            .effect()
            .unwrap()
            .effect()
            .clone();
        assert_eq!(
            start_effect(&mut fx, &certificate),
            RootPlanProgress::Pending
        );
        assert_eq!(finish_effect(&mut fx, false), RootPlanProgress::EffectFiled);
        let file = fx.files.last().unwrap().clone();
        let work = fx.run().context.root_plan_work().unwrap();
        let receipt = work.effect_filing().unwrap();
        assert_eq!(receipt.wait_bound(), bound);
        assert_eq!(receipt.receipts().dispositions().count(), 1);
        assert_eq!(
            file.effect,
            Some(s::StatementEffect::Plan(Box::new(expected.clone())))
        );
        assert_eq!(
            s::decode_strict::<s::PlanEffect>(file.context.as_ref().unwrap().raw.get()).unwrap(),
            expected
        );
        assert_eq!(file.text, "Read plan.");
        assert_eq!(file.source.turn, None);
        assert_eq!(file.source.channel, "internal");
        assert_eq!(file.source.origin, "loop");
        assert_eq!(
            file.source.causal_parent.unwrap().0,
            work.input_filing().unwrap().sequence().0
        );
        assert_eq!(fx.records(Kind::PlanEvent).len(), 2);
        assert_eq!(
            fx.records(Kind::PlanEvent)[1]["payload"]["template_id"],
            match operation {
                "create" => "plan.lock",
                "authorize" => "plan.authorize",
                "replace" => "plan.replace",
                _ => unreachable!(),
            }
        );
        assert!(
            fx.run().context.plan_cache.required.is_some(),
            "actual commit owes a summary"
        );
        assert!(fx.run().context.committed_plan_summary().is_err());
        assert_eq!(fx.run().context.root_plan_bound(), Some(bound));
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_template_rejection_records_fault_and_keeps_prior_cache() {
    for missing in [false, true] {
        let (mut fx, _boot, certificate) = ready_with_inventory(if missing {
            template_tests::inventory()
        } else {
            inventory()
        });
        prepare(&mut fx, "create");
        let prior_files = fx.files.len();
        start_effect(&mut fx, &certificate);
        assert_eq!(finish_effect(&mut fx, true), RootPlanProgress::RefusalReady);
        assert_eq!(
            fx.files.len(),
            prior_files,
            "template rejection cannot commit an effect"
        );
        assert_eq!(
            fx.records(Kind::PlanEvent).len(),
            if missing { 1 } else { 2 }
        );
        assert_eq!(fx.records(Kind::Fault).len(), 1);
        let work = fx.run().context.root_plan_work().unwrap();
        assert_eq!(
            work.refusal().unwrap().case,
            s::PlanUpdateCase::TemplateRejected
        );
        assert_eq!(work.effect_filing().is_some(), !missing);
        assert!(fx.run().context.plan_cache.required.is_none());
        assert!(fx.run().context.halt().is_none());
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_effect_cancellation_withholds_new_effect_but_drains_admitted_receipt() {
    for admitted in [false, true] {
        let (mut fx, _boot, certificate) = ready_with_inventory(inventory());
        prepare(&mut fx, "create");
        start_effect(&mut fx, &certificate);
        if admitted {
            for _ in 0..128 {
                advance(&mut fx);
                if fx.records(Kind::PlanEvent).len() == 2 {
                    break;
                }
            }
            assert_eq!(fx.records(Kind::PlanEvent).len(), 2);
        }
        fx.run().context.cancel_root_plan_update();
        assert_eq!(
            finish_effect(&mut fx, false),
            if admitted {
                RootPlanProgress::EffectFiled
            } else {
                RootPlanProgress::Cancelled
            }
        );
        assert_eq!(
            fx.run()
                .context
                .root_plan_work()
                .unwrap()
                .effect_filing()
                .is_some(),
            admitted
        );
        assert_eq!(fx.run().context.plan_cache.required.is_some(), admitted);
        assert_eq!(
            fx.records(Kind::PlanEvent).len(),
            if admitted { 2 } else { 1 }
        );
    }
}
