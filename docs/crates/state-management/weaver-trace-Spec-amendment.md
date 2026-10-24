# weaver-trace Spec: dependable-flow amendment

**Status:** MERGED.

**Version:** 0.27 **Date filed:** 2026-10-21 **Parent:** weaver-trace-PRD
**Landing PR:** #312

## 0. What this document is

Replacement text for trace Spec sections 3, 5, 6, 9, and 11, with instruments added to
section 10. It implements FLOW-1 through FLOW-8 and TAP-1 through TAP-5 at the trace
boundary. It requires the charter and harness-trace contract amendments in this packet.
The crate still authors no event and owns no store interface.

## 1. Envelope and event registry, destination section 3

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TRACE-ENVELOPE.** Add `channel: Channel` and `origin: Origin` after `kind` in
`event::Envelope`. Define these serialize-only enums in weaver-trace, which does not
gain a dependency on weaver-types. Channel spellings are `input`, `output`, `internal`,
`tool`, and `measurement`. Origin spellings are `operator`, `external`, `model`, `loop`,
`tool`, and `runtime`. The harness constructs these values at its one authoring boundary
from HARNESS-AUTHOR's kind/source table and verified caller identity. No additional
Channel or Origin wire enum is implied in the floor. Reason: TAP-1's predicate is on the
recorded envelope, not a payload inference. [TAP-1, TAP-5]

Keep existing canonical rules: sequence and monotonic_ns are decimal strings, wall_ms is
a JSON number, and absent turn and causal_parent are omitted. Keep subsystem `harness`
for conversation messages. Message and ContentBlock remain the weaver-traits shapes. No
top-level message `text` or `return` is introduced. [FLOW-3, STORE-5]

**TRACE-REGISTRY.** Define `tee::ContentSchema` and a total `content_schema(Kind) ->
ContentSchema` in this crate. Its registry ID is `state-copy/1`. The schema is a finite
projection surface, not deserialization of every trace payload. This registry owns the
names accepted by the harness's load-time validator. Admin validates syntax without
linking this crate. Reason: the trace owns its event vocabulary, while the harness links
both the trace and declaration parser. [TAP-1, TAP-3]

| Kind | Payload representation | Projection surface | Turn rule |
| --- | --- | --- | --- |
| message.user, message.assistant | Existing Payload::Message, canonical Message JSON | content.text[*], content.text[index] | Required |
| message.tool_result | Existing Payload::Message | Forbidden, model-facing copy of a held pair result | Required |
| tool.call.started | Serving PairStarted from Spec 05 | pair, whole payload | Required |
| tool.call.completed | Serving PairEnded from Spec 05 | pair, whole payload | Required except abandoned_at_load, which forbids a turn |
| tool.return_rejected | RejectedReturn from Spec 05 | Forbidden, unpaired or duplicate return | Optional |
| loop.instruction | Spec 02 LoopInstructionRecord | message.content.text[0] or wildcard, no context sidecar | Required except wind_down |
| sort.decision | Spec 08 SortDecisionRecord | message.content.text[0] or wildcard, no context sidecar | Optional |
| state.sort.measurement | Spec 08 SortMeasurement | Forbidden measurement | Optional |
| step.outcome | Spec 07 StepOutcomeRecord | message.content.text[0] or wildcard, context sidecar required | Required |
| plan.event | Spec 07 operator input or internal effect record | All message Text, context sidecar required | Forbidden for operator updates, required for model lock/activation/break |
| plan.break | Spec 07 PlanBreakReport | Forbidden metadata | Optional |
| state.plan.measurement | Spec 07 PlanMeasurement | Forbidden measurement | Required for next_ready, optional otherwise |
| state.mark | New Payload::StateMark holding template_id, one-Text Message and context | message.content.text[0] or wildcard, context sidecar required | Required |
| state.mark.position | MarkPosition from Spec 06 | Forbidden measurement | Optional |
| state.placement | PlacementRecord from Spec 06 | Forbidden measurement | Optional |
| input.offered | New Payload::InputOffered holding input_id and the base64 TurnFrame.octets member, unchanged | Forbidden for state projection | Forbidden |
| state.flow | New Payload::FlowObservation, as harness amendment section 5 | Forbidden measurement | Optional |
| state.store.measurement | StateStoreMeasurement from Spec 09 | Forbidden measurement | Optional |
| ste100.result | Ste100ResultRecord, full typed result and correlation | Forbidden, derived classification evidence | Optional |
| load | Existing load payload plus state_flow: FlowDeclarationRecord and state_start: StateStartRecord | Untapped | Forbidden |
| recall | Payload::RecallRecord with TRACE-RECALL's serving read or boot variant | Forbidden, including boot recall | Forbidden |
| message.system, message.restored, save_point | Existing payloads | Forbidden | Existing rule |
| All other kinds | Existing payloads | No selectable surface in this amendment | Existing rule |

**TRACE-REJECTED-RETURN.** Spec 05's RejectedReturn.raw_envelope is a JSON string
whose decoded UTF-8 bytes equal the original organ frame's complete payload, as held
by ReceivedOrgan.raw_bytes, excluding transport delimiters and length prefixes. The
trace mirrors this as a serialize-only String field, never RawValue or a parsed
object. Serialize the
outer string normally so quotes, backslashes and literal CR/LF in that packet cannot
introduce a second trace line. The event retains one final newline and all existing
kind, channel, origin, attribution and partition rules. The harness retains the actual
received packet until the exact rejection is Recorded, through capacity pressure and
cancellation under the existing applicable bound. [FLOW-3, TAP-3, TAP-5, PAIR-2]

This representation applies to new foundation runs at the paired cutover. Historical
records whose raw_envelope is an object remain historical objects. Readers distinguish
the field types and preserve old source records without rewriting them into the new
shape or claiming a normalized object reproduces its original packet. Update every
producer and trace reader before cutover, including WeaverAnalysis's canonical trace
reader and its diagnostic/preload conversion path. This repository's authoritative
weaver-analysis-state contract draws this reader rule from the canonical trace;
WeaverAnalysis must mirror it and verify both historical shapes. The organ wire,
state-copy/1 registry and forbidden projection are unchanged. The old shape is not
accepted by the new publisher
as an alternate encoding for a new run. [FLOW-3, TAP-1, TAP-3]

The organ packet bound governs the received payload, not the escaped trace line.
The recorder has no per-record byte ceiling. The resulting line may exceed the organ
bound and is not PayloadMalformed merely for that size. Qualification must account
for the full outer event and escaping expansion, retained packet and author copies,
and recording time under the existing rejection wait. Test maximum admitted payloads
and control-character expansion; admission on the organ wire alone does not qualify
recording cost. Existing recorder failure and pressure rules still apply. Escaping
adds no recorder byte ceiling, new timing field, overflow queue or renewed deadline.
The record certifies exact evidence carriage, not the rejected outcome's truth or an
execution grant. [FLOW-3, FLOW-4, TAP-3, COST-4]

