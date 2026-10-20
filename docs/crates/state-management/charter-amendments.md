# Dependable flow: charter amendment text

**Status:** MERGED.

**Landing PR:** #182

**Version:** 0.18 **Date filed:** 2026-10-20

## 0. What this document is

These are replacement clauses for the destinations below, grounded in PRD v0.6 and
October 18 A1, R3, and R10. Their cutover clauses govern when they take effect. All
other clauses remain in force. The destination registers must name work that lands
later. The source anchors use local main 2eb5c5a2.

## 1. Trace PRD sections 3.1 and 11

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Replace section 11's envelope-only default, unconditional whole copies, store carry, and
detached operation with this text:

The tee copies the completed trace traffic at its last content boundary. The trace
continues whole. The partition selects from that copy by event kind, channel, and
origin. Projection determines which source blocks the admitted copy holds, never whether
the event is admitted. The tee yields the copy to the harness and has no store
connection. The loader checks the declaration against the allowed projection surface
before serving. No configuration can admit measurement payload, save points,
restored-message copies, configuration, the trace head, or a boot read. The seeded
system prompt is already held and is never filed again. [FLOW-1, FLOW-2, TAP-1 through
TAP-5, STORE-8, PROMPT-1, review S1]

The trace head is the existing load record, extended by the seed and configuration
identities. It does not pass the tap. Every other completed content record passes one
tap. A tap failure is reported separately from trace submission, so a record can be on
the trace while its copy failed. The harness then halts. The trace remains available for
the failure record and shutdown records, with no further content filing or decisions.
Sink delivery and durability remain trace obligations, including failure to deliver the
failure record. [FLOW-2 through FLOW-6]

In section 3.1 add the following kinds, with payloads elected in the trace amendment:
`input.offered`, `loop.instruction`, `sort.decision`, `step.outcome`, `state.flow`,
`ste100.result`, `tool.return_rejected`, `state.mark`, `state.mark.position`,
`state.placement`, `state.sort.measurement`, `state.store.measurement`, `plan.event`,
`plan.break` and `state.plan.measurement`. The offered input is trace evidence before
conformance, not a conversation message. The flow account is measurement. Existing
`fault`, `refusal`, `recall`, `load`, and `unload` keep their meanings. The label
scorer's two classify kinds keep their label-score meanings. The new ste100.result
records the complete typed binding result before filing. [GATE-1, FLOW-3, FLOW-8,
SORT-4, COST-4, R10]

plan.event records operator plan input or a new loop-authored plan effect. step.outcome
records the successful or failed review account. plan.break records the loop's break
report and supplies no store mutation. state.plan.measurement is excluded measurement.
TRACE-PLAN and Spec 07 own the payload and authoring rules. The harness authors each new
record when the decision happens. Nothing amends an earlier event or gives the trace a
plan or review judgment. [PLAN-1, PLAN-2, PLAN-5, PLAN-6, SORT-4, FLOW-8]

The state member returns operation measurements beside its replies. The harness records
them as state.store.measurement, with the queue and transport interval it observed.
Neither the member nor the trace files this payload. Unknown counters remain explicit,
and neither party claims disk-wait attribution from an unavailable sample. The trace and
store Specs define the representation and measurement boundaries under Spec 09. [COST-1,
COST-2, COST-4, COST-5, TAP-3]

state.mark records a load-certified boundary statement and its mechanical source
references. The declared partition may copy its text and licensed context. The store
applies the matching effect atomically after classification. state.mark.position records
the current canonical argument endpoint of a held span. state.placement distinguishes
boot, confirmed cut and prospective append, with source maps and SPU counts. The two
position kinds are excluded measurement. None permits the tee to inspect meaning or
alter the through-going trace. [MARK-1, MARK-5, MARK-6, PLACE-5, TAP-1, TAP-3]

Extend the fault case table with state_flow_failed and state_template_rejected. The
former names a terminal operating-flow failure. The latter names a correctly reported
conformance defect in harness text and never authorizes a component halt. The typed
refusal for a semantic partition or template check at enter follows load, uses the
existing refusal event kind, and carries the admin-harness refusal. The load records the
template inventory identity. The worker starts the STE100 duty and records its template
results afterward, before boot content and the user channel. An invalid partition
permits only the preflight recorder's refusal and terminal records, never operating
content. [FLOW-3, FLOW-4, CLASS-8, review S2]

