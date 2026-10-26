# Failed serving exit ordering and terminal progress

Date: 2026-10-26.
Parent: #339 at e01611d23f3bd0f72d83adfc759d7610fe99eaa2, including the
operator's #340-#345 folds and the F1/G1 corrections.
Branch: code/state-failed-exit-composition.
Tracker: #146, lifecycle composition and failed-run teardown; #339 Planner H1.

A rejected tool return first observed after TurnClosed keeps its run-scoped
author through ordinary trace pressure. Lower and the remaining cleanup stages
retry the same packet and ticket. Unload makes the final attempt, then retains
any unfinished original before SessionClosed. Only Unload may follow that close.
An actual unavailable driver still latches once. The first failure, frozen halt
account and original cap remain unchanged.

Lower drains capacity notifications before attempting admission. A notification
that arrives between that attempt and the wait remains readable. The wait keeps
the original halt deadline and existing capacity descriptor. No timeout, event
kind, wire shape, overflow queue or new admission authority is added.

Implements: harness amendment HARNESS-HALT, HARNESS-HALT-BOUND,
HARNESS-HALT-ACCOUNT and stopped HARNESS-OPEN accounting; Spec 05 PAIR-MATCH,
PAIR-END and PAIR-LEAVE; trace amendment TRACE-CAPACITY,
TRACE-REJECTED-RETURN and TRACE-WAKE. Author assessment: conforms for the tested
run-scoped return and bounded cleanup behavior, pending Planner regrade. The
earlier turn-scoped rejection under pressure remains Spec gap Q1; this act does
not change that author's turn or elect a serialized halt-account representation.

## Boundaries and consumers

| Site | Handling |
| --- | --- |
| FoundationFailedExit::terminals | Ready turn accounts get local steps; blocked or unavailable accounts cannot delay the stopped response and Lower. Retained turn-T originals do not restart after TurnClosed(T); Q1 governs their remaining disposition. |
| FailedRunCleanup::advance_serving_rejection | Ok(None) means keep the exact packet, claim and ticket eligible for retry. Recorded clears that exact rejection through its existing receipt. Actual unavailability retains the original and latches once. |
| FoundationFailedExit::lower | Retry a pending rejection before consuming another ToolAnswer, and attempt each newly observed rejection. A second packet waits on the first owner's capacity. A permanently unavailable first owner retains later packets through the existing received owner, with allocation preflight. |
| FoundationFailedExit::wait_with_io | Lower registers capacity without draining after admission. The preceding Lower step drains before its retry. Original deadline and matching GateStopped/empty-tools checks remain. |
| FailedRunCleanup::advance_accounts | After turn-account retirement, Accounts, Release, Retire, Reap and Unload each offer a run-scoped rejection another attempt. An ordinary capacity wait does not hold these tail stages. Pre-input callers retain their existing continuation. |
| FailedRunCleanup::close_serving_session | Its sole caller is Unload, after the final account step. Retain any still-unrecorded rejection before either a deferred or fresh SessionClosed. Drain and Closed do not author another account. |
| RejectedAuthor / RecordBarrier | Existing exact-ticket retention owns any unrecorded prepared event, purpose and author. The original raw packet and claim remain held. No receipt, placement, filing or successful continuation is invented. |
| Placement, generation, summary and deferred closes | G1 behavior remains: original reservations and incomplete evidence stay owned, ready accounts precede closing, and prepared closes retain their payload, cause, ticket and timestamp. |

The new active Lower capacity path had a lost-wake defect: its wait drained a
notification that arrived after the failed admission. The second-return fixture
reproduced that race before the drain moved. Later shared-tail waits retain their
existing 10 ms wake capped by the original deadline; this act changes no timer.

## Author evidence

Pinned nightly-2026-02-13, fresh private target
/tmp/weaver-state-146-run-rejection-target, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, and --locked. Lock ran before Cargo. Cargo chains and Git
mutations were serial; source stayed unchanged during each gate.

