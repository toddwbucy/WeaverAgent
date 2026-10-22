# Capture cleanup continuation

Date: 2026-10-21. Author verification on Olympus. Advances issue #146 and answers
issue #223. Branch: `code/state-capture-cleanup-resume`, stacked on PR #226 at
`0f42f7395e980bc8f4071d3fa9539fefafc7b99f`.

## Behavior and authority

A timely Snapshot or Acknowledge refusal consumed its recorded completion before
cleanup admission. If that admission yielded to Leave, the owner lost the operation
that would enqueue Abort. It now retains an explicit abort obligation before the
recoverable check, while keeping the predecessor phase and bound. Ordinary capture
advancement resumes that obligation through the same lifecycle gate.

One successful enqueue changes the phase to Aborting and installs its existing absolute
cleanup bound. Repeated advancement neither duplicates Abort nor grants more time. The
original refusal remains private until the matching recorded Aborted result. Existing
fatal and earlier lifecycle outcomes keep precedence.

Implements: state-management-foundation-Spec PERSIST-CLEANUP and persistence actor
transitions, scoped to refusal, timeout and cancellation cleanup continuation. Author
grade: conforms for those transitions. This does not implement the remaining lifecycle
publication owner.

The Planner's review identified that encoding this obligation as a phase also changed
cancellation at a second clock boundary. The revised owner keeps the obligation
separate. Cancellation validates its capture identity before settling owned work. A new
cancellation checks lifecycle admission once and a Wait requires the parent to re-issue
it. Enqueue and ownership installation have no second recoverable check. If settlement
itself admits an already owed Abort, cancellation keeps that accepted continuation
without another admission check. Cancellation cannot replace an outstanding Release.

The same retained obligation covers original timeouts and results whose actual
Recorded boundary missed the original deadline. Regression fixtures now interrupt each
of those two legs at cleanup admission, resume through Leave, and require one correlated
Abort with the unchanged cleanup bound. Additional cases pin both cancellation clock
boundaries, wrong-ID refusal before an owed Abort is enqueued, and Release ownership.
Review: [Planner PR #227 review][planner]. The preceding revision passed 1179 harness
tests (18 inherited ignored), all-target harness clippy, lock and formatting checks.
Six compiled protective probes failed and exact restorations passed: dropped retention,
retention after the wait, skipped resume, a second cancellation yield, wrong-ID
settlement before admission, and refusal visibility before Aborted. An independent
source review found no concrete defect in the revised ownership and deadline handling.
The baseline results below remain tied to the earlier implementation.

## Recorded cleanup and delayed cancellation

The [next review][delayed-cancel] found that the first correction checked cancellation
admission before settling existing Recorded cleanup. A timely Aborted or CaptureReleased
could then halt with ControlBound when the parent resumed after the small-control
deadline. Cancellation now settles that proof immediately after its identity check and
before checking the remaining deadlines. Consumer scheduling cannot invalidate timely
Recorded completion. An unfinished cleanup still owns and enforces its original bound.

Settlement can itself enqueue an owed Abort after an original refusal or late result.
The transition into Aborting proves that this settlement already passed admission.
Cancellation returns Pending for that owned continuation without sampling a second live
boundary. The original phase remains intact while a recoverable Wait prevents enqueue.
The identity check remains first, so a wrong-ID call cannot advance owed cleanup.

Three new real recorder/socket regressions cover timely Aborted, cancellation-driven
CaptureReleased and ordinary Release completion with a delayed cancellation consumer,
newly consumed and already pending refusal cleanup across repeated Leave waits, and
deadline failure for cleanup that has no Recorded result. They check exact retained
answers, one cleanup request, no renewed bound and no false halt after timely
completion.
The updated harness suite passed 1182 tests with 18 inherited ignored cases. Strict
all-target clippy, formatting, lock and diff checks passed. Three compiled mutations
failed: restore admission before settlement, recheck after settlement enqueues Abort,
and bypass admission for an already Aborting owner. Each exact restoration passed.
Independent source review found no concrete defect. The new logs and source hashes
are deposited under capture/review2 in the shared evidence directory. The 1179-test
result and six probes above remain evidence for the preceding revision.

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

The initial independent source review covered the production and regression changes.
It did not grade Beginning, Commit, publication, actual member room-file cleanup or a
production save. These tests and the author grade do not replace the Planner's review
or qualification. The PR was submitted as a draft; only the operator merges it.
Issue #146 remains open.

[planner]:
https://github.com/toddwbucy/WeaverAgent/pull/227#pullrequestreview-5608883320
[delayed-cancel]:
https://github.com/toddwbucy/WeaverAgent/pull/227#discussion_r4344359913
