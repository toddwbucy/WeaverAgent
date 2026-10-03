# weaver-state - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth.

**Date filed:** 2026-08-18
**Document ID:** `weaver-state-PRD`
**Parent:** `weaver-harness-PRD`
**Companion contract:** `weaver-harness-state-contract`, owed by the act that opens
the seam and named here so the seam cannot open without it.
**Editorial:** Per the Working Rules.
**Landing PR:** #740

---

## 1. What this crate is

`weaver-state` is the store primitive of state management. It stores what it
is handed, organizes what it stores, and serves what it is asked for, and it
does nothing else. The charter is the operator's ruling of 2026-08-18, and the
sentence is short enough to carry whole: the crate holds state, and the
management of state as it concerns context for the decoder belongs to the
harness and its loop.

**State management is the middle of three layers, and authority runs downward**, per
the operator's rulings of 2026-10-02 on issue #1 and `weaver-agent-PRD` section 2. The
trace is what happened. State management is what is supposed to happen, reconciled
with what did, built only from the tee and rebuildable from the trace. Memory is a
lossy compression of state, written only by sleep-cycle consolidation and out of scope.
Memory proposes and state enforces, and the trace outranks both. **This crate is one of
state management's two primitives, and a primitive is mechanics without motive**: a
contract and a proven connection to a database, which is this crate, beside a contract
and a proven connection to a classifier on the SPU,
`weaver-harness-spu-classify-contract`. Neither holds an opinion about what is stored,
how it is judged, or why. The operator's side is the schema, which is enforcement by
construction, what violates it having nowhere to land, and the loop, which is purpose.
**The loop is state management**, per apex section 5.5: one loop, written in Rust and
compiled into the worker, calling on this crate and on the classifier through the
harness, organs never linking.

**The store is an in-memory embedded SQLite in this member's own process.** One
database, never more, with no server, no network and no pool, reached by exactly one
peer, the harness, over one Unix socket. It keeps save points, files each stamped with
the trace position it covers and never overwritten, taken at every serving unload and
on demand, and never under a diagnostic binding. A load restores one, the latest by
default, and with none state is rebuilt from the trace offline, per section 4.

**Two functions and no third: ingest and serve.** The tee's distillate flows
in and is kept, and what is kept answers asks. The two were not symmetric
at chartering, and the asymmetry was load-bearing for the build order. The
ingest had its producer standing and built whole against real traffic on a
real agent, landing 2026-08-18. The serve half waited for its first asker,
per the reserved-slot rule: a query surface elected against a guess about
its caller is an interface-shaped empty joint, and the fault report's own
history is the precedent, chartered and unshaped rather than absent until
its first real traffic gave it a shape. The asker arrived 2026-08-19 as the
context-injection loop, and the surface was elected against its real ask
rather than ahead of it, which is the rule honored rather than raced.

This is the statefulness leg returning by the door apex section 9 built for it: a
schema extension plus a new socket and contract, never a retrofit. This document is
that return's first paper, and nothing in the base set moved to make room for it,
which is what the door was for. The memory leg stays out and returns by the same
door, reading from this crate's save points when it comes.

```graph
node: weaver-state
kind: crate

edge: parent
from: weaver-state
to: weaver-harness
```

**It links `weaver-types` as floor, and the link stands ahead of its consumer.**
That crate is floor per `weaver-agent-PRD` section 5.1, this crate asks it
nothing, so the record is a `floor-link` and never a `seam`. **No source file here
consumes it today, and it stands anyway on the operator's ruling of 2026-09-14:**
the dependency stays because this crate is not finished, and a link held for work
not yet done is an election rather than a leftover. The ruling is the whole of the
reason and this charter adds none, because a declared dependency with no consumer
and no written reason is what an audit re-reports on every pass, and because a
reason invented to fill the gap is worse than the gap. **Which member of that crate
this one will consume is section 5's cell**, opened rather than answered: the one
candidate this charter could have named is refused by the seam's own contract, whose
vocabulary clause reads "**From `weaver-types`.** Nothing."

```graph
edge: floor-link
from: weaver-state
to: weaver-types
```

