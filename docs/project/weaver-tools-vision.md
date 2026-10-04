# WeaverTools - Vision

**Status:** LIVING. Outside the document set. This document changes as each stage of
development teaches what the next one looks like. It is never ratified and nothing
is written against it.

**Date started:** 2026-07-28
**Document ID:** `weaver-tools-vision`
**Editorial:** ASCII, no em-dashes, no semicolons.

Companion to `weaver-agent-PRD`. The PRD is design and is checkable against code.
This document is motive and is not. Where the PRD specifies one turn of the
proto-stateful agent, this document holds the arc that turn is the first step
of.

---

## 0. What this document is, and is not

The apex PRD builds a proto-stateful Weaver agent and claims nothing beyond it. That
restraint is deliberate and it is correct. A design document earns its authority by
being falsifiable against the code that fulfills it, and a document that also
carried the long-horizon intent would tangle load-bearing near-term claims with
speculation nobody can check yet. That tangle is how the prior corpus lost its
coherence.

This document holds the intent the PRD sheds. It is the picture of where WeaverTools
goes after stage one, written so the near-term work stays clean by having somewhere
else to put the motive. It is living rather than merged into the set. Its later
stages are named, not dated, because the shape of any stage is not knowable until
the stage before it is most of the way built.

One rule governs the boundary between the two documents. This document speaks in
biology, cortex and brainstem and hippocampus, because the organism model is the
clearest way to say what the architecture is for. The PRD never does. Biology
explains why the architecture is shaped the way it is. It never becomes a spec term.

The registers are three, and the ambition lives in exactly one of them. The
readme stays grounded on engineering principles, with one theory claim conceded
and defended, that latency is the enemy of agency, and agentic-performance claims
stay out of it entirely. The spec corpus carries only what is current work. This
document carries the destination.

## 1. The staged arc

Stage one is the agent that keeps a complete record and no memory: one turn, end to
end, through the gate, against a real local model, emitting a trace worth trusting.

The distinction is worth drawing carefully, and the corpus stopped calling stage
one stateless on 2026-08-01 for exactly this reason: the old name was a
convenience that misled anyone who took it literally. The trace is state. It is a
factual, sequence-faithful account of everything that occurred in a session, held in
RAM while the session runs, and the agent reasons over it, so turn two knows what
turn one did because the trace says so. What stage one lacks is not state. It is
everything that turns a record into a memory: selection, so that what matters is
kept and the rest is not, association, so that recall works by relevance rather than
by position, consolidation, so that many episodes settle into one structure, and
persistence, so that anything the agent can draw on survives the session at all. A
record answers what happened, in order. A memory answers what bears on now.

Something does survive, and saying otherwise would be sloppy. The record the
stream accumulates outlives the process and the session both, on the operator's
side of the sink, per the ruling of 2026-08-01 at `weaver-admin-operator-contract`
section 3. It is written for the operator and for later analysis, the agent has no
path to it, and since the cut of that date nothing in the program reads it back
into a new session at all. So the claim worth making is narrower and stronger than
nothing survives, which a careful reader falsifies by pointing at the operator's
storage. Nothing the agent can draw on survives, and that is why there is nothing
to individuate from, and why stage one tests nothing.

It is the apparatus floor, the Level A substrate of privileged low-latency
connectivity, per-agent OS individuation, and a trace worth trusting, on which the
later work becomes possible.

Later stages add the capacity to accumulate and use experience. They are not
specified here and not committed to an order, because the discipline of this project
is to decide architecture from measurement rather than ahead of it. What can be said
now is the shape of the growth, not its schedule: each new capability arrives as an
organ behind its own socket and contract, and the agent grows from a record toward a
memory rather than being rebuilt for it.

The destination is the test the whole program exists to run. WeaverTools is built to
test whether an agent whose boundary is other-produced can nonetheless self-produce
its individuation through accumulated experience (Bucy, 2026). An agent whose record
ends with its session cannot test this. The staged arc is the path from the floor
that makes the test possible to the agent that is its object of measurement.

