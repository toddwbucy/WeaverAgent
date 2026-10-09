# weaver-admin-harness-contract

**Status:** MERGED. In `main` and the source of truth. Written with
`weaver-admin-PRD` as one act, per apex section 10, and the two moved together on the
human's ruling of 2026-07-31.

**Date filed:** 2026-07-29
**Document ID:** `weaver-admin-harness-contract`
**Parent:** `weaver-agent-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #72

---

## 0. What this document is

The agreement over the coordination seam: what crosses it, what each crossing means,
what each party may rely on, and how it fails. It is read alongside `weaver-admin-PRD`
and `weaver-harness-PRD`, and none of the three is complete without the other two.

**The seam has two initiators at the channel and there is one document for it.** Either
party may open an exchange by the channel's mechanics, and the two-initiator channel is
what makes admin an organ: an organ owns a domain and holds a two-initiator channel with
the harness, both properties and not either, and admin owns the lifecycle domain. The
property is the channel's rather than the exchange census's, the same reading the
half-chartered organ seams take, so the census standing at six exchanges since the join
exchange of the lifecycle act (2026-10-09), all admin's since the fault-carrier ruling
of 2026-08-01 rerouted the fault to the stream, retires no half of what makes admin an
organ. The invariant is authored in the apex and this document is downstream of it.

**Two layers meet in this document and the boundary between them is a draw.** Sections 1
and 2 draw `weaver-organ-channel`, which states the channel mechanics once for every
organ the harness holds a two-initiator channel with, and keep only what is this seam's
own. Sections 3 through 7 are admin's instance, which is the exchange list and its
rules. The layering is the point: the channel does not know what a load directive is, in
the way that IP does not know what a name lookup is. The lift that document's section 0
records was anticipated here and landed on 2026-07-31.

It carries no representation. The types it names have a definition site and no field
list here, the ordering it fixes is stated as a rule rather than as a state machine,
and how any of it is encoded is the Spec's.

```graph
node: weaver-admin-harness-contract
kind: document

edge: party
from: weaver-admin-harness-contract
to: weaver-admin

