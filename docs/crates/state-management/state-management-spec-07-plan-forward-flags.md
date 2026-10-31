# State Management Spec 07 - Locked Plan and Forward Flags

**Status:** MERGED.

**Landing PR:** #420

**Version:** 0.22 **Date filed:** 2026-10-20 **Document ID:** `state-management-spec-07`
**Parent:** state-management-primitives-PRD **Editorial:** Per the Working Rules.

## 0. What this document is

**SCOPE.** This Spec supplies PLAN-1 through PLAN-6, the SERVE-3 step label and the plan
duties of PROMPT-3. It depends on the foundation's receipt, Spec 02's classifier, Spec
05's pairs and Spec 06's marks. It adds no reviewer inside the agent and gives sort no
write handle. The [foundation transition][transition] applies to every owning clause
below. Until cutover its previous serving contract remains applicable. Reason: state
holds the plan and validates transition evidence, while the caller chooses review and
progression. R20-12 removes that orchestration from this Spec. [PLAN-1 through PLAN-6,
SORT-1]

The baseline is 2eb5c5a2. Admin accepts fixed command lines and no stdin. The only
external execution tool is the confined sh grant. The harness does not execute programs.
The revised carriers below extend those interfaces, rather than inventing a management
request from WeaverWeb or a private test runner. [PLAN-6, October 18 R5, October 18 R6]

## 1. Operator carrier and declaration

**PLAN-ADMIN.** Add the root-shell-only command `weaver-admin plan-update <agent>`. It
takes no additional argument and reads no stdin. Admin requires effective uid 0 and the
existing parsed cause uid 0. A nonroot sudo cause refuses even when effective uid is
root. It is not added to either connector sudo role. The fixed root file is
config-root/plan-update.json, outside the bound declaration. Admin opens a regular
root-owned file without following symlinks, refuses group or other write permission,
reads it once into a bounded buffer and hashes those bytes. It holds the ordinary
invocation lock and requires a running serving worker. Reason: an operator update needs
a real management-plane carrier without granting connectors plan authority. [PLAN-6,
FLOW-1, October 18 R5]

The maximum is weaver_types::MAX_ENVELOPE_BYTES for the complete encoded lifecycle
directive. Oversize refuses before dialing. No prefix of a document is sent. The file's
strict JSON shape is PlanUpdateDocument {format, expected, operation}, format
state-plan-update/1. expected is null or PlanRef. Unknown fields refuse. The document is
not a configuration rebind or a caller-selected path. [PLAN-6, PLACE-4]

**PLAN-TYPES.** PlanId and StepId are {seed_id, local_id}, with DecimalU64 local IDs.
PlanRef is {plan_id:PlanId, revision:DecimalU64}. IDs use separate plan and step
counters held in the store, never span counters. A StepId's condition spelling is
`step:<local_id>` in its seed's loaded store. That spelling is never resolved across
seeds. Reordering changes ordinal, not ID. Retired IDs are never reused. [PLAN-1,
PLAN-3, MARK-6]

PlanOperation is externally tagged create, authorize, replace or answer_break. In every
create, authorize and replacement body categories is Vec<Category>, groups is
Vec<GroupDraft> and review is ReviewSetupRef. answer_break carries those fields inside
its replacement body. create contains {task: String, steps: Vec<StepDraft>, categories,
groups, review}. authorize contains {task: String, categories, groups, review},
authorizing one model draft. replace contains {task: null-or-String, steps, categories,
groups, review, preserved_closures: Vec<StepId>} and requires expected. answer_break
contains {replacement: PlanReplacement}, the replace body rather than a recursive
operation. A StepDraft is {id: null-or-StepId, instruction: String}. New IDs are null.
create requires every ID null and expected null. authorize also requires expected null.
replace and answer_break require an exact expected PlanRef. The harness allocates them
after classification, never before a model has supplied its step count. Every
instruction must classify as exactly one statement. [PLAN-1, PLAN-5, PLAN-6]

The typed operation is authoritative. There is no second free prose description whose
equivalence the harness must guess. Task and instruction strings are the exact STE100
content to check and file. Model creation takes each accepted binding in Spec 02's
EffectiveEmission order as one step instruction. The original pass must finish
Completed, with nonempty statements and attempted_call false. Its repairs may not
execute calls. The candidate remains a proposal until lock. [PLAN-1, PLAN-5, CLASS-2,
CLASS-9]

**CATEGORY.** A category is {id, verbs: Vec<String>, acts: Vec<PlanAct>,
review_required:bool}. The operator supplies review_required explicitly for every
category. true declares that closure needs a completed review pair, and false declares
that it does not. No default is inferred from the verb. The store holds this bit and
returns it in plan reads, and the primitive does not enforce it: enforcing it is the
loop's (#423). Either value permits unreviewed and reviewed closure evidence, and the
caller decides when to seek closure. [PLAN-2, PLAN-5, R20-17]

PlanAct is statement, external_tool {name}, or inward_recall {verb}. Category IDs are
technical names in the loaded STE100 scheme. The operator supplies categories only.
Their verb sets are nonempty, disjoint and subsets of the closed scheme. The step's
classified verb chooses exactly one category. Acts match exact enum and tool-name
values. Unknown fields, shell-prefix predicates and executable predicates refuse. An
external act must also pass the loaded tool grant. Reason: a plan-scoped check cannot
replace the kernel boundary or parse sh as a permission language. [PLAN-5, CLASS-2,
October 18 R6]

Every operator operation uses GroupDraft {id:String, step_indices:Vec<DecimalU64>}.
Indices are zero-based in its supplied or authorized step order, never StepIds. Resolve
them once at lock to PlanGroup {id:String, step_ids:Vec<StepId>,span:SpanRef-or-null}.
An authorization may precede the count. Check its indices against the model's candidate
before lock. Groups cannot overlap or nest in this release, and their steps must be
contiguous in plan order. Each group becomes a Spec 06 subtask span. The task contains
all groups and ungrouped steps. Structural predicates do not prove that arbitrary shell
semantics further the goal. Review and the operator assess that limit. [MARK-1, PLAN-2,
PLAN-5]

**PLAN-REVIEW-IDENTITY.** ReviewSetupRef is {setup_id:String,sha256:Digest}. setup_id is
a nonempty scheme technical name. The operator supplies this identity in
PlanUpdateDocument. It identifies the external review procedure and evidence contract
selected by the caller. The store holds it and requires equality on preserved closures
and review pairs. Changing either field changes the setup. The primitive neither opens
the named procedure nor supplies its script, interpreter, trigger or test list. No
state-management.plan.review declaration is added. Reason: state preserves the identity
on which closure depends, while review is orchestration. [PLAN-2, PLAN-5, PLAN-6,
R20-12]

## 2. Management and filing protocol

