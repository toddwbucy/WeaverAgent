# weaver-types - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. The crate PRD set is
written together and merged together, and no Spec is written against any member
before the whole set is merged. **Ratification became per-charter on 2026-08-23**, per
Working
Process section 2 as amended, so this document is ratified on its own terms rather
than waiting on the set. The set-wide act of 2026-08-04 established the pattern it
conforms to.

**Date filed:** 2026-07-29
**Document ID:** `weaver-types-PRD`
**Parent:** `weaver-agent-PRD`
**Depends on:** `weaver-traits`
**Editorial:** Per the Working Rules.
**Landing PR:** #72

---

## 1. What this crate is

Where `weaver-traits` holds the contracts the engine is written against,
`weaver-types` holds **the declaration that configures an agent and the identity by
which processes recognize each other**. It binds no socket, spawns no process, holds
no handle, and depends on `weaver-traits` and nothing else internal.

That dependency is a floor link and not a lateral edge. Both crates sit on the floor
under the same root, so an untagged edge between them would read as the sibling
dependency G3 forbids. It is declared as what it is.

```graph
node: weaver-types
kind: crate

edge: parent
from: weaver-types
to: WeaverTools

edge: floor-link
from: weaver-types
to: weaver-traits
```

**It is a requirements document, not a catalogue.** Under the apex vocabulary-clause
rule every contract names what it draws from here in its vocabulary clause, and a
contract without that clause is not valid. This charter states what a crate must satisfy
to participate, and the terms are enforced at every seam rather than in one document
nobody opens.

For a data kernel the compiler is the primary enforcement: the struct definitions are
the schema and the serialization derives are the format. This charter carries only
what the compiler cannot, which is semantic invariant, compatibility posture, and the
obligations that run from a producer to a consumer.

## 2. What it holds

Everything here is present because the harness demonstrably needs it. The set grows
when a later crate demands more, by the ritual: a crate PRD added or changed names
the floor edit it needs, which lands with it or is named as owed.

### 2.1 The agent config

The declarative document that defines an agent. The operator produces it, and both
`weaver-admin` and the harness read it. Neither crate in this program authors it,
because creating an agent is an operator act and the file is its declaration, per
`weaver-admin-PRD` section 1. Admin validates it before a process exists and the harness
consumes the elections it carries.

**The system prompt is state and not a declaration's text or a file it names, on the
operator's ruling of 2026-10-06 (#1)**, which supersedes the ruling of 2026-10-02 that
made it a file. The declaration says how the agent is built and state says who it is:
the operator drafts the prompt as a markdown file beside the declaration, the seeding
step sends it through the gate as the agent's first turn, the state member holds it,
and every later load seats what the member answers. Admin reads no prompt, the enter
carries none in bytes or by digest, and the agent process never reads the draft. The
declaration's grammar carries no identity key, both earlier forms refusing by name, per
`weaver-types-Spec` section 2, and the wire carries the operator's uid so the harness
admits the seeding line from the operator alone, per section 4.

**Validation divides by binding time, per the operator's ruling of 2026-08-02.** The
compiler verifies the frozen half at build, an immutable binary needing no load-time
check of what it baked, and admin verifies the tunable remainder at load, at whatever
size the builder left it. Everything baked but a weights location gives admin one field
to verify. Everything left open gives admin all of it. The earlier
worker-validates-at-enter sketch is superseded.

**Each organ registers the surface it needs to run, and the registration is declared at
build.** Admin runs before any organ process exists, so it can never ask one what it
needs, and the binary's declared surface is therefore emitted at build rather than
answered at load. The config rides one common syntax: the declared surface is a template
the operator completes, and a builder's fields extend the same dialect the core uses
rather than inventing a second.

**So refusal divides three ways and each way has an owner**, per the operator's ruling
of 2026-08-04:

- **A registered field the config omits is admin's refusal.** The operator learns the
  file is incomplete before a process exists.
- **A field the config carries that no organ registered is admin's refusal.** This is
  the typo case, and it is why an unknown field is refused rather than ignored.
- **A field present, registered, and wrong is the organ's refusal**, travelling back
  through the harness as a `lifecycle-refusal` on that organ's own seam. Admin checks
  presence and needs no domain knowledge to do it. What a good value looks like is the
  organ's to know, per apex section 5.5, and an organ answering for its own domain is
  what keeps admin from having to hold a view of every domain at once.

**A conversion may narrow a refusal for a reader and may not be the only
record of it**, per the operator's ruling of 2026-08-22. A refusal crosses
boundaries on its way to whoever must act on it, and every boundary is a
chance to lose what it carried. The rule is one rule because the losses were
found as three:

