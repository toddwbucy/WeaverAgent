//! D1: one pinned preview crosses both owners. The store serves these exact
//! bytes and accepts their WorkRef. The harness pages the same bytes at several
//! capacities and constructs the same WorkRef (context_plan_closure_tests.rs).
//! A change to the preview encoding fails here first and must re-pin both.
use super::*;

const PREVIEW: &[u8] =
    include_bytes!("../../../../../../../../weaver-harness/src/flow/fixtures/work-preview.json");
const WORK: &str =
    include_str!("../../../../../../../../weaver-harness/src/flow/fixtures/work-preview-ref.json");

fn pinned() -> WorkRef {
    serde_json::from_str(WORK).unwrap()
}

fn closing(sequence: u64, work: WorkRef, outcome: SpanRef) -> StatementFile {
    effect(
        sequence,
        PlanEffect::Close {
            expected: plan(1),
            step_id: step(1),
            review_call: None,
            work,
            outcome_span: outcome,
        },
    )
}

/// The named guard refuses, and the database bytes are unchanged.
fn refused(fixture: &mut Fixture, mut filing: StatementFile, guard: &str) {
    filing.scheme_id = fixture.store.context.scheme_id.clone();
    let before = fixture
        .store
        .connection
        .serialize(rusqlite::MAIN_DB)
        .unwrap()
        .to_vec();
    let error = fixture.store.file(&filing).unwrap_err();
    assert_eq!(error.code, StateRefusalCode::InvalidEffect, "{error}");
    assert!(error.detail.contains(guard), "{error}");
    let after = fixture
        .store
        .connection
        .serialize(rusqlite::MAIN_DB)
        .unwrap();
    assert_eq!(&*after, before.as_slice());
    assert_eq!(snapshot(&fixture.store).steps[0].status, StepStatus::Active);
}

fn span_status(store: &ServingSqlite, id: u64) -> (String, String) {
    store
        .connection
        .query_row(
            "SELECT kind,status FROM span WHERE local_id=?1 AND revision=?2",
            params![blob(id), blob(1)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[test]
fn d1_store_serves_the_pinned_bytes_and_accepts_their_work_reference() {
    let mut fixture = active_fixture();
    let (work, served) = completed_work(&mut fixture);
    // Exact bytes, not JSON equality: the pinned fixture is the served encoding.
    assert_eq!(served, PREVIEW, "re-pin the shared preview fixture");
    assert_eq!(work, pinned());
    assert_eq!(digest(PREVIEW), work.content_sha256);

    // Negative: one encoded byte changed in the claimed preview.
    let mut changed = PREVIEW.to_vec();
    let at = changed.iter().position(|&b| b == b'z').unwrap();
    changed[at] = b'y';
    let mut wrong = pinned();
    wrong.content_sha256 = digest(&changed);
    refused(
        &mut fixture,
        closing(7, wrong, span(4)),
        "work preview changed",
    );

    // A stale (the step's own) or skipped outcome span is not the next span.
    for outcome in [span(3), span(5)] {
        refused(
            &mut fixture,
            closing(7, pinned(), outcome),
            "span allocation is not consecutive",
        );
    }

    fixture.file(closing(7, pinned(), span(4)));
    let current = snapshot(&fixture.store);
    let closed = &current.steps[0];
    assert_eq!(closed.status, StepStatus::Closed);
    assert_eq!(closed.outcome_span, Some(span(4)));
    assert_eq!(closed.work.as_ref(), Some(&pinned()));
    assert_eq!(closed.review_call, None);
    assert_eq!(current.status, PlanStatus::Locked);
    // The outcome is opened and closed below the step, which closes too.
    assert_eq!(
        span_status(&fixture.store, 4),
        ("step_outcome".into(), "closed".into())
    );
    let parent: Vec<u8> = fixture
        .store
        .connection
        .query_row(
            "SELECT parent_local_id FROM span WHERE local_id=?1",
            params![blob(4)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(parent, blob(3).to_vec());
    assert_eq!(span_status(&fixture.store, 3).1, "closed");
    assert_eq!(
        span_status(&fixture.store, 1).1,
        "open",
        "a step still waits"
    );

    // Historical verification uses the same encoding after dump and load.
    assert_historical(&fixture.store, &pinned(), PREVIEW);
    fixture.store = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    assert_historical(&fixture.store, &pinned(), PREVIEW);
}

#[test]
fn d1_store_refuses_a_frontier_with_an_incomplete_pair() {
    let mut fixture = active_fixture();
    let source = statement(4, "message.assistant");
    fixture.file(source.clone());
    let mut started = pair_file(5, PairRecord::Started(opening(&source)), "model");
    started.scheme_id = fixture.store.context.scheme_id.clone();
    fixture.store.file_pair(&started).unwrap();
    let through = fixture.store.position().unwrap().unwrap();
    let selector = ReadSelector::SpanPreview {
        span: span(3),
        through: through.clone(),
    };
    // The open pair keeps the work preview pending, so no WorkRef exists.
    assert!(fixture.store.query_encoded(&selector, None).is_err());
    let mut claimed = pinned();
    claimed.through = through;
    refused(&mut fixture, closing(6, claimed, span(4)), "SpanPending");
}

#[test]
fn d1_closing_the_last_step_closes_complete_ancestors() {
    let mut fixture = active_fixture();
    let (work, _) = completed_work(&mut fixture);
    fixture.file(closing(7, work, span(4)));
    let (successor, _) = reads::step_successor(
        &fixture.store.connection,
        &fixture.store.context.seed_id,
        &plan(1),
        Some(&step(1)),
        fixture.store.read_limits.max_result_bytes,
    )
    .unwrap();
    fixture.file(effect(
        8,
        PlanEffect::Activate {
            expected: plan(1),
            step_id: step(2),
            step_span: span(5),
            group_span: None,
            successor,
        },
    ));
    let through = fixture.store.position().unwrap().unwrap();
    let bytes = fixture
        .store
        .query_encoded(
            &ReadSelector::SpanPreview {
                span: span(5),
                through: through.clone(),
            },
            None,
        )
        .unwrap();
    let last = WorkRef {
        plan: plan(1),
        step_id: step(2),
        span: span(5),
        through,
        content_sha256: digest(&bytes),
    };
    fixture.file(effect(
        9,
        PlanEffect::Close {
            expected: plan(1),
            step_id: step(2),
            review_call: None,
            work: last,
            outcome_span: span(6),
        },
    ));
    assert_eq!(snapshot(&fixture.store).status, PlanStatus::Complete);
    for id in [1, 5, 6] {
        assert_eq!(span_status(&fixture.store, id).1, "closed", "span {id}");
    }
    fixture.store = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    assert_eq!(snapshot(&fixture.store).status, PlanStatus::Complete);
}
