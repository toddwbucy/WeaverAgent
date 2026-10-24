# Cancelled correction attempt accounting proposal

Date: 2026-10-22. Issue #146. Proposal only; no implementation authority or code change.
Base: main `45f8483393ce25af4ade3f0fa7bd96c1e8ed78f1`.

## Election requested

[Planner #261 row 5][review] asks whether cancelled empty and nonempty correction
passes should use one failed-attempt rule. The reviewed head `ef32e86b` counts an empty
correction when its refusal records, but a cancelled attempted-call correction does
not count when its pair account returns. That makes a complete call pass depend on
whether cancellation precedes the ledger's final hook. The grade has zero fix-bucket
rows; this is its explicit Spec question.

The proposed election is: a failed requested correction counts once when its complete
checked message, all required Recorded refusals, and all actual required CompletedPair
proofs are retained, even if Stop or leave has revoked continuation. A CompletedPair
includes the real opening commits and terminal Filed receipt with recorded observations.
A terminal trace record alone is insufficient. The count belongs to the original
rejected source and exact correction identity, never to extraction or polling timing.

This preserves [#213's merged empty rule][empty]: its exact Recorded refusal is the
complete empty-pass boundary, with no checker or pair fabricated. It changes the
nonempty/call cancelled-completion rule in the reviewed implementation; that change
must wait for this approved, reviewed proposal to merge. Existing repair custody
followups deliberately leave both counting paths unchanged.

## Exact boundaries

| Facts and cancellation timing | Proposed retained count and evidence |
| --- | --- |
| Stop/leave before a complete requested correction exists | No attempt; preserve incomplete generation/check custody under existing bounds. |
| Complete all-accepted, nonempty, call-free correction | Not a failed attempt. Cancellation grants no successful continuation. |
| Empty correction; exact refusal still unrecorded | No attempt yet. Same held refusal and original bound remain owed where lifecycle permissions allow. |
| Empty correction; exact refusal Recorded before or after cancellation | One failed attempt, preserving #213. No retry or new cap continuation after cancellation. |
| Defective nonempty correction; any required checker/refusal missing | No attempt yet, regardless of how many blocks already completed. |
| Call correction; all refusals Recorded but a required opening commit or terminal Filed receipt is missing | No attempt yet. Recorded PairEnded alone does not supply CompletedPair. |
| Two calls; one terminal completed before Stop/leave, second incomplete | No attempt yet; retain the first actual cause and the second owed disposition. |
| All required facts complete, then cancellation before a later ledger hook or extraction | One failed attempt. Caller timing cannot erase or create it. |
| Last required fact completes after cancellation through existing healthy-flow permissions | One failed attempt in retained cancelled evidence only; do not reopen repair. |
| Pass already counted; later Stop or leave | Preserve that one count; no replay or second increment. |
| Restatement instruction refused or actual runtime-template check rejects | Retain exact operation-refusal/check evidence; no new failed correction without a complete requested correction and its required accounting. |
| Applicable bound or foundation halt prevents the remaining facts | Retain incomplete evidence; no new filing or fabricated completion. Foundation halt behavior is unchanged. |

The failure predicate remains correct defect, attempted call, or EMPTY-CORRECTION.
An operation's cancellation is not itself a classifier defect. Multiple failed Text
blocks, malformed recalls or conforming calls in one correction never consume multiple
attempts. Ordinary accepted Text filing does not add a new barrier beyond
DECISION-BARRIER; Spec 05's required send and pair receipts still do.

PassPurpose bars correction dispatch. A Stop or Leave already accepted owns only a
terminal not yet admitted to its author. A pending admitted record keeps its exact
source, bytes and cause. Thus a two-call batch may truthfully retain an immutable
PassPurpose prefix and Stop suffix, or an immutable Stop prefix and Unload suffix.
The proposal makes that boundary explicit in Spec 05 instead of rewriting history to
force one cause across the batch.

After cancellation, even a count reaching the cap grants no RestatementRequest,
EffectiveEmission or RepairExhausted, starts no decode, and authors no new
repair_exhausted continuation. An already Recorded cap event stays retained without
replay. No accounting or receipt renews Decision or an earlier lifecycle deadline.
This election adds no recording, classification or filing permission after an existing
phase boundary and no new store-write permission during foundation halt.

## Alternative and the choice not proposed

The alternative retains the reviewed implementation's timing rule: explicitly elect
#213 as an empty-pass special case, while cancellation before the nonempty/call
ledger-completion hook prevents that pass from counting even if its last actual pair
receipt already exists. This is implementable, but its count depends on continuation
scheduling and the two failure classes remain observably different.

Counting neither type after cancellation would reverse #213's explicit Recorded
empty-refusal rule. It is not treated as an incidental cleanup. The preferred election
preserves that rule and defines complete accounting precisely for other failure shapes.

## Paired documents and verification owed

The concrete proposal edits [Spec 02 REPAIR, EMPTY-CORRECTION and the new
CANCELLED-CORRECTION clause][spec02], [Spec 05 purpose precedence and
COMPLETED-PAIR][spec05], and [HARNESS-RECALL-REFUSAL][harness]. The Spec headers describe
the intended merged document state under Working Process section 2; this unmerged
proposal itself carries no implementation authority. Landing metadata names draft
PR #263. The operator approved complete-accounting count in comment 6357978329;
Planner follow-up 5623970858 closed the wording rows. Implementation awaits merge.

A census of failed_corrections and correction-count references found no additional
wire field, trace kind, state operation or owning contract to change. Historical
project records, including the 2026-10-21 empty implementation note, remain evidence of
their author-tested revisions, not independent normative cancellation elections.

Implementation after merge owes actual-owner tests for every table boundary,
including exact empty/call identity, delayed terminal Filed receipts, mixed immutable
causes, late completion before account extraction, no count for a partial multi-call
suffix, at-most-once behavior below and at cap, original-bound expiry, and no cancelled
success/cap continuation. Protective changes must lose the corresponding invariant.
This docs-only proposal executes none of those tests and claims no implementation pass.
The full state-management cutover and production qualification holds remain in force.

[review]: https://github.com/toddwbucy/WeaverAgent/pull/261#pullrequestreview-5623117895
[empty]: https://github.com/toddwbucy/WeaverAgent/pull/213
[spec02]: ../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
[spec05]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md

## Integration with merged release-absence wording

On 2026-10-23, main at f4482c60936efb6b7b38dd888fb61f25f8a953ed
was joined into this proposal without rewriting its history. The header
conflict keeps this proposal as Landing PR and advances the harness
amendment from merged version 0.30 to proposed version 0.31. Date filed
retains each owning document's date from main under the operator's
first-filing convention. Both the proposed clauses and the merged
HARNESS-RELEASE-ABSENT / TRACE-RELEASE-ABSENT wording are retained.

Exact body comparisons preserve the previously reviewed Spec clause text;
the harness body additionally contains the unchanged merged absence
clause. Documentation checks accompany this conflict resolution. At this
checkpoint the five proposals were separate main-based PRs; later landings
must incorporate preceding merged clauses and advance shared versions again.
The proposal remains unmerged and grants no implementation authority.

## Integration after the Enter-window merge

On 2026-10-23, #282 merged as main commit
8010e16e64e137a28c239800c1e9724ae80c27b5. This proposal now joins that
exact base, retaining the merged Enter-window and release-absence clauses.
The harness amendment advances from 0.31 to 0.32. Where this proposal
also changes another shared amendment, its version advances from main
likewise. Landing PR still names this proposal; filing dates stay fixed.

The manual resolution changes only conflicting headers. The clause bodies
match the clean three-way combined text byte-for-byte. Lock, document,
ASCII and diff checks pass. No runtime source changes or proposal merge
is claimed. Other proposals remain separate and need the same version
sequencing as each preceding proposal actually lands.
