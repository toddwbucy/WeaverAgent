# weaver-gate - Spec

**Status:** MERGED. Cut 2026-08-02, sixth of the Spec pass, specced to the same boundary
its charter is chartered to: the lifecycle half, with the traffic arriving via the token
workflow. Code is written against it under the gates of Working Process section 6.

**Date filed:** 2026-08-02
**Document ID:** `weaver-gate-Spec`
**Parent:** `weaver-gate-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #731

---

## 0. What this document is

Build instructions for `weaver-gate`: the binary's layout, the seam end's
adoption, the hook's mechanics, the predicate at accept, and the elections a
builder would otherwise invent. It is derived from `weaver-gate-PRD` and from
the two contracts this crate is party to, `weaver-harness-gate-contract` and
`weaver-gate-world-contract`, together with `weaver-organ-channel`, the drawn
material the first of them draws.

Level discipline. The charter says what the crate needs and why. This document
says how it is represented, and per gate G2 it elects against grounds the
charter and the contracts state rather than developing grounds of its own.
Where this document and the charter disagree the charter yields nothing.

**This document declares its crate's assertion records,** per Document Format
sections 3 and 4 as of the notation of 2026-08-03. The charter stays the source
of this crate's node, its parent edge, its one floor link, and its one declared
seam, and a Spec that restated them would give the mapper two sources for one
record, per Document Format section 1. What this document sources is the claims
code must conform to, declared at the clauses that argue them rather than
gathered in one place, per that format's section 6, and `asserts` runs from the
crate rather than from this document, which is why the document needs no node of
its own. **What this document leans on and does not argue is declared by the
crate that argues it,** and carries no record here. The descriptor's number and
the fork discipline that leaves this process one inherited descriptor are
`weaver-harness-Spec`'s, per its sections 2.2 and 8. The floor's three
exhaustive wire enums, `PeerIdentity`'s missing `Deserialize`, the socket-type
election with the boundary and truncation tests it owes the pair-creating
crates, and the one policy function with denial before permission are
`weaver-types-Spec`'s, per its sections 3, 4.2, and 4.3. A node declared twice
is the one-name-two-nodes defect that format forbids, and a reliance stays prose
because a citation is what it is.

**It is written from the merged corpus alone,** per the ruling of 2026-08-01
that keeps the old tree's Specs out of the Spec pass.

**The bound is the charter's own half-chartered line, and the line has
moved.** What is fully specifiable is the raise and the lower, the boundary
predicate, the process facts, and the turn half the charter's act of
2026-08-02 landed and section 4 charters: the turn exchanges, the relay's
interior, the line bound, concurrency against the one-turn loop, and drain on
stop. **What still defers is section 7's register and not a second list
here.** Two lists for one set is the duplication G5 refuses, and it is why
this clause and that one disagreed on three entries until 2026-09-15. Each
entry there is named rather than omitted, per charter section 8: an omission
and a deferral read alike to a later reader and only one of them is a
decision.

## 1. The crate

**One binary.** The gate is its own executable, forked and exec'd by the
harness during the enter fan-out, per apex section 12, and no other crate
links it. **The bin target does link the lib target beside it,** which is
this crate's own wiring rather than a consumer's reach. What
`tests/manifest.rs` pins is the forward half, that no organ appears in this
crate's own resolved tree at any depth. **The reverse half is review's, and
no instrument in this crate holds it:** a workspace member adding
`weaver-gate` to its dependencies would leave all six of those tests
passing. Buying it wants a reverse walk or a workspace-wide manifest scan,
and it is not bought here.

```graph
node: gate-one-binary
kind: assertion
tag: manifest

edge: asserts
from: weaver-gate
to: gate-one-binary

edge: grounds
from: gate-one-binary
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**Layout.** One module per obligation, with the lib target beside the entry.

    src/lib.rs      the module roots, and the target the bind-shape pins run under
    src/main.rs     entry, the serve loop, and the dispatch, sections 2 to 4
    src/channel.rs  adoption with the two hygiene sets, the framed send and recv
    src/hook.rs     the instruction's resolution, the bind, the predicate, section 3
    src/relay.rs    the pass-through, section 4
    src/tools.rs    the shell execution, this crate's one held tool, section 8

Six files. **`channel.rs` is the seam end and it performs both hygiene
sets**, `adopt` clearing the dumpable flag and setting close-on-exec and
electing the parent-death signal before the first read, per section 2, with
the framed send and recv and the fault render over descriptor 3 beside them.
`main.rs` calls `adopt` and holds no set of its own: what it holds is the
serve loop of section 4, the dispatch that drives the raise and the lower of
section 3, and the response routing, which is why its line reaches three
sections and why it does not read to wiring and nothing else.

**The lib target exists so the pins can execute.** `lib.rs` is not an API for
a consumer. Section 6's bind-shape pins are compile-fail doctests, cargo
collects doctests from lib targets alone, and a bin-only crate would carry
them as text that never runs. **So the target's presence is a precondition of
an instrument this document claims,** and it is asserted rather than assumed:
fold the modules into the binary and the pins stop being collected while
every other gate still passes, which is the reading that converts unenforced
into documented as enforced.

**The claim is about doctest collection and not about the `[lib]` table**,
and the distance between the two was measured rather than reasoned to. Cargo
auto-discovers `src/lib.rs` whether or not the table is written, so reading
the manifest for `[lib]` fails on a removal that changes nothing. Keeping the
target and setting `doctest = false` stops `cargo test -p weaver-gate` running
the pins with the target still standing, so reading only that a lib target
exists fails the other way. The instrument reads `cargo metadata` and asserts
two things: a target of the lib family, and its `doctest` field true.

**A third way out is the compiler's and not the instrument's.** The pins are
doctests on `hook.rs`, and a `hook` that left the lib's module graph would
carry them out, but `relay.rs` takes `crate::hook::Admitted`, so the removal
is a compile error rather than a silent unenforcement. Making the module
private is not that removal: measured 2026-09-15, a private `hook` with a
re-export builds and the pins are still collected, so an assertion on the
`pub` would fail while the property held.

**Two removals nothing here reaches, stated rather than left to be
discovered.** A doctest deleted from `hook.rs`, and the pins moved to a
module no target compiles. Both want an inventory of the collected doctests,
which a test in this crate cannot take: `cargo test --doc` inside a `cargo
test` waits on the build lock, and a hang is the one failure a watch cannot
report, which is the reasoning section 3 applies to the umask lock. **The
limit is the accepted one and the claim is drawn to it**, this clause saying
collection is possible rather than that the two pins were collected.

This clause is where the reasoning lives and the manifest, `lib.rs` and the
test cite it rather than restating it, per gate G5. **It develops a criterion
the charter does not carry**, and it stays here rather than landing in the
charter under G2 because what it elects is the representation's own
mechanics - which cargo target this crate declares so that an instrument this
document already claims can run - and not a reason the crate needs
something.

```graph
node: gate-lib-target-exists
kind: assertion
tag: manifest

edge: asserts
from: weaver-gate
to: gate-lib-target-exists
```

