use super::*;
use rusqlite::{ErrorCode, params};

mod constraints;

fn constraint(result: rusqlite::Result<usize>) {
    let error = result.expect_err("the invalid row must refuse");
    assert_eq!(
        error.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    );
}

fn seed_block(connection: &Connection, ordinal: u64) -> i64 {
    connection
        .execute(
            "INSERT INTO content_block
             (source_kind, seed_ordinal, admission, raw_block, statement_text,
              scheme_id, content_type)
             VALUES ('seed', ?1, 'seeded', ?2, 'Hold state.', 'scheme', 'statement')",
            params![
                ordinal.to_be_bytes(),
                r#"{"type":"text","text":"Hold state."}"#
            ],
        )
        .expect("seed-shaped fixture");
    connection.last_insert_rowid()
}

fn binding(connection: &Connection, block: i64, ordinal: u64) -> i64 {
    connection
        .execute(
            "INSERT INTO statement_binding
             (block_id, ordinal, start, end, verb, verb_start, verb_end)
             VALUES (?1, ?2, ?3, ?4, 'hold', ?3, ?5)",
            params![
                block,
                ordinal.to_be_bytes(),
                0_u64.to_be_bytes(),
                11_u64.to_be_bytes(),
                4_u64.to_be_bytes(),
            ],
        )
        .expect("binding fixture");
    connection.last_insert_rowid()
}

#[test]
fn literal_ddl_is_the_complete_merged_catalog() {
    let specification =
        include_str!("../../../../../docs/crates/state-management/weaver-state-Spec-amendment.md");
    let expected: Vec<_> = specification
        .split("```sql\n")
        .skip(1)
        .filter_map(|block| block.split_once("\n```"))
        .map(|(sql, _)| sql.trim())
        .filter(|sql| sql.starts_with("CREATE "))
        .collect();
    assert_eq!(expected.len(), 27);
    assert_eq!(schema::DDL.as_slice(), expected);
    assert!(schema::DDL.iter().all(|sql| !sql.contains(';')));
}

#[test]
fn creates_every_table_and_index_but_no_seed_or_legacy_table() {
    let catalog = ServingCatalog::create().unwrap();
    let objects = catalog_objects(&catalog.connection).unwrap();
    let named: Vec<_> = objects
        .iter()
        .filter(|object| !object.name.starts_with("sqlite_"))
        .map(|object| object.name.as_str())
        .collect();
    assert_eq!(named.len(), 27);
    for table in [
        "store_meta",
        "content_block",
        "statement_binding",
        "binding_noun",
        "pair_event",
        "tool_pair",
        "span",
        "span_prefix",
        "span_member",
        "observation_run",
        "operation_observation",
        "plan_revision",
        "plan_category",
        "plan_group",
        "plan_step",
        "plan_group_step",
    ] {
        assert!(named.contains(&table), "missing {table}");
        let count: i64 = catalog
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "catalog construction does not seed {table}");
    }
    assert!(!named.contains(&"event"));
    assert!(!named.contains(&"field"));
    catalog.verify_catalog().unwrap();
}

#[test]
fn ram_settings_and_temporary_tables_are_verified() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    connection
        .execute("CREATE TEMP TABLE work(value TEXT)", [])
        .unwrap();
    connection
        .execute("INSERT INTO work VALUES ('temporary')", [])
        .unwrap();
    verify_connection(connection).unwrap();
    for (name, expected) in [("foreign_keys", 1), ("synchronous", 0), ("temp_store", 2)] {
        assert_eq!(
            connection
                .pragma_query_value(None, name, |row| row.get::<_, i64>(0))
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        connection
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .unwrap(),
        "memory"
    );
    let databases = connection
        .prepare("PRAGMA database_list")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(databases.contains(&("temp".to_owned(), String::new())));
    assert!(databases.iter().all(|(_, path)| path.is_empty()));
}

#[test]
fn changing_any_required_connection_setting_refuses_verification() {
    for (pragma, setting) in [
        ("PRAGMA foreign_keys = OFF", "foreign_keys"),
        ("PRAGMA journal_mode = OFF", "journal_mode"),
        ("PRAGMA synchronous = FULL", "synchronous"),
        ("PRAGMA temp_store = FILE", "temp_store"),
    ] {
        let catalog = ServingCatalog::create().unwrap();
        catalog.connection.execute_batch(pragma).unwrap();
        assert!(matches!(catalog.verify_catalog(),
            Err(CatalogFault::ConnectionInvariant {setting: actual, ..}) if actual == setting));
    }
}

#[test]
fn only_supported_unambiguous_temp_store_build_facts_pass() {
    for fact in ["TEMP_STORE=1", "TEMP_STORE=2", "TEMP_STORE=3"] {
        verify_temp_store_build(&["THREADSAFE=1".into(), fact.into()]).unwrap();
    }
    for facts in [
        vec![],
        vec!["TEMP_STORE=0"],
        vec!["TEMP_STORE=4"],
        vec!["TEMP_STORE="],
        vec!["TEMP_STORE=2x"],
        vec!["TEMP_STORE=1", "TEMP_STORE=2"],
        vec!["THREADSAFE=1"],
    ] {
        let facts: Vec<String> = facts.into_iter().map(str::to_owned).collect();
        assert!(
            matches!(
                verify_temp_store_build(&facts),
                Err(CatalogFault::TempStoreBuild { .. })
            ),
            "{facts:?}"
        );
    }
}

#[test]
fn adopted_bytes_require_settings_reapplied_and_the_same_catalog() {
    let original = ServingCatalog::create().unwrap();
    let bytes = original.connection.serialize("main").unwrap().to_vec();
    let mut scratch = Connection::open_in_memory().unwrap();
    // This tests only the connection/catalog subcheck, not STATE-ADOPT's semantic pass.
    scratch
        .deserialize_read_exact("main", bytes.as_slice(), bytes.len(), false)
        .unwrap();
    scratch.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    assert!(verify_connection(&scratch).is_err());
    configure(&scratch).unwrap();
    verify_schema(&scratch).unwrap();
    verify_integrity(&scratch).unwrap();
    verify_json_tables(&scratch).unwrap();
}

