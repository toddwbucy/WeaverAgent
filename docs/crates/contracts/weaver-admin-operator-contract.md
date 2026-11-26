# weaver-admin / operator - contract

**Status:** MERGED. In `main` and the source of truth. Written on the human's
ruling of 2026-08-01: the two external boundaries take contracts before any Spec is
written, this document and `weaver-gate-world-contract` in one act, and both are
blockers on the Spec phase. One party is an external principal rather than a crate,
which is the seam category `weaver-admin-PRD` section 10 held open, settled by that
same ruling.

**Date filed:** 2026-08-01
**Document ID:** `weaver-admin-operator-contract`
**Parent:** `weaver-agent-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #533

---

## 0. What this document is

The agreement over the program's outward boundary, as the operator's rulings of
2026-10-03 on #50 draw it: the `weaver-admin` command lines a caller may run and the
answer each prints, how the program's one output leaves it and where it lands, and the
trace relay's read-only stream to the one declared reader. It is read alongside
`weaver-admin-PRD`, whose section 8 names the interface.

It carries no representation beyond the two format rulings of sections 2 and 3. Those
are stated here rather than in a Spec because the party on the far end is outside the
program and builds against this page and nothing else. What a field list contains is
the Spec's. What shape a message takes on the wire is this document's.

**The filename carries the `-contract` suffix like every contract, on the human's
correction of 2026-08-01.** A contract is named as one whatever its parties, per
Document Format section 2, and the external category is carried by the party prose
below and by the party-edge category the Format is owed per section 9, never by a
withheld suffix. An earlier form of this document withheld it as an
exclusion-by-naming mark and was corrected the same day.

```graph
node: weaver-admin-operator-contract
kind: document

