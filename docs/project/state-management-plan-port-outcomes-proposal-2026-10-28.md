# Plan-operation refusal proposal

**Version:** 0.1, 2026-10-28.
**Parent:** Issue 146, state management implementation.

This proposes the missing refusal outcomes for the non-root plan ports. It is a
proposal for the operator and Planner, with no implementation or qualification claim.
Draft PR #384 uses `docs/state-plan-port-outcomes`, based on main
`f6f707799e2b0349180582032bc909a1e021a383`. The runtime audit also read draft #383
at `dee6c94a13053c34455fa70e0ca7cf5b5669e07c`. The original serving implementation
remains selected. Implementation waits for the operator's election, review and merge.

## Problem and proposed election

Spec 07 PLAN-PORT returns PlanOperationRefused with PlanBreakCase as its cause.
That set names eight plan-domain failures. It has no read resource refusal, local
allocation exhaustion or correctly rejected template. PLAN-STATES nevertheless says a
correct STE100 defect refuses the operation without halting. Step activation needs the
shared mark allocator and required reads before its effect. A bounded read can refuse
without any plan, category or source being wrong. Root plan-update has explicit cases
and cleanup rules from #268, but its lifecycle ask and refusal do not identify an
internal activation or closure request.

Elect a separate PlanOperationCase containing the existing eight spellings plus
read_refused, counter_exhausted and template_rejected. PlanBreakCase stays unchanged.
PlanOperationRefused becomes {case,detail,read_code}, with required nonnull read_code
only for read_refused. The code and detail are the finalized read's unchanged values.
Local allocation overflow has read_code null. Correct template rejection retains its
complete source dispositions and existing state_template_rejected fault before the
operation refusal. No one of these outcomes asks the primitive to break the plan.

Elect a typed RefusalRecord::PlanOperation under the existing refusal event. It names
the exact accepted operation, expected plan, optional step and returned refusal. Read
refusals parent the refusing leg's Recorded measurement. Template rejection parents its
Recorded fault. Other cases parent the latest Recorded read proof consumed by that
operation, or null when none was consumed. The record is metadata with no selectable
content. This is an internal result and trace carrier, not a new management request.

Both changes are part of this election. A result whose diagnostic text encodes an
untyped cause would not meet it. A measurement alone has no read refusal code, so it
cannot replace the exact typed refusal record. The new operation case also cannot be
cast into a stored plan break. Callers still elect every subsequent operation.

## Outcome and cleanup boundary

| Boundary | Proposed result |
| --- | --- |
| Required initial plan_current, plan, marks, statement or span_preview read | Resource codes counter_exhausted, result_too_large, result_unavailable and answer_too_large become read_refused. Domain checks retain their existing meanings. |
| next after an acquired page | unknown_snapshot, wrong_offset, result_unavailable and answer_too_large retain that cursor until matching release, then read_refused. |
| Initial refusal without a page | No cursor, no release, no successful recall or private grant. |
| release refusal, malformed reply/observation, invalid leg/code, dead seam or expiry | Existing failure path, never a healthy operation refusal. |
| Successful reads, local allocator overflow | counter_exhausted with null read_code, no effect. |
| Correct effect/outcome template rejection | Complete all dispositions, record the existing fault, then template_rejected. No plan break or component halt solely for that rejection. |
| Trace pressure | Keep the exact event, parent and ticket. Return no result before Recorded. |
| Stop or leave | Retain owed read/source/author custody under original bounds and shutdown permissions, grant no dependent continuation. |
| Effect admitted before cancellation | Settle the actual disposition. Never claim cancellation proves that no commit occurred. |
| Mandatory summary refresh after commit | Existing foundation failure on refusal, committed receipt retained, no stale successful cache or operation result. |

The generic reader, store selectors and paging wire do not change. STEP-QUERY still
returns a checked successor or its existing read outcome. Returning that read never
activates a step. The proposal does not turn arbitrary selector errors into resource
refusals or authorize more reads. It does not define a narrower substitute query or a
retry policy. A correct pre-effect refusal preserves the prior committed cache, and
an allocator grant survives until all its owed accounting settles.

## Alternatives and tradeoffs

