# Capture cleanup continuation

Date: 2026-10-21. Author verification on Olympus. Advances issue #146 and answers
issue #223. Branch: `code/state-capture-cleanup-resume`, stacked on PR #226 at
`0f42f7395e980bc8f4071d3fa9539fefafc7b99f`.

## Behavior and authority

A timely Snapshot or Acknowledge refusal consumed its recorded completion before
cleanup admission. If that admission yielded to Leave, the owner lost the operation
that would enqueue Abort. It now retains `AbortPending` before the recoverable check.
Ordinary capture advancement resumes that obligation through the same lifecycle gate.

One successful enqueue changes the phase to Aborting and installs its existing absolute
cleanup bound. Repeated advancement neither duplicates Abort nor grants more time. The
original refusal remains private until the matching recorded Aborted result. Existing
fatal and earlier lifecycle outcomes keep precedence.

Implements: state-management-foundation-Spec PERSIST-CLEANUP and persistence actor
transitions, scoped to refusal cleanup continuation. Author grade: conforms for that
transition. This does not implement the remaining lifecycle publication owner.

## Evidence

The regression drives both original legs through actual recorder and socket fixtures.
It injects Leave at cleanup admission after consuming Recorded, handles that wait
through the lifecycle API, and resumes ordinary advancement. It checks the original
correlation, exactly one Abort, the unchanged cleanup deadline and the exact refusal
after matching Aborted. It fails on the unchanged production code.

Private target: `/tmp/weaver-146-capture-target`. Gates ran serially with `--locked`:

- `process/gates/lock.sh`: passed.
- `cargo test -p weaver-harness --locked`: 1174 passed, 18 inherited ignored.
- `cargo clippy -p weaver-harness --all-targets --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Removing the retained obligation or its resumed advancement each compiled and failed
the regression. Each exact restoration passed. Tested capture source SHA-256:
`54f0536a0fa6b4a77be592f18defb013450bd93a108a099cbd1348526f6c9d33`.
The initial main-based suite passed 1168 tests. The 1174 count above includes the
preceding draft stack. Logs and mutation results are deposited on the shared bulk store
under `weaver-testing/state-management-146-olympus-2026-10-21/capture/`.
The local staging path is `/tmp/weaver-146-evidence/capture/`.

The first sandbox attempt could not create its Unix socket. The host-lane baseline
then reproduced the missing Abort assertion. Only that host result is behavioral
evidence. No deployed agent was accessed.

An independent source review found no defect in this two-file change. It did not grade
Beginning, Commit, publication, actual member room-file cleanup or a production save.
These tests and the author grade do not replace the Planner's review or qualification.
The PR is submitted as a draft and only the operator merges it. Issue #146 remains open.
