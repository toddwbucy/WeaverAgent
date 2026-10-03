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
**Landing PR:** #63

---

## 0. What this document is

The agreement over the trace's exit and, since the operator's rulings of 2026-10-02
on #50, over the two sockets a caller holding a role reaches the agent's admin by: how
the program's one output leaves it, where it lands, what such a caller may send and
receive, and what either side may rely on at that boundary. It is read alongside
`weaver-admin-PRD`, whose section 8 names both doors: the operator's shell, which runs
the crate and crosses no channel this document governs, and the sockets, which do.

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
`weaver-admin-PRD` section 7: at the shell a human holding root, and at the sockets any
caller acting for that person through a group the agent's role list grants, a connector
of WeaverWeb's among them. The graph carries no node for a principal outside the
program, so the party is named in prose and the missing category is registered rather
than improvised.

## 1. The boundary

**This contract binds three crossings: the stream's sink and the agent's two admin
sockets.** At the shell the operator's asks reach the program by an invocation the
operating system already governs, per `weaver-admin-PRD` section 8, and an executed
program is no seam by the Working Process test. **The two sockets of 2026-10-02 are real
crossings** and this contract governs them: per agent, at
`/run/weaver-<agent>/admin.sock` and `/run/weaver-<agent>/trace.sock`, held by the init
system, root-owned, mode `0660` and grouped to the agent's own access group
`weaver-<agent>-admin`, never shared across agents, each admitting by the kernel's peer
credential against the agent's `roles.toml` before a request is read (the operator's
rulings of 2026-10-03 on #63). The sink is the third: **admin passes the sink handle
across the boundary and the program writes through it**, something of the operator's
stands behind it, and neither side sees the other's interior. It is not network ingress
and breaches nothing Gate holds, on the grounds `weaver-admin-PRD` section 3 states.

## 2. What crosses in

**At the lifecycle socket, one request line**, on the operator's ruling of 2026-10-02 on
#50: a JSON object naming one verb, and optionally the person the caller acts for, the
shape `LifecycleRequest` of `weaver-types-Spec` section 3.1. The verbs are the command
line's, `show`, `grants`, `validate`, `load`, `unload` and `stop`, admitted per the
peer's roles: observer holds `show` and `grants`, and operator adds the four that act.
The agent is never named, the socket being one agent's. **The claimed person is a claim
and never an authorization input**, per #51: admin logs it beside the verb and nothing
that decides reads it.

**At the trace socket, one request line**: a byte offset on a record boundary and the
digest of the record before it, the shape `TraceRequest` of the same section, from the
one `trace-reader` the agent's `roles.toml` declares, the agent's own admin-con. Every
other caller is refused and logged, and the newest connection from the reader replaces
the old.

At the shell nothing crosses here: the verbs arrive as an invocation's arguments. No
prompt, turn, task, or run crosses either door, per apex section 6, and the record is
one-way by construction, so what an operator decides after reading it re-enters by a
verb rather than by answering on this boundary.

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

**At the lifecycle socket, one answer line**: the same `lifecycle-answer` or
`lifecycle-refusal` object the command line writes, a verb outside the peer's roles
answering `Unauthorized` before anything is touched. `grants` answers the verbs the
peer's roles permit and the time of the observation. A connection the caller drops does
not cancel its verb, which runs to completion and is readable afterwards through `show`
and the record.

**At the trace socket, the record as a read-only stream**: admin, started for the
connection, opens the trace file read-only without following links, verifies the
requested position, and writes a header line naming the file's identity (device, inode,
birth time), then the trace's own lines exactly as written, following new lines with a
heartbeat while idle, and it reports a change of the file's identity rather than
smoothing a rotation or a truncation over. A position that does not verify refuses
before a byte is sent. A caller reading the record this way reads the file through no
grant of its own, so the territory's layout gives it nothing.

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

