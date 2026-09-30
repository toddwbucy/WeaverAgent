# Sketch: the two use cases

**Status:** SKETCH, v0.1, 2026-08-23, filed 2026-08-24. Decides nothing and authors
no Spec. Feeds PRD authoring.

**Placement, which answers this sketch's own last open cell.** It sits in
`docs/project/` because it is suite-level: it is about what the program is for
rather than about any crate. It is a sketch rather than a section of a suite PRD
because there is no suite PRD, deliberately, per the operator's ruling of 2026-08-24
that the architectural pattern is worth a rough draft and the scope's details are not
while those details are still moving. **It feeds that rough draft when it is written.**

**Section 5 does not cross the publish boundary.** It carries commercial framing,
customer segmentation, a competitive comparison, and the single-operator against
multi-tenant distinction, all of which the workshop's standing rule keeps out of
anything destined for publication. This repository is private, so filing it here
publishes nothing. **What it must not do is travel into `docs/technical/` or into a
released PRD when those are drafted from these sources**, and the marker at that
section says so where an author will meet it.

**Document ID:** `sketch-the-two-use-cases`
**Editorial:** Per the Working Rules.

---

## 1. Purpose

WeaverTools serves two use cases and only one of them requires anything not yet
built. The corpus documents the components thoroughly and has never stated the two
side by side, so a reader arriving at the framework sees the use case that is gated
on a memory substrate that does not exist and misses the one that is available now
and applies to agents deployed today. This sketch separates them so that the
documentation can carry both, and so that the near one stops being read as a
staging area for the far one.

The near use case is diagnostic. Take an agent that runs on a network, bring it
down to one box, remove transport as a variable, solve the semantic problem, then
redeploy. The far use case is the individuated agent with substrate-resident state,
which waits on the memory substrate. The near one is not a lesser version of the
far one. It is a different job that the same architecture happens to do.

## 2. What this is not

**This is not the framework and deployment split.** That split is about who holds
the code, an open experimentation framework against a private derivation built for
one customer. This split is about what an agent is made of and what it is being
used for. The two axes cross rather than align, and a proto-stateful agent can be
run in either the framework or a derivation of it.

**This is not an argument against networked agents.** The claim is about ordering
rather than destination. An agent that works locally and is then distributed is a
different object from an agent that was distributed before it worked, and the
difference is that the first one existed before the network was involved.

**This is not a maturity ladder.** Use case two does not retire use case one. A
developer with a fully stateful agent still has reason to collapse it onto one box
when something breaks, and the diagnostic value of doing so does not decrease as
the agent gets more state.

## 3. Use case one: the proto-stateful agent

A proto-stateful agent holds real state within a session and none across sessions,
per `weaver-agents-PRD` section 2. Two things persist across turns inside a session,
the working structure the agent reasons over and the hot KV cache, and what the agent
begins each session without is accumulated experience: no memory substrate of any
kind, which is the whole of what this program defers. That is what WeaverTools
currently holds, which is the weaver-agents domain.

**The agents deployed on the internet right now are the narrower thing.** They carry
no state outside the prompt, and what looks like memory is rebuilt from scratch every
turn and recomputed through the decoder each time. An earlier vocabulary called our
own stage stateless as well, which the ruling of 2026-08-01 retired as an
overstatement. The distinction earns its place here because use case one is about
standing in for that narrower behavior rather than about sharing it.

Nothing in the architecture prevents this from being reproduced faithfully. The loop
decides what the prompt contains at each position, so a loop written to discard the
working structure and rebuild it every turn is a supported configuration rather than
a degraded one. The same latitude extends to the network itself. A loop can hold
sleeps at the seams and reproduce the latency profile of a distributed deployment on
one machine. There is limited reason to want that, and the point is that the
architecture does not forbid it, which is the property that makes the local host a
faithful stand-in rather than an approximation.

The diagnostic move follows from that. A networked agent is failing and the failure
is diffuse. Collapse it onto one box. Transport is now absent by construction rather
than merely believed to be innocent. Work the logic and the semantics until the
agent behaves. Redeploy to the network. Anything that breaks after redeployment is a
transport problem, because everything else was settled while transport was not in
the picture. That is the elimination argument, and section 7 states what it costs.

