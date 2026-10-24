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
    for defect in 0..10 {
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
            9 => {
                fixture
                    .store
                    .connection
                    .execute(
                        "UPDATE content_block SET scheme_id='another-scheme' WHERE id=?1",
                        [id],
                    )
                    .unwrap();
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

#[test]
fn plan_input_array_context_refuses_admission_without_committing_content() {
    let mut fixture = Fixture::new();
    let ctx = context(
        PlanInputOperation::Authorize {
            task: 0.into(),
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    let object = serde_json::to_value(&ctx).unwrap();
    let array = serde_json::Value::Array(
        ["document_sha256", "cause", "expected", "operation"]
            .map(|key| object[key].clone())
            .into(),
    );
    let mut filing = input(1, 0, &ctx);
    filing.scheme_id = fixture.store.context.scheme_id.clone();
    filing.context.as_mut().unwrap().raw = RawValue::from_string(array.to_string()).unwrap();
    let before = fixture.store.image().unwrap();
    let error = fixture.store.file(&filing).unwrap_err();
    assert_eq!(error.code, StateRefusalCode::InvalidEffect);
    assert_eq!(error.detail, "invalid operator plan context");
    assert_eq!(fixture.store.image().unwrap(), before);
    // The refusal left no content, mark or revision behind. The same source
    // identity can still begin a valid plan and survive ordinary adoption.
    fixture.begin(true);
    fixture.adopt(&fixture.store.image().unwrap()).unwrap();
}

#[test]
fn plan_operator_context_scheme_check_is_shared_by_filing_reads_and_adoption() {
    let mut fixture = Fixture::new();
    fixture.begin(true);
    let id = source_id(&fixture.store.connection, &block(1, 0))
        .unwrap()
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE content_block SET scheme_id='another-scheme' WHERE id=?1",
            [id],
        )
        .unwrap();
    for selector in [
        ReadSelector::PlanCurrent {},
        ReadSelector::Plan { plan: plan(1) },
    ] {
        assert!(matches!(
            reads::query(&fixture.store, &selector),
            Err(reads::ReadFailure::ComponentCorruption(
                "invalid plan operator source"
            ))
        ));
    }
    assert_eq!(
        fixture
            .adopt(&fixture.store.image().unwrap())
            .err()
            .unwrap()
            .code,
        StateRefusalCode::OpenFailed
    );

    // Exercise operator_input before any current-plan snapshot can perform the
    // same check: the input is held, but its authorizing effect is not filed yet.
    let mut fixture = Fixture::new();
    let ctx = context(
        PlanInputOperation::Authorize {
            task: 0.into(),
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    fixture.file(input(1, 0, &ctx));
    let id = source_id(&fixture.store.connection, &block(1, 0))
        .unwrap()
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE content_block SET scheme_id='another-scheme' WHERE id=?1",
            [id],
        )
        .unwrap();
    let mut filing = effect(
        2,
        PlanEffect::Authorize {
            plan: plan(1),
            source: block(1, 0),
            task: TaskScopeRef {
                task: span(1),
                prompt: span(2),
            },
            review: setup(),
        },
    );
    filing.scheme_id = fixture.store.context.scheme_id.clone();
    let before = fixture.store.image().unwrap();
    let error = fixture.store.file(&filing).unwrap_err();
    assert_eq!(error.code, StateRefusalCode::InvalidEffect);
    assert!(error.detail.contains("invalid plan operator source"));
    assert_eq!(fixture.store.image().unwrap(), before);
    fixture
        .store
        .connection
        .execute(
            "UPDATE content_block SET scheme_id=?1 WHERE id=?2",
            params![fixture.store.context.scheme_id, id],
        )
        .unwrap();
    fixture.store.file(&filing).unwrap();
    fixture.adopt(&fixture.store.image().unwrap()).unwrap();
}

#[test]
fn plan_transition_source_distinguishes_absence_from_a_held_non_event() {
    for missing in [false, true] {
        let mut fixture = Fixture::new();
        fixture.begin(false);
        let (id, detail) = if missing {
            // Model a damaged foreign key, then restore the enforcement gate
            // before exercising reads. Healthy writes cannot create this state.
            fixture
                .store
                .connection
                .pragma_update(None, "foreign_keys", false)
                .unwrap();
            (i64::MAX, "missing plan transition source")
        } else {
            let seed: i64 = fixture
                .store
                .connection
                .query_row(
                    "SELECT id FROM content_block WHERE source_kind='seed' LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            (seed, "plan transition source is not an event")
        };
        fixture
            .store
            .connection
            .execute("UPDATE plan_revision SET last_source=?1", [id])
            .unwrap();
        if missing {
            fixture
                .store
                .connection
                .pragma_update(None, "foreign_keys", true)
                .unwrap();
        }
        for selector in [
            ReadSelector::PlanCurrent {},
            ReadSelector::Plan { plan: plan(1) },
        ] {
            match reads::query(&fixture.store, &selector) {
                Err(reads::ReadFailure::ComponentCorruption(actual)) => assert_eq!(actual, detail),
                other => panic!("missing={missing}: {other:?}"),
            }
        }
    }
}