**It links `weaver-traits` as floor too, and that link has its consumer.** The
typed landing of `weaver-state-Spec` section 3 reads a message kind's `role` and
`content` through the message model that crate defines, the model the harness
renders them from, so custody holds a message as the floor spells it rather than
under a second definition of its own. `weaver-agent-PRD` section 5.1 rules the
crate floor and Document Format section 4 admits a floor link to a floor crate, and
this crate asks it nothing, so the record is a `floor-link`. The seam's contract
draws the same name in its vocabulary clause, per the operator's ruling of
2026-09-29 that the trace's payload is typed where it is read.

```graph
edge: floor-link
from: weaver-state
to: weaver-traits
```

On the operator's ruling of 2026-09-22, `weaver-trace` is a dev-dependency for
this crate's tests alone, outside H2's edge set per Working Process section 6,
with no production imports, closing section 5's dependency cell without a
state-trace contract.

## 2. What it is not

**It does not manage.** The loop decides what enters the decoder's context, what
leaves it, when a flush is worth its cost, and what any stored fact is worth to the
turn at hand. Every one of those is policy, every one is the loop's, and this crate
answers asks without holding an opinion about why it was asked. **The judging and the
ranking are the loop's, through the SPU**: where a held fact is classified or ordered
by worth, the classifier of `weaver-harness-spu-classify-contract` or the decoder makes
the judgment at the loop's call, the harness records it on the trace, and it reaches
this crate through the tee like any other event, per section 4. The trace charter drew
this line for recording and it holds here for keeping: custody without policy is the
whole of the charter, and a member that judged its own contents would be a second
reasoning loop wearing a filing cabinet's name.

**It does not initiate, and it holds only its schema's shape.** Nothing in this crate
fires on a condition, watches a threshold, or acts unasked, a save point included: the
harness asks for one, per section 4. A loop that consults state is the compiled loop in
the harness's seat, per the tool boundary ruling's placement of control loops, and
this crate is a place that loop reaches rather than a place one lives. What it holds is
what the loop's schema admits and nothing else, so the shape of the holdings is the
schema's and never this crate's own.

**It is not the trace and does not compete with it.** The trace is the primary
artifact and the one authoritative record, per apex section 1, and what this
crate holds is a distillation of the record, never stored back into it, per the
ruling carried at `weaver-trace-PRD` section 3.2. Where the two disagree the
trace is right by construction, because the trace is the account and state is a
working derivative of the account, and every holding is rebuildable from the trace
without re-asking a model, the model-made judgments among them standing in the record
as the events that made them. The distillation surface is the tee over
the canonical event stream, ruled by the operator 2026-08-12: the mechanism is
`weaver-trace`'s, because what is being tee'd is the trace's own rendering,
and the harness applies it as the one party that writes. Its output is this
crate's ingress: state receives what the tee elects, holds it organized, and
answers with it.

**The labor divides three ways and each part holds one.** The tee selects and
never computes. This crate transforms as part of organizing, per the
operator's ruling of 2026-08-18: a derived shape, an aggregate, an index are
custody's work on what was selected, and they carry no judgment about what
the turn should do with them. The harness's loops decide. A tee that
computed would smuggle state's work into the trace's crate, and a state that
decided would smuggle the loops' work into custody, and the three-way split
is what keeps each part answerable to its own charter.

**It is not the agent's to reach.** The harness reads state for its own
assembly and its loop's decisions, and the model receives only what the loop
serves it as composed context, the same wall `weaver-harness-PRD` section 5
holds for the trace. There is no model-facing read path and no tool that opens
one. **The agent never reaches its own raw trace or its save points**, per the
operator's ruling of 2026-10-02: a save point is a file in the operator's custody that
this member writes without a path the agent's uid holds, per section 4, and nothing
the agent's uid holds names it or opens it.

## 3. The seam

The seam is the apex's own prescription read literally: a schema extension plus
a new socket and contract. State crosses a process line from the harness under
`weaver-harness-state-contract`, initiator first in the name because the
harness asks and state answers, on a Unix socket that authenticates its peer
per the first invariant. Two kinds of traffic and no third: the tee's
distillate flowing in as the harness applies its filter, and served answers
flowing back when the harness or a control loop in its seat asks. Both ride the
one seam, because both are the harness talking to its member and a second
channel would be a topology fact no need has produced.

