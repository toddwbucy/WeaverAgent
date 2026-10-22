use super::*;

fn selected(store: &ServingSqlite, revision: u64, current: bool) -> PlanSnapshot {
    let selector = if current {
        ReadSelector::PlanCurrent {}
    } else {
        ReadSelector::Plan {
            plan: plan(revision),
        }
    };
    match reads::query(store, &selector).unwrap() {
        QueryResult::PlanCurrent {
            current: Some(snapshot),
            ..
        }
        | QueryResult::Plan { snapshot } => snapshot,
        _ => panic!("missing selected plan"),
    }
}

fn assert_summary(
    store: &ServingSqlite,
    revision: u64,
    current: bool,
    digit: &str,
    sequence: u64,
    turn: Option<&str>,
) {
    let snapshot = selected(store, revision, current);
    assert_eq!(snapshot.plan, plan(revision));
    assert_eq!(
        snapshot.document_sha256,
        Digest::new(digit.repeat(64)).unwrap()
    );
    assert_eq!(
        snapshot.last_source,
        SourcePosition {
            block_id: block(sequence, 0),
            turn: turn.map(str::to_owned),
        }
    );
}

#[test]
fn plan_summary_carrier_keeps_exact_transition_across_reads_reload_and_null_turn() {
    let mut fixture = Fixture::new();
    fixture.begin(false);
    for current in [false, true] {
        assert_summary(&fixture.store, 1, current, "1", 2, None);
    }
    let (successor, _) = reads::step_successor(
        &fixture.store.connection,
        &fixture.store.context.seed_id,
        &plan(1),
        None,
        fixture.store.read_limits.max_result_bytes,
    )
    .unwrap();
    let mut activation = effect(
        3,
        PlanEffect::Activate {
            expected: plan(1),
            step_id: step(1),
            step_span: span(3),
            group_span: None,
            successor,
        },
    );
    activation.source.turn = Some("plan-turn".into());
    fixture.file(activation);
    let mut later = statement(4, "message.assistant");
    later.source.turn = Some("unrelated-turn".into());
    fixture.file(later);
    for current in [false, true] {
        assert_summary(&fixture.store, 1, current, "1", 3, Some("plan-turn"));
    }
    // Adoption under a new operating run must retain the original source run
    // and turn, rather than substituting the boot identity or held frontier.
    fixture.common.run_id = "reloaded-run".into();
    let adopted = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    assert_eq!(adopted.context.run_id, "reloaded-run");
    for current in [false, true] {
        assert_summary(&adopted, 1, current, "1", 3, Some("plan-turn"));
    }
    let held = reads::query_encoded(&fixture.store, &ReadSelector::PlanCurrent {}, None).unwrap();
    fixture.file(effect(
        5,
        PlanEffect::Break {
            expected: plan(1),
            step_id: Some(step(1)),
            case: PlanBreakCase::WorkChanged,
            proposed_source: None,
        },
    ));
    for current in [false, true] {
        assert_summary(&fixture.store, 1, current, "1", 5, None);
    }
    let QueryResult::PlanCurrent {
        current: Some(before),
        ..
    } = decode_strict(std::str::from_utf8(&held).unwrap()).unwrap()
    else {
        panic!("held immutable current result");
    };
    assert_eq!(
        before.last_source,
        SourcePosition {
            block_id: block(3, 0),
            turn: Some("plan-turn".into()),
        }
    );
    assert_eq!(before.status, PlanStatus::Locked);
    let adopted = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    for current in [false, true] {
        assert_summary(&adopted, 1, current, "1", 5, None);
    }
}

#[test]
fn plan_summary_carrier_uses_each_revisions_operator_digest_and_keeps_flag_block_id() {
    let mut fixture = Fixture::new();
    fixture.begin(false);
    fixture.replace_with_digest(true, Digest::new("2".repeat(64)).unwrap());
    let adopted = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    for store in [&fixture.store, &adopted] {
        assert_summary(store, 1, false, "1", 4, None);
        assert_eq!(selected(store, 1, false).status, PlanStatus::Superseded);
        for current in [false, true] {
            assert_summary(store, 2, current, "2", 4, None);
        }
        let current = selected(store, 2, true);
        let result = reads::query(
            store,
            &ReadSelector::PlanFlags {
                plan: current.plan,
                after: current.last_source.block_id,
            },
        )
        .unwrap();
        let QueryResult::PlanFlags { snapshot } = result else {
            panic!("flags");
        };
        assert_eq!(snapshot.plan, plan(2));
        assert_eq!(snapshot.after, block(4, 0));
    }
}

