# WeaverTools Working Process

**Version:** v0.36, 2026-09-28. Companion to the Working Rules, the Document
Format, and the Handoff Format. The apex says what we are building. The Working
Rules say how we write. The Document Format says what shape a document takes. The
Handoff Format says what shape a batch takes when it moves between seats. This says
who is primary, in what order the work moves, and what must be true before it
moves.

This document is the boot prompt for every fresh session on this project, in either
seat. Read it first. Section 7 says where the work currently sits on the map.

Phase three opened 2026-08-04, and gates H1 through H5 are in force per section 6.
H6 retired on the operator's ruling of 2026-09-27, which defers the knowledge graph
and its upkeep to release. Code merges against the gates and against nothing
invented at review time.

## 1. Standing rules

These hold in every phase and are not restated inside the protocols.

Whoever holds the authoritative artifact is primary. The artifact that is
authoritative changes as the work moves, and the primary seat moves with it. The other
seat advises in and does not edit in place.

Two seats carry the work, and their assignment changed on the human's ruling of
2026-08-01. The authoring seat is the session that holds the working tree: it
drafts and lands PRDs, contracts, specs, and their edits, because grounding in
the files and the corpus's cross-document state proved decisive through phase
one. The review seat is the remote session: it consults, reviews uploaded
snapshots at a distance, and returns findings in the standard shape below, which
is the defect-finding a fresh context does best. The earlier assignment, the
architecture seat authoring and the implementation seat landing, is history the
changelogs of this date record. The codebase and the graph stay with the seat
that holds the tree. A session in either seat states which one it is in before
it begins.

Neither seat settles a disagreement with the other. Both surface it to the human, who
is the adjudicator by definition and who sits in both seats.

Advice moves in one shape, in both directions. A return marks its content by type:
facts found, advice offered, and requests to reopen a closed decision. A return that
arrives with edits already made has skipped the gate rather than passed it.

A gate is a condition, not an intention. Every gate below is written so that it can be
checked by looking, and a gate that cannot be checked by looking is not a gate.

**Git is the archive, and no archive directory stands in the tree.** The operator's
ruling of 2026-09-13. A retired document or module is deleted, and whatever referenced
it cites the content against a ref reachable from `main`. Branches and worktrees are
how work in flight is held.

**The command differs by what is cited, and reading is not the same act as
materialising.** `git show <ref>:<path>` prints a file to stdout. On a directory it
prints the tree's entry names and retrieves nothing, so a citation that reads as a
recovery instruction recovers nothing - which PR #567 shipped twice before CodeRabbit
caught it.

    read a file           git show <ref>:<path>
    save a file           git show <ref>:<path> > /tmp/<name>
    save a tree           git archive -o /tmp/<name>.tar <ref> <path>

**The tree form writes an archive and never extracts.** Two earlier forms of this line
did, and each failed differently: `| tar -x` extracts relative to the current
directory, so run from the repository root it recreates the directory the ruling
deleted, and a reader who then commits has undone the rule by following the citation.
`| tar -x -C /tmp/<dir>` fixed that and then exited 2 whenever the destination did not
already exist. **`git archive -o` is one command and needs no destination directory**,
and what it does not do is unpack: nothing appears at `<path>` in the working tree, so
the failure the first form had is structurally gone. The reader unpacks the archive
where they choose, which is their decision rather than this rule's.

**The `/tmp` in that line is doing work and is not decoration.** `-o` writes wherever
it is pointed, so `git archive -o ./x.tar` lands a tarball in the repository. An
earlier form of this paragraph said the option could not write into the working tree
at all, which is false and was corrected by CodeRabbit on PR #567 - **the option
removes the unpacking hazard and the path removes the writing one.**

**A citation is for reading and recovery is the exception.** Most of the time the
first line is the whole answer: the content is wanted on screen, not back in the tree.

A copy inside the tree duplicates a job git already does, so every encounter with it
becomes a question about which copy is authoritative - and that question has no cheap
answer. The `weaver-web` archive held nine files at its deletion, three drifted from
their originals and six byte-identical, and telling them apart cost a hash of every
file. A commit cannot drift. **The rule is every archive**, `docs/archive/` included:
a record of a decision is not a second copy of the thing decided, and the carve-out
made on that ground was overturned the day it was made.

**The rule has two halves and neither is a substitute for the other.** Review holds
the tree at zero, an archive directory in a diff being a finding, since the census
that read the tracked set for it retired on 2026-09-27. `.hadesignore` excludes the
pattern from the ingest, for the window between a directory appearing and someone
acting on the finding, and it is dormant with the graph until release. A rule naming
today's instance is the same mistake as deleting today's instance, so both halves
match a family of spellings rather than one.

## 2. Document states

Three states, and the transition between each pair is an event with a gate.

**STUB.** A named slot with no content. It exists so a crate or a seam has a home and
a filename before it has a charter, and it is written when the thing it names is
chartered. A stub settles nothing, declares no graph records, and may not be cited by
any document as having decided anything. It is replaced wholesale rather than refined,
which is what separates it from a draft.

**Replaced wholesale means the stub leaves the tree.** The draft is written fresh at
the stub's name and the stub is deleted in the same act, a tracked deletion, so the
commit that cuts the draft is the commit that holds the stub's last state. Ruled
2026-07-31, reversing the preservation rule this section carried through v0.5, which
kept the file beside the draft under a `.stub` suffix. History is the archive and the
tree is not: a note that did not survive the drafting is findable at the commit that
retired it, and a tree that carries consumed stubs accumulates files no gate reads, no
mapper counts, and no pass owns. A stub accumulates what earlier passes learned about
the thing it names, because the work is chartered workflow by workflow and a workflow
runs through crates that have no charter yet, so the drafting reads the stub alongside
the old tree's code. Not every note survives that drafting and none of them binds,
since a stub decides nothing.

**Where a stub and the old tree conflict, the stub wins.** The stub is this program's
more recent statement of intent about that crate and the tree predates the split, so a
conflict is a question the project has already answered once. The precedence fires only
on conflict. Where a stub is silent and the tree speaks, nothing opposes the tree and it
flows into the draft unopposed, which is what step 3 review is for. Precedence handles
collision and review handles absorption, and neither substitutes for the other.

It is a state and not a member of the document set, so ratification does not wait on
it and the mapping does not read it. A stub that acquires a decision has stopped being
a stub and is a draft.

**DRAFT.** Cut from the old tree or newly begun. Carries no decisions and is not built
against.

**MERGED.** In `main` and declared the source of truth for now. Merged means the file
is in a working state that allows the work to move forward, and it means nothing more
than that. It is not a verdict on the contents. Individual files and individual lines
change after merge, and a correction to a merged document is an edit rather than a
ceremony.