**PLAN-EXCHANGE.** Add LifecycleDirective::PlanUpdate {cause, document_sha256,
document:String}. The string is the exact UTF-8 file bytes. The worker checks their
digest, then strictly decodes PlanUpdateDocument. The worker admits it only from its
root coordination peer, at rest, while serving and not leaving, saving or handling
another update. Otherwise the existing lifecycle refusal names out_of_order. It does not
queue behind a running turn or tool review. LifecycleAnswer::PlanUpdated contains {plan:
PlanRef, source: SourcePosition}. A new explicit PlanUpdateRefused contains
{case:PlanUpdateCase,detail:String,rejected_blocks:Vec<DecimalU64>,
read_code:ReadRefusalCode-or-null}, with cases malformed_plan, stale_plan, non_ste100,
invalid_category, invalid_group, closure_changed, wrong_seed, plan_exists,
counter_exhausted, template_rejected and read_refused. All fields are required.
read_code is nonnull only for read_refused under PLAN-PREFLIGHT-READ, and null for
all other cases. Invalid root cause uses the existing lifecycle refusal. detail is
diagnostic text, never a machine-coded extra case. rejected_blocks is ordered source
block indices for non_ste100 and empty for every other case. Admin logs the answer
and prints it through its existing one-answer interface. [PLAN-6, FLOW-4]

The wait uses the loaded decision-wait bound from Spec 02 from worker acceptance through
its last required read and trace record, including the committed plan-summary read.
Admin carries that bound from the run's loaded facts, not a mutable draft, and its
timeout ends the exchange without claiming the update did not commit. The worker traces
the outcome before the answer. A lost answer is resolved by show's committed PlanSummary
and the receipt trace, never blind retry. A correctly rejected document refuses the
operation and leaves the agent running. Component failure follows the foundation latch.
[CLASS-8, PLAN-6, FLOW-4]

**PLAN-ADMISSION-REFUSAL.** Trace-before-answer also applies when a fully received,
well-formed PlanUpdate opener is judged and refused before plan-operation admission.
This includes out_of_order and the existing root-cause refusal. The lifecycle owner
retains the exact received packet and the selected refusal. Before it sends the
refusal answer, it records RefusalRecord::Lifecycle with asked LifecycleAsk::PlanUpdate
from TYPES-PLAN-CONTROL, encoded as {"ask":"plan_update"}, and that unchanged refusal
under the existing refusal event. This requires the floor ask variant, its codec and
typed refusal consumers to be implemented with the author path before this accounting
is enabled. The event is harness/runtime metadata with no turn or causal parent.
Its envelope omits turn and causal_parent under the trace's absence rule, without
explicit JSON null members. It is not a plan input or effect and grants no allocator,
read, placement or generation. [PLAN-6, FLOW-3, FLOW-4]

The refusal belongs to the rejected management opener, which has no accepted plan
operation or turn. An already open turn remains the original caller's work. The
trace envelope still supplies the current run. PLAN-PORT-RECORD instead attributes
its refusal to an accepted internal operation and that operation's nullable turn.
Neither owner borrows the ambient turn merely because it is open. [PLAN-6, FLOW-3]

This rule is specific to PlanUpdate. It adds no record obligation to other lifecycle
directives. Invalid framing or descriptors and incomplete requests retain the existing
transport disposition. A rejected PlanUpdate never queues for later admission, even
if the original turn or operation finishes while its refusal records. [PLAN-6, SEAM-3]

The refusal's accounting and delivery share one loaded decision-wait interval, starting
at the worker's retained monotonic reading immediately after receiving the complete
opener. It borrows the loaded decision_wait elapsed_ns value, not the model's decision
barrier or its authority. It coexists with every earlier operation bound, which it
cannot reset or retire. Trace pressure keeps the same event and author ticket. If an
earlier author owns the slot, only that author's original continuation may consume its
receipt. Run
that continuation only as needed to free the slot, under its original bounds, then
record the refusal. This does not cancel the original operation or elect its next
workflow. [CLASS-8, FLOW-1, FLOW-4]

EOF after the complete opener does not discard the owed record. A refusal that cannot
reach Recorded within its original interval takes the foundation decision_bound failure
path and retains its request, original or ticket, and first failure through teardown.
Once Recorded, the exact author claim still consumes its own completion. Answer
delivery uses only the remaining interval. EOF or delivery expiry then ends delivery
without retry, without halting solely for the missing answer and without claiming the
refusal was unrecorded. Recording a valid refusal preserves the active turn and prior
committed plan. [PLAN-6, CLASS-8, FLOW-3, FLOW-4]

**PLAN-PREFLIGHT-READ.** Before a root update admits its plan effect, a valid read
refusal in the table below refuses only that operation as read_refused. The table
covers the plan_current and marks snapshots and the statement reads needed to compare
preserved closures. It does not authorize another selector, query retry or narrower
substitute. ReadRefusalCode keeps TYPES-READ's vocabulary. Admission to this outcome
requires the matching complete reply and its Recorded state.store.measurement under
HARNESS-READ, plus any acquired-snapshot cleanup. Consume only the read owner's
finalized Refused {code, detail}, never a raw control_refused before cleanup.
[PLAN-6, SEAM-3, FLOW-3, FLOW-4]

| Refusing leg | Codes admitted to read_refused |
| --- | --- |
| Initial read(plan_current) or read(marks) | counter_exhausted, result_too_large, result_unavailable, answer_too_large |
| Initial read(statement) for a preserved closure | not_found, counter_exhausted, result_too_large, result_unavailable, answer_too_large |
| next for one of those acquired snapshots | unknown_snapshot, wrong_offset, result_unavailable, answer_too_large |
| release | None. The owned acquired snapshot must receive its matching released answer. |

counter_exhausted may come from the selector's allocator frontier or the run-local
snapshot counter. A successful read followed by a local allocation overflow retains
the existing counter_exhausted plan case with read_code null. Statement not_found is
preserved as the read's refusal, never recast as an empty instruction or permission to
preserve a closure. A valid later next refusal keeps the acquired cursor until release.
No refusal alone proves that release occurred. [PLAN-1, PLAN-6, MARK-6, SEAM-3]

The initial selector table excludes unsupported_operation and invalid_selector:
these are required, harness-constructed selectors in this serving contract. wrong_seed
and the span-only codes do not belong to these selectors. read_busy is incompatible
with the sole owner's initial read, and unknown_snapshot/wrong_offset are not initial
read outcomes. Do not downgrade an invalid leg/code, uncorrelated reply, malformed
result or observation, failed cleanup, dead seam, or expired bound to read_refused.
Their existing component-failure or lifecycle account applies. This rule changes no
generic read codec, selector semantics or state-member refusal. [SEAM-3, FLOW-4]

Copy the validated read's code into read_code and its detail unchanged into detail. Keep
rejected_blocks empty. Record the existing lifecycle refusal with ask plan_update before
answering, using the refusing leg's Recorded state.store.measurement sequence as
causal_parent. The retained operation account keeps the original selector, exact
submitted leg/request identity, refused completion, acquired cursor and partial bytes
where present, and that sequence. A release record cannot replace the refusing leg's
parent. No new event kind, clock, source identity or successful recall is created.
StateStoreMeasurement has no refusal-code member, so it is not a substitute for this
typed refusal record. [PLAN-6, FLOW-3, COST-2, SEAM-3]

