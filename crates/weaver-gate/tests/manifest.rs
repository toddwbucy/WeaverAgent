//! conforms: gate-one-binary
//! conforms: gate-lib-target-exists
//! conforms: gate-floor-link-types-without-config
//! conforms: gate-no-runtime-no-logging-no-yaml
//!
//! The manifest assertions of `weaver-gate-Spec` sections 1 and 6. Gate H2
//! reads the internal edge against the graph; these are the instrument for the
//! external half H2 does not check, plus the featureless take, which is a
//! `Cargo.toml` fact of its own rather than an edge.

use std::process::Command;

/// One place the cargo calls are made, so a flag change cannot land in one
/// copy and leave two tests asserting about different resolutions.
///
/// `--locked --offline` and the manifest path are appended here rather than at
/// each call site. **The flags keep this inner cargo from writing the lock as a
/// side effect of answering, and they gate nothing.** The outer `cargo test`
/// resolved and repaired the lock before this binary was spawned, so by the
/// time these calls run there is no drift left for them to see: measured
/// 2026-09-15 at 9caf02b, a dependency added to another crate's manifest leaves
/// every test in this file passing and `Cargo.lock` modified. An earlier form
/// of this comment said the lock file was the subject, which is the overclaim
/// issue #551 was filed against. `process/gates/lock.sh` is the instrument that
/// can fail and it runs before the suite, which is the only place it can.
///
/// The manifest path is passed because the working directory a test binary
/// runs in is not this crate's, and a call that resolved another workspace
/// would answer a question nobody asked.
fn cargo_output(args: &[&str]) -> String {
    let mut full: Vec<&str> = args.to_vec();
    full.extend_from_slice(&[
        "--locked",
        "--offline",
        "--manifest-path",
        concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
    ]);
    let out = Command::new(env!("CARGO"))
        .args(&full)
        .output()
        .unwrap_or_else(|error| panic!("cargo {} runs: {error}", args[0]));
    assert!(
        out.status.success(),
        "cargo {} failed: {}",
        args[0],
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8")
}

fn cargo_json(args: &[&str]) -> serde_json::Value {
    serde_json::from_str(&cargo_output(args)).expect("cargo output is json")
}

/// One helper for both depths, so a flag change cannot land in one copy and
/// leave the two tests asserting about different resolutions.
fn resolved_tree(edges: &str, depth: Option<&str>) -> String {
    let mut args = vec![
        "tree",
        "-p",
        "weaver-gate",
        "--edges",
        edges,
        "--prefix",
        "none",
    ];
    if let Some(depth) = depth {
        args.push("--depth");
        args.push(depth);
    }
    cargo_output(&args)
}

fn crate_names(tree: &str) -> Vec<String> {
    tree.lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(|s| s.to_string())
        .collect()
}

fn manifest() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("the manifest reads")
}

/// **The internal set is exactly one floor link, and no organ appears.** A
/// crate this one answers to is reached over a socket, so no organ is in the
/// resolved tree at any depth.
///
/// `weaver-traits` is in the tree and is not a defect: it arrives transitively
/// under `weaver-types`, which is the floor linking its own sibling. What the
/// charter's floor-link set forbids is a *direct* line, which the test below
/// reads from the manifest, because a transitive arrival is the floor's shape
/// rather than this crate's draw.
#[test]
fn no_organ_is_in_the_resolved_tree() {
    let names = crate_names(&resolved_tree("normal", None));
    let mut internal: Vec<_> = names
        .iter()
        .filter(|n| n.starts_with("weaver-") && *n != "weaver-gate")
        .cloned()
        .collect();
    internal.sort();
    internal.dedup();
    assert_eq!(
        internal,
        vec!["weaver-traits", "weaver-types"],
        "the floor, reached directly and transitively, and no organ"
    );
    // Read against the whole resolved tree rather than the filtered set above,
    // which the assertion has already fixed: a loop over `internal` could
    // never fail and would report a check nobody ran.
    for organ in [
        "weaver-harness",
        "weaver-spu",
        "weaver-admin",
        "weaver-trace",
    ] {
        assert!(
            !names.iter().any(|n| n == organ),
            "{organ} is a crate this one reaches over a socket, never a link"
        );
    }
}

