# weaver-spu - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. It is chartered to the
end of the lifecycle workflow, an agent loaded, idle, and traced, and its remaining
sections arrive with the token workflow, gate to harness to SPU and back. **Half
chartered by ruling rather than incomplete:** a crate is chartered workflow by workflow
and this crate's part in the workflow now finishing is one thing, admitting a model and
later releasing it.

**Date filed:** 2026-07-31
**Document ID:** `weaver-spu-PRD`
**Parent:** `weaver-agents-PRD`
**Companion contract:** `weaver-harness-spu-contract`, drafted with this document
**Editorial:** Per the Working Rules.
**Landing PR:** #734

---

## 0. What this document is

The charter of the crate that holds the model on the device. It is drafted together with
`weaver-harness-spu-contract`, which governs the residency seam, the first of the two
this crate holds, and neither is complete without the other. The token seam arrived with
the decoder cut of 2026-08-02 under `weaver-harness-spu-decode-contract`, and section 6
carries both.

Level discipline, stated once. This document carries what the crate needs and why,
including the order in which it admits and releases, because that order is this crate's
own work rather than a wire agreement. What crosses the seam, what it means, and how it
fails is the contract's. How any of it is represented is the Spec's and appears in
neither.

**What this charter reaches, and what it holds open.** It reaches the two exchanges the
enter and leave directives of `weaver-admin-harness-contract` section 3 fan out into,
the residency those exchanges move between, and the process facts a charter has to state
before another crate can build against it. It does not reach the decode seam or anything
crossing it, sessions, turn processing, measurement payloads, residual readout,
sampling, or the fault cases this crate raises. Each of those is named as
deferred in section 8 or as a cell in section 10 rather than left out, because an
omission and a deferral read alike to a later reader and only one of them is a decision.

The test that drew the line is mechanical. A clause not needed to make enter and leave
true is out of this pass.

```graph
node: weaver-spu
kind: crate

edge: parent
from: weaver-spu
to: WeaverTools
```

## 1. What this crate is

**The organ that governs model residency, and one of them per agent.** It holds the
weights on the device, it holds them for exactly as long as the worker that forked it
lives, and it answers the harness about that residency across a two-initiator channel.
Both halves are what make it an organ under apex section 5.4, and neither alone would.
Governing a domain without a two-initiator channel would make it a submodule of the
harness, and a two-initiator channel without a domain would make it a second
coordinator.

**It is the only crate in this program that holds device memory.** `weaver-harness-PRD`
section 3 gives it model residency, GPU arbitration at the device, decode compute,
embedding compute, and the cache, and has the harness holding no weights and performing
no forward pass. That is a statement about this crate read from the other side, and this
charter states it from this one so that a reader arriving here is not sent back.

**The organ has a second implementation of its decode role, and it is the same organ.**
`python-spu`, per `python-spu-Spec`, implements the decode process in Python as a
program outside the crate tree and is party to the residency and decode contracts. What
this charter says of the organ binds it: the residency, the device, the cache and its
rule, the seams it serves and the one organ per agent. What it says of this crate's own
code does not, and the classify role of section 15 stays with this crate's classify
process, which `python-spu` does not replace. Which implementation serves is chosen per
agent in that agent's own admin configuration, per `weaver-admin-Spec` section 9, so two
agents on one box may be served by two implementations at once, and the sentence above
holds of the organ: whichever implementation serves an agent is the one process holding
that agent's model on the device.

**It is a domain root, and its members are not enumerated here.** The domain is semantic
processing, all of it, per the reading the stub carried and this charter keeps: decode
serving and encode serving are two jobs of one domain rather than two domains, and a
classifier or a small activation network the harness calls for effect is a third kind of
semantic processing under the same root. What organizes them is that they share
residency accounting, device arbitration, and lifecycle confirmation, and the harness
routes to them while holding none of them. Which of them become member crates is
discovered when they are chartered, per the Document Format's rule that depth is
discovered rather than designed, and this charter draws no subtree.

**It is mortal in the same way the harness is.** `weaver-harness-PRD` section 1 has
one harness, one agent, one process, one uid, and this crate inherits that shape by
being the harness's child. An SPU serving several agents would put the model residency
of several principals inside one principal, which is the arrangement the architecture
exists to avoid, and it would also give residency a lifetime that outlives the thing
residency is for.

## 2. What this crate owns

**The umbrella's substrate.** The crate is about two things, per the operator's
ruling of 2026-08-02: the family libraries of section 14, where everything a
model family defines once is defined once, and the presentation of Unix
sockets over which the harness hands work in and takes results back. Each
semantic domain of processing is tied to its own submodule, the decoder the
first per section 13 and the classifier the second per section 15,
chartered 2026-08-19 with its workflow. The encoder and the later operation
types arrive as their own submodules when their workflows charter them, and
nothing here reserves their shape.

**Model residency.** The device-side fact that one model's weights are present and ready
to serve, established by an admit and ended by a release. Residency is what the two
exchanges of section 4 move between, and it is the whole of what this crate publishes
about itself in this workflow.

**Admission is the one check on the device, and this crate is its one authority.**
Nothing upstream weighs the GPU: admin's load-time concern is the configuration file
and what it points at, per ruling C of 2026-07-31, so a device conflict is discovered
here, at model admission, and nowhere earlier. **Admission refuses and never
evicts.** A conflict is rejected until the operator explicitly unloads the occupant,
which is apex section 6's binding relocated to the place that enforces it, so no
load, at any point in its sequence, auto-evicts another agent. The refusal names
itself and travels to admin inside the enter aggregate like any other refused arm of
the fan-out.

**Release of the device.** Freeing what admission took, so that the device is available
to the next load rather than to an unload that is merely intended. Apex section 6 seats
GPU release here and the binding rule that a load never auto-evicts an occupant is what
makes an unconfirmed release expensive: an occupant that is gone but not confirmed gone
blocks a device that is free. Section 4.2 states the ordering that follows.

**The hot cache, and the rule apex section 2 asks this charter to state.** The cache is
the one surface in this program holding state that nothing could reconstruct even
in principle, which is why apex section 2 singles it out by name
and assigns its ownership rule here rather than to the crate that uses it. The rule has
three parts and all three are already settled in the merged corpus. **This crate owns
the cache.** **The harness owns the flush decision,** per `weaver-harness-PRD` section
2, so flushed on the harness's terms is an obligation of the seam rather than a policy
this crate elects. **The harness is forbidden to touch it,** holds no handle to it, and
so cannot protect or corrupt any region of it. What this pass does not state is how a
flush is expressed on the wire, which belongs to the decode seam and is deferred with
it.

**The cache ends with the residency and never outlives it.** A release frees the
device, and the cache is on the device, so nothing survives a release to be reattached
to a later admission. This is the same fact as residency ending at unload, stated once
about the surface most likely to grow quietly into session state.

**What it reports about itself, and nothing it authors.** The harness is the sole writer
of the trace, so this crate reports and the harness authors, per apex section 6 and
`weaver-harness-trace-contract`. That holds at every exchange in section 4 and it is why
this charter names no event kind.

## 3. What this crate must not hold

**Trace authorship, and any route to the record.** It authors no event, holds no event
kind, and holds no descriptor to the stream's sink. The route it must not have is
concrete rather than abstract: the worker holds the trace descriptor and the
coordination channel to admin in its address space at the moment it forks, so an SPU
that inherited either would hold a writable handle to the record its own agent produces
and a channel to the supervisor of that agent. What stops it is the close-on-exec
discipline the corpus already binds the harness to, and section 7 states the obligation
this crate places on the fork.

**A channel to `weaver-admin`.** Admin holds one seam and reaches no organ directly, per
`weaver-admin-PRD` section 6, so what this crate has to tell admin reaches admin through
the harness as hub. That is not a limitation on this crate. It is what makes the harness
the coordinator rather than one peer among several.

**Any state that crosses a residency.** There is no warm reload, no retained
compiled artifact handed from one residency to the next, and no cached device allocation
held past release in anticipation of the next admit. Residency ends at unload, plainly,
with no gesture toward keeping the expensive part warm. This is free rather than
costly, because the process holding the state dies with the unit and
there is nowhere for it to be kept.

**Fleet knowledge.** It knows what it was asked to admit and what it holds. It does
not know what other agents exist, what they hold, or whether one of them would
rather have this device, and it does not need to: it is the one authority on the
device, per ruling C, and it judges the device it can see rather than the fleet it
cannot.

**Device selection, which is the operator's and is nobody's at load.** The
binding names the devices a model runs on, per the ruling of 2026-08-03, so
this crate judges an assignment rather than making one. It surveys no devices
looking for a fit, ranks none, and falls back to none: an assignment that does
not admit is a refusal the operator answers by editing the file, not a search
this crate performs on the operator's behalf. The archived tree carried the
opposite, a selector that placed a model on whichever device had room, and it
is the seventh place that tree's working code yields to a merged ruling, per
the salvage survey. A program that placed models dynamically would be deciding
hardware policy at load, which is the arbitration ruling C removed from admin
rather than relocated into this crate. Whether the fleet should spend this
device on this agent is the operator's
question, answered in the configuration before a load is directed at all.

