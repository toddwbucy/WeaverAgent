//! conforms: spu-fixture-surface-pinned
//!
//! The fixture-surface assertion of `weaver-spu-Spec` section 1.2, checked the
//! way this workspace's other manifest tests check theirs: by reading the build
//! rather than by trusting a comment.
//!
//! **This crate had no manifest test until #288, and it is the crate with the
//! feature gates.** Every other crate in the workspace carries one. The absence
//! is why the Spec's table could have been written and then drifted with nobody
//! finding out.
//!
//! **What it pins and what it deliberately does not.** It pins that every
//! artifact variable a test target reads appears in the table, and that every
//! variable the table names is read by a target. It does not pin the vocabulary
//! roster: `markers.rs` asks each registry entry for its own artifact under
//! `WEAVER_VOCAB_<FAMILY>`, so the set grows with the registry and a fixed list
//! would be stale on the next family. The shape is checked instead.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// The table of `weaver-spu-Spec` section 1.2, per target: the feature gate the
/// target compiles behind, and the fixtures it reads.
///
/// **Per target rather than a crate-wide union.** An earlier form compared only
/// the union of names, which left the document claiming a mapping the test did
/// not check: a fixture could move from one target to another, or a gate could
/// change, and the table would go on saying otherwise. Review found that gap and
/// this closes it.
///
/// **Transcribed from the Spec rather than derived from the tests.** A table
/// generated from the sources would agree with them whatever the document said,
/// which is the drift this exists to catch.
///
/// `markers.rs` carries [`VOCAB_PREFIX`] in place of a roster, because its
/// fixtures are per family and grow with the registry. That row is checked by
/// shape.
const TABLE: &[(&str, &[&str], &[&str])] = &[
    ("entry.rs", &[], &[]),
    // The pin itself. It is excluded from the scan, so it reads no fixture as
    // far as this table is concerned.
    ("manifest.rs", &[], &[]),
    // Reads none. It names `WEAVER_TEST_GGUF` in a doc comment discussing the
    // fixture convention and never asks the environment for it, which the
    // crate-wide union could not distinguish and the per-target check did on
    // its first run.
    ("seam.rs", &[], &[]),
    ("markers.rs", &["gguf"], &[VOCAB_PREFIX]),
    (
        "selection.rs",
        &["gguf"],
        &[
            "WEAVER_ARTIFACT_GEMMA4",
            "WEAVER_ARTIFACT_MISTRAL_SMALL",
            "WEAVER_ARTIFACT_PHI4",
            "WEAVER_ARTIFACT_PHI4_MINI",
            "WEAVER_ARTIFACT_SMOLLM2",
        ],
    ),
    (
        "native_loaded.rs",
        &["cuda"],
        &[
            "WEAVER_ARTIFACT_QWEN25_32B",
            "WEAVER_ARTIFACT_QWEN25_SAFETENSORS",
            "WEAVER_MEASURE_PACE",
        ],
    ),
    // The column watch names what its artifact owes rather than deriving
    // it, so a target pointed at another family carries that family's
    // shape with its artifact.
    (
        "loaded.rs",
        &["cuda", "gguf"],
        &[
            "WEAVER_ARTIFACT_COLUMN_LAYERS",
            "WEAVER_ARTIFACT_COLUMN_WIDTH",
            "WEAVER_ARTIFACT_QWEN25_SAFETENSORS",
            "WEAVER_TEST_GGUF",
        ],
    ),
    (
        "two_card.rs",
        &["cuda", "gguf"],
        &["WEAVER_ARTIFACT_SPLIT", "WEAVER_ARTIFACT_TWO_CARD"],
    ),
    (
        "readout_neutral.rs",
        &["cuda", "gguf"],
        &["WEAVER_ARTIFACT_READOUT", "WEAVER_ARTIFACT_READOUT_LAYERS"],
    ),
];

/// The prefix `markers.rs` builds its per-family names from.
const VOCAB_PREFIX: &str = "WEAVER_VOCAB_";

#[path = "manifest/source_scan.rs"]
mod source_scan;

/// Read the declared module tree even when the target is feature-disabled.
/// Child fixture literals belong to the target; a child's inner cfg does not.
fn read_target(path: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    // The pin and its synthetic sources name the table's fixtures themselves.
    if path.file_name().is_some_and(|name| name == "manifest.rs") {
        return (BTreeSet::new(), BTreeSet::new());
    }
    let inventory = source_scan::read(path);
    (inventory.fixtures, inventory.target_gate)
}

/// Every test source on disk, by file name.
fn targets_on_disk() -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    fs::read_dir(&dir)
        .expect("the tests directory reads")
        .filter_map(|e| {
            let path = e.expect("the entry reads").path();
            if path.extension().is_none_or(|x| x != "rs") {
                return None;
            }
            Some(path.file_name()?.to_string_lossy().into_owned())
        })
        .collect()
}

