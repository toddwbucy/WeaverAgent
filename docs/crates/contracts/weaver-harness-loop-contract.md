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
is stated in section 2.4 as its own.

**What act 1 builds against this document:** the relay in `weaver-harness`, the board
client and runner in Python, and their tests over a socket pair. A deployed agent still
runs the compiled loop until act 2. Where a clause names act 2 or act 3, act 1 builds
nothing for it.

**The relay names only what it can see.** Every break in section 4 is one the relay
observes on its own end, and each has a test that fails when the relay stops refusing
it. A rule the relay cannot observe is the loop's, kept and tested in the board client.

## 1. The channel

- **Kind:** one connected `SOCK_SEQPACKET` pair, as every organ channel
  (`channel.rs`:160).
- **Place:** none. The pair has no path, so there is no mode or owner to get wrong.
- **Who makes it:** the worker, before the loop process exists (act 2). In act 1 the
  test makes it.
- **Who holds it:** the worker holds one end. The loop process holds the other as
  descriptor 3, and holds no other socket. Nothing else can open it.
- **Life:** one pair per load. When the worker closes its end, the loop reads end of
  file and exits. When the loop's end closes, the relay's next read or send fails
  (section 4).

## 2. The frames

Every message is one JSON object in UTF-8, the encoding the organ seams use. **Every
frame carries `kind`.** Each kind's members are fixed. A member outside them, a member
missing, or a member named twice in one object is a break, on both sides. Python's
`json` keeps the last of two members of one name unless told otherwise, so the board
client refuses a repeated member by its own code.

**Integers** are JSON integers from 0 to 2^64 - 1, written as digits with no sign, no
fraction and no exponent. `-0`, `1.0`, `1e2` and `true` where an integer stands are each
a break. A derive over `u64` takes `-0`, so the relay checks integers by hand, and the
board client reads each integer from its literal text. **Strings** hold Unicode scalar
values only: an escape that decodes to a lone surrogate is a break.

### 2.1 The five kinds

| Kind | Direction | Members | When |
| --- | --- | --- | --- |
| `hello` | loop to harness | `version`, `wires_sha256` | once, the loop's first frame, read when the relay is made |
| `pass` | harness to loop | `pass`, `text` | a turn begins |
| `ask` | loop to harness | `pass`, `tag`, `ask`, and the ask's arguments (2.2) | inside a pass |
| `return` | harness to loop | `pass`, `tag`, `value` | answers one ask |
| `refuse` | loop to harness | `pass` | ends a pass with `ChannelLost` |

- **`hello`:** `version` is `"weaver-loop/1"`. `wires_sha256` is the lowercase hex
  sha256 of the bytes the runner read and compiled, 64 characters. The relay compares it
  with the digest it was given (by admin from act 2, by the test in act 1).
- **`pass`:** `pass` is an integer, counting up from 1 within one load. `text` is the
  turn's text as the harness received it.
- **`ask`:** `tag` is an integer, 0 to 2^64 - 1, chosen by the loop and unique within
  its pass.
- **`return`:** `value` is the port's answer in the form of 2.2. A return carries no
  ordering promise across tags (ruling 2 of 2026-11-11).

The loop sends only `hello`, `ask` and `refuse`. A `pass` or a `return` from the loop is
a break, though each is a known kind with valid members.

### 2.2 The closed list of asks, in the order of a turn

The order is the compiled loop's (`dev_loop/mod.rs`:125-200). Under `independent` a turn
asks rows 1, 2, 5 and 8, or rows 1, 2, 5 and 9 where the flush answers `null`. Under
`continuous` it asks rows 1 to 3, then row 4 on a later turn, then rows 5 and 6 under
pressure, then row 7 on a first turn with nothing seated, then row 8.

