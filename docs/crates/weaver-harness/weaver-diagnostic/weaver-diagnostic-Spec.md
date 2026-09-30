# weaver-diagnostic - Spec

**Status:** MERGED. Cut 2026-08-27, the first Spec of the diagnostic leg. Code is
written against it under the gates of Working Process section 6.

**Date filed:** 2026-08-27
**Document ID:** `weaver-diagnostic-Spec`
**Parent:** `weaver-diagnostic-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #630

---

## 0. What this document is

The representation of the mechanism `weaver-diagnostic-PRD` charters: the record a
replay makes, the shape of the events that compose it, and the surface the harness
authors through. **It is written against that charter and against
`weaver-harness-diagnostic-contract`**, and it develops no rationale of its own:
where reasoning appears it traces to a clause of one of those two, per G2.

**Three elections the charter left, and this document is where they land.** How much
of the serving vocabulary this writer reuses, where the residual readout sits beside
it, and whether the seam mirrors `weaver-trace`'s surface or takes its own. The
charter's section 6 names all three as owed here and calls the first two the larger
half of the work.

**A fourth lands here that was named from outside.** How a diagnostic-trace says it
ended and how it says what happened, owed to this document by
`weaver-analysis-PRD` section 4, which names the outcomes a marker has to separate
and states that the crate paying for its absence is not the crate that can supply
it.

**This document declares its crate's assertion records, and five `asserts` edges that
are `weaver-harness`'s.** The five are the replay claims of sections 3.3 and 4, each
carrying an edge from that crate beside this one's because the instruments stand in
its suite and none stands here. Section 7 argues it and is the authority, this
paragraph answering only the question a reader asks here, which is whether this
document declares anything that is not this crate's. `weaver-harness-Spec` section 0
carries the other half and says it declares those five nowhere.

## 1. The crate

**Layout.** One module per obligation, re-exported at the root.

    src/lib.rs        re-exports, and nothing else
    src/event.rs      the envelope, the kind set, the payload shapes, section 3
    src/recorder.rs   the receive, the submit, and the write, section 5
    src/failure.rs    the failure vocabulary, section 6

**Edition and toolchain.** Edition 2024 on the pinned nightly, no nightly feature
used.

**The dependency set is two crates and one internal one.** `serde` with `derive`,
`serde_json` with `raw_value` for splicing the harness's pre-rendered message
payloads without re-encoding them, and `weaver-traits` for the message model the
replayed contributions carry, which the contract's vocabulary clause draws.

**This crate does not link `weaver-trace`, and the negative is an election rather than
an accident.** It is not the no-internal-dependency claim the floor Specs and
`weaver-analysis-Spec` section 1 ground in apex section 5.1, and it carries no edge for
that reason: this crate holds an internal dependency, `weaver-traits` above, so what is
claimed here is the absence of one particular link and the reason is the counterpart
relation rather than the socket invariant. The two records share a form and not a type:
the canonical form of `weaver-trace-Spec` section 2 is this record's by the charter's
own naming, and following a rule is not linking its author. Linking it would buy nothing
this crate needs, that crate's `Kind` being exhaustive and closed against the kinds a
replay authors, and would cost the counterpart relation the charter rests on, a
mechanism that depended on its twin being a submodule of it in all but name.

```graph
node: diagnostic-no-trace-dependency
kind: assertion
tag: manifest

edge: asserts
from: weaver-diagnostic
to: diagnostic-no-trace-dependency
```

**No async runtime and no socket crate in the resolved tree**, by the build-time
`cargo tree` assertion the floor Specs share. This crate crosses no process line and
holds a handle rather than a name, so a socket crate would be weight against a
capability it must not have.

```graph
node: diagnostic-no-runtime-no-socket-crate
kind: assertion
tag: manifest

edge: asserts
from: weaver-diagnostic
to: diagnostic-no-runtime-no-socket-crate

