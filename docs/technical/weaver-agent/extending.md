---
title: Extending the program
summary: what the framework requires of you when you add a shape, a tool, or an organ, and where its requirements stop
version: v0.1
date: 2026-09-15
commit: unreleased
parent: WeaverTools Technical Documentation
---

# Extending the program

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

[The loop](loop.md) is the surface an operator writes against every turn. It is not
the only one. When what you need is a shape the program does not yet have - a new
vocabulary on a seam, a tool the trait set does not describe, an organ that is not
here - you are past the springs and changing what is on the board.

**This page is about that, and about the price the program charges for it.** The
price is deliberate and it is not high, but it is exact.

## The four surfaces, cheapest first

| surface | what it changes | when it takes effect | what it costs |
|---|---|---|---|
| the agent's declaration | the model binding, the elections, the operator-tunable knobs | the next load | edit a file |
| [the loop](loop.md) | what the agent does with a turn | the next crossing, under the Python surface | edit a file |
| the composition root | which knobs are frozen and which are the operator's | the next build | a rebuild |
| **the floor and its seams** | what shapes exist to be sent at all | the next build | **a rebuild of both sides** |

The first three are yours alone and cost nothing but your own time. The fourth
touches everything, and that is the one this page is about.

## The floor is two crates and it is deliberately thin

`weaver-types` holds the agent's declaration, peer identity, and the wire
vocabulary the socket contracts draw. `weaver-traits` holds the message model,
roles, permission modes, and the tool surface.

Every domain draws from both and no domain contains either. They are linked rather
than dialed, because **a type definition cannot be sent over a socket** - it has to
be present at compile time on both sides of every seam that names it.

**Thin is the point rather than an accident.** The value of the floor is that it is
small enough to audit. A floor that accumulates becomes a place to put things
rather than a place that means something.

## Adding a shape to the floor

A shape crosses a seam only if both sides can name it at compile time. That is the
whole of the constraint and everything below follows from it.

**So a new shape is a floor edit and a rebuild of every crate that links the floor**,
which is every crate. There is no way to add a vocabulary on one side of a seam and
discover it on the other, and no runtime registration that would let you try. If that
sounds heavy, it is the same weight the type system charges anywhere, arriving at the
seam instead of at the call.

**What the framework asks of the shape itself** is small and mechanical:

- **It is a type, not a convention.** A shape agreed in prose and encoded ad hoc by
  each side is the failure mode the floor exists to prevent. The previous tree
  carried an attribute vocabulary that emitters could ignore, and the names actually
  emitted drifted into a superset of the names declared.
- **It is drawn by a seam that exists.** A definition with no seam behind it is a
  shape carried for a reader that does not exist, and the floor is small enough to
  audit only while that stays true.
- **It is named for the loop whose traffic it carries**, not for a sender and not for
  a seam. The channels have two initiators, so direction is a fact about a loop's
  walk rather than a property a name can carry, and loops are unique where senders
  are not.

## What the framework requires of a crate that links the floor

Three things, and they are properties of the running system rather than of anyone's
paperwork.

**Link the floor rather than forking it.** A crate that defines its own version of a
floor shape is a crate whose seam does not typecheck against the one it is talking
to. The compiler is the enforcement here, which is why the floor is a library and not
a specification.

**Do not reach around the seam.** Anything a seam carries as opaque is opaque to you,
and parsing it anyway couples you to a shape nobody promised to keep. The gate relays
without reading for the same reason.

**Send what the seam says and nothing else.** A seam's stated interface is what
crosses, what it means, and how it fails. Sending more is not an extension, it is a
shape the other side will refuse or ignore.

## Naming, if you are adding wire vocabulary

**Wire vocabulary is named for the loop whose traffic it carries**, not for a sender
and not for a seam. The channels have two initiators, so direction is a fact about a
loop's walk rather than a property a name can carry, and loops are unique where
senders are not. An earlier sender convention was retired when it collided on the
harness.

## Adding an organ

An organ is a crate that governs a domain and holds a two-initiator channel with
the harness. Adding one is smaller than it sounds, because of how the hub is
arranged.

Every organ presents its contracts to the harness and nothing to any other organ,
so a conflict between two organs is settled in the contracts each holds with the
harness rather than between them. **There are no lateral edges.** So **adding an
organ is a socket and a contract onto the hub rather than surgery on everything
already standing.**

What that costs in practice: a socket, a contract naming what crosses it, whatever
vocabulary that seam draws from the floor, and a rebuild. What it does not cost is
any change to the organs already there.

## Where the framework's requirements stop

Everything above is a property of the running system: what compiles, what crosses a
seam, what the hub will route. **How you organise the work of changing it is yours.**

This program keeps its own discipline for authoring the documents behind these
pages - how a change is recorded, what has to move together, what is tracked as
outstanding. **That is a working process for this shop and it is not a requirement on
anyone else.** It is not published here because it would read as an instruction, and
we are in no position to tell you how to develop software in yours.

So the honest division is this. **The framework tells you what must be true of the
code.** A shape has to be on the floor before it can cross a seam. An organ has to
present a stated interface to the hub. A consumer must not parse what a seam calls
opaque. **What it does not tell you is how to sequence, review, or record the work
that gets you there.**

## What this page does not yet carry

- **A worked example.** The most useful version of this page walks one real
  extension end to end: a seam drawing a new shape, the floor edit beside it, and
  the rebuild that makes both sides agree. That is owed.
- **The extension seam** is the one builder-facing surface for loops rather than
  for the floor, and it is named in [the loop](loop.md) rather than described in
  either place.
- **The graph's part in this.** Documents carry their nodes and edges in a fixed
  notation and the knowledge graph is generated from them, so a floor change is a
  graph change too. What a builder owes the graph is not written here.
