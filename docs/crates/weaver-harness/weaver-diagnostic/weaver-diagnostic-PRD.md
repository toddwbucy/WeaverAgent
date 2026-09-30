# weaver-diagnostic - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. Ratified on its own
terms under the per-charter rule of 2026-08-23, conforming to the pattern the
2026-08-04 act established.

**Date filed:** 2026-08-24
**Document ID:** `weaver-diagnostic-PRD`
**Parent:** `weaver-harness-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #586

---

## 1. What this crate is

`weaver-diagnostic` is **the harness's third member and the diagnostic-trace's
mechanism**, standing to a replay exactly as `weaver-trace` stands to a turn.
It is chartered by the operator's ruling of 2026-08-23, carried by the epic
that tracked it, and it is the second half of the taxonomy promotion the
operator split on 2026-08-24: the binding kinds landed in `weaver-agents-PRD`
section 6 and the acts of that date, and the loop class lands here.

**It does not produce the diagnostic-trace.** The harness authors. This crate
is the mechanism the harness authors through, and the distinction is the whole
of the charter, restated from `weaver-trace-PRD` section 1 because the two
members stand in one relation to one author and a reader holding both should
meet the same sentence twice. `weaver-trace` is the mechanism under a serving
binding and this crate is the mechanism under a diagnostic one, the harness
being the sole writer either way.

**It never sees weights.** The tap is not part of it: the residual readout is
SPU-internal, elected at the load, and puts vectors on the wire. What this
crate receives is what the wire carried, and it renders that into the record.
The fitting, the projection, the layer trajectory, the artifact store, and the
reading are downstream of the record and belong to `weaver-analysis`, outside
the agent, per section 6.

**It binds no listening port and holds no cognition**, which is the property it
shares with `weaver-trace` and the reason neither is an organ. It governs no
domain and holds no duplex channel with the harness. It is reached by being
linked and called, one caller and no other, and its output leaves by the sink
admin opened for the binding.

**Which member the harness authors through is the binding kind's to settle, and
nothing else's.** A serving binding composes `weaver-trace` and a diagnostic
binding composes this crate, decided at the load and never entered afterward,
so no run holds both and no code chooses between them at a turn's grain.

```graph
node: weaver-diagnostic
kind: crate

