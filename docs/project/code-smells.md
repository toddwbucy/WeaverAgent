# WeaverTools code smells

**Status:** RUNNING. Opened 2026-08-03 alongside the axiom layer. Entries accumulate
as they are found and nothing here is retired without a note saying why.

**Checked against the corpus 2026-08-24 and no entry was retired.** All four axioms
the entries ground in still stand, no entry cites a document that has moved or been
archived, and none carries a framing the corpus has retired since. **Two rejections in
section 4 were corrected** where the corpus had moved under them, and the one
invariant with no entry is now named.

**Date filed:** 2026-08-03
**Document ID:** `code-smells`
**Parent:** `weaver-agents-PRD`
**Editorial:** Per the Working Rules.

---

## 0. What this document is

A smell is a pattern the code must avoid. It is not an assertion, and the difference
is the direction it arrives from. An assertion is a claim a Spec makes and code must
satisfy, authored top-down from a document. A smell is recognized bottom-up, either
because an invariant makes a whole class of construction wrong or because the same
defect has now been met more than once.

**This document declares no graph records.** The format carries no node kind for a
smell and no edge from one, and inventing both before any code exists would fix a
shape against nothing. Each entry below names the axiom or rule it falls out of, so
the edges are decided when there is code to draw them against.

**Entries are stated as detections, not as advice.** "Prefer composition" is not a
smell. A smell says what pattern to look for, what it breaks, and how a reader or a
query would find it, because a smell that cannot be looked for is a preference.

**A smell is architectural unless it says otherwise, and a few do.** The architecture
is silent about substrate, which is what lets an organ move, so a smell that is really
a fact about this deployment would import a local decision into a rule meant to
survive one.

**Naming a substrate is not the test, and using it as one would misfile most of this
document.** Entries here say socket and path because that is what this deployment
runs on, and saying so costs them nothing. The test is the one G2 applies to a
contract: **change the substrate and see whether the entry still detects something.**
If the pattern survives with a different noun, the entry is architectural however it
is worded. If the pattern has nothing left to find, it was local.

Most survive. "One seam carrying two services" is the same defect over a wire as over
a socket. **Two do not**, and each says so at its head: a filesystem node's lifetime
and a listener's accept policy are not defects on a substrate that has neither.

**Where an entry is local, the scope filters which modules are in question, not how
many may be compared.** A local entry applies to the modules that bind the thing, so
a query for it selects those and ignores the rest. **Within that set it may compare
freely, and for some entries it must**: "one syscall, several accept policies" is a
disagreement between binding modules, and an entry confined to one module could never
see it. Scoping narrows the population, not the reasoning over it.

An entry with no scope line is architectural and applies everywhere.

**Where an entry cites the quarry it is evidence, not a carry.** The archived tree is
a parts source read and never edited, per the carry rule. A sighting there proves the
pattern is reachable by this team on this architecture, which is the only thing a
sighting is for. **An entry earns its place one of two ways and says which:** it
grounds in an invariant or the deliverable with a nameable clause, or it has two or
more distinct sightings. A candidate passing neither is recorded in section 4 rather
than filed, because a plausible rule with no grounding is how the prior program's
axiom basis reached seven of seventy-one claims and was kept anyway.

---

## 1. Smells that fall out of an invariant

These need no experience to justify. The invariant makes the construction wrong, and
the smell is the code-level shadow of a rule the document layer already carries.

### 1.1 A peer organ called directly, bypassing the harness

**Identifier:** `smell-peer-organ-bypass`
**Grounds:** `axiom-organ-and-submodule`, with
`axiom-floor-is-vocabulary-behavior-is-socket`

**The pattern.** One organ reaching another organ without going through the harness -
a Cargo dependency from one organ crate to another, a socket dialed at a path that
resolves to a peer, or a descriptor for a peer's channel held by anything other than
the harness that created the pair.

**What it breaks.** Apex section 5.4 makes the harness the organ whose domain is
coordination, and the hub every other organ holds its two-initiator channel with rather
than a spoke. A peer call is an edge the topology does not have. Section 5.1 then leaves
it nowhere to live: every seam where one crate asks another process to do something is a
socket governed by a named contract, and a peer call has no contract because no contract
was written for a seam the architecture does not admit. So the call is either an
undeclared seam or a path dependency across a process line, and 5.1 forbids both.

The cost is not stylistic. The harness is the sole author of the trace, and a turn
that includes an exchange the harness never brokered produces a trace with a hole in
it that no one can see, because the missing span was never anyone's to write. That
makes the deliverable wrong rather than merely the layering.

**How it is found.** Three ways, cheapest first:

- **The manifest.** An organ crate naming another organ crate as a dependency. This
  is the whole of the static case and it costs one read of seven `Cargo.toml` files.
- **The graph.** A `seam` edge between two non-harness organs, which phase two's
  closing checklist item 4 already queries for as "no lateral edges." The document
  layer is therefore already covered, and this entry exists because code can grow an
  edge the documents never declared.
- **The socket path.** A dial to any path not named by a contract this crate is party
  to. The organ channel case needs no check, since a channel with no name is reachable
  only by the party handed the descriptor, which is the authentication.

