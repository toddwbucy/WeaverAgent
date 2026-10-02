# WeaverTools Document Format

**Version:** v0.25, 2026-09-27. Companion to the Working Process. Project
documents carry a version and a date and no state, per Working Process section 2.
**Parent:** WeaverTools Working Process

The Working Process says who is primary and in what order the work moves. This says
what shape the documents take so that the graph can be built from them mechanically.

**Ratification is per-charter as of 2026-08-23**, per Working Process section 2 as
amended, and section 9 of this document carries what that changed. What has not
changed is why the notation is mechanical: a document that has to be interpreted
before it can be mapped puts a human in the middle of a step this project treats as a
gate, and an interpreted mapping is a hand edit wearing a different name. So the nodes
and the edges are stated, not implied.

## 0. What this document is for

Every document in the corpus already states edges. It states them in four notations
and none of them is mechanical. `Parent:` headers state one kind. `Depends on:` states
another and carries no tag. `weaver-harness-PRD` section 4 states its crate's seams
and floor links in prose, the charter being the authority on how many there are and
this sentence naming the notation rather than restating the count. Every contract's
vocabulary clause states a whole edge set to a fixed shape by ruling, which makes it
the largest body of edge data in the corpus and still not a form a mapper can read.

This document does not invent the edges. It picks one notation for edges that are
already being written and says where the notation goes.

**The graph is deferred to release**, on the operator's ruling of 2026-09-27 recorded
at Working Process section 6, and two obligations below are suspended with it. **The
assertion record** of section 3 and **the conformance header** of sections 3 and 4
are required of nothing new until release: existing records and existing headers are
legal, stay in place, and are not maintained. The rest of the notation still governs
how a document is written, since a block states its edges to a reader as surely as
to a mapper, and nothing reads either obligation until the graph returns.

## 1. Two node layers, and the third code adds

The document layer is what the corpus is made of. The subject layer is what the corpus
is about. These are different graphs sharing one file set, and a file can sit in both
without the layers collapsing. A contract is the standing case: it is a document node
with a container, and it is also the source of the `party` edges that bind its crates,
the `draws` edges that name what its clause reaches, and the `defines` edges that name
its own seam's terms. The document it is and the crates it speaks about are different
subjects, and a mapper that treats one as the other builds a graph where a file is its
own dependency.

A document node carries a container and nothing else. The container comes from the
directory, per section 2, so a document node is built by walking the tree rather than by
reading a block. **Only a document an edge runs from needs a `node` record, and the test
is being that endpoint rather than being where the record is written.** A contract is
the one kind that qualifies: `party` and `draws` run from the contract node, and so does
a `defines` edge where the contract defines its own seam's vocabulary. A PRD writes
`defines` records too, forty of the corpus's forty-four, and needs no node of its own
because those edges run from the crate the charter is about. Section 4 puts the `from`
at a crate or at a contract for that reason, and a rule reading `defines` as
contract-sourced would be wrong about forty of them.

No edge joins these two layers. A document's subject is its directory and its
filename, which the walk already has, so a `describes` edge would carry a fact
nothing had to look up. The same argument retired `governs`: an edge no rule can
emit, restating what another record already holds.

Phase three adds the code layer, per section 3's `code` kind. A source unit is a
file in the tree and is still not a document node: the document layer holds what the
Working Process's three kinds produce, and a source file enters the graph only
through the node its own conformance header declares. The `cites` edge of section 4
is the one relation that leaves the layer, running from a code node to an assertion
in the subject layer, which is apex section 11's chain in graph form. The no-edge
rule between the document and subject layers stands unchanged: code cites a claim,
and nothing cites a file.

**That layer is every supported unit, and Rust is not alone in it.** A conformance
header exists so the `cites` edge can be drawn, and a unit the ingest never reads has
no edge for a header to carry, so the kind a unit is decides whether the layer
reaches it at all. Rust is primary in this workspace and the supported kinds are
`.rs`, `.toml`, `.cu` and `.py`, on the operator's ruling of 2026-09-17. **A
language outside that set is deferred rather than excused**, its units entering the
layer in the act that lands their support, which is why the set is written here and
a list of what stays out is not: such a list goes stale from the day support moves,
and it reads to a later seat as a ruling nobody made. One kind is excluded outright
rather than deferred, and `.hadesignore` carries that exclusion with the argument
for it, which is the one place either is stated.

**Reading a citation and owing a header are two questions and the sets differ.** The
manifest reads and owes nothing, having no module of its own to head, which is why
the three `tag: manifest` citations resolve at all. Section 3 carries which kinds owe
and this section carries which are read, and a seat narrowing one to match the other
loses those three.

**Kind is half the bound and ownership is the other half**, per section 3, which
reads the unit as a tracked file a workspace member owns. The ingest is negated back
over `process/gates/` and `process/ingest/`, so five `.py` files under them are read
by it and owe no header, correctly: they are this project's instruments and not a
crate's source. **An argument from the ingest alone would pull them into the layer**,
which is the shape to check for whenever this clause is applied to a new kind.

**Exempt from a count is not outside the regime**, and the distance between those
is why the boundary falls at the kind rather than inside the reading. A supported
unit carrying no header is an absence the count returns and the corpus answers,
which is what a count is for. Carving a kind out of the count instead leaves its
units under a rule nothing measures, which is the state this clause replaces: the
reading walked Rust alone while section 3's rule read every tracked unit, so a
kernel and three loop files stood inside the crates that rule names and nothing
ever asked them for a header.

State is not a graph property. It answers no query the phase two checklist names, it
lives in the status header where the merge process reads it, and a second copy in a
block would be a G5 duplication with nothing gained. Section 5's key set closes
without a state key deliberately.

## 2. Containers come from the directory

