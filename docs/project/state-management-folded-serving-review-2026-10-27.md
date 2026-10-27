# Folded serving lifecycle review

Date filed: 2026-10-27. Author record for issue #146 and PR #332.
Branch: code/state-refusal-close-composition. Base: main.

The folded implementation retains the existing parent ancestry. This correction
answers the fix and immediate-correction rows of the Planner's review of #332 at
c4fafcdd, together with the operator's three design rulings. The foundation
dispatcher remains unselected. Do not deploy the incomplete replacement.

Review: https://github.com/toddwbucy/WeaverAgent/pull/332#pullrequestreview-5670951279

## Operator elections and implementation

- D1/D2: a converted Leave may atomically replace only its publisher's guarded
  pending index tail from graceful_unload to forced_unload. It verifies the exact
  guard, original prefix, tail and final report. Every trace event keeps the cause
  at its first submission. The existing implementation and paired text remain. The folded graceful-conversion
  process test now expects the final forced cause, matching that ruling.
- L3/X6: a timely validated successful Left, with the existing marker eligibility,
  completes the guarded save before run cleanup. Cleanup and marker-write errors
  still refuse Unloaded. They retain the completed save's identity and eligibility.
  If publication and cleanup both fail, retain the publication failure first and
  the cleanup failure separately. Closed, absent and different-run markers keep
  their existing no-publication rule; a joiner remains a verifier.
- Z3: timely validated Ready ends Enter. Open-marker durability remains necessary
  for Loaded. Slow marker work cannot expire an accepted Ready; failed marker work
  still rolls back. The original deadline governs receipt and validation.

ADMIN-SAVE-COMPLETION explicitly replaces the old publication-after-run-end clause
for replacement Serving only. ADMIN-ENTER-WINDOW and foundation
PERSIST-LEAVE-CAUSE/PUBLICATION-GUARD carry the corresponding paired text. The
harness/types amendment headers are 0.38/0.34; admin/foundation are 0.22/0.25.

## Owner and caller census

| Owner / caller | Behavior and evidence |
| --- | --- |
| FoundationInput::drive_with_io | A Pending organ observation waits through the common pump even when called through advance. Real descriptors and the original deadlines remain in that wait. The controlled non-ready fixture spans the full last-word interval with bounded steps. |
| FoundationEnteredRollback | Pending observation work returns Pending, so its driver waits. Both existing actual gate-EOF rollback cases assert that obligation before completing cleanup. |
| RecordingContext::pump_with_wait | An observed gate's closed-tail wake performs one bounded receive after real-source and deadline arbitration. EOF settles the debt. A malformed tail returns its retained refusal and raw evidence, without an input-specific driver. |
| ObservedOrgans / halt | The cached wake identifies its observation and is unavailable after that observation records or is abandoned. Halt retires the serving wake and uses its existing settlement clock and halt cap. |
| FoundationInput::retain_report | Gate and residency use the same one-slot intake. An exact held SPU Fault remains consumable after loss. A final packet still queued when decode EOF is observed also enters the report owner. Non-report originals stay on their own port. |
| Report | Keeps the received packet and original source identity through author pressure; no replacement port or invented report is admitted. The SPU process cases retain one run-scoped residency_degraded account through a frozen SQLite capture and subsequent Leave. |
| EmissionWork::close_correction_refusal | A cancelled emission refuses a new close election. An actual refused correction and Stop produce the retained cancelled repair; a later Unload cancellation cannot resurrect CancelAccount or consume its account twice. |
| Repository::verify_completed_leave | A standing pending or broken guard precedes cause/confirmation comparison. A converted join therefore reports BrokenSavePoint; an unguarded cause mismatch still refuses its binding. |
| Serving Leave invocation | Only the eligible exact publisher finalizes. Cleanup is attempted even if finalization fails. Finalized saves survive later cleanup or marker failure; no failed cleanup grants Unloaded or a closed marker. |
| Serving Load invocation | The complete correlated Ready must arrive and validate within Enter. Marker work follows acceptance. The process fixtures delay this work beyond Enter and separately force its failure. |

## Review disposition

