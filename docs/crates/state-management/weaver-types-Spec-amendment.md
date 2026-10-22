# weaver-types Spec: dependable-flow amendment

**Status:** MERGED.

**Landing PR:** #235

**Version:** 0.25 **Date filed:** 2026-10-21 **Parent:** weaver-types-PRD

## 0. What this document is

Amend sections 2 and 4 and add the instruments below to section 5. The baseline has
AgentConfig.state_election, StateElection, and EnterPayload.state_election. It has no
implemented `[state-management]` parser despite the apex declaring that table. This
amendment implements that table's flow portion. It does not add a third config reader or
change Message. [TAP-1, TAP-2, SEAM-2]

## 1. Declaration representation, destination section 2

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TYPES-SCALARS.** Add state_flow::Digest as a validated String newtype. JSON is one
string of exactly 64 lowercase hexadecimal characters. Its constructor and strict
deserializer reject all other spellings. MeasuredWait is
{elapsed_ns:DecimalU64,measurement_ref:String}, with positive elapsed_ns and a nonempty
reference. MeasuredInterval has the same fields and validation, but does not grant a
deadline. No consumer defines a second digest or wait representation. Reason: every port
must interpret identities and measured quantities alike. [PERSIST-2, CLASS-8, COST-4]

**TYPES-CANONICAL.** Canonical JSON throughout this set is the shared types' compact
serde_json rendering under [trace Spec section 2][canonical]: one UTF-8 line, fields in
their declared order, no insignificant whitespace, and integers that can exceed the
double-safe range as decimal strings. Hashes and byte comparisons use those object
bytes without a terminating record separator. Preserve string content and serde_json's
escaping, and serialize typed values directly rather than reordering them through a
map. Externally tagged UnitId puts its variant tag first, so its byte comparison orders
that tag before its declared payload fields. Reason: hashes and selection ties need
one byte rule across the shared representations. [STORE-3, SORT-3, MARK-5, SEAM-2]

**TYPES-CONFIG.** In config.rs replace the serving StateElection with
`StateManagementConfig { flow: FlowConfig, start: StateStartConfig, persistence:
PersistenceConfig, reads: ReadLimits, ste100: Ste100Instruction, tools: ToolPolicies,
placement: PlacementConfig }`. FlowConfig holds `version`, `partition`, `gate_wait`,
`cut_wait`, `observation_interval`, `backlog`, and `scheme`. MeasuredInterval has the
same positive elapsed_ns and measurement_ref representation as MeasuredWait, but grants
no timeout. Spec 09's FLOW-COUNTS owns its sampling use. SchemeDeclaration holds
scheme_id, artifact_sha256, and verbs, an ordered unique list of nonempty closed verb
keys. A changed dictionary changes artifact_sha256 and scheme_id. scheme_id is `ste100:`
followed by artifact_sha256, under Spec 02's SCHEME clause. Admin checks syntax and the
exact scheme bytes, and the classifier checks its loaded artifact. Use the existing
kebab-case TOML naming and unknown-field refusal. Later strategy keys are added by their
owning amendment under the same state-management parent. Reason: the declaration is
loaded once and records operator elections under the table already chartered. [TAP-2,
CLASS-8, GATE-1]

```toml
[state-management.flow]
version = 1

[[state-management.flow.partition]]
id = "user-content"
kind = "message.user"
channels = ["input"]
origins = ["operator", "external"]
blocks = ["content.text[*]"]
```

This is a projection example, not a loadable deployment. Both bounds and the other
required deployment elections are absent. The complete key grammar is listed below. No
amount is invented for the example. Each bound is a `MeasuredWait` with `elapsed-ns` as
a nonzero decimal string and `measurement-ref` as a nonempty string. Parsing proves the
quantity fits u64 and the representation is valid. The reference is operator
attestation, not proof that a benchmark was run. Qualification checks its evidence.
[CLASS-8, GATE-1, FALS-5]

| Required table or key | Representation |
| --- | --- |
| state-management.flow.version | Literal integer 1 |
| state-management.flow.partition | Ordered array of PartitionRule, empty allowed |
| state-management.flow.gate-wait | elapsed-ns decimal string, measurement-ref string |
| state-management.flow.cut-wait | elapsed-ns decimal string, measurement-ref string |
| state-management.flow.observation-interval | elapsed-ns decimal string, measurement-ref string, a cadence rather than a wait bound |
| state-management.flow.backlog | max-pending-blocks and max-pending-bytes positive decimal strings, measurement-ref nonempty string, lag-alarm-turns positive decimal string |
| state-management.flow.scheme | scheme-id string, artifact-sha256 lowercase 64-hex string, verbs array of strings |
| state-management.start | dump-sha256 lowercase 64-hex string in a published copy |
| state-management.persistence.save-wait | elapsed-ns decimal string, measurement-ref string |
| state-management.persistence.snapshot-wait | elapsed-ns decimal string, measurement-ref string, initial explicit value 120000000000 ns under R20-6 |
| state-management.reads | max-result-bytes positive decimal string, measurement-ref string |
| state-management.ste100 | Ste100Instruction, with the seven fields below |
| state-management.tools.sh | ToolPolicy, required exactly when tool-set elects sh |
| state-management.placement | c-head positive u64, c-tail positive u64, measurement-ref nonempty string |

FlowBacklog is {max_pending_blocks:DecimalU64,max_pending_bytes:DecimalU64,
lag_alarm_turns:DecimalU64,measurement_ref:String}. Both maxima must fit usize, and
lag_alarm_turns must be positive. The parser refuses zero, overflow, missing keys and
unknown keys as ConfigInvalid. The wire uses snake_case, and the declaration uses the
table's kebab-case. HARNESS-BACKLOG owns the counters and intake behavior. Spec 09 owns
measurement and the alarm sample. No default or ordinary receipt-age timeout is
supplied. [FLOW-4, CLASS-6, CLASS-8, FALS-5]

**TYPES-SERVING.** state_flow owns Spec 08's SortCriterion, CandidateOrder, Rational,
ExternalDrop and selection metadata shapes. They are per-call values, not a
state-management.serving declaration or a strategy enum. CandidateOrder is provided,
recency or current_step_then_recency with its exact condition. Keep
OutboundCriterion::Identity as the outbound criterion defined by Spec 08. The harness
owns the private SelectedUnit and PreparedPrefix constructors. [SORT-1, SORT-3, PLACE-4,
R20-12]

ReviewSetupRef and plan operation types are Spec 07's shapes. The root plan input
carries the setup identity. No PlanConfig or ReviewSetup script is loaded by the
state-management parser. The primitive does not interpret an external procedure.
[PLAN-2, PLAN-6, R20-12]

Ste100Instruction is the shared representation of Spec 02's DECLARATION, with
model_binding:ModelBinding, model_artifact_sha256:Digest, instances:u32,
max_wire_bytes:DecimalU64, measurement_ref:String, repair_cap:RepairCap and
decision_wait:MeasuredWait. RepairCap is {attempts:u32,measurement_ref:String}. Require
positive instances, attempts and max_wire_bytes, nonempty references and the existing
ModelBinding judgment. max_wire_bytes must fit usize. Digest is the validated SHA-256
representation defined above. The TOML form is kebab-case, the wire form snake_case.
There is one decision_wait, under ste100, not another flow key. [CLASS-2, CLASS-6,
CLASS-8, CLASS-9]

Within Ste100Instruction, model_binding.artifact must be an absolute pathname for
Spec 02's exact manifest file under MANIFEST-LOCATOR. Keep ModelBinding and
ArtifactRef's shared shape and existing syntax judgment, with this duty's added
absolute-path requirement. Filesystem resolution and named-file digest checks belong
to SPU admission. Neither the declaration codec nor the wire rewrites this locator
to a directory. Decoder and legacy ClassifyInstruction bindings are unchanged.
[CLASS-2, CLASS-7, SEAM-2]

ToolPolicies is a closed map with sh as its only possible key. ToolPolicy has
clock_ms:u64 and measurement_ref:String. Require a nonempty reference and a clock in
1..=SHELL_MAX_CLOCK_MS. Move that existing 60000 ms ceiling into the shared tool
vocabulary. An unelected tool's clock, a missing elected clock or any other tool name
refuses ConfigInvalid. Retry policy is not loaded here. [PAIR-2, R20-12]

