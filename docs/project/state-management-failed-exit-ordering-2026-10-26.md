# Failed serving exit ordering and terminal progress

Date: 2026-10-26.
Parent: #339 at 516079a0e18a5dc6bddaa4e8acb9d53d650d38f9, including the
operator's #340-#345 folds and the F1 ordering correction.
Branch: code/state-failed-exit-composition.
Tracker: #146, lifecycle composition and failed-run teardown; #339 Planner G1.

Failed exit records ready turn accounts before TurnClosed. If an account cannot
progress without ordinary capacity or an unavailable exchange, cleanup retains
its exact original as incomplete evidence and proceeds with the stopped response
and Lower. It never resumes that turn account after closing. SessionClosed still
precedes only Unload. The first failure, frozen account and original halt cap stay
unchanged; no new timeout, Spec field, wire shape or terminal reserve is added.

Implements: harness amendment HARNESS-HALT, HARNESS-HALT-BOUND,
HARNESS-HALT-ACCOUNT and stopped HARNESS-OPEN accounting; Spec 05 PAIR-MATCH,
PAIR-END and PAIR-LEAVE; trace amendment TRACE-CAPACITY and
TRACE-REJECTED-RETURN. Author assessment: conforms on the tested failed serving
route, pending Planner regrade. The Planner accepted F1's ready-account ordering
and run-scoped late-return interpretation at the parent. G1 corrects the wait
that could prevent mandatory Closing when one of those accounts cannot finish.

## Boundaries and consumers

| Site | Handling |
| --- | --- |
| FoundationFailedExit::terminals | The sole consumer of settle_serving_turn_accounts. Real local account progress returns Progress without waiting. Unavailable or blocked work is retained before reserved closing attempts. |
| FailedRunCleanup::advance_accounts | Pre-input callers keep their existing continuation and cap. Its internal progress result is used only by the serving closer. Once serving turn accounts are retired, generic ordinary account drivers cannot resume them. |
| RecordBarrier::retain_halt_record | With a stopped tap, move the one exact prepared Event, purpose and AuthorTicket into retained evidence. Do not abandon the recorder or spend a terminal reserve. Preserve the frozen first snapshot even if this attempt began later. |
| Placement, measurement and summary | Existing owners retain requests, raw replies, grants and incomplete dispositions. Revoke the outstanding placement identity. Retain the stopped summary read and account; install no journal or plan success. |
| RejectedAuthor | Keep the original ledger packet, claim, any receipt and any unavailable prepared record. Consume the handoff's rejection-unavailable latch. No later close restarts that author. |
| Lower | When an unavailable rejection already owns the ledger slot, retain a later actual ToolAnswer in the existing received owner after allocation preflight. Continue to the matching GateStopped; never fabricate a rejection receipt or replace the earlier packet. |
| Newly observed post-turn return | Attempt the existing run-scoped rejection once while capacity permits; otherwise retain it incomplete and continue shutdown. Preserve CallId, exchange, causal parent and exact raw bytes. |
| Stopped generation and cut | An unanswered exchange returns Pending. A received Generated terminal returns Progress even while Cancel still owes AtRest, allowing its ready records to be accounted. Original reservations and bounds remain owned. |
| Deferred closes | Preserve the exact original event, payload, cause and ticket. Preserve its preparation timestamp; trace sequence orders it after earlier accounts. |
| Session close and Unload | The obsolete serving account-wait predicate at Unload is removed. Accounts have settled or been retained incomplete. Only Unload follows SessionClosed; later steps observe children and drain without authoring. |

The public wait method already returns immediately unless progress is Pending,
and the worker uses that same condition. Local Terminals progress cannot enter
its capacity-wait branch. A dead trace may still abort the later release/drain
wait; the regression requires the stopped response and actual Lower first.

## Author evidence

Pinned nightly-2026-02-13, private target
/tmp/weaver-state-146-held-leave-target, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, and --locked. Lock ran before Cargo. Cargo chains and Git
mutations were serial; source stayed unchanged during each gate.

- Full cargo test -p weaver-harness --locked: 2,061 passed, including 26
  doctests, with 572 ignored by the ordinary invocation. Counts use the final
  result per test binary plus both doctest groups, excluding nested child
  summaries. The #331 case was included and passed.
- All 508 inventoried worker cases passed in one serial delegated invocation,
  including the five new cases. The other 64 ordinary ignores were not run by
  this gate. Stop and Observe used the recorded actual admin test executable.
- Strict harness clippy for all targets, formatting, diff and final lock checks
  passed. All 969 source-manifest hashes matched after the controls and gates.
- Eight separately compiled controls failed their intended assertions: waiting
  before mandatory close, losing the exact retained original, skipping a ready
  placement, blocking Lower behind a later packet, falsely reporting progress
  for unanswered generation, falsely reporting Pending for a received terminal,
  falsely reporting progress for an unanswered cut, and leaving the placement
  Unload guard stale. Each original was restored and the library rebuilt.

The dead-trace regression has an actual sent, unanswered MeasureDelta, actual
state EOF, a real writer failure and the original cleanup cap. The fully answered
placement already abandoned on sink failure at the parent; that passing baseline
is retained separately. The unanswered version reproduces G1 before the fix.

The unavailable-rejection regression injects an invalid private turn receipt and
executes the real InvalidIdentity refusal. It declares that internal fault
instrument rather than claiming an ordinary peer can create the private state.
The second case adds an actual late ToolAnswer and checks its retained raw bytes
and matching Lower. Ready TurnStarted/placement cases record before closing;
blocked versions retain their exact tickets without waiting. An unrecorded
TurnStarted creates no invented turn bracket. Ordered trace assertions forbid
turn-T records after TurnClosed(T) and any non-Unload record after SessionClosed.

Logs preserve the first test-only type/visibility compile failures, the initial
passing answered-placement probe, both failing G1 baselines, the release-wait
fixture correction, the later-packet failure and the corrected Cargo filter
syntax. None of these setup/rework failures counts as a protective mutation.

Existing SQLite member and admin binaries are identified in prerequisites.json.
They were not rebuilt for a fresh admin, SPU or deployment qualification claim.
The eight privileged admin-startup cases and GPU qualification were not run.

Evidence deposit:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-terminal-progress-20261026-7bc699ae/.
Source archive SHA256:
7bc699aee3b1a1c524cb557398a4f51dc346770d8b8a6d5e74278ba7627e6cd8.
Source manifest SHA256:
3dc96adb4a1201968de940080055ef90bb14af27bc2d768ecd90196881599475.
This working record is revised after the gates and is excluded from that source
manifest. It is included separately in the evidence deposit.

## Review disposition and remaining work

G1 has an author correction and executable evidence, not a Planner clearance.
The new review's B2 timestamp explanation is added. B3's old Unload predicate is
removed; its old mutation is not claimed to have begun detecting a regression.
The speculative refused-restore L1 is not collectively closed by these results.
Existing #339 and folded batch/file findings and operator elections remain
carried, including gate reaping, Lower's sub-bound, management world-send identity
and remaining failed-owner integration. The #346/#347 integration still needs
review when those descendants join this changed parent.

The required named review skills remain unavailable. Manual author readings and
executable checks are recorded without claiming those skill invocations, a
Planner grade, security disposition or production qualification.

Foundation remains unselected and production still calls dev_loop::drive.
#339 remains draft. #146 remains open for integration, review carries, authority
proposals, qualified artifacts, measured limits, security and operator cutover.
No CodeRabbit review, merge or deployment is part of this act.
