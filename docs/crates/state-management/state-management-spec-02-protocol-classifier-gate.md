# State management Spec 02 - STE100 protocol, classifier and input check

**Status:** MERGED.

**Landing PR:** #213

**Version:** 0.14 **Date filed:** 2026-10-21 **Document ID:** state-management-spec-02
**Parent:** state-management-primitives-PRD

## 0. What this document is

This Spec serves CLASS-1 through CLASS-10, GATE-1, READ-3, the protocol part of PROMPT-2
and PROMPT-3, and their FLOW-8, COST-4 and COST-5 obligations. Read it with the
[PRD][prd], [foundation][foundation] and [October 18 rulings][rulings]. It uses the
foundation's TYPES-CLASSIFIER, HARNESS-CLASSIFY and TEMPLATE-CHECK ports. The owning
additions are in the types, harness, SPU and internal amendments and the separate
STE100 contract. The system prompt's text is a separate owed document.

The code baseline is 2eb5c5a2e47f89309631327ff121393d393be85c. On that baseline,
`weaver-spu/src/bin/classify.rs` serves ModernBERT label scores through one
SOCK_SEQPACKET descriptor. It cannot emit these bindings. R10 keeps that duty, its
LabelDirective/LabelAnswer/LabelRefusal types, its optional presence, and its existing
consumers. This Spec adds a separate STE100 duty. It does not convert a score into a
conformance verdict. The gate continues to relay content unread.

Every replacement in this document applies at the foundation's qualified code cutover.
Until then its previous serving contract remains applicable under
[FOUNDATION-TRANSITION][transition]. This applies also to each amendment in section 8
when placed in its owning document. A document merge alone neither activates the new
classifier nor retires the working seeding path.

## 1. Ownership and the scheme

**PROTOCOL-OWNER.** Add `weaver-spu-ste100`, an SPU binary with its implementation in
`weaver-spu::ste100`. The worker starts it under the existing organ fork, exec,
credential, stderr and reaping machinery. It receives one inherited connected socket at
fd3 and no state, gate or trace descriptor. The harness owns its near end in
`flow/classifier.rs`. The SPU duty owns sentence analysis, model execution, binding and
template proof. The harness validates the result's representation, records it and
decides its use. Reason: classification is model-based sort on the SPU, while routing,
admission and filing remain separate duties. [CLASS-1, CLASS-2, CLASS-4, SEAM-2, October
18 R10]

**SCHEME.** Use one immutable UTF-8 JSON artifact, `ste100-scheme.json`, provisioned as
a judged regular file in admin's config root with the root/group/mode checks of admin
Spec 9. It contains the fields below, rejects unknown or duplicate keys at every depth,
and carries no self-hash. Its exact bytes, including final whitespace, hash to the
lowercase 64-hex SHA-256 in flow.scheme.artifact-sha256. scheme_id is `ste100:` followed
by that digest. The declaration's ordered verbs must equal the artifact's keys in order.
No runtime ask extends this dictionary. Reason: the store, input check, output check and
template checker must agree on one language instance. [CLASS-1, CLASS-2, STORE-2,
STORE-8]

| Artifact field | Type and meaning |
| --- | --- |
| format | Literal ste100-scheme/1 |
| reference | {edition:String, document_sha256:Digest, rule_ids:Vec<String>, dictionary_sha256:Digest}, all nonempty and unique where listed |
| grammar | {format:String, bytes_base64:String, sha256:Digest}, the checked executable grammar described in PARSER |
| verbs | Ordered Vec<{key:String, forms:Vec<String>, sense:String, authority:RuleReference}>, unique keys and unambiguous forms |
| technical_names | Vec<{text:String, authority:RuleReference}>, exact source spellings |
| conditions | {format:String, bytes_base64:String, sha256:Digest}, the grammar for conditions and step references |
| recall_keys | Nonempty unique subset of verbs.key, all with the same noun-history action |
| qualification | {report_sha256:Digest, corpus_sha256:Digest, checker_abi:String}, identifies the separately reviewed qualification record |

Digest means lowercase 64-hex SHA-256. RuleReference is {rule_id:String,
disposition:String}, with disposition approved_word or technical_use. The grammar and
condition blobs decode canonically and match their digests. The record identifies the
normative material that the qualified checker implements. It is not a copy of the
standard hidden in a prose claim. The final dictionary, rule compiler and normative
corpus are owed as O02-1. No verb in the old draft's illustrative list is presumed
approved. A synthetic `fixture-read/1` artifact remains test-only. [CLASS-1, CLASS-2,
CLASS-7]

**CUSTODY.** Admin reads the scheme once during inventory, verifies its digest and
closed dictionary against the bound declaration, and carries the exact bytes in
`EnterBinding::Serving.ste100_scheme`. The harness never opens agent.toml or this root
file. It gives those bytes to the new duty during bootstrap and checks the ready echo.
All three retain the same artifact identity. Rebinding the same dump cannot change its
scheme, as ADMIN-REBIND requires. The configuration has no prompt field. Reason:
repeating a path lookup after inventory would allow a different scheme to serve the
checked declaration. [TAP-2, CLASS-2, PERSIST-3, PERSIST-7]

The enter carrier retains the floor's MAX_ENVELOPE_BYTES bound, currently 64 KiB for the
complete lifecycle envelope. It does not use CHANNEL's fragmented transport. Admin's
existing send_with_sink checks the fully serialized envelope, including the escaped
scheme text, before sendmsg. An over-bound envelope uses ChannelFault::Truncated and
follows the existing DescriptorsUnusable enter-send refusal and rollback, with pre-load
diagnostic enter_frame_too_large. It is not non_ste100 and no model input is admitted.
Qualification must fit the complete declared instance through both carriers. An artifact
that needs a larger enter requires a separately reviewed transport amendment, never
silent truncation or a worker path lookup. [CLASS-8, FLOW-3, PERSIST-6, SEAM-2]

**DECLARATION.** Add `state-management.ste100` with model-binding in the existing
ModelBinding grammar, model-artifact-sha256, instances, max-wire-bytes, measurement-ref,
repair-cap and decision-wait. `instances` is a positive u32. max-wire-bytes is a
positive DecimalU64 that fits usize. repair-cap is {attempts:positive u32,
measurement-ref:nonempty String}. decision-wait is the foundation's MeasuredWait. In
TOML use kebab-case, and in shared Rust/JSON use snake_case as the existing floor does.
All values are required for serving, with no default. Production rejects fixture
artifacts. Reason: resource choices and a repair cap must be explicit, attributable and
testable, rather than implicit queue or model behavior. [CLASS-6, CLASS-7, CLASS-8,
CLASS-9]

