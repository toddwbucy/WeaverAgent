use super::*;

fn second(fixture: &Fixture, first: &StateIndexEntry) -> StateIndexEntry {
    let image = fs::read(fixture.path.join(first.artifact_id.as_str()).join(DUMP)).unwrap();
    fixture.artifact(2, Some(first), &image, None)
}

#[test]
fn index_snapshot_retains_the_exact_judged_bytes_and_whole_chain_checks() {
    let f = Fixture::new();
    assert_eq!(
        f.repository().read_index_snapshot().unwrap(),
        (vec![], vec![])
    );
    let first = f.seed();
    let bytes = format!(
        "  {}  \n",
        String::from_utf8(first.to_json().unwrap()).unwrap()
    );
    f.write(MANIFEST, bytes.as_bytes(), 0o644);
    let repository = f.repository();
    let (entries, original) = repository.read_index_snapshot().unwrap();
    assert_eq!(entries.as_slice(), std::slice::from_ref(&first));
    assert_eq!(original, bytes.as_bytes());
    assert_eq!(repository.read_index().unwrap(), entries);

    let malformed = format!("{bytes}{{\"ordinal\":\"2\"");
    f.write(MANIFEST, malformed.as_bytes(), 0o644);
    assert!(repository.read_index_snapshot().is_err());
    assert_eq!(
        fs::read(f.path.join(MANIFEST)).unwrap(),
        malformed.as_bytes()
    );

    let mut extra_seed = first.clone();
    extra_seed.ordinal = DecimalU64(2);
    extra_seed.artifact_id = ArtifactId::from_parts(DecimalU64(2), first.config_sha256).unwrap();
    f.rewrite_index(&[entries[0].clone(), extra_seed]);
    assert!(repository.read_index_snapshot().is_err());
}

