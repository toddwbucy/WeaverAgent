# State management inward recall owner

Date: 2026-10-22 Branch: code/state-inward-recall PR: #245 Epic: #146

## Authority and scope

This dormant slice implements valid model-elected recall from merged Spec 02 RECALL,
Spec 05 PAIR-STATES, PAIR-END and COMPLETED-PAIR, the internal amendment HARNESS-RECALL,
and the harness amendment HARNESS-READ/RECALL. It joins PR #190's pure query constructor
by ancestry at 76f9a52. The operator's foundation-cutover hold on that PR remains.
Neither this implementation nor the dependency change activates the serving path.

The harness now links weaver-internal for recall::query. Its dependency-set test admits
that pure library and still excludes state and SPU. No gate tool schema, ToolDefinition,
process, transport, automatic schedule or new placement grant is added.

An explicit caller begins one exact ready CallId from EffectiveEmission. Existing
association retains original call ordinals, corrected StatementRefs and source order.
The first unfinished request remains a barrier. Both the actual statement receipt and
opening receipt must exist before the inward operation can start. The subject comes only
from that private pair proof. The pure constructor borrows its exact bytes, then History
uses that subject with verb and condition null. It does not trim or normalize.

## Read, terminal and failure custody

The new driver uses a private read purpose tied to the exact CallId. Ordinary read
admission still excludes pair_work. Public advance, take and cancel cannot consume the
inward owner's read, including a released or completed result. Other owner exclusions
remain in force. The explicit controller still drives the existing common pump.

Read acceptance installs its original two-second StateControl bound and transfers the
pair into ExecutingInternal before a nested queue operation can yield. The completed
request-receipt Decision window retires in that same transition. A first-control Busy or
recoverable Leave after acceptance retains the original read and deadline. A Busy before
acceptance retains the still-unexecuted pair under its existing request window. Partial
pages, unrelated progress and cleanup never restart the read bound.

A success retains the private RecordedRead after complete validation, one
acquired-cursor release and recall Recorded. Its entire original encoded QueryResult
becomes the ToolOutcome::Result content. The pair retains that read provenance after
completion. Nested held statements remain their original sources and are not filed
again.

An initial typed read refusal sends no release. A later permitted refusal releases the
acquired snapshot once. Both become ToolOutcome::Refused with the exact compact JSON
object containing code then detail. They are completed inward pairs with outer execution
and reason null. FlowFailed, malformed result, broken seam and applicable expiry remain
component failures, never empty-history success or fabricated operation refusal.

Only the existing terminal filing door constructs CompletedPair and ToolResult after its
exact recorded filing receipt. On the read path, consumed evidence is retained in the
pair before arming the terminal Decision wait. The success path has no intervening clock
sample or recoverable yield. The existing serialized pair owner starts the next call's
distinct request window after this terminal commits. No gate ExchangeId is invented.

A stop or leave accepted before execution keeps the existing not_dispatched account.
After an inward read starts, its explicit internal state prevents a null gate execution
from being mistaken for an unsent request. The read drains to its real result or typed
refusal under its unchanged bound and the earlier lifecycle deadline. Remaining calls
close under the existing cause dominance rules. The caller's delivery policy is separate
from the recorded fate. A failed read retains ExecutingInternal in outstanding_pairs.

The pure helper's EmptySubject mapping is implemented defensively. Checked nonempty noun
spans prevent that path through the current valid-emission port. Its representation is
not evidence of a newly reachable bad-shape execution path. Existing association
continues to refuse invalid recall shape before this owner receives a ready request.
An empty constructor input retains the remaining request-receipt Decision window while
its refusal terminal files. It does not acquire a read deadline or a fresh terminal
window, matching the never-executed bar path. This is separate from the unmerged #241
malformed-recall proposal and implements none of that proposal.

## Freshness boundary

