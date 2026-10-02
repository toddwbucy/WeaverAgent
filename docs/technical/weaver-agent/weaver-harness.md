---
title: weaver-harness
summary: the switchboard: the loops, the seams, and sole authorship of the trace
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-harness

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**One agent's interior coordinator.** One harness, one agent, one process, one
identity. It is a constituent organ of an agent, not a resident service agents
are loaded into and unloaded from - and the difference is the load-bearing one
for a reader arriving from orchestration frameworks, where the runtime outlives
the agents it hosts. Here an agent whose coordinator outlives it is not an
agent, it is a session. The whole regulation model rests on the agent being an
operating-system user, and that boundary means something only if the user owns
the whole assembly rather than a slot in something shared.

It holds control and nothing else: no substrate storage, no model of any kind,
no socket that work arrives on. The one name it binds faces inward, inside its
own sandbox, and admits exactly one caller. Model weights are resident in
[weaver-spu](weaver-spu.md) and reached across a seam, so it drives generation
without hosting it. And there is no privilege window anywhere in its life: the
process begins as the agent's identity and never holds anything above it, with
custody of the record resting on possession of a passed descriptor - a
capability, not a permission - so the worker appends to a file its own identity
could not open.

## What it owns

**Loop 0, the running agent service.** The thing that boots under its unit,
comes up as the provisioned identity, binds the coordination socket inside its
own sandbox, and sits there being one sealed agent. The organ pairs are
created during the enter fan-out, in the order the binding declares - the
decoder's residency confirms before the gate exists at all, and Gate stands
last. **One inbound listener, and no other name.** The coordination socket is
the harness's one listener, dialable by construction with the credential check
refusing. The state seam rides a nameless pair since the ruling of
2026-08-26, admin creating it at the member's spawn and the harness receiving
its end at the enter. The organ pairs have no names at all, and possession is
the authentication of every pair alike.

**The machinery loop 1 composes against.** The seat, granted on work that
arrives owed an answer and on nothing else. A turn begins at the gate and
nowhere else - nothing interior to the agent originates work, read from the
shipped contracts rather than legislated. Which loop an agent runs is the
agent's own declared fact: a compiled worker carries its loop in the binary,
a development worker reads the declared file per crossing, and a compiled
worker handed a declared loop file refuses loudly at its own argument parse.
[The loop page](loop.md) is the operator's book for this surface.

**Prompt assembly's deterministic floor.** The canonical conversation: the
identity prefix, then the session's message sequence, then the tool schemas,
in that order, always. The per-family half - the template application that
reliably elicits a tool call from one decoder and not another - lives across
the decode seam in the family library, and the rendered reality returns on the
report path, so the harness authors what the model saw without having rendered
it. No model is a build or run dependency of this crate.

**The model's view of the record.** What reaches the model is the assembly
floor above, whole: the identity prefix, the message sequence, the tool
schemas. What is scoped is the record's part - the harness reasons over the
whole working structure and renders from it only the message sequence.
Measurement events, lifecycle events, and custody records never enter a
prompt, so trace content reaches the model as ordinary conversation and in no
other form.

**The flush and the elision.** It decides when the decode context returns to
its prefix and names the span an elision removes - which part of a context is
worth keeping is cognition, and the component that holds the cache judges
nothing. The span is state and never the record: the record gains an event
carrying the span and the resident counts, and loses nothing.

**Trace authorship.** The sole writer of the record and the sole broker of
access to it. Components report - tagged with the turn key they were given -
and the harness authors: the turn brackets, the message events, the tool
events, the decode boundary, the run's opening line. One emission per event,
feeding the stream and the working structure together.

**Activity control.** Starting, stopping, cancelling, interrupting - each
returns the agent to loaded and idle, and none unloads it.

## Seams

**Seven, this crate being a party to every internal seam the program has.**
The boundary seam to [weaver-gate](weaver-gate.md), the coordination seam to
[weaver-admin](weaver-admin.md), three to [weaver-spu](weaver-spu.md) -
residency, decode, and classify, each on its own socket - the state seam to
[weaver-state](weaver-state.md), and the trace seam, which crosses no process
line and is tagged link rather than socket. Contracts for each are on
[the contracts page](../contracts.md).

Authentication follows the channel's nature: by credential where a socket has
a name - coordination, and the member's preload door - and by possession
where it has none, the state pair and the organ pairs existing only as
descriptors the right processes hold.

