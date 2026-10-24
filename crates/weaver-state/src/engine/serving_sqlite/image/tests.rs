use super::super::ServingSqlite;
use super::*;
use crate::ServingStore;
use weaver_types::state_flow::{Binding, NounBinding, PromptDuty, SeedCertificate, Span};
use weaver_types::state_flow::{Common, ReadLimits, ReadSelector, StartIdentity, StateRefusalCode};

pub(in crate::engine::serving_sqlite) fn fixture() -> SeedRequest {
    // This fixture proves the byte contract only. It is not a qualified language
    // artifact, model, prompt or production conformance certificate.
    let artifact = serde_json::json!({
        "format":"ste100-scheme/1",
        "reference":{"edition":"synthetic-test", "document_sha256":sha256(b"reference"),
            "rule_ids":["test-rule"],"dictionary_sha256":sha256(b"dictionary")},
        "grammar":{"format":"test-grammar/1","bytes_base64":"Z3JhbW1hcg==",
            "sha256":sha256(b"grammar")},
        "verbs":[{"key":"read","forms":["Read"],"sense":"test",
            "authority":{"rule_id":"test-rule","disposition":"approved_word"}}],
        "technical_names":[],
        "conditions":{"format":"test-conditions/1","bytes_base64":"Y29uZGl0aW9ucw==",
            "sha256":sha256(b"conditions")},
        "recall_keys":["read"],
        "qualification":{"report_sha256":sha256(b"report"),
            "corpus_sha256":sha256(b"corpus"),"checker_abi":"test/1"}
    })
    .to_string();
    let scheme_id = format!("ste100:{}", sha256(artifact.as_bytes()));
    let bindings = vec![Binding {
        statement_span: Span {
            start: 0.into(),
            end: 10.into(),
        },
        verb: "read".into(),
        verb_span: Span {
            start: 0.into(),
            end: 4.into(),
        },
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: "item".into(),
            span: Span {
                start: 5.into(),
                end: 9.into(),
            },
        }],
    }];
    let certificate = SeedCertificate {
        prompt_sha256: sha256(b"Read item."),
        scheme_id: scheme_id.clone(),
        checker_artifact_sha256: sha256(b"checker"),
        review_reference: "test fixture, no production qualification".into(),
        bindings: bindings.clone(),
        duties: PromptDuty::ALL.to_vec(),
    };
    SeedRequest {
        schema_id: "state-content/1".into(),
        seed_id: "a".repeat(32),
        scheme_id,
        ste100_scheme: artifact,
        checker_artifact_sha256: sha256(b"checker"),
        prompt_text: "Read item.".into(),
        raw_block: serde_json::value::RawValue::from_string(
            r#"{"type":"text","text":"Read item."}"#.into(),
        )
        .unwrap(),
        bindings,
        certificate,
    }
}

fn connection(image: &[u8]) -> Connection {
    let mut connection = super::super::open_memory().unwrap();
    connection
        .deserialize_read_exact(rusqlite::MAIN_DB, image, image.len(), false)
        .unwrap();
    super::super::configure(&connection).unwrap();
    connection
}