edge: party
from: weaver-admin-operator-contract
to: weaver-admin
```

The second party is the operator, one person in the operator role, per
`weaver-admin-PRD` section 7: at a root shell a human holding sudo, and through a sudo
rule the operator installs, the box's admin-con acting for that person under its own
system account, never the operator's account. Human and machine accounts stay separate,
on the operator's ruling of 2026-10-03 on #50: the connector holds only fixed lines, so
it can write none of the operator's files, and WeaverWeb's audit names the person while
the box's record names the service uid sudo reports. **One admin-con account serves
every agent on the box**, `weaver-admincon`, on the operator's rulings of 2026-11-10,
which supersede #50's per-agent account. It holds the fixed lines of each agent whose
sudo rule names it and is that agent's trace reader, and it is in no gate rule, the
box's gate-con account being the one caller `weaver-gate-world-contract` section 1
names, so neither account reaches the other's door. Least privilege holds of what the
account can do and not of how many agents it reaches: it holds the agents' fixed lines
and their read-only doors and nothing else, for the reason `weaver-admin-Spec` section 2
states. An agent whose reader is its own per-agent account and whose gate rule names the
operator keeps both and is not refused for them. The graph carries no node for a
principal outside the program, so the party is named in prose and the missing category
is registered rather than improvised.

## 1. The boundary

**This contract binds three crossings: the command lines, the sink, and the trace
door.** **The command lines** are `weaver-admin <verb> <agent>` run as root, at a root
shell or through a sudo rule the operator installs as the box's own boundary, which
names the box's admin-con account and the exact command lines for this agent and allows
no caller-chosen argument, per `weaver-admin-Spec` section 2. An executed program is no
seam by the Working Process test, and what this contract fixes is which lines exist and
what each prints. **The wire between WeaverWeb and its connector is WeaverWeb's**:
WeaverWeb sends admin-con an abstract verb, admin-con maps it locally to one of the
fixed command lines, and no command crosses a network. **The sink** is a real crossing:
admin passes the sink handle across the boundary and the program writes through it,
something of the operator's stands behind it, and neither side sees the other's
interior. **The trace door** is the relay's door, standing only for a file sink,
reachable by the agent's own access group alone and admitting exactly one declared
reader, whose identity the box verifies and the caller never asserts. Where it stands
and how it is reached are `weaver-admin-Spec` section 6's. **Section 8 states a fourth, the
service's socket**, on the operator's rulings of 2026-11-26: act one's code stack builds
it, and moves the command lines and the trace door behind it. None is network ingress, and
none breaches anything Gate holds, on the grounds `weaver-admin-PRD` section 3 states.

## 2. What crosses in

**A granted command line, and nothing else from the caller.** The verbs are `show`,
`validate`, `load`, `unload` and `stop`, each with the agent's name, on the operator's
rulings of 2026-10-03 on #50, and, as of A3.2 on the operator's rulings of 2026-10-06 on
#1, `save-point`, which takes a save point of the running agent and publishes it at
once, `restore`, which names the save point the next load restores, the one the
declaration's `[restore]` names and never one the caller chooses, and `force-unload`,
the unload that stops the work and keeps the state: it waits for no turn and runs no
wind-down, takes the leave save point all the same, and comes down without it, the loss
recorded, only where it cannot be taken, per section 4 and `weaver-admin-Spec` section
3, the forced unload.
**Which lines a caller may run is the rule the operator installs**: an observer's rule
grants `show`, and an operator's adds `validate`, `load`, `unload`, `stop`, `save-point`,
`restore` and `force-unload`, the role split of #50 mapped onto command lines. Nothing
else crosses in: no argument the caller chooses, no standard input, which the program
never reads, and no claim of who asked. **The cause the record carries is the uid sudo reports**, and which
person asked is WeaverWeb's record and never the agent's. **The rule opens no login
session for these lines**, on the operator's confirmation of 2026-10-03 on
toddwbucy/WeaverWeb#15: sudo's `pam_session` is off for the connector's user
(`Defaults:<admin-con user> !pam_session`). Since the operator's ruling of 2026-11-07 on
#166 the placement is admin's own step, which leaves any session's containment, so a
session would no longer break the agent's lifetime. It stays off because none is
needed and the granted rule does not change. A requirement on the boundary is declared
and checked: **every constituent sits inside the agent's own root-owned delegated
scope, which admin makes for the load**, in the agent's containment admin makes beneath
it, and the invoker verifies after a load that every constituent `show` names sits
there. **The host must let admin make that scope, delegated, root's, and admitting a
child containment that root makes**, or the load refuses `BoundaryUnverified`. Root's
means owned by root, the scope and every containment above it, so no account but root
can move the agent's processes out of the agent's containment (#166's guard, the
operator's ruling of 2026-10-15). Admin makes the scope the same way whoever invoked
the load, so a load from admin-con's unit stands as an operator's does. Where the scope
stands, its slice, and the request that makes it are `weaver-admin-Spec` section 6.1's.
No prompt, turn, task, or run crosses,
per apex section 6.

**The invoker's execution context reaches admin and the agent it starts**, whatever
cgroup they run in: a move changes no privilege, capability, namespace, mount, filter
or resource limit. So the invoker's context must leave the program what it needs: no
`NoNewPrivileges`, under which sudo cannot gain root, a capability bounding set that
does not narrow root's (or one the operator has measured to suffice, the scope's
request included), no system call filter, an address family filter that admits
`AF_UNIX`, no private or read-only view of `/sys/fs/cgroup`, `/run`, the agent's
territory, the admin config root or the stack's prefix, and resource limits sized for
the agent, model loading included, or unset. The invoker's cgroup limits no longer
contain the agent, which runs in its own scope. That is the invoker's install's to
provide (the operator's ruling of 2026-11-09 on #166).

**At the trace door, one request line**: a byte offset on a record boundary and the
digest of the record before it, the shape `TraceRequest` of `weaver-types-Spec` section
3.1, from the one `trace-reader` the agent's boundary file declares, the box's admin-con
account. Every other caller is refused: one outside the agent's access group by the
box before the relay sees it, and one inside the group but not the reader by the relay,
which logs it. The newest connection from the reader replaces the old.

The record is one-way by construction, so what an operator decides after reading it
re-enters by a command line rather than by answering on this boundary.

## 3. What crosses out, and its format

**The output stream, which is the program's one output.** Per the human's ruling of
2026-08-01: the program tees the exact content of the working structure to an NDJSON
stream, one event per line, authored against the durable event schema `weaver-trace`
owns, and hands it to the operator. **Durability is the operator's responsibility and
not the program's.** Retention, indexing, persistence, and every view built on the
record are backend work, served by separate tooling on the operator's own compute,
addressed to that operator's specific needs, and outside this program entirely.

**What the program promises is the tee, and the promise states its own bounds.**
What reaches the stream is in order and identical in content to what the working
structure holds. What can fail to reach it is bounded and named, twice and no more.
The writer's queue trails the working structure and its tail is forfeited to process
death, per `weaver-trace-PRD` section 4.2, an abrupt exit covering nothing the queue
still held. And under sustained pressure from a slow sink the stream side does what
the tee back-pressure election rules, blocking the emitter, shedding with the gap
marked in the stream, or detaching with the detachment marked, an election the spec
seat takes against a real consumer at a real rate and nothing this contract
pre-decides. Nothing is ever shed silently: a stream that lost events says so in the
stream, so a record with no marks and a record that lost something are never
confusable, and a silent drop is a broken build rather than a policy.

**Where the stream lands is the operator's declaration.** The sink is named in the
agent's configuration, validated by admin at load like every other field, and the
stream is connected to it under root, the role's principal. A file, a pipe, and a socket
into the operator's tooling are all conforming sinks, and the program treats them
alike: it writes the stream and holds no opinion about what stands behind the
handle. Whatever its kind, the sink stands in the agent's territory, and only a file
sink is served through the trace door. The mechanism is the Spec's. **The declaration itself lives in the agent's
territory**, root's, on the operator's ruling of 2026-10-07 on #1: the operator writes
it, the connector reads it and never writes it, and admin reads it only as part of the
territory it has judged. Its path, how the operator edits it and how each party reaches
it are `weaver-admin-Spec` section 9's.

**Custody survives the ruling and survives the recut.** The agent principal does not
reach
the stream's sink, per the boundary `weaver-admin-PRD` section 2 verifies, so the
record's exclusion of the agent does not depend on who persists it. What the recut of
2026-08-05 changed is the principal holding the handle, from a service account to
root, which narrows the set of parties that can reach the record rather than widening
it. What it did not change is the direction of the exclusion: the agent writes
through a handle and reaches nothing behind it.

**For a command line, one answer object on standard output**: one `lifecycle-answer`
or one `lifecycle-refusal` in the floor's rendering, the exit status agreeing with it,
per `weaver-admin-Spec` section 2. That pair, the granted lines and the object each
prints, is this repository's interface to a connector, and nothing else is. **Its bounds
are named.** The object is at most 64 KiB. Exit 0 is an answer and 1 a refusal, and any
other status, or a status with no object, is a fault the caller answers by reading the
next `show`. **`show` names the run's constituents** where a run holds the agent's run
lock, on toddwbucy/WeaverWeb#15: the process id of every one of them, the worker, the
state member and the trace relay, so a caller can check that each sits in its own
containment at every load. They are absent where no run stands. **`Unloaded` means
nothing resident**, and `show` prints it only once the run lock is free and no process
of an ended run still stands (`weaver-admin-Spec` section 6.1), printing `InTransition`
naming them where one does. While any
constituent still holds it and no worker answers, a run that never entered or one
ending, `show` prints `InTransition` with the constituents named (`weaver-admin-Spec`
section 5), in the answer's optional `constituents` field (`weaver-types-Spec` section
3.1). Standard error carries
human-readable diagnostics no caller parses. A
`load` answers once the agent is up or refused, within admin's own bound of 900 seconds
by default on the agent's start, which a caller's bound must exceed, after any
publication of recovered files from an unclosed run, which is bounded by their size and
not by time. A load whose agent does not start inside that bound rolls back before it
answers, within the rollback's bound `weaver-admin-Spec` section 3 states (a load's
rollback), which includes the wait for the agent's processes to end. **An invocation finishes even when its
caller disappears**, its outcome recorded in the agent's `admin.log`, so a caller that
gives up reads the outcome from the next `show`.

**The agent's lifetime is its own scope's**, on the operator's ruling of 2026-11-07 on
#166, which replaced the binding to the containment a load ran in (the rulings of
2026-10-03 on #72 and of 2026-10-14 on #111): the root-owned delegated scope admin
makes for the load (section 2, its making `weaver-admin-Spec` section 6.1's). It is
enforced by that scope, and the program depends on no supervisor for it. The agent
lives until `unload`, `force-unload`, root stopping its scope, or its own processes
ending. The invoker's end, admin-con's restart included, ends neither the agent nor a
start step already in its scope, so the binding under which stopping an agent's
admin-con stopped that agent (toddwbucy/WeaverWeb#15) is reversed. A unit per agent
stays later fleet work. A dropped invocation still runs to completion, per the bound
above. Three
consequences are the invoker's to build against: a stop of the scope is an unclean stop
whose next load resets to the latest save point, the invoker's execution context
reaches the agent (section 2), and the invoker's cgroup limits do not. Admin sets no
resource limit on the scope, so the host's defaults apply, and a limit across the
agents' scopes is later fleet work. admin-con's unit shape is held on
toddwbucy/WeaverWeb#46.

**At the trace door, the record as a read-only stream**: the relay writes a header line
naming the file's identity (device, inode, and birth time where the filesystem reports
it) and its length at the relay's size read, the end of the last record complete at
that read and so a record boundary (#88), then the trace's own lines from
the verified position exactly as written, following new lines with a heartbeat while
idle, every added line a `TraceLine` of `weaver-types-Spec` section 3.1. It serves
the file the loaded run opened, so a rotation of the sink's path changes
nothing it reads and reaches the reader as a new identity in the next run's header, ends
the stream on a truncation rather than smoothing it over, and refuses a position that
does not verify before a byte is sent. **The length places a boundary for a reader that
reads only through the door**: where the file's complete records ended at the relay's
size read, once the request's position verified, so it can bound a backfill or close a
replay at that moment. Every record past it completed at or after the read. **It is a
lower bound for the reader**: a record past it may have completed before the header
reached the reader, or before the reader's next act. A reader bounding an act of its own
(a verb's invocation) places the act by the trace's own events, the act's event, never
by the length alone. A boundary mid-stream is taken by a new request: the newest
connection from the reader replaces the old, so a reconnect yields a fresh header and
its length, and no other line carries one.
A reader of the record this way reads the file through no grant of its own,
so the territory's layout gives it nothing.

**The stream is also the program's one fault carrier,** per the fault-carrier
ruling of 2026-08-01. A fault the worker survives rides it as the `fault` event of
`weaver-trace-PRD` section 3.1, in order with everything else, and the operator's
tooling keys on the fault fields for its own purposes. There is no second alert
path anywhere in the program, and tooling that decides a fault warrants action
comes back by running a verb, per section 6.

## 4. Ordering

- The stream is ordered by the writer's queue and trails the working structure, with
  no cadence to elect and no window to tune.
- The stream's order is independent of any invocation's lifetime. An operator's verb
  answers and its process exits while the stream continues, because the writer is
  the worker's and the sink is held by whatever the operator put behind it.
- The request-ordering rules this section carried retired with the socket. What
  ordered two asks was one connection serving them in turn, and what orders them now
  is that each is a process the operator starts, per `weaver-admin-Spec` section 3.

### 4.1 What each command line answers, in each state of the agent

**The agent's lifecycle is the lifecycle state table** approved by the operator on
2026-10-09, recorded on #1, which `weaver-admin-Spec` section 3 holds: its states S0 to
S11 and invariants I1 to I9 are that section's, cited here by their ids. This
subsection is the table read from the caller's side: what each granted line prints in
each state. Where it and the table disagree, the table holds. A caller never names a
state; it reads `show` and acts on what it prints.

| State | `load` | `unload` | `force-unload` | `save-point` | `restore` | `show` |
|---|---|---|---|---|---|---|
| S0, down, clean | the load's answer, or its refusal | `Unloaded`, nothing to do | `Unloaded`, nothing to do | `OutOfOrder` | `RestoreNamed` | `Unloaded` |
| S0d, down, a run ended without closing | as S0, the load recording the reset, or `AgentRunning` naming them where processes of the ended run are still standing (`weaver-admin-Spec` section 6.1) | ends any process of the ended run still standing, then publishes the save points the run left as recovered, then `Unloaded`, or `SavePointNotTaken` naming `published` where that fails. The marker is left as it stands, and the next load records the cause it carries (`NoCleanUnload` where the run was left open, `ForcedUnload` where a force closed it) | ends them and publishes the same way, then `Unloaded`, or refuses as `unload` does. The marker is left as it stands, a force on an agent already down having ended no run, and the next load records the cause it carries | `OutOfOrder` | `RestoreNamed` | `Unloaded`, or `InTransition` naming the processes of the ended run still standing |
| S1, loading | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight`, a load being no unload to join | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S2, serving at rest | `AgentRunning` | the graceful unload, below | the forced unload, below | `SavePointTaken` | `RestoreNamed` | `Idle` |
| S3, serving, a turn in flight | `AgentRunning` | the graceful unload, after the turn | the forced unload, the turn cancelled | `ActivityNotAtRest` | `RestoreNamed` | `Active` |
| S4 to S7, a graceful unload in progress | `InvocationInFlight` | `InvocationInFlight` | joins, below | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S8a, an unload concluding | `InvocationInFlight` | `InvocationInFlight` | waits for the unload to finish, then answers as `show` would: `Unloaded` once nothing of the agent stands | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S8b, the worker gone with no answer (it died), a process still holding the run | `AgentRunning` until the run ends, then as S0d | ends what still holds the run, then publishes what the run left as recovered. The next load records `NoCleanUnload` | the same as `unload`, the marker left as it stands | `OutOfOrder` | `RestoreNamed` | `InTransition` with the run's processes named while a process holds the run, and after that while any process of the agent stands, naming those. `Unloaded` only once none does |
| S9, an unload stopped at its save point | `AgentRunning` | retried: the save point, then down | the forced unload | `SavePointTaken` | `RestoreNamed` | `Idle`, the gate lowered |
| S10, a force in progress | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S11, a silent worker being ended | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` where a force is ending it; beside an `unload` ending it, joins, below, and ends it beside the unload, the second ending changing nothing | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |

**The table is for one command at a time** (`weaver-agent-PRD` section 6.1, the
operating envelope): a caller issues one command and waits for its answer, the one
admitted overlap being `force-unload` joining an `unload`. **A command killed
mid-command is outside the envelope** and gets no takeover: the run finishes or dies on
its own, and a later command finding the agent's lock free mid-transition answers
conservatively (`weaver-admin-Spec` section 3, outside the envelope): `load` refuses
`AgentRunning`; `unload` refuses `OutOfOrder` where a leave is already in progress, or
ends a run whose agent no longer listens; `force-unload` ends whatever stands, marking
it forced where it ended a live agent mid-unload, and leaving the marker as it stands
where the agent had already died (S8b), the crash and not the force having ended the run; `save-point` refuses `OutOfOrder`. The next load recovers any save point
left in the room, and its reset may be conservative but is never clean where the state
was not kept.

`restore` names the save point the next load restores in every state that answers it,
the live restore being A5's. `stop` and `validate` take the invocation lock as every
line but `show` does, so each refuses `InvocationInFlight` wherever the table shows an
invocation holding it (S1, S4 to S7, S8a, S10, S11); a `stop` in S9 answers `AtRest`,
and in S8b, where no invocation holds it, both take the free lock: `stop`, no worker
listening, refuses `Unanswered` once the dial's bound passes, and `validate` answers as
it does in any state, starting nothing. The
lock is taken before the state is read, so a line that meets a held lock refuses
`InvocationInFlight` whatever the state behind it. `show` alone reads beside a holder, and
what it prints is one ladder (`weaver-admin-Spec` section 3): `InTransition` at once
where another command holds the agent; otherwise the agent's own word where it answers
(`Idle` or `Active`, or `InTransition` while an unload is in progress); `Unanswered`
where it accepts and does not answer in time, a worker wedged; `InTransition` with the
run's processes named where no agent listens and a process still holds the run, a run
that never started or one ending. It prints `Unloaded` only once nothing holds the run
and no process of the agent stands. **Every `Unloaded` this section names is printed only once
nothing of the agent stands** (`weaver-admin-Spec` sections 3 and 6.1): where its
processes do not all end within the containment bound, the command refuses
`WorkerWouldNotExit` in its place and publishes nothing (section 5).

**`unload` drains, winds down, saves, then goes down** (`weaver-admin-Spec` section 3,
the graceful unload). It may take the length of the turn in flight and one generation
more: from its first step the agent takes no new request, each one it had admitted and
not started refused on its own connection with `Unloading`, "the agent is unloading", the operator's
seeding line among them (S4 x dialer request); the turn in flight finishes what it can
without further input, and a tool call it has out is interrupted, its return recorded
interrupted by the unload and re-runnable after the reload, unless its result was
already on its way, which stands, is delivered and is never re-run (S4 x tool return); the agent
then writes one wind-down turn summarizing where the work stands, for the reload (S5);
then the gate closes and the leave's save point is taken. The `unload` holds the
invocation lock throughout (I1). It prints `Unloaded` with the marker closed, so the
next load records no reset; or, where the unload stayed graceful to the miss,
`SavePointNotTaken` naming the leg the save point missed, the run left standing at rest with its gate closed, for a retried `unload`, a
`save-point` and then an `unload`, or a `force-unload` (S9), while one that turned forced, by
`weaver-admin-Spec` section 3's definition of `forced` (directed forced, joined, a
declared bound passing, or a dead gate or SPU), comes down without it, prints `Unloaded` with the
marker `Forced`, and the next load records `ForcedUnload`; or `SavePointNotTaken`
naming `published`, the run ended and its save point standing unpublished, so the next
load records `NoCleanUnload` and recovers the file (S0d).

**The graceful unload has no time bound unless the agent declares one**, on the
operator's ruling of 2026-10-09 recorded on #1: the drain and the wind-down are
unbounded by default, and `force-unload` is the caller's recourse (I3). An agent may
declare `drain-bound` and `wind-down-bound`, in seconds, in its declaration's
`[lifecycle]` table, per `weaver-types-Spec` section 2; past a declared bound the leave
turns forced, the record naming no `forced_by` and keeping the `unload`'s caller as the
leave's cause, and the `unload` concludes as a forced leave does. With no bound
declared, or one alone, the `unload` waits for the agent's answer with no deadline, and
a caller's own timeout must allow for that; with both declared it waits their sum and
150 seconds, then escalates as `weaver-admin-Spec` section 3 states. The escalation's
waits, the wait for the agent's processes to end at the run's end (`weaver-admin-Spec`
section 6.1) and the publication after the answer come on top, their total stated once in
`weaver-admin-Spec` section 3.

**`force-unload` always ends the run, alone or joining an `unload`** (I3).
- **Alone** (S2, S3, S9): the gate closes at once
  and every request it holds is recorded refused and its connection closed; the turn in
  flight is cancelled and recorded as a stop for the unload; no wind-down runs; the
  leave save point is taken as at any unload. It prints `Unloaded`, or, where the agent's
  processes do not all end within the containment bound, refuses `WorkerWouldNotExit`,
  publishing nothing and answering no state. Where the save point
  published, the marker closes and the next load records no reset; where it could not
  be taken, the marker stands `Forced` and the next load records `ForcedUnload`; where it
  was taken and did not publish, the force has still ended the run, but it refuses
  `SavePointNotTaken` naming `published` with the marker left open, so the next load
  records `NoCleanUnload` and recovers the file (I4).
- **Beside a graceful `unload`** (S4 to S7): it joins without the lock, turning the
  pending leave forced from where it stands, and the record names both callers, the
  `unload`'s as the leave's cause and, as `forced_by`, the first force's that turned it
  forced while it was still graceful (I2), a later force, or one joining a leave a
  declared bound already turned forced, named nowhere. Every caller the agent answers concludes
  the same way (`weaver-admin-Spec` section 3, the conclusion): the `unload` holding the
  lock first, publishing and closing the marker, and the force after it, finding the run
  concluded and changing nothing (I1); each prints only once the run lock has freed,
  never on the agent's answer alone, so `Unloaded` means no process of the agent still runs. From
  S7 on, the leave's outcome being fixed, the join adds nothing to it and concludes the same way,
  and once the agent no longer listens it waits for the unload to finish: at most 150
  seconds and then the escalation while the agent's processes still hold the run, and
  with no deadline once they are gone and the unload is publishing, a publication taking
  as long as its copy does. It then answers as `show` would, `Unloaded` where nothing
  of the agent stands, nothing being resident: ending the run was its job, and whether the unload's save point
  published is the unload's own answer and the marker's, for the next load.
- **Beside any other command** (a load in S1, its rollback included, a `save-point`,
  another force): it refuses `InvocationInFlight` and takes over nothing, one command
  at a time being the caller's to keep (`weaver-agent-PRD` section 6.1). **It knows the
  command by the verb the lock's holder records beside the lock**, never by guessing
  from the agent's answers (`weaver-admin-Spec` section 3, the invocation lock). A `show`'s
  brief shared hold is waited out, as by every command.
- **On a worker that answers nothing** (S11): a join unanswered within 150 seconds ends
  the run's processes without the lock (I3), and only then writes the marker `Forced`,
  by `weaver-admin-Spec` section 4's marker-write rule. The `unload` holding the lock is answered no `Left`,
  so it concludes nothing and releases the lock, and the marker stands `Forced`, the
  escalator's. The force prints `Unloaded`, or `LockHolderUnknown` or `WorkerWouldNotExit`
  where the escalation cannot end the run, per section 5.

**A caller whose invocation is killed mid-command is outside the envelope**
(`weaver-agent-PRD` section 6.1): no later command takes over its work. The run finishes
or dies on its own, `force-unload` ends whatever still stands, and the next load
publishes any save point left in the member's room as recovered and records the reset
the marker carries, `NoCleanUnload` where the state was in fact kept being a
conservative label with nothing lost (`weaver-admin-Spec` section 3, outside the
envelope).

## 5. Failure

**At the sink, one failure.** A sink that cannot be opened refuses the load, per
`weaver-admin-PRD` section 4.1 step 4, and a sink that fails mid-run is the tee's
bounded loss of section 3 rather than a refusal, because a run does not stop for its
reader.

**At a command line, the floor's refusals**, per `weaver-admin-Spec` section 3: a verb
arriving while another invocation holds this agent's invocation lock refuses
`InvocationInFlight` before touching anything, `force-unload` alone excepted, which
joins, waits or escalates per section 4, and `show` then answers by the ladder above. A
second `load` of a running agent answers `AgentRunning` and touches nothing, a load
never ending an existing run, whether or not that run ever entered. A missing or
malformed boundary file refuses `ConfigInvalid` naming `roles.toml`. An `unload` that
cannot pin the run lock's holder refuses `LockHolderUnknown`, and one whose worker still
holds the run lock after the escalation refuses `WorkerWouldNotExit` and answers no
state. An `unload` whose leave save point was reported and did not publish refuses
`SavePointNotTaken` naming `published`, admin's own leg, and leaves the marker open for
the next load's reset, per `weaver-admin-Spec` section 3: unlike every other leg, the
run has ended and the save point stands unpublished in the member's room. A
`save-point` whose publication does not land refuses `SavePointNotTaken` naming
`published` too, the run still open, and so does one whose report names another run than
the one the marker stands open on: its file is published as recovered, never as this
run's save point, so no save point of this run was published (`weaver-admin-Spec` 3.0,
the `save-point` verb's publication; #140, S17). **`SavePointTaken` is the
hash-confirmed write** (#157): the verb answers it only once the save point's published
copy has been read back through the territory's `save-points/` and digests to its
manifest line, the digest the answer's report carries, and a copy that does not read
back refuses `SavePointNotTaken` naming `published`, as does an `unload` whose leave save
point fails the same read-back; a `load` whose publication meets one refuses
`BoundaryUnverified`, as on any save point left in the room. The read-back proves the published
file's bytes, as the kernel serves them, digest to the line; durability on the device
rests on the syncs `weaver-admin-Spec` section 6 makes before it. **A member's room past 4096 finished
names refuses `BoundaryUnverified`** at every verb that publishes (a `load`, an `unload`,
a `save-point`), until the operator clears it (`weaver-admin-Spec` section 6; #140,
S16). **Containment refuses in these ways** (`weaver-admin-Spec` section 6.1, #111). A
`load` on a box without a containment that admits a child made by root and ends all of
its processes at once (the floor is `weaver-admin-Spec` section 6.1's), one for which
admin cannot make the agent's scope and read itself inside it within the bound
`weaver-admin-Spec` section 6.1 states, or one whose scope is not delegated, not
root's, or admits no child, refuses `BoundaryUnverified`,
naming what is missing. A `load` meeting processes of an ended run still standing, in
the agent's scope or its containment, refuses `AgentRunning`, naming them, for `unload`
to end. An `unload` or
`force-unload` whose agent's processes do not all end within the containment bound refuses
`WorkerWouldNotExit`, naming them, and answers no state. Any verb after a load that
finds, in the recorded place, a containment other than the one that load made refuses
`BoundaryUnverified`, naming both, and acts on nothing. A recorded containment found
gone was already removed, so a `load`, an `unload` or a `force-unload` clears the stale
record and goes on, and `show` reads it as nothing standing and leaves the record. Where
clearing it fails, an `unload` or a `force-unload` concludes and leaves the record, and
a `load` refuses `BoundaryUnverified`, naming the record. A record of the containment
that does not read refuses `BoundaryUnverified` too, and so does a load that finds a
containment where the agent's would be made that is not root's. A run's end whose
containment cannot be removed refuses `BoundaryUnverified`, naming it, and publishes
nothing.
`force-unload` takes the leave save point as
`unload` does and ends the run whether or not it can be taken, refusing
`SavePointNotTaken` naming `published` where it was taken and did not publish, per
section 4.1. A `stop`
or a `show` whose answer does not arrive within its bound refuses
`Unanswered`, and so does a `load` meeting a run whose worker is silent, each leaving
the run as it stands for `unload`. **Recovery from a killed invocation is the
caller's**: admin-con reads `show`'s facts and issues `unload`, which ends whatever
holds the agent's run, then `load`, per `weaver-admin-Spec` section 3. A line a caller's
rule does not grant never reaches the program, sudo refusing it, so it has no refusal
here. **At the trace door, refusals before a byte is sent**: a caller outside the
agent's access group, refused by the box before the relay sees it and so logged by no
one here, and a member of the group that is not the declared reader, and a position that
does not verify, each of the last two logged in the agent's `admin.log`. Where the sink
is a pipe or a socket no relay stands, so the door is closed, and a relay that has died
closes it until the next load, both of which a reader sees as a refused connection.

The ask-side cases this section enumerated until 2026-08-05 travelled with the socket to
`weaver-admin-PRD` section 8 and its Spec: the malformed request, the unknown agent, the
request carrying work, and the config's registered-field failure are all still refusals,
typed as `lifecycle-refusal` and returned by the invocation. The peer predicate is not
among them anywhere, the trace relay judging one declared reader and the command lines
judging nothing of the caller. An organ refusing a field it registered is still not on
any of these lists: that refusal is the organ's, travels back through the harness on its
own seam, and reaches the operator inside the aggregate.

## 6. Prohibitions

**On admin.** It carries no work inward, however an invocation frames it. It reads
nothing from a caller but the granted command line. It emits the stream to the declared
sink, and serves the sink's file read-only through the trace relay to its one declared
reader, and to no other reader, and it repairs, reconciles, and adjudicates nothing on
the way through, per `weaver-admin-PRD` section 2. The relay does nothing with the
record but read it.

**On the operator's tooling.** Nothing behind the sink reaches back. The stream is
one-way, and tooling that wants to act on what it reads comes back by running a granted
command line. **On a connector.** It maps WeaverWeb's verbs to the fixed lines locally,
asserts no identity the kernel did not supply, and reads the record through the trace
door rather than through any grant into the territory, fanning it out, if at all, on
WeaverWeb's side and never the box's.
The monitoring is the outside's job and the verb is admin's, per the basic loop's
section 2.

## 7. Vocabulary

**Drawn from `weaver-types`:** `lifecycle-refusal` and `lifecycle-answer`, which a
command line prints, and since 2026-10-03 (#50) `trace-stream`, the trace door's
vocabulary, whose satellites are `weaver-types-Spec` section 3.1's `TraceRequest`,
`TraceHeader`, `TraceLine` and `TraceControl`, with `peer-identity`, which the relay
reads from the kernel. `authorization-predicate` stays undrawn: the relay judges one
declared reader, not the allow-and-deny rule the gate's client boundary and the
coordination seam share.

**Drawn from `weaver-trace`:** the durable event schema, as the content of the
output stream. It is drawn as published format rather than as a linked type, which
is the contract-coupled reading `weaver-admin-PRD` section 8 states.

**Drawn from `weaver-traits`:** nothing. The clause is present with that answer
because `weaver-types-PRD` section 5 asks for it even when it is empty.

The directive draws left this clause with the asks at the recut and stay out: the verbs
enter as a command line's arguments, which `weaver-admin-PRD` section 8 governs. The
answer and the refusal are drawn because each command line prints one.

**The clause above is stated in edge form here**, per Document Format section 4, which
makes `draws` the vocabulary clause a query can walk and is what turns G4 from a
reading into a query. The block sits at the clause it argues rather than in section 0
beside the party edge, per that format's section 6.

```graph
edge: draws
from: weaver-admin-operator-contract
to: lifecycle-refusal

