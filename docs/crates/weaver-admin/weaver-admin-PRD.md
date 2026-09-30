# weaver-admin - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. The rulings that
reshaped this charter are recorded in it rather than pending against it, and section
10 now carries a shorter list than it did. The header moved on the human's ruling of
2026-07-31 with no other edit.

**Date filed:** 2026-07-29

**Parent:** `weaver-agents-PRD`
**Companion contract:** `weaver-admin-harness-contract`, written with this document
**External boundaries:** `weaver-admin-operator-contract` for the record's exit and
`weaver-admin-systemd-contract` for the unit, both parties outside the program
**Editorial:** Per the Working Rules.
**Landing PR:** #505

---

## 0. What this document is

The charter of the crate that verifies the agent's operating-system boundary and
drives the agent through its lifecycle verbs. It is written together with
`weaver-admin-harness-contract`, which governs the one seam this crate holds during
a transition, and neither is complete without the other.

Level discipline, stated once. This document carries what the crate needs and why,
including the order in which a transition happens, because the order is admin's own
work rather than a wire agreement. What crosses the seam, what it means, and how it
fails is the contract's. How any of it is represented is the Spec's and appears in
neither.

## 1. What this crate is

**The fleet's lifecycle driver, run by the operator with root.** One crate, many
agents, and it is not a constituent organ of any of them. Two facts put lifecycle
here. A harness cannot drive the early steps of its own creation, because the worker
spawn runs before the harness is running as the harness at all. And the acts a
lifecycle verb performs, starting a unit under another identity, opening a sink the
agent could not, are root's acts, so they belong to the one seat that holds root,
which is the operator in the admin role, per section 7. The crate is an invocation
rather than a resident: it runs when the operator runs a verb, exits when the verb
answers, holds nothing between verbs, and what persists across invocations is what
the init system and the filesystem already hold. The standing party in every agent's
lifetime is the init system, which this program inherits rather than shadows.

```graph
node: weaver-admin
kind: crate

edge: parent
from: weaver-admin
to: WeaverTools
```

**It verifies the agent's boundary and authors none of it.** Apex section 12 rests
the whole regulation model on the agent being an operating-system user whose reach
the kernel bounds. That model has a writer and this crate is not it. The agent's uid,
its group memberships, its home directory and the modes on it, and the trace
directory the agent uid cannot search are the operator's to establish before an agent
is created, by whatever means that operator's site already uses to admit a principal.
Admin reads that boundary, checks it against what a load requires, and refuses a load
it cannot confirm.

**Creating an agent is not creating a user, and the separation is the ruling.**
Admitting a principal to a system is administrator authority over the operating
system's own trust model, and a program that took it would be raising a second trust
model above the one it claims to inherit. So provisioning leaves this crate entirely
and leaves the program with it. What remains is an act on an identity that already
exists, which is a different and much smaller kind of work.

**The cost of that is stated rather than glossed.** This charter guarantees the
boundary by checking it and no longer by building it, and those are different
strengths. A boundary that passes verification is the operator's artifact, produced by
a script this corpus does not govern, and the program ships a check rather than a
constructor. That is the same move the corpus made when reading and analysis left in
section 8, and it is right for the same reason, but a later reader must not take the
charter to defend a property it only confirms.

**The agent has no path to its own supervisor.** This is the property the whole
custody model reduces to, and it is structural in two independent ways. The
coordination channel of section 6 is answered by a credential check: the harness
reads the dialing peer's uid at accept and refuses every principal that is not root,
so a tool elected at the agent uid reaches a refusal rather than a supervisor, and
the check discriminates on its own rather than leaning on an absent name. And
admin's code is never linked into the worker's address space, so no build exists in
which a harness process contains supervisory code that a bug or an elected tool
could reach. The second is the one that stops the shortcut where someone adds a
repair path to the worker because the function was already compiled in.

The property is cheaper to hold than it was. A supervisor that authored boundaries had
to keep authorship away from the agent, and this one has no authorship to keep.

## 2. What this crate owns

**Authorization of lifecycle intent.** Whether a given operator principal may run a
given verb against a given agent. This is external authorization, it happens before
anything else is touched, and a refusal leaves the system exactly as it found it.

**Verification of the boundary, which is a different thing from its authorship.**
Per section 1, the OS identity, the home directory that is the agent's sandbox, and
the trace directory with its modes and group memberships are the operator's artifacts.
What this crate owns is the check: that the identity resolves, that the home exists
with the ownership and modes a load requires, and that the trace directory is
root-owned and not searchable by the agent uid. A boundary that fails any of these
refuses the load, and nothing here is repaired.

**Validation of the agent's configuration file.** The operator writes it and both this
crate and the harness read it, per `weaver-types-PRD` section 2.1. It is declarative,
read at load, and fixed for the run, carrying the model binding, the tool set, the
permission mode, and the two elections. Validating it before a process exists is this
crate's, because a file that is malformed, or that leaves unnamed what a load needs,
must fail the load at the cheapest possible moment, and what it names is judged by
the organ that owns it, per section 4.3.

```graph
edge: reads
from: weaver-admin
to: agent-config
```

**The node renamed to `agent-config` on 2026-08-01, the human's ruling and the
last register entry of that pass.** The artifact is configuration rather than
state, the agent's state is the trace, and the old `agent-state-file` identifier
pointed at the wrong artifact. The record above and `weaver-types-PRD` section
2.1 moved in one act, which emptied section 11's register until the act of
2026-08-05 filled it again. What that register holds now is section 11's to say.

**Standing the state member, which this charter has never said and the code
has done since the ingest act.** The custodian of `weaver-state-PRD` runs under
its own account over its own subdirectory of the operator's territory, and the
party that starts it has to hold three things before any process exists: the
operator's territory, the binding kind the inventory settled, and the enter,
the one exchange the harness's end of the pair can cross, which only this
crate opens. This crate holds all three at
that moment and no other party holds any of them, so the member is this
crate's to start. **Whether it starts is the declaration's fact, as of
2026-09-04**: the store election of `weaver-state-PRD` section 4 says which
engine the member stands on or that none stands, an absent election meaning
the embedded engine with the member standing, and a box that cannot stand what
the declaration elects refuses the load rather than serving without the leg.
Until that date the member stood where its binary sat beside the worker's,
recorded nowhere, which issue #381 measured as the cause of a verdict no report
could explain. The agent's resolved identity stood third on this list
while the vector carried its uid, and left it with the ruling of 2026-08-26,
both of that value's uses having died. **It is not the
worker's unit and does not become one.** The worker is asked of the init system
per section 4's sequencing, and the member is a direct child of this crate,
which is what lets it hold a uid the agent's cannot reach.

**No seam is created by starting it, and the transport this crate now makes is
not one either.** Per the operator's ruling of 2026-08-26 this crate creates
the socketpair the first door is, hands one end to the member with its process,
carries the other to the harness inside the enter directive, and speaks on
neither: a courier of ends rather than a party. Section 6's seam list is
unchanged by this paragraph, because starting a process is not a channel and
neither is carrying one's ends, and what the member serves it serves to the
harness and to `weaver-analysis` under their own contracts.

**Nothing is waited on, because there is no name to watch.** The ruling of
2026-08-26 retires the pathname wait with the pathname: the pair exists before
the member does, so a load cannot race a bind that no longer happens. **What
establishes the leg is the enter's own first traffic**, bounded at the harness
per the contract's dead-peer clause. A member whose spawn failed puts no end
on the enter, and one that died after the spawn answers the first traffic as
a closed pair, the leg not standing either way rather than a refused load.

**The start falls between steps 5 and 6 and takes no step number of its own.**
The worker's unit is asked for at step 5 and the enter is directed at step 6, and
the member is started between them, the pair in hand before the enter that
carries its harness end. It takes no number because
section 5's rollback discriminates by step and no step of the load turns on the
member: the leg either stands or does not, and section 4's sequencing is
unchanged either way.

**Nothing of this crate reaps the member, and nothing needs to.** This crate is
one invocation per verb and exits when the verb answers, so the member is
reparented to the init system and reaped there, which is also what lets it
outlive the invocation that started it - a member that died with this process
would serve no residency at all.

