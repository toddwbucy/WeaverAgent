# weaver-analysis - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. Ratified on its own
terms under the per-charter rule of 2026-08-23, conforming to the pattern the
2026-08-04 act established.

**Date filed:** 2026-08-24
**Document ID:** `weaver-analysis-PRD`
**Parent:** the WeaverTools suite, whose governing document is deliberately
not yet written, per `weaver-agents-PRD` section 0. The graph parent edge
names the `WeaverTools` system node, and the header and the edge name the
same thing.
**Editorial:** Per the Working Rules.
**Landing PR:** #650

---

## 1. What this crate is

`weaver-analysis` is **the diagnostic consumer**, a crate outside the agent boundary
standing in the structural position Weaver Web stands in. It holds both ends of a replay
and the agent holds neither: it reads a finished record the operator holds, preloads it
through the state member's second door, and reads the diagnostic-trace the run produces
off the sink admin opened for the binding. Whether that reading trails the run or runs
beside it follows the sink's declared shape and is not assumed here, per section 3.
Everything downstream of the record is here - the fitting, the projection, the layer
trajectory, the artifact store, and the reading.

It is chartered by the operator's ruling of 2026-08-24, which split the
diagnostic leg in two. `weaver-diagnostic` is the mechanism the harness
authors a diagnostic-trace through, inside the agent as the harness's third
member. This crate is what reads that record. An earlier reading had one crate
holding both roles, which put a rendering mechanism and a consumer under one
name and would have had the harness linking a crate from outside its own
boundary.

**One crate at both ends, and that is deliberate.** The party that decides what
a replay is made of is the party that can say what its output means, because
only it knows what it elected: which message kinds it preloaded, which turns it
projected, and what it left out. A second consumer reading the record cold
would be interpreting a run whose shape it did not choose. The certification is
not the ground for this and cannot be, that comparison belonging to the loop
inside the run per section 3.

**It never sees weights.** The residual readout is SPU-internal and elected at
the load, and what reaches this crate is what the record carried. Nothing here
touches a device, and no election of this crate's changes what a replay
computes.

```graph
node: weaver-analysis
kind: crate

edge: parent
from: weaver-analysis
to: WeaverTools

edge: seam
from: weaver-analysis
to: weaver-state
via: weaver-analysis-state-contract
tag: socket
```

**The emission to `weaver-web` is this crate's second seam, and the record for
it is here because this crate initiates it.** Neither party asks on this seam,
`weaver-analysis-web-contract` naming this crate the emitter that asks nothing
of the reader and naming the reader as never asking the emitter for anything,
so the Document Format's one-asking-party branch has no answer here. **That
page settles it where the silence is**, in its Parties clause, on the
initiating side, which is where the corpus already puts this crate's other
seam, per `weaver-agents-PRD` section 6 and `weaver-diagnostic-PRD` section 6.
**What crosses is not argued in this charter**, the contract holding the shape
and `weaver-analysis-Spec` section 5 representing it, and section 4 names the
clause this charter owes.

```graph
edge: seam
from: weaver-analysis
to: weaver-web
via: weaver-analysis-web-contract
tag: socket
```

## 2. What it is not

**Not an organ.** An organ governs a domain and holds a duplex channel with
the harness, both properties and neither alone. This crate has neither. It
governs nothing inside the agent, and the harness has no channel to it, no
knowledge of it, and no behavior conditioned on its presence.

**Not a member of the agent domain.** It parents to the suite rather than to
`weaver-harness`, per `weaver-agents-PRD` section 0's rule that crates outside
the agent boundary do not enter that roster. `weaver-diagnostic` went the
other way in the same act and for the opposite reason.

**Not the writer of anything the agent reads.** Its only seam into the agent
sends a preload into `weaver-state`, and that is material rather than
instruction: state holds what it is given and the agent's loops decide, per
`weaver-analysis-state-contract`. Nothing this crate produces reaches a
decoder except as tokens a loop chose to feed.