| # | `ask` | Arguments | `Ports` call | `value` | Event the call authors |
| --- | --- | --- | --- | --- | --- |
| 1 | `seated` | none | `seated()` | `true` or `false` | none |
| 2 | `turn_context` | none | `turn_context()` | `"continuous"` or `"independent"` | none |
| 3 | `first_turn` | none | `assembled()`, true where absent or empty | `true` or `false` | none |
| 4 | `fullness` | none | `fullness()` | `null`, or `{"resident": n, "capacity": n}` | none |
| 5 | `flush` | `keep`: integer | `flush(keep)` | `null`, or `{"resident_before": n, "resident_after": n}` | `flush`, or the refusal |
| 6 | `recall` | `last_turns`: integer or `null`, present either way | `recall(last_turns)` | `null`, or a list of recalled events (below) | `recall` |
| 7 | `shape` | none | `session_shape()` | `null`, or `{"runs": [{"run": s, "kinds": [[s, n], ...]}, ...]}` | none |
| 8 | `turn` | `delta`: a list of messages (below) | `turn(delta)` | no return, it ends the pass | the turn's whole bracket |
| 9 | the frame `refuse`, not an ask | none | none, the entry answers `Err(ChannelLost)` | no return, it ends the pass | as today (section 4) |

- **Row 6's argument is present.** `last_turns` absent is a break. A derive over
  `Option<u64>` takes a missing member as `None`, so the relay checks its presence by
  hand.
- **A recalled event** is `{"kind": s, "run": s, "turn": s or null, "sequence": s,
  "pairs": [[key, value], ...]}`, the members of `Recalled` (`state.rs`:136-142). The
  pairs keep the member's order, and each value is the member's string unchanged.
- **The runs of a shape** keep the member's order, the order custody first saw them
  (`state.rs`:105-114), and each run's kinds keep theirs.
- **The rows answered from the harness's memory** (1 to 4) exchange with no organ and
  author no event, exactly as they do for the compiled loop. The record rule of
  2026-11-11 (every read an event) lands first on the compiled loop, as its own act, and
  then on both loops at once.

**A delta, checked by hand.** `Message`'s derive (`weaver-traits` `message.rs`:24-75)
refuses no unknown member and takes roles, blocks and empty lists this seam does not, so
the relay reads the delta itself. A delta is a list of at least one message. Each
message is an object of exactly `role` and `content`. `role` is `"system"` or `"user"`.
`content` is a list of at least one block, and each block is an object of exactly
`type`, which is `"text"`, and `text`, a string. **An empty `text` is legal**, since the
turn's text may be empty and the compiled loop sends it unchanged. Each of these is a
break where it fails:

- an empty delta, or a delta that is not a list.
- a message with a member other than `role` and `content`, or missing one.
- the role `assistant`, `tool_result` or any other but `system` and `user`.
- an empty `content` list.
- a block of another `type`, a block with another member, or a `text` that is not a
  string.

### 2.3 What the seam does not carry

`Ports` offers more than the seat (`engine.rs`:674, :787, :822, :890, :1073). None of
these crosses, and an ask naming one is a break:

- `seed`, the seeding turn, which the harness runs from the operator's uid and never the
  loop.
- `elide` and `replay`.
- `classify` and `score`, which the seat does not offer (backlog B9).
- The stop directive, which acts inside `turn`.
- The wind-down turn of a graceful unload, which is the harness's own
  (`lifecycle.rs`:1443-1500, `engine.rs`:1105) and never reaches the loop.

### 2.4 Frames past the envelope: the segment series

One message on the pair holds at most `MAX_ENVELOPE_BYTES`, 64 KiB (`wire.rs`:34). A
frame whose serialized form is longer crosses as the segment series of
`weaver-types-Spec` section 4.4, in either direction: one preamble `{"segments": N,
"bytes": M}`, then exactly N slices of at most 64 KiB whose concatenation is the M bytes
of the frame. The preamble is known by its missing `kind`. M must exceed 64 KiB and must
not exceed `DECODE_MESSAGE_BOUND`, 8 MiB (`wire.rs`:42). N must be exactly M divided by
64 KiB, rounded up. **A series is contiguous:** its sender sends nothing between the
preamble and the last slice. Each slice is read and sent under the bound of section 5. A
preamble that breaks a rule, a slice that does not come in time, or slices that total
other than M is a break.