**Which doors the member stands is this crate's to say, because the kind is.**
A diagnostic binding stands the preload door of
`weaver-analysis-state-contract` and a serving binding stands none, per
`weaver-agents-PRD` section 6 as ruled 2026-08-24. The member holds no opinion
about the binding and learns which doors to stand from the party that starts
it, per `weaver-state-Spec` section 4, which is the same division the rest of
this charter runs on: this crate resolves what the declaration meant, once, and
hands the resolution down decided.

**That leaves the vector watched by nothing unless this crate watches it.**
`state-preload-door-stands-only-diagnostic` is asserted by `weaver-state` and
covers what the member does with what it is given. **What this crate owes is the
vector in both directions**: a serving inventory carries no preload name and a
diagnostic one carries one. It needs a record of its own because the member's
covers neither direction - given a name it binds one and given none it binds
none, which is that record holding however the name was wrong - so without one,
the seam between the two crates is watched by nothing. **The
record is the Spec's**, per the Document Format's definition of an assertion as
a claim a Spec makes, and it sits in `weaver-admin-Spec` section 6 under the
vector clause, which is the clause the claim is about.

**Directing a transition, and receiving the aggregate result.** Admin directs the
transition across its one seam and the harness sequences it, each organ performing its
own operation, and the transition publishes only after every organ has confirmed to the
harness and the harness has returned the aggregate to admin. Apex section 6 gives the
harness the word coordinator and the act of sequencing the organs with it, and nothing
here contests it. What admin owns is the directive and the aggregate it gets back, which
is a different act from coordinating a turn, and the two never run at the same time.

**Custody of the stream's sink, and the agent reaches none of it.** The program
owns no record, per the ruling of 2026-08-01, restated here as the authority since
the contract that carried it retires with this act: durability is the operator's,
the program opens what was declared, passes the handle, implements no transport,
and elects no destination. What admin owns is the connection: the invocation opens
the sink the agent's configuration declares under root, the role's principal, and
passes the descriptor into the enter directive, so the worker writes a stream it
could not have opened for itself. Where the sink is a file, the operator's
provisioning keeps the agent uid off it by ownership, mode, and the directory's
search bit, which admin's boundary check verifies rather than builds, the same
posture as section 1, and a kernel check rather than a concealment, since a process
holding a descriptor can always `readlink` its own `/proc/self/fd`.

**The agent writes through a descriptor and by no other route.** Permission is
checked once when the sink is opened, against the opener, and a descriptor passed
by `SCM_RIGHTS` installs against that same open file description with no recheck.
So the worker writes a stream it could not have opened for itself, which is the
whole of its access and is revocable by closing the descriptor.

**The working structure in RAM is append-only by construction, because its API
exposes no update path.** That is the one append-only claim this charter carries, a
structural property of the in-RAM store rather than a witnessed discipline, and
integrity accounting past the emission boundary is the consumer's business on the
consumer's compute.

**A log of admin's own privileged acts.** A completed rollback and a refused load
are privileged acts that leave nothing behind
otherwise. A refusal reaching an operator is a typed `lifecycle-refusal` the
invocation returns before exiting and needs no log to be legible, but an act whose
invocation was interrupted, or whose rollback ran after the answer, has no reader at
all. Admin holds a log file for those.
It is admin's own file, admin is its sole author, and it is not the trace and shares no
schema with it. **It records acts of the supervisor and never conduct of the
supervised.** The moment it carries a fact about what an agent did, it is a second
record of the agent with a second author, which is the arrangement the trace's single
writer exists to prevent. Format, retention, and rotation are the Spec's, and are not
settled here on the grounds that a log format decided before a rollback has run is a
format decided from no measurement. The artifact is named in the mapping regardless,
because deferring a format is not a reason to leave the sole record of the privileged
half of the lifecycle invisible to it.

**Custody, stated because every other artifact in this corpus has its access argued.**
The log is owned by root, mode 0640, in a root-owned directory at mode 0750. It is
fleet-scoped rather than per-agent and it never lands inside an agent home, which is
the load-bearing half. The named adversary is the agent uid, and it is excluded
twice over: it is neither owner nor group, and the directory's missing search bit
means it cannot reach the file to try. An agent that could read this file would read
the record of its own supervision, which is the same class of hole the trace
directory's search bit exists to close. The operator reads it as root, the same seat
that wrote it, per section 7.

**The stream's sink and this log are secured against the agent and against nothing
stronger, and the party that concerns is the operator.** Custody is exclusion of
the agent rather than evidence against the holder. This is not a gap the charter
can close, because the operator is the party that admits principals, holds root per
section 7, owns the sink, and holds what accumulates behind it. The
program secures the agent against reaching its own record and does not secure
anything against the operator, who is trusted by construction. The apex states that
trust model once, per section 11, and this paragraph is its local half.

```graph
node: admin-operations-log
kind: artifact

edge: writes
from: weaver-admin
to: admin-operations-log
```

**Admin concludes nothing about the record, because it holds none.** The comparison
an earlier version of this section emitted, the reference direction it argued, and
the repair it forbade all presupposed a program-owned file to compare against the
working structure, and each dissolved with it on 2026-08-01. What survives is the
posture: admin repairs nothing, adjudicates nothing, and calls nothing good, over
any artifact it touches.

## 3. What this crate does not own

**The turn, in any part.** No prompt, turn, task, or run enters through this crate,
per apex section 6. The line is worth stating in its live form rather than its
abstract one, because the operator-facing surface of section 8 is where it will be
tested: reporting an agent's state, listing agents, and driving a verb are in bounds,
and carrying a prompt to a loaded agent is out however convenient a menu makes it.

**Activity control's mechanics.** The abort of a turn belongs to the harness and
returns the agent to loaded and idle. What this crate holds is the conveyance: the
operator's intent to stop crosses as the stop exchange of the contract, one bit and
no work, because the operator holds no other crossing. Admin carries the intent and
never the mechanics, reaches nothing of the turn's content, and unload still waits on
rest rather than racing it.

**Trace authorship.** The harness is the sole writer of the record. Admin authors no
event, holds no event kind, and its own first contact is recorded by the harness as
the `load` event of the session's first run rather than by an entry of its own.

**Reading the stream as content.** Admin connects the sink, which is custody, and
custody is not comprehension. Parsing events is the operator's tooling's business
on the other side of the sink, and admin interprets no content in either direction,
the monitoring being the outside's job and the verb being admin's, per the basic
loop's section 2.

**Provisioning, in every part.** The OS identity, the home directory, the modes and
group memberships on it, and the agent's configuration file are the operator's, per
section 1. Admin creates no principal, chowns nothing, and edits no account. A verb of
this crate that found a boundary missing refuses and does not build one.

**Model admission, residency, and release, and the device entire.** The SPU admits
the model, holds residency, releases, and is the one authority on the device, per
apex section 6 as relocated under ruling C. Admin arbitrates no hardware and reasons
about the device at no point: a conflict is discovered at admission, refused there,
and the refusal travels SPU to harness to admin inside the enter aggregate.

**Network ingress.** There is none. Gate binds no listening network socket, per the
ruling of 2026-07-31 carried on the apex correction list, and no other crate does
either. A localhost Unix socket carrying an operator's verb is not network ingress and
breaches nothing, on two grounds stated here rather than cited. A Unix socket binds no
port and is reachable only by a process already on the host, so it adds no ingress for
the invariant to be about. And a network-attached surface on the admin side would be the
first thing in the architecture arguing against apex section 12, since the regulation
model rests on the kernel bounding reach and a listener admits a principal the kernel
did not place. Section 8 states this where the surface that tests it is named.

**The floor's vocabulary, beyond what it demands.** This crate links `weaver-types`
and does not link `weaver-traits`. It reads a configuration file whose permission mode
and tool set are `weaver-types` fields electing from `weaver-traits` definitions, and
it validates the file's shape rather than the existence of a tool implementation, which
is harness-internal and unreachable from here. A link added to validate something
this crate cannot see would be a dependency taken for nothing.

