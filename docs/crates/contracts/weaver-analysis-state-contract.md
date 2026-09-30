# weaver-analysis / weaver-state - contract

**Status:** MERGED. In `main` and the source of truth.

**Date filed:** 2026-08-24
**Document ID:** `weaver-analysis-state-contract`
**Editorial:** Per the Working Rules.
**Landing PR:** #650

---

## Parties

- **`weaver-analysis`, the driver and the only sender.** Parses the finished
  trace outside the agent, as an operator principal over the operator's own
  storage, and sends across this seam what the parse projects. Decides what to
  send from the record's content and the election it declares, and asks
  nothing. It is also what reads the diagnostic-trace the run produces, per
  `weaver-analysis-PRD` section 1, so one crate stands at both ends of the
  replay and neither end is inside the agent.
- **`weaver-state`, the custodian.** The same member, the same charter, the
  same store, a second door. Receives the preload, holds it organized, and
  speaks at no time on this seam. What it holds here is answered on the other
  door, to the harness, exactly as if a tee had landed it.

No third party reaches this seam. The agent holds no end of it: the harness never dials
this door, the model has no path to it, and the door refuses the agent's own credential
at the accept. The one ruled crossing into the agent that is not the two external
contracts of 2026-08-01, per `weaver-agents-PRD` section 0 as amended 2026-08-24, and it
exists only where the load declared the diagnostic kind or elected a restore, per
section 1.

**This seam is a wire.** A named Unix socket on the state member, the member's one named
door since the first lost its name to the operator's ruling of 2026-08-26, stood under a
diagnostic binding or a serving load that elects a restore, per section 1, and
authenticated by credential per the first invariant's rule for a channel with a name.
Its name stands in the member's own territory on the operator's side, per
the same ruling: the driver is an operator principal and traverses, the worker's
identity holds nothing there, and the squat an agent-writable directory invited is
unrepresentable rather than defended. The door's absence under a serving load that
elects no restore is the charter's cheap refusal made structural: a driver pointed at
such an agent finds nothing to dial, and a serving load that elects a restore is
distinct from one by that election alone, admin having named the door on the vector for
it.

```graph
node: weaver-analysis-state-contract
kind: document

edge: party
from: weaver-analysis-state-contract
to: weaver-analysis

edge: party
from: weaver-analysis-state-contract
to: weaver-state
```

## Vocabulary

Every contract names the vocabulary it depends on, grouped by where it is
defined, and a group is stated even when empty.

**From `weaver-harness-state-contract`.** The `election` and the
`distillate`, defined there and drawn here rather than restated, one
authority per gate G5. The preload is the ingest direction of that contract
re-run with a different author, and a second definition of its nouns would
give the mapper two sources for one record. What those terms mean does not
shift with the author: the election is the opener declaring the session and
the elected kinds with their payload key paths, and the distillate is one
distilled event, envelope whole and elected pairs beside it.

**From `weaver-trace`.** The `canonical-event`, defined at `weaver-trace-PRD`
section 2.3, and its envelope fields, spelled as that crate's canonical form
spells them. The driver reads the record itself, outside the agent, and its
distillates are projections of canonical events exactly as the tee's are:
the source facts crossing this seam are facts the record carries. An explicit
destination rewrite changes the session name alone, as section 2 governs.

**From `weaver-types`.** Nothing crosses. The binding kind conditions this
door's existence and never rides it, the declaration having done its work at
the load, per `weaver-agents-PRD` section 6.

**This seam's own.** Nothing. A seam whose whole vocabulary is drawn is the
point rather than an omission: the preload's claim is that what it lands is
indistinguishable in the holdings from what a live tee under the same election
and destination session would have landed,
and a term of its own would be a place for that claim to quietly fail.

```graph
edge: draws
from: weaver-analysis-state-contract
to: election

edge: draws
from: weaver-analysis-state-contract
to: distillate

edge: draws
from: weaver-analysis-state-contract
to: canonical-event
```

## 1. What this contract governs