PersistenceConfig is {save_wait:MeasuredWait,snapshot_wait:MeasuredWait}. Both
declaration fields are required. Provisioning initially writes snapshot-wait.elapsed-ns
= "120000000000" and measurement-ref = "initial-merged-120s". That reference records
inherited behavior, not benchmark qualification. A missing value refuses rather than
silently defaulting. Loaded facts and TRACE-LOAD include the exact value. Qualification
replaces the reference and may adjust the value using PERSIST-EXPIRY's method.
[PERSIST-4, R20-6]

ReadLimits owns the reads table. max-result-bytes fits usize and u64 on the target. It
bounds one complete encoded QueryResult before paging, not a page or a partial
selection. STATE-PAGES specifies its refusal and measurement. [STORE-1, SEAM-3, READ-5,
COST-2]

PlacementConfig is {c_head:u64,c_tail:u64,measurement_ref:String} on the wire, with
kebab-case TOML members. The parser checks positivity, a nonempty reference and checked
c_head+c_tail arithmetic. After SPU Opened, the harness checks the complete budget
against the returned capacity, generation_limit and terminal_tokens under Spec 06's
CAPACITY. No parser guesses a compiled SPU limit from an absent tunable key. A failed
semantic fit refuses enter before user input. [PLACE-1, PLACE-3]

Each PartitionRule has the fields in the example and optional context. When present,
context is exactly the string "context", selecting that top-level metadata member.
TRACE-REGISTRY licenses this projection only for named statement kinds. Ordinary message
and pair rules refuse it. It is not another admission predicate. Kind and envelope
values are strings at this floor. The harness validates their vocabulary against
weaver-trace. Blocks accepts `content.text[*]`, `content.text[N]`,
`message.content.text[*]`, `message.content.text[N]`, or the complete `pair` projection.
pair is the sole projection on a pair rule and is licensed only for the two pair event
kinds in TRACE-REGISTRY. N is a canonical unsigned decimal index, with zero written `0`
and no leading zero on a nonzero value. No other dot path, predicate, or recursive
selector is part of this grammar. Reason: these paths address the Message shape and
preserve complete source blocks. [TAP-1, TAP-5, STORE-3]

At parse, refuse empty rule IDs, empty channel/origin/blocks lists, repeated strings,
invalid index syntax, overlapping projections, and duplicate IDs. At harness enter,
refuse unknown kinds or envelope values, unavailable projections, forbidden kinds, and
overlapping kind/channel/origin tuples. An empty partition list is permitted by the flow
mechanic. A complete deployment must separately satisfy the capability coverage rules of
its later amendments. Do not infer a hidden default partition. [TAP-1, TAP-2, TAP-3,
CAP-1]

For serving, state-election and state-flow are retired spellings and refuse with
ConfigInvalid. The serving declaration also refuses the old restore table. Its selection
moves to admin's restore-artifact key under ADMIN-SELECTION and is never part of the
bound declaration. state-store.engine = none refuses. Diagnostic configuration retains
its diagnostic election representation until that lane's separate amendment, requires no
serving flow bounds, and cannot start the serving coordinator. Admin enforces the
binding-kind distinction at inventory. [PERSIST-5, SCOPE-1, October 18 R8]

**TYPES-ENTER.** Remove the top-level EnterPayload.state_election. Add
`state_management: StateManagementConfig`, `start: StartIdentity` and `ste100_scheme:
String` to `EnterBinding::Serving`, and `state_election: StateElection` to
`EnterBinding::Diagnostic`. Keep the remaining binding fields. Remove
LoadFacts.state_election and add LoadFacts.state: LoadedStateFacts. LoadedStateFacts is
externally tagged Serving {state_management, start, ste100_scheme, ste100_binary_sha256}
or Diagnostic {state_election}, with the same types as EnterBinding. Keep state_store
and state_member as runtime facts, requiring sqlite and true for Serving.
LoadFacts.declaration is B, the loaded configuration digest. Admin's show answer
therefore reports the actual loaded declaration, not the mutable draft. ste100_scheme
holds the exact UTF-8 scheme file bytes read by admin. The worker gets Ste100Instruction
from state_management.ste100, not a duplicate instruction field. ste100_binary_sha256 is
the root-judged executable digest in the existing binary facts, copied into
LoadedStateFacts for show. It is not the model-manifest digest or scheme digest. Spec
02's CUSTODY governs all three identities. [CLASS-2, FLOW-3]

The harness converts StateManagementConfig.flow, .persistence, .reads, .ste100, .tools
and .placement into trace::FlowDeclarationRecord. Its verified start becomes
StateStartRecord. There is no floor enum or flatten alias that accepts both serving
spellings. [TAP-2, TAP-3, FLOW-3]

Enter retains wire.rs::MAX_ENVELOPE_BYTES for its entire serialized lifecycle envelope.
The new scheme field does not authorize fragmentation on that channel. The sender checks
the encoded length before sending. Both endpoints retain the existing over-bound
rejection. The corpus includes JSON escaping that makes an apparently short scheme
exceed the bound, plus exact-bound and one-byte-over envelopes. A guard that measures
only the scheme or the unescaped source must fail that corpus. [CLASS-8, FLOW-3,
PERSIST-6, SEAM-2]

Add `LifecycleRefusal::StateManagementInvalid { stage, code, rule_id, path, detail }`.
Optional rule_id and path are omitted. Stage/code combinations are closed:

| Stage | Permitted codes |
| --- | --- |
| partition | unknown_vocabulary, invalid_projection, overlap, forbidden_projection |
| template | template_rejected, template_failed, scheme_mismatch |
| open | open_failed, scheme_mismatch |
| serving | invalid_span |

Spec 02 DECLARATION maps checker admission scheme_mismatch to template/scheme_mismatch
and the other admission codes, including unsupported_artifact, to
template/template_failed. Preserve the checker code and exact detail in detail when the
complete encoded lifecycle refusal fits MAX_ENVELOPE_BYTES, under DECLARATION's bounded
mapping. This includes a failed reference already present in checker detail. The
optional path is not parsed from that prose. Include it only when independently
attributable under its owning contract. Qualification checks the complete encoded
admission-failure envelope, including its wrapper and JSON escaping, at the bound and
one byte over. Existing over-bound channel failure and after-load cleanup remain
unchanged. No truncation, new lifecycle code or restart of the original load deadline is
added. [CLASS-8, FLOW-3, PERSIST-6, SEAM-2]

Admin syntax failures, including invalid bounds, remain ConfigInvalid with field
attribution. There is no semantic invalid_bound code. EnterBinding::Serving keeps its
required gate_instruction. No gateless serving variant is introduced. Loop instructions
do not enter the external input check. Diagnostic binding requires neither serving
bound. Reason: enter owns semantic validation, while admin owns grammar. [TAP-3,
CLASS-8, GATE-1, review S2]

## 2. Filing types, destination section 4

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TYPES-FILE.** Add public shared types in `weaver-types::state_flow`, exported by the
crate root, with config parsing still behind the existing config feature. The module
owns both endpoints' request and response shapes. It links no interior crate. Reason:
the harness and member must use one representation across their seam. [SEAM-1, SEAM-2,
STORE-3, STORE-5]

| Type | Fields |
| --- | --- |
| DecimalU64 | Newtype over u64, serialized only as a JSON string of ASCII digits, zero as 0, otherwise no leading zero, no sign, whitespace, fraction, exponent, or overflow |
| SourceBlockId | run String, sequence DecimalU64, block DecimalU64 |
| SourceEnvelope | Existing trace envelope fields plus channel and origin, represented as strings for named vocabulary |
| SourcePathPart | Untagged JSON string for a key, decimal-string index object {index: DecimalU64} for an array index |
| Span | JSON object {start: DecimalU64, end: DecimalU64}, start inclusive and end exclusive |
| NounBinding | value String, span Span |
| Binding | statement_span Span, verb String, verb_span Span, condition Option<String>, condition_span Option<Span>, nouns Vec<NounBinding> |
| SourceContext | source_path Vec<SourcePathPart>, raw Box<RawValue> |
| StatementFile | block_id, source, partition_rule_id, source_path, admission, raw_block, text, scheme_id, bindings, context Option<SourceContext>, effect Option<StatementEffect> |
| CallId | run String, sequence DecimalU64, call DecimalU64 |
| StatementRef | block_id SourceBlockId, binding DecimalU64 |
| PairFile | block_id, source, partition_rule_id, source_path, admission, raw_block, scheme_id, pair |