edge: parent
from: weaver-diagnostic
to: weaver-harness
```

**The `weaver-traits` dependency is not declared here, and the reason is that this
charter cannot state a ground the seam's own contract allows.** The manifest carries
that crate and the graph carries no edge for it, which is the H2 breach the audit of
2026-09-13 found. The obvious ground is the message model, and it does not survive
reading: `weaver-harness-diagnostic-contract`'s vocabulary clause draws that model
from `weaver-traits` and then says who draws it, that **the harness draws it and the
recorder does not**, those payloads being opaque to the recorder. This crate is the
recorder. Apex section 5.1 has which floor crates a crate links follow from what it
draws, and section 5.3's mechanical consequence has a party linking the crate that
defines what it emits, so a floor link declared here would be declared against both.
**No `.rs` file under `src/` names `weaver_traits`** either, so nothing in the tree
supplies the ground the contract withholds.

**What this crate does not link, and the election behind that absence, are
`weaver-diagnostic-Spec` section 1's**, where `diagnostic-no-trace-dependency` stands
and where the manifest test reads the resolved tree. That test asserts the presence of
`weaver-traits` as well as the absence of `weaver-trace`, so the dependency is pinned
by an instrument while its reason is unwritten - which is the shape that makes this a
question rather than an omission. **The question is the operator's**: a dependency no
source consumes and no contract draws is either removed or given a ground, and it is
the same question `weaver-state` carries twice at that charter's section 5. Removing
it is a code act and fails the test that pins it, so neither half belongs to this
charter.

## 2. What it is not

**Not an organ.** An organ governs a domain and holds a duplex channel with
the harness, both properties and neither alone, and this crate has neither.
It governs no domain, and the harness reaches it by linking rather than by a
channel, which is `weaver-trace`'s position and for `weaver-trace`'s reason:
a type definition does not travel over a socket and a rendering mechanism is
not a peer.

**Not the consumer.** An earlier form of this charter placed this crate
outside the agent as the diagnostic consumer, and the operator's ruling of
2026-08-24 corrected it. The consumer is `weaver-analysis`, chartered in the
same act, which reads the record this crate renders. Nothing of the reading
belongs here.

**Not the intervention overlay.** Cut-and-recompute is a mechanic this crate
does not carry, per the taxonomy's ruling: intervention composes with either
substrate and its product is a counterfactual token path, which is a
different deliverable with a different custody story. The calculator loop
holds that mechanic on the production side, and where the licence boundary
runs through it is the operator's open item, named in `weaver-analysis-PRD`
section 4, which took that cell with the reading. Section 6 records only that
it departed.

**Not a memory leg.** It renders a record and reads none, `weaver-analysis`
holding the reading. What it renders enters no agent's state, no prompt, and
no working structure, and statefulness returns through `weaver-agents-PRD`
section 9's door or not at all. The sentence this replaces had this crate
reading records and writing analysis artifacts, which was true of the
consumer it was chartered as for one day and is true of nothing it does now.

**Not an evaluator.** The mechanic-motive line of section 5 is this crate's
outer edge: it claims a faithful readout with replayability and claims
nothing about what the readout is worth.

## 3. The diagnostic loop is a class, and the readout is interchangeable

The diagnostic substrate is defined by three refusals, against the production
substrate's grants:

- **Gate never starts.** A diagnostic binding declares no Gate, per
  `weaver-agents-PRD` section 6, so nothing enters from outside and the
  wrong arrangement is unrepresentable rather than guarded against.
- **The working structure is preloaded from a finished trace** and read
  positionally as the source of prompts, rather than accumulating. Same
  organ, opposite direction. **Whose structure it is follows the kind and is
  not settled here.** `weaver-trace-PRD` section 1 has that crate holding the
  rendering in RAM as the working structure, and a diagnostic binding does not
  compose that crate, so whether this mechanism stands one up in the same
  shape is part of the surface election `weaver-harness-Spec` section 9 holds
  open. The term is the apex's, section 0's, and this bullet uses it in that
  sense whichever mechanism holds it. The preload crosses
  `weaver-analysis-state-contract`, whose papers merged 2026-08-24 and whose
  near side is `weaver-analysis` rather than this crate, per section 6. Two
  earlier readings of this bullet are corrected together: it called the seam
  section 6's when that section now disclaims holding one, and it called the
  refusal not yet drivable when the papers had already landed.
- **Nothing writes back**, so the substrate under examination is immutable
  for the loop's duration.

**The residual tap is one passive reader on a faithful re-execution, and the
loop is a class because the reader swaps without the loop changing.** The
membership test is mechanical: a member needs only a forward pass over a
fixed token path, and writes nothing. Attention-pattern capture passes.
Per-layer logit-lens decoding passes and is the cheapest member.
Recomputation of per-position entropy and surprisal passes and is close to
free, the measurement payload already carrying the production figures to
compare against. Activation patching fails, because it changes the input and
so produces a different run - it is the intervention overlay meeting this
substrate, not a member of the class.

**What is authored once is the loop, the custody rule, the artifact
identity, and the certification. Every instrument after the first is a tap
plus a Spec clause.** Naming the loop for one instrument would make the
class look like one thing, and the second instrument would then either
wrongly amend it or wrongly fork it.

## 4. Certification, and what inherits what

**The null replay certifies the substrate.** The mechanic's correctness is
not established by whether a readout is interpretable. It is established by
exact-match comparison of a replay against its original, and then again with
the readout elected, to show the read is passive. Exact match is a claim
about the token path, which is integers and matches or does not. **Within
one device model the vectors hold the same bar, measured rather than
assumed**: two certified column replays of one source differenced to
9,784,320 of 9,784,320 values exactly equal on 2026-09-01, so the
passive-read comparison holds the token path and the vectors exact alike
where the device model matches, the report naming that model per the
licence rule. Across device models the comparison keeps the GPU float
tolerance of `weaver-agents-PRD` section 8, unmeasured and bounded by
issue #346's finding that replay is bit-exact within a card model and not
across one. A readout from an
uncertified replay is a picture of an unknown run.

**Certification is two claims and not one.** An output comparison means
nothing unless the input was the same one, so the certification checks the
input first, established from the record: input token identifiers, output
token identifiers, model identity with its weights hash, sampling
parameters, and the prompt-block partition, per `weaver-agents-PRD` section
8, with the template's identity traveling with them so a replay re-feeds
rather than re-renders. The same conversation rendered under a later
template is a different prompt, and a replay that re-rendered would compare
two different runs and find them different. **A claim about the state rests
on one fact beyond that list, claim-relative the same way: the tee's
election**, per `weaver-agents-PRD` section 8. It is the rule that decided
what the original agent's state held, so a replay preloaded under a
different one rebuilds a session that never ran, and a record written before
the member existed fails a state claim while its token path still stands.
Input identity checks it with the rest, before any forward pass.

**No second instrument lands before the first replay is certified**, because
a second tap sharing an uncertified replay inherits the uncertainty rather
than dividing it.

**The production column needs no replay certification and its mechanic is
not yet shown.** The intended ordering is that cut-and-recompute earns its
correctness on the production side and the diagnostic side inherits it, and
that inheritance is conditional on a splice that has not landed, per
`weaver-internal`'s own cell. A plan rather than a record, stated as such.

## 5. The mechanic and the motive

**In scope, and this program's to answer:** whether replay re-executes the
same forward passes, whether the readout perturbs the pass, whether the
capture joins back to the forward pass that produced it, whether the
artifact is identified by what determines it, and whether the vectors reach
disk uncorrupted.

**Out of scope and the operator's:** whether a readout means anything, how
it compares against a cheaper instrument, how many fitting sequences
suffice, what a trajectory across layers indicates, and whether one
binding's readouts agree with another's. Two bindings serving the same
weights at different precisions are, to the mechanic, two bindings it must
serve identically, and whether their readouts agree is a finding.

## 6. The seam, the custody, and what this charter leaves owed

**This crate declares no seam of its own and dials nothing.** It is linked and
called, one caller and no other, which is `weaver-trace`'s position and for the
same reason: a rendering mechanism is not a peer and nothing it makes travels
except by the sink its author writes into. **It is still one end of a seam**,
the `link` the harness declares from the asking side, and that record landed
2026-08-27 with `weaver-harness-diagnostic-contract` in the act that wrote this
crate's Spec. The distinction is
between declaring an edge and being named by one, and this crate does the
second only.

**The preload crosses a second seam on the state member, per the operator's
ruling of 2026-08-24, and the papers stand**, but the near side of it is not
this crate. `weaver-analysis-state-contract` is initiated by `weaver-analysis`
from outside the agent, checked against the operator principal, on the
precedent of the gate's two doors, and the edge is declared in
`weaver-analysis-PRD` where the initiating side's charter carries it. An
earlier form of this section declared that seam here, when this crate was
chartered as the consumer, and the ruling that moved it inside moved the seam
with the party rather than leaving an inside crate holding an operator's door.

**Custody is unchanged and needs no exception.** The trace path is declared
material, so `weaver-analysis` reads the operator-held stream as an operator
principal, over the operator's own storage, and preloads it through the door
below. The trace parser belongs to that crate, outside the agent, on the same
ground the reading does. `weaver-state` never opens a trace file and
`weaver-trace` keeps its write-only pin. Nothing inside the agent opens the
record the replay runs from.

**The record runs one way only, and the other product is authored through this
crate.** A
diagnostic binding writes no trace, per `weaver-agents-PRD` section 6 as
amended 2026-08-24, so `weaver-trace` has nothing to do in a replay at either
end: it is not read, because a reader of a finished record is downstream of a
file rather than a party to that crate, and it is not written, because a
replay performs no cognition of its own to record. What a diagnostic run
produces instead is the **diagnostic-trace**, the record of the replay, named
here for the first time and authored by the harness through this crate on
section 1's terms. The graph block below carries the relation the charter
means: this crate `defines` the term, which is counterpart status rather than
ownership, and nobody calls a serving record `weaver-trace`'s own.

**The diagnostic-trace is a trace, and the two are separate for how they are made rather
than for what they look like.** It carries the run the way a serving record carries one
and carries the residual readout beside it, which is the bulk of it and the reason it
exists. **What is shared and settled here is the canonical serialization and nothing
above it**, one line of UTF-8 JSON per record with the newline as the separator, per
`weaver-trace-Spec` section 2. Whether an instrument that reads a serving record reads
this one is **not claimed**: that follows from the event vocabulary this writer reuses
and from where the residual readout sits beside it, both of which are this crate's own
Spec's election, so reader compatibility is that document's to define and to state the
rules of. An earlier form of this sentence assigned the election to the driver's Spec,
which was true while one crate held both roles and is not now: the writer's shape
belongs to the writer. What differs is the making, and the making is the whole
distinction.

**One begets the other, in a different context.** A diagnostic-trace is made
from a trace: the serving record is the replay's input, and the
diagnostic-trace is what the replay returns, so the two stand in a parent
relation rather than side by side. That is why they cannot be one type
however alike they read. A single type would have a record and the record
derived from it wearing one name, and the first question anyone asks of a
file in hand, which of the two it is, would have no answer in the file. It
would also make the derivation circular on its face, a shape defined in terms
of a run over itself, when what happened is that one run was read and a
second was performed against it.

The making differs on the same author. Both records are authored event by
event by the harness as the sole writer, inside the agent, into a sink admin
opened for the binding. What differs is the run underneath: a serving record
brackets a turn as it happens, and this one runs over turns read from a
record rather than lived, carrying the residual readout dense enough
to be the point of the run, though a serving load may elect it too, per apex
section 8. Two provenances that far apart cannot share one mechanism without the
mechanism losing the ability to say which it made, and `weaver-trace`'s
charter closes the question from its own side: it has one caller and no other
crate submits an event to it. **So this crate is the counterpart mechanism**,
the one the harness authors a diagnostic-trace through, standing to a replay
as `weaver-trace` stands to a turn.

**The shared form has one authority and it is not this crate.** Canonical form
is `weaver-trace-Spec` section 2's, one line of UTF-8 JSON per record with the
newline as the separator, and where this crate's writer diverges from it the
defect is this crate's, per G5. **This crate's traffic in records runs one
direction**, out, section 2 having it render a record and read none. The
parser answers to `weaver-trace-Spec` section 3 instead, the event's shape
rather than the line's, and it lives in `weaver-analysis` where the reading
does. An earlier form of this sentence claimed one authority for both
directions, which was true of the crate this one was chartered as and would
send an implementer looking for a parser inside the agent.

```graph
node: diagnostic-trace
kind: term