**Not the intervention overlay.** Cut-and-recompute is not carried here, on
the same taxonomy ruling that keeps it out of `weaver-diagnostic`:
intervention changes the input and produces a counterfactual token path, which
is a different product from a measurement of a run that happened.

## 3. What it does, in the order it does it

**It reads the operator's record as an operator principal**, over the
operator's own storage, and parses it. The parse is this crate's, sharing no
code with the writer, which is the boundary working as intended and is also
two statements of one shape: `weaver-trace-Spec` section 3 is the authority
and a divergence is a defect here, per G5. The reader rules of
`weaver-trace-PRD` section 6 bind this parser in both directions, and section
6 of that document is where they are argued rather than restated here.

**It derives the diagnostic declaration from the record it parsed, and the
analyst declares only the diagnostic run's own facts.** Per the operator's
direction of 2026-08-31 on issue #394. Every fact of the source run comes
from the record - the artifact, the seated identity prefix, the declared
seed and every tunable the effective sampling and its bounds name, the
source session's own name - because every value an analyst re-types into a
declaration is a chance to be correct to memory instead of to the run,
which is the record-silent-property defect class entering through the
config file. Four facts stay the analyst's to declare, each for its own
reason. **Device placement**, because the record deliberately names no
silicon and a replay on other silicon is a legitimate act the record must
not forbid. **The readers' elections**, riding the declaration per apex
section 8, the analyst's question and never the source's property. **The
diagnostic sink**, the new record's home. **The diagnostic destination session**,
a name distinct from the source because a new selection creates different holdings.
The derived declaration and the diagnostic preload name that same destination. A member
the record does
not carry and the run under this binding does not read takes the fixed
spelling the Spec names rather than a guess. The rule is also what makes a
later randomly drawn seed free: a value that lands in the record's
sampling members is picked up by the derivation with no further act, where
a re-typed declaration would be wrong by construction. The replay's own
identity checks then guard the derivation itself - a drifted derivation
fails certification rather than replaying a run that never was.

**Reconstruction follows the recorded election, and diagnostic projection is an explicit
different session**, per the operator's ruling of 2026-09-22 after PR #649's three-way
measurement. Ordinary reconstruction selects what the record says its live tee selected.
A diagnostic projection selects what the replay reads under this crate's own election
and requires an explicitly named destination different from the source session. A richer
diagnostic projection cannot silently replace the source session's holdings under its
old name. The record remains the authority for source-run facts in either case.

**It preloads what the selected rule projects**, across the seam
`weaver-analysis-state-contract` governs. The election opens the channel whole,
declaring the destination session the receiving load also names. Then one distillate per
elected event follows in record order, owed nothing back. Then the seal tells custody
that the driver finished. The driver checks its selection and required evidence before
the opener can retire any holdings. Missing evidence refuses the claim that needs it,
never selecting a different rule by default. This crate sends all three and asks nothing
on this seam.

**It reads the diagnostic-trace off the sink.** The sink is admin's, opened for
the binding under root by whatever discriminant the declaration named, per
`weaver-admin-Spec` section 5, and **this charter assumes no discriminant**:
the operator declares the shape, and whether this crate reads a finished file,
drains a pipe, or holds a connection follows from that declaration rather than
from anything here. Whether the reading trails the run or runs beside it
follows the same way. **No separate diagnostic delivery contract is introduced
and none is owed**, because a sink's reader is downstream of it rather than a
party to it, which is the position every consumer of a serving trace already
occupies, per `weaver-trace-PRD` section 1. **What governs the sink is
unchanged and reaches both kinds**: the declaration carries `trace-sink` under
either binding per `weaver-types-Spec` section 2, admin opens it by its
discriminant and holds its custody per `weaver-admin-Spec` section 5, and
`weaver-admin-operator-contract` section 3 governs what crosses out and whose
durability it is. This crate inherits that arrangement rather than standing
outside it.

