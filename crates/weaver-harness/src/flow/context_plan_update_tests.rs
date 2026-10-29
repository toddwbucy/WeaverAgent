//! Real root packet and common read/recording owners after the boot-summary fixture.
//! The member is scripted; this does not claim the full admin/SQLite update circuit.
use super::*;
#[path = "context_plan_effect_tests.rs"]
mod effects;
use crate::flow::context::plan_update_driver::{RootPlanInput, RootPlanProgress};
use crate::flow::pump::plan::tests::{document, packet};

fn ready() -> (SummaryFixture, BootPlanSummary) {
    let (fx, boot, _) = ready_with_inventory(template_tests::inventory());
    (fx, boot)
}
fn ready_with_inventory(
    inventory: crate::flow::templates::CompiledInventory,
) -> (
    SummaryFixture,
    BootPlanSummary,
    crate::flow::templates::TemplateCertificate,
) {
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let mut scheme: serde_json::Value = serde_json::from_str(&seed.ste100_scheme).unwrap();
    scheme["technical_names"] = serde_json::json!([
        {"text":"review","authority":{"rule_id":"test-rule","disposition":"technical_use"}},
        {"text":"second","authority":{"rule_id":"test-rule","disposition":"technical_use"}}
    ]);
    let text = serde_json::to_string(&scheme).unwrap();
    use sha2::{Digest as _, Sha256};
    let digest = s::Digest::new(format!("{:x}", Sha256::digest(text.as_bytes()))).unwrap();
    let declared = s::SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into()],
    };
    let mut templates = TemplateFixture {
        input: input_tests::InputFixture::with_template_inventory_and_scheme(
            &inventory, text, declared,
        ),
    };
    templates
        .input
        .run
        .context
        .begin_template_checks(inventory.clone(), &mut templates.input.run.clock)
        .unwrap();
    for spec in inventory.templates() {
        let mut answered = false;
        for _ in 0..128 {
            templates.advance().unwrap();
            templates.input.classifier_transport();
            let Receive::Complete(message) = templates.input.peer.try_receive().unwrap() else {
                continue;
            };
            let request: s::Ste100Request =
                s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
            assert_eq!(
                request.target,
                s::Ste100Target::Template {
                    template_id: spec.template_id.clone()
                }
            );
            assert_eq!(request.text, serde_json::to_string(spec).unwrap());
            templates.input.reply(&s::Ste100Reply {
                request_id: request.request_id,
                target: request.target,
                scheme_id: request.scheme_id,
                result: s::Ste100Result::Accepted { bindings: vec![] },
            });
            answered = true;
            break;
        }
        assert!(answered, "template check never crossed the socket");
    }
    for _ in 0..128 {
        if templates.advance().unwrap() == TemplateProgress::Complete {
            break;
        }
    }
    assert_eq!(templates.advance().unwrap(), TemplateProgress::Complete);
    let mut boot = BootFixture::from_templates(templates);
    let entry = boot.entry();
    let request = boot.begin();
    boot.released(&request, entry);
    assert!(matches!(boot.finish(), BootReadCompletion::Seated));
    let (decode, peer) = DecodeChannel::pair().unwrap();
    let open = OpenFixture {
        boot,
        decode: DecodePort::from_channel(decode).unwrap(),
        peer,
    };
    let mut fx = SummaryFixture::from_open(open, vec![]);
    let boot = fx.boot(None);
    let run = fx.run();
    let certificate = run
        .context
        .take_template_certificate(&mut run.clock)
        .unwrap()
        .unwrap();
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Enter)
            .is_some()
    );
    (fx, boot, certificate)
}
fn begin(fx: &mut SummaryFixture) {
    let run = fx.run();
    let result = run
        .context
        .begin_root_plan_update(packet(&document()), &mut run.clock);
    if let Err(error) = result {
        panic!("root begin: {:?} {:?}", error.refusal, error.failure)
    }
}
fn advance(fx: &mut SummaryFixture) -> RootPlanProgress {
    let run = fx.run();
    run.context
        .advance_root_plan_update(&mut ports(&mut run.clock))
        .unwrap()
}
fn next(fx: &mut SummaryFixture) -> s::StateFrame {
    for _ in 0..128 {
        advance(fx);
        if !fx.run().context.flow.state().outstanding().is_empty() {
            return fx.boot.member.sent();
        }
    }
    panic!("root preflight did not issue its next request")
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_preflight_owns_allocator_and_keeps_original_decision_through_both_reads() {
    let (mut fx, _boot) = ready();
    begin(&mut fx);
    let bound = fx.run().context.root_plan_bound().unwrap();
    assert!(fx.run().context.committed_plan_summary().is_err());
    let prior = fx.records(Kind::PlanEvent).len();
    let current = next(&mut fx);
    assert_eq!(
        fx.run()
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap()
            .deadline_ns(),
        bound.deadline_ns()
    );
    assert!(matches!(
        &current,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::PlanCurrent {}
            },
            ..
        }
    ));
    let run = fx.run();
    assert!(run.context.complete_wait(PumpDeadline::Decision).is_none());
    assert!(run.context.take_task(&mut run.clock).unwrap().is_none());
    let error = run
        .context
        .begin_read(s::ReadSelector::Marks {}, None, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, ReadBeginRefusal::Busy);
    let error = run
        .context
        .begin_marks_read(TurnRef("foreign-task".into()), &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, ReadBeginRefusal::Busy);
    run.context.cancel_task();
    let second = run
        .context
        .begin_root_plan_update(packet(&document()), &mut run.clock)
        .err()
        .unwrap();
    assert!(second.original.is_some());
    assert_eq!(second.refusal, weaver_types::LifecycleRefusal::OutOfOrder);
    assert!(second.failure.is_none());
    let bytes = summary_bytes(None);
    fx.boot
        .member
        .respond(&current, page(51, 0, bytes.len(), &bytes));
    let release = next(&mut fx);
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 51.into(),
        },
    );
    let marks = next(&mut fx);
    assert_eq!(
        fx.run()
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap()
            .deadline_ns(),
        bound.deadline_ns()
    );
    assert!(matches!(
        &marks,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::Marks {}
            },
            ..
        }
    ));
    let bytes = br#"{"marks":{"next_span_id":"1","current_task":null,"open_scopes":[]}}"#;
    fx.boot
        .member
        .respond(&marks, page(52, 0, bytes.len(), bytes));
    let release = next(&mut fx);
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 52.into(),
        },
    );
    let done =
        (0..128).find_map(|_| (advance(&mut fx) == RootPlanProgress::InputReady).then_some(()));
    assert!(done.is_some());
    assert_eq!(fx.run().context.root_plan_bound(), Some(bound));
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert_eq!(fx.records(Kind::PlanEvent).len(), prior);
    assert!(matches!(
        fx.run().context.root_plan_work().unwrap().input(),
        RootPlanInput::Accepted(_)
    ));
    fx.boot.member.no_request();
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_preflight_refusal_retains_measurement_parent_without_releasing_allocator_early() {
    let (mut fx, _boot) = ready();
    begin(&mut fx);
    let bound = fx.run().context.root_plan_bound().unwrap();
    let request = next(&mut fx);
    fx.boot
        .member
        .respond(&request, refused(s::ReadRefusalCode::CounterExhausted));
    assert!((0..128).any(|_| advance(&mut fx) == RootPlanProgress::RefusalReady));
    let parent = fx.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"].clone();
    let work = fx.run().context.root_plan_work().unwrap();
    assert_eq!(
        work.refusal().unwrap().read_code,
        Some(s::ReadRefusalCode::CounterExhausted)
    );
    assert_eq!(
        serde_json::json!(work.read_refusal().unwrap().measurement_sequence()),
        parent
    );
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert_eq!(fx.run().context.root_plan_bound(), Some(bound));
    assert!(fx.run().context.halt().is_none());
    fx.boot.member.no_request();
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_preflight_cancel_before_read_keeps_original_input_without_a_new_request() {
    let (mut fx, _boot) = ready();
    begin(&mut fx);
    fx.run().context.cancel_root_plan_update();
    assert_eq!(advance(&mut fx), RootPlanProgress::Cancelled);
    assert!(matches!(
        fx.run().context.root_plan_work().unwrap().input(),
        RootPlanInput::Accepted(_)
    ));
    fx.boot.member.no_request();
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_preflight_original_expiry_keeps_input_and_cannot_become_an_operation_refusal() {
    let (mut fx, _boot) = ready();
    begin(&mut fx);
    let bound = fx.run().context.root_plan_bound().unwrap();
    let _request = next(&mut fx);
    let run = fx.run();
    run.clock.advance_to(bound.deadline_ns()).unwrap();
    let failure = run
        .context
        .advance_root_plan_update(&mut ports(&mut run.clock))
        .unwrap_err();
    assert!(
        matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            }
        ),
        "{failure:?}"
    );
    assert!(matches!(
        run.context.root_plan_work().unwrap().input(),
        RootPlanInput::Accepted(_)
    ));
    assert!(run.context.root_plan_work().unwrap().refusal().is_none());
    assert_eq!(run.context.root_plan_bound(), Some(bound));
}

