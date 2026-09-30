# Sketch: what is not built, not proven, or not measured

**Status:** SKETCH, opened 2026-08-23. A working register rather than a member of
the document set, and never ratified. It carries every case this program describes
and cannot yet stand behind, in one place, so a reader does not have to find them by
reading closely.

**Lifted from `weaver-tools-technical-report` appendix B** when that report was
retired on 2026-08-23, because the register outlived the document that housed
it. The report is at `git show 0bca344:docs/archive/weaver-tools-technical-report.md`,
git being the archive per the ruling of 2026-09-13. Its entries were
written as an appendix and are being reworked into a standing list, which is why this
is a sketch: the categories hold, the wording still points at a report in places, and
the whole thing is meant to be developed further.

**Document ID:** `sketch-what-is-not-built`
**Editorial:** Per the Working Rules.

## How to read it

**Each entry says which of the three it is.** Not built is a thing no code does. Not
proven is a thing code does that nothing has demonstrated. Not measured is a claim
with no figure behind it.

**The list shrinks as work lands**, and an entry leaves because something was built
and shown rather than because the wording softened. That rule is the whole value of
the register and it is easy to lose.

## What it does not yet do

- **It is scoped to the agent framework, with one exception it owns deliberately.**
  Suite topology is tracked here - the missing suite apex, and the consumers that
  parent to a suite level nothing governs - because no suite-level register exists to
  hold it. **What the consumers themselves cannot stand behind is not here**, and
  wants its own register once there is more than one of them.
- **Entries are not linked to the assertions or charter cells that also track
  them**, so an entry can close in a Spec and linger here.

---
**Not built, and chartered.**

- **The gate's agent-opened socket.** Chartered for registered applications that
  bind a listening port. No exchange of the harness-gate seam reaches it, and its
  contract is the tool workflow's to author.
- **The idle report.** No report authors without a turn because nothing authors
  one at all, which is what `harness-idle-report-authors-without-a-turn` waits on.
- **Client-facing streaming.** Deferred, and it arrives as an extension to the
  world contract rather than as a replacement for it. One line in and one line out
  is the resting shape.
- **The status ask.** `show` and `list` refuse today, the init system's three unit
  values not mapping onto the four agent states, and a translation is where
  invention would enter. The observation exchange retires the refusal when it
  lands.
- **The memory leg.** Out entirely, arriving through apex section 9's door as a
  socket peer with its own contract. No seam, stub, reserved slot, or dormant
  contract party is carried in anticipation of it.
**Not built, and not chartered either.** Named because a reader will look for them,
not because anything promises them.

- **Any seam over a wire.** The transports in code are Unix-specific, and a seam
  crossing a machine boundary would need its own framing and a peer-authentication
  mechanism to replace `SO_PEERCRED`. Neither exists. **Descriptor passing is the
  hard case of the three and is named separately for it.** `SCM_RIGHTS` has no
  wire analogue: a descriptor is a capability the kernel hands across a local
  socket and rechecks at no point afterwards, and there is nothing to send over
  TCP that is the same kind of thing. Custody rests on that mechanism, the sink
  reaching the recorder as an already-open descriptor and the recorder offering no
  call that takes a path, so a wire seam would need a different custody design
  rather than a port of this one. The topology would carry, the implementation
  would not.
- **Shard widths beyond two.** A pair is what the salvaged tensor-parallel path
  implements. An N-way forward and its all-reduce are work this program does
  rather than salvage it inherits.

**Built, and not yet proven.**

- **The GGUF tap's neutrality on a real device pair.** The tap landed 2026-08-22 and
  carries `spu-two-taps-one-shape`, so it is built and the container is no longer a
  ground for refusing. Its neutrality is watched clearing the no-token-change bar on
  the host backend, **which reaches everything but the one hazard the Spec names**:
  installing the callback turns one graph compute into a walk of windows, and a
  fusion candidate straddling a window boundary goes unapplied, which is a device
  concern no host run can reach. **The tap is not shipped against a family until that
  measurement is taken**, and the deployed family declares no tap, so nothing in
  service elects the path today.

- **Deterministic re-feed.** Apex section 8's second arrangement is the one argued for,
  and the demonstration on record is of the third. Pushing a
  recorded token sequence back through the forward pass with nothing re-sampled
  is owed its own run.
- **`spu-one-forward-per-prompt`.** Watchable under the standing native tap and
  waiting only on its count being taken.

**Claimed, and not measured.**

- **Latency is the enemy of agency.** The program's one conceded theory claim, and
  the seam argument marks it. No per-hop figure for loopback against a Unix socket at
  these message sizes has been taken in this repository.
- **The encoding's share of the per-token cost.** The seam argument concedes that
  serialization is paid on a Unix socket exactly as on loopback, and names JSON on
  the interior seams. The decode seam carries **one message per retained token**, so
  that cost lands per token, which is the same place the seam argument says a per-hop
  cost
  compounds. **If the encoding dominates what the transport saves, the argument's
  own logic points at the encoding rather than at the transport.** No measurement
  separates them, and the claim should not be published as settled until one does.
- **The surprisal correlation's provenance.** Whether r = -0.300 over two hundred
  rounds was pre-registered or found post hoc on the rounds the A/A ran on is not
  recorded, and the two carry different evidential weight.
- **The effective range of the score.** Every one of the fifty-seven rounds that
  answered turn two converted turn four, which suggests the usable range may be
  narrower than the four points the scale declares. If it is, every bound stated in
  points is tighter than it reads.
- **The entropy result's n and interval.** The measurement regime reports 59.5 percent
  against a
  51 percent base rate with neither, so what died on the evidence and what merely
  went undetected cannot be told apart from what is reported.