- Full cargo test -p weaver-harness --locked: 2,061 passed, including
  26 doctests, with 575 ignored by the ordinary invocation.
  Counts use the final result per binary plus both doctest groups, excluding
  nested child summaries. The #331 case was included and passed.
- All 511 inventoried worker cases passed in one serial delegated
  invocation, including the three new Lower pressure cases. The remaining 64
  ordinary ignores were not run by that invocation. SQLite member and actual
  admin test binaries were rebuilt in the same private target and hashed.
- Strict harness clippy for all targets, formatting, diff and final lock checks
  passed. All 969 source-manifest hashes matched after controls and gates.
- Four separately compiled controls failed their intended checks: abandoning at
  first pressure, leaving the rejection author active through session closure,
  skipping tail retries, and consuming the capacity wake after admission. Every
  source was restored and rebuilt. The second control failed at the retirement
  check before the checker's initially expected timeout; both results and that
  marker correction are preserved without rerunning already passing gates.

The three new cases use actual organ packets and a physically full trace writer:
capacity returns after Lower/Release; it remains unavailable through session
closure; and a second ToolAnswer waits while capacity returns between the first
admission attempt and poll. They check the unchanged ticket and raw packet across
retries, one run-scoped record per late return when capacity returns, exact raw
String bytes, the first failure and original deadline. Trace assertions forbid
turn-T records after TurnClosed(T) and any non-Unload record after SessionClosed.

The writer-fill helper exists only in recording_tests. It submits run-scoped
fixture evidence directly to the stopped writer because the ordinary harness
author correctly refuses fresh work once retention is frozen. It creates actual
writer pressure without adding a runtime author bypass. The wake probe polls the
real capacity descriptor and checks the unchanged deadline; scripted peers and
the injected clock do not qualify actual model execution or production limits.

Both original retention baselines fail the intended assertion. The separate
lost-wake baseline also fails its intended readiness assertion. Initial fixture
compile/setup failures and the unrelated gate-poll flag correction remain in the
logs and are not counted as protective mutations.

Evidence deposit:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-run-rejection-20261026-354bac9d/.
Source archive SHA256:
354bac9d888dbbb7dfe5ef5929f527e109d9e040b37c033e721479e99d306f67.
Source manifest SHA256:
c6ffb33fd3770800137d6f7615a797fc633731487ca8f266f25b600b6c3bd654.
This working record is revised after gates and is excluded from that source
manifest. It is included separately in the evidence deposit.

G1's eight-control evidence remains at the immutable
state-terminal-progress-20261026-7bc699ae deposit named in the
[parent record](https://github.com/toddwbucy/WeaverAgent/blob/e01611d23f3bd0f72d83adfc759d7610fe99eaa2/docs/project/state-management-failed-exit-ordering-2026-10-26.md).

## Review disposition and remaining work

The [Planner regrade](https://github.com/toddwbucy/WeaverAgent/pull/339#pullrequestreview-5659530643)
accepts G1 and identifies H1, four batch rows, one file row and Q1. H1 has an
author correction here. The Lower wait that B2 called dead is active and tested.
Q1 still needs a concrete Spec proposal and the operator's election for an earlier
turn-T rejection that cannot be Recorded before mandatory Closing. No such
re-attribution or authority change is implemented here.

B1's incomplete serialized accounts, B3's surviving dead-trace short-circuit
control, B4's unpinned outstanding-grant revocation and L1's placement-owned
TurnClosed custody remain carried. These tests do not relabel those controls as
caught. Earlier #339 and folded batch/file findings and elections remain,
including gate reaping, Lower's sub-bound and management world-send identity.
The #346/#347 descendants need integration checking when they join this parent.

The required named review skills remain unavailable. Manual author readings and
executable checks are recorded without claiming those invocations, a Planner
grade, security disposition or production qualification. The eight privileged
admin-startup cases and GPU qualification were not run.

Foundation remains unselected and production still calls dev_loop::drive.
#339 remains draft. #146 remains open for integration, review carries, authority
proposals, qualified artifacts, measured limits, security and operator cutover.
No CodeRabbit review, merge or deployment is part of this act.