The directory structure supplies the container of every document node and no document
declares its own container.

    process/                            the project documents
    docs/project/                       the apex, and documents outside the set
    docs/crates/weaver-<n>/             the PRD and the Spec for one crate
    docs/crates/weaver-<n>/weaver-<m>/  a member crate of that domain root
    docs/crates/contracts/              contracts, and the material they draw
    docs/crates/weaver-<n>/Loops/       workflow documents of that domain root
    experiments/<e>/README.md           the charter of one experiment, its hypothesis
    experiments/<e>/<arm>/              one variable of that hypothesis
    experiments/<e>/<arm>/<probe>/      one measurement: its Spec, code/ and results/

A document under `docs/crates/contracts/` is a contract by its `-contract` suffix,
named for its parties. A document there without the suffix is drawn material, stated
once where the contracts that cite it draw from it, and `weaver-organ-channel` is
the first, per the filing ruling its section 5 records. The suffix reading is the
same exclusion-by-naming device this document uses twice already, the absent
`weaver-` prefix and the `.stub` suffix. A document under a `weaver-<n>/` directory
is a PRD or a Spec by its own filename suffix. This is what makes the mirror check
mechanical: every `docs/crates/weaver-*` has a matching `crates/weaver-*` at the
same depth, and `contracts/` excludes itself by the absent prefix rather than by a
written exception.

A document under a `Loops/` directory is a workflow document, the container typing the
kind directly because that directory splits into no further kinds, where `contracts/`
needs the suffix to split contracts from the material they draw. It walks a settled set
and authors no edges of its own, so it carries no graph block, and where it and a
charter disagree the charter yields nothing. `Loops/` excludes itself from the mirror by
the absent `weaver-` prefix, the same exclusion `contracts/` takes. Loops file under the
organ that holds them, and the holder is decided by the human's ruling of 2026-08-01:
loops belong to the harness unless specific to one domain and no other, a loop confined
to one domain filing under that domain's own root. The first such directory is
`docs/crates/weaver-harness/Loops/`. The composition of loop 0 is not a loop and does
not file under a `Loops/` directory, which is what the ruling settles. It was written
as `load-unload-path`, archived 2026-08-23 once `weaver-harness-Spec` carried its
rules directly. Per the operator's ruling of 2026-08-05: the loop taxonomy reaches
loop 1 and above, and the bracket the loops run inside is the harness's mechanism
rather than a member of the taxonomy.

**A contract is named for its parties, initiator first.** The name records which of the
two nodes initiates the signal the contract governs, so `weaver-admin` asking the
harness to enter and leave a run gives `weaver-admin-harness-contract`, and the harness
asking the SPU to admit and release a model gives `weaver-harness-spu-contract`. This is
the same fact the seam record carries in `from` and `to`, which is why a name that
disagrees with its seam is a defect in one of them rather than a matter of taste.

**The name states flow direction and nothing else.** It does not say which crate
sequences the workflow that signal sits inside, and it does not say that this is the
only traffic between the pair. Behavior between two nodes collapses into direction of
flow, one signal at a time, and a later signal running the other way is a second seam
with its own name rather than a contradiction of this one.

**A live stub carries `.stub` and is not a document.** It is not a `.md` file, so the
walk never reaches it and the mirror never counts it, which is the same exclusion by
naming that `contracts/` gets from its absent prefix. A stub leaves the tree in the
act that cuts its draft, per Working Process section 2 as ruled 2026-07-31, the draft
landing at the `.md` name beside the suffix's tracked deletion, so no consumed stub
survives to be walked and history is the archive.

**An experiment carries its own documents beside its code**, per the operator's
rulings of 2026-09-25 recorded at Working Process section 5. `experiments/<e>/` is
the container of one experiment, a hypothesis, and its root `README.md` is the
charter that registers it: what is declared, what is predicted, what would falsify
it, and which probe answers for each part. Beneath it an arm is one variable the
hypothesis names, and a probe under an arm is one measurement of that variable,
holding its Spec as `<probe>-Spec.md`, its `code/`, which review reads exactly as it
reads a crate's units, and its `results/`, which holds the
dated records the probe produces and which no gate and no ingest reads, on the
two-clocks reason that section 5 of the Working Process states. The mirror rule does
not reach it, since the documents and the code share one tree rather than two. The
first experiment is `experiments/deployment-tuple/`, its first arm `device/`, and its
first probe `device/blackwell/`.

**The project documents sit outside `docs/` rather than under it.** Working Process
section 2 puts them outside the document set and outside the mapping, so a mapper that
walks `docs/` never reaches them and needs no rule to skip them. Placing them under
`docs/project/` would have required one.

**The mirror is recursive, and the nesting carries domain rather than dependency.** A
**domain root** is a crate that other crates are members of, and a member crate nests
inside its root at both trees. The word is not *composition root*, which this program
already uses for the wiring site inside one binary where the concrete transport is
constructed and injected. That is one place, made of code. This is several, made of
crates. Section 5 forbids two spellings of one name, and one spelling of two things
is the same defect with the halves swapped. `weaver-trace` sits under `weaver-harness`
because the harness is its only caller, not because nothing else may link it. Nesting
is not a visibility rule, a Cargo boundary, or a claim about who may depend on what.
Those are settled by the seam and floor-link records, which do not read the
directory. Nesting says which crate holds the domain, and its purpose is that
someone who knows where a crate lives can find its documentation without a search.

**The floor does not nest.** Which crates are floor is `weaver-agent-PRD` section 5.1's
to say and not this document's, per section 8. What belongs here is where they sit:
a floor crate is placed at the top of the crate tree rather than under a domain root.
Filing one under a root would make every other root reach through that root's subtree
to draw floor vocabulary, which reads as the floor belonging to one domain when the
invariant has it belonging to none. The rule is a directory rule and takes its
classification from the apex, so a change to what the floor consists of changes no
line here.