Add the root key `ste100-binary` with the existing organ-binary custody judgment. It
stays in admin's root, outside the bound declaration. Add its executable digest to the
load tuple. The serving enter payload carries `Ste100Instruction` once at
state_management.ste100 and the scheme bytes at ste100_scheme. Loaded facts retain those
same fields, without changing legacy ClassifyInstruction. Admin syntax errors are
ConfigInvalid naming the field. Semantic artifact failure during enter is
StateManagementInvalid with stage template and template_failed or scheme_mismatch, and
follows after-load rollback. [FLOW-3, CLASS-8, PERSIST-6]

Ste100Instruction has the seven fields of state-management.ste100 above, using the same
floor types. The model artifact's manifest supplies its supported binder ABI, encoder
family, tokenizer, weight files and their digests. model_artifact_sha256 hashes the
manifest bytes. Admission verifies each named file against it before reporting Ready.
That check supplements the existing ModelBinding admission, never assumes that a
sequence-classifier artifact has a compatible head. [CLASS-2, CLASS-7, FLOW-3]

The manifest is UTF-8 JSON with format ste100-model/1, checker_abi ste100-span/1,
encoder:{family:String,config:ArtifactFile,weights:Vec<ArtifactFile>},
tokenizer:ArtifactFile, span_head:ArtifactFile, alignment:String,
max_sentence_tokens:positive u32, and labels:Vec<String>. ArtifactFile is
{name:String,sha256:Digest}. A name is a relative artifact path with no empty, dot or
parent component and no symlink escape. Family and alignment must name compiled adapters
supplied by O02-2. The label array is exactly MODEL's seven labels in that order.
Duplicate/unknown fields refuse. checker_artifact_sha256 on ready and seed certificates
is this manifest digest. artifact_sha256 on ready is the scheme digest. The checker
executable's separate digest remains in the load tuple. [CLASS-2, CLASS-7, FLOW-3]

The scheme-load pass certifies byte identity, the fixed vocabulary and compatible
checker ABI under the recorded qualification. It does not certify safety, truth, or
unseen model accuracy. Root custody makes the qualification reference an operator
attestation. A hash alone is not a conformance proof. [CLASS-4, CLASS-7, PROMPT-2]

## 2. Parsing and the model's narrow job

**PARSER.** `ste100::grammar` applies the pinned grammar to complete Text blocks. It
preserves the original UTF-8 bytes, identifies sentence and word spans, validates the
declared lexical/rule constraints and yields candidate action, object and condition
spans. It splits computation at sentence boundaries, never the content block or its
filing obligation. A valid rejection names a rule in detail and exact defect spans under
code non_ste100. The grammar artifact's ABI, compiler and rule coverage are part of
O02-1. An unsupported rule is failed admission, not a rule silently omitted from
checking. Reason: a model capacity limit and a language defect must not be the same
verdict. [CLASS-1, CLASS-2, CLASS-8, STORE-3]

**MODEL.** The SPU owns `ste100::binder`, a span-labeling model adapter. Its required
output is a finite score matrix over source words and the fixed labels O, B-VERB,
I-VERB, B-NOUN, I-NOUN, B-COND and I-COND. Word-to-token alignment returns source byte
intervals. It is not the ModernBERT sequence head's id2label vector. A model artifact
without this adapter and alignment contract refuses admission. The selected encoder,
tokenizer and head, their digests, backend ABI and supported sentence capacity are
recorded in the measured model selection. The existing label model may be evaluated as
an encoder candidate, but its sequence scores cannot satisfy this port. Reason: the
required output is a verb bound to its objects and condition, not a ranked label for the
whole message. [CLASS-2, CLASS-6, CLASS-7]

For each grammar action, the binding decision selects the highest total model score
among the grammar's valid tag assignments, in source word order. Each assignment has one
governing verb span, one or more object spans, and at most one governing condition span
for that action. Compound objects give several nouns in the same Binding. Distinct
governing actions produce distinct Bindings. Ties use the lexicographically smallest
full label sequence in the label order written above. Scores are finite f32 values,
summed in source order after conversion to f64, with no epsilon tie rule. No deployment
confidence cutoff changes admission. Nonfinite scores, missing word scores, broken
alignment or no model result are component failures. An empty grammar parse is a correct
defect only when a pinned rule supports that result. This is a model judgment, whose
binding error is measured, not a claim of certainty. [CLASS-2, CLASS-8, STORE-3]

O02-2 must select a model and a working adapter that meet this interface. A different
representation shown necessary by that measurement returns as a Spec delta before code
relies on it. The fixed score interface is a writer-seat proposal for review, not a
claim that the current tree contains such a head. [CLASS-7]

**BINDING.** Emit the foundation's Binding, NounBinding and Span types unchanged.
Offsets are half-open byte positions in the exact decoded text. statement_span contains
verb_span, noun spans and any condition_span. noun.value is exactly the substring named
by its span, with neither case folding, lemmatization nor trimming. Non-step conditions
likewise use exact source text. A condition recognized by the scheme's step-reference
grammar yields `step:<local_id>`, with the positive DecimalU64 local ID from Spec 07's
PLAN-TYPES. The loaded store supplies the seed identity. The condition does not embed a
JSON StepId or a second seed spelling. That grammar has one owner in the scheme and is
shared with the plan amendment. Empty Text is non_ste100. Accepted text has a nonempty
ordered binding list. Template targets have the exception in TEMPLATES. Reason: the noun
index must query what the classifier filed and the source must remain reconstructible.
[STORE-2, STORE-3, READ-1, PLAN-3, CLASS-2]

**CAPACITY.** A grammatical sentence over a pinned language-rule limit is non_ste100,
with that rule named. An otherwise conforming sentence beyond the selected model's
capacity is component_failed with code no_result and detail artifact_capacity. It is
never called bad STE100. The qualification must demonstrate capacity over the
deployment's supported inputs and outputs, including long technical names and tokenizer
expansion. A block may contain many sentences and they may be scheduled across the
declared instances, but one terminal reply covers the entire original block. No accepted
prefix is filed from a rejected block. Reason: whole-block filing must not hide loss
behind model limits or segmentation. [CLASS-2, CLASS-8, CLASS-10, STORE-3, PLACE-4]

## 3. The SPU seam and its states

**CHANNEL.** Create a separate SOCK_SEQPACKET pair using the existing child-end
placement and non-inheritance pattern. No organ envelope crosses it. One logical message
can occupy several datagrams, because a complete binding answer can exceed the label
seam's 64 KiB. The old label seam still uses its original whole-datagram contract. This
is transport framing, not document chunking or partial classification. Reason: preserve
connected-pair custody without adding a second process that owns store or gate traffic.
[CLASS-2, SEAM-2, FLOW-4, October 18 R10]