**Where the sink is a file, the kept record is the capture artifact, and
this clause is what a kept capture must carry to be one.** Per the cell
this act closes, every member measured before it was papered. **Identity**:
a capture is a certified diagnostic record, and its identity is the closure
of what its claim rests on - the source trace it replayed, the declaration
the driver derived (itself the record's projection plus the analyst's
three), the device model and code identity the licence clause already
requires, and the certification outcome in the record's own
`replay.closed`. An uncertified record is an account and never a capture,
per the no-second-instrument rule. **Custody**: the operator's, outside the
agent, on the arrangement the sink already has - admin opens it, the
operator contract governs what crosses out - and this crate reads it as an
operator principal like every record. **Dataset shape**: the
diagnostic-trace's own, per `weaver-diagnostic-Spec`, at whatever density
the load elected. No second format exists, because a capture is a record
kept rather than a record converted. **Quota**: the sink shape is the
operator's retention election - a pipe retains nothing and a file retains
whole - and this charter adds no ceiling of its own: what bounds keeping is
the operator's storage, and the discard licence stands wherever
certification does, a kept capture being the operator declining a licence
rather than lacking one.

**The lens artifact is the fitted transport, versioned against the weights
it was fitted to and meeting only their captures.** **Identity**: the
weights by content hash, the corpus by source, selection rule, and content
hash, the estimator by implementation revision and parameters, and the
environment that ran the fit - each spelled in a manifest beside the
matrices, and a reader refuses a lens whose manifest names other weights,
recomputing the hash against the model in hand rather than trusting the
name. **Versioning**: the weights hash is the version, so a refit is a new
artifact beside the old and never a mutation - a lens refitted later
applies to a capture recorded earlier, the capture holding activations
rather than readouts, which is the provenance property the stream design
bought. **The fit-size election is made from measurement**: two hundred
corpus prompts, the paper-scale fit at five times the compute having moved
no evaluation number, per the measurement act of 2026-09-01. **Custody**:
the operator's storage beside the captures, outside the agent, the fit
never touching the agent at all.

**A pipe-shaped sink retains nothing, and the report is the kept artifact.**
Per the operator's ruling of 2026-08-30: a diagnostic run whose sink is a pipe
streams through this crate, the reading is taken as the stream drains, the
report carries its evidence, and the raw capture is kept nowhere. Retention is
not this crate's to choose there - the sink's shape made it - so what this
clause governs is the claim a report may make about what was not kept.

**What licenses the discard is the replay's own certification, stated at the
exactness the corpus grants each payload and no more.** The token path is
held exact and the vectors within the tolerance `weaver-diagnostic-PRD`
section 4's comparison states, that document declining bitwise equality of
floats on purpose, and this clause claims nothing stronger for the columns
than the certification that regenerates them claims. A capture the
certification vouches for is derivable rather than data, so keeping it would
store what can be recomputed to the same certified exactness, and the trace
with its declaration is already the capture's compressed form. The evidence
in hand is the measured half: within one device the weekend's replays of
2026-08-29 through 30 reproduced token paths and per-token entropies
byte-identical across 5,530 sessions and five precisions - the reduction
rather than the columns, which is why the tolerance clause above carries the
columns' share.

**Two captures are two captures of one run only under one loop and one member**,
as of 2026-09-07. The loop assembles every prompt the model sees and a deployment
can run more than one, so two records that name different loops were prompted
differently and their disagreement is the prompt's and not the engine's, and a
record whose loop injects the session's past is not byte-comparable with one whose
loop did not, per `weaver-trace-PRD` section 3.1 as ruled 2026-09-03 on issues #381
and #382. The state member's standing is the same fact one leg over. The `load`
event names both since that date, so the comparison reads them from each record
before it reads a value and refuses where they differ, naming which, and refuses a
record that names neither, an older record being one whose loop cannot be known
rather than one whose loop is the default. What this buys is that a verdict of
divergence is always about the engine: the finding of 2026-08-29, a reissue reported
as not reproduced because a standing member added a line to the prompt, is refused
at the door with its cause named rather than reported as a determinism failure.