**Edition and toolchain.** Edition 2024 on the pinned nightly, no nightly
feature used.

**The dependency set is one internal crate and two external ones.**
`weaver-types` is taken **without its `config` feature**: this crate reads no
configuration file, per charter section 3, the gate instruction arriving over
the seam instead, so no parser enters a process whose whole argument is that
it holds little, which is the thinness the feature gate exists for, per
`weaver-types-Spec` section 1. It is the charter's one floor link and the
whole internal set. **`weaver-traits` is not taken.** It joined with the tool
workflow's opening act as the tool contract's executor and left with the tool
boundary ruling of 2026-08-18: the shell is this crate's own verb, dispatched
with no table, so the trait has no consumer here, and
`weaver-harness-gate-contract` section 7 draws nothing from it.
`serde_json` encodes and decodes the seam's envelopes and touches no client
byte, because the client's line is octets this crate must not read, per the
opacity rule. `nix` is the OS surface for what the standard library does not
give, on the grounds `weaver-harness-Spec` section 2.4 argued. The calls are
`send` and `recv` for the framed exchange over descriptor 3 with `fcntl` for
its close-on-exec, the two `prctl` sets of section 2, `poll` for the serve
loop of section 4, `umask` around the bind for the mode election of section
3, `getsockopt` for the peer credential with `getuid` for the predicate it
feeds, `User::from_uid` for the home the shell starts in, which comes from
the account database because there the uid's home is a fact rather than a
variable the agent's environment may or may not carry, and `killpg` with `waitid` for the group kill and the reap of section 8.
**The bind, the listen and the accept are not among them.**
`UnixListener::bind` is the standard library's and creates and binds and
listens in one call, per section 3, which is the shape the one bind site
pins. The list is closed over this crate's production paths and excludes the
`socketpair` its own unit suite builds fixtures with, a list that omits a
call being a surface this document understates and a builder discovers.

```graph
node: gate-floor-link-types-without-config
kind: assertion
tag: manifest

edge: asserts
from: weaver-gate
to: gate-floor-link-types-without-config

edge: grounds
from: gate-floor-link-types-without-config
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**No async runtime, no logging crate, nothing else.** The lifecycle traffic
is two exchanges and the turn traffic is served by `poll` and a serial loop
per section 4, so nothing here needs an
executor, and this crate writes no account of anything, per charter section
1: a logging crate would be a second author's first step. The absences are
checked by the build-time `cargo tree` assertion the floor Specs share.

```graph
node: gate-no-runtime-no-logging-no-yaml
kind: assertion
tag: manifest

edge: asserts
from: weaver-gate
to: gate-no-runtime-no-logging-no-yaml
```

## 2. The seam end, and the process facts

**The channel end arrives at descriptor 3, inherited rather than
re-decided.** `weaver-harness-Spec` section 2.2 elects the number and owes it
to this document, and this document takes it: the exec'd binary finds its
one inherited descriptor at 3, the first after the standard streams, and
wraps it as an owned handle at entry. It is the only descriptor this process
begins with beyond the standard streams, per the fork discipline of
`weaver-harness-gate-contract` section 1, which makes a build in which this
crate holds a trace, coordination, or residency handle broken whether or not
it uses one. That property is the harness's to enforce at the fork and this
crate's to rely on, and the reliance is stated rather than re-tested here,
the enforcing test living in the harness Spec's section 8. **The wrap is not
a formality.** Descriptors are owned types end to end in this crate, the
listener and the accepted connection of section 3 included, so no raw number
outlives the thing it names and no close happens twice.

```graph
node: gate-descriptors-owned-types
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-gate
to: gate-descriptors-owned-types
```

**Entry performs two hygiene sets and one election, before the first read.**
The dumpable flag is cleared and the channel end is set close-on-exec, both
sets and never checks, per charter section 7 and the set-not-check rule of
`weaver-organ-channel` section 2. **Close-on-exec answers a planned fork and
not only a compromise's exec.** This crate forks and execs a shell at every
invocation of the one tool it holds, per section 8, so without the flag
descriptor 3 - the channel to the harness - is inherited by a child process
running a command the model chose, which is the adversary the whole boundary
is built against handed the interior's own handle. An earlier form of this
clause reasoned from this crate spawning nothing, which was true when it was
written and stopped being true when the shell landed on 2026-08-18. The rule
is unchanged and its reason is now load-bearing rather than precautionary,
and it still costs one call. The election is the parent-death signal, which
the charter leaves
to this Spec: taken, one `prctl` naming `SIGTERM`, with its guarantee stated
at the width the kernel gives it and no wider. The signal fires on the
termination of the thread that forked this process, not of the harness
process, verified by both seats against a live kernel with the forking
thread exited and the process fully alive. So the backing is real exactly
while the gate is forked from a harness thread whose lifetime is the
worker's, an obligation owed to `weaver-harness-Spec` the way descriptor 3
was owed in the other direction, filed on the working list: that Spec's
everything-on-the-caller's-thread posture already implies it, and the
sentence makes it a stated constraint a later threading change must answer.
It backs the closure observation rather than replacing it, the requirement
standing on closure alone per charter section 4, and it covers the window
where the gate is blocked anywhere other than the channel read, for as long
as that constraint holds. **The backing is review's by election and not by
impossibility,** the third walk's test reaching the closure half alone. A
harness double that forks the gate from a thread of its own and then ends
that thread while its process lives and holds its channel end open produces
the condition the signal keys on, which is the case both seats compiled on
2026-08-02 to narrow the guarantee in the first place, so the reach exists
and this suite simply does not buy it. The signal's presence, the width of
its guarantee, and the thread-lifetime constraint the backing rests on are
review's on that ground, while the closure requirement they back is the
walk's, which is the same split the bind-site absence takes in section 3.
**The channel end's close-on-exec is review's on that ground and not a
weaker one,** an `fcntl` reading it as cheaply as a `prctl` reads the flag
set beside it: the two hygiene sets differ in the instrument bought, walk 2
taking the flag, and not in the instrument available.

```graph
node: gate-dumpable-flag-cleared
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-dumpable-flag-cleared

edge: grounds
from: gate-dumpable-flag-cleared
to: axiom-floor-is-vocabulary-behavior-is-socket

node: gate-channel-end-close-on-exec
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-channel-end-close-on-exec

edge: grounds
from: gate-channel-end-close-on-exec
to: axiom-floor-is-vocabulary-behavior-is-socket

node: gate-parent-death-signal-thread-scoped
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-parent-death-signal-thread-scoped
```

**The exchange service is a serial loop over the channel.** Directives
arrive as `OrganEnvelope` JSON, one message per envelope, on the
`SOCK_SEQPACKET` end the harness created, and this crate carries the
election's receive obligation as every receiving crate does, per
`weaver-types-Spec` section 4: the buffer is sized to the 64 kibibyte
envelope bound, a read returning with `MSG_TRUNC` set is a channel fault and
never a message, and the same bound is asserted on this crate's sends. A
directive out of order for the channel's state answers `OutOfOrder`, per
`weaver-harness-gate-contract` section 3, and the state has three positions,
before-raise, raised, and lowered, the last terminal.

```graph
node: gate-truncation-is-a-fault
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-truncation-is-a-fault