#[test]
fn plan_summary_carrier_refuses_invalid_held_operator_context_on_reads_and_adoption() {
    for defect in 0..9 {
        let mut fixture = Fixture::new();
        fixture.begin(false);
        let id = source_id(&fixture.store.connection, &block(1, 0))
            .unwrap()
            .unwrap();
        let raw: String = fixture
            .store
            .connection
            .query_row(
                "SELECT context_raw FROM content_block WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        let mut context: serde_json::Value = serde_json::from_str(&raw).unwrap();
        match defect {
            0 => {
                context.as_object_mut().unwrap().remove("document_sha256");
            }
            1 => context["document_sha256"] = serde_json::json!("F".repeat(64)),
            2 => context["document_sha256"] = serde_json::Value::Null,
            3 => context["cause"]["uid"] = serde_json::json!(1000),
            4 => context["unknown"] = serde_json::Value::Null,
            5 => {
                fixture
                    .store
                    .connection
                    .execute(
                        "UPDATE content_block SET turn='wrong-turn' WHERE id=?1",
                        [id],
                    )
                    .unwrap();
            }
            6 => {
                // Healthy writes already enforce this attribution in the
                // catalog CHECK. Bypass it only to model damaged held content,
                // then restore enforcement before testing read/adoption guards.
                assert!(
                    fixture
                        .store
                        .connection
                        .execute("UPDATE content_block SET origin='model' WHERE id=?1", [id])
                        .is_err()
                );
                fixture
                    .store
                    .connection
                    .pragma_update(None, "ignore_check_constraints", true)
                    .unwrap();
                fixture
                    .store
                    .connection
                    .execute("UPDATE content_block SET origin='model' WHERE id=?1", [id])
                    .unwrap();
            }
            7 => {}
            8 => {
                context = serde_json::Value::Array(
                    ["document_sha256", "cause", "expected", "operation"]
                        .map(|name| context[name].clone())
                        .into(),
                )
            }
            _ => unreachable!(),
        }
        let mut raw = context.to_string();
        if defect == 7 {
            raw = raw.replace(
                "\"document_sha256\":",
                "\"document_sha256\":null,\"document_sha256\":",
            );
        }
        fixture
            .store
            .connection
            .execute(
                "UPDATE content_block SET context_raw=?1 WHERE id=?2",
                params![raw, id],
            )
            .unwrap();
        if defect == 6 {
            fixture
                .store
                .connection
                .pragma_update(None, "ignore_check_constraints", false)
                .unwrap();
        }
        for selector in [
            ReadSelector::PlanCurrent {},
            ReadSelector::Plan { plan: plan(1) },
        ] {
            assert!(
                matches!(
                    reads::query(&fixture.store, &selector),
                    Err(reads::ReadFailure::ComponentCorruption(_))
                ),
                "defect={defect}, {selector:?}"
            );
        }
        assert!(
            fixture.adopt(&fixture.store.image().unwrap()).is_err(),
            "defect={defect}"
        );
    }
}

#[test]
fn plan_summary_carrier_bytes_are_included_in_the_existing_result_ceiling() {
    let mut fixture = Fixture::new();
    fixture.begin(false);
    for selector in [
        ReadSelector::PlanCurrent {},
        ReadSelector::Plan { plan: plan(1) },
    ] {
        fixture.store.read_limits.max_result_bytes = 1_000_000.into();
        let bytes = reads::query_encoded(&fixture.store, &selector, None).unwrap();
        fixture.store.read_limits.max_result_bytes = (bytes.len() as u64).into();
        assert_eq!(
            reads::query_encoded(&fixture.store, &selector, None).unwrap(),
            bytes
        );
        fixture.store.read_limits.max_result_bytes = (bytes.len() as u64 - 1).into();
        assert!(matches!(
            reads::query_encoded(&fixture.store, &selector, None),
            Err(reads::ReadFailure::Refused(ReadRefusalCode::ResultTooLarge))
        ));
    }
}
