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

/// **The loop's two flags are handed together or not at all**
/// (`weaver-harness-Spec` section 2.5): either alone is refused by name
/// before the bind, and both together pass the parse and fail at the bind,
/// the socket's directory not existing. Perturbation: let one flag stand
/// alone, and its row fails at the bind instead.
#[test]
fn the_loops_two_flags_are_handed_together() {
    let run = |flags: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_worker"))
            .args([
                "/nonexistent-weaver-worker-arguments/coordination.sock",
                "/nonexistent/weaver-spu",
                "/nonexistent/weaver-gate",
                "--gate-socket",
                "/nonexistent-weaver-worker-arguments/gate/gate.sock",
            ])
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let together = "--loop-launcher and --loop-grant are handed together or not at all";
    for alone in [
        ["--loop-launcher", "/opt/weaver/bin/weaver-loop-launch"],
        ["--loop-grant", "/opt/weaver/lib/weaver-loop/grant"],
    ] {
        let stderr = run(&alone);
        assert!(stderr.contains(together), "{alone:?}: {stderr}");
    }
    let stderr = run(&[
        "--loop-launcher",
        "/opt/weaver/bin/weaver-loop-launch",
        "--loop-grant",
        "/opt/weaver/lib/weaver-loop/grant",
    ]);
    assert!(
        !stderr.contains(together) && stderr.contains("did not bind"),
        "both flags pass the parse: {stderr}"
    );
}

/// **The gate's socket is handed by admin, never derived** (`weaver-harness-Spec`
/// section 3, `weaver-admin-Spec` section 6): a worker given no
/// `--gate-socket` refuses by name before the bind, and one given the flag with
/// no value refuses naming the flag. With the flag and a value, the parse
/// passes and the worker fails at the bind, the socket's directory not
/// existing. Perturbation: fall back to a `gate.sock` beside the coordination
/// socket where the flag is absent, and the first row fails at the bind
/// instead.
#[test]
fn the_gate_socket_is_required_by_name() {
    let run = |flags: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_worker"))
            .args([
                "/nonexistent-weaver-worker-arguments/coordination.sock",
                "/nonexistent/weaver-spu",
                "/nonexistent/weaver-gate",
            ])
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let stderr = run(&[]);
    assert!(
        stderr.contains("worker: --gate-socket is required") && !stderr.contains("did not bind"),
        "no flag: {stderr}"
    );
    let stderr = run(&["--gate-socket"]);
    assert!(
        stderr.contains("worker: --gate-socket takes a value"),
        "no value: {stderr}"
    );
    let stderr = run(&[
        "--gate-socket",
        "/nonexistent-weaver-worker-arguments/gate/gate.sock",
    ]);
    assert!(
        !stderr.contains("--gate-socket") && stderr.contains("did not bind"),
        "the flag with its value passes the parse: {stderr}"
    );
}