edge: grounds
from: gate-truncation-is-a-fault
to: axiom-floor-is-vocabulary-behavior-is-socket

node: gate-out-of-order-refused
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-out-of-order-refused

edge: grounds
from: gate-out-of-order-refused
to: axiom-contract-is-a-complete-interface

edge: grounds
from: gate-out-of-order-refused
to: axiom-harness-integrates-by-the-loop

node: gate-channel-state-three-positions
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-gate
to: gate-channel-state-three-positions

edge: grounds
from: gate-channel-state-three-positions
to: axiom-contract-is-a-complete-interface
```

**Closure is death, and the response is the charter's.** A read that returns
closure means the interior is gone: this crate closes its listener if one
stands and exits, per charter section 4, never treating closure as an
answer, per the drawn material.

```graph
node: gate-closure-is-death
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-closure-is-death
```

## 3. The hook

**The instruction is resolved, never interpreted beyond its fields.** The
raise directive carries the `gate-instruction` the operator declared and
admin validated, uninterpreted by the harness, and beside it the socket the
harness supplies, per `weaver-gate-PRD` section 2. This crate consumes exactly
two things: that socket path to bind and the access rule the predicate judges
against. **They arrive as two fields because they have two authors**, the rule
being the operator's election and the path being the program's deployment
fact, and a single field carrying both would put the operator's name on a
value they do not choose. The field list is the floor's satellite, per
section 6, and the demand stated here is what that satellite must carry.

```graph
node: gate-instruction-two-fields-consumed
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-instruction-two-fields-consumed
```

**The client socket is `SOCK_STREAM`, elected on its contract's framing.**
`weaver-gate-world-contract` section 2 fixes
newline-delimited JSON, one request per line, so the newline is the framing
and a boundary-preserving type would carry a second framing under the
first. A stream is also what a local client's ordinary tooling dials, which
is the audience that contract names. The opposite election from the organ
channels, principled the same way twice now, per `weaver-admin-Spec`
section 2.

```graph
node: gate-client-socket-stream
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-client-socket-stream

edge: grounds
from: gate-client-socket-stream
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The bind takes the path as given and refuses what it finds in the way.**
The socket is created with the close-on-exec flag in the creating call and
bound to the instruction's path. A path already occupied refuses the raise
with `BindFailed` and the reason carried, and this crate unlinks nothing:
the path is the operator's artifact, a stale socket left by an unclean
death is the operator's to clear, and a gate that deleted filesystem
entries to make room for itself would hold an authority its charter never
grants. Ready is answered only after both the bind and the listen have
returned, which is what makes ready a fact about the listener, per
`weaver-harness-gate-contract` section 2. **`hook.rs` exposes the crate's one
bind site,** taking the instruction's path, and a listener built anywhere
else in this crate is out of bounds. The two named shapes are pinned by the
compile-fail doctests of section 6, because an absence is what a runtime test
structurally cannot demonstrate, **and the general prohibition stays
review's,** a pair of doctests reaching the shapes they name and not the open
set of every way a path becomes a listener. The pinning and the prohibition
are two records for that reason, per section 6.

```graph
node: gate-ready-follows-bind
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-ready-follows-bind

edge: grounds
from: gate-ready-follows-bind
to: axiom-contract-is-a-complete-interface

node: gate-unlinks-nothing
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-unlinks-nothing

node: gate-one-bind-site
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-one-bind-site

edge: grounds
from: gate-one-bind-site
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The socket's mode is the boundary's election, not the umask's.**
`UnixListener::bind` sets no mode, so the file lands at `0777 & ~umask` and
the access control of the agent's front door is decided by whatever umask
the process inherited. The raise holds a umask denying every bit to others
across the bind, so the figure is this crate's and travels with it.

**The election is not a tightening in every direction, and saying so plainly
matters more than the story it replaces.** Against the two boxes measured on
2026-08-28 it narrows - `0777` and `0775` both become `0770`. Against the
common umasks it widens the group's reach:

| umask | before | after |
|---|---|---|
| `0022`, the usual default | `0755` | `0770` |
| `0002` | `0775` | `0770` |
| `0077`, a hardened default | `0700` | `0770` |

Connecting needs the write bit, so at `0022` the group could not connect and
now can, and at `0077` the socket was the owner's alone. **That is the
intent and it is conditional.** The operator reaches the socket by membership
in the agent's group, which is what `0770` is for, and the whole boundary
therefore rests on `Group={identity}` making that group exactly one agent -
per `weaver-admin-Spec` section 6, which sets it on the unit and refuses a
template that would take it back. Where the unit's group were a shared one,
`users` or `nogroup`, this mode would hand connect rights to everyone in it.
A dev run outside systemd has no such guarantee and no such boundary.

**The election is made in the creating call and not on the path
afterwards.** A `chmod` after the bind would answer the same question and
open three others. `bind` also listens, so between the two calls the socket
is live at the inherited mode and connections queued there are served. The
`chmod` would go by path, and the adversary this crate's reference walk
names is a tool running as the agent uid, which owns the runtime directory:
in that window it can unlink the socket and point the name elsewhere, so
the `chmod` lands on a decoy while the real socket keeps the umask's mode
and the raise reports an election it did not make. And a `chmod` that
failed would return after the pathname exists, leaving a file this crate
unlinks nowhere - so `gate-refused-raise-holds-nothing` would stop holding
and every later raise on that path would answer `Address already in use`
for the life of the runtime directory.

**The umask is process-global, so the guard serializes on it.** Two raises
on one process would otherwise interleave their save and restore and the
socket would land at whatever the loser inherited, which is the same defect
one level up. A gate organ raises once and meets no contention, and the lock
costs nothing while removing the case.

**The lock is reentrant within a thread, and that is a property of the
mechanism rather than a convenience.** It serializes threads, and a thread
cannot interleave with itself, so a nested acquire on the holding thread is a
no-op. Without that, a caller holding the umask and raising inside - the
natural shape, since the scoped accessor is offered for exactly the callers
that must set the umask around this crate's use - takes one non-reentrant
mutex twice on one thread and waits on itself forever. **The failure is a
hang and not a refusal**, so no socket stands, nothing is recorded, and the
test harness reports a killed binary rather than a defect. A watch on it
therefore runs the nested raise on its own thread against a timeout, a
deadlock being the one failure a test cannot observe from inside.

```graph
node: gate-umask-lock-is-reentrant-within-a-thread
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-umask-lock-is-reentrant-within-a-thread
```

**The watch is the removal of the umask guard**, not of a mode call: with it
gone the socket binds at the runner's own umask and the test reports `0777`.
Naming the removal matters because a property and its failure path are not
the same thing, and this act's first form was watched on a call that no
longer exists. **This
is not a hypothetical drift**: on 2026-08-28 one build bound `0777` on one
box and `0775` on another, the two holding different ambient umasks, and
neither figure was anyone's election.

Connecting to a Unix socket requires write permission, so denying the write
bit outside owner and group is what excludes a uid. The read and execute
bits others would hold under a laxer mode buy them nothing on a socket. The
operator reaches the socket through membership in the agent's group, which
is the provisioning already owed, and no one else reaches it at all.

**Two locks, answering different adversaries.** The mode stops a stranger
from reaching the door, and the credential check below stops one who does. A
boundary resting on the credential alone would still let any local uid
spend this process's accept loop, and one resting on the mode alone would
trust the filesystem with a judgment the rule owns. Neither is redundant,
which is the reasoning `weaver-harness-PRD` section 5 applies to the trace
descriptor's two locks.

```graph
node: gate-socket-mode-is-the-boundarys-election
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-socket-mode-is-the-boundarys-election
```

**Every connection is authenticated at accept, before any byte is read.**
The accepting call sets close-on-exec on the connection, the peer's
credential is read with `SO_PEERCRED`, and the identity is judged by the
floor's one predicate against the instruction's rule. Verified on the
admin Spec's pass and relied on here: the credential on an accepted
connection reports the connecting peer's own uid, gid, and pid.

```graph
node: gate-authenticated-at-accept
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-authenticated-at-accept

