# Dependable flow: contract amendment text

**Status:** MERGED.

**Version:** 0.17 **Date filed:** 2026-10-20

## 0. What this document is

Each section supplies replacement or additional contract text, not a request for the
coder to design a contract. Representations live in the matching crate amendments. The
PRD IDs name the reason for each obligation. All unnamed exchanges keep their existing
meaning. A contract below must land with its parties' amendments.

## 1. Harness-trace contract, sections 2.1, 3, 4, 5, and 7

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

After admission and canonicalization, the recorder makes the one tap attempt before
external handoff. It returns the assigned event identity and the tap outcome to the
harness. The outcome distinguishes an unselected copy, selected source blocks, and a
failed tap. A failed tap does not recast an already recorded event as an unrecorded
submission. The canonical trace record remains whole. The trace head has a distinct
untapped disposition. [FLOW-2, FLOW-3, TAP-4]

The harness supplies kind, channel, and origin from the producing boundary, never from
claims in the content. It supplies a validated declaration. The recorder may assume that
declaration has passed the registry check, but must report a failure if an admitted
event cannot be projected as declared. It must not turn a bad projection into an
unselected event. The harness must consume every tap outcome, even when the original
author ignores the event's returned sequence. [TAP-1, TAP-3, FLOW-4]

A submission waiting for trace capacity has not been admitted and has no sequence. The
harness retains that offered event and continues servicing control, failure, and
deadline observations. It starts no dependent decision before that event is recorded.
The recorder never changes a record after its tap. Its later delivery failure is a trace
failure, not evidence that a store receipt was false. [FLOW-2, FLOW-6, CLASS-8]

After the flow halt, failure and shutdown authoring may continue. They cannot restart
state filing or authorize another decision. Failure-report delivery is subject to the
trace's existing shutdown and durability contract. [FLOW-4 through FLOW-6]

Add state.store.measurement to the serving vocabulary as excluded measurement. The
harness preserves the member observation exactly and adds its own transport timestamps,
using the one monotonic origin. state.flow carries transition and interval counts, with
the final interval in the halt account when the flow fails. Neither carrier licenses a
second filing or a decision. The trace refuses a kind, payload or envelope mismatch, and
the reader must distinguish unknown counters from measured zero. Representation and
timing are Spec 09's and TRACE-OBSERVATION's. [FLOW-3, TAP-3, COST-2, COST-4, COST-5]

Add plan.event, plan.break and state.plan.measurement beside step.outcome in the serving
vocabulary. The harness supplies TRACE-PLAN's PlanInputRecord only for verified operator
input and PlanEffectRecord only from its licensed coordinator. It converts
StepOutcomeRecord and PlanBreakReport without changing identities, outcomes or null
context. The recorder refuses a payload, channel, origin or turn combination outside
TRACE-PLAN. It may assume the harness owns the operator and review grants, but never
treats recorded text as such a grant. Only the declared Text/context surface is
selectable. The raw operator document, break report and plan measurement are not
selectable content. [PLAN-1, PLAN-2, PLAN-5, PLAN-6, TAP-1, TAP-3, FLOW-8]

SourceBlockId identifies one original trace Text block. CallId identifies one
communication, StatementRef identifies a binding within its source, and SpanRef
identifies a stored span. None is interchangeable with a request ID, which correlates
one protocol exchange. [STORE-3, PAIR-3, MARK-6, SEAM-2]

## 2. Harness-state contract, vocabulary and sections 2 through 8

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Replace the operating distillate and no-receipt clauses with these terms:

A filing is one selected source block, its source envelope and projection address, and
the bindings or pair metadata required by its content class. Statement filings carry
classified verb/noun/condition spans. A licensed mark statement also carries its copied
mechanical context and matching effect. Context refers to held content, and cannot carry
unclassified message text as another content feed. Pair filings carry the complete
request or terminal record and its request-statement reference, without classification.
A filing identity names the source, not a send attempt. The seed is outside this
operating exchange. The store owns the schema and confirms its supported filing schema
when the channel opens. The harness may not supply table definitions. [STORE-2 through
STORE-5, SEAM-1]

