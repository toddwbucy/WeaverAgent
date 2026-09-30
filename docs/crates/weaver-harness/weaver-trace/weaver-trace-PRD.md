# weaver-trace - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. The crate PRD set is
written together and merged together, and no Spec is written against any member
before the whole set is merged. **Ratification became per-charter on 2026-08-23**, per
Working
Process section 2 as amended, so this document is ratified on its own terms rather
than waiting on the set. The set-wide act of 2026-08-04 established the pattern it
conforms to.

**Date filed:** 2026-07-29
**Document ID:** `weaver-trace-PRD`
**Parent:** `weaver-harness-PRD`
**Companion contract:** `weaver-harness-trace-contract`, written with this document
**Editorial:** Per the Working Rules.
**Landing PR:** #613

---

## 1. What this crate is

`weaver-trace` defines the primary artifact. It depends on no other Weaver crate,
binds no socket, and holds no cognition.

**It is not a floor crate.** The floor is `weaver-traits` and `weaver-types`, which
every domain draws from and none contains. This crate has one caller. The harness
authors every event and no other crate submits one, and a reader of a finished record is
downstream of a file rather than a party to this crate. An earlier reading grouped it
with the floor, and that grouping is residue from before sole-writer collapsed the
five-party trace seam to two. Depending on nothing is what this crate has in common with
the floor. Being drawn by everything is what it does not. The no-dependency claim
survives contact with the message kinds only because their payloads are opaque here,
which section 3 states rather than assumes. It defines what an event is, renders each
one to canonical form once, holds that rendering in RAM as the working structure,
and hands the same rendering to the outbound stream.

```graph
node: weaver-trace
kind: crate

edge: parent
from: weaver-trace
to: weaver-harness
```

**It does not produce the trace.** The harness authors. This crate is the mechanism the
harness authors through, and the distinction is the whole of the charter. `weaver-trace`
decides nothing about what is worth recording, when a session begins, or what a turn
is. Every one of those is policy and every one is the harness's. This crate
guarantees that what it is handed is recorded faithfully, ordered correctly, and
readable afterward.

The previous tree had no document governing production at all. It had a schema
contract, an access contract, and a draft custody contract, and nothing binding the
sole producer on how it produces. Every defect that corpus records is on the
producer side, which is why this charter is organized around production and why it
is written together with the contract that binds it.

## 2. The artifact

### 2.1 Three nested units

**A turn** is one request through to its final answer, bounded by `turn.started`
and `turn.closed`.

**A run** is one residency working on a session, and a session opens with its
first. A run
ends at unload or at process death, and the identifier is a reference admin
mints at the load and distinct within the session, per `weaver-admin-PRD`
section 10, so which run produced an event is answerable from the event alone.
It is not an ordinal: nothing program-side counts runs, because nothing
program-side is alive between two of them.

**A session** is the identity the runs share, and the unit the agent's conversation
belongs to. It is the boundary the proto-stateful definition of apex section 2 is
drawn against: a new session begins with nothing. What continuity a later run of
the same session holds is the enter cell `weaver-admin-PRD` section 10 names, the
program promising none since the ruling of 2026-08-01, and the stream's account of
the session accumulating on the operator's side rather than in anything the
program keeps.

### 2.2 The session has two materializations

The **working structure** is the session in RAM: the run's admitted events in the
same canonical NDJSON form the stream carries, held in order, per the ruling of
2026-08-01 that retired the relational projection. It is what the harness reasons
over, which makes it state rather than a report about state, and the loop's reads
are sequential by design, the message sequence and the tool events in the order
they landed. It lives for one run, and it runs ahead of the outbound stream by the
depth of the writer's queue, per section 4.2.

**The working structure is append-only by construction.** It offers no update
surface, no delete surface, and no mechanism a caller could reach to alter an event
after it has landed. This is a structural guarantee rather than a behavioral one.
The agent runs as its own UID with access to bash, and a mutable in-process store
reachable through any path that UID can touch is a store the agent could alter.
A misconfigured sudoers entry, a future dependency, or a bug in a calling crate
cannot produce a mutation because the structure offers no mutation. The audit trust
property this crate exists to protect requires that guarantee to be architectural,
not a promise that everyone agreed to keep.

The **stream** is the session's outbound account: the same admitted events in the
same canonical NDJSON form, one event per line, in order, written to the sink the
operator declares and `weaver-admin` connects at load. Durability is the operator's,
per `weaver-admin-operator-contract` section 3, and the program holds nothing of the
stream once it has left the writer.

**The session record is what the stream accumulates, and the operator owns it.**
The name survives the ruling of 2026-08-01 because the thing it names survives: an
append-only, sequence-faithful, canonical account of the session, outliving process
and run both. What changed is custody. The record lives on the operator's side of
the sink, in whatever storage that operator's tooling keeps, and the program neither
reads it back nor vouches for what stands behind the descriptor. The agent has no
path to the sink, per `weaver-harness-PRD` section 5, so the custody that matters,
the agent's exclusion from its own account, does not depend on who persists it.

```graph
node: session-record
kind: artifact
```

### 2.3 One rendering, held and handed, and nothing to reconcile

The working structure and the stream carry the same canonical rendering, one held
and one handed, so no reconciliation between them can be owed and none exists.
An earlier version of this section guaranteed a deterministic projection from
events to rows, and the guarantee dissolved with the rows: with one representation
there is nothing a projection version could govern and nothing a divergence could
open between. A consumer of the stream that wants rows, an index, or any other
shape builds a derived view on its own compute from the same bytes, and a derived
view names its source range and is never a durable home, per section 8.

An earlier version of this section also made recreation the session-resume
mechanism, `run1` opening the record, projecting it, and appending. That mechanism
dissolved with the program-owned record under the ruling of 2026-08-01. Every run
begins with an empty working structure, the program promises no resume, and what
continuity across runs becomes, against operator-held storage or the memory round,
is the enter cell `weaver-admin-PRD` section 10 holds.

**The rendering has a name, and two contracts outside this crate draw it.** The
`canonical-event` is one event in the form this section holds and hands, envelope
and payload together, and it is the unit a reader of the record meets.
`weaver-harness-state-contract` draws it for the tee's distillate and
`weaver-analysis-state-contract` draws it for the driver's, each of those being a
projection of this form and never a reshaping of it, so both clauses name what this
crate spells rather than spelling it again. It is defined here because the rendering
is this crate's.

**Three nodes stand over one form, and section 3 owns the parts.** `event-envelope`
and `payload-shapes`, declared there, are what an event is made of, the one envelope
every event carries and the payload its kind carries beside it, and `canonical-event`
is the whole the record hands out. The alternative this act weighed was re-pointing
the two state contracts at those two rather than declaring a third node, and it is
declined: each of those seams depends on the event as a unit, its distillate being a
projection of one canonical event with the envelope whole and the elected payload
pairs beside it, so drawing the halves would name the parts and leave the unit the
clause rests on unnamed, and it would overload two nodes
`weaver-harness-trace-contract` draws for the recorder's own seam. The relation is
stated here rather than drawn, the format carrying no edge between a meaning and its
parts, so a reader meeting any of the three learns from this paragraph which is
which: section 2.3 owns the whole and section 3 owns the parts.

```graph
node: canonical-event
kind: vocabulary

edge: defines
from: weaver-trace
to: canonical-event
```

### 2.4 Spans

Spans are neither materialization and they are not stored. A span is a view over a range
of events, derived on demand for interoperability with tools that speak spans. It
has no durable home, no residency, and no authority. The previous tree wrote closed
spans into its durable log as events, which made spans a durable primitive, which
inverted its own ruling that a derived span is never the durable home, and which
left a separate span export that was a projection of data already stored in
projected form. Its own crate Spec could not resolve the resulting redundancy. This
charter resolves it by honoring the original rule: events are durable, spans are
derived, and nothing is stored twice.

### 2.5 Schema authority

The single emission is authored against the durable event schema, and that schema
is the only schema. The working structure holds the same canonical form rather
than a second representation, so no projection version exists to govern anything,
per the ruling of 2026-08-01. A change to the durable event schema is the breaking
change, and it is the one version every consumer keys on.

