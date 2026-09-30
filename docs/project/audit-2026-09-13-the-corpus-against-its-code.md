# Audit: the corpus against its code, read through the graph

**Status:** AUDIT, 2026-09-13. A dated reading rather than a member of the
document set, and **nothing here is decided until the operator rules on it.**
It reports what `WeaverTools_v4` says about the tree at `e2a5378`, which is the
first time this program has been able to compare a claim in a document against
a count in code by query rather than by accident.

**Base:** `e2a5378`, the merge of PR #560. Every figure below was measured
against that commit on 2026-09-13 and re-derived independently before it was
written down.

**Method:** thirteen agents. Six audited one dimension each against the graph
and the tree, six tried to refute what the first six found, and one grouped the
survivors. The verifiers were instructed to default to rejecting when
uncertain, and every confirmed finding carries the command its verifier ran.
626 tool calls, about twenty five minutes.

## 1. What this cost and what it bought

**Thirty five findings survived verification.** Eleven of them are
structurally invisible to every one of the census's eleven readings.

That is the sentence this audit exists to produce. `census.py` reproduced its
baseline exactly at this commit - 48, 33, 12, 1, 0 - while eleven real defects
stood in the corpus it had just read clean. They are invisible for a reason
rather than by oversight: prose section references, edge endpoints, node kinds,
seam tags, a `compile-fail` tag cited at a file holding no instrument, and a
`review` tag standing over a test named for the claim it says was not bought.
**None of them is a claim a regex can reach**, which is what a graph is for.

**A clean automated gate is evidence the gate did not fire.** The corpus has
said so since before the graph existed. This is the first measurement of how
much it was not firing on.

**And the constraint that governs everything below.** `act-08` could take the
header backlog from 48 to 27 by authoring twenty one assertions nobody needs,
which converts an honest "unenforced" into "documented as enforced" - the
failure the perturbation rule exists to prevent. **The assertions come first
because they are owed, or they do not come.** It is stated again at section 8
and it is stated here because it decides whether the largest act in the epic is
progress or damage, and a reader who stops after section 4 should still have
met it.

## 2. The baseline, at open

```
sources_without_a_header                48
uncited_perturbations                   33
documents_without_an_enforcement_table  12
malformed_node_ids                       1
dangling_citations                       0
archive_directories                      0
```

If every act in section 5 lands, the projection is 48 -> 19, 33 -> 27, 12 -> 11,
and 1 -> 0. **The header number is the one to distrust**, for the reason
section 7 gives.

## 3. Where the findings fell

| dimension | found | what it can see that nothing else can |
|---|---|---|
| claim-vs-code-drift | 12 | a count in prose against a count of symbols |
| structural-orphans | 9 | an edge whose endpoint no document declares |
| headerless-sources | 6 | which crate the backlog sits in |
| enforcement-tables | 4 | a table against its own document's nodes |
| tag-vs-instrument | 3 | a tag against what the citing file holds |
| uncited-perturbations | 1 | an instrument that exists and is not cited |

Ten are high, twenty three medium, two low. By act size: fourteen are one
document, eight one crate, eight one line, five corpus-wide.

## 4. The findings worth reading first

### The census is wrong about the tree in exactly one place

**Five weaver-spu perturbations already have their instrument.** The corpus
records an instrument as owed that was in fact bought, and one `conforms:` line
per file closes each.

```
spu-seed-derives-per-generation         sampling.rs:546, with its own
                                        Perturbation: clause at 544
spu-sampler-holds-nothing-between-...   decoder/session.rs:1777
spu-elected-readout-changes-no-token    tests/readout_neutral.rs:3-6, which
                                        states the claim verbatim
spu-family-is-architecture-and-template  tests/selection.rs, 7 tests, 0 headers
spu-room-refusal-carries-capacity       gpu/mod.rs:224
```

The auditor put the last one at `gpu/mod.rs:184-199`. **The verifier found that
wrong** - that test asserts only `ordinal` and `needed` - and located the real
instrument 36 lines later, stronger than the finding claimed. That is the
verification tier earning its cost.

`readout_neutral.rs` is `cfg(all(feature = "gguf", feature = "cuda"))` and
cannot compile on a box with no `nvcc`. Per issue #397's parking that is a fact
the act states, not a reason to withhold the citation.

### The drift the graph was built to find

Twelve findings, and these six are the sharp ones:

```
weaver-types-Spec     FaultCase block lists ten, code compiles eleven, and the
                      same Spec's prose already says eleven
weaver-harness-Spec   Outcome pinned as "two cases" - the code has one, and the
                      same Spec's section 6 already says one
weaver-harness-Spec   "Nine library files" - the crate has twelve, four of
                      them never listed at all
weaver-gate-Spec      "Four files", omitting src/tools.rs - the tool executor
                      the same section's next paragraph argues for
weaver-types-Spec     Payload lists five variants, the code carries seven, and
                      neither Tool arm appears anywhere in the documents
weaver-trace          failure.rs's own module header says five cases and "a
                      sixth" - the enum has four, and the Spec and contract
                      both say four
```