**A backend that delegates the forward to a separate serving process.** What this crate
carries are engines it runs against weights in its own address space, one per artifact
container, and an adapter to a general serving process is not a further one. The ground
is authority over the device rather than the shape of a dependency. A serving process
runs its own admission, holds the device it placed the weights on, and answers from a
residency this crate never judged, so a crate that delegated would be a client of the
device's authority rather than the authority itself, and the conflict section 4.1 step 3
refuses would be refused somewhere this crate cannot see. The weights hash, the
renderers' marker discipline, and the measurement of section 13.6 would move behind that
same boundary, each becoming a number this crate repeats rather than a fact it holds.
**How many engines there are is a consequence rather than a policy:** one per container
this program writes an engine for, the container being a property of the artifact read
at section 4.1 step 2, so a further engine arrives with a container and never with a
protocol.

**The agent's configuration file.** `weaver-types-PRD` section 2.1 has one writer, the
operator, and two readers, admin and the harness. This crate is not a third. The model
binding reaches it across the seam because the harness was handed it in the enter
directive, so a read here would be a second interpretation of a file admin already
validated, with no way to disagree usefully and every way to disagree quietly.

**A listening socket of any kind.** `weaver-gate` holds the agent's outward seams, local
hooks and no network socket, per the demotion ruling of 2026-07-31 and the egress ruling
of 2026-08-07 that made them two. This crate binds no socket at all, because the channel
it uses was created by the harness before the fork and arrives already connected.

**Policy about whether a load should happen.** It answers whether this device can take
this model now, and whether the agent should be loaded at all is the operator's.

**Cognition.** It runs a forward pass and it does not decide what to run one for. The
prompt is assembled by the harness, the tool decision is the model's, and the
interpretation of either is the harness's.

## 4. Loading an agent, from this crate's side

Loading an agent means one thing here. Admit a model, and later release it. Everything
in this section is derived from `weaver-admin-harness-contract` section 3 and
`weaver-admin-PRD` sections 4.1 and 4.2, which are the documents that create the
obligations this charter exists to make true.

**The two exchanges are the whole of the fan-out that reaches this crate.** Admin
directs the harness to enter, the harness fans that directive out along its own seams,
and one arm of the fan-out is the admit below. Admin directs the harness to leave, and
one arm of that unwinding is the release. Admin holds no channel here and learns of this
crate only through the aggregate answer the harness returns, which is why a refusal
below has to carry a reason the harness can place in that aggregate without translation.

### 4.1 admit

Opened by the harness, carrying the SPU instruction it was handed in the enter
directive. It ends in residency confirmed or in a typed refusal.

1. **Resolve the binding to an artifact.** A binding naming a model this crate
   cannot find is refused before the device is touched, and the refusal's account
   names the path that failed, the step that failed on it, and the identity it
   was judged under, because this crate runs as the agent and admin does not,
   per section 7: the operator's
   likeliest fault, an artifact readable to root under a directory the agent uid
   cannot traverse, is visible from here and nowhere earlier.
2. **Read what the artifact declares about itself, without loading it.** The old tree
   reads a model file's header and metadata block to answer what architecture this is
   and what its basic dimensions are, without reading tensor data and without touching
   the device. That is a mechanic worth keeping, because it converts the most common
   shape of a bad binding, an artifact that is present and wrong, into a refusal that
   costs no device work.
3. **Judge the assigned devices and the readout election against what admission
   requires.** The binding names the devices, per `weaver-types-PRD` section 2.1 and the
   ruling of 2026-08-03, and this crate judges those and selects none. What the
   artifact's shard needs plus the working headroom the residency requires must fit what
   each assigned device has free, which is the same inequality read per device rather
   than a new one. Where the set is larger than one the devices must also be able to
   reach each other, since a sharded forward exchanges activations between them and a
   set that cannot is a set that cannot serve. And the backend must be able to shard
   across that many, per section 14's capability declaration. This is the one check on
   the devices, per section 2, and nothing upstream performed an earlier one. The
   election is judged in this step as well, against what the resolved entry declares,
   per section 13.7: the entry is resolved from step 2's header read, so the check costs
   no device work and an election the engine cannot honor refuses before any device is
   taken.
4. **Take the assigned devices and load the weights.** Where the set is one this
   is one take and one load. Where it is larger the takes and the loads run in
   the binding's shard order, so a failure partway is a partial take rather than
   none, which section 5 accounts for.
5. **Confirm residency.** The answer confirms and carries nothing else, per section
   4.4.

**Every step before the fourth is refusable at no cost, and that ordering is the
substance.** A refusal reaching the harness before any device work has happened is a
refusal the enter fan-out can unwind cheaply, and a load that fails after any device
is taken is the case section 5 is written for, whether one was taken or several.

**Exactly one model is admitted, because exactly one binding crosses.** The agent's
configuration file carries a model binding, singular, per `weaver-types-PRD` section
2.1, and the enter directive carries what admin was handed. Whether the domain later
admits more than one, which is what an encoder beside a decoder would mean, arrives with
the members and is not settled by this workflow admitting one.

**Idempotence has no subject here.** The old tree allowed a model load to be
idempotent for an identical resident model request, which was a real property of a
long-lived server that several agent loads passed through. This crate begins each life
empty, admits once, and dies, so there is no prior residency for a second admit to match
and no second admit in the workflow. A charter that carried the idempotence rule anyway
would be carrying a mechanic whose premise it had removed.

### 4.2 release

Opened by the harness during the leave fan-out. It ends in the device freed.

1. **Stop serving.** Nothing new is accepted against this residency.
2. **Free the device.** The weights, the working allocations, and the cache go together,
   because they are one residency rather than three things that happen to be resident.
3. **Confirm release.**

**The confirmation is what the harness's answer to admin rests on, and the ordering is
not negotiable.** The old tree freed the reservation and deferred the release, so a
subsequent load could be admitted while the prior model still occupied the device, and
its own record names the resulting overcommit. The merged corpus already orders this
correctly, at `weaver-admin-PRD` section 4.2, where the release sits inside the leave
directive and admin publishes unloaded only on the aggregate. This charter states the
obligation from this side: **release is confirmed after the device is free and never
before it,** so that a confirmation is a fact about the device rather than a statement
of intent.

**Release is the orderly end of a residency and process death is the abrupt one.**
Both end it. Only one is confirmed. The abrupt path costs nothing that has to be
reaped, because the process holding the device dies with the unit
and the device is reclaimed with the process, which is section 5's second half.

### 4.3 The residency this crate moves between

Two states, observable at the seam, and no third.

**Empty.** No model resident and no device memory held on this residency's account. It
is the state this crate is in when the harness first speaks to it, and the state a
refused admit leaves behind.

**Admitted.** One model resident under the binding it was admitted with, and the device
holding it.

**The transition is not observable and this charter does not name it as a state.** The
exchange is the unit, the harness has one directive outstanding, and nothing in this
workflow reads residency in the middle of an exchange. A third state would be a state
machine in a charter, which is level material this corpus keeps out of contracts and has
no better claim to here.

**A refused admit leaves empty and never leaves partial.** Whatever a failed admission
took, it gives back before it answers, so the refusal the harness receives is true about
the device and not merely true about the attempt. This is the same rule as a partial
load never being published as loaded, one level down, and it is what lets admin's
rollback treat a refusal from this arm as needing nothing undone here.

**A refused admit does not end the process.** This crate answers the refusal on the
channel and stays alive to be reaped by the unit stop that follows. An organ that exited
on refusal would close the channel instead of answering, and the harness would be left
observing a death where a typed reason was available, which converts a refusal that
names its cause into one that does not.

### 4.4 What the answer carries, and what it deliberately does not

**A confirmation confirms and carries no payload.** Nothing in this workflow consumes a
description of the admitted model. The candidate content is the model's identity and its
weights hash, which apex section 8 does require, and which reach the record inside
`model.measurement` at decode rather than at load. Adding them to this answer because
they exist would be a payload field whose only reader is elsewhere, which
`weaver-trace-PRD` section 8 and apex section 9 both name as a reserved slot in data
form.

**That leaves a gap worth naming rather than closing here.** All three `model.*` kinds
are elected, per `weaver-trace-PRD` section 5, so a run recorded at the floor carries no
measurement payload and its record therefore names no model. The run happened, it was
served by particular weights, and nothing in the record says which. Whether the run's
`load` event should carry the admitted model's identity is a question about the record
rather than about this seam, and section 10 files it where it can be answered.

## 5. What a failure partway through leaves behind

Stated by where it failed, because the obligations differ and a charter that says a
refusal leaves nothing has not yet said what nothing means at a device.

**A refusal at any step before the first device is taken leaves nothing.** No
allocation, no
handle, no residency, and an answer the harness can place in its aggregate.

**A failure after any device is taken leaves every taken device freed before the
answer, and a partial take is the ordinary case rather than an exception.** An
assigned set is taken in shard order, per section 4.1 step 4, so a failure on the
second device finds the first already held, and what the refusal owes is the same
either way: give back what was taken, all of it, before answering. Per
section 4.3, whatever was taken is given back first. The cost of getting this wrong is
that a device is held by an agent that failed to load, and the next load is refused
against a conflict that does not exist.

