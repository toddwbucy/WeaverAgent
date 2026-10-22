# Committed plan summary evidence

## Problem and authority boundary

This Spec proposal for issue #146 items D and E is draft PR #244 on
branch docs/state-plan-summary-evidence. The merged HARNESS-PLAN requires a
plan_current read after boot pair repair and before enter confirmation, then
caching PlanSummary from that snapshot. It already requires refreshing the cache
after every plan-effect receipt before the control answer or next generation.
Observe reads that committed cache.

The merged PLAN-READ snapshot cannot supply the complete summary. PlanSnapshot
holds operator_source and last_source as SourceBlockId. PlanSummary requires the
operator document's digest and the last source's SourcePosition, including its
exact nullable turn. Those two facts are absent. A source ID, current turn or
freshly hashed plan does not prove either one.

The store already retains the operator PlanInputContext and last transition's
source envelope. This proposal supplies a missing read carrier without adding a
store table. Existing selectors provide no arbitrary source-context lookup that
returns both facts. A complete boot summary cannot be claimed from the current
PlanSnapshot alone.

The proposal changes the owning read shape and described consumers together. It
is not implementation authority before operator election, review and merge. The
previous serving path remains applicable until qualified cutover.

## Proposed carrier and validation duties

Add document_sha256:Digest to PlanSnapshot and replace its bare last_source with
SourcePosition. That existing type carries block_id and required nullable turn.
Both plan_current and historical plan return this exact shape.

The member resolves document_sha256 from the held PlanInputContext identified by
that revision's operator_source. It resolves last_source from the stored last
transition's source, preserving null for turnless events. Both belong to the
same immutable snapshot as the revision. The digest is not computed from the
materialized plan, prompt, result encoding or current operator request. The
member owns consistency with these held sources during adoption and read.

The harness checks strict shape, selector/result association and any already-held
trusted expected facts. At boot, absent an independent expected value, it cannot
detect a well-shaped false member digest or turn from the snapshot alone. That
negative belongs to member derivation/adoption tests. This proposal adds no
lookup or claim that a consumer can independently prove unavailable facts.

After its own plan-effect receipt the harness has that effect's exact
SourcePosition. It must match the refreshed last_source, including nullable turn.
Where it also holds a trusted expected operator digest for that revision, from
validated input or prior committed evidence, it compares that digest too. It
cannot construct the expectation from the same response being checked.

At HARNESS-RECALL's Recording/Recorded transition, after HARNESS-READ has completed
snapshot release, the harness projects PlanSummary from plan, status,
document_sha256 and last_source. A valid null current plan means no committed plan
only when no held receipt requires one. A failed mandatory boot read cannot
establish enter success and invokes the existing failed-enter rollback.

After a committed effect, the required refresh must succeed before the control
answer or next generation. Refusal, failed seam, malformed result or disagreement
with a held trusted expectation latches component failure through the existing
foundation path. The pre-effect cache may remain prior evidence but cannot be a
successful committed Observe/show result. The effect remains committed. Neither
failure nor a lost answer licenses rollback or a blind retry. While the operation
is pending, the existing InTransition convention still applies.

The stored last_source key remains a content key. PlanFlags.after remains a
SourceBlockId, supplied as last_source.block_id. The trace provenance walk visits
that block_id at its existing position. Neither digest nor turn adds a source
visit or fabricated span. No selector, SQL schema, event kind, refusal code,
timing field, wait or model policy is added. Strict decoding does not upgrade old
results by inventing a missing digest or turn. Producers and consumers change at
the same eventual qualified cutover.

## Alternatives and requested election

1. Add document_sha256 and use canonical SourcePosition for last_source. This
   keeps the exact source identity together and permits direct PlanSummary
   projection. Consumers needing a SourceBlockId use last_source.block_id. This
   is the proposed carrier.
2. Add document_sha256 and required nullable last_source_turn beside the existing
   SourceBlockId. This preserves more projections but splits the canonical
   source position across fields that must remain associated.