**A header states the state its act lands it in**, per the operator's ruling of
2026-08-04: a branch cut to merge carries MERGED, made true at the merge, rather
than flipping in a follow-up act, and a header never outlives its state. The sweep
of this date flipped the merged set's DRAFT headers, which had been wrong in the
direction this rule closes, and the earlier finding against a branch pre-asserting
MERGED resolves the other way: pre-asserting the post-merge state is the practice,
and the defect would be merging without the human's call, not the header. The
boundary of the flip is doctrinal rather than enumerated: every member header read
MERGED alone at that sweep, ratification being represented only at the set level
under the rule of the time, and the same sweep retired the source-of-truth "for now"
from every member header, ratification having ended the provisionality the phrase
carried. **The set-level-only half of that sentence is retired**, per the ruling of
2026-08-23 recorded below.

**RATIFIED.** The document conforms to the pattern the set-wide act of 2026-08-04
established, and clearing its gates is how it shows that. After ratification a
document does not change, and a change found necessary during implementation is not a
patch. Coding stops and the work re-enters authoring.

**Ratification is suspended until release**, on the operator's ruling of 2026-09-27
recorded in section 6. No document is ratified meanwhile and none is un-ratified: a
header reading RATIFIED stands as the record of its act and is not maintained. A
merged document that disagrees with the code changes by edit, in the same act as the
code, under H1's write-time rule, and coding does not stop for a re-entry. The
paragraphs below record how ratification ran and are the rule it resumes under.

**A charter ratifies on its own**, per the operator's ruling of 2026-08-23. A crate
that has been chartered and has cleared its gates is ratified, and **the set is
whatever the charters currently say** rather than a snapshot of one date. A member
header may read RATIFIED.

**The set-wide act of 2026-08-04 was a requirement of its moment rather than a
standing obligation.** Nothing existed then to be consistent with, so consistency had
to be established across the whole corpus at once. That act built the skeleton every
later charter is built on. It is not a ceremony to repeat whenever a charter lands.

**The rule this replaces was scaffolding, and it is recorded as such rather than
quietly dropped.** It read that ratification is a property of the set and never of
one file, that no document ratifies alone, and that a member header tops out at
MERGED. That was correct while the pattern was being established, because a document
ratifying alone before a pattern existed would have been ratifying against nothing.
The pattern is established and the gates carry what the ceremony carried, so the rule
is retired rather than softened.

**Which gates a document clears alone, and which it cannot.** G1, G2, and G3 are
document-scoped and a charter clears them by itself. G4 has two halves: the draw
side, that every name a vocabulary clause draws resolves to a definition that exists,
is checkable per document and is cleared here. **The definition side, that every
definition is named by some clause or stated to be internal, cannot be checked from
one document, and neither can G5 or G6.** To know a definition is unused, or a fact
stated twice on purpose, you must read the set, and nothing the document declares
bounds that search. Those two run at the release inventory, and until they do a
per-charter ratification is a claim about the document rather than about the set's
coherence.

**G7 is not in that class and is grouped apart.** It spans more than one document, but
**a ruling names the documents it changes, so the list to check is declared and
finite.** That is a volume problem rather than a scope one, and it runs with the
ruling rather than at any close. **The graph's set-level mark is unchanged by this and
continues to record
the founding act**, the question of whether a ratified charter carries its own tag
being the Document Format's rather than this document's.

**The project documents sit outside this model.** The Working Process, the Working
Rules, the Document Format, and the Handoff Format govern the set rather than belonging
to it, they do not map into the graph, and ratification is defined as the mapping. They
carry a version and a date and no state. A state on a document that cannot reach the
terminal state is a label that never resolves.

**A project document's `Parent:` header carries no edge.** The Document Format defines
that header against a `parent` edge and rules that the edge governs where the two
disagree. These documents do not map, so there is no edge to govern and nothing for the
header to disagree with. It is kept as a reader's convenience, naming which project
document a reader should have in hand first, and every project document parents to the
Working Process because this one is the boot prompt.

The word freeze is not used. It was doing the work of both merged and ratified, which
forced corrections to a merged document to queue behind a ceremony that does not apply
to them.

## 3. The map

Three phases in order. Each has a protocol and a closing gate. Nothing begins until
the phase before it has closed.

    Phase one   Authoring     PRDs, contracts, specs      architecture seat primary
    Phase two   Graph         knowledge graph built       implementation seat primary
    Phase three Coding        crates written and merged   implementation seat primary

## 4. Phase one, authoring

Produces three document types at three levels. The PRD says what a crate needs and
why. The contract says what crosses one seam, what it means, and how it fails. The
spec says how it is represented. No fourth kind, and no leak between levels.

Every document carries its nodes and edges in the notation the Document Format
defines. A document that states an edge only in prose has left work for the mapping,
and the mapping is the terminal gate.

The old tree is live during this phase and only during this phase. It is raw material
and never evidence. It supplies a starting draft and answers questions of fact about
what was built. It does not ratify anything, because this project was split off to
escape that context. A finding in the old tree is a candidate that returns for
ratification.

### Protocol

**Step 1, draft.** The implementation seat cuts a rough draft from the old code,
because that is where the raw material lives. A draft is marked as drawn from the old
tree and carries no decisions.

**Step 2, author.** The architecture seat writes the document. This is the step where
staleness is caught, because everything inherited from the draft is re-derived here or
dropped.

**Step 3, advise.** The document returns to the implementation seat for review against
the code it can see, in the return shape named in section 1.

**Step 4, merge.** The human rules. The document merges on his call and the
implementation seat applies the merge without reopening it. Merge is not ratification
and confers none of its finality.

Specs are written last, after every PRD and contract in the set is merged, because a
spec is a traversal of a settled document set. Contracts are written with their PRDs
and not as a later pass.

### Gates

**G1, mechanical.** Editorial rules hold. ASCII only, no em-dashes, no semicolons,
none of the forbidden words, line lengths in corpus range, no swallowed headings, no
double punctuation, no visual collisions. The forbidden-word list and the line width
live in the Working Rules section 1.

**G2, level.** Nothing in the document belongs to a different level. A PRD carrying
protocol, a contract carrying representation, or a spec carrying rationale fails this
gate, and the material relocates rather than being trimmed.

**Transport silence is part of this gate as of 2026-08-24.** A contract states what
crosses, what it means, and how it fails. **It does not name the mechanism that
carries it.** A contract naming a filesystem path, a descriptor, a socket type, a
flag, or a latency the substrate happens to provide has taken a Spec's material into
a contract, which is this gate's own case read one level finer.

**The failure mode is vocabulary rather than subject, which is why it needs stating
separately.** A custody obligation belongs in a contract, so the gate reads
clean on the question it was written to ask and the defect passes. What is at the
wrong level is the wording: **"sets close-on-exec at the fork" names a flag where
"does not permit a child process to inherit the handle" names the obligation.** The
obligation survives a change of substrate. The flag does not.

**The test is mechanical.** Read the clause with the substrate removed. If nothing is
left to require, the clause was representation and relocates. If the requirement
stands and only the noun goes, the noun is rewritten and the clause stays. **This is
what makes an organ relocatable in principle**, and a contract that fails it has
decided the substrate on behalf of every later deployment.