edge: grounds
from: gate-authenticated-at-accept
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The agent uid is denied by construction, not by configuration.** This
process runs as the agent uid, so it knows the one uid the boundary exists
to exclude: its own. The deny set the predicate judges is the instruction's
rule with this process's own uid added at raise, unconditionally, so no
operator mistake in the rule can readmit the agent, denial winning over
permission per `weaver-types-Spec` section 3. A peer that fails is refused
by closure before any content is read, per `weaver-gate-world-contract`
section 5, and nothing is written to it, an admitted-looking answer to a
refused peer being a conversation the boundary already declined.

```graph
node: gate-agent-uid-denied-by-construction
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-agent-uid-denied-by-construction
```

**The lower closes the listener first and confirms after.** Stopped is
answered only after the close has returned, per the contract, so nothing
new can arrive anywhere in the interior once the harness proceeds. What
happens to an in-flight connection at lower is section 4's and is
chartered rather than deferred: the listener closes first, then every
accepted connection with whatever its buffer still held undelivered, per
charter section 13.3. **This clause read deferred with the token workflow
until 2026-09-15**, which the turn half had already settled and the
charter had already contradicted.

```graph
node: gate-stopped-follows-close
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-stopped-follows-close

edge: grounds
from: gate-stopped-follows-close
to: axiom-contract-is-a-complete-interface
```

**A refusal leaves nothing held.** A failed bind holds no listener and no
half-bound socket, so the aggregate's rollback has nothing of this crate's
to unwind, per charter section 5, and the refusal is answered rather than
exited on, a party that exited replacing a typed reason with an observed
death.

```graph
node: gate-refused-raise-holds-nothing
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-refused-raise-holds-nothing

edge: grounds
from: gate-refused-raise-holds-nothing
to: axiom-harness-integrates-by-the-loop
```

## 4. The relay, chartered

**This section closes what the cut deferred, the turn half arriving
2026-08-12 as act two of the first-live-turn epic.** What it charters is the
pass-through's mechanics and the client line's shape, against merged rails on
every side: `weaver-harness-gate-contract` section 2's exchanges, the world
contract's framing, charter section 13's protocol, and the frame election of
`weaver-types-Spec` section 4.1. `src/relay.rs` is its placement, filled by
the epic's wiring act.

**The service is one `poll` across three kinds of descriptor, serial as the
harness's own.** Between ready and stopped this crate waits against the
listener, every accepted client connection, and the channel end, and wakes on
the first ready. A listener wake accepts and judges the predicate, per
section 3. A connection wake reads octets. A channel wake reads the envelope,
a response frame to route out or the lower directive. Service is serial, one
wake handled at a time, and no executor enters: section 1's ground held
because the client traffic was deferred, and it holds now because `poll` and
a serial loop serve it, the same election `weaver-harness-Spec` section 2.4
carries for the same wait.

**The loop never blocks on a client, in either direction.** Accepted
connections are nonblocking end to end: a connection wake reads what is
there, and a response drains to its connection under the same poll,
writability being a wake like readability. A response a connection cannot
take yet waits in that connection's outbound buffer and in nothing else, so
a slow client holds its own responses and never the loop, the listener, or
the channel. A connection whose peer is gone fails its write, which is the
lost-delivery case of charter section 13.4, the record's close already
standing.

**One exchange is open per connection, and the cap is the flow control.** A
connection with an exchange open leaves the poll's read set until its
response returns, so a client's further lines wait where waiting is free,
in the socket's own buffer, back-pressure by the transport with nothing
refused and nothing dropped: a second request waits rather than being
refused, per the world contract, and one line in and one line out is the
resting shape realized per connection. One qualification keeps that sentence
honest. Bytes a read already delivered past the first delimiter wait in this
crate's input buffer rather than the socket's, at most one read's worth by
construction since the withdrawal follows the first opened exchange, the
undelimited-bound rule reading the residual like any bytes. When the
response returns, the scan resumes over the residual before the connection
re-enters the read set, so a line the residual already holds is served in
its turn and never skipped. The cap bounds everything the relay
holds. The open-exchange set carries at most one entry per connection, the
outbound buffer at most one response, and the channel send obeys the same
no-blocking rule, an envelope the channel cannot take yet waiting in this
crate and draining under the poll, at most one per connection by the same
cap. Pipelining within a connection would buy a client nothing, the harness
serving one turn at a time regardless, and its cost would be exactly the
unbounded state and the blockable send this cap refuses.

```graph
node: gate-one-exchange-open-per-connection
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-one-exchange-open-per-connection
```

**A lower closes in order, and stopped answers last.** The lower read from
the channel closes the listener first, then every accepted connection with
whatever its buffer still held undelivered, and answers stopped only after
the closes return, per charter section 13.3 and the ordering the lifecycle
half already pins. No turn is in flight at a lower, leave refusing while
one is, so what the closes drop is deliveries at most and never turns.

**A frame is bounded at the delimiter, and the scan reads nothing.** The
world contract fixes the framing as delimiter and octets, never fields, so
this crate scans an accepted connection's bytes for the delimiter and what
stands before it is the line. The scan is carriage rather than reading, a
byte comparison against the delimiter parsing no field, which is the same
distinction the frame election drew for the encoding. Bytes after a
delimiter wait for the next scan, order preserved per connection.

```graph
node: gate-frame-bounded-at-the-delimiter
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-frame-bounded-at-the-delimiter
```

**One line becomes one exchange, and the exchange's identity is the
routing.** A bounded line is encoded per the frame election into
`turn-frame`'s one member, the encoding riding with the type on the floor so
one implementation holds the canonical form for every party, and the frame
crosses as a carry-a-turn exchange this crate opens. The response frame
returns on the exchange, its member decodes to the response line's octets,
and the line goes out the connection its request came in on, delimiter
appended, per the correlation rule of charter section 13.1: the identity is
the channel's own and this crate mints nothing beside it. What this crate
holds is the set of its open exchanges, each the channel's identity paired
with the connection owed the response, made at the open and gone at the
response's write, at the connection's death, or at the lower, whichever
arrives first. That set is the retention rule enforced rather than
contradicted: an entry lives exactly as long as its exchange, and nothing
about the turn survives the response's write.

