# State Management Spec 08 - Sort and Selection Primitives

**Status:** MERGED.

**Landing PR:** #182

**Version:** 0.11 **Date filed:** 2026-10-20 **Document ID:** `state-management-spec-08`
**Parent:** state-management-primitives-PRD **Editorial:** Per the Working Rules.

## 0. Scope and applicability

**SCOPE.** This Spec defines selection over complete attributed material and the
outbound identity sort. It serves SORT-1 through SORT-4, SERVE-1 through SERVE-3, OUT-1
and OUT-2, and the selection mechanics needed by READ-2 through READ-5. Sort reads,
labels, joins and selects. It has no store-write or argument-mutation handle. The caller
chooses when to query, which candidates are mandatory, which criterion to use and when
to place the result. R20-12 moves the four strategies, wall trigger, step progression
and pass order to local loop scratch work. They are not part of this deliverable.
[CAP-1, SORT-1, R20-12, R20-13]

The cut-and-reload protocol formerly mixed into WALL-STATES stays in Spec 06 under
R20-15. Only its trigger, summary pass and strategy schedule leave this Spec. Selection
returns material for that primitive without choosing when it runs. [MARK-4, PLACE-5,
R20-12, R20-15]

The baseline is local main at 2eb5c5a2. The implementation is introduced in
weaver-harness/src/flow/sort.rs, using floor and trace types and the contracted store
and SPU ports. No caller links the store or SPU implementation. Every replacement
applies at the [foundation cutover][transition]. Until then the previous serving
contract applies. The loop stays in the worker's dev_loop folder. [SEAM-1 through
SEAM-3, FOUNDATION-SCOPE]

## 1. Complete units and caller contract

**METADATA-BOUNDARY.** The observations result is typed query data, not a ReadEntry or a
SelectedUnit. No selection or placement constructor implicitly presents it. A caller
that explicitly requests this metadata owns its presentation, without reclassifying the
store's bookkeeping or changing a content query. [SEAM-3, SORT-1, R20-19]

**UNIT.** SelectedUnit is {unit_id:UnitId,sources:Vec<MaterialRef>,
source_map:Vec<SourceMapEntry>,messages:Vec<Message>,spans:Vec<SpanRef>,
step_queries:Vec<StepQueryOccurrence>}, the Spec 06 input. StepQueryOccurrence is
{reference:StepQueryRef,message:DecimalU64}. message is the unit-local index containing
that query's instruction only. Ordinary units have an empty vector. SourceMapEntry is
Spec 06's MESSAGE-MAP record. UnitId is externally tagged block {source:ContentRef},
statement {statement:StatementRef}, pair {call_id:CallId}, result {call_id:CallId}, span
{span:SpanRef}, or instruction {source:ContentRef}. MaterialRef is externally tagged
text {source:ContentRef, span:Span}, request {call_id:CallId}, or returned
{call_id:CallId}. Span is the foundation's decoded UTF-8 byte interval. sources follows
first appearance in the unit, with no repeated identical reference. It describes
coverage, not another write or an executable grant. [STORE-3, READ-1, READ-5, PAIR-4,
PLACE-1]

The private HeldMaterial adapter constructs these values from a fully validated read or
a fresh receipt-backed source. Selection callers cannot supply raw Message values or
source IDs. bindings on reads use ReadBinding {ordinal,value}. The ordinal remains its
position in the filed block, so a filtered binding at read index zero cannot silently
become StatementRef binding zero. Use the following complete unit rules. Reason:
selection needs a stable unit and attributed bytes, while a requested statement may be
one statement inside a multi-statement block. [STORE-3, PLAN-1, PLAN-3, FLOW-8, SERVE-1]

For each selected Text interval, source_map names its exact unit-local Message and
block, with start zero and end equal to the selected byte length. MaterialRef keeps the
absolute stored interval. Whole-span repetitions retain their separate map entries. A
pair request maps to its whole ToolCall block, or to the request Text interval for
inward recall. Its returned reference maps to the whole ToolResult block. A result unit
maps only its returned reference. No map entry names a schema wrapper or an assistant
answer that has not yet occurred. The adapter validates these locations before returning
a grant. [STORE-3, PAIR-4, MARK-6, PLACE-4]

