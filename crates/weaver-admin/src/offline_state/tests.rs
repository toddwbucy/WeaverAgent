//! Coordinator evidence uses real locks and artifact publication. The synthetic
//! member image is deliberately not SQLite, which the member alone certifies.

use super::*;
use std::fs;
use std::os::fd::AsFd;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command};

use preparation::tests::{Fixture, fixture};
use weaver_types::state_flow::{DecimalU64, PersistenceCase, SeedCreated};

use crate::save_points::MANIFEST;
use crate::save_points::serving::codec::ArtifactId;
use crate::save_points::serving::publication::SeedImage;

const IMAGE: &[u8] = b"synthetic member image with unchanged prompt bytes";

fn repository(fixture: &Fixture) -> Repository {
    let (uid, gid) = fixture.config.file_owner();
    Repository::from_judged_directory(
        fixture
            .config
            .save_points_fd()
            .unwrap()
            .try_clone_to_owned()
            .unwrap(),
        Owner { uid, gid },
        fixture.config.access_gid,
    )
    .unwrap()
}

fn seed(fixture: &Fixture) -> StartIdentity {
    let prepared = fixture.prepare(false).unwrap();
    let scope = LoadScope {
        session: &prepared.draft.agent().session,
        trace_path: trace_path(&prepared.draft.agent().trace_sink),
    };
    let room_path = fixture.config.root.with_file_name("synthetic-member-room");
    fs::create_dir(&room_path).unwrap();
    fs::set_permissions(&room_path, fs::Permissions::from_mode(0o700)).unwrap();
    let seed_id = "1".repeat(32);
    let room_name = format!("seed-{seed_id}.sqlite");
    fs::write(room_path.join(&room_name), IMAGE).unwrap();
    fs::set_permissions(
        room_path.join(&room_name),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let created = SeedCreated {
        schema_id: "state-content/1".into(),
        seed_id,
        room_name,
        size_bytes: DecimalU64(IMAGE.len() as u64),
    };
    let room = crate::save_points::open_directory(&room_path).unwrap();
    let mut repository = repository(fixture);
    let mut publication = Publication::seed(
        &mut repository,
        &scope,
        prepared.draft.clone(),
        SeedImage {
            expected_seed_id: &created.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: nix::unistd::geteuid().as_raw(),
        },
        &mut |_| Ok(()),
    )
    .unwrap();
    // No child exists in this artifact fixture. This acknowledges that fact,
    // not a claim that these tests establish member process cleanup.
    publication.seed_cleanup_complete(&mut |_| Ok(())).unwrap();
    loop {
        match publication.advance(&mut |_| Ok(())).unwrap() {
            Progress::Renamed | Progress::Verified => {}
            Progress::Complete(identity) => return identity,
            Progress::Published(_) => panic!("seed entered serving confirmation"),
        }
    }
}

fn run_fixture(fixture: &Fixture, operation: Operation) -> Result<StartIdentity, Failure> {
    run_authorized(
        &fixture.config,
        &AgentName(fixture.config.agent.clone()),
        operation,
        |config, agent, required| {
            preparation::prepare_against(config, agent, required, &fixture.boundary)
        },
    )
}

fn index(fixture: &Fixture) -> Vec<u8> {
    fs::read(fixture.config.territory.join("save-points").join(MANIFEST)).unwrap()
}

fn entries(fixture: &Fixture) -> Vec<serde_json::Value> {
    String::from_utf8(index(fixture))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn triple(fixture: &Fixture, identity: &StartIdentity) -> [Vec<u8>; 3] {
    let directory = fixture
        .config
        .territory
        .join("save-points")
        .join(&identity.artifact_id);
    ["state.sqlite", "agent.toml", "binding.json"]
        .map(|name| fs::read(directory.join(name)).unwrap())
}

fn assert_admission(result: Result<StartIdentity, Failure>, expected: LifecycleRefusal) {
    match result {
        Err(Failure::Admission(actual)) => assert_eq!(*actual, expected),
        other => panic!("expected admission {expected:?}, got {other:?}"),
    }
}

#[test]
fn authorization_distinguishes_root_bind_from_authenticated_restore() {
    use std::ffi::OsStr;
    for operation in [Operation::Restore, Operation::BindState] {
        assert!(authorize(operation, true, crate::cause_from(None)).is_ok());
        assert!(matches!(
            authorize(operation, false, crate::cause_from(None)),
            Err(LifecycleRefusal::Unauthorized)
        ));
        assert!(matches!(
            authorize(operation, true, crate::cause_from(Some(OsStr::new("bad")))),
            Err(LifecycleRefusal::Malformed)
        ));
    }
    assert!(
        authorize(
            Operation::Restore,
            true,
            crate::cause_from(Some(OsStr::new("1000")))
        )
        .is_ok()
    );
    assert!(matches!(
        authorize(
            Operation::BindState,
            true,
            crate::cause_from(Some(OsStr::new("1000")))
        ),
        Err(LifecycleRefusal::Unauthorized)
    ));
}

#[test]
fn public_entry_points_refuse_nonroot_before_any_file_admission() {
    let fixture = fixture();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "offline_state::tests::public_refusal_process",
            "--test-threads",
            "1",
        ])
        .env("WEAVER_OFFLINE_REFUSAL", "yes");
    if nix::unistd::getuid().is_root() {
        use std::os::unix::process::CommandExt;
        command.uid(65534).gid(65534);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.config.run_directory().exists());
}

