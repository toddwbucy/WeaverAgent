# weaver-harness / weaver-spu - classify contract

**Status:** MERGED. Cut with the classifier act of `weaver-spu-PRD` section 15,
per apex section 10, and merged on the human's call like every document, the
header flipping at that call per the Working Process. It governs the label
seam: the third seam between the same parties, on its own socket, per section
13.1's rule for later operation types, exercised for the first time by this
act. The residency and token seams keep their own contracts and no document of
the three restates another.

**Date filed:** 2026-08-19
**Document ID:** `weaver-harness-spu-classify-contract`
**Parent:** `weaver-agents-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #300

---

## 0. What this document is

The agreement over the label seam: how the harness's content reaches the
classify artifact and how the scored labels return, what each party may rely
on, and how it fails. It is read alongside `weaver-harness-PRD` and
`weaver-spu-PRD` section 15, and none of the three is complete without the
others.

It carries no representation. The types it names have a definition site and no
field list here, the ordering it fixes is stated as a rule rather than a state
machine, and how any of it is encoded is the Spec's.

**The name carries the seam's traffic, because three seams between one pair
need three names.** The Document Format names a contract for its parties,
initiator first, and every seam between this pair has the harness initiating,
so the party rule alone cannot distinguish them. The `-classify-` infix is the
distinguisher, naming the governed traffic the way `-decode-` names the token
seam's, and both older names stand unchanged.

**This document does not draw `weaver-organ-channel`.** The label seam is not
an organ channel, per `weaver-spu-PRD` section 15.2: the organ test names one
two-initiator channel and the lifecycle channel is it. The channel mechanics
this seam needs are stated in section 1 as its own, and the organ envelope
does not cross here.

```graph
node: weaver-harness-spu-classify-contract
kind: document

edge: party
from: weaver-harness-spu-classify-contract
to: weaver-harness

edge: party
from: weaver-harness-spu-classify-contract
to: weaver-spu

edge: draws
from: weaver-harness-spu-classify-contract
to: label-directive

edge: draws
from: weaver-harness-spu-classify-contract
to: label-answer

edge: draws
from: weaver-harness-spu-classify-contract
to: label-refusal

