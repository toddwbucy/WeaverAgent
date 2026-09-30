# Handoff: building WeaverTools_v4, one crate at a time

**Version:** v0.1, 2026-09-12. Written from the thinkpad seat for the HADES
seat on olympus, against the overnight recovery handoff of the same date.

**Base:** `WeaverTools` at `eebecf1`, the merge of PR #563. The Handoff Format
section 2 wants the commit rather than the pull request - it is the one
mechanical fact in a batch that cannot be reconstructed from the documents -
and the first form of this line named a pull request and no commit.

**Re-measured 2026-09-13 against `eebecf1`**, after the olympus review of PR
#562 and after every archive directory left the tree. The figures below moved
because the corpus did. Each says whether it is measured or estimated.

This is a plan and not a runbook. Two decisions in it are settled and named as
such. One number in it is an estimate doing real work and is the first thing
to replace.

## 1. What is settled

**Rebuild rather than migrate.** `WeaverTools_v3` is a reference copy from a
2026-08-08 dump of the 2026-08-04 build. It holds 242 assertions against the
413 the corpus declares today, it carries the older `wt_*` schema, and it has
no code side at all. Beyond the count, **identifiers have been renamed** -
`web-a-column-frees-at-most-one-member` became
`web-an-arm-frees-at-most-one-member` on 2026-09-11 among others - so a
migration would reconcile adds, deletes and renames across 242 assertion rows
to reach 413, with renames the hardest of the three to tell apart. **Like for
like**: 242 to 413 for assertions, or 293 to 491 for nodes of every kind. The
first form paired 242 assertions against 491 nodes and overstated the
migration by the whole non-assertion population. A rebuild costs one
ingest per crate, which the plan does anyway.

**v3 is the template for the schema and not for the rows.** What transfers is
the notation: the seven keys, the edge representation, the idempotency rule.

**One GPU and one window.** GPU 1, the A6000, at a 32,768-token window. No
size-based router, no second device in the path. What does not fit is a named
list of files handled by hand, given in section 4.

**Batch of one.** A 32,768-token sequence peaked at 23,906 MiB at batch one on
the A6000, so `HADES_EMBEDDER_BATCH_SIZE=4` cannot survive the larger window -
four of those is roughly 95 GiB against the card's 48. The corpus is 227 files
and about 1.35 million tokens, this is a one-off build rather than a hot path,
and the operator's ruling of 2026-09-12 is that a slow ingest is acceptable.
**So there is no token-budget batcher and no throughput tuning in this plan.**

**`model_type` says GraphSAGE before the first train.** `hades graph-embed
train` reads it from the schema rather than a flag. The roadmap treats
inductive as a constraint because the graph regenerates per commit, and the
corpus ruled the same on 2026-08-03, that GraphSAGE is right *because* inductive, so
code nodes added at merge time need no retrain. **Crate-by-crate ingest is only
cheap under that choice** - transductive would make per-crate the worst
possible order, twelve full retrains, rather than the best.

## 2. What the corpus holds today

Measured at the base commit.

```
nodes 491     assertion 413 | vocabulary 39 | document 13 | crate 12
              term 5 | axiom 5 | artifact 3 | system 1

edges 665     asserts 414 | grounds 81 | draws 59 | defines 44 | party 23
              seam 12 | parent 12 | holds 8 | floor-link 7
              writes 2 | elects 2 | reads 1

keys 7        node kind tag edge from to via
```

Against the 2026-08-04 build: **293 nodes, 426 edges**, 7 node kinds, the same
seven keys, per `WeaverTools-Working-Process` line 581 and the repo
`CLAUDE.md`, which agree. The first form said 290 and 425 with no source, three
nodes and one edge under the governing document - on the baseline the
rebuild-rather-than-migrate argument rests on.

**The seven-key precondition holds five weeks on**, which is the notation
proving stable and is the strongest argument for templating v3's schema.

**`term` is an eighth node kind v3 never saw.** Five of them. A schema copied
from v3 has no home for them and will drop them silently or wedge them into
`vocabulary`, where they do not belong.

**`tag` appears 426 times against 413 assertions**, so thirteen non-assertion
nodes carry tags. The census validates tags on assertions only, so
`tag: ratified` on a `kind: system` node has never been checked against the
format's five. **Whether the v4 schema admits a tag on a non-assertion is a
decision and not a detail.**

**`reads` fell from two edges to one** between the builds. Small, and an edge
kind losing its only sibling is worth knowing the reason for before a rebuild
absorbs it.

## 3. Chunking

**Structural, with a two-level fallback.** Measured against the three largest
Specs and the three largest sources:

```
docs   ## sections            largest  12,187 tok
rust   top-level items        largest  30,125 tok   (a mod tests)
rust   methods inside that    largest   3,408 tok
```