**Note on scope.** A submodule reaching its own organ is not this smell. Section 5.4
leaves the shape of that channel unconstrained and makes it the organ's business, so
a submodule-to-organ call is in bounds by construction and only an organ-to-organ
call is the pattern.

### 1.2 One socket carrying two services

**Identifier:** `smell-multiplexed-seam`
**Grounds:** `axiom-contract-is-a-complete-interface`, with
`axiom-floor-is-vocabulary-behavior-is-socket`

**The pattern.** A single socket carrying more than one service or modality. An
encoder and a decoder sharing one channel is the reference case. The correct shape is
two sockets, one per service, each with its own contract.

**What it breaks.** Apex section 5.3 requires a contract to name, for each party, the
vocabulary that crosses, the errors it can return, and the ordering guarantees it
relies on and provides. A socket carrying two services forces one of two wrongs: one
contract describing both, or two contracts describing one socket. The first destroys
the property 5.3 states outright, that an agent handed one side of a contract can
build that side without asking what the other side does, because an encoder builder
must now read the decoder's vocabulary, its errors, and its ordering to know which of
them reach the wire it is writing to. The second has no home in 5.1, which governs a
seam by **a** named contract and not by two.

The ordering clause is where it bites first. Two services on one channel share one
ordering regime, so a rule that exists for one constrains the other for no reason a
reader can find. A flush ordering written for decode traffic silently becomes a rule
about encode traffic, and nobody wrote that rule or can say why it holds.

There is a second cost the apex does not have to carry, because the Working Rules
already do. Service is serial per channel, so a multiplexed socket puts a long
operation in front of a short one that shares nothing with it. Latency is the enemy
of agency, and head-of-line blocking between unrelated services is that cost taken
for no gain.

**The corpus already has the correct shape, which is why this is a code smell and not
an open question.** `weaver-spu` holds two channel ends at descriptors 3 and 4 and
declares two `seam` edges to `weaver-harness`, one via `weaver-harness-spu-contract`
and one via `weaver-harness-spu-decode-contract`, both tagged `socket`. The decision
is made. The smell guards it against code that merges what the documents separated.

**How it is found.**

- **The count.** A crate holding fewer channel ends than it has seam edges. The two
  numbers are stated in every organ's Spec and are a direct comparison.
- **The dispatch.** A read loop whose first act is to branch on a field naming which
  service the message is for. That branch is the multiplexing, and it is visible at
  the top of the loop rather than buried.
- **The vocabulary.** One envelope enum whose variants span two contracts. If the
  wire type names both a residency directive and a decode ask, the socket beneath it
  carries both.

**Note for the graph.** Two `seam` edges between the same crate pair is correct here
and a query that treats a crate pair as unique would report this program's own
intended shape as a duplicate. The seam's identity is its `via` contract, not its
endpoints.

### 1.3 Behavior in the floor crate

**Identifier:** `smell-behavior-in-the-floor`
**Grounds:** `axiom-floor-is-vocabulary-behavior-is-socket`, with
`axiom-organ-and-submodule`

**The pattern.** A trait in `weaver-traits` or `weaver-types` whose methods perform
work rather than describe it. The tells are an async trait attribute, a method
returning an error that names I/O or a backend, and an implementor living in an organ
crate. The threading form is the surest marker: a boxed trait object handed across the
composition root into a consumer that never named the producer.

**What it breaks.** 5.1 admits the floor as a Cargo dependency for one stated reason,
that the floor is shared vocabulary and a type definition cannot be sent over a
socket. The permission is grounded in an impossibility. A trait whose method runs a
forward pass or a store query **can** cross a socket, so it fails the test that earns
floor status, and putting it there converts the floor from the thing no domain
contains into the place every cross-process service interface is declared. 5.1 then
names the result exactly: a behavior reached by path dependency across a process line,
which it forbids.

**This smell causes 1.4, and that is why it leads.** A seam expressed as a trait object
has no wire, so there is no wire format for a join key to be designed into, and 5.2's
instruction to design the key in at the moment the wire is specified never gets a
moment. Anyone who fixes only the join key will keep re-earning it here.

**How it is found.**

- **The attribute.** An async trait attribute anywhere in a floor crate. A floor crate
  with any async trait at all is the finding and it costs one grep.
- **The implementor.** A floor trait whose implementation block lives outside the floor
  and outside test modules. An implementor in an organ crate means the floor carries
  that organ's service interface.
- **The error type.** A floor trait method returning an error naming a transport, a
  backend, or a device. That is the vocabulary of a seam rather than of a type.

**Note on scope.** A trait of pure inspection methods is vocabulary and belongs in the
floor. Per-invocation tool safety classification, which inspects an input value and
does no work, is the legitimate near-neighbour. The smell is the work-performing method
beside it, not the trait's existence.

**Seen in the quarry.** All ten of the floor crate's public traits are async service
interfaces, including `AgentMemory`, `SessionContext`, `Embedder`, `Provider`, and
`GNNInference`, with implementors in `weaver-memory`, `weaver-spu`, and
`weaver-interface`. `weaver-traits/src/agent_memory.rs` calls the boxed handle "the
seam's threading form" in its own comment: the author knew it was a seam and gave it an
in-process shape anyway.

