# weaver-analysis / weaver-web - contract

**Status:** MERGED. In `main` and the source of truth. **Ratification is not
claimed by the act that lands this text.** Whether it has cleared its gates
is the operator's to say.

**Date filed:** 2026-09-05
**Document ID:** `weaver-analysis-web-contract`
**Editorial:** Per the Working Rules. ASCII, absolute dates.
**Landing PR:** #628

## Parties

- **`weaver-analysis`, the emitter and the only sender.** Its signals reader
  rides the shared drain of issue #408, needs no tap, no lens, and no
  weights, and pairs what every record already carries: the tokens drawn at
  each generated position, the entropy the generation measured, and the
  surprisal where that election stands. It decides what to send from the
  record's content and the elections the record declares, and asks nothing
  of the reader.
- **`weaver-web`, the reader and the only receiver.** The connector on the
  agents' own box, per `weaver-web-PRD` section 5, which lands what crosses
  in its own store and draws from the store rather than from the wire. The
  browser is a display engine and holds no end of this seam. Since 2026-09-26 the
  crate and its charter live outside this repository, at
  `WeaverTools_Project/weaver-web/` beside it, on the operator's ruling of that
  date, and this page stays with its sender. **Every `weaver-web-PRD` and
  `weaver-web-Spec` citation on this page reads against `112bc65`**, the last
  `main` commit holding them, per Working Process section 1:
  `git show 112bc65:docs/crates/weaver-web/weaver-web-Spec.md`, and
  `weaver-web-PRD.md` beside it.

No third party reaches this seam. **The agent holds no end of it**: the
emitter parses a finished record outside the agent as an operator principal,
and nothing on this seam reaches the harness, the SPU, or the model.

**This seam is a socket.** The emitter drains a file or a stream and its
emission leaves over a socket the reader consumes, per `weaver-web-Spec`
section 3. It is the second of the reader's two seams, section 7.3's. What
separates it from the gate is what crosses: the gate carries this crate's
own turn out and the answer back, where this seam carries a finished
record of work the crate did not run. **And what separates it from the gate
in the graph is who signed.** Both parties to this page are crates and both
hold a `party` edge, so this pair is checkable from either end. The gate's
page names one crate and signs the world in prose, the graph carrying no
node for a principal outside the program, so as of 2026-09-16 it is the
`via` of no seam record and that boundary is a seam holding no seam edge.

**The emitter initiates and the reader never asks**, so this seam's record
runs `from: weaver-analysis`, corrected on 2026-09-05: it had run from the
reader, disagreeing with this contract's own name under the Document Format's
rule that the initiator is named first. Its tag was `stream`, which is
outside that document's seam vocabulary of `socket` and `link`.

**Neither party asks, so this page says which charter declares the seam.** The
Document Format's branch for a seam with one asking party has no unique answer
where there is none, exactly as it has none on an organ channel, and a question
its branches do not reach is settled here rather than reasoned around in a
charter, per that document's section 7. **The initiating side declares**, which
is where the corpus already puts the other seam this emitter holds, per
`weaver-agents-PRD` section 6 and `weaver-diagnostic-PRD` section 6. So the
record is `weaver-analysis-PRD` section 1's as of 2026-09-16, and the reader's
charter carries none. Nothing about the boundary moves with it: the parties,
the direction and the socket are what the sections below state.

```graph
node: weaver-analysis-web-contract
kind: document

edge: party
from: weaver-analysis-web-contract
to: weaver-analysis

edge: party
from: weaver-analysis-web-contract
to: weaver-web

node: weaver-web
kind: crate

edge: parent
from: weaver-web
to: WeaverTools
```