PairFile.pair is externally tagged started:PairStarted or ended:PairEnded. The closed
payload types and outcome table are defined once in Spec 05's PAIR-OPEN and PAIR-END.
They are shared floor types, never stringly typed arbitrary JSON. PairFile has the same
source, scheme and admission types as StatementFile. Its block index is zero,
source_path is empty and raw_block is the whole canonical pair payload. It has neither
text nor bindings. The member compares the parsed pair with that raw payload before
committing. [PAIR-1, PAIR-3, STORE-2, SEAM-2]

SourceEnvelope retains the trace's field encodings and omission rules. Admission is the
literal `admitted`. raw_block uses Box<RawValue>, not a reparsed and reserialized Value.
In this slice it is a complete Text ContentBlock. Text must equal that block's decoded
text member exactly. The path ends at the block, not its text leaf. Spans refer to
decoded text, never JSON escape bytes. Condition and condition_span are both null or
both present. Each binding has at least one noun. [STORE-2, STORE-3, TAP-4]

**TYPES-EFFECT.** context and effect are required members, null on ordinary statements.
A licensed context is copied from source_path ["context"] without reserialization. It
carries references and mechanical fields, never duplicate message text. StatementEffect
is externally tagged mark:MarkEffect or plan:PlanEffect. Spec 07 defines PlanEffect and
PlanInputContext once. Operator plan.event/input/operator block zero carries
PlanInputContext and a null effect. Its other blocks carry nulls. Internal
plan.event/loop block zero carries the matching plan effect. step.outcome/internal/loop
block zero carries the matching close effect on success. On failed review, its copied
literal null context maps to null context and effect in StatementFile. A conformance
rejection grants none of these effects. MarkEffect, SpanText and TaskScopeRef have the
exact shapes in Spec 06's MEMBER, BOUNDARIES and MARK-FILE. Its open/close/retire
variants are internal operations of plan effects, not separately licensed file requests.
Only begin_task on state.mark/internal/loop, block zero, is licensed directly. Its
context is the exact MarkEffect and must compare equal to effect.mark after strict
decoding. No effect is inferred from statement wording. Outside the licensed operator
input, missing context, unexpected context or mismatched effect is invalid_effect.
Reason: the source copy accounts for the mechanical update without letting a rejected
block enter through metadata. [MARK-5, PLAN-2, SEAM-2, TAP-4]

A StatementCopy context belongs to block zero only. Other blocks, where a later licensed
kind permits them, carry null context and effect. The registry and its coverage check
must ensure that block zero is selected when context is required. This does not change
source ordinals or let a caller attach context from another event. [STORE-3, TAP-5,
MARK-5]

The source ID exists independently of selection. An unselected emission can therefore
receive a conformance check without becoming a filing obligation. A gate check uses
input_id until its accepted message has a trace source ID. The adapter can reuse its
result only for identical text under the same scheme, preserving the new source ID.
[CLASS-4, CLASS-9, FLOW-1]

**TYPES-WIRE.** The new serving channel opener and filing messages are UTF-8 JSON, one
line per frame, carried on the existing inherited UnixStream. Unknown fields, unknown
versions, duplicate JSON keys, noncanonical counters, and trailing nonspace bytes refuse
parsing. The wire version is state-flow/1. The schema is state-content/1, defined by the
complete store and read amendments in this unit. Pair, mark and plan tables join that
initial catalog before its first deployment. A smaller draft is not a supported runtime
catalog. After deployment a changed catalog needs a new version and its own
compatibility act. No unknown JSON extension field is silently held. [SEAM-1, SEAM-2,
STORE-5]

| Message | Required fields after protocol, session_id, run_id, request_id, op |
| --- | --- |
| open | schema_id, scheme_id, start: StartIdentity, read_limits: ReadLimits |
| opened | schema_id, scheme_id, start: StartIdentity, read_limits: ReadLimits, member_observation: MemberObservationIdentity, observation: StateOperationObservation |
| file | filing: StatementFile |
| file_pair | filing: PairFile |
| filed | block_id, observation: StateOperationObservation |
| refused | code, detail, block_id or null, observation: StateOperationObservation |
| control | frame, the typed control object in TYPES-READ or TYPES-PERSIST |
| control_answer | frame, the typed answer for that control, observation: StateOperationObservation |

All five common fields are required, with request_id a DecimalU64 starting at `1`. Open
uses request `1`. Each subsequent exchange increments by one, checked for exhaustion.
The control adapter strips the outer wrapper and uses the replacement serving control
parser in the store/read amendment. The diagnostic parser stays separate. Its answer is
wrapped with the same request identity. Snapshot continuations use one exchange ID.
Finished or refusal permits a fresh exchange, but the original ID remains valid for
the correlated timeout abort under TYPES-PERSIST until release or the next fresh
exchange. Abort consumes no fresh ID. The control field is not an arbitrary new
operation or a second writer. [SEAM-1, SEAM-3, FLOW-5]

Filing refusal codes are malformed_filing, wrong_run, duplicate_source,
unsupported_schema, scheme_mismatch, invalid_binding, invalid_effect, wrong_phase and
write_failed. file_pair additionally permits unknown_pair, already_terminal,
invalid_pair, statement_missing and duplicate_pair. Open refusal codes are
unsupported_schema, scheme_mismatch, invalid_read_limits and open_failed. They share the
refused envelope, with block_id null during open, and are parsed against the outstanding
operation. An invalid control returns control_refused as the store/read amendment
defines. It never uses oversized-answer silence in serving mode. A missing or malformed
answer is never accepted. DecimalU64 overflows and unknown IDs are protocol failure.
[FLOW-4, FLOW-5, SEAM-2]

These replace spec 01's `block` wrapper and nonexistent top-level payload paths. There
is no raw-return JSON invented on Message. A tool return uses PairFile, never
StatementFile. Mark effects carry typed metadata beside the statement. Plan effects must
join the same closed enum before deployment. No unsupported case may silently become
StatementFile. [PAIR-3, STORE-2, PLAN-2, MARK-5]

**TYPES-FAULT.** Add `FaultCase::StateFlowFailed`, serialized as state_flow_failed, to
wire.rs. FaultReport keeps its existing case and opaque RawValue account fields.
HARNESS-HALT-ACCOUNT owns the account's complete representation, including source
identities, wait and uncertain-commit evidence. The floor neither redefines that
representation nor turns it into another lifecycle answer. Add
`FaultCase::StateTemplateRejected`, serialized as state_template_rejected, for a correct
runtime rejection that exposes an invalid template certificate. The latter case reports
an operation failure and never sets the component halt latch. These extend the closed
fault-case vocabulary and must update its matching trace-charter fault table. Neither
case changes the existing label scorer's result vocabulary. Reason: a component failure
and a correct conformance rejection remain distinct observable facts. [FLOW-4, CLASS-8,
CLASS-9, review S2]

**TYPES-OBSERVATION.** state_flow defines StateOperationObservation, StateOperation,
OperationPhase, PhaseName, OperationResource, ResourceField, OperationDisk, DiskSample
and ObservationUnavailable exactly as Spec 09's STORE-OBSERVATION. Every object refuses
unknown or duplicate fields. Optional values use explicit null, and parent_id must be
null in this release. StateOperation and both reason and field enums use the closed
snake_case spellings there. These are response metadata, never a content source. Stored
metadata is returned only through the observations selector below under R20-19. [COST-2,
SEAM-2]

**TYPES-OBSERVATION-READ.** ReadSelector adds observations
{run:String,after:DecimalU64,through:Option<DecimalU64>}. QueryResult adds only for that
selector observations:{run:String,after:DecimalU64,through:DecimalU64,
member_observation:MemberObservationIdentity,records:Vec<StoredOperationObservation>}.
StoredOperationObservation is {request_source:Option<SourceBlockId>,
observation:StateOperationObservation}. All fields are required. run is a valid RunRef.
after may be zero, through is the fixed held frontier, and every record's operation_id
is strictly increasing in (after,through]. Empty records is valid. request_source is
nonnull exactly for file/file_pair. The harness checks these invariants and selector
echoes before exposing query data. The query's own response observation remains outside
this result, with its usual new operation ID. No content unit or automatic argument
material is implied by either form. Reason: retention and presentation have distinct
contracts. [SEAM-3, COST-2, R20-19]