HARNESS-CLOCK locates Cut after the preceding turn closes and before decode work for the
next turn. HARNESS-READ requires prepare_cut before mandatory fresh history, then
retires Cut before the read deadline starts. Model-elected inward recall is an
active-turn operation with its ordinary bounded read contract. This slice adds no
active-turn Cut or new freshness policy. The between-turn freshness primitive keeps its
open-turn and pair-owner exclusions. Mandatory automatic retrieval composes that
primitive separately.

## Instruments and author gates

Twelve tests drive real checked output, pair filing receipts, the member stream and the
recorder with synthetic classifier/member replies. They exercise:

- Separate withheld statement and opening receipts before any inward state traffic.
- Held Release, recall provenance, terminal Filed and complete original result bytes.
- Initial ResultTooLarge and later ResultUnavailable refusals with exact escaped JSON.
- Two recall calls, mixed recall/external source order and preserved binding ordinals.
- Public read ownership exclusion and stop before or after inward execution starts.
- One original read deadline across progress, old request-window expiry and Leave.
- Forty-eight deterministic Leave checkpoints through admission and terminal handoff.
- Whole shared-corpus history with original provenance and no repeated content filing.
- A malformed result that retains internal-execution evidence and creates no terminal.
- Exact mixed-case, multiword and edge-space subject preservation through the real wire.
- A full real trace writer retaining one recall ticket, payload and read deadline before
  drain allows the one terminal account and committed result.

Sixteen mutation recipes cover request readiness, deadline retirement, retained
admission continuation, internal pair state, stop mapping, raw result bytes, typed
refusal fields, exclusive advance/take/cancel, the consumed-read handoff and subject
normalization. Fifteen compiled and failed their named assertions, then passed after
exact restoration. Removing the statement-receipt term alone survived the context
fixture and is retained separately. The existing lower-level ledger test then pinned
that local defense: its mutation compiled and failed, and exact restoration passed.
Existing non-Clone CompletedPair/FiledReceipt pins and single-grant-door manifest
instruments continue to apply without another constructor.

Two bounded independent Executor readings found no production defect in the draft.
Review corrected a test-only late-refusal code and requested the exact-subject and real
recorder-pressure instruments now included. These readings are not Planner grades. The
final joined production source includes #243 ec4121f, #240 b5b5893 and #239 21bfd08. Its
full harness suite passed 1327 tests with 18 inherited ignored cases. The inward-focused
filter passed 14 tests, including the twelve new owner cases. The internal crate passed
12 tests. Strict all-target harness and internal clippy, formatting and lock passed with
/tmp/weaver-146-inward-target.

The initial nested-Leave fixture expected recovery after a read admitted under that
earlier cap. Its correction requires the unchanged cap to expire even if the Leave key
is later retired. The initial full parallel run also hit the inherited real-writer
EAGAIN fixture failure. That test passed alone, then the full old source passed serially
with 1320 tests. The final joined suite used four test threads and passed all 1327
without skipping that test. Initial lint findings led to boxed ReadCompletion custody
and existing owned-error annotations.

The first opening-receipt mutation survived because the context's pending filing leg
also excludes execution. The regression now inspects the real ledger's local readiness
while that leg is held. Removing the ledger opening check then failed. That assertion
was added after the final full-suite run and passed its focused restoration and strict
lint checks. The statement-check survivor is equivalent through current context
producers. Source-ledger ordering prevents filing ahead of the earlier statement, and
advance_pairs collects that statement before installing the opening receipt. Corrected
requests keep their exact generation lease, and completed predecessors retain their
statement evidence. The existing lower-level ledger fixture admits receipts in another
order and independently pins the defensive predicate. It is not presented as a reachable
context-order failure. No new fabricated receipt or grant was introduced to kill it.

Logs from the initial failures, final gates, every mutation attempt and exact source
hashes are retained under inward-recall in the shared issue #146 evidence deposit. These
are scoped author checks, not Planner grading or production qualification.

## Review follow-up

