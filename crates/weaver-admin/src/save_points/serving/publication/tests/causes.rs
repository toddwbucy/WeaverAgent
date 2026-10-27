use super::*;

#[test]
fn final_left_settles_only_the_pending_tail_cause_and_keeps_artifact_identity() {
    for forced in [false, true] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let prefix = fixture.index_bytes();
        let mut publication =
            fixture.capture(&mut repo, &loaded, 3, PersistenceCause::GracefulUnload);
        let identity = publish(&mut publication, 3);
        let provisional = fixture.index()[1].clone();
        let binding_path = fixture
            .root
            .join(&identity.artifact_id)
            .join("binding.json");
        let binding = fs::read(&binding_path).unwrap();
        fixture.assert_guarded(&identity, PublicationState::Pending);
        let report = report(&identity, 3);
        assert_eq!(
            publication
                .finish(
                    FinalOutcome::Left {
                        report: &report,
                        forced
                    },
                    &mut alive
                )
                .unwrap(),
            identity
        );
        let mut expected = provisional;
        expected.cause = if forced {
            PersistenceCause::ForcedUnload
        } else {
            PersistenceCause::GracefulUnload
        };
        assert_eq!(fixture.index()[1], expected);
        assert!(fixture.index_bytes().starts_with(&prefix));
        assert_eq!(fs::read(&binding_path).unwrap(), binding);
        let completed = fixture.index_bytes();
        assert!(
            publication
                .finish(
                    FinalOutcome::Left {
                        report: &report,
                        forced: !forced
                    },
                    &mut alive
                )
                .is_err()
        );
        assert_eq!(
            fixture.index_bytes(),
            completed,
            "a later reply cannot edit a completed save"
        );
        drop(publication);
        assert_eq!(
            repo.load(None, &fixture.scope()).unwrap().identity(),
            identity
        );
        repo.verify_completed_leave(&report, forced, fixture.trace_path.parent().unwrap())
            .unwrap();
        assert!(
            repo.verify_completed_leave(&report, !forced, fixture.trace_path.parent().unwrap())
                .is_err()
        );
    }
}

#[test]
fn joined_converted_leave_checks_pending_and_broken_guards_before_cause() {
    for broken in [false, true] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication =
            fixture.capture(&mut repo, &loaded, 3, PersistenceCause::GracefulUnload);
        let identity = publish(&mut publication, 3);
        if broken {
            publication.failed("converted Leave ended before finalization");
        }
        drop(publication);
        let before = fixture.index_bytes();
        let error = repo
            .verify_completed_leave(
                &report(&identity, 3),
                true,
                fixture.trace_path.parent().unwrap(),
            )
            .unwrap_err();
        assert_eq!(error.case, PersistenceCase::BrokenSavePoint);
        assert_eq!(fixture.index_bytes(), before);
        fixture.assert_guarded(
            &identity,
            if broken {
                PublicationState::Broken
            } else {
                PublicationState::Pending
            },
        );
    }
}

#[test]
fn final_left_cannot_downgrade_a_forced_publication() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::ForcedUnload);
    let identity = publish(&mut publication, 3);
    let indexed = fixture.index_bytes();
    let error = publication
        .finish(
            FinalOutcome::Left {
                report: &report(&identity, 3),
                forced: false,
            },
            &mut alive,
        )
        .unwrap_err();
    assert!(error.detail.contains("forced Leave answered as graceful"));
    assert_eq!(fixture.index_bytes(), indexed);
    fixture.assert_guarded(&identity, PublicationState::Broken);
}