**A failure this crate cannot recover from, having taken the device, is process death
and is not an answer.** The device is reclaimed when the process exits, so the failure
mode that would be worst under a long-lived unit, a leaked residency with no owner and
no reaper, has no subject here. What the harness observes is closure, which the contract
governs.

**A release that cannot be confirmed is survivable here for the same reason.** The
harness reports it as unconfirmed rather than synthesizing a confirmation, admin's leave
answer names where the sequence stopped, and admin's own step of stopping the unit frees
the device regardless.

**Nothing here retries.** A refused admit returns to the harness, which unwinds. This
crate does not re-attempt an admission under one directive, because two attempts behind
one operator intent is the shape `weaver-admin-harness-contract` section 6 forbids on
the seam above and there is no reason for this seam to differ.

## 6. The seam

This crate holds three seams, all to `weaver-harness`. The residency seam is the
two-initiator channel that makes this crate an organ, governed by
`weaver-harness-spu-contract`. The token seam is the decoder cut's own socket,
governed by `weaver-harness-spu-decode-contract` and not an organ channel. The
label seam is the classify submodule's own socket, governed by
`weaver-harness-spu-classify-contract` since the classifier act of 2026-08-19,
operation surface like the token seam and not an organ channel, per section
15.2. The sentence here counted one until the cut of 2026-08-02 landed the
second and two until the classifier act landed the third.

| Seam | Peer | What crosses |
|---|---|---|
| Residency | `weaver-harness` | The harness asks this crate to admit the model binding it was handed and later to release it. This crate confirms or refuses with a reason the enter aggregate can carry unchanged. |
| Token | `weaver-harness` | The harness opens the resident decode session, appends each turn's delta, and receives the generation with its measurement. The stop's cancel and the flush cross here. Per section 13 and `weaver-harness-spu-decode-contract`. |
| Label | `weaver-harness` | The harness sends content with the turn's trace context and receives every label the classify artifact defines, each scored, whole. The submodule's readiness and its faults cross here too. Per section 15 and `weaver-harness-spu-classify-contract`. |

```graph
edge: seam
from: weaver-spu
to: weaver-harness
via: weaver-harness-spu-contract
tag: socket

edge: seam
from: weaver-spu
to: weaver-harness
via: weaver-harness-spu-decode-contract
tag: socket

edge: seam
from: weaver-spu
to: weaver-harness
via: weaver-harness-spu-classify-contract
tag: socket

edge: floor-link
from: weaver-spu
to: weaver-types

edge: floor-link
from: weaver-spu
to: weaver-traits
```

**The token seam is the second seam to the same peer, on its own socket, and it
is not an organ channel.** The decoder-cut ruling of 2026-08-02 gave decode its
own socket so no lifecycle directive queues behind decode traffic, and section
13.2 carries the classification that ruling deferred: the organ test of apex
section 5.4 names one two-initiator channel, the lifecycle channel is that channel,
and this one is operation surface, so the organ envelope does not cross it and
the every-channel sentence of `weaver-types-PRD` section 2.3 stays scoped to
organ channels with no exception admitted.

**The tag is `socket` because the seam crosses a process line,** which is the test apex
section 5.1 states after its restatement. It does not imply the credential mechanism. On
a connected pair created by one process and handed to another, `SO_PEERCRED` reports the
creating process at both ends and distinguishes nothing, so this seam authenticates by
possession, which is the apex's second case rather than an exception to its first.

**The seam edge is declared here because this crate is the organ.** Under the rule
`weaver-admin-harness-contract` section 0 states, the organ declares and the harness
does not, because the harness is the hub every organ holds its two-initiator channel
with and a hub declaring its own edges would carry the whole seam graph in one crate.
The older rule, that the crate which asks declares, had no unique answer on a
two-initiator seam, which is why it was replaced, and Document Format section 4 now
carries the scoped rule.

**The channel has two initiators because this crate is an organ, and this pass charters
one direction of it.** Apex section 5.4 makes a two-initiator channel with the harness
one of the two properties of an organ, and the property does not bend. Both exchanges
chartered here are opened by the harness. The direction this crate opens is the fault it
raises, which reaches the record through the harness as author, written as the `fault`
event of `weaver-trace-PRD` section 3.1 per the fault-carrier ruling of 2026-08-01, and
the case set for it is deferred with a named exit in section 10. A reader should not
take a half-chartered direction for an absent one.

**One channel carries the organ's traffic, and decode does not share it.** Apex section
5.4 names a two-initiator channel, singular, and `weaver-types-PRD` section 2.3 has
`organ-envelope` as the carrier every organ channel draws. This paragraph once left the
decode question open and filed it for measurement, and the decoder-cut ruling of
2026-08-02 answered it structurally instead: decode took its own socket under its own
contract, which is the second seam this section's table carries, so a release directive
never queues behind decode traffic and the head-of-line risk this paragraph would have
had measured cannot arise. Section 10 records the closure and section 13.2 carries the
classification that follows from it.

**This crate links both floor crates, and the second link is the decode
workflow's answer to the question this paragraph used to leave open.** It
draws the carrier and the binding it is asked to admit from `weaver-types`,
and it draws the message model from `weaver-traits`, because the framing
ruling of 2026-08-02 has the family library rendering the harness's canonical
messages and `weaver-harness-spu-decode-contract` section 7 draws that model
across this crate's second seam. The link follows the draw, per apex section
5.3. Until that workflow this crate drew no trait and this paragraph said so,
ending by leaving the answer to the workflow that could give one, which is the
half-chartered discipline working rather than a claim overturned.
`provider-trait` is still not implemented here and the reason is unchanged:
the `provider-trait` of `weaver-traits-PRD` section 3.2 is the abstraction the
harness issues decode requests through, constructed at the worker composition
root and injected there, so it is the far side of this seam's transport rather
than something this crate implements.

## 7. Identity, process boundaries, and the model artifact

**This crate runs as the agent.** The worker holds the agent uid from its first
instruction, per `weaver-admin-PRD` section 7, and a child it forks holds the same uid.
There is no drop to order and no window to get wrong, and there is also no separation:
the SPU is a same-uid peer of the worker rather than a process the kernel holds apart
from it.

**Which puts this crate inside the hardening boundary rather than outside it.**
`weaver-admin-PRD` section 7 names same-uid reach as a live hole, closed by the worker
clearing its dumpable flag so that a same-uid process cannot attach to it and drive its
descriptors. This crate is a same-uid process by construction. It holds the weights and
one end of a channel to the interior coordinator, so **it clears its own dumpable flag
after its final exec** for the same reason and by the same mechanism. The flag resets on
`execve`, so the requirement is stated against the last exec and is a set rather than a
check.

**The fork carries exactly two descriptors across and the discipline that holds
them is the harness's.** The count was one until the token workflow's act: the
lifecycle channel's end, and now the decode socket's end beside it, both pairs
created by the harness before the fork, per the decoder-cut ruling and section
13.2. At the moment of the fork the worker holds the trace descriptor and the
coordination channel to admin. Both are close-on-exec under the receiver rule of
`weaver-organ-channel` section 2, bound on that seam by `weaver-admin-harness-contract`
section 2 and `weaver-harness-PRD` section 5, and that discipline is what keeps them out
of this process. This charter states the requirement it depends on rather than restating
the obligation: **this crate receives its two channel ends and no other
descriptor, and a
build in which it holds a trace descriptor is broken whether or not it writes through
one.** The contract binds the party that can meet it.

**The model artifact is owned outside the agent uid and readable into it.** The
agent must read the weights and must not be able to alter them, and ownership is what
buys the second half, because an owner can restore its own access whatever the mode
says. So the artifact is owned by a principal that is not the agent, readable by the
agent uid through group or mode, and writable by neither the agent nor anything running
as it. This is the trace custody argument inverted, and it lands the same way: the
property comes from non-ownership rather than from the bits.

**It is an operator artifact and this program verifies rather than authors it.** That is
the same posture `weaver-admin-PRD` section 1 takes toward the boundary, and it reaches
here through the two failure points admin already names. Admin refuses a load whose
configuration leaves the binding's artifact unnamed, before a process exists, and
judges nothing else about it, per `weaver-admin-PRD` section 4.3 as ruled 2026-09-05
on issue #456. This crate refuses an admit whose artifact it cannot resolve, read, or
parse, judged as the agent uid, which is the failure admin could not have seen: a
look from admin's side runs as root and passes a directory the agent is denied. The
refusal that crosses the seam is the floor's typed case, and the account that stays
on this side names the path and the identity, so the operator reads which file and
which uid rather than a sound binding refused for no stated reason. Neither repairs
anything.

**This crate's code compiles into this crate's processes and into no other.** It is
not linked into the worker's address space, so no build exists in which harness code
contains a forward pass it could reach without crossing the seam. This is the same
structural argument `weaver-admin-PRD` section 1 makes about admin's code and the
worker, applied one level down, and it is what keeps the routing claim of
`weaver-harness-PRD` section 3 from being a discipline someone has to keep.

## 8. What does not cross, and what waits