- A crate's own refusal converts into the seam vocabulary that crosses.
  `AdmitRefusal`, `FamilyRefusal`, `ReadoutRefusal`, `DeviceRefusal`, and
  `KnobRefusal` each become a `lifecycle-refusal`, and `RenderRefusal`
  becomes a `token-refusal`. **`RaiseRefusal` is the case that shows what
  the rule demands**: the gate's bind failure carries a detail the closed
  set has no field for, so the seam carries `BindFailed` alone and the
  detail goes to standard error. **That satisfies the rule only because the
  deployment retains it.** The organ's streams are inherited from the worker, whose
  standard output and error the start step of `weaver-admin-Spec` section 6 points at
  the agent's worker log, so the detail lands there, beside the operations log and never
  in it.

  **The retention is out of band and the rule does not treat that as equal
  to the record.** The worker log is the deployment's and this program does not
  read it, so an operator clearing a bind path has
  the detail and a consumer replaying a trace does not. **Naming which of
  the two a narrowing leaves behind is part of applying the rule**, and a
  narrowing whose only carrier is neither the record nor a retained
  out-of-band channel is the case the rule forbids outright.
- A seam refusal renders for a client that is owed a sentence rather than a
  case. That rendering is **derived from the recorded refusal** rather than
  matched in parallel, so the client's account cannot say what the record
  does not.
- A refusal reaches the record as itself, which is what the clerking act of
  this date provides. **This is the condition the other two rest on**: a
  derived rendering is safe because the record holds what it narrowed, and a
  seam vocabulary is safe because the case it carries reaches the record
  whole. **Remove the source record and both become the only copy again**,
  which is the state the rule forbids rather than a lesser version of it.

**The narrowing is not the defect and being the only copy is.** A client
sentence that drops four integers is right to drop them. What would be wrong
is that sentence standing as the whole account, which is the state every one
of the three was in before this rule. **Where a conversion is the only
carrier, the loss is permanent and silent**, and the party that would notice
is downstream of the boundary that dropped it.

**The file's readers are still admin and the harness and no organ reads it.** An organ
receives the slice it registered across its own seam, handed to it by the harness at
enter, which is the integration the loop performs rather than a second reader of the
operator's declaration.

**What the ruling leaves for the spec round, named rather than implied.** The
un-freezable bound: knobs admin consumes on its side of the load, `trace-sink` above
all, cannot be frozen into the worker binary without handing the agent the sink's name.
The trace-kind parallel: builder event kinds as compile-time extensions, closed
per-binary the way the config surface is declared per-binary. And the per-seam case:
each organ contract owes the config-refusal case on its own `lifecycle-refusal` set,
since the case set is settled per seam rather than in the floor.

**The identifier corrected on 2026-08-01, from `agent-state-file` to
`agent-config`, on the human's ruling.** The file is configuration, read at load
and fixed for the run, and the agent's state is the trace, so the old name
pointed at the wrong artifact. The node, its seven `holds` edges, and every
citation in the corpus moved in one act.

**The model binding carries the device assignment, per the human's ruling of
2026-08-03.** An operator declares which hardware a model runs on and the
program does not choose: placement is a fact the operator states in this file,
not a negotiation the program conducts at load. The binding therefore names
the artifact and the devices together, because a binding that named an
artifact alone would leave the placement to be decided somewhere, and every
somewhere is a crate reasoning about hardware the corpus has already ruled it
does not reason about. **The assignment is a set rather than one device,**
because tensor parallelism is in scope from the start and a model may be
sharded across the devices it is given. A set of one is the ordinary case and
carries no special shape.

What the set means to each reader is theirs: the operator states it, admin
validates that it is present and well-formed and never that the devices exist,
per `weaver-admin-PRD` section 2 and ruling C of 2026-07-31, and the SPU judges
the devices it was assigned at admission, per `weaver-spu-PRD` section 4.1.
Nothing selects.

**The field election joins the file 2026-08-21**, on `weaver-spu-PRD`
section 13.11. It is the operator's election of the probability field at a
declared depth, optional in the file because the election is what makes
it exist and its absence is the ordinary posture rather than a value
withheld. It sits beside `residual-readout-election` and is deliberately
not merged with it: each diagnostic election stands alone and none is
bundled under a name for a set, because a named set drifts as members
join it and every record already carrying the name becomes a record of
something else. What the depth means to the reader that judges it is the
SPU's, at admit, where it meets the sampling cutoff it may not fall
below.

**The surprisal election joins the file 2026-08-21**, on `weaver-spu-PRD`
section 13.12. It is the operator's election of the per-position surprisal
vector, a plain flag rather than a depth because there is nothing to size:
the reading exists per decode position or it does not. It sits beside the
field's election and the readout's, unbundled for the reason those two are
unbundled.

