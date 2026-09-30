# weaver-organ-channel

**Status:** MERGED. In `main` and the source of truth. Cut as the lift
`weaver-admin-harness-contract` section 0 anticipated, when a second organ contract
needed the same channel, and ruled in on 2026-07-31. This document is the one
statement of the organ channel's mechanics and every organ contract draws it.

**Date filed:** 2026-07-31
**Document ID:** `weaver-organ-channel`
**Parent:** `weaver-agents-PRD`, invariant 5.4
**Editorial:** Per the Working Rules.

**On the word floor.** Apex section 5.1 rules that the floor is exactly `weaver-traits`
and `weaver-types`, that floor is a linkage fact rather than a rank, and that a floor
crate is one every domain draws from and no domain contains. A document is not a crate
and links nothing, so this document is not called floor. What it is, said without the
word, is the organ channel's mechanics stated once and drawn by every organ contract.
Section 5 states where it files and what marks its kind.

---

## 0. What this document is

The mechanics of the channel an organ holds with the harness. Apex section 5.4 makes a
two-initiator channel with the harness one of the two properties of an organ, so every
organ has one of these and every organ contract would otherwise describe it. This
document describes it once.

**The layering is the point, and it is not new here.** `weaver-admin-harness-contract`
section 0 already marks the boundary inside itself: sections 1 and 2 are the organ
channel, sections 3 through 7 are admin's instance, and the channel does not know what a
load directive is in the way that IP does not know what a name lookup is. Those sections
were written to survive being moved, and this is the move.

**It settles nothing an organ contract settles.** It holds no exchange list, no failure
vocabulary, no ordering rule about what may follow what, and no prohibition on either
party's conduct. Every one of those is the business of the contract above it, and a
layer is usable in proportion to how sharply its non-guarantees are stated.

**It declares no graph records, deliberately.** Each organ contract already draws
`organ-envelope` in its own vocabulary clause, so this document is the source of no edge
and needs no node record of its own. That is not a stub's silence. A stub declares
nothing because it decides nothing, and this document decides a good deal and happens to
decide nothing the graph carries. The consequence is that the filing ruled in section 5
was a small one: a question about where the file sits, not about what kind of node it is
or who its parties are.

## 1. The channel has two initiators

**This section is the message layer and it holds for any two-initiator channel, whether
or not a process boundary is crossed.** An in-process two-initiator channel inside one
domain is each domain's own business, unconstrained per apex section 5.4, and may draw
this section alone if its domain so chooses.

**Either party may open an exchange.** Two initiators exist on an organ channel and both
directions are first-class. The harness opening an exchange is a normal event rather
than an intrusion, and an implementation that treats a harness-opened message as a
protocol error has the seam backwards. The same holds read the other way for the organ.

**Two-initiator is the property's name, per the initiation ruling of 2026-08-07.** The
earlier name was duplex, and the flow words are retired from live prose because they
answer a different question: simplex and duplex describe how bytes move on a wire and
say nothing about which party may open an exchange. Every channel in this program moves
bytes both ways, so a flow word distinguishes nothing here, while the property this
section states is initiation and does the work the old name was carrying. A channel is
named by how many parties may open an exchange on it, and this one takes two. Records
of acts made under the old name keep their wording, being records rather than live
claims.

**Which party opened an exchange is carried per message, and that follows from the
property.** Either party may open one, so the fact cannot be read off the channel, which
is why the identity rule below is the opening party and that party's ordinal, and why
`weaver-types-PRD` section 2.3 names wire vocabulary for the loop whose traffic it
carries rather than for a sender. A channel only one party may open would carry the same
fact in the object instead, and no such channel is chartered in this corpus today.

**The exchange is the unit, and every message names the one it belongs to.** A message
carries the exchange it is part of, its position in that exchange as open or continue or
close, and the type of its payload. Nothing else is required of the envelope for the
channel to work. The channel routes on those three fields and reads no further, which is
what keeps this layer indifferent to what the layer above is saying.

**An exchange is identified by its opening party and that party's ordinal.** Two
initiators numbering their own exchanges cannot collide without coordinating, so there
is no correlation authority to appoint and no shared counter to keep. This is the
mechanism that lets one channel carry both directions, and it is why a single-initiator
reading needed a second channel and this one does not.

**The minimal exchange is a single message that opens and closes at once.** Nothing
requires an exchange to have two sides. An announcement that expects no answer is a
complete exchange of one message, and it is the same shape as a directive rather than a
special case beside it.

**More than one exchange may be in flight.** A rule permitting one is an artifact of a
single initiator rather than a property worth keeping. Under two initiators it is
unholdable, because the party that did not ask has no way to know an exchange is
outstanding at the moment it needs to open one.

**What the channel does not provide is as load-bearing as what it does.** It does not
interpret a payload, does not retry, does not time anything out, does not synthesize a
message neither party sent, and holds no opinion about whether an exchange makes sense.

**What it does provide is boundaries, ordering, and loss-free delivery until closure.**
One write is one read, messages arrive in the order they were written, and a message
either arrives or the channel closes. There is no silent loss.

**The datagram shape is borrowed and the datagram guarantees are not.** The envelope is
a discriminated record on a preserved boundary, which is the shape worth taking from a
protocol like IPv4. What is not taken is best effort, and what is not needed is
addressing. A connected pair is one hop with one peer and possession identifies that
peer per section 2, so there is no address to carry and nothing to route between. On
this substrate ordering and boundary preservation are properties of the socket type
rather than work done above it, and a reader who inherits unreliability by association
with the metaphor has read the metaphor as a guarantee.