edge: draws
from: weaver-admin-operator-contract
to: lifecycle-answer

edge: draws
from: weaver-admin-operator-contract
to: peer-identity

edge: draws
from: weaver-admin-operator-contract
to: trace-stream
```

**The draw from `weaver-trace` takes no edge and the reason is stated rather than
left to be inferred.** What crosses here is the durable event schema as published
format rather than as a linked type, per `weaver-admin-PRD` section 8, so the clause
names a format an external reader consumes and not a vocabulary node this contract
binds a party to. An edge would assert a coupling the contract spends a paragraph
denying. Whether G4 wants a form for a published-format draw is a question for that
gate rather than a defect in this clause.

## 8. The service's socket

**On the operator's rulings of 2026-11-26**, the sudo rule moves from the admin
connector to the weaver-admin account, under which a standing service runs, and the
connector asks that service four things on one local socket: which verbs are granted,
run this verb, the trace from this position, and the admin log's lines (his answer 77
(f), the second shape, with two conditions this section carries). The four exchanges
are those the WeaverWeb seat filed as WeaverAgent issue 645, and this section takes or
answers each of them. **Act one's code stack builds it**: until that stack lands,
sections 0 to 3 stand as written, the connector holding the rule and dialling the trace
door, and that stack changes them to this section.

**Three things carry the name, and this section says which one at each site.** The
**weaver-admin account** is the system account `weaver-admin`. The **service** is
`weaver-admin-service`, a binary of the admin crate that systemd keeps standing from
boot under the weaver-admin account. The **root program** is `weaver-admin`, run as root
at a root shell or through the sudo rule, one fixed line per verb, which sections 1 to
6 call admin. The service is the root program's invoker: it runs the fixed lines through
sudo, so section 2's obligations on the invoker's execution context are the service's.
**The service runs without no-new-privs**, because sudo gains root by its set-user-id
bit and under no-new-privs every verb would fail. `weaver-admin-Spec` section 2 gives
the reason.

### 8.1 Admission

- **The socket** is one path, `/run/weaver-admin.sock`, made by systemd before the
  service starts. It is owned by the weaver-admin account, grouped to the admin
  connector's own group, `weaver-admincon`, which holds no member but the connector's
  account, and is mode `0660`. A process outside the group cannot connect.
- **The peer is judged at accept**, by its uid from `SO_PEERCRED`. The one uid admitted
  is the connector account's, `weaver-admincon`. Every other uid is refused, root's
  included, since the operator at a root shell runs the root program directly. A refused
  peer reads no byte, its connection is closed, and the service's own log names the uid.
- **Connections are bounded per agent and in all.** For each agent, at most one trace
  connection and one log connection stand, the newest of either replacing the older,
  which the service closes, and at most 8 control connections. The service holds at
  most 1024 connections in all, those still awaiting their opening line included. A
  control connection past 8 for its agent, or any connection past 1024, is answered
  `refused` `full` and closed.

### 8.2 The opening line, and the framing

**Every message is one JSON object on one line, ending in a newline,** externally
tagged: its one member names the message, and that member's object is the message's
body. This framing holds for the opening line and its answer, and for every line on a
control or log connection. **On a trace connection, after `open`, the framing is the
relay's alone** (section 8.5), so a line there is never read by this paragraph.
**One form is reserved** for the message of section 8.9: an ask line whose body names
`length` and `sha256`, followed by exactly `length` raw bytes. It belongs to the purpose
or version the amendment of section 8.9 adds, which a version 1 service refuses at the
opening line. On a version 1 connection no ask carries it, and an ask whose body names
`length` is refused `ask_malformed`.

**Each connection opens with one line from the connector**, at most 4096 bytes with its
newline, within 10 seconds of the connect:

```json
{"weaver_admin":{"version":1,"purpose":"control","agent":"a1"}}
```

- `version` is the contract version the connector speaks, a positive integer. This
  section is version 1.
- `purpose` is one of a closed list, `control`, `trace` and `log`, which only an
  amendment of this contract extends.
- `agent` is the agent's name, judged by the root program's own name rule
  (`weaver-admin-Spec` section 4): non-empty ASCII letters, digits, `-` and `_`, with
  the reserved suffixes and names refused. The name `admin` joins the names refused in
  act one's code stack, on the operator's ruling of 2026-11-26, since an agent of that
  name would own `weaver-admin`, the service's account.

**The service answers it with one line**, before any purpose's exchange begins, at
most 4096 bytes with its newline and within 10 seconds of the opening line's arrival.
That time holds the log's judging of section 8.6 and the relay's dial of section 8.5,
and one not done within it is refused `log_unread` or `relay_unreachable`. Either
`{"open":{"version":1}}`, and the purpose's exchange follows, or one `refused` line and
the close:

```json
{"refused":{"word":"purpose_unknown","detail":"...","purposes":["control","trace","log"]}}
```

Every refusal of the service's, on every purpose, is one of the words below. A
refusal of the opening line closes the connection. On a control connection a refusal
leaves it standing, except `ask_malformed` and `ask_outstanding`, which close it. On a
log connection `log_behind` and `ask_malformed` close it.

| Word | Where | Cause |
| --- | --- | --- |
| `opening_malformed` | opening | the line is past 4096 bytes, ends without a newline, does not parse, lacks a member, or names a version that is not a positive integer |
| `opening_late` | opening | no complete line within 10 seconds of the connect |
| `version_unknown` | opening | the version is above the service's own. The answer names `versions`, every version it speaks |
| `purpose_unknown` | opening | the purpose is not on the list of the version named. The answer names `purposes` |
| `name_refused` | opening | the name fails the root program's rule |
| `log_unread` | opening | the purpose is `log`, and the agent's `admin.log` cannot be opened or judged (section 8.6) |
| `relay_unreachable` | opening | the purpose is `trace`, and no relay of the agent stands to dial, or the service's dial of it fails (section 8.5) |
| `full` | opening | the connection is past a bound of section 8.1. Sent before the opening line is read |
| `ask_unknown` | control | the ask's member is not an ask of the version opened. The connection stands, so a newer connector reads it as "not here yet" |
| `ask_malformed` | control, log | a line past 4096 bytes, one that does not parse, an ask whose body names `length`, a principal of another shape, or any byte sent on a log connection |
| `ask_outstanding` | control | a line sent while an ask is outstanding |
| `grants_unread` | control | the eight grants are not all read within 10 seconds (section 8.3) |
| `verb_unknown` | control | the verb is not one of the eight. The connection stands |
| `not_granted` | control | the verb's grant read answers not granted (section 8.4) |
| `grant_unread` | control | the verb's grant cannot be read within 10 seconds |
| `busy` | control | another run of the agent stands, a `show` excepted. The answer names it as `run` |
| `not_started` | control | the root program could not be started, so nothing ran |
| `log_behind` | log | 1 MiB stands unsent (section 8.6). Best effort: a connector that does not read may never receive it |

**A service answers every version at or below its own as that version was written**,
so a newer connector reads `version_unknown` or `purpose_unknown` as "not here yet",
and the exchanges of an older version never change. The `refused` line, its `word` and
its `detail` are one shape on every connection: a refusal of the service's own is
always this line, and never an exit code or a part of the root program's object.

### 8.3 The grants

On a control connection, the ask `{"grants":{}}`, which names nothing beyond the agent
the opening named.

- **The service reads the grant of each of the eight verbs** (section 8.4 states the
  read) and answers `{"grants":{"verbs":[...]}}`, naming the granted verbs in the list's
  order: `show`, `validate`, `load`, `unload`, `stop`, `save-point`, `restore`,
  `force-unload`. An empty list is an honest answer: nothing is granted.
- **A read that fails is not a guess.** Where any of the eight cannot be read, or the
  eight are not read within 10 seconds of the ask, the answer is `refused`
  `grants_unread`, never a shorter list. A verb whose read exits 1 is left out as not
  granted, and exit 1 covers a rule sudo could not read as well (section 8.4). So an
  empty list says that no verb's read answered granted, and never that a read did not
  answer.
- **The answer is the connector's ceiling for Weaver Web and never the service's
  authorization.** Nothing in it is read again at a verb.

### 8.4 A verb

On a control connection, one ask names the verb and the principal:

```json
{"verb":{"verb":"load","principal":{"kind":"person","person_id":"pe-0123456789abcdef"}}}
```

- `verb` is one of the eight. Any other name is refused `verb_unknown`, and the
  connection stands.
- `principal` is `{"kind":"server"}` or `{"kind":"person","person_id":"pe-<16 lowercase
  hex>"}`, and any other shape is refused `ask_malformed`. **It is a claim for the
  record and never an authorization input**: it reaches no decision of the service's,
  and never the root program's arguments.

**The grant is read in the request.** On every verb ask, after the ask has arrived, the
service reads whether the machine grants that verb on that agent, and runs nothing
until that read answers granted. Nothing sent earlier counts, on this connection or
another, a grants answer included, so a grant removed after a grants answer is refused
at the verb. **In act one the read is `sudo -n -l` run by the weaver-admin account on
the exact fixed line** (the root program's path as the rule names it, the verb, the
agent), the read the connector makes today:

| The read | The answer |
| --- | --- |
| exit 0 | granted |
| exit 1 | `refused` `not_granted`, nothing run. Exit 1 means not granted, or a rule sudo could not read |
| any other exit, a read that does not start, or none within 10 seconds of the ask | `refused` `grant_unread`, nothing run |

A rule that asks a password is not a grant, since sudo runs with `-n`. **sudo exits 1
both where the rule does not grant the line and where it could not read a rule**, and
the service makes no decision from sudo's standard error, so the two read alike.
**The service's own log keeps sudo's standard error** for every read that did not
answer granted, here and in section 8.3, so the operator finds the cause where Weaver
Web shows a verb not granted. Both refusals are the service's own, and the root program
never runs to produce them.

**Then one run at a time per agent.** A granted verb starts unless a run of the same
agent stands, in which case it is refused `busy`, the answer naming that run's id as
`run`. Two overlaps are admitted, the two the root program admits (section 4.1). **A
`show` starts beside any run**, since the root program's `show` alone reads beside a
holder, and it holds the agent for no other ask. **A `force-unload` asked while an
`unload` of the same agent stands is started beside it.** The grant is read before this
rule, so a verb not granted is `not_granted` whatever stands. The refusal:

```json
{"refused":{"word":"busy","detail":"...","run":"0123456789abcdef"}}
```

`run` is the 16 hex digits of the run that holds the agent, the one its `started` line
named, and the `unload` where an `unload` and a `force-unload` stand.

**A run started is named, then answered.** Once the root program is running, the
service sends `{"started":{"run":"<16 hex>"}}`, the run's id drawn by the service. A
root program that cannot be started is refused `not_started`, and nothing ran. When it
ends, one final line:

```json
{"ran":{"run":"...","verb":"load","exit_code":0,"answer":{...},"stderr":"...","stderr_cut":false}}
```

- **`ran`** where it exited 0 or 1 with one JSON object on standard output, at most 64
  KiB, in UTF-8. `answer` is that object, carried untouched: a `lifecycle-answer` at 0
  or a `lifecycle-refusal` at 1, so a refusal of the root program's travels inside the
  object at exit 1, never as a `refused` line. `stderr` is its standard error, at most
  the first 16 KiB with any byte that is not UTF-8 replaced, `stderr_cut` true where it
  was longer. Neither is parsed by the service.
- **`{"fault":{"run":"...","verb":"...","detail":"..."}}`** where it ended any other
  way: another exit, a signal, no object, more than one value, past 64 KiB, or not
  UTF-8. Whether it acted is read with the next `show`.

**A run goes on when its connection closes.** A close before the grant's read answers
starts nothing. Once the read answers granted, the service goes on to start the root
program whether or not the connection still stands. From the moment the root program
is started, the run goes on to its end and is kept, whether or not `started` reached
the connector, and a `run` ask on a new connection names it. **The service's own end ends no run it
started**, as an invocation finishes when its caller disappears (section 3). That is a
requirement on the service's install, which act one's code stack meets: a stop or
restart of the service ends the service's own process and signals no root program it
started, whose sudo and root program stand in the service's containment until a load
moves into the agent's scope (section 2). An install that does not meet it leaves a
verb killed mid-command, outside the envelope (section 4.1). A new control connection
asks `{"run":{}}` and is answered:

```json
{"run":{"standing":[{"run":"...","verb":"unload","started_ms":0}],"ended":{"ran":{...}}}}
```

`standing` lists the agent's runs standing, a `show` excepted. `ended` is the final
line, `ran` or `fault`, of the last run that ended, a `show` excepted, or `null` where
none has ended since the service started. **A `show` is never listed in `standing` and
never kept as `ended`**, so a `show` that ends later never hides the outcome of the
verb a connector asked before a drop. A `show` whose connection closed is not named,
and the next `show` reads the state. A service that restarted knows no run of before: the root program's own
invocation lock then refuses a second verb inside its object, and the outcome is in
`admin.log` and the next `show`.

**The cause is the service's account.** The root program reads sudo's `SUDO_UID`, so
the cause `admin.log` and the trace record is the weaver-admin account's uid (section
2). **The principal is recorded in the service's own log**, one line per run naming the
run's id, the agent, the verb, the principal and the outcome, and in Weaver Web's own
audit. It is never in `admin.log` or the trace.

