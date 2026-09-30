//! conforms: types-one-floor-link
//! conforms: types-no-socket-no-runtime-no-io
//!
//! The manifest assertions of `weaver-types-Spec` section 5: no socket crate, no
//! runtime, no I/O in the dependency tree, checked as the traits Spec checks its
//! own, by a build-time assertion over the resolved external tree rather than by
//! H2. H2 itself reads the one `weaver-*` edge against the graph's single
//! `floor-link`, and the internal-dependency test below is its mechanical echo.

use std::process::Command;

fn resolved_tree(features: &[&str]) -> String {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args([
        "tree",
        "-p",
        "weaver-types",
        "--edges",
        "normal",
        "--prefix",
        "none",
    ]);
    for feature in features {
        cmd.args(["--features", feature]);
    }
    let out = cmd.output().expect("cargo tree runs");
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

/// No socket crate, no async runtime, no I/O, no logging: this crate defines
/// what crosses a boundary and crossing it is somebody else's crate. Checked
/// with the config feature both off and on, since a parser must not smuggle a
/// runtime.
#[test]
fn no_socket_no_runtime_no_io() {
    for features in [&[][..], &["config"][..]] {
        let names = crate_names(&resolved_tree(features));
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
                "{banned} in the resolved tree with features {features:?}"
            );
        }
    }
}

/// The crate's one internal dependency is `weaver-traits`, the graph's single
/// `floor-link`.
#[test]
fn one_floor_link() {
    let names = crate_names(&resolved_tree(&[]));
    let internal: Vec<_> = names
        .iter()
        .filter(|n| n.starts_with("weaver-") && *n != "weaver-types")
        .collect();
    assert_eq!(
        internal,
        vec!["weaver-traits"],
        "exactly one internal dependency"
    );
}
