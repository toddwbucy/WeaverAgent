# weaver-harness - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. The crate PRD set is
written together and merged together, and no Spec is written against any member
before the whole set is merged. **Ratification became per-charter on 2026-08-23**, per
Working
Process section 2 as amended, so this document is ratified on its own terms rather
than waiting on the set. The set-wide act of 2026-08-04 established the pattern it
conforms to.

**Date filed:** 2026-07-28
**Document ID:** `weaver-harness-PRD`
**Parent:** `weaver-agents-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #623

---

## 1. What the harness is

**One agent's interior coordinator.** One harness, one agent, one process, one uid.
The harness is a constituent organ of an agent rather than a resident service that
agents are loaded into and unloaded from. An agent whose harness outlives it is not
an agent, it is a session.

```graph
node: weaver-harness
kind: crate

edge: parent
from: weaver-harness
to: WeaverTools
```

This is load-bearing rather than pedantic. The apex rests the entire regulation
model on the agent being an operating-system user, and that boundary only means
something if the user owns the whole assembly rather than a slot in something
shared. A harness serving many agents would put the regulated behavior of several
principals inside one principal, which is the arrangement the architecture exists
to avoid.

The harness holds control. It holds no substrate storage, binds no socket work
arrives on, and implements no model of any kind. The one name it binds is the
coordination socket admin dials, inward and inside the sandbox, per section 2.
Model weights are resident in the SPU and
reached over the decode socket behind a provider interface, so the harness drives
generation without hosting it.

**There is no privilege window.** The harness runs inside the worker process, and the
worker runs as the agent uid for the whole of its life. `weaver-admin-PRD` section 7
has admin holding no capability of its own and asking the init system to start the
worker as a transient unit carrying the agent's `User=`, so the process begins at
`weaver-<name>` and never holds anything above it. Everything the harness does happens
under that identity, and the descriptors arrive into it.

**Custody rests on possession of the passed descriptor and not on the receiving
identity.** An earlier reading of this section had the worker begin under the admin
principal, receive its descriptors, and drop, and it argued the ordering of that
drop. The drop is retired: the init system starts the unit at the agent identity, so
there is no interval in which worker code runs as anything else and the ordering has
no subject. What survives, and what this paragraph is kept to say, is the reason the
ordering never mattered. A descriptor passed over a Unix socket is a capability
rather than a permission, the kernel runs no permission check at the receiving end,
and possession is what custody of the sink rests on, so the worker appends to a file
its own uid could not open. This is the sink's mechanism and not the coordination
channel's, which authenticates by credential since the inversion. A charter that
says only that the
harness runs as the agent leaves the mechanism protecting the trace unexplained,
which is why it is stated rather than assumed.

## 2. What the harness owns

**Loop 0 is the running agent service, per the operator's ruling of 2026-08-05.**
It is not a document set, not a milestone to reach and pass, and not a loop a
builder supplies. It is the object itself: the thing that boots under its unit,
comes up as the statically provisioned agent identity, binds the coordination
socket inside its own sandbox and listens on it, creates the unnamed pairs its
organs are reached over, and sits there being one sealed agent. **It binds
exactly one name and listens on exactly one socket**, per the inversion ruling
of 2026-08-05: any socket connecting to the harness is an internal connection,
so the coordination socket lives inside the sandbox and this crate is the party
that creates it. It is dialable by construction and the credential check is what
refuses, root or refused, per `weaver-admin-harness-contract` section 2, where
the earlier form's unreachability did the refusing and could not tell an elected
tool from the worker. Nothing else here is dialable: the organ pairs have no
names. **That is why it is seated in this crate directly and not in the loop
container.** The container under `Loops/` holds the agent's internal logic,
which the running loop 0 service executes, and a loop 0 filed beside those
documents would read as one more supplied loop rather than as the service that
executes them. **There is one builder-facing surface, it is limited to loop 1
and above, and it excludes loop 0.** It is the extension seam of section 6, and
it has two stages rather than two forms: a builder develops against it as a
loop file that a Python-iterating worker reads per crossing, and deploys by
compiling that loop into the worker binary. Loop 0 is the service that runs a
builder's loop at either stage, and is not itself supplied through that
surface. **The socket-binding surface is not coming**, per the operator's
ruling of 2026-09-15. An earlier form of this paragraph named a second surface
beside the seam, a builder's loop standing in a separate process and reached
over a socket, dropped in beside a running agent, and no such proxy loop is
built. The development path and the deployment path are the one seam at two
stages, so a loop is composed into the worker it runs under rather than bound
to from outside it.

**Loop 0, and the machinery loop 1 composes against.** The harness owns the
lifecycle interior of loop 0, the load and the unload, the same for every agent,
together with the engine machinery a loop is built from: tool dispatch, batch
partitioning, and the surfaces the rest of this section names. The loop itself,
loop 1, is the builder's, per the composability ruling of 2026-08-02: written at
the worker composition root against what this crate exposes, which is the
extension seam the children list of section 6 names. Which loop an agent runs
is the agent's own declared fact, per the operator's ruling of 2026-08-20: the
loop is a member of that agent's harness and unique to it. A worker whose loop
is compiled carries it in the binary its unit starts, and a worker that reads
its loop from a file runs the one the agent's declaration names. A compiled
worker handed a declared loop file refuses at its own argument parse and the
load fails loudly, because a declaration the installation cannot honor is an
error to surface rather than a fact to ignore, the residual-readout rule's
own reasoning one field over. Absence means
the worker's own default, the compiled body or the installed file, which is
what lets every declaration written before the member stand. The earlier form
of this sentence called the loop a provisioning fact rather than a
configuration field, and the ruling retires that reading where the loop is a
file: two agents drawing one shared file differ only by conditionals inside
it, which contaminates any comparison run between them, and separating
experimental arms is exactly what a per-agent declaration is for. What holds
for every loop 1 alike is loop 0's bracket discipline: the operator's interrupt
arrives as the stop exchange of `weaver-admin-harness-contract` section 3,
aborts whatever is in flight, and returns the agent to loaded and idle,
whatever the loop's own semantics. The basic loops this program ships are
demonstrations built by the same path, native in the same way, and they run
until the model returns a final answer or that interrupt arrives, which
describes the demonstrations rather than constraining a builder's loop, a
judgment loop or a fixed-turn loop being licensed by the same ruling.

**What drives loop 1, and what only clerks, per the operator's ruling of
2026-08-12.** The seat is granted on work that arrives owed an answer, and on
nothing else. Between enter and leave the harness waits wherever a flow can
originate, and what an origination means is each seam's own contract's to
say. On the decode seam the direction of expectation is stated at
`weaver-harness-spu-decode-contract` section 2: every ask originates in the
harness and its answer returns what the ask produced, and the SPU's one
emission is a report owed nothing back at all and never an exchange, so
nothing interior to the agent originates work. The coordination seam carries no work
by its own contract. What remains is the boundary seam, whose turn exchange
is the one inbound flow that arrives owed an answer, so a turn begins at the
gate and nowhere else, read from the shipped contracts rather than
legislated over organs not yet chartered. A later organ's contract states
its own dataflow rule at its own chartering, and what such a rule licenses
is that act's question rather than a corner this one closes.

The invocation follows the criterion, and the wait stays loop 0's. The
reasoning-loop boundary has the loop calling through the gate rather than
sequencing it, and the agent's outer wait is content-blind coordination,
exactly what that document excludes from the loop. So loop 0 waits, and what
arrives is dispatched by what it is: a directive is the lifecycle
interior's, a report is clerked to the record with no turn opened and
nothing answered, the trace entry being the acknowledgment per the decode
contract's second ruling of 2026-08-12, and a frame grants the seat, loop 1
entered with the turn's content and the granted surface, the response
returning through the same exchange when the seat comes back. The mechanics
are the Spec's, at its section 6.2.

**The tool system.** The registry, the execution context, and the permission modes.
Permission modes are operator policy and not a safety boundary. What a tool can reach
depends on which side of the port test it sits, per the port ruling of 2026-08-07. A
tool that binds a listening port is external, a registered application the harness
addresses across the gate's agent-opened seam, and its containment is that application's
own and the operator's to provision. A tool that binds none is internal, loop code this
crate runs, and what it can reach is bounded by the kernel through the identity the
worker holds, which is what this paragraph said before the egress act read it outward
and is true again. The mode setting governs only whether the operator wants to be
consulted before a class of action. Stating this plainly is the point. A permission mode
that reads as a security control when the kernel is the actual control is a thing that
gets trusted wrongly later.

**Prompt assembly's deterministic floor, with the family render across the seam.** The
harness composes the canonical conversation: the identity prefix, which is the system
prompt together with the agent's fixed identity material, read from state where a member
stands and from the declaration's seed where the store holds none or no member stands,
per `weaver-state-PRD` section 4 as ruled 2026-09-04, then the session's message
sequence read from the working structure, then the tool schemas, in that order, always.
What moved on the framing ruling of 2026-08-02, ratified with the token workflow's act,
is the per-model half: the family template's application, the phrasing layer that
reliably elicits a tool call from one decoder and not another, seats in the SPU's family
library per `weaver-spu-PRD` sections 13.4 and 14, because family knowledge lives in one
home and the harness links nothing of any model's. The harness sends canonical messages,
the family library renders, and the rendered reality returns on the report path,
template identity, token identifiers, and block partition, so the harness authors what
the model saw without having rendered it. No model is a build or run dependency of this
crate, now stronger than a discipline: the per-model knowledge lives across a seam
entirely. The deterministic assembly floor stands alone, always. Under a restoring load,
per the ruling of 2026-09-04 on issue #432, the recorded conversation through the cut is
prefix material beside the identity, seated once and permanent for the residency,
because a session standing from a record is conditioned by that record the way every
session is conditioned by its system prompt.

**Decode against a resident session.** The harness issues decode requests over the
decode socket against a resident KV session rather than resending the conversation
each turn. The identity prefix is rendered once per session and held resident. Each
turn appends only its delta. The harness owns the flush decision while the SPU owns
the cache, so "flushed on the harness's terms" is an obligation of the decode seam
rather than a local policy. Permanence of the identity prefix is an invariant of
that same seam, honored by the SPU, not a guarantee this crate can hold. The
harness does not touch the cache and so cannot protect any region of it.

The alternative this rules out is resending the full message history every turn.
The previous tree ran production on that path and measured a single exploration
turn climbing from 5,988 to 24,932 prompt tokens, with a concurrent request timing
out because the accumulated context made every call slow. **Proto-stateful in
this program means the agent begins each session with no accumulated experience,
holding real state only within one, per apex section 2. It does not mean the
decoder is driven statelessly.** The previous tree used the old word stateless
for cold full-history resend, the opposite architecture, and the rename of
2026-08-01 retires that collision along with the overstatement.

Assembly is also where the model's view of the trace is settled. The harness
reasons over an in-RAM working structure that holds the whole trace, and it renders
from that structure only the message sequence. The measurement events, the
lifecycle events, and the custody records stay in the harness's working state and
never enter a prompt. This is a discipline of the engine and not a property of the
model, and it is the seam a later recall feature would breach by rendering the
record back into context. Trace content reaches the model as the ordinary
conversation and in no other form.

The message model is provider-agnostic. The concrete transport is constructed at
the worker composition root and injected, so this crate names no wire format.

**Trace authorship.** The harness is the sole writer of the trace and the sole
broker of access to it. This is a first-order responsibility of the crate rather
than instrumentation attached to one. Section 5 governs it.

**Activity control.** Starting, stopping, cancelling, and interrupting a run inside
a loaded agent. Every one of these returns the agent to loaded and idle. None of
them unloads it. Activity is the only lifecycle layer the harness owns.

## 3. What the harness does not own

**The lifecycle transition goes to `weaver-admin`, and the fan-out inside it comes
back.** Authorizing a load or unload, opening the record, starting and stopping the
worker unit, rolling back what its own acts built, and supervising worker and gate
lifetimes are `weaver-admin`'s, which the operator runs with root, one invocation per
verb, per that charter's section 1 as recut on 2026-08-05. The party that is
long-lived where the harness is mortal is the init system, which holds the unit and
outlives every invocation that drives it. The harness is one of the things a load
assembles, not the thing that assembles it, and it cannot drive the early steps of
its own creation, because the worker spawn runs before the harness is running as the
harness at all. What the
harness does own is the interior of the enter and leave directives: admin holds one
seam and no channel to the SPU or the gate, per `weaver-admin-PRD` section 6, so the
harness fans admin's directive out along its own seams, collects each organ's
confirmation, and returns one aggregate. Sequencing the organs is the harness's
because the seams are, and the previous tree carried roughly four and a half thousand
lines of multi-agent coordination inside the opposite reading.

**Network ingress goes to `weaver-gate`.** The harness binds no socket the world
can reach and has no first-contact surface. Work arrives already authenticated.
**The coordination listener of section 2 is not a counterexample and the
distinction is the direction.** It faces admin rather than the world, it lives
inside the agent's sandbox, and it admits root alone, so no work and no external
principal reaches it. The gate holds the doors that face out and this crate holds
the one that faces in, which is each owning its own side rather than two owners of
one. The outward count became two with the egress ruling of 2026-08-07 and the
inward count is unchanged.

**Boundary verification and lifecycle supervision go to `weaver-admin`.** Provisioning
is the operator's rather than admin's, per `weaver-admin-PRD` section 1, and there is
no privileged startup window to assign. The operator produces the configuration file and
admin reads it, per `weaver-admin-PRD` section 2: this crate receives what the enter
carries, the sink's descriptor beside it, and the loop's path on the unit's vector, and
holds no parser and no path. **The `reads` edge this paragraph declared to
`agent-config` retired on 2026-09-04, per issue #311**, because it recorded a read the
crate does not perform and no chartered workflow exercises, which is the reserved slot
in graph form. It returns, declared here again, with the tool workflow that consumes the
tool set and the permission mode, on the day that workflow is chartered and the read
exists.

**Encoding and decoding both go to `weaver-spu`, and so does everything that
serves them.** Model residency, GPU arbitration, decode compute, embedding compute,
and the KV cache are that crate's. The encoder is not a harness component that
happens to live near the decoder. It is the other half of the semantic processing
unit and is held to the same discipline: the same residency accounting, the same
GPU arbitration, the same lifecycle confirmation.

**The harness routes.** It sends work to the SPU and receives tokens or embeddings
back, and which of the two it asked for changes nothing about the seam. It holds no
weights, performs no forward pass, and has no in-process model of either kind. A
harness that hosts an embedder for latency reasons has taken half the SPU into the
interior and given up the arbitration that made residency accountable.

The harness decides when a KV flush happens. It does not hold the cache and does
not reach into it. Eviction granularity is constrained by the append-only session
protocol and is settled in that crate's PRD and the contract, not here.

**The harness also names the span an elision removes**, per the operator's
rulings of 2026-08-22 and `weaver-spu-PRD` section 13.13. The same division
holds and is the reason the decision sits here: which part of a context is
worth keeping is cognition, the SPU judges nothing, and naming positions is
this crate exercising a possession it already had rather than reaching into
a cache it still does not hold. **The span is state and never the record.**
What the elision removes is the SPU's resident sequence, and this crate's
other obligation is unchanged by it: the trace gains an event and loses
nothing, because nothing in this program removes anything from a record.

**This crate writes the span it named into that event.** The SPU is the
authority on the resident counts and this crate is the authority on the
ask, so the record carries both and a consumer can see them disagree. The
span is what makes the event replayable: an interior removal reporting only
counts says how much went and never which positions, and the reconstruction
`weaver-trace-PRD` section 3.2 promises needs the where.

**The trace primitive goes to `weaver-trace`.** The span type, the event schema, the
stream mechanics, the working structure, and the export
formatters are that crate's. The harness authors events. `weaver-trace` defines
what an event is and what it costs to commit one.

**The trait contracts go to `weaver-traits`, and the config file format to
`weaver-types`.**

**Safety adjudication of tool input is not a harness function at all.** The harness
does not inspect a command to decide whether it is dangerous. There is no classifier
here and none is coming, because a classifier would be a heuristic standing where a
boundary already stands.

**Where that boundary stands moved with the egress ruling of 2026-08-07.** This
paragraph said the harness executes the tool as the agent's constrained user and the
kernel decides what that user can touch. Apex step 7 now has the harness address a
registered tool across the gate's agent-opened seam rather than fork one, so what the
harness does is reach a boundary rather than run inside it, and what a tool may touch
is that application's own affair on the far side of the socket. **The seam this
requires is chartered and its contract is not written**, per apex section 3 step 7, so
this charter states the direction and owes the rest to the tool workflow: the exchange
that carries a tool call toward the gate, this crate's part in it, and what a tool's
death mid-call means are all that workflow's and none of them is shaped here.

**Memory in every form is out of scope.** No recall, no consolidation, no
surfacing, no substrate. When memory returns it returns as a socket peer under its
own contract, per apex 5.1, and this crate gains a seam rather than a dependency.

## 4. The seams

The harness is party to eight seams by this section's own enumeration, and every one
of them is governed by a named contract, per apex 5.1. **The enumeration counted six
until this act, and the gap it disclosed was two seams rather than the one it
named.** It named the classify seam and owed itself the act that would carry it, and
this is that act. What it did not name is the seam the decoder cut of 2026-08-02
landed: this section carried one seam to `weaver-spu`, called it the decode seam, and
resolved it through `weaver-harness-spu-contract`, which is the residency seam's
contract, so the token seam was named in the table and absent from the count. Both
are `weaver-spu`'s and both are carried below. Six cross a process line and are Unix
sockets that authenticate their peer, by credential where the channel has a name and
by possession where it has none. The coordination seam is a named socket this crate
binds and authenticates by credential, root or refused, per
`weaver-admin-harness-contract` section 2 as of the inversion of 2026-08-05. Four are
unnamed pairs this crate creates before the far end exists and authenticate by
possession: the boundary pair, the residency and decode pairs whose far ends the SPU
child carries, and the classify pair of the classify process, which this crate starts
beside its siblings per `weaver-spu-PRD` section 15.3. **Not all four are organ
forks**, the classify process being no organ and its seam no organ channel per the
record paragraph of this section, so what the four share is the creating act rather
than the kind of peer at the far end. **Three of them stand in every run and the
classify pair stands in some**, that submodule being optional by presence, so an agent
declaring no classifier has this crate creating three. The seam is in the count all
the same, this enumeration counting the seams the corpus holds and not the channels a
run opens. The sixth is the state seam of the 2026-08-18 charter, an unnamed pair
since the operator's ruling of 2026-08-26: admin creates it at the member's spawn and
this crate receives its end inside the enter directive, beside the sink, so it
authenticates by possession like the pairs this crate makes itself.

The seventh and eighth seams are the ones to `weaver-trace` and to
`weaver-diagnostic`, the two record mechanisms this crate authors through. Neither
crosses a process line, so both are library boundaries tagged `link` rather than
`socket`, and neither authenticates anything because there is no second process to
identify. They are seams under contracts all the same, which is why the count here
is eight and the table below is six.

**The egress ruling of 2026-08-07 adds no seam here, and the socket count above it is a
claim worth checking rather than an omission.** Apex section 3 step 7 has the harness
address a registered tool *through the gate's* agent-opened socket, so this crate's
counterparty is the gate it already holds a seam with, and what is new is a direction
on that seam rather than a party. The tool is the gate's peer and not this crate's,
which is why no row below is the tool's and why nothing here authenticates a tool. **The
exchange that carries a tool call is owed by `weaver-harness-gate-contract`**, which
closes its enumeration at five and says in its section 7 that the clause is drawn
again on the day that exchange arrives, so this seam's traffic is chartered ahead of
the document that must admit it.

Every request carrying work across these seams carries the turn context and returns
it, per apex 5.2. Lifecycle directives on the coordination seam carry no turn context
because they belong to no turn, per 5.2 as scoped by the re-authoring of
2026-08-01.

**The three rows to `weaver-spu` carry this document's own words for those seams, and
two of the three are named for the infixes their contracts carry.** The residency row
is the exception. Its contract is `weaver-harness-spu-contract` and holds no infix,
the party rule alone having named it while it was the only seam between this pair, and
`weaver-harness-spu-decode-contract` section 0 records that the residency contract's
name stands unchanged where the two later names take one. `weaver-spu-PRD` section 6
is the authority on that crate's seams and names the same three residency, token and
label, in that order.

The six sockets:

| Seam | Peer | The harness's role |
|---|---|---|
| Boundary | `weaver-gate` | Receives authenticated work inbound. Opens the exchange that carries a tool call outbound, per apex step 7, which the gate relays to the registered tool and never interprets. Gate never reaches past this seam in either direction. |
| Residency | `weaver-spu` | Hands over the model binding it was given to admit, and later asks for its release. One arm of the enter fan-out and one of the unwinding. Consumes the confirmation or the typed refusal, which travels into the aggregate unchanged. |
| Decode | `weaver-spu` | Opens the resident session, appends each turn's delta, and issues the flush. Requests carry `turn_key` and `session_key`. Consumes the response and its measurement payload. |
| Classify | `weaver-spu` | Sends content with the turn's trace context to the classify submodule and consumes every label that artifact defines, each scored. The submodule's readiness and its faults arrive here. The submodule is optional by presence per `weaver-spu-PRD` section 15.3, so an agent declaring no classifier stands no end of it. |
| Coordination | `weaver-admin` | Receives lifecycle sequencing, the trace descriptor of section 5, and the operator's intent to stop. Reports readiness, confirmation, and the turn's fate on a stop. Opens no exchange of its own, the fault travelling as a `fault` event on the stream per the fault-carrier ruling of 2026-08-01. |
| State | `weaver-state` | Sends the tee's election at the channel's opening, the distillate stream after it, and its loops' asks on the same channel, each answered once against the holdings the stream carried before it. A dead peer costs the distillate or the answer and never the turn, per the contract's failure vocabulary. |

**A fault the worker survives is a `fault` event, and the stream is its carrier.**
The fault-carrier ruling of 2026-08-01 retired the alert exchange: with one
outbound path carrying every event in order to the operator's sink, a second
carrier for the same fact earned nothing. The harness authors the fault like every
other event, no run blocks on anything downstream of the emission, and the
operator's tooling keys on the fault fields and comes back by running a verb, per
`weaver-admin-operator-contract` section 6.

The harness links `weaver-traits` and `weaver-types` as floor vocabulary, and links
`weaver-trace` and `weaver-diagnostic` as members of its own domain under
contracts. **It links no other internal crate**, the second record mechanism having
joined that list 2026-08-27 with the act that wrote its contract. The four are one
dependency surface and two classifications, which is what the blocks below project:
two `floor-link` records and two `seam` records tagged `link`. Calling all four
floor vocabulary would use the word `weaver-agents-PRD` section 5.1 reserves for
what every domain draws from and no domain contains. That is the whole
dependency surface, and it is checkable against this list.

**A second member stands beside the trace and is not a link.** `weaver-state`
is a member of this domain reached over a socket, per apex section 9's
re-entry door and its charter's seam section, so it adds a `seam` record and
no dependency: nothing of it is linked, and the dependency-surface claim
above stands unchanged. The harness declares the edge as the asking side,
the contract carrying the traffic's whole shape.

```graph
edge: seam
from: weaver-harness
to: weaver-state
via: weaver-harness-state-contract
tag: socket
```

**A third member joins the trace and is linked the same way, its contract landed
2026-08-27.** `weaver-diagnostic` is the mechanism this crate authors a
diagnostic-trace through, per `weaver-agents-PRD` section 6 as ruled
2026-08-24, standing to a diagnostic binding as `weaver-trace` stands to a
serving one. It is linked rather than dialed on the trace's own ground: apex
section 5.1 forbids a behavior reached by path dependency **across a process
line**, not a crate calling a crate inside one binary, and this call crosses no
process line. The harness does ask this crate to record, which is what makes
the record a `seam` rather than a `floor-link`, per Document Format section 4,
whose own example is this pair's sibling. **Its `seam` record tagged `link` is
declared below as of 2026-08-27**, the act that wrote
`weaver-harness-diagnostic-contract` and the member's Spec landing the `via` this
record needs and the surface `weaver-harness-Spec` section 9 held open.
**The graph and the manifest parted here on purpose and have since rejoined.** The
edge is declared by this document and the dependency by a manifest, so the graph went
to four with the act that wrote the contract and the manifest went to four with the
act that built the member. Both have landed. The member is built, the workspace
carries it, and this crate's manifest declares it, so the dependency-surface claim
above reads four crates the corpus links and four the tree compiles, with no split
left to disclose. `weaver-harness-Spec` section 1 reads the same four from the side
that reads against the manifest.

```graph
edge: seam
from: weaver-harness
to: weaver-diagnostic
via: weaver-harness-diagnostic-contract
tag: link
```

```graph
edge: seam
from: weaver-harness
to: weaver-trace
via: weaver-harness-trace-contract
tag: link