## 2. The organism model

The agent is one body, and its parts are organs rather than modules. The framing is
not decoration. It carries a specific commitment: capability is grown when the body
needs it and only then, and the thing that coordinates the growth is written to
spend out new organs without cracking the chest open again.

The SPU, the encode and decode tissue, is the cortex. It is where tokens become
thought and thought becomes tokens. The harness is the brainstem. It keeps the
pulse, coordinates the involuntary function, and holds the loop. Every organ reports
to it, and it is the sole writer of the trace and the sole broker of access to the
trace. That centrality is what lets it grow the body outward: a new organ is a new
socket and a new contract onto the brainstem, not surgery on everything already
there.

The organs that arrive later are filtration and judgment. A classifier or a small
activation network that filters prompts or queries is a kidney or a liver, an organ
that decides what passes and what does not. Where such an organ's judgment runs as
a model, the model's compute resides in the SPU under the same residency accounting
as the decoder, per the umbrella of section 9: the organ is the judgment function,
and the SPU is where its model lives. The memory leg, drey together with the
consolidation pass, is the hippocampus, the organ that turns momentary experience
into lasting memory. None of these exist in stage one, because a body with no
bloodstream needs no kidney, and an agent whose record ends with its session has no
accumulation to settle. The architecture's whole claim is that when the body needs
an organ it grows one, rather than having been built around a slot left empty in
anticipation.

The order in which organs arrive is not arbitrary either, and this is the lesson
the prior attempt paid for by not noticing it. Life carried no persistent memory
long before it carried any, though never without working state: even bacterial
chemotaxis keeps a few seconds of methylation memory to compare conditions now
against a moment ago, the working-state floor the trace is the agent's version of.
Reflex arcs preceded learning. The brainstem is older than the cortex, and the
cortex is older than anything that consolidates experience into lasting structure.
Nothing evolved a hippocampus first and then worked out how to metabolize. Memory
is a late organ everywhere it appears, because memory is a property added to a
working loop rather than a substitute for having one.

The prior attempt built the memory leg and the turn loop at the same time and spent
its final months subtracting one from the other. That was not a resourcing failure,
it was an ordering failure, and the same dependency shows up in both places. An
agent that cannot complete a turn has nothing to remember, and an organ that turns
experience into structure needs experience to work on. The record before the memory
is not a compromise forced by scope. It is the sequence.

## 3. The hardware ceiling

An organism does not grow organs it cannot perfuse. This is the constraint that
keeps the growth model honest rather than open-ended (Bucy, 2026). Because the
substrate of a Weaver agent is the device it runs on, the ceiling on how many organs
an agent can carry is hardware you can budget for rationally. VRAM, compute, and
memory bandwidth are the blood supply. An agent does not sprout an activation
network it cannot fit alongside the cortex, and a deployment that wants more organs
buys more substrate or accepts fewer.

This is not a limitation to engineer around. It is the property that makes the
organism model tractable. A growth model with no ceiling is a wish. A growth model
bounded by a resource you can measure and provision is a plan.

## 4. The protoautonomic path

Autonomic behavior is the harness acting without being asked, and the model seeking
that action without being prompted to. Neither half exists in stage one, and neither
can be conjured by architecture alone. The harness can be built to inject, but the
model has to be trained both to accept injected input in the right circumstances and
to emit a signal when it is reaching for that input. That training needs data, and
the data is what stage one quietly produces.

The mechanism is trace shaping. An ordinary tool call, named and structured with
care, leaves a trace of close to the right shape. The calculator is the
deterministic reference: the harness injects a computed result into the stream in
place of a stochastic one, and the trace of that injection is the seed of the
pattern. A Hades-backed retrieval tool, reached as an ordinary call out through the
gate rather than as an internal store, widens the set from arithmetic to retrieval
shapes. Prompt and context engineering coax these calls into a simulacrum of the
autonomic behavior, close enough that the traces can be sanded into real SFT
examples. The simulacrum comes first, and the trained behavior is built from it.

