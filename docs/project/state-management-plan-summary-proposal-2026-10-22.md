# Committed plan summary evidence

Date: 2026-10-22. Spec proposal for issue #146, items D and E.
Branch: docs/state-plan-summary-evidence.

## Problem and authority boundary

The merged HARNESS-PLAN requires a plan_current read after boot pair repair and
before enter confirmation, then caching PlanSummary from that snapshot. It also
requires refreshing the committed cache after each plan-effect receipt before the
control answer or next generation. Observe reads that cache.

The current PLAN-READ snapshot cannot supply the complete summary. PlanSnapshot
holds operator_source and last_source as SourceBlockId. PlanSummary requires the
operator document's digest and the last source's SourcePosition, including its
exact nullable turn. Those two facts are absent from the snapshot. A source ID,
current turn or freshly hashed plan does not prove either one.

The store already retains the operator PlanInputContext and the last transition's
source envelope. This is a missing read carrier, not a request for new durable
provenance or another store table. The existing selectors provide no arbitrary
source-context lookup that returns both facts. A complete boot summary cannot be
claimed from current PlanSnapshot alone.

This proposal changes the owning read shape and all described consumers together.
It is not runtime implementation authority before operator election, review and
merge. The previous serving path remains applicable until qualified cutover.

## Proposed carrier

Add document_sha256:Digest to PlanSnapshot and replace its bare last_source with
SourcePosition. That existing type carries block_id and a required nullable turn.
Both plan_current and historical plan return this same exact shape.

The member resolves document_sha256 from the held PlanInputContext identified by
that revision's operator_source. It resolves last_source from the stored last
transition's source, preserving null for turnless events. Both are resolved within
the same immutable read snapshot as the revision. The digest is not computed from
the materialized plan, prompt, result encoding or current operator request.

After complete checked paging, Released and recall Recorded, the harness projects
PlanSummary from plan, status, document_sha256 and last_source. A null current plan
means no committed plan. A refused or malformed read cannot substitute null or
replace the previous committed cache. Boot failure still refuses enter.

The stored last_source key remains a content key. PlanFlags.after remains a
SourceBlockId. A caller deriving that explicit query uses last_source.block_id.
The trace provenance walk visits that same block_id at its existing position.
Neither the digest nor the turn adds a second source or a fabricated span.

No new selector, SQL schema, event kind, refusal code, timing field, wait or model
policy is introduced. Existing read and adoption failures retain their existing
handling. The strict wire shape does not silently accept a missing digest/turn or
upgrade old results by inventing evidence. Producer and consumer changes belong
to the same eventual qualified cutover.

## Alternatives and requested election

1. Use the canonical SourcePosition for last_source and add document_sha256.
   This keeps the complete source identity together and lets PlanSummary be a
   direct projection. It changes existing last_source consumers to block_id where
   they require only a SourceBlockId. This is the proposed choice.
2. Keep last_source as SourceBlockId and add required nullable last_source_turn
   beside document_sha256. This preserves more existing projections but splits
   the canonical source position across fields that must remain associated.
3. Add a source-context query and read again. This requires a larger selector and
   consistency contract and changes the requirement to derive the cache from the
   same snapshot. It is not an implementation shortcut under the current rule.

The operator's election concerns that carrier and its exact provenance. It does
not elect a plan policy, approve artifacts or qualify serving behavior.

## Paired documents and owed instruments

Spec 07 PLAN-READ and PLAN-SUMMARY-EVIDENCE own the shape and source projection.
The types amendment owns strict field presence. STATE-PLAN-READ owns derivation
from stored provenance. HARNESS-PLAN owns Released/Recorded-before-cache and the
previous committed cache. The harness-state and persistence amendments pair the
read/boot obligations. The trace amendment keeps the source traversal exact.
The index versions track every changed owning document.

Implementation must test current and historical revisions before and after real
save/reload. Use distinct document digests and both null and nonnull last-source
turns. A later effect changes last_source without silently changing the original
operator document digest. Replacing a revision changes the digest according to
its own operator_source, never whichever update the caller happens to hold.

Required negative cases include missing or malformed digest/turn, foreign source
association, snapshot bytes hashed instead of the document, current-run turn
substitution, a read refusal mistaken for no plan, and cache replacement before
Released or Recorded. Exact recall-source order and deduplication must survive the
nested last_source shape. PlanFlags must still compare the original block_id.
No raw snapshot or read record alone grants plan mutation or enter success.

The proposal's document checks and independent review are recorded on its PR.
No Cargo test, runtime fix, production qualification or issue completion follows
from this document.
