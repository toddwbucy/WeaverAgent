# Bounded gate answer non-delivery

This slice answers issue #222 and advances issue #146 section G. It is stacked
on PR #227, branch `code/state-capture-cleanup-resume`, at f6f2fb91. Its branch is
`code/state-gate-nondelivery`.

## Behavior and authority

A completed execution whose answer cannot cross the gate channel before its
original containment deadline returns DeliveryBound, retaining the exchange and
whether the unsent answer is a tool terminal or an OutOfOrder refusal. It no longer
reports an unobserved peer closure. The production reporter writes one structured
bounded_nondelivery diagnostic to standard error and exits unsuccessfully. An
observed orderly closure remains a clean exit. The fixed channel-fault vocabulary
does not change.

The original bound covers the first write and retries. Terminal and retained-refusal
delivery recheck it after polling and immediately before attempting a write. A
transport fault already observed by that poll keeps its identity. Containment and
reaping pass the same deadline through supervision; no fresh delivery window is
created. Active execution before containment retains its existing supervision.

Implements: weaver-harness-gate-contract section 2, Execute a tool / One clock,
scoped to bounded answer delivery. Author grade: conforms.

Implements: weaver-gate-Spec section 8 and weaver-admin-Spec section 6.1, scoped
to retaining the original containment bound and reporting an unanswered execution.
Author grade: conforms. No containment duration, kill rule or wire case is added.

## Evidence

Author checks ran on Olympus with private target /tmp/weaver-146-gate-target and
serialized Cargo invocations using --locked after the lock gate:

- Gate tests: 84 passed, 56 inherited ignored.
- Gate all-target clippy with -D warnings, formatting and lock: passed.
- Twelve compiled protective mutations failed their focused behavioral tests.
  Each exact source restoration passed its focused rerun.

Actual Unix sockets exercise backpressure, expired first writes on writable peers,
retained refusals, and peer closure. Deterministic wait-boundary hooks force expiry
between the outer check and the attempted write. Subprocess tests exercise the actual
reporter and assert its complete diagnostic and process status. The serving call to
that reporter is source-reviewed; these fixtures do not drive a real contained tool
through a full host timeout. The 56 ignored cases are not qualification evidence.

Mutations distinguish both expiry producers from closure, discard or renew deadlines,
remove each pre-write check, reverse observed-fault priority, and corrupt diagnostic
or exit behavior. Independent review found the post-poll deadline edge; its correction
and three regressions are included in the checked source.

Logs, source hashes and mutation/restoration manifests are deposited under
`weaver-testing/state-management-146-olympus-2026-10-21/gate/` on the shared bulk
store. /tmp/weaver-146-evidence/gate is staging only. These are author checks, not
the Planner's grade or production qualification. The PR is submitted as a draft.
Only the operator merges; issue #146 and the coupled cutover remain open.

## Leader-reap review follow-up

Planner review 5609027238 graded both Implements rows conforms, with nine of ten
protective mutations biting. G9 survived: changing only the production leader-reap
wait to omit its delivery deadline had no behavioral test. The production argument
was correct; the uncovered call path was the defect in the evidence.

The leader-reap closure is now one private helper shared by run_in_home and a real
child/socket regression. The fixture retains an actual OutOfOrder envelope, crosses
the original deadline at the poll boundary and requires that exact refusal to remain
unsent. It then requires DeliveryBound at the same bound. Its child cleanup is bounded
also when a mutant causes an assertion failure. No new production interval, deadline
ownership policy, outcome or wire case is introduced.

The updated gate suite passed 85 tests with 56 inherited ignored cases. Strict
all-target clippy, formatting, lock and diff checks passed. G9 changed the exact
shared production forwarding call, compiled and failed the new assertion. Its
byte-exact source restoration passed. Independent source review found no concrete
defect. The 84-test and twelve-mutation results above belong to the earlier source.
The review's other filed/batched findings retain their separate disposition in #146.
Follow-up logs and source hashes are in the shared gate/reap evidence deposit.