**The channel is the seam's own and reuses nothing.** Apex section 9 says a new
socket and this charter reads it literally: the coordination seam keeps its
one kind of traffic, and state's seam is a second, distinct channel. **It has
no name, per the operator's ruling of 2026-08-26.** Admin creates a socketpair
at the member's spawn, this member inherits one end, the harness receives the
other at the enter, and the channel authenticates its peer by descriptor
possession, per the first invariant's rule for a channel with no name. A name
would have to stand where a dialer can reach, and the one directory the worker
could reach is the one place a name can be replaced before it is dialed, so
the channel that needs no name carries none. How the ends travel is the
contract's mechanics, deliberately absent here.

**A second door stands on this member as of 2026-08-24, under a diagnostic binding,
and for the operator's offline save-point builder**, on the operator's ruling of
2026-10-02 on #58, which retires the serving load that elected a record restore (issue
#432). `weaver-analysis-state-contract` names it, initiator first: the driver, an
operator principal, preloads the holdings from a finished record it parsed outside the
agent, whole under a diagnostic binding and whole or cut for the builder, which writes
the result as a save point, and this member receives on a second socket what a live tee
would have fed on the first, the same distillate shapes drawn from the first door's
contract rather than redefined. The door's judgment is a credential's, this member's one
such judgment since the first door authenticates by possession: it admits the operator
principal and refuses every other peer, the agent's among them. Its name stands in this
member's own territory, per the operator's ruling of 2026-08-26, where the agent's
identity holds nothing. The door itself never stands under a serving load, so the
serving membrane of `weaver-agent-PRD` section 0 is untouched for every agent that
serves, and the builder opens it to the one operator principal the diagnostic binding
already admits and to nobody else. No ask crosses it, ever: the driver is a sender and
never an asker, so section 5's who-else-may-ask cell keeps its answer, the serve
direction having exactly the two ends it had. The door's opener retires the declared
session's prior holdings in the same transaction, per the contract's section 2, so a
preload lands against empty whatever stood, and a retry after a dead driver is a
replacement rather than a double - the recovery invariant stated before the loss clause
leans on it. With that in place the loss clause below covers the new door without
amendment, the preload being rebuildable from the record more directly than any holding
the tee fed. **The door is also where the trace reaches a rebuild**, the member never
reading the record itself: the builder lands the whole record's distillates through
this door, per section 4, honouring every reset the record names, and writes a save
point a load restores. No tail
replays on top of a save point by default; a tail landing on a stamped position is a
door the contract does not yet say, and section 5 names that contract as owed.

The nesting under `weaver-harness` carries domain membership and nothing else,
per apex section 5.4: nesting is never process topology, and this member
crossing a process line while the trace links in-process is two right answers
to two different questions. The trace must be unreachable by the agent and is
written by exactly one party, so it lives inside that party. State serves reads
back toward its writer's loops and holds volume the harness's own process
should not carry, so it stands beside the harness rather than inside it.

**Across sessions, under custody.** State management accumulates across runs and
across sessions, per the operator's ruling of 2026-10-02 on issue #1, which revises
both the session boundary this paragraph drew on the ruling of 2026-08-18 and apex
section 2's "none across sessions" of 2026-08-01. The trace already persists across
load and unload cycles, and the holdings persist beside it by save point: an unload
retires a run and its member process and takes a save point, which stands in the
operator's custody beside every earlier one, and the next load, of the same session or
of a later one, restores the latest or the one its declaration's `restore` names.
**Custody is what makes the crossing lawful**: a save point sits where the agent's uid
cannot reach, it is written without a path the agent holds, and it is rebuildable from
a record the agent cannot reach either. Which sessions an answer reaches is the
asking loop's to say, the store holding one database for the agent and no boundary of
its own between its sessions, and the asks standing today keep the session bound their
definitions carry, per `weaver-harness-state-contract` section 2. The memory leg, which
will read from these save points, is still out, and nothing here lays in for it: no
export
surface a future act would wish existed, per the no-reserved-slots rule.

