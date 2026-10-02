---
title: weaver-internal
summary: callables the loop dispatches inward, and never through the gate
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-internal

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The operator's promotion space.** The place where an operator takes a
capability their agent needs at loop speed and mounts it as a callable the
reasoning loop reaches directly - dispatched inward, never through the gate.
The ruling that chartered it settled three classes at once: the agent's working
tools are emergent - scripts the agent writes and keeps in its own home,
reached through the shell and governed by ordinary permissions - the shell
itself is the gate's own outbound verb and lives there, and what remains is a
third class that is neither: a capability that could stand as an external
script but is wanted inside, because a control loop needs its answer at a
latency and a determinism the shell round trip cannot give.

It is not a plugin ecosystem, and a reader expecting a tool marketplace should
spend the assumption here. **Membership is an operator decision, made against
latency and risk the operator has accepted, and never a contributor
convenience.** The path runs from the emergent roster inward: an agent grows a
tool in its home, the operator judges it worth promoting, rewrites it against
this charter, and mounts it for the control loop they own.

One honest note on standing: what kind of thing this crate is remains an open
question in the program's own terms. It is not an organ - it holds no process
and no channel - and the submodule definition does not reach it either. The
papers describe it as what it is: a library the worker links, with a charter
of its own.

## What it owns

**The space, and its first member.** The calculator - a scientific expression
evaluator, a pure function from an expression string to a value or a refusal
in its own words. It has its own page at
[the calculator](weaver-internal/calculator.md), which also states plainly what
is and is not wired today. The calculator is also the cap: no second
framework-shipped member joins without an act that argues its corner the way
the program argued this one's.

**The pure bar, for anything the framework ships.** A member is a function of
its arguments alone - no filesystem, no network, no clock, no randomness.
Purity is what lets a recorded call reproduce its answer, which is the
property the replay machinery prices. The framework holds itself to the bar
because it cannot accept risk on an operator's behalf - an operator-promoted
member answers to the operator instead, the same way the agent's sudoers file
does.

**An empty dependency set, as the bar's manifest form.** One library target,
no binary, no socket anywhere, and no dependency at all: a crate that cannot
name a filesystem, network, or clock crate cannot reach one by dependency. A
member that needs a dependency is arguing for operator-owned risk, and that
argument happens in an act, not in a manifest edit.

## Seams

**None.** It is linked into the worker and holds no channel with anything. It
is also unadvertised: a member carries no schema, no name the model calls, and
no entry in any prompt assembly. The model never learns this crate exists -
the loop recognizes substitutable work in the stream and answers
deterministically, which is a different thing from the model addressing a
tool it knows.

## How it works

**Call in, answer out, and nothing else.** A member's whole surface is one
public pure function: value in, answer out, on the crate's library surface.
Who fires it is the caller's fact, not the member's - elected, when the model
emits a call the loop recognizes and answers deterministically, or autonomic,
when a control loop fires it on a condition the loop set. Autonomicity is a
property of the loop the operator writes, never of the member, which is why
the crate is not named for it.

**The determinism is the point.** The same expression yields the same answer
on every call, which is what makes a recorded call reproducible on replay and
what makes the answer safe to substitute into a stream the record must stay
faithful to.

## What it refuses

**The shell.** The shell is the gate's verb, and a shell mounted here would
put the world back inside the reasoning loop - the one move the chartering
ruling exists to refuse.

**A roster or a registry.** The agent's tool inventory is emergent and lives
in its home as files it owns. The trace of the agent writing a tool is the
record that the tool exists, and a registry here would be a second account of
that fact.

**A safety classification.** A member does not carry a judgment of its own
danger, because the enforceable constraint is the identity boundary and a
heuristic beside it is hope wearing a uniform.

**Initiation.** A member that can act unasked is a control loop, and control
loops are the operator's code in the engine's seat - never members here. No
thread spawned, no process forked, no socket dialed, anywhere in the crate.

## What is not built

- **The calling surface.** Two shapes are named and neither is elected. A
  pure member is reachable by direct call. A promoted member that holds
  state or risk would be reached across a socket standing outside this
  crate - the mounting arrangement's, on the caller's side of the library
  boundary - so its faults land on a seam rather than inside the caller.
  Nothing here binds or depends on any of that: this crate stays the pure
  library either way, and which shape the calculator takes is settled when
  its first caller lands, because a surface elected before its caller
  exists is a reserved slot. Until that act the interim minimum stands: one
  public pure function per member.
- **The autonomic wiring.** A three-gate ladder governs it - the signal
  exists, the signal is actionable, and the autonomic path beats the
  deliberate loop head to head - and the ladder gates the wiring, not the
  placement: the member landed with its charter and waits.
- **The classification.** Whether this crate is a fourth kind of thing or a
  case an existing kind should stretch to is an apex question, recorded in
  the program's own register, and nothing settles it yet.
