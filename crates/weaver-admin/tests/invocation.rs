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
/// configuration is read, no allow-list is consulted, and no agent is named.
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

/// `show` and `list` refuse, and emit no state or agents object.
///
/// **This test does not watch the residency substitution, and saying so is the
/// point.** The invention Spec section 3 forbids, answering `show` with a
/// state read from the unit, is carried by
/// `show_and_list_construct_no_agent_state` in `main.rs`, which tests
/// `dispatch` directly. This one cannot: the root guard answers before
/// `dispatch` is reached in an unprivileged suite, so the `show` arm is
/// unreachable from the binary and this test passed unchanged under that
/// substitution when it was run. What it does hold is narrower and still worth
/// holding: these verbs refuse, and no `state` or `agents` object leaves the
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
