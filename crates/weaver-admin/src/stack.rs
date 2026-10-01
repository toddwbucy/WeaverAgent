//! The binaries one agent's stack names, judged before any verb, per
//! `weaver-admin-Spec` section 9.
//!
//! The agent's root names its worker, gate and SPU, and the state member is the
//! worker's sibling. The record keys the stack by file name, so the four must
//! differ, and a root that names two binaries under one file name fails the
//! invocation as an unreadable configuration does, before any unit is asked.

use std::path::Path;

/// The state member's binary, a sibling of the worker's, whose name the stack
/// already holds, per section 9.
const STATE_MEMBER: &str = "weaver-state";

/// **Every name the stack records differs from every other**, the stack being
/// keyed by file name: the worker, the state member, the gate and the SPU,
/// pairwise. An error names the two that collide.
pub fn judge_names(worker: &Path, gate: &Path, spu: &Path) -> Result<(), String> {
    let named = [
        ("the worker", file_name(worker)),
        ("the state member", STATE_MEMBER.to_string()),
        ("the gate", file_name(gate)),
        ("the SPU", file_name(spu)),
    ];
    for (i, (one, name)) in named.iter().enumerate() {
        for (other, other_name) in &named[i + 1..] {
            if name == other_name {
                return Err(format!(
                    "{one} and {other} share the file name {name:?}, which the stack keys by"
                ));
            }
        }
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
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
                "/opt/weaver/python-spu/python-spu.pyz"
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
}
