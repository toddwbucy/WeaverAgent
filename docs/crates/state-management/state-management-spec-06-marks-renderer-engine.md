# State Management Spec 06 - Marks, Place, and the Decode Boundary

**Status:** MERGED.

**Landing PR:** #232

**Version:** 0.12 **Date filed:** 2026-10-21 **Document ID:** `state-management-spec-06`
**Parent:** state-management-primitives-PRD **Editorial:** Per the Working Rules.

## 0. What this document is

**SCOPE.** This Spec lifts MARK-1 through MARK-6 and PLACE-1 through PLACE-6. It extends
the foundation's span tables and the existing decode seam. It consumes Spec 05's whole
pairs and supplies Spec 07's step boundaries and Spec 08's placement port. The
foundation's [transition][transition] applies to every amendment here. Until cutover the
previous serving contracts apply. Reason: marks, canonical messages and resident token
positions are different representations of the same placed work. [MARK-6, PLACE-5,
FLOW-7]

The baseline is main 2eb5c5a2. The decoder already holds a hot resident sequence and
implements Flush and Elide. Its family library renders and tokenizes. The harness has
neither its tokenizer nor its buffer. Saved-buffer restoration remains deferred. This
revision removes the invented decode call and harness tokenizer from v0.1. It does not
change the framing ruling in weaver-spu-PRD section 13.4. The PRD's harness renderer is
message-level place, with token facts supplied by the SPU. [PLACE-1, PLACE-5, PLACE-6,
DEFER-1, October 18 R7]

The Open evidence [proposal and acceptance record][open-election] names this carrier
act's choices and the implementation work still owed.

## 1. Marks and their creation

**IDENTITY.** Use SpanRef {seed_id, local_id, revision} from the foundation. Counters
are DecimalU64 on the wire and eight-byte big-endian BLOBs in SQLite. The system span is
local_id 0, revision 1. Each newly created span uses the next unused local ID and
revision 1. The first release never rewrites a revision or reuses its ID. A repeated
step execution gets a new span, while its StepId remains stable. The revision field is
retained in the shared address and catalog, with no first-release operation that
increments it. A fork retains inherited addresses, interpreted in the loaded branch.
Reason: changing execution content cannot change what an old address returns. [MARK-1,
MARK-6, PLAN-1, PERSIST-7]

**MEMBER.** SpanText is {source:ContentRef,start:DecimalU64,end:DecimalU64,
step_query:Option<StepQueryRef>}. Ordinary and seed members use null. StepQueryRef is
defined by STEP-QUERY below. ContentRef is externally tagged seed:{seed_id,ordinal} or
event:{block_id}. Offsets address exact decoded statement bytes and must be UTF-8
boundaries. A span read returns each original block with that selected interval, not
rewritten stored text. It joins whole ReadPairs through each selected request binding,
as of the span's end source. For a preview, the frontier is its through source instead.
Ignore pairs opened after that frontier. An interval that intersects only part of a
paired request refuses partial_pair. Each included pair must be terminal by the frontier
and is returned whole once per request occurrence. No separate pair-member list can
disagree with that index. A later pair or return cannot change an ended span. Reason:
STORE-3 makes the request the address of its return, and the mark fixes an immutable
retrieval boundary. [MARK-3, STORE-3, PAIR-3, PAIR-4]

**SPAN-ORDER.** An execution span has a declared prefix of held instructions, then work
that was filed during its scope. That prefix may reference older content, because a
plan's instructions are filed before their execution. It copies no content into a second
holding. The store records the prefix references at open. The ordinary work kinds are
message.user, message.assistant and loop.instruction. Only admitted, correctly filed
statement blocks enter this set. Plan, mark and sort bookkeeping are outside it. A
step.outcome enters through its outcome span below, not a second ordinary-work entry.
[MARK-1, MARK-3, STORE-5, PLAN-1]

On close, the store constructs the canonical membership as follows. Include this span's
prefix, each descendant span's prefix and every ordinary-work block filed strictly after
this span's open event and before its close event. A prefix's order anchor is its span's
opened_content_id. An ordinary block's anchor is its own content ID. Sort by anchor,
then span local ID and prefix ordinal for prefixes. An ordinary block cannot share its
anchor with a mark/control event. References repeated by distinct declared prefixes
remain distinct occurrences. Arbitrary extra repetitions or omissions cannot be supplied
by a caller, because close takes no member list. Reason: an old instruction can be
placed at a later activation without pretending that its source was filed again.
[MARK-3, MARK-5, MARK-6]

Runtime open scopes form one nested path beneath the current task. A sibling may open
only after the earlier sibling closes or retires. A parent cannot close or retire while
a child remains open. Those rules make each ordinary-work interval unambiguous. A
descendant prefix is included only where its opening anchor falls at or after the
enclosing open and at or before its end. Inclusive prefix anchors include a task prompt
created with its task and an outcome created at step closure. The ordinary-work bounds
remain strict. No interval includes work from a later task. The canonical members are
materialized once at close or retirement, in the same transaction as the source
statement. [MARK-1, MARK-5, STORE-5]

**STEP-QUERY.** A step prefix is the typed result of the caller's query about the
previous step. StepQueryRef is {plan:PlanRef,previous_step:Option<StepId>,
step_id:StepId,instruction:StatementRef}. The step_successor selector returns
{reference:StepQueryRef,instruction:ReadEntry}. It returns only the next held
instruction, not the previous step's work. The result is the first waiting step in that
revision, with no active step or earlier unclosed step. previous_step is null exactly
when the first waiting step has ordinal 1. Otherwise it names the closed immediate
predecessor by ordinal. A preserved closed step later in the revision does not prevent
this read. STATE-STEP-QUERY owns these guards and their replacement-plan fixtures. The
result names the same loaded seed and exact held statement. [MARK-3, PLAN-1, PLAN-4,
R20-10]

Activation takes this checked result as its prefix. The store checks it again against
the plan and records step_query beside that prefix and its finalized membership. A task
read that also includes the earlier original instruction therefore distinguishes the
ordinary occurrence from this query result. A span read preserves both occurrences with
their different context, never substitutes a dump of prior work. ReadEntry.step_query
carries this reference or null. Reason: repeating an instruction does not repeat its
filing or mislabel its provenance. [MARK-3, MARK-6, STORE-3, R20-10]

**BOUNDARIES.** Boundary operations are requested at typed ports. The seed creates and
ends the system span. begin_task creates a task and its ended prompt child. Plan
activation can open a subtask and step with the held instruction prefix. A valid close
creates an ended outcome span and closes the reviewed step and complete ancestors. These
are atomic effects, not a schedule for invoking them. Model prose alone changes none of
these states. TaskScopeRef is {task:SpanRef,prompt:SpanRef}. A replacement retires the
old path before creating its replacement. The caller supplies the already-filed task
input and requests the operation. [MARK-1, PLAN-2, PLAN-6, PLACE-2, R20-12]