An initial refusal owns no cursor and sends no release. A refusal after a valid page
finishes the one acquired release under the original read and decision bounds before
the root refusal is answered. Recording pressure retains the same refusal and ticket.
Cleanup failure or bound expiry takes its existing failure path and does not report a
clean read_refused completion. Stop/leave grants no new operation continuation, and
only existing shutdown permissions may settle the retained account. [CLASS-8, FLOW-4,
SEAM-3]

No effect is authored or filed after this refusal. Already admitted operator blocks
still finish their owed dispositions, and accepted proposal holdings remain. The prior
committed PlanSummary stays available once this operation's duties settle, since no
plan effect has committed. Do not infer an empty plan, invalidate that cache solely for
this read refusal, retry the update, renew its bound or halt solely for a valid code in
the table. The operation answers no PlanUpdated and grants no dependent effect.
[PLAN-6, FLOW-1, FLOW-4]

Release the root operation's allocator grant at settlement, after any acquired cursor
cleanup, all admitted dispositions and the Recorded refusal, before answering. The
grant does not survive the transition back to Idle. [PLAN-1, PLAN-6, FLOW-4]

This outcome ends at effect admission. A required refresh after a committed effect
still follows PLAN-SUMMARY-EVIDENCE: a refusal, malformed result, failed seam or held
expectation mismatch latches component failure, preserves the committed receipt and
blocks successful stale Observe/show or generation. It cannot become read_refused or
undo the effect. [PLAN-6, FLOW-4, SEAM-3]

**PLAN-FEED.** An admitted control exchange first authors plan.event, channel input,
origin operator, turnless, with {message,context,document:String}. document retains the
exact operator file bytes on the trace and has no selectable projection. message has
role user and only Text blocks. The task, when present, is block zero. Instructions
follow in supplied order, starting at zero when replacement has no task. authorize holds
its task block alone. Empty text and an empty supplied step vector refuse. The partition
must select every block and the context sidecar. Classify before model access. A correct
rejection returns non_ste100 with the rejected block indices and no plan effect.
Accepted statements may remain as proposals. Neither the trace nor held statements roll
back. [PLAN-6, FLOW-1, GATE-1, CLASS-10]

PlanInputContext is {document_sha256:Digest,cause:Cause,expected:PlanRef-or-null,
operation:PlanInputOperation}. Its strict operation has the PlanOperation shape with
task strings replaced by block indices and StepDraft replaced by PlanInputStep
{id:StepId-or-null,block:DecimalU64}. Categories and GroupDraft values are unchanged.
The context holds no task or instruction text. Indices enumerate exactly the Message
blocks in the order above, with no alias or unused block. Block zero's filing holds this
context with effect null. All other blocks have both members null. Trace document bytes
retain the original hash identity even if some text was rejected. A rejected block
cannot supply a later effect. Reason: the metadata must not become a second path for
unclassified instructions. [PLAN-6, CLASS-10, STORE-3, TAP-4]

After all required receipts, preflight the change against plan_current. Author a new
certified plan.event, channel internal, origin loop, turnless for operator updates, with
{message,context:PlanEffect,template_id}. It has exactly one Text block and role user.
Source references name the operator event's filed blocks. Only a private
AcceptedPlanUpdate grant from the root coordination handler can construct an
operator-change effect. The store checks those references and the expected revision
atomically. A correct rejection of this certified text fails the operation visibly,
never grants the effect or halts for conformance. Reason: content and its materialized
plan both take the one feed. [PLAN-6, FLOW-8, SEAM-2]

**PLAN-EFFECT.** StatementEffect gains plan:PlanEffect beside mark:MarkEffect. Both
layers are externally tagged. Each field below is required, including nulls, and unknown
fields refuse. The effect equals the strictly decoded context, never a guess from the
template's words. References use SourceBlockId and StatementRef, not SQLite row IDs.
[PLAN-1, PLAN-2, PLAN-6]

| Variant | Exact body |
| --- | --- |
| authorize | plan:PlanRef, source:SourceBlockId, task:TaskScopeRef, review:ReviewSetupRef |
| lock | plan:PlanRef, source:PlanLockSource, task:TaskScopeRef, steps:Vec<PlanStepInit>, review:ReviewSetupRef |
| replace | expected:PlanRef, next:PlanRef, source:SourceBlockId, task:TaskScopeRef, steps:Vec<PlanStepInit>, review:ReviewSetupRef |
| activate | expected:PlanRef, step_id:StepId, step_span:SpanRef, group_span:SpanRef-or-null, successor:StepQueryRef |
| close | expected:PlanRef, step_id:StepId, review_call:CallId-or-null, work:WorkRef, outcome_span:SpanRef |
| break | expected:PlanRef, step_id:StepId-or-null, case:PlanBreakCase, proposed_source:SourceBlockId-or-null |

PlanLockSource is operator:{input:SourceBlockId} or
model:{authorization:SourceBlockId,candidate:Vec<StatementRef>}. PlanStepInit is
{step_id:StepId,instruction:StatementRef}. Its vector order is plan order. Categories,
groups, task text and preserved_closures come from the referenced operator context, not
another effect field. The store verifies all required blocks of that event are held
under its scheme. Each supplied step block has exactly one binding. Model candidate
references equal the complete effective statement list from one proposal operation.
Valid corrections may have later source IDs. The harness's private proposal grant proves
that derivation, and the store verifies every held reference. No raw one-emission
restriction may reject a valid repaired proposal. [PLAN-1, PLAN-6, CLASS-9]

Authorize allocates the next PlanId at revision one, status authorized, no steps. Its
model lock consumes that same authorization and PlanRef once, without changing
categories. An invalid candidate leaves it unconsumed and returns a typed plan-break
outcome. The caller owns any turn response. Rejected text still follows CLASS-9. A
supplied create allocates the next PlanId at revision one directly. Both require no
unfinished current plan. The allocator is the mark coordinator, using plan_current and
marks reads, and allocation becomes durable only with the receipt. New step IDs start at
MAX held step_local_id plus one, assigned in instruction order. Overflow refuses before
authoring an effect. A committed ID is never reused. [PLAN-1, PLAN-5, MARK-6]

A replacement matches the current PlanRef and increases revision by one. It cannot
recycle retired step IDs. Preserved closure requires the same instruction bytes,
complete Category value, review setup and filed evidence. Otherwise the step runs again,
with a new SpanRef at revision one, not a revision of its old span. Old closure remains
historical. A changed task retires its former work scope and invalidates the old task
placement. A later placement must obtain a valid prefix from Spec 06. The detailed
parent and preservation guards are owned by the store amendment's plan transaction.
[PLAN-2, PLAN-5, PLAN-6, PLACE-2]

