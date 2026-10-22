# Malformed recall request proposal

## Election requested

Issue #146 still needs a disposition for an accepted recall Binding with several nouns
or a governing condition. This proposal elects a typed refusal before pair creation,
retaining the reserved CallId, original Text index and exact StatementRef. It does not
select a subject or manufacture a model-facing tool return. The operator has not ruled
on that choice. Review and merge of the owning clauses must precede implementation.

The baseline is merged main c2646e293c8c1bb4376c4b9ea1d41cbebcc080b2. The edited Spec
headers describe the intended documents after merge, not an approval of this draft.
No runtime implementation, serving activation or qualification is part of this act.
FOUNDATION-TRANSITION's coupled cutover remains in force. The pure recall helper in
#190 and its operator hold are unchanged.

## Existing authority and the gap

Spec 02 MODEL requires one or more object spans. BINDING and PORT reject malformed
Accepted evidence before a checked generation exists. An Accepted zero-noun result
therefore follows invalid_result/component failure. A correct non_ste100 judgment can
instead lead to ordinary Text repair. Neither result licenses a guessed recall CallId
or accepted StatementRef. This proposal preserves that boundary and the pinned grammar.

Several nouns and a governing condition are valid general Binding shapes, but RECALL
requires exactly one noun without a condition. Its previous blanket typed-refusal
promise did not say how to carry the identified invalid request. PAIR-OPEN admits
conforming requests, PairRequest::Recall requires a subject, and INTERNAL-QUERY takes
one supplied string. None can represent the invalid operation without choosing data.

The baseline model_requests.rs association reserves the ordinal and retains an
InvalidRecall with identity and shape facts. repair.rs retains RecallGap and
context_repair.rs reports SpecGap. That stop must not be removed by silently discarding
the request. A recall attempted inside a correction is also an attempted call, even
when its shape cannot create a valid pair.

PAIR-SCHEMA already requires checking the held binding's shape, and RECALL already
requires one noun and no condition. The baseline state filing validator checks nonempty
subject membership among held nouns but omits the complete shape. Correcting that
validator and the corresponding saved-image check enforces existing authority. It is
separate from electing this new refusal outcome. The proposal makes the mechanical
check explicit and adds no catalog or PairRequest variant.

## Proposed rule and alternatives

The new floor carrier is RefusalRecord::RecallRequest with seam recall_request and
exact fields call_id, call_block, statement, noun_count, has_condition and detail.
Its detail is exactly `recall requires exactly one noun and no governing condition`.
Its count is positive, and either the count differs from one or a condition exists.
TYPES-RECALL-REFUSAL defines strict representation and Spec 02 RECALL-REFUSAL owns its
meaning. There is no optional subject or string-encoded identity.

The harness authors the existing refusal kind as internal/runtime, subsystem harness,
in the same open turn, parented to the checked statement's message.assistant. It
retains the event and evidence before admission and the receipt before another fallible
step. The private repair account exposes Recorded refusals alongside valid requests.
No refused ID enters a SortAct::Recall. Accepted selected statements still file, and
unrelated requests retain their identities. The original CallId and corrected
StatementRef of a repaired external sibling remain separate identities.

A correction containing malformed recall records correction_call once for that pass,
each identified malformed recall refusal, and the required terminal accounts for its
valid requests. Only after those obligations does the ledger charge one failed
correction. Several invalid calls do not consume several attempts. Cancellation
revokes successful continuation, not accepted author custody. Existing phase permissions
and original bounds apply. There is no new teardown reserve or late success after halt.

| Choice | Consequence |
| --- | --- |
| Refuse before pair creation, proposed | One typed identity carrier and an explicit exception to recall pair creation. The loop can inspect the operation refusal without a fabricated tool result. Valid pair, read and material representations stay unchanged. |
| Pair every invalid request | Add an invalid-recall PairRequest variant carrying exact binding/defect evidence and expand PAIR-OPEN beyond conforming requests. Existing completed/null-reason/ToolOutcome::Refused can carry its terminal with execution null. Store filing/adoption, trace mirrors, read material, result grants and boot repair all need corresponding authority. This is the alternative if a stored pair and model-facing return are required. |
| Reject the language in the checker | Requires a separate grammar and qualification election. Harness cannot relabel a presently valid compound or conditional binding as non_ste100. |

