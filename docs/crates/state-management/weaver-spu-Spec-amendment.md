# weaver-spu Spec: placement measurement and STE100 duty amendment

**Status:** MERGED.

**Landing PR:** pending assignment

**Version:** 0.6 **Date filed:** 2026-10-21 **Parent:** weaver-spu-PRD

## 0. Scope and owning clauses

Amend SPU Spec sections 4.2, 4.3, 5, 6, 9 and 10, with the matching token types in types
Spec 4.4 and decode contract sections 2 through 5 and 8. These are Spec 06's near sides.
Section 6 adds Spec 02's separate STE100 duty to Spec 1 and new section 12, moving the
old Open elections section to 13. The label scorer remains unchanged. These clauses
apply at the [foundation cutover][cutover]. Until then, the previous serving contract
applies. [PLACE-1 through PLACE-6, R10]

At baseline 2eb5c5a2, main.rs::render_delta uses family::render_each, then the
AppendAndGenerate arm adds the generation opener and tokenizes. Session already owns
resident, prefix_len, capacity, Flush and Elide. TokenAnswer::Opened is a unit. This
amendment extends that path. It adds no harness tokenizer, new session owner, saved KV
representation or replacement decode operation. [PLACE-5, PLACE-6, DEFER-1,
weaver-spu-PRD 13.4]

## 1. Rendering and open facts, destinations sections 4.2, 4.3 and 5

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**SPU-RENDER.** Factor the existing append rendering into
family/mod.rs::render_generation(family,delta,tool_schemas,generation_opener). It
returns one rendered String or the existing MalformedDelta refusal. delta remains
canonical Vec<Message>. If tool_schemas is nonempty, append one canonical user Message
with one Text block containing those strings joined in order by a single LF. Run the
complete sequence through family::render_each once, preserving its fold and role rules,
then append the existing generation opener. With no schemas the bytes are the current
render_delta plus opener. Tokenize the complete string once through the resident
tokenizer. Do not sum independently tokenized fragments. Reason: measurement and append
need identical rendering, including schema and boundary tokens, without moving a
family's markers into the harness. [PLACE-1, PLACE-3, PLACE-5, FLOW-3]

The schema Message is an internal rendering adapter. It is not a new gate input or a
conversation event. The harness records the exact schemas as granted metadata before
their use, and model.request retains the actual rendered input. The SPU owns no STE100
check over this metadata. Identity rendering remains render_identity with no schema
Message and no generation opener. No new Family trait method or family-independent
marker syntax is needed. [FLOW-3, PROMPT-1, PLACE-1, CLASS-4]

**SPU-OPEN.** After session.open succeeds, answer Opened with prefix_tokens from
Session::prefix_len, capacity from Session::capacity, generation_limit from the resolved
max_tokens_per_turn and terminal_tokens equal to one. Use checked u64 conversion. The
current renderer supplies one promoted terminator token under SPU Spec 4.3. A different
convention is outside this wire's admitted behavior. The harness checks its measured
head/tail budget against these facts before enter can succeed. A failed fit closes this
residency by existing failed-enter teardown, not a second Open. Reason: a resolved
OperatorTunable generation limit is an SPU fact even when no declaration key carries it.
[PLACE-1, PLACE-3, PLACE-5, PERSIST-6]

Keep the request's template identity in model.request only. The prefix's reported count
is the identity-prefix boundary, not another copy of its text. The first placement
record correlates it with the earlier boot recall. No prompt is read or filed again.
[PROMPT-1, FLOW-3, STORE-8]

## 2. Measurement protocol, destinations sections 4.2, 6 and 9

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**SPU-MEASURE.** MeasureDelta is a read-only ask on the standing decode channel. It uses
SPU-RENDER and the same resident tokenizer as append. Read resident, prefix_len and
capacity from the open session. Clamp the requested keep to
max(prefix_len,min(keep,resident)), exactly as Flush does. Answer DeltaMeasured with
those facts and the complete input token count. A proposal that exceeds capacity still
receives its measured count, since this ask does not append. Release temporary rendered
bytes and token IDs after answering. No measurement result or proposal is retained for a
later request. [PLACE-3, PLACE-5, PLACE-6]

