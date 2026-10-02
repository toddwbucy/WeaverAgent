---
title: weaver-trace
summary: the recorder, and the in-RAM working structure the loop reasons over
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-trace

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The mechanism the trace is produced through.** The trace is this program's
primary artifact - not a log of what the engine did, but the substrate the
components coordinate over and the record a session leaves behind. This crate
defines what an event is, renders each admitted event to canonical form exactly
once, holds that rendering in RAM as the working structure the loop reasons over,
and hands the same rendering to the outbound stream.

It is not a logging framework, and the difference is custody, not features. A
logging crate is something every module calls, at levels, through filters, with
the interesting records chosen by whoever holds the config. Here there is one
author - the engine - and no levels, no filters, and no choosing: every admitted
event is recorded when it occurs, and the crate decides nothing about what is
worth recording, when a session begins, or what a turn is. Those are all policy,
and all the author's. This crate guarantees the mechanical half: what it is
handed is recorded faithfully, ordered correctly, and readable afterward.

It also does not produce the trace. The engine authors every event and no other
component submits one. A reader of a finished record is downstream of a file,
not a party to this crate.

## What it owns

**The event.** An envelope and a payload. The envelope identifies the session,
the run, the turn, the sequence, the kind, the producing subsystem, the causal
parent, and two timestamps - a session-scoped wall-clock stamp for the calendar
question and a run-scoped monotonic reading for interval measurement. The two
are not interchangeable and neither answers the other's question, which is why
there is no single occurrence time.

**The closed kind set.** Twenty-four kinds, flat, every one recorded when it occurs: the
run brackets and the session close, the turn brackets, the four conversation messages,
the tool brackets, the decode boundary with its measurement, the classify pair, the
context edits - flush and elision, each carrying the resident counts either side - the
fault, the refusal, the per-position field when its election stands, and the recall, an
answered ask on the state seam named by the identities it returned, and a restored
message, one of a branch's opening conversation seated ahead of every turn, and the
score, a task's verdict on its run. Adding a kind is a document edit, because consumers
key on the closure. Five payload shapes - the conversation messages, restored ones among
them - are deliberately opaque here: this crate records their octets and never decodes
them, because the engine is the only party that reads a message as a message.

**The two materializations of a session.** The working structure is the session
in RAM: the run's admitted events, in canonical form, in order - what the loop
reasons over, which makes it state rather than a report about state. The stream
is the same events, same bytes, one per line, written to the sink the operator
declared. One rendering, held and handed, so no reconciliation between the two
can be owed and none exists. What the stream accumulates - the session record -
lives on the operator's side of the sink, and the program neither reads it back
nor vouches for what stands behind the descriptor.

**The tee.** A third reader of the same rendering: a per-event selection whose
output feeds the state seam. It selects and never computes. The election matches
on the event's kind, then selects payload keys by path, and the identifying
members - session, run, turn, kind, and sequence - cross on every distilled
event and are not electable, so no election can produce an unattributable row.
The subsystem, the causal parent, and the timestamps stay with the full record.
The election is fixed at load, so what was elected is a load condition the
record carries like any other.

## Seams

**One, to the engine**, governed by the production contract - see
[the contracts page](../contracts.md). It is tagged a link rather than a socket
because no process line is crossed: the recorder lives inside the worker and the
engine calls it. The contract carries the two exchanges, the ordering rule that
admission precedes the fan-out, the failure vocabulary, and the prohibitions on
both sides.

## How it works

**Submit, admit, sequence, render once, fan out.** The engine submits an event.
Admission checks the envelope binding and the octet well-formedness - never a
message's interior - and refusal is typed, so nothing fails silently and nothing
returns a partial result with a success status. An admitted event takes the next
sequence number, is rendered to canonical bytes once, and that one rendering
goes three ways: appended to the working structure, queued to the sink writer,
and offered to the tee's election.

**The sequence is the order and the clock is the instrument.** The sequence is
strictly increasing and gapless over admitted events, scoped to the run.
Session-wide order is assembled by the consumer from the run reference and the
sequence, because the program holds nothing across a residency: strict order
inside a run, and between runs the order the operator's clock saw the loads in -
a calendar account, not a monotonic guarantee, which is all a program alive for
one residency at a time can put on an identifier.

**Every run begins empty.** The working structure starts with nothing, the run's
first authored event is its load line - which carries every diagnostic election
of the load by name, so a record holding no measurements is distinguishable
from a record whose election produced nothing - and the stream continues at
whatever sink was connected. There is no resume path, no projection of prior
history, and no reconstruction from anything.

**Custody rides the descriptor.** The sink is opened by the operator's side and
the descriptor is passed to the worker - the write surface accepts descriptors,
never paths, so there is no way around the boundary for a crate that only ever
holds a handle. Where the sink is a file it is opened append-only, a property of
the handle rather than of the writer behaving well, and every handle is closed
to child processes at the receive, so a tool subprocess cannot inherit a
writable route into the record.

**The committed boundary is interrogable while the process lives**: what was
handed to the sink, what the queue still holds, what failed. It is not a promise
that nothing is lost. The writer's queue is forfeit to process death, the writer
hands the sink one complete line per write and retries a short write to
completion rather than reporting it as success, and durability behind the
descriptor is the operator's - committed means handed to the sink, not made
safe on anyone's disk.

## What it refuses

**A path-taking write surface.** A function that opens a path is a way around
the custody boundary, and the prior program carried exactly that - a resolver
with zero production callers whose layout three other documents cited as the
security invariant. Descriptors only.

**A mutation surface on the working structure.** No update, no delete, no reach.
The agent runs with access to a shell, and a mutable in-process store reachable
by that identity is a store the agent could alter. The guarantee is structural -
a bug in a calling crate cannot produce a mutation because the structure offers
none. Context edits happen at the decoder and the record grows where the state
shrinks: an elision adds an event, removes nothing, and a consumer reconstructs
a context by replaying the edits.

**A recording level, a filter, a sampling knob.** Any of them is a policy seat
in the mechanism, and the whole design is that the mechanism holds no policy.
The one place volume is negotiable is an election, declared at load, carried in
the record, so the posture the record was written in is part of the record.

**Decoding the conversation.** Four kinds carry messages and this crate carries
their octets opaquely. Every guarantee it makes - canonical bytes, gapless
sequence, one complete line per write, typed refusal - requires knowing nothing
about what a message says, and a dependency it does not demand is refused on
that ground.

## What is not built

- **The interoperability target.** Spans are derived on demand for tools that
  speak spans - never stored, so there is no second artifact - and whether the
  derived view targets OTLP specifically, with what conformance record behind
  the claim, is an open ruling.
- **The payload shapes the decode work settles** ride that work and land with
  it.
- Nothing else is crate-local. The kind set stands at twenty-four and the
  variant, rename, and dispatch counts stand with it.