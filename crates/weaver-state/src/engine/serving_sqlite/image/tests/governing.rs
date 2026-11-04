use super::*;
use weaver_types::state_flow::{Digest, Ste100Scheme};

fn thesaurus(scheme: &str) -> String {
    format!(
        "\r\n{}\n",
        serde_json::json!({
            "format": "ste100-thesaurus/1", "scheme_id": scheme,
            "source": {"repository": "synthetic", "commit": "test", "path": "test",
                "sha256": sha256(b"synthetic source")},
            "entries": [{"word": "commence", "tag": "splice",
                "senses": [{"sense": "begin", "use": ["start"]}]}]
        })
    )
}

fn adopt(
    bytes: &[u8],
    scheme: &str,
    thesaurus: Option<&Digest>,
) -> Result<ServingSqlite, super::super::OpenFault> {
    ServingSqlite::adopt(
        bytes,
        &common(),
        "state-content/2",
        scheme,
        thesaurus,
        &identity(bytes),
        limits(),
    )
}

#[test]
fn governing_seed_keeps_exact_versions_with_and_without_checker_or_thesaurus() {
    for checker in [false, true] {
        for elected in [false, true] {
            let mut request = fixture();
            if !checker {
                request.checker_artifact_sha256 = None;
                request.certificate.checker_artifact_sha256 = None;
                request.bindings.clear();
                request.certificate.bindings.clear();
            }
            request.ste100_thesaurus = elected.then(|| thesaurus(&request.scheme_id));
            let expected = request
                .ste100_thesaurus
                .as_ref()
                .map(|s| sha256(s.as_bytes()));
            let image = seed_image(&request).unwrap();
            let db = connection(image.as_bytes());
            let rows = db
                .prepare(
                    "SELECT kind,sha256,bytes,source_sha256,ruling,install_ordinal
                 FROM governing_version ORDER BY install_ordinal",
                )
                .unwrap()
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(rows.len(), 1 + usize::from(elected));
            assert_eq!(
                rows[0],
                (
                    "scheme".into(),
                    sha256(request.ste100_scheme.as_bytes()).to_string(),
                    request.ste100_scheme.as_bytes().to_vec(),
                    None,
                    "seed".into(),
                    0_u64.to_be_bytes().to_vec()
                )
            );
            if let Some(bytes) = &request.ste100_thesaurus {
                assert_eq!(
                    rows[1],
                    (
                        "thesaurus".into(),
                        sha256(bytes.as_bytes()).to_string(),
                        bytes.as_bytes().to_vec(),
                        Some(sha256(b"synthetic source").to_string()),
                        "seed".into(),
                        1_u64.to_be_bytes().to_vec()
                    )
                );
            }
            let store = adopt(image.as_bytes(), &request.scheme_id, expected.as_ref()).unwrap();
            let prompt =
                serde_json::to_string(&store.query(&ReadSelector::SystemPrompt {}).unwrap())
                    .unwrap();
            assert!(prompt.contains("Read item."));
            assert_eq!(store.held().unwrap().0, 1);
            let saved = store.image().unwrap();
            adopt(&saved, &request.scheme_id, expected.as_ref()).unwrap();
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM statement_binding", [], |row| row
                    .get::<_, i64>(0))
                    .unwrap(),
                i64::from(checker)
            );
        }
    }
}

#[test]
fn governing_open_requires_the_exact_elected_current_versions() {
    let mut request = fixture();
    request.ste100_thesaurus = Some(thesaurus(&request.scheme_id));
    let digest = sha256(request.ste100_thesaurus.as_ref().unwrap().as_bytes());
    let image = seed_image(&request).unwrap();
    for expected in [None, Some(sha256(b"other"))] {
        let fault = adopt(image.as_bytes(), &request.scheme_id, expected.as_ref())
            .err()
            .unwrap();
        assert_eq!(fault.code, StateRefusalCode::SchemeMismatch);
        assert!(fault.detail.contains("thesaurus"));
    }
    for sql in [
        "DELETE FROM governing_current WHERE kind='scheme'",
        "DELETE FROM governing_current WHERE kind='thesaurus'",
        "UPDATE governing_version SET bytes=CAST(bytes || ' ' AS BLOB) WHERE kind='scheme'",
        "UPDATE governing_version SET bytes=CAST(bytes || ' ' AS BLOB) WHERE kind='thesaurus'",
        "UPDATE governing_version SET source_sha256=printf('%064d',0) WHERE kind='thesaurus'",
    ] {
        let db = connection(image.as_bytes());
        db.execute_batch(sql).unwrap();
        let changed = db.serialize(rusqlite::MAIN_DB).unwrap();
        let fault = adopt(&changed, &request.scheme_id, Some(&digest))
            .err()
            .unwrap();
        assert_eq!(
            fault.code,
            StateRefusalCode::SchemeMismatch,
            "{sql}: {fault}"
        );
        assert!(fault.detail.contains(if sql.contains("thesaurus") {
            "thesaurus"
        } else {
            "scheme"
        }));
    }
    request.ste100_thesaurus = None;
    let image = seed_image(&request).unwrap();
    assert_eq!(
        adopt(image.as_bytes(), &request.scheme_id, Some(&digest))
            .err()
            .unwrap()
            .code,
        StateRefusalCode::SchemeMismatch
    );
}

