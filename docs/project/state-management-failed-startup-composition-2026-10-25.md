# Failed startup composition

Date: 2026-10-25.
Parent: draft #341 at b8a09fd703de5d79f5a39b2f589aefcde5823358.
Branch: code/state-startup-failed-exit-composition.
Tracker: #146, startup/lifecycle composition and failed-run teardown.

The public FoundationEnter can now consume an actual latched startup failure
into a handoff for its exact outer FoundationRun. The joined owner retains the
original Enter bytes, pending requests, consumed Admit reply, extracted boot
proofs, listener, gate, tools group and actual child resources through failed
cleanup. It matches the context, original decode/residency ports and halt cap;
a foreign join returns both complete owners. Healthy refusal and Enter expiry
remain owned for rollback and are not relabelled component failures.

Implements: harness amendment HARNESS-HALT, HARNESS-HALT-BOUND,
HARNESS-HALT-ACCOUNT, HARNESS-OPEN, HARNESS-RECORD-LIFETIME,
HARNESS-ENTER-WINDOW and HARNESS-RELEASE-ABSENT; Spec 09 FLOW-COUNTS.
The author assessment is conforms for this startup route. Planner grading is
pending, and no broader lifecycle completion is asserted.

| Boundary | Retained fact and action |
| --- | --- |
| Stopped startup | Consume the actual stage once, including evidence extracted from primitive owners; seal the actual listener and retire the original peer. |
| Matched outer run | Move the consumed Admit packet into existing failed cleanup without reconstructing it. Preserve the first failure and original cap. |
| Reporting | Attempt the first fault before gate work and trace-only owed Open accounting. No fabricated pre-load bracket. |
| Raised gate | Require an exact sent Lower and matching GateStopped plus an empty actual tools group; retain intervening frames without starting a turn. |
| Unconfirmed gate | Use actual child/tools containment under its existing bound capped by the original halt deadline. Retain rejected replies. |
| Release and drain | Drive existing owed accounts, actual SPU release, state/classifier retirement, Unload and bounded trace drain. Return only ChannelFault::Closed. |

Orchestration is in worker/dev_loop::foundation::failed_startup_exit. The serving
selector is unchanged. Do not select or deploy foundation from this slice.
Closed retains terminal evidence for the final failed worker exit; it is not a
claim that a blocked writer has disappeared while that owner remains retained.

A real early-startup circuit found a FLOW-COUNTS defect: valid Open confirmation
could precede observation initialization, which happened only after its held
measurement released. Failure in that interval had no final flow sample. The
latch now uses the actual held confirmation timestamp for that sample, without
releasing the measurement or permitting new state work. The confirmation,
load and activation boundary cases pin the original timestamp.

## Author evidence

The locked pinned toolchain used private target
/tmp/weaver-state-146-worker-session-target with CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4 and the normal thread stack. All 21 final gates passed:
lock before/after, formatting, affected strict lint, test builds, ordinary
harness tests, all 13 delegated composition families and diff check.

- 2,050 ordinary harness tests passed, including doctests.
- 395 delegated cases passed, including 22 new failed-startup cases.
- 413 cases are ignored by the ordinary run; the delegated runs exercise
  395 of them. Eighteen inherited ignores remain outside this evidence.
- All 14 compiled protective mutations failed their named tests after positive
  baselines passed. The original files were restored and hashed.
- 694 source/configuration files were verified unchanged across the gates.
  Source manifest SHA256: 3212988ee820f27c305adb7ea58e5f8fda7ca537e8e841cf3fed85b6346f1ef1.

The new circuits use real Unix sockets, original Enter ancillary descriptors,
actual fork/reap and tools cgroups, plus explicitly declared member/classifier/
model protocol doubles. They cover pre-Open, confirmed Open before Load,
activation, held measurement, pre-SPU fork, pending Admit, boot proofs, owed
Open, installed journal, raised gate, actual worker loop, wrong/premature/silent
gate replies, trace pressure, deadline/wait failure and foreign joins. The
silent gate case supplies no readiness and requires the original cap.

Controls cover original run/Enter custody, moved Admit bytes, certificate/prior/
summary proof retention, report-before-Lower, exact and timely gate evidence,
original wait cap, descriptor retirement, the Open sample, absent-SPU release
and the worker's Closed return. The handoff also has a compile-fail non-Clone pin.

The first experiments failed on the actual Open sample defect and two fixture
assumptions: EOF before dropping retained terminal evidence, and a turn-required
model measurement used during startup pressure. Both fixtures were corrected;
only the later named passing runs count. Full earlier logs remain in the evidence.

The named code-review and verification-before-completion skills were unavailable
in both installed skill roots. Two manual author passes and the rework are
recorded in /tmp/state-startup-failed-exit-20261025/author-review.md. This is not
a Planner, third-party, security or production qualification grade.

## Remaining work and after-action

Accepted rollback failures, healthy expired-startup rollback, Ready rollback,
healthy Leave/save publication, final dispatcher/process-exit integration,
production artifacts, measured limits and qualified coupled cutover remain
under #146. Eight privileged admin-startup cases and GPU qualification were
not run for this harness-only act. No CodeRabbit review was triggered.

The plan was the consuming failed-startup join; the Open sample fix was required
to make its earliest real failure account correctly. Sustain actual sockets,
retained move-owned evidence and bounded waits. Carry two fixture checks into the
next plan: obey each trace kind's turn rule, and distinguish terminal evidence
lifetime from process lifetime. The PR body names the exact published tree and
verified evidence deposit. Only the operator merges or deploys.
