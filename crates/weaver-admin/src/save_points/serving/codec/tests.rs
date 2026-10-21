use super::*;
use serde_json::{Value, json};
use weaver_types::state_flow::SourceBlockId;

fn digest(byte: char) -> Digest {
    Digest::new(byte.to_string().repeat(64)).unwrap()
}

fn artifact(ordinal: u64, byte: char) -> ArtifactId {
    ArtifactId::from_parts(DecimalU64(ordinal), digest(byte)).unwrap()
}

fn seed() -> StateBinding {
    StateBinding {
        version: 1,
        schema_id: "state-content/1".into(),
        seed_id: "a".repeat(32),
        dump_sha256: digest('1'),
        config_sha256: digest('2'),
        parent_artifact_id: None,
        position: None,
        confirmation: Confirmation::Seed {
            seed_id: "a".repeat(32),
        },
    }
}

fn serving() -> StateBinding {
    StateBinding {
        parent_artifact_id: Some(artifact(1, '2')),
        position: Some(SourcePosition {
            block_id: SourceBlockId {
                run: "source-run".into(),
                sequence: DecimalU64(u64::MAX),
                block: DecimalU64(0),
            },
            turn: None,
        }),
        confirmation: Confirmation::Serving {
            run_id: "capture-run".into(),
            capture_id: DecimalU64(u64::MAX),
        },
        config_sha256: digest('3'),
        ..seed()
    }
}

fn rebind() -> StateBinding {
    let parent = artifact(1, '2');
    StateBinding {
        parent_artifact_id: Some(parent.clone()),
        confirmation: Confirmation::Rebind {
            parent_artifact_id: parent.to_string(),
        },
        ..serving()
    }
}

fn publication(ordinal: u64, binding: &StateBinding, cause: PersistenceCause) -> StateIndexEntry {
    StateIndexEntry {
        ordinal: DecimalU64(ordinal),
        artifact_id: ArtifactId::from_parts(DecimalU64(ordinal), binding.config_sha256.clone())
            .unwrap(),
        dump_sha256: binding.dump_sha256.clone(),
        config_sha256: binding.config_sha256.clone(),
        seed_id: binding.seed_id.clone(),
        parent_artifact_id: binding.parent_artifact_id.clone(),
        cause,
        position: binding.position.clone(),
        confirmation: binding.confirmation.clone(),
        arrival: Arrival::Published,
    }
}

fn selection(ordinal: u64, original: &StateIndexEntry) -> StateIndexEntry {
    StateIndexEntry {
        ordinal: DecimalU64(ordinal),
        arrival: Arrival::Selected,
        ..original.clone()
    }
}

fn guard() -> PublicationGuard {
    let id = artifact(3, '4');
    PublicationGuard {
        format: "state-publication/1".into(),
        artifact_id: id.clone(),
        capture_id: DecimalU64(1),
        state: PublicationState::Pending,
        phase: PersistencePhase::Preparing,
        locations: vec![format!(".{id}.part"), id.to_string()],
    }
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

fn stream(entries: &[StateIndexEntry]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in entries {
        bytes.extend_from_slice(&entry.to_json().unwrap());
        bytes.push(b'\n');
    }
    bytes
}

/// Exercise every object boundary, including the external enum tag object and
/// its payload, rather than relying only on the top-level record's key checks.
fn strict_objects(bytes: &[u8], accepts: impl Fn(&[u8]) -> bool) {
    let value: Value = serde_json::from_slice(bytes).unwrap();
    let canonical = value.to_string();
    let mut paths = vec![String::new()];
    let mut cursor = 0;
    while cursor < paths.len() {
        let path = paths[cursor].clone();
        let object = value.pointer(&path).unwrap().as_object().unwrap();
        for (key, field) in object {
            let mut missing = value.clone();
            missing
                .pointer_mut(&path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                !accepts(missing.to_string().as_bytes()),
                "accepted missing {path}/{key}"
            );

            let subtree = Value::Object(object.clone()).to_string();
            let duplicate = format!(
                "{{{}:{field},{}",
                serde_json::to_string(key).unwrap(),
                &subtree[1..]
            );
            let malformed = canonical.replacen(&subtree, &duplicate, 1);
            assert!(
                !accepts(malformed.as_bytes()),
                "accepted duplicate {path}/{key}"
            );
            if field.is_object() {
                paths.push(format!("{path}/{key}"));
            }
        }
        let mut unknown = value.clone();
        unknown
            .pointer_mut(&path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), Value::Null);
        assert!(
            !accepts(unknown.to_string().as_bytes()),
            "accepted unknown field in {path}"
        );
        cursor += 1;
    }
}