**One ask at a time on a connection.** The connector sends its next ask once the
previous one's final line has arrived. A line sent while an ask is outstanding is
refused `ask_outstanding` and the connection is closed, a run already started going on.
An unknown ask is refused `ask_unknown` and the connection stands. A line past 4096
bytes, or one that does not parse, is refused `ask_malformed` and closes it. A connection with no ask
outstanding may stand idle without limit.

### 8.5 The trace

On a trace connection, after `open`, the bytes each way are the relay's wire of
`weaver-types-Spec` section 3.1, unchanged: the connector's one `TraceRequest`, then the
relay's header, the trace's own lines byte for byte, its heartbeats while idle, and
`truncated` before the end. **The service is the relay's one declared reader in act
one**: the boundary file's `trace-reader` names the weaver-admin account, and the relay
judges the service's uid as it judges any reader's.

- **The service dials the agent's relay before it answers the opening line**, and sends
  `open` only once that dial has connected. It then copies the bytes each way until
  either side closes, and closes the other.
- **Where no relay stands to dial** (a pipe or socket sink, or a relay that has died), or
  the dial fails, the opening is refused `relay_unreachable` in place of `open`. The
  connector reads it as a closed door, as it reads a failed dial of the relay today.
- **After `open`, a close before any header byte is the relay's refusal alone**, its
  verdict on the position the request named.
