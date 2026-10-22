# State management plan-summary carrier

Date: 2026-10-22
Branch: code/state-plan-summary-carrier
Base: draft #255, code/state-open-evidence-carrier at b26420e5
Epic: #146
Authority: merged #244

## Authority and scope

This dormant slice implements the paired PlanSnapshot carrier from Spec 07
PLAN-READ and PLAN-SUMMARY-EVIDENCE, TYPES-READ, STATE-PLAN-READ/FLAGS,
TRACE-RECALL and the corresponding harness/state and persistence amendments.
It changes the existing producer and consumers together. The previous serving
implementation remains active under FOUNDATION-TRANSITION's coupled-cutover hold.

PlanSnapshot now requires document_sha256:Digest and last_source:SourcePosition.
The latter requires block_id and turn, including explicit null for a turnless
source. There is no default digest, omitted-turn compatibility or decoder for the
old bare SourceBlockId. Both plan_current and historical plan use this shape.
PlanSnapshot, SourcePosition and its SourceBlockId decode only from objects;
serde's alternative array representation cannot bypass their named fields.

The state serializer takes the digest from the referenced held operator
PlanInputContext and the complete position from the revision's stored
last-transition content row. These queries run inside the existing immutable
read transaction. The operator source must still be the root operator's input
plan event with its strict typed context. The digest is not recomputed from a
snapshot or prompt. The source run and nullable turn are not replaced with the
latest event, a prior nonnull turn or the adopting run.

Image verification compares the carrier against its held operator context and
transition turn as well as retaining the existing revision/effect provenance
checks. Stored last_source remains a content key; no schema column changes.
PlanFlags.after remains SourceBlockId. A caller derives it with
snapshot.last_source.block_id. Existing checked-successor behavior remains
unchanged; the endpoint election tracked by #246 remains open.

Harness strict decoding and selector validation retain the new evidence. The
existing read owner releases the immutable snapshot before recall Recording and
Recorded permit consumption. Recall traversal visits last_source.block_id in its
existing position. The digest and turn add no source visit. The shared fixture
used by both harness and trace includes the new shape; trace's Recall wire itself
does not acquire new fields.

## Instruments and validation boundary

The focused instruments exercise:

- Strict current and historical codecs with nullable and named turns, and refusal
  of absent/malformed digests, the old source shape, omitted turn, unknown fields
  duplicate keys and array aliases for the carrier or its source objects.
- Real plan filing followed by both selectors, adoption under a different run,
  a later unrelated source, a named transition followed by a turnless transition,
  and a previously serialized immutable result retained through the later effect.
- Replacement revisions with different held operator digests, historical access
  to the superseded revision and a PlanFlags query using the source block only.
- Read and adoption refusal for invalid held operator contexts or attribution.
- The complete encoded result at and one byte beyond its existing byte ceiling.
- Actual harness read/release and recall backpressure, exact result bytes and one
  recall ticket, and malformed responses retained without Release or recall.
- Identical recall source traversal when only the digest or turn changes.

Author gates on Olympus used the pinned toolchain, --locked and private target
/tmp/weaver-146-plan-summary-carrier-target. Focused instruments passed. Full
suites passed: harness 1,463 with 18 inherited ignores, state 216 with eight
inherited ignores, types 134 default and 189 with config, and trace 160.
All four affected all-target lint gates, formatting, lock and whitespace checks
passed. All 15 protective mutations compiled and failed their intended assertions.
Every exact hash-checked restoration passed its focused rerun. The mutations cover
held/revision digest derivation, exact transition source and nullable turn, root
attribution, closed object codecs and the required digest/source traversal. These
author results do not establish production qualification.

Gate logs, both initial fixture failures, the inherited pressure failure and its
reruns, mutation recipes/results and exact source hashes are deposited under
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-summary-carrier-b26420e5/.
The digest derivation decodes one held input context; it does not treat that
input's byte length as the encoded-result limit. The additional transient typed
context allocation belongs in eventual production memory measurement.

## Remaining owner and cutover work

No boot or plan-effect cache coordinator currently projects PlanSummary from
RecordedRead. This slice supplies its required evidence and preserves the
existing consumption boundary; it does not claim that absent caller is complete.
The later owner still owes mandatory boot success/refusal, held expected digest
and exact effect SourcePosition comparisons, current-null rejection when a
receipt requires a plan, and fatal refresh handling after a committed effect.
It must never answer PlanUpdated, expose a successful stale Observe/show result
or start generation after that failure. The effect remains committed.

A well-shaped false digest or turn cannot be detected by comparing the same
response with itself. The harness must use independently held expectations when
available and otherwise rely on the member's source validation. No source lookup,
new refusal, lifecycle success, placement authority or plan mutation was added.
Loop orchestration stays in worker/dev_loop and classification stays in the SPU.
Issue #146 and production qualification remain open.

## Author review and after-action

An independent source reader found no carrier blocker. The repository's requested
code-review and verification skills are unavailable in this session; no invocation,
Planner grade or third-party pass is claimed by the author review.

The first validation runs exposed two fixture errors. The damaged-origin probe
attempted an update that the catalog CHECK already forbids. It now first confirms
that rejection, bypasses CHECK only to construct damaged content, then restores
enforcement before testing read and adoption refusal. The shared selector fixture
now supplies a separate earlier operator event with its coherent envelope and
context path, preserving the original transition's turn and identity. Fixtures
for healthy held state must satisfy its source constraints; corruption probes
must distinguish ordinary write rejection from defensive read/adoption checks.

An inherited pump pressure test hit its five-second socket write timeout in the
default parallel suite. Its isolated rerun passed without a source change, then
the complete harness suite passed with eight test threads. Both failed-run and
passing logs are retained; the full gate does not omit the pressure instrument.