| Unit | Required material | Canonical messages and coverage |
| --- | --- | --- |
| block | One subjects or conversation entry with every binding in the block | Complete Text, then every attached complete pair in opening order. One full text interval plus pair references |
| statement | One history, condition or statement entry and its original binding ordinal | Exactly value.statement_span, then that binding's complete pairs. One text interval plus pair references |
| pair | A terminal ReadPair and the statement query for its StatementRef | Exactly that request statement, the pair's request block where present, and its whole terminal content |
| result | A fresh terminal receipt, the private tool-result grant and a kept request occurrence | The existing whole tool-result Message only. returned identifies its pair. The kept request is a placement precondition |
| span | One successful closed or retired span read, never a preview | All selected intervals and their complete pairs in returned order. The requested SpanRef is the unit's one span |
| instruction | A filed certified loop.instruction and its template grant | Its complete one-Text Message and full text interval. No arbitrary replacement text |

A span entry with step_query is a typed previous-step query result under Spec 06. Its
canonical message contains only the referenced instruction Text. The unit's step_queries
retains StepQueryRef and the source map attributes that exact instruction. It cannot
expand the prior step's work. Repeated source intervals keep distinct locations.
[MARK-3, R20-10]

Text reconstruction creates one Message with one Text block per selected interval.
message.assistant has role assistant. message.user, loop.instruction, plan.event,
step.outcome, sort.decision and state.mark have role user. These are source-kind rules,
not caller-supplied roles. A seed interval is already in the permanent head and cannot
be appended as a unit. Reading it remains legal, including as data inside an explicit
recall return. An attempt to append the system span refuses the operation as
invalid_span. No second system prompt is generated. [PROMPT-1, PLACE-1, PERSIST-6,
FLOW-8]

For each non-result unit, expand pair attachments immediately after their owning Text
interval, once per CallId within the unit. External requests add an assistant Message
containing one ToolCall with the exact held name and arguments. Review requests add the
same record with name sh and the exact held command. Inward recall adds no ToolCall,
since its already selected STE100 statement is the complete request. Add the terminal as
one role tool_result Message containing one whole ToolResultBlock. sources includes
request and returned for every such pair, even for a recall request represented by its
Text alone. [PAIR-3, PAIR-4, READ-4]

The terminal's non-null ToolOutcome uses the same pure formatting function as
tools.rs::ToolResult::granted. Factor that formatting into a private helper shared by
the completion and held-material adapters. The held path constructs only the floor's
ToolResultBlock record, never the harness's ToolResult capability. If a terminal has
outcome null, use the complete canonical PairEnded JSON as its record content. That
makes abandoned or never-dispatched communications visible without inventing a tool
answer. Past requests cannot dispatch from this adapter. Family rendering remains the
SPU's and may refuse an unsupported tool-bearing Message. [PAIR-1, PAIR-3, STORE-3,
FLOW-8, PLACE-3]

A result unit uses the exact grant produced by the current completion, including its
existing not-dispatched notice. It requires the request and associated statement to
remain in the kept argument. It is never used after a cut removed that request. In that
case select the complete pair unit instead, with its held statement. No cut occurs while
the communication remains open. [PAIR-1, PAIR-4, MARK-4, PLACE-6]

An open pair, missing reference, partial binding interval or mismatched source cannot
become a unit. A correct read that includes an open pair yields incomplete_candidates. A
malformed reply is the foundation's protocol failure. The adapter must distinguish those
outcomes. It never omits one attachment to fit. A whole span is atomic for selection
even though its canonical form contains many messages. Partial overlap with an already
kept span does not permit slicing the candidate. [READ-5, PAIR-4, CLASS-8, SERVE-2]

**SORT-PORT.** select_argument accepts SelectionRequest and returns Selection or
SelectionRefused. The public request contains these fields. Private source and prefix
constructors stay with the read and placement adapters. Reason: the caller supplies
purpose, while sort can enforce attribution and fit without knowing the workflow.
[SERVE-1, SERVE-2, SORT-3, PLACE-3, R20-12]