#[test]
fn artifact_id_preserves_creation_ordinal_and_digest_without_aliases() {
    for ordinal in [1, 10, u64::MAX] {
        let id = artifact(ordinal, 'a');
        let expected = format!("{ordinal:020}-{}", "a".repeat(64));
        assert_eq!(id.as_str(), expected);
        assert_eq!(id.to_string(), expected);
        assert_eq!(ArtifactId::parse(&expected).unwrap(), id);
        assert_eq!(id.creation_ordinal(), DecimalU64(ordinal));
        assert_eq!(id.config_digest(), &digest('a'));
    }
    assert!(ArtifactId::from_parts(DecimalU64(0), digest('a')).is_err());
    let valid = artifact(1, 'a').to_string();
    for malformed in [
        format!("{:020}-{}", 0, "a".repeat(64)),
        format!("18446744073709551616-{}", "a".repeat(64)),
        format!("99999999999999999999-{}", "a".repeat(64)),
        format!("{:019}-{}", 1, "a".repeat(64)),
        format!("{:021}-{}", 1, "a".repeat(64)),
        format!("+0000000000000000001-{}", "a".repeat(64)),
        valid.replace('-', "/"),
        valid.replace('a', "A"),
        valid.replace('a', "g"),
        format!("{valid}\n"),
        format!(" {valid}"),
        format!("{valid} "),
        "../artifact".into(),
        format!("\u{e9}{}", &valid[2..]),
        String::new(),
    ] {
        assert!(ArtifactId::parse(&malformed).is_err(), "{malformed:?}");
    }
}