fn filing_document() -> String {
    let mut value: serde_json::Value = serde_json::from_str(&document()).unwrap();
    let create = &mut value["operation"]["create"];
    create["task"] = "Read plan.".into();
    create["categories"] = serde_json::json!([{"id":"review","verbs":["read"],"acts":["statement"],"review_required":false}]);
    for step in create["steps"].as_array_mut().unwrap() {
        step["instruction"] = "Read plan.".into();
    }
    format!("\n {} \n", serde_json::to_string_pretty(&value).unwrap())
}
fn read_to_input(fx: &mut SummaryFixture, raw: &str) {
    assert_eq!(read_catalogs(fx, raw, None), RootPlanProgress::InputReady);
}
fn read_catalogs(
    fx: &mut SummaryFixture,
    raw: &str,
    current: Option<s::PlanSnapshot>,
) -> RootPlanProgress {
    read_catalog_values(
        fx,
        raw,
        summary_bytes(current),
        br#"{"marks":{"next_span_id":"1","current_task":null,"open_scopes":[]}}"#.to_vec(),
    )
}
fn read_catalog_values(
    fx: &mut SummaryFixture,
    raw: &str,
    current: Vec<u8>,
    marks: Vec<u8>,
) -> RootPlanProgress {
    let run = fx.run();
    if let Err(error) = run
        .context
        .begin_root_plan_update(packet(raw), &mut run.clock)
    {
        panic!("root admission: {:?} {:?}", error.refusal, error.failure);
    }
    for (snapshot, bytes) in [(61, current), (62, marks)] {
        let request = next(fx);
        fx.boot
            .member
            .respond(&request, page(snapshot, 0, bytes.len(), &bytes));
        let release = next(fx);
        fx.boot.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: snapshot.into(),
            },
        );
    }
    for _ in 0..128 {
        let progress = advance(fx);
        if progress != RootPlanProgress::Pending {
            return progress;
        }
    }
    panic!("catalogs did not settle");
}
fn file_input(fx: &mut SummaryFixture, reject: bool) -> RootPlanProgress {
    for _ in 0..512 {
        let progress = advance(fx);
        if matches!(
            progress,
            RootPlanProgress::InputFiled
                | RootPlanProgress::RefusalReady
                | RootPlanProgress::Cancelled
        ) {
            return progress;
        }
        fx.classifier(reject);
        fx.file_reply();
    }
    panic!("operator input filing did not finish");
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_input_files_exact_document_and_every_original_block_before_preflight() {
    let (mut fx, _boot) = ready();
    let raw = filing_document();
    read_to_input(&mut fx, &raw);
    let bound = fx.run().context.root_plan_bound().unwrap();
    let (mut other, _other_boot) = ready();
    let RootPlanInput::Accepted(input) = fx.run().context.root_plan_work().unwrap().input() else {
        panic!("accepted input");
    };
    assert!(input.event(&other.run().context).is_err());
    assert_eq!(file_input(&mut fx, false), RootPlanProgress::InputFiled);
    let records = fx.records(Kind::PlanEvent);
    assert_eq!(records.len(), 1, "input only; no effect yet");
    assert_eq!(records[0]["payload"]["document"], raw);
    assert_eq!(records[0]["channel"], "input");
    assert_eq!(records[0]["origin"], "operator");
    assert_eq!(records[0]["turn"], serde_json::Value::Null);
    assert_eq!(fx.files.len(), 3);
    for (index, file) in fx.files.iter().enumerate() {
        assert_eq!(file.text, "Read plan.");
        assert_eq!(file.block_id.block.0, index as u64);
        assert_eq!(file.source.turn, None);
        assert!(file.effect.is_none());
        assert_eq!(file.context.is_some(), index == 0);
    }
    let run = fx.run();
    let work = run.context.root_plan_work().unwrap();
    let proof = work.input_filing().unwrap();
    assert!(proof.belongs_to(&run.context));
    assert_eq!(proof.receipts().dispositions().count(), 3);
    let RootPlanInput::Accepted(input) = work.input() else {
        unreachable!()
    };
    assert!(
        input
            .check_filing(
                &other.run().context,
                proof,
                run.context.flow.run(),
                &run.context.declaration.scheme.scheme_id,
                bound
            )
            .is_err()
    );
    assert!(
        input
            .check_filing(
                &run.context,
                proof,
                "other-run",
                &run.context.declaration.scheme.scheme_id,
                bound
            )
            .is_err()
    );
    assert!(
        input
            .check_filing(
                &run.context,
                proof,
                run.context.flow.run(),
                "other-scheme",
                bound
            )
            .is_err()
    );
    let wrong_bound =
        AbsoluteBound::after(bound.began_ns(), bound.deadline_ns() - bound.began_ns() + 1).unwrap();
    assert!(
        input
            .check_filing(
                &run.context,
                proof,
                run.context.flow.run(),
                &run.context.declaration.scheme.scheme_id,
                wrong_bound
            )
            .is_err()
    );
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(run.context.complete_wait(PumpDeadline::Decision).is_none());
    assert!(run.context.committed_plan_summary().is_err());
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_input_rejection_waits_for_all_blocks_and_keeps_ordered_dispositions() {
    let (mut fx, _boot) = ready();
    read_to_input(&mut fx, &filing_document());
    let bound = fx.run().context.root_plan_bound().unwrap();
    assert_eq!(file_input(&mut fx, true), RootPlanProgress::RefusalReady);
    assert!(
        fx.files.is_empty(),
        "correct rejection cannot be filed as accepted content"
    );
    assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
    let work = fx.run().context.root_plan_work().unwrap();
    assert_eq!(work.refusal().unwrap().case, s::PlanUpdateCase::NonSte100);
    assert_eq!(
        work.refusal().unwrap().rejected_blocks,
        vec![0.into(), 1.into(), 2.into()]
    );
    assert_eq!(
        work.input_filing()
            .unwrap()
            .receipts()
            .dispositions()
            .count(),
        3
    );
    assert_eq!(fx.run().context.root_plan_bound(), Some(bound));
    assert!(fx.run().context.halt().is_none());
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_input_cancellation_drains_admitted_filing_and_retains_all_receipts() {
    let (mut fx, _boot) = ready();
    read_to_input(&mut fx, &filing_document());
    assert_eq!(advance(&mut fx), RootPlanProgress::Pending);
    assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
    assert!(fx.files.is_empty());
    fx.run().context.cancel_root_plan_update();
    // Foreign callers cannot drive or take this private subscription.
    let run = fx.run();
    assert!(matches!(
        run.context.take_filing(&mut run.clock),
        Err(RunFailure::PairOwned)
    ));
    assert!(matches!(
        run.context
            .take_owned_filing(receipt_driver::FilingOwner::RootPlan, &mut run.clock),
        Err(RunFailure::MarksOwned)
    ));
    assert_eq!(file_input(&mut fx, false), RootPlanProgress::Cancelled);
    assert_eq!(fx.files.len(), 3);
    assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
    let work = fx.run().context.root_plan_work().unwrap();
    assert_eq!(
        work.input_filing()
            .unwrap()
            .receipts()
            .dispositions()
            .count(),
        3
    );
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert!(fx.run().context.halt().is_none());
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_catalog_checks_loaded_names_disjoint_verbs_and_ordered_groups() {
    use s::PlanUpdateCase as C;
    let valid: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
    let cases = [
        (
            "/operation/create/review/setup_id",
            serde_json::json!("absent"),
            C::MalformedPlan,
        ),
        (
            "/operation/create/categories/0/id",
            serde_json::json!("absent"),
            C::InvalidCategory,
        ),
        (
            "/operation/create/categories/0/verbs",
            serde_json::json!([]),
            C::InvalidCategory,
        ),
        (
            "/operation/create/categories/0/verbs",
            serde_json::json!(["read", "read"]),
            C::InvalidCategory,
        ),
        (
            "/operation/create/categories/0/verbs",
            serde_json::json!(["invented"]),
            C::InvalidCategory,
        ),
        (
            "/operation/create/categories",
            serde_json::json!([
                {"id":"review","verbs":["read"],"acts":[],"review_required":false},
                {"id":"second","verbs":["read"],"acts":[],"review_required":false}
            ]),
            C::InvalidCategory,
        ),
        (
            "/operation/create/groups",
            serde_json::json!([{"id":"g","step_indices":[]}]),
            C::InvalidGroup,
        ),
        (
            "/operation/create/groups",
            serde_json::json!([{"id":"g","step_indices":["1","0"]}]),
            C::InvalidGroup,
        ),
        (
            "/operation/create/groups",
            serde_json::json!([{"id":"g","step_indices":["2"]}]),
            C::InvalidGroup,
        ),
        (
            "/operation/create/groups",
            serde_json::json!([
                {"id":"g","step_indices":["0"]}, {"id":"h","step_indices":["0"]}
            ]),
            C::InvalidGroup,
        ),
        (
            "/operation/create/groups",
            serde_json::json!([
                {"id":"g","step_indices":["0"]}, {"id":"g","step_indices":["1"]}
            ]),
            C::InvalidGroup,
        ),
    ];
    for (path, value, case) in cases {
        let (mut fx, _boot) = ready();
        let mut document = valid.clone();
        *document.pointer_mut(path).unwrap() = value;
        assert_eq!(
            read_catalogs(&mut fx, &document.to_string(), None),
            RootPlanProgress::RefusalReady,
            "{path}"
        );
        assert_eq!(
            fx.run()
                .context
                .root_plan_work()
                .unwrap()
                .refusal()
                .unwrap()
                .case,
            case,
            "{path}"
        );
        assert!(fx.records(Kind::PlanEvent).is_empty());
        assert!(fx.run().context.halt().is_none());
    }
    for authorize in [false, true] {
        let (mut fx, _boot) = ready();
        let mut document = valid.clone();
        if authorize {
            let mut body = document["operation"]["create"].take();
            body.as_object_mut().unwrap().remove("steps");
            body["groups"] = serde_json::json!([{"id":"g","step_indices":["98","99"]}]);
            document["operation"] = serde_json::json!({"authorize":body});
        } else {
            document["operation"]["create"]["groups"] =
                serde_json::json!([{"id":"g","step_indices":["0","1"]}]);
        }
        assert_eq!(
            read_catalogs(&mut fx, &document.to_string(), None),
            RootPlanProgress::InputReady
        );
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_catalog_preserves_current_revision_seed_and_retired_identity_guards() {
    use s::PlanUpdateCase as C;
    for (case, expected) in [
        ("new", C::PlanExists),
        ("stale", C::StalePlan),
        ("foreign_plan", C::WrongSeed),
        ("revision_overflow", C::CounterExhausted),
        ("answer_unbroken", C::StalePlan),
        ("foreign_step", C::WrongSeed),
        ("unknown", C::MalformedPlan),
        ("duplicate", C::MalformedPlan),
        ("retired", C::MalformedPlan),
        ("closure_not_closed", C::ClosureChanged),
        ("closure_foreign", C::WrongSeed),
    ] {
        let (mut fx, _boot) = ready();
        let mut current = fx.snapshot();
        current.status = s::PlanStatus::Locked;
        current.lock_source = Some(current.operator_source.clone());
        let id = s::StepId {
            seed_id: fx.seed().into(),
            local_id: 7.into(),
        };
        current.steps.push(s::PlanStep {
            step_id: id.clone(),
            ordinal: 1.into(),
            instruction: s::StatementRef {
                block_id: current.operator_source.clone(),
                binding: 0.into(),
            },
            category: "review".into(),
            group: None,
            status: s::StepStatus::Waiting,
            span: None,
            outcome_span: None,
            review_call: None,
            work: None,
            closure_source: None,
        });
        let mut document: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
        if case != "new" {
            let mut body = document["operation"]["create"].take();
            body["task"] = serde_json::Value::Null;
            body["preserved_closures"] = serde_json::json!([]);
            document["expected"] = serde_json::to_value(&current.plan).unwrap();
            match case {
                "stale" => document["expected"]["revision"] = "2".into(),
                "foreign_plan" => {
                    document["expected"]["plan_id"]["seed_id"] = "b".repeat(32).into()
                }
                "revision_overflow" => {
                    current.plan.revision = u64::MAX.into();
                    document["expected"]["revision"] = u64::MAX.to_string().into();
                }
                "foreign_step" => {
                    body["steps"][0]["id"] =
                        serde_json::json!({"seed_id":"b".repeat(32),"local_id":"7"})
                }
                "unknown" => {
                    body["steps"][0]["id"] = serde_json::json!({"seed_id":fx.seed(),"local_id":"8"})
                }
                "duplicate" => {
                    for step in body["steps"].as_array_mut().unwrap() {
                        step["id"] = serde_json::to_value(&id).unwrap();
                    }
                }
                "retired" => {
                    current.steps[0].status = s::StepStatus::Retired;
                    body["steps"][0]["id"] = serde_json::to_value(&id).unwrap();
                }
                "closure_not_closed" => {
                    body["steps"][0]["id"] = serde_json::to_value(&id).unwrap();
                    body["preserved_closures"] = serde_json::json!([id]);
                }
                "closure_foreign" => {
                    body["preserved_closures"] =
                        serde_json::json!([{"seed_id":"b".repeat(32),"local_id":"7"}])
                }
                _ => (),
            }
            document["operation"] = if case == "answer_unbroken" {
                serde_json::json!({"answer_break":{"replacement":body}})
            } else {
                serde_json::json!({"replace":body})
            };
        }
        assert_eq!(
            read_catalogs(&mut fx, &document.to_string(), Some(current)),
            RootPlanProgress::RefusalReady,
            "{case}"
        );
        assert_eq!(
            fx.run()
                .context
                .root_plan_work()
                .unwrap()
                .refusal()
                .unwrap()
                .case,
            expected,
            "{case}"
        );
        assert!(fx.records(Kind::PlanEvent).is_empty());
        assert!(fx.run().context.halt().is_none());
    }
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_input_requires_filed_instruction_to_match_an_operator_category() {
    let (mut fx, _boot) = ready();
    let mut document: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
    document["operation"]["create"]["categories"] = serde_json::json!([]);
    read_to_input(&mut fx, &document.to_string());
    assert_eq!(file_input(&mut fx, false), RootPlanProgress::RefusalReady);
    assert_eq!(fx.files.len(), 3, "accepted input remains as proposals");
    assert_eq!(
        fx.run()
            .context
            .root_plan_work()
            .unwrap()
            .refusal()
            .unwrap()
            .case,
        s::PlanUpdateCase::InvalidCategory
    );
    assert_eq!(
        fx.records(Kind::PlanEvent).len(),
        1,
        "no effect from unmatched category"
    );
    assert!(fx.run().context.halt().is_none());
}

#[test]
#[ignore = "actual root coordination peer"]
fn root_plan_input_mixed_results_keep_block_indices_and_multiple_statements_cannot_be_steps() {
    for multiple in [false, true] {
        let (mut fx, _boot) = ready();
        let mut document: serde_json::Value = serde_json::from_str(&filing_document()).unwrap();
        if multiple {
            document["operation"]["create"]["steps"][0]["instruction"] =
                "Read plan. Read plan.".into();
        }
        read_to_input(&mut fx, &document.to_string());
        let mut finished = false;
        for _ in 0..512 {
            let progress = advance(&mut fx);
            if progress == RootPlanProgress::RefusalReady {
                finished = true;
                break;
            }
            assert_eq!(progress, RootPlanProgress::Pending);
            fx.classifier_result(|request| {
                let s::Ste100Target::Source(source) = &request.target else {
                    panic!("source classification");
                };
                if !multiple && source.block.0 == 1 {
                    s::Ste100Result::NotSte100 {
                        code: s::Ste100DefectCode::NonSte100,
                        detail: "fixture reject middle".into(),
                        spans: vec![],
                    }
                } else {
                    let offsets: &[u64] = if request.text == "Read plan. Read plan." {
                        &[0, 11]
                    } else {
                        assert_eq!(request.text, "Read plan.");
                        &[0]
                    };
                    s::Ste100Result::Accepted {
                        bindings: offsets
                            .iter()
                            .map(|start| s::Binding {
                                statement_span: s::Span {
                                    start: (*start).into(),
                                    end: (start + 10).into(),
                                },
                                verb: "read".into(),
                                verb_span: s::Span {
                                    start: (*start).into(),
                                    end: (start + 4).into(),
                                },
                                condition: None,
                                condition_span: None,
                                nouns: vec![s::NounBinding {
                                    value: "plan".into(),
                                    span: s::Span {
                                        start: (start + 5).into(),
                                        end: (start + 9).into(),
                                    },
                                }],
                            })
                            .collect(),
                    }
                }
            });
            fx.file_reply();
        }
        assert!(finished);
        let work = fx.run().context.root_plan_work().unwrap();
        let refusal = work.refusal().unwrap();
        assert_eq!(
            refusal.case,
            if multiple {
                s::PlanUpdateCase::MalformedPlan
            } else {
                s::PlanUpdateCase::NonSte100
            }
        );
        assert_eq!(
            refusal.rejected_blocks,
            if multiple { vec![] } else { vec![1.into()] }
        );
        assert_eq!(
            work.input_filing()
                .unwrap()
                .receipts()
                .dispositions()
                .count(),
            3
        );
        assert_eq!(
            fx.files
                .iter()
                .map(|file| file.block_id.block.0)
                .collect::<Vec<_>>(),
            if multiple { vec![0, 1, 2] } else { vec![0, 2] }
        );
        assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
        assert!(fx.run().context.halt().is_none());
    }
}
