# weaver-harness / weaver-trace - contract

**Status:** MERGED. In `main` and the source of truth. Written with
`weaver-trace-PRD` as one act, and ratified with the document set rather than
separately.

**Date filed:** 2026-07-29
**Document ID:** `weaver-harness-trace-contract`
**Parent:** `weaver-agent-PRD`, invariant 5.3
**Editorial:** Per the Working Rules.
**Landing PR:** #58

**This is a contract, not a Spec.** It states the protocol two parties agree to. It
names no Rust type, no module, and no function, because how either party implements
its side is that crate's Spec and no Spec is written until every PRD and every
contract is done.

---

## Parties

- **`weaver-harness`, the author.** Sole producer of trace content. Decides what is
  worth recording, when a session begins and ends, and what a turn is. Holds every
  handle.
- **`weaver-trace`, the recorder.** Assigns ordering, produces canonical form,
  holds the working structure, and hands the same rendering to the outbound
  stream. Holds no policy and decides nothing about content.

No third party writes. Components elsewhere in the system report to the harness and
the harness authors. A crate that emits directly into the trace is outside this
contract and is a defect.

**This seam is a library boundary, not a wire.** The harness links `weaver-trace` as a
crate rather than reaching it over a socket. A contract is not the same thing as a
socket, and this one governs an API surface rather than a protocol on a transport.

```graph
node: weaver-harness-trace-contract
kind: document

edge: party
from: weaver-harness-trace-contract
to: weaver-harness

edge: party
from: weaver-harness-trace-contract
to: weaver-trace
```

Nothing here points back at the seam. The seam is an edge declared once in
`weaver-harness-PRD` section 4 with this contract named in its `via` field, and an
edge cannot be the target of an edge. The two `party` records are what make the pair
checkable from this side.

## Vocabulary

Every contract names the vocabulary it depends on, grouped by the crate that
defines it. A contract without this clause is not a valid contract, and a group is
stated even when empty, because an explicit nothing is an assertion someone checked
and an absent group is silence.

**From `weaver-traits`.** The message model. The `message.system`,
`message.user`, `message.assistant`, `message.tool_result`,
and `message.restored` payloads carry conversation messages in whatever shape that
crate defines, and this contract does not redefine them. The harness draws it. The
recorder does not, because those five payloads are opaque to it per `weaver-trace-PRD`
section 3, so this name crosses the seam in one direction only.

**From `weaver-types`.** Nothing. The clause is present with that answer because
`weaver-types-PRD` section 5 asks for it even when it is empty. An earlier version
drew the verbosity ceiling election, which left the agent config entirely under the
ruling of 2026-08-02, and no other field of that artifact crosses this seam: the
recorder receives events and never reads the operator's declaration.

**From `weaver-trace`.** The event envelope and its field set, the closed event-kind
vocabulary, the per-kind payload shapes, the three commit-boundary states, the
working structure as the RAM copy of the same canonical form, and the failure
vocabulary of section 5. These
are the party's own definitions, named because they cross the seam and a reader of
this document should not have to infer where they come from.

**Nothing from any other crate.** This seam is a library boundary between two crates
and touches no third.

```graph
edge: draws
from: weaver-harness-trace-contract
to: message-model

edge: draws
from: weaver-harness-trace-contract
to: event-envelope

edge: draws
from: weaver-harness-trace-contract
to: event-kind-set

edge: draws
from: weaver-harness-trace-contract
to: payload-shapes

edge: draws
from: weaver-harness-trace-contract
to: commit-boundary-states

edge: draws
from: weaver-harness-trace-contract
to: working-structure

edge: draws
from: weaver-harness-trace-contract
to: failure-vocabulary
```

**No draw above dangles.** An earlier version of this clause drew a commit policy
from `weaver-types`, which defined no such field, and the edge was written anyway so
the mapping would fail visibly rather than behind a clean gate. The ruling that
settled it removed the field rather than seating it: `weaver-trace-PRD` section 4.2
fans one rendering into both sinks with no flush cadence to elect and no window to
tune, so there is no policy for any principal to hold.

---