**The encoder is named as domain and is not built.** Encoding and decoding are one
domain and it is this crate's, per `weaver-harness-PRD` section 3. The deferral
rests on order of construction, per the operator's framing recorded on
issue #93: **memory is a lossy compression of state, and state is the trace's
faithful account of what happened.** There is no memory until
something can reliably extract from the trace what a compression takes as
input, so the encoder - the thing that compresses - cannot precede a
trustworthy trace, whatever anyone wants to build. The trace is this program's
deliverable and its reliability is the entry condition, so the ordering holds
by the program's own structure rather than by the current feature list.

The consumer argument still stands and is now the corollary rather than the
ground: an encoding is useful only if something retrieves by similarity,
nothing in the proto-stateful turn retrieves, and an embedder would produce
vectors with no consumer. A consumer could be invented sooner than a
trustworthy trace could be skipped, which is why the order-of-construction
ground is the sturdier of the two.

**State and memory are not the same thing, and this charter needs the
distinction stated.** State is the faithful account. Memory is a lossy
derivative of it. The memory leg's exclusion is a consequence of that ordering
rather than a scope decision, and the eventual memory PRDs inherit a
definition to build against instead of one to invent.

So this charter states the domain, adds no affordance for it, and builds
nothing: no trait, no variant, no feature flag, no configuration field.
**Ownership is not usage.** A charter naming a domain is a decided boundary,
and an unbuilt interface waiting to be filled is what apex section 9 forbids.

**The memory leg in every form.** No substrate, no recall, no consolidation. When memory
returns it returns as a socket peer under its own contract, per apex 5.1.

**The framework migration argument, entirely.** The old tree's charter for this crate is
substantially an argument about which tensor framework to depend on and how to relate to
it, decided against a workspace this program does not have. It is mechanics at best and
shape never, and nothing in it crosses. What framework this crate uses is a Spec
question in a program whose apex names one deliverable.

**Throughput machinery for many concurrent requests on one device.** Continuous
batching, paged attention, and prefix sharing across unrelated requests. These answer
how one device serves many requests at once, and this architecture has one agent per
residency by construction, so the question they answer does not arise here.

**Chartered by the token workflow's act of 2026-08-02, no longer deferred.** The
decode seam, sessions and their append-only protocol, turn processing, the
measurement obligation, residual readout as it reaches this crate, sampling and
its dispositions, and the flush all arrive in section 13, which is the
charter's next section arriving with its workflow exactly as this paragraph
promised. What stays out is the encoder's operations, per the paragraph
above, and the turn-path elaborations that belong to the gate's own charter,
streaming and concurrent clients among them, which reach this crate only
after that seam's turn-direction act shapes them.

## 9. Staged requirements

Recognized work with an entry condition that holds it out of this pass. This section is
authoritative for staged work belonging to this crate, per the rule `weaver-trace-PRD`
section 9 sets.

**Admission's headroom figure.** Section 4.1 step 3 requires that what the artifact
needs plus working headroom fit what the device shows free. What the headroom is, and
whether it is a constant, a fraction, or derived from the artifact's declared shape, is
not settled. The old tree carried a headroom term in its admission inequality and stated
no derivation for it. **Entry condition:** a measurement on a real artifact
against a real device, which the decode workflow produces as a side effect of existing.

**Whether the artifact's declared shape is trusted or verified.** Section 4.1 step 2
reads what the artifact says about itself. An artifact that declares one shape and holds
another is a load that fails late rather than early, and whether this crate checks the
declaration against the tensor data before taking the assigned devices is a
cost this pass cannot price. **Entry condition:** the Spec pass, with a measurement of
what the check costs.

## 10. Open cells

Each names what settles it. A cell with a proposed reading and a named test is a handoff
rather than a hole, and a draft with no cells has hidden them.

**The admit refusal reason set.** Section 4.1 names four points at which an admit can
refuse and this charter does not enumerate the reasons. The old tree's device authority
carried a usable candidate list, which is offered as mechanics rather than as the
answer: the artifact does not resolve, the artifact does not parse or declares a shape
this crate cannot serve, the artifact is present and unreadable by the agent uid, an
assigned device ordinal is not present on this host, an assigned
device is occupied by something this program did not
put there, and what admission requires exceeds what an assigned device has free.
The device-assignment ruling of 2026-08-03 adds two candidates to that list, an
assigned set the devices of which cannot reach each other and an assigned set
the backend cannot shard across, both of which are the same kind of fact as the
others, discovered here and nowhere earlier. Every device
case is caught here and nowhere earlier, per ruling C. **Settled by:** the type
half settled with the
naming ruling of 2026-08-01, the cases extending `lifecycle-refusal` as loop 0
cases, and the enumeration itself still arrives with the decode workflow's
measurements.

**Whether the record names the model that served the run is closed.** The cell
rested on a run recorded at the floor carrying no `model.measurement`, and the
ruling of 2026-08-02 retired the recording levels, so every run carries one and
every record names its model and weights hash. The question had no answer to give
once its premise left, which is a cell closing rather than being settled.

**Whether decode shares this channel is closed: it does not, per the decoder-cut
ruling of 2026-08-02.** Decode traffic takes its own Unix socket, owned by this
crate and distinct from the lifecycle channel, so no lifecycle directive ever
queues behind decode traffic and the head-of-line question dissolves rather than
being measured. The organ test still names one two-initiator channel and still holds:
the lifecycle channel is that channel, the decode socket is additional surface
this crate owns, and the sockets multiply while the organ does not. The
measurement this cell once waited on is superseded, because the risk it would
have measured is answered structurally. How the decode socket reaches the
harness, the exchanges that cross it, and whether it carries the organ envelope
at all, under what encoding, are the token workflow's to charter. The last is
named because `weaver-types-PRD` section 2.3 has every two-initiator channel with
the harness carrying that envelope, and this socket is the universal's first
unclassified instance: either it is such a channel and the envelope's encoding
on the hot path becomes a live question, the JSON election stopping short of
the decode seam by `weaver-types-Spec` section 4's own rule, or the universal
gains its first exception and says so, and that classification belongs to the
workflow that charters the traffic rather than to this closure.

**What this crate raises is named, and the corpus-wide cell is closed.** The
fault-carrier ruling of 2026-08-01 made the stream the fault's carrier and the
`fault` event kind its shape, with this charter named as the first of the
organs owing an enumeration. Section 13.10 carries it, the token workflow's
act having chartered the run this crate's faults occur in, and the remaining
two organs landed in the same workflow's gate act of 2026-08-02, the gate's
at `weaver-gate-PRD` section 13.4 and the harness's at `weaver-harness-PRD`
section 5, which closed the set across all three. What remains is the payload's
shape, which the trace act elects against the closed set rather than against
a guess, and that is the trace's work rather than a cell of this charter's.

**Device-state reporting, which the old tree answers in a way this corpus has not ruled
on.** That tree read free memory from the device driver through a command-line tool and
marked the reading as diagnostics only, holding its own accounting of what it had
allocated as the authority for admission decisions. The two disagree whenever something
outside the program holds memory, which is the case section 2 says this crate exists to
catch, so preferring the internal accounting is preferring the number that cannot see
the thing the check is for. **Settled by:** a ruling on which reading admission judges
against, taken with a measurement of what the driver query costs on the admit path.
That tree recorded a preference and not a reason, and shelling to an external tool on an
admission path is the visible cost a reason would have named.

**The naming of the wire definitions is closed.** The collision governed, per the
human's ruling of 2026-08-01, and the convention changed: wire vocabulary is named
for the loop whose traffic it carries, direction being a fact about a loop's walk
rather than a name on a two-initiator channel. This seam draws loop 0's trio and owes
the floor nothing, per `weaver-types-PRD` section 2.3.

**The classify submodule's binary spelling, settled.** Whether the classify
process of section 15 runs this crate's one binary under a role argument or a
second bin target of the same crate was representation, and either satisfied
section 15.1. **Settled** at the classifier code act's opening: the Spec's
section 11 elects the second bin target, `weaver-spu-classify`, with the
reasoning recorded there.

## 11. Edits owed

Apex section 10 requires that a change touching a contract names every party, and a
party that does not merge with the change is named as owed here. This section is that
register. Nothing below is applied by this document.
An entry leaves this register when the edit lands, because a ruling recorded and not
landed reads as settled and an entry landed and not cleared reads as outstanding.

- **The harness's never-asks assertions**, owed by 13.7 and 13.14 as amended
  2026-08-31: the discipline that a serving harness never writes the column
  ask and never opens the re-feed drive is watched on the harness, and each
  assertion lands with the code act that builds its ask.
- `open-items.md`, the note owed to this crate's stub: discharged by section 2 of this
  charter carrying its substance directly, so the item leaves the list rather than
  moving into the stub it was owed to. Named by substance because that list renumbers.
- `open-items.md`, the encoder item: it moves into section 8 of this charter, which is
  where staged and excluded work belonging to a crate lives once the crate has a
  charter to hold it.
- `weaver-types-PRD` section 2.3, owed by the decode contract's act: the token
  vocabulary this seam draws, named for the seam's currency under the naming
  ruling's ratified extension, this seam's loop being loop 1, the builder's
  and variable, defined at the floor because both parties need it and
  neither may depend on the other.
- `weaver-harness-PRD` section 2: landed in this same act. The framing candidate of
  section 13.4 ratified, the per-model assembly paragraph rescopes, the deterministic
  floor staying the harness's and the renderer's seating in the family module, with the
  rendered reality returning on the report path.