**Depth is discovered, not designed.** A root gains a subtree when it gains a member,
and a root with no members yet is a directory holding one PRD. Drawing a subtree for
a crate that has not been chartered would guess at its membership, which is the
reserved-slot move applied to directories.

## 3. Node kinds

**system.** One node, `WeaverTools`, declared by the apex. It exists because G3
requires each crate to have exactly one parent and the domain roots have no crate
above them.

**A parent edge names the domain parent.** A member crate parents to its domain root and
a domain root parents to `WeaverTools`, so the parent edges reproduce the directory tree
exactly and the graph answers the question the filesystem answers. This matters because
domain membership is an architectural fact, and a fact that lived only in a directory
listing would be hiding in the one place section 1 says facts are not allowed to hide.
Every crate still has exactly one parent and the crate graph is still a tree, so the
shape G3 checks is unchanged. What changed is what the edge means, and G3's wording is
updated in the same act rather than left to be read against a rule it predates.

**The `Parent:` header names the same thing the edge does.** It carries the charter of
the domain parent, which is the apex for a domain root and the root's charter for a
member. The header is a reader's convenience and the edge governs, so a disagreement
between them is a defect in the header. It is kept rather than dropped because a
charter opened on its own should say what it belongs to without a directory listing to
hand. An experiment's charter carries none, having no parent, as the apex carries
none, and a probe's Spec names that charter.

**A crate outside the agent boundary names the suite.** `weaver-agent-PRD` section 0
rules that such a crate does not refine that document and parents to the suite, and that
what governs outside crates at the suite level is not yet written. So the value is the
suite rather than a charter, and the graph parent edge still names the `WeaverTools`
system node, which is the same belonging under the name the graph has for it.
`weaver-analysis-PRD` is the one document that takes it, `weaver-web-PRD` having taken
it too until its crate left the repository on 2026-09-26.

**What the apex's absent header reads against.** Working Rules section 1 elects which
fields each kind carries and is authoritative for `weaver-agent-PRD` carrying none.
What this section owes is the reading against the edge, and the edge is missing rather
than impossible: that document charters the weaver-agent domain and not the suite, per
its own section 0, which records that the graph carries no `weaver-agent` node and that
the level lands either as a Format act adding a kind or with the crate tree gaining the
directory. Until one of those happens there is nothing for a value in that header to
agree with, and the act that builds the level is the act that says what the header then
carries.

**What the field means on a contract is unsettled.** A contract declares `party` edges
and no `parent` edge, so the derivation above reaches it with nothing to read, and the
contracts carrying the field name an apex invariant, which is an axiom citation rather
than a parent. Issue #637 carries the measurement and the question.

**crate.** Every crate, whether a domain root or a member of one. Organ is the
vision document's word and it does not cover the floor, so it is not used here. One PRD,
one Spec, and as many contracts as it has seams. **A crate whose sources and charter
live outside this repository has its node and its one `parent` edge declared by the
contract it is party to**, as of 2026-09-26 and pending the operator's word, so the
`party` edge and any seam edge naming it point at a declared node rather than dangling
and the crate keeps its place in the domain tree G3 reads. `weaver-web` is the one case,
its charter having left with the crate.

**experiment.** One node per experiment, declared by its charter, the root
`README.md` of `experiments/<e>/`, which is the primary document of the experiment
and carries no `Parent:` and no `parent` edge, as the apex carries none. The charter
is the one document whose filename is not its Document ID, `<e>-PRD`, because the
operator ruled that a reader opens the README first, and the ID keeps the kind's
name. The charter registers the hypothesis and every prediction and falsifier before
any probe runs, and the experiment node is the one record it authors.

**probe.** A code root under an experiment's arm, declared by its own Spec, which
asserts the probe's records the way a crate's Spec asserts the crate's, per the
operator's rulings of 2026-09-25. The probe declares one `parent` edge to its
experiment's node, and the Spec's `Parent:` names the charter, so the header and the
edge agree as section 3 requires of every other pair. The probe's Spec states what
its code enforces. A Spec belongs to any code that requires one, and
a probe's code requires one for the same reason a crate's does, the phase-three rule
that no code lands without a ratified document. The charter and the Spec are the two
document kinds an experiment produces, and neither is a contract, an experiment
having no seams of its own.

**artifact.** A durable thing produced or consumed by crates without a call between
them. The agent config, the session record, the model artifact. Artifacts are
first-class because the corpus's real relationships around them are three-place and
pass through one: `weaver-types` owns the agent config's format, the operator
writes it from outside the program, and `weaver-admin` and the harness both read it,
with no Cargo edge anywhere in that sentence. Without an artifact node those
relationships are untypeable. An earlier form of this entry had `weaver-admin` as the
writer and cited a deferred state-file contract, and both halves are retired:
authorship is the operator's per `weaver-admin-PRD` section 1, and section 10 of that
charter rules the contract out rather than deferring it, there being no producer
inside the program to bind.

**vocabulary.** A named definition a crate owns and contracts draw: a trait, a type, a
mode, an event kind. The common unit G4 resolves against.

**term.** A named meaning the documents settle that no crate owns as a single
definition. The four terms of the harness-state seam are the standing case, the
`election` that opens the channel, the `distillate` that crosses it, and the `ask` and
the `answer` the serve direction carries, each established by the contract that governs
the seam rather than by either party to it. The `diagnostic-trace` is the other, a
record kind its own charter names. **A term is not a vocabulary node under a second
word.** A vocabulary node is one definition a crate owns and its Spec represents, which
is what tells a reader which crate to open. A term is a meaning the corpus holds steady
while code spells it wherever it needs it, the tee's election standing in four
representations at once per `weaver-trace-Spec` section 3. The kind is admitted on the
ruling of 2026-09-15, which took the five declarations the corpus already carried
rather than retyping them into a kind that answers a different question.