**The envelope field set appears here and in `weaver-trace-PRD` section 3,
deliberately.** A Spec is derived from its crate's PRD plus every contract that crate
is party to, so the harness Spec writer reads this document and never opens the trace
charter. Pointing there instead of enumerating would break that derivation for the
crate on the other side of the seam. The two copies are not redundant, because the
charter says what an event **is** and this document says what the harness
**supplies**, but they describe the same field set and can diverge. **If they ever
disagree the charter is authoritative**, and the divergence is a defect to file
rather than one a reader resolves by choosing.

## 1. What this contract governs

The production seam: how an authored event becomes the outbound stream and a
queryable present, what each party guarantees to the other, and what happens when
either fails.

It does **not** govern the content of events, which is the harness's, nor the
internal storage mechanism, which is `weaver-trace`'s, nor how any consumer reads a
finished artifact, which is a separate agreement.

## 2. One exchange

The seam carried two until the cut of 2026-08-01. The resume exchange left with the
program-owned record: every run begins with an empty working structure, per
`weaver-trace-PRD` section 4.1, and there is nothing for the recorder to read back.
What remains is the exchange that happens once per event.

### 2.1 Emit

One authored event moves through four steps, in this order, always.

1. **Submit.** The harness offers an authored event carrying its kind, its payload,
   its session, run, and turn identity, its producing subsystem, its causal parent
   when it has one, and both timestamps of section 3. It does not carry a sequence
   number.
2. **Admit or refuse.** `weaver-trace` validates the envelope and admits the event,
   or refuses it with a named failure and no partial effect.
3. **Order and canonicalize.** An admitted event is assigned its sequence and
   rendered to canonical bytes once.
4. **Fan out.** That one rendering enters the working structure and is handed to the
   stream writer in the same act, and the assigned sequence returns to the harness.

**The order is the guarantee.** Admission precedes the fan-out, so neither
materialization can hold an event the other refused, and the account the operator
accumulates is the same admitted events the harness reasons over.

The working structure lands first and is the acknowledgment. The stream trails by
the depth of the writer's queue, per `weaver-trace-PRD` section 4.2, and process
death forfeits whatever that queue still held.

## 3. What the harness owes

**Authorship.** It submits only events it authored. When another component reports
something, the harness turns that report into an event and submits it. The reporting
component does not submit.

**Identity the recorder cannot derive.** The session, the run, the turn key, the
producing subsystem, and the causal parent when one applies. `weaver-trace` has no
notion of a turn or of a load episode and must not acquire one. A run is bracketed
by the harness authoring its `load` and `unload` events, so the recorder could not
infer a run boundary even if it tried, because nothing else writes one.

**Both timestamps, stamped at authoring.** The recorder's own clock would read
commit or receive time, which is a later and different quantity, so it does not own
these fields.

- **Wall-clock, millisecond resolution, session-scoped.** The calendar question,
  when in the day this happened. Millisecond is deliberate rather than lazy: it is
  all the calendar question needs, and holding the resolution there keeps an NTP
  correction jittering a low digit from ever looking like a real event. Wall-clock
  is never used for interval arithmetic, so a step or a backward correction cannot
  corrupt a latency reading.
- **Monotonic, nanosecond source with a microsecond floor, run-scoped.** The
  measurement instrument. Its origin is the run start, captured at the `load` event,
  because monotonic time counts from an arbitrary origin valid only within one
  process lifetime and a run is one process lifetime. The origin lands on an event
  that already exists for the bracket rule, so one point serves two purposes reached
  from different directions.

**Monotonic readings compare only within a single run.** Across a run boundary
monotonic is undefined, and the fallback is wall-clock at millisecond. The three
fields sort by scope with no overlap: sequence is session-wide and is the order,
wall-clock is session-wide and is the calendar, monotonic is run-scoped and is the
microsecond instrument. No field is ever read for the one job it cannot do.

**Descriptors, never paths.** Every write target is supplied as an already-open
handle, obtained from `weaver-admin` in the enter directive. The harness never
asks the recorder to open a path, and the recorder offers no way to.

**Everything authored, and no filtering at either end.** The harness submits every
event it authors and the recorder records every event it admits, per the ruling of
2026-08-02 that retired the recording levels. What an operator elects at load
governs what the agent produces rather than what reaches the record, so neither
party holds a level and neither drops an event for being one. A recorder that
dropped an event because it judged its kind would be taking a policy nobody holds.