Each datagram has a 26-byte header followed by data. Byte 0 is version 1. Byte 1 is tag:
bootstrap 1, request 2, reply 3, ready 4, admission_refused 5. Bytes 2..10 are the
big-endian u64 message ID, 10..18 the total payload byte length, and 18..26 the offset.
The maximum datagram, including header, is 65536 bytes. Fragments have nonempty data,
start at zero, advance without overlap or gaps and finish exactly at total. Bootstrap,
ready and admission_refused use ID zero. Request and reply use the request_id in their
JSON. The receiving end checks total against the loaded max_wire_bytes before
allocating, and holds at most one partial logical message. MSG_TRUNC, excess length,
repeated fragments, wrong order or mixed IDs fail the channel. There is no fragment
acknowledgment or result before full reassembly.

JSON is UTF-8 with the foundation's strict codec, decimal counters and duplicate-key
refusal. Bootstrap uses shared Ste100Bootstrap and is {protocol:"state-ste100/1",
scheme_text:String, instruction:Ste100Instruction}. Ready is {protocol:"state-ste100/1",
scheme_id, artifact_sha256, checker_artifact_sha256, instances, max_wire_bytes}. Its
values must equal the inventory. It uses shared Ste100Ready. AdmissionRefused uses
shared Ste100AdmissionRefused and is {protocol:"state-ste100/1", code, detail}, where
code is scheme_mismatch, unsupported_artifact, invalid_grammar, insufficient_capacity or
invalid_instruction. The complete request and reply JSON are exactly Ste100Request and
Ste100Reply from TYPES-CLASSIFIER. [CLASS-2, CLASS-8]

The binary takes exactly `--max-wire-bytes <positive-decimal>` on its trusted launch
vector. That bounds the first receive before bootstrap is available. Bootstrap must
agree with that value and carries the complete model binding. The harness starts this
duty after load and before template checks and boot, using the remaining admin load
budget. No stage receives a fresh full load timeout. [CLASS-8, PERSIST-6, FLOW-3]

**PORT.** Keep Ste100Port's submit/try_result/readiness shape and its one outstanding
request. One harness pump owns its read cursor. The duty may use several resident model
instances internally to process independent sentences of that request. Instances use the
same model and scheme and results are reassembled in sentence order. Their count is
measured. No second thread or runtime is added to the harness. No legacy LabelAnswer or
LabelRefusal maps to a Ste100Reply: receiving one on this new seam is invalid_result.
Reason: preserving the floor's closed result shape prevents optional label-service
behavior from entering the mandatory flow. [CLASS-4, CLASS-6, CLASS-8, October 18 R10]

try_result's Completed transfers the complete reply once. The adapter then has no
outstanding request. HARNESS-CLASSIFY's coordinator keeps ResultHeld until its
ste100.result authoring completes and forbids another submit during that interval. This
local retention adds neither a wire acknowledgment nor another trait method. The
held-result fixture blocks trace capacity and requires no duplicate completion or new
request before Recorded. Removing the coordinator guard must fail it. [FLOW-3, CLASS-8,
SORT-4]

| Duty state | Event | Result and next state |
| --- | --- | --- |
| Starting | Valid complete bootstrap and successful admission | Ready once, then Idle |
| Starting | Invalid bootstrap, artifact or grammar | AdmissionRefused then exit, enter refuses |
| Starting | Request before readiness | Protocol failure, close, enter refuses |
| Idle | Request with next ID and matching scheme | Checking, own the whole request |
| Checking | Complete valid accepted or non_ste100 result | Reply once, then Idle |
| Checking | Backend component fault | component_failed reply if writable, then Failed and exit |
| Checking | Another logical request | Protocol failure, close |
| Idle | Peer closes on orderly teardown | Exit without invented result |
| Any live state | Malformed transport or peer loss | Close, harness observes component failure |

| Harness observation | Result/code and record |
| --- | --- |
| Peer exit or EOF during service | crashed, fault with the outstanding request |
| Terminal backend cannot return bindings, including capacity | no_result, ste100.result followed by fault |
| Wrong ID/target/scheme, extra or invalid spans, malformed JSON or unsolicited reply | invalid_result, fault with received identity and outstanding request |
| Failed send, bad fragment framing or MSG_TRUNC | transport_failed, fault |
| Correct non_ste100 reply | ste100.result then rejection/repair, never the halt latch |
| Still pending below an applicable bound | Pending plus lag measurement |
| Still pending at gate, cut or decision bound | Missing, foundation halt with the wait domain |

A single writer means one pump owner advances each socket's send cursor. It does not
mean a writer thread. Every complete reply is recorded as ste100.result before reuse,
repair or filing. Requests are attributable to their source, offered input or template
target. Template text and identity are in the load inventory. No new classify.output
meaning or gate.refused kind is introduced. [FLOW-3, SORT-4, CLASS-8, October 18 R10]

**PORT-SERIALIZATION.** The port accepts one complete request at a time and never
preempts it. A second submit returns Busy with ownership of the unsent request. The
caller chooses which pending work to submit next. Queue wait and execution time remain
separate measurements. Gate, decision and cut bounds cover their respective
model-blocking waits, including time before submission. Template certification must
complete before serving admission. No priority policy is embedded in the classifier or
this adapter. [CLASS-4, CLASS-6, CLASS-8, R20-12]

## 4. Harness templates and seed certification

**TEMPLATES.** The compiled harness inventory is the only template source. The scheme
contains grammar and lexical classes, never a second inventory. Each entry is
TemplateSpec {template_id:String, kind:String, parts:Vec<TemplatePart>}. TemplatePart is
externally tagged literal:String or slot:{name:String, class:SlotClass}. SlotClass is
technical_name or step_id. The former is one of the scheme's technical_names and the
latter is the scheme's step-reference token class. Arbitrary noun spans and arbitrary
strings are not substitution classes. Long held content uses the store's HeldMaterial
contribution. Rejected text stays in its current argument for repair and is named by
source ID, never manufactured as HeldMaterial or inserted into a template slot. Reason:
accepting a source sentence does not prove that concatenating it into a different
sentence conforms. [CLASS-2, FLOW-8, SORT-4, PLAN-2]