**The non-link is a declared surface and not a build exclusion.** `weaver-types`
floor-links `weaver-traits`, so those definitions are in this crate's dependency tree
whatever this charter says. What the declaration buys is a checkable statement that
nothing draws them on this crate's behalf, and claiming it as an exclusion would be
claiming a property the manifest does not have.

**A charter carries no vocabulary clause, so this crate draws nothing by its own
words.** The clause `weaver-types-PRD` section 5 asks for is a contract instrument,
present in a contract even when its answer is empty, and the union the G4 check
reads is the union over the contracts a crate is party to. This crate is party to
one, and `weaver-admin-harness-contract` section 8 draws five `weaver-types` values
and nothing from `weaver-traits`. That union is the whole of what this crate draws.

```graph
edge: floor-link
from: weaver-admin
to: weaver-types
```

That contributes one fact and settles nothing. `tool-set` reaches this crate as a field
of the configuration file it reads, which is an artifact this crate validates rather
than a value drawn across a seam, and the distinction matters because a field read out
of a file and a definition drawn by a contract are answerable to different checks.
`tool-trait` is reached neither way. It was held blocked until the tool
workflow's opening act of 2026-08-17, and this charter neither settled nor
opened it: the unblocking is `weaver-traits-PRD` section 3.1's own, on the
inverted ground the ratified loop boundary supplied, and this crate still
draws nothing from it.

## 4. The lifecycle

Three verbs, and two acts that are not verbs. The invariant that orders them is stated
first because every subsection depends on it.

**No verb of this crate writes the boundary.** Admitting and removing a principal are
operator acts on the operating system, performed before an agent exists and after it
stops existing, and they sit outside this crate's verb set entirely. What the operator
leaves behind is a resting state this charter calls provisioned and unloaded, and the
three verbs move an agent within and out of that state rather than into or out of
existence.

**This is a different verb set from the apex's and not only a smaller one.** Apex
section 6 carries four. Two leave, `create` and `destroy`, as operator acts on the
operating system. One arrives, `validate`, which the apex does not carry at all. The
state the departing two bracket survives, so the diagram loses two arrows rather than
a state. Both edits are owed in the same act, per section 11.

**Validation belongs to the crate that takes the verb's first action, and that crate
differs by verb.** On a load the first action is admin's, because admin is handed the
configuration file and nothing moves until admin confirms that file will work, so the
inventory of 4.3 sits in this crate. On an unload the first action that matters is the
comparison of the working structure against the file, and that belongs to whichever
crate holds both of those things, which is not this one. The rule is locality rather
than seniority, and it is why one verb's validation is this crate's own work and the
other's is only directed by it.

**Opening the session record is not a boundary write.** `weaver-trace-PRD` section
4.1 has the session's first run creating the record, and this charter keeps
that. A record belongs to a
session and a boundary belongs to an agent, so a verb that creates one is not
creating the other, and the invariant above is scoped to the boundary deliberately.

### 4.1 load

Verifies the boundary, opens the record, starts the interior, and publishes. The
order is the substance.

1. **Authorize the intent.** The invocation runs as root or performs nothing, the
   kernel having already settled who may act, so what remains to authorize is the
   name: the agent is on the operator's allow-list and shaped as a name rather than
   a path, or the verb refuses without touching anything else.
2. **Read and validate the agent's configuration file.** A file that is absent, that
   is missing a required field, or that leaves the model binding's artifact unnamed
   fails the load before any process exists. Whether the artifact it names resolves
   is the SPU's to answer at admission, per section 4.3.
3. **Verify the boundary the operator wrote.** The OS identity resolves, the home
   directory exists with the expected ownership and modes, and the trace directory is
   root-owned and not searchable by the agent uid. Any failure refuses. Nothing here
   is repaired, and nothing here is built.
4. **Resolve the session and open the sink.** Admin decides which session is being
   loaded, a decision the harness is structurally unable to make because it never
   learns a path, and opens the sink the configuration declares. The descriptor is
   obtained here, under root, the role's principal, which is what lets the worker
   write a stream its uid could not open. Close-on-exec is not admin's to confer on
   a passed descriptor and is the harness's obligation at the receive, per the
   contract.
5. **Ask the init system to start the worker as a transient unit carrying the
   agent's `User=` and the worker's provisioning.** Root asks the process manager
   it already commands, and the unit's cgroup arrives with the unit rather than
   being shaped in advance. The unit declares no open and receives no descriptor:
   the worker starts bare of descriptors, and its first act is to bind the
   coordination socket of section 6 inside its own sandbox and listen.
   **Bare states what no descriptor crosses and says nothing about arguments.**
   A worker cannot bind a name it was never told, and the organ binaries it forks
   at enter are a provisioning fact its composition root is handed rather than one
   it discovers, the same standing `weaver-harness-PRD` section 2 gives to which
   binary an agent's unit starts. The ask carries them because it is the only path
   from the operator's installed values into a process that does not yet exist.
   **An arm only some declarations elect is provisioned the same way and
   installed differently**, as of 2026-09-07: its binary is found beside the
   worker's rather than placed by name, so an installation owes a path for
   what every agent needs and owes nothing for what one agent might, and the
   ask carries the derived path where the file stands. **Where a declaration
   elects an arm the installation never provided, the organ refuses the load
   and this crate does not**, per the custody division of section 2. The
   classify arm is the first of that shape and the Spec's sections 6 and 9
   hold it.
   What crosses stays a deployment fact and widens nothing: the socket's name
   derives from the agent name the unit's own name already carries, the binary
   paths are the operator's installed values rather than anything the invocation
   composed, and no part of the agent's declaration reaches the manager.
6. **Dial the channel, direct enter, and receive the aggregate.** The invocation
   connects to the socket the worker bound, retrying within a stated bound because
   the bind is the worker's first act and the dial may arrive first, the bound
   being the Spec's to state. The directive carries the session identity, the run
   ordinal, the kind of the binding, the trace descriptor, the state channel's
   end where the member stands, the model binding, and, where the kind declares
   a Gate, the gate instruction, per the contract. The descriptors ride inside
   the directive over the coordination channel as `SCM_RIGHTS` ancillary
   payload, per `weaver-harness-PRD` section 5, one for the sink and a second
   for the state end where the leg stands, its absence the leg not standing
   and never a refused load, so the worker receives handles and never a path
   and accepts them close-on-exec at its one receive site. Everything after
   the directive and before the answer is the harness's: it stands up an
   empty working structure, authors its `load`
   event, which is the record of admin's contact and the origin of the run's
   monotonic clock, asks the SPU to admit the model, and starts Gate last where
   the kind declares one, so no
   work arrives before the interior can serve it. Admin holds no channel to either
   organ, per section 6, so what admin receives is one answer aggregating the
   fan-out, ready or a refusal naming where it stopped.
7. **Publish loaded and idle.** Only now, and only on a ready aggregate. A partial
   load is never published as loaded, and the published state is idle rather than
   active. Publishing is the invocation's answer and the log's entry, and the
   standing fact behind both is the unit itself: what state an agent is in between
   invocations is a question the init system answers, held by no map of admin's.

Step 6 is the one ask in the sequence, and a refusal at any point inside it enters
the rollback of section 5 carrying the name of the step that refused. A device
conflict discovered at model admission is such a refusal, named by the SPU inside
the aggregate, and admin holds no earlier check to catch it, per ruling C.

**The worker never holds a principal above the agent's, which removes a window rather
than narrowing one.** Under the delegation of section 7 the init system starts the
process already as `weaver-<n>`, so there is no interval in which worker code runs as
anything else and no drop for the ordering of an earlier draft to get right. The
privilege window that section 10 once carried as an open cell has no subject.

**Steps 1 through 5 produce no trace entry, and neither does the rollback path.** That
is a ruling and not an omission. `weaver-trace-PRD` section 3.1 makes the first run's
`load` the record of admin's first contact and places the worker start and the
descriptor
handoff outside the trace by construction, because they run before the harness exists
to author anything. The load's one trace entry is written at step 6 by the harness,
and the unload's at its own bracket. Nothing admin does before that moment reaches the
record, and nothing should, because a second party writing into the stream ends the
single-writer property that makes the account evidence at all. The consequence is
stated in section 2 rather than left for a reader to rediscover here: the
supervisory half of the lifecycle is unrecorded by the trace, and its record is
admin's own.