**Which frames need it.** A `return` of `recall` or `shape` can, since the state seam
reads an answer line of up to 1 MiB (`state.rs`:64, :657), and so can a `turn` ask,
which carries a re-entry and the turn's text. **A `pass` frame never does.** Its text is
the decoded member of a gate line of at most 49,041 bytes (`weaver-gate` `relay.rs`:51),
and `serde_json` writes the shortest escape for every character, so no text re-encodes
longer than it arrived. Encoding the worst lines (text of `U+0001`, of `"`, of a
four-byte character, and of ASCII, each at the largest length the line bound takes)
gives `pass` frames of 49,079 to 49,083 bytes with the pass number at 2^64 - 1. The
relay still sends any frame over 64 KiB as a series, so the rule has no exception.

**The harness's side.** The series' writer today is `send_decode`
(`channel.rs`:789-812), which serializes a `TokenDirective` and then segments, and the
reassembler is `reassemble_if_series` (:1025), over bytes. Both are private to
`channel`, and the octet reader and writer (:1013, :1063) wait without a deadline. Act 1
moves the segmenting half of `send_decode` into a function over bytes that takes the
send as a closure, which `send_decode` then calls, and makes it and
`reassemble_if_series` visible in the crate. The relay supplies its own send and
receive, which wait under the pass's deadline. Nothing else in `channel.rs` changes.

## 3. Ordering and the state table

**The relay answers asks one at a time, in arrival order, and counts nothing
outstanding.** Since it answers each ask before it reads the next, it holds no ask
outstanding at any read, and cannot see how many the loop has sent ahead. The limit of
four outstanding asks (ruling 2 of 2026-11-11) and the rule that `turn` and `refuse` go
with no other ask outstanding are the loop's, kept by the board client and tested there.
A later relay that batches may enforce them, without a new version.

**At most 64 asks in a pass**, the constant `LOOP_PASS_ASKS`. The relay counts the asks
of the open pass, and the 65th is a break. Today's loop asks seven at most.

### 3.1 The relay's states

| State | Meaning |
| --- | --- |
| `Unannounced` | the relay is being made and has not read the `hello` |
| `Idle` | between passes |
| `Open(N, a)` | pass N is open and has asked `a` times |
| `Broken` | the seam broke, terminal for the load |

**The `hello` is read when the relay is made, before any pass.** Making the relay reads
the `hello` under its own bound of section 5, and gives either a relay that is `Idle` or
an error. The harness is never handed a relay that is `Unannounced`. **A failed `hello`
refuses the load**, as a loop folder that fails its check does: the worker kills and
reaps the loop at once and the load rolls back (`weaver-harness-Spec` section 2.5).

### 3.2 Events, by state

| State | Event | Caused by | Next state | The relay does |
| --- | --- | --- | --- | --- |
| `Unannounced` | a `hello` with the version and the digest it was given | loop | `Idle` | the making succeeds |
| `Unannounced` | any other frame, end of file, a failed read, or no frame within the bound | loop | none | the making fails, naming the reason |
| `Idle` | the harness calls the entry, and a look at the socket without waiting finds nothing | harness | `Open(N+1, 0)` | sends `pass` |
| `Idle` | the harness calls the entry, and the look finds a frame or end of file | loop | `Broken` | the entry answers `Err(ChannelLost)` |
| `Open(N, a)` | an `ask` of rows 1 to 7 for pass N, with a tag unused in pass N, a < 64 | loop | `Open(N, a+1)` | calls the row's `Ports` method and sends `return` |
| `Open(N, a)` | an `ask` of row 8 for pass N, with a tag unused in pass N, a < 64 | loop | `Idle` | calls `turn(delta)`, and the entry answers its result unchanged |
| `Open(N, a)` | a `refuse` for pass N | loop | `Idle` | the entry answers `Err(ChannelLost)` |
| `Open(N, a)` | any frame section 4 names a break | loop | `Broken` | the entry answers `Err(ChannelLost)` |
| `Open(N, a)` | the pass's bound expires, in a read or in a send | loop | `Broken` | the entry answers `Err(ChannelLost)` |
| `Open(N, a)` | end of file, a failed read, or a failed or unfinished send | loop | `Broken` | the entry answers `Err(ChannelLost)` |
| `Broken` | the harness calls the entry | harness | `Broken` | sends nothing and answers `Err(ChannelLost)` at once |