| Field | Type and meaning |
| --- | --- |
| assembly_id | DecimalU64 from place, identifying this attempt |
| mandatory | Vec<SelectedUnit>, in the exact caller-required order |
| candidates | Vec<SelectionCandidate>, optional complete units |
| optional_at | usize insertion boundary in mandatory, from zero through mandatory.len() |
| prefix | PreparedPrefix from Spec 06, unchanged coverage, rebuild and SPU facts |
| criterion | SortCriterion, defined below |
| tool_schemas | Vec<String>, exact granted schemas in supplied order |
| reads | Vec<SelectionRead>, candidate-producing read provenance |

SelectionCandidate is {unit:SelectedUnit,read_ordinal:DecimalU64,
entry_ordinal:DecimalU64,repeat:bool}. Ordinals address reads and its returned entries,
zero-based. The private read adapter supplies them and retains the binding conditions
needed by the criterion. A complete span is one candidate. A caller may request a
repeat, but cannot change its attributed bytes or rank. A missing ordinal, mismatched
source or wrong snapshot refuses incomplete_candidates. Material without a read rank may
be mandatory only. [STORE-3, READ-5, SERVE-2]

SortCriterion is {order:CandidateOrder,injection_share:Rational,
external_drop:ExternalDrop}. CandidateOrder is externally tagged provided:{},
recency:{}, or current_step_then_recency:{condition:String}. The condition must be
nonempty and matches a held binding's condition by exact bytes. Rational is
{numerator:u64,denominator:u64}, with positive denominator and numerator no larger than
denominator. ExternalDrop is refuse or whole_over_limit. There is no strategy field,
loaded serving table or default criterion. These are per-call parameters. Reason: a
criterion controls this selection, not the loop's next act. [SORT-3, READ-2, PLACE-4,
R20-12]

recency and current_step_then_recency require all optional candidates to come from one
ordered read snapshot. Newer means larger entry_ordinal in that result. They refuse
invalid_criterion across different reads, rather than comparing run IDs or unrelated
snapshot ordinals as time. provided preserves caller order and can join several
completed reads. Selection keeps the stated whole-unit guards in either case. [STORE-5,
READ-1, SORT-1]

Selection is {assembly_id,prefix:PreparedPrefix,units:Vec<SelectedUnit>,
selected_refs:Vec<UnitId>,dropped_refs:Vec<SelectionDrop>,criterion:SortCriterion,
measured:DeltaMeasured,measurement:SortMeasurement}. SelectionRefused is
{assembly_id,case:SelectionCase,refs:Vec<UnitId>,placement_code:Option<PlacementCode>,
detail:String,measurement:SortMeasurement}. PlacementCode is Spec 06's closed
PlaceRefused.code set. Use null when no placement fault occurred. selected_refs equals
units.map(unit_id). A refusal has no successful selection, even if some probes fitted.
Sort returns the unchanged prefix grant. The caller passes the units and exact schemas
to place if it chooses to use this result. [SERVE-2, PLACE-4, PLACE-5]

The caller may assume source attribution and whole-unit fit for the measured prefix
revision. It must refuse stale prefix grants, mutate no selected bytes, and record the
decision before use. Sort must refuse invalid references, incomplete pairs, unsupported
criteria and over-capacity mandatory sets. It cannot silently supply a missing task,
instruction or flag. Those are the caller's chosen inputs. [PLACE-4, STORE-3, FLOW-8,
R20-12]

## 2. Deterministic selection mechanic

**SUBJECTS.** noun_keys(bindings) returns unique noun bytes sorted by UTF-8 order. It
accepts the checked Binding list from Spec 02, reads every noun and performs no model
call. An empty list returns an empty key list. The caller may pass a nonempty result to
STATE-READ's subjects selector. STATE-SUBJECTS owns that query, including its
complete-block and complete-pair rules. No query is started automatically by this
function. [READ-1, READ-2, STORE-4, R20-12]

