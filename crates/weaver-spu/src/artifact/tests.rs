/// A directory of one split's shards resolves as one artifact, to its
/// first shard, and the pin collects the set. A directory mixing stems
/// stays the ambiguity it always was.
#[test]
fn a_sharded_directory_resolves_to_its_first_shard() {
    let dir = std::env::temp_dir().join(format!("weaver-sharded-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    for index in 1..=3 {
        std::fs::write(
            dir.join(format!("model-{index:05}-of-00003.safetensors")),
            b"x",
        )
        .expect("a shard writes");
    }
    let resolved = super::container_within(&dir).expect("the set resolves");
    assert_eq!(
        resolved,
        dir.join("model-00001-of-00003.safetensors"),
        "the first shard is the container"
    );
    let pinned = super::pin(&resolved).expect("the set pins");
    assert_eq!(pinned.paths().len(), 3, "every shard is pinned");

    // A stranger in the directory restores the refusal.
    std::fs::write(dir.join("other.safetensors"), b"x").expect("writes");
    assert!(
        matches!(
            super::container_within(&dir),
            Err(LifecycleRefusal::ArtifactUnresolvable)
        ),
        "a mixed directory is ambiguous"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A split with a missing later shard refuses as unresolvable, which is
/// the pin doc's own promise: a partial set is an absent artifact.
#[test]
fn a_split_missing_a_shard_is_unresolvable() {
    let dir = std::env::temp_dir().join(format!("weaver-split-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let first = dir.join("model-00001-of-00002.gguf");
    std::fs::write(&first, b"GGUF").expect("the first shard writes");
    // The sibling is deliberately absent.
    let outcome = super::pin(&first);
    assert!(
        matches!(outcome, Err(LifecycleRefusal::ArtifactUnresolvable)),
        "a missing sibling is the artifact failing to resolve, got {:?}",
        outcome.as_ref().err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The split pattern is exact: five digits, `-of-`, five digits, the
/// `.gguf` suffix. A near miss is a single-file artifact rather than a
/// malformed split, because the pattern is llama.cpp's convention and a
/// name outside it is just a name.
#[test]
fn the_split_pattern_is_exact() {
    use std::path::Path;
    let hit = super::split_pattern(Path::new("/m/model-00001-of-00002.gguf"));
    let (stem, count, suffix) = hit.expect("the pattern parses");
    assert_eq!(stem, Path::new("/m/model"));
    assert_eq!(count, 2);
    assert_eq!(suffix, ".gguf");
    assert_eq!(
        super::shard_name(&stem, 2, count, suffix),
        Path::new("/m/model-00002-of-00002.gguf")
    );

    // Naming a later shard reaches the same set.
    let (stem2, _, _) = super::split_pattern(Path::new("/m/model-00002-of-00002.gguf"))
        .expect("a later shard parses");
    assert_eq!(stem2, stem);

    // The second container spells the same pattern, per the exporter's
    // own convention.
    let (_, count_st, suffix_st) =
        super::split_pattern(Path::new("/m/model-00001-of-00017.safetensors"))
            .expect("a safetensors shard parses");
    assert_eq!(count_st, 17);
    assert_eq!(suffix_st, ".safetensors");

    for miss in [
        "/m/model.gguf",
        "/m/model-1-of-2.gguf",
        "/m/model-00001-of-00000.gguf",
        "/m/model-00001-of-00002.bin",
        "/m/00001-of-00002.gguf",
        // The integer parse alone would take a signed count.
        "/m/model-00001-of-+1234.gguf",
        // An index outside the set it claims membership of.
        "/m/model-00000-of-00002.gguf",
        "/m/model-00003-of-00002.gguf",
    ] {
        assert!(
            super::split_pattern(Path::new(miss)).is_none(),
            "{miss} is not a split"
        );
    }
}

use super::*;
use std::io::Write;

/// A minimal GGUF whose one metadata value is an array nested past the
/// depth bound: each level is `element_type=9` and `count=1`, so the file
/// is small while the recursion it asks for is not.
fn nested_gguf(depth: u32) -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(b"GGUF");
    file.extend_from_slice(&3u32.to_le_bytes()); // version
    file.extend_from_slice(&0u64.to_le_bytes()); // tensor count
    file.extend_from_slice(&1u64.to_le_bytes()); // kv count
    let key = b"a";
    file.extend_from_slice(&(key.len() as u64).to_le_bytes());
    file.extend_from_slice(key);
    file.extend_from_slice(&9u32.to_le_bytes()); // the value is an array
    for _ in 0..depth {
        file.extend_from_slice(&9u32.to_le_bytes()); // of arrays
        file.extend_from_slice(&1u64.to_le_bytes()); // one element each
    }
    file.extend_from_slice(&0u32.to_le_bytes()); // ending in u8
    file.extend_from_slice(&0u64.to_le_bytes()); // of zero elements
    file
}

/// **A crafted GGUF refuses instead of aborting.** Without the depth bound
/// a file of repeated array-of-array headers drives the recursion to a
/// stack overflow, an abort reachable from an untrusted artifact at step
/// two of the admit. Every other malformation on this path is a clean
/// refusal, and this one must be too.
///
/// Perturbation: remove the `depth >= MAX_ARRAY_DEPTH` branch from the
/// array arm and this test dies by stack overflow rather than failing an
/// assertion, which is the abort the bound exists to prevent. Watched
/// under exactly that removal.
#[test]
fn a_deeply_nested_gguf_refuses_rather_than_aborting() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-spu-artifact-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let path = dir.join("nested.gguf");
    let mut handle = File::create(&path).expect("a fixture file");
    handle
        .write_all(&nested_gguf(200_000))
        .expect("the fixture is written");
    drop(handle);

    assert_eq!(
        read_header(&mut pin(&path).expect("the fixture pins")),
        Err(LifecycleRefusal::ArtifactUnreadable),
        "a nesting no real artifact carries is a refusal, never an abort"
    );

    // A shallow nesting inside the bound still reads to the family error,
    // which is what shows the bound refuses depth rather than arrays.
    let shallow = dir.join("shallow.gguf");
    let mut handle = File::create(&shallow).expect("a fixture file");
    handle
        .write_all(&nested_gguf(3))
        .expect("the fixture is written");
    drop(handle);
    assert_eq!(
        read_header(&mut pin(&shallow).expect("the fixture pins")),
        Err(LifecycleRefusal::ArtifactUnreadable),
        "no general.architecture key, so the refusal is the absent family"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// **A directory holding several containers refuses on ambiguity** rather
/// than silently resolving to the alphabetically first.
///
/// Perturbation: restore the sort-and-take-first and this test fails,
/// because the header read succeeds against whichever file sorts first.
/// Watched under exactly that restoration.
#[test]
fn an_ambiguous_directory_refuses() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-spu-artifact-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    for name in ["a.gguf", "b.gguf"] {
        let mut handle = File::create(dir.join(name)).expect("a fixture file");
        handle
            .write_all(&super::tests::nested_gguf(0))
            .expect("written");
    }
    assert_eq!(
        resolve(&ArtifactRef(dir.to_string_lossy().into_owned())),
        Err(LifecycleRefusal::ArtifactUnresolvable),
        "two containers is an ambiguity, not a choice"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// **A directory the identity cannot traverse is a present artifact it
/// cannot reach**, per Spec section 3, and refuses unreadable rather than
/// unresolvable, so the operator's account says denied and not absent.
/// Skips as root, a uid no directory denies, rather than passing on
/// nothing.
///
/// Perturbation: map every metadata error to unresolvable and this test
/// fails. Watched under exactly that removal.
#[test]
fn a_denied_directory_is_unreadable_rather_than_unresolvable() {
    use std::os::unix::fs::PermissionsExt as _;
    // The suite skips loudly rather than attesting to nothing: a bare
    // return reports as a pass, and this is the one test that watches
    // the fault the ruling exists to catch.
    if nix::unistd::geteuid().is_root() {
        eprintln!(
            "SKIP a_denied_directory_is_unreadable_rather_than_unresolvable: \
             no directory denies root, rerun as an unprivileged uid"
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "weaver-spu-artifact-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let inside = dir.join("model.gguf");
    std::fs::write(&inside, b"GGUF").expect("a file inside");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("deny");
    let outcome = resolve(&ArtifactRef(inside.to_string_lossy().into_owned()));
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    assert_eq!(
        outcome,
        Err(LifecycleRefusal::ArtifactUnreadable),
        "a denied lookup is present and unreachable, not absent"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// **A named pipe does not resolve.** `File::open` on a FIFO blocks until a
/// writer connects, so an artifact reference naming one would hold the
/// admit open indefinitely where every other malformation refuses.
///
/// Perturbation: drop the `is_file` guard from `resolve` and this test
/// fails. Watched under exactly that removal.
#[test]
fn a_named_pipe_does_not_resolve() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-spu-artifact-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let fifo = dir.join("pipe.gguf");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("a fifo");

    assert_eq!(
        resolve(&ArtifactRef(fifo.to_string_lossy().into_owned())),
        Err(LifecycleRefusal::ArtifactUnresolvable),
        "a pipe named like a container is not an artifact"
    );
    // And the same inside a directory, where it would be selected by name.
    assert_eq!(
        resolve(&ArtifactRef(dir.to_string_lossy().into_owned())),
        Err(LifecycleRefusal::ArtifactUnresolvable),
        "a directory holding only a pipe holds no container"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// **The pin refuses what is not a regular file, judged on the descriptor
/// rather than on the name.** A pipe opens without blocking because of
/// `O_NONBLOCK` and is refused by the kind check, where a blocking open
/// would hold the admit open before any check could run.
///
/// Perturbation: drop the `is_file` branch from `pin` and this test fails.
/// Watched under exactly that removal.
#[test]
fn the_pin_refuses_a_pipe_on_the_descriptor() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-spu-artifact-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let fifo = dir.join("pipe.gguf");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("a fifo");
    assert!(
        matches!(pin(&fifo), Err(LifecycleRefusal::ArtifactUnresolvable)),
        "the descriptor says pipe, so the pin refuses"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// **A directory artifact resolves to the file inside it, at step one.**
/// An earlier cut resolved the container inside the header read, so the
/// admission carried the directory while the judgments blessed the file,
/// and the loader was handed a path it cannot open, refusing as a device
/// condition three steps after the cause.
///
/// Perturbation: return the directory itself from `resolve` and this test
/// fails on the extension assertion. Watched under exactly that change.
#[test]
fn a_directory_artifact_resolves_to_its_container_file() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-spu-artifact-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let mut handle = File::create(dir.join("only.gguf")).expect("a fixture file");
    handle.write_all(&nested_gguf(0)).expect("written");
    drop(handle);

    let resolved = resolve(&ArtifactRef(dir.to_string_lossy().into_owned()))
        .expect("a one-container directory resolves");
    assert!(
        resolved.is_file(),
        "the admission carries the file, never the directory"
    );
    assert_eq!(resolved.extension().and_then(|e| e.to_str()), Some("gguf"));
    std::fs::remove_dir_all(&dir).ok();
}

/// A scratch directory artifact: a container and a sidecar, the shape a
/// downloaded safetensors directory has. Named per test so runs in
/// parallel never share one.
fn walked_fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("weaver-spu-walk-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    std::fs::write(dir.join("model.safetensors"), b"the weights").expect("written");
    std::fs::write(dir.join("config.json"), b"{}").expect("written");
    dir
}

/// The admit's own sequence for the hash: resolve, pin, then hash the
/// reference through the pin.
fn hash_of(dir: &Path) -> Result<crate::residency::WeightsHash, LifecycleRefusal> {
    let (resolved, kind) = resolve_with_kind(&ArtifactRef(dir.to_string_lossy().into_owned()))?;
    let mut pinned = pin(&resolved)?;
    weights_hash(dir, kind, &mut pinned)
}

/// **A dot-entry is outside the artifact**, per Spec section 3 and the
/// operator's ruling of 2026-10-01 on #32: a downloader's `.cache/` beside
/// the model, readable or not, moves neither the identity nor the
/// resolution, and a container named under a dot is never the one
/// resolved.
///
/// Perturbation: drop the walk's `filter_entry` and the hash moves.
#[test]
fn a_dot_entry_is_outside_the_identity() {
    let dir = walked_fixture("dot");
    let before = hash_of(&dir).expect("the fixture hashes");
    let cache = dir.join(".cache").join("huggingface").join("trees");
    std::fs::create_dir_all(&cache).expect("a cache dir");
    std::fs::write(cache.join("rev.json"), b"downloader bookkeeping").expect("written");
    std::fs::set_permissions(
        cache.join("rev.json"),
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .expect("chmod");
    std::fs::write(dir.join(".stray.safetensors"), b"not the model").expect("written");
    assert_eq!(hash_of(&dir), Ok(before), "the dot-entries moved nothing");
    std::fs::remove_dir_all(&dir).ok();
}

/// **A symbolic link is followed and hashed under its own name**, per the
/// same ruling: a directory whose sidecar is a link to identical bytes
/// elsewhere is the same identity as one holding the file, and a container
/// reached through a link is hashed through the pin like any other.
///
/// Perturbation: `follow_links(false)` and the linked sidecar drops out of
/// the walk, so the two identities differ.
#[test]
fn a_linked_member_is_hashed_under_the_links_name() {
    let plain = walked_fixture("plain");
    let linked = walked_fixture("linked");
    let elsewhere = walked_fixture("elsewhere");
    std::fs::remove_file(linked.join("config.json")).expect("removed");
    std::os::unix::fs::symlink(elsewhere.join("config.json"), linked.join("config.json"))
        .expect("a link");
    std::fs::remove_file(linked.join("model.safetensors")).expect("removed");
    std::os::unix::fs::symlink(
        elsewhere.join("model.safetensors"),
        linked.join("model.safetensors"),
    )
    .expect("a link");
    assert_eq!(
        hash_of(&linked).expect("a linked directory hashes"),
        hash_of(&plain).expect("the plain directory hashes"),
        "the link's target bytes under the link's name"
    );
    for dir in [plain, linked, elsewhere] {
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// **A member the walk cannot read refuses the admission**, per the same
/// ruling, where it once admitted under the empty sentinel: an identity
/// that leaves out a member it could not read names other weights.
///
/// Perturbation: skip a member whose open fails, and the hash is a value.
#[test]
fn an_unreadable_member_refuses() {
    // The suite skips loudly rather than attesting to nothing: no mode
    // denies root.
    if nix::unistd::geteuid().is_root() {
        eprintln!("skipped: no file mode denies root, rerun as an unprivileged uid");
        return;
    }
    let dir = walked_fixture("unreadable");
    std::fs::set_permissions(
        dir.join("config.json"),
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .expect("chmod");
    assert_eq!(hash_of(&dir), Err(LifecycleRefusal::ArtifactUnreadable));
    // A link that leads nowhere is a member that cannot be read too.
    let dangling = walked_fixture("dangling");
    std::os::unix::fs::symlink(dangling.join("gone"), dangling.join("tokenizer.json"))
        .expect("a link");
    assert_eq!(
        hash_of(&dangling),
        Err(LifecycleRefusal::ArtifactUnreadable)
    );
    for dir in [dir, dangling] {
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// **The container is hashed through the pin, inside the walk**: a name
/// replaced after the pin leaves the identity of the bytes the load reads,
/// and a name removed after it refuses rather than leave them out.
///
/// Perturbation: open every walked file by name and the replaced case
/// hashes the new bytes.
#[test]
fn the_walk_reads_the_container_through_the_pin() {
    let dir = walked_fixture("pinned");
    let before = hash_of(&dir).expect("the fixture hashes");
    let resolved = resolve(&ArtifactRef(dir.to_string_lossy().into_owned())).expect("resolves");
    let mut pinned = pin(&resolved).expect("pins");
    let swapped = dir.join("swapped");
    std::fs::write(&swapped, b"other weights").expect("written");
    std::fs::rename(&swapped, dir.join("model.safetensors")).expect("the swap");
    assert_eq!(
        weights_hash(&dir, Reference::Directory, &mut pinned),
        Ok(before)
    );

    let mut pinned = pin(&resolved).expect("pins again");
    std::fs::remove_file(dir.join("model.safetensors")).expect("removed");
    assert_eq!(
        weights_hash(&dir, Reference::Directory, &mut pinned),
        Err(LifecycleRefusal::ArtifactUnreadable)
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// **A directory reference stays one through the hash**, per Spec section
/// 3: the kind is the resolution's, so a directory renamed away or
/// replaced by a file after the pin is a walk that fails, never a file
/// reference hashing the pinned container alone. Found by Codex on #44.
///
/// Perturbation: decide the kind at the hash with `reference.is_dir()`
/// and both cases admit.
#[test]
fn a_directory_gone_after_the_pin_refuses() {
    for replace_with_file in [false, true] {
        let dir = walked_fixture(if replace_with_file {
            "replaced"
        } else {
            "moved"
        });
        let (resolved, kind) =
            resolve_with_kind(&ArtifactRef(dir.to_string_lossy().into_owned())).expect("resolves");
        assert_eq!(kind, Reference::Directory);
        let mut pinned = pin(&resolved).expect("pins");
        let aside = dir.with_extension("aside");
        let _ = std::fs::remove_dir_all(&aside);
        std::fs::rename(&dir, &aside).expect("moved away");
        if replace_with_file {
            std::fs::write(&dir, b"not a directory").expect("a file in its place");
        }
        assert_eq!(
            weights_hash(&dir, kind, &mut pinned),
            Err(LifecycleRefusal::ArtifactUnreadable),
            "replaced with a file: {replace_with_file}"
        );
        let _ = std::fs::remove_file(&dir);
        std::fs::remove_dir_all(&aside).ok();
    }
}