The second door on the state member: the preload traffic that flows when a diagnostic
binding stands or a serving load elects a restore, per `weaver-state-PRD` section 4,
the condition under which the door exists being that admin named it on the member's
vector under either, what each party owes, the cut and the session rewrite the driver
may apply to what it projects, what each party owes, how the seam fails, and what
neither party may do. It is read alongside `weaver-analysis-PRD` and `weaver-state-PRD`
section 3, and none of the three is complete without the others.

## 2. The traffic

**Preload, flowing, one direction, and the election opens it.** The first traffic
declares the effective election and destination session whole. Ordinary reconstruction
uses the rule recorded for the source holdings. A diagnostic projection uses the
driver's own rule only by explicit election of that behavior, and declares a nonempty
destination different from the source session. A whole ordinary reconstruction may
retain the source name or name a branch
while retaining the recorded rule. A cut requires a nonempty destination
different from the source. The receiving load, opener, and
projected envelopes name the same destination, because every answer on the other door is
bounded to the session that load declared.

**A selected record has one source session, and one standing carries one election.** The
driver refuses ambiguous source identity before opening the preload. Ordinary
reconstruction refuses absent or malformed governing election evidence and a selected
history requiring different rules within that one standing. It never invents a union or
chooses one run's rule for another. Equivalent effective rules are one election. The
driver preserves the source
rule's priority when multiple entries name the same kind, and resolves that
priority before comparing selections. Reordering entries is not evidence of
equivalence when it changes which entry governs a kind. An explicit diagnostic
projection may apply its own rule across source
runs, without manufacturing missing source evidence or certifying a claim whose evidence
is absent.

**Validation precedes retirement.** A cut under the source session name refuses
at the driver before any opener, even if the receiving load has already validated
its own declaration. This preserves the resume/branch boundary in
`weaver-state-PRD` section 4. Selection, destination, cut, and required election
evidence are checked before an opener crosses. A refusal leaves existing destination
holdings untouched. After the opener the driver sends one distillate per elected event
in the record's order and is owed nothing back. The fact has one home, and a
confirmation discarded by its only reader would be the retired receipt's error.

**The seal ends the preload, and the close alone does not.** After the last
distillate the driver sends the seal, one frame carrying nothing, and then
closes. **Carrying nothing is spelled**: an empty JSON object on its own line,
`{}` canonically and any spelling that parses to an object with no members,
every frame on this seam being a JSON object on its own line. A bare empty
line is a sender's framing residue and not a seal, so a driver that sealed
with a blank line has not sealed and its close reads as a dying sender's. A
close without the seal is a dead driver, per section 5, and the
distinction is the whole of the seal's job: a channel's close looks the same
from a finished sender and a dying one, and the party waiting on the
preload, the replay ask of `weaver-harness-state-contract` section 2, must
not answer over a prefix that looks whole. One standing of the driver, one
preload, one seal: the driver owes at most one preload per standing of its
own against the member.
**The owing is a live driver's, and the door outlives any one driver**: the
member stands the name again after any close of the preload channel, sealed
or sealless, for its own standing's life, which is the mechanism the
dead-driver retry of section 5 rides - a retry is a new standing of the
driver against the same standing of the member, its opener retiring the
dead prefix, and a driver that preloads twice on one of its own standings
has broken its owing whether or not the member's door would admit it.

**The opener retires the declared session's prior holdings, in the same
transaction that records it.** A preload therefore lands against empty and
never beside anything, whatever the store held for that session, a whole
earlier preload or a dead driver's prefix. That is what makes a retry a
replacement rather than a double, and it is the record re-asserting itself
rather than custodial judgment: the preload is the session's account
projected from the authoritative record, so what it replaces was at best an
older projection of the same authority. The retirement is this door's
opener's and no other traffic performs it - the first door never retires
anything, per its own contract.

