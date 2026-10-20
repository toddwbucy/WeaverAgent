use super::*;

fn judge(worker: &str, gate: &str, spu: &str) -> Result<(), String> {
    judge_names(Path::new(worker), Path::new(gate), Path::new(spu))
}

/// The installed stack's own names pass.
#[test]
fn distinct_names_pass() {
    assert_eq!(
        judge(
            "/opt/weaver/bin/weaver-worker",
            "/opt/weaver/bin/weaver-gate",
            "/opt/weaver/bin/weaver-spu"
        ),
        Ok(())
    );
    assert_eq!(
        judge(
            "/opt/weaver/bin/weaver-worker",
            "/opt/weaver/bin/weaver-gate",
            "/opt/alternate/bin/custom-spu"
        ),
        Ok(())
    );
}

/// **Every pair is judged**, the state member's fixed name included.
///
/// Perturbation: judge the first pair only and the later cases pass.
#[test]
fn every_collision_fails_naming_its_pair() {
    for (worker, gate, spu, pair) in [
        ("/a/x", "/b/x", "/c/spu", "the worker and the gate"),
        (
            "/a/weaver-state",
            "/b/gate",
            "/c/spu",
            "the worker and the state member",
        ),
        (
            "/a/worker",
            "/b/weaver-state",
            "/c/spu",
            "the state member and the gate",
        ),
        (
            "/a/worker",
            "/b/gate",
            "/c/weaver-state",
            "the state member and the SPU",
        ),
        ("/a/worker", "/b/gate", "/c/gate", "the gate and the SPU"),
        (
            "/a/worker",
            "/b/gate",
            "/c/worker",
            "the worker and the SPU",
        ),
    ] {
        let failure = judge(worker, gate, spu).expect_err(pair);
        assert!(failure.starts_with(pair), "{pair}: {failure}");
    }
}