For a template target, Ste100Request.text is the canonical compact JSON encoding of
TemplateSpec, with fields in the order above. The target's template_id must match. The
checker parses the template and proves inclusion of all its substitutions in the
compiled grammar. The elected proof method is language inclusion over the grammar's
finite-state token classes, including its bounded word/noun-cluster counters: intersect
the template language with the grammar's complement and require that intersection to be
empty. An unsupported construct refuses certification. This is a symbolic rule check,
not a universal inference from sampled model outputs. O02-1 must supply that compiler
and its normative rule correspondence. [CLASS-2, FLOW-8]

Accepted template replies have bindings:[] and no invented text span. Text-target
replies retain nonempty bindings. A valid bad template returns non_ste100 and refuses
load. An invalid checker result is component failure and refuses load through the same
after-load rollback. The harness derives its TemplateCertificate from the exact accepted
replies, scheme digest, checker digest and inventory digest. Replies are turnless
ste100.result events, after load. The inventory bytes are serialized in the load tuple
so replay can recover what was checked. [FLOW-3, CLASS-8, PROMPT-2]

Runtime TemplateExpansion is private and constructed only from a certified ID and typed
TechnicalName or StepId values. Invalid substitutions or unknown IDs refuse the
operation before authoring, with refusal evidence. The harness emits each expansion's
own event before its text reaches the argument. A correctly rejected expansion still
follows state_template_rejected and operation failure, never model repair or component
halt. A certificate does not make a model infallible. [FLOW-8, CLASS-8, CLASS-10]

The template pass certifies the named inventory's grammar closure under exactly those
classes. Evidence is the inventory, grammar, proof outcome and exact replies. It does
not prove arbitrary substitution, safety, step authorization or model accuracy. Removing
either the inclusion check or typed-value constructor must make its respective
acceptance test fail. [CLASS-2, FLOW-8]

**INSTRUCTION-METADATA.** loop.instruction has one trace-owned shape,
LoopInstructionRecord {template_id:String,message:Message,
instruction:LoopInstructionMeta}. message is role user with exactly one complete Text
block from a certified expansion. instruction is externally tagged by the closed
variants below. Its references are metadata, never text interpolated into a template.
SourceContext and StatementEffect remain null when that Text is filed. The metadata is
retained on the trace, with any required plan/pair references held by their own typed
records. Reason: every loop instruction must have its own trace record and a declared
purpose before it affects a decision. [FLOW-8, CLASS-9, PLAN-4, STRAT-2]

| Variant | Exact metadata | Valid authoring condition |
| --- | --- | --- |
| restatement | source:SourceBlockId, attempt:DecimalU64 | Current correctly rejected model block, attempt starts at one and respects the repair cap |
| retry | retry_of:CallId, attempt:DecimalU64 | Filed terminal from Spec 05 RETRY, same-session identity and positive attempt |
| plan_proposal | plan:PlanRef, authorization:SourceBlockId | Current authorized plan and exact operator source, no previous lock |
| step | plan:PlanRef, step_id:StepId, instruction:StatementRef, span:SpanRef | Filed activation and exact held instruction of the active step |
| review | setup:ReviewSetupRef, work:WorkRef | Matching held plan/setup identity and immutable work reference |
| compact | reservation:SpanRef, through:SourcePosition | Closed reservation and committed frontier, no trigger or summary decision implied |
| wind_down | Empty object | Existing lifecycle wind-down phase |

Only wind_down permits an absent turn. Other variants require the current open external
turn, except restatement of the lifecycle wind-down's own emission, which uses that
already bracketed wind-down turn. No instruction opens a turn by itself. The variant
chooses its certified template family. Review and step use the referenced step ID for
the typed slot. Compaction chooses the fixed system or task reservation template, never
arbitrary source content as a slot. Metadata mismatches refuse the operation before
authoring and emit RefusalRecord::Instruction
{template_id:String,case:InstructionRefusal,detail:String}. InstructionRefusal is
invalid_template, invalid_context, stale_source or wrong_turn. This uses the existing
refusal kind, with seam instruction. The operation stops with that refusal. It does not
latch component failure or create a language-defect result. They never grant a model
pass. These are structural provenance checks. The caller chooses the instruction and
invocation time. No variant grants a strategy or starts a pass. [FLOW-8, CLASS-2,
PLAN-2, PLAN-4, STRAT-2]

A closed-enum codec fixture enumerates every variant, its required fields, null refusals
and turn rule. Runtime fixtures substitute a stale source, wrong step, foreign seed and
report-shaped model text. Removing the private constructor or reference/turn check must
fail the respective fixture. This is a representation and authority pass, not production
certification of the template words. [FLOW-8, CLASS-2, PLAN-5]

**SEED-CERTIFICATE.** The foundation's SeedCertificate fields remain unchanged. The
separately run qualification tool in WeaverAnalysis produces the certificate from a full
accepted prompt check through this same SPU protocol, paired with the operator's review
of all nine PROMPT-3 duties. It uses input target {input_id:1, block:0} outside any
serving run, never an input to a live store. The certificate holds the exact bindings,
prompt hash, scheme, checker artifact hash, review reference and complete duty IDs
already required by TYPES-PERSIST. [PERSIST-5, PROMPT-2, PROMPT-3, CLASS-2, October 18
R9]

Add `verify_seed_certificate(prompt, prompt_digest, certificate, scheme,
checker_digest)` to the floor's state_flow validation module. Admin and the member each
compute prompt_digest from the exact supplied UTF-8 bytes through their own hash
implementation. The floor validator compares that digest, scheme and checker identity,
all duty IDs once, a nonempty review_reference, and the same binding span/dictionary
checks used for filing. TYPES-SEED carries the exact ste100_scheme bytes and expected
checker_artifact_sha256 to the member. Decode and validate that scheme under SCHEME,
including its byte digest, before using its verb dictionary. Compare the request's
scheme_id and the certificate's scheme_id with that result. Admin takes the expected
checker digest from the authoring declaration, never from the certificate itself. It
returns VerifiedSeed or the foundation's seed_certificate_invalid. Admin runs it before
the seed command, and the member repeats the byte/binding checks before insertion.
Neither runs a model or treats a checksum as proof of language. The protected
certificate is an operator-provided attestation from the reviewed tool. Its pass is
structural and attributional. Semantic qualification remains the producer's duty, and
that producer and normative artifact are explicit cutover prerequisites. [PERSIST-5,
PROMPT-2]

## 5. Input at the harness frame path

**INPUT.** Keep credential admission in the gate and request parsing in the harness.
After parse_request and dialer judgment, before Ports is granted and before
turn.started, the harness checks request.text. The wire grammar remains {text:String},
with any role member refused after cutover. That retires #76's live system seeding line
as part of the foundation, not in an earlier standalone act. Origin is operator exactly
when the gate-authenticated dialer UID equals the operator UID carried at enter,
otherwise external. JSON text cannot choose it. Reason: the gate owns who may connect,
not the meaning of a line. [GATE-1, TAP-1, PERSIST-5, PERSIST-7]