/// **The internal dependency set is exactly one floor crate**, per Spec
/// section 1 and the tool boundary ruling of 2026-08-18: `weaver-types` for
/// the wire, alone. The traits line joined with the tool workflow's opening
/// act as the trait's executor and left with the ruling - the shell is this
/// crate's own verb, dispatched with no table, so the trait has no consumer
/// here and the line would be a dependency taken for nothing. The pin has
/// turned twice now and the story is the history's, not this comment's: a
/// second internal dependency fails here naming itself.
///
/// Perturbation: add any other `weaver-*` line to the dependency section and
/// this fails naming it. Watched under the traits line's own addition and
/// again under its removal.
#[test]
fn the_internal_dependency_set_is_the_one_floor_crate() {
    // Structural first: the resolved direct set, which catches a renamed or
    // table-form declaration a text scan reads straight past.
    let direct = crate_names(&resolved_tree("normal", Some("1")));
    let internal: Vec<&String> = direct
        .iter()
        .filter(|n| n.starts_with("weaver-") && *n != "weaver-gate")
        .collect();
    assert_eq!(
        internal.len(),
        1,
        "the internal set is weaver-types, exactly: found {internal:?}"
    );
    assert!(
        internal.iter().any(|n| *n == "weaver-types"),
        "weaver-types is not a direct dependency: found {internal:?}"
    );
}

/// **`weaver-types` is taken without its `config` feature.** This crate reads
/// no configuration file, the gate instruction arriving over the seam instead,
/// so no parser enters a process whose whole argument is that it holds little.
///
/// Perturbation: drop `default-features = false` from the dependency line and
/// this test fails. Watched under exactly that removal.
#[test]
fn the_floor_link_is_taken_without_config() {
    let manifest = manifest();
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("weaver-types"))
        .expect("the floor link is declared");
    assert!(
        line.contains("default-features = false"),
        "the config feature would put a parser in this process, got: {line}"
    );
}

/// **One binary and its library, with integration tests.** Cargo's inventory
/// sees explicit and implicit targets alike, per `weaver-gate-Spec` section 1.
/// Build scripts, examples, benches and extra binaries are not this shape.
/// The separate lib watch below still holds its doctest flag.
/// Perturb each forbidden target route; a comment spelling [[bin]] must pass.
///
/// **This does not hold "no other crate links it".** The no-organ test above
/// reads `cargo tree -p weaver-gate`, which is the forward relation, what this
/// crate links. A workspace member adding `weaver-gate` to its dependencies
/// leaves every test in this file passing. The reverse half is review's, per
/// Spec section 1, and wants a reverse walk or a workspace-wide manifest scan.
#[test]
fn the_manifest_declares_one_binary() {
    let meta = cargo_json(&["metadata", "--no-deps", "--format-version", "1"]);
    let package = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "weaver-gate")
        .expect("gate package");
    let targets = package["targets"].as_array().expect("target inventory");
    let mut bins = 0;
    let mut libs = 0;
    for target in targets {
        match (
            target["name"].as_str(),
            target["kind"].as_array().unwrap().as_slice(),
        ) {
            (Some("weaver-gate"), [kind]) if kind == "bin" => bins += 1,
            // Cargo spells an explicitly elected rlib as rlib, not lib.
            // Both are the library the binary links and the doctest watch holds.
            (Some("weaver_gate"), [kind]) if kind == "lib" || kind == "rlib" => libs += 1,
            (_, [kind]) if kind == "test" => {}
            _ => panic!("unexpected gate target: {target}"),
        }
    }
    assert_eq!(
        (bins, libs),
        (1, 1),
        "one gate binary and library: {targets:?}"
    );
}

