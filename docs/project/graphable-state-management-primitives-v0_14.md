---
title: State Management Primitives
doc_id: loop-state-mgmt
version: 0.14
date: 2026-10-15
author: Todd W. Bucy
kind: graphable-document
budget_tokens: 30000
estimated_tokens: 24018
source: State Management Primitives, graphable v0.13, a PRD shape test on v0.13, and a voice session, 2026-10-15 (Bucy, 2026)
source_status: derived
split_of: none
keywords: [state management, loop, buffer, residue, tee, retain, store, sort, place, prefix position, conditioning, simulation, suspend and restore, ledger, cost, meaning protocol]
---

# State Management Primitives
<!-- node: loop-state-mgmt | keywords: state management, loop, buffer, residue, tee, retain, sort, place, prefix position, simulation -->

To simulate intelligence, an agent must manage state. This document states what
managing state means, part by part, and what each way of doing it costs. It also
states the goal it serves, the simulation goal, and why that goal needs state
management.

This document is paper four in a sequence. It walks through the parts of state
management at the grain where one party's duty ends and the next party's duty
starts. It states the cost of each way to manage the buffer. It does not describe
how to build any part. It does not restate paper one.

This document describes the primitives that state management needs. It implies the
loop that composes them and does not study it. The loop gets its own PRD, and later
its own document. Earlier versions carried the title The Loop and State Management.

## Status
<!-- node: loop-state-mgmt/status | keywords: candidate, ruling, figure -->

This is a working draft. It is the v0.5 theory paper rewritten as a graphable
document. The content is the content of v0.5. The wording is controlled.

A ruling is a decision the operator ratified. A candidate is a proposed decision
that is not ratified. The document marks each ruling and each candidate.

A claim with no mark is an unmarked claim. This document asserts it. It is not a
ruling, and it is not a candidate. A PRD may adopt an unmarked claim, but no ruling
binds it. Most of the walk, the ladder, and the costs are unmarked claims.

The ten figures are v0.2 drawings. Some boxes in them carry the label "store". From
v0.14 the store is a term of its own: the organ that retain reaches. Read each box
labeled store as retain and the store it reaches. The redraw is owed.

Version 0.6 adds the premise node. It carries the first cross-document edge in the
corpus, to the node engineering-model/statelessness in The Model, an Engineering
Account.

Version 0.7 answers a deep dive, a critique, and a debate run on v0.6. It opens on
the reason for the paper. It states its audience and what it adds. It adds cut and
cache at rung six. It says what a cut does to the steps after the boundary, and what
prefetch changes. It corrects the step-one formula so the bare arrangement is a
plain chat.

Version 0.8 sets the budget to 30000 tokens, for a 32K window on a large GPU. It
adds the section on simulation before the premise. Each claim in that section is a
candidate from a voice session on 2026-10-15. The section moves the replication
question into this document. Version 0.7 deferred it to the engineering account. The
illustrations are numbered again, so the count in the middle is now Illustration 3.

Version 0.10 starts from v0.8. It takes the engineering corrections of a v0.9 review
and leaves the rest of that review out. It redefines N against the valid prefix of
the engine. It separates a pass from a forward step. It states that the argument and
N both cross the engine interface. It splits conditioning into an execution cost and
a behavioral effect. It adds a fifth strategy at rung six, suspend and restore, as a
candidate. It does not change the strict residue ruling. A proposal to narrow that
ruling is on the owed list as a decision for the operator.

Version 0.11 settles the mechanics and leaves the section on simulation as it was.
It answers a deep dive run on v0.10. The operator affirmed the strict residue ruling
and ruled that the buffer is intermediate computation toward residue. The three
prices now sit on a ledger with two axes, resource and wall time, which are never
summed. Suspend and restore gets two restore disciplines, recompute restore and
cache restore. A reload of saved buffer entries is a loop decision, so the trace
records it. The admission test, the qualifier on conditioning, the conditions on cut
and cache, the deciding pass, the head, and prefetch are each stated so that a
reader cannot flatten them as the deep dive did.

Version 0.12 answers a targeted deep dive and a review, both run on v0.11 with a PRD
framing. It records three rulings of 2026-10-15: N never exceeds the valid prefix,
the unmarked claim is named and nothing new is ratified, and the costs replace the
three prices. It fixes the residue wording at rung four, the definition of sort, the
plane wording of the in-band failure, and the silence on how a step outcome reaches
retain. It names the work that aims at replication. It states that the primitives
exist so that building an agent is composition.

Version 0.13 answers a targeted deep dive on v0.12 and a voice session. It retitles
the document State Management Primitives, by ruling of 2026-10-15. The node prefix
loop-state-mgmt stays, so graph edges hold across versions. Each content node now
opens with a standing line: ruling, candidate, or unmarked claim. The owed list is
split by owner. The one-origin rule is stated as verbs. Two candidates are added:
sort at any crossing, and a classify on write under the same label scheme as the
read. The handoff states the layering of the documents that follow.

Version 0.14 answers a PRD shape test run on v0.13 and a voice session. A PRD drafted
from v0.13 found that no part performed the write into S_R. The operator ruled on
2026-10-15 that the store owns its read and its write. The store is a member of state
management in its own crate. Retain reaches it, as the classifier reaches the SPU.
No primitive is added. The write path is stated in order: the tee filters, the
partition admits, the classifier labels when the declaration wires it, and the store
writes. The label scheme of classify on write is stated as a meaning protocol, by its
properties, with STE100 as the instance this program uses. Rung six now gives the
election of a strategy to the operator, as a sort criterion, in one voice.

The glossary at the end defines each term. One term has one meaning.

## Scope
<!-- node: loop-state-mgmt/scope | keywords: state management, capability, primitive, rung, ladder, replication -->

This document IS a walk through one capability, state management, at the boundary
between duties. It IS a statement of what the capability owes each part. It IS a
ladder of arrangements of those parts. It IS the home for the control loops as
instances of one technique.

This document IS NOT an engineering description of any part. It IS NOT a PRD or a
specification. It IS NOT a second account of what an agent is. It IS NOT a study of
the loop. It IS NOT a guide to assembling an agent. It IS NOT an argument against
replication. When a rung meets an engineering fact, the document states the fact
once as a constraint and moves on.

The capability is realized as primitives in the harness. A PRD charters a primitive.
It does not assemble an agent. The primitives exist so that building an agent is
composition, not construction. When an agent needs a part the harness does not have,
the gap is a feature request against the harness. The agent does not grow a new part
of its own.

## Simulation, not replication
<!-- node: loop-state-mgmt/simulation | keywords: simulation, simulation goal, replication, simulacrum, interpreter, symbol, meaning, holding, candidate -->

Standing: candidate.

This section states which goal the agent serves and why that goal needs state
management. Each claim in this section is a candidate.

### Two goals
<!-- node: loop-state-mgmt/simulation/goals | keywords: replication, simulation goal, simulation, model, candidate | xref: engineering-model/statelessness -->

Standing: candidate.

There are two goals for a machine that does what intelligence does. Replication
builds a mechanism that is an intelligence. The simulation goal builds a mechanism
that produces a simulation of the conduct of an intelligence when a party engages
it.

The 1955 proposal that named the field states its goal in terms of simulation
(McCarthy et al., 1955). The research community later took replication as the goal
with no stated decision. Engineering received the artifact around 2019 and scaled
it. Scale was the correct answer to the cost problem. The cost problem did not
require a choice between the two goals.

Work that aims at replication is a live research front. It puts memory inside the
model. One line learns a memory at test time (Behrouz et al., 2025). One line builds
state into the architecture (Kosowski et al., 2025). One program proposes to replace
next-token prediction with learned world models (LeCun, 2022). That program and this
document agree on the diagnosis: the model holds no world model and no state that
persists. They differ on the goal. Against replication, the missing state is a dead
end. Against simulation, it is the specification of a part. An operator whose goal
is replication should follow that work. The engineering account reviews it (Bucy,
2026).

This document asks a different question: what an operator can build today, with the
tools that exist, on hardware already owned. The answer returns to the stated goal
of the field. The 1955 proposal named simulation. The simulation goal is the
original charter and the practical path at once. This document serves it, and it
states which goal it serves so that a reader can state which goal the reader serves.

### The artifact and the event
<!-- node: loop-state-mgmt/simulation/artifact-event | keywords: simulacrum, simulation, interpreter, loop, model, candidate -->

Standing: candidate.

The simulacrum is the trained artifact at rest. The simulation is the event that
runs when an interpreter engages the simulacrum through a loop. A simulacrum on a
disk simulates nothing. A simulation exists only while the loop runs.

This document takes the word simulacrum from Baudrillard (1981) and narrows it. In
Baudrillard a simulacrum has no use. In this document it is a part.

Thus the simulation is a property of the arrangement, not of the model. The parts
around the model are parts of the simulation. Remove them, and the simulacrum
remains and nothing runs.

> **Illustration 1.** The flight simulator
> <!-- illustration: loop-state-mgmt/ill-1 | type: text | subject: simulation/artifact-event | keywords: simulacrum, simulation, interpreter -->
> A flight simulator in its crate is software and a frame. Nobody is flying. The
> flight starts when the machine powers up and a pilot sits down, and it stops when
> the pilot climbs out. Nobody confuses the crate with an airplane, and nobody calls
> the machine useless because it never leaves the ground. Licensing boards accept
> the hours. Now look at what is in the crate. The picture of the terrain is a small
> part of it. The rest is hydraulics, the motion platform, the instruments, the
> weather the instructor injects, and the failures scripted in. Those parts run
> without the pilot thinking about them. They are the autonomic part of the
> simulation, and most of the simulation is them. Strip them and you have a pretty
> picture that does not fly. A model with no loop around it is the pretty picture.
> One more thing the crate can do: the instructor can freeze the flight, run a
> different approach, and put the pilot back in the frozen cockpit exactly as it
> was. A recording only plays forward. A simulator lets you leave and come back.

### Symbols and meaning
<!-- node: loop-state-mgmt/simulation/symbols | keywords: symbol, meaning, interpreter, technical level, semantic level, model, candidate -->

Standing: candidate.

The builders fit the model to as much text as they could gather. The model is a
fitted distribution over a fixed alphabet of tokens. Training scores the model on
how well it predicts the next symbol. Training does not score the model on meaning.

Meaning is not on the channel. A writer applies meaning when the writer chooses a
symbol. A reader interprets meaning when the reader receives the symbol. The meaning
is held at the two ends. The channel carries only the symbol.

Shannon put meaning outside the engineering problem of communication (Shannon,
1948). Weaver named three levels: technical, semantic, and effectiveness (Weaver,
1949). Next-token prediction is an objective at the technical level. The model is an
artifact of the technical level.

These levels locate the questions. They do not show that the model holds no semantic
structure in its weights. The claim of this section is about where significance is
held, not about what structure the weights contain.

At the semantic level the exchange with a model is one-sided. The model applies no
meaning when it emits a symbol. The only meaning in the exchange is what the
interpreter constructs. No intent exists to check that construction against.

The model emits symbols and simulates meaning. It does not generate meaning. The
common error reads an artifact of the technical level as an achievement of the
semantic level. It then credits the artifact with knowing what its symbols mean.