**The rule is the contract's alone, and the other two levels are its opposite.**
Substrate belongs in a PRD, which decides it: a charter ruling that this program
opens no network surface, or that a seam is local, is a decision about substrate and
is that document's to make. **Substrate is what a Spec is about.** A Spec naming no
mechanism would have elected no representation, which is the whole of its job. Only
the contract is silent, and it is silent for one reason: **so that what stands on
either side of a seam can move without the page between them changing.**

**Reading this as a corpus-wide ban is the available mistake and it was made before
the rule was a day old.** A pass proposed stripping mechanisms from thirty-seven PRD
sites, which would have taken decisions out of the documents that make them and left
charters unable to say what they had ruled. The rule names contracts and means
contracts.

**Rationale is developed in the PRD and may be restated in the Spec,** per the
human's ruling of 2026-08-01, which is what the spec clause above means and what
it failed to say. A Spec elects a representation, and an election with no stated
ground cannot be reviewed, so the Spec names the ground it answers to. What it may
not do is develop that ground: a criterion argued first in a Spec has been settled
outside the context that governs it, which is how a PRD and its Spec drift into
saying different things about the same crate. The test is mechanical. Where a
Spec's reasoning traces to a charter clause it is restatement and passes. Where it
does not, the criterion lands in the PRD and the Spec cites it, in the same act
where that is practical and named as owed where it is not. This is the alignment the
colocation rule exists for, one PRD and one Spec in one crate directory. **A Spec
merging ahead of the charter clause it cites names that clause as owed**, which is
the case the register exists for rather than a case the gate refuses.

**G3, graph facts.** The crate has exactly one parent edge, naming its domain parent:
the domain root for a member crate, and `WeaverTools` for a domain root. It carries no
contract and no tag, because nothing is asked across it. It is domain membership rather
than containment, and the two reach the same mechanical shape without being the same
claim, so the word is checked here rather than assumed. Every seam names the contract
that governs it and is tagged socket or link, with the grounds for that tag stated. No
lateral edge to a sibling appears. Floor links are declared as floor links and are not
confused with the parent edge. Checked against the blocks the Document Format defines,
where the containing section is the grounds.

**G4, vocabulary.** Every name a contract's vocabulary clause draws from elsewhere
resolves to a definition that exists where the clause says it does. **Elsewhere is a
crate in the ordinary case and may be another contract**, per the ruling of 2026-09-15
that makes a contract a source of `defines` for the terms its own seam establishes, so
a clause drawing the `election` and the `distillate` from
`weaver-harness-state-contract` resolves against that contract and not against either
party's charter. Every definition a crate holds is either named by some clause or
stated to be internal. Where a clause and the floor disagree, the document names which
side yields and why.

**The gate has two halves and they run on different occasions**, per the per-charter
ruling of 2026-08-23, which removed the phase close that used to carry both. The draw
side, that every name drawn resolves, is checkable against one document and runs when
that document lands, and again over every document drawing from a crate whose
definitions an act moves. **The floor's ritual already requires that second act to
update every affected PRD and contract, so this is the check that the ritual was
carried rather than a separate sweep.** The definition side, that every definition is
drawn or declared internal, is meaningless against a partial set and runs at the
release inventory, over the whole set.

**G5, duplication authority.** Where the same fact is stated in two documents on
purpose, one is named authoritative, and divergence is a defect to file rather than
something a reader resolves by picking.

**G6, extraction complete.** Nothing the graph or the code will need still lives only
in the old tree. This is the gate that makes the deletion in phase two safe.

**G7, rulings landed.** A ruling names the documents it changes, and this gate checks
that each named document carries the change. It is in force because the first live
ruling in this corpus named four documents and landed in none of them, and nothing
detected that until a re-review opened for other reasons. A ruling recorded in a working
list reads as settled to every later reader, so an unlanded ruling is worse than an open
one. It is checkable by looking, since the ruling names the documents and the documents
either carry the change or do not. Where a ruling is landed in part on purpose, the
documents still owed are named as owed rather than left to be noticed.

**That naming is the standing rule for every change, not only for rulings, as of
2026-08-23.** A change names every document it affects. What lands with it, lands.
What does not is named as owed, in the register that tracks it, and leaves that
register when it lands.

**Carrying a whole change in one act is no longer required, and the requirement is
recorded rather than deleted.** Several documents said a change that could not be
carried in one act had not been thought through. That was right while the set was
being established, when nothing existed to be consistent with and a partial change
would have left documents encoding different understandings of the same system with
no register to catch it. The registers exist now, G7 checks them, and demanding
simultaneity of a corpus this size buys nothing the register does not already buy.
**What is required is that nothing a change touches goes unnamed.**

Phase one closes when every crate in scope has a merged PRD, every seam has a merged
contract, every spec is merged, and G4 and G6 hold across the whole set. Only seams
take contracts. The other edge kinds are structure and carry none.

## 5. Phase two, graph

**Phase two is suspended until release**, on the operator's ruling of 2026-09-27.
The HADES graph and every rebuild, ratification, and the closing checklist below
wait for release, and no rebuild is owed on document movement meanwhile. HADES is
not part of this project until then, and it returns at release as a lookup and a
diff of documents against code. Code is checked against the Spec directly in the
meantime, by section 6's conformance gate. The rest of this section is the record of
how the phase ran and the rule it resumes under.

Produces the knowledge graph as a standing artifact, and performs ratification. A
HADES database is stood up from the merged documents, which are already structured to
graph cleanly, with the edges and vocabulary present.

The graph is generated and is never hand-edited. Where the graph and a document
disagree, which of them is wrong is the first question and not a foregone one.
If the document is wrong, the fix is a phase one reopening for that piece
followed by a rebuild. If the document is right, the defect is in extraction, in
ingest, or in what the build took, and the fix is there followed by a rebuild.
Issue #579 is the standing instance: the ingest took Rust sources and Markdown
and left the manifests out, so the graph is wrong about three conformance
citations whose documents are correct. Either way the repair is to a source and
never to the graph. A hand-edited graph is a second source of truth and drifts
from the first, which is the failure this phase exists to prevent.

The graph is what code is checked against in phase three. Prose does not answer a
conformance query and a graph does.

**A workspace and a knowledge graph are not the same unit.** The operator's ruling of
2026-09-13. More than one tree feeds one graph, each on its own schedule, and ingest
is per workspace - so a tree joins when it is ready to be read rather than when the
graph is built. Three are named. This repository is the instrument and what the
instrument is, `weaver-experiments` is what was run on it, and the paper drafts come
later.

**The reason is that the trees answer to different clocks.** Every gate in section 4
and section 6 checks a claim against current state, which is right for a tree that
describes something still changing. An experiment is a dated fact that must never
change: it was run against a commit, and `weaver-web` alone has had nine migrations
and a rewritten Spec since the runs now held elsewhere. Under one set of rules the
edges point at current state and the graph answers confidently that an old result is
about today's code.