fn add_scheme(db: &Connection, source: &str) -> String {
    let mut artifact: Ste100Scheme = decode_strict(source).unwrap();
    artifact.verbs[0].key = "read_later".into();
    artifact.recall_keys = vec!["read_later".into()];
    let bytes = serde_json::to_vec(&artifact).unwrap();
    let digest = sha256(&bytes).to_string();
    db.execute(
        "INSERT INTO governing_version VALUES('scheme',?1,?2,NULL,'test',?3)",
        params![digest, bytes, 2_u64.to_be_bytes()],
    )
    .unwrap();
    db.execute(
        "UPDATE governing_current SET sha256=?1 WHERE kind='scheme'",
        [&digest],
    )
    .unwrap();
    let scheme = format!("ste100:{digest}");
    db.execute("UPDATE store_meta SET scheme_id=?1", [&scheme])
        .unwrap();
    scheme
}

#[test]
fn governing_history_checks_saved_bindings_against_their_own_dictionary() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let db = connection(image.as_bytes());
    let current = add_scheme(&db, &request.ste100_scheme);
    let changed = db.serialize(rusqlite::MAIN_DB).unwrap();
    let mut store = adopt(&changed, &current, None).unwrap();
    let mut filing = super::super::super::tests::filing::statement(1, "message.assistant");
    filing.scheme_id = current.clone();
    assert!(
        store.file(&filing).is_err(),
        "old dictionary is not the current one"
    );
    filing.bindings[0].verb = "read_later".into();
    store.file(&filing).unwrap();
    let saved = store.image().unwrap();
    let mut start = identity(&saved);
    start.position = store.position().unwrap();
    ServingSqlite::adopt(
        &saved,
        &common(),
        "state-content/2",
        &current,
        None,
        &start,
        limits(),
    )
    .unwrap();
    db.execute("UPDATE statement_binding SET verb='read_later'", [])
        .unwrap();
    let changed = db.serialize(rusqlite::MAIN_DB).unwrap();
    assert_eq!(
        adopt(&changed, &current, None).err().unwrap().code,
        StateRefusalCode::OpenFailed,
        "current dictionary cannot license historical bindings"
    );
}

#[test]
fn governing_current_thesaurus_must_follow_the_current_scheme() {
    let mut request = fixture();
    request.ste100_thesaurus = Some(thesaurus(&request.scheme_id));
    let digest = sha256(request.ste100_thesaurus.as_ref().unwrap().as_bytes());
    let image = seed_image(&request).unwrap();
    let db = connection(image.as_bytes());
    let current = add_scheme(&db, &request.ste100_scheme);
    let changed = db.serialize(rusqlite::MAIN_DB).unwrap();
    let fault = adopt(&changed, &current, Some(&digest)).err().unwrap();
    assert_eq!(fault.code, StateRefusalCode::SchemeMismatch);
    assert!(fault.detail.contains("thesaurus"));
}

#[test]
fn governing_history_bytes_are_checked_even_when_the_version_is_not_current() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let db = connection(image.as_bytes());
    let current = add_scheme(&db, &request.ste100_scheme);
    db.execute(
        "UPDATE governing_version SET bytes=CAST(bytes || ' ' AS BLOB)
        WHERE kind='scheme' AND sha256=?1",
        [sha256(request.ste100_scheme.as_bytes()).to_string()],
    )
    .unwrap();
    let changed = db.serialize(rusqlite::MAIN_DB).unwrap();
    let fault = adopt(&changed, &current, None).err().unwrap();
    assert_eq!(fault.code, StateRefusalCode::SchemeMismatch);
    assert!(fault.detail.contains("scheme"));
}

#[test]
fn governing_adoption_refuses_the_retired_image_even_with_a_new_open_schema() {
    let db = super::super::super::open_memory().unwrap();
    for ddl in &super::super::super::schema::DDL[..27] {
        db.execute(&ddl.replace("state-content/2", "state-content/1"), [])
            .unwrap();
    }
    let bytes = db.serialize(rusqlite::MAIN_DB).unwrap();
    let request = fixture();
    let fault = adopt(&bytes, &request.scheme_id, None).err().unwrap();
    assert_eq!(fault.code, StateRefusalCode::UnsupportedSchema);
    assert!(fault.detail.contains("state-content/1"));
}

#[test]
fn governing_adoption_checks_artifact_grammar_even_with_rebound_outer_hashes() {
    let request = fixture();
    let image = seed_image(&request).unwrap();
    let db = connection(image.as_bytes());
    let mut artifact: Ste100Scheme = decode_strict(&request.ste100_scheme).unwrap();
    artifact.grammar.sha256 = sha256(b"wrong grammar");
    let bytes = serde_json::to_vec(&artifact).unwrap();
    let digest = sha256(&bytes).to_string();
    db.execute(
        "INSERT INTO governing_version VALUES('scheme',?1,?2,NULL,'test',?3)",
        params![digest, bytes, 1_u64.to_be_bytes()],
    )
    .unwrap();
    db.execute(
        "UPDATE governing_current SET sha256=?1 WHERE kind='scheme'",
        [&digest],
    )
    .unwrap();
    let scheme = format!("ste100:{digest}");
    db.execute("UPDATE store_meta SET scheme_id=?1", [&scheme])
        .unwrap();
    let bytes = db.serialize(rusqlite::MAIN_DB).unwrap();
    let fault = adopt(&bytes, &scheme, None).err().unwrap();
    assert_eq!(fault.code, StateRefusalCode::SchemeMismatch);
    assert!(fault.detail.contains("scheme"));
}