**It is the first election whose absence is the ordinary posture of a
reading the record already carried.** The other two elect an observation
into existence. This one elects one out, the surprisal vector having been
unconditional since the measurement was written, and what stands in its
place by default is the per-generation perplexity of section 13.12 rather
than nothing. The distinction matters to a reader of old records: a
measurement written before this act carries the vector with no election
beside it, and a measurement written after carries the flag whichever way
it fell.

**The binding kind joins the file 2026-08-24**, on `weaver-agent-PRD` section 6
as amended that date. It is the operator's declaration of what the load is for,
serving or diagnostic, and the components the load stands up follow from it: a
serving binding raises the whole interior and a diagnostic binding raises it
without Gate, for the replay the apex's section 8 places outside the agent.
Absence is the ordinary posture and means serving, so every declaration written
before this act declares what it always meant. It sits beside the elections and
is not one: an election turns an observation on or off inside a load whose
shape is settled, where the kind is the shape, read before any component the
kind governs exists. What it means to each reader is theirs, per this section's
standing form: the operator states it, admin validates presence and form and
carries it into the enter, and the harness stands up the components it
declares, per the contract. No organ reads it, and the SPU in particular
decodes the same under either kind, which is why it crosses at the directive's
top level rather than inside `spu-instruction`.

**The refusal record joins the wire vocabulary 2026-08-22**, per the
operator's ruling that a refusal is clerked. It names the seam that refused
and carries that seam's own typed case with the values the case holds, so
what reaches the record is the refusal itself rather than a rendering of it.

**It is defined here because the trace cannot define it.** `weaver-trace`
depends on no crate of this program, so a refusal typed in that crate would
oblige it to hold four seam vocabularies and version them as seams change.
Defining the record here puts the type where the seams' own refusals already
live, and the trace carries it opaque exactly as it carries `fault-report`.
**A consumer dispatches on this definition**, which is the whole reason the
record is typed rather than a reason field: nothing dispatches on prose it
must parse back into the type the sender declined to declare.

**It grows as seams add refusals, and that cost is accepted.** A seam that
gains a refusal case gains it here, in the same act, on the rule this
section already runs on for every wire term.

```graph
node: agent-config
kind: artifact

node: model-binding
kind: vocabulary

node: binding-kind
kind: vocabulary

node: tool-set
kind: vocabulary

node: permission-mode
kind: vocabulary

node: residual-readout-election
kind: vocabulary

node: gate-instruction
kind: vocabulary

node: trace-sink
kind: vocabulary

node: state-election
kind: vocabulary

node: field-election
kind: vocabulary

node: surprisal-election
kind: vocabulary

node: refusal-record
kind: vocabulary

edge: defines
from: weaver-types
to: model-binding

edge: defines
from: weaver-types
to: binding-kind

edge: defines
from: weaver-types
to: tool-set

edge: defines
from: weaver-types
to: permission-mode

edge: defines
from: weaver-types
to: residual-readout-election

edge: defines
from: weaver-types
to: field-election

edge: defines
from: weaver-types
to: surprisal-election

edge: defines
from: weaver-types
to: refusal-record

edge: defines
from: weaver-types
to: gate-instruction

edge: defines
from: weaver-types
to: trace-sink

edge: defines
from: weaver-types
to: state-election

edge: holds
from: agent-config
to: model-binding

edge: holds
from: agent-config
to: binding-kind

edge: holds
from: agent-config
to: tool-set

edge: holds
from: agent-config
to: permission-mode

edge: holds
from: agent-config
to: residual-readout-election

edge: holds
from: agent-config
to: gate-instruction

edge: holds
from: agent-config
to: trace-sink

edge: holds
from: agent-config
to: state-election

edge: elects
from: permission-mode
to: permission-mode-vocabulary

edge: elects
from: tool-set
to: tool-trait
```

The last two are the floor's own argument in edge form. `weaver-traits-PRD` section 2
rests the two-crate floor on one fact, that the config file names a permission mode and
a tool set whose definitions therefore cannot sit above the floor. Those are fields
here electing from vocabulary that crate defines, and without an edge the single
argument the floor's structure rests on is prose the graph cannot see.

Each field is a vocabulary node this crate defines and the artifact holds, because a
contract's clause draws a field rather than the whole file and an edge needs
something to point at.

