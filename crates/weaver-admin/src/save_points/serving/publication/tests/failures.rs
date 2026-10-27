use super::*;

#[test]
fn loaded_identity_and_capture_identity_are_exact_before_work() {
    for case in 0..10 {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let mut loaded = fixture.seed_complete(&mut repo);
        let original = fixture.index_bytes();
        let mut capture_id = DecimalU64(3);
        let mut event_run = run();
        let mut cause = PersistenceCause::Requested;
        let mut name = "capture-3.sqlite.part".to_owned();
        match case {
            0 => loaded.seed_id = "2".repeat(32),
            1 => loaded.dump_sha256 = hash(b"wrong image"),
            2 => loaded.config_sha256 = hash(b"wrong configuration"),
            3 => loaded.parent_artifact_id = Some(loaded.artifact_id.clone()),
            4 => loaded.position = Some(position(8)),
            5 => loaded.artifact_id = "invalid".into(),
            6 => capture_id = DecimalU64(0),
            7 => event_run = RunId(String::new()),
            8 => cause = PersistenceCause::Rebind,
            9 => name = "capture-3.sqlite".into(),
            _ => unreachable!(),
        }
        fixture.write_room(&name, CAPTURE_BYTES);
        let room = open_directory(&fixture.room).unwrap();
        let error = Publication::serving(
            &mut repo,
            &fixture.scope(),
            ServingImage {
                loaded: &loaded,
                run: event_run,
                cause,
                capture_id,
                room_name: &name,
                size_bytes: DecimalU64(CAPTURE_BYTES.len() as u64),
                position: None,
                room: room.as_fd(),
                member_uid: fixture.owner.uid,
            },
            &mut alive,
        )
        .err()
        .unwrap();
        assert!(error.published.is_none(), "case {case}");
        assert!(error.artifacts.is_empty(), "case {case}");
        assert_eq!(fixture.index_bytes(), original, "case {case}");
        assert_eq!(fixture.names().len(), 2, "case {case}");
    }
}

#[test]
fn wrong_order_or_correlation_permanently_withdraws_publication_authority() {
    for case in 0..11 {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        let mut prepared = None;
        let error = match case {
            0 => publication
                .commit(&run(), DecimalU64(3), &mut alive)
                .unwrap_err(),
            1 => publication
                .image_finished(
                    &RunId("other-run".into()),
                    DecimalU64(3),
                    "capture-3.sqlite",
                    &mut alive,
                )
                .unwrap_err(),
            2 => publication
                .image_finished(&run(), DecimalU64(4), "capture-4.sqlite", &mut alive)
                .unwrap_err(),
            3 => publication
                .image_finished(&run(), DecimalU64(3), "capture-3.sqlite.part", &mut alive)
                .unwrap_err(),
            4 => publication
                .image_finished(&run(), DecimalU64(3), "../capture-3.sqlite", &mut alive)
                .unwrap_err(),
            5 => publication.advance(&mut alive).unwrap_err(),
            _ => {
                let identity = prepare(&mut publication, 3);
                prepared = Some(identity.clone());
                match case {
                    6 => publication.advance(&mut alive).unwrap_err(),
                    7 => publication
                        .commit(&RunId("other-run".into()), DecimalU64(3), &mut alive)
                        .unwrap_err(),
                    8 => publication
                        .commit(&run(), DecimalU64(4), &mut alive)
                        .unwrap_err(),
                    9 => publication
                        .image_finished(&run(), DecimalU64(3), "capture-3.sqlite", &mut alive)
                        .unwrap_err(),
                    10 => publication
                        .finish(
                            FinalOutcome::SavePointTaken(&report(&identity, 3)),
                            &mut alive,
                        )
                        .unwrap_err(),
                    _ => unreachable!(),
                }
            }
        };
        assert!(error.published.is_none(), "case {case}");
        let names = fixture.names();
        let index = fixture.index_bytes();
        let mut unexpected_work =
            |_| -> Result<(), String> { panic!("terminal capture performed work") };
        assert!(publication.advance(&mut unexpected_work).is_err());
        assert!(
            publication
                .commit(&run(), DecimalU64(3), &mut unexpected_work)
                .is_err()
        );
        assert_eq!(fixture.names(), names);
        assert_eq!(fixture.index_bytes(), index);
        assert_eq!(fixture.index().len(), 1);
        if let Some(identity) = prepared {
            fixture.assert_guarded(&identity, PublicationState::Broken);
            assert!(!fixture.root.join(identity.artifact_id).exists());
        } else {
            assert_eq!(
                fixture.names().len(),
                2,
                "unlisted work must be removed, case {case}"
            );
        }
    }
}

