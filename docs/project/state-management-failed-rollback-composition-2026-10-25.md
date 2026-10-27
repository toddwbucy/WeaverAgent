# Failed rollback composition

Date: 2026-10-25.
Parent: draft #342 at fe07be03e20a6c5b144251384045b14957f26150.
Branch: code/state-rollback-failed-exit-composition.
Tracker: #146, lifecycle composition and failed-run teardown.

The public classifier-refusal rollback owner now consumes stopped custody into
its exact outer FoundationRun for bounded failed cleanup. Every representation
retains its original Enter, response evidence, accepted directive and Leave,
readings, pending author input and actual terminal receipt. A foreign join
returns both complete owners. The handoff is move-owned and not Clone.

Implements: harness Spec section 6 rollback and terminal release; harness
amendment HARNESS-HALT, HARNESS-HALT-BOUND, HARNESS-HALT-ACCOUNT,
HARNESS-RECORD-LIFETIME, HARNESS-ENTER-WINDOW and HARNESS-RELEASE-ABSENT;
trace Spec section 12 terminal Unload ordering.
The author assessment is conforms within this existing no-SPU rollback route.
Planner grading and broader lifecycle integration remain pending.

| Actual stopped state | Cleanup |
| --- | --- |
| Unload already Recorded, including later observer or Left-send failure | Retire remaining descriptors and perform bounded drain only. Preserve the existing Unload and its payload; author no later fault or second terminal. |
| Latched component failure before Unload | Existing failed cleanup reports the first fault, settles available owed accounting, retires resources and attempts component-failure Unload and drain. |
| Healthy accepted Leave expired | Close and abandon under that same Leave; invent no component halt, Unload or Left. |
| Progressing/successful owner, or healthy refusal awaiting root | Return the complete owner unchanged. Expired Enter is not a substitute component failure. |

The actual accepted Leave caps cleanup even before the rollback transition arms
its context deadline. Accepted rollback preserves the load cause and forced
fact. A failure before acceptance does not invent a Leave. The first cause and
original halt cap remain retained through later errors and wait failure.

Current Grants accounting reuses the existing private checks for the exact
request, read interval and pending author ticket. After the first fault attempt,
one nonblocking trace-only attempt can retain its actual Recorded sequence.
Ordinary capacity cannot delay mandatory terminal work. Both unavailable and
recorded cases keep the original measurement and inspection evidence without a
live completion, new Grants request or state write. Completed initial/current
readings and their comparison remain in the consumed owner.

Orchestration stays in worker/dev_loop::foundation::failed_rollback_exit. The
existing serving selector is unchanged. Do not select or deploy foundation.
Closed retains the evidence for the final failed worker exit and grants no
Ready, Left, save, turn, ordinary authoring or reusable context.

## Author evidence

The pinned toolchain used --locked and private target
/tmp/weaver-state-146-worker-session-target with CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4 and the normal thread stack. All 21 final gates passed,
including lock before/after, formatting, affected strict lint, test builds,
ordinary harness tests, all 13 delegated families and diff check.

- 2,055 ordinary harness tests passed, including both doctest runners.
- 417 delegated composition cases passed, including 22 new rollback cases.
- The ordinary run ignores 435; the delegated runs exercise 417 of
  them. Eighteen inherited ignores remain outside this evidence.
- All 17 compiled protective controls failed their named tests after
  positive baselines passed. Original hashes were restored before final gates.
- 696 source/configuration hashes match the final tested tree.
  Source manifest SHA256: 178359f2ad2af6ab4454f50372b104c07dbf442be16ae434ed7ba704ea1e77a6.

The circuits exercise real sockets, original ancillary descriptor adoption,
actual fork/reap and delegated cgroups, with declared state/classifier protocol
doubles. They inject real EOF or peer shutdown, deterministic clock/wait
failure, actual recorder-observer failure and real writer pressure. Coverage
includes refusal before recording, Recorded refusal, response, waiting root,
accepted transition, current Grants, completed comparison, seal/member release,
Unload observer failure, blocked drain, lost Left peer, actual Leave expiry,
foreign joins and the worker loop. The recorded-drain cap case supplies no
readiness. No new model or state operation is allowed during cleanup.

Controls cover all decision rows, exact run and original packet custody,
completed comparison, failure preflight, accepted Leave cap, actual terminal
receipt, rollback cause, owed ticket/measurement, descriptor retirement, bounded
post-Unload wait and the worker's Closed return. The named positive baselines
and deliberate failures are in mutations/results.json.

The first fixture run overflowed the normal debug thread stack. Isolating five
large by-value run moves in one helper removed redundant circuit-frame slots;
no stack allowance was increased. One subsequent assertion incorrectly expected
no Unload when a later drain wait failed; it now preserves the actual receipt.
The first ordinary rollback regression run could not bind local sockets inside
the sandbox; its host rerun passed all 94 cases. Earlier logs are retained and
are not included in the passing counts. One stale private-helper test message
now names its actual boundary: outer cleanup owns Unload.

Named code-review and verification-before-completion skills were unavailable
in both installed skill roots. Two manual author passes and rework are recorded
in the evidence. This is not a Planner, third-party, security or production
qualification grade. Eight privileged admin-startup cases and GPU qualification
were not run for this harness-only change. No CodeRabbit review was triggered.

## Remaining work and after-action

Healthy expired-startup rollback, Ready rollback, healthy Leave/save publication,
capture-owned failure cleanup, final dispatcher/process exit, production
artifacts, measured limits and qualified coupled cutover remain under #146.
This slice completes failed exit for the existing classifier-refusal rollback
composition, not those other admission or serving paths.

The plan was to preserve all rollback representations through failed exit.
The implementation follows the existing terminal order: an actual Recorded
Unload decides the route before any later fault or expiry. Sustain tests at each
owned representation, retained exact packets and non-ready deadline controls.
Carry the debug-frame and post-terminal receipt checks into the next plan:
move large fixture owners through one helper, and derive assertions from actual
Recorded evidence before injecting a later failure. The PR body names the exact
published tree and immutable evidence deposit. Only the operator merges/deploys.