edge: grounds
from: diagnostic-no-runtime-no-socket-crate
to: axiom-floor-is-vocabulary-behavior-is-socket
```

## 2. Canonical form

**One rule, and it is not this document's.** Every event renders to exactly one line
of UTF-8 JSON with no interior newline, the newline terminating it as the record
separator, and every integer that can exceed the double-safe range renders as a
decimal string. That is `weaver-trace-Spec` section 2, named authoritative for this
record by `weaver-diagnostic-PRD` section 6 under G5, and a divergence here is a
defect against it rather than a second rule.

**What this document adds is the obligation to follow it in a crate that cannot
import it.** The rule crosses as prose, so the two renderings can drift where a
shared type could not, and the instrument is the one place that drift is visible: a
perturbation test renders an event of a kind both records carry and compares the
line against the same event's line from a serving record, byte for byte, watched to
fail when either renderer's field order or number spelling moves.

```graph
node: diagnostic-canonical-form-follows-trace
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-canonical-form-follows-trace
```

## 3. The event

### 3.1 The envelope

The envelope is the serving envelope's, field for field: `session`, `run`, `turn`,
`kind`, `sequence`, `subsystem`, `causal_parent`, and both timestamps, flattened
into the event so `kind` sits at the top level of every line where a consumer keys
on it. **The turn and the causal parent are optional and nothing else is**, per
`weaver-trace-Spec` section 3, and a replay fills neither to satisfy a shape: an
event belonging to no turn carries none, and an event the pass cannot attribute to
a cause carries no parent. The shapes
are `weaver-trace-Spec` section 3's and a divergence is a defect against it, per G5.

**The session is the diagnostic run's own and never the replayed one**, per the
contract's section 3. A derived record that wore its source's name would answer the
first question a reader asks of a file, which of the two it is, with the wrong
answer, and the charter's section 6 argues that identity at length. What names the
replayed session is the opening event's payload, where a reader looks for provenance
rather than inferring it from an envelope.

```graph
node: diagnostic-session-is-the-replays-own
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-session-is-the-replays-own
```

### 3.2 The kind set

**Nineteen kinds, exhaustive, and the set is this crate's own.**

    replay.opened          the pass's bracket opens, and the record identifies itself
    replay.identity        the input identity the pass established
    replay.closed          the pass's bracket closes, carrying its outcome
    residual.column        one generated position's residual columns, where asked
    turn.started           a replayed turn opens
    turn.closed            a replayed turn closes
    message.system         the seated prefix, as the record carried it
    message.user           a replayed contribution
    message.assistant      a replayed contribution
    message.tool_result    a replayed contribution
    model.request          what was fed to the forward pass
    model.output           what the forward pass produced
    model.measurement      the reading, the residual readout riding it
    model.field            the elected field, where the pass elected one
    flush                  a cut the loop drove, as the record carries one
    refusal                a typed refusal answering an ask the pass sent
    fault                  a death, named
    recall                 an answered state-seam ask, by the identities it returned
    elision                a span the loop removed, as the record carries one

**Fifteen spellings are the serving vocabulary's and mean there what they mean
here.** A kind that names the same fact carries the same spelling and the same
payload shape, which is what makes reader compatibility a rule rather than a
coincidence, per section 4. **Four are this record's own**, the `replay.` trio
and `residual.column`, and no serving record carries any of them.

**Every kind carries a turn rule, and a shared kind keeps its serving one**, as of
2026-09-26. Keeping the serving meaning includes keeping when the kind belongs to a
turn, so the recorder applies the rule at admission before the pairing, with the
serving recorder's own refusals: a turn on a kind that belongs to none refuses as a
malformed payload, and a missing turn on a kind that belongs to one refuses as the
required field absent. Until this act the admission judged the pairing alone and
admitted a turned `flush` or a turnless `model.request`, which a serving recorder
refuses.

    turnless        replay.opened, replay.identity, replay.closed, flush, elision,
                    recall
    turn-required   turn.started, turn.closed, message.user, message.assistant,
                    message.tool_result, model.request, model.output,
                    model.measurement, model.field, residual.column
    turn-optional   message.system, refusal, fault

The replay trio belongs to the pass and not to a replayed turn, and `residual.column`
to the turn whose position it was taken at, which is how the harness authors each.
**The rule is one exhaustive match in `src/recorder.rs`**, so a kind added to the set
is not admitted until it is given a rule and a row here.

```graph
node: diagnostic-turn-rule-per-kind
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-turn-rule-per-kind
```

**`flush` is carried because the loop is granted the flush by name.**
`diagnostic-replay-loop` section 1 enumerates what the seat grants it, the state
port, the decode surface, and the flush, and `weaver-harness-Spec` section 6 has the
harness author a `flush` event on the flush's confirmation. A record that could not
carry it would drop an act its own loop is chartered to perform, which is the
accumulation reading broken exactly where the serving record protects it.

**`recall` is carried because the replay's own replay port authors one**, on the
ruling of 2026-09-26 that made an answered state-seam ask a serving kind, per
`weaver-trace-Spec` section 3. The loop's replay port asks the member for the
holdings, and the harness records the answered ask as a `recall` whatever the record's
mechanism, so a record that could not carry one would refuse the replay's own walk. It
lands inside the bracket after `replay.opened` and before `replay.identity`, named by
the holdings' first and last events and their count. The replay's enter records
nothing, as it records no load and no prefix, per the same section. It splices the
serving payload as the other fourteen shared kinds do. The walk reads the source's
flush and elision since 2026-09-26, and a source's own `recall` since the same day: a
post-flush request with no recall of the `recall` verb before it refuses at identity on
a record carrying the kind, per `diagnostic-replay-loop` section 2.

**`elision` is carried because the loop now performs one**, as of 2026-09-26, which
closes the open election section 8 held. `diagnostic-replay-loop` section 2 reproduces
a source's elision where it fell, as it reproduces a flush, so the loop's grant names
the elision port beside the flush's, and a record that could not carry the act would
drop one its own loop performs, the flush's reason. Until this act the harness's
authoring of a replay's elision was refused as outside this vocabulary and went
unrecorded while the pass went on. It splices the serving payload, the span and both
counts, and belongs to no turn.

**`refusal` is carried because the same section authors every typed refusal, and a
replay produces two.** `weaver-harness-Spec` section 6's ruling of 2026-08-22 has a
refusal answering an ask this harness sent become a `refusal` event naming the ask
and carrying the seam's own case. A replay sends asks that can be refused: **the
flush it was just granted**, whose `keep` that section says nothing else records,
and **the decode ask mid-replay**, which is `diagnostic-replay-loop` section 4's
third named failure and which that document requires the record to carry, the
partial account being the point of authoring one. Routing either to `fault` would
collapse a refusal into a death, which is the collapse this section refuses twice
elsewhere.

**The nine serving kinds this set does not carry are absent by construction rather than
by omission.** A replay runs no Gate, calls no tool, and asks no classifier, so
`tool.call.started`, `tool.call.completed`, and the classify pair have nothing to author
them, and a variant standing for a case nothing produces is the reserved slot apex
section 9 forbids. `load`, `unload`, and `session.closed` are absent for a different
reason: they bracket a serving load and this record's bracket is the pass, which the
`replay.` trio carries. `message.restored`, since 2026-09-26, is absent on the
identity's ground: a diagnostic enter seats no prefix, per section 4. `score`, since the
same day, is absent because a replay runs no task and so reaches no verdict. **`elision`
was absent too until 2026-09-26**, on the narrower ground the flush's presence left
standing, the loop's grant then naming the flush and not the elision. The act that
granted the loop the elision added the kind with its own argument, above.

**`residual.column` is carried because the diagnostic binding may ask past the fold**,
per the operator's ruling of 2026-08-30, `weaver-spu-PRD` section 13.7 as amended, and
the decode contract's third intermediate. One event per sampled position - each decode
forward, and each reissued append's prefill final position, nothing else of the prompt,
per that clause's bound. **A recorded tool round replays as an ordinary append and is
covered**: the replay calls no tool, per the absent-kinds paragraph above, and the
recorded `message.tool_result` still re-feeds as its own append, whose prefill final
samples the continuation's first token - the position after the retrieved fact lands,
which a bound excluding it would hollow at exactly the point a diagnosis reads. Authored
by the harness from the intermediate the seam delivered, carrying the position it names,
the layer count, and the tap's width, the values crossing in the provisional bare JSON
the floor states and the efficient encoding being section 8's open election beside the
seam's own at `weaver-spu-Spec` section 12. **Authored only where the ask stood, and
under a standing ask a sampled position without one is charter 13.10's fault**, owned
there since this act and cited rather than restated: an elected observation that
silently stopped observing. This record's `Failure` enum cannot yet express it, and the
case lands with the code act that builds the seam, named in section 8 as owed. The
record's identity is its provenance and never a member, per section 4's discriminant and
the parent charter's 2026-08-24 correction, so nothing here reads the kind's presence as
identifying anything. **The exhaustiveness grounds in apex section 5.3, and not on the
serving record's reason.** `weaver-trace-Spec` section 3 argues from a set closed by
ruling and matching its charter one to one, and this set is not that one: nineteen
kinds, fifteen of them the serving vocabulary's and four this record's own, so closure
by charter does not carry across. What carries is the contract's. Section 7 of
`weaver-harness-diagnostic-contract` makes a change to this kind set a change both
parties merge in one act, which is a closure the seam states, and a consumer able to
absorb a further kind into a wildcard would leave that stated closure unenforced on the
side that reads.

```graph
node: diagnostic-kind-set-exhaustive
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-diagnostic
to: diagnostic-kind-set-exhaustive