**axiom.** One of the apex's five invariants, declared by `weaver-agent-PRD` at the
subsection that states it. An axiom is not a claim about a crate and binds no code
directly. It is what a claim can be grounded in, so a query can ask which claims serve
an invariant and which invariant a claim serves. There are five and the set is closed: a
sixth would be an apex act, not a Format one. It grew from four on 2026-08-03, so a
version of this format saying four is behind the apex rather than ahead of it.

**assertion.** A claim a Spec makes that code must conform to, named so a source
file can cite it and a query can return it. Two kinds of clause qualify and both
are the same node: what a Spec's enforcement section lists, which is already a
discrete checkable claim, and the load-bearing elections outside those sections,
the socket type or the descriptor placement or a stated bound, which code must
conform to as surely and which gate H1 would otherwise leave untraceable.
**No new assertion record is required until release**, per section 0, and a record
standing is legal and unmaintained.
**A claim that divides into two records counts wholly as the first kind.** Where a
clause names one instrument for a claim's core and another for its periphery, the
two records it becomes both belong to the enforcement section for any provenance a
Spec states, whichever sections they sit in, because neither half was elected and
one was divided out of a bullet the enforcement section already carried. Counting
the review half as an election would make the same document report a different
split depending on how many of its claims happened to divide, which is a fact
about the division and not about the document.

**An assertion node names and locates a clause and never carries it.** The graph
is an index into the Specs rather than a copy of them: a reader queries what binds
the module in hand, receives identifiers with their sections, and reads those
clauses. Carrying the clause would put its content in two places, which is the
duplication G5 makes someone adjudicate, and it would grow the graph into the
topology document this format has refused twice.

**document.** A PRD, a Spec, or a contract. Working Process section 4 produces
three kinds and no fourth, and the two documents this format types that are none of
the three do not breach that: drawn material is contract material stated once,
holding what the organ contracts share and deciding nothing they do not draw, and a
workflow document walks a settled set and authors no edges of its own. The three
kinds author the graph's document-sourced records. These two add no authored record
to it, which is what the Working Process's count is a count of, and the code layer
authors its own through the conformance header rather than through any document.

**code.** A source unit carrying a conformance header, arriving in phase three and never
before. **Until release no unit owes a header**, per section 0, and a header standing is
legal and unmaintained. A code node is declared by its own header rather than by any
fenced block, the way a child declares its own parent edge: the header is the set of
citations the unit carries and it names the assertion identifiers the unit conforms to.
A unit is a tracked file a workspace member owns, of the kinds the reading walks, the
crate's manifest among them and its source files the rest. **The bound is ownership and
kind and never the marker a file could hold**, a `#` line being writable in most of a
tree that has no business citing anything. The mapper reads headers at merge, so code
accrues into the graph as work merges and no document restates what source already
carries. Its identifier is the source path relative to the repository root, the one
spelling the filesystem already enforces. The kebab-case rule governs names this format
invents, and a path is not invented, so `crates/weaver-types/src/role.rs` is a node
identifier as it stands. One canonical form, so the path cannot do what two spellings of
a name do: forward slashes, no leading `./`, exactly as `git ls-files` prints it. The
mapper derives the node identifier and every `cites` edge's `from` value from that one
form, so a second spelling of one file is a defect the same way `permission-modes`
beside `permission-mode` would be.

**A conformance count is over every tracked unit carrying a header, and a
directory is never the rule.** The scope follows from the node kind, a code node
being any source unit with a header wherever it sits, so no enumeration of
directories belongs in a count and one that appears is a defect in the count
rather than a tightening of it. Stated because it was mis-applied: a count taken
over `src` alone reported the crates through 2026-08-16, and it excluded every
assertion whose citation sits in an integration test while including manifest
assertions cited in `lib.rs` whose instrument is the manifest. That is arbitrary
rather than strict, and it undercounted six of seven crates.

**The tag is what says where a citation belongs.** An assertion names its
enforcing instrument, so a `perturbation` claim is bought by a test and cited
where the test is, a `review` claim is cited in the unit review reads, and a
`manifest` claim is cited in the unit that carries the crate's root. A citation
sitting where its instrument sits is correct placement and never a gap, which is
the reading a directory-bounded count gets backwards.

The rule is written against the tree rather than against today's layout for the
same reason the identifier is: the layout moves. A crate that grows a build
script or a benchmark carrying a header gains a code node by this clause without
the clause being revised, and a count that enumerated directories would have to
be, silently, by whoever noticed.

## 4. Edge kinds

Between crates:

- `parent`, exactly one per crate, declared by the child.
- `floor-link`, a compile-time dependency on a floor crate, declared as a floor link
  and never confused with the parent edge.
- `seam`, a place where one crate asks another to do something. Carries `via`, the
  governing contract, and `tag`, either `socket` or `link`. A seam with no governing
  contract is an incomplete edge and G3 fails on it.

A pair is never both a `floor-link` and a `seam`. Linking a floor crate and asking it
to do something are different relationships and the harness has one of each: it links
`weaver-types` and asks nothing of it, and it links `weaver-trace` and asks it to
record. Where a governing contract exists the pair is a `seam` tagged `link`, because
the seam record carries the contract and the floor-link record cannot. Where none
exists the pair is a `floor-link`. Writing both for one pair produces two edges where
the system has one relationship.

