---
title: weaver-admin
summary: lifecycle authorization, boundary verification, and custody of the sink
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-admin

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The fleet's lifecycle driver, run by the operator with root.** One crate,
many agents, and it is a constituent organ of none of them. Two facts put
lifecycle here: an engine cannot drive the early steps of its own creation,
because the worker spawn runs before the engine exists at all - and the acts a
verb performs, starting a unit under another identity, opening a sink the
agent could not, are root's acts, belonging to the one seat that holds root.

It is not a control plane, and a reader expecting a daemon should spend that
assumption here. The crate is an invocation rather than a resident: it runs
when the operator runs a verb, exits when the verb answers, and holds nothing
between verbs. What persists across invocations is what the init system and
the filesystem already hold - the standing party in every agent's lifetime is
the init system, which this program inherits rather than shadows.

**It verifies the agent's boundary and authors none of it.** The regulation
model rests on the agent being an operating-system user whose reach the kernel
bounds, and that model has a writer - the operator, by whatever means their
site already admits a principal. Creating an agent is not creating a user:
admitting a principal is administrator authority over the operating system's
own trust model, and a program that took it would be raising a second trust
model above the one it claims to inherit. So the program ships a check rather
than a constructor, and the cost is stated rather than glossed - a boundary
that passes verification is the operator's artifact, and the charter defends a
property it only confirms.

## What it owns

**Authorization of lifecycle intent.** Whether this operator may run this verb
against this agent - settled before anything is touched, the agent named on an
allow-list as a name rather than a path, and a refusal leaving the system
exactly as it found it.

**Verification of the boundary.** The identity resolves, the home directory
exists with the ownership and modes a load requires, and the record's
directory is root-owned and not searchable by the agent's identity. Any
failure refuses the load, and nothing is repaired - a verb that found a
boundary missing does not build one.

**Validation of the declaration, before a process exists.** A file that is
absent, missing a required field, or leaving the model binding's artifact unnamed
fails at the cheapest possible moment, and whether the artifact resolves is the
SPU's to answer at admission.

**Custody of the sink.** The invocation opens the sink the declaration names,
under root, and passes the descriptor into the enter directive - so the worker
writes a stream it could not have opened for itself. A passed descriptor is a
capability: it installs against the same open file description with no
permission recheck, which is the whole of the agent's access and is revocable
by closing it.

**A log of its own privileged acts.** A completed rollback or a refused load
whose invocation was interrupted has no reader otherwise, so admin keeps its
own file, as its sole author. **It records acts of the supervisor and never
conduct of the supervised** - the moment it carried a fact about what an agent
did, it would be a second record of the agent with a second author, which is
the arrangement the record's single writer exists to prevent. It is
fleet-scoped, owned by root, and the agent's identity is excluded twice over -
neither owner nor group, and the directory withholds the search bit.

## Seams

**One internal seam, to the engine** - the coordination socket, which the
engine binds inward inside its own sandbox and which admits root alone. **And
two external contracts**: the operator surface, which is the invocation itself
- a verb and an agent name in, a typed answer or a typed refusal out, the exit
status agreeing - and the init-system contract, facing the one party that
outlives every invocation. All three are on
[the contracts page](../contracts.md).

## How it works

**A load, in order - and the order is the substance.**

1. **Authorize the intent.** The invocation runs as root or performs nothing,
   so what remains to authorize is the name against the allow-list.
2. **Validate the declaration.** Before any process exists.
3. **Verify the boundary the operator wrote.** Refuse or proceed - never
   repair.
4. **Resolve the session and open the sink.** Which session is being loaded is
   admin's decision - the engine is structurally unable to make it, never
   learning a path - and the descriptor is obtained here, under root.
5. **Ask the init system to start the worker** as a transient unit carrying
   the agent's identity and the worker's provisioning. The unit receives no
   descriptor: the worker starts bare, and its first act is to bind the
   coordination socket inside its own sandbox.
6. **Dial the channel, direct enter, receive the aggregate.** The directive
   carries the session identity, the run reference minted for this load, the
   binding's kind, the trace descriptor, the model binding, and - where the
   kind declares one - the gate instruction. The engine fans it out, and the
   transition publishes only after every component the binding declares has
   confirmed. A refusal names
   where the fan-out stopped, so rollback undoes what admin's own acts built
   without asking a second question.

**The stop is one bit and no work.** The operator's intent to stop crosses the
seam, the abort itself is the engine's, and the agent returns to loaded and
idle. Unload waits on rest rather than racing it. Closing a session is load,
stop, unload - the close is content, and the engine authors content, so the
agent must be standing for its session to be closed.

**The agent has no path to its own supervisor, structurally and twice.** The
coordination channel refuses every principal that is not root at the
credential check, so an elected tool at the agent's identity reaches a refusal
rather than a supervisor. And admin's code is never linked into the worker's
address space, so no build exists in which the worker contains supervisory
code a bug or a tool could reach - which is what stops the shortcut where
someone adds a repair path to the worker because the function was already
compiled in.

## What it refuses

**The turn, in any part.** No prompt, turn, task, or run enters through this
crate. The line is stated in its live form because the operator surface is
where it will be tested: reporting state, listing agents, and driving a verb
are in bounds - carrying a prompt to a loaded agent is out, however convenient
a menu makes it.

**Provisioning, in every part.** It creates no principal, changes no
ownership, and edits no account.

**Authoring any part of the record.** Its own first contact is recorded by the
engine as the opening line of the session's first run, not by an entry of its
own.

**Reading the stream as content.** Custody is not comprehension - parsing
events is the operator's tooling's business on the other side of the sink, and
admin interprets nothing in either direction.

**Reasoning about the device.** The decoder's component admits, holds, and
releases - a conflict is discovered at admission, refused there, and the
refusal travels back inside the enter aggregate. Admin arbitrates no hardware
at any point.

**Network ingress.** There is none, here or anywhere in the program. A local
socket carrying an operator's verb binds no port and is reachable only by a
process already on the host - and a network-attached surface on the admin side
would be the first thing in the architecture arguing against the kernel
bounding reach.

**Repair.** Admin repairs nothing, adjudicates nothing, and calls nothing
good, over any artifact it touches.

## What is not built

- **The status ask.** `show` and `list` refuse today: the init system reports
  three unit values and the lifecycle has four agent states, and a translation
  between them is where invention would enter. The observation exchange
  retires the refusal when it lands.
- **The log's format and retention.** Deferred on stated grounds: a format
  decided before a rollback has run is a format decided from no measurement.
  The artifact is named regardless, so the privileged half of the lifecycle
  is not invisible to the map while its shape waits.
- **What cues the session close.** The close requires the agent standing, and
  what cues the authoring inside that window is an open cell, deliberately
  unclosed by wording.