**PLAN-STORAGE.** The store owns plan_revision, plan_step and plan_category.
plan_revision's key is (plan_id, revision), with task/prompt references, status,
operator/lock/last sources and review_setup. Status is authorized, locked, broken,
complete or superseded. plan_step's key is (plan_id, revision, step_id), with unique
ordinal, statement_binding reference, category ID, span reference, status and
review_pair reference. The group link lives only in plan_group_step, not in a second
plan_step column. Step status is waiting, active, closed or retired. plan_category's key
is (plan_id, revision, category_id), with the exact canonical JSON Category. Group
membership is stored in plan_group and plan_group_step with foreign keys and unique step
membership. Counter columns use the foundation's eight-byte encoding, not signed trace
counters. [PLAN-1, STORE-5]

STATE-PLAN-SCHEMA and STATE-PLAN-FILE in the store amendment own the literal DDL and
per-effect guards. The effect transaction inserts the source statement, updates the
plan's guarded state and creates/closes its Spec 06 marks atomically. An update matches
exactly one expected current revision. Unknown sources, changed closure, forged origin
or multiple active steps refuse the whole filing. A null review_call is valid under
either CATEGORY value. plan_current {} and plan {plan:PlanRef} return PLAN-READ's typed
snapshots. plan_flags uses the metadata result in PLAN-FLAG-READ. All use bounded
QueryResult paging, and all plan tables are part of the dump. [PLAN-2, PLAN-6, MARK-5,
PERSIST-1]

**PLAN-READ.** Add selectors plan_current:{} and plan:{plan:PlanRef}. QueryResult gains
plan_current:{current:PlanSnapshot-or-null,
next_plan_id:DecimalU64,next_step_id:DecimalU64} and plan:{snapshot:PlanSnapshot}.
Current is the nonsuperseded revision with greatest last-transition content order,
including authorized, broken and complete. At most one unfinished plan exists. Missing
historical identity refuses not_found, wrong seed refuses wrong_seed, and exhausted
allocation refuses counter_exhausted. These reads reserve no ID. Reason: consumers need
one durable identity and closure account on both sides of reload. [PLAN-1, PLAN-2,
SEAM-3]

PlanSnapshot is {plan:PlanRef,task:TaskScopeRef,status:PlanStatus,
review:ReviewSetupRef,operator_source:SourceBlockId,lock_source:SourceBlockId-or-null,
document_sha256:Digest,last_source:SourcePosition,categories:Vec<Category>,
group_drafts:Vec<GroupDraft>,groups:Vec<PlanGroup>,steps:Vec<PlanStep>,
broken:PlanBreakRecord-or-null}. PlanStatus is authorized, locked, broken, complete
or superseded. PlanStep is
{step_id:StepId,ordinal:DecimalU64,instruction:StatementRef,
category:String,group:String-or-null,status:StepStatus,span:SpanRef-or-null,
outcome_span:SpanRef-or-null,review_call:CallId-or-null,work:WorkRef-or-null,
closure_source:SourceBlockId-or-null}. StepStatus is waiting, active, closed or
retired. PlanBreakRecord is {case:PlanBreakCase,step_id:StepId-or-null,
proposed_source:SourceBlockId-or-null,source:SourceBlockId}. Categories and groups
keep operator order, steps sort by ordinal, and group members follow step order.
group_drafts retain indices even before authorized steps exist. groups and steps are
empty while authorized. Every identity is seed-qualified on output. [PLAN-1, PLAN-2,
STORE-3, STORE-5]

**PLAN-SUMMARY-EVIDENCE.** Both selectors return document_sha256 from the held
PlanInputContext at operator_source. last_source carries the exact SourceBlockId
and required nullable turn of that revision's stored last-transition source.
Resolve these facts in the same immutable read snapshot as the plan revision.
The digest is the operator document's digest, not a hash of the snapshot, current
prompt or materialized plan. A turnless last transition carries turn null, never
the boot run's turn or the previous nonnull turn. [PLAN-1, PLAN-6, STORE-5, SEAM-3]

The member owns consistency with its held operator and transition sources, both
when adopting the image and when deriving a read. At boot the harness validates
strict shape, selector/result association and any already-held trusted expected
facts. With no independent expected digest or turn, it relies on the member's
source validation. It cannot detect a well-shaped false digest or turn merely
from that snapshot. No extra source lookup or invented comparison is required.
[PLAN-1, PLAN-6, STORE-5, PERSIST-6, SEAM-3]

At HARNESS-RECALL's Recording/Recorded transition, after HARNESS-READ has completed
snapshot release, the harness projects PlanSummary from plan, status,
document_sha256 and last_source. After its own plan-effect receipt, it must also
match last_source to that effect's exact SourcePosition, including nullable turn.
It compares document_sha256 with any already-held trusted expected operator digest
for that revision. The comparison uses the held expectation, not a value copied
back from the same response. A valid current null means no committed plan only
when no held receipt requires one. [PLAN-1, PLAN-6, FLOW-3, SEAM-3]

A refused or malformed mandatory boot read cannot establish enter success. A
refusal, malformed result, failed seam or trusted-expectation mismatch during the
required refresh after a committed effect latches component failure through the
existing foundation path. Do not serve the old cache as a successful committed
Observe/show result, answer PlanUpdated or start another generation. The committed
effect remains committed and its receipt stays evidence. Add no selector, source
lookup, event kind, counter, clock or new refusal code. [PLAN-6, FLOW-3, FLOW-4,
PERSIST-6, SEAM-3]

This read projection does not change the stored last_source content key or the
PlanFlags after field. When deriving an explicit flag query from the snapshot,
use last_source.block_id. A digest and a turn annotation add no source visit to
recall and grant no plan mutation or lifecycle success. [PLAN-3, STORE-5, SEAM-3]

**PLAN-FLAG-READ.** Add the ReadSelector tag plan_flags
{plan:PlanRef,after:SourceBlockId} and QueryResult tag plan_flags
{snapshot:PlanFlagSnapshot}. PlanFlagSnapshot is
{plan:PlanRef,after:SourceBlockId,through:SourcePosition,total_flags:DecimalU64,
matched_flags:DecimalU64,unmatched_flags:DecimalU64}. It is an observation over held
statements, not candidate content for place. The store checks the loaded seed,
current PlanRef and exact last_source.block_id equal to after, then takes one read
snapshot. A wrong seed refuses wrong_seed, an absent plan/source refuses not_found,
and a stale revision or transition refuses invalid_selector. The ordinary read-result
bound, paging and release apply. Reason: a count needs the same committed plan and
holding boundary at both ends of the read. [PLAN-3, PLAN-6, SEAM-3, COST-4]

Count each held binding with a canonical positive `step:<local_id>` condition once,
across the loaded chain. A compound noun does not multiply it. matched_flags counts
those whose target occurs as a nonretired step in that exact revision, including
waiting, active and closed steps. All remaining canonical step targets are
unmatched_flags. total_flags equals their checked sum. Nonstep conditions are outside
the count. A flag aimed at an older plan or a removed step can be unmatched in the
current plan while remaining held. This count says nothing about whether the flag was
needed or missed at a decision. WeaverAnalysis owns that judgment. STATE-PLAN-FLAGS owns
the literal query. [PLAN-3, COST-4, SCOPE-1, October 18 R9]