- The newest connection replaces the old at the relay, as section 2 states, so the
  service closes the older splice when the relay ends it.
- The request's bound is the relay's: 4096 bytes with its newline, within 5 seconds of
  the relay's accept. The service's dial comes before `open`, so the connector sends
  the request at once after `open`.

### 8.6 The log

On a log connection, the service sends each line appended to the agent's `admin.log`
from the moment it answered `open`, with no history before it:

```json
{"log":{"line":{...},"through_service":true}}
```

- **`line`** is the appended line's object. A line that is not one JSON object is sent
  as `{"log":{"text":"...","through_service":false}}`, any byte that is not UTF-8
  replaced. A line past 8 KiB is sent as `{"log":{"oversized":<its length>}}` and not
  carried.
- **`through_service` is judged from the line alone**: true where an invocation's line
  names the weaver-admin account's uid as its caller, or the relay's line names it as
  its peer, and false otherwise. In act one the service runs the root program only on a
  control connection's ask and is the relay's one declared reader, so that uid names
  exactly the acts that came through this socket. An operator's own act carries the uid sudo
  reported, the operator's, or 0 where no sudo set one, and never the weaver-admin
  account's, so it is false. An act that gives the service a run of its own amends this rule.
- **A truncation or a replacement of the file** is sent as
  `{"log":{"discontinuity":"truncated"}}` or `"replaced"`, and the service then reads
  the file from its start.
