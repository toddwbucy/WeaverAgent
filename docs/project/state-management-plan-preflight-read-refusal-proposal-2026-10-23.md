# Root plan preflight read-refusal proposal

**Version:** 0.1, 2026-10-23.
**Parent:** Issue 146, state management implementation.

This proposes one missing management outcome before a root plan effect. It is a
proposal for operator ruling and review, with no approval, implementation, production
qualification or cutover claim. The draft branch is
`docs/state-plan-preflight-read-refusal`. Its PR number has not been assigned.
The baseline is main commit 45f8483393ce25af4ade3f0fa7bd96c1e8ed78f1.

## Problem and requested election

A root update must read plan_current and marks before authoring its effect. A preserved
closure also requires the old instruction's held binding slice. These reads use the
same bounded read protocol as other consumers. result_too_large or reportable
result_unavailable is a valid query outcome. It does not mean malformed input, stale
revision or component failure. The existing PlanUpdateCase set has no faithful outcome
for it. The post-effect refresh rule already requires a failure latch, but that rule
applies after a commit and cannot answer this earlier case.

Approve the proposed read_refused PlanUpdateCase before effect admission, with a
required nullable read_code on PlanUpdateRefused. For this case read_code is nonnull,
from the admitted read refusal set, detail is the member's unchanged diagnostic text,
and rejected_blocks is empty. For every other plan case read_code is required null.
Record the same typed lifecycle refusal before answering. Use the refusing leg's
Recorded state.store.measurement as its causal parent and retain the exact request and
completion in the private operation account.

This keeps the existing response wrapper and refusal event. It adds no read selector,
new event, clock, store write or timing field. StateStoreMeasurement carries operation
identity and clocks but no refusal code. Its Recorded sequence cannot alone preserve
the code, and diagnostic detail cannot become an extra machine-coded case.

## Scope of the proposed outcome

| Boundary | Proposed behavior |
| --- | --- |
| Initial plan_current or marks refusal | Admit counter_exhausted, result_too_large, result_unavailable or answer_too_large. |
| Initial statement refusal for preserved closure | Also admit not_found. Snapshot-counter exhaustion remains possible for this read. |
| next after a valid acquired page | Admit unknown_snapshot, wrong_offset, result_unavailable or answer_too_large. Retain the cursor until its matching release completes. |
| release | Require the matching released answer. A cleanup refusal or failure cannot become a clean root operation refusal. |
| Initial refusal with no page | No cursor exists. Send no release and invent no recall. |
| Valid pre-effect refusal completed | Record its code/detail and measurement parent, retain accepted proposals and prior committed cache, answer refused. No effect, retry or new bound. |
| Invalid response, wrong leg/code, bad correlation or observation, dead seam, expiry | Existing failure/lifecycle path, with original evidence. No downgrade to read_refused. |
| Mandatory refresh after an effect commits | Existing component latch, committed receipt retained, no stale successful Observe/show, generation or PlanUpdated. |

The initial set excludes read_busy and cursor-only codes because the sole read owner
has no earlier cursor at that point. Span-only codes do not belong to these selectors.
unsupported_operation and invalid_selector are not normal outcomes of these required,
validated harness selectors. A document's invalid expected seed/revision remains a
root preflight case rather than a deliberately invalid read sent to obtain a refusal.
The generic read protocol keeps its full vocabulary for its other consumers.

The code set follows STATE-PAGES, TYPES-READ and the required query shapes. It does
not copy every ReadRefusalCode into root operation handling. Later next refusals keep
the existing acquired-snapshot rule. No code implies a guessed release, successful
recall or hidden retry. Local checked allocation overflow remains the existing
counter_exhausted plan case with read_code null, distinct from a member read refusal.

The owner must finish all already admitted input dispositions before returning its
clean refusal. Correctly accepted proposal statements remain held. A valid pre-effect
read refusal does not change the committed plan, so it does not revoke the prior
committed cache. Concurrent Stop/leave or foundation failure still controls available
recording and cleanup permissions. Nothing here renews the original read/decision
bounds or permits a cancelled continuation.

