# weaver-harness / weaver-state - contract

**Status:** MERGED. In `main` and the source of truth.

**Date filed:** 2026-08-18
**Document ID:** `weaver-harness-state-contract`
**Editorial:** Per the Working Rules.
**Landing PR:** #58

---

## Parties

- **`weaver-harness`, the feeder and the asker.** Applies the tee of `weaver-trace-PRD`
  section 11 and sends what it elects across this seam. The only party that will ever
  ask, its loop asking through it. Decides everything: what is elected, the schema the
  holdings take, what any held fact is worth, when a save point is taken or restored,
  and what to do with an answer. The loop is state management, per `weaver-agent-PRD`
  section 5.5, and its schema and its judgments are the harness's to carry and never
  this seam's to hold an opinion about.
- **`weaver-state`, the custodian.** Receives the distillate, holds it organized in the
  shape the loop's schema admits, answers asks, and writes or restores a save point when
  asked. Transforms as part of organizing, per its charter, and decides nothing.

No third party reaches this seam. The model has no path to it, per
`weaver-state-PRD` section 2, and no other crate holds an end.

**This seam is a wire.** State crosses a process line, per apex section 9's
re-entry door, so this contract governs a protocol on a transport: a connected
Unix socketpair, stood at load, authenticated by descriptor possession per the
first invariant's rule for a channel with no name, per the operator's ruling of
2026-08-26. Admin creates the pair at the member's spawn and speaks on neither
end: the member inherits its end with its process, the harness receives the
other inside the enter directive beside the trace sink that already crosses
there, and no name exists for any third party to find, replace, or dial. A
standing of the channel is a load's pair, and a reopened channel is the next
load's new pair.

```graph
node: weaver-harness-state-contract
kind: document

edge: party
from: weaver-harness-state-contract
to: weaver-harness

edge: party
from: weaver-harness-state-contract
to: weaver-state
```

## Vocabulary

Every contract names the vocabulary it depends on, grouped by the crate that
defines it. A contract without this clause is not a valid contract, and a group
is stated even when empty.

**From `weaver-trace`.** The `canonical-event`, defined at `weaver-trace-PRD`
section 2.3, and its envelope fields: `session`, `run`, `turn`, `kind`, and
`sequence`, spelled as that crate's canonical form spells them. The distillate
is a projection of the canonical form and never a reshaping of it, so every
name that crosses this seam is a name the record already carries. The harness
draws it.

**From `weaver-types`.** Nothing. The distillate's shape is this seam's own
vocabulary, defined below, and the floor carries no member for it, per the
custody rule of apex section 5.2: the floor carries only what the harness
itself consumes, and what crosses here is consumed by state.

**From `weaver-traits`.** The message model. A pair elected from a message kind's
`role` or `content`, and every pair of a message kind that crosses whole, carries that
member of a conversation message in the shape that crate defines, and this contract
does not redefine it. The harness renders the message from it and state reads the
pair through it, so the name crosses in both directions. It is floor on the
harness's own consumption, the harness being the party that reads a message as a
message, so state drawing it adds no member to the floor, per apex section 5.2.

**From `weaver-spu`.** Three members of the generation measurement, `perplexity`,
`entropies` and `surprisals`, in the spelling and with the absence `weaver-spu-Spec`
section 6 gives them, which is authoritative where this clause and that section
disagree. A pair elected from `model.measurement` at one of the three carries that
reading as the SPU rendered it, and an unproduced reading crosses as no pair at all.
The measurement's other members are the SPU's too and are named here once that
section spells them, until when they cross as values this contract does not name.
State reads the three member by member and the floor carries no measurement shape,
the harness consuming none of it.

