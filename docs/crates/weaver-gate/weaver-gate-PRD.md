# weaver-gate - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth, per the human's ruling
of 2026-08-01 that a document on `main` is merged and not a draft. It reaches the
lifecycle half of this crate, the hook the enter and leave directives raise and
lower. The exchanges that carry work arrive with the token workflow.

**Date filed:** 2026-07-31
**Document ID:** `weaver-gate-PRD`
**Parent:** `weaver-agents-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #647

---

## 0. What this document is

The charter of the crate that holds the agent's mouth and ears. It is drafted together
with `weaver-harness-gate-contract`, which governs the one seam this crate holds, and
neither is complete without the other.

Level discipline, stated once. This document carries what the crate needs and why,
including the order in which the hook rises and falls, because that order is this
crate's own work rather than a wire agreement. What crosses the seam, what it means,
and how it fails is the contract's. How any of it is represented is the Spec's and
appears in neither.

**What this charter reaches, and what it holds open.** It reaches the two exchanges
the enter and leave directives of `weaver-admin-harness-contract` section 3 fan out
into on this seam, the hook those exchanges raise and lower, the boundary predicate at
the client socket, and the process facts a charter has to state before another crate
can build against it. It does not reach the traffic: the turn exchanges this crate
will open toward the harness, the framing a client speaks at the socket, streaming,
backpressure, drain on stop, or the fault cases a running hook raises.
Each of those is named as deferred in section 8 or as a cell in section 10 rather than
left out, because an omission and a deferral read alike to a later reader and only one
of them is a decision. Cancellation stood in this list from the drafting until
2026-09-22 while neither section named it, a gap `weaver-gate-Spec` section 7
carried, and it is chartered at section 13.5 as of that date.

The test that drew the line is mechanical. A clause not needed to make enter and leave
true is out of this pass.

```graph
node: weaver-gate
kind: crate

edge: parent
from: weaver-gate
to: WeaverTools

