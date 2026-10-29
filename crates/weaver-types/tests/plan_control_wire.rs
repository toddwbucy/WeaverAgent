//! Spec 07 PLAN-EXCHANGE and TYPES-PLAN-CONTROL, through actual floor consumers.
use serde_json::{Value, json};
use weaver_types::state_flow::{PlanUpdateRefused, decode_strict};
use weaver_types::{
    LifecycleAsk, LifecycleDirective, LifecycleRefusal, OrganEnvelope, RefusalRecord,
};

fn refusal(case: &str, code: Value) -> Value {
    json!({"case":case,"detail":"exact read detail","rejected_blocks":[],"read_code":code})
}

fn accepts_refusal(fields: Value, accepted: bool) {
    let source = fields.to_string();
    assert_eq!(
        decode_strict::<PlanUpdateRefused>(&source).is_ok(),
        accepted,
        "{source}"
    );
    let mut wire = fields;
    wire["kind"] = json!("plan_update_refused");
    for _ in 0..2 {
        assert_eq!(
            serde_json::from_value::<LifecycleRefusal>(wire.clone()).is_ok(),
            accepted,
            "{wire}"
        );
        let envelope = json!({
            "exchange":{"opener":"admin","ordinal":2},"position":"close",
            "payload":{"kind":"refusal","body":wire}
        });
        assert_eq!(
            decode_strict::<OrganEnvelope>(&envelope.to_string()).is_ok(),
            accepted,
            "{envelope}"
        );
        let trace = json!({"seam":"lifecycle","asked":{"ask":"plan_update"},"refusal":wire});
        assert_eq!(
            serde_json::from_value::<RefusalRecord>(trace.clone()).is_ok(),
            accepted,
            "{trace}"
        );
        wire = json!({"kind":"organ_refused","organ":"spu","reason":wire});
    }
}

#[test]
fn plan_read_refusal_accepts_exact_code_union_at_every_consumer() {
    for code in [
        "not_found",
        "counter_exhausted",
        "unknown_snapshot",
        "wrong_offset",
        "result_too_large",
        "result_unavailable",
        "answer_too_large",
    ] {
        accepts_refusal(refusal("read_refused", json!(code)), true);
    }
    for code in [
        "unsupported_operation",
        "invalid_selector",
        "wrong_seed",
        "unknown_span",
        "span_open",
        "span_pending",
        "partial_pair",
        "read_busy",
        "invented",
    ] {
        accepts_refusal(refusal("read_refused", json!(code)), false);
    }
    accepts_refusal(refusal("read_refused", Value::Null), false);
    accepts_refusal(refusal("read_refused", json!({"not_found":null})), false);
    let mut blocks = refusal("read_refused", json!("not_found"));
    blocks["rejected_blocks"] = json!(["0"]);
    accepts_refusal(blocks, false);
}

#[test]
fn plan_operation_refusals_require_explicit_null_and_only_non_ste100_carries_blocks() {
    for case in [
        "malformed_plan",
        "stale_plan",
        "non_ste100",
        "invalid_category",
        "invalid_group",
        "closure_changed",
        "wrong_seed",
        "plan_exists",
        "counter_exhausted",
        "template_rejected",
    ] {
        let fields = refusal(case, Value::Null);
        accepts_refusal(fields.clone(), true);
        let typed: PlanUpdateRefused = serde_json::from_value(fields.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), fields);
        accepts_refusal(refusal(case, json!("not_found")), false);
        let mut blocks = fields;
        blocks["rejected_blocks"] = json!(["0", "2"]);
        accepts_refusal(blocks, case == "non_ste100");
    }
}

#[test]
fn plan_refusal_rejects_missing_unknown_wrong_shape_and_duplicate_fields() {
    let fields = refusal("read_refused", json!("result_unavailable"));
    for name in ["case", "detail", "rejected_blocks", "read_code"] {
        let mut missing = fields.clone();
        missing.as_object_mut().unwrap().remove(name);
        accepts_refusal(missing, false);
    }
    let mut unknown = fields.clone();
    unknown["extra"] = json!(true);
    accepts_refusal(unknown, false);
    let mut case = fields;
    case["case"] = json!({"read_refused":null});
    accepts_refusal(case, false);
    assert!(
        serde_json::from_value::<PlanUpdateRefused>(json!([
            "read_refused",
            "detail",
            [],
            "not_found"
        ]))
        .is_err()
    );
    let duplicate = r#"{"case":"read_refused","detail":"first","detail":"second","rejected_blocks":[],"read_code":"not_found"}"#;
    assert!(decode_strict::<PlanUpdateRefused>(duplicate).is_err());
}