**The rationale is the tell, and it will be offered again.**
`weaver-spu/src/encoder/client.rs` deletes a socket seam under the heading "Why
in-process" and cites `latency is the enemy of agency` by name to justify collapsing
it. That mantra is in this program's Working Rules. The argument is locally correct
every time it is made, which is why it worked, and the cost surfaces two crates later.

### 1.4 The join key not carried with the work

**Identifier:** `smell-join-key-not-carried`
**Grounds:** `axiom-join-key-travels-with-the-work`

**The pattern.** Three forms of one failure, and a codebase usually has more than one.

- **Absent.** A request or response crossing a seam on behalf of a turn with no field
  for the trace context.
- **Optional and unset.** A key field typed as an option, defaulted to nothing by the
  constructor and set by a builder, so carrying the key is opt-in per call site. Worse
  than absent, because it looks compliant.
- **Ambient.** Code that needs to know which turn it serves reading it from process
  state, a thread-local span stack or a static, rather than from the work item.

**What it breaks.** 5.2 states the requirement and its consequence together: a
component handed work without a turn key cannot tell the harness which turn its result
belongs to, and with more than one turn in flight the harness cannot recover the
association afterward. The invariant closes by requiring the key be designed into every
wire format at the moment the wire is specified, which is an instruction about exactly
the moment this smell is introduced.

The cost is not a missing field. A component with a result to report and no key finds
another route back to the turn, and every such route is worse. The failure is silent by
construction: an absent key reads as a legal value, so nothing errors, and the system
stays coherent for as long as only one turn is ever in flight. The day a second one is,
the trace does not break loudly. A payload sniffer keeps returning a plausible key and
events attach to the wrong turn.

**How it is found.**

- **The setter census.** Grep every call of the key's builder and classify each as
  production or test. Zero production calls on a field the schema declares is the whole
  finding, and it costs one grep.
- **The type.** A join key typed as an option, or carrying a skip-if-absent
  serialization attribute. The field identifying the turn cannot be optional on
  anything belonging to a turn.
- **The wire diff.** For each seam contract, diff the request type's fields against the
  contract's vocabulary clause.
- **The ambient read.** A function returning a trace identity that takes no argument.
  If the id is not derivable from the parameters, it came from ambient state.
- **The fallback.** A reader resolving an identifier by trying a list of candidate field
  names. The list is the confession.

**Note on scope.** 5.2's carve-out is real and load-bearing. A lifecycle directive on
the coordination seam and a residency directive on the SPU seam belong to no turn and
correctly carry no key. The test is whether the request is on behalf of a turn, not
whether it crosses a seam. A session-open ask carrying no key is correct. A decode ask
carrying none is the smell.

**Seen in the quarry, in all three forms.** The decode request and response types carry
no trace context at all, and the callee mints its own correlation id from the wall clock
which nothing then reads. The trace envelope's turn key is optional with a
skip-if-absent attribute, and across 150k lines its setter is called at four sites,
every one inside a test module: **production set the turn key zero times.** The reader
then guesses, matching four candidate field names positionally against an untyped
payload. The ambient form is a thread-local span stack whose harness-side reader is a
stub returning nothing unconditionally, documented as degrading gracefully, so three
call sites emitted unparented events for the whole life of the tree with no test
failing.

### 1.5 The seam's vocabulary declared once per side

**Identifier:** `smell-per-side-wire-vocabulary`
**Grounds:** `axiom-contract-is-a-complete-interface`

**The pattern.** A seam whose types exist in two places, one per party, with nothing
shared. The tells are mechanical: request types deriving deserialization only and
response types deriving serialization only, so the peer cannot link them even if it
wanted to. The peer then re-declares the same shapes privately under a prefix, or builds
the payload from string keys, or both.

**What it breaks.** 5.3 requires the contract to name the vocabulary that crosses the
seam with its meaning. Vocabulary existing only as one side's private struct plus the
other side's hand-typed string keys is named nowhere a contract could point at. The
stated property collapses: an agent handed one side can build it without asking what the
other side does becomes an agent that must read the other side's source, because the
source is the only statement of the wire.

The cost is silent divergence. A field one side sends and the other never modeled is
discarded by the deserializer with no error, so the two parties disagree about the
vocabulary and there is no place for the disagreement to surface. Under 5.3 that is not
a bug found late, it is the shape working as built.

**How it is found.**

- **The derive.** A wire struct deriving one direction and not the other. An asymmetric
  derive on a seam type says only one party can hold it.
- **The prefix.** Privately-scoped structs in a client module named after the wire
  rather than after the domain. Their presence means the vocabulary has a second
  declaration.
- **The string key.** Any string key on a wire. It is a field the contract cannot check.
- **The unknown-field policy.** A wire struct that does not deny unknown fields. This is
  what converts divergence into silence.

**Note on scope.** A translation layer at the composition root converting program
vocabulary to a foreign provider's schema is correct and is a pattern worth carrying.
The smell is this shape on an internal seam, where both sides are ours and a shared
definition was available.

**Seen in the quarry.** Twenty-plus decode wire types derive one direction only, and the
peer re-declares eleven of them privately and assembles its request body from string
keys. One such key, carrying prompt provenance for the attribution instrument, is
silently discarded by the receiver, whose comment documents the discard and calls it
acceptance: serde ignores the unknown field, so it is accepted, not rejected. The
deny-unknown-fields attribute appears thirty-eight times in that tree and never once on
a wire type, only on files a human authored.