### 4.2 unload

The reverse in effect and not in shape, because what the load built across seven steps
is unwound in three, and one of admin's acts here, the publish to a state that is not
absent, has no counterpart on the way up.

1. **Dial the channel, direct leave, and receive the aggregate.** The unload
   invocation connects to the socket the worker holds, the same way every verb
   reaches a running worker, per section 6. Everything between the directive and
   the answer is the harness's, in its own order: it stops Gate first where one
   stands, so a Gate
   process never outlives the interior it protects and nothing new arrives, and it
   refuses while a turn is in flight rather than racing one, because a turn
   interrupted mid-decode leaves the SPU holding a session. At rest it authors its
   `unload` event, closing the bracket, drains the writer's queue to the stream,
   then releases the SPU, so residency ends and the device is freed. The answer
   carries left, or a refusal naming where the sequence stopped. Admin holds no
   channel to Gate or the SPU, per section 6, so this directive is the whole of
   admin's part in their unwinding.
2. **Stop the worker unit.** The process exits, descriptors close with it, and the
   unit's cgroup goes with the unit.
3. **Publish provisioned and unloaded.** Which is a different state from absent, and
   absent is reached by an operator act rather than by a verb.

**What leave promises is the drain, and nothing more since 2026-08-01.** A left
answer means everything admitted reached the stream, per the contract's ordering,
and everything after the sink is the operator's, per the ruling section 2 restates.
Nothing is compared, certified, or adjudicated at unload, the leave-time
validation having dissolved with the program-owned record it compared against, and
the stream ends where the run did, finalized by nothing.

### 4.3 validate

Confirms that an agent's configuration and boundary will work before anything is
started against them. It is the front half of `load` and it is also invocable alone,
which is one code path entered two ways rather than two checks free to drift apart.

What it inventories is what a load depends on and cannot discover later. The
configuration file is present and parses. Its required fields are there, the model
binding's artifact among them. The stream sink it declares exists, or the flag that
orders one created is set. The boundary checks of load step 3 run against the
operator's provisioning, and where the declaration elects a store the walk asks the
store the two questions its gates pose, per `weaver-state-PRD` section 4. Every one of
these is a question about a thing admin provisioned or holds custody of, or an answer
the owner gave when asked, and this verb reaches one seam, the store's, and starts no
process, which is what keeps it cheap and free of side effects.

**Admin adjudicates what it holds custody of, asks the owner about everything else,
and never substitutes its own test for the owner's judgment, per the operator's ruling
of 2026-09-05 on issue #456.** The uid, the home, the sink, and the boundary's members
with their modes are admin's to judge, because admin provisions them and holds custody
of where the record leaves the system. A fact interior to another organ is not:
whether these weights load at this precision, whether this family exposes the taps the
declaration elects, whether a Postgres account carries the grant it claims. Where the
owner can be asked before any process exists, the inventory asks and carries the
answer, telling a refusal from an owner that could not be reached, which is the
distinction an ask has and a look does not. Where the owner is the organ a load stands
up, the question waits for admission and the inventory checks only that the
declaration names the thing. The devices are the oldest instance of the rule, **which
this crate does not reason about at all**, per ruling C and the device-assignment
ruling of 2026-08-03: their existence is the SPU's to answer at admission. The grant
surface is the second, read at the enter and never judged, per `weaver-trace-PRD`
section 3.1. **The model artifact is the third, and the one the inventory had been
judging.** Whether it resolves, in the format it is expected to be in, is the SPU's to
answer at admission under the agent's identity, per `weaver-spu-PRD` section 4.1, and
a look taken here runs as root: an artifact readable to root under a directory the
agent uid cannot traverse passes such a check and fails the load, so the check turns
unenforced into documented as enforced, which is worse than no check. The inventory
therefore checks that the artifact is named and nothing more, an empty member refusing
as the declaration's fault, and the SPU's refusal is the one that names the path and
the identity it was judged under, per that charter's section 7.

Reached as the front half of a load, a clean inventory flows into step 4 and a failure
refuses before any process exists. Invoked alone, it stops at the result and reports,
which is the cheap way to test an operator's provisioning before a load depends on it.
It changes no agent state either way.

**It repairs nothing and concludes nothing.** A check that fails emits what was expected
against what was found, per section 2, and the remedy is the operator's over artifacts
this charter does not govern.

**A reachable organ declares its boundary, and a load without one is refused, per the
operator's ruling of 2026-09-05 on issue #438.** Every service reachable other than by
kernel-enforced peer identity carries a declared boundary on the declaration, placement
and boundary moving together because they are one fact about where the agent's identity
can be reached from. A Unix socket is exempt exactly where its peer is judged by kernel
peer identity, by credential where the channel has a name and by descriptor possession
where it does not, per the apex's first invariant, because there the kernel has already
answered. A Unix socket reachable without that judgment is not exempt, and loopback is
not, a bound port being reachable by anything that can reach the host. The inventory
refuses a load where a reachable organ carries no boundary, naming the organ, at the
load's cheapest moment as the family and election refusals are, and the load event
records what was declared so the record carries it. **The rule sets no minimum on what a
boundary is.** A username and a password over a public address is a legitimate
declaration and this crate holds no opinion about it, because the instrument has to be
able to measure the arrangement most people ship and the unsafe declaration is the
useful case rather than the tolerated one. The mechanic is the requirement to declare,
and what a good boundary is belongs to the operator. The ground is completeness of the
record rather than security: whatever is declared becomes part of the cell's identity,
so a reading can never be quoted without saying what was holding the boundary when it
was taken. **The rule stands ahead of any organ it binds.** Every organ in the base
today is reached by kernel peer identity over a Unix socket under a named contract, per
apex section 5, and the SPU's charter reserves no shape for a member off the host, so no
declaration member exists for a boundary and none is added here: a field whose only
reader is unbuilt is the reserved slot apex section 7 refuses. The member arrives with
the first act that charters a loopback or off-host organ, born carrying the boundary
rather than gaining it as a retrofit, and until then a declaration naming such an organ
is refused by the floor's parse as an unknown field, which is the one refusal the
framework can make today and the one the front end may present.

### 4.4 Session close, and removal

`weaver-trace-PRD` section 3.1 carries `session.closed`, meaning the session will not
be resumed. No verb performs it, because a session spans runs and an agent outlives
a session. The harness authors `session.closed` while it is alive, because it is
content and the harness is the sole writer of content. The cost is that closing a
session requires the agent loaded for the authoring, which is load, stop, unload:
the stop exchange ceases token processing and holds everything loaded at rest, and
no close crosses the seam as a verb or a conveyance of its own. What cues the
authoring of `session.closed` inside that window is the session-close cell of
section 10, which this wording does not close. The checksum and the manifest an
earlier version ordered after the drain dissolved with the program-owned record on
2026-08-01, finalization being the operator's over the operator's storage.

**Removing an agent removes no record, because the program holds none.** The
stream's accumulations live with the operator and outlive the process, the session,
and the agent by that operator's own election. Deprovisioning still happens and it
is the operator's, over files and accounts, and this charter states only that no
verb of this crate reaches the operator's storage.

## 5. What a failure partway through leaves behind

Stated by where it failed, because rollback obligations differ and a charter that
says a partial load is never published has not yet said what is left to clear.

**A crate that writes no boundary has no partial boundary to reap**, so rollback is
smaller than it was. What a failed load can leave is a worker process, a connected
sink, and a device the SPU took. Nothing that survives a crash needs a later verb to
recognize and refuse it, because nothing durable of the program's was authored.

A `load` that fails at steps 1 through 4 leaves nothing to reap, and the sentence
that once read "no process exists" is narrowed rather than kept: **the state member
may exist by then**, this crate having started it, per section 2. It still leaves
nothing to reap, for a reason of its own rather than by absence - the member retires
itself, waiting on an accept the worker never makes and ending that wait bounded,
per `weaver-state-Spec` section 4. A sink opened at step 4 is closed, nothing having
been written through it.