**`experiments/` left this repository on 2026-09-13** under that ruling, with its
history, and the reruns are when it becomes worth ingesting. Broken links out are
accepted on the operator's ruling of the same date, every one of those experiments
being due a rerun whatever the graph decides.

**An experiment directory returns when its experiment is a live instrument, per the
operator's ruling of 2026-09-25, and the reason above stays true.** The Weaver probe
reruns per card and per driver, which is the rerun the paragraph above was waiting
for, so `experiments/<experiment>/` stands in this tree again. An experiment is a
hypothesis with a charter, its root `README.md`, which is the primary document of the
experiment and carries no parent. Its arms are sub-directories, one per variable the
hypothesis names, and each arm holds probes, one per measurement, each a directory
carrying that probe's Spec, its `code/` and its `results/`. The Spec and the code
answer to the gate's clock and are read like a crate's, and `results/` holds dated
records the gates and the ingest never read. A Spec belongs to any code that requires
one, a crate's or a probe's. The Document Format's section 2 carries the container
and section 3 the kinds, review reads every document under `experiments/` outside a
`results/`, the charter and each probe's Spec among them, and every probe's `code/`,
and `.hadesignore` excludes `results/` alone. The first experiment is
`experiments/deployment-tuple/`, and its first probe is `device/blackwell/`.

### Closing checklist

Phase two closes on a checklist, each item verifiable by looking. Closing it is what
ratifies the set.

1. Graph built from the merged document set, with no hand edits.
2. Every crate present as a node, every seam present with its contract name and its
   socket-or-link tag.
3. Floor layer present as a layer and not as tree edges.
4. Conformance queries answered: one parent per crate, no lateral edges, every
   vocabulary name resolving to a definition site.
5. **A build question answered, not only a structural one.** Every crate's
   assertions present as nodes with their enforcing instrument, and a query
   naming a crate returning the claims that bind it, per Document Format
   sections 3 and 4. This item exists because the items above it are all
   satisfiable by a graph built from charters and contracts alone, which
   carries nothing from any Spec, so the checklist could close over a graph
   that is complete by its own terms and cannot serve the phase it exists to
   enable. Phase three reads the graph, and a graph that answers only where a
   crate sits tells a coder nothing about what to build.
6. The set-level record marks the document set RATIFIED.
7. Old code removed from the workspace, confirmed gone.

Item 7 is last for a reason. The old tree is still legitimately reachable through
phase one drafting and through fact checks during authoring. It stops being reachable
the moment everything it had to offer has been extracted into artifacts this project
trusts, which is what G6 certifies and what the built graph demonstrates. After item 7
the only sources are the documents, the graph, and the specs. Fresh code has nothing to
copy from, by accident or otherwise. The test is not whether the coding seat intends to
avoid the old tree. The test is whether it can reach it, and the answer must be no.

### The ratification of 2026-08-04

The operator ruled on 2026-08-04 that the set is ratified, and the ruling answers
the question open since 2026-08-02: the set ratifies as the complete document set
for the toolless inference deliverable, and the tool workflow's later arrival is a
planned re-entry to authoring rather than a defect. The half-chartered discipline
anticipated this - crates are chartered workflow by workflow and two charters say
so on their faces - so a re-entry adds a workflow to a settled set rather than
reopening the set's meaning.

The graph was built on the HADES server the same day, per
`HANDOFF-2026-08-04-hades-graph-build`, and the checklist was reported item by
item, with item 1 owing its drop-and-rebuild audit trail. Item 6 is carried by the
apex: section 0's system record bears `tag: ratified`, so the set-level mark is
generated from a document and the never-hand-edited rule reaches the mark itself.
The mark lands in the graph on the next rebuild. Item 7 stands open and is the one
item that outlives ratification: it certifies workspace hygiene rather than the
set's coherence, it waits on G6, and the quarry's deletion being irreversible is
the reason it is not hurried.

For section 6's entry gate, phase two's close reads as items 1 through 6, per the
same ruling, so item 7 blocks neither phase-three entry nor a code merge. What
item 7 protects is held meanwhile by the workspace: the build workspace is a fresh
clone carrying neither the old tree nor the probe's code, so the coding seat has
nothing in reach to copy from while the quarry still stands elsewhere. Item 7
stays owed, and G6 followed by the deletion retire the reachability question
rather than deferring it.

## 6. Phase three, coding

Ratified by the operator, 2026-08-04, all five gates and the three cells below.
**H6 joined them on 2026-09-11**, the first gate added since, **and retired on
2026-09-27** with the graph, on the ruling recorded under the conformance gate
below.
The entry gate held until that date: no crate code was written until this section
ratified and phase two closed, because a gate invented while looking at a diff is a
gate shaped by that diff. Both conditions are met, phase two's close reading as
checklist items 1 through 6 with item 7 outliving it per section 5's ratification
record, and the gates are in force.

The gates:

**H1, authorization.** No code without a merged Spec. Behavior in a diff that traces
to no spec clause is out of scope and returns to phase one rather than being argued at
review.

**H1 is applied as documents are written**, on the operator's ruling of 2026-09-27.
Where code and a Spec disagree, the act decides which of the two is wrong, changes
that one in the same act, and moves on. A contract is part of the Spec under the
same rule, and a contract change reaches every party to it in the same act.

**H2, dependency conformance.** The crate's Cargo dependency list matches its position
in the graph. Every Cargo edge is a declared `floor-link` or a `seam` tagged `link`,
since the Document Format rules that a pair governed by a contract is a seam and never
also a floor link. No dependency on a sibling. The parent edge is domain membership and
appears in no Cargo file, since nesting carries domain rather than dependency. Checked
against the charters' records by review while the graph is deferred to release. **A
dev-dependency is outside this edge set**, on the operator's ruling of 2026-09-22 that
closed the `weaver-state` to `weaver-trace` cell of #586 and #590: it is test
scaffolding, admitted on the condition that the crate's production code imports nothing
from it, a condition the crate's own manifest instrument watches. The edge set is stated
by dependency kind and not by manifest section: every dependency of the normal or build
kind is a production edge, wherever the manifest declares it, in a target-qualified
table, behind a feature, or under a rename alike, and only the dev kind is exempt. The
mechanization above reads cargo's dependency kinds and not the manifest's text. A
dev-dependency that production code reaches is the undeclared edge this gate exists to
refuse.

**H3, seam conformance.** The seam is exercised against the contract's failure cases
and not only its success path. A contract that names a refusal and a build that cannot
produce it has not implemented the contract.

**H4, vocabulary conformance.** The types and traits used across a seam are the ones
the contract's vocabulary clause names, at the definition site the clause names, not a
local redefinition of the same shape.