through is the greatest held operating position in the same read transaction, and
includes after. The store compares their internal commit order, never the lexical order
of run IDs. The harness checks the exact plan/after echo and count sum before exposing
the result. A typed read refusal yields an unavailable measurement, not zero, a plan
rollback or a conformance halt. A broken read seam retains the foundation's
component-failure rule. [STORE-5, FLOW-4, COST-4]

## 3. Transition ports and closure evidence

**PLAN-PORT.** The worker's FoundationInput (weaver-harness
flow/context_worker_plan_ports.rs) exposes activate(expected,step_id,successor),
assess_act(expected,step_id,statement,act), work_reference(expected,step_id,span,through),
observe_review(expected,step_id,pair,work,assessment),
attest_closure(expected,step_id,work), close(expected,step_id,evidence),
and record_break(expected,step_id,case,proposed_source). IDs have PLAN-TYPES' types.
statement is StatementRef, act is PlanAct, span is SpanRef, through is SourcePosition,
pair is Spec 05's private CompletedPair, work is WorkRef, and proposed_source is
Option<SourceBlockId>. Each operation returns its
typed result or PLAN-PORT-REFUSAL's PlanOperationRefused. A transport/component
failure remains the foundation failure, never a refusal. The mutable
operation port belongs to the compiled loop and verified management handler. Neither a
model message nor a gate frame can construct it or a private grant. Reason: callers
request mechanics, while authority and exact state remain checked at the primitive.
[PLAN-1, PLAN-2, PLAN-5, PLAN-6, R20-12]

successor is the private checked step_successor result from Spec 06. The store validates
its StepQueryRef and uses it as the step's typed prefix. activate returns the committed
active PlanStep after its effect receipt. assess_act returns AllowedAct, a private grant
tied to plan revision, step, StatementRef and the exact act, or
wrong_category/unknown_tool. It looks up the held binding verb and the operator's
Category and compares exact enum fields. It dispatches nothing. close and record_break
return the committed PlanSnapshot. No operation selects a successor, invokes a reviewer
or starts a generation. Correct refusals leave durable state unchanged. [PLAN-2, PLAN-4,
PLAN-5, R20-12]

**PLAN-PORT-REFUSAL.** The named PLAN-PORT operations, including WORK-REFERENCE's
work_reference constructor, return PlanOperationRefused
{case:PlanOperationCase,detail:String,read_code:ReadRefusalCode-or-null} for an
admitted operation refusal. All fields are required. PlanOperationCase has the eight
PLAN-BREAK spellings plus read_refused, counter_exhausted and template_rejected.
PlanBreakCase keeps its existing eight cases. An operation refusal never implicitly
requests record_break. The three additional cases cannot appear in a PlanEffect::Break,
PlanBreakRecord or plan.break. Reason: a bounded read or rejected template does not
establish that the locked plan is broken. [PLAN-2, PLAN-5, PLAN-6, FLOW-4]

read_code is required nonnull for read_refused. A read-derived invalid_target or
work_changed also carries its exact nonnull code under the table below. Those two
cases carry null when a local check, rather than a refused read, established the cause.
Every other case carries null. For every read-derived refusal, copy the finalized read
owner's code and detail unchanged. A local checked allocation
overflow after successful reads returns counter_exhausted with read_code null. A
correctly rejected compiled plan or outcome template returns template_rejected with
read_code null, after its complete source dispositions and existing
state_template_rejected fault record. Neither case grants an effect or a successful
plan result. detail is diagnostic text, never another machine-coded case. [PLAN-2,
PLAN-6, MARK-6, CLASS-2, FLOW-3, FLOW-4]

Before effect admission, and before a read-only operation grants its result, the
following resource refusals end only that operation as read_refused. This table applies
to the plan_current, plan, marks, statement and span_preview reads already required by
these ports. It licenses no new read, narrower substitute or retry. The following
domain table supplies the exact mapping for these required reads. The standalone
STEP-QUERY read keeps its existing read outcome and does not activate a step. [PLAN-2,
PLAN-4, PLAN-6, SEAM-3]

| Refusing leg | Codes admitted to read_refused |
| --- | --- |
| Initial required read | counter_exhausted, result_too_large, result_unavailable, answer_too_large |
| next for an acquired required snapshot | unknown_snapshot, wrong_offset, result_unavailable, answer_too_large |
| release | None. Require its matching released answer. |

| Initial required read domain outcome | Plan-operation disposition |
| --- | --- |
| not_found from plan or statement | invalid_target, retaining read_code not_found. |
| invalid_selector, span_pending or partial_pair from span_preview | work_changed, retaining that exact read_code. The requested work frontier cannot establish closure evidence. |
| wrong_seed from any required read | Foundation protocol failure. The harness must validate the loaded seed before submitting the selector. A foreign request identity is refused locally as invalid_target with null read_code. |
| not_found from plan_current, marks or span_preview | Foundation protocol failure. These selectors do not license this outcome. |
| invalid_selector from plan_current, plan, marks or statement | Foundation protocol failure. These required validated selectors do not license this outcome. |
| unknown_span or span_open from any required read | Foundation protocol failure. They belong to the closed-span selector, which this port table does not authorize. span_preview uses invalid_selector for an absent or terminal span. |
| span_pending or partial_pair from a selector other than span_preview | Foundation protocol failure. These codes belong to the required work preview only. |

A successful plan_current with null current, or a current status other than locked,
establishes no_locked_plan with null read_code where a port requires a locked plan.
An expected-plan/revision mismatch checked against a successful snapshot is
invalid_target with null read_code. A locally detected WorkRef digest/frontier mismatch
is work_changed with null read_code. These are successful reads followed by domain
judgments, not read refusals. Keep their required checked proofs. No domain code on a
next or release leg uses this initial-read table. [PLAN-2, PLAN-5, PLAN-6, SEAM-3]

All read-derived refusals in both tables obey the same following cleanup and recording
rules. Refusal changes no durable plan, even when its case is invalid_target or
work_changed. [PLAN-2, PLAN-6, FLOW-4]

Admit only the exact pending selector, request and complete correlated reply with its
valid observation and Recorded state.store.measurement. An initial refusal owns no
cursor and sends no release. A refusal after an acquired page retains the cursor and
partial result until the single required release completes. Consume the read owner's
finalized refusal after that cleanup, never a raw control_refused. Keep its original
selector, leg, request identity, exact reply, measurement sequence and acquired evidence
in the private operation account. A release measurement cannot replace the refusing
leg's sequence. No refused read yields recall, an empty plan, a private grant or a
successful result. [SEAM-3, PLAN-6, FLOW-3, FLOW-4]

An invalid leg/code, mismatched identity, malformed response or observation, failed
release, dead seam or applicable expiry keeps its existing foundation/lifecycle
failure. Do not convert read_busy, an impossible selector outcome or failed cleanup to
read_refused. All reads, cleanup and recording consume their original bounds within the
operation's single Decision deadline. Cancellation grants no continuation, but retains
and settles all already owed records and admitted source dispositions under existing
shutdown permissions. Release the allocator, if owned, only after these duties settle.
A pre-effect refusal leaves the committed plan/cache unchanged. Once an effect is
admitted, settle its actual disposition. After a committed effect, the mandatory summary
refresh still follows PLAN-SUMMARY-EVIDENCE's foundation latch on refusal and forbids
stale success. [PLAN-6, CLASS-8, FLOW-3, FLOW-4, SEAM-3]