#[test]
fn explicit_restore_appends_a_selection_without_changing_artifact_identity_or_bytes() {
    let f = Fixture::new();
    let first = f.seed();
    let latest = second(&f, &first);
    let repository = f.repository();
    let prefix = fs::read(f.path.join(MANIFEST)).unwrap();
    let artifact_path = f.path.join(first.artifact_id.as_str());
    let before =
        [DUMP, CONFIGURATION, BINDING].map(|name| fs::read(artifact_path.join(name)).unwrap());
    let original = repository
        .verify_indexed(&first.artifact_id, &f.scope())
        .unwrap();
    let mut phases = Vec::new();
    let restored = repository
        .restore(&first.artifact_id, &f.scope(), &mut |phase| {
            phases.push(phase);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        phases,
        [
            PersistencePhase::Preparing,
            PersistencePhase::Preparing,
            PersistencePhase::Publishing,
            PersistencePhase::Reporting,
        ]
    );
    assert_eq!(restored.identity(), original.identity());
    assert_eq!(
        restored.configuration_bytes(),
        original.configuration_bytes()
    );
    assert_eq!(restored.entry().ordinal, DecimalU64(3));
    assert_eq!(restored.entry().arrival, Arrival::Selected);
    let mut expected = first.clone();
    expected.ordinal = DecimalU64(3);
    expected.arrival = Arrival::Selected;
    let index = repository.read_index().unwrap();
    assert_eq!(index, [first, latest, expected]);
    let bytes = fs::read(f.path.join(MANIFEST)).unwrap();
    assert!(bytes.starts_with(&prefix));
    assert_eq!(
        [DUMP, CONFIGURATION, BINDING].map(|name| fs::read(artifact_path.join(name)).unwrap()),
        before
    );
    assert_eq!(
        repository.load(None, &f.scope()).unwrap().identity(),
        restored.identity()
    );
}

#[test]
fn selecting_the_current_latest_again_always_records_a_new_ordinal() {
    let f = Fixture::new();
    let first = f.seed();
    let repository = f.repository();
    for ordinal in [2, 3] {
        let selected = repository
            .restore(&first.artifact_id, &f.scope(), &mut |_| Ok(()))
            .unwrap();
        assert_eq!(selected.entry().ordinal, DecimalU64(ordinal));
        assert_eq!(selected.entry().arrival, Arrival::Selected);
        assert_eq!(selected.entry().binding(), first.binding());
        assert_eq!(selected.entry().cause, first.cause);
    }
    assert_eq!(repository.read_index().unwrap().len(), 3);
    assert_eq!(fs::read_dir(&f.path).unwrap().count(), 2);
}

#[test]
fn every_pre_append_verification_failure_preserves_the_index() {
    for failure in [
        "missing",
        "guard",
        "dump",
        "configuration",
        "scope",
        "index",
    ] {
        let f = Fixture::new();
        let first = f.seed();
        let repository = f.repository();
        let selected = if failure == "missing" {
            ArtifactId::from_parts(DecimalU64(99), hash(b"missing")).unwrap()
        } else {
            first.artifact_id.clone()
        };
        match failure {
            "guard" => f.guard(&first, PublicationState::Pending),
            "dump" => f.write(
                Path::new(first.artifact_id.as_str()).join(DUMP),
                b"changed",
                0o640,
            ),
            "configuration" => f.write(
                Path::new(first.artifact_id.as_str()).join(CONFIGURATION),
                b"changed",
                0o640,
            ),
            "index" => {
                let mut bytes = fs::read(f.path.join(MANIFEST)).unwrap();
                bytes.extend_from_slice(b"{bad tail");
                f.write(MANIFEST, &bytes, 0o644);
            }
            _ => {}
        }
        let wrong_session = SessionId("wrong".into());
        let mut scope = f.scope();
        if failure == "scope" {
            scope.session = &wrong_session;
        }
        let before = fs::read(f.path.join(MANIFEST)).unwrap();
        let error = repository
            .restore(&selected, &scope, &mut |_| Ok(()))
            .unwrap_err();
        assert!(!error.selection_may_be_appended, "{failure}: {error:?}");
        assert_eq!(
            fs::read(f.path.join(MANIFEST)).unwrap(),
            before,
            "{failure}"
        );
    }
}

#[test]
fn checkpoint_failure_before_append_is_clean_but_after_append_retains_the_selection() {
    for stop in 0..4 {
        let f = Fixture::new();
        let first = f.seed();
        let repository = f.repository();
        let before = fs::read(f.path.join(MANIFEST)).unwrap();
        let mut calls = 0;
        let error = repository
            .restore(&first.artifact_id, &f.scope(), &mut |_| {
                let index = calls;
                calls += 1;
                if index == stop {
                    Err("stop fixture".into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(calls, stop + 1);
        assert_eq!(error.selection_may_be_appended, stop == 3);
        assert!(error.fault.detail.contains("stop fixture"));
        let after = fs::read(f.path.join(MANIFEST)).unwrap();
        if stop == 3 {
            assert!(after.starts_with(&before));
            assert_ne!(after, before);
            let index = repository.read_index().unwrap();
            assert_eq!(index.len(), 2);
            assert_eq!(index[1].arrival, Arrival::Selected);
        } else {
            assert_eq!(after, before);
        }
    }
}

#[test]
fn changed_index_prefix_refuses_the_append_without_truncating_the_newer_record() {
    let f = Fixture::new();
    let first = f.seed();
    let repository = f.repository();
    let mut other = first.clone();
    other.ordinal = DecimalU64(2);
    other.arrival = Arrival::Selected;
    let mut changed = None;
    let error = repository
        .restore(&first.artifact_id, &f.scope(), &mut |phase| {
            if phase == PersistencePhase::Publishing {
                f.rewrite_index(&[first.clone(), other.clone()]);
                changed = Some(fs::read(f.path.join(MANIFEST)).unwrap());
            }
            Ok(())
        })
        .unwrap_err();
    assert!(error.selection_may_be_appended);
    assert_eq!(fs::read(f.path.join(MANIFEST)).unwrap(), changed.unwrap());
    assert_eq!(repository.read_index().unwrap(), [first, other]);
}

#[test]
fn partial_append_and_failed_sync_are_uncertain_and_never_rolled_back() {
    use crate::save_points::serving::fs_ops::{AppendFault, with_append_fault};

    for fault in [AppendFault::PartialWrite(13), AppendFault::FileSync] {
        let f = Fixture::new();
        let first = f.seed();
        let repository = f.repository();
        let before = fs::read(f.path.join(MANIFEST)).unwrap();
        let error = with_append_fault(fault, || {
            repository.restore(&first.artifact_id, &f.scope(), &mut |_| Ok(()))
        })
        .unwrap_err();
        assert!(error.selection_may_be_appended);
        let after = fs::read(f.path.join(MANIFEST)).unwrap();
        assert!(after.starts_with(&before));
        assert!(after.len() > before.len());
        match fault {
            AppendFault::PartialWrite(_) => {
                assert_eq!(after.len(), before.len() + 13);
                assert!(repository.read_index().is_err());
            }
            AppendFault::FileSync => {
                let index = repository.read_index().unwrap();
                assert_eq!(index.len(), 2);
                assert_eq!(index[1].arrival, Arrival::Selected);
            }
        }
    }
}

#[test]
fn publisher_readback_ignores_only_index_and_guard_authority() {
    let f = Fixture::new();
    let first = f.seed();
    let repository = f.repository();
    f.guard(&first, PublicationState::Pending);
    assert!(repository.verify(first.clone(), &f.scope()).is_err());
    let checked = repository
        .verify_unindexed_publication(first.clone(), &f.scope())
        .unwrap();
    assert_eq!(checked.entry(), &first);
    f.rewrite_index(&[]);
    assert!(
        repository
            .verify_unindexed_publication(first.clone(), &f.scope())
            .is_ok()
    );
    assert!(repository.load(None, &f.scope()).is_err());
    assert!(
        repository
            .restore(&first.artifact_id, &f.scope(), &mut |_| Ok(()))
            .is_err()
    );
    f.write(
        Path::new(first.artifact_id.as_str()).join(DUMP),
        b"wrong image",
        0o640,
    );
    assert!(
        repository
            .verify_unindexed_publication(first, &f.scope())
            .is_err()
    );
}

#[test]
fn selection_ordinal_overflow_refuses_instead_of_reusing_an_ordinal() {
    assert_eq!(
        next_selection_ordinal(DecimalU64(1)).unwrap(),
        DecimalU64(2)
    );
    assert!(next_selection_ordinal(DecimalU64(u64::MAX)).is_err());
}
