# weaver-harness / weaver-spu - decode contract

**Status:** MERGED. Cut with the token-workflow act of `weaver-spu-PRD` sections 13 and
14, per apex section 10, and merged on the human's call like every document, the header
flipping at that call per the Working Process. Written and it governs the token seam:
the second seam between the same parties, on its own socket per the decoder-cut ruling
of 2026-08-02. The residency seam keeps its own contract, `weaver-harness-spu-contract`,
and neither document restates the other.

**Date filed:** 2026-08-02
**Document ID:** `weaver-harness-spu-decode-contract`
**Parent:** `weaver-agents-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #393

---

## 0. What this document is

The agreement over the token seam: how the harness's turns reach the resident
model and how the generation and its measurement return, what each party may
rely on, and how it fails. It is read alongside `weaver-harness-PRD` and
`weaver-spu-PRD` section 13, and none of the three is complete without the
others.

It carries no representation. The types it names have a definition site and no
field list here, the ordering it fixes is stated as a rule rather than a state
machine, and how any of it is encoded is the Spec's, elected against a
measurement because this is the hot path.

**The name carries the seam's traffic, because three seams between one pair need
three names.** The Document Format names a contract for its parties, initiator
first, and all three seams between this pair have the harness initiating, so the
party rule alone cannot distinguish them. The `-decode-` infix is the
distinguisher, naming the governed traffic the way the wire vocabulary is
named for its loop, and the residency contract's name stands unchanged.

**This document does not draw `weaver-organ-channel`.** The token seam is not
an organ channel, per `weaver-spu-PRD` section 13.2: the organ test names one
two-initiator channel and the lifecycle channel is it. The channel mechanics this
seam needs are stated in section 1 as its own, and the organ envelope does
not cross here.

```graph
node: weaver-harness-spu-decode-contract
kind: document

edge: party
from: weaver-harness-spu-decode-contract
to: weaver-harness

edge: party
from: weaver-harness-spu-decode-contract
to: weaver-spu

edge: draws
from: weaver-harness-spu-decode-contract
to: token-directive

edge: draws
from: weaver-harness-spu-decode-contract
to: token-answer

edge: draws
from: weaver-harness-spu-decode-contract
to: token-refusal

edge: draws
from: weaver-harness-spu-decode-contract
to: fault-report

