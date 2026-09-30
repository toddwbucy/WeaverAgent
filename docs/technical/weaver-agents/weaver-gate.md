---
title: weaver-gate
summary: the agent's boundary, and the shell as the crate's own outbound verb
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-gate

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The agent's mouth and ears, and nothing else.** A local socket hook on the
front of the agent: work in, response out, both passed through opaque, with no
translation and no opinions about content in either direction. Whoever connects
gets to converse with the agent and gets nothing else.

It is not an API gateway, and a reader expecting one should spend the
assumption here. There is no protocol termination, no dialect, no translation
layer, and no listening network socket - here or anywhere in the program. An
earlier design built exactly that face, and the ruling that killed it left the
thinnest boundary that lets a local client reach a loaded agent. What a
client's bytes mean is the interior's question - this crate judges a crossing
by reading no payload at all.

It writes nothing - both of a turn's crossings land in the record because the
engine authors them, never because the gate logs. And it is an organ with its
job simplified: a domain root whose domain is the agent's external boundary,
with no member crates and none anticipated.

## What it owns

**Two doors out, one door in - and it holds both that face out.** The axis
that separates the two outward doors is which party may open an exchange. The
world opens exchanges on one, and the agent perceives and answers. The agent
opens exchanges on the other, and a registered tool answers. Each is
single-initiator, so the opening party is a fact about the socket rather than
one carried per message - which is what lets the crate judge a crossing
without reading it. The one door in - the coordination listener - faces admin
and is not this crate's.

**A boundary predicate per socket, and both exclude the agent's identity.**
Each named socket is dialable by anything that can reach it, so each
authenticates every connection by kernel-reported peer credential, judged by
the program's one shared rule shape - with a different rule per socket,
because the two admit different principals and one allow-list covering both
would admit each socket's peers to the other's. The agent's own identity is
excluded on both, and the ground differs by door: on the world side, an
elected tool dialing the agent's own mouth would let the agent prompt itself
through its front door - on the tool side, a process at the agent's identity
answering where a registered tool should be is the same loop arriving by the
other door.

**Where the door stands is the program's, and who may pass is the
operator's.** The instruction declares the access rule - the operator's
election - and the engine supplies the endpoint in the raise directive, this
crate binding it: a path inside the unit's runtime directory, which the
service manager creates before the unit's first instruction runs - so the
directory exists before anything can bind in it - and destroys with the
unit's contents at stop, measured against a live manager rather than
assumed. Each party
names what only it can know. A socket pathname outlives the process that
bound it, so a name chosen anywhere else survives its worker and refuses the
next load with a stale file. Placing it where nothing can outlive the unit
makes the hazard unreachable rather than checked for.

**The shell, as the crate's own outbound verb.** The one tool this crate
executes is the shell - not a guest, but the general form of the agent's
effect on the world, crossing the membrane the gate exists to hold. There is
no tool table: one verb, dispatched directly, and a name that is not the
shell's refuses by name. The agent's wider roster is emergent - scripts the
agent writes and keeps in its home, reached through the shell, owned by its
identity, no crate's member.

**What a client can name.** A close that answers a turn carries the turn key
and the run it belongs to, so a conversation has references in it and not only
content - something to put in a bug report, to join two client-side records
with, to name when asking for a stop. Both are carried because one does not
identify: a turn key restarts with the next run, and the pair names a turn
once. **A name is not a capability** - every seam authenticates by credential
and none accepts a name as a reason, so what a client holds is a label for
something it already took part in.

## Seams

**One internal seam, to the engine**, a socket with two initiators - the
engine opens the raise and lower direction, and the turn direction crosses the
same seam. **And the world-facing surface**, one of the program's two external
contracts, where the obligations run to whatever dials rather than to a crate.
Both are on [the contracts page](../contracts.md). The agent-opened door is
chartered as this crate's second outward seam, and its contract does not exist
yet - the last section carries that.

## How it works

**Accept, authenticate, relay unread.** A connection arrives on the
world-opened socket and the predicate judges the kernel's report of who
dialed - before any content is read. An admitted request becomes an exchange
opened toward the engine, carrying the client's line as octets this crate has
not read. The response returns on that exchange and goes back out the
connection it came in on. The exchange's own identity is the correlation, so
the crate holds no table and mints no identifier - what it needs to route a
response is what the channel already gives it. **Retention ends at the
answer**: nothing about a turn survives the response returning through it.

**Clients may speak at once, and the crate relays as they do.** More than one
exchange may be open toward the engine, which serves them one turn at a time
in arrival order. The gate refuses nothing on the grounds that the interior
is busy - waiting is what a conversation already means. Order per connection
holds in both directions, and that is the whole of what a client is owed: no
promise is made across clients, and none is needed, since a client sees only
its own connection.

**The shell executes here, forked and supervised per call**, and the result
crosses back exactly once, as the execution exchange's answer. The loop
reaches tools through this crate, so the spawn belongs on this side of the
loop's membrane.

**Lowering is modest by construction.** A lower arrives only when the run is
at rest - the lifecycle refuses a leave while a turn is in flight - so what
remains is connections peers are holding open, closed after the seams and
before answering stopped. A peer that reconnects finds nothing standing,
which is refusal by absence.

**Three faults, and one non-fault.** The listener lost while the hook is
raised - the agent unreachable with its interior healthy, which the engine
cannot observe from its side and is why the fault direction exists. A client
gone mid-turn - a lost delivery, never a lost turn, since the record already
holds the close. Admission failing systematically - a misconfigured boundary
or something probing it, either way the operator's to see. **A peer failing
the predicate is not a fault.** It is the boundary working, and a fault per
refused dial would make the record noisiest exactly when the boundary is
doing its job.

## What it refuses

**Parsing the line.** Bytes cross unread in both directions. A gate with
opinions about content is a translation layer growing back.

**Work state.** A request forwarded is a request gone - no session, no
replay, nothing retained past the answer. The turn is not this crate's to
name: nothing here mints a turn key, because the turn does not exist until
the engine opens it.

**A name as a reason.** Admission rests on the kernel's report of who dialed,
never on what was said or what identifier was presented.

**A channel to admin, and a reading of the declaration.** What this crate has
to say to admin travels through the engine as hub, and the instruction
arrives on the seam already validated - a second reading of the operator's
file would sit beside the first with every way to disagree quietly.

**A surface of its own judgment.** Two seams, each established once per
raise, are the whole of the surface area. Nothing chartered nothing opens.

## What is not built

- **The agent-opened seam carries no traffic.** The door is chartered - it
  exists, it is this crate's, the agent opens - and its contract is the tool
  workflow's to write: which end binds, what credential a registered tool
  presents, and how that maps to a registration. A credential names an
  identity and not a tool, so a contract that does not close the mapping
  authenticates a user and admits an application it never checked.
- **The dialing direction does not authenticate symmetrically, and closing
  that is the contract's demand.** The mechanism is Linux's peer-credential
  option on local sockets, and its two directions differ: where this crate
  accepts, the kernel reports the connecting peer's own credentials - where
  this crate dials, it reports the credentials captured when the far side
  called listen, before any identity drop that followed. Verified against a
  live kernel rather than read off a manual page. So a tool that listens
  under one identity and drops to its provisioned one presents the pre-drop
  identity, and the check cannot by itself confirm the peer is the
  registered tool. A regression test pinning the pre-drop behavior is the
  code's to add and is not in the tree.
- **That seam's fault cases are unenumerated on purpose.** A registered tool
  unreachable or a dial refused are faults this crate will raise, and a case
  set written against a guess is the thing the fault enumeration refuses
  elsewhere - the closure claim is scoped and the gap is named.
