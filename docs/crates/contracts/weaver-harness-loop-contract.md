# weaver-harness / the loop - contract

**Status:** DRAFT. Act 1 of the loop, from the brief of 2026-11-25
(`docs/project/brief-2026-11-25-the-loop-seam.md`). It governs the loop seam: how the
operator's wires ask the harness, one socket, one closed list of asks. Merged on the
operator's call like every document.

**Date filed:** 2026-11-25
**Document ID:** `weaver-harness-loop-contract`
**Parent:** `weaver-agent-PRD`, section 5.5
**Editorial:** Per the Working Rules.

---

## 0. What this document is

The agreement over the loop seam: how a loop outside the worker hears a turn, how it
asks the harness, what each party may rely on, and how the seam fails. It is read
alongside `weaver-harness-Spec` section 6 (the ports) and the operator's rulings of
2026-11-11 (relay through the harness, a wire is one ask and one return).

**The harness is named first because it opens every pass.** The loop opens every ask
inside a pass, and nothing reaches the loop that the harness did not send.

**The seam is a second way to run the loop the worker already runs.** The worker hands
the harness one function, called once per turn with the granted `Ports` and the turn's
text (`lifecycle.rs`:663-673). Today that function is `dev_loop::drive`. The relay is a
second function of the same type: it answers the call by sending the turn to the loop
and calling, for each ask the loop makes, the same `Ports` method the compiled loop
calls. **The harness authors every event either way.** The loop authors nothing and
reaches nothing but this socket.

**This document does not draw `weaver-organ-channel`.** The loop is not an organ. It
holds no exchange of its own with admin or with any organ, and the organ envelope does
not cross here. The one piece of organ channel mechanics it takes, the segment series,
is stated in section 2 as its own.

**What act 1 builds against this document:** the relay in `weaver-harness`, the board
client and runner in Python, and their tests over a socket pair. A deployed agent still
runs the compiled loop until act 2. Where a clause names act 2 or act 3, act 1 builds
nothing for it.

## 1. The channel

- **Kind:** one connected `SOCK_SEQPACKET` pair, as every organ channel
  (`channel.rs`:160).
- **Place:** none. The pair has no path, so there is no mode or owner to get wrong.
- **Who makes it:** the worker, before the loop process exists (act 2). In act 1 the
  test makes it.
- **Who holds it:** the worker holds one end. The loop process holds the other as
  descriptor 3, and holds no other socket. Nothing else can open it.
- **Life:** one pair per load. When the worker closes its end, the loop reads end of
  file and exits. When the loop's end closes, the harness's next read fails (section 4).

## 2. The frames

Every message is one JSON object in UTF-8, the encoding the organ seams use. **Every
frame carries `kind`.** Each kind's members are fixed, and a member outside them is a
break (section 4). Integers are JSON integers in the range of an unsigned 64-bit number,
never a fraction or an exponent. Strings hold Unicode scalar values only: an escape that
decodes to a lone surrogate is a break.

### 2.1 The five kinds

| Kind | Direction | Members | When |
| --- | --- | --- | --- |
| `hello` | loop to harness | `version`, `wires_sha256` | once, the loop's first frame |
| `pass` | harness to loop | `pass`, `text` | a turn begins |
| `ask` | loop to harness | `pass`, `tag`, `ask`, and the ask's arguments (2.2) | inside a pass |
| `return` | harness to loop | `pass`, `tag`, `value` | answers one ask |
| `refuse` | loop to harness | `pass` | ends a pass with `ChannelLost` |

- **`hello`:** `version` is `"weaver-loop/1"`. `wires_sha256` is the lowercase hex
  sha256 of the bytes the runner read and compiled. The harness compares it with the
  digest it was given (by admin from act 2, by the test in act 1). The hello is read
  under the bound of section 5.
- **`pass`:** `pass` counts up from 1 within one load. `text` is the turn's text as the
  harness received it.
