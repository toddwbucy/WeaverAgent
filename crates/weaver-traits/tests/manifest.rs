//! conforms: traits-no-format-crate
//! conforms: traits-no-async-runtime
//! conforms: traits-no-internal-dependency
//!
//! The manifest assertions of `weaver-traits-Spec` sections 1 and 7: a build-time
//! assertion over the resolved external tree, named there so it is not a claim
//! nothing runs. Gate H2 reads `weaver-*` edges against the graph; this test is
//! the instrument for the external half H2 does not check.

use std::process::Command;

fn resolved_tree() -> String {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "weaver-traits",
            "--edges",
            "normal",
            "--prefix",
            "none",
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

/// No format crate: the recorder renders, the floor defines, and the format is
/// decided where the rendering happens. Perturbation: adding `serde_json` to
/// `[dependencies]` (not dev-dependencies) fails this test.
#[test]
fn no_format_crate() {
    let names = crate_names(&resolved_tree());
    for banned in [
        "serde_json",
        "serde_yaml",
        "serde_yaml_ng",
        "toml",
        "rmp-serde",
        "bincode",
    ] {
        assert!(
            !names.iter().any(|n| n == banned),
            "format crate {banned} in the resolved tree"
        );
    }
}

/// No async runtime, no `async-trait`, no `futures`, per the boxing election of
/// Spec section 5. Perturbation: adding `futures` to the manifest fails this test.
#[test]
fn no_async_runtime() {
    let names = crate_names(&resolved_tree());
    for banned in [
        "tokio",
        "async-std",
        "smol",
        "futures",
        "futures-core",
        "async-trait",
        "mio",
    ] {
        assert!(
            !names.iter().any(|n| n == banned),
            "async crate {banned} in the resolved tree"
        );
    }
}

/// No internal dependency: this crate declares no `floor-link` and no `seam` in
/// the graph, so any `weaver-*` Cargo edge at all is a defect.
#[test]
fn no_internal_dependency() {
    let names = crate_names(&resolved_tree());
    let internal: Vec<_> = names
        .iter()
        .filter(|n| n.starts_with("weaver-") && *n != "weaver-traits")
        .collect();
    assert!(
        internal.is_empty(),
        "internal dependencies present: {internal:?}"
    );
}