### Holding meaning still
<!-- node: loop-state-mgmt/simulation/holding | keywords: holding, schema, chat template, outbound sort, serve-side sort, classifier, symbol, meaning, trace, candidate -->

Standing: candidate.

Engineering already holds applied meaning still. A schema fixes the form of a stored
value by a declared type and constraint. The form alone is not the whole meaning. An
integer type does not say whether a count is physical stock or unreserved stock. The
record must also carry its reference, its counting rule, its source, and its time.

The chat template is a schema on emission. The model developer trains the model to
emit role markers, turn boundaries, an end-of-turn token, and tool-call blocks.
Downstream parts then read each field by its position, not by its content.

State management is the third stratum of the same discipline. The schema holds
meaning for stored data. The chat template holds it at the emission boundary. Sort
and place hold it through the loop.

Each slot in a chat template is itself a symbol the model emits. The tokenizer and
the runtime decide whether an emitted sequence is a special token. The model can
still emit a slot in the wrong place, or a malformed tool block. The template does
not check its own use, and nothing in the model detects the error. Thus the holding
sits outside the model. It checks. It does not assume.

The model also sorts symbols inside the pass, by attention. A classifier outside the
model differs in kind. It is declared, versioned, and readable from the trace. The
attention inside the pass is none of these.

> **Illustration 2.** The man at the console
> <!-- illustration: loop-state-mgmt/ill-2 | type: text | subject: simulation/holding | keywords: holding, interpreter, meaning -->
> In the Oz story the great wizard is a man at a console, running smoke and a
> projected head. The people of the city do not want to see the levers. They want
> the wizard, and they are not wrong to want him. The man is no fraud while he knows
> what he is running. He fails on the day he starts to believe the head is him. The
> operator of an agent stands where that man stands. The person at the far end
> experiences being understood. The operator knows the meaning was applied
> downstream, by parts the operator built and can inspect. The illusion is safe
> because the holding is mechanical. It stays safe only while the operator never
> falls for it.

### The requirements of intelligence
<!-- node: loop-state-mgmt/simulation/requirements | keywords: intelligence, candidate -->

Standing: owed.

This node is owed. The opening sentence uses the word intelligence. The operator
names the requirements that the word binds to. Until then, the word has no
definition in this document beyond its use in the opening sentence.

## The premise
<!-- node: loop-state-mgmt/premise | keywords: statelessness, model, agent, state management, in-model branch, out-of-model branch | xref: engineering-model/statelessness -->

Standing: unmarked claim. The premise is drawn from the engineering account.

The model is stateless. Nothing a pass computes stays in the model after the pass
ends. This is an established fact in the field, not a claim of this document.
Several groups saw the same condition and built architectures to treat it. The
engineering account of the model reviews that work (Bucy, 2026). This node draws the
premise from that account, at node engineering-model/statelessness, by a
cross-document edge.

Statelessness has a consequence. An agent that must carry anything from one turn to
the next must get statefulness from somewhere. There are two places to put it. An
in-model branch changes the model so that it carries memory: a recurrent state
inside the architecture, or weights that update during use. Each in-model branch
carries a hardware or compute bill that most operators cannot pay today. An
out-of-model branch keeps the model stateless and puts statefulness in the agent
around it.

This document takes the out-of-model branch, because the model is not the agent.
State management is how an agent gets statefulness from outside a stateless model. A
partition that admits nothing keeps the agent stateless on purpose, so each turn is
a single call. Each arrangement on the ladder buys back part of what the model does
not hold.

### The audience
<!-- node: loop-state-mgmt/premise/audience | keywords: operator, foundry, context bound, wall compaction -->

Standing: unmarked claim.

The largest model vendors grow the window so that compaction comes as late as
possible. That approach is valid. It has made hosted coding tools usable for longer
tasks. It does not remove the wall. It moves the wall, and only a foundry can keep
moving it.

This document is written for an operator who is not a foundry. That operator runs a
model on owned hardware, or rents GPU time by the hour. For that operator the card
fixes the context bound, and each token in the window is paid for on each pass. A
developer who works in a hosted coding tool sees plenty of context, because a
foundry upstream pays for it. On owned or rented hardware, the developer pays.

### What is new and what is not
<!-- node: loop-state-mgmt/premise/fit | keywords: classifier, retain, sort, place, arrangement, ladder -->

Standing: unmarked claim.

No part in this document is new. Classifiers on input and output, re-rankers,
retrieval stores, scratchpads, and summary compaction are in use today. Each tool
adds them in its own way. This document adds the fit. Each part gets one duty, one
boundary, and one contract, and the parts compose as one loop.

A plain chat uses almost none of the parts. A part runs only when the declaration
names it and its condition holds. Read the ladder as the same parts in more complete
arrangements, not as a machine that grows.

## Terms drawn in
<!-- node: loop-state-mgmt/terms-drawn-in | keywords: agent, boundary, loop, model, state management, residue, external input, argument, act, trace, strict residue ruling, pass, forward step -->

Standing: ruling. The strict residue ruling is inherited and was affirmed on
2026-10-15, with the buffer ruling of the same day.

Paper one closes on this form, and this document opens on it:

$$A = B\,[\,\ell\,[\,S_M, M\,]\,] \leftarrow p_{ext} \rightarrow Y, T$$

Read: inside a declared boundary B, the loop orchestrates state management S_M and a
model M. External input p_ext comes in. The agent emits an act Y and a trace T.

This document takes these terms from paper one and does not restate them:

- the boundary B, the loop, and the model M,
- state management S_M, the set of functions retain, sort, and place,
- the residue S_R, the external input p_ext, and the argument p_int,
- the act Y and the trace T.

From the tuple paper it takes the context bound C.

The strict residue ruling is inherited whole. State exists only in the pass. It is
not readable after the pass ends. It belongs to nobody. There is no second state
that belongs to the agent.

What the agent carries from one turn to the next is not state. It is residue.
Residue has two conditions: the partition admitted it, and it is readable from the
trace. Readable alone is not enough. The trace holds everything. Retain holds only
what the partition admitted. The residue arm of the tee admits it, retain holds it,
sort selects it, and place lays it into the next argument. Rungs four to six show
each step.

The trace is the record of the operator. It is never the record of the agent. The
operator reads residue and measurement from the trace, never state. The operator can
also observe a pass while it runs, as diagnostic replay does. That is observation of
a pass in flight. It is not a reading of state after the pass.

The ruling is about a decoder and a trace. It is not a claim about any other kind of
memory.

The operator affirmed the ruling on 2026-10-15 and added one ruling to it. The
buffer is intermediate computation. It comes before residue in the pass, and it is
never residue itself. It is not residue, because it is not readable from the trace.
It is not state, because it outlives the pass. It belongs to the engine. Saved
buffer entries are the same kind of thing.

A pass is one generation call that produces e. A pass can hold a prefill and many
forward steps. A forward step is one forward computation inside a pass. A claim
about timing or intervention names which of the two it uses. The expression M(p_int,
N) in the walk is shorthand for a call to the model through the engine.

## The figure rule
<!-- node: loop-state-mgmt/figure-rule | keywords: figure, declaration, primitive, seam -->

Standing: unmarked claim.

Each rung has one formula line. Each line adds one term. Each term in a line is a
block a builder can place or a wire a builder can draw. A figure is admissible only
when each symbol in it maps to a declaration field or a chartered primitive.

The reason is the builder. A symbol with no block and no wire behind it is a claim
the paper makes in private. The rule catches that claim before a reader does.

## The rulings
<!-- node: loop-state-mgmt/rulings | keywords: ruling, tee, sort, per-turn line, store -->

Standing: ruling, 2026-10-13, and ruling, 2026-10-15.

The operator ratified three rulings on 2026-10-13 and a fourth, on the store, on
2026-10-15. The walk depends on them. Two candidates sit beside them.

### The tee is the seam of the harness
<!-- node: loop-state-mgmt/rulings/tee | keywords: tee, harness, envelope, partition, trace arm, residue arm, trace floor, SPU, classifier, measurement payload, management plane -->

Standing: ruling, 2026-10-13. The narrowing for mechanical content reading is an
unmarked claim.

The tee is active. It reads a stream, filters each event on its envelope under the
partition the operator declared, and forks by result. The tee sits on the arc of the
loop, and the loop belongs to the harness. Thus the tee is the seam of the harness.

The partition reads only the envelope: the event kind, the channel, the origin. The
partition never reads content. A tee that read content would cross the duty boundary
of the tee. Content reading does not always need a model. Parsing, exact match, and
validation are mechanical. A classifier call that uses a model belongs to the SPU,
where each model that does semantic work runs. The reason for one SPU is an
engineering reason. It is in the SPU charter and the handoff, not here.

The tee has two arms. The trace arm receives everything that crossed, labeled. The
trace holds the assembled argument at each position, the seed, the sampler state,
the artifact identity, and the measurement payload. That content is the trace floor.
No partition can cut below the trace floor.

The residue arm receives what the partition admits. The admission authorizes a
write. The store performs the write. What the store holds by that path is S_R.

The measurement payload belongs to the operator. It stays on the trace arm. If the
next turn of the agent reasons about its own entropies, the partition leaked. That
is the in-band management failure: data of the management plane read on the data
plane, with no controlled crossing between them.

### Sort runs twice
<!-- node: loop-state-mgmt/rulings/sort-twice | keywords: sort, outbound sort, serve-side sort, emission, act, tee, retain -->

Standing: ruling, 2026-10-13. The verbs of the write follow the store ruling of
2026-10-15. The generalization to any crossing is a candidate.

The outbound sort runs on the emission e. It decides what part of e becomes the act
Y. The serve-side sort runs on the residue and the new external input. It decides
what goes into the next argument.

Neither sort elects retention. The tee is the only origin of retention. Each event
crosses the tee labeled, and the partition decides what retain holds. One origin,
one filter, no second door.

Stated as verbs: the tee filters the envelope. The partition admits, and the
admission authorizes the write. The loop carries the admitted item to the store. The
store writes it. Retain holds, by way of the store. The sort reads. No sort writes,
the tee writes nothing, and no primitive writes. The store writes.

The walk shows sort at two positions. Two is what the walk uses, not a limit. A sort
can sit at any crossing of the loop under the same rules: it labels and selects, it
holds no motive, and its output reaches retain only through the tee. The decision a
classifier appears to make sits in the label scheme the operator declared. The
classifier applies that scheme. This generalization is a candidate.

The label of the outbound sort crosses the tee too. The operator decides whether the
partition admits it. The default keeps the management-plane envelope out of retain.
If the label goes in, a serve-side sort reads its own prior verdict. That is the
model managing itself one turn late.

### Classify on write
<!-- node: loop-state-mgmt/rulings/classify-on-write | keywords: classify on write, label scheme, classifier, sort, partition, retain, serve-side sort, candidate -->

Standing: candidate. It narrows the reading of the ruling above that the two sorts
run under two criteria.

The order is fixed by the store ruling. The tee filters. The partition admits on
the envelope. The classifier labels each admitted item under a declared label scheme.
The loop carries the labeled item to the store, and the store writes it. The label is
assigned after the filter and before the write, so it never reaches what the
partition reads. The label rides with the item into the store. A label is not an
election. The partition alone decides that a write happens.