**This seam's own.** Five terms. The `election`: the seam's opener, the session the load
declared, the loop's `schema`, and, beside them, three facts the load declared about
what crosses. Whether every kind crosses carrying its envelope or only the kinds named
do, which kinds are named, and the payload key paths elected for each. It is sent whole
at every standing of the channel and never per event. **The same three facts are written
into the record**, on the `load` event's payload per `weaver-trace-Spec` section 3, so a
reader of a finished record knows the rule this seam carried without being told it.
**The first of the three is a term of this seam and not a shorthand for the other two**:
a deployment electing every kind and no paths is saying something a list of kinds cannot
say, and the record that flattened it into a list would be recording a different rule.
The record carries the rule and not the session, which rides the opener by the clause
below and reaches a reader from the envelope of every event. That matters to nobody
standing this seam and to everyone replaying it: a replay preloads under the rule the
record names or it preloads a different session under a rule of its own. **The session
rides the opener rather than the ask**, per the operator's ruling of 2026-08-20 on the
custody defect: it is a load-declared fact standing for the channel's life, the same
shape as the election it sits beside, and a restarted member relearns it with its
reopened channel exactly as it relearns the election. An asker naming its own session on
every question would put the fact on the wire per ask and would make the asking loop
state something it has no reason to know. What the member does with it is its own, per
section 2: each standing ask answers within the declared session by its own definition,
the store holding no boundary between sessions of its own. The `distillate`: one
distilled event, carrying the envelope whole and the elected payload pairs beside it,
each pair a payload key path and the value the canonical JSON held at it. The `ask`: one
question the harness puts to the holdings on the standing channel, carrying a name from
the closed vocabulary section 2 enumerates. The `answer`: the custodian's one reply to
one well-formed ask, sent only when asked and at no other time. The `schema`: the loop's
declaration of the tables its state takes, their keys and constraints, and the rows a
distillate of each kind lands as, carried in the opener, per the operator's ruling of
2026-10-02 on issue #1. **Its content is the loop's and this contract names only its
carriage**: what a schema says is the loop act's to write, and a schema the custodian
cannot stand refuses the opener rather than standing in part.

```graph
edge: draws
from: weaver-harness-state-contract
to: canonical-event

edge: draws
from: weaver-harness-state-contract
to: message-model

node: distillate
kind: term

edge: defines
from: weaver-harness-state-contract
to: distillate

node: election
kind: term

edge: defines
from: weaver-harness-state-contract
to: election

node: ask
kind: term

edge: defines
from: weaver-harness-state-contract
to: ask

node: answer
kind: term

edge: defines
from: weaver-harness-state-contract
to: answer

node: loop-schema
kind: term

edge: defines
from: weaver-harness-state-contract
to: loop-schema
```

## 1. What this contract governs

The one seam between the harness and its state member: the channel's standing, the
ingest traffic, the serve direction, the save points the harness asks for, what each
party owes, how the seam fails, and what neither party may do. It is read alongside
`weaver-state-PRD` and neither is complete without the other.

## 2. The traffic

**Ingest, flowing, one direction, and the election opens it.** The first
traffic on every standing of the channel is the election itself, whole: the
elected kinds and their payload key paths, as the load declared them, and the loop's
schema. The custodian needs the election before the first distillate, because its
indexes are built from it and its schema stood from it at load, and a restarted member
receives the identical election with its reopened channel, which is what keeps the
selection deterministic across the processes of one load. After the opener,
the harness sends a `distillate` per elected event, in sequence order, and
is owed nothing back: the fact has one home, state's holdings, and a
confirmation whose one reader would discard it is the retired receipt's
error again. The harness does not wait, and a distillate is not a turn's
work: nothing about a turn's completion depends on this seam accepting
anything.