- `weaver-trace-PRD`, by the token workflow's trace act: **landed 2026-08-02**
  at that charter's section 3.2 rather than 3.1, which is where the shapes
  went, and covering four payloads rather than the three this entry
  anticipated, the `fault` shape having landed in the same act against the
  case set section 10 records as closed. Section 3.1's measurement row was
  corrected in that act too, the sampling values moving to `model.request`,
  so section 13.6's obligations are satisfied across the pair of kinds rather
  than one. Recorded as discharged rather than deleted, because the entry
  named a section the content did not land in and a reader of the earlier
  register should be able to see where it went.

## 12. Children

Specs to be written against this charter once the PRD set is ratified. Named so the set
is bounded, not drafted here, and incomplete for the same reason the charter is.

- Admission, covering artifact resolution, the header read, the device judgment, and the
  refusal set.
- Release, covering the ordering that makes a confirmation a fact about the device.
- The organ channel's construction and the fork, covering the descriptor discipline of
  section 7.
- The decode submodule of section 13: the session, the token-boundary stop, the
  measurement production, the readout tap per backend, and the encoding of the
  token seam, elected with a measurement.
- The family libraries of section 14: the per-family module surface and the per-entry
  capability declaration.
- `python-spu-Spec`, the second implementation of this organ's decode role, filed
  2026-09-28 on epic #25: which of this crate's behaviours it carries identically and
  how each is proven, which are its own, and what comparing the two certifies.

Contracts this crate is party to are written with the PRDs of their other parties, one
per seam in section 6, and are not children of this document.

## 13. Serving the turn: the decode submodule and the token seam

Arrived with the token workflow, per section 8's promise, and filed after the
children because sections 8 through 12 are cited by number across the merged
corpus and a renumbering that broke every citation would buy tidiness at the
wrong price. Everything here is derived from the turn path of apex section 3,
the grammar of `basic-inference-loop`, and the operator's rulings of
2026-08-02, with the salvage survey at `docs/project/` as the record of what
the prior program learned.

### 13.1 The submodule

**The decoder is the first operation submodule, and each semantic domain gets
its own.** The crate's shape is the substrate of section 2: family libraries
plus socket presentation, with the processing itself in per-domain
submodules. In this pass the decoder is the only submodule and it shares the
crate's one process. A later operation type arrives as its own submodule in
its own process under this domain root, with its own socket and its own
contract, the sockets and processes multiplying while the organ does not.
Nothing is reserved for the later types: no trait, no variant, no socket
bound early, per apex section 9.

### 13.2 The token seam

**The second seam to the harness, on its own socket, created by the harness
before the fork like the first.** The decoder-cut ruling gave decode its own
socket, and this section charters what that ruling deferred. The pair is
created beside the lifecycle pair and crosses the same fork, so possession
authenticates it the same way, and section 7 counts both ends.

**It is not an organ channel, and the classification is this act's.** Apex
section 5.4's test names one two-initiator channel and the lifecycle channel is it.
This socket is operation surface, so the organ envelope does not cross it,
`weaver-types-PRD` section 2.3's every-channel sentence stays scoped to organ
channels with no exception admitted, and the seam's vocabulary is its own,
named for the seam's currency under the naming ruling's ratified extension,
a loop name being what this seam cannot take with loop 1 the builder's and
variable, defined at
the floor, and owed to `weaver-types-PRD` section 2.3 by the contract's act.

**Every request on this seam belongs to a turn and carries its context.**
Apex section 5.2 scopes the join-key invariant to requests that belong to an
existing turn, and unlike the lifecycle directives that forced the scoping,
decode traffic is exactly what the invariant is for: every ask carries the
turn's context and every answer carries it back, so the reports this crate
returns are attributable with more than one turn in flight or none.

### 13.3 The session

**One resident decode session per residency, append-only, and the discipline is forced
rather than elected.** The prior program proved on live hardware that hybrid and
recurrent decoder architectures cannot roll their state back: a protocol that rewinds
resident state fails silently on the architectures that keep recurrent layers, and the
failure surfaces as position errors far from its cause. So the session advances only
forward. Each turn appends its delta at the resident end, nothing ever asks resident
state to rewind, and the protocol is uniform across architectures because the weakest
architecture sets the rule.

**The identity prefix is established at open and permanent for the session's
life.** `weaver-harness-PRD` section 2 rests the prefix's permanence on this
seam as an invariant the SPU honors, and this is where it is honored: no
operation of this seam removes or alters the prefix - the flush of 13.9 is
defined against it, and the elision of 13.13 refuses a span that reaches
into it.

**A session that cannot take the next delta refuses, typed, and sheds
nothing.** There is no eviction and no compaction inside this crate, because
either would be this crate deciding which part of the agent's context
matters, which is cognition and the harness's. The refusal names the
overflow and the harness decides what a full context means for the turn.

### 13.4 Framing, and where it is performed

**Ratified by the operator, 2026-08-02, in this act.** The family library renders. The
harness sends canonical messages, the message model of `weaver-traits`, and the
submodule renders them through the template the resolved entry renders into what the
model sees. The rendered reality returns on the report path of 13.6, the template's
identity, the token identifiers, and the block partition, so the record holds the
mapping from the canonical conversation to what the model saw, per the operator's
end-to-end requirement of 2026-08-02: input formatted correctly for the template the
resolved entry renders, verified per entry, and output style handled by the trace
holding both the verbatim emission and the canonical parse.

The grounds. Family knowledge lives in one home, section 14's, rather than
splitting across two domains or forcing the harness to link an SPU-domain
member across the topology's grain. The harness's deterministic assembly
floor is untouched: order of parts, the message sequence read from the
working structure, and everything `weaver-harness-PRD` section 2 fixes stay
the harness's, and what moves is only the renderer's application of its template. The
trace's authorship is untouched, because the render reports back and the
harness authors the report. The operator ratified the candidate with this
act and directed the whole change in one place, so the
`weaver-harness-PRD` edit lands in this same act rather than riding the
register as a condition, the change tied to its reason.

### 13.5 The turn, and the stop

**Append and generate is the turn's shape on this seam.** The harness
appends the turn's delta and the generation returns with its measurement,
the two crossings of the fork the basic loop's section 4 draws.

**Ratified at the act's merge, 2026-08-02.** The stop lands at the token boundary. The
generation checks for the harness's cancel between sampled tokens, which at production
decode rates bounds the stop's latency well under the operator's perception, and a
mid-kernel abort would buy milliseconds at the cost of device-state certainty. **An
aborted generation still leaves the session well-framed:** the turn terminator of the
renderer the resolved entry names is made resident before the answer returns, the prior
program's own lesson, because an append-only session whose last turn ends mid-emission
is malformed for every turn that follows. The partial output returns marked stopped, the
harness closes the turn with the stop reason per the grammar, and the run stays open.

### 13.6 The measurement obligation

**This crate produces what the record requires, at production time, and the
harness authors it.** `weaver-trace-PRD` section 3.1 is authoritative for
the payload's field list. What this charter fixes is the producing side:
the per-token signals are computed against the pre-sampler distribution,
positionally paired with the token identifiers, and absent rather than
zeroed when not produced, because an empty vector and a certain model are
different facts. The timings, the model's identity with its weights hash,
the template identity, and the block partition of 13.4 travel with the
generation, and nothing is retained here afterward: produced, reported,
gone, per section 3's no-state rule.

**The bound the generation ran under travels with it**, per the ruling of
2026-08-21 on issue #258 and `weaver-trace-PRD` section 3.2. The resolved
per-turn ceiling and the stop condition in force are effective values, and
section 13.8's disposition rule already says an effective value is recorded
whichever side set it, so this is that rule reaching a value it had not
reached rather than a new obligation. **The ceiling is the case that forces
it.** It is frozen in the composition root or set in an operator's file, per
`weaver-spu-Spec` section 8, so a consumer holding the record alone can read
`finish: "length"` and have no way to learn where the cut fell. The stop
token set is the same shape of fact for a generation that ended on one.

**This is the producing side rather than the field list**, per 13.6: what a
consumer is owed is `weaver-trace-PRD`'s, and what this charter fixes is
that the values are the resolved ones this crate holds at the moment of the
generation, not the declared ones an instruction carried.

### 13.7 Residual readout

**The election governs production, and this crate is where production happens.** The
agent's configuration elects the readout per load, per apex section 4's definition of
done, and the election reaches this crate at admit beside the binding, the two
crossing as one instruction. Elected, the per-layer activations are reduced in place
at the tap and the reductions return on the same path as the generation, per apex
section 3 step 6 - a clause the paragraphs below scope to the serving binding, the
diagnostic binding's wider answer being theirs. Not elected, no tap runs and no
affordance idles. **An instruction that elects readout against an artifact whose
resolved entry declares no tap refuses at admit,** because a load that grants an
observability election it cannot honor fails at its cheapest moment or lies at its
most expensive one.

**The resolved entry's declaration is the ground and the engine is not.** While one of
the two engines could not tap at all, which engine would serve was a second ground for
refusing, and standing that engine's tap on 2026-08-22 retired it: both engines tap, so
which one serves says nothing about whether the election can be honored. An entry that
declares no tap refuses whichever engine would have served it. **And an entry declares a
tap only where the bar below has been shown for it**, on the engine that would serve it,
so the declaration is a record of a measurement rather than a statement of intent.