**The look before a pass.** A frame the loop sends while the relay is `Idle` stands on
the socket when the next pass opens, and the look finds it, whatever pass it names. A
frame the loop sends after the look and before it reads the `pass` cannot be told apart
from an ask of the new pass: where it names pass N+1 it is taken as one, and nothing in
the record tells it apart. The loop cannot know the new pass's text before it reads
`pass`.

### 3.3 Invariants

1. At most one pass is open, and passes are numbered 1, 2, 3 with no gap.
2. Every `return` answers one `ask` of the open pass, once, in the order the asks
   arrived.
3. A pass holds at most 64 asks, and no tag twice.
4. A pass ends at exactly one of `turn`, `refuse` and a break of section 4, which leaves
   the relay `Broken`. After `turn` or `refuse` the relay reads nothing more of that
   pass, and any frame of it that follows is found by the next pass's look or read.
5. The relay calls no `Ports` method outside section 2.2, and calls each listed method
   with the arguments the ask carried.
6. Every event in the record is the harness's. A relayed ask authors exactly what the
   same `Ports` call authors when the compiled loop makes it.
7. `Broken` is terminal for the load. Nothing respawns the loop, and the agent has no
   supervisor.
8. The entry's result for row 8 is `turn`'s result unchanged: `Refused`, `Faulted`,
   `Unlicensed` and `ChannelLost` each reach the harness as `turn` answered them. A
   `Refused` turn keeps service (`lifecycle.rs`:1162) where `ChannelLost` may end it
   (:1195).

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

The breaks the relay observes, each named in one line on the worker's standard error as
the relay meets it:

- A message that does not parse as JSON, or is not an object.
- A member named twice in one object.
- A frame with no `kind` that is not exactly the preamble, or a series that breaks 2.4.
- An unknown `kind`, or a `pass` or `return` from the loop.
- A member outside its kind's, a member missing, or a member of the wrong type, row 6's
  `last_turns` included.
- An integer outside 0 to 2^64 - 1, or written with a sign, a fraction or an exponent,
  or a lone surrogate in a string.
- A `hello` with another version or another digest, a frame before the `hello`, or a
  second `hello`.
- A frame standing on the socket when a pass opens.
- A frame naming a pass other than the open one.
- A tag used twice in one pass, or a 65th ask in one pass.
- An ask outside the closed list of 2.2, or a delta outside its form.
- The pass's bound expiring in a read or a send.
- End of file, a failed read, or a send that fails or does not finish.

The trace holds what it holds today for `ChannelLost`. The reason line goes to the
worker's standard error alone, since a trace kind for it would be a new event and
section 6's identity forbids one the compiled loop does not author.

## 5. The bounds

**A pass gives the loop 10 seconds of its own time.** The relay's waits on the socket in
one pass, reads and sends together, the slices of a series included, are summed, and the
pass breaks when the sum passes 10 seconds. The harness's time inside a `Ports` call is
not the loop's and is not counted. A loop that stops reading in the middle of a
segmented `return`, or a slice that never comes, meets the bound like silence does.

**The `hello` has its own 10 seconds**, from the moment the relay's making begins.

