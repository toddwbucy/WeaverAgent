use super::*;

fn active_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    let ctx = context(
        PlanInputOperation::Create {
            task: 0.into(),
            steps: (1..=2)
                .map(|index| PlanInputStep {
                    id: None,
                    block: index.into(),
                })
                .collect(),
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    for index in 0..=2 {
        fixture.file(input(1, index, &ctx));
    }
    fixture.file(effect(
        2,
        PlanEffect::Lock {
            plan: plan(1),
            source: PlanLockSource::Operator { input: block(1, 0) },
            task: TaskScopeRef {
                task: span(1),
                prompt: span(2),
            },
            steps: (1..=2)
                .map(|id| PlanStepInit {
                    step_id: step(id),
                    instruction: reference(1, id),
                })
                .collect(),
            review: setup(),
        },
    ));
    let (successor, _) = reads::step_successor(
        &fixture.store.connection,
        &fixture.store.context.seed_id,
        &plan(1),
        None,
        fixture.store.read_limits.max_result_bytes,
    )
    .unwrap();
    fixture.file(effect(
        3,
        PlanEffect::Activate {
            expected: plan(1),
            step_id: step(1),
            step_span: span(3),
            group_span: None,
            successor,
        },
    ));
    fixture
}

fn completed_work(fixture: &mut Fixture) -> (WorkRef, Vec<u8>) {
    let source = statement(4, "message.assistant");
    fixture.file(source.clone());
    let started = opening(&source);
    let ended = PairEnded {
        call_id: started.call_id.clone(),
        execution: Some(weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: 1,
        }),
        fate: TerminalPairFate::Completed,
        reason: None,
        outcome: Some(weaver_types::ToolOutcome::Result {
            content: "opaque {\"z\": 2, \"a\":1}\nresult".into(),
        }),
        loaded_from: None,
    };
    for (sequence, pair, origin) in [
        (5, PairRecord::Started(started), "model"),
        (6, PairRecord::Ended(ended), "tool"),
    ] {
        let mut filing = pair_file(sequence, pair, origin);
        filing.scheme_id = fixture.store.context.scheme_id.clone();
        fixture.store.file_pair(&filing).unwrap();
    }
    served_work(&fixture.store)
}

fn served_work(store: &ServingSqlite) -> (WorkRef, Vec<u8>) {
    let through = store.position().unwrap().unwrap();
    let bytes = store
        .query_encoded(
            &ReadSelector::SpanPreview {
                span: span(3),
                through: through.clone(),
            },
            None,
        )
        .unwrap();
    let work = WorkRef {
        plan: plan(1),
        step_id: step(1),
        span: span(3),
        through,
        content_sha256: digest(&bytes),
    };
    (work, bytes)
}

fn close_file(sequence: u64, work: WorkRef) -> StatementFile {
    effect(
        sequence,
        PlanEffect::Close {
            expected: plan(1),
            step_id: step(1),
            review_call: None,
            work,
            outcome_span: span(4),
        },
    )
}

fn assert_historical(store: &ServingSqlite, work: &WorkRef, served: &[u8]) {
    let historical = reads::historical_span_preview_encoded(
        &store.connection,
        &store.context.seed_id,
        &work.span,
        &work.through,
    )
    .unwrap();
    assert_eq!(historical, served);
    assert_eq!(digest(&historical), work.content_sha256);
}