## 3. Events are typed, and that is a safety property

An event carries an envelope and a payload. The envelope identifies the session, the
run, the turn, the sequence, the kind, the producing subsystem, the causal parent,
and **two timestamps**: a session-scoped wall-clock stamp at
millisecond resolution for the calendar question, and a run-scoped monotonic reading
at nanosecond source with a microsecond floor for interval measurement. The two are
not interchangeable and neither answers the other's question, which is why there is no
single occurrence time. `weaver-harness-trace-contract` section 3 states which party
stamps them and why the scopes differ. The payload shape is determined by the kind,
and the kind vocabulary is closed.

**Five payload shapes are opaque to this crate.** `message.system`,
`message.user`, `message.assistant`, `message.tool_result`, and `message.restored` carry
conversation messages in the shape `weaver-traits` defines, and this crate neither
defines that shape nor decodes it. It records the octets, sequences them, and carries
them as opaque content. Decoding is the harness's, which links `weaver-traits` and is
the only party that reads a message as a message.

This is the demand rule rather than a convenience. Section 6 guarantees canonical
byte form, a gapless run-scoped sequence, an interrogable committed boundary, whole
events to the sink, one rendering held and handed, and typed refusal, and not one
of them requires knowing what a message says. A crate that depends on no other
linking a definition it does not need gives up the property for nothing, so the
alternative reading, where
`weaver-trace` floor-links `weaver-traits` to decode three kinds, is refused on the
same grounds section 4 refuses everything else it does not demand.

**What refusal on submit means for these three.** The `payload does not decode for
its kind` case of `weaver-harness-trace-contract` section 5 reaches the envelope
binding and the octet well-formedness, not the message's interior. A malformed
conversation message is a defect the harness catches before submitting, because the
harness is where a message is a message.

```graph
node: event-envelope
kind: vocabulary

node: event-kind-set
kind: vocabulary

node: payload-shapes
kind: vocabulary

node: commit-boundary-states
kind: vocabulary

node: working-structure
kind: vocabulary

node: failure-vocabulary
kind: vocabulary

edge: defines
from: weaver-trace
to: event-envelope

edge: defines
from: weaver-trace
to: event-kind-set

edge: defines
from: weaver-trace
to: payload-shapes

edge: defines
from: weaver-trace
to: commit-boundary-states

edge: defines
from: weaver-trace
to: working-structure

edge: defines
from: weaver-trace
to: failure-vocabulary
```

These six are what `weaver-harness-trace-contract` draws from this crate, so the
union check of G4 runs against this list rather than against a reading. The
`canonical-event` of section 2.3 is this crate's seventh definition and is not among
them, the two state contracts drawing it and that contract drawing these.

**One field is recorder-assigned, the rest harness-supplied.** The sequence is
computed by the recorder on admission, because ordering is a property of the account
rather than a fact the harness holds. The hashes an earlier version assigned beside
it left under ruling B and the cut of 2026-08-01. Everything else is a fact only the
harness holds and the recorder could not derive.

**The recorder assigns properties of the account. It never authors content.** That
is the line. A sequence is a fact about where an event sits rather than a statement
about what happened. An event is content, which is why the harness authors every one
of them and why section 3.1 refuses a commit-checkpoint kind. Assigning a field to
an event the harness authored does not make the recorder an author. Emitting an
event the harness did not author would.

### 3.1 The closed kind set

The kind vocabulary is closed, and this is where it lives. A set declared closed and
held nowhere cannot be checked against, and the compile-time pin above is argued from
its closure.

**The set is flat, and every kind is recorded when it occurs**, per section 5:

| Kind | Meaning |
|---|---|
| `load` | opens a run |
| `unload` | closes a run |
| `session.closed` | the session will not be resumed |
| `turn.started` / `turn.closed` | the turn bracket |
| `flush` | the decode context returned to its prefix, the loop's election, resident counts before and after |
| `message.system` | the framing of the field, the loop's or the operator's |
| `message.user` | operator input |
| `message.assistant` | model output as conversation |
| `message.tool_result` | a tool result entering the conversation |
| `tool.call.started` / `tool.call.completed` | the tool bracket |
| `fault` | a fault the worker survived, reported by an organ and authored by the harness |
| `model.request` | the decode boundary, request side |
| `model.output` | the decode boundary, response side, with the session's resident count and capacity as the generation closed |
| `model.measurement` | input and output token identifiers, entropies, the generation's perplexity, the decode timings, model identity with its weights hash, the prompt-block partition, and where their elections stand the surprisals and the residual reductions with the two counts that describe their shape |
| `classify.request` | the label seam, request side: the content the loop sent for classification |
| `classify.output` | the label seam, response side: every label of the artifact's head scored |
| `model.field` | one decode position's ranked candidates with their probabilities and the rank the draw landed on, when the field election stands |
| `elision` | an interior span of the decode context made absent, the loop's election, the span it named beside the resident counts either side |
| `refusal` | a typed refusal clerked to the record, one kind for every seam, the organ's case carried opaque as `fault` does |
| `recall` | an answered ask on the state seam, what was asked and the identities of the events custody answered with, never their contents |
| `message.restored` | one message of a restored conversation, seated at a restoring load's open ahead of every turn, the message whole |
| `score` | a task's verdict on its run at the run's close, the predicate answered and whether it held, and the ratio over the task's denominator as its two terms where one exists |

Twenty-four kinds. Adding one is an edit to this charter and to every contract whose
vocabulary clause names the set, because consumers key on the closure.
**`model.field` is the nineteenth**, added 2026-08-21 on `weaver-spu-PRD`
section 13.11's election. It is the first kind recorded per decode position
rather than per turn or per exchange, and the granularity is forced rather
than chosen: the field is the ordering of candidates at one position, and a
per-generation collapse of it would be a single message of megabytes,
past the decode seam's own bound. **It is recorded only while its election
stands**, which is the first kind of which that is true, and the exception
is what the election exists to buy: the volume is an order of magnitude
past everything else the record carries, so a kind always present would
make the record unaffordable rather than complete.

**`elision` is the twentieth and `refusal` the twenty-first**, both of
2026-08-22 and both argued at their own clauses below - the table and the
count word above caught up to them on 2026-08-26. **`recall` is the twenty-second**, on
the operator's ruling of 2026-09-26 under epic #690: the M1 run's post-flush input was
drawn from a state-seam ask the record did not hold, and `weaver-trace-Spec` section 3
argues the shape. **`message.restored` is the twenty-third**, on the operator's ruling
of 2026-09-26 under the same epic: a branch's record did not hold the conversation its
model was opened with, each restored message refused turnless under its turned kind, and
the same section argues the shape. **`score` is the twenty-fourth**, on the operator's
rulings of 2026-09-09 and 2026-09-26 on issue #523: a task's verdict on a run is a fact
about that run, so the record carries it, and the same section argues the shape.

**The record declares the posture it was written in.** Section 3.1's `load`
event carries every diagnostic election of the load by name, because
without it a record holding no field is indistinguishable from a record
whose election stood and produced nothing - a configuration and a fault
wearing the same absence. No election is bundled under a profile name: a
named set drifts as features are added to it, and every earlier record
carrying the name silently becomes a record of something else, which is
the provenance defect this charter exists to prevent.

`classify.request` and `classify.output` are the seventeenth and eighteenth,
added 2026-08-19 with the classifier's charter, per `weaver-spu-PRD` section
15.5's demand: every exchange of the label seam is recorded by the sole
writer, an unrecorded semantic exchange being the precedent that charter
refuses. The pair brackets nothing and each is recorded when it occurs, the
turn member optional because a classify between turns belongs to no turn, a
death mid-exchange authors no fabricated answer, and what a closure means is
the fault custody rule's.
**`classify.output` stopped carrying a refusal 2026-08-22.** Per the
operator's ruling on the refusal class: the outcome the seam scored is one
fact and a refusal is another, and carrying both under one kind made a
reader disambiguate on every read. A refused classify now authors
`classify.request` and then `refusal`, where a scored one authors
`classify.request` and then `classify.output`. **The exchange is still
recorded whole**, which is what section 15.5 of the SPU's charter demands,
and what changed is which kind carries the half that was refused.