There is a second effect that is not diagnostic and is worth naming because it is
what a developer notices first. Stripping the network out leaves standing whatever
part of the codebase was about agent behavior. Most working agent code is not agent
code. It is retry handling, serialization, timeout policy, and connection state, and
the ratio is invisible until the transport layer is gone and the remainder is small.

## 4. Use case two: the stateful agent

The second use case begins when the agent has a memory substrate, which is state
that lives outside the prompt and persists across the load boundary. That substrate
is not built. The proto-stateful work has to finish first, and the ordering is a
build constraint rather than a conceptual one.

Adding the substrate does not make the agent local. How a stateful agent is
distributed stays an operator decision, and the decision has real content once the
components have different appetites, since decode wants the accelerators and the
memory substrate wants something else. What does not change is the ordering. Solve
it locally, then distribute it. The substrate widens what has to be solved locally
before that step, and it does not move the step.

## 5. Why either one is worth the cost

> **This section does not cross the publish boundary.** It carries commercial
> framing, customer segmentation, a competitive comparison, and the single-operator
> against multi-tenant distinction. **It does not travel into `docs/technical/`, into
> a released PRD, or into the repository's README.** The architectural claims it
> rests on do travel, stated without the market reasoning around them.

Both use cases carry an adoption cost, and it is the same cost. WeaverTools requires
its own serving path, which means giving up vLLM and the managed serving layer built
around it. That looks arbitrary until the reason is stated, and the reason is the
motivation for the whole program.

Continuous batching requires that requests be fungible. A server that interleaves
many requests cannot let any one of them own anything, because ownership is what
makes a request non-interchangeable. State therefore has nowhere to live except the
prompt, which is rebuilt each turn. This is correct engineering for the product
being sold, which is a decoder offered as a service, and the customers for that
product are web application developers who have been trained for fifteen years to
treat state as a hazard to be pushed out of the service layer. Nobody chose
statelessness for agents. The product shape required fungibility, fungibility
required batching, and batching forbade state. Web application developers inherited
the constraint and read it as a property of language models.

Individuation is non-fungibility, so the two cannot coexist. The customer this
matters to is the one with proprietary data that cannot leave the building and who
wants an agent that accumulates state on that data over time. That customer was
never trying to serve a thousand concurrent strangers, so the throughput property
being surrendered was never one they needed, and the property being purchased is the
only one that does what they are asking for. Market selection and architecture are
doing the same work here.

**The comparison case is Palantir.** Their published Ontology material describes
statefulness held in a governed semantic layer above the data, with marking-based,
purpose-based, and role-based policies affixed across it and lineage flowing across
data, logic, action, and application artifacts. Underneath runs a zero-trust
Kubernetes runtime that drains and replaces compute nodes on a fixed cycle. The
design is elegant and it discharges a real audit obligation, which is what a
regulator recognizes as state, meaning the sensitive data and its lineage.

Two observations follow and neither is a claim of superiority. The first is that
enforcement lands at the kernel in both architectures, through hooks on the physical
machines the cluster runs on, so the trust root is the same one WeaverTools inherits
directly. The difference is the number of layers between that enforcement point and
the decoder, and the layers buy deployment flexibility rather than a stronger trust
boundary. The second is a definitional gap. What their controls govern is retrieval,
which is a proxy for the thing being controlled, since data is sensitive
because someone wishes to restrict who may process it. Control at the point of
execution is one step below where their boundary sits, and the assembled context at
the moment of the forward pass is where that step happens.

**The trade is real and runs both ways.** Node ephemerality is a containment
property, and a persistent agent is by construction a persistence mechanism. Making
the substrate durable reintroduces exactly what disposable compute exists to deny,
and the compromise of a durable agent stays. That is the price of reaching the point
of execution, and it should be stated rather than argued around.

What is bought for it is reconstruction. Measured against a standard deployed agent
trace, the reproducible record runs about 1.8 times the size with diagnostics off,
because the token identifiers are a second encoding of text the record already
holds. That duplication is the evidence rather than waste, since the pairing of text
to identifiers is what witnesses that the declared tokenizer is the one that ran, and
a third party holding the named tokenizer can check it offline without the operator's
infrastructure. Diagnostics on runs closer to seven times, and that increment is
measurement rather than reproducibility and can be dropped without losing replay. No
float-valued material from inside the model is recorded at all. The retention
question is therefore a storage tier and a retention window rather than an
architectural decision, which is a conversation a compliance officer already knows
how to have.

