# Refusal author custody through cancellation

Date: 2026-10-26. Author working record for issue #146 and PR #352.
Branch: code/state-refusal-author-composition.
Stack base: #351 at 45cf1dc3f0c6eeaf6b83b6e0914decfbfbaa0217.
Review correction parent: 27fe560f71e0d7dcb9aab78ad0ffd781ab5ebc22.

Held wind-down and emission refusal authors retain their exact unadmitted event
through repeated inspection holds, Stop and Unload. When the earlier inspection
already holds a completed answer, the author returns to its caller so the
inspection owner can take that answer. It does not sleep waiting for unrelated
transport traffic. Only Pending from the common reducer calls the blocking pump.

Implements: harness amendment HARNESS-RECORD-LIFETIME,
HARNESS-HELD-BOUNDARY, HARNESS-CLOCK, HARNESS-HALT and HARNESS-HALT-BOUND;
harness Spec section 6 wind-down and force joining a pending Leave.

| Section | Author assessment at the exercised boundary |
| --- | --- |
| HARNESS-RECORD-LIFETIME | Conforms: only the matching caller regains its original event. An admitted author uses its existing ticket and actual receipt. |
| HARNESS-HELD-BOUNDARY | Conforms for the exercised inspection holds. Permanent holds remain #351 B1 and #352 B1 at all three callers, including the two refusal sites. |
| HARNESS-CLOCK | Conforms for these callers: retain the original Decision bound, reduce actual local work, wait on Pending, and return on Idle or Progress. Tests supply no wake for a ready inspection completion. |
| HARNESS-HALT / HARNESS-HALT-BOUND | Conforms for unchanged first-fatal and returned-original handling. No reclamation clears a fatal cause or grants a new halt allowance. |
| Harness Spec section 6 | Conforms at the exercised force boundary: one owed refusal precedes wind-down closure, including force before or after the author completes. |

These are author assessments for regrade. Foundation remains unselected and the
production worker still calls dev_loop::drive.

## Implementation and caller census

| Every production reclaim_held caller | Original and continuation |
| --- | --- |
| context_worker_emission.rs, RefusalAuthor | Keeps the exact refusal through inspection holds, Stop and Unload. Recorded continues to Refused or CancelAccount. BeginOutbound already armed Decision on production paths. |
| context_worker_wind_down.rs, record_refusal | Keeps the original refusal through holds and force. Its caller already armed Decision in Instruction, Filing or force. No instruction, placement or generation is admitted by reclaiming it. |
| context_worker_wind_down_cancel.rs, record_turn | Keeps the exact unadmitted start/close. It uses the same Pending-only wait and returns on Idle so the earlier owner can consume its ready completion. |

Each caller resets the unadmitted started flag before retrying ordinary admission
on a later step. The two decision calls preserve the bound already armed by their
production caller; comments now state this dependency (#352 B2). A direct-stage
unit test can arm an absent bound, which does not establish an extra production
protection. Fatal reclamation failures still use retain_started.

No wire, public type, deadline, dependency, overflow queue or dispatcher selection
changes. Returning Idle does not create a progress owner for a permanent hold.
#352 B3's repeated event construction after a retained stopped author remains a
carry. #351 B1, B3-B5 and remaining author migration remain open before activation.

## Verification

Pinned nightly-2026-02-13, --locked, private target
/tmp/weaver-state-146-held-leave-target, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4. The ordinary harness gate used RUST_TEST_THREADS=4;
delegated composition runs were serial. No source edit occurred during a gate.

- Full cargo test -p weaver-harness --locked: 2,108 passed, including 38
  doctests, with 702 ignored by that invocation. This sums top-level binary and
  doctest results, without counting their nested child-mode summaries again.
- Five complete repetitions each passed all 222 selected cases from
  worker_session_, worker_graceful_, worker_emission_ and worker_generation_:
  1,110 successful case executions on identical source. The case reported
  intermittent in #352 F2 passed in every repetition.
- The repetitions include the five privileged harness joint scenarios and
  their actual admin participant. The logs retain the participant's own result.
  They exercise normal SQLite capture, publication and Leave in isolated
  real-root delegated scopes. Model/classifier peers remain declared doubles.
- All seven held-author SQLite conversion cases also passed as a targeted
  baseline after the runtime correction. They are included in the repeated
  selection and are not counted again as distinct coverage.
- Three compiled protective controls independently restored waiting on Idle
  in each runtime caller. Every control failed its named test at the intended
  assertion. Each original was restored and the library rebuilt before the
  final gates. The 1,025-file source manifest remained unchanged through them.
- Strict harness clippy for all targets, formatting, diff and lock gates passed.

The emission fixture uses actual inspection exchanges and recorder receipts. It
retains exact raw bytes and allocation identity through three original owners,
then records one refusal. It covers no cancellation, Stop and Unload. Once the
inspection result is complete, it schedules no descriptor wake and verifies that
the caller returns without advancing the fake clock or spending Decision.

The wind-down cases consume the normal member's actual inspection reply. A
probe rejects a blocking wait while that completion is ready. It delegates only
nonblocking polls to the real clock and invents no readiness. The original owner
then takes its completion before Decision, with the original bound unchanged.
Start, close and refusal cases preserve their original custody through subsequent
force, Lower, capture, release and Left.

Before the runtime correction, the strengthened emission fixture reached
DecisionBound with its inspection answer already complete. Both held-refusal
cases and both held-close cases failed the no-blocking-wait assertion. The old
fixtures had supplied a wake and tested only that a wait was requested. Those
assertions have been replaced with the progress property required by the Spec.

The normal SQLite member was already built in the private target. The existing
actual admin test executable was supplied through WEAVER_LEAVE_ADMIN_TEST_BIN,
WEAVER_STOP_ADMIN_TEST_BIN and WEAVER_OBSERVE_ADMIN_TEST_BIN. Their binary hashes
are in prerequisites.json. The actual-admin tests ran with sudo systemd-run
--scope -p Delegate=yes, not only a user namespace. No new admin or GPU crate
gate is claimed. The eight separate admin-startup cases were not run.

## Evidence and remaining work

Verified deposit:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-ready-owner-20261026-ad43ecee/.

Source archive SHA256:
`ad43ecee77fdbfe1c064e4b442074722440a702590c6ac3c7c5152d43da6b259`.
Source manifest SHA256:
`4c7e7a73eb3a5cff343716dafaa4d2a1b4b92fd0198e3e246f8c8d9a0a3445c7`.

The archive includes 1,025 tracked files and excludes this working record, which
was rewritten after the gates. The deposit contains the record separately,
initial failing fixtures, compiled controls and their originals, all five
repeated runs, exact case inventory, gate commands and manual author review.

This correction answers #352 F1 and F2 as author work pending Planner regrade.
It also answers B2's comment request and explicitly carries B1 and B3. Named
code-review and verification-before-completion skills remain unavailable; the
manual reading is not their invocation or a Planner/security grade.

#339's separate correction at 516079a0 has a new G1 blocker concerning mandatory
terminal progress when accounts are unavailable. #350's fix findings and design
questions also remain. #348/#349 retain their operator, review and merge gates.
#146 stays open for those carries, on-demand save, complete lifecycle integration,
qualified artifacts, measured limits, security and coupled cutover. This act
performs no merge, deployment or CodeRabbit review request.

The failure in the earlier tests was choosing the wait request as the property
instead of progress by the completion's owner. Future held-author instruments
must first establish that the result is complete, supply no unrelated wake, and
prove that its original owner can consume it before the unchanged bound.