A seam is declared once, from the declaring side. On a seam with one asking party
the asker declares. On an organ channel both parties ask, so the asks rule has no
unique answer there, and the organ declares rather than the harness, per the rule
`weaver-admin-harness-contract` section 0 states: the harness is the hub every organ
holds its two-initiator channel with, and a hub that declared its own edges would
carry the whole seam graph in one crate. The other party's charter points at the
contract and does not restate the edge, which is what keeps the party list checkable
rather than doubled.

Nothing points back at a seam. A seam is an edge, an edge cannot be the target of an
edge, and giving seams identifiers so that contracts could address them would buy an
inverse edge that carries no fact the seam's own `via` does not already carry. The
governing relation is stated once, on the seam, from the declaring side. The
contract's `party` records are what make the pair checkable from the other direction.

Between an assertion and an axiom:

- `grounds`, from an assertion to the axiom it serves, declared by the Spec that
  argues the assertion, in the same block. This is the third term of apex section 11's
  chain read upward: code cites an assertion, an assertion grounds in an invariant.

**A claim grounds in an axiom two ways, and the second is easy to miss.** The first is
that the axiom is the reason the claim exists: remove the axiom and ask whether the
claim still has a point. The second is that **the claim is a precondition of the
axiom's stated reason.** Where an invariant argues from a premise about the world rather
than from a rule, whatever holds that premise true serves it, and a corpus grounding
only the first relation leaves the premise unguarded. The apex's possession case is the
live instance: it rests authentication on no third party being able to reach a socket
that has no address, and a process's own descriptor table is that address unless
something closes it.

**An assertion with no `grounds` edge is not a defect and G3 does not fail on one.**
Most of this corpus is representation election - a format, a name shape, a tagging
rule, a bound - which code must conform to as surely and which serves no invariant
because the invariants are not about representation. An axiom layer that demanded
total coverage would collect edges drawn to satisfy the demand, which is the failure
mode the prior program's basis reached at seven of seventy-one claims: the number was
low and the response was to keep the layer rather than ask what the layer was for.
What the edge is for is the query, and a query is only worth its answer if a missing
edge means the claim is representation rather than that nobody got to it.

**A clause grounded in one Spec is grounded in every Spec that states it for the same
reason, and where the reason differs the Spec says so at the clause.** Settled here
on 2026-09-14. Several claims recur across crates in one wording - a flag cleared, a
descriptor flagged at creation, an enum exhaustive against its contract, a manifest
carrying no internal dependency - and an edge on one statement and none on its twin
is a clause that was copied without its edge rather than a second judgment. So the
asymmetry is the defect and it resolves both ways: the edge is drawn where the second
Spec argues what the first argued, and the second Spec states its different reason
where it has one, which is what turns a bare clause into a decision a later reader
meets rather than a gap.

**Parity is not coverage and this rule reaches no further than the asymmetry.** A
clause bare in every Spec that states it is representation under the paragraph above
and nothing here touches it, and no edge is drawn whose argument no Spec makes. The
rule exists because the paragraph above protects a considered nothing and cannot
distinguish one from a copy, and that is the whole of the gap it fills. An act that
read it as licence to raise the grounded count would collect the edges drawn to
satisfy a demand, which is the failure the paragraph above names by its number.

**A reason is stated wherever the Spec states it, and the enforcement section is one
of the places.** The clause above says a differing reason is written at the clause,
and that is where a Spec writes a new one, the clause being where a reader meets the
claim. It is not where a Spec is read for one already written: a Spec that argues in
its enforcement section that one axiom reaches nothing in it has stated a reason for
every clause in it, and the parity rule is answered before the clauses are reached.
One question has one answer each - write at the clause, read at both - and the first
form of this paragraph gave two answers to the second by naming only the enforcement
section. Where such a document-wide reason stands, a parity edge drawn against it is
a disagreement between two Specs about an axiom's scope rather than a copy repaired,
and that disagreement goes to the operator, which is what stops this rule from
settling a scope question by the side effect of counting slugs. Stated because the
act that landed it drew two such edges into `weaver-trace-Spec` before that
document's own enforcement section was read, and withdrew them.

**The comparison is over the set of axioms and never over whether the edge is
there.** Two statements of one clause can both carry an edge and still disagree, one
grounding in two invariants where the other grounds in one, and a reading that asks
only whether a `grounds` edge exists reports that pair as agreeing. The corpus
carried exactly that case while the first reading of this rule was taken.

Between code and an assertion:

- `cites`, from a code node to an assertion it conforms to, declared by the source
  file's conformance header. This is the first term of apex section 11's chain, and
  it arrives in phase three: no document authors a `cites` edge, because a document
  stating what code conforms to would be a claim about code that only the code can
  make. The header is the declaration and the mapper reads it at merge.

**The code layer authors no fenced block and the seven keys do not grow.** A code
node and its `cites` edges are read from the conformance header, which is source
rather than notation, so section 5's block grammar is untouched and a `graph` fence
appearing in source is a defect. The header's shape is fixed the way the keys are
fixed, so the mapper never guesses: one line per citation, each reading
`conforms: <crate>-<slug>` behind a comment marker its file admits, and the markers are
four. `//!` is the file's own and sits at the top above any other doc comment, which is
the form to reach for where the claim is the whole file's. `///` sits at the item that
holds the instrument and `//` at the statement that holds it, so a file carrying
several claims can say which line answers each. `#` is the manifest's, a
`Cargo.toml` having no doc comment to hoist a citation into. **Whatever the marker, a
citation is the first thing on its line**, and a marker that trails other code on the
same line carries none. That is the quiet failure rather than the loud one, because a
perturbation cited in the trailing position reads as uncited and no gate names the
line it is on. A header naming an assertion the corpus does not declare is a dangling
edge and the mapping pass fails on it, which is the no-dangling-endpoint precondition
reaching code.

