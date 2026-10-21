//! Real SQLite constraint checks. These fixtures exercise the catalog, not the
//! semantic seed, filing, mark or plan validators that will sit above it.

use super::*;

fn event_pair(connection: &Connection, sequence: u64, record_type: &str) -> i64 {
    connection
        .execute(
            "INSERT INTO content_block
         (source_kind, run, sequence, block, session, kind, channel, origin,
          envelope, rule_id, source_path, admission, raw_block, scheme_id, content_type)
         VALUES ('event', 'run', ?1, ?2, 'session', 'tool.call.started', 'tool', 'model',
          '{}', 'pair', '[]', 'admitted', '{}', 'scheme', 'pair')",
            params![sequence.to_be_bytes(), 0_u64.to_be_bytes()],
        )
        .unwrap();
    let id = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO pair_event(id, record_type) VALUES (?1, ?2)",
            params![id, record_type],
        )
        .unwrap();
    id
}

fn closed_span(connection: &Connection, id: u64, content: i64) {
    connection.execute(
        "INSERT INTO span(local_id, revision, kind, status, opened_content_id, ended_content_id)
         VALUES (?1, ?2, 'task', 'closed', ?3, ?3)",
        params![id.to_be_bytes(), 1_u64.to_be_bytes(), content],
    ).unwrap();
}

#[test]
fn metadata_is_singleton_and_pins_the_schema_id() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let sql = "INSERT INTO store_meta VALUES (?1, ?2, 'seed', 'scheme')";
    constraint(connection.execute(sql, params![2, "state-content/1"]));
    constraint(connection.execute(sql, params![1, "state-content/2"]));
    connection
        .execute(sql, params![1, "state-content/1"])
        .unwrap();
    constraint(connection.execute(sql, params![1, "state-content/1"]));
}

#[test]
fn content_checks_reject_partial_sources_and_wrong_content_classes() {
    for alteration in [
        "seed_ordinal = X'01'",
        "seed_ordinal = NULL",
        "source_kind = 'unknown'",
        "source_kind = 'event'",
        "run = 'run'",
        "admission = 'admitted'",
        "content_type = 'pair'",
        "statement_text = NULL",
        "context_path = '[]'",
        "effect = '{}'",
    ] {
        let catalog = ServingCatalog::create().unwrap();
        seed_block(&catalog.connection, 0);
        constraint(
            catalog
                .connection
                .execute(&format!("UPDATE content_block SET {alteration}"), []),
        );
    }
    let catalog = ServingCatalog::create().unwrap();
    seed_block(&catalog.connection, 0);
    constraint(catalog.connection.execute(
        "INSERT INTO content_block
         (source_kind, seed_ordinal, admission, raw_block, statement_text, scheme_id, content_type)
         VALUES ('seed', ?1, 'seeded', '{}', 'Hold state.', 'scheme', 'statement')",
        [0_u64.to_be_bytes()],
    ));
    let opened = event_pair(&catalog.connection, u64::MAX, "started");
    assert!(opened > 0);
    constraint(catalog.connection.execute(
        "UPDATE content_block SET block = X'01' WHERE id = ?1",
        [opened],
    ));
    constraint(catalog.connection.execute(
        "UPDATE content_block SET statement_text = 'Wrong class.' WHERE id = ?1",
        [opened],
    ));
}

#[test]
fn bindings_and_nouns_keep_counter_width_and_condition_nullability() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let block = seed_block(connection, 0);
    let id = binding(connection, block, 0);
    for field in ["ordinal", "start", "end", "verb_start", "verb_end"] {
        constraint(connection.execute(
            &format!("UPDATE statement_binding SET {field} = X'01' WHERE id = ?1"),
            [id],
        ));
    }
    for set in [
        "condition = 'step:14'",
        "condition_start = X'0000000000000000'",
        "condition_end = X'0000000000000001'",
    ] {
        constraint(connection.execute(&format!("UPDATE statement_binding SET {set}"), []));
    }
    connection.execute(
        "UPDATE statement_binding SET condition = 'step:14', condition_start = ?1, condition_end = ?2",
        params![0_u64.to_be_bytes(), 1_u64.to_be_bytes()],
    ).unwrap();
    connection
        .execute(
            "INSERT INTO binding_noun(binding_id, ordinal, value, start, end)
         VALUES (?1, ?2, 'state', ?2, ?3)",
            params![id, 0_u64.to_be_bytes(), 1_u64.to_be_bytes()],
        )
        .unwrap();
    for field in ["ordinal", "start", "end"] {
        constraint(connection.execute(&format!("UPDATE binding_noun SET {field} = X'01'"), []));
    }
}

#[test]
fn pair_fates_require_whole_terminal_references() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let statement = binding(connection, seed_block(connection, 0), 0);
    let opened = event_pair(connection, 1, "started");
    let ended = event_pair(connection, 2, "ended");
    connection
        .execute(
            "INSERT INTO tool_pair(run, sequence, call, call_block, statement_id,
         opened_event, lane, request, fate)
         VALUES ('run', ?1, ?2, ?2, ?3, ?4, 'external', '{}', 'opened')",
            params![1_u64.to_be_bytes(), 0_u64.to_be_bytes(), statement, opened],
        )
        .unwrap();
    for set in [
        "fate = 'completed'",
        "fate = 'failed'",
        "reason = 'timeout'",
        "lane = 'unknown'",
    ] {
        constraint(connection.execute(&format!("UPDATE tool_pair SET {set}"), []));
    }
    connection
        .execute(
            "UPDATE tool_pair SET fate = 'failed', reason = 'timeout', terminal_event = ?1",
            [ended],
        )
        .unwrap();
    constraint(connection.execute("UPDATE tool_pair SET terminal_event = NULL", []));
    connection
        .execute("UPDATE tool_pair SET fate = 'completed', reason = NULL", [])
        .unwrap();
    verify_integrity(connection).unwrap();
}