This is why the calculator in the PRD is named protoautonomic rather than autonomic.
It is step one toward the behavior, tied to it by name, and it makes no claim to be
the finished thing.

## 5. Reinjection and the first judgment organ

A cheap-looking win is available the moment the trace lives in RAM at the token
level: pack past the context limit, clear space, and reinject earlier material from
the working structure. Dumb recycling, keep everything you can, is a harness policy
and nothing more. Selective reinjection is a different animal. The moment something
decides what is worth carrying forward and what to drop, that decision is the seed
of consolidation, and it is the first judgment in the system that is about
relevance rather than order.

So reinjection is not a PRD line. It arrives as its own organ. A small activation
network or a classifier, trained on trace data, watches the trace, pre-caches
portions of it ahead of time, and injects on the right tag.

That organ creates no state. It selects over state the trace already holds, which is
why it can stand on the in-RAM working structure before drey is ever provisioned. It
is the first of the four properties from section 1 to arrive, and it arrives alone.
There is still no association, no consolidation, and nothing the agent can draw on
that survives the session. What changes is that the agent stops carrying its whole
record and starts carrying what bears on now, which is the beginning of a memory and
not yet one.

Two questions are open here and are left open deliberately, because they are
architecture decisions and there is not yet a measurement to decide them from:

- Whether the reinjection organ is a classifier or a small activation network.
- Whether it lands before or after drey.

## 6. Individuation and safety as one primitive

The strongest property of the OS-level substrate is that agent individuation and
agent safety are the same mechanism seen from two sides. The UID that isolates one
agent from another is the same UID that bounds what that agent can touch. The kernel
that keeps two agents out of each other's state is the kernel that keeps a single
agent's tool calls inside their permitted reach.

This is what makes tool safety a solved problem rather than a novel one. An agent
that runs as a constrained Linux user is bounded by filesystem permissions, sudoers,
PAM, and cgroups, the same instruments a system administrator has used for decades
and can audit without learning anything new. Operationalizing agent safety becomes
connecting the agent's identity to LDAP or Active Directory, not inventing a
classifier that adjudicates danger from the model's disposition. The enforceable
constraint lives in the OS, where it can be checked, rather than in a heuristic,
where it can only be hoped for.

The agent's reachable world is its own home directory and nothing more. There is
no architectural reason to grant it any wider access to the host it runs on, and
as standing doctrine it never holds root on that host, for its own integrity and
safety. This completes outward what the harness charter already establishes
inward, where the agent is never told the trace file's name and its tool surface
cannot reach the file. Confinement is not a policy layered on top. It is the OS
trust model the agent inherits as an ordinary user, and the home directory is the
whole of what that user owns.

**Two questions hide inside the word tool and they are independent.** Who initiated
the action, and where it was dispatched. An earlier cut of this section ran them
together as one inside-versus-outside axis, which cannot classify the ordinary case
of a memory lookup that reaches a database over a network, and which the apex already
avoids: `weaver-agent-PRD` section 4 calls the calculator **a model-elected call whose
result the harness supplies deterministically**, and reserves autonomic for
**harness-initiated** action.

**Initiation is the primary axis.** An action is elected when the model emits it as
ordinary output, and autonomic when the loop fires it because a condition the loop
set was met. Nothing about where the work happens enters that judgment, and the
judgment is a fact about the trace rather than a reading: the trace shows who moved
first.

**Dispatch is the second axis and it is a consequence.** The harness routes an
elected action inward to the organ whose domain it belongs to, or outward through the
gate as ordinary output. Autonomic action is dispatched by the loop that fired it,
usually inward, to the organ whose condition was met.

The two axes cross rather than nest, and the corners are all occupied except one:

- **Elected and dispatched inward.** The protoautonomic calculator. The model asks,
  the harness answers deterministically, and nothing crosses the gate.