- **`ask`:** `tag` is unique within its pass. The loop chooses it.
- **`return`:** `value` is the port's answer in the form of 2.2. A return carries no
  ordering promise across tags (ruling 2 of 2026-11-11).

### 2.2 The closed list of asks, in the order of a turn

The order is the compiled loop's (`dev_loop/mod.rs`:125-200). Under `independent` a turn
asks rows 1, 2, 5 and 8, or 1, 2, 5 and 9 where the flush answers `null`. Under
`continuous` it asks rows 1 to 3, then row 4 on a later turn, then rows 5 and 6 under
pressure, then row 7 on a first turn with nothing seated, then row 8.

| # | `ask` | Arguments | `Ports` call | `value` | Event the call authors |
| --- | --- | --- | --- | --- | --- |
| 1 | `seated` | none | `seated()` | `true` or `false` | none |
| 2 | `turn_context` | none | `turn_context()` | `"continuous"` or `"independent"` | none |
| 3 | `first_turn` | none | `assembled()`, true where absent or empty | `true` or `false` | none |
| 4 | `fullness` | none | `fullness()` | `null`, or `{"resident": n, "capacity": n}` | none |
| 5 | `flush` | `keep`: integer | `flush(keep)` | `null`, or `{"resident_before": n, "resident_after": n}` | `flush`, or the refusal |
| 6 | `recall` | `last_turns`: integer or `null` | `recall(last_turns)` | `null`, or a list of recalled events (below) | `recall` |
| 7 | `shape` | none | `session_shape()` | `null`, or `{"runs": [{"run": s, "kinds": [[s, n], ...]}, ...]}` | none |
| 8 | `turn` | `delta`: a list of messages (below) | `turn(delta)` | no return, it ends the pass | the turn's whole bracket |
| - | (the frame `refuse`) | none | none, the entry answers `Err(ChannelLost)` | no return, it ends the pass | as today (section 4) |

- **A recalled event** is `{"kind": s, "run": s, "turn": s or null, "sequence": s,
  "pairs": [[key, value], ...]}`, the members of `Recalled` (`state.rs`:136-142). The
  pairs keep the member's order, and each value is the member's string unchanged.
- **The runs of a shape** keep the member's order, the order custody first saw them
  (`state.rs`:105-114), and each run's kinds keep theirs.
- **A delta** is a list of at least one message, each `{"role": r, "content": [{"type":
  "text", "text": s}, ...]}`, the serde form of `Message` (`weaver-traits`
  `message.rs`:38-60). `r` is `system` or `user`. No other role and no other block
  crosses. A delta outside this is a break.
- **The rows answered from the harness's memory** (1 to 4) exchange with no organ and
  author no event, exactly as they do for the compiled loop. The record rule of
  2026-11-11 (every read an event) lands first on the compiled loop, as its own act, and
  then on both loops at once.

### 2.3 What the seam does not carry

`Ports` offers more than the seat (`engine.rs`:674, :787, :822, :890, :1073). None of
these crosses, and an ask naming one is a break:

- `seed`, the seeding turn, which the harness runs from the operator's uid and never the
  loop.
- `elide` and `replay`.
- `classify` and `score`, which the seat does not offer (backlog B9).
- the stop directive, which acts inside `turn`.
- the wind-down turn of a graceful unload, which is the harness's own
  (`lifecycle.rs`:1443-1500, `engine.rs`:1105) and never reaches the loop.

### 2.4 Frames past the envelope: the segment series

One message on the pair holds at most `MAX_ENVELOPE_BYTES`, 64 KiB (`wire.rs`:34). A
frame whose serialized form is longer crosses as the segment series of
`weaver-types-Spec` section 4.4, in either direction: one preamble `{"segments": N,
"bytes": M}`, then exactly N slices of at most 64 KiB whose concatenation is the M bytes
of the frame. The preamble is known by its missing `kind`. M must exceed 64 KiB and must
not exceed `DECODE_MESSAGE_BOUND`, 8 MiB (`wire.rs`:42). N must be exactly M divided by
64 KiB, rounded up. A preamble that breaks either rule, or slices that total other than
M, is a break. The harness reuses its own segmenter and reassembler (`channel.rs`:1013,
:1025). The board client writes the same rules in Python.