### 1.6 A component authoring its own span

**Identifier:** `smell-component-authored-span`
**Grounds:** `axiom-join-key-travels-with-the-work`

**The pattern.** A non-harness crate emitting a trace span rather than reporting a
result. Two mechanisms. The first looks like logging: a component annotates a method and
a process-global subscriber turns that span into a durable record with no harness
involvement. The second is direct: a writer handle to the trace is cloned and handed
out, so anything holding it appends.

A component authoring a span cannot share the trace crate's vocabulary constants,
because span macros require literal field names. So it restates the attribute vocabulary
as string literals, and the restatement is the reliable tell.

**What it breaks.** 5.2 is flat: the harness is the sole writer, so a component does not
emit its own spans. It reports, and the harness authors the event. A globally-installed
subscriber inverts this. Authorship becomes a property of which crate happened to
annotate a function, which is not a decision anyone made about the trace.

The cost compounds with 1.4. A component-authored span carries whatever the component
had in scope, and a component with no turn key has none to put in it. So the mechanism
that lets components write is also the mechanism guaranteeing what they write is
unjoinable. The trace fills with correctly-formatted records that cannot be attributed,
which is worse than a hole, because a hole is visible.

**What it also breaks, and this is the part that surprises.** Once the vocabulary is
restated, a rename in the definition compiles clean everywhere and silently detaches
every emit site from every reader. The guards written against that fail in a specific
way worth naming: **a drift guard comparing a constant to its own literal, in the file
that defines it, cannot fail.**

**How it is found.**

- **The manifest, inverted.** A crate that emits spans and does not link the trace
  crate. It cannot be sharing the vocabulary, so it is copying it.
- **The literal.** Any attribute-name string in a span macro that also exists as a
  constant in the trace crate.
- **The subscriber install.** One global registry install makes every crate in the
  process a potential author. It is a single line and it is the enabling condition, so
  check it first in a new tree.
- **The writer handle.** A trace writer that is cloneable and shared, or any setter
  handing one out. A writer that can be handed out will be.
- **Guards that name themselves.** For every comment containing "drift", find the test
  and check what it compares. If both sides of the assertion live in the file defining
  the constant, it guards nothing. Same for any build feature with no code behind it.

**Note on scope.** A component printing a diagnostic for an operator is not this smell
and is covered by 2.3. The line is whether the emission reaches the durable trace. The
new program's answer is not to share the constant harder, it is that the component
should not author the span at all. A component reporting a structured result the harness
turns into an event needs no trace vocabulary and cannot drift from one.

**Seen in the quarry.** Two SPU modules carry byte-identical span annotations with a
comment admitting the coupling is unenforced, in a crate that cannot even link the trace
crate. The harness mixes both forms in one macro call, a constant on one field and a raw
literal on the next. The cited drift guard asserts a constant equals its own value in
the same file and cannot fail unless someone edits both halves of one line. A third
guard is a build feature justified by a dependency claim the same manifest contradicts
twenty lines later, and it gates no code at all.

### 1.7 A seam carried by a shared path

**Identifier:** `smell-seam-by-shared-path`
**Grounds:** `axiom-floor-is-vocabulary-behavior-is-socket`, with
`axiom-organ-and-submodule`

**The pattern.** Two processes coordinating through a file. One writes a serialized
structure to a path, the other opens the path and parses it, and both link a common
crate for the type. No socket, no contract, no listener. The tell is a read of a path
the calling process did not write.

**What it breaks.** 5.1 sorts every seam into two boxes and tags it. Within a process it
is a link under a named contract. Across a process line it is a socket under a named
contract, and it authenticates its peer. A file drop is in neither box, so it is
untagged, uncontracted, and invisible to any query that walks seams. It is also the one
form that cannot authenticate: the reader learns the path's ownership and never the
writer's identity, and 5.1's stated property is that no process talks to another without
the second knowing who the first is.

5.4 takes the second half. A submodule has its organ as consumer. A submodule that
writes files acquires as many consumers as there are readers, and each reader links that
crate to parse them. Depending on nothing while being drawn by everything is the exact
pair of properties 5.1 uses to define the floor, applied to a crate 5.1 denies floor
status by name.

**How it is found.**

- **The read.** A file read of a path in a crate that has no writer for that path.
- **The manifest.** A crate linking another domain's crate with no method call on it,
  only type imports. The link exists to parse, so the transport is out of band.
- **The consumer count.** A submodule crate named as a dependency by more than one
  crate. 5.4 gives a submodule exactly one consumer, so two is the finding.

**Note on scope.** A durable trace written to disk is correct and is the deliverable.
The smell is a **second process** treating that file as its input channel. An offline
tool an operator runs against a finished trace is the legitimate near-neighbour, and the
distinction is whether the reader is part of the running system.

**Seen in the quarry.** Admin builds an agent state file and the harness reads it from a
path, the two running as different processes under different OS users, with the whole
producer-to-consumer relationship being a YAML file. The harness authenticates the peer
that asked it to load the file and never learns who wrote it. `weaver-trace` is linked
by four crates across three domains and depends on nothing itself.

