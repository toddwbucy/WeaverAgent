# Startup refusal rollback composition

Date: 2026-10-24
Branch: `code/state-startup-rollback-composition`
Parent: #340, `0754aee22cd9a0d635a02afa2f20c4c279d67363`
Tracker: #146

This record covers the classifier-admission refusal route from the actual Enter
through explicit rollback and Left. It also implements the merged #285 release
shape for the replacement writer and removes the stale NoSpu hold from the private
failed-residency handoff. The active worker still serves `dev_loop::drive`.

## Authority and scope

Implements: harness Spec section 6, refusal rollback and the seal; harness amendment
HARNESS-ENTER-WINDOW, HARNESS-CLOCK, HARNESS-RECORD-LIFETIME and
HARNESS-RELEASE-ABSENT; trace amendment TRACE-RELEASE-ABSENT and TRACE-HALT's release
member. The admin Spec section 3's S1 rollback and seal supply the existing forced,
cause, save-point and answer rules. No Spec election changes in this act.

| Implemented portion | Author assessment |
| --- | --- |
| Actual classifier refusal | Conforms: original Enter, raw bytes, load cause, source refusal and Recorded sequence remain owned. The response uses the original authenticated peer and exact send witness. |
| Explicit rollback | Conforms: only an actual valid rollback Leave begins cleanup. A late rollback preserves the expired Enter and receives the existing independent Leave cap. Waiting for root still observes state/trace failure through the common pump; no boot reduction or replacement Enter allowance occurs. |
| Grants and seal | Conforms: the existing owner retains initial/current Grants and derives Unchanged/Varied/Unreadable. Seal unlinks first, drains the actual backlog, refuses secondary directives OutOfOrder, and isolates secondary read expiry from the accepted rollback. |
| Terminal release | Conforms on this no-SPU route: reap the actual classifier, close the actual member ask end, author one Unload with the original load cause and forced true, and require the writer drain before the exact timely Left. No snapshot or wind-down runs. |
| TRACE-RELEASE-ABSENT | Conforms in the replacement writer: every Unload requires an outer release. Optional SPU/member fields serialize absence rather than null; SPU present values retain confirmed/unconfirmed. The active legacy type remains mandatory-SPU until coupled cutover. |
| Failed no-SPU cleanup | Conforms in the private failed-residency handoff: retain the first failure/cap and use the existing bounded failed cleanup. SPU absence is derived from actual fork custody; reap and port retirement never turn a created child absent. |

This is not a complete implementation of the enclosing lifecycle sections. General
failed/expired startup and failed rollback handoffs into terminal cleanup,
Ready-then-rollback, healthy Leave/save publication, final process exit/dispatcher
selection and production qualification remain owed under #146. A stopped public
rollback owner retains its intermediate values; dropping it is not successful
cleanup or permission to admit another run. Do not select or deploy this partial
replacement.

## Ownership and terminal ordering

`FoundationEnter::rollback_refusal` consumes only the original classifier refusal.
Rejected joins return the original owner. The public rollback stages retain their
private owners through authoring, response, waiting, accepted rollback, measured
preparation and terminal cleanup. Orchestration remains in worker/dev_loop.

The awaited Enter cap remains registered and retained. The common transport pump
omits only that cap from its local wait view while awaiting root; other waits and
component failures survive. While awaiting rollback, a secondary connection gets one read/answer allowance
from its actual accept, never a fresh allowance on retry. A lost original Enter
answer path cannot authorize a replacement response or erase the recorded refusal.

After explicit Leave, no secondary connection changes the original cause, forced
outcome or cap. At the seal the pathname disappears before member release. The
listener descriptor closes only after the tracked Left is wholly sent; the
completed owner retains descriptorless input/send evidence. A staged answer is
insufficient. Expiry or failure grants no new successful continuation.

The terminal author retains its actual Unload sequence before a recorded-observer
failure. Such a failure still stops the run and cannot resend or turn into Left.
A blocked writer at the inherited Leave cap remains a stopped owner, even when
Unload was already admitted. New release absence creates no child, exchange,
terminal permit, time allowance or success claim.

## Evidence and limits

The PR body supplies the final gate counts, compiled protective controls, source
hashes and immutable evidence deposit. The authored process circuits exercise the
original refusal exchange, late rollback, Grants change, secondary read expiry,
state EOF while waiting, original/Leave peer loss, sealed backlog, actual send
pressure, Unload observer failure, real blocked-writer drain expiry, late Left and
the actual worker loop. All startup cases use the normal test thread stack.

Model/member/classifier replies are declared doubles. These tests establish the
stated composition properties, not production language/model qualification. The
pinned scheme/dictionary/corpus, classifier artifact, reviewed system prompt,
measured limits and coupled cutover remain open. Existing privileged/inherited
ignored cases remain disclosed by the PR; no new Planner or security grade is
claimed. The named review skills were unavailable; author review was manual.

## After-action carry

The sequence walk exposed the unimplemented #285 representation, the stale no-SPU
failure hold, missing common-pump failure observation while waiting, and seal
expiry/backlog defects. Tests and code were corrected together. Two queued callers
are required to distinguish completion of one answer from exhaustion of a backlog.
A silent accepted caller tests the separate read bound through the seal.

Reuse the existing full-name subprocess macro and inspect child test counts. The
first six added test launchers ran zero child cases and were corrected; those runs
are not gate evidence. Bound fixture setup and fit queued dials to its actual
listen backlog. These rules are the standing carry into the next composition act.

A drain mutation reached its intended assertion but blocked during fixture unwind.
The fixture now shuts its real sink on panic before the outer recorder drops. The
mutation must exit as a failed test; a timeout is not counted as detection.
