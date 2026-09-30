# weaver-analysis - Spec

**Status:** MERGED. Cut 2026-08-27, the second Spec of the diagnostic leg. Code is
written against it under the gates of Working Process section 6.

**Date filed:** 2026-08-27
**Document ID:** `weaver-analysis-Spec`
**Parent:** `weaver-analysis-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #731

---

## 0. What this document is

The representation of the driver `weaver-analysis-PRD` charters: how it parses a
finished record, what it projects across the preload seam, and how it reads the
record a replay makes. It is written against that charter, against
`weaver-analysis-state-contract`, against `weaver-analysis-web-contract` for the
shape the signals reader's summary crosses in, and against `weaver-diagnostic-Spec`
for the record it reads, and it develops no rationale of its own.

**What the charter left here is named there.** Section 4's closing cell owes this
document the driver's shape and the parser's, and says the certification's
mechanics are not among them: that comparison belongs to the loop inside the run,
and what this document settles about it is only how this crate elects a null
replay and reads the outcome.

**One cell it owed closed before this document opened.** How a diagnostic-trace
says it ended settled in `weaver-diagnostic-Spec` section 3.3 on 2026-08-27, which
is what makes section 5's gate writable at all: a driver that could not tell a
finished record from a truncated one could hold this shape and not use it.

## 1. The crate

**Layout.** One module per obligation, re-exported at the root.

    src/main.rs       the invocation's composition root, and nothing else
    src/lib.rs        re-exports, and nothing else
    src/record.rs     the parse of a serving record, section 2
    src/project.rs    the election and the projection, section 3
    src/selection.rs  the recorded rule and preflight, sections 3 and 4
    src/declare.rs    the declaration derived from the record, section 3
    src/preload.rs    the preload seam's sender, section 4
    src/reading.rs    the diagnostic-trace's parse and the gate, section 5
    src/lens.rs       the lens artifact, loaded and applied, section 5
    src/capture.rs    a capture's columns and their comparison, section 5
    src/field.rs      a position's field, read from the record, section 5
    src/stream.rs     the drain, one road under every reader, section 5
    src/signals.rs    the per-position series, read on the drain, section 5
    src/deposit.rs    the explicitly named device and code identity, section 5

Fourteen rows for the fourteen files the tree holds: `src/lib.rs` and the twelve
modules it declares beside itself, thirteen library files in all, and `src/main.rs`
as the one bin target. The listing names existing files, per the convention
`weaver-harness-Spec` section 1 states. Sections 3 and 4 argue the selection and
preflight, and section 5 argues the deposit reader.

**Edition and toolchain.** Edition 2024 on the pinned nightly, no nightly feature
used.

**The dependency set is five crates and no internal one.** `serde` with `derive`, and
`serde_json` with `raw_value`, which sections 2 and 3 elect for carrying a payload's
elected values as the record spelled them rather than re-encoding them, `safetensors`,
which section 5's reading elects to open the lens artifact and the weights its
unembedding needs, `sha2`, which the same section's identity check elects to recompute
the digest a manifest names, and `toml`, which section 3's derivation elects to write
the declaration the operator loads. **Neither of the first two additions is an engine**:
one maps a file and answers tensors, the other answers a digest, so what enters this
crate is a parser for the container section 3 elected and the arithmetic that checks an
identity, with no inference runtime - which is the whole reason that election named a
format both sides of the boundary can read. `safetensors` is the crate `weaver-spu`
links for the native backend's weights and `sha2` already stands in this workspace's
resolved tree, both vetted here rather than newly admitted. **The digest is not
hand-rolled**: an identity check written here would be this crate's approximation of a
standard, and a wrong one would refuse good artifacts or admit bad ones. **The
declaration is not hand-written either**: `toml` is the crate `weaver-types` parses the
declaration with, taken here with its writer and without its parser, so the derivation
builds a typed value, the crate writes every string's escaping, and no member crosses as
hand-written text. Its tests take the parser to read a derived declaration back.

**No `weaver-*` dependency at all, and the negative is the boundary in the
manifest.** This crate stands outside the agent, per the charter's section 1, and
both of its seams draw their whole vocabulary from documents rather than from
types: `weaver-analysis-state-contract` draws the election and the distillate from
`weaver-harness-state-contract` and the event names from `weaver-trace`, and every
one of those crosses this crate's wire as JSON the record already spells.
`weaver-analysis-web-contract` draws the same way from papers of its own, one of
them `weaver-types-Spec` section 4.4 for the resident count, and **a draw from a
floor crate's paper is a draw from a document**: what crosses is the number the
record spells and never that crate's type, which is why the floor draw is an
instance of this claim rather than the exception to it. Linking any of them would
make an outside consumer a compile-time dependent of the agent's interior, which is
the coupling the boundary exists to prevent, and would buy nothing: the parse shares
no code with the writer by the charter's own election.

```graph
node: analysis-no-internal-dependency
kind: assertion
tag: manifest

edge: asserts
from: weaver-analysis
to: analysis-no-internal-dependency

edge: grounds
from: analysis-no-internal-dependency
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**No async runtime and no socket crate in the resolved tree**, the floor Specs'
build-time `cargo tree` assertion read here for a different reason: this crate dials one
Unix socket and binds none, so the standard library's own client is the whole of what it
needs. **The different reason is why this record carries no `grounds` edge where the
floor's and `weaver-diagnostic-Spec` section 1's do.** Theirs hold because a socket
crate would be a capability those crates must not have, and this crate has it and uses
it, dialing the state port of section 3. What is left here is a dependency election over
which client does the dialing, and an election between two ways of doing the same
permitted thing reads the same under every invariant.

```graph
node: analysis-no-runtime-no-socket-crate
kind: assertion
tag: manifest

edge: asserts
from: weaver-analysis
to: analysis-no-runtime-no-socket-crate
```

**No crate in this workspace depends on this one, of any kind**, on the operator's
condition of 2026-09-26 for this crate staying in the repository when `weaver-web`
left it. The claim above keeps the interior out of this crate. This one keeps this
crate out of the interior, so it stays a leaf that can leave the tree the way the web
did without any member's build changing. A normal, build or dev dependency on it,
direct or renamed, target-qualified or optional, is the dependency the condition
forbids, and the instrument reads cargo's declared dependencies of every workspace
member rather than a manifest's text so every one of those routes is seen. **It reads
every `crates/*/Cargo.toml` against the member set** as well, so a crate standing
under `crates/` outside the workspace is a refusal rather than a member the read never
saw. The record carries no `grounds` edge: the condition is the operator's for where
this crate lives, not an invariant of the agent.

```graph
node: analysis-no-crate-depends-on-it
kind: assertion
tag: manifest

edge: asserts
from: weaver-analysis
to: analysis-no-crate-depends-on-it
```

**It binds no listening port and holds no server.** The charter's section 2 has
this crate governing nothing inside the agent, with the harness holding no channel
to it and no behavior conditioned on its presence, and the absence of a listener is
what makes that true rather than merely asserted: a driver that listened would be
reachable, and a consumer the agent cannot reach is the structural claim this
crate's position rests on.

**The instrument is review, and what the manifest reaches is named beside it so
the two are not confused**, on the reason its sibling's compile-fail set gives: a
compile-fail doctest buys an absence from a crate's own surface,
naming a call that crate does not offer, and binding a listener is `std`'s call
rather than this crate's, so no doctest can refuse it. What the manifest reads is
that no socket crate stands in the resolved tree, which is the same assertion above
read for this claim's sake, and the residue - that this crate's own code calls
`bind` nowhere - is review's, named here rather than claimed as bought.