edge: defines
from: weaver-diagnostic
to: diagnostic-trace
```

**The names this crate's seam depends on are defined here and represented in the
Spec**, per the rule that a charter is the source of a crate's definitions and a
Spec restates none. `weaver-harness-diagnostic-contract` draws these four beside
`diagnostic-trace` above, five of this crate's definitions in all: the
**diagnostic event-kind set**,
the closed vocabulary of what a replay authors, the **diagnostic payload shapes**,
one accepting shape per kind, the **replay outcome** the closing kind carries, and
this seam's **failure vocabulary**. What each holds is
the Spec's to represent, per its sections 3 and 6, and what they are is this
charter's to name.

```graph
node: diagnostic-event-kind-set
kind: vocabulary

edge: defines
from: weaver-diagnostic
to: diagnostic-event-kind-set

node: diagnostic-payload-shapes
kind: vocabulary

edge: defines
from: weaver-diagnostic
to: diagnostic-payload-shapes

node: replay-outcome
kind: vocabulary

edge: defines
from: weaver-diagnostic
to: replay-outcome

node: diagnostic-failure-vocabulary
kind: vocabulary

edge: defines
from: weaver-diagnostic
to: diagnostic-failure-vocabulary
```

**The harness authors through this crate and the record leaves by the sink.**
The measurement returns from the SPU by the same path the generation does,
per the decode contract's answer shape, so it reaches the harness whatever
the binding's kind - the contract rather than apex step 6 cited here since
2026-08-31, that step now describing the serving turn alone. On a serving
binding the harness authors it through `weaver-trace`. On
a diagnostic one it authors it through this crate, into the diagnostic-trace,
beside the replayed run the same record carries. The rendering leaves by the
sink admin opened for the binding, and `weaver-analysis` reads it from there.
**Admin's custody of the sink is unchanged and this charter fixes no
discriminant**, the shape being the operator's to declare and section 5 of
`weaver-admin-Spec` opening by it without reading the kind. An earlier form of
this paragraph called the diagnostic sink a socket, which this charter had no
standing to decide and which would have carried a consequence it did not
argue: admin's socket case has no creation flag, something of the operator's
must already be listening, and a connection refused refuses the load, so
naming the shape here would have made the consumer's readiness a precondition
of the agent's load. What stands at the far end of a sink is the operator's
arrangement, exactly as it is for a serving trace.

**Nothing here crosses the boundary outward.** The harness links a member of
its own domain, which is the relation it already has with `weaver-trace` and
`weaver-state`, and the one crate outside the agent reaches the record only
after it has left by the sink. An earlier form of this section had the
harness relaying to an outside assembler, which would have put an agent crate
in reach of a consumer's shape, and the ruling of 2026-08-24 removed the
question rather than answering it.

**The refusal rests on the binding's declaration, and its form is the door's
absence.** State learns the kind at its standing, from the party that stands
it, which resolved the kind at inventory, and binds the preload name only
under a diagnostic binding. A driver pointed at a serving agent therefore
finds nothing to dial, per the contract: no directive arrives to be refused,
because the seam it would cross does not stand. An earlier form of this
paragraph described a refusal at the seam, which the landed papers made
structural instead.

Open cells, each named rather than implied:

- **This crate's Spec landed 2026-08-27** and both elections this cell held are
  settled there. The writer's vocabulary is its own closed set of nineteen
  kinds since the acts of 2026-09-26 added `recall` and `elision`, fifteen
  spelled as the serving vocabulary spells them and meaning there what they
  mean here, four of the record's own
  that no serving record carries, `residual.column` the fourth per section
  13.7 of the SPU's charter as amended. The residual
  readout rides `model.measurement` exactly where a serving record puts it, density
  rather than shape being what a diagnostic run changes. Reader compatibility
  follows as a stated rule rather than a hope, per that document's section 4, which
  is the claim this charter declined to make and assigned there.
- **The seam this crate presents to the harness** is settled with it: it mirrors
  `weaver-trace`'s receive and submit surface and shares no type with it, so one
  call site in the harness serves both mechanisms and neither crate depends on the
  other. `weaver-harness-Spec` section 9's item closes in the same act.
- **The null replay** of section 4 is owed behind both, and it is the run
  that certifies this mechanism rather than a property this charter can
  assert.
- **What this charter no longer carries.** The licence boundary, the capture
  artifact, the instrument suite, and the parser's obligations moved to
  `weaver-analysis-PRD` with the reading, on the operator's ruling of
  2026-08-24 that split this leg in two. They are named here as departed
  rather than dropped, because this document carried them for one day and a
  reader of its history will meet them.