**The reader's node is declared here since 2026-09-26**, its charter having left this
repository with the crate for `WeaverTools_Project/weaver-web/` (destination commit
`f2f01d8`) on the operator's ruling of that date. A contract whose party lives outside
the repository declares that party's node and its one parent edge, per the Document
Format section 3: the crate's place in this graph's domain tree is a fact of this graph,
so the edge the charter carried stays here, and the party edge above and the seam edge
`weaver-analysis-PRD` section 1 draws point at a node a document declares rather than
dangling.

## Vocabulary

Every contract names the vocabulary it depends on, grouped by where it is
defined, and a group is stated even when empty.

**From `weaver-spu-Spec` section 6.** The **entropy** at a decode position
and the **surprisal** of the token drawn there, both in bits, defined there
and drawn here rather than restated. **The election** that decides whether a
surprisal is measured is the declaration's, per the same authority. The
**input** and **output token identifiers**, the turn's delta and the drawn
tokens, defined there since 2026-09-05.

**From `weaver-spu-Spec` section 3.** The **weights hash**, the record's
identity of the artifact, BLAKE3 over the canonical manifest that section
states, and its **sentinel**, the empty string a hash that could not be
computed reports.

**From `weaver-trace-PRD` section 3.1.** The **`model.measurement`** event
and its members, the output token identifiers, the entropies, the
generation's perplexity, and where their elections stand the surprisals.
The **`model.output`** event and its resident count and capacity as the
generation closed. **`model.field`** and its ranked candidates, which this
seam does not carry and which section 6 names as the other reader's.
**From `weaver-trace-PRD` section 2.1 and `weaver-trace-Spec` section 1**, the
**session**, the identity the runs share, carried on every line's envelope, and the
**run** and its ending at `unload` or at process death per that charter's section 4.3.

**From `weaver-trace-Spec` section 3.** The **`load`** event and the members of its
elections this seam carries: the **field** election's depth, the **lineage** with its
parent, its run and its `through`, which is a turn, and the **stack**, the organ
binaries admin started keyed by name with their sha256. The **`model.request`** event
and its **effective sampling**, which the reader's row splits into a seed and a sampler.

**Defined since 2026-09-05.** That section tables the measurement's members,
and the two section 3 reads were defined at issue #461: the token
identifiers by `weaver-spu-Spec` section 6 and the resident count by
`weaver-types-Spec` section 4.4 with that Spec's section 4 for why it
includes the terminator. This contract's first draft rested on the members'
names and asserted an arithmetic the records refuted, which is why the
definitions were asked before the ask was restated.

**From `weaver-analysis-PRD` section 3.** The **deposit**, what the operator keeps
beside a record, and the two members of it this seam reads: the **device model**, which
that section's licence clause already bounds a report's claims by, and the **code
identity**, its repository commit by hash, toolchain and driver by pinned version, and
engine libraries by sha256. **This group did not exist until 2026-09-09** and the
members crossed nothing before it.

**From `weaver-analysis-Spec`.** The **drain** of section 5, one for the
class with readers above it, and the **signals reader** that rides it.
**That reader's clause landed 2026-09-05 at issue #451**, and this contract
draws its behaviour from there rather than restating it. Section 8 carries
what this contract owes when that clause lands.

**From `weaver-web-Spec`.** The **position**, the **run**, and the **turn**
of section 2.1, which are the store's address, and the **recorded query** of
section 2.6.

**From `weaver-diagnostic-PRD` section 4.** The **certified** close of a
replay bracket, which is the licence a diagnostic reading needs and a
serving reading does not.

**Defined here.** Three nouns, since 2026-09-09. **The record's digest** of section 2.2,
sha256 over a run's own lines as the emitter drained them, and what makes a run whole
for it: no other document holds it, the trace charter's section 4.3 saying in as many
words that the record carries no checksum. **The seated prefix's length** of section
2.2, the resident length before the run's first turn's input as the emitter derives it
from the first generation: the SPU records the figure at open per its section 4.2 and no
event carries it, per the trace charter's section 3.1, so the derivation that recovers
it is this contract's. **The task's verdict** of section 2.2, the predicate the task
answered, whether it held, and the ratio over its denominator as its two terms, which
`weaver-trace`'s `score` kind carries since #707 on issue #523 and whose shape is fixed
here, amended 2026-09-26 when the reader first read it. This contract is the authority
for all three, and `weaver-analysis-Spec` section 5 and `weaver-web-Spec` section 2.2
cite them here. Every other term above has one authority already, and this contract
states what crosses of them and defines none.

