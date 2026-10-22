# State management inward recall owner

Date: 2026-10-22
Branch: code/state-inward-recall
PR: pending assignment
Epic: #146

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
opening receipt must exist before the inward operation can start. The subject comes
only from that private pair proof. The pure constructor borrows its exact bytes, then
History uses that subject with verb and condition null. It does not trim or normalize.

## Read, terminal and failure custody

The new driver uses a private read purpose tied to the exact CallId. Ordinary read
admission still excludes pair_work. Public advance, take and cancel cannot consume the
inward owner's read, including a released or completed result. Other owner exclusions
remain in force. The explicit controller still drives the existing common pump.

Read acceptance installs its original two-second StateControl bound and transfers the
pair into ExecutingInternal before a nested queue operation can yield. The completed
request-receipt Decision window retires in that same transition. A first-control Busy
or recoverable Leave after acceptance retains the original read and deadline. A Busy
before acceptance retains the still-unexecuted pair under its existing request window.
Partial pages, unrelated progress and cleanup never restart the read bound.

A success retains the private RecordedRead after complete validation, one
acquired-cursor
release and recall Recorded. Its entire original encoded QueryResult becomes the
ToolOutcome::Result content. The pair retains that read provenance after completion.
Nested held statements remain their original sources and are not filed again.

An initial typed read refusal sends no release. A later permitted refusal releases the
acquired snapshot once. Both become ToolOutcome::Refused with the exact compact JSON
object containing code then detail. They are completed inward pairs with outer execution
and reason null. FlowFailed, malformed result, broken seam and applicable expiry remain
component failures, never empty-history success or fabricated operation refusal.

Only the existing terminal filing door constructs CompletedPair and ToolResult after its
exact recorded filing receipt. Consumed read evidence is retained in the pair before
arming the terminal Decision wait. The success path has no intervening clock sample or
recoverable yield. The existing serialized pair owner starts the next call's distinct
request window after this terminal commits. No gate ExchangeId is invented.

A stop or leave accepted before execution keeps the existing not_dispatched account.
After an inward read starts, its explicit internal state prevents a null gate execution
from being mistaken for an unsent request. The read drains to its real result or typed
refusal under its unchanged bound and the earlier lifecycle deadline. Remaining calls
close under the existing cause dominance rules. The caller's delivery policy is separate
from the recorded fate. A failed read retains ExecutingInternal in outstanding_pairs.

The pure helper's EmptySubject mapping is implemented defensively. Checked nonempty
noun spans prevent that path through the current valid-emission port. Its representation
is not evidence of a newly reachable bad-shape execution path. Existing association
continues to refuse invalid recall shape before this owner receives a ready request.

## Freshness boundary

HARNESS-CLOCK locates Cut after the preceding turn closes and before decode work for the
next turn. HARNESS-READ requires prepare_cut before mandatory fresh history, then
retires
Cut before the read deadline starts. Model-elected inward recall is an active-turn
operation with its ordinary bounded read contract. This slice adds no active-turn Cut
or new freshness policy. The between-turn freshness primitive keeps its open-turn and
pair-owner exclusions. Mandatory automatic retrieval composes that primitive separately.

## Instruments and pending gates

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

Sixteen compiled protective mutation recipes are prepared outside the repository. They
cover the two request receipts, deadline retirement, retained admission continuation,
internal pair state, stop mapping, raw result bytes, typed refusal fields, exclusive
advance/take/cancel, the consumed-read handoff and subject normalization. These probes
have not run. Existing non-Clone CompletedPair/FiledReceipt pins and single-grant-door
manifest instruments continue to apply without another constructor.

Two bounded independent Executor readings found no production defect in the draft.
Review corrected a test-only late-refusal code and requested the exact-subject and real
recorder-pressure instruments now included. These readings are not Planner grades.
Lock, compilation, focused/full suites, lint, formatting and mutation execution remain
pending the root's serialized Cargo lane. No test pass or protective kill is claimed.

## Remaining work

No worker/dev_loop, serving activation, instruction policy or final-answer policy is
changed. RETURN-CARRY placement acknowledgement, fresh-result material, review lane,
boot repair, complete journal/place ownership, failed-run integration and coupled
cutover
remain separate obligations. Production artifacts, prompts, corpus qualification and
measured limits remain owed. Synthetic peers and the supplied reference PDF qualify none
of those artifacts. The operator alone merges, and all new PRs remain drafts.