#[test]
fn seed_contains_exact_prompt_source_binding_and_complete_closed_span() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let db = connection(image.as_bytes());
    super::super::verify_schema(&db).unwrap();
    super::super::verify_integrity(&db).unwrap();
    let row: (String, String, String, String, Option<String>) = db
        .query_row(
            "SELECT source_kind,admission,raw_block,statement_text,context_raw FROM content_block",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "seed".into(),
            "seeded".into(),
            request.raw_block.get().into(),
            request.prompt_text,
            None
        )
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM content_block", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM statement_binding", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT value FROM binding_noun", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "item"
    );
    assert_eq!(
        db.query_row("SELECT kind || ':' || status FROM span", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "system_prompt:closed"
    );
    let differences: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM (SELECT * FROM span_prefix EXCEPT SELECT * FROM span_member)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(differences, 0);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM span_member WHERE start=X'0000000000000000' AND end=X'000000000000000A' AND step_query IS NULL",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}

#[test]
fn seed_refusals_preserve_schema_shape_certificate_order_and_hash_attribution() {
    let mut request = fixture();
    request.schema_id = "old".into();
    request.seed_id.clear();
    request.ste100_scheme = "invalid".into();
    assert_eq!(
        seed_image(&request).err().unwrap().code,
        SeedRefusal::UnsupportedSchema
    );
    request.schema_id = "state-content/1".into();
    assert_eq!(
        seed_image(&request).err().unwrap().code,
        SeedRefusal::MalformedSeed
    );
    request.seed_id = "a".repeat(32);
    assert_eq!(
        seed_image(&request).err().unwrap().code,
        SeedRefusal::SeedCertificateInvalid
    );
    for field in ["prompt", "scheme", "checker", "duty", "binding", "grammar"] {
        let mut request = fixture();
        match field {
            "prompt" => request.certificate.prompt_sha256 = sha256(b"wrong"),
            "scheme" => request.scheme_id = format!("ste100:{}", sha256(b"wrong")),
            "checker" => request.checker_artifact_sha256 = sha256(b"wrong"),
            "duty" => {
                request.certificate.duties.pop();
            }
            "binding" => {
                request.bindings[0].nouns[0].span.start = 6.into();
                request.certificate.bindings = request.bindings.clone();
            }
            "grammar" => {
                let mut artifact: Ste100Scheme = decode_strict(&request.ste100_scheme).unwrap();
                artifact.grammar.sha256 = sha256(b"wrong");
                request.ste100_scheme = serde_json::to_string(&artifact).unwrap();
                request.scheme_id = format!("ste100:{}", sha256(request.ste100_scheme.as_bytes()));
                request.certificate.scheme_id = request.scheme_id.clone();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            seed_image(&request).err().unwrap().code,
            SeedRefusal::SeedCertificateInvalid,
            "{field}"
        );
    }
}

#[test]
fn image_limit_refuses_whole_before_copy_and_accepts_exact_boundary() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let db = connection(image.as_bytes());
    let size = db.serialize(rusqlite::MAIN_DB).unwrap().len() as u64;
    assert_eq!(
        bounded_image(&db, size - 1).unwrap_err().code,
        SeedRefusal::ImageTooLarge
    );
    assert_eq!(bounded_image(&db, size).unwrap().len() as u64, size);
}

fn identity(image: &[u8]) -> StartIdentity {
    StartIdentity {
        seed_id: "a".repeat(32),
        artifact_id: "seed-test".into(),
        dump_sha256: sha256(image),
        config_sha256: sha256(b"admin owns B"),
        parent_artifact_id: None,
        position: None,
    }
}
fn common() -> Common {
    Common {
        session_id: "session".into(),
        run_id: "run".into(),
        request_id: 1.into(),
    }
}
fn limits() -> ReadLimits {
    ReadLimits {
        max_result_bytes: 1_000_000.into(),
        measurement_ref: "fixture only".into(),
    }
}

#[test]
fn adopted_seed_reads_prompt_without_filing_and_roundtrips_verified_image() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let start = identity(image.as_bytes());
    let store = ServingSqlite::adopt(
        image.as_bytes(),
        &common(),
        &request.schema_id,
        &request.scheme_id,
        &start,
        limits(),
    )
    .unwrap();
    assert_eq!(store.held().unwrap().0, 1);
    assert_eq!(store.position().unwrap(), None);
    let prompt = store.query(&ReadSelector::SystemPrompt {}).unwrap();
    let bytes = serde_json::to_string(&prompt).unwrap();
    assert!(bytes.contains("Read item."));
    assert_eq!(store.held().unwrap().0, 1);
    let captured = store.image().unwrap();
    let restored = ServingSqlite::adopt(
        &captured,
        &common(),
        &request.schema_id,
        &request.scheme_id,
        &identity(&captured),
        limits(),
    )
    .unwrap();
    assert_eq!(
        restored
            .query_encoded(&ReadSelector::SystemPrompt {}, None)
            .unwrap(),
        bytes.as_bytes()
    );
}

#[test]
fn adoption_refuses_corruption_even_when_image_hash_is_rebound() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    for sql in [
        "CREATE TABLE surprise(x)",
        "DELETE FROM store_meta",
        "UPDATE store_meta SET seed_id='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'",
        "UPDATE content_block SET raw_block='{\"type\":\"text\",\"text\":\"wrong\"}'",
        "UPDATE statement_binding SET verb_start=X'0000000000000063'",
        "UPDATE statement_binding SET ordinal=X'0000000000000001'",
        "UPDATE binding_noun SET value='other'",
        "UPDATE binding_noun SET ordinal=X'0000000000000001'",
        "PRAGMA foreign_keys=OFF;
         UPDATE binding_noun SET binding_id=-binding_id;
         UPDATE statement_binding SET id=-id;",
        "DELETE FROM span_member",
        "DELETE FROM span_prefix",
        "UPDATE span_member SET end=X'0000000000000009'",
        "UPDATE span_prefix SET start=X'0000000000000001'",
        "UPDATE content_block SET scheme_id='other'",
    ] {
        let db = connection(image.as_bytes());
        db.execute_batch(sql).unwrap();
        let bytes = bounded_image(&db, crate::save_point::SAVE_POINT_BOUND).unwrap();
        let error = ServingSqlite::adopt(
            &bytes,
            &common(),
            &request.schema_id,
            &request.scheme_id,
            &identity(&bytes),
            limits(),
        )
        .err()
        .expect(sql);
        assert_eq!(error.code, StateRefusalCode::OpenFailed, "{sql}");
    }
}

