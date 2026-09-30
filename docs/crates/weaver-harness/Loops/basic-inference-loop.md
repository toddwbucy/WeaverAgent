# Basic Inference Loop

**Status:** MERGED v0.12, 2026-08-01. The workflow document for the basic inference
loop, filed under the harness's `Loops/` container per the Document Format's
container entry. It argues no edges of its own: the seams it walks are declared in
the crate charters per Document Format section 4, and a graph block here would
duplicate a record that already has a home.

**Document ID:** `basic-inference-loop`
**Parent:** `weaver-harness-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #317

## Rulings of 2026-07-31, landed by the batch of this date

**Struck 2026-08-01. The batch merged and every landing below is history rather than
work owed.** The block stays for provenance, per the strike convention, so a reader
of an earlier revision can tell a landed ruling from one never made.

This document is written against four rulings made in session on this date, and the
batch of this date lands their edits in the documents this register names. The
review first read the Format's copy as behind a merge, and the human ruled the truth
simpler: the four cited rulings were made and never landed, so the Format's v0.6
lands them and the `Loops/` entry in one act. The charters remain the decision
record and this block is the register that traces the landing.

1. **Stop is the fifth exchange.** `weaver-admin-harness-contract` section 3 gains
   a stop exchange, opened by admin, valid between enter and leave, answered with
   the turn's fate. Section 7's prohibition is re-scoped from reaching into a run
   to carrying work, aligning it with the rule section 3 already states. Section 4
   gains one ordering line. `weaver-admin-PRD` section 3 splits activity control
   across the seam, the harness executes the stop and admin conveys the operator's
   intent. `weaver-harness-PRD` section 2's operator interrupt gains its citation.
   The turn-close kind set gains the stop reason, reaching `weaver-trace-PRD` and
   `weaver-harness-trace-contract`. By the contract's own derivation rule the
   exchange also lands in its sections 5 and 6, supplies and failure.
2. **OVERTURNED, and not citable.** The ruling that introduced the live view, a
   descriptor at enter carrying `QueryEvent` frames to a handle admin exposes, is
   overturned by ruling A of the subtraction batch of this date, and its provenance
   is under separate review in the architecture seat. Its edits are withdrawn from
   every document that carried them. Nothing cites the overturned ruling as
   authority for anything. What replaces it is no view at all: the one output is
   the NDJSON structure exiting `weaver-admin`, a consumer who wants a front end
   builds one on that stream on the consumer's own compute, and WeaverTools takes
   no responsibility for any view of anything.
3. **The gate is a local Unix socket hook and binds no network socket.** The
   apex's section 3 claim that the gate binds the only listening network socket
   went to the apex correction list at review, deposited by substance rather than
   position: the gate binds no network socket. The gate charter drafts against the
   local hook.
4. **Loops file under their organ.** A `Loops/` directory under a domain root
   holds workflow documents, the container typing the kind directly, one entry in
   the Document Format's container table, landing in the v0.6 of this batch. Agent
   loops belong to the harness, so this document lives here.

Two rulings of the subtraction batch join this register. **Ruling A** retires the
live view, per the overturning above, reaching the coordination contract's enter
payload, supplies, and vocabulary, `weaver-harness-PRD` sections 2, 4, and 5,
`weaver-admin-PRD` sections 4.1, 6, and 8, the gate pair's fork enumerations, and
this document. **Ruling B** retires the integrity witness: emission is a tee, the
two copies are the same writes, downstream verification is the consumer's business
on the consumer's compute, and the turn hash, the payload hash, the recompute
exchange, and the append-only apparatus come out of the coordination and trace
contracts and the charters that carried them. Ruling B's cut of `weaver-trace-PRD`
and the leave-time material waits on the durable-record ruling: whether the trace
crate still persists internally with the tee adding the outbound copy, or the
consumer owns persistence and the durable record dissolves, which would also
dissolve the only session-resume path the corpus holds. That ruling landed
2026-08-01, the consumer owning persistence, recorded at
`weaver-admin-operator-contract` section 3 and carried across the corpus by the cut
batch of that date, the enter question left as the cell `weaver-admin-PRD` section
10 holds.

**Ruling C** takes its own entry, apart from the two above, because it overturns an
apex binding and they do not. Admin arbitrates no hardware and reasons about the
device at no point, the SPU is the one authority on the device, and apex section 6's
GPU-conflict rejection relocates from the load driver to SPU model admission, the
no-auto-evict guarantee moving with the rejection rather than leaving with it. The
ruling reaches `weaver-agents-PRD` section 6, `weaver-admin-PRD` sections 2 through 6
with the load renumbered to seven steps, `weaver-spu-PRD` sections 2, 3, and 4.1,
and `weaver-harness-spu-contract` sections 4 and 6. This document takes no content
edit under it, the basic loop never reaching the device.

One cell of v0.1 is deleted rather than carried: flush cadence for the durable
write. `weaver-trace-PRD` section 4.2 leaves no cadence to elect, a periodic flush
being an interruption on a path built to have none, and a cell that reopens a
merged ruling without naming the reopen reads as never having been ruled.

## 0. What this document is for

The corpus charters crates one at a time, and a crate charter is the wrong place
to hold a path that runs through five of them. This document holds the path. It
states the event grammar the trace records, the loop the harness runs, and the
role each boundary organ plays in one complete pass from agent load to agent
unload.

It is a workflow document. It IS the statement of how the basic loop moves and
what lands in the trace at every step. It IS NOT a charter, it binds no crate, and
it decides no seam. Where this document and a merged charter disagree, the charter
yields nothing and this document is corrected, because the charters are the
decision record and this is their composition read back as one motion. Version 0.1
of this document failed that rule by citing a session record as settled, and this
revision is the rule applied.

**Scope.** The basic loop serves a local client over the gate's Unix socket hook.
There is no listening network port anywhere in the loop, per the ruling above.
Streaming responses, memory reads, and every other interior elaboration are
out of scope here and arrive with later loops.

## 1. The grammar

Three levels, strictly nested. Session over run over turn.

A **session** is opened by the load event of its first run, there being no
`session.started`, per `weaver-trace-PRD` section 3.1. Admin resolves the session
identity and opens the stream's sink under root before any run
exists, per `weaver-admin-PRD` section 4.1, and the sink is a connection rather
than the session. A sink connected and never written was never a session at all. A
session spans runs in the stream's account, and what a later run holds of it is
the enter cell `weaver-admin-PRD` section 10 names. `session.closed` is in the
merged kind set with its shape proposed at `weaver-admin-PRD` section 4.4, its
ordering is the open cell in section 7, and nothing in the basic loop depends on
the answer.

A **run** is the agent's life. The load event opens it and the unload event closes
it. One run, bookended, with its turns inside. Runs are monotonic under admin's
ordinal, supplied in the enter directive. The ordinal is not a trace counter, and
where it survives across invocations is a face of the enter cell `weaver-admin-PRD`
section 10 holds.

A **turn** is one external exchange, bounded by the boundary crossing. It opens at the
harness's `turn.started` off the gate's inbound crossing and closes at the harness's
`turn.closed` at the final answer. The two crossings are the boundary the turn spans,
not its clock. The brackets are the harness's authorship, so a gate that dies between
the close and the outbound delivery leaves a closed turn and an undelivered response,
and the record states it in that order. Everything on the agent side between those two
crossings is interior to the turn. The decode is inside the turn. When later loops add
memory reads, embedding passes, or any other interior traffic, that traffic is inside
the turn too. Interior events add depth to a turn. They never add turns. A turn IS a
boundary-to-boundary exchange and IS NOT anything that happens without an external
crossing.

Every turn that opens also closes. A clean turn closes with its response. A
stopped turn closes with the stop reason marked in place of a response, the
payload edit registered above. The run never holds a dangling turn, which is what
makes a later read of the trace honest. Every open has a close and the close says
which kind it was.

The placement rule follows from the grammar. Every event lands at its position in
the structure, run-level events at the run they bookend, turn-level events as the
paired back and forth inside their turn. Recording an event IS placing it at its
position. There is no separate log beside the structure. The harness authors both
brackets of a turn, the open off the inbound crossing and the close at the final
answer, and the placement runs through the harness like every other placement.

## 2. The organs at the boundary

Two organs sit on the membrane and they are the same mechanism pointed at two
scopes. **Admin brackets the run. The gate brackets the turn.** The procedure that
opens and closes a run at the admin crossing is the procedure that opens and
closes a turn at the gate crossing, applied one level down. This symmetry is this
document's composition read, not a charter's ruling, and the charters it composes
are cited where each fact lands below.

Admin is the operator's window, ask in, state out. The gate is the agent's window
on the world, work in, response out. Each organ observes the crossings at
its own scope and the harness authors the bracketing events. One writer,
one grammar. There is no gate-side writer and no admin-side writer. Provenance
does not pick the organ.

The gate, in this loop, is a local Unix socket hook on the front end, the mouth
and the ears, an opaque pass-through with no translation and no opinions about
content in either direction. Whoever connects gets to converse with the agent and
gets nothing else. The gate charter merged on 2026-08-01, so this paragraph stands
on it rather than proposing ahead of it, and the client socket's own boundary is
governed by `weaver-gate-world-contract` as of the same date. The gate's lifecycle
position is stated in section 3.

Admin's interface is the lifecycle exchanges of the contract, and the program's one
output is the NDJSON stream exiting at admin, per `weaver-admin-operator-contract`
section 3, the ruling the register above awaited having landed on 2026-08-01.
Admin monitors nothing. It holds
no session with a watcher, keeps no watch of its own, and interprets no content,
which is custody without comprehension per `weaver-admin-PRD` section 3. If
something external watches the output and decides the agent must be stopped or
unloaded, that judgment lives outside, and the external thing comes back to admin
and opens an exchange. The monitoring is the outside's job. The verb is admin's.

## 3. The lifecycle

Admin's chartered verb set is load, unload, and validate, per `weaver-admin-PRD`
section 4. This loop walks load and unload, adds the stop exchange, and never
reaches validate, which starts no process and joins no loop. Three operator acts,
then, all crossing at admin, carried by the exchanges of
`weaver-admin-harness-contract` section 3, and all three are merged.

**This loop runs under a serving binding.** The kind that crosses in the enter
declares the whole interior, Gate included, per `weaver-agents-PRD` section 6
as amended 2026-08-24, and the walk below is that kind's. A diagnostic
binding runs a different loop, owed to its own document.

**Enter the run.** Admin resolves the session, opens the stream's sink under its
own principal, and directs the harness to enter, supplying the session identity,
the run reference, the trace descriptor, the
model binding, and the gate instruction. The harness never resolves a path and
never learns a name. It crosses once, in this directive, and is not
re-sent, revoked, or replaced. The harness stands up an empty working structure,
authors its load event, which is the run opening, asks the SPU to admit
the model binding it was handed, and starts Gate last. It answers ready only when
every step of that fan-out has confirmed, or it refuses naming where the fan-out
stopped, so that admin rolls back without asking a second question. Per contract
section 3 and `weaver-admin-PRD` section 4.1.

**Leave the run.** Admin directs the harness to leave. The harness stops Gate
first, refuses while a turn is in flight, authors its unload event, which is the
run closing, drains the writer's queue to the stream, and releases the SPU last.
It answers left. The stream ends where the run did, finalized by nothing, which is
why session close is only the authoring of its own event. Per contract section 3
and `weaver-admin-PRD` section 4.2. Gate last up, Gate first down: the agent is
never reachable while its interior is half built or half torn down.

**Stop.** The fifth exchange, per contract section 3. Admin conveys the operator's
intent to stop, one bit, no work. The harness aborts the current turn, the turn
closes with the stop reason marked, the run stays open, and the agent takes the next
prompt.
The harness answers with the turn's fate, aborted or nothing in flight, both clean
closes. Stop is a turn-level interruption exposed on the run-level organ because
the operator has no seat inside the gate exchange, and `weaver-harness-PRD`
section 2 already runs the loop until the model finishes or the operator
interrupts it. This exchange is the channel that interrupt arrives on.

The verbs are events. Load IS the run-open placement, unload IS the run-close
placement, stop IS the aborted turn's close placement. There is no ack layer
beside the events. The answer returning across the boundary is channel traffic,
the aggregate of the exchange, and the event in the trace is the record.

## 4. The turn path

Six steps, one fork, with the gate already serving because enter started it.

1. The client sends a prompt to the gate socket.
2. The gate passes it through opaque. The harness authors turn open at the
   inbound crossing.
3. The fork, at the harness. Same content to two sinks in parallel. The decoder
   receives the prompt over the decode socket against the resident session, and
   the emission receives the prompt as the first half of the pair.
4. The decoder returns the response to the harness.
5. The fork again on the way out. The response goes to the gate and the emission
   takes turn close, response as the second half, pair complete under admin's
   ordinal.
6. The gate passes the response out to the client, delivering a turn already
   closed at step 5. The crossing delivers. It does not clock.

The fork is not part of recording. It is a property of turn events, which have a
decoder on the other tine. Run-level events place without forking, because
nothing sits downstream of their placement. What is universal is the placement.
What forks is the event kind that has a second sink.

## 5. The trace

The trace is a recorder, not a bus. The harness hands it content and nothing
downstream reads its input out of it. The decoder reads from the harness. The
trace witnesses the traffic, it does not carry it.

One emission, one rendering, two holders, per `weaver-harness-PRD` section 5 and
the working-structure ruling of 2026-08-01. The harness authors each event once,
the durable event schema is the only schema, and the working structure holds the
same canonical NDJSON the stream carries, so nothing is derived and nothing can
diverge. Admission precedes the fan-out per
`weaver-trace-PRD` section 4.2: one rendering reaches both sinks in the same act,
the working structure lands first and is the acknowledgment, and the stream side
trails by the writer's queue with no cadence to elect and no window to tune.
It may never shed silently. Nothing on the turn path waits on the sink, and a slow
or failing sink never slows the interior read. There is no second representation
and no view this program takes responsibility for: a consumer who wants rows, an
index, or a front end
builds one on the output exiting admin, on the consumer's own compute, and if
accountability matters to that consumer they hash the stream as it lands, before
they persist it, on their own hardware.

The record moved, by ruling. Durability is the operator's as of 2026-08-01, the
program's obligation ending at the tee, per `weaver-admin-operator-contract`
section 3, and the record is what the stream accumulates on the operator's side of
the sink. When the memory round comes, memory returns as a new socket peer under
its own contract, per `weaver-agents-PRD` sections 5.1 and 9, and what it consumes is
that round's question, answered against operator-held storage per the enter cell
`weaver-admin-PRD` section 10 holds. The stream is the framework's obligation.
Everything after the sink is the consumer's.

## 6. The harness

The harness is the holder of loops, which is why this document files under it.
This is the basic loop, and later loops, memory, embedding, whatever the corpus
grows, are also held by the harness, each taking its own workflow document in
this directory. The loop is never held by the model. The SPU decoder is a token
function the harness calls, one interior step of a turn, and the loop runs until
the model returns a final answer or the operator interrupts it, per
`weaver-harness-PRD` section 2, the interrupt arriving as the stop exchange.

What the harness does in this loop: it serves the contract's exchanges, it
authors every event and places each at its position, it fans turn content to the
decoder and the emission in parallel, and it returns the response to the gate. It
holds the sequence and hands content to sinks. It does not resolve paths, does
not manage the sinks' interiors, does not translate, and does not reason about
content.

## 7. Open cells

Each cell names the ruling or measurement it awaits.

- **Session close ordering.** The shape of `session.closed` is proposed at
  `weaver-admin-PRD` section 4.4 and only its ordering is open. Awaits an
  architecture ruling, and nothing in the basic loop depends on it.
- **Stop mechanics at the decoder is closed.** The stop lands at the token
  boundary, per `weaver-spu-PRD` section 13.5, ratified at the token
  workflow's act of 2026-08-02, and the generation leaves the session
  well-framed with the family's turn terminator resident before it answers.
  The trace semantics were settled either way and are unchanged, the turn
  closing with the reason marked. Recorded as closed rather than deleted,
  this document's cells naming what settled them.
- **Tee back-pressure.** The tee has two sinks, the RAM working structure and the
  output stream, and the RAM copy always lands. The option space narrowed when the
  tee promise merged: `weaver-admin-operator-contract` section 3 makes a silent
  drop contractually impossible, so the election is between blocking the emitter,
  shedding with the gap marked in the stream, and detaching with the detachment
  marked. Which one is a spec-seat measurement against a real consumer at a real
  rate.
- **Streaming and partial turns.** Content-shaped and deferred. Carried here so
  the cell has one home, and it awaits the memory round's architecture pass.