**An elected readout must be observational, and this clause was owed.** A
diagnostic that changed the run it observes would corrupt every use of it
and would do so invisibly, so the obligation is that an elected readout
changes no token: the same declaration and the same seed produce the same
token sequence with the election on and off, shown before the tap ships.
The demonstration belongs to the act that builds each tap, per Working
Process section 6.

**The rule is 13.11's and arrives here late rather than newly.** The field's
clause of 2026-08-21 stated it for the field, and the readout, chartered
first, never carried it. **That gap was not academic for the tap
this act stands**: reading the residual on the GGUF path installs
a scheduler callback, which changes how the graph is computed rather than
only what is read from it, so the one tap most able to perturb was the one
under the clause that did not forbid perturbing. The bar is stated for
every tap, present and future, rather than for the one that exposed the
absence.

**Shown per tap and never once for the election.** Two backends tap by two
mechanisms, so a demonstration on one says nothing about the other, and a
tap's neutrality is a property of the built path rather than of the
election that asks for it.

**The reduction is the serving answer, and the diagnostic binding may ask for
what it reduced.** A lens transports a residual vector and cannot read a norm,
so an instrument of that class needs the activations the tap already holds
rather than the figure folded from them. **This crate answers them under
the diagnostic binding and refuses them under the serving one,** per the
operator's ruling of 2026-08-30.

**The permission and the isolation are one election rather than two.** A
diagnostic binding raises no gate, so the agent has no work ingress at all: it
is fed from a record the operator already holds, through the state member's
second door, and what leaves reaches a consumer standing outside the boundary.
The clause apex section 3 step 6 states holds of every agent a client can
reach, which is what it was protecting, and it is a property of the serving
binding rather than of this seam in general.

**Three elections are this clause's, made here so that one authority holds them and
every citing document points rather than deciding a share.** First, the cadence: **the
ask crosses once, at session open**, the once-crossing discipline of the instruction
reaching the one ask whose answer runs for the residency. Second, the enforcement: **the
permission crosses at admit, inside the instruction, set from the binding by admin** -
the party that holds the binding and already derives the member vector from it - and
**the open's refusal registry is this clause's, three arms and no others**: an open
carrying the ask refuses, typed, at the open where the instruction carries no
permission, where the readout was not elected at admit - no election means no tap runs
and no column exists to continue - or where the resolved entry's declaration holds no
column. The Spec and the decode contract cite this registry and add no arm of their own.
This crate still never learns the binding's kind: it learns one admitted permission that
admin vouches for, which is the same shape every other admitted election takes. The
harness's own discipline, that a serving harness never writes the ask, is watched on the
harness and its assertion lands with the code act, named here as owed. The bar is 13.7's
unchanged: a load granting what it must not answer fails at its cheapest moment or lies
at its most expensive one.

**What the tap can answer bounds what may be asked, and the bound's ground
is the tap's own copy rather than the engine's gather.** Third election: the
answer covers **every position at which a token was sampled** - each decode
forward, and each append's prefill final position, a turn holding one append
per generation so a tool round adds its own - and
nothing else of the prompt. The ground is stated
as it is: at every layer but the last the callback sees every prompt
position, and what narrows the answer to one column per layer per forward is
the tap's one-column copy, a design choice this charter keeps because it is
what makes the tap cheap enough to elect. A later act that widens the copy
widens the bound with it, against this clause rather than against a
mechanism claim.

### 13.8 Sampling, and the dispositions

**Every sampling knob carries a disposition, and seed is a knob.** Per the
composability ruling of 2026-08-02: each knob is frozen at the worker's
composition root or left operator-tunable, the builder's election per knob,
and the effective values are recorded whichever side set them, because a
disposition changes who sets a value and never whether the record holds
it. The prior program never made its seed configurable at all, so the
disposition mechanism is the seed's first real home.
The knob enumeration and its types are the Spec's and the floor's.

**The declared seed is the run's, and each generation draws from its own,
derived.** Per the operator's ruling of 2026-08-21 on issue #255. An
earlier form of this section said a frozen seed plus a frozen sampling
surface was what made a binary's declared starting field re-enterable, per
apex section 8's third arrangement. That was false of the mechanism
underneath it: one sampler stood for the residency and every draw advanced
one stream, so a generation's draws depended on every draw before it, and
a flush reseeded the stream as a side effect of clearing the penalty
window. Freezing the seed did not deliver what freezing was said to
deliver, and apex section 8 is untouched by the correction, having only
ever said that freezing narrows variance and buys audit rather than
determinism.

**The sampler's random stream depends on the run's seed and on which
generation it is, and on nothing that happened before it.** The claim is
about the stream and not about the draw, and the distinction is the whole
of what this section promises. A draw is that stream read against a
distribution, and the distribution is the model's answer to everything
resident, so what a generation produces depends on its context exactly as
much as it ever did. What stops depending on history is the luck.

The derivation is the Spec's and its inputs are the declared seed, the
turn, and which generation of that turn this is, the last because a turn
runs as many generations as its tool rounds and two of them sharing a seed
would draw one stream twice.

**Re-entering a generation takes the seed, the surface, and the state.** A
reader supplies the declared seed, the turn, and the generation's index,
which fix the stream. **The rest of the effective sampling values beside
them**, because a stream is only half of a draw: the temperature and the
truncating filters decide what the stream selects, and the penalty pair
decides against what. A re-entry differing in any of them draws the same
luck through a different filter and answers something else. And the same
resident tail and the same weights standing behind all of it, since the
state governs the distribution the whole apparatus is read against.

**The values are the requirement rather than where they come from.** Today
every one but the seed is frozen at the composition root, so a re-entry on
the same binary carries them without asking, and the record holds them
regardless because a disposition changes who sets a value and never
whether the record holds it. That is what makes them re-suppliable when a
binary changes its mind about which are frozen, and it is why apex section
8 lists the sampling parameters among what deterministic re-feed requires
rather than listing the seed alone.

**The derived value is what a re-entry reproduces rather than what it is
given.** It is recorded so a reader can check that the stream came out
where it should, and this program offers no route that accepts it: a
sampler is built from the three inputs above and from nothing else.
Reading it as a substitute for them would be a different operation than
the one on offer, and it would be one nothing here performs.

What the ruling removes is narrower than it may sound, and worth keeping
narrow: the need to replay every draw that preceded a generation in order
to reach its stream. That is a smaller thing than replaying a run, and
the program still claims no run again.

**Two arms of one comparison meet each turn at the same stream**, so where
they differ, the difference is the treatment's and the context's rather
than the sampler's position. That is what a paired comparison needed and
could not have while one stream ran the length of a residency.

**Both seeds are recorded, and the requirement is this charter's.** The
disposition rule above already has the effective values recorded whichever
side set them, because a disposition changes who sets a value and never
whether the record holds it. Under derivation the effective value is two
values, and neither alone answers what a reader needs: the declared seed
does not say what a generation drew from, and the derived seed does not
say what to declare in order to re-enter it. A record carrying one of them
supports neither re-entry nor audit. Which members carry them and how they
spell is the Spec's.

**The sampler holds nothing across generations.** It is built for each
from the derived seed, and the penalty window is restored from the
resident tail rather than accumulated, which is the same window read two
ways and makes the two engines agree by construction. **The flush stops
being coupled to sampling at all**: a window read from the resident tail
after a truncation is the truncated tail, so nothing needs clearing and
nothing gets reseeded by accident. The defect that ruling closes was
invisible in the record and would have eaten a paired comparison silently,
the two arms differing in resident length by construction, crossing the
flush threshold at different turns, and one reseeding where the other did
not.

### 13.9 The flush

**The harness owns the decision and this seam carries it, the cut included.** The flush
is the harness's ask on this seam, and since the operator's ruling of 2026-08-19 the ask
names its cut: after a flush the session's first kept tokens are resident and everything
beyond them is gone, the kept length being the ask's, bounded below by the identity
prefix the prefix invariant keeps permanent and above by the resident count. The bound
is arithmetic rather than refusal, the confirmation's counts carrying what held, so no
outcome is silent. The cut is the loop's because the loop knows what it does not want to
re-decode - a fixed outcome made one policy true for every use case, and which part of
the context matters is cognition, which section 13.3 places outside this crate for
eviction and this section now places outside it for retention, the same rule facing both
ways. Where the resolved entry's flush permits truncation to a position the outcome is
reached by truncating to the kept length, and where it cannot roll back it is reached by
re-establishing the kept resident sequence fresh, the invariant being the outcome rather
than the mechanism, and the mechanism the Spec's per resolved entry. The harness still
holds no handle to the cache and touches nothing, per section 2.

### 13.10 The faults this submodule raises

The enumeration section 10's cell waited on, each a fault the worker
survives, reported to the harness and authored by it as the `fault` event:

- **Device fault during generation.** The device errored mid-forward and
  the generation cannot complete. The turn fails, the residency's
  continued fitness is this crate's next answer, and the report names the
  device's account of itself.