The pass states only the size of this exact canonical proposal under the current family,
tokenizer and residency, and the clamped keep. It does not authorize a cut, certify a
selection, or prove the deployment's memory budget. Evidence is the whole request and
returned counts. A malformed family rendering refuses MalformedDelta without mutation.
Tokenization or channel failure follows the existing decode component-failure path,
never a zero count. [PLACE-3, PLACE-5, FALS-4, FLOW-4]

| SPU position | Request or observation | Action and next position |
| --- | --- | --- |
| BeforeOpen | MeasureDelta | Refuse NotOpen, unchanged |
| AtRest | MeasureDelta | Render and count with immutable session access, Measuring |
| Measuring | Count complete | Send one DeltaMeasured, AtRest |
| Measuring | Family refuses representation | Send MalformedDelta, AtRest |
| Measuring | Broken channel observed | Existing protocol failure, no successful answer |
| AtRest | AppendAndGenerate with schemas | Use the same render function, then existing append/stream/Generated path |
| Any active generation or re-feed | MeasureDelta | Refuse OutOfOrder through the existing non-cancel ordering path |
| Any | Component failure | Existing fault/teardown, no successful measure or fabricated count |

The SPU can see at-rest decode position. The harness owns the stronger between-turn rule
because only it knows the externally opened turn. It may measure an append between tool
rounds, with keep equal to resident. A smaller keep proposal, Flush or Elide requires
the harness's between-turn grant. The measurement adds no turn-close directive or second
clock. [MARK-4, PLACE-5, October 18 R7]

The count runs synchronously in the SPU's serial decode dispatcher. It reads no next
directive while measuring. The harness must not send one until the complete answer
returns. This is a sender obligation, not a claim that the SPU can infer when an unread
socket message was written. [PLACE-5, SEAM-2]

**SPU-UNCHANGED.** Measurement invokes no backend forward, decode, sample, flush,
elision, readout collection or RNG operation. It does not change the resident vector,
sampler, stop set, turn_generations or diagnostic readout state. Existing
AppendAndGenerate still checks physical capacity before mutation and makes its single
terminator resident on clean and cancelled paths. Its existing capacity stop remains.
Flush/Elide behavior and ReFeed's recorded-token path are unchanged. Reason: asking
whether content fits cannot itself alter the argument being measured. [PLACE-4, PLACE-5,
PLACE-6, MARK-4]

## 3. Harness and codec agreement, destination sections 2 and 9

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**SPU-WIRE.** Use the exact types in TYPES-DECODE. DecodeSocket's current segmented
whole-message transport, not a new socket or organ envelope, carries the request and
reply. Opened retains TokenAnswer's adjacent tagging and gains its required body.
AppendAndGenerate requires tool_schemas, even when empty. There is no alias for the old
shape after cutover. The paired harness and SPU amendments must deploy together.
Diagnostic senders use the new shape with an empty schema list when their recorded
rendering supplies none. ReFeed stays unchanged. [SEAM-2, PLACE-3, SCOPE-1]

The harness associates one outstanding MeasureDelta with one complete DeltaMeasured or
refusal. Fault emissions retain their existing non-answer role. It holds the exact
proposal until use or refusal. Between a successful measure and append, it permits only
the planned Flush or suffix Elide, or administrative cancellation. It compares Flushed's
or Elided's actual counts with the proposal, and invalidates the measurement after any
other mutation. No counter inferred from message bytes may replace the returned resident
or prefix facts. [PLACE-5, PLACE-6, FLOW-4]

