//! conforms: admin-runs-as-root-or-performs-nothing
//! conforms: admin-answer-and-exit-status-agree
//! conforms: admin-residency-is-not-lifecycle-state
//!
//! The invocation's interface exercised as an invocation, per
//! `weaver-admin-Spec` section 2. These run the built binary rather than
//! calling into it, because what they assert is the process boundary: which
//! uid it demands, what it writes to standard output, and what status it
//! exits with. A unit test inside the crate can reach none of those.

use std::process::Command;

fn admin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_weaver-admin"))
}

/// The suite runs as an ordinary uid, so the root check is reachable exactly
/// as an operator without root would meet it.
fn running_as_root() -> bool {
    nix::unistd::getuid().is_root()
}

/// **The invocation runs as root or performs nothing.** Authorization is the
/// kernel's, and the refusal is enacted before any verb touches anything: no
/// configuration is read, no agent's root is looked at, and no agent is named.
///
/// Perturbation: remove the `running_as_root` guard from `run` in `main.rs`
/// and this test fails, because the invocation proceeds to argument parsing
/// and refuses as `malformed` or reads the configuration instead. Watched
/// under exactly that removal.
#[test]
fn a_non_root_invocation_performs_nothing() {
    if running_as_root() {
        eprintln!("SKIP a_non_root_invocation_performs_nothing: the suite holds root");
        return;
    }
    // No configuration in the environment: if the guard were absent, the
    // failure would be the missing configuration rather than the uid, and the
    // two are distinguishable in the object below.
    let out = admin()
        .arg("validate")
        .arg("alpha")
        .env_remove("WEAVER_ADMIN_CONFIG")
        .output()
        .expect("the binary runs");
    let body = String::from_utf8_lossy(&out.stdout);
    assert!(
        body.contains("\"kind\":\"unauthorized\""),
        "a non-root invocation refuses as unauthorized, got {body}"
    );
    assert_ne!(out.status.code(), Some(0), "and the status agrees");
}

/// **The answer and the exit status agree**, checked where they can disagree.
/// A refusal exits non-zero and writes one object carrying the floor's tag, so
/// a shell reads the status and a tool reads the object.
///
/// Perturbation: exit zero on the refusal branch of `main` and this test
/// fails. Watched under exactly that removal.
#[test]
fn a_refusal_writes_one_object_and_exits_non_zero() {
    let out = admin().output().expect("the binary runs");
    let body = String::from_utf8_lossy(&out.stdout);
    assert_ne!(out.status.code(), Some(0), "a refusal exits non-zero");
    assert_eq!(
        body.lines().filter(|l| !l.trim().is_empty()).count(),
        1,
        "one invocation writes one object, got {body}"
    );
    let parsed: serde_json::Value = serde_json::from_str(body.trim()).expect("one JSON object");
    assert!(
        parsed.get("kind").is_some(),
        "the object carries the floor's tag, got {body}"
    );
}

/// `show` refuses, and emits no state object, and the retired `list` is
/// malformed.
///
/// **This test does not watch the residency substitution, and saying so is the
/// point.** The invention Spec section 3 forbids, answering `show` with a
/// state read from the unit, is carried by
/// `show_answers_the_absence_as_unloaded` in `main.rs`, which tests
/// `dispatch` directly. This one cannot: the root guard answers before
/// `dispatch` is reached in an unprivileged suite, so the `show` arm is
/// unreachable from the binary and this test passed unchanged under that
/// substitution when it was run. What it does hold is narrower and still worth
/// holding: these verbs refuse, and no `state` object leaves the
/// binary on any path a non-root operator can reach.
#[test]
fn show_and_list_refuse_as_not_observable() {
    if running_as_root() {
        eprintln!("SKIP show_and_list_refuse_as_not_observable: needs the non-root path off");
        return;
    }
    // Without root the guard answers first, so this asserts the reachable half:
    // the refusal is a refusal, and no `state` answer is ever emitted by these
    // verbs on any path.
    for verb in [vec!["show", "alpha"], vec!["list"]] {
        let out = admin().args(&verb).output().expect("the binary runs");
        let body = String::from_utf8_lossy(&out.stdout);
        assert_ne!(out.status.code(), Some(0), "{verb:?} refuses");
        assert!(
            !body.contains("\"kind\":\"state\"") && !body.contains("\"kind\":\"agents\""),
            "{verb:?} constructs no AgentState, got {body}"
        );
    }
}

/// A pipe whose read end is already closed: every write to the write end
/// fails with `EPIPE`, as a caller that went away leaves standard output.
fn broken_pipe() -> std::os::fd::OwnedFd {
    let (read_end, write_end) = nix::unistd::pipe().expect("a pipe");
    drop(read_end);
    write_end
}

/// **A closed or broken standard output and error never end the verb**, per
/// `weaver-admin-Spec` section 2: the invocation ignores `SIGPIPE`, so its
/// writes fail with `EPIPE` instead, and those failures are discarded rather
/// than panicking. The refusal still exits 1, the status agreeing with the
/// object nobody could read. Perturbation: write the answer with `println!`
/// and the process panics on the broken pipe, exiting 101.
#[test]
fn a_broken_standard_output_never_ends_the_verb() {
    let out = admin()
        .arg("show")
        .arg("alpha")
        .env_remove("WEAVER_ADMIN_CONFIG")
        .stdout(broken_pipe())
        .stderr(broken_pipe())
        .status()
        .expect("the binary runs");
    assert_eq!(
        out.code(),
        Some(1),
        "a refusal exits 1 whatever became of the caller"
    );
}

/// **The mid-verb diagnostics survive a broken standard error too**: run as
/// root inside a user namespace, the invocation reaches the root's judgment,
/// which refuses `BoundaryUnverified` naming the directory on standard error,
/// a write that fails on the broken pipe. The verb completes and exits 1.
/// Perturbation: write the diagnostic with `eprintln!` and the process panics
/// part way, exiting 101. A box with no user namespace prints SKIP.
#[test]
fn a_broken_standard_error_never_ends_the_verb() {
    let base = std::env::temp_dir().join(format!("weaver-admin-broken-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("alpha")).unwrap();
    let namespaced = || {
        let mut command = Command::new("unshare");
        command
            .args(["--map-auto", "--map-root-user"])
            .arg(env!("CARGO_BIN_EXE_weaver-admin"))
            .arg("show")
            .arg("alpha")
            .env("WEAVER_ADMIN_CONFIG", &base);
        command
    };
    // The probe run, with standard error kept: the verb must reach its
    // diagnostic, or the run below would pass on `unshare`'s own failure.
    let probe = namespaced().output();
    let reached = probe
        .as_ref()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stderr).contains("weaver-admin:"));
    if !reached {
        let _ = std::fs::remove_dir_all(&base);
        eprintln!("SKIP a_broken_standard_error_never_ends_the_verb: no user namespace here");
        return;
    }
    let ran = Command::new("unshare")
        .args(["--map-auto", "--map-root-user"])
        .arg(env!("CARGO_BIN_EXE_weaver-admin"))
        .arg("show")
        .arg("alpha")
        .env("WEAVER_ADMIN_CONFIG", &base)
        .stdout(broken_pipe())
        .stderr(broken_pipe())
        .status();
    let _ = std::fs::remove_dir_all(&base);
    let Ok(status) = ran else {
        eprintln!("SKIP a_broken_standard_error_never_ends_the_verb: unshare could not run");
        return;
    };
    assert_eq!(
        status.code(),
        Some(1),
        "the verb completes on a broken standard error"
    );
}