**A compile-pin assertion is among them, and it is worse than a count.**
`crates/weaver-harness/src/failure.rs` pins `harness-outcome-two-cases` on line
2. Twenty lines below, `Outcome` carries one variant whose own doc comment argues
at length that a second would be "the reserved slot the apex forbids in data as
firmly as in an interface". **The pin does not only contradict a count. It
contradicts the reasoning printed in the same file, under the header carrying
it.** `crates/weaver-trace/src/failure.rs` is the same shape: the header says
five cases, the enum has four, and the "a sixth case reaches every caller" line
sits directly above the four. Sharpened by the olympus seat, 2026-09-14.

### Two graph facts no reading could reach

**Three `draws` edges name vocabulary nodes no document declares.** The
endpoints are `tool-name` and `canonical-event`, and the charters that would
declare them do not.

**Three real Cargo workspace dependencies have no crate-level edge**, which is
a direct H2 breach. One of them, `weaver-state -> weaver-trace`, is either a
floor-link or a seam needing a contract that does not exist.

### Citation forms the format does not admit

**Eighty conformance citations use comment forms the Document Format does not
admit**, and thirty two perturbations are credited only by them. The census and
the HADES mapper both accept the wider set. The format does not. One of those
three has to move, and which one is an operator decision.

## 5. Twenty acts, in eight tiers

No act proposes a behaviour change. **Every code edit in the whole epic is a
comment line, a doc comment or a prose section reference.**

```text
1  act-03 apex devices          authority documents the rest of the epic reads
   act-01 format vocabulary

2  act-02 system node id        same files as tier 1, and the only path that
                                takes malformed_node_ids to 0

3  act-11 grounds parity        changes the numerator four count acts write

4  act-09 draws targets         graph facts, both breaches, both invisible to
   act-10 floor-link edges      every census reading

5  act-12 spu-Spec              the drift tier, one act per Spec as the
   act-13 harness-Spec          standing direction requires. 12 through 15
   act-14 admin-Spec            wait on act-11, 16 through 18 do not
   act-15 trace
   act-16 gate-Spec
   act-17 floor documents
   act-18 web Spec sections

6  act-04 inline citations      header acts, all conditional on act-01's
   act-05 spu headers           ruling. act-04 first, because act-05 stops
   act-06 state headers         needing half of itself once it lands
   act-07 analysis headers
   act-19 web PRD placement

7  act-08 web headers           the largest act, and the only one authoring
                                rather than sweeping. If the epic stops
                                anywhere, it stops before this

8  act-20 census readings       turn the new readings on after the defects
                                they would report are cleared
```

**Tier 3 is the hard ordering constraint.** `act-11` changes the grounded
numerator that acts 12, 13, 14 and 15 write into prose. Landing it after them
means recounting twice.

**Tier 8 is deliberate.** Five new census readings turn on only once what they
would report is cleared, so each opens at zero or at a baseline the operator
declared rather than inherited.

## 6. Four decisions the epic carries

Each is a genuine fork. The audit established that two documents disagree, not
which one should move.

1. **Citation forms.** Widen the format to the four forms `census.py` and the
   mapper already accept, or hoist eighty citations into `//!` headers - which
   is impossible for the three manifest citations, since a `Cargo.toml` has no
   module header.
2. **`term` as a node kind.** Five nodes declare `kind: term`, which the format
   does not define, and four are sourced by a contract against an explicit
   rule. Admit a ninth kind, or retype the five and move four declarations.
3. **`verb` as a seam tag.** One seam carries it, and a contract asserts a
   three-value seam vocabulary the format does not carry.
4. **The system node's identifier.** `WeaverTools` is the one identifier the
   gate cannot read, and the format contradicts itself about it. Rename to
   `weaver-tools`, or carve a named exception and admit it in `NODE_OK`.

`act-10` carries a fifth: whether `weaver-state -> weaver-trace` is a
floor-link or a seam that needs a contract authored.

## 7. What this audit did not establish

**That the code is right and the documents wrong.** Every claim-vs-code finding
establishes only that the two disagree, and the audit's own example of this was
itself overstated. It hedged `src/classify.rs` as a file that may be an owed
placement, and **the Spec answers that on its own page**: section 1's listing names
`src/classify.rs` and closes on "Nine library files, two of them placements". A
placement is a declared device, so no hedge was needed.
`crates/weaver-spu/src/bin/classify.rs` also exists, so the never-existed claim was
true only of the harness path and false as summarized.

