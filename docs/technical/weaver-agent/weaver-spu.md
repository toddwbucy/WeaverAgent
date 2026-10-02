---
title: weaver-spu
summary: model residency, two decode engines, and the measurement that rides a generation
version: v0.1
date: 2026-08-24
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-spu

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The organ that governs model residency, and one of them per agent.** It holds
the weights on the device, holds them exactly as long as the worker that started
it lives, and answers the engine about that residency. It is the only component
in the program that holds device memory: nothing upstream holds weights or
performs a forward pass, and it is the one authority on the device it was
assigned.

It is not a serving stack, and a reader arriving from one should spend that
assumption here. Continuous batching, paged attention, and prefix sharing across
unrelated requests answer the question of how one device serves many requests at
once, and this architecture has one agent per residency by construction, so that
question does not arise. It is also not a client of one: the engines it carries
run against weights in its own address space, because a separate serving process
would run its own admission and hold the device itself, making this component a
client of the device's authority rather than the authority - and the weights
hash, the family discipline, and the measurement below would become numbers it
repeats rather than facts it holds.

## What it owns

**Residency.** The device-side fact that one model's weights are present and
ready, established by an admit and ended by a release. **Admission is the one
check on the device, and it refuses and never evicts**: a conflict is rejected
until the operator explicitly unloads the occupant, so no load, at any point in
its sequence, can push another agent off a device.

**The hot cache, under a three-part rule.** This component owns the cache. The
engine above owns the flush decision, so the cache is flushed on the loop's
terms rather than per prompt. And that engine is forbidden to touch the cache -
it holds no handle to it, so it can neither protect nor corrupt any region of
it. The cache ends with the residency: a release frees the device and nothing
survives to be reattached to a later admission.

**The family libraries.** Everything a model family defines is defined once, in
that family's module, and nowhere else - the template, the marker vocabulary,
the tokenizer conventions, the parsing and the rendering. A family also declares
its capabilities: which operations its models serve, what template identity it
renders, whether its engine can tap for readout, and how many devices it can
shard across. Admission judges a binding against that declaration, which is how
a refusal knows to fire before a load reaches its expensive step.

**Two operation submodules.** The decoder serves generation over its own seam,
and the classifier scores content against the labels of a small artifact's head
over another. Each semantic domain gets its own submodule, and later operation
types arrive the same way - nothing is reserved for them, no trait, no variant,
no socket bound early.

**The measurement that rides a generation.** Per-token signals computed against
the pre-sampler distribution, positionally paired with the token identifiers,
absent rather than zeroed when not produced - because an empty vector and a
certain model are different facts. The timings, the model identity with its
weights hash, the template identity, the prompt-block partition, and the bound
the generation actually ran under travel with it. Produced, reported, gone:
nothing is retained here afterward.

## Seams

**Three, all to the engine above**, each on its own socket, every socket created
before this component's process exists and arriving already connected -
possession is the authentication. The residency seam carries the lifecycle
exchanges. The decode seam carries turn traffic, every request belonging to a
turn and carrying its context, so answers stay attributable with more than one
turn in flight. The label seam carries classification. Contracts for all three
are on [the contracts page](../contracts.md), linked and not restated.

## How it works

**An admit, end to end.** The instruction arrives with the model binding and the
measurement elections, crossing as one. The artifact's declaration is read, the
family's capability declaration is consulted, and the assigned devices are
judged - what the artifact needs plus working headroom against what the device
shows free. Admission judges an assignment and never makes one: it surveys no
devices looking for a fit, ranks none, falls back to none. An assignment that
does not admit is a refusal the operator answers by editing the declaration.

**A generation.** The engine above assembles the conversation and hands it in.
The family module renders it through the family's template - input formatted
correctly for the family is the family module's job, and every control marker
tokenizes to exactly one token, because a degraded marker is structure read as
prose. The forward pass runs, the sampler draws, and the emission returns with
its measurement. The family's parsers bridge the verbatim emission back to
canonical form, with parse failures carried as their own facts rather than
collapsed into clean turns.