**Losing the member loses the derivative and never the account.** State can
die while the session lives, and the session goes on: the trace is the
authoritative record, this crate holds a working derivative of it, and a
harness whose state member is gone serves turns the way it did before the leg
existed. **The holdings live in memory, so an unclean stop costs what landed since the
latest known-good save point**, and nothing more: the next load resets to that save
point and records the reset on the trace, which save point and which position, and no
tail replays by default, on the operator's ruling of 2026-10-02. Everything the
holdings distilled is still in the record, and a full rebuild from it honours the
recorded reset, so the reloaded store equals the replay. Whether a member restarted
inside a run is refilled or stands empty is the loop's policy like every other
judgment. What this charter forbids is the inversion: no design in this leg may make
the session's continuation depend on the derivative surviving.

## 4. Its material

The input is the inspected artifact's lineage: canonical event JSON as the trace renders
it, selected per event by the tee's key-based filter, fixed at load. **The tee's rulings
of 2026-08-18**: the envelope always rides and is not electable, session, run, turn,
kind, and sequence crossing on every distilled event so no election can produce an
unattributable row, and the election ranges over payload keys alone, with one exception
ruled 2026-09-04 and stated below. An elected kind with no payload keys is a meaningful
election, because presence itself is state. **The default election is the envelope of
every kind and nothing more**, the operator electing payload keys on top of it, so a
deployment that elects nothing still holds the session's shape - what happened, in what
order, in which turn - and pays for no payload it never asked to keep. An event the
election does not match costs nothing and is dropped at the tee, the trace remaining
complete regardless, because the tee reads the stream and never thins it. The run
identifies itself in that stream since the ruling of 2026-08-14, admin's run reference
having replaced the ordinal, so what state receives is attributable to the run that
produced it without this crate minting any identity of its own.

**The identity is held here and seated from the prompt file at every load**, on the
operator's ruling of 2026-10-02 on this act's fifth question, which revises the ruling
of 2026-09-04 on issue #422. The system prompt is the first bounding of the possibility
space the decoder samples from, which is to say it is context, and context is this
crate's material: each load seats the declaration's identity, the text of the prompt
file it names, and the tee lands it here as the turnless `message.system` events at the
run's opening. **The prompt file is authoritative at every load**: editing
`system-prompt.md` takes effect at the next load, its digest recorded on the load event,
and what this crate holds under that kind is the history of the identities the loads
seated, never an authority over the next one. **The identity's kind cannot be elected
out**: the seated prefix crosses the tee whole under every election, per
`weaver-trace-PRD` section 11, the one exception to the key-based rule, so no election
produces a load whose identity this crate never held. Within a residency the prefix is
permanent, the decode seam holding it from open to release, so a revised prompt takes
effect at the next load, or at an SPU session reopened with the new prefix as a live
restore that needs one does, per section 4. An identity rewritten by consolidation is
the memory leg's and stays out, per apex section 9.

**A session branches from a record offline, through a save point**, on the operator's
ruling of 2026-10-02 on #58, which retires the record restore of 2026-09-04 (issue
#432). The declaration's `restore` names a save point and nothing else. Branching from a
record, a prefix another session produced, is an operator-side step outside the agent:
the builder rebuilds the holdings to a cut through the preload door, whole or through a
named turn, and writes them as a save point, which a load then restores like any other.
The save point names the record and the cut it was built from, so the record that stood
the session is still named on the new record's load, per `weaver-trace-PRD` section 3.1.
Custody is indifferent to how a save point was made: it holds what it was handed and
answers what it is asked, and the ground stays the identity's and the vectors': the
record is the source, and what it conditions is computed from it under the conditions
the load declares.

One member instance serves one run of one agent: it stands with each run, ingests a
stream whose events already carry their session, run, and turn identity, and its
process retires with each unload while its holdings stand in that unload's save point
for the next run, so nothing this crate holds needs an identity it minted itself.

What organizing means at this charter's level: the holdings are queryable by
the facts the record already carries, the run, the turn, the kind, and the
keys the tee elected, and by whatever the loop's schema adds.