**Serve, flowing, the second direction, asked and answered on the one
channel.** The cell that stood here closed 2026-08-19: the first asker
arrived as the context-injection loop and the shape below is elected
against its real ask, per `weaver-state-PRD` section 5. After the ingest's
opener, the harness may put an `ask` on the standing channel at any point
in the stream, and the custodian sends exactly one `answer` per well-formed
ask, in the order the asks arrived, and speaks at no other time. **An ask
is answered against exactly the holdings the seam carried before it**,
which is what makes a served fact attributable to a position in the stream:
every distillate sent ahead of the ask is in the answer's view and nothing
sent after it is. The parked replay ask of the paragraph below is this
clause's one exception, its view being the seal's position rather than the
ask's: it parks precisely because the holdings it is for arrive after it,
so every distillate received through the seal is in its answer's view. The
shape and recall asks keep the pre-ask view without exception.

**State carries across sessions, and each ask's definition says which sessions its
answer reaches**, per the operator's ruling of 2026-10-02 on issue #1, which revises the
session bound of 2026-08-20. The custodian holds one database for the agent across its
runs and its sessions, carried between loads by save point, per `weaver-state-PRD`
section 3, and holds no boundary of its own between sessions. **The asks standing on
that date that read held events, `shape`, `recall`, `replay` and `identity`, answer
within the session the opener declared**, because each was elected to, so holdings of an
earlier session stay outside their view: that bound is each ask's own definition and
never the store's wall. An ask that reaches across sessions enters under section 7 like
any further ask, with the loop act that writes the schema it reads.

**The ask vocabulary is closed and enumerated here, and it holds eight names: `shape`,
`recall`, `replay`, `grants`, `identity`, `snapshot`, `restore`, and `restored`.** The
shape ask carries no members, the session being the opener's, and asks for the session's
shape - what happened, in what order, in which run, which is the phrase the charter uses
for what the default election holds. Its answer carries the session's runs in the order
custody first saw them, each with its run reference and its held event counts by kind,
every name spelled as the envelope spelled it. The counts are organized envelope fact
and carry no judgment: what a kind's count means to a turn is the asking loop's
business, per the three-way division of `weaver-state-PRD` section 2.

**The `recall` ask returns the conversation as custody holds it**, added
2026-08-19 against the context-management loop's need: after a flush the
decode context is empty and the session's knowledge is not, so the loop
asks for the material and composes its own re-entry. The ask carries one
optional member, `last-turns`: a count bounding the answer to the most
recent turns, absent meaning the session whole. The answer carries the
events of the four message kinds, and of `message.restored` where a record written
before 2026-10-02 carries it, in landing order,
each with its envelope whole and its elected pairs beside it - the distillate's
own shape served back - so what returns is exactly what the election kept, no
more recallable than it was distillable. Ordering is custody's organizing licence and
every judgment about what to keep, summarize, or drop in the rebuilt context is the
loop's. **The choice of the message kinds is a loop opinion custody holds for now**, per
the operator's ruling of 2026-10-02: it leaves into the loop's schema with the act that
writes it, under section 7, and so does the identity ask's newest-run rule below.

**The `replay` ask returns the session's elected events whole, in landing
order**, added 2026-08-24 against the diagnostic replay loop's need: the
loop walks a preloaded session positionally, and the message kinds the
recall serves are less than a replay reads, the rendered contributions and
the recorded measurements being the point. The ask carries no members and
the answer serves every held event of the declared session as the
distillate's own shape, envelope and pairs, in landing order, no more
replayable than it was distillable.