**The readout, when elected.** The election arrives at admit beside the binding.
Elected under a serving binding, per-layer activations are reduced in place at
the tap and the reductions return on the same path as the generation, carrying
both counts that describe their shape - the boundary declared rather than
inferred, so a reader never recovers the layer count by arithmetic no document
states. Under a diagnostic binding the tap's columns themselves may be asked
for at session open and answered per sampled position, per the ruling of
2026-08-30 and `weaver-spu-PRD` section 13.7, the serving in-place clause
being that binding's alone since that act. Not elected, no
tap runs and no affordance idles. Both engines tap, by two different mechanisms,
and a tap's neutrality is a property of the built path - shown per tap, on the
engine that would serve it, never once for the election. Whether a tap holds a
column to answer the diagnostic ask is a second capability the family declares
beside it, per `weaver-spu-PRD` section 14, and an ask a declaration cannot
honor refuses at the open.

**Context edits.** The loop above elects a flush - the decode context returns to
its prefix - or an elision, a named span of the resident sequence made absent.
The state shrinks and the record grows: each edit is reported with the resident
counts on either side, and nothing is ever removed from the record.

## What it refuses

**Eviction.** Admission refuses and never evicts, ground above.

**A readout election a family cannot honor.** An instruction electing readout
against a family that declares no tap refuses at admit, because a load that
grants an observability election it cannot honor fails at its cheapest moment
or lies at its most expensive one.

**Shipping a tap that changes the run.** This refusal is spent before release,
not at admission: an elected readout must be observational - the same
declaration and the same seed produce the same token sequence with the election
on and off - and the demonstration is owed by the act that builds each tap,
per tap and per engine, before it ships. Nothing re-evaluates neutrality at
runtime, and it is not a second admission predicate: the family declaration
above is the one runtime ground for refusing an election, and a family declares
a tap only where this bar has been shown for it. The bar exists because the
mechanism most able to perturb - a scheduler callback on one of the two
engines - was the one the earliest wording did not forbid perturbing.

**Any route to the record.** It authors no event and holds no descriptor to the
sink. The handles that would matter are closed to it at the moment its process
is created, so the component that produces the measurements cannot touch the
record they land in.

**State across a residency.** No warm reload, no retained artifact between
residencies, no allocation held past release in anticipation. The process dies
with the unit and there is nowhere for the state to be kept - the cheap form of
a guarantee that would otherwise need policing.

**Fleet knowledge and device selection.** It knows what it was asked to admit
and what it holds - not what other agents exist or which would rather have the
device. Whether the fleet should spend this device on this agent is the
operator's question, answered in the declaration before a load is directed.

**Reading the agent's declaration, binding any socket, and deciding anything.**
The declaration has two readers and this is not one. The channels arrive
connected. It runs a forward pass and does not decide what to run one for.

## What is not built

- **The encoder.** Named as this domain's and deliberately unbuilt, on an
  ordering the program holds to: memory is a lossy compression of state, state
  is the trace's faithful account, and the thing that compresses cannot precede
  a trustworthy trace. No trait, variant, flag, or field waits for it.
- **The admission headroom figure.** What the working headroom is - constant,
  fraction, or derived from the artifact's shape - waits on a measurement
  against a real artifact on a real device.
- **Whether the artifact's declared shape is trusted or verified** before the
  devices are taken - a cost question waiting on a measurement of the check.
- **Families beyond the first on the native engine.** The registry knows more
  architectures than the engine serves.
- **Shard widths beyond a pair.** A pair is what the salvaged tensor-parallel
  path implements - an N-way forward and its all-reduce are work this program
  does rather than salvage it inherits.
- **The newer tap's neutrality on a real device pair.** The tap is built and
  has cleared the no-token-change bar on the host backend, which reaches
  everything but the one hazard its own Spec names on device.