#[test]
fn wrong_final_report_or_outcome_keeps_indexed_capture_guarded() {
    for case in 0..10 {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let cause = if case == 9 {
            PersistenceCause::GracefulUnload
        } else {
            PersistenceCause::Requested
        };
        let mut publication = fixture.capture(&mut repo, &loaded, 3, cause);
        let identity = publish(&mut publication, 3);
        let mut final_report = report(&identity, 3);
        match case {
            0 => final_report.event_run = RunId("different".into()),
            1 => final_report.capture_id = DecimalU64(4),
            2 => final_report.start.seed_id = "2".repeat(32),
            3 => final_report.start.artifact_id = loaded.artifact_id.clone(),
            4 => final_report.start.dump_sha256 = hash(b"wrong dump"),
            5 => final_report.start.config_sha256 = hash(b"wrong config"),
            6 => final_report.start.parent_artifact_id = None,
            7 => final_report.start.position = None,
            _ => (),
        }
        let outcome = if case == 8 {
            FinalOutcome::Left {
                report: &final_report,
                forced: true,
            }
        } else {
            FinalOutcome::SavePointTaken(&final_report)
        };
        let error = publication.finish(outcome, &mut alive).unwrap_err();
        assert_eq!(error.published.as_deref(), Some(&identity), "case {case}");
        assert_eq!(fixture.index().len(), 2);
        fixture.assert_guarded(&identity, PublicationState::Broken);
        assert!(
            error
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == RetainedArtifactKind::Guard)
        );
    }
}

#[test]
fn changed_or_foreign_guard_cannot_be_removed_by_matching_final_report() {
    for malformed in [true, false] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        let identity = publish(&mut publication, 3);
        let path = fixture
            .root
            .join(format!(".{}.pending", identity.artifact_id));
        let replacement = if malformed {
            b"not a guard".to_vec()
        } else {
            let mut guard = PublicationGuard::from_json(&fs::read(&path).unwrap()).unwrap();
            guard.capture_id = DecimalU64(99);
            guard.to_json().unwrap()
        };
        fs::write(&path, &replacement).unwrap();
        let error = publication
            .finish(
                FinalOutcome::SavePointTaken(&report(&identity, 3)),
                &mut alive,
            )
            .unwrap_err();
        assert_eq!(error.published.as_deref(), Some(&identity));
        assert_eq!(fs::read(path).unwrap(), replacement);
        assert!(error.detail.contains("broken guard update failed"));
        assert!(fixture.repository().load(None, &fixture.scope()).is_err());
    }
}

