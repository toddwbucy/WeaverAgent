use super::*;

fn fixture() -> (String, SchemeDeclaration, String, ThesaurusDeclaration) {
    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../weaver-harness/src/flow/fixtures/seed.json"
    ));
    let seed: weaver_types::state_flow::SeedRequest = decode_strict(raw).unwrap();
    let scheme = format!("\r\n{}\n", seed.ste100_scheme);
    let digest = hash(scheme.as_bytes());
    let declared = SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into()],
    };
    let thesaurus = format!(
        "\r\n{}\n",
        serde_json::json!({
            "format": "ste100-thesaurus/1", "scheme_id": declared.scheme_id,
            "source": {"repository": "synthetic", "commit": "test", "path": "test",
                "sha256": hash(b"source")}, "entries": []
        })
    );
    let election = ThesaurusDeclaration {
        artifact_sha256: hash(thesaurus.as_bytes()),
        scheme_id: declared.scheme_id.clone(),
    };
    (scheme, declared, thesaurus, election)
}

#[test]
fn checked_governing_files_preserve_source_bytes_and_both_presence_cases() {
    let (scheme, declared, thesaurus, election) = fixture();
    for elected in [false, true] {
        let files = GoverningFiles::verify(
            scheme.clone(),
            elected.then(|| thesaurus.clone()),
            &declared,
            elected.then_some(&election),
        )
        .unwrap();
        assert_eq!(files.scheme().as_bytes(), scheme.as_bytes());
        assert_eq!(files.thesaurus(), elected.then_some(thesaurus.as_str()));
        assert_eq!(files.scheme_sha256(), &hash(scheme.as_bytes()));
        assert_eq!(
            files.thesaurus_sha256(),
            elected.then_some(&election.artifact_sha256)
        );
        files
            .verify_declaration(&declared, elected.then_some(&election))
            .unwrap();
    }
}

#[test]
fn declared_identity_cannot_be_replaced_by_the_candidate_files_own_hashes() {
    let (scheme, declared, thesaurus, election) = fixture();
    for (scheme, thesaurus, elected) in [
        (format!("{scheme} "), None, None),
        (
            format!("{scheme} "),
            Some(thesaurus.clone()),
            Some(&election),
        ),
        (
            scheme.clone(),
            Some(format!("{thesaurus} ")),
            Some(&election),
        ),
        (scheme.clone(), None, Some(&election)),
        (scheme.clone(), Some(thesaurus.clone()), None),
    ] {
        let fault = GoverningFiles::verify(scheme, thesaurus, &declared, elected).unwrap_err();
        assert_eq!(fault.case, PersistenceCase::SchemeMismatch);
    }
}

#[test]
fn carrying_files_forward_refuses_a_changed_election_or_identity() {
    let (scheme, declared, thesaurus, election) = fixture();
    let files =
        GoverningFiles::verify(scheme, Some(thesaurus), &declared, Some(&election)).unwrap();
    assert!(files.verify_declaration(&declared, None).is_err());
    let mut changed = election.clone();
    changed.artifact_sha256 = hash(b"changed");
    assert!(files.verify_declaration(&declared, Some(&changed)).is_err());
    changed = election.clone();
    changed.scheme_id = format!("ste100:{}", hash(b"foreign"));
    assert!(files.verify_declaration(&declared, Some(&changed)).is_err());
    let mut changed = declared.clone();
    changed.artifact_sha256 = hash(b"changed");
    changed.scheme_id = format!("ste100:{}", changed.artifact_sha256);
    assert!(files.verify_declaration(&changed, Some(&election)).is_err());
    let mut changed = declared.clone();
    changed.verbs.push("invented".into());
    assert!(files.verify_declaration(&changed, Some(&election)).is_err());
}

// Synthetic artifact bytes exercise publication, not scheme qualification.
pub(in crate::save_points::serving) fn publication_fixture(
    elected: bool,
) -> (GoverningFiles, weaver_types::ServingDeclaration) {
    let (scheme, declared, thesaurus, election) = fixture();
    let files = GoverningFiles::verify(
        scheme,
        elected.then_some(thesaurus),
        &declared,
        elected.then_some(&election),
    )
    .unwrap();
    let old = crate::save_points::serving::declaration::tests::fixture();
    let mut source = crate::save_points::serving::declaration::render(&old)
        .unwrap()
        .replace(&"b".repeat(64), declared.artifact_sha256.as_str())
        .replace("[\"hold\", \"recall\"]", "[\"read\"]");
    if elected {
        source.push_str(&format!(
            "state-management.flow.thesaurus.artifact-sha256 = \"{}\"\n\
             state-management.flow.thesaurus.scheme-id = \"{}\"\n",
            election.artifact_sha256, election.scheme_id,
        ));
    }
    (
        files,
        weaver_types::parse_serving_declaration(&source).unwrap(),
    )
}

#[test]
fn matching_outer_hashes_do_not_license_invalid_artifact_shape_or_grammar() {
    let (scheme, mut declared, thesaurus, mut election) = fixture();
    let mut artifact: Ste100Scheme = decode_strict(&scheme).unwrap();
    artifact.grammar.sha256 = hash(b"different grammar bytes");
    let altered = serde_json::to_string(&artifact).unwrap();
    declared.artifact_sha256 = hash(altered.as_bytes());
    declared.scheme_id = format!("ste100:{}", declared.artifact_sha256);
    let fault = GoverningFiles::verify(altered, None, &declared, None).unwrap_err();
    assert_eq!(fault.case, PersistenceCase::SchemeMismatch);
    assert!(fault.detail.contains("ste100-scheme.json"));

    let (_, declared, _, _) = fixture();
    for defect in ["format", "source_path", "splice_uses"] {
        let mut artifact: serde_json::Value = serde_json::from_str(&thesaurus).unwrap();
        match defect {
            "format" => artifact["format"] = "ste100-thesaurus/2".into(),
            "source_path" => artifact["source"]["path"] = "".into(),
            "splice_uses" => {
                artifact["entries"] = serde_json::json!([
                    {"word":"commence", "tag":"splice", "senses":[
                        {"sense":"begin", "use":["start", "begin"]}
                    ]}
                ])
            }
            _ => unreachable!(),
        }
        let altered = artifact.to_string();
        election.artifact_sha256 = hash(altered.as_bytes());
        let fault =
            GoverningFiles::verify(scheme.clone(), Some(altered), &declared, Some(&election))
                .unwrap_err();
        assert_eq!(fault.case, PersistenceCase::SchemeMismatch, "{defect}");
        assert!(fault.detail.contains("ste100-thesaurus.json"), "{fault}");
    }
}