```graph
node: analysis-binds-no-port
kind: assertion
tag: review

edge: asserts
from: weaver-analysis
to: analysis-binds-no-port
```

## 2. The parse

**The read types are this crate's own and share no code with the writer.** The
charter's section 3 elects that separation and calls it the boundary working as
intended, so this crate spells the envelope and the payload members it reads rather
than importing them. **`weaver-trace-Spec` section 3 is authoritative for every
shape spelled here**, and a divergence is a defect against it per G5, which is the
same arrangement the charter states from its side.

**The parse is envelope-first and payload-lazy.** Every line yields its envelope
eagerly, that being what the projection groups and orders by, and a payload is held
as raw text until a key path is read out of it. That is what lets an elected value
cross this crate byte-identical to what the record spelled, per section 3, and it
is the same reason `weaver-trace`'s tee holds a payload as raw text on the other
side of the same wire.

**Semantic readers skip unknown kinds and members.** Unknown content cannot
become a request or measurement, decide diagnostic pairing, or influence an
interpretation that does not read it. The record carries no version marker,
per `weaver-trace-PRD` section 6. This is the existing semantic-skip claim.

```graph
node: analysis-parse-skips-the-unknown
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-parse-skips-the-unknown
```

**Raw reconstruction retains unknown material.** The envelope-first parse keeps
each canonical event and its raw payload for election-driven reconstruction.
An all-kinds rule admits a future kind's envelope, and an elected unknown path
crosses verbatim. This new obligation belongs to
`analysis-reconstruction-follows-recorded-election` in section 3, not to the
existing semantic-skip instrument. The charter's section 4 makes the same
distinction.

**A member a record does not carry is absent and is never derived from the members
beside it.** That is the harder direction of the same rule, per the charter's
section 4 and `weaver-trace-PRD` section 6, and this crate is where it costs
something: a record written before the layer and forward counts existed omits them,
and deriving a layer count from the length of a norm array is exactly the
arithmetic those counts were added to retire. **A replay over such a record is a
replay whose layer count is unknown rather than one whose layer count is guessed**,
and this crate carries the unknown forward rather than closing it.

```graph
node: analysis-derives-no-absent-member
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-derives-no-absent-member
```

## 3. The election and the projection

**Ordinary preload reconstructs under the recorded election.** It reads each
included run's governing `load.tee` under the representation defined by
`weaver-trace-Spec` section 3 and independently applies that rule to the canonical
record. Required members are not supplied by defaults. Missing or malformed
evidence refuses before the opener, as section 4 requires. Projection includes
unknown elected kinds and paths and the standing turnless-system exception from
`weaver-trace-Spec` section 11, independently of the tee implementation. No new
writer dependency is introduced. Recorded-rule selection, including the effective
rule comparison of section 4, is watched by `tests/driver.rs`, separately from the
fixed-election instrument below.

```graph
node: analysis-reconstruction-follows-recorded-election
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-reconstruction-follows-recorded-election
```

**Diagnostic preload explicitly selects this crate's election.** Its election is
composed from what `diagnostic-replay-loop` reads: run and turn envelopes, each rendered
contribution and its identity, and the recorded measurements that certification
compares. The paragraphs below define that diagnostic election alone. It is not the
default reconstruction rule, and selecting it requires a different destination session
under section 4.

**The ceiling is the loop's reading and not this crate's judgment of size.** What
bounds the election from above is that a kind `diagnostic-replay-loop` never reads,
in its walk of section 2 or its certification of section 3, is a kind whose
holdings nothing would ask for. So the rule is checkable against that document
rather than against a preference of this one, and a step added there widens this
election in the act that adds it.

**`load` is elected, and naming it matters because the identity does not come from
the holdings alone.** Step one establishes the five re-feed items and the template
from the events it walks, and takes **the tee's election from the record's `load`
event**, which that step says outright it reads there and never from the holdings:
the holdings are what that rule produced, so recovering it from them would be
reading a projection to learn what did the projecting. Since this crate's election
decides what reaches the holdings at all, an election omitting `load` would land a
session whose certification cannot check the rule that built it, which is the failure
`weaver-agents-PRD` section 8 added the criterion to prevent. **The source's resident
edits, a fault's case and the recall are elected too**, as of 2026-09-26: `flush` with
`resident_before` and `resident_after`, and `elision` with `from`, `to` and both counts,
because `diagnostic-replay-loop` section 2's walk reproduces each where it fell and
holds the reproduction to them. `fault` is elected with its `case`, because the identity
step refuses a source whose open recorded `identity_prefix_unrecorded`. `recall` is
elected with `ask`, `returned` and `count`, bounded to identities, so the destination
carries what the source's post-flush input was drawn from and the identity step can
require a recall between each flush and the next request.

**This document is authoritative for the election's content**, per G5, and
`diagnostic-replay-loop` section 2's step 2 sketches it for a reader walking the
three acts rather than fixing it. A divergence there is a defect against this
section.

**The declared session is the destination the receiving load names**, per the
contract's section 2. Whole-record reconstruction retains the source name unless the
operator names a branch. A cut requires a nonempty source-distinct destination
before the opener, per section 4. Diagnostic projection requires a nonempty distinct
destination, and declaration derivation uses that same explicit name. Source run,
turn, and sequence remain recorded facts.

