# State management review custody followups

Date: 2026-10-22. Issue: #146.
Branch: code/state-review-custody-followups.
Parent: exact #259 e3833de20642b6adfd5a5f2915d72a90fe0aeda8, retaining
#258, #257, #256 and #255. The join was a clean fast-forward plus disjoint
followup source restoration. Replacement serving remains inactive.

## Scope and authority

This slice answers the contained Planner followups on #255, #256, #257 and #258.
It changes ownership at actual Recorded, closes existing object-codec shapes,
shares held-source predicates and removes avoidable decode-envelope copies.
It adds no event, refusal code, workflow, model-facing feedback or runtime policy.

Implements, as an author assessment pending independent Planner verification:

| Merged section | Assessment and boundary |
| --- | --- |
| Spec 02 EMPTY-CORRECTION, RECALL-REFUSAL and repair accounting; HARNESS-RECALL-REFUSAL / HARNESS-AWAIT-AUTHOR | Conforms for actual Recorded receipt retirement, original identity, cancelled accounting and exact author completion custody. Full worker composition remains owed. |
| Spec 07 PLAN-SUMMARY-EVIDENCE; STATE-PLAN-READ | Conforms for shared held operator provenance, source derivation and distinct corruption diagnostics. Complete boot/effect cache ownership remains separate. |
| TYPES-EFFECT, TYPES-READ, TYPES-PLAN-CONTROL and TYPES-CODEC | Conforms for the existing PlanInputContext, PlanSummary and PlanFlagSnapshot closed named objects. No fields or public response limits change. |
| Spec 05 PAIR-OPEN; STATE-PAIR-FILE / STATE-ADOPT; TYPES-RECALL-REFUSAL | Conforms for one shared exact single-noun predicate and reuse of the existing StatementRef under the named-object gate. |
| TYPES-DECODE; HARNESS-OPEN evidence consumer | Conforms for exact raw evidence, closed answer envelopes and existing NoResidency refusal with retained diagnostic cause. |
| HARNESS-OPEN, HARNESS-OPEN-LIVE, HARNESS-RECORD-LIFETIME; Spec 06 OPEN-FACTS / PLACEMENT-RECORD | Conforms for dormant boot owner ticket custody, unsent cancellation, LIVE revocation and moved placement evidence. Full failed-enter composition remains held by named row 4. |

## Actual Recorded custody

RepairAuthor retains the full staged RecallRequestRefusal. Its Recorded hook
checks that identity against the next owed refusal before retiring anything.
The actual source is saved first, so a mismatch keeps the staged event and its
recorded evidence while taking the existing failure path.

EmptyCorrectionRefusal and Association now update their ledger at Recorded,
before any later wait, Stop or Leave. A late receipt preserves Cancelled and
grants no retry, effective emission or exhausted-success continuation. The
cancelled account retains each actual association/source and the empty
correction's source, refusal source and counted attempt. Unrecorded work remains
unrecorded; a partial association batch does not claim its unadmitted remainder.

| Purpose | Recorded hook | Author completion |
| --- | --- | --- |
| RecallRefusal | Retain actual source, check staged identity, retire the matching owed refusal once. | Clear only the matching generic completion. |
| ProtocolRefusal | Retain source and mark the pending attempted-call protocol duty recorded. | No second semantic update. |
| EmptyCorrectionRefusal | Retain actual source and empty correction, count once, preserve cancellation. | No count or transfer update. |
| Association | Retain the exact typed refusal/source and advance only that batch index; preserve cancellation. | No cursor or ledger update. |

A refused repair keeps its original outcome and Decision bound while the
existing trace-only continuation drains owed recall/protocol records. The
caller advances repair until take_refused_repair yields that original account.
This invents no Stop or pair fate. Exhausted transfer now also refuses while an
author is retained. Pending protocol state resets at every pending-transfer or
replacement boundary, and correction causal parent is supplied before staging.

## Plan and pair source checks

Image verification no longer compares a freshly derived digest and turn with
their own source rows. Those facts hold by construction through the validated
snapshot read. Independent revision/effect/source provenance checks remain.

The shared operator-context read checks the held event, block zero, input/operator
origin, turnless/no-effect context, admitted scheme and root cause. Live operator
effects, public digest derivation and image revision validation use that helper.
Missing transition rows and held non-event rows remain ComponentCorruption with
distinct detail. The malformed carrier test now pins valid transport admission
followed by ProtocolFailed at StateReply after the Recorded measurement, with
no Release or recall.

PlanInputContext, PlanSummary and PlanFlagSnapshot require closed named objects
at their actual Deserialize entry points, including explicit nullable fields.
Live pair filing and image adoption share exact binary subject equality, one
held noun and no governing condition. Refusal payloads reuse StatementRef under
the existing map-only adapter.

## Open carrier followups

Kind-first TokenAnswer decoding delegates directly through the generated typed
decoder over the original map. Only a body-first answer buffers raw outer values
for ordered replay. Both preserve raw evidence spelling. Test-only counters at
the actual buffer helper observe envelope buffers and bytes, not all allocation.
The performance claim is limited to removal of those canonical outer copies.

Opened has one private declaration list and a closed private decoder. No public
inherent decoder bypasses its object gate. Exhaustive equality destructures both
values and compares the raw boxes byte-for-byte. The existing Open consumer logs
only the typed OpenEvidenceError cause; its NoResidency result is unchanged and
a failed diagnostic write cannot panic or replace that refusal.

Planner #255 row 4 needs a factual clarification: decode_strict scans duplicate
keys with a visitor and then performs one typed parse. The visitor retains key
sets, not a second Value tree. Open validation creates one Value tree per box.
Two syntax walks and their memory costs still require qualification; this slice
does not rewrite the shared strict decoder or claim a measured peak-memory bound.

