# Cancelled correction call accounting

Date: 2026-10-24. Branch: `code/state-cancelled-call-count`.
Parent: #319 at `e8d4a0439a68043ce2d3b157c4783430dca72ef1`.
Stack: main, #312, #314, #315, #317, #318, #319, then this draft.
Implements: merged #263, Spec 02 CANCELLED-CORRECTION, REPAIR and
RECALL-REFUSAL; Spec 05 COMPLETED-PAIR; harness amendment HARNESS-RECALL-REFUSAL and
HARNESS-LEDGER. Refs #146. No full implementation checkbox closes here.

## Behavior and ownership

A requested correction containing an attempted call now counts once when its
last required fact is retained. All complete Text checks, correction_call and
attributed recall refusals remain mandatory. Every conforming request must have
its actual CompletedPair, with the original CallId, StatementRef, statement and
opening commits, terminal Filed receipt and recorded observations. A completed
prefix or Recorded PairEnded without Filed contributes no attempt. Unassociated,
correctly rejected, unrecovered and malformed recall calls require their actual
refusals but no invented pairs.

The repair ledger keeps its account while its correction loan owns the call
suffix. Cancellation no longer transfers that account before the count can be
retained. The private loan moves its recorded protocol-refusal flag with its
exact correction identity. Returning an incomplete or foreign pair account
returns the original capability and refuses completion. After complete cancelled
accounting, the exact correction returns through the existing pending field;
the original RepairAccount retains the failed-pass count and source identities.

At the actual terminal observation, FlowCoordinator retains a CommittedSource
before subsequent clock and drain operations. A narrow internal transfer moves
only the one complete PairEnded receipt belonging to the existing repair terminal
leg into PairLedger. The regular FilingCompletion port remains live-gated.
Opening or ordinary statement receipts cannot use this transfer. It creates no
grant, timestamp, author, filing, wait, send or next-pair continuation. The existing
pair validator checks the exact receipt before constructing CompletedPair; an
invalid receipt remains in the pair owner's refusal slot. PairLedger is still
live at this producer boundary even if a later coordinator clock has failed;
the context observes the fact before freezing PairLedger. No frozen pair owner
is reopened.

Once the actual pair batch is complete, the repair owner validates all immutable
terminals against its exact correction loan and retains one failed-pass count.
PassPurpose, Stop and Unload prefixes keep the causes actually admitted to
authoring. Cancellation may strengthen only the unadmitted suffix. Account return
validates the same proofs and reuses the already retained identity, so it cannot
charge twice. An attempted correction with no conforming request counts at its last Recorded refusal
instead; its account remains in the repair ledger until ordinary extraction.

Counting changes no continuation stage. Below and at the cap, cancellation grants
no RestatementRequest, EffectiveEmission or RepairExhausted and authors no new
repair_exhausted event. Decision and Leave bounds retain their original values.
Ordinary accepted Text filings add no new wait; request filings remain mandatory.
The Text-only and empty rules implemented in #319 remain in force.

| Spec scope | Author assessment |
| --- | --- |
| CANCELLED-CORRECTION attempted-call boundary | Conforms in the changed repair/pair owners: all required facts, actual complete pairs, exact source/correction, once before extraction, including later clock failure. |
| REPAIR / RECALL-REFUSAL | Conforms for forbidden-call accounting: every owed attributed refusal precedes the count; nonconforming requests receive no synthetic pair. |
| Spec 05 COMPLETED-PAIR / HARNESS-LEDGER | Existing actual statement/opening/terminal evidence constructs CompletedPair. The added internal receipt movement grants no dependent operation or public filing completion. |
| HARNESS-RECALL-REFUSAL cancellation, causes and bounds | Conforms for this owner with its original Decision bound: cancelled accounting retains custody and count without retry, success, exhaustion continuation, rewritten admitted cause or renewed bound. Parent review row 2's absent-bound admission remains a carry below. |
| Whole worker/lifecycle and production qualification | Incomplete under #146. Actual correction placement/decode and full worker Stop/Leave/failed-run circuits remain separate composition work. |

## Instruments and validation

Six new tests use real recording, checker and state socket owners with scripted
language/member replies. Their Stop and Unload inputs enter through the repair
cancellation port; they are not full worker lifecycle circuits.

- Two-call corrections below and at the cap, with Stop and Unload: a complete
  first call and Recorded second terminal still count zero while its actual
  filing is held. The final Filed observation counts once before pair extraction.
- A three-call correction retains PassPurpose, Stop and Unload terminals in order
  across two cancellations, including already authored terminals with held filings.
- A complete pass cancelled before its correction loan returns keeps its count.
- Persistent clock failure after the actual final Filed observation preserves
  count, attribution, the first fatal cause and the absence of a successful return.
- Unassociated, rejected, unrecovered and two malformed recall shapes below/at
  cap require every refusal, retain exact attribution and create no pair.
- A mixed correction with a conforming request and two malformed recalls needs
  both recall refusals, correction_call and the terminal filing before counting.