- **A line appended reaches the connection within 1 second.** Where 1 MiB stands unsent
  to a connector that does not read, the service closes the connection, sending
  `refused` `log_behind` first where the connection takes it. That line is best effort,
  and a connector that does not read may never receive it.
- **Any close of a log connection is a discontinuity for the connector**, with or
  without `log_behind` before it, which it marks as it marks a replacement today.
  Resending toward Weaver Web stays the connector's.
- **The log is judged before `open`**: it is opened and judged by every check
  `weaver-admin-Spec` section 8 makes of a log, a non-blocking open of a regular file
  among them. Which groups let the weaver-admin account pass the territory and read the
  log is act one's code stack's. One that does not open or judge is refused `log_unread`
  in place of `open`.

### 8.7 Limits

| What | Number | At the limit |
| --- | --- | --- |
| Control connections standing for one agent | 8 | `refused` `full`, closed |
| Trace or log connections standing for one agent | 1 of each | the newest replaces the older, which is closed |
| Connections standing at the service | 1024 | `refused` `full`, closed |
| The opening line | 4096 bytes with its newline | `refused` `opening_malformed`, closed |
| The opening's arrival | 10 seconds from the connect | `refused` `opening_late`, closed |
| The service's answer to the opening | 4096 bytes with its newline, within 10 seconds of the opening line, the log's judging and the relay's dial included | a judging or dial not done in time is `refused` `log_unread` or `relay_unreachable`. A connector bounding its read here reads past it as a fault |
| A connector's line on a control connection | 4096 bytes with its newline | `refused` `ask_malformed`, closed |
| Asks outstanding on a connection | 1 | `refused` `ask_outstanding`, closed |
| The eight grants read | 10 seconds from the ask | `refused` `grants_unread` |
| One verb's grant read | 10 seconds from the ask | `refused` `grant_unread` |
| Runs standing of one agent | 1, with any `show` beside it, or an `unload` and a `force-unload` beside it | `refused` `busy`, naming the run |
| The root program's object | 64 KiB | `fault` |
| The root program's standard error kept | 16 KiB | cut, `stderr_cut` true |
| A line the service sends on a control connection | 256 KiB: a 64 KiB object, 16 KiB of standard error at most 96 KiB escaped, and the members | a connector bounding its read here reads past it as a fault |
| A run's length | none at the service. The root program's own bounds hold: 900 seconds on a load's start by default (section 3) | the connector's own bound, then `run` |
| Runs remembered per agent | those standing, and the last ended, until the service restarts | `ended` is `null` |
| The trace request | 4096 bytes within 5 seconds, the relay's (`weaver-types-Spec` section 3.1) | the relay's close before the header |
| A log line carried | 8 KiB | the `oversized` marker |
| A log line's delay | 1 second from its append | none: the service's promise |
| A log connection's unsent bytes | 1 MiB | closed, `refused` `log_behind` sent first where the connection takes it |
| The reserved text of section 8.9 | 256 KiB, with its sha256 | reserved. A version 1 service refuses its opening `version_unknown` or `purpose_unknown`, and an ask naming `length` on a version 1 connection `ask_malformed` |