The existing refusal kind also carries the harness-authored raw encoding of
TYPES-RECALL-REFUSAL. It has no selectable projection. For that variant the harness
licenses internal/runtime, subsystem harness, the same open turn and the sequence of the
message.assistant containing the referenced checked statement as causal_parent under
Spec 02 RECALL-REFUSAL. The parent is not the ste100.result sequence. Trace keeps its
existing refusal payload representation and envelope checks. The harness validates the
floor carrier and its checked source proof before submission. Trace gains no floor
dependency, new kind or pair mirror, and does not infer bindings from the refusal
payload. [FLOW-3, TAP-1, TAP-5, READ-3]

`input_id` is a run-local decimal-string counter assigned by the harness at serialized
check intake, before input.offered authoring, under Spec 02's INPUT. The gate deadline
starts at that same intake. Arrival behind an active turn assigns no input_id and starts
no check deadline. [FLOW-3, CLASS-4, GATE-1]

LoopInstructionRecord and StepOutcomeRecord each have one Text block. SortDecision has
exactly one Text block. All have role user and no tool blocks, and their text comes from
the template inventory. The IDs alongside the Message are provenance, not selectable
text. Spec 07 owns the plan and step identities. state.mark, plan.event and step.outcome
have a context surface. Their explicit partition member context="context" selects the
top-level context bytes, alongside the complete Text block at index zero. The harness
owns their mechanical meaning under Specs 06 and 07, and the tee never interprets them.
A failed step.outcome has literal null context, which is still copied. Every other kind
refuses that projection until its owning amendment licenses it. The sidecar cannot
substitute for Text, contain another message or grant an effect by itself. [FLOW-8,
SORT-4, PLAN-2, CLASS-4]

The harness's kind-to-payload licensing checks these shapes before submit. Add the new
Kind variants to `event.rs::Kind`, `writer.rs::admit`, and
`writer.rs::pairing_licensed`, with explicit turn rules above. The diagnostic converter
rejects the new kinds as OutsideDiagnosticVocabulary and also rejects the new serving
payloads on the two existing pair kinds. Diagnostic runs do not author them. Analysis
must update its serving trace reader when this vocabulary lands. The old classify kinds
stay label-scoring records. [FLOW-3, SCOPE-1, October 18 R10]

**TRACE-PLAN.** Add serialize-only, trace-owned PlanInputRecord and PlanEffectRecord.
Payload::PlanInput carries {message,context:PlanInputContext,document:String}.
Payload::PlanEffect carries {message,context:PlanEffect,template_id:String}. Both
license Kind::PlanEvent, with Spec 07's exact PLAN-FEED field shapes and no extra
payload discriminator. The kind-to-payload check admits PlanInput only on
input/operator, without a turn. It admits PlanEffect only on internal/loop. authorize,
replace and operator-source lock forbid a turn. Model-source lock, activate and break
require one. The close effect is never a plan.event payload. Reason: the two authors
share one event kind without sharing update authority. [PLAN-1, PLAN-2, PLAN-6, FLOW-3,
FLOW-8]

Payload::StepOutcome carries Spec 07's exact StepOutcomeRecord and licenses only
Kind::StepOutcome on internal/loop with a turn. A closed outcome requires its matching
close context. A failed outcome requires null context. Payload::PlanBreak carries
PlanBreakReport and licenses only Kind::PlanBreak on internal/loop, with an optional
turn. The trace mirrors all nested identities and closed variants without linking
weaver-types. The harness performs the explicit conversion and retains the private
operator/review grants. Payload licensing checks recorded structure, never the truth of
a review or the authority claimed in text. [PLAN-2, PLAN-5, PLAN-6, SORT-4, FLOW-3]

Payload::PlanMeasurement mirrors PLAN-OBSERVATION's exact instrument_version, plan and
tagged event fields, including the measured/unavailable flag result. It licenses only
Kind::StatePlanMeasurement on internal/loop. next_ready requires a turn, while flags and
fit_refused permit an absent turn. Mirror the complete PlanFlagSnapshot, closed
read-refusal codes, fit stages and two fit codes without normalizing strings or dropping
an unavailable result. The harness validates echoes, count arithmetic and clock
endpoints before conversion. The trace records them and never obtains its own store
sample. Diagnostic conversion rejects the kind, and the loader refuses its admission
under TRACE-VALIDATION. [PLAN-3, PLAN-4, FLOW-3, TAP-3, COST-4]

## 2. Copy and projection, destination section 11

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TRACE-COPY.** Replace Election, ElectedKind, opener, distill, and socket-owning Tee
with `Partition`, `ValidatedPartition`, `Tee`, `AdmittedCopy`, and `TapOutcome` in
`tee.rs`. Only `validate_partition` can construct ValidatedPartition. Tee holds that
value and a terminal stopped flag. It contains no socket, store frame, or writer.
Reason: the tee selects a copy, and the loop owns carrying. [TAP-1, TAP-3, TAP-4]

| Type | Fields or variants |
| --- | --- |
| SourceBlock | run String, sequence Sequence, block u64 |
| SelectedBlock | source SourceBlock, source_path Vec<PathPart>, raw Arc<str> |
| ContextBlock | source_path Vec<PathPart>, raw Arc<str> |
| PathPart | Key(String), Index(u64) |
| StatementCopy | source_envelope canonical JSON, partition_rule_id String, blocks Vec<SelectedBlock>, context Option<ContextBlock> |
| PairCopy | source_envelope canonical JSON, partition_rule_id String, block SelectedBlock |
| AdmittedCopy | Statement(StatementCopy) or Pair(PairCopy) |
| TapFailure | source event identity, code, optional source_path |
| TapOutcome | HeadUntapped, Unselected, Selected(AdmittedCopy), Failed(TapFailure), Stopped |
| TapFailureCode | InvalidCanonicalRecord, MissingProjection, InvalidProjection, CopyAllocationFailed |

For StatementCopy, `raw` is the exact canonical JSON spelling of a selected
ContentBlock, including its type tag. For PairCopy it is the complete pair payload, with
virtual block index zero and an empty source_path. The tee does not classify either copy
or interpret a pair's fate. The envelope is copied whole. Block ordinal is the original
content-array index, not the index in a filtered result. The source path carries any
`message` nesting. The selector's text segment is a structural type selector, not a JSON
member. It never appears in source_path, which records the actual content array and
original index only. All copies keep their own source run, turn, and sequence. [TAP-4,
TAP-5, STORE-3, STORE-5]