**The four forms are admitted as citation forms on the ruling of 2026-09-15, and what
moved was this document.** A citation is what binds a unit to an assertion, and
`census.py` resolved one from any of the four before the question was put, so a
rule admitting one form alone was a rule the reading gate did not run under.
That act moved no count, which is the shape of a format catching up with a reading
rather than changing one.

**The module header is a second obligation and the marker that carries it is the
unit's own file-level leader.** Admitting a form as a citation says nothing about
where a file owes a header, and phase three's rule that every source unit carries one
was read by H6 through that marker by itself, and is suspended with H6 until release.
A file citing only at item level resolved each of its citations and was counted
headerless all the same. **For Rust and CUDA the
leader is `//!`**, which the language gives to the file rather than to an item, so it
heads a unit wherever in the unit it sits. **For Python it is `#` at the head of the
unit**, on the ruling of 2026-09-17 that admitted the kind: `#` opens every comment
Python has, so position is what makes one a header, and a reader matching it anywhere
would make an item's citation the file's and leave the obligation weaker for Python
than for Rust. The head is the shebang, the blank lines and the `#` comments that open
the unit, and it ends at the first line that is none of those.

**The count and the four files this passage carried are deleted rather than
corrected.** It read "four such files stand inside today's forty-eight" and named
`replay.rs`, `native.rs`, `gemma4.rs` and `mistral3.rs`. Measured 2026-09-17: all four
carry `//!` headers, no headerless unit in the tree cites at item level, and the
reading is not forty-eight. A count in a governing document argues with the command
the first time the command is run, so the rule stands here and a reading, when one is
wanted, is taken from the tree. `act-04-inline-citations-get-a-header` of issue #569
was filed against the set this passage listed and closed with that issue under the
ruling of 2026-09-27.

The alternative the 2026-09-15 ruling weighed was to hoist every citation into
a file header, which a manifest cannot hold at all, and the manifest citations are the
ones that let H2 be asked of the graph: it carries `crates/weaver-analysis/Cargo.toml`
citing `analysis-no-internal-dependency`, which is a claim about a dependency list that
no other file in that crate can make. The marker set is closed at four and a fifth is a
Format act rather than a convenience.

Between a contract and what it binds:

- `party`, from a contract to each crate it binds.
- `draws`, from a contract to each node its clause names. This is the vocabulary
  clause in edge form and it is what makes G4 a query rather than a reading.
  **An endpoint is a `vocabulary` node or a `term` node and nothing else**, on the
  ruling of 2026-09-16 that closes the item the format left open here. A clause names
  a meaning the contract depends on, and section 3 types a meaning two ways: a
  `vocabulary` node where one crate owns the definition, and a `term` where the
  documents settle the meaning and no crate owns it as a single definition. Both are
  meanings, so both are drawn, and the `election` and the `distillate` drawn by
  `weaver-analysis-state-contract` and the `diagnostic-trace` drawn by
  `weaver-harness-diagnostic-contract` are admitted where they stand rather than
  retyped. **The other seven kinds section 3 declares are each disposed of**, which
  is what makes the rule exhaustive rather than merely stated: an artifact is
  reached through the field the clause draws, per the `holds` entry below, and the
  `system` node, a crate, a document, an axiom, an assertion and a code unit are the
  subjects of other edges entirely. Two plus seven is section 3's nine, and an act
  that adds a kind adds it to one side of that count or the other.
  **A field of a drawn meaning is not a second endpoint.**
  `weaver-harness-state-contract` section 7 draws the `canonical-event` and then
  spells its five envelope fields, which says what the drawn meaning carries rather
  than naming five more meanings, and the edge points at the node that owns them.
  The artifact case runs the other way because an artifact is not a meaning, so a
  clause that needs one points at the field, which is, and `holds` carries the rest.
  Those five are declared nowhere and this rule asks for no declaration of them,
  the format carrying no edge between a meaning and its parts and `elects` running
  from a field to the values it selects from rather than to the whole it sits in.
  **A `draws` edge introduces no node**, exactly as a `defines` edge does not, so an
  endpoint no document declares is a dangling edge rather than an implied node.

Between a crate and what it owns or touches:

- `defines`, from a crate or a contract to a vocabulary node or a term. The definition
  site declares the node with a `node` record beside the edge. A `defines` edge does
  not introduce its target, because an implied node has nowhere to carry a kind and
  because implied nodes are what this format exists to remove. **A contract sources
  this edge where what is defined is the seam's own**, on the ruling of 2026-09-15. A
  contract defining the terms its own clause establishes is a contract doing what it is
  for, and relocating them into one party's charter would put a seam's vocabulary where
  a single party owns it. The crate case is unchanged and is still the common one.
- `elects`, from a vocabulary node that is a field to the vocabulary node whose values
  it selects from or is validated against. The floor's two-crate structure rests
  entirely on this relation: the agent config's permission mode and tool set are
  `weaver-types` fields electing from `weaver-traits` definitions, and a `floor-link`
  is crate-level and carries none of it.
- `holds`, from an artifact to a vocabulary node that is one of its fields. A clause
  that draws a field of an artifact points at the field, which is a vocabulary node
  the owning crate defines, and the artifact holds it. Without this the config's
  fields are drawable in prose and unaddressable in the graph.
- `writes` and `reads`, from a crate to an artifact.
- `asserts`, from a crate or a probe to an assertion node. The Spec declares
  the assertion node with a `node` record beside the edge, at the clause the
  assertion names, the same shape `defines` takes at a definition site.

**A Spec states records and is not their source, which is the shape a PRD already
has.** `asserts` runs from the crate rather than from the document, so a Spec needs
no `node` record of its own and section 1's rule stands unchanged: a contract is
still the one document kind that sources an edge, because `party`, `draws` and
`defines` run from the contract itself. What changes is that the Spec joins the PRD
as a document that states its subject crate's records, which is why this format's
placement rule reaches it without amendment.