**STATEMENT-READ.** The statement adapter takes one StatementRef, uses the foundation's
statement selector and returns the exact binding interval and its complete pairs as a
statement unit. Condition reads can produce the same units for forward flags. Preserve
the stored binding ordinal even after filtering. Missing or changed identity refuses
before selection. No adapter activates a step or places its instructions. [PLAN-3,
READ-1, STORE-3, R20-12]

**COVERAGE.** A unit is already covered only if all its MaterialRefs are covered by the
kept prefix or earlier mandatory units. Text coverage requires the same ContentRef and
containment of its byte interval. Pair request and returned coverage are separate exact
CallIds. Equal bytes from different sources are never equal coverage. All mandatory
units use this test before appending. Optional history uses it against the resulting
mandatory set. A candidate with repeat=true retains its new occurrence by explicit
caller request. Partial overlap retains the complete candidate, subject to fit, rather
than slicing a span or a pair. [STORE-3, READ-5, PAIR-4, MARK-6, SERVE-2]

**SELECT-ORDER.** This is the algorithm within one selection call. It prescribes no
order between calls or other primitives. [SORT-1, SORT-3, PLACE-3, R20-12]

1. Validate criterion, ordinals, optional_at and the exact prefix revision. A missing
   boundary is boundary_unavailable with invalid_cut. Staleness keeps
   Placement.stale_position. No measure or mutation has occurred.
2. Deduplicate wholly covered mandatory units against the kept prefix and prior
   mandatory coverage, preserving supplied order. Retain partial overlaps whole. Map
   optional_at through removed covered units to the corresponding remaining boundary.
   Measure the unchanged rebuild and this mandatory proposal with its exact schemas
   through place.measure.
3. If the proposal does not fit, refuse mandatory_over_capacity with the measured
   capacity code. The explicit whole_over_limit alternative applies only when the
   original request has exactly one mandatory unit, that unit is the current external
   input and removing it makes the unchanged proposal fit. In that case record the whole
   unit as whole_over_limit and return an operation refusal, not an empty successful
   answer. No pair, flag or other unit can use this case.
4. Compute B = floor(C_tail * numerator / denominator), using u128 arithmetic. Drop
   fully covered optional candidates unless repeat=true. For provided, preserve order.
   For recency, use descending entry_ordinal. For current-step ordering, put exact
   condition matches first, then descending entry_ordinal. Break ties by UnitId JSON
   bytes under TYPES-CANONICAL in the types amendment, in ascending byte order with the
   variant tag compared first, then by original candidate index.
5. Try each remaining whole candidate once. Insert accepted candidates at optional_at in
   the mandatory proposal. Measure the complete proposal with the unchanged rebuild and
   schemas. Accept only when it fits tail and generation room and max(0, measured_tokens
   - mandatory_tokens) <= B. Otherwise record the limiting reason and continue without
   partial inclusion.
6. For a recency order, restore accepted candidates to their source entry order within
   the insertion slot. For provided, keep their supplied order. Measure that final
   proposal. If it fails either fit or B, remove the most recently accepted optional
   unit and repeat. This phase makes at most accepted_count+1 measurements and never
   removes mandatory material.
7. Return the immutable selection, decision metadata and measurement, or the typed
   refusal. The harness records them before granting use. This operation neither places
   the result nor starts another pass.

**DROP.** A selected tool return and a marked span are indivisible. Every omitted
candidate has a typed reason and source identity in sort.decision. The explicit
whole_over_limit branch returns Placement.tail_over_capacity or generation_room with the
measured cause, and the caller gives a loud operation refusal. Its admitted filing
remains owed. Place has no drop path. [PLACE-4, PAIR-4, SORT-3]

**SORT-RECORD.** sort.decision carries SortDecisionRecord {template_id:String,
message:Message,decision:SortDecision}. Its Message is role user with exactly one Text
from the certified inventory. The two templates are sort.serve and sort.outbound. Their
only variable slot is a technical_name derived from the closed order variant: provided,
recency or current_step_then_recency for serve, identity for outbound. These four names
must be present in the qualified dictionary. All source and assembly identities stay in
metadata. No candidate text enters a slot. SortDecision is externally tagged with these
shapes: [SORT-4, FLOW-8, CLASS-2]

