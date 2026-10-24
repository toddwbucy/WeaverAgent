# Rejected-return raw packet representation

Date filed: 2026-10-24
Issue: #146. PR: #312, folded parent
`47506ac04a1bb941ac27750e05917934bd2cb7d3`.
Branch: `code/state-author-custody-composition`.

The operator elected String on 2026-10-24 and directed that the Spec and code
travel together in #312. This act implements the producer and trace payload in
that PR. The foundation dispatcher remains unselected; its other activation and
production qualification work remains open. This representation adds no
WeaverAnalysis cutover dependency.

## Concrete finding

The #324 process instrument `worker_external_composition_multiline` sends a valid
pretty-printed ToolAnswer on an unexpected exchange. The harness retains its
complete raw packet in `pairs::RejectedGateReturn`. The RawValue trace
field embeds literal newlines, and the recorder refuses the authoring attempt as
PayloadMalformed. The flow halts with the exact source still held. No malformed
JSON or lost transport byte is needed to reproduce the conflict.

Spec 05 PAIR-MATCH requires the original full packet without parse/reserialize.
The trace's canonical record and reader both require one event per line. A
string carrier satisfies both obligations without changing the organ protocol,
event kind, outcome/correlation rules or forbidden state projection.

## Operator election

RejectedReturn.raw_envelope changes from RawValue to String in Spec 05 and
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

The string is the elected shape. No production trace holds the earlier object
shape: this record is written only on the unselected foundation path.
WeaverAnalysis does not read this event, and the event is forbidden from state
projection, so it does not cross the analysis-state seam.
[WeaverAnalysis#15][reader] is informational: any future reader of this event
decodes the string. It grants no preload traffic and adds no reader, mirror or
cutover obligation. DIAGNOSTIC-STATE retains the operator's October 18 R8 clause.

Alternatives considered:

| Choice | Consequence |
| --- | --- |
| Escaped UTF-8 string, elected | Exact packet round trip in one trace line; the trace field type changes. |
| Normalize the object | Preserves semantic JSON but loses the promised whitespace/escape/member-order evidence; requires weakening PAIR-MATCH. |
| Base64 packet bytes | Lossless and handles arbitrary bytes, but expands scope to a byte carrier even though this event already requires a decoded valid envelope. |
| Forbid multiline organ packets | Changes the existing organ acceptance contract and still needs a raw protocol-fault account; does not repair the stated evidence requirement. |

## Implementation and remaining obligations

| Site | Disposition in #312 |
| --- | --- |
| harness flow/pairs.rs | Copies received.raw_bytes with checked UTF-8 and no parsing, trimming or lossy conversion. Source-port/exchange and late/duplicate mapping remain in the same owner. |
| harness pair execution and halt account | Carry the String through the existing ordinary and trace-only author. Original receive custody, original bounds and the Recorded acknowledgement remain unchanged. |
| trace serving payloads/records.rs | Serialize-only String field; normal outer escaping and one final newline. No new internal-crate dependency. |
| trace payload/recorder instruments | Real writer/sink checks recover exact CR/LF, tabs, outer whitespace, quotes, backslashes, UTF-8 and original JSON escapes. The String golden fixture matches the actual writer's line. |
| harness primitive/process instruments | Exact bytes remain before/after Recorded and across capacity pressure plus an intervening Leave wait. The multiline process case now requires Recorded rejection followed by the legitimate tool completion. Late, duplicate and unexpected dispositions keep their prior identities and fates. |
| maximum payload | The harness receives exactly MAX_ENVELOPE_BYTES through an actual organ socket. The trace instrument records a 64 KiB escaped sample whose line exceeds the packet bound. This is a regression instrument, not production size/memory/latency qualification. |
| WeaverAnalysis | [WeaverAnalysis#15][reader] is informational. No existing reader consumes this event and no production object-shaped record needs compatibility support. |
| qualification | Open in #146: full escaped event size, all retained copies and authoring latency under the selected limits and deployment tuple. The recorder still has no per-record byte ceiling. |

The trace registry references Spec 05 for this payload. The analysis-state
contract gains no rejected-return rule and is restored to main's text.
Spec 05 remains version 0.12 and the trace amendment 0.27, the versions proposed
in folded #325. Their Landing PR names #312; first filing dates remain unchanged.
The diagnostic amendment is restored to main's version 0.8 and R8 sentence.

## Evidence and completion boundary

The operator's String election and this producer implementation answer P1 in
#312 itself. Planner correction 5639504195 withdrew P2: WeaverAnalysis has no
reader of this event, the event does not cross its seam and no production record
holds the earlier shape. Regrade 5639614128 found the code conforming and the
retained P2 obligations a document fix. This correction restores R8 and the
contract, removes the historical-shape rule and its fixture, and treats
WeaverAnalysis#15 as informational.
The earlier #325 proposal grade 5638105018 was not implementation authority;
the explicit 2026-10-24 instruction supplies the election for this combined act.

The exact #324 reproduction and source are deposited in
`worker-external-b5f48f6a-20261024` under the approved #146 evidence root. Its
manifest SHA256 is
`1419473b3ab2241959fc843eecc3ac95baac1a42cd1b29bfffdf357faea83946`.
That source reproduces the old multiline refusal. The #312 body names the new
source revision and its restored-source gates, protective controls and evidence
deposit. Neither the earlier #324 result nor the folded tree's #329 gates is
substituted for testing this code change.

The six activation blockers and other batch findings in the folded #312 grade
remain tracked under #146. [WeaverAgent#330][retention] carries the separately
filed CompletedPair retention design and reported growth measurement. The String
change does not settle those lifecycle, routing, ownership or retention findings.
Foundation activation, production language artifacts and coupled cutover remain
held for their existing reasons. No #146 checkbox closes here.

The implementation lesson is to exercise the actual recorder with the received
payload spelling: parsed JSON equality cannot establish exact packet evidence.
The new checks compare bytes after one outer decode and keep the backpressure
continuation in the same test.
Review follow-through must also read corrections posted while implementation is
under way; an earlier grade is not authority after its premise is withdrawn.

[reader]: https://github.com/toddwbucy/WeaverAnalysis/issues/15
[retention]: https://github.com/toddwbucy/WeaverAgent/issues/330
