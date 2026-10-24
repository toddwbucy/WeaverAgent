use super::super::{ServingCatalog, ServingContext};
use super::*;

fn test_store() -> ServingSqlite {
    let catalog = ServingCatalog::create().unwrap();
    catalog
        .connection
        .execute(
            "INSERT INTO store_meta VALUES(1,'state-content/1','seed','scheme')",
            [],
        )
        .unwrap();
    catalog.connection.execute("INSERT INTO content_block(source_kind,seed_ordinal,admission,raw_block,statement_text,scheme_id,content_type) VALUES('seed',?1,'seeded',?2,'Read file.','scheme','statement')",params![blob(0.into()),r#"{"type":"text","text":"Read file."}"#]).unwrap();
    let seed = catalog.connection.last_insert_rowid();
    binding(&catalog.connection, seed, 0, "read", "file", None);
    catalog.connection.execute("INSERT INTO span(local_id,revision,kind,status) VALUES(?1,?2,'system_prompt','closed')",params![blob(0.into()),blob(1.into())]).unwrap();
    catalog.connection.execute("INSERT INTO span_member(local_id,revision,ordinal,block_id,start,end) VALUES(?1,?2,?1,?3,?1,?4)",params![blob(0.into()),blob(1.into()),seed,blob(10.into())]).unwrap();
    ServingSqlite {
        connection: catalog.connection,
        context: ServingContext {
            session_id: "session".into(),
            run_id: "run".into(),
            scheme_id: "scheme".into(),
            seed_id: "seed".into(),
            start: StartIdentity {
                seed_id: "seed".into(),
                artifact_id: "artifact".into(),
                dump_sha256: Digest::new("0".repeat(64)).unwrap(),
                config_sha256: Digest::new("0".repeat(64)).unwrap(),
                parent_artifact_id: None,
                position: None,
            },
        },
        read_limits: ReadLimits {
            max_result_bytes: 1_000_000.into(),
            measurement_ref: "fixture".into(),
        },
        filing_hooks: None,
    }
}
fn event(db: &Connection, run: &str, sequence: u64, turn: Option<&str>, kind: &str) -> i64 {
    let source = SourceEnvelope {
        session: "session".into(),
        run: run.into(),
        turn: turn.map(Into::into),
        sequence: sequence.into(),
        kind: kind.into(),
        channel: "output".into(),
        origin: "model".into(),
        subsystem: "harness".into(),
        causal_parent: None,
        wall_ms: 0,
        monotonic_ns: sequence.into(),
    };
    db.execute("INSERT INTO content_block(source_kind,run,sequence,block,session,turn,kind,channel,origin,envelope,rule_id,source_path,admission,raw_block,statement_text,scheme_id,content_type) VALUES('event',?1,?2,?3,'session',?4,?5,'output','model',?6,'statements',?7,'admitted',?8,'Read file.','scheme','statement')",params![run,blob(sequence.into()),blob(0.into()),turn,kind,serde_json::to_string(&source).unwrap(),r#"["content",{"index":"0"}]"#,r#"{ "type":"text","text":"Read \u0066ile." }"#]).unwrap();
    db.last_insert_rowid()
}
fn binding(
    db: &Connection,
    block: i64,
    ordinal: u64,
    verb: &str,
    noun: &str,
    condition: Option<&str>,
) -> i64 {
    db.execute("INSERT INTO statement_binding(block_id,ordinal,start,end,verb,verb_start,verb_end,condition,condition_start,condition_end) VALUES(?1,?2,?3,?4,?5,?3,?6,?7,?8,?9)",params![block,blob(ordinal.into()),blob(0.into()),blob(10.into()),verb,blob(4.into()),condition,condition.map(|_|blob(0.into())),condition.map(|_|blob(1.into()))]).unwrap();
    let id = db.last_insert_rowid();
    db.execute(
        "INSERT INTO binding_noun VALUES(?1,?2,?3,?4,?5)",
        params![id, blob(0.into()), noun, blob(5.into()), blob(9.into())],
    )
    .unwrap();
    id
}
fn entries(result: QueryResult) -> Vec<ReadEntry> {
    match result {
        QueryResult::Entries { entries } => entries,
        _ => panic!("wrong result"),
    }
}
fn refused(result: Result<QueryResult>, expected: ReadRefusalCode) {
    assert!(matches!(result,Err(ReadFailure::Refused(code)) if code==expected));
}

#[test]
fn history_and_subjects_keep_binary_keys_original_ordinals_and_whole_blocks() {
    let store = test_store();
    let db = &store.connection;
    let first = event(db, "z-run", 1, Some("1"), "message.assistant");
    binding(db, first, 0, "read", "File", None);
    binding(db, first, 1, "write", "file", Some("ready"));
    let second = event(db, "a-run", 1, Some("1"), "message.user");
    binding(db, second, 0, "read", "file", None);
    let hidden = event(db, "a-run", 2, Some("1"), "loop.instruction");
    binding(db, hidden, 0, "read", "file", None);
    let exact_case = entries(
        query(
            &store,
            &ReadSelector::History {
                noun: "File".into(),
                verb: None,
                condition: None,
            },
        )
        .unwrap(),
    );
    assert_eq!(exact_case.len(), 1);
    assert_eq!(exact_case[0].bindings[0].ordinal.0, 0);
    let result = entries(
        query(
            &store,
            &ReadSelector::History {
                noun: "file".into(),
                verb: Some("write".into()),
                condition: Some("ready".into()),
            },
        )
        .unwrap(),
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].bindings[0].ordinal.0, 1);
    let condition = entries(
        query(
            &store,
            &ReadSelector::Condition {
                condition: "ready".into(),
            },
        )
        .unwrap(),
    );
    assert_eq!(condition.len(), 1);
    assert_eq!(condition[0].bindings[0].ordinal.0, 1);
    assert!(result[0].raw_block.get().contains("\\u0066"));
    let whole = entries(
        query(
            &store,
            &ReadSelector::Subjects {
                nouns: vec!["File".into(), "file".into()],
            },
        )
        .unwrap(),
    );
    assert_eq!(whole.len(), 2);
    assert_eq!(whole[0].bindings.len(), 2);
    assert_eq!(whole[1].bindings.len(), 1);
    assert!(matches!(&whole[0].source,ReadSource::Event(source) if source.block_id.run=="z-run"));
    let selected = StatementRef {
        block_id: SourceBlockId {
            run: "z-run".into(),
            sequence: 1.into(),
            block: 0.into(),
        },
        binding: 1.into(),
    };
    let result = entries(
        query(
            &store,
            &ReadSelector::Statement {
                statement: selected,
            },
        )
        .unwrap(),
    );
    assert_eq!(result[0].bindings[0].ordinal.0, 1);
    refused(
        query(
            &store,
            &ReadSelector::Subjects {
                nouns: vec!["file".into(), "file".into()],
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
}

#[test]
fn conversation_uses_commit_order_and_u64_limit_without_signed_wrap() {
    let store = test_store();
    let db = &store.connection;
    for (run, seq, turn) in [("z", 1, Some("1")), ("a", 1, Some("1")), ("a", 2, None)] {
        let id = event(db, run, seq, turn, "message.user");
        binding(db, id, 0, "read", "file", None);
    }
    let latest = entries(
        query(
            &store,
            &ReadSelector::Conversation {
                last_turns: 1.into(),
            },
        )
        .unwrap(),
    );
    assert_eq!(latest.len(), 1);
    assert!(matches!(&latest[0].source,ReadSource::Event(source) if source.block_id.run=="a"));
    assert_eq!(
        entries(
            query(
                &store,
                &ReadSelector::Conversation {
                    last_turns: u64::MAX.into()
                }
            )
            .unwrap()
        )
        .len(),
        2
    );
    refused(
        query(
            &store,
            &ReadSelector::Conversation {
                last_turns: 0.into(),
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
}

#[test]
fn complete_result_limit_is_exact_and_never_omits_an_entry() {
    let mut store = test_store();
    let selector = ReadSelector::SystemPrompt {};
    let expected = query_encoded(&store, &selector, None).unwrap();
    store.read_limits.max_result_bytes = (expected.len() as u64).into();
    assert_eq!(query_encoded(&store, &selector, None).unwrap(), expected);
    store.read_limits.max_result_bytes = (expected.len() as u64 - 1).into();
    refused(query(&store, &selector), ReadRefusalCode::ResultTooLarge);
    store.read_limits.max_result_bytes = 1_000_000.into();
    assert_eq!(query_encoded(&store, &selector, None).unwrap(), expected);
    assert_eq!(held(&store.connection).unwrap().0, 1);
    assert!(position(&store.connection).unwrap().is_none());
}

#[test]
fn seed_and_empty_plan_queries_have_strict_results_and_no_writes() {
    let store = test_store();
    let before = store.connection.total_changes();
    let prompt = entries(query(&store, &ReadSelector::SystemPrompt {}).unwrap());
    assert_eq!(prompt.len(), 1);
    assert!(matches!(prompt[0].source, ReadSource::Seed(_)));
    assert!(prompt[0].selected_span.is_some());
    assert!(
        matches!(query(&store,&ReadSelector::Marks{}).unwrap(),QueryResult::Marks{next_span_id,current_task:None,open_scopes} if next_span_id.0==1 && open_scopes.is_empty())
    );
    assert!(
        matches!(query(&store,&ReadSelector::PlanCurrent{}).unwrap(),QueryResult::PlanCurrent{current:None,next_plan_id,next_step_id} if next_plan_id.0==1 && next_step_id.0==1)
    );
    assert!(
        matches!(query(&store,&ReadSelector::OpenPairs{}).unwrap(),QueryResult::OpenPairs{pairs} if pairs.is_empty())
    );
    refused(
        query(
            &store,
            &ReadSelector::Span {
                seed_id: "other".into(),
                local_id: 0.into(),
                revision: 1.into(),
            },
        ),
        ReadRefusalCode::WrongSeed,
    );
    assert_eq!(store.connection.total_changes(), before);
}

fn plan_fixture(store: &ServingSqlite) -> (PlanRef, SourceBlockId) {
    let db = &store.connection;
    let operator = event(db, "run", 9, None, "plan.event");
    let source = event(db, "run", 10, Some("1"), "message.user");
    let instruction = binding(db, source, 0, "read", "file", None);
    db.execute("INSERT INTO span(local_id,revision,kind,status,opened_content_id) VALUES(?1,?2,'task','open',?3)",params![blob(1.into()),blob(1.into()),source]).unwrap();
    db.execute("INSERT INTO span(local_id,revision,kind,parent_local_id,parent_revision,status,opened_content_id,ended_content_id) VALUES(?1,?2,'task_prompt',?3,?2,'closed',?4,?4)",params![blob(2.into()),blob(1.into()),blob(1.into()),source]).unwrap();
    db.execute("INSERT INTO span_member(local_id,revision,ordinal,block_id,start,end) VALUES(?1,?2,?3,?4,?3,?5)",params![blob(2.into()),blob(1.into()),blob(0.into()),source,blob(10.into())]).unwrap();
    let review = ReviewSetupRef {
        setup_id: "review".into(),
        sha256: Digest::new("1".repeat(64)).unwrap(),
    };
    db.execute("INSERT INTO plan_revision(plan_id,revision,task_id,task_revision,prompt_id,prompt_revision,status,operator_source,lock_source,last_source,review_setup,group_drafts) VALUES(?1,?1,?1,?1,?2,?1,'locked',?3,?4,?4,?5,?6)",params![blob(1.into()),blob(2.into()),operator,source,serde_json::to_string(&review).unwrap(),r#"[{"id":"group","step_indices":["0","1"]}]"#]).unwrap();
    let category = Category {
        id: "work".into(),
        verbs: vec!["read".into()],
        acts: vec![PlanAct::Statement],
        review_required: false,
    };
    // This selector fixture supplies the held operator context required by the
    // snapshot carrier. Full image/provenance tests use the actual filing path.
    let context = PlanInputContext {
        document_sha256: Digest::new("2".repeat(64)).unwrap(),
        cause: weaver_types::Cause { uid: 0 },
        expected: None,
        operation: PlanInputOperation::Create {
            task: 0.into(),
            steps: vec![],
            categories: vec![category.clone()],
            groups: vec![],
            review: review.clone(),
        },
    };
    let operator_envelope = SourceEnvelope {
        session: "session".into(),
        run: "run".into(),
        turn: None,
        sequence: 9.into(),
        kind: "plan.event".into(),
        channel: "input".into(),
        origin: "operator".into(),
        subsystem: "harness".into(),
        causal_parent: None,
        wall_ms: 0,
        monotonic_ns: 9.into(),
    };
    db.execute(
        "UPDATE content_block SET channel='input',origin='operator',envelope=?1,source_path=?2,context_path=?3,context_raw=?4 WHERE id=?5",
        params![
            serde_json::to_string(&operator_envelope).unwrap(),
            r#"["message","content",{"index":"0"}]"#,
            r#"["context"]"#,
            serde_json::to_string(&context).unwrap(),
            operator,
        ],
    ).unwrap();
    db.execute(
        "INSERT INTO plan_category VALUES(?1,?1,'work',?2,?3)",
        params![
            blob(1.into()),
            blob(0.into()),
            serde_json::to_string(&category).unwrap()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO plan_group VALUES(?1,?1,'group',?2,NULL,NULL)",
        params![blob(1.into()), blob(0.into())],
    )
    .unwrap();
    for id in 1..=2 {
        db.execute("INSERT INTO plan_step(plan_id,revision,step_id,ordinal,instruction_id,category_id,status) VALUES(?1,?1,?2,?2,?3,'work','waiting')",params![blob(1.into()),blob(id.into()),instruction]).unwrap();
        db.execute(
            "INSERT INTO plan_group_step VALUES(?1,?1,'group',?2)",
            params![blob(1.into()), blob(id.into())],
        )
        .unwrap();
    }
    (
        PlanRef {
            plan_id: PlanId {
                seed_id: "seed".into(),
                local_id: 1.into(),
            },
            revision: 1.into(),
        },
        SourceBlockId {
            run: "run".into(),
            sequence: 10.into(),
            block: 0.into(),
        },
    )
}
fn close_fixture_step(store: &ServingSqlite, plan: &PlanRef, step: u64, source: &SourceBlockId) {
    let db = &store.connection;
    let source_id = resolve_source(db, source).unwrap().unwrap();
    let span = 10 + step * 2;
    let outcome = span + 1;
    for (id, parent, kind) in [(span, 1, "step"), (outcome, span, "step_outcome")] {
        db.execute("INSERT INTO span(local_id,revision,kind,parent_local_id,parent_revision,status,opened_content_id,ended_content_id) VALUES(?1,?2,?3,?4,?2,'closed',?5,?5)",params![blob(id.into()),blob(1.into()),kind,blob(parent.into()),source_id]).unwrap();
    }
    let work = WorkRef {
        plan: plan.clone(),
        step_id: StepId {
            seed_id: "seed".into(),
            local_id: step.into(),
        },
        span: SpanRef {
            seed_id: "seed".into(),
            local_id: span.into(),
            revision: 1.into(),
        },
        through: SourcePosition {
            block_id: source.clone(),
            turn: Some("1".into()),
        },
        content_sha256: Digest::new("2".repeat(64)).unwrap(),
    };
    db.execute("UPDATE plan_step SET status='closed',span_id=?1,span_revision=?2,outcome_id=?3,outcome_revision=?2,work=?4,closure_source=?5 WHERE step_id=?6",params![blob(span.into()),blob(1.into()),blob(outcome.into()),serde_json::to_string(&work).unwrap(),source_id,blob(step.into())]).unwrap();
}
#[test]
fn plan_queries_keep_groups_and_find_waiting_steps_before_preserved_closures() {
    let store = test_store();
    let (plan, source) = plan_fixture(&store);
    let snapshot = plan_snapshot(&store.connection, "seed", &plan).unwrap();
    assert_eq!(snapshot.steps.len(), 2);
    assert_eq!(snapshot.groups[0].step_ids[1].local_id.0, 2);
    close_fixture_step(&store, &plan, 2, &source);
    let (reference, instruction) =
        step_successor(&store.connection, "seed", &plan, None, 1_000_000.into()).unwrap();
    assert_eq!(reference.step_id.local_id.0, 1);
    assert!(reference.previous_step.is_none());
    assert_eq!(instruction.step_query.as_ref(), Some(&reference));
    refused(
        query(
            &store,
            &ReadSelector::StepSuccessor {
                plan: plan.clone(),
                previous_step: Some(StepId {
                    seed_id: "seed".into(),
                    local_id: 2.into(),
                }),
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
    assert!(
        matches!(query(&store,&ReadSelector::PlanCurrent{}).unwrap(),QueryResult::PlanCurrent{next_plan_id,next_step_id,..} if next_plan_id.0==2&&next_step_id.0==3)
    );
    close_fixture_step(&store, &plan, 1, &source);
    refused(
        query(
            &store,
            &ReadSelector::StepSuccessor {
                plan,
                previous_step: None,
            },
        ),
        ReadRefusalCode::NotFound,
    );
}
#[test]
fn successor_checks_predecessor_active_step_and_unsigned_allocations() {
    let store = test_store();
    let (plan, source) = plan_fixture(&store);
    close_fixture_step(&store, &plan, 1, &source);
    refused(
        query(
            &store,
            &ReadSelector::StepSuccessor {
                plan: plan.clone(),
                previous_step: None,
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
    let (previous, _) = step_successor(
        &store.connection,
        "seed",
        &plan,
        Some(&StepId {
            seed_id: "seed".into(),
            local_id: 1.into(),
        }),
        1_000_000.into(),
    )
    .unwrap();
    assert_eq!(previous.step_id.local_id.0, 2);
    let mut wrong = plan.clone();
    wrong.revision = 2.into();
    refused(
        query(
            &store,
            &ReadSelector::StepSuccessor {
                plan: wrong,
                previous_step: None,
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
    store
        .connection
        .execute(
            "UPDATE plan_step SET status='active',span_id=?1,span_revision=?2 WHERE step_id=?3",
            params![blob(12.into()), blob(1.into()), blob(2.into())],
        )
        .unwrap();
    refused(
        query(
            &store,
            &ReadSelector::StepSuccessor {
                plan,
                previous_step: Some(StepId {
                    seed_id: "seed".into(),
                    local_id: 1.into(),
                }),
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
}
#[test]
fn flag_counts_use_canonical_addresses_once_per_binding_and_fixed_frontier() {
    let store = test_store();
    let (plan, source) = plan_fixture(&store);
    for (i, condition) in [
        "step:1",
        "step:2",
        "step:3",
        "step:01",
        "Step:1",
        "step:0",
        "step:18446744073709551615",
        "step:18446744073709551616",
        "step:1\0",
    ]
    .iter()
    .enumerate()
    {
        let id = event(
            &store.connection,
            "run",
            20 + i as u64,
            Some("2"),
            "message.assistant",
        );
        let bind = binding(&store.connection, id, 0, "read", "file", Some(condition));
        store
            .connection
            .execute(
                "INSERT INTO binding_noun VALUES(?1,?2,'extra',?3,?4)",
                params![bind, blob(1.into()), blob(5.into()), blob(9.into())],
            )
            .unwrap();
    }
    let result = query(
        &store,
        &ReadSelector::PlanFlags {
            plan: plan.clone(),
            after: source.clone(),
        },
    )
    .unwrap();
    match result {
        QueryResult::PlanFlags { snapshot } => {
            assert_eq!(snapshot.total_flags.0, 4);
            assert_eq!(snapshot.matched_flags.0, 2);
            assert_eq!(snapshot.unmatched_flags.0, 2);
            assert_eq!(snapshot.through.block_id.sequence.0, 28);
        }
        _ => panic!("wrong result"),
    }
    let absent = SourceBlockId {
        sequence: 999.into(),
        ..source.clone()
    };
    refused(
        query(
            &store,
            &ReadSelector::PlanFlags {
                plan: plan.clone(),
                after: absent,
            },
        ),
        ReadRefusalCode::NotFound,
    );
    let stale = SourceBlockId {
        sequence: 20.into(),
        ..source
    };
    refused(
        query(&store, &ReadSelector::PlanFlags { plan, after: stale }),
        ReadRefusalCode::InvalidSelector,
    );
}
#[test]
fn preview_is_canonical_read_only_and_checks_exact_through_turn() {
    let store = test_store();
    let (plan, source) = plan_fixture(&store);
    let span = SpanRef {
        seed_id: plan.plan_id.seed_id,
        local_id: 1.into(),
        revision: 1.into(),
    };
    let id = event(&store.connection, "run", 11, Some("1"), "message.assistant");
    binding(&store.connection, id, 0, "read", "file", None);
    let through = SourcePosition {
        block_id: SourceBlockId {
            sequence: 11.into(),
            ..source
        },
        turn: Some("1".into()),
    };
    let before = store.connection.total_changes();
    let bytes =
        span_preview_encoded(&store.connection, "seed", &span, &through, 1_000_000.into()).unwrap();
    let decoded: QueryResult = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&decoded).unwrap());
    assert_eq!(store.connection.total_changes(), before);
    assert!(matches!(decoded,QueryResult::SpanPreview{entries,..} if entries.len()==1));
    let wrong = SourcePosition {
        turn: None,
        ..through.clone()
    };
    refused(
        query(
            &store,
            &ReadSelector::SpanPreview {
                span: span.clone(),
                through: wrong,
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
    refused(
        query(
            &store,
            &ReadSelector::Span {
                seed_id: "seed".into(),
                local_id: span.local_id,
                revision: span.revision,
            },
        ),
        ReadRefusalCode::SpanOpen,
    );
    store
        .connection
        .execute(
            "UPDATE span SET status='closed',ended_content_id=?1 WHERE local_id=?2 AND revision=?3",
            params![id, blob(span.local_id), blob(span.revision)],
        )
        .unwrap();
    assert_eq!(
        historical_span_preview_encoded(&store.connection, "seed", &span, &through).unwrap(),
        bytes,
    );
    refused(
        query(
            &store,
            &ReadSelector::SpanPreview {
                span: span.clone(),
                through: through.clone(),
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
    let later = event(&store.connection, "run", 12, Some("1"), "message.assistant");
    binding(&store.connection, later, 0, "read", "file", None);
    let after_end = SourcePosition {
        block_id: SourceBlockId {
            sequence: 12.into(),
            ..through.block_id
        },
        turn: through.turn,
    };
    assert!(matches!(
        historical_span_preview_encoded(&store.connection, "seed", &span, &after_end),
        Err(ReadFailure::Refused(ReadRefusalCode::InvalidSelector)),
    ));
}

fn pair_fixture(store: &ServingSqlite) -> (StatementRef, CallId, i64, i64) {
    let db = &store.connection;
    let request = event(db, "run", 40, Some("4"), "message.assistant");
    let binding_id = binding(db, request, 0, "read", "file", None);
    let statement = StatementRef {
        block_id: SourceBlockId {
            run: "run".into(),
            sequence: 40.into(),
            block: 0.into(),
        },
        binding: 0.into(),
    };
    let call_id = CallId {
        run: "run".into(),
        sequence: 40.into(),
        call: 0.into(),
    };
    let opening = PairStarted {
        call_id: call_id.clone(),
        call_block: 1.into(),
        statement: statement.clone(),
        lane: PairLane::Recall,
        request: PairRequest::Recall {
            subject: "file".into(),
        },
        retry_of: None,
    };
    let open = event(db, "run", 41, Some("4"), "tool.call.started");
    db.execute("UPDATE content_block SET content_type='pair',statement_text=NULL,source_path='[]',raw_block=?1 WHERE id=?2",params![serde_json::to_string(&opening).unwrap(),open]).unwrap();
    db.execute("INSERT INTO pair_event VALUES(?1,'started')", [open])
        .unwrap();
    db.execute("INSERT INTO tool_pair(run,sequence,call,call_block,statement_id,opened_event,lane,request,fate) VALUES('run',?1,?2,?3,?4,?5,'recall',?6,'opened')",params![blob(40.into()),blob(0.into()),blob(1.into()),binding_id,open,serde_json::to_string(&opening.request).unwrap()]).unwrap();
    (statement, call_id, request, open)
}
fn end_pair(store: &ServingSqlite, call: &CallId) -> i64 {
    let db = &store.connection;
    let reason = PairFailure::NotDispatched {
        cause: NonDispatchCause::NoGate,
    };
    let ending = PairEnded {
        call_id: call.clone(),
        execution: None,
        fate: TerminalPairFate::Failed,
        reason: Some(reason.clone()),
        outcome: None,
        loaded_from: None,
    };
    let end = event(db, "run", 42, Some("4"), "tool.call.ended");
    db.execute("UPDATE content_block SET content_type='pair',statement_text=NULL,source_path='[]',raw_block=?1 WHERE id=?2",params![serde_json::to_string(&ending).unwrap(),end]).unwrap();
    db.execute("INSERT INTO pair_event VALUES(?1,'ended')", [end])
        .unwrap();
    db.execute("UPDATE tool_pair SET fate='failed',reason=?1,terminal_event=?2 WHERE run=?3 AND sequence=?4 AND call=?5",params![serde_json::to_string(&reason).unwrap(),end,call.run,blob(call.sequence),blob(call.call)]).unwrap();
    end
}
#[test]
fn pair_reads_attach_whole_current_outcomes_and_frontiers_refuse_pending_or_partial() {
    let store = test_store();
    let (statement, call, request, open) = pair_fixture(&store);
    assert!(
        matches!(query(&store,&ReadSelector::OpenPairs{}).unwrap(),QueryResult::OpenPairs{pairs} if pairs.len()==1&&pairs[0].terminal_source.is_none())
    );
    let end = end_pair(&store, &call);
    let entry = entries(query(&store, &ReadSelector::Statement { statement }).unwrap()).remove(0);
    assert_eq!(entry.pairs.len(), 1);
    assert_eq!(entry.pairs[0].fate, PairFate::Failed);
    assert!(
        entry.pairs[0]
            .terminal_source
            .as_ref()
            .unwrap()
            .raw_block
            .get()
            .contains("not_dispatched")
    );
    assert!(
        matches!(query(&store,&ReadSelector::OpenPairs{}).unwrap(),QueryResult::OpenPairs{pairs} if pairs.is_empty())
    );
    let db = &store.connection;
    db.execute("INSERT INTO span(local_id,revision,kind,status,opened_content_id,ended_content_id) VALUES(?1,?2,'task','closed',?3,?4)",params![blob(1.into()),blob(1.into()),request,open]).unwrap();
    db.execute("INSERT INTO span_member(local_id,revision,ordinal,block_id,start,end) VALUES(?1,?2,?3,?4,?3,?5)",params![blob(1.into()),blob(1.into()),blob(0.into()),request,blob(10.into())]).unwrap();
    let selector = ReadSelector::Span {
        seed_id: "seed".into(),
        local_id: 1.into(),
        revision: 1.into(),
    };
    refused(query(&store, &selector), ReadRefusalCode::SpanPending);
    db.execute(
        "UPDATE span SET ended_content_id=?1 WHERE local_id=?2",
        params![end, blob(1.into())],
    )
    .unwrap();
    assert_eq!(entries(query(&store, &selector).unwrap())[0].pairs.len(), 1);
    db.execute(
        "UPDATE span_member SET start=?1 WHERE local_id=?2",
        params![blob(1.into()), blob(1.into())],
    )
    .unwrap();
    refused(query(&store, &selector), ReadRefusalCode::PartialPair);
    db.execute(
        "UPDATE span_member SET start=?1 WHERE local_id=?2",
        params![blob(0.into()), blob(1.into())],
    )
    .unwrap();
    db.execute(
        "UPDATE span SET ended_content_id=?1 WHERE local_id=?2",
        params![request, blob(1.into())],
    )
    .unwrap();
    assert!(
        entries(query(&store, &selector).unwrap())[0]
            .pairs
            .is_empty()
    );
    assert_eq!(position(db).unwrap().unwrap().block_id.sequence.0, 42);
    assert_eq!(held(db).unwrap().0, 4);
}
fn stored_observation(request: u64, id: u64) -> StateOperationObservation {
    StateOperationObservation {
        operation_id: id.into(),
        parent_id: None,
        request_id: request.into(),
        operation: StateOperation::Read,
        began_ns: 10.into(),
        ended_ns: 20.into(),
        phases: vec![
            OperationPhase {
                name: PhaseName::Validate,
                began_ns: 10.into(),
                ended_ns: 11.into(),
            },
            OperationPhase {
                name: PhaseName::Encode,
                began_ns: 19.into(),
                ended_ns: 20.into(),
            },
        ],
        resource: OperationResource {
            cpu_ns: None,
            copied_bytes: Some(0.into()),
            held_bytes: Some(0.into()),
            unavailable: vec![ResourceUnavailable {
                field: ResourceField::CpuNs,
                reason: ObservationUnavailable::NotSampled,
            }],
        },
        disk: OperationDisk {
            enabled: None,
            before: None,
            after: None,
            unavailable_reason: Some(ObservationUnavailable::NotSampled),
        },
    }
}
#[test]
fn observation_reads_pin_frontier_validate_columns_and_stay_out_of_content() {
    let store = test_store();
    let db = &store.connection;
    let member = MemberObservationIdentity {
        instrument_version: "state-observation/1".into(),
        pid: 1,
        start_ticks: None,
        ticks_per_second: None,
        unavailable_reason: Some(ObservationUnavailable::NotSampled),
    };
    db.execute(
        "INSERT INTO observation_run VALUES('history',?1)",
        [serde_json::to_string(&member).unwrap()],
    )
    .unwrap();
    for id in [1, 2, u64::MAX] {
        let mut observation = stored_observation(1, id);
        let operation = if id == 1 {
            observation.operation = StateOperation::Open;
            "open"
        } else {
            "read"
        };
        db.execute("INSERT INTO operation_observation(run,operation_id,request_id,operation,observation) VALUES('history',?1,?2,?3,?4)",params![blob(id.into()),blob(1.into()),operation,serde_json::to_string(&observation).unwrap()]).unwrap();
    }
    let selector = ReadSelector::Observations {
        run: "history".into(),
        after: 1.into(),
        through: None,
    };
    let held = query_encoded(&store, &selector, None).unwrap();
    // This synthetic read corpus pins unsigned ordering up to u64::MAX. Its
    // intentional gap cannot be adopted as a complete recorded member run.
    assert!(matches!(
        verify_image_observations(db),
        Err(ReadFailure::ComponentCorruption(_))
    ));
    let result: QueryResult = serde_json::from_slice(&held).unwrap();
    assert!(
        matches!(result,QueryResult::Observations{through,records,..} if through.0==u64::MAX && records.len()==2&&records[0].observation.operation_id.0==2)
    );
    assert_eq!(super::held(db).unwrap().0, 1);
    assert!(position(db).unwrap().is_none());
    assert!(shape(db, 1_000_000.into(), None).unwrap().is_empty());
    refused(
        query(
            &store,
            &ReadSelector::Observations {
                run: "absent".into(),
                after: 0.into(),
                through: None,
            },
        ),
        ReadRefusalCode::NotFound,
    );
    refused(
        query(
            &store,
            &ReadSelector::Observations {
                run: "history".into(),
                after: 2.into(),
                through: Some(1.into()),
            },
        ),
        ReadRefusalCode::InvalidSelector,
    );
    db.execute(
        "UPDATE operation_observation SET request_id=?1 WHERE operation_id=?2",
        params![blob(2.into()), blob(2.into())],
    )
    .unwrap();
    assert!(matches!(
        query(&store, &selector),
        Err(ReadFailure::ComponentCorruption(_))
    ));
    assert!(matches!(
        verify_image_observations(db),
        Err(ReadFailure::ComponentCorruption(_))
    ));
    assert!(serde_json::from_slice::<QueryResult>(&held).is_ok());
}

#[test]
fn flag_sampling_does_not_allocate_ids_or_fail_at_allocation_exhaustion() {
    let store = test_store();
    let (plan, source) = plan_fixture(&store);
    let db = &store.connection;
    let instruction: i64 = db
        .query_row("SELECT instruction_id FROM plan_step LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    db.execute("INSERT INTO plan_step(plan_id,revision,step_id,ordinal,instruction_id,category_id,status) VALUES(?1,?1,?2,?3,?4,'work','waiting')",params![blob(1.into()),blob(u64::MAX.into()),blob(3.into()),instruction]).unwrap();
    refused(
        query(&store, &ReadSelector::PlanCurrent {}),
        ReadRefusalCode::CounterExhausted,
    );
    assert!(
        matches!(query(&store,&ReadSelector::PlanFlags{plan,after:source}).unwrap(),QueryResult::PlanFlags{snapshot} if snapshot.total_flags.0==0)
    );
}

#[test]
fn result_writer_growth_stays_within_its_declared_capacity() {
    let mut out = Output::new(100.into()).unwrap();
    for _ in 0..100 {
        out.raw("a").unwrap();
        assert!(out.bytes.capacity() <= 100);
    }
    assert_eq!(out.bytes.len(), 100);
    assert!(matches!(
        out.raw("a"),
        Err(ReadFailure::Refused(ReadRefusalCode::ResultTooLarge))
    ));
}

#[test]
fn whole_block_pairs_use_indexed_binding_ownership_and_interleaved_open_order() {
    let store = test_store();
    let (statement, _call, request, _open) = pair_fixture(&store);
    let db = &store.connection;
    let second = binding(db, request, 1, "read", "other", None);
    let first: i64 = db
        .query_row(
            "SELECT id FROM statement_binding WHERE block_id=?1 AND ordinal=?2",
            params![request, blob(0.into())],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "INSERT INTO binding_noun VALUES(?1,?2,'also',?3,?4)",
        params![first, blob(1.into()), blob(5.into()), blob(9.into())],
    )
    .unwrap();
    for (call, binding_id, binding_ordinal, sequence) in [(1, second, 1, 42), (2, first, 0, 43)] {
        let record = PairStarted {
            call_id: CallId {
                run: "run".into(),
                sequence: 40.into(),
                call: call.into(),
            },
            call_block: (call + 1).into(),
            statement: StatementRef {
                block_id: statement.block_id.clone(),
                binding: binding_ordinal.into(),
            },
            lane: PairLane::Recall,
            request: PairRequest::Recall {
                subject: "file".into(),
            },
            retry_of: None,
        };
        let opened = event(db, "run", sequence, Some("4"), "tool.call.started");
        db.execute("UPDATE content_block SET content_type='pair',statement_text=NULL,source_path='[]',raw_block=?1 WHERE id=?2",params![serde_json::to_string(&record).unwrap(),opened]).unwrap();
        db.execute("INSERT INTO pair_event VALUES(?1,'started')", [opened])
            .unwrap();
        db.execute("INSERT INTO tool_pair(run,sequence,call,call_block,statement_id,opened_event,lane,request,fate) VALUES('run',?1,?2,?3,?4,?5,'recall',?6,'opened')",params![blob(40.into()),blob(call.into()),blob((call+1).into()),binding_id,opened,serde_json::to_string(&record.request).unwrap()]).unwrap();
    }
    let result = entries(
        query(
            &store,
            &ReadSelector::Subjects {
                nouns: vec!["also".into(), "file".into()],
            },
        )
        .unwrap(),
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].bindings.len(), 2);
    assert_eq!(
        result[0]
            .pairs
            .iter()
            .map(|pair| pair.call_id.call.0)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let sql = format!("EXPLAIN QUERY PLAN {}", content::WHOLE_PAIRS);
    let mut explain = db.prepare(&sql).unwrap();
    let detail = explain
        .query_map(
            params![
                request,
                Option::<i64>::None,
                Option::<Vec<u8>>::None,
                Option::<Vec<u8>>::None,
                Option::<i64>::None
            ],
            |row| row.get::<_, String>(3),
        )
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        detail.iter().any(|line| line.contains("pair_statement")),
        "{detail:?}"
    );
}

#[test]
fn observed_result_encodings_retain_actual_written_prefixes_on_refusal() {
    let mut store = test_store();
    let selector = ReadSelector::SystemPrompt {};
    let mut copied = BufferBytes::default();
    let bytes = query_encoded(&store, &selector, Some(&mut copied)).unwrap();
    assert_eq!(copied.value(), Ok(bytes.len() as u64));
    store.read_limits.max_result_bytes = 32.into();
    let mut refused_bytes = BufferBytes::default();
    assert!(matches!(
        query_encoded(&store, &selector, Some(&mut refused_bytes)),
        Err(ReadFailure::Refused(ReadRefusalCode::ResultTooLarge))
    ));
    assert_eq!(
        refused_bytes.value(),
        Ok(br#"{"entries":{"entries":["#.len() as u64)
    );

    let mut shape_bytes = BufferBytes::default();
    assert_eq!(
        shape(&store.connection, 2.into(), Some(&mut shape_bytes)).unwrap(),
        vec![]
    );
    assert_eq!(shape_bytes.value(), Ok(2));
    let mut shape_refusal = BufferBytes::default();
    assert!(matches!(
        shape(&store.connection, 1.into(), Some(&mut shape_refusal)),
        Err(ReadFailure::Refused(ReadRefusalCode::ResultTooLarge))
    ));
    assert_eq!(shape_refusal.value(), Ok(1));
}