**Why the series and not a larger message.** Three frames can pass 64 KiB. A `pass`
frame carries the gate's line, at most 49,041 bytes (`weaver-gate` `relay.rs`:51), and
its re-encoding can grow past the envelope where the text holds characters JSON escapes.
A `return` of `recall` or `shape` carries an answer the state seam reads up to 1 MiB a
line (`state.rs`:64, :657). A `turn` ask carries the text again with the re-entry. 8 MiB
covers the 1 MiB answer with room for its re-encoding, and the seam then has one bound
and one mechanism, both already tested on the decode seam.

## 3. Ordering and the state table

**The harness answers asks one at a time, in arrival order, in January.** The contract
promises no order across tags, so a later harness may batch up to four asks without a
new version.

### 3.1 The relay's states

| State | Meaning |
| --- | --- |
| `Unannounced` | the pair stands, no `hello` read yet |
| `Idle` | between passes |
| `Open(N, k)` | pass N is open with k asks outstanding |
| `Broken` | the seam broke, terminal for the load |

### 3.2 Events, by state

| State | Event | Caused by | Next state | The relay does |
| --- | --- | --- | --- | --- |
| `Unannounced` | a `hello` with the version and the digest it was given | loop | `Idle` | nothing more |
| `Unannounced` | any other frame, end of file, or no frame within the bound | loop | `Broken` | names the reason (section 4) |
| `Idle` | the harness calls the entry with a turn's text | harness | `Open(N+1, 0)` | sends `pass` |
| `Open(N, k)` | an `ask` of rows 1 to 7 for pass N, with a new tag, k < 4 | loop | `Open(N, k+1)`, then `Open(N, k)` on its return | calls the row's `Ports` method and sends `return` |
| `Open(N, 0)` | an `ask` of row 8 for pass N, with a new tag | loop | `Idle` | calls `turn(delta)` and the entry answers its result |
| `Open(N, 0)` | a `refuse` for pass N | loop | `Idle` | the entry answers `Err(ChannelLost)` |
| `Open(N, k)` | any frame section 4 names a break | loop | `Broken` | the entry answers `Err(ChannelLost)` |
| `Open(N, k)` | no frame within the bound of section 5 | loop | `Broken` | the entry answers `Err(ChannelLost)` |
| `Open(N, k)` | end of file or a failed read | loop | `Broken` | the entry answers `Err(ChannelLost)` |
| `Broken` | the harness calls the entry | harness | `Broken` | sends nothing and answers `Err(ChannelLost)` at once |

A frame the loop sends while the relay is `Idle` is read at the next pass's first read,
names an earlier pass, and is a break there.

### 3.3 Invariants

1. At most one pass is open, and passes are numbered 1, 2, 3 with no gap.
2. Every `return` answers one `ask` of the open pass, once.
3. At most four asks are outstanding.
4. A pass ends at exactly one of `turn` and `refuse`, sent with no other ask
   outstanding, and nothing of that pass crosses after it.
5. The relay calls no `Ports` method outside section 2.2, and calls each listed method
   with the arguments the ask carried.
6. Every event in the record is the harness's. A relayed ask authors exactly what the
   same `Ports` call authors when the compiled loop makes it.
7. `Broken` is terminal for the load. Nothing respawns the loop, and the agent has no
   supervisor.

## 4. Failure

**A loop that refuses, breaks this contract, dies, or stays silent past the bound is
`Err(TurnError::ChannelLost)` from the entry. Nothing else.** The harness then does what
it does today with that error (`lifecycle.rs`:1195-1196): where no organ hung up,
service ends and the worker exits (`bin/worker/main.rs`:116-119). Where an organ died in
the turn, the lifecycle table meets the death as it does for the compiled loop. There is
no new state and no new row in the lifecycle table (the Planner's ruling of 2026-11-25,
point 1). A gentler end is a later act, on the operator's word.