A `load` that fails at step 5, or at step 6 before the `load` event is authored,
leaves a worker that never entered a run. Admin stops the unit, and nothing entered
the stream.

**A device conflict refused at model admission is a refusal inside step 6, and it
arrives after the `load` event in the fan-out's own order,** stand up, author,
admit, gate. So what it leaves is an authored bracket with no `unload` on the
stream, plus a device the SPU's refusal left free, per the residency contract. It
needs no clause of its own, per ruling C, because it is a refusal inside the fan-out
like any other, which is what retiring the pre-flight arbitration was for.

**A `load` that fails inside step 6 after the harness has authored its `load` event
leaves a stream whose last run shows a `load` and no `unload`.** That is a truthful
account of a death rather than corruption the program must repair, resume having
dissolved with the program-owned record on 2026-08-01, and what a consumer makes of
a broken bracket is that consumer's reading over the operator's storage. The
run-bracket fix an earlier version of this section carried, and the cell section 10
held for it, dissolved the same day for want of a validator to enforce them.

**Rollback is admin's own reap plus one directive, because admin built with its own
acts plus one directive.** A refused fan-out is the harness's to unwind along the
same seams it fanned out on, and what returns to admin is the refusal naming where it
stopped. Admin's remaining obligations are its own: direct leave where a run was
entered, and stop the unit. Nothing durable of the program's exists to remove. Each
of those can itself fail, and a rollback that cannot complete reports what it could
not undo and does not publish any state, which is the same rule as a partial load
and not a second one.

## 6. The seams

This crate holds one seam to another crate and two contracted boundaries to parties
outside the program. The one-seam claim this section carried until 2026-08-05 was
true of crate-to-crate seams and is restated as such rather than dropped: admin
reaches exactly one crate, and what changed is that the boundaries it already had to
root and to the operator are now written down.

| Seam | Peer | What crosses |
|---|---|---|
| Coordination | `weaver-harness` | Admin dials the socket the worker bound and directs the run to be entered and left, hands the trace descriptor inside the enter directive, and conveys the operator's intent to stop. The harness confirms, refuses, or answers a stop with the turn's fate. A fault the worker survives crosses nowhere here, travelling as the `fault` event on the stream. |
| The unit | the init system, as root | Admin asks for a transient unit under the agent's `User=` with the sandbox properties the operator's template fixes, asks for it to be stopped, and asks what state it is in. The init system starts, holds, and reaps the unit, and it is what keeps an agent alive past the operator's login session. No descriptor crosses. Governed by `weaver-admin-systemd-contract`. |
| The record's exit | the operator | The stream leaves to the sink the operator declared, one event per line, with durability the operator's. Governed by `weaver-admin-operator-contract`. |

**The two outward boundaries carry no seam edge, and the absence is the graph's rule
rather than an omission.** A seam edge runs between two crate nodes, and neither the
init system nor the operator has one, the graph carrying no node for a principal
outside the program. Each is declared by its contract's party edge instead, the same
shape `weaver-gate-world-contract` takes.

```graph
edge: seam
from: weaver-admin
to: weaver-harness
via: weaver-admin-harness-contract
tag: socket
```

**Admin reaches no other crate, and the harness carries what admin cannot.** An earlier
draft drew three seams here, adding a channel to `weaver-spu` for model admission and
one to `weaver-gate` for the membrane. Both are struck. Admin owns one constructed
pathway, and when its intent has to reach the SPU or the gate the harness carries it,
because the harness is the coordinator every organ is situated against and admin is not
a peer of the organs the harness sequences. This is apex section 6 read straight: all
coordination passes through the harness, so admin directing a load is admin opening an
exchange on its one channel and the harness fanning that out along its own pathways.
Admin makes no admission decision at all, per ruling C: the device is the SPU's to
judge, and what admin would have asked the SPU it learns from the aggregate instead.

**The seam has two initiators at the channel, because admin is an organ.** An organ
governs a domain and holds a two-initiator channel with the harness, both properties
and neither alone, and admin governs the lifecycle domain. Either party may open an
exchange by the channel's mechanics, and the chartered census is admin's three, two
that drive a transition and one that conveys the operator's intent to stop. The
harness opens none since the fault-carrier ruling of 2026-08-01: a fault the worker
survives travels as the `fault` event on the stream, the operator's tooling keys on it
there, and the two-initiator property is the channel's rather than the census's, per
the contract's section 0. A worker's death is observed through process exit rather
than reported over the channel, and readiness is a confirmation to a directive
rather than a directive.

**The seam edge is declared by the organ rather than by the harness.** It was declared
by the party that asks, and both parties ask now. The harness is the hub every organ
holds its two-initiator channel with, and a hub that declared its own edges would carry
the whole seam graph in one crate, so the organ declares its seam and the rule
generalizes to every organ the harness gains. Here the organ is `weaver-admin` and the
record above is admin's to state.

**The channel is dialable and the check is what refuses, restated from
unreachability on 2026-08-05.** The socket the harness binds lives inside the
agent's sandbox, per the inversion ruling of this date: any socket connecting to
the harness is an internal connection, so the harness binds and listens as its
first act and every verb's invocation dials in. The name is reachable from the
agent's own processes, and what refuses an elected tool is the accept itself: the
harness reads the peer credential and refuses every principal that is not root,
so the check discriminates on its own. The earlier mechanism had it backwards
twice over. Its fence, the admin-owned 0700 directory, stood between the worker
and the very socket the worker was expected to dial, which is the defect the
review of PR 67 found. And its credential check expected the agent uid, which is
exactly what an elected tool holds, so the check could not tell the worker from
the tool and the one-accept closure carried the refusal alone. Inverted, the
closure is not even wanted: the listener lives as long as the worker and answers
each verb's dial, one connection at a time, because a per-invocation admin has no
standing end to keep.

**This seam authenticates by credential, and the restatement now runs with the
invariant rather than beside it.** Apex section 5.1 reads by credential where the
channel has a name and by possession where it does not. This channel has a name,
the harness's bound socket, so the credential is the check: `SO_PEERCRED` at the
harness's accept reports the dialing peer's uid, root or refused. The earlier form
of this paragraph argued the possession case for an inherited pair, and the
inversion retired the pair along with the argument. The tag stays `socket`, and
the seam is the invariant's first case with nothing restated.

**Close-on-exec is asked for in the creating calls, and the ordering problem is
gone.** The worker's listener and each accepted connection are created after the
worker's last exec, the flag asked for in the creating and accepting calls
themselves, so no set-again-after-exec ordering exists on this channel to get
wrong, where the earlier design's handed-across end needed one. The invocation's
own end is flagged at its connect and lives only as long as the verb. A passed
descriptor is the unchanged case: close-on-exec is a property of the descriptor
rather than of the open file description, so it does not cross with the trace
descriptor and the receiving party is the only one that can supply it. Admin
cannot confer it and does not claim to, and the obligation sits in the contract
where the party that can meet it is bound.

## 7. Identity, process boundaries, and privilege posture

**Two things carry this crate's name, per the operator's ruling of 2026-08-05, and
the third the earlier ruling carried is struck.** The role and the crate remain.
The service account, `weaver-admin-user`, is gone: no system account, no group, no
delegation, because the seat that runs the verbs already holds root, and a
delegation to a lesser principal was machinery for an occupant that no longer
exists. Every ownership clause in this corpus that named the `weaver-admin`
account now names root, and `weaver-<n>` continues to name the agent.

**`weaver-admin-role` is what a human assumes, and the human is the operator
holding root.** The role is assumed at install, when the operator sets up
WeaverTools on a machine they administer, and it is the seat: the acts the
supervisor may perform, taken on and later dropped. **Only a human assumes it, and
never an AI or an automation.** That is a statement about what this program is
designed for and **not a guarantee about conduct** - assuming the role does not
make its holder careful, and nothing here prevents a human from doing something
unwise inside the bound they hold. What it fixes is who occupies the seat, so
scripted fleet management or an agent driving the fleet is a **new role with its
own ruling** rather than a quiet reuse of this one.

