# Paired decode measurement implementation

Date: 2026-10-21. Status: implementation evidence, not a Spec amendment.
Branch: code/state-decode-measurement, based on #184 at 6d5e6b0d.

## Contract and scope

This slice implements TYPES-DECODE in the [types amendment][types] and the wire
portions of SPU-OPEN, SPU-MEASURE and SPU-WIRE in the [SPU amendment][spu]. It reuses
SPU-RENDER from #197, now folded into #184. The existing session owns rendering,
tokenization and the counts. Measurement is read-only and holds no proposal after
its answer. The loop owns orchestration and this slice changes no loop policy.

Open retains its existing request. Its successful answer now requires numeric
prefix_tokens, capacity, generation_limit and terminal_tokens, taken from the open
session and resolved limit. MeasureDelta carries keep, canonical delta and ordered
tool_schemas. DeltaMeasured returns resident, prefix_tokens, clamped keep,
input_tokens and capacity. AppendAndGenerate requires tool_schemas even when empty.
The same complete render is tokenized for measurement and append. Oversized proposals
return their count without appending. No defaults admit the old shapes.

The existing serial dispatcher owns the socket. Measurement before Open refuses
NotOpen. Measurement observed during generation or re-feed refuses OutOfOrder and
neither cancels that drive nor becomes queued work. A malformed family rendering
refuses MalformedDelta. Tokenization or channel failure retains the existing
component-failure path. Generation, streaming and ReFeed shapes remain unchanged.

The retiring harness engine sends an explicit empty schema list, preserving its
rendered input. Its boot adapter requires a complete Opened body. Diagnostic replay
uses that same open adapter and its unchanged ReFeed exchange. These adapters do not
provide a second placement implementation. External diagnostic clients must migrate
before the paired vocabulary deploys.

## Remaining consumer and cutover obligations

The [harness amendment][harness] still owes its sole-pump measurement owner, exact
proposal custody, cached boot facts, checked head/tail budgets, cancellation,
PlacementGrant and Generated reconciliation. The old open adapter structurally
decodes counts but does not validate those placement contracts. Marks, cuts and the
placement journal remain H3b work in the [implementation map][map].

The existing Spec gap remains explicit: OPEN-FACTS asks for an identity-prefix open
measurement label, while TYPES-DECODE defines only four Opened counts and leaves
Generation unchanged. This slice invents no carrier for that label. The unmerged
recall amendment #189 remains a separate boot/placement provenance dependency.

This branch is based on #184 rather than the harness stack. Review its diff against
that root. Integrate the exact paired change into the harness descendant of #217
before the coordinated foundation cutover. Do not deploy this wire independently or
claim that the legacy caller adaptations qualify the replacement serving path.

## Validation

The restored types and harness suites passed 498 tests, with 16 inherited ignored
instruments. The types config-feature suite passed 171 tests. Both host all-target
lint gates passed. The local target is /tmp/weaver-state-decode-target.
The required Olympus cuda,gguf suite reported 299 passes and its all-target lint
gate passed. The existing pace instrument explicitly skipped because
WEAVER_MEASURE_PACE was unset, so no throughput claim follows from that reported
pass. Both real measurement backends ran. Formatting and the lock gate passed.
The Olympus source and private target are under
/var/tmp/weaver-state-decode-20261021. The device lane uses cuda,gguf and explicitly
names the Qwen2.5 GGUF and native safetensors fixtures. These fixtures do not qualify
production capacity, throughput, every model or the STE100 classifier.

The codec tests pin exact JSON, all mandatory fields, numeric u64 counts and refusal
correlation. Harness socket tests carry a segmented proposal byte for byte, reject
incomplete answers and retain exact refusal cases. The boot test refuses a measurement
answer in place of Opened. The legacy engine test observes the explicit empty schema
list on its actual socket.

The real SPU socket instrument measures malformed, oversized and clamped proposals,
then appends the exact messages and schemas. It checks the actual rendered request,
input token IDs, measured length and resident arithmetic. Further measurement after
generation and Flush checks current facts. A second append in the same turn checks
that measurement did not advance the generation ordinal. Prequeued probes test the
active generation and re-feed refusal paths without a scheduling race.

Twelve perturbations compiled and failed their protective tests: seven on the host
and five on Olympus. Exact saved source bytes were restored after every probe.
The full host suites and both real measurement backends passed on restored source.
All 350 crate and manifest files match between the local and device snapshots.

| Removed or changed protection | Detecting test |
| --- | --- |
| Required append schemas | every_proposal_member_is_required_and_null_is_not_a_default |
| Required Opened limit | every_answer_fact_is_required_and_null_is_not_a_default |
| Required measured input count | every_answer_fact_is_required_and_null_is_not_a_default |
| Refusal ask spelling | measurement_refusals_carry_the_exact_requested_keep |
| Open reply association | opening_requires_the_complete_paired_answer_and_not_a_measurement |
| Legacy empty schemas | a_refused_turn_closes_once_and_names_the_refusal |
| Ordered proposal schema bytes | a_measurement_proposal_crosses_the_decode_channel_byte_for_byte |
| Resolved Opened limit | real_residency_child, exact wire answer |
| Returned keep clamp | real_residency_child, before/after generation facts |
| Returned input count | real_residency_child, measured versus actual append |
| Same append schemas | real_residency_child, complete rendered request and token IDs |
| Active measurement refusal | active_decode_refuses_measurement_without_cancelling_or_queuing_it |

Independent local review caught five CUDA fixture constructors missing the required
schema field. They were corrected. This review does not replace the Planner grade or
third-party review. No serving cutover or deployment has occurred.

Local logs use /tmp/state-decode-*. The host mutation results and restoration hashes
are in /tmp/state-decode-host-mutations/. Device logs and probes are held under
/tmp/state-decode-olympus-evidence/.

[types]: ../crates/state-management/weaver-types-Spec-amendment.md
[spu]: ../crates/state-management/weaver-spu-Spec-amendment.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[map]: state-management-implementation-plan-2026-10-20.md