The harness supplies mark effects only under the authority of its boundary or plan
coordinator, after the associated statement passes classification. The store may assume
that classification and operator/review grants belong to the harness, but must refuse
mismatched source context, invalid references, crossed scopes or an attempt to alter an
ended mark. It commits the source, index and effect together. Neither party may infer a
closure from the model's claim that work is done. [MARK-1, MARK-5, PLAN-2, SEAM-2]

A mark read returns the content fixed at that span's end, including whole pairs that
existed by that frontier. A preview instead names a committed frontier in an open span.
It writes nothing and supplies no closed-mark or cut authority. The harness must
preserve the result distinction. The store must refuse partial or unterminated included
pairs rather than return a partial marked span. Later filing cannot change an ended
span's membership. [MARK-3, MARK-4, MARK-6, PAIR-4]

subjects selects whole message.user/message.assistant blocks by exact noun and returns
every binding and pair of each chosen block. statement resolves exactly one original
StatementRef. All reads expose binding ordinals from the filed block. The harness may
assume store commit order and complete attachments, but must refuse malformed results or
wrong query variants. The store refuses invalid selector lists and absent statement
identities explicitly. Neither side may turn a result-size refusal into empty history or
treat a filtered binding's array position as its original ordinal. The selected result's
byte limit includes all pair attachments. [READ-1, READ-2, READ-4, PLAN-3, STORE-3,
PAIR-4]

The store holds at most one immutable read result for the channel. A refused initial
read leaves none. The harness releases a result only after acquiring its snapshot
identity, once on success or a valid later refusal. A failed channel drops the result
instead. The read's existing deadline includes release, and neither party can extend it
by paging or cleanup. [SEAM-3, CLASS-8, COST-2]

Reading held statements does not refile them. An inward recall call does have its own
new send and return pair under READ-3 and PAIR-1. Its complete return may contain the
held query result, but those nested statements retain their old identities and acquire
no new statement bindings from that return. The recall provenance event and placed
tool-result copy are not admission carriers. [STORE-2, STORE-8, READ-3, PAIR-3]

The harness sends each filing at most once on the standing channel. The store either
commits its block, index entries, and associated metadata together and answers accepted,
or leaves all of them unchanged and answers refused. Acceptance names the filing and the
request it closes. It carries no replacement content. No acceptance may be emitted
before commit. A lost answer leaves the commit outcome uncertain. The harness must not
resend to resolve that uncertainty. [SEAM-2, FLOW-4, FLOW-5]

One harness owner sequences all write and read traffic. The replacement read and
lifecycle exchanges in this unit run through that owner. A read observes filings
accepted before it, not queued filings. A save may proceed only after the flow has
accounted for all obligations through its requested boundary. The publication and
snapshot mechanic is the three-party persistence contract in this unit. [SEAM-2, SEAM-3,
MARK-4, PERSIST-4]

Each correlatable serving response carries the member's operation observation, including
a refusal and each capture continuation. The store samples its own work, never the
harness's transport or admin's publication. It retains the measurement as separate
operation metadata under STATE-OBSERVE, never as classified content. Only an explicit
observations query returns those records as query data. The harness validates
correlation and timing before exposing a completion, then records the exact observation
on the trace. It may assume a well-formed unavailable counter is unknown, not zero or
component failure. It must refuse a malformed required observation as a malformed
answer, preserving unknown commit status and never resending. Measurement carriage does
not weaken the after-commit meaning of acceptance. Spec 09 STORE-OBSERVATION and the two
owning Specs supply the representation and sampling boundaries. [SEAM-2, SEAM-3, FLOW-4,
COST-2, COST-5]