#[test]
fn cancellation_at_every_stage_keeps_only_actual_required_evidence() {
    for stage in 0..6 {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        let prepared = (stage >= 1).then(|| prepare(&mut publication, 3));
        if stage >= 2 {
            publication
                .commit(&run(), DecimalU64(3), &mut alive)
                .unwrap();
        }
        for _ in 3..=stage {
            publication.advance(&mut alive).unwrap();
        }
        let error = publication.cancel("observed caller cancellation");
        assert_eq!(
            error.published.as_deref(),
            if stage == 5 { prepared.as_ref() } else { None }
        );
        assert_eq!(fixture.index().len(), if stage == 5 { 2 } else { 1 });
        for artifact in &error.artifacts {
            assert!(
                fixture
                    .base
                    .join(&artifact.location)
                    .symlink_metadata()
                    .is_ok(),
                "claimed absent artifact: {}",
                artifact.location
            );
        }
        if let Some(identity) = prepared {
            fixture.assert_guarded(&identity, PublicationState::Broken);
            let dump = error
                .artifacts
                .iter()
                .find(|artifact| artifact.kind == RetainedArtifactKind::Dump)
                .unwrap();
            assert_eq!(dump.sha256, Some(hash(CAPTURE_BYTES)));
            assert!(error.artifacts.iter().any(|artifact| artifact.kind
                == RetainedArtifactKind::Configuration
                && artifact.sha256 == Some(identity.config_sha256.clone())));
        } else {
            assert!(error.artifacts.is_empty());
            assert_eq!(fixture.names().len(), 2);
        }
    }
}

#[test]
fn stopped_publication_does_not_retry_after_partial_or_unsynced_index_append() {
    for fault in [
        fs_ops::AppendFault::PartialWrite(9),
        fs_ops::AppendFault::FileSync,
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let prefix = fixture.index_bytes();
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        let identity = prepare(&mut publication, 3);
        publication
            .commit(&run(), DecimalU64(3), &mut alive)
            .unwrap();
        publication.advance(&mut alive).unwrap();
        publication.advance(&mut alive).unwrap();
        let error =
            fs_ops::with_append_fault(fault, || publication.advance(&mut alive)).unwrap_err();
        assert!(
            error.published.is_none(),
            "unverified append does not grant Published"
        );
        let bytes = fixture.index_bytes();
        assert!(bytes.starts_with(&prefix));
        assert!(
            bytes.len() > prefix.len(),
            "failure must retain the actual append"
        );
        match fault {
            fs_ops::AppendFault::PartialWrite(_) => assert!(codec::decode_index(&bytes).is_err()),
            fs_ops::AppendFault::FileSync => {
                assert_eq!(codec::decode_index(&bytes).unwrap().len(), 2)
            }
        }
        assert!(publication.advance(&mut alive).is_err());
        assert_eq!(fixture.index_bytes(), bytes);
        fixture.assert_guarded(&identity, PublicationState::Broken);
    }
}

#[test]
fn guard_update_failure_retains_pending_or_broken_refusal_and_reports_actual_files() {
    for fault in [
        fs_ops::GuardFault::BeforeReplace,
        fs_ops::GuardFault::AfterReplace,
    ] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        let identity = publish(&mut publication, 3);
        let error = fs_ops::with_guard_fault(fault, 0, || publication.failed("peer disappeared"));
        assert_eq!(error.published.as_deref(), Some(&identity));
        assert!(
            error.detail.contains("broken guard update failed"),
            "{}",
            error.detail
        );
        let expected_state = match fault {
            fs_ops::GuardFault::BeforeReplace => PublicationState::Pending,
            fs_ops::GuardFault::AfterReplace => PublicationState::Broken,
            _ => unreachable!(),
        };
        fixture.assert_guarded(&identity, expected_state);
        let guard_artifacts = error
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == RetainedArtifactKind::Guard)
            .collect::<Vec<_>>();
        assert_eq!(
            guard_artifacts.len(),
            if matches!(fault, fs_ops::GuardFault::BeforeReplace) {
                2
            } else {
                1
            }
        );
        for artifact in guard_artifacts {
            assert!(fixture.base.join(&artifact.location).exists());
        }
        assert_eq!(fixture.index().len(), 2);
    }
}

#[test]
fn failed_final_guard_removal_keeps_the_published_identity_but_refuses_loading() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
    let identity = publish(&mut publication, 3);
    let error = fs_ops::with_guard_fault(fs_ops::GuardFault::BeforeRemove, 0, || {
        publication.finish(
            FinalOutcome::SavePointTaken(&report(&identity, 3)),
            &mut alive,
        )
    })
    .unwrap_err();
    assert_eq!(error.published.as_deref(), Some(&identity));
    assert_eq!(error.phase, PersistencePhase::Reporting);
    fixture.assert_guarded(&identity, PublicationState::Broken);
    assert_eq!(fixture.index().len(), 2);
}

