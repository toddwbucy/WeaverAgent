---
title: weaver-traits
summary: the floor's vocabulary: messages, roles, permission modes, and the tool surface
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-traits

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The vocabulary the rest of the program is written against.** It holds the message
model a conversation is made of, the permission modes an operator declares, and the
tool surface a registered service implements. It depends on nothing internal, binds
no socket, performs no work, and holds no state. Every other crate links it, and
what they link is definitions alone.

It is not a utilities crate, and the distinction is the point. A reader arriving
from another framework expects the bottom crate to be the place shared helpers
accumulate - a prelude, a grab bag of conversions, a home for whatever two crates
both wanted. This crate refuses that role by charter: a thing that does work is in
the wrong crate, and a definition earns its place here only by crossing a boundary
between two others. A type only one crate uses lives in that crate, however tempting
the symmetry of putting all the vocabulary together. The value of the floor is that
it is small enough to audit, and a floor that accumulates becomes a place to put
things rather than a place that means something.

## What it owns

**Four definitions, one module each, re-exported at the root.**

**The message model.** What a conversation is made of: a message is a role and a
sequence of content blocks. Four roles - system, user, assistant, tool result - and
three block shapes - text, a tool call, a tool result. Content is a sequence of
blocks rather than a string because an assistant turn is not one: a turn carries
prose and tool calls in one emission, and a string type would force the engine to
re-parse its own model's output to find the call. The block sequence is where a
turn's parts stay distinguishable from authoring through to the trace.

**The permission modes.** The operator-facing policy vocabulary: ask before a class
of action, allow it without asking, deny it. Three modes and no fourth, because the
question has three answers, and the enum is closed so a fourth cannot arrive without
every consumer's match failing to compile. This is policy, not enforcement - the
kernel bounds what a tool can reach, and a mode governs only whether the operator is
asked first. The definition site says so because anything that reads as a security
control while the kernel is the actual control is a thing that will be trusted
wrongly.

**The tool surface.** The interface a registered outward service implements: the
name the model calls, the schema the tool advertises, and an execute method taking
the arguments exactly as the model spoke them. It is object-safe on purpose, because
the engine dispatches tools it does not know the identity of, and its future is
sendable because the floor does not get to bound which executor the composition
root chooses. This is the shape as chartered today, and it is the least settled
definition on the floor: no dispatch reaches it yet, and the tool workflow that
will supply its one consumer may still move it - the last section carries what is
open.

**The provider surface.** The abstraction that keeps the engine transport-agnostic:
the engine issues decode requests without naming a wire format, and the concrete
transport is constructed at the composition root and injected against this trait.
Its role is fixed and its signature is deferred, per the last section.

**The invariants ride the shapes.** Every type is data and derives what data
derives. Nothing here implements a default, because a default is a decision about
what an absent value means and that decision belongs to a charter, not a derive.
Enumerations are closed where the set is closed and open where it grows, elected
per type rather than as a house style. The external dependency set is one crate,
the serialization derive, with no format crate beside it - the octets are rendered
where the rendering happens, not where the shape is defined.

## Seams

**None owned, and that is a statement about seams rather than participation.**
This crate holds no process, binds no socket, and signs no contract, because it
performs nothing and so has nothing to agree to. Its definitions still cross
other parties' seams constantly - a conversation message rides the decode wire
and lands in trace payloads, in exactly the shape defined here - but they cross
as the vocabulary of someone else's exchange, never as an obligation this crate
carries. It is named in contracts rather than signing them: every contract in
the program carries a clause naming the vocabulary it draws from the floor, and
this crate's governance runs entirely through those clauses. See
[the contracts page](../contracts.md) for the seams that draw it.

## How it works

**A definitions crate works by being drawn, and the drawing is governed.** Every
contract names what it takes from this crate, even when the answer is nothing. The
union of those clauses is the crate's required surface: a definition no clause
names is unused, and a definition a clause names that is absent is a gap. The
declared vocabulary is also the only vocabulary - a consumer does not invent a
parallel definition of something defined here, which is the failure the mandatory
clause exists to make visible.

**The message model in motion.** One party authors conversation messages - the
engine - and it is also the only party that reads a message as a message. The
licensed combinations are stated because the shape cannot state them: a system or
user message carries text, an assistant message carries text and tool calls in the
order the model emitted them, a tool-result message carries tool results. The
engine refuses an unlicensed message before submitting it. The recorder downstream
judges the envelope and never the interior, so a rule it cannot see is a rule it
is not given.

**Serialization is elected for the reader.** A role serializes as a plain string.
A content block carries a stable type tag, because these payloads reach the
operator's tooling through the stream, where a consumer that is not written in
this language keys on a member name that must not move when a variant is renamed.

**A change here is loud by design.** Everything above links this crate, so a change
to the floor ripples to every consumer at compile time. That is correct rather than
unfortunate - it is the audit the floor exists to provide.

## What it refuses

**No safety classification, anywhere on the tool surface.** The prior program made
a per-invocation read, mutate, or destructive judgment the obligation of every
tool. That is gone, on a ground that has outlived two designs: the judgment
belongs to the boundary the service runs behind, not to a method the tool answers
about itself. A trait method asking a tool whether it is dangerous is a heuristic
standing where a boundary already stands, and its presence would invite the belief
that the answer is load-bearing.

**No adjudicating method on a permission mode.** No check, no is-allowed, no
predicate of any kind. The mode is what the operator declared, the engine reads
it, and a helper here would be the first step toward a floor type that decides.
The tempting method names are pinned by tests that fail to compile the day someone
writes them.

**No default on any type.** Absence is never read as a default unless a charter
says so and says what it means. A derived default on a floor type is that rule
defeated by one line.

**No internal dependency, no async runtime, no format crate.** The floor is what
everything links, so anything it carries is carried everywhere. Each refusal is
checked in the manifest rather than remembered.

**No anticipatory contract.** A trait added because a future crate will probably
want it is a reserved slot wearing a trait's shape. Everything here was demanded
by a consumer that exists, and the one definition whose consumer is still arriving
is stated as exactly that, below.

## What is not built

- **The provider signature.** Its role is fixed - injection at the composition
  root, transport named nowhere else - and its request, response, and error shapes
  are deferred to the decode work, which owns the latency measurement that would
  justify the floor taking a second dependency.
- **The tool surface's dispatching consumer.** The trait is shaped and no dispatch
  reaches it yet: its constituency is the elected outward service, and the draw
  returns when that exchange arrives. The inward callables and the one held
  outbound verb are reached by their own surfaces and never through this trait.
- **The tool call and tool result field lists** ride the tool protocol and move
  with it.