## 2. State PRD sections 1 through 5

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Replace the clauses that permit the turn to continue after loss of the derivative, that
give the opener ownership of schema, and that permit silent ingest rejection. The exact
targets are weaver-state-PRD section 3's "Losing the member loses the derivative and
never the account" paragraph and section 4's "The schema is the loop's" paragraph, with
their continued-serving and missing-schema behavior. The replacement is:

The store owns its schema, reads, and writes. A content write carries content selected
from the tap with its attribution and the required classification. The store accepts the
complete filing atomically or refuses it. Acceptance is evidence that the write
committed to the in-memory holdings. The harness receives that acceptance before
considering the filing complete. A refused filing, broken carry, or failed store is a
failure of the operating state flow and stops serving. A correct classifier rejection is
decided before this write and is not a refused filing. [STORE-1 through STORE-5, SEAM-1,
SEAM-2, FLOW-4, FLOW-5]

A halted run takes no save point. The last published save point remains the default
starting state. There is no attempt to publish incomplete holdings as the next starting
state. The trace records the failure and outstanding obligations. This preserves #99's
broken-seam rule under October 18 R3 and supersedes the October 16 spec-review S3 dump
requirement. [FLOW-4, FLOW-5, R3]

The empty first load, live seeding turn, and old event/field writer retire in this
combined unit with its seed, boot and read amendments. Every serving agent loads a
verified seed or subsequent save point. No intermediate deployment is authorized.
Diagnostic replay remains outside serving and can never be selected as a serving preload
route. [PERSIST-5 through PERSIST-7, SCOPE-1, R8]

Operation observations are retained separately as store-owned metadata under R20-19.
They use the same in-memory store and no external writer. The classifier and content
indices never receive them. The harness carries each response observation to the trace,
and only a typed observations query exposes held metadata as query data. This amends the
state PRD's write/read duty without admitting measurement trace payloads at the tee.
[STORE-1, SEAM-2, SEAM-3, TAP-3, COST-2, R20-19]

## 3. Harness PRD sections 2, 4 and 5, and apex section 5.5

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Replace the state seam's optional continuation on loss with the following:

The harness owns the operating flow's progress and terminal failure latch. Every content
write after load comes from the tap. The harness may continue computing while filing
lags, but it may not discard material still owed to the store. A wait that blocks the
model is bounded. On component failure it stops decisions and ends the serving run,
records the obligations that remain, and takes no save point. Classification rejection
follows the declared repair or refusal behavior and does not set this component-failure
latch. [FLOW-4, FLOW-5, CLASS-8, MARK-4, R3]

Add state_flow_failed and state_template_rejected to harness PRD section 5's closed
fault cases. A flow failure records its first cause, selected obligations, uncertain
acceptance and pending authoring, then stops serving. A correctly reported template
defect is an operation failure and never a component halt. Trace failure keeps
stream_write_failed. Repeated shutdown evidence cannot replace the first cause or count
as another halt. The owning Specs define the account and reuse the existing fault and
unload records. [FLOW-3 through FLOW-5, CLASS-8, CLASS-9, review S2]

In apex section 5.5 retain the `[state-management]` table and replace the universal
compiled-default rule: a strategy, automatic-retrieval election, and measured bounds
must be declared where their requirements call for them. No new `[state-flow]` or
`[state-serving]` table is introduced. Other existing defaulted keys are unchanged until
their owning amendment names a replacement. [TAP-2, STRAT-1, READ-2, CLASS-8]

## 4. STE100 and recall duties in the owning charters

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**SPU-STE100-CHARTER.** Add to SPU PRD section 6 and new section 16: the SPU has a
separate STE100 seam governed by weaver-harness-spu-ste100-contract. This is a required
duty for a serving state-management agent. It checks one fixed language instance and
returns complete bindings, a conformance defect or component failure. It also checks the
closure of the harness's declared templates under their permitted substitutions. It
holds neither the partition nor a store, tool or argument-writing motive. The existing
label scorer and its optional-by-presence rule in section 15 remain unchanged. [CLASS-1
through CLASS-8, FLOW-4, FLOW-8, October 18 R10]

The harness supplies the exact scheme and targets. The checker owns analysis and
qualified model execution. Its result supplies no authority to file, place or dispatch.
Its operational and failure contract is distinct from both labels and decode. The new
section's implementation is SPU-STE100 in the SPU amendment, with Spec 02 as its
representation definition site. [CLASS-2, SEAM-1, SEAM-2]

**TYPES-STE100-CHARTER.** Add to types PRD section 2.3: the floor holds the STE100
instruction, admission and request/result vocabulary shared by the harness and SPU. It
holds typed templates and seed-certificate validation that checks identity and source
spans. It executes no language model and owns no semantic qualification judgment. The
label vocabulary remains separate. [CLASS-2, CLASS-4, PROMPT-2, SEAM-2]

