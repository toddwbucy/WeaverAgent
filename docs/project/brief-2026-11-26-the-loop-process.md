# Brief: the loop as a process, act 2 of the loop

Written 2026-11-26. Act 2 of three, on the operator's rulings of 2026-11-25 (73 a to e).
Act 1, the seam, merged into `act/the-loop-seam` as #638 to #643.

**What act 2 leaves.** The worker starts the runner at each load, under walls. A loop
laid by hand in the agent's loop folder runs today's loop written as wires on an agent,
and its trace is shown identical to the compiled loop's by the contract's section 6.
Laying a loop from Weaver Web is act 3.

**How each line here is held.** Each line is one of three things:

- read in the source at `act/the-loop-seam` `e33ff15b` and cited
- measured, with the instrument named
- marked as not known, with what would measure it.

**The measurements were made on the build machine:** Fedora 45, kernel 7.3.4, Landlock
ABI 11, Python 3.15.2.

- A made harness held the pair at descriptor 3 and drove today's wire file
  (`tests/fixtures/loop-wires-today.py`) through three passes. Together they hit every
  ask the compiled loop makes under `continuous`: a first turn, then two later turns
  under pressure, each with its flush and recall.
- A launcher laid each wall on itself, then executed the runner exactly as section 1
  below starts it.
- **A wall is called sufficient where the runner, under it, sent its hello, served the
  three passes whole and exited 0.**

## Rulings this act stands on

1. **The loop runs as the agent's own account** (73 d, the Planner on 2026-11-25 20:11
   UTC, ask 1). Root lays the folder, its owner and its modes. The worker lays the
   walls.
2. **The standing loop folder is the election, and the declaration has no key** (act 1's
   brief, point 6).
3. **A broken loop is `ChannelLost`, and service ends as today** (the Planner, point 1).
   There is no new state and no new row in the lifecycle table.
4. **A pass is bounded at 10 seconds** (point 2, `LOOP_PASS_BOUND_MS`, `wire.rs`:58).
5. **A failed `hello` refuses the load** (act 1's ruling 3, its lean taken here).
6. **Test is functional: "is the loop functional"** (73 e). Act 3 calls the check, and
   act 2 keeps its door open (section 6).

## 1. The start

**Who starts the runner, and when.** The worker, at the enter, once per load:

1. The worker forks the SPU, waits for residency, then forks the gate
   (`lifecycle.rs`:2530-2560, :2785-2810).
2. The loop comes last. The worker makes the pair, forks the runner, drops its copy of
   the child's end at once, and makes the relay. Making the relay reads the `hello`
   under its own 10 s (contract section 3.1).
3. A runner that does not start, sends no `hello`, or sends a wrong version or digest
   refuses the load, closed.
4. Only then is the run loaded and idle.

The compiled loop's load is unchanged where no loop stands.

**Its exact command:**

```text
<interpreter> -I -B -S -X pycache_prefix=<an empty folder root owns> \
    <prefix>/lib/weaver-loop/runner.py /etc/weaver/<agent>/loop/wires.py
```

- **The environment** is cleared to one pair, `LANG=C.UTF-8`, as the gate's shell is
  cleared (`confine.rs`, `command`, `env_clear`).
  - Measured: the runner served the three passes under `env -i` with only `PATH`,
    `LANG=C.UTF-8` and an `LD_LIBRARY_PATH` naming a folder the walls do not grant.
  - The worker's own environment is `PATH`, `HOME`, `LANG` and, where set,
    `LD_LIBRARY_PATH` (`weaver-admin` `start.rs`:980-992). Without the clear, the runner
    would inherit it through the organ fork's rule (`organ_environment`,
    `spawn.rs`:171-193).
- **The interpreter is named by path, resolved by the stack.** The stack's install
  resolves `/usr/bin/python3` once and records the resolved path and its sha256. Admin
  hands both to the worker in the enter.

**How descriptor 3 reaches it.**

- The pair is made with `SOCK_CLOEXEC` in the `socketpair` call, as every organ pair is
  (`channel.rs`:472-482).