## 1. What this contract governs

**The shape a front end graphs from.** The emitter's output today has a
command line's shape, points on one stream and summary on another, and a
shape a view may rely on is a fact between two crates rather than inside
one. This contract names it.

It governs one direction. The emitter sends and the reader receives, and
the reader never asks the emitter for anything: what it wants more of, it
gets by reading a different record or by an act on the emitter's own
charter.

**It asks the emitter for what it does not do today**, named in section 3 and nowhere
else in this document, each on the summary stream: the two counts per generation, the
weights hash, the record's session and digest, and the seated prefix's length. Every
other clause states what the signals reader already emits as of `main` at `20b9cdf`,
per `weaver-analysis-Spec` section 5.

## 2. The traffic

**Two streams, and they stay two.** The per-position series and the
per-generation summary are separate, because they are indexed differently
and because a reader that wants one rarely wants the other at the same
grain.

### 2.1 The series

One entry per generated position, carrying:

```text
turn        the turn key, where the record carries one
ordinal     the position's index within its generation, zero-based
token       the drawn token's identifier
entropy     the distribution's entropy in bits, or absent
surprisal   the drawn token's surprisal in bits, or absent
```

**The position is not a member of this entry.** It is derived by the reader
from the summary's two counts, per section 3, and stored beside the entry
rather than carried on the wire.

**The token identifier crosses and its surface text does not.** Detokenizing
is the reader's, because the tokenizer that answers it is the artifact's and
this seam carries the artifact's identity, on the summary, and never the
artifact.

### 2.2 The summary

**One entry per measured generation, in landing order**, carrying its turn,
its perplexity where the record holds one, **the resident count at the
generation's close beside the count of output tokens**, which are what
section 3's conversion reads, and **the weights hash the generation's
measurement carries**, the record's identity of the artifact per
`weaver-spu-Spec` section 3, spelled as the record spelled it.

**The weights hash is per generation on the wire and once per run in the
reader's row.** A run admits one artifact, so its generations agree, and a
run whose generations disagree is a defect the reader names rather than a
run with two artifacts. **The sentinel crosses as the empty string it is**:
a hash the SPU could not compute is a fact of the record, distinct from a
measurement carrying no member, which crosses absent, and a reader that
folded the two together could not tell a failed identity from an older
record.

**The record's session and the record's digest cross the same way: per
generation on the wire and once per run in the reader's row.** The session is
the envelope's `session` every line of the record carries, per
`weaver-trace-Spec` section 1, spelled as the record spelled it, and it is
the identity the run's session shares with the runs before and after it,
which `weaver-web-Spec` section 2.2 stores so a session's runs are readable
as a family. The digest is sha256 over the bytes of the run's own lines as
the emitter drained them, each line with its terminating newline, in the
order they were read, spelled as lowercase hex, so a reader holding the
record can recompute it and a row can be checked against the record it
descends from. **Every line names a run, and a line belongs to the digest
of the run it names**, the envelope's `run` being a required member per
`weaver-trace-Spec` section 1, so two emitters over one file agree. **A
run's generations
agree on both, and a run whose generations disagree is a defect the reader
names.** **The digest is present only for a whole run, and a run is whole
when its `unload` landed and the drain began at or before its `load`.** A
run that ended at process death has no `unload`, per `weaver-trace-PRD`
section 4.3, its queue's tail forfeited, and an emitter that drained every
line that exists has still not drained the run, and a drain that opened
after the run's first line saw a suffix. In both the digest is absent, so an
absent digest says the emitter could not vouch for the bytes rather than
vouching for the bytes it happened to see, and the reader's refusal on a
mismatch never reads a digest of a truncated record as a digest of the
whole. The session is absent where the record predates the envelope's
session member and never otherwise.