A refusal is the loop's own answer and leaves the relay `Idle`, as the compiled loop is
called again on the next turn where the run still stands. `independent`'s refusal
(`dev_loop/mod.rs`:146-148) is then the same in both loops. Every other row below leaves
the relay `Broken`.

The breaks, each named in one line on the worker's standard error as the relay meets it:

- a message that does not parse as JSON, or is not an object.
- a frame with no `kind` that is not exactly the preamble, or a series that breaks 2.4.
- an unknown `kind`, a member outside its kind's, a member missing, or a member of the
  wrong type.
- an integer outside 0 to 2^64 - 1, a fraction, or a lone surrogate in a string.
- a `hello` with another version or another digest, a second `hello`, or a frame before
  the `hello`.
- a frame naming a pass other than the open one.
- a tag used twice in one pass, or a fifth outstanding ask.
- an ask outside the closed list of 2.2, or a delta outside its form.
- `turn` or `refuse` with another ask outstanding.
- no frame within the bound of section 5, end of file, or a failed read.

The trace holds what it holds today for `ChannelLost`. The reason line goes to the
worker's standard error alone, since a trace kind for it would be a new event and
section 6's identity forbids one the compiled loop does not author.

## 5. The bound on a pass

**The relay waits at most 10 seconds for the loop's next frame**, timed from the frame
it last sent (the `pass` or a `return`), and for the `hello` from the moment the pair
stands. The harness's own time inside a `Ports` call is not the loop's and is not timed
by it. The bound's expiry is the `ChannelLost` of section 4.

The entry is called inside the turn's handling (`lifecycle.rs`:1134-1137), so a stop or
an unload waits on the loop until the bound passes. 10 seconds is the Planner's ruling
of 2026-11-25, to be measured against the slowest node a January loop holds. It is one
constant, `LOOP_FRAME_BOUND_MS`, beside `LOWER_BOUND_MS` in `weaver-types` `wire.rs`.

## 6. Identity

**The comparison.** Two records are identical when they hold the same events, in the
same order, with the same payloads, after exactly these differences are set aside, and
no others:

1. every envelope's `wall_ms` and `monotonic_ns`, which differ between any two runs.
2. the load event's `composer`, which names the loop that ran.
3. the identifiers minted per load and per save point, under one renaming applied to the
   whole record of each arm: the run reference admin mints from the clock and a nonce
   (`weaver-admin` `channel.rs`:740-748), the session's identifier where it is minted,
   and a save point's name and identifier. A renaming maps each identifier of one arm to
   exactly one of the other, at every place it appears, envelope and payload alike.

The declaration's digest (`event.rs`:749) is not set aside. The election of the loop
does not live in the declaration (the brief, point 6), so both arms load one declaration
and the digest is equal.

**A record holding a `RecorderCommitPressure` fault is outside the comparison.** The
fault reads the recorder's queue depth at the turn's end (`engine.rs`:162-188), which
depends on the writer's timing and not on the loop. The identity test keeps each session
short of the high-water mark (`weaver-trace` `writer.rs`:50, 768 queued records),
asserts that neither arm holds the fault, and repeats a run that does rather than
comparing it.

A difference outside the three is a failure of the test, never a field to add after the
fact. Adding to the list is a change of this contract.

## 7. What each party supplies and guarantees

- **The harness** answers every listed ask by the listed `Ports` call, authors what that
  call authors, keeps the bound, and refuses every break. It holds no opinion of the
  loop's content.
- **The loop** holds nothing between passes, and asks for what it needs. A node is a
  pure function of what crossed the socket in its pass: no clock, no randomness, no
  file, no network, no state kept at module level between passes (the rulings of
  2026-11-11, section 6.3, items 4 and 5). The runner checks the wire file's syntax tree
  for these before it runs anything, and the walls of act 2 hold what a syntax check
  cannot.
- **The runner** reads the wire file once, hashes those bytes, compiles from them and
  not from the path, sends the `hello`, and calls `on_turn(board, text)` once per pass.