### 1.8 Peer authentication decided per route

**Identifier:** `smell-route-selective-peer-auth`
**Grounds:** `axiom-floor-is-vocabulary-behavior-is-socket`

**The pattern.** A server that resolves peer credentials at accept, passes them inward,
then consults them on some request paths and not others. The exempted paths are the hot
ones, and the stated reason is that the socket's file mode already limits who can reach
it.

**What it breaks.** 5.1 is unconditional here: every seam is a socket governed by a
named contract, **and it authenticates its peer. There are no exceptions.** It then
names exactly two mechanisms and ties each to a channel property. A named path
authenticates by credential. An unnamed pair authenticates by descriptor possession.
File mode is a third mechanism and it establishes reachability, not identity. It cannot
tell two admitted uids apart, which is the whole question on a machine running one OS
user per agent.

The invariant's own commentary anticipates this. It records that reading the credential
mechanism as the invariant made a real seam look like an exception, and warns that an
invariant admitting one exception in its first contact with a real seam will admit a
second.

**How it is found.**

- **The unused binding.** A handler taking a peer identity and never reading it, or an
  optional identity threaded through a dispatcher and consulted in only some arms. No
  compiler warning fires, because a sibling arm uses it.
- **The mode.** A permissive mode on a socket path. Correct only when paired with a
  credential check on every route, so it sends you to detection one rather than being a
  finding by itself.
- **The comment.** Prose in the accept loop explaining which routes consult the
  credential. Anyone who writes that sentence made the split deliberately.

**Note on scope.** A permissive socket mode is not the smell on its own. Mode plus an
unconditional credential check on every path is the shape 5.1 asks for. Mode **instead
of** credential on some routes is the smell.

**Seen in the quarry.** The SPU decode server resolves the peer credential at accept and
states in its own comment that only the mutating model-lifecycle routes consult it,
decode staying open at the socket-mode boundary. Decode is every turn, so the turn path
is the unauthenticated one. The gate shows the correct use in the same tree: a
permissive mode with the comment that filesystem permissions only admit connection
attempts and actual admission is the credential plus the operator list, authorizing
unconditionally on every path.

### 1.9 A socket path with no owner

**Identifier:** `smell-unowned-socket-node`
**Grounds:** `axiom-floor-is-vocabulary-behavior-is-socket`, with repetition
**Scope:** local substrate. A filesystem node has a lifetime only where the seam is
carried by one, so this detects nothing on a substrate without paths. It applies to
the modules that bind, and not to the architecture.

**The pattern.** A process binding a Unix socket creates a filesystem node, and nothing
owns that node. Each server hand-rolls both halves of its lifetime. The acquire half is
an unlink of the path followed by a bind. The release half is an unlink somewhere near
exit, or nothing.

**What it breaks.** 5.1 makes every cross-process seam a socket governed by a named
contract, and the apex makes the agent's gate hooks the listening sockets the agent
binds, two as of the egress ruling of 2026-08-07 and each bound from an instruction.
Neither survives a path whose custody is decided by whoever calls unlink last, and a
second bound path is a second custody question rather than a new kind of one.
Three failures fall out and they sustain each other:

- **Bind steals a live peer.** A guard distinguishing a socket from a regular file never
  distinguishes a stale socket from a live one. A second instance deletes the first's
  node and binds its own. The first keeps its listener descriptor and keeps accepting on
an inode with no name, still holding its residency and its device budget, serving turns
no client can reach. Nothing errors, so no fault is reported.
- **Unlink kills a live successor.** The mirror case. Identity-checking the node at bind
  closes the common case and leaves a stat-then-unlink window open.
- **Bind leaks a node on the failure path.** Bind first, set the mode second: if the
  mode call fails and returns, the listener drops and closes while the node stays on
  disk. The next start's blind unlink removes it, so the leak on one side is what
  motivates the theft on the other.

**How it is found.**

- **The unlink.** Any path removal within a few lines of a bind.
- **The gap.** Any statement between the bind and the socket's final mode being set. If
  it can return, the node outlives the descriptor.
- **The exit path.** An unlink at shutdown naming a path instead of consulting the
  identity of the thing it bound.
- **The count.** Several servers with several different guard strengths on one seam
  kind. Divergence on a repeated seam is the tell before you decide which version is
  right.

**Note on scope.** A process removing a socket file it is about to replace within the
same start is not this. The smell is the absence of an owner, not the presence of an
unlink. The correct shape binds in a directory the process holds exclusively, or takes
the node's identity at bind and never touches the path by name again.

**Seen in the quarry.** Five servers, five different guard strengths, from an unlink
with the error discarded and no checks at all to an existence-plus-type check with an
identity captured at bind. One crate has no release half whatever: it installs no signal
handling, so neither of its two bound sockets is ever unlinked, and one of them is bound
inside a spawned task whose handle is dropped on the floor.

---

## 2. Smells that fall out of the deliverable

The deliverable is a deployable proto-stateful agent emitting a clean, turn-bracketed,
correctly-custodied trace, and the trace is the primary artifact rather than a
diagnostic. These ground there rather than in one of the invariants, and say so.

### 2.1 A defined event with no producer