edge: grounds
from: diagnostic-kind-set-exhaustive
to: axiom-contract-is-a-complete-interface
```

### 3.3 The payload shapes

**The mapping is total: nineteen kinds, eighteen shapes named whole and the
nineteenth named to its members.** Fifteen take the serving payload of the
same name, spliced or shaped as `weaver-trace-Spec` section 3 shapes it, that
document being authoritative and a divergence a defect against it. Three are
declared here. `residual.column`'s identity members are fixed in section 3.2
and its values cross in the floor's provisional bare JSON, so the kind is
buildable today, and what section 8 holds open is the efficient encoding, not
the shape.

    pub struct ReplayOpened {
        pub reader_elected: bool,
    }

    pub struct ReplayIdentity {
        pub replayed_session: String,
        pub model: ModelId,
        pub weights_hash: WeightsHash,
        pub template: TemplateId,
    }

    pub struct ReplayClosed {
        pub outcome: ReplayOutcome,
    }

    pub enum ReplayOutcome {
        Certified,
        Diverged { divergence: Divergence },
        Abandoned { reason: AbandonReason },
    }

    pub enum Divergence {
        TokenPath {
            position: u64,
            recorded: TokenId,
            recomputed: TokenId,
        },
        Readout { position: u64, layer: u32 },
    }

**`ReplayOpened` carries only what the load declared, and the provenance rides its
own kind.** The pass's bracket opens when the run opens, which is before the replay
ask has been answered, so what the harness holds at that moment is the pass's own
election and nothing about the record it is about to read: the declaration does not
name the replayed record, the driver's invocation does, per `weaver-analysis-PRD`
section 4's placement of the path. **`replay.identity` carries the input identity
once step one of `diagnostic-replay-loop` section 3 has established it**, which is
the first moment the replayed session, the model, its weights hash, and the
template are known from the answered holdings. **Established means read and
checked, not merely read**: a step one that read the holdings and refused them has
established nothing, so the pass authors no identity event and closes
`Abandoned`, which is the distinction this kind exists to keep.

**Step one refuses a source whose record does not account for its model's input**,
as of 2026-09-26 under epic #690. A certification says the recorded path reproduces
from the recorded input, so a record that cannot say what the model's input was drawn
from gives it nothing to stand on. Two cases reach step one, per
`diagnostic-replay-loop` section 2: an open that recorded an
`identity_prefix_unrecorded` fault, whose opening prefix is missing, and a post-flush
request with no seat recall before it on a record carrying the `recall` kind, whose
re-entry's provenance is missing. Each closes `Abandoned` at identity naming what was
missing, and authors no `replay.identity`, per the paragraph above.

```graph
node: diagnostic-identity-refuses-an-unaccounted-input
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-identity-refuses-an-unaccounted-input