edge: party
from: weaver-admin-harness-contract
to: weaver-harness
```

**The seam edge is declared once, by the organ rather than by the harness.** Under a
single-initiator reading the declaring crate was the one that asks, and both ask now.
The rule that replaces it is that the organ declares, because the harness is the hub
every organ holds its two-initiator channel with and a hub that declared its own edges
would carry the whole seam graph in one crate. Here the organ is `weaver-admin`. This
document names its parties and does not restate that edge.

## 1. The channel has two initiators

`weaver-organ-channel` section 1 states the message layer: two initiators, the exchange
as the unit, identity by opening party and ordinal, the minimal exchange, concurrent
exchanges, the non-guarantees, and delivery. Nothing in that layer is this seam's own
and nothing of it is restated here.

One fact of this seam lands at that layer. Either party may open an exchange on this
channel, the census of chartered exchanges is section 3's, and every one of them is
admin's today, per the fault-carrier ruling of 2026-08-01, and the census is six since
the join exchange of the lifecycle act (2026-10-09).

## 2. The channel

`weaver-organ-channel` section 2 states the process-boundary layer for the organ
channels, and this seam draws it in part since the inversion ruling of 2026-08-05.
What lands here unchanged: boundary preservation as a property of the channel, the
non-inheritance split, and closure never read as an answer. What does not: the
unaddressable connected pair, authentication by possession, the holder in transit, and
the channel's life bound to the far process, each departed from below with the
departure stated as this seam's own. The organ channels the harness creates keep
the drawn shape whole, and this seam is the one that left it.

**The harness creates the channel, per the inversion ruling of 2026-08-05, and the
creating party is not the initiating party.** Any socket connecting to the harness
is an internal connection and lives inside the agent's sandbox, so the harness
binds the coordination socket and listens as its first act, before any directive
can arrive, and admin dials in. The earlier form had admin binding before the worker
started because admin was the only party that existed then, and the inversion
retires the premise: nothing needs to exist before the worker, because the worker
brings its own end.

**How each connection arrives is the dial, one per verb.** Admin is per-invocation,
per `weaver-admin-PRD` sections 1 and 7, so each verb's invocation connects to the
socket the worker holds, is served, and closes with the verb. The bind is the
worker's first act and the dial may arrive before it, so the dialing party retries
within a bound the Spec states, and a bound exceeded is a refusal of the verb
rather than a wait without end. Admin's start step carries no handle of this channel
across the worker's exec: it starts the worker and that is the whole of its part, the
sink crossing in the enter directive below.

**The credential is this seam's authentication, per apex 5.1's first case.** The
invariant reads by credential where the channel has a name and by possession where
it has none, and this channel is addressable, bound by the harness. **The harness
verifies the
dialing peer's principal at every accept, before any byte, and refuses every peer
that is not the operator's.** The channel is reachable from inside the sandbox, so
that check is what refuses an elected tool holding the agent's own principal, and it
discriminates where the earlier design's
credential check could not: the expected peer is root, which no tool of the
agent's holds. The second-opener case `weaver-organ-channel` section 2 rejects
stays rejected, by refusal at accept rather than by an absent name.

**The worker holds the agent's own principal from its first instruction and makes its
memory unreadable to that principal's other processes after its final image
replacement.** The worker's code never runs above the agent's principal, because admin's
start step drops before the exec, per `weaver-admin-PRD` section 7 as ruled 2026-10-03.
An earlier form of this clause ordered a drop against the handoff, and the ordering had
a subject only while the worker began life holding a higher principal.

**Nothing about the handoff rested on the drop, which is why removing it costs nothing
here.** **What crosses is a capability rather than a name**: the receiver is granted
access to the already-open sink itself, and its own principal is never checked against
what the sink refers to. So the principal the worker held at the moment of receipt was
never what made the handoff safe, and the clause that said the drop does not gate what
may
cross is now a statement about a principal that never changes.

**The worker's memory is unreadable to processes sharing its own principal**, which
is what stops one of them from attaching to the worker and driving this channel and
the trace sink directly, **and it closes the second route by which such a process
could enumerate what the worker holds.** **The property resets when a process replaces
its
image**, so the requirement is stated against the last replacement. This is the whole
of what the removed ordering was protecting and it stands unchanged.

**This section is authoritative for that property.** It is something admin relies on
and cannot verify from outside the process, which is what a contract is for, so
`weaver-admin-PRD` section 7 points here rather than restating it. **What supplies it
is the Spec's.**

**On this seam the non-inheritance obligation of `weaver-organ-channel` section 2
lands on the trace sink, on the state channel's end the enter carries since the
ruling of 2026-08-26, and on this channel's own end.** The property rides the handle
rather than the thing it opens, so **a receiver that does not ask for it at the moment
of receipt accepts a handle its children will inherit**, and every child process a tool
call spawns from that point holds a writable
handle to the trace. Admin can open the file correctly and still lose the property at
the receive, so the obligation is the receiver's in section 5 rather than
the sender's. This channel's own handles are the simple case since the
inversion: the listener and every accepted connection are created after the
worker's last image replacement with the flag asked for in the creating and accepting
calls,
so no set-again ordering exists on the worker's side, and admin's dialing end is
flagged at its connect and dies with the verb.

**The listener lives from the worker's start until the seal (S7), and a connection
lives exactly as long as its verb.** The lifetime rule of `weaver-organ-channel` section 2 lands
on the listener: bound once at the worker's start, closed by the seal or, short of it,
by the worker's death, shared with no second worker, **and sealed once the leave's outcome is fixed** (S7),
in the steps `weaver-admin-Spec` section 3 gives for the seal: a join already held is
answered at the normal answer point, after the writer's drain, and a dial after the
unlink finds no socket. Each
accepted connection is one invocation's, closed
by admin when the verb answers, and the harness serves one connection at a time, a
second dial waiting at the listener rather than being answered concurrently, **except
while a leave is pending**: every wait of the leave hears the listener, so a
`JoinLeave` is served beside the leave's own connection, per section 3 and
`weaver-admin-Spec` section 3, I3, and a dial carrying any other directive meanwhile, but `Observe` and
the `Leave`s section 4 admits, is refused `OutOfOrder`, per section 4. **An answer
the harness cannot deliver ends nothing** (the operator's ruling of 2026-10-08 on #99):
where the caller went away before its answer, an operator's interrupt, a connector's
own timeout or a killed invocation, the harness says so on its standard error and
returns to the listener, the run standing as the directive left it; only the listener's
loss ends service, and a run ends only by the directive that ends it.

## 3. The exchanges

Six, and no others, all opened by admin.

**Every exchange of the lifecycle implements a row of the lifecycle state table**
approved by the operator on 2026-10-09, recorded on #1, which `weaver-admin-Spec`
section 3 holds as the one authority: the states S0 to S11, the transitions between
them and the invariants I1 to I9 are its. This section names what each row puts on this
seam and cites the row, never restating it, and where this text and the table disagree
the table holds.

**Save a point**, as of A3.2 on the operator's rulings of 2026-10-06 on #1.
`SavePoint`, opened by admin, carries the cause; the harness, at rest, asks the member
for a save point through `weaver-harness-state-contract` section 2's four legs, authors
the `save_point` event, and answers `SavePointTaken` naming the digest, the finished
name, the position the save point covers and the trace position of the event, its own
run and sequence (`weaver-admin-Spec` section 3, S2 x save-point and S9 x save-point, a
run whose unload stopped at its save point taking one at rest as any other does). It
refuses `ActivityNotAtRest` where a turn runs (S3 x save-point), `SavePointNotTaken`
naming which leg missed, or `OutOfOrder` where the run takes no save point, a diagnostic
binding or a serving run with no member seam, which has no member to ask and is never
reported as the member dead, and wherever a leave is pending (S4 to S7, and S10): while
the leave's invocation lives admin refuses the verb `InvocationInFlight` before it dials
(I1), and where it has died, outside the envelope, the verb reaches the harness and is
refused here.
The legs the harness names are the answer, the finished answer and the member dead, a
member's failed write arriving as the answer missed; admin names a fourth of its own,
`published`, at the unload and the `save-point` verb, where the save point was reported
and its publication did not land, per `weaver-admin-operator-contract` section 3.

**Enter the run.** Opened by admin. Admin directs the harness to enter, supplying the
session identity, the run reference, the kind of the binding, the trace sink handle, the
state channel's end where the member stands, the SPU instruction, the gate instruction
where the kind declares a Gate, the state election the tee applies, the store election
the member stands on, resolved to the embedded engine where the declaration is silent,
per `weaver-state-PRD` section 4, the lineage of the save point the load restores,
whether the declaration's `restore` names it or the inventory selected the latest
published by default, marked where it was named at a restore, and none where no save
point stands, the graceful leave's drain bound and wind-down bound as the declaration's
`[lifecycle]` sets them, each absent where it is silent and absence meaning unbounded,
the harness's only source for timing S4 and S5 (`weaver-admin-Spec` section 3, the
bounds), beside it and apart from it the reset, where the agent's last run did not
end in a clean unload or was forced to end and its leave save point could not be taken,
that run and the reason, per `weaver-admin-Spec` section 3, I4, whether or not a save point stands, the digests of the organ binaries admin
started and of the two it hands the worker to fork, the agent's SPU and the gate, the
digest of the declaration, as of 2026-10-03 the digest of the agent's boundary file, `roles.toml`, marked boundary and
never constitution, and the cause, the uid sudo reports, on the operator's rulings on
#50. The state channel's end is the harness half of the socketpair admin created at the
member's spawn, per the operator's ruling of 2026-08-26 carried at
`weaver-harness-state-contract`: admin couriers it and speaks on it never. Its absence
is the state leg not standing, never a refused load, and the two failure moments read
differently on purpose: a member whose spawn failed puts no end on the enter, and one
that died after the spawn is delivered and discovered as the closed pair the state
contract's dead-peer clause already covers. The instructions and the election are in the
directive because the ruling of `weaver-admin-PRD` section 6 gives admin no channel to
the SPU, the gate, or the state member, so if admin's intent for any of them does not
cross this seam it crosses nowhere. The kind is in the directive on the same argument
one step earlier: it decides which components the fan-out stands up, the fan-out is the
harness's, and a kind that does not cross here reaches no party that acts on it. On a
diagnostic enter the gate instruction is absent as a matter of shape rather than
omission, because the instruction names what the fan-out is to start and a diagnostic
fan-out starts no Gate. Which refusal catches a directive whose members disagree with
its kind is the Spec round's to place. The harness stands up an empty working structure,
authors its opening event, `load` on a serving binding and whatever the diagnostic
mechanism's own vocabulary names, which `weaver-harness-Spec` section 9 and
`weaver-diagnostic-PRD` section 6 hold open, asks the SPU to admit against the
instruction it was handed, and starts Gate last where the kind declares one. It answers
ready only when every step of the fan-out its kind declares has confirmed, or it
refuses, and a refusal names where the fan-out stopped, so that admin rolls back what
was built without asking a second question. The answer, either way, closes the exchange
and is the aggregate: one directive out, one answer back, and the organs appear in the
answer's content rather than as parties to this seam.

**Leave the run.** Opened by admin. `Leave` carries the cause, as the enter does, which
the harness records on its closing event, `forced`, and `rollback`, true only on a
load's rollback, each required on the wire with no default. **A leave is never refused for
activity**: one heard mid-turn is pending from that moment and the turn is handled as
below (`weaver-admin-Spec` section 3, S3 x unload and S3 x force-unload).

**Unforced, it is the graceful unload** (`weaver-admin-Spec` section 3, the graceful
unload, steps 2 to 7, and S2 x unload and S3 x unload). Where a tool call is out at the
gate, the harness first interrupts it (`ToolInterrupt`, the first outcome winning); then
it quiesces the gate (`Quiesce`, answered `GateQuiesced`) and refuses through the gate each frame the gate
flushes to it with `Unloading`, recording each as a refusal of the leave (S4 x dialer
request, I5). A turn in flight finishes what it can without further input, the interrupted call
answered `Killed { by: unload }` and the turn's
answer going to its caller (S4 x tool return and S4 x turn closes). It runs the
wind-down turn (S5). **The drain and the wind-down are unbounded unless the declaration
bounds them** (the operator's ruling of 2026-10-09 recorded on #1, `weaver-types-Spec`
section 2's `[lifecycle]`): past a declared bound the harness turns the leave forced
from where it stands, with no `forced_by`, the leave's `cause` staying the graceful
caller's, and comes down as a forced leave does. It lowers the gate (`Lower`,
answered `GateStopped`), takes the leave's save point through the four legs of
`weaver-harness-state-contract` section 2 and authors `save_point` (S6), then releases
the SPU and the member, authoring a `fault` for an SPU that dies in its release and
any recorder pressure,
authors `unload` with `forced` and the release outcome, the terminal event, drains
the writer's queue to the stream, and answers `Left` naming the save point as `SavePointTaken` does, so
admin publishes it under the same ordinal rule, with the same `forced` (S7). **`forced`
is the leave's state at its end, not the directive it began as**: false where the leave
stayed graceful, true where a join or a declared bound turned it forced, the cause
staying the graceful caller's and a bound naming no `forced_by` (`weaver-admin-Spec`
section 3, I2). **Where the save point of a leave that stayed graceful is not finished
it answers `SavePointNotTaken`** naming the leg, authors
nothing more, and stays entered at rest with the gate lowered, the leave no longer
pending (S6 x member misses a save-point leg, and S9). A later unforced `Leave` from
there goes straight to the save point, the lower being done (S9 x unload).

**Forced, it stops the work and keeps the state** (`weaver-admin-Spec` section 3, the
forced unload, sole, and S10). The gate is lowered at once with no drain, a frame it
holds recorded refused and its connection closed (I5); a turn in flight is cancelled and
closes as a stop with `reason: unload`; no wind-down runs; the leave's save point is
taken as at any leave; the SPU and the member are released and `unload` is authored
last, with `forced` true and the release outcome; and the answer is
`Left` with `forced` true, naming the save point where it was taken and none where it
could not be, the miss then recorded as a refusal of the leave naming the leg.

**The load's rollback is its own row** (`weaver-admin-Spec` section 3, S1 x Leave): a
`Leave` with `rollback` true, which admin sends only to undo its own load, takes no save
point and runs no wind-down in any position, `Entered` included, since a `Ready` can
land just after admin's enter deadline and leave the harness `Entered` while admin rolls
back; it authors `unload` with `forced` true and the load's cause, and answers `Left`
with none. Admin leaves the marker as section 4's marker-write rule gives (K1), and the
rollback is the exemption I2 names. The directive marks it, never the harness's
position.

**`Left` carries `forced` and `no_state`**, each required on the wire with no default, `no_state` true where the run has no state to keep (every diagnostic binding, graceful or forced, and a serving run with no member seam), so a `Left` with no save point says on the wire whether there was nothing to keep or the save point was not taken; `forced` true where the leave
came down forced, directed so, joined, or turned so past a declared bound, so every
invocation answered `Left` concludes by it, the first closing the marker as the leave
ended and the rest finding it closed (`weaver-admin-Spec` section 3, the conclusion), and the
answer's `forced`, the `unload` event's `forced` and its `forced_by` agree
(`weaver-admin-Spec` section 3, I2, the rollback excepted by name). A `Left` with no save
point is a forced leave whose save point could not be taken, a rollback's leave, a
leave under a diagnostic binding, or a leave of a serving run with no member seam, the
last three taking none. Admin concludes each `Left`, with a save point or none, forced
or not, as `weaver-admin-Spec` section 3's outcome table for the conclusion gives, one
row per combination: a `Left` with `no_state` true, every diagnostic binding and a
serving run with no member seam, closes `Closed`, nothing having been kept to lose, the forced leave whose save point was not taken closes `Forced`, and the
rollback leaves the marker as K1 splits it, `Open` once `load` is on the trace. **After answering left the
worker exits**, its run being its only purpose, and admin reads that exit as the run
lock's release (S8). The stream ends where the run did, finalized by nothing, per the
ruling of 2026-08-01. As with enter, the answer is the aggregate and the organs appear
in its content rather than as parties to this seam.

**Join the leave**, as of the lifecycle act (2026-10-09). Opened by admin's
`force-unload` when another invocation holds the invocation lock, so a force acts
beside the lock's holder only through this exchange or by ending the run's processes
(`weaver-admin-Spec` section 3, the forced unload, joining, and I1). `JoinLeave` carries
the forcing caller's cause and nothing else. **Where a graceful leave is pending** (S4
to S6) the harness turns it forced from where it stands, skipping what remains of the
drain and the wind-down, and comes down as a forced leave does; the leave's own `cause`
stays the caller who asked for it and the cause of the first force to turn the
still-graceful leave forced is recorded as `forced_by` on `unload`, a join into a leave a
declared bound already turned forced naming none; and the join and the leave's own dialer are answered with the
same `Left`, `forced` true (S4, S5, S6 x force-unload). A later join receives the same
`Left` and is named nowhere. **From S7 on**, the save point taken or missed and
the leave's outcome fixed, a join the harness holds at the seal adds nothing to the
record and is answered with the `Left` as it stands, its caller concluding (S7 x
force-unload; the conclusion), and a join after the unlink finds no socket. **Where a
forced leave is pending** (S10) the join is answered with that leave's `Left` (S10 x
force-unload). **A `Leave` meeting a pending leave is refused `OutOfOrder`**, forced or
not: the join is the one overlap the operating envelope admits (`weaver-agent-PRD`
section 6.1), and an invocation killed mid-command gets admin's conservative answer,
never a takeover (`weaver-admin-Spec` section 3, outside the envelope). **The harness hears a join in every wait of
the leave**: the wait for `GateQuiesced`, the turn's finish, the wind-down, the lower,
the save point's legs and the unwind after them, so a forced leave never waits on what
remains (I3). **With no leave pending** (S2, S3, S9, or before the enter) the harness
refuses `OutOfOrder`, which tells admin the lock's holder is no unload, and the force
refuses `InvocationInFlight` (the forced unload, joining). A join
unanswered within 150 seconds is a silent harness, which admin ends without the lock
(S11, and I3); this seam carries nothing of that escalation.

**Stop the turn.** Opened by admin. Admin conveys the operator's intent to stop, one bit
and no work beside its cause, which the harness records on the stop's turn close. A stop
that finds no turn in flight closes no turn and changes nothing in the agent, so admin
writes it with its cause to its operations log instead. The harness aborts the turn in
flight, the turn closes with the stop reason marked in place of a response, and the run
stays open. The harness answers with the turn's fate, aborted naming the turn it closed,
or at rest because nothing was in flight, and both are clean closes of the exchange
rather than refusals, because the operator's intent is satisfied by the state either
way. The answer is given only after the close event is placed, which is the
announce-after-record discipline. Stop touches no run bracket. It is the channel the
operator interrupt of `weaver-harness-PRD` section 2 arrives on, and it exists on this
seam because the operator holds no other crossing. How the abort lands at the decoder is
the harness's interior and crosses nowhere on this seam, and since 2026-09-22 the same
holds of a tool running when the stop arrives: the harness cancels the execution through
`weaver-harness-gate-contract` section 2 and answers here after the turn's close is
placed, the promise above carrying no invocation exception.

**Observe the run.** Opened by admin, added 2026-09-04 per issue #435 as the observation
exchange this contract's parties named as owed on 2026-08-06. Admin asks what stands and
carries nothing. The harness answers `State` from whichever position it holds: before
any enter, `Unloaded` and no load. Entered, `Idle` where no turn is in flight and
`Active` where one is, with the load's facts beside the state, the session, the run, the
declaration's digest as admin read it at the enter, the artifact, the elections the load
stands under, the store the member stands on and whether its end arrived, and the
composing loop by binary and, where it is a file, path and digest, the same facts the
`load` event carries and read from the run rather than the record. While a leave is
pending, `InTransition`; while the leave's invocation lives `show` answers `InTransition`
without dialing, so an `Observe` reaches a pending leave only where that invocation has
died, outside the envelope (`weaver-admin-Spec` section 3, the ladder). After a leave,
`Unloaded` and no load, the position being terminal. The answer is the harness's own
word and never a read of the deployment, it touches no bracket and authors no event, and
an observation arriving during a turn is answered from inside it, between tokens, as
`Active` with the load's facts, since 2026-09-05 per issue #441, so the answer is
trustworthy at every moment a dial is accepted, the one bound being the single token
whose decode is in progress. Where no worker answers the dial at all, admin has no
exchange to open and answers by `weaver-admin-Spec` section 3's ladder for `show`:
`InTransition` with the constituents named while the run lock is held, and `Unloaded`
only once it is free, the one place residency is read.

**There is no alert exchange, per the fault-carrier ruling of 2026-08-01.** A fault
the worker survives is a `fault` event, authored by the harness into the stream
like every other event, per `weaver-trace-PRD` section 3.1, and the stream is the
program's one fault carrier: the operator's tooling keys on it there and comes back
by running a verb, per the basic loop's section 2. Admin learns nothing of a fault,
holding
custody of the sink and comprehension of nothing. An earlier form of this section
carried the fault to admin as a fourth exchange, the alert, and the ruling retired
it: with one outbound path carrying every event in order, a second carrier for the
same fact was a channel earning nothing.

**A fault the worker does not survive is not a `fault` event.** Death is observed
through closure per section 4, and the harness does not report its own death.

**The fault's case set is open with a defined exit.** The candidates named so far
originate in the SPU and reach the record through the harness as author. The set
closes when the organs that can raise a fault have charters naming what they raise,
and the first of those is `weaver-spu-PRD`. This document binds nothing of it, the
shape now being the event kind's, per `weaver-trace-PRD` section 3.1.

**Handles cross once, in the enter exchange, under either kind.** None is
re-sent, revoked, or replaced. A harness that needs a handle it was not given
has a failed load rather than a second request to make, because there is no
exchange in which it asks for one. **The sink's count is one and does not
follow the kind**, both kinds authoring a record and the kind selecting the
mechanism the harness authors through, per `weaver-agent-PRD` section 6 as
ruled 2026-08-24. An act earlier that date had the handle crossing only for a
serving binding, on a reading the ruling replaced, and this sentence is the
correction rather than a second rule beside it. **The state channel's end
follows the leg rather than the kind**, per the operator's ruling of
2026-08-26: it crosses where admin stood the member and is absent where no
member stands, either kind standing the member per `weaver-agent-PRD`
section 6, and its absence reads as the leg not standing at the same site the
dead-peer conversion already covers.

**No exchange carries a path.** Admin sends handles and the harness never learns a
name, which is the handle discipline of `weaver-harness-PRD` section 5 stated as
an obligation on the party that could break it.

**No exchange carries work,** in any form and under any framing, in either direction.

## 4. Ordering

- The ordering below is the worker's rather than any connection's: exchange state
  survives a connection closing, because connections come and go with verbs and
  the run does not.
- Enter is first and happens exactly once in a worker's life.
- Stop is valid only between a completed enter and a leave, and a stop arriving at
  rest answers at rest rather than refusing, a run whose unload stopped at its save
  point (`weaver-admin-Spec` section 3, S9) included.
- An organ fault before the enter aggregate is answered is a refusal on the enter
  exchange naming the arm, rather than a `fault` event, the report to admin and
  the account on the stream being two different things.
- Leave is terminal for the worker and is pending at most once. While one is pending
  (`weaver-admin-Spec` section 3, S4 to S7 and S10), every directive but `Observe`,
  answered `InTransition`, `JoinLeave` and a `Leave` is refused `OutOfOrder`. While the holder lives, admin's invocation lock
  refuses those verbs before it dials (I1); where it has died, outside the envelope, the
  harness refuses them here, admin answering conservatively (`weaver-admin-Spec`
  section 3, outside the envelope). An unforced `Leave` meeting a pending
  forced leave is refused `OutOfOrder`. A leave refused `SavePointNotTaken`, one that stayed graceful to its
  miss (a forced one answering `Left` instead), is no longer pending, and the run stands entered at rest for a retried or a forced `Leave`
  (S9).
- `JoinLeave` is valid only while a leave is pending, and is refused `OutOfOrder`
  otherwise, before the enter included.
- Within the run, the record's order is `weaver-admin-Spec` section 3, I9: the
  harness authors `save_point` before `unload`, releases the SPU and the member before
  `unload`, any fault of that release and any recorder pressure authored before it, and
  authors `unload` last,
  nothing after it, per `weaver-trace-Spec` section 3.
- Messages within one exchange are ordered.
- An answer to enter arrives only after the working structure is standing, the
  model is admitted, and Gate is started, so admin may rely on a ready answer meaning
  the interior is serving rather than starting. The reliance is exactly as large as
  the fan-out, per section 3.
- An answer to leave arrives only after the queue is drained, so admin may rely on a
  left answer meaning what was admitted reached the stream. The `Left` a join receives
  is the same answer, sent after the same drain.
- A directive that arrives out of this order is refused and is not queued.

## 5. What each party supplies and guarantees

This section is derived from section 3 rather than prose beside it, because every
exchange payload change is a supplies change by construction, and a Spec writer reads
this list.

**Admin supplies** the session identity and the run reference for the run being entered,
the kind of the binding, resolved to serving where the declaration is silent, the trace
sink handle, the state channel's end where the member stands, the SPU instruction the
fan-out admits, the gate instruction the fan-out starts where the kind declares a Gate,
the state election the tee applies, resolved to the ruled default where the declaration
is silent, the store election the member stands on, resolved to the embedded engine
where the declaration is silent, the
lineage of the save point the load restores, its digest, the run, sequence and last turn
it covers, whether it was named at a restore, where the offline builder made it from a
record that record's session and the run and turn of its cut, resolved by admin from the
save point's stamp and the manifest's line and never the save point's path, beside it
and apart from it the reset, where the agent's last run did not end in a clean unload or
was forced to end and its leave save point could not be taken, that run and the reason, whether or not
a save point stands, on the operator's rulings of 2026-10-02 on #58, so the harness names where
its state came from without opening anything, the digests of the organ binaries admin
started and of the agent's SPU and the gate it hands the worker, keyed by name, the
declaration's digest as this crate read the file at the inventory, so the run and the
record can both name what they were built from, the operator's uid as the root's
`operator` key names it, as of 2026-10-06, so the harness admits the seeding line of
`weaver-gate-world-contract` section 2 from the operator alone, the boundary file's
digest, so the record declares who could read the run without that file
joining the tuple, the cause of every load, unload, save point and stop, the uid sudo reports, whether
a leave is forced (by `force-unload` or by a load's rollback), whether it is a load's
rollback (`Leave.rollback`, which admin sets and the harness never infers from its own
position, per section 3), the forcing caller's cause on a join, the engine libraries'
directory where the agent's root names one, and the intent to stop. Admin never writes the trace: it hands these facts to the harness,
the single writer, as it hands the declaration's digest.

**Admin guarantees** that the trace sink handle it passes refers to the sink the
session's configuration declares, that the run reference distinguishes this run
from every other run of that session, distinctness being the guarantee rather
than any particular rendering of it and the session possibly spanning agents,
and
that the boundary the worker runs inside exists and is correct, because
admin verified it before the worker started and is the only party positioned to. The
guarantee is of verification rather than of authorship, since the boundary is the
operator's artifact. It guarantees that no directive carries work of any kind. It
guarantees that a `force-unload` acting beside a holder of the invocation lock acts
only through `JoinLeave` or by ending the run's processes, never through publication or
the marker while the holder holds the lock; that any caller answered `Left` publishes
and writes the marker only once it holds the invocation lock itself (the conclusion);
that where it ends the run's processes it writes the marker `Forced` only after they
are gone; and that every marker write follows `weaver-admin-Spec` section 6's
marker-write rule, under the invocation lock, the marker read first, one another run's
or already closed for this run left alone (`weaver-admin-Spec`
section 3, I1 and I3).

**The harness supplies** its readiness as the aggregate of the enter fan-out, its
confirmation of departure naming the leave's save point where it took one and whether
the leave came down forced, the same confirmation to a join, a save
point's report (digest, finished name, covered position and the event's trace position)
or the leg that missed, the turn's fate on a stop, and its state with the load's facts
on an observation.

**The harness guarantees** that every connection is credential-checked at its
accept, before any byte is read, and that a peer that is not root is refused, per
section 2. It guarantees that every handle it accepts is accepted withheld from its
children, per section 2, **which is an obligation on the receiving call and cannot be
met by the sender.** It guarantees that it authors the run's bracket
events, that it writes only through the handle it was handed, that it resolves no
path, and that a ready answer is given only after a standing working structure, an
admitted model, and every further component the kind declares started, the
gate where one is declared. It guarantees that a refusal names where the
fan-out stopped, so that admin rolls back on the answer alone. It guarantees that a
fault the worker survives is authored to the stream as a `fault` event, per the
fault-carrier ruling, and that no run blocks on anything downstream of the
emission. It guarantees that a stop answer follows the close event it reports, so
the record holds the abort before the channel does. It guarantees that `Left`'s
`forced`, the `unload` event's `forced` and its `forced_by` agree (`weaver-admin-Spec`
section 3, I2, the rollback's leave excepted by name), that every wait of a pending leave hears a join (I3), and that every
frame the gate admitted is answered, recorded refused, or recorded as a turn the unload
stopped, before the leave's save point is taken (I5).

**Non-inheritance is the receiver's, and only the receiver can supply it.** The flag
rides the handle rather than the open file description, so it does not cross with
a passed handle and the receiving call is the one place it exists. It is a behavior
rather than a type property on the receive path, so it takes the perturbation-verified
test apex section 11 asks for rather than a compile-time pin. What can be pinned is
the shape: one receive site, taking no flag argument, returning a handle the rest of
the crate cannot construct another way.

**Neither party guarantees the stream's tail.** `weaver-trace-PRD` section 4.2
forfeits the writer's queue to process death and bounds the depth by the deployment,
so an answer to leave covers what was drained and an abrupt exit covers nothing.

## 6. Failure

Refusals are typed and enumerable, and every one of them is the harness refusing an
ask, because admin answers nothing. The cases:

- the dialing peer's credential is not root, and the connection is refused at the
  accept before any exchange opens
- the sink handle is absent, unusable, or does not carry the required flags,
  the state channel's end being no refusal ground: its absence is the leg not
  standing, per section 3
- an organ the enter fans out to refused, and the refusal names which organ and
  carries its reason, so the aggregate answer is one refusal rather than a report to
  parse
- the directive is out of order for the channel's state: a `JoinLeave` with no leave
  pending; and, while a leave is pending, any directive but `Observe` (answered
  `InTransition`) and `JoinLeave`, a `Leave` of either kind included
- activity is not at rest, so no save point is taken on demand (`weaver-admin-Spec`
  section 3, S3 x save-point); a leave is never refused for activity
- the leave's save point is not finished on a leave that stayed graceful to its miss
  (one a join or a declared bound turned forced answers `Left` with none instead), which names the leg and
  leaves the run entered at rest with the gate lowered (S9)

`Unloading` is no refusal on this seam: it is the harness's answer through the gate to
a request the drain meets, per `weaver-harness-gate-contract`.

**A stop at rest is not a refusal.** Nothing was in flight, the intent is satisfied
by the state, and the answer says at rest. The out-of-order case above still covers a
stop before enter or after leave.

What a refusal leaves behind is scoped by the authoring point of the fan-out.
Before the `load` event is authored, a refusal leaves the harness in the state it
was in before the directive: no run was entered, no bracket was opened, and the
stream never shows a run that was not entered. After the `load` event is authored,
a refusal from a later arm leaves the authored bracket on the stream with no
`unload` behind it, a truthful account of a load that did not complete rather than
corruption to repair, per `weaver-admin-PRD` section 5. The exit itself does not
change: the harness reports where the fan-out stopped, admin unwinds and publishes
no state, and nothing reaches back to erase what was authored.

**A worker that dies is not a refusal.** Admin observes the process exit and the
channel closure together, and what that leaves on the stream is a run whose `load`
has no `unload`, a truthful account of a death rather than corruption to repair,
per `weaver-admin-PRD` section 5. **The worker's death** while a leave is pending ends
the run with no `Left`, and the marker stays `Open` (`weaver-admin-Spec` section 3, S4 to
S10 x the worker dies, I2 and I4): a run whose record holds no `unload` never answered
`Left`, and a worker's death in S2 to S7, S9 or S10 is S8b as far as admin can tell (`weaver-admin-Spec` section 3: the worker gone, no `Left`, the marker `Open`, S0d once the run lock frees).
**The member's death, the worker alive,** ends nothing: it is the dead peer of
`weaver-harness-state-contract` section 5, so in S4 to S6 the leave goes on and its save
point misses `MemberDead` at S6, a leave that stayed graceful answering
`SavePointNotTaken` and standing in S9, a forced one answering `Left` with `forced` true
and no save point; and in S7 a member whose seam is found dead at the release reads
`Unconfirmed` in the `unload` event's release, no `fault` authored, a dead peer being
no fault (`weaver-admin-Spec` section 3, S4 to S10 x the member
dies, the worker alive).

**Nothing on this seam retries, but the join.** A refused directive returns to admin,
which either rolls back or reports. A harness that retried an author, or an admin that
re-sent a directive after a refusal, would put two attempts behind one operator intent.
A `JoinLeave` refused `OutOfOrder` is not retried either: it tells admin the lock's
holder is no unload, and the force refuses `InvocationInFlight` (`weaver-admin-Spec`
section 3, the forced unload, joining).

## 7. Prohibitions

**On admin.** It sends no work, in any form and under any framing, and a run in progress
narrows nothing about that. It sends no path. It asks for no event to be authored on its
behalf. Into a running
turn it conveys the operator's intent to stop or to leave and nothing narrower, because
the abort's mechanics are the harness's, per `weaver-admin-PRD` section 3: a graceful
leave waits for the turn's close rather than racing it, and a forced one has the harness
cancel the turn as a stop does (`weaver-admin-Spec` section 3, S3 x unload and S3 x
force-unload).

**On the harness.** It opens no exchange at all, the alert retired to the stream by
the fault-carrier ruling, and this is the prohibition that replaces the older one
that it initiates nothing. It writes nothing
outside an exchange. It does not resolve a trace path or accept one. It does not report
its own death. It announces nothing it has not first recorded. It asks admin for
nothing, because a notification carrying a request
is a control surface wearing a notification's clothes. It does not treat a directive as
authorization for anything beyond the directive, which is the shape a lifecycle channel
would grow a control surface through.

**On both.** Neither party carries a fact about the other's interior. Admin does not
know what a turn contains and the harness does not know what a boundary is made of, and
the exchanges above are the whole of what either learns.

## 8. Vocabulary

**Drawn from `weaver-types`:** `organ-envelope`, `lifecycle-directive`,
`lifecycle-answer`, `lifecycle-refusal`, `peer-identity` as of the inversion ruling of
2026-08-05, because the identity the harness reads at every accept is the floor's,
`model-binding` and `residual-readout-election` as of the route act of 2026-08-10,
`gate-instruction` as of 2026-08-17, `state-election` as of 2026-08-19, `field-election`
with `surprisal-election` as of 2026-08-21, and `binding-kind` as of 2026-08-24. The
observation exchange of 2026-09-04 draws nothing further: its directive is a case of
`lifecycle-directive`, its answer the `State` case of `lifecycle-answer` grown to carry
the load's facts, both the floor's.

```graph
edge: draws
from: weaver-admin-harness-contract
to: peer-identity

