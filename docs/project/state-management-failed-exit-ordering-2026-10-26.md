# Failed serving exit trace ordering

Date: 2026-10-26.
Parent: #339 at e9b6280a03c9a909174b556515fcdbd35648b6a2, including the
operator's #340-#345 folds.
Branch: code/state-failed-exit-composition.
Tracker: #146, lifecycle composition and failed-run teardown; #339 Planner F1.

Failed serving exit settles owed turn accounts before closing their turn. It
retains an already prepared closing event and its exact author ticket while
earlier accounts use ordinary writer capacity. SessionClosed is attempted after
gate retirement and remaining accounts, immediately before Unload. This answers
the three sites of Planner F1 without permitting records after SessionClosed.

Implements: harness amendment HARNESS-HALT, HARNESS-HALT-BOUND,
HARNESS-HALT-ACCOUNT and stopped HARNESS-OPEN accounting; Spec 05 PAIR-MATCH,
PAIR-END and PAIR-LEAVE; trace amendment TRACE-CAPACITY and
TRACE-REJECTED-RETURN. The author assessment is conforms for the tested failed
serving route. Planner regrading remains required, including the attribution
interpretation below. No Spec or wire change is proposed by this correction.

## Ordering and custody

| Boundary | Handling |
| --- | --- |
| Before TurnClosed | Drive existing stopped placement, generation, measurement/refusal and ordinary author accounts. Wait through the existing capacity/decode driver under the original halt cap. No placement grant, model request or successful continuation is produced. |
| Pending TurnClosed | Move the exact prepared event and ticket into the failed closer, settle earlier accounts and ToolCallCompleted, then promote that original through its existing reservation. Preserve its original payload and cause. |
| Pending SessionClosed | Keep the original in the shared failed tail until gate retirement and remaining accounts complete. Promote it immediately before Unload. |
| Placement-owned close | Leave its ticket with the dedicated placement driver; the generic closer does not detach that claim. |
| Late return first observed after TurnClosed | Author run-scoped rejected-return evidence before SessionClosed. Retain original CallId, exchange, causal parent and exact raw payload. Earlier pending rejection authors keep the turn chosen with their original event. |
| After SessionClosed | Attempt Unload in the same terminal stage. Subsequent work observes children and drains; it authors no additional event. |

The trace amendment's event table already makes the rejected-return turn
optional. PAIR-MATCH requires the closed exchange/CallId mapping and exact raw
evidence through Lower and reap. This implementation interprets a newly observed
late return as run-scoped cleanup evidence while keeping those identities. It
does not reopen the closed turn or create a post-session exception. The Planner
must grade this reading; it is not a new operator ruling.

HARNESS-HALT's turn-close, stopped world response and Lower order is retained.
Boot-only HARNESS-OPEN accounting retains its existing pre-input path; this
serving correction does not move it ahead of mandatory terminal work. The first
failure and original teardown cap remain authoritative.

## Changed surface and callers

| Site | New surface and consumers |
| --- | --- |
| RecordBarrier::defer_halt_close / restore_halt_close | A private move-owned carrier holds one original prepared close. Restore requires the same recorder, stopped tap and an empty ordinary slot; refusal returns the complete original. Only the failed closer and its context resume helper consume it. |
| RecordingContext::resume_deferred_halt_close | Restores and resumes the original reserved ticket, binds actual Recorded turn state, and retains an unrecorded original. Called by the turn closer and shared serving-session closer. |
| advance_halt_trace | Reporting cannot opportunistically admit TurnClosed or SessionClosed, even when ordinary capacity returns. The consuming closer owns their order. Existing boot, placement, measurement and halt consumers retain their admission rules. |
| FoundationFailedExit::terminals / wait_with_io | Drains stopped accounts before closure; its wait uses the shared tail's actual decode/capacity interests and original cap. |
| FailedRunCleanup | Serving-only session-close state is consumed at Unload. Pre-input constructors create no SessionClosed. The same account predicate gates pre-turn closure and final session closure. |
| advance_place_account | Updates the recorder's owed-placement flag after each actual stopped step. A final author could previously finish the owner while leaving the flag set, preventing Unload; the new real-placement test exposed this defect. |
| RejectedAuthor | Both healthy pair recording and failed rejection recording retain the turn chosen with their original event. Neither recomputes attribution when acknowledging a delayed receipt. |
| PairLedger::ack_halt_rejection_recorded | The failed rejection driver is its sole runtime caller. Only a frozen ledger accepts a run-scoped receipt. Healthy turn matching, run/block/sequence checks, uniqueness and original packet custody remain enforced. |