Add MemberObservationIdentity with Spec 09's OBSERVATION-IDENTITY shape. opened requires
member_observation, and all other frames refuse that member. Its PID is a u32 JSON
number and its start/tick values are explicit nullable DecimalU64. A current opened
requires state-observation/2, the sampling definition in Spec 09 SNAPSHOT-SAMPLING. The
harness copies it into load, never StartIdentity. Historical observation_run identities
and observations query results permit /1 or /2 and retain the recorded value. The
shared validator separates historical identity validation from the current-open version
check. Unknown versions refuse both contexts. The store retains the run-keyed copy in
ordinary dumps under R20-19. Missing optional start facts keep their reason and do not
become an open refusal. [FLOW-3, COST-5, PERSIST-1]

The response parser requires observation on opened, filed, refused and control_answer,
including control_refused. Requests refuse it. The dispatcher supplies the expected
operation, request ID and next operation ID to validation. It checks interval and
availability rules before exposing the answer. Missing or invalid required metadata is a
malformed answer. An explicit unavailable counter is valid metadata and cannot become a
filing refusal. No new state refusal code is introduced for an observation the member
cannot form. That component failure closes the channel, preserving commit uncertainty.
[FLOW-4, FLOW-5, COST-5]

StateOperationObservation::MAX_ENCODED_LEN is the maximum encoded byte count over this
finite shape, including every permitted unavailable entry, phase and maximum DecimalU64.
The encoding fixture proves that bound and rejects duplicate phases or extra entries.
Response/page construction includes it in the existing complete frame limit. Adding a
field changes that bound and its fixture in the same act. The bound is codec arithmetic,
not a measured capacity election. [COST-2, SEAM-3]

The shared cross-crate corpus also checks the observations selector/result identity,
after/through bounds, unique ordered operation IDs, request_source nullability and exact
same-tag query association. Remove each check and require its negative case to fail. An
ordinary entries query must never decode an observations result. Current opened must
refuse /1 while an explicit historical observations result preserves /1. Both refuse an
unknown version. Remove each context check and require its case to fail. [SEAM-3,
COST-2, COST-5, R20-19]

The shared cross-crate corpus covers every StateOperation, every response kind, unknown
counters and each phase rule. It includes two valid snapshot continuations with one
request ID and different operation IDs. Removing a metadata requirement, allowing the
wrong operation or forgetting wrapper space must fail its fixture. This pass proves
shape and correlation, not the accuracy of a sampled counter or the absence of disk
wait. [SEAM-2, COST-5, FALS-3]

## 3. What is enforced, destination section 5

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Claim | Instrument and test | Protective change that must fail |
| --- | --- | --- |
| TYPES-CANONICAL | Golden typed bytes for field order, escaping, large counters and every UnitId variant, plus review of shared definitions | Reorder fields, render a large integer as a number, add separator bytes to a hash or use enum declaration order for ties |
| TYPES-CONFIG | Table-driven parser perturbation tests in config.rs | Accept a missing bound, legacy serving table, duplicate selector, or zero bound |
| TYPES-SERVING | Per-call criterion, ratio and snapshot-order corpus | Invent a strategy default or compare unrelated read ranks as chronology |
| TYPES-ENTER | Serialization fixtures for both binding kinds and exhaustive refusal mapping | Drop validation attribution or require serving bounds for diagnostic replay |
| TYPES-FILE | Shared round-trip corpus read by harness and state tests | Alter a UTF-8 offset, source ordinal, optional field rule, or raw block spelling |
| TYPES-EFFECT | Strict context/effect corpus and private-constructor compile tests | File an effect without its source context, admit effect on model text, or carry rejected text inside context |
| TYPES-WIRE | Decoder perturbation tests for each frame and refusal | Accept duplicate keys, wrong IDs, or unknown fields |
| TYPES-FAULT | Exhaustive fault serialization and harness disposition tests | Route state_template_rejected into the component halt |
| Dependency boundary | Existing manifest test | Add an interior crate as a production dependency |

## 4. Frame decoding and classifier targets, destination section 4

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TYPES-CODEC.** Use a custom MapAccess visitor for the outer frame. It records seen
keys in a set and deserializes each value directly into Box<RawValue>, not Value. After
the map closes it checks the exact common and op-specific key set, then parses each
field from its raw slice into the typed variant. No internally tagged or flattened enum
buffers RawValue. A recursive duplicate-key scanner runs over the original line before
typed parsing, including raw_block and control interiors. RawValue preserves raw
spelling after that scan. The resulting Rust enum is StateFrame with Open, Opened, File,
FilePair, Filed, Refused, Control and ControlAnswer variants, each holding Common plus
its tabled fields. It uses custom Serialize and Deserialize implementations. Reason: a
single parse contract must preserve source bytes and refuse ambiguity. [SEAM-2, TAP-4,
STORE-3]

**TYPES-LIMITS.** Retain the member's 8 MiB incoming line bound, including newline. The
harness measures the serialized frame before sending. A larger filing is frame_too_large
and halts as failed carry with the unsent block still owed. A peer that exceeds the
bound closes. Control answers retain the 1 MiB line bound, including the outer wrapper
and newline. Oversized control results are explicit operation refusals, never silence or
truncation. The read amendment supplies pagination. Reason: removing existing memory
guards requires separate evidence. [FLOW-4, COST-2, SEAM-2, SEAM-3]

**TYPES-CLASSIFIER.** `Ste100Target` is an externally tagged enum with JSON forms
`{"source": SourceBlockId}`, `{"input": {"input_id": DecimalU64, "block": DecimalU64}}`,
or `{"template": {"template_id": String}}`. `Ste100Request` has request_id, target,
text, scheme_id. `Ste100Reply` has request_id, target, scheme_id, and result. Result is
externally tagged accepted with bindings, not_ste100 with {code, detail, spans}, or
component_failed with {code, detail}. The accepted payload is {bindings: Vec<Binding>}.
Template targets require an empty binding array and use canonical TemplateSpec JSON as
request.text under Spec 02's TEMPLATES. Source and input targets require nonempty
bindings over their unchanged Text. A target never changes between request and reply.
The closed defect code is non_ste100. Component codes are crashed, no_result,
invalid_result, transport_failed. These types live in state_flow and are independent of
the legacy label scorer. [CLASS-2, CLASS-4, CLASS-8, October 18 R10]

**TYPES-PAIRS.** Define PairLane as external, recall or review. PairRequest is
externally tagged external:{name:String,arguments:String}, recall:{subject:String}, or
review:{setup:ReviewSetupRef,work:WorkRef,command:String}. PairFailure is externally
tagged timeout:{}, cancelled:{}, unload:{}, flow_failure:{}, gate_fault:{},
not_granted:{}, not_dispatched:{cause:NonDispatchCause}, or abandoned_at_load:{}.
NonDispatchCause is pass_purpose, repair_exhausted, plan_break, round_limit, stop,
unload or no_gate. lane/request equality and the terminal rules are Spec 05's guards.
Both endpoints use these floor definitions. [PAIR-1, PAIR-2, PAIR-3, STORE-5]

**TYPES-TEMPLATE.** LoopInstructionMeta is the externally tagged enum whose exact
variants and fields are Spec 02 INSTRUCTION-METADATA's table. It lives in state_flow,
with no untyped metadata member. TemplateCertificate is harness-only, private
{run_id:String,scheme_id:String,checker_artifact_sha256:Digest,
inventory_sha256:Digest,accepted:Vec<TemplateAcceptance>}. TemplateAcceptance is
{template_id:String,request_id:DecimalU64,result_source:SourcePosition} in inventory
order. Only the completed load-template check constructs it. It serializes no runtime
authority onto a socket. VerifiedSeed is a private floor validator result
{prompt_sha256:Digest,scheme_id:String,checker_artifact_sha256:Digest,
bindings:Vec<Binding>}. Only verify_seed_certificate constructs it after all TYPES-SEED
checks. SeedCertificate is the public exact TYPES-PERSIST struct, not a synonym for
either private grant. [CLASS-2, FLOW-8, PERSIST-5, PROMPT-2]