| Finding | Disposition |
| --- | --- |
| #360 S1 | Corrected and mutation-protected: the non-waiting driver cannot spin through observation settlement. |
| #360 S2 | Corrected and mutation-protected: rollback returns Pending for the same wait. |
| #360 S3 | Corrected and mutation-protected: the common pump consumes its closed-tail wake; malformed evidence returns a refusal. |
| #360 S4 | Corrected and mutation-protected: halt clears the cached serving wake; completed or unavailable observations provide no wake. |
| #360 X7 / Codex 4390760991 | Corrected and mutation-protected for held and queued final SPU reports after observed loss. Both are actual process/SQLite/Leave cases. |
| #350 CR1 | Corrected and mutation-protected: publication guard refusal has precedence over cause mismatch. |
| #350 D1/D2 | Operator approved the tested mechanism and per-event causes on 2026-10-27. |
| #350 F1 / Codex 4389527832 | The final-termination-mode correction is already in the fold; this act retains it and answers its thread. |
| #350 L3/X6 / Codex 4389527838 | Operator ruling, paired Spec/code correction and mutation-protected cleanup-failure case. |
| #332 Z1 | Corrected and mutation-protected: cancelled correction refusal cannot elect another close. |
| #332 Z2 | Carried drift: unify outbound/refusal-author accounting with the original Decision budget in the next emission-close act. No activation claim. |
| #332 Z3 | Operator ruling, paired Spec/code correction and mutation-protected delayed marker case. |
| #332 Z4 | Filed as #362, with a bounded-retry/terminal-precedence acceptance test. |
| #332 Z5/Z6 | Retain the Planner's declines; explicit omitted bounds and the existing OutOfOrder retry rule are not changed. |
| #332 Z7 | Carried to deployment/diagnostic qualification: document the socket-option kernel floor and identify unsupported peer-pidfd acquisition precisely. |
| #332 Z8 | Carried internal-invariant consistency work: align the three permit-mismatch assertions with their fallible siblings. |
| #332 Z9/Z10 | Carried admin consolidation: one channel/refusal mapping and one capture checkpoint adapter. |
| #332 Z11 / #350 CR9 | Carried measured-allocation work: capture probe buffers and whole-index replacement costs remain. D1's mechanism is explicitly approved. |
| #332 amendment headers | Corrected to harness 0.38 and types 0.34, retaining first filing dates. |
| #332 admin flake | Already filed as #359; duplicate #361 closed. The Planner's full-suite/isolated evidence and inferred fork/OFD-lock mechanism are distinguished. |
| #350 CR4/CR10 | Corrected header history and publication module documentation. |
| #350 CR3/CR5/CR6/CR8 | Carried publication consolidation and retained-part attribution. CR7 keeps the Planner's decline. |
| #360 S5/S6/S7/S8/S9/S10 | Carried lifecycle ordering, claim consumption, no-executing-pair debt, rollback identity retirement, diagnostic precision and independent-observation progress. These were batch findings, not closed by the focused fix round. |
| #360 T1 | Carried load-sensitive halt-marker fixture race. The repeated worker runs below report its actual outcome for this source. |
| Q3 | Corrected evidence scope: the six harness joint parents run as root in delegated scopes and launch the actual admin test participant, whose fixture asserts real root. These seven distinct test identities exercise finalization and joint Leave. This is author evidence, not a Planner reproduction. |

## Validation

- Ordinary admin/harness invocation: 2,709 tests and doctests passed (551 admin,
  2,158 harness); 68 admin and 781 harness ignores were declared. Nested child
  summaries are excluded from those counts.
- Three default-parallel runs of 706 selected non-joint worker cases passed.
  Six actual admin/SQLite joint parents passed serially under real root in
  delegated scopes. This covers 712 selected worker cases; the named later-input
  SPU-loss case remains excluded.
- The corrected standalone delegated admin selection passed 29 cases, including
  accepted-Ready marker delay/failure, save preservation after cleanup failure,
  and simultaneous publication/cleanup failures. The final converted-cause
  assertion and ordinary admin unit suite (519 cases) passed on the rebuilt binary.
- Both affected crates passed strict all-target clippy. Workspace formatting,
  SQLite member build, before/after lock and diff checks passed.
- Ten independent protective mutations compiled and failed their intended
  regression assertions. The unchanged existing D1/D2 mechanism retains its
  earlier mutation scope; the converted-cause process assertion is also corrected.
- The 796-file crate/config manifest was stable through each gate run. The only
  source change after the full worker runs was the stale expected cause in the
  admin test; admin lint, build, unit and process checks were then rerun.
- Pinned nightly, --locked, private target, four ordinary test threads and serial
  Cargo/Git mutations. The full worker runs used default parallelism; joint and
  privileged admin cases ran serially. No production model or role-isolation
  qualification follows from these fixtures.

Ten temporary protective mutations each remove one correction. Every mutated
program must compile, then fail its named regression assertion. The final gates
run only after every source is restored. Existing mutation evidence for unchanged
stack predicates remains scoped to those earlier heads; it is not counted as new
coverage here.

The initial added queued-report fixture could receive its packet before the test
pinned that ordering. A first handshake correction then blocked a worker waiting
on capture. The final fixture explicitly closes decode to wake that worker before
sending its residency report. Both failed fixture attempts are retained in the
author evidence; neither is represented as a passing test.

No deployed agent changed. Required named code-review and verification skills
remain unavailable; no invocation or waiver is claimed. These tests and source
review are author checks, not security disposition or production qualification.

The first full admin-process invocation exposed a stale graceful-conversion
assertion and selected a child-only joint helper without its parent fixture. The
assertion now checks the final forced cause. Joint helpers run only from their
actual parents; the corrected standalone admin selection excludes both. The
failed invocation remains in the evidence, followed by the corrected run.

## Required follow-on acts before activation

| Act | Required work |
| --- | --- |
| World request / NoResidency | Implement merged #358's typed carrier and refusal, including two requests under pressure and precedence. The existing later-input SPU-loss test remains explicitly excluded and is not claimed fixed by report intake. |
| Refused capture S4-S7 | Implement the merged #348/#349 refusal/cleanup continuation rules through the actual worker/admin owners. |
| Emission close | Remove the Z2 second Decision budget and consolidate the remaining close owners without losing cancellation or first-failure identity. |
| Failed report and lifecycle handoffs | Complete the carried S5-S10 and prior C3/X5/X8, X11/X12 and C9/C10/X13/X14 owner boundaries. |
| Final qualification and cutover | Complete dispatcher/process-exit integration, retained-byte limits, pinned language/classifier artifacts, reviewed prompt, measured limits, security and coupled cutover. |

Issue #146 remains open. These corrections are scoped to this implementation slice;
they do not establish readiness to select or deploy the replacement. Only the operator
merges or deploys.