- The child places its end at 3 by `dup2`, which is born without close-on-exec, as
  `place_child_ends` does for the organs (`spawn.rs`:28-31, `channel.rs`:1102-1160).
- **Measured:** at the first pass the runner held exactly descriptors 0, 1, 2 and 3.

**Where `runner.py` and the board stand.**

- **Where:** `<prefix>/lib/weaver-loop/`, with `<prefix>` `/opt/weaver` by default
  (`bootstrap-stack.sh`:71). The folder is `root:root 0755`, and `runner.py` and
  `weaver_board.py` are each `root:root 0644`.
- **Who lays them:**
  - `bootstrap-stack.sh` and `update-stack.sh`, beside the binaries they install
    (`bootstrap-stack.sh`:406-416).
  - The carry set as well, once `install-carry.sh` lands. It is draft #632, not merged,
    so this act names it and does not depend on it.
- **No `__pycache__` folder ships.** The stack refuses one standing beside them.
- **The retired-Python check does not reach them.** `retired-python.sh` refuses only the
  names `python-spu` and `pyworker` (:8-14, :16-33).

**The loop folder, as 73 (d) rules it:**

- `/etc/weaver/<agent>/` is `root:weaver-<agent> 0750`.
- `loop/` in it is `root:weaver-<agent> 0750`.
- `wires.py` in that is `root:weaver-<agent> 0640`. The group enters the folder and
  reads the file, and nothing executes the file (the Planner's reading of 73 d).
- **Who lays and removes the parent:** `create-agent.sh` lays it, and `decommission.sh`
  removes it, in this act, where admin first reads it (act 1's brief, point 6).
- **In act 2, root lays `loop/` by hand** for the identity run. Act 3's `install-loop`
  lays it from Weaver Web.

**The bytecode cache, off.**

- `-B` writes no bytecode. The folders are root's, and the walls grant no write
  anywhere.
- `-X pycache_prefix` points the interpreter's bytecode reads at an empty folder the
  walls do not grant. So the interpreter reads none of the standard library's shipped
  bytecode either.
- **Measured, five starts each, every wall laid:**

  | Bytecode | `hello` arrives after | | --- | --- | | the shipped bytecode read | 35 ms
  | | no bytecode read | 73 to 77 ms |

  Nothing was written in the prefix folder. The cost is about 40 ms, once per load.

## 2. Which loop runs

**As decided on 2026-11-25:** a loop standing in the agent's loop folder runs. Where
none stands, the compiled loop runs. The declaration has no key.

**What "standing" means, exactly.** Admin's load looks, without following a link, at
`/etc/weaver/<agent>/loop/`.

- **Absent:** the compiled loop runs.
- **Present:** each check below must hold, or the load is refused, naming the check, and
  never falls back by itself.
  1. `loop/` is a directory, not a link, `root:weaver-<agent> 0750`.
  2. `wires.py` in it is a regular file, not a link, `root:weaver-<agent> 0640`.
  3. `wires.py` is at most 256 KiB.
  4. Admin reads it once and records its sha256.
  5. At the start, the runner reads the same file, checks its grammar, and sends that
     digest in its `hello`.
  6. The relay compares the `hello`'s digest with admin's.

  A grammar the runner refuses ends the runner before its `hello`, with its line on
  standard error, so the load is refused and the line says why (section 6).

**The check is in two places.** Admin checks owner, mode, size and digest. The runner
checks the grammar. Admin runs no Python.

**A file changed between the two reads** (an install during a load) gives two digests,
and the load is refused.

**The load event's `composer` names what ran,** and that moves the composer from the
binary to the enter.

- **Today:** `serve` takes the composer once for the binary's life, "before any enter
  can author a load" (`lifecycle.rs`:663-677), and `LoadFacts.composer` copies it
  (`lifecycle.rs`:2215-2218, `wire.rs`:521, :538-544).
- **In act 2:** the enter carries the election, and the worker sets each run's composer
  from it.
  - The compiled loop's composer stays `{binary: "worker"}`, unchanged by a byte.
  - The wires' composer is `binary`, plus `file` and `sha256` (the two fields kept in
    the vocabulary, `event.rs`:687-693, trace Spec :889-893).
  - It also gains three optional fields: the interpreter's resolved path, its sha256,
    and the runner's sha256.
- **Not known:** how WeaverAnalysis reads a `composer` with new fields. No checkout of
  it is here. Its trace reader, run against a fixture holding them, would measure it.
  That goes to the Planner as a note, not as an edit there.

## 3. The walls, each with its measurement

**The order is the shell's,** applied by the child to itself before its exec
(`confine.rs` `apply`, :447-492): no-new-privs, the limits, the filter, then Landlock.

**Every wall at once, with the minimal grant below:** sufficient.

**The account, and no-new-privs.**

- The runner runs under the worker's uid and groups, the agent's own (ruling 1).
- The worker already holds no-new-privs, set by admin's start step before its exec
  (`start.rs`:964-971, :1016). It is inherited across a fork and an exec.
- The child sets it again, as the gate's shell does, because the filter needs it.

**The limits** (the shell's are 512 MiB of address space and 64 descriptors,
`confine.rs`:43-46). All were measured with every wall laid:

| Limit | Lowest that serves | Peak measured | The shell's |
| --- | --- | --- | --- |
| Address space, small frames | between 18 and 20 MiB | `VmPeak` 19.6 MB | 512 MiB |
| Address space, an 8,000,000-byte recall | between 32 and 48 MiB | `VmPeak` 52.4 MB | 512 MiB |
| Descriptors | 5 (4 fails loading `libpython`) | 4 open at the first pass | 64 |

- **The lean is to take the shell's values unchanged:** 512 MiB, 64 descriptors, 64 MiB
  of file size, and no core.
  - A frame is at most 8 MiB, the decode seam's total (`wire.rs`:36-42).
  - 512 MiB is about ten times the peak measured with the largest return.
- **At 32 MiB the large recall raised `MemoryError` inside a board call.** The runner
  refused the pass, then met "a frame that does not parse as JSON", and the seam broke.
  That is known item 1 of act 1 (section 9) meeting a real limit. Under 512 MiB no frame
  reaches it. A wire file that allocates on purpose can.

**The filter: no `socket` and no `socketpair`, measured, not read.**

- **The instrument:** a filter that kills the process on `socket`, `socketpair` or
  `io_uring_setup`, laid before the exec.
- **The result:** the runner served the three passes and exited 0. So from its exec to
  its exit it made none of the three calls.
- **The filter's own check, measured:** under it, `socket.socket(AF_UNIX)` and
  `socket.socketpair()` each die by `SIGSYS`, and a plain `print` lives.
- **Why the runner needs neither call.** It takes its end by `socket.socket(fileno=3)`
  (`runner.py`:256), which makes no `socket` call.
- The shell's own filter, refusing those calls with `EACCES` (`confine.rs`:161-181), was
  also measured sufficient.
- **The lean:** the loop's filter is the shell's first nine instructions, with an allow
  and a refusal after them. Those nine refuse another architecture word, x32,
  `io_uring_setup`, `socket` and `socketpair`, all with `EACCES`. The file-attribute
  rows (#587) are the shell's, for a shell that writes its home. The loop writes
  nothing.

**Landlock: what the interpreter must execute and read, measured.**

- **The method:** the runner was started under a grant naming each file it maps, then
  run again with each grant dropped in turn.
- **The minimal grant that serves:**

  | Path | Right | Dropped alone | | --- | --- | --- | | `/usr/bin/python3.15`, the
  resolved interpreter | execute, read | the exec fails, `EACCES` | |
  `/usr/lib64/ld-linux-x86-64.so.2`, the loader | execute, read | the exec fails,
  `EACCES` | | `/usr/lib64/libc.so.6` | read | the loader refuses it | |
  `/usr/lib64/libm.so.6` | read | the loader refuses it | |
  `/usr/lib64/libpython3.15.so.1.0` | read | the loader refuses it | |
  `/usr/lib64/libgcc_s-16-20261029.so.1` | read | the loader refuses it | |
  `/usr/lib64/python3.15/`, the standard library | read, list beneath | the interpreter
  cannot find its encodings | | `<prefix>/lib/weaver-loop/` | read, list beneath | the
  runner does not open | | `/etc/weaver/<agent>/loop/` | read, list beneath | the wire
  file does not open |

- **Not needed, each measured by dropping it:** `/etc/ld.so.cache`, the locale files,
  the `gconv` cache, `libcrypto` and `libz`.
  - Without `libcrypto`, `_hashlib` does not load, and `hashlib` falls back to the
    interpreter's own sha256. The `hello`'s digest still equalled the file's sha256,
    measured.
  - `/dev/urandom` is not needed either, since the interpreter takes its seed by
    `getrandom`.
- **What the loop may read beyond the interpreter: its own folder alone,**
  `/etc/weaver/<agent>/loop/`. It writes nowhere, executes nothing but the interpreter
  and the loader, and holds no TCP right.
- **Scoping as the shell's:** signals and abstract sockets.
- **These paths are the build machine's, and the appliance's are not known.** The sealed
  image's program list names `python3.14` (`deploy/image-programs.txt`:779-782, seal
  `68c570cc`), and its library paths may differ.
  - **So the grant is not written into the code.** The stack's install computes it on
    the machine that runs it, from the resolved interpreter, its loader (`PT_INTERP`)
    and the libraries the loader resolves for it. It writes the grant into a root-owned
    file beside the runner, which admin reads and hands to the worker. The worker judges
    each path as the shell's grant is judged (`open_judged`, :584-607).
  - Running the same drop-each measurement on a made appliance would measure its list.

**The dumpable flag, for a child that is not the gate.**

- **Measured: the exec sets it back.** With the flag cleared before the exec, the
  runner's `/proc/<pid>/fd` was still owned by its own uid, not root. The flag is 1
  after every exec here.
- **Two routes were measured not to work:**
  - clearing it in the child before the exec
  - a grant of execute without read on the interpreter, under which the exec itself
    fails with `EACCES`.
- **One route works.** The runner clears its own flag through `ctypes` before it reads
  the wire file. Measured: the flag read 1 before and 0 after.
  - **Its cost:** one more file in the grant, `libffi.so.8`.
  - **It leaves `ctypes` reachable from the wire file by attribute** (section 7 of the
    contract). The runner drops `ctypes` and `_ctypes` from `sys.modules` after the
    call, so no attribute path reaches them through `sys`.
  - That is a question for the operator (section 11, question 3).
- **Without the flag:** the kernel's ptrace scope reads 1 here, so only an ancestor may
  attach, and the worker is the runner's only ancestor under the agent's uid. The
  appliance's scope is not known. Reading `/proc/sys/kernel/yama/ptrace_scope` there
  measures it.

## 4. The confinement code

**What the loop's walls and the shell's share,** by function and line count in
`weaver-gate` `src/tools/confine.rs` (657 lines at `e33ff15b`):

| Share | Functions and items | Lines |
| --- | --- | --- |
| **Shared as written** | Landlock's kernel structures and rights (:56-89), seccomp's structures, opcodes and `op` (:90-139), `rlimits_within_reach` (:241-265), `landlock_abi` and `abi_at_floor` (:266-295), `apply` (:447-492), `allow` and `open_judged` (:571-607), `add_rule` (:625-655) | 255 |
| **Shared once a value is a parameter** | the rlimit table and the ABI floor (:29-55, the values become the caller's), `ConfineFault` (:196-240, five variants are the shell's), the ruleset's creation inside `build_ruleset` (about 30 of :493-570) | about 100 |
| **The shell's alone** | `SHELL_FILTER`'s file-attribute rows and its view (:140-195, 9 of its 17 instructions are the loop's too), `Shell` and `Layers` (:296-312), `confined` and `confined_in` with home's `.tmp` (:313-375), `bare` (:376-395), `command` (:396-446), `judge_home` (:608-624), the shell's rules in `build_ruleset` (about 48) | about 275 |
| Header | the module's comment (:1-27) | 27 |

The tests are `src/tools/confine/tests.rs` (892 lines) and `tests/landlock.rs` (1,911
lines).

**What the loop's grant needs that the shell's does not:**

- **Execute on two files where the shell grants one.** The busybox is one file. The
  interpreter needs its loader.
- **Read on named library files.**
- **Read and list beneath three folders:** the standard library, the runner's, and the
  loop's.
- **Read on a file the stack computed.** The shell's paths come from its declared grant.
- **Nothing the shell has that the loop lacks needs adding.** The loop takes no home, no
  write, no `/dev` file and no TCP right.

**What a floor crate would newly depend on.**

- `weaver-types` and `weaver-traits` hold no `unsafe` (0 mentions in their sources) and
  depend on `serde`, `serde_json`, an optional `toml` and each other.
- The code above holds 10 `unsafe` sites and calls `nix` and its re-exported `libc`. The
  gate takes `nix` 0.31 with `socket`, `fs`, `process`, `user`, `poll` and `signal`. The
  confinement needs `fs` and `user`, and raw system calls through `libc`.

**Three ways:**

1. **A new floor crate, `weaver-confine`:** the shared 255 lines and the parameterized
   100, depending on `nix` alone. The gate and the harness link it. Every crate still
   links at most the floor, and the two floor crates stay free of `unsafe`.
2. **Into `weaver-types`, behind a feature:** the floor gains `nix` and `unsafe`.
3. **A copy in the harness:** about 355 lines held twice, each with its own tests.

**My lean is the first.** Act 3's check of a laid loop (section 6) also starts the
runner under the same walls, and that start belongs to admin. So the walls are needed by
three crates, not two. **The operator rules on the numbers.**

## 5. The end

**How the worker stops a loop.**

- At the leave, after the drain, the worker shuts its end of the pair down, then closes
  it.
- The runner reads the end of file and exits 0 (`test_runner.py`, "the harness's end
  closing ends the runner with 0").
- **Measured, under every wall, five runs:** the runner exited 2.8 to 3.8 ms after the
  shutdown.

**Who reaps it.** The worker reaps it, by the leave's shared deadline: 150 s from the
lower's start (`lifecycle.rs`:3020-3032, `LEAVE_BOUND_MS` `wire.rs`:71). A runner not
gone by then is killed (`reap_by`, :4284-4302).

**What the worker's death does to it.** The child arms `PR_SET_PDEATHSIG` to `SIGKILL`
before its exec, and compares `getppid` with the worker's pid, as every organ does
(`spawn.rs`:14-20). So the loop dies with the worker. Its read meets the end of file in
any case.

**What an unload waits for, and for how long.** The runner's exit, which is the
milliseconds above, and at most the shared deadline. The wind-down turn does not reach
the loop (section 7), so stopping the runner after the drain takes nothing from that
turn.

**Point 17: who holds which end.** A worker that forks while it holds the loop's far end
makes the relay's look miss the end of file. The look is the poll before each pass
(contract section 3.2). The next read then breaks as an end of file instead.

- **After the start, the worker holds the near end, and the runner holds the far end at
  3.** The worker drops its copy of the far end in the same step as the fork, before any
  further fork, as it drops the gate's (`drop(gate_child)`, `lifecycle.rs`:2810).
- **What closes on exec:** every end the worker makes is `SOCK_CLOEXEC`. The far end
  placed at 3 is the one descriptor born without it.
- **A fork the worker makes later copies only the near end, and only until that child's
  exec.** That window never hides the loop's end of file from the relay.
- **The stop shuts the end down before it closes it.** Measured in act 1: a shutdown
  stands at once even where a forked child holds a copy, and a close alone does not.
- **The loop is forked last at the enter,** so no organ's fork window holds the far end.

**The interrupt signal.** The runner catches everything around `on_turn`, so `SIGINT`
during a pass refuses the pass and does not stop the runner. **The stop leans on no
signal:** the end of file first, then `SIGKILL` at the deadline.

## 6. What reaches the operator

**Today:** a break or a refusal is one line on the runner's standard error, and one line
of the relay's on the worker's (contract section 4).

**In act 2, the runner's standard error is the loop's last word,** handled as an organ's
(`spawn.rs` `LastWord`, :196-260).

- A reader thread copies every line into the worker's standard error, which is
  `worker.log` in the territory (`start.rs`:953-955, :987-989).
- It keeps the last line that parses as JSON.
- The runner writes each of its lines as one JSON object (`{"loop": ..., "pass": n,
  "reason": ...}`), so the last one is kept typed.

**Where a page reads it.**

- `worker.log` is grouped to `weaver-<agent>-admin` and read by that group (the
  territory's layout, `create-agent.sh`:57-70).
- The management connector `weaver-admincon` may hold that group
  (`create-agent.sh`:828-829).
- So Weaver Web reads the reason through the management connector, from someone not at
  the appliance.
- **No new trace event in act 2** (ruling 3). Carrying the runner's reason into the
  record would be a new event, and that is the operator's.

**"Test is functional: is the loop functional"** (73 e). A check of a laid loop is:

1. the owner, mode, size and digest checks of section 2
2. the runner's grammar check
3. one made pass for each path of the compiled loop, against a made harness, with no
   model touched. The paths are a first turn with nothing seated, a later turn, pressure
   with its flush and recall, a continuity line, a seated prompt, and `independent`.

The runner is started as the agent's account under the same walls.

- **What it answers:** whether this file, on this appliance, starts under the walls,
  passes the grammar, and makes asks the relay takes on each path, ending each pass with
  `turn` or `refuse`.
- **It does not answer** whether the turns are good. That is evaluation (73 e).
- **Act 3 builds it as `check-loop`.** Act 2 keeps the door open in three ways:
  - the walls and the start are one function admin can call (section 4)
  - the runner names every failure in one typed line
  - the made harness of this brief's measurements becomes a tested tool in act 3, not
    test code in act 2.

## 7. Reload

**In the lifecycle's own terms,** a reload is `unload`, then `load`. Those are two fixed
lines the sudo rule grants today (`create-agent.sh`:274), with no new verb.

**The unload is a graceful leave:** drain, wind-down turn, lower, save point, unload
(`lifecycle.rs`:1540-1625).

**The wind-down turn, read and cited.**

- A graceful unload of a run holding an SPU, outside a diagnostic binding, runs one
  wind-down turn before its save point (`lifecycle.rs`:1434-1500, act 1's brief, point
  10).
- The turn is the harness's own, `ports.wind_down` (:1500). It never reaches the loop,
  under either loop.
- So each reload adds one turn and one save point to the record, then opens a new run.
- **Where it is skipped:**
  - a forced leave or a rollback (:1549, :1576)
  - a record already latched (:1582).

**What the load does.** It reads the standing folder again (section 2) and starts a new
runner. A laid change takes effect at the next load and at no other time.

## 8. The identity run

73 (c) A: today's loop written as wires, run on an agent, with its trace identical to
the compiled loop's, by the contract's section 6.

**What runs.**

- Two agents made fresh by the same steps, under the same name and declarations.
- One has no loop folder (the compiled arm), and one has today's wire file laid (the
  wires arm).
- Each runs the same script, and the compiled arm runs twice first, as the control
  (section 6, "A control arm comes first").

**The script, path by path, as section 6 names the paths:**

1. **Load 1, `continuous`, nothing seated:**
   - a first turn (shape asked, no continuity line, since there are fewer than two runs)
   - a later turn
   - turns until fullness passes four-fifths of capacity (the flush and the recall).

   Unload.
2. **Load 2:** a first turn with earlier runs (the continuity line). Unload.
3. **The declaration set to `independent`, load 3:** a turn with nothing seated. Unload.
4. **The declaration set back to `continuous`, load 4:** the seeding turn
   (`deploy/turn.py <agent> --system`). Unload.
5. **Load 5:** a first turn with a prompt seated. Unload.
6. **The declaration set to `independent`, load 6:** a turn with a prompt seated.
   Unload.

Each arm keeps every session short of the recorder's high-water mark, and each must hold
no state answer that timed out (section 6's two timing paths).

**What the comparison reads.** The two records' canonical lines, parsed, through section
6's three steps:

1. the clocks set aside
2. each arm's `composer` checked, then set aside
3. the minted identifiers renamed in order of first appearance.

The program is a standard-library Python script with its own tests, outside the agent's
runtime.

**On what kind of machine, with which model.**

- **The build machine has no card and no model,** and nothing here serves the decode
  path without one. The state-flow suite's scripted organs answer the foundation path,
  not `serve` (`tests/state_flow/organs.rs`:1-2).
- **Not known:** whether `weaver-spu`'s `gguf` backend serves on the CPU with a small
  model the operator names. A reading of its device election and one load would measure
  it.
- **If it does,** the whole script runs here, with that model.
- **If not,** act 2 writes a scripted decode stand-in, a test binary that answers the
  decode seam deterministically. The whole script then runs here with the real worker,
  the real gate and the real state member, and no model.
- **Only an agent appliance with its cards measures the run with the declared model.**
  That is one seeded session each way, the control arm first. It measures whether
  generation repeats bit for bit on that machine. Where the control arms differ, the
  test reports it and does not compare the wires.

**What comparing the saved images costs** (owed by act 1's brief).

- Each save point's file, less its stamp line (`weaver-state` `save_point.rs`:22-33), is
  compared byte for byte.
- That is one function and one read per save point, with six save points in the script.
- **Not known:** whether two SQLite images serialized after the same operations are
  byte-equal. The comparison itself measures it.

## 9. What act 1 left

1. **Known item 1: a failure at the interpreter's own limits inside a board call can
   lose a return the board had already read.**
   - The reader ran it at 988 to 990 calls deep. This brief met it again with
     `MemoryError` at a 32 MiB limit (section 3).
   - **The fix:** the board takes a whole frame, or the whole of a series' slices, into
     bytes, and keeps the raw frame before it parses it.
   - A limit raised while parsing leaves the frame kept. The runner's refusal path waits
     on the kept frame, so the pass is refused and the seam holds.
2. **Known item 2: a write of the runner's own line can raise, where the wires closed
   standard error.**
   - **The fix:** one helper for every line the runner writes. It catches any exception
     from the write and goes on. The pass is refused whether the line was written or
     not.
3. **Known item 3: the board case "Refused raised by a caller is no breach the board
   met" asserts nothing of the board.**
   - **The fix:** it asserts `board.broken is None` afterwards, and a perturbation row
     that records the caller's exception goes red.
4. **A wire file nested past the interpreter's limits ends the runner with a traceback
   and no named line.**
   - **The fix:** `load` catches `RecursionError`, `MemoryError` and the parser's own
     errors, and names one line: "the wire file nests past the interpreter's limits".
     The runner exits 1 before its `hello`.
5. **On an interpreter older than 3.12 the runner fails with an attribute error**
   (`ast.TypeVar`).
   - **The fix:** the runner's first statement checks `sys.version_info`, and names
     "weaver-loop runner needs Python 3.12 or later, and this is X.Y". It exits 1 before
     its `hello`, so the load is refused, naming it.
   - **The appliance carries Python 3.14**, by the sealed image's program list
     (`deploy/image-programs.txt`:779-782). That is unmeasured on a running appliance.

## 10. The children, in order

| # | Holds | Tests and perturbations | Size |
| --- | --- | --- | --- |
| 1 | This brief, text alone | none | 1 file |
| 2 | The confinement, as ruled in section 4 (my lean: `weaver-confine`, the gate moved onto it with no change of behaviour) | the gate's suites unchanged and green (`tests/landlock.rs`, `confine/tests.rs`). The manifest tests pin the new edge. A row per moved function | about 6 files, about 400 lines moved |
| 3 | The runner's five fixes of section 9, typed lines, the dumpable clear if ruled | a case and a row each, in `test_runner.py` and `test_weaver_board.py` | 4 files |
| 4 | The loop folder: `create-agent.sh` lays `/etc/weaver/<agent>/` and refuses the names `admin` and `stack`, `decommission.sh` removes it, the stack lays `<prefix>/lib/weaver-loop/` and its grant file, admin's checks of section 2, the enter carrying the election, admin's Spec | `test_plans.py`, admin's tests for each refusal, the workspace gate (`deploy/` changes) | about 10 files |
| 5 | The worker starts the loop: the fork with its walls, the relay at the enter, the composer per run, the stop, the reap, the last word, the harness and trace Specs | the start, the walls and the stop driven against the real runner in the harness's tests (unprivileged, as measured here), and a row per wall and per refusal | about 8 files |
| 6 | The identity run: the comparison script, the stand-in if needed, the script of section 8, and the procedure for the appliance | the comparison's own tests, the control arm, the run here | about 5 files |
| 7 | The documents of section 12 | none | about 10 files |

**What act 2 does not do:**

- Laying a loop from Weaver Web, `install-loop`, `remove-loop` and `check-loop` are act
  3's.
- No trace event for a loop's break, and no gentler end than `ChannelLost` (rulings 3
  and 6).

## 11. Questions for the operator

Each is a situation, one yes or no, and my lean. I assume no answer.

1. **When a loop you wrote refuses a turn or breaks, the agent stops serving until
   someone reloads it.** Should a later act make it refuse only that turn and keep
   serving? Lean: yes, in a later act. Act 2 keeps today's end.
2. **The agent's own Python loads its standard library from files compiled when Python
   was installed.** Your ruling says the cache is off. Should the loop also skip those
   shipped files, at about 40 ms more each time the agent loads? Lean: yes.
3. **Other programs under the agent's account can read the loop's memory unless it marks
   itself private.** That needs one more system library and a module the runner drops at
   once. Should the runner mark itself private? Lean: yes.
4. **The agent's own group reads the loop's folder, and Weaver Web's gate connector may
   be a member of that group.** Should the gate connector be able to read the loop's
   code? Lean: yes. It is Weaver Web's own text.
5. **Each reload saves the agent's state and asks the model for a closing turn first.**
   Should a reload made only to swap the loop skip that closing turn? Lean: no. Keep the
   graceful unload.
6. **A loop's failure reason is written to the agent's log, which Weaver Web can read
   through the management connector.** Should the reason also be in the agent's record
   (the trace), as a new kind of event? Lean: not in act 2. Yes later, if the page needs
   it in the record.

## 12. The documents

The candidates are those of the plan of 2026-11-25, section 8, as act 1's brief amended
them at point 6. Each is re-read in the child that changes it, and each pull request's
body lists the ones it changes.

- **Made true by this act (child 7):**
  - 1, the apex PRD section 5.5. A loop laid in the agent's loop folder runs, and the
    compiled loop runs where none is laid.
  - 4 to 7, the harness Spec.
  - 8, the harness PRD.
  - 9, `Loops/basic-inference-loop.md`.
  - 10, the store crate's PRD.
  - 11, the trace Spec's `composer`.
  - 12, admin's Spec gains the folder check. The types Spec does not change.
  - 13, `deploy/REDEPLOY.md`.
  - 15, `dev_loop`'s head comment.
- **Made true by act 3:**
  - 2, the apex PRD section 9: the loop installed over the management plane.
  - 3, the vision: swapping the loop is an install and a reload.
- **Candidate 14, WeaverAgent's `CLAUDE.md`,** changes on the operator's word alone.