Finish is the existing weaver_types::Finish in wire.rs, with Completed, Stopped and
Length. No new Finish enum or conversion by spelling is introduced for
EffectiveEmission. The trace retains its own mirror through the existing explicit
converter. [FLOW-3, CLASS-9]

**TYPES-STE100-ADMISSION.** Add the three shared state_flow structs for Spec 02's
CHANNEL. Ste100Bootstrap is {protocol:String,scheme_text:String,
instruction:Ste100Instruction}. Ste100Ready is {protocol:String,scheme_id:String,
artifact_sha256:Digest,checker_artifact_sha256:Digest,instances:u32,
max_wire_bytes:DecimalU64}. Ste100AdmissionRefused is
{protocol:String,code:Ste100AdmissionCode,detail:String}. protocol is always
state-ste100/1. Ste100AdmissionCode is scheme_mismatch, unsupported_artifact,
invalid_grammar, insufficient_capacity or invalid_instruction. The wire structs have no
extra kind field. Spec 02's fragment tag chooses the exact struct. Unknown and duplicate
fields refuse at every depth. [CLASS-2, CLASS-8, SEAM-2]

Ready's scheme and artifact are the exact scheme identity, while checker_artifact_sha256
is the admitted model-manifest digest. instances and max_wire_bytes must equal the
loaded instruction. A complete admission refusal is an enter result, never a
content-language defect or a ready state. Export the same structs to both endpoints and
prohibit a local shadow definition. The strict-codec corpus checks all three tags,
identity echoes, missing/extra fields and a legacy label-ready body. Removing any echo
or strict-key check must fail its fixture. This is representation evidence, not artifact
qualification. [CLASS-2, CLASS-7, CLASS-8, PERSIST-6]

**TYPES-INPUT.** Add RefusalRecord::Input with InputRefusal {input_id, case, detail},
where case is non_ste100 or invalid_input_shape. The JSON tagged refusal names seam
input. Also add RefusalRecord::ProtocolRepair {source_id:SourceBlockId,case,detail},
with seam protocol_repair and case correction_call, empty_correction,
missing_tool_statement or ambiguous_tool_statement. Both use the existing trace kind
refusal. A correct protocol refusal grants neither filing nor component halt.
[CLASS-9, CLASS-10]

source_id names evidence according to the case. For correction_call, it is the
correction generation's SourcePosition.block_id, block zero of the recorded
message.assistant. That block exists because the pass attempted a call. For
empty_correction, it is the current rejected original Text source. The empty
correction has no block, and no block is invented for it.
[CLASS-9, CLASS-10]

For missing_tool_statement and ambiguous_tool_statement at initial association,
source_id names the recovered ToolCall block. For ambiguous_tool_statement after
repair, it names the original rejected Text whose correction cannot supply the held
external call's one-statement association. These IDs identify refusal evidence, not
an accepted Binding or permission to dispatch. [CLASS-9, CLASS-10, PAIR-1]

Harness-to-trace fixtures cover each case and both ambiguous_tool_statement contexts.
Changing the required source_id to another generation or block must fail those
fixtures. [CLASS-9, CLASS-10, FLOW-3]