| Variant | Fields |
| --- | --- |
| serve | assembly_id:DecimalU64, criterion:SortCriterion, selected_refs:Vec<UnitId>, dropped_refs:Vec<SelectionDrop>, reads:Vec<SelectionRead>, refusal:Option<SelectionCase> |
| outbound | source:SourcePosition, criterion:OutboundCriterion, acts:Vec<SortAct> |

SelectionRead is {selector:ReadSelector,snapshot_id:DecimalU64}, in completed-read
order. It identifies the immutable results used, even after their cursors release.
SelectionDrop is {unit:UnitId,reason:DropReason}. DropReason is already_covered,
injection_budget, tail_over_capacity, generation_room, invalid_candidate,
boundary_unavailable, not_evaluated or whole_over_limit. SelectionCase is
incomplete_candidates, invalid_criterion, mandatory_over_capacity or
boundary_unavailable. selected_refs follows placement order. dropped_refs follows
candidate evaluation order, with wholly covered mandatory units first. No reason permits
truncating a return. [SERVE-2, READ-2, PAIR-4, COST-4]

SortAct is externally tagged tool {call_id:CallId,statement:StatementRef}, recall
{call_id:CallId,statement:StatementRef}, or answer {source:SourceBlockId}. Tool and
recall IDs use Spec 05's original-emission ordinal rules before their pair start. Answer
references name each accepted Text block of a final answer in source order. The caller
chooses where the returned acts go. No sort record opens a pair. [OUT-1, OUT-2, PAIR-1,
STRAT-2]

state.sort.measurement carries SortMeasurement {assembly_id:DecimalU64,
candidate_count:DecimalU64,selected_count:DecimalU64,mandatory_tokens:Option<u64>,
optional_marginal_tokens:Option<u64>,budget:u64,fit_attempts:DecimalU64,
wait_ns:DecimalU64,refusal:Option<SelectionCase>}. It is channel internal, origin loop
and structurally excluded as measurement. Missing counts from an unperformed SPU measure
are null, never guessed zero. fit_attempts counts completed MeasureDelta answers.
wait_ns covers the complete selection attempt on the shared monotonic clock. The counter
fields count units, including mandatory candidates, with covered units counted as
candidates but not appended selections. [COST-2, COST-4, TAP-3]

The port returns the decision and its measurement to the harness. The harness records
both before releasing a successful selection for placement. A failed selection records
both with refusal and no selected units. A successful selection's metadata must agree
exactly with the immutable result handed to place. The trace owns serialize-only mirrors
of these shapes. DIAGNOSTIC-CONVERT in the diagnostic amendment rejects them under the
amended harness-diagnostic contract sections 3 and 7. [SORT-4, FLOW-3, FLOW-8, SCOPE-1]

## 3. Outbound identity and operation state

**OUTBOUND.** OutboundCriterion is the unit variant identity.
identity_sort(source:SourcePosition,acts:Vec<SortAct>) returns the same complete ordered
acts and its outbound decision record. Its private input adapter accepts only checked
statement and request identities from Spec 02. Unknown, duplicated or foreign-source
identities refuse invalid_candidate. This first implementation performs no ranking,
authorization or dispatch. More criteria require an explicit Spec addition. Reason: a
sort selects acts, while execution remains with the caller and the relevant permission
boundary. [OUT-1, OUT-2, SORT-1, SORT-4]

**SELECTION-STATES.** One selection owns its immutable prefix and candidate snapshot
until its result or refusal. The port does not hold an open store cursor while
measuring. Read pages were collected and released before entry. [SEAM-3, PLACE-5,
SORT-1]