```graph
node: gate-one-exchange-per-line-by-identity
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-one-exchange-per-line-by-identity
```

**The line takes a bound, and the bound closes the connection.** The organ
envelope is bounded at 64 kibibytes, per `weaver-types-Spec` section 4, and
the frame's encoding inflates its octets by a third, so an unbounded line
would overrun the envelope on this crate's own send. **The election, flagged
for the operator: a client line is bounded at 32 kibibytes of octets before
the delimiter,** which encodes to under 44 kibibytes and leaves the envelope
its overhead with margin rather than arithmetic exactness. The bound is
inclusive: a line of exactly the bound's octets followed by its delimiter is
legal, and the connection closes when more than the bound stands undelimited,
however many reads delivered it. A connection past the bound has left the
protocol at the
framing layer, below any turn, so the connection closes: there is no line to
refuse and no turn to open, and a truncated relay would be the corpus-wide
truncation fault worn as a feature. The world contract carries the case in
its failure enumeration as of this act.

```graph
node: gate-line-bound-closes-the-connection
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-line-bound-closes-the-connection
```

**The client line's shape is this Spec's to fix, and it is fixed here.** The
world contract delegates the field list to this page, the client being built
against the format while the gate carries it unread: the shapes are stated
here because the client reads this page, never because this crate does. A
request is one JSON object with one required member, `text`, a string, the
turn's content from the client's side. Roles are not the client's to name:
the harness makes the text the turn's user message, per its Spec section
6.2, so a client cannot claim the system position or any other, which is the
injection this shape exists to refuse. An unknown member refuses the turn
rather than being ignored, the no-extension rule the corpus applies to its
own records applied at its mouth. A response is one JSON object whose `kind`
names the close: `answered` carrying `text`, the assistant's canonical
content, the record holding the verbatim beside it per `weaver-trace-PRD`
section 3.2. `stopped` carrying `reason`, the stop reason in place of a
response per the world contract. `refused` carrying `reason`, the answer a
line earns when it does not parse as the request this paragraph fixes. The
harness renders every one of them, per its guarantee in
`weaver-harness-gate-contract` section 4, and their Rust representation is
the wiring act's satellite, no floor type minted ahead of the loop entry's
signature.

What the merged contracts fixed before this act still binds: the relay reads
no content, and a line that does not parse is refused by the harness with
the refusal returning by the path the line took, the gate carrying both
directions unread. The claim binds the suite as much as the
build, which is why section 6 has no test that parses a client line, and it
is review's at both ends: no instrument distinguishes octets forwarded from
octets read.

```graph
node: gate-client-content-unread
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-client-content-unread

edge: grounds
from: gate-client-content-unread
to: axiom-harness-integrates-by-the-loop
```

## 5. The failure vocabulary

```rust
pub enum ChannelFault {
    Truncated { bound: usize },
    Undecodable,
    Closed,
}
```

**Three faults, the harness Spec's split adopted.** A refusal is a typed
answer on an exchange, every refusal this crate issues is a
`lifecycle-refusal` drawn from the floor, `BindFailed` above all, and a
channel fault is a failure below the exchange layer. `Closed` is the one
this crate does not survive: the exit path of section 2 is its handling,
and the other two are answered as faults of the exchange in flight.
**The split is what keeps the contract's refusal set closed,** which is the
grounding the block below carries: a refusal is drawn from the floor's
enumeration and named by the contract, so a failure that is not one of
those is classified below the exchange layer rather than admitted as a new
refusal the other party never agreed to read.

```graph
node: gate-fault-below-the-exchange-layer
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-fault-below-the-exchange-layer

edge: grounds
from: gate-fault-below-the-exchange-layer
to: axiom-contract-is-a-complete-interface
```

## 6. What is enforced, and by which instrument

Per apex section 11, the walks first.

**The reference walk: an elected tool dials the agent's own mouth.** The
adversary is the agent's tool surface running as the agent uid, the attack
a dial of the named socket the instruction declares, the prompt-yourself
loop the charter names. The mechanism is the predicate at accept with the
agent's own uid denied by construction, per section 3. The test is the one
`weaver-types-Spec` section 3 owed this document, landed: a connection
from the agent uid is refused at accept, before any content is read,
confirmed by watching content reach the harness when the predicate is
weakened. **The walk carries two records,** the peer authenticated at accept
and the agent uid denied by construction, both section 3's: a boundary that
authenticated the dialer and then admitted it would satisfy the first and
fail the walk, so the two are not one claim stated twice.

**The second walk: a same-uid process attaches to the gate.** The adversary
is the same tool surface, the attack a `ptrace` or `/proc/[pid]/fd` reach
into the one process of the worker's family that faces outward. The
mechanism is the dumpable flag cleared at entry, a set and not a check.
The test confirms the flag is clear after entry, watched to fail when the
set is removed.

**The third walk: the boundary outlives its interior.** The adversary is
timing, a client conversing with a gate whose harness is gone. The
mechanisms are the closure observation, exit on a closed channel, and the
elected parent-death signal backing it. The test closes the harness end of
a standing pair, the double staying alive, and confirms the gate exits and
the listener is gone, watched to fail when the closure handling is removed.
**The double has to live for the watch to mean anything.** Killing it as a
process ends the forking thread too, and the signal then exits the gate
whether or not the closure handling stands, which is a perturbation that can
never be seen to fail. **The test reaches the closure half and no further,**
the signal's backing being reachable by a double that forks from a thread
and exits it while its process lives, and simply not bought here. So this
walk's perturbation record is the closure mechanism and the signal's backing
is review's by election, at the section 2 clause that elects it, two records
rather than one for the reason the bind-site absence divides below.

**Enforced by the compiler.**

- The floor's three wire enums are exhaustive, so every directive, answer,
  and refusal case reaches this crate's matches loudly.
- Descriptors are owned types end to end.
- The channel state's three positions are a type, so a directive against a
  lowered hook is refused by a match arm rather than a flag check.

**Enforced by compile-fail tests.** One absence is this crate's own to pin:
`hook.rs` exposes one bind site taking the instruction's path, and a
doctest constructing a listener from a bare `&str` or `PathBuf` anywhere
else in the crate fails to compile, the two named shapes with the general
prohibition staying review's, per the floor Specs' split. **The split is two
assertions rather than one,** the pinned shapes here and the prohibition
itself at the section 3 clause that argues it: a single record tagged for the
mechanical half would claim the doctests for the whole, which is the
overclaim this corpus refuses in prose and has no reason to admit in a graph.
The load-bearing absence this crate relies on, `PeerIdentity` deriving no
`Deserialize`, is the floor's pin, per `weaver-types-Spec` section 3.