**HARNESS-STE100-CHARTER.** Add to harness PRD sections 2 and 4: the harness owns the
STE100 port, validates and records its results, and applies their declared dispositions.
It checks external content before model access, repairs model emissions and certifies
its own templates before service. It calls the pure internal recall constructor, while
carrying that query through its already owned state seam. It does not turn classifier
scores into a policy judgment or link another organ into the worker. [CLASS-2, CLASS-4,
CLASS-9, READ-3, SEAM-2, SEAM-3]

**ADMIN-STE100-CHARTER.** Add to admin PRD sections 2 and 4.1: inventory holds the
protected checker executable and scheme under the existing custody rules, and carries
the exact verified artifact identities into load. Admin validates the seed certificate's
identity and representation. It runs no conformance model and claims no semantic pass
from a checksum. [CLASS-2, PERSIST-3, PERSIST-5, PROMPT-2]

**INTERNAL-RECALL.** Replace internal PRD section 2's calculator-only cap for this act
and add to sections 3 and 5: the framework's second member is a pure constructor of a
noun-history query, required by READ-3. Its only input is the subject. It owns no store
connection and computes no retrieval decision. The harness executes the already decided
query and accounts for its result. The member has the direct pure calling surface. It is
not an advertised tool schema, shell command, registry entry or self-initiated activity.
This is the bounded framework addition authorized by the recall requirement. It grants
no further member by analogy. [READ-1, READ-3, SEAM-3]

## 5. Composite state service and prompt continuity

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Amend apex 5.5's one seam per service clause for this elected representation: the state
member exposes one composite custody service whose single owner serializes filing, reads
and lifecycle operations. Ordering between those operations is part of its contract. A
read or snapshot cannot secretly add another writer. Capture holds serving at a turn
boundary, and its delay is measured. No long control starts during a gate/cut check.
This is a named exception to the prior service-separation wording, not an assertion that
multiplexing hides latency. [SEAM-1 through SEAM-3, COST-4]

Replace apex 2's across-session paragraph: the member holds the admitted state in RAM
and restores an indexed, verified save point. Admin's selection stays outside the bound
declaration. With no serving start, load refuses until the operator's seed exists.
Diagnostic preload cannot build a serving start. An unclean run keeps the last
confirmed, indexed artifact, and the next load records its reset and selected start
together. A missing or malformed latest refuses, never silently falls back. [PERSIST-2,
PERSIST-3, PERSIST-5, SCOPE-1, R2, R3, R8]

Replace apex 9's parked file/identity account and state PRD 4's empty first load: the
operator's seed tool creates the first held prompt and mark before the agent exists. A
normal load reads the stored prompt and never files it again. A new prompt requires a
new seed and chain. Diagnostic preload does not seed a serving agent. No configuration
contains a prompt. [PERSIST-5 through PERSIST-7, PROMPT-1]

Amend admin PRD 3, 4.1 and 8 for the pre-agent seed duty. Admin's fixed root-only seed
verb reads the protected prompt draft and its certificate, verifies their attribution,
and asks a temporary member to insert the exact bytes. This exception exists before an
agent is created. It carries no prompt to a loaded agent and grants no arbitrary
turn-input verb. Normal load selects and verifies the bound declaration and dump, and
the harness reads the held prompt. Remove 4.1's claim that load reads a system prompt
named by configuration. The separate plan-update duty below has its own boundary.
[PERSIST-3, PERSIST-5, PERSIST-6, PROMPT-1, PROMPT-2, October 18 R1]

Replace harness Spec 6.1 and trace PRD 11's load-no-content wording: the boot recall
records the complete seated prompt for replay, excluded from state admission. Replace
trace PRD 3.1's numeric count with the closed TRACE-REGISTRY set. The primitive still
never changes an earlier event. [FLOW-3, PERSIST-6, STORE-8]