**MARK-FILE.** StatementFile.effect is null or an externally tagged StatementEffect. The
mark arm is mark:MarkEffect. MarkEffect has exactly the variants below. Every referenced
SpanRef must name the loaded seed. A new span must use the next local ID, revision 1.
Multiple opens in one transaction allocate in their execution order. The store derives
open/end sources from the filing's source identity. [MARK-5, SEAM-2, FLOW-1]

| MarkEffect variant | Required body and meaning |
| --- | --- |
| begin_task | {task:SpanRef,prompt:SpanRef,source:ContentRef}, create the root and its closed prompt child from one complete already-filed input Text |
| open | {span:SpanRef,parent:SpanRef,kind,prefix:Vec<SpanText>}, kind subtask, step or step_outcome |
| close | {span:SpanRef}, materialize the canonical members and set closed |
| retire | {span:SpanRef,replacement:SpanRef-or-null}, materialize the current members and set retired without claiming review passed |

Only begin_task is licensed directly on state.mark in the first release. It requires no
open task and a source kind message.user. The remaining operations are internal parts of
the typed plan effects in Spec 07. An operator plan update may begin a task from its own
filed task block. It retires the old open path first. Model input has no effect
constructor. A retired span keeps its administrative end and contents, but never
supplies a reviewed-step label. [PLAN-2, PLAN-6, MARK-5]

state.mark is channel internal, origin loop, with {message,context,template_id}. Message
has role user, one Text block and context is the MarkEffect. Its partition rule names
message.content.text[0] or its wildcard and context="context". The tee copies that
sidecar as bytes, without interpreting it. The loop constructs StatementEffect only
after the load-certified statement passes. The store checks it against the copied
context. No message bytes are hidden in the sidecar, and no rejected sibling can be
stored through it. [TAP-1, CLASS-2, FLOW-8, MARK-5]

**CLOSE-DISPOSITION.** Before authoring an end, settle every candidate's disposition
through the frozen frontier. The store includes only filed work. A correctly rejected
block is absent and owes nothing. Every pair linked from a selected request must be
terminal and filed before a span closes or retires. A model-blocking request uses the
enclosing decision or cut bound. No unfiled member is named optimistically, and no
separate clock is started. [MARK-4, CLASS-8, FLOW-4, PAIR-4]

**MARK-STORAGE.** The store-owned initial catalog defines span, span_prefix and
span_member. span holds parent, kind, status, open/end source IDs and optional
replacement. Prefix and finalized member ordinals are separate domains. All source
references resolve to content_block IDs inside the transaction, never on the wire.
Closed and retired membership is immutable. The seed is closed with null event sources.
Runtime terminal spans require both sources. [MARK-1, MARK-3, STORE-5]

Refuse unknown parents, cycles, crossed siblings, noncanonical allocation, invalid
prefixes, partial paired requests, open paired requests or an edit of terminal
membership. The store checks shape, source custody and mechanical nesting, not the truth
of the work or the review report. Spec 07 checks the caller-supplied review evidence. A
refused filing is a broken contract and follows FLOW-4. Preflight failures before an
event is authored are operation refusals. [PLAN-2, SEAM-2, MARK-5]

**MARK-READ.** Add the ReadSelector tag marks {}. The QueryResult tag marks contains
{next_span_id:DecimalU64,current_task:TaskScopeRef-or-null,open_scopes:Vec<SpanInfo>}.
SpanInfo is {span,parent,kind,status,opened_source,ended_source,replacement}. Sources
are SourceBlockId or null. open_scopes is the root-to-leaf active path. current_task
names the latest task and its unique prompt child, including a terminal task until the
next begin_task. next_span_id is checked MAX(local_id)+1, never a reservation or write.
Counter exhaustion is an explicit read refusal. [MARK-1, MARK-6, SEAM-3]

The existing span selector returns Entries only for closed or retired spans. An open
span refuses span_open. For review, add span_preview {span,through}, where through is a
committed SourcePosition. Its result is externally tagged
span_preview:{span,through,entries}. It computes canonical membership through that
frontier without changing the mark. The span must be open and the source must be held at
or after its opening, with the recorded turn matching SourcePosition. Any included pair
must have been terminal by that frontier. A preview is not an end mark and cannot
authorize a cut or closed reminder. Both selectors use the existing whole-result paging
and byte limit. [MARK-3, MARK-4, PLAN-2, SEAM-3]

The mark pass certifies exact referenced content, nesting and a fixed end, not semantic
truth or task success. Its evidence is the source effects, filed frontier and committed
membership. The open/closed distinction and paired-request guards must each fail their
acceptance test when removed. [MARK-1, MARK-3, PLAN-2]

## 2. Canonical placement and SPU facts

**PLACE-PORT.** The harness's place module accepts PlaceRequest {assembly_id,
prefix:PreparedPrefix,selection:Vec<SelectedUnit>,tool_schemas:Vec<String>,
context:PlacementContext}. assembly_id is a checked run-local DecimalU64. selection is
the ordered array from Spec 08. Each unit has unit_id, sources, source_map, messages and
spans. Messages use weaver_traits::Message. sources contains Spec 08's MaterialRef
values, including exact text intervals and distinct pair request/return coverage. spans
contains SpanRef values. PreparedPrefix holds the validated cut, N and unchanged rebuild
under PREFIX-PREPARE. The head is the boot-held system span and is not supplied anew
with each request. A unit has no token IDs. Place preserves supplied order and records a
unit-to-message map. It makes no selection and starts no generation. [PLACE-1, PLACE-4,
SERVE-1, MARK-6]

PlacementContext is {label:String,instruction:Option<SourceBlockId>}. label is nonempty
caller metadata, not a policy enum. A non-null instruction must name a filed certified
loop instruction. It is recorded with the append and never rendered as model text. It
opens no turn and selects no strategy. [FLOW-3, FLOW-8, PLACE-1]

UnitMapEntry is
{unit_id,occurrence_id,message_start,message_end,spans,source_map,step_queries}. unit_id
has Spec 08's SelectedUnit ID type. message_start/message_end are DecimalU64 indices
into the canonical delta, end exclusive, with a nonempty interval. Entries follow unit
order, have disjoint increasing message intervals and cover the selected portion of the
delta after its rebuilt prefix. spans is that unit's ordered SpanRef list. step_queries
is Vec<PlacedStepQuery>, where PlacedStepQuery is
{reference:StepQueryRef,message_id:DecimalU64}. It preserves each typed query occurrence
at its actual instruction Message. source_map contains PlacedSource values from
MESSAGE-MAP. The SPU's internal schema-rendering adapter is outside this canonical
message map and is included in the returned token count. [PLACE-1, MARK-6, FLOW-3]

Placed is {assembly_id,delta,unit_map,message_ids,assistant_message_id,
recomputed_messages,N,prefix_tokens,resident_before,input_tokens,capacity}. message_ids
is Vec<DecimalU64>, one ID per canonical delta Message. assistant_message_id and
recomputed_messages are DecimalU64. PlaceRefused is {assembly_id, code, detail}, with no
delta. Codes are invalid_span, invalid_pair, invalid_cut, head_over_capacity,
tail_over_capacity, generation_room, and stale_position. Each occurrence ID is {run,
assembly_id, unit_index}. These UnitOccurrenceIds identify placements, not marks. A
CutReference is {span:SpanRef,occurrence_id:MarkOccurrenceId}. MarkOccurrenceId is
{run:String,ordinal:DecimalU64}. Reminders get new mark occurrences for the same stored
SpanRef. Invalid addresses refuse before any SPU mutation. Reason: identity and position
remain separately auditable, and a refused place cannot leak a partial argument.
[MARK-3, MARK-6, PLACE-3 through PLACE-5]