**PLAN-PORT-RECORD.** Before returning an admitted PlanOperationRefused, author the
existing refusal event with RefusalRecord::PlanOperation. Its exact payload is
{seam:"plan_operation",operation:PlanPortOperation,expected:PlanRef,
step_id:StepId-or-null,refusal:PlanOperationRefused}. PlanPortOperation is activate,
assess_act, work_reference, observe_review, attest_closure, close or record_break.
step_id is required nonnull except for record_break, where it preserves the request's
optional step. expected and step_id preserve the request's exact typed values, including
a foreign identity refused before a read. These metadata values grant no loaded-seed
authority. The harness validates the loaded seed before constructing a selector or
private grant. The nested refusal is the same value returned to the caller. Busy
admission
returns the unaccepted request under PLAN-STATES and is outside this record. [PLAN-2,
PLAN-5, PLAN-6, FLOW-3]

License internal/runtime, subsystem harness, the accepted operation's run and nullable
turn, with no selectable content. For any nonnull read_code, causal_parent is the
refusing leg's Recorded state.store.measurement sequence. For template_rejected it is
the existing
Recorded state_template_rejected fault sequence. For the remaining cases use the latest
Recorded read proof consumed by that operation, including a checked proof already held
in its committed cache. Use null only when the refusal precedes consumption of any
read proof, such as local request-identity validation or a record_break validation that
needs no read. A judgment about held plan or work state still requires its owning
clause's checked proof. Do not omit that proof to manufacture a null parent. This rule
adds no fresh read when an already held proof satisfies the owning clause.
The owner retains that parent before authoring. Neither a caller-supplied sequence nor a
later unrelated event can replace it. Keep the same event and author ticket through
pressure. Only Recorded completes the refusal, and cancellation/failure retains the
owed account without releasing a dependent grant. No lifecycle plan_update ask, new
trace kind, counter or clock is introduced. [PLAN-6, FLOW-3, FLOW-4, COST-4]

**WORK-REFERENCE.** WorkRef is {plan:PlanRef,step_id:StepId,span:SpanRef,
through:SourcePosition,content_sha256:Digest}. PLAN-PORT's
work_reference(expected,step_id,span,through) constructs it for the current expected
plan revision and active step. span and through supply the target and committed
frontier for span_preview. content_sha256 is SHA-256 over the exact complete tagged
result bytes retained by the successful read owner. Concatenate the decoded page bytes
in read order, including the span_preview tag, span, through, entries and complete pair
records. Hash neither the control envelopes nor the base64 text or page boundaries.
Validate and release the complete read before constructing the checked WorkRef.

Do not parse and reserialize the result for this digest. Whitespace, field order and
opaque record bytes remain part of the released result. Two encodings with equal JSON
values can have different digests. The store uses the same result encoding for the
serving preview, closure recheck and historical closure verification. A change to that
encoding changes this evidence and must reach those producers and consumers together,
with dump/load and preserved-closure fixtures. Reason: a WorkRef binds the complete
held preview without an additional normalization algorithm. [PLAN-2, MARK-4, STORE-3]

The active step must own that span, and every attached pair must be terminal. A missing
or changed source refuses invalid_target or work_changed. For reviewed closure the
frontier precedes the review request. For unreviewed closure it is the last held work
before the outcome. No model text can supply a trusted WorkRef. An admitted refusal uses
PLAN-PORT-REFUSAL and records PLAN-PORT-RECORD's work_reference operation with the exact
requested expected and nonnull step_id before returning it. No refused construction
returns a WorkRef. [PLAN-2, MARK-4, STORE-3]

**CLOSURE-EVIDENCE.** ClosureEvidence is a private, single-use, run-bound grant holding
{plan:PlanRef,step_id:StepId,review_call:Option<CallId>,work:WorkRef,
setup:ReviewSetupRef}. Only the trusted primitive ports below construct it. Neither
model text nor a serialized record can supply one. close rechecks current revision,
active step, unchanged work and its operator Category. Missing or stale evidence never
becomes an effect. [PLAN-2, STORE-5, PAIR-3, R20-17]

ReviewAssessment is externally tagged passed:{} or failed:{case:ReviewFailure}.
observe_review checks the private pair receipt, review lane, completed fate, Result
outcome, exact setup and WorkRef, current revision and unchanged work frontier. It
returns ClosureEvidence with Some(review_call) for passed, or ReviewRejected
{case:ReviewFailure} for failed. Either category setting permits this reviewed form.
[PLAN-2, STORE-5, PAIR-3, R20-17]