Planner review 5610306631 at ec3b3409 found zero fix-bucket findings and five test gaps.
The earlier gate and mutation totals above describe that prior source, not this
follow-up. Five additional tests are now validated. On 2026-10-22, private target
`/tmp/weaver-146-inward-target` with locked dependencies passed 19 focused tests,
1344 full harness tests with 18 inherited ignored cases, and twelve internal tests.
Both affected strict all-target lint gates, formatting and lock passed. All eight
compiled protective mutations failed their intended tests and each exact restored
baseline passed. Independent author review found no concrete defect in this bounded
follow-up. No unavailable code-review skill invocation or Planner grade is claimed.

The added instruments cover:

- The exact empty_subject completed refusal, with no read control, recall event or new
  wait. This is a defensive input probe after real receipts. A test-only helper clears
  the derived ready subject while preserving the immutable opening evidence. That
  deliberate inconsistency cannot come from checked noun spans and is not represented
  as a valid model emission or a production empty-noun path.
- A one-shot, per-instance Busy return from the actual begin_control boundary after
  admission. It creates no operation, identity or recording proof. The retained read
  survives and retries under its original StateControl window. Current synchronous
  admission checks all the coordinator's Busy predicates, so this is a defensive
  returned-result probe, not a claim that a competing producer can interleave there.
- Pre-admission Busy caused by a real lower coordinator template check using canonical
  TemplateSpec JSON. The inward retry reduces its actual classifier result to a recorded
  completion while retaining the pair's request window. Its caller takes that proof
  before the inward read admits. This is a defensive coordinator-capacity probe. The
  public template owner excludes pair_work, so this ordering is not claimed as public
  composition. Removing either the retry arm or its reduction must fail this test.
- advance_pairs while an inward owner still holds a newly barred request. It cannot
  author the terminal before inward withdrawal. The ordinary bar account then files
  its actual not_dispatched terminal. Terminal progress includes this withdrawal and
  grants no result by itself.
- Independent readiness assertions before and after actual read admission. Before
  admission the ledger remains ready but the context excludes another execution.
  After admission a direct ledger assertion pins internal_started without relying on
  the outer context guard. Each redundant predicate has its own mutation.

Small cleanup borrows the History selector for comparison, names the dependency test's
actual five-crate set, documents the synchronous driving scope and consolidates repeated
owner unwrap chains. Stage::Terminal remains an explicit retained fatal-handoff state.
A healthy path clears it atomically, and a failed path cannot resume ordinary work.
The existing public read-exclusion tests continue to cover the driving permission.

Payload custody was inspected without a broad representation change. RecordedRead
retains exact raw bytes and parsed provenance. PairEnded, its completed copy and the
ToolResult own separate account and delivery representations. Those repeated payload
allocations remain a batch efficiency item. Removing them needs a measured custody
change across the shared pair grant path, including external pairs. It must not discard
raw evidence or add a result constructor. No unmeasured saving is claimed here.

The lane, readiness, selector and origin checks remain local defensive assertions.
Current private producers already satisfy the remaining survivor preconditions. No
fabricated grant or foreign read was introduced to make those redundant checks fail.
The review's WrongPhase claim remains disproved by pair-owned release exclusion and the
initial live check. Turnless Recall with null causal parent remains the merged
HARNESS-RECALL rule. Changing that attribution would require a separate Spec decision.

Final recipes, runner, source hashes and per-row dispositions are deposited under
`inward/review-batch/` in the shared issue #146 evidence root. The original surviving
probes and their reachable-equivalence explanations remain in the earlier evidence.
The #190 foundation-cutover hold remains.

## Remaining work

No worker/dev_loop, serving activation, instruction policy or final-answer policy is
changed. RETURN-CARRY placement acknowledgement, fresh-result material, review lane,
boot repair, complete journal/place ownership, failed-run integration and coupled
cutover remain separate obligations. Production artifacts, prompts, corpus qualification
and measured limits remain owed. Synthetic peers and the supplied reference PDF qualify
none of those artifacts. The operator alone merges, and all new PRs remain drafts.