#[test]
fn evidence_does_not_follow_a_replaced_part_directory_or_reuse_changed_file_hashes() {
    for replace_directory in [true, false] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        prepare(&mut publication, 3);
        let path = fixture.root.join(&publication.work_name);
        if replace_directory {
            let detached = fixture.base.join("detached");
            fs::rename(&path, &detached).unwrap();
            symlink(detached, &path).unwrap();
        } else {
            fs::write(path.join("state.sqlite"), b"changed after known digest").unwrap();
        }
        let error = publication.failed("observed filesystem replacement");
        if replace_directory {
            assert!(
                error.detail.contains("no longer names the held inode"),
                "{}",
                error.detail
            );
            assert!(!error.artifacts.iter().any(|artifact| matches!(
                artifact.kind,
                RetainedArtifactKind::Dump
                    | RetainedArtifactKind::Binding
                    | RetainedArtifactKind::Configuration
            )));
        } else {
            let dump = error
                .artifacts
                .iter()
                .find(|artifact| artifact.kind == RetainedArtifactKind::Dump)
                .unwrap();
            assert_eq!(dump.sha256, None);
        }
        for artifact in &error.artifacts {
            assert!(
                fixture
                    .base
                    .join(&artifact.location)
                    .symlink_metadata()
                    .is_ok()
            );
        }
    }
}

#[test]
fn unrelated_later_failure_cannot_retroactively_guard_a_finished_save() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
    let identity = publish(&mut publication, 3);
    publication
        .finish(
            FinalOutcome::SavePointTaken(&report(&identity, 3)),
            &mut alive,
        )
        .unwrap();
    let original = fixture.index_bytes();
    let failure = publication.failed("unrelated later peer failure");
    assert_eq!(failure.published.as_deref(), Some(&identity));
    assert!(failure.artifacts.is_empty());
    assert!(publication.advance(&mut alive).is_err());
    assert_eq!(fixture.index_bytes(), original);
    assert!(fixture.names().iter().all(|name| !name.starts_with('.')));
    drop(publication);
    assert_eq!(
        repo.load(None, &fixture.scope()).unwrap().identity(),
        identity
    );
}

fn complete_capture_with_checkpoint(
    fixture: &Fixture,
    repo: &mut Repository,
    loaded: &StartIdentity,
    checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
) -> Result<StartIdentity, Failure> {
    fixture.write_room("capture-3.sqlite.part", CAPTURE_BYTES);
    let room = open_directory(&fixture.room).unwrap();
    let mut publication = Publication::serving(
        repo,
        &fixture.scope(),
        ServingImage {
            loaded,
            run: run(),
            cause: PersistenceCause::Requested,
            capture_id: DecimalU64(3),
            room_name: "capture-3.sqlite.part",
            size_bytes: DecimalU64(CAPTURE_BYTES.len() as u64),
            position: Some(position(3)),
            room: room.as_fd(),
            member_uid: fixture.owner.uid,
        },
        checkpoint,
    )?;
    let identity =
        publication.image_finished(&run(), DecimalU64(3), "capture-3.sqlite", checkpoint)?;
    publication.commit(&run(), DecimalU64(3), checkpoint)?;
    publication.advance(checkpoint)?;
    publication.advance(checkpoint)?;
    publication.advance(checkpoint)?;
    publication.finish(
        FinalOutcome::SavePointTaken(&report(&identity, 3)),
        checkpoint,
    )
}

