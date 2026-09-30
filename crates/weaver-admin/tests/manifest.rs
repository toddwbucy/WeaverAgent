//! conforms: admin-one-floor-link-types-config
//! conforms: admin-no-direct-traits-line
//! conforms: admin-no-runtime-no-bus-no-logging
//! conforms: admin-no-library-surface
//!
//! The manifest assertions of `weaver-admin-Spec` sections 1 and 10. These
//! reach the crate from outside without needing its modules, which is what
//! lets them live in an integration test while every other test in this crate
//! sits inside it: the no-library rule means nothing else can.

use std::process::Command;

fn resolved_tree() -> String {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "weaver-admin",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--locked",
            "--offline",
        ])
        .output()
        .expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8")
}

fn crate_names(tree: &str) -> Vec<String> {
    tree.lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(|s| s.to_string())
        .collect()
}

/// The internal dependency is exactly `weaver-types`, and `weaver-traits`
/// arrives transitively rather than by a line of this crate's own - the
/// charter's declared non-link as a checkable absence.
#[test]
fn one_floor_link_and_no_direct_traits_line() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("manifest");
    // **The section is bounded at the next header.** Splitting on
    // `[dependencies]` alone runs past it into `[features]` and
    // `[dev-dependencies]`, so the assertions below could pass while the
    // property they name is false - a test that cannot fail.
    let after = manifest
        .split("\n[dependencies]\n")
        .nth(1)
        .expect("a dependency section");
    let deps = after.split("\n[").next().expect("a bounded section");
    assert!(deps.contains("weaver-types"), "the one floor link is named");
    assert!(
        !deps.contains("weaver-traits"),
        "weaver-traits is not a direct line: it arrives through weaver-types"
    );
    assert!(
        deps.contains("features = [\"config\"]"),
        "taken with the config feature, admin being the crate that parses the file"
    );
    // ...and the tree confirms it arrives all the same.
    let names = crate_names(&resolved_tree());
    assert!(
        names.iter().any(|n| n == "weaver-traits"),
        "transitively present"
    );
    assert!(names.iter().any(|n| n == "toml"), "the parser is linked");
}

/// No async runtime, no bus crate, no logging crate: the surface's traffic is
/// operator-paced, the init system is reached by its command-line interface,
/// and the operations log is this crate's own writer.
#[test]
fn no_runtime_no_bus_no_logging() {
    let names = crate_names(&resolved_tree());
    for banned in [
        "tokio",
        "async-std",
        "smol",
        "futures",
        "async-trait",
        "dbus",
        "zbus",
        "systemd",
        "tracing",
        "tracing-subscriber",
        "log",
        "env_logger",
    ] {
        assert!(
            !names.iter().any(|n| n == banned),
            "{banned} in the resolved tree"
        );
    }
}

/// One binary with integration tests and no library surface, per the Spec's
/// crate shape. Cargo's inventory covers every explicit and implicit route,
/// including a library outside src/lib.rs and a spaced TOML target header.
/// Perturb with an extra binary, lib, build script, example or bench. A
/// manifest comment spelling [[bin]] changes no target and must pass.
#[test]
fn one_binary_and_no_library_surface() {
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        ])
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("cargo JSON");
    let package = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "weaver-admin")
        .expect("admin package");
    let targets = package["targets"].as_array().expect("target inventory");
    let mut bins = 0;
    for target in targets {
        match (
            target["name"].as_str(),
            target["kind"].as_array().unwrap().as_slice(),
        ) {
            (Some("weaver-admin"), [kind]) if kind == "bin" => bins += 1,
            (_, [kind]) if kind == "test" => {}
            _ => panic!("unexpected admin target: {target}"),
        }
    }
    assert_eq!(bins, 1, "exactly one admin binary: {targets:?}");
}