**Which fields a contract draws, and why the rest are internal rather than owed.** Seven
are drawn: `model-binding`, `residual-readout-election`, `field-election`,
and `surprisal-election` by
`weaver-harness-spu-contract` and by `weaver-admin-harness-contract`,
`binding-kind` by `weaver-admin-harness-contract` alone as of 2026-08-24,
the seam it crosses and the only one, its consumer being the harness that
stands up what it declares,
`gate-instruction` by its three, `weaver-admin-harness-contract` joining the
two gate seams as of 2026-08-17, the seam it crosses first joining the seams
that consume it, and `state-election` by `weaver-admin-harness-contract` as
of 2026-08-19, the seam it crosses first and today the only seam that
carries it, its consumer being the tee the harness applies. The other three
are read from the artifact
rather than across a seam, which `weaver-admin-PRD` section 3 distinguishes in its own
words, a field read out of a file and a definition drawn by a contract being answerable
to different checks. `trace-sink` is consumed by admin at load, demanded into this
section by `weaver-admin-operator-contract` and named by the builder-extension ruling as
the un-freezable bound admin holds. `permission-mode` elects the floor's mode
vocabulary, which is the argument the two-crate floor rests on, and the harness reads
the mode. `tool-set` elects `tool-trait` by the same argument, and its element shape
arrives with the tool workflow.

**`field-election` was added to this file on 2026-08-21 and drawn by
neither contract until this act, which is the same defect one paragraph
below closed for the readout.** The rule that paragraph states is that a
field the SPU judges at admission is a field that reached it across a seam,
and section 2.1 says in its own words that the depth's judgment is the
SPU's at admit. So the election was declared here, held by the artifact,
judged at the far end of two seams, and drawn by neither, which is a
definition the graph shows nothing carrying. **A rule stated once is not a
rule applied**, and the act that added the next election read the
paragraph's conclusion about one node rather than its argument about any.
`surprisal-election` is drawn in the act that adds it, which is this one.

**`residual-readout-election` is drawn rather than internal, and an earlier wording of
this section had it both ways.** That wording listed the election among the fields read
from the artifact and said in the same breath that the SPU judges it at admission. The
two cannot both hold. The SPU reads no configuration file, by its own charter's
argument that a parser in a process whose claim is thinness is weight, so a field it
judges is a field that reached it across a seam or a field it never sees. It reached it
across none: the election was declared here, held by the artifact, and given no route,
which is the defect this act closes. Being judged at admission is what makes it drawn,
per apex section 4's definition of done and that Spec's admit test, and the route runs
through both seams the load crosses rather than one, admin to the harness and the
harness to the SPU.

**An organ's fields travel together, and the gate's already do.** `gate-instruction` is
one organ's declaration moving as one thing, along the path this section describes
below. The SPU's did not: its two fields sat loose beside the other four
with one of them crossing and one of them stranded, and the stranding above is what
that asymmetry produced rather than an accident beside it. What this section rules is
that an organ's fields are named together and cross together, so a field that organ
later needs joins the ones it belongs with instead of being threaded through every
directive that would have to carry it. How they are grouped is representation and the
Spec's, and this charter rules only that they are.

**The shape is what extends, and nothing is carried for a reader that does not exist.**
Apex section 9 draws that line and this section stays on its near side. A reserved slot
is a shape carried for a reader that does not exist, and every field named here has one
today. A section gains a field in the act that gives that field a reader, which is the
apex's own mechanism for statefulness read at a smaller scale: a feature add with its
schema extension and its contract amendment, never a retrofit. What the uniform shape
buys is that such an act amends one contract and one organ's declaration rather than
reshaping what carries them. Room made in advance and a shape that admits growth are
different things, and only the second is ruled here.

**The definitions and their coding are sequenced separately, per the operator's ruling
of 2026-08-04.** The graph maps the documented set whole, and material whose coding
follows loop 0, the readout above all, is roadmap the graph carries rather than a
commitment the first build answers for. What the first coding run builds is scoped
against the graph after it exists, and the per-binary registration of section 2.1
already makes the config surface honest at every stage: a binary that does not
register a field does not carry it. The ten field records above are the ten fields
this section lists, so a clause naming an eleventh has no target and the mapping says
so. The block above carries twelve node records and not ten, the two that are not fields
being `agent-config`, the artifact the fields are held by, and `refusal-record`,
which is wire vocabulary this section argues beside them.

There is no `writes` edge from any crate, because the writer is the operator and the
operator is not a node the graph carries. Both crates that touch the file declare
`reads`, each in its own charter, and section 6 rules the once-deferred contract out
rather than deferring it.