**Enforced by the manifest.** Four claims, all of them section 1's. One `[[bin]]` and no
organ anywhere in the resolved tree, which is the linkage half of the socket invariant
read off the manifest. The internal dependency is exactly `weaver-types` without the
`config` feature, read against the graph's floor links under gate H2, `weaver-traits`
having left with the tool boundary ruling of 2026-08-18. No async runtime, no logging
crate, and no config-format implementation, YAML or TOML, in the resolved tree, by the
build-time `cargo tree` assertion the floor Specs share. And the package has a lib
target **carrying `doctest = true`**, both read from `cargo metadata`, which is the
precondition of the compile-fail bullet above rather than a fact about the crate's
shape. The two are one instrument because either removal has the same effect: with no
target, and equally with the target kept and its doctests disabled, cargo collects no
doctest and the two pinned bind shapes go unenforced with this section still claiming
them.

**Which invariant each claim serves, and why twenty-two serve none.** Seventeen
`grounds` edges run from sixteen of the thirty-eight, nine to
`axiom-floor-is-vocabulary-behavior-is-socket`, five to
`axiom-contract-is-a-complete-interface`, and three to
`axiom-harness-integrates-by-the-loop`, with one claim carrying two edges because two
invariants each give it a reason. **The test applied is whether the axiom is the reason
the claim exists, or the claim a precondition of the axiom's own stated reason.** Remove
the socket invariant and this crate has no reason to authenticate at accept, no reason
to elect a socket type for the world seam, no reason to carry a truncation obligation,
and no reason to confine socket creation to one site, so those ground in it. Remove it
and the descriptors are still owned types and the parent-death signal is still elected,
so those ground in nothing. **Two claims this act first refused are grounded on the
operator's rulings of this date.** The dumpable flag holds a premise the socket
invariant argues from rather than a rule it states, which is the second grounding
relation Document Format section 4 now names. The channel state grounds in the contract
invariant because
`weaver-harness-gate-contract` section 3 states the ordering outright, that
lower is last and terminal and that turn exchanges are valid only between a
completed raise and a lower, so the claim was never the internal
representation that invariant excludes. **Nineteen claims grounding
in no invariant is the expected result and not a gap**, per Document Format
section 4: most of what this document elects is a representation, a
placement, or a hygiene set the charter's walks argue, and representation is
what the invariants are not about.

**The other two axioms take nothing from this crate.** The join key binds a
request belonging to an existing turn, and every exchange this document
specifies is a lifecycle directive belonging to none and carrying none, per
apex section 5.2. The turn exchanges that will carry a key are the token
workflow's and defer with the relay, so that edge lands when they are specced
rather than now. The organ test is a classification the charter passes, and
the one claim it would reach here, that this process holds a channel with the
harness and no other organ's handle, is `weaver-harness-Spec`'s to enforce at
the fork and carries no record in this document, per section 0.

**Three groups among the sixteen are worth stating rather than leaving to be
read.** Two of the four manifest claims ground in the socket invariant
because those two are the linkage facts that invariant defines: the gate is
a separate executable whose own resolved tree draws no organ, because its
behavior is reached over a socket - **the reverse half, that no workspace
crate links the gate, is review's per section 1**, a forward tree walk being
unable to establish it - and its one internal dependency is a floor link because the
floor is the two crates every domain draws from and no domain contains. The
other two ground in nothing and belong with the representations below, the
absent runtime being a thinness claim and the lib target being the
scaffolding an instrument stands on. The bind site's two
records ground there for a reason both halves share, the pinned shapes and
the general prohibition being halves of a divided claim: a listener built
anywhere but the one site is a socket the accept predicate never guards,
which is the exception that invariant states it does not admit. Four of the
five contract edges are the ordering half and the error half of a complete
interface, ready and stopped being answers the harness builds against
without asking what this crate does, out of order being an ordering
guarantee refused rather than queued, and the fault split being what keeps
the refusal vocabulary drawn from the floor rather than invented here.

**The three loop edges are the places this crate hands work back to the
integrator rather than settling it here.** Apex section 5.5 makes the harness
answerable for correctness and for timing across domains and leaves each organ
answerable inside its own, so a claim grounds in it when the claim is this crate
declining what the loop is answerable for. A directive out of order arrived in an
order this crate did not choose, and a gate that queued it would be reconciling a
timing failure across domains, which that invariant assigns to the loop by
construction. **That record carries two edges and both are real.** Apex section
5.3 requires the contract to state the ordering it relies on, which is why the
refusal is specifiable at all, and apex section 5.5 makes the loop answerable for
that ordering still holding when this crate's ordering meets the SPU's in one
fan-out. A refused raise holding nothing is what lets the fan-out's rollback stay
the loop's work, a half-bound listener left behind being state the harness cannot
see and would have to be told about, which is the organ-to-organ arrangement that
invariant exists to prevent. The relay carrying both directions unread draws the
same line at the world seam: a gate that read a client's line would be deciding
something about the turn, which is the harness's domain, and the refusal of an
unparseable line returning from the harness by the path the line took is that
decision staying where it belongs.

**What the loop invariant did not reach, and why the line falls there.** Ready
follows the bind and stopped follows the close are this crate's own sequencing
inside its own domain, presented at its edge as guarantees the contract states,
and apex section 5.5 says nothing about what happens inside a domain. The channel
state's three positions are the representation that makes the refusal mechanical
rather than the refusal itself, and grounding a representation in an invariant
about what crosses would read the scope limit backwards. The one claim that
invariant's presents-nothing-to-any-peer clause would reach here, that this
process holds a channel with the harness and no other organ's handle, is the same
claim the organ axiom would reach and is `weaver-harness-Spec`'s to enforce at the
fork, carrying no record in this document, per section 0.

**Two calls the labelling first refused, and why.** The agent uid denied by
construction is this crate's centerpiece and it grounds in nothing. The
socket invariant protects the second party knowing who the first is, which
the credential at accept discharges in full, and what the boundary then does
with that knowledge is the charter's reference walk rather than the
invariant's. **The two hygiene sets do not go the same way, and the sentence this act
first wrote here answered for the wrong seam.** This process holds two seams of
different kinds. The world socket is named and its identity property does stand
or fall with the credential. The channel end at descriptor 3 is an unnamed pair
with no credential at all, authenticated by possession, and the socket invariant
rests that case on a premise rather than a rule: no third party can reach a
socket that has no address. A same-uid process reaches it through this process's
own descriptor table unless the flag is cleared, so clearing it holds the premise
true and grounds on the second relation. The channel end's close-on-exec is review's by
election, per section 6, and grounds on the same footing, an exec'd image holding the
end being a third party in possession of a socket with no address.

**Where the assertion records sit, and which of these bullets another crate
declares.** The records are at the clauses that argue the claims rather than
gathered here, per Document Format section 6: this section sorts by
instrument and the arguments are elsewhere, so a block here would sit apart
from the prose that earns it. One record is the exception and sits at the end
of this section, the doctest pinning of the two bind-site shapes, whose
argument is nowhere else and whose general prohibition is section 3's.

