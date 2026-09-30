//! conforms: trace-no-internal-dependency
//! conforms: trace-no-async-runtime-no-socket-crate
//! conforms: trace-serde-json-raw-value-feature
//!
//! The manifest assertions of `weaver-trace-Spec` sections 1 and 10: the
//! build-time assertion over the resolved external tree the floor Specs
//! share, and the `raw_value` feature read the other way, a feature the
//! manifest must carry rather than one it must not.

use std::process::Command;

fn resolved_tree(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "-p", "weaver-trace", "--prefix", "none"])
        .args(args)
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

/// No `weaver-*` dependency at all: this crate depends on nothing internal,
/// declaring no `floor-link` and one `seam` tagged `link`, which gate H2 reads
/// against the graph.
#[test]
fn no_internal_dependency() {
    let names = crate_names(&resolved_tree(&["--edges", "normal"]));
    let internal: Vec<_> = names
        .iter()
        .filter(|n| n.starts_with("weaver-") && *n != "weaver-trace")
        .collect();
    assert!(
        internal.is_empty(),
        "internal dependencies present: {internal:?}"
    );
}

/// No async runtime and no socket crate in the resolved tree.
#[test]
fn no_async_runtime_no_socket_crate() {
    let names = crate_names(&resolved_tree(&["--edges", "normal"]));
    for banned in [
        "tokio",
        "async-std",
        "smol",
        "mio",
        "futures",
        "nix",
        "socket2",
        "log",
        "tracing",
    ] {
        assert!(
            !names.iter().any(|n| n == banned),
            "{banned} in the resolved tree"
        );
    }
}

/// The `raw_value` feature is carried: the splice election is a manifest fact,
/// named rather than discovered at build time.
#[test]
fn raw_value_feature_is_carried() {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "weaver-trace",
            "--edges",
            "features",
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo tree runs");
    let tree = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        tree.lines()
            .any(|l| l.contains("serde_json") && l.contains("raw_value")),
        "the raw_value feature edge is present in the feature tree"
    );
}