**No file in this corpus requires an arbitrary split.** Cut at top-level items
for Rust and at `##` for documents. Where one piece still exceeds the window,
cut at its methods. The worst file in the tree is a test module with
twenty-five clean method boundaries inside it.

**The 32k window is for context and the structural boundaries are the cut
points.** These are two different jobs and the plan uses both. A window that
covers most of a Spec in one forward pass means each chunk's vector is
informed by the whole argument - section 9's enforcement table refers to
assertions declared in section 2, and independently embedded sections never
see each other. Late chunking buys that, and structural boundaries buy chunks
that end where a thought ends.

**The overlap seam is placed at a structural boundary**, for the files in
section 4 that exceed one window. The overlapping region is embedded twice
under two different contexts and produces two vectors for the same tokens.
Putting the seam on a section or item boundary means no chunk straddles it and
the question of whose vector wins does not arise.

**A graph fence is atomic.** A ```graph block carries `node`/`kind`/`tag` and
`edge`/`from`/`to` stanzas, and a cut inside one yields a chunk holding half a
declaration. No chunk boundary falls inside a fence.

## 4. The files done by hand

**Ten, measured at `eebecf1`.** The test is `TARGET` and not `CEILING`, on
the olympus review of 2026-09-13: testing at the ceiling applied the headroom
to the pieces of a file already held and never to the decision about which
files to hold, so a file sitting inside the ratio's own error was left to the
embedder. That is the file the headroom exists for.

```
crates/weaver-harness/src/lifecycle.rs             ~67k
crates/weaver-harness/src/engine.rs                ~52k
docs/crates/weaver-spu/weaver-spu-Spec.md          ~47k
docs/crates/weaver-harness/weaver-harness-Spec.md  ~43k
crates/weaver-spu/src/family/mod.rs                ~38k
docs/crates/weaver-web/weaver-web-Spec.md          ~37k
docs/crates/weaver-admin/weaver-admin-Spec.md      ~34k
docs/crates/weaver-types/weaver-types-Spec.md      ~33k
crates/weaver-spu/src/main.rs                      ~32k   under the ceiling
crates/weaver-admin/src/inventory.rs               ~28k   under the ceiling
```

**The last two are the finding.** `main.rs` measures about 31,844 estimated
tokens, 2.8 percent under the ceiling, which section 5 calls inside the error
of any ratio - and it was not held out while two files at the same distance
were. If jina returns three characters per token or fewer for Rust it was
truncated by the embedder rather than cut by the plan.

**Four of the ten are Specs whose sections already cut at 12k or less**, so by
hand means one cut at a section boundary rather than a judgement.
`lifecycle.rs` and `engine.rs` are the two needing thought, and
`lifecycle.rs`'s bulk is the 30k `mod tests`.

## 4a. The held-out cut plan, as offsets

`process/ingest/chunk_plan.py` emits `process/ingest/chunk-plan.json`: for each
held-out file, its byte length, its SHA-256, and the pieces as byte offsets.
Ten files become twenty two pieces.

**Hashed against a ref and never the working tree.** The ingest runs on what is
merged, so a manifest hashed from a branch carries offsets for content that may
never reach main. Measured on 2026-09-13 while checking the graph against the
tree: the working tree held one document main did not, and it read as a file
missing from the graph until the ref was named. Against main the document half
reconciles exactly, 101 tracked and 101 in the graph.

**Offsets rather than files on disk**, so a Spec edit does not leave cut pieces
behind it. **The hash is what makes that safe** - offsets against changed
content are worse than no offsets, because they are wrong rather than absent.
An ingester applying this manifest refuses where the hash does not match, and
`--check` reports whether the held-out set moved or only its content did.

**Each boundary is a semantic seam and the overlap is one whole unit,
repeated.** A sliding window repeats two half-thoughts where a boundary-aligned
one repeats a single whole thought, so both neighbours hold the argument entire
and nothing is duplicated that does not buy continuity. In
`weaver-types-Spec.md` piece one ends at byte 67,297 and piece two begins at
61,783: **the overlap is section 3, entire, in both** - the heading at 61,783
is `## 3. Peer identity and the authorization predicate` and the one at 67,297
is `## 4.`. The first form of this named section 5, which begins at 119,473,
fifty two thousand characters past the end of piece one. The mechanism was
right and the worked example was wrong, which is the worse way round because
the example is what a reader checks.

**The overlap is dropped where repeating it would not fit.** Continuity is
worth a repeated unit and is not worth a piece the embedder truncates, and the
first form appended the next unit after the overlap with no budget test - so a
piece was bounded by two units rather than by the target, and three adjacent
27,900-token sections gave pieces of 55,800 against a 32,768 ceiling. Nothing
in this corpus reached it and nothing checked. **The generator now refuses to
write a manifest holding a piece over `ceiling_tokens`**, which is the one
property the file exists to guarantee.