attest_closure checks the same plan, step and held WorkRef, requires every attached pair
terminal. It does not read category.review_required (#423). It returns ClosureEvidence
with review_call None. This port accepts no model completion statement and invokes no
tool. Its trusted caller chooses when to request closure, and the port does not apply
the operator's declared review_required (#423). No fabricated review pair or tool
outcome is filed. [PLAN-2, PLAN-5, R20-17]

The review assessment and unreviewed closure request are trusted orchestration inputs. A
pass certifies authority, current work and, when a review pair is cited, its structural
evidence, not the adequacy of the caller's review. The caller interprets reports and
chooses when to invoke these ports. The primitive prescribes no report grammar or review
schedule. Its durable account is the WorkRef, category, outcome source and, when
present, the held review pair and setup. [PLAN-2, CAP-1, R20-12, R20-17]

**CLOSURE.** close consumes ClosureEvidence. After its work reread, close takes its own
Marks read and allocates outcome_span as that read's next span, with the same allocator
as activate. No caller supplies outcome_span. That read is this run's Marks read, its
catalog is coherent with the plan's task as its current task, and its open path is
exactly the active step's: the task, the group's Subtask span when the group has one,
then the step's span. Any other read is a foundation fault before any effect. close then
authors the certified step.outcome with the closed label and exact plan close effect.
The event takes the trace, partition, classifier and store path. The store commits the
effect, outcome span and closing marks atomically. The loop records a failed assessment
as PLAN-TRACE's step.outcome with the failed label and null context, through the
ordinary trace, partition, classifier and store path. That record writes no plan effect
and consumes no ClosureEvidence, so observe_review still leaves the step and plan
unchanged. Sort writes nothing. [SERVE-3, SORT-4, PLAN-2, MARK-1, FLOW-8]

The store opens and closes outcome_span below the active step, closes that step, and
closes complete ancestors from their held membership. The pair and frontier guards
remain STATE-PLAN-FILE's. No caller supplies an arbitrary ancestor list. A close receipt
changes no argument and fires no successor. Any store-guard refusal of close after the
local rechecks pass is a foundation fault, not a PlanOperationRefused (the operator's
ruling of 2026-10-30). The guard is a defence: close's allocation is serialized with
every other allocator in the run. [PLAN-2, MARK-1, MARK-5, R20-12]

## 4. Flags and locked-plan predicates

**FLAGS.** A forward flag is an ordinary held statement whose condition is the stable
step address. The condition query returns its exact statements and pairs in store order.
Reordering never rewrites that address. A nonexistent or retired target remains held and
appears as unmatched in PLAN-FLAG-READ. The caller chooses when to query and which
results to place. No extra arming field exists. [PLAN-3, READ-1, R20-12]

**PLAN-BREAK.** PlanBreakCase is no_locked_plan, wrong_category, unknown_tool,
self_edit, invalid_target, work_changed, review_setup_changed or review_required. A
failed assess_act returns its cause and grants no act. The caller may request
record_break, which authors a classified plan.event effect through the normal feed and
returns only after its receipt. It then records plan.break with
{plan:PlanRef-or-null,step_id:StepId-or-null,
proposed_source:SourceBlockId-or-null,case:PlanBreakCase}. With no locked plan it
records only the report and changes no stored status. The primitive never edits
categories from model content. [PLAN-5, PLAN-6, FLOW-3, R20-12]

A model proposal can consume only an outstanding operator authorization through the lock
port. A model cannot construct AcceptedPlanUpdate, replace a plan, change categories or
preserve its own closure. Root update, break recording and closure remain distinct
operations. What the caller does after a refusal is outside this Spec. [PLAN-1, PLAN-5,
PLAN-6, R20-12]

## 5. State tables and trace

**PLAN-STATES.** These are durable transitions requested at the port, not a schedule.
The source, current revision and effect must satisfy STATE-PLAN-FILE before one atomic
receipt changes the state. [PLAN-1, PLAN-2, PLAN-5, PLAN-6]

| State | Requested operation | Result |
| --- | --- | --- |
| NoPlan, Complete or Broken | Root create or authorize, no unfinished authorized/locked plan | Retire any broken open path and supersede that broken revision atomically, then new identity, Locked or Authorized, old history retained |
| Authorized | Authorized candidate lock | Validate complete held statements and operator categories, Locked |
| Authorized | Invalid candidate or model category edit | Typed refusal, authorization unconsumed |
| Locked, no active step | activate(first waiting step) | Guard order and marks, same plan with one active step |
| Locked, active step | assess_act | AllowedAct or typed refusal, no state change |
| Locked, active step | observe_review | Check held review and current work, ClosureEvidence or ReviewRejected, no state change |
| Locked, active step | attest_closure | Current work matches, under either category value: ClosureEvidence without pair. Otherwise work_changed or identity refusal, no state change |
| Locked, active step | close(valid evidence) | Close marks atomically, Locked with no active step or Complete |
| Locked | record_break | Broken, no fabricated closure |
| At rest | Root replace/answer_break | Guard expected revision, supersede and insert next revision |
| Any busy operation | Second operation | Busy with ownership returned, no queued mutation |
| Any | Correct STE100 defect | Operation refusal, no component halt |
| Any | Refused filing, component failure or model-blocking expiry | Foundation latch, no successful result |
| Any | Stop/leave before effect admission | Cancel request, no new effect |
| Any | Stop/leave after effect admission | Settle admitted disposition, preserve any committed receipt |

Completed and broken history remains addressable when a new plan is created. An older
review cannot close a newer revision. Only the root update port can replace a plan. No
row means that the primitive autonomously starts another row. [PLAN-1, PLAN-2, PLAN-6,
R20-12]

**PLAN-TRACE.** plan.event uses PLAN-FEED's operator or loop payload. Trace-owned
structs mirror the floor fields explicitly. StepOutcomeRecord is
{message:Message,template_id:String,context:PlanEffect-or-null,plan:PlanRef,
step_id:StepId,review_call:CallId-or-null,work:WorkRef,outcome:ReviewOutcome}.
ReviewOutcome is externally tagged closed:{} or failed:{case:ReviewFailure}.
ReviewFailure is mismatched_work, mismatched_setup, assessment_failed, refused, errored
or killed. assessment_failed is the caller's negative assessment, not a prescribed
report grammar. The two mismatch cases identify the structural identity checks in
CLOSURE-EVIDENCE. The last three identify the corresponding terminal tool outcome.
Successful context is exactly the close effect, with matching outer IDs. Failed context
is literal null. A closed record with null review_call must carry the close effect from
attest_closure, with the same null and WorkRef. Its category must allow closure without
review. Its copied null sidecar maps to null context/effect in StatementFile, never to a
mutation. PlanBreakReport has PLAN-BREAK's exact fields. PLAN-OBSERVATION defines
PlanMeasurement. loop.instruction uses Spec 02's INSTRUCTION-METADATA grammar. [FLOW-3,
FLOW-8, PLAN-2, SORT-4]

plan.break is metadata and is not filed as a plan mutation. The preceding classified
plan.event supplies that mutation. state.plan.measurement carries PlanMeasurement,
channel internal, origin loop, excluded from residue as measurement. Every judgment
precedes the action it permits. No existing event is rewritten when a later review
completes. [SORT-4, FLOW-3, FLOW-8, COST-4]

**PLAN-OBSERVATION.** PlanMeasurement is
{instrument_version:String,plan:PlanRef,event:PlanMeasureEvent}. instrument_version is
the literal plan-observation/1. PlanMeasureEvent is externally tagged with the three
variants below. Every field is required. There are no omitted fields or implicit zero
samples. IDs, counts and clock readings use DecimalU64. All plan and step identities
name the loaded seed. Reason: the record must distinguish a measured value, an
unavailable read and an interval that never began. [PLAN-3, PLAN-4, COST-4, FLOW-3]

| Variant | Exact body |
| --- | --- |
| flags | after:SourceBlockId, request_id:DecimalU64, observation:PlanFlagObservation |
| next_ready | closed:SourceBlockId, previous_step:StepId, step_id:StepId, assembly_id:DecimalU64, began_ns:DecimalU64, ended_ns:DecimalU64, closure_to_next_ns:DecimalU64 |
| fit_refused | step_id:StepId, assembly_id:DecimalU64, stage:PlanFitStage, code:PlanFitCode, mandatory_fit_refusals:DecimalU64 |

PlanFlagObservation is externally tagged measured:{snapshot:PlanFlagSnapshot} or
unavailable:{code:ReadRefusalCode}. ReadRefusalCode is the closed STATE-PAGES read
refusal set plus the common answer_too_large code. It adds no vocabulary or free-text
reason. A measured snapshot must echo the outer plan and after. request_id identifies
the original read, not a next-page or release request. The count fields are never null,
while SourcePosition.turn retains its ordinary explicit null for a turnless source. A
refused read has no invented snapshot or count. [PLAN-3, SEAM-3, COST-4]

Sampling is explicit. observe_flags(plan,after) reads PLAN-FLAG-READ and records its
exact result or typed unavailability. It does not run on every transition. The caller
may supply a next_ready interval only with a same-session closure receipt and an actual
placement receipt for the named successor. The observation port checks
plan/step/revision, clock ordering and receipt identities. It starts no progression and
fabricates no zero interval for missing endpoints.

