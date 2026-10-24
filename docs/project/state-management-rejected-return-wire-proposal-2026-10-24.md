# Rejected-return raw packet representation proposal

Date filed: 2026-10-24
Issue: #146. Parent: #324 at
`bbe299fe6fd8c3515a16f550976c7652ba5e78f7`.
Branch: `docs/state-rejected-return-wire`.

This is a design proposal for operator election, Planner review and merge before
implementation. No runtime source changes. The active serving path stays active.

## Concrete finding

The #324 process instrument `worker_external_composition_multiline` sends a valid
pretty-printed ToolAnswer on an unexpected exchange. The harness retains its
complete raw packet in `pairs::RejectedGateReturn`. The current RawValue trace
field embeds literal newlines, and the recorder refuses the authoring attempt as
PayloadMalformed. The flow halts with the exact source still held. No malformed
JSON or lost transport byte is needed to reproduce the conflict.

Spec 05 PAIR-MATCH requires the original full packet without parse/reserialize.
The trace's canonical record and reader both require one event per line. A
string carrier satisfies both obligations without changing the organ protocol,
event kind, outcome/correlation rules or forbidden state projection.

## Proposed election

Change only RejectedReturn.raw_envelope from RawValue to String in Spec 05 and
its trace mirror. The producer copies the exact valid UTF-8 packet into that
string. The trace's outer JSON encoding escapes the packet's quotes, backslashes
and literal control characters. One outer JSON decode recovers exactly the
original frame payload bytes, as held by ReceivedOrgan.raw_bytes, including
surrounding payload whitespace, member order and original escape spellings.
It is not a parsed envelope or a normalized JSON dump.
Transport delimiters and length prefixes are outside that payload.
Malformed or non-UTF-8 traffic keeps the existing protocol-fault evidence path.

Keep the original packet until the rejection is Recorded. Use the existing
pending author, applicable original bound and failed-run evidence owner through pressure
or cancellation. The organ bound limits the received payload. The escaped trace
line may be larger; the recorder has no per-record byte ceiling and does not
refuse a line merely for exceeding the organ bound. Qualify the full event's
escaped size, copies and recording time, including maximum admitted payloads
and control-character expansion. There is no new recorder byte ceiling, clock,
queue, trace event or execution permission.

New publishers use the string shape. Historical object-valued records stay
unchanged. Readers distinguish both shapes without rewriting history or treating
a reconstructed object as byte-exact evidence. Harness, trace and every external
reader must change together before the held foundation cutover. This includes
WeaverAnalysis's canonical reader and diagnostic/preload converter. This act
changes this repository's Spec 05, trace amendment and authoritative
weaver-analysis-state contract copies. The contract draws the two-shape reader
rule from trace; no new preload encoding is defined. WeaverAnalysis's mirror
and implementation remain owed. No external consumer is claimed updated.

Alternatives considered:

| Choice | Consequence |
| --- | --- |
| Escaped UTF-8 string, proposed | Exact packet round trip in one trace line; the field type and consumers must change. |
| Normalize the object | Preserves semantic JSON but loses the promised whitespace/escape/member-order evidence; requires weakening PAIR-MATCH. |
| Base64 packet bytes | Lossless and handles arbitrary bytes, but expands scope to a byte carrier even though this event already requires a decoded valid envelope. |
| Forbid multiline organ packets | Changes the existing organ acceptance contract and still needs a raw protocol-fault account; does not repair the stated evidence requirement. |

## Paired implementation and review obligations

| Site | Required follow-through |
| --- | --- |
| harness flow/pairs.rs | Build from received.raw_bytes without parsing, trimming or lossy conversion; retain existing source-port/exchange and late/duplicate mapping. |
| harness pair execution and halt account | Use the same exact record through ordinary and trace-only authoring, capacity, cancellation and first-fatal retention. |
| trace serving payloads/records.rs | Serialize-only String mirror, outer escaping and one final newline. No internal-crate dependency. |
| trace payload/recorder instruments | Whole-record round trip with CR/LF, tabs, surrounding whitespace, quotes, backslashes, Unicode and original JSON escapes. |
| harness primitive/process instruments | Exact bytes before/after Recorded, no early discard, no changed terminal or forged result, and multiline regression now records successfully. |
| WeaverAnalysis canonical reader and diagnostic/preload converter | Historical object and new string cases, unchanged diagnostic refusal and forbidden state projection; update its contract mirror and audit other external consumers before cutover. |
| qualification | Escaped line size, retained memory and authoring latency against the actual declared limits and transport maximum. |

The trace registry references Spec 05 for this payload. The analysis-state
contract draws its reader rule from trace rather than defining another wire.
Version advances from 0.11 to 0.12 in Spec 05 and 0.26 to 0.27 in the trace
amendment. First filing dates stay 2026-10-20 and
2026-10-21. Landing PR names #325 in each changed normative document; their first
filing dates remain unchanged.

## Evidence and completion boundary

Planner review 5638105018 at b600ee88 graded zero fix and four batch, and marked
the proposal ready without electing it. This revision answers all four rows:
Landing PR names #325; raw_bytes defines the payload boundary; WeaverAnalysis
and its authoritative contract are named; the organ bound and recorder's lack
of a per-record byte ceiling are explicit. The corrected #324 parent is joined
without changing its tested runtime. The proposal now changes four documents.

The exact #324 reproduction and source are deposited in
`worker-external-b5f48f6a-20261024` under the approved #146 evidence root. Its
manifest SHA256 is
`1419473b3ab2241959fc843eecc3ac95baac1a42cd1b29bfffdf357faea83946`.
This proposal's mechanical checks and small JSON round-trip illustration are
separate evidence, not a runtime implementation or Rust conformance test.

The operator has not yet elected this representation. Implementation waits for
that election, Planner review and merge. The schema clock/certification and gate
input routing blockers from #324 remain independent, as do full lifecycle,
workflow, artifact and deployment qualification. No #146 checkbox closes.
