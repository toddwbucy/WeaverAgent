//! Independently pin the comparisons identified by Planner review 5705372425.
use super::*;
use crate::flow::context::{input_tests::InputFixture, template_tests};

fn loaded(sh: bool) -> SummaryFixture {
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let mut scheme: serde_json::Value = serde_json::from_str(&seed.ste100_scheme).unwrap();
    let mut write = scheme["verbs"][0].clone();
    write["key"] = serde_json::json!("write");
    write["forms"] = serde_json::json!(["Write"]);
    scheme["verbs"].as_array_mut().unwrap().push(write);
    scheme["technical_names"] = serde_json::json!([{
        "text": "work", "authority": {"rule_id": "test-rule", "disposition": "technical_use"}
    }]);
    let text = serde_json::to_string(&scheme).unwrap();
    let digest = s::Digest::new(format!("{:x}", Sha256::digest(text.as_bytes()))).unwrap();
    let declaration = s::SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into(), "write".into()],
    };
    let mut templates = TemplateFixture {
        input: InputFixture::with_declaration_scheme_tools(
            Some(&template_tests::inventory()),
            crate::flow::context::tests::config(),
            Some((text, declaration)),
            sh.then(|| t::ToolPolicy {
                clock_ms: 1234,
                measurement_ref: "plan-port-tool-name-fixture".into(),
            }),
        ),
    };
    templates.begin();
    for index in 0..template_tests::templates().len() {
        templates.accept(index);
    }
    assert_eq!(templates.advance().unwrap(), TemplateProgress::Complete);
    let mut boot = BootFixture::from_templates(templates);
    let entry = boot.entry();
    let request = boot.begin();
    boot.released(&request, entry);
    assert!(matches!(boot.finish(), BootReadCompletion::Seated));
    let (decode, peer) = DecodeChannel::pair().unwrap();
    SummaryFixture::from_open(
        OpenFixture {
            boot,
            decode: DecodePort::from_channel(decode).unwrap(),
            peer,
        },
        vec![],
    )
}

fn assessment_refuses(
    mut fx: SummaryFixture,
    (request, bytes): (PlanRequest, Vec<u8>),
    case: s::PlanOperationCase,
) {
    let original = format!("{request:?}");
    begin(&mut fx, request);
    let read = next(&mut fx);
    fx.boot
        .member
        .respond(&read, page(81, 0, bytes.len(), &bytes));
    let release = next(&mut fx);
    assert!(matches!(release, s::StateFrame::Control {
        frame: s::StateControl::Release { snapshot_id }, ..
    } if snapshot_id.0 == 81));
    assert!(take(&mut fx).is_none());
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 81.into(),
        },
    );
    let result = finish(&mut fx);
    assert_eq!(format!("{:?}", result.request()), original);
    let PlanPortCompletion::Refused { refusal, .. } = result else {
        panic!("assessment must refuse this independent comparison");
    };
    assert_eq!(refusal.case, case);
    assert_eq!(refusal.read_code, None);
    let rows = fx.records(Kind::Refusal);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["payload"]["operation"], "assess_act");
    assert_eq!(
        rows[0]["payload"]["refusal"],
        serde_json::to_value(refusal).unwrap()
    );
    assert_eq!(
        rows[0]["causal_parent"],
        fx.records(Kind::Recall).last().unwrap()["sequence"]
    );
    assert!(fx.records(Kind::PlanEvent).is_empty());
    fx.boot.member.no_request();
}

#[test]
fn plan_port_assess_act_refuses_verb_outside_category_with_permitted_act() {
    let fx = ready_configured(
        loaded(false),
        true,
        false,
        s::StepStatus::Active,
        s::Category {
            id: "work".into(),
            verbs: vec!["read".into()],
            acts: vec![s::PlanAct::Statement],
            review_required: false,
        },
    );
    let (request, bytes) = statement_request(&fx, s::PlanAct::Statement);
    let mut result: s::QueryResult = serde_json::from_slice(&bytes).unwrap();
    let s::QueryResult::Entries { entries } = &mut result else {
        unreachable!()
    };
    let entry = &mut entries[0];
    entry.text = "Write file.".into();
    entry.raw_block = serde_json::value::to_raw_value(&serde_json::json!({
        "type":"text", "text":entry.text,
    }))
    .unwrap();
    let binding = &mut entry.bindings[0].value;
    binding.verb = "write".into();
    binding.statement_span.end = 11.into();
    binding.verb_span.end = 5.into();
    binding.nouns[0].span.start = 6.into();
    binding.nouns[0].span.end = 10.into();
    assessment_refuses(
        fx,
        (request, serde_json::to_vec(&result).unwrap()),
        s::PlanOperationCase::WrongCategory,
    );
}

#[test]
fn plan_port_assess_act_refuses_other_tool_with_shell_loaded_and_category_permitting() {
    let act = s::PlanAct::ExternalTool {
        name: "curl".into(),
    };
    let mut fx = ready_configured(
        loaded(true),
        true,
        false,
        s::StepStatus::Active,
        s::Category {
            id: "work".into(),
            verbs: vec!["read".into()],
            acts: vec![act.clone()],
            review_required: false,
        },
    );
    assert!(
        fx.run()
            .context
            .load_duty
            .as_ref()
            .unwrap()
            .tools
            .sh
            .is_some(),
        "actual load grants sh"
    );
    let statement = statement_request(&fx, act);
    assessment_refuses(fx, statement, s::PlanOperationCase::UnknownTool);
}

#[test]
fn plan_port_work_reference_refuses_other_open_span_without_reading() {
    let mut fx = ready(true);
    // The real boot-head proof selected this open identity-prefix span. It is
    // local and valid, but it is not the active step's span (5).
    let prefix = fx.boot.entry().selected_span.unwrap().span_ref;
    let parent = fx
        .run()
        .context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .recall_sequence();
    let recalls = fx.records(Kind::Recall).len();
    let mut request = request(fx.seed());
    let PlanRequest::WorkReference { span, .. } = &mut request else {
        unreachable!()
    };
    assert_ne!(*span, prefix);
    *span = prefix;
    begin(&mut fx, request);
    let result = (0..128)
        .find_map(|_| {
            advance(&mut fx).unwrap();
            assert!(
                fx.run().context.flow.state().outstanding().is_empty(),
                "another span must refuse before any read is admitted"
            );
            take(&mut fx)
        })
        .expect("local refusal must record");
    let PlanPortCompletion::Refused { refusal, .. } = result else {
        panic!("another open span cannot grant this step's work");
    };
    assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
    assert_eq!(refusal.read_code, None);
    let rows = fx.records(Kind::Refusal);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]["payload"]["refusal"],
        serde_json::to_value(refusal).unwrap()
    );
    assert_eq!(rows[0]["causal_parent"], serde_json::json!(parent));
    assert_eq!(fx.records(Kind::Recall).len(), recalls);
    fx.boot.member.no_request();
}