**This retires the free-form shape the class was written against.** The
refusal travelled as a string, which is the one thing the ruling of this
date rejects: a consumer dispatches on a declared type rather than parsing
prose, and the one refusal that already reached the record was the one
example of the shape being refused. **Records written before this act carry
the old shape**, and the operator's ruling accepts that: the program is in
development, its traces are rebuilt against each binary, and a permanent
exception in the place the class is meant to be uniform would have cost
more than the discontinuity does.

`flush` is the sixteenth, added 2026-08-19 by the ruling of 2026-08-12's own
condition: a reachable flush adds its event, and the context-management arc
made it reachable. Its payload carries the resident token counts before and
after, so the record accounts for a context that shrank: the model.request
ruling reads the effective context by accumulation, and an unrecorded flush
would break that reading silently.
`message.system` is the fifteenth, added under that rule 2026-08-19 with the
floor's `Role::System`: the slot the system prompt rode user role for since
the control loop's first act. `fault` is
the fourteenth, added under that rule by the fault-carrier ruling of 2026-08-01:
the stream is the program's one fault carrier, so a fault the worker survives is an
event like every other, recorded whenever it occurs because a fault absent from an
account would be the silently partial record this charter forbids. The
operator's tooling keys on it from the stream and comes back through the operator
surface with a verb. Its payload names the raising organ and carries what that
organ reported, and the case set behind it is deferred to the token workflow with
the organs' own charters, per `weaver-spu-PRD` section 10.

**Apex section 8's deterministic re-feed rests on five inputs, and since the
trace act of 2026-08-02 they span two kinds of one turn rather than one
payload.** Replay feeds recorded tokens back through a forward pass rather
than re-sampling, which needs the input token identifiers, the output token
identifiers, the model identity and its weights hash, the prompt-block
partition, and the sampling parameters, five items displayed as five because
the apex splits the identifiers and joins the identity to its hash. **The
first four are `model.measurement`'s and the fifth is `model.request`'s,**
the sampling values having moved to the request in that act because an input
to a decode belongs with what was asked rather than with what was read off
it. The join is the turn: both kinds carry the same
turn in the envelope, so a replay driver reads the pair rather than one
payload, and this sentence exists because a driver built against the earlier
single-payload reading would find four of five and conclude the record does
not support the arrangement every record supports. The prompt-block partition
and the explicit identity were
absent from an earlier version of this row. A payload carrying a weights hash and no
identity says which bytes were loaded without saying what was loaded, which is enough
to detect that two runs differ and not enough to reconstitute either, and a payload
without the partition cannot tell a replay where one block ended and the next began.

**Replay is universal since the levels retired on 2026-08-02, and the apex's
requirement is claim-relative since the correction of the same date.** Every run
carries its measurement payload, so the deterministic re-feed arrangement's
inputs are in every record and every record names the model that served it.
What the apex forbids, per its section 8 as corrected, is a record missing an
input of an arrangement it claims. Which further arrangements a record
supports, stochastic re-entry above all, follows from what the deployment
declared and produced, elections at the source governing production and
production governing the record, which is this charter's own section 5
instrumentation rule read forward.

**There is no `session.started`.** A session begins when its first run begins, so a
separate
kind would fire at the same moment as that run's `load` and mean the same thing. The
same argument retires the description of `weaver-admin`'s initial contact as a
distinct first entry: **the `load` event of the session's first run is that
contact's record.** Admin
authorizes the transition and hands it across, the harness authors its `load` on
entering the run, and the monotonic origin is captured at that event. There is no
room between those for an earlier entry, and the worker spawn and descriptor handoff
precede the harness existing at all, so they sit outside the trace by construction.

**There is no commit-checkpoint kind.** The committed boundary is interrogable from
the recorder's boundary report, per section 4.2, and an event marking it would be
the recorder authoring into an account the harness is the sole writer of.

**This enumeration appears in the contract as well, deliberately.** A Spec is derived
from its crate's PRD plus every contract that crate is party to, so the harness Spec
writer reads the contract and never opens this charter. The contract must therefore
carry the field set rather than point at it. The two copies are not redundant, because
this one says what an event **is** and the contract's says what the harness
**supplies**, but they assert a property of the same thing and can diverge. **If they
ever disagree, this charter is authoritative** for the envelope, because this crate
defines the artifact and consumers outside that contract read the record. A divergence
is a defect to file rather than one a reader resolves by choosing.

**The emission path is typed values, not instrumentation macros.** This is a design
requirement rather than an implementation preference, and the reason is a defect
the previous tree documented against itself. Its span layer took field names as
literal tokens at the emit site, so nothing connected an emitter to the declared
attribute vocabulary. The result was that four of the most operationally valuable
measurements in the system, the decode timing fields, appear nowhere in the
vocabulary at all, and a span kind outside the closed set was emitted at three
sites. The Spec's own conclusion was that the vocabulary could not be read as the
schema.

A typed event with a closed kind and a typed payload cannot carry an invented
field, because the compiler refuses it. The apex asks for compile-time pins where a
runtime test structurally cannot reach. This is one, obtained by construction
rather than by adding a check that someone must remember to run.

Adding an event kind is a change to this charter and to the contract. It is not a
free extension, because consumers key on the closed set.

**`load` and `unload` are event kinds, and they are the run bracket.** Entering a
run writes its `load` event and leaving it writes its `unload` event, both authored
by the harness. The minimal run is those two events with nothing between, so **a run
cannot be empty**, and that is what makes run numbers verifiable by whoever reads
the account rather than merely declared. The program runs no such check since the
cut of 2026-08-01, the reading being the consumer's, and the property holds either
way because it is a property of what is written.

**The `load` event carries the diagnostic elections of its load, each by
name.** A record has to declare the posture it was written in, per section
3.1: an election that stood and produced nothing and an election that was
never made are the same absence on disk, and the difference between them
is a fault and a configuration. Naming them at the bracket's opening puts
the declaration where a reader meets it before any event it governs, and
costs one event per run rather than one member per event. What it names
is each election individually and never a profile, for the drift reason
section 3.1 states.

**The tee's election is one of them and was left out.** The three the `load` event named
until 2026-08-25 were the readout, the field's depth, and the surprisal, each an
election about what the model reports. The tee's is an election about what leaves the
record for the state member, and the clause above reaches it unchanged: absent the
member, a tee that elected nothing and a tee that was never asked are the same absence
on disk. **From this act they are not**, the rule being written whole on every `load`
this crate authors, the deployment that declared none carrying the default rule
explicitly rather than carrying nothing. Absence means a record older than the member
and is read as the rule being unrecoverable, never as the default. **Without
it the record says what the model did and not what the agent's state was built from**,
which are different because the tee both selects which events cross and projects which
payload paths ride with them, per `weaver-harness-state-contract` section 2.

**Which loop composed the run, and whether the state member stood, are two more
and were left out.** Both decide what a record is a record of and neither is an
election about what the model reports. The loop is the party that assembles
every prompt the model sees, and a deployment can run more than one: the worker
binary's own, or a file the declaration names and a Python-iterating worker
reads per crossing, per `weaver-harness-Spec` section 6. Two loops assemble
different prompts from one declaration, so two records that do not say which
loop composed them cannot be compared, and a reissue whose loop injects the
session's past is not byte-comparable with a source whose loop did not, which
is the finding of 2026-09-03 on issues #381 and #382. The state member's
standing is the same shape one leg over: admin elects it from where a binary
sits on disk, per issue #381, and no record said whether it stood, so a record
taken with the member standing and one taken without are not comparable
either. **From this act the `load` event names both**, the loop by the binary
that ran it and the file and digest where the loop is a file, and the member
not by that election but by whether its end arrived on the enter, which is the
harness's own knowledge whatever admin decided from.