| Alternative | Consequence |
| --- | --- |
| Proposed separate operation case and typed trace account | Preserves exact reasons without expanding durable break causes. Requires floor codec, harness owner and exhaustive refusal-consumer changes before cutover. Adds one metadata record per admitted refused operation under the original deadline. |
| Add resource cases directly to PlanBreakCase | Smaller type edit, but admits resource/template failures as stored broken-plan causes and broadens PlanEffect::Break plus every reader. |
| Return invalid_target or hide the cause in detail | Keeps the old shape but attributes a resource/template failure to the target or requires parsing diagnostic text. |
| Halt on every required read or template refusal | Makes a valid bounded-query or correctly rejected template end healthy service and conflicts with the current correct-refusal rule. |
| Return typed data only, with no exact refusal trace account | Keeps fewer records, but the read measurement omits the code and the trace cannot reconstruct the operation's exact refused outcome. |

This proposal changes representation and refusal settlement, not retry policy, review
scheduling, category authority or automatic progression. The added trace work must fit
the existing original bound. A stalled recorder or exhausted bound keeps its failure
path rather than extending the deadline. Its cost and reader compatibility still need
qualification with the full production tuple.

## Paired owning clauses and implementation plan

| Act | Owning clauses | Disposition |
| --- | --- | --- |
| This draft | Spec 07 PLAN-PORT, PLAN-PORT-REFUSAL, PLAN-PORT-RECORD and instruments | Spec gap proposed for the operator. Owns exact cases, operation identities, leg/code set, cleanup, parents and pre/post-effect distinction. |
| This draft | Types amendment TYPES-PLAN-PORT-REFUSAL | Spec gap proposed. Owns strict shared result and refusal codec. Root PlanUpdateRefused remains unchanged. |
| This draft | Harness amendment HARNESS-PLAN-PORTS | Spec gap proposed. Owns one retained request/read/source/author account and allocator settlement. |
| This draft | Trace amendment TRACE-REGISTRY and TRACE-PLAN instrument | Spec gap proposed. Existing raw refusal payload carries the exact floor encoding under the existing envelope checks. No floor dependency or new trace kind. |
| After election, Planner review and merge | Issue #146 section D, checked successor and step-activation composition stacked after #383 | Implement these paired cases/codecs and the activation owner, worker adapter, shared allocator and real SQLite receipt/cache-refresh path together. Address review corrections and run gates before publication. |
| Before coupled cutover | Issue #146 qualification and trace-reader checks | Verify this refusal family through the full trace reader and measure its retained memory and recording time with the deployed artifact tuple. |

Do not deploy from this document change. The code still has the two-field
PlanOperationRefused and lacks the non-root activation/closure composition. No branch
may select foundation serving on the strength of these Spec sentences. The implementing
act above must carry code, codec consumers, trace accounting and tests together. The
state member and admin gain no new wire request, and no external contract text changes.
No assertion is made that an external trace reader has already been qualified.

## Verification owed by the implementing act

Exercise every admitted initial and next refusal code through the real read owner,
with invalid leg/code controls and a cursor acquired before refusal. Pin one matching
release, the exact initial/next measurement parent, unchanged code/detail, and no
recall, effect or private grant. Refused release and the same code after a committed
effect must follow failure paths. A valid pre-effect refusal must preserve a previously
committed cache and return the allocator only after accounting completes.

Add actual SQLite worker composition for checked successor query, activation with and
without opening a group, trace-before-effect, exact receipt and summary refresh. Deliver
Stop, leave and recorder pressure at the read, effect and refusal boundaries. Preserve
all owed evidence through failed-run handoff and prevent any cancelled continuation.
Use an unready transport to pin the original Decision/read expiry. Pin local span-ID
overflow and a correct certified-template rejection independently.

Codec fixtures reject missing read_code, incompatible case/code pairs, an unsupported
read code, unknown fields, a missing required step and a foreign refusal family. Break
codecs reject each of the three new operation-only cases. Exact serialized refusal
bytes and their canonical envelope must round-trip through the ordinary trace path.
Compiled protective controls must remove the individual custody, deadline, parent,
recording and break-separation guards and fail meaningful cases. No runtime evidence is
claimed by this docs-only proposal.

## Author checks

The lock gate and all three DocsMirror tests pass. Added text is ASCII and the diff
has no whitespace errors. Manual author review compared the proposed shapes, read
legs, parents and cancellation boundary across all four owning Specs. The requested
code-review and verification-before-completion skills are unavailable in this session,
so no skill pass or Planner grade is claimed. No runtime source changed and no Rust
suite was run for this proposal. Runtime gates and protective controls remain owed by
the implementing act.