#[test]
fn plan_directive_preserves_exact_document_and_closes_its_fields() {
    let document = " {\n \"quoted\": \"\\\\\", \"unicode\": \"\u{2603}\"\n }\n";
    let fields = json!({"kind":"plan_update", "cause":{"uid":0}, "document_sha256":"1".repeat(64), "document":document});
    let parsed: LifecycleDirective = decode_strict(&fields.to_string()).unwrap();
    assert!(
        matches!(&parsed, LifecycleDirective::PlanUpdate { document: exact, .. } if exact == document)
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), fields);
    for name in ["cause", "document_sha256", "document"] {
        let mut missing = fields.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert!(serde_json::from_value::<LifecycleDirective>(missing).is_err());
    }
    let mut unknown = fields;
    unknown["path"] = json!("other.json");
    assert!(serde_json::from_value::<LifecycleDirective>(unknown).is_err());
    let ask = json!({"ask":"plan_update"});
    assert_eq!(
        serde_json::from_value::<LifecycleAsk>(ask.clone()).unwrap(),
        LifecycleAsk::PlanUpdate
    );
    assert_eq!(serde_json::to_value(LifecycleAsk::PlanUpdate).unwrap(), ask);
    assert!(
        serde_json::from_value::<LifecycleAsk>(json!({"ask":"plan_update","body":{}})).is_err()
    );
}

#[test]
fn plan_updated_preserves_source_and_requires_turnless_null() {
    let wire = json!({
        "exchange":{"opener":"admin","ordinal":2},"position":"close",
        "payload":{"kind":"answer","body":{"kind":"plan_updated",
            "plan":{"plan_id":{"seed_id":"seed","local_id":"7"},"revision":"4"},
            "source":{"block_id":{"run":"run","sequence":"28","block":"0"},"turn":null}
        }}
    });
    let parsed: OrganEnvelope = decode_strict(&wire.to_string()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
    for path in ["plan", "source"] {
        let mut missing = wire.clone();
        missing["payload"]["body"]
            .as_object_mut()
            .unwrap()
            .remove(path);
        assert!(serde_json::from_value::<OrganEnvelope>(missing).is_err());
    }
    let mut missing_turn = wire.clone();
    missing_turn["payload"]["body"]["source"]
        .as_object_mut()
        .unwrap()
        .remove("turn");
    assert!(serde_json::from_value::<OrganEnvelope>(missing_turn).is_err());
    let mut unknown = wire;
    unknown["payload"]["body"]["extra"] = json!(false);
    assert!(serde_json::from_value::<OrganEnvelope>(unknown).is_err());
}

#[test]
fn plan_documents_keep_operation_maps_and_explicit_nullable_identity() {
    use weaver_types::state_flow::PlanUpdateDocument;
    let revision = json!({"plan_id":{"seed_id":"seed","local_id":"1"},"revision":"1"});
    let create = json!({"task":"OPEN the file.","steps":[{"id":null,"instruction":"READ the file."}],
        "categories":[{"id":"read","verbs":["READ"],"acts":[{"external_tool":{"name":"shell"}}],"review_required":true}],
        "groups":[{"id":"first","step_indices":["0"]}],"review":{"setup_id":"review","sha256":"1".repeat(64)}});
    let mut replacement = create.clone();
    replacement["task"] = Value::Null;
    replacement["preserved_closures"] = json!([]);
    let mut authorize = create.clone();
    authorize.as_object_mut().unwrap().remove("steps");
    for (name, body, expected) in [
        ("create", create.clone(), Value::Null),
        ("authorize", authorize, Value::Null),
        ("replace", replacement.clone(), revision.clone()),
        ("answer_break", json!({"replacement":replacement}), revision),
    ] {
        let mut root = json!({"format":"state-plan-update/1","expected":expected,"operation":{}});
        root["operation"][name] = body.clone();
        let parsed: PlanUpdateDocument = decode_strict(&root.to_string()).unwrap();
        assert!(parsed.validate_shape().is_ok());
        assert_eq!(serde_json::to_value(parsed).unwrap(), root);
        root["operation"][name] =
            Value::Array(body.as_object().unwrap().values().cloned().collect());
        assert!(
            decode_strict::<PlanUpdateDocument>(&root.to_string()).is_err(),
            "{name} accepted a tuple"
        );
    }
    let root =
        json!({"format":"state-plan-update/1","expected":null,"operation":{"create":create}});
    for (path, value) in [
        ("/format", json!({"state-plan-update/1":null})),
        ("/operation/create/steps/0", json!([null, "READ the file."])),
        (
            "/operation/create/categories/0",
            json!(["read", ["READ"], ["statement"], true]),
        ),
        (
            "/operation/create/categories/0/acts/0/external_tool",
            json!(["shell"]),
        ),
        ("/operation/create/groups/0", json!(["first", ["0"]])),
        (
            "/operation/create/review",
            json!(["review", "1".repeat(64)]),
        ),
    ] {
        let mut invalid = root.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            decode_strict::<PlanUpdateDocument>(&invalid.to_string()).is_err(),
            "accepted {path}"
        );
    }
    let mut missing = root.clone();
    missing["operation"]["create"]["steps"][0]
        .as_object_mut()
        .unwrap()
        .remove("id");
    assert!(decode_strict::<PlanUpdateDocument>(&missing.to_string()).is_err());
}