**Completeness is not this seam's judgment.** The record is the authority on
what a whole preload would have carried, and whether the holdings match it is
the certification's question, per `weaver-diagnostic-PRD` section 4, answered
from the record and the shape ask rather than by any confirmation crossing
here. A driver that died mid-preload leaves a prefix, and the prefix is
custody doing its job on what arrived.

**No asks cross this door, ever.** The ask vocabulary lives on the harness
door alone, and the who-else-may-ask cell of `weaver-state-PRD` section 5
stands untouched: the driver is a sender and never an asker, so the serve
direction still has exactly the two ends it had.

## 3. What the driver owes

- **The parse kept outside.** State never opens a trace file, per its charter
  and the custody rule: what crosses is distillate-shaped already, the
  driver having read the operator-held record as an operator principal.
- **The envelope always.** Every distillate carries all five envelope fields
  as the canonical form spelled them, in the record's sequence order. An
  unattributable distillate is a defect in the sender, on this door as on
  the first.
- **The election faithful to what follows.** What the opener declares is what
  the stream delivers under that rule and the standing turnless-system exception
  drawn from trace Spec section 11. Other unelected kinds do not cross, and the
  declared session is the explicit destination or the source name retained by
  ordinary reconstruction. The receiving load names that same session.
- **Its own credential.** The driver dials as an operator principal and never
  as the agent, and a driver that cannot present that credential has no
  business this contract recognizes.

## 4. What state owes

- **Custody whole, indistinguishably.** What the preload lands is held,
  organized, and attributable exactly as a tee's landing would be, one store
  and one set of obligations, so the loop's asks on the other door answer
  against it with nothing marking how it arrived. That indistinguishability
  is this contract's whole point and the charter's inversion made real: same
  organ, opposite direction.
- **The refusal at the accept.** A peer bearing the agent's credential is
  refused before any byte is read, this member's one credential judgment
  since the first door authenticates by possession, and the door itself
  stands only where the binding declared the diagnostic kind.
- **Silence.** No traffic flows driver-ward, at any time, for any reason.

## 5. Failure vocabulary

**A dead driver costs the preload and never the holdings.** A distillate
lands whole or not at all, per `weaver-state-Spec` section 4's transaction,
so a driver dying mid-stream leaves a clean prefix and no corruption, and
the prefix is unsealed, so nothing downstream mistakes it for a whole
preload. The
recovery is the next preload's opener, which retires the prefix with
everything else the session held, per section 2, so no cleanup act exists
between the death and the retry and none is needed. What became of the
partial one is the certification's to notice, and it notices nothing once a
whole preload has replaced it.

**A malformed distillate is the sender's defect**, dropped whole without
closing anything, exactly as the first door drops it.

**A dead member costs the session nothing that matters.** The loss clause of
`weaver-state-PRD` section 3 covers this door as it covers the first: the
holdings are rebuildable by construction, here trivially, because the driver
re-preloads from the record it still holds.

## 6. What neither party may do

- Neither party writes the trace through this seam, in either direction. The
  record stays the operator's, read by the driver outside the agent, and
  `weaver-trace` keeps its write-only pin.
- Neither party exposes this seam to the model or to the loop's interior.
- Neither party persists anything across the session through this seam. The
  door adds no life to the store.
- State never learns a path. Distillates arrive as content, and everything
  path-shaped was resolved by the driver on its own side of the boundary.

## 7. Change protocol

A change to what crosses this door, to the door's standing condition, or to
the credential judgment touches this contract, and every party merges in the
same act. The `election` and `distillate` shapes are the drawn contract's
own: a change to them there reaches here through the draw, and such an act
merges the parties of both contracts.

## 8. Conformance

The door is testable against the living pair under a diagnostic load: a real
member stood with the preload name, a real driver landing a real record's
projection, and the holdings answering the shape ask on the harness door with
exactly the replayed session's runs. The wrong-peer refusal and the
kind-conditioned standing are perturbation-grade and their assertions live in
`weaver-state-Spec` section 4. How the member learns the preload name at its
standing follows the pattern of `weaver-state-Spec` section 2 and is elected
in the code act that opens the door.
