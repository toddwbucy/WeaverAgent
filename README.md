# WeaverTools

**Context engineering is the practice of making context salient at the
moment of decision for the model.**

A local-first agent framework whose primary artifact is the trace: one agent, one
machine, every organ behind its own socket, and one record of everything that
crossed between them.

## Purpose

We built WeaverTools to answer a practical frustration. Our agents do not behave
the way we need them to, and when they fail we cannot see why. The failure is not
the model's alone and it is not the scaffolding's alone. It happens somewhere in
the traffic between them, and that traffic is exactly what conventional setups
leave unobservable. So we built a framework whose first commitment is visibility
into that traffic, on one machine, where nothing crosses a boundary we cannot
watch and nothing varies that we did not set.

The unit of concern is the agent, not the model. The model is one organ among
several, and how an agent meets the world is a property of the whole assembly,
the memory it carries, the tools it reaches for, the loop that sequences its
work, not of the decoder in isolation. WeaverTools makes the agent central by
giving every organ its own place and connecting them through a single
content-neutral switchboard, the harness, which holds the sockets, routes
between organs, and holds no opinion about the content it routes. That is the
whole architecture, a 1920s operator's board, and every organ, including the
memory we build ourselves, plugs into it as an equal caller. If you have a
better idea for how memory should work, connect it. The board does not care
whose voice is on the line.

Making the agent central does not mean giving up sight of the model. It means
the opposite. To get the agent behavior we want, we have to understand why the
model emits what it does and why it fails, and that demands real insight into
the model's internals, captured locally, at conditions fixed before the run. The
architecture keeps that instrument where physics already puts it, inside the
organ that holds the model, while keeping the agent as the thing under study.
Local, legible, and honest about the difference between the model and the agent
it serves.

## Bring your own organs

The switchboard framing is the offer, not a modesty. Looked at squarely, there
is no reason anyone should have to choose our eventual memory system over
someone else's, and the architecture is built so nobody ever has to. Our memory
arrives as one more peer on the board, behind the same kind of socket and the
same kind of contract as anything you would write yourself. Yank it and put
something else in. That flexibility is measured in time: drop your own memory
or state system in, spend an afternoon on the code, and you are experimenting
before dinner, instead of spending a week writing one more Python script to
test a single idea. The same goes for the loops: they are held as workflow
documents, not baked into the switchboard, and an agent running your loop is as
native as one running ours. The model sits behind the SPU for the same reason.
Swapping the decoder is a change to the agent's configuration, not surgery on
the framework, and a second model in the assembly, an embedder, a classifier,
an instrument of your own, is another port turned on and a contract written for
what crosses it, with the traffic routed the same way all traffic is routed.

The same offer runs outward, and there it is smaller than you might expect. We
build no tools. A bash tool, a database client, an API caller are not crates
here and never will be, because a hammer is not installed into a carpenter, it
is shaped so a hand can grip it. What the framework owns is the grip: the
boundary contract a tool is built to fit. Which grip a tool fits is settled by
whether it binds a listening port: one that does is external and reaches the
agent through the gate, and one that does not is internal, running inside the
loop the operator builds. That is why external tooling can stay
external at no cost to the agent, and why the thing on the far side of that
contract can change without the agent changing at all.

What WeaverTools offers, then, is not a better memory or a better loop. It is
the framework in which you build your own agent, locally, in a controlled
environment: the same mental model as the network, abstracted down one level so
that the noise in the wire is something you set rather than something you
tolerate.

## Why not the network?

Every seam that crosses a process line in WeaverTools is a Unix domain socket on
a single machine. The first question any engineer asks is why not localhost, and
the answer has three parts, each of which stands on its own.

Latency. We cop to one piece of theory here: latency is the enemy of agency. An
agent routes through the harness on every exchange, and any per-hop cost
compounds directly into the loop, per token, at batch one. Loopback still pays
the TCP stack, the kernel network path, and serialization on every one of those
hops. Unix domain sockets collapse that cost to nearly nothing while keeping the
same topology. The agent that pauses is the agent that behaves worse, so the
transport with the least overhead is not an optimization, it is a behavioral
requirement.

Security. A Unix socket inherits the operating system's trust model:
SO_PEERCRED, filesystem permissions, kernel-enforced identity. Localhost
inherits the network's, which means adding authentication above a surface that
is, by construction, reachable as a network port. We went OS-level precisely so
that trust belongs to the kernel and no organ exposes a network surface to
defend. Loopback would reintroduce the exact attack surface the architecture was
shaped to eliminate.

