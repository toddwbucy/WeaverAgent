# State management malformed recall refusal owner

Date: 2026-10-22
Branch: code/state-malformed-recall-owner
Base: draft #256, code/state-plan-summary-carrier at ddebb53c
Epic: #146

## Authority and scope

This slice implements merged #241's Spec 02 RECALL-REFUSAL, EFFECTIVE-EMISSION and
correction-attempt accounting, Spec 05 PAIR-IDENTITY and PAIR-OPEN, and the paired
HARNESS-RECALL-REFUSAL, TYPES-RECALL-REFUSAL, STATE-PAIR-FILE and STATE-ADOPT clauses.
Spec 08's existing identity adapter consumes the resulting valid request list.
The refusal kind retains TRACE's existing opaque raw payload and envelope checks.
No trace mirror, selectable projection or event kind is added.

The operator's elected outcome is trace and caller evidence only. An accepted recall
binding with multiple nouns or a governing condition produces no pair, query, subject
selection, ToolResult or new model feedback. Its accepted statement remains in the
effective text. attempted_call remains true. This primitive requests no correction
for that operation refusal and chooses no final answer or subsequent workflow. The
existing caller round bound still limits repetitions.

The implementation remains inside the dormant foundation primitives. Existing serving
and worker/dev_loop orchestration are unchanged. This slice does not establish the
qualified coupled cutover or complete issue #146.

## Identity and recording custody

Checked association reserves every recall ordinal after the recovered external call
count, including refused ordinals. It retains the original Text content index as
call_block and the exact checked StatementRef. External siblings retain their original
CallIds even when repair supplies statements from a later message. Refused IDs never
enter the valid request vector or acquire an opening by inference.

RecallRequestRefusal is a closed, named object with the six fields required by
TYPES-RECALL-REFUSAL. The tagged RefusalRecord arm uses seam recall_request. Its codec
requires positive noun_count, an invalid operation shape, exact fixed detail and valid
decimal identities. The carrier and nested CallId, StatementRef and SourceBlockId
accept maps only. Positional arrays, duplicate keys, missing fields and extra fields
refuse. The codec keeps CallId and StatementRef independent and does not equate their
sequences or infer a subject.
It reuses #256's private map-only decoder and SourceBlockId boundary; no second
local map decoder or redundant nested block guard is retained after the stack join.

The existing per-binding classifier validation rejects a zero-noun Accepted binding
before a checked generation can reach association. A sibling's noun cannot repair that
malformed classifier result. It remains invalid_result/component failure, distinct
from a correctly reported non_ste100 result and from the operation refusal here.

Repair derives pending refusals only from its owned checked original and correction
accounts. It stages the exact event before fallible author admission and uses the
existing single-author port and original Decision bound. The event is refusal on
internal/runtime, subsystem harness, with the same open turn. causal_parent names the
message.assistant that contains its checked statement, not ste100.result.

At the actual Recorded hook, before later clock, intake or lifecycle work, the repair
owner retains the exact refusal and sequence in its private RecordedRecallRefusal view.
Only that receipt retires the request's refusal obligation. EffectiveEmission cannot
transfer while a refusal remains owed or its author continuation remains held. After a
post-Recorded wait, the owner consumes only its matching retained author completion
before retiring that author. Its RepairAccount retains the immutable ordered
view, exact generations and source leases. The refusal itself creates no filing. The
selected accepted Text continues through its ordinary file path.

## Correction, cancellation and failure

A correction that attempts calls records correction_call once and every malformed
recall refusal separately. Any valid requests still take their ordinary pair opening
and not_dispatched/pass_purpose terminal path. Only after those required records and
terminals are accounted does the repair owner charge one failed correction. Several
invalid recalls cannot consume several attempts or become an empty correction.
Exhaustion retains the same refusal account and original/correction evidence.

Stop and leave revoke successful continuation. An unadmitted refusal remains owed,
and an admitted author resumes the same ticket under the same bound. A correction
proof returned after cancellation enters the same retained account before cancellation
can complete. Required recall and correction_call records settle before that account
transfers. Cancelled work supplies no successful emission and charges no new attempt.