#[test]
fn selector_allows_only_the_exact_basename_and_one_optional_final_lf() {
    let id = artifact(1, 'b');
    for bytes in [id.to_string().into_bytes(), format!("{id}\n").into_bytes()] {
        assert_eq!(ArtifactId::parse_selector(&bytes).unwrap(), id);
    }
    for bytes in [
        format!("{id}\n\n").into_bytes(),
        format!("{id}\r\n").into_bytes(),
        format!("{id}\r").into_bytes(),
        format!(" {id}").into_bytes(),
        format!("{id} ").into_bytes(),
        format!("\t{id}\n").into_bytes(),
        format!("{id}\0").into_bytes(),
        format!("{id}\n{id}").into_bytes(),
        format!("save-points/{id}").into_bytes(),
        vec![0xff],
    ] {
        assert!(ArtifactId::parse_selector(&bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn canonical_binding_has_declared_field_order_and_explicit_nulls() {
    let binding = seed();
    let expected = format!(
        "{{\"version\":1,\"schema_id\":\"state-content/1\",\"seed_id\":\"{}\",\"dump_sha256\":\"{}\",\"config_sha256\":\"{}\",\"parent_artifact_id\":null,\"position\":null,\"confirmation\":{{\"seed\":{{\"seed_id\":\"{}\"}}}}}}",
        "a".repeat(32),
        "1".repeat(64),
        "2".repeat(64),
        "a".repeat(32)
    );
    assert_eq!(text(binding.to_json().unwrap()), expected);
    assert_eq!(
        StateBinding::from_json(expected.as_bytes()).unwrap(),
        binding
    );
}

#[test]
fn canonical_nested_source_position_and_confirmation_keep_their_contract_order() {
    let binding = serving();
    let expected = format!(
        "{{\"version\":1,\"schema_id\":\"state-content/1\",\"seed_id\":\"{}\",\"dump_sha256\":\"{}\",\"config_sha256\":\"{}\",\"parent_artifact_id\":\"{}\",\"position\":{{\"block_id\":{{\"run\":\"source-run\",\"sequence\":\"18446744073709551615\",\"block\":\"0\"}},\"turn\":null}},\"confirmation\":{{\"serving\":{{\"run_id\":\"capture-run\",\"capture_id\":\"18446744073709551615\"}}}}}}",
        "a".repeat(32),
        "1".repeat(64),
        "3".repeat(64),
        artifact(1, '2')
    );
    assert_eq!(text(binding.to_json().unwrap()), expected);
    assert_eq!(
        StateBinding::from_json(expected.as_bytes()).unwrap(),
        binding
    );
    let mut escaped = binding.clone();
    escaped.position.as_mut().unwrap().turn = Some("turn\n\"\u{e9}\\".into());
    let bytes = escaped.to_json().unwrap();
    assert!(!bytes.contains(&b'\n'));
    assert_eq!(StateBinding::from_json(&bytes).unwrap(), escaped);
}

#[test]
fn binding_rejects_missing_unknown_and_duplicate_keys_at_every_depth() {
    for binding in [seed(), serving(), rebind()] {
        strict_objects(&binding.to_json().unwrap(), |bytes| {
            StateBinding::from_json(bytes).is_ok()
        });
    }
}

#[test]
fn escaped_duplicate_keys_cannot_alias_artifact_fields() {
    let bytes = text(serving().to_json().unwrap());
    for malformed in [
        bytes.replace("\"version\":1", "\"version\":1,\"\\u0076ersion\":1"),
        bytes.replace(
            "\"run_id\":\"capture-run\"",
            "\"run_id\":\"capture-run\",\"\\u0072un_id\":\"capture-run\"",
        ),
        bytes.replace(
            "\"run\":\"source-run\"",
            "\"run\":\"source-run\",\"\\u0072un\":\"source-run\"",
        ),
    ] {
        assert!(StateBinding::from_json(malformed.as_bytes()).is_err());
    }
}

#[test]
fn binding_refuses_noncanonical_scalars_schema_and_legacy_aliases() {
    let valid: Value = serde_json::from_slice(&serving().to_json().unwrap()).unwrap();
    for (pointer, invalid) in [
        ("/version", json!(0)),
        ("/version", json!(2)),
        ("/version", json!(1.0)),
        ("/version", json!("1")),
        ("/version", Value::Null),
        ("/schema_id", json!("state-content/2")),
        ("/seed_id", json!("a".repeat(31))),
        ("/seed_id", json!("A".repeat(32))),
        ("/seed_id", json!("g".repeat(32))),
        ("/dump_sha256", json!("1".repeat(63))),
        ("/config_sha256", json!("A".repeat(64))),
        ("/parent_artifact_id", json!("../state.sqlite")),
        ("/position", json!({})),
        ("/position/block_id/run", json!("")),
        ("/position/block_id/sequence", json!(1)),
        ("/position/block_id/sequence", json!("01")),
        ("/position/block_id/sequence", json!("18446744073709551616")),
        ("/position/block_id/block", json!("-1")),
        ("/position/turn", json!(1)),
        ("/confirmation", json!("serving")),
        ("/confirmation", json!({"recovered": {}})),
        ("/confirmation/serving/run_id", json!("")),
        ("/confirmation/serving/capture_id", json!("0")),
        ("/confirmation/serving/capture_id", json!(1)),
        ("/confirmation/serving/capture_id", json!("01")),
    ] {
        let mut malformed = valid.clone();
        *malformed.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            StateBinding::from_json(malformed.to_string().as_bytes()).is_err(),
            "{pointer}: {malformed}"
        );
    }
    for bytes in [b"null".as_slice(), b"[]", b"{} {}", b"\xff", b"{"] {
        assert!(StateBinding::from_json(bytes).is_err());
    }
}

#[test]
fn seed_and_rebind_confirmation_must_agree_with_parent_and_seed() {
    let mut bad_seed = seed();
    bad_seed.confirmation = Confirmation::Seed {
        seed_id: "b".repeat(32),
    };
    assert!(bad_seed.to_json().is_err());
    bad_seed = seed();
    bad_seed.parent_artifact_id = Some(artifact(1, '2'));
    assert!(bad_seed.to_json().is_err());
    bad_seed = seed();
    bad_seed.position = serving().position;
    assert!(bad_seed.to_json().is_err());

    for parent in [None, Some(artifact(1, '9'))] {
        let mut invalid = rebind();
        invalid.parent_artifact_id = parent;
        assert!(invalid.to_json().is_err());
    }
    let mut invalid = rebind();
    invalid.confirmation = Confirmation::Rebind {
        parent_artifact_id: "unindexed-path".into(),
    };
    assert!(invalid.to_json().is_err());
    assert_eq!(
        StateBinding::from_json(&rebind().to_json().unwrap()).unwrap(),
        rebind()
    );

    // No source position has been filed in this run; null is an exact frontier.
    let mut no_source = serving();
    no_source.position = None;
    assert_eq!(
        StateBinding::from_json(&no_source.to_json().unwrap()).unwrap(),
        no_source
    );
}

#[test]
fn index_canonical_shape_uses_current_names_and_shared_cause() {
    let entry = publication(1, &seed(), PersistenceCause::Seed);
    let expected = format!(
        "{{\"ordinal\":\"1\",\"artifact_id\":\"{}\",\"dump_sha256\":\"{}\",\"config_sha256\":\"{}\",\"seed_id\":\"{}\",\"parent_artifact_id\":null,\"cause\":\"seed\",\"position\":null,\"confirmation\":{{\"seed\":{{\"seed_id\":\"{}\"}}}},\"arrival\":\"published\"}}",
        artifact(1, '2'),
        "1".repeat(64),
        "2".repeat(64),
        "a".repeat(32),
        "a".repeat(32)
    );
    assert_eq!(text(entry.to_json().unwrap()), expected);
    assert_eq!(
        StateIndexEntry::from_json(expected.as_bytes()).unwrap(),
        entry
    );
    assert!(entry.matches_binding(&seed()));
    assert!(!entry.matches_binding(&serving()));
    for malformed in [
        expected.replace("\"ordinal\"", "\"stamp\""),
        expected.replace("\"arrival\"", "\"arrived\""),
        expected.replace("\"published\"", "\"recovered\""),
        expected.replace("\"ordinal\":\"1\"", "\"ordinal\":1"),
        expected.replace("\"ordinal\":\"1\"", "\"ordinal\":\"01\""),
        expected.replace("\"ordinal\":\"1\"", "\"ordinal\":\"0\""),
        expected.replace("\"ordinal\":\"1\"", "\"ordinal\":\"18446744073709551616\""),
    ] {
        assert!(
            StateIndexEntry::from_json(malformed.as_bytes()).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn index_rejects_missing_unknown_and_duplicate_fields_at_every_depth() {
    for (binding, cause) in [
        (seed(), PersistenceCause::Seed),
        (serving(), PersistenceCause::Requested),
        (rebind(), PersistenceCause::Rebind),
    ] {
        strict_objects(
            &publication(2, &binding, cause).to_json().unwrap(),
            |bytes| StateIndexEntry::from_json(bytes).is_ok(),
        );
    }
}

#[test]
fn cause_confirmation_matrix_and_artifact_identity_are_enforced_on_both_directions() {
    let causes = [
        PersistenceCause::Seed,
        PersistenceCause::Rebind,
        PersistenceCause::Requested,
        PersistenceCause::GracefulUnload,
        PersistenceCause::ForcedUnload,
    ];
    for (binding, valid_causes) in [
        (seed(), vec![PersistenceCause::Seed]),
        (rebind(), vec![PersistenceCause::Rebind]),
        (serving(), causes[2..].to_vec()),
    ] {
        let good = publication(2, &binding, valid_causes[0]);
        let valid: Value = serde_json::from_slice(&good.to_json().unwrap()).unwrap();
        for cause in causes {
            let mut entry = good.clone();
            entry.cause = cause;
            let expected = valid_causes.contains(&cause);
            assert_eq!(entry.to_json().is_ok(), expected);
            let mut raw = valid.clone();
            raw["cause"] = serde_json::to_value(cause).unwrap();
            assert_eq!(
                StateIndexEntry::from_json(raw.to_string().as_bytes()).is_ok(),
                expected
            );
        }
    }
    let good = publication(2, &serving(), PersistenceCause::Requested);
    let mut invalid = good.clone();
    invalid.config_sha256 = digest('9');
    assert!(invalid.to_json().is_err());
    invalid = good.clone();
    invalid.parent_artifact_id = Some(invalid.artifact_id.clone());
    assert!(invalid.to_json().is_err());
    invalid = good.clone();
    invalid.ordinal = DecimalU64(3);
    assert!(invalid.to_json().is_err());
    invalid = good.clone();
    invalid.arrival = Arrival::Selected;
    assert!(invalid.to_json().is_err());
    let maximum = publication(u64::MAX, &serving(), PersistenceCause::Requested);
    assert_eq!(
        StateIndexEntry::from_json(&maximum.to_json().unwrap()).unwrap(),
        maximum
    );
}

#[test]
fn index_stream_requires_complete_records_and_contiguous_positive_ordinals() {
    let first = publication(1, &seed(), PersistenceCause::Seed);
    let second = publication(2, &serving(), PersistenceCause::Requested);
    let chosen = selection(3, &first);
    let entries = vec![first.clone(), second.clone(), chosen];
    let bytes = stream(&entries);
    assert_eq!(decode_index(&bytes).unwrap(), entries);
    assert!(decode_index(b"").unwrap().is_empty());
    for malformed in [
        bytes[..bytes.len() - 1].to_vec(),
        [bytes.as_slice(), b"{"].concat(),
        [bytes.as_slice(), b"{}\n"].concat(),
        [bytes.as_slice(), b"\n"].concat(),
        [b"\n", bytes.as_slice()].concat(),
        [stream(std::slice::from_ref(&first)), b" \n".to_vec()].concat(),
        stream(&[second]),
        stream(&[first.clone(), selection(3, &first)]),
        stream(&[first.clone(), first.clone()]),
    ] {
        assert!(
            decode_index(&malformed).is_err(),
            "{:?}",
            String::from_utf8_lossy(&malformed)
        );
    }
}

#[test]
fn selections_preserve_every_original_identity_field_and_may_repeat() {
    let first = publication(1, &seed(), PersistenceCause::Seed);
    let second = publication(2, &serving(), PersistenceCause::Requested);
    let chosen = selection(3, &second);
    let again = selection(4, &second);
    assert!(validate_index(&[first.clone(), second.clone(), chosen.clone(), again]).is_ok());

    for field in [
        "dump",
        "config",
        "seed",
        "parent",
        "cause",
        "position",
        "confirmation",
    ] {
        let mut altered = chosen.clone();
        match field {
            "dump" => altered.dump_sha256 = digest('9'),
            "config" => altered.config_sha256 = digest('9'),
            "seed" => altered.seed_id = "b".repeat(32),
            "parent" => altered.parent_artifact_id = Some(artifact(1, '9')),
            "cause" => altered.cause = PersistenceCause::ForcedUnload,
            "position" => altered.position = None,
            "confirmation" => {
                altered.confirmation = Confirmation::Serving {
                    run_id: "different-run".into(),
                    capture_id: DecimalU64(1),
                }
            }
            _ => unreachable!(),
        }
        assert!(
            validate_index(&[first.clone(), second.clone(), altered]).is_err(),
            "{field}"
        );
    }

    let unpublished = publication(1, &serving(), PersistenceCause::Requested);
    assert!(validate_index(&[first, selection(2, &unpublished)]).is_err());
}

#[test]
fn guard_canonical_shape_and_all_closed_state_and_phase_values_round_trip() {
    let guard = guard();
    let expected = format!(
        "{{\"format\":\"state-publication/1\",\"artifact_id\":\"{}\",\"capture_id\":\"1\",\"state\":\"pending\",\"phase\":\"preparing\",\"locations\":[\".{}.part\",\"{}\"]}}",
        guard.artifact_id, guard.artifact_id, guard.artifact_id
    );
    assert_eq!(text(guard.to_json().unwrap()), expected);
    let phases = [
        PersistencePhase::Queued,
        PersistencePhase::Beginning,
        PersistencePhase::Barrier,
        PersistencePhase::Capturing,
        PersistencePhase::Hashing,
        PersistencePhase::Finishing,
        PersistencePhase::Preparing,
        PersistencePhase::Confirming,
        PersistencePhase::Publishing,
        PersistencePhase::Reporting,
    ];
    for state in [PublicationState::Pending, PublicationState::Broken] {
        for phase in phases {
            let record = PublicationGuard {
                state,
                phase,
                capture_id: DecimalU64(u64::MAX),
                ..guard.clone()
            };
            assert_eq!(
                PublicationGuard::from_json(&record.to_json().unwrap()).unwrap(),
                record
            );
        }
    }
}

#[test]
fn guard_rejects_key_errors_wrong_literals_counters_and_path_locations() {
    let valid = guard();
    strict_objects(&valid.to_json().unwrap(), |bytes| {
        PublicationGuard::from_json(bytes).is_ok()
    });
    let value: Value = serde_json::from_slice(&valid.to_json().unwrap()).unwrap();
    for (key, invalid) in [
        ("format", json!("state-publication/2")),
        ("artifact_id", json!("../artifact")),
        ("capture_id", json!("0")),
        ("capture_id", json!(1)),
        ("capture_id", json!("01")),
        ("capture_id", json!("18446744073709551616")),
        ("state", json!("published")),
        ("phase", json!("published")),
        ("locations", Value::Null),
        ("locations", json!([1])),
    ] {
        let mut malformed = value.clone();
        malformed[key] = invalid;
        assert!(
            PublicationGuard::from_json(malformed.to_string().as_bytes()).is_err(),
            "{key}: {malformed}"
        );
    }
    for location in [
        "",
        ".",
        "..",
        "/absolute",
        "../escape",
        "nested/path",
        "nul\0name",
    ] {
        let mut record = valid.clone();
        record.locations = vec![location.into()];
        assert!(record.to_json().is_err(), "{location:?}");
        let mut malformed = value.clone();
        malformed["locations"] = json!([location]);
        assert!(
            PublicationGuard::from_json(malformed.to_string().as_bytes()).is_err(),
            "{location:?}"
        );
    }
}

#[test]
fn pure_codecs_are_deterministic_and_do_not_require_artifacts_to_exist() {
    // These fixture identities describe no on-disk repository. The codec can
    // validate evidence shape without granting publication or consulting paths.
    for binding in [seed(), serving(), rebind()] {
        let bytes = binding.to_json().unwrap();
        let reordered = serde_json::from_slice::<Value>(&bytes).unwrap().to_string();
        for input in [bytes.as_slice(), reordered.as_bytes()] {
            let parsed = StateBinding::from_json(input).unwrap();
            assert_eq!(parsed.to_json().unwrap(), bytes);
            assert_eq!(parsed.to_json().unwrap(), bytes);
        }
    }
    let mut empty_locations = guard();
    empty_locations.locations.clear();
    assert_eq!(
        PublicationGuard::from_json(&empty_locations.to_json().unwrap()).unwrap(),
        empty_locations
    );
}