**H5, advisory pass.** The architecture seat reviews the diff against the PRD, the
contract, and the spec, and returns advice in the standard shape. The
implementation seat holds the merge call and answers the advice in its decision.

**The conformance gate**, on the operator's ruling of 2026-09-27, which retired H6
and the census with it. Every pull request body carries a line naming what it builds:

    Implements: <Spec> <sections>

The Planner's grade gives each named section one verdict. **Conforms**: the code does
what the section says. **Drifted**: the code departs from the section, and the code
is fixed. **Better way**: the code found a better shape than the section, and the
Spec changes in the same pull request, a design-level change going to the operator.
**Spec gap**: the section is silent on what the code does, and the Spec is extended,
a new capability being the operator's ruling. The Planner also greps the Spec for
other claims about the items the pull request changes, since a section named is not
the only place a Spec speaks.

**What the ruling suspends until release**: the graph and every rebuild, ratification,
phase two's checklist, the census, and H6's header rule. Existing assertion records
and existing `conforms:` headers stay in place and are not maintained, and no new
record or header is required. The census, its fixture and its baseline left the tree
in the act that landed the ruling, git being the archive. What it keeps: H1 as above,
the editorial rules, the separation of PRD, contract and Spec, precise vocabulary
across crates, and the instruments, being tests, compile pins, perturbation, clippy
and Codex review.

**The seat reviews twice, and the second pass reviews the rework.** Answering a
review is itself an act: on 2026-09-11 it introduced a real defect in three pull
requests of four, each found by the pass that came after the fixes rather than the
one that found the originals. A second pass is not optional where the first
produced substantive work. **Passing means no finding that changes behaviour or
corrects a claim is unanswered**, a declined finding being answered with its reason
on the pull request.

**A pass is stated before it is hardened**, on the operator's ruling of 2026-09-27,
after #716 took fifteen Codex passes and every finding of every pass was valid. Code
whose exit is a verdict - a pass, a reproduction, an approval - has its Spec state,
before the hardening begins, what a pass certifies, on what evidence the code reads
itself, and what the pass guards against and what it does not. The Planner's grade
before undraft checks that the statement is there. Without it a review has no ground on
which to decline a finding, and each finding is the next site of a claim nobody bounded.
#716's statement arrived at its twelfth pass, and from then on a finding that needed a
party racing the run could be declined by citing it.

**A carry is not a hardening**, on the same ruling. Code brought into the tree from
elsewhere lands first as it is, its Spec marking it a lab instrument and naming its
known limits. The act that makes it an instrument whose verdict the program relies on is
a second act, with the statement of its pass written first. An act that does both widens
the surface each rework puts in front of the next pass, which is how #716's carry became
its hardening one finding at a time.

**More than four review rounds is a checkpoint, not a stop**, on the operator's word of
2026-09-28, which relaxed the stop this section first stated. The first statement
returned a pull request to design at its fifth round. The bound was chiefly a cost
control from the CodeRabbit era, and Codex's review is a fixed cost, so more back and
forth is affordable where it improves the work. Past four rounds the Planner evaluates
whether the findings converge, and says so on the pull request:

- Valid, distinct findings, each a new class, keep the loop going.
- The same class found again, or a run of findings sharing one design question, returns
  the pull request to design, as an act of its own or a ruling.

#683 stopped at twenty-eight passes of narrowly fixed classes. #716 met the fifth round
and went on, and the fold of its two entry points and the statement of its pass, taken
at the twelfth, were that design question answered eight rounds late. The rule is that
the question is asked when the rounds pass four, not that the rounds stop there.

**A pull request names what it answers, and a merge is not done until the ledger
is**, on the operator's ruling of 2026-09-26. The body carries every issue the act
closes and every epic item it closes or moves, by epic and item number with what
the act did to it, and the Planner's verification before undraft checks the list
is there. After the operator merges, the Executor ticks each named item on its
epic with the merge commit, or annotates it as moved or declined with the reason,
in the same session as the merge notice, and the Planner's verification of main
after a merge reads those edits. An epic closes only when its checklist is empty
or every remaining item is annotated with where it went. The audit of that date
found thirty items landed and never ticked across eleven epics, which is the drift
this rule stops. `CLAUDE.md`'s pull request path carries the same rule with the
invocation, and this section owns it.

The three cells, settled with the ratification. H2 runs as a review read of the
charters' records against the manifests while the graph is deferred to release, a
build script being a later mechanization of the same check.
H1's mechanical bar is the Spec's own instruments: the doctests, compile pins, and
perturbation tests land with the code they pin, with clippy and fmt as the floor.

**The bar names instruments and owes the invocation that runs them.** That absence
is the defect of #288 and is closed here. `cargo test --workspace` was **not** the
suite: it compiled `weaver-spu` with no engine, so it ran none of the
model-loading, decode, device, or family-selection tests. Measured 2026-08-23 it
reported 398 passing and did not run 51.

The suite is two invocations and a machine is honest about which it can offer.

    the host suite     cargo test --workspace

    the device suite   cargo test --workspace --all-features

**The host suite is one command as of the gguf default**, and was two while
that gate was off, the second reaching a family surface the first could not
compile. `weaver-spu-Spec` section 1.1 argues the change: the gates' shared
reason, that a build with neither keeps the family surface testable on a
machine with no device, covered the device gate and never covered the other.
The host suite needs no card
and runs everywhere the C++ toolchain builds llama.cpp. The device suite needs
the pair and the fixtures, and it is the one whose green means the suite is
green. **A merge states which it ran**, because a claim of green that does not
say which suite is a claim about an unnamed subset.

**The perturbation obligation reaches the rule, not only the test.** Apex
section 11's third device is the standing rule: always confirm the test fails
when the property is removed, because a test that passes either way converts
unenforced into documented as enforced. That is stated for tests, and two
cases fell outside it in the week of 2026-08-23.

**A judgment can have no test to perturb.** #284 narrowed the readout refusal
from the container to the family's declaration, which is the whole substance
of that act. Restoring the container ground afterwards, which undoes it
entirely, **failed nothing in a hundred and sixty-four tests**. Every negative
case reached its refusal through a family declaring no tap and so refused
under either rule, and the judgment's own unit tests called it directly
without travelling the admit path. The obligation says to confirm that a test
fails when its property is removed, and the property here had no test whose
failure could be confirmed, so the obligation had nothing to bite on. **So an
act that narrows or widens a judgment perturbs the judgment**, restoring what
it retired, and records what caught it. Where nothing does, that is the finding,
and the act owes the watch before it owes anything else.

**A build requirement cannot be perturbed on the machine that has it.** The
`gguf` gate carrying the default on 2026-08-23 left `llama-cpp-2` taking CUDA
unconditionally, so a host build would have demanded a toolchain it has no use
for. No test on this workstation can fail for that, because the machine it
breaks does not exist here, and two documents asserted the opposite while the
suite ran green. **So an act that changes what a build requires names the
machine that can no longer build it**, and that naming is the check. A green
suite is silent on this by construction rather than by oversight.