When the declared context projection is present, copy its exact canonical JSON spelling
once into ContextBlock with source_path [Key("context")]. A missing member is
MissingProjection, never a context-free success. The raw value is not decoded as
MarkEffect, PlanEffect or PlanInputContext here. The loop makes the floor SourceContext
without changing bytes and licenses it only on source block zero. Ordinary statements
and PairCopy have no context. [TAP-4, MARK-5, SEAM-2]

**TRACE-PROJECT.** Admission compares only kind, channel, and origin against the
validated tuples. An unselected copy is discarded. On a selected tuple, iterate
`content.text[*]` in source order or inspect the original declared index. StatementCopy
projects only Text blocks, whole and without normalization. It omits ToolCall,
ToolResult, and UnrecoveredCall by structural tag. That projection rule never changes
envelope admission. An explicit index naming a non-Text block yields zero projected
blocks. Missing containers, missing indices, and malformed blocks are Failed. An empty
valid projection gives Selected with zero blocks and an explicit zero-block observation,
not Unselected. Reason: projection errors cannot masquerade as a partition decision.
[TAP-1, TAP-4, FLOW-4]

An admitted source array cannot contain overlapping projected indices. Validation
rejects wildcard-plus-index and duplicate selectors for the same container. The tee may
inspect the array structure to copy it, but its result cannot depend on text meaning.
Tool blocks remain whole on the trace but this registry does not offer them to the text
ledger. A pair projection instead copies the entire payload and produces PairCopy. It
cannot mix with a Text projection. The harness, not the tee, validates that copy against
Spec 05's pair codec before producing PairFile. A malformed licensed payload is a
component fault, never an unselected event. [TAP-1, PAIR-1, PAIR-3, FLOW-4]

**TRACE-VALIDATION.** Load refuses a partition selecting measurement payload,
save_point, message.restored, message.system, input.offered, ste100.result, recall,
state.placement, state.mark.position, state.sort.measurement, plan.break,
state.plan.measurement, message.tool_result or tool.return_rejected. It also requires
all loop.instruction/internal/loop Text, with no context projection, so every authorized
review send can be filed. Category.review_required chooses whether closure needs that
send under R20-17. Coverage supports both category modes because a later root plan
update can change the category without changing the partition. It requires all assistant
Text blocks and the complete pair projection for the enabled serving call paths. It
requires message.user/input/operator and message.user/input/external Text coverage, and
state.mark/internal/loop block-zero coverage with its context. It requires
plan.event/input/operator wildcard Text plus context, and plan.event/internal/loop and
step.outcome/internal/loop one-Text coverage plus context. Single-Text records must name
only block zero or the one-Text wildcard. An unsupported, missing or extraneous context
projection refuses invalid_projection at load. Required pair tuples are
tool.call.started/tool/model and tool.call.completed/tool/tool plus
tool.call.completed/tool/loop, and tool.call.started/tool/loop for Spec 07's available
review path. A no-review closure uses step.outcome and its typed null review_call, never
a fabricated pair. No payload or fate predicate is allowed. The validator proves
coverage over these finite envelope tuples before constructing ValidatedPartition. The
tee then applies only that declaration. Reason: an unfiled request or closure cannot
become an invisible partial pair. [TAP-1, TAP-2, TAP-3, PAIR-1, PAIR-3]

## 3. Submission and capacity, destinations sections 5, 6, and 9

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TRACE-SUBMIT.** Replace the serving Recorder result with `Result<SubmitAttempt,
Failure>`. SubmitAttempt is either `Recorded(Submission)` or `CapacityPending(Event)`.
Submission contains sequence and TapOutcome and is marked must_use. The existing Failure
means no newly admitted event on that call, or the existing standing sink failure. A tap
failure is inside Recorded, never an Err that suggests the source event is absent.
Reason: trace admission and copy success are separate facts. [FLOW-2, FLOW-4, FLOW-6]

The order is: validate envelope, check standing failure, reserve writer capacity, assign
sequence, render once, append working structure, apply the tap, and enqueue the same
canonical line. `load` returns HeadUntapped without invoking Tee. Render or append
failure releases the reservation and follows the existing trace failure path. No
sequence is consumed by CapacityPending. No record crosses the tap twice. [FLOW-2,
FLOW-3]

**TRACE-CAPACITY.** Replace Writer::enqueue's wait on a full queue with a nonblocking
reservation checked before admission. Keep QUEUE_DEPTH at 1024. Reserve seven entries
from normal submissions. Each reservation is available once per run for one terminal
record: fault, tool.call.completed, message.tool_result, turn.closed, session.closed,
save_point with phase refused, or unload. Absent calls, turns or captures leave their
reservations unused. Only the halt path receives terminal permits. Ordinary faults,
normal unload and normal save records use ordinary slots. The serving Recorder exposes
submit_terminal(Event) -> Result<Submission, Failure>. It refuses unless the tee is
stopped, the kind and phase match an unused terminal reservation, and the envelope is
valid. It cannot return CapacityPending because that reservation is already owned. No
caller can spend an ordinary event against it. Record's halt path is its only serving
caller, checked by review and tests. The first fault carries the outstanding ledger and
pending author ticket, so this account needs no additional state.flow event after the
latch. Suppress secondary fault records after the first and include their evidence in
the final unload account. A reserved entry still consumes a sequence only when admitted.
Reservation and queued-count change occur under the existing writer-state mutex. A
reservation counts toward occupancy until enqueued or cancelled. After a reservation,
enqueue cannot wait for capacity. The writer's later I/O remains on its existing writer
thread. Reason: the harness's poll owner must still observe gate and cut deadlines when
the trace queue is full. [FLOW-4, CLASS-8, GATE-1]

CapacityPending returns ownership of the unchanged Event. The harness retains it and
polls a capacity-wake descriptor while servicing its other work. No retained event is
submitted again after Recorded. Fatal-report authoring uses its reserved entry and
bounded shutdown. Exhausted trace delivery can prevent a report from reaching the sink
and must not prevent the failure latch from stopping decisions. No extra durable outbox
or guarantee of sink survival is introduced. [FLOW-4 through FLOW-6]