The connector's own bounds (its 960 seconds on a verb, its 1,080 seconds of grace at a
stop, its 16 asks queued, its record bound of 2 MiB on the trace) are the connector's and
bind nothing here.

### 8.8 The state table

Two machines: machine K, one per connection, and machine A, one per agent at the
service. The connector causes the C events, the service the V events, and the machine
the M events.

| State | What holds |
| --- | --- |
| K0 | Accepted. The peer's uid is read |
| K1 | Admitted, waiting for the opening line |
| K2 | Control, no ask outstanding |
| K3 | Control, a grants ask outstanding |
| K4 | Control, a verb ask outstanding, its grant being read |
| K5 | Control, a verb ask outstanding, its run started |
| K6 | Control, a run ask outstanding |
| K7 | Trace, after `open`: the relay's wire |
| K8 | Log, after `open`: lines as appended |
| K9 | Closed |

| State | Event | Next, and what is sent |
| --- | --- | --- |
| K0 | V: the peer's uid is the connector's | K1 |
| K0 | V: any other uid | K9, nothing sent, the service's log names the uid |
| K0 | V: a bound of section 8.1 is met | K9, `refused` `full` |
| K1 | C: a control or log opening line that judges | K2 or K8 by its purpose, `open` sent |
| K1 | C: a trace opening line that judges, and the service's dial of the agent's relay connects | K7, `open` sent |
| K1 | C: a trace opening line that judges, where no relay stands or the service's dial of it fails | K9, `refused` `relay_unreachable` |
| K1 | C: an unknown version or purpose, a refused name, or an unread log | K9, `refused` with its word |
| K1 | C: a line past 4096 bytes, one that does not parse, or a version that is not a positive integer | K9, `refused` `opening_malformed` |
| K1 | V: a newer trace or log connection of the agent is opened | K9, closed |
| K1 | V: 10 seconds pass with no line | K9, `refused` `opening_late` |
| K2 | C: `grants` | K3 |
| K2 | C: `verb` naming one of the eight, with a principal of its shape | K4. The grant is read now |
| K2 | C: `verb` naming another verb | K2, `refused` `verb_unknown` |
| K2 | C: `run` | K6 |
| K2 | C: an unknown ask | K2, `refused` `ask_unknown` |
| K2 | C: a line that does not parse, past 4096 bytes, naming `length`, or a principal of another shape | K9, `refused` `ask_malformed` |
| K2 | C: close | K9 |
| K3 | V: the eight grants read | K2, `grants` naming the granted verbs |
| K3 | V: a read fails, or 10 seconds pass | K2, `refused` `grants_unread` |
| K3 | C: close | K9 |
| K4 | V: granted, and machine A admits the run | K5, `started` with the run id |
| K4 | V: not granted | K2, `refused` `not_granted` |
| K4 | V: the grant cannot be read within 10 seconds | K2, `refused` `grant_unread` |
| K4 | V: granted, and machine A refuses the run | K2, `refused` `busy` naming the run that holds the agent |
| K4 | V: granted, and the root program does not start | K2, `refused` `not_started` |
| K4 | C: close, before the read answers | K9. Nothing starts |
| K4 | C: close, after the read answers granted | K9. The start goes on as if the connection stood, and a run started is kept in machine A |
| K5 | V: the root program ends with exit 0 or 1 and one object | K2, `ran` |
| K5 | V: it ends any other way | K2, `fault` |
| K5 | C: close | K9. The run goes on in machine A |
| K6 | V: machine A read | K2, `run` |
| K6 | C: close | K9 |
| K2 to K6 | C: a line while an ask is outstanding | K9, `refused` `ask_outstanding`. A run started goes on |
| K7 | the relay: a close before any header byte | K9, the connector's side closed. It is the relay's refusal of the position |
| K7 | C or the relay: either side closes | K9, the other side closed |
| K7 | V: a newer trace connection of the agent is opened | K9, closed |
| K8 | V: a line appended | K8, `log` with `line` or `text`, or the `oversized` marker past 8 KiB |
| K8 | V: the file truncated or replaced | K8, the `discontinuity` marker, the file read from its start |
| K8 | C: any byte | K9, `refused` `ask_malformed` |
| K8 | V: a newer log connection of the agent is opened | K9, closed |
| K8 | V: 1 MiB stands unsent | K9, `refused` `log_behind` where the connection takes it |
| K8 | C: close | K9 |
| any | M: the sudo rule is edited | no change. The next verb ask reads the rule as it stands then |

| State | What holds |
| --- | --- |
| A0 | No run of the agent stands. The last run that ended, a `show` excepted, is kept, or none since the service started |
| A1 | One run stands |
| A2 | An `unload` and a `force-unload` beside it stand |