- **Elected and dispatched outward.** A registered service the agent addresses across
  the gate, the model asking and the gate carrying it opaque as any other output. This
  corner listed a bash tool, a database client, and an API call until the port ruling
  of 2026-08-07, which files a tool by whether it binds a listening port rather than
  by where its bytes go. None of those three binds one, so all three are internal and
  dispatched inward, and what remains here is the service that listens.
- **Autonomic and dispatched inward.** Held state surfacing because the loop's
  condition was met, entering the decoder's path without the model having asked.
- **Autonomic and dispatched outward** is the empty corner. The loop reaching the
  world unprompted is out of scope with the rest of autonomic action.

**Why initiation rather than location.** It classifies the hard case, since a
networked autonomic lookup is autonomic by initiation and unclassifiable by location.
It is checkable, since who moved first is in the record. And it survives the boundary
moving: encapsulate this whole body inside a larger one and every location claim
flips at once, while every initiation claim stays true.

**One consequence is a latency requirement rather than a taxonomy note.** An elected
inward action must not leave the body, because an agent that reasons its way to
asking what it holds should get an answer at close to the speed of the same
state surfacing autonomically. Both ride the organ's own channels. Neither is gate
traffic, and a design that sent elected recall out through the gate and back would be
bounded by the world for an answer that never left the machine.

How dispatch is chartered for the outward case remains the token workflow's question.
Which corner a given capability sits in is no longer a question at all, which is what
changing the axis bought.

**The outward corner, elected and dispatched through the gate, is what the rest of
this section is about.** A tool there is external to the agent and not constitutive
of it, and this program builds none of them: what it owns is the grip they are built
to fit, per `weaver-agent-PRD` section 9. Its call leaves as ordinary model output,
crossing the gate opaque and logged, and the harness owns dispatch on the far side, so
the gate never distinguishes a tool call from any other output. The return is symmetric:
the result re-enters through the gate, opaque and logged, and reaches the model only as
part of the next prompt. Outbound reach to other machines, over SSH or any other means
the agent's credentials permit, is ordinary user capability on the OS trust model rather
than a crate surface, so the rule that no crate exposes a network surface stands
untouched, because nothing listens.

Stated at the level of the program, this is the Level A claim in operational dress:
per-agent OS individuation is not only how agents are told apart, it is how they are
made safe.

## 7. Every organ is a framework for composability

Each core crate is separated out the way it is so that a builder can extend it in
place without touching anything above it. The SPU is separated so new models and
model families drop in under one umbrella, and so decoder, encoder, classifier,
reranker, and small-activation-network operations are distinct concerns managed
under that umbrella rather than one tangled surface. The harness is separated so
new capability connects by a hook the loop reaches, not by surgery on the board.
Admin authorizes and never executes the interior: it verifies what the operator
declared, directs the transition across its one seam, and the harness orchestrates
the loading of the organs the agent's binary carries, against the bindings its
configuration declares. Memory and state are deliberately
not core. Memory arrives later as a built organ with harness hooks the loop
connects to, and it is left out of the core set on purpose, to be the first
demonstration of how a brand new organ is added and connected. That is a feature
of the plan, not a gap.

The bar each organ answers to is the reversibility test. If a builder has to go
the other way inside an organ later, the cost is that organ and nothing above it.
An organ that passes is a composability framework. An organ that fails leaks the
cost outward, and the leak names the boundary that is wrong.

## 8. Loop zero and loop one

Loop zero is the guaranteed reach point and the responsibility the framework
takes. A proper configuration file is the price of admission. Admin authorizes
and provisions against it, the harness orchestrates the interior, and what the
framework delivers is the agent up, model loaded, sitting at loop zero waiting
for instructions. Loop zero is the load and the unload, the same for everyone,
and it never leaves the framework's hands.

