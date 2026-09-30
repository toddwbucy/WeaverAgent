# Audit: the corpus against its code, second pass, read through v5

**Status:** AUDIT, 2026-09-14. A dated reading rather than a member of the
document set, and **nothing here is decided until the operator rules on it.**
It is the second pass, run against `WeaverTools_v5` after the corpus scope
narrowed, and it reports what the first pass missed rather than restating what
it found. The first pass is at
`docs/project/audit-2026-09-13-the-corpus-against-its-code.md` and its epic is
issue #569, which this audit amends rather than replaces.

**Base:** `cc7c34e` on main, and `WeaverTools_v5` built from it. Every figure
below was re-derived by a verifier before it was written down, and where a
recheck corrected the filed number the correction is what stands.

**Method:** thirteen agents. Six audited one dimension each, six tried to refute
what the first six found, one placed the survivors against the standing epic.
Each agent surveyed the graph before querying it and was told to trust no count
in its prompt or in any document. 569 tool calls, about twenty five minutes.

## 1. What changed between the passes, and why it matters

**The corpus scope narrowed, and then narrowed less.** The four process
documents and `docs/project/` left the graph on the operator's ruling of
2026-09-14, and v5 holds 72 documents where v4 held 83. **The three gate scripts
under `process/` came back in as code at `f8c8618`**, after this audit ran, so
v5 now holds 185 `codebase_files` where the audit read 182. `census.py`,
`test_census.py` and `chunk_plan.py` are instruments rather than documents and
`.hadesignore` says so.

Every one of the corpus's 491 nodes and 665 edges is declared
under
`docs/crates/`, so the conformance layer is identical - the eleven documents
that left declared nothing. The first pass did not know that, and some of its
sprawl came from treating a rulebook as subject matter.

**The graph is exact now.** Two document counts appear in this section and they
count different things: the `documents` collection is what was ingested, 72 of
them, and `wt_documents` is the declared `kind: document` node set, which is the
thirteen contracts. `wt_documents` holds thirteen where v4 held eighty one,
the two seam edges the first pass counted missing were three real
seams collapsed on a key that ignored `via`, and both are fixed. The extractor
takes its scope from the ingest rather than deciding its own.

**The dimensions rotated rather than grew.** The set stayed at six. Two carried
over unchanged, `claim-vs-code-drift` and `enforcement-tables`. Three were
renamed and widened - `citation-reach` succeeds `headerless-sources`,
`instruments-and-tags` merges `uncited-perturbations` with `tag-vs-instrument`,
`edge-integrity` succeeds `structural-orphans`. **`mirror-and-shape` is the one
with no predecessor**, and directory shape being absent from the first six is
why the first pass missed a real defect. A later pass should not assume
coverage is monotonic: a rotation can drop a question.

## 2. What this cost and what it bought

**Seventy seven findings were filed and forty five survived.** Six confirmed
pairs are one defect seen from two dimensions, so the count of distinct things
wrong is thirty nine.

**The thirty two rejections are the healthiest number in the run.** The
verifiers threw out re-reports of first-pass findings, a finding whose central
claim was false, and two that concerned directories now outside the graph. A
verifier that confirms everything is decoration. These did not.

**Five findings corrected their own numbers on recheck**, and the corrections
are kept beside the originals: 32 manifest assertions rather than 33, 25
weaver-web perturbations rather than 22, and three whose corrections this
document had to correct in turn.

**The contracts page carries eleven `###` headings and ten contract entries**,
one heading being `weaver-organ-channel`, which the page marks as not a
contract. Against thirteen contracts in the corpus, three are missing. The
recheck moved the unit from contracts to headings without saying so, and
thirteen against eleven computes two.

**`weaver-analysis` holds twelve `.rs` under `src/` and six under `tests/`.**
The filed twelve was exact for source - the recheck's eighteen counted the test
suite as source without saying so.

