# State management Open evidence carrier proposal

**Version:** 0.2 **Date filed:** 2026-10-21

This act proposes the missing OPEN-FACTS carrier for
[Issue #146](https://github.com/toddwbucy/WeaverAgent/issues/146). It changes documents
only. It records no operator approval, Planner pass, implementation result or production
qualification. The owning documents use merge-time MERGED headers under Working Process
section 2. Their proposed delta is not implementation authority before approval and
merge. The existing serving implementation and coupled foundation cutover remain held.

## 1. Gap and proposed election

Spec 06 requires an identity-prefix partition in the Open measurement. The merged
Opened body supplies only four counts, and SPU section 6 explicitly owes the future
carrier. Existing model.request and model.measurement are turn-bound. Boot has no turn
to put those events in, and no implementation may invent one.

Extend the existing Opened answer with an exact session echo and the SPU's raw request
and measurement boxes. Preserve those boxes in a new required open_evidence field on
state.placement. The trace wrapper contains request and measurement only; the wire echo
is checked before conversion and is not copied into that wrapper. That field is nonnull
exactly at boot and null at cut or append. Assembly zero and the causal link to the
earlier full boot recall already belong to that event. The proposal adds no event kind,
exchange, projection or prompt filing.

The owning definitions are [OPEN-FACTS and PLACEMENT-RECORD][place],
[TYPES-DECODE][types], [SPU-OPEN and SPU-WIRE][spu],
[HARNESS-BOOT and HARNESS-OPEN][harness], [TRACE-PLACEMENT][trace] and
[the decode contract amendment][contracts]. The base [SPU Spec][base-spu] names the
identity-prefix label at that same cutover. Their version index changes together.

| Proposed choice | Reason and consequence |
| --- | --- |
| Keep one Opened answer and add the existing Open.session echo | Binds a complete response to the held request without another counter, ask or socket |
| Retain exact request and measurement in boot placement | Accounts for the real prefix through an existing turnless causal and Recorded boundary |
| Retain a second, family-framed full prompt copy in the trace | Boot recall already holds the canonical Message. Open evidence duplicates its content with framing and token IDs; it increases retained memory and trace disclosure under the same protected operator trace custody, with no projection or second state filing |
| Keep only the existing trace-envelope session | The wire echo remains an admission check; another session in open_evidence would add redundancy without another source of identity |
| Keep SPU box contents opaque to trace | The harness validates semantic contents before authoring; trace validates its outer wrapper and envelope, preserving existing custody |
| Retain current recorder capacity semantics | Entry count is bounded but per-record bytes are not; qualify the complete encoded line and retained memory rather than claim a nonexistent byte ceiling |
| Include render, template, admitted model/weights identity, prefix_token_ids and one identity-prefix byte range | Supplies the owed partition and count provenance without fabricating a generation |
| Omit Open prefill timing entirely | Timing is unnecessary to close this carrier gap and would add an independent sampling election. Existing placement wait_ns remains the completed Open wait |
| Require null evidence at cut/append | Keeps a single closed placement shape and prevents copying boot facts into unrelated operations |
| Permit only the owed boot account during teardown | Retains truthful evidence after cancellation without granting use or weakening the first failure |
| Use ordinary writer capacity for that teardown account | Preserves the seven terminal reserves and their existing priorities. A failed or unavailable trace can leave an incomplete account |
| Diagnostic clients validate and consume the new wire, without a new diagnostic event | Preserves their separate vocabulary and the serving-placement conversion refusal |

These are proposed choices for operator and Planner review, not a numbered ruling.
No timing field is present, optional or zero-filled. The request and measurement are
separate from generation payloads, which remain unchanged and turn-bound.

## 2. Provenance and ownership correction

Boot recall Recorded establishes BootHead before decoder Open. BootHead retains the
canonical system Message, stored SpanRef and stored UTF-8 byte end. The Open evidence
does not create it. A boot placement that is Recorded, fits the configured limits and
passes HARNESS-OPEN-LIVE releases initialized journal/decoder-use authority and boot
mark readiness. That predicate requires Entered/active ownership, a clear latch, no
observed stop or leave, and the unchanged original Enter bound still unexpired. A
cancelled owner never regains readiness through a wake. Pair repair and the second empty
open-pairs read remain later enter duties.

The harness holds the exact boot source, request, reservation, port and original enter
bound. It requires a complete send and complete answer. It checks the echoed session,
closed box shape, numeric domains, token count and whole-render byte interval. It must
not compare rendered text with BootHead Text, because the family adds framing and may
fold messages. It must not tokenize or retain a usable token-vector cache. Raw evidence
is held only for recording and grants no buffer access.

The SPU proves that the reported render was tokenized once and that the reported vector
was supplied to the successful prefill. The harness proves request, port, source and
trace correspondence. The trace checks its closed outer wrapper, raw-object presence and
event envelope without inspecting the SPU box members or acquiring a floor/harness
dependency. The named floor Opened wrapper manually compares raw bytes for PartialEq
while TokenAnswer keeps its existing trait contract. Structural checks cannot prove what
a backend executed.

The evidence vector is named prefix_token_ids. Its length is prefix_tokens. The
outer placement.input_tokens remains zero at boot because no delta is appended;
it is neither that vector nor its length. Generation.measurement.input_tokens
keeps its existing name and meaning.

The exact event and author ticket survive recorder pressure. Recorded sequence and
next continuation must be saved before a recoverable lifecycle check. No wake rereads
the prompt, resends Open, changes the source, steals another owner's ticket or authors
the same placement twice. The original enter bound includes evidence recording.

## 3. Refusal, cancellation and incomplete accounts

A valid complete Opened means the prefix was seated. If the configured head/tail fit
then fails, the truthful boot placement precedes the existing boot_failed refusal.
Neither record grants readiness. An SPU Open refusal means no accepted prefix evidence
exists. A malformed, wrong-session, wrong-port or inconsistent reply is protocol
failure. It cannot be turned into a fit refusal or accepted placement.

An unsent reservation is withdrawn on cancellation. A partial send remains owned by
the existing partial-message cleanup, without a restart or guessed send completion.
Cancellation after a sent Open cannot withdraw a successful prefill. The proposed
private HARNESS-OPEN permit can drain that same exchange and record its valid evidence
during teardown, under the earlier original-enter/active leave or halt bound. This
explicitly extends author admission for this held obligation only. It spends ordinary
writer capacity, never the seven reserved terminal entries, and cannot delay mandatory
terminal work. It creates no content filing, generation, journal grant or mark
readiness. In both Releasing and Failed, settle the account or declare it
incomplete before retiring its decode transport and authoring unload. An expired
bound or failed seam/sink abandons the remaining account and continues cleanup.
The reserved terminal work before Releasing keeps priority.
The first failure remains first. Failed trace, lost seam or expired bound can leave an
incomplete account and still require teardown. No new clock, reconnect or retry exists.

The complete Opened JSON message remains subject to the existing 8 MiB
DECODE_MESSAGE_BOUND, even if its prefix fits KV. An oversized answer is failure,
never permission to truncate bytes, omit tokens or open again. The resulting
trace line has a different envelope and no existing per-record byte ceiling.
Its size and retained-memory qualification are separate from decode admission.
This proposal adds no numeric bound or reserved queue entry.

## 4. Actual size and custody boundaries

The source audit is against the proposal's main-based implementation, without
running or changing code:

| Boundary | Actual existing rule and consequence |
| --- | --- |
| Decode answer | [wire.rs][source-wire] DECODE_MESSAGE_BOUND is 8 MiB for the complete JSON message, including kind/body and raw-box escaping; segmentation does not enlarge it |
| Recorder | [serving/recorder.rs::submit][source-recorder] reserves an entry, renders the complete event, retains its line and enqueues it; no per-record byte maximum is checked |
| Trace sink | [serving/writer.rs][source-writer] writes the complete line with write_all; envelope, boxes and LF must all be accounted for |
| Tee/state member | state.placement is forbidden projection. An active [tee][source-tee] returns Unselected and emits no state request; a stopped tee returns Stopped. STATE_REQUEST_LINE_BOUND is 8 MiB including LF for actual state requests, not this trace record |
| External readers | Each reader's limits must be named and tested at the paired cutover; no decode or state-request limit can stand in for them |

Qualification must measure the actual final encoded placement line and the
simultaneously retained Open answer, evidence, event and canonical-line memory.
Use the actual escaping and envelope, not decoded prompt length or an assumed
second escaping of raw JSON boxes. Entry capacity limits count, not bytes.
Encoding or sink errors follow their existing component-failure paths. Current
infallible allocations may terminate the process; no graceful allocation-failure
recovery is claimed or elected. A failed submission grants no Recorded; a later sink
failure preserves any already Recorded sequence and does not turn it into a successful
sink write. No evidence
box is shortened.

The second prompt copy is a proposed, explicit custody tradeoff: both the
canonical prompt and family-framed render become operator-readable trace
content. Excluding placement from state projection prevents refiling, not trace
disclosure. No independent telemetry destination or broader reader permission
is elected. The operator must accept this duplication with the carrier.

The decode contract's general typed-overflow guarantee is broader than its failure
list, which specifically names the next delta. This act does not reclassify
that path or add a TokenRefusal. Implementation review must check the existing Open
failure contract and report a conflict if its prefix-overflow mapping remains unclear.
A failed prefill cannot supply successful Opened evidence under either interpretation.

## 5. Alternatives not selected by this proposal

A separate turnless event or request/measurement pair would require new registry,
envelope, order, custody, projection and reader rules. Permitting turnless model events
would alter the existing generation binding and replay assumptions. A second report or
intermediate would add ordering and missing-half failure states to the seam. Counts
alone would leave the required identity-prefix partition unrepresented. Each is a
possible operator choice, but none is implicit authority for an implementation.

The wire session echo is an explicit extra check. The sole outstanding request and
reserved port remain necessary and cannot be replaced by equality of that string.
Omitting the wire echo would be a different election. A second echo in the trace wrapper
is not selected, because the envelope already supplies that identity. Adding prefill
timing would also need its own exact sampling boundary and fixtures, rather than a
guessed reading in the chosen carrier.

## 6. Acceptance work owed after approval

| Boundary | Required instrument and protective change |
| --- | --- |
| Shared codec | The named Opened wrapper preserves raw bytes through roundtrip and manual PartialEq. Floor rejects old/malformed outer shapes; SPU/harness check box keys, numeric domains, label/range and counts. Trace consumes only its wrapper/envelope cases from the same corpus. Removing each layer's own check must fail its negative case |
| Producer | Real session for every admitted rendering family proves the exact preamble/render, one tokenization, actual prefill vector, prefix count and identity-prefix bytes. A second tokenization, changed preamble, schema/opener or false reading must fail |
| Source and transport | Real sole-reader fixture rejects incomplete send/answer, wrong session/port/reservation and stale response. Removing correlation must fail before a journal grant |
| Recorded boundary | Seeded boot with real writer pressure records one recall and one boot placement with exact boxes and causal source. Removing the Recorded guard, changing a box or duplicating after wake must fail |
| Continuations | Interrupt response validation, author admission and Recorded settlement. Cover repeated recoverable waits, cancellation before and after send, fit refusal, failed sink and late/oversized answer. Consumption before retained continuation or deadline renewal must fail |
| Trace license | Actual event admission tests boot/nonboot wrapper presence, raw objects, no boot turn, envelope session, causal parent, forbidden projection and diagnostic refusal. Raw-content retention must stay independent of SPU member interpretation. Wrong phase, lost parent or conversion must fail |
| Lifecycle priority | Cover Releasing and Failed: settle or explicitly abandon the account before decode transport retirement and unload, while earlier reserved terminal work keeps priority. Saturate ordinary capacity. Spending a terminal slot, retiring the source early or extending the bound must fail |
| Size boundary | Exercise complete decode JSON at DECODE_MESSAGE_BOUND and one byte over; measure the final trace envelope, escaping, LF and live retained copies. Recorder tests preserve complete lines without assuming the decode bound is a trace limit; no selected state filing may appear |
| Deployment | Paired decoder, harness, trace and external-reader cutover only after the complete serving replacement and production tuple qualify |

The current act performs document ASCII, wrapping, link and diff checks only. It claims
none of the runtime instruments above. Full boot/place ownership, UNIT/HeldMaterial,
inward pair integration, lifecycle migration and production qualification remain
separate Issue #146 work. Approval cannot activate an incomplete replacement.

[place]: ../crates/state-management/state-management-spec-06-marks-renderer-engine.md
[types]: ../crates/state-management/weaver-types-Spec-amendment.md
[spu]: ../crates/state-management/weaver-spu-Spec-amendment.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[trace]: ../crates/state-management/weaver-trace-Spec-amendment.md
[contracts]: ../crates/state-management/contract-amendments.md
[base-spu]: ../crates/weaver-spu/weaver-spu-Spec.md

[source-wire]: ../../crates/weaver-types/src/wire.rs
[source-recorder]: ../../crates/weaver-trace/src/serving/recorder.rs
[source-writer]: ../../crates/weaver-trace/src/serving/writer.rs
[source-tee]: ../../crates/weaver-trace/src/serving/tee.rs