Loop one is the builder's. It is whatever the builder decides it is, written
inside loop zero, bounded only by what the harness exposes and what can be
hooked through it. Loop zero is why the agent runs. Loop one is why the agent is
not everyone else's. And loop one is compiled, not loaded: the builder writes
the loop in narrow Rust at the worker's composition root, composes it against
the ports the harness offers, and recompiles. The behavior is immutable in the
binary, and that is a feature rather than a cost. A config immutable at
deployment cannot drift, what the binary declared is what it was, and swapping
the loop is swapping which binary the agent's unit starts. The basic loops the
framework ships are demonstrations of that motion, written by the same path any
builder's loop takes, native in exactly the same way.

## 9. The SPU umbrella, fully grown

The SPU's destination is prehooks standing ready for five operation types:
decoder, encoder, classifier, reranker, and small activation network. Each ships
wired to a representative model so the claim is demonstrated rather than
asserted: a reranker loads, takes traffic, and a loop routes through it end to
end. The demonstration matters because a deployed assistant is not one model
anyone points at. It is a decoder with rerankers and classifiers sitting in
front of it, sorting traffic before the token everyone calls the answer. Showing
all five operation types live is what proves the umbrella is real and not a
decoder with aspirations.

The growth path is incremental by construction. Each operation type is internal
organization of the SPU, piping out its own Unix socket, and every such socket
is the SPU's own: the SPU remains the party to every contract those sockets
carry, the harness is told where a socket is and when to route to it, and the
organ test of the apex is untouched because the sockets multiply while the organ
does not. Adding an operation type is a bounded job: write it, pipe it out its
own socket, tell the harness where and when. A loop that does not call an
operation up simply does not use it. Decoder and encoder are split precisely
because the many decoder families need more ongoing support than the fewer
encoder and reranker families, and the split keeps add and subtract clean.

## 10. The builder's end state

A builder edits one directory. They write their loop where the scaffolding already
reaches, point at a model the way any config-driven loader points at one, add the state
they need, and plug in at the harness. Narrow Rust and a recompile is the expected cost
today, and pointing at a safetensors file with a plain declaration is the aim, a
framework question the SPU's own charter reserves for its Spec with candle as the
illustrative candidate rather than a commitment. When that level is reached and the
visibility inside the model is standing, that is where the payoff arrives.

Every knob in the builder's assembly carries a disposition, and the disposition
is the builder's election at the composition root: frozen, the value baked into
the binary, or operator-tunable, the value routed from the agent's configuration
at load. Seed and temperature are the reference cases. Freeze them and the
binary declares a re-enterable starting field, leave them tunable and the
operator elects per load, and either way the trace records the effective values,
because a disposition changes who sets a value and never whether it is recorded.

The point of all of it is variance held to a predictable range. The engine stays
stochastic on purpose, every condition around it is frozen or declared, and so
the variance that remains is attributable to the thing under study. An immutable
binary makes repeated runs a sample of a characterizable distribution, which is
what turns agent behavior into an object of measurement rather than anecdote.
This is the payoff half of the readme's substrate claim that nothing varies that
we did not set. The basic loops and the memory-organ demonstration are what gets
published upstream to show exactly that motion: build what you need, not the
scaffolding, because the scaffolding is what shipped.

## 11. What this document does not commit

The value of this document depends on it not hardening into design. It names organs
that do not exist, an order of stages it does not fix, and questions it does not
answer. That is the point. A vision that committed to the classifier over the
activation network, or fixed drey before the reinjection organ, would be guessing at
designs that have not been written, which is the exact reserved-slot move the PRD
forbids.

**One disagreement is owed a settlement here.** The previous tree's autonomic-memory
PRD proscribes decoder-invoked memory tools outright, arguing that retrieval-as-tool
spends the decoder's budget on retrieval machinery rather than on reasoning. Section
4 of this document has the retrieval tool as an ordinary call out through the tool
surface, which is the pattern that PRD rejects. Both positions are defensible and
this document is the newer thinking, so this is where the disagreement is settled
rather than inherited. It is named here rather than left in a working list because a
working list is never ratified and shrinks toward empty.