**TRACE-STOP.** `Tee::stop` is terminal and is public for the harness's serving record
integration. Review, not a visibility claim, confines its use to that path. Later
non-head submissions report Stopped and continue along the trace. They are shutdown
evidence, never a path for another model pass. Construct
`PreflightRecorder::receive(sink: OwnedFd, run: RunRef, session: SessionRef)`. It
creates the private Writer and admits only one load, followed by enter-refusal or fault
and terminal unload until activated. It exposes no content submission method. After load
is recorded and partition validation passes, `activate(self, partition:
ValidatedPartition) -> Recorder` consumes this preflight value and installs the single
Tee. It cannot be called twice or after a refused enter. The resulting Recorder has no
attach or reactivate method. Keep the existing receive constructor for diagnostic/legacy
paths. Replace the public constructor pin trace-receive-shape-pinned-by-doctest with
pins for diagnostic receive and the preflight/activate sequence. Replace attach_tee at
enter with this sequence. No attach, reopen, or clear-loss API exists during a run.
Remove tee_lost and its lifecycle.rs::take_save_point consumer, replaced by the flow
latch and save barrier. [FLOW-4, FLOW-5]

## 4. What is enforced, destination section 10

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Claim | Instrument and test | Protective change that must fail the test |
| --- | --- | --- |
| TRACE-ENVELOPE | Perturbation in tests/recorder.rs, canonical fixture with escaped content and omitted options | Infer origin from content or change sequence to a JSON number |
| TRACE-REJECTED-RETURN | Real recorder lines from exact multiline and escaped packets, capacity/cancellation resume and historical object/string reader fixtures | Embed RawValue, normalize the packet, discard before Recorded, rewrite history or turn the evidence into a selectable projection |
| TRACE-REGISTRY | Exhaustive enum match and tests/kinds.rs, plus table-driven turn/payload tests and harness-authored recall refusal bytes | Omit a new kind, alter the typed refusal bytes or admit a forbidden projection |
| TRACE-PLAN | Canonical operator/effect, failed/closed outcome and break fixtures, checked against the floor-to-trace conversion | Swap the two plan payloads, license close as plan.event, omit null context, change an identity or accept a wrong turn/channel/origin |
| Plan observation | Canonical fixtures for flags success/refusal, successor readiness and both capacity paths | Replace unavailable with zero, drop an identity, add an unknown variant or admit the measurement to residue |
| TRACE-VALIDATION | Load matrix covering every required call/task tuple and forbidden kind | Permit partial pair or mark coverage, select a boot read or allow a payload predicate |
| TRACE-COPY | Review of Tee fields and methods for no transport ownership, compile pin for result types | Reintroduce UnixStream or a store frame into Tee |
| TRACE-PROJECT | Tee tests with mixed Messages, mark sidecars and complete pair records | Treat a tool block as Text, change an index, omit a context/pair field, hide a bad path as Unselected or alter copied bytes |
| TRACE-SUBMIT | Perturbation with tap failure and working/sink observations | Return Err after tap failure or omit the canonical source record |
| TRACE-CAPACITY | Perturbation with 1017 ordinary slots full, a live tool, turn and save, and all seven terminal records | Reduce the terminal reserve, reuse one reservation, block enqueue, or consume a sequence on pending capacity |
| TRACE-STOP | Perturbation with a fault record after latch | Allow a second filing or erase the fault's trace record |

The compile pin on a return type is not a guarantee that a caller consumes it. That
behavior is enforced at Record::submit in the harness tests. The production crate's
no-internal-crate assertion remains. No graph-header maintenance is introduced.

## 5. Capacity wake and load evidence, destinations sections 3 and 6

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TRACE-WAKE.** The writer creates a nonblocking UnixStream pair solely for capacity
notification. Recorder exposes the read end as a borrowed descriptor and a drain method,
never the stream writer. The writer thread sends one byte on a transition from no
ordinary capacity to available capacity, or on standing writer failure. WouldBlock means
a wake is already pending. A broken notifier is a trace failure. The harness drains
notifications, attempts its reservation again, then polls. A notification arriving
between that attempt and poll remains readable. This replaces the condvar-only wake at
writer.rs without adding a harness thread. Reason: trace capacity must wake the same
wait that enforces CLASS-8. [FLOW-4, CLASS-8]

**TRACE-LOAD.** `FlowDeclarationRecord` is serialize-only trace-owned data with
registry_id, version, partition, gate_wait, cut_wait, observation_interval, backlog,
scheme, persistence, reads, ste100, tools and placement. These configuration members
carry the types amendment's fields as explicit trace-owned representations. Each member
has the types amendment's wire spelling. The harness converts each field explicitly. The
load also carries clock_source with the literal CLOCK_MONOTONIC and origin_ns as a
decimal-string count. All envelope monotonic_ns values are elapsed from that one origin.
Measurement records that carry raw component clock readings retain those readings and
their declared clock source, under Spec 09's CLOCK. load.state_member_observation is the
explicit trace-owned mirror of Spec 09's MemberObservationIdentity, preserving its u32
PID, decimal start/tick fields and unavailable reason. It names the current member and
instrument, never an identity restored from the dump. [COST-4, COST-5]
`StateStartRecord` carries seed_id, artifact_id, dump_sha256, config_sha256, and
parent_artifact_id and position, each null where absent in the verified start. The
load's old tee-election member is removed for serving. The load also records
template_inventory: {scheme_id, inventory_sha256, checker_artifact_sha256, templates}.
templates is the ordered Vec<TemplateSpec> defined by Spec 02's TEMPLATES, including
every literal and typed slot. The load also holds ste100_scheme, the exact scheme bytes,
and ste100_binary_sha256 among its executable facts. It records the inputs to
certification, not an unperformed pass. Template checks run after load and emit turnless
ste100.result records in inventory order. They carry the exact replies used by
TEMPLATE-CHECK. No organ fork or template check is moved ahead of the trace bracket. A
partition refusal is recorded after load by the preflight recorder, before any
activation. Diagnostic load retains its old fields and has no state_flow member. Reason:
the trace must name the declaration and proof used, without linking the trace crate to
the floor wire crate. [TAP-2, FLOW-3, PERSIST-2, review S2]

Spec 09 RUN-EVIDENCE binds external build and host observations to this exact load and
captured trace. It is an analysis artifact, not a payload kind, load extension or
selectable event. Preserve stack and ste100_binary_sha256 exactly. Do not synthesize
source revisions from binary names or fill host observations from the declared device
assignment. The existing model.request and model.measurement remain turn-bound and keep
the SPU's later facts in their original position. The load has no account of an
admission that has not happened. [FLOW-2, FLOW-3, COST-5, SCOPE-1]

**TRACE-INSTRUCTION.** Payload::LoopInstruction mirrors Spec 02's single
LoopInstructionRecord and closed metadata enum. The trace does not import a wire crate
to obtain those types. The harness converts every field explicitly, preserving its typed
source IDs, DecimalU64 strings, WorkRef and review digest. Kind licensing checks the
metadata variant and its turn rule before submit. The tee projects only the declared
Message Text, and neither parses purpose to decide admission nor creates a second filing
from metadata. [FLOW-8, TAP-1, PLAN-2, CLASS-2]