| State | Call or result | Next state and obligation |
| --- | --- | --- |
| Idle | select(valid request) | MeasuringMandatory, hold exact input |
| Idle | invalid request | Refused, return typed cause without a probe |
| MeasuringMandatory | fits | TestingCandidates, retain mandatory count |
| MeasuringMandatory | does not fit | Refused, return whole mandatory account |
| TestingCandidates | complete measure | Accept or drop whole candidate, continue finitely |
| TestingCandidates | candidates exhausted | FinalMeasure, restore prescribed result order |
| FinalMeasure | fits | ResultHeld, return immutable selection for trace recording |
| FinalMeasure | fails with optional units | Remove last accepted optional, FinalMeasure |
| Any measuring state | typed rendering refusal | Refused, preserve exact PlacementCode |
| Any measuring state | component failure | Foundation failure, no selection grant |
| Any occupied state | second select | Busy(request), return ownership unchanged |
| ResultHeld | decision and measurement recorded | Idle, release selection for caller use |
| Refused | refusal records complete | Idle, return operation refusal |
| Any occupied state | caller cancellation | Record completed probes, abandon proposal without mutation |

The selection pass certifies exact source coverage, declared ordering and measured fit
of the returned proposal. It does not certify relevance, truth, plan success or the
eventual decision's use of a fact. Evidence is the read identity, source map, criterion
and complete SPU measures. [FALS-4, SORT-4, PLACE-5]

## 4. Owning amendments and enforcement

**AMENDMENTS.** Types Spec sections 2 and 4 gain the request, criterion and result
shapes under TYPES-SERVING. Harness Spec sections 5, 6 and 8 gain HARNESS-SORT, which
implements this port and its trace-before-use guard. Trace Spec section 3 owns
TRACE-SORT and the serialize-only record mirrors. Internal Spec sections 1, 2 and 5
retain recall as a separate query consumer. No basic-inference-loop replacement or
deployment strategy table is part of this act. Foundation section 1 owns retirement of
old recall readers and deploy instructions. [SORT-1, SORT-4, READ-3, R20-12]

| Claim | Instrument and evidence | Protective change that must fail |
| --- | --- | --- |
| UNIT | Real read shapes, every unit family and full request/return attachment | Permit raw Message construction, lose StepQueryOccurrence, import prior-step work, partial attachment or preview-as-closed span |
| SORT-PORT | Caller fixture with exact fields, immutable prefix and strict codec | Accept unrelated ordinals, mutate prefix or invent a mandatory input |
| SUBJECTS and STATEMENT-READ | Compound nouns, filtered original ordinals and exact query results | Use a filtered-array index as StatementRef or hide a query refusal |
| COVERAGE | Distinct equal text, partial overlap and explicit repeat cases | Deduplicate by text, slice a span or omit a pair return |
| SELECT-ORDER | Literal algorithm cases using scripted whole-proposal measures and TYPES-CANONICAL UnitId tie fixtures | Sum isolated token counts, drop mandatory material, change canonical tie order or ignore final-order cost |
| DROP | Oversized external singleton, paired input and multi-mandatory cases | Treat omission as success or allow the exception for a pair |
| SELECTION-STATES | Every state row and cancellation at each probe | Grant before records, accept a second request or mutate the argument |
| OUTBOUND | Exact ordered acts and invalid identity corpus | Dispatch from sort or change source identities |
| Ownership | Compile/API check of sort's dependencies and private constructors | Give sort a filing or decoder-mutation handle |

The runtime tests use the real read/result adapters and paired SPU transport fixtures. A
table-only mock cannot prove the wire or source map. Qualification against an actual
family measures the same full proposal in selection and place. These are acceptance
obligations, not a claim that this document pass ran them. [SORT-1, PLACE-5, COST-2,
COST-4]

## 5. Open measured values and deferred ports

The caller's injection share, C_tail and measurement references remain qualified under
the declared tuple. This Spec chooses no value or strategy. Measure whole request
memory, candidate count, SPU probe cost and selection misses. Thresholds belong to the
evaluation record, not the selector. [READ-2, FALS-4, COST-2, COST-4]

Prefetch and semantic ranking may supply candidates through the same immutable read
adapter. They receive no filing or argument-writing capability. Their conditional work
remains above the release line. Both Flush and Elide remain available through Spec 06,
with their choice made by the eventual loop. [DEFER-3, DEFER-5, R20-3, R20-9, R20-12]

[transition]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