**The store the member stands on is a third such fact, as of 2026-09-04.** Per
`weaver-state-PRD` section 4 the engine is the deployment's
election and, under the service engine, the database and the role are the object gate's
own members, changing only across the load boundary. What a record was built into
decides what can be asked of it later, so the `load` event names the engine and, where
the service engine stands, its database and role, resolved as the enter carried them.
**The leave reads the boundary back**: the harness reads the role's grant surface from
the store's catalog at the enter through the member and again at the leave, and the
`unload` event carries the reading as unchanged, varied, or unreadable, the envelope the
confirm drivers already carry for the engine libraries. A grant surface that varied
inside a session is a boundary move the record carries and never absorbs, and one
unreadable at the close is said to be so rather than reported unchanged. Absence means a
record older than the act and is read as those facts being unrecoverable, never as a
default. The record then says what the agent's prompts were built by and what its state
was built into, which the elections alone could not. **The `load` event names its
lineage and its stack as of 2026-09-04**, per issue #432. Where the session stands from
a record it names the parent's session and run and the turn the holdings stop at, so a
resume and a branch each say what they came from, and every load names the digests of
the organ binaries that ran it, keyed by name, so a record is sufficient for its own
conditions and the experiment deposits stop being where the stack is remembered. The
record is the cache's canonical form and nothing derived from it is kept: a session that
stands from a record recomputes its prefix from the record under whatever conditions the
load declares, and the load event is where those conditions and that lineage are read.

**A replay under the wrong projection does not fail.** It lands, the loop asks,
an answer comes back, and what the run produces is a plausible replay of a
session that never happened. That is the shape this charter's own section 3.1
argues against, and it is why the tee's election belongs at the bracket's
opening beside the other three rather than in the deployment's memory.

**The record carries the rule and not the outcome.** Which events a rule took
is derivable from it, and this charter has already weighed one member per event
against one event per run and taken the second. **The day that stops being
enough is named rather than left**: if the tee's back-pressure election lands on
shedding, per `weaver-harness-Spec` section 9, then elected stops meaning
arrived and only a per-event mark of what crossed is truthful. That act carries
this one with it.

The bracket is defined here beside the run field deliberately. A run label that
could be stamped without the events existing to verify it against would be an
assertion with nothing behind it, which is the shape of every defect in the previous
tree's attribute vocabulary.

**A run edge is authored, not inferred.** Its boundaries are known at the moment
each bracket event is written, because nothing else writes a boundary and the
envelope's run field is the only place one is recorded. No consumer reconstructs a
run by range analysis.

**A `turn.closed` payload states which kind of close it was.** A clean close carries
the turn's completion. A stopped close carries the stop reason in place of one,
authored by the harness when the abort of `weaver-admin-harness-contract` section 3's
stop exchange lands. Every open has a close and the close says which kind it was, so
a reader never infers an abort from an absence. This adds a field to one payload
shape and no kind to the set, and it edits this charter and the contract in the same
act, per the rule of this section. **A generation cut at the turn's token
limit closes clean**, stated with the `Length` finish of 2026-08-19: the
turn ran to its own end and the bracket closed whole, the cut being a fact
about the generation that `model.output.finish` carries and the answered
close may surface as `finish: "length"` per the world contract, while the
stopped close stays reserved for the aborts, the stop directive's, a
fault's, and as of 2026-08-22 a refusal's, so the kinds of ending never
share a spelling.

**A turn ended by a seam refusal gains its own stop reason**, per the
operator's ruling on the refusal class. The close had two reasons and a
refused turn matched neither, so it ended under a reason that could only
misreport what ended it. **The close and the refusal event are two records
of two facts rather than one fact twice**, which is the division `fault`
already runs on: the close says the bracket ended and which kind of ending
it was, the event says what happened, and neither is recoverable from the
other. A reader walking brackets needs the first and a reader diagnosing
needs the second.

**`elision` joins the kind set, twentieth, and the record grows where the
state shrinks.** Per `weaver-spu-PRD` section 13.13 as chartered
2026-08-22. The harness names a span of the SPU's resident sequence, the
SPU makes it absent, and this record gains one event carrying the span that
was elided beside the resident counts either side.

**The span is on the event because the counts cannot stand in for it.** A
flush is fully described by what it leaves, its outcome being a prefix. An
elision removes an interior, so two elisions reporting identical counts can
have removed different positions and left different sequences. **An edit
that does not say where it fell is not replayable**, and the replay below is
this crate's own promise rather than a convenience.

**Nothing is removed from a record, and this is the act that could have
looked like an exception.** The state is a distillation of the record and
is never stored back into it, per section 3.2, and an elision edits the
distillation. What reaches this crate is the fact that it happened and its
extent. **A reader of a trace can therefore see what the model no longer
held**, which a reader of the state cannot, and that asymmetry is the
elision's whole accountability.

**A consumer reconstructing what the model held replays the edits.** The
accumulation of section 3.2 runs from the identity prefix through the
recorded contributions, and it now meets a second kind of edit beside the
flush: a contribution recorded before an elision that covered it was in the
model's context and then was not. **An accumulation that ignores the edits
reconstructs a context no generation ever ran against**, which was true of
the flush already and is easier to trip over here, an elision leaving no
gap in the sequence it edits. **The replay consumes the recorded
coordinates**: the elision event names the span it removed, and a consumer
applies it at those positions rather than inferring an extent from a
difference of counts.

**`refusal` joins the kind set, twenty-first, and it is one kind for every
seam.** Per the operator's ruling of 2026-08-22. A typed refusal crossing
any seam of this program reaches the record here: which ask was refused,
which seam answered, and the seam's own case with the values it carries.

**The ask is named rather than reproduced, and a value rides it only where
no other event holds that value.** A refused flush's cut and a refused
elision's span reach no other kind, so they ride. An open's messages, an
append's delta, and a cancel's turn each reach the record under kinds of
their own before the exchange, so the refusal names the ask and stops.
**This charter's own refusal of derived quantities is the reason**, one fact
in two places being the defect rather than the redundancy being a safety.

**It is not the `fault` kind and the distinction is the contracts' own.** A
refusal is an answerable ask that was wrong, and the party that refused is
well. `fault` says an organ is not well. Clerking a refusal as a fault would
report a healthy session as unwell, which is the misreport the seam
contracts already refuse in their own words, and it would put the record's
consumers on the wrong remediation.

**And it is not a member of the kind that was refused.** One kind meaning
both an operation that happened and one that did not is an overloading a
reader disambiguates on every read, so `elision` says a span went and
`refusal` says a span was turned away.

**The payload is the organ's, carried opaque.** A refusal is produced by the
party that refused, so it splices, per the custody rule of 2026-08-11 that
already governs `fault`. What makes it typed rather than free-form is that
its shape is the floor's: `weaver-types` declares the record and each seam's
case, so a consumer dispatches on a declared type rather than parsing prose.
**This crate neither knows nor versions the seam vocabularies**, which is
what its own no-dependency rule requires and what a typed payload declared
here would have broken.

### 3.2 The payload shapes the token workflow settles

Six of the fifteen kinds the set held at the time carried no chartered
shape, and this subsection settles four of them on the token workflow's
acts of 2026-08-02: `fault` and the three `model.*` kinds. **The fifteen is
the count of that date and not of this document**, the set standing at
twenty since the elision joined it. The tool bracket's two are the remaining pair and
stay deferred with the tool workflow, per section 9. What follows is content
and its obligations, which is this document's level. How any of it is
represented is the Spec's.

**`fault` carries the floor's report and this crate defines no second shape.**
`fault-report` is `weaver-types`' definition as of the gate act, one shape for
every reporting seam because what an organ hands the harness is the same fact
whichever socket carried it. The case set behind it closed the same day across
all three organs, `weaver-spu-PRD` section 13.10, `weaver-gate-PRD` section
13.4, and `weaver-harness-PRD` section 5, so this payload is elected against a
closed enumeration rather than a guess. Which subsystem raised it is the
envelope's field and is not repeated in the payload. A fault the worker does
not survive is not this kind, per the contracts, because the party that would
report it is the one that died.

**`model.request` also carries the bound the generation ran under**, per
the ruling of 2026-08-21 on issue #258: the resolved per-turn token ceiling
and the stop condition in force, effective values whichever side set them,
on the same disposition rule the sampling values ride. **A `finish` of
`length` is otherwise unreproducible from the record**, because the bound
that produced it lives in the worker's composition root or in an operator's
file and nowhere a consumer can read, and a re-feed that does not know where
the cut fell observes a generation the run never had. The stop token set
travels for the same reason and closes the same kind of gap: a generation
ending on a stop token is explained by a set the record did not hold.

