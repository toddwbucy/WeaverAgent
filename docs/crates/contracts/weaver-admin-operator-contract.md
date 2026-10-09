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
**Landing PR:** #72

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
below and by the party-edge category the Format is owed per section 8, never by a
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
rule the operator installs, this agent's own connector acting for that person under its
own service user, never the operator's account, on the operator's ruling of 2026-10-03
on #50: human and machine accounts stay separate, the connector holds only the fixed
lines, so it can write none of the operator's files, and WeaverWeb's audit names the
person while the box's record names the service uid sudo reports. The graph
carries no node for a principal outside the program, so the party is named in prose and
the missing category is registered rather than improvised.

## 1. The boundary

**This contract binds three crossings: the command lines, the sink, and the trace
door.** **The command lines** are `weaver-admin <verb> <agent>` run as root, at a root
shell or through a sudo rule the operator installs as the box's own boundary, which
names this agent's connector user and the exact command lines for this agent and allows
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
and how it is reached are `weaver-admin-Spec` section 6's. None is network ingress, and
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
`restore` and `force-unload`, the role split of #50 mapped onto command lines. Nothing else crosses in:
no argument the caller chooses, no standard input, which the program never reads, and no
claim of who asked. **The cause the record carries is the uid sudo reports**, and which
person asked is WeaverWeb's record and never the agent's. **The rule opens no login
session for these lines**, on the operator's confirmation of 2026-10-03 on
toddwbucy/WeaverWeb#15: sudo's `pam_session` is off for the connector's user
(`Defaults:<admin-con user> !pam_session`), because a session can move the invocation,
and with it the agent it starts, into a containment other than the invoker's, which
would silently break the lifetime binding of section 3. A requirement on the boundary is
declared and checked: the invoker verifies after a load that every constituent `show`
names sits in its own containment. No prompt, turn, task, or run
crosses, per apex section 6.

**At the trace door, one request line**: a byte offset on a record boundary and the
digest of the record before it, the shape `TraceRequest` of `weaver-types-Spec` section
3.1, from the one `trace-reader` the agent's boundary file declares, the agent's own
admin-con. Every other caller is refused: one outside the agent's access group by the
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
territory**, `/var/lib/weaver-agent/weaver-<agent>/agent.toml`, root's, on the operator's
ruling of 2026-10-07 on #1: the operator edits it with `sudoedit`, the connector reads
it through the access group and never writes it, and admin reads it through the
territory's descriptor, per `weaver-admin-Spec` section 9.

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
nothing resident**, and `show` prints it only once the run lock is free: while any
constituent still holds it and no worker answers, a run that never entered or one
ending, `show` prints `InTransition` with the constituents named (`weaver-admin-Spec`
section 5). Standard error carries
human-readable diagnostics no caller parses. A
`load` answers once the agent is up or refused, within admin's own bound of 900 seconds
by default on the agent's start, which a caller's bound must exceed, after any
publication of recovered files from an unclosed run, which is bounded by their size and
not by time; a load whose agent does not start inside that bound rolls back before it
answers, which takes at most 195 seconds more. **An invocation finishes even when its
caller disappears**, its outcome recorded in the agent's `admin.log`, so a caller that
gives up reads the outcome from the next `show`.

**The agent's lifetime is bound to its admin-con**, on the operator's ruling of
2026-10-03 on #72, enforced by the containment the invoker runs in, and the program
depends on no supervisor for it. Each agent has its own admin-con, and stopping that
admin-con stops its agent and only its agent: the management plane going down may mean
it was taken over, so the agent fails closed with it. A dropped invocation is not a
stopped admin-con, and still runs to completion, per the bound above. Three consequences
are the invoker's to build against: an orderly stop of admin-con unloads its agent
first, a kill is an unclean stop whose next load resets to the latest save point, and
the invoker's resource limits contain the agent. The cross-repository half is
toddwbucy/WeaverWeb#15.

