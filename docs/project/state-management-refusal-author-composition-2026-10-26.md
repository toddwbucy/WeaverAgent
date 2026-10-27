# Refusal author custody through cancellation

Date: 2026-10-26. Author working record for issue #146 and PR #352.
Branch: code/state-refusal-author-composition.
Stack base: #351 at 45cf1dc3f0c6eeaf6b83b6e0914decfbfbaa0217.
Review correction parent: 32594414936cf34ae7b61dcad5158bf8c1b2a6e0.

Held wind-down and emission refusal authors retain their exact unadmitted event
through inspection holds, Stop and Unload. If an earlier inspection has a ready
answer, the author returns so its owner can take that answer. Only Pending from
the common reducer requests a blocking pump wait. The Planner accepted this F1
correction at the parent in review 5659980022.

This revision addresses that review's remaining F2 timing fixture. It changes
two test-only files and this record. Runtime behavior is unchanged. Foundation
remains unselected, and production still calls dev_loop::drive.

Implements: harness amendment HARNESS-RECORD-LIFETIME,
HARNESS-HELD-BOUNDARY, HARNESS-CLOCK, HARNESS-HALT and HARNESS-HALT-BOUND,
and harness Spec section 6 wind-down and force joining a pending Leave.

| Section | Disposition at the exercised boundary |
| --- | --- |
| HARNESS-RECORD-LIFETIME | F1 accepted at the parent: exact original custody, existing admission ticket and actual receipt. |
| HARNESS-HELD-BOUNDARY | Accepted for the exercised inspection holds. Permanent holds remain #351 B1 / #352 B1, including the bounded busy loop on Idle. |
| HARNESS-CLOCK | F1 accepted for all three callers. F2 fixture correction is author work for regrade: declared preparation margin, original bounds preserved and actual expiry diagnostics. |
| HARNESS-HALT / HALT-BOUND | Unchanged accepted handling of the first fatal cause and returned original. No new allowance. |
| Harness Spec section 6 | Accepted on the exercised force paths. This revision changes no runtime deadline or force policy. |

## Implementation and caller census

| Every production reclaim_held caller | Original and continuation |
| --- | --- |
| context_worker_emission.rs, RefusalAuthor | Keeps its refusal through inspection holds, Stop and Unload. Recorded continues to Refused or CancelAccount. BeginOutbound already armed Decision. |
| context_worker_wind_down.rs, record_refusal | Keeps its refusal through holds and force. Instruction, Filing or force already armed Decision. Reclamation admits no instruction, placement or generation. |
| context_worker_wind_down_cancel.rs, record_turn | Keeps the unadmitted start/close. Only Pending waits. Idle returns to the owner of an already completed inspection answer. |

All three callers reset the unadmitted started flag before later ordinary
admission. Fatal reclamation still uses retain_started. B2's comments name the
already-armed Decision caller. B3's repeated event construction after a retained
stopped author remains carried. A permanent hold needs an owner that can progress
or a refusal, which this act does not supply. The Planner's T4 control, removing
the Pending wait, remains undetected by the current runtime tests.

## F2 diagnosis and correction

The author_wait fixture fills the real trace queue after WindDown has begun,
then sends Observe from the first blocking author wait. With diagnostics and its
old five-second allowance, 20 individual runs and one 487-case family run passed.
Natural failure was not reproduced locally. Queue preparation consumed about
3.9 seconds, leaving about 1.1 seconds at Observe for management and the retained
author to complete.

A controlled two-second delay before queue preparation failed at the reported
ManagementPending assertion in all three runs on #351's 45cf1dc3 and all three
on #352's 32594414. Both converted to LeavePending because the original WindDown
bound had expired at the last real clock read, before any Observe was sent. This
establishes a shared fixture timing sensitivity. It does not establish that
every failure in the Planner's run had that cause.

The runtime record_turn body at 32594414 matches the base, apart from its
comment. The other additions to that file are test-only refusal accessors.
Thus the base's 0/15 result does not establish absence of this race. Scheduling
margin is a plausible explanation for that difference, not a measured cause of
the Planner's individual failures.

