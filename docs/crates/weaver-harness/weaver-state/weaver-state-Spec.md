# weaver-state - Spec

**Status:** MERGED. In `main` and the source of truth.

**Date filed:** 2026-08-18
**Document ID:** `weaver-state-Spec`
**Parent:** `weaver-state-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #740

---

## 0. What this document is

How the store primitive is represented: its process, its store and the save points
that carry it across loads, its territory, the slot for the loop's schema, and
the shapes both halves of the seam take. Written from the merged corpus
alone. The charter carries every why, and where reasoning appears here it
restates a charter clause and cites it. The serve half stood deliberately
absent until its first asker shaped it, per the charter's cell, and arrived
with the context-injection loop's act of 2026-08-19 at section 4.

## 1. The crate

A binary crate, one process per run of one agent, spawned at load and retired at
unload while its holdings stand in the latest save point, per `weaver-state-PRD`
sections 3 and 4. The charter's one sentence is asserted at the crate: custody without
policy. **The store initiates nothing and holds only its schema's shape**, and the
judging and the ranking are the loop's, made through the SPU and recorded on the
trace before they reach this crate as distillates, per the charter's section 2.
Review checks it by reading the crate's surface for any door a judgment could enter
by.

```graph
node: state-custody-without-policy
kind: assertion
tag: review

edge: asserts
from: weaver-state
to: state-custody-without-policy
```

**The manifest declares two internal crates in `[dependencies]`, `weaver-types`
and `weaver-traits`.** No unit here consumes `weaver-types`, this crate's tests
included, and charter section 5 keeps that consumer cell open. That floor link is
declared at charter section 1 and kept on the operator's ruling of 2026-09-14, a
link held for work not yet done being an election rather than a leftover.
`weaver-traits` has its consumer: `src/typed.rs` decodes a message kind's `role`
and `content` through `weaver_traits::Role` and `weaver_traits::ContentBlock`, the
types the harness renders them from, per section 3's typed landing, and charter
section 1 declares that floor link beside the first.

**`weaver-trace` stands in `[dev-dependencies]` for this crate's tests alone.**
They build the opener and distillate through `weaver_trace::opener` and
`weaver_trace::distill` rather than by hand. Production imports nothing from it,
and the dependency cell is closed by the operator's ruling of 2026-09-22,
recorded in charter section 5 under Working Process section 6.

**Dependencies, external.** One engine, behind the feature named for it: `rusqlite` with
its bundled engine, so the store's version is the build's fact rather than the host's,
its serialization interface carrying the save point, pinned by the lock file like every
dependency. **The `postgres` client and its feature retire** with the service engine, on
the operator's ruling of 2026-10-02 reversing #38, in the code act that removes the
engine. `serde_json` for the canonical event JSON the ingest reads. `nix` for the
preload door's credential check and for the descriptor handling both doors and the save
point's descriptor require. Nothing else: no async runtime, no logging crate, no HTTP,
per the corpus's standing refusals.

## 2. The process and its territory

The member runs under its own account, owning one subdirectory in the
operator-side territory where the session record lives, per the charter's
custody ruling. **The live store is not in that subdirectory**: it lives in the
member's memory. The subdirectory, the territory's `state/`, is the member's room: it
holds the preload door's name where one stands and **the save points the member
writes**, a new file each and never one rewritten, per the operator's ruling of
2026-10-02 on #58 and `weaver-state-PRD` section 4, and admin, as root, publishes each
finished one into the operator's declaration directory at the next load or unload.
**The save point a load restores arrives as a descriptor and never as a path**, per
`weaver-admin-Spec` section 6: admin opens the chosen save point in the operator's
directory for reading, the latest by default or the one `restore` names, and the member
inherits it at its spawn the way it inherits the first door's end. Where no save point
exists the descriptor is absent, which is an agent's first load or every save point
deleted, and the load of section 3 rebuilds. The descriptor's number is a fixed
convention between this crate and admin, the code act's to elect beside the first
door's, and it is probed before it is adopted, by the rule below, a regular file open
for reading. A live restore reads a save point from the member's own room by name, per
section 4, and needs no descriptor. The engine flag and the
service engine's three flags leave the vector with that engine, in the code act that
removes it, `weaver-admin-Spec` section 6 moving in the same act.

The first door's end arrives with the process, per the operator's ruling of 2026-08-26:
admin creates the pair at the spawn and this member inherits its end, so the peer is
authenticated by possession and no credential is judged on this door, the one party that
can hold the other end being the one the enter handed it to. The end's number is the
code act's to elect, a fixed convention between this crate and admin rather than a value
the vector carries, so the vector's positionals are the territory and the preload path
alone, the engine's flags standing ahead of them, per `weaver-admin-Spec` section 6.
**The number is probed before it is adopted**: the member reads the number's socket type
and refuses, with a named fault, a number holding no stream socket, because a hand-run
process holds whatever its shell left there and an adoption would read it as seam
traffic and close it on exit. The probe borrows and owns nothing, so the refusal closes
nothing that is not this process's own. The choreography election below is narrowed once
already by section 4 and now again by the ruling: what remains that act's is the number
and the probe's mechanics. The preload door's name arrives on the vector under a
diagnostic binding alone, the record restore of issue #432 retiring on the operator's
rulings of 2026-10-02 on #58, this member binding whatever name it is given and none it
is not, and binds under this member's own territory, the credential judgment of section
4 unchanged on it.

## 3. The store

**The store is a port and one engine stands behind it, per the operator's ruling of
2026-10-02 on issue #1.** `src/store.rs` declares `Store`, the port: open with the
loop's schema, load a save point, land a distillate whole, build the elected indexes,
answer the asks, and write a save point. The ingest and serve of section 4 speak to the
port and never to an engine, so the seam's traffic is the same whatever answers it.
**One engine stands**, `Sqlite`, an embedded database opened in memory in this process,
one database and never more, with no file of its own, no server, no network and no
pool. Its holdings reach the disk only as a save point, per the clause below. **The
service engine, `Postgres`, retires** with its module, its feature and its suites, in
the code act that removes it, the ruling of 2026-09-04 that elected it being reversed
because a shared database server breaks individuation, per the charter's section 4.
The port is the one place a query language is spelled, and a further engine is an
implementation of the port in its own act.

```graph
node: state-store-is-a-port
kind: assertion
tag: review