record_fit_refused accepts a SelectionRefused or PlaceRefused receipt plus the plan/step
attribution. PlanFitStage is mandatory_probe or place_backstop. PlanFitCode is
tail_over_capacity or generation_room. Count exactly one for a run/assembly_id.
mandatory_probe requires mandatory_over_capacity. The backstop requires no optional
selected units. The caller supplies the attribution and the primitive verifies it
against the units' held plan/step references. [PLAN-3, PLAN-4, COST-4, R20-12]

Pending trace capacity retains each immutable observation under its original author
ticket. Only Recorded completes it. Retrying the ticket never samples a new endpoint or
increments the count. A halt follows the existing terminal record rules and does not
fabricate an unrecorded observation. [FLOW-3, FLOW-4, COST-4]

## 6. Enforcement and acceptance

**INSTRUMENTS.** The implementation transcribes the state and control tables into
fixtures through the real typed adapters and store transactions. A pass certifies the
primitive's stated authority, correlation and atomicity, not the caller's review policy
or task success. [PLAN-1 through PLAN-6, R20-12]

| Claim | Instrument and evidence | Protective change that must fail |
| --- | --- | --- |
| PLAN-SUMMARY-EVIDENCE | Member current/historical reads and reload verify held-source derivation. Harness fixtures check strict shape, held receipt/digest expectations, Recording/Recorded gating and failed refresh | Member hashes snapshot or substitutes turn/source. Harness omits required fields, ignores held expectation, projects early or serves stale success after failed refresh |
| PLAN-ADMIN | Root/sudo cause, fixed-file, extra-argument, stdin and whole-envelope cases | Remove cause check or measure unencoded text only |
| PLAN-TYPES and CATEGORY | Stable-ID and operator-only category fixtures | Reuse a retired ID or accept model category changes |
| PLAN-EXCHANGE | Busy, stale, cancelled, refused and lost-answer transitions | Blindly retry an unanswered committed update |
| PLAN-ADMISSION-REFUSAL | Busy/cause/inner refusal, prior-author custody, EOF, trace-before-send and both expiry boundaries | Answer before Recorded, admit after idle, steal a ticket, renew either bound or discard accounting at EOF |
| PLAN-PREFLIGHT-READ | Every listed initial/next code, invalid leg/code, acquired release, record pressure, Stop/leave, expiry and post-effect contrast | Lose typed code/detail or measurement parent, invent recall, retry, renew the bound, author an effect, halt a valid preflight refusal or retain stale success after a failed post-effect refresh |
| PLAN-PORT-REFUSAL and PLAN-PORT-RECORD | Every permitted initial/next code, counter overflow, correct template rejection, source/record pressure, Stop/leave, failed release and post-effect contrast | Break a plan on resource refusal, lose code/detail or parent, return before Recorded, renew a bound, skip cleanup or treat a failed refresh as a healthy refusal |
| PLAN-FEED and PLAN-EFFECT | Trace-to-receipt fixture and private authority compile pin | Construct root authority from a gate frame or bypass trace |
| PLAN-STORAGE and PLAN-READ | Literal DDL, current/history reads, every effect and rollback | Commit marks without the plan label or allow two active steps |
| WORK-REFERENCE | Complete released bytes across page sizes, encoding-sensitive digests, closure recheck and historical verification after dump/load | Hash a page or parsed reconstruction, omit a pair, trust a model digest, accept an open pair or use a later frontier |
| CLOSURE-EVIDENCE and CLOSURE | Matched review pair, revision, consumed-grant cases and a failed assessment traced and filed with null context | Accept model prose, stale work, ordinary external lane or reused evidence, omit the failed record or close a step from it |
| PLAN-STATES | Create after complete and broken, replace and old-history reads | Reject a legal new plan or overwrite its predecessor |
| FLAGS | Condition query after reorder/retirement | Retarget or delete a held flag |
| PLAN-OBSERVATION | Explicit requests, receipt identity and duplicate assembly corpus | Invent zero, cross run clocks or count the same refusal twice |

No acceptance test needs a particular loop strategy or review script. Protocol fixtures
call the primitive ports directly with valid and invalid grants. The store's structural
closure pass does not certify the external reviewer. [PLAN-2, SEAM-2, R20-12]

Exercise both category values with reviewed and unreviewed grants. Unreviewed closure
succeeds under either value, and either value may use a valid review. Remove the
harness's category-existence check, the private-grant check or the unchanged-work check
separately and require each fixture to fail. The store's category existence is held by
plan_step's foreign key to plan_category, so removing its Rust lookup alone fails no
fixture. Repeat the unreviewed closure under review_required true after dump and load
and after a replacement that preserves it. [PLAN-2, PLAN-5, R20-17]

## 7. Owning amendments and dependencies

**AMENDMENTS.** The following exact owning clauses join this document act. Every
replacement retains its foundation transition notice. [PLAN-6, FLOW-7]

| Document | Sections | Amendment |
| --- | --- | --- |
| weaver-admin-Spec | 2, 3, 4 and 7 | Root-only plan-update, fixed input, lock and typed answer |
| weaver-admin-operator-contract | 4.1 and 5 | Verb/cause, PlanUpdated and PlanUpdateRefused |
| weaver-admin-harness-contract | 2 and 3 | Pinned update bytes and receipt-backed answer |
| weaver-types-Spec | 2 and 4.2 | Plan identities, operator input, effect and result |
| weaver-harness-Spec | 5, 6, 6.1 and 8 | PLAN-PORT and its private grants, no workflow |
| weaver-harness-gate-contract | 2 | A caller-requested review uses the ordinary tool contract |
| weaver-harness-state-contract | 2 and 3 | Guarded plan effects and typed reads |
| weaver-state-Spec | 3 and 4 | STATE-PLAN-SCHEMA, STATE-PLAN-FILE, STATE-PLAN-READ and STATE-PLAN-FLAGS |
| weaver-trace-PRD and Spec | 3.1 and 3 | Plan records and measurement vocabulary |
| weaver-harness-diagnostic-contract | 2 | No serving plan updater or plan-vocabulary conversion |

This new primitive depends on the flow and source retention specified by the
foundation's INV-2 and INV-5 clauses in section 6. It also consumes Spec 05's complete
pair and Spec 06's span/mark contracts. Spec 08 can consume condition-query
units without a mandatory step bundle. These are type and transaction dependencies, not
a prescribed runtime call order. [PLAN-1, FLOW-7, R20-12]

## 8. Open measured values

Measure the plan operation's decision-wait bound, complete update memory, query/result
limits and observation cost. No production value is invented. The choice of reviewer,
its script and interpreter, progression scheduling and context composition belong to the
later loop. They are outside this Spec's measured-value list. The plan-update verb was
approved by the operator on 2026-10-20. [PLAN-2, PLAN-6, COST-2, COST-4, R20-11, R20-12]

[transition]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