So this document grows as the work teaches it. Each stage that completes turns one
of its open questions into a settled decision, and that decision moves into a PRD and
out of here. The vision shrinks toward the built system as the built system grows
toward the vision. When they meet, this document has done its job.

## 12. The center and the shell

Added 2026-08-11, on the ratification of the reasoning-loop criterion. The agent
divides at a seam the stripping method exposed rather than any plan drew, and the
division gives the road its shape. The reasoning loop is the center: what directly
enhances the processing of meaning, built first because nothing else can be built
without it. Around it is the shell, still the agent: tools including the local
shell, durable identity written through admin, the corrective mechanisms that may
come. The shell earns its place against its own bar, and center-first means
built-first, never matters-more.

The loop's edge is the gate, and the agent's edge is the network boundary: what
crosses a network line is outside the agent entirely, for now and by ruling. The
ruling reads network lines alone, and a local Unix socket crosses no boundary
that counts, which is how a tool sits one socket away and still inside the
agent. That crossing is what the crossings-per-decode-step measure prices as
nearly free, so the loop calls its shell without ceremony and contains none of
it.

The reasoning loop nearing completion does not complete the agent. It is a road
marker, the first one that runs, and the criterion that bounded it is the same
criterion that will admit memory and perception when their acts come. What stands
at the center today is small, proven, and deliberately incomplete, which is the
correct state for a center.

## 13. The suite around the organism

Added 2026-08-19, on the operator's direction as the framework crossed into
use. The apex deliverable stands and is exceeded: the agent completes turns
against a real model, keeps a trustworthy record, and knows its own past
within a session. With that, the seat shifts. The framework's first consumer
class was the developer, and the work rightly faced them. Using the framework
raises the development purpose one level, to the operator and the user, and
what they meet is not a crate but a suite.

**The suite moves up a level**, on the operator's ruling of 2026-09-27. **WeaverTools**
names the suite and no longer this repository: HADES, weaver-agent, weaver-web,
weaver-analysis, and whatever joins them. This repository is **WeaverAgent**, the name
of the domain it holds, one agent, so the repository and its domain root carry one name
and no second spelling stands one letter from it.

**The ruling of 2026-08-23 that retired the rebrand is overturned**, and its reason is
recorded here rather than deleted. It held because the repository was the suite. That
stopped being true when weaver-web left the tree on 2026-09-26 and HADES was counted a
member, being a repository of its own. One tree had also become a limit on the work, the
frontend waiting on the backend because both moved through one queue.

**The rename followed the extraction.** weaver-web left on 2026-09-26 and weaver-analysis
on 2026-09-30, each to its own repository, and the agent's repository took its one name
when the second had gone.