The foundation's input.offered is authored before parsing, with its exact original
frame. Input checks use target input {input_id,block:0}. input_id is assigned on
serialized check intake, not arrival behind an active turn. The gate deadline starts
there, before classifier queueing. All subsequent delays for that check count without
reset, including a wait to author input.offered. Gate input may not open another turn
while checking. [FLOW-3, CLASS-4, GATE-1, COST-4]

| Input state | Event | Action and next state |
| --- | --- | --- |
| HeldFrame | Earlier turn/check still owns service | Remain queued, no new check deadline |
| HeldFrame | Frame becomes eligible | Assign input_id, start gate_wait, author input.offered, Offered |
| Offered | Shape/dialer refusal | Record refusal, send unnamed refused close, Idle |
| Offered | input.offered recorded, shape and dialer accepted | Submit exact text, Checking |
| Checking | Accepted reply recorded | Mint AcceptedInput, disarm gate_wait, no turn or message.user yet |
| Checking | Correct non_ste100 recorded | Record refusal/Input(non_ste100), send unnamed refused close, Idle |
| Offered, Checking or AcceptedInput | Quiesce or operator leave | Cancel this input operation, revoke any token, send unnamed stopped close, keep any outstanding exchange accounted until result or bounded teardown |
| Offered or Checking | Component failure or gate_wait reached | Halt, no turn or model input, send unnamed stopped close if transport stands |
| AcceptedInput | Component failure | Halt and revoke token, no turn or model input |
| AcceptedInput | Engine consumes token | Author turn.started and message.user with causal_parent=input.offered sequence, consume token once |

Correct refusal disarms gate_wait only after its required trace records complete.
Cancellation with no outstanding check disarms it after the cancellation disposition.
With a submitted check, retain that exchange and its original deadline until its
recorded result or bounded teardown. A late accepted result never remints the revoked
token. Reason: the input check and turn authoring are different steps, and neither
cancellation nor a nested cut may reuse the input or silently renew its check. [GATE-1,
CLASS-8, FLOW-3, FLOW-4]

The refused close is the following single line. It has neither run nor turn.

```json
{"kind":"refused","reason":"Restate the input in STE100."}
```

Do not echo a large input in the close or add fields to the gate-world contract. The
sender already has that input. A halted check uses {"kind":"stopped","reason":"The agent
stopped because state flow failed."}. Operator leave uses reason "The agent is leaving."
Both are unnamed when no turn opened. The gate merely relays the line. [GATE-1, FLOW-4,
FLOW-5]

**REUSE.** AcceptedInput privately holds input_id, exact checked UTF-8 text, scheme_id
and bindings. Its consumed message.user is Role::User with one Text block at content
index zero. Reuse requires byte-for-byte text equality, equal scheme and that token's
single use. The resulting source key is the trace run/sequence/index, with input.offered
linked by causal_parent. The partition must still select an actual copy. A mismatch is a
harness contract failure, never permission to reuse labels or create a second feed. The
gate-release pass certifies only this qualified check and correlation, not safety or
authorization of text instructions. [CLASS-4, FLOW-1, SEAM-2, GATE-1]

Operator plan updates arrive through the separate root management verb specified by 07,
not a fabricated gate role. That Spec must use this check before applying a statement
update and must define its own admin reply. Tool returns bypass this check and remain
unclassified. Safety can attach before or after input conformance and has its own
result, never an alias for accepted. [PLAN-6, STORE-2, CLASS-4, DEFER-4]

## 6. Model emission, repair and internal recall

**OUTPUT-UNIT.** Enumerate every Text block of each message.assistant in its original
content Vec, independent of partition paths. Its source is run, event sequence and that
original content index. Never renumber after removing tool blocks. ToolCall and
UnrecoveredCall are not text for this check. A selected Text copy reuses the result only
after exact source, scheme and decoded text comparison. Unselected Text is still checked
for the emission contract and repair, but produces no filing. Accepted siblings remain
ordinary filings even if another block is rejected. [CLASS-3, CLASS-9, CLASS-10,
STORE-3, TAP-1]

**DECISION-BARRIER.** Do not dispatch an emitted tool request, start the next decode
round or close a successful answer until the current generation's Text checks and any
required repair have a recorded disposition. The generation is already in the argument.
This check does not wait for ordinary accepted statements to finish filing, except where
the pair contract requires a send receipt before dispatch. Use the declared
decision_wait only while a classifier result or required filing receipt blocks
progression. Arm it when that wait starts and disarm it when the required disposition is
recorded. It does not run while the decoder generates a correction or the gate executes
a tool. A later correction's check is a new wait. Partial traffic, unrelated results and
intermediate receipts within one barrier never restart its deadline. Normal background
filing has no new age deadline. Reason: late rejection cannot repair an answer already
delivered as successful, while a blocked model must have a finite wait under CLASS-8.
This is delta C02-3, not a redefinition of gate_wait or cut_wait. [CLASS-8, CLASS-9,
CLASS-6, PAIR-1]

**REPAIR.** RepairLedger is a passive harness primitive. begin(generation) owns a repair
queue of rejected Text source IDs in original index order. Each context holds original
source, current correction sources, failed_corrections and the measured cap.
request_restatement(source) returns a private RestatementRequest with
{source,attempt,instruction:TemplateExpansion}. It authors and records the certified
restatement loop.instruction before returning the grant. The caller may pass its text to
a decoder, and then submit observe_correction(source, effective_generation) with the
complete checked result. The ledger never calls the decoder. The instruction names the
held source in its metadata and reaches the trace before the next decode call. Its
rejected text is already in the current turn's argument. It is held there for this
repair, not copied into a template or refiled. No cut occurs inside this turn. A
correction is an ordinary message.assistant and each Text block goes through
OUTPUT-UNIT. [CLASS-9, FLOW-8, MARK-4, October 18 R7]

Before the first restatement request for that original source, author state.flow with
phase repair_requested, source_id, failed_corrections zero and retry_cap. Emit that
phase once per repair context. Later failed attempts update that context and its result
records, without creating a second repair operation for the same source. The final cap
event is repair_exhausted. [CLASS-9, COST-5, FALS-2]

| Repair state | Event | Action and next state |
| --- | --- | --- |
| CheckingGeneration | All Text accepted | Return EffectiveEmission to the caller |
| CheckingGeneration | One or more correct defects | Hold actions, enqueue sources, NeedRestatement |
| NeedRestatement | Request recorded | Return RestatementRequest, AwaitingCorrection |
| AwaitingCorrection | At least one Text block, all correction Text accepted, no attempted calls | Resolve that source, advance queue or release held generation |
| AwaitingCorrection | Correct defect, correction attempts a call, or empty correction under EMPTY-CORRECTION | Record defect/protocol refusal, increment failed_corrections once |
| AwaitingCorrection | Failed corrections below cap | NeedRestatement for the unresolved source |
| AwaitingCorrection | Cap reached | Record repair_exhausted, account conforming requests without dispatch, return RepairExhausted, no successful answer grant |
| Any repair state | Stop or operator leave | Cancel ledger, retain dispositions, no successful answer grant |
| Any repair state | Component failure or an armed decision wait expires | Foundation halt, no dump and no action dispatch |

The initial defect is not a failed correction. Every new correction pass counts at most
once for that source, even with several rejected blocks. A correction may contain
several Text blocks, all checked and filed normally when accepted. A call inside a
correction is recorded but not executed, since this pass was asked to restate the held
block. That protocol refusal is not component failure. Accepted correction text is never
erased from state. Rejected text owes no filing. Original and correction remain on the
trace, and training-pair analysis remains external. [CLASS-8, CLASS-9, CLASS-10,
SCOPE-1]

**EMPTY-CORRECTION.** The [2026-10-21 operator ruling][empty-correction-ruling]
counts a correction with zero Text blocks and no attempted call as one failed
correction for its current rejected source. It neither resolves that source nor
contributes an empty replacement to EffectiveEmission. Record the complete correction
as its ordinary message.assistant, then record RefusalRecord::ProtocolRepair with case
empty_correction. Its detail is exactly
`correction contains no Text block or attempted call`.
source_id names the current rejected original Text. The refusal's
causal_parent names the empty correction's message.assistant sequence, and its turn is
the same open turn. Do not invent a source block inside the empty correction. Reason:
an empty correction supplies no replacement for the unresolved statement. Counting it
under the existing cap gives that repair a finite outcome without erasing the source.
[CLASS-9, CLASS-10, FLOW-3]

After that refusal is recorded, increment failed_corrections once and follow the same
below-cap or cap-reached row. This is an operation refusal, never non_ste100 fabricated
without a checker result and never component failure. It creates no statement, binding
or pair. A correction containing a ToolCall or UnrecoveredCall follows correction_call
instead, even if it contains no Text. A Text block whose text is empty still takes
OUTPUT-UNIT's normal classifier path and its recorded non_ste100 outcome. This rule
applies only to a requested correction. An original generation with no Text retains
the existing OUTPUT-UNIT and DECISION-BARRIER behavior.
[CLASS-8, CLASS-9, CLASS-10, FLOW-3]

Every conforming request in a forbidden correction pass still takes Spec 05's start and
terminal path, ending not_dispatched/pass_purpose. This includes a valid recall binding.
Correctly rejected or unassociated call blocks have no accepted send and do not acquire
a pair by inference. On repair exhaustion, conforming requests held from the original
pass end not_dispatched/repair_exhausted. Discarding their proposed execution does not
discard their ledger account. [CLASS-9, CLASS-10, PAIR-1, PAIR-4]

**EFFECTIVE-EMISSION.** After repair, expose a private EffectiveEmission to the caller.
Its fields are source:SourcePosition, finish:weaver_types::Finish,
statements:Vec<StatementRef>, requests:Vec<PairStarted>, text:String and
attempted_call:bool. source and finish are the original generation's. requests contains
the pending model request records in Spec 05's dispatch order, not receipts or
permission to execute them. Review requests are never model records. text joins the
effective statement spans with a single LF. attempted_call is true if the original parse
found ToolCall or UnrecoveredCall, or the effective bindings request recall. Replace
each rejected original Text in that logical list with its successful correction's
bindings, in correction-block and binding order. Accepted original siblings keep their
positions. A rejected correction contributes nothing to the list. Physical argument
Messages and trace events are never edited by this substitution. For a held external
call, the replacement still meets TOOL-STATEMENT's one-statement rule. [CLASS-9,
CLASS-10, STORE-3, OUT-1]

