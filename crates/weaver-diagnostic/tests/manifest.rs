//! conforms: diagnostic-no-trace-dependency
//! conforms: diagnostic-no-runtime-no-socket-crate
//!
//! The manifest assertions of `weaver-diagnostic-Spec` section 1, read from
//! the resolved tree the way the floor Specs share: `weaver-trace` absent
//! because the two records share a form and not a type, and no async
//! runtime and no socket crate because this crate crosses no process line
//! and holds a handle rather than a name.

use std::process::Command;

fn resolved_tree() -> String {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "-p", "weaver-diagnostic", "--prefix", "none"])
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

/// The counterpart writer is not a dependency, per section 1: linking it
/// would buy nothing this crate needs and would cost the counterpart
/// relation the charter rests on.
#[test]
fn no_trace_dependency() {
    let names = crate_names(&resolved_tree());
    assert!(
        !names.iter().any(|n| n == "weaver-trace"),
        "weaver-trace resolved into the tree"
    );
    assert!(
        names.iter().any(|n| n == "weaver-traits"),
        "weaver-traits is the one internal dependency and is absent"
    );
}

/// No async runtime and no socket crate in the resolved tree, per the
/// build-time assertion the floor Specs share.
#[test]
fn no_runtime_no_socket_crate() {
    let names = crate_names(&resolved_tree());
    for forbidden in ["tokio", "async-std", "smol", "mio", "socket2", "nix"] {
        assert!(
            !names.iter().any(|n| n == forbidden),
            "{forbidden} resolved into the tree"
        );
    }
}