An expired bound or component failure follows the existing halt path, which freezes
the owner with its pending event and checked evidence. Recorded receipts survive a
later failure or recoverable lifecycle wait. No new halt reserve, Closed-phase author
permission, timeout interval or model-facing failure policy is introduced.

## State member and consumer audit

Live pair filing now requires the referenced held binding to have exactly one noun,
no condition and exact subject equality. A failure returns existing InvalidPair and
rolls back the source and pair transaction together. Image adoption applies the same
predicate and reports existing OpenFailed for an invalid stored pair, even when its
raw opening, indexed request and rebound image digest otherwise agree. These checks
do not reclassify the verb or add a schema or pair variant.

The existing Spec 08 identity adapter accepts only EffectiveEmission's valid requests.
A refused CallId cannot form SortAct::Recall. Its accepted Text may still be selected
as an Answer under the existing caller policy. The pure recall helper, read port, pair
execution owner, tool-result grant and SPU protocol gain no refusal path or permission.

## Instruments and validation boundary

The strict-codec fixture pins exact bytes, independent identities, required fields,
invalid shape/detail/decimal cases and positional-array rejection. Existing classifier
fixtures cover an empty noun vector in any binding, including a later binding sharing
the same statement span. A compile-time pin keeps RecordedRecallRefusal non-Clone.

Actual recorder/checker/member socket fixtures cover malformed-only output, mixed
original requests with a corrected external sibling, denied sort-call reconstruction,
recorder pressure and repeat lifecycle waits, Stop/Leave before and after admission,
the unchanged Decision deadline, a cancelled correction proof loan, and the actual
Recorded-before-recoverable-wait boundary. A mixed correction proves that every
refusal and valid pair terminal precedes its single attempt charge. Repeated forbidden
corrections reach the existing cap without losing the retained refusal account.
One- and two-refusal fixtures resume a witnessed post-Recorded wait, finish normal or
cancelled accounting, and drive the next output owner. A foreign completion stays
retained and fails the owner instead of being consumed as this refusal's completion.

State fixtures reject compound and conditional live openings without partial rows.
Saved-image fixtures forge consistent recall pair metadata around legitimate held
bindings and rebind the image digest. Adoption must still reject the invalid shape.
Both suites retain a valid single-unconditional-noun control case.

Author gates on Olympus used the pinned toolchain, --locked and private target
/tmp/weaver-146-malformed-recall-target. The nine focused harness tests, two state
tests, strict codec test and shared trace corpus passed. Full suites passed:
harness 1,472 with 18 inherited ignores; state 218 with eight inherited ignores;
types 135 default and 190 with config; trace 160. All four affected all-target lint
gates, formatting, lock and whitespace checks passed. The complete harness suite
used eight test threads. All 28 protective mutations compiled and failed their
intended assertions; each exact hash-checked restoration passed its focused rerun.
They cover identities and shape, receipt-before-transfer/attempt accounting,
post-Recorded continuation cleanup, closed codecs and both state predicates.

Gate logs, mutation recipes/results, source hashes and the published source
archive are deposited under
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/malformed-recall-owner-ddebb53c/.

These are synthetic mechanics fixtures. They do not qualify language artifacts,
classifier behavior, templates, the system prompt, measured production limits or
complete lifecycle integration.

## Author review and after-action

Independent source review found a post-Recorded completion leak: the refusal
receipt was already retained, but its author continuation could survive a wait
and stall the next refusal or output owner. The fix consumes only the matching
completion before retiring that author, and blocks emission transfer while the
continuation remains held. Re-review checked both fixes and the one/two-refusal,
cancelled, next-owner and foreign-completion regressions.

After joining #256, the codec reused its private map-only adapter and strict
SourceBlockId decoder. A fresh read found no blocker in this adaptation. The
mutation targeting the redundant nested block wrapper now removes protection
at the actual shared SourceBlockId boundary instead. Recovery tests must drive
the next owner after a witnessed post-Recorded wait; retaining the receipt alone
does not prove that the shared completion slot has been released.

The repository's code-review and verification skills are unavailable in this
session. No invocation, Planner grade or third-party pass is claimed here.