ProtocolRepairCase::EmptyCorrection serializes as empty_correction. Spec 02's
EMPTY-CORRECTION owns its meaning, source attribution and exact detail under the
[operator's ruling of 2026-10-21][empty-correction-ruling]. The trace carries that
existing refusal payload without a new event kind or a fabricated content block.
The strict-codec fixture requires this case to preserve source_id and the exact detail
defined there. The harness-to-trace fixture requires that same detail and requires the
refusal envelope to keep the same open turn and set causal_parent to the empty
correction's message.assistant sequence. Both fixtures require unknown cases to refuse.
Removing the case or changing its detail must fail both fixtures. Changing source_id
must fail the strict-codec fixture. Changing the turn or causal_parent must fail the
harness-to-trace fixture. [CLASS-9, CLASS-10, FLOW-3]

ToolInterrupt becomes `ToolInterrupt { by: KillCause }`, allowing only unload or
flow_failure on this command. Add KillCause::FlowFailure with wire spelling
flow_failure. It means the live gate contained the call because the serving flow failed.
Keep Fault's existing meaning: the gate died before answering, so the harness reports an
indeterminate outcome. Fault, Clock and Cancel are not accepted interrupt-command
causes. A FlowFailure outcome is terminal and is never retried automatically as an
interrupted unload. [FLOW-4, FLOW-5, PAIR-2] Cancel keeps its existing command. The gate
returns its existing Killed {by} only after containment. A fault interrupt never
authorizes an unload retry. TRACE-HALT owns the UnloadClose extension: end_reason and
state_flow_failure appear together only for component failure. Ordinary unload keeps
both absent. The floor defines no second unload payload or operator_leave spelling.
[GATE-1, FLOW-4, FLOW-5, PAIR-2]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| TYPES-CODEC | Shared raw-spelling round trips and duplicate-key corpus at every nesting level | Deserialize through Value, accept duplicate control keys, or accept extra variant field |
| TYPES-LIMITS | Exact-bound and one-byte-over socket fixtures | Permit over-bound allocation or silently truncate |
| TYPES-CLASSIFIER | Compile pin plus valid, rejected, failed, and wrong-target corpus | Reuse the state request counter or erase target |
| TYPES-INPUT | Round trips consumed by harness and gate | Conflate live-gate flow_failure with gate-death fault, encode it as unload, or lose input attribution |

The fixture dictionary is exactly scheme_id fixture-read/1 with verbs [read] and a
recorded SHA-256 of its fixture artifact. It is accepted only by the test constructor.
No deployment parser accepts a test adapter as a production artifact. The production
model, certificate algorithm, and transport remain the classifier amendment's work. This
packet supplies a near-side contract, not a conformance model. [CLASS-2, CLASS-7]

## 5. Reads and persistence types, destinations sections 2 and 4

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TYPES-READ.** Add these shared types to state_flow. Control frames contain exactly one
externally tagged operation. They share the state channel's request ID domain. Every
answer is control_answer with the matching identity. A valid refusal stays an operation
answer. The closed refusal sets live in the state amendment, with common
answer_too_large. Reason: reads cannot have a second writer or hidden order token.
[SEAM-3, READ-1, READ-5]

ReadLimits is {max_result_bytes:DecimalU64, measurement_ref:String} on the wire. Both
endpoints require a positive size representable by their allocation index and a nonempty
measurement reference. The harness sends its loaded declaration's value in open and
checks the opened echo. The member holds it unchanged for the session. The measure is
not a claim that either codec ran qualification. [STORE-1, SEAM-3, COST-2]

| Control frame | Result frame |
| --- | --- |
| {read: {selector: ReadSelector}} | {read_page: ReadPage} or {control_refused: {code, detail}} |
| {next: {snapshot_id, offset}} | {read_page: ReadPage} or control_refused |
| {release: {snapshot_id}} | {released: {snapshot_id}} or control_refused |
| {shape: {}} | {shape: [{run, kind, count}]} or control_refused |
| {held: {}} | {held: {count}} or control_refused |
| {position: {}} | {position: SourcePosition or null} or control_refused |
| {grants: {}} | {grants: {surface: Vec<String>}} or control_refused |

ReadSelector is externally tagged history {noun, verb:null-or-string,
condition:null-or-string}, subjects {nouns:Vec<String>}, statement
{statement:StatementRef}, condition {condition}, span {seed_id, local_id, revision},
system_prompt {}, conversation {last_turns:DecimalU64}, open_pairs {}, marks {},
span_preview {span:SpanRef,through:SourcePosition}, plan_current {}, plan
{plan:PlanRef}, step_successor {plan:PlanRef,previous_step:Option<StepId>}, or
plan_flags {plan:PlanRef,after:SourceBlockId}, or observations with the exact
TYPES-OBSERVATION-READ shape. last_turns must be positive. IDs, offsets and counts are
DecimalU64. QueryResult is externally tagged entries:{entries:Vec<ReadEntry>},
open_pairs:{pairs:Vec<ReadPair>}, marks:{next_span_id:DecimalU64,
current_task:TaskScopeRef-or-null,open_scopes:Vec<SpanInfo>}, or
span_preview:{span:SpanRef,through:SourcePosition,entries:Vec<ReadEntry>},
plan_current:{current:PlanSnapshot-or-null,next_plan_id:DecimalU64,
next_step_id:DecimalU64}, plan:{snapshot:PlanSnapshot}, or
step_successor:{reference:StepQueryRef,instruction:ReadEntry}, or
plan_flags:{snapshot:PlanFlagSnapshot}, or observations as TYPES-OBSERVATION-READ
defines. Every non-entries result belongs only to its same-named selector. Spec 07's
PLAN-TYPES, PLAN-EFFECT, PLAN-READ and PLAN-FLAG-READ define the exact plan structures.
The harness checks that exact association before exposing a result. ReadEntry contains
source (SeedSource or EventSource), raw_block, text, bindings:Vec<ReadBinding>,
selected_span or null, step_query:Option<StepQueryRef>, and pairs:Vec<ReadPair>,
required even when empty. ReadBinding is {ordinal:DecimalU64,value:Binding}. ordinal is
the binding's original index in its filed block, never its position in a filtered read.
Require strictly increasing unique ordinals. SeedSource is {seed_id, ordinal}.
EventSource is {block_id, envelope, source_path, partition_rule_id}. selected_span is
{span_ref, start, end}. History entries contain one matched binding with all its nouns.
The statement selector returns one entry with exactly its requested binding, or
not_found. Both keep the complete original text and raw_block. subjects returns each
matched block once, with every binding in that block, including bindings that did not
match a query noun. Its noun list is nonempty, unique and strictly ordered by UTF-8
bytes, with no empty noun. Violations refuse invalid_selector. Span and preview entries
contain only bindings wholly within their selected interval. Intersecting a binding
partially refuses invalid_selector, or partial_pair when it is a paired request.
Conversation entries contain the block's complete binding list. Each read preserves the
original binding ordinal, including span and seed reads. ReadPair is Spec 05's PAIR-READ
shape, defined there once. Pair sources preserve the complete raw opening and terminal
blocks. These are attached to the request's statement, never parsed as another Text
block. No internal SQLite ID crosses the socket. [STORE-3, STORE-5, PAIR-3]

PlanFlagSnapshot has exactly {plan,after,through,total_flags,matched_flags,
unmatched_flags}, with the types in PLAN-FLAG-READ. The codec preserves nonnull
identities and DecimalU64 counts. The harness checks the plan/after echo and the checked
count sum. This result cannot be consumed as Entries or as a placement unit.
ReadRefusalCode names STATE-PAGES' closed read refusal set plus the common
answer_too_large case. Round-trip and wrong-variant fixtures must fail if either
endpoint accepts missing counts, a mismatched echo, overflow or a different result tag.
[PLAN-3, SEAM-3, COST-4]

ContentRef is externally tagged seed:{seed_id,ordinal} or event:{block_id}. SpanInfo is
{span:SpanRef,parent:SpanRef-or-null,kind:SpanKind,status:SpanStatus,
opened_source:SourceBlockId-or-null,ended_source:SourceBlockId-or-null,
replacement:SpanRef-or-null}. SpanKind is system_prompt, task, task_prompt, subtask,
step or step_outcome. SpanStatus is open, closed or retired. StepQueryRef and
PlacementContext are Spec 06's STEP-QUERY and PLACE-PORT shapes. CutMethod is flush or
elide. Spec 06's MARK-READ owns the ordering and frontier rules. Preview entries keep
selected_span with the original SpanRef, but only the tagged preview result can contain
entries from an open span. No preview is convertible to a closed-mark grant. [MARK-1,
MARK-3, MARK-6, PLAN-2]

**TYPES-INSTRUCTION.** Add RefusalRecord::Instruction with the exact fields and
InstructionRefusal cases in Spec 02's INSTRUCTION-METADATA. Its seam is instruction. The
harness owns purpose/reference checks, and the trace mirrors each case. The codec
fixture must reject a field or code belonging to another refusal family. A refusal
prevents that instruction and operation, never changes store contents or becomes a
classifier-component halt. [FLOW-8, CLASS-2]

**TYPES-PLAN-CONTROL.** Add LifecycleDirective::PlanUpdate and
LifecycleAnswer::PlanUpdated with Spec 07's PLAN-EXCHANGE fields, using the existing
internal kind tag. Add LifecycleAsk::PlanUpdate with no body. Operation refusal is
LifecycleRefusal::PlanUpdateRefused {case:PlanUpdateCase,detail:String,
rejected_blocks:Vec<DecimalU64>}, inside the existing Payload::Refusal. The cases are
defined once in Spec 07. No new adjacent response wrapper or untyped Value is admitted.
Exhaustive consumers include admin dispatch, worker coordination and CLI answer
rendering. The schema fixture must decode both ends and reject unknown fields. [PLAN-6,
FLOW-7]

PlanSummary is {plan:PlanRef,status:PlanStatus,document_sha256:Digest,
last_source:SourcePosition}. Add plan:Option<PlanSummary> to LifecycleAnswer::State,
omitted outside serving or when no plan is held. InTransition carries no new plan field.
HARNESS-PLAN supplies this committed account at boot and after effect receipts. It
changes no static LoadFacts identity. show renders it through the existing exhaustive
State match. Extend LifecycleRefusal::Unanswered's description to include plan-update,
without making it a negative commit receipt. [PLAN-6, STORE-5, SEAM-3]

**TYPES-PERSIST.** StateManagementConfig gains start: StateStartConfig, whose TOML
member dump-sha256 is a lowercase 64-hex SHA-256. It is required in published copies.
The root seed command may parse an authoring draft without it, then inserts it before
publication. A normal load never accepts that incomplete grammar. Configuration holds no
system prompt or prompt path. Artifact selection remains the administrative restore
selection outside the loaded operational declaration. [PERSIST-2, PERSIST-3]

Open carries the declared scheme_id and start: StartIdentity {seed_id, artifact_id,
dump_sha256, config_sha256, parent_artifact_id, position}, and opened echoes schema_id,
scheme_id, read_limits and that complete start. position is SourcePosition or null, with
no second top-level position in opened. The opener refuses a scheme_id unequal to
store_meta.scheme_id. The harness checks the echoed value before any content read or
filing. Each filing carries the same scheme_id and the member checks it again. A rebind
may change partition or bounds, but cannot relabel an existing dump under a different
scheme. It refuses scheme_mismatch before publication. No member claims to verify B,
because it cannot read configuration. Admin verifies the triple before passing start,
and the member checks the image/schema. SourcePosition is {block_id,
turn:null-or-string}, or null for a seed without operating events. [PERSIST-2,
PERSIST-3, STORE-5]

SeedRequest is a seed-mode-only first frame under protocol state-seed/1, carrying
schema_id, seed_id, scheme_id, ste100_scheme, checker_artifact_sha256, prompt_text,
raw_block, bindings and certificate. ste100_scheme holds the exact checked UTF-8 scheme
file bytes, as in EnterBinding. checker_artifact_sha256 is the expected model-manifest
digest from the authoring declaration, not the executable digest or a value inferred
from the certificate. The member has no root configuration path to read. The certificate
is SeedCertificate {prompt_sha256, scheme_id, checker_artifact_sha256, review_reference,
bindings:Vec<Binding>, duties:[PROMPT-3 duty IDs]}. Its parser requires every duty once.
The operator attests the separate review_reference. Semantic certification comes from
spec 02, not a schema check or an invented signature. The test constructor provides
fixture certificates and cannot be selected by deployment. [PERSIST-5, PROMPT-2,
PROMPT-3]

**TYPES-SEED.** state_flow owns SeedRequest, SeedCreated, SeedRefusal and their
state-seed/1 codec. A frame has exactly protocol, request_id and op, plus the members in
this table. protocol is state-seed/1 and request_id is the DecimalU64 string "1" in
every frame. There is no session_id, run_id or observation wrapper, because no serving
session exists. [PERSIST-5, SEAM-1]

| op | Direction | Additional required members |
| --- | --- | --- |
| seed | admin to member | request:SeedRequest |
| created | member to admin | created:SeedCreated |
| refused | member to admin | code:SeedRefusal, detail:String |

The request uses the 8 MiB complete-line bound and the answer uses the 1 MiB bound,
including the final LF. Use TYPES-CODEC's duplicate-key and strict-field rules. A
malformed envelope, wrong ID or wrong direction closes without a correlated answer. A
well-formed seed request has exactly the fields listed above. Its schema_id must be
state-content/1, seed_id is 32 lowercase hex characters, and raw_block is one complete
Text ContentBlock whose decoded text equals prompt_text. The nonempty bindings equal
certificate.bindings. Spec 02 owns scheme parsing and certificate validation against the
supplied scheme and expected checker identity. Reason: both seed endpoints need the
dictionary bytes to check the same bindings, and must not substitute an identifier for
that dictionary. [STORE-2, STORE-3, PERSIST-5, CLASS-2, PROMPT-2]

SeedRefusal has exactly unsupported_schema, malformed_seed, seed_certificate_invalid,
write_failed, image_too_large and snapshot_failed. Check schema, seed/block shape, then
certificate before any insertion, in that order. A valid request whose insertion fails
returns write_failed after rollback. Image creation failures use the same
image_too_large/snapshot_failed meanings as capture. No refused response carries
SeedCreated or leaves a publishable image. Admin maps seed_certificate_invalid to that
StatePersistenceInvalid case at seed, and the other valid refusals to its seed_failed
case, retaining code and detail in the invocation's diagnostic. Lost or malformed
results use the existing failed start/rollback path, never an inferred refusal or index
entry. [PERSIST-3, PERSIST-5, SEAM-2]

Save controls on state-flow/1 are {snapshot:{capture_id}}, {acknowledge:{capture_id,
dump_sha256}}, and {abort:{capture_id}}. The first result is {snapshot:{capture_id,
room_name, size_bytes, position}}, acknowledge returns {finished:{capture_id,
room_name}}, and abort returns {aborted:{capture_id}}. A valid failure returns
control_refused with image_too_large, snapshot_failed, unknown_capture, or
invalid_capture_state. The whole capture retains one state exchange request ID through
its continuations, including timeout abort and aborted after a late finished result.
The correlation is protocol, session_id, run_id, request_id and capture_id. The next
fresh exchange uses the ID after the original snapshot exchange, not another increment
for abort. A new capture cannot share it. [SEAM-2, PERSIST-1, PERSIST-4]

Admin-harness adds SavePointPrepared {capture_id, artifact_id, dump_sha256,
config_sha256} and SavePointCommit {capture_id} on the existing lifecycle channel.
Existing SavePointTaken/Left retain their role as outcomes. Replace SavePointReport with
{start:StartIdentity, capture_id:DecimalU64, event_run:RunId,
event_sequence:DecimalU64}. The event fields name the confirmed save_point record. The
captured source frontier appears once, at start.position. These fields replace the old
one-file name, save_point, run, sequence, turn, event_run and position fields, not
aliases for them. Only a Published capture produces this report. Diagnostic takes no
save point and uses Left.no_state without a report. SavePointNotTaken remains for
explicit operation refusal. Its missing snapshot/finished use remains under R20-6.
Prepared is an admin request to confirm a frozen healthy capture. Commit is the worker's
one-way confirmation, as defined by the persistence contract. Snapshot paths never come
from an external caller. [PERSIST-1 through PERSIST-4, FLOW-5]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| TYPES-READ | Cross-crate frame corpus and unknown-field cases | Admit SQL, missing identity, or wrong cursor offset encoding |
| TYPES-PERSIST | Draft/published parse matrix, save/load corpus and snapshot/finished/abort correlation cases | Load an unbound draft, accept a caller destination, change the abort exchange ID or accept a stale aborted answer |
| TYPES-SEED | Shared admin/member codec corpus, every refusal and substituted scheme/checker case | Accept numeric request_id, a nested refusal payload, missing scheme bytes or certificate bindings checked against another dictionary |

The read source field is externally tagged: {seed:{seed_id,ordinal}} or
{event:{block_id,envelope,source_path,partition_rule_id}}. SpanRef is
{seed_id:String,local_id:DecimalU64,revision:DecimalU64}. snapshot_id is DecimalU64.
Each read/next/release command is a new completed state exchange and increments its
request ID, while snapshot/acknowledge/finished/abort/aborted retain one ID. A read
snapshot may survive several exchanges and has its own ID domain. [SEAM-3, STORE-5]

The lifecycle capture subprotocol uses these Payload variants, all with capture_id (a
run-local DecimalU64 starting at 1), and only the listed additional members:

| Variant | Direction | Additional members |
| --- | --- | --- |
| SavePointBeginning | harness to admin | window:SaveWindow, tail_window:Option<SaveWindow> |
| SavePointImage | harness to admin | room_name, size_bytes, position |
| SavePointImageChecked | admin to harness | dump_sha256 |
| SavePointImageFinished | harness to admin | room_name |
| SavePointPrepared | admin to harness | artifact_id, dump_sha256, config_sha256 |
| SavePointCommit | harness to admin | none |
| SavePointPublished | admin to harness | artifact_id, dump_sha256, config_sha256 |
| SavePointCaptureComplete | harness to admin, leave only | none |
| SavePointAborted | either direction before Commit | code, detail |

These are ordered continuations of the original SavePoint or Leave exchange, not
unsolicited new invocations. Each receiver must be in the matching phase of
PERSIST-SAVE. A wrong capture, direction, phase, or A/B closes that exchange as protocol
failure. Beginning precedes Image. Published acknowledges Commit. After capture release
and recording, on-demand save answers SavePointTaken, while a leave carries
CaptureComplete before its remaining unwind and Left. Admin withholds public success
until index publication and the actual final response. If the worker exits after Commit,
R20-7 makes an incomplete capture broken. The failed_run shape below can report an
indexed artifact, but never substitutes for successful Left or elects the pending
branch. [PERSIST-2, PERSIST-4, FLOW-5]

State adds release_capture {capture_id}, returning capture_released {capture_id}, as a
fresh exchange after finished. It releases the retained image and room file after admin
copied/published it. abort returns aborted and is permitted in CaptureHeld and
FinishedHeld on the original capture exchange. STATE-PERSIST-STATES also permits that
abort for the just-refused capture until the next fresh exchange, closing the late
refusal race. Any other unknown or out-of-phase capture receives invalid_capture_state.
The member holds at most one capture. Seed mode has its own one-request lifecycle in the
admin and state amendments and never participates in these serving continuations.
[PERSIST-1, PERSIST-4]

Certificate duty IDs are ste100_emission, recall, forward_flags, locked_plan,
review_closure, restatement, ste100_input_history, initial_plan, and wall_summary. They
enumerate the nine PROMPT-3 duties, with no additional prompt requirement. Seed and
template certificates are distinct. The fixture dictionary certifies only its fixture
inventory and is never evidence for production templates. [PROMPT-3]

The seed certificate file supplies bindings. Admin copies them into SeedRequest and the
member requires equality with certificate.bindings. The prompt digest binds exact UTF-8
prompt_text. Production certificate verification remains spec 02's port. [PERSIST-5,
CLASS-2]