**The 3,324 figure for unnamed weaver-web lines is not reproducible** and is
withdrawn. The crate's `src/` totals 7,072 lines and no stated file set yields
3,324. The finding it supports - that section 1's layout names directories the
crate does not have - stands on the names, which are checkable. **A precise
figure nobody can re-derive is the defect class this audit exists to find**, and
it reached the audit's own summary.

## 3. Where the findings fell

| dimension | found | what it asks that the first pass did not |
|---|---|---|
| claim-vs-code-drift | 15 | counts the first pass missed, and whether its eight hold |
| citation-reach | 8 | the non-perturbation assertions no code reaches |
| edge-integrity | 8 | multiplicity, and whether a seam's contract names both parties |
| mirror-and-shape | 7 | **new** - the two trees against each other, by query |
| instruments-and-tags | 5 | `manifest`, which the Format names and no device backs |
| enforcement-tables | 2 | the count words, re-derived |

Fifteen high, twenty four medium, six low. By act size: eighteen one document,
fourteen one line, eight one crate, five corpus-wide.

## 4. The findings worth reading first

### The new dimension found a paper that contradicts its own rule

`docs/technical/contracts.md` calls itself "the one place a contract is read
out here" and counts the internal seams as seven, six sockets and one link. The
corpus holds thirteen contracts and the graph holds twelve seam edges. Three
merged contracts appear nowhere on the page. **The page's own rule makes it
the defect**: where a paper and its source disagree, the paper is the defect.
Its last edit predates two of the charters it omits.

Beside it: `weaver-analysis` is a chartered crate with a PRD, a Spec, **twenty nine
assertions and twelve source files** under `src/` with six more under `tests/`,
and the published technical set never names it. The roster discloses its other
gaps in prose. This one it does not.

### A manifest tag its own manifest falsifies

`spu-one-binary` is tagged `manifest` and the Spec asserts "One binary". The
crate's `Cargo.toml` declares two `[[bin]]` targets. **A manifest-tagged
assertion is one the Format says a manifest holds**, and this one the manifest
refutes. The same act carries a candle fork pin the Spec calls its "durable
record" naming a revision the manifest does not pin.

### The drift the first pass would have found with one more file

`weaver-trace/src/event.rs`'s own doc comment says fourteen event kinds and
warns about a fifteenth. Then it states the kind-to-payload mapping as nineteen
kinds. The enum has neither. This is the first pass's `failure.rs` finding one
file over - the same crate, the same shape, a header arguing with the code
beneath it.

### Nine assertions no reading can see

**Nine non-perturbation assertions have no inbound `cites` edge.** The census
reports `uncited_perturbations` and nothing else, so an uncited `compile-pin`
or `review` claim is invisible to every reading. **Three of the nine already
have their instrument in the tree** - the citation is missing, not the
instrument - which is the same shape the first pass found in weaver-spu.

### The layout that describes a crate not on disk

`weaver-web-Spec` section 1 names seven directories. Four do not exist, one is
a file, and a large part of the crate sits under names the layout never gives.
**The quantity is deliberately absent** - section 2 withdraws the figure this
sentence used to carry, and repeating it here would be the same unreproducible
number in a second place.

**This is why the epic's largest act cannot start**: most of the twenty
one headerless weaver-web files have nothing honest to cite until the layout
admits the modules that exist. `act-18` now hard-blocks `act-08`, and the first
pass's own warning about authoring assertions nobody needs has a named cause.

### Two seams whose contract names one party

Two `wt_seam_edges` rows are governed by a contract that names only one of the
two crates it binds, and neither contract mentions the other party at all. A
seam via a contract that does not know both its ends is a contract with a
missing party. **The rule this offends is the Document Format's, that a contract
is named for its parties**, and not the Working Process's transport-silence
clause, which concerns a contract naming its substrate and was cited here in
error.

### Thirteen `review` tags standing over real tests

The first pass named this violation once, among its census-invisible eleven.
This pass quantifies it at thirteen sightings, eleven unfiled, which is a
re-report admitted deliberately: **a rule the corpus states once and breaks
thirteen times is a ruling rather than a sighting**, and `act-34` is written as
one. The re-reports that were rejected added no count and no ruling.