**PLACE-CALLS.** prepare_place(PlaceRequest) validates, measures and records the
complete proposed argument, then returns a private PlacementGrant containing Placed and
the exact schemas. It does not call AppendAndGenerate. The loop may submit that
single-use grant to the existing engine's append port. The engine checks the journal
revision and pending assembly before sending the same delta and schemas. A cancelled or
stale grant cannot start generation. [PLACE-1, PLACE-4, PLACE-5, R20-12]

The engine reports its matching complete Generation to place.observe_generation. Only
that callback installs prospective Message IDs, the assistant answer and resident facts.
A refusal/cancel callback retains any owed rebuild but installs no speculative
occurrence. There is at most one outstanding placement grant, and a second prepare
returns Busy with its request. Place's preparation pass certifies shape, fit and trace
attribution, not decoder completion. Its completion pass certifies the matching engine
facts, not the model's decision. [FLOW-3, MARK-6, PLACE-5, R20-12]

**OPEN-FACTS.** TokenAnswer::Opened requires the exact Open.session echo, the four
u64 facts {prefix_tokens,capacity,generation_limit,terminal_tokens}, and the two raw
JSON boxes request and measurement defined by SPU-OPEN in the
[SPU amendment][open-spu]. The prefix is the SPU family's rendering of the boot system
prompt and is made resident once. It is never appended again. Opened.prefix_tokens is
the floor of N for that session. The actual input token vector and the identity-prefix
byte partition are open
measurement evidence, with template identity only in request. No timing or generation
reading is invented. generation_limit is the resolved max-tokens-per-turn, including
the OperatorTunable value that the harness cannot infer from the declaration.
terminal_tokens is one under the merged single-token terminator contract. No alternate
framing is silently elected by this field. Reason: the SPU owns both the actual prefix
length and the effective generation limits. [PROMPT-1, PERSIST-6, PLACE-1, PLACE-5]

The harness binds the complete answer to its exact sent Open, sole decode reservation,
source port and existing recorded BootHead. It checks the session echo, closed boxes,
token-vector length and whole-render byte interval without rendering or tokenizing.
Family framing means rendered text need not equal the canonical system Text. The SPU's
instrument proves the actual render/prefill correspondence. The harness preserves both
boxes unchanged for PLACEMENT-RECORD, without a usable token-vector cache. The counts
remain typed journal facts only after that record is Recorded and the fit succeeds.
BootHead itself is already held from the earlier boot recall. [PROMPT-1, FLOW-3,
PLACE-1, PLACE-5, SEAM-2]

**MEASURE-DELTA.** Add the read-only TokenDirective::MeasureDelta {keep, delta,
tool_schemas} and TokenAnswer::DeltaMeasured {resident, prefix_tokens, keep,
input_tokens, capacity}. Counts are u64. delta is Vec<Message> and tool_schemas is
Vec<String>, the exact granted schemas in assembly order. MeasureDelta renders and
tokenizes the proposed delta with the same family path as AppendAndGenerate, including
schemas and generation opener. It runs no decoder, sampler or buffer mutation and
retains no result after answering. It clamps keep as Flush does. Malformed rendering
uses the existing MalformedDelta refusal. NotOpen and OutOfOrder retain their meanings.
It is valid at rest, including between tool rounds when keep equals resident. A smaller
keep is requested only between turns. Reason: fit must be known before a destructive cut
without a harness tokenizer. [PLACE-3 through PLACE-6, MARK-4]

AppendAndGenerate gains the same tool_schemas member. The existing canonical delta
continues to cross. The SPU appends one canonical user Message containing schemas joined
by a single LF after the supplied delta, when the list is nonempty. It passes that
complete sequence once through family::render_each, then appends the existing generation
opener. This is a rendering adapter, not another authored user input. The schemas are
the harness's loaded tool declarations, recorded in the trace before use. They are
machine metadata, not extra system prompts or model emissions to file. This moves the
already specified prompt-part order onto the concrete decode path. An empty tool grant
supplies an empty list. The harness does not serialize a family-specific schema wrapper.
[PLACE-1, PLACE-3, FLOW-3]

The MeasureDelta request is owned by the existing single-reader decode port and uses the
seam's current message segmentation. One request is outstanding. A measure answer closes
it, and a refusal leaves the session unchanged. No new socket, retained measurement
cache or buffer access is added. Between measure and append, the harness permits only
the planned suffix cut and administrative cancellation. Any other resident mutation
invalidates the measured result. [PLACE-5, PLACE-6]

**CAPACITY.** StateManagementConfig gains placement {c-head, c-tail, measurement-ref},
positive u64 token counts. c-head bounds the prefix at load. The qualified tuple must
satisfy c-head + c-tail + generation_limit + terminal_tokens <= capacity, with checked
arithmetic and the actual Opened facts. The current single-token terminator is an
existing seam fact, not an invented measured value. C_tail includes every delta token,
tool schemas, framing and generation openers, and all generated work already resident
outside the prefix. No separate uncounted schema allowance exists. A different
terminator convention requires an explicit amendment to the session contract before
admission. Reason: the universal tail limit cannot spend the same memory twice.
[PLACE-3, PLACE-4, COST-2]

At place, require prefix_tokens <= c-head, and keep - prefix_tokens + input_tokens <=
c-tail. Also check keep + input_tokens + generation_limit + terminal_tokens <= capacity.
The SPU retains its independent physical check that resident plus input plus its
terminator fits, before any append mutation. Its existing capacity stop remains in
place. It does not know the harness's declared tail budget or selection policy.
MeasureDelta may report an oversized proposal without accepting it. No text is
truncated. The measured maximum must include concurrent state/classifier work and memory
pressure. The values stay open until qualified under Spec 09. [PLACE-3, PLACE-4]

## 3. N and the resident journal

**POSITION.** The harness keeps a session-local placement journal, not a second store or
a token vector. Each completed decode exchange contributes its canonical delta, the
returned canonical assistant message, unit/source occurrences and the SPU's resident
count at close. Open contributes the prefix count. A journal entry is usable only after
the generation's whole answer, when the terminator is resident. Spec 02's rejection
status and Spec 05's pair state are annotations, not changes to that recorded message. A
cut removes logical content past its mark and physical boundaries past N. Unchanged
content between N and the mark remains as rebuild. Reason: an append-only session is not
a fresh rendering of its whole history. [PLACE-5, PLACE-6, MARK-6]