#[test]
fn public_refusal_process() {
    if std::env::var_os("WEAVER_OFFLINE_REFUSAL").is_none() {
        return;
    }
    assert!(!nix::unistd::getuid().is_root());
    let mut fixture = fixture();
    // These paths are unjudged and absent. Removal of wrapper authorization
    // reaches a different refusal, or creates coordination files.
    fixture.config.root = fixture.config.root.join("not-present");
    let agent = AgentName(fixture.config.agent.clone());
    assert_admission(
        restore(&fixture.config, &agent),
        LifecycleRefusal::Unauthorized,
    );
    assert_admission(
        bind_state(&fixture.config, &agent),
        LifecycleRefusal::Unauthorized,
    );
    assert!(!fixture.config.run_directory().exists());
    assert!(!fixture.config.root.exists());
}

struct HeldChild(Child);

impl Drop for HeldChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn probe_locks(directory: &Path) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "offline_state::tests::lock_probe_process",
            "--test-threads",
            "1",
        ])
        .env("WEAVER_OFFLINE_LOCK_PROBE", directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lock_probe_process() {
    let Some(directory) = std::env::var_os("WEAVER_OFFLINE_LOCK_PROBE") else {
        return;
    };
    let directory = Path::new(&directory);
    assert!(matches!(
        start::take_invocation_lock(directory, verb_table::HolderVerb::Restore),
        Err(LifecycleRefusal::InvocationInFlight)
    ));
    assert!(start::take_run_lock(directory).unwrap().is_none());
}

#[test]
fn both_real_locks_precede_file_preparation_and_are_released_afterward() {
    for operation in [Operation::Restore, Operation::BindState] {
        let fixture = fixture();
        let identity = seed(&fixture);
        fixture.selector(identity.artifact_id.as_bytes());
        run_authorized(
            &fixture.config,
            &AgentName(fixture.config.agent.clone()),
            operation,
            |config, agent, required| {
                probe_locks(&config.run_directory());
                preparation::prepare_against(config, agent, required, &fixture.boundary)
            },
        )
        .unwrap();
        let run = start::take_run_lock(&fixture.config.run_directory()).unwrap();
        assert!(run.is_some());
        drop(run);
        let invocation = start::take_invocation_lock(
            &fixture.config.run_directory(),
            verb_table::HolderVerb::Restore,
        )
        .unwrap();
        drop(invocation);
    }
}

#[test]
fn competing_invocation_or_held_run_never_reaches_preparation() {
    for operation in [Operation::Restore, Operation::BindState] {
        let fixture = fixture();
        seed(&fixture);
        let before = index(&fixture);
        let directory =
            crate::prepare_run_directory(&fixture.config, nix::unistd::geteuid().as_raw()).unwrap();
        let child = HeldChild(start::tests::holding_invocation(
            &directory,
            &[("WEAVER_ADMIN_HOLDER_MODE", "take:restore")],
        ));
        let run = || {
            run_authorized(
                &fixture.config,
                &AgentName(fixture.config.agent.clone()),
                operation,
                |_, _, _| panic!("contended operation reached preparation"),
            )
        };
        assert_admission(run(), LifecycleRefusal::InvocationInFlight);
        drop(child);
        let held = start::take_run_lock(&directory).unwrap().unwrap();
        assert_admission(run(), LifecycleRefusal::AgentRunning);
        drop(held);
        assert_eq!(index(&fixture), before);
    }
}

#[test]
fn unloaded_judgment_precedes_preparation_and_is_repeated_for_publication() {
    for changed_after_prepare in [false, true] {
        for operation in [Operation::Restore, Operation::BindState] {
            let fixture = fixture();
            let identity = seed(&fixture);
            fixture.selector(identity.artifact_id.as_bytes());
            let before = index(&fixture);
            let directory =
                crate::prepare_run_directory(&fixture.config, nix::unistd::geteuid().as_raw())
                    .unwrap();
            let record = directory.join(crate::containment::RECORD);
            if !changed_after_prepare {
                fs::write(&record, b"unreadable record").unwrap();
            }
            let result = run_authorized(
                &fixture.config,
                &AgentName(fixture.config.agent.clone()),
                operation,
                |config, agent, required| {
                    assert!(
                        changed_after_prepare,
                        "unverified cgroup reached preparation"
                    );
                    let prepared =
                        preparation::prepare_against(config, agent, required, &fixture.boundary)?;
                    fs::write(&record, b"unreadable record").unwrap();
                    Ok(prepared)
                },
            );
            if changed_after_prepare {
                assert!(
                    matches!(
                        result,
                        Err(Failure::Publication(_)) | Err(Failure::Restore(_))
                    ),
                    "{result:?}"
                );
            } else {
                assert_admission(result, LifecycleRefusal::BoundaryUnverified);
            }
            assert_eq!(index(&fixture), before);
            assert_eq!(fs::read(&record).unwrap(), b"unreadable record");
        }
    }
}

#[test]
fn bind_keeps_the_image_and_seed_but_binds_the_judged_new_configuration() {
    let fixture = fixture();
    let original = seed(&fixture);
    let before = triple(&fixture, &original);
    fixture.edit("synthetic-decoder", "replacement-decoder");
    let draft_bytes = fs::read(fixture.declaration()).unwrap();
    let rebound = run_fixture(&fixture, Operation::BindState).unwrap();
    assert_eq!(rebound.dump_sha256, original.dump_sha256);
    assert_ne!(rebound.config_sha256, original.config_sha256);
    assert_eq!(rebound.seed_id, original.seed_id);
    assert_eq!(rebound.position, original.position);
    assert_eq!(
        rebound.parent_artifact_id.as_deref(),
        Some(original.artifact_id.as_str())
    );
    let after = triple(&fixture, &rebound);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[0], IMAGE);
    assert!(String::from_utf8_lossy(&after[1]).contains("replacement-decoder"));
    assert_eq!(triple(&fixture, &original), before);
    assert_eq!(fs::read(fixture.declaration()).unwrap(), draft_bytes);
    assert!(fs::read_dir(&fixture.config.root).unwrap().next().is_none());
}