The repair regression run passed 151 tests, with two ordinary ignored cases.
Its earlier runs exposed custody inspection assumptions after delaying account
transfer, an alternate completed-proof return that still owed its ledger loan,
and an old test allowing a cancelled correction to finish without required pair
proofs. The corrected paths retain the account in either legitimate owner,
settle the exact outstanding loan and return an incomplete capability unchanged.
The shared refusal validator now sees ledger-held pending custody as well as the
external correction slot. Existing original-identity, foreign-proof rejection,
frozen-owner error and protocol-flag controls continue to pass. Initial failures
remain in the evidence; they are not claimed as successful checks.

Checks used the pinned toolchain, --locked, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4 and private target
`/var/tmp/weaver-state-146-cancelled-call-count-target`.

| Gate or instrument | Result |
| --- | --- |
| lock.sh before/after Cargo, formatting, strict harness clippy, diff check | Passed |
| Full harness suite, four test threads | 2,012 passed, including 22 doctests; 183 ordinary ignored |
| Delegated gate/startup/input/Observe/Stop/generation/emission | 46/17/19/14/16/20/33 passed; 165 total, serial scopes |
| Protective mutations | Eleven compiled and were detected by the new tests |
| Restored focused tests | Thirteen passed: six new call-count tests and all seven parent Text/empty tests |
| Source identity | All 662 crate/configuration files restored and verified against the full gate manifest |

The mutations delay counting until extraction, delay receipt transfer, count at
correction_call before pair filings, omit unpaired or cancelled counts, duplicate
loan-return counts, misattribute the rejected source, rewrite admitted causes,
freeze before observing the final receipt, omit required refusals and reopen a
cancelled continuation. Each compiled, reached a failing test and was restored.

Source manifest SHA-256: `65c7e87454425b0a1f9c18443fc4fcdb9def01abe82b596967c5adf371eb2f68`.
The full/delegated gates tested this same source before mutation; hashes verify
its restoration, followed by the focused rebuild/run. Observe and Stop used the
actual admin test executable rebuilt in the private target. Eighteen inherited
ignores and eight privileged admin-startup instruments remain outside this gate.
Serial passes do not close #313. No deployed agent changed or CodeRabbit review
was requested. These are author checks, not an independent Planner grade,
security disposition or production qualification.

## Limits and remaining work

Planner review [5637604779](https://github.com/toddwbucy/WeaverAgent/pull/319#pullrequestreview-5637604779)
cleared parent #319 at unchanged `e8d4a043`: zero fix, eleven batch, one file.
The Planner marked it ready. Its trace/harness gates passed; the parallel Stop
instrument hit the #313 clock signature once, then passed three isolated reruns.
These parent findings remain separate from this child's author evidence:

| Parent review row | Disposition |
| --- | --- |
| 1 | Open: simplify redundant Text observers or pin their independent reachability; add cap, count-failure, ordering and post-count issued-ordinal controls. This child's pair producer is called at reduce_flow before error mapping; its protective mutations cover that site and its failure ordering. |
| 2 | Open: cancelled refusal admission can arm Decision if output began without one. This child's instruments retain an actual original bound; they do not prove absent-bound admission. The correction composition must enforce/check the original wait without renewal. |
| 3 | Open: Text observation must share release's selected/unselected, ownership and timestamp validation. This child adds no Text observation path and does not close that guard carry. |
| 4-5 | Open: gate early Text evidence to issued corrections and unify Recorded storage with the ordinary output slot. |
| 6 | Open: remove the immediate held_output store/take in cancelled output association. |
| 7-8 | Open: pass the exact already-parsed RequestScheme into repair and report its parse failure with the correct refusal classification. |
| 9 | Open: unify duplicate empty-correction and failed-pass identity/count records. |
| 10 | Open: use repair_fault for the count-failure latch mapping. |
| 11 | Disclosed unavailable self-check skills. |
| 12 | Extend #313 with the parallel worker_stop_composition_actual_admin_and_worker boot-clock race and three passing isolated reruns; do not call serial qualification a fix. |

No trace or wire fields, classifier artifact, SPU implementation or active serving
entry point change. The changed completion boundary does not authorize work after
lifecycle/recording permissions expire. An incomplete decoder message, absent
check/refusal/filing or failed terminal construction remains incomplete.

The #312/#314/#315/#317/#318 review maps and #313/#316 carries remain open where
their current records say so. In particular, generic held-caller identity/takers,
healthy rollback/failed-run exits, held-refusal Spec precedence, the release test,
positive-boot detail/test carries and active legacy cutover are not closed here.

Issue #146 still owes correction placement/decode and further worker/lifecycle
composition, pinned production language artifacts, classifier and prompt review,
measured limits, privileged/device qualification, security disposition and the
operator's qualified coupled cutover. Existing serving remains active. New PRs
are submitted as drafts; only the operator merges, deploys or requests CodeRabbit.
Unavailable self-check skills remain disclosed; these author checks are not a
Planner grade or production qualification.