edge: draws
from: weaver-harness-spu-classify-contract
to: fault-report
```

**The seam edge is declared by the organ and appears in `weaver-spu-PRD`
section 6.** This document names its parties and does not restate the edge.

## 1. The channel

**An unaddressable connected pair, created by the harness before the child exists,
beside the lifecycle and token pairs.** **The classify process receives one end when it
is created and the harness retains the other**, per `weaver-spu-PRD` section 15.3, and
it holds no handle beyond what that section names. Possession authenticates, because a
pair
no third party can address has no second opener, and the channel lives exactly as long
as
the process at its far end.

**One write is one message, and the property comes from the socket type.**
The seam's traffic is framed by the channel rather than by a layer above it,
so this contract carries no framing. Which socket type supplies the property
is the Spec's election. This seam owes no hot-path measurement: its traffic
is one ask and one whole answer, never a per-token stream.

**Non-inheritance on both ends, by the split the corpus already runs.** The
harness's end carries the flag from the pair's creation, and the classify
process's end is set after its final image replacement, so nothing either side later
spawns inherits a handle to this seam.

**Closure is not an answer.** A closed channel with an exchange outstanding
is the far party dead mid-exchange, treated as that exchange's failure and
never its success, and what a death means is section 5's.

## 2. The exchanges

One exchange and two emissions, the dataflow rule of the token seam holding
here unchanged: a harness-opened exchange asks and its answer returns what
the ask produced, and an emission is owed nothing back, the trace entry
being the acknowledgment.

**Readiness, emitted once.** The classify process admits its artifact at
start, per `weaver-spu-PRD` section 15.3, and its first message on this seam
is the admission's outcome: ready, or a typed refusal naming itself the way
the model's admission refusals do, either traveling in the enter aggregate
as the fan-out arm's answer. The harness asks nothing before it, and a
directive arriving before readiness anyway is refused as not ready and
never answered late, section 3's ordering rule stating the same edge from
its side.

**Classify.** Opened by the harness, carrying the content to classify and
the turn's trace context, per apex invariant 5.3. The answer returns whole:
every label the artifact's head defines, each with its score, and the trace
context echoed back. There is no session and no accumulation: the exchange
is stateless, each ask independent of every earlier one, and two identical
asks answer identically within one admission. A content exceeding the
artifact's own bound is refused typed with the bound named. The label set is
the artifact's and never the ask's: an ask carries content alone, per
`weaver-spu-PRD` section 15.4.

**A refused ask reaches the record as a refusal, 2026-08-22.** Per the
operator's ruling that a refusal is clerked in one kind for every seam. The
harness authors `classify.request` when it sends and, on a refusal, authors
`refusal` carrying this seam's own typed case rather than a
`classify.output`. **The exchange is recorded whole either way**, which is
what `weaver-spu-PRD` section 15.5 demands, and what changes is which kind
carries the half that was refused.

**The refusal carries no ask**, alone among the seams in the class. Its ask
is the content, and `classify.request` already holds that under its own
kind, so reproducing it in the refusal would be one fact in two places.
**Where an ask already reaches the record the refusal names the seam and
stops**, per `weaver-types-Spec` section 4's rule for the record.

**This retires a shape rather than adding one.** The refusal travelled to
the record as a string inside `classify.output`, and it was the one refusal
in the program that reached the record at all. Being free-form, it was also
the one example of the shape the class rejects.

**Report a fault.** Emitted by the classify process at rest, carrying a
`fault-report` naming a case of `weaver-spu-PRD` section 15.6, and owed
nothing back. A fault arising while the classify exchange is outstanding is
that exchange's typed answer rather than a report, so a report never
interleaves an exchange.

## 3. Ordering

- Readiness precedes everything: no classify is answered, and none is asked,
  before the readiness emission, and a directive arriving earlier is refused
  as not ready.
- One classify is outstanding at a time, and its answer closes it before the
  next opens.
- The answer arrives whole or not at all: there is no intermediate message
  on this seam.
- The fault report is emitted only at rest, no exchange outstanding, and it
  takes no answer.

## 4. What each party supplies and guarantees

**The harness supplies** content within the artifact's bound, the turn's
trace context on every ask, and nothing else: no label set, no threshold, no
statement of purpose, per the prohibition below.

**The classify process guarantees** that the answer is complete, every label
of the artifact's head scored with none elided, because a silent top-k would
be this crate deciding what matters, which is the asker's business. That the
scoring is deterministic within one admission: same content, same scores.
That nothing is retained between exchanges: produced, answered, gone. That
the trace context returns exactly as it arrived.

## 5. Failure

A death is observed through closure, per section 1, and it fails the
outstanding exchange. The leg is optional by presence, per `weaver-spu-PRD`
section 15.3: the harness converts the dead seam into the same absence a
missing leg serves, the asking loop loses its judgment and never its turn,
and the death is recorded per the fault custody rule where an account
exists. A typed refusal is not a death, and the seam keeps serving after
one.

## 6. Prohibitions

- No session state and no retention: nothing an exchange learns outlives its
  answer.
- The loop's why never crosses. This seam carries what to classify and never
  what the answer is for, per the operator's ruling of 2026-08-19: the loop
  is the operator seat, and this crate processes.
- No per-ask label set and no per-ask threshold: the artifact defines the
  head and the asker judges the scores.
- The classify process asks nothing on this seam, holding no exchange in
  which to ask, the token seam's dataflow rule generalized to this one
  deliberately and to no other seam.
- The organ envelope does not cross here, per section 0.

## 7. Vocabulary

Every contract names the vocabulary it depends on, grouped by the crate that
defines it, and a group is stated even when empty.

**Drawn from `weaver-types`:** the label trio, `label-directive`,
`label-answer`, and `label-refusal`, with the cases section 2 enumerates. The
naming extends the ruling the token trio extended: this seam's loop is loop
1, the builder's and variable, so the trio is named for the seam's currency,
the label, which is the naming ruling's second case. The definitions land in
`weaver-types-PRD` section 2.3 and are owed by this act, the demand existing
now, and the records are written unfenced deliberately so a mapper reading
this document does not ingest records this document is not the source of:

    node: label-directive
    kind: vocabulary

    node: label-answer
    kind: vocabulary

    node: label-refusal
    kind: vocabulary

    edge: defines
    from: weaver-types
    to: label-directive

    edge: defines
    from: weaver-types
    to: label-answer

    edge: defines
    from: weaver-types
    to: label-refusal

**Also drawn: `fault-report`**, the one definition every reporting seam
carries, here inside the trio's cases rather than inside an envelope, the
classify socket carrying none, the same definition with a further carriage
rather than a further definition.

**Drawn from `weaver-traits`:** nothing.

## 8. Conformance

How each check is implemented is Spec work. What must be checkable:

- Statelessness: two identical asks within one admission answer with
  identical scores, watched to fail when anything is retained between them.
- Completeness: the answer's labels are the artifact head's exactly, each
  present once and none beyond, with a silent top-k, a duplicated label,
  and an invented label the perturbations that must fail. A count alone
  cannot check this, because a duplicate hides a missing label behind a
  matching total.
- The bound refuses typed: a content past the artifact's bound answers the
  refusal naming the bound, and the seam keeps serving after it.
- Readiness gates service: a classify before the readiness emission is
  refused as not ready, never answered late.
- The trace context echoes back byte-exact on every answer.

## 9. What this document changes elsewhere

`weaver-spu-PRD` gains section 15 and the third seam in section 6, landed in
this act. Owed beyond it, each named per G7: the label trio's definitions in
`weaver-types-PRD` section 2.3 and their shapes in `weaver-types-Spec`, with
the floor's act. The classify event kinds in `weaver-trace-PRD` section 3
and their payloads in `weaver-trace-Spec`, per section 15.5's demand. The
harness's papers, the fan-out arm and the seat's ask port, at the classifier
code act's opening through `weaver-harness-Spec` section 6's front door. The
declaration's classify binding beside the model binding, in the declaration's
papers with the same code act. Until each lands, the owing is this
document's and this section is where a reader finds it.
