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
interior. **The trace door** is the relay's socket, `trace.sock` in the agent's
root-owned run directory `<coordination-root>/weaver-run/<agent>/`, standing only for a
file sink, grouped to the agent's own access group `weaver-<agent>-admin`
and admitting exactly one declared reader by the kernel's peer credential, per
`weaver-admin-Spec` section 6. None is network ingress, and none breaches anything Gate
holds, on the grounds `weaver-admin-PRD` section 3 states.

## 2. What crosses in

**A granted command line, and nothing else from the caller.** The verbs are `show`,
`validate`, `load`, `unload` and `stop`, each with the agent's name, on the operator's
rulings of 2026-10-03 on #50. Two more, `save-point` and `restore` against a running
agent, are owed to #58's code act (A3), which shapes their exchange, and no rule grants
them until it lands.
**Which lines a caller may run is the rule the operator installs**: an observer's rule
grants `show`, and an operator's adds `validate`, `load`, `unload` and `stop`, the role
split of #50 mapped onto command lines. Nothing else crosses in:
no argument the caller chooses, no standard input, which the program never reads, and no
claim of who asked. **The cause the record carries is the uid sudo reports**, and which
person asked is WeaverWeb's record and never the agent's. No prompt, turn, task, or run
crosses, per apex section 6.

**At the trace door, one request line**: a byte offset on a record boundary and the
digest of the record before it, the shape `TraceRequest` of `weaver-types-Spec` section
3.1, from the one `trace-reader` the agent's boundary file declares, the agent's own
admin-con. Every other caller is refused and logged, and the newest connection from the
reader replaces the old.

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
handle. The mechanism is the Spec's.

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
next `show`. Standard error carries human-readable diagnostics no caller parses. A
`load` answers once the agent is up or refused, within admin's own bound of 900 seconds
by default, which a caller's bound must exceed. **An invocation finishes even when its
caller disappears**, its outcome recorded in the agent's `admin.log`, so a caller that
gives up reads the outcome from the next `show`.

**At the trace door, the record as a read-only stream**: the relay writes a header line
naming the file's identity (device, inode, birth time), then the trace's own lines from
the verified position exactly as written, following new lines with a heartbeat while
idle, every added line a `TraceControl` of `weaver-types-Spec` section 3.1. It serves
the loaded run's own file by descriptor, tells the reader when the sink's path comes to
name another file and goes on serving the run's own, ends the stream on a truncation
rather than smoothing it over, and refuses a position that does not verify
before a byte is sent. A reader of the record this way reads the file through no grant
of its own, so the territory's layout gives it nothing.

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

## 5. Failure

**At the sink, one failure.** A sink that cannot be opened refuses the load, per
`weaver-admin-PRD` section 4.1 step 4, and a sink that fails mid-run is the tee's
bounded loss of section 3 rather than a refusal, because a run does not stop for its
reader.

**At a command line, the floor's refusals**: a second `load` of a running agent answers
`AgentRunning`, a missing or malformed boundary file refuses `ConfigInvalid` naming
`roles.toml`, and an `unload` whose worker still holds the run lock after the escalation
refuses with that carried and no state, per `weaver-admin-Spec` section 3. A line a
caller's rule does not grant never reaches the program, sudo refusing it, so it has no
refusal here. **At the trace door, refusals before a byte is sent**: any caller but the
declared reader, and a position that does not verify, each logged in the agent's
`admin.log`. Where the sink is a pipe or a socket no relay stands and no socket is
bound, so the door is closed, and a relay that has died closes it until the next load,
both of which a reader sees as a refused connection.

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
command line prints, and since 2026-10-03 (#50) the trace door's shapes of
`weaver-types-Spec` section 3.1, `TraceRequest`, `TraceHeader` and `TraceControl`, with
`peer-identity`, which the relay reads from the kernel. `authorization-predicate` stays
undrawn: the relay judges one declared reader, not the allow-and-deny rule the gate's
client boundary and the coordination seam share.

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
- The agent leaving systemd, 2026-10-03 (#50), landing in one act.
  `weaver-admin-systemd-contract` retires. `weaver-admin-PRD` sections 1, 2, 4, 5, 6, 7,
  8 and 10 and `weaver-admin-Spec` sections 2, 3, 6, 8, 9, 10 and 11 move to the start
  step, the run lock, the sudo rule and the trace relay. `weaver-types-Spec` section 3.1
  carries the trace door's shapes and the cause, and `weaver-harness-Spec` section 2
  the worker's ownership of its organs. WeaverWeb's admin-con maps its verbs to the
  granted lines and reads the trace door, toddwbucy/WeaverWeb#12.