## 2. The channel's shape

**This section is the process-boundary layer and it holds only where the channel
crosses a process line.** A channel that crosses none has no handle to carry, no
peer to authenticate, and no image replacement to survive, so it draws section 1 and
none of this
one.

**A connected pair, created by one party and reaching the other, that no third party
can open.** The channel is unaddressable from outside: there is no name a stranger
could use to reach it, and **holding an end is what identifies a peer.** Those three
properties are what an organ contract binds, and they hold whatever carries the end
across. **What supplies them is the Spec's.**

**One pair carries both directions.** The creating party and the initiating party are
separate roles. Which party creates is the seam's own fact and belongs to its contract,
because it follows from which party exists first, and that differs by organ.

**The pair preserves message boundaries.** Section 1 requires one write to be one read,
which is a property of the socket type rather than of framing done above it. A stream
pair would push framing into every contract that draws this document, which is the layer
violation this section exists to prevent. The requirement is stated as the property, and
which socket type supplies it is the Spec's.

**This channel authenticates its peer by possession rather than by credential.** A
pair created by one party and handed to another cannot distinguish its ends by asking
the substrate who is on them, because both ends trace to the creator. An addressable
channel could answer that question and would also be reachable by anything running as
the agent's own principal, which includes an elected shell. **The pair is chosen
because it removes the second party rather than because it authenticates one**, and a
channel no second party can open needs no credential to tell them apart. This is apex
section 5.1's
possession case, and the invariant it satisfies is that no process in this program talks
to another without the second knowing who the first is.

**Non-inheritance does not survive a crossing, and the receiver is the only party that
can supply it.** Whether a handle is withheld from a party's own children is a
property of the handle rather than of the thing it opens, so **a party receiving an
end without asking for that property accepts one its children will inherit**, and
every process it spawns from that point holds the channel.

**The obligation is a set rather than a check, and it survives whatever the party does
next.** A step that finds the property absent and reports rather than repairs leaves
the channel inheritable, which is no better than not looking. And a party that
replaces its own image has to establish the property again afterwards, because
whatever supplied it did not survive the replacement. **How a substrate expresses the
property, and what clears it, is the Spec's.** This is why the obligation splits
across the parties in every organ contract rather than resting on the sender alone.

**A holder in transit is not a peer.** Where an intermediary places an end without
retaining one, the possession property is stated against retention rather than against
having ever held. Which intermediary, if any, and whether it exists at all, is the
seam's own fact.

**The channel lives exactly as long as the process at its far end.** It is not
reconnected, not reopened, and not shared with a second process. An organ that has lost
its channel has lost the thing the channel was about, rather than a connection to it,
and closure is therefore observed as death rather than as a transport fault. What each
party does with that observation is its contract's.

**Closure is not an answer.** A closed channel with no answer outstanding is the far
party having exited. A closed channel with an answer outstanding is the far party having
died mid-exchange, and the near party treats that as the failure of that exchange and
never as its success. Neither party synthesizes an answer from a closure. This is here
rather than in each contract because a synthesized success is the same defect on every
organ channel and the reasoning does not vary by organ.

## 3. What an organ contract keeps

Everything specific to its seam, which is more than it sounds like.

- Which party creates the pair, and how the far end reaches the process that holds it.
- What the channel's lifetime is bound to, since the far process differs by organ.
- Which handles may and may not accompany it across whatever crossing delivers it,
  and which party carries that obligation.
- The exchange list, and what each exchange means.
- The ordering rules over those exchanges, including which is first, which is terminal,
  and what an out-of-order directive does.
- What each party supplies and guarantees.
- The failure vocabulary and every refusal case.
- The prohibitions on both parties.
- The vocabulary clause, including its draw of `organ-envelope` from `weaver-types`.

The test is mechanical. A clause that would read the same for every organ belongs here,
and a clause that names an exchange, a party's own act, or a process this seam has and
another does not belongs there.

## 4. Vocabulary

`organ-envelope` is defined by `weaver-types`, per `weaver-types-PRD` section 2.3, which
names it the carrier every organ channel draws. The definition stays there. This
document does not draw it and does not redefine it. Each organ contract draws it in its
own clause, which is what keeps the G4 union computable from the contracts rather than
from a document that is not one.

The three fields section 1 names, the exchange a message belongs to, its position, and
the type of its payload, are that definition's content and not this document's. What is
stated here is that the channel routes on them and reads no further.

## 5. Filing

Ruled with the lift on 2026-07-31.

**This document files at `docs/crates/contracts/weaver-organ-channel.md`, and the
absent `-contract` suffix marks it as not a contract.** It binds no two parties, so it
is not a contract by the Format's own naming rule. It describes no crate, so it is not
a PRD. It is not build instructions, so it is not a Spec. The suffix reading is the
same exclusion-by-naming device the Format already uses twice, once where `contracts/`
excludes itself from the crate mirror by its absent `weaver-` prefix and once where
`.stub` excludes a live stub from the walk. The Format's sentence that a document
under `contracts/` is a contract is now a sentence about the suffix rather than the
directory, per its section 2.

**What the alternatives would have cost, kept as the record of the ruling.** Filing it
under `docs/project/` puts it beside the apex, where the Format says documents outside
the set live, which would make a document every organ contract draws from a document
the set does not contain. A fourth container of its own costs a new directory and a new
rule for a set with one member. A contract with every organ as a party costs a party
list that grows whenever an organ is chartered, which is the topology-document shape
the corpus has already rejected twice.