/// **Every target is in the Spec's table and every row names a target.**
///
/// Perturbation: add a test file without a row and this fails naming it, which
/// is what keeps a new target from arriving undocumented.
#[test]
fn every_target_appears_in_the_documented_table() {
    let on_disk = targets_on_disk();
    let rows: Vec<String> = TABLE.iter().map(|(name, _, _)| name.to_string()).collect();
    let documented: BTreeSet<String> = rows.iter().cloned().collect();
    // **One row per target, checked before the set swallows the evidence.**
    // The comparisons below are over sets, and a set collapses a repeated row
    // rather than reporting it. A duplicate naming the same gate and fixtures
    // would pass every assertion here silently, and one naming different ones
    // would fail further down against the source file, blaming the target for
    // a defect in the table.
    assert_eq!(
        rows.len(),
        documented.len(),
        "weaver-spu-Spec section 1.2 carries {} rows naming {} targets, so a \
         target is listed twice",
        rows.len(),
        documented.len()
    );
    let undocumented: Vec<&String> = on_disk.difference(&documented).collect();
    assert!(
        undocumented.is_empty(),
        "test targets absent from weaver-spu-Spec section 1.2: {undocumented:?}"
    );
    let missing: Vec<&String> = documented.difference(&on_disk).collect();
    assert!(
        missing.is_empty(),
        "section 1.2 names targets that do not exist: {missing:?}"
    );
}

/// **Each target's gate is the one the table gives it.**
///
/// Perturbation: change any file-level `cfg` and this fails naming the target
/// and both sets.
#[test]
fn each_target_compiles_behind_its_documented_gate() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for (name, gate, _) in TABLE {
        let (_, found) = read_target(&dir.join(name));
        let documented: BTreeSet<String> = gate.iter().map(|g| g.to_string()).collect();
        assert_eq!(
            found, documented,
            "{name} compiles behind {found:?} and section 1.2 says {documented:?}"
        );
    }
}

/// **Each target reads the fixtures the table gives it, and no others.**
///
/// This is the claim the document makes and the one an earlier form left
/// unchecked: comparing only the crate-wide union let a fixture move between
/// targets with the table going on saying otherwise.
///
/// `markers.rs` is checked by shape, its row carrying the prefix rather than a
/// roster, because its fixtures are per family and grow with the registry.
///
/// Perturbation: move a fixture from one target to another and this fails
/// naming both, where the union comparison passed.
#[test]
fn each_target_reads_its_documented_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for (name, _, fixtures) in TABLE {
        let (found, _) = read_target(&dir.join(name));
        if *fixtures == [VOCAB_PREFIX] {
            assert!(
                !found.is_empty(),
                "{name} is documented as a per-family set and reads nothing"
            );
            for fixture in &found {
                let named = fixture.len() > VOCAB_PREFIX.len();
                assert!(
                    fixture.starts_with(VOCAB_PREFIX) && named,
                    "{name} reads {fixture}, which is not a per-family name under \
                     {VOCAB_PREFIX}"
                );
            }
            continue;
        }
        let documented: BTreeSet<String> = fixtures.iter().map(|f| f.to_string()).collect();
        assert_eq!(
            found, documented,
            "{name} reads {found:?} and section 1.2 says {documented:?}"
        );
    }
}

/// The merged STE100 amendment adds one exact executable, preserving both older
/// duty names and the library target used by the existing doctest instruments.
#[test]
fn ste100_binary_set() {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let mut bins = Vec::new();
    for section in manifest.split("[[bin]]").skip(1) {
        let mut name = None;
        let mut path = None;
        let mut required = None;
        for line in section.lines().map(str::trim) {
            if line.starts_with('[') {
                break;
            }
            if line.starts_with('#') {
                continue;
            }
            if let Some(value) = line.strip_prefix("name = ") {
                name = Some(value.to_owned());
            }
            if let Some(value) = line.strip_prefix("path = ") {
                path = Some(value.to_owned());
            }
            if let Some(value) = line.strip_prefix("required-features = ") {
                required = Some(value.to_owned());
            }
        }
        bins.push((name.unwrap(), path.unwrap(), required));
    }
    bins.sort();
    assert_eq!(
        bins,
        vec![
            ("\"weaver-spu\"".into(), "\"src/main.rs\"".into(), None),
            (
                "\"weaver-spu-classify\"".into(),
                "\"src/bin/classify.rs\"".into(),
                Some("[\"cuda\"]".into())
            ),
            (
                "\"weaver-spu-ste100\"".into(),
                "\"src/bin/ste100.rs\"".into(),
                None
            ),
        ]
    );
    let library = manifest
        .split("[lib]")
        .nth(1)
        .unwrap()
        .split("[[bin]]")
        .next()
        .unwrap();
    assert!(
        library
            .lines()
            .any(|line| line.trim() == "name = \"weaver_spu\"")
    );
    assert!(
        library
            .lines()
            .any(|line| line.trim() == "path = \"src/lib.rs\"")
    );
}