An earlier version of this section carried both halves of a filter: the level named
under what the harness owes, and a rule that the harness does not submit events
above it. The two could not both be operative even before the ruling, since nothing
above an unelected ceiling was ever submitted for the recorder to apply a level to,
and the ruling removed the subject rather than adjudicating between them.

**Both layers of a decode, and neither standing for the other.** From the
token workflow's trace act of 2026-08-02: the harness submits the emission
verbatim as `model.output`, family-styled and before any parse, and the
canonical parse of it as the message kinds, and it submits both. A harness
that submitted only the parse would leave the record unable to answer what
the model said, and one that submitted only the verbatim would make every
consumer a parser of every family. The recorder judges neither, per section
6, so this is an obligation on the party that can meet it.

**The rendered form and its template identity, with `model.request`.** The
harness submits what the model was asked this turn, the turn's delta as
rendered and the identity of the template that produced it, beside the
effective sampling values for the turn, per the operator's ruling of
2026-08-12: the seam is append-only, so the full effective context is the
accumulation the record determines rather than a quantity any one event
stores. It has the rendered form because the
family library returns it on the report path, per `weaver-spu-PRD` section
13.4, and it submits it because a record holding only canonical messages
forces a replay through a template that may have changed, per
`weaver-trace-PRD` section 3.2.

**Every judgment the loop makes, recorded.** The loop is state management and state is
rebuilt from the record alone, per `weaver-agent-PRD` section 5.5, so the harness
authors every judgment its loop makes before the loop acts on it, a model-made
classification or ranking as surely as a flush or a verdict, on the precedents of
`classify.request`, `classify.output` and `score`. A judgment whose kind the closed set
does not yet carry enters by section 7 with the loop act that makes it.

**Faults as the floor's report.** A fault the worker survives is submitted as
a `fault` event carrying `weaver-types`' `fault-report`, the same shape the
reporting organ handed the harness, against the case set closed across the
three organs on 2026-08-02. The harness authors it and does not translate it.

**The close kind on `turn.closed`.** The payload carries which kind of close it was,
clean or stopped, and a stopped close carries the reason, authored when the abort of
`weaver-admin-harness-contract` section 3's stop exchange lands. The charter is
authoritative for the shape, per the divergence rule of the vocabulary clause.

**Handling refusal.** A refused event is not emitted. The harness must not project
it, must not treat it as recorded, and must not retry it under a new sequence as
though it were a new occurrence.

## 4. What the recorder owes

**Ordering.** Sequence numbers are assigned by the recorder, strictly increasing
**within the run**, with no duplicates and no gaps among admitted events. The scope
narrowed from the session on 2026-08-01, because a recorder that begins every run
empty holds nothing to continue numbering from. Session-wide order is the pair of
admin's run reference and the sequence, assembled by the consumer, per
`weaver-trace-PRD` section 6. The reference distinguishes runs and orders them
by the operator's clock rather than monotonically, so the pair is an ordering
between runs and the sequence is the ordering inside one. Reads follow sequence
order.

**Canonical form.** One byte-form rule for every artifact it writes. Integer fields
that can exceed the double-safe range are written as decimal strings, so a consumer
parsing numbers as doubles cannot read a silently different value.

**A commit boundary that can be interrogated.** The last sequence handed to the
sink, the last admitted sequence, the current depth of the writer's queue, and the
last stream-write error when one exists. The queue depth is reported rather than
bounded, because the bound is the deployment's and not the recorder's.

**No silent loss.** When the stream cannot keep pace, the recorder surfaces the
pressure rather than absorbing it, a write that fails under a live process is named,
and what may happen next is the marked election of `weaver-admin-operator-contract`
section 3. It never drops an admitted event quietly. A tail forfeited to process
death is outside this obligation, because the process that would report it is the
one that died.

**One rendering, held and handed.** The working structure holds the canonical
form the stream carries, byte-identical, per the ruling of 2026-08-01 that retired
the relational projection, so nothing exists between the two to reconcile.

**Typed failure.** Every refusal is named. Nothing returns a partial result with a
success status.

## 5. Failure vocabulary

Named here so both parties agree on what each means. The representation is each
crate's Spec.

- **Refused on submit.** The event was not admitted and has no effect. Sub-cases:
  unknown kind, payload does not decode for its kind, required envelope field absent.
  A sequence conflict is not among them. Section 2.2 has the submission carry no
  sequence and assigns one after admission, so at submit time there is nothing to
  conflict with. A duplicate sequence is reachable on read, where section 4 checks
  for it, and that is a different failure.
