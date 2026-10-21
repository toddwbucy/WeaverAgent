//! Seed admission never turns missing history into a second starting chain.

use super::*;

#[test]
fn seed_requires_a_new_chain_and_never_an_indexed_one() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    repository.require_unseeded().unwrap();
    fixture.seed();
    assert_eq!(
        repository.require_unseeded().unwrap_err().case,
        PersistenceCase::SeedExists
    );
}

#[test]
fn missing_or_empty_index_beside_a_published_artifact_refuses_reseeding() {
    for absent in [true, false] {
        let fixture = Fixture::new();
        let published = fixture.seed();
        if absent {
            fs::remove_file(fixture.path.join(MANIFEST)).unwrap();
        } else {
            fixture.rewrite_index(&[]);
        }
        let repository = fixture.repository();
        for _ in 0..2 {
            let failure = repository.require_unseeded().unwrap_err();
            assert_eq!(failure.case, PersistenceCase::UnsupportedLayout);
            assert!(failure.detail.contains(published.artifact_id.as_str()));
        }
        assert!(fixture.path.join(published.artifact_id.as_str()).is_dir());
        assert_eq!(fixture.path.join(MANIFEST).exists(), !absent);
    }
}

#[test]
fn unindexed_guard_or_legacy_publication_refuses_without_reading_or_promoting_it() {
    let artifact = format!("{:020}-{}", 1, "a".repeat(64));
    for name in [
        format!(".{artifact}.pending"),
        format!(".{artifact}.pending.broken"),
        ".damaged-name.pending".into(),
        "legacy.save-point".into(),
    ] {
        let fixture = Fixture::new();
        // Malformed evidence is still a refusal. Reading it as an artifact
        // would be an unrequested recovery or format judgment.
        fixture.write(&name, b"retained evidence, not a valid artifact", 0o600);
        let repository = fixture.repository();
        let failure = repository.require_unseeded().unwrap_err();
        assert_eq!(failure.case, PersistenceCase::UnsupportedLayout);
        assert!(failure.detail.contains(&name));
        assert_eq!(
            fs::read(fixture.path.join(&name)).unwrap(),
            b"retained evidence, not a valid artifact"
        );
        assert!(!fixture.path.join(MANIFEST).exists());
    }
}

#[test]
fn artifact_shaped_links_refuse_without_following_them() {
    let fixture = Fixture::new();
    let artifact = format!("{:020}-{}", 1, "b".repeat(64));
    symlink("absent-target", fixture.path.join(&artifact)).unwrap();
    assert!(fixture.repository().require_unseeded().is_err());
    assert!(!fixture.path.join("absent-target").exists());
}

#[test]
fn unlisted_parts_are_not_mistaken_for_a_completed_seed() {
    let fixture = Fixture::new();
    let part = fixture
        .path
        .join(format!(".publication-{}.part", "c".repeat(32)));
    fs::create_dir(&part).unwrap();
    fs::write(part.join("state.sqlite"), b"an incomplete image").unwrap();
    let repository = fixture.repository();
    repository.require_unseeded().unwrap();
    repository.require_unseeded().unwrap();
    assert!(repository.load(None, &fixture.scope()).is_err());
    assert_eq!(
        fs::read(part.join("state.sqlite")).unwrap(),
        b"an incomplete image"
    );
    assert!(!fixture.path.join(MANIFEST).exists());
}

#[test]
fn seed_admission_never_treats_a_failed_directory_listing_as_empty() {
    let fixture = Fixture::new();
    let repository = fixture.repository();
    let mut called = false;
    let failure = repository
        .require_unseeded_with(&mut |_| {
            called = true;
            Err(nix::errno::Errno::EIO)
        })
        .unwrap_err();
    assert!(called, "seed admission must inspect the held directory");
    assert_eq!(failure.case, PersistenceCase::UnsupportedLayout);
    assert!(failure.detail.contains("list pre-seed repository"));
    assert!(!fixture.path.join(MANIFEST).exists());
}