#[test]
fn every_live_checkpoint_can_stop_without_publishing_a_later_step() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut phases = Vec::new();
    complete_capture_with_checkpoint(&fixture, &mut repo, &loaded, &mut |phase| {
        phases.push(phase);
        Ok(())
    })
    .unwrap();
    assert!(phases.contains(&PersistencePhase::Hashing));
    assert!(phases.contains(&PersistencePhase::Preparing));
    assert!(phases.contains(&PersistencePhase::Confirming));
    assert_eq!(
        phases
            .iter()
            .filter(|&&phase| phase == PersistencePhase::Publishing)
            .count(),
        6,
        "each of rename, verify and append has before/after liveness checks"
    );
    assert_eq!(
        phases
            .iter()
            .filter(|&&phase| phase == PersistencePhase::Reporting)
            .count(),
        2,
        "final removal has before/after liveness checks"
    );
    for (stopped_at, &stopped_phase) in phases.iter().enumerate() {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut calls = 0;
        let error = complete_capture_with_checkpoint(&fixture, &mut repo, &loaded, &mut |_| {
            let current = calls;
            calls += 1;
            if current == stopped_at {
                Err("deadline or live peer failure observed".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(
            calls,
            stopped_at + 1,
            "failure at checkpoint {stopped_at} must not start later work"
        );
        assert_eq!(error.phase, stopped_phase);
        assert!(
            error
                .detail
                .contains("deadline or live peer failure observed")
        );
        for artifact in &error.artifacts {
            assert!(
                fixture
                    .base
                    .join(&artifact.location)
                    .symlink_metadata()
                    .is_ok(),
                "checkpoint {stopped_at}: {}",
                artifact.location
            );
        }
        if let Some(identity) = &error.published {
            assert_eq!(fixture.index().len(), 2);
            fixture.assert_guarded(identity, PublicationState::Broken);
        } else {
            assert_eq!(fixture.index().len(), 1);
        }
        if !error
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == RetainedArtifactKind::Guard)
        {
            assert_eq!(
                fixture.names().len(),
                2,
                "pre-guard failure removes owned work at checkpoint {stopped_at}"
            );
        }
    }
}

#[test]
fn rename_and_index_checkpoints_observe_both_sides_of_the_actual_change() {
    for stop_after_append in [false, true] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        let identity = prepare(&mut publication, 3);
        publication
            .commit(&run(), DecimalU64(3), &mut alive)
            .unwrap();
        if stop_after_append {
            publication.advance(&mut alive).unwrap();
            publication.advance(&mut alive).unwrap();
        }
        let mut calls = 0;
        let error = publication
            .advance(&mut |phase| {
                assert_eq!(phase, PersistencePhase::Publishing);
                if stop_after_append {
                    assert_eq!(fixture.index().len(), 1 + calls);
                } else {
                    assert_eq!(
                        fixture.root.join(&identity.artifact_id).exists(),
                        calls == 1
                    );
                }
                calls += 1;
                if calls == 2 {
                    Err("deadline expired just after durable action".into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(calls, 2);
        assert_eq!(
            error.published.as_deref(),
            stop_after_append.then_some(&identity)
        );
        fixture.assert_guarded(&identity, PublicationState::Broken);
        assert_eq!(fixture.index().len(), if stop_after_append { 2 } else { 1 });
    }
}

#[test]
fn draft_cannot_change_the_judged_session_or_trace_territory() {
    for rebind in [false, true] {
        for bad_session in [false, true] {
            let fixture = Fixture::new();
            let mut repo = fixture.repository();
            if rebind {
                fixture.seed_complete(&mut repo);
            }
            let previous = fixture.index_bytes();
            let names = fixture.names();
            let original = declaration::render(&declaration::tests::fixture()).unwrap();
            let text = if bad_session {
                original.replace("fixture-session", "other-session")
            } else {
                original.replace("/fixture/trace.jsonl", "/another/trace.jsonl")
            };
            let draft = parse_serving_draft(&text).unwrap();
            let error = if rebind {
                Publication::rebind(&mut repo, &fixture.scope(), None, draft, &mut alive)
                    .err()
                    .unwrap()
            } else {
                let created = fixture.seed_image();
                let room = open_directory(&fixture.room).unwrap();
                Publication::seed(
                    &mut repo,
                    &fixture.scope(),
                    draft,
                    SeedImage {
                        expected_seed_id: &created.seed_id,
                        created: &created,
                        room: room.as_fd(),
                        member_uid: fixture.owner.uid,
                    },
                    &mut alive,
                )
                .err()
                .unwrap()
            };
            assert_eq!(error.case, PersistenceCase::ConfigurationMismatch);
            assert_eq!(fixture.index_bytes(), previous);
            assert_eq!(fixture.names(), names);
        }
    }
}

#[test]
fn failed_parent_sync_after_guard_unlink_recreates_broken_refusal() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
    let identity = publish(&mut publication, 3);
    let error = fs_ops::with_guard_fault(fs_ops::GuardFault::AfterRemoveDirectorySync, 0, || {
        publication.finish(
            FinalOutcome::SavePointTaken(&report(&identity, 3)),
            &mut alive,
        )
    })
    .unwrap_err();
    assert_eq!(error.published.as_deref(), Some(&identity));
    assert!(error.detail.contains("guard read"), "{}", error.detail);
    fixture.assert_guarded(&identity, PublicationState::Broken);
    assert_eq!(fixture.index().len(), 2);
}

#[test]
fn failed_preguard_cleanup_retains_foreign_file_and_reports_owned_evidence() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let mut publication = fixture.seed(&mut repo);
    let part = fixture.root.join(&publication.work_name);
    fs::write(part.join("foreign-file"), b"not owned by this attempt").unwrap();
    let error = publication.cancel("seed process cleanup failed");
    assert!(
        error.detail.contains("unlisted part cleanup failed"),
        "{}",
        error.detail
    );
    assert_eq!(
        fs::read(part.join("foreign-file")).unwrap(),
        b"not owned by this attempt"
    );
    assert_eq!(fs::read(part.join("state.sqlite")).unwrap(), SEED_BYTES);
    assert!(error.published.is_none());
    assert!(
        error
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == RetainedArtifactKind::Part)
    );
    assert!(
        error
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == RetainedArtifactKind::Dump
                && artifact.sha256 == Some(hash(SEED_BYTES)))
    );
    assert!(
        !error
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == RetainedArtifactKind::Guard)
    );
    assert!(fixture.index().is_empty());
}

