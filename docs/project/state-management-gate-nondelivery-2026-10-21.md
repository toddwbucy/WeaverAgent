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