**TRACE-PLACEMENT.** PlacementRecord has the exact fields in Spec 06's PLACEMENT-RECORD,
with UnitMapEntry from its PLACE-PORT. The trace owns explicit serialize-only mirrors of
these types. phase is boot, cut or append, with null context for boot/cut and
PlacementContext for append. Its label and instruction reference mirror Spec 06, without
importing harness types. method is null for boot and CutMethod otherwise. assembly_id,
wait_ns, recomputed_messages, Message IDs and occurrence/map indices are decimal
strings. assistant_message_id is null for boot and cut. N, prefix_tokens,
resident_before, input_tokens, capacity, generation_limit and terminal_tokens are u64
JSON numbers, matching their SPU facts. cut is null or the typed CutReference. The
record is channel internal, origin loop. It holds no selectable Text or context surface.
Assembly zero is the single boot placement, causally linked to the boot recall. [FLOW-3,
PLACE-5, MARK-6, COST-4]

open_evidence is a required final field. It is null for cut and append, and for boot it
is the serialize-only trace-owned OpenEvidenceRecord
{request:Box<RawValue>,measurement:Box<RawValue>}. Splice both SPU-OPEN boxes without
normalization. The trace owns this closed outer wrapper and requires both boxes
to be JSON objects. It does not parse their members, token IDs, model identity or
partition. HARNESS-OPEN checks those contents, including duplicate/unknown keys,
token count and the whole-render interval, before authoring. This preserves the
base trace Spec's custody: the organ owns the contents of its opaque boxes.
[FLOW-3, PLACE-5, SEAM-2]

Boot requires prefix_tokens no greater than capacity, terminal_tokens one,
assembly zero, no turn, cut null, and a causal_parent naming the earlier boot recall.
Trace validates the event envelope, boot/nonboot wrapper presence and local outer
placement fields. The harness proves the held Open, wire session echo, source port
and causal-source correspondence. The event envelope already supplies the trace
session; OpenEvidenceRecord has no additional session member. Serialize-only
authoring does not give the trace a decoder for the SPU's measurement vocabulary.
[FLOW-3, PLACE-5, SEAM-2, PROMPT-1]

The serving recorder has entry-count capacity, not a per-record byte ceiling.
DECODE_MESSAGE_BOUND limits the complete incoming Opened JSON message to 8 MiB;
it does not bound the larger state.placement line with its trace envelope. Qualify
the actual final encoded line, including all JSON escaping and its LF, and the
simultaneously retained evidence/line memory under Spec 09's tuple. Record limits
of external trace readers separately. No new recorder byte limit is elected by
this carrier. Encoding or sink errors keep their existing trace
failure disposition. Current infallible allocations may terminate the process; no
new allocation-failure recovery is elected. A failed submission grants no Recorded;
a later sink
failure does not erase an already assigned Recorded sequence. No truncation or
successful completion may be invented.
[FLOW-3, FLOW-4, COST-4, COST-5]

STATE_REQUEST_LINE_BOUND applies to state request frames and includes their LF.
It is not a limit on this trace record: state.placement has no legal projection,
so the active tee returns Unselected and emits no state filing for its evidence.
The trace's canonical envelope parsing and sink write still process the whole
record. Qualification therefore cannot infer trace-record capacity from the
member's 8 MiB request bound. [TAP-3, SEAM-3, COST-4]

The nested request/measurement are Open evidence, not model.request/model.measurement
events. Those generation kinds remain turn-bound. Boot placement can account for a
valid seated prefix whose configured fit failed, or whose enter was cancelled, without
claiming readiness. Its payload grants no projection, including a wildcard. Diagnostic
conversion refuses the serving placement instead of dropping the new field or creating
a diagnostic event. Serving readers must accept this required shape at the paired
cutover, including null open_evidence on nonboot placements. [FLOW-3, TAP-3, SCOPE-1]

HARNESS-OPEN's owed teardown account uses normal submission capacity with the tee
stopped when required. It gains no submit_terminal license and cannot spend any of
TRACE-CAPACITY's seven reserved entries. Missing ordinary capacity preserves the held
event only within the existing bound. No additional queue reserve is elected here.
[FLOW-3, FLOW-4, FLOW-5, COST-4]

Payload::MarkPosition mirrors Spec 06's MARK-POSITION, including the distinct
MarkOccurrenceId, ArgumentCursor and boot/placed/native source variants. The harness
authors state.mark.position only after its licensing receipt or complete append. The
payload contains no projection surface. The loader refuses any attempt to select it,
including a wildcard. Diagnostic conversion refuses this new serving kind. Neither a
placement proposal nor a preview is a completed mark occurrence. [FLOW-3, FLOW-8, TAP-3,
MARK-6]

**TRACE-SORT.** Payload::SortDecision and Payload::SortMeasurement mirror Spec 08's
SORT-RECORD exactly, including the closed serve/outbound variants, MaterialRef/UnitId
identities and typed refusal/drop reasons. They import no floor or harness type. The
harness converts every field and validates the loaded criterion and template identity
before authoring. sort.decision remains selectable only as its one Text block.
state.sort.measurement is forbidden by the loader, including wildcard attempts to select
it. Neither carries a context projection or store effect. Both are new events when the
choice or measurement is made. [SORT-4, TAP-1, TAP-3, FLOW-8, COST-4]

For an observations read, the existing recall record carries the exact typed selector
and snapshot_id with empty sources and spans. The query names operation metadata, not a
content provenance list. Its response observation still produces the ordinary
state.store.measurement and remains excluded by the partition. [FLOW-3, SEAM-3, TAP-3,
R20-19]

**TRACE-OBSERVATION.** Add Kind::StateStoreMeasurement and
Payload::StateStoreMeasurement with explicit serialize-only mirrors of Spec 09's
StateStoreMeasurement and all nested observation types. The harness converts each field
without linking the trace to weaver-types. Channel is measurement, origin runtime,
subsystem harness. A filing measurement retains its source turn. A control measurement
uses the current turn when one exists. The record has no selectable surface, and the
loader rejects every projection of it. Diagnostic conversion refuses this new serving
kind. [FLOW-3, TAP-3, COST-2]

FlowObservation mirrors HARNESS-ACCOUNT, including phases sampled, backlog_wait and
backlog_resumed and the exact FLOW-COUNTS gauges, interval deltas and raw timestamps.
retained_blocks and retained_bytes and pending_turns are decimal strings. lag_alarm and
backlog_waiting are Booleans, and waiting_copy is the optional CopyAtHalt shape. These
are retention gauges and an alarm, never a new halt reason. The halt account's
flow_sample uses the same representation, not another counter grammar. The serving trace
reader must recognize both carriers without counting the final sample twice. No
measurement triggers another measurement or filing. [FLOW-5, CLASS-6, COST-4, FALS-5]

