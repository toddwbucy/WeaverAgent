# Flag-sample turn boundary evidence

**Version:** 0.1, 2026-10-29.
**Parent:** Issue 146, state management implementation.

This test-only act addresses Y2 in Planner review 5692935815 of #383. It also adds
diagnostics requested by Y1 and corrects the author record's superseded classification
of #385 AA1. The branch is `code/state-plan-flags-boundary`, based on #385 at
`4d8f4a144807b88680338f67babb04ffd7054824`. Production code and serving selection do
not change. The two helpers in rollback implementation files are cfg(test) only.

Implements: Spec 07 PLAN-OBSERVATION, harness amendment HARNESS-PLAN-OBSERVE and
HARNESS-READ at the existing worker boundary. These conform within the tested scope.
This adds no selection, progression, plan mutation or language qualification claim.
No broad issue #146 checkbox closes.

## Process case and protective control

The real worker and normal SQLite member first complete a root plan update. A private
fixture marker then asks the real gate child to send an input on its existing socket.
The worker checks that input, opens a turn and records its source. The explicit caller
starts a flag observation at that checked-turn boundary while the member withholds its
response. The worker must expose Pending, and finish must return the complete input
owner rather than the available turn.

After the member answers and the read, cleanup and plan measurement complete, the
ordinary trace contains exactly one flag sample. The same raw gate input and turn
identity survive. The caller takes the sample, finishes to the original checked turn
and returns it to service. No new turn or duplicate sample is created. The existing
flag-family assertions still pin the summary, request ID, count, source, cleanup and
original read bound.

The compiled control removes only flags_pending from FoundationInput::progress's
Pending guard. The new case fails at its first boundary assertion with Turn where
Pending is required. This is the guard removal that survived #383's Planner tests.
The control compiles and runs in a delegated process scope. All 809 Rust/configuration
hashes restore, and the final gates run after restoration without further source edits.

The first development case tried to start sampling while an input was Accepted.
It correctly received Busy because that input still owns the reader. The corrected
case consumes the accepted input into its checked turn before sampling. Both the failed
development log and the corrected pass are retained. The case does not change reader
admission to make the fixture pass.

## Cleanup diagnostics and unresolved wording

Planner Y1 reported one worker_plan_flags_leave process_exit failure in 39 runs,
followed by 30 isolated passes and passing family reruns. No root cause is established.
The assertion now includes detail, abandoned, terminal refusal/stopped/sequence,
context failure and halt. It still requires Success. Its pass in this act's family
run does not resolve that intermittent failure. The measurement remains on #146.

The admission proof record now agrees with #385's reviewed disposition: AA1 is a Spec
wording question, filed as #386. Draft #387 proposes explicit PlanUpdate-only
pre-admission accounting and awaits the operator's election, review and merge. The
unpublished local experiment is not included in this branch or its test evidence.
The already graded #385 head is unchanged.

## Validation and limits

The pinned nightly-2026-02-13 toolchain used --locked and private target
`/tmp/weaver-state-146-serving-organ-loss-target`, with CARGO_INCREMENTAL=0. Cargo ran
without root. Only compiled process tests ran in owned systemd scopes with Delegate=yes.
The actual admin binary was supplied to the worker family and its hash is recorded.

| Gate | Result |
| --- | --- |
| Lock, rustfmt, strict all-target harness clippy | Pass |
| Focused ordinary flag tests | 9 passed, 8 ignored process entries |
| Delegated worker_plan_ family | 22 distinct top-level cases passed, including all 8 flag cases and actual admin/SQLite |
| Compiled flags_pending removal | Caught by Turn versus Pending assertion |
| Source restoration and final-gate hashes | All 809 match |
| DocsMirror and diff checks | 3 tests passed, clean diff |

Nested child output is not counted again. The full harness package suite is not rerun
for this test-only act, and earlier full-suite counts are not relabeled as this head's
results. The new fixture uses a scripted language checker and does not qualify a
production language artifact or deployment tuple.

The scratch evidence is `/tmp/state-plan-flags-boundary-20261029`. Publication deposits
the source archive, exact logs, source hashes, control and process census under the
authorized issue #146 evidence root and records its verified destination on the PR.
Manual author checks found no remaining issue in this slice. The code-review and
verification-before-completion skills are unavailable, so no skill pass or Planner
grade is claimed for this new head. Operator review/merge, inherited batch carries,
production qualification and coupled cutover remain owed.