**MESSAGE-MAP.** The journal's logical layout is an ordered sequence of canonical
Message occurrences. Allocate a checked message_id starting at one for each new delta
Message and one for the expected assistant answer. IDs are never reused after cancel or
refusal. An occurrence retains its ID when an unchanged prefix is recomputed. A reminder
is a new occurrence and gets new IDs. The layout has no system Message, because the
permanently seated head is a separate sentinel. A step prefix retains the exact
step_successor query's StepQueryRef in UnitMapEntry.step_queries. It is an addressed
instruction, never an implicit dump of previous work. [MARK-6, PLACE-1, FLOW-3, R20-10]

ArgumentCursor is externally tagged head:{} or message:{message_id:DecimalU64,
block_end:DecimalU64,byte_end:Option<DecimalU64>}. block_end is the exclusive
ContentBlock index. A non-null byte_end ends inside the last included Text at a valid
UTF-8 byte boundary. Null keeps the whole block. No byte cursor addresses a ToolCall,
ToolResult or UnrecoveredCall. A zero block_end is legal only for a real empty Message,
with byte_end null. Keep earlier blocks and messages unchanged, and discard later blocks
when taking the prefix. A byte_end equal to Text length has the same ordering as that
whole block. Cursors order by the current logical layout, then block and byte end, never
by the numeric message ID. [MARK-2, MARK-6, PLACE-5, PAIR-4]

Spec 08's unit-local SourceMapEntry is {material:MaterialRef,message:DecimalU64,
block:DecimalU64,start:Option<DecimalU64>,end:Option<DecimalU64>}. A Text location has
both offsets and a nonempty valid interval. Other blocks have both null and refer to the
complete block. The adapter maps every text interval and pair request/return to its
canonical location. Place converts it to PlacedSource
{material,message_id,block,start,end}. It checks bounds and exact text-offset
correspondence before measuring. A text MaterialRef's absolute source interval has the
same byte length as its local mapped interval. The ordered distinct materials in
source_map must equal sources. Repeated locations for one source are permitted within a
whole span, and are not silently deduplicated. [STORE-3, MARK-3, MARK-6, PLACE-4]

Each completed generation adds a physical boundary {end:ArgumentCursor, resident:u64} at
the end of its whole assistant Message, including an empty answer as its own Message
occurrence. These boundaries are the only token positions the journal may reuse. Family
framing, schema rendering and the generation opener are counted in resident, not
assigned guessed message offsets. [PLACE-3, PLACE-5]

**MARK-POSITION.** A MarkPosition is {id:MarkOccurrenceId,span:SpanRef,
end:ArgumentCursor,source:MarkPositionSource}. MarkPositionSource is externally tagged
boot:{}, placed:{unit:UnitOccurrenceId}, or native:{event:SourceBlockId}. Allocate mark
ordinals from zero, with zero reserved for the boot system span at head. A private
constructor requires the following evidence before registering an active occurrence and
authoring state.mark.position. [MARK-1, MARK-3, MARK-6]

| Source | Required evidence and exact endpoint |
| --- | --- |
| boot | Recorded boot recall, matching validated Opened, Recorded boot placement, checked fit and live admission under HARNESS-OPEN. Span is the seeded system span and end is head |
| placed | A successful complete append containing one whole closed-span unit. End is the last block of that unit's last Message, including any complete attached return |
| native task prompt | Filed begin_task or operator plan effect and the prompt's complete held Text is present in one current source-map interval. End is that interval's end |
| native execution end | Filed close/retire effect for an observed active task, subtask or step. End is the logical argument end after its last completed generation and before later-scope generation |

The native execution rule binds a scope's argument boundary, not every state member to
an argument Message. Review instructions and records can be held without being placed.
The loop records the scope's first observed placement and last completed generation, and
refuses a native mark whose claimed end is no longer the current scope end. A scope with
no observed argument work gets no native occurrence. Its stored span is still readable
and can acquire a placed occurrence later. step_outcome uses the placed rule, since its
statement is not automatically part of the model's argument. [MARK-1, MARK-3, PLAN-2,
PLAN-4, FLOW-8]

At registration require the boot evidence above, source effect receipt or whole append
completion as applicable, the
matching span identity and the exact current cursor. A preview never supplies this
evidence. MarkPosition is measurement only and creates no store field or filing.
Repeated reminders leave earlier valid occurrences alone. A cut removes every active
occurrence whose end is after its chosen cursor. Positions before or at that end remain
valid through recomputation with the same Message IDs. Their physical token position may
change and is always derived afresh. [MARK-6, STORE-8, TAP-3, PLACE-5]

**COVERAGE-JOURNAL.** Record each selected unit's MaterialRefs against its message
interval. A completed assistant answer gets Text coverage from that emission's
SourceBlockIds and exact decoded byte intervals. After a PairStarted receipt, attach
request coverage to the originating call or recall binding already resident there. This
annotation changes no message, token or trace event. A result placement adds returned
coverage. The pair coordinator and emission recorder supply these identities, never the
model or a caller-selected role. [STORE-5, PAIR-1, MARK-6, FLOW-8]

A proposed cut derives coverage from the canonical prefix it will keep, including the
unchanged material to be recomputed after conservative N. Coverage of a removed suffix
is unavailable even before Flush executes. A result unit is legal only when that logical
kept prefix covers its request and statement. Otherwise Spec 08 must provide the
complete held pair. Place verifies this condition without selecting a replacement
itself. An incompatible result unit refuses invalid_pair before buffer mutation. Reason:
a fresh return must not duplicate its already resident send, and a later replay must not
lose that send. [PAIR-4, PLACE-4, PLACE-5, MARK-4]

**PREFIX-PREPARE.** place.prepare_prefix(assembly_id, cut, method) returns
PreparedPrefix or PlaceRefused. Its constructor is private. CutMethod is flush or elide.
It chooses the existing SPU operation, not a cut location or strategy. PreparedPrefix
holds assembly_id, journal_revision:DecimalU64, cut:Option<CutReference>,
method:CutMethod, end:ArgumentCursor, N:u64, resident_before:u64, prefix_tokens:u64,
capacity:u64, rebuild:Vec<RetainedMessage> and coverage:Vec<MaterialRef>.
RetainedMessage is {message_id:DecimalU64,message:Message,sources:Vec<PlacedSource>}. It
is unchanged journal content, not a caller-authored unit or another state read. The
journal revision advances on placement completion, cut, elision, source annotation and
mark registration. An outdated grant refuses stale_position before mutation. [PLACE-4,
PLACE-5, MARK-6]

place.measure(prefix, units, tool_schemas) is the read-only adapter used by sort. It
validates the private prefix grant and each unit map, then submits exactly
prefix.rebuild's Messages followed by the units' Messages to MeasureDelta, with keep
equal to prefix.N. It returns DeltaMeasured or PlaceRefused and checks all returned
facts against the current journal. It neither allocates argument occurrences nor mutates
the journal or SPU. The final place uses the same complete delta and schemas. A
measurement never authorizes a different rebuild or selection. [PLACE-3, PLACE-4,
PLACE-5, SORT-1]