The classify of new input on the read side uses the same label scheme. The
serve-side sort joins input to residue by label, so a pairing holds only when both
labels come from one scheme. The criteria of selection on the two sides may differ.
The label scheme is shared.

One scheme is the invariant. One classifier instance or several copies is a choice
of deployment. A classify is small. It runs off the clock of the decoder, so it adds
cost on the resource axis and little wall time. A classify that grows until the
decoder waits on it has become a deciding pass, and the ledger shows it.

### The meaning protocol
<!-- node: loop-state-mgmt/rulings/meaning-protocol | keywords: meaning protocol, label scheme, classify on write, classifier, STE100, candidate -->

Standing: candidate. It states the label scheme of classify on write. The list of
instances is a reading of sources, not a survey.

A classifier can extract only what the emission makes regular. The label scheme
therefore comes from a meaning protocol: an agreed structure for meaning that holds
across a channel where a misreading is likely. The builder chooses the protocol. The
requirement is on its properties, not on its name. Each property earns its place by
what it gives the classifier:

1. Standardized, so the base training of the model covers it and the classifier does
   not learn a private dialect.
2. Short declarative sentences, so one sentence carries one action and the boundary
   of each clause is findable.
3. Classifiable, so the label scheme is finite and declared.
4. A verb bound to its noun, with the condition that governs it, so the read side
   can pair and sort on the verb, the noun, or both.

Technical institutions built such protocols for their own motives. ASD-STE100,
Simplified Technical English, fixes the language of aerospace and defense maintenance
documents. Caterpillar Technical English restricted the vocabulary and grammar of
heavy-equipment service manuals so that machine translation could run on them. The
IMO Standard Marine Communication Phrases, adopted in 2001 from Seaspeak, fix the
language of ships whose crews speak different native languages. A tekom work group
published rules-based authoring for technical German in 2011. The motives differ:
safety, the cost of translation, and communication between crews. The problem is the
same: move technical meaning across a lossy channel without loss. The solution
converged on the same structure. The structure is also classifiable, as a
consequence.

The boundary between the emission and the classifier is such a channel. The same
structure serves it, with no claim that the model shares any motive. This program
uses STE100 as its instance and extends it under its own rules with a compact set of
computing verbs, each with its standard meaning. A deployment may declare another
protocol.

Two limits hold. A meaning protocol here is a one-way discipline of encoding. It
gives regular encoding and decoding of units of meaning. It does not give the
handshake, acknowledgment, or retransmission of a network protocol. And the fit
depends on the work. The work this program targets is technical, long-horizon, and
not conversational, so its acts fall into a controlled set of verbs. A
conversational agent emits open prose, and the classifier cannot extract it as well.
No comparison between protocols has run.

### The store owns its read and write
<!-- node: loop-state-mgmt/rulings/store | keywords: store, retain, write, read, query seam, SPU, crate, Unix socket, deployment tuple | xref: deployment-tuple -->

Standing: ruling, 2026-10-15, that the read and the write belong to the store. The
range of stores and the trade with the meaning protocol are unmarked claims.

A PRD drafted from v0.13 found that no part performed the write into S_R. The tee
writes nothing, no sort writes, and the classifier does not write. The operator
ruled that the read and the write belong to the store. A store that cannot be read
and written is not a store. Each store has its own interface, so a generic service
for the read and the write would fit none of them well. No primitive is added.

The store is a member of state management. It is built as its own crate, behind its
own contract on its own Unix socket. State management reaches the store and does not
contain it, as the classifier reaches the SPU. Retain is the function. The store is
the organ that performs it.

Whether to write is a question for this document. The partition admits, and the
admission authorizes the write. How to write is a question for the store. The loop
carries the admitted item to the socket of the store, in the format the contract of
the store states. The loop does not know which store it is. The store writes. On the
way out, the store serves the query seam with its own read.

The store is a declaration. A markdown file searched by pattern is a store. An
in-memory SQL store is a store. A graph database is a store. The field already uses
each. WeaverTools elects an in-memory SQL store. That is a preference of this program,
not a requirement of this document.

The power of the store and the meaning protocol trade together. A flat file returns
text to scan, so all selection falls on sort. A structured store answers a query over
structured clauses, so part of the selection can move into the store. A meaning
protocol over a flat file leaves its structure unused. A structured store over loose
text has little structure to use. Which stores a deployment can afford depends on
where its boundary sits. A local agent with one owner carries none of the cost of a
store per user held by a provider, so its set of stores is wider. The tuple paper
carries why a boundary admits a store.

### The per-turn line
<!-- node: loop-state-mgmt/rulings/per-turn-line | keywords: per-turn line, place, sort, partition, tee, prefix position, head, tail, candidate, K_valid -->

Standing: ruling, 2026-10-13, with the ruling of 2026-10-15 that N never exceeds the
valid prefix. Each expression for N is a candidate.

The set S_M = {retain, sort, place} does not change. This document adds one per-turn
line. Each figure is drawn against it:

$$p_{int}(t+1) = place_{\{head,\,tail\}}\big(sort(S_R(t),\ p_{ext}(t+1))\big), \qquad |p_{int}| \le C_{head} + C_{tail}$$

$$N(t+1) = lcp\big(p_{int}(t),\ p_{int}(t+1)\big)$$

$$S_R(t) = S_R(t-1) \cup partition\big(tee(p_{ext}(t),\ e(t),\ Y(t),\ d(t))\big)$$

Read: the tee forks everything. The partition admits residue, the store writes it,
and retain holds it. The union in the third line is that write. Sort labels and joins
under a criterion. Place lays a pinned head and a moving tail inside the
bound. Place reports the prefix position N. The term d(t) is the set of decisions of
the loop on the turn.

The operator ruled on 2026-10-15 on the second line. N never exceeds the length of
the prefix for which the engine holds valid entries, K_valid(t). A value too low
costs a recompute. A value too high reuses entries that are not valid. The exact
expression for N is owed. The second line above is the expression of 2026-10-13. It
satisfies the ruling, but it never counts the emission, so it reports N too low. Two
candidate expressions follow. The first measures N against K_valid(t):

$$N(t+1) = lcp\big(K_{valid}(t),\ p_{int}(t+1)\big)$$

The engine can restrict reuse further, by block boundaries or by what its cache
still holds. Valid entries also need compatible conditions: the same model, the same
adapters, the same positional treatment, and the same non-text inputs. The last
emitted token can have no entry yet, because the pass stopped before it was
computed.

Where K_valid(t) is the argument followed by the emission, the line simplifies:

$$N(t+1) = lcp\big(p_{int}(t) \parallel e(t),\ p_{int}(t+1)\big)$$

The symbol for concatenation is the double bar. The rest of the walk uses the
simplified line under that condition. The simplified line is a candidate expression.
It is not the ruling. A requirement may rely on the ruling alone.

The buffer is outside the line. It is the primitive of the engine. State management
supplies p_int and controls prefix reuse through N. It reaches the buffer by no
other path.

The external input is what crosses the gate on a turn: a user prompt, a tool return,
a task charter from the operator. The load-time declaration is not external input.
It crosses the admin door and lives in B.

Sort over external input means label and join, not judge. The gate refuses nothing
on content. Inside the boundary, nothing drops external input unless the declaration
says so.

## The costs
<!-- node: loop-state-mgmt/prices | keywords: cost, recompute, conditioning, deciding pass, selection, storage, I/O, buffer, fourth duty, SPU -->

Standing: ruling, 2026-10-15, that the costs replace the three prices. The account
of each cost is an unmarked claim.

Each strategy to manage the buffer has costs. The operator ruled on 2026-10-15 that
each cost is named on the page, because each place a byte lands or moves is a cost
the engineer pays. Three costs belong to the buffer: the recompute, the
conditioning, and the deciding pass. Three costs belong to carrying residue across
turns: selection, storage, and I/O. The walk names each cost where it applies. This
document puts the costs on the page. It does not elect a strategy.

The recompute is what the engine pays when N moves back and the engine computes each
position after N again.

The conditioning is what each pass pays for each token in the buffer. It has two
parts, and the two are measured apart. The execution cost is the compute and memory
to process and attend to the supplied context. The behavioral effect is what that
context does to the decision. The behavioral effect is a measurement. Length does
not establish it, and no strategy in this document assumes it. Position and material
that bears on nothing can lower use (Liu et al., 2024, Shi et al., 2023). Paper one
states this cost as the fourth duty.

The deciding pass is what a loop pays when a model must read the buffer and decide
what stays. That is semantic work. It runs on the SPU with its own tokens and its
own place in the timing of the loop. Selection is a different cost. Sort selects
from retain on each turn, by a model or by mechanical rule. The deciding pass reads
the buffer. Selection reads retain. The two are counted apart.

Storage is the capacity the operator must provision, at a given class, to hold
residue or saved buffer entries. The class sets how fast the bytes come back. A fast
class costs more and stands ready whether or not a restore runs this turn.

I/O is the time to move bytes from storage into memory or onto the card. While the
bytes move, the decoder waits. Storage and I/O are a coupled trade: the operator
buys a faster class of storage to hold the I/O time down.

Each strategy is compared on the costs. A cost can be zero for one operation. A
strategy often moves a cost to another place or another time. This document gives no
proof that all the costs cannot be low at once. A cheap lookup is not evidence that
the decision to look was cheap.

### The ledger
<!-- node: loop-state-mgmt/prices/ledger | keywords: ledger, resource, wall time, storage, deployment tuple, operator -->

Standing: unmarked claim.

Each cost is read on two axes. The resource axis is what the operator spends: the
compute of a recompute, the execution cost of conditioning, the tokens of a deciding
pass, the work of selection, and storage at its class. The wall-time axis is the
time a cost adds on the critical path, where the decoder waits: the wait for a
recompute, the I/O time, and the wait for a deciding pass. The behavioral effect of
conditioning sits on neither axis. It is a measured effect on the decision. The two
axes are never summed. A strategy can lower one axis and raise the other.

The deployment tuple decides which axis an operator feels. On owned hardware the
resource axis sits near its floor, because the card, the memory, and the disk are
paid for. What remains is mostly wall time. On rented hardware the resource axis is
metered by the hour. Through a hosted API the resource axis is metered by the token,
and the provider decides much of what is cached. The same strategy on the same task
has a different cost on each.

Thus the true cost of an agent is a ledger, not one number. Each strategy states its
cost on both axes, and the operator reads the axis the tuple makes bite. The
crossover between two strategies is a measurement for each tuple.

> **Illustration 3.** The count in the middle
> <!-- illustration: loop-state-mgmt/ill-3 | type: text | subject: prices | keywords: buffer, conditioning, step outcome -->
> An agent works a task with many steps. Early on it takes an inventory, and the
> count it finds goes into the window. It is a number and a few lines, nothing a
> model could fail to read. Then come many turns of other work: tool calls,
> reasoning, small jobs opened and closed, none of them about the count. At last a
> decision arrives that depends on the count. The decision sits at the tail. The
> count sits in the middle, with all that other work between them. The model writes
> a plan that ignores the count. Nothing was lost. The count was in the argument at
> the forward pass. It just was not used. Every reader has seen this one. Later
> illustrations come back to it rung by rung.