**`model.request` carries what the model was asked this turn, the turn's rendered
contribution, per the operator's ruling of 2026-08-12.** The turn's delta as the family
library rendered it, the identity of the template that produced it, and the sampling
values in effect for the turn, effective values whichever side set them, per the
disposition rule of `weaver-spu-PRD` section 13.8. The seam is append-only, so what the
model received grows each turn while only the delta crosses, and the full effective
context is what the record determines rather than what any one event stores: the
accumulation of the recorded contributions under their recorded template identities,
from the identity prefix the run's opening records, per section 5. Recording the full
effective prompt per turn would store a derived quantity the record already determines,
one fact in two places, and would grow the account by the square of the conversation,
the resend pathology the append-only seam exists to kill returning as record bloat. The
state is a distillation of the record and is never stored back into it. The rendered
form is here rather than nowhere because of the end-to-end requirement of 2026-08-02:
each model carries its own template, so the turn's contribution exists twice, as the
canonical messages this crate's message kinds hold and as the rendered piece the model
was given, and a record holding only the first forces a replay to re-render through a
template that may have changed since, which observes a forward pass that never happened.
Recording both, with the measurement's token identifiers carrying the exact
tokenization, is what makes apex section 8's rule that tokenization is reproducible from
what is recorded true rather than hopeful. Two conditions ride the ruling, named rather
than slid past. The accumulation reading holds while every operation that moves the
resident context is recorded. **The flush's condition fired 2026-08-19 and was
honored**: the context-management act exposed the flush at the seat and added its event
to the kind set through the front door in the same act, exactly as this sentence
required. The reading therefore survives a flush by the event itself: a reader or a
replay meeting `flush` resets its accumulation to the identity prefix, the payload's
resident counts confirming the reset, so no later turn can be read against pre-flush
context. And a replay's identity prefix is the record's own since the ruling of
2026-09-04: the store preloaded from the record answers the identity ask at the replayed
run's opening, the prefix having crossed the tee whole under every election, so the
record's sufficiency for replay no longer leans on the configuration, and the open
question this ruling once named closes.

**`model.output` carries the emission verbatim, before any parse.** The
model's own family-styled output, the reasoning blocks and channel markers
and tool-call markers exactly as emitted, and how the generation ended:
completed by the model's own stop, stopped from outside mid-stream, or cut
at the turn's token limit, the third case added 2026-08-19 because a cap
that reported itself complete put an ambiguity in the one record whose
reason for existing is that it never lies. **The record holds both layers
and never flattens one into the other:** this payload is the verbatim
reality and the message
kinds hold the canonical parse of it, with the family parsers as the recorded
bridge between them, which is the outbound half of the same end-to-end
requirement. A record that kept only the parse could not answer what the
model said, and one that kept only the verbatim would make every consumer a
parser of every family.

**It carries the session's two counts beside them: `resident`, the count as
this generation closed, and `capacity`, the ceiling the load resolved.** The
closing count includes the turn terminator, per `weaver-types-Spec` section 4.4
as of 2026-08-19, because it is taken after the terminator lands, one token, per
`weaver-spu-Spec` section 4 as of 2026-09-05, and
the measurement's token identifiers beside it are the turn's rather than the
session's, the input the turn's delta with the identity prefix outside it and
the output the drawn tokens with the terminator outside them, per that Spec's
section 6. A reader deriving where a generation's first draw sat subtracts the
drawn tokens and the terminator from the closing count, and a reader adding
the delta to the previous closing count is wrong on the first generation by the
prefix, which no delta carries and no measurement member counts. The
pair is the overflow refusal's exactly, the two numbers that refusal names
after the wall. **It is not the flush's pair and the difference is worth
holding**, because both sections speak of resident counts and a reader who
merged them would look for a capacity the flush does not carry: `flush`
records `resident_before` and `resident_after`, two counts of the same
quantity bracketing one cut, where this payload records one count and the
ceiling it is measured against. What the two have in common is the occasion
rather than the shape. A generation moves the resident context exactly as a
flush does, and the accumulation ruling above holds only while every
operation that moves it is recorded. The counts are what make the accumulation checkable
rather than merely reconstructible, and the difference is the whole reason to spend two
integers on it. Neither is recoverable from the events around them.
Reconstructing the resident count means re-tokenizing every recorded
contribution through the template that rendered it, which needs the artifact
in hand and answers only where the artifact still exists. The capacity is
worse and is why it is named here rather than left implied: it is the load's
resolved ceiling and no turn states it, so no accumulation over the other
events reaches it at all, and recording it in this payload is the only place
a reader learns it.

**This does not reopen the derived-quantity refusal above, and the
distinction is exact.** What that ruling refused was recording the full
effective prompt, a quantity the record determines and whose storage grows
by the square of the conversation. Two integers per generation grow by the
turn and are a reading rather than a restatement: the number the decoder
held, as it held it, which no accumulation over the record can confirm
without repeating the tokenization the measurement already performed. The
refusal was of bloat and duplication, and neither reaches a count.

**The forcing case is an analysis that must place a turn in the context.**
An agent asked in a late turn for a fact stated in an early one is answering
either from resident context or from custody, and which of the two it was is
the finding, not a detail. Without the counts, the record dates a turn
relative to a flush and no closer, so two probes at opposite ends of a full
context read identically, and any comparison whose treatment shortens the
context cannot separate the mechanism from the length. **A reading taken at
the seam and not written down cannot be recovered from the finished record**,
which is the property this crate exists to prevent and is why the pair lands
now rather than when an experiment wants it.

**`model.measurement` carries the instrument readings, and its field list is
section 3.1's row as this act corrects it.** The row loses the sampler
parameters, which move to the request above because an input to a decode is
part of what was asked, and gains the surprisals and the decode timings, which
were readings the row had never named. **The row separates the
readings that carry an election from those that do not, as of
2026-08-21**: the surprisals and the residual reductions are produced where
their elections stand, while the entropies and the perplexity carry no
election and are produced wherever a distribution was read. **Carrying no
election is not the same as always present**, a backend serving no
distribution measuring nothing and the generation still answering, per
`weaver-spu-Spec` section 6. What this
subsection adds is the producing obligations that make those readings
trustworthy, stated as content because a reading
produced wrongly is not the same fact recorded imperfectly. The per-token
signals are computed against the pre-sampler distribution, because a signal
taken after the sampler measures the sampler. They are positionally paired
with the token identifiers, so a consumer joins by position rather than by
inference. And an unproduced reading is absent rather than zero, because an
empty vector and a confident model are different facts and a zero that means
neither is the silently partial account this charter forbids everywhere else.
The residual reductions appear exactly when the residency was admitted with
readout elected, per `weaver-spu-PRD` section 13.7, and a tap that failed
while elected is a fault rather than an absence. **They arrive with the two
counts that describe them**, how many layers a forward folded and how many
forwards were folded, because figures alone leave a consumer deriving the
shape from the token count and an election that taps a subset would make that
derivation wrong without making it wrong visibly. A reading whose
decomposition depends on a fact outside the record is a reading this charter
cannot claim is joinable by position.

**These four shapes close the deferral this charter carried, and the tool
bracket's two are what remain.** Section 9's staged list keeps them, and
nothing here shapes them, because a payload shaped against an unchartered
protocol is the reserved slot apex section 9 forbids.

## 4. What producing the trace requires

### 4.1 Open

The sink is opened by `weaver-admin` under root, the role's principal, per
`weaver-admin-operator-contract` section 3, and the descriptor is passed to a worker
that has held the agent uid from its first instruction, because the init system
starts the unit under `User=` and there is no drop to order, per
`weaver-admin-harness-contract` section 2. The receiving uid confers nothing either
way: a descriptor crossing a Unix socket is a capability and the kernel rechecks no
permission at receipt. `weaver-admin` resolves which session is being loaded. The
harness never learns a path or a sink's nature.