Known fields, from the passes already done: the model binding, the tool set, the
permission mode, the residual-readout election, the gate instruction, and the trace
sink. Four joined after this sentence was written, the binding kind, the state
election, the field election and the surprisal election, each named where this
section argues it, so the set this section lists is ten today and this sentence is
the six it stood at. The last three of those six arrived from the gate and
operator-contract passes and from the trace pass rather than from this crate's own
reasoning, which is the demand rule working as intended. The gate instruction names
the seams the gate holds, two as of the egress ruling of 2026-08-07, and it travels
the way the model binding does: the operator writes it, admin validates it, the
harness carries it uninterpreted, and the gate resolves it, per `weaver-gate-PRD`
section 10. The trace sink names where the stream lands, per
`weaver-admin-operator-contract` section 3: the operator writes it, admin validates it
and connects the stream to it at load, and no other crate reads it.

Two of those fields are named in `weaver-harness-trace-contract`'s vocabulary clause,
so the config file is already a declared dependency of a contract this crate is not
party to. That is the normal shape: this crate supplies vocabulary that agreements
between other crates are written in.

**The file's reader is a human writing it, and that is a requirement rather than a
preference.** The operator hand-authors this declaration, often on a box under load and
often while diagnosing why a load refused, and the fields it carries nest by nature: a
binding with its settings, a set that is a list, elections, an instruction, a sink. So
the format this crate elects for it answers to a writer rather than to a parser, carries
the declaration's nesting, and survives the comments an operator leaves for the next
reader. **The parser's surface in the binary that runs as root is part of the
criterion**, on the operator's ruling of 2026-09-28: admin parses this file with root's
authority, so a parser that is unsafe code, or C translated into Rust, is surface the
writer's convenience does not buy, and ceremony in the file is accepted where that
safety is bought. That criterion is stated here because it is a fact about who uses the
artifact and who reads it, and the Spec elects against it rather than inventing the
ground it elects on.

**A field takes effect at load and is fixed for the life of the run.** A session
whose configuration changed mid-run is one every consumer afterward has to reason
about, and the lifecycle already provides the place to change it. Unload, edit, load.

**Every field here is run-scoped, and none is sealed.** A run is a load, so this file
is read at every load and the value it carries governs that run and nothing wider. An
operator editing between runs of a live session is using the mechanism the lifecycle
provides rather than working around one. A consumer needing to know how a given run
was configured reads what that run produced: the model identity and the sampler
parameters ride `model.measurement`, and the residual election is legible from the
presence or absence of the reductions. That is the elections-govern-production
rule read from the reader's end, and it is narrower than a claim that the run's
conditions are authored somewhere, which no mechanism does since the ruling of
2026-08-02.

Run scope is what this crate declares. Whether a particular consumer wants a
particular field held steady across the runs of one session is that consumer's
concern, argued in its own charter against its own harm, and it does not become a
property of this file by being wanted.

An earlier version of this section made some fields session-scoped and sealed at
the session's first run, naming a recording level as the instance. That scope
is withdrawn, and the
level itself left the file entirely under the ruling of 2026-08-02. The seal also
left the residual-readout election, which was never sealed, able to enable readout
against a session whose recording level licensed no home for the reductions. Both
interactions close with the scopes that created them.

### 2.2 Peer identity, and one bounded policy carve-out

The seams that admit an outside principal and the seams that cross the membrane are
named `SO_PEERCRED` Unix sockets, so a process accepting a connection on one of them
must establish who is on the other end. The identity type lives here because more than
one process kind needs it and none of them may depend on another to get it.

**The claim is scoped to those seams and was once stated of all of them.** The earlier
wording made every seam in the program a credentialed socket, and
`weaver-admin-harness-contract` falsifies it: an inherited unnamed pair reports the
creating process at both ends, distinguishes nothing, and authenticates by possession
instead. That contract draws neither definition below and says so. A universal with a
counterexample is a weaker seat than a scoped claim that holds, and the scoped claim is
what the carve-out rests on.

**The scoped claim has its consumers named as of 2026-08-01, and they are re-aimed by
the recut of 2026-08-05.** The client socket of `weaver-gate-PRD` section 2 is the seam
that admits an outside principal, and it is unchanged. The second consumer was the
operator surface, which retired with the service account it authenticated to, and the
coordination seam of `weaver-admin-harness-contract` section 2 takes its place: the
socket inversion gave that seam a name and a real credential check, root or refused,
where an inherited pair could distinguish nothing. Every surviving consumer reads
`SO_PEERCRED`, judges by the one shared rule, and draws the pair below. **Reads rather
than is authenticated by, on the third seam**: where a consumer accepts, the credential
is the connecting peer's own and the rule decides, and where a consumer dials, the
credential is the one captured for the listening socket and cannot by itself say which
application answered. The gate's agent-opened seam is the case, per `weaver-gate-PRD`
section 2, and what closes it is the tool-seam contract stating how a registered tool
proves itself and how that proof maps to its registration. This crate carries the rule
and not that proof. The claim is stronger for the exchange rather than weaker, the seam
that left having been the one whose predicate a compromised group grant could widen.
**The count was two and is not closed at two**: the egress ruling of 2026-08-07 charters
the gate's agent-opened seam as a third, and what credential a registered tool presents
on it is the tool-seam contract's, so this subsection states the shared rule and leaves
the enumeration open rather than naming a consumer whose contract does not exist.
**Admin's trace relay reads `peer-identity` from 2026-10-03**, on the operator's rulings
on #50, and judges it against the one reader the agent's boundary file declares rather
than by the shared allow-and-deny rule, so it draws the identity type and not the
predicate, and `weaver-types-Spec` section 3.1 carries its request shape.