#[test]
fn adoption_distinguishes_scheme_limits_and_image_identity_failures() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let start = identity(image.as_bytes());
    let open = |schema: &str, scheme: &str, start: &StartIdentity, limits: ReadLimits| {
        ServingSqlite::adopt(image.as_bytes(), &common(), schema, scheme, start, limits)
            .err()
            .unwrap()
            .code
    };
    assert_eq!(
        open("old", &request.scheme_id, &start, limits()),
        StateRefusalCode::UnsupportedSchema
    );
    assert_eq!(
        open(&request.schema_id, "other", &start, limits()),
        StateRefusalCode::SchemeMismatch
    );
    let mut invalid = limits();
    invalid.max_result_bytes = 0.into();
    assert_eq!(
        open(&request.schema_id, &request.scheme_id, &start, invalid),
        StateRefusalCode::InvalidReadLimits
    );
    let mut wrong = start.clone();
    wrong.dump_sha256 = sha256(b"wrong");
    assert_eq!(
        open(&request.schema_id, &request.scheme_id, &wrong, limits()),
        StateRefusalCode::OpenFailed
    );
    wrong = start;
    wrong.position = Some(weaver_types::state_flow::SourcePosition {
        block_id: weaver_types::state_flow::SourceBlockId {
            run: "old".into(),
            sequence: 1.into(),
            block: 0.into(),
        },
        turn: None,
    });
    assert_eq!(
        open(&request.schema_id, &request.scheme_id, &wrong, limits()),
        StateRefusalCode::OpenFailed
    );
}

#[test]
fn image_adoption_keeps_committed_order_across_new_runs_and_refuses_old_run_writes() {
    use weaver_types::state_flow::*;
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let mut store = ServingSqlite::adopt(
        image.as_bytes(),
        &common(),
        &request.schema_id,
        &request.scheme_id,
        &identity(image.as_bytes()),
        limits(),
    )
    .unwrap();
    let statement = |run: &str, sequence: u64| StatementFile {
        block_id: SourceBlockId {
            run: run.into(),
            sequence: sequence.into(),
            block: 0.into(),
        },
        source: SourceEnvelope {
            session: "session".into(),
            run: run.into(),
            turn: Some("turn".into()),
            sequence: sequence.into(),
            kind: "message.assistant".into(),
            channel: "output".into(),
            origin: "model".into(),
            subsystem: "harness".into(),
            causal_parent: None,
            wall_ms: 0,
            monotonic_ns: 1.into(),
        },
        partition_rule_id: "text".into(),
        source_path: vec![
            SourcePathPart::Key("content".into()),
            SourcePathPart::Index { index: 0.into() },
        ],
        admission: Admission::Admitted,
        raw_block: request.raw_block.clone(),
        text: request.prompt_text.clone(),
        scheme_id: request.scheme_id.clone(),
        bindings: request.bindings.clone(),
        context: None,
        effect: None,
    };
    store.file(&statement("run", u64::MAX)).unwrap();
    let captured = store.image().unwrap();
    let mut start = identity(&captured);
    start.position = store.position().unwrap();
    let mut next_common = common();
    next_common.run_id = "next".into();
    let mut next = ServingSqlite::adopt(
        &captured,
        &next_common,
        &request.schema_id,
        &request.scheme_id,
        &start,
        limits(),
    )
    .unwrap();
    assert_eq!(
        next.file(&statement("run", 1)).unwrap_err().code,
        StateRefusalCode::WrongRun
    );
    next.file(&statement("next", 1)).unwrap();
    let QueryResult::Entries { entries } = next
        .query(&ReadSelector::History {
            noun: "item".into(),
            verb: None,
            condition: None,
        })
        .unwrap()
    else {
        panic!("history entries")
    };
    assert_eq!(entries.len(), 3);
    assert!(matches!(&entries[0].source, ReadSource::Seed(_)));
    let ReadSource::Event(first) = &entries[1].source else {
        panic!("old source")
    };
    let ReadSource::Event(second) = &entries[2].source else {
        panic!("new source")
    };
    assert_eq!(first.block_id.sequence.0, u64::MAX);
    assert_eq!(second.block_id.run, "next");
    assert_eq!(next.position().unwrap().unwrap().block_id, second.block_id);
}

