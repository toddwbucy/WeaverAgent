# Plan summary review followups, 2026-10-22

Date: source prepared on 2026-10-22; root integration on 2026-10-23.
Branch: code/state-plan-summary-review-followups. Parent: exact #266
742ae9cb0344594dbf13fcf4653b952ae74da346.

The source was independently staged against exact #262 7cad4da7. Root verified
and joined all eight overlays, preserving #264 codecs, #265 stopped-release
custody and #266 repair-ledger changes. The only textual conflict was two appended
test-module includes; both remain. Independent join review found no blocker.
This record answers Planner review 5623894517 and epic #146. Ordinary gates and all
17 protective mutations passed. No qualification is claimed.

## Authority and scope

Merged Spec 07 PLAN-SUMMARY-EVIDENCE and PLAN-EXCHANGE, and the harness amendment's
HARNESS-PLAN, HARNESS-OWNERS and HARNESS-READ remain the authority. Boot summary
precedes user admission. A committed effect owes a refreshed Recorded summary
before the next generation. Stop finishes admitted dispositions and preserves
committed evidence; it grants no new work.

This remains a dormant primitive. It does not implement the full plan producer,
management result policy, placement composition, lifecycle Ready election, active
serving cutover or production qualification.

## Review findings

1. Plan-effect filing before boot summary commit now returns the unchanged event
   with Busy, before receipt reservation or author admission. An actual boot
   summary can then complete and the returned event can be filed normally.
2. The common pump temporarily withholds only a wholly unsent generation decode
   leg while summary evidence is unavailable. It continues state, lifecycle,
   classifier and trace transport, and restores the caller's exact port before
   processing any pump outcome. An already started segmented generation and
   Cancel continue transport. This is not a new generation grant.
3. `cancel_filing` is documented as terminal Stop/Leave cancellation, including
   the unstarted case. The unchanged event is returned, prior evidence remains,
   and future cache use is revoked. A future non-terminal withdrawal API must
   preserve an unchanged cache; this patch does not invent such an API.
4. Summary admission now checks the omitted template work, blocked template
   author, held measurement and unrelated Decision/Cut/Gate/StateControl waits.
   It permits a current turn and its own exact Decision or Cut for an effect
   refresh; boot admits neither. The read guard also permits only the summary's
   exact driving Cut key and bound. Previously a valid Cut filing could enter
   summary ownership but never start its read. Placement's separate reload-drain
   permission is not introduced here.
5. Stop/Leave after summary completion revokes future cache use but preserves the
   already completed Boot/Effect disposition and Recorded read. Explicit
   cancellation uses the same rule. This follows the merged requirement to
   preserve completed admitted facts, rather than redefining completion at
   extraction. A successful historical disposition is not a live cache grant.
6. The two transaction literals use one private constructor; the original bound
   seeds the account deadline in one place.
7. Typed transaction metadata and bounded public streaming retain their separate
   traversals, shared row decoders and existing parity tests. A full typed
   builder on the public path would allocate the whole result before its byte
   ceiling can refuse it. A comment makes this deliberate allocation difference
   explicit. A common streaming visitor is a possible later refactor, not needed
   to correct these ownership guards.
8. Receipt observation returns early unless a flagged plan filing exists. The
   direct-author test protects the prerequisite: an unflagged plan effect or
   StepOutcome cannot enter the author/filing path. The redundant C1 Pending
   assignment is removed; actual in-flight ownership and the retained refresh
   duty already deny cache success across the commit window.
9. The root's earlier artifact followup already corrects the implementation map
   with PR #262. This staging does not edit the map again.

## Planner survivor disposition

The original Planner found 15 survivors, distinct from the author's earlier
protective mutations. This record does not reclassify them as failed probes.