edge: asserts
from: weaver-harness
to: diagnostic-identity-refuses-an-unaccounted-input
```

**What `reader_elected` separates is the null replay from the pass beside it**, per
that loop's section 3 step 3, each pass running as its own run under its own
reference, so the flag rather than the reference is what a reader keys on.

```graph
node: diagnostic-identity-absent-not-invented
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-identity-absent-not-invented

edge: asserts
from: weaver-harness
to: diagnostic-identity-absent-not-invented
```

**Splitting them is what lets the record carry the loop's first named failure.**
That loop's section 4 opens with the replay answer being absent, where the run
"ends its run having replayed nothing and the account says so in the
diagnostic-trace, which is how the operator learns it." A bracket whose opening
event required the provenance could not open at all in that case, so either no
bracket would exist and the operator would learn nothing, or one would open
carrying members filled from nothing. **A bracket with no `replay.identity` is a
pass that never established one**, which is the same absence-means-something
discipline the outcome below runs on.

**The monotonic clock originates at `replay.opened`.** A serving record originates
it at `load`, per `weaver-harness-trace-contract` section 3, and this set carries no
`load`, so the pass's own opening event is the origin and every reading in the
bracket is relative to it.

**`ReplayOutcome` is the terminal marker, and it names three of four outcomes.**
`weaver-analysis-PRD` section 4 names the four a reader must separate: certified and
ended, failed its comparison and ended, ended without finishing, and not ended. The
first three are facts the pass knows and authors. **The fourth is the absence of a
`replay.closed` event and is therefore free**, which is that section's own
observation read forward: a reader that finds a bracket opened and never closed
holds a pass that did not end, and no event has to say so.

**`Divergence` separates the two comparisons certification performs**, per
`weaver-diagnostic-PRD` section 4: the token path matches exactly or it does not,
which is integers, and the reader's vectors compare within the GPU float tolerance
the apex names. A single shape typed to the token path would have had a vector
divergence routed to `Abandoned`, which collapses the second outcome into the third
exactly as manufacturing a close would collapse the fourth. Each carries the first
divergent position, per the loop's section 3 step 2, and the token path's carries
both identifiers so a reader can say how the two differ without rerunning anything.

**`position` is the resident length at the draw in both variants**, the coordinate
`model.field` keys on per `weaver-spu-Spec` section 4.2, on the operator's ruling of
2026-09-09 at issue #519. A record carries three position coordinates and this ruling
retires one of them: the measurement's vectors run by output ordinal from zero, the
field by the resident length at the draw, and a divergence gave the draw's index in the
turn's identifiers, which is neither. The resident length is the one that survives the
turn's decomposition, indifferent to whose token it was or which append put it there, so
a divergence and the field row it fell in share a key and a reader holding only the
close event needs no conversion it cannot perform. A token-path divergence in the
appended input names the resident length that token occupies, below the first draw by
the input's length, so a tokenization divergence and a draw divergence land on one
scale. **The pass derives it from its own re-fed answer and not from the holdings**: the
closing count less the drawn tokens less the terminator is the first draw's position,
per that section's closing-count clause, and the null replay therefore still rests on
the recorded identifiers alone. **The pin below reaches the token path alone**: no pass
constructs a `Readout` divergence today, the reader pass of `diagnostic-replay-loop`
section 3 step 3 being unbuilt, so the ruling stands over that variant as a statement
its first producer honors rather than one an instrument holds. Records written before
this ruling carry the position as the turn's identifier index, which on the essays of
2026-09-08 sits 127 below the field's key for the same token, and are read as the
pre-ruling coordinate where they are cited.

```graph
node: diagnostic-divergence-position-is-the-resident-length
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-divergence-position-is-the-resident-length

