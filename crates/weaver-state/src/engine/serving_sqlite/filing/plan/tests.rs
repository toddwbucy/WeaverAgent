use super::super::tests::{opening, pair_file, statement, store};
use super::*;
use crate::{ServingStore, engine::serving_sqlite::ServingSqlite};
use serde_json::value::RawValue;
use sha2::{Digest as _, Sha256};

mod provenance;

fn block(sequence: u64, index: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: index.into(),
    }
}
fn span(id: u64) -> SpanRef {
    SpanRef {
        seed_id: "a".repeat(32),
        local_id: id.into(),
        revision: 1.into(),
    }
}
fn plan(revision: u64) -> PlanRef {
    PlanRef {
        plan_id: PlanId {
            seed_id: "a".repeat(32),
            local_id: 1.into(),
        },
        revision: revision.into(),
    }
}
fn step(id: u64) -> StepId {
    StepId {
        seed_id: "a".repeat(32),
        local_id: id.into(),
    }
}
fn reference(sequence: u64, index: u64) -> StatementRef {
    StatementRef {
        block_id: block(sequence, index),
        binding: 0.into(),
    }
}
fn setup() -> ReviewSetupRef {
    ReviewSetupRef {
        setup_id: "review".into(),
        sha256: Digest::new("0".repeat(64)).unwrap(),
    }
}
fn categories(review: bool) -> Vec<Category> {
    vec![Category {
        id: "read".into(),
        verbs: vec!["read".into()],
        acts: vec![PlanAct::Statement],
        review_required: review,
    }]
}
fn context(operation: PlanInputOperation, expected: Option<PlanRef>) -> PlanInputContext {
    PlanInputContext {
        document_sha256: Digest::new("1".repeat(64)).unwrap(),
        cause: weaver_types::Cause { uid: 0 },
        expected,
        operation,
    }
}
fn input(sequence: u64, index: u64, context: &PlanInputContext) -> StatementFile {
    let mut file = statement(sequence, "plan.event");
    file.block_id = block(sequence, index);
    file.source.channel = "input".into();
    file.source.origin = "operator".into();
    file.source.turn = None;
    file.source_path = vec![
        SourcePathPart::Key("message".into()),
        SourcePathPart::Key("content".into()),
        SourcePathPart::Index {
            index: index.into(),
        },
    ];
    if index == 0 {
        file.context = Some(SourceContext {
            source_path: vec![SourcePathPart::Key("context".into())],
            raw: RawValue::from_string(serde_json::to_string(context).unwrap()).unwrap(),
        });
    }
    file
}
fn effect(sequence: u64, effect: PlanEffect) -> StatementFile {
    let kind = if matches!(effect, PlanEffect::Close { .. }) {
        "step.outcome"
    } else {
        "plan.event"
    };
    let mut file = statement(sequence, kind);
    file.source.channel = "internal".into();
    file.source.origin = "loop".into();
    file.source.turn = None;
    file.source_path
        .insert(0, SourcePathPart::Key("message".into()));
    file.context = Some(SourceContext {
        source_path: vec![SourcePathPart::Key("context".into())],
        raw: RawValue::from_string(serde_json::to_string(&effect).unwrap()).unwrap(),
    });
    file.effect = Some(StatementEffect::Plan(Box::new(effect)));
    file
}
fn snapshot(store: &ServingSqlite) -> PlanSnapshot {
    reads::current_plan_snapshot(&store.connection, &store.context.seed_id)
        .unwrap()
        .unwrap()
}
fn unchanged(store: &mut ServingSqlite, filing: &StatementFile) {
    let before = store
        .connection
        .serialize(rusqlite::MAIN_DB)
        .unwrap()
        .to_vec();
    let error = store.file(filing).unwrap_err();
    assert_eq!(error.code, StateRefusalCode::InvalidEffect, "{error}");
    let after = store.connection.serialize(rusqlite::MAIN_DB).unwrap();
    assert_eq!(&*after, before.as_slice());
}
fn create(store: &mut ServingSqlite, count: u64, review: bool, group: bool) {
    let groups = if group {
        vec![GroupDraft {
            id: "group".into(),
            step_indices: (0..count).map(Into::into).collect(),
        }]
    } else {
        vec![]
    };
    let ctx = context(
        PlanInputOperation::Create {
            task: 0.into(),
            steps: (1..=count)
                .map(|i| PlanInputStep {
                    id: None,
                    block: i.into(),
                })
                .collect(),
            categories: categories(review),
            groups,
            review: setup(),
        },
        None,
    );
    for index in 0..=count {
        store.file(&input(1, index, &ctx)).unwrap();
    }
    store
        .file(&effect(
            2,
            PlanEffect::Lock {
                plan: plan(1),
                source: PlanLockSource::Operator { input: block(1, 0) },
                task: TaskScopeRef {
                    task: span(1),
                    prompt: span(2),
                },
                steps: (1..=count)
                    .map(|id| PlanStepInit {
                        step_id: step(id),
                        instruction: reference(1, id),
                    })
                    .collect(),
                review: setup(),
            },
        ))
        .unwrap();
}
fn activate(store: &mut ServingSqlite, sequence: u64, id: u64, span_id: u64, group: Option<u64>) {
    let current = snapshot(store);
    let index = current
        .steps
        .iter()
        .position(|s| s.step_id == step(id))
        .unwrap();
    let previous = if index == 0 {
        None
    } else {
        Some(current.steps[index - 1].step_id.clone())
    };
    let (successor, _) = reads::step_successor(
        &store.connection,
        &store.context.seed_id,
        &current.plan,
        previous.as_ref(),
        store.read_limits.max_result_bytes,
    )
    .unwrap();
    store
        .file(&effect(
            sequence,
            PlanEffect::Activate {
                expected: current.plan,
                step_id: step(id),
                step_span: span(span_id),
                group_span: group.map(span),
                successor,
            },
        ))
        .unwrap();
}
fn work_ref(store: &ServingSqlite, through: u64) -> WorkRef {
    let current = snapshot(store);
    let step = current
        .steps
        .iter()
        .find(|s| s.status == StepStatus::Active)
        .unwrap();
    let through = SourcePosition {
        block_id: block(through, 0),
        turn: None,
    };
    let span = step.span.clone().unwrap();
    let bytes = reads::span_preview_encoded(
        &store.connection,
        &store.context.seed_id,
        &span,
        &through,
        store.read_limits.max_result_bytes,
    )
    .unwrap();
    WorkRef {
        plan: current.plan,
        step_id: step.step_id.clone(),
        span,
        through,
        content_sha256: Digest::new(format!("{:x}", Sha256::digest(bytes))).unwrap(),
    }
}
fn close(
    store: &mut ServingSqlite,
    sequence: u64,
    outcome: u64,
    work: WorkRef,
    review: Option<CallId>,
) {
    store
        .file(&effect(
            sequence,
            PlanEffect::Close {
                expected: work.plan.clone(),
                step_id: work.step_id.clone(),
                review_call: review,
                work,
                outcome_span: span(outcome),
            },
        ))
        .unwrap();
}