The entry is called inside the turn's handling (`lifecycle.rs`:1134-1137), so a stop or
an unload waits on the loop until the bound passes. 10 seconds is the Planner's ruling
of 2026-11-25, to be measured against the slowest node a January loop holds. The
constants are `LOOP_PASS_BOUND_MS`, 10,000, and `LOOP_PASS_ASKS`, 64, beside
`LOWER_BOUND_MS` in `weaver-types` `wire.rs`.

## 6. Identity

Identity is a program's check over two records, an arm of the compiled loop and an arm
of the wires, driven by one script.

**The arms' starting state.** Each arm starts from an agent made fresh by the same
steps, under the same name and the same declarations, with nothing in its state, and
runs the same script of loads, turns and unloads. Neither arm is a second load of the
other's agent, so the two shapes hold the same runs and each load's lineage names its
own arm's save points.

**What the script drives.** Each path of the compiled loop at least once: a first turn
with nothing seated, a later turn, pressure with its flush and recall, a first turn on a
later load with earlier runs (the continuity line), a first turn with a prompt seated,
and `independent`, both with nothing seated and with a prompt seated.

**The comparison** is over parsed values, each event's canonical line parsed as JSON,
event by event. Two values are the same where they are of one JSON type and equal: a
boolean never equals a number, an integer never equals a float, and `0.0` is not `-0.0`,
while an object's members in another order are the same object. The two records are
identical when they hold the same events, in the same order, with the same values, after
exactly these three steps and no others:

1. Every envelope's `wall_ms` and `monotonic_ns` are set aside, and so are the values
   of each `model.measurement`'s `payload.timings.prefill_ns` and
   `payload.timings.decode_ns`, whose presence is still compared.
   The clocks differ between any two runs, and the two timings are elapsed times as the
   clocks are, taken by the SPU (`weaver-spu` `decoder/session.rs`) and rendered into
   every measurement (`main.rs`). These four and no others: a timing added to the record
   later fails the comparison until this step names it.
2. Each arm's load events' `composer` is checked against what that arm must hold, then
   set aside: the compiled loop's `{binary: "worker"}` in the compiled arm, and the
   wires' form with the wire file's digest in the wires arm.
3. In each arm, every distinct value of these fields, and of no others, is replaced by a
   name in the order it first appears, `id-1` for the first value met, `id-2` for the
   next new one, the same value always by the same name: the envelope's `session` and
   `run`, the `save_point` event's `save_point`, `run` and `name` (`event.rs`:404-408),
   the `run` of each identity in a recall's `returned` (:447, :489), the load's
   `lineage` with its `save_point` and `run` (:771, :830-831) and its `built_from`
   branch's `parent` and `run` (:836, :843-844), and the load's `reset`'s `prior_run`
   (:778, :852). The renamed arms are then compared, so wherever they are equal the map
   between the two arms' values is one to one.

The declaration's digest (`event.rs`:749) is not set aside. The election of the loop
does not live in the declaration (the brief, point 6), so both arms load the same
declarations and each digest is equal.

**The record's identity does not cover the saved image.** A save point's name is the
digest of its file, and its stamp holds the clock (`weaver-state`
`save_point.rs`:22-33), so the renaming of step 3 is right and hides the image's bytes.
Comparing the images apart, less the stamp line, is costed in act 2's brief.

**A control arm comes first.** The record holds the model's output and `model.field`'s
probabilities as `f32` (`weaver-trace` `event.rs`:576-588), equal only where generation
repeats bit for bit. So the test runs the compiled loop twice under this comparison
before it runs the wires. Where the two compiled arms differ, the test reports that and
does not compare the wires.

**Two timing paths are outside the comparison, read by three marks.**

- The `RecorderCommitPressure` fault reads the recorder's queue depth at the turn's end
  (`engine.rs`:162-188), which depends on the writer's timing. The script keeps each
  session short of the high-water mark (`weaver-trace` `writer.rs`:50, 768 queued
  records). The worker's log reads the same mark a second way: its queue line names the
  peak the trace queue reached and the mark, and a peak at or past the mark is the
  third mark of an unclean arm.