## The rung template
<!-- node: loop-state-mgmt/rung-template | keywords: rung, pass, figure -->

Standing: unmarked claim.

The walk goes from the pass outward. The pass is the one moment when the strict
residue ruling allows the word state.

Each rung has one figure, one paragraph, and one return to Illustration 3. The
paragraph has four parts in fixed order: what the pass left, who holds it, what
state management owes it, where the line is. The return applies the move that the
rung makes possible.

The walk is a logical order, not a clock. Loop work runs at the same time as the
emission. The order in which code runs is a matter for the harness PRD.

## The walk
<!-- node: loop-state-mgmt/walk | keywords: rung, pass, buffer, argument, emission, tee, retain, sort, place -->

Standing: unmarked claim.

### Rung one. The buffer
<!-- node: loop-state-mgmt/walk/rung-1 | keywords: buffer, prefix position, recompute, cut, append, membership, management, wall compaction, deciding pass, conditioning, recurrent state -->

Standing: unmarked claim, with the buffer ruling of 2026-10-15.

What the pass left: a buffer in device memory. It holds the attention entries for
each position the engine computed. It is valid as a prefix and nothing else.

Who holds it: the engine. The code of the runner built it at load. It is not in the
tensors. It is not readable from the trace. Thus it is not S_R. Paper one called the
buffer residual state. That test was too loose and this document corrects it. The
test here is readable from the trace. The buffer is recomputable from the trace,
because the trace holds p_int at each position and the artifact identity. The buffer
is not readable from the trace, because its bytes are a fact about a device.
Recomputable and readable are two properties. Residue has the second. The buffer is
intermediate computation, by the ruling under Terms drawn in. Residue is a different
thing: an admitted record, readable from the trace.

Management is not membership. State management manages the buffer through N and the
argument it supplies. Place is a buffer management discipline, because each write to
the argument decides what in the buffer survives. But the buffer is not in S_M,
because S_M is a set of functions and the buffer is a buffer. The buffer is not in
S_R, because S_R is readable from the trace and the buffer is not. To manage a thing
is not to make it a member.

What state management owes it: one number, N, with the argument. N is the position
from which the next pass recomputes. The argument p_int crosses the engine interface
too. N is the only control state management has over the buffer.

Where the line is: at N. Below N the engine keeps. Above N the engine recomputes.
The buffer is valid only as a prefix, so there is no middle swap. Two operations
exist: append, and cut at N then recompute forward.

At this rung the loop has no label for a step boundary. Thus a cut has no target,
and append is the only free operation. The one strategy available is wall
compaction. The loop waits until the buffer is full. A deciding pass reads the whole
window and writes a summary. N moves back to the head. The loop resumes on the
summary.

Wall compaction pays each buffer cost in full. It pays the recompute once, at the
wall. It pays the conditioning on each pass until the wall. Its deciding pass reads
the largest window the task will have. No step boundary guides it. The stale middle
it must clear also conditions it. The loop stops while the pass runs. This document
records wall compaction as the limit of a loop with only wall compaction, not as a
fault of any deployment.

The middle gets in the way. The cut is the remedy. The recompute is how the remedy
takes effect. Rung one cannot elect the cut. The rest of the walk supplies the means
to elect it.

One constraint applies to engines whose buffer holds a recurrent state. That buffer
is still valid only as a prefix. It is recomputable from the trace only when the
tuple fixes the dtype and rounding of the state. The tuple paper owes that entry.

$$e(t) = M\big(p_{int}(t),\ N(t)\big)$$

> **Illustration 4.** Figure 1, the engine and its buffer
> <!-- illustration: loop-state-mgmt/ill-4 | type: png | subject: rung-1 | keywords: buffer, prefix position -->
> File: figures/fig-01-buffer.png
> The box is M. The upper arrow in is p_int(t). The blue arrow in and the blue mark
> on the bar are both N(t). The arrow out is e(t). The bar is the buffer: kept left
> of the mark, recomputed right of it.

> **Illustration 5.** The count at rung one
> <!-- illustration: loop-state-mgmt/ill-5 | type: text | subject: rung-1 | keywords: wall compaction, append -->
> Back to Illustration 3. The count went in at the inventory turn, and every token
> since has been appended after it. Nothing can say the inventory step is done, so a
> cut would be a guess. The loop can only wait for the wall, then summarize
> everything at once. If the count survives that summary, it is luck.

### Rung two. The argument
<!-- node: loop-state-mgmt/walk/rung-2 | keywords: argument, head, tail, context bound, boundary, task charter -->

Standing: unmarked claim.

What the pass left: nothing. The pass read p_int and did not change it.

Who holds it: place assembled it, the loop handed it on, the engine read it. After
the pass, only the buffer prefix and the trace record hold it.

What state management owes it: its composition, a head and a tail. The head is
pinned. The share of B comes first. The task charter comes second, if there is one.
The tail is everything after. The tail is the only part that moves.

The share of B is what the declaration fixed at load. The task charter is what the
operator pushed for this task through the gate. Neither is text the user typed. The
tuple, the boundary, and the charter are three things, not one. The head is not one
system prompt. It has two parts with two origins: the declaration at load, and the
operator for each task. A change to either has its own door and its own record.

Where the line is: between head and tail, at a position the trace knows. The tuple
fixes the bound C for the loaded artifact. The declaration splits C into C_head and
C_tail. A larger window raises C. It adds nothing to the content.

$$p_{int}(t) = head \parallel tail(t), \qquad |head| \le C_{head}, \quad |tail(t)| \le C_{tail}$$

> **Illustration 6.** Figure 2, the argument
> <!-- illustration: loop-state-mgmt/ill-6 | type: png | subject: rung-2 | keywords: head, tail, context bound -->
> File: figures/fig-02-argument.png
> The whole bar is p_int(t). The two shaded parts are the head, B's share first and
> the charter second. The blue line is the head and tail boundary. The braces are
> C_head, C_tail, and their sum C.

> **Illustration 7.** The count at rung two
> <!-- illustration: loop-state-mgmt/ill-7 | type: text | subject: rung-2 | keywords: head, tail -->
> The head says what the task is. It says nothing about the count, and that is
> right: the count is something the task produced, not something the task was given.
> So it lives in the tail. C_tail decides how soon the wall comes. A bigger window
> pushes the wall back. It does not move the count out of the middle.

### Rung three. The emission
<!-- node: loop-state-mgmt/walk/rung-3 | keywords: emission, act, outbound sort, splice, append, prefix position -->

Standing: unmarked claim.

What the pass left: a token sequence e, and the buffer extended through it.

Who holds it: the loop, which reads it and calls the next seam.

What state management owes it: the outbound sort, which decides what of e becomes Y.
A splice loop that refits e at a position is this sort under the criterion of an
oracle.

Where the line is: between e and Y. The decoder owns e. The agent owns Y. A
criterion someone wrote is the difference.

An edit to the act Y alone does not change the buffer. The prefix stays valid. If
the edit also changes the next argument, first at position k, reuse ends at k. Thus
N on the next turn is at most k. Offered material appends. Binding material that
rewrites the feedback splices. Append leaves N at full length. Splice sets N at the
splice.

$$Y(t) = sort_{out}\big(e(t)\big), \qquad N(t+1) \le k \text{ when } p_{int}(t+1) \text{ first differs from } K_{valid}(t) \text{ at } k$$

> **Illustration 8.** Figure 3, the emission
> <!-- illustration: loop-state-mgmt/ill-8 | type: png | subject: rung-3 | keywords: emission, outbound sort, splice -->
> File: figures/fig-03-emission.png
> Both rows show p_int(t) with e(t) after it. The diamond is the outbound sort. The
> arrow out is Y(t). Upper row: append, N at full length. Lower row: splice at k,
> the dashed box is the refit, and N moves back to k.

> **Illustration 9.** The count at rung three
> <!-- illustration: loop-state-mgmt/ill-9 | type: text | subject: rung-3 | keywords: emission, outbound sort -->
> The plan that ignores the count is an emission. This is where the symptom shows
> up, and the cause is three rungs away. A calculator-style splice cannot fix it.
> Arithmetic cannot supply a premise the pass never used. An oracle that held the
> count could reject the plan, but that is a different contract.

### Rung four. The tee
<!-- node: loop-state-mgmt/walk/rung-4 | keywords: tee, partition, trace arm, residue arm, trace floor, envelope, classifier, SPU -->

Standing: unmarked claim, under the tee ruling.

What the pass left: the external input, the emission, the act, the outbound label,
and the decisions of the loop. Each becomes residue only if the partition admits it.

Who holds it: the harness, at its seam.

What state management owes it: a declared partition on the envelope.

Where the line is: the tee is the line. It is the plane boundary. The trace arm
carries everything to the trace, at the trace floor. The residue arm carries what
the partition admits. The admission authorizes the only write into S_R, and the store
performs it.

The tee is not a classifier. It reads the envelope and never the content. The tee
does not retain. It holds nothing. It forks, and the two arms hold.

$$tee\big(p_{ext}(t),\ e(t),\ Y(t),\ d(t)\big) \rightarrow \big(T,\ partition(\cdot)\big)$$

> **Illustration 10.** Figure 4, the tee
> <!-- illustration: loop-state-mgmt/ill-10 | type: png | subject: rung-4 | keywords: tee, trace arm, residue arm, partition -->
> File: figures/fig-04-tee.png
> The arrow from the left carries the four arguments. The black node is the tee. The
> dashed arm up goes to the trace, with no partition mark. The solid arm down passes
> the partition box and enters retain (the box labeled store).

> **Illustration 11.** The count at rung four
> <!-- illustration: loop-state-mgmt/ill-11 | type: text | subject: rung-4 | keywords: partition, residue arm -->
> The count crossed the tee at the inventory turn. If the partition admits that kind
> of event, the count becomes residue and gets an address that is not just a spot in
> the buffer. If not, the count lives only in the trace and the buffer, and every
> fix past rung one is closed. To know whether this agent can be helped at all, read
> its partition.

### Rung five. Retain
<!-- node: loop-state-mgmt/walk/rung-5 | keywords: retain, residue, query seam, context bound -->

Standing: unmarked claim.

What the pass left: now held. S_R is what retain holds. It accrues across each turn
since load. It is what the partition let through.

Who holds it: retain, by way of the store. Retain holds no criterion. Retain is the
function, and the store is the organ that performs it. A database, a directory, or a
file can be the store. The store owns its write on the way in and its read on the
way out. The deployment declares which store it is. S_R is what retain holds. The
symbol S_R does not name the function.

What state management owes it: nothing on the way in beyond the residue arm and the
carry of the loop to the socket of the store. On the way out, a query shape the serve
side can call, which the store answers with its own read. That shape includes the lookup
from a return to its call that rung six needs.

Where the line is: between the everything of the trace and the reach of retain. The
operator drew it at the tee. S_R has no context length. The bound is on p_int, not
on retain. A residue that grows for a year changes nothing about C. It changes how
much the serve-side sort must select from.

$$S_R(t) = S_R(t-1) \cup partition\big(tee(p_{ext}(t),\ e(t),\ Y(t),\ d(t))\big)$$