**Identifier:** `smell-producerless-event`
**Grounds:** the deliverable, trace-is-the-artifact

**The pattern.** The consuming half of a feature ships complete, the event kind, the
parser, the projection, the durability hook, the route, the test, and the emitting half
never lands. The result is not an obvious stub. It is a fully furnished, unit-tested
surface that is never reached.

**What it breaks.** Under trace-is-the-artifact a defined but unemitted event is not
neutral. It is load-bearing for anything downstream keyed on it, and it fails silently:
the reader handles the kind, so nothing errors, and the absence reads as "that did not
happen this run" rather than "nothing can make that happen."

**The worst form is a durability or flush policy keyed on an event nobody emits.** The
operator configures a bounded loss window and receives an unbounded one, with nothing
anywhere reporting the difference.

**How it is found.**

- **The producer census.** For every variant of the event-kind enum, grep for the
  variant outside the trace crate and outside test modules. A zero is a defined event
  nobody writes. One shell loop, and it is the whole detection.
- **The commit predicate.** Any durability policy whose trigger is an event kind,
  cross-checked against that census.
- **A dead-code attribute on a wire type**, especially one with a parser or a
  serialization derive.
- **The stub comment.** Phrases like "not yet wired" or "the upstream sources do not
  exist yet." In a healthy tree these are honest and sit directly above the surface.
- **Reader-writer asymmetry.** A projection field written once and read nowhere, or a
  function with no caller outside its own tests.

**Note on scope.** A named stub is in bounds. The carry rule's second door admits one
when a written design already names the joint. This smell is different: a surface
shipped as finished, tested, and routed, whose producer was deferred without the
deferral being visible to anything depending on it. **The distinguishing question is
whether a caller can tell.**

**A test can make this worse rather than catching it.** A test that synthesizes the
event production never emits proves the consumer works and says nothing about whether
anything reaches it. That converts unenforced into documented-as-enforced, which the
enforcement posture names as worse than no test.

**Seen in the quarry.** Ten of twenty-six event kinds had no producer outside the trace
crate. Five of them were events the apex one-turn walk requires the harness to write.
The commit policy advertising a per-turn loss window keyed on a turn-close event nothing
emitted, so it behaved exactly like manual commit, and the test proving it worked
synthesized the event.

### 2.2 The turn bracket that exists only in vocabulary

**Identifier:** `smell-unbracketed-turn`
**Grounds:** the deliverable, turn-bracketed

**The pattern.** The turn open and turn close exist as vocabulary and as function-local
variables in the loop that runs the turn, not as emitted events. The turn becomes
visible only at the moment it succeeds.

**What it breaks.** The deliverable is a turn-bracketed trace, so an unclosable bracket
is not a logging gap. It is the product failing to be the thing it is. The failing case
is the one that matters: a turn erroring mid-decode leaves the trace with the events
that preceded it and no statement that anything ended, and a reader cannot distinguish
that from a turn still running. **The trace is silent in precisely the situation it
exists to explain.**

**How it is found.**

- **The open and close pairing.** For each bracket-open site, ask whether the close is a
  destructor or a sequential statement. Grep the span between them for early returns,
  error propagation, and select arms. A sequential close with a propagation in between
  is the smell.
- **The producer census** of 2.1, applied to the bracket events specifically.
- **The caller obligation.** A close function whose callers are a subset of the paths
  that open. One caller honoring it and one not is the same smell in a milder form.

**Note on scope.** Destructor-based bracketing is the correct shape and is not this
smell. Bracketing spans is also not bracketing turns: a tree can have working span
brackets and no turn bracket at all, which is what makes this worth its own entry rather
than folding it into 2.1.

**Seen in the quarry.** The turn open and close events were declared, restated in two
conversion functions, consumed by the projection, and gated a commit policy, and no
production code emitted either. What existed instead was two locals inside the iteration
loop with an await and error propagation between them, so on a decode error the whole
session trace local was dropped, which the caller confirms by binding it to a discard
after the propagation.

### 2.3 A second alert path

**Identifier:** `smell-second-alert-path`
**Grounds:** the deliverable, the stream as the program's one fault carrier

**The pattern.** A component's decisions and faults written to stderr, sometimes with a
structured event name embedded in the string, while the trace records nothing.

**What it breaks.** The apex permits exactly one fault carrier and names it the stream.
Every such print is a second one, and it is the channel the trace cannot reach, the sink
cannot receive, and replay cannot consume. The sharp cost is at the boundary: the gate
is where every request enters, so a denied connection is the one class of event the
harness structurally never sees, recorded only where the artifact is not. An operator
diagnosing a silent agent reads a trace in which nothing happened, because what happened
was a credential rejection printed on another descriptor.

**How it is found.**

- **The ratio.** Per crate, count print macros against trace emissions. A crate with a
  nonzero first number and a zero second has one output channel and it is the wrong one.
- **The dotted name in a format string.** A print whose argument contains a dotted
  identifier. Someone wrote an event and chose a print macro.
- **Decision sites with no emission.** For each authorization or admission check, look
  at both arms. An arm returning an error to the caller and emitting nothing is the
  pattern.