**The schema is the loop's, and this crate holds a slot for it.** The loop provides
its schema, the tables its state takes and how each distilled kind lands in them, and
the store stands that schema at open and holds nothing it does not admit. That is
enforcement by construction, per the operator's ruling of 2026-10-02: a distillate the
schema has no place for has nowhere to land, and the trace still holds it whole. **The
neutral substrate stays custody's**: the event row and its elected pairs, the envelope
and the key paths the tee carried, are a shape with no opinion in it. **The typed
tables and the selection rules this crate holds today are loop opinions and leave
custody** into the loop and its schema: the message, part, measurement and series
tables of `weaver-state-Spec` section 3, the recall's choice of message kinds, and the
identity's newest-run rule. Each was elected here against a real ask, and each is a
judgment about what a turn needs rather than a mechanic of keeping, so each moves with
the act that writes the loop's schema, and until it lands the store serves them as it
does.

**Save points are custody's mechanics, and their timing is the harness's ask**, on the
operator's rulings of 2026-10-02 on this act's second and third questions. A save point
is a file of the whole store, stamped with the trace position it covers, the run and the
sequence of the last distillate in it, and **never overwritten**: each is its own file,
and the operator keeps, deletes or archives them. One is taken at every serving unload,
and one on demand, a diagnostic binding taking none because its holdings are a replay
and not the agent's state, when the harness asks; who triggers an on-demand save point,
the loop on a setting, a later admin verb or WeaverWeb through admin-con under a role,
is later work, and the primitive supports the ask. **A load restores a chosen save
point, the latest by default**, chosen through the declaration's existing `restore`
member pointed at a save point, data in `agent.toml`, so admin's verbs and answers stay
as they are. **After an unclean stop, the load resets to the latest known-good save
point**, and the reset is recorded on the trace, so a full rebuild honours it; there is
no tail replay by default. **With no save point, state is rebuilt from the trace**, by
the offline builder landing every distillate the record holds through the preload door
of section 3 and writing a save point the next load restores, so deleting save points
never loses state while the record stands, and a fresh state is a new agent, per the
seventh question. **Taking, restoring and resetting each author a trace event**, which
save point by digest and which position, per the fourth question, so the record says
where state came from. A restore and a full rebuild to the same position arrive at the
same holdings by construction, the save point being a cache of what the replay produces,
and `weaver-state-Spec` section 5 names the instrument that holds them equal.

**A live restore swaps state without unloading**, on the operator's ruling of
2026-10-02.
State lives in this member's process, apart from the models, so a save point restores
while the agent stays loaded: the member's in-memory store is replaced from it, the SPU
stays resident with its decoder and classifier untouched, and the hot KV cache is
flushed with `keep = 0` through `weaver-harness-spu-decode-contract`'s existing cut,
which clears everything but the identity prefix, permanent from open to release; the
next turn's context comes from the restored state. A larger cut that keeps a prefix the
save point and the live context share is the loop's choice through the same two calls,
later. A save point that needs a different system prompt cannot be served by a flush:
it needs the SPU session closed and reopened with the new prefix, one prefix recompute
with the weights resident, never a model reload. The trace records the restore, by
digest, and the flush. **The harness triggers a live restore**, on the operator's ruling
of 2026-10-02 on #58, through the `restore` ask of `weaver-harness-state-contract`
section 2, and flushes on its answer; the channel that carries the operator's demand to
the loop is later work.

**An edited save point is an input, not derived state.** A save point the operator edits
offline holds what the trace never recorded, so loading one records its digest, marked
operator-supplied, and the file is kept as an input, as a prompt file is. Rebuildable
from the trace then holds from the latest recorded save-point load onward.

**The store is a port, and one engine stands behind it.** This crate's custody holds no
opinion about a query language, and the seam to the harness names asks and never a
query, which `weaver-harness-state-contract` already holds: what crosses the seam is
the same whatever answers it, and the only place this crate spells a database is the
port that integrates one. **The engine is sqlite, embedded and in memory**, on the
operator's ruling of 2026-10-02 on issue #1, and its ground is the election of
2026-08-18 carried one step further: the regime's backend comparison held its
registered prediction that an in-process query is a function call while a service on
loopback pays a round trip per ask, and a store held in the member's own memory pays
neither a round trip nor a disk write per landing. **One database, never more**, with
no server, no network and no pool, and one peer, so the store is one agent's as every
organ is, per `weaver-agent-PRD` section 6.