**At the trace door, the record as a read-only stream**: the relay writes a header line
naming the file's identity (device, inode, and birth time where the filesystem reports
it), then the trace's own lines from
the verified position exactly as written, following new lines with a heartbeat while
idle, every added line a `TraceLine` of `weaver-types-Spec` section 3.1. It serves
the file the loaded run opened, so a rotation of the sink's path changes
nothing it reads and reaches the reader as a new identity in the next run's header, ends
the stream on a truncation rather than smoothing it over, and refuses a position that
does not verify before a byte is sent. A reader of the record this way reads the file
through no grant of its own, so the territory's layout gives it nothing.

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
| S0d, down, a run ended without closing | as S0, the load recording the reset | publishes the save points the run left as recovered, then `Unloaded`, or `SavePointNotTaken` naming `published` where that fails; the next load records `NoCleanUnload` | publishes them the same way, then `Unloaded`, or refuses as `unload` does; the marker is left as it stands, a force on an agent already down having ended nothing | `OutOfOrder` | `RestoreNamed` | `Unloaded` |
| S1, loading | `InvocationInFlight`; the lock free, `AgentRunning` while a process holds the run, else as S0d | `InvocationInFlight`; the lock free, ends whatever holds the run, else as S0d | waits for the load's publication of recovered files, bounded by their size, then at most the load's bound, then, where the agent did not start inside it, the load's rollback (at most 195 seconds), then answers as the state it finds; the lock free, ends whatever holds the run as `unload` does | `InvocationInFlight`; the lock free, `OutOfOrder` unless the agent had started serving | `InvocationInFlight`; the lock free, `RestoreNamed` | `InTransition`; the lock free, `InTransition` with the run's processes named while they stand, then `Unloaded` |
| S2, serving at rest | `AgentRunning` | the graceful unload, below | the forced unload, below | `SavePointTaken` | `RestoreNamed` | `Idle` |
| S3, serving, a turn in flight | `AgentRunning` | the graceful unload, after the turn | the forced unload, the turn cancelled | `ActivityNotAtRest` | `RestoreNamed` | `Active` |
| S4 to S6, a graceful unload in progress | `InvocationInFlight`; the lock free, `AgentRunning` | `InvocationInFlight`; the lock free, adopts the leave and is answered when it completes, or `OutOfOrder` where the leave was already forced | joins, below; the lock free, takes over and joins the same way | `InvocationInFlight`; the lock free, `OutOfOrder` | `InvocationInFlight`; the lock free, `RestoreNamed` | `InTransition` |
| S7, the unload finishing | `InvocationInFlight`; the lock free, `AgentRunning` | `InvocationInFlight`; the lock free, answered as the leave stands where it reached the agent before the agent stopped listening, or, after that, waits for the run to end and finds it ended | joins as at S4, or, after the agent stopped listening, waits for the run to end, escalating past the leave's 150 seconds and 45 more | `InvocationInFlight`; the lock free, `OutOfOrder` | `InvocationInFlight`; the lock free, `RestoreNamed` | `InTransition`, with the run's processes named once the agent stopped listening |
| S8, an unload concluding | `InvocationInFlight`; the lock free, `AgentRunning` while a process still holds the run, else as S0d | `InvocationInFlight`; the lock free, waits for the run to end, then as S0d | waits, then as S0 or S0d; the lock free, waits for the run to end, escalating past 150 seconds and 45 more, then as S0d | `InvocationInFlight`; the lock free, `OutOfOrder` while a process still holds the run, else as S0d | `InvocationInFlight`; the lock free, `RestoreNamed` | `InTransition` with the run's processes named, then `Unloaded` once they are gone |
| S9, an unload stopped at its save point | `AgentRunning` | retried: the save point, then down | the forced unload | `SavePointTaken` | `RestoreNamed` | `Idle`, the gate lowered |
| S10, a force in progress | `InvocationInFlight`; the lock free, `AgentRunning` | `InvocationInFlight`; the lock free, `OutOfOrder` | joins; the lock free, takes over and joins the same way | `InvocationInFlight`; the lock free, `OutOfOrder` | `InvocationInFlight`; the lock free, `RestoreNamed` | `InTransition` |
| S11, a silent worker being ended | `InvocationInFlight`; with the lock free while the processes are being ended, `AgentRunning` | `InvocationInFlight`; the lock free, waits for the run to end | waits, then as S0 or S0d | `InvocationInFlight`; the lock free, `OutOfOrder` | `InvocationInFlight`; the lock free while the processes are being ended, `RestoreNamed` | `InTransition`, or `Unanswered` while a wedged agent still accepts and does not answer, then `InTransition` with the run's processes named once it no longer listens, then `Unloaded` once they are gone |

**"The lock free" is what admin observes**: no other command holds the agent's invocation
lock, whether its caller was killed or an escalation is ending the run's processes
without it (`weaver-admin-Spec` section 3); each split cell is keyed on that, not on
whether a caller lives.