Existing NotDispatched causes describe conforming requests barred by purpose or
lifecycle. NotGranted belongs to missing external permission. EmptySubject describes
an empty string supplied to the pure helper. None means malformed recall shape.
ToolOutcome::Refused remains correct for a valid opened inward call's refusal, but
cannot itself supply the missing opening. Empty history means a query succeeded with
no matches, so it cannot stand for a query that never existed.

## Owning clauses and affected parties

| Owner | Proposed change |
| --- | --- |
| Spec 02 | RECALL-REFUSAL, repair attempt ordering and inspectable private refusal account. Preserve zero-noun invalid_result and effective Text ordering. |
| Spec 05 | Reserve every accepted recall ordinal, exclude invalid-operation requests from pair creation and distinguish valid opened refusals. State the existing valid opening shape check explicitly. |
| Types | Exact RecallRequestRefusal wire carrier using existing identities and strict codecs. No new PairRequest, PairFailure or ProtocolRepairCase. |
| Harness | Checked association proof, source envelope, retained author/receipt, original Decision bound and once-only correction accounting. |
| Internal | Refuse invalid shape before constructing a query. Keep the pure constructor and exact-byte borrow contract. |
| Trace | Carry the existing raw refusal payload with no projection. Harness validates typed domain evidence, trace retains its envelope duty and no floor dependency. |
| Spec 08 | Keep refused request IDs outside identity-sort candidates while retaining accepted statement candidates. |
| State | Enforce one held noun, no condition and exact subject equality on filing and adoption, with rollback. No classifier or new catalog. |
| Harness/state and harness/SPU contracts | State each party's checking boundary without moving grammar judgment, adding a seam or enlarging gate tools. |

Gate, admin, diagnostic, decoder and the pure query library gain no new message or
runtime duty. Classification stays in the SPU, authoring and checked associations in
the harness, and workflow scheduling in worker/dev_loop. A new refusal is not a
placement grant, a generation permission or a final-answer policy.

## Required implementation evidence after approval

| Boundary | Required fixture and protective change |
| --- | --- |
| Shape | Several nouns, one noun with condition, both defects and a valid one-noun request. Selecting or concatenating a subject must fail. |
| Classifier | Accepted zero-noun reply fails before association, while a real non_ste100 nounless Text takes ordinary repair. Creating a recall refusal from malformed Accepted evidence must fail. |
| Identity | Mixed external and inward requests, later valid recall ordinals, and a repaired external sibling. Renumbering CallId or replacing corrected StatementRef with original source must fail. |
| Codec and envelope | Exact tagged payload, positive count and invalid-shape predicates, strict keys, source turn and causal_parent. Losing an identity, changing detail or accepting a duplicate key must fail. |
| Recorder | Actual capacity pressure and a lifecycle wait after Recorded but before continuation. Early exposure, duplicate authoring or lost receipt must fail. |
| Repair | Several malformed recalls plus valid correction calls below and at cap. Missing correction_call, missing per-request refusal or incrementing failed_corrections before complete accounting must fail. |
| Cancellation | Stop and leave before and after author admission, under the original bound. Discarding accepted work, renewing the interval or releasing later success must fail. |
| Excluded effects | Query, state, gate and pair spies on malformed requests. A read, opening, ToolResult or empty-success result must fail. |
| Existing store obligation | Forged compound/conditional openings, transaction rollback and saved-image adoption. Mere subject membership must not satisfy exact valid recall shape. |
| Valid inward call | Exact subject bytes, bounded read/release and a real pair terminal for read refusal. Omitting the terminal or mapping refusal to empty history must fail. |

Compile each behavioral mutation and run its named assertion, then restore the exact
source and require the original test to pass. Representation tests alone do not prove
author custody, lifecycle integration or production classifier qualification.

## Validation and remaining review

This act changes documentation only. Static checks passed for ASCII and 88-column
prose in all 12 changed or new files, all ten README/header versions, all ten preserved
Date filed values, 49 local link targets, added prose without semicolons and diff
whitespace. A second author reading found no concrete semantic contradiction in the
proposed rule. This is not a Planner grade or operator approval.

The lock gate passed. Draft PR #241 retains this proposal and the required evidence
list. No Cargo test, runtime source change or qualification run is claimed. Planner
verification, operator election and merge remain required before implementation.
