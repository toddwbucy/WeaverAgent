# weaver-gate / world - contract

**Status:** MERGED. In `main` and the source of truth. Written on the human's
ruling of 2026-08-01: the two external boundaries take contracts before any Spec is
written, this document and `weaver-admin-operator-contract` in one act, and both
are blockers on the Spec phase. One party is an external principal rather than a
crate, the second instance of the category that ruling settles.

**Date filed:** 2026-08-01
**Document ID:** `weaver-gate-world-contract`
**Parent:** `weaver-agents-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #614

---

## 0. What this document is

The agreement over the client socket: how the world reaches a loaded agent and how
the agent's responses return. It governs the boundary `weaver-gate-PRD` section 2
names, and it is read alongside that charter and `weaver-harness-gate-contract`.

Three parties act at this boundary and only two sign. The client speaks, the gate
admits and relays, and the harness interprets. The gate stays opaque per its charter,
so the framing of section 2 is an agreement between the client and the harness that
the gate carries without reading, delimiter and octets, never fields. The harness is
bound to this page through `weaver-harness-gate-contract` when the token workflow
charters the turn exchanges, and this document does not restate that seam.

**The filename carries the `-contract` suffix like every contract,** per
`weaver-admin-operator-contract` section 0, whose naming correction of 2026-08-01
this document shares.

```graph
node: weaver-gate-world-contract
kind: document