#[test]
fn served_preview_bytes_survive_close_adoption_and_preserved_closure() {
    let mut fixture = active_fixture();
    let (work, served) = completed_work(&mut fixture);
    assert!(String::from_utf8_lossy(&served).contains("opaque"));
    let current = reads::span_preview_encoded(
        &fixture.store.connection,
        &fixture.store.context.seed_id,
        &work.span,
        &work.through,
        fixture.store.read_limits.max_result_bytes,
    )
    .unwrap();
    assert_eq!(current, served);

    // Equal JSON is not equal evidence. A digest of a different encoding must
    // refuse before any outcome or closing mark becomes durable.
    let value: serde_json::Value = serde_json::from_slice(&served).unwrap();
    let reencoded = serde_json::to_vec_pretty(&value).unwrap();
    assert_ne!(served, reencoded);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&reencoded).unwrap(),
        value
    );
    let mut wrong = work.clone();
    wrong.content_sha256 = digest(&reencoded);
    let mut rejected = close_file(7, wrong);
    rejected.scheme_id = fixture.store.context.scheme_id.clone();
    unchanged(&mut fixture.store, &rejected);

    fixture.file(close_file(7, work.clone()));
    assert_historical(&fixture.store, &work, &served);
    fixture.store = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    assert_historical(&fixture.store, &work, &served);

    let ctx = context(
        PlanInputOperation::Replace(PlanInputReplacement {
            task: None,
            steps: vec![
                PlanInputStep {
                    id: None,
                    block: 0.into(),
                },
                PlanInputStep {
                    id: Some(step(1)),
                    block: 1.into(),
                },
            ],
            categories: categories(false),
            groups: vec![],
            review: setup(),
            preserved_closures: vec![step(1)],
        }),
        Some(plan(1)),
    );
    fixture.file(input(8, 0, &ctx));
    fixture.file(input(8, 1, &ctx));
    fixture.file(effect(
        9,
        PlanEffect::Replace {
            expected: plan(1),
            next: plan(2),
            source: block(8, 0),
            task: TaskScopeRef {
                task: span(1),
                prompt: span(2),
            },
            steps: vec![
                PlanStepInit {
                    step_id: step(3),
                    instruction: reference(8, 0),
                },
                PlanStepInit {
                    step_id: step(1),
                    instruction: reference(1, 1),
                },
            ],
            review: setup(),
        },
    ));
    fixture.store = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    let current = snapshot(&fixture.store);
    assert_eq!(current.plan, plan(2));
    assert_eq!(current.steps[1].status, StepStatus::Closed);
    assert_eq!(current.steps[1].work.as_ref(), Some(&work));
    assert_historical(&fixture.store, &work, &served);
}

#[test]
fn unchanged_old_preview_does_not_authorize_closure_past_new_work() {
    let mut fixture = active_fixture();
    let (work, served) = completed_work(&mut fixture);
    fixture.file(statement(7, "message.assistant"));
    let reread = fixture
        .store
        .query_encoded(
            &ReadSelector::SpanPreview {
                span: work.span.clone(),
                through: work.through.clone(),
            },
            None,
        )
        .unwrap();
    assert_eq!(served, reread);
    let mut rejected = close_file(8, work);
    rejected.scheme_id = fixture.store.context.scheme_id.clone();
    unchanged(&mut fixture.store, &rejected);
    assert_eq!(snapshot(&fixture.store).steps[0].status, StepStatus::Active);

    let (current, _) = served_work(&fixture.store);
    fixture.file(close_file(8, current));
    assert_eq!(snapshot(&fixture.store).steps[0].status, StepStatus::Closed);
}

#[test]
fn adoption_recomputes_work_bytes_when_stored_closure_copies_agree() {
    let mut fixture = active_fixture();
    let (work, served) = completed_work(&mut fixture);
    fixture.file(close_file(7, work.clone()));
    fixture.adopt(&fixture.store.image().unwrap()).unwrap();

    let parsed: serde_json::Value = serde_json::from_slice(&served).unwrap();
    let changed_digest = digest(&serde_json::to_vec_pretty(&parsed).unwrap());
    assert_ne!(changed_digest, work.content_sha256);
    fixture
        .store
        .connection
        .execute(
            "UPDATE plan_step SET work=json_set(work,'$.content_sha256',?1) WHERE work IS NOT NULL",
            [changed_digest.as_str()],
        )
        .unwrap();
    let source = source_id(&fixture.store.connection, &block(7, 0))
        .unwrap()
        .unwrap();
    let mut changed = work;
    changed.content_sha256 = changed_digest;
    let closing = PlanEffect::Close {
        expected: plan(1),
        step_id: step(1),
        review_call: None,
        work: changed,
        outcome_span: span(4),
    };
    fixture
        .store
        .connection
        .execute(
            "UPDATE content_block SET effect=?1,context_raw=?2 WHERE id=?3",
            params![
                serde_json::to_string(&StatementEffect::Plan(Box::new(closing.clone()))).unwrap(),
                serde_json::to_string(&closing).unwrap(),
                source,
            ],
        )
        .unwrap();
    // The whole image hash and both stored closure copies now agree. Only
    // re-reading the original encoded preview can expose the false evidence.
    let error = fixture
        .adopt(&fixture.store.image().unwrap())
        .err()
        .expect("false work digest");
    assert_eq!(error.code, StateRefusalCode::OpenFailed);
    assert!(
        error.detail.contains("stored closure work digest changed"),
        "{error}"
    );
}

#[path = "work_bytes_d1.rs"]
mod d1;