**The trace door's stream is one vocabulary record, `trace-stream`**, defined here on
the same rulings: the request a reader sends, the header the relay answers with, and the
control lines it interleaves with the record's own lines, `TraceRequest`, `TraceHeader`,
`TraceLine` and the `TraceControl` it wraps, of `weaver-types-Spec` section 3.1, are its
satellites rather than four records, the way the token trio is carried, because no party
draws one without the others. `weaver-admin-operator-contract` draws it.

**Alongside the identity type, this crate carries the authorization predicate, and
that is a deliberate exception to holding only data.** The rule that decides whether a
given peer may reach a given agent is enforced independently by more than one process,
and one shared definition in a floor crate is the only way separate processes provably
enforce the same rule. Three implementations of one rule are three chances to disagree,
and the disagreement is a privilege boundary.

**The carve-out is bounded and the boundary is the important half.** This crate holds
the identity type and the predicate, and nothing else. Where the rule is applied, how
a failure is handled, how an access list is loaded, and what happens on refusal all
live in the consuming crates. **A second policy function arriving here is a charter
violation to be flagged rather than absorbed**, because the argument above justifies
exactly one exception and does not generalize into a licence.

```graph
node: peer-identity
kind: vocabulary

node: authorization-predicate
kind: vocabulary

edge: defines
from: weaver-types
to: peer-identity

node: trace-stream
kind: vocabulary

edge: defines
from: weaver-types
to: trace-stream

edge: defines
from: weaver-types
to: authorization-predicate
```

### 2.3 The wire vocabulary the socket contracts draw

**Everything here arrived on demand and not in advance.** Section 4 held wire
protocol vocabulary out of this crate until a written contract needed it.
`weaver-admin-harness-contract` section 8 was the first, arriving with four,
`weaver-harness-spu-decode-contract` section 7 the second, arriving with the
token trio on 2026-08-02, `weaver-harness-gate-contract` section 7 the
third, arriving with `turn-frame` and `fault-report` in the same workflow's
gate act, that same clause again the fourth, demanding `tool-name` in the tool
workflow's opening act of 2026-08-17 and having it land here with this act, and
`weaver-harness-spu-classify-contract` section 7 the fifth, arriving with the
label trio on 2026-08-19. Nothing
else enters this subsection until another contract draws it, a definition
arriving without a contract behind it being the reserved-slot error apex section
9 forbids, in schema form. `harness-alert` was
drawn here until the fault-carrier ruling of 2026-08-01 retired the alert exchange,
the fault travelling as the `fault` event kind `weaver-trace-PRD` section 3.1
defines, and a definition no contract draws leaves the floor by G4's own test.

**Three of the four are loop 0's trio, named for the loop and not for a sender or
a seam, per the human's naming ruling of 2026-08-01.** Wire vocabulary is named
for the loop whose traffic it carries: the channels have two initiators, so direction
is a fact about a loop's walk rather than about a name, and loops are unique where
senders are not, which is what retired the sender convention after it collided on
the harness. `lifecycle-directive` is the ask of the load/unload path, entering
at admin's invocation and fanning out along the harness's seams.
`lifecycle-answer` is its answer, aggregating back. `lifecycle-refusal` is the
typed refusal, carrying why a lifecycle act could not be performed rather than a
free string. Each closed case set lives here and every loop 0 contract draws the
cases that cross its own seam: enter, leave, and stop at coordination, admit and
release at residency, raise and lower at the gate. The verbs and observations
entered at the operator surface until 2026-08-05 and now arrive as an
invocation's arguments, so they are the charter's rather than a contract's, and
the drawers number three. One owner, no drift, which is the shape
`lifecycle-refusal` already proved.

