//! Seed publication ordering, with real files and explicit cleanup outcomes.

use super::*;
use crate::save_points::MANIFEST;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use weaver_types::state_flow::{DecimalU64, SeedCreated};

fn coordination(fixture: &preparation::tests::Fixture) -> std::path::PathBuf {
    fs::create_dir(&fixture.config.coordination_root).unwrap();
    fs::set_permissions(
        &fixture.config.coordination_root,
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    crate::prepare_run_directory(&fixture.config, nix::unistd::geteuid().as_raw()).unwrap()
}

#[test]
fn seed_reads_the_unloaded_ladder_before_inventory_and_preserves_unverified_evidence() {
    if crate::test_isolation::isolated!(
        seed_reads_the_unloaded_ladder_before_inventory_and_preserves_unverified_evidence
    ) {
        return;
    }
    let fixture = preparation::tests::fixture();
    let directory = coordination(&fixture);
    let agent = AgentName(fixture.config.agent.clone());
    repository(&fixture.config)
        .unwrap()
        .require_unseeded()
        .unwrap();
    // An absent record is the real ladder's Unloaded case and reaches inventory.
    let absent = run_authorized(&fixture.config, &agent, Instant::now(), |_, _, _| {
        Err(preparation::PreparationFailure::Certificate(
            "inventory reached".into(),
        ))
    });
    assert!(matches!(
        absent,
        Err(Failure::Preparation(preparation::PreparationFailure::Certificate(ref detail)))
            if detail == "inventory reached"
    ));

    let record = directory.join(crate::containment::RECORD);
    fs::write(&record, b"unverified cgroup record").unwrap();
    let refused = run_authorized(&fixture.config, &agent, Instant::now(), |_, _, _| {
        panic!("unverified cgroup reached seed inventory or member startup")
    });
    assert!(matches!(
        refused,
        Err(Failure::Admission(ref error)) if **error == LifecycleRefusal::BoundaryUnverified
    ));
    assert_eq!(fs::read(&record).unwrap(), b"unverified cgroup record");
    assert!(
        fs::read_dir(fixture.config.territory.join("save-points"))
            .unwrap()
            .next()
            .is_none()
    );
    // The refusal releases both actual locks without removing the evidence.
    let _run = start::take_run_lock(&directory).unwrap().unwrap();
    let _invocation =
        start::take_invocation_lock(&directory, verb_table::HolderVerb::Seed).unwrap();
}

struct HeldChild(Child);

impl Drop for HeldChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A free run lock does not establish Unloaded while the recorded cgroup has
/// a surviving process. This uses the real kernel record and ladder, without
/// standing a seed member or claiming the complete root-host seed instrument.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_seed_refuses_a_populated_recorded_cgroup_with_a_free_run_lock() {
    let fixture = preparation::tests::fixture();
    let directory = coordination(&fixture);
    let directory_fd = crate::agent_cgroup::open_run_directory(&directory).unwrap();
    let scratch = crate::containment::tests::ScratchCgroup::new("seed-refusal");
    let cgroup = scratch
        .cgroup
        .make_child(&crate::agent_cgroup::cgroup_name(&fixture.config.agent))
        .unwrap();
    let mut child = HeldChild(crate::containment::tests::escaper_in(&cgroup));
    assert_eq!(cgroup.populated(), Some(true));
    assert!(cgroup.procs().unwrap().contains(&(child.0.id() as i32)));
    let (dev, ino) = cgroup.identity().unwrap();
    let record = crate::containment::Record {
        path: cgroup.relative().to_string(),
        dev,
        ino,
        run: "earlier-run-with-surviving-processes".into(),
    };
    crate::containment::write_record(directory_fd.as_fd(), &record).unwrap();
    let before = fs::read(directory.join(crate::containment::RECORD)).unwrap();
    let free = start::take_run_lock(&directory).unwrap().unwrap();
    drop(free);
    repository(&fixture.config)
        .unwrap()
        .require_unseeded()
        .unwrap();
    let refused = run_authorized(
        &fixture.config,
        &AgentName(fixture.config.agent.clone()),
        Instant::now(),
        |_, _, _| panic!("populated cgroup reached seed inventory or member startup"),
    );
    assert!(
        matches!(
            refused,
            Err(Failure::Refused {
                case: PersistenceCase::SeedRequiresUnloaded,
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(child.0.try_wait().unwrap().is_none());
    assert_eq!(cgroup.populated(), Some(true));
    assert_eq!(
        fs::read(directory.join(crate::containment::RECORD)).unwrap(),
        before
    );
    assert!(
        fs::read_dir(fixture.config.territory.join("save-points"))
            .unwrap()
            .next()
            .is_none()
    );
}

fn copied<'a>(
    fixture: &preparation::tests::Fixture,
    repository: &'a mut Repository,
) -> Publication<'a> {
    let request = fixture.prepared.request();
    let room = fixture.config.territory.join("test-member-room");
    std::fs::create_dir(&room).unwrap();
    let bytes = b"member image checked at the member seam";
    let name = format!("seed-{}.sqlite", request.seed_id);
    std::fs::write(room.join(&name), bytes).unwrap();
    std::fs::set_permissions(room.join(&name), std::fs::Permissions::from_mode(0o600)).unwrap();
    let room = crate::save_points::open_directory(&room).unwrap();
    let created = SeedCreated {
        schema_id: request.schema_id.clone(),
        seed_id: request.seed_id.clone(),
        room_name: name,
        size_bytes: DecimalU64(bytes.len() as u64),
    };
    let draft = fixture.prepared.draft();
    Publication::seed(
        repository,
        &LoadScope {
            session: &draft.agent().session,
            trace_path: trace_path(&draft.agent().trace_sink),
        },
        draft.clone(),
        SeedImage {
            expected_seed_id: &request.seed_id,
            created: &created,
            room: room.as_fd(),
            member_uid: nix::unistd::geteuid().as_raw(),
        },
        &mut |_| Ok(()),
    )
    .unwrap()
}

#[test]
fn publication_follows_successful_cleanup_and_retains_seed_identity() {
    let fixture = preparation::tests::fixture();
    let mut repository = repository(&fixture.config).unwrap();
    let publication = copied(&fixture, &mut repository);
    let cleaned = AtomicBool::new(false);
    let path = fixture.config.territory.join("save-points").join(MANIFEST);
    let result = finish_publication(publication, Instant::now() + Duration::from_secs(5), || {
        assert!(!path.exists(), "no indexed seed precedes process cleanup");
        cleaned.store(true, Ordering::Relaxed);
        Ok(())
    })
    .unwrap();
    assert!(cleaned.load(Ordering::Relaxed));
    assert_eq!(result.seed_id, fixture.prepared.request().seed_id);
    assert!(result.position.is_none());
    assert!(result.parent_artifact_id.is_none());
    assert!(path.exists());
    assert!(repository.require_unseeded().is_err());
}

#[test]
fn cleanup_failure_discards_the_copied_part_and_never_indexes_it() {
    let fixture = preparation::tests::fixture();
    let mut repository = repository(&fixture.config).unwrap();
    let publication = copied(&fixture, &mut repository);
    let failure = finish_publication(publication, Instant::now() + Duration::from_secs(5), || {
        Err("member reaped, but cgroup removal failed".into())
    })
    .unwrap_err();
    assert!(failure.to_string().contains("cgroup removal failed"));
    let entries = std::fs::read_dir(fixture.config.territory.join("save-points"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        entries.is_empty(),
        "failed cleanup leaves no root seed part or index"
    );
    repository.require_unseeded().unwrap();
}

#[test]
fn cleanup_runs_after_deadline_but_an_expired_operation_never_publishes() {
    let fixture = preparation::tests::fixture();
    let mut repository = repository(&fixture.config).unwrap();
    let publication = copied(&fixture, &mut repository);
    let cleaned = AtomicBool::new(false);
    assert!(
        finish_publication(publication, Instant::now(), || {
            cleaned.store(true, Ordering::Relaxed);
            Ok(())
        })
        .is_err()
    );
    assert!(
        cleaned.load(Ordering::Relaxed),
        "expiry must not strand the member"
    );
    assert!(
        !fixture
            .config
            .territory
            .join("save-points")
            .join(MANIFEST)
            .exists()
    );
    repository.require_unseeded().unwrap();
}

#[test]
fn os_seed_identifiers_have_the_exact_length_alphabet_and_freshness() {
    let first = mint_seed_id().unwrap();
    let second = mint_seed_id().unwrap();
    assert_eq!(first.len(), 32);
    assert!(
        first
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_ne!(first, second);
}