**Thirty-eight records in all, and the split is where they are declared,
which a reader can count.** Four in section 1, eight in section 2, eleven in
section 3, five in section 4, one in section 5, eight in section 8, and this
section's one. **Every one of the thirty-eight is sorted above** under the
instrument that holds it, and the instruments divide them four to the
manifest, two to the compiler, one to compile-fail, twenty-two to a
perturbation-verified test, and nine to review. The three walks carry five of
those across two instruments, four perturbations and the parent-death
signal's backing, which is why the walks are not a sixth column. **The
earlier form of this sentence gave the
split as an unmeasurable one** - so many from this section's sorting and the
rest from the elections outside it - and no reading of the section reproduced
the figure it gave, which is how it stood wrong from the day it was written
without any act noticing. The records the tool boundary ruling landed in
section 8 and the two the mode election landed in section 3 were sorted here
nowhere at all, which is the substance behind the arithmetic. **A split
stated as a count of declarations is one the next act can check by counting
declarations**, and it is also why the scope clause above no longer says
sections 1 through 5, six records sitting outside them.

**Two of the bullets above are claims another crate argues,** and carry no
record here: the floor's three exhaustive wire enums and `PeerIdentity`'s
missing `Deserialize`, both `weaver-types-Spec`'s. An assertion belongs where
its argument and its instrument live, and a node declared twice is the defect
that format forbids.

**Requiring a perturbation-verified test, beyond the walks.**

- Ready follows the bind: the answer is sent only after bind and listen
  return, confirmed by watching a client's dial succeed against an
  unconfirmed raise when the ordering is reversed.
- Stopped follows the close: confirmed by watching a dial succeed after a
  stopped answer when the ordering is reversed.
- A refused raise holds nothing: after a `BindFailed`, no listener exists
  and no socket file was created by this crate, confirmed by watching a
  leaked listener appear when the cleanup-on-refusal is removed.
- Truncation is a fault: an over-bound envelope produces `Truncated` and
  no directive, confirmed by watching a silently shortened directive
  decode when the `MSG_TRUNC` check is removed.
- A directive out of order is refused and not queued: a lower arriving
  before any raise answers `OutOfOrder` with no listener standing after
  it, and a directive of any kind arriving after a lower answers the
  same, the lowered position being terminal. The compiler bullet above
  pins that the refusal reaches a match arm rather than a flag check,
  and this pins what the arm then does, an arm being free to queue or to
  answer the wrong refusal while compiling exactly as well. Confirmed
  twice, once per case, by watching an early lower drive the channel to
  the terminal position when the before-raise arm stops refusing it,
  after which a legitimate raise is refused and no listener ever stood,
  and by watching a lowered channel accept a second raise when the
  terminal arm is collapsed into the raised one. One watch would leave
  whichever case it misses claimed and unenforced, which is the reading
  this bullet exists to refuse. The refusal is owed to each organ by
  `weaver-types-Spec` section 5, which states it and enforces it nowhere,
  and this discharges the gate's side of that owing alone.
- Framing is at the delimiter: two lines arriving in one write open two
  exchanges in order, confirmed by watching them fuse into one when the
  scan is removed, and the scan compares bytes against the delimiter and
  parses nothing, which review holds.
- One line is one exchange and the identity routes: two clients speaking at
  once each receive their own response on their own connection, confirmed
  by watching a response cross connections when the exchange identity stops
  carrying the routing.
- One exchange is open per connection: a second line arriving while one is
  open waits unread in the socket, confirmed by watching two exchanges
  stand for one connection when the read-set withdrawal is removed.
- The line bound closes the connection, inclusively: a line of exactly the
  bound's octets followed by its delimiter opens its exchange, a connection
  fed one octet more with no delimiter is closed with no exchange opened,
  and both are confirmed by watching an over-bound line reach the channel
  when the bound check is removed and a bound-exact line die when the check
  reads exclusive.
- The umask lock is reentrant within a thread: the raise holds a umask
  across the bind, so a caller that sets one around this crate's use takes a
  non-reentrant mutex twice on one thread and hangs. The watch runs the
  nested raise on its own thread against a timeout, per section 3, a
  deadlock being the one failure a test cannot observe from inside.
- The socket mode is the boundary's election: `0o770` is elected in the
  creating call rather than inherited, confirmed by watching the umask guard
  removed and the socket bind at the runner's own umask, per section 3.
- The shell execution's seven watches are section 8's and are argued there:
  the one held tool, the one clock, the group kill, the drain riding the
  run, the cancel bringing the clock forward, the late cancel dropped, and
  the exit heard within a slice, each with the removal that makes it fail
  named at the clause. They
  are named here because this sorting reaches every record and not only the
  records that predate the tool boundary ruling of 2026-08-18, and the
  argument stays where the instrument is rather than being restated, per
  gate G5.
- The unparseable-line refusal is the harness's, per its Spec section 6.2 as
  of the turn-half act: no test here parses a client line, and a test that
  did would be the opacity rule breached by the suite, which review checks
  for.

**Held by review, each by an election argued at its own clause.** The
channel end's close-on-exec and the parent-death signal's backing, both
section 2's and both discussed above. The instruction's two consumed
fields, the client socket's type, this crate unlinking nothing, and the
general bind-site prohibition, all section 3's. The relay reading no client
content, section 4's. The fault sitting below the exchange layer, section
5's. The execution's four contents, section 8's, where the tag records that
no watch was bought for the enumeration - **if the compiler held it the tag
would be `compile-pin`**, and which of the two it is belongs with the audit
named below. **Nine in all, and the tag means the instrument was not bought
and never that none exists**, per apex section 11.
**The walk behind that reading is over file-level `conforms:` headers alone**
and finds all nine cited from `src/`. It under-reads in two ways and the
clearance is weaker than it sounds by both. It does not reach the in-file
`#[cfg(test)]` suites `channel.rs`, `relay.rs` and `tools.rs` carry, and it
does not reach a citation written at an item or at a statement, which the
Document Format's section 4 admits beside the file header. So it is not a
clearance against the inverse overclaim and is not offered as one:
`gate-execution-four-contents` is cited from `tools.rs`, whose unit suite
holds a test per content. **Auditing the nine is not `act-34`'s**, that act
having landed the format rule in Document Format section 5 and taken no
retag, and all nine of these sit outside the rule's scope in any case,
cited from `src/` file headers rather than at a test. The audit is named as
owed in issue #569's `act-34` entry and falls to the next act that opens
this document.

```graph
node: gate-bind-shapes-pinned-by-doctest
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-gate
to: gate-bind-shapes-pinned-by-doctest

edge: grounds
from: gate-bind-shapes-pinned-by-doctest
to: axiom-floor-is-vocabulary-behavior-is-socket
```

## 7. Open elections

Each names what settles it, and none is this Spec's to settle alone.

- **The `gate-instruction` field list.** A satellite of
  `weaver-types-Spec`, with this document's demand stated in section 3:
  the socket path and the access rule. The demand is recorded here so the
  satellite is shaped against a consumer rather than invented.