**The record goes in the act.** A perturbation run and reported in a
conversation is evidence that expires with the session. Named in the commit or
the pull request, it is the one place a later reader can find out whether the
property was ever watched. Apex section 11's closing line is the reason: a
clean automated gate is evidence that the gate did not fire, and it is not
evidence of correctness.

**Coverage is not readable from any one manifest.** Cargo unifies features across a
workspace, so a crate's tests may run because a different crate wanted the feature.
`weaver-types` gains twenty-one config tests under `--workspace` only because
`weaver-admin` takes that feature for its own reasons, and nothing connects the two.
Were admin to stop needing it, those tests would stop running with no edit to
`weaver-types` and nothing reporting the change. So a run states the features it
resolved rather than the features it was asked for.

**The three bars are not equally held today, and saying so is the point of naming
them.** Measured 2026-08-23: the device suite passes, `cargo clippy --workspace
--all-targets` reports six warnings, and `cargo fmt --all --check` reports a hundred
and thirty-nine diffs. A floor stated and unmet reads to a later seat as a floor that
was never meant, so either the tree rises to it or the bar is rewritten to what the
work holds. That ruling is the operator's and is not taken here.
A failing H3 case files as a known gap with a named owner rather than blocking
merge, for the loop 0 act only, because the bare-minimum milestone does not wait
on refusal-path coverage - and the gaps are named at filing so the exception does
not become the permanent state. After loop 0, a failing H3 case blocks merge.

## 7. Current position

**Where the work sits as of 2026-09-27.** The knowledge graph and its upkeep are
deferred to release, on the operator's ruling of that date recorded in section 6:
review against the named Spec sections is the conformance check, the census and H6
are retired, and ratification and phase two wait for release. HADES is not part of
this project until then. The graph paragraphs below are the record of the phase as
it ran, and none of them owes work before release.

The set was ratified set-wide on 2026-08-04 per the operator's ruling recorded in
section 5, and **charters have ratified on their own since 2026-08-23**, so the set
is whatever the charters currently say. Phase one closed with the whole set merged:
seven charters, seven Specs, the contract layer, and the assertion records under
their instruments. **Two crates were chartered since**, `weaver-state` and
`weaver-internal`, both on
2026-08-18, both ratified under the per-charter rule, and **both joined the apex's
enumeration on 2026-08-23 when it was corrected to nine.** **The roster reached
ten on 2026-08-24**, when `weaver-diagnostic` was chartered as a consumer
outside the boundary and the operator's later ruling of the same date moved it
inside as the harness's third member, the mechanism the harness authors a
diagnostic-trace through. `weaver-analysis` was chartered beside it in that act
and does not enter the roster, holding the position outside that
`weaver-diagnostic` vacated. Phase two ran
on the HADES server per `HANDOFF-2026-08-04-hades-graph-build`, the graph stood up
from the merged set, and the set-level mark rides the apex's system record.

**The diagnostic leg is delivered end to end, 2026-09-01, and the roster
is twelve directories under `crates/`.** What was owed at the last refresh
is built: `weaver-diagnostic` writes the diagnostic-trace as the harness's
third member, the replay loop runs from the Gateless seat's own criterion,
the null replay certifies against real records, the column seam carries the
residual vectors under the diagnostic binding and no other, and
`weaver-analysis` stands outside the boundary as the driver and the reader
- deriving the declaration from the record, preloading, gating on the
stated outcome, applying the lens, and comparing two captures exactly.
Epic #293 closed on the operator's direction with its one live row, the
capture artifact, carried by issue #386, and that act's papers landed the
artifact criteria from measurement rather than assumption. **The vector
bar is a measured number**: two certified column replays of one source
differenced to 9,784,320 of 9,784,320 values exactly equal, so
certification's vector comparison is exact within a device model and the
float tolerance is the cross-device bar alone.

**What the instrument measured about itself belongs here too, because it
bounds where the next acts point.** On the 0.5b the lens reads concrete and
lexical content and does not read the abstract evocations the source
paper's workspace results turn on - unchanged at five times the fitting
compute, so the bound is the model's scale rather than the fit's thinness.
The families above it are therefore where the instrument earns its keep,
and `weaver-spu`'s per-family `taps_readout` and `taps_column` are what
stand between it and them, each owed its neutrality demonstration on the
engine that would serve it, per issue #212.

**The graph was rebuilt 2026-08-08 from `98c8713`** and stood at 293 nodes and 426 edges
in 19 `wt_` collections under the named graph `corpus_graph`. The census below verified
on that build. Two earlier builds preceded it, 2026-08-06 from `96c40bb` and 2026-08-04
from `0426ef5`, and each was a drop-and-rebuild rather than an upsert, which is
checklist item 1's audit trail as far as it has been recorded. **A rebuild was owed on
document movement even where the census did not move**, until the graph was deferred to
release on 2026-09-27. The 2026-08-08 rebuild found the record set almost unchanged
across 28 document commits, two assertions retagged and none added or removed, while 138
of 242 assertions pointed at a line the document no longer held. A count check would
have reported that graph healthy, so a matching census is not evidence a rebuild can be
skipped. Document movement since `98c8713` had added three `draws` edges by 2026-08-10,
and the state leg's papers of 2026-08-18 and 2026-08-19 have since added a crate node,
its parent and seam edges, a contract with its parties, draws, and four term
definitions, and three assertion records, so the stated expectation of 293 nodes and 429
edges is withdrawn as stale. The next build, at release, derives its expected census
from a fresh pass over the merged set before it runs, stated as numbers at that pass per
this section's own discipline.

The floor probe of 2026-08-04 is the evidence the entry into code rested on: a
commissioned session with no repository access rebuilt both floor crates from the
graph and the corpus text, both compiled, the suite passed whole, and all 41 floor
assertion slugs landed under their instruments. Its three divergences were named
for the first code act rather than as defects in the set. One, the probe inverted
the non-exhaustive election and re-aimed the compile-fail pin at the inverted
claim: the real Specs elect the attribute per type, growing sets carrying it and
closed sets not. Two, the probe elected two permission modes where the floor Spec
enumerates three, Ask, Allow, and Deny. Three, the probe filled the fault report's
deliberately open election with an invented shape, and the deferral is the
corpus's and stands.

Seat assignment follows section 1's rule unchanged: the seat holding the working
tree authors, and review runs through the PR's review seats. Later code acts sit
where the operator points them.