- The state seam's answer bound of 2 seconds (`state.rs`:27). A late `recall` or `shape`
  answers `null` and changes the delta.

Every arm, the control arms included, must hold none of the three: no pressure fault, no
queue peak at its mark in the worker's log, and no answer the state seam timed out. A
timed-out answer retires the state seam, and the unload records the member as
`unconfirmed`. An arm that holds one is run again, at most three
times, and then the test fails.

A difference outside the three steps is a failure of the test, never a field to add
after the fact. Adding a step or a field is a change of this contract.

## 7. What each party supplies and guarantees

- **The harness** answers every listed ask by the listed `Ports` call, authors what that
  call authors, keeps the bounds, and refuses every break it observes. It holds no
  opinion of the loop's content.
- **The loop** holds nothing between passes, and asks for what it needs. A node is a
  pure function of what crossed the socket in its pass: no clock, no randomness, no
  file, no network, no state kept between passes (the rulings of 2026-11-11, section
  6.3, items 4 and 5). A set of strings has no fixed order between two loads, since each
  process seeds its string hashing, so a node that iterates over one is not the same
  function of its pass in the next load. The board client keeps at most four asks
  outstanding, and sends `turn` and `refuse` with no other ask outstanding. It checks
  each frame it would send by the rules of sections 2 and 3, and sends none that breaks
  them.
- **The runner** reads the wire file once, hashes those bytes, compiles from them and
  not from the path, checks the grammar below, sends the `hello`, and calls
  `on_turn(board, text)` once per pass. Where `on_turn` fails before its pass ends (it
  raises anything, it exits, it returns with its pass open, or the board refuses a frame
  of its), the runner reads the returns of any asks outstanding and refuses that pass.
  Where `on_turn` raises after its `turn` or `refuse` is sent, the pass stands as it
  ended, and nothing more is sent. The runner and the relay stand ready if the harness
  opens another pass. On an agent the harness opens none: a refused pass is
  `ChannelLost`, and service ends. The harness's breach is told by what the board met (a
  frame the harness sent that breaks the contract, its end closed inside a segment
  series, or the end gone inside a pass), and never by what `on_turn` raised. Then the
  runner exits 1, whatever the wires did after. At the harness's end of file between
  passes it exits 0.
- **A limit of the interpreter's** (`RecursionError`, `MemoryError`) met while the board
  parses a return loses no frame. The board keeps the frame's bytes before it parses
  them. A limit met in a wire file's wait is raised to the wires as it is, and the
  runner's refusal reads the kept bytes again at its own depth, where a limit is the
  frame's own and a breach.

**What the runner says.** Every line the runner writes on its standard error is one JSON
object in ASCII, `{"loop": <word>, "pass": <n>, "reason": <text>}`, `pass` present where
a pass is open. A reason past 1024 characters is cut there and ends
` ... (cut at 1024 of <n> characters)`: a refused or raised line's reason is the wire
file's own exception text, and the worker holds and logs every line. The words:

- `start`: the runner ends before its `hello`, exit 1 (exit 2 for a wrong count of
  arguments). The reasons:
  - an interpreter older than Python 3.12, named with its version
  - the file unreadable, or not UTF-8
  - the grammar below, naming the line
  - a file `compile` refuses after the parse took it (a `break` outside a loop), naming
    the line
  - descriptor 3 not the loop's end of a socket pair (closed, or not a socket)
  - the hello that cannot be sent, the harness's end gone
  - a file nested past the interpreter's limits: "the wire file nests past the
    interpreter's limits".
- `refused`: a pass the runner refused, with what `on_turn` raised.
- `raised`: `on_turn` raised after its pass ended.
- `unrefused`: a pass the runner could not refuse, exit 1.
- `broken`: the harness broke the contract, exit 1.
- `end`: the harness's end of file, exit 0, carrying `passes`, `turns` and `refused`:
  the passes served, those ended by `turn`, and those ended by `refuse`, the wires' own
  refusals among them.

