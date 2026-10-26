# Leave and save composition

Date: 2026-10-26. Author working record for issue #146.
Branch: code/state-leave-save-composition. PR: #350.
Correction parent: 22d29f42f1f712d20e32cb3a500d029b032c69cc.
Stack base: #347, 534eca304ad4d28d86a423093e7351b7002867d1.

## Scope and authority

This composition retains forced and graceful Leave through worker Lower,
member capture, admin publication, release and the matching final outcome.
The F2 correction returns a capture deadline observed by the outer wait to the
existing capture owner. Missing snapshot/finished results receive correlated
abort and a recorded refusal. A failure from the capture owner's own advance,
including post-Commit expiry, still enters failed cleanup.
The Planner accepted F2 at 22d29f42. The J1 correction makes the composed
fixture read through the refusal's actual Recorded sequence before judging
its trace. It changes no runtime behavior.

Implements: harness Spec section 6 Leave, Quiesce and wind-down; admin Spec
section 3 unload and force-unload; foundation PERSIST-SAVE, PERSIST-BEGIN,
PERSIST-TIME, PERSIST-EXPIRY and PERSIST-PUBLICATION-GUARD; persistence contract
amendment sections 2-3 PERSIST-CAPTURE, PERSIST-CONFIRM, PERSIST-END,
PERSIST-IMAGE and PERSIST-CLEANUP; HARNESS-CAPTURE, HARNESS-CONTROL-FAILURE,
HARNESS-CLOCK, HARNESS-HELD-BOUNDARY, HARNESS-HALT and HARNESS-HALT-BOUND;
TYPES-PERSIST; ADMIN-SAVE and ADMIN-RECOVERY; STATE-CIRCUIT-INSTRUMENT;
Spec 05 pair accounting and Spec 06 placement/generation cancellation.

**Do not deploy or select foundation.** Production still uses dev_loop::drive.
The replacement admin invocation remains dormant. #146 and this PR remain
open; J1 has an author correction pending Planner regrade.

| Named boundary | Current assessment |
| --- | --- |
| PERSIST-BEGIN/TIME/EXPIRY; HARNESS-CLOCK; HARNESS-CONTROL-FAILURE | F2 is Planner-accepted. The original capture and leg windows remain unchanged. Only the capture owner elects abort/refusal or failure. J1 corrects the fixture's later trace read. |
| TYPES-PERSIST; persistence contract 2 | F1 remains drifted: graceful capture still has a forced trace cause. Q1 needs a shared rule for converted/joined Leave before its correction. No cause rule is elected here. |
| HARNESS-CAPTURE; PERSIST-CAPTURE | Frozen Recorded frontier and owed filing remain under the existing owner. Planner B10's missing independent controls remain carried. |
| PERSIST-CONFIRM/IMAGE/CLEANUP/END | The existing owner requires actual abort before a timeout refusal, and preserves late result observations. B1/B2 and L1/L2 remain carried. |
| ADMIN-SAVE/RECOVERY; PUBLICATION-GUARD | This correction changes no admin behavior. L3/L4 and the publication review carries remain open. |
| Harness 6; Spec 05/06; HELD-BOUNDARY/HALT/HALT-BOUND | Existing graceful/forced lifecycle, first-failure and ownership rules remain. The review's management and component-death carries remain open. |
| STATE-CIRCUIT-INSTRUMENT | The affected worker run includes five joint cases with the actual admin participant and normal SQLite member. Synthetic model/classifier peers and test budgets do not qualify deployment. |

## Correction and caller census

| Site | Handling |
| --- | --- |
| FoundationEnteredRollback::wait_with_io | A pump Wait for CaptureResult or CaptureWindow yields Progress only while CleanupStage is Capture. It neither resets a bound nor claims a completed operation. |
| FoundationEnteredRollback::advance_with_io / handle_error | Errors returned by capture.advance still follow the existing failure path. Moving recovery to the common handler would swallow post-Commit expiry; a compiled control pins this distinction. |
| LeaveCapture / RecordingContext::capture_live | Existing snapshot/finished timeout handling queues abort on the original request and capture IDs, measures/discards late results and waits for actual Aborted before recording refusal. |
| Forced and graceful Leave wrappers | Both wait only on Pending and advance their same retained owner afterward. CaptureRefused returns that owner without electing a final Leave answer. |
| Ordinary entered rollback | It never enters Capture, so its waits and cleanup are unchanged. |
| Dormant worker runners | Continue the same owner on Progress and return it at CaptureRefused/Closed. Orchestration stays in worker/dev_loop. |
| Member instrument | Holds a real socket response until it receives the correlated Abort, then sends the late result before Aborted. No private deadline, completion or failure is injected. |
| Forced/graceful Leave fixture at CaptureRefused | After joining its reader thread, use the existing trace_through helper to read through last_author_sequence under a three-second fixture watch. This covers timeout and explicit refusals. The helper keeps partial lines and fails if the Recorded row never arrives. Other terminal modes keep their existing checks. |