With cut null, the logical end is the current layout end. With a cut, require that its
active MarkOccurrenceId belongs to this run and names the same SpanRef, then take
exactly that cursor's prefix. Unknown, removed or mismatched occurrences refuse
invalid_cut. A caller cannot supply an arbitrary cursor or a token count. Source
coverage is projected onto that prefix using the maps above. A cut cannot retain part of
a paired request interval or a return block, or keep a placed return while removing its
request. It also cannot remove a placed return while retaining its request. Such a
boundary refuses invalid_pair rather than being rounded to a different logical mark.
[MARK-2, PLACE-5, PAIR-4]

**N.** For an ordinary append with no staged rebuild, N is the last SPU-reported
resident count. For a marked cut or a pending unchanged rebuild, choose the latest
still-resident completed boundary at or before the chosen logical end. Its resident
count is N. The system-head end always uses prefix_tokens. If no completed boundary
fits, use prefix_tokens. No boundary removed by an earlier Flush may be reused. rebuild
contains the exact canonical prefix after that boundary through the chosen end, with
original Message IDs and source maps. It precedes every new selected unit. A cut inside
Text keeps only that valid UTF-8 prefix in its rebuilt Message. This rounds N back to a
known whole exchange boundary, never forward from a byte offset. [MARK-2, MARK-3,
PLACE-5, PLACE-6, PAIR-4]

The logical layout excludes family scaffolding and schema-rendering blocks. Rebuild
preserves canonical agent Messages, and the SPU renders that suffix afresh with the
current loaded schemas and generation opener. Tokens after N are recomputed, so this
does not assert byte or token equality for the old rendering there. Every token at or
before N remains untouched. [PLACE-1, PLACE-5, PLACE-6]

This is a deliberately conservative N. It keeps the logical prefix through the mark
while recomputing any retained text after the last safe exchange boundary. It never
rounds a token position forward from a byte position. Use the SPU's MeasureDelta
resident and prefix facts to check prefix_tokens <= N <= resident. A mismatch is
stale_position, not permission to guess a larger N. A mark repeated as a reminder is an
address only unless the selected current occurrence is the boundary of that unchanged
prefix. [MARK-2, MARK-6, PLACE-5]

**FLUSH.** At CutReady send the existing Flush {keep: N}. Require Flushed's counts to
match the measured resident_before and N. The existing flush trace event carries exactly
those authoritative counts. The next generation uses AppendAndGenerate with only the
rebuilt suffix and new material. Flush {keep: 0} means prefix-only under the merged
contract, never a claim of N = 0. Changing the model, template or system prefix requires
a new admitted residency, not a new key on this session. Reason: the resident prefix is
permanent and all buffer work belongs to the SPU. [PLACE-2, PLACE-5, PLACE-6, PERSIST-7]

After confirmation, truncate the logical layout at the chosen end, remove physical
boundaries past N and invalidate mark occurrences past that end. Retain rebuild in RAM
with its original Message IDs and projected source maps. It represents unchanged content
still owed to the next argument, not resident KV. Any caller-supplied new units are
staged separately as carry. Do not add carry to the logical layout before append
completes. A bare cut may later shorten the retained logical prefix again. A composite
Staged result instead pins its tail until the next append under STAGED-APPEND. It
resolves against that prefix and the remaining physical boundaries, so it cannot reuse a
token merely because its Message is waiting to be recomputed. [MARK-2, MARK-6, PLACE-5,
STRAT-3]

The next place prepends retained rebuild once, followed by the selected units. It
measures that full delta and schemas again with any new input. On complete generation,
reuse rebuilt Message IDs in the layout, add the newly allocated Messages and assistant
answer, clear the staged rebuild and carry, and record the new physical boundary. Failed
fit or cancellation before dispatch preserves staged content. A composite tail can only
be retried as the same pinned head, not replaced by another staging operation. Unload
discards these engine intermediates, since their source content was filed before the
cut. No buffer dump or special boot path is introduced. [PLACE-1, PLACE-4, PLACE-5,
MARK-4, PERSIST-6, DEFER-2]

**ELIDE.** A CutReady with method elide sends the existing Elide {from:N,
to:resident_before}. Require Elided.resident_before to match the measured value and
resident_after to equal N. This suffix removal has exactly the logical prefix and
rebuild of FLUSH. Use the same journal update and next-append rules. If N equals
resident_before, send no Elide and author no elision event. Keep the already confirmed
count and enter the next state directly. The SPU refuses an empty span. Either reply's
wrong counts are a broken decode contract. The caller chooses flush or elide, and both
are tested. [PLACE-5, PLACE-6, MARK-4, R20-9]

The SPU still exposes its general Elide primitive under its merged contract. This
marked-cut adapter issues only suffix elision. A caller that uses a different resident
mutation invalidates all affected journal grants and must establish a new journal before
state place can accept work. It may not preserve a suffix as if it were an unchanged
prefix. No method provides a middle swap. Reason: both buffer operations remain
available while N still bounds the unchanged prefix. [PLACE-5, PLACE-6, R20-9]

## 4. The cut coordinator

**CUT-WAIT.** Start the foundation cut-wait bound when a requested cut enters the
between-turn pause, after turn.closed. Freeze the removed-work obligation set. The cut
port takes exclusive mutation ownership. It admits no new decode work and drains
classification needed to resolve that set and close its marks. The single pump continues
draining its existing ports. The same deadline covers filing, mark closure, quiescence,
measurement and cut confirmation. It never restarts at a state transition. Record
elapsed time and the outstanding set. The earlier lifecycle deadline, if any, wins.
Reason: a hung dependency cannot move to a fresh clock by changing phase. [MARK-4,
CLASS-8, COST-4, October 18 R7]

Quiescence means no outstanding append, re-feed or classify exchange on either
classifier port, no tool execution, and no pending filing or mark needed by the cut. The
existing legacy label scorer may retire under its own optional contract, but an active
ask must finish or retire before the cut. Closing its optional channel does not erase an
STE100 obligation. New gate inputs are held outside the model until the cut either
completes or refuses. [MARK-4, CLASS-6, CLASS-8]

| State | Observation | Action and next state |
| --- | --- | --- |
| RunningTurn | Cut requested | Record request, defer to turn close, no Flush |
| BetweenTurns | Valid requested mark | Start one deadline, freeze obligations, Draining |
| Draining | Correct rejection or unselected item | Keep disposition, remove from owed set, remain |
| Draining | Filing receipts and closure complete | Stop remaining scheduling, Quiescing |
| Quiescing | No outstanding compute | Measure exact proposed suffix, Measuring |
| Measuring | Fit and position checks pass, nonempty suffix or method flush | Issue private CutReady, send chosen operation, Cutting |
| Measuring | Checks pass, method elide and N = resident_before | Skip Elide, retain confirmed count, Ready |
| Measuring | Typed fit/address refusal | Trace refusal, preserve old resident sequence, Refused |
| Cutting | Matching Flushed or Elided counts | Update journal and trace placement, Ready |
| Any waiting state | Bound expires or component fails | Foundation failure latch, no new cut or decision |
| Before buffer mutation | Stop or leave | Cancel proposal, account pending input, lifecycle owns next state |
| Cutting | Stop or leave | Consume the outstanding answer, account committed cut, no generation, lifecycle owns next state |