**The crate is the lifecycle tool the role runs.** One binary, run with root, one
invocation per verb, exiting when the verb answers. It holds a two-initiator channel
with the harness like every other organ, per apex section 5.4, the property living at
the channel the harness holds open rather than in any standing process of admin's, and
its narrow domain is authorization, the lifecycle's direction, and **custody of where
the record leaves the system**. That last is the organ's reason rather than its
bookkeeping: the trace exits through a sink opened under root, and routing it
through `weaver-gate` instead would put the agent's own account on the one surface
the agent reaches.

**This shape is inherited rather than invented, which is still the argument.** A
machine's administrator holding root and driving services through the init system
is the ordinary arrangement on a Linux system, and this crate exists to inherit
the operating system's trust model rather than to raise a second one above it. The
earlier form of this section built a lesser principal between the operator and the
kernel, a service account with a narrow delegation, and the recut removes the
middle: the authority the delegation bounded is authority root already holds, and
the bound that matters, what the agent can reach, is the kernel's and unchanged.
What outlives the operator's login session is the agent's own unit under the init
system, which is the standing party this program inherits, so nothing of admin's
needs to run for the coordination socket or the sink to survive a logout.

**Reading and analysis stay outside the program, and the recut does not move
them.** A trace carries whatever the agent handled, so the code that parses
finished records consumes attacker-influenced data, and it runs wherever and as
whomever the operator points it, holding no place in this charter. The recut
removes the claim the earlier form made about which uid that is, and keeps the
line that matters: no lifecycle act parses events, per section 8.

**What the operator does with the record past that descriptor is the operator's.** The
sink's three shapes are a file, a pipe, and a socket, so an operator names `/dev/null`,
a FIFO their own loader drains, or a listener they hold. This program opens what was
declared, passes the handle, implements no transport, and elects no destination.

**One unit per agent is not a fleet, and the distinction is the one this program keeps
getting wrong.** The smell this corpus rejected was a shared service accepting many
agents and routing commands among them. A unit running one statically provisioned agent
identity inside a kernel-enforced sandbox is the opposite of that: it is the wall drawn
around exactly one agent, built from OS primitives rather than from a supervisor's
bookkeeping. **It is also the honest answer to keep-alive.** The agent must outlive the
human's login session, and a unit started by the one principal permitted to open that
session is what delivers it - not a backgrounded orphan and not a terminal multiplexer,
both of which tie an agent's life to a shell that was never meant to hold it.

**The agent's uid is statically provisioned, and a dynamic identity is excluded for two
reasons that are independent and are recorded as independent.** The first is durability:
the trace's accumulations are tied to one individuated principal, and an identity
minted per start and discarded at stop would destroy the individuation they rest on.
The second
is mechanical and would hold even if nothing durable existed: **every `SO_PEERCRED`
predicate in this program takes a uid as its subject**, so a principal that changes
between runs leaves those checks with nothing stable to name. Either reason alone
excludes the dynamic form.

**The sandbox is required and its directives are not enumerated, which are two different
statements.** What this charter requires is that the unit deliver the properties: no
privilege escalation from inside, no reach into another principal's home, a bound on
what the agent may consume. Which directives name those properties, and at what values,
is the operator's deployment posture the way a firewall configuration is - and section
11's refusal to freeze a list stands, because a list frozen in a document is a posture
that cannot track its host.

**One property is a question rather than a requirement, and it has a real cost.**
Restricting the address families the unit may open to `AF_UNIX` is **not** a restatement
of this program's no-network-surface rule: that rule binds what these crates link, and
this would bind what an agent's tools may reach. **The egress ruling of 2026-08-07
narrows what that costs, and the port ruling of the same date narrows it again.** A tool
that binds a listening port is external and no longer started by this program at all,
being a registered application the agent addresses across the gate's agent-opened socket
per apex section 3 step 7. A tool that binds none is internal, which by the port ruling
means either loop code inside the worker or a subprocess the harness forks per call.
**Neither is a process this unit starts beside the worker**, so the restriction reaches
the worker and what it forks rather than a tool unit of its own, and what section 10
keeps is whether it is free for those, the worker's own sockets being AF_UNIX by
construction and a forked tool's reach being the question `weaver-gate-PRD` section 7
holds open. It goes on section 10's list as an open question with the cost named rather
than onto this list as a requirement.

**Root performs the privileged acts directly, and the cost is stated rather than
dressed.** Starting a unit under an agent identity, opening a sink the agent could
not, dialing the coordination socket and reading its answers: each is done by an
invocation holding root, not through a grant, a sudoers entry, or a capability on
the binary. An earlier form of this section weighed those mechanisms as ways to
bound a delegated service account, and the account left with the recut. What the
delegation bought, a compromise bounded to the delegated agent set, is not bought
here: a compromise of admin is a compromise of root, and what bounds that surface
is how little the crate parses, named at the end of this section, and how little
it runs, one invocation per verb. Admin still chowns nothing, creates no account,
and provisions nothing, because those stayed operator acts under every reading.

**The name-validation discipline survives the grant it was written for.** The
agent name is validated against the operator's allow-list and shaped as a bare
name before it reaches a filesystem path or a unit invocation, and the paths are
constructed by the crate rather than accepted from anywhere. A name that
traverses is a defect whatever principal runs the verb, so the discipline stands
on its own ground now that no policy file exists to be gotten wrong.

**Admin's own process boundaries, stated because they are a fact about this crate.**
Apex section 5.1's scoped invariant permits a crate calling a crate inside one binary
and forbids the same reach across a process line, which is a test that reads a process
topology no document states. The general sentence naming which crates compile into one
binary is the apex's to write.
What is this charter's is its own half: admin's code compiles into admin's processes
and into no other, the worker holds the harness and the trace and holds nothing of
admin's, and a boundary between two admin processes is a process line like any other.

**Posture through a load, which is simpler than it was.** Admin runs as root
throughout and never as an agent. The worker holds the agent uid from its first
instruction, because the init system starts it there, so no ordering of a drop against
a handoff has to be gotten right and no window exists for one to be gotten wrong. The
trace file is owned by root, the agent uid holds no bit on it and reaches it only
through a passed descriptor, and the trace directory is root-owned and not searchable
by the agent uid.

**Same-uid reach is a live hole and the flag that closes it is a requirement stated
elsewhere.** A tool that binds a listening port is external and is not started by this
program at all, per apex section 3 step 7. A tool that binds none is internal, either
loop code inside the worker or a subprocess the harness forks per call, per the port
ruling of 2026-08-07. **The forked case is the one this paragraph reaches**, and which
uid it runs under is what `weaver-gate-PRD` section 7 keeps open. Where such a process
runs as the agent uid, it can attach to the worker and drive the coordination channel
and the trace descriptor directly, which defeats descriptor scoping. The worker's
dumpable flag is what closes it, and `weaver-admin-harness-contract` section 2 is
authoritative for both the requirement and its ordering, because the flag is a property
admin relies on and cannot verify.

**The one parse surface inside root, named as the trade it is.** Admin reads the
coordination channel's answers and the peer at the far end is a worker running agent
code, so a small fixed message vocabulary is parsed by a root process. The earlier
design parsed it under a delegated service account, and the recut trades that layer
for the simpler topology, knowingly. The bound is the contract's: the vocabulary is
fixed and closed, one envelope is one message under a stated size bound, and a
truncated read is a fault and never a message. Nothing of any turn's content crosses
here.

**Whether external tool processes run as the agent uid is `weaver-gate`'s to rule,
and this charter names it as an assumption.** If they do, the boundary between the
agent and its own worker is hardening rather than kernel-enforced separation. If they
run under a uid Gate owns, the worker's descriptors are unreachable from tool code by
construction. Admin states no requirement beyond assuming the first case is no worse
than hardening, and the cell is filed against the gate charter so it is inherited as
a constraint rather than rediscovered.

## 8. The operator interface, and the domain that left