## 6. Registers changed by these clauses

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Destination | Text that yields in this unit | Replacement |
| --- | --- | --- |
| trace PRD 3.1, 11, Spec 3/11, harness-trace contract 7 | 26-kind count, unconditional copies, tee carry, detached mode, old election assertions | TRACE-REGISTRY/COPY/LOAD and contract sections 1/6 |
| state PRD 1-5, Spec 1/3/4/6, harness-state Parties/2/3/5/6/7/8 | Caller schema, event/field write, no receipt, silence, per-session asks and dead-peer continuation test | STATE-SCHEMA/FILE/READ and contract section 2 including its replacement instrument |
| harness PRD 4 state row and 5 fault cases, harness Spec 3/6/7, admin Spec 3.0 | MemberDies serves-on and old closed fault cases. Missing snapshot-leg timeout/retry stands under R20-6 | HARNESS-HALT/CONTROL-FAILURE/HALT-ACCOUNT and ADMIN-FLOW-END |
| apex 5.5 | One seam per service and universal defaults | Composite custody service above, explicitly declared elections |
| spu PRD 6/new 16 and types PRD 2.3 | Optional scorer read as STE100 port | R10 retains scorer, SPU-STE100-CHARTER and the separate STE100 agreement govern the new duty |
| internal PRD 2/3/5, internal Spec 1/2/5, harness PRD 2/4 and Spec 1/6 | Calculator-only cap and exact harness dependency set | INTERNAL-RECALL, INTERNAL-QUERY and HARNESS-RECALL add only the pure subject constructor |
| types Spec 2/4/5, admin Spec 4/6/9 | Optional state, none, old Enter/LoadFacts, old layout | TYPES-CONFIG/ENTER/PERSIST and ADMIN-VALIDATION |
| types Spec 2 paragraphs beginning The identity, [state-management], and The declaration carries no identity | Turnless system identity query, universal compiled defaults and #76 seeding turn | TYPES-CONFIG/ENTER/PERSIST, ADMIN-SEED and HARNESS-BOOT |
| harness Spec 6.1, state Spec 2/3, state PRD 4, apex 2/9 | #76 turn, empty first load, declaration-selected restore, serving offline builder, silent boot seating and parked identity source | ADMIN-SEED, ADMIN-SELECTION and HARNESS-BOOT |
| admin PRD 3/4.1/8 | Load reads prompt named in configuration, no pre-agent seed duty | Pre-agent seed exception and verified-start load above |
| gate-world contract 2/3, harness-gate contract 2/3/5/7 | System-role seeding and unload-only interrupt | User STE100 refusal and typed fault interrupt |
| admin-harness contract 3/5/6/8, admin-operator 2/3/4.1/5 | Seeding turn, fixed old command set, S3 save refusal, publication outcomes | Persistence contract and root-only seed/bind-state |
| harness-state contract 2 identity/snapshot, 3/5, state Spec 2/3 | Empty identity, one-file image, Election adoption, recovered room publication | Seeded span read, three-file format, schema adoption and confirmed publication |
| trace Spec 6/7, harness Spec 4 | Blocking full queue | TRACE-CAPACITY/WAKE and HARNESS-AWAIT-AUTHOR |
| harness Spec 4 and record.rs::convert, harness-diagnostic contract 3/7, diagnostic Spec 3.1/3.2 | Member-for-member serving conversion assumption | Harness-owned converter in diagnostic amendment, reject new serving-only vocabulary |
| analysis-state contract 1/2/3, state preload_door.rs | Serving offline preload interpretation | Diagnostic-only old schema, no serving builder |
| admin Spec 3.0 S0d load/unload/force-unload, S8b unload/force-unload, conclusion publication rows, 6/9/10 | One-file catalog and room recovery | PERSIST-FORMAT/RECOVERY and ADMIN-SAVE/RECOVERY, Open without Left |
| CLAUDE deployment account, HowToDeployANewAgent 0/2/3/4, REDEPLOY | --system, none and load-before-seed | ADMIN-RETIRE sequence |
| deploy/turn.py, create-agent.sh, verify-lifecycle.sh, test_plans.py, update-stack.sh | Old seed and adoption path | Remove --system, require seed, old-layout refusal tests |
| apex and Document Format 2 | Domain-charter location | Container exists locally under the Document Format exception, no new container act here |

The gate-world and admin-operator contracts reach WeaverWeb. The diagnostic and
analysis-state vocabulary reaches WeaverAnalysis. This packet stages text only.
WeaverAgent owns the analysis-state contract under R20-5. WeaverAnalysis mirrors it.
Sibling implementation and landing remain owed. [SCOPE-1, October 18 A1/A2]

The caller elects a compaction reservation under R11. Spec 06 enforces its
unchanged-prefix boundary. Spec 08 only selects complete units. Spec 06 may
conservatively choose N before the reservation, in which case unchanged reserved text is
recomputed before the replacement summary. No reserved text is summarized away. The
owning placement and selection interfaces must preserve that relationship. [STRAT-1,
PLACE-5, October 18 R11]