edge: asserts
from: weaver-harness
to: diagnostic-divergence-position-is-the-resident-length
```

**No outcome is authored for a pass that died.** The contract's section 5 forbids
manufacturing one, and the reason is the fourth outcome: a closing event written at
a death would collapse unended into abandoned and tell a reader a pass ended when it
stopped. **`Abandoned` is for a pass that reached its own end without certifying**,
whatever stopped it, the loop's likeliest being a refusal at input identity before
any forward pass ran. What `AbandonReason` enumerates is section 8's.

```graph
node: diagnostic-outcome-absent-not-manufactured
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-outcome-absent-not-manufactured

edge: asserts
from: weaver-harness
to: diagnostic-outcome-absent-not-manufactured
```

### 3.4 Where the readout sits

**On the measurement, exactly where a serving record puts it.** The residual readout
rides `model.measurement` as `weaver-trace-Spec` section 3 shapes it, with the layer
count and the forward count beside the figures as the act of 2026-08-24 landed them.

**What differs between the two records is density and not shape.** On a serving load
the readout is an election the operator pays for and may decline, and on a
diagnostic pass it is the point of the run, per the charter's section 6. That is a
fact about how often the member is populated and never about what the member is, and
a representation that forked on it would make one reading of one quantity wear two
shapes for no reason a reader could act on.

**Two consequences follow, and both are why this election is the right one.** An
instrument that reads a serving record's measurement reads this one's without
learning a second shape, which is section 4's compatibility rule doing real work
rather than being asserted. And the raw-over-fold ruling this leg already carries is
honored rather than restated: nothing here widens a reduction or freezes a fitting
into a capture, because nothing here reshapes what the SPU rendered.

```graph
node: diagnostic-readout-rides-the-measurement
kind: assertion
tag: review