A cut pass certifies that the removed obligations are disposed, the named boundary is
unchanged, all required ports are quiescent, the exact replacement fits, and N names a
known resident prefix. It does not certify that sort selected the right history. The
CutReady constructor is private to this coordinator. No model can supply it. [MARK-2,
MARK-4, PLACE-5, FALS-4]

### 4.1 Cut-and-reload composite

**CUT-RELOAD.** R20-15 and R20-16 retain cut-and-reload as a fixed composite primitive
of the harness engine. It drains, quiesces, checks and files new material, measures,
cuts the suffix and pins the replacement. It ends Staged. The next caller append
performs the refill and generation under the existing decode contract. It chooses no
summary, strategy, trigger or next task. Reason: the component must preserve its input
through the cut without inventing a between-turn serving exchange. [MARK-4, PLACE-4
through PLACE-6, SEAM-2, R20-15, R20-16]

The local port is engine.cut_and_reload(prefix:PreparedPrefix, tail:ReloadTail).
ReloadTail is a private ordered Vec<ReloadItem>. ReloadItem is held:SelectedUnit or
pending:SourceBlockId. pending is constructible only from an owned, admitted whole Text
copy in the live filing ledger, never arbitrary text or a supplied source ID. It
requests that whole block's UNIT representation after filing. The engine uses its loaded
schemas. No token path, policy selector or model instruction enters this port. [FLOW-1,
STORE-8, MARK-4, PLACE-5, R20-12, R20-15]

The port returns Staged {assembly_id:DecimalU64,N:u64,resident_after:u64,
tail:StagedTail,measured:DeltaMeasured}, ReloadRefused
{assembly_id:DecimalU64,code:ReloadCode,detail:String,tail:ReloadTail}, or
Busy {prefix:PreparedPrefix,tail:ReloadTail}. StagedTail privately holds
{rebuild:Vec<RetainedMessage>,units:Vec<SelectedUnit>}. Its order is rebuild followed by
units. Staged owns a single-use, run-bound engine reservation. It cannot be cloned,
deserialized or constructed by the caller. A second cut or staging call returns Busy
with that call's inputs unchanged while the existing reservation is held. Busy acquires
no reservation and starts no wire request. ReloadCode is classifier_not_admitted,
invalid_prefix, invalid_tail or non_ste100. These are local types, not new wire
messages. [PLACE-4, PLACE-5, SEAM-2, R20-16]

resident_after is the count confirmed by Flushed or Elided. For the skipped empty suffix
it is the already confirmed resident count, never a fabricated answer. measured is the
exact pre-cut MeasureDelta result for rebuild, units and loaded schemas. The stage
succeeds only with resident_after = N = measured.keep. Its measured.resident still names
the pre-cut resident_before. No staged message is yet resident, and no new argument
occurrence is registered. [PLACE-3, PLACE-5, MARK-6, R20-16]

**RELOAD-CLASSIFY.** Require the declared STE100 process to be admitted and standing
before accepting this operation. Absence refuses classifier_not_admitted without
mutation. Loss after acceptance is component failure. For pending Text use its existing
classifier target and trace the result. Require its normal filing receipt before
constructing a unit. A correct rejection returns non_ste100 with the source accounted on
the trace. The composite performs no restatement. Held units retain their checked
read/receipt proof and are neither classified nor filed again. Reuse a completed check
already owned by the ledger. Tool returns remain unclassified pair blocks, and the boot
prompt is never filed or appended again. [CLASS-2, CLASS-8, CLASS-10, STORE-2, STORE-8,
PROMPT-1, MARK-4, R20-3, R20-15]

**RELOAD-STATES.** One cut deadline covers drain through the Staged result, with an
earlier lifecycle deadline winning. It ends when staging completes. The next append uses
its own existing decision/decode deadlines, not the expired cut clock. Preserve the old
resident sequence until fit, disposition and quiescence pass. [MARK-4, CLASS-8, PLACE-4,
PLACE-5, COST-4, R20-16]

| State | Observation | Action and next state |
| --- | --- | --- |
| Idle | Classifier absent or invalid private input | Named refusal with inputs, no mutation |
| Idle | Valid inputs and admitted dependencies | Own inputs and mutation reservation, QueueCut |
| QueueCut | External turn open | Wait for turn.closed, keep old resident |
| QueueCut | Between-turn pause | Start cut deadline once, freeze obligations and sources, Draining |
| Draining | Pending Text | Classify/file through the existing ledger, stay Draining |
| Draining | Correct rejection | Trace disposition, return non_ste100, no cut |
| Draining | Removed obligations disposed and tail held | Quiescing |
| Quiescing | No active compute | Measure exact rebuild, tail and schemas, Measuring |
| Measuring | Fit/address refusal | Preserve resident, return invalid_prefix or invalid_tail |
| Measuring | Checks pass, nonempty suffix or method flush | Mint CutReady, send chosen suffix operation, Cutting |
| Measuring | Checks pass, method elide and N = resident_before | No Elide or elision event, pin tail, Staged |
| Cutting | Matching Flushed or Elided | Record cut, retain rebuild, pin tail, return Staged |
| Any wait before Staged | Bound expires or component fails | Foundation latch, no rollback claim |
| Before mutation | Stop or leave | Return inputs and preserve resident, lifecycle proceeds |
| Cutting | Stop or leave | Consume the pending cut answer. Matching Flushed/Elided pins the tail, Staged with no generation. A refusal confirming no mutation returns the inputs, Idle. A missing or invalid answer takes the foundation failure latch |
| Staged | Another stage or cut | Return Busy with inputs, preserve reservation |
| Staged | Append without its private grant | No constructor available, no wire request or mutation |
| Staged | Owner supplies next append | STAGED-APPEND consumes reservation through confirmation |

A confirmed cut cannot restore its old KV suffix on failure. Live restore's existing
prefix-only flush uses this composite after the separately authorized store operation.
An empty tail is valid. The permanent system head is already resident. The next turn
appends the restored canonical context. No store replacement policy is added here.
[FLOW-4, MARK-4, PLACE-5, PERSIST-7, R20-15, R20-16]

**STAGED-APPEND.** The engine's single delta constructor requires the outstanding Staged
grant whenever one exists. It prepends StagedTail.rebuild and units exactly once before
new caller units. No direct AppendAndGenerate path may bypass that constructor,
including tool rounds and lifecycle callers. It measures this entire delta and the
loaded schemas again at keep = resident_after. Name this second DeltaMeasured
final_measure. It covers new input, family framing and generation openers. The stored
stage measurement alone never grants an append with added input. All PLACE-PORT and
capacity checks still apply. A typed fit refusal returns the same reservation and owns
no new mutation. Reason: new input may change the rendered length even when the pinned
tail is unchanged. [PLACE-3, PLACE-4, PLACE-5, MARK-4, R20-16]

The private PlacementGrant retains the Staged reservation through enqueue and answer. On
dispatch it permits exactly one AppendAndGenerate with those measured Messages and
schemas. A complete Generated answer confirms placement with checked arithmetic:
resident_after + final_measure.input_tokens + output_count + terminal_tokens must equal
Generation.resident. capacity must match the admitted capacity. output_count is the
length of Generation.measurement.output_tokens, whose existing SPU producer excludes the
terminator. Also require measurement.input_tokens length = final_measure.input_tokens.
Validate those arrays' existing shape without retaining a token vector or tokenizing
text in the harness. The current terminal_tokens is one under CAPACITY. The resident
count includes it. Wrong or missing facts are component failure. [PLACE-3, PLACE-5,
FLOW-4, R20-16]