**Assertion identifiers are `<crate>-<slug>` and carry no positional number.** The
crate prefix is what keeps two Specs from naming one thing twice, and the bar on
positions is the load-bearing half: a positional number renumbers when something is
inserted, and every citation of one then points at the wrong claim. The slug says
what the claim is rather than where it sits.

**A figure that is part of a name or a value is not a position and is admitted.**
The qualifier arrived on 2026-08-03 with the pilot act, where the flat rule would
have refused `types-loop0-encoding-json` and `types-envelope-bound-64k`, one naming
the loop this corpus calls loop 0 and the other carrying a stated bound. Neither
renumbers when a claim is inserted, which is the whole of what the rule guards
against, and a rule that forbade them would leave a slug unable to name the thing
its own corpus named. The test is mechanical enough to apply: ask whether the figure
would change if a neighbouring claim were added, and refuse it only then.

## 5. The block

One fenced block, info string `graph`, one or more records, records separated by a
blank line, one `key: value` per line. A mapper finds every block by its fence and
needs to know nothing else about markdown.

    ```graph
    edge: seam
    from: weaver-harness
    to: weaver-trace
    via: weaver-harness-trace-contract
    tag: link
    ```

    ```graph
    node: weaver-harness
    kind: crate

    edge: floor-link
    from: weaver-harness
    to: weaver-types
    ```

Identifiers are kebab-case, always, including for vocabulary that names a Rust item.
`Tool` is `tool-trait` and `Provider` is `provider-trait`. Two spellings of one name
is how a graph acquires two nodes for one definition, and a rule that says follow the
source spelling produces exactly that on the day a trait is renamed. Identifiers that
differ by one character are a G1 visual collision as much as a mapping hazard, so
`permission-modes` and `permission-mode` do not both exist.

**The rule governs names this format invents.** That is section 3's reason for a
source path being a node identifier as it stands, and it is the same reason the
system node is `WeaverTools`: the name of the thing being built, carried by the
repository, the workspace and the graph alike, so a format that renamed it would
mint the second spelling this rule exists to prevent. Two classes stand outside the
rule and section 3 declares both, the system node being one and no other and a code
node's identifier being a path. Per the operator's ruling of 2026-09-16.

**`tag` carries one vocabulary per record kind and the kind disambiguates.** On a
seam it is `socket` or `link`. On an assertion it is the instrument that enforces the
claim, `compile-pin`, `compile-fail`, `perturbation`, `manifest`, or `review`, which
is the sorting every Spec already performs in its own enforcement section and which
apex section 11 requires a claim to state. A second key for the instrument would
grow the fixed set for a fact the existing key already carries, and the point of the
fixed set is that the mapper never guesses. **A threat walk's test tags
`perturbation`**, settled here rather than seven times: the Specs phrase their
walks as a category beside the perturbation-verified list, and apex section 11's
third device is written as a blanket obligation on behavioral tests, always
confirm the test fails when the property is removed, so a walk's test is
perturbation-verified by that obligation whatever a section calls it. The walk
itself is prose that derives the test and takes no node. On the system record, the
vocabulary is `ratified`, which is what lets the graph's set-level mark be generated
from the apex rather than hand-edited, per Working Process section 5's checklist item
6. **It marks the set-wide act of 2026-08-04 and nothing narrower.** Since
ratification became per-charter on 2026-08-23 a charter no longer waits on that mark,
and whether a ratified charter carries its own `tag: ratified` is the open question
section 9 names.

**`review` means an instrument was not bought and never that none exists**, per
apex section 11, and the checkable form of that rule is a sighting rather than a
verdict. A `review` assertion cited at a test is a disagreement between its tag and
its citation: a test is an instrument, so a citation written at one says an
instrument exists, and a tag reading `review` beside it says the opposite. Cited at
a test means the citation a file under a crate's `tests/` target carries, every unit
there being a test, or a citation written at an item or at a statement inside a
`#[cfg(test)]` suite. **The scope follows where the instrument sits and never a
directory.** `weaver-state` has no `tests/` target at all and holds every test it has in
the in-file suite, as `weaver-web` did until it left the repository on 2026-09-26, so a
scope named for the directory is empty by construction for one of the eleven crates, and
section 4 admits four citation markers where a path under `tests/` reads one.

**A sighting is read per claim and four dispositions occur in this corpus.** The tag
moves, where the test is the instrument the claim wants and the clause bought it
without saying so. The citation is deleted, where the unit that holds the instrument
already cites the claim and the test-side citation is a second copy of an edge the
graph has. The citation names a different node, where the test beside it holds a
neighbouring claim and the header cited this one instead. Or nothing moves, where
the clause names what was declined and what was taken in its place and the test
watches a property the claim does not make. **A retag taken over the reading** would
write a defect at every claim of the last kind, which is the same overclaim run the
other way.

**The clause settles the tag and never the citation.** A clause naming what was
declined and what was taken in its place answers whether `review` is the right tag,
and it says nothing about whether the citation sits at the unit that holds the
claim. Both of this corpus's mis-citations stand under clauses that argue their own
declining, so a compliant clause closes a sighting's first question and leaves the
second one open. Naming the non-purchase is what separates an election from an
overclaim, because the overclaim forecloses tests the corpus may later want.

**Two Specs write their clauses that way and neither is evidence of compliance under
this scope.** `weaver-diagnostic-Spec` section 7 states the rule in these words
beside the shared fixture that would buy the one claim it declines, and
`weaver-gate-Spec` section 6 argues an election at each of its nine clauses while
saying of its own walk that it is not a clearance against the inverse overclaim and
is not offered as one. Both crates cite every one of those claims from `src/` file
headers, which the scope does not reach, so what the two show is the clause a sound
election carries rather than a corpus that complies.