#[test]
fn missing_extra_or_changed_catalog_objects_refuse() {
    for changes in [
        vec!["DROP INDEX noun_value"],
        vec!["CREATE TABLE surprise(value TEXT)"],
        vec!["CREATE TRIGGER surprise AFTER INSERT ON content_block BEGIN SELECT 1; END"],
        vec![
            "DROP INDEX noun_value",
            "CREATE INDEX noun_value ON binding_noun(value COLLATE NOCASE)",
        ],
        vec!["ALTER TABLE content_block ADD COLUMN surprise TEXT"],
    ] {
        let catalog = ServingCatalog::create().unwrap();
        for change in changes {
            catalog.connection.execute_batch(change).unwrap();
        }
        assert!(matches!(
            catalog.verify_catalog(),
            Err(CatalogFault::CatalogMismatch { .. })
        ));
    }
}

#[test]
fn a_changed_check_or_foreign_key_is_not_the_catalog() {
    for (needle, replacement) in [
        ("CHECK(length(ordinal) = 8)", "CHECK(length(ordinal) > 0)"),
        (
            "block_id INTEGER NOT NULL REFERENCES content_block(id)",
            "block_id INTEGER NOT NULL",
        ),
    ] {
        let connection = open_memory().unwrap();
        for sql in schema::DDL {
            connection
                .execute(&sql.replace(needle, replacement), [])
                .unwrap();
        }
        assert!(matches!(
            verify_schema(&connection),
            Err(CatalogFault::CatalogMismatch { .. })
        ));
    }
}

#[test]
fn noun_verb_and_condition_queries_have_binary_indexes() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let block = seed_block(connection, 0);
    let binding = binding(connection, block, 0);
    connection
        .execute(
            "INSERT INTO binding_noun(binding_id, ordinal, value, start, end)
         VALUES (?1, ?2, 'State', ?3, ?4)",
            params![
                binding,
                0_u64.to_be_bytes(),
                5_u64.to_be_bytes(),
                10_u64.to_be_bytes()
            ],
        )
        .unwrap();
    for (query, index) in [
        (
            "EXPLAIN QUERY PLAN SELECT * FROM binding_noun WHERE value = ?1 COLLATE BINARY",
            "noun_value",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT * FROM statement_binding WHERE verb = ?1 COLLATE BINARY",
            "binding_verb",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT * FROM statement_binding WHERE condition = ?1 COLLATE BINARY",
            "binding_condition",
        ),
    ] {
        let details: Vec<String> = connection
            .prepare(query)
            .unwrap()
            .query_map(["State"], |row| row.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(
            details.iter().any(|detail| detail.contains(index)),
            "{details:?}"
        );
    }
    for (noun, expected) in [("State", 1), ("state", 0), ("State ", 0)] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM binding_noun WHERE value = ?1 COLLATE BINARY",
                [noun],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, expected);
    }
}

#[test]
fn unsigned_counter_blobs_order_above_signed_and_double_ranges() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let values = [
        0,
        1,
        (1_u64 << 53) + 1,
        i64::MAX as u64,
        1_u64 << 63,
        u64::MAX,
    ];
    for value in values.into_iter().rev() {
        seed_block(connection, value);
    }
    let stored: Vec<Vec<u8>> = connection
        .prepare("SELECT seed_ordinal FROM content_block ORDER BY seed_ordinal")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let decoded: Vec<u64> = stored
        .into_iter()
        .map(|bytes| u64::from_be_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(decoded, values);
    let maximum: Vec<u8> = connection
        .query_row("SELECT MAX(seed_ordinal) FROM content_block", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(u64::from_be_bytes(maximum.try_into().unwrap()), u64::MAX);
}

#[test]
fn foreign_keys_refuse_dangling_rows_and_integrity_finds_disabled_inserts() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let sql = "INSERT INTO binding_noun(binding_id, ordinal, value, start, end)
               VALUES (999, ?1, 'state', ?1, ?1)";
    constraint(connection.execute(sql, [0_u64.to_be_bytes()]));
    connection
        .execute_batch("PRAGMA foreign_keys = OFF")
        .unwrap();
    connection.execute(sql, [0_u64.to_be_bytes()]).unwrap();
    configure(connection).unwrap();
    assert!(matches!(
        catalog.verify_catalog(),
        Err(CatalogFault::Integrity { .. })
    ));
}

#[test]
fn legacy_engine_remains_separate_and_its_image_is_not_a_serving_catalog() {
    use crate::store::Store;
    let mut legacy = crate::engine::sqlite::Sqlite::stand().unwrap();
    super::super::raw_objects_survive_the_engine_and_answers(&mut legacy);
    let bytes = legacy.image().unwrap();
    let mut scratch = open_memory().unwrap();
    scratch
        .deserialize_read_exact("main", bytes.as_slice(), bytes.len(), false)
        .unwrap();
    configure(&scratch).unwrap();
    assert!(matches!(
        verify_schema(&scratch),
        Err(CatalogFault::CatalogMismatch { .. })
    ));
    let catalog = ServingCatalog::create().unwrap();
    assert!(legacy.schema().unwrap().contains("CREATE TABLE event"));
    catalog.verify_catalog().unwrap();
}

// The transaction instrument lives beside the serving engine under STATE-FILING-INSTRUMENTS.
pub(super) mod filing {
    //! Transaction evidence uses the production methods and each instance's private connection.

    use super::super::filing::*;
    use super::super::{ServingContext, ServingSqlite};
    use crate::ServingStore;
    use rusqlite::{Connection, Transaction, params};
    use serde_json::value::RawValue;
    use std::{cell::Cell, rc::Rc};
    use weaver_types::state_flow::*;