Measurement. We wanted network variability out of the equation completely, and
this stands by itself on engineering grounds. Even on localhost you are adding
compute and process that must be accounted for when you are trying to isolate
one slice of behavior. When the object of study is a stochastic engine, every
component between you and it is noise in your instrument. The simpler the
substrate, the cleaner the reading.

We give things up for this. No fungibility across machines, no distributed
uptime, no direct network applicability of the deployed whole. We take that
trade deliberately, because what remains applicable to the network is not the
whole system, it is the individual organs and their interactions, observed with
the network out of the way. The topology is network-shaped on purpose: a hub and
spoke of content-neutral duplex channels is the same architecture a network
uses, moved onto a quieter substrate. Work proven at this level carries upward
because nothing was ever coded to the substrate, only to the topology. The seam
that runs over a socket today can run over a wire tomorrow, and moving one seam
while pinning everything else is exactly the controlled comparison this
framework exists to make possible.

## The trace is the artifact

The deliverable is a deployable proto-stateful agent that completes a turn end
to end against a real local model and emits a clean, turn-bracketed,
correctly-custodied trace. The trace is not a log kept beside the work. It is
the substrate the organs coordinate over: every component reports what it did,
the harness authors those reports into one record, and the record leaves the
program as a single ordered stream, one event per line, in the same canonical
form the agent itself reasons over. Correctly custodied means the agent cannot
reach its own record. The stream lands behind a boundary the kernel enforces,
so the account of what an agent did is never something that agent could edit.

The record is built to be replayed. Because it holds the sampler's actual
tokens rather than a seed, a recorded scenario can be fed back through the
forward pass with nothing re-sampled, and when deeper visibility is needed an
agent can be reloaded with residual readout enabled by a change to its
configuration alone, per-layer activations reduced in place at production time.
Observation is a deployment decision, not a rebuild.

## Why the name

Warren Weaver, introducing Shannon's mathematical theory of communication in
1949, drew three levels at which communication can fail. Level A is technical:
were the symbols transmitted accurately. Level B is semantic: did the symbols
carry the meaning intended. Level C is effectiveness: did the received meaning
change conduct the way the sender needed. Those three levels are this program's
diagnostic and architectural inspiration, and the architecture is the frame
made structural. The substrate pins Level A, one machine, kernel-enforced
identity, boundaries preserved on every seam, so transmission stops being a
variable. The trace captures Level B, every symbol that crossed every seam, in
order, in one canonical form. What remains is Level C, whether the agent
behaves the way we need it to, and that is the question this apparatus exists
to make answerable rather than guessable. When an agent fails, the useful
question is which level it failed at, and a conventional setup cannot ask that
question, because its levels are smeared across a network nobody controls.

The big foundation labs have tooling of this class and keep it in house. The
rest of us, working on the outside, have had to improvise ours one script at a
time. That is the thing WeaverTools is built to change.

## What is here today

This repository runs a strict order of work: documents first, then a knowledge
graph built from those documents, then code. The first two phases are complete:
`main` holds the document corpus - a charter per crate, a contract for every
seam, a Spec per crate carrying its assertion records, the workflow documents
that read the set back as one motion, and the process documents the corpus
governs itself by - and the graph has been built from that corpus, with the set
ratified on 2026-08-04. Ratification is mechanical: the documents carry their
nodes and edges in a fixed notation, and the graph is generated from them
rather than maintained beside them. Code has since been built against it. On the
operator's ruling of 2026-09-27 the graph and its upkeep are deferred to release,
and until then review against the Spec sections a pull request names is the
conformance check.

## Reading the corpus

- `process/WeaverTools-Working-Process.md` is the boot prompt: who is primary,
  in what order the work moves, and where it currently sits.
- `docs/crates/weaver-agents-PRD.md` is the apex: the deliverable, the five
  invariants, the lifecycle, and the enforcement posture every other document
  answers to. It sits beside the crate directories rather than inside one,
  being the domain's own charter.
- `docs/project/weaver-tools-vision.md` is the longer line the program is on.
- `docs/crates/` holds one directory per crate, each carrying that crate's PRD
  and Spec, with the contracts under `docs/crates/contracts/` and the workflow
  documents under `docs/crates/weaver-harness/Loops/`.
