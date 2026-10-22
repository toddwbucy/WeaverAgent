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
            "--locked",
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

/// Retirement of the Python worker is a manifest property, including optional
/// dependencies and targets that a default build would leave uncompiled.
/// Perturbation: add the optional pyo3 edge or a pyworker bin and this fails.
#[test]
fn the_only_worker_target_is_rust_and_no_python_dependency_is_carried() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata JSON");
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "weaver-harness")
        .expect("the harness package");
    let bins: Vec<_> = package["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["kind"].as_array().unwrap().iter().any(|k| k == "bin"))
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(bins, ["worker"]);
    assert!(package["features"].get("pyworker").is_none());
    for dependency in package["dependencies"].as_array().unwrap() {
        assert!(
            !dependency["name"].as_str().unwrap().starts_with("pyo3"),
            "the interpreter cannot return as an optional or renamed edge"
        );
    }
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

/// **The grant has one door, called at four sites, all in the function that
/// closes a call** (`weaver-harness-Spec` section 6, #29). The replacement
/// pair owner has one site after the terminal commit under Spec05. No other
/// crate file may construct a result from a public outcome.
///
/// Perturbation: add a constructor anywhere else and this fails.
#[test]
fn the_grant_is_constructed_only_at_the_legacy_and_committed_pair_doors() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let engine = std::fs::read_to_string(root.join("engine.rs")).unwrap();
    let start = engine.find("    fn execute_call(").expect("execute_call");
    let end = start + engine[start..].find("\n    }\n").expect("its end");
    let calls = |text: &str| text.matches("ToolResult::granted(").count();
    assert_eq!(calls(&engine[start..end]), 4);
    let pairs = std::fs::read_to_string(root.join("flow/pairs.rs")).unwrap();
    let start = pairs.find("    pub(super) fn install_terminal(").unwrap();
    let end = start + pairs[start..].find("\n    }\n").unwrap();
    assert_eq!(calls(&pairs[start..end]), 1);
    fn visit(path: &std::path::Path, sites: &mut Vec<(std::path::PathBuf, usize)>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, sites);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                let count = source.matches("ToolResult::granted(").count();
                if count != 0 {
                    sites.push((path, count));
                }
            }
        }
    }
    let mut sites = Vec::new();
    visit(&root, &mut sites);
    sites.sort();
    assert_eq!(
        sites,
        vec![(root.join("engine.rs"), 4), (root.join("flow/pairs.rs"), 1)]
    );
}

/// A committed pair constructs the harness notice. Records, tests and replay
/// cannot acquire a second constructor site. Removing its receipt checks is
/// separately perturbed through the real pair driver.
#[test]
fn non_dispatch_notice_has_one_pair_coordinator_construction_site() {
    fn visit(path: &std::path::Path, sites: &mut Vec<(std::path::PathBuf, usize)>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, sites);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                let count = source.matches("ToolResult::not_dispatched(").count();
                if count != 0 {
                    sites.push((path, count));
                }
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sites = Vec::new();
    visit(&root, &mut sites);
    assert_eq!(sites, vec![(root.join("flow/pairs.rs"), 1)]);
}
