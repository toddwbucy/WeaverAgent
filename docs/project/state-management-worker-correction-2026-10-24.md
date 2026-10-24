# Worker correction placement and decode composition

Date filed: 2026-10-24

Branch: `code/state-worker-correction`, based on exact #320
`1790f66de36e3f868f194e1b20966af776f3a349`. Executor author record for #146;
not a Planner grade or production qualification.

## Result and authority

The dormant foundation worker can now place its actual restatement instruction,
generate the correction, and return its checked output to the same repair attempt.
An issued restatement alone neither measures nor decodes. The loop explicitly
elects `place_restatement`, then `generate_correction`. The developer-loop adapter
also elects the required refused close after a placement refusal.

Implements the following merged sections, with the scope of the author assessment
limited to the new joins and the instruments below:

| Authority | Assessment and evidence |
| --- | --- |
| Spec 02 REPAIR and EFFECTIVE-EMISSION | Conforms in this composition: actual certified instruction, unchanged rejected argument, checked corrections and original CallIds with corrected StatementRefs; no decoder call in RepairLedger. |
| Spec 02 DECISION-BARRIER and CANCELLED-CORRECTION | Conforms for the exercised waits and completed facts: filing blocks placement; no Decision wait runs during correction decode; started correction checks finish for accounting after Stop. In-flight decode cancellation drains the exact exchange. |
| Spec 06 PLACE-PORT, PLACE-CALLS and PREFIX-PREPARE | Conforms in this no-cut join: one actual filed instruction unit, exact source context, measured delta and schemas, private single-use placement and actual matching generation. |
| Spec 06 FIT-REFUSAL; harness HARNESS-OPERATION-CLOSE | Conforms in the worker's explicit close path: retain the actual refusal, finish original conforming pairs, record the refused turn close, then send the original gate exchange's close using the existing renderer. |
| Spec 05 PAIR-END and PAIR-STATES | Conforms in refusal cleanup: original accepted requests retain actual filing and terminal evidence; the worker's terminating pass bars dispatch with PassPurpose, while accepted cancellation retains the existing cause precedence. No tool executes in these instruments. |
| Harness HARNESS-MATERIAL, HARNESS-PLACE and HARNESS-HELD-BOUNDARY | Actual committed material and retained grants cross the existing ports. Held decode admission returns Busy with the same grant. |
| Full serving, lifecycle and production qualification | Incomplete under #146. This does not activate the replacement serving implementation or qualify any production language artifact. |

## Ownership and continuation

The instruction watcher registers one finite source lease immediately after the
selected instruction enters the ledger, before classification or filing can dispose
it. Its successful check carries the lease into the issued RestatementRequest.
The lease does not turn Recorded into Filed and is not a placement capability.
Dropping interest leaves ordinary selected filing debt intact.

The worker advances the actual filing owner, then moves its exact CommittedSource
into the selected unit. The same unit retains the filed source through measurement,
placement and generation. It is not reconstructed from the instruction text or an
event ID. The actual instruction text must equal the certified expansion.

A restatement belongs to one private repair-account identity, source, attempt and
recorded instruction. The narrow context bridges validate that ownership before
borrowing the repair continuation. They preserve the current journal without a cut
and accept only the actual instruction's unit. Generic placement remains blocked
by the repair owner. No broad bypass or public proof constructor is added.

Each completed correction retains its restatement, placement and decoder account
in the worker while its checked output moves into RepairLedger. Correction
association uses the request scheme already retained by that repair owner. The
original generation, original call IDs, corrected statement references, and all
previous failed attempts remain owned. The correction list is bounded by the
existing source and retry limits; it is not a retry policy of its own.

Stop during an already started OUTPUT check finishes that check and its required
accounting. The cancelled GenerationCompletion exposes only the actual checked
output for that return; it grants no successful continuation. Stop before checking
has begun drains any decode and retains its account without inventing checks or
counting an incomplete correction. Existing repair cancellation retains complete
failed Text, empty and attempted-call passes under #319/#320.

