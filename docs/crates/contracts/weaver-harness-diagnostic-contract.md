# weaver-harness / weaver-diagnostic - contract

**Status:** MERGED. In `main` and the source of truth.

**Date filed:** 2026-08-27
**Document ID:** `weaver-harness-diagnostic-contract`
**Editorial:** Per the Working Rules.
**Landing PR:** #366

---

## Parties

- **`weaver-harness`, the author.** Sole producer of diagnostic-trace content
  and the only caller. Decides what a replay is, when a pass opens and closes,
  and what outcome the pass reached. Holds every handle.
- **`weaver-diagnostic`, the recorder.** Assigns ordering, produces canonical
  form, and hands the rendering to the sink admin opened for the binding.
  Holds no policy and decides nothing about content.

No third party writes. `weaver-analysis` reads what this seam produced after
it has left by the sink, which is a reader's position downstream of a file
rather than a party here, per `weaver-analysis-PRD` section 3.

**This seam is a library boundary.** It crosses no process line, so it is
tagged `link` rather than `socket` and authenticates nothing: there is no
second process to identify. That is `weaver-trace`'s position at its own seam
and for the same reason, a rendering mechanism being linked and called rather
than dialed.

```graph
node: weaver-harness-diagnostic-contract
kind: document

edge: party
from: weaver-harness-diagnostic-contract
to: weaver-harness

edge: party
from: weaver-harness-diagnostic-contract
to: weaver-diagnostic
```

Nothing here points back at the seam. The seam is an edge declared once in
`weaver-harness-PRD` section 4 with this contract named in its `via` field,
per `weaver-diagnostic-PRD` section 6's rule that this crate is named by an
edge and declares none.

## Vocabulary

Every contract names the vocabulary it depends on, grouped by the crate that
defines it. A contract without this clause is not a valid contract, and a
group is stated even when empty, because an explicit nothing is an assertion
someone checked and an absent group is silence.

**From `weaver-traits`.** The message model. The replayed contributions cross
this seam carrying conversation messages in whatever shape that crate
defines, and this contract does not redefine them. The harness draws it and
the recorder does not, those payloads being opaque to the recorder on the
precedent `weaver-harness-trace-contract` sets, so the name crosses in one
direction only.

**From `weaver-types`.** Nothing. The clause is present with that answer
because `weaver-types-PRD` section 5 asks for it even when it is empty: the
recorder receives events and never reads the operator's declaration.

**From `weaver-trace`.** Nothing drawn. The canonical form this record renders
in is `weaver-trace-Spec` section 2's, which `weaver-diagnostic-PRD` section 6
fixes as this record's and names authoritative under G5, **and that is a rule this
seam follows rather than a definition it draws**: no crate here links that one, no
name crosses, and a divergence from it is a defect against that section rather than
a breach of this contract. The clause is present with that answer because a group
is stated even when empty.

**From `weaver-diagnostic`.** The `diagnostic-trace` itself, the closed
diagnostic event-kind vocabulary and its per-kind payload shapes, the
`replay-outcome` the closing kind carries, and the failure vocabulary of
section 5. These are the party's own definitions, named because they cross
the seam and a reader of this document should not have to infer where they
come from.

**Nothing from any other crate.** This seam is a library boundary between two
crates and touches no third.

```graph
edge: draws
from: weaver-harness-diagnostic-contract
to: message-model

edge: draws
from: weaver-harness-diagnostic-contract
to: diagnostic-trace

edge: draws
from: weaver-harness-diagnostic-contract
to: diagnostic-event-kind-set

edge: draws
from: weaver-harness-diagnostic-contract
to: diagnostic-payload-shapes

edge: draws
from: weaver-harness-diagnostic-contract
to: replay-outcome

edge: draws
from: weaver-harness-diagnostic-contract
to: diagnostic-failure-vocabulary
```

## 1. What this contract governs

The production seam of the second record: how an authored diagnostic event
becomes the outbound stream, what each party guarantees to the other, and
what happens when either fails.

It does **not** govern the content of events, which is the harness's, nor the
internal rendering mechanism, which is `weaver-diagnostic`'s, nor how any
consumer reads a finished diagnostic-trace, which is downstream of the sink
and no agreement of this seam's.

**It governs one record and never the other.** A serving binding authors
through `weaver-trace` under its own contract and reaches this seam at no
point, and a diagnostic binding authors here and reaches that one at no
point, per `weaver-agents-PRD` section 6 as ruled 2026-08-24: the kind
selects the mechanism, and the selection happens once, at the fan-out, from
the kind the enter declared.

## 2. One exchange

**The emit, and nothing else.** This seam carries no resume, no read-back,
and no query: the recorder renders outward and reads nothing, per
`weaver-diagnostic-PRD` section 2, so the exchange that happens once per
event is the whole of the traffic.

One authored event moves through four steps, in this order, always.

1. **Submit.** The harness offers an authored event carrying its diagnostic
   kind, its payload, its session, run, and turn identity, its producing
   subsystem, and both timestamps. It does not carry a sequence number.
2. **Admit or refuse.** `weaver-diagnostic` validates the envelope against
   the kind's accepting shape and admits the event, or refuses it with a
   named failure and no partial effect.
3. **Order and canonicalize.** An admitted event is assigned its sequence
   and rendered to canonical bytes once.
4. **Write.** That one rendering is handed to the sink, and the assigned
   sequence returns to the harness.