| Standing | Harness may cause | Store may cause | Next standing and invariant |
| --- | --- | --- | --- |
| Opening | Identify run and required schema | Confirm or refuse | Ready only after matching confirmation |
| Ready | One file or control request | Nothing unsolicited | Awaiting that request's answer |
| Awaiting file | Continue carrying that same request | Accepted after commit, or refusal after rollback | Ready on acceptance, failed on refusal |
| Awaiting control | Continue its current exchange | That control's next answer | Its existing continuation or Ready |
| Any open standing | Detect component failure | Close on unrecoverable failure | Failed, no reopen in this run |
| Failed | Close and report | Nothing further is relied on | Terminal for this run |

A parseable refusal names its request. An unparseable request whose identity cannot be
trusted closes the channel. A refused filing, malformed answer, mismatched answer, or
lost channel halts serving. Correct classifier rejection is outside this seam and is not
a store refusal. Delayed output filing remains pending until a model-blocking wait
reaches its declared bound. The harness retains all owed copies within its measured
retention capacity and reports increasing lag. If it cannot retain a whole new admitted
copy, that is intake failure and ends serving, with the failed copy accounted. It never
drops a queued block, admits part of a copy, or treats age alone as failure. No silent
drop or continue-without-state rule remains for serving operation. [FLOW-4, FLOW-5,
CLASS-8]

Diagnostic replay retains its separately chartered access and is never granted this
serving writer by a serving load. No live reattachment repairs a failed channel.
[SCOPE-1, FLOW-5, October 18 R8]

Replace section 8's dead-peer conformance claim for serving with this instrument: kill
the real state member while a selected filing is pending, and prove the harness stops
every later decoder request and tool dispatch, records the owed source and its uncertain
commit, and exits without a save point or successful Left. Repeat with a lost receipt
after a real commit. Removing the halt check must make the test fail. Diagnostic's
separately specified retirement behavior is unchanged. This replaces the old assertion
that killing the member leaves turns unaffected. [FLOW-4, FLOW-5, SEAM-2]

## 3. Admin-harness contract, sections 3, 5, 6, and 8

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Admin supplies the parsed state-management declaration and the already established state
connection under its existing custody. After the first load event, the harness checks
the semantic partition, activates the recorder, starts the STE100 duty and checks the
template inventory, in that order. The load names the inventory, and turnless
ste100.result events carry the replies. All checks precede the boot content read and
user input. A failed check refuses enter through AfterLoad with attributed
state-management validation details. Admin then applies its existing failed-enter
rollback. A confirmation certifies that the required flow stands, not that every future
input will conform. [TAP-3, CLASS-4, CLASS-8, GATE-1, review S2]

The enter's existing overall bound includes opening the state flow, template checking,
and boot reads. No component independently restarts that budget. A serving load requires
a state member, the STE100 duty, and a verified starting store. Diagnostic load has no
serving coordinator. [PERSIST-5, PERSIST-6, FLOW-4, SCOPE-1]

A terminal flow failure ends the run without a save point. It cannot become a clean
leave or a successful save. An admin invocation observing the end uses the existing
unclean-run recovery rules. The harness does not impersonate an operator invocation or
publish a save point. [FLOW-4, FLOW-5, October 18 R3]

The capture continuations and timing follow the persistence contract and foundation
section 5. The on-demand directive supplies its absolute save window. A leave keeps its
existing graceful phases and announces a capture window only after the lower. Capture
completion ends that window, not the leave, its shared tail or the duty to answer Left.
The caller may not infer a phase-specific refusal from silence, and no continuation
renews a deadline. These additions reach the typed lifecycle consumer in admin and the
Rust worker. [PERSIST-4, FLOW-5, COST-4]

## 4. Harness-gate 2, 3, 5, 7 and gate-world 2, 3, 5

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

The gate continues to transport the caller's input and kernel-derived identity to the
harness. The harness performs STE100 checking before offering that input to the model.
The gate acquires no classifier connection and does not inspect content for conformance.
A refused input is answered with a restatement request and is recorded, but never enters
a later model argument. [GATE-1, CLASS-4]