**The licence is bounded by the device model the deposit names, and the
report must name it or say that it cannot.** Reissue holds within one device
model and is refuted across them by the measurement of issue #346, so a
report whose evidence was discarded is reproducible on the silicon that
produced it and nowhere else, and a report that cannot establish which states
that plainly, carrying no reproducibility claim in place of a member that
would read as one. The serving device does not yet ride the run's own record:
it reaches the deposit through the driver that took it, per the
run-records-what-served act of 2026-08-28, and a record event carrying it is
owed its own act, named here as owed rather than assumed present. Until it
lands the device model's source is the deposit the operator holds.
**Code identity has the same status and the same source**: regeneration runs
the seam's code as well as the silicon, and the repository commit reaches
the deposit by hash, the toolchain and the driver by pinned version string,
and the engine libraries and organ binaries by the sha256 the driver takes
of each, so their source is that same deposit, and a report surviving a
rebuild claims nothing the deposit's identifiers do not carry.

**It reads nothing for meaning from a replay that did not certify**, per
`weaver-diagnostic-PRD` section 4, which carries the criterion because it
belongs with the mechanism being judged. **The comparison is not performed
here.** `diagnostic-replay-loop` section 3 walks it inside the run, which is
where it has to happen: the loop holds the recorded path in its holdings and
the recomputed identifiers as they arrive, so it alone can refuse before the
first forward pass and name the first divergent position rather than reporting
after a whole replay has run. What this crate does is **require a null replay
before anything downstream**, read its outcome from the record, and gate every
later reading on it. **The reader's election is not this crate's to make**, riding
the declaration at the load per apex section 8, so what this crate elects is the
order it consumes outcomes in and never which pass the agent runs. An earlier form
of this sentence said this crate elects the null replay, which read as a control
over the load it does not hold, and `weaver-analysis-Spec` section 5 settled the
distinction this clause now carries.

**That gate rests on telling a finished record from a truncated one, and the
record carries the fact as of 2026-08-27.** A reader that has consumed every byte
available to it once could not say whether the replay certified and ended, failed
its comparison and ended, ended without finishing, died mid-replay leaving a
partial record, or was still running, and all of them looked alike at the end of
what it had. **`replay.closed` separates the three a pass can state from the two
it cannot**, per `weaver-diagnostic-Spec` section 3.3: its `ReplayOutcome` names
certified, diverged, and abandoned, each authored by a pass that reached its own
end, and **a pass that died authors no close at all**, which is that Spec's own
refusal to manufacture one.

**So the absence is one answer and not two.** A bracket with no `replay.closed`
is a pass that did not end, and whether it died or is still running is not a
distinction this record makes or this crate needs: both leave the same absence,
both may yet be followed by nothing, and reading either as an ending would be
treating the end of available bytes as the end of a run, which is what this
paragraph refused before the marker existed and still refuses. So this crate
gates on the outcome the record states: it produces its reading where a bracket
closed certified, produces the divergence where one closed diverged, produces
neither where one closed abandoned, and **produces nothing for any unclosed
bracket, on the same terms whichever way it came to be unclosed**. Where that
marker landed is section 4's cell, now settled. An
earlier form of this paragraph had the comparison here on the ground that this
crate holds both records, which is true and is not the reason the loop cannot,
so it would have put a second implementation of one check on the other side of
a sink.

## 4. Open cells, each named rather than implied

- **The trace as an input format, settled 2026-08-24 and left here as the
  obligation it puts on this crate.** The record carries no version marker and
  needs none, per `weaver-trace-PRD` section 6: the schema extends and does
  not change, so every vintage is the one schema and a reader keys on nothing.
  What that costs this crate is a rule its parser owes rather than a question
  it was owed. **Semantic interpretation ignores unknown kinds and members,
  while reconstruction retains their raw material when the recorded rule elects
  it.** Unknown content cannot acquire the meaning of a known diagnostic event
  or change its request-to-measurement grouping. That does not permit dropping
  an unknown event's envelope under an all-kinds rule or dropping an elected
  payload path because the diagnostic reader does not use it. **The other direction
  binds this parser harder**: a record
  written before a member existed omits it, and the parser reads that record
  without rejecting it and without deriving the missing member from the
  members beside it, per the same section. This crate is the place that rule
  costs something, the layer counts on the measurement payload being younger
  than the traces it will be pointed at, and deriving a layer count from a
  norm array is the arithmetic the counts were added to retire. A replay over
  a record that predates them is a replay whose layer count is unknown rather
  than one whose layer count is guessed. The same rules bind the
  diagnostic-trace when this crate reads it, that record being versionless on
  the same terms and for the same reason.