- **Residency degraded.** The weights or the session state are no longer
  servable, discovered outside any single generation.
- **Readout fault while elected.** The tap failed with readout elected,
  the generation itself surviving, reported because an elected
  observability that silently stopped observing is the lie 13.7 refuses.
- **Field fault while elected.** The distribution could not be read or
  ranked with the field elected, the generation itself surviving,
  reported for the reason the readout's fault is: 13.11 refuses the same
  lie, and a field that stopped being produced while its election stood
  would leave a viewer unable to tell an absent election from a broken
  one.
- **Elision refused.** The span's bounds do not describe a removable
  region: it overlaps the identity prefix, it runs past the resident count,
  its end precedes its start, or it is empty, the end equalling the start.
  **The empty span refuses rather than succeeding on nothing**, because a
  span that removes no position is a span the loop computed wrongly, and an
  answer confirming an elision that elided nothing would report a state
  change that did not happen. A refusal rather than a fault, typed on the
  seam, because the ask was answerable and wrong rather than the session
  being unwell.
- **Surprisal fault while elected.** The per-position vector could not be
  produced with its election standing, the generation itself surviving,
  reported for the reason the readout's and the field's faults are: an
  elected observation that silently stopped observing is the lie 13.7
  refuses. **The fault does not depend on the perplexity.** The scalar may
  stand or fail beside it, and a standing scalar corroborates what the
  record already says rather than being what makes the failure reportable,
  the election alone being the thing that was promised and not kept.

**An asked column that does not arrive is the same fault**, added 2026-08-31
with the diagnostic answer: where the ask stood at open, a sampled position
whose column message never crosses is an elected observation that silently
stopped observing, owned here so that three restatements downstream cite one
clause rather than each asserting a share. The record's expression of it is
the diagnostic-Spec's and lands with the code act, named there as owed.

An overflow is a typed refusal on the exchange rather than a fault, and a
death is observed through closure rather than reported, both per the
contracts. The gate's cases arrive with its turn direction, and the
corpus-wide case set closes there, per section 10.

### 13.11 The probability field

**The election governs production and this crate is where production
happens**, the same shape as 13.7's readout and deliberately so. The
agent's configuration elects the field per load, the election reaching
this crate at admit beside the binding, and elected it carries a depth:
how many candidates are ranked and reported at each position. Not
elected, nothing is ranked and no affordance idles.

**What the field is, and what it is not.** At each decode position the
distribution over the vocabulary exists for exactly as long as it takes
the sampler to consume it, and 13.6's measurement already reads it there
to take the entropy and the surprisal. Those are collapses of the
distribution to one number each, and no pair of collapsed statistics
recovers which tokens were available or in what order. **The field is
that ordering**: the ranked candidates with their probabilities and which
rank the draw landed on, produced at the same site and from the same
distribution as the measurement, because a field taken after the draw
would report a state the sampler has already moved past.

**The depth has a floor and the floor is the sampling cutoff.** The
sampler truncates before it draws, so there is a wall the operator's own
top-k election puts in the field, and that wall is a fact about the run
where the reported depth is an artifact of the reporting. Distinguishing
them requires reporting past the wall: at a depth below the cutoff the
two coincide exactly where the field is widest, which is where anyone
looks. A declaration electing the field at a depth below the sampling
cutoff therefore refuses at admit, naming both numbers, and the margin
above the cutoff is what makes the wall a measurement rather than an
assumption.

**The election is per feature, per load, and recorded.** No profile
names a bundle of elections, because a named bundle's contents drift and
every record already carrying that name silently becomes a record of
something else. Each election stands alone in the configuration, is
frozen for the residency, and is named individually in the record of the
load, so a record declares the posture it was written in. **A record
without a field is then distinguishable from a record whose election
stood and produced nothing**, which is the difference between a
configuration and a fault, and no consumer can recover it from the
absence alone.

**Diagnostic elections are accepted rather than mitigated.** A field at
useful depth costs decode time at every position and grows the record by
an order of magnitude, and the answer to that is that it is off unless
someone wants it, not that it is made cheap enough to forget. The same
rule is owed to what follows: residual capture is expected and is
larger than this by orders of magnitude, and it arrives as its own
election under this pattern rather than as a widening of this one.

**An election must be observational before it ships.** A diagnostic that
changed the run it observes would corrupt every use of it and would do so
invisibly, so the obligation on this crate is that an elected field
changes no token: the same declaration and the same seed produce the same
token sequence with the election on and off. The demonstration belongs to
the act that builds it, per Working Process section 6.

### 13.12 The surprisal, and what stands in its place

**The election governs production, and what it elects is a vector rather
than an instrument.** Per the operator's ruling of 2026-08-21 on
issue #258. The per-position surprisal, the drawn token's own surprise in
bits, has been unconditional since the measurement was written and belongs
to no
arrangement of apex section 8. It is elected per load beside the field and
the readout, unbundled from both for the reason they are unbundled from
each other.

**This election runs the other way from the two beside it, and the
difference is worth stating.** 13.7 and 13.11 elect an observation into
existence, so unelected there is nothing and the record says so by absence.
Here the reading already existed and the election is what keeps it, so the
default posture is the smaller record rather than the larger. Measured over
the series of 2026-08-20, the vector was thirty-six percent of the bytes a
run produced, against under ten percent for everything apex section 8
enumerates.

**What stands in its place is the generation's perplexity, and it is a
different reading rather than the same one summarized.** Two to the power
of the mean surprisal, one number per generation, produced whether or not
the vector is elected and absent where the distribution was not there to
read, which is the one ground the entropies are absent on rather than a
ground of its own. It is what the vector was read for in practice: a
generation's difficulty, comparable across runs, which is what lets a cold
record be ranked without a device.

**The accumulator is not a produced reading, and this is the line that
keeps the election honest.** Section 5's ruling of 2026-08-02 has the
recorder filter nothing and what an operator elects govern production, so
an election that computed a reading and then declined to write it down
would be a recording level wearing an election's name. Computing a mean
requires each term, and the terms are arithmetic in flight rather than
readings produced: nothing holds them, nothing could report them, and they
do not survive the position that made them. **The test is whether the
absence is recoverable from what remains**, and it is not, a mean being
lossy in exactly the way the ruling cares about. What the election governs
is whether a reading per position is produced at all, and unelected none
is.

**Entropy does not move, and its argument is not this one.** The
distribution's entropy at each position stays unconditional, per
issue #258's navigation tier: it is how a run carrying no election says
which
turn deserves reloading with one, and a record that cannot aim an
instrumented run has lost more than seven megabytes buys back. The two
readings are computed at the same place against the same distribution and
are elected differently, which is a fact about what they are read for
rather than about what they cost.

### 13.13 The elision

**The harness names a span of the resident sequence and this crate makes it
absent.** Per the operator's rulings of 2026-08-22. The ask carries the
span's bounds, the session's resident sequence afterward is what stood
before with those positions gone and what followed them re-established
against the shortened prefix, and the confirmation carries the resident
counts either side. **The span is state and never the record**: what is
elided is the sequence this crate holds resident, and no operation in this
program removes anything from a trace.

**This crate decides nothing here, which is 13.9's rule facing a third
way.** Section 13.9 places which part of the context matters outside this
crate twice over, for eviction and for retention, on the ground that it is
cognition and belongs to the loop. Naming a span is the loop exercising that
same possession rather than this crate acquiring it: the harness knows what
it does not want re-read, says so in positions, and this crate executes what
the ask names exactly as it executes the flush's cut. **A crate that chose
the span would be judging what a context is worth**, and that is the thing
the rule refuses in all three directions.

**The outcome is specified and the mechanism is the Spec's per resolved entry**, which
is 13.9's shape and for its reason. Where the resolved entry's flush permits a rollback
to the span's start the outcome is reached by rolling back and re-establishing the tail,
and where it cannot it is reached by re-establishing the kept sequence whole. The
invariant is that the resident sequence afterward is the concatenation, in order, of
what preceded the span and what followed it.

**The identity prefix stays permanent and bounds the ask from below**, per
the prefix invariant the flush already answers to. A span reaching into the
prefix is refused rather than clamped, which differs from the flush's cut on
purpose: a `keep` below the prefix is an ask for less context that the
arithmetic satisfies without misreporting, while a span overlapping the
prefix is an ask to remove what the session is defined by, and it has no
smaller true version.

**Nothing is inserted, and the seam stays append-only.** The elision removes
and does not replace. Whatever should stand where the span stood arrives as
the next contribution and therefore lands at the end, because this seam
appends and this act does not change that. **Injection is not this act's to
charter**: apex section 9 already places it with the harness, a function
loop injecting a computed result where a stochastic one would otherwise go,
so the two halves the calculator work needs are already divided between two
parties and only this one was missing.

**The state and the record disagree afterward, and that is the design.** The
record holds the elided material and gains an event saying it was elided.
The state holds neither the material nor a marker: a context carrying a
scar would be a reserved slot for a reader that does not exist, and the
model is not the party the elision is accountable to. **The record is that
party.** A consumer reconstructing what the model held at any turn replays
the record's edits, the flushes and now the elisions, rather than
accumulating contributions naively, which is the accumulation rule
`weaver-trace-PRD` section 3.2 already carries meeting a second kind of
edit.