edge: asserts
from: weaver-diagnostic
to: diagnostic-readout-rides-the-measurement
```

## 4. What a reader may assume

**The charter declines to claim reader compatibility and assigns the claim here.**
Section 6 of it states that whether an instrument reading a serving record reads
this one follows from this document's elections, so this section states the rule
rather than leaving a reader to test it.

**An instrument that keys on `kind` and skips what it does not know reads both
records, and reads every shared kind identically.** That holds because of three
elections above and no others: the line is the same line, section 2, the envelope is
the same envelope, section 3.1, and a shared spelling means a shared shape, section
3.3. Nothing else is promised.

**What tells the two records apart is the opening kind, and it is in the file.**
A diagnostic-trace opens every bracket with `replay.opened`, which no serving record
carries, and a serving record opens with `load`, which this one does not. The
charter's section 6 argues that a reader's first question of a file in hand is which
of the two it holds and that a shape answering it nowhere would be a defect, so the
answer sits in the first event of every bracket rather than in a member a reader has
to hunt for.

```graph
node: diagnostic-record-identifies-itself-at-the-open
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-record-identifies-itself-at-the-open

edge: asserts
from: weaver-harness
to: diagnostic-record-identifies-itself-at-the-open
```

**The versionless-schema rule binds a reader of this record exactly as it binds a
reader of a serving one**, per `weaver-trace-PRD` section 6: the schema extends and
does not change, a reader skips a kind and a payload member it does not know, and a
record written before a member existed omits it rather than carrying a default.
`weaver-analysis-PRD` section 4 states what that costs the reader, and this document
adds only that this record is on the same terms.

## 5. The recorder

**The surface mirrors `weaver-trace`'s receive and submit, and shares no type with
it.** That is the charter's open election settled toward the mirror, and the harness
Spec's section 9 item settles with it.

    pub struct Recorder { /* private */ }

    impl Recorder {
        pub fn receive(
            sink: OwnedFd,
            run: RunRef,
            session: SessionRef,
        ) -> Result<Recorder, Failure>;

        pub fn submit(&mut self, event: Event) -> Result<Sequence, Failure>;
    }

**Mirroring is what lets one call site serve both mechanisms**, which is the
election's whole argument: the harness holds the two recorders under one shape and
the binding's kind selects the arm, per `weaver-agents-PRD` section 6, so the
authorship path does not fork per site. A surface of its own would have made every
authoring site in the harness ask which record it was writing.

**Sharing no type is what keeps the mirror from becoming a dependency.** A trait
would need a home, and neither crate may hold it: `weaver-trace` links nothing at
all, and a floor trait for two writers that never meet on a wire would put behavior
in the vocabulary layer the floor invariant keeps out of it. So the shape is shared
and the types are not, which is the same forced duplication the election types
already carry across this corpus, named rather than resolved.

```graph
node: diagnostic-surface-mirrors-the-recorder
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-diagnostic
to: diagnostic-surface-mirrors-the-recorder
```

**No path-taking constructor.** The receive takes a descriptor and this crate holds
no name at any point, per the contract's section 6 and `weaver-harness-PRD`
section 5's handle discipline. The instrument is the compile-fail set the sibling
writer already runs: constructing a recorder from a `&str`, a `String`, or a
`PathBuf` each fails to compile.

```graph
node: diagnostic-no-path-taking-constructor
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-diagnostic
to: diagnostic-no-path-taking-constructor
```

**Admission precedes the write, and the sequence is gapless over the run's whole
admitted traffic.** A refused submission touches the sink at no point and consumes
no sequence, per the contract's section 2, so a gap in a record is a lost write and
never a refusal.

```graph
node: diagnostic-admission-precedes-the-write
kind: assertion
tag: perturbation