**Note on scope.** A process that has not installed its trace sink, or is failing in a
way that precludes emitting, may legitimately print. Startup and the panic path are
those. Steady-state authorization decisions are not. Where a component has no route to
the trace at all, the print is the symptom saying so and the fix is the seam.

**Seen in the quarry.** The gate and the admin trusted base each had zero trace calls
and fourteen and eight prints respectively, while the harness had 135 trace calls and no
prints. The gate's fatal accept path printed a dotted event name as a formatted string
on file descriptor 2.

### 2.4 A held resource released only by a message the holder may never send

**Identifier:** `smell-cooperative-release`
**Grounds:** the deliverable, proto-stateful

**The pattern.** Process A claims a resource inside process B. B releases it only on an
explicit close verb from A, with no timeout, no liveness check, and no reclamation of
its own, so the resource's lifetime is bounded by A's cooperation rather than by A's
existence.

**What it breaks.** Apex section 2 names the KV cache as one of two deliberate
cross-turn holders and says its owner, its flush trigger, and who is forbidden to touch
it are named as a rule code can be checked against. It also warns the cache is the
surface most likely to grow quietly into session state if the line is not drawn. This is
precisely how that line fails: the owner is named, and the owner has no way to take the
thing back.

The cost is the deliverable. A single-resident slot plus one orphaned claim makes the
model permanently unavailable to every other owner, so the next load gets a hard refusal
from a resource held by a process that no longer exists. Recovery depends on a human
noticing, and nothing on the stream says so, because the failed close was logged as a
warning rather than emitted as a fault.

**How it is found.**

- **The reaper.** For any owner-side slot, grep the owning crate for timeout, idle,
  expire, ttl, last-seen, heartbeat. Zero hits is the finding and it is conclusive.
- **The release call.** Count the exits from the caller's release path that do not
  release.
- **The words.** Best-effort, fire-and-forget, a discarded result on a close, a spawn
  whose handle is dropped.
- **The connection.** A slot claimed over a connection the owner does not hold open. If
  the handler serves one request and returns, the peer's liveness was never in the
  picture and cannot be.

**Note on scope.** The hot KV cache is a named cross-turn holder, so a smell about
long-held state must say why the cache is not it. **It is not it for two reasons. The
cache's persistence is deliberate and its owner and flush trigger are named, which is
the opposite of a lifetime nobody bounded. And the cache is the thing being leaked here,
not the thing doing the leaking.** Holding KV state across turns within a session is the
sanctioned shape. Holding it across the death of the session that claimed it is not.

**Seen in the quarry.** The resident session slot had no timestamp and no expiry, and a
grep of the two owning modules for every liveness word returned one unrelated hit about
write backpressure. The handler served one request and returned, so no connection
outlived a verb. On the caller side one shutdown path had six exits that did not close,
one of them logging "leaving resident KV state untouched," and none of it runs on a kill
signal.

---

## 3. Smells earned from repetition

Entries land here when a defect has been met more than once and no invariant or
deliverable clause forbids the construction outright. The second sighting is what
promotes it, so each entry names both.

### 3.1 One syscall, several accept policies

**Scope:** local substrate. The pattern is a listener's accept-error handling, which
exists where a listener does, so it applies to the modules that bind and not to the
architecture. **The detection is the comparison between them**: a single module's
policy is never the defect, and the entry is only visible by reading the binding
modules against each other.

**Identifier:** `smell-per-seam-accept-policy`
**Grounds:** repetition, with `axiom-floor-is-vocabulary-behavior-is-socket`

**The pattern.** Each crate that binds a listener grows its own accept-error handling,
and they disagree about which errors are survivable.

**What it breaks.** The seams are the architecture. Under 5.1 every one is a socket
under a named contract, so what happens when accept fails is one question the program
answers once. Answered several times it produces the operator-visible cost that the same
kernel condition yields different outcomes depending on which seam met it, and under
trace-is-the-artifact the record of a seam failure becomes a property of which seam
failed rather than of what failed.

**How it is found.**

- **The listener census.** Grep for the accept call and read the ten lines after each.
  The divergence is visible without leaving the grep output.
- **The bare propagation.** An accept whose error is propagated with no classification
  is the absent-policy case and needs no further reading.
- **The orphaned public item.** A public function whose doc says it is public so another
  crate can adopt it, with no cross-crate caller. That comment is a design decision that
  did not happen, and it is greppable.

**Note on scope.** Two faces of one crate sharing one helper is the correct shape. The
smell is the next crate writing another answer.

**Seen in the quarry.** Three implementations. One classified errors into transient,
resource-exhaustion, and fatal, with per-class delays and failure budgets. One had two
classes and neither a delay nor a budget, so a sustained descriptor exhaustion became an
unbounded busy loop logging once per iteration. One had no classification at all, so a
single interrupted syscall would take down the inference server's accept loop. The first
was made public specifically so the others could adopt it, and nobody did.

### 3.2 Acquire and release written as statements, with a return between them

**Identifier:** `smell-hand-rolled-release`
**Grounds:** repetition

**The pattern.** A resource acquired by one statement and released by another, with
error propagation, an early return, a match arm, or an await in between. Correctness
then depends on every exit path having remembered to release. The tell is a match where
some arms release and some do not.