#[test]
fn final_cause_requires_exact_pending_guard_index_prefix_and_tail() {
    for case in [
        "guard",
        "missing-guard",
        "prefix",
        "tail",
        "extra-line",
        "foreign-temporary",
        "identity",
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication =
            fixture.capture(&mut repo, &loaded, 3, PersistenceCause::GracefulUnload);
        let identity = publish(&mut publication, 3);
        let guard_path = fixture
            .root
            .join(format!(".{}.pending", identity.artifact_id));
        let mut final_report = report(&identity, 3);
        match case {
            "guard" => {
                let mut guard =
                    PublicationGuard::from_json(&fs::read(&guard_path).unwrap()).unwrap();
                guard.capture_id = 99.into();
                fs::write(&guard_path, guard.to_json().unwrap()).unwrap();
            }
            "missing-guard" => fs::remove_file(&guard_path).unwrap(),
            "prefix" | "tail" => {
                let mut index = fixture.index();
                let slot = usize::from(case == "tail");
                index[slot].dump_sha256 = hash(b"foreign index bytes");
                let bytes = index
                    .iter()
                    .flat_map(|entry| {
                        let mut bytes = entry.to_json().unwrap();
                        bytes.push(b'\n');
                        bytes
                    })
                    .collect::<Vec<_>>();
                fs::write(fixture.root.join(MANIFEST), bytes).unwrap();
            }
            "extra-line" => {
                let mut bytes = fixture.index_bytes();
                bytes.extend_from_slice(b"unexpected\n");
                fs::write(fixture.root.join(MANIFEST), bytes).unwrap();
            }
            "foreign-temporary" => {
                fs::write(fixture.root.join(publication.cause_part_name()), b"foreign").unwrap()
            }
            "identity" => final_report.capture_id = 4.into(),
            _ => unreachable!(),
        }
        let before = fixture.index_bytes();
        assert!(
            publication
                .finish(
                    FinalOutcome::Left {
                        report: &final_report,
                        forced: true
                    },
                    &mut alive
                )
                .is_err(),
            "{case}"
        );
        assert_eq!(fixture.index_bytes(), before, "{case}");
        assert!(
            fixture.repository().load(None, &fixture.scope()).is_err(),
            "{case}"
        );
        if case == "foreign-temporary" {
            assert_eq!(
                fs::read(fixture.root.join(publication.cause_part_name())).unwrap(),
                b"foreign"
            );
        }
    }
}

#[test]
fn final_cause_replacement_failure_keeps_the_save_guarded_and_reports_retained_work() {
    for fault in [
        fs_ops::GuardFault::BeforeReplace,
        fs_ops::GuardFault::AfterReplace,
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let prefix = fixture.index_bytes();
        let mut publication =
            fixture.capture(&mut repo, &loaded, 3, PersistenceCause::GracefulUnload);
        let identity = publish(&mut publication, 3);
        let error = fs_ops::with_guard_fault(fault, 0, || {
            publication.finish(
                FinalOutcome::Left {
                    report: &report(&identity, 3),
                    forced: true,
                },
                &mut alive,
            )
        })
        .unwrap_err();
        fixture.assert_guarded(&identity, PublicationState::Broken);
        assert!(fixture.index_bytes().starts_with(&prefix));
        let before_rename = matches!(fault, fs_ops::GuardFault::BeforeReplace);
        assert_eq!(
            fixture.index()[1].cause,
            if before_rename {
                PersistenceCause::GracefulUnload
            } else {
                PersistenceCause::ForcedUnload
            }
        );
        let temporary = format!("save-points/{}", publication.cause_part_name());
        assert_eq!(
            error
                .artifacts
                .iter()
                .any(|a| a.location == temporary && a.kind == RetainedArtifactKind::Part),
            before_rename
        );
        for artifact in &error.artifacts {
            assert!(
                fixture
                    .base
                    .join(&artifact.location)
                    .symlink_metadata()
                    .is_ok()
            );
        }
        let failed = fixture.index_bytes();
        assert!(
            publication
                .finish(
                    FinalOutcome::Left {
                        report: &report(&identity, 3),
                        forced: true
                    },
                    &mut alive
                )
                .is_err()
        );
        assert_eq!(fixture.index_bytes(), failed, "failure grants no retry");
    }
}