A component failure while checking input is a failed serving run, not a conformance
refusal. If the current caller can still be answered before a turn opened, the harness
returns the existing unnamed stopped close, with neither run nor turn, then closes the
serving boundary. Spec 02 INPUT owns its reason text and wire representation. After a
turn opened, its stopped answer names that run and turn under the existing grammar. The
trace retains the failure reference. A broken gate cannot deliver an answer, so
diagnosis remains the trace and run end. The gate's tool clock and containment remain.
ToolInterrupt now carries unload or flow_failure. Every existing unload sender supplies
unload explicitly, and a flow halt supplies flow_failure, returned as
Killed{by:flow_failure} only after containment. Fault still means the gate died with no
answer and the outcome is indeterminate. Before a terminal result is sent, flow_failure
supersedes an unload interrupt's cause without restarting the deadline. A result already
sent always stands. [FLOW-4, FLOW-5, GATE-1, COST-4]

A correctly refused input has no opened turn. The harness records input.offered,
ste100.result where applicable, and refusal/Input. It returns the existing refused JSON
response under Spec 02 INPUT, which owns its exact reason and wire shape. No input
fragment reaches argument assembly before its Text block passes. Role system is no
longer a seeding command and is refused as invalid_input_shape. The gate relays this
answer without acquiring classifier or store access. [GATE-1, CLASS-4, PERSIST-5]

## 5. Required adjacent contract work

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

The new STE100 duty is governed by the complete [harness-SPU STE100
contract](weaver-harness-spu-ste100-contract.md) in
docs/crates/state-management/ under R20-14. The existing
label-scoring agreement remains unchanged. Its definition sites are
TYPES-STE100-ADMISSION, TYPES-CLASSIFIER, HARNESS-CLASSIFY and SPU-STE100. Spec 02 owns
representation, transport, artifact grammar and template proof. Test doubles use the
exact near-side shape and never substitute for a qualified production duty. [CLASS-1
through CLASS-8, October 18 R10]

The seed/boot and structured-read contracts are specified in this combined unit. Spec 05
supplies the tool-pair contract, consumed by the shared file_pair channel. Its
unclassified metadata remains a copy from the same tap. This packet grants none of them
an untraced write path. These amendments must be read with the crate state tables, which
elect the concrete transport and failure mapping. [FLOW-1, STORE-8, PERSIST-5, PAIR-3,
SEAM-3]

## 6. Change protocol and diagnostic conversion

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Add to harness-trace contract 7's change protocol: this unit expands the closed serving
set as enumerated by TRACE-REGISTRY and introduces state-copy/1 with Text ContentBlock
and whole pair-payload projections. The serving caller and recorder change together. No
older distillate or whole-system exception is accepted as an alternative representation.
The trace's no-internal-crate boundary still stands. [TAP-1 through TAP-5, SEAM-1]

state-copy/1 names the tap projection, state-flow/1 names the store channel protocol,
and state-content/1 names the member's content schema. They are separate versioned
contracts, not alternative names for one payload. [SEAM-1, SEAM-2, STORE-8]

In harness-diagnostic contract 3 and 7, and diagnostic Spec 3.1/3.2, state that the
converter handles the legacy diagnostic envelope with no channel/origin fields. For
shared legacy kinds, the converter explicitly drops serving channel/origin while
preserving each diagnostic member, as DIAGNOSTIC-CONVERT specifies. New serving kinds
and payload variants have no conversion. Diagnostic runs author their existing 20-kind
set only and reject the new serving kinds in TRACE-REGISTRY as
OutsideDiagnosticVocabulary. New serving recall/save payload variants and unloads
carrying either TRACE-HALT failure field are rejected too. Diagnostic preload retains
its legacy schema and is selected only by an admin judged diagnostic binding. Amend
analysis-state contract 1/2/3 to exclude use of that door as a serving seed builder. The
replacement is defined here, with the external party's landing explicitly owed.
[SCOPE-1, PERSIST-5, October 18 R8]

In harness-state Parties and section 7, the schema is the member's versioned contract.
The opener names a supported schema ID and never supplies SQL or table definitions.
Serving compatibility changes require both endpoint amendments and schema validation.
Diagnostic caller schemas remain isolated in their separate mode. [SEAM-1]