**On a member standing with the preload door, a replay ask answers only at a seal.** The
preload door of `weaver-analysis-state-contract` seals its stream, and the ask parks
until a seal has landed, whatever the door's transport is doing: not yet dialed, open
mid-stream, or closed without the seal all park it alike. The last is the clause's
point, because a dead driver's channel closes and its prefix then looks exactly like
holdings at rest, and a retry's opener may yet retire that prefix and seal a whole
preload the parked ask should answer against. So the seal is the only fact that answers,
transport openness answers nothing, and an ask on a standing that never seals is
converted by the asker's bound into the missing answer it always was, per section 3's
bounded-wait rule. **Under a diagnostic load the `identity` ask the harness sends at
the enter parks the same way**, per `weaver-state-PRD` section 3: the replay stands from
holdings the driver has yet to seal, so the enter waits on the seal rather than reading
a store that is empty for a moment, and the identity ask's refusal of the dead-peer
conversion is unchanged, a channel closed unsealed converting to the missed answer that
refuses the enter. A serving load stands no preload door, so its identity ask of
section 2 answers immediately, parking never, on the operator's ruling of 2026-10-06
that the system prompt is state; the record restore of issue #432 and the parked
`recall` it sent retired on the rulings of 2026-10-02 on #58. The replay ask was the one
ask whose answer may wait, and this one joins it under that binding alone, the waiting
is not the custodian initiating, and one answer still follows one ask. **The parked ask
steps out of the arrival order, and that is this clause's stated exception to section
4's ordering rule**: a shape or recall ask arriving while a replay ask parks is answered
in its own arrival order, against the holdings the stream carried before it, except the
enter's `identity` ask under a diagnostic load, which parks on the seal itself per the
clause above and steps out of the order the same way, and the replay's
answer follows the seal whenever that is, its view the seal's position, every distillate
received through the seal in it. What keeps the pairing unambiguous without a
correlation member is that every answer names the ask it answers, per the answer's own
shape, and at most one replay ask parks per channel: **a second replay ask arriving
while one parks replaces it**, the first cleared unanswered, the seal answering the
newest alone. Replacement is the retry's whole mechanism, because the asker's bound is
the asker's own and its expiry crosses this seam as nothing at all: the custodian cannot
clear a parked ask on a fact it cannot see, so what clears one is the next ask or the
channel's close, and an asker whose patience ran out retries by asking again rather than
by any un-ask this seam does not carry. The cleared ask's answer is never owed, its
asker's bound having already converted it to the missing answer. An asker that cannot
tell a late replay answer from a prompt shape answer has not read the answer's name, and
no further identity crosses the seam for it. On a member standing without the preload
door, the ask answers immediately, against the holdings the stream carried before it,
like its two siblings.

**The `grants` ask returns the boundary as the store states it**, added 2026-09-04
against the leave clause of `weaver-trace-PRD` section 3.1: the record names what its
state was built into, and the harness reads the store's grant surface at the enter and
again at the leave so the `unload` event can say whether the boundary moved inside the
session. The ask carries no members. The answer carries one member, `surface`, an
ordered list of strings the custodian renders from its boundary, which with the store in
memory is the member's room: the owner, group and mode of the territory's `state/`,
where it writes its save points, read through the descriptor it holds for the room and
never by path. The lines are spelled so two readings
compare as equal or not and for nothing else: the harness compares and the record
carries the comparison, and custody states the surface and never judges it, per section
4. The ask answers immediately against the store, holding no event and parking never,
like `shape`.