#[test]
fn malformed_recall_adoption_refuses_exact_shape_even_with_consistent_pair_and_hash() {
    use super::super::filing::tests::{opening, pair_file, statement};
    use weaver_types::state_flow::*;
    for (compound, conditional, subject) in [
        (true, false, "item"),
        (false, true, "item"),
        (true, true, "item"),
        (false, false, "other"),
        (false, false, "Item"),
        (false, false, "item"),
    ] {
        let request = fixture();
        let seed = seed_image(&request).unwrap();
        let mut store = ServingSqlite::adopt(
            seed.as_bytes(),
            &common(),
            &request.schema_id,
            &request.scheme_id,
            &identity(seed.as_bytes()),
            limits(),
        )
        .unwrap();
        let mut send = statement(1, "message.assistant");
        send.scheme_id = request.scheme_id.clone();
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
        // An external send may legitimately use this binding. Rewriting only
        // the consistent pair lane/request models an image forged around filing.
        let mut started = opening(&send);
        let mut filed = pair_file(2, PairRecord::Started(started.clone()), "model");
        filed.scheme_id = request.scheme_id.clone();
        store.file_pair(&filed).unwrap();
        started.lane = PairLane::Recall;
        started.request = PairRequest::Recall {
            subject: subject.into(),
        };
        started.call_block = 0.into();
        let db = connection(&store.image().unwrap());
        db.execute(
            "UPDATE tool_pair SET lane='recall', call_block=?1, request=?2",
            params![
                0_u64.to_be_bytes(),
                serde_json::to_string(&started.request).unwrap()
            ],
        )
        .unwrap();
        db.execute(
            "UPDATE content_block SET raw_block=?1 WHERE id=(SELECT opened_event FROM tool_pair)",
            [serde_json::to_string(&started).unwrap()],
        )
        .unwrap();
        let image = bounded_image(&db, crate::save_point::SAVE_POINT_BOUND).unwrap();
        let mut start = identity(&image);
        start.position = store.position().unwrap();
        let adopted = ServingSqlite::adopt(
            &image,
            &common(),
            &request.schema_id,
            &request.scheme_id,
            &start,
            limits(),
        );
        if compound || conditional || subject != "item" {
            let error = adopted
                .err()
                .expect("invalid recall shape must refuse adoption");
            assert_eq!(error.code, StateRefusalCode::OpenFailed);
            assert!(error.detail.contains("single held noun"), "{error}");
        } else {
            assert!(
                adopted.is_ok(),
                "valid single unconditional subject remains adoptable"
            );
        }
    }
}

#[test]
fn abandoned_pair_adoption_requires_a_prior_run_even_with_rebound_image_identity() {
    use super::super::filing::tests::{opening, pair_file, statement};
    use weaver_types::state_flow::*;
    let request = fixture();
    let seed = seed_image(&request).unwrap();
    let mut store = ServingSqlite::adopt(
        seed.as_bytes(),
        &common(),
        &request.schema_id,
        &request.scheme_id,
        &identity(seed.as_bytes()),
        limits(),
    )
    .unwrap();
    let mut send = statement(1, "message.assistant");
    send.scheme_id = request.scheme_id.clone();
    store.file(&send).unwrap();
    let started = opening(&send);
    let mut start_file = pair_file(2, PairRecord::Started(started.clone()), "model");
    start_file.scheme_id = request.scheme_id.clone();
    store.file_pair(&start_file).unwrap();

    let loaded_image = store.image().unwrap();
    let mut loaded_from = identity(&loaded_image);
    loaded_from.position = store.position().unwrap();
    let next = Common {
        run_id: "next-run".into(),
        ..common()
    };
    let mut reloaded = ServingSqlite::adopt(
        &loaded_image,
        &next,
        &request.schema_id,
        &request.scheme_id,
        &loaded_from,
        limits(),
    )
    .unwrap();
    let mut terminal = pair_file(
        3,
        PairRecord::Ended(PairEnded {
            call_id: started.call_id,
            execution: None,
            fate: TerminalPairFate::Failed,
            reason: Some(PairFailure::AbandonedAtLoad {}),
            outcome: None,
            loaded_from: Some(loaded_from),
        }),
        "loop",
    );
    terminal.scheme_id = request.scheme_id.clone();
    terminal.block_id.run = next.run_id.clone();
    terminal.source.run = next.run_id.clone();
    terminal.source.turn = None;
    reloaded.file_pair(&terminal).unwrap();
    let captured = reloaded.image().unwrap();
    let mut start = identity(&captured);
    start.position = reloaded.position().unwrap();
    let restored = ServingSqlite::adopt(
        &captured,
        &next,
        &request.schema_id,
        &request.scheme_id,
        &start,
        limits(),
    )
    .unwrap();
    assert!(matches!(
        restored.query(&ReadSelector::OpenPairs {}).unwrap(),
        QueryResult::OpenPairs { pairs } if pairs.is_empty()
    ));

    // Forge only the repair event's run. Its CallId now names the same run,
    // while the indexed source, raw envelope, hash and frontier remain aligned.
    let db = connection(&captured);
    assert_eq!(
        db.execute(
            "UPDATE content_block SET run='run', envelope=json_set(envelope,'$.run','run')
             WHERE id=(SELECT terminal_event FROM tool_pair)",
            [],
        )
        .unwrap(),
        1
    );
    let corrupted = bounded_image(&db, crate::save_point::SAVE_POINT_BOUND).unwrap();
    start.dump_sha256 = sha256(&corrupted);
    start.position.as_mut().unwrap().block_id.run = "run".into();
    let error = ServingSqlite::adopt(
        &corrupted,
        &next,
        &request.schema_id,
        &request.scheme_id,
        &start,
        limits(),
    )
    .err()
    .expect("same-run abandonment must not enter through a saved image");
    assert_eq!(error.code, StateRefusalCode::OpenFailed);
    assert!(error.detail.contains("abandoned pair"), "{error}");
}