**The seated prefix's length crosses the same way: on every generation's entry, and once
per run in the reader's row.** It is the resident length before the run's first turn's
input, the position that turn's input begins at, which `weaver-spu-Spec` section 4.2 has
the open record and no event carries as a member, per `weaver-trace-PRD` section 3.1.
**The emitter derives it from the run's first generation and from nothing later**: the
closing count less the drawn tokens, less the terminator, less the count of that
generation's input identifiers, the input being the turn's delta with the prefix outside
it per `weaver-spu-Spec` section 6, so what remains is the prefix and only the prefix.
**It is present only where the drain began at or before the run's `load`**, because a
drain that opened later would take some later turn's floor for the prefix and name a
wrong position with a right-looking number, and it is absent where the first generation
carries no closing count or no measurement, each member absent on its own terms. A
restoring run's prefix includes what the load restored, per `weaver-harness-Spec`
section 8, and the figure says so by being larger, which is what a whole-run arm's
branch position needs it to say. Every entry repeats it as every entry repeats the
weights hash, so a run's generations agree on it as they do on the session, and a run
whose entries disagree is the defect the reader names.

**The run identity crosses, and it is the one member every row the reader writes is
keyed by.** It is the envelope's `run`, which every line of the record carries per
`weaver-trace-Spec` section 1 and which the emitter already reads to refuse a second
run's close. Until 2026-09-09 the emitter held it and rendered it nowhere, so a reader
keyed every row by a run the wire never named.

**An absent run identity is an emitter older than 2026-09-09 and never a run without
one**, per section 7's rule for an added member. Every record carries the member on
every line, so the emitter cannot fail to find it, and a reader meeting an emission
without it is meeting the emitter that did not send it. **Such an emission is not
ingestible and the reader says so naming the emitter**, because the run identity keys
every row it would write and there is nothing to key them by. **An absent effective
sampling is the same case at a lower cost**: the reader writes no run row for that run,
the sampler being a condition the row is not a row without.

**What the run ran under crosses beside it, per generation on the wire and once per run
in the reader's row**, which is the weights hash's own rule at the head of this section
and the rule the record's session, its digest and the seated prefix's length already
cross under. Every entry repeats every member it has, and **a run whose entries disagree
is the defect the reader names** rather than a run with two conditions, exactly as for
the four before them. **The rule reaches what a run can agree on and no more**, and two
of these cannot.

**The effective sampling agrees on its declared members and never on the derived seed.**
`weaver-spu-Spec` section 8.5 has each generation draw from a seed derived from the
declared one, the turn's reference and the ordinal, and `model.request` records that
derived value per generation, so **every run of more than one generation disagrees on
the sampling block by construction**. The reader compares the declared members and takes
the derived seed for what it is, a per-generation fact riding a per-run member.

**The task's verdict is authored at the run's close and agrees with nothing.** Every
generation but the last has none to repeat, so the agreement rule would name a defect on
exactly the runs the member exists for. **It crosses on the entry for the generation the
close names**, once, and a second entry carrying one is the defect instead. Each member
is absent on its own terms and never defaulted:

- **the effective sampling**, as `model.request` carries it per `weaver-trace-Spec`
  section 3, spelled as the record spelled it, from which the reader takes both the
  sampler and the seed it names
- **the field election's depth**, the `load` event's `field` election, absent where the
  election did not stand
- **the lineage**, the `load` event's parent session, the run the cut falls in and
  `through`, the turn the holdings stop at, present only where the session stood from a
  record. **`through` is a turn and never a position**, per `weaver-trace-Spec` section
  3, so a reader that wrote it where a branch position is expected would name a place in
  the parent's tape that the number does not mean