Only that confirmation installs rebuilt and new Message occurrences, clears the pinned
tail and releases the reservation. A complete stopped Generated answer still accounts
for its seated input, partial output and terminator. Cancellation before dispatch
returns the same Staged grant. Cancellation after dispatch consumes the decode seam's
terminal answer and verifies it, never retries an uncertain append. Missing answers
follow component failure. Unload may discard filed engine intermediates after the
existing lifecycle account. The stage does not itself start the next turn. Prefill
latency is charged to that next append. ReFeed remains diagnostic-only. [FLOW-4, MARK-6,
PLACE-5, COST-4, PERSIST-6, SCOPE-1, R20-16]

## 5. Trace and failure surfaces

**PLACEMENT-RECORD.** Author state.placement, channel internal, origin loop, before the
next append. Its payload is {assembly_id, phase, context, cut, method, N, prefix_tokens,
resident_before, input_tokens, capacity, generation_limit, terminal_tokens, unit_map,
message_ids, assistant_message_id, recomputed_messages, wait_ns, open_evidence}. phase
is boot, cut or append. context is null for boot/cut and PlacementContext for append.
method is null for boot and CutMethod otherwise. message_ids and recomputed_messages
use Placed's types.
assistant_message_id is DecimalU64 or null. It is measurement, not selectable statement
content. Each newly authored loop instruction has its own earlier event under FLOW-8.
Held material cites its original source. The assembled argument record includes the
complete selected canonical messages, including the boot head by reference, without
filing them again. open_evidence is required and is null for cut and append. For boot
it is exactly {session,request,measurement}, with the Opened session string and the
unchanged SPU-OPEN raw boxes. Its session equals the containing trace session and the
held Open request. Its input-token length equals placement.prefix_tokens. It grants no
selectable content or generation event. [FLOW-3, FLOW-8, MARK-6, COST-4]

Reserve assembly_id zero for boot. After a validated Opened, record one turnless
state.placement with phase boot, cut null, N and resident_before both prefix_tokens,
input_tokens zero, empty unit_map and message_ids, null assistant_message_id and
recomputed_messages zero.
wait_ns is the completed open wait, excluding recorder wait. Its causal_parent names
the earlier boot recall that recorded the full system Message. Subsequent assembly IDs
start at one. The record accounts for a seated prefix, including when the valid facts
subsequently fail the configured fit. That case records the boot placement and then the
existing boot_failed enter refusal, with no journal readiness or user gate. An SPU Open
refusal or invalid answer has no accepted boot placement. Only Recorded plus successful
fit and live admission releases initialized journal/decoder-use authority and boot mark
readiness. HARNESS-OPEN owns interruption and teardown accounting under the existing
bounds. There is no second prompt read, filing or Open. [PROMPT-1, PERSIST-6, PLACE-1,
FLOW-3, FLOW-4]

After a confirmed Flush or Elide, phase cut records its preparation's measured
replacement and recomputed_messages count, with empty unit_map and message_ids and null
assistant_message_id. It records no append and creates no argument occurrence. Its
staged carry remains outside the logical argument. A later append has a new assembly_id,
phase append, the complete delta Message IDs, reserved assistant ID and the selected
unit map after the rebuild interval. Append records precede the append request. Only a
completed generation makes those prospective occurrences usable. An attempt with no
complete Generated answer cannot make a mark from a planned placement. A complete
stopped answer uses STAGED-APPEND's confirmed partial-output account. [FLOW-3, MARK-6,
PLACE-5, COST-4]

state.mark.position is channel internal, origin loop, with payload MarkPosition from
MARK-POSITION. It is authored after the receipt or append completion that licenses
registration, never by editing an earlier event. It is measurement, excluded by the
partition loader. It binds a stored span to the precise canonical argument occurrence
without asserting a token offset inside a Message. [FLOW-3, FLOW-8, TAP-3, MARK-6]

**FIT-REFUSAL.** Add RefusalRecord::Placement {assembly_id, code, detail} to the
existing refusal event, with codes from PLACE-PORT. If a turn is open, author
turn.closed with its existing refused reason and send the matching stopped close with
run and turn. Before a turn opens, send the existing unnamed refused close without run
or turn. Between turns with no requester, the trace refusal is the operator surface and
the next requester receives it before a generation starts. A fit or address refusal sets
no component-failure latch. Sort may submit a new selection only with its own traced
criterion. [PLACE-4, FLOW-4, SORT-3]

**DEFERRED.** Exported KV handles and restoration remain the deferred KV-cache seam on
the SPU, not fields in place or the store. The hot session used here is already
implemented and is not the deferred feature. No first-release structure contains device
pointers, cache entries or a harness tokenizer. [DEFER-1, DEFER-2, PLACE-6]

## 6. What is enforced, and by which instrument

**INSTRUMENTS.** Each runtime check below has a fixture that passes normally and fails
when its named protective predicate is removed. Compile and manifest checks cover
unavailable construction and dependency edges. Reason: a nominal assertion cannot
demonstrate that a bad N or dangling mark is refused. [MARK-4, PLACE-5]