Add LifecycleRefusal::StatePersistenceInvalid {stage, case, artifact_id, detail}. Stage
is seed, bind, load, publish, or boot. Case is unsupported_layout, seed_exists,
seed_requires_unloaded, seed_certificate_invalid, seed_failed, binding_mismatch,
configuration_mismatch, scheme_mismatch, broken_save_point, publication_failed, or
boot_failed. artifact_id is an omitted Option<String>. Seed cases apply only at seed,
publication_failed only at publish, boot_failed only at boot, scheme_mismatch only at
bind, and the layout/binding/configuration/broken_save_point cases at bind or load. A
selected guarded artifact refuses at stage load or bind, never falls through to an older
entry. Admin restore uses the load-stage validation before appending selection. Other
combinations refuse decoding. Component lost-result outcomes remain failed run outcomes,
not this refusal. [PERSIST-3, PERSIST-5, FLOW-4]

SeedCreated is {schema_id:String, seed_id:String, room_name:String,
size_bytes:DecimalU64}. Every snapshot size_bytes has the same count encoding. Add
LifecycleDirective::Seed {agent} and BindState {agent} as admin command surfaces, and
LifecycleAnswer::Seeded {start:StartIdentity} and StateBound {start:StartIdentity} as
their successful answers. They are never forwarded as worker directives. Root
authorization applies before either command reads its fixed inputs. A normal
RestoreNamed keeps save_point and name, now A and artifact_id respectively. No new
command accepts content, a path or an optional destination. [PERSIST-2, PERSIST-3,
PERSIST-5]