## Alternatives and tradeoffs

| Alternative | Tradeoff |
| --- | --- |
| Recommended: read_refused plus required nullable read_code | Preserves a typed root outcome and exact read code with one new case and one field. Requires paired floor, harness, admin and codec changes. Existing plan-refusal cases acquire explicit null. |
| Add read_refused but keep the old three-field shape | Smaller wire edit, but the outcome loses its typed read cause. Measurement has no code, and interpreting detail would contradict PLAN-EXCHANGE. A separate evidence carrier would still need design. |
| Map to malformed_plan, stale_plan, OutOfOrder or ConfigInvalid | Keeps the wire but falsely attributes a valid bounded query outcome to the operator document, revision, activity or configuration. Not recommended. |
| Latch every preflight refusal | Keeps the response set but makes a correct bounded-query result terminate healthy service. That would be a wider change to the read/refusal contract. Not recommended. |
| Automatically retry or narrow the read | Adds policy and may observe a different state or omit required evidence. It also risks renewing the decision bound. Not recommended. |

A caller may investigate the preserved cache and trace and submit a later explicit
operation. This proposal adds no advice to retry blindly, and an unanswered prior
exchange still has unknown commit status. Admin performs no save or publication as a
side effect of this outcome.

## Paired amendments

- [Spec 07](../crates/state-management/state-management-spec-07-plan-forward-flags.md),
  PLAN-EXCHANGE and new PLAN-PREFLIGHT-READ, owns the outcome, exact field combination,
  selector/leg code set, cleanup and pre/post-effect distinction.
- [Types amendment](../crates/state-management/weaver-types-Spec-amendment.md),
  TYPES-PLAN-CONTROL, owns the identical lifecycle/shared refusal shape and strict
  codec obligations. The existing raw lifecycle refusal record carries it to trace.
- [Harness amendment](../crates/state-management/weaver-harness-Spec-amendment.md),
  HARNESS-PLAN, owns the phase transition, same-ticket record, measurement parent,
  retained read account, source settlement and cache behavior.
- [Admin amendment](../crates/state-management/weaver-admin-Spec-amendment.md),
  ADMIN-PLAN, owns one typed answer and log with no retry, rollback or hidden mutation.

The state-member read contract and observation shape do not change. Existing plan
control contract amendments delegate representation and actor states to these clauses.
No additional authority is assigned to the model, connector, loop or state member.

## Evidence required after ruling and merge

Use table-driven real-owner cases for every permitted initial and next code. Include
initial failure without a cursor, later refusal with one release, refused/failed
release, wrong leg/code, malformed observation, pressure before both measurements and
refusal, and exact original decision/read expiry. Pin the typed code, unchanged detail,
empty rejected_blocks and the refusing leg's Recorded causal parent.

Use an existing committed plan, a successful preflight control, then a pre-effect
refusal to prove unchanged prior cache, no effect filing and no component latch. Use
an initially absent plan as a separate control, with a successful prior null cache.
Pair that with an effect that commits followed by the same refused read to prove the
existing post-effect latch and stale-cache ban still hold. Cancel between read refusal
and cleanup/record, retain any admitted sources, and prove no resumed grant or bound.

Floor and admin fixtures must reject missing read_code, read_refused/null, another
case with nonnull read_code, unsupported codes, nonempty rejected_blocks and unknown
fields. Exact records must survive the normal one-answer and trace encoding. Protective
mutations must remove each outcome guard, parent binding or cleanup boundary and fail
its meaningful fixture after compiling. Compilation failure alone proves no behavior.

## Independence and hold

This is docs only. It does not implement draft #260's boot allowance proposal or
#263's cancelled-correction counting proposal. Those remain separate elections.
The previous serving implementation and FOUNDATION-TRANSITION stay in force.
Implementation of this new outcome waits for operator ruling, review and merge.