## Refusal close

A placement primitive used by repair cannot close the turn ahead of still-owned
original requests. It returns its recorded refusal and full account to the worker.
`close_correction_refusal` withdraws only the exact ungenerated attempt, recovers
its repair account and certificate, and settles every original conforming request.
The developer-loop placement adapter explicitly chooses this path, as required by
FIT-REFUSAL. The source and actual refusal stay retained until the original world
close is fully sent. A malformed-delta refusal creates no component-failure latch.

The original rejected call has no accepted statement and is not given a fabricated
pair. Its accepted recall sibling does receive a real opening and terminal. Tests
require that terminal, not merely an ordering predicate over a possibly empty set.
An accepted Stop during the owed refused close cannot create a second turn close:
the earlier refusal settles, and management reports AtRest.

The pair driver now pumps only for PairProgress::Pending. Pumping after a Progress
transition could wait for nonexistent readiness while the next pair transition was
already runnable. A real-clock worker refusal instrument exposed this stall; the
corresponding fake-clock circuit alone did not detect it.

## Instruments and review surface

Nineteen delegated cases use actual worker, gate, classifier, decoder and recorder
owners, with scripted model and state-member responses. They cover:

- successful correction and outbound identity sort, including the actual
  developer-loop adapter and physical delta/schema equality;
- held instruction filing, explicit placement/decode elections, and held caller
  plus template input at decode admission without replacing either input;
- failed Text, empty and attempted-call corrections followed by a successful
  retry, plus retry-cap exhaustion;
- Stop at issued instruction, placed grant, in-flight decode and started checks;
  failed Text, empty and call corrections retain one completed attempt after Stop;
- fit and typed measurement refusals, actual worker close delivery, and Stop
  while refusal cleanup's terminal filing is held.

An ordinary ledger instrument also rejects a restatement from another owner with
identical serialized IDs, a prior attempt, cancellation and a frozen ledger.
Existing non-Clone proof pins continue to apply.

The source lease adds no trace fields or wire changes. The checked watcher variant
is boxed to avoid enlarging the unselected enum arm. The two bridge errors retain
the existing port's move-owned input shape, including recoverable Busy.

Initial runs are retained honestly: a missing filing reduction stalled placement;
an empty-correction fixture initially made the original generation empty; a
proposed unselected-instruction fixture failed the existing partition admission
rule before serving and was removed; and the real-clock refusal fixture detected
the pair Progress stall. Earlier failed fixture snapshots are not claimed.

## Validation

Checks used the pinned toolchain, --locked, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, and private target
`/var/tmp/weaver-state-146-worker-correction-target`.

| Check | Result |
| --- | --- |
| Lock before/after Cargo, formatting, strict harness clippy and diff checks | Passed |
| Full harness | 2013 passed, including 22 doctests; 202 ordinary ignored |
| Delegated process cases, serial | 184 passed: gate 46, startup 17, input 19, Observe 14, Stop 16, generation 20, emission 33, correction 19 |
| Protective mutations | Nine compiled and detected |
| Source restoration | All 665 crate/configuration files match the full gate manifest |
| Restored focused checks | Exact-owner ordinary test and all 19 correction process cases passed again |

The mutations remove the instruction receipt, filing progression, private owner
identity, explicit decode election, cancelled-check continuation, held-decode Busy,
ready-pair progression, pair-before-close accounting, and deferred primitive close.
Each failed its intended behavioral instrument after compiling; no compiler error
was counted as a detected protection.

The full and delegated gates ran on the same source before mutation. Restoration
hashes and the focused rebuild establish that the committed code is that tested
source, not a surviving mutant. Source manifest:
`d71ccc128efaf21b4bc3bcd58c3cc2187b41a4f1889ce5fd754bfcc42b04d6c3`.