#[test]
fn spans_keep_the_seed_shape_unique_open_path_and_nonempty_members() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let block = seed_block(connection, 0);
    connection
        .execute(
            "INSERT INTO span(local_id, revision, kind, status)
         VALUES (?1, ?2, 'system_prompt', 'closed')",
            params![0_u64.to_be_bytes(), 1_u64.to_be_bytes()],
        )
        .unwrap();
    constraint(connection.execute("UPDATE span SET status = 'open'", []));
    constraint(connection.execute("UPDATE span SET revision = ?1", [2_u64.to_be_bytes()]));
    connection
        .execute(
            "INSERT INTO span(local_id, revision, kind, status, opened_content_id)
         VALUES (?1, ?2, 'task', 'open', ?3)",
            params![1_u64.to_be_bytes(), 1_u64.to_be_bytes(), block],
        )
        .unwrap();
    constraint(connection.execute(
        "INSERT INTO span(local_id, revision, kind, status, opened_content_id)
         VALUES (?1, ?2, 'task', 'open', ?3)",
        params![2_u64.to_be_bytes(), 1_u64.to_be_bytes(), block],
    ));
    for table in ["span_prefix", "span_member"] {
        let sql = format!(
            "INSERT INTO {table}(local_id, revision, ordinal, block_id, start, end)
                           VALUES (?1, ?2, ?1, ?3, ?1, ?4)"
        );
        constraint(connection.execute(
            &sql,
            params![
                0_u64.to_be_bytes(),
                1_u64.to_be_bytes(),
                block,
                0_u64.to_be_bytes()
            ],
        ));
        connection
            .execute(
                &sql,
                params![
                    0_u64.to_be_bytes(),
                    1_u64.to_be_bytes(),
                    block,
                    11_u64.to_be_bytes()
                ],
            )
            .unwrap();
    }
}

#[test]
fn observations_are_separate_and_filing_metadata_keeps_its_source() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    connection
        .execute("INSERT INTO observation_run VALUES ('run', '{}')", [])
        .unwrap();
    let sql =
        "INSERT INTO operation_observation(run, operation_id, request_id, operation, observation)
               VALUES ('run', ?1, ?2, ?3, '{}')";
    constraint(connection.execute(
        sql,
        params![0_u64.to_be_bytes(), 0_u64.to_be_bytes(), "open"],
    ));
    constraint(connection.execute(
        sql,
        params![1_u64.to_be_bytes(), 0_u64.to_be_bytes(), "file"],
    ));
    constraint(connection.execute(
        sql,
        params![1_u64.to_be_bytes(), 0_u64.to_be_bytes(), "unknown"],
    ));
    connection
        .execute(
            sql,
            params![1_u64.to_be_bytes(), 0_u64.to_be_bytes(), "open"],
        )
        .unwrap();
    let content: i64 = connection
        .query_row("SELECT COUNT(*) FROM content_block", [], |row| row.get(0))
        .unwrap();
    assert_eq!(content, 0);
    constraint(connection.execute("UPDATE operation_observation SET source_run = 'run'", []));
}

#[test]
fn plan_catalog_enforces_one_unfinished_plan_and_one_active_step() {
    let catalog = ServingCatalog::create().unwrap();
    let connection = &catalog.connection;
    let block = seed_block(connection, 0);
    let statement = binding(connection, block, 0);
    closed_span(connection, 1, block);
    closed_span(connection, 2, block);
    let revision = "INSERT INTO plan_revision(plan_id, revision, task_id, task_revision,
                    prompt_id, prompt_revision, status, operator_source, lock_source,
                    last_source, review_setup, group_drafts)
                    VALUES (?1, ?2, ?2, ?2, ?3, ?2, 'locked', ?4, ?4, ?4, '{}', '[]')";
    connection
        .execute(
            revision,
            params![
                1_u64.to_be_bytes(),
                1_u64.to_be_bytes(),
                2_u64.to_be_bytes(),
                block
            ],
        )
        .unwrap();
    constraint(connection.execute(
        revision,
        params![
            2_u64.to_be_bytes(),
            1_u64.to_be_bytes(),
            2_u64.to_be_bytes(),
            block
        ],
    ));
    connection
        .execute(
            "INSERT INTO plan_category VALUES (?1, ?1, 'code', ?2, '{}')",
            params![1_u64.to_be_bytes(), 0_u64.to_be_bytes()],
        )
        .unwrap();
    let step = "INSERT INTO plan_step(plan_id, revision, step_id, ordinal, instruction_id,
                category_id, status, span_id, span_revision)
                VALUES (?1, ?1, ?2, ?2, ?3, 'code', 'active', ?1, ?1)";
    connection
        .execute(
            step,
            params![1_u64.to_be_bytes(), 1_u64.to_be_bytes(), statement],
        )
        .unwrap();
    constraint(connection.execute(
        step,
        params![1_u64.to_be_bytes(), 2_u64.to_be_bytes(), statement],
    ));
    constraint(connection.execute("UPDATE plan_step SET status = 'closed'", []));
    constraint(connection.execute("UPDATE plan_step SET category_id = 'missing'", []));
    constraint(connection.execute("UPDATE plan_step SET step_id = ?1", [0_u64.to_be_bytes()]));
    verify_integrity(connection).unwrap();
}