edge: asserts
from: weaver-diagnostic
to: diagnostic-admission-precedes-the-write
```

**No working structure, and the absence is the contract's clause represented.** This
crate holds no RAM copy of the record it writes, per the contract's section 2: a
replay's present is the holdings `weaver-state` serves through the replay ask, so a
second copy here would hold by one road what the loop already holds by another. The
instrument is the compile-fail absence of any accessor: no method on `Recorder`
yields a held event.

```graph
node: diagnostic-holds-no-working-structure
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-diagnostic
to: diagnostic-holds-no-working-structure
```

## 6. The failure vocabulary

Exhaustive, so a new case reaches every caller.

    pub enum Failure {
        SubmitRefused { refusal: SubmitRefusal },
        WriteFailed { error: WriteError },
    }

    pub enum SubmitRefusal {
        UnknownKind,
        PayloadMalformed,
        PayloadKindMismatch,
        RequiredFieldAbsent { field: FieldName },
    }

**A refusal has no effect on the sink and a write failure is terminal for the record**,
per the contract's section 5. What the harness does with either is its own, and this
crate reports rather than decides, which is the no-policy half of the charter's section
1. **It grounds in apex section 5.3, where `weaver-trace-Spec` section 9 grounds the
same claim, and on this document's own arithmetic rather than on that one's.** `Failure`
carries two cases here against that record's four, so what exhaustiveness buys is that a
third reaches every caller in the act that adds it. Section 5 of the contract names the
two, and an interface that named its errors and then let a third arrive silently would
have the hole that invariant forbids.

```graph
node: diagnostic-failure-enum-exhaustive
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-diagnostic
to: diagnostic-failure-enum-exhaustive