| Probe | Disposition in this slice |
| --- | --- |
| C4 | New actual segmented-generation test sends the real preamble, starts an effect, then checks every remaining original body byte drains while cache use stays Pending. |
| R2 | New actual shutdown ENOTSOCK injection checks the completed refused boot account retains close-error evidence. Only this fixture's owned descriptor is atomically replaced. |
| R4 | New stopped-account test replaces the actual pending recall ticket with another valid recorder ticket, attempts the first fault report, and proves the summary account neither resumes nor retires the foreign ticket. |
| F5 | New direct-author tests refuse actual PlanEffect and StepOutcome inputs before recording or filing. |
| E6 | New actual foreign-seed effect is classified and accepted by a scripted state member; the harness must fault on its committed receipt before starting a refresh. This is distinct from changing a later member summary reply. |
| C9 | Actual checked Open/repair proof is retained while deliberate registry corruption changes Enter identity; admission refuses the exact proof without consuming it, and the restored original bound permits it. This is a defence test, not a claimed ordinary API path. |
| L2 | Actual effect ownership is retained while deliberate registry corruption changes its Decision identity; advance must fault before read admission. This is a defence test, not a claimed ordinary API path. |
| C1 | Removed redundant stored Pending assignment; in-flight owner and required-duty checks remain the executable availability guards. |
| C8 | Retained defence in depth. A foreign actual receipt already fails independent context/source/key/bound checks; the private non-Clone completion and monotonic source identity prevent a normal caller from constructing a second same-context same-source receipt. No artificial receipt is presented as a reachable case. |
| C10 | Retained defence in depth behind consumption of the actual non-Clone RepairedBootPairs proof. No second ordinary proof can restart the boot summary. |
| L3 | New recoverable Label-wait clock sweep finds actual read admission before a later hook yields, then proves the same read resumes to Released/Recorded with one request and its original bound. It pins forward progress, not merely absence of a duplicate request. |
| L4 | Retained defence in depth: an effect key remains held by required; boot also has its existing Enter/template custody. Existing real receipt tests protect the combined wait-retention property. |
| E7 | Retained defence in depth behind the closed authored payload/source mapping. No fabricated private committed source is called a normal owner path. |
| E8 | Retained defence in depth behind the authored Close/StepOutcome pairing and strict payload validation. |
| S1 | Retained deterministic SQL ordering; valid mutation paths permit one non-superseded revision. Reversing that order cannot change a valid current result. Existing typed/public parity checks remain the invariant evidence. |

## Executed author validation

Root ran lock first and all resolving Cargo commands with --locked in the private
/tmp/weaver-146-plan-summary-review-followups-target. Gates and mutations are serial.

| Gate | Result |
| --- | --- |
| Lock | 0 |
| New summary review fixtures | 16 passed |
| All focused summary fixtures | 53 passed |
| Internal typed/public metadata controls | 4 passed |
| Full harness | 1,588 passed, 18 inherited ignores |
| Full state | 225 passed, eight inherited ignores |
| State without SQLite | 31 passed |
| Harness/state all-target clippy | Pass with warnings denied |
| Formatting and diff whitespace | Pass |
| Protective mutations | 17 compiled failures and 17 exact restored passes |

The initial focused suite compiled and passed 14 cases. Two fixture inputs were
corrected: a close/StepOutcome now carries the same required turn used by the
existing effect controls, and the common-pump fixture receives its measured state
answer after the member's reported end. Production code and all control assertions
were unchanged. The original log and independent diagnosis remain in the evidence.
All focused/full gates above subsequently passed on the final source. Every final
mutation source hash matches. The probes pin pre-boot refusal, actual state progress
with unsent decode withholding, partly sent drain, exact Cut/bound/seed/ticket
identity, retained completed dispositions, shutdown-error evidence and preserved
port/read custody across fallible hooks.

Evidence is /tmp/state-plan-summary-review-followups-gates/ and
/tmp/state-plan-summary-review-followups-mutation-evidence/. Published source,
source/input hashes, logs, recipes and review notes are deposited at
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-summary-review-followups-742ae9cb/.
These are author checks and independent source reviews, not a Planner grade or
production qualification. The state comment retains its existing typed/public
parity boundary. Full plan production, lifecycle/placement composition and active
serving cutover remain open under #146.