edge: draws
from: weaver-admin-harness-contract
to: organ-envelope

edge: draws
from: weaver-admin-harness-contract
to: lifecycle-directive

edge: draws
from: weaver-admin-harness-contract
to: lifecycle-answer

edge: draws
from: weaver-admin-harness-contract
to: lifecycle-refusal
```

**`model-binding` and `residual-readout-election` are drawn as of the route act
of 2026-08-10, and the first was owed from the cut.** The enter directive has
carried the model binding since this contract was written, and a payload term
the clause never named left the interface short of the completeness apex
section 5.3 demands. The election joins it because admin holds no channel to
the SPU, per `weaver-admin-PRD` section 6, so what the SPU judges at admit
crosses this seam first or crosses nowhere. Both are fields of the agent's
configuration, defined at `weaver-types-PRD` section 2.1, and both cross inside
`spu-instruction`, the section `weaver-types-Spec` section 2 shapes. The
section is representation rather than a term of its own, so the draws name the
definitions and not the grouping.

**A lifecycle refusal is clerked where the run stands, 2026-08-22.** Per the
operator's ruling that a refusal is clerked in one kind for every seam. An
enter refused after its bracket has been authored reaches the record as a
`refusal` naming this seam and carrying `lifecycle-refusal`, authored by the
harness into the run, per `weaver-trace-PRD` section 3.1's twenty-first
kind. The refusal still travels to admin in the answer, unchanged, and the
record gains what the answer alone never left behind.

**The run stays entered and the refusal does not unwind it.** A bracket that
stands is a run in place, and what unwinds it is the operator's `leave`,
arriving later as its own directive: the refused enter leaves a run holding
what it forked, which is why dropping it here would orphan those processes
and leave the bracket unclosed. **So the refusal is an event inside a
standing run rather than a last word before teardown**, and the close that
ends the bracket is the leave's.

**An enter refused before its bracket stands reaches no record at all**, and
this act does not change that. The `load` event opens the run, a refusal
falling earlier has no run to author into, and no later event can carry it.
**The refusal reaches the operator's answer and nothing else**, which is a
hole named here rather than closed, since closing it means authoring into a
run that does not exist.

**`field-election` and `surprisal-election` are drawn as of 2026-08-21, and
the first was owed from the act that defined it.** Both cross inside
`spu-instruction` beside the binding, and both reach the SPU by this seam
first or by none, which is the route act's own argument applied to the two
elections added after it. The field's election was defined on this date and
drawn by neither contract it crosses, so the graph carried a definition no
seam admitted to carrying while the SPU judged its depth at admit. **The
rule that catches this is `weaver-types-PRD`'s**, that a field the SPU
judges is a field that reached it across a seam, and it was stated for the
readout and not applied to what followed.

**`gate-instruction` is drawn as of 2026-08-17, and the route act named it
owed.** It crosses inside the same directive as the two above and had the same
gap: the enter directive has carried it since the fan-out was drawn, per
sections 3 and 5, and a payload term the clause never names leaves the interface short
of the completeness apex section 5.3 demands. The argument is the election's,
one seat over: admin holds no channel to the gate, so the instruction that
names the seams the gate holds crosses this seam first or crosses nowhere, and
the harness carries it to the gate spawn. The definition is
`weaver-types-PRD` section 2.1's, and this contract is the third to draw it,
after the two gate seams that consume it.

```graph
edge: draws
from: weaver-admin-harness-contract
to: model-binding

