# Worker result delivery and subsequent generation

Date filed: 2026-10-24

Branch: `code/state-worker-return-delivery`, based on exact #322
`9f46cbbc454a57bff0d8db4bb414ff977c7b65f0`. Executor author record for #146;
not a Planner grade or production qualification.

## Result and authority

The dormant worker can select whole completed results and carry them through
their model-facing records, measured placement and a subsequent generation.
The existing pair pool now distinguishes completion, result record and actual
delivery. A cancelled or refused placement does not consume the result. An
acknowledged result cannot be selected again as a fresh result unit.

Implements: Spec 05 RETURN-CARRY and PAIR-END's model-facing record; Spec 06
PLACE-CALLS and COVERAGE-JOURNAL; Spec 08 UNIT's fresh-result path;
Spec 02 C02-3 and HARNESS-CLASSIFY decision/decode clock separation.

| Authority | Author assessment for this join |
| --- | --- |
| RETURN-CARRY | Conforms in the exercised ports: borrowed completed grant or NotReady/UnknownCall/AlreadyDelivered; cancellation and refusal retain it; acknowledgement requires the actual matching completed append. |
| PAIR-END model-facing record | Conforms: whole private ToolResult, tool attribution, exact terminal parent, Recorded before the placement grant, and no duplicate author on retry. The record creates no content filing. |
| PLACE-CALLS / COVERAGE-JOURNAL | Conforms in the tested no-cut worker path: original request and statement must remain kept; preparation sends no generation; the journal creates its receipt only after matching append confirmation. |
| UNIT fresh result | Conforms: only the actual private completion constructs the unit. Held-pair replay still uses its separate immutable read adapter. A cut removing the request rejects the fresh result before mutation. |
| Spec 02 C02-3 | Conforms in task and result process instruments: Busy admission is bounded, successful admission disarms Decision during decode, and OUTPUT starts a fresh full bound. |
| Full serving, selection and lifecycle | Incomplete under #146. Full worker selection/cuts, external execution, healthy final answer and coupled cutover remain separate joins. |

## Ownership and transitions

`PairLedger` keeps each existing Arc<CompletedPair> with its result-record and
delivery sequences. The completion's original CallId, corrected StatementRef,
terminal fate, raw return and receipts remain unchanged. `take_result` borrows
the actual complete result. No caller-supplied receipt, JSON result, trace
sequence or public CallId can construct a completion capability.

The place owner records each previously unrecorded selected result after a valid
measurement and checked fit. It uses the actual grant's whole ToolResultBlock,
with the current turn and original terminal as causal parent. The existing
claimed-author ticket, original Decision deadline, move-owned failure retention
and stopped accounting apply to this event as they do to placement records.
An owed author remains retained under trace pressure and cancellation. No new
overflow queue or timer is introduced. The session pool records that exact
sequence before a later retry can consider authoring the same result.

The append placement record follows these result authors. The worker must then
elect `generate` explicitly. `Journal::complete_generation` mints a private,
non-clone AppendReceipt only after accepting the reserved assembly's physical
append. Acknowledgement checks this receipt, the context origin, the selected
unit's exact CompletedPair identity, returned coverage, and prior result record.
An unrelated generation, an equal public identity from another context, or the
decoder answer without journal confirmation cannot mark delivery.

Cancellation before append leaves the grant available, including after an
already Recorded model-facing result. A later turn can place that same complete
grant once without another call or another result record. A stopped generation
which already physically appended the result carries that actual receipt and
does mark delivery; it grants no successful continuation. Delivery is a physical
coverage fact, not a final-answer decision. Terminal facts remain available for
pair and late-return accounting after delivery.

`FoundationInput::place_results` accepts an explicit ordered, nonempty selection
only at PairsComplete. It rejects unknown, duplicate and already delivered IDs,
and checks request/statement coverage in the current no-cut journal before
consuming the current emission. The developer-loop adapter makes the
same election. The worker retains each previous generation and emission owner
when advancing to the next pass, including original decoder bytes, repair,
sort and pair evidence. It can then repair and sort the new actual emission
through the existing ports. No result is automatically selected or dispatched.

The shared task/result preparation path now treats held-input Busy as a wait,
and generation admission arms Decision before a possible Busy pump. A successful
admission disarms that bound while decoding; OUTPUT starts a fresh complete
decision_wait. This bounds admission without timing the model with Decision. It is not a claim to
fix the separate correction owner's Busy/refusal/cancellation carries in #321.
The current pair road keeps Decision armed at PairReady, so time taken by the
loop to elect the next operation counts against decision_wait; a slow election
can halt at that existing bound.

## Review corrections

Planner review 5638101361 at 64d6460b found one blocking Decision-clock
regression and eleven batch rows. This correction changes the same PR; it does
not claim a regrade. The complete disposition is:

| Row | Disposition |
| --- | --- |
| 1 | Fixed: successful generation admission disarms Decision. Two process instruments hold actual task/result decoder replies while the common clock advances beyond two decision_wait intervals, then require a fresh full OUTPUT bound. |
| 2 | Fixed at the worker precheck: the current journal validates the complete fresh selection before any generation/emission owner moves. Existing cut/coverage instruments exercise the journal refusal; the full worker cut-selection join remains owed. |
| 3 | Fixed: placement wait is sampled after all result authors. Actual trace-pressure instrumentation includes that delay. |
| 4 | Fixed: delivered/foreign/duplicate selection admission queues the placement refusal owner under the existing author ordering. Invalid input remains caller-owned. |
| 5 | Fixed for fresh live-result grants across selected and staged units, before measurement. Two instruments retain both refused inputs and require no decode send. Historical complete-span replay remains governed by its separate repeated-occurrence rule. |
| 6 | Fixed: result_record, record_result and acknowledge_delivery run the frozen check. Each check has an isolated compiled mutation. |
| 7 | Comparison retained deliberately: AppendReceipt holds the journal-issued identity and assembly independently of the pending request. Swapping two genuine completed append receipts fails, while restoring the correct one succeeds. The comparison does more than test presence. |
| 8 | All five named guards are isolated: delivered material, absent result record, Returned coverage, changed result sequence and context origin. Corruption fixtures start with real pair/append receipts; test-only helpers create no production capability. Each guard's removal must fail its instrument. |
| 9 | Fixed: hold_place_return uses one checked completed lookup and preserves Frozen, UnknownCall, NotReady and AlreadyDelivered. Its only production caller, place_results, returns the unchanged CallId selection on refusal. |
| 10 | Shared unit validation, a retained result cursor and a private CallId index avoid rescanning preceding result units and the entire completed pool for each author. Original order and exact CompletedPair pointer checks remain. |
| 11 | Open composition carry: EngineWork retains prior rounds without enforcing the existing 32-round workflow bound. This act neither drops evidence nor invents a different limit. The loop must account for every barred request before its round-limit close; #146 tracks that join. |
| 12 | Required self-check skills remain unavailable; gates and manual review are not represented as those invocations. |

The CallId index is reserved with batch admission and populated only while actual
completed pairs retire. It retains ordered pairs as before. Lookup keys include
run, generation sequence and original call ordinal. New mutation surfaces are
the result cursor, index, frozen checks, duplicate precheck and late timing
sample. The result admission paths, cancelled physical append, stopped-account
retention and prior round owners are exercised by the full harness/process gates.

Correction checks used the same private target and pinned --locked toolchain.
All lock, formatting, strict harness lint and diff checks passed. The full suite
passed **2026 harness tests**, including 22 doctests, with 240 ordinary ignored
(222 delegated and 18 inherited). All **222 delegated cases passed serially**:
gate 46, startup 17, input 19, Observe 14, Stop 16, generation 21, emission 33,
correction 19, inward 24 and result delivery 13. The admin fixture helper was
built; its eight privileged startup cases were not run. #313 remains open.

All **13 compiled protective mutations failed as intended**: rearm Decision
through decode, return delivered material, omit the result-record/coverage/
sequence/context/append-identity guards, omit each of three frozen checks,
accept duplicate fresh results, omit result-author waiting time and skip the
next selected result. Each mutation's patch and output are retained. The restored
source rebuilt and passed all 13 RETURN-CARRY tests, 21 generation process
cases and 13 result process cases. All 668 crate/configuration files match
source manifest `b1a56445df384508ddfa6daa8b0f5fc9e0934902661e7ad23a6b2e00340ef3d4`.

Evidence is in `/tmp/state-146-worker-return-review/`, with exact-head bulk
publication recorded on #323 and #146. Initial compile/test fixture failures
are retained separately: a missing import and non-Hash CallId key, an invented
Staged test accessor, a shared-Arc corruption probe, a reload attempted before
its turn closed, and a clock assertion that omitted six scheduling ticks.
The corrected tests keep actual shared evidence and wait for the true operation
boundary; no runtime validation was relaxed. These checks are author evidence,
not a Planner regrade, security clearance or production qualification.


## Original checkpoint evidence

Six new ordinary port instruments use real completed pair receipts, the trace
recorder, measured grants and matching decoder answers. They exercise actual and
foreign completion proofs, missing journal confirmation, stale material after
acknowledgement, duplicate acknowledgement, cancellation under real trace
pressure, refused measurement, and a retained result crossing a stopped turn.
The existing cut instrument still rejects a result whose request was removed.
The stopped-turn port test consumes an actual lifecycle Stop and authors the
stopped turn close; it is not a claim that the worker's next world-input path is
fully joined.

