# Failed-context first-fault report owner

Date: 2026-10-22. Draft PR: #249.

## Scope and authority

This slice advances issue #146 E through an actual consumer of the stopped recorder's
first-fault reserve. Its parent is #248. That ancestry retains #190's
operator hold for the foundation cutover. Runtime serving remains on the complete
legacy implementation.

Implements: weaver-harness-Spec-amendment HARNESS-RECORD-LIFETIME, HARNESS-HALT,
HARNESS-HALT-BOUND and HARNESS-HALT-ACCOUNT, scoped to the first-fault reporting
operation and opportunistic progress of already owed trace-only records. Spec 09
FLOW-COUNTS supplies the existing frozen sample. This is part of the Reporting phase,
not its complete lifecycle composition. The author grades these scoped clauses
conforms on the checks below. This is not a Planner grade or production qualification.

The new owner requires the same failed RecordingContext and its already installed
checked halt account. The caller freezes real external owner facts through the
existing freeze_halt_account operation first. This port accepts neither a raw account
nor caller-supplied fault sequence. It does not invent capture windows,
Commit/Published progress or a pre-open flow observation. Spec 09 retains the existing
diagnostic path for Open failure before load.

The first bounded advance constructs an immutable runtime FaultReport using the exact
first-failure account. A trace-first cause uses StreamWriteFailed. Other causes use
StateFlowFailed. It attempts the existing Fault reserve once while retaining any
ordinary pending event and its original ticket. The report owner keeps the prepared
original and actual Recorded sequence before a later clock sample. Rendering or
submission refusal cannot recursively attempt a new fault. The raw account and first
cause remain in their original HaltLatch.

Every advance checks the same common clock and original halt deadline before and after
bounded work. The existing earlier Leave cap remains authoritative. Missing, reversed
or expired time, including an unavailable overflowed deadline, produces AbortRequired
with retained evidence. It never rearms a serving wait, renews the halt deadline or
clears failure. An aborted report stays aborted.

Later advances make at most one opportunistic trace-only progress step. They never
poll or wait for capacity. A future teardown driver can therefore interrupt the gate
without waiting for an ordinary pending record. The measurement-refusal owner retains
its own ticket and exact sequence for work still owed at report admission. Other
already pending events use the existing stopped-tap trace-only road. Neither road
creates a state copy, receipt, generation permission or successful serving
continuation.

The evidence remains in the context for a future unload to name its exact fault
sequence, or null if no fault was recorded. It is neither Clone nor Default and has no
conversion from a bare Sequence. Its status is not containment, release, drain
completion or authority to skip another halt phase.

## Tests and protective changes

Twelve new tests cover:

- Failed-phase and installed-account admission, with no replacement of an owner.
- Real state EOF and wrong control response, preserving the exact request and
  frozen account in one canonical fault without resuming or requerying.
- A full actual writer, a real pending author, reserved fault admission and later
  trace-only drain without a tap copy or ledger transition.
- The earlier Leave cap, exact expiry, initial clock failure, deadline overflow,
  and consumed fault or ordinary trace sequences retained before a later clock
  failure, reversal or expiry.
- A sound writer with its Fault permit already consumed through the real lower
  API. This is an explicit permit-misuse fixture, not a fabricated sink failure.
- A failed sink with StreamWriteFailed and sticky reporting failure.
- Immutable first-fault account and deadline with secondary evidence retained for
  unload, without authoring an unload prematurely.
- A real fatal decoder refusal under recorder pressure. The same measurement
  owner records it after the reserved fault and retains its raw response without
  releasing a measurement grant.

The member and decoder fixtures use real sockets and the actual trace recorder. They
do not establish SQL, device, cgroup, gate containment or deployment evidence. The
overflow case is a checked arithmetic boundary, not an observed host clock. No test
fabricates a capture publication or transport-completion proof.

Validation on 2026-10-22 used private target `/tmp/weaver-146-halt-report-target` and
locked dependency resolution:

- All twelve focused tests passed on the joined inspection/boot stack.
- Full harness suite: 1391 passed, with 18 inherited ignored cases.
- Strict all-target harness clippy, formatting and lock checks passed.
- The author run reported fourteen compiled protective failures with exact
  restored baselines. The subsequent Planner pass found two surviving mutations,
  H3 and H7, and those gaps are addressed in the follow-up below.
- Three E0283 probes rejected Clone, Default and conversion from a bare Sequence.
  Exact restoration and the final baseline check passed.

The initial focused run had three fixture failures: its blocking measurement lacked
its required turn. The corrected fixture uses an ordinary attributed measurement. The
initial failure logs remain alongside the corrected and joined results. Independent
author review and a separate parent-join reading found no remaining concrete defect in
this scope. The code-review skill was unavailable, so no skill invocation is claimed.

Logs, source hashes, runners and recipes are deposited under `halt-report/` in
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-21/`.

## Review follow-up

The Planner review at 5512789c found no fix-bucket issue, but H3 and H7 did not
protect fault turn attribution or secondary evidence from a failed terminal
attempt. The new H3 fixture records TurnStarted through the context, triggers a
real bound failure, and reads the resulting fault's run and turn. The existing
used-permit fixture now reads the exact secondary detail, point and observation,
and verifies that the frozen fault still has an empty secondary list while the
unload view carries that later evidence.

The stopped-tap drain now repeats the existing run and release-purpose check
before writer admission. A lower-owner misuse fixture passes a foreign author
run or an incompatible purpose through the real admission API. Both fail
without losing their exact original ticket, prepared event or source payload.
This test does not claim that the healthy context constructs either mismatch.

RecordBarrier now retains its latest actual stopped-tap Recorded sequence.
HaltReport initializes and refreshes its separate trace evidence from that slot
before any post-work clock check. A real decoder refusal is drained before
report admission and its sequence remains available afterward. This does not
change the frozen first-fault account, settle its pending-author snapshot, or
implement the separate Recorded watermark rule merged in #237.
The slot covers ordinary stopped-tap drain admissions only. It excludes the
separate reserved fault sequence and is not a global trace-sequence maximum.

The inherited measurement helper can resume an older pending event and return
None because its Some result belongs only to its own refusal. That admitted
sequence is now retained by the common barrier. The healthy measurement path
checks both pending event and pending author before consuming its reserved
decoder reply, and admission excludes competing authors. No fixture forges a
simultaneous older author and fatal decoded refusal to claim public reachability.

A confirmed recorder failure during the first fault attempt now makes later
advances return Unavailable without another drain attempt, even with no pending
work. Real closed-sink fixtures cover both pending and empty cases and preserve
the first attempt's secondary count. An inner measurement clock error retains
its ticket and exact secondary without claiming a failed sink. A rejected
generic drain still stops that invalid reporting road while preserving its
original pending event.

The fault helper now borrows its retained original through the same terminal
submission API, preserving the recorder's one-use per-kind permits. A private
method replaces the immediately invoked closure. The existing owned wrapper
still returns its original event on failure.

Five tests were added and two existing checks strengthened. The follow-up passed
16 focused halt-report tests and the stopped-drain guard test, followed by the full
harness suite: 1425 passed, with 18 inherited ignored cases. Strict all-target
harness clippy, formatting and lock checks passed. Nine protective mutations
compiled and failed their intended assertions, and each exact restored baseline
passed. The independent source reading found no remaining concrete defect in this
scope; it is not a Planner grade. Follow-up logs and source hashes are retained at
`/tmp/weaver-146-evidence/review-round2-249/`; these new results have not been
deposited in the shared evidence store.

Batch dispositions remain explicit. The final unload integration must reconcile
later admitted author evidence without rewriting the first snapshot. No unload
is authored here. The existing closed reason vocabulary remains unchanged, with
precise clock or deadline detail under trace_failed/teardown. The measurement
helper retains its independent clock check because callers can invoke it
without HaltReport, and the outer before/after checks still bound this driver.
The fixtures remain nested under read_tests to reuse their actual socket and
recorder setup. Neither a new reason spelling nor a fixture-module relocation
is required for these corrections.

## Review dispositions

Planner review 5610812388 and its merge-only carry 5611201559 are answered
by scope, without changing the merged failure vocabulary:

| Finding | Disposition |
| --- | --- |
| H3, fault turn | Record a real TurnStarted and compare the fault run/turn with that recorded envelope. |
| H7, failed-attempt secondary | Assert the exact secondary detail, observation and teardown point; the first account stays frozen and the unload view carries the secondary. |
| 1, drain run/purpose guard | Apply both existing author checks before stopped-tap submission; retain the original on rejection. The direct malformed-owner probes are defensive, not reachable healthy producers. |
| 2, lost older drain sequence | Retain the latest ordinary stopped-tap admission in RecordBarrier and expose it separately from the reserved fault sequence. Both measurement drain sites and the generic halt drain use that same retained slot. |
| 3, pending author in frozen account | Keep the first snapshot immutable as HARNESS-HALT-ACCOUNT requires. The separate actual admission sequence remains available to the future unload owner; no invented live snapshot replaces the frozen one. |
| 4, non-sink measurement error | Inspect the actual standing recorder failure before setting the sink-unavailable state. The clock-error fixture retains the owed ticket and exact secondary. |
| 5, first-attempt sink failure | Make reporting unavailable immediately when the recorder confirms failure; repeated advances retain the one secondary without another trace attempt, with and without pending work. |
| 6, clock/deadline reason | Retain the existing trace_failed/teardown spelling and exact detail. A new closed reason is a separate Spec decision, not required to correct attribution evidence here. |
| 7, fault clone | Submit a borrow of the retained original; preserve the owned wrapper for existing callers that require their original returned on refusal. |
| 8, repeated clocks | Retain the inner check: failed_measure_refusal_progress is also callable without HaltReport. The outer before/after checks bound this adapter and preserve consumed evidence before a late failure. Removing either ownership boundary would weaken its existing deadline contract. |
| 9, inline closure | Extract prepare_and_submit_halt_fault; retain the event before terminal submission. |
| 10, nested tests | Keep the existing actual read-peer fixture in its current module. Relocating it would broaden the change across read-owner tests without strengthening the halt invariant. The test names identify halt behavior explicitly. |

The affected consumer sweep found three stopped-tap drain sites: the generic
halt owner, the measurement owner's prior event, and its own refusal. The new
retention slot covers all three. Terminal submission has the halt owner and
lower-level permit tests as consumers; the borrowed adapter retains the same
one-use recorder gate and creates no new terminal permit.

## Remaining integration

The older fatal measurement path can queue or progress its newly created refusal
immediately after the latch, before an external caller freezes the halt account. Its
ordinary admission checks exclude the suspected concurrent older pending writer case.
This slice consumes an already installed account and orders only work still owed at
that point. Broader latch-time snapshot and caller composition remain an integration
obligation. The pressure fixture pins the retained refusal at report admission, not
every pre-admission timing in that older path.

The owner does not cancel a real pending publication or author its refused save_point.
That requires the complete capture lifecycle owner and its actual window, cause and
Commit/Published facts. It must never relabel a valid Commit as refused. Requiring an
existing checked account does not qualify external facts that the supplying caller did
not own.

Gate interruption and containment, stopped turn/caller response, gate lower, SPU
release, failed state/classifier retirement, component_failure unload and bounded
final drain remain separate executable work. The legacy Run-to-context bridge and
serving FlowFailed/EndServingWithoutSave dispatch are not implemented. No grants
query, snapshot, successful Left, worker-written admin marker or serving activation
follows from this slice. Production qualification and the coupled foundation cutover
remain open.