edge: floor-link
from: weaver-harness
to: weaver-traits

edge: floor-link
from: weaver-harness
to: weaver-types
```

The seams to the organs carry no records here, and the state seam is the one
socket seam that does. The organ rule of Document Format section 4 puts an
organ channel's declaration on the organ's side, so the boundary, residency and
coordination seams each resolve through their contracts with no record
declared here on any organ's behalf. **The decode and classify seams are not
organ channels**, per `weaver-spu-PRD` sections 13.2 and 15.2, so that rule does
not reach them and the Format's other branch does: on a seam with one asking
party the asker declares, and this crate is the asker on both. Their records
stand in `weaver-spu-PRD` section 6 all the same, each contract's section 0
putting the edge on the organ's side in its own words. **That is a disagreement
between the Format and the corpus, named here rather than settled.** A contract
does not decide a notation question the Format owns, per G2, and a charter act
that amended the Format to match would be the same level violation running the
other way, so the placement is owed a ruling and this document declares no
record that would double either edge. The state seam is a member seam rather
than an organ channel, the asking side declares, and its record stands in the
dependency block above with `weaver-harness-state-contract` as its `via`. A
seam without the contract that governs it fails G3 rather than passing
incompletely, and none is in that state now. The diagnostic member was the last
one that was, its relation standing while its edge waited on
`weaver-harness-diagnostic-contract`, and the contract, the edge and its `via`
landed together 2026-08-27 as the paragraph above said they would. The seams as
they stand: the boundary seam, named turn ingress until the egress ruling gave it
a second direction, resolves through `weaver-harness-gate-contract` as of the gate
pair's merge on 2026-08-01, and the SPU's three through
`weaver-harness-spu-contract`, `weaver-harness-spu-decode-contract` and
`weaver-harness-spu-classify-contract`.

The coordination seam is no longer in that state. `weaver-admin-harness-contract` is
written and declares the seam from admin's side, per the Document Format rule that on
an organ channel the organ declares. That contract is the `via` this seam
resolves through, and no record is declared here on admin's behalf.

## 5. Trace authorship and custody

The trace is the primary artifact and this crate authors it. Three rules govern
that, and they are separable.

**Verification sits outside the loop, on latency grounds alone**, absorbed here from
`verification-placement` on 2026-08-23 when that document was retired into this one.
The placement says nothing about verification's importance. It is the loop's own cost
model applied: a check on the hot path is paid on every event, and the loop's named
purpose is processing meaning rather than auditing it.

**What holds instead is split by when it runs.** At author time, the vocabulary
guard: this crate authors every event and enforces the kind-to-payload pairing and
the payload's conformance at the one submit call, which is where the splice's opacity
is paid for. After emission, the observability consumer: the record leaves through
admin's egress and faithfulness is checked where the record lands, **outside the
process whose account it is.**

**Corrective mechanisms are allowed for and none is built.** Nothing here precludes
admin-level sockets for verification, remedial context, or other correction. What is
ruled is that such mechanisms and their monitoring are external and secondary to the
loop's named purpose. One consistency fact binds any of them: the coordination seam's
contract guarantees that no directive carries work of any kind, **so a
corrective-context path cannot ride the existing admin seam.** It arrives as a new
socket under a new contract, which is the feature-add pattern this corpus already
mandates. **Nothing is reserved for it meanwhile: no slot, no field, no dormant
party.**

The security model this rests on is the apex's role-scoped socket handoff and is not
restated here.

These rules defend against one adversary, the model reaching for its own trace
through legitimate operation, whether an elected tool call or a recall path that
renders the record back into context. Against that adversary the walls below are
complete, because the model receives only what the harness hands it. They do not
defend against a compromised harness. Owning the deterministic engine is owning the
agent, and no trace custody changes that. That risk is bounded where multi-agent
isolation is bounded, at the operating system, by uid separation and least
privilege. Custody stops what the model can reach through normal operation. The
kernel stops what a compromise would reach. Neither asks the model to be
trustworthy.

**The harness authors, components report.** No other component writes to the trace.
The SPU returns its generation and measurement payload tagged with the `turn_key`
it was given, and the harness writes `model.request`, `model.output`, and
`model.measurement`. The harness writes `turn.started` and `turn.closed` around
every turn, the message events, and the tool-call events. It records the
`load` event that opens each run, which for a session's first run is both that
session's first entry and the record of `weaver-admin`'s initial contact. One
emission per event,
authored against the durable event schema, feeding the outbound stream and the
working structure together.

**Sole writer means the harness authors whatever occurs, and filters nothing.**
`weaver-trace-PRD` section 5 carries no recording level since the ruling of
2026-08-02, so there is no class of event the harness declines to emit and no
policy for it to apply on the recorder's behalf. What an operator elects at load
governs what the agent produces, the residual readout being the reference case, and
the harness authors what production yields. A later run finding the file changed
adopts the new value as its own load condition, which is the mechanism working
rather than a conflict to refuse.

```graph
edge: writes
from: weaver-harness
to: session-record
```

**One emission, one rendering, two holders.** The outbound stream and the working
structure hold the same canonical rendering of the same authored emission, per the
ruling of 2026-08-01 that retired the relational projection: nothing is derived, so
nothing can diverge and no reconciliation is owed. The durable event schema is the
only schema, and there is no second representation and no view this program takes
responsibility for: a consumer who wants rows, an index, or a front end builds one
on the output, on the consumer's own compute.

**Nothing on the turn path waits on the sink.** The harness reads its own trace from
the working structure in RAM. Even a fast sink is too much latency to block a turn
on, so the stream write runs off the hot path and a slow or failing sink never slows
the interior read. The two are failure-isolated. Under pressure the stream side
reports its growing queue rather than absorbing it, `weaver-trace-PRD` section 4.2
leaves it no cadence to elect and no window to tune, and what may happen next is the
marked election of `weaver-admin-operator-contract` section 3. It may never shed
silently, because the trace is measurement data and a silently partial account
renders a plausible and wrong story of the turn.

**Custody is structural, not policy.** The stream's sink is opened by
`weaver-admin` under root, the role's principal, per
`weaver-admin-operator-contract` section 3, and the record the stream accumulates
lives on the operator's side of it. So the record is readable there with no
harness involved, replay and audit are
the operator's acts over the operator's storage, per `weaver-admin-PRD` sections 7
and 8, and the agent that produced it holds nothing of its own.

**The agent holds no route to the sink, twice over.** The first lock is the
operator's provisioning, where the sink is a file: ownership, mode, and the
directory's search bit deny the agent uid the path, verified by admin at load, so a
tool the agent elects, `bash` above all, cannot traverse to it. The kernel refuses
the path lookup before anything else is consulted. The second is the descriptor
discipline. **The harness never resolves a trace path.** The descriptor is opened by
a `weaver-admin` principal and passed to the worker over the coordination socket
using `SCM_RIGHTS`, so the harness receives a handle, never a path, and writes
through it. The agent is never told the name and could not act on it if it were.
Neither lock is redundant, because they answer different adversaries. The
provisioning stops the agent's tool surface from reaching the sink. The descriptor
discipline stops the harness from being the leak, since a component that never
resolves a path holds no path to disclose and cannot be induced to open one. The
write capability rides the descriptor rather than the mode, so custody takes
nothing the agent needs for its own home directory and work.

**The trace descriptor is close-on-exec, and the crate is broken without it.**

Without `O_CLOEXEC`, every subprocess the harness spawns for a tool call inherits a
writable handle to the trace, and the agent edits its own audit record through a
descriptor requiring no path and passing no check.

The flag rides the descriptor and does not cross with a passed handle, so the
receiving call is the only place it can be supplied and supplying it is this crate's
obligation rather than the sender's. `weaver-admin-harness-contract` section 5 holds
that obligation.

It is invisible at runtime until exploited, and a test that opens a trace normally
will never see it. **It is not reachable by a compile-time pin.** A behavior on the
receive path is not a type property. What a pin does reach is the shape of the
receive: one receive site, taking no flag argument, returning a handle the rest of
the crate cannot construct another way. That pin is real and it is a different claim.
The flag itself takes the perturbation-verified test of apex section 11, and the test
counts only when it has been watched to fail with the flag removed.

**What this crate itself raises, which the organs' enumerations cannot
cover.** The harness is an organ too, per apex section 5.4, the one whose
domain is coordination, so the `fault` event's case set is incomplete without
its sources and they are named here with the organs': the recorder surfacing
commit pressure while the sink stays writable, a stream write failing against
a live process, an organ's death observed as channel closure after the
enter aggregate was answered, and a message-kind record of its own working
structure that does not decode at assembly, the hole authored as the fault it
is rather than handed to the model as context, and a seated identity prefix
the record cannot account for. The third is why this enumeration cannot be
derived from the organs' own: a dead party is exactly
the one that cannot report, so the SPU's cases at `weaver-spu-PRD` sections
13.10 and 15.6, one set per submodule since the classify submodule raises its
own, and the gate's at `weaver-gate-PRD` section 13.4, are silent about their
own deaths by construction, and the harness's observation is the only account
there is. The fourth was authored by the built crate before it was named
here, which is the enumeration read against standing practice and found one
short: the assembly fault predates the case set's typing and no case of the
three covered it, so the charter records the condition its crate already
raises rather than leaving the record's one unclassified fault standing.

**The fifth is the enumeration read against the seated prefix, and it is
named for the same reason the fourth was.** The identity door writes
`message.system` and refuses every other role, per `weaver-harness-Spec`
section 6, because a door that writes what it was not built for launders a bad
declaration into a record that looks well formed. That refusal is this crate's
own observation and it has nowhere to go: the declaration crossed admin's
parse, the prefix is seated at the session's open regardless, and the organ
that would report is the operator's file rather than a party on a seam. So a
dropped refusal leaves the record with no account of a prefix the model
received - which launders the declaration by omission, the door's stated
purpose defeated by the caller rather than by the door. A reader cannot
distinguish an agent that seated no prefix from one that seated an unrecorded
one, and a replay seats nothing and reports a divergence against the model for
a prefix the record never held.

**The case is the hole and not its cause**, which is why it is one case rather
than a family. The door refuses on three conditions - a role it does not
write, a system message carrying a block the licensing rule does not admit,
and a message that will not render - and past the door the recorder may
decline the write outright. All four end the same way, a prefix seated into
the decode context and a record that cannot show it, and all four mean the
same thing for the turn and the residency, which is nothing: the load stands.
The custody rule of apex section 5.2 puts exactly that judgment in the case
and leaves the rest to the account, so which of the four happened is the
account's to carry, and splitting the case would move an organ's descriptive
vocabulary into the floor.

**The load is not refused on any of them.** The seated prefix is the
operator's declaration and a run that has already bracketed does not die on a
record it could not write, so what is owed is the account rather than the
abort. The account is best-effort for the same reason it is owed: where the
recorder is what failed the account may fail with it, and a miss nobody could
write down is still not a miss worth aborting a run over. Where the rule that
judges a declaration lands is `weaver-types`' to say and not this section's.

With these five named the corpus-wide case set closes across all three
organs, and the payload's shape is the trace act's to elect against it.

**The harness reads, the model does not.** The harness reads the trace continuously,
because the working structure is its own copy of the record and every prompt
assembly is a read
of it. That is harness-internal work on the harness's own state. Sole broker names
the harness holding the only handle, not a service it offers the model. There is no
model-facing read path, and the pin in section 2 is what keeps that true: trace
content reaches the model as the rendered message sequence and in no other form.
This is what apex section 4 means by writing the trace where the agent cannot reach
it, stated precisely. The tool surface has no path to the file, and the model has no
path to the content.

## 6. Children

Specs to be written against this charter once the PRD set is ratified. Named here
so the set is bounded, not drafted here.

- The extension seam where loop 1 composes, and the machinery it composes
  against.
- Prompt assembly, including the per-model scaffolding this layer is re-erected
  from for each decoder integrated into the SPU.
- The tool system, the execution context, and permission modes as policy.
- Activity control, covering stop, cancel, and interrupt.
- Trace authorship, covering the event set this crate writes and the descriptor
  discipline of section 5.

Contracts this crate is party to are written with the PRDs of their other parties,
one per seam in section 4, and are not children of this document.