The trace corpus checks every new kind/envelope/payload association, decimal encoding,
unavailable value and turn rule. A wildcard partition attempting to select
state.store.measurement must refuse load. A cross-crate conversion fixture compares the
received observation with the serialized trace object field for field. Deleting a
counter, converting a raw clock to elapsed twice or admitting the payload must fail its
check. This proves faithful record carriage, not OS attribution. [FLOW-3, TAP-3, COST-5]

**TRACE-RESULT.** Ste100ResultRecord is the trace-owned payload for `ste100.result`. It
carries request_id, target, scheme_id, and result. Target and result use
TYPES-CLASSIFIER's exact representations, converted into serialize-only trace types.
Accepted results carry every binding, noun, condition, and span. Rejected results carry
the defect. Component failures carry their code. Runtime results are authored before
they change the filing ledger. Enter-time template results use the same kind with
template targets and no turn, after load and before boot. A record that cannot be
authored cannot authorize filing. This is evidence for rebuild, never another state
feed. The result is forbidden by the partition validator. [SORT-4, FLOW-3, STORE-3, apex
5.5]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| TRACE-WAKE | Saturate the real writer behind a held sink, release one slot, poll wake descriptor | Remove writer notification or reserve after sequence assignment |
| TRACE-LOAD | Canonical load, failed-partition, failed-template and diagnostic fixtures | Author before load, omit inventory identity or starting hash, or claim a certificate before its results |
| TRACE-PLACEMENT | Shared Open evidence corpus, actual event admission, byte-preserving serialization, boot/nonboot and fit/cancelled boot cases | Accept absent/wrong-phase evidence, malformed boxes, count/range/session mismatch, a boot turn or missing causal parent, normalize boxes or permit a projection/diagnostic conversion |
| TRACE-RESULT | Rebuild bindings from source plus result records and compare committed rows | Remove one noun or condition from the trace payload |
| TRACE-SORT | Exact decision/measurement captures with optional and mandatory overflow | Hide a dropped unit, admit measurement content, truncate a return or mutate an earlier event |

Retire trace Spec 11 assertions trace-tee-no-turn-backpressure and
trace-tee-envelope-not-electable, and section 3's trace-load-carries-the-tee-election
assertion. Their replacements are TRACE-CAPACITY, TRACE-REGISTRY and TRACE-LOAD. Replace
trace PRD 3.1's numeric kind count with the closed TRACE-REGISTRY set, so an added
serving kind has one registry owner. Persistence extends existing payloads and adds no
further kind. [FLOW-2, FLOW-3, TAP-1 through TAP-5]

**TRACE-RELEASE-ABSENT.** At the foundation cutover, replace the base trace
Spec section 3's Release definition with:

```rust
pub struct Release {
    pub spu: Option<ReleaseOutcome>,
    pub member: Option<MemberRelease>,
}
```

Every new replacement serving Unload carries the outer `release`, on ordinary
leave, healthy refusal rollback and component failure alike. Omit its `spu` only
when the harness created no SPU child for this run. At the read, absence inside a
present `release` means actual noncreation, never a created child's confirmed or
unconfirmed release. Do not write null or a new enum spelling; explicit null is
invalid. Present values retain exactly `confirmed` and `unconfirmed`, according
to the existing sent Release/Released rule. A created SPU that dies during release
still has an Unconfirmed outcome and its fault precedes Unload.

Successful fork returning a child PID in the parent makes the child present for
this purpose, even if exec, Load or Open later fails or the child is already
reaped. A fork that creates no child does not stand an SPU. Derive absence from
actual process ownership, never a missing port, missing reply, cleared resource
slot or successful reap. Unknown or lost child custody is not absence.

The member retains its independent Closed/Unconfirmed/absent rule. If neither
organ stood, `release` is an empty object; if only a member stood, it contains
only the member outcome. Historical absence of the whole `release` continues to
mean the old record did not carry that evidence. Existing present SPU values
retain their meaning and need no rewrite, including old Unconfirmed values that
alone do not distinguish noncreation from a created but unconfirmed SPU.

This amendment takes effect only at the foundation cutover. Until then the base
trace Spec and its mandatory `Release.spu: ReleaseOutcome` govern the existing
serving writer. Replacement readers and writers adopt this representation
together before cutover; this clause does not activate a partial replacement.
[FLOW-3, FLOW-4, FLOW-5]

**TRACE-HALT.** Keep fault as the existing FaultReport raw payload. The harness supplies
HARNESS-HALT-ACCOUNT under the HALT-ACCOUNT-SIZE bound, one account per terminal record,
under case state_flow_failed, or the existing stream_write_failed case for trace
failure. Trace does not reconstruct an outstanding ledger from that account or feed any
of it to state. Extend UnloadClose with optional end_reason and state_flow_failure.
end_reason has the single new spelling component_failure and is absent on an ordinary
unload. state_flow_failure is {fault_sequence,account}, with fault_sequence an optional
Sequence encoded as a decimal string or null, and account Box<RawValue>. These two
UnloadClose members are either both absent or both present. Diagnostic conversion
refuses them under the serving-vocabulary rule. [FLOW-3 through FLOW-6]

The harness supplies the same first-failure snapshot as the fault and adds only its
secondary failures to account.secondary. fault_sequence names the successfully recorded
first fault in this run, or is null if it could not be recorded. Existing forced,
forced_by, cause and grant_surface keep their meanings. Release remains required
on every new Unload; optional `release.spu` follows TRACE-RELEASE-ABSENT and
HARNESS-RELEASE-ABSENT at the same foundation cutover. Its absence describes actual
noncreation of an SPU child, not missing failure evidence. An unload with
component_failure supplies no successful Left. A reader joins the fault sequence rather
than counting the repeated account as a second halt. The trace's raw account arm stays
opaque and adds no dependency on the harness. The evidence fixture compares the first
account with the unload, removes the fault reference, changes its first cause and
changes unknown commit to success in turn. Each alteration must fail its corresponding
assertion. [FLOW-3, FLOW-5, COST-5]

## 6. Persistence payloads, destination section 3

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TRACE-PERSIST.** TRACE-RECALL defines the serving read and boot representations of
the existing recall kind. Recall remains unselectable. Define trace::SavePointRecord
and Payload::SavePointRecord for the serving save_point, with
phase requested, confirmed, published, or refused, capture_id, optional artifact_id,
A/B, location, position and refusal code. A/B use field names dump_sha256 and
config_sha256. Location is the artifact's territory-relative name, never dump bytes.
Requested records precede capture. Confirmed means worker Commit authorization,
published means admin reported index completion. [PERSIST-1, PERSIST-2, PERSIST-6,
FLOW-3]