**The agent keeps two doors and gains no others.** weaver-gate carries the data plane
and weaver-admin the management plane, at `weaver-gate-world-contract` and
`weaver-admin-operator-contract`. Each door has a connector beside it, `gate-con` beside
the gate and `admin-con` beside admin, the names the 2026-10-02 ruling gives
(toddwbucy/WeaverTools#6). **Nothing dials a connector.** Each is a client of its door's
socket on one side and a client of the WeaverWeb server on the other, dialing out to it
over mutual TLS, so the agent listens to nothing new and the connectors listen to
nothing at all. They are one agent's own, bound by certificate to that agent and one
plane, run as their own unprivileged service users, and start under the operator's
provisioning, never under admin. Their code is WeaverWeb's, in its repository or their
own and never in this one, and an agent works with no connectors at all.

**This repository stops at its two doors.** Each door's contract is its interface, and
that interface is what the repository publishes. weaver-web is its own repository and
builds its connectors against those contracts and nothing deeper, so frontend and backend
move at once from the day a contract merges, the frontend against a stub that answers it.
A change to a door's contract changes what an outside team builds against, and it moves
with the care a contract takes.

**Three gates stand between a person and a verb, and each is its own party's.**
WeaverWeb authenticates the person and decides, in its own IAM, what that person may ask
of which agent, and it records the person in its own audit. admin-con maps WeaverWeb's
abstract verb to one of the agent's fixed `weaver-admin <verb> <agent>` command lines,
so no command crosses a wire, and runs it with `sudo -n` through a strict sudo rule the
operator installs as the box's own boundary: it names the connector's own service user,
grants only that agent's lines its role allows, and takes nothing from the caller, no
argument, no environment and no standard input ("the box decides", weaver-admin-Spec
section 2). The connector's ceiling is the rule's lines, which it reads with `sudo -n
-l` and never writes. The observer role's rule grants `show`, and the operator role's
adds `validate`, `load`, `unload` and `stop`. A compromised server or connector reaches
no further than the lines that rule grants, at most starting and stopping its one agent.
**The person never reaches admin**: where the connector runs a verb, the cause the
agent's record carries is the uid sudo reports, the connector's, and which person asked
stays WeaverWeb's record. gate-con carries no person at all, the gate admitting it like
any client, by its kernel-checked uid or group against the declaration's allow-list
(`allowed-uids`, `allowed-gids`). For a file sink, admin-con reads the trace through the
trace relay admin's start step stands beside it, which admits exactly the one reader the
agent's `roles.toml` declares, the connector's user, rather than through any grant into
the territory. **The agent fails closed with its connector** where WeaverWeb's packaging
gives the connector a containment of its own: sudo opens no session for it
(`!pam_session`), so an agent it loads lives in that containment, and stopping the
connector stops its agent (the operator contract section 2, toddwbucy/WeaverWeb#15).

**The door contracts carry no principal, and the amendment once predicted here is
withdrawn.** An earlier form of this section had each connector pass a verified
principal across its door and both door contracts amended to carry it. The three gates
make that unnecessary: authorization at the door rests on the connector's own service
user and the lines its sudo rule grants, the person's identity stays WeaverWeb's, and
nothing of the person enters the agent, the cause it records being the uid sudo reports.
The connectors build against the two external contracts the ruling of 2026-08-01 wrote,
`weaver-gate-world-contract` and `weaver-admin-operator-contract`, as #50 extends the
second. weaver-analysis reaches the record the same way an operator does, consuming a
trace an operator holds and never touching weights.

**A post-training leg is a direction and not a consumer.** The clause that kept
the native path alive is what it would stand on, a program that intends training
as a continuation being unable to let that path decay, but nothing is chartered
and no door is owed. And the
gaps a real frontend will meet each have a named front door, though not
one shape of door: streaming and a status ask enter by amending the two
external contracts in docs acts, the operator's read on state enters
through `weaver-state-PRD` section 5's named cell for the day such a
reader exists, and a new ask between harness and state has
`weaver-harness-state-contract` section 7's change protocol, which today
governs only that seam's own vocabulary. That is the inversion this
section records: needs discovered in use pull framework acts through
front doors, where the roadmap once pushed.

One vocabulary line is held hard, because collapsing it is how the prior tree
drifted. **weaver-store is a commons between agents and is not memory.** It
stands outside every agent, fleet-level, on the world side of the loop's
membrane, and an agent reaches it the way an agent reaches anything in the
world: through the gate, as a tool. The agent's own memory - drey, the
hippocampus, the consolidation pass - remains in-agent, arriving through apex
section 9's door as an organ with its own socket and contract, on the
schedule section 2's ordering argues. The terms, held exactly: the record
and the working state distilled from it are agent state, in-agent and
per-agent. A memory is the agent's own late organ. A commons is the
fleet's, outside every agent. Agent state, memory, and commons are three
things, and the suite works only while each stays itself.

The discipline that binds the suite is the one the state leg just proved end
to end: satellites are consumers or contract peers, never retrofits. Nothing
reaches into the organism because a product wants it to. What a satellite
needs, it asks for, and the ask lands as a charter and contract act or it
does not land.