> **Illustration 12.** Figure 5, retain
> <!-- illustration: loop-state-mgmt/ill-12 | type: png | subject: rung-5 | keywords: retain, residue arm, query seam -->
> File: figures/fig-05-retain.png
> The line at the top is the loop with the tee on it. The dashed arm up is the trace
> arm. The vertical arm passes the partition and enters the large box, which is
> retain (labeled store). The arrow out to the left is the query.

> **Illustration 13.** The count at rung five
> <!-- illustration: loop-state-mgmt/ill-13 | type: text | subject: rung-5 | keywords: retain, reminder -->
> Retain now holds the count. For the first time something can reach the count
> without the model having to attend across a long buffer. That opens a second fix:
> lay the count again at the tail, next to the decision. The fix is possible here
> but not yet electable, because nothing at this rung picks the count out or knows
> the decision needs it.

### Rung six. Sort and place
<!-- node: loop-state-mgmt/walk/rung-6 | keywords: serve-side sort, place, classifier, renderer, SPU, step boundary, step outcome, reminder, cut, conditioning, recompute, deciding pass, wall compaction, fourth duty, suspend and restore, branch, recompute restore, cache restore, storage, wall time, trace -->

Standing: unmarked claim. Suspend and restore and both its disciplines are
candidates. The table is not a ranking.

What the pass left: a query result from S_R and the new external input. Neither is
arranged.

Who holds it: sort runs by a model or by a mechanical rule, and the declaration says
which. When sort uses a model, the classifier performs it on the SPU, over the
classify contract. The renderer performs place. The harness hosts the renderer. No
document charters the renderer yet.

What state management owes it: a criterion for sort and an order for place.

Sort labels the external input by envelope. Sort joins it to the residue it answers.
Sort selects from the residue when the residue does not fit C_tail. Sort does not
drop external input unless the declaration wires a classifier that may drop. Place
puts the head first, then the tail. Place reports N.

Sort can also label a step closed and name its step outcome. The label and the outcome
are decisions of the loop. They cross the tee in d(t), and the partition decides whether
retain holds them. Sort does not write retain. With that label, the operator can declare
one of five strategies as a sort criterion. Suspend and restore has two restore
disciplines, so it takes two rows. Wall compaction from rung one is the last row. This
document elects no default. The columns carry all six costs: the recompute, the
conditioning, the deciding pass, selection, storage, and the waits. The entries are
qualitative and assume the engine and task of the walk. They are not measured rankings.
The wait column is the wall-time axis of the ledger, and it names which wait applies.
The other columns are on the resource axis.

| Strategy | Recompute | Conditioning | Deciding pass | Selection | Storage | Wait (recompute, I/O, deciding pass) |
|---|---|---|---|---|---|---|
| Leave in place | none | stays, effect measured | none | none | none new | none |
| Reminder at the tail | none, if appended | stays, plus a duplicate | none | detect need, look up | none new | lookup, unless prefetched |
| Cut at the step boundary | from the boundary | intermediates not supplied | one step | boundary and outcome | none new | the recompute |
| Cut and cache | from the boundary | depends on retrieval | one step | outcome, later lookup | residue in retain | recompute, lookup |
| Recompute restore | the branch, at each wake | live branch only | none | branch labels, ordered lookup | residue in retain | the recompute |
| Cache restore | none for the saved span | live branch only | none | branch labels, saved span | saved buffer entries, fast class | I/O of the load |
| Wall compaction | the window, at the wall | full until the wall | the whole window | none | none new | the loop stops |

Leave in place costs nothing now and keeps the full conditioning. The material stays
at its position. Whether that position lowers use is a measurement.

The reminder lays the material again at the tail. It spends a duplicate in C_tail.
It keeps the intermediates in the buffer, so the conditioning stays.

The cut lays the step outcome where the step closed, lays the decision after it, and
cuts N there. The recompute is short only when the rebuilt tail is short. Later
passes cost less when the shorter context saves more than the added selection work.
The deciding pass reads one step, guided by a boundary the task named. The
intermediates leave the buffer. Retain still holds what the partition admitted, so
the cut removes material from the buffer, not from the agent. The loop pays both
costs before it knows whether the conditioning hurt. A cut removes material from the
pass. It does not show that the decision improves. That is a measurement.

Cut and cache joins the cut to the reminder. Every claim about it holds only under
two conditions: the partition admitted the material, and sort selects it when a
decision needs it. The tee cached each intermediate in retain on its own turn, if
the partition admitted it, so the cut writes nothing new. Retention and the later
lookup still cost something. The cut clears the buffer to the step boundary. When a
later decision needs cut material, the serve-side sort selects it from retain and
place lays it at the tail. The buffer holds the working set, and retain holds the
rest. On a fixed card, a long task can then continue inside the bound when each
working set fits and selection keeps pace. That is a condition, not a guarantee. The
limit is the serve-side sort. If sort does not select cut material when a decision
needs it, the agent acts as if the material were gone. That is a measurement.

A cut to an early step boundary does not discard the steps after it. Sort labeled
each later step closed and named its outcome, and retain holds each outcome. Place
lays the outcomes that the next decision needs after the boundary. The recompute
covers those outcomes and the decision, not the intermediates of each step. This
holds when the partition admitted the outcomes and the evidence they need. Selection
and summary can still omit a detail a later decision needs.

Suspend and restore is a candidate. It uses the cut to move between threads of one
task, not to shorten the buffer. Sort labels a span of turns as a branch. When the
task turns to another thread, the loop suspends the live branch. Place cuts N to the
position where the branch began and lays the other thread from there. Retain still
holds the suspended branch, because the tee admitted each of its turns as it
crossed. When the task returns, sort selects the suspended branch, and place lays it
back in its original order at its original position.

One mechanic gives two properties. The agent holds more than one thread of work, and
the interpreter sees one continuous task across them. Both follow from one fact. The
buffer holds one prefix, so exactly one branch is live. The others wait. A restore
rebuilds the prior argument in place, so the seam does not show.

A restore does not reload state. State ended with each pass. A restore rebuilds the
buffer for the branch by one of two disciplines.

Recompute restore keeps only residue. The engine recomputes the branch forward from
the cut at each wake. A suspend costs almost nothing, because the tee already wrote
each turn to retain. A wake pays the full recompute on the resource axis. On the
wall-time axis the recompute is work of the engine, done at its own tempo.

Cache restore keeps the buffer entries the engine computed for the branch. At the
wake the engine loads them in place of a recompute. A suspend costs storage for the
saved entries, which are large next to the residue. A wake pays a transfer. While
the entries load, the decoder waits, so the load is on the critical path. Cache
restore trades recompute on the resource axis for storage and a wait on the
wall-time axis. Hosted providers ship prefix caching today, to reuse one shared
prefix across many callers. Cache restore points the same mechanism at the branches
of one agent.

The election between the two is a crossover. It depends on the length of the branch,
the time it sleeps, where the saved entries live, and the bandwidth back to the
card. It also depends on the tuple. On owned hardware storage is near its floor and
the question is the wait against the recompute. On rented hardware the saved entries
cost rent while they sleep. Through a hosted API the provider decides what is
cached, and the operator cannot elect a splice point. Cache restore at chosen splice
points needs an operator who holds the device. The crossover is a measurement for
each tuple. This document claims no winner.

Saved buffer entries are intermediate computation, by the ruling under Terms drawn
in. They belong to the engine. They are not residue, and the trace does not hold
them. The decision to use them is a decision of the loop. It crosses the tee, so the
trace records which branch was suspended, where, and whether each wake was a
recompute restore or a cache restore of a named span.

Replay from the trace depends on that record. A recompute restore replays from the
trace alone under a fixing tuple, because the trace holds the argument at each
position. A cache restore replays exactly only when the saved entries equal what a
recompute yields under the tuple. The order of computation can change the bits. Thus
an operator who must replay a cache-restore run later keeps the consumed entries
with the trace, and the state of retain that sort read. That is a storage cost, and
the operator elects it per run. How a run is kept for replay is owed to the PRD.

A restore is faithful only when the partition admitted every event in the branch. If
the partition dropped an emission, the restore rebuilds a different argument, and
the continuity fails at that point.

The admission test decides where each discipline lives. Recompute restore needs sort
to label spans and place to lay retained material in order. If the query seam of
retain can return a branch in order and by position, recompute restore loads from
its declaration and belongs on the ladder. Cache restore reaches the buffer, which
is the primitive of the engine. If the engine exposes a call that loads saved
entries, the declaration elects it and cache restore belongs on the ladder. If the
harness must reach into the buffer itself, cache restore is a new mechanic and
belongs in the walk. Until those readings are made, the test has not run. Neither
discipline has failed it.

The motive for a suspend and a wake lives in the loop. The motive is a better
simulation: one continuous task across threads that the buffer could never hold at
once. The primitives do not know that motive. They retain, sort, and place.

This document does not say which strategy to elect. The choice is a sort criterion
the operator declares. It depends on how much material is in the middle and how much
it pulls. The operator will sometimes choose wrong. The trace shows afterward
whether the choice was right. A document that fixed the line would hold a motive,
and a capability holds none.

Where the line is: between selection and arrangement. Two primitives, two contracts.
Place has two things to serve, the prefix and the position effects. The weight
between them is a measurement.

$$p_{int}(t+1) = place_{\{head,\,tail\}}\big(sort(S_R(t),\ p_{ext}(t+1))\big), \qquad N(t+1) = lcp\big(p_{int}(t) \parallel e(t),\ p_{int}(t+1)\big)$$

> **Illustration 14.** Figure 6, the serve side
> <!-- illustration: loop-state-mgmt/ill-14 | type: png | subject: rung-6 | keywords: serve-side sort, classifier, renderer -->
> File: figures/fig-06-serve.png
> The bar on the left is the gate, with p_ext(t+1) crossing it. The first box is the
> classifier, which is sort, fed from retain below. The second box is the renderer,
> which is place. The blue arrow out carries p_int(t+1) and N(t+1).

> **Illustration 15.** The count at rung six
> <!-- illustration: loop-state-mgmt/ill-15 | type: text | subject: rung-6 | keywords: step outcome, reminder, cut -->
> Now sort can join the count to the decision, and it can mark the inventory step
> closed with the count as its outcome. Four choices. Leave it: free, and the plan
> ignores the count again. Remind: copy the count to the tail, cheap, but all the
> wandering is still in the buffer pulling on the pass. Cut: go back to where the
> inventory closed, put the count and then the decision there, recompute the short
> tail, and the wandering is gone. Cut and cache: the same cut, but every other step
> that closed along the way left its result in retain. Sort still has to pick what
> the decision needs. If the partition admitted it, it is there. It just is not in
> the buffer until something asks for it. Somebody has to choose, and sometimes they
> will choose wrong.