`restore` names the save point the next load restores in every state that answers it,
the live restore being A5's. `stop` and `validate` take the invocation lock as every
line but `show` does, so each refuses `InvocationInFlight` wherever the table shows an
invocation holding it (S1, S4 to S8, S10, S11); a `stop` in S9 answers `AtRest`. The
lock is taken before the state is read, so a line that meets a held lock refuses
`InvocationInFlight` whatever the state behind it. `show` alone reads beside a holder, and
what it prints is one ladder (`weaver-admin-Spec` section 5): `InTransition` at once
where another command holds the agent; otherwise the agent's own word where it answers
(`Idle` or `Active`, or `InTransition` while an unload is in progress); `Unanswered`
where it accepts and does not answer in time, a worker wedged; `InTransition` with the
run's processes named where no agent listens and a process still holds the run, a run
that never started or one ending; and `Unloaded` only once nothing holds the run.

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
`save-point` and then an `unload`, or a `force-unload` (S9), while one a declared bound
or a joining force turned forced comes down without it, prints `Unloaded` with the
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
waits and the publication after the answer come on top, as before.

**`force-unload` can be issued in any state, and it always ends the run** (I3).
- **Alone** (S2, S3, S9, and wherever it finds the lock free): the gate closes at once
  and every request it holds is recorded refused and its connection closed; the turn in
  flight is cancelled and recorded as a stop for the unload; no wind-down runs; the
  leave save point is taken as at any unload. It prints `Unloaded`. Where the save point
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
  never on the agent's answer alone, so `Unloaded` means no constituent still runs. From
  S7 on, the leave's outcome being fixed, the join adds nothing to it and concludes the same way.
- **Behind any other holder of the lock** (a load in S1, a `show`, a `save-point`, an
  unload concluding in S8): it retries the join and the lock in turn, and acts alone
  once it holds the lock. It first waits the holder's own bound and takes over none.
  Behind a load it waits for the load to publish the recovered files it found from an
  unclosed run, which is bounded by their size and not by time, then at most that
  load's bound (900 seconds unless the agent's root names another), which bounds the
  enter alone, the load concluding inside it; where the agent does not start inside it,
  the force also waits the load's rollback, a forced leave of at most 150 seconds and
  the escalation's 45. Behind a holder that is
  publishing (a `save-point`'s publication, or another caller's conclusion) it waits for
  that publication's copy, which is bounded by size and not by time, since a publication
  interrupted midway is worse than one waited for. The escalation applies to an
  unanswered join, never to these waits. So, from the moment it can act, a force ends
  the run within 150 seconds plus the escalation's 45, after the load's publication, its
  bound and, where the agent did not start, its rollback, where a load is in flight (I3). That bound ends before the publication: copying the leave's
  save point, and any recovered file the room holds, is bounded by their size and the
  copy's speed and not by time (`weaver-admin-Spec` section 3), so the command prints
  once that copy is done.
- **On an orphaned force** (a `force-unload` whose invocation was killed in S10 before
  the agent wrote its `unload`): a later force takes the lock alone and joins the forced
  leave, receives its outcome, and concludes as every answered caller does.
- **On an orphaned unload** (an `unload` whose invocation was killed in S4 to S6): the
  lock is free and the leave still pending, so the force takes the lock alone and its
  leave joins the pending one, turning it forced, the record naming its cause as
  `forced_by`; the force concludes as every answered caller does. A second `unload` in the same case adopts the leave instead,
  keeping it graceful: it waits for the drain and the wind-down, then concludes as every
  answered caller does, and the record names it as `adopted_by` (the operator's
  ruling of 2026-10-09).
- **On a worker that answers nothing** (S11): a join unanswered within 150 seconds ends
  the run's processes without the lock (I3), and only then, holding the invocation lock
  as every marker write does, writes the marker `Forced`, leaving it as it stands where
  it is already closed for the run. The `unload` holding the lock is answered no `Left`,
  so it concludes nothing and releases the lock, and the marker stands `Forced`, the
  escalator's. The force prints `Unloaded`, or `LockHolderUnknown` or `WorkerWouldNotExit`
  where the escalation cannot end the run, per section 5.

**A caller whose invocation is killed during an unload** (S4 to S8) leaves the next
caller the agent answers to conclude: a force or a second `unload` as above while the
leave can still change, S4 to S6, or from S7 on a later `force-unload` or `unload`, which
the agent answers with the leave as it stood, adding nothing to the record, where it
reached the agent before the agent stopped listening, the answer coming once the agent
has written its record; one after that waits for the run to end and finds it ended,
publishing what the run left as recovered, as `weaver-admin-Spec` section 3's table for
the seal gives, a `force-unload` waiting at most 150 seconds and then the escalation's
45. With no
answered caller the agent finishes the leave unanswered: the save point stays in the member's room for the next verb
to publish, and the marker stays open, so the next load records `NoCleanUnload` although
the state was kept, a conservative label with nothing lost (`weaver-admin-Spec` section
3, the conclusion, and an invocation killed during S4 to S8).

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
`published` too, the run still open. `force-unload` takes the leave save point as
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

## 8. What this document changes elsewhere

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