- **the device model**, which the operator's deposit names per `weaver-analysis-PRD`
  section 3 and which the licence clause already bounds this emitter's claims by
- **the code identity**, which has two sources and the reader's row joins them into what
it calls the engine at build grain. **The record holds the organ binaries**, the `load`
event's `stack` keyed by binary name with its sha256, which `weaver-trace-Spec` section
3 put there so a record is sufficient for its own conditions without a deposit beside
it. **The deposit holds the rest**, the repository commit by hash, the toolchain and the
driver by pinned version, and the engine libraries by sha256, per `weaver-analysis-PRD`
section 3. **The record is the authority for the organ binaries** where both name them,
the `load` event carrying them since 2026-09-06 for this reason, and that charter's own
list moves with this act
- **the task's verdict** where the record carries one, the predicate the task answered,
  `passed`, whether it held, and `ratio`, the run's `measured` count over the task's
  `denominator`, per issue #523. **The ratio crosses as its two terms and never as a
  quotient**, as the record carries it, so no rounding enters between the record and
  the store, and a task that supplies no denominator sends no ratio

**What the record does not hold is read from a deposit the caller names**, on the
operator's ruling of 2026-09-09. The device model is the deposit's whole, and the code
identity is the deposit's but for the organ binaries the record carries. **The deposit
is named by the invocation and never inferred from the record's path**: this emitter's
sink may be a pipe, per `weaver-analysis-Spec` section 5, and a record arriving on one
has no filesystem sibling to look beside, so a rule that said beside the record would
name nothing on exactly the shape the charter licenses.

**Where the caller names no deposit the device model is absent and the code identity
carries only what the record held.** A run whose silicon nobody recorded is a real run
and the store holds it saying so, which is why the reader's columns for both are
nullable. A default would say the run ran on hardware nobody observed, and a refusal
would make every record drained without a deposit unreadable, including every record
written before deposits were kept.

**The task's verdict has a producer since #707 and a reader since this act**,
2026-09-26. `weaver-trace` carries it as the `score` event, authored at the run's close
by the task through the seat, and the emitter reads that event and places it on the
entry for the generation the close names. **This clause fixed the shape before the
producer existed and is amended by the act that first read it**: the trace carries
whether the predicate held beside its name, which the earlier words "the predicate the
task answered" held in reading and did not spell, and the ratio as its two terms rather
than a quotient. No emitter had sent the member in its earlier reserved shape, so no
reader meets the change, and a record written before #707 carries no verdict. The web
half of this change, the store's reading of the member, is the web repository's since
#689.

**The entry does not depend on the perplexity.** A generation whose record
carries none still carries its counts, because the residency is what a store
keyed by position converts from and it is owed whether or not a perplexity
was taken. **Each member is absent on its own terms**: a perplexity the
record does not hold, a resident count no `model.output` reported. An entry
keyed to any one member's presence would drop the others where they are
still wanted, which is the same absent-not-empty rule section 3 states for
the series applied one grain up.

**An absent resident count costs the reader its positions and not its
entry.** The entry still lands, carrying the turn, the output count, and
the perplexity where one was taken, and what the reader cannot do is form
the address section 4 requires for that generation's points. The summary
says what the record held either way, and a consumer reading it can tell a
generation it could not address from one that was never measured.

**No entry is ever synthesized from the series.** The output count is the
generation's drawn tokens as the record spells them and not the length of
what this seam happened to carry, so a truncated or partially ingested
series never changes what a summary entry says.

### 2.3 The record's own facts ride beside both

**The opening kind and the bracket outcome**, which are how a record says
which record it is and whether its bracket closed certified. Section 5
states what the reader owes on them.

## 3. What the emitter owes