edge: asserts
from: weaver-state
to: state-store-is-a-port
```

**Two tables, and the shape is provisional with a stated trigger.** The
distillate lands as an event row and its elected pairs:

```sql
CREATE TABLE IF NOT EXISTS event (
    id       INTEGER PRIMARY KEY,
    session  TEXT NOT NULL,
    run      TEXT NOT NULL,
    turn     TEXT,
    kind     TEXT NOT NULL,
    sequence INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS field (
    event_id INTEGER NOT NULL REFERENCES event(id),
    key      TEXT NOT NULL,
    value    TEXT NOT NULL
);
```

The envelope's five ride the event row, `turn` nullable because the record
carries turnless events, and every elected pair is a field row holding the
key path and the value as the canonical JSON spelled it. This is the simplest
shape that keeps custody whole and every row attributable, and it is elected
against the ingest alone on purpose: the serve surface is unshaped, so a
shape optimized for queries nobody has asked would be optimizing a guess.
**The trigger is the serve act**: when the first asker's shapes land, this
election is reconsidered against real asks, and a build that has served real
asks without reconsidering has an open election reading as a settled one.
**The trigger fired 2026-08-19 with the shape ask, and the election
stands.** The first real ask groups the event table by run and kind, which
the two-table shape answers in one pass over rows a session keeps in the
low thousands, at a cadence of one ask per run's opening, so a query-side
reshaping would buy nothing measurable and the provisional shape is kept on
that ground. No index is added for it, by the same arithmetic. The next ask
that arrives reopens the question under the same clause.

**The election is reopened 2026-09-29 and a typed landing joins the two
tables**, on the operator's ruling of that date that the trace's payload is
typed where it is read rather than in the record. The asks standing since the
shape ask read what the typing reaches: the recall and the identity serve the
message kinds, the replay serves the measurement beside them, and the
context-management loop and the harness's enter read those answers as
messages. The two tables stand unchanged and four join them:

```sql
CREATE TABLE IF NOT EXISTS message (
    event_id INTEGER PRIMARY KEY REFERENCES event(id),
    role     TEXT,
    parts    INTEGER
);
CREATE TABLE IF NOT EXISTS part (
    event_id  INTEGER NOT NULL REFERENCES event(id),
    ordinal   INTEGER NOT NULL,
    block     TEXT NOT NULL,
    text      TEXT,
    name      TEXT,
    arguments TEXT,
    content   TEXT
);
CREATE TABLE IF NOT EXISTS measurement (
    event_id   INTEGER PRIMARY KEY REFERENCES event(id),
    perplexity REAL,
    entropies  INTEGER,
    surprisals INTEGER
);
CREATE TABLE IF NOT EXISTS series (
    event_id INTEGER NOT NULL REFERENCES event(id),
    member   TEXT NOT NULL,
    ordinal  INTEGER NOT NULL,
    value    REAL NOT NULL
);
```

**What lands typed is what
the seam's vocabulary names**, per `weaver-harness-state-contract`'s Vocabulary
clause. A message kind's `role` and `content` are read through the floor's
message model, `weaver_traits::Role` and `weaver_traits::ContentBlock`: the role
lands as the floor spells it, the content as a count of its blocks on the
`message` row, and each block as a `part` row carrying the floor's tag in
`block` and the columns its variant fills, `text` for a text block, `name` and
`arguments` for a tool call, `content` for a tool result. A measurement's
`perplexity`, `entropies` and `surprisals` are read member by member as the SPU
renders them, `weaver-spu-Spec` section 6 authoritative, and this crate defines
no measurement type: the perplexity is a double on the `measurement` row, and
each series is its length on that row with its readings as `series` rows in
decode order. **An absent member has no row, or a null column on a row a
sibling stood on, and never a zero**, so section 6's absent-not-empty property
of the SPU survives the landing: a measurement that crossed with no perplexity
holds a null and serves no perplexity member.

**A member lands typed only where its rows render back to the bytes that crossed, and
where every engine can hold the value.** `typed::split` renders each typed fragment
through the renderer every answer uses and compares it with the pair, and a value that
does not decode as its named type, or decodes and renders differently, lands as a
`field` row instead. So a typed row cannot serve a spelling the record did not hold, and
the contract's served-as-it-crossed clause holds by construction. A decoded string
carrying U+0000 lands verbatim too, in any string the message model holds, a rule the
retired service engine's `TEXT` forced and that stands unchanged until the act that
moves these tables into the loop's schema re-elects it. A block the floor adds after
this build, or a named member the SPU spells some other way, lands whole and verbatim
rather than typed in part or refused. The canonical form spells each value one way, so
what the tee sends from a record types where the vocabulary names it unless a string in
it carries U+0000, and the other fallbacks are reached only by frames the tee does not
produce.

**A pair lands one way and never both.** The `field` row and the typed rows
never hold one pair twice, so the elected index over `field` for a path that
lands typed indexes the fallback alone. The typed tables carry standing indexes
on their event keys, `part (event_id, ordinal)` and `series (event_id, member,
ordinal)`, built with the schema at open rather than from the election, because
what lands typed is the vocabulary's to name and not the load's. A store opened
before this election gains the four tables empty and serves its earlier rows
from `field` as it always did.

**The indexes are built at load from the election the seam's opener
carried**, per the contract's ingest clause: the opener arrives before the
first distillate on every standing of the channel, so a restarted member
rebuilds the identical index set before it holds a single new row. Per the
charter: the
envelope's standing indexes on `(run, turn)` and `(kind, sequence)`, and one
index per elected key path on `field (key, value)` filtered to that key,
so extension within a session is rows accumulating under standing indexes
and a new election is a new load's new index set. The engine's automatic
index machinery is not relied on, because an index that appears when a query
happens to want it is a cost landing mid-serve rather than at load.

```graph
node: state-indexes-built-at-load
kind: assertion
tag: perturbation

edge: asserts
from: weaver-state
to: state-indexes-built-at-load
```

**An elected index is named from its key path, and a name that cannot be made
is a refusal.** Two elected paths never share a name, so a later load's
differing election never falls silently under `CREATE INDEX IF NOT EXISTS`
beneath an earlier load's, which is the whole reason the name is derived from
the path rather than from the key's position in the election. Where an engine's
identifier limit cannot hold a path's name the load refuses the election with a
named fault, because building the subset that fits is the same silent loss read
from the other end. The encoding is the code act's under this election. **The
ceiling belongs to the engine and so does the encoding**, and sqlite sets no
identifier limit, so the refusal stands for an engine a later act integrates rather
than for this one.

**The typed tables are loop opinions held here until the loop's schema takes them**,
per the operator's ruling of 2026-10-02 on issue #1. The `message`, `part`,
`measurement` and `series` tables above, and the selection rules section 4's recall and
identity carry, the message kinds the recall reads and the newest run the identity
answers from, were each elected against a real ask, and each is a judgment about what a
turn needs rather than a mechanic of keeping. They leave custody into the loop and its
schema with the act that writes that schema, the asks that serve them moving with them
under the contract's change protocol, and until it lands the store holds and serves them
as this section and section 4 state. **The neutral substrate stays**: the `event` and
`field` tables carry the envelope and the elected pairs with no opinion in either, and
every schema the loop provides stands beside them rather than in their place.

**The schema slot.** The loop provides its schema and the store stands it at open: the
tables the loop's state takes, their keys and constraints, and for each distilled kind
the rows a distillate of that kind lands as. It crosses the seam in the opener, per
`weaver-harness-state-contract` section 2, so a restarted member stands the identical
schema with its reopened channel exactly as it builds the identical indexes. **What the
schema does not admit has nowhere to land**: a distillate whose landing the schema's
constraints refuse rolls back whole, by the transaction rule of section 4, and stands
in the trace and in the neutral substrate and nowhere else. The store applies the
schema mechanically and holds no opinion about it, so a schema is never this crate's
to write, repair or extend. **A save point carries the schema it was taken under**, and
a load whose opener carries a different schema is a save point that disagrees, so the
load refuses by the clause below and the offline builder rebuilds the holdings under the
new schema from the trace.

**Durability is the save point's, and the charter is the license.** The derivative is
rebuildable from the record and the session never depends on it, per the loss clause,
so the live store pays no disk write per landing: it holds its rows in memory, and the
crash cost is what landed since the latest save point, which the record keeps and no
default replays. **A save point is the whole database serialized**, written as a new
file in the member's room when the harness asks for one with the contract's `snapshot`
ask, at every leave and on the operator's demand, and never on the store's own
initiative. **It is stamped with the trace position it covers**, the run and the
sequence of the last distillate landed in it and the last turn that run's holdings
carry, and with a check over its own bytes, so a
save point written in part or damaged since reads as corrupt rather than as holdings,
and its finished name is given only once the write is whole, so a torn write leaves no
file under a finished name.

**A load restores a save point and replays no tail.** At the spawn the member reads the
save point through the descriptor into its in-memory database and holds its stamp, per
the operator's rulings of 2026-10-02 on #58. After an unclean stop that is the latest
known-good save point, and the harness records the reset on the trace, so a rebuild
honours it; nothing the record holds past the stamp lands by default. **A save point
whose bytes fail never reaches the member**: admin judges its check and stamp at the
inventory, per `weaver-admin-Spec` section 4, and refuses the load. **A save point that
disagrees with the opener's schema is refused on the `restored` ask**: the member holds
the restore's outcome from the opener on, and answers the enter's `restored` ask of the
contract's section 2 with the restored lineage or with a refusal naming the schema
mismatch, so the harness refuses the enter before it authors `load` and no `load` event
claims a lineage the holdings did not come from; the operator names another save point
through `restore` or rebuilds one. **Where the descriptor is absent the member
stands empty**, which is an agent's first load. **A rebuild from the trace is the
offline save-point builder's**, per `weaver-state-PRD` section 3 and the operator's
ruling of 2026-10-02 on #58: the builder lands the record through the preload door of
section 4 by the same path as every other landing, honouring every reset the record
names, and writes a save point the next load restores. A restore and a rebuild to the
same position arrive at the same holdings by construction, and section 5 names the
instrument that holds them equal. Which party runs the builder before a serving load
whose record holds events and no save point stands is section 6's to elect.

## 4. The ingest and the serve

The seam's traffic is the contract's `election` opener, its `distillate`
stream, and since 2026-08-19 its `ask` and `answer`, and this crate's half
is mechanical: parse, insert, index nothing per event, answer exactly what
was asked. **A distillate lands whole or not at all**: the
parse completes before any write, and the event row with its field and typed rows go
in as one transaction that rolls back entire on any failure, because a
distillate held in part would be an attributable envelope over missing
pairs, a corruption custody cannot detect later. A distillate that does not
parse is dropped whole, per the contract's malformed-row clause, and the
defect waits for the serve direction to give its surfacing a voice. Inserts
ride the sequence order the harness owes, and a gap in sequence is not this
crate's to notice: the record is the account
of what happened, and custody keeps what arrives.

```graph
node: state-distillate-lands-whole
kind: assertion
tag: perturbation

edge: asserts
from: weaver-state
to: state-distillate-lands-whole
```

**The serve half, shaped by its first asker per the charter's cell.** An
`ask` frame arriving on the stream is handled in stream order by the same
loop that lands distillates, which is what delivers the contract's
answered-against clause without a lock or an isolation level: the holdings at the
ask's position are the holdings, because nothing lands between reading the
ask and answering it.

**Every standing serve query restricts to the opener's session, and the restriction
is the query's rather than the caller's.** The contract's `election` carries
the session the load declared, per its 2026-08-20 amendment, and this crate
holds it for the channel's life and puts it in the `WHERE` of every read
below. It is stated here as a shape rather than left to a reader because
the defect it repairs was invisible: both queries once read the whole table,
answering across every session a store file had ever held, and the answers
looked perfectly well formed - a shape ask reporting a lifetime's runs as
this session's, and a recall reaching a fact the operator believed a session
cut had retired. Nothing surfaced it until a fresh session reported
twenty-eight earlier runs it never had. **A store holding more than one
session is the normal case rather than the broken one**, and since the operator's
ruling of 2026-10-02 it is the design: state carries across sessions under custody,
per `weaver-state-PRD` section 3, and the store holds one database for the agent with
no boundary of its own between its sessions. **So the restriction is each standing
ask's definition and not a wall of the store's**: the asks that read held events,
`shape`, `recall`, `replay` and `identity`, each answer within the declared session
because that is what each was elected to answer,
and an ask the loop's schema brings may reach across sessions where its definition
says so. Which sessions an answer reaches is a loop opinion like the typed tables of
section 3, and it moves with them.

```graph
node: state-serve-restricts-to-the-session
kind: assertion
tag: perturbation

edge: asserts
from: weaver-state
to: state-serve-restricts-to-the-session
```

The `shape` ask runs one grouped count over the event table, and the landing order the
contract's first-seen clause asks for is the `id` column's, custody's own order key: the
run groups are ordered by the least `id` each holds, each carrying its kinds and their
counts as the envelope spelled them, rendered as the contract's answer frame and written
back on the channel as one answer frame, the frame's byte shape riding the encoding
election of section 6. The `recall` ask reads the event rows of the four message kinds
and of `message.restored`, which records written before 2026-10-02 carry, with their
pairs, ordered by the `id` column like every landing-order answer, and where
`last-turns` bounds it the bound resolves as the distinct session, run, and turn triples
of the most recent turns by id, the rows outside them left unread, a turn label
recurring across runs naming two different turns. The answer serves each event as the
distillate's own shape, envelope and pairs, each pair the value that crossed: read back
from `field`, or rendered from section 3's typed rows through the renderer the landing
checked it against. One reader serves every answer, so the recall, the replay and the
identity cannot spell one event two ways. The `grants` ask reads no event row: it reads
the store's own boundary, which with the store in memory is the member's room, the
owner, group and mode of the territory's `state/`, read through the descriptor the
member holds for the room and never by path, and answers them in a fixed order as
`{"answer":{"grants":{"surface":[...]}}}`, each line a string, per the contract's fourth
ask of 2026-09-04. The `identity` ask reads the event rows of kind `message.system`
whose turn is absent and whose run is the run of the newest such row, ordered by the
`id` column, with their pairs, and answers them as
`{"answer":{"identity":{"messages":[...]}}}`, each the distillate's own shape, an empty
list where the session holds none, per the contract's fifth ask of 2026-09-04. A
malformed ask is dropped whole the way a malformed distillate is, and the resulting
silence is the harness's bound to convert into a missing answer. **The `restored` ask
answers the load's outcome**, per the contract's eighth ask of 2026-10-02: the member
judges the descriptor's save point against the opener's schema when the opener lands,
holds the outcome, and answers `{"answer":{"restored":{"lineage":{...}}}}` with the
stamp it restored, `{"answer":{"restored":{}}}` where it stood empty, or
`{"answer":{"restored":{"refused":"schema-mismatch"}}}`, immediately and parking never.
**The `snapshot` ask
writes a save point and answers its stamp**, per the contract's sixth ask of 2026-10-02:
the member serializes the whole database with its schema and its stamp, writes it as a
new file in its room, never over one that stands, and answers
`{"answer":{"snapshot":{"save-point":...,"run":...,"sequence":...,"digest":...}}}`
naming the file, the position it covers and the digest of its bytes, or drops the ask
unanswered where the write failed, the silence converting at the harness into a missing
answer like every other. It runs in stream order like every ask, so the save point
covers exactly the distillates the stream carried before it, and a write that failed
part way is what the load's check exists to catch. **The `restore` ask replaces the
holdings from a save point in the room**, per the contract's seventh ask of 2026-10-02:
the member opens the named file relative to its room's descriptor, refusing a name that
is not a plain entry of the room, checks it and its schema as a load does, and only then
swaps it in for the live database whole, answering `{"answer":{"restore":{...}}}`
carrying the `snapshot` answer's four members and `identity`, the prefix read from the
restored holdings as the `identity` ask reads it, or drops the ask unanswered with the
holdings as they stood. The distillates the stream carries after the ask land on the
restored holdings.

**Three protocol bounds are this crate's elections, each named with what its
breach means, per the audit of 2026-08-26.** The answer ceiling is one
mebibyte: an answer past it is not sent at all, silence the asking side's
bound converts into the missing answer per `weaver-harness-state-contract`
section 3, because custody never invents an answer shape for a fault - and
the ceiling is reachable on an ordinary session through `replay`, which
serves every held event, so a loop reading a missing answer there is told
the truth about the seam and nothing about the holdings. The inbound frame
cap is eight mebibytes: one frame larger is not the seam's traffic and the
door that carried it ends as closure, the peer holding a credential and not
a license to exhaust this process. The answer write deadline is two
seconds: a peer that takes nothing for that long has stopped reading, and a
custodian wedged on its behalf would cost the session its custody, so the
seam retires with the holdings standing. The values are this act's and a
measurement may move them, the meanings being the elections.

**The preload door lands its distillates through the same path the first door does, and
that is the mechanism of the contract's indistinguishability claim.** A distillate
arriving on the preload channel parses, transacts, and lands exactly as one arriving
from the tee, one code path and one store, so nothing marks how a holding arrived and
the serve restriction binds to the preload opener's session the way it binds to the
harness opener's. **The one act the preload path adds is the opener's retirement**:
receiving the preload election deletes the declared session's event, field and typed
rows in the same transaction that records the opener, before any distillate lands, per
the contract's section 2. The path is thereby idempotent at the preload grain -
re-running it replaces the session's holdings rather than appending to them - and a dead
driver's prefix needs no cleanup act, the next opener being the cleanup. The first
door's path performs no retirement and gains no branch: the delete hangs on the preload
opener alone. **A tail, where one is ever elected over a save point, is the one preload
that must not retire**, landing on top of the save point section 3 loaded, so its opener
names the position it follows and retires nothing, where the builder's rebuild opener
retires everything the member held and honours every reset the record names.
`weaver-analysis-state-contract` carries neither the tail opener nor the reset event
yet, and the charter's section 5 names both owed. What is new is the door's standing and
its judgment, and both are conditioned facts: the member binds the preload name only
where the party that stands it names one, and that party names it under a diagnostic
binding alone, the record restore of issue #432 retiring on the operator's rulings of
2026-10-02 on #58, holding the resolved kind from the inventory per `weaver-admin-Spec`
section 4. **That party is `weaver-admin` and the name rides the vector**, per that
Spec's section 6 as amended 2026-08-25, no exchange this member holds carrying a path.
**Section 2's election is narrowed rather than closed**: the descriptor choreography it
leaves to the code act is still that act's, and what is settled here is only that a name
arrives on the vector and not on a descriptor. The credential judgment is this member's
one, the first door authenticating by possession per the operator's ruling of
2026-08-26: the accept on the preload name admits the operator principal and refuses
every other peer before any byte is read, the agent's among them and no longer knowable
by number, the vector having dropped the agent's uid with the first door's judgment.

**The seal is a per-standing fact, held apart from the transport, and the
replay ask reads it alone.** The member holds, for its own standing's life,
whether a preload has sealed, per that contract's section 2, and the fact
is not the preload channel's openness: it is false before any dial, false
mid-stream, false after a sealless close, and true from the seal frame on.
Where the member stands with the preload door, a `replay` ask parks until
the fact is true, surviving the preload channel's close, answered at the
seal against the sealed holdings in one frame stream like any answer. **The
`identity` ask parks on the same fact**, per the contract's section 2: a
diagnostic load asks for its prefix at the enter, before the driver has
sealed, so the ask parks where the door stands and no seal has landed and
answers at the seal against the sealed holdings, the replay ask's
replacement rule reaching the replay ask alone. Where no door stands it
answers immediately. The door's standing is the fact the member reads, and
a diagnostic load's `identity` ask answered at the seal is the record's own
prefix, which is what `weaver-analysis-Spec` section 3 says the preloaded
store answers. The parked `recall` of the record restore retires with issue
#432's restore on the operator's rulings of 2026-10-02 on #58. **The
door itself survives the channel too**: on any close of the preload
channel the member unlinks and rebinds the name and the per-channel opener
state resets, per the contract's retry mechanism, while the seal fact
stands apart and is never reset, so a retry's opener retires the dead
prefix and a parked ask still answers only at a seal. A re-stand whose bind
fails logs the fault and the member serves on doorless, the derivative
degrading rather than the standing ending, where the same failure at the
initial stand is fatal because nothing has been served yet that a death
would interrupt. The
parking is the serve
loop's and blocks nothing else: distillates land and the other asks answer
while a replay ask waits, one parked slot per channel sufficing because a
newer replay ask replaces the parked one, per the contract's retry
mechanism, the replaced ask cleared unanswered and the seal answering
whatever the slot holds when it lands. Where the member stands without the
door, the ask
answers
immediately, the query being the recall's generalized past the
message kinds: every event row of the declared session with its pairs,
ordered by the `id` column, served as the distillate's own shape.

```graph
node: state-replay-answers-at-the-seal
kind: assertion
tag: perturbation

edge: asserts
from: weaver-state
to: state-replay-answers-at-the-seal
```

**The preload name's mode is this member's election.**
`UnixListener::bind` sets none, so the name would land at `0777 & ~umask` and
the door's permissions would be whatever umask this process inherited. The
bind happens under a umask denying every bit outside the owner, so the name
lands at `0700`.

**`0700` here where the gate's door is `0770`**, per `weaver-gate-Spec`
section 3. The accept below admits `uid() == 0` and no other, so no group
reaches this door and a mode granting one would describe access it does not
offer. The credential check is the lock that decides and this is the lock that
keeps a stranger from arriving at it, which is the two-locks reasoning
`weaver-harness-PRD` section 5 applies to the trace descriptor.

**Elected in the creating call and not on the path afterwards**, for the
reason the gate states: a mode set after the bind leaves the name live at the
inherited mode in between, races a path an unprivileged process may be able to
swap, and on failure leaves a file behind. The umask is process-global, so the
guard serializes on it.

```graph
node: state-preload-door-states-its-mode
kind: assertion
tag: perturbation

edge: asserts
from: weaver-state
to: state-preload-door-states-its-mode
```

**`state-preload-door-stands-only-diagnostic`, below, is one half of a two-sided
claim.** This crate's half is that the member binds no name it is not given. The other
half is `weaver-admin`'s, `admin-preload-name-follows-the-kind` at `weaver-admin-Spec`
section 6, which holds the vector in **both** directions: a serving inventory carries no
name, restoring or not, and a diagnostic one carries one. **The two records do not
divide the fact evenly.** This crate's covers what the member does with what it is
given, and the vector is entirely the other side's, because a member given a name binds
it and a member given none binds none, which is this record holding rather than failing
whichever way the name was wrong. The claim is recorded twice because the two crates'
behaviours are two facts, and the seam between them is the other record's alone.

**The identifier below still names the pair's claim and this half is narrower
than its name.** `state-preload-door-stands-only-diagnostic` reads as the whole
two-sided fact, and what this record now asserts is that the member binds no
name it is not given, the kind being the other half's to hold. A rename reaches
every document that cites it and the conformance header that will cite it from
code, so it is its own act and is named here as owed rather than taken in an act
about where an assertion sits.

```graph
node: state-preload-door-stands-only-diagnostic
kind: assertion
tag: perturbation

edge: asserts
from: weaver-state
to: state-preload-door-stands-only-diagnostic

node: state-preload-door-refuses-the-agent
kind: assertion
tag: perturbation

edge: asserts
from: weaver-state
to: state-preload-door-refuses-the-agent
```

**Transformation is chartered and the shape aggregate is its first
member.** The grouped count above is custody's derivation under the
charter's license: an organized envelope fact carrying no judgment about
what any count means to a turn. Further derivations land as further asks
name what they consume, for the reserved-slot reason: a derivation nothing
reads is a data-shaped empty joint.

## 5. What is enforced, and by which instrument

**This crate holds no `tests/` target** and every test it has stands in an
in-file `#[cfg(test)]` suite beside the unit it watches, so Document Format
section 5's review rule reaches this crate at the item where an instrument sits
rather than at a directory, which is the scope that rule states. **No claim here
is cited at a test.** Every `conforms:` line in the crate sits at a `//!` file
header but one, the line inside `stand_preload_name`, which is a function the
binary runs, so none of the nine is a sighting under that rule.

**Requiring a perturbation-verified test.** Eleven claims stand today, each watched
where the behaviour sits, and three join them with the store primitive, owed by the
code act that lands it.

- The standing asks restrict to the opener's session, watched by dropping any of the
  three `WHERE session` predicates the reads carry, which returns an earlier
  session's runs to a shape answer and an earlier session's rows to a recall. The
  claim is now each ask's definition rather than the store's boundary, per section
  4, and it moves with the asks when the loop's schema takes them.
- The indexes are built at load, watched by naming them positionally: a later
  load's differing election then falls under the earlier name through
  `CREATE INDEX IF NOT EXISTS` and one index stands where the election asks
  two. **The timing is the half no test reaches.** The build sits
  on the binary's startup path between the opener's parse and the serve loop,
  and every test that drives it calls the port directly, so what is watched is
  that the election's own indexes are built and not that they are built before
  the first distillate lands.
- The replay answers at the seal, watched by making the park ignore the seal,
  which answers the after-the-seal case from the wrong slot, and by making the
  ready check ignore it, which answers a sealless close over a prefix.
- The preload name states its mode, watched by dropping the owner-only umask
  from the bind, which leaves the name at whatever mode this process inherited.
- The member binds no name it is not given, watched where the name is decided
  rather than where it is bound: default the vector's preload positional and a
  serving load stands a door nothing should dial.
- The preload door refuses every peer but the operator, watched by dropping
  the root arm from the accept, which admits the agent's own uid.
- A distillate lands whole. Its watch stood at the service engine as of PR #644,
  a pair carrying a NUL that engine's `TEXT` refuses forcing the failure inside the
  transaction, and it leaves with that engine. **The embedded engine owes the
  watch**: the code act that retires the service engine forces a failure inside
  the embedded landing's transaction, the loop schema's own constraint refusing a
  row being the natural lever, requires the refusal and unchanged holdings, and
  perturbs by committing the event before its rows.
- A named member lands typed and serves what the record reads, watched by
  `recorded_lines_land_typed`. It distills four recorded lines through
  the tee, lands them, and requires the replay to serve every pair as it crossed,
  the served message read through the floor's type to equal the record's payload
  read the same way, and the typed tables to hold the rows. The perturbation makes
  the split type nothing: every answer still matches and the part count reads
  zero, so the test fails on the typing rather than on the bytes.
- A typed member serves only the bytes that crossed, watched by
  `what_cannot_land_typed_exactly_lands_verbatim`, which lands a spelling of each
  named member its type would change and requires it verbatim. The perturbation
  drops the render comparison from `typed::split`, and a perplexity crossing as
  `1.50` lands typed and would serve as `1.5`.
- An absent reading is absent and not zero, watched by
  `an_absent_reading_is_absent_and_not_zero` and by the null perplexity the
  recorded test lands. The perturbation defaults a measurement's missing
  perplexity to zero, and both fail.
- A string carrying U+0000 lands verbatim, watched by
  `a_nul_in_a_string_lands_verbatim` and `a_nul_in_a_message_lands_verbatim`,
  which lands a message whose text carries U+0000 and requires it served byte for
  byte from a `field` row with no part typed. The perturbation drops the holdable
  check from `typed::split`, and the embedded engine holds the part typed.
- **A reloaded store equals a full replay, and this is the store primitive's
  instrument**, per the operator's rulings of 2026-10-02 on issue #1 and #58. The
  test lands a recorded session's distillates, takes a save point part way, lands
  the rest, then stands a second store from the save point and a third from the
  record rebuilt through the save point's stamp with no save point, and requires the
  second and third to answer every ask alike and to hold the same rows, table by
  table, the loop's schema included; a live restore of the same save point on the
  first store must hold the same. The perturbations are the stamp written one
  position late and one position early, each naming a position the holdings do not
  cover, and a rebuild that ignores a reset the record names: each must fail the
  equality.
- A save point that fails its check is not loaded, watched at a live restore by one
  truncated part way and by one with a byte flipped, each of which must leave the
  holdings standing; at a load the same two are admin's to refuse, per
  `weaver-admin-Spec` section 4. The perturbation drops the check, and a torn save
  point is read as holdings.
- A save point under another schema is refused on the `restored` ask, watched by a
  load whose opener carries a schema the save point does not: the answer must name the
  mismatch. The perturbation answers the restored lineage regardless, and the enter
  stands on holdings the schema does not admit.
- A save point is never overwritten, watched by two `snapshot` asks on unchanged
  holdings answering two names with both files standing. The perturbation writes
  to a fixed name, and the first file is lost.
- What the schema does not admit lands nowhere, watched by a distillate whose
  landing the loop schema's constraint refuses: the landing rolls back whole and
  the neutral substrate holds the event. The perturbation drops the constraint from
  the schema the store stands, and the row lands.

**The service engine's suites retire with it.** The scratch PostgreSQL suites of
PR #644, the statement suite over the index naming, and the `--features postgres`
runs leave in the code act that removes the engine, and nothing here rests on them
after it.

**Enforced by review, and each clause names what would buy it.** Review here
means the instrument was not bought and never that none exists, per Document
Format section 5.

- **Custody without policy** is read off the crate's surface for any door a
  judgment could enter by, a save point taken unasked among them. The claim is an
  absence spread across a surface rather than a shape, so no single compile-fail
  pin names it and a test can only watch the doors that exist. What would buy it
  is a check over the crate's exported items refusing any that ranks, judges or
  initiates, which is a shape this corpus has no instance of, and naming it is
  what keeps the claim from reading as unbuyable.
- **The store is a port** and the nearest thing to an instrument is the
  signature: the ingest and the serve take the port behind a reference and can
  name no engine, so a call reaching past it would not compile as this code
  stands. Nothing holds that signature in place, so a later act widening one of
  them to a concrete engine compiles and this claim goes quiet. What would buy
  it is a compile-fail pin over an ingest path that names an engine.
- **The member sees a descriptor and never the operator's path** is read off the vector
  and the spawn: no positional or flag names a save point in the operator's directory,
  and the member opens files by name only inside its own room. What would buy it is a
  test over the vector admin composes refusing any member naming the declaration
  directory, which is admin's side and its Spec's.

**The walks the seam's conformance asks for are not in this tree.** The contract's
section 8 names them and says both directions land with the acts that open the seam and
shape the surface, which have landed: the election round trip and real events to
attributable rows against the living producer, the dead-peer clause watched by killing
the member mid-run and by asking with the member gone, the replay ask observed waiting
in all three unsealed states and its retry sequence, and the answered-against clause
read in time with asks interleaved among distillates. The suites here reach most of
those properties through the port or through the unit that holds them rather than across
the seam, which is the cheaper instrument and not the one the contract names, and **the
dead-peer clause is reached by nothing here in either direction**. The save points'
custody is owed the same kind of walk, the agent's uid asked to open one in the member's
room and in the operator's directory and refused, and the member's own process asked to
open the operator's copy by path and refused. The `grants` ask reports the room's owner
and mode and asserts nothing about either, so it is a surface for that walk rather than
the walk.

**Where the records sit.** The assertion records are at the clauses that argue
the claims, across sections 1 through 4 rather than gathered here, per Document
Format section 6. The three claims the store primitive adds carry no record, the
assertion record being required of nothing new until release.

## 6. Open elections

The serve surface's election closed 2026-08-19: its shape and vocabulary
landed in the contract, its query-side representation at section 4, and the
store shape's trigger fired and was answered at section 3, all elected
against the context-injection loop's real ask per the charter's cell. The
store shape was answered again 2026-09-29 with the typed landing, and the
operator's ruling of 2026-10-02 moves that landing toward the loop's schema.

- **The measurement's remaining members.** The SPU renders a generation's
  model, weights hash, token identifiers, prompt blocks, timings and residual
  reductions beside the three readings section 3 types, and `weaver-spu-Spec`
  section 6 spells none of their member names, so they land verbatim. Each types
  by section 3's rule once that section spells it and the contract's vocabulary
  names it, an act of the SPU's Spec and this seam's together rather than a name
  this crate takes from the SPU's code, and once the loop's schema holds the
  typed tables, an act of that schema's too.
- **The transformation vocabulary, beyond its first member.** The shape
  aggregate landed with the serve act, and which further derivations
  custody performs stays elected ask by ask, because a derivation is named
  by what reads it.
- **The save point's format and its write.** Section 3 fixes what a save point holds,
  the whole database with its schema, its stamp and a check, and that it is a new
  file never rewritten, and leaves to the code act how it is laid out, how its name
  is formed, what the check and the digest are, and how a whole write earns its
  finished name.
- **The builder's runner.** Section 3 has the offline save-point builder rebuild
  through the preload door, which stands otherwise only under a diagnostic binding
  and is dialed by the operator's driver, so which party runs the builder before a
  serving load whose record holds events and no save point, and how such a load is
  told from an agent's first load, is an election owed with
  `weaver-analysis-state-contract` and admin's vector, neither of which this act
  moves.
- **The retirement of an agent's holdings.** A session's close no longer retires
  anything, state carrying across sessions per the charter's section 3, so what
  remains is what an operator does to retire an agent's state: the save points are
  files in the operator's own directory and the member's room, and deleting them
  makes the next load a full rebuild rather than an empty store, the trace still
  holding everything; a fresh state is a new agent, on the operator's ruling of
  2026-10-02 on #58.
- **The member's account name and the territory's exact key.** Deployment
  facts, elected where the spawn path lands, the way every path in the
  admin configuration is.
- **The seam's encoding.** JSON as loop zero carries it, provisionally, per
  the same election `weaver-types-Spec` section 4.3 records for the decode
  seam: reconsidered when real traffic is measurable, and a build that has
  produced that traffic and not reconsidered has an open election reading
  as settled.