**Every run begins empty.** The working structure starts with nothing, the run's
first authored event is its `load`, and the stream continues at whatever sink admin
connected. There is no resume path, per the cut of 2026-08-01, no projection of
prior history, and no reconstruction from anything, the enter cell of
`weaver-admin-PRD` section 10 holding what continuity may later become.

**This crate's write surface accepts descriptors, never paths.** That is the API
consequence of the custody model, and stating it here is what makes custody
expressible rather than merely intended. A crate that offers a path-taking write
function offers a way around the boundary, and the previous tree carried exactly
that: a trace-root resolver with zero production callers whose layout described a
path no artifact ever used, while the security invariant three other documents
cited was specified against it.

Every descriptor this crate writes through is close-on-exec, and where the sink is
a file it is append-only as well. The first keeps tool subprocesses from inheriting
a writable handle. The second makes append-only a property of the handle rather
than of the writer behaving well, and it applies where the sink has a seek to
forbid, a pipe or a socket having none.

**The two flags do not arrive by the same route, and only one of them travels.**
Append-only rides the open file description, so a descriptor passed over a Unix
socket carries it and the opener confers it once. Close-on-exec rides the
descriptor, so it does not cross, and the receiving call is the only place it can be
supplied. `weaver-admin` opens a file sink append-only and cannot confer
close-on-exec on a descriptor it hands over. The harness supplies it at the receive,
and `weaver-admin-harness-contract` section 5 holds that split.

**What is pinnable at compile time is the shape and not the flags.** A behavior on
the receive path is not a type property, so no pin reaches either flag. What a pin
does reach is one receive site, taking no flag argument, returning a handle the rest
of the crate cannot construct another way. The flags themselves take the
perturbation-verified test of apex section 11, and the test is confirmed by watching
it fail when the flag is removed.

### 4.2 Update

One emission per event, and one emission reaches two sinks. The order is fixed and
is not an implementation detail, because the failure it prevents is a working
structure holding an event the stream never received:

1. the harness submits an authored event,
2. this crate admits it or refuses it with a typed failure,
3. an admitted event is assigned its sequence and rendered to canonical bytes once,
4. that one rendering fans out, appended to the working structure and handed to
   the stream writer in the same act.

**Refusal sits at admission, ahead of the fan-out.** An event that fails validation
reaches neither sink and an event that passes reaches both, so there is no state in
which one materialization holds what the other refused. What the recorder may judge
is bounded by section 3, reaching the envelope binding and the octet well-formedness
and never the interior of a payload.

**The working structure lands first and is the acknowledgment.** It is in-process
memory and the sink is not, so the append completes and the turn proceeds while
the stream write is still in flight. Reads are served at memory latency and the
stream trails. This is a trade the architecture makes deliberately rather than a gap
in it.

**The stream trails by the depth of the write queue.** Process death forfeits
whatever the writer had not yet handed to the sink. On a fast, quiet sink that tail
is effectively one event. On a saturated, contended, or slow sink the queue is
deeper and so is the loss. **The bound is a property of the deployment, not of this
crate.** This crate offers no policy field to tune it and no flush cadence to elect,
because a periodic flush is an interruption on a path whose purpose is to never have
one. What may happen under sustained pressure is the marked election of
`weaver-admin-operator-contract` section 3, and nothing here widens it.

**What reaches the sink is whole.** The writer hands the sink complete events and
never a partial one, so a consumer's account truncates at an event boundary rather
than holding a corrupted middle.

The three states of the commit boundary describe where an event sits in that
fan-out. **Committed** has been handed to the sink and is what the operator's
account holds. **Pending** has been admitted and appended and is still in the
writer's queue, so it is the loss window, whose depth the deployment sets rather
than an election. **Failed** is a stream write that errored against a live process,
and it is surfaced.

**Silent shed is forbidden while the process lives.** A stream write that fails
under a running process is named and surfaced and never swallowed, because the
process is there to report it. A tail lost to process death is a different failure,
unreportable by the thing that died, and the account must not present the two as
one. A trace is measurement data, and a silently partial account does not produce a
gap a reader can see. It produces a coherent account of a session that did not
happen.

**Nothing on the turn path waits on the sink.** The harness reads the working
structure in RAM. The stream write runs off the hot path, and a slow or failing
sink never slows the interior read. The two are failure-isolated.

**Canonical form is one mechanism, used everywhere.** Integer fields that can exceed
the double-safe range serialize as decimal strings. Nanosecond values exceed it by
roughly two hundred times, so a reader parsing them as doubles gets a silently
different number with no error and no way back. The previous tree got this right in
its record and wrong in its span export, which is what happens when two artifacts
carry the same field under two rules.

**The reason is resolution, not overflow.** Read as overflow avoidance the rule
looks like defensive rounding, and the natural response is to store fewer digits.
That response destroys the measurement. The monotonic clock resolves to nanoseconds
with a microsecond floor because the harness runs orders of magnitude faster than
the decoder, which pays network-class millisecond latency, and local latency is the
reason the model was brought next to the services in the first place. A record that
cannot resolve finer than the fastest thing it measures makes that advantage
invisible. This is systems-architecture timing, not network timing, and the decimal
string exists to carry the digits rather than to dodge an exception.

### 4.3 Drain

A run ends at unload or at process death. At unload the writer's queue is drained,
so a left answer on the coordination seam means everything admitted reached the
sink, per `weaver-admin-harness-contract` section 4. At process death the queue's
tail is forfeited, per section 4.2, and nothing drains it.

That is the whole of maintenance since the cut of 2026-08-01. There is no
finalization, no checksum, and no manifest, because the program owns no artifact to
finalize: what shape the account takes at rest, whether one file or many, rotated or
not, indexed or not, is the operator's tooling's business on the operator's side of
the sink. `session.closed` is content, authored by the harness like every other
event, and the stream simply ends where the session did.

## 5. Instrumentation

**The trace carries no recording level, and the recorder filters nothing.** Per the
human's ruling of 2026-08-02: whatever the agent produces reaches the record, and
what an operator elects at load is what the program *does* rather than what it
writes down. There is no floor and no ceiling here, because those named a filter
between the working structure and the stream, and the stream is a copy of the
working structure rather than a selection from it.

**The ruling follows the one that placed durability with the operator.** The cut of
2026-08-01 ended this program's obligation at the tee and left retention to the
party that owns the storage. A recording level is that same decision taken one
layer earlier, the program choosing what an operator is permitted to keep before
the operator chooses what to keep. Removing it is the durable-record ruling read
consistently rather than a new position.

**Elections live at the source, and the residual readout is the reference case.**
It is elected per agent in the config file, read at load, fixed for the life of the
run, and what it changes is what the SPU computes. When it is on, the reductions
exist and are recorded. When it is off, there are none to record. That is the shape
every instrumentation election takes: it governs production, and production governs
the record.

**What this dissolves is an ambiguity rather than only a mechanism.** An earlier
version of this section had the harness author each run's level into the run's own
events, because elected brevity and silent loss otherwise looked identical to a
consumer. With no level to elect, anything missing from an account is loss, which
is a stronger property than a reader consulting a recorded level to interpret a
gap.

**Two consequences follow, both improvements.** Every run is replayable, because
the token identifiers apex section 8 rests replay on are in every run's
measurement payload rather than in an elected subset, with the sampling
parameters in the same turn's request payload since the trace act of
2026-08-02, per section 3.1. And every record
names the model that served it, along with its weights hash, which closes the cell
`weaver-spu-PRD` section 10 held open on the premise that a floor run named
neither.

**The cost is stated rather than prevented.** Measurement payloads are per-token,
so every run grows the working structure at the rate an elected run grew it before,
and RAM is the one thing an operator cannot reclaim downstream. The lever is the
production election, which is the honest place for a lever: not producing a
measurement is cheaper than producing and discarding it, and discarding it after
the fact is the operator's to do on the operator's own compute.