## 8. Conformance: today's loop as wires

The fourth piece of the plan of 2026-11-25 is the compiled loop written as a wire file,
with the record shown identical by section 6, at the end of act 2. Its inputs name each
point below, and the wire file's constants equal the compiled loop's byte for byte,
which a test reads from both sides. Points marked unmeasured are measured in act 1
against the Rust function itself, never from this text.

**The constants:** `SYSTEM_PROMPT` (`dev_loop/mod.rs`, a Rust string continued across
lines, so the bytes are the joined sentence), `RECALL_TURNS` 4, `QUOTE_BOUND` 600, the
four-fifths threshold, the re-entry's opening sentence, the sentence for an unreachable
record, and the continuity sentence.

**The branches:**

- under `independent`, `first_turn` is not asked and the opening is composed as on a
  first turn every turn, with no continuity line. A flush answered `null` is a `refuse`.
- under `continuous`, a flush under pressure is asked and its answer ignored.
- pressure is `capacity > 0` and `resident * 5 >= capacity * 4`, each product saturating
  at 2^64 - 1 as Rust's `saturating_mul` does. Python's integers do not saturate, so the
  wires clamp both products (resident 2^62 against capacity 2^63 is pressure in Rust and
  not in plain Python).
- a re-entry opens with `SYSTEM_PROMPT` and a blank line only where nothing is seated.
- each message of the delta holds one text block.

**The quoting** (`dev_loop/mod.rs`:222-292):

- only `message.user` and `message.assistant` are quoted. Every other kind is skipped.
- the first pair keyed `content` is read, and the others are not.
- `content` is parsed as JSON as `serde_json::from_str::<Value>` parses it at the locked
  version. A text it refuses yields no line. Python's `json` module takes more: `NaN`
  and `Infinity`, a lone surrogate escape, nesting past serde's depth of 128, and a
  number past the range of a double. Each is an input of the test (unmeasured).
- a value that is not a list yields no line. An element that is not an object, or has no
  string `text`, is skipped.
- the budget is checked before each block, and a block met at a budget of 0 is skipped.
- words split on the Unicode `White_Space` property, Rust's `split_whitespace`. Python's
  `str.split()` also splits on U+001C to U+001F, which Rust keeps inside a word.
- a word's cost is its length in UTF-8 bytes, not in characters.
- the budget counts the words and not the single spaces that join them.
- a word longer than what is left sets the budget to 0, appends `" ..."` with its own
  leading space, and ends the block. A block whose first word is too long is the line `"
  ..."`.
- a word exactly as long as what is left is kept, and the next word, if any, then
  appends `" ..."`.
- a block of whitespace alone, or an empty `text`, yields an empty line, so the
  speaker's label stands alone.
- each line is a newline, the speaker, `": "` and the kept text.

**The continuity line** (`dev_loop/mod.rs`:306-327): the earlier runs are all but the
last, there is no line where the shape holds fewer than two runs, each earlier run
contributes its first `turn.closed` count or 0, and the two counts take `run` or `runs`
and `turn` or `turns` by whether each is exactly 1.

**The crossing.** Each string crosses the relay as the same Unicode text. A recalled
pair's value reaches the wires as the member's string, so the bytes the wires parse are
the bytes serde wrote.

## 9. Vocabulary

- **Pass:** one call of the entry, one turn's handling, from `pass` to `turn` or
  `refuse`.
- **Wire:** one `ask` and its `return`, under one tag. The harness sees wires and
  nothing else.
- **Node:** a Python function in the wire file that makes asks. `on_turn(board, text)`
  is the root node, called once per pass.
- **Board:** the loop's handle on its end of the pair, through which every wire runs.
- **Relay:** the harness's function that answers the entry by the loop.

## 10. What this document changes elsewhere

Nothing, when it lands. Each standing sentence that says the loop is compiled alone
changes in the act that makes the new sentence true, and each pull request's body lists
those it changes (the Planner's ruling of 2026-11-25, point 11). The list of candidates
is the brief's.