The strict schema, read-page and capture control representations are in TYPES-READ,
TYPES-PERSIST, STATE-READ and STATE-PERSIST-STATES. No caller may replace them with
legacy ask JSON merely because the outer control field can hold raw JSON. The parser
refuses unknown operations. [SEAM-1, SEAM-3]

## 7. Harness-SPU decode contract, sections 2 through 5 and 8

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Replace section 2's six-exchange count with seven and add the read-only measurement
exchange beside the existing decode exchanges. The harness supplies the exact canonical
proposal and granted tool schemas, with the resident prefix it proposes to keep. The SPU
renders through the same family and tokenizer as append and returns the resulting counts
without decoding or changing the resident sequence. Open also returns the actual prefix,
capacity and effective generation limits. The harness may rely on those facts, not on a
token count it derives independently. [PLACE-1, PLACE-3, PLACE-5]

The SPU may assume one outstanding request on the existing decode channel. The harness
must refuse an unexpected, malformed or incomplete answer, and may not reuse a
measurement after an unrelated resident mutation. The SPU refuses an unopened session,
out-of-order ask or malformed family input. An oversized proposal may be measured
without being appended. The measurement pass certifies exact size under that residency,
not selection quality or a deployment budget. [PLACE-3, PLACE-4, PLACE-5, FALS-4]

Only the existing Flush and Elide operations reduce the buffer. The harness grants them
between external turns, and measurement cannot supply that grant. It may measure an
append between tool rounds while keeping the whole resident prefix. The identity prefix
remains permanent. The existing physical overflow, whole answer, cancellation and
terminator guarantees remain. New counts and schema carriage use the existing
whole-message channel, without another socket or a store feed. [MARK-4, PLACE-5,
PLACE-6, October 18 R7]

Conformance cases compare measurement with actual append rendering, prove no
resident/backend/sampler mutation during measurement, and exercise every refusal and
turn-boundary guard. The paired amendments must clear together. Diagnostic senders
consume the same token vocabulary while their separate replay operation retains its own
scope. [PLACE-5, PLACE-6, SCOPE-1]

The R20-15/R20-16 composite ends Staged. The harness must pin its replacement as the
mandatory head of the next ordinary AppendAndGenerate and measure the entire delta,
including later input. It may assume only the confirmed cut is resident before that
append. The complete Generated resident count and existing measurement token counts
confirm the placement, including the terminator. A mismatch is component failure. ReFeed
retains diagnostic permission, and no new serving exchange is added. Both sides enforce
Spec 06's STAGED-APPEND and the SPU's existing count guarantees. [SEAM-2, PLACE-5,
R20-15, R20-16]

## 8. Plan control and review, owning contract amendments

These additions apply at the [foundation cutover][cutover]. Until then, each previous
serving clause remains applicable.

**PLAN-CONTROL-AGREEMENT.** Amend admin-operator sections 2 through 5 and admin-harness
sections 3, 5 and 6. The root operator may supply a plan update only through admin's
fixed plan-update command. Its fixed protected input belongs to admin's configuration
root, not the caller's command line or stdin. Neither connector role gains the verb.
Admin must refuse a nonroot cause or untrusted input file and may assume only the
root-custodied bytes it read once. [PLAN-6, R5]

The worker receives those exact bytes and their digest over the existing judged
coordination channel. It must refuse an incompatible current revision, busy state,
malformed operation or nonconforming input without changing the held plan. Accepted
proposal statements can remain held. It must not report an update accepted before the
plan effect's store receipt. Admin may assume a matching success names that commit, not
a disk save or proof that the plan is adequate. Missing answers leave commit status
unknown and cannot authorize a retry. Observe's committed plan summary and the trace
provide the account. Representation, byte bounds and actor states are owned by
ADMIN-PLAN, TYPES-PLAN-CONTROL and HARNESS-PLAN. [PLAN-6, STORE-5, FLOW-4, SEAM-2]