#[test]
fn pair_image_roundtrip_keeps_whole_result_and_rejects_index_divergence() {
    use super::super::filing::tests::{opening, pair_file, statement};
    use weaver_types::state_flow::*;
    let request = fixture();
    let seed = seed_image(&request).unwrap();
    let mut store = ServingSqlite::adopt(
        seed.as_bytes(),
        &common(),
        &request.schema_id,
        &request.scheme_id,
        &identity(seed.as_bytes()),
        limits(),
    )
    .unwrap();
    let mut send = statement(1, "message.assistant");
    send.scheme_id = request.scheme_id.clone();
    store.file(&send).unwrap();
    let started = opening(&send);
    let mut start_file = pair_file(2, PairRecord::Started(started.clone()), "model");
    start_file.scheme_id = request.scheme_id.clone();
    store.file_pair(&start_file).unwrap();
    let mut end_file = pair_file(
        3,
        PairRecord::Ended(PairEnded {
            call_id: started.call_id,
            execution: Some(weaver_types::ExchangeId {
                opener: weaver_types::Opener::Harness,
                ordinal: 1,
            }),
            fate: TerminalPairFate::Completed,
            reason: None,
            outcome: Some(weaver_types::ToolOutcome::Result {
                content: "complete return".into(),
            }),
            loaded_from: None,
        }),
        "tool",
    );
    end_file.scheme_id = request.scheme_id.clone();
    store.file_pair(&end_file).unwrap();
    let captured = store.image().unwrap();
    let mut start = identity(&captured);
    start.position = store.position().unwrap();
    let restored = ServingSqlite::adopt(
        &captured,
        &common(),
        &request.schema_id,
        &request.scheme_id,
        &start,
        limits(),
    )
    .unwrap();
    let encoded = restored
        .query_encoded(
            &ReadSelector::Statement {
                statement: StatementRef {
                    block_id: send.block_id,
                    binding: 0.into(),
                },
            },
            None,
        )
        .unwrap();
    assert!(
        String::from_utf8(encoded)
            .unwrap()
            .contains("complete return")
    );
    for sql in [
        "UPDATE tool_pair SET call_block=X'0000000000000009'",
        "DELETE FROM tool_pair",
        "UPDATE tool_pair SET statement_id=(SELECT MIN(id) FROM statement_binding)",
        "UPDATE pair_event SET record_type='ended' WHERE id=(SELECT opened_event FROM tool_pair)",
    ] {
        let db = connection(&captured);
        db.execute(sql, []).unwrap();
        let bytes = bounded_image(&db, crate::save_point::SAVE_POINT_BOUND).unwrap();
        let mut start = start.clone();
        start.dump_sha256 = sha256(&bytes);
        assert_eq!(
            ServingSqlite::adopt(
                &bytes,
                &common(),
                &request.schema_id,
                &request.scheme_id,
                &start,
                limits()
            )
            .err()
            .expect(sql)
            .code,
            StateRefusalCode::OpenFailed
        );
    }
}
