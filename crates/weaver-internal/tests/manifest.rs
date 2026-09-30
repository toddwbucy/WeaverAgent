//! conforms: internal-no-dependencies
//! conforms: internal-one-library-target
//!
//! The manifest assertions of `weaver-internal-Spec` section 1: no normal
//! dependency is declared, the manifest form of the charter's pure bar, with
//! build and dev dependencies admitted per the operator's ruling on #577, and
//! the targets are exactly one library and this manifest test.

use std::io::Write;
use std::process::{Command, Stdio};

/// **No normal dependency ships.** Read from cargo's own declared dependency
/// list for this package, where the normal kind is `null`, so a normal
/// dependency arriving by any route, target-qualified, behind a feature or under
/// a rename, is what the instrument sees. Build and dev declarations are
/// admitted, per the operator's ruling of 2026-09-23 on #577: nothing is
/// compiled into the library unless operations require it. Python's standard
/// JSON parser keeps this crate's own normal edges empty, and it refuses by
/// `sys.exit` rather than `assert`, which optimization strips.
/// Perturbations: a normal, a target-qualified, an optional and a renamed
/// normal dependency each fail; a build and a dev dependency each pass.
#[test]
fn no_normal_dependency_ships() {
    let out = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
        ])
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut check = Command::new("python3")
        .args([
            "-c",
            r#"
import json
import sys

packages = [p for p in json.load(sys.stdin)["packages"]
            if p["name"] == "weaver-internal"]
if len(packages) != 1:
    sys.exit("metadata must name exactly one weaver-internal package")
normal = [(d["name"], d.get("rename"), d.get("target"), d.get("optional"))
          for d in packages[0]["dependencies"] if d["kind"] is None]
if normal:
    sys.exit(f"a pure member ships no normal dependency; got {normal!r}")
"#,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 (the deploy script's prerequisite) runs");
    check
        .stdin
        .take()
        .expect("parser stdin")
        .write_all(&out.stdout)
        .expect("metadata reaches the JSON parser");
    let checked = check
        .wait_with_output()
        .expect("dependency check completes");
    assert!(
        checked.status.success(),
        "cargo's declared dependencies include a normal one: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

/// **One library and this instrument are the complete target set.** Cargo's
/// metadata sees every implicit and explicit target, including build scripts.
/// Python's standard JSON parser reads it without adding a Rust dependency,
/// and Python3 is already required by the repository's deploy script.
/// Perturbations: add build.rs or src/bin/x.rs; either adds a forbidden target.
#[test]
fn the_one_target_is_a_library() {
    let out = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
        ])
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut check = Command::new("python3")
        .args([
            "-c",
            r#"
import json
import sys

packages = [p for p in json.load(sys.stdin)["packages"]
            if p["name"] == "weaver-internal"]
if len(packages) != 1:
    sys.exit("metadata must name exactly one weaver-internal package")
targets = sorted((t["name"], t["kind"], t["crate_types"])
                 for t in packages[0]["targets"])
expected = [("manifest", ["test"], ["bin"]),
            ("weaver_internal", ["lib"], ["lib"])]
if targets != expected:
    sys.exit(f"expected library plus manifest test only; got {targets!r}")
"#,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 (the deploy script's prerequisite) runs");
    check
        .stdin
        .take()
        .expect("parser stdin")
        .write_all(&out.stdout)
        .expect("metadata reaches the JSON parser");
    let checked = check.wait_with_output().expect("target check completes");
    assert!(
        checked.status.success(),
        "cargo's target set violates the one-library claim: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
}