**The service engine is retired**, reversing the ruling of 2026-09-04 and its
confirmation of 2026-09-30 on #38, on the operator's ruling of 2026-10-02. A shared
database server is a substrate many agents could reach across, which breaks
individuation, and local-first means one embedded database per agent. What the
service engine was elected to buy, a queryable structure that similarity, ranking and
a classifier in front of the selection make cheap, the embedded engine holds too, and
the ranking and the classifying are the loop's through the SPU rather than the store's.
**The port stays as the seam**, so a further engine would arrive as an integration in
its own act, with its own custody clause, and section 5 names that cell without laying
one in.

**Custody is the member's room, the operator's directory, and the member's process.**
The wall of section 2 is one requirement: the agent's uid reaches no store, because a
store the model's uid could read would hand the model its own state through an ordinary
tool call. **The live store is in the member's memory**, in a process running under the
member's own account, which the agent's uid cannot read or signal. **The member writes
each save point into its own room**, the territory's `state/`, which it owns `0700` and
which the agent's uid cannot enter, on the operator's ruling of 2026-10-02 on #58: a
new file per save point, never one rewritten, so the write needs no handle from admin
and no admin resident. **Admin, as root, publishes each finished save point into the
operator's declaration directory**, `~/.weaveragent/<agent>/` by default, beside the
declaration and the system prompt, at the next load or unload, per `weaver-admin-Spec`
section 6, so a save point taken on demand lands with the operator at the next verb, and
one that survived an unclean stop is published at the next load. That directory stays
`0700` to the operator, so the member cannot open it by path and does not try. **A load
restores through a descriptor**: admin, as root, opens the chosen save point in the
operator's directory, the latest by default or the one `restore` names, an edited one
included, and hands the member the open descriptor at spawn, exactly as it opens the
trace sink and hands it down, and for the same reasons. **A live restore reads the
member's own room**, the save point the member itself wrote and still holds, with no
admin; a save point that is only in the operator's directory restores at a load. The
member sees its own room and a descriptor and never the operator's path, and the agent
never reaches either.

**The store is reached through the seam, never as a file and never as a
connection.** A save point opened from two processes would be a seam crossing a process
line without a socket, which the first invariant forbids however convenient the driver
makes it: the member is the one holder of the engine and of its save points, and the
harness asks the member. The harness's speed rides the seam's standing channel, which
this workshop has measured well below any control loop's cadence, and a caller that
someday needs faster than the seam is a ruling for that day rather than a shared file
today.

## 5. Open cells

- **What the `weaver-trace` dependency is, or whether it stays. Opened 2026-09-14,
  closed 2026-09-22.** The operator's ruling places it in dev-dependencies for
  tests alone per Working Process section 6, closing this cell with the earlier
  alternatives below retained as history.
  The manifest carries the crate and the graph carries no edge for it, which is the
  H2 breach the audit of 2026-09-13 found. **The `floor-link` branch is closed**:
  `weaver-agent-PRD` section 5.1 rules the floor exactly `weaver-traits` and
  `weaver-types` and rules that crate out of it by name, and Document Format section
  4 admits a floor link only to a floor crate. What is left is a `seam` tagged `link`
  under a contract that does not exist, or the edge coming off.
  **H2's "No dependency on a sibling" stands unanswered against this pair, and this
  charter does not answer it.** That crate is this crate's sibling, both declaring a
  `parent` edge to `weaver-harness`. Two readings were put and neither settles it.
  One is that the bar is the directory, which Document Format section 3 refuses:
  nesting "is not a visibility rule, a Cargo boundary, or a claim about who may
  depend on what", and the records settle it instead. **That disposes of the
  directory and supplies no licence**, H2's sentence not being about the directory.
  The other is that declaring an edge cures it, on `weaver-types-PRD` section 1,
  where two floor crates under one root carry a dependency that "is a floor link and
  not a lateral edge". **That instance does not reach this pair**: H2's own first
  sentence admits a floor link by name, so what licenses it there is the category,
  and this pair is expressly not a floor link. So the sentence applies on its face
  and the cell is open on it.
  **What is measured rather than argued** is that no `link` seam in this corpus runs
  between two members of one domain. Every `link` seam it carries runs from a domain
  root to its member. A seam here would be the first of its shape, which is a reason
  for the operator to rule rather than a rule this charter can read off.
  **What the tree says meanwhile.** Nothing but this crate's own tests reaches that
  crate, so the cheap reading is that the dependency is a test fixture and moves to
  where fixtures live, which needs a prior ruling on whether H2 reaches a
  dev-dependency at all - `WeaverTools-Working-Process` section 6 does not say, and
  `weaver-admin` declares one floor crate in both sections under a single record.
  The dear reading is that a finished ingest calls into it, which makes the seam real
  and owes it a contract, and owes `weaver-agent-PRD` section 5.1 the retirement of
  "the harness is its only caller".