Define LifecycleRefusal::FailedRun {run:RunId, capture_id:Option<DecimalU64>,
phase:PersistencePhase, published:Option<StartIdentity>,
artifacts:Vec<RetainedArtifact>, detail:String}, serialized with kind failed_run,
following LifecycleRefusal's existing tag. Optional fields are omitted. RetainedArtifact
is {location:String,kind:RetainedArtifactKind, sha256:Option<Digest>}.
RetainedArtifactKind is dump, configuration, binding, part or guard. Locations are
admin-derived names relative to the judged save-point/member room and include the root
name, never caller paths. Only confirmed extant files are listed, and an unknown digest
is null. Report a failed location probe in detail, not as proof that a file does not
exist. PersistencePhase is queued, beginning, barrier, capturing, hashing, finishing,
preparing, confirming, publishing or reporting. published may be present only after the
index append is verified. It is evidence of an artifact, not a successful Left or a
clean marker. Use this shape after Commit when the save operation has not completed and
the worker fails, under R20-7. The operator elected this outcome on 2026-10-20. [FLOW-5,
PERSIST-3, PERSIST-4]

Publication Confirmation is externally tagged serving {run_id,capture_id}, seed
{seed_id}, or rebind {parent_artifact_id}. capture_id is DecimalU64 and exists only in
serving confirmation. Cause is requested, graceful_unload, forced_unload, seed, or
rebind. seed/rebind confirmations require matching cause, and serving requires one of
the other three. Index and binding file carry the same confirmation. [PERSIST-2,
PERSIST-5]

StateManagementConfig also has persistence: PersistenceConfig, with save_wait and
snapshot_wait as MeasuredWait under their corresponding save-wait and snapshot-wait
declaration tables. Its elapsed-ns and measurement-ref grammar is the same as the flow
bounds. StartIdentity.position comes from binding.json, and opened must echo it inside
start, never as a second field. The state member compares it with the image's last
committed event before adoption. A seed position is null. [PERSIST-3, PERSIST-4,
STORE-5]

**TYPES-SAVE-TIME.** state_flow defines SaveWindow with exactly
{began_ns:DecimalU64,deadline_ns:DecimalU64}, both raw CLOCK_MONOTONIC readings and
began_ns < deadline_ns. LifecycleDirective::SavePoint adds required window beside cause.
Leave and JoinLeave gain no save-window member. The internal Payload variants above
carry Beginning and CaptureComplete on their existing exchange, never as external
commands. No CLI accepts a deadline or caller-selected budget. [PERSIST-1, PERSIST-4,
FLOW-5]

Beginning always carries tail_window, null on demand and present on unload. PERSIST-TIME
and PERSIST-BEGIN define the exact arithmetic, phase and load-value checks. If admin
cannot form an on-demand window without overflow, it returns ConfigInvalid for
state-management.persistence.save-wait before sending a directive. An expired but
well-formed window remains distinguishable from a malformed one. No codec converts
expiry to a fresh duration. CaptureComplete is legal only after Published on a leave
exchange, never on demand, and never stands for Left. The wire corpus tests both
variants, their phase association, null/present rules, canonical counters and the new
SavePoint field. Removing any check must admit its forbidden case. This certifies
representation and lifecycle association, not measured adequacy or an unresolved failure
election. [PERSIST-4, FLOW-5, COST-4]

Add SavePointLeg::CaptureNotReady, serialized capture_not_ready. It is used only when a
queued on-demand save's outer deadline expires before its barrier starts.
LifecycleRefusal::SavePointNotTaken {missed: CaptureNotReady} leaves serving active.
Admin and harness match it explicitly, never treating it as a component failure or
reusing Answer/Finished. Other explicit save operation refusals use SavePointNotTaken
with Answer, Finished, or Published. For serving, these mean an explicit refusal at
image creation, image finish, or root publication respectively. R20-6 preserves their
missing image/finished timeout use and retry after PERSIST-CLEANUP confirms aborted,
with late-answer discard. A failed cleanup follows FlowFailed, not SavePointNotTaken.
The serving MemberDead path is retired in favor of the typed FlowFailed
outcome. Diagnostic takes no save point. The detailed typed refusal goes on the trace,
never in the success-only SavePointReport. [PERSIST-1, PERSIST-4, CLASS-8]

## 6. Placement and decode vocabulary, destination sections 2 and 4.4

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**TYPES-DECODE.** Preserve TokenDirective's internal kind tag, TokenAnswer's adjacent
kind/body tags and their existing numeric u64 encoding. Extend these closed enums with
the following exact shapes. Keep Open's existing session, messages and column_ask, and
keep Generation and the streaming intermediates unchanged. Reason: canonical messages
still cross the existing decode seam while the SPU supplies token facts. [PLACE-1,
PLACE-3, PLACE-5, PLACE-6]

| Type and variant | Required fields |
| --- | --- |
| TokenDirective::AppendAndGenerate | Existing turn:TurnKey and delta:Vec<Message>, plus tool_schemas:Vec<String> |
| TokenDirective::MeasureDelta | keep:u64, delta:Vec<Message>, tool_schemas:Vec<String> |
| TokenAnswer::Opened | prefix_tokens:u64, capacity:u64, generation_limit:u64, terminal_tokens:u64 |
| TokenAnswer::DeltaMeasured | resident:u64, prefix_tokens:u64, keep:u64, input_tokens:u64, capacity:u64 |
| TokenAsk::MeasureDelta | keep:u64, under TokenAsk's existing ask tag |
| RefusalRecord::Placement | assembly_id:DecimalU64, code:PlacementCode, detail:String, seam value placement |

PlacementCode is invalid_span, invalid_pair, invalid_cut, head_over_capacity,
tail_over_capacity, generation_room or stale_position, as PLACE-PORT uses. No new
TokenRefusal variant is required. NotOpen, OutOfOrder and MalformedDelta answer
measurement where appropriate, while a physically oversized proposal can still return
its count. Opened is no longer a unit answer after cutover, and an absent body or
schemas member is not defaulted. An empty schema list is explicit. [PLACE-3, PLACE-4,
PLACE-5]

CUT-RELOAD's ReloadTail, ReloadItem, StagedTail, Staged, ReloadRefused, Busy and
ReloadCode are private harness types defined by Spec 06. They add no wire arm.
STAGED-APPEND consumes the pinned tail in the next ordinary AppendAndGenerate. ReFeed's
rendered string, nonempty token path and ReFed answer retain their diagnostic meaning.
[SEAM-2, PLACE-5, R20-15, R20-16]

SpanRef and argument occurrences remain separate types. UnitOccurrenceId is
{run:String,assembly_id:DecimalU64,unit_index:DecimalU64}. MarkOccurrenceId is
{run:String,ordinal:DecimalU64}. CutReference is
{span:SpanRef,occurrence_id:MarkOccurrenceId}. ArgumentCursor is externally tagged
head:{} or message:{message_id:DecimalU64,block_end:DecimalU64,
byte_end:Option<DecimalU64>}, with Spec 06's exact endpoint rules. Neither occurrence ID
nor cursor is a token index. The harness alone maps a current cursor to N from SPU
facts. UnitOccurrenceId cannot stand in for MarkOccurrenceId. [MARK-6, PLACE-5]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| TYPES-DECODE | Shared Rust enum/serde corpus at both peers | Accept old unit Opened, default schemas, confuse count and SpanRef, or decode a wrong response arm |
| PlacementConfig | Parser and load-fit cases with a resolved OperatorTunable SPU generation limit | Guess an absent tunable value, wrap capacity arithmetic or open user channel after failed fit |

## 7. Open elections

Measured values are gate-wait.elapsed-ns and cut-wait.elapsed-ns. Their measurement
method is in the harness amendment. Schema and control vocabulary expansions are named
dependencies, not an open map for a coder to fill. The scheme declaration is defined
above. Its production artifact and the tool binding implementation remain owed to
classifier and pair amendments.

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover

[canonical]: ../weaver-trace/weaver-trace-Spec.md#2-canonical-form
[empty-correction-ruling]:
https://github.com/toddwbucy/WeaverAgent/pull/213#issuecomment-6330669954