edge: party
from: weaver-gate-world-contract
to: weaver-gate
```

The second party is the world: a local client principal admitted by the boundary
predicate. The graph carries no node for a principal outside the program, so the
party is named in prose and the missing category rides
`weaver-admin-operator-contract`'s register entry rather than taking a twin.

**So no seam record names this document as its `via`, and that is the party list
above read out rather than a gap in it.** Stated here as of 2026-09-16, because
`weaver-web-PRD` carried such a record until that date and a later reader with the
same client in hand would write it again. A seam is declared from the declaring
side and its `via` names the contract governing the pair, so a record naming this
page asserts that the crate at its far end signed it. None did, the far end of
this boundary being the world. A client is built against this page and nothing
else, per section 2, which is the same fact from the client's side: the page owes
it an interface and the page binds it to nothing. The category a client falls in
is the world, and a crate that happens to be a client is an instance of that
category rather than a party alongside it.

**What the deleted record was reaching for is real and is stated where it belongs.**
`weaver-gate-PRD` section 2 has the world dialing and this crate binding and
accepting, and `weaver-web-Spec` section 1 (`git show
112bc65:docs/crates/weaver-web/weaver-web-Spec.md`, in the web's own repository since
2026-09-26) has that crate reaching the agent exactly as an outside consumer does, a
socket dialed by path. Both crossings are this boundary working. **What neither carries
is a seam edge, and the absence is the graph's rule rather than a ruling that the
boundary is not a seam.** A seam edge runs between two crate nodes, the far end of this
one is the world, and the graph carries no node for it, so the `party` edge above is the
whole of what the graph can hold here. `weaver-admin-PRD` section 6 says the same of its
own two outward boundaries and names this page as the shape they share, so a later
reader meets one reading rather than two. **The world at this boundary is a local
principal**, per the party prose above and the named local Unix sockets of
`weaver-gate-PRD` section 2, so a client stands on the box the agent runs on by
construction and no document owes a placement clause to put it there.

## 1. The channel

The named endpoint the raise directive carries, supplied by the harness and bound by
the gate, per `weaver-gate-PRD` section 2. **What kind of endpoint it is and where it
lives are the Spec's**, and a consumer needs neither: it is given the name and finds a
listener there. The access rule beside it is the operator's, so what the world meets is
a door the program placed and a predicate the operator wrote. It exists between raise
and lower and at no other time: a connection before ready or after stopped finds no
listener, which is the boundary the lifecycle protects.

**Admission is by verified peer identity.** Every connection carries a principal
identity **the channel authenticates rather than the caller asserts**, and that
identity is judged by the `authorization-predicate` of `weaver-types-PRD` section
2.2, admitting front-end principals only. **The predicate excludes the agent's own
principal**, so an elected tool cannot dial the agent's own mouth and prompt it
through its own front door. Whoever connects gets to converse with the agent and gets
nothing else.

**The mechanism is the Spec's.** What this page requires is that the identity be
verified by the channel rather than claimed in the message, and that a peer failing
the predicate be refused before any byte of its ask is read. What supplies that
verification is a property of the substrate the seam runs on and is named in
`weaver-gate-Spec`. **A consumer builds against the property and not against the
mechanism**, which is what lets the same page stand if the substrate changes.

## 2. What crosses in, and its format

One turn's work: a prompt. **The format is newline-delimited JSON, UTF-8, one request
per line.** The field list of a request is the Spec's, and so is the line's
bound. The framing is this document's,
because a client is built against this page and nothing else.

The gate does not parse the line. Delimiting and meaning are the harness's question,
and the gate relays octets in order, per its charter's opacity rule. A line that does
not parse as one JSON value is a refused turn, and the refusal returns by the path
the line took.

## 3. What crosses out, and its format

The turn's close: one JSON line per turn, carrying the response, or carrying the
close reason where no response exists, a stopped turn closing with the stop reason
marked in its place per the grammar of the basic loop. The close names its kind, so
a client can tell a clean close from a stopped one without reading anything else.
An answered close whose generation was cut at the turn's token limit carries one
more member, `finish` with the value `"length"`, and carries it only then: absence
means the model reached its own end, so a client renders a truncated answer as
truncated and pays nothing on the whole ones.

**A close that answers a turn names it, and names the run that turn belongs
to**, per `weaver-gate-PRD` section 1, so a client can refer to a turn rather
than only receive one. Both cross because one does not identify: a turn key
counts within its run and restarts with the next, and the run reference is what
distinguishes runs. A turn refused by the interior carries them like any other,
that being the close a client is most likely to need to name.

**A close that answers no turn names none, because there is nothing to name.**
A line that does not parse as a request is refused before any turn opens, so no
turn exists for the close to identify and the close says only what it is. The
distinction is the client's to read and is worth the reading: a named close
reports what became of a turn, and an unnamed one reports that a line never
became a turn at all.

**They are labels and not keys to anything.** Neither admits a client to any
seam, every seam authenticating by peer credential and none accepting a name as
a reason, and neither reaches the record, which is the operator's on the far
side of a sink this boundary never touches. What crosses is the ability to
refer.

The crossing delivers and does not clock. A response returning through this socket
belongs to a turn already closed in the record, per `basic-inference-loop` section 4,
so a client that never receives its line has lost a delivery and not a turn.

**One line in and one line out is the resting shape, and streaming is deferred.** The
token workflow rules streaming, partial output, and whatever else elaborates the
response path, as extensions to this page rather than replacements of it.

## 4. Ordering

- Order is preserved per connection, in both directions.
- A response returns by the path its request took, and by no other.
- One turn is in flight per agent, per the basic loop, and a second request
  waits rather than being refused. The token workflow's gate act of 2026-08-02
  settled it: clients may speak at once, the gate relays each as its own
  exchange, and the harness serves them one at a time in arrival order, per
  `weaver-harness-gate-contract` sections 2 and 3. A client is owed order on
  its own connection and is promised nothing across clients.

## 5. Failure

- A peer that fails the predicate is refused at accept, before any content is read.
- A request while the hook is lowered finds no listener, which is refusal by absence
  and not a typed answer.
- A line that does not parse is a refused turn, per section 2.
- A line that exceeds the Spec's bound with no delimiter found has left the
  framing, and the connection closes at that layer, below any turn: nothing
  was refused because nothing was ever a line. The bound is inclusive, a
  line of exactly the bound followed by its delimiter being legal.
- A gate death mid-turn is the loss of the delivery and not of the turn: the record
  holds the close, and what the harness does with the death is the coordination
  seam's, per `weaver-harness-gate-contract` section 5.

## 6. Prohibitions

**On the client.** It holds no channel to the program but this socket. It reaches
nothing past the gate, learns nothing of the interior, and receives responses and
refusals and nothing else.

**On the gate.** It reads no content and translates nothing, in either direction. It
retains nothing about a turn after the response returns. It admits no peer the
predicate does not name, and it does not degrade the predicate to a warning.

**On both.** Neither party carries a fact about the other's interior. The client
does not know what serves it and the gate does not know what a prompt means, and the
crossings above are the whole of what either learns.

## 7. Vocabulary

**Drawn from `weaver-types`:** `peer-identity`, `authorization-predicate`,
`gate-instruction`.

**Drawn from `weaver-traits`:** nothing. The clause is present with that answer
because `weaver-types-PRD` section 5 asks for it even when it is empty.

**Drawn from `weaver-trace`:** nothing, and the close's two identifiers do not
change that. They are the floor's, minted by admin and the harness and carried
in `weaver-types` shapes, and the record uses them rather than owning them, so
what crosses is the program's own label and not an envelope field. No event
kind, envelope field, or payload shape crosses this boundary, and what the
record holds about a turn is authored inside, by the harness, on the other side
of the gate.

**They add no draw either**, because neither is a vocabulary definition. The
floor holds them as identifier choices whose representation carries no
vocabulary weight, per `weaver-types-Spec` section 11, and a satellite of that
kind takes no node, so there is nothing here for a `draws` edge to point at.
The claim is about the graph and nothing else: these identifiers do cross this
boundary, which is what section 3 records.

**The clause above is stated in edge form here**, per Document Format section 4,
which makes `draws` the vocabulary clause a query can walk and is what turns G4 from a
reading into a query. The block sits at the clause it argues rather than in section 0
beside the party edge, per that format's section 6.

```graph
edge: draws
from: weaver-gate-world-contract
to: peer-identity

edge: draws
from: weaver-gate-world-contract
to: authorization-predicate

edge: draws
from: weaver-gate-world-contract
to: gate-instruction
```

The turn frame definitions this boundary implies are owed on demand when the token
workflow charters the turn exchanges, and nothing enters `weaver-types-PRD` section
2.3 before that demand fires. **They take no edge until they exist**, an edge to an
undeclared node being the dangling target Document Format section 4 rules out.

## 8. What this document changes elsewhere

- `weaver-gate-PRD` section 10: the client-boundary cell and the wire-framing cell
  close against this document. Landed in the same act.
- `basic-inference-loop` section 7: the wire-framing cell leaves, settled here at
  the delimiter level with the field shapes staying with the spec seat. Landed in
  the same act.
- `WeaverTools-Document-Format.md`: the external-principal party category rides
  `weaver-admin-operator-contract`'s register entry.