The load holds the starting identity once, in its top-level state_start member. Existing
lineage retains its existing trace-position fields when present and gains no duplicate
StartIdentity. state_start.artifact_id supplies location without an absolute path. The
new load tuple includes template_inventory and state_flow under TRACE-LOAD. Diagnostic
conversion refuses the new serving recall/save payload variants rather than silently
dropping them. Analysis's serving reader must update before deployment. [PERSIST-2,
SCOPE-1]

Unclean recovery preserves the existing load.reset object with prior_run and reason. The
harness converts admin's ResetReason to its existing wire spelling, including
no-clean-unload, and records the selected StateStartRecord on that same load. There is
no reset event kind, new Reset field or second copy of A/B. The recovery fixture must
pair the recorded prior run with the selected start, including explicit rollback to an
older indexed artifact. [FLOW-3, PERSIST-3, PERSIST-7]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| TRACE-PERSIST | Boot/save fixtures checked against actual published A/B | Omit seated bytes, equate confirmed with published, or put dump bytes on trace |

| save_point phase | Required fields besides phase, capture_id, window and tail_window | Omitted fields |
| --- | --- | --- |
| requested | cause | artifact_id, dump_sha256, config_sha256, location, position, refusal_code |
| confirmed | cause, artifact_id, dump_sha256, config_sha256, location, position | refusal_code |
| published | cause, artifact_id, dump_sha256, config_sha256, location, position | refusal_code |
| refused | cause, refusal_code | artifact_id, dump_sha256, config_sha256, location, position |

Position is explicitly null for a seed-only image. Cause uses TYPES-PERSIST's serving
cause values. A seed or rebind has no live save_point event and is accounted for by its
first load's StateStartRecord, which also carries the verified position. TRACE-LOAD's
FlowDeclarationRecord additionally carries persistence.save_wait under persistence, with
the same MeasuredWait spelling. [PERSIST-2, PERSIST-5, FLOW-3]

window and tail_window are explicit trace-owned mirrors of TYPES-SAVE-TIME's SaveWindow,
with decimal raw CLOCK_MONOTONIC timestamps. tail_window is null on demand. Each phase
preserves the same windows for its capture. An unload creates requested only when its
capture stage begins after the lower. Its earlier S4/S5 does not acquire a save deadline
by trace authoring. No dump bytes or captured content enter this record, and the whole
kind stays excluded from state. The corpus includes queued expiry and an unload whose
unbounded drain outlasts save_wait. Altering a window or moving its anchor must fail the
comparison with the lifecycle frame. [PERSIST-4, TAP-3, COST-4]

### 6.1. Serving recall representation

**TRACE-RECALL.** Add trace-owned serialize-only RecallRecord and
Payload::RecallRecord. RecallRecord is externally tagged read or boot, with these
fields in the declared order. Reuse the trace's existing ReadSelector, SelectionRead,
ContentRef, SpanRef, StatementRef, StepQueryRef and PlanRef types where those shapes are
needed. Add only missing trace-owned shapes, with no internal crate dependency. Preserve
the flat selector and snapshot_id fields below. Use TYPES-CANONICAL's compact typed
rendering and exact DecimalU64 strings. Reason: one complete provenance shape must cover
the specified read results and boot evidence. [FLOW-3, SEAM-3, STORE-3, PERSIST-6]

| Type or variant | Fields in serialization order |
| --- | --- |
| RecallRecord::Read | selector:ReadSelector, snapshot_id:DecimalU64, sources:Vec<RecallSource>, spans:Vec<RecallSpan> |
| RecallRecord::Boot | selector:ReadSelector, snapshot_id:DecimalU64, sources:Vec<RecallSource>, spans:Vec<RecallSpan>, message:Box<RawValue>, end_mark:RecallEndMark |
| RecallSource | source:ContentRef, binding_ordinals:Vec<DecimalU64> |
| RecallSpan | source:ContentRef, selected_span:RecallSelection |
| RecallSelection | span_ref:SpanRef, start:DecimalU64, end:DecimalU64 |
| RecallEndMark | span:SpanRef, end:DecimalU64 |

These are closed objects. Every field is present, including empty arrays. ReadSelector,
ContentRef, SpanRef and SourceBlockId retain the exact TYPES-READ and TYPES-FILE shapes,
including every selector variant and its required nullable fields. There is no generic
JSON selector or result arm. snapshot_id is the acquired positive read snapshot ID,
never a state request ID. message holds the complete canonical Message JSON and gains
no dependency on the crate defining Message. Only boot carries message and end_mark.
[SEAM-3, STORE-3, STORE-5, PERSIST-6]

Both variants license only kind recall, channel internal, origin runtime, subsystem
harness, with no turn. This retains the old recall turn rule. TRACE-VALIDATION still
refuses the whole kind, including the boot variant. Diagnostic conversion refuses both
new variants as OutsideDiagnosticVocabulary, without coercion into the old
RecallAccount. The legacy recall grammar is unchanged. Serving readers must recognize
the new read and boot wrappers before deployment. [FLOW-3, TAP-3, SCOPE-1]

**TRACE-RECALL-CHECK.** Before recording, require a positive snapshot_id and the
selector's context-free TYPES-READ constraints. Every mirrored identity must meet its
owning grammar, including nested selector references and every ContentRef and SpanRef.
Require unique ContentRefs in sources and strictly increasing, unique numeric
binding_ordinals in each source. Every
RecallSpan.source must occur in sources. Each selected interval must have start less
than end. Repeated equal RecallSpans remain permitted. Apply TRACE-RECALL-BOOT's exact
single-source, single-selection and Message/end agreement to Boot. A failed check takes
the existing invalid-authoring path and never becomes an empty successful recall.
Reason: representation errors must fail at the authoring seam that can observe them.
[FLOW-3, STORE-3, STORE-5, MARK-3, SEAM-3]

The harness has the checked QueryResult needed to compare returned binding ordinals,
UTF-8 boundaries, interval limits and original content bytes. It performs those checks
before conversion under HARNESS-MATERIAL and HARNESS-RECALL. The trace checks the
record's structure and its boot Message, not whether a source exists in state. It never
queries the store or reruns selection to validate recall. [FLOW-3, STORE-3, MARK-3,
SEAM-3]

**TRACE-RECALL-SOURCES.** Build sources in the traversal order below. Visiting an
EventSource contributes event:{block_id}, and visiting a SeedSource contributes
seed:{seed_id,ordinal}. An explicit SourceBlockId contributes event:{block_id} with no
binding ordinals. A StatementRef contributes its event block with its original binding
ordinal. Deduplicate sources by exact ContentRef in first-encounter order. Union the
binding ordinals for each source in increasing numeric order. An empty ordinal list
means the read cited a source without selecting a binding. It never means binding zero.
The source identity is not a newly authored event. [STORE-3, STORE-5, SEAM-3]