Twelve new delegated process cases exercise two results, equal request texts,
reverse selection order, barred notices, typed recall refusal, a selected subset,
fit refusal, Stop after selection, Stop after placement, Stop during generation,
Stop after generation, and the actual developer-loop adapter with real clock.
They compare the complete appended messages and result records to the actual
private grants, check delivery states, retain earlier raw generation evidence,
and run the next emission through repair/sort/pair completion. Model and member
responses are scripted doubles; real sockets and process boundaries are used.

Checks and evidence:

Checks used the pinned toolchain, --locked, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, and private target
`/var/tmp/weaver-state-146-worker-return-target`.

| Check | Result |
| --- | --- |
| Lock before/after Cargo, formatting, strict harness clippy and diff checks | Passed |
| Full harness | 2019 passed, including 22 doctests; 238 ordinary ignored |
| Delegated process cases, serial | 220 passed: gate 46, startup 17, input 19, Observe 14, Stop 16, generation 20, emission 33, correction 19, inward 24, result delivery 12 |
| Protective mutations | Nine compiled and detected |
| Source restoration | All 668 crate/configuration files match the full gate manifest |
| Restored focused checks | All 12 result-delivery process cases and six RETURN-CARRY port cases passed again |

The mutations omit worker acknowledgement, mark delivery before append, forget
the result record, accept a missing append receipt, accept an unrelated append,
forget AlreadyDelivered, change the recorded result, release before the result
record, and forget delivery after a cancelled but confirmed physical append.
Every mutant compiled and failed its intended instrument; no build failure was
counted as protection.

The first full gates passed before strengthening one coverage assertion and adding
the receipt compile pin. They and their exact source are archived separately.
The full and delegated gates then ran again on the final source before mutation. Restoration
hashes and the focused rebuild establish that the committed code is that tested
source, not a surviving mutant. Source manifest:
`3a13a9150e6b5a5769a7611e1e06351f00701d403c70f86921daa009cd07eadb`.

Evidence is retained locally in `/tmp/state-146-worker-return/` and deposited
under the already approved Issue #146 evidence root after exact-head publication.
The deposit leaf and hash are in the PR and tracker comment. The initial failed fixture run is preserved. Eighteen inherited harness ignores and
eight privileged admin-startup instruments remain outside these gates. Serial
passes do not close #313's parallel races.


The initial process run passed 11 of 12. The remaining fixture recognized only
the older exact pairs-read-refused mode and returned success for its new return
variant. The corrected selector elects the typed refusal on both test paths.
The initial log is retained; no archived exact source is claimed for that first
failed fixture. Runtime behavior was not relaxed to satisfy it.

## Parent review and remaining scope

[#322 Planner review 5637953634][parent-review] passed at unchanged `9f46cbbc`
with 0 fix and 9 batch, and the Planner marked it ready. The Planner's four
effective perturbations and six surviving defensive/redundant ones remain
distinct from this act's protective mutations. Codex code review also completed
without findings at that same head (comment 6383214429); no security pass is
claimed. Dispositions of all nine rows:

| Row | Disposition |
| --- | --- |
| 1, redundant Stop roads and unpinned selection/next/batch guards | Retained as a batch carry. This result composition does not claim those guards are newly pinned or remove the duplicate Stop owner. |
| 2, selected external request can block the batch | External election/dispatch is the next composition join. The current path remains dormant; this act neither grants external execution nor invents a refusal cause. No production caller selects it. |
| 3, PairReady's Decision wait includes election time | Documented above. No new timer or changed deadline election. |
| 4, lost readiness after inward election faults | Defensive carry for the pair-election join; not claimed fixed here. |
| 5, overbroad plan_break preservation sentence | Corrected in the prior implementation record: only the tested Filed-before-Stop prefix is claimed immutable; an unclaimed terminal remains subject to cause precedence. |
| 6, repeated not_dispatched constructor | Existing cleanup carry in PairLedger. This act changes result bookkeeping, not terminal construction. |
| 7, elected Option versus Recall stage payload | Existing pair-owner cleanup carry. |
| 8, duplicate cancellation flags | Existing emission/pair cleanup carry. |
| 9, unavailable self-check skills | Disclosed below. |

[parent-review]: https://github.com/toddwbucy/WeaverAgent/pull/322#pullrequestreview-5637953634

The worker still lacks external execution/interruption, complete caller selection
and cuts, the loop round-limit/final-answer/world-close composition, full lifecycle and
failed-run teardown. Parent review carries remain open. In particular #313's
parallel races and #316's failed ownership handoffs are not closed by these
successful serial process runs. The current serving path is unchanged.

Qualification still requires the pinned language scheme/dictionary/corpus and
span classifier artifact, independent prompt/certificate review, measured limits,
real SQL/model/device runs and security disposition. Eight privileged admin
startup cases remain unrun. The unavailable code-review and
verification-before-completion skills remain disclosed; invocation is not
claimed. All new PRs are drafts. Review, merge and deployment decisions remain
with the operator. No CodeRabbit review is requested.