> **Illustration 16.** The count and the second thread
> <!-- illustration: loop-state-mgmt/ill-16 | type: text | subject: rung-6 | keywords: suspend and restore, branch, step outcome -->
> Change the story. After the inventory, the agent starts a second job and works it
> for twenty or thirty turns. Then a task comes up that needs the count. Cut and
> cache would cut back to the inventory and pull in what the task needs. But the
> second job is not finished, and nobody wants to throw it away. So the loop
> suspends it. Retain already holds every turn of it, because the tee wrote each one
> as it crossed. The loop cuts to just after the inventory, lays the open tasks
> there, and the agent sees the count right beside the task that needs it. It does
> that work. When the second job comes back up, the loop cuts to the same spot, lays
> the suspended turns back in their old order, and the engine recomputes them, or
> loads the entries it saved for them. The agent picks up the second job where it
> left off. Nothing was frozen in between. The job was rebuilt from its record. From
> the outside it looks like one agent that never lost its place. Underneath, only
> one thread was ever in the buffer at a time.

### Rung seven. Back to the buffer
<!-- node: loop-state-mgmt/walk/rung-7 | keywords: prefix position, circuit, buffer, per-turn line -->

Standing: unmarked claim.

What the pass left: p_int(t+1) and N(t+1).

Who holds it: the loop. It hands p_int to the engine with N.

What state management owes it: nothing more. The circuit is closed.

Where the line is: at N again. N is the only control state management has over the
buffer. The argument p_int is what it supplies. The cost of the recompute is a
measurement for the harness PRD.

$$\begin{aligned}
e(t) &= M\big(p_{int}(t),\ N(t)\big)\\
Y(t) &= sort_{out}\big(e(t)\big)\\
S_R(t) &= S_R(t-1) \cup partition\big(tee(p_{ext}(t),\ e(t),\ Y(t),\ d(t))\big)\\
p_{int}(t+1) &= place_{\{head,\,tail\}}\big(sort(S_R(t),\ p_{ext}(t+1))\big)\\
N(t+1) &= lcp\big(p_{int}(t) \parallel e(t),\ p_{int}(t+1)\big)
\end{aligned}$$

Each symbol in the circuit is a block or a wire. The engine receives p_int and N and
emits e. N is the control over prefix reuse, not the whole interface.

> **Illustration 17.** Figure 7, the full circuit
> <!-- illustration: loop-state-mgmt/ill-17 | type: png | subject: rung-7 | keywords: circuit, tee, outbound sort, serve-side sort -->
> File: figures/fig-07-circuit.png
> Start at the engine box. The arrow out is e. The diamond is the outbound sort, and
> Y leaves to the boundary. The tee forks: dashed to the trace, solid through the
> partition into retain (labeled store). Retain feeds the classifier, the gate feeds
> it too, the renderer follows, and the blue arrow back to the engine carries
> p_int(t+1) and N(t+1). The outer box is B.

> **Illustration 18.** The count at rung seven
> <!-- illustration: loop-state-mgmt/ill-18 | type: text | subject: rung-7 | keywords: prefix position, cut, reminder -->
> Three endings. Leave or remind: N stays at full length and the engine computes
> only what was added. Cut: N sits at the step boundary and the engine recomputes a
> short tail with the count, the decision, and nothing between. In every case the
> argument and N crossed, and nothing else. Where state management put the count is
> one lever on whether the plan uses it. What sort selected and what the record kept
> about the count are the others.

## The ladder
<!-- node: loop-state-mgmt/ladder | keywords: ladder, arrangement, admission test, declaration, harness -->

Standing: unmarked claim.

The walk fixes the mechanics once. The ladder shows arrangements of them. Each step
uses the same primitive set with different wiring. Each step loads from its
declaration with no code change.

That is the admission test. A step that needs a harness edit is a new mechanic. It
belongs in the walk, not on the ladder.

The test classifies an arrangement. It is not a rule that a builder passes or fails.
A builder can edit the harness to save a recompute. The edit is then a new mechanic,
and the arrangement is off the ladder.

The trace stays true after such an edit. It records what ran. What goes out of date
is the declaration, because it no longer describes the behavior the trace shows. The
gap between the declaration and the trace is where the edit becomes visible.
Enforcement is a deployment concern.

### Step one. The bare arrangement
<!-- node: loop-state-mgmt/ladder/step-1 | keywords: arrangement, partition, trace arm, residue arm, place, retain -->

Standing: unmarked claim.

The partition admits the external input and the act of each turn, and nothing else.
Sort applies no criterion beyond turn order. Place lays the earlier turns in order
and the new input after them. This is a plain chat. No classifier runs. When the
turns fill C_tail, this step meets the wall of rung one.

A partition that admits nothing is the limit case below this step. Each turn is then
a single call with no history. Neither case is the absence of state management. Each
is a partition declared to admit a stated set. To learn whether a deployment retains
anything, read the partition, not the code.

$$S_R(t) = S_R(t-1) \cup \{p_{ext}(t),\ Y(t)\}, \qquad p_{int}(t+1) = place_{\{head,\,tail\}}\big(S_R(t) \parallel p_{ext}(t+1)\big)$$

$$\text{limit case: } S_R(t) = \varnothing \Rightarrow p_{int}(t+1) = place_{\{head,\,tail\}}\big(p_{ext}(t+1)\big)$$

> **Illustration 19.** Figure 8, the bare arrangement
> <!-- illustration: loop-state-mgmt/ill-19 | type: png | subject: step-1 | keywords: arrangement, trace arm -->
> File: figures/fig-08-ladder-1.png
> The drawing shows the limit case, a partition that admits nothing: retain, the
> partition, and the classifier are gone, and the gate feeds the renderer directly.
> The tee keeps its trace arm, because the trace floor is not an election. A drawing
> of the plain chat is owed.

### Step two. The tool-return join
<!-- node: loop-state-mgmt/ladder/step-2 | keywords: tool-return join, classifier, retain, place, partition -->

Standing: unmarked claim.

One classifier, one query on retain. The partition admits emissions and tool
returns. When a tool return arrives, the classifier reads its envelope, labels it as
a tool return, and queries retain for the call it answers. Place lays the return
beside its call. Without this step, a return lands at the tail with no context,
turns after the call.

This step does not judge the return. Sort here labels and joins. A deployment that
refuses a return must wire a classifier that may drop. That is step three.

> **Illustration 20.** Figure 9, the tool-return join
> <!-- illustration: loop-state-mgmt/ill-20 | type: png | subject: step-2 | keywords: tool-return join, classifier -->
> File: figures/fig-09-ladder-2.png
> This is Figure 7 with sort as one classifier under one criterion, the join. The
> query from retain (labeled store) is the lookup from a return to its call.

### Step three. The layered arrangement
<!-- node: loop-state-mgmt/ladder/step-3 | keywords: safety classifier, prefetch organ, sort, place, measurement payload, management plane, append -->

Standing: unmarked claim.

A safety classifier sits before the join. A prefetch organ sits on the serve side.
Two classifiers are composed, and one of them may drop under declaration. The safety
classifier reads content and may refuse to pass it. Because the drop is declared,
the trace can point at it.

The drop belongs to sort, never to place. Place lays what sort passes. A place that
cannot lay something fails. It does not drop.

The prefetch organ runs the serve-side sort on a signal from the live emission. It
fills candidate bins from retain while the decoder works. It releases under append,
so N stays at full length. The signal is read on the management plane, as part of
the measurement payload. The signal never reaches the prompt. Only the candidates
do. Prefetch does not hide latency. It changes what is present at the decision, not
what is retained. A candidate that prefetch releases before the decision turn is in
the argument at that pass. Without prefetch, the same candidate arrives at the next
serve-side sort, one turn late.

Both are instances of sort at rung six. Neither is a new mechanic.

$$sort = sort_{join} \circ sort_{safety}, \qquad sort_{safety}: p_{ext} \mapsto p_{ext} \text{ or } \varnothing \text{ by declaration}$$

> **Illustration 21.** Figure 10, the layered arrangement
> <!-- illustration: loop-state-mgmt/ill-21 | type: png | subject: step-3 | keywords: safety classifier, prefetch organ -->
> File: figures/fig-10-ladder-3.png
> Lower left, external input enters the safety box, then the join box. The box under
> the engine is the prefetch organ, fed by a dotted signal from the engine and a
> query from retain, releasing into the renderer. Each added box and wire is a
> declaration field.

### The builder reading
<!-- node: loop-state-mgmt/ladder/builder | keywords: primitive, seam, loop, declaration -->

Standing: unmarked claim.

Each block is a primitive. Its ports are the contracts it honors. The wires are the
seams. The developer fills in two things: the contract name on each seam and the
loop script in the harness block. Each other block is content-neutral by charter.
Content-neutral means the block holds no motive. It does not mean the block holds no
model. Sort can run a model on the SPU or a mechanical rule. The loop block is the
one hole in the builder, because the loop is where the motive lives. A primitive
never knows why it runs.

## After the walk
<!-- node: loop-state-mgmt/after | keywords: head, tail, cut, capability, primitive, control loop -->

Standing: unmarked claim.

### The head and tail budget
<!-- node: loop-state-mgmt/after/budget | keywords: context bound, head, tail, management plane, candidate -->

Standing: unmarked claim. That place fails loudly is a candidate.

C = C_head + C_tail is declared, not discovered. Each ladder step is sized to the
tail. Thus running out of context is a design error, not an event.

Only external input on the management plane changes the head. A data-plane emission
cannot reach the configuration of the next turn. The in-band failure is a deployment
that lets the tail write the head, or lets the partition leak the measurement
payload into retain. The declaration shows both before the agent runs.

One case stays open. A single external input can be larger than C_tail, for example
a tool return that must arrive whole. Then sort must drop it or place must fail.
Chunking is document processing and is out of scope. No ruling is made here. The
candidate, which stays on the owed list, is that place fails loudly. A sort that
drops silently makes a content decision under no declared criterion.

### The one-direction cut
<!-- node: loop-state-mgmt/after/cut | keywords: cut, prefix position, middle swap, splice, recompute, conditioning -->

Standing: unmarked claim.

keep = N is the whole mechanism. Span disposition is withdrawn, because the buffer
is valid only as a prefix. Removing a middle span forces a recompute of all that
follows it. A change to the head mid-task costs a full recompute, so it starts a new
task by default.

A reach election that removes material from the middle of a stable tail also elects
a recompute. Retain can stop serving the material. Place cannot remove it from the
prefix without a recompute from that position.

A move from the middle to the end is not a middle swap. It is a splice, and it cuts
the prefix at the old position. A middle swap would change one position and leave
the rest valid. No engine this document admits does that.

The cut has two readings. The first is a constraint the engine imposes. The second
is a remedy. The middle gets in the way. The cut is the only operation that leaves
the model conditioned on nothing it should not be. A buffer that allowed a middle
swap would keep the stale conditioning on both sides of the swap. The recompute is
the cost of the remedy, not the penalty for an error. The walk makes the cut
electable.

### Capability and primitive
<!-- node: loop-state-mgmt/after/capability | keywords: capability, primitive, technique, query seam, trace arm, management plane, consolidating organ, retain -->

Standing: unmarked claim. The qualification for candidates built from the trace is a
candidate.

S_M is the set of functions the decoder cannot perform for itself. The build
realizes them as primitives that the loop wires, with the tee as the seam between
the loop and retain. The state management PRD says which primitive performs which
function. State management is the technique the loop runs over the three functions.
It is not the loop. The loop is orchestration, and state management is the content
of that orchestration.