## Author evidence

Pinned nightly-2026-02-13; private target
`/tmp/weaver-state-146-held-leave-target`; CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, --locked on dependency-resolving Cargo commands. Lock ran
before Cargo. The final strict harness clippy, formatting and diff checks passed.

- Full `cargo test -p weaver-harness --locked`: 2,061 passed, including 26
  doctests; 567 ignored in that ordinary invocation. No #331 exclusion was used.
- The separate delegated worker invocation exercised 503 of those ignores.
  Its first run passed 502 and failed Observe setup because the required
  admin-test executable variable was absent. That exact Observe case passed
  with WEAVER_OBSERVE_ADMIN_TEST_BIN set. All 503 selected cases have passing
  results; the other 64 ordinary ignores were not exercised by this gate.
- Seven compiled protective controls failed their named assertions: omitted
  pre-close account settlement, early SessionClosed, early TurnClosed,
  closed-turn attribution for a late return, stale placement flag, foreign
  recorder accepting a deferred ticket, and live ledger accepting a run-scoped
  rejection receipt. Every source was restored and the library rebuilt.
- Strict lint then requested `expect_err` instead of `err().expect()` in the
  new ticket test. This test-only edit leaves all runtime bytes unchanged.
  Clippy and formatting passed after it. The rebuilt exact test passed on the
  host; its sandbox attempt was denied local socket creation before reaching
  the behavior. The full suite and controls above precede only this assertion
  spelling change and this record.
- All 969 files in the final source manifest match the final worktree bytes.
  This record was added after the source archive and is not in its test claim.

The process cases use actual worker, state, gate, decoder and classifier seams
with scripted peers. The placement case waits for a real MeasureDelta reply,
holds the resulting StatePlacement author behind writer pressure, and injects
actual state EOF from the capacity wait. It asserts no turn-T event follows
TurnClosed(T), nothing except Unload follows SessionClosed, and no decoder append
or new dispatch occurs. Pending-close fixtures queue the original close through
the real barrier; they assert its original reason and uid 42 survive. They do
not claim every healthy workflow can create that interleaving.

The first placement setup delivered EOF before the pending author existed. The
next setup reached the author and exposed the stale placement flag. A test-helper
visibility error and a duplicate trace drain in the ticket fixture were also
corrected. These failures, the lint failure, missing Observe variable and sandbox
refusal are preserved, and are not counted as protective-control detections.

The existing private-target SQLite member and admin test executable are recorded
by hash in prerequisites.json. They were not rebuilt for a fresh admin or SPU
qualification claim. The eight privileged admin-startup cases were not run.

Evidence deposit:
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-failed-exit-ordering-20261026-b7f32ad8/`.

Final source archive SHA256:
`b7f32ad847441fab50956e5db2062ad43f6377bc2801affd0e8491c2f3667a11`.
Final source manifest SHA256:
`b94f99748c7b9ee62f3f56e68ebfa61cedad046fe5aec80f6ed080c5c471155f`.
The full-suite/control source archive before the assertion spelling change is
preserved as pre-lint-source.tar.gz, SHA256:
`15d5c929d5db9c068b0374e27495a479a366eac626e447d6aa8ddbea8b005b90`.

## Remaining work and after-action

This is an author correction, not a Planner, third-party, security or production
grade. The named code-review and verification-before-completion skills remain
unavailable; the evidence contains a manual author reading and executable checks.
#339 stays draft pending regrade. Its F2 and B1-B7 are not collectively closed:
gate reaping, Lower's sub-bound, management world-send identity, rejection
unavailability, shared-tail design and abandon-at-cap coverage retain their
recorded dispositions. All folded #340-#345 batch/file and Spec questions carry.

The plan was to reproduce all three closing-order sites and settle their original
owners before closing. The real placement fixture additionally exposed the stale
recorder flag, so the correction includes that synchronization. Future fixtures
should assert the actual pending author before injecting failure, supply every
required peer executable, and retain the complete ordered trace rather than
checking event counts alone.

Foundation remains unselected and production still calls dev_loop::drive.
#146 remains open for integration, remaining review carries, authority proposals,
qualified artifacts, measured limits, security and the operator's coupled cutover.
No CodeRabbit review, merge or deployment is part of this act.