**Where one unit is itself too large the cut recurses into it.**
`lifecycle.rs` holds a `mod tests` of about thirty thousand tokens. Cutting
around it left a piece over the target; cutting into it at method boundaries
gives 6,443 / 27,713 / 27,722 / 6,781, **every piece under the target and each
test function whole.** That is the same rule one level down rather than a
second rule.

**It does not give four even pieces**, which the first form of this claimed.
Two of the four are runts and the recursion did not remove them - what it
removed is a piece the packer would otherwise have had to over-fill. Stating
the benefit it actually buys is the point, since the other claim is checkable
against the manifest committed beside this file and fails.

**A graph fence is never cut.** A ```graph block carries node and edge stanzas
and a piece holding half a declaration is worse than a piece that is slightly
too large.

**These offsets are reviewable and not load-bearing.** Section 6a re-cuts any
file whose hash differs from current content, so the ingester runs the cutter
anyway and the committed manifest is a reading a person can check rather than
an input the ingest depends on. That is the point of committing it - a run is
not reviewable and a table of offsets is - but the two mechanisms read like one
job until it is said.

## 5. The one estimate doing real work

**Every token count above is 3 to 5 characters per token and nothing here has
seen the jina tokenizer.** That assumption is what decides how many files are
cut by hand rather than by the embedder.

**Measured across the band at `eebecf1`**, files exceeding the 32,768 ceiling:

```
3 characters per token   9 files
4 characters per token   7 files
5 characters per token   3 files
```

**Nine is the maximum the band reaches.** The first form of this said the
assumption was worth "between eight files needing the larger window and
thirteen", and thirteen is not reachable from three to five characters per
token - it came from somewhere this document does not say. Ten are held out
because the hold-out test is the target and not the ceiling, which is the
headroom doing its job.

**Run the tokenizer over the ten held-out files, before anything else.** 245 of
255 tracked units fit whatever the tokenizer says, so this is ten files rather
than a corpus. Two of them - `weaver-admin-Spec` at about 33,644 tokens and
`weaver-types-Spec` at about 33,315 - sit within three percent of the ceiling,
which is inside the error of any ratio, and `main.rs` at 31,844 sits the same
distance on the other side of it. The overnight session's own lesson is the
argument: a token ceiling repeated from a comment that described a refusal
rather than a measurement, caught by running something.

## 6. First ingest: weaver-trace, and why

Every number is known in advance, so the ingest is a test rather than a leap.

```
9 code nodes      61 cites edges      0 files without a header
```

**If the graph reproduces those three, the extractor port, the SCIP reader and
the edge-attachment decision are verified at once**, on a crate small enough to
redo. If it does not, which of the three is wrong is knowable before anything
else is in.

`weaver-trace` is also the crate whose Spec drifted for three weeks - saying
eighteen kinds where the code compiled twenty-one - so the first real query has
a known wrong answer waiting for it.

**weaver-web goes last.** Twenty-seven files, twenty-one with no header,
sixteen citations: the least conformant crate in the tree and the one still
being rewritten. Ingesting it early means ingesting it twice.

## 6a. The update rule

**Any file whose content hash differs is deleted from the graph and re-cut from
current content.** Not patched. Ruled by the operator, 2026-09-13.

Partial update leaves phantoms: chunks whose text no longer sits at those
offsets, which is worse than stale because nothing in the graph says which
chunks belong to the current file. **Delete and reingest is the only version
where the graph's contents are a function of the tree's contents.**

**The hash is the update key and not only a staleness detector.** Matching,
skip the file. Differing or absent, drop its chunks and re-cut. The ingest is
then idempotent, so running it twice over an unchanged tree does nothing and
**running it again is a complete recovery procedure** rather than the first
step in one.

**A crash between the delete and the write needs to be loud rather than
ordered around.** Ordering buys something only where the failure is quiet, and
where it is quiet no ordering saves you. What has to hold is per-file outcome
reporting: 255 files where one fails mid-write and the job reports success is
the shape that hides, and it is a reporting property rather than a sequencing
one. A file left with no chunks is picked up by the next run on the hash alone.

**A reingest does not trigger a retrain**, the schema being inductive, so
`graph-embed update` is a forward pass over the new chunk nodes. Transductive
would have made every Spec edit a full retrain, which is the constraint the
roadmap and the corpus settled independently before anyone reached this.

## 7. The reconciliation owed before the first ingest

**Every denominator here is decomposed**, on the olympus review of
2026-09-13. The first form of this section mixed archive-inclusive and
archive-exclusive counts across three paragraphs and reached "the document
half reconciles exactly" through the mixing. At `eebecf1` no archive stands in
the tree, so the ambiguity is gone rather than resolved - what follows is the
decomposition anyway, because the gaps predate the deletion.

```
.rs   under crates/          160        .md   docs/          67
.toml under crates/           15              experiments/    9
.cu   under crates/            1              crates/         4
.py   under crates/            3              process/        4
                                              root            2
                             ---                            ---
                             179                             86