**The operator interface is the invocation itself.** The role of section 7 runs the
crate with root: a verb and an agent name in, a typed answer or a typed
`lifecycle-refusal` out, the exit status agreeing with the answer. It reports state,
lists agents, drives the three verbs, and conveys the operator's intent to stop
across the contract's stop exchange. It carries no work, per section 3. The socket,
the group, and the peer-credential check the earlier form of this section carried
retired with the recut of 2026-08-05: a surface that authenticated the operator to a
service has no subject when the operator is root running the tool, the kernel having
settled who may execute it. What state an agent is in between invocations is the
init system's answer, which `list` and `show` consult rather than shadow. The
trace's exit remains the contracted external boundary, governed by
`weaver-admin-operator-contract`, which the recut narrows to that boundary: the
stream that crosses out, its sink shapes, and the custody either side may rely on.

**Reading and analysis of finished records is not this crate's, and not this
repository's.** An earlier reading named `weaver-admin-tools` a member of this domain,
chartered by name and left unbuilt. The identity structure of section 7 retires that
reading rather than deferring it further. Reading runs wherever the operator points
it, shares no process with the lifecycle tool, and reaches the record through the
published trace format and nothing besides. Repository and the wire contract that is
the only coupling fall on the far side of the line, and a member of this domain is a
thing this charter bounds, which describes none of it.

**What leaves with it.** Analysis, export, the operator views a graph or a fleet menu
would be built on, and any indexed query over history. Each is a consumer of the
operator-held record on the terms `weaver-trace` publishes, which is what
contract-coupled means here, and none is staged work in section 9, because staged
work is work this crate will later do. Multi-agent management through the operator
interface stays, since it drives verbs rather than reading records.

**Admin reads nothing of the record at all, as of 2026-08-01.** The manifest this
paragraph once held open dissolved with the program-owned record, per the ruling
section 2 restates, so no lifecycle act parses events and the reader edge section
10 once held has no subject.

## 9. Staged requirements

Recognized work with an entry condition that holds it out of this pass. This section
is authoritative for staged work belonging to this crate, per the rule
`weaver-trace-PRD` section 9 sets.

**An admin-side database is not among them, and its absence is a ruling rather than an
oversight.** Staged work is work this crate will later do, and section 8 has said this
crate will not: indexed query over history left with the reading and analysis tooling.
It is therefore not staged here and not staged anywhere in this program, and a reader
who expects a third item is reading a count this section used to carry.

**The binary layout is settled rather than staged, and it leaves this section.** It was
staged as downstream of the grant mechanism, and the mechanism ruling of section 7
removed the thing a second executable would have separated. One authority remains and
it is delegated, so a split buys nothing and the layout is one binary. A staged item
whose entry condition resolves to no work is closed rather than carried.

Nothing is staged here at present. The GID-mask item this section carried was
written against admin owning the record's file, and it left with the record on
2026-08-01, the write path now ending at a sink the operator declares.

## 10. Open rulings

Each names what settles it. A cell with a proposed reading and a named test is a
handoff rather than a hole. Four cells closed in this pass and are recorded as closed
where the closure is recent enough that a reader would otherwise look for them.

**The grant mechanism is closed.** Delegation to the init system, by a transient unit
carrying the agent's `User=`. It was the fourth of four candidates and the ruling that
moved provisioning out of the program left it the only one with a subject, since the
choice was between ways of separating two authorities and one of the two is gone.
Section 7 carries the consequence.

**The layout is closed with it.** One binary. The choice decided the layout rather than
the other way round, and a delegation that holds no capability leaves a second
executable separating nothing.

**The cgroup is closed, and by neither of the two candidates.** It was posed as a
provisioning artifact that load populates against a residency artifact created at load
and torn down at unload. Under delegation the init system creates a cgroup for the
transient unit and removes it when the unit stops, so the second shape holds and this
program does not shape it. The cell is closed by the mechanism rather than by a ruling
on the cell.

**Drop-first is closed and its subject is gone.** The worker starts as the agent uid,
so there is no privilege window to order a handoff against.