## Boot Open followups

Planner #258 rows 1/2 close two retained-account edges. If withdrawal of a wholly
unsent Open refuses due to premature inbound traffic, stopped accounting abandons
that exchange without advancing a send. A failed author admission before recorder
preparation retains its actual matching ticket, so Failed can account for and
dispose of that exact slot under the original bound.

Owed-Unload admission returns BusyAuthored with the original event and no false
second-author latch. A transferred boot grant keeps reporting Complete after a
later failure, remains non-repeatable, and cannot be cancelled retroactively.
The shared held-lifecycle predicate yields only for cancelling Open Stop/Leave/
JoinLeave directives; a held Observe cannot suppress the ready Open first byte.
Boot Open and boot pair repair share their common busy predicate.

The placement event moves from its owner into the barrier or stopped permit;
the owner retains its actual ticket instead of another prefix-token copy.
Allocation-identity controls cover both live and stopped movement. Necessary
Opened-to-trace and original/prepared recorder custody remain; no peak-memory
qualification is claimed. Tautological trace/request session checks and the
redundant stopped-mark guard are removed. Independent received-echo validation
and effective cancellation demotion of unsubmitted/queued marks remain tested.

Eleven new boot regressions reach B3/B7/B8/B9/B10/B13's distinct surviving guard
sites, unsent failure, pre-prepare clock failure, transferred progress, held
Observe, effective mark demotion and actual event moves. Sixteen boot
mutations target those boundaries, separate from #258's original 21 mutations.

Review row 4 remains a named failed-enter integration hold: ordinary Releasing
permits SessionClosed/Fault/Refusal, while the stopped tap admits only the
specified trace-only account and Unload. The complete composer must retain a
live tap for its ordinary release sequence or prove its narrowed admissions
against the merged contract before activation. This slice does not invent that
policy. Row 5's zero-generation behavior remains unchanged. The operator approved
positive generation for foundation boot in draft Spec #260 on 2026-10-22.
Planner review 5622765455 found no fix-bucket rows; its two prose-wrap findings
are corrected at f0b2cedd. Implementation waits for operator merge.

## Descriptor ownership test isolation

The dedicated-target state gate exposed an inherited parallel-test race in
image_descriptor_is_read_only_regular_and_owned_once. Another test could reuse
the descriptor number after the production owner closed it and before the
assertion probed that number. The existing test now re-executes itself alone,
requires exactly one child test to pass and retains the actual refusal, image
bytes and closed-descriptor assertions. It also checks a read-only directory
refusal and EBADF explicitly. Production descriptor handling is unchanged.
A protective mutation leaks the adopted File after the successful read, leaving
its content intact so the ownership assertion must fail.

## Validation status

Internal source review found no concrete blocker in the state/types, harness
custody and Open carrier changes. Real socket/recorder fixtures cover the four
purposes, post-Recorded waits, Stop/Unload, queued and partial association batches,
foreign completion, refused extraction and next-owner progress. State fixtures
exercise admission rollback, both reads and image adoption. Codec tests exercise
direct, trait, nested and reader paths, with raw duplicate-key inputs.

Final gates ran from this checkout with the dedicated target
/tmp/weaver-146-review-custody-target and locked dependencies. Lock, formatting,
diff and all affected all-target lint gates pass. Results:

| Gate | Result |
| --- | --- |
| weaver-types default / config | 149 / 205 passed |
| weaver-harness full | 1,520 passed; 18 inherited ignored |
| boot Open focused | 37 passed |
| weaver-state full | 221 passed; eight inherited ignored |
| weaver-trace full | 160 passed |
| weaver-spu cuda,gguf lib/bins | 275 passed |
| real loaded decode seam | one passed; actual rendered prompt and emission retained |

The SPU scope here is lib/bins plus the real loaded decode seam, not a rerun of
#259's full 336-test suite. The loaded check used
/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf; the retained SPU model fixture
path was /bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct. Host execution was required
for Unix sockets and CUDA.

All 54 final protective mutations compiled and failed their intended assertions;
every restored control passed and every restored source hash matched. They cover
13 repair duties, 16 state/types boundaries, eight Open carrier/diagnostic checks,
16 boot checks and the isolated descriptor-ownership proof. Compilation errors
and environmental failures are not counted as successful perturbations.

Earlier gates reused #259's target; those logs and 53 earlier mutation results
are retained but superseded by the dedicated-target runs above. Initial logs
also retain the invalid B10 fixture setup, clippy correction, descriptor-reuse
test race and its formatting correction. The B10 replacement ticket now carries
the original valid causal payload, so its actual ticket guard is reached.
The state suite passes at eight test threads after descriptor-test isolation.

Local evidence: /tmp/state-review-custody-gates/ and
/tmp/state-review-custody-final-mutation-evidence/. The published source archive,
hashes, recipes, logs and source reviews are retained under
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/
review-custody-followups-e3833de2/.

The required local review/verification skills remain unavailable; internal
source review is not a claim to their invocation or a Planner grade. These are
author checks, not production qualification. CodeRabbit requests remain
operator-only.

## Remaining work

#256's internal plan filing guard still uses the public read result ceiling and
belongs to the complete PlanSummary owner act. Repeated plan derivation and the
bounded recall-refusal scans remain measured efficiency work. #246 retains its
merged endpoint refusal behavior pending an operator decision. The map records
implementation separately from operator landing.

Worker/lifecycle composition, the complete journal/place/generation and plan
owners, production language/model/prompt qualification, measured limits and
coupled cutover remain required for #146. This slice does not close that issue,
merge a PR, deploy an agent or activate replacement serving.