edge: draws
from: weaver-harness-spu-decode-contract
to: message-model
```

**The seam edge is declared by the organ and appears in `weaver-spu-PRD`
section 6.** This document names its parties and does not restate the edge.

## 1. The channel

**An unaddressable connected pair, created by the harness before the child exists,
beside the lifecycle pair.** The same act creates both pairs, **the child carries one
end of each and the harness retains the counterparts**, and `weaver-spu-PRD` section 7
counts them: the SPU receives its two channel ends and no other handle. Possession
authenticates, because a
pair no third party can address has no second opener, and the channel lives exactly as
long as the process at its far end.

**One write is one message, and the property comes from the socket type.**
The seam's traffic is framed by the channel rather than by a layer above
it, so no contract that touches this seam carries framing. Which socket
type supplies the property is the Spec's election, taken with the hot-path
measurement this seam uniquely owes, since the per-token volume here is
what the loop-0 elections deliberately did not price.

**A message larger than the envelope crosses as a segment series, added
2026-08-19 by issue #236.** The generate exchange's close grows with the
turn - the emission, its parse, the request, and the measurement all scale
with the tokens - and a close past the envelope bound has to cross rather
than kill: the alternative was the silent death that issue records, an
answer the seam could not carry ending the organ with nothing said, and a
turn cap the declaration validates that the seam could never serve. The
series is one message to every rule of this document: it counts once, it
orders where its message orders, and an answer's series arrives whole
before the answer exists. Each segment is one write within the envelope
bound, so the framing property above is untouched. How a series is
spelled, recognized, and bounded in total is the Spec's, and a series
violating the total bound or arriving short is a channel fault, never a
partial message.

**Non-inheritance on both ends, by the split the corpus already runs.** The
harness's end carries the flag from the pair's creation, so no tool
child process the harness creates inherits a handle to the decode seam. The
SPU's end is set after its final image replacement, a set and not a check, so nothing a
later workflow spawns from the SPU inherits it either.

**Closure is not an answer.** A closed channel with an exchange outstanding
is the far party dead mid-exchange, treated as that exchange's failure and
never its success, and what a death means is section 5's.

## 2. The exchanges

Six exchanges and one emission. The exchanges are opened by the harness.
The emission is the SPU's fault report, the origination the residency
contract deferred, arriving here because the faults it carries are
decode-domain traffic, and it is an emission rather than a fifth exchange
per the receipt's retirement, the second ruling of 2026-08-12.

**The count carries the seam's dataflow rule, stated with the operator's
ruling of 2026-08-12.** An exchange the harness opens is an ask, expected
to set the SPU producing, and its answer returns what the ask produced:
the session opened, the generation drawn, the flush's outcome held. What
the SPU originates is a report, owed nothing back at all. The fact it
carries has one home, the record, whose custodian is the harness, so the
trace entry is the acknowledgment and no answer returns to the SPU.
Nothing that originates on this seam arrives owed work, which is the fact
the harness charter reads when it names what may drive a turn. The rule
is stated for the SPU in particular and generalized to no other organ: a
later organ's contract states its own dataflow rule at its own chartering
rather than inheriting this seam's.

**Open the session.** Opened by the harness, once per residency, after
residency is confirmed on the lifecycle seam. It carries the session's
identity material, the canonical messages the identity prefix is rendered
from, per the framing ruling of `weaver-spu-PRD` section 13.4, ratified by
the operator with this act. The SPU renders through the family library,
establishes the resident session with the prefix resident, and answers
opened, or refuses, typed.

**Append and generate.** Opened by the harness, one per generation - a
turn running as many generations as its tool rounds, per `weaver-spu-PRD`
section 13.8's derivation, so a tool round's result re-feeds as its own
append - carrying the turn's context per apex section 5.2 and that
generation's delta as canonical messages under the same framing ruling.
An earlier wording said one per turn, which 13.8 already contradicted and
the column gloss below made sharp. **It carries no sampling values.**
The operator-tunable remainder reaches the SPU in the declaration at
admit, per `weaver-spu-Spec` section 8, because the engine builds its
sampler once at session open and a value arriving with a token directive
has no engine to reach. An earlier wording carried them here, which is
the route this act closes. The SPU appends the delta at the
resident end and generates, **and the exchange streams as it does**, per
the operator's ruling of 2026-08-11: each token crosses as it is drawn,
an intermediate message carrying the token's identifier and its rendered
piece, any number of them and possibly none, ordered, and none closing
the exchange. The answer closes the exchange as it always did, carrying
the generation whole, the model.request content beside the measurement
because each splices into its own box of the record, the request being the
turn's delta as rendered with its template and effective sampling per the
ruling of 2026-08-12, the SPU
rendering it whole per the custody act of 2026-08-11, its measurement:
the token identifiers, the per-token signals, the timings, the block
partition, and, where the residency was admitted with readout elected, the
residual reductions. Beside the two boxes, as the generation's own typed
members and never inside the measurement, the session's fullness as the
generation closed: the resident token count and the capacity, added
2026-08-19 so the loop that elects a flush sees the pressure before the
wall rather than in the wall's refusal, shaped at `weaver-types-Spec`
section 4.4 because the harness consumes them. **The template identity
travels in the request and only
there**, per the ruling of 2026-08-17 on #129: it is an input-side fact of
the same kind as the sampling values, which moved to the request for the
same reason, and a member carried by both boxes of one turn's pair would be
one fact in two places with no authority named. **A second intermediate crosses where
the field election stands**, added
2026-08-21 on `weaver-spu-PRD` section 13.11. It carries one decode
position's ranked candidates with their probabilities and the rank the
draw landed on, one message per retained token, and like the token
intermediate it closes nothing. **It is a separate message rather than a
member of the token's**, and the reason is mechanical rather than
aesthetic: the token intermediate crosses once per renderable piece and a
piece may span several tokens, so a field carried inside it would be
paired with a position it does not name. The field's own message carries
its position and is paired by that.

**Where the election does not stand the message does not cross**, rather
than crossing empty, so a consumer counts what was produced instead of
filtering what was not. A party that receives no field message learns
nothing about why from the seam, the election's record being the load
event's per `weaver-trace-PRD` section 3.1 and not this seam's to carry.

**A third intermediate crosses where the column ask stands**, added
2026-08-31 on the operator's ruling of 2026-08-30. `weaver-spu-PRD` section
13.7 as amended holds the elections this paragraph carries out: the cadence,
the enforcement, and the position bound are that clause's, cited here rather
than fixed here. It carries one sampled position's residual columns, the
tap's own copy per layer at the width the artifact sets, one message per
sampled position - each decode forward, and each append's prefill final
position, a tool round's append adding its own, nothing else of the
prompt - paired by
the position it names like the field's, and like both intermediates it
closes nothing. **The ask crosses once, at session open, and its permission
crosses at admit**: admin sets the permission from the binding's kind inside
the instruction, so the SPU never learns the kind and never needs to, and an
open carrying the ask against no admitted permission, or against a family
whose declaration holds no column, refuses typed at the open. A serving
harness never writes the ask, the discipline watched on the harness per that
clause's owed assertion. Where the ask does not stand the message does not
cross, on the same no-empty-crossing rule as the field's. The column crosses
in `weaver-types-Spec` section 4.4's provisional bare JSON, the efficient
framing staying `weaver-spu-Spec` section 12's open election, settled
against the consumer draining it.

**The stream changes the seam's rhythm
and not the record's
shape**: the emission and the measurement arrive whole at the close, so a
consumer that ignores every intermediate message reads the exchange
exactly as the batch seam read. The answer closes the exchange and the
turn's context returns with it.

**Re-feed.** Opened by the harness, one per recorded generation, added
2026-08-31 per `diagnostic-replay-loop` section 2's owed act and
`weaver-spu-PRD` section 13.14, which carries the criterion and the
permission this exchange cites rather than fixes. It carries the turn's
context, the recorded generation's rendered contribution - the string
`model.request` recorded, per the ruling of 2026-08-12, tokenized and
appended with no family rendering on the way - and the
recorded token path. The SPU appends the rendered form, runs the forward passes
along the recorded path, computes each position's draw exactly as
append-and-generate would - the derived seed, the effective knobs, the
sampler consuming the distribution - **and appends the recorded token
whatever the draw said**, so a divergent position stays comparable instead
of cascading. **It draws no token of its own**: no token intermediate
crosses, there being no piece drawn to stream, while the field and the
column intermediates cross where their elections stand, which is what makes
this exchange the reader pass's substrate. The answer carries the
measurement with the recomputed draw identifiers in the slots the sampled
identifiers would fill, **and it is its own answer and never a
generation's**: a re-feed answering in the generation's shape would put a
supplied path in a sampled path's clothes, which is the collapse the
protoautonomic design names as the defect class, made unrepresentable here
by the answer's own type. The ask refuses, typed, where the instruction
carries no admitted re-feed permission - admin's, set from the binding's
kind like the column's and named individually beside it per the no-bundle
rule - where the session is not open, where the ask is out of order, or
where the recorded path is empty, an empty path being a replay of nothing
wearing an exchange.

**Cancel.** Opened by the harness while a generation is in flight,
carrying the turn's context and nothing else. The generation stops at the
next token boundary, the session is left well-framed with the family's
turn terminator resident, and the outstanding append-and-generate answers
with the partial output marked stopped, the tokens already streamed
standing and the close accounting every one of them. A cancel with nothing in flight
answers at rest, a clean close rather than a refusal, the same shape the
stop exchange takes on the coordination seam.

**Flush.** Opened by the harness between turns, never while a generation
is in flight. The directive names its cut, `keep`, added 2026-08-19 on
the operator's ruling that the cleanup line is the loop's: the session
returns to its first `keep` resident tokens, everything beyond them gone,
the outcome fixed against the named cut and the mechanism the Spec's per
family, per `weaver-spu-PRD` section 13.9. The cut is bounded, not
refused: a `keep` below the identity prefix's length holds the prefix
entire, its permanence being this seam's own guarantee, and a `keep` at
or beyond the resident count cuts nothing, the confirmation's counts
carrying what held in either case, so no outcome is silent. A `keep` of
zero is the prefix-only state the flush has always meant. The answer
confirms after the outcome holds, and carries the resident token counts
before and after the truncate: the SPU is the one authority on either
number, and the harness authors the record's `flush` event from exactly
them.

**Elide a span.** The harness names a half-open span of the resident
sequence and the SPU makes it absent: afterward the resident sequence is
what preceded the span followed by what followed it, the tail
re-established against the shortened prefix, the outcome fixed against the
named bounds and the mechanism the Spec's per family, per `weaver-spu-PRD`
section 13.13. **The span is state and never the record**, and this seam
carries no operation that removes anything from a trace.

**The ask is refused rather than bounded, which is the flush's rule
reversed and deliberately.** A `keep` the flush cannot honour describes
less context than exists and the arithmetic satisfies it without
misreporting. A span overlapping the identity prefix, running past the
resident count, ending before it starts, or empty with its end equal to its
start describes no removable region, and it has no smaller true version to
fall back to. **An empty span refuses rather than eliding nothing**, a span
removing no position being one the loop computed wrongly. The answer
confirms after the outcome holds and carries the resident counts before and
after, and it does not echo the span. **Each party writes what it is the
authority on**: the harness named the span and needs no confirmation of its
own ask, the SPU is the one authority on either count, and the record's
`elision` event is authored from the span the harness held and the counts
the SPU returned.

**The SPU decides nothing here.** It executes the span the ask names, as
it executes the cut the flush names. Which part of a context is worth
keeping is the loop's, per `weaver-spu-PRD` section 13.9 and 13.13, and an
organ choosing the span would be an organ judging what a context is worth.

**Nothing is inserted.** The elision removes and does not replace, and this
seam stays append-only: whatever should stand where the span stood arrives
as the next delta and lands at the end.

**Report a fault.** Emitted by the SPU, the seam's one SPU-originated
message and not an exchange, carrying a `fault-report` naming a case of
`weaver-spu-PRD` section 13.10 that arose outside any outstanding
exchange, residency degraded above all. A fault arising inside a
generation is that exchange's typed answer instead, so one fact never
travels twice. The harness authors what it is handed as the `fault`
event, per the fault-carrier ruling, and answers nothing. The receipt
retired on the second ruling of 2026-08-12: the SPU could do nothing
with it but discard it, and a confirmation whose one reader discards it
is the alert exchange's error one layer down. The promise of faithful
authorship is this contract's, in section 4, rather than an answer's.
The SPU forgets the fault at the send, produced, reported, gone, and a
send that fails is the channel dead, which is section 5's account of a
death.

**No exchange carries a filesystem path, and no exchange carries
lifecycle.** Neither party learns a trace path, a record name, or any name
the filesystem resolves from the other, the handle discipline of
`weaver-harness-PRD` section 5 holding on this seam as on every seam. The
re-feed's recorded token path is not that kind of path and is no exception
to this clause: it is a sequence of token identifiers, data of the
exchange, and the word is the only thing the two share. Admit and release
live on the residency seam and never here, the two seams carrying two
kinds of traffic being the decoder-cut ruling's whole point.

## 3. Ordering

- Open is first on this seam, happens once, and is valid only after the
  residency it serves is confirmed. An open before residency is refused
  and not queued.
- One generation is in flight at a time. A second append-and-generate
  while one is outstanding is refused and not queued, one turn behind
  one intent, per the grammar.
- Cancel is valid only after the session opens, and at rest it answers at
  rest, the validity window being the session rather than the generation.
- Flush is valid only between turns. A flush while a generation is in
  flight is refused, the cancel existing for exactly that case.
- Elision is valid only between turns, on the flush's ground and for its
  reason: a span named against a resident sequence that is still growing
  names positions that will have moved by the time it lands.
- The fault report is emitted only at rest, no exchange outstanding, any
  time after the session opens, and it takes no answer. A fault arising
  while an exchange is outstanding is that exchange's typed answer, per
  section 5, so a report never interleaves a stream.
- Messages within one exchange are ordered, and the intermediate messages
  of append-and-generate, token, field, and column alike, all precede its
answer, and a re-feed's intermediates precede its answer the same way,
token intermediates never among them.
- A retained token's field message precedes the token message its piece
  eventually lands in, because the field is read before the sampler draws
  and a piece exists only after. **The two streams are not joinable on
  this seam and no wording here should suggest they are.** The field
  stream is positionally self-describing, one message per retained token
  naming its own position. The token stream is the emission's rhythm,
  crossing once per renderable piece and carrying no position, a piece
  spanning as many tokens as the family's rendering needs. A consumer
  wanting a position's text joins in the record rather than here, where
  `model.measurement` pairs the output token identifiers positionally,
  and detokenizing is that consumer's own work.
- An answer to append-and-generate arrives only after the session is
  well-framed for the next turn, stopped or complete alike, so the
  harness may rely on the session's coherence without inspecting it.
- An answer to flush arrives only after the kept-prefix state holds.
- An answer to elision arrives only after the shortened state holds, on the
  flush's rule and for its reason.

## 4. What each party supplies and guarantees

Derived from section 2 rather than prose beside it, because every payload
change is a supplies change by construction and a Spec writer reads this
list.

**The harness supplies** the session's identity material at open, the
column ask where and only where its binding is diagnostic, each
turn's context and delta, the re-feed's rendered contribution and path
where and only where its binding is diagnostic, the cancel, the flush, and the
elision's span.
The sampling values are not among them and reach the SPU in the
declaration instead.

**The harness guarantees** that it opens no session before residency
confirms and no second session ever. That one generation is in flight at
a time. That every ask carries the turn's context and that it supplies no
sampling value on this seam at all, the operator-tunable remainder having
crossed once in the declaration and the frozen knobs being the binary's
and crossing nowhere. That it never touches the
cache, holds no handle to it, and derives nothing from the session but
what answers carry. That what it receives it authors faithfully, the
verbatim emission and the canonical parse both reaching the record, per
the operator's end-to-end requirement.

**The SPU supplies** the opened confirmation, the token stream as each is
drawn, the field stream where its election stands, the column stream where
the ask stood at open, one message per sampled position, each generation
with its measurement and elected reductions, each re-feed's typed answer
with the recomputed draws in its measurement, the stopped partials, the
flush confirmation, the elision confirmation with its resident counts
either side, the typed refusals, and the fault reports.

**The SPU guarantees** that the session only advances: nothing ever asks
resident state to rewind, and the weakest family sets the rule for all.
That the identity prefix is permanent from open to release, the flush's
outcome and the elision's included, a span reaching the prefix being
refused rather than trimmed to spare it. That an elision removes exactly
the span it was given and preserves the order of every position it did
not, so what remains is the sequence it was and never a resequencing. That
an aborted generation leaves the session well-framed before its answer
returns. That an elected field is read from the same
distribution the
measurement is read from and before the sampler consumes it, so the two
never describe different states of one position, and that it names its
position rather than relying on arrival order. That an elected field
changes no token: the same declaration and seed produce the same sequence
with the election standing and without it, an observation that altered
what it observed being worthless in a way no consumer could detect. That
the measurement is produced
at production time, positionally paired, absent rather than zeroed, and
that nothing is retained after the answer: produced, reported, gone. That
an overflow refuses, typed, and sheds nothing silently. That a refusal
leaves the session as the directive found it. That it answers a refusal
rather than exiting on one. That every token it streamed appears in the
close's account, so the stream and the answer never disagree about what
was drawn.

## 5. Failure

Refusals are typed and enumerable. The cases:

- the session is not open, or residency is not confirmed, for the ask
- the directive is out of order for the seam's state, a second generation
  or a mid-flight flush above all
- the session cannot take the next delta, the overflow named with the
  session's account of itself
- the delta is malformed for the family, under the framing candidate
- the elision's span describes no removable region: it overlaps the
  identity prefix, runs past the resident count, ends before it starts, or
  is empty with its end equal to its start
- the open carries the column ask against any arm of `weaver-spu-PRD`
  section 13.7's refusal registry - no admitted permission, no readout
  election, no column in the family's declaration - refused at the open
  rather than answered thin at the first sampled position, the registry
  being that clause's and this list citing it
- the re-feed arrives against any arm of `weaver-spu-PRD` section 13.14's
  registry - no admitted re-feed permission, or an empty recorded path -
  the registry being that clause's and this list citing it, the session and
  ordering arms above covering the re-feed as they cover every ask

**Every refusal on this seam is clerked, and the harness is the party that
clerks it.** Per the operator's ruling of 2026-08-22. The SPU answers the
refusal and the harness authors the record's `refusal` event from it, naming
the ask it sent beside the case the SPU returned, per `weaver-trace-PRD`
section 3.1's twenty-first kind. **A flush's `keep` and an elision's span
ride the record because no other event holds them**, and the open's
messages, the append's delta, and the cancel's turn are named without being
reproduced, each reaching the record under its own kind before the exchange
this seam refused. **The SPU authors
nothing**, which is unchanged: this seam has one SPU-originated message and
it is the fault report.

**A refused ask leaves no other mark, which is why the event is owed.**
Before this ruling a refusal between turns reached the loop as an absence
and the record as nothing, so a reader could not tell an ask that was turned
away from an ask never made. Inside a turn it reached the client's close as
prose with its values dropped. **Neither is recoverable after the run**, and
the run most likely to produce a surprising refusal is the first one a new
port is driven in.

**Where a refusal ends a turn the close says so too**, carrying the stop
reason the trace charter adds in the same act. The two are not one fact
twice: the close says the bracket ended and the event says what was refused,
which is the division a fault already runs on here.

**A fault inside a generation is the exchange's typed answer**, naming the
fault the way section 13.10 enumerates, and the turn fails with its
account on the record. **A fault outside any exchange is the SPU's
emitted report.** **A death is neither:** the harness observes closure, the run's
account shows what was authored, and what the harness does with a dead
decoder is the coordination seam's business, the fault travelling to the
record as the death's observation rather than the dead party's report.

**Nothing on this seam retries.** A refused ask returns to the harness,
and a re-sent generation would put two attempts behind one turn.

## 6. Prohibitions

**On the harness.** It opens no exchange this document does not
enumerate. It sends no path. It does not touch, flush by any private
means, or reason about the cache beyond the flush ask this seam carries.
It does not resend history the session already holds, because the resend
is the anti-pattern the append-only protocol exists against. It does not
treat an answer as authorization beyond its exchange.

**On the SPU.** It authors no trace event and holds no handle to the
record. It opens no exchange at all, the fault report being an emission,
and it has no exchange in which to ask, per section 2's dataflow rule, so
an SPU that needs the harness to act has a fault to report and never a
request to make. It retains nothing
across answers and nothing across the residency. It evicts and compacts
nothing, cognition being the harness's. It reaches no other crate.

**On both.** Neither carries a fact about the other's interior beyond
what the exchanges state. The harness does not know how the device is
allocated, the SPU does not know what a turn means, and the family
knowledge of `weaver-spu-PRD` section 14 lives on the SPU's side of the
line, which is the framing candidate's ground.

## 7. Vocabulary

Every contract names the vocabulary it depends on, grouped by the crate
that defines it, and a group is stated even when empty.

**Drawn from `weaver-types`:** the token trio, `token-directive`,
`token-answer`, and `token-refusal`, with the cases section 2 enumerates.
**The name extends the naming ruling and the extension is ratified with
this act.** The ruling names wire vocabulary for the loop whose traffic
it carries, and this seam's loop is loop 1, the builder's and variable
since the composability ruling, so a loop name is exactly what this seam
cannot take. The trio is named for the seam's currency instead, the
token, which is the naming ruling's second case, vocabulary for a seam
whose loop varies, ruled by the human at this act's merge and carried
into the floor's act with the definitions. The definitions land in
`weaver-types-PRD` section 2.3 and
are owed by this act, the demand existing now, and the records are
written unfenced deliberately so a mapper reading this document does not
ingest records this document is not the source of:

    node: token-directive
    kind: vocabulary

    node: token-answer
    kind: vocabulary

    node: token-refusal
    kind: vocabulary

    edge: defines
    from: weaver-types
    to: token-directive

    edge: defines
    from: weaver-types
    to: token-answer

    edge: defines
    from: weaver-types
    to: token-refusal

**Also drawn: `fault-report`**, added on the gate act of 2026-08-02, which
found that the gate's fault had no carriage and settled one definition for
every reporting seam. This seam carries it inside the trio's cases rather
than inside an envelope, the decode socket carrying none, which is the same
definition with a second carriage rather than a second definition.

The session and turn identities ride inside the trio's cases rather than
being drawn as vocabulary of their own: the floor holds `SessionId` and
`TurnKey` as satellite types, no vocabulary node exists for either, and
this contract draws none, the turn context of apex section 5.2 crossing
as fields of the cases the trio enumerates.

**Drawn from `weaver-traits`:** the message model, under the framing
ruling of `weaver-spu-PRD` section 13.4, ratified with this act: the
canonical messages cross this seam into the family library's render, so
the name crosses in one direction, the harness supplying and the SPU
consuming.

**Drawn from `weaver-trace`:** nothing. The SPU reports and the harness
authors, so no event kind and no envelope field crosses this seam. What
the answers carry is aligned with the three `model.*` payload shapes the
token workflow's trace act fixes, and alignment is not a draw: the trace
defines what the record holds and this contract defines what the seam
carries, with the harness converting as author.

**No organ envelope.** Stated as a negative deliberately, per section 0:
this is the one contracted socket seam in the program that does not draw it,
and the absence is the classification rather than an oversight. The egress
ruling of 2026-08-07 charters a second seam that will not draw it either, the
gate's agent-opened socket, and this sentence narrows to contracted seams
until the tool workflow authors that contract and the count is read again.

## 8. Conformance

How each check is implemented is Spec work. What must be checkable:

- A session never rewinds: no operation of this seam reduces the resident
  state except the flush and the elision. The flush's outcome is the kept
  prefix and never less than the identity prefix. The elision's outcome
  holds the identity prefix entire and preserves the order of every
  position it did not remove, so what remains is the sequence it was,
  shorter, and never a resequencing.
- The elision's bounds hold at their edges: a span inside the resident
  count elides exactly its own length, and a span overlapping the identity
  prefix, running past the resident count, ending before it starts, or
  empty is refused rather than clamped, the confirmation's counts agreeing
  with the outcome in every case that holds.
- The cut's bounds hold at their edges: a keep of zero, one inside the
  span, one at the resident count, and one beyond it each resolve to the
  clamped length, the confirmation's counts agreeing with the outcome in
  every case and the identity prefix never cut.
- A close past the envelope bound crosses whole: a generation whose close
  exceeds one envelope arrives reassembled and byte-identical, watched to
  fail when the segment carriage is removed, and a series past the total
  bound faults rather than truncating.
- An interrupted series faults whole: a peer that closes before delivering
  its declared count leaves no partial frame in the receiver's hands, the
  fault the same one a truncation raises.
- An aborted generation leaves the session accepting the next turn's
  delta cleanly, watched to fail when the terminator step is removed.
- The stop lands within a token boundary of the cancel's arrival.
- The measurement is positionally paired and absent-not-zeroed.
- A re-feed's resident outcome is the recorded path exactly, whatever the
  draws said: a position whose recomputed draw diverges appends the
  recorded token like every other, watched to fail when the append is made
  to follow the draw.
- A re-feed streams nothing of its own: no token intermediate crosses it,
  while the field and the column cross where their elections stand, on the
  sites a generation crosses them.
- A re-feed's answer arrives under its own arm and never a generation's,
  so a reader of the seam can never mistake a supplied path for a sampled
  one.
- A re-feed against either arm of `weaver-spu-PRD` section 13.14's registry
  refuses typed before any device work - no admitted permission, or an
  empty recorded path - and the seam's session and ordering refusals cover
  it as they cover every ask.
- Readout reductions appear exactly when the residency was admitted with
  the election, and a tap failure while elected is a fault, never a
  silent absence.
- An overflow refuses before any partial append, the session unchanged.
- One generation in flight, enforced by refusal rather than queueing.
- A report is never emitted while an exchange is outstanding, a fault
  arising then being that exchange's typed answer.
- No sampling value crosses **inbound** on this seam, watched by a capture
  that fails when one reaches the SPU here. The check was written against the
  frozen knobs alone, when the tunable remainder travelled here, and it
  widens to the whole set with the remainder's move to the declaration.
  **The answer is excluded and the exclusion is the point.** `Generation`
  carries the request the SPU rendered whole, its template and its effective
  sampling among it, because the record holds what a turn ran with whichever
  side set it, per charter section 13.8. A capture reading both directions
  would fail on that and call the record a leak.

## 9. What this document changes elsewhere

Named here because a document whose reach cannot be read for the reach is
a trap. The authoritative register is `weaver-spu-PRD` section 11.

- `weaver-types-PRD` section 2.3: the token trio, owed by this act, per
  section 7.
- `weaver-harness-PRD` section 2: landed in this same act, the framing
  ruling ratified, per the register.
- `weaver-trace-PRD` section 3.1: the three `model.*` payload shapes, by
  the token workflow's trace act, aligned with section 2's answers.
- `basic-inference-loop`: the turn path's decode tine now has its seam
  and contract, a citation the next revision of that document carries.
