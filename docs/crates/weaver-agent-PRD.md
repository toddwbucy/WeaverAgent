# weaver-agent - Domain PRD

**Status:** MERGED 2026-07-28. RATIFIED 2026-08-04 in the set-wide act, per the
operator's ruling of that date: the whole set mapped into the graph on the HADES
server and ratifies as the complete document set for the toolless inference
deliverable. Section 0's system record carries the set-level mark, which records that
founding act.

**Ratification has been per-charter since 2026-08-23**, per the operator's ruling of
that date and Working Process section 2 as amended. A charter ratifies on its own by
conforming to the pattern the 2026-08-04 act established, so the set is whatever the
charters currently say. The set-wide form was a requirement of its moment, when
nothing yet existed to be consistent with, rather than a standing obligation.

**Date filed:** 2026-07-28
**Document ID:** `weaver-agent-PRD`, renamed from WeaverAgents / weaver-agents-PRD on
2026-10-02 (operator's ruling, WeaverTools#6)
**Editorial:** Per the Working Rules.
**Landing PR:** #713

---

## 0. What this document is

**This is the apex of the weaver-agent domain, not of the suite.** Every crate PRD
inside that domain refines it, every Spec refines a crate PRD, and every contract is
derived from the PRDs of its parties. Coherence across the domain is checked by
reading each document against this one rather than by reading pairs against each
other.

**The correction of 2026-08-23, recorded because the scaffolding is what needs
explaining.** This document was written as the apex of everything and named for the
suite, which was right while the agent framework was the only thing there was to
govern. It is not right now. Its five invariants are agent-domain invariants: they
speak of organs, sockets, the hub, and the turn, and **a crate outside the agent
boundary refines none of them.** A consumer holds no organ, holds no two-initiator
channel with the harness, and stands in no position the invariants describe. So the
document governs a domain, and the name it carried claimed more.

The domain it governs holds ten crate PRDs:

```
weaver-admin    weaver-harness   weaver-spu      weaver-gate
weaver-trace    weaver-traits    weaver-types    weaver-state
weaver-internal weaver-diagnostic
```

**`weaver-state` and `weaver-internal` joined the roster on 2026-08-23.** Both were
chartered 2026-08-18 and both are ratified on their own, per the per-charter rule of
2026-08-23. The enumeration lagged them and that act closed the lag.

**`weaver-diagnostic` joins it 2026-08-24**, on the operator's ruling of that
date, which moved it inside as the harness's third member, the mechanism the
harness authors a diagnostic-trace through, so it enters this roster by the
rule below rather than by an exception to it. Its charter was filed the same
date, on a ruling of 2026-08-23 that chartered it as a consumer outside the
boundary, so the ruling and the document are a day apart and neither is the
other. `weaver-analysis` was chartered
in the same act and does not enter, holding the position outside that
`weaver-diagnostic` vacated.

**Crates outside the agent boundary do not enter this roster and do not refine this
document.** They parent to the suite. A serving binding is reached only across the two
external contracts of 2026-08-01, and that boundary does not move. A diagnostic
binding adds one seam, the preload of `weaver-analysis-state-contract`, dialed by
`weaver-analysis` from outside, per the operator's rulings of 2026-08-24: the
ruled seam reaches the state member alone,
checked against the operator principal, and exists only where the load declared the
diagnostic kind, so the serving membrane is exactly what it was. What governs outside
crates at the suite level is not yet written, and naming that gap is more honest than
widening this document to cover it.

```graph
node: WeaverTools
kind: system
tag: ratified
```

**The graph does not yet carry the domain level this document now names, and that is
stated rather than quietly deferred.** `WeaverTools` remains the one `system` node, the
seven domain roots parent to it, and `weaver-trace`, `weaver-state`, and
`weaver-diagnostic` parent to `weaver-harness` as members. There is no `weaver-agent`
node.

Inserting one needs a decision this document cannot take alone. Document Format
section 3 declares exactly one `system` node, gives `crate` to "every crate,
whether a domain root or a member of one," and carries no kind for a level between
them. It also holds that the parent edges "reproduce the directory tree exactly,"
and `crates/` has no `weaver-agent/` directory to reproduce. **So the level lands
either as a Format act adding a kind, or with the crate tree gaining the
directory,** and until one of those happens the graph answers the older shape while
this document answers the newer one. A reader meeting both should trust the graph
about edges and this document about scope.

**Two different parents are in play and the words for them have collided.** Every
crate PRD in the domain declares a **document parent** in the `Parent:` header it
carries, the domain roots naming this document and the members naming their root's
charter. That is a fact about the corpus and it is true today.

**The graph parent edge is a different relation and it is unchanged**, pointing at
the `WeaverTools` system node exactly as described above. Nothing in this document
moves an edge. A reader meeting the header and the edge should not read them as the
same claim while the domain level is unbuilt.

**Document Format section 3 holds that the header and the edge name the same thing,
and today they do not.** That is a real disagreement rather than a wording
preference, and the Format governs it: it says the edge governs and a disagreement
is a defect in the header. **The header is not corrected here**, because correcting
it would point every crate PRD back at a document that no longer governs the whole
set, and the disagreement closes when the domain level lands rather than by moving
the headers twice. It is named as owed.

This document declares no edge of its own beyond the system node above, because a
child declares its own parent and an apex holding a copy of the whole crate graph is
a topology document under another name.

**The founding set was written together, as one act, and merged together.** That
was the property that mattered then, and it is a fact about that act rather than a
standing requirement: crates chartered since have landed one at a time, and the
register carries what each owes. A corpus assembled from documents written weeks apart
encodes a different understanding of the system in each document, and the
contradictions that result are not mistakes anyone made - they are what
happens when documents from different moments are read as a single body.

## 1. The deliverable

A deployable **proto-stateful agent** that completes a turn end to end against a
real local model and emits a clean, turn-bracketed, correctly-custodied trace.

The trace is the primary artifact. It is not a log of what the harness did. It
is the substrate the crates coordinate over: every component reports what it
did, the harness authors those reports into one record, and the per-turn root
span is the frame that joins them. Every later capability - replay under
observation, and eventually the memory leg - is a consumer of finished traces
rather than a peer of the harness loop.

## 2. What proto-stateful means, precisely

**The goal is three layers, reached one step at a time, and authority runs
downward**, per the operator's rulings of 2026-10-02 on issue #1. **The trace** is what
happened, the primary artifact of section 1. **State management** is what is supposed to
happen, reconciled turn by turn with what did, built only from the tee of
`weaver-trace-PRD` section 11 and rebuildable from the trace without re-asking a model.
**Memory** is a lossy compression of state, written only by sleep-cycle consolidation.
Memory proposes and state enforces, and the trace outranks both. **State management is
the current step**: its primitives are a contract and a proven connection to a
database, `weaver-state`, and a contract and a proven connection to a classifier on the
SPU, `weaver-harness-spu-classify-contract`, and the loop of section 5.5 is state
management itself. Memory is the step after it and is out, per section 9.

Proto-stateful means **the agent holds real state, within a session and across
sessions under custody, and no memory**. It holds no belief graph, no consolidation,
no sleep or nap pass, and no memory substrate of any kind. The prefix reads as
protoautonomic does in section 4, naming the mechanics alone and claiming nothing
about the finished behavior. An earlier vocabulary called this stage stateless, which
the human's ruling of 2026-08-01 corrected as an overstatement: an agent holding a
working structure and a hot cache is not stateless. That ruling also held that the
agent's state died with the session, and the operator's ruling of 2026-10-02 revises
it: **state management accumulates across sessions**, its holdings carried between
loads by save points in the operator's directory, which the agent never reaches, and
rebuildable from a record the agent never reaches either. The custody is what makes
the crossing lawful, and memory, which will read from those save points, is still to
come.

Two things hold state across turns inside one session, both deliberate, and
they are not two things of a kind:

- **The working structure** is the state the agent reasons over: the run's trace
  events held in RAM in the same canonical form the stream carries, per the
  ruling of 2026-08-01 that retired the relational projection. Lose it and turn
  two has nothing to be about. It is volatile by construction, the program
  rebuilds it from nothing rather than from a record, the stream's accumulation
  being the operator's, and it is working memory, not a store.
- **The KV cache** is an optimization holding the same content precomputed at
  the decoder. Lose it and the agent is slow rather than absent. It is kept hot
  and flushed on the harness's terms, not per prompt, and because its loss costs
  real compute it is the surface most likely to grow quietly into session state
  if the line is not drawn. Its owner, its flush trigger, and who is forbidden
  to touch it are named in `weaver-spu-PRD` as a rule the code can be checked
  against.

**Across sessions one thing holds state, and it is neither of those.** The state
member's store, per `weaver-state-PRD`, holds what the loop's schema admits of what the
tee carried, in memory while the agent serves and in save points between loads, each
stamped with the trace position it covers and never overwritten. It is a derivative of
the record and never a second account: a load restores a save point, the latest by
default or the one the declaration's `restore` names, and with no save point state is
rebuilt from the trace by the offline builder, whose save point the next load restores.
An unclean stop resets to the latest known-good save point, the reset recorded on the
trace, so a rebuild from the record arrives where the reset did. A save point the
operator edits is an input like a prompt file, recorded by digest, per
`weaver-state-PRD` section 4.

## 3. One turn, end to end

This is the path the MVP must execute. Every requirement in every crate PRD
states how it serves this path.

1. A client reaches the agent's **Gate** at its world-opened Unix socket hook,
   per the demotion ruling of 2026-07-31 and `weaver-gate-world-contract`.
   Admin, the agent's management plane, opens no socket the world can reach: it
   is an invocation that dials the worker and exits, per section 12, and there
   is no listening network socket anywhere in the program. **The gate carries a
   second socket, opened by the agent rather than by the world, and step 7's
   tool traffic crosses it**, per the egress ruling of 2026-08-07. The two are
   told apart by which party may open an exchange, per `weaver-organ-channel`
   section 1, and each is single-initiator where an organ channel is not. This
   step's clause once said outbound tool connections do not pass through Gate,
   which the ruling reverses: the gate is the agent's only boundary to the
   world, and a tool call is world traffic that crosses it like any other.
2. Gate authenticates the connection by peer credential under the boundary
   predicate, which admits front-end principals and excludes the agent uid, and
   relays the request to the harness unread: one NDJSON line in, per the world
   contract, opaque at the gate and interpreted by the harness.
3. The **harness** opens the turn root span, assigns the `turn_key`, and emits
   `turn.started`.
4. The harness assembles the prompt: system prompt, the session's message
   sequence read from the working structure, and the tool schemas.
5. The harness issues a decode request to the **SPU** over the decode socket.
   The request carries `turn_key` and `session_key`, so the SPU can parent its
   own spans to this turn.
6. The SPU decodes against the hot KV cache and returns the generation together
   with its measurement payload, tagged with the `turn_key` it was given. The
   harness writes `model.request`, `model.output`, and `model.measurement` into
   the trace. If residual readout is enabled in this agent's config file, the
   eval callback reduces per-layer activations in place and the reduction
   returns by the same path. **The in-place clause is the serving
   binding's**, whose only turn shape this section describes: every step
   here runs gate to gate, and what it protects is the agent a client can
   reach. A diagnostic binding raises no gate and takes no step 1, and under
   it the SPU may answer the activations the tap holds rather than the
   reduction alone, per the operator's ruling of 2026-08-30 and
   `weaver-spu-PRD` section 13.7, which carries the criterion and its
   bounds.
7. If the generation contains a tool call, **the harness addresses it to a
   registered tool through the gate's agent-opened socket**, per the egress
   ruling of 2026-08-07. The tool is a standing application the operator
   provisions, holding its own identity and lifecycle outside the run, and the
   harness reaches it rather than forking it. The seam is a Unix socket under a
   named contract that authenticates its peer, per invariant 5.1, which is the
   floor every other seam already meets and the reason this shape follows
   rather than being elected. This step once had the harness execute the tool as
   the agent's own constrained Linux user, bounded by filesystem permissions,
   argless sudoers grants, and cgroups. What that model put inside the component
   set was a process holding a network descriptor, which is what the socket
   floor exists to prevent, and the ruling moves the network to the far side of
   the socket where the agent never becomes it. The harness emits
   `tool.call.started` and `tool.call.completed`, then returns to step 5.

   **What the tool reaches on its own side is not this program's to bound.** The
   gate selects the application and does not select that application's
   destinations, and the kernel-enforced containment the old clause named
   belongs to the network-facing layer this program is not building. The
   distinction is `weaver-gate-PRD` section 8's and is why this step states a
   boundary rather than a containment.
8. On a final answer the harness emits `turn.closed`, closing the bracket in
   the stream, the close naming its kind.
9. The response returns through Gate to the client, one NDJSON line out,
   delivering a turn already closed.

Throughout: each event is emitted **once**, and that single emission feeds both
the volatile working structure and the outbound NDJSON stream, whose durability is
the operator's per `weaver-admin-operator-contract` section 3. Not two writes to
reconcile. The stream is also the program's one fault carrier: a fault the worker
survives is a `fault` event on it, per the fault-carrier ruling of 2026-08-01,
and no second alert path exists anywhere in the program.

The single emission is authored against the **durable event schema**, and that
schema is the only schema. The working structure holds the same canonical form
the stream carries rather than a projection of it, per the ruling of 2026-08-01,
so no projection version exists and nothing can diverge between the two. A
change to the durable event schema is the breaking change, and it is the one
version every consumer keys on. **Nothing carries that version today and
nothing should**, per `weaver-trace-PRD` section 6, which holds the authority
for this: while section 9's rule keeps the schema extending rather than
changing, every record is of the one schema and a reader that skips what it
does not recognise reads any vintage, so a version member would be a field
whose only reader is unbuilt. The act that first breaks the schema is the act
that adds the marker, and it is an act at this level.

## 4. Definition of done

A proto-stateful agent that:

1. completes a turn end to end, through the gate, with a real local model,
2. emits a trace that is turn-bracketed, contains only events this system
   authored with nothing leaked in from dependencies, reaches the declared sink,
   and conforms to its own schema,
3. hands that trace to a sink the agent cannot reach, durability being the
   operator's per `weaver-admin-operator-contract` section 3,
4. keeps a hot KV cache across turns, flushed on the harness's terms,
5. executes a real internal tool, with bash as the reference case, invoked by the
   loop and bounded by the kernel through the identity it runs as,
6. fires at least one protoautonomic tool call, where the harness injects a
   deterministic result into the stream in place of a stochastic one, with the
   calculator as the reference case,
7. can be reloaded with residual readout enabled or disabled by a change to its
   config file alone, with no rebuild.

**Item 5 moved twice in one day and this is where it rests.** The egress ruling of
2026-08-07 read it as a tool reached across the gate, and the port ruling of the same
date settled the discriminator that reading rested on: **a tool that binds a listening
port is external and one that does not is internal**, and bash binds none. So bash is
internal, invoked by the loop rather than addressed through a socket, and this item
asks what it always asked. The external case has its own hook and no reference tool in
this deliverable, per section 9.

Autonomic action, in the sense this program reserves the word, is
harness-initiated and out of scope here. Protoautonomic names the mechanic
alone, a model-elected call whose result the harness supplies deterministically,
and it makes no claim about the finished behavior. A tool call that requires retained
state to decide when to fire belongs to the later stateful program.

**Initiation is what these words name, and dispatch is a separate question.** Whether
an action was elected or initiated is settled by who moved first, which the trace
records, so it is a fact rather than a reading. Where the harness then routes it,
inward to the organ whose domain it belongs to or outward through the gate as
ordinary output, follows from what the action is for and changes nothing about what
it was. **Neither question is answered by where the capability sits**, and an action
that reaches a network is classified by who moved first exactly as one that never
leaves the process is. `weaver-tools-vision` section 6 works the cross of the two.

## 5. The five invariants

These bind every crate PRD, every Spec, and every contract. A document that
violates one of them is wrong, not merely inconsistent.

**Each is declared here as an `axiom` node, and a Spec's assertion grounds in one
by naming it,** per Document Format sections 3 and 4. The edge runs from the claim
to the invariant, so the chain apex section 11 states reads upward without a break:
code cites an assertion, an assertion grounds in an invariant. The set is closed at
five, and a sixth is an act on this document rather than on the format. It grew from
four on 2026-08-03, when a labelling batch found two Specs disagreeing about a seam and
the corpus had no stated forum for the disagreement.

**A claim that grounds in no invariant is representation, not an omission.** Most of
what a Spec elects is a format, a name shape, a tagging rule, or a bound, and none of
those serves an invariant because the invariants are not about representation. The
coverage number is a fact to read rather than a target to reach, and it is stated
that way here so it cannot later be argued down: the prior program's basis reached
seven of seventy-one claims and the answer was to keep the layer rather than to ask
what the layer was for. **This rule has one stated exception and Document Format
section 4 carries it**, per the ruling of 2026-09-14. Read it there.

**The named exception to Working Process section 7 ran its course and is
closed.** The 5.1 restatement and 5.4 were taken early on 2026-07-31, further
entries followed as collisions with merged charters demanded, and the
re-authoring of 2026-08-01 absorbed them all. Nothing files against this
document now except by editing it.

### 5.1 The floor is vocabulary and every behavior is a socket

**The floor is exactly `weaver-traits` and `weaver-types`.** They are linked as
Cargo dependencies because they are shared vocabulary, types and traits and
schema, and you cannot send a type definition over a socket. Floor is a linkage
fact rather than a rank: a floor crate is one every domain draws from and no
domain contains. This is the definition the whole corpus classifies against.

`weaver-trace` is not floor. The harness links it as a Cargo dependency too, but
under a contract and as a member of the harness domain, because the harness is
its only caller. Depending on nothing is what it has in common with the floor.
Being drawn by everything is what it does not.

**Every seam where one crate asks another process to do something is a Unix
socket governed by a named contract, and it authenticates its peer.** There are
no exceptions, including for crates that arrive later. When the memory leg
returns it returns behind a socket, not as a path dependency.

**How it authenticates follows from whether the channel has a name.** A channel
reached by a path is reachable by anyone who can resolve that path, so it
authenticates by credential, which is what `SO_PEERCRED` is for. A channel with
no name is a connected pair created by one party and handed to another, and
possession of the descriptor is the authentication, because no third party can
reach a socket that has no address. Which party creates the pair, and how the far
end travels to the process that holds it, belong to the contract governing that
seam and are not the apex's to settle. Two cases, one property.

The property is what the invariant protects: no process in this program talks to
another without the second knowing who the first is. The earlier reading named
the credential mechanism as the invariant itself, which made the coordination
channel of `weaver-admin-harness-contract` section 2 read as an exception rather
than as the second case, and an invariant that admits one exception in its first
round of contact with a real seam is a rule that will admit a second.

A seam that does not cross a process boundary is a library boundary. It is still
a seam and still governed by a named contract, and it is tagged `link` rather
than `socket` so the difference is stated rather than inferred. The
harness-to-trace seam is the one such seam in the base set. What the invariant
forbids is a behavior reached by path dependency across a process line, not a
crate calling a crate inside one binary.

**A crate outside the floor links the floor for the reason this section gives, and
takes from it only what it draws.** The link is licensed here: the floor is shared
vocabulary and a type definition cannot be sent over a socket, so every consumer links
it and no consumer is asking it to do anything. **Which floor crates it links follows
from what it draws**, per section 5.3's mechanical consequence that a party links the
crate defining what it emits, which is what makes the party list checkable against the
dependency graph. So a Spec whose clause argues only that it links the floor carries the
one edge here, and a Spec whose clause argues that a particular floor crate is linked
because a contract draws from it carries that one too.

**Which features a floor link is taken with grounds in nothing.** It is a build
election that would read the same under any invariant, and a crate takes the
configuration feature or leaves it because of what that crate does rather than because
of anything this section or 5.3 requires. Stated because the shape invites the opposite
reading: a feature named for what the crate does not draw looks like the draw rule
speaking, and it is not.

**The filesystem is reached the same way, through a door and not by a path**, per
the operator's ruling of 2026-09-23. Nothing internal to the agent's operation opens
the filesystem directly: access goes through admin or the gate, over a Unix socket.
The exception is narrow and has three conditions, all required: the access is needed
for the agent's internal operation, it is scoped to the one function that needs it,
and the kernel secures it. The SPU reading its pinned model artifact, the embedded
state engine opening its store in a territory only its own account can enter, and the
Python loop worker reading its loop file each meet all three. A descriptor handed over
by admin is the preferred form where it fits, as the trace sink and the diagnostic
record already take it, since an organ that holds a descriptor and no path cannot name
anything else. The rule is the same one that governs what is compiled into a binary:
nothing is present unless operation requires it, and a need that arrives later is
answered by adding it and rebuilding rather than by admitting it in advance.

This invariant is what makes statefulness a feature add rather than a
re-architecture. Memory behind a socket is a new socket, a new contract, and a
schema extension. Memory as a linked crate is surgery on the harness's
dependency graph and on every call site.


```graph
node: axiom-floor-is-vocabulary-behavior-is-socket
kind: axiom
```

### 5.2 The join key travels with the work

**Every request that belongs to an existing turn carries, at every seam it
crosses, the trace context identifying that turn, and every response carries it
back.** The scope is deliberate and load-bearing: a lifecycle directive on the
coordination seam and a residency directive on the SPU seam each cross a seam and
belong to no turn, so they carry none, and the earlier universal wording made
every such directive a counterexample to the invariant it was meant to serve.

The harness is the sole writer of the trace, so a component does not emit its
own spans. It reports, and the harness authors the event. That only works if
the report can be attributed. A component handed work without a `turn_key`
cannot tell the harness which turn its result belongs to, and with more than
one turn in flight the harness cannot recover the association afterward. This
is what turns the trace from a set of per-process logs into a coordination
substrate, and it must be designed into every wire format at the moment the
wire is specified.

**The custody behind that sentence has four roles, and each sits with the party
that must hold it.** An organ renders the content of every event it causes, the
harness authors the event, the trace writes the line, and the floor carries
only what the harness itself consumes. The first three are the sole-writer rule
read at three seams, and the organ's role is the one this document had not
said: what a component hands over is its own rendering of what happened,
because the organ is the party that knows, and the harness splices what it is
handed rather than reconstructing it. The fourth is the floor half, and it is
what keeps the floor from growing by one crate's vocabulary every time an organ
gains something to say. Under the rule a new organ arrives with its own payload
shapes and renders them, the harness gains an event kind, and no shared type
library accumulates a member per organ per feature. The cost is stated rather
than discovered: spliced content is opaque to the compiler, so conformance
moves from serde to the check at the harness's submit call, which is affordable
because it is one place, and it is the place where the kind-to-payload pairing
is already enforced by admission rather than by types.

**The fault path is the standing exception, named rather than silently
tolerated.** The floor carries `FaultReport` today and the harness renders it,
which runs this rule backwards, while the generation measurement already runs
it forwards, the SPU rendering and the harness splicing verbatim. Two patterns
with a reason discovered per case is how a set acquires no rule, and this
paragraph is the rule. The shape of `FaultReport` is an open election in
`weaver-types-Spec` section 6, so the fault path's correction lands in the act
that settles that election, where the custody and the shape are one judgment
rather than a wording change ahead of its mechanism.


```graph
node: axiom-join-key-travels-with-the-work
kind: axiom
```

### 5.3 A contract is a complete interface

A contract names its parties, and for each party: the vocabulary that crosses
the seam with its meaning, the errors it can return, and the ordering guarantees
it relies on and provides. How a party represents any of that internally belongs
to its Spec and appears in no contract.

**Every contract carries a vocabulary clause, and a contract without one is not
a valid contract.** The clause names what the contract draws, grouped by the
crate that defines it, and a group is stated even when it is empty, because an
explicit nothing is an assertion someone checked and an absent group is silence.
This is what makes the floor governable without a floor contract: the floor's
required surface is the union of every clause, a definition no clause names is
unused, and a definition a clause names that the floor lacks is a gap.

Two mechanical consequences:

- A party that emits **links the crate that defines what it emits**, so the
  party list is checkable against the dependency graph.
- An agent handed one side of a contract can build that side without asking
  what the other side does. This is not an aspiration - it is the property
  that permits crates to be built in parallel once the floor is merged.


```graph
node: axiom-contract-is-a-complete-interface
kind: axiom
```

### 5.4 Organ and submodule

**An organ is a crate that governs a domain and holds a two-initiator channel with the
harness.** Both properties, and neither alone. A crate that governs a domain and
reaches the harness some other way is not an organ, and the two-initiator requirement
does not bend.

**A submodule falls under an organ's domain with that organ as its consumer.** It
holds no channel with the harness, and the shape of its channel with its own
organ is unconstrained and is that organ's business. That a submodule is never a
party to a lifecycle transition follows from its having no channel with the
harness, rather than standing as a rule of its own.

The harness is the organ whose domain is coordination, which is why it is the hub
every other organ holds its two-initiator channel with rather than a spoke. This is
written as a test a candidate passes and not as a list of the organs that exist
today, so a crate chartered later is classified by reading it against the test
rather than by amending an enumeration.

**No crate holds an allowance another crate could not have.** What a Spec elects for
one organ is available to any organ that later finds the need or the capability, and
a reader meeting an election in one document should not read it as that crate's
privilege. The classification above is written as a test for the same reason, and an
election that could only ever be one crate's is a sign the election is really a
charter fact wearing a Spec's clothes.

**The harness is the one exception, and its form and function are not negotiable.**
It is the organ whose domain is coordination, so the hub is what it is rather than a
shape it elected, and no efficacy argument reaches it. Every other organ is a spoke
by construction and cannot trade that away.

**Whether an organ mirrors this shape inside its own domain is section 5.5's, not
this section's.** The topology here is fixed. What an organ does one level down is a
question about integration and is answered where integration is.

Section 6 already names organs and the harness as the coordinating center, and
5.1 already carries the floor half of the same three-way distinction. This
harvests what those two imply rather than importing a new frame.

```graph
node: axiom-organ-and-submodule
kind: axiom
```

### 5.5 The harness integrates, and the loop is the mechanism

**The harness is the integrator.** Section 5.4 makes it the hub every organ holds its
two-initiator channel with, which is a statement about topology. This is the role that
topology exists to serve. The harness is answerable for the whole working, and an
organ is answerable for its own domain and for nothing outside it. A hub that only
carried traffic would leave integration to whichever organ noticed it was missing,
which is how an organ starts reasoning about a domain that is not its own.

**The loop is the integrating mechanism.** Integration is not a property the parts have
when assembled correctly, it is work something does, and the loop running in the harness
is the thing that does it. This is why the composition root is new code rather than
carried code, and why loop 0 is the framework's rather than a builder's: the mechanism
that makes the parts one program cannot itself be a part.

**The loop is state management**, per the operator's rulings of 2026-10-02 on issue #1,
and every agentic loop is: it decides what reaches the model and when, and reconciles
what happened with what is supposed to happen, turn by turn. **There is one loop,
written in Rust and compiled into the worker**, which is the deployment path
`weaver-harness-Spec` section 1 already states, the Python-iterating worker staying
untouched as the bench and experiments lane. There is no loop election and no pluggable
loop, and an operator bringing a loop of its own is parked by name at section 9. The
loop calls on the state member's store and on the SPU's classifier through the harness,
organs never linking, and both are primitives in the sense of section 2: mechanics
without motive, the schema and the purpose being the loop's. **Its opinions are the
operator's to set**: wherever an operator could reasonably want a different answer, the
loop exposes the choice as a setting in a `[state-management]` table of the agent's
declaration, each setting with a compiled default, and the `load` event records the
values in effect, so a record says what its loop was told. Which settings exist is a
later act's. Custody, the rebuild from the trace, and every state write passing through
the tee are never settings. **Every judgment the loop makes is recorded on the trace**,
the model-made ones included, a classification or a ranking as surely as a flush, so
state stays rebuildable from the record without re-asking a model, per
`weaver-trace-PRD` section 3.1.

**Each organ presents its contracts to the harness and presents nothing to any other
organ.** A contract is what an organ offers the integrator, being the vocabulary it
speaks, the errors it returns, and the ordering it relies on and provides, per section
5.3. That is the whole of what an organ exposes and the whole of what the loop has to
work from, which is what lets an organ be built against its own contract alone.

**An organ presents one seam per service within its domain, and the plural is the
general case rather than an exception.** A socket serves one service, so an organ with
three kinds of traffic holds three seams under three contracts, which is what
`weaver-spu` already does with residency, decode and classify, per `weaver-spu-PRD`
section 6. The alternative is one channel carrying two services, where a flush ordering
written for one silently becomes a rule about the other and neither party can say why
it holds. **The seam's identity is the contract governing it and not the pair of crates
it runs between**, which is why three seams between one pair need three names and why a
reader counting crate pairs would miscount this program's shape.

**The loop is answerable for correctness and for timing.** It ensures that point A talks
to point B correctly and at the right time. Correctly means the vocabulary each contract
names, in the direction that contract states. At the right time means the ordering each
contract relies on still holds when two organs' orderings have to be reconciled against
each other, which no organ can do from inside its own domain, because no organ can see
the other's. **Reconciliation across domains is the loop's work by construction and not
by convention.** Reconciliation inside one domain is the organ's own and this section
does not reach it.

**So a conflict between organs is settled in the contracts they hold with the harness.**
There is no organ-to-organ contract to settle it in, by 5.4, and no other party sees
both sides. A seam question two organs answer differently is not a hard case calling for
a ruling, it is an incomplete contract, which 5.3 forbids by name. The obligation runs
toward the contract rather than away from it, and Document Format section 7 states the
order a reader follows.

**This invariant binds what crosses between domains and says nothing about what
happens inside one.** An organ integrates its own parts however its domain demands,
and it may well arrive at this same shape one level down, a submodule reaching its
organ and an interior loop making the organ's parts one thing. That is emulation and
not obligation. An organ that adopts the pattern inherits an argument already made,
and an organ that departs from it answers to its own Spec and says why there.

**Demand is the pressure and the domain is where it acts.** An organ develops as its
domain's demands dictate, the way an organism develops under selection, and the shape
it arrives at is answerable to those demands rather than to this section. What the
program fixes is the environment that development happens in, which is the harness's
form and the contracts at each organ's edge. **The harness's form is not negotiable
for the same reason the environment is not: it is what the organs develop against,
not an organism developing alongside them.** An environment that changed shape under
the same pressure would leave nothing for anything to adapt to, and every organ would
be adapting to every other organ's adaptations, which is the peer coupling 5.4 forbids
arriving by a slower route.

So the constraint is narrow and it is the whole of it. What an organ exposes is its
contract. What crosses between organs is the loop's. **Everything else inside a domain
is that domain's to evolve.**

```graph
node: axiom-harness-integrates-by-the-loop
kind: axiom
```


## 6. The agent lifecycle

The administrative lifecycle has exactly two state-transition verbs, `load` and
`unload`, and one verb that transitions nothing, `validate`, which confirms an
agent's configuration and boundary without starting anything against them.
Read-only `show` is an observation, not a transition. Admitting and
removing a principal are operator acts on the operating system, performed before
an agent exists and after it stops existing. They sit outside the program's verb
set, and what they bracket is the resting state the diagram names provisioned
and unloaded, per `weaver-admin-PRD` section 4.

```
absent
  |   ^
  |   |    operator provisioning / removal
  v   |    (operator acts, not verbs of this program)
provisioned, unloaded
  |   ^
  |   |    load / unload
  v   |
loaded, idle
  |   ^
  |   |    work starts / work ends
  v   |
active
```

Provisioned identity is made and removed by the operator rather than by a verb.
`load` and `unload` govern the complete residency boundary. **Loaded-and-idle
is a first-class state** -
without it the system cannot interrupt one run while leaving the agent ready
for the next.

The chain, in order:

| Component | Owns |
|---|---|
| **weaver-admin** | External authorization, boundary verification, lifecycle direction, custody of the sink, rollback |
| **weaver-harness** | The fan-out inside enter and leave, readiness aggregation, activity control |
| **weaver-spu** | Model admission, decoder and encoder residency, GPU release |
| **weaver-gate** | Sole work ingress, and the loop's membrane, raised last and lowered first |

**Every organ is one agent's own**, on the operator's ruling of 2026-10-01: started
under that agent's identity, constituted by that agent's configuration, and serving
no other agent. Admin is the one organ that does not run under the agent's identity:
it is the operator's root invocation against that one agent, reading only the agent's
own root, so it is that agent's by what it acts on rather than by its uid. An organ is a component of one agent and not a service of the host.
A second agent gets its own admin, SPU, gate, state member, loop and harness, so two
agents never share a process, everything an agent does is tied to its own
processes, and the operating system enforces the identity between them.
The constitution reaches the organ whole: each starts with the environment of its
agent's worker, which is the agent's unit as admin configured it, and never with an
environment of its own, since an organ constituted from anywhere but its agent would
be a host service able to serve several, per `weaver-harness-Spec` section 2.2.

**Admin is the coordinating center of the load, and the harness is the
coordinating center of the turn.** Admin authorizes the intent, verifies the
boundary the operator wrote, opens the sink, starts the worker unit, and directs
the transition across its one seam, rolling back its own acts where a directive
refuses. The harness cannot drive the early steps of its own creation, because
the worker spawn runs before the harness exists at all, and the acts a verb
performs are root's, held by the operator in the admin role rather than by any
standing process of the program's, per `weaver-admin-PRD` section 7 as recut on
2026-08-05. What is long-lived where the harness is mortal is the init system,
which holds the unit and outlives every invocation that drives it. What the
harness owns is the interior of the directives:
admin holds no channel to the SPU or the gate, so the harness fans the directive
out along its own seams, collects each organ's confirmation, and returns one
aggregate. Sequencing the organs is the harness's because the seams are, and
each organ performs its own operation.

**The harness is the sole writer of the trace across both centers.** It stands
up the working structure, authors the `load` event of the run, which for a
session's first run is the record of admin's initial contact, and writes every
component's activity
into the stream for the whole of the residency. All coordination between
components passes through the harness, which is what makes it the hub every
later organ attaches to, each behind its own socket and contract, and the
structural reason statefulness is an extension rather than a re-architecture.

`weaver-trace` is not a party to the transition. It is linked by the harness
under a contract as a member of the harness's domain, per 5.1's link case, so it
cannot be a socket peer that confirms anything. It is the mechanism the harness
records through **on a serving binding**, during the load and for the whole of
the agent's residency thereafter. On a diagnostic binding that place is
`weaver-diagnostic`'s, per the composition below, and the non-party argument
above reads the same for either: a linked member confirms no transition.

**An agent stands up under a binding, and the binding declares its kind.** The
same weights serve more than one purpose, and which components a load brings up
follows from the purpose rather than from the weights. A **serving** binding
stands up the whole interior and takes work from outside. A **diagnostic**
binding stands up the interior without Gate, for the replay of section 8,
where a driver outside the agent preloads a record the operator holds and the
interior replays it. The
kind is declared at the load and never entered afterward. Nothing puts a loaded
agent into a kind, no verb moves between kinds, and an agent that is to serve
and an agent that is to be replayed are two loads rather than two states of
one. What that buys is that the wrong arrangement is unrepresentable rather
than guarded against, there being no transition for a guard to sit on.

**The kind selects a composition of members rather than a different program**,
and that is the property this project was built to have. The harness is the
sole writer under either kind and `weaver-state` stands under either, composed
differently: distilled into and asked of under a serving binding, preloaded and
walked positionally under a diagnostic one. What the kind swaps is the
mechanism the harness authors through. A serving binding composes
`weaver-trace` and writes a trace, and a diagnostic binding composes
`weaver-diagnostic` and writes a diagnostic-trace, each into the sink admin
opened for it. Neither is an absence and neither is a special case in a turn's
path, the choice being made once at the load and never entered afterward.

**A diagnostic binding writes no trace, and the distinction is not a
quibble.** A replay performs no cognition of its own: it re-executes a token
path some record already holds, over prompts that arrive preloaded rather than
raised. A replay that wrote a *trace* would leave the corpus holding two
records of one turn, alike in kind and alike in shape, with nothing in either
saying which was lived and which was re-executed, and that is worse than an
absence because the second one reads as evidence. What it writes instead is a
record that says what it is by being a different record, one whose every event
came from a replay rather than from a turn. **The ground is provenance and not
any member it carries.** The residual readout is a production instrument, per
section 8, elected at the load under either kind, and a serving record carries
it whenever a serving load elects it, per section 3 step 6. A diagnostic-trace
is dense with it because a replay is run to be measured, and that is a fact
about why the run happened rather than a member only one record may hold. **One
begets the other**, per `weaver-diagnostic-PRD` section 6, so the two stand in
a parent relation rather than side by side.

**The rules below measure a load against the components its binding declared.**
Measured against a fixed set they say a diagnostic binding cannot complete: a
load that never starts Gate never confirms Gate's transition and so never
succeeds. Section 8 places the replay driver outside the agent and does not
say whether the agent under replay carries a live Gate, so the fixed reading
does not contradict a promise so much as force the worse arrangement, a
replaying agent holding an open work ingress nothing should dial. An ingress
whose safety rests on nobody dialing it is a guard, and this section's own
ground for the kinds is that a guard is what the declaration exists to
replace. **The scoping is section 8's own move rather
than a new one.** Completeness there is claim-relative, a record owing what its
claimed arrangement requires and nothing beyond, and a load owes the components
its binding declared on the same reasoning. What the rules protect is
unchanged, that a published agent is whole, and whole is read against the
declaration rather than against a list that assumed one purpose.

Lifecycle rules:

- A lifecycle call succeeds only after every component transition the binding
  declares is confirmed.
- A partial load is never published as loaded, partial being measured against
  the components the binding declared.
- A serving binding starts Gate last and stops Gate first, and a Gate process
  never outlives the worker interior it protects. A diagnostic binding declares
  no Gate, so it holds no such process and the ordering has nothing to order.
- Every binding declares a sink and the harness authors the run's record into
  it. Which record it is follows the kind: a serving binding authors a trace
  through `weaver-trace`, and a diagnostic binding authors a diagnostic-trace
  through `weaver-diagnostic`. The sink handle crosses the enter under either
  kind, so its count is one and does not vary.
- No lifecycle verb auto-chains another.
- A GPU conflict is rejected at model admission, by the SPU, until the operator
  explicitly unloads the occupant. Admission refuses and never evicts, so no load,
  at any point in its sequence, auto-evicts another agent. The SPU is the one
  authority on the device and nothing upstream weighs it.
- No prompt, turn, task, or run enters through Admin. Work enters only through
  Gate.
- Activity stop, cancellation, or interruption returns the agent to
  loaded-and-idle. It does not unload the agent.

**What a diagnostic binding takes in place of work is settled by the
operator's ruling of 2026-08-24, and the papers have landed.** The work-ingress
rule above is untouched and stands as written. A replay does present prompts,
and they cross a seam of their own rather than arriving as material declared
at the load: the preload of `weaver-analysis-state-contract`, on the state
member, checked against the operator principal, per section 0's boundary
sentence as amended the same date. The contract and the state charter's
second door landed in the seam-papers act of 2026-08-24 and the seam is open
rather than ruled. Its near side is `weaver-analysis`, whose charter carries
the edge from the initiating side. An earlier form of this sentence called
the papers owed, which they were for part of one day, and pointed at
`weaver-diagnostic-PRD` section 6 for a seam that section now disclaims
holding. An earlier form of this paragraph held the question open
on purpose, so that wording would not settle what the drawing has to decide,
and the ruling is what closed it rather than a drafting choice.

The lifecycle is the system's largest behavioral contract - four parties over
three sockets - and it is filed at this level rather than under any one crate,
because it belongs to none of them.

## 7. Scope criteria

Material crosses from the prior tree, or is written fresh, only if it satisfies
one of exactly two criteria. State which one, at the moment you carry it.

### 7.1 Does it serve one turn end to end?

"Serves" means you can name the step in section 3 that exercises it. Not that
it might be needed, not that the prior tree had it.

### 7.2 Is it observability the operator needs to diagnose a deployed agent?

The named set, closed:

- **Residual-stream readout.** Per-layer activations from the running decoder,
  reduced in place under the serving binding, enabled or disabled per agent by
  its config file, and under the diagnostic binding answerable as the tap's
  columns per the ruling of 2026-08-30 and `weaver-spu-PRD` section 13.7. See
  section 8.
- **Measurement payloads.** Token identifiers and token entropies, emitted into
  the stream at production time. These are what make replay under
  observation possible. Without them a replay is approximate, and an
  approximate replay of a forward pass is a false diagnosis rather than a weak
  one.

The set is closed deliberately. A second criterion with an open membership
becomes the door through which everything re-enters, and each individual
admission looks principled.

Anything satisfying neither criterion does not cross. **Nothing crosses because
the prior tree has it.**

## 8. Replay under observation

The trace is not only the record of a session. It is the source from which a
session can be re-run under close observation.

The loop is stochastic and does not reproduce, and this program makes no
run-again claim, in any arrangement below. Replay does not need one.

**What a record supports follows from what the deployment declared and
produced, per the ruling of 2026-08-02, and the arrangements are not ranked.**
Re-analysis over the frozen record is always available, because everything
produced is recorded. Deterministic re-feed is available when the record holds
the token path: because the trace records the sampler's **actual tokens**
rather than a seed, a recorded scenario is replayed by feeding the recorded
token sequence back through the forward pass. Nothing is re-sampled. The
residuals are deterministic given the same weights, within GPU float
tolerance. And stochastic re-entry from the same starting field is available
when the binary declares one, the setup surface frozen at the worker's
composition root, seed and sampling parameters baked immutable. A frozen seed
narrows variance and buys audit rather than determinism, which is why the
run-again claim stays unmade there too. The conditions are stated for the
requirement's shape, and on the corpus this lands into the first two
arrangements are always available, every record holding the token path since
the levels retired, so the third alone waits on a binary's declared
disposition and the claim-relative rule governs arrangements not yet built
rather than forking the present.

This makes residual readout a **production troubleshooting and interpretability
instrument** rather than a research aside. When visibility is needed it is
enabled and the agent runs slower. When it is not, the agent loads without it.
The cost is real and it is the operator's to elect, per load.

**Completeness is claim-relative rather than a fixed bar.** Deterministic
re-feed requires, exactly: input token ids, output token ids, model identity
and weights hash, sampling parameters, and the prompt-block partition, with
tokenization reproducible from what is recorded. These are requirements on
`weaver-trace-PRD`, derived from a `weaver-spu-PRD` capability. A record
claiming a replay arrangement carries everything that arrangement requires,
and a replay missing an input its claim requires observes a forward pass that
never happened. A deployment claiming only re-analysis owes nothing beyond
the record itself, because it claims nothing more.

**A claim about the state rests on one fact beyond that list, and it is
claim-relative the same way: the tee's election**, the rule that decided which
events crossed into the agent's working state and which payload paths rode
with them. A replay that preloads under a different rule rebuilds a session
that never ran, and nothing in either record would mark it, so the record
carries the rule itself, a requirement on `weaver-trace-PRD` landed at its load
bracket, its representation `weaver-trace-Spec` section 3's. A record written
before that member existed fails a claim about the state and not a claim
about the token path: the recorded identifiers stand and everything resting
on the holdings does not, which is the claim-relative rule saying which claim
such a record is good for rather than failing both.

Custody places the replay driver outside the agent. The agent must not own or
even read its own trace, so the tool that reads the operator-held stream runs as
an operator principal, over the operator's own storage. This is structural, not
policy. **That tool reaches no device.** It preloads what it read across the one
ruled seam and the interior replays it, per section 6 as ruled 2026-08-24 and
`weaver-analysis-PRD` section 3. An earlier form of this paragraph had the tool
driving the SPU, which would have given a crate outside the agent a path across
a process line into an organ, the arrangement section 5.1 exists to forbid.

**The load that carries this arrangement is the diagnostic binding of section
6.** This section places the driver outside the agent and leaves the agent's
own shape to the lifecycle, and the diagnostic binding is that shape: no Gate
stands, so the record the driver preloaded is the only source of prompts, by
construction rather than by nobody dialing a live ingress.

## 9. Out of scope, and how it returns

**Out entirely:** the memory leg in every form - belief graph, consolidation, sleep
and nap passes, and any memory substrate. Also out: offline analysis,
training, and the desktop frontend.

**The identity's source is parked here by name**, on the operator's ruling of
2026-10-02. Today an agent's system prompt is a file the operator edits, named by the
declaration and read by admin, and the harness carries it to every load's open, per
`weaver-harness-Spec` section 6.1, which names the one site where the identity's source
is read. The operator's long-range direction is for state management to supply that
identity instead, a semantic-graph store kept current by sleep-cycle consolidation.
That is the memory leg above and is out, not built here, and it returns by the
mechanism this section states: naming the site is not a slot, and no field, ask or
seam is carried for it.

**External tooling is out entirely as well, and stays out.** This program builds no
tool crate, and the reason differs on each side of the boundary.

**Outward, what it owns is the grip:** the interface a tool is built to be gripped by.
A hammer is not installed into a carpenter, it is shaped so a hand can hold it, and the
boundary contract is that hand stated as a specification. **Which tools fit a grip is
settled by the port test**, per the port ruling of 2026-08-07: a tool that binds a
listening port is external and reaches the agent through a grip, and one that does not
is internal and is loop code the harness runs. A bash tool, a database client, and an
API caller bind no listening port and are therefore internal, whatever their bytes
reach. What fits the agent-opened grip is a service that listens, and none of these is
a crate here either way.

**The grip is two contracts and not one, as of the egress ruling of 2026-08-07.**
`weaver-gate-world-contract` is the grip on the world-opened socket, where the world
dials the agent and a client holds its own execution. The agent-opened socket is a
second grip with the trust direction turned around, the agent reaching a registered tool
and each end reading the other by kernel-supplied credential, and its contract is the
tool workflow's to author.

**What the gate owns of that second grip and what the workflow owns are split here, so
the workflow inherits a boundary rather than a blank page.** The gate binds the
listener, judges every connection by peer credential against the registered-tool
allow-list, and relays octets in order without reading them, which is the same three
things it already does on the world-opened socket. The workflow owns what a credential
means: which identity a registered tool presents and how that maps to its registration,
how a tool is addressed among several, what a call and its answer are, and how a tool
is provisioned and supervised. The split is the ordinary one between a socket's
mechanics and a seam's meaning, and it lands in two documents, the tool-seam contract
and `weaver-gate-Spec`, neither of which this act writes.

This paragraph once named the world contract alone and said
the tool case was that same path with a different thing on the far side, which was true
of a client-held tool and is not true of one the agent addresses.

**Inward, what an older reading called an internal tool is a function loop, located
with the control loops in the harness.** A control loop runs the turn and the
lifecycle. A **function loop** is a function used in a predefined way when the correct
signal is given, and the calculator section 4 requires is one: the harness injects a
computed result where a stochastic one would otherwise go. Managing state and deciding
when state enters the decoder's path are the same kind of thing. Routing the right
work to the right part of the agent is what the loop is for, per invariant 5.5, and
**a calculator small enough to be a few lines does not become an organ by being
useful.**

So the rule reaches the far side of the grip and nothing else. A hand grips what is
outside it, and a function loop the harness runs was never on the far side of
anything.


**The grip is not a reserved slot and is the clearest case of the difference.** A
reserved slot is a shape carried for a reader that does not exist. The world-opened
grip has a reader today: every client that dials the gate uses it. The agent-opened
grip has none yet, and it is not carried in anticipation either, because nothing is
built for it until the tool workflow charters its contract and a party to it exists.
What the ruling of 2026-08-07 added is a requirement in this document, not a shape in
a crate.

**Memory alone stays out, and state management is in**, per the operator's rulings of
2026-10-02 on issue #1. State management came back through the door this section
fixed: `weaver-state` is a schema extension behind a new socket and contract, with its
own PRD, and nothing in the base set moved for it. Holding what the agent was asked to
hold, and answering a query over what it holds, are state management's and in scope.
What stays out is the layer below it, the lossy compression consolidation writes, which
returns by the same door and reads from state's save points when it comes.

**The operator bringing the loop is parked here by name**, on the same rulings. The
loop of section 5.5 is one loop, compiled into the worker, and `loop-file`, a
declaration naming code the worker runs, retires for the compiled path with the
pyworker lane. Its opinions are settings in the declaration's `[state-management]`
table, recorded on the load event: the table's grammar is `weaver-types-Spec`'s, with
the declaration's other keys, and its meaning is `weaver-harness-Spec`'s, with the
loop, per the operator's ruling of 2026-10-02 on #58's sixth question. An operator
supplying a loop of its own, by a declared file or any other path, is a possible
future feature, not built here, and it returns by the mechanism below if it returns at
all. Naming it is not a slot: no field, flag or seam is carried for it.

**Save points as task- or workflow-level presets are parked here by name**, on the
operator's ruling of 2026-10-02 on #58: a known-good context returned to repeatedly for
different tasks on one project, like a system prompt but at a task or workflow level.
State management keeps save points and restores one on demand, per `weaver-state-PRD`
section 4, and a preset built on them is a future feature, not built here: no field,
flag or seam is carried for it.

Memory returns as a **feature add**, not as a retrofit. The mechanism is
fixed now, in three parts:

1. **Schema extension.** The durable NDJSON event schema is the one schema,
   versioned once and extending additively, under the authority stated in
   section 3. Memory adds event kinds and payload shapes, and it does not
   reshape existing ones.
2. **A new socket and a new contract.** Per invariant 5.1, memory arrives as a
   socket peer with a complete contract, never as a linked crate.
3. **Its own PRDs.** Memory's PRDs are written per crate as required, and
   contracts are amended or added by the order of work in section 10.

No seam, stub, reserved slot, or dormant contract party is carried in
anticipation of this, **and a reserved slot can be a data field as easily as an
interface**: a payload field whose only reader is unbuilt, a vector nothing
retrieves, an event kind with no emitter. The schema is where this rule is
tested most often, because adding a field feels smaller than adding an
interface and is the same error. Preparation for memory is a property of the
schemas being extensible, not a set of empty joints or empty fields. A slot
reserved today is a guess about a design that has not been written.

## 10. The order of work

A merged document changes by being edited. A ratified document does not change at
all, and a change found necessary after ratification returns the work to authoring
rather than being patched in place. **Ratification is suspended until release**, on
the operator's ruling of 2026-09-27, so until then a document that disagrees with
the code is edited in the same act as the code. The three states and their
transitions belong to the Working Process, section 2, and are not restated here.

No amendment banners. No supersession notices. No citations into retired
documents. No obligations patched inline because their referent was withdrawn.
If a change touches a contract, it names every party to that contract, and a party
that does not merge with it is named as owed in that document's register.

Every one of those devices is a reasonable local decision. Together they are
how a corpus stops being coherent while every individual document still looks
maintained.

The order of work is strict:

```
1. This document and the seven crate PRDs of phase one, together
2. Each crate PRD with its contracts, written as one act
3. Specs, against the merged PRD and contract set
4. Graph mapping, which ratifies the set
5. Floor code: traits, types, trace
6. spu | harness | admin | gate, in parallel
7. Composition root, integration
```

Nothing at step N is written before step N-1 is merged. The parallelism at step
6 is earned by the completeness of step 2 and the merging of step 5, and by
nothing else. Contracts that are complete cannot be built against in parallel
while the floor beneath them is still moving.

A contract is not a phase of its own. The harness is the hub every crate connects
to, so a crate PRD is largely about its seam with the harness, and a PRD written
without its contract has no center to attach to and grows one of its own. Specs
come after the contracts rather than before them, because a Spec is build
instructions for one crate written against its PRD and every contract that crate
is party to. A Spec written first documents code instead of governing it.

## 11. Enforcement

**The graph exists, and this section was written before it did.** This section
predicted that no graph would measure code against this document set during this
program, and that a conformance graph built from code and a mapping graph built
from documents would share a word and nothing else. Both predictions are retired,
and Working Process sections 5 and 7 and `CLAUDE.md` carry the same retirement,
landed in the same act. What the graph holds is a reading and is not stated in a
charter.

**The graph is deferred to release**, on the operator's ruling of 2026-09-27, and
until then the conformance check is review of the code against the Spec sections a
pull request names, per Working Process section 6. HADES returns at release as a
lookup and a diff of documents against code.

**The devices below are not retired by it.** The graph indexes them and does not
replace them. The PRD to Spec to contract to code chain is enforced by the devices
below, the first suspended until release and the sixth retired by the ruling of
2026-09-27.

What enforces:

1. **Conformance trace headers in source**, carrying the code to assertion to
   document chain. **Suspended until release**: existing headers stay and are not
   maintained, and no new header is required.
2. **Compile-time pins** for invariants that are type properties. A runtime
   test structurally cannot pin the absence of a trait implementation.
3. **Perturbation-verified tests** for invariants that are behaviors. Always
   confirm the test fails when the property is removed. A test that passes
   whether or not the property holds is worse than no test, because it converts
   "unenforced" into "documented as enforced".
4. **Human and third-party review**, reading the review body rather than the
   thread count.
5. **Clippy at deny-warnings**, which became a gate on the operator's ruling of
   2026-09-06. Working Process section 6 and `CLAUDE.md` state its place and its
   invocation, and this section does not.
6. **The census**, which was H6 from the operator's ruling of 2026-09-11 and
   **retired on 2026-09-27**. Working Process section 6 states the conformance
   gate that stands in its place, and this section does not.

The last two arrived after the first four and are appended rather than sorted in,
because device 3 is cited by its ordinal elsewhere in this corpus and renumbering
would move what those citations point at.

A clean automated gate is evidence that the gate did not fire. It is not
evidence of correctness.

## 12. Why this architecture

The design rests on one engineering principle: lower latency is always better,
provided you know what you are giving up to get it. Everything above is a
consequence of naming that trade rather than leaving it implicit, and every
decision in this document is justified by it and by nothing else.

What this architecture gives up is fungibility, and with it the service
reliability that fungibility underwrites. The model is not swappable behind a
remote API, and the harness does not run somewhere other than where the agent
runs. Both are bound to the device, and that binding forecloses what a network
architecture gets by construction: horizontal scale, failover to a healthy
replica, and rolling replacement of a degraded node. A deployment that needs
those is building a different system than this one, and should say so rather
than treating the difference as configuration.

**The gate's far side is experience and never the world.** Whatever stands out
there - a client, a tool process, an appendage - meets the environment under its own
contract with that environment, and hands back what the meeting produced. Physics,
protocol, timeout, and failure mode are settled on the far side of the grip and reach
this program already rendered as data. Nothing here holds a contract with the world
itself, at any depth.

**Opacity is what makes that true, and it does three jobs rather than one.** The gate
carries octets it must not read, opaque both ways. Stated as confinement, that is the
familiar reason: a boundary cannot leak what it cannot parse. It is also why a tool
call is indistinguishable from any other output, which is what keeps the gate from
growing a tool vocabulary. And it is why the boundary is relocatable: **a gate that
parsed content would have to know what kind of thing produced it**, and because it
does not, it cannot tell a human client from an appendage. The same crate therefore
sits unchanged wherever the outermost boundary happens to be. That property is a
consequence and not a plan, and this program builds nothing to exploit it.

What it buys is that the locus of regulation moves up to the agent. Because an
agent is an operating-system user, what it may touch is bounded once, at the
agent, by the kernel. The boundary that does the bounding is the operator's
artifact, authored before an agent exists by whatever means that operator's site
admits a principal, and the program verifies it rather than authors it, per
`weaver-admin-PRD` section 1: a program that admitted principals would be
raising a second trust model above the one it claims to inherit. The
alternative is to distribute components across a
network and regulate every seam between them, so each component carries its own
policy layer and the composite behavior is whatever those layers happen to add
up to. Regulating the principal is cheaper to reason about and cheaper to audit
than regulating every seam, and it is a discipline with decades of tooling
behind it rather than one invented for the purpose.

**The trust model, stated once.** The program secures the agent's record against
the agent and against nothing stronger. The operator is trusted by construction:
the operator admits the principal, writes the boundary, declares the sink, and
holds what accumulates behind it, and every custody argument in this corpus is
exclusion of the agent rather than evidence against the holder. A charter that
appears to defend an artifact against its operator is misread.

**The process topology, stated once so 5.1's test has something to read.** An agent is
four processes, with its supervisor arriving as an invocation rather than standing
beside them. The state member is its own binary, which admin stands at the load beside
the worker, per `weaver-state-Spec` section 2. The worker is the composition root: its binary compiles the harness,
`weaver-trace` under its contract, and the floor, and it binds the coordination socket
admin dials, per the inversion of 2026-08-05. The SPU and the gate are each their own
binary, forked by the harness during enter and holding one channel end each from their
first instruction. `weaver-admin` is one agent's own organ, its management plane beside
the gate as its data plane, per the operator's ruling of 2026-10-01: it compiles into
its own processes and is never linked into a worker. Those processes are the operator's
verb invocations against that one agent: one runs, dials, directs, and exits, so the
supervisor is outside the worker's address space and intermittent in time. Managing
several agents is not an organ's and belongs to WeaverWeb or a separate application,
which drives each agent through its own admin. A crate calling a crate inside one of
those binaries is a link seam. Anything else crosses a process line and is a socket, per
5.1.

This is systems architecture at the OS level rather than at the network level,
and the difference is not stylistic. At the network level the unit of
composition is a service, the unit of trust is a credential presented across a
wire, and every seam costs a round trip. At the OS level the unit of
composition is a process, the unit of trust is a UID the kernel enforces, and a
seam costs a socket write. Co-location is what makes a low-latency interior
possible, and per-agent OS identity is what makes a co-located interior safe to
have. Neither half works without the other, which is why they are the two
things this document refuses to trade away.

## 13. Where the trade does not yet pay

The trade in section 12 is not evenly justified across the program, and this
document is the wrong place to pretend otherwise.

A proto-stateful agent that holds nothing across sessions is precisely the workload a
network architecture serves well. Holding nothing, it is replicable by construction,
and replicable workloads are what horizontal scale, failover, and rolling
replacement exist for. Measured at stage one and nowhere else, giving up
redundancy in order to co-locate a proto-stateful agent is the worse
engineering choice, and no amount of saved latency repays it. A local
proto-stateful agent is not a better version of a networked service of the
same shape. It is a worse one with a faster interior.

The trade pays only once the agent holds state bound to it. At that point the thing
redundancy would protect is unique by construction and cannot be replicated regardless
of architecture, so the reliability being given up was never available. **State
management carried across sessions under custody is the first step on which the trade
starts to pay**, per section 2: one embedded store per agent, bound to it, which a
shared database server would have broken, and which is why the operator's ruling of
2026-10-02 retired that engine. The latency being bought stops being a per-request
saving and becomes the budget a continuous coordination loop runs inside.

So this document specifies a floor and not a product. A proto-stateful local
agent is a defensible intermediate and an indefensible end state, and building one
without building past it would be paying the full cost of this architecture for
none of what the cost is for. That is an accounting of where the costs land,
not a claim about what gets built next.

The order, however, is not a staging preference. Statefulness is a property
added to a working turn rather than a substitute for having one, so a stateful
agent cannot be built before there is a proto-stateful agent to make stateful.
The
prior attempt is the evidence. It built the memory leg and the turn loop
concurrently, so every change to either had to stay coherent with the other
while both were still moving, and its final phase deleted roughly nine thousand
six hundred lines against four thousand added across five separate retirement
commits. Stage one is therefore not a stepping stone that a better-resourced
program would skip. It is the only order in which either thing gets built.
