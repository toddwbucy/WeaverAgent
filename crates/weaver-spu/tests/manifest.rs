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

/// The `WEAVER_*` names appearing as string literals in one test source, and
/// the features its file-level `cfg` gate names.
///
/// Read off disk rather than compiled in, because a `cfg`-gated target is not
/// compiled on a host build and its variables would vanish from a compiled
/// inventory exactly when the gate is what hid them. That is the failure this
/// whole issue is about, and it would be reproduced here.
///
/// **The quote is what makes a name rather than prose.** An earlier form
/// searched the bare text and read a doc comment's `WEAVER_VOCAB_<FAMILY>` as a
/// variable no test asks the environment for. Requiring the surrounding quote
/// leaves comments, documentation, and diagnostic prose out without a parser.
///
/// **Anchoring to `env::var` call sites was considered and rejected.** Two
/// access shapes exist here: `loaded.rs` and its neighbours call `env::var_os`
/// directly, while `markers.rs` and `selection.rs` carry the name as a field of
/// a fixture table and look it up elsewhere. A scan anchored to the call site
/// would miss eighteen of the twenty-four names, which is the under-reporting
/// this test exists to prevent. A real token parse would reach both, and it
/// would put a parser in the dependency set of a crate whose Spec section 1.1
/// argues every dependency it takes.
fn read_target(path: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let source = fs::read_to_string(path).expect("the source reads");
    let bytes = source.as_bytes();

    let mut fixtures = BTreeSet::new();
    // **This file is not a subject of its own scan.** It names fixtures in its
    // table and in its own perturbation notes, and a scanner that read them
    // would report the document's table as a finding against itself. Watched
    // doing exactly that before this guard existed.
    if path.file_name().is_none_or(|n| n != "manifest.rs") {
        let mut at = 0;
        // A name is `"WEAVER_..."`, quoted. Both access shapes in this crate put
        // it in a string literal: a direct `env::var_os` argument, and a fixture
        // table's field. Prose carries it in backticks or bare, and is left out
        // by the opening quote alone.
        while let Some(offset) = source[at..].find("\"WEAVER_") {
            let start = at + offset + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_uppercase()
                    || bytes[end].is_ascii_digit()
                    || bytes[end] == b'_')
            {
                end += 1;
            }
            // The literal must close where the name ends, or the match is a
            // longer string that merely begins with the prefix.
            if bytes.get(end) == Some(&b'"') {
                fixtures.insert(source[start..end].to_string());
            }
            at = end;
        }
    }

    // The file-level gate, as the set of features it names. **A set rather than
    // the text**, because the sources spell the same gate two ways: `loaded.rs`
    // writes `cuda, gguf` and `two_card.rs` writes `gguf, cuda`, and a string
    // comparison would call that a difference.
    let mut gate = BTreeSet::new();
    if let Some(line) = source.lines().find(|l| l.starts_with("#![cfg(")) {
        let mut at = 0;
        while let Some(offset) = line[at..].find("feature = \"") {
            let start = at + offset + "feature = \"".len();
            let end = start + line[start..].find('"').expect("the feature closes");
            gate.insert(line[start..end].to_string());
            at = end;
        }
    }
    (fixtures, gate)
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
