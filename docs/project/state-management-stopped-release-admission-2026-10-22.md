# Stopped release admission implementation record

Date: 2026-10-22.
Branch: code/state-stopped-release-admission. Parent: exact #264
285ab7fb1dcda0b23f16f4dcbd4c671f0433720a. Root three-way integration preserves
#262's final fixture corrections; all seven joined source hashes were checked.
Authoring seat. Epic #146; Planner #258 row 4.

The dormant recording context now admits its existing nonselectable shutdown set
through a private release author boundary. This closes the concrete mismatch where
boot Open rollback stopped the tap, but ordinary Fault, Refusal and SessionClosed
could become Recorded and then fail solely because the tap remained stopped.
PlanSummary rollback retains its existing live tap. Neither road reactivates a tap.

## Authority and scope

This is an implementation of already merged clauses, not a new lifecycle election:

| Authority | Implementation assessment |
| --- | --- |
| HARNESS-RECORD-LIFETIME | Conforms within the executed owner tests: only the actual healthy Releasing context can issue the private author scope; the existing four shutdown kinds remain the closed set. No selected content, filing, classifier or read grant. |
| TRACE-STOP | Conforms within the executed owner tests: accepting Stopped requires the exact retained release ticket and scope. Generic/raw authoring cannot acquire that permission. No tap reset or reopen. |
| TRACE-CAPACITY | Conforms within the executed owner tests: release uses the ordinary recorder submit path. Failed reporting alone retains its existing terminal reserves. |
| HARNESS-OPEN / HARNESS-OPEN-LIVE | Conforms within this slice: the exact owed Open owner still holds generic release authoring and transport retirement until settled or bounded incomplete. No journal, mark or generation grant follows a cancelled account. |
| HARNESS-PLAN | Conforms within this slice: sealed boot summary rollback keeps its live tap and retained read/account; state retirement is not repeated. The carrier and summary consumer remain separate duties. |
| HARNESS-HALT / HARNESS-HALT-BOUND | Conforms within this slice: failure revokes healthy release permission, retains the original/ticket or actual Recorded sequence, and leaves Failed obligations to their existing owner. |

The author grades above have dedicated-target runtime and protective-mutation
evidence. Independent source review is complete; the Planner grade remains pending.
No production qualification is claimed. The full failed-enter/leave lifecycle composer and author migration remain
held. Worker/dev_loop still owns orchestration; classification stays in the SPU.
No existing serving caller is activated or replaced by this slice. Draft #260's
positive generation-limit proposal is not implementation authority here.

## Implementation and custody

context_release.rs owns the four-kind predicate and all private proof fields. Its
scope borrows the issuing context's actual phase and first-failure slot and carries
the barrier identity. The owned author consumes the exact event; an allocation
identity distinguishes each attempt even when all public run/session/kind values
match. RecordBarrier retains the original AuthorTicket and a private ticket bound to
that barrier and attempt. Preparation failure occurs after original retention.

Each pressure continuation verifies the actual phase, first-failure absence,
barrier, attempt, original ticket and kind, prepared kind, run and Observation
purpose. A completed attempt cannot be replayed. The boundary records its actual
sequence before fallible context hooks; a later recoverable wait can return that
same sequence once, while a first fatal failure retains it for accounting. A new
caller cannot replace an existing ticket or take another owner's completion.

The trace crate and its capacity policy are unchanged. Ordinary release records use
ordinary capacity even after stop. The separately owned Failed route may make its
first reserved Fault while the release original is still pending; healthy release
cannot reuse its former scope once failure has latched. Existing private Open and
PlanSummary trace-only account paths retain their distinct authority.

## Instruments and limits

Thirteen private-boundary tests cover live/stopped success, actual ordinary-capacity
pressure and Failed reserve priority, recoverable waits, pre-prepare clock/source
failure, foreign same-ID contexts, wrong phase/run/purpose/kind, malformed payload,
replacement ticket/attempt, generic/raw bypass, completion replay, post-Recorded
clock failure/wait, closed recorder, and a competing caller's original custody.
Private capability types also have compile-time negative Clone/Default/scalar and
authored-event conversion pins.

Three integration instruments compose actual existing owner APIs and real sockets:

- Boot Open prepared-unsent cancellation, complete sent cancellation, placement
  pressure, truthful fit refusal, and original Enter expiry all settle before
  decode/state/classifier retirement and the same four ordinary shutdown records.
  Pressure also checks that a generic release Fault cannot steal the Open ticket.
- PlanSummary typed refusal and cancellation before request or after Released retain
  their actual refusal/read evidence through the live-tap shutdown and trace drain.
- A real partial PlanCurrent snapshot remains owned through original Enter expiry,
  sealed state retirement and the live-tap shutdown; no synthetic completion or
  second state retirement is used.

These are scripted member/classifier/decode instruments. They prove neither a live
SQL lifecycle composition nor any classifier/model artifact qualification. The
existing B3/B7/B8/B9/B10/B13 transport/account tests remain required. SEQPACKET has no
healthy partial-packet send fixture: transport failures and retained sent ownership
remain covered by their existing tests, rather than by a fabricated partial send.

All cleanup assertions stop at the dormant context boundary. They do not fabricate
successful Left after an incomplete exchange or choose a lifecycle response policy.
Unload stays last, trace drains, and Closed refuses further authoring/reduction.

## Executed author validation

Root ran lock first, then all resolving Cargo commands with --locked in the private
/tmp/weaver-146-stopped-release-admission-target. Gates and mutations were serial.

| Gate | Result |
| --- | --- |
| Lock | 0 |
| Focused release tests | 92 passed, including all 16 new fixtures |
| Full harness | 1,567 passed, 18 inherited ignores |
| Full trace | 160 passed, zero ignored |
| Harness all-target clippy | Pass with warnings denied |
| Formatting and diff whitespace | Pass |
| Protective mutations | 13 compiled test failures, 13 exact restored passes |

Every final mutation source hash matches. The probes pin actual phase, issuing
context, attempt, ticket, closed kind set, Observation-only admission, lawful stopped
success, ordinary capacity, Recorded sequence retention, final attempt retirement,
raw stopped refusal, original-before-clock custody and the existing owed Open hold.
Redundant phase/failure defenses are not claimed as independently killable guards.

The first compile exposed a missing test-helper import; only that import was added.
Clippy then required an equivalent factoring of the release refusal guard. All eight
boolean inputs agree, and query evaluation order is preserved. Full harness/lint
were rerun on the final source. Those initial logs remain in the evidence. Mutation
anchors were refreshed after formatting; all thirteen final probes executed.

Evidence is /tmp/state-stopped-release-admission-gates/ and
/tmp/state-stopped-release-admission-mutation-evidence/. Published source, source/input
hashes, logs, recipes and review notes are deposited at
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/stopped-release-admission-285ab7fb/.
These are author checks, not a Planner grade, complete lifecycle circuit or production
qualification.

The staging base is exact #261 ef32e86b250c79f66a919ce483a676337bc233ee plus the
39 resolved PlanSummary join-preview files, with an external source manifest for
later three-way integration. The only PlanSummary test-file edit in this slice is
the new integration module include; preserve root's later corrected fixtures.
