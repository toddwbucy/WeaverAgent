# weaver-admin / systemd - contract

**Status:** MERGED. Cut 2026-08-05 with the admin recut, on the operator's ruling
that root and the init system are external to WeaverTools and our interfacing with
them is settled by contract. It is the third external boundary, joining
`weaver-admin-operator-contract` and `weaver-gate-world-contract`, and it is written
because the recut made this boundary load-bearing: an admin that runs one invocation
per verb holds nothing across time, so the party that keeps an agent alive across a
logout is the init system, and what the program relies on from it was stated nowhere.

**Date filed:** 2026-08-05
**Document ID:** `weaver-admin-systemd-contract`
**Parent:** `weaver-agent-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #734

---

## 0. What this document is

The agreement over the boundary between this program and the init system: what admin
asks of it, what it does on the program's behalf, what each side may rely on, and how
it fails. It is read alongside `weaver-admin-PRD`.

**Its subject is the agent's service lifetime and nothing else, per the operator's
ruling of 2026-08-05.** The trace's handoff was weighed at this boundary and stays
where it was: admin opens the sink under root and passes the handle inside the
enter directive, per `weaver-admin-harness-contract` section 3. Placing it here was
examined and declined, because the far side of every sink shape is already the
operator's, a file persisting on its own and a pipe or socket held by the operator's
own reader, so nothing needed a holder that a per-invocation admin lacks. The
placement would also have put the record on the unit's own output stream, **which is
inherited by every process the unit starts, by design**, handing each organ the
harness creates a writable handle to the agent's own account. That is the leak the
non-inheritance
discipline exists to prevent, and buying it back would have cost a new obligation
where a tested one already stands.

**The name pins an implementation and that is deliberate.** The corpus says the init
system in prose where the mechanism is generic, and this document says systemd
because a contract names its parties. What is contracted is the interface a systemd
implementation presents, and a site running something else is running a program this
contract does not describe.

It carries no representation. Which command-line verbs carry the asks below, and
whether a bus library replaces them, is `weaver-admin-Spec` section 6's election.

```graph
node: weaver-admin-systemd-contract
kind: document

edge: party
from: weaver-admin-systemd-contract
to: weaver-admin
```

The second party is the init system running as root, an external program rather than
a crate. The graph carries no node for a principal outside the program, so the party
is named in prose and the missing category rides
`weaver-admin-operator-contract`'s register entry rather than taking a twin.

**Why this is a seam and not a tool dependency.** Apex 5.1 binds every seam where one
crate asks another process to do something, and admin asks another process to start a
unit under an identity admin cannot assume. The init system's own interface is a
socket, its bus, so the boundary satisfies the invariant's first case and
authenticates by credential in the direction that matters: the manager admits root
and refuses everyone else, which is the operating system's check rather than one this
program writes. Reaching that socket through a command-line shim is a representation
of the ask and not a second boundary, which is why the shim is the Spec's and the
boundary is this document's.

## 1. What root holds on the program's behalf

**The agent's service lifetime, and that is the whole of it.** A worker runs as a
transient unit the init system starts, holds, and reaps. It outlives the invocation
that asked for it and outlives the operator's login session, which is the property
`weaver-admin-PRD` section 7 names as the honest answer to keep-alive.

**It is not the program's to hold, and that is the point of contracting it.** A
program that kept an agent alive itself would be writing a process supervisor beside
the one the operating system ships, and the supervisor it wrote would need a standing
process under a principal the agent cannot become, which is the service account the
recut struck.

## 2. What crosses in, from admin to the init system

**A start ask.** One transient unit for one agent, carrying the agent's `User=`, the
sandbox properties the operator's template fixes, the runtime-directory declaration
the coordination socket is bound inside, and the worker's argument vector. The unit's
name is derived from the validated agent name and nothing else crosses that could
widen it.

**The argument vector carries what the worker cannot discover and nothing else.** The
coordination socket's path, which the worker binds as its first act, the two organ
binaries it creates once a load reaches it, the agent's loop file where the
declaration names one, and the classify arm's binary where one stands beside the
worker's. A worker cannot bind a name it was never told, and this ask is the only path
from the operator's installed values into a process that does not yet exist, so the
alternative is not a smaller ask but a worker that refuses its own start.

**This clause named two values until 2026-09-07 and the vector had carried more since
2026-08-20**, the loop file having ridden it from the operator's ruling on issue #243
and the classify binary joining it at issue #497. Neither addition crossed anything new
at this boundary, which is why the omission went unnoticed and why it is corrected as a
statement rather than as a change.

**The vector widens nothing this boundary did not already carry.** The socket's path
derives from the agent name the unit's name and its runtime directory already carry, the
two organ binary paths are the operator's installed values, the SPU's being the one the
agent's own configuration root names, **the classify binary is a sibling of the worker
binary** rather than a value of its own, and the loop file is a path in the operator's
validated declaration. **The worker binary is the derivation's anchor and is not itself
on the vector**, being the value the unit starts rather than a value the vector carries,
so naming it here names a path the operator wrote and this boundary already knew. So a
manager reading the vector learns the same agent name twice, paths the operator wrote,
one path derived from a path the operator wrote, and one path the operator's own
declaration named. Section 7's prohibition holds unchanged: no part of the agent's
declaration is here, and a manager that logged the whole vector would still learn
nothing about a turn.

**The runtime directory is asked for here because its removal is the answer to a
stale socket.** A Unix socket's pathname outlives the process that bound it, so a
worker's death would otherwise leave a name behind and the next start would fail to
bind it. The program does not solve that by unlinking: a blind unlink races a live
successor and can remove the socket of the agent it is trying to start. It is solved
by asking for the directory to be the unit's, created at start and destroyed with the
unit, so the pathname cannot outlive the worker and no cleanup path exists to get
wrong. Measured 2026-08-05 against a live manager: the directory is created before
the unit's first instruction, owned by the unit's user, and removed with its contents
when the unit stops.

**A stop ask.** One unit named, stopped.

**A state ask.** One unit named, its activity reported.

**A clear ask.** One unit named, its failed state cleared so its name is free for
the next start. Asked only after a state ask has answered `failed` for a load that
was refused.

**No handle crosses in either direction**, per section 0. The unit is started
bare, it binds its own coordination socket, and the trace's handle reaches it
over that socket rather than through the manager.

**No agent work of any kind crosses**, in either direction and under any framing. The
init system starts a process and learns nothing of what the process is for.

## 3. What crosses out, from the init system to the program

**An outcome per ask.** A start that succeeded or failed, a stop that succeeded or
failed, and an activity state. These are the whole of what admin learns here, and
admin holds no other source for them since the recut, per `weaver-admin-Spec`
section 3.

**What an outcome does not carry is the reason, and the limitation is named rather
than glossed.** Measured against a live manager on 2026-08-05: a duplicate unit name
and a malformed property both fail with the same status and differ only in prose on
the error stream, and a start ask whose `User=` names no account **succeeds**, the
unit failing asynchronously where the ask never looks. So this boundary reports that
an ask failed and does not reliably report which failure it was, and a successful
start ask is not by itself evidence that a worker is running.

**The state ask is narrower than its three values suggest, measured the same date.**
`active` and `failed` each mean one thing, a running unit and one whose process
exited non-zero. `inactive` means at least three: a unit that stopped cleanly, a
unit that never existed, and a unit whose exec never succeeded because the binary it
named was absent. A party reading this boundary may therefore learn that a worker is
not running and may not learn why, and a program that rendered `inactive` as any one
of the three would be asserting what the boundary did not say.

**A failed unit holds its name until it is reaped, measured 2026-08-16.** A
transient unit whose process exits non-zero stays registered under its name, and
a later start ask for that name fails rather than replacing it. The failure is
not distinguishable in the start ask's status, per the paragraph above, and the
state ask reports it as `failed`, which is the one value this boundary states
without ambiguity. Admin therefore reads the condition from the state ask and
answers `PriorUnitUnreaped`. **Admin clears the failed unit itself, on the
operator's ruling of 2026-10-01,** as part of the refused load's rollback: the
refusal is answered as it stood, the clear ask frees the name, and the operator's
next load starts. The clear is a logged act of the rollback and the worker's own
output stays in the journal, so what the clear discards is the manager's record
that the unit failed and not the evidence of why. There is no automatic reload.

**Two things carry what the outcome cannot, and both are the program's own.** The
identity, the home, and the boundary are verified by admin before any ask is made,
per `weaver-admin-PRD` section 4.1 step 3, which is why the account case is refused
upstream rather than discovered here. And the coordination dial is the liveness
proof: a worker that never bound its socket refuses the load at the dial's bound, per
`weaver-admin-Spec` section 7, so a load never publishes on the strength of a start
ask alone. **A state ask follows a failed dial** so the refusal names the unit's
failure rather than reporting an absent residency, per that same section.

**What would settle a move to the bus.** Typed outcomes distinguishing the failure
cases above, and a job completion admin can await rather than infer. Neither is
reachable through the shim, both are ordinary on the manager's own interface, and the
cost is the dependency `weaver-admin-Spec` section 6 weighs. This clause names the
condition so a later pass weighs a stated trade rather than rediscovering it.

## 4. Ordering

- The unit is started before admin dials the coordination socket, because the socket
  is bound by the worker the unit starts, per `weaver-admin-harness-contract`
  section 2.
- A state ask is valid at any time and transitions nothing.
- A clear ask follows a state ask that answered `failed`, inside a refused load's
  rollback, and is followed by a state ask that says whether the name is free.
- A stop ask is answered when the unit has stopped rather than when the stop was
  accepted, so an unload that returns has a stopped unit behind it.
- Nothing here is ordered against the trace's handle, which crosses on a seam
  this boundary does not touch.

## 5. What each party supplies and guarantees

**Admin supplies** the validated agent name, the unit's properties as the operator's
template fixes them, and the worker's argument vector of section 2.

**Admin guarantees** that the name it interpolates is shaped as a name rather than a
path and names an agent whose own configuration root the operator wrote, root-owned
and writable by no one else, per `weaver-admin-PRD` section 7, so the authority cannot
be widened by an argument. It guarantees that every value in the argument vector is
one the operator wrote into that agent's root or one derived from that same validated
name, so the vector is a second reading of the agent's own root rather than a second
authority beside it. It guarantees that it asks for one unit per agent
and holds no second route to start one. It guarantees that the agent's identity and
boundary were verified before the ask, so the manager is never asked to resolve what
this program should have refused.

**The init system guarantees**, and this is the reliance set a reviewer checks, that
a started unit runs at the declared `User=` from its first instruction with no
interval at any other identity, that the declared sandbox properties are in force
before that instruction rather than applied after, that a unit name is unique so a
second start for a live agent fails rather than racing, that the unit's runtime
directory exists before that first instruction and is removed with its contents when
the unit stops, that the unit's cgroup arrives with the unit and is removed with it,
and that a stopped unit stays stopped without the program watching it.

**The runtime directory's removal is the load-bearing half of that list**, because
it is what makes the coordination socket's pathname unable to outlive its worker.
A manager that left the directory standing would return the stale-path problem to
the program, and the program's only answers would be an unlink that races or a name
that changes per run.

**What neither party guarantees.** Nothing here promises that a started unit will
run to readiness, which is what the dial proves, and nothing here promises anything
about the record, which is `weaver-admin-operator-contract`'s.

## 6. Failure

- A start ask fails and no unit exists, which refuses the load before any dial.
- A start ask succeeds and the worker never binds its socket, which the dial's
  bounded retry refuses and the following state ask explains.
- A stop ask fails and the unit stands, which the rollback reports as an act it
  could not undo, per `weaver-admin-PRD` section 5.
- A state ask fails, which is reported as unknown rather than guessed at, because a
  state this boundary could not answer is not a state the program may invent.
- A clear ask fails and the unit stays `failed`, which the rollback reports as an
  act it could not undo, the next load then refusing `PriorUnitUnreaped` as before.

**A unit that dies on its own is not a failure of this boundary.** The death is
observed where it is observed, at the coordination socket the next verb finds absent,
and this boundary reports what it is asked rather than announcing.

## 7. Prohibitions

**On admin.** It starts no process under another identity by any route but this ask.
It writes no unit file and edits no manager configuration, the template being the
operator's installed artifact. It does not read the manager's journal as a source of
program state, the trace being the program's one account. It holds no capability that
would let it bypass this boundary.

**On the init system's side, stated as what the program may not lean on.** The
program does not rely on the manager parsing, transforming, or interpreting anything
it carries. Nothing of the agent's content reaches this boundary, so a manager that
logged everything it saw would learn nothing about a turn.

## 8. Vocabulary

**Drawn from `weaver-types`:** `lifecycle-refusal`, the shape a failed ask returns
in.

```graph
edge: draws
from: weaver-admin-systemd-contract
to: lifecycle-refusal
```

**Drawn from `weaver-traits`:** nothing. The clause is present with that answer
because `weaver-types-PRD` section 5 asks for it even when it is empty.

**Drawn from `weaver-trace`:** nothing, and the absence is the subject boundary of
section 0 read as a vocabulary fact. Nothing of the record crosses here.

**The agent's name crosses and is drawn from nothing**, the negative stated rather
than left to a missing edge. It reaches this boundary as an interpolated string
inside a unit name rather than as a typed value, because what receives it is a
manager that holds no type of this program's. `weaver-types` shapes it where the
program handles it, and the shaping stops at the boundary, which is the honest
account of a value leaving a program's own vocabulary.

**The unit's properties are not vocabulary of this program.** `User=` and the
hardening set are the manager's own interface, named here as what admin asks for
rather than defined here as what the program owns. A corpus that defined them would
be claiming authorship of an interface it consumes.

## 9. What this document changes elsewhere

- `weaver-admin-PRD` section 6. The crate holds a second contracted boundary where
  that section said one, and the seam table gains a row. The one-seam claim was true
  of crate-to-crate seams and is restated as such.
- `weaver-admin-Spec` sections 3 and 6. The init system's reliance set is this
  document's, so the Spec cites it rather than arguing it, and the command-line
  election stays the Spec's as the representation of these asks. Section 3 gains the
  state ask that follows a failed dial.
- `WeaverTools-Document-Format`. The party-edge category for an external principal
  gains a third instance and is still owed, per
  `weaver-admin-operator-contract` section 8.