**The order is the guarantee, and it is one-directional.** Admission precedes
the write, so no refused event reaches the sink, and the sequence the harness
holds names the same event the record carries. **It does not guarantee that
every admitted event reaches the sink**: a write can fail after an admission,
which section 5 names as terminal for the record, so what admission buys is
that the sink holds nothing the recorder refused and never that it holds
everything the recorder took.

**There is no working structure on this seam, and its absence is the
difference that matters.** `weaver-trace` holds the run's events in RAM
because the harness reasons over the present turn, per that crate's
section 4. A replay's present is the record it reads from, which lives in
`weaver-state`'s holdings and reaches the loop through the replay ask of
`weaver-harness-state-contract`, so a second in-RAM copy here would hold what
the loop already has by another road. The recorder therefore lands its
rendering in one place and the acknowledgment is the sequence.

## 3. What the harness owes

- **A bracket per pass.** Every run of a replay opens with the opening kind
  and closes with the closing kind, and the closing kind carries the pass's
  outcome. A pass that ends without its closing event is a pass that died,
  and the record says so by the absence, per section 5.
- **One outcome, stated once.** The closing event names which outcome the
  pass reached, from the closed set `weaver-diagnostic-Spec` declares, and
  names it from what the pass observed rather than from what it expected.
- **The record it replays, named once it is known.** The opening event names
  the elections the pass ran under, which the load declares, and the identity
  of the record being replayed crosses on its own kind once the pass has
  established it, that fact not being in the harness's hands when the bracket
  opens. A pass that never establishes it authors no such event and invents
  none. **The envelope's own session is the diagnostic run's**, not the
  replayed one, because a derived record
  that wore its source's name would answer the first question a reader asks
  of a file with the wrong answer.
- **Envelope completeness.** Every event carries the envelope whole, with the
  turn and the causal parent present exactly where the event has one and absent
  where it does not, neither ever filled to satisfy a shape.
- **Its own judgment kept to itself.** What a reading means is the reader's
  business, per `weaver-diagnostic-PRD` section 1, and no interpretation
  crosses this seam.

## 4. What the recorder owes

- **Ordering and canonical form.** A gapless run-scoped sequence over the
  run's whole admitted traffic, and every event rendered in the canonical
  form `weaver-trace-Spec` section 2 fixes, that section being this record's
  authority per `weaver-diagnostic-PRD` section 6. What that form is stays
  there rather than being restated here, a contract naming the obligation
  and the representation naming the shape.
- **Admission before the write, always**, so a refusal touches the sink at
  no point.
- **One rendering.** The bytes the sink receives are rendered once, so two
  readings of one event are byte-identical.
- **No content judgment.** The recorder validates shape and never meaning,
  which is the same division `weaver-harness-trace-contract` section 4
  draws: a payload well formed as octets and matched to its kind is admitted
  whatever it says.

## 5. Failure vocabulary

**A refused submission has no effect on the sink.** The recorder names the
refusal and the harness holds an unrecorded event, which is the harness's to
answer with a fault under its own contracts rather than a condition this seam
resolves.

**A sink that will not take the write is terminal for the record.** The
recorder reports it once, named, and the harness authors no more into a
record that cannot receive: a diagnostic run whose record died has lost the
product it exists to make, and continuing would compute a replay nobody can
read. What the harness does with that is its own, per
`weaver-harness-Spec`, and this seam requires only that the failure is named
rather than swallowed.

**A pass that dies mid-replay leaves an unclosed bracket, and that is the
record telling the truth.** No party manufactures a closing event for a pass
that did not reach one, because a fabricated outcome is worse than an absent
one: the reader's four outcomes, per `weaver-analysis-PRD` section 4, are
certified, diverged, abandoned, and unended, and the fourth is exactly the
absence of the other three. Writing one at a death would collapse the fourth
into the third and tell a reader a run ended when it stopped.

## 6. What neither party may do

- Neither party reads a record through this seam, in either direction. The
  recorder renders outward and reads nothing, and the harness's reading of a
  replayed session comes from `weaver-state` under a different contract.
- Neither party exposes this seam to the model. There is no tool, no verb,
  and no path from the loop's interior to either end.
- Neither party authors into the other record. A crate that submitted a
  diagnostic event to `weaver-trace`, or a serving event here, is outside
  both contracts and is a defect.
- Neither party names the sink. Admin opens it by the discriminant the
  declaration named and hands a handle down, per `weaver-admin-Spec`
  section 5, and the recorder holds a handle and never a path, on the same
  ground `weaver-harness-PRD` section 5 holds it for the serving record.

## 7. Change protocol

A change to the diagnostic event-kind set, to a per-kind payload shape, to
the outcome vocabulary, or to the exchange's order touches this contract, and
every party merges in the same act. A change to the canonical form is not
this contract's to make: that authority is `weaver-trace-Spec` section 2's
per `weaver-diagnostic-PRD` section 6, and this seam follows it.

## 8. Conformance

The seam is exercised by a replay that runs: a bracket opened, events
admitted in order, an outcome named at the close, and the rendering read back
off the sink in the canonical form `weaver-trace-Spec` section 2 fixes,
which section 4 of this document names as the recorder's owing. The refusal
path is exercised by submitting an event whose payload does not match its kind
and confirming the sink is untouched. The death path is exercised by ending a
pass without its closing event and confirming the record carries an unclosed
bracket rather than a manufactured outcome.

**The null replay of `weaver-diagnostic-PRD` section 4 is what certifies the
mechanism**, and it is owed behind this contract's own Spec rather than
asserted here.
