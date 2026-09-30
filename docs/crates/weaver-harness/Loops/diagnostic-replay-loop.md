# Diagnostic Replay Loop

**Status:** MERGED v0.4, 2026-08-25. The workflow document for the diagnostic replay
loop, filed under the harness's `Loops/` container per the Document Format's container
entry. It argues no edges of its own: the seams it walks are declared in the crate
charters, and a graph block here would duplicate a record that already has a home.

**Document ID:** `diagnostic-replay-loop`
**Parent:** `weaver-harness-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #650

---

## 1. What this loop is

The diagnostic substrate's workflow, per `weaver-diagnostic-PRD` section 3:
the loop that re-executes a finished session's forward passes under a
diagnostic binding, with whatever passive readers the load elected observing.
The null replay of that charter's section 4 is this loop run with no reader
elected, and every certified instrument after it is this loop with a reader
on, which is what the charter means by the class being authored once.

**It runs in the harness's seat, as the agent's declared loop.** The diagnostic
declaration names this loop the way any declaration names its loop, riding `loop_file`
per `weaver-harness-PRD` section 2, and it runs at the run's opening rather than on an
arriving frame, because under a diagnostic binding no frame can arrive, there being no
Gate. **That entry landed 2026-08-31**: `weaver-harness-Spec` section 6.2
carries the second criterion beside the frame's, the seat granted once at
the run's opening on the run itself as the work, the sealed preload having
arrived owed its certification. An earlier
form of this sentence rested it on the precedent the context-injection loop set, which
does not carry it: that loop gained a port on a seat a turn had already granted, and a
turn begins at the gate and nowhere else. The loop composes what the seat grants and
nothing else - the state port, the decode surface, the flush and, since 2026-09-26, the
elision - and it mints no port, per `weaver-harness-Spec` section 6.

**What it refuses is the substrate's three refusals**, per the charter:
nothing enters from outside, the working structure is preloaded and read
positionally, and nothing writes back to what is under examination.

## 2. The walk

The operator sequences three acts, and the loop is the third:

1. **Load the diagnostic agent.** The declaration admin loads arrives
   already derived, step 2's first half having run before this one, per
   `weaver-analysis-PRD` section 3 as amended 2026-09-01. Admin stands the
   interior without Gate and
   the state member with its preload door, per `weaver-agents-PRD` section 6
   and `weaver-analysis-state-contract`. The run opens, the loop takes its
   seat, and its first act is the replay ask below, so the agent at this
   moment is waiting on custody rather than idle. **This loop's entry is not
   a turn's.** A serving loop runs when work arrives through Gate, and a
   diagnostic binding has no Gate, so nothing arrives to start this one: it
   runs from the run's opening. The mechanism is the loop entry's concrete
   signature, deferred per `weaver-harness-Spec` section 6.2 and listed in
   that Spec's section 9, and this document records the difference rather
   than inventing a lifecycle event for it, a declared trigger nothing
   raises being the empty joint apex section 9 refuses. **The seat's second
   criterion landed 2026-08-31 in section 6.2**, this loop having been the
   creditor that named the gap: under a diagnostic binding the seat is
   granted once at the run's opening, on the run itself as the work, and
   the two criteria partition by binding with neither widening the other.
2. **Run the driver.** `weaver-analysis` parses the operator-held record
   outside the agent, dials the preload door as an operator principal,
   sends the election and the distillates, and seals. **The driver also
   derives the declaration step one loads**, per `weaver-analysis-PRD`
   section 3 as amended 2026-09-01 on issue #394: every source-run fact
   from the record, the analyst declaring only device placement, the
   readers' elections, the sink, and a distinct diagnostic destination. The
   derived declaration and the explicitly diagnostic preload name that same
   destination, while the source facts stay record-derived. Derivation runs before step
   one and the preload after it - one act of the driver's, bracketing the
   load. **What the diagnostic election
   holds is `weaver-analysis-Spec` section 3's**, composed from what this
   loop reads, and what follows sketches its shape rather than fixing the
   set: the message kinds, `model.request` for each turn's rendered
   contribution and template identity, `model.measurement` for the recorded
   token path the certification compares against, and **`load`, without
   which step one of section 3 cannot read the tee's election and no claim
   about the state can stand**. A divergence between this sketch and that
   section is a defect against that section.
3. **The loop replays.** Its replay ask answers at the seal, per
   `weaver-harness-state-contract` section 2, returning the session's
   elected events in landing order. The loop walks the turns positionally
   and re-feeds each turn's recorded contribution through the decode seam,
   and the readers elected at the load observe the forward passes.

**The walk's unit is the generation, grouped from the envelope, and a
grouping that does not resolve rejects the replay.** The answer's events
group by run and turn from their envelopes, in landing order. Turn-bearing events feed
the walk, and turnless ones, the seated prefix, the run brackets, a recall, inform input
identity and feed nothing positionally. **A resident edit is a step of the walk**, as of
2026-09-26 (#673 item 1): a flush or an elision the source made between two turns is
driven through the seat's own port with the record's counts before the later turn
re-feeds, so that turn draws on the resident the source's did, and a reproduction whose
counts disagree with the record's closes the pass abandoned at identity, naming both. An
edit landing while a request stands unpaired, or holding no counts, rejects the
grouping. The M1 run `m1-002` flushed at 255 and re-entered at 259, and a walk that
passed the flush by re-fed turn 37 onto every earlier turn's tokens and diverged there.
**A source whose open recorded an `identity_prefix_unrecorded` fault does not certify**:
the identity step refuses it, the input its run opened under being missing from the
record, and a fault of any other case passes by. Within a turn, `model.request` and
`model.measurement` events pair in landing order, each request to the first unpaired
measurement after it, because a turn holds one pair per generation and tool rounds make
several generations of one turn the ordinary case. A measurement with no preceding
unpaired request, a request left unpaired at the turn's end, or counts that disagree
reject the replay before any forward pass, naming the run and turn, and nothing is ever
paired across turns: a replay over a grouping the record does not determine would be a
replay of a conversation that never happened.

**The re-feed is by the record and never by rendering.** The loop feeds the
rendered contribution `model.request` recorded, per the operator's ruling of
2026-08-12 that the record holds the rendered form precisely so a replay
does not re-render through a template that may have changed. Tokenization
identity is verified against the recorded token identifiers of
`model.measurement`, which is `weaver-agents-PRD` section 8's reproducible
claim exercised rather than assumed, and a mismatch is a failed certification and not a
lesser reading. **The post-flush input is the recorded rendered delta**: the re-entry a
loop builds after a flush is inside the later turn's `model.request`, so the replay
re-feeds it as it re-feeds any turn, and the source's `recall` event, which the holdings
carry, names the custody events that re-entry was drawn from. **A post-flush request
with no recall before it does not certify**, as of 2026-09-26 (#690 item C2.10): on a
record new enough to carry the `recall` kind, the identity step requires a recall of the
`recall` verb between each flush and the next `model.request`, and refuses the source,
naming the flush and the turn, where one is missing. The enter's identity recall does
not stand in for it, being the answer to another ask. **A record is new enough when its
holdings carry any `recall`**: the schema carries no format marker, by the versionless
rule, and the kind's presence is what says the record's recorder and seat record
recalls. A record holding none is read as older than the kind and certifies as before,
which is also the case of a run with no state member, whose seat has no recall to make.

**The re-feed exchange landed 2026-08-31 and this walk runs against it.**
The serving seam's append-and-generate samples, and a replay samples
nothing: the drive is the decode contract's sixth exchange under
`weaver-spu-PRD` section 13.14, appending the recorded path, running the
forward passes, computing each draw as a generation would and appending
the recorded token whatever the draw said, answering in its own type with
the recomputed draws in the measurement. An earlier form of this paragraph
named the drive as the one act standing between these papers and a running
null replay, and that act is the one that cleared it.

## 3. Certification, walked

The charter's section 4 procedure, as this loop performs it:

1. **Input identity first.** The loop establishes, from the answered
   holdings alone, that what it is about to feed is what the record says was fed: the
   rendered contributions, their template identities, the sampling parameters, the model
   identity and weights hash against the binding the load declared, and the prompt-block
   partition. A record missing what its claim requires fails here, before any forward
   pass, which is the completeness-is-claim-relative rule doing its work. **A claim
   about the state rests on one fact beyond that list, and it is claim-relative the same
   way**, per the charter's section 4 and `weaver-agents-PRD` section 8: the tee's
   election. It is the rule that decided what the original agent's state held, so a
   projection under a different one builds different holdings. This workflow
   explicitly names that projection as a different diagnostic session, and token-path
   certification does not certify those holdings as the original state. A claim of
   reconstructed state requires a separate comparison under the recorded rule. **The
   loop reads it from the record's `load` event and never from the holdings**, which
   is why the record carries it at all: the holdings are what that rule produced, so
   recovering the rule from them would be
   reading a projection to learn what did the projecting, and a rule that dropped a kind
   entirely leaves nothing behind to read. **A record written before that member existed
   fails a claim about the state and not a claim about the token path**: the null replay
   of step 2 rests on the recorded identifiers alone and stands, and everything resting
   on the holdings does not. The loop says which the record is good for rather than
   replaying under a guess or refusing a run it could have made.
2. **The null replay.** No reader elected. The recorded path re-feeds, the
   recomputed token identifiers match the recorded ones exactly, integers,
   or the certification fails naming the first divergent position, in the
   coordinate `weaver-diagnostic-Spec` section 3.3 states, the resident length
   at the draw, so the divergence and the field row it fell in share a key.
3. **Again, with the reader, as its own load.** The readout election rides
   the declaration and is read at the load, per `weaver-agents-PRD` section
   8, so the reader pass is a second load of the same declaration with the
   election on rather than a later phase of the first: the decode state is
   fresh by construction, each pass runs as its own run with its own run
   reference, and the two passes' records never share a bracket, which is
   what keeps the token-path comparison and the passivity claim clean of
   each other. The token path must match as before, and the reader's
   presence changing
   any token is the read failing its own passivity claim. The reader's
   vectors compare exact within one device model and within the GPU float
   tolerance across models, per the charter's section 4 as measured
   2026-09-01.

**A failed certification refuses the readout and keeps the run.** What was
attempted and where it diverged lands in the run's own record, the
diagnostic-trace this run authors through `weaver-diagnostic`, per
`weaver-agents-PRD` section 6 as ruled 2026-08-24. `weaver-analysis` reads it
off the sink and judges there. An earlier form of this paragraph had the run
authoring nothing and the divergence reaching the driver as an answer, which
the same date's ruling replaced. The capture artifact act shapes what an
artifact would have carried. An uncertified replay
producing no artifact is the no-second-instrument rule's floor case: not
even the first instrument reads from a replay that did not certify.

## 4. Failure, in the loop's terms

- **The replay answer is absent, for any of its reasons.** The seat's
  replay port serves nothing alike for a leg that is down, an answer that
  is malformed, and a bound that expired on a seal that never came, per
  `weaver-harness-Spec` section 6, and the loop does not tell them apart
  by outcome: none is an empty replay. An empty session is a sealed answer
  carrying zero events and fails input identity in the ordinary way, where
  an absent answer is no answer at all, so the loop ends its run having
  replayed nothing and the account says so in the diagnostic-trace, which is
  how the operator learns it, before unloading and retrying. A dead driver's
  prefix needs no cleanup, the next preload's opener retiring it.
- **The holdings fail input identity.** The loop refuses at certification
  step one, named above.
- **The decode seam refuses mid-replay.** The loop ends the run and the record
  it authors carries where the replay stopped. **The no-write guarantee is the
  third refusal's and does not reach that record**, which is the point of
  authoring one: a partial diagnostic-trace is the honest account of a replay
  that stopped, bracketed and readable, and refusing to write it would leave
  the operator with a silence to interpret. What the guarantee scopes to is
  what a later reading would rest on: no holding and no artifact takes a byte
  from the partial work, and `weaver-analysis` produces nothing downstream of
  a replay that did not certify. The decode context the forwards
  mutated and whatever measurement work was in flight are transient, die
  with the run at the leave, and a retry is a fresh load meeting them
  never, which is the same freshness the certification's two-load split
  already rests on.

## 5. What this document does not carry

The capture artifact - identity, custody, dataset shape, quota - is
`weaver-analysis-PRD` section 4's owed act, that crate holding the reading, and this
loop writes no artifact until that act lands. The re-feed exchange is the decode seam's
owed act, named in section 2. The driver's own shape is `weaver-analysis-Spec`,
landed 2026-08-27.
**The record's shape landed 2026-08-27** in `weaver-diagnostic-Spec`, which
carries the nineteen kinds of the record since 2026-09-26, seventeen from 2026-08-31,
the flush and the elision among them because section 1's grant names both and the loop
reproduces a source's, the recall because the replay port authors one, the refusal
because that grant and section 4's third failure both produce one, and the outcome the
certification reaches. The seventeenth, `residual.column`, is the harness's authoring
from the seam's column stream rather than an act of this loop, so this loop's own
authored set is unchanged by it: the loop drives the replay and the columns ride the
pass where the ask stood, per that Spec's section 3.2. This document is the workflow
that binds them, and it moves when any of them lands, per the Working Rules on documents
that cite owed acts, which is what this entry records.