**The series is addressed by the ordinal and the store by the position, and
the summary carries what converts between them.** The two are different
coordinates: the ordinal is the index within a generation, which is what a
series is drawn against, and the position is the resident length at the
draw, which is what `weaver-web-Spec` section 2.1 keys on and what the field
read addresses. **A consumer that treats them as one word addresses the
wrong token.**

**The emitter therefore carries, per generation, the resident count as the generation
closed and the count of output tokens**, beside the perplexity on the summary stream,
**and the record's session and digest per section 2.2**, the emitter being the party
that drained the bytes and the only one that can name them, **and the seated prefix's
length per the same section**, derived from the first generation once per run, **and per
section 2.2 the run identity, the effective sampling, the field election's depth, the
lineage, the device model, the code identity and the task's verdict**, the device model
and the deposit's half of the code identity read from a deposit the invocation names,
never from one looked for beside the record, and the verdict only where the record
carries one. **The output count is the length of that generation's `model.measurement`
`output_tokens` sequence**, the drawn tokens with the terminator outside them per
`weaver-spu-Spec` section 6, and not a separate scalar the record carries. **A
generation whose measurement holds no readable `output_tokens` produces no summary entry
and no points at all**, so `O` is never absent from an entry that exists and a consumer
never meets a half-formed one. Both are facts the record already holds, on
`model.output` and `model.measurement`, so the emitter reports them and derives nothing,
which is the property `weaver-analysis-Spec` section 5 argues for.

**The two counts landed at PR #462 and #463 on 2026-09-05**, with the
definitions the members wanted, and the emitter carries them today.

**The emitter also carries, per generation, the weights hash the
measurement holds**, as of 2026-09-06 per issue #465, so that the run row
of `weaver-web-Spec` section 2.2 has a source on the wire: a reading without
its artifact is a reading of an unnamed compound, and the identity is the
recorded half's fact rather than the declaration's, a declaration naming a
path and being editable after the run. The emitter reports the member as
the record spelled it and derives nothing, per section 2.2.

**The conversion is exact and was measured rather than reasoned.** Where `R`
is the resident count at the generation's close, `O` the count of output
tokens, and `j` the ordinal:

```text
position = (R - O - 1) + j
```

The `- 1` is the turn terminator, which the SPU makes resident before the
answer returns. **It is one token, measured** across three records on the
2026-09-05 hub, nine generations, two precisions, and two finish kinds,
checked against the position `model.field` reports directly. The obvious
alternative, the previous generation's resident count plus the turn's input
delta, is exact from the second generation and wrong on the first by the
session prefix, which the first turn's delta does not carry.

**The arithmetic belongs to the reader and the facts to the emitter.** The
reader derives once at ingest, per `weaver-web-Spec` section 2.7, a value
derived at ingest and stored being one a second reader can reproduce and a
value computed in the interface being one nobody can.

**Absence crosses as absence.** An entropy the generation did not measure
and a surprisal whose election did not stand are absent, never zero. A
vector shorter than the tokens is not stretched and a missing one is not
invented, which is what the emitter does today.

**The elections are the record's and the emitter reports them.** The emitter
does not decide whether a surprisal exists. It reports what the record
carries, and the record carries what the declaration elected.

## 4. What the reader owes

**It draws a spike against an absolute bar in bits.** A bar computed as
`mean + k * deviation` over the series drifts with the body of the
distribution, so a session whose median entropy falls fourfold across its
length manufactures spikes late where none stand. Run 3 of the depth series
is the measurement: the median falls fourfold while tokens above six bits
hold both their rate and their height. **The absolute figure is the one that
held still.**

The series-relative spike rule stays what it is, a rule whose caller
names its `k`, and this clause governs which figure a view may graph from
rather than what the emitter may compute.

**A rarity reference rides beside any spike view.** Surprisal measures
rarity and not wrongness. The OCR ground truth is the measurement: flagged
artifacts clear the page's p95 at 1.2 to 1.7 times the rate of the honest
null, and legitimate words occur once in a whole book. A view that shows a
spike without the means to tell a rare name from damage is inviting the
reader to read damage into a proper noun.