edge: grounds
from: diagnostic-failure-enum-exhaustive
to: axiom-contract-is-a-complete-interface
```

## 7. What is enforced, and by which instrument

**Enforced by the compiler.** The kind enum is exhaustive, so a kind added beyond
the set section 3.2 declares breaks every consumer's match. The count is not
restated here, a second copy of it being one more place for it to go stale. The
failure enum is exhaustive, so a new case reaches every caller. The receive shape
is read by a doctest, so an argument added to the one constructor stops the build
loudly.

**Enforced by compile-fail tests, because the property is an absence.** No
path-taking constructor, three named shapes. No accessor yielding a held event.

**Enforced by the manifest.** No `weaver-trace` dependency, read against the graph
under gate H2. No async runtime and no socket crate in the resolved tree.

**Requiring a perturbation-verified test.**

- Canonical form follows `weaver-trace-Spec` section 2: a shared kind's line
  compared byte for byte against the same event's serving line, watched to fail
  when either renderer's field order or number spelling moves.
- The session is the replay's own: a pass over a record of session `alpha` renders
  its envelope under the diagnostic run's session and names `alpha` in the identity
  payload alone, watched to fail when the two are crossed.
- The record identifies itself at the open: every bracket's first event is
  `replay.opened`, watched to fail when a pass authors any other kind first.
- No identity is invented: a pass whose replay answer never arrived authors no
  `replay.identity`, and neither does one whose step one read the holdings and
  refused them, watched to fail when either yields an event filled from defaults.
  The second is the case that bites, an absent answer having nothing to build from
  while a refused reading has everything and must still author nothing.
- An unaccounted input is refused: a source whose open recorded an
  `identity_prefix_unrecorded` fault, and one whose post-flush request has no seat
  recall before it on a record carrying the kind, each close abandoned at identity,
  watched to fail when either refusal is dropped from step one and when a later flush
  goes untracked once one recall has been seen.
- No outcome is manufactured: a pass ended without its closing event leaves an
  unclosed bracket, watched to fail when a death path authors a `replay.closed`.
- Admission precedes the write: a refused submission leaves the sink untouched and
  consumes no sequence, watched to fail when the refusal is moved after the write.
- Every kind carries its turn rule: a turnless kind refuses a turn, a turn-required
  kind refuses its absence, and a turn-optional kind is admitted both ways, each
  watched to fail when one kind is moved to another arm of the rule.
- The divergence position is the resident length at the draw: a re-fed answer whose
  first draw differs from the recorded path closes naming the position the closing
  count places that draw at, watched to fail when the pass names the draw's index in
  the turn's identifiers instead, which sits below by the input's length and the
  prefix.

**Enforced by review, one claim.** That the readout rides the measurement where a
serving record puts it is a claim about where a shape sits rather than a behavior a
run can falsify, and no instrument here reaches it: a test could confirm this
writer's placement and could not confirm it still matches the other's. The
byte-comparison above is the nearest thing and watches the line rather than the
member. **Review means the instrument was not bought and never that none exists**,
and what would buy it is a shared fixture the two writers render, which wants the
sibling crate's participation and is not this document's to elect.

**Where the records sit.** The assertion records are at the clauses that argue the
claims, across sections 1 through 6, rather than gathered here, per Document Format
section 6. Seventeen sit there and none sits here.

**Five of the seventeen take a second `asserts` edge, and it runs from
`weaver-harness`.** That the record identifies itself at the open, that an absent
identity is not invented, that an outcome is not manufactured, that a divergence
position is the resident length at the draw, and that an unaccounted input is refused
are five claims **this crate holds no instrument that can falsify**: the recorder
validates shape and never meaning, per `weaver-harness-diagnostic-contract` section 4,
and `weaver-diagnostic-PRD` section 1 has the harness author while this crate is the
mechanism it authors through. The instruments for all five sit in
`crates/weaver-harness/src/replay.rs`, so the second edge is what lets apex section 11's
chain close on the crate that holds them.
**The instrument is the discriminating test and the wording is not.**
`diagnostic-session-is-the-replays-own` is written the same way and takes no second
edge, correctly, its instrument standing in this crate's own `tests/recorder.rs`.

**Neither of the corpus's two rules for this shape reaches it, and the general rule is
owed to `WeaverTools-Document-Format` under issue #631.** `weaver-types-Spec` section
0 has one claim be one node with an `asserts` edge per crate bound by it, the node
living at "the statement both floor Specs share", and that qualifier is the
load-bearing half: it is what makes the tagging test a case where neither Spec owns
the node. There is no shared statement here, `weaver-harness-Spec` stating none of the
five. `weaver-traits-Spec` section 7 runs the other way, moving a record to the crate
whose suite holds its test because "an assertion belongs where its test lives", which
is how the licensed combinations and the close-on-exec test came to be declared in
`weaver-harness-Spec`, at its section 4 and its section 2.3 and discharged at its
section 8's sorting. **What moved there was the crate and never the placement.** Each
of the two landed at the clause arguing it, which is where this rule puts a record
either way, and the crate moved because the subject moved with it, a claim about the
harness's own refusal being the harness's to state. The subject of these five is this
record, which is this crate's, so the clause that argues them is here and only the
edge crosses. **That is a third case, and the Format does not carry it**, its section
4 stating no cardinality for `asserts` and no cross-document case, so until it does
this disposition is argued here and not ruled. `weaver-harness-Spec` section 8 states
the crossing from the side that holds the instruments, which is the half both rules
ask for either way.

**Which invariant each claim serves.** Three carry a `grounds` edge.
`axiom-floor-is-vocabulary-behavior-is-socket` is why this crate's manifest holds no
socket crate: a seam that crosses no process line has no socket to hold, and a
mechanism that acquired one would be claiming a boundary it does not have.
`axiom-contract-is-a-complete-interface` takes the two exhaustive enums, the kind
set of section 3.2 and the failure vocabulary of section 6, which
`weaver-trace-Spec` grounds there too. **The edges are owed on the parity rule
Document Format section 4 states and the arguments are this document's**, that
record's closure resting on a charter and this one's on the seam's own change
protocol, each argued at its clause. The other three axioms reach none of these
claims. **Fourteen claims grounding in no invariant
is the expected result and not a gap**, per Document Format section 4: most of this
document is representation, and representation is what the invariants are not
about.

## 8. Open elections

- **The diagnostic-trace's own instrument set**, beyond what a certification reads.
  `weaver-analysis-PRD` section 4 names the suite as a sketch that does not exist in
  this tree, and nothing here is built against it.
- **`residual.column`'s efficient encoding.** The kind crosses and records in
  the floor's provisional bare JSON today, and what stays open is the framing
  worth shipping at the column's volume. The record's encoding and the seam's
  framing are one question answered twice if settled apart: `weaver-spu-Spec`
  section 12 holds the seam's half, and both land in the act that measures
  the consumer draining them, per the same rule stated there.
- **The column fault's case.** Charter 13.10 owns the fault of a sampled
  position without its asked column, and this record's `Failure` enum cannot
  yet express it. The case lands with the code act that builds the seam,
  owed rather than seated now, a variant standing for what nothing yet
  produces being the reserved slot this Spec refuses elsewhere.

- **The satellite types.** `Sequence`, `Subsystem`'s spelling here, `FieldName`,
  `WriteError`, `AbandonReason`'s case set, and `RunRef`, `SessionRef`, and
  `TurnRef` as this crate's own newtypes over owned strings, on the same
  no-dependency ground the sibling writer states. Identifier choices with no
  cross-crate consequence, listed so what this Spec leaves to a builder is complete
  rather than implied.
- **The null replay** of the charter's section 4 is what certifies this mechanism,
  and it is owed behind this document rather than asserted in it.