## Deadline ownership and production reap follow-up

Planner comment 6332490043 confirmed the earlier G9 helper mutation bites, but G9b
survived: replacing run_in_home's helper call with a raw reap closure could still omit
the deadline. The prior 85-test evidence therefore pins the helper and does not prove
that production must use it. This follow-up answers that narrower production gap and
the review's deadline-ownership cleanup in row 4.

The actual containment supervisor now owns its original Instant at construction.
Its wait accepts only a duration, and its finish obtains the same stored Instant.
No containment caller can supply None or a replacement deadline to a later wait.
Answer delivery uses the same implementation with the ExecutionAnswer's optional
bound: a refusal before any process starts still has no containment interval. That
case introduces no synthetic deadline. The ordinary active-execution Supervision
remains unchanged. No new kill clock, interval, wire case or policy is elected.

Compile-time function pins require a real Instant at the containment constructor and
no deadline argument at wait. The exact old G9b expression with wait(slice, None)
therefore cannot compile. That structural protection is distinct from a compiled
behavioral mutation; it is not counted as one.

A new delegated fixture runs the actual execute_answer_in/run_in_home path with a
real shell, private scratch cgroup and socket. After the actual kill it waits for
that group to empty, then queues a misplaced control before releasing containment.
The production empty wait retains its OutOfOrder answer. A timing-only hook at the
actual leader-reap stage delays the next poll beyond the original containment bound.
The fixture requires bounded non-delivery and no peer answer. It uses the unchanged
30-second bound, so each run takes about 30 seconds. It does not touch a live agent.
The scratch group's existing drop path kills, waits and removes only its own group.

A plain socket fixture also checks preexecution refusals with no containment interval
still deliver a queued OutOfOrder refusal followed by the terminal answer. Existing
observed-fault priority, first-write expiry and retained-refusal cases remain required.

The updated ordinary gate suite passed 86 tests with 57 ignored. The new delegated
production fixture passed separately under an owned systemd user scope with
Delegate=yes, using the same private target. Its original containment interval ran
for 30.04 seconds. That exercises one of the suite's 57 ignored cases, leaving the
56 inherited ignored cases unexercised in this round. Strict all-target clippy,
formatting and the lock gate passed. The first compile exposed a lifetime-general
function-pointer test pin that did not type-check. Its corrected generic pin compiled
before the successful suite and mutations. No production behavior changed for that
test correction.

Both new behavioral mutations compiled and failed their intended assertions. Erasing
the supervisor's stored bound failed the retained-refusal regression. Bypassing it
in the production run_in_home reap closure failed the real delegated production
fixture. Both exact restorations passed their focused reruns, including another
30.04-second production run. The restored tools.rs SHA-256 is
6ad472a96b28a604b3cdb159e3f59308d7f5b6f33ca8b16981fd160a6cc999fa,
matching both mutation records. The constructor/wait type pins remain separate from
these two compiled behavioral mutations.

The new fixture proves production reaping retains the original delivery bound with
an unsent OutOfOrder response. It does not exercise every terminal-delivery route,
every inherited containment fixture or production workload qualification. The earlier
84-test/twelve-mutation and 85-test/G9 results above remain historical source evidence,
not extra current-suite passes. Reporter subprocess coverage retains its earlier
scope, with the serve-side reporter routing source-reviewed.

This round's logs, mutation results and source hashes are staged in
/tmp/weaver-146-evidence/gate/review3. The mutation recipes are
/tmp/weaver-gate-g9b-mutations.json. source-sha256.json records the tested gate source,
tests, manifests and lock/toolchain inputs plus this evidence note. The same evidence
is deposited under gate/review3 in the shared weaver-testing deposit named above.
These are bounded author checks, not a new Planner grade, an operator merge decision
or completion of issue #146.