**What it breaks.** Nothing in the invariants forbids it, which is why it sits here
rather than in section 1. What promotes it is that the correct construction is usually
already present in the same file, reached for once and never generalized. At its worst
the asymmetric arm strands a resource against something that never became resident, so
every later request is refused by an authority protecting capacity that is free. The
refusal is correct-looking, the accounting is wrong, and nothing distinguishes the two.

**How it is found.**

- **The asymmetric match.** For any match on a result or an outcome, check whether every
  arm performs the same release. The panic and cancellation arms are where it hides.
- **The propagation between.** Any error propagation textually between an acquire and
  its paired release.
- **The container count.** Count independently-locked containers keyed by the same id.
  Several co-owners means several writes per transition and several chances to return in
  the middle. One owner holding one record has no partial state to leave behind.
- **The contrast.** A file containing one destructor-based guard and several manual
  release sequences. The manual ones are the finding.

**Note on scope.** A release deliberately skipped, with a named reason, is not this. The
smell is the arm that forgot, not the arm that chose.

**Seen in the quarry.** A model-admission path recorded a full device-memory estimate
before construction, and two of its three failure arms un-recorded it while the panic
arm did not. A residency transition wrote three containers through three locks with
error propagation on each. Two subprocess spawns propagated an error before the wait,
leaving children unreaped. The correct shape existed in both files: one reservation
guard whose own doc states the exact hazard, and one process-group guard doing the same
job.

---

**One invariant carries no smell.** Four of the apex's five axioms ground an entry
here. `axiom-harness-integrates-by-the-loop` grounds none, which is a gap in coverage
rather than a defect in an entry: no construction violating it has been met twice, and
none has been argued from the invariant. **Recorded so the absence is read as unmet
rather than as unreachable.**

---

## 4. Considered and rejected

Recorded so the same candidates are not re-litigated. Each failed the acceptance test.

- **A large routing module treated as a god file.** Size alone is not the smell, and the
  real finding about that file is 2.1, since it hosts the wire vocabulary of features
  whose producers never landed. Grounded there instead.
- **Hand-rolled protocol framing duplicated across many files.** Real repetition, but
  the new program's external contract makes the newline the framing, so there is no
  framing layer to hand-roll and no duplicated artifact to find. **The rejection rests
  on the framing rather than on the substrate**, which is what keeps it standing after
  the contract went transport-silent on 2026-08-24. That sweep removed the named
  mechanisms of descriptor passing and process creation, and it did not stop the
  contracts naming a socket: `weaver-harness-spu-decode-contract` section 1 still
  names the socket type as what makes one write one message, leaving which type
  supplies that to the Spec's election.
  The accept-loop half carries and is 3.1.
- **A wire enum restated in three conversion functions.** Guarded duplication in one
  file with a test pinning the pairs. Not the shape that hurt the tree.
- **A required wire field made optional for compatibility.** Promising until the
  handlers were read, which refuse the defaults with an explicit error. Defended, so it
  fails the test. The undefended instance of the same shape is inside 1.4.
- **A composition root emitting session-open and session-close events.** Two events,
  both process bookends, written by the process owning the record's file handle. A
  legitimate near-neighbour of 1.6 rather than an instance of it.

  **The new program faced the question and answered it, which closes this note.** The
  bookends are `load` and `unload`, they bracket the run rather than the process, and
  `session.closed` is a third kind saying the session will not be resumed, which is a
  different claim from a run ending. **The harness authors all three as sole writer**,
  so no component writes its own bookend and the near-neighbour never arises.
- **A crate with both trace calls and prints.** Not the same defect as 2.3, since those
crates do reach the trace and the prints are largely command-line output from binaries.
- **A static authorization memo with no invalidation.** One sighting, bounded, with a
  stability argument that holds on a provisioned box. Security-relevant, not this lens.
- **A task map inserted into and never removed from.** One sighting, and its three
  sibling maps in the same struct all have removal paths, so it reads as one omission
  rather than a pattern. Worth an issue, not a smell.
- **Descriptor custody generally.** A deliberate sweep for raw descriptor conversion,
duplication, close-on-exec handling, and leak-by-forget across all twelve quarry crates
found production code has none of it. **There is no descriptor smell to encode**, worth
recording as a negative result so nobody sweeps for it again.

---

## 5. Positives worth carrying

Not smells. Shapes the quarry got right, recorded so they are not re-derived.

- **Process custody through the service manager.** Workers and gates were transient
  units, and the coordinator held a set of names as intent rather than child handles,
  with the service manager as the source of truth for liveness. That is the correct
  answer to parent-death handling.
- **Destructor-based bracketing with an explicit no-panic contract**, and one engine
  path closing on every exit with the close-then-propagate ordering done deliberately
  and the reasoning written down. Both were single instances in 150k lines, and they are
  the shape the turn bracket needed.
- **Mode plus unconditional credential checking** on the gate's listener, with the
  comment stating that filesystem permissions only admit connection attempts. It is the
  correct use of the permissive mode that 1.8 indicts elsewhere.
- **The comments in that tree were honest.** Every producerless surface announced itself
  in prose above the code. What was missing was any mechanism making an unbuilt producer
  fail rather than merely say so, which is the argument for the compile-time pins and
  perturbation-verified tests the enforcement posture already names.
