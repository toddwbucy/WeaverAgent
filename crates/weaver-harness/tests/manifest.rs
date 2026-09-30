//! conforms: harness-internal-dependency-set
//! conforms: harness-types-without-config
//! conforms: harness-no-runtime-no-logging-no-http
//! conforms: harness-os-surface-nix
//!
//! The manifest assertions of `weaver-harness-Spec` sections 1 and 8. Gate H2
//! reads the internal edges against the graph; these are the instrument for
//! the external half H2 does not check, plus the featureless take, which is a
//! `Cargo.toml` fact of its own rather than an edge.

use std::process::Command;

fn resolved_tree(edges: &str) -> String {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "weaver-harness",
            "--edges",
            edges,
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

/// The internal set is exactly the two floor links and the two member seams
/// tagged `link`, per `weaver-harness-Spec` section 1's four-crate count as
/// of the diagnostic member's build - no organ appears, because a crate this
/// one asks to do something is reached over a socket.
#[test]
fn internal_dependency_set_is_the_floor_and_the_trace_seam() {
    let names = crate_names(&resolved_tree("normal"));
    let mut internal: Vec<_> = names
        .iter()
        .filter(|n| n.starts_with("weaver-") && *n != "weaver-harness")
        .cloned()
        .collect();
    internal.sort();
    internal.dedup();
    assert_eq!(
        internal,
        vec![
            "weaver-diagnostic",
            "weaver-trace",
            "weaver-traits",
            "weaver-types"
        ],
        "the floor pair and the two member seams, and no organ"
    );
}

/// `weaver-types` is taken without its `config` feature: this crate reads no
/// field from the agent config's file, so it links no parser. The model
/// binding and gate instruction arrive over the coordination seam already
/// validated.
#[test]
fn types_is_taken_without_the_config_feature() {
    let names = crate_names(&resolved_tree("normal"));
    assert!(
        !names.iter().any(|n| n == "toml"),
        "no config parser reaches this crate through the floor link"
    );
}

/// No async runtime, no logging crate, and no HTTP client in the resolved
/// external tree. The old tree's harness carried tokio with every feature,
/// `async-trait`, `tracing` with two subscriber crates, and an outbound HTTP
/// client, and none of it crosses.
#[test]
fn no_runtime_no_logging_no_http() {
    let names = crate_names(&resolved_tree("normal"));
    for banned in [
        "tokio",
        "async-std",
        "smol",
        "futures",
        "async-trait",
        "tracing",
        "tracing-subscriber",
        "log",
        "env_logger",
        "reqwest",
        "hyper",
        "ureq",
        "curl",
    ] {
        assert!(
            !names.iter().any(|n| n == banned),
            "{banned} in the resolved tree"
        );
    }
}

/// The OS crate is `nix` and there is no `libc` or `rustix` line of this
/// crate's own: one crate covering the whole surface beats two crates covering
/// it between them.
#[test]
fn the_os_surface_is_one_crate() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("manifest");
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("a dependency section");
    assert!(deps.contains("nix"), "nix is the elected OS surface");
    for absent in ["libc =", "libc.workspace", "rustix"] {
        assert!(
            !deps.contains(absent),
            "{absent} must not be a dependency of this crate"
        );
    }
}
