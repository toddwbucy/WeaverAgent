use super::*;

#[test]
fn loaded_copy_checks_every_observed_identity_field_and_the_judged_territory() {
    let fixture = Fixture::new();
    let entry = fixture.seed();
    let repository = fixture.repository();
    let verified = repository
        .verify_indexed(&entry.artifact_id, &fixture.scope())
        .unwrap();
    let start = verified.identity();
    let territory = crate::inventory::sink_directory(&verified.declaration().agent().trace_sink);
    for case in [
        "valid",
        "seed",
        "artifact",
        "dump",
        "config",
        "parent",
        "position",
        "declaration",
        "session",
        "territory",
    ] {
        let mut expected = start.clone();
        let mut session = fixture.session.clone();
        let mut declaration = start.config_sha256.as_str().to_owned();
        let mut judged = territory.to_path_buf();
        match case {
            "seed" => expected.seed_id = "2".repeat(32),
            "artifact" => expected.artifact_id = format!("{:020}-{}", 2, start.config_sha256),
            "dump" => expected.dump_sha256 = Digest::new("d".repeat(64)).unwrap(),
            "config" => expected.config_sha256 = Digest::new("e".repeat(64)).unwrap(),
            "parent" => expected.parent_artifact_id = Some(start.artifact_id.clone()),
            "position" => {
                expected.position = Some(weaver_types::state_flow::SourcePosition {
                    block_id: weaver_types::state_flow::SourceBlockId {
                        run: "other".into(),
                        sequence: 1.into(),
                        block: 0.into(),
                    },
                    turn: None,
                })
            }
            "declaration" => declaration = "f".repeat(64),
            "session" => session = SessionId("another-session".into()),
            "territory" => judged = PathBuf::from("/not-the-judged-territory"),
            "valid" => (),
            _ => unreachable!(),
        }
        let result = repository.verify_loaded(&expected, &session, &declaration, &judged);
        assert_eq!(result.is_ok(), case == "valid", "{case}: {result:?}");
    }
}

#[test]
fn loaded_copy_retains_index_guard_custody_and_hash_checks() {
    for case in [
        "guard",
        "broken",
        "index",
        "image",
        "configuration",
        "custody",
    ] {
        let fixture = Fixture::new();
        let entry = fixture.seed();
        let repository = fixture.repository();
        let verified = repository
            .verify_indexed(&entry.artifact_id, &fixture.scope())
            .unwrap();
        let start = verified.identity();
        let territory =
            crate::inventory::sink_directory(&verified.declaration().agent().trace_sink);
        match case {
            "guard" => fixture.guard(&entry, PublicationState::Pending),
            "broken" => fixture.guard(&entry, PublicationState::Broken),
            "index" => fs::remove_file(fixture.path.join(MANIFEST)).unwrap(),
            "image" => fixture.write(
                Path::new(entry.artifact_id.as_str()).join(DUMP),
                b"corrupt",
                0o640,
            ),
            "configuration" => fixture.write(
                Path::new(entry.artifact_id.as_str()).join(CONFIGURATION),
                b"corrupt",
                0o640,
            ),
            "custody" => fs::set_permissions(
                fixture.path.join(entry.artifact_id.as_str()).join(DUMP),
                fs::Permissions::from_mode(0o666),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        assert!(
            repository
                .verify_loaded(
                    &start,
                    &fixture.session,
                    start.config_sha256.as_str(),
                    territory
                )
                .is_err(),
            "{case}"
        );
    }
}