**An eighth crate joined the set's shadow on 2026-08-18 and the roster
below predates it.** The statefulness leg returned through apex section 9's
door: `weaver-state-PRD` chartered the custodian on the operator's rulings,
`weaver-harness-state-contract` and the Spec landed its seam and shapes, and
the code acts stood the ingest (#208) and the serve direction with its
first asker, the context-injection loop (#209, #210), each proven against
the living agent - the loop now injects the session's shape at a run's
opening and the model has answered from it. The crate carries three
conformance headers, whose assertion records land with the position
refresh of 2026-08-19 after the refresh found them cited but undeclared,
each tagged review. A recount over the eight-crate set is owed at the next
counting pass and the table below is the seven-crate figure of 2026-08-16.

**All seven crates of the ratified set are built and merged.** Each file
carries its conformance header per Document Format sections 3 and 4, and no
crate carries a header citing an assertion no Spec declares. Recounted
2026-08-16 over every tracked unit carrying a header, per the Document
Format's counting clause:

    weaver-types      18/18      weaver-harness   55/56
    weaver-traits     24/24      weaver-gate      27/27
    weaver-trace      38/38      weaver-admin     32/32
    weaver-spu        60/62

**The figures moved on the method rather than on the work**, six of the seven
rows changing at that recount and none of them because a crate gained or lost a
citation that day. The count had been taken over `src` alone, which excluded
every assertion whose citation sits in an integration test while including
manifest assertions cited in `lib.rs` whose instrument is the manifest. The
earlier table read `types 17/17`, `trace 39/39`, `harness 48/48`, `gate 23/23`,
`admin 31/31`, and `spu 56/60`, and it is recorded here because a reader
comparing an old batch against this section needs to know the ruler changed and
not the thing measured.

**The roll of open assertions is recounted 2026-09-02, and it is fifteen
rather than three.** The method is stated so a later reader can repeat it
rather than trust it: every `node:` in an assertion record across `docs`,
against every `conforms:` header across `crates`, the difference being
what no code file claims. **355 assertion records are declared and 340 are
cited**, and the tag census runs 137 perturbation, 136 review, 32
manifest, 32 compile-pin, and 18 compile-fail. The earlier roll of three
was taken by hand at a smaller set and did not move as the set grew, which
is the drift a stated method exists to prevent.

    weaver-analysis   analysis-binds-no-port                        review
    weaver-analysis   analysis-writes-no-record                     compile-fail
    weaver-harness    harness-idle-report-authors-without-a-turn     perturbation
    weaver-spu        spu-architecture-and-markers-are-unique        compile-pin
    weaver-spu        spu-elected-readout-changes-no-token           perturbation
    weaver-spu        spu-family-is-architecture-and-template        perturbation
    weaver-spu        spu-field-changes-no-token                     perturbation
    weaver-spu        spu-field-depth-refused-below-the-cutoff       perturbation
    weaver-spu        spu-gguf-is-the-default-gate                   manifest
    weaver-spu        spu-one-forward-per-prompt                     review
    weaver-spu        spu-reduction-renders-its-shape                perturbation
    weaver-spu        spu-room-refusal-carries-capacity              perturbation
    weaver-spu        spu-sampler-holds-nothing-between-generations   perturbation
    weaver-spu        spu-seed-derives-per-generation                perturbation
    weaver-trace      trace-no-version-member                        review

**Uncited is not the same as unbought, and the roll cannot tell them
apart.** Three of the fifteen are review-tagged, where the instrument is a
reading and a header is a courtesy rather than the purchase. The rest name
an instrument, and for those the header's absence is either an unwritten
test or a written one whose act forgot the citation - a distinction only
the reading of each can make, which is the counting pass owed rather than
this refresh's to settle. **What the roll does say is where they cluster**:
eleven of fifteen are the SPU's, the crate that grew fastest under the
readout and decode acts, and the seed derivation, the room refusal, and
the sampler's memory each name behaviour the seam tests exercise daily
without claiming.

`spu-two-taps-one-shape` left this roll on the GGUF tap's landing, and
`harness-idle-report-authors-without-a-turn` remains what it was: the idle
report is unbuilt, so nothing authors one at all, and its sibling
`harness-frame-grants-the-seat` is cited.

**Four came off this roll on 2026-08-16, and each is named with what closed
it**, because a reader who saw one listed learns it closed rather than finding
it absent:

    admin-run-reference-distinguishes                    cited, PR 161
    spu-session-parameters-carry-dispositions            cited, PR 160
    spu-tunables-arrive-in-the-declaration               cited, PR 160
    harness-organ-argv-carries-construction-parameters   cited, PR 162

`admin-run-reference-distinguishes` was satisfied on 2026-08-15 and uncited, the
act that built the three-part reference not adding the header, so its citation
landed separately. The other three were cited by the acts that closed them, the
last of them landing hours after the document that authorized it, which is the
ordinary sequence rather than a delay.

An assertion sitting in a census unexplained is the shape this project has
repeatedly found, a record unable to say what it did not measure, so the roll
carries the reason beside the name in both directions: what an open one waits
on, and what closed one that has gone.

**Both decode engines are written, and the turn completes through either.**
The GGUF engine landed 2026-08-08 and the native engine followed across
issue #158's arc, closed 2026-08-19: `native.rs` stood 2026-08-17 (#196),
the pair
forward and the split loaders opened the dual-GPU grid in both containers
(#200, #201), and a 65 GB sharded artifact too large for any single card
served across the pair (#202). Section 4.1's derivation answers for both
containers to about 90 GiB across the device pair. The direct peer-to-peer
reduction was entered measured at the close and rejected on the evidence,
the hop staying host-staged with the rejection recorded at the function.
Epic #130 completed the first live turn on 2026-08-14, gate to trace,
against a real local model, and turns have run daily since.

**The demonstration is the evidence and it is inspectable.** A trace taken
2026-08-16 carries one run reference over six turns, each running
`turn.started`, `message.user`, `model.request`, `model.output`,
`model.measurement`, `message.assistant`, `turn.closed`, the whole bracketed by
a `load` and an `unload`. So a directive does reach an engine from outside its
process and an answer returns.

**What rides back with it is the measurement payload, and the readout joins
it where elected**, the two being apex section 7.2's two items rather than
one. The payload carries the timings, the per-token entropies and
surprisals, and a block label over the turn's token range. The
residual-stream readout's native tap stood 2026-08-19 with #158's closing
act: an elected qwen2 residency taps every forward at either width, each
layer's norm taken on the device with one scalar crossing, and the
reduction travels in the measurement as `residual_norms`, absent rather
than empty. The refusal turns on the election alone: an elected GGUF load
refuses at admit by name, that tap being unwritten still, while a GGUF
load with `residual_readout_election` false admits and serves turns
exactly as before the tap existed. Where readout is not elected the SPU
emits no `residual_norms` member at all, and the harness and the trace
carry the measurement as opaque JSON either way, so the omission stays an
absence in the record rather than anything converted to empty. The
standing agent declaration elects no readout.

**A completed turn satisfies no conformance assertion, and the SPU's two
open moved on 2026-08-19 without closing.** Both waited on the readout tap
existing, and the native half now does. `spu-two-taps-one-shape` still
waits on the GGUF half, the eval-callback pin holding that seam open with
nothing driving it. `spu-one-forward-per-prompt` is now watchable under
the standing native tap and waits only on its count being taken. A reader
who takes the demonstration as having closed either has read a count into
a behaviour.

An earlier wording of this paragraph said four, which was the figure the table
carried when the count ran over `src` alone. Two of that four were cited all
along in the tests that buy them, and the recount above is where the figure and
its method now sit together.

The caution this paragraph carried still holds and now points both ways. A
crate could always report a high conformance figure while completing no turn,
which is why the apex asks for a demonstration and not a count, and a completed
turn is likewise not a count of claims met. Read the figures below as what they
are, and read the trace for whether the deliverable runs.

**The four defects the live turn surfaced are closed, 2026-08-15.** Run
identity landed first because five registered measurements join a result to a
trace and could not: the session is the operator's and declared in
`agent.toml`, and the run reference is minted at the load from an instant, the
agent's name, and eight bytes of randomness, so a declaration without a
`session` field is now refused at load. The unload's misreport of a clean
unwind closed against the Spec's own clause. The unclosed run bracket on a
failed load needed no ruling, the charter and the Spec having both already
required the rollback to direct a leave. The gate socket closed by the
operator's reshaping of 2026-08-14: the socket is fixed by the application
rather than named by a declaration, so `GateInstruction` no longer carries a
path and the stale-pathname hazard is unreachable rather than guarded against.

**A measurement regime stands outside this repository and the first baseline is
taken.** Nine tests are registered with their methods before they run, and each
result carries the conditions that make it comparable later, which includes the
commit, the build profile, and the identity of the binaries measured rather
than the profile's bare claim. It is deliberately not a corpus member and
nothing here is written against it. It does not reach the gates, and a reading
it produces is evidence about the code rather than authority over a document.

**Where the work sits as of 2026-09-25.** `experiments/` returned for the Weaver probe
under section 5's ruling of that date, as a live instrument whose code and Spec are
gated and whose results are dated records outside the gates.

**Where the work sat as of 2026-09-13.** `experiments/` left this repository for the
`weaver-experiments` tree under section 5's ruling of that date, with its history, and
every archive directory left under section 1's. The paragraph on the experiment
directory below was written about #404 and about a directory that no longer stands here,
and it is kept because the reporting defect it names is unfixed and travels with the
runs.

**Where the work sat as of 2026-09-02.** The seat is using the framework
rather than building it, per the 2026-08-19 shift, and the pulls this week
came from use exactly as that shift predicted: the diagnostic leg's papers
were pulled by a replay that had to run, the artifact criteria by
artifacts that already existed and needed identity, and the vector bar by
a comparison that wanted a number. What stands open, in the order the
seat holds it: the family taps toward the scale the lens needs (#212), the
streaming shape the sink's declaration already permits and no run has
exercised, the counting pass the roll above names, and the graph rebuild
owed since `98c8713` and unrunnable while HADES is down. One operational
finding rides beside them, filed 2026-09-02 as #404: a deployment whose
organ binaries and admin come from different commits dies with a bare
`Undecodable` and reports as `no_residency`, naming neither the binaries
nor the field, so an experiment directory is all-or-nothing until that
reporting is sharpened.

What remains from the phase behind: G6 and then item 7, and the G2 and G5
phase-close sweeps.

**The graph's expected census stood at 242 assertion nodes carried by 243 `asserts`
edges, and that pair is withdrawn as stale in both terms**, on the same discipline
section 5 applies to the 293 nodes and 429 edges it withdrew. It existed so a rebuild
could detect a change nobody intended, and it can no longer do that job: the corpus
has grown past it and the disclosure that stood here named only the state leg's three
review assertions of 2026-08-19, which is node growth and cannot account for an edge
moving at all. The act of 2026-09-16 that gave the four replay claims a second
asserting crate moved the edge count by four and is the case in point. The absolute
pair waits for the pre-build pass at release named above, and no figure stands here
in the meantime, a wrong number being worse for a detector than none.

**What replaces it is a check that does not go stale: the edge census exceeds the node
census by one for each crate beyond the first that holds a claim, which is the sum
over claims of the asserting crates less one, and by nothing else.** A node takes one
`asserts` edge per crate that holds the claim, so a rebuild derives the delta from the
corpus in the same pass that counts it rather than reading a roll kept by hand here.
**The count is of edges beyond the first and never of claims**, the two coinciding
only while no claim is held by three crates, which is a property of the tree at a
moment rather than of the rule. Which figure a check reads still matters, and the
closing checklist's item 5 reads nodes. A rebuild whose delta disagrees with that
derivation has found either an unlanded edit or an assertion an act changed without
recording.

**Code is ingested into the graph, and the position that it should not be is
retired as of 2026-09-14.** The v4 build named in
`docs/project/HANDOFF-2026-09-12-the-graph-build` reversed it crate by crate, and
the builds since carry Rust sources beside the Markdown documents. Issue #579
records what the ingest does not take. Two grounds were given for holding code
out and both are discharged. The earlier was that conformance headers cited
retired assertions and ingesting would bake dangling edges into the map, and that
count reached zero on 2026-08-08. The later was the operator's: the architecture
is not stable while acts like the 2026-08-05 re-entry still move it, and a
conformance graph built from moving code would record a shape neither the
documents nor the code will keep. **What that ground did not anticipate is the
question the graph answered first**, which is where a document and the code
disagree about a count, a file list, or a name. That question does not wait on
the architecture settling, and the audits of 2026-09-13 and 2026-09-14 are what
it produced. Both are dated readings rather than members of the set.
`weaver-agents-PRD` section 11 carries the same retirement, landed in the same
act.

**The seat shifted 2026-08-19, on the operator's direction: from building
the framework to using it.** The apex deliverable stands and is exceeded,
and the suite around it is named in the living vision's section 13,
weaver-web standing up as the first outside consumer against the two
external contracts of 2026-08-01. **It stood in its own tree until 2026-08-23 and
is absorbed into this one by the ruling of that date**, which leaves the contract
coupling untouched and makes both sides of the seam editable in one commit. **It left
again on 2026-09-26, on the operator's ruling of that date**, for
`WeaverTools_Project/weaver-web/` beside this repository, its history carried by subtree
split and its repository to be decided, because a frontend this close to the code slowed
production down: it stands up as the first outside consumer from outside the tree, which
is where an outside consumer stands, and the contracts are what it builds against. What
that changes here is who leads: needs discovered in use pull framework acts through the
change protocols, where the roadmap once pushed, and the predicted pulls are streaming
through the gate's world contract, a status ask on admin's operator contract, and the
operator's read on state per that charter's named cell. Framework work queued on its own
account: the Python connector of issue #134, the payload-key election through the
declaration, and the Role::System floor act.

## 8. What this document does not do

It does not govern the content of any crate. It governs which seat is primary, in what
order the work moves, and what must be true before it moves. A rule that constrains
what a crate does rather than how it comes to exist belongs in the apex or in a
charter.

It does not say what shape a document takes. That is the Document Format's.