A line that cannot be written is let go: the pass is refused whether or not its line
was written. The launcher's refusal takes the same form with the word `launch`
(`weaver-harness-Spec` section 2.5).

**The wire file's grammar, as what is allowed.** One Python 3 module, UTF-8 whatever a
coding line names, a leading byte-order mark dropped. At the top level it holds only:

- a docstring.
- the imports `import weaver_board`, `import json` and `import re`, each whole, with no
  alias and no other module.
- constants, a name assigned once to a literal: a string, an integer, a float, `True`,
  `False`, `None`, or a tuple of these. An integer or a float may carry one leading
  minus. Nothing else carries a sign: no plus, no second minus, no minus over `True` or
  `False`.
- function definitions, with no decorator.

It defines `on_turn(board, text)`. No name it assigns, defines or takes as an argument
begins with two underscores. Inside a function there is no `global`, no `nonlocal`, no
import, no class, no name among `open`, `exec`, `eval`, `compile`, `__import__`,
`input`, `globals`, `locals`, `vars`, `getattr`, `setattr`, `delattr`, `hash` and `id`,
no type parameter, no name beginning with two underscores, no attribute name beginning with an underscore,
no assignment to an attribute or deletion of one, no assignment or deletion by subscript
into an attribute, and no default argument that is not a literal of the kinds a
constant may be. Anything else is refused before the file runs, naming the
line.

**The check helps the author. It is not the boundary.** A syntax check cannot hold a
determined program. The three modules a wire file may import reach the rest of the
standard library by attribute (`json.codecs.builtins.open`, for one), so the grammar keeps
no wire file from a file, from a socket, or from state kept between passes. Keeping the
rulings of 2026-11-11 is the author's work. The walls of act 2 (the account, no-new-privs, the limits, the
filter with no new socket, Landlock read-only) are what hold the loop.

## 8. Conformance: today's loop as wires

The fourth piece of the plan of 2026-11-25 is the compiled loop written as a wire file,
with the record shown identical by section 6, at the end of act 2. Act 1 holds a test
that runs the compiled `compose` and the wire file against one scripted seat and
compares the asks in order, their arguments, and the delta's bytes. Its inputs reach
every point below, and every fixed sentence. Four of the seven are inline literals in
the Rust, so this comparison is what holds them. Points marked unmeasured are measured
there, against the Rust function itself, never from this text.

**The fixed text, byte for byte.** Each block is one string, with no newline at its end
unless one is written as `\n`.

```text
SYSTEM_PROMPT
You are a careful assistant. Answer from what you know, say plainly when you do not know, and keep answers as short as the question allows.

the re-entry's opening sentence
The working context was reset to stay within its limit. Recent conversation, restored from the session's record:

the sentence for an unreachable record, its first character a newline
\n(The record could not be reached: continue from the request alone.)

the continuity sentence, {prior} and {turns} as decimal integers
Continuity: this session held {prior} earlier {runs_word} with {turns} completed {turns_word} before this one. The conversation content of those runs is not in your context.

the words and joins
run | runs | turn | turns | user | assistant | ": " | " ..." | "\n" | "\n\n"
```

**The asks:**

- `seated` is asked first, then `turn_context`.
- `keep` is 0 in both flushes (`dev_loop/mod.rs`:146, :176), and the recall asks
  `last_turns` 4, `RECALL_TURNS`.
- Under `independent`, `first_turn` is not asked. A flush answered `null` is a `refuse`.
  Otherwise the turn is asked at once, composed as on a first turn with no continuity
  line and no re-entry.
- Under `continuous`, `first_turn` is asked. `fullness` is asked only on a later turn. A
  `null` fullness is no pressure. Under pressure the flush is asked and its answer
  ignored, then the recall. `shape` is asked only on a first turn with nothing seated.