**The real defect in that listing is larger than the audit found.** The Spec
names nine and the crate holds twelve, and the gap is not classify. Four library
files exist that the listing never names - `failure.rs`, `record.rs`,
`replay.rs`, `spawn.rs` - and `src/tools.rs` is listed as "blocked" while
carrying 118 lines and its own `conforms:` header. So the count is wrong because
four files were never listed and one placement has been filled without the
listing moving. **`act-13` is rewritten to that before anyone works it**, per the
olympus seat's review of 2026-09-14.

**That anything compiles or passes.** No `cargo build`, test, clippy or fmt was
run. No perturbation was verified to fail on removal. The five weaver-spu
instruments were read, not executed.

**That the corpus is clean where this reports nothing.** Findings concentrate
where the queries pointed. `weaver-internal`, most contracts, `weaver-admin`'s
code, `weaver-gate`'s code and `weaver-diagnostic`'s Spec were barely touched.
**Silence here is absence of a query, not absence of a defect.**

**Two concrete instances of that, found by the olympus seat and not by this
audit.** Both are worth naming because each shows a different reason the method
could not reach them.

**The recursive mirror.** Document Format says a member crate nests inside its
root at both trees. `weaver-trace`, `weaver-state` and `weaver-diagnostic` are
documented under `docs/crates/weaver-harness/` while their code sits flat at
`crates/weaver-trace`, `crates/weaver-state` and `crates/weaver-diagnostic`, and
no ruling in Working Process carves an exception. If it is a defect rather than a
settled exception it is a three-crate move touching every path dependency in the
workspace, which makes it the operator's rather than a later mirror check's.

**The reason the audit missed it is not the reason first recorded here.** This
section said no graph query reaches it because code is not ingested. That is
false, and it was written by accepting a reviewer's reasoning without running
anything against it. **The code is ingested**: 185 files in `codebase_files`,
each carrying its `path`, alongside 4,749 symbols and 3,348 call edges, built
2026-09-14. Comparing `docs/crates/weaver-harness/weaver-trace/` against
`crates/weaver-trace/` is one query over two collections that were both sitting
there.

**So this was reachable and no dimension asked.** That is a worse answer than
the one it replaces and a more useful one: the six dimensions were chosen before
the graph was surveyed, and directory shape was not among them. A defect the
method could not reach is a limit. A defect the method could have reached and
did not is a gap in the dimensions, and the next audit adds one.

**Recorded at length because of where it happened.** A claim was taken on
authority and repeated without a check, in the document whose subject is claims
taken on authority and repeated without a check.

**G1 is a gate nobody ran.** Seventeen forbidden-word hits stand across `docs`
and `process`, and fifteen files carry semicolons. The audit queried the graph
and the census and never ran the mechanical checks, which is the same shape as
its own thesis one level out: **the census is a gate that fired and saw nothing,
and G1 is a gate that was not fired at all.** One of those seventeen was in this
document, at the table in section 3, and is corrected.

*A note on that finding as it was filed.* The review attributed the staleness to
`CLAUDE.md` calling these checks "all currently clean". That phrase appears
nowhere in `CLAUDE.md` or in any document in this corpus - what stands at
`CLAUDE.md:168` is a dated 2026-09-06 reading of `build` and `fmt`, framed by
its own next sentence as something a later reader re-runs rather than trusts.
The tree-wide counts are right and were re-derived here. The attribution is not.

**That the 48 and 33 baselines are wrong.** They are the checked-in backlog and
they describe real debt. This audit moves six of the 33 and ten of the 48. The
remainder is issue #558's and is not disputed.

**That the 333 ungrounded assertions are defects.** The format protects a
missing `grounds` edge as representation rather than an omission. `act-11` names
only the five where that criterion cannot explain the asymmetry.

**Whether `weaver-harness-Spec:2943`'s "Twenty-nine" is stale or disclosed.**
Two findings reached opposite verdicts. `act-13` must settle it and this did
not.

## 8. What not to conclude

**That high severity means blocking.** The severities rank how wrong a document
is about its own code. None of these acts is on the SPU -> trace -> harness
critical path.

**That twenty acts means twenty sessions.** Eight are one line or near it. The
weight is `act-08`, `act-13`, `act-17`'s Payload half and `act-18`.

**That a clean census means a clean corpus.** Eleven of these findings stood
while the census reproduced its baseline exactly.

**That the header backlog shrinking is progress by itself.** `act-08` could take
48 to 27 by authoring twenty one assertions nobody needs, which converts an
honest "unenforced" into "documented as enforced" - the failure the
perturbation rule exists to prevent. **The assertions come first because they
are owed, or they do not come.**

**That these counts are facts rather than readings.** They were taken at
`e2a5378` on one box. Re-derive before acting.

## 9. On the arithmetic

**Thirty five distinct findings, forty seven legs.** Three corpus-wide findings
were split into per-document legs by the grouping, which is the one-act-per-
document direction applied correctly and which makes the totals disagree if
read carelessly. The epic's own scope line says thirty six and is wrong.