**The `identity` ask returns the session's seated prefix as custody holds it**, added
2026-09-04, and **it governs every load under both bindings** on the operator's ruling
of 2026-10-06 (#1) that the system prompt is state, which supersedes the narrowing of
2026-10-02 to the diagnostic load: the harness asks once at every enter, after this seam
stands, after `restored` at a serving enter, and before the decode session opens, and
seats what custody answers, the prompt the seeding turn entered and a save point
carried, per `weaver-harness-Spec` section 6.1. The ask carries no members. The answer carries one member, `messages`, the turnless
`message.system` events of the declared session's most recent run that holds any, in
landing order, because every load seats the prefix and the prefix door records it again,
so the session holds one prefix per run and the one in force is the newest run's, each
served as the distillate's own shape, envelope and pairs, the pairs being the prefix's
payload whole because that kind crosses the tee whole under every election. An empty
list is an answer and not a miss: it says the preloaded record holds no prefix for the
session, and the open seats the empty identity. **Under a diagnostic load this is one
of the two asks the dead-peer clause of section 5 does not convert**, `restored` being
the other: a missed answer refuses the enter, because a replay whose bounding cannot be
read is not a replay with no bounding. **Where the preload door stands the ask parks on
the seal**, per the clause on the parked replay ask above, so it never answers from a
store the driver has not yet sealed. Where no door stands it answers immediately,
parking never, like `shape`.

**The `snapshot` ask writes a save point and answers where it stands**, added 2026-10-02
on the operator's ruling on issue #1 and shaped by the rulings of the same day on #58:
the store initiates nothing, so a save point is taken when the harness asks, at every
serving leave and on the operator's demand, and never under a diagnostic binding, per
`weaver-harness-Spec` section 6. The ask carries no members. The custodian writes the
whole of its holdings with the schema they stand under as a new file in its own room,
the territory's `state/`, never overwriting one that stands, per `weaver-state-PRD`
section 4, and admin publishes each finished one into the operator's directory at the
next load or unload. The answer carries five members: `save-point`, the name it wrote,
`run` and `sequence`, the trace position of the last distillate it holds, `turn`, the
last turn that run's holdings carry, and `digest`, the digest of its bytes, which the
harness records on the trace as the `save_point` event of `weaver-trace-PRD` section 3. It answers against the
holdings the stream carried before it, like `shape`, and parks never. A write that fails
is not answered and leaves no file under a finished name, the asker's bound converting
the silence into the missing answer of section 5, and the harness records no save point
for it. The answer is a stamp and never the holdings: no byte of a save point crosses
this seam.

**The `restore` ask replaces the holdings from a save point in the member's room**,
added 2026-10-02 on the operator's ruling on #58 that a live restore swaps state without
unloading and that the harness triggers it. The ask carries one member, `save-point`, a
name the member's room holds, and the custodian reads only its own room for it, never a
path the name could lead out of. The custodian replaces its holdings whole with the save
point's, or leaves them as they stood. The answer carries the five members the
`snapshot` answer carries, read from the save point, so the harness records which save
point and which position on the trace before it acts, and a sixth, `identity`, the
prefix the save point holds: the turnless `message.system` events of its newest run that
holds any, in landing order, served as the `identity` ask serves them, an empty list
where it holds none. The live restore is the one serving path that reads a prefix from
the store between loads, because the save point's state was built under its prefix and
a live restore is not a load. Every
distillate the stream carries after the ask lands on the restored holdings. The ask
parks never, and a restore that fails is not answered, the asker's bound converting the
silence into the missing answer of section 5: the harness then flushes nothing, records
the miss, and serves on the holdings the member still holds. An answer lost after the
member swapped its holdings leaves the restore indeterminate, the holdings restored and
the decode cache not flushed. What an indeterminate restore owes, a stopped run or a
commit step that keeps the swap invisible until acknowledged, is a named design item
of the save-point code act, carried on #1. On its answer the harness
flushes the decode session to `keep = 0` through `weaver-harness-spu-decode-contract`'s
existing cut, or, where the answered `identity` differs from the seated prefix, closes
and reopens the decode session with it, per `weaver-harness-Spec` section 6, and this
seam carries nothing of the flush. A save point only in the operator's directory
restores at a load, through the descriptor admin hands the member at spawn, and never
through this ask.

**The `restored` ask answers what the load restored**, added 2026-10-02 on the Planner's
ruling on #58's fourth review round, because the opener is one-way and a schema the
member refuses would otherwise have no path to the enter. The harness sends it at every
serving enter, after the opener and before it authors `load`. The ask carries no
members. The answer carries either `lineage`, the stamp of the save point the member
restored, its digest, run, sequence and last turn, which the harness compares with the
same four members of the enter's lineage and never with the members admin alone
resolves, or nothing where it stood empty with
no save point handed to it, or `refused` with a reason, `schema-mismatch` where the save
point's schema is not the opener's. The member judges the save point at the opener,
holds the outcome, and answers it immediately, parking never. **A refusal or a miss
refuses the enter**, on the identity ask's rule that the dead-peer clause of section 5
does not convert it: an enter that cannot say what its state was restored from must not
author a `load` event that says it. The opener stays one-way, and the save point's bytes
never reach this ask, admin having judged them at the inventory.

A further ask name is a change under section 7 and does not exist until it merges there.

## 3. What the harness owes

- **The election applied faithfully.** Every event matching the election
  crosses, whole per the election, in the order the record assigned. The
  harness neither thins what was elected nor adds what was not. **The save-point
  kinds are outside every election**, on the operator's rulings of 2026-10-02 on #58:
  taking, restoring and resetting a save point are provenance about the holdings,
  authored after the holdings they name, so no election matches them, `all_kinds`
  included, and the tee never sends them, per `weaver-trace-PRD` section 3.
- **The envelope always.** Every distillate carries all five envelope fields
  as the canonical form spelled them. An unattributable distillate is a
  defect in the sender.
- **Its own judgment reaches the holdings only through the record.** What a fact is
  worth is the loop's business, and where the loop judges, a classification or a
  ranking among them, the harness records the judgment on the trace and it crosses
  this seam as the distillate of that event and in no other form. No ask carries a
  judgment and no answer is ranked by the custodian: **the custodian serves what its
  schema holds, and the ranking is the loop's**, held where its schema put it.
- **Asks from the enumerated vocabulary only, and a bounded wait.** The
  harness sends no ask this contract does not name, and it does not wait
  unboundedly for an answer: an answer that has not arrived inside the
  harness's own bound is a missing answer, treated as the dead peer of
  section 5, and the turn proceeds without the fact.

## 4. What state owes

- **Custody whole.** What arrived is held, organized, and attributable by its
  envelope, and nothing that arrived is judged, ranked, or discarded by any
  policy of the custodian's own. Retention is total across runs and sessions in the
  neutral substrate, and the loop's tables hold what its schema admits, a
  distillate the schema refuses landing there alone.
- **The save point and the restore faithful.** A save point holds exactly the
  holdings the stream carried before its ask, stamped with the position they cover, and
  is never overwritten. A load or a live restore from it holds exactly what it holds,
  and a rebuild from the trace through that position, honouring every reset the record
  names, holds the same.
- **Transformation without judgment.** Derived shapes, aggregates, and
  indexes are custody's work and carry no opinion about what a turn should
  do, per the three-way division of `weaver-state-PRD` section 2.
- **Served as it crossed.** Every pair an answer serves is the value that
  crossed, byte for byte, whatever custody holds it as. A member the
  Vocabulary names may be held typed and every other is held as it crossed,
  and neither changes what an answer spells.
- **The answer, only when asked.** Exactly one answer per well-formed ask,
  in arrival order, each answered against the holdings the stream carried
  before its ask, and no other traffic ever. The one stated exception is
  the parked replay ask of section 2, which later asks lawfully pass. A
  custodian that spoke
  unasked would be initiating, which its charter forbids.

## 5. Failure vocabulary

**A dead peer costs the distillate and never the turn.** If state is gone,
the harness observes closure, drops what it would have sent, and serves turns
exactly as it did before the leg existed, per the loss clause of
`weaver-state-PRD` section 3. The holdings meanwhile stand in the latest save point and
the record, and the next load resets to the latest known-good save point and records
the reset on the trace, the distillates landed since it kept by the record and replayed
by no default, per `weaver-state-PRD` section 4. There is no buffering, no
retry, and no backpressure onto the turn path: the derivative is rebuildable
from the record, so the cheapest honest answer to a broken seam is to stop
distilling until the next load.

**A malformed distillate is the sender's defect.** State refuses it by
closing nothing: the event is dropped, the defect is state's to surface when
the serve direction gives it a voice, and the record remains authoritative
for what was elected. The seam does not fault the worker for a bad row.

**A dead or silent peer costs the answer and never the turn.** The serve direction fails
the way the ingest does: where state is gone, or an answer does not arrive inside the
harness's bound, the harness proceeds as if nothing were held, the loop composes its
turn without the fact, and no retry follows on this standing of the channel. A malformed
ask is dropped by the custodian without an answer, which the harness's bound converts
into the same missing-answer outcome, and a malformed answer is dropped by the harness
to the same effect. In every one of these the record remains whole and the next load's
channel asks again against holdings that never moved. The `identity` ask of section 2 at
a diagnostic load is the stated exception, ruled 2026-09-04 and narrowed 2026-10-02: its
miss refuses the enter rather than costing an answer, the turn it would cost being every
turn of the replay. The `restored` ask at a serving enter is the second, ruled
2026-10-02 on #58: its miss or refusal refuses the enter, a `load` event naming state
the member never restored being an untrue record.

## 6. What neither party may do

- Neither party writes the trace through this seam, in either direction. The
  distillate is a projection of the record and nothing here flows back.
- Neither party exposes this seam to the model. There is no tool, no verb,
  and no path from the loop's interior to either end.
- Neither party carries state across sessions through this seam. The holdings
  cross by save point, written into the member's room and published by admin, per
  `weaver-state-PRD` section 4, and no byte of a save point crosses here in either
  direction.
- Neither party hands the agent a save point or its own raw trace. What the agent
  receives is what the loop serves it as composed context, per the operator's
  ruling of 2026-10-02.

## 7. Change protocol

A change to the distillate's shape, to the election's semantics, to the
ask vocabulary, or to the answer's shape or its ordering guarantees
touches this contract, and every party merges in the same act. The serve
direction's first shape was a change under this protocol and landed
2026-08-19 with the loop act the charter named, which is the
sentence above kept as the rule it demonstrated: a second ask name enters
by the same door. **The schema's content is not a change under this protocol**: what
the loop's schema says is the loop's, and a new schema is a new load's opener, like a
new election. Its carriage in the opener is this contract's, and so is any ask that
serves what a schema holds.

## 8. Conformance

The ingest direction is testable against the living producer: a real load, a
real election, real events crossing, and the holdings queried for exactly
what the election named, attributable by envelope. The dead-peer clause is
testable by killing the member mid-run and watching the turn path not
notice. Both land with the code act that opens the seam.

The replay ask is testable against the living pair under a diagnostic load:
a preload landed and sealed, the ask answered with every elected event in
landing order, and the same ask observed waiting in all three unsealed
states, before the dial, mid-stream, and after a sealless close, rather
than answering over emptiness or a prefix, which is the perturbation
its assertion in `weaver-state-Spec` section 4 watches. The retry sequence
is its own case: a parked ask outlived by its asker's bound, a second
replay ask replacing it on the same channel, the seal landing, and the
answer arriving once, for the newest ask alone, the replaced one cleared
unanswered.

The serve direction is testable against the living pair: a real load, real
events landed, and the shape ask answered with exactly the runs and counts
the record shows for the session, in first-seen order. The recall ask is
testable the same way: the answer's events are exactly the elected
message-kind rows, in landing order, bounded to the named turn count where
one was given, byte-faithful to what the tee carried in. The answered-against
clause is testable in time: asks interleaved with distillates, each answer
holding every count the stream carried before its ask and nothing sent
after it. The serve half of the dead-peer clause is testable by asking with
the member gone and watching the turn complete without the fact inside the
bound. All three land with the loop act that shapes the surface.

The snapshot and restore asks are testable against the living pair: a real load, real
events landed, the snapshot ask answered with the position of the last distillate it
carried before the ask and a name no earlier save point holds, a live restore of that
save point after further landings answering the same position and holding what the
first held at the ask, a second load restoring it through the spawn descriptor holding
the same, and a rebuild from the record through the stamped position holding the same,
ask for ask. They land with the code act that stands the store primitive.