    pub(in crate::engine::serving_sqlite) fn store() -> ServingSqlite {
        let catalog = super::super::ServingCatalog::create().unwrap();
        let connection = catalog.connection;
        connection
            .execute(
                "INSERT INTO store_meta VALUES (1,'state-content/1',?1,?2)",
                params!["a".repeat(32), "scheme"],
            )
            .unwrap();
        connection.execute("INSERT INTO content_block(source_kind,seed_ordinal,admission,raw_block,statement_text,scheme_id,content_type) VALUES ('seed',?1,'seeded',?2,'Read state.','scheme','statement')",params![blob(0),r#"{"type":"text","text":"Read state."}"#]).unwrap();
        connection.execute("INSERT INTO span(local_id,revision,kind,status) VALUES (?1,?2,'system_prompt','closed')",params![blob(0),blob(1)]).unwrap();
        for table in ["span_prefix", "span_member"] {
            connection.execute(&format!("INSERT INTO {table}(local_id,revision,ordinal,block_id,start,end) VALUES (?1,?2,?1,1,?1,?3)"),params![blob(0),blob(1),blob(11)]).unwrap();
        }
        let start = StartIdentity {
            seed_id: "a".repeat(32),
            artifact_id: "fixture".into(),
            dump_sha256: Digest::new("0".repeat(64)).unwrap(),
            config_sha256: Digest::new("1".repeat(64)).unwrap(),
            parent_artifact_id: None,
            position: None,
        };
        ServingSqlite {
            connection,
            context: ServingContext {
                session_id: "session".into(),
                run_id: "run".into(),
                scheme_id: "scheme".into(),
                seed_id: start.seed_id.clone(),
                start,
            },
            read_limits: ReadLimits {
                max_result_bytes: 1_000_000.into(),
                measurement_ref: "test fixture".into(),
            },
            filing_hooks: None,
        }
    }

    pub(in crate::engine::serving_sqlite) fn statement(sequence: u64, kind: &str) -> StatementFile {
        let text = "Read item and file.";
        let source = SourceEnvelope {
            session: "session".into(),
            run: "run".into(),
            turn: Some("turn".into()),
            sequence: sequence.into(),
            kind: kind.into(),
            channel: if kind == "message.user" {
                "input"
            } else {
                "output"
            }
            .into(),
            origin: if kind == "message.user" {
                "operator"
            } else {
                "model"
            }
            .into(),
            subsystem: "harness".into(),
            causal_parent: None,
            wall_ms: 0,
            monotonic_ns: sequence.into(),
        };
        StatementFile {
            block_id: SourceBlockId {
                run: "run".into(),
                sequence: sequence.into(),
                block: 0.into(),
            },
            source,
            partition_rule_id: "text".into(),
            source_path: vec![
                SourcePathPart::Key("content".into()),
                SourcePathPart::Index { index: 0.into() },
            ],
            admission: Admission::Admitted,
            raw_block: RawValue::from_string(format!(r#"{{"type":"text","text":"{text}"}}"#))
                .unwrap(),
            text: text.into(),
            scheme_id: "scheme".into(),
            bindings: vec![Binding {
                statement_span: Span {
                    start: 0.into(),
                    end: 19.into(),
                },
                verb: "read".into(),
                verb_span: Span {
                    start: 0.into(),
                    end: 4.into(),
                },
                condition: None,
                condition_span: None,
                nouns: vec![
                    NounBinding {
                        value: "item".into(),
                        span: Span {
                            start: 5.into(),
                            end: 9.into(),
                        },
                    },
                    NounBinding {
                        value: "file".into(),
                        span: Span {
                            start: 14.into(),
                            end: 18.into(),
                        },
                    },
                ],
            }],
            context: None,
            effect: None,
        }
    }

    fn rows(connection: &Connection) -> Vec<(String, Vec<String>)> {
        let names: Vec<String> = connection
            .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        names
            .into_iter()
            .map(|name| {
                let mut query = connection
                    .prepare(&format!("SELECT * FROM {name} ORDER BY rowid"))
                    .unwrap();
                let columns = query.column_count();
                let values = query
                    .query_map([], |row| {
                        let fields = (0..columns)
                            .map(|index| row.get::<_, rusqlite::types::Value>(index))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(format!("{fields:?}"))
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                (name, values)
            })
            .collect()
    }

    struct Hook {
        fail: Option<FilingCheckpoint>,
        deferred: bool,
        committed: Rc<Cell<u32>>,
        source: SourceBlockId,
    }

    impl FilingTestHooks for Hook {
        fn checkpoint(
            &self,
            point: FilingCheckpoint,
            tx: &Transaction<'_>,
        ) -> Result<(), InjectedWriteFailure> {
            if self.fail == Some(point) {
                return Err(InjectedWriteFailure);
            }
            if self.deferred && point == FilingCheckpoint::BeforeCommit {
                tx.execute("INSERT INTO span(local_id,revision,kind,status,opened_content_id,ended_content_id,replacement_local_id,replacement_revision)
                    SELECT ?1,?2,'task','retired',MAX(id),MAX(id),?3,?2 FROM content_block",params![blob(500),blob(1),blob(999)]).unwrap();
            }
            Ok(())
        }
        fn committed(&self, connection: &Connection) {
            assert!(
                connection.is_autocommit(),
                "receipt callback cannot precede commit"
            );
            let id = source_id(connection, &self.source).unwrap().unwrap();
            let kind: String = connection
                .query_row(
                    "SELECT content_type FROM content_block WHERE id=?1",
                    [id],
                    |row| row.get(0),
                )
                .unwrap();
            if kind == "statement" {
                let (bindings,nouns):(i64,i64)=connection.query_row("SELECT COUNT(*),(SELECT COUNT(*) FROM binding_noun n JOIN statement_binding s ON s.id=n.binding_id WHERE s.block_id=?1) FROM statement_binding WHERE block_id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
                assert_eq!(
                    (bindings, nouns),
                    (1, 2),
                    "receipt observer sees all source index rows"
                );
            } else {
                let (event,ledger):(i64,i64)=connection.query_row("SELECT (SELECT COUNT(*) FROM pair_event WHERE id=?1),(SELECT COUNT(*) FROM tool_pair WHERE opened_event=?1 OR terminal_event=?1)",[id],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
                assert_eq!(
                    (event, ledger),
                    (1, 1),
                    "receipt observer sees both pair rows"
                );
            }
            self.committed.set(self.committed.get() + 1);
        }
    }

    #[test]
    fn every_statement_checkpoint_rolls_back_the_source_indexes_and_effects() {
        for point in [
            FilingCheckpoint::AfterSource,
            FilingCheckpoint::AfterBinding(0),
            FilingCheckpoint::AfterNoun(0, 0),
            FilingCheckpoint::AfterNoun(0, 1),
            FilingCheckpoint::AfterEffect,
            FilingCheckpoint::BeforeCommit,
        ] {
            let mut store = store();
            let filing = statement(1, "message.user");
            let before = rows(&store.connection);
            let committed = Rc::new(Cell::new(0));
            store.filing_hooks = Some(Box::new(Hook {
                fail: Some(point),
                deferred: false,
                committed: committed.clone(),
                source: filing.block_id.clone(),
            }));
            assert_eq!(
                store.file(&filing).unwrap_err().code,
                StateRefusalCode::WriteFailed,
                "{point:?}"
            );
            assert_eq!(rows(&store.connection), before, "{point:?}");
            assert_eq!(committed.get(), 0);
        }
    }

    #[test]
    fn deferred_commit_failure_has_no_receipt_or_surviving_rows() {
        let mut store = store();
        let filing = statement(1, "message.user");
        let before = rows(&store.connection);
        let committed = Rc::new(Cell::new(0));
        store.filing_hooks = Some(Box::new(Hook {
            fail: None,
            deferred: true,
            committed: committed.clone(),
            source: filing.block_id.clone(),
        }));
        assert_eq!(
            store.file(&filing).unwrap_err().code,
            StateRefusalCode::WriteFailed
        );
        assert_eq!(rows(&store.connection), before);
        assert_eq!(committed.get(), 0);
    }

    #[test]
    fn exhausted_internal_ids_refuse_instead_of_using_sqlite_random_rowids() {
        let mut store = store();
        store.connection.execute("INSERT INTO statement_binding(id,block_id,ordinal,start,end,verb,verb_start,verb_end) VALUES (?1,1,?2,?2,?3,'read',?2,?4)",params![i64::MAX,blob(0),blob(11),blob(4)]).unwrap();
        let before = rows(&store.connection);
        assert_eq!(
            store.file(&statement(1, "message.user")).unwrap_err().code,
            StateRefusalCode::WriteFailed
        );
        assert_eq!(rows(&store.connection), before);
        store
            .connection
            .execute("DELETE FROM statement_binding", [])
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE sqlite_sequence SET seq=?1 WHERE name='content_block'",
                [i64::MAX],
            )
            .unwrap();
        let before = rows(&store.connection);
        assert_eq!(
            store.file(&statement(1, "message.user")).unwrap_err().code,
            StateRefusalCode::WriteFailed
        );
        assert_eq!(rows(&store.connection), before);
    }

    #[test]
    fn receipt_follows_commit_and_source_spelling_remains_exact() {
        let mut store = store();
        let mut filing = statement(1, "message.user");
        filing.raw_block =
            RawValue::from_string(r#"{"type":"text","text":"Read\u0020item and file."}"#.into())
                .unwrap();
        let committed = Rc::new(Cell::new(0));
        store.filing_hooks = Some(Box::new(Hook {
            fail: None,
            deferred: false,
            committed: committed.clone(),
            source: filing.block_id.clone(),
        }));
        assert_eq!(store.file(&filing).unwrap().block_id, filing.block_id);
        assert_eq!(committed.get(), 1);
        let (raw,nouns):(String,i64)=store.connection.query_row("SELECT raw_block,(SELECT COUNT(*) FROM binding_noun) FROM content_block WHERE source_kind='event'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(raw, filing.raw_block.get());
        assert_eq!(nouns, 2);
        let before = rows(&store.connection);
        assert_eq!(
            store.file(&filing).unwrap_err().code,
            StateRefusalCode::DuplicateSource
        );
        assert_eq!(rows(&store.connection), before);
    }

    #[test]
    fn mismatched_binding_scheme_run_and_unlicensed_effect_write_nothing() {
        for defect in ["noun", "utf8", "scheme", "run", "effect"] {
            let mut store = store();
            let mut filing = statement(1, "message.user");
            match defect {
                "noun" => filing.bindings[0].nouns[0].value = "Item".into(),
                "utf8" => filing.bindings[0].nouns[0].span.end = 99.into(),
                "scheme" => filing.scheme_id = "other".into(),
                "run" => filing.source.run = "other".into(),
                "effect" => {
                    filing.effect = Some(StatementEffect::Mark(MarkEffect::Close {
                        span: SpanRef {
                            seed_id: store.context.seed_id.clone(),
                            local_id: 0.into(),
                            revision: 1.into(),
                        },
                    }))
                }
                _ => unreachable!(),
            }
            let before = rows(&store.connection);
            assert!(store.file(&filing).is_err(), "{defect}");
            assert_eq!(rows(&store.connection), before, "{defect}");
        }
    }

    fn begin_task(store: &ServingSqlite, source: &StatementFile, sequence: u64) -> StatementFile {
        let mut filing = statement(sequence, "state.mark");
        filing.source.channel = "internal".into();
        filing.source.origin = "loop".into();
        filing
            .source_path
            .insert(0, SourcePathPart::Key("message".into()));
        let effect = MarkEffect::BeginTask {
            task: SpanRef {
                seed_id: store.context.seed_id.clone(),
                local_id: 1.into(),
                revision: 1.into(),
            },
            prompt: SpanRef {
                seed_id: store.context.seed_id.clone(),
                local_id: 2.into(),
                revision: 1.into(),
            },
            source: ContentRef::Event {
                block_id: source.block_id.clone(),
            },
        };
        filing.context = Some(SourceContext {
            source_path: vec![SourcePathPart::Key("context".into())],
            raw: RawValue::from_string(serde_json::to_string(&effect).unwrap()).unwrap(),
        });
        filing.effect = Some(StatementEffect::Mark(effect));
        filing
    }

    #[test]
    fn direct_task_effect_is_atomic_and_cannot_reuse_ids_or_another_turn() {
        let mut store = store();
        let input = statement(1, "message.user");
        store.file(&input).unwrap();
        let filing = begin_task(&store, &input, 2);
        let before = rows(&store.connection);
        let committed = Rc::new(Cell::new(0));
        store.filing_hooks = Some(Box::new(Hook {
            fail: Some(FilingCheckpoint::AfterEffect),
            deferred: false,
            committed: committed.clone(),
            source: filing.block_id.clone(),
        }));
        assert!(store.file(&filing).is_err());
        assert_eq!(rows(&store.connection), before);
        store.filing_hooks = None;
        let mut wrong_turn = filing.clone();
        wrong_turn.source.turn = Some("other".into());
        assert_eq!(
            store.file(&wrong_turn).unwrap_err().code,
            StateRefusalCode::InvalidEffect
        );
        store.file(&filing).unwrap();
        let (spans, members): (i64, i64) = store
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM span),(SELECT COUNT(*) FROM span_member)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((spans, members), (3, 2));
        let another = begin_task(&store, &input, 3);
        let before = rows(&store.connection);
        assert_eq!(
            store.file(&another).unwrap_err().code,
            StateRefusalCode::InvalidEffect
        );
        assert_eq!(rows(&store.connection), before);
    }

    pub(in crate::engine::serving_sqlite) fn pair_file(
        sequence: u64,
        pair: PairRecord,
        origin: &str,
    ) -> PairFile {
        let mut source = statement(sequence, "tool.call.started").source;
        source.channel = "tool".into();
        source.origin = origin.into();
        let raw = match &pair {
            PairRecord::Started(value) => serde_json::to_string(value).unwrap(),
            PairRecord::Ended(value) => {
                source.kind = "tool.call.completed".into();
                serde_json::to_string(value).unwrap()
            }
        };
        PairFile {
            block_id: SourceBlockId {
                run: "run".into(),
                sequence: sequence.into(),
                block: 0.into(),
            },
            source,
            partition_rule_id: "pair".into(),
            source_path: vec![],
            admission: Admission::Admitted,
            raw_block: RawValue::from_string(raw).unwrap(),
            scheme_id: "scheme".into(),
            pair,
        }
    }

    pub(in crate::engine::serving_sqlite) fn opening(statement: &StatementFile) -> PairStarted {
        PairStarted {
            call_id: CallId {
                run: "run".into(),
                sequence: statement.block_id.sequence,
                call: 0.into(),
            },
            call_block: 1.into(),
            statement: StatementRef {
                block_id: statement.block_id.clone(),
                binding: 0.into(),
            },
            lane: PairLane::External,
            request: PairRequest::External {
                name: "sh".into(),
                arguments: "echo fixture".into(),
            },
            retry_of: None,
        }
    }

    fn completion(start: &PairStarted) -> PairEnded {
        PairEnded {
            call_id: start.call_id.clone(),
            execution: Some(weaver_types::ExchangeId {
                opener: weaver_types::Opener::Harness,
                ordinal: 1,
            }),
            fate: TerminalPairFate::Completed,
            reason: None,
            outcome: Some(weaver_types::ToolOutcome::Result {
                content: "whole result".into(),
            }),
            loaded_from: None,
        }
    }

    #[test]
    fn malformed_recall_opening_refuses_compound_and_conditional_bindings_atomically() {
        for (compound, conditional, subject) in [
            (true, false, "item"),
            (false, true, "item"),
            (true, true, "item"),
            (false, false, "other"),
        ] {
            let mut store = store();
            let mut send = statement(1, "message.assistant");
            if !compound {
                send.bindings[0].nouns.truncate(1);
            }
            if conditional {
                send.bindings[0].condition = Some("file".into());
                send.bindings[0].condition_span = Some(Span {
                    start: 14.into(),
                    end: 18.into(),
                });
            }
            store.file(&send).unwrap();
            let before = rows(&store.connection);
            let mut started = opening(&send);
            started.lane = PairLane::Recall;
            started.request = PairRequest::Recall {
                subject: subject.into(),
            };
            started.call_block = 0.into();
            let refused = store
                .file_pair(&pair_file(2, PairRecord::Started(started), "model"))
                .unwrap_err();
            assert_eq!(refused.code, StateRefusalCode::InvalidPair);
            assert!(refused.detail.contains("single held noun"));
            assert_eq!(
                rows(&store.connection),
                before,
                "refused opening must not leave a source or pair row"
            );
        }
        let mut store = store();
        let mut send = statement(1, "message.assistant");
        send.bindings[0].nouns.truncate(1);
        store.file(&send).unwrap();
        let mut started = opening(&send);
        started.lane = PairLane::Recall;
        started.request = PairRequest::Recall {
            subject: "item".into(),
        };
        started.call_block = 0.into();
        store
            .file_pair(&pair_file(2, PairRecord::Started(started), "model"))
            .unwrap();
    }

    #[test]
    fn pair_lane_request_mismatch_refuses_and_rolls_back_its_source() {
        let mut store = store();
        let send = statement(1, "message.assistant");
        store.file(&send).unwrap();
        let mut started = opening(&send);
        started.lane = PairLane::Recall;
        let filing = pair_file(2, PairRecord::Started(started.clone()), "model");
        let before = store.image().unwrap();
        let error = store.file_pair(&filing).unwrap_err();
        assert_eq!(error.code, StateRefusalCode::InvalidPair);
        assert_eq!(store.image().unwrap(), before);
        assert_eq!(
            source_id(&store.connection, &filing.block_id).unwrap(),
            None
        );
        for table in ["pair_event", "tool_pair"] {
            let count: i64 = store
                .connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "refused pair must leave no {table} row");
        }
        started.lane = PairLane::External;
        store
            .file_pair(&pair_file(2, PairRecord::Started(started), "model"))
            .unwrap();
    }

    #[test]
    fn pair_receipts_are_atomic_and_terminal_is_single_use() {
        let mut store = store();
        let send = statement(1, "message.assistant");
        store.file(&send).unwrap();
        let started = opening(&send);
        let start = pair_file(2, PairRecord::Started(started.clone()), "model");
        for point in [
            FilingCheckpoint::AfterSource,
            FilingCheckpoint::AfterEffect,
            FilingCheckpoint::BeforeCommit,
        ] {
            let before = rows(&store.connection);
            store.filing_hooks = Some(Box::new(Hook {
                fail: Some(point),
                deferred: false,
                committed: Rc::new(Cell::new(0)),
                source: start.block_id.clone(),
            }));
            assert!(store.file_pair(&start).is_err());
            assert_eq!(rows(&store.connection), before);
        }
        store.filing_hooks = None;
        store.file_pair(&start).unwrap();
        let end = pair_file(3, PairRecord::Ended(completion(&started)), "tool");
        let before = rows(&store.connection);
        store.filing_hooks = Some(Box::new(Hook {
            fail: Some(FilingCheckpoint::AfterEffect),
            deferred: false,
            committed: Rc::new(Cell::new(0)),
            source: end.block_id.clone(),
        }));
        assert!(store.file_pair(&end).is_err());
        assert_eq!(rows(&store.connection), before);
        let committed = Rc::new(Cell::new(0));
        store.filing_hooks = Some(Box::new(Hook {
            fail: None,
            deferred: false,
            committed: committed.clone(),
            source: end.block_id.clone(),
        }));
        store.file_pair(&end).unwrap();
        assert_eq!(committed.get(), 1);
        store.filing_hooks = None;
        let again = pair_file(4, PairRecord::Ended(completion(&started)), "tool");
        let before = rows(&store.connection);
        assert_eq!(
            store.file_pair(&again).unwrap_err().code,
            StateRefusalCode::AlreadyTerminal
        );
        assert_eq!(rows(&store.connection), before);
    }

    #[test]
    fn terminal_fate_mapping_refuses_clock_kill_as_completed() {
        let mut store = store();
        let send = statement(1, "message.assistant");
        store.file(&send).unwrap();
        let started = opening(&send);
        store
            .file_pair(&pair_file(2, PairRecord::Started(started.clone()), "model"))
            .unwrap();
        let mut ended = completion(&started);
        ended.outcome = Some(weaver_types::ToolOutcome::Killed {
            partial: Some("partial".into()),
            by: weaver_types::KillCause::Clock,
        });
        let before = rows(&store.connection);
        assert_eq!(
            store
                .file_pair(&pair_file(3, PairRecord::Ended(ended.clone()), "tool"))
                .unwrap_err()
                .code,
            StateRefusalCode::InvalidPair
        );
        assert_eq!(rows(&store.connection), before);
        ended.fate = TerminalPairFate::Failed;
        ended.reason = Some(PairFailure::Timeout {});
        store
            .file_pair(&pair_file(3, PairRecord::Ended(ended), "tool"))
            .unwrap();
    }

    #[test]
    fn every_terminal_outcome_mapping_keeps_the_whole_outcome() {
        use weaver_types::{KillCause, ToolOutcome};
        let cases = [
            (
                None,
                ToolOutcome::Result {
                    content: "nonzero exit is still a result".into(),
                },
                "tool",
            ),
            (
                None,
                ToolOutcome::Refused {
                    reason: "gate refusal".into(),
                },
                "tool",
            ),
            (
                None,
                ToolOutcome::Errored {
                    detail: "machinery failed".into(),
                },
                "tool",
            ),
            (
                Some(PairFailure::Timeout {}),
                ToolOutcome::Killed {
                    partial: Some("partial clock".into()),
                    by: KillCause::Clock,
                },
                "tool",
            ),
            (
                Some(PairFailure::Cancelled {}),
                ToolOutcome::Killed {
                    partial: None,
                    by: KillCause::Cancel,
                },
                "tool",
            ),
            (
                Some(PairFailure::Unload {}),
                ToolOutcome::Killed {
                    partial: Some("partial unload".into()),
                    by: KillCause::Unload,
                },
                "tool",
            ),
            (
                Some(PairFailure::GateFault {}),
                ToolOutcome::Killed {
                    partial: Some("indeterminate".into()),
                    by: KillCause::Fault,
                },
                "loop",
            ),
            (
                Some(PairFailure::NotGranted {}),
                ToolOutcome::Refused {
                    reason: "no grant".into(),
                },
                "loop",
            ),
        ];
        for (reason, outcome, origin) in cases {
            let mut store = store();
            let send = statement(1, "message.assistant");
            store.file(&send).unwrap();
            let started = opening(&send);
            store
                .file_pair(&pair_file(2, PairRecord::Started(started.clone()), "model"))
                .unwrap();
            let mut ended = completion(&started);
            ended.fate = if reason.is_some() {
                TerminalPairFate::Failed
            } else {
                TerminalPairFate::Completed
            };
            if matches!(reason, Some(PairFailure::NotGranted {})) {
                ended.execution = None;
            }
            ended.reason = reason;
            ended.outcome = Some(outcome);
            let terminal = pair_file(3, PairRecord::Ended(ended.clone()), origin);
            store.file_pair(&terminal).unwrap();
            let raw: String = store
                .connection
                .query_row(
                    "SELECT raw_block FROM content_block WHERE kind='tool.call.completed'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let held: PairEnded = decode_strict(&raw).unwrap();
            assert_eq!(held, ended);
        }
        for cause in [
            NonDispatchCause::PassPurpose,
            NonDispatchCause::RepairExhausted,
            NonDispatchCause::PlanBreak,
            NonDispatchCause::RoundLimit,
            NonDispatchCause::Stop,
            NonDispatchCause::Unload,
            NonDispatchCause::NoGate,
        ] {
            let mut store = store();
            let send = statement(1, "message.assistant");
            store.file(&send).unwrap();
            let started = opening(&send);
            store
                .file_pair(&pair_file(2, PairRecord::Started(started.clone()), "model"))
                .unwrap();
            let ended = PairEnded {
                call_id: started.call_id,
                execution: None,
                fate: TerminalPairFate::Failed,
                reason: Some(PairFailure::NotDispatched { cause }),
                outcome: None,
                loaded_from: None,
            };
            store
                .file_pair(&pair_file(3, PairRecord::Ended(ended), "loop"))
                .unwrap();
        }
    }

    #[test]
    fn current_run_pair_cannot_be_abandoned_at_load_and_refusal_rolls_back() {
        let mut store = store();
        let send = statement(1, "message.assistant");
        store.file(&send).unwrap();
        let started = opening(&send);
        store
            .file_pair(&pair_file(2, PairRecord::Started(started.clone()), "model"))
            .unwrap();
        let abandoned = PairEnded {
            call_id: started.call_id.clone(),
            execution: None,
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::AbandonedAtLoad {}),
            outcome: None,
            loaded_from: Some(store.context.start.clone()),
        };
        let mut terminal = pair_file(3, PairRecord::Ended(abandoned), "loop");
        terminal.source.turn = None;
        let before = rows(&store.connection);
        assert_eq!(
            store.file_pair(&terminal).unwrap_err().code,
            StateRefusalCode::InvalidPair
        );
        assert_eq!(rows(&store.connection), before);
        // Reusing the refused source for its real result proves that neither
        // the source nor the pair terminal was committed by the refusal.
        store
            .file_pair(&pair_file(
                3,
                PairRecord::Ended(completion(&started)),
                "tool",
            ))
            .unwrap();
    }

    #[test]
    fn inward_terminal_and_abandoned_pair_have_no_fabricated_gate_execution() {
        let mut store = store();
        let mut send = statement(1, "message.assistant");
        send.bindings[0].nouns.truncate(1);
        store.file(&send).unwrap();
        let mut started = opening(&send);
        started.lane = PairLane::Recall;
        started.request = PairRequest::Recall {
            subject: "item".into(),
        };
        started.call_block = 0.into();
        store
            .file_pair(&pair_file(2, PairRecord::Started(started.clone()), "model"))
            .unwrap();
        let mut ended = completion(&started);
        ended.execution = None;
        store
            .file_pair(&pair_file(3, PairRecord::Ended(ended), "loop"))
            .unwrap();

        let send = statement(4, "message.assistant");
        store.file(&send).unwrap();
        let started = opening(&send);
        store
            .file_pair(&pair_file(5, PairRecord::Started(started.clone()), "model"))
            .unwrap();
        store.context.run_id = "next-run".into();
        let ended = PairEnded {
            call_id: started.call_id,
            execution: None,
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::AbandonedAtLoad {}),
            outcome: None,
            loaded_from: Some(store.context.start.clone()),
        };
        let mut terminal = pair_file(1, PairRecord::Ended(ended), "loop");
        terminal.block_id.run = "next-run".into();
        terminal.source.run = "next-run".into();
        terminal.source.turn = None;
        let mut incorrect = terminal.clone();
        if let PairRecord::Ended(record) = &mut incorrect.pair {
            record.loaded_from.as_mut().unwrap().config_sha256 =
                Digest::new("9".repeat(64)).unwrap();
            incorrect.raw_block =
                RawValue::from_string(serde_json::to_string(record).unwrap()).unwrap();
        }
        let before = rows(&store.connection);
        assert_eq!(
            store.file_pair(&incorrect).unwrap_err().code,
            StateRefusalCode::InvalidPair
        );
        assert_eq!(rows(&store.connection), before);
        store.file_pair(&terminal).unwrap();
    }

    #[test]
    fn retry_requires_a_terminal_identical_current_run_external_predecessor() {
        let mut store = store();
        let first = statement(1, "message.assistant");
        store.file(&first).unwrap();
        let predecessor = opening(&first);
        store
            .file_pair(&pair_file(
                2,
                PairRecord::Started(predecessor.clone()),
                "model",
            ))
            .unwrap();
        let second = statement(3, "message.assistant");
        store.file(&second).unwrap();
        let mut retry = opening(&second);
        retry.retry_of = Some(predecessor.call_id.clone());
        let before = rows(&store.connection);
        assert_eq!(
            store
                .file_pair(&pair_file(4, PairRecord::Started(retry.clone()), "model"))
                .unwrap_err()
                .code,
            StateRefusalCode::InvalidPair
        );
        assert_eq!(rows(&store.connection), before);
        store
            .file_pair(&pair_file(
                4,
                PairRecord::Ended(completion(&predecessor)),
                "tool",
            ))
            .unwrap();
        let mut changed = retry.clone();
        if let PairRequest::External { arguments, .. } = &mut changed.request {
            *arguments = "different".into();
        }
        assert_eq!(
            store
                .file_pair(&pair_file(5, PairRecord::Started(changed), "model"))
                .unwrap_err()
                .code,
            StateRefusalCode::InvalidPair
        );
        store
            .file_pair(&pair_file(5, PairRecord::Started(retry), "model"))
            .unwrap();
    }

    #[test]
    fn interval_checks_distinguish_partial_and_pending_pairs_at_the_fixed_frontier() {
        let mut store = store();
        let send = statement(1, "message.assistant");
        store.file(&send).unwrap();
        let content = source_id(&store.connection, &send.block_id)
            .unwrap()
            .unwrap();
        assert!(matches!(
            marks::validate_interval(&store.connection, content, 0, 9, content),
            Err(marks::IntervalFault::PartialBinding)
        ));
        let started = opening(&send);
        let opened = pair_file(2, PairRecord::Started(started.clone()), "model");
        store.file_pair(&opened).unwrap();
        let open_id = source_id(&store.connection, &opened.block_id)
            .unwrap()
            .unwrap();
        assert!(marks::validate_interval(&store.connection, content, 0, 19, content).is_ok());
        assert!(matches!(
            marks::validate_interval(&store.connection, content, 0, 9, open_id),
            Err(marks::IntervalFault::PartialPair)
        ));
        assert!(matches!(
            marks::validate_interval(&store.connection, content, 0, 19, open_id),
            Err(marks::IntervalFault::PendingPair)
        ));
        let ended = pair_file(3, PairRecord::Ended(completion(&started)), "tool");
        store.file_pair(&ended).unwrap();
        let end_id = source_id(&store.connection, &ended.block_id)
            .unwrap()
            .unwrap();
        assert!(matches!(
            marks::validate_interval(&store.connection, content, 0, 19, open_id),
            Err(marks::IntervalFault::PendingPair)
        ));
        assert!(marks::validate_interval(&store.connection, content, 0, 19, end_id).is_ok());
    }

    #[test]
    fn canonical_members_keep_prompt_prefix_and_strict_ordinary_work_order() {
        let mut store = store();
        let input = statement(1, "message.user");
        store.file(&input).unwrap();
        let start = begin_task(&store, &input, 2);
        store.file(&start).unwrap();
        let work = statement(3, "message.assistant");
        store.file(&work).unwrap();
        let end = statement(4, "sort.decision");
        store.file(&end).unwrap();
        let task = SpanRef {
            seed_id: store.context.seed_id.clone(),
            local_id: 1.into(),
            revision: 1.into(),
        };
        let source = source_id(&store.connection, &end.block_id)
            .unwrap()
            .unwrap();
        let tx = store.connection.transaction().unwrap();
        marks::finish(&tx, &store.context, source, &task, SpanStatus::Closed, None).unwrap();
        tx.commit().unwrap();
        let ids: Vec<i64> = store
            .connection
            .prepare("SELECT block_id FROM span_member WHERE local_id=?1 ORDER BY ordinal")
            .unwrap()
            .query_map([blob(1)], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            ids,
            vec![
                source_id(&store.connection, &input.block_id)
                    .unwrap()
                    .unwrap(),
                source_id(&store.connection, &work.block_id)
                    .unwrap()
                    .unwrap()
            ]
        );
        let before = rows(&store.connection);
        let tx = store.connection.transaction().unwrap();
        assert!(
            marks::finish(
                &tx,
                &store.context,
                source,
                &task,
                SpanStatus::Retired,
                None
            )
            .is_err()
        );
        drop(tx);
        assert_eq!(rows(&store.connection), before);
    }

    #[test]
    fn mark_adoption_checks_allocation_sources_prefixes_and_open_membership() {
        for defect in [
            "none",
            "seed_member",
            "prompt_member",
            "parent",
            "context",
            "open_member",
            "query",
        ] {
            let mut store = store();
            let input = statement(1, "message.user");
            store.file(&input).unwrap();
            let begin = begin_task(&store, &input, 2);
            store.file(&begin).unwrap();
            match defect {
                "none" => {}
                "seed_member" => {
                    store
                        .connection
                        .execute("DELETE FROM span_member WHERE local_id=?1", [blob(0)])
                        .unwrap();
                }
                "prompt_member" => {
                    store
                        .connection
                        .execute(
                            "UPDATE span_member SET start=?1 WHERE local_id=?2",
                            params![blob(1), blob(2)],
                        )
                        .unwrap();
                }
                "parent" => {
                    store
                        .connection
                        .execute(
                            "UPDATE span SET parent_local_id=?1 WHERE local_id=?2",
                            params![blob(0), blob(2)],
                        )
                        .unwrap();
                }
                "context" => {
                    store
                        .connection
                        .execute(
                            "UPDATE content_block SET context_raw='{}' WHERE kind='state.mark'",
                            [],
                        )
                        .unwrap();
                }
                "open_member" => {
                    store.connection.execute("INSERT INTO span_member SELECT ?1,revision,ordinal,block_id,start,end,step_query FROM span_member WHERE local_id=?2",params![blob(1),blob(2)]).unwrap();
                }
                "query" => {
                    store
                        .connection
                        .execute(
                            "UPDATE span_prefix SET step_query='{}' WHERE local_id=?1",
                            [blob(2)],
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }
            let checked = marks::verify_image_marks(&store.connection, &store.context.seed_id);
            assert_eq!(checked.is_ok(), defect == "none", "{defect}: {checked:?}");
        }
    }

    #[test]
    fn terminal_mark_adoption_recomputes_members_instead_of_trusting_the_dump() {
        let mut store = store();
        let input = statement(1, "message.user");
        store.file(&input).unwrap();
        let begin = begin_task(&store, &input, 2);
        store.file(&begin).unwrap();
        let work = statement(3, "message.assistant");
        store.file(&work).unwrap();
        let task = SpanRef {
            seed_id: store.context.seed_id.clone(),
            local_id: 1.into(),
            revision: 1.into(),
        };
        let prompt = SpanRef {
            seed_id: store.context.seed_id.clone(),
            local_id: 2.into(),
            revision: 1.into(),
        };
        // A mark-only adoption fixture supplies typed plan provenance. The separate
        // plan adopter owns the full revision/category/evidence account.
        let plan_id = PlanId {
            seed_id: store.context.seed_id.clone(),
            local_id: 1.into(),
        };
        let effect = PlanEffect::Replace {
            expected: PlanRef {
                plan_id: plan_id.clone(),
                revision: 1.into(),
            },
            next: PlanRef {
                plan_id,
                revision: 2.into(),
            },
            source: input.block_id.clone(),
            task: TaskScopeRef {
                task: task.clone(),
                prompt,
            },
            steps: vec![],
            review: ReviewSetupRef {
                setup_id: "fixture".into(),
                sha256: Digest::new("0".repeat(64)).unwrap(),
            },
        };
        let mut end = statement(4, "plan.event");
        end.source.channel = "internal".into();
        end.source.origin = "loop".into();
        end.source_path
            .insert(0, SourcePathPart::Key("message".into()));
        end.context = Some(SourceContext {
            source_path: vec![SourcePathPart::Key("context".into())],
            raw: RawValue::from_string(serde_json::to_string(&effect).unwrap()).unwrap(),
        });
        end.effect = Some(StatementEffect::Plan(Box::new(effect)));
        let tx = store.connection.transaction().unwrap();
        let source = insert_statement(&tx, &end).unwrap();
        marks::finish(&tx, &store.context, source, &task, SpanStatus::Closed, None).unwrap();
        tx.commit().unwrap();
        marks::verify_image_marks(&store.connection, &store.context.seed_id).unwrap();
        store
            .connection
            .execute(
                "DELETE FROM span_member WHERE local_id=?1 AND ordinal=?2",
                params![blob(1), blob(1)],
            )
            .unwrap();
        assert!(marks::verify_image_marks(&store.connection, &store.context.seed_id).is_err());
    }

    #[test]
    fn production_adoption_requires_every_task_effect_and_its_original_input() {
        use sha2::{Digest as _, Sha256};
        let digest = |bytes: &[u8]| Digest::new(format!("{:x}", Sha256::digest(bytes))).unwrap();
        let request = super::super::image::tests::fixture();
        let seed = super::super::image::seed_image(&request).unwrap();
        let mut start = StartIdentity {
            seed_id: request.seed_id.clone(),
            artifact_id: "task-adoption-fixture".into(),
            dump_sha256: digest(seed.as_bytes()),
            config_sha256: digest(b"fixture configuration"),
            parent_artifact_id: None,
            position: None,
        };
        let common = Common {
            session_id: "session".into(),
            run_id: "run".into(),
            request_id: 1.into(),
        };
        let limits = ReadLimits {
            max_result_bytes: 1_000_000.into(),
            measurement_ref: "task-adoption test fixture".into(),
        };
        let mut store = ServingSqlite::adopt(
            seed.as_bytes(),
            &common,
            &request.schema_id,
            &request.scheme_id,
            &start,
            limits.clone(),
        )
        .unwrap();
        let mut input = statement(1, "message.user");
        input.scheme_id = request.scheme_id.clone();
        store.file(&input).unwrap();
        let mut effect = begin_task(&store, &input, 2);
        effect.scheme_id = request.scheme_id.clone();
        store.file(&effect).unwrap();
        let image = store.image().unwrap();
        start.position = store.position().unwrap();
        start.dump_sha256 = digest(&image);
        ServingSqlite::adopt(
            &image,
            &common,
            &request.schema_id,
            &request.scheme_id,
            &start,
            limits.clone(),
        )
        .unwrap();

        for (defect, sql) in [
            (
                "deleted materialized task",
                "DELETE FROM span_member WHERE local_id != X'0000000000000000'; DELETE FROM span_prefix WHERE local_id != X'0000000000000000'; DELETE FROM span WHERE local_id != X'0000000000000000'",
            ),
            (
                "different turn",
                "UPDATE content_block SET turn='another-turn',envelope=json_set(envelope,'$.turn','another-turn') WHERE id=2",
            ),
            (
                "absent turn",
                "UPDATE content_block SET turn=NULL,envelope=json_set(envelope,'$.turn',NULL) WHERE id=2",
            ),
            (
                "different input kind",
                "UPDATE content_block SET kind='message.assistant',channel='output',origin='model',envelope=json_set(envelope,'$.kind','message.assistant','$.channel','output','$.origin','model') WHERE id=2",
            ),
        ] {
            let mut database = Connection::open_in_memory().unwrap();
            database
                .deserialize_read_exact(rusqlite::MAIN_DB, image.as_slice(), image.len(), false)
                .unwrap();
            database.execute_batch(sql).unwrap();
            let corrupted = database.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
            let mut forged_start = start.clone();
            forged_start.dump_sha256 = digest(&corrupted);
            assert!(
                ServingSqlite::adopt(
                    &corrupted,
                    &common,
                    &request.schema_id,
                    &request.scheme_id,
                    &forged_start,
                    limits.clone(),
                )
                .is_err(),
                "{defect}"
            );
        }
    }
}