Thirteen assertions tagged `review` are cited from test files, eleven unfiled,
six in files whose own header calls them the perturbation-verified suite. The
corpus rule is that `review` means an instrument was not bought, never that
none exists. **This is a ruling and not a fix**, and it is `act-34`.

### One that lands on this seat

`.hadesignore` said the process documents were "not in this repository at all
from 2026-09-14" while nine were tracked at the commit the sentence dated. The
exclusion was right. The justification was false, and it framed a live rule as
a legacy safety net a later act would delete. Written by this seat before the
decision changed. **`act-25` at PR #575 carries the correction and has not
merged**, so the false sentence stands in the tree until it does.

## 5. How this amends the epic

**Twenty six findings fold into fifteen acts #569 already carries, and
thirteen open the fourteen new acts `act-21` through `act-34`** - one finding
splits across two acts, which is why the act count exceeds the finding count.
Twenty six and thirteen is thirty nine, the distinct total. The epic body
carries every fold, every new act with its findings spelled out, and the
ordering.

**Two constraints it imposes on the standing acts:**

- `act-18` hard-blocks `act-08`, for the reason section 4 gives.
- `act-34` joins tier 3 beside `act-11`. Retagging thirteen assertions moves the
  instrument tallies five drift acts write into prose, and landing it after
  them means recounting twice - which is exactly why `act-11` is tier 3.

**Two acts grew past their epic size.** `act-12` is now a crate act touching
`Cargo.toml`, `main.rs` and `tests/manifest.rs`. `act-13` turns a four-field
code block into a re-argued paragraph.

## 6. What needs the operator before an act can start

**`weaver-analysis`'s absence from the published set.** The audit established
that the crate is missing from the roster and the contracts page and that the
roster discloses its other gaps. It did not ask why. The OPSEC rule says check
visibility rather than assume it, and this may be the publish boundary working.

**`agent-declaration.md`.** Referenced by nothing in the repository. Delete and
link are equally consistent with the evidence.

**The `review` tag ruling.** Thirteen citations, and the rule they break is the
corpus's own. Retag, or state why a test named for a claim does not buy it.

## 7. What this audit did not establish

**That the code is right and the documents wrong.** Every drift finding
establishes only that two authorities disagree. In four cases the code is
plainly ahead, because a second file in the same crate already states the fact
correctly. In others - a fourth `SeamPosition`, an unused dependency - the
document may be the authority and the code the drift. Each act names the fork.

**That anything compiles, passes, or fails on perturbation.** Nothing was
built or run. Four weaver-web perturbation citations sit in tests that return
success when `DATABASE_URL` is unset, and what is established is that they
assert nothing then, not that they would fail with it set.

**That the counts are current past `cc7c34e`.** Every figure is one reading on
one tree state on one box.

**That a finding that narrowed is a finding that shrank.** Two were filed with
two legs and refuted on one. The surviving leg is real and the refuted one is
recorded.

**That silence is absence.** Findings concentrate where the queries pointed.
The first pass said this and it held: the new dimension found seven things in
ground the old six never touched.

## 8. What not to conclude

**That the first pass was wrong, or that it was re-tested.** Eight of its
thirty five findings were put to this pass: seven were re-derived and stand and
one open question is settled. **The other twenty seven were not examined**, so
nothing here confirms or refutes them - and reading their absence as
confirmation is the move section 7 forbids two pages up. This pass found what
the first missed, which is what a second pass is for and is not the same as
auditing the first.

**That thirty nine more defects means the corpus is worse than it read.** The
corpus is the same. The graph is exact now, the dimensions rotated so one
question was asked that had never been asked, and the verifiers rejected what
they should. The number went up because the instrument
got better.

**That the epic's header projection improved.** It did not. Section 4's
weaver-web finding makes it worse: the assertions come first because they are owed, or
they do not come, and now there is a named reason most of them are not owed
yet.