edge: floor-link
from: weaver-gate
to: weaver-types
```

## 1. What this crate is

**The agent's mouth and ears, and nothing else.** A local Unix socket hook on the
front of the agent: work in, response out, both passed through opaque, with no
translation and no opinions about content in either direction. Whoever connects gets
to converse with the agent and gets nothing else. There is no listening network socket
here and none anywhere in the program, per the demotion ruling of 2026-07-31, whose
apex correction is deposited on the correction list. What the earlier reading built as
a network face, protocol termination and a ruled dialect and a translation layer, dies
with that ruling, and what remains is the thinnest boundary that lets a local client
reach a loaded agent.

**A client can name the turn it received, ruled 2026-08-15.** A close that
answers a turn carries that turn and the run it belongs to, so a conversation
has references in it and not only content. A close that answers no turn carries
neither, a line that never parsed as a request having produced nothing to
refer to. **The need is the client's own and not an
instrument's.** A client that gets an answer and cannot say which answer it was
has nothing to put in a bug report, nothing to hold two of its own records
together with, and nothing to name when it asks for a turn to be stopped. Every
one of those is a question a client asks about its own traffic, and today the
only honest answer is the order the lines arrived in, which stops being an
answer the moment a turn is refused or stopped and the ordering shifts under it.

**Both are carried because one does not identify.** A turn key counts within its
run and restarts with the next one, so a client that spans a reload sees a
second turn wearing the first one's name. The run reference distinguishes runs
by construction, per `weaver-admin-PRD` section 10, and the pair is what names a
turn once.

**A name is not a capability, which is what makes this safe to hand out.**
Possession of either identifier admits nothing and authorizes nothing: every
seam in the program authenticates by peer credential and none of them accepts a
name as a reason, so what a client holds is a label for something it already
took part in. It gains no reach into the record either, the record being the
operator's on the far side of a sink this crate never touches. What crosses is
the ability to refer, which is the smallest thing that answers the need.

**It is an organ with its job simplified, and a domain root with no members.** The
organ test of apex section 5.4 is a domain and a two-initiator channel with the
harness, both properties and neither alone, and this crate passes it: its domain is
the agent's external boundary, the crossing that brackets every turn, and its channel
with the harness has two initiators, this pass chartering the direction the harness
opens and the turn direction arriving with the token workflow, the same half-chartered
shape the SPU carries. What the demotion kills is the acquired-parts argument, protocol
termination and authentication and translation as future member crates, so the root
holds one PRD and no subtree, which the Document Format's depth rule names as the
ordinary state of a root whose members have not been discovered. Admin brackets the
run and this crate brackets the turn, the same mechanism pointed at two scopes, and
neither is a member of the domain it borders.

**It writes nothing.** The harness is the sole writer of the trace, and both of a
turn's crossings are witnessed by the harness's authorship, the open off the inbound
crossing and the close at the final answer, per `weaver-harness-PRD` section 5. A
logged pass-through means the crossings land in the record, never that this crate
logs.

**A tool reaching outward crosses this crate, as of the egress ruling of 2026-08-07.**
This clause said the opposite until that ruling: an outbound connection made by a tool
under apex step 7 did not pass through this crate, the hook being about what enters and
the only thing entering being work. Apex step 1 now reverses it, and the hook is about
what crosses in either direction, with the agent-opened seam of section 2 carrying the
outbound half.

## 2. What this crate owns

**The hook.** This crate holds the agent's outward seams, of any kind: named local
Unix sockets. **There are two as of the egress ruling of 2026-08-07**, and the axis
that separates them is which party may open an exchange, per `weaver-organ-channel`
section 1. The world opens exchanges on one and the agent perceives and answers. The
agent opens exchanges on the other and a registered tool answers. Each is
single-initiator, which is what the interior's organ channels are not, and on a
single-initiator socket the opening party is a fact about the socket rather than one
carried per message, which is what lets this crate judge a crossing by reading no
payload at all.

**Which end binds the agent-opened seam is not this charter's to say.** On the
world-opened seam it is settled and merged: the world dials, this crate binds and
accepts, per `weaver-gate-world-contract`. On the agent-opened seam the transport is
one of three things this charter names as the tool-seam contract's own, beside which
credential a registered tool presents and how that maps to its registration, and who
binds each path with what ownership and mode, a path nothing owns exclusively being a
path another process can bind first, and a charter that answered it here would settle by
assertion what the
frame parked for a reason: a path nothing owns exclusively is a path another process
can bind first, and which end owns it decides that. **What this charter fixes is that
the seam exists, that it is this crate's, and that it is single-initiator with the
agent opening.** The rest arrives with the contract, per apex section 3 step 7.

**Whichever end binds, the peer is authenticated, and the two transports do not
authenticate equally.** Where this crate accepts, `SO_PEERCRED` reports the connecting
peer's own credentials and the predicate below judges them, which is the world-opened
seam's merged mechanism. **Where this crate dials, `SO_PEERCRED` reports the
credentials captured for the listening socket rather than those of the process that
accepted**, so a registered tool that listens under one identity and drops to its
provisioned one presents the pre-drop identity, and the check cannot by itself confirm
that the peer reached is the registered tool. Verified against a live kernel on
2026-08-07 rather than read off a manual page: a dialing client's `SO_PEERCRED`
reported the pid that called `listen`, not the pid that called `accept`.

**This is a demand on the contract rather than a defect in the seam.** What closes it
is the contract stating how a registered tool proves itself to a dialer, which is the
first of the three owed above and is the same question read from the other end. A
charter claiming symmetric identification
would hand `weaver-gate-Spec` a premise the kernel does not support, so it is named
here instead.

The claim is scoped to the outward-facing seams deliberately, since the inversion of
2026-08-05 has the harness binding a coordination socket inward, inside the same
sandbox, which faces admin rather than the world and admits root alone. **Two doors out
and one door in, and this crate holds both that face out**, a count the egress ruling
changed from one and the reason unchanged: what faces the world is this crate's and what
faces admin is not. Ready means every seam stands. Stopped means every one is closed.
The seams are not this crate's decision: they arrive as the gate instruction inside
the enter fan-out, operator-declared and admin-validated, carried by the harness
uninterpreted, resolved here.

**Where the door stands is the program's and only who may pass is the
operator's**, ruled 2026-08-15. An earlier reading had the socket's pathname
declared beside the access rule, on the parallel to `model-binding`, and the
parallel does not reach that far. Which weights an agent runs is a choice only
the operator can make, and there is no other place the answer could come from.
Where a Unix socket sits is not a choice of that kind: it is a deployment
detail with exactly one correct answer, and the program is the party that knows
it.

**The correct answer is inside the unit's runtime directory, and that is the
whole of the ruling's ground.** A Unix socket's pathname outlives the process
that bound it, so a name chosen anywhere else survives its worker and refuses
the next bind, and the program has no cleanup it can perform without racing a
live successor. The runtime directory is the answer already taken for the
coordination socket, per `weaver-admin-systemd-contract` section 2: the manager
creates it at start and destroys it with the unit, so a pathname inside it
cannot outlive the worker. The gate's socket carries the identical hazard and
takes the identical answer, which makes the hazard unreachable rather than
checked for. **A path an operator could write is a path an operator could write
wrongly**, and the failure it produces is a second load refusing on a name the
first left behind, which reads as a bind failure and is a stale file.

So the instruction declares the access rule and the harness supplies the
socket, each party naming what only it can know. What this costs is the ability
to put an agent's door somewhere of the operator's choosing, which nothing in
the program needed and which the client reaches by the agent's name either way.

**The boundary predicate, and there is one per socket.** A named socket is dialable by
anything that can reach it, so each hook authenticates every connection by peer
credential, which is the `peer-identity` and `authorization-predicate` pair of
`weaver-types-PRD` section 2.2 finding the consumer that subsection describes: a seam
that admits an outside principal, authenticated by `SO_PEERCRED` and judged by the one
shared rule. The mechanism is shared and the rule judged against is not, because the
two sockets admit different principals and one allow-list covering both would admit
each socket's peers to the other's.

**On the world-opened socket the predicate admits front-end principals only.** This is
the merged rule, unchanged by the egress ruling.

**On the agent-opened socket it admits registered tools only**, the principals the
operator provisioned for that door and no others, per apex section 3 step 7. Which
credential a registered tool presents and how that maps to its registration is the
tool-seam contract's to state, and this charter records the demand rather than
inventing the answer. **A credential names a uid and not a tool**, so a clause that
does not close the mapping authenticates a user and admits an application it never
checked.

**Both predicates exclude the agent uid**, and the ground differs on each. On
the world-opened socket an elected tool that could dial the agent's own mouth would let
the agent prompt itself through its own front door. On the agent-opened socket a
process at the agent uid answering where a registered tool should be is the agent
answering its own call, which is the same loop arriving by the other door. The
exclusions are stated here as boundary facts, with the mechanism the Spec's.

**The relay.** Inbound bytes to the harness, outbound bytes to the client, opaque both
ways, order preserved, nothing retained after the response returns.

## 3. What this crate must not hold

**No trace authorship, and no descriptor to the record.** It holds
its seam end and its listener and nothing else, which the contract's fork discipline
makes structural rather than behavioral.

**No channel to `weaver-admin`.** Admin holds one seam, and what this crate has to
say to admin travels through the harness as hub, per `weaver-admin-PRD` section 6.

**No translation and no dialect.** The earlier draft ruled an external dialect and
this re-draft removes it. Bytes cross unread, and what a client's bytes mean is the
harness's question, per the framing cell of section 10.

**No work state.** A request forwarded is a request gone. This crate retains nothing
about a turn after the response has returned through it, holds no session, and
replays nothing.

**No reading of the agent's configuration file.** The gate instruction arrives on the
seam. A crate that read the file would put a second reading of the operator's
declaration beside admin's, the same argument the SPU charter makes for the model
binding.

**No process it spawns, and two seams and no third.** The two seams of section 2, each
established once per raise and neither more than once, are the whole of this crate's
surface area. **Established rather than bound**, because which end binds the
agent-opened seam is section 2's open question and this clause must not close it from
seventy lines away. This clause once read one socket and no second listener, and the
egress ruling of 2026-08-07 is what changed the count: the second seam is the agent's
own door outward, not an acquired part. **What the clause still forbids is a surface
this crate opens on its own judgment**, and the reason is unchanged, such a surface
being one nothing chartered.

**`weaver-harness-gate-contract` admits the second seam as of 2026-08-07**, which
it forbade until that act. The contract and this charter agree, and a builder reading
both reads one count.

## 4. Raising and lowering, from this crate's side

**Raising.** The harness forks this crate last in the enter fan-out, after the SPU
has confirmed residency, over a pair the harness creates before the fork. The order is
apex section 6's binding rule, and the reason is the boundary: a hook that rises
before the interior is whole would accept work the agent cannot yet do. On its side of
the fork this crate performs its final exec, sets its own end of the channel
close-on-exec, clears its own dumpable flag, receives the gate instruction, establishes
every seam the instruction names, and confirms ready. Ready is a fact about the seams,
not about the process: **it is sent only after every one of them stands**, which the
egress ruling of 2026-08-07 turned from one seam into two. A ready answered after the
first would name a boundary half open, and the harness reading it would proceed with a
door still shut. What standing means on the agent-opened seam follows from the
transport its contract settles, per section 2, and the rule stated here does not
depend on that answer.

**Refusing.** A bind that fails is a refusal with the reason, typed, and a refusal
leaves nothing held: no listener, no half-bound socket, nothing a retry would trip
over. **With two seams that becomes an unwind rather than a return**: a second seam
that fails to stand closes whatever the first established before the refusal is
answered, so a refused raise leaves the same nothing a single-seam refusal left. The
aggregate's rollback has nothing of this crate's to undo, which is the property
section 5 rests on, and it holds only if the partial success is closed here.

A refusal is answered to the harness rather than exited on, for the reason the SPU
charter states, a party that exited would replace a typed reason with an observed
death.

**Lowering.** The harness stops this crate first in the leave fan-out. Stopping
closes **every seam this crate holds** before anything else happens anywhere else,
which is what stopped-first means and why it is first. Confirmation of stopped is
sent only after all of them are closed, which the egress ruling of 2026-08-07 turned
from one into two: a stopped answered with the agent-opened seam still reachable
would tell the harness the boundary is down while a door still stands, which is the
same half-open the raise refuses in the other direction. What happens to a connection
accepted and in flight at that moment is drain, and drain is the token workflow's,
deferred in section 8, and it reaches both seams for the same reason the close does.
In this pass no traffic exists, so closed is the whole of it.

**Never outliving.** A gate process never outlives the worker interior it protects,
per apex section 6. The mechanism this pass states is the channel: the pair closes
when the harness is gone, and a gate that observes closure closes every seam it holds
and exits. The count is the lowering clause's and the reason is the same, a seam
outliving the interior being the boundary standing with nothing behind it, which is
worse on the agent-opened seam than on the world-opened one because a registered tool
would go on reaching a gate whose agent is gone. Whether a second mechanism backs
this is the Spec's choice, and the requirement does not depend on it.

## 5. What a failure partway through leaves behind

A refusal at the bind leaves the agent interior loaded and the aggregate not ready,
and the rollback that follows is `weaver-admin-harness-contract` section 3's, not this
crate's. This crate's obligation is that its refusal is true: nothing held, so the
rollback has nothing of this crate's to unwind.

A gate death after ready is the loss of the agent's reachability, observed by the
harness as channel closure. What the harness does with that observation is the
coordination seam's. This crate holds nothing whose loss corrupts the interior, so its
death is an availability fact and not an integrity fact.

## 6. The seam

One seam, with the harness, governed by `weaver-harness-gate-contract`, drafted with
this charter as one act.

**The organ declares, and the record is here.** This is an organ channel, so Document
Format section 4's organ rule reaches it: the organ declares and the harness does
not, because the harness is the hub every organ holds its two-initiator channel with
and a hub that declared its own edges would carry the whole seam graph in one crate.
`weaver-harness-PRD` section 4 points at the contract and gains no record, per
section 11.

```graph
edge: seam
from: weaver-gate
to: weaver-harness
via: weaver-harness-gate-contract
tag: socket
```

**The contract's name stands as filed.** The Format names a contract for its parties,
initiator first, and the harness initiates the governed signals: the data flows
inward, the ask flows outward, and the name follows the ask. The open-items note that
the direction was unsettled settles with this act. When the token workflow adds
gate-opened turn exchanges the channel carries two initiators and the name does not
change, because the name records the governed signal's initiator and not the traffic's
direction.

## 7. Identity, process boundaries, and the tool-uid cell

This crate runs as the agent uid, `weaver-<n>`, inside the boundary
`weaver-admin-PRD` section 7 constructs. After its final exec it clears its own
dumpable flag and sets its own channel end close-on-exec, the same two acts in the
same order the SPU charter states, and for the same reason: an outward-facing process
in the agent's uid is exactly the process a same-uid attach should not reach.

**The cell this charter inherits.** Whether external tool processes run as the agent
uid or under a uid this crate owns is this crate's to rule, per `weaver-admin-PRD`
section 7, which names the agent-uid case as its assumption and files the cell here so
it is inherited as a constraint rather than rediscovered.

**The fork is this crate's as of the tool workflow's opening act of
2026-08-17.** The shell executes here, forked and supervised per call
and answered on the execution exchange of `weaver-harness-gate-contract`
section 2, the mechanism election the finding of 2026-08-11 left to the seat
with codebase context. An earlier reading had the harness forking the tool,
and the ratified boundary is what retired it: the loop reaches tools through
this crate, so the spawn belongs on this side of the loop's membrane and the
result crosses back exactly once, as the exchange's answer. This pass forks
at the uid this crate already holds, which is the agent-uid arm admin's
charter assumed, and the cell's separate-uid arm stays PENDING with nothing
built against it, awaiting the threat measurement the cell names.

**PENDING, ruling candidate for ratification.** Stage one ratifies the agent-uid
case. Under that case the boundary between the agent and its worker is hardening
rather than kernel-enforced separation, and the hardening is named: the cleared
dumpable flag on every process of the worker's family, and close-on-exec on every
descriptor at every fork, including the creating party's own ends.

**The cell narrows to one case and does not close, and an earlier cut of this act said
it closed.** The port ruling of 2026-08-07 removes two of the three tools it used to
reach. A tool that binds a listening port is external, a registered application the
operator provisions and this program forks none of, so no uid of its is this program's
to choose. Loop code the operator compiles into the worker holds no separate process
and so no separate uid. **What remains is the shell this crate forks per call**, which
`weaver-harness-Spec` section 10 describes as a subprocess running as the
agent uid and builds a reference walk on. The word internal names the
agent-level answer the port test gives, and under the ratified loop boundary
of 2026-08-11 the same shell is external to the reasoning loop, every tool
being on the far side of this crate by that criterion - two answers naming
two boundaries rather than contradicting each other. As of the tool boundary
ruling of 2026-08-18 the word also names a third thing, the `weaver-internal`
promotion space, which holds inward-dispatched callables and no process, so
no uid question of this cell reaches it. That process has a
uid, the choice between the agent's and one this crate owns is live for it, and the
descriptor custody the walk protects is exactly what the choice decides.

**PENDING stands for that case, on the arms section 8 stages.** What it awaits is
unchanged, the tool workflow chartering plus a stated rationale. The egress ruling
supplied one for the external case and the port ruling removed that case from this
cell rather than answering it, so neither ruling has priced the arm that remains.

## 8. What does not cross, and what waits

**Chartered by the token workflow's act of 2026-08-02, no longer deferred:** the
turn exchange this crate opens toward the harness, how a turn is identified
across the boundary, concurrent clients against the one-turn loop, drain on
stop, and the fault cases a running hook raises. Each lands in section 13,
which is this charter's next section arriving with its workflow.

**Still deferred, and named rather than omitted:** streaming responses and the
connection the hook holds open while the interior generates, and the
backpressure that rides them. `weaver-gate-world-contract` section 3 makes one
line in and one line out the resting shape and streaming an extension to that
page rather than a replacement, so it arrives when a client needs it and this
crate gains nothing in anticipation.

**Cancellation of an execution was deferred here by omission until 2026-09-22**,
section 0 listing it and neither this section nor section 10 carrying it, and it
is deferred no longer: section 13.5 charters it on the operator's ruling of that
date, and `weaver-harness-gate-contract` section 2 carries the exchange.

**Staged:** the separate-uid arm of the section 7 cell.

None of these acquires a trait, a variant, a feature flag, or a config field in this
pass. A charter naming a domain boundary is a decided boundary. An unbuilt interface
waiting to be filled is the thing apex section 9 forbids.

### 8.1 What the boundary is not asked to carry

Three things sit next to this boundary and are not it, absorbed here from
`tool-egress-boundary-frame` on 2026-08-23 when that frame was archived. Naming them
keeps the boundary claim from being asked to carry weight it was not built for.

**It is not a compromise answer.** This crate authorizes the processes on its list
and refuses the rest, and it holds the door for the traffic it was configured to
allow. **Whether a process that was authorized is later turned is a separate problem
at a separate layer**, and loading it onto this crate is how network paranoia creeps
back into the one place the socket floor cleared it out of. The question is parked
with its own layer rather than answered here.

**It is not the network-facing security layer.** A registered tool that reaches the
world will want a firewall, application support, and a hardened surface. That layer
is real and is not what this program is building. **This crate selects the
application and does not select that application's destinations**, which is why the
turn path states a boundary rather than a containment.

**It is not an injection answer, and this is the corpus's only statement of where
injection lives.** A tool's answer is world-authored content entering the next
prompt, **so the answer path has an injection surface.** Its owner is the
network-facing wrapper behind the tool contract rather than this crate, and the
mechanisms that would address it, small activation networks and rerankers and
categorizers inside the reasoning process, are not built. Injection is named here to
place it and to demote it, so the boundary claim is not asked to contain it.
**Nothing in this program mitigates it today**, and a reader should not infer from
the gate's existence that anything does.

## 9. Staged requirements

**The token workflow arrived on 2026-08-02 and ratified this list in place.**
The forwarded prompt reaches the harness with the turn not yet in existence, so
this crate carries no `turn_key` inward and mints none. Responses return by the
path the request took. This crate sees octets and nothing of the model, the
trace, or the session. Section 13 is where each of these now binds, and this
section keeps them as the record of what the earlier pass could already see.

## 10. Open cells

**The tool-uid ruling.** Stated in section 7 with its candidate. **Settled by:** the
architecture seat's ratification of the candidate, or the tool workflow chartering
**Narrowed by the port ruling of 2026-08-07** to the forked shell, an external
tool being the operator's to provision and loop code holding no separate
process. Section 7 carries the reasoning. Until settled, admin's
assumption stands and nothing builds against the separate-uid arm.

**The refusal type.** This seam's refusal reuses `lifecycle-refusal` rather than
taking a definition of its own, on the grounds the SPU charter states: the refusal has
to reach admin inside the enter aggregate unchanged. Whether the case set grows to
hold bind failure is the cell `weaver-spu-PRD` section 10 holds for its cases, and
this charter rides that cell rather than opening a twin. **Settled by:** the ruling
that settles it there.

**Wire framing at the client socket is closed.** Newline-delimited JSON, one request
line and one response line per turn, ruled at `weaver-gate-world-contract` section 2 on
2026-08-01. The field shapes stay with the spec seat and streaming with the token
workflow, per that contract's section 3.

**The gate instruction's home is closed.** A field of the agent's configuration
file, defined at `weaver-types-PRD` section 2.1 beside `model-binding`, on the
identical grounds: the operator writes it, admin validates it, the harness carries
it, this crate resolves it. The types edit landed with the merge of this charter on
2026-08-01.

**The client boundary's seam category is closed.** The client socket takes a
contract of its own, `weaver-gate-world-contract`, per the human's ruling of
2026-08-01 that the external boundaries are contracted before any Spec. The party
category the
Document Format lacked is owed to the Format by `weaver-admin-operator-contract`
section 8, one register entry for both instances.

## 11. Edits owed

Named here because a document whose reach cannot be read for the reach is a trap.
These are owed by this act and this list is the authoritative one under G5. An entry
leaves this register when the edit lands.

**Four left on 2026-08-01, with the merge of this pair.** The `gate-instruction`
field landed in `weaver-types-PRD` section 2.1, the predicate's consumer citation in
2.2, the turn-ingress resolution in `weaver-harness-PRD` section 4, and the
open-items entry on this contract's name settled with the name standing as filed.

- The section 2.3 entry this register carried dissolved with the naming ruling of
  2026-08-01: the seam draws loop 0's trio and owes the floor nothing, per the
  contract's section 7.
- `weaver-agents-PRD`: the gate's rows and prose join the correction list's demotion
  entry, the component table, the section 3 path, and the organ framing of section 6,
  all filing with the re-authoring.

## 12. Children

Specs to be written against this charter once the PRD set is ratified. Named so the
set is bounded, not drafted here, and incomplete for the same reason the charter is.

- The hook, covering the bind, the instruction's resolution, the predicate, and the
  refusal set.
- The fork and the descriptor discipline of section 7.
- Stop, covering the close and the confirmation ordering.

## 13. Serving the turn

Arrived with the token workflow's act of 2026-08-02, filed after the children
for the reason `weaver-spu-PRD` section 13 states: sections 8 through 12 are
cited by number across the merged corpus, and arrival order is filing order.
Everything here is derived from `weaver-harness-gate-contract` section 2 as
that act extends it, `weaver-gate-world-contract`, and the turn grammar of
`basic-inference-loop`.

### 13.1 The relay

**One exchange per client request, opened by this crate, opaque both ways.**
A request admitted at the hook becomes an exchange opened toward the harness
carrying the client's line as octets this crate has not read. The response
returns on that exchange and goes back out the connection it came in on. The
exchange's own identity is the correlation, so this crate holds no table of
its own and invents no identifier: what it needs to route a response is the
thing the channel already gives it.

**The turn is not this crate's to name.** No `turn_key` crosses inward,
because the turn does not exist until the harness opens it, per section 9,
and nothing here mints one. What this crate knows about a turn is that a
line went in and a line came out.

**Retention ends at the answer.** Per section 3, nothing about a turn
survives the response returning through it, and the exchange's close is where
that is enforced rather than promised.

**The relay has a second leg, and it runs the other way.** On the world-opened
socket a request arrives and this crate opens an exchange toward the harness. On
the agent-opened socket the harness opens the exchange and this crate carries it
out to the registered tool, whose answer returns by the path it took. The
opacity rule is the same on both legs and so is the retention rule: octets in
order, nothing read, nothing kept past the answer. What differs is only which
side opens, which is the axis section 2 draws. The exchanges this leg carries,
how a tool is addressed among several, and what a tool's death mid-call means
are the tool workflow's to charter, and nothing is shaped here ahead of it.

### 13.2 Concurrency, and where it resolves

**Clients may speak at once and this crate relays as they do.** More than one
exchange may be open toward the harness, and the harness serves them one turn
at a time in arrival order, per the contract's ratified ruling. This crate
refuses nothing on the grounds that the interior is busy, because waiting is
what a conversation already means and a busy interior is not a boundary
condition. Order per connection holds in both directions, which is what a
client is owed and the whole of what it is owed: no promise is made across
clients, and none is needed, since a client sees only its own connection.

### 13.3 Lowering with traffic present

**Drain is modest by construction and this section states why.** A lower
arrives only when the run is at rest, per the coordination seam's own rule
that leave refuses while a turn is in flight, so no turn is outstanding when
the seams close. What remains is connections a peer is holding open on either
seam, which this crate closes after the seams and before answering stopped. A
peer that reconnects finds nothing standing, which is refusal by absence and
the boundary the lifecycle protects.

### 13.4 The faults this crate raises

The enumeration section 8 deferred, each a fault the worker survives,
reported to the harness on the exchange the contract names and authored by
the harness as the `fault` event:

- **The listener is lost.** The bound socket became unusable while the hook
  was raised, so the agent is unreachable with its interior healthy. This is
  the reference case, and it is why the fault direction exists at all: the
  harness cannot observe this from its side, the channel being fine while
  the boundary is gone.
- **A client connection failed mid-turn.** The response could not be
  delivered because the client is gone. The record already holds the turn's
  close, per `weaver-gate-world-contract` section 3, so this reports a lost
  delivery rather than a lost turn, which is the distinction that keeps a
  consumer from reading a delivered answer where none arrived.
- **Admission is failing systematically.** The predicate is refusing every
  peer, which is either a boundary the operator misconfigured or something
  probing it, and either way the operator wants it in the record.

A peer failing the predicate is not a fault. It is the boundary working, and
a fault event per refused dial would make the record noisiest exactly when
the boundary is doing its job.

**With these named the corpus-wide case set closes across all three organs for the seams
that exist.** The agent-opened seam of section 2 is not among them: a registered tool
unreachable, its path gone, or a dial refused are faults this crate will raise and this
section does not enumerate, because the seam's contract does not exist and a case set
written against a guess is the thing this section refuses elsewhere. **The closure claim
is therefore scoped and the gap is named**, so the trace act electing a payload shape
against a closed set knows which set closed. The `fault` event's cases are the SPU's at
`weaver-spu-PRD` section 13.10, this crate's here, and the harness's own five at
`weaver-harness-PRD` section 5, that last enumeration landing in this same act on the
review seat's finding, and it cannot be derived from the other two because an organ
death is reported by the party that survives rather than the one that died. The
payload's shape lands with the trace act against the closed set rather than against a
guess.

### 13.5 Cancelling an execution

Arrived 2026-09-22 on the operator's ruling that a stop reaches a running
tool, taken after the W1c characterization of #646 measured the stop waiting
the whole invocation out. Derived from `weaver-harness-gate-contract` section
2 as that act amends it.

**A cancel is the caller's clock brought forward, and this crate kills on it
as it kills at the clock.** The harness sends the cancel inside the open
execution exchange when the operator's stop reaches it mid-invocation, and
this crate signals the invocation's whole process group at once, descendants
included, and answers the exchange with a kill that names the cancel as its
cause. The contract's one-clock rule stands: the caller's number still bounds
the exchange, a cancel never lengthens a wait, and a cancel this crate never
reads leaves the clock to do the same work later.

**Every execution still completes with one answer.** A cancel that arrives
after the answer left is dropped, because the exchange is closed from this
side and the harness reads what became of the tool from the answer already
in flight. A tool that finished as the cancel arrived answers in its own
words. Nothing here opens an exchange, retains anything past the answer, or
reads content, per section 3.

**What this does not settle.** A descendant that detached itself from the
group before the kill is the containment question issue #642 carries, and a
cancel reaches exactly what the clock reaches, no more. How the supervisor
hears the cancel while a tool runs is the Spec's.