Evidence is retained locally in `/tmp/state-146-worker-correction/` and deposited
under the already approved Issue #146 evidence root after exact-head publication.
The deposit leaf and hash are in the PR and tracker comment. Initial failed runs
and lint findings are preserved separately. Eighteen inherited harness ignores and
eight privileged admin-startup instruments remain outside these gates. Serial
passes do not close #313's parallel races.


## Remaining scope

Parent [#320 Planner review 5637748755][parent-review] passed at unchanged
`1790f66d` with 0 fix, 10 batch and 1 file. The Planner marked it ready. Its gates
passed, including all 165 prior delegated cases in parallel. Seven of twelve
single mutations bit; both paired guard-removal experiments survived. This act
does not describe those defensive guards as mutation-proven.

| Parent review row | Disposition |
| --- | --- |
| 1, defensive guard tests | The public accounting roads order all required refusals before pairing; the protocol/recall flags, cancelled-loan extraction and cancelled/Ended receipt checks are additional defensive layers. The review did not reach them even by removing both refusal guards. This record distinguishes construction evidence from defensive guard coverage; additional fault-injection coverage remains a batch. |
| 2, terminal take bound/preconditions | Carry: the early terminal transfer is the ordinary accounting observation road, not merely a late-error fallback. Its missing recorded-time bound check and differing release preconditions remain open. |
| 3, rejected terminal diagnostic | Carry: retain the actual install error and reconcile the taken ReceiptSet, reservation, filing and leg on failure. |
| 4, failed accounting cleanup | Carry: the refused-accounting early-error paths can leave observer state installed. Clear it on every exit or suppress the observer after failure. |
| 5, cancelled pending representation | Clarified: a fully accounted cancelled call correction is returned in RepairCancelled.pending, outside account.corrections. Pending does not imply that its pairs remain unfinished. The worker retains this owner and its completed attempt record; it grants no dispatch. The internal pending_calls_accounted flag is not a public mutable witness. |
| 6, withheld cancelled terminal | Filed under #316 with exact pending-call debt, original Decision/earlier Leave bound, first timeout and complete failed-run exit acceptance. No new reproduction or closure is claimed. |
| 7, call cap guard | Carry: the stage gate currently bounds call attempts; a defensive failed >= cap check remains owed. |
| 8, cancelled correction slot/result | Carry: assert the destination slot is empty and propagate the inner pending-accounting result. |
| 9, repeated proof scans | Carry: record that this exact loan was counted, avoiding repeated complete proof-vector construction until extraction. |
| 10, receipt side channel/flags | Carry: unify the actual recorded receipt with the normal completion port and move accounting flags with the loan, retaining late-failure facts. |
| 11, unavailable self-check skills | Same disclosed process limit below. |

[parent-review]: https://github.com/toddwbucy/WeaverAgent/pull/320#pullrequestreview-5637748755

This joins correction placement, decode, checked return and fit-refusal cleanup.
It does not implement tool execution, selection/cuts, the complete Leave/SavePoint
and failed-run lifecycle, healthy final-answer policy, or coupled cutover. Those
remain separate #146 duties. Existing serving remains active.

The #319 review carries mapped in #320 remain: absent-bound cancellation admission,
early Text road/timestamp validation, guard/mutation gaps and representation
cleanup. This composition reuses the existing scheme for corrections; the original
worker-emission/begin-repair double parse remains. The new held-decode test addresses
that admission site, not every held boundary tracked by #316. The parallel fixture
races in #313 are not closed by serial delegated passes.

The pinned language scheme, dictionary/corpus, classifier artifact, independently
reviewed system prompt, measured limits, privileged/system qualification and
security disposition remain required. Unavailable code-review and
verification-before-completion skills are still a disclosed process limit; their
invocation is not claimed. Independent review, all merges and deployment remain
the operator's decisions. No CodeRabbit review is requested by this act.