#[test]
fn restore_historic_and_latest_append_selection_without_rewriting_the_artifact() {
    let fixture = fixture();
    let first = seed(&fixture);
    let original = triple(&fixture, &first);
    fixture.edit("synthetic-decoder", "replacement-decoder");
    let second = run_fixture(&fixture, Operation::BindState).unwrap();
    let second_bytes = triple(&fixture, &second);
    let draft_bytes = fs::read(fixture.declaration()).unwrap();
    fixture.selector(first.artifact_id.as_bytes());
    for ordinal in [3, 4] {
        let prefix = index(&fixture);
        let restored = run_fixture(&fixture, Operation::Restore).unwrap();
        assert_eq!(restored, first);
        assert!(index(&fixture).starts_with(&prefix));
        let rows = entries(&fixture);
        let latest = rows.last().unwrap();
        assert_eq!(latest["ordinal"], ordinal.to_string());
        assert_eq!(latest["artifact_id"], first.artifact_id);
        assert_eq!(latest["arrival"], "selected");
        let mut expected = rows[0].clone();
        expected["ordinal"] = serde_json::json!(ordinal.to_string());
        expected["arrival"] = serde_json::json!("selected");
        assert_eq!(*latest, expected);
        assert_eq!(triple(&fixture, &first), original);
        assert_eq!(triple(&fixture, &second), second_bytes);
        assert_eq!(fs::read(fixture.declaration()).unwrap(), draft_bytes);
    }
}