**PLAN-STORE-AGREEMENT.** Amend harness-state sections 2 through 5 and 8. The harness
supplies only held statement references and a trace-copied plan effect under its private
operator or typed closure grant. It may assume a receipt commits the source,
materialized plan and required marks together. The store refuses stale identities,
unknown references, changed preserved closure or incompatible mark/plan state. It stores
the supplied label but does not run the tests, judge work or authorize an operator. A
read returns the same stable IDs and source evidence after reload. No new direct
plan-update store call or side feed exists. [PLAN-1, PLAN-2, PLAN-6, MARK-5, SEAM-2,
SEAM-3]

The plan_flags read supplies PlanRef and the exact last-effect source. The store may
assume the caller requests a mechanical count, but must refuse an absent, wrong-seed or
stale identity. It returns PLAN-FLAG-READ's one snapshot and counts, with ordinary
bounded paging and release. The harness must check the result tag, identity echo and
count sum before recording it. It must preserve a typed refusal as an unavailable
reading, never zero or a reversal of the committed update. Neither party writes a flag's
target or decides task relevance. [PLAN-3, SEAM-3, COST-4, SCOPE-1]

For the harness-trace agreement, PLAN-OBSERVATION has one author in the plan
coordinator. The trace receives the exact tagged reading and enforces its payload and
envelope licensing. The harness owns receipt/placement clock endpoints and assembly
deduplication. Sort and place supply outcomes, not another plan counter. A pending
author ticket preserves its original reading. Both parties refuse a measurement
projection, and the external reader treats an unavailable or censored reading as
incomplete evidence rather than measured zero. [PLAN-3, PLAN-4, FLOW-3, TAP-3, COST-4]

**PLAN-REVIEW-AGREEMENT.** Amend harness-gate section 2's model-request-only supplies
clause. The harness may additionally send a review request on the trusted caller's
request, bound to the held operator setup identity, plan and immutable work. It uses the
same granted shell, execution shape, confinement, clock and outcomes. The gate may
assume the ordinary request contract and must refuse the same unavailable tool or
inadmissible clock. It interprets neither a plan nor a review report. The harness must
refuse a model report as review authority. Closure follows the step's operator
Category.review_required: true requires a matching held review return and recorded
assessment, while false also permits Spec 07's private attest_closure grant. Both forms
bind current work and require the complete structural evidence. The gate has no role in
the no-review form. No new runner, process, socket or timeout clock is authorized.
[PLAN-2, PAIR-1, PAIR-2, R6, R20-17]

**PLAN-DIAGNOSTIC-AGREEMENT.** Amend harness-diagnostic record vocabulary to reject
these serving plan events as OutsideDiagnosticVocabulary. No serving plan updater or
reviewer is introduced into diagnostic mode. WeaverAnalysis reads the new trace records
under its independent serving reader, not by a link into this store. [SCOPE-1, October
18 R8, October 18 R9]

The contract pass means the exact actor, source, refusal and receipt obligations above
are represented on both sides. Compile pins and the actor/SQL perturbation fixtures in
Spec 07 and its owning amendments enforce them. It is not evidence that the review call
and its input are adequate or that a production tuple has qualified. [FLOW-7, PLAN-2,
PLAN-6]

## 9. Wind-down and operation closure, owning lifecycle contracts

These additions apply at the [foundation cutover][cutover]. Until then, each previous
serving clause remains applicable.

**OPERATION-AGREEMENT.** RepairLedger can be used by a lifecycle caller under its
existing earlier bound. A correct refusal grants no successful answer and sets no
component halt. This set changes no wind-down pass count, summary policy or lifecycle
schedule. Such wiring belongs to the loop. A broken classifier or carry still fails the
run and permits no save publication. [CLASS-8, CLASS-9, FLOW-4, PERSIST-4, R20-12]

In harness-gate contract sections 3 and 4, state-management operation refusals use the
same single external close and existing named stopped client shape. They do not create
another turn, clock or execution. The exact record-before-close ordering is
HARNESS-OPERATION-CLOSE. Each primitive retains its own refusal payload. There is no
StateTurn refusal enum. The gate still relays these responses without interpreting their
cause. [FLOW-3, PLAN-5, PLACE-4]

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