Visit a ReadEntry's source with every returned ReadBinding.ordinal, then visit its
step_query.instruction when nonnull, then each attached ReadPair in returned order.
Visit a ReadPair's statement first, opening_source.source second, and nonnull
terminal_source.source last. The latter two cite the complete opening and terminal pair
blocks. They have empty binding ordinals and no byte selection. Do not parse a return
as another statement or substitute the request's identity for a pair block's identity.
A metadata citation adds no binding unless the table names a StatementRef. [STORE-2,
STORE-3, STORE-5, PAIR-1, PAIR-3, PAIR-4, MARK-3]

For each visited ReadEntry with nonnull selected_span, append one RecallSpan containing
that entry's ContentRef and exact selected_span. Append in entry occurrence order and
preserve repeated equal occurrences. No pair block, StepQueryRef or metadata reference
adds a RecallSpan. Sources answer which original blocks and bindings were returned.
Spans answer which selected intervals occurred, including repeated declared prefixes.
Neither array contains recalled text or grants placement authority. A binding's
statement, verb, noun and condition byte spans remain recoverable through its original
ordinal and the source's traced ste100.result, or the starting artifact for a seed.
They are not copied into RecallSpan, which records selected_span occurrences only.
[STORE-3, MARK-3, MARK-6, SEAM-3]

| Selector | Checked QueryResult and exact traversal |
| --- | --- |
| history, subjects, statement, condition, span, system_prompt, conversation | entries, visit entries in returned order |
| span_preview | span_preview, visit entries in returned order, preserve the selector's exact span and through |
| step_successor | step_successor, visit instruction once, then reference.instruction as a StatementRef, which deduplicates against the checked instruction |
| open_pairs | open_pairs, visit pairs in returned order |
| marks | marks, visit open_scopes in returned root-to-leaf order, each opened_source then ended_source when nonnull, spans empty |
| plan_current | plan_current, no visits for current null, otherwise the PlanSnapshot walk below, spans empty |
| plan | plan, the PlanSnapshot walk below, spans empty |
| plan_flags | plan_flags, visit after then through.block_id, spans empty |
| observations | observations, no content-source visits, sources and spans empty |

For PlanSnapshot, visit operator_source, nonnull lock_source, then
last_source.block_id. Visit steps in their returned ordinal order. For each step,
visit instruction as a StatementRef, nonnull work.through.block_id, then nonnull
closure_source. Finally, if broken is nonnull, visit its nonnull proposed_source and
then source. This is the entire walk. Do not recursively traverse task, review,
categories, group_drafts, groups, PlanRef, StepId, CallId, SpanRef or digest fields.
They are metadata identities, not returned content. The marks walk likewise does not
turn current_task, parent or replacement addresses into selected spans. Reason:
metadata reads cite their recorded sources without pretending that marked content was
fetched. [PLAN-1, PLAN-2, PLAN-3, MARK-1, MARK-3, SEAM-3, FLOW-3]

A complete empty entries or open_pairs result produces both arrays empty. An empty
metadata result follows its row, so plan_current with current null also produces empty
arrays. A refused or failed read produces no RecallRecord. It follows the existing
refusal or fault account, including a refusal after an acquired page. A partial result
cannot be recorded as a smaller success. Reason: an empty answer and no answer have
different meanings at the decision. [READ-1, SEAM-3, FLOW-3, FLOW-4]

**TRACE-RECALL-BOOT.** Boot uses selector system_prompt and the first checked content
read of the session. That read has exactly one seed entry, ordinal zero, whose seed_id
is the loaded state_start.seed_id. Its selected_span has that same seed, local_id zero,
revision one, start zero and end equal to the complete prompt's decoded UTF-8 byte
length. It has no step_query or pairs. sources has exactly that seed ContentRef with
its returned original binding ordinals. spans has exactly that full selected_span.
Reason: the prompt is held by the seed, not authored or filed again at load. [PROMPT-1,
PERSIST-5, PERSIST-6, STORE-8, MARK-1]

message is the canonical system-role Message with exactly one Text block, containing
the complete unchanged decoded prompt. end_mark.span equals that selected SpanRef, and
end_mark.end equals its end byte offset. This is the stored prompt boundary, not a token
count, N, argument position or later mark occurrence. The harness checks these fields
against the loaded seed identity and its completed read before authoring. The trace
checks their structural agreement and the Message shape before recording. It performs
no store read and adds no source event for the seed. Later explicit system_prompt reads
use Read, not Boot, and do not seat a second boot head. [PROMPT-1, PERSIST-6, MARK-1,
MARK-6, PLACE-1, FLOW-3]

A pass at trace authoring certifies the closed payload shape, canonical bytes, envelope
license and boot's internal field agreement. It does not certify store contents or
which history was useful. HARNESS-RECALL owns comparison with the completed read and
record-before-consumer ordering. No new event kind, state feed, binding or content
copy is introduced by the representation. [FLOW-3, TAP-3, SEAM-3, STORE-8]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| TRACE-RECALL | Exact typed byte fixtures for both variants and every selector, plus authoring and diagnostic licensing tests | Change a field spelling/order, encode a counter as a number, add an unconstrained payload arm, allow a turn or accept serving recall in diagnostic conversion |
| TRACE-RECALL-CHECK | Context-free selector and reference cases, duplicate sources and ordinals, missing source citations, reversed intervals and repeated-span positive cases | Accept zero snapshot_id, invalid identity grammar, a duplicate source or ordinal, an uncited span or an empty interval, or reject a repeated selected occurrence |
| TRACE-RECALL-SOURCES | Shared concrete read/result fixture corpus consumed by trace and harness tests, without a crate dependency | Renumber filtered ordinals, erase a repeated selected-span occurrence, cite a pair under its request identity or manufacture a span from metadata |
| TRACE-RECALL-BOOT | Seeded boot fixture with complete Message and stored end mark, plus malformed authoring cases | Omit prompt bytes, accept a non-system or extra-block Message, substitute an event for the seed or use a token count as the stored byte end |

## 7. Open elections and dependencies

No numeric queue size is changed. The new kind set must be accepted by serving trace
consumers before deployment. The A/B representation is in this unit's persistence
amendments. The load keeps its existing organ binary identities and declared elections
as well as the new fields. Actual model identity and effective parameters remain in
generation model.measurement/model.request and boot placement's Open evidence. Tool
metadata and any later selected event kind must enter the registry through its owning
amendment before it can be configured. These are dependencies, not permission for an
implementer to guess payload shapes.

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