**At the sockets, the boundary's own failures, each refused before anything is touched
and logged in `admin.log`**, per the operator's rulings of 2026-10-02 on #50 and
2026-10-03 on #63. A listening socket whose owner, group or mode is wrong serves
nothing. A peer none of whose groups holds a role on the agent is refused before its
request is read, and so is any caller but the declared trace reader at the trace socket.
A missing or malformed `roles.toml` refuses as `ConfigInvalid` naming it. A verb outside
the peer's roles is answered `Unauthorized`, the floor's `lifecycle-refusal`. A trace
position that does not verify refuses before a byte is streamed, and a request against a
sink that is not a file refuses `ConfigInvalid` naming the sink.

The ask-side cases this section enumerated until 2026-08-05 travelled with the socket to
`weaver-admin-PRD` section 8 and its Spec: the malformed request, the unknown agent, the
request carrying work, and the config's registered-field failure are all still refusals,
typed as `lifecycle-refusal` and returned by the invocation or the lifecycle socket
alike. The shared allow-and-deny predicate is not among them, the sockets judging a role
list rather than that rule. An organ refusing a field it registered is still not on any
of these lists: that refusal is the organ's, travels back through the harness on its own
seam, and reaches the operator inside the aggregate.

## 6. Prohibitions

**On admin.** It carries no work inward, however an invocation frames it. It emits the
stream to the declared sink, and serves the sink's file read-only through the trace
socket to its one declared reader, and to no other reader, and it repairs, reconciles,
and adjudicates nothing on the way through, per `weaver-admin-PRD` section 2. At either
socket it reads no request before the peer is admitted, authorizes nothing on the
claimed person, and does nothing with root on the trace socket but read the file.

**On the operator's tooling.** Nothing behind the sink reaches back. The stream is
one-way, and tooling that wants to act on what it reads comes back by running a verb,
at the shell or at the lifecycle socket under a granted role. The monitoring is the
outside's job and the verb is admin's, per the basic loop's section 2.

**On a caller at the sockets.** It acts within the verbs `grants` answers and asserts no
identity the kernel did not supply. The declared trace reader reads the record through
the trace socket rather than through any grant into the territory, and fans it out, if
at all, on WeaverWeb's side and never the box's.

## 7. Vocabulary

**Drawn from `weaver-types`:** `lifecycle-refusal`, and since 2026-10-02 (#50) the
sockets' shapes of `weaver-types-Spec` section 3.1, the lifecycle request, the trace
request and the `grants` answer, with `lifecycle-answer` that the lifecycle socket
writes and `peer-identity` that it reads from the kernel. `authorization-predicate`
stays undrawn: a role is a group granted a set of verbs, judged against the role list,
and not the allow-and-deny rule the gate's client boundary and the coordination seam
share.

**Drawn from `weaver-trace`:** the durable event schema, as the content of the
output stream. It is drawn as published format rather than as a linked type, which
is the contract-coupled reading `weaver-admin-PRD` section 8 states.

**Drawn from `weaver-traits`:** nothing. The clause is present with that answer
because `weaver-types-PRD` section 5 asks for it even when it is empty.

The directive draws left this clause with the asks at the recut and stay out: at the
shell the verbs enter as an invocation's arguments, and at the lifecycle socket as the
request shape above rather than as directive cases.
The refusal stays drawn, because a sink that cannot be opened refuses a load and that
refusal reaches the operator as the floor's own type. Nothing is owed to the floor.

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
- The admin sockets of 2026-10-02 (#50), landing in one act: `weaver-admin-PRD` sections
  7 and 8, `weaver-admin-Spec` sections 2, 5, 10 and 11, `weaver-admin-systemd-contract`
  (the socket and service units), and `weaver-types-Spec` section 3.1 (the request
  shapes and the `grants` answer). `weaver-types-PRD` section 2.2 names admin's sockets
  as readers of `peer-identity` that draw no predicate, landed in the same act.
  WeaverWeb's admin-con moves its tailer from the file to the trace socket, an interface
  change its seat agrees to on toddwbucy/WeaverWeb#12.