/// **The lib target stands and collects the doctests.** It is the precondition
/// of an instrument this crate claims rather than a fact about the crate's
/// shape, and the reasoning is `weaver-gate-Spec` section 1's - not restated
/// here, per gate G5.
///
/// **Two assertions, because the property has two silent ways out.** Fold the
/// modules into the binary and there is no lib target. Keep the target and set
/// `doctest = false` and the target stands while `cargo test -p weaver-gate`
/// stops running the pins. Either way
/// `gate-bind-shapes-pinned-by-doctest` goes unenforced and every other gate
/// still passes.
///
/// **A third way out is held by the compiler and not by this test.** The pins
/// are doctests on `hook.rs`, and a `hook` that left the lib's module graph
/// would take them with it - but `relay.rs:34` has `use crate::hook::Admitted`,
/// so removing the declaration is `error[E0432]` and not a silent
/// unenforcement. Making the module private does not remove it: measured
/// 2026-09-15, `mod hook;` with a re-export builds and the two compile-fail
/// doctests are still collected. **An assertion on `pub mod hook;` would fail
/// on that change while the property held**, which is why this test does not
/// carry one.
///
/// **Two removals nothing here reaches**, per the Spec clause: a doctest
/// deleted from `hook.rs`, and the pins moved to a module no target compiles.
/// Both want an inventory of the collected doctests, and `cargo test --doc`
/// inside a `cargo test` waits on the build lock - a hang being the one failure
/// a watch cannot report.
///
/// Perturbations, each run alone on 2026-09-15 with the other five tests in
/// this file passing:
///
/// 1. Modules folded into `main.rs`, `src/lib.rs` deleted, `[lib]` dropped -
///    fails at the target, and `--doc` answers "no library targets found".
/// 2. `doctest = false` added to `[lib]` - fails at the doctest field, and
///    `cargo test -p weaver-gate --no-fail-fast` runs no `Doc-tests` section
///    where it had run two compile-fail pins. An explicit `cargo test --doc`
///    still collects them, the flag overriding the target setting, so the
///    removal is invisible to that one command and visible to the gate.
/// 3. `pub mod hook;` made private with a re-export - builds, collects, and is
///    not a removal of the property, which is the finding above.
///
/// **Deleting `src/lib.rs` alone is not among them**: the bin's `use
/// weaver_gate::` lines stop compiling, so the test never runs and a build
/// failure that proves nothing is reported instead.
#[test]
fn the_lib_target_stands_and_collects_the_doctests() {
    let meta = cargo_json(&["metadata", "--no-deps", "--format-version", "1"]);
    let package = meta["packages"]
        .as_array()
        .expect("packages is an array")
        .iter()
        .find(|p| p["name"] == "weaver-gate")
        // Named apart from the assertions below so a metadata call that
        // returned another workspace blames itself rather than the target.
        .expect("weaver-gate is not in this metadata: the call resolved another workspace");

    // A lib target's `kind` mirrors its crate types, so `crate-type =
    // ["rlib"]` reports `rlib` with doctests still collected. Matching the
    // family keeps this from failing on a change that breaks nothing.
    const LIB_KINDS: [&str; 6] = ["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];
    let lib = package["targets"]
        .as_array()
        .expect("targets is an array")
        .iter()
        .find(|t| {
            t["kind"].as_array().is_some_and(|ks| {
                ks.iter()
                    .any(|k| k.as_str().is_some_and(|k| LIB_KINDS.contains(&k)))
            })
        })
        .expect("no lib target: cargo collects no doctest and the bind-shape pins go unenforced");

    assert_eq!(
        lib["doctest"],
        serde_json::Value::Bool(true),
        "doctest = false: `cargo test -p weaver-gate` runs no doctest and the pins go unenforced"
    );
}

/// **No async runtime, no logging crate, no config-format implementation.** The
/// lifecycle traffic is two exchanges and the client traffic is deferred, so
/// nothing here needs an executor, and this crate writes no account of
/// anything: a logging crate would be a second author's first step.
///
/// Perturbation: add `tokio` or `tracing` to the manifest and this test fails.
/// Watched under exactly that addition.
#[test]
fn the_resolved_tree_carries_no_runtime_no_logging_no_yaml() {
    let names = crate_names(&resolved_tree("all", None));
    for forbidden in [
        "tokio",
        "async-std",
        "smol",
        "tracing",
        "log",
        "env_logger",
        "slog",
        "serde_yaml",
        "serde_yaml_ng",
        "yaml-rust",
        "serde_yml",
        "toml",
    ] {
        assert!(
            !names.iter().any(|n| n == forbidden),
            "{forbidden} is in the resolved tree, and this crate takes none of its kind"
        );
    }
}