## 7. SPU PRD sections 13.4 and 13.9, and harness place

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

The family remains the renderer and tokenizer. The harness lays canonical messages and
source references, and asks the SPU for the exact size of a proposal without altering
the resident argument. The SPU supplies actual prefix, capacity and effective generation
limits at open. A measurement changes no resident token, sampler or backend state, and
does not authorize a cut. The harness grants cuts only between external turns, with the
existing Flush and Elide operations. No tokenizer, device pointer or saved KV entry
enters the harness or store. [PLACE-1, PLACE-3, PLACE-5, PLACE-6, DEFER-1, October 18
R7]

This adds the read-only exchange to the decode charter and its seam enumeration. It
preserves the permanent identity prefix and physical overflow guarantees. The harness's
deterministic assembly and part-order duties remain, with selected state content
replacing the old trace-kind-only read filter. Measurements and source maps reach the
trace and are excluded from state admission. [FLOW-3, TAP-3, PLACE-1, PLACE-4, PLACE-5]

## 8. Admin, harness, state and trace plan duties

These additions apply at the [foundation cutover][cutover]. Until then, the named
previous serving clauses remain applicable.

**PLAN-CHARTERS.** Amend admin PRD sections 3, 4.1 and 8 to carry the root operator's
fixed plan update verb. It carries the protected update into the worker and reports its
outcome. It does not edit the live store, accept a caller path or grant plan updates to
WeaverWeb. Section 3's turn-content exclusion retains its ordinary-input boundary, with
only this fixed plan-update duty admitted here. Amend harness PRD sections 2 and 4 for
the review-request port, locked-plan predicates, source-backed state changes and
operator update through the one feed. The caller chooses and invokes the review through
the existing gate. The primitive checks held setup, work and pair identity before
closure. It contains no reviewing organ or workflow. [R20-12] [PLAN-2, PLAN-4, PLAN-5,
PLAN-6, October 18 R5, October 18 R6]

Amend state PRD sections 2 through 5 for stable step identity, source-backed plan rows
and atomic label/mark writes behind its own contract. The store holds the review account
but never judges the work. Amend trace PRD section 3.1 for operator plan input, the
loop's new effect/activation/outcome events and unselected plan measurements. These are
new events in occurrence order. No earlier event is edited, and the tee still applies
only the validated partition to a copy. [PLAN-1, PLAN-2, PLAN-3, PLAN-6, MARK-5, FLOW-2,
FLOW-8]

Spec 07 and the owning ADMIN-PLAN, HARNESS-PLAN, STATE-PLAN-SCHEMA, STATE-PLAN-FILE,
STATE-PLAN-READ, STATE-PLAN-FLAGS and TRACE-REGISTRY clauses specify these duties. Their
state tables, exact field tests and guarded transaction perturbations are the
instruments. The separate prompt still teaches PLAN-1 through PLAN-5 under PROMPT-3 and
is the document owed under Spec 02 O02-5. [PLAN-1 through PLAN-6, PROMPT-3]

## 9. Serving criteria and read content

These additions apply at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**SORT-CHARTERS.** Amend harness Spec sections 5 and 6 for caller-supplied typed
selection criteria under Spec 08. No strategy declaration, trigger, pass ordering or
basic-inference-loop replacement is added by this set. Those choices belong to the loop
under R20-12. The primitive applies exactly the criterion in its request and records
that decision. [SORT-1, SORT-3, R20-12]

In harness PRD sections 2 and 4, sort has read and measure authority only. It receives
whole attributed units and returns a choice. The loop records and applies that choice,
place preserves it, and the SPU alone renders and changes its buffer. A held tool record
can be placed as history without granting execution. No loop caller can supply a raw
role or use a historical record to mint a fresh tool-result grant. [SORT-1 through
SORT-4, SERVE-1, PLACE-1, FLOW-8]

State PRD's read duty includes exact original binding identities and whole pairs.
Automatic retrieval selects whole user/model blocks by noun, and plan reads address
individual held statements without renumbering their bindings. Trace PRD section 3.1
holds the closed SortDecisionRecord and the excluded state.sort.measurement vocabulary.
The owned interfaces and instruments are STATE-SUBJECTS, STATE-STATEMENT,
HARNESS-MATERIAL, HARNESS-SORT and TRACE-SORT. [READ-1, READ-2, READ-4, PLAN-3, STORE-3,
PAIR-4, TAP-3]

STATE-MODE explicitly reverses the 2026-10-02 no-vector-flag ruling for the state member
only, as named in the state amendment. [SCOPE-1, R8]

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