**SPU-RELOAD-BOUNDARY.** Spec 06's composite ends Staged in the harness engine. The SPU
owns rendering, tokenization and cache mutation. Its ReFeed remains the diagnostic
recorded-path drive in Spec 4.6 and charter 13.14. The next ordinary AppendAndGenerate
seats the pinned tail and new delta, then samples. MeasureDelta counts that complete
delta before dispatch. Generated.resident includes input, output and the terminator,
while measurement.output_tokens excludes the terminator. Preserve those existing facts
for STAGED-APPEND's confirmation. No prefill-only serving exchange is added. Test
serving ReFeed refusal and full count agreement for complete and stopped answers.
[SEAM-2, PLACE-5, SCOPE-1, R20-15, R20-16]

## 4. Enforcement, destination section 10

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Claim | Instrument and evidence | Protective change that must fail |
| --- | --- | --- |
| SPU-RENDER | Family fixture for empty/nonempty schemas, tool content, folding and generation opener | Use separate render paths, omit schemas or tokenize fragments independently |
| SPU-OPEN | Real session plus fixed and tunable limits, empty old-answer corpus | Omit effective limit, report a guessed prefix or accept old unit Opened |
| SPU-MEASURE | Every protocol row with exact datagram/segment boundaries | Queue an out-of-order request, accept partial answer or truncate a count |
| SPU-UNCHANGED | Session/backend/RNG counter snapshot around a measurement | Perform any decode, clear a reduction, advance RNG or alter resident tokens |
| Measure/append agreement | Same proposal measured then appended, compare input count and rendered request bytes | Use another family fold or schema order on append |
| Prefix and cut | Existing Flush/Elide fixtures plus harness turn-grant test | Permit a cut below prefix or grant one inside an external turn |
| SPU-WIRE | Cross-crate enum/serde corpus and one-reader socket fixture | Accept a missing body, missing schemas, wrong reply or concurrent second ask |

These are implementation acceptance instruments, not results from this document pass.
They run for every admitted rendering family on the supported engine lane.
Measure/append equality compares the actual delta path. It never equates resident
history with a new render of all conversation messages. [PLACE-5, PLACE-6]

## 5. Measured values and external consumers

The head/tail budget, temporary render memory, measure latency and complete cut-wait
bound remain measured under the declared tuple. No new numeric timeout or capacity is
set here. WeaverAnalysis and diagnostic clients must consume the new token shapes before
this paired cutover, while their separate trace/state authority remains unchanged. C06's
mark/plan and selected-unit integrations are the harness/store consumers, not additional
duties of the SPU. [PLACE-3, COST-2, COST-4, SCOPE-1]

## 6. STE100 duty, destinations sections 1, 10 and new 12

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**SPU-STE100.** R20-3 of 2026-10-20 elects this duty in the existing SPU crate. This
Spec chooses its own third binary rather than overloading the label scorer. Amend Spec
section 1's layout and [[bin]] prose from two binaries to three, retaining the library
and both existing binaries. Section 1.2's table and tests/manifest.rs's TABLE describe
test targets, their feature gates and fixtures, not binary targets. The new manifest
instrument below pins the binary set. Amend section 11 to distinguish the score duty
from this span-binding duty. Add the binary target
weaver-spu-ste100 at src/bin/ste100.rs and the library module ste100. Its grammar,
binder and protocol dispatcher live in src/ste100/. It implements [Spec 02][protocol]
PROTOCOL-OWNER, SCHEME, MANIFEST-LOCATOR, PARSER, MODEL, BINDING, CAPACITY, CHANNEL,
PORT and TEMPLATES as
normative parts of this section. Their score labels, tie rule, complete-block results,
fragment header, state tables and refusal mapping are the definition sites, not
alternatives for the implementation to choose. The [STE100 contract][contract] governs
the new seam. Section 11's binary and label result types keep their old meaning. Reason:
the existing sequence-score head cannot return source span bindings. [CLASS-2, CLASS-4,
CLASS-8, SEAM-2, October 18 R10]

