//! The worker's command line, read from outside: the binary is started with
//! the arguments admin's start step would never hand it.
#![cfg(target_os = "linux")]

/// **A fourth positional is refused, not taken** (ADP-4 of
/// `weaver-spu-adapter-binding-Spec-amendment` section 4). The positional
/// once carried an identity the serving path assembled; that path retired,
/// so a fourth word is a caller's mistake and the worker exits naming it
/// rather than starting with the word silently dropped. The coordination
/// socket's directory does not exist, so a worker that took the word would
/// fail at the bind with another message rather than serve.
///
/// Perturbation: let the parse ignore the word and the worker fails at the
/// bind, its stderr naming no unexpected argument.
#[test]
fn a_fourth_positional_is_refused_by_name() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_worker"))
        .args([
            "/nonexistent-weaver-worker-arguments/coordination.sock",
            "/nonexistent/weaver-spu",
            "/nonexistent/weaver-gate",
            "an-identity",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the worker exited zero: {stderr}");
    assert!(
        stderr.contains("worker: unexpected argument an-identity"),
        "the refusal does not name the word: {stderr}"
    );
}