Three properties of the capability follow from the walk. The build inherits them as
constraints.

First, the serve-side sort reads over one query seam, and retain is the first party
on it. The query shape belongs to the seam, not to retain. An organ that holds
something sort may select from presents itself on that seam. Sort does not grow a
second path, because a second path is a harness edit.

Second, the trace arm is the only source an organ outside the circuit may read. The
trace is the floor. Retain is the choice of a partition. An organ fed from retain
would inherit an election it did not make.

Third, an organ that fires on the measurement payload runs on the management plane.
Its signal never enters retain or p_int. Only what it offers over the seam does,
labeled by sort and laid by place.

A candidate qualification follows from the second property. A candidate built from
the trace carries its source and its admission status. Access to the trace must not
turn a partition that excluded material into permission to reuse it. The operator
declares whether a partition limits storage only, or also limits later use. The
contract for that difference is owed.

B may hold parties other than retain that sort queries and state management does not
own. Only retain holds S_R, by way of the store.

Each sentence belongs to one vocabulary. A sentence about a capability belongs here.
A sentence about a part belongs in the PRD of that part.

> **Illustration 22.** The count and the three properties
> <!-- illustration: loop-state-mgmt/ill-22 | type: text | subject: capability | keywords: consolidating organ, step outcome, query seam -->
> When sort marks the inventory step closed, the outcome, the count with nothing of
> the wandering, is the kind of thing a consolidating organ would hold. If a
> deployment has such an organ, it offers that outcome over the seam at the decision
> turn. It is the same candidate the reminder takes from retain, arriving from a
> second party. The properties say how it arrives, what it may read, and that
> whatever woke it never reaches the prompt.

### Control loops as instances
<!-- node: loop-state-mgmt/after/control-loops | keywords: control loop, outbound sort, serve-side sort, trace arm, consolidating organ, candidate -->

Standing: unmarked claim. The source of a consolidating organ is open.

A control loop fires on a condition the harness set, reads the trace, shapes what
reaches the engine, and records each edit. Each control loop is a sort or a place at
a named rung. The calculator splice is the outbound sort at rung three under an
oracle, with N at the splice. Autonomic prefetch is the serve-side sort at rung six
under append, with N at full length. Diagnostic replay reads the trace arm at rung
four and reissues p_int. Whether replay is a loop or an instrument is a question for
its sketch.

Principle six says a loop does not write state. Its hard case is consolidation. The
three properties resolve it. A consolidating organ reads the trace arm, runs on the
management plane, and offers over the query seam. It is not a loop that writes
retain. Nothing it does crosses the residue arm, so the one-origin claim holds. One
measurement remains: at a release, does the consolidated candidate become present at
the decision where the retained candidate did not.

The source of a consolidating organ is open. The second property says the trace arm.
A candidate proposed on 2026-10-14 says the record of state management. The ruling
waits on a targeted transmission pass.

## What would falsify the spine
<!-- node: loop-state-mgmt/falsify | keywords: partition, middle swap, place, admission test, trace floor, trace arm, holding -->

Standing: unmarked claim. Conditions 7 to 9 and 11 are candidates.

Each condition is a measurement or a reading of a declaration. None blocks another.

1. A partition must read content to do its job. Then the envelope-only duty fails
   for that deployment. If the added work needs a model, it belongs on the SPU, and
   the first ruling fails. If the work is mechanical, the ruling narrows.
2. A deployment needs a middle swap that the engine supports. Then keep = N is not
   the whole mechanism, and rung one has a second operation.
3. Place cannot serve stability and position effects at once. Then rung six has two
   criteria in one primitive, and the renderer splits.
4. A ladder step cannot load from its declaration without a harness edit. Then it is
   a mechanic, and the walk is short a rung.
5. The trace floor cannot be stated so that no partition cuts below it. Then replay
   depends on the operator choosing well, and the two-arm reading fails.
6. An organ outside the circuit must read retain rather than the trace arm to be
   useful. Then the second property fails.
7. A model holds its emission slots with no outside check, across long tasks and
   adversarial input. Then the holding need not sit outside the model, and the third
   stratum is optional. This condition is a candidate.
8. A branch restored from retain gives a different decision than the same branch
   never suspended, under the same tuple. Then the restore is lossy, and suspend and
   restore does not give continuity. This condition is a candidate.
9. A loop decision to load saved buffer entries cannot be recorded on the trace.
   Then a cache-restore run cannot be replayed, and cache restore leaves the ladder.
   This condition is a candidate.
10. A flat file searched by pattern over loose text matches a structured store under
    a meaning protocol on cross-session recall, compute, and wait, on the same agent
    and task. Then the trade between the store and the protocol fails.
11. A conversational agent gains as much from a meaning protocol as a technical one.
    Then the technical scope is not what makes the protocol fit.

## What is exported
<!-- node: loop-state-mgmt/exported | keywords: tee, partition, trace floor, outbound sort, serve-side sort, prefix position, head, tail, cut, recompute, conditioning, deciding pass, query seam, simulation, simulacrum, holding -->

This document sends these terms forward:

- the two goals, and the split of simulacrum and simulation,
- the three strata of holding,
- the tee and its two arms, the partition, and the trace floor,
- the two sorts and N,
- the head and tail split,
- the one-direction cut and its two readings,
- the three functions and the technique,
- the costs, the ledger on two axes, and the strategy table,
- the ruling that N never exceeds the valid prefix,
- the unmarked claim as a grade,
- suspend and restore with recompute restore and cache restore, as candidates,
- the buffer as intermediate computation toward residue,
- the three properties of the capability,
- the per-turn line and its candidate expressions for N,
- the one-origin rule as verbs: the tee filters, the partition admits, the store
  writes, retain holds, the sort reads,
- the ruling that the store owns its read and write, and the store as a member of
  state management in its own crate,
- sort at any crossing, classify on write, and the meaning protocol, as candidates.

This document describes. A PRD for state management charters the primitives the
description needs: the tee, the partition, retain, the classifier, the renderer
that performs place, and the two sorts. It states the contract on the seam into the
store and out of it. It charters no store. Each store is its own crate with its own
contract. The classifier is a primitive
the walk uses and no document has chartered. A spec follows for each primitive,
bounded by what that primitive alone must do. A separate PRD for the loop charters
the composition. Only the loop PRD holds a motive. It comes after the specs, because
it composes what they charter. When an agent needs a part no primitive offers, the
gap returns to the primitives PRD as a feature request.

It hands these items to later documents:

- To the state management PRD: the three functions and the query seam.
- To the renderer charter, which does not exist yet: place and its two things to
  serve.
- To the harness PRD: the tee, the trace floor, and the query seam as a seam the
  harness hosts. The cost of a recompute stays an open measurement. The record of
  each restore discipline on the trace, and how a run is kept for replay.
- To the engine charter: whether the engine exposes a call that loads saved buffer
  entries.
- To the v0.9.1 list of paper one: the buffer correction at rung one and the
  head-pinning correction.
- To paper six: the narrowed word simulacrum, for a ruling against its strict
  definition there.

## Owed before the next draft
<!-- node: loop-state-mgmt/owed | keywords: candidate, figure, ruling, transmission pass, intelligence -->

The owed items are split by owner. A candidate is ruled by the operator. The PRD
builds nothing against a candidate and decides none of it. The PRD decides only the
items handed to it.

### Owed to the operator
<!-- node: loop-state-mgmt/owed/operator | keywords: ruling, candidate -->

1. Find the exact expression for N under the ruling of 2026-10-15.
2. Rule on the source of a consolidating organ, after a targeted transmission pass
   on the section after the walk.
3. Rule on the candidate that place fails loudly.
12. Rule one meaning of model across this document, paper one, and the engineering
    account.
13. Rule whether the section on simulation stays here. Version 0.7 deferred the
    replication question to the engineering account and the methodology document.
14. Name the requirements of intelligence.
15. Rule the word simulacrum. Paper six defines it strictly, as Baudrillard did.
    This document narrows it.
17. Pick the term for suspend and restore. Candidates: suspend and restore, cut and
    replace, branch and resume.
25. Rule or leave each unmarked claim: head and tail, the cut, the costs, the
    ladder, the admission test, and the three properties.
27. Rule on the candidate of classify on write, which narrows the ruling that sort
    runs twice.
28. Rule on the candidate that a sort can sit at any crossing.
31. Rule on the candidate of the meaning protocol, and whether STE100 is the default
    instance or only the instance of this program.
32. Rule whether the PRD charters the partition as a primitive apart from the tee, or
    as the declaration the tee applies.

### Owed to the drafting seat
<!-- node: loop-state-mgmt/owed/seat | keywords: figure, transmission pass -->

5. Decide the home of the technique document.
6. Check the three loop sketches against the rung assignments.
7. Add the head-pinning correction to the v0.9.1 list of paper one.
8. Redraw Figures 4 to 10 so each box labeled store shows retain and the store it
   reaches as two parts.
9. Confirm the wording of the share of B against the tuple paper.
10. Redraw Figure 8 as the plain chat, with the limit case as a note.
11. Check that the bare rung of paper one matches the corrected step one.
16. Retire or rebuild Parts 1 and 2. They split v0.5 at the 10K budget, and this
    document now fits one window. Confirm the fit with the tokenizer of the
    embedding model.
33. Check whether a Department of Defense standard and a controlled French name a
    meaning protocol before the meaning protocol node cites them.

### Handed to the PRD and the charters
<!-- node: loop-state-mgmt/owed/prd | keywords: query seam, trace floor, classifier -->

4. Read section eleven of the trace PRD to confirm the trace floor. Confirm that the
   trace lets an operator read, per turn, the recomputed tokens, the deciding-pass
   tokens, the selection work, the I/O, and the wait of each.
18. Run the admission test on recompute restore against the query shape of retain:
    an ordered span by position, or a new operation.
19. Read the engine for a call that loads saved buffer entries. That reading decides
    whether cache restore is on the ladder or in the walk.
23. Specify how candidates built from the trace carry their admission status.
24. Hand the PRD the question of how a run is kept for replay: whether the kept
    entries and the state of retain are part of the trace or a separate record keyed
    to it.
26. Hand the PRD the question of whether the product detects a declaration that no
    longer describes what the trace shows.
29. The position of the classify on write is settled by the store ruling: after the
    partition admits, before the store writes. The PRD charters the carry.
30. Hand the PRD the charter of the classifier as a primitive, with its label scheme
    as a declaration field.
34. Hand the PRD the contract on the seam into the store and out of it, and the
    declaration field that names the store.

### Owed as measurements
<!-- node: loop-state-mgmt/owed/measurements | keywords: recompute restore, cache restore, ledger -->

20. Measure the crossover between recompute restore and cache restore on each tuple:
    branch length, sleep time, where the entries live, and bandwidth. Report the
    resource axis and the wall-time axis apart.
21. Measure suspend and restore against cut and cache on the same tasks and tuple:
    decision correctness, missed selections, recomputed tokens, selection cost, and
    wait.
22. Measure whether a cache restore and a recompute restore of one branch give the
    same prefix under a fixing tuple.
35. Measure a flat file searched by pattern against a structured store under a
    meaning protocol, on the same agent and task: recall across sessions, compute,
    and wait.