#[test]
fn preexisting_guard_collision_is_never_replaced_or_removed() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
    let configuration =
        declaration::render(&declaration::tests::fixture().with_start(hash(CAPTURE_BYTES)))
            .unwrap();
    let id = ArtifactId::from_parts(DecimalU64(2), hash(configuration.as_bytes())).unwrap();
    let guard_path = fixture.root.join(format!(".{id}.pending"));
    let foreign_bytes = b"existing refusal owned by another attempt";
    fs::write(&guard_path, foreign_bytes).unwrap();
    fs::set_permissions(&guard_path, fs::Permissions::from_mode(0o600)).unwrap();
    let error = publication
        .image_finished(&run(), DecimalU64(3), "capture-3.sqlite", &mut alive)
        .unwrap_err();
    assert_eq!(fs::read(guard_path).unwrap(), foreign_bytes);
    assert!(!fixture.root.join(&publication.work_name).exists());
    assert!(error.published.is_none());
    assert_eq!(fixture.index().len(), 1);
    assert!(!fixture.root.join(id.to_string()).exists());
}

#[test]
fn rename_collision_and_readback_corruption_never_reach_index() {
    for collision in [true, false] {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let loaded = fixture.seed_complete(&mut repo);
        let prefix = fixture.index_bytes();
        let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
        let identity = prepare(&mut publication, 3);
        publication
            .commit(&run(), DecimalU64(3), &mut alive)
            .unwrap();
        let final_path = fixture.root.join(&identity.artifact_id);
        let error = if collision {
            fs::create_dir(&final_path).unwrap();
            fs::write(final_path.join("existing"), b"do not replace").unwrap();
            publication.advance(&mut alive).unwrap_err()
        } else {
            publication.advance(&mut alive).unwrap();
            fs::write(final_path.join("agent.toml"), b"corrupt readback").unwrap();
            publication.advance(&mut alive).unwrap_err()
        };
        assert!(error.published.is_none());
        assert_eq!(fixture.index_bytes(), prefix);
        assert!(publication.advance(&mut alive).is_err());
        fixture.assert_guarded(&identity, PublicationState::Broken);
        if collision {
            assert_eq!(
                fs::read(final_path.join("existing")).unwrap(),
                b"do not replace"
            );
        }
    }
}

#[test]
fn changed_index_prefix_is_preserved_and_never_retried() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let loaded = fixture.seed_complete(&mut repo);
    let mut publication = fixture.capture(&mut repo, &loaded, 3, PersistenceCause::Requested);
    let identity = prepare(&mut publication, 3);
    publication
        .commit(&run(), DecimalU64(3), &mut alive)
        .unwrap();
    publication.advance(&mut alive).unwrap();
    publication.advance(&mut alive).unwrap();
    let mut changed = fixture.index_bytes();
    changed.extend_from_slice(b"foreign append\n");
    fs::write(fixture.root.join(MANIFEST), &changed).unwrap();
    let error = publication.advance(&mut alive).unwrap_err();
    assert!(error.published.is_none());
    assert_eq!(fixture.index_bytes(), changed);
    assert!(publication.advance(&mut alive).is_err());
    assert_eq!(fixture.index_bytes(), changed);
    fixture.assert_guarded(&identity, PublicationState::Broken);
}

fn complete_seed_with_checkpoint(
    fixture: &Fixture,
    repo: &mut Repository,
    checkpoint: &mut impl FnMut(PersistencePhase) -> Result<(), String>,
) -> Result<StartIdentity, Failure> {
    let created = fixture.seed_image();
    let room = open_directory(&fixture.room).unwrap();
    let mut publication = Publication::seed(
        repo,
        &fixture.scope(),
        draft(),
        SeedImage {
            expected_seed_id: &created.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: fixture.owner.uid,
        },
        checkpoint,
    )?;
    publication.seed_cleanup_complete(checkpoint)?;
    publication.advance(checkpoint)?;
    publication.advance(checkpoint)?;
    let Progress::Complete(identity) = publication.advance(checkpoint)? else {
        panic!("seed incomplete")
    };
    Ok(identity)
}

#[test]
fn every_seed_checkpoint_stops_later_work_without_fabricating_a_guard() {
    let fixture = Fixture::new();
    let mut repo = fixture.repository();
    let mut phases = Vec::new();
    complete_seed_with_checkpoint(&fixture, &mut repo, &mut |phase| {
        phases.push(phase);
        Ok(())
    })
    .unwrap();
    for (stopped_at, &stopped_phase) in phases.iter().enumerate() {
        let fixture = Fixture::new();
        let mut repo = fixture.repository();
        let mut calls = 0;
        let error = complete_seed_with_checkpoint(&fixture, &mut repo, &mut |_| {
            let current = calls;
            calls += 1;
            if current == stopped_at {
                Err("seed deadline expired".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(calls, stopped_at + 1);
        assert_eq!(error.phase, stopped_phase);
        assert!(error.detail.contains("seed deadline expired"));
        assert!(
            !error
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == RetainedArtifactKind::Guard)
        );
        assert!(
            !fixture
                .names()
                .iter()
                .any(|name| name.ends_with(".pending"))
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
        if let Some(identity) = error.published.as_deref() {
            assert_eq!(fixture.index().len(), 1);
            assert_eq!(
                &repo.load(None, &fixture.scope()).unwrap().identity(),
                identity
            );
        } else {
            assert!(fixture.index().is_empty());
        }
    }
}