- **Commit failed.** The event was admitted and the stream writer could not hand it
  to the sink. The session is failed rather than continued.
**Commit pressure is not among them, as of 2026-08-22.** The stream failing
to keep pace while the sink stays writable is a condition of the recorder
rather than an outcome of a submission: the event was admitted, it reached
the working structure and the writer's queue, and it is not lost. It was
enumerated here as a failure and returned as one, so a party acting on it
treated a recorded event as an event that had not landed. **The depth is
read from the recorder instead**, and the harness authors the `fault` naming
`RecorderCommitPressure` on the crossing, which is what the conformance
clause below has always asked for. A sink that stops being writable is a
failure and stays one, under commit failed.

- **Append failed after admission.** The working structure could not take the
  event, one side of the fan-out failing against the other. **The recorder owns
  the resolution:** fail loudly. It must never continue against a silently stale
  present, and there is no record to rebuild from since the cut of 2026-08-01.
- **Write target unusable.** A handle is closed, is not writable, or does not
  satisfy the required flags.

Three entries left this vocabulary with the cut of 2026-08-01, unload divergence,
record invalid on read, and resume refused, each presupposing a program-owned record
to read back.

## 6. What neither party may do

**The recorder may not hold policy.** It does not decide what is worth recording,
what constitutes a turn or a session. A recorder that acquires either of these has
taken cognition into the floor.

**The recorder may not offer a path-taking write surface.** Not as a convenience,
not for tests, not behind a feature. A path-taking function is a way around the
custody boundary, and its existence is the defect regardless of who calls it.

**The harness may not reach the sink directly.** It writes through the recorder
and reads through the working structure. It does not open, seek, or inspect what
stands behind the handle, and it holds no path with which it could.

**Neither may write a second authoritative representation.** Derived views are
permitted when they name the committed source range they represent and do not claim
completeness beyond it. A derived view is never a durable home. The divergence
artifact an earlier version excepted here left with the leave-time comparison on
2026-08-01, there being no program-owned record for the working structure to
diverge from. **The state member's save point is a derived view of the kind this rule
permits and stands outside this seam**: it names the trace position it covers, claims
nothing past it, and yields to the record wherever the two disagree, per
`weaver-state-PRD` section 4. It is durable as a cache is and never a home, every
holding it carries being rebuildable from the record.

**Neither may make a span durable.** Spans are views over event ranges, produced on
demand. Writing a span into the record makes it a primitive and reintroduces the
duplication this seam exists to prevent.

## 7. Change protocol

This contract is one agreement. A change to the exchange in section 2, to either
party's obligations, to the ordering guarantee, or to the failure vocabulary is one
edit reaching both parties in the same act, together with any Spec written against
it.

Adding an event kind is a change to `weaver-trace-PRD` and to this contract, because
the kind set is closed and consumers key on it.

## 8. Conformance

How each check is implemented is Spec work. What must be checkable is stated here.

- A refused submission leaves no event in the working structure and no line on
  the stream.
- Admitted sequence is monotonic and gapless under concurrent authoring during a
  single run, and every run's numbering begins fresh.
- An append failure after admission fails loudly and never continues stale.
- A monotonic reading is never compared across a run boundary.
- The stream receives whole events and never a partial one.
- What reaches the stream is byte-identical to the canonical rendering the working
  structure holds, one rendering feeding both.
- No write surface accepts a path. This is a compile-time property and is pinned as
  one, because a runtime test cannot demonstrate the absence of a function.
- **The sink handle is withheld from the harness's children.** A handle the harness
  receives gains that property at the receive site, which is the only place a receiver
  can supply it. A handle the harness creates has it from creation. This seam supplies
  the first case and `weaver-organ-channel` section 2 governs the second.
- Commit pressure is surfaced as an event while the sink remains writable and is
  fatal when it is not. **The surfacing is the harness's authored `fault` and
  never a returned failure**, the submission it followed having landed.
- Every `turn.closed` states its close kind, clean or stopped with reason.
- A decode reaches the record as both layers, the verbatim emission and the
  canonical parse, neither standing for the other.
- A measurement's per-token vectors are positionally paired with its token
  identifiers, and an unproduced reading is absent rather than zero.
- No stored span appears on the stream.
