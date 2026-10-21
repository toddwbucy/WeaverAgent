//! Seed publication ordering, with real files and explicit cleanup outcomes.

use super::*;
use crate::save_points::MANIFEST;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use weaver_types::state_flow::{DecimalU64, SeedCreated};

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