**The token trio is named for the seam's currency, under the naming ruling's
ratified extension of 2026-08-02.** `token-directive`, `token-answer`, and
`token-refusal` carry the decode seam's traffic, and that seam's loop is loop
1, the builder's and variable since the composability ruling, so a loop name
is exactly what the seam cannot take and the ruling gained its second case:
vocabulary for a seam whose loop varies is named for the seam's currency.
The cases are the decode contract's enumeration, one owner and one drawer
today, and the session and turn identities ride inside the cases as the
floor's satellite types rather than as vocabulary of their own. The
representation is the token workflow's, elected with the hot-path
measurement, per the criterion below.

**The label trio arrives on the classify contract's demand, under the same
currency case.** `label-directive`, `label-answer`, and `label-refusal`
carry the label seam's traffic, per `weaver-harness-spu-classify-contract`
section 7, drawn 2026-08-19 with the classifier's charter: that seam's loop
is loop 1 and variable, so the trio is named for the currency, the label.
The cases are the classify contract's enumeration, one owner and one drawer
today, and the turn identity rides inside the cases as the floor's
satellite type, the way the token trio already carries it. The traffic is
one ask and one whole answer per the asking loop's election, low in volume
and diagnostic in audience like loop 0's rather than like the decode
seam's, and the Spec elects its encoding against that criterion beside the
shapes it holds.

**The turn frame is one definition for two directions, and its opacity is the
point.** `turn-frame` arrives on `weaver-harness-gate-contract` section 7's
demand with the token workflow's gate act of 2026-08-02, named for the seam's
currency under the naming ruling's ratified extension. It carries a client's
line inward and the turn's answer outward, unread by the crate that relays it
in either direction, so one definition serves both: a frame is what crossed,
and which way it was going is the exchange's position rather than the value's
type. A refused turn rides inside it as content the harness authored, which is
why no refusal case is added for it here. What a frame's octets mean is the
harness's, per `weaver-gate-PRD` section 3, and what they look like on the
wire is the Spec's, against a constraint that subsection states rather than
waves at.

**The fault report is the carriage a reporting organ needs, and it belongs to
no loop either.** `fault-report` arrives with the gate act on the same
demand-driven terms, because the gate raises faults across the organ channel
and the channel's payload set was closed before a gate fault existed. It is
one definition rather than one per seam: what an organ hands the harness when
something it survived has to reach the record is the same fact whichever
socket carries it, and the harness authors all of them into the one `fault`
event kind `weaver-trace-PRD` section 3.1 defines. So both reporting seams
draw it, the gate's inside the envelope and the decode socket's inside its
own trio, which is the one-owner-many-drawers shape this subsection already
runs on. The case set it carries is the organs' enumerations, closed on
2026-08-02, and its shape is the trace act's to elect against them.

**The tool name arrives with the tool workflow.** `tool-name` arrives on
`weaver-harness-gate-contract` section 7's demand with that workflow's opening act
of 2026-08-17, carrying across the execution exchange the name the model spoke for
the tool it called, and it is named for the seam's currency under the same extension
the turn frame is named under. It is this crate's for the reason everything in this
subsection is this crate's: it crosses a socket, so it is a representation two
processes must agree on, and what a tool itself is stays with `weaver-traits` by the
floor's own division. Where the comparison of a name that crossed against what a
tool answers lives is `weaver-traits-Spec` section 5's to state, and this charter
cites it rather than carrying a second copy, per G5.

**One of this subsection's definitions belongs to the floor and not to any loop.**
`organ-envelope` is the
carrier every organ channel draws, holding the exchange a message belongs to, that
message's position in the exchange, and the type of its payload. It is defined here
because the coordination seam was the first channel to need it, and it is the one value
in this subsection no later contract draws as new, because apex section 5.4 makes a
two-initiator channel with the harness the test of an organ and every such channel
carries this envelope. The mechanics this record serves live in `weaver-organ-channel`
as of the lift of 2026-07-31, and that document declares no records of its own, so this
record stays here with the others and each organ contract draws it in its own vocabulary
clause. The decode socket is not an organ channel, per `weaver-spu-PRD` section 13.2, so
the every-channel sentence above stays scoped and the envelope does not cross that seam.

**Loop 0's traffic is low in volume and diagnostic in audience, which is the
criterion its encoding answers to.** A residency carries a handful of these
messages, twice per run and once per stop, so the latency doctrine has nothing to
bite on here and compactness buys nothing measurable. What the traffic is for,
when it matters, is telling an operator why a load refused for a reason nobody
expected, which is read from a capture or a strace rather than from a decoder.
The Spec elects an encoding against that criterion. Decode traffic is the
opposite case on both counts and its encoding is the token workflow's, with a
measurement behind it.

The meaning of each, its ordering against the others, and the failure modes are the
contract's and are not restated here. This crate holds the representation the two
processes must agree on, which is the whole of what a floor definition is.