**The record sorts into three tiers, and what sorts a member is which
arrangement needs it rather than what it costs.** Per the operator's rulings
of 2026-08-21 on issue #258. The first tier is apex section 8's enumeration
for deterministic re-feed, read rather than designed here: input token
identifiers, output token identifiers, model identity and weights hash,
sampling parameters, and the prompt-block partition. The second is
navigation. The third is diagnostic, elected at the source per section 5's
own rule and produced only while its election stands. **No tier is a
recording level and none reaches the recorder,** which still filters
nothing: a tier names why a member is produced, and production governs the
record exactly as it did before this sort.

**Navigation is a tier because aiming an instrumented run is work the
ordinary record has to do.** The diagnostic cycle starts from a failure in a
run that carried no election, so the trace of that run is what says which
turn deserves reloading with the field on. Entropy across position is what
says it. The tier's members are recoverable by a re-feed rather than lost
without them, so what carrying them buys is the aiming step, and what
dropping them costs is a forward pass over the whole run before the search
can begin. Decode timings sit in the tier for a different reason: a splice
win is priced by paired timings across two arms, and a member that could be
absent from one arm prices nothing.

**The identity prefix is recorded at the run's opening, as the messages it
is.** Section 3.2 determines the full effective context by accumulating the
recorded contributions from the identity prefix, and before this act that
prefix lived in the configuration alone. A consumer handed the record and not
the configuration could therefore not perform the accumulation this charter
promises, which left the replayability claim above resting on a fact outside
the record. **The remedy is a contribution rather than a new shape.** The
prefix is a system message and section 3.1 already carries
`message.system`, so what closes the rule is writing the prefix as the kind
it is rather than inventing a member to hold the same text under another
name. **The kind is reachable today and the prefix does not reach it**: a
system message submitted inside a turn is licensed and authored, while the
prefix is seated through the declaration at open and passes no door at all,
which is why the gap reads as an empty kind in a record rather than as a
missing kind in the set.

**Three properties make this the cheap fix rather than a convenient one.**
This crate holds the prefix already, arriving in the enter's SPU instruction,
so the write costs one read of what is in hand and moves no seam. The
assembled structure re-reads the three turn-bearing message kinds and takes
the prefix from the declaration, so a recorded prefix enters no later prompt
and the record gains a fact without the loop gaining a message. And the event
belongs to no turn, on the precedent the classify pair set, because a prefix
seated at open precedes every turn there is.

**It is recorded as text rather than as token identifiers.** Apex section 8
asks for tokenization reproducible from what is recorded rather than for the
tokens themselves, and the model identity, the weights hash, and the template
identity ride in every measurement already. The identifiers are the SPU's and
reach the record by the partition span `weaver-spu-Spec` section 6 names as
owed, which this act does not deliver and does not need.

**A measurement's input member is the turn's delta and not the generation's
input.** The distinction is this charter's own, per section 3.2: the seam is
append-only, only the delta crosses, and the effective context is what the
record determines rather than what any one event stores. It is stated here
because the member reads as apex section 8's first enumerated input to a
consumer who has not read that rule, and a driver feeding the delta where
the input belongs observes a forward pass that never happened. The remedy is
not to store the input per generation, which is the derived quantity section
3.2 refuses and would grow the account by the square of the conversation.
The input at any position is the seated prefix followed by every recorded
delta and output in order, and this act closes that chain by recording its
first link. **The member's name is owed a correction** in the crate that
renders it, which is where the name is declared.

**The surprisal's election joins the load's declaration**, per section
3.1's rule that each is named individually and none bundled. The reading it
governs is the SPU's to produce, so what this crate holds is the flag and
what the SPU holds is the consequence, which is the same division the field
and the readout already run on.

**Emission is required by no arrangement, and the training target is not an
arrangement.** Per the operator's ruling of 2026-08-21: apex section 9 puts
training out of scope entirely, so no arrangement of section 8 rests on the
record serving as a training corpus, and `model.output`'s emission is not
held by the first tier on that ground. What holds it instead is the
end-to-end requirement of 2026-08-02 already stated in section 3.2, the
model's own rendering being a different fact from the harness's canonical
message even where the two agree byte for byte. **Where they disagree the
disagreement is the finding**, so neither is dropped in favor of the other
and a divergence is filed rather than resolved by preferring one.

**What this sort rules, and where it landed.** The surprisal vector belongs
to no arrangement and is an election rather than a default, per the operator
of 2026-08-21, with a per-generation perplexity kept in navigation so a cold
record stays rankable without carrying a reading per position. **The
election landed in the act of this date that added it**, `weaver-spu-PRD`
section 13.12, which is where it had to land: that election is a member of the
SPU instruction and reaches this crate across two contracts, so every party
merged together rather than this charter ruling ahead of them. **The route is
per election rather than general**: the three the model reports ride the
instruction, and the tee's is a sibling of it on the enter payload, per
`weaver-types-Spec` section 4.

**The load's elections are four and the tier is the reason they are named
one by one.** A record whose posture is recoverable from itself is what
section 3.1's refusal of a profile name buys, and two of the four can be
absent while their subject is present: the surprisal, in every record written
before its election existed, and the tee's, in every record written before
2026-08-25. A consumer therefore reads the absence of the flag
as a record predating the election rather than as an election declined,
which is a distinction the flag's presence makes and its absence cannot.

## 6. What this crate guarantees

- Canonical byte form, one rule, all artifacts.
- A strictly increasing, gapless sequence over admitted events, **scoped to the
  run**. Session-wide order is the pair of admin's run reference and the sequence,
  assembled by the consumer, because the program holds nothing across a residency
  since the cut of 2026-08-01. The reference orders runs as the operator's clock
  saw them rather than as a monotonic sequence, per `weaver-admin-PRD` section
  10, so a consumer that needs strict order has it inside a run and calendar
  order between runs. The word monotonic is reserved in this charter for
  the clock of section 4.2. The sequence is the order and the clock is the
  instrument, and reading either for the other's job is an error the contract names
  explicitly.
- An explicit committed boundary, interrogable while the process lives: what was
  handed to the sink, what the queue still holds, and what failed. It is not a
  promise that nothing is lost, because section 4.2 forfeits the writer's queue to
  process death and bounds that queue by the deployment rather than by this crate.
- Whole events to the sink, never a partial one, so a consumer's account truncates
  at an event boundary.
- One rendering, held and handed: the working structure and the stream carry the
  same canonical bytes, so no reconciliation between them exists to owe.
- Typed failure for every refusal. Nothing fails silently and nothing returns a
  partial result with a success status.
- **Additive evolution, so any vintage of the record reads under any later
  vocabulary.** Argued below, because the guarantee is the reason no version
  member is carried and a bullet cannot hold both.

**The record's compatibility is additive evolution, and that is why it carries
no version.** Apex section 3 makes a change to the durable event schema the
breaking change and names it the one version every consumer keys on, and apex
section 9 fixes how the schema moves in the meantime: versioned once, extending
additively, adding event kinds and payload shapes and not reshaping the ones
that stand. Those two are the whole compatibility model, and the consequence is
stated here rather than left to be discovered by whoever first reads an old
record. **While the schema only extends, a reader needs no version to key on**,
every vintage being the one schema met at a different moment. A reader that
finds a kind it does not know skips it, a reader that finds a payload member it
does not know ignores it, and neither changes what it makes of the records it
does know. What a reader may never do is let either decide a grouping, because
a grouping resting on what the reader failed to recognise is a wrong answer
rather than a missing one, and a wrong answer here is silent.

**Adding a member to an existing payload is extension and not reshaping**, and
the corpus has already done it: the measurement payload gained its layer and
forward counts in the act that closed the layer-count defect, beside the
figures already standing and displacing none. Reshaping is renaming a member,
changing its type, removing it, or changing what one already there means. The
line is drawn because adding a member feels smaller than adding a kind and is
the same operation seen from a reader's side.

**The guarantee runs both directions, and the second one is where the damage
lands.** The paragraph above covers an old reader meeting a new record. The
other case is a current reader meeting an old record, where the member it
expects is **absent**, and the two are not symmetric: ignoring something extra
costs a reader nothing, while a member it counted on and did not get is a hole
it has to do something about. So an added member is **optional at the read, and
the act that adds it says what its absence means**, on exactly the rule
`weaver-types-Spec` section 2 already runs for the declaration: absence is
never read as a default unless the document says a field may be absent and says
what the absence signifies. A reader may not reject a record for a member added
after it was written, that record being valid under the schema it was written
under and the schema being one.