The five new cases cover forced and graceful snapshot timeout, finished timeout,
a capture window expiring during the snapshot leg, and post-Commit expiry.
They use real monotonic poll and loaded test allowances: two seconds for the
snapshot/finished leg and one second for the short-window cases. They check
that the actual composed wait expires, the original bound is unchanged, the
late observation keeps its request identity, and the abort observation precedes
the exact snapshot_timeout or finished_timeout refusal. The late image/finished
result never reaches publication. Post-Commit expiry still fails the run.

The recoverable cases stop at the existing CaptureRefused boundary with no
halt, successful report or Unload. They do not claim a delivered
SavePointNotTaken wire answer, final Left or retry policy. Those final Leave
outcomes still require #348's approval, review and merge.

## Verification

Pinned nightly-2026-02-13, --locked, CARGO_INCREMENTAL=0 and
CARGO_BUILD_JOBS=4. Fresh private target:
/tmp/weaver-state-146-capture-trace-target.
Lock ran before Cargo. Cargo chains and Git mutations were serial; no source
changed during a build or test invocation.

The Planner reproduced J1 at 22d29f42: a CaptureRefused step can enqueue its
refusal before the writer reaches the reader. Our unchanged-source baseline
ran finished timeout and graceful snapshot timeout twenty times each; all
forty passed. That sample did not reproduce the measured intermittent failure.
The correction replaces the timing assumption with an actual sequence read.

- Full harness: 2,105 passed, including 31 doctests; 701 ignored by the ordinary
  invocation. The five new process cases are among those ignores. The #331
  case was included and passed.
- Each of the five timeout cases passed twenty individual runs. The existing
  explicit forced-refusal case also passed twenty, for 120 individual runs.
- All 153 affected worker_session_ and worker_graceful_ cases passed in one
  delegated invocation with one test thread. The other worker families from
  the earlier 637-case F2 gate were not rerun for this fixture-only change.
- The five joint worker cases each ran the actual admin participant and normal
  SQLite member, including failed-release cases. No full privileged admin
  startup gate or production qualification is claimed.
- Strict harness clippy for all targets, formatting, diff and final lock passed.
- The full ordinary suite includes the existing helper tests for a delayed,
  partial Recorded row and a missing row at its deadline. The helper is
  unchanged. No new runtime mutation claim is made for this fixture change.
- F2's three compiled controls remain in its earlier evidence deposit: omit
  CaptureResult handoff, omit CaptureWindow handoff, and swallow capture-owned
  expiry. Those controls were not rerun here.
- All 1,024 source-manifest files matched after verification. This working
  record is updated afterward and is separately included in the deposit.

Count method for B9: use each Cargo test binary's final summary, then both
rustdoc groups; do not add nested child summaries. The breakdown is library
2,024; worker 7; authorship 11; channels 7; manifest 7; service 10; start_step 1;
state_flow 7; doctests 2 + 29. This reproduces 2,105 on this source. The review's
2,134 is not reproduced by this rule; the complete log and parsed groups are
preserved without attributing that difference to an unobserved test change.

B7 prerequisites were built in the same fresh target before the full gate:

    cargo build -p weaver-state --features sqlite --bin weaver-state --locked
    cargo test -p weaver-admin --bin weaver-admin --no-run --locked

The worker run supplies WEAVER_STOP_ADMIN_TEST_BIN,
WEAVER_OBSERVE_ADMIN_TEST_BIN and WEAVER_LEAVE_ADMIN_TEST_BIN with the fresh
admin test executable named in prerequisites.json. It runs in an isolated
root-owned delegated scope. These author runs do not grant the Planner
permission for a separate privileged run.

The current baseline and correction use the same fresh target in this
worktree. F2's earlier failed baselines and compiled controls remain in their
own immutable deposit. No baseline run is relabelled as a J1 reproduction.

## Evidence and remaining work

Immutable deposit:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-capture-trace-20261026-dbc6acee/.
Source archive SHA256:
dbc6acee409fb71f357c1ce232de4a70b0c2e06ea89eeeb165c8c54bc39da195.
Source manifest SHA256:
57b5650613dbdaf4d76d0225ff9578502707afca03e33c4e78f32699758c4fe5.

The F2 runtime evidence remains in the same approved root under
state-capture-wait-20261026-6602cb7d/.

F2 is Planner-accepted. J1 has an author correction. F1/Q1 remains open. The rest
of Planner review 5657773404 is not collectively closed. B7 has explicit prerequisites;
B9 has the reproducible count above. B10's additional mutation claims remain
unproven by merely running the joint baseline. #348/#349 authority, #350 L3,
held-owner descendants #351/#352, integration with the #353 join into #332,
on-demand save, final selection, artifact/limit qualification and security
remain part of #146. Only the operator merges or deploys.

The required named review skills remain unavailable. Manual review and tests
do not claim those invocations, Planner clearance or production qualification.
No CodeRabbit review is triggered by this correction.

The review exposed a distinction the primitive tests missed: a recoverable
capture deadline can escape through the containing owner's wait. Keep tests
on the composed wait and keep recovery local to that handoff; recovery in the
common error handler would hide capture-owned failures.
J1 also distinguishes Recorded admission from bytes received at the sink. Keep
composed trace assertions behind the actual Recorded sequence. The bounded
helper makes that sequence the read boundary.