**The charter's own count is five and is short by two** - the classify seam
was declared on the SPU's side after that count was taken, and residency and
decode were collapsed into one entry where the contracts page carries them
separately. This page reports the discrepancy rather than settling it: where
a paper and its source disagree the paper is the defect, so the recount is
owed to the act that next touches the charter.

## How it works

**Between enter and leave, loop 0 waits, and what arrives is dispatched by
what it is.** A directive is the lifecycle interior's. A report is clerked to
the record with no turn opened and nothing answered - the trace entry is the
acknowledgment. A frame grants the seat: loop 1 enters with the turn's content
and the granted surface, does whatever it does between being handed the text
and running the turn, and the response returns through the same exchange.

**A turn's interior.** Assembly renders the delta against the resident decode
session - the identity prefix is rendered once per session and held resident,
each turn appends only its delta. The alternative is resending the full
history every turn, and the prior program measured one exploration turn
climbing from 5,988 to 24,932 prompt tokens on that path. The generation
returns with its measurement and the harness authors the boundary events.
When the tool workflow lands its exchange, a tool call will cross toward the
gate as a new direction on the boundary seam - the tool is the gate's peer by
charter, not this crate's - and until then that direction is chartered, not
running, per the last section.

**The lifecycle fan-out.** Admin holds one seam and no channel to any organ,
so the harness fans the enter and leave directives out along its own seams,
collects each organ's confirmation, and returns one aggregate. Sequencing the
organs is the harness's because the seams are. The prior program carried
roughly four and a half thousand lines of multi-agent coordination inside the
opposite reading.

**The operator's interrupt** arrives as the stop exchange on the coordination
seam, aborts whatever is in flight whatever the loop's own semantics, and
returns the agent to loaded and idle.

**A fault the worker survives is an event, and the stream is its carrier.**
With one outbound path carrying every event in order to the operator's sink, a
second carrier for the same fact earns nothing. Nothing blocks on anything
downstream of the emission, and the operator's tooling keys on the fault
fields and comes back by running a verb.

## What it refuses

**Serving more than one agent.** A harness serving many would put the
regulated behavior of several principals inside one principal, which is the
arrangement the architecture exists to avoid.

**Binding an outward name.** Work arrives already authenticated, through the
gate. The one listener faces admin, inside the sandbox, and admits one
identity - each side owns its own door.

**Safety adjudication of tool input.** No classifier inspects a command to
decide whether it is dangerous, and none is coming - a heuristic standing
where a boundary already stands gets trusted wrongly. Permission modes are
operator policy about being consulted, never a security control, and what a
tool can reach is bounded on the far side of the port test: external tools by
their own containment, internal ones by the kernel through the identity the
worker holds.

**Filtering the record.** Sole writer means the harness authors whatever
occurs. There is no recording level, no class of event it declines to emit,
and no policy applied on the recorder's behalf - what an operator elects at
load governs what is produced, and the harness authors what production
yields.

**Verification in the loop.** A check on the hot path is paid on every event,
and the loop's purpose is processing meaning rather than auditing it. What
holds instead splits by when it runs: the vocabulary guard at the one submit
call, and faithfulness checked where the record lands - outside the process
whose account it is. The walls defend against the model reaching for its own
trace through legitimate operation. They do not defend against a compromised
harness - that risk is bounded where isolation is bounded, at the operating
system, and neither wall asks the model to be trustworthy.

**Touching the cache, and driving its own creation.** It decides the flush
and holds no handle to what the flush clears. And it cannot drive the early
steps of its own load, because the worker spawn runs before the harness
exists at all - it is one of the things a load assembles, not the assembler.

## What is not built

- **The idle report.** No report authors without a turn, because nothing
  authors between turns yet - the assertion that would pin it waits on the
  mechanism.
- **The tool-call exchange.** The boundary seam's outbound direction is
  chartered ahead of the contract that must admit it - the gate contract's
  enumeration closes without it, named as owed to the tool workflow, and
  nothing here shapes what that workflow owns.
- **Corrective mechanisms are allowed for and none is built.** A
  corrective-context path cannot ride the coordination seam, whose contract
  guarantees no directive carries work, so it would arrive as a new socket
  under a new contract - and nothing is reserved for it meanwhile: no slot,
  no field, no dormant party.
