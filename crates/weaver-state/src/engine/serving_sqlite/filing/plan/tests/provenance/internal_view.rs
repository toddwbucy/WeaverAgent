//! Internal plan metadata is not an externally serialized query response.
use super::*;

fn public_snapshot(store: &ServingSqlite, plan: &PlanRef) -> PlanSnapshot {
    let QueryResult::Plan { snapshot } =
        reads::query(store, &ReadSelector::Plan { plan: plan.clone() }).unwrap()
    else {
        panic!("historical plan result");
    };
    snapshot
}
fn assert_same_views(store: &ServingSqlite, revision: u64, current: bool) {
    let expected = public_snapshot(store, &plan(revision));
    assert_eq!(
        reads::plan_snapshot(&store.connection, &store.context.seed_id, &plan(revision)).unwrap(),
        expected
    );
    if current {
        assert_eq!(
            reads::current_plan_snapshot(&store.connection, &store.context.seed_id).unwrap(),
            Some(expected.clone())
        );
        let QueryResult::PlanCurrent { current, .. } =
            reads::query(store, &ReadSelector::PlanCurrent {}).unwrap()
        else {
            panic!("current plan result");
        };
        assert_eq!(current, Some(expected));
    }
}
fn assert_public_plan_too_large(store: &ServingSqlite) {
    for selector in [
        ReadSelector::PlanCurrent {},
        ReadSelector::Plan { plan: plan(1) },
    ] {
        assert!(
            matches!(
                reads::query(store, &selector),
                Err(reads::ReadFailure::Refused(ReadRefusalCode::ResultTooLarge))
            ),
            "{selector:?}"
        );
    }
}

#[test]
fn internal_plan_metadata_guard_commits_activation_when_public_plan_exceeds_its_bound() {
    let mut store = store();
    create(&mut store, 8, false, true);
    assert_same_views(&store, 1, true);
    let selector = ReadSelector::StepSuccessor {
        plan: plan(1),
        previous_step: None,
    };
    let instruction = reads::query_encoded(&store, &selector, None).unwrap();
    let plan_bytes = reads::query_encoded(&store, &ReadSelector::PlanCurrent {}, None).unwrap();
    assert!(
        plan_bytes.len() > instruction.len(),
        "the full plan, not its first instruction, crosses the limit"
    );
    store.read_limits.max_result_bytes = (instruction.len() as u64).into();
    assert_public_plan_too_large(&store);
    let QueryResult::StepSuccessor {
        reference: successor,
        ..
    } = reads::query(&store, &selector).unwrap()
    else {
        panic!("public successor remains valid at its own exact byte bound");
    };
    // This is a real atomic effect filing, including its mark and complete
    // source receipt. Only its internal full-plan metadata guard is unbounded.
    let mut filing = effect(
        3,
        PlanEffect::Activate {
            expected: plan(1),
            step_id: step(1),
            step_span: span(4),
            group_span: Some(span(3)),
            successor,
        },
    );
    filing.source.turn = Some("guard-turn".into());
    let commit = store.file(&filing).unwrap();
    assert_eq!(commit.block_id, filing.block_id);
    let current = reads::current_plan_snapshot(&store.connection, &store.context.seed_id)
        .unwrap()
        .unwrap();
    assert_eq!(current.steps[0].status, StepStatus::Active);
    assert_eq!(current.last_source.block_id, block(3, 0));
    assert_public_plan_too_large(&store);
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
    store.read_limits.max_result_bytes = 1_000_000.into();
    assert_same_views(&store, 1, true);
}

#[test]
fn internal_plan_metadata_guard_and_adoption_ignore_only_the_public_result_cap() {
    let mut fixture = Fixture::new();
    fixture.begin(false);
    fixture.store.read_limits.max_result_bytes = 1.into();
    assert_public_plan_too_large(&fixture.store);
    fixture.file(effect(
        3,
        PlanEffect::Break {
            expected: plan(1),
            step_id: None,
            case: PlanBreakCase::WorkChanged,
            proposed_source: None,
        },
    ));
    let current =
        reads::current_plan_snapshot(&fixture.store.connection, &fixture.store.context.seed_id)
            .unwrap()
            .unwrap();
    assert_eq!(current.status, PlanStatus::Broken);
    assert_eq!(current.last_source.block_id, block(3, 0));
    assert_public_plan_too_large(&fixture.store);
    let adopted = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    assert_eq!(
        reads::current_plan_snapshot(&adopted.connection, &adopted.context.seed_id).unwrap(),
        Some(current)
    );
    assert_public_plan_too_large(&adopted);
}

#[test]
fn internal_plan_metadata_views_preserve_full_historical_and_current_fields() {
    let mut fixture = Fixture::new();
    fixture.begin(false);
    assert_same_views(&fixture.store, 1, true);
    fixture.complete();
    assert_same_views(&fixture.store, 1, true);
    fixture.replace_with_digest(false, Digest::new("2".repeat(64)).unwrap());
    assert_same_views(&fixture.store, 1, false);
    assert_same_views(&fixture.store, 2, true);
    let adopted = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    assert_same_views(&adopted, 1, false);
    assert_same_views(&adopted, 2, true);
}

#[test]
fn internal_plan_metadata_guards_keep_source_and_row_corruption_checks() {
    for defect in 0..3 {
        let mut fixture = Fixture::new();
        fixture.begin(false);
        if defect == 2 {
            fixture.complete();
        }
        let db = &fixture.store.connection;
        match defect {
            0 => {
                let id = source_id(db, &block(1, 0)).unwrap().unwrap();
                let raw: String = db
                    .query_row(
                        "SELECT context_raw FROM content_block WHERE id=?1",
                        [id],
                        |row| row.get(0),
                    )
                    .unwrap();
                let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
                value["cause"]["uid"] = serde_json::json!(1000);
                db.execute(
                    "UPDATE content_block SET context_raw=?1 WHERE id=?2",
                    params![value.to_string(), id],
                )
                .unwrap();
            }
            1 => {
                let raw: String = db
                    .query_row("SELECT category FROM plan_category LIMIT 1", [], |row| {
                        row.get(0)
                    })
                    .unwrap();
                let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
                value["id"] = serde_json::json!("different-index");
                db.execute("UPDATE plan_category SET category=?1", [value.to_string()])
                    .unwrap();
            }
            2 => {
                db.execute("UPDATE plan_step SET work='[]'", []).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                reads::current_plan_snapshot(db, &fixture.store.context.seed_id),
                Err(reads::ReadFailure::ComponentCorruption(_))
            ),
            "internal current, defect={defect}"
        );
        assert!(
            matches!(
                reads::plan_snapshot(db, &fixture.store.context.seed_id, &plan(1)),
                Err(reads::ReadFailure::ComponentCorruption(_))
            ),
            "internal historical, defect={defect}"
        );
        for selector in [
            ReadSelector::PlanCurrent {},
            ReadSelector::Plan { plan: plan(1) },
        ] {
            assert!(
                matches!(
                    reads::query(&fixture.store, &selector),
                    Err(reads::ReadFailure::ComponentCorruption(_))
                ),
                "public {selector:?}, defect={defect}"
            );
        }
        let mut filing = effect(
            5,
            PlanEffect::Break {
                expected: plan(1),
                step_id: None,
                case: PlanBreakCase::WorkChanged,
                proposed_source: None,
            },
        );
        filing.scheme_id = fixture.store.context.scheme_id.clone();
        unchanged(&mut fixture.store, &filing);
    }
}