**The election names what the harness's open reads as well, as of 2026-09-06**, per
`weaver-state-PRD` section 4 and `weaver-harness-Spec` section 2: a session standing
from a preloaded record asks the store for its seated prefix and its conversation, the
`identity` and `recall` asks, and rebuilds each
message from
the `role` and `content` the distillate carried. So the election names the four
message kinds the store's `recall` serves, `message.system`, `message.user`,
`message.assistant`, and `message.tool_result`, each with `role` and `content`, beside
the three the replay reads. **A restored message crosses whole instead**, as of
2026-09-26 (#697): it is always turnless, the rule the tee applies to the identity takes
it first, and an entry for it would never be read, so the store's `recall` serves it
beside the four without the election naming it. The ceiling widens with the asker rather
than moving from the rule: what bounds the election from above is what any party asks of
the preloaded holdings, and the harness's open is the second such party after the replay
loop. The diagnostic loop reads past those four kinds untouched. Ordinary reconstruction
uses the recorded rule even when that rule does not carry the same conversation pairs.
The driver never enriches a reconstruction to make it resemble a diagnostic projection.

**The existing fixed-election instrument checks that the stream follows the
analysis election.** It watches the diagnostic projection's selected kinds and
paths. The recorded-rule mode, turnless-system exception, and preflight refusals
have separate instruments in `tests/driver.rs`, listed in section 6.

```graph
node: analysis-election-declares-what-follows
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-election-declares-what-follows
```

**A distillate is the envelope whole and the elected pairs beside it, each value as
the record spelled it.** The shape is `weaver-harness-state-contract`'s, drawn by
this crate's own contract rather than restated, and the projection reaches it by
splicing raw payload text rather than re-encoding a parsed value. **That is what
makes the preload's indistinguishability claim true rather than approximate**: a
number the record spelled one way cannot reach the holdings spelled another, so a
holding cannot say by its own bytes whether a tee or this crate landed it.

```graph
node: analysis-projection-splices-verbatim
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-projection-splices-verbatim
```

**Every distillate carries all five envelope fields, in the record's sequence
order**, per the contract's section 3. An unattributable distillate is a defect in
this sender, and an out-of-order one is worse: the loop's pairing runs in landing
order, so a stream that reordered would pair a request with a measurement from
another generation and the replay would read a turn that never happened.

```graph
node: analysis-sequence-order-preserved
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-sequence-order-preserved
```

**The declaration is the third projection, and the record is its one source for every
source-run fact**, per the charter's section 3 as amended on issue #394. The derivation
reads members the record already spells and writes the declaration the operator loads,
so the diagnostic run is correct to the run and never to the analyst's memory. Each
derived member names its source: the source session from the envelope, the artifact from
`model.measurement`'s `model`, the seated identity prefix from the turnless
`message.system` events at the run's opening in landing order with each payload carried
value for value, every string and number the value the record carries, decoded from its
JSON and written by `toml` as the declaration's TOML, per `weaver-types-Spec` section 2,
which since the ruling of 2026-09-04 is the seed the derived declaration carries while
the preloaded store answers the replayed run's `identity` ask, the two agreeing by
construction because both are the same record's events, the seed from `model.request`'s
`sampling.seed`, the per-turn ceiling from that request's `stop.max_tokens`, and the
context capacity from `model.output`'s `capacity`. **A member the record spells two ways
refuses the derivation naming the member**, disagreement being a question for the
operator and never a pick, **a derived member the record does not carry refuses the same
way** rather than defaulting, and **a member the record carries and the declaration
cannot refuses the same way**: a `null`, which TOML has no spelling for, and a number
past the declaration's integer, a seed past `i64::MAX` among them per
`determinism-matrix-Spec` section 4. Completeness is claim-relative here exactly as it
is at input identity, and the claim is the whole declaration. The rule reaches the
derived members alone - the fixed and analyst-supplied members below come from no record
and refuse on no absence.

```graph
node: analysis-declaration-derives-from-the-record
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-declaration-derives-from-the-record
```

**Four members are the analyst's inputs and three take fixed values.**
Device placement, the readers' elections, the diagnostic sink, and the distinct
diagnostic destination arrive from the invocation, per the charter's four exceptions.
**The sink's
input carries its shape and not only its name**: the charter has this
crate assume no discriminant, so the analyst who elects a pipe is electing
that the run retains nothing and the reading is taken as the stream
drains, and the analyst who elects a file is declining that licence and
keeping a capture. A derivation that could only write one shape would
make that election the crate's rather than the operator's. `binding-kind` is
`diagnostic` by construction, `tool-set` takes the empty list, and
`permission-mode` takes `ask` - the fixed values for members the record
does not carry and a run under this binding never reads, stated here so
the derivation writes a spelling rather than a guess.

**`derive` requires `--as <session>` for the diagnostic destination.** The name is
nonempty and differs from the one source session whose evidence is being derived. The
declaration's session is this destination, while every source-run input retains its
record-derived value. The same destination must be passed to diagnostic preload. Neither
command defaults the destination to the source name. Invalid names refuse before a
declaration is written or a preload is opened.

```graph
node: analysis-derive-uses-explicit-destination
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-derive-uses-explicit-destination
```

**The lens artifact's representation, per the charter's clauses of this
date.** The matrices are stored as safetensors, one tensor per source layer
named by its index, f32 - elected over the fitting tool's own torch
serialization because both sides of the boundary must read the artifact:
the fitting runs under the reference implementation and this crate reads
the artifact without it, and a pickle-bearing format would put the fitting
tool's runtime inside this crate's parse. The manifest is JSON beside the
matrices: `lens-manifest{-tag}.json` beside the lens file
`jacobian_lens_{model}{-tag}.safetensors`. Its members are the measured
shape: `lens` naming the sibling file, `fitted_for` with the model path,
its `model_safetensors_sha256`, and the dtype the fit ran in, `corpus`
with source, selection rule, and `prompts_sha256`, `estimator` with the
implementation, its revision, and its parameters, `environment`,
`fit_seconds`, and `lens_shape` with `d_model`, `source_layers`, and
`n_prompts`. **The weights digest takes the shape the model on disk takes**,
as of 2026-09-03: one digest for a model kept in one file, and for a sharded
model a map from each shard's file name to its digest, because the reader
recomputes against the files it opens and a single digest over a sharded
model would name a file that does not exist to hash. A reader handed a
directory follows the model's own index to the shards holding the head and
the final norm, opens those and nothing else, and verifies each under its
own name. A shard the map does not name is a file the fit never saw and
refuses as a wrong digest does, and a map naming no shard refuses as its own
case, an empty identity verifying nothing. **A reader
refuses before it reads**: a manifest naming another lens file, other
weights (the hash recomputed against each file the read opens, never
trusted from the name, and the identity's shape and the model's crossing
refusing before a byte is hashed), a width disagreeing with the loaded
matrices, or a `source_layers` set the tensor names do not match one for one
- a missing layer and an extra tensor alike - each refuse naming the member,
the identity discipline the first-light act exercised. The reading names
the files it verified, so a report rests on weights it can point to.

```graph
node: analysis-lens-refuses-other-weights
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-lens-refuses-other-weights
```

**The capture artifact takes no representation here** because it has one
already: a capture is a certified diagnostic record kept whole, per the
charter, and its shape is `weaver-diagnostic-Spec`'s. What this document
adds is only the reading side's rule, section 5's gate unchanged: a record
is a capture exactly where its bracket closed certified.

## 4. The preload

**Three things in one order, and the seam is owed nothing back.** The election
opens the channel, a distillate per elected event follows in sequence order, and
the seal ends it, per the contract's section 2. This crate asks nothing on this
seam and reads nothing from it.

**The seal is an empty JSON object on its own line**, `{}` canonically, and this
crate writes that spelling. A blank line is framing residue and not a seal, per the
contract, so a driver that emitted one would have closed without sealing and the
parked replay ask on the other door would never answer.

**The command selects its mode before opening the door.** `preload <trace> <socket>`
reconstructs under the recorded election. For a whole record, `--as <session>`
optionally names a branch without changing that election. With `--through`,
`--as` is required and must name a nonempty destination different from the source.
The driver refuses a bare cut or a source-equal destination before connecting or
sending an opener, even if admin has already checked its declaration. This is the
resume/branch boundary of `weaver-state-PRD` section 4 and `weaver-admin-Spec`
section 4: the whole record under its own name is a resume, never a rewind.
`--diagnostic` instead selects section 3's diagnostic
election and requires `--as <session>` naming a nonempty destination different from the
source session. A selected prefix with multiple source sessions refuses rather than
merging their identities. Mode, destination, cut, and election evidence are validated
before connecting or sending any opener.

```graph
node: analysis-diagnostic-requires-distinct-destination
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-diagnostic-requires-distinct-destination
```

**The cut bounds the evidence used to choose the rule.** In ordinary mode, each included
run needs a valid governing `load.tee` before its projected events. An absent rule,
malformed rule, conflicting duplicate load, or event without its run's governing load
refuses naming the missing or conflicting evidence. A false `all_kinds` with no named
kinds is an explicit rule, not missing evidence. One preload has one opener, so included
runs with different effective rules refuse. The recorded list retains the tee's
first-matching-kind semantics: for repeated
`keys` entries naming one kind, only the first entry elects that kind's paths.
Later entries are shadowed, never merged into a union. Resolve that precedence
before comparing effective rules. Afterward, order of distinct kinds and order
or repetition of paths within the effective entry do not change selection.
`all_kinds` remains a distinct fact, never inferred from the events observed in
the selected prefix. Reversing conflicting duplicate-kind entries can therefore
change the rule and make an ordinary multi-run preload refuse. A rule change after the
cut does not invalidate
the earlier prefix. Diagnostic mode can use its own election across source runs, but
never fills in absent source evidence or waives the certification that requires it.

```graph
node: analysis-preload-validates-before-opener
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-preload-validates-before-opener
```

**The completed preload reports what it selected.** The report names mode, source
session, destination session, effective election, projected count, and seal. It reports
the rule used rather than claiming the destination was unused. The wire still carries
only the existing opener, distillates, and seal, with no policy added to state.

```graph
node: analysis-preload-reports-effective-selection
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-preload-reports-effective-selection
```

**The preload takes a cut, as of 2026-09-04.** `preload` accepts `--through
<run>:<turn>`, a run of the record by its reference and a turn within it, and projects
every event of the record through that turn's close and none after it, the seal
following as before, so a session can stand on a prefix of a record rather than the
whole, under the distinct destination required above, per `weaver-state-PRD`
section 4 and issue #432. The cut is by turn because the
turn is the record's own unit and a cut inside one would land a generation without its
close. A run the record does not hold, or a turn that run does not hold, refuses before
anything is sent, naming it, and a turn named without its run is refused for the same
reason the floor's `Cut` carries one: a turn's number recurs across runs. `--as
<session>` lands the projection under another session name, every distillate's session
member rewritten as it crosses and run and sequence as recorded, which is what a branch
needs, because the member bounds every answer to the session its opener declared. Under
a restoring load this verb is the door's driver as it is under the diagnostic binding,
per `weaver-analysis-state-contract` section 1: admin names the
door and dials it never, the load's enter parks until this verb's seal, and the operator
runs the two side by side as the diagnostic flow already does. **The cut is the named
turn's close event**, every event through it crossing and none after, and a turn the
record holds without its close refuses naming the turn, because a cut inside a
generation is what the cut-by-turn rule exists to refuse and a run that died mid-turn
holds exactly that. The rename touches the envelope's session member alone. The
perturbation is two-sided and each side names its removal: drop the cut and events
past the named turn cross, drop the rename and every distillate keeps the record's
session under an opener that declared another.

```graph
node: analysis-preload-cuts-and-renames
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-preload-cuts-and-renames
```

```graph
node: analysis-seal-ends-the-preload
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-seal-ends-the-preload
```

**It dials as an operator principal and never as the agent.** The door refuses the
agent's credential before any byte is read, per the contract's section 4, so a
driver running as the agent's uid has no business this contract recognises. The
instrument is review: what this crate can assert of itself is that it opens the
socket under whatever identity it was invoked with and mints none, and whether that
identity is the operator's is the operator's arrangement rather than a property a
test of this crate reaches.

```graph
node: analysis-dials-as-invoked
kind: assertion
tag: review

edge: asserts
from: weaver-analysis
to: analysis-dials-as-invoked
```

**One preload per standing of this driver.** The owing is the contract's and this
crate meets it by structure: a run of this crate opens one channel, sends one
preload, seals, and closes, so a second preload is a second run. **The member's
door outliving this driver is not this crate's concern**, per the contract's same
clause, and a retry is a new run of this crate rather than a second preload inside
one. The instrument is a compile-fail pin on the shape that would break it: no call
sends a second opener on a channel that carried one, the sender being consumed by
the seal.

```graph
node: analysis-one-preload-per-run
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-analysis
to: analysis-one-preload-per-run
```

## 5. The reading, and the gate

**The diagnostic-trace's parse is the serving parse's sibling and answers to a
different authority.** The line is the same line and the envelope the same
envelope, per `weaver-diagnostic-Spec` sections 2 and 3.1, so section 2's rules
above bind here unchanged: skip unknown content in semantic interpretation,
derive nothing absent, and retain raw projection material. **What differs is the kind
set**, nineteen rather than twenty-four, and `weaver-diagnostic-Spec` section 3.2 is
authoritative for it, a divergence being a defect against that document rather than this
one.

**Which record this crate holds is answered by the record.** A bracket opening with
`replay.opened` is a diagnostic-trace and one opening with `load` is a serving
record, per that Spec's section 4, so this crate reads the first event rather than
its invocation to know what it was pointed at, and a record that answers neither is
refused as neither.

**The gate is the outcome the record states.** Per the charter's section 3 as
settled: this crate produces its reading where a bracket closed certified, produces
the divergence where one closed diverged, produces neither where one closed
abandoned, and **produces nothing for any unclosed bracket, on the same terms
whichever way it came to be unclosed**. A pass that died and a pass still running
leave one absence between them, and this crate does not try to tell them apart,
because treating the end of available bytes as the end of a run is exactly what the
marker exists to stop.

```graph
node: analysis-gates-on-the-stated-outcome
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-gates-on-the-stated-outcome
```

**Where the sink is a pipe the reading drains it and keeps nothing but the
report**, per the charter's section 3 as amended on the operator's ruling of
2026-08-30. The stream is read once in landing order, the report carries the
evidence it rests on, and no member of this crate retains the drained bytes:
retention was the sink shape's to give and it did not, and the charter
carries the licence, the certification's exactness per payload, and the
bound. **What the licence's members gate is the claim and never the drain**: the
report names the device model and the code identity the evidence came from -
both members of the charter's licence, at the precisions that clause states,
their source the deposit the operator holds until the record event that
clause names as owed lands - and a report that cannot establish either says
so and carries no reproducibility claim, the absent member otherwise reading
as one. **The `read` report still cannot establish either member.** Its output
carries the run, reader election, and outcome, and its invocation takes no deposit.
Only `signals` reads an explicitly named deposit through `src/deposit.rs`, under
the summary clause below. That reader does not discharge the reading report's
licence obligation.

**The null replay is elected by this crate's own procedure and not by its control
over the load.** The reader's election rides the declaration and is the operator's,
per apex section 8, so what this crate elects is the order it consumes outcomes in:
it reads a null pass's outcome first and gates every reading downstream on it, and
where no certified null pass stands in the record it produces nothing regardless of
what a reader pass beside it reported. **A readout from an uncertified replay is a
picture of an unknown run**, per `weaver-diagnostic-PRD` section 4, and the gate is
this crate's half of that rule.

```graph
node: analysis-null-replay-gates-the-rest
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-null-replay-gates-the-rest
```

**It writes no record and nothing it produces reaches a decoder.** What this crate
makes is a reading, held or written wherever the operator directs it, and its only
seam into the agent sends material rather than instruction, per the charter's
section 2. The instrument is the compile-fail absence of any write surface toward
either record: no call constructs a trace writer, and the preload's sender takes
distillates and never events.

```graph
node: analysis-writes-no-record
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-analysis
to: analysis-writes-no-record
```

**The lens is loaded here and applied here, and the reading it produces is
the layer trajectory.** The artifact is section 3's: the manifest judged
whole before the file is opened, the header's tensor names answering
before any tensor's data materializes - which is what the format election
bought - and the matrices then held to the manifest one layer for one. The
application is the source's own arithmetic and this crate restates it
rather than inventing one: `unembed(J_l @ h)`, the transport at the layer
the column came from, then the model's own final norm and unembedding, per
`weaver-analysis-PRD` section 1's naming of the reading as this crate's.
**The weights this crate reads for that step are the artifact's own**, the
same safetensors the manifest's hash identifies, so the reading needs no
inference runtime: a matrix multiply, a norm, and a second multiply are
the whole of it. **The second multiply is applied across the cores by
disjoint row ranges**, as of 2026-09-03: the head is a hundred and fifty
thousand rows against one normalized residual, and each row's sum runs in
one thread in index order, so the logits are the single-thread reading's
to the bit and the exactness section 6's compare rests on is untouched by
the parallelism. The threads are the standard library's, scoped, because
section 2's dependency set is four crates and speed is not a reason to
make it five.

```graph
node: analysis-threaded-head-is-bit-identical
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-threaded-head-is-bit-identical
```

**The control precedes every reading and gates it.** At the final layer,
with no transport, the model's own unembedding must rank the token each
position drew at or within the reading's stated bar, and below the bar
this crate produces nothing and says the rate: the pairing, the layer
convention, and the numerics are what the control establishes, and a
trajectory printed over an unestablished pairing is a picture of an
unknown alignment. This is the no-reading-from-an-uncertified-replay rule
one level down, and it is the crate's own bar rather than the record's.

```graph
node: analysis-control-gates-the-reading
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-control-gates-the-reading
```

**A capture's columns are paired by turn and by the measurement's own
order.** A record holds several brackets and positions repeat across them,
so a column is keyed by its turn beside its position, and a turn's
measurement consumes exactly the positions gathered for that turn - the
output order being the draws' own order, the same pairing the field's
realized rank encodes. A column that pairs with no drawn token is not read.

**Two captures compare exactly, and this is certification step 3's own
check performed where both records are held.** Per
`weaver-diagnostic-PRD` section 4 as measured: within one device model the
comparison is exact, so two captures of one source under one declaration
agree value for value or the comparison names the first disagreement with
its turn, position, and layer. **Cardinality is checked and never
truncated**: differing layer counts, differing widths, and an empty column
set each refuse rather than comparing what happens to align, an equal-and-
empty comparison being a verdict over no evidence. **This is what licenses
the discard**: a capture the comparison vouches for is derivable, per the
charter's section 3, and the licence is only as good as the check.

```graph
node: analysis-captures-compare-exactly
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-captures-compare-exactly
```

**The comparison reads each record's `load` event before it reads a value**, as
of 2026-09-07 per the charter's section 3: the composer, by binary and by file and
digest where the loop is a file, and whether the state member stood, both as
`weaver-trace-Spec` section 3 spells them on the payload. Two records naming
different composers, or one standing the member and one not, refuse as incomparable
with the differing fact named, and a record whose `load` names neither refuses as
one whose loop cannot be known. The refusal stands ahead of the token-path check,
because a prompt assembled by another loop diverges at the first token and the
token-path refusal would report that as two runs rather than as two loops. **The
instrument is perturbation**: drop the composer check and two records naming
different loops compare, drop the member check and a record standing the member
compares with one that did not, drop the mute rule and a record naming neither
compares as though it named the same.

```graph
node: analysis-compare-refuses-across-loops-and-members
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-compare-refuses-across-loops-and-members
```

**The reading is taken as the stream drains, and the drain is the class's
rather than the lens's.** `diagnostic-replay-loop` names the diagnostic
loop a class with an interchangeable reader, so what this crate builds is
one drain over the record's events with readers above it: a reader
consumes events as they land and holds only what its own reading needs.
The lens is the first such reader and sets no precedent the next one must
break.

**What a reading holds while it drains is bounded by one turn.** The
control needs each position's final-layer column against the token that
position drew, and the drawn tokens arrive with the turn's measurement
after its columns, so the final-layer columns of the turn in flight are
held until that measurement pairs them, the ranks are taken, and the
columns are dropped. The trajectory's own columns are held only for the
positions the analyst named. **So a reading over a pipe costs one turn's
final layers and the named positions, never the record**, which is what
makes the discard licence a live property rather than a claim about
storage.

**On a stream the analyst names the positions, and the default spread is the file's
alone.** A spread over the whole record cannot be chosen without the whole record, and a
reader that buffered to find one would have kept what the pipe exists not to keep, so a
stream with no named positions refuses rather than silently retaining. **The file's
default spread is eight positions**, defined here since 2026-09-04 because the clause
named one and nothing defined it, which left the file branch refusing with an empty
list: the record's generated positions in order, the first and the last among the eight
and the rest evenly spaced between them by index, every position where the record holds
eight or fewer. The file is read twice to take it, once to learn its positions and once
to read, because a file can be, and the analyst's `--positions` replaces the spread
rather than adding to it. The control runs over every position in both cases.

**A reading is produced only where the record's own bracket closed
certified**, per the charter's section 3 and the gate above: the outcome
arrives at the end of the stream, so the reading accumulates while the
stream runs and is emitted after the close, and a bracket that closed
otherwise or did not close produces nothing. A readout from an
uncertified replay is a picture of an unknown run whether it was read from
a file or drained from a pipe.

```graph
node: analysis-reading-drains-within-a-turn
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-reading-drains-within-a-turn
```

**A position's field is read from the record on the same drain, one position at a
time and never the record.** The record already carries what else had mass at a
generated position: under the field election a `model.field` event stands at every
position the generation retained, per `weaver-trace-Spec` section 3, carrying the
position as the resident length at the draw, the ranked candidates with their
probabilities, and the rank the draw landed on. So the `field <record> --position
<turn>:<position> [--run <run>]` verb, added 2026-09-04 for the trace surface's click
on a spike, adds no reading of its own. Its reader drains a file or a stream as the
signals reader does, keeps the one `model.field` event whose turn and position match
the address, splices the ranked list as the record spelled it rather than parsing and
re-rendering a probability, and drops every other event as it lands, so twenty
thousand positions cost the read one position. **The drawn token is the candidate at
the realized rank** where that rank is within the list, and where the draw fell past
the reported depth it is the entry of the generation's `model.measurement`
`output_tokens` at the field's ordinal within that generation, the fields pairing with
the drawn tokens one for one in landing order because the stop token is neither
retained nor ranked, and where neither answers it is absent rather than invented. One
line answers per run holding the position, each naming its run, because turn keys
repeat across the runs of a serving record, and `--run` narrows the read to one run
and ends it when that run has answered. **It gates on no certified close**, unlike the
lens: the field is the record's own fact about a position and not a reading taken
over a replay, so a serving record and a diagnostic record answer alike, the
diagnostic bracket's close ending the read as it ends every reader's. Three refusals,
each typed as the others are. A record holding no `model.field` event at all refuses
as the field not having been elected, or as elected and unproduced where the `load`
carried a depth. A position the record does not hold at that turn refuses naming the
address. An address that is not `<turn>:<position>` refuses naming what was given.

**The per-position signals are read from the record on the same drain, as a
series, and this reader needs no tap, no lens, and no weights.** Every generation's
`model.measurement` carries the tokens it drew, the distribution's entropy at each
position unconditionally, the drawn token's surprisal where that election stands,
each paired position for position with the drawn tokens, and a `perplexity` for the
generation wherever a distribution was read, per `weaver-spu-Spec` section 6. So the
series a reader wants, where the model was uncertain and where the token it drew
surprised it, is in every record, serving and diagnostic alike, and this reader
pairs and emits it and derives nothing. It is the class's second reader, standing
since 2026-09-02 with the drain: it rides the road the lens rides, holds one turn at
a time, and shares nothing with the lens but the road. **A point is addressed by its
ordinal within its generation**, zero-based, which is what a series is drawn
against, **and not by the position the field read above addresses**, the resident
length at the draw. The record carries both coordinates for one drawn token, each
reader emits the one it reads by, and the conversion between them belongs to a
reader of the record, which holds the residency fact it needs, and never to a
consumer of the series, which does not, per section 7's open election. **Absent
stays absent.** A missing
vector is not invented and a vector shorter than the tokens is not stretched, so a
generation without the surprisal election carries an entropy at every point and a
surprisal at none, and a consumer that fills an absence with a zero is lying about
the election. **A spike is a rule and not a threshold**: the positions whose
surprisal exceeds the series' mean by `k` of its deviations, the caller naming `k`,
empty where fewer than two positions carry a surprisal, and which figure a view may
draw from is a contract's clause rather than this crate's. **It gates only the
record that has a gate.** A serving record carries no bracket and none is owed, it
being an account of what happened rather than a claim that something was
reproduced, and a diagnostic record's series is produced only where its own bracket
closed certified, per the gate above and `weaver-diagnostic-PRD` section 4, the
close ending the read as it ends every reader's. **The summary carries, per
generation, what a store keyed by position converts from**, as of 2026-09-05 per
issue #461: the turn, the perplexity where the record holds one, the resident count
as the generation closed as `model.output` reported it, and the count of output
tokens, and this reader reports the two counts and derives nothing from them. The
counts are the record's own facts, the closing count including the terminator per
`weaver-types-Spec` section 4.4, with `weaver-spu-Spec` section 4 for why, and the
input identifiers being the turn's delta per that Spec's section 6, so the derivation
is the consumer's at ingest, subtracting the drawn
tokens and the terminator from the closing count, and a reader that reported the
previous closing count plus the delta in the resident's place would be wrong on
every first generation by the identity prefix. A generation whose `model.output`
the record does not hold carries no resident count, absent rather than derived.
**The summary carries the record's identity of the artifact too**, as of 2026-09-06
per issue #465 and `weaver-analysis-web-contract` section 3: the weights hash the
generation's measurement carries, per `weaver-spu-Spec` section 3, spelled as the
record spelled it, so the sentinel crosses as the empty string it is, a hash the SPU
could not compute being a fact of the record and not an absence, and the member is
absent only where the measurement carries none. This reader reports it and derives
nothing, and it is the source the run row of `weaver-analysis-web-contract` section 2.2
fills from, the row the web renders from its own repository since 2026-09-26. The
`signals <record> [<k>]` verb, `k` two deviations where none is named, renders the
summary first, one object naming the position count, how many carry an entropy and how
many a surprisal, and the generations by turn with those members, then one line per
point carrying turn, ordinal, token, entropy, and surprisal, and the spikes with their
bar on standard error. **An absent member is omitted from the object and never rendered
null**, on the record's own absent-not-empty rule, so a reader tells a member this verb
did not send from one it sent, and the sentinel is sent. A record holding no measured
generation refuses, typed as the others are.

```graph
node: analysis-signals-keep-absence
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-signals-keep-absence

node: analysis-summary-reports-residency
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-summary-reports-residency

node: analysis-summary-reports-the-record-identity
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-summary-reports-the-record-identity
```

**The summary names the record it was read from, twice**, as of 2026-09-09 per
`weaver-analysis-web-contract` section 2.2 and issue #521, per generation on the
wire and once per run in the reader's row on the weights hash's own rule. **The
record's session** is the envelope's `session`, spelled as the record spelled it,
absent only where a record predates the member. **The record's digest** is sha256
over the run's own lines, every line whose envelope names the run and no other, each
with its terminating newline, in the order this crate drained them, spelled as
lowercase hex. It is computed as the bytes pass and retained no longer than they are,
which is the drain's own rule above applied to a hash: the report carries the digest
and never the lines. **A run is whole when its `unload` landed and the drain began
at or before its `load`**, and the digest is absent otherwise. A run that died has no
`unload`, per `weaver-trace-PRD` section 4.3, and a reader that drained every line
that exists has still not drained the run, because the queue's forfeited tail is
part of what the run was and nobody can vouch past it. A drain that opened after the
run's first line saw a suffix, and a digest of a suffix would read as a digest of the
whole. **Absent says this crate could not vouch for the bytes, and never that it
vouched for the bytes it happened to see.**

```graph
node: analysis-summary-reports-the-record-session
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-summary-reports-the-record-session

node: analysis-summary-reports-the-record-digest
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-summary-reports-the-record-digest
```

**The summary carries the seated prefix's length on every generation's entry, the reader
holding it once per run**, as of 2026-09-09 per `weaver-analysis-web-contract` section
2.2 and issue #527: the resident length before the run's first turn's input, which this
crate derives from the run's first generation and from nothing later, the closing count
less the drawn tokens, less the terminator, less the count of that generation's input
identifiers, the input being the turn's delta with the prefix outside it per
`weaver-spu-Spec` section 6. **It is present only where the drain began at or before the
run's `load`**, since a drain that opened later would take a later turn's floor for the
prefix, and it is absent where the first generation carries no closing count or no
measurement. This crate derives it because no event carries it and the SPU's own record
of it, per its section 4.2, does not leave the SPU, and a reader with only the summary
could not form it, the first draw's position being the earliest the two counts alone
reach.

```graph
node: analysis-summary-reports-the-prefix-length
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-summary-reports-the-prefix-length
```

**The summary names the run and what it ran under, once per run**, as of 2026-09-09 per
`weaver-analysis-web-contract` section 2.2 and issue #532. The run identity is the
envelope's `run`, which this reader already holds to refuse a second run's close and
**rendered nowhere until this act**, so a store keyed by run took its key from nothing
this seam sent. Beside it ride the effective sampling of `model.request`, the field
election's depth and the lineage of the `load` event, and the task's verdict where the
record carries one, each spelled as the record spelled it and each absent on its own
terms.

**What the record does not hold this crate reads from a deposit the caller names.** The
record holds the organ binaries, the `load` event's `stack`, and the device model and
the rest of the code identity are the operator's, per the charter's section 3, which
already has this crate read the device model from a deposit to bound a licence.

**The deposit is named by the invocation and never found beside the record.** **A rule
that looked beside the record would name nothing on a pipe**, which section 5 above
licenses as a sink shape, so the caller names it or names none. **The signals
invocation takes the deposit's path beside the record's**, which is the shape a caller
names one in, and `src/deposit.rs` reads it. The invocation accepts `--deposit`
for file and pipe input alike. `tests/driver.rs` watches a valid named deposit
and the named-none branch with both input shapes. Refusal of a named deposit that
cannot be read or parsed is watched with pipe input only. The corresponding
file-input refusal has no separate watch.

**The agreement rule reaches what a run can agree on.** The sampling's declared members
agree across a run and its derived per-generation seed does not, per `weaver-spu-Spec`
section 8.5, so this crate reports the block as the record spells it and never reads a
run of two generations as a run of two conditions. The task's verdict is authored at the
close and crosses on that generation's entry alone, read from the record's `score`
event as it spelled it. A record holding two refuses the run, and so does a scored run
whose closing generation produced no entry, the verdict then having nowhere true to
cross. **The end of the drain is the reader's other exit**: a scored record that stops
before its run's unload refuses naming the run, where the verdict would otherwise vanish
into a summary saying no score was taken.

**Where the caller names none, the device model crosses absent and the code identity
crosses carrying only what the record held.** This crate neither infers a device from a
driver version nor a library set from a binary it did not see, and a deposit reaches
these members alone.

**Nothing here is derived.** The sampling is spliced as the record spelled it, the depth
and the lineage are the load event's own, and a verdict is the task's. A member this
crate could compute and did not is the discipline section 2 states, applied to a wider
set.

```graph
node: analysis-summary-reports-the-run-and-its-conditions
kind: assertion
tag: perturbation

edge: asserts
from: weaver-analysis
to: analysis-summary-reports-the-run-and-its-conditions
```

**How this crate reaches the sink follows the operator's declaration and not this
document.** The charter's section 3 assumes no discriminant, so this crate takes a
byte stream from its invocation and reads records off it, and whether that stream
is a finished file, a drained pipe, or a held connection is the operator's
arrangement. **What this document refuses to do is elect per shape**, which would
put back the assumption the charter withdrew and make a consumer's reading depend
on how an operator declared a sink.

## 6. What is enforced, and by which instrument

**Enforced by compile-fail tests, because the property is an absence.** No write
surface toward either record: a doctest constructing a trace writer fails to
compile, and **the buyable half is the sender's shape**, which takes distillates
and never events, the no-writer half being bought already by the absent dependency
below. No second opener on one channel, the sender consumed by the seal.

**Enforced by the manifest.** No `weaver-*` dependency at all, read against the
graph under gate H2, this crate declaring two `seam` records, both tagged
`socket`, and no `floor-link`. No async runtime and no socket crate in the
resolved tree. No workspace crate depending on this one, read from cargo's
declared dependencies of every member.

**Requiring a perturbation-verified test.**

- The existing semantic-skip test keeps diagnostic grouping unchanged when
  invented kinds and members appear. It does not watch raw reconstruction.
- No absent member is derived: a record without the layer and forward counts yields
  an unknown layer count, watched to fail when a derivation from the norm array's
  length is put back.
- The existing election instrument checks that the fixed diagnostic projection
  admits only its elected kinds and paths. It does not watch default mode selection.
- The projection splices verbatim: a value the record spelled in a way a
  re-encoding would change crosses byte-identical, watched to fail when the
  projection re-encodes a parsed value.
- Sequence order is preserved: distillates leave in the record's order, watched to
  fail when the projection sorts or groups before sending.
- The seal ends the preload: an empty JSON object follows the last distillate,
  watched to fail when a blank line or a close alone is substituted.
- The gate reads the stated outcome: a record whose bracket never closed produces
  nothing, watched to fail when the end of available bytes is read as an ending.
- The null replay gates the rest: a reader pass's outcome produces nothing where no
  certified null pass stands in the record, watched to fail when the ordering is
  removed.
- The summary reports the residency: a generation's resident count is the one
  `model.output` carried, watched to fail when the reader derives it from the
  previous closing count and the delta, which the first generation refutes.
- The preload cuts and renames: a `--through` projects every event through the named
  turn's last event and none after, watched to fail when the cut is dropped, and a
  `--as` rewrites every envelope's session, watched to fail when the rewrite is
  dropped and a distillate keeps the record's name. These existing cut and rename
  mechanics do not watch the new mode, destination, or preflight refusals.
- The summary reports the record's identity as spelled: a measurement carrying the
  sentinel crosses as the empty string and one carrying no member crosses absent,
  watched to fail when the reader folds the sentinel into absence, and an absent
  member is omitted at the wire rather than rendered null, watched to fail when the
  verb renders the option directly.
- Absence stays absent in the series: a record without the surprisal election
  yields points carrying an entropy and no surprisal, watched to fail when the
  reader fills an absent vector with zero.

- The summary names the record's session: a record whose envelope carries one
  crosses it per generation, watched to fail when the member is dropped or filled
  from the invocation rather than the envelope.
- The summary names the record's digest only for a whole run: a run lacking its
  `unload`, or drained from after its `load`, crosses with the digest absent, watched
  to fail when a digest over the lines that happened to be present is sent in its
  place, and watched to fail when two emitters over one file disagree, which is the
  delimiter or the newline rule being broken.
- The summary names the run it read: an emission carries the envelope's run once,
  watched to fail when the member is dropped, which is the state before 2026-09-09, and
  when it is taken from the invocation's path rather than from the envelope.
- The summary reports what the run ran under without deriving any of it: a record whose
  caller named no deposit crosses with the device model absent and the code identity
  carrying only the load event's stack, watched to fail when either is inferred from a
  driver version or a library this crate did not see, and a sampling crosses spliced,
  watched to fail when a re-encoding changes what the record spelled.
- The summary carries the seated prefix's length only from a whole-from-the-start drain:
a run whose first generation the drain saw crosses with the length derived from that
generation, watched to fail when it is derived from a later generation or sent from a
drain that opened after the run's `load`, and watched to fail when the input identifiers
are not subtracted, which names the first draw and not the prefix.

**Recorded-election instruments.** `tests/driver.rs` watches the CLI wire and
refusals described below, with the remaining destination-agreement watch named
beside its obligation. The three-way comparison with an independent tee is a
separate live instrument in the state crate's in-file suite. It is ignored by
default and requires scratch PostgreSQL, `unshare -Ur`, and a prebuilt analysis
binary. It cites state claims, not `analysis-reconstruction-follows-recorded-election`,
and is not that row's instrument. The tee is reached through state's test-only
trace dependency, rather than through a new link from this crate.

- Recorded reconstruction: compare the CLI wire with the fixture's expected
  elected material. Include unknown elected kinds and paths, all-kinds with no named
  kinds, a restrictive empty rule retaining turnless system messages, and an excluded
  ordinary event. Preserve first-match precedence for duplicate-kind entries.
  Restoring the fixed default, dropping unknown material, suppressing the system
  exception, or merging or sorting duplicates before selection must fail.
- Preflight: absent or conflicting evidence, mixed source sessions, invalid cuts,
  and source-named cuts refuse without an opener and leave existing holdings intact.
  The CLI watch checks that the listening door receives no connection, rather than
  querying a store after refusal. Reversing conflicting duplicate-kind entries
  across runs refuses, while reordering
  distinct kinds or effective paths does not. Moving validation after the opener or
  accepting a source-named cut must fail the preservation check.
- Diagnostic destination: missing, empty, or source-equal destinations refuse before
  the opener. Removing any of those checks must fail the refusal fixture.
- Declaration destination: `derive --as` writes the requested distinct destination
  while keeping source evidence unchanged. Dropping the rename or accepting an invalid
  destination must fail. A comparison of derive and preload destinations remains
  owed: the derive test reads its stdout alone.
- Selection report: mode, source and destination, effective rule, count, and seal
  match the invocation and sent projection. Removing or falsifying any member must
  fail its report comparison.

**Enforced by review, two claims.** That this crate dials as an operator principal
is the operator's arrangement rather than a property a test of this crate reaches,
per section 4: what a suite can confirm is that this crate mints no identity and
opens the socket under the one it was invoked with, and the credential's rightness
is judged at the far end by the door. That this crate's own code binds no listener
is the residue section 1 names: the manifest reaches the dependency and no
instrument here reaches the absence of a call `std` offers every crate, so the
claim is review's and says so rather than borrowing the manifest's coverage.

**Enforcement inventory.** Rows name their declared instrument and citation
locations. A citation locates a claim and does not establish the scope of its
watch. The instrument descriptions above state that scope and the remaining gaps.

| Claim | Instrument |
| --- | --- |
| `analysis-no-internal-dependency` | manifest: `Cargo.toml`, `tests/manifest.rs` |
| `analysis-no-runtime-no-socket-crate` | manifest: `Cargo.toml`, `tests/manifest.rs` |
| `analysis-no-crate-depends-on-it` | manifest: `Cargo.toml`, `tests/manifest.rs` |
| `analysis-binds-no-port` | review: section 1 review of binding calls |
| `analysis-parse-skips-the-unknown` | perturbation: `src/record.rs`, `tests/driver.rs` |
| `analysis-derives-no-absent-member` | perturbation: `src/record.rs`, `tests/driver.rs` |
| `analysis-reconstruction-follows-recorded-election` | perturbation: `src/selection.rs`, `tests/driver.rs` |
| `analysis-election-declares-what-follows` | perturbation: `src/project.rs`, `tests/driver.rs` |
| `analysis-projection-splices-verbatim` | perturbation: `src/project.rs`, `tests/driver.rs` |
| `analysis-sequence-order-preserved` | perturbation: `src/project.rs`, `tests/driver.rs` |
| `analysis-declaration-derives-from-the-record` | perturbation: `src/declare.rs`, `tests/driver.rs` |
| `analysis-derive-uses-explicit-destination` | perturbation: `tests/driver.rs` |
| `analysis-lens-refuses-other-weights` | perturbation: `src/lens.rs`, `tests/lens.rs` |
| `analysis-diagnostic-requires-distinct-destination` | perturbation: `src/selection.rs`, `tests/driver.rs` |
| `analysis-preload-validates-before-opener` | perturbation: `src/selection.rs`, `tests/driver.rs` |
| `analysis-preload-reports-effective-selection` | perturbation: `tests/driver.rs` |
| `analysis-preload-cuts-and-renames` | perturbation: `src/project.rs` |
| `analysis-seal-ends-the-preload` | perturbation: `src/preload.rs`, `tests/driver.rs` |
| `analysis-dials-as-invoked` | review: `src/main.rs` |
| `analysis-one-preload-per-run` | compile-fail: `src/lib.rs` |
| `analysis-gates-on-the-stated-outcome` | perturbation: `src/reading.rs`, `tests/reading.rs` |
| `analysis-null-replay-gates-the-rest` | perturbation: `src/reading.rs`, `tests/reading.rs` |
| `analysis-writes-no-record` | compile-fail: `src/lib.rs` |
| `analysis-threaded-head-is-bit-identical` | perturbation: `src/lens.rs`, `tests/lens.rs` |
| `analysis-control-gates-the-reading` | perturbation: `src/lens.rs`, `tests/lens.rs` |
| `analysis-captures-compare-exactly` | perturbation: `src/capture.rs`, `tests/lens.rs` |
| `analysis-compare-refuses-across-loops-and-members` | perturbation: `src/capture.rs`, `tests/lens.rs` |
| `analysis-reading-drains-within-a-turn` | perturbation: `src/stream.rs`, `src/capture.rs`, `tests/stream.rs` |
| `analysis-signals-keep-absence` | perturbation: `src/signals.rs`, `tests/stream.rs` |
| `analysis-summary-reports-residency` | perturbation: `src/signals.rs`, `tests/stream.rs` |
| `analysis-summary-reports-the-record-identity` | perturbation: `src/main.rs`, `src/signals.rs`, `tests/stream.rs` |
| `analysis-summary-reports-the-record-session` | perturbation: `src/main.rs`, `src/signals.rs`, `tests/stream.rs` |
| `analysis-summary-reports-the-record-digest` | perturbation: `src/main.rs`, `src/signals.rs`, `tests/stream.rs` |
| `analysis-summary-reports-the-prefix-length` | perturbation: `src/main.rs`, `src/signals.rs`, `tests/stream.rs` |
| `analysis-summary-reports-the-run-and-its-conditions` | perturbation: `src/main.rs`, `src/signals.rs`, `src/deposit.rs`, `tests/stream.rs`, `tests/driver.rs` |

**Where the records sit.** Records remain beside their clauses in sections 1
through 5. The selection, preflight, destination, report, and summary records
retain the scope their clauses declare. Their instruments are named above.

**Which invariant each claim serves.** The internal-dependency claim carries the
standing `grounds` edge to `axiom-floor-is-vocabulary-behavior-is-socket`. The other
claims are representation and carry no new grounds edge, per Document Format
section 4.

## 7. Open elections

- **The instrument suite**, per the charter's section 4: what this crate carries
  beyond the certification is named there as a sketch that does not exist in this
  tree, and nothing here is built against it.
- **The capture artifact, closed 2026-09-01**: a certified diagnostic record
  kept whole, the charter's section 3 carrying identity, custody, shape, and
  quota, and section 3 here adding that no second representation exists.
- **The lens artifacts, closed 2026-09-01**: representation in section 3 -
  safetensors matrices beside a JSON manifest, refused before read where the
  identity disagrees - the criteria the charter's clauses of the same date.
- **What the reading is, as an artifact, narrowed 2026-09-01.** Section 5
  settles the reading's content - the layer trajectory, the control that
  gates it, and the capture comparison - and leaves its rendered form to
  the suite's act, that being a presentation question rather than the
  gate's.
- **The satellite types.** The parse's newtypes over the record's identity strings,
  the election's spelling here, and the byte-stream reader's shape. Identifier
  choices with no cross-crate consequence, listed so what this Spec leaves to a
  builder is complete rather than implied.
- **The licence boundary** is the operator's, per the charter, and this document
  takes no position beyond noting that nothing here carries cut-and-recompute.
- **What the series carries so a store keyed by position can address it, closed
  2026-09-05.** Section 5's summary carries the resident count as the generation
  closed and the output count, both reported from the record, and the consumer
  derives the position once at ingest. The other shape, the record carrying the
  count as the generation opened, was not taken: the closing count and the drawn
  tokens already determine it once the terminator's width is stated, which
  `weaver-spu-Spec` section 4 now does, and the opening count would be a new
  member on a new event where the closing count has been on the wire since
  2026-08-19, so the cheaper answer is also the one that asks the record for
  nothing. Per issue #461.
- **What reads a deposit, and how, closed 2026-09-22 by #658.** Section 5 elects
  an explicit `signals --deposit` argument, and `src/deposit.rs` reads the named
  file. File and pipe inputs use the same reader. The device model comes only from
  that deposit, and code identity combines the recorded stack with the deposit's
  observed members.
  No deposit means those members stay absent, while an unreadable named deposit
  refuses. The record event that would carry the observations remains owed by the
  charter. The task verdict is read since 2026-09-26, from the `score` event #707
  added on issue #523.