### 13.14 The re-feed drive

**The replay's forward passes run through this crate, and the drive that runs them is
this clause's**, added 2026-08-31 per `diagnostic-replay-loop` section 2, which named it
as the one act standing between the diagnostic papers and a running null replay. One
drive per recorded generation: the recorded rendered contribution appends verbatim, no
renderer on the way, the forward passes run along the recorded token path, and at each
position the draw is computed exactly as a generation would compute it - the derived
seed of 13.8, the effective knobs, the sampler consuming the distribution - **and the
recorded token appends whatever the draw said**. A divergent draw is a finding carried
in the answer, never a fork in the path, so every later position stays comparable to its
recorded self instead of drifting behind the first divergence.

**The drive draws no token of its own, and the answer's type is what makes
that checkable.** No token intermediate crosses, there being no drawn piece
to stream, while the field and the column cross where their elections
stand, which is what makes this drive the reader pass's substrate. The
answer carries the measurement with the recomputed draw identifiers where
sampled identifiers would sit, and it is its own answer and never a
generation's: a supplied path in a sampled path's clothes is the collapse
the protoautonomic design names as the defect class, and the answer's own
type makes it unrepresentable rather than discouraged.

**The permission is admin's, set from the binding's kind, named
individually beside the column's per the no-bundle rule.** A serving
harness never opens the drive and cannot usefully lie about it: the drive
refuses, typed, where the instruction carries no admitted re-feed
permission. **The registry is this clause's, two arms and no others**: no
admitted permission, and an empty recorded path, a replay of nothing
wearing an exchange. The session and ordering refusals of the seam cover
the drive as they cover every ask and are no arm of this registry. The
harness's own discipline is watched on the harness beside the column ask's
watch, its assertion landing with the code act, named in section 11 as
owed.

**What the drive does not do bounds it the way the column's bound does.**
It replays what a record holds and nothing else: no re-render through a
template that may have changed, per the ruling of 2026-08-12 that the
record carries the rendered form precisely so a replay does not, and no
sampling of a fresh path, that being a different instrument this charter
does not hold.

## 14. The family libraries

**Everything a model family defines is defined once, in that family's module, and
nowhere else.** A family is a template lineage, on the operator's ruling of 2026-09-28:
the models that share one format family whatever their vendor, Qwen or Gemma or the
Harmony speakers, held in one module with the orchestration quirks the shared kernels
are driven by, a decode lineage rendering that format through one or two renderers and a
classify lineage rendering nothing and serving its entries from the classify table. The
prior program proved the premise concretely: its encoder's text tower was a
decoder-family member, sharing architecture and tokenizer with the decoder path that
never knew it. The module discipline is the boundary: one module per family, with
nothing family-specific living outside its module and the kernels shared beneath, which
is the prior program's own share-kernels-own-orchestration rule promoted to a charter
line.

**What sits beneath a lineage, and which of four things owns each fact, read from the
code as it stands.** The module, the lineage, owns its placement, its shared
orchestration, and the renderers it holds, one or two for a decode lineage and none for
a classify lineage, whose entries sit in the classify table and render nothing. A
renderer, the format the module renders and the thing an entry names, owns the rendering
of an identity prefix and of a turn's delta, the fold where its template names no system
turn, the marker vocabulary, the parsing of the emission, the stop conditions and the
turn terminator. A registry entry, which carries one architecture where an architecture
may carry several entries, owns the architecture string the artifact's header declares
and the tensor layout it names, the configuration shape, the template identity, the
generation opener, which renderer it names, the markers that select it where its
architecture is contested, the flush mechanism with rollback derived from it, the
readout tap, the column, and the shard widths, which together are the capabilities
admission judges. Which operation an entry's models serve is which table the entry sits
in, the decode registry or the classify table, and not a field of the entry. The
artifact owns its tokenizer and its weights, and a lineage's tokenizer conventions are
what its renderers' markers ask of that tokenizer, every control marker promoting to one
token. Where one architecture string carries two formats, the module holds both entries
and the artifact's own template selects between them. Admission and the decode path read
an entry's facts from the entry the artifact resolved to, and a renderer's from the
renderer that entry names, never from a module by its name.

**Both directions of the end-to-end template requirement live here and are tested
here.** Inbound, the renderer the resolved entry names is what makes input formatted
correctly for its template, and the reference test shape is the prior program's marker
promotion, every control marker tokenizing to exactly one token because a degraded
marker is structure read as prose. Outbound, the renderer's parser is the recorded
bridge from the verbatim emission to the canonical form, with parse failures carried as
their own distinct facts rather than collapsed into clean turns.

**The modules become member crates when the second consumer exists, and
not before.** This round the decoder is the only consumer, and a shared
library with one consumer is a reserved slot by apex section 9's own
test. The boundary is drawn now so the extraction is a local move in the
act that charters the encoder, which is the reversibility test applied
to this crate's own future, and the destination is recorded where
destinations live rather than here.

**An entry declares its capabilities, and the entry the artifact resolves to is
consulted at admit.** The operation its models serve, by the table it sits in, what
template identity it renders, whether its engine can tap for readout, **whether its tap
holds a column to answer the diagnostic ask** per 13.7 as amended, and **how many
devices its backend can shard a model across** are facts the entry states, and admission
judges a binding against the resolved entry's, which is how 13.7's refusal knows to fire
and how section 4.1 step 3 knows whether an assigned set is servable. The declaration's
shape is the Spec's.

## 15. Classifying content: the classify submodule and the label seam

Chartered 2026-08-19 with its workflow, the loop-side relevance work of the
context-management arc: a loop that rebuilds a working set from custody
needs a judgment about content it cannot make from string mechanics alone,
and a classification is semantic processing, which is this domain and no
other crate's. The operator's ruling of the same date drew the line this
section builds on: the loop owns the why and the when, and this crate
processes and never judges what its output is for.

### 15.1 The submodule

**The classifier is the second operation submodule, and section 13.1's rule
for later types is exercised for the first time.** It arrives as its own
submodule in its own process under this domain root, with its own socket
and its own contract, the sockets and processes multiplying while the organ
does not. The lifecycle channel stays the one two-initiator channel and it
stays with the decode-carrying process, so the organ test of apex section
5.4 still names exactly one organ here. The separate process is the failure
boundary: the decoder's process dies of a poisoned session or an engine
fault, and a classifier riding inside it would die of the decoder's wounds,
so each submodule fails alone under the one domain root.

### 15.2 The label seam

**The third seam to the harness, on its own socket, created by the harness
before the fork like the other two.** Possession authenticates it, the pair
having no name and no second opener, and it is operation surface rather
than an organ channel, the same classification section 13.2 carries for the
token seam. It is governed by `weaver-harness-spu-classify-contract`, cut
with this act per apex section 10, and the seam edge lands in section 6
beside its siblings.

### 15.3 The artifact and its admission

**The classify artifact is declared beside the model binding and admitted
at load under the one authority.** An agent that declares no classifier
runs none, the submodule optional by presence like every optional leg, and
the seat's ask then answers as a missing leg. Where the declaration carries
the binding, the enter fan-out grows an arm: the harness forks the classify
process as it forks its siblings, the binding arrives at exec, and the
process admits before it serves, its readiness or its typed refusal
traveling in the enter aggregate like any other arm. Admission law is
unchanged in every word that matters: a conflict is discovered at
admission and nowhere earlier, the refusal names itself, and nothing
evicts. **The model admits before the classifier**, so where the pair
exceeds the device the refusal is deterministically the classifier's and
names its own arm rather than whichever raced last. A load failing at
either arm fails whole, per the lifecycle's standing shape and section 5's
partway rule, leaving nothing admitted behind it, and unload releases the
set.

### 15.4 The exchange

**Stateless, whole, and turn-attributed.** The harness sends content with
the turn's trace context, and the answer carries every label the artifact
defines, each with its score, in one message. There is no session, no
accumulation, and nothing retained between exchanges: two identical asks
within one admission answer identically, which is the statelessness a
later reader can check, and the claim ends at the admission's edge, a
re-admitted artifact being a new scorer.
The artifact's head defines the label set and the ask carries content
alone, because a per-ask label set would be the asker teaching the model
mid-exchange, which is a different instrument this charter does not hold.
The loop's why never crosses: this crate learns what to classify and never
what the answer is for. A content exceeding the artifact's own bound
refuses typed with the bound named, the overflow discipline of section
13.3 in this submodule's shape.

### 15.5 The record

**Every exchange is recorded by the harness, per the custody rule.** The
classify ask and its answer land as their own event kinds, authored by the
sole writer and distilled through the tee like everything else, because an
unrecorded semantic exchange would be the first in this program and the
precedent is refused. The kinds land in `weaver-trace-PRD` section 3 and
are owed by this act, the demand existing now. Quick searches over custody
are exactly what the recording buys: the labels reach `weaver-state` as
record, never as a side channel.

### 15.6 The faults this submodule raises

The set is 13.10's shape at this submodule's size, each survived by the
worker and authored by the harness as the `fault` event: a device fault
during a forward, and a residency degraded outside any single exchange. An
oversized content is a typed refusal on the exchange rather than a fault,
and a death is observed through closure, both per the contract.
