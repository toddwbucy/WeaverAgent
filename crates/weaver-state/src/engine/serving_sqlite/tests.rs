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