- **Which member of `weaver-types` this crate consumes. Opened 2026-09-14.** The
  floor link is declared at section 1 and the operator has ruled it stays. What the
  corpus does not carry is what it is for. The store election's block,
  `state-election`, was the candidate and does not serve: it is shaped at
  `weaver-types-Spec` section 2 and rides the enter directive rather than this
  crate's seam, and the seam's own contract answers "**From `weaver-types`.**
  Nothing" in its vocabulary clause. So the link is sound by the ruling and
  unexplained by the corpus, and the cell closes when the consumer is written or
  when a clause names what it draws. The envelope's identifiers do not close it:
  the floor's session, run and turn types are bare strings with no invariant to
  check, the envelope crosses as the record spells it, and landing it through them
  would convert without testing anything. Stated as a cell rather than guessed at,
  because the guess is what an audit would have to un-write.
- **The schema extension.** Apex section 9's door names one and this charter
  does not write it: the shape of the distillate the tee emits is settled with
  `weaver-harness-state-contract`, because the schema is the seam's vocabulary
  and a contract is a complete interface or it is not a valid contract. The
  store's internal shape is ruled at section 4 and is not this cell: what the
  seam carries and what the store holds are two facts, and only the first is
  the contract's. **The loop's schema is a third fact, opened 2026-10-02**: it
  crosses this seam in the opener, per the contract's section 2, and what it
  holds is the loop act's to write, the slot being all this charter carries.
- **The tee's charter section. Closed 2026-08-19.** The distillation
  surface's mechanism is `weaver-trace`'s, per the ruling of 2026-08-18,
  its paper standing at `weaver-trace-PRD` section 11 since the seam act,
  and the deployment's shape settled with the declaration act: the
  election's block is `state-election` in the agent's file, shaped at
  `weaver-types-Spec` section 2, riding the enter directive per
  `weaver-admin-harness-contract`, absent meaning section 4's ruled
  default.
- **A further engine.** The port admits an engine by an integration of its
  own, and the previous program's graph store is the one most likely to ask,
  having been tied into that program's base code. It arrives as a port
  implementation in its own act, with its own custody clause, and never in the
  base, and it answers the individuation ground section 4 retired the service
  engine on: one database per agent, reached by one peer, with nothing another
  agent can reach across.
- **The preload door's contract, owed.** Section 3 makes the door the path by
  which the trace reaches a rebuild, landing whole and honouring every reset
  the record names. `weaver-analysis-state-contract` says the door's opener
  retires the declared session's holdings and says nothing of a reset event a
  rebuild must honour, nor of a tail opener keyed to a stamped (run, sequence)
  position, so that contract is owed both, with every party to it, before the
  save-point code act lands.
- **Who else may ask.** Today the harness is the one peer, and every ask
  arrives through it. Whether a later operator surface reads state directly or
  through an admin verb is a cell for the day such a reader exists, refused
  until then by the seam having exactly two ends.
- **The serve surface's shape. Closed 2026-08-19.** Chartered here and held
  deliberately unshaped until its first asker, which is the fault
  emission's own pattern on the decode seam. The context-injection loop
  arrived and the surface was elected in its act against its real ask: the
  vocabulary in `weaver-harness-state-contract` section 2, the
  representation in `weaver-state-Spec` section 4.
- **What a control loop's ask looks like. Closed 2026-08-19.** The loop
  is the compiled loop in the harness's seat, so its asks ride the
  harness's end of the seam, and the calling shape landed where the loop
  surface is: the seat's state port, `weaver-harness-Spec` section 6.