- **The instrument suite.** What this crate carries beyond the certification
  is named in the chartering ruling as a sketch that does not exist in this
  tree. The reference is recorded so the ghost is a known gap rather than a
  silent one, and no crate is built against it.
- **The capture artifact** - identity, custody, dataset shape, quota -
  **closed 2026-09-01** in section 3's kept-artifact clauses, every member
  written from the measurement acts of the same date rather than assumed,
  and the lens artifact beside it. `weaver-diagnostic-PRD` section 6 carried
  it while one crate held both roles.
- **The licence boundary.** This crate is the piece that can be given away and
  it carries no cut-and-recompute, which is cleaner than expected. The
  intervention loop shares that mechanic with the calculator loop and is where
  the boundary runs. The call is the operator's.
- **How a diagnostic-trace says it ended, and how it says what happened.** The
  gating of section 3 depends on it and cannot be honoured without it. **It is
  not settled here, and settling it here would reach past two things the corpus
  holds open on purpose.** The first is the diagnostic-trace's event
  vocabulary, which `weaver-diagnostic-PRD` section 6 owes to that crate's Spec
  and which is where a terminal event would have to be declared, a serving
  record's `unload` and `session.closed` being that vocabulary's answer rather
  than a shape this crate may assume carries over. The second is the sink's
  discriminant, which section 3 states this charter assumes nothing about, so a
  rule written per shape - what a closed file, a drained pipe, or a dropped
  connection each mean - would put back the assumption that section withdrew
  and would make the consumer's reading depend on an operator's declaration.
  **The outcomes a marker has to separate are nameable now and are named here
  so the owed act has its criterion**: certified and ended, failed its
  comparison and ended, ended without finishing, and not ended. The first three
  are facts the run knows and can author. The fourth is the absence of the
  other three, which is why it costs nothing to distinguish once any of them
  exists. **Settled 2026-08-27 in `weaver-diagnostic-Spec`**, which was owed it
  and which lands the marker as this cell's criterion asked: the first three
  outcomes ride `replay.closed`'s `ReplayOutcome`, the second splitting by which
  of certification's two comparisons diverged, and the fourth is that event's
  absence and costs nothing, exactly as this cell read it forward. The identity a
  claim rests on rides its own kind for a reason this cell did not foresee: a pass
  whose replay answer never arrived can open a bracket and author no identity,
  which is how the account this crate reads says that nothing was replayed.
- **This charter names no Rust item and elects no representation.** **Its Spec
  landed 2026-08-27** and the driver's shape and the parser's are there: the
  parse's own read types answering to `weaver-trace-Spec` section 3 under G5,
  ordinary reconstruction under the recorded election and explicit diagnostic
  projection under an election composed from what the replay reads, the projection
  splicing raw payload text so a holding cannot say by
  its own bytes which side landed it, and the gate on the outcome this
  charter's section 3 now rests on. **The certification's mechanics do not**,
  that comparison belonging to the loop inside the run per section 3, and what
  this crate's Spec settles about it is only the order it consumes outcomes in,
  requiring a certified null pass before any reading downstream.
- **What the emission to `weaver-web` is for is argued nowhere, and the clause
  is owed here.** Section 1 carries the seam's record because this crate
  initiates the seam, `weaver-analysis-web-contract` holds what crosses, and
  `weaver-analysis-Spec` section 5 represents it under that section's summary
  records, so a Spec and a contract stand over a charter that names the seam
  and argues nothing about it. **Naming it is not settling it.** What this
  crate sends a reader, and why sending it is this crate's job rather than a
  second consumer's, is a phase one question, and the act of 2026-09-16 that
  moved the record to the initiating side declined to answer it rather than
  writing a charter clause to hold a block up.