#[test]
fn bind_rejects_a_stale_selector_and_a_changed_scheme_before_publication() {
    let fixture = fixture();
    let first = seed(&fixture);
    fixture.edit("synthetic-decoder", "replacement-decoder");
    let second = run_fixture(&fixture, Operation::BindState).unwrap();
    fixture.selector(first.artifact_id.as_bytes());
    let before = index(&fixture);
    let stale = run_fixture(&fixture, Operation::BindState).unwrap_err();
    assert!(
        matches!(stale, Failure::Publication(ref e) if e.case == PersistenceCase::ConfigurationMismatch),
        "{stale:?}"
    );
    assert_eq!(index(&fixture), before);
    fixture.selector(second.artifact_id.as_bytes());
    fixture.edit(&"b".repeat(64), &"d".repeat(64));
    let scheme = run_fixture(&fixture, Operation::BindState).unwrap_err();
    assert!(
        matches!(scheme, Failure::Publication(ref e) if e.case == PersistenceCase::SchemeMismatch),
        "{scheme:?}"
    );
    assert_eq!(index(&fixture), before);
}

#[test]
fn independently_judged_identity_and_pending_guards_cannot_be_bypassed() {
    for operation in [Operation::Restore, Operation::BindState] {
        for defect in ["session", "trace", "guard"] {
            let fixture = fixture();
            let first = seed(&fixture);
            fixture.selector(first.artifact_id.as_bytes());
            match defect {
                "session" => fixture.edit("offline-session", "different-session"),
                "trace" => fixture.edit("trace.jsonl", "different-trace.jsonl"),
                "guard" => {
                    let path = fixture
                        .config
                        .territory
                        .join("save-points")
                        .join(format!(".{}.pending", first.artifact_id));
                    fs::write(
                        &path,
                        serde_json::to_vec(&serde_json::json!({
                            "format": "state-publication/1",
                            "artifact_id": first.artifact_id,
                            "capture_id": "2",
                            "state": "pending",
                            "phase": "reporting",
                            "locations": [first.artifact_id],
                        }))
                        .unwrap(),
                    )
                    .unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
                }
                _ => unreachable!(),
            }
            let before = index(&fixture);
            let error = run_fixture(&fixture, operation).unwrap_err();
            let case = match error {
                Failure::Publication(error) => error.case,
                Failure::Restore(error) => {
                    assert!(!error.selection_may_be_appended);
                    error.fault.case
                }
                other => panic!("wrong refusal for {defect}: {other:?}"),
            };
            assert_eq!(
                case,
                if defect == "guard" {
                    PersistenceCase::BrokenSavePoint
                } else {
                    PersistenceCase::ConfigurationMismatch
                }
            );
            assert_eq!(index(&fixture), before);
        }
    }
}

#[test]
fn restore_retains_append_uncertainty_when_the_final_checkpoint_fails() {
    let fixture = fixture();
    let first = seed(&fixture);
    fixture.selector(first.artifact_id.as_bytes());
    let before = index(&fixture);
    let mut repository = repository(&fixture);
    let result = perform(
        &mut repository,
        fixture.prepare(true).unwrap(),
        Operation::Restore,
        &mut |phase| {
            if phase == PersistencePhase::Reporting {
                Err("lost unloaded proof after append".into())
            } else {
                Ok(())
            }
        },
    );
    match result {
        Err(Failure::Restore(error)) => {
            assert!(error.selection_may_be_appended);
            assert!(
                error
                    .fault
                    .detail
                    .contains("lost unloaded proof after append")
            );
        }
        other => panic!("lost restore uncertainty: {other:?}"),
    }
    assert!(index(&fixture).starts_with(&before));
    assert_eq!(entries(&fixture).len(), 2);
    let prepared = fixture.prepare(true).unwrap();
    let scope = LoadScope {
        session: &prepared.draft.agent().session,
        trace_path: trace_path(&prepared.draft.agent().trace_sink),
    };
    assert_eq!(
        repository
            .load(
                Some(&ArtifactId::parse(&first.artifact_id).unwrap()),
                &scope
            )
            .unwrap()
            .identity(),
        first
    );
}