**The reading is taken from the tree and the rule carries no instrument yet.** It is
every node whose record reads `tag: review` matched against every `conforms:`
citation under `crates`, narrowed to the citations the scope above admits, which
wants a grep over two trees and no graph server. Issue #569 carried each reading with
its date and each sighting with its disposition rather than this document, a count
written here being stale by the next act. **The reading was owed to
`act-20-census-readings`**, which closed with #569 and the census under the ruling of
2026-09-27. The rule is checked by looking, and a sighting is answered on the act that
opens the file it sits in.

Keys are `node`, `kind`, `edge`, `from`, `to`, `via`, and `tag`. A record
begins with `node` or with `edge` and no record carries both. Unknown keys are a defect
rather than an extension, because the point of the fixed set is that the mapper never
guesses.

## 6. Placement, and why not a header block

A block sits in the section that argues the edge, directly under the prose that argues
it. Not in a header block, and not in a topology document.

A topology document is the single document binding every crate that the vocabulary
ruling already rejected, wearing a different noun. It separates every claim from its
grounds, it duplicates facts the charters also state, and under G5 it would need an
authority named against each one.

A header block is the same failure at smaller scale. It collects every edge at the top
of the file, two screens from the paragraph that justifies it, so a rewrite of the
paragraph and a stale line in the header cannot be seen together. G3 asks for grounds
per edge, and grounds are prose.

So the grounds are the containing section, with no way to write them anywhere else.
Position is the pointer, and a key that let a block point at a remote argument would
be the header block's failure admitted one record at a time. This is the
vocabulary clause's own argument applied one level out: a clause is checked at
the seam, by the people writing that seam, while they are thinking about it.

## 7. Where a block and its prose disagree

The prose yields nothing and the block yields. The prose is the argument and the block
is its projection, so a block that contradicts the paragraph above it is a transcription
defect and is fixed by rewriting the block.

This is a G5 authority statement and it is written here once rather than at every
block, because the relationship is the same at every block. It is the only duplication
in the corpus that needs no local authority line.

**Where two Specs disagree about what an invariant reaches, read the contract before
arguing the invariant.** A disagreement between two documents about the scope of a rule
looks like a question of doctrine and is usually a question of fact, and the contract
governing the seam is where the fact lives. The labelling batch of 2026-08-03 is the
worked case: two Specs divided a claim by instrument and one refused to ground its
compile-time half, citing this format's rule that internal representation appears in no
contract. The reading was reasonable and the contract had already settled it, stating
that lower is last and terminal and that turn exchanges are valid only between a
completed raise and a lower. Those are ordering guarantees a contract is required to
name, so the claim was never internal representation, and a doctrinal argument between
two Specs was about to decide a question the seam's own governing document answers.

**Where the contract is silent, the answer is to settle it in the contract, not to
reason around it.** A seam question two parties answer differently is not a hard case
calling for a ruling. It is an incomplete contract, which apex section 5.3 forbids by
name: a contract states the ordering guarantees it relies on and provides, and a
disagreement about ordering is that clause missing. **Settling such a question anywhere
but the contract leaves the defect in place and adds a second statement of the answer**,
which is the duplication G5 then has to adjudicate. The obligation runs the other way
from how it feels in the moment: the disagreement is the finding, and the contract is
where it is owed.

Reaching for the invariant first inverts the corpus, since an invariant binds every
crate and a contract binds the two parties that meet at the seam. It is also the cheaper
check, being one document and one clause rather than a comparison across Specs.

## 8. What this does not do

It does not say what any document contains. Level discipline is G2's and the three
document kinds are the Working Process's. A document that carries correct blocks and
wrong material passes this format and fails that gate.

**One clause stands against that and section 5 carries it.** The `review` tag's rule
reads a Spec clause for the non-purchase it names, which is material rather than
notation. It is admitted because `review` is this format's own vocabulary and the
clause is the only place the tag's meaning can be read: a tag saying an instrument
was not bought cannot be checked without the sentence naming which one. The exception
is that clause and no other, and a second one is a Format act.

It does not make the graph. Phase two builds the graph from documents written this way
and never by hand.

## 9. When mapping runs

**Mapping is suspended until release** with the graph, per section 0. What follows is
how it ran and the rule it resumes under.

Mapping runs continuously, from the first charter. **Ratification is per-charter as of
2026-08-23**, per the operator's ruling of that date and Working Process section 2 as
amended, so a charter ratifies on its own by conforming to the pattern the set-wide act
of 2026-08-04 established.

A format that is mechanical from the first charter can be mapped the day that charter
lands, which surfaces a missing edge while one document is still in hand rather than in
a terminal pass over nine. The alternative concentrates every mapping defect into one
pass at the end of phase one, where each fix is a phase one reopening and the document
that would answer it was written weeks earlier.

**Mapping is still not ratification, and the two were separated before the rule
changed.** A mapped document is not thereby a ratified one, and an intermediate build
is a check rather than a milestone. What changed on 2026-08-23 is what supplies
ratification, which is now the charter clearing its gates rather than a set-wide act.
What has not changed is that the graph records rather than confers it.

What continuous mapping buys is that phase two's closing checklist met a graph that
had already been built many times rather than one being attempted for the first time.

**The set-level mark records the founding act and is unchanged.** Whether a ratified
charter carries its own `tag: ratified` beside it is open, and it is this document's
question rather than the Working Process's, since the mark is generated from the
notation defined here. Until it is settled the mark means the 2026-08-04 act and
nothing narrower.