| Decision | Instrument and required observation | Protective change |
| --- | --- | --- |
| IDENTITY, MARK-STORAGE | Store fixture, reload preserves IDs and rejects stale revision and crossed scope | Remove revision or nesting check |
| MEMBER, SPAN-ORDER | Store fixture, ignored events make no holes and a pair is returned whole | Admit partial pair or sort by trace sequence across runs |
| MARK-READ | Ended span and preview fixtures with a later pair on an old request | Ignore the fixed frontier, return an open pair, or allow preview as a cut boundary |
| BOUNDARIES | Harness fixture, model prose cannot close step or replace task | Bypass authority or review receipt |
| MARK-FILE | Compile pin, effect only through admitted StatementFile | Expose effect-only store write |
| CLOSE-DISPOSITION | Reject one pending candidate, close contains only filed members | Author members before disposition |
| PLACE-PORT | Compile pin for refusal without delta, deterministic message/map fixture | Permit argument on refusal or reorder units |
| STEP-QUERY | Closed predecessor, first step, repeated instruction and foreign revision fixtures | Lose typed query context, substitute previous work or accept a different predecessor |
| PLACE-CALLS | Private grant, no-decode spy, stale grant and matching-completion fixture | Start decode during preparation, reuse grant or install prospective IDs without completion |
| OPEN-FACTS | Real SPU open and harness transport fixtures, exact render/tokens/partition, session/port/send correlation and one prior boot recall | Reappend head, tokenize twice, accept stale/partial/wrong-session facts, change boxes or omit prefix count |
| MEASURE-DELTA | SPU fixture, measure leaves resident tokens and sampler untouched | Decode during measure or use different render path |
| CAPACITY | Boundary fixtures include schemas, opener, generation and terminator reserve | Omit any counted part |
| POSITION, N | Session fixture with inside-message mark, repeats and multiple tool rounds | Round N forward or use text token equality |
| MESSAGE-MAP, COVERAGE-JOURNAL | Mixed Text/call/result and repeated-span fixture | Map by equal text, split a whole return or lose a repeated source location |
| MARK-POSITION | Native closure with unplaced review records and replayed-span fixture | Use the last stored member as the argument position or permit preview as a mark |
| PREFIX-PREPARE | Successive cuts before append, empty answer and stale-grant fixtures | Reuse a removed physical boundary, duplicate rebuild or ignore journal revision |
| FLUSH | SPU fixture, keep zero leaves prefix, returned counts match mutation | Report zero or skip clamp |
| ELIDE | SPU fixture, survivor sequence recomputed after removal | Keep stale suffix entries |
| CUT-WAIT | Pump fixture, one deadline across drain, measure and both suffix operations | Restart deadline or allow mid-turn cut |
| CUT-RELOAD, RELOAD-CLASSIFY | Missing classifier, pending/held Text, rejected Text and complete-pair fixtures | Skip classification/filing, refile held content or classify a return |
| RELOAD-STATES | Every state/stop/leave row, including empty suffix, empty tail, Busy ownership and cancellation with a confirmed or refused cut | Mutate before fit, lose the rejected call's inputs, discard a confirmed tail, report staged material resident, install unconfirmed IDs or restart the cut deadline |
| STAGED-APPEND | Private grant, pinned ordering, added-input overflow, full/stopped answer counts and cancellation fixture | Bypass or replace the pinned tail, consume it twice, use only its old measure, omit terminator, accept wrong resident count or clear before confirmation |
| PLACEMENT-RECORD, FIT-REFUSAL | Trace/world capture, boot success and fit refusal, invalid Opened, recorder pressure and bounded teardown | Omit or duplicate placement, grant before Recorded, lose retained evidence, grant after cancellation or name an unopened turn |
| DEFERRED | Cargo manifest check, harness links no tokenizer or SPU member | Add a forbidden dependency |

The token oracle is a session that appends the same actual deltas and sampled paths from
the same prefix, then performs the same Flush or Elide and appends the same rebuilt
suffix. It is not a fresh render of the entire conversation. Compare resident token
sequences inside SPU tests, where they are owned. Test every admitted family and both
supported flush mechanisms. [PLACE-5, PLACE-6]

## 7. Owning documents and integration

**AMENDMENTS.** The following replacement clauses belong to this document act, before
code. Each amended section carries the transition link from section 0. Reason: a reader
of one owning Spec must see the change and its applicability. [FLOW-7, SEAM-2, PLACE-5]

| Owning document | Section | Replacement or addition |
| --- | --- | --- |
| weaver-types-Spec | 4.4 | Opened facts, MeasureDelta/DeltaMeasured and tool_schemas member defined in section 2 |
| weaver-types-Spec | 2 | placement declaration, SpanRef/member/effect and Placement refusal shapes |
| weaver-harness-Spec | 5 | Replace harness-assembly-kind-filter-at-read-site with typed selected-source assembly. Keep harness-deterministic-assembly over the same selection and source bytes, and harness-prompt-part-order over head, messages and schemas |
| weaver-harness-Spec | 6 and 6.1 | Sections 2 through 5 replace the state-placement path, leave stream, cancel and tool-round handling intact |
| weaver-spu-Spec | 4.2, 4.3, 5, 6, 9 and 10 | Read-only measurement, effective open facts and identical family rendering in the SPU amendment. Existing 4.4/4.5 Flush/Elide remain |
| weaver-spu-PRD | 13.4 and 13.9 | Retain family-owned rendering and buffer custody. Add the read-only fit exchange and its no-mutation guarantee |
| weaver-harness-spu-decode-contract | 2 through 4 and 8 | Add the agreement below and its conformance cases |
| weaver-state-Spec | 3 and 4 | Section 1's mark schema, effects, guards and span read union |
| weaver-trace-PRD and Spec | 3.1 and 3 respectively | state.mark, state.mark.position, phased state.placement, Placement refusal and unchanged flush/elision counts |
| weaver-harness-diagnostic-contract | 3 and 7, record vocabulary | Reject these new serving kinds as OutsideDiagnosticVocabulary under DIAGNOSTIC-CONVERT. No serving mark or cut authority enters the diagnostic record |

The decode agreement adds one read-only exchange: the harness supplies a proposed keep,
canonical delta and granted schemas. The SPU supplies exact render counts without
mutation or sampling. Neither may treat that answer as an append. Only Flush and Elide
reduce the resident sequence. Append and generate consumes the same schema
representation as measure. Open reports the permanent prefix count. The existing
one-reader, whole-message, typed-refusal and closure rules cover the new exchange. Its
refusals leave resident state unchanged. [PLACE-3, PLACE-5, PLACE-6]

C06-1 is integrated in the foundation's schema, StatementFile, context projection, mark
reads and harness boundary coordinator. C06-2's placement configuration, registry,
refusal and decode facts now have matching owning text. C06-3 is the [SPU
amendment](weaver-spu-Spec-amendment.md), TYPES-DECODE, HARNESS-PLACE
and the decode agreement in contract-amendments section 7. C06-4 joins Spec 07's
explicit activation/closure ports and Spec 08's mapped units, PreparedPrefix and
selection-to-place fields. Invocation order belongs to the loop under R20-12. The set
contains no strategy pass coordinator. CUT-RELOAD retains the fixed composite protocol
under R20-15/R20-16, ending Staged with confirmation on the next append. These deltas
belong to the single authorizing set. No partial contract is ready for implementation.
The foundation's section 6 INV-2 and INV-5 clauses specify the dependable flow and
classifier result prerequisites. This Spec defines the added mark and placement
mechanics. [MARK-5, PLACE-5, FLOW-7]

## 8. Open measured values and dependencies

O06-1 is C_head and C_tail under the selected family and hardware. Measure maximum
supported tail, all framing, actual schemas, peak returns and concurrent memory use, and
verify the family's existing one-token terminator. O06-2 is the cut-wait bound,
including classification drain, mark filing, measure, both Flush and Elide, and the
composite staging. Measure next-append prefill and confirmation separately under
STAGED-APPEND, since that latency belongs to the next turn. Neither is elected here.
Qualify both in the later loop under the same tuple. Freeze values before held-out
qualification and record both work and wall cost. [PLACE-3, CLASS-8, MARK-4, COST-1
through COST-4]

O06-3 is the family render extension's implementation evidence, including exact counts
and the unchanged-prefix oracle. O06-4 is integration of C06-1 through C06-4 and the
root-authorized task/group definitions in Spec 07. None is an operator choice about the
already ruled timing or tokenizer ownership. The separate reviewed system prompt and
qualified production classifier remain cutover prerequisites. [PROMPT-3, CLASS-7,
FLOW-7]

[transition]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover

[open-election]:
../../project/state-management-open-facts-carrier-2026-10-21.md

[open-spu]:
weaver-spu-Spec-amendment.md