**The absence must never be papered over by derivation**, which is the specific
error this project has already made once. The layer-count defect had a reader
recovering the layer count by dividing the norm array by the token count plus
one, an arithmetic that held only while every forward tapped every layer, and
the counts were added precisely so nothing had to infer it. A reader that meets
a record without those counts has one honest move, which is to say it does not
know, and one silent failure, which is to infer. **Deriving a missing member
from the members around it is the reshaping this section forbids, performed by
the reader instead of the writer**, and it turns an old record into a wrong
answer rather than a partial one.

**No version member is carried, and carrying one today would be the error apex
section 9 names.** A version whose only reader is unbuilt is a reserved slot,
and that section's own sentence has a data field reserving a slot as readily as
an interface does. So the mechanism for a break is stated rather than built:
**a breaking change to the durable event schema is an apex act, and the act
that breaks the schema is the act that adds the marker consumers key on.**
Until such an act every record is of the one schema, and a reader that skips
what it does not know reads any of them. This paragraph is the authority for
that claim and a divergence anywhere else is a defect against it, per G5.

What left this list on 2026-08-01, deliberately rather than by drift: exact
recreation, run integrity on read, mechanical validity on read, and the per-turn
hash. Each presupposed a program-owned record to read back, and the ruling recorded
at `weaver-admin-operator-contract` section 3 places the record with the operator.

## 7. The seam

The production seam is bound by `weaver-harness-trace-contract`: the two exchanges,
what each party supplies and guarantees, the ordering rule that admission precedes
the fan-out, the failure vocabulary, and the prohibitions on both sides. It is read
alongside this charter and neither is complete without the other.

The seam edge is declared once, by the crate that asks, which is the harness. This
charter points at the contract and does not restate the edge.

This section was a list of what the contract had to settle, written before the
contract existed. It is now a third partial copy of a document that says it better,
so it is a pointer instead.

## 8. What does not cross

**The stored span export.** Spans are derived on demand. There is no second
artifact.

**The reserved span kinds.** The previous tree retained sleep and reconciliation
kinds so its enum could keep parsing archived artifacts. There are no archived
artifacts here, so the reason does not transfer and the kinds do not cross. This is
consistent with the apex rule against carrying slots for designs not yet written,
and it is a different reason than that rule gives.

**The attribute-name vocabulary as a convention.** Typed payloads replace a module
of name constants that emitters were free to ignore.

**Post-hoc annotation by consumers.** The previous tree let its analysis crate write
spans into a finished trace after the run, which meant a reader could not assume
every span in an artifact came from the producing process. The harness is the sole
writer, and an analysis artifact is a separate file.

**Memory, in every form.** No consolidation events, no belief vocabulary, no
substrate. The event kinds that serve those exist when their emitters do.

**Redaction, at write time.** The recorder never filters, so it never scrubs. The
harness authors what it authors and the recorder never filters, so a recorder that
dropped or altered content because it judged the payload would have taken policy
the harness holds. There is no third
place on the write path where redaction could occur, so the record is raw by
construction rather than by an unmade decision. Scrubbing, if ever wanted, is a
formatter election on export and downstream of everything here. What an operator
does with a delivered artifact is theirs.

**Embeddings.** Nothing in the proto-stateful turn retrieves by similarity, so a vector
recorded here would have no consumer. A payload field whose only reader is unbuilt
is a reserved slot in data form, and the schema is where that rule is easiest to
break, because adding a field feels smaller than adding an interface. Embeddings
enter the record when something reads them.

## 9. Staged requirements

A staged requirement is recognized work with an entry condition that holds it out of
the current pass. It is written here rather than carried in conversation so that its
disposition is a decision made at ratification rather than a thing remembered. This
document is living, so an item leaves this section by being built rather than by
being rediscovered.

**This section is authoritative for staged work belonging to this crate.** A working
list of open items holds deferred work too, and the two placements need one of them
named. A staged requirement with a crate to belong to lives in that crate's charter,
because it travels with the crate and because a Spec is derived from the charter plus
its contracts and never from a working list. A working list holds what has no owner
yet, and an item moves into a charter the moment it acquires one. That list is never
ratified and shrinks toward empty, so work parked only there is work that evaporates.

**The in-RAM representation.** Whether the working structure holds canonical lines
parsed on read or typed events rendered to their line once, and how the loop's
sequential reads are served, are the Spec's, bounded by section 2.2: one canonical
structure, append-only, no second authoritative form and no mutation surface. The
relational engine this item used to stage, a hand-rolled typed query builder that
existed because `rusqlite` was ruled out on append-only grounds, dissolved under
the ruling of 2026-08-01: nothing chartered reads relationally, the loop's reads
are sequential, and a consumer wanting an index builds a derived view. Recall over
past runs, when the memory round takes it, is similarity through the SPU's encode
side over the NDJSON account rather than a relational query, per the proposed
reading the enter cell of `weaver-admin-PRD` section 10 carries. **Entry
condition:** the Spec pass.

**The weight of the previous durability implementation.** The previous tree's
`event_log.rs` is 2,773 lines carrying canonical byte encoding, payload hashing,
sequence-gap detection, a committed-versus-pending boundary, and commit-pressure
policy. The obligation it would be read against shrank under the cut of 2026-08-01,
so the examination is now whether anything beyond canonical encoding, the boundary,
and pressure surfacing survives at all. **Entry condition:** the Spec pass.

## 10. Open ruling

**The interoperability target.** Spans are derived for tools that speak spans, and
the previous tree's format borrowed OTLP field names without being OTLP, then
carried a claim that a trivial converter could bridge them which turned out to be
untrue and unverified. Whether the derived view targets OTLP specifically, and what
conformance record backs that claim, is not settled here.

## 11. The tee

**The distillation surface is this crate's mechanism, because what is tee'd
is this crate's own rendering.** The canonical form of section 2 is rendered
once per event, and the tee is a second reader of that one rendering: the
recorder hands the same canonical JSON to the outbound stream, to the working
structure, and now to a selection whose output feeds the state seam under
`weaver-harness-state-contract`. The harness applies the tee as the sole
writer, per the custody rule of apex section 5.2, and this crate holds the
mechanism the way it holds every other: without policy.

**The tee selects and never computes**, per the operator's ruling of 2026-08-18. The
selection is per event and key-based: the envelope crosses whole on every distilled
event and is not electable, session, run, turn, kind, and sequence, so no election can
produce an unattributable row, and the election ranges over payload keys alone, named as
paths into the canonical JSON. An elected kind with no payload keys is a meaningful
election, presence itself being state. **Two events cross whole under every election:
the seated identity prefix**, the turnless `message.system` at the run's opening, per
the operator's ruling of 2026-09-04 on issue #422, **and a restored message**, the
conversation a branch's run opened under, as of 2026-09-26 (#697). The prefix is the
session's first bounding and the store is its custodian from that ruling on, so an
election that could drop its payload could produce a session whose identity the store
never held and a replay that could not rebuild it. The exception is stated here because
the rule it excepts is stated here, and it stays selection: the prefix's payload crosses
as the record rendered it, and no election adds to or subtracts from it. Transformation
of what was selected is `weaver-state`'s work and decision about what it means is the
harness's loops', and a tee that computed would smuggle the first into this crate.
**This charter declares no assertion node.** It argues the claim and Document Format
section 4 gives the declaration to the Spec, which declares
`trace-tee-selects-never-computes` at `weaver-trace-Spec` section 11, where the rule it
rests on is stated.

**The election is fixed at load and arrives with the declaration.** What the
operator elected is thereby a load condition the record carries like any
other. The default election is the envelope of every kind and nothing more,
so a deployment that elects nothing still distills the session's shape and
pays for no payload it never asked to keep.

**A miss costs nothing.** An event the election does not match is dropped at
the tee and the trace remains complete regardless, because the tee reads the
stream and never thins it. No account of the record, canonical, outbound, or
working, changes because a tee stands beside it, and an act that trimmed the
trace to what the tee keeps would have inverted the derivative and the
account.