| State | Event | Next |
| --- | --- | --- |
| any | a `show` ask, granted | no change. The `show` starts beside whatever stands, and is not a run this machine holds |
| any | a `show` ends | no change. Its final line goes to its own connection alone, never kept as the last ended |
| A0 | a verb ask other than `show`, granted | A1, holding that run |
| A1 | a `force-unload` ask, granted, while the run A1 holds is an `unload` | A2, holding both |
| A1 | any other verb ask but `show`, granted | A1, that ask refused `busy` |
| A1 | the run A1 holds ends | A0, its final line kept as the last ended |
| A2 | another verb ask but `show`, granted | A2, that ask refused `busy` |
| A2 | the `unload` or the `force-unload` ends | A1, holding the one still standing, the ended one's final line kept as the last ended |
| any | the asking connection closes | no change |
| any | the service restarts | A0 with no run kept. A root program it started runs on, under the root program's own lock, where the install meets section 8.4's requirement |

**The invariants.**

- **S1.** A root program starts only from K4, after a read of that verb's grant on that
  agent, made in that request after its ask arrived, answering granted. No path from K2
  reaches K5 without it.
- **S2.** Nothing enters S1's read from a grants answer, an earlier ask, or another
  connection.
- **S3.** The principal enters no decision: not S1's read, not machine A, and not the
  root program's arguments.
- **S4.** A run's end does not depend on its connection. A close before the grant's
  read answers starts nothing. From the moment the root program is started, the run
  goes on to its end and is kept, whether or not `started` reached the connector.
- **S5.** At most one run of an agent stands, except a `show`, which starts beside any
  run, and the `force-unload` beside an `unload` of A2.
- **S6.** Every ask gets exactly one final line, or the connection closes. A verb's
  `started` comes before its final line.
- **S7.** After `open` on a trace connection, the bytes each way are the relay's,
  unchanged. `open` is sent only once the relay's dial has connected, so a close there
  before any header byte is the relay's refusal of the position alone.
- **S8.** A refusal of the service's own is a `refused` line, never inside the root
  program's object and never an exit code. A refusal of the root program's is inside its
  object, at exit 1, carried untouched.

### 8.9 Reserved: the loop's text, for loop install

**Reserved by the operator's answer of 2026-11-26 (77 (f)), and brought by the loop's
third act, not by act one.** A fifth message will carry the text of an agent's loop
wire file, for loop install: up to 256 KiB, the bound the root program's judge of the
loop folder holds for that file today, with its sha256, framed by the reserved form of
section 8.2, a header line naming `length` and the sha256, then exactly that many bytes.
It arrives by an amendment of this section that raises the version and names its
purpose or ask, so the four exchanges above do not change and a version 1 service
refuses it `version_unknown` or `purpose_unknown`. **The text goes through the service
to a fixed root program under sudo, which checks the input and puts the file's digest
in the trace**, on the operator's ruling of 2026-11-26. This section says that much and
no more.

### 8.10 What the socket does not carry, and the fake

**Not carried in act one:** the pairing, the gate, and an edit of an agent's
configuration. A new verb or a new message arrives only by an amendment of this
contract.

**A fake service can be written from this section alone**, for the connector's tests
(issue 645, section 7). It listens at a temporary path, admits its test's uid in place
of the connector account's, and can:

- answer the grants with a configured list, refuse them `grants_unread`, or send nothing
  past 10 seconds.
- answer a verb with `started` and a configured exit code and object, `show` from a
  state it holds (`InTransition` for a set count), refuse it with any word of section
  8.4, end it as a `fault`, or delay its final line.
- append lines to a trace file before and after it takes its state, and record each
  verb it ran with its principal.
- drop a control connection while a verb runs, and answer a later `run` with that run
  standing or ended.
- on a trace connection, refuse the opening `relay_unreachable` as where no relay
  stands, and after `open` do what a relay does: verify the position by its digest,
  refuse by closing before the header, name `length`, send heartbeats while idle, send
  `truncated` then end, and let the newest connection replace the old.
- on a log connection, send scripted lines with their flag, markers, and `log_behind`,
  and close.

It reaches no agent and no privileged program.

## 9. What this document changes elsewhere

- `WeaverTools-Document-Format.md`: a party-edge category for an external principal,
  this document and `weaver-gate-world-contract` being the two instances. Owed to the
  Format's next revision.
- `weaver-admin-PRD` sections 8 and 10: the surface cites this contract and the
  operator-to-service cell closes. Landed in the same act.
- `weaver-types-PRD` section 2.3: the request and answer pair, pending the naming
  ruling, per section 7.
- `weaver-types-PRD` section 2.2, owed by the recut of 2026-08-05 and landing in the
  same act. That section rests its scoped claim about `peer-identity` and
  `authorization-predicate` on this contract being one of the pair's two consumers,
  and this contract stopped drawing them. The pair's consumers are now the gate's
  client boundary and the coordination seam, whose credential check the inversion
  gave a real subject, so the claim is re-aimed rather than weakened.
- The durable-record cut this ruling scopes landed on 2026-08-01 as its own batch:
  the program-side checksum, the manifest, the leave-time comparison, and
  record-based session resume left the corpus, `weaver-types-PRD` section 2.1
  gained `trace-sink` on this contract's demand, and what the batch left behind is
  the enter cell `weaver-admin-PRD` section 10 holds.
- The lifecycle act, 2026-10-09: `weaver-admin-Spec` section 3 holds the lifecycle
  state table, and section 4.1 here reads it from the caller's side, landing in one act
  with the admin-harness, harness-gate and gate-world contracts.
- The agent leaving systemd, 2026-10-03 (#50), landing in one act.
  `weaver-admin-systemd-contract` retires. `weaver-admin-PRD` sections 1, 2, 4, 5, 6, 7,
  8 and 10 and `weaver-admin-Spec` sections 2, 3, 6, 8, 9, 10 and 11 move to the start
  step, the run lock, the sudo rule and the trace relay. `weaver-types-Spec` section 3.1
  carries the trace door's shapes and the cause, and `weaver-harness-Spec` section 2
  the worker's ownership of its organs. WeaverWeb's admin-con maps its verbs to the
  granted lines and reads the trace door, toddwbucy/WeaverWeb#12, and binds its agent's
  lifetime to its own, toddwbucy/WeaverWeb#15.
- The agent's own scope, 2026-11-09 (#166), on the operator's rulings of 2026-11-07 and
  2026-11-09. `weaver-admin-Spec` sections 3, 6, 6.1 and 11, `weaver-admin-PRD` sections
  7 and 10, and sections 2, 3 and 5 here move the placement into admin's start step and
  the agent's lifetime to its own scope, landing in one act, with the measurement in
  `docs/project/admin-root-scope-measurement-2026-11-09.md`. The binding of
  toddwbucy/WeaverWeb#15 above is reversed. WeaverWeb's half (its Spec section 8's
  install paragraph, the execution-context obligation of section 2, item 1 of
  toddwbucy/WeaverWeb#46 and its PRD's Run row) is filed on toddwbucy/WeaverWeb, one
  issue per interface question, once this act merges.
- The box's two connector accounts, on the operator's rulings of 2026-11-10. Sections 0
  to 2 here, `weaver-gate-world-contract` sections 1, 2 and 8, `weaver-admin-PRD`
  section 7, `weaver-admin-Spec` sections 2, 4 and 9, `weaver-gate-Spec` section 3,
  `weaver-types-Spec` section 3.1 and `weaver-harness-Spec` section 6.1 replace the
  per-agent connector account with `weaver-admincon` and `weaver-gatecon`, one of each
  per box, landing in one act. WeaverWeb's half is filed on toddwbucy/WeaverWeb, one
  issue per interface question, once this act merges.
- The service's socket, on the operator's rulings of 2026-11-26 and his answer 77 (f).
  Section 8 here, and the pointer in `weaver-admin-Spec` section 2, land first, in a
  stack of documents alone, with the four exchanges of WeaverAgent issue 645. Act one's
  code stack builds the service and its socket, and moves sections 0 to 3 here,
  `weaver-admin-Spec` sections 1 (the third binary, with the manifest test
  `two_binaries_and_no_library_surface`), 2, 4 (the name `admin` refused), 8 and 9, and
  `weaver-types-Spec` section 3.1's `trace-reader` to it. WeaverWeb's half, the connector asking the socket in place of
  sudo and the relay, is the other seat's, against section 8.
