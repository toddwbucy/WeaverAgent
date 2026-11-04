# weaver-spu Spec: LoRA fixture loading amendment

**Status:** MERGED.

**Landing PR:** #475

**Version:** 0.1
**Date filed:** 2026-11-04
**Document ID:** `weaver-spu-lora-loading-Spec-amendment`
**Parent:** `weaver-spu-PRD`
**Editorial:** Per the Working Rules.

## 0. Authority and scope

**LORA-0.** This amendment implements [#473][epic] item 2's fixture mechanic.
The operator requested an SPU amendment before code on 2026-11-04.
The [organ audit][audit], merged in #474, identifies this gap in section 6,
group E. [#435][conversion] retains adapter conversion and target coverage.

Amend SPU Spec sections 1.2, 3, 4.1, 4.2, 5, 6, 9 and 10 as mapped below.
The grounds are SPU PRD sections 4.1, 4.2, 7, 13.3, 13.6 and 14.
These clauses authorize the SPU's fixture load and application mechanic after
merge. They do not claim that the mechanic exists at the source baseline.

| Clause | Destination | Addition |
| --- | --- | --- |
| LORA-1 | Spec 1.2 and 3 | One optional adapter input to SPU fixture admission |
| LORA-2 | Spec 3 and 9 | Pinned acquisition, identity checks and refusal accounting |
| LORA-3 | Spec 3, 4.1 and 5 | Backend ownership and complete adapter loading |
| LORA-4 | Spec 4.2 and 9 | Context attachment before the first forward |
| LORA-5 | Spec 3 and 4.1 | Release and failure cleanup |
| LORA-6 | Spec 6 | Fixture evidence, separate from production measurement |
| LORA-7 | Spec 1.2 and 10 | Mechanical tests and the real 4B application proof |
| LORA-8 | Spec 10 | Qualification cells and the limit of a fixture pass |

The baseline is main at `aa863f5bb3cfcee2f0c444bd075283bf94ccec79`.
The dependency pin is `toddwbucy/llama-cpp-rs` at
`ecce255bcb14dd6d88f184cc8776c23a85afafeb`, as recorded by Cargo.lock.
PR #450 is unmerged evidence and supplies no implementation to this amendment.

The fixture holds one fixed adapter for one residency. The normal production
caller continues to request no adapter. No production declaration key, wire
member, role socket or adapter choice per request is added here. Those belong
to item 3, with the corresponding record and replay changes. Item 4 waits.
This boundary avoids a temporary public single-adapter binding before the
declared-set design is specified.

The ThinkPad proof is the 4B lane. Its fixture source remains the operator's
choice. The 30B family entry and R1 emitter adapter test remain on olympus.
This document supplies no download authority, fixture artifact or drift limit.

## 1. The fixture admission input

**LORA-1.** Add an optional adapter request to the SPU-owned admission mechanic.
Keep the existing base admission and the fixture admission on one code path.
The normal caller supplies absence. The SPU fixture driver supplies presence.
Other crates neither link this mechanic nor receive a new configuration field.

The base instruction retains its model binding, assigned devices, readout
election and effective parameters. The optional adapter request supplements
that instruction with exactly the four members below. It does not copy or
override those base values.

| Member | Meaning and validation |
| --- | --- |
| Artifact reference | One GGUF LoRA file. It must be named explicitly. No directory search, remote locator or conversion runs during admission. |
| Adapter SHA-256 | Digest of that file's complete bytes, expressed as 64 lowercase hexadecimal characters. It is not the source PEFT tree digest. |
| Expected base weights hash | The base's existing SPU Spec 3 BLAKE3 identity, expressed as 64 lowercase hexadecimal characters. It names the exact runtime artifact. |
| Scale | An explicit finite f32 multiplier passed to the backend. No omitted value receives a default. Zero is representable but cannot prove application. |

These are fixture input semantics, not a production serialization schema.
Absent adapter means base-only decode and no adapter acquisition or attachment.
A present incomplete request fails before admission. Invalid digest syntax or
non-finite scale also fails before admission. Do not replace presence with
absence after an error.

The expected base hash pins the runtime GGUF, including its quantization.
It does not prove the adapter's training ancestry. The fixture's provenance
must link the training base, source adapter and converted runtime artifacts.
The operator chooses that fixture, and #435 records its conversion.

## 2. Acquisition and pre-load judgment

**LORA-2.** Extend the standing five-step admission sequence to the adapter.
The SPU opens and judges it as the same identity that reads the base model.
The adapter has the SPU PRD 7 ownership and access requirements: owned outside
the agent uid, readable by that uid and not writable by it.
The fixture driver cannot bypass these checks by loading through another uid.

Resolve and pin the adapter before loading. Retain the checked file owner
through every backend read. Hash and load through that same pinned file.
Do not hash one pathname target and later load a replacement at that name.
The existing base pinning and canonical BLAKE3 manifest rule stay intact.
The adapter SHA-256 does not replace or extend the base hash algorithm.

Read the GGUF metadata and tensor directory before device allocation.
Require an ordinary LoRA adapter for the resolved base architecture.
Reject malformed tensor pairs, unknown target names and incompatible shapes
that the header and tensor directories can already establish.
Reject adapter features the fixture mechanic does not implement.
In particular, token-triggered aLoRA and tensors silently skipped by the
backend are not qualified by an ordinary LoRA fixture.

Judge the assigned devices for the base, adapter allocations and working
headroom. Keep the existing device assignment and no-fallback rules.
Include adapter tensor storage in the preflight need. Retain the existing
staged headroom parameter, recording the supplied allowance and its basis.
An unmeasured adapter overhead remains a qualification gap, not a zero cost.
Use the current driver readings for room, not a stale reservation ledger.
Hash both pinned artifacts before device allocation, in the existing final
free admission stage. Compare the adapter digest and expected base identity.
Do not begin device loading when either identity differs.

The admission account identifies the failed artifact and stage. It includes
the process uid and gid under the existing SPU Spec 3 error-account rule.
No parser in admin or the harness acquires a second artifact-reading duty.

| Failure | SPU admission result |
| --- | --- |
| Missing adapter path | ArtifactUnresolvable, with the adapter path and lookup stage |
| Denied access, bad custody, unreadable or malformed bytes | ArtifactUnreadable, with the failing acquisition stage |
| Adapter digest differs from the requested artifact | ArtifactUnreadable, with an explicit digest-mismatch account |
| Expected base identity differs, or the backend cannot serve the declared pair | DeviceCannotAdmit, with the compatibility reason rather than a claim of insufficient memory |
| Base plus adapter and headroom cannot fit | DeviceCannotAdmit, with device capacity, free space and the complete need |
| Weight or adapter allocation fails after device work begins | DeviceCannotAdmit only after cleanup, with the backend's account |

These are the existing internal admission result mappings. No new lifecycle
wire case is added. The ordinary production caller cannot exercise an adapter
request under this amendment. The fixture records the detailed stage so that
the broad result is never presented as a more specific cause.

## 3. Backend load and ownership

**LORA-3.** The GGUF backend loads the base, then the requested adapter against
that resident base. Use the pinned backend's LoRA load operation.
The resident model owns the adapter handle and its verified identity.
It retains the scale with that identity. A context can borrow this residency
but cannot outlive it or refer to an adapter from another model.
Preserve the converted adapter's rank and alpha semantics in the backend.
The fixture scale is a separate multiplier, not a replacement for alpha.

The pinned Rust wrapper exposes `LlamaModel::lora_adapter_init` and
`LlamaContext::lora_adapter_set`. Their existence is source evidence of an API,
not evidence that this fixture or every target tensor works.
The C backend checks architecture, target presence and tensor shape during
load. It also has paths that skip normalization tensors or select CPU buffers.
Therefore a successful handle alone does not satisfy this amendment.

The fixture elects the pinned backend's log callback as its placement observer.
In an isolated fixture process, install llama_log_set through llama-cpp-sys-2
before admission. Capture DEBUG and WARN records as well as the other levels.
Keep the capture alive for the isolated process's entire lifetime, including
model teardown and every in-flight callback. Do not replace it during the run.
The callback copies the supplied text and level. It calls no backend function
and cannot unwind across the C boundary. Run no unrelated load in that process.

Preserve the raw callback records in the fixture report. Parse the pinned
llama_adapter_lora_init_impl records that map each target to its buffer type.
Require one placement observation for every requested target pair. Match each
buffer to the declared backend placement and assigned devices. Missing,
ambiguous or unmapped observations fail qualification, rather than implying
device placement from the absence of a warning.

The pinned warning includes "cannot use buft" and "fallback to CPU".
That warning fails the fixture even when the adapter load returns success.
The per-target DEBUG record also exposes the selected buffer. A changed backend
pin requires this observer to be requalified against its emitted records.
No production logging or measurement shape changes with this observer.

Account for every adapter tensor and the buffer placement it receives.
Every required pair must be loaded for a target the selected graph uses.
An ignored target, empty adapter or undeclared placement fallback fails the
fixture. Backend support for one matrix path proves no other target class.
The Mamba in_proj to ssm_in path remains a #435 measurement until exercised.
The pinned mamba-base.cpp routes ssm_in and ssm_out through build_lora_mm.
That source fact supports the trial. It does not establish the converted
fixture's tensor names, output change or accepted numerical agreement.

Confirm fixture admission only after the base and all adapter weights are
resident under the selected backend. Admission does not yet claim that a
decode context has the adapter attached. LORA-4 owns that separate fact.
With absence, preserve the existing base-only load and result.

The 4B family must resolve through the same family registry as normal decode.
Do not build a fixture-only family selector or hard-code a second template.
Any missing 4B registration needs its measured artifact facts in the later
implementation act. This amendment asserts no new 30B family support.

## 4. Context attachment and fixed residency

**LORA-4.** Attach the resident adapter with the declared scale when the GGUF
context is created. Attachment must succeed before any identity-prefix token
is decoded. The same context then serves generation and diagnostic readouts
selected by the fixture. A base-only prefix followed by adapted generation
does not satisfy the rule.

Preserve the adapter through generation, cancellation, flush and elision.
If an existing cache operation recreates its backend context, reattach before
the first replayed token. It must not rebuild the kept prefix with the base
alone. The adapter and scale remain fixed until release.

An attachment failure is a decode-engine failure. It produces no successful
Open result and no fallback generation. Follow the existing decode failure
and owned-residency cleanup path. Do not add a retry, second Open, adapter
replacement or independent timeout.

| SPU position | Event | Required result |
| --- | --- | --- |
| Empty | Admit with no adapter | Existing base-only admission |
| Empty | Fixture admit with an adapter | Validate and load the pair, or refuse after complete cleanup |
| Resident, unopened | Open | Create context, attach adapter, then decode prefix |
| Resident, unopened | Attachment failure | No opened session, no prefix forward, existing failure cleanup |
| Open, at rest | Generate or re-feed permitted by the existing session | Use the same adapter and scale |
| Open | Existing cancel, flush or elision | Preserve the adapter and the existing operation's ordering |
| Resident | Release | Destroy contexts and adapter resources before release completes |
| Any | Request to change adapter inside the residency | No such fixture operation or wire exchange exists |

The pinned llama.cpp attachment is per context, not per sequence.
The Rust lora_adapter_set wrapper supplies exactly one adapter and replaces
the whole attached set. Its lora_adapter_remove wrapper ignores the supplied
handle and clears the entire set. Neither wrapper selects one member of a
larger set. This pass needs no remove call during a residency.
Item 3 must specify its use of the multi-adapter C API or a changed wrapper
before relying on a set. It cannot inherit this wrapper as a set manager.
This pass adds no batching across adapters and no scheduling policy.
The existing one-session and one-active-generation rules remain in force.

## 5. Release and failures

**LORA-5.** A loaded adapter belongs to the same residency as its base.
This pass elects model-owned cleanup. Stop serving and destroy every attached
context. Discard the inert Rust adapter handle, then drop LlamaModel.
Its Drop calls llama_free_model. The C model destructor deletes its registered
adapters and releases their allocations with the base's other resources.
Complete release only after that destructor returns.
Do not add an independent llama_adapter_lora_free call in this pass.

A partial adapter load must leave no successful resident object or held
allocation on the refused residency's account. An error after context creation
must destroy that context before its model. A base-only retry inside the same
attempt would hide the failure and is forbidden.

LlamaLoraAdapter has neither a lifetime parameter nor a Drop implementation.
Dropping that Rust handle alone frees nothing. The SPU's ownership must make
the model outlive the handle and every attached context.
A successful C call is insufficient evidence for that Rust ownership claim.

The fixture records memory after backend initialization, after base load,
after adapter load and after full residency destruction. Pair an adapted
cycle with a base-only control using the same device and backend settings.
The final comparison is to the empty-residency baseline, since this release
frees the base too. Record driver caches and unrelated device activity
separately. Do not call an unexplained retained allocation a cleanup pass.
Fault-injection tests also require balanced owned-resource releases and
context-before-model destruction. Global device memory alone proves neither.

## 6. Fixture evidence and the production boundary

**LORA-6.** Write the fixture report outside the repository's tracked tree.
Give each attempt an identity and record these facts for both successful and
failed attempts. Mark unavailable observations absent, not zero.

| Evidence | Required content |
| --- | --- |
| Source and build | WeaverAgent commit, llama-cpp-rs pin, backend features, host and effective device assignment |
| Artifact provenance | Training-base identity, runtime-base BLAKE3 identity, source adapter identity, conversion command and version, converted adapter SHA-256 |
| Adapter operation | Scale, stored alpha, target names, ranks, loaded tensor pairs, raw placement callback records, parsed per-target buffers, and attachment outcome |
| Exact input | Rendered input bytes, template and tokenizer identities, input token IDs and effective generation settings |
| Output and resources | Output token IDs and text, stop reason, elapsed load and decode times, and measured allocation or peak-memory evidence |
| Refusal and cleanup | Failed stage, typed result, backend detail and release observations, including partial-load and attach failures |
| Comparison | Base-only and adapted outputs, the Python reference, conversion and quantization differences, and the operator's drift ruling if one exists |

The report must distinguish loaded, attached, output changed and accepted
Python/SPU agreement. None implies the next. With drift still open, report
the measurements and label agreement unruled. Do not invent a tolerance or
report a parity pass because the strings happen to look similar.

No production model.measurement or Open evidence shape changes in this pass.
The current Open measurement has a closed four-member shape. Replay identity
and its comparisons also assume the existing base identity. Item 3 must amend
these consumers together before a production adapter binding is admitted.
This separation leaves the trace-floor question open while retaining the
fixture evidence needed to judge the mechanic.

## 7. Instruments and the 4B proof

**LORA-7.** The implementation supplies the following instruments inside the
SPU's existing test surface. A fake backend can prove ordering and refusal,
but cannot stand for the real adapter application proof.

| Claim | Instrument | Deliberate change that must fail |
| --- | --- | --- |
| Absence is base-only | Compare the existing path with absent adapter input and count adapter operations | Acquire or attach any adapter under absence |
| Identity and custody | Wrong digest, wrong base, denied uid, writable or agent-owned artifact, and pathname-replacement fixtures | Skip a check or reopen an unchecked pathname |
| Cheap checks precede loading | A recording driver with input that fails before allocation | Allocate before the identity and compatibility judgments |
| Complete loading and placement | Pair, shape, target and skipped-feature fixtures plus the callback observer. Force CPU fallback while load succeeds and require fixture failure. Also exercise missing placement records | Accept the recorded CPU fallback, infer a missing placement or ignore a requested tensor |
| Attachment precedes prefix | Backend call order plus an injected attachment failure | Decode any prefix token before attachment or continue after failure |
| Residency remains fixed | Exercise generation, cancel, flush and elision, including any context recreation | Lose or change the adapter while rebuilding kept tokens |
| Cleanup | Inject failures during adapter allocation and context attachment, then inspect owned-resource release | Leak a resource, double-free or complete release early |
| Real 4B application | Fixed base, fixture, input and settings, comparing base-only and adapted output | Omit adapter attachment or disable a target contribution used by the proof |
| Production boundary | Parser and normal admission tests, plus a caller census | Admit an adapter through an unamended declaration or production call |

The real proof uses an operator-selected fixture trained on the same base.
It must change at least one output token on a pinned input under the declared
settings. The base-only control must be repeatable under those settings.
A logit difference alone is useful evidence but does not satisfy #473's
output-change requirement.

The perturbation that removes attachment must make the application assertion
fail. Do not merely assert that a handle exists or that a load log was emitted.
Name the target classes exercised by the fixture. Do not claim Mamba coverage
from an attention-only adapter or extend a 4B result to the 30B family.

Run the Python comparison with matched input tokens and recorded settings.
Record unavoidable differences, including base quantization and conversion.
The operator decides whether the measured drift is acceptable.
A successful load or changed output does not settle that decision.

The accelerator fixture is an explicit test, separate from the ordinary
hardware-free suite. A missing artifact, backend or device is not a pass.
When the operator requests the fixture run, missing prerequisites must fail
the run with their names. No test downloads or trains its own fixture.

## 8. Open cells and release limits

**LORA-8.** These cells remain open. They do not authorize substitutes.

| Cell | Evidence or ruling that closes it |
| --- | --- |
| 4B fixture source | Operator selects the compatible artifact or authorizes its production and any download, then pins its provenance and digests |
| Accepted Python/SPU drift | Operator names the comparison and tolerance after reading the pinned base-only and adapted measurements |
| Mamba target support | #435 conversion and application evidence for each relevant target class, including in_proj to ssm_in where present |
| 4B resource need | Actual base, adapter, context and device measurements, including adapter allocation and transient peak use |
| Missing 4B family facts | The selected base's metadata, tokenizer, renderer and stop behavior verified for the registry entry |
| Production adapter serving | Item 3's merged binding, request selection, role contracts, evidence and replay amendments, followed by their implementation |

The trace-floor ruling remains with the operator under item 1.
It is not a prerequisite for the external fixture report and is not answered
by producing one. No result here qualifies the full R0 composition or its kit.

## 9. Source and consumer index

These locations were read at the baseline in section 0. They locate the work
and are not claims that the new mechanic has already been implemented.

| Source | Relevant fact |
| --- | --- |
| SPU PRD 4.1, 4.2 and 7 | Admission order, release ownership and artifact custody |
| SPU Spec 3 and residency.rs | Existing hash, room judgment, admission results and resident ownership |
| decoder/gguf.rs | ResidentModel loads the base, and GgufEngine::open creates its context |
| placement.rs::OpenedSession::open | Session creation precedes prefix decode and Open evidence |
| SPU Spec 5 | Family resolution and capability declarations |
| SPU Spec 6 and main.rs::render_measurement | Production model measurement ownership |
| SPU Spec 9 | Lifecycle result mappings and decode failure behavior |
| Pinned model.rs::lora_adapter_init and context.rs::lora_adapter_set | Separate load and one-adapter set replacement operations |
| Pinned context.rs::lora_adapter_remove | Clears the entire set, not only its handle argument |
| Pinned LlamaModel::drop and llama-model.cpp::~llama_model | Model drop releases registered adapters, the mechanism elected by LORA-5 |
| Pinned llama-adapter.cpp and llama.h | Metadata and tensor checks, skipped tensor path, buffer placement and model-owned adapter lifetime |
| Pinned llama-impl.h, llama-impl.cpp and llama_log_set | DEBUG and WARN delivery to the fixture's placement observer |
| Pinned models/mamba-base.cpp::build_mamba2_layer | LoRA graph paths for ssm_in and ssm_out, separate from conversion proof |

Production identity consumers held for item 3 are listed below. The source
paths are under crates/. Their inclusion records the boundary that prevents
the fixture from being presented as production adapter support.

| Path and site | Later amendment responsibility |
| --- | --- |
| weaver-types/src/config.rs::DecoderInstruction and SpuInstruction | Declared set and role selection representation |
| weaver-spu/src/artifact.rs::weights_hash and residency.rs | Preserve base identity while identifying each adapter |
| weaver-spu/src/placement.rs::open_evidence | Adapter identity in successful Open evidence |
| weaver-spu/src/main.rs::render_measurement | Actual adapter identity for each request, including base-only |
| weaver-harness/src/open_evidence.rs | Closed Open measurement validation |
| weaver-harness/src/replay.rs::measurement_members and compare | Replay source identity and comparison of the actual adapter |
| weaver-diagnostic/src/event.rs::ReplayIdentity | Diagnostic identity carrier |
| weaver-state/src/preload_door.rs | Projection of model measurement identity for replay |

The merged [SPU placement amendment][placement] defines SPU-OPEN's closed
measurement. It must change with those item 3 consumers, not by silently
adding an extra member from this fixture.

[epic]: https://github.com/toddwbucy/WeaverAgent/issues/473
[conversion]: https://github.com/toddwbucy/WeaverAgent/issues/435
[audit]: ../../project/primitives-organ-audit-2026-11-04.md
[placement]: ../state-management/weaver-spu-Spec-amendment.md