**Pressure** is `capacity > 0` and `resident * 5 >= capacity * 4`, each product
saturating at 2^64 - 1 as Rust's `saturating_mul` does. Python's integers do not
saturate, so the wires clamp both products (resident 2^62 against capacity 2^63 is
pressure in Rust and not in plain Python).

**The delta** (`dev_loop/mod.rs`:336-376), in this order, each message one text block:

1. The opening, role `system`, only where the turn is first and nothing is seated
   (:344). Under `independent` that is every turn where nothing is seated, and no turn
   where a prompt is. Its text is `SYSTEM_PROMPT`, or `SYSTEM_PROMPT`, `"\n\n"` and the
   continuity line where there is one (:346).
2. The re-entry, role `system`, only under pressure.
3. The request, role `user`, its text the turn's text unchanged.

**The re-entry** (:222-255): `SYSTEM_PROMPT` and `"\n\n"` only where nothing is seated,
then the opening sentence. A `null` recall appends the unreachable sentence and nothing
after it. Otherwise each quoted line follows as `"\n"`, the speaker, `": "` and the kept
text.

**The quoting** (:262-292):

- Only `message.user` and `message.assistant` are quoted, as `user` and `assistant`.
  Every other kind is skipped.
- A recalled event with no pair keyed `content` is skipped. Where several are, the first
  is read.
- One budget of 600 bytes, `QUOTE_BOUND`, is shared across the whole recall.
- `content` is parsed as JSON as `serde_json::from_str::<Value>` parses it at the locked
  version, 1.0.151, with the feature `float_roundtrip`, which the harness's own manifest
  turns on so that a float is read correctly rounded however the worker is built. A text
  it refuses yields no line. Python's `json` module takes more,
  and each of these is an input of the test: `NaN`, `Infinity` and `-Infinity`, a lone
  surrogate escape, a number past the range of a double (serde refuses it,
  `de.rs`:378-408, :620-634), and nesting 128 levels deep (serde counts down from 128
  and refuses at 0, `de.rs`:63, :1372-1385, so 127 levels parse and 128 do not,
  measured against `compose`).
- **The one known difference.** serde_json's `raw_value` is on in every build of the
  harness, and with it `Value` reads an object whose first member is named
  `$serde_json::private::RawValue` as that member's string parsed as a fresh document,
  its nesting counted afresh (`value/de.rs`:131-135). The wires stay plain and read such
  an object as an object. No text a user types makes a member's name, so no pass of
  today reaches it, and a test pins each side.
- A value that is not a list yields no line. An element that is not an object, or has no
  string `text`, is skipped.
- The budget is checked before each block, and a block met at a budget of 0 is skipped.
- Words split on the Unicode `White_Space` property, Rust's `split_whitespace`. Python's
  `str.split()` also splits on U+001C to U+001F, which Rust keeps inside a word.
- A word's cost is its length in UTF-8 bytes, not in characters.
- The budget counts the words and not the single spaces that join them.
- A word longer than what is left sets the budget to 0, appends `" ..."` with its own
  leading space, and ends the block. A block whose first word is too long is the line `"
  ..."`.
- A word exactly as long as what is left is kept, and the next word, if any, then
  appends `" ..."`.
- A block of whitespace alone, or an empty `text`, yields an empty line, so the
  speaker's label stands alone.

**The continuity line** (:306-327): a `null` shape is no line. The earlier runs are all
but the last, and there is no line where the shape holds fewer than two runs. Each
earlier run contributes its first `turn.closed` count, or 0. The two counts take `run`
or `runs` and `turn` or `turns` by whether each is exactly 1. The sum is a `u64` sum,
which panics in a test build and wraps in the release build where it overflows
(:311-319). The wires take it modulo 2^64, as the deployed worker does, and the test's
inputs hold no sum past 2^64 - 1.

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

Section 6 changed on 2026-11-26, before any run: step 1 names the measurement's two
timings, which every real arm differs in, the unclean arm's marks name the worker's
queue line, and two values are the same only where their JSON types are (the Planner's
ruling of 2026-11-26).