Text-only plan or summary consumers use this complete logical list, including
corrections from later emissions. They cannot select convenient accepted sentences from
failed attempts. The store verifies that each referenced statement is held, while the
harness owns the private repair proof. The eventual client answer uses the same
corrected text order. Reason: repair changes what may be acted on without rewriting the
record or forcing a valid correction back into its old source ID. [CLASS-9, PLAN-1,
STRAT-2, FLOW-3]

**REPAIR-REFUSAL.** RepairExhausted is {source:SourceBlockId,
failed_corrections:DecimalU64,retry_cap:DecimalU64}. The ledger records state.flow phase
repair_exhausted before returning it. This is a conformance outcome, not component
failure. It grants no successful response or action. The caller uses the existing
stopped/refused surface, with run and turn only when answering an open turn. The service
remains available for another input. Lifecycle callers retain their existing earlier
deadline. How they schedule correction passes belongs to the loop. [CLASS-9, COST-5,
FALS-2, R20-12]

**TOOL-STATEMENT.** Keep Message and ToolCall unchanged. For a recovered ToolCall, the
immediately preceding Text block supplies its STE100 request statement. Require one
accepted Binding in that block, whose verb is not a recall key. Its source ID and
binding ordinal zero identify the send, and the original ToolCall block supplies name
and executable arguments. The latter are not misrepresented as STE100. Do not dispatch
without the statement and the pair contract's filing acknowledgment. Missing or
ambiguous association is an operation refusal with source evidence, not a fabricated
binding or a component halt. Spec 05 owns the typed pair record, the copied call payload
and its result. Reason: the statement supplies verb and noun while the execution payload
preserves what the model asked the tool to run. [CLASS-3, STORE-2, PAIR-1, PAIR-3]

A repair of that Text block must yield one accepted statement before releasing its held
call. It can change the statement, but cannot silently replace the original name or
arguments. The system prompt and qualification must test this association. It does not
prove that a shell command safely implements its description. Empty text before a call,
bare calls and UnrecoveredCall never acquire a successful send by inference. Their
visible refusal leaves the agent available to the user. [CLASS-3, CLASS-9, PROMPT-3,
PAIR-1]

**RECALL.** A model-output Binding whose verb is one of scheme.recall_keys requests the
noun-history action. It must have exactly one noun and no governing condition. This is
an inward call recognized from the classified statement, not a new `sh` tool-set entry
or a new gate tool. `weaver-internal::recall::query(subject)` is a pure constructor
returning a borrowed RecallQuery or EmptySubject. The harness owns the state socket,
executes the foundation's noun query and records its answer as recall. The helper owns
no socket, clock or store. The selected request statement still enters state by the tap
and file seam. Reason: READ-3 fixes the action and lets the model supply only the
subject, while R10 preserves other classification consumers. [READ-1, READ-3, SEAM-3]

The internal call uses the pair amendment's send/return accounting and the store's
bounded read result. A correct bad recall shape or refused read becomes that call's
typed refusal, never an empty successful history. Its result can be placed only in a
later decode round, through the existing tool-result grant. It supplements the automatic
retrieval floor. This adds one framework internal member and amends the calculator-only
cap explicitly in section 8. It does not advertise a callable schema or enlarge the
merged tool registry beyond sh. [READ-2, READ-3, PAIR-1, CAP-4]

The exact constructor, borrow and refusal representations are INTERNAL-QUERY and
HARNESS-RECALL in the [internal amendment][internal]. That amendment also names the
harness dependency-set change and its instruments. It carries no store implementation
into the library. [READ-3, SEAM-3]

## 7. What is enforced, and by which instrument

**QUALIFICATION.** Protocol fixtures and doubles test mechanics. Production conformance
is a separate pass against the pinned corpus. WeaverAnalysis fixes candidate sets,
calibration/held-out split, task identities and metric definitions before calibration.
It freezes accuracy/obstruction thresholds and chosen limits before the held-out run.
The held-out set cannot tune them. Measure exact verb, noun, condition and span
agreement, false acceptance/rejection, template proof coverage, repair yield, gate wait,
decision wait, cut wait, filing lag and throughput with both directions active. Compare
decoder candidates across all CLASS-7 duties. Select the qualified tuple by the declared
task-completion and cost ordering, not by an unstated post-hoc threshold. [CLASS-6,
CLASS-7, FALS-2, FALS-5, COST-4]

That pass certifies the stated tasks and tuple against frozen criteria. It does not
prove all future inputs conform, replace safety screening or qualify another model.
O02-1 and O02-2 must exist before production cutover. The double is never a fallback for
a serving declaration. [CLASS-4, CLASS-7, PROMPT-2, FLOW-7]

| Claims | Instrument | Protective change that must fail |
| --- | --- | --- |
| PROTOCOL-OWNER, PORT | Manifest, fd-count and fork/exec integration tests, loop-thread review | Link an organ into harness, give the duty a store/gate fd, or replace the legacy label consumer |
| SCHEME, CUSTODY, DECLARATION | Strict-codec and root-custody fixtures, load digest and immutable dictionary tests | Change artifact after inventory, accept an unknown key, or invent a runtime verb |
| PARSER, CAPACITY | Normative boundary and long-name corpus with a deliberately undersized adapter | Call capacity a language defect or file an accepted prefix |
| MODEL, BINDING | Model-adapter shape tests plus exact-span held-out corpus | Return sequence scores, accept NaN, select a condition's verb or case-fold a noun |
| CHANNEL | Real SEQPACKET tests at exact datagram and whole-message bounds | Skip total/offset check, accept MSG_TRUNC, or use an incomplete reply |
| PORT, PORT-SERIALIZATION | Single-thread pump with held output and later input | Submit two requests or omit queue time from a model-blocking bound |
| TEMPLATES | Grammar-inclusion negative examples and compile-fail typed-slot fixtures | Certify samples only, allow arbitrary String slots or use an unlisted template |
| SEED-CERTIFICATE | Offline produced certificate and shared TYPES-SEED admin/member fixtures | Alter a prompt byte, binding, scheme bytes, expected checker or duty ID and still seed |
| INPUT, REUSE | Gate-to-worker integration with byte/UID substitution, queued input, delayed check and held trace capacity | Parse content in gate, open a turn early, trust payload origin, reuse changed text, start the check clock on queued arrival or after input.offered records |
| OUTPUT-UNIT | Mixed Text/ToolCall messages with sparse selected indices | Renumber indices or skip checking unselected Text |
| DECISION-BARRIER, REPAIR | Multiple-block generation, delayed rejection, repair cap, stop/leave and fake-clock cases | Deliver success early, dispatch before acceptance, omit cap evidence or halt on correct rejection |
| EMPTY-CORRECTION | Requested empty correction below and at cap, held refusal recording, empty original, empty Text and call-only correction fixtures | Resolve the rejected source with no text, count an attempt twice, continue before the refusal is recorded, fabricate a checker result or change the original-generation rule |
| TOOL-STATEMENT, RECALL | Pair and read-port doubles, shell schema regression, real pure helper tests | Bind unrelated text, execute a correction call, register recall at gate or omit the internal return |
| QUALIFICATION | Review of frozen protocol plus held-out qualification run | Accept a tuple whose thresholds were changed after its held-out results |

Every behavioral invariant above must fail under the named removal or inversion. Type
properties use compile-fail or manifest checks. Review-only claims state the review
evidence and cannot be marked passed by a green codec test. Phase A carries the ports,
framing, scripts, tables and negative instruments against the double. Phase B supplies
the normative grammar, model adapter, certificate producer and qualification. Phase A
can be reviewed and merged as specification work but cannot clear the serving cutover by
itself. [FLOW-7, CLASS-7, PROMPT-2]

EFFECTIVE-EMISSION is checked by a multi-block original with accepted siblings, several
rejected corrections and a final valid correction in another emission. Only the ordered
effective StatementRefs reach plan/summary consumers. Removing the private repair proof
or admitting one failed correction into that list must fail the fixture. Pair spies also
require terminals for conforming calls barred by correction purpose or repair
exhaustion, with no gate dispatch. [CLASS-9, CLASS-10, PAIR-1, PLAN-1, STRAT-2]

## 8. Owning-document amendments and foundation deltas

**AMENDMENTS.** The following replacement/addition text is part of the document act,
before implementation. The old label contract remains in force for labels. Its
optionality and retirement do not apply to the new mandatory STE100 seam. Every owning
section also carries the transition sentence from section 0. Reason: an implementer must
not choose between two incompatible contracts or write missing charter amendments inside
a code PR. [FLOW-7, CLASS-8, October 18 A1, R10]

