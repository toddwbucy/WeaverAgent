# Brief: the loop seam, act 1 of three

Written by the WeaverAgent Executor on 2026-11-25, from the loop plan the Planner
accepted in outline that day, with the Planner's five answers and eleven points. One
branch from `b74062bf`. This brief and the contract it names are the act's first commit,
and no code comes before the Planner has read them.

## Why

The operator's direction is that the agent's loop is Python wires that ask the harness
through one socket (the rulings of 2026-11-11, and "All of our loop code is going to be
Python", 2026-11-24). Today the loop is `dev_loop`, compiled into the worker, and
nothing outside the worker can ask the harness anything: no socket, no contract, no
install. The operator's order is to create the loop, test it, then evaluate it. These
three acts create it and leave it testable.

The seam already stands inside the worker. The worker hands the harness one function,
called once per turn with the granted `Ports` and the turn's text
(`lifecycle.rs`:663-673). The relay is a second function of that type, which passes each
turn to a Python process and answers each of its asks by the same `Ports` call
`dev_loop` makes. The harness authors every event either way.

## Rulings this act stands on

1. **Relay through the harness** (2026-11-11, ruling 1). The loop holds one socket, to
   the harness, and nothing dials a component directly.
2. **A wire is one ask and one return** (2026-11-11, ruling 2), tagged, with no ordering
   promise across wires and one to four asks outstanding.
3. **The loop's account is the agent's own** (73 d). Root lays the folder, its owner and
   its modes (0750 on the folder, 0640 on the file), and the worker lays the walls.
4. **A broken loop is `ChannelLost`, and service ends as today** (the Planner,
   2026-11-25, point 1). No new state and no new row in the lifecycle table.
5. **A pass is bounded at 10 seconds** (point 2), a ruling to be measured.
6. **The closed list is the contract's own** (point 8). `elide` and `replay` are among
   what is not carried.
7. **The relay names only what it can see** (the Planner, 2026-11-25 20:42 UTC, ruling
   1). It answers in arrival order and counts nothing outstanding. The limit of four and
   the ending ask sent alone are the loop's rules, kept by the board client. This brief
   takes that lean.
8. **The bound is on the pass and covers sends** (ruling 2): the relay's waits in a
   pass, reads and sends, are summed and expire at 10 seconds, with at most 64 asks in a
   pass.
9. **The `hello` is read when the relay is made** (ruling 3). A failed `hello` refusing
   the load is act 2's, and the lean.
10. **A frame standing when a pass opens is a break** (ruling 4).
11. **The documents that say the loop is compiled alone change in the act that makes
    each new sentence true** (point 11). `CLAUDE.md` changes on the operator's word
    alone.

## The three acts

| Act | What it leaves runnable | What it holds |
| --- | --- | --- |
| **1. The seam** | The relay and the Python board client and runner, driven over a socket pair in tests. Deployed agents still run the compiled loop | The contract, the relay, the runner, their tests, and today's loop as a wire file compared with `dev_loop` at the seat |
| **2. The loop as a process** | An agent with a loop folder laid by hand runs it as wires | The election by the standing folder, the parent folder `/etc/weaver/<agent>/` laid and removed with the agent, the names `admin` and `stack` refused, admin's folder checks, the worker's fork and walls, the load event's `composer`, the trace Spec, the identity test on the record (the fourth piece), the confinement numbers for the Planner's ruling |
| **3. Loop install** | Weaver Web sends a wire file over admin-con, admin checks and lays it, and a reload runs it | `install-loop`, `remove-loop` and `check-loop`, the sudo rule's verb list, admin's Spec, the runbook |

## Act 1, file by file

1. **The contract,** `docs/crates/contracts/weaver-harness-loop-contract.md`, in this
   commit.
2. **This brief,** in this commit.
3. **`weaver-types` `wire.rs`:** `LOOP_PASS_BOUND_MS`, 10,000, and `LOOP_PASS_ASKS`, 64,
   beside `LOWER_BOUND_MS`.
4. **The `Seat` trait moves** from the worker's `dev_loop/mod.rs`:57-66 into the harness
   library, unchanged, with its `Ports` implementation. `dev_loop` composes against it
   as before, and the relay is written against the same trait, so a stand-in seat tests
   both.
5. **`channel.rs`:** the segmenting half of `send_decode` (:789-812) moves into a
   function over bytes that takes the send as a closure, and `send_decode` calls it.
   That function and `reassemble_if_series` (:1025) become visible in the crate. Nothing
   else in the file changes. The relay supplies its own send and receive under the
   pass's deadline, since `send_octets` (:1013) blocks and `recv_octets` (:1063) has no
   deadline.
6. **The relay,** `crates/weaver-harness/src/loop_relay.rs`, its tests in
   `loop_relay/tests.rs` (#118): the making that reads the `hello`, the states of the
   contract's section 3, the look before each pass, the series, the bounds, and each
   break of section 4. The relay writes each reason line to a writer it is given, the
   worker's standard error when deployed, so the tests read every reason.
   - **The tests compare the entry's own result.** A `turn` answered `Refused`,
     `Faulted`, `Unlicensed` or `ChannelLost` reaches the harness as that same error, so
     a relay that turns every error of `turn` into `ChannelLost` goes red.
   - **Each break has its case and its reason,** so two swapped reasons go red.
   - **Perturbations, beside each break taken out:** the seat's time counted against the
     bound, the bound not summed across a pass, sends not timed, the look before a pass
     removed, the 65th ask taken, `last_turns` taken when absent, `-0` taken as an
     integer, a repeated member taken, and a refusal leaving the relay `Broken`.
7. **The Python, `loop-runner/`, standard library alone:** `weaver_board.py` (the
   frames, the series, the tags, the limit of four outstanding asks, `turn` and `refuse`
   held until no ask is outstanding, a repeated member refused by its own code, integers
   read from their literal text) and `runner.py` (reads the wire file once, hashes and
   compiles those bytes, checks the grammar of the contract's section 7, sends the
   `hello`, calls `on_turn` per pass), and their tests. **Perturbations:** the grammar
   check removed, the wires compiled from a second read of the path, the series' count
   as M // 65536 + 1, a fifth outstanding ask let through, `turn` sent with an ask
   outstanding, a repeated member taken, and `-0` taken. Each goes red under its own
   case.
8. **One file of frames, valid and broken,** read by the Rust and the Python tests
   alike, so the two sides refuse one set. Beside each break of section 4 it holds a
   member named twice, `true` where an integer stands, a fraction, an exponent, `-0`, an
   absent `last_turns`, a `pass` and a `return` from the loop, and each refusal of a
   delta: an empty delta, an extra member, the roles `assistant` and `tool_result`, an
   empty `content`, a tool block. An empty `text` is among the valid frames.
9. **Today's loop as a wire file,** a test fixture in act 1. A test of the worker's
   `dev_loop` runs `compose` and the wires against one scripted seat over the inputs of
   the contract's section 8, and compares the asks in order, their arguments, and the
   delta's bytes. The seat records every argument, so a flush with `keep` 1 goes red.
   The inputs reach each fixed sentence and each branch, a prompt seated under
   `independent` among them, and measure every point section 8 marks unmeasured against
   the Rust function itself. The comparison of whole records is act 2's.
10. **One test across the two languages:** the relay and `/usr/bin/python3 -I -B -S`
    running the runner over a socket pair, relaying to a stand-in seat. It asserts at
    least two passes, one refusal, a `recall` return over 64 KiB crossing as a series to
    the loop and a `turn` ask over 64 KiB crossing as a series to the harness. It fails
    where the interpreter is absent, and never skips.

## Not in act 1

- The loop process, its account, its walls, its start and its stop (act 2).
- The election, admin's checks and the declaration (act 2).
- Install, removal and the check verb (act 3).
- The record rule of 2026-11-11 (every read an event). It lands on the compiled loop
  first, as its own act.
- B3, B4 and B10. Today's loop asks none of them.
- `dev_loop`'s `foundation` module, which nothing selects.
- Every document sentence of the plan's section 8. None is true after act 1.

## Gates

Each with a private `CARGO_TARGET_DIR` outside the home directory, behind the PATH
tripwire, each run alone:

- `process/gates/lock.sh`, then `cargo fmt --all -- --check`.
- `cargo clippy -p weaver-types --all-targets --locked -- -D warnings`, and the same for
  `weaver-harness`.
- `cargo test -p weaver-types --locked` and `cargo test -p weaver-harness --locked`.
- `bash -c 'for t in loop-runner/test_*.py; do python3 -I -B "$t" || exit 1; done'`. The
  form with `|| break` exits 0 after a failing test, since `break` returns 0. Measured
  on 2026-11-25: a failing first test gives 0 under `break` and 1 under `exit 1`.
- The full suites at the end of every self-check round: `cargo test --workspace
  --locked` and `python3 deploy/test_plans.py`.

**Perturbations** run in a detached scratch worktree at the head, as items 6 and 7 name
them. Each must fail the suite under its own case.

## Point 3: how a frame over 64 KiB crosses

**By the segment series of `weaver-types-Spec` section 4.4, in both directions, written
in Python too.** Every loop frame carries `kind`, so the preamble is known by its
absence, as on the decode seam. The total bound is `DECODE_MESSAGE_BOUND`, 8 MiB.

- **Which frames pass 64 KiB.** A `return` of `recall` or `shape`, and a `turn` ask,
  which carries a re-entry and the turn's text. **A `pass` frame never does.** Its text
  is the decoded member of a gate line of at most 49,041 bytes (`weaver-gate`
  `relay.rs`:51), and `serde_json` writes the shortest escape for every character, so no
  text re-encodes longer than it arrived. The worst lines encoded (text of `U+0001`, of
  `"`, of a four-byte character, of ASCII) give `pass` frames of 49,079 to 49,083 bytes.
- **The 1 MiB of `state.rs`:64 is every serving answer of the state member,** recall and
  shape included. It bounds the line the state seam reads (`state.rs`:657), so a
  `recall` or `shape` return can carry up to that much, re-encoded. 8 MiB covers it with
  room.
- **Why not another way.** A larger message bound for this pair alone would be a second
  mechanism to test. The series is tested on the decode seam already, and the harness's
  side is two functions it holds, moved as item 5 says. The Python side is about forty
  lines.

## Point 6: the election, with nobody at the appliance

**The standing loop folder is the election, and the declaration has no `loop` key.**
Admin runs the wires where a checked loop stands in `/etc/weaver/<agent>/loop/`, and the
compiled loop where none stands. A folder that stands and fails its owner, mode or check
refuses the load, naming it, and never falls back by itself. `install-loop` lays a loop,
and `remove-loop`, one fixed line, falls back. A reload makes either take effect.

Why, against the design of 2026-11-11:

- **Declaring comes before wiring, and they are two things.** The declaration screen
  declares the board, the components the harness is wired to. The wires are made on the
  loop page and are not a component. A `loop` key would put the wiring into the
  declaration.
- **The record names the loop by its digest already.** By section 6.3 of 2026-11-11 the
  load event's `composer` names the wires and the interpreter. The folder election keeps
  one fact in one place: what ran is in `composer`, and the declaration's digest is the
  same in both arms of the identity test.
- **Admin never writes the declaration.** It is the operator's, edited with `sudoedit`.
  A key that Weaver Web sets would need a new path by which a remote caller changes
  root's declaration. The folder is laid by a verb that already has to exist to carry
  the code.
- **Fewer fixed lines.** Install, remove and check are three lines, where the key would
  need install, check and two lines to set the key.

What it costs: an operator who forgets a loop is laid runs it. `check-loop` answers
whether one stands, and the load event's `composer` names what ran on every load.

**The parent folder,** `/etc/weaver/<agent>/`, `root:weaver-<agent>` 0750, is laid by
`create-agent.sh` and removed by `decommission.sh` in act 2, the act in which admin
first reads it. The refusal of the names `admin` and `stack`, beside the refusal of the
connector accounts' names, comes in the same act with its test, since the collision
begins when the folder is laid.

**The plan's section 8 candidates change with point 6.** Candidate 1 reads "a loop laid
in the agent's loop folder runs, and the compiled loop runs where none is laid" in place
of the declaration's key. Candidate 12 loses "Each gains the `loop` key": admin's Spec
gains the folder check in act 2, and the types Spec does not change.

## Point 10: what a reload adds to the record

Checked at `lifecycle.rs`:1443-1500 and `engine.rs`:1105. A graceful unload of a run
that holds an SPU, outside a diagnostic binding, runs one wind-down turn before its save
point. The turn is the harness's own, asked in its own words (`engine.rs`:156), and
never reaches the loop, under either loop. So each reload adds one turn and one save
point to the record, then opens a new run. That line goes to the Weaver Web seat with
the reload.

## Unmeasured, and what measures it

| Unknown | What measures it |
| --- | --- |
| Where `serde_json` and Python's `json` part on `content` (`NaN`, lone surrogates, depth, a number past a double) | Item 8's test against the Rust function, in act 1 |
| That 10 seconds holds for a January loop | The slowest node a January loop holds, timed in act 2 |
| Python's start under the walls | Act 2, on the build machine's kernel, then on a made appliance |
| How WeaverAnalysis reads a `composer` with new fields | A note to the Planner in act 2. No checkout here |

## What act 2's brief owes

- What the two confinements would share by function and line count, what the loop's
  Landlock grant needs beyond the shell's, and what a floor crate would newly depend on.
- What comparing the saved images apart, less the stamp line, would cost.
- `LoadFacts.composer`, the observation answer admin reads (`lifecycle.rs`:2377-2387,
  `wire.rs`:507), and the wires' form of `composer`.
- The identity session's script, path by path, as the contract's section 6 names them.