#[test]
fn plan_lock_activation_and_unreviewed_close_are_atomic_and_complete_ancestors() {
    let mut store = store();
    create(&mut store, 2, false, true);
    activate(&mut store, 3, 1, 4, Some(3));
    let work = work_ref(&store, 3);
    close(&mut store, 4, 5, work, None);
    let first = snapshot(&store);
    assert_eq!(first.status, PlanStatus::Locked);
    assert_eq!(first.steps[0].status, StepStatus::Closed);
    activate(&mut store, 5, 2, 6, Some(3));
    let work = work_ref(&store, 5);
    close(&mut store, 6, 7, work, None);
    let complete = snapshot(&store);
    assert_eq!(complete.status, PlanStatus::Complete);
    let open: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM span WHERE status='open'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(open, 0);
    let stale = effect(
        7,
        PlanEffect::Break {
            expected: plan(1),
            step_id: None,
            case: PlanBreakCase::WorkChanged,
            proposed_source: None,
        },
    );
    unchanged(&mut store, &stale);
}

#[test]
fn operator_effect_requires_all_blocks_root_provenance_and_exact_indices() {
    let mut store = store();
    let mut ctx = context(
        PlanInputOperation::Create {
            task: 0.into(),
            steps: vec![PlanInputStep {
                id: None,
                block: 1.into(),
            }],
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    ctx.cause.uid = 1000;
    store.file(&input(1, 0, &ctx)).unwrap();
    store.file(&input(1, 1, &ctx)).unwrap();
    let lock = effect(
        2,
        PlanEffect::Lock {
            plan: plan(1),
            source: PlanLockSource::Operator { input: block(1, 0) },
            task: TaskScopeRef {
                task: span(1),
                prompt: span(2),
            },
            steps: vec![PlanStepInit {
                step_id: step(1),
                instruction: reference(1, 1),
            }],
            review: setup(),
        },
    );
    unchanged(&mut store, &lock);
    ctx.cause.uid = 0;
    store.file(&input(3, 0, &ctx)).unwrap();
    let incomplete = effect(
        4,
        PlanEffect::Lock {
            plan: plan(1),
            source: PlanLockSource::Operator { input: block(3, 0) },
            task: TaskScopeRef {
                task: span(1),
                prompt: span(2),
            },
            steps: vec![PlanStepInit {
                step_id: step(1),
                instruction: reference(3, 1),
            }],
            review: setup(),
        },
    );
    unchanged(&mut store, &incomplete);
}

#[test]
fn replacement_preserves_a_later_closed_step_without_hiding_the_new_first_step() {
    let mut store = store();
    create(&mut store, 2, false, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    close(&mut store, 4, 4, work, None);
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
    store.file(&input(5, 0, &ctx)).unwrap();
    store.file(&input(5, 1, &ctx)).unwrap();
    let replacement = PlanEffect::Replace {
        expected: plan(1),
        next: plan(2),
        source: block(5, 0),
        task: TaskScopeRef {
            task: span(1),
            prompt: span(2),
        },
        steps: vec![
            PlanStepInit {
                step_id: step(3),
                instruction: reference(5, 0),
            },
            PlanStepInit {
                step_id: step(1),
                instruction: reference(1, 1),
            },
        ],
        review: setup(),
    };
    store.file(&effect(6, replacement)).unwrap();
    let current = snapshot(&store);
    assert_eq!(current.steps[1].status, StepStatus::Closed);
    assert_eq!(current.steps[1].span, Some(span(3)));
    assert_eq!(current.steps[1].work.as_ref().unwrap().plan, plan(1));
    let (next, _) = reads::step_successor(
        &store.connection,
        &store.context.seed_id,
        &plan(2),
        None,
        store.read_limits.max_result_bytes,
    )
    .unwrap();
    assert_eq!(next.step_id, step(3));
    activate(&mut store, 7, 3, 5, None);
    let work = work_ref(&store, 7);
    close(&mut store, 8, 6, work, None);
    assert_eq!(snapshot(&store).status, PlanStatus::Complete);
}

#[test]
fn closure_refuses_changed_work_digest_and_closes_a_review_required_step_unreviewed() {
    let mut store = store();
    create(&mut store, 1, true, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    let mut bad = work.clone();
    bad.content_sha256 = Digest::new("f".repeat(64)).unwrap();
    unchanged(
        &mut store,
        &effect(
            4,
            PlanEffect::Close {
                expected: plan(1),
                step_id: step(1),
                review_call: Some(CallId {
                    run: "run".into(),
                    sequence: 8.into(),
                    call: 0.into(),
                }),
                work: bad,
                outcome_span: span(4),
            },
        ),
    );
    // review_required is held operator data the store returns and does not
    // enforce (#423): an unreviewed close of that step files and verifies.
    // The refused filing above held sequence 4 and was not committed; this
    // close takes a fresh sequence.
    close(&mut store, 5, 4, work, None);
    let closed = snapshot(&store);
    assert!(closed.categories[0].review_required);
    assert_eq!(closed.steps[0].review_call, None);
    assert_eq!(closed.status, PlanStatus::Complete);
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
}

#[test]
fn reviewed_close_requires_exact_setup_work_pair_and_fixed_frontier() {
    let mut store = store();
    create(&mut store, 1, true, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    let mut send = statement(4, "loop.instruction");
    send.source.channel = "internal".into();
    send.source.origin = "loop".into();
    send.source_path
        .insert(0, SourcePathPart::Key("message".into()));
    store.file(&send).unwrap();
    let mut start = opening(&send);
    start.lane = PairLane::Review;
    start.call_id.sequence = send.block_id.sequence;
    start.call_id.call = 0.into();
    start.call_block = 0.into();
    start.request = PairRequest::Review {
        setup: setup(),
        work: Box::new(work.clone()),
        command: "review".into(),
    };
    store
        .file_pair(&pair_file(5, PairRecord::Started(start.clone()), "loop"))
        .unwrap();
    let ended = PairEnded {
        call_id: start.call_id.clone(),
        execution: Some(weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: 1,
        }),
        fate: TerminalPairFate::Completed,
        reason: None,
        outcome: Some(weaver_types::ToolOutcome::Result {
            content: "passed".into(),
        }),
        loaded_from: None,
    };
    store
        .file_pair(&pair_file(6, PairRecord::Ended(ended), "tool"))
        .unwrap();
    close(&mut store, 7, 4, work, Some(start.call_id));
    assert_eq!(snapshot(&store).status, PlanStatus::Complete);
}

#[test]
fn model_lock_accepts_unique_repaired_statements_from_multiple_emissions_once() {
    let mut store = store();
    let ctx = context(
        PlanInputOperation::Authorize {
            task: 0.into(),
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    store.file(&input(1, 0, &ctx)).unwrap();
    store
        .file(&effect(
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
        ))
        .unwrap();
    store.file(&statement(3, "message.assistant")).unwrap();
    store.file(&statement(4, "message.assistant")).unwrap();
    let lock = PlanEffect::Lock {
        plan: plan(1),
        source: PlanLockSource::Model {
            authorization: block(1, 0),
            candidate: vec![reference(3, 0), reference(4, 0)],
        },
        task: TaskScopeRef {
            task: span(1),
            prompt: span(2),
        },
        steps: vec![
            PlanStepInit {
                step_id: step(1),
                instruction: reference(3, 0),
            },
            PlanStepInit {
                step_id: step(2),
                instruction: reference(4, 0),
            },
        ],
        review: setup(),
    };
    let mut duplicate = lock.clone();
    if let PlanEffect::Lock {
        source: PlanLockSource::Model { candidate, .. },
        steps,
        ..
    } = &mut duplicate
    {
        candidate[1] = candidate[0].clone();
        steps[1].instruction = steps[0].instruction.clone();
    }
    unchanged(&mut store, &effect(5, duplicate));
    store.file(&effect(5, lock.clone())).unwrap();
    assert_eq!(snapshot(&store).steps.len(), 2);
    unchanged(&mut store, &effect(6, lock));
}

#[test]
fn plan_image_checks_preserved_closure_and_ignores_the_query_size_limit() {
    let mut store = store();
    create(&mut store, 1, false, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    close(&mut store, 4, 4, work, None);
    let tiny = ReadLimits {
        max_result_bytes: 1.into(),
        measurement_ref: "not an image bound".into(),
    };
    verify_image_plan(&store.connection, &store.context.seed_id, &tiny).unwrap();
    store
        .connection
        .execute(
            "UPDATE plan_step SET work=json_set(work,'$.content_sha256',?1)",
            ["f".repeat(64)],
        )
        .unwrap();
    let id = source_id(&store.connection, &block(4, 0)).unwrap().unwrap();
    let raw: String = store
        .connection
        .query_row(
            "SELECT effect FROM content_block WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let mut effect: StatementEffect = decode_strict(&raw).unwrap();
    let StatementEffect::Plan(plan_effect) = &mut effect else {
        panic!("plan fixture")
    };
    let PlanEffect::Close { work, .. } = plan_effect.as_mut() else {
        panic!("closure fixture")
    };
    work.content_sha256 = Digest::new("f".repeat(64)).unwrap();
    let context = serde_json::to_string(plan_effect).unwrap();
    store
        .connection
        .execute(
            "UPDATE content_block SET effect=?1,context_raw=?2 WHERE id=?3",
            params![serde_json::to_string(&effect).unwrap(), context, id],
        )
        .unwrap();
    assert!(verify_image_plan(&store.connection, &store.context.seed_id, &tiny).is_err());
}

#[test]
fn new_operator_plan_retires_broken_execution_without_closing_it() {
    let mut store = store();
    create(&mut store, 1, false, true);
    activate(&mut store, 3, 1, 4, Some(3));
    store
        .file(&effect(
            4,
            PlanEffect::Break {
                expected: plan(1),
                step_id: Some(step(1)),
                case: PlanBreakCase::WrongCategory,
                proposed_source: None,
            },
        ))
        .unwrap();
    let ctx = context(
        PlanInputOperation::Create {
            task: 0.into(),
            steps: vec![PlanInputStep {
                id: None,
                block: 1.into(),
            }],
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    store.file(&input(5, 0, &ctx)).unwrap();
    store.file(&input(5, 1, &ctx)).unwrap();
    let next = PlanRef {
        plan_id: PlanId {
            seed_id: store.context.seed_id.clone(),
            local_id: 2.into(),
        },
        revision: 1.into(),
    };
    store
        .file(&effect(
            6,
            PlanEffect::Lock {
                plan: next.clone(),
                source: PlanLockSource::Operator { input: block(5, 0) },
                task: TaskScopeRef {
                    task: span(5),
                    prompt: span(6),
                },
                steps: vec![PlanStepInit {
                    step_id: step(2),
                    instruction: reference(5, 1),
                }],
                review: setup(),
            },
        ))
        .unwrap();
    let old = reads::plan_snapshot(&store.connection, &store.context.seed_id, &plan(1)).unwrap();
    assert_eq!(old.status, PlanStatus::Superseded);
    assert_eq!(old.steps[0].status, StepStatus::Retired);
    assert_eq!(snapshot(&store).plan, next);
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
}

#[test]
fn closure_refuses_any_new_content_after_unreviewed_work_and_keeps_active_marks() {
    let mut store = store();
    create(&mut store, 1, false, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    let mut changed_digest = work.clone();
    changed_digest.content_sha256 = Digest::new("f".repeat(64)).unwrap();
    unchanged(
        &mut store,
        &effect(
            4,
            PlanEffect::Close {
                expected: plan(1),
                step_id: step(1),
                review_call: None,
                work: changed_digest,
                outcome_span: span(4),
            },
        ),
    );
    store.file(&statement(4, "message.assistant")).unwrap();
    unchanged(
        &mut store,
        &effect(
            5,
            PlanEffect::Close {
                expected: plan(1),
                step_id: step(1),
                review_call: None,
                work,
                outcome_span: span(4),
            },
        ),
    );
    assert_eq!(snapshot(&store).steps[0].status, StepStatus::Active);
}

#[test]
fn replacement_of_completed_plan_renews_task_and_preserves_terminal_evidence() {
    completed_plan_renewal(false);
}

#[test]
fn completed_plan_renewal_retires_an_intervening_task_and_keeps_the_old_prompt() {
    completed_plan_renewal(true);
}

fn completed_plan_renewal(intervening: bool) {
    let mut store = store();
    create(&mut store, 1, false, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    close(&mut store, 4, 4, work, None);
    let (input_sequence, task_id) = if intervening {
        let mut source = statement(5, "message.user");
        source.text = "Read data and file.".into();
        source.raw_block =
            RawValue::from_string(r#"{"type":"text","text":"Read data and file."}"#.into())
                .unwrap();
        source.bindings[0].statement_span.end = (source.text.len() as u64).into();
        source.bindings[0].nouns[0].value = "data".into();
        store.file(&source).unwrap();
        let mark = MarkEffect::BeginTask {
            task: span(5),
            prompt: span(6),
            source: ContentRef::Event {
                block_id: source.block_id,
            },
        };
        let mut filing = statement(6, "state.mark");
        filing.source.channel = "internal".into();
        filing.source.origin = "loop".into();
        filing
            .source_path
            .insert(0, SourcePathPart::Key("message".into()));
        filing.context = Some(SourceContext {
            source_path: vec![SourcePathPart::Key("context".into())],
            raw: RawValue::from_string(serde_json::to_string(&mark).unwrap()).unwrap(),
        });
        filing.effect = Some(StatementEffect::Mark(mark));
        store.file(&filing).unwrap();
        (7, 7)
    } else {
        (5, 5)
    };
    let ctx = context(
        PlanInputOperation::Replace(PlanInputReplacement {
            task: None,
            steps: vec![PlanInputStep {
                id: Some(step(1)),
                block: 0.into(),
            }],
            categories: categories(false),
            groups: vec![],
            review: setup(),
            preserved_closures: vec![step(1)],
        }),
        Some(plan(1)),
    );
    store.file(&input(input_sequence, 0, &ctx)).unwrap();
    store
        .file(&effect(
            input_sequence + 1,
            PlanEffect::Replace {
                expected: plan(1),
                next: plan(2),
                source: block(input_sequence, 0),
                task: TaskScopeRef {
                    task: span(task_id),
                    prompt: span(task_id + 1),
                },
                steps: vec![PlanStepInit {
                    step_id: step(1),
                    instruction: reference(1, 1),
                }],
                review: setup(),
            },
        ))
        .unwrap();
    let current = snapshot(&store);
    assert_eq!(current.status, PlanStatus::Complete);
    assert_eq!(current.task.task, span(task_id));
    assert_eq!(current.steps[0].span, Some(span(3)));
    let prompt_source: (String, Vec<u8>, Vec<u8>) = store.connection.query_row(
        "SELECT c.run,c.sequence,c.block FROM span_prefix p JOIN content_block c ON c.id=p.block_id WHERE p.local_id=?1 AND p.ordinal=?2",
        params![blob(task_id + 1), blob(0)],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(
        prompt_source,
        ("run".into(), blob(1).to_vec(), blob(0).to_vec())
    );
    if intervening {
        let status: String = store
            .connection
            .query_row(
                "SELECT status FROM span WHERE local_id=?1",
                [blob(5)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "retired");
    }
    marks::verify_image_marks(&store.connection, &store.context.seed_id).unwrap();
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
}

#[test]
fn invalid_group_and_changed_preserved_category_roll_back_the_whole_effect() {
    let mut store = store();
    create(&mut store, 1, false, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    close(&mut store, 4, 4, work, None);
    let ctx = context(
        PlanInputOperation::Replace(PlanInputReplacement {
            task: None,
            steps: vec![PlanInputStep {
                id: Some(step(1)),
                block: 0.into(),
            }],
            categories: categories(true),
            groups: vec![],
            review: setup(),
            preserved_closures: vec![step(1)],
        }),
        Some(plan(1)),
    );
    store.file(&input(5, 0, &ctx)).unwrap();
    unchanged(
        &mut store,
        &effect(
            6,
            PlanEffect::Replace {
                expected: plan(1),
                next: plan(2),
                source: block(5, 0),
                task: TaskScopeRef {
                    task: span(5),
                    prompt: span(6),
                },
                steps: vec![PlanStepInit {
                    step_id: step(1),
                    instruction: reference(1, 1),
                }],
                review: setup(),
            },
        ),
    );
    let ctx = context(
        PlanInputOperation::Replace(PlanInputReplacement {
            task: None,
            steps: vec![PlanInputStep {
                id: Some(step(1)),
                block: 0.into(),
            }],
            categories: categories(false),
            groups: vec![GroupDraft {
                id: "outside".into(),
                step_indices: vec![1.into()],
            }],
            review: setup(),
            preserved_closures: vec![],
        }),
        Some(plan(1)),
    );
    store.file(&input(7, 0, &ctx)).unwrap();
    unchanged(
        &mut store,
        &effect(
            8,
            PlanEffect::Replace {
                expected: plan(1),
                next: plan(2),
                source: block(7, 0),
                task: TaskScopeRef {
                    task: span(5),
                    prompt: span(6),
                },
                steps: vec![PlanStepInit {
                    step_id: step(1),
                    instruction: reference(7, 0),
                }],
                review: setup(),
            },
        ),
    );
}

#[test]
fn image_rejects_agreeing_effect_rows_that_skip_ids_or_retarget_operator_instructions() {
    for skip_id in [true, false] {
        let mut store = store();
        store.file(&statement(9, "message.assistant")).unwrap();
        create(&mut store, 1, false, false);
        verify_image_plan(
            &store.connection,
            &store.context.seed_id,
            &store.read_limits,
        )
        .unwrap();
        let source = source_id(&store.connection, &block(2, 0)).unwrap().unwrap();
        let raw: String = store
            .connection
            .query_row(
                "SELECT effect FROM content_block WHERE id=?1",
                [source],
                |row| row.get(0),
            )
            .unwrap();
        let mut effect: StatementEffect = decode_strict(&raw).unwrap();
        let StatementEffect::Plan(plan_effect) = &mut effect else {
            panic!("plan fixture")
        };
        let PlanEffect::Lock { steps, .. } = plan_effect.as_mut() else {
            panic!("lock fixture")
        };
        if skip_id {
            steps[0].step_id = step(3);
            store
                .connection
                .execute("UPDATE plan_step SET step_id=?1", [blob(3)])
                .unwrap();
        } else {
            steps[0].instruction = reference(9, 0);
            let held = marks::resolve_statement(&store.connection, &reference(9, 0)).unwrap();
            store
                .connection
                .execute("UPDATE plan_step SET instruction_id=?1", [held.id])
                .unwrap();
        }
        let context = serde_json::to_string(plan_effect).unwrap();
        let effect = serde_json::to_string(&effect).unwrap();
        store
            .connection
            .execute(
                "UPDATE content_block SET effect=?1,context_raw=?2 WHERE id=?3",
                params![effect, context, source],
            )
            .unwrap();
        assert!(
            verify_image_plan(
                &store.connection,
                &store.context.seed_id,
                &store.read_limits
            )
            .is_err(),
            "agreed rows cannot replace authority or allocation checks"
        );
    }
}

#[test]
fn reviewed_close_refuses_mismatched_setup_work_and_nonresult_outcome() {
    for defect in 0..3 {
        let mut store = store();
        create(&mut store, 1, true, false);
        activate(&mut store, 3, 1, 3, None);
        let work = work_ref(&store, 3);
        let mut send = statement(4, "loop.instruction");
        send.source.channel = "internal".into();
        send.source.origin = "loop".into();
        send.source_path
            .insert(0, SourcePathPart::Key("message".into()));
        store.file(&send).unwrap();
        let mut start = opening(&send);
        start.call_block = 0.into();
        start.lane = PairLane::Review;
        let mut review_setup = setup();
        let mut review_work = work.clone();
        if defect == 0 {
            review_setup.setup_id = "different".into();
        }
        if defect == 1 {
            review_work.content_sha256 = Digest::new("f".repeat(64)).unwrap();
        }
        start.request = PairRequest::Review {
            setup: review_setup,
            work: Box::new(review_work),
            command: "review".into(),
        };
        store
            .file_pair(&pair_file(5, PairRecord::Started(start.clone()), "loop"))
            .unwrap();
        let outcome = if defect == 2 {
            weaver_types::ToolOutcome::Refused {
                reason: "not run".into(),
            }
        } else {
            weaver_types::ToolOutcome::Result {
                content: "passed".into(),
            }
        };
        let ended = PairEnded {
            call_id: start.call_id.clone(),
            execution: Some(weaver_types::ExchangeId {
                opener: weaver_types::Opener::Harness,
                ordinal: 1,
            }),
            fate: TerminalPairFate::Completed,
            reason: None,
            outcome: Some(outcome),
            loaded_from: None,
        };
        store
            .file_pair(&pair_file(6, PairRecord::Ended(ended), "tool"))
            .unwrap();
        unchanged(
            &mut store,
            &effect(
                7,
                PlanEffect::Close {
                    expected: plan(1),
                    step_id: step(1),
                    review_call: Some(start.call_id),
                    work,
                    outcome_span: span(4),
                },
            ),
        );
    }
}

#[test]
fn image_rejects_retained_plan_effects_whose_entire_materialization_was_deleted() {
    let mut store = store();
    create(&mut store, 1, false, false);
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
    store
        .connection
        .execute("DELETE FROM plan_step", [])
        .unwrap();
    store
        .connection
        .execute("DELETE FROM plan_category", [])
        .unwrap();
    store
        .connection
        .execute("DELETE FROM plan_revision", [])
        .unwrap();
    assert!(
        verify_image_plan(
            &store.connection,
            &store.context.seed_id,
            &store.read_limits
        )
        .is_err()
    );
}

fn rewrite_effect_step(store: &ServingSqlite, sequence: u64, old_id: u64, new_id: u64) {
    let source = source_id(&store.connection, &block(sequence, 0))
        .unwrap()
        .unwrap();
    let raw: String = store
        .connection
        .query_row(
            "SELECT effect FROM content_block WHERE id=?1",
            [source],
            |row| row.get(0),
        )
        .unwrap();
    let mut effect: StatementEffect = decode_strict(&raw).unwrap();
    let StatementEffect::Plan(plan_effect) = &mut effect else {
        panic!("plan fixture")
    };
    let steps = match plan_effect.as_mut() {
        PlanEffect::Lock { steps, .. } | PlanEffect::Replace { steps, .. } => steps,
        _ => panic!("lock fixture"),
    };
    steps
        .iter_mut()
        .find(|s| s.step_id.local_id.0 == old_id)
        .unwrap()
        .step_id = step(new_id);
    let context = serde_json::to_string(plan_effect).unwrap();
    let effect = serde_json::to_string(&effect).unwrap();
    store
        .connection
        .execute(
            "UPDATE content_block SET effect=?1,context_raw=?2 WHERE id=?3",
            params![effect, context, source],
        )
        .unwrap();
}

#[test]
fn image_refuses_new_plan_reuse_and_reintroduction_of_a_dropped_step_id() {
    let mut store = store();
    create(&mut store, 1, false, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    close(&mut store, 4, 4, work, None);
    let ctx = context(
        PlanInputOperation::Create {
            task: 0.into(),
            steps: vec![PlanInputStep {
                id: None,
                block: 1.into(),
            }],
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    store.file(&input(5, 0, &ctx)).unwrap();
    store.file(&input(5, 1, &ctx)).unwrap();
    let second = PlanRef {
        plan_id: PlanId {
            seed_id: store.context.seed_id.clone(),
            local_id: 2.into(),
        },
        revision: 1.into(),
    };
    store
        .file(&effect(
            6,
            PlanEffect::Lock {
                plan: second,
                source: PlanLockSource::Operator { input: block(5, 0) },
                task: TaskScopeRef {
                    task: span(5),
                    prompt: span(6),
                },
                steps: vec![PlanStepInit {
                    step_id: step(2),
                    instruction: reference(5, 1),
                }],
                review: setup(),
            },
        ))
        .unwrap();
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
    store
        .connection
        .execute(
            "UPDATE plan_step SET step_id=?1 WHERE plan_id=?2",
            params![blob(1), blob(2)],
        )
        .unwrap();
    rewrite_effect_step(&store, 6, 2, 1);
    assert!(
        verify_image_plan(
            &store.connection,
            &store.context.seed_id,
            &store.read_limits
        )
        .is_err()
    );

    let mut store = super::super::tests::store();
    create(&mut store, 2, false, false);
    for (revision, id, draft_id, sequence) in [(2, 1, Some(step(1)), 3), (3, 3, None, 5)] {
        let ctx = context(
            PlanInputOperation::Replace(PlanInputReplacement {
                task: None,
                steps: vec![PlanInputStep {
                    id: draft_id,
                    block: 0.into(),
                }],
                categories: categories(false),
                groups: vec![],
                review: setup(),
                preserved_closures: vec![],
            }),
            Some(plan(revision - 1)),
        );
        store.file(&input(sequence, 0, &ctx)).unwrap();
        store
            .file(&effect(
                sequence + 1,
                PlanEffect::Replace {
                    expected: plan(revision - 1),
                    next: plan(revision),
                    source: block(sequence, 0),
                    task: TaskScopeRef {
                        task: span(1),
                        prompt: span(2),
                    },
                    steps: vec![PlanStepInit {
                        step_id: step(id),
                        instruction: reference(sequence, 0),
                    }],
                    review: setup(),
                },
            ))
            .unwrap();
    }
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
    store
        .connection
        .execute(
            "UPDATE plan_step SET step_id=?1 WHERE revision=?2",
            params![blob(2), blob(3)],
        )
        .unwrap();
    rewrite_effect_step(&store, 6, 3, 2);
    assert!(
        verify_image_plan(
            &store.connection,
            &store.context.seed_id,
            &store.read_limits
        )
        .is_err()
    );
}

#[test]
fn image_refuses_contiguous_plan_ids_swapped_across_all_rows_and_effects() {
    let mut store = store();
    create(&mut store, 1, false, false);
    store
        .file(&effect(
            3,
            PlanEffect::Break {
                expected: plan(1),
                step_id: None,
                case: PlanBreakCase::WrongCategory,
                proposed_source: None,
            },
        ))
        .unwrap();
    let ctx = context(
        PlanInputOperation::Create {
            task: 0.into(),
            steps: vec![PlanInputStep {
                id: None,
                block: 1.into(),
            }],
            categories: categories(false),
            groups: vec![],
            review: setup(),
        },
        None,
    );
    store.file(&input(4, 0, &ctx)).unwrap();
    store.file(&input(4, 1, &ctx)).unwrap();
    let second = PlanRef {
        plan_id: PlanId {
            seed_id: store.context.seed_id.clone(),
            local_id: 2.into(),
        },
        revision: 1.into(),
    };
    store
        .file(&effect(
            5,
            PlanEffect::Lock {
                plan: second,
                source: PlanLockSource::Operator { input: block(4, 0) },
                task: TaskScopeRef {
                    task: span(3),
                    prompt: span(4),
                },
                steps: vec![PlanStepInit {
                    step_id: step(2),
                    instruction: reference(4, 1),
                }],
                review: setup(),
            },
        ))
        .unwrap();
    verify_image_plan(
        &store.connection,
        &store.context.seed_id,
        &store.read_limits,
    )
    .unwrap();
    let tx = store.connection.transaction().unwrap();
    tx.pragma_update(None, "defer_foreign_keys", true).unwrap();
    for table in [
        "plan_revision",
        "plan_category",
        "plan_step",
        "plan_group",
        "plan_group_step",
    ] {
        for (from, to) in [(1, 3), (2, 1), (3, 2)] {
            tx.execute(
                &format!("UPDATE {table} SET plan_id=?1 WHERE plan_id=?2"),
                params![blob(to), blob(from)],
            )
            .unwrap();
        }
    }
    for sequence in [2, 3, 5] {
        let source = source_id(&tx, &block(sequence, 0)).unwrap().unwrap();
        let raw: String = tx
            .query_row(
                "SELECT effect FROM content_block WHERE id=?1",
                [source],
                |row| row.get(0),
            )
            .unwrap();
        let mut effect: StatementEffect = decode_strict(&raw).unwrap();
        let StatementEffect::Plan(plan_effect) = &mut effect else {
            panic!("plan fixture")
        };
        let plan = match plan_effect.as_mut() {
            PlanEffect::Lock { plan, .. } => plan,
            PlanEffect::Break { expected, .. } => expected,
            _ => panic!("fixture effect"),
        };
        plan.plan_id.local_id = (3 - plan.plan_id.local_id.0).into();
        let context = serde_json::to_string(plan_effect).unwrap();
        let effect = serde_json::to_string(&effect).unwrap();
        tx.execute(
            "UPDATE content_block SET effect=?1,context_raw=?2 WHERE id=?3",
            params![effect, context, source],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert!(
        verify_image_plan(
            &store.connection,
            &store.context.seed_id,
            &store.read_limits
        )
        .is_err()
    );
}

#[test]
fn close_files_at_the_last_allocatable_span_and_the_next_marks_read_refuses_counter_exhausted() {
    // FL1 (#418): the span counter's last value is allocatable. With the
    // held maximum at u64::MAX - 1 the Marks read offers u64::MAX, close
    // files its outcome there, and only the following Marks read refuses.
    // The raw row is no store a run could reach (allocation is consecutive
    // from the seed), so this pins the filing path's counter arithmetic
    // only; the image check is not consulted.
    let mut store = store();
    create(&mut store, 1, false, false);
    activate(&mut store, 3, 1, 3, None);
    let work = work_ref(&store, 3);
    // A closed outcome under the active step takes the counter to its
    // second-to-last value.
    store
        .connection
        .execute(
            "INSERT INTO span(local_id,revision,kind,status,opened_content_id,ended_content_id,parent_local_id,parent_revision) VALUES (?1,X'0000000000000001','step_outcome','closed',(SELECT MIN(id) FROM content_block),(SELECT MIN(id) FROM content_block),?2,X'0000000000000001')",
            [blob(u64::MAX - 1), blob(3)],
        )
        .unwrap();
    let marks =
        |store: &ServingSqlite| store.query(&weaver_types::state_flow::ReadSelector::Marks {});
    assert!(matches!(
        marks(&store),
        Ok(QueryResult::Marks { next_span_id, .. }) if next_span_id.0 == u64::MAX
    ));
    close(&mut store, 4, u64::MAX, work, None);
    let closed = snapshot(&store);
    assert_eq!(closed.steps[0].outcome_span, Some(span(u64::MAX)));
    assert_eq!(closed.status, PlanStatus::Complete);
    assert!(matches!(
        marks(&store),
        Err(crate::ReadFault::Refused(ReadRefusalCode::CounterExhausted))
    ));
}