Only the -wind-down-author-wait fixture now declares 15 seconds through the
normal Enter settings. It gives queue preparation and Observe enough margin
without resetting the accepted bound. Expiry and delayed-join fixtures still
declare one second, other ordinary fixtures five, and unbounded fixtures None.
Decision remains five seconds. The actual original-bound assertion remains.

Both failing assertions now report the actual clock, last context clock,
original WindDown bound, signed remaining time, expiry at the last read,
armed and expired deadline keys, force state, stage, detail, Observe send time
and last pump wait. Successful checkpoints retain these values in the log too.
The same two-second preparation control passed three times with the new
allowance. The controlled test files in both worktrees were restored byte for
byte before the final build. No settling sleep ships in this correction.

## Verification

Pinned nightly-2026-02-13, --locked, private target
/tmp/weaver-state-146-wind-down-observe-target, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4. The ordinary harness gate used RUST_TEST_THREADS=4.
Composition runs were serial in isolated real-root delegated scopes. No source
edit or second Cargo chain overlapped the gates.

- Full harness: 2,108 top-level tests/doctests passed, with 702 ignored.
  The total is 2,027 library, 50 binary/integration and 31 doctests. Nested
  child-mode summaries are excluded. This corrects the earlier 38-doctest count.
- All 15 repetitions of the Planner's composition filter set passed all 487
  inventoried cases: 7,305 successful case executions, including author_wait
  in every run. The filters and exact inventory are retained with the logs.
- The selection includes the actual admin/SQLite joint participants, with the
  normal state member and admin executable built in this fresh target. Their
  hashes are retained. Model/classifier peers remain declared doubles.
- Strict harness clippy for all targets, formatting, diff and first/last lock
  checks passed. All 1,025 tested file hashes match after the gates.
- The three F1 controls and no-wake custody fixtures remain accepted from the
  parent. Their original evidence remains in the state-ready-owner deposit.
  The delay controls above are fixture diagnosis, not new runtime protection.

The initial diagnostic driver was interrupted after its 20 individual probes
while its first compiled family run continued. That test process finished with
487 passed before any source edit or rebuild. The driver has no successful
overall exit, so only the individual results and completed family log are
claimed. The restored final chain completed normally, including all 15 repeats.

## Evidence and remaining work

Source archive SHA256:
`4b96126b6194f463e6d031eba8f335f1e9a574736a246911118a5bc304e7ad52`.
Source manifest SHA256:
`9a1ded52e32d88c3506de3f869e101b5bcf8d81b456638d3de01d6f99f34a5a9`.

Evidence destination:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-wind-down-observe-20261026-4b96126b/.

Accepted F1 evidence remains at:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-ready-owner-20261026-ad43ecee/.

The archive contains 1,025 tracked files and excludes this record, rewritten
after the gates and retained separately. The deposit carries diagnostic probes,
both original-head controls, the fixed control, restored verification, exact
commands, inventories, binary hashes, source originals and this manual review.
The publication receipt records the verified deposit and commit separately.

F1 and B2 are accepted at the parent. F2's correction awaits Planner regrade.
B1, B3, T4 and #351's other carries remain. #339's G1/H1/wake corrections are
accepted, while its Q1 and other carries remain. #353 joins the reviewed rollback
stack into #332 and retains its B1/L1. #350's F2/J1 are accepted, while its
F1/Q1 and #348/#349 policy elections still block the corresponding work.

The named code-review and verification-before-completion skills remain
unavailable. Manual review is not their invocation, a security pass or
production qualification. No GPU gate, whole-workspace gate or separate
privileged admin-startup suite is claimed. #146 remains open for lifecycle,
on-demand capture, author migration, qualified artifacts, measured limits,
selection and coupled cutover. This act performs no merge or deployment.

After-action: a real-clock fixture must budget its setup as well as its asserted
exchange. A passing comparison run does not exclude a shared timing race.
Expiry diagnostics and a controlled delay on both heads distinguish that
sensitivity without claiming an unobserved runtime regression.