```graph
node: organ-envelope
kind: vocabulary

node: lifecycle-directive
kind: vocabulary

node: lifecycle-answer
kind: vocabulary

node: lifecycle-refusal
kind: vocabulary

edge: defines
from: weaver-types
to: organ-envelope

edge: defines
from: weaver-types
to: lifecycle-directive

edge: defines
from: weaver-types
to: lifecycle-answer

edge: defines
from: weaver-types
to: lifecycle-refusal

node: token-directive
kind: vocabulary

node: token-answer
kind: vocabulary

node: token-refusal
kind: vocabulary

edge: defines
from: weaver-types
to: token-directive

edge: defines
from: weaver-types
to: token-answer

edge: defines
from: weaver-types
to: token-refusal

node: turn-frame
kind: vocabulary

node: fault-report
kind: vocabulary

edge: defines
from: weaver-types
to: turn-frame

edge: defines
from: weaver-types
to: fault-report

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

node: tool-name
kind: vocabulary

edge: defines
from: weaver-types
to: tool-name
```

## 3. What it must not hold

**No logic beyond the section 2.2 predicate.** The carve-out is one rule with one
justification, not a category.

**No internal dependency other than `weaver-traits`.** The config file names a
permission mode and a tool set, which is why that edge exists and why the floor is two
crates rather than one.

**No socket, no process, no handle.** The definitions describe what crosses a boundary.
Crossing it is somebody else's crate.

**Nothing one crate needs.** A shape only the harness reads belongs in the harness.

**No anticipatory schema.** A field added because something will probably want it is a
reserved slot in data form, which apex section 9 forbids as plainly as the interface
kind. The residual election is here because a completed pass demanded it, not
because a configuration file felt incomplete without it.

## 4. What does not cross

**The task schema.** The previous tree carried a task work-spec with benchmark, chat,
chatroom, and training-loop kinds. This program has no task surface: work enters
through the gate as turns, and the apex is explicit that no prompt, turn, task, or run
enters through the administrative plane. The previous tree's own issue #350 records
that its agent worker implemented no task executor, so the schema described a
capability that was never built.

**Per-agent path constants.** That tree shared root-custodied path constants between
producers and consumers. Under the custody model the harness never resolves a path,
and `weaver-admin` holds the paths it opens, so there is nothing for a shared constant
to coordinate. If the admin pass finds a path two crates must agree on, it enters then.

**Wire protocol vocabulary was held here and has left.** It was absent on demand
rather than excluded on principle, and the demand has fired.
`weaver-admin-harness-contract` is the first socket contract, it names five values
that cross, and their shared representation is now section 2.3. Nothing was guessed:
each of the five arrived because a written agreement needed it. The rule that placed
the paragraph here still governs everything not yet drawn.

## 5. The terms of participation

**Declare what you use.** Every contract names what it draws from here, and the clause
is present even when the answer is nothing. The union of those clauses is this crate's
required surface: a definition no clause names is unused, and a definition a clause
names and this crate lacks is a gap.

**The producer and the consumers are all bound.** The agent config has one writer,
the operator, who stands outside the program, and two readers inside it, admin
validating before a process exists and the harness consuming the elections it
carries. The format is an obligation on all three. A producer that emits a shape a
consumer does not accept, or a consumer that tolerates a shape the producer never
emits, are the same defect seen from two ends, and two readers that disagree about
one file is that defect a third way.

**Compatibility posture is stated, not assumed.** A field added, removed, or given a
new meaning is a change to every producer and consumer of the file, and each is named
whether or not it lands in the same act.
Absence is never read as a default unless the charter says that field is optional and
says what its absence means.

**A change here is loud by design.** Everything above links this crate. The ritual
carries it: a floor change names every PRD and contract it affects, and what does
not land with it is named as owed in the register that tracks it. An earlier form
required one act and is retired per the ruling of 2026-08-23, the naming being what
it was protecting.

## 6. The seams

This crate is party to no contract. It defines and does nothing, so there is nothing
to agree to, and it is **named in** contracts rather than signing them.

The agent config is a real producer-to-consumer agreement, but its producer is the
operator and its parties are not two crates. An earlier form of this paragraph
deferred a contract between `weaver-admin` and the harness to the admin pass, and that
pass ruled the other way, in `weaver-admin-PRD` section 10: authorship moved to the
operator, both crates that touch the file read it, so there is no producer inside the
program and no producer-consumer agreement between two crates to write. Only seams
take contracts, per Working Process section 4. This charter is the named authority on
the format under G5, each reading charter carries its own validation obligations, and
no third document exists.