## 6. What makes both possible

The hub and spoke shape with a named contract at every seam. Each organ speaks to
the harness through a contract rather than to a neighbor through an implementation,
so the transport under any one seam is a substitutable detail rather than a
structural commitment. That is what allows an organ to be relocated to a network
substrate without the agent logic above it changing, and it is what allows the
reverse move of collapsing a distributed agent onto one host.

This was designed in and has never been stated as a property, because stating it
during the local build would have pulled attention toward the network problem
before the local one was solved. The demotion of the old WeaverTools PRD to the
weaver-agents domain is the same widening reaching the document set. **The sketch as
drafted paired that demotion with authoring a new apex above it, and the operator's
ruling of the following day went the other way**: the suite has no apex on purpose
while the scope is still moving, which is why this sketch is a sketch.

**The contract layer went transport-silent on 2026-08-24**, and the accurate statement
of what that sweep did is narrower than the property this section wants. It removed
the named mechanisms of descriptor passing and process creation from the contracts.
**The enumerated case G2 gives is clean, and the word socket survives anyway.**
`weaver-harness-spu-decode-contract` and `weaver-harness-spu-classify-contract` both
require one write to produce one message and both leave which socket type supplies
that write to the Spec's election. Those are the only two sites in the layer that say
socket type at all, and they say it in order to defer it, which is the rule working
exactly as written. **No contract names a specific type**, checked this date across
the whole layer.

What survives is the bare noun, naming an endpoint that gets bound and dialed rather
than a mechanism that carries a message. Whether G2's enumeration reaches that use is
the open scope question rather than a settled violation, and an earlier reading of
this paragraph called it residue the rule condemns without checking. G2 carries the
test going forward.
The relocation property is therefore claimed by design and not yet demonstrated by the
document set, and section 7 opens it as a cell rather than asserting it.

## 7. Open cells

Each awaits a measurement rather than an argument, with one exception kept at the
end: the documentation cell is answered, and it stays as the record of the question.

**Seam relocatability.** Whether the named contracts are sufficient for an organ to
move across a process or host boundary without change above the contract. The
measurement is to relocate one organ and count what changed outside it, which should
be nothing, and the count is the finding either way. This is the cell the whole
sketch rests on and it is buildable against what exists now.

**Simulation fidelity.** Whether a locally simulated network reproduces the failure
modes of a real one closely enough that a fix transfers. The measurement is to take
an agent with a known networked failure, reproduce the failure locally, repair it,
redeploy, and check whether the failure is gone. A repair that does not survive
redeployment says the simulation was not the thing being simulated.

**The elimination argument's cost.** Attributing post-deployment failures to
transport assumes the local pass was complete. The measurement is the rate at which
locally clean agents fail on redeployment for reasons that turn out not to be
transport, and until that rate is known the argument is a heuristic rather than an
elimination.

**The network to logic ratio.** The claim that most of a working agent codebase is
transport handling is currently anecdote shaped and has no instance behind it. The
measurement is a real port, counting modules or lines that survive the collapse
against those that do not. One measured instance is worth more here than the
generality, and the generality should not be published without one.

**The trace size multipliers.** The standard-agent baseline they are measured
against was constructed as a subset of the WeaverTools trace rather than exported
from a deployed vendor tool, so it is the right shape and not a like-for-like. This
is the second constructed baseline in the corpus and the first one did work despite
its disclaimer. The measurement is one real export of one real run from a deployed
tracing tool, and until it exists the multipliers should not appear in anything
published. Separately, the figures should be reported compressed as well as raw,
since a retention conversation is about bytes at rest and integer streams compress
well.

**Detection of a compromised agent.** Durable substrate is a persistence mechanism
and the containment property that ephemeral compute provides is being surrendered
deliberately. Prevention is not the question. The question is what shows a
compromise, and the trace is the only artifact that would. The measurement is
whether a known tampering is visible in the record, which requires deciding what
tampering means for accumulated state before it can be run.

**Where the two use cases enter the documentation.** Answered at the head of this
sketch and left here as the record of the question: they enter as this sketch, and
they reach a rough-draft suite PRD when the scope settles enough to write one.
Section 5 does not travel with them.