**How the descriptors reach a process admin did not fork closed on 2026-08-05, by
dissolution, and both measurements are kept.** The cell asked how a channel end
crosses a privilege-changing exec performed by a party admin does not control, and
the inversion ruling of this date removed every crossing the question needed:
nothing is delivered at the unit's start, the worker builds and binds its own end
of section 6's channel as its first act, and the sink crosses as `SCM_RIGHTS`
ancillary payload on the enter directive, per section 4.1 step 6, over a
connection admin dialed. The sudo measurement stands as recorded: descriptors
above the standard streams do not survive a `sudo` exec, the `-C` override needs
a `closefrom_override` grant this corpus refuses, and a direct fork and exec
delivers where the same exec through `sudo` does not. A second measurement of
2026-08-05 is recorded beside it: a caller-held socketpair end is delivered into
a systemd transient unit by the manager's own descriptor passing, `systemd-run
--pipe`, measured against the user manager with the unit's write arriving on the
caller-held end, the passing path being the manager's own and shared between
instances. Neither mechanism is needed by the design as ruled, and both are kept
so a later pass weighs against facts rather than re-deriving them.

**A file sink is truncatable from inside the wall and the other two shapes are
not, which is a fact about the sink's kind rather than an open cell.** Stated here
because it was weighed on 2026-08-05 and settled rather than deferred. A file
descriptor open for writing permits `ftruncate` whatever `O_APPEND` says, so an
agent holding a file sink's descriptor can destroy the written past, and the fork
window widens that to any same-uid child before its exec. A pipe or a socket sink
hands the agent a descriptor with no such operation, so everything already written
is beyond reach from inside by construction.

**Custody of the past is therefore an operator election and the corpus names it
rather than forcing it.** An operator who needs the record immutable against the
agent elects a pipe or a socket, and one who elects a file has a sink the agent can
append to and truncate. Moving the open to the init system was examined the same
date and declined, per `weaver-admin-systemd-contract` section 0: it bought nothing
the sink's kind does not already buy, and it would have put the record on the unit's
standard output, which every forked organ inherits.

**Session close.** Section 4.4 puts `session.closed` with the harness, at the cost
of requiring the agent loaded for the authoring. What cues that authoring inside the
load, stop, unload window is the open half. **Settled by:** the human's ruling. The
drain-and-checksum half an earlier form of this cell carried dissolved with the
record on 2026-08-01.

**What enter becomes without a record.** Enter stands up an empty working structure,
per the cut of 2026-08-01, so **every run begins with the session's identity and
none of its conversation**. Cardinality and continuity are two statements and
only the second is open: a session may hold as many runs as the operator loads
under it, per section 4.4, and what the program promises between them is
nothing. An earlier wording called sessions single-run at the promise level,
which read as a limit on how many runs a session may have rather than on what
carries between them. Whether continuity returns
through operator-held storage handed back at load, through the memory round's own
substrate, or not at all is a design the corpus defers on purpose. A proposed
reading stands as of 2026-08-01, per the
working-structure ruling: continuity returns as similarity recall, the SPU's
encode side querying the session's NDJSON account rather than any relational
store, which turns this cell into a handoff rather than a hole. **Settled by:**
the memory-and-state round, which takes or declines the proposed reading, and
deliberately not by the cut batch that left it, because the batch deletes what
dissolved and a resume redesigned in a deletion batch would be a load-bearing
decision taken in passing.

**The identity half of that cell closed on 2026-08-14 and the continuity half
did not.** They were filed together because they looked like one design, and
they separate cleanly: whether a later run may read an earlier run's
conversation is the memory question, and whether a reader can tell two runs
apart is not. The second is answerable now and blocks work that should not wait
on the first, a record whose runs are indistinguishable being unusable as
evidence whatever continuity later becomes.

**A run identifies itself and is not counted.** The earlier wording had admin
supply an ordinal that was the next one for its session, which was written when
admin was a standing service and could hold one. The recut of 2026-08-05 made
admin per-invocation with no standing end, and a party that holds nothing
across time cannot know what the next number is. Rather than give admin a
counter to keep, which would return by the back door exactly what the recut
removed, the requirement becomes the one the record needs: **the run
reference distinguishes this run from every other run of its session.** A value
minted at the load satisfies it with nothing remembered between invocations.

**Distinctness has to be argued rather than assumed, because a session may span
agents.** The session is the operator's to name, so two agents can be given one
session deliberately, which is what a benchmark pass across several agents is.
A reference that were only a clock reading would then collide whenever two of
them loaded in the same instant, and the guarantee is distinctness within a
session rather than within an agent.

**The reference has three parts and each does one job.** An instant, so it
reads as a date and sorts into the order the operator's clock saw. The agent's
name, so a reader can see whose run it is without joining anything. And a
value drawn from the operating system's randomness at the load, which is what
carries the distinctness guarantee. **A clock cannot carry it.** Wall-clock
time is adjustable and an adjustment can move it backwards, so no resolution
makes two instants certainly different, and an argument resting on how fast
two loads can be issued is a probability rather than a guarantee. Separating
the parts is what lets each be judged on its own: the instant may be coarse
enough to read, because nothing rests on it, and the guarantee holds whatever
the clock does.

**Ordering is the operator's calendar order and the charter does not claim
more.** The instant is a wall-clock reading, which an adjustment can move
backwards, so sorting references orders runs as the operator's clock saw them
rather than as a monotonic sequence. That is the right answer for a human
reading an artifact and the wrong one for a consumer needing strict order,
which is why order within a run stays the sequence's job, per
`weaver-trace-PRD` section 6. The word monotonic is reserved in that charter
for a different clock and is not borrowed here.

**The session is the operator's to name, and the declaration is where it is
named.** Per section 4.4 a session spans runs and an agent outlives a session,
so a session is a grouping only the operator can draw: a benchmark pass, an
evaluation, a day's work. Deriving it from anything the program mints would put
the program in charge of a boundary it cannot see. It joins the agent's
declaration with the rest of what the operator elects, so the same agent serves
many sessions by editing one field, and admin carries it uninterpreted the way
it carries every other declared value. **What this act does not do is promise
continuity between two runs of one session.** Enter still stands up an empty
working structure, and a shared session name is a shared label rather than a
shared conversation until the memory round says otherwise.

**The operator-to-service seam dissolved with the service, 2026-08-05.** The
socket the earlier closure covered retired with the recut, the operator now
reaching the crate by running it, which is no seam by the Working Process test.
What survives under `weaver-admin-operator-contract` is the trace's exit, the
external boundary contracted on the 2026-08-01 ruling, narrowed to that subject
by this act. The party category the Document Format lacks stays owed to the
Format through the gate's client boundary, the external party that remains.

**Whether there is a configuration-file contract at all.** `weaver-types-PRD` section 6
defers one to this pass, and the ruling that moved authorship to the operator changed
what the question is. Both crates that touch the file now read it, so there is no
producer inside the program and no producer-consumer agreement between two crates to
write. Working Process section 4 rules that only seams take contracts, and this is an
artifact neither party authors. So `weaver-types` is the named authority on the format
under G5, both charters carry their own validation obligations, and no third document
exists. **Settled by:** the human's ruling. Either way the Document Format's sentence
citing that contract is an owed edit, per section 11.

## 11. Edits owed

Apex section 10 requires that a change touching a contract names every party, and
gate G7 asks that a ruling name the documents it changes. A party that does not merge
with the change is named as owed here. This section is that register. Nothing below is
applied by this document, and the items
already carried in `open-items` are cited rather than restated.

**An entry leaves this register when the edit lands.** The register is the G7 instrument
read from the other end, and it has a symmetric failure: a ruling recorded and not
landed reads as settled to every later reader, and an entry landed and not cleared
reads as outstanding. A reader who checks two entries against the corpus, finds them
already done, and stops trusting the rest has lost the whole register for the cost of
two stale lines. Entries for the wire vocabulary, the section 2.2 repair, the
close-on-exec mechanism, and the drop-first privilege window left this way.

**Four more left on 2026-07-31.** The apex 5.1 restatement and the 5.4 definition
landed together, taken early as a named exception to Working Process section 7 and
recorded as such in `weaver-agents-PRD` section 5. The five wire definitions landed in
`weaver-types-PRD` section 2.3 rather than section 4, which is where this register
said they would go, because 2.3 is where that charter keeps definitions and 4 is where
it keeps the departure argument. Both were edited. The harness clause on opening
exchanges landed in `weaver-harness-PRD` section 4. Nothing else in this register
moved while the apex re-authoring waited on all seven charters.

**The act of 2026-08-05 landed and its entries left this register on 2026-08-06,
per the rule above.** The three rulings were the socket inversion, the admin recut,
and loop 0's departure from the loop taxonomy, with the init system's contract cut
during the act's review. Every document that act named carries its change in `main`
at the merge commit, which is where a reader checks them rather than here: a
register that kept landed lines as a changelog would be the second stale-line
failure this section names. What the act left outstanding stays below.

**The re-authoring of 2026-08-01 drained this register.** Every entry that filed
with it landed in that act: the verb-set restatement, the
verification-not-authorship clauses of apex sections 12 and 3, the trust model
stated once, the 5.2 scoping to requests that belong to an existing turn, the
component-ownership drop, the argless qualification on section 3 step 7, and the
process topology. The entries that had already landed by other acts left with
them: the `weaver-harness-PRD` custody restatement with the durable-record cut,
the `weaver-types-PRD` section 2.2 motivating case with the external-boundary
contracts, the Document Format's state-file clause with that document's v0.5, and
both `open-items` entries as that list shrank. The last entry, the
`weaver-types-PRD` section 2.1 rename to `agent-config`, landed on 2026-08-01 on
the human's ruling, and the register stood empty from that date until the act of
2026-08-05 filled it.

**What this register holds, as of 2026-08-06.** Two entries, both from the act
of 2026-08-05 and neither landed by it.

- **`weaver-types-Spec` section 4.2, the `AgentState` gap, partly discharged
  2026-08-06 and discharged 2026-09-04.** The `weaver-admin` code act found the gap
  load-bearing rather
  than theoretical: `show` and `list` had no writable implementation, since the
  answer must be one of the floor's two enums and the only fitting cases carry an
  `AgentState`. The floor gained `StateNotObservable` and those verbs refuse with
  it, so the gap has an honest surface. **What stays owed is the gap itself**, not
  its marker: the enumeration is still unable to say what a running unit is doing,
  and the marker is what a reviewer greps for to find the hole.
  The residency answer of `weaver-admin-Spec` section 3 carries the init
  system's three values, and they do not map onto `AgentState`'s four cases: a
  running unit covers both `Idle` and `Active`, and a failed one has no case.
  The enumeration is still not grown to fit what a service manager happens to
  report, which would settle a vocabulary from a representation. **Settled by:**
  the observation exchange `weaver-admin-Spec` section 11 names, which fixes
  what can be observed before the floor is asked to enumerate it, and which
  retires `StateNotObservable` in the same act. It landed 2026-09-04, per issue
  #435, and the marker left the floor with it.
- **The merged code of `weaver-admin` and `weaver-harness`.** Four things
  contradict what `main` builds: the coordination channel's direction, the
  operator surface's retirement, the fleet map's removal, and the harness
  constructor's move from `Harness::adopt` to `Harness::listen` with the bound
  listener it now takes. Thirteen conformance headers in that code cite
  assertions this act retired, which is the divergence stated as a number a
  check can read. This
  is the expected shape of a phase one re-entry, per Working Process section 2,
  and it is named as owed rather than left for a reader to discover by compiling.
  **Settled by:** the code acts, which follow this act rather than riding it.

**Why the landed entries are gone rather than kept as a changelog.** The rule
above is that an entry leaves when its edit lands, and it has a second half worth
stating once: a register that kept its landed lines would be a changelog, and a
reader checking it would find every line already done and stop trusting the two
that are not. Where the 2026-08-05 act landed is the merge commit and the
documents themselves, and this section is for what is still owed.

## 12. Children

Specs to be written against this charter once the PRD set is ratified. Named so the
set is bounded, not drafted here.

- Boundary verification, covering identity, home, and the trace directory.
- Lifecycle sequencing and the rollback of section 5.
- The coordination channel, covering the transient unit, the dial, and the
  descriptor handoff.
- The operator interface of section 8.

Contracts this crate is party to are written with the PRDs of their other parties,
one per seam in section 6, and are not children of this document.