```

**The SCIP reader reports 156 documents and the tree holds 160 `.rs` under the
twelve members.** Four files.

Probably `build.rs` and units outside the compile graph, but a silent four-file
gap at twelve crates is a silent forty-file gap at a corpus, and it is one
`comm -13` to settle. **The extractors reproduce three of four census metrics
exactly - `dangling_citations` 0, `sources_without_a_header` 48,
`uncited_perturbations` 33 - and disagree on the fourth**: 478 `cites` edges
against the census's 481 distinct file-and-assertion pairs, out of 516 raw
occurrences. Three edges. Candidates are the `archive/` exclusion, whether
`tests/` counts as citing, and the three `tag: manifest` assertions cited only
from a `Cargo.toml`.

**A third gap, found on 2026-09-13 by counting and then closed the same day.**
The graph held 185 code files against 181 counted `.rs` and `.toml`. **The four
are `crates/weaver-spu/kernels/transformer.cu` and the three
`crates/weaver-harness/src/bin/pyworker/dev_python/*_loop.py`** - extensions
the router sends to the code half that the count did not include. 179 plus the
six root and workspace `.toml` reaches the 185. Recorded closed rather than
deleted, because the next seat would otherwise re-derive it.

**The document half did not reconcile exactly and was reported as though it
had.** 101 in the graph against 101 tracked `.md` was archive-inclusive on both
sides. Fifteen of those sat under `docs/archive/` and four under `process/`,
which the container rules put outside the document set, so the like-for-like
figure was 82 against 101. **At `eebecf1` the tree holds 86 `.md` and the graph
still holds its 101**, which the update rule of section 6a resolves on the next
ingest by hash - and is the concrete case for running it before anything is
asked of the graph.

**And what it says about observability matters more than the four files.**
`codebase validate` passes seventeen invariants with zero dangling edges, so
the graph is internally consistent and disagrees with the tree in both
directions at once. **An inconsistency inside the graph is checked and a
divergence from the tree is not**, which means a file silently absent after a
failed write would pass validate. The cheap assertion is a count of the
tracked set against `db_count` on each half, and it costs one query.

**Both gaps are cheap now and expensive later.** After an ingest they stop
being a difference between two scripts and become a difference between the
graph and the tree, and telling which is wrong gets much harder once the graph
is the thing people ask.

**One question about the extractors that changes what their agreement is
worth:** were they written independently, or calibrated against the census
baseline? If the baseline was the target, matching it confirms the port and not
the number. If they were blind, 33 is the first figure in this corpus two
implementations have reached separately, which is a different and much stronger
thing. The census shipped with twenty-five bugs of its own, so its numbers were
never better than one implementation.

## 8. What ingesting today would write in as fact

The census at the base commit:

```
dangling_citations                      0
untagged_assertions                     0
unknown_tags                            0
duplicate_node_ids                      0
malformed_node_ids                      1
malformed_citations                     0
enforcement_table_mismatch              0
documents_without_an_enforcement_table 12
uncited_perturbations                  33
sources_without_a_header               48
```

**Thirty-three assertions carry `tag: perturbation` and no source file cites
them.** The tag claims an instrument exists. Ingested unflagged, the graph
asserts as fact the thing issue #558 exists to say is not yet true, and a
coverage query over v4 reads thirty-three enforced claims that are not.

**Twelve documents carry no enforcement table**, so for most of the corpus the
rule "cited in the same act, or its enforcement-table row marked owed" has no
table to mark. **One malformed node identifier** and **forty-eight units with
no header** are the remainder.

None of these blocks a build. All of them decide what a build *means*, and the
quarry is the case against ingesting them quietly: `weavertools_v2` froze with
a bilateral-contract certificate at zero edges and an axiom basis covering
seven of seventy-one claims. **The structure was right and the edges were never
drawn.** A graph that encodes today's drift as fact is the same failure with
the edges drawn wrong instead of not at all.

## 9. What this plan does not reach

**The paper corpus.** 300,510 papers, 713,889 abstract-level embeddings, four
docling-converted documents, zero citation edges. Its own numbers are the
argument for treating it as a separate project with its own justification, and
they are the quarry's failure shape stated in a second corpus.

**The memory layer.** `store_observation` fails and `retrieve_memories` has a
signature mismatch, against two client methods that do not exist. API drift
between two files rather than anything deep, and out of scope here.

**The rebrand.** Scheduled separately.