## References
<!-- node: loop-state-mgmt/references | keywords: fourth duty, simulation, symbol -->

Behrouz, A., Zhong, P., and Mirrokni, V. (2025). Titans: Learning to Memorize at
Test Time. NeurIPS 2025. arXiv:2501.00663.

Baudrillard, J. (1981). Simulacres et simulation. Paris: Galilee.

Bucy, T. W. (2026). What An Agent Is. Working draft v0.9.

Bucy, T. W. (2026). The Deployment Tuple. Working draft v0.5.

Bucy, T. W. (2026). The Model, an Engineering Account. Graphable document v0.1.

Bucy, T. W. (2026). Control Loop Design Principles. Sketch v0.1.

Bucy, T. W. (2026). Two Planes on One Loop. Sketch.

Kosowski, A., Uznanski, P., Chorowski, J., Stamirowska, Z., and Bartoszkiewicz, M.
(2025). The Dragon Hatchling: The Missing Link between the Transformer and Models of
the Brain. arXiv:2509.26507.

LeCun, Y. (2022). A Path Towards Autonomous Machine Intelligence. OpenReview,
version 0.9.2.

Liu, N. F., et al. (2024). Lost in the Middle: How Language Models Use Long
Contexts. Transactions of the ACL 12, 157-173.

McCarthy, J., Minsky, M. L., Rochester, N., and Shannon, C. E. (1955). A Proposal
for the Dartmouth Summer Research Project on Artificial Intelligence.

Shannon, C. E. (1948). A Mathematical Theory of Communication. Bell System Technical
Journal 27, 379-423 and 623-656.

Shi, F., et al. (2023). Large Language Models Can Be Easily Distracted by Irrelevant
Context. ICML 2023.

Weaver, W. (1949). Recent Contributions to the Mathematical Theory of Communication.
In Shannon and Weaver, The Mathematical Theory of Communication. University of
Illinois Press.

Wu, K., Wu, E., and Zou, J. (2024). ClashEval. NeurIPS 2024.

## Glossary
<!-- node: loop-state-mgmt/glossary | keywords: glossary -->

- **act (Y)**: the part of an emission that the outbound sort passes to the data
  plane.
- **admission test**: the rule that a ladder step must load from its declaration
  with no harness edit.
- **agent**: the arrangement A defined in paper one.
- **append**: an operation that adds tokens after the buffer and leaves N at full
  length.
- **argument (p_int)**: the one sequence the model reads on a pass. Rejected
  synonyms: prompt, internal prompt.
- **arrangement**: one wiring of the primitive set that loads from a declaration.
- **boundary (B)**: the declared boundary of the agent, fixed at load.
- **branch**: a span of turns that sort labels as one thread of work, which can be
  suspended and restored.
- **cache restore**: the restore discipline that loads saved buffer entries for a
  branch in place of a recompute.
- **buffer**: the attention entries the engine keeps in device memory, valid only as
  a prefix. Rejected synonyms: cache, KV cache.
- **candidate**: a proposed decision that the operator has not ratified.
- **capability**: what the agent can do, stated apart from the parts that realize
  it.
- **chat template**: the trained format of emission, with role markers, turn
  boundaries, and an end-of-turn token, that downstream parts read by position.
- **circuit**: the closed path of the per-turn line from the engine back to the
  engine.
- **classifier**: the primitive that performs sort with a model, on the SPU.
- **classify on write**: the candidate that each item the partition admits is
  labeled before the store writes it, under the label scheme the read side also uses.
- **cost**: a named charge a strategy pays, read on the resource axis or the
  wall-time axis. Rejected synonym: price.
- **conditioning**: the cost each pass pays for the tokens in the buffer, measured
  as an execution cost and a behavioral effect.
- **consolidating organ**: an organ outside the circuit that builds a denser form of
  past residue.
- **context bound (C)**: the maximum length of the argument, fixed by the tuple,
  split into C_head and C_tail.
- **control loop**: a loop that fires on a harness condition, reads the trace,
  shapes engine input, and records each edit.
- **cross-document edge**: a declared reference from a node in one document to a
  node in another.
- **cut**: the operation that moves N back and recomputes forward from it. Rejected
  synonym: compaction cut.
- **cut and cache**: the strategy that cuts the buffer to a step boundary and lays
  cut material back from retain when a decision needs it.
- **deciding pass**: a semantic pass that reads part of the buffer and decides what
  stays.
- **deployment tuple**: the declared conditions of a deployment, from the tuple
  paper, which decide the axis an operator feels.
- **declaration**: the load-time statement of an arrangement and its fields.
- **emission (e)**: the token sequence a pass produces.
- **envelope**: the event kind, channel, and origin of an event, apart from its
  content.
- **external input (p_ext)**: what crosses the gate on a turn. Rejected synonym:
  input.
- **figure**: a drawing in which each symbol maps to a declaration field or a
  primitive.
- **forward step**: one forward computation inside a pass.
- **foundry**: an organization that owns compute at the scale of a cloud provider.
- **fourth duty**: the paper one duty that places material where the model uses it,
  and its cost.
- **glossary**: the list of terms this document defines, one meaning each.
- **harness**: the organ that contains the loop.
- **head**: the pinned start of the argument, the share of B then the task charter.
- **holding**: keeping an applied meaning fixed by declaration, so that a receiving
  part does not interpret it freely.
- **in-model branch**: a way to give an agent statefulness by changing the model.
- **intelligence**: the property the opening sentence names, with its requirements
  owed at node simulation/requirements.
- **interpreter**: the party at the receiving end of a symbol, who constructs its
  meaning.
- **I/O**: the time to move bytes from storage into memory or onto the card, during
  which the decoder waits.
- **label scheme**: the declared set of labels a classifier assigns, which holds the
  decision the classifier appears to make.
- **ladder**: the ordered set of arrangements, simplest first.
- **loop (l)**: the orchestration inside the boundary.
- **management**: acting on a thing through a defined interface.
- **meaning protocol**: an agreed structure for meaning that holds across a lossy
  channel: standardized, in short declarative sentences, classifiable, with each
  verb bound to its noun and condition. STE100 is one instance.
- **management plane**: the plane of the operator, where the trace and the
  measurement payload live.
- **meaning**: what a symbol bears on for a party, applied by a writer and
  interpreted by a reader, never carried on the channel.
- **measurement payload**: timings, per-token entropies, and residual norms recorded
  on the trace arm.
- **membership**: being an element of S_M or S_R.
- **middle swap**: a change at one position of the buffer that leaves later
  positions valid, which no admitted engine supports.
- **model (M)**: a function from an argument to a distribution, run by the engine.
  Rejected synonym: meaning generator.
- **operator**: the person who declares and runs a deployment.
- **out-of-model branch**: a way to give an agent statefulness outside the model.
- **outbound sort**: the sort over the emission that decides the act.
- **partition**: the declared election on the envelope that decides what the residue
  arm admits.
- **pass**: one generation call that produces an emission, which can hold a prefill
  and many forward steps.
- **per-turn line**: the set of equations each figure is drawn against.
- **place**: the function that assembles the argument from head and tail and reports
  N.
- **prefetch organ**: a serve-side sort that fills candidate bins from retain while
  the decoder works.
- **prefix position (N)**: the position from which the next pass recomputes.
- **primitive**: a part that performs one function and holds no motive.
- **query seam**: the one seam over which the serve-side sort reads parties such as
  retain.
- **recompute**: the cost the engine pays to compute again each position after N.
- **recompute restore**: the restore discipline that rebuilds a branch from residue
  by a recompute.
- **resource**: the axis of the ledger that holds compute, device memory, selection,
  and storage.
- **recurrent state**: a compressed state some engines keep in the buffer beside
  attention entries.
- **reminder**: the strategy that lays material again at the tail. Rejected synonym:
  lay-again.
- **renderer**: the primitive that performs place, hosted in the harness.
- **replication**: the goal of building a mechanism that is an intelligence.
- **residue (S_R)**: what the pass left and the partition admitted, readable from
  the trace.
- **residue arm**: the arm of the tee that carries admitted residue to retain.
  Rejected synonym: store arm.
- **retain**: the function that holds S_R and answers a query. The store performs it.
- **ruling**: a decision the operator ratified.
- **ledger**: the statement of the costs of a strategy on two axes, never summed.
- **K_valid**: the sequence for which the engine holds valid reusable entries.
- **rung**: one step of the walk, ending where one duty ends.
- **safety classifier**: a classifier that may drop external input by declaration.
- **schema**: a declaration of type and constraint that fixes the form of a stored
  value.
- **seam**: a wire between two primitives, named by a contract.
- **semantic level**: the second level of Weaver, which asks whether the symbols
  convey the intended meaning.
- **serve-side sort**: the sort over residue and external input that decides what
  enters the next argument.
- **simulacrum**: the trained artifact at rest, which simulates nothing until a loop
  engages it.
- **simulation**: the event that runs when an interpreter engages the simulacrum
  through a loop.
- **simulation goal**: the goal of building a mechanism that produces a simulation
  when a party engages it.
- **store**: the organ that performs retain. It owns its read and its write, is built
  as its own crate behind its own contract and socket, and is named in the
  declaration.
- **suspend and restore**: the strategy that cuts away a live branch and later
  rebuilds it at its original position. Candidate term.
- **selection**: the work sort does to pick residue from retain, by a model or by a
  rule.
- **sort**: the function that labels, joins, and selects under a criterion, by a
  model or by a mechanical rule as the declaration says.
- **splice**: an operation that changes the buffer content at position k and sets N
  at k.
- **SPU**: the semantic processing unit, where each model that does semantic work
  runs.
- **state**: what exists only during the pass.
- **storage**: the capacity, at a provisioned class, that holds residue or saved
  buffer entries between passes.
- **state management (S_M)**: the set of functions retain, sort, and place.
- **statefulness**: the property of carrying something from one turn to the next.
- **statelessness**: the property of a deployed model that nothing a pass computes
  stays in the model.
- **step boundary**: the position where sort labels a step closed.
- **step outcome**: the result of a closed step, named by sort.
- **strict residue ruling**: the ruling that state exists only in the pass, is not
  readable after it, and belongs to nobody.
- **symbol**: a token on the channel, which carries no meaning of its own.
- **tail**: the moving part of the argument after the head.
- **task charter**: the statement of the task the operator pushes through the gate.
- **technical level**: the first level of Weaver, which asks whether the symbols
  arrive accurately.
- **technique**: what the loop runs over the three functions.
- **tee**: the seam of the harness that forks each event to the trace arm and the
  residue arm.
- **tool-return join**: the arrangement that lays a tool return beside its call.
- **trace (T)**: the record of the operator.
- **trace arm**: the arm of the tee that carries everything to the trace.
- **trace floor**: the content of the trace that no partition can cut below.
- **transmission pass**: a reading of a document by a synthetic reader to measure
  what transferred.
- **wall time**: the axis of the ledger that holds the time a cost adds on the
  critical path.
- **unmarked claim**: a claim this document asserts with no mark, which is neither a
  ruling nor a candidate.
- **wall compaction**: the strategy that waits for a full buffer, then summarizes
  all of it in one deciding pass.
- **working set**: the material a pass needs, held in the buffer.