The harness worker's existing organ-launch owner forks and execs this duty using the
executable already judged and supplied by admin. Admin does not start a second
classifier implementation. The binary takes the exact trusted max-wire-bytes launch
option and fd3 in CHANNEL, with the existing SPU organ credential, stderr and reaping
treatment. It uses the shared TYPES-STE100-ADMISSION and TYPES-CLASSIFIER types.
Bootstrap chooses a compiled compatible adapter and verifies the scheme/model artifacts.
Absent backend support is unsupported_artifact, not an implicit fallback to the label
scorer or a test model. The active backend and feature selection are pinned by O02-2's
qualified artifact. No lifecycle or decode handler is exposed on this duty's port.
[CLASS-2, CLASS-7, CLASS-8, PERSIST-6]

The SPU owns MANIFEST-LOCATOR acquisition after exec, as the agent uid. Keep its pinned
artifact root and checked byte/file owners through every adapter read. The model
identity is Spec 02's manifest SHA-256 plus verification of its named files, not
Spec section 3's legacy BLAKE3 file-or-directory walk. No legacy decoder or label
binding is reinterpreted. Acquisition failure uses unsupported_artifact before Ready
and does not weaken the inherited descriptor boundary. [CLASS-2, CLASS-7, SEAM-2]

Several declared instances run inside this one process and share the exact scheme and
artifact identities. They may analyze independent sentences in the one outstanding
request. The dispatcher assembles them in source order and sends one terminal answer for
the whole block. It sends no prefix on failure and accepts no overlapping next request.
The harness alone decides the next request's priority. This is the CHANNEL/PORT state
table's Checking state, not another cross-process queue or harness thread. [CLASS-4,
CLASS-6, STORE-3]

The duty has no state, gate, trace or argument handle. It computes bindings and template
inclusion results, reports them, and waits for the next request. No result from this
duty dispatches a tool, chooses a partition or updates a store. Correct non_ste100 keeps
the checker usable. A component_failed reply ends the duty after its attempted terminal
answer, under PORT. [CLASS-2, CLASS-8, CLASS-10, SEAM-1, FLOW-4]

Add tests/manifest.rs::ste100_binary_set as a new instrument. It reads Cargo.toml and
requires exactly weaver-spu at src/main.rs, weaver-spu-classify at src/bin/classify.rs
and weaver-spu-ste100 at src/bin/ste100.rs, with the existing library retained. It does
not add binaries to the fixture TABLE. Removing or replacing a target must fail this
test. Reason: the new duty must coexist with both standing SPU duties. [CLASS-2, SEAM-2,
October 18 R10]

| Claim | Instrument and evidence | Protective change that must fail |
| --- | --- | --- |
| SPU-STE100 target and custody | New tests/manifest.rs::ste100_binary_set pins the three [[bin]] names and paths in Spec section 1, separate from the existing fixture TABLE. Dependency and fork/exec descriptor checks use the existing organ launch path | Replace or omit a binary, link an interior crate or inherit a state/gate/trace handle |
| Admission and transport | CHANNEL corpus over real SEQPACKET plus TYPES-STE100-ADMISSION fixtures | Accept a mismatched initial limit, wrong identity, legacy ready body or incomplete logical message |
| Manifest acquisition | Spec 02 MANIFEST-LOCATOR acquisition and retained-owner fixtures | Resolve a directory by guessing, follow an escaping link, reopen checked inputs or load an unreferenced file |
| Whole result and internal instances | PORT state-table rows with held/faulting sentence workers | Return a prefix, overlap requests or reorder sentence bindings |
| Grammar and model judgment | PARSER/MODEL/BINDING/TEMPLATES instruments in Spec 02 | Interpret sequence scores as spans, silently skip a grammar rule or certify examples instead of inclusion |
| Conformance versus component failure | Accepted, not_ste100, capacity, invalid-output and process-death corpus | Kill the checker for correct rejection or keep serving after a terminal component result |

The mechanical pass can use the defined test double. Production admission and
conformance require O02-1/O02-2 and their held-out evidence. The manifest, model adapter
and grammar compiler remain explicit qualification dependencies, not a claim that the
existing ModernBERT implementation can supply this result. [CLASS-7, FLOW-7, PROMPT-2]

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
[protocol]: state-management-spec-02-protocol-classifier-gate.md
[contract]: weaver-harness-spu-ste100-contract.md
