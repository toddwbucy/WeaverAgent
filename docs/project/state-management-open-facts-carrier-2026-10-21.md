# State management Open evidence carrier proposal

**Version:** 0.1 **Date filed:** 2026-10-21

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
state.placement. That field is nonnull exactly at boot and null at cut or append.
Assembly zero and the causal link to the earlier full boot recall already belong to
that event. The proposal adds no event kind, exchange, projection or prompt filing.

The owning definitions are [OPEN-FACTS and PLACEMENT-RECORD][place],
[TYPES-DECODE][types], [SPU-OPEN and SPU-WIRE][spu],
[HARNESS-BOOT and HARNESS-OPEN][harness], [TRACE-PLACEMENT][trace] and
[the decode contract amendment][contracts]. The base [SPU Spec][base-spu] names the
identity-prefix label at that same cutover. Their version index changes together.

| Proposed choice | Reason and consequence |
| --- | --- |
| Keep one Opened answer and add the existing Open.session echo | Binds a complete response to the held request without another counter, ask or socket |
| Retain exact request and measurement in boot placement | Accounts for the real prefix through an existing turnless causal and Recorded boundary |
| Include render, template, admitted model/weights identity, actual input tokens and one identity-prefix byte range | Supplies the owed partition and count provenance without fabricating a generation |
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
passes live admission releases initialized journal/decoder-use authority and boot mark
readiness. Pair repair and the second empty open-pairs read remain later enter duties.

The harness holds the exact boot source, request, reservation, port and original enter
bound. It requires a complete send and complete answer. It checks the echoed session,
closed box shape, numeric domains, token count and whole-render byte interval. It must
not compare rendered text with BootHead Text, because the family adds framing and may
fold messages. It must not tokenize or retain a usable token-vector cache. Raw evidence
is held only for recording and grants no buffer access.

The SPU proves that the reported render was tokenized once and that the reported vector
was supplied to the successful prefill. The harness proves request, port, source and
trace correspondence. Trace checks the closed payload and envelope without acquiring a
floor or harness dependency. Structural checks cannot prove what a backend executed.

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
readiness.
The first failure remains first. Failed trace, lost seam or expired bound can leave an
incomplete account and still require teardown. No new clock, reconnect or retry exists.

New evidence may exceed a transport or recorder bound even if the prefix fits KV.
That is failure, not permission to truncate bytes, omit tokens or open again. The
implementation must measure retained evidence memory and whole-message size under the
qualified tuple. This proposal sets no new numeric bound or reserved queue entry.

The decode contract's general typed-overflow guarantee is broader than its failure
list, which specifically names the next delta. This act does not reclassify
that path or add a TokenRefusal. Implementation review must check the existing Open
failure contract and report a conflict if its prefix-overflow mapping remains unclear.
A failed prefill cannot supply successful Opened evidence under either interpretation.

## 4. Alternatives not selected by this proposal

A separate turnless event or request/measurement pair would require new registry,
envelope, order, custody, projection and reader rules. Permitting turnless model events
would alter the existing generation binding and replay assumptions. A second report or
intermediate would add ordering and missing-half failure states to the seam. Counts
alone would leave the required identity-prefix partition unrepresented. Each is a
possible operator choice, but none is implicit authority for an implementation.

The session echo is an explicit extra check. The sole outstanding request and reserved
port remain necessary and cannot be replaced by equality of that string. Omitting the
echo would be a different election. Adding prefill timing would also need its own exact
sampling boundary and fixtures, rather than a guessed reading in the chosen carrier.

## 5. Acceptance work owed after approval

| Boundary | Required instrument and protective change |
| --- | --- |
| Shared codec | One concrete corpus consumed by floor, SPU, harness and trace at their own validation layers. Reject old unit/four-count Opened, missing/duplicate/unknown fields, wrong numeric domains, wrong label/range and count disagreement. Removing a required check must fail its negative case |
| Producer | Real session for every admitted rendering family proves the exact preamble/render, one tokenization, actual prefill vector, prefix count and identity-prefix bytes. A second tokenization, changed preamble, schema/opener or false reading must fail |
| Source and transport | Real sole-reader fixture rejects incomplete send/answer, wrong session/port/reservation and stale response. Removing correlation must fail before a journal grant |
| Recorded boundary | Seeded boot with real writer pressure records one recall and one boot placement with exact boxes and causal source. Removing the Recorded guard, changing a box or duplicating after wake must fail |
| Continuations | Interrupt response validation, author admission and Recorded settlement. Cover repeated recoverable waits, cancellation before and after send, fit refusal, failed sink and late/oversized answer. Consumption before retained continuation or deadline renewal must fail |
| Trace license | Actual event admission tests boot/nonboot evidence, no boot turn, session and causal-parent checks, forbidden projection and diagnostic refusal. Wrong phase, changed session, lost parent or conversion must fail |
| Lifecycle priority | Saturate ordinary capacity during cancelled boot, retain the event within the original bound and still admit mandatory reserved terminal records. Spending a terminal slot or extending the bound must fail |
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