**It plots an absence as an absence.** A view that draws a missing surprisal
as zero is lying about the election, and the ordinary posture of a serving
agent is that the election did not stand. Entropy rides every generation
unconditionally and surprisal does not, so a timeline carrying both will
routinely hold one series and not the other.

**It records what it read.** A reading a second person reruns is a reading
whose query was stored, per `weaver-web-Spec` section 2.6, and the reader
that served it is named there by name and version. This seam is one such
reader.

**It stores what it receives and draws from the store.** A value that must
be derived is derived once at ingest and stored, per `weaver-web-Spec`
section 2.7, so nothing on a screen is computed from the wire.

## 5. The licence, and which records answer

**A serving record has no gate and a diagnostic one does.** A series read
from an uncertified replay is a picture of an unknown run, exactly as a
readout is, per `weaver-diagnostic-PRD` section 4.

The emitter reports the record's opening kind and its bracket outcome and
leaves the judgment to its caller, which is what it does today. **The reader
is that caller and owes the judgment**: a diagnostic record whose bracket
did not close certified is not drawn, and the refusal names the outcome
rather than rendering an empty view.

This gate is `weaver-analysis-Spec` section 5's and is not restated here.

## 6. What neither party may do

**Neither derives a value the record does not carry.** Not a stretched
vector, not an interpolated position, not a perplexity computed from the
series when the record holds none.

**Neither reads the field through this seam.** The ranked candidates at a
position are `model.field`'s and are addressed by `<turn>:<position>`
through the other reader, per `weaver-analysis-Spec` section 5. A series
carrying the alternatives would put twenty thousand positions' worth of
ranked lists on a stream whose whole point is that it is cheap.

**Neither party reaches the agent.** The emitter parses a finished record
and the reader draws from its own store.

**The reader does not present a forced trajectory as a sampled one.** Where
the record says a token was forced, per `weaver-web-PRD` section 3.5, the
series through it is not quotable as one the model produced on its own.

## 7. Change protocol

**The wire shape is versioned and grows additively**, per the compatibility
discipline of issue #338. An added member is optional at the read, and **the
act that adds it says what its absence means**, because a reader meeting an
older emitter must be able to tell a member that was never sent from one
sent empty.

A member is never repurposed and never narrowed in place. A member whose
meaning changes is a new member beside the old, and the old one's retirement
is its own act.

**A change to what the emitter emits is a documents act through this
contract**, not a code act that the contract follows. That is the direction
of the ask in section 3, which is why it is named there rather than
performed here.

## 8. Conformance

**This contract is cited by both parties.** `weaver-web-Spec` section 7.3
took it with the act that landed this text, and `weaver-analysis-Spec`
section 0 names it among what that document is written against as of
2026-09-06, the citation having been owed since the signals reader's clause
landed at issue #451.

**Section 3's asks have landed and the wire tells the states apart.** The
two counts landed at PR #462 and #463 and the weights hash with issue #465,
each an additive member per section 7. A reader meeting an older emitter
finds the counts absent and cannot key its store, whose primary key is the
run, the turn and the position, so it stores what crosses and addresses by
ordinal within a run and turn, converting nothing. A reader finding the
counts and no weights hash can key its store and cannot join the run to
the catalog. Each state is a fact on the wire rather than a version to
negotiate.

**What this contract asserts is asserted in the parties' Specs and not
here.** A contract carries no assertion records of its own, per the
Document Format, and the clauses above trace to `weaver-web-Spec` section 9
on the reader's side, which gains the rows for the derivation at ingest and
the uncertified refusal with the act that lands this text, and to
`weaver-analysis-Spec` section 5 on the emitter's, whose clause for the
signals reader landed 2026-09-05 at issue #451.