| Owning document and clause | Text to add or replace |
| --- | --- |
| weaver-spu-PRD sections 6 and new 16 | Add a separate STE100 seam between harness and SPU. The SPU checks the fixed language instance and returns complete bindings, a correct defect or component failure. This duty is mandatory for serving and holds neither policy nor state. Section 15 continues to govern the optional label scorer. |
| weaver-spu-Spec sections 1 and new 12 | Add the third binary target and its owned module. Add PROTOCOL-OWNER, PARSER, MODEL, BINDING, CAPACITY, CHANNEL, PORT and TEMPLATES with their state tables and instruments. Move the current Open elections section 12 to 13. Section 11 and its label binary remain unchanged. |
| weaver-types-PRD section 2.3 and Spec sections 2 and 4 | Add Ste100Instruction, scheme bootstrap/readiness/refusal types, template types and certificate validation. TYPES-CLASSIFIER remains the request/reply definition. The legacy label trio is neither removed nor reinterpreted. |
| weaver-harness-PRD sections 2 and 4, Spec sections 1, 2.1 and 6 | Add the STE100 duty and pump-owned port, typed template construction, input state table and repair barrier. Permit the pure weaver-internal recall constructor as a member dependency. Legacy Ports::classify retains its separate semantics through the shared pump. |
| weaver-trace-PRD section 3 and Spec section 3 | Keep foundation ste100.result for exact bindings and refusal for refused operations. Extend state.flow with the repair phases and fields below. Legacy classify.request/classify.output remain label traffic. |
| weaver-admin-PRD sections 2 and 4.1, Spec sections 4, 6 and 9 | Inventory the protected scheme, ste100-binary and qualified instruction, carry exact bytes at enter and validate the seed certificate. Admin runs no conformance model. All load work consumes the existing remaining load budget. |
| weaver-gate-world-contract sections 2 and 3 and gate Spec section 4 | At foundation cutover retire the role system seeding line. Harness refuses failed input checks before a turn, returning kind/refusal reason with no run or turn. Gate relays those lines unread and keeps its existing credential and size checks. Named repair-exhausted closes answer an opened turn. |
| weaver-internal-PRD section 2 and Spec sections 1, 2 and 5 | Add the pure noun-query constructor under READ-3 as the second framework member. It accepts only the subject and has no dependencies or I/O. Harness dispatches the query on its existing store seam. No new shell or gate registry entry is created. |
| harness loop documents and Rust worker contract | After cutover all serving input uses AcceptedInput, every loop instruction is a certified expansion and every model generation follows DECISION-BARRIER. No loop synthesizes the boot system prompt or dispatches a pending/rejected call. |

The complete [harness-SPU STE100 contract][ste100-contract] sits beside the owning
amendments in docs/crates/state-management/ under R20-14. It
governs admission, requests, whole outcomes, lag, failure, custody and pass meanings
without electing transport. CHANNEL owns the transport representation. The [SPU
amendment][spu] supplies new section 12's duty, launch, model-instance ownership and
instruments. TYPES-STE100-ADMISSION supplies the shared bootstrap/readiness structs.
Charter section 4 supplies the exact SPU, types, harness, admin and internal duties.
These are authored clauses, not work left to an implementer. [CLASS-2, CLASS-8, FLOW-4,
SEAM-2]

| Delta | Earlier owner and required change |
| --- | --- |
| C02-1 | TYPES-CONFIG/ENTER and TRACE-LOAD add Ste100Instruction, exact scheme bytes and executable/model identities. flow.scheme keeps its field names and gains the content-derived scheme_id rule. |
| C02-2 | TEMPLATE-CHECK accepts canonical TemplateSpec text for template targets, bindings:[] on accepted template replies, and complete inventory bytes in load. Text targets still require nonempty bindings. |
| C02-3 | HARNESS-CLASSIFY/CLOCK/PUMP add decision_wait, decision_bound and blocked_stage decision. Each result/receipt barrier has one absolute deadline, disarmed during decode or tool execution and never reset by partial traffic. |
| C02-4 | FlowObservation adds repair_requested and repair_exhausted phases, failed_corrections and retry_cap only for those phases. Repair records require source_id. Other phases reject those fields. |
| C02-5 | RefusalRecord gains ProtocolRepair {source_id,case,detail}, case correction_call, empty_correction, missing_tool_statement or ambiguous_tool_statement. These use kind refusal and authorize no filing or halt. EMPTY-CORRECTION defines the added case. TYPES-INPUT defines source_id for each case. |

The foundation port's logical request/reply shapes are unchanged. C02-1 through C02-5
now have their named owning clauses in the assembled foundation. Spec 05 supplies inward
pair accounting, and 07 shares the scheme's step:<local_id> condition spelling. The
final consumer and finding checks still assess those joined texts. This list is not a
runtime-conformance claim. [CLASS-3, CLASS-9, PAIR-1, PLAN-3]

## 9. Open elections and review evidence

**OPEN.** The following are dependencies, not silently chosen production values. Reason:
the PRD assigns model choice, budgets and qualification to measurement. [CLASS-6,
CLASS-7, CLASS-9, PROMPT-2]

| Item | Method and closure evidence |
| --- | --- |
| O02-1 | Supply the pinned STE100 edition and lexical rights, normative grammar/compiler with rule correspondence, computing dictionary, condition/StepId grammar and negative corpus. Qualification must show its coverage before the scheme can pass. |
| O02-2 | Compare compatible span-model adapters, model digests and instance counts with both directions active under the named tuple. Include the operator's stock-encoder candidate. Select and freeze the artifact and runtime limits before held-out evaluation. No current label head is assumed sufficient. |
| O02-3 | Measure repair yield per additional attempt and task obstruction, then freeze repair_cap and gate/cut/decision waits before the held-out run. Record scheduling and transport contributions. |
| O02-4 | Qualify max_wire_bytes against the largest supported exact source blocks, complete binding answers, scheme and inventory messages, with memory pressure included. The complete admin enter must also fit its separate unchanged MAX_ENVELOPE_BYTES bound. No measured number is invented here. |
| O02-5 | Produce the separate PROMPT-3 document, its review, prompt certificate and the WeaverAnalysis producer/verifier fixtures. No prompt is written by this Spec. |

The foundation's HARNESS-CLASSIFY replacement addresses the missing mandatory production
result port and preserves the legacy classifier side identified by R10. It also supplies
the template, input, output and seed certification seams absent from the original draft.
It is not yet a production qualification result.

[prd]: state-management-primitives-prd-v0_6.md
[foundation]: state-management-foundation-Spec.md
[transition]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
[rulings]: state-management-foundation-Spec.md#0-what-this-document-is
[internal]: weaver-internal-Spec-amendment.md
[spu]: weaver-spu-Spec-amendment.md
[ste100-contract]: weaver-harness-spu-ste100-contract.md
[empty-correction-ruling]:
https://github.com/toddwbucy/WeaverAgent/pull/213#issuecomment-6330669954