edge: draws
from: weaver-admin-harness-contract
to: residual-readout-election

edge: draws
from: weaver-admin-harness-contract
to: field-election

edge: draws
from: weaver-admin-harness-contract
to: surprisal-election

edge: draws
from: weaver-admin-harness-contract
to: gate-instruction
```

**`state-election` is drawn as of 2026-08-19, with the declaration act that
adds it to the enter.** The argument is the standing one, a third seat over:
admin holds no channel to the state member, so the election the tee applies
crosses this seam first or crosses nowhere, and a payload term the clause
never named would leave the interface short of the completeness apex section
5.3 demands. The definition is `weaver-types-PRD` section 2.1's, the shape is
`weaver-types-Spec` section 2's, and the shape's groupings, `StateElection`
and `ElectedKindConfig`, are representation rather than terms of their own,
per this section's standing rule: the draws name the definitions and not the
grouping.

```graph
edge: draws
from: weaver-admin-harness-contract
to: state-election
```

**`binding-kind` is drawn as of 2026-08-24, with the act that adds it to the
enter.** The standing argument holds in a shorter form than any seat's: the
kind decides which components the fan-out stands up, the fan-out is the
harness's, and a kind that does not cross this seam reaches no party that acts
on it. The definition is `weaver-types-PRD` section 2.1's, per
`weaver-agent-PRD` section 6 as amended this date, and it crosses at the
directive's top level rather than inside any instruction, because its consumer
is the harness itself and no organ reads it.

```graph
edge: draws
from: weaver-admin-harness-contract
to: binding-kind
```

**Drawn from `weaver-traits`:** nothing. The clause is present with that answer
because `weaver-types-PRD` section 5 asks for it even when it is empty.

**`organ-envelope` belongs to the floor and not to this seam,** because it is the
carrier every organ contract draws rather than a thing admin and the harness agreed on
between themselves. It is named here because this was the first contract to need it. The
definition stays in `weaver-types` and the mechanics it serves live in
`weaver-organ-channel`, per `weaver-types-PRD` section 2.3.

**`peer-identity` is drawn as of the inversion and `authorization-predicate` still
is not, each stated rather than left to edges.** This seam authenticates by
credential per section 2, and what the harness reads at accept is the floor's
`peer-identity`. The rule it applies is fixed at root rather than configured, so no
predicate definition is reached and `authorization-predicate` stays undrawn.
`weaver-types-PRD` section 2.2 rested its scoped claim on this contract being the
counterexample to a universal, and this act re-aims that claim in the same batch,
the counterexample having inverted.

**Drawn from `weaver-trace`:** nothing. No event kind, envelope field, or payload
shape crosses this seam, and this contract names no field of the record's envelope.

**The definitions land in `weaver-types` and were owed by the act that wrote this
section, four of them, the fifth having left with `harness-alert`.**
`weaver-types-PRD` section 4 rules that wire vocabulary is absent on demand and that
the shared representation arrives when the first socket contract is written. This is
that contract, so the demand exists now and the definitions belong to the floor rather
than to either party, since admin and the harness both need them and neither may
depend on the other. The records below belong in `weaver-types-PRD` section 4 and are
written unfenced deliberately, so that a mapper reading this document does not ingest
records this document is not the source of:

    node: organ-envelope
    kind: vocabulary

    node: lifecycle-directive
    kind: vocabulary

    node: lifecycle-answer
    kind: vocabulary

    node: lifecycle-refusal
    kind: vocabulary

    edge: defines
    from: weaver-types
    to: organ-envelope

    edge: defines
    from: weaver-types
    to: lifecycle-directive

    edge: defines
    from: weaver-types
    to: lifecycle-answer

    edge: defines
    from: weaver-types
    to: lifecycle-refusal

Four definitions and no more. A carrier, a directive with its cases, an answer with
its cases, and a refusal with its cases is what sections 1, 3, and 6 demand, and a
fifth added because a fifth felt tidy would be a reserved slot in data form. The
stop exchange adds a case to `lifecycle-directive` and a case to `lifecycle-answer` and
adds no fifth definition, which is the enumeration growing where the shape already
lives, and the join exchange of the lifecycle act does the same: `JoinLeave` is a case
of `lifecycle-directive` and `Left`'s `forced` a required member of a case that stands, per
`weaver-types-Spec` section 4.2. `Quiesce` and `GateQuiesced` are cases of the same two
enumerations that cross the gate's seam and not this one. `harness-alert` was the fifth until the fault-carrier ruling of 2026-08-01
retired the alert exchange, the fault travelling as a `fault` event on the stream,
and the definition left `weaver-types-PRD` section 2.3 in the same act.

## 9. What this document changes elsewhere

Named here because a document whose revision reaches other documents and cannot be read
for the reach is a trap. These are owed by this act and belong in `weaver-admin-PRD`
section 11's register.

- `weaver-admin-PRD`. The seam is bilateral and the charter says so. That is a
  statement about admin being an organ rather than a statement about alerts.
- `weaver-admin-PRD` section 10. The handle cell is unchanged in count, because one
  pair still carries the seam, and unchanged in its exit condition.
- The G4 union grew from three values to five with the duplex rewrite of this
  contract, which drew `organ-envelope` and `harness-alert` where the simplex form
  drew three. The count of contracts is unchanged, both crates remain party to one,
  and an earlier form of this line said the union was unchanged by reading the first
  fact for the second.
- **The union stands at five again by a different route, as of 2026-08-05.** The
  fault-carrier ruling of 2026-08-01 retired `harness-alert` and left four, and the
  inversion of this act adds `peer-identity`, the identity the harness reads at
  every accept. No definition is owed to the floor by either move: `peer-identity`
  is defined at `weaver-types-PRD` section 2.2 and `harness-alert`'s definition left
  that charter with the exchange. The four definitions section 8 lists unfenced are
  what this contract's own act owed and remain four.

**Three of these landed on 2026-07-31 and are struck rather than deleted, so that a
reader of an earlier revision can tell a closed item from one that was never there.**

- `weaver-types-PRD`. Two definitions were added to the three already owed, and
  `organ-envelope` is marked as floor vocabulary rather than as this seam's. Landed in
  section 2.3, which is where that charter keeps definitions, rather than in section 4,
  which is where this list said they would go and where only the departure argument
  lives.
- `weaver-harness-PRD`. The harness opens exchanges, and the alert emit point is not
  designed on the assumption that the record is its only sink. Landed in section 4,
  beside the seam table the clause describes.
- `weaver-agent-PRD`. The organ definition, its two properties, and the harness as hub
  rather than spoke. Landed as invariant 5.4, taken early as a named exception to
  Working Process section 7 and recorded as such in apex section 5. This is the one
  item on this list that was registered and not applied under the apex rule, and the
  exception is what released it.

**Landed with the fifth revision, recorded rather than owed,** because the batch of
2026-07-31 carried every party in one act:

- `weaver-admin-PRD` sections 3, 4.1, and 8. The activity-control split, the
  handle payload at load, and the operator surface's stop conveyance.
- `weaver-harness-PRD` section 2. The interrupt's citation.
- `weaver-trace-PRD` section 3.1 and `weaver-harness-trace-contract` section 3. The
  `turn.closed` payload states its close kind.
- The `weaver-agent-PRD` correction list, deposited at review rather than by this
  act and cited by substance rather than position: the gate binds no network socket,
  restated at the apex re-authoring.
