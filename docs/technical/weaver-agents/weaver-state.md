---
title: weaver-state
summary: the session custodian, sqlite behind its own channel, ingest and serve
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-state

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The custodian of the agent's working state.** It stores what it is handed,
organizes what it stores, and serves what it is asked for, and it does nothing
else. Two functions and no third: the trace tee's distillate flows in and is
kept, and what is kept answers asks. This is the statefulness leg returning by
the door the architecture built for it in advance - a schema extension plus a
new socket and contract, never a retrofit - and it is the first component of
that return.

It is not a memory system, and a reader arriving from retrieval-augmented
frameworks should spend that assumption here. There are no embeddings, no
similarity search, and no accumulated experience across sessions - the holdings
are a relational derivative of the session's own record, queryable by the facts
the record already carries, and they die with the session. It is also not a
second reasoning surface: it answers asks without holding an opinion about why
it was asked, and a member that judged its own contents would be a second
reasoning loop wearing a filing cabinet's name.

## What it owns

**The holdings.** Distilled events: the identifying members of every elected
event - session, run, turn, kind, sequence - plus whatever payload keys the
operator elected at load. The default election is the identifying members of
every kind and nothing more, so a deployment that elects nothing still holds
the session's shape - what happened, in what order, in which turn - and pays
for no payload it never asked to keep.

**The store.** Sqlite - an embedded relational store, elected on this
workshop's own measurements rather than preference: an in-process query is a
function call where a service on loopback pays a round trip per ask. It lives inside the
member's process as internal representation - the seam's traffic and the
store's shape are two facts, and only the first is contracted.

**Its territory.** One subdirectory on the operator's side, beside the session
record, held by the member's own account - an identity of its own that the
agent's identity cannot enter. The wall is enforced at the filesystem: a state
file the agent could read would hand the model its own state through an
ordinary tool call.

**The session boundary.** Holdings live and die with the session, and the
session - not the run - is the boundary. An unload retires a run and leaves the
holdings standing, the next load of the same session reopens them, and the
close of the session retires them. Persistence across sessions is a second
return through the same door with its own paper, and nothing here lays in for
it.

## Seams

**One, to the engine above**, on a nameless socketpair that authenticates its
peer by possession as of the ruling of 2026-08-26 - see
[the contracts page](../contracts.md). It is a member seam
rather than an organ channel: one party asks. Two kinds of traffic ride it and
no third - the distillate flowing in, and served answers flowing back when the
engine or a loop in its seat asks.

## How it works

**The election opens the channel.** The first traffic on every standing of the
channel is the election itself, whole: the elected kinds and their payload key
paths, as the load declared them. The custodian builds its indexes from it, and
a restarted member receives the identical election with its reopened channel,
which is what keeps the selection deterministic across the processes of one
load.

**Ingest flows one way and is owed nothing back.** A distillate per elected
event, in sequence order, no receipt - the fact has one home, and a
confirmation whose one reader would discard it is a known error this program
does not repeat. Nothing about a turn's completion depends on this seam
accepting anything. **A distillate lands whole or not at all**: the parse
completes before any write, and the event and its fields go in as one
transaction that rolls back entire on any failure, because a distillate held
in part would be an attributable envelope over missing pairs - a corruption
custody cannot detect later.

**Asks are answered in stream order, by the same loop that lands
distillates.** That one choice delivers the attribution guarantee without a
lock or a snapshot: an ask is answered against exactly the holdings the seam
carried before it, so every distillate sent ahead of the ask is in the
answer's view and nothing sent after it is.

**Every answer is bounded to the opener's session, and the bound is the
query's rather than the caller's.** The defect this repairs was invisible:
queries that read the whole table answered across every session the store file
had ever held, and the answers looked perfectly well formed. The session the
load declared rides the channel opener and sits in every read.

## What it refuses

**Managing.** What enters the decoder's context, what leaves it, when a flush
is worth its cost, and what any stored fact is worth to the turn at hand are
all policy, and all the loops'. The labor divides three ways and each part
holds one: the tee selects and never computes, this crate transforms as part
of organizing and never judges, the loops decide.

**Initiating.** Nothing here fires on a condition, watches a threshold, or
acts unasked. A loop that consults state is the operator's code in the
engine's seat - this crate is a place such a loop reaches, not a place one
lives.

**Competing with the record.** The trace is the one authoritative account and
the holdings are a derivative of it, never stored back into it. Where the two
disagree the trace is right by construction. Losing the member loses the
derivative and never the account: the holdings are rebuildable because
everything they distilled is still in the record, and no design in this leg
may make the session's continuation depend on the derivative surviving.

**A model-facing read path.** The engine reads state for its own assembly and
its loops' decisions, and the model receives only what the engine hands it as
rendered context. No tool opens a route around that.

**Being reached as a file.** A database file opened from two processes would
be a seam crossing a process line without a socket, which the architecture
forbids however convenient the driver makes it. The store is reached through
the seam, and the seam has been measured well below any loop's cadence.

**Outliving the session.** No export surface exists, deliberately - not
because export is hard but because a surface a future act would wish existed
is a reserved slot, and the cross-session return writes its own paper.

## What is not built

- **Who else may ask.** Today the engine is the one peer and every ask arrives
  through it. Whether a later operator surface reads state directly is a cell
  for the day such a reader exists, refused until then by the seam having
  exactly two ends.
- **Persistence across sessions.** A second return through the same door, with
  its own paper, and nothing in this component anticipates it.
- **The integration surface is thin.** Four conformance headers against 1,178
  lines and no integration test directory - the crate is young, landed in the
  same week as its charter, and its test posture has not caught up to its
  prose.