3. Add a source-context query and read again. That needs a larger selector and
   consistency contract and changes the same-snapshot requirement. It is not an
   implementation shortcut under the current rule.

The proposed election includes the producer/consumer validation duties and failed
post-receipt refresh behavior above. It does not elect plan policy, approve
artifacts or qualify serving behavior.

Planner review 5610138872 row 9 also notes an effect-receipt summary carrier that
could keep plan_current for boot alone. That is a separate design alternative.
The full post-effect read, including its decision-bound cost, is already required
by merged HARNESS-PLAN and PLAN-EXCHANGE. This proposal does not introduce that
cost. Replacing it with a receipt carrier would require a separately elected,
paired change to the receipt, store transaction, harness, trace and lifecycle
contracts. This proposal retains the merged read requirement.

## Paired documents and owed instruments

Spec 07 PLAN-READ and PLAN-SUMMARY-EVIDENCE own shape and source projection.
TYPES-READ owns strict presence and encoding. STATE-PLAN-READ owns derivation from
stored provenance. HARNESS-PLAN owns trusted-expectation checks, the
Recording/Recorded cache gate and failure after commit. FOUNDATION-READ pairs the
consumer boundary. PLAN-STORE-AGREEMENT and PERSIST-ENTER pair read and boot duties.
TRACE-RECALL-SOURCES keeps the source traversal exact. The index versions track
every changed document, and their Landing PR fields name #244.

The member's implementation tests must use current and historical revisions,
real save/reload, distinct operator digests and null/non-null last-source turns.
Perturbations hash the snapshot, choose another operator source, or substitute a
transition's turn. Those must fail the producer's derivation/adoption checks.
Later effects retain their revision's operator digest. Replacements use their
own operator_source, not whichever caller input is currently held.

The types fixtures reject absent or malformed digest and omitted required turn.
The harness fixtures use actual held receipts and trusted expected digests to
reject returned source/turn/digest mismatches. They cover delayed
Recording/Recorded, mandatory boot refusal, and failed required refresh after an
actual committed effect. Perturbations must expose premature cache use or a stale
successful Observe/show, PlanUpdated or generation after that failure. A boot
fixture without independent expected values cannot prove false member metadata.

Trace fixtures preserve source order and deduplication with nested last_source.
PlanFlags still compares the original block_id. No raw snapshot or recall record
alone grants plan mutation or enter success. These instruments appear in the
owning state, harness and types amendments as well as Spec 07.

## Review disposition and remaining boundary

Planner review 5610138872 rows 1 and 2 are addressed by the proposed failure and
validation duties. Rows 4 through 8 and 10 are addressed by updated Landing PR
fields, owning instruments, TYPES-READ placement, block_id/foundation pairing,
the uniform Recording/Recorded gate and reflow. Row 9 is the separate receipt
carrier design alternative above, with the merged read requirement retained.

Row 3 identifies an inherited PLAN-READ endpoint limitation: exhausted allocation
refuses counter_exhausted even when the committed plan remains intact. A mandatory
boot summary then cannot succeed. Issue #246 tracks this endpoint decision. This proposal
leaves the refusal and endpoint unchanged and does not add another query or mask
it as no plan.

Proposals #232, #235, #237 and #241 are merged. This branch joins main at
1acf0bdf and reconciles the shared document headers and README versions while
preserving their paired text.

Current-revision checks pass for ASCII, added prose width, local links, paired
versions, preserved Date filed values, Landing PR fields and whitespace. The
second independent reading and root lock check passed. No Cargo result,
runtime fix, production qualification or issue completion follows from this
proposal. Planner review 5610257792 found no remaining review rows; the later
main join at a19d2412 changed shared headers only.

On 2026-10-22 the operator approved the revised proposal in this document:
document_sha256 plus complete SourcePosition, the stated independent-validation
boundary, and component failure after an unsuccessful required post-effect
refresh. The real commit remains evidence and stale success or generation is
forbidden. This records the election; review and merge remain required before
implementation.
