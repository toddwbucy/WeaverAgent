# weaver-admin / operator - contract

**Status:** MERGED. In `main` and the source of truth. Written on the human's
ruling of 2026-08-01: the two external boundaries take contracts before any Spec is
written, this document and `weaver-gate-world-contract` in one act, and both are
blockers on the Spec phase. One party is an external principal rather than a crate,
which is the seam category `weaver-admin-PRD` section 10 held open, settled by that
same ruling.

**Date filed:** 2026-08-01
**Document ID:** `weaver-admin-operator-contract`
**Parent:** `weaver-agents-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #300

---

## 0. What this document is

The agreement over the trace's exit: how the program's one output leaves it, where it
lands, and what either side may rely on at that boundary. It is read alongside
`weaver-admin-PRD`, whose section 8 names the interface the operator's asks now enter
by, which is running the crate rather than a channel this document governs.

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

The second party is the operator, a human role holding root on the machine, per
`weaver-admin-PRD` section 7. The graph carries no node for a principal outside the
program, so the party is named in prose and the missing category is registered rather
than improvised.

## 1. The boundary

**The stream's sink is where this contract binds, and it is the one crossing left.**
The operator's asks reach the program by an invocation the operating system already
governs, per `weaver-admin-PRD` section 8, and an executed program is no seam by the
Working Process test, so the socket this section carried until 2026-08-05 has no
subject. What remains is a real crossing: **admin passes the sink handle across the
boundary
and the program writes through it**, something of the operator's stands behind it, and
neither side sees the other's interior. It is
not network ingress and breaches nothing Gate holds, on the grounds
`weaver-admin-PRD` section 3 states.

## 2. What crosses in

**Nothing.** The ask set this section enumerated until 2026-08-05, the three verbs,
the stop conveyance, and the two observations, still exists and no longer crosses
here: it arrives as an invocation's arguments, per `weaver-admin-PRD` section 8, and
its shape is that charter's and its Spec's rather than this contract's. No prompt,
turn, task, or run crosses, per apex section 6, and the stream is one-way by
construction, so what an operator decides after reading it re-enters by running a
verb rather than by answering on this boundary.

**The section is kept with that answer rather than deleted,** because a contract
states what crosses in even when the answer is nothing, and because the numbering of
what follows is cited across the corpus.

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

**This boundary has one failure and it is the sink's.** A sink that cannot be opened
refuses the load, per `weaver-admin-PRD` section 4.1 step 4, and a sink that fails
mid-run is the tee's bounded loss of section 3 rather than a refusal, because a run
does not stop for its reader.

The ask-side cases this section enumerated until 2026-08-05 travelled with the socket
to `weaver-admin-PRD` section 8 and its Spec: the malformed request, the unknown
agent, the request carrying work, and the config's registered-field failure are all
still refusals, typed as `lifecycle-refusal` and returned by the invocation. The peer
predicate is not among them anywhere, having retired with the surface that applied
it. An organ refusing a field it registered is still not on any of these lists: that
refusal is the organ's, travels back through the harness on its own seam, and reaches
the operator inside the aggregate.

## 6. Prohibitions

**On admin.** It carries no work inward, however an invocation frames it. It emits
the stream to the declared sink and to no other reader, and it repairs, reconciles,
and adjudicates nothing on the way through, per `weaver-admin-PRD` section 2.

**On the operator's tooling.** Nothing behind the sink reaches back. The stream is
one-way, and tooling that wants to act on what it reads comes back by running a verb.
The monitoring is the outside's job and the verb is admin's, per the basic loop's
section 2.

## 7. Vocabulary

**Drawn from `weaver-types`:** `lifecycle-refusal`, and nothing else as of
2026-08-05. `peer-identity` and `authorization-predicate` left this clause with the
socket that read them, and the consequence for `weaver-types-PRD` section 2.2 is
named in section 8: that pair now finds its consumers at the gate's client boundary
and at the coordination seam, and not here.

**Drawn from `weaver-trace`:** the durable event schema, as the content of the
output stream. It is drawn as published format rather than as a linked type, which
is the contract-coupled reading `weaver-admin-PRD` section 8 states.

**Drawn from `weaver-traits`:** nothing. The clause is present with that answer
because `weaver-types-PRD` section 5 asks for it even when it is empty.

The directive and answer draws left this clause with the asks, per the recut: the
verbs and the stop are directive cases still, and they enter as an invocation's
arguments where `weaver-admin-PRD` section 8 governs them rather than crossing here.
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