- **The headroom on the admit judgment.** A construction parameter at the worker's
  composition root until a measurement on a real artifact against a real device
  replaces it. Whether it is a constant, a fraction, or derived from the
  artifact's declared shape is unsettled.
- **Which reading admission judges free memory against.** The admission argument states
  the case for the driver over the crate's own ledger and does not settle it. What
  is owed is a ruling taken with a measurement of what a driver query costs on the
  admit path, and no driver query stands in the code today.

**Owed by the corpus rather than by the code.** These are gaps in what is written
down, not in what is built.

- **The measurement regime cannot be checked from the tree.** The measurement regime's
  registrations and results stand outside this repository. Until they travel with
  the release or move into it, its sources are the only ones a reader holding the
  tree cannot reach at all. The formalism citation below is the narrower case, its
  own claims being checkable where these sources are not.
- **The equivalence bound on the A/A test was computed rather than pre-registered**,
  from the reported means, standard deviations, and sample sizes. It is not a
  pre-registered power
  analysis, and a series designed against a target effect would state the bound
  before running rather than after.
- **The reasoning-loop formalism is not in the tree.** The builder's-seat clock argument
  cites it and it sits on an open pull request, so the citation resolves to
  nothing a reader can open.
- **`weaver-internal` is unclassified.** It fails the organ test and the submodule
  definition does not reach it, while its parent edge makes it a domain root.
  That is an apex question, and nothing settles what the crate is.
- **The `Parent:` headers and the graph parent edges disagree.** Every crate PRD's
  header names `weaver-agents-PRD` while its parent edge points at the `WeaverTools`
  system node. Document Format section 3 requires them to name the same thing and
  makes the header the defect. It closes when the domain level lands in the graph,
  which is the same act as the entry below rather than a second one.
- **The taxonomy's promotion is complete and its sketch is archived.** The operator's
  ruling of 2026-08-24 split it by what each document governs, and both halves now
  govern: the binding kinds in the apex's section 6 and
  `weaver-admin-harness-contract`, with the Spec and code acts landed the same date,
  and the loop class in `weaver-diagnostic-PRD` sections 3 through 5, the chartering
  act of 2026-08-24. What that charter leaves owed it names itself in its section
  6, and four of the five items this sentence once listed have moved: the seam
  papers landed, and the capture artifact, the driver with its Spec, and the
  instrument-suite sketch went to `weaver-analysis-PRD` section 4 when the
  operator's later ruling of the same date split the leg, `weaver-diagnostic`
  composing inside the harness as a mechanism and `weaver-analysis` taking the
  reading. What section 6 owes now is `weaver-diagnostic`'s own Spec, the seam
  it presents to the harness, and the null replay, which is the one item that
  never moved. The two use cases remain at
  `sketch-the-two-use-cases`, a sketch that decides nothing, and where they land is
  the entry below: a statement of what the program is for belongs at a suite level
  that has no document.
- **G2's transport silence has no stated scope, and the first case it meets is one it
  gets wrong.** The rule says a contract naming a path, a descriptor, a socket type, or
  a flag has taken a Spec's material. `weaver-admin-systemd-contract` names a Unix
  socket's pathname at section 118, and that sentence is doing the contract's own work:
  it is why the runtime directory is asked for, because a pathname outliving its binder
  is the failure the ask prevents. **Substrate is substitutable at a seam between our
  own organs and it is the counterparty at an external boundary**, and the three
  external contracts - systemd, operator, world - are the second kind. The rule was
  written for the first kind and does not say so. **This is left for the operator
  rather than carved out here**, because narrowing a gate's reach is a ruling and
  because over-applying this same ruling to the PRDs was already the week's mistake.
  A second question the same gap raises: the word socket survives widely in the seam
  contracts, and **the enumerated case is nonetheless clean.** The only two sites
  saying socket type are the decode and classify contracts, which say it to defer the
  election to the Spec, and no contract names a specific type. What survives is the
  bare noun for an endpoint. Whether the enumeration reaches that use is unsettled,
  and an earlier wording here called it substrate the sweep missed without checking.
- **The suite has no apex, deliberately, per the operator's ruling of 2026-08-24.**
  It is not an omission at this stage. **The architectural pattern is worth a rough
  draft PRD and the scope's details are not**, because those details are still moving
  and a document that pins them early costs more to unwind than it saves. The entry
  stays in this register as a state rather than a defect, and what would close it is
  the scope settling rather than someone writing the page. What follows below is
  therefore context and not an owed edit: the document that carried that name governs
  the weaver-agents domain, corrected 2026-08-23. Crates outside the agent boundary,
  `weaver-web` and `weaver-analysis`, parent to the suite and refine nothing that
  is written. Naming the gap is the honest form until a second consumer makes the
  suite-level claims obvious enough to state from evidence.
- **The apex counts two state holders and the tree has three.** `weaver-state` was
  chartered 2026-08-18 and holds across runs rather than merely across turns.
  Both are stated rather than chosen, and the amendment is the apex's act.
- **The agent formulation is not checkable from the tree.** It states what an agent is
  taken
  to be rather than what this code does, so a build neither confirms nor falsifies
  it, and its own status line says so. The argument behind the notation sits in the
  agent paper, which is not in this repository, so that citation resolves to
  nothing a reader holding the tree can open.
- **The loop numbering is unsettled between the two documents.** The formulation
  numbers the primary reasoning loop `L_0` where this program numbers the
  framework's service loop zero and the builder's reasoning loop one, per section
  11. Appendix C states the collision and names a candidate resolution it does not
  adopt. The ruling is the apex's.