- **The token workflow's half arrived, 2026-08-12, and this entry is the
  register of what still defers.** The turn exchanges, the relay's
  interior, the line bound, concurrency against the one-turn loop, and
  drain on stop are chartered at section 4 and charter section 13.
  **Charter section 8 defers three:** streaming responses with the partial
  output they carry, the connection the hook holds open while the interior
  generates, and the backpressure that rides them, all arriving with the
  token workflow's extensions to the world contract's section 3 at the
  memory round's architecture pass. **Cancellation of an execution no
  longer defers**: it landed at section 8 and at the contract's section 2
  on the operator's ruling of 2026-09-22, after the W1c characterization of
  #646 measured a stop waiting the whole invocation out, and the charter
  carries it at section 13.5. Until that act the charter named it in its
  section 0 list alone, in neither section 8 nor section 10, a gap this
  entry carried so it would not read as an omission here. Cancellation of a
  generation was never this crate's, landing at the decode seam per
  `weaver-harness-Spec` section 6.1. Recorded as narrowed rather than
  deleted, this list naming what settled each entry.
- **The tool-uid ruling.** Charter section 7's pending candidate, settled
  by the architecture seat's ratification or the tool workflow's threat
  measurement, and nothing here builds against the separate-uid arm.
- **The satellite types.** `ChannelFault`'s spelling against the harness
  Spec's identical enum, one shared shape in two crates being tolerable
  where a shared crate would be a dependency taken for a name, and the
  channel-state enum's name. Choices with no cross-crate consequence,
  listed so what this Spec leaves to a builder is complete.

## 8. The shell execution

Landed by the tool boundary ruling of 2026-08-18, against the amended
`weaver-harness-gate-contract` section 2 and the charter's amended election.
This crate holds one tool, the shell, and holds it as its own verb rather
than as a table's member: the execution exchange resolves against that one
name directly, and a name that is not the shell's refuses by name, never a
nearest match.

**The invocation forks the shell in its own process group and supervises it
to the caller's clock.** The caller's timeout crossed the exchange, already
validated against the declared maximum at the refusal layer, and it is the
one kill clock. Past it the kill reaches the whole group, because `bash`
leaves descendants holding the pipes' write ends, and a kill that reaped
only the leader would hold the answer open on a straggler's schedule. The
exit is observed unreaped before the group is signaled, so the group id
cannot be reissued between the observation and the kill.

**The shell starts in the uid's home with `HOME` and no other variable.** This
crate runs with the agent's environment, per `weaver-harness-Spec` section 2.2
and the operator's ruling of 2026-10-01, and that ruling reaches the organs and
not the commands a tool runs, so the invocation clears the environment and sets
`HOME` alone, the surface the shell had when this crate itself started empty.
Whether a command should see the agent's environment is a ruling of its own.

**Both pipes drain concurrently with the run, bounded, and the drain
continues past the bound.** A pipe left unread to the exit fills at the
kernel's buffer and blocks the child's writes, converting a chatty command
into a false kill. The capture keeps the bound and discards the rest, and
what was discarded is marked in the answer.

**The answer is one of the contract's four contents and the account's
speaker is the tag's meaning.** A nonzero exit is a result, the shell's own
answer accounted in content. A refusal is this crate's voice and nothing
ran. An error is the machinery's. A kill carries no account from the tool by
construction, with any drained partial riding as an attachment and never as
a result.

**The supervisor hears the seam while it waits, and a cancel is the caller's
clock brought forward, per the contract's section 2 as amended 2026-09-22.**
The supervision wait is a loop of bounded slices, each an unreaped exit
check and then a sleep of at most the slice or the clock's remainder,
whichever is shorter, and the change is to the sleep alone: it becomes a
`poll` on the channel end bounded the same way, so a cancel wakes the slice
early and an exit is seen at the next check as it is today. The exit does
not wake the poll, a child's death making no descriptor of this crate's
readable, which is why the slice stays and does not stretch to the clock: a
supervisor that polled the channel for the whole remainder would hold a
command that exited in its first millisecond until the caller's clock ran
out. On a cancel the group is
signaled at once by the same path every ending takes, the drain finishes
bounded as it does at the clock, and the answer is a kill naming the cancel
as its cause, the partial riding as it rides at the clock. Nothing the
contract permits arrives on the channel during an execution but the cancel
and closure, executions being serial and the harness blocked in its wait, so
an envelope there is the cancel or it is the closure the charter already
requires this crate to exit on, and closure breaks the same loop, which
kills the group before the exit as every ending does. The cancel adds no
clock: the caller's number still bounds the wait, and a supervisor that
never reads the cancel still answers by it.

**A cancel that arrives past the answer is dropped, never refused.** The
exchange it names closed from this side when the answer was sent, and the
two directions crossing is the channel's ordinary case, so the envelope is
read and discarded and the next open is served as if nothing arrived. A
refusal in its place would put an envelope on the channel the harness has no
exchange to correlate, which is the stray answer the contract's ordering rule
exists to prevent. The out-of-order refusal stays what it is for every other
misplaced envelope, and this is the one continue-position envelope this
crate reads.

```graph
node: gate-shell-the-one-held-tool
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-shell-the-one-held-tool

node: gate-execution-one-clock
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-execution-one-clock

node: gate-execution-group-kill
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-execution-group-kill

node: gate-execution-drain-rides-the-run
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-execution-drain-rides-the-run

node: gate-execution-four-contents
kind: assertion
tag: review

edge: asserts
from: weaver-gate
to: gate-execution-four-contents

node: gate-execution-cancel-brings-the-clock-forward
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-execution-cancel-brings-the-clock-forward

node: gate-cancel-past-the-answer-is-dropped
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-cancel-past-the-answer-is-dropped

node: gate-execution-exit-heard-within-a-slice
kind: assertion
tag: perturbation

edge: asserts
from: weaver-gate
to: gate-execution-exit-heard-within-a-slice
```

The perturbation obligations are apex section 11's: the unheld-name watch
fails when the refusal arm is removed, the clock watch fails when the kill
branch is removed, the group-kill watch fails when the group signal is
removed and a straggler holds the pipes, and the drain watch fails when the
readers wait for the exit. The cancel watch cancels a sleep far inside a
long clock and fails when the channel end leaves the supervision wait, the
answer then arriving at the clock. The late-cancel watch sends a cancel
after an answer and opens a second execution, and fails when the cancel
takes the out-of-order arm, a refusal then standing where the second answer
is read. The slice watch runs a command that exits at once under a clock of
seconds, sends nothing, and asserts the answer inside a few slices, failing
when the poll's bound stretches to the clock's remainder, the answer then
arriving at the clock with the tool long dead. The chatty-command watch of
the drain claim bounds the same regression loosely, at half the clock, and
the slice watch is the one that names it. The three were authored ahead of
their code on 2026-09-22, the slice claim on the review seat's finding that
a channel poll is deaf to an exit, and stand uncited until the act that buys
them cites them, per Working Process section 6. The four-contents claim is
review's because the enumeration is a shape fact the compiler holds once
the answer type carries the cases.
