# Brief: the loop as a process, act 2 of the loop

Written 2026-11-26. Act 2 of three, on the operator's rulings of 2026-11-25 (73 a to e)
and 2026-11-26 (73 f and i), and the Planner's rulings of 2026-11-26.
Act 1, the seam, merged into `act/the-loop-seam` as #638 to #643. Its base is #644.

**What act 2 leaves.** The worker starts the runner at each load, through a launcher
that lays the walls. A loop laid by hand in the agent's loop folder runs today's loop
written as wires on an agent, and its trace is shown identical to the compiled loop's
by the contract's section 6. Laying a loop from Weaver Web is act 3.

**How each line here is held.** Each line is one of three things:

- read in the source at `act/the-loop-seam` `e33ff15b` and cited
- measured, with the instrument named
- marked as not known, with what would measure it.

**The measurements were made on the build machine:** Fedora 45, kernel 7.3.4, Landlock
ABI 11, Python 3.15.2, the kernel's ptrace scope 1.

- A made harness held the pair at descriptor 3 and drove today's wire file
  (`tests/fixtures/loop-wires-today.py`) through three passes. Together they hit every
  ask the compiled loop makes under `continuous`: a first turn, then two later turns
  under pressure, each with its flush and recall.
- A launcher laid each wall on itself, then executed the runner exactly as section 1
  below starts it. It is a measurement tool written in Python, not the launcher of
  section 4.
- **A wall is called sufficient where the runner, under it, sent its hello, served the
  three passes whole and exited 0.**

## Rulings this act stands on

1. **The loop runs as the agent's own account** (73 d, the Planner on 2026-11-25 20:11
   UTC, ask 1). Root lays the folder, its owner and its modes. The launcher lays the
   walls.
2. **The standing loop folder is the election, and the declaration has no key** (act 1's
   brief, point 6).
3. **A broken loop is `ChannelLost`, and service ends as today** (the Planner, point 1).
   There is no new state and no new row in the lifecycle table.
4. **A pass is bounded at 10 seconds** (point 2, `LOOP_PASS_BOUND_MS`, `wire.rs`:58).
5. **A failed `hello` refuses the load** (act 1's ruling 3, its lean taken here).
6. **Test is functional: "is the loop functional"** (73 e). Act 3 calls the check, and
   act 2 keeps its door open (section 6).
7. **No date gates the compiled loop** (73 f). The operator's words: "No date. The
   compiled loop runs until the wire loop shows an identical trace, then the wire loop
   takes over. The acts run in order." The identity run of 73 (c) A is the gate
   (section 8).
8. **The Planner's rulings of 2026-11-26 01:46 UTC**, each built into the section named:
   - a launcher lays the walls, and the harness's fork and the gate do not change
     (sections 1, 3 and 4)
   - the `composer` keeps today's vocabulary (section 2)
   - the loop makes no new process (section 3)
   - no dumpable call, no `ctypes`, no `libffi` in the grant (section 3)
   - the shipped bytecode is skipped (section 1)
   - no gentler end, no skipped closing turn and no trace event in this act
   - the gate connector may read the loop folder, with nothing to build
   - the runner's descriptors 0 and 1 are the null device (section 1)
   - the build machine holds no agent: no account is made there and no root act is done
     there for the identity run (section 8).

## 1. The start

**Who starts the runner, and when.** The worker, at the enter, once per load:

1. The worker forks the SPU, waits for residency, then forks the gate
   (`lifecycle.rs`:2535-2565, :2785-2810).
2. The loop comes last. The worker makes the pair, forks the launcher by the organ fork
   as it stands, drops its copy of the child's end at once, and makes the relay. Making
   the relay reads the `hello` under its own 10 s (contract section 3.1).
3. A launcher that refuses, or a runner that does not start, sends no `hello`, or sends
   a wrong version or digest, refuses the load, closed (section 5, the failed start).
4. Only then is the run loaded and idle.

The compiled loop's load is unchanged where no loop stands.

**A binding with no gate.** The enter returns early where `gate_raise` stands no gate
(`lifecycle.rs`:2772-2776), so "the loop comes last" misses that path. That path is the
diagnostic binding alone (`gate_raise`, :571-592). Read: the loop entry is called only
when a turn arrives through the gate (`serve_gate_wake`, :774, and the call, :1136). So a
diagnostic binding never runs a loop. **A diagnostic binding checks no folder and starts
no runner,** and its `composer` stays the compiled one.

**The fork and its thread.** The parent-death signal fires when the forking thread ends,
not the process (`spawn.rs`:36-41). The loop's fork stays on the serving thread, as
every organ's does.

**The launcher, as the Planner's ruling 1 has it.** The worker forks it by the organ fork
as it stands, six calls and no more (`spawn.rs`:6-12). The fork places the far end at 3
and the last word's pipe at 2 (`spawn.rs`:118-127). The launcher then, with one thread:

1. clears the environment to one pair, `LANG=C.UTF-8`
2. opens the null device and places it at 0 and 1
3. reads the grant from its root-owned file (section 3, Landlock)
4. lays no-new-privs, the limits, the filter and Landlock on itself, in the shell's
   order
5. execs the interpreter.

**Its exact command, as the launcher execs it:**

```text
<interpreter> -I -B -S -X pycache_prefix=<an empty folder root owns> \
    <prefix>/lib/weaver-loop/runner.py /etc/weaver/<agent>/loop/wires.py
```

- **The environment, measured as planned.** Under every wall, the runner served the
  three passes with `LANG=C.UTF-8` alone (`env -i LANG=C.UTF-8` before the measurement
  launcher), and again with no variable at all.
  - The worker's own environment is `PATH`, `HOME`, `LANG` and, where set,
    `LD_LIBRARY_PATH` (`weaver-admin` `start.rs`:980-992). Without the clear, the runner
    would inherit it through the organ fork's rule (`organ_environment`,
    `spawn.rs`:171-193).
- **Descriptors 0 and 1 are the null device.** Read: admin points the worker's standard
  output at its log (`start.rs`:988), and the organ fork places descriptor 2 alone
  (`spawn.rs`:122-127). Without the launcher's step 2, the runner would hold the
  worker's log open for writing. The launcher opens the null device before it lays
  Landlock, so the grant does not name it.
- **The interpreter, its digest and the grant come from the grant file,** which the
  stack computes (section 3). The enter carries none of them.
- **The parent-death signal survives both execs**, the launcher's and the interpreter's.
  Measured: a child that armed `PR_SET_PDEATHSIG` to `SIGKILL`, then executed one
  program that executed a second, died with its parent, and so did a child after one
  exec. Each stood as the last program before its parent died. Child 5 names the test:
  "the runner dies with the worker, through the launcher".

**How descriptor 3 reaches it.**

- The pair is made with `SOCK_CLOEXEC` in the `socketpair` call, as every organ pair is
  (`channel.rs`:472-482).
- The child places its end at 3 by `dup2`, which is born without close-on-exec, as
  `place_child_ends` does for the organs (`spawn.rs`:28-31, `channel.rs`:1102-1160).
- **Measured under a made harness, not under the worker:** at the first pass the runner
  held exactly descriptors 0, 1, 2 and 3. Child 5 measures it under the real worker.

**Where `runner.py`, the board and the launcher stand.**

- **Where:** `<prefix>/lib/weaver-loop/`, with `<prefix>` `/opt/weaver` by default
  (`bootstrap-stack.sh`:71). The folder is `root:root 0755`, and `runner.py`,
  `weaver_board.py` and the grant file are each `root:root 0644`. The launcher is a
  binary in `<prefix>/bin`, as the worker is.
- **Who lays them:**
  - `bootstrap-stack.sh` and `update-stack.sh`, beside the binaries they install
    (`bootstrap-stack.sh`:406-416).
  - The carry set as well, once `install-carry.sh` lands. It is draft #632, not merged.
    The appliance's run of section 8 depends on it or on the stack's scripts run there
    (section 8).
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
- **An appliance made again loses the loop folder** (inferred: the folder is on the
  image, and the territory on the state drive). By the operator's rule a new appliance
  is a new agent. Loop install lays the loop again, and the load event names the loop
  that ran.

**The bytecode cache, off, and the shipped bytecode skipped** (the Planner's ruling 5).

- `-B` writes no bytecode. The folders are root's, and the walls grant no write
  anywhere.
- `-X pycache_prefix` points the interpreter's bytecode reads at an empty folder the
  walls do not grant. So the interpreter reads none of the standard library's shipped
  bytecode either.
- **Measured, five starts each, every wall laid:**

  | Bytecode | `hello` arrives after |
  | --- | --- |
  | the shipped bytecode read | 35 ms |
  | no bytecode read | 73 to 77 ms |

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
and the load is refused. Child 4 names the test.

**The enter carries the election, and so the types change.** `EnterPayload` refuses
unknown fields (`wire.rs`:744-752), so a member for the election is a change to:

- `weaver-types-Spec`, the enter's payload
- `weaver-admin-harness-contract` sections 3 and 5, which define what admin supplies in
  the enter.

The member names the wire file's path and admin's digest, and is absent where no loop
stands. The interpreter, its digest and the grant stay out of the enter: the launcher
reads them from the grant file.

**How one load's relay reaches the entry.** `serve` takes the entry once for the
binary's life (`lifecycle.rs`:663-677, `main.rs`:111-113). At each load:

- where the enter carries no election, the run holds no loop arm, and a turn calls the
  entry as today
- where it carries one, the run holds the loop arm (section 5), and a turn calls the
  relay's pass in place of the entry. The relay answers the board's asks through the
  same `Ports` by its `Seat` calls (contract section 2.2).

The choice is made per run, from the run's own field, so two loads of one binary may run
different loops.

**The load event's `composer` names what ran,** in today's vocabulary (the Planner's
ruling 2).

- **Today:** `serve` takes the composer once for the binary's life, "before any enter
  can author a load" (`lifecycle.rs`:663-677), and `LoadFacts.composer` copies it
  (`lifecycle.rs`:2215-2218, `wire.rs`:521, :538-544).
- **In act 2,** the worker sets each run's composer from its election.
  - The compiled loop's composer stays `{binary: "worker"}`, unchanged by a byte.
  - The wires' composer is `binary`, `file` and `sha256`, and no more (the fields kept in
    the vocabulary, `event.rs`:687-693, trace Spec :889-893).
  - `binary` is `worker`, as today.
  - `file` holds the wire file's path as admin read it, `/etc/weaver/<agent>/loop/wires.py`.
  - `sha256` is the digest the relay accepted in the `hello`, not admin's election
    alone. So a wires `composer` shows that a relay stood.
- **WeaverAnalysis, read by the Planner at `12a7243`** (`src/capture.rs`:67-90): its
  reader takes `binary`, `file` and `sha256` by name and ignores any other member. A
  compiled record and a wires record differ by `file`, which its Spec refuses as
  incomparable. Act 2's identity run compares by its own script (section 8), not by
  WeaverAnalysis.

**The names an agent may not take.** The loop folder sits at `/etc/weaver/<agent>/`,
beside admin's config bases and the stack's record.

- `decommission.sh` discovers its config bases as every `/etc/weaver/admin*`
  (`decommission.sh`:10-11, :127).
- `agent_name_ok` admits `adminx` (`create-agent.sh`:817-822).
- So an agent named `adminx` would lay `/etc/weaver/adminx/`, and decommission would
  read it as a config base.
- **Child 4 refuses every name the pattern takes:** a name beginning `admin`, and
  `stack`. Its test makes `adminx` and is refused. Narrowing the discovery instead would
  change what decommission finds on a machine laid before this act.
- **The names `admin` and `stack` are an open question with the operator (75 e).** The
  refusal is the floor whatever he answers. A wider rule is his.

## 3. The walls, each with its measurement

**The order is the shell's,** laid by the launcher on itself before its exec
(`confine.rs` `apply`, :447-492): no-new-privs, the limits, the filter, then Landlock.

**Every wall at once, with the minimal grant below:** sufficient.

**The account, and no-new-privs.**

- The runner runs under the worker's uid and groups, the agent's own (ruling 1).
- The worker already holds no-new-privs, set by admin's start step before its exec
  (`start.rs`:964-971, :1016). It is inherited across a fork and an exec.
- The launcher sets it again, as the gate's shell does, because the filter needs it.

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

**The loop makes no new process** (the Planner's ruling 3).

- **Read:** the shell's filter refuses another architecture, x32, `io_uring_setup`,
  `socket`, `socketpair` and the file-attribute calls (`confine.rs`:161-179), and no call
  that makes a process. The shell's limits are four, with no process limit (:49-54).
- **Measured, the gap:** under today's walls with no bar, a probe in the interpreter
  made a child by `fork`, a thread, and a child by `posix_spawn`.
- **Measured, both bars,** each with every other wall laid:

  | Bar | `fork` | a thread | `posix_spawn` | the runner's three passes |
  | --- | --- | --- | --- | --- |
  | the filter refuses `clone`, `fork`, `vfork` and `clone3` with `EACCES` | refused, `EACCES` | refused | refused, `EACCES` | served, exit 0 |
  | `RLIMIT_NPROC` at 1 | refused, `EAGAIN` | refused | refused, `EAGAIN` | served, exit 0 |

- **The choice is the filter.**
  - It refuses the named calls in this process alone, whatever else the agent's uid
    runs. `RLIMIT_NPROC` is judged against the count of every process of the uid.
  - Its refusal is `EACCES`, as the shell's refusals are. `EAGAIN` reads as "try again",
    and a loop that retries would spin.
  - The runner needs no thread: it served the three passes with threads refused.
- **The probe is the walls', not a wire file's.** The grammar bans names, not
  attributes (`runner.py`:138-150), and the contract says a wire file reaches `builtins`
  by attribute (:426). The walls hold whatever the wire file reaches.

**The filter: no `socket` and no `socketpair`, measured, not read.**

- **The instrument:** a filter that kills the process on `socket`, `socketpair` or
  `io_uring_setup`, laid before the exec.
- **The result:** the runner served the three passes and exited 0. So from its exec to
  its exit it made none of the three calls.
- **The filter's own check, measured:** under it, `socket.socket(AF_UNIX)` and
  `socket.socketpair()` each die by `SIGSYS`, and a plain `print` lives.
- **Why the runner needs neither call.** It takes its end by `socket.socket(fileno=3)`
  (`runner.py`:256), which makes no `socket` call.
- **The launcher's filter, written for it** (section 4): the architecture check, x32,
  then `io_uring_setup`, `socket`, `socketpair`, `clone`, `fork`, `vfork` and `clone3`,
  each refused with `EACCES`, then an allow. The shell's file-attribute rows (#587) are
  for a shell that writes its home, and the loop writes nothing. Measured as above, less
  the x32 check, which the shell's filter holds and child 2 copies.

**Landlock: what the interpreter must execute and read, measured.**

- **The method:** the runner was started under a grant naming each file it maps, then
  run again with each grant dropped in turn.
- **The minimal grant that serves:**

  | Path | Right | Dropped alone |
  | --- | --- | --- |
  | `/usr/bin/python3.15`, the resolved interpreter | execute, read | the exec fails, `EACCES` |
  | `/usr/lib64/ld-linux-x86-64.so.2`, the loader | execute, read | the exec fails, `EACCES` |
  | `/usr/lib64/libc.so.6` | read | the loader refuses it |
  | `/usr/lib64/libm.so.6` | read | the loader refuses it |
  | `/usr/lib64/libpython3.15.so.1.0` | read | the loader refuses it |
  | `/usr/lib64/libgcc_s-16-20261029.so.1` | read | the loader refuses it |
  | `/usr/lib64/python3.15/`, the standard library | read, list beneath | the interpreter cannot find its encodings |
  | `<prefix>/lib/weaver-loop/` | read, list beneath | the runner does not open |
  | `/etc/weaver/<agent>/loop/` | read, list beneath | the wire file does not open |

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

**The grant, computed where it runs.**

- **These paths are the build machine's.** The sealed image's program list names
  `python3.14` (`deploy/image-programs.txt`:779-782, seal `68c570cc`). The Planner
  measured `/usr/bin/python3` as Python 3.15.2 on the first agent appliance on
  2026-11-26, with `libffi.so.8` and `libgcc_s.so.1` as links and `libc.so.6` and
  `libm.so.6` as files. So the program list is the record of what was sealed, and the
  appliance's interpreter is what the stack resolves there. The computed grant covers
  both, since it is computed from the interpreter that stands.
- **So the grant is not written into the code.** The stack's install computes it on the
  machine that runs it, from the resolved interpreter, its loader (`PT_INTERP`) and the
  libraries the loader resolves for it. It writes it into the grant file beside the
  runner, with the interpreter's path and sha256.
- **The computation resolves every link to its file,** since the shell's rule refuses a
  link (`open_judged`, `confine.rs`:582-604), and `libgcc_s.so.1` is a link on the
  appliance.
- **The launcher reads the grant file.** The enter does not carry it, and admin does not
  read it.
- **Who computes it again:** `update-stack.sh`, at each run, and the stack's install.
  A package update of the interpreter outside those scripts leaves the grant stale.
- **What a stale grant does at a load:** the launcher judges each path as the shell's
  grant is judged, and compares the interpreter's sha256 with the grant file's. A path
  that fails, or a digest that differs, ends the launcher before its exec with one line
  naming the path. The load is refused naming it (section 5, the failed start). **No
  runner starts with a wall missing.**
- Running the same drop-each measurement on a made appliance would measure its list.

**The dumpable flag: no call** (the Planner's ruling 4).

- **Measured: the exec sets it back.** With the flag cleared before the exec, the
  runner's `/proc/<pid>/fd` was still owned by its own uid, not root. The flag is 1
  after every exec here.
- **No `ctypes` call and no `libffi` in the grant.** A granted `libffi` makes `ctypes`
  importable from a wire file, and dropping it from `sys.modules` does not hold against
  an import by attribute (`runner.py`:138, :143).
- **What holds instead, measured:** from inside a Landlock domain, a process outside that
  domain could not be attached to (`EPERM`), nor its memory read by `/proc/<pid>/mem`
  (`EACCES`) or `process_vm_readv` (`EPERM`).
  - **The instrument:** a prober forks its target first, so the ptrace scope's
    ancestor rule is met and Landlock alone decides, then lays a domain on itself and
    tries all three.
  - **The control:** the same prober with no domain made all three and read the
    target's bytes. A target in a domain of its own, as the runner holds one apart from
    the shell's, was refused from inside the prober's domain as well.
  - So the gate's shell, in its own domain, cannot read the runner's memory, and the
    ptrace scope of 1 bars every other process of the uid that is not the runner's
    ancestor. The worker is its only ancestor under the agent's uid.
  - **Measured with a domain of the prober's own, not the shell's.** The kernel's rule
    does not read the domain's rules, only whether the target is inside it. A test from
    inside the shell's real walls would take a probe program granted execute in the
    gate's test grant (`tests/landlock.rs`). The busybox has no applet that attaches.
- The ptrace scope reads 1 on the build machine and, by the Planner's measurement, on
  the first agent appliance.

## 4. The confinement code

**As ruled:** the launcher holds its own copy of the kernel plumbing it needs, with its
own tests. The gate does not change in this act: not `confine.rs`, not its manifest,
not its one binary, not its one internal edge (`weaver-gate` `tests/manifest.rs`:1-4).
There are two holders and no third, since admin runs the launcher in act 3. Making the
two copies one waits for the operator's rethink of the repository's layout.

**What the launcher copies, what stays the shell's, and what it adds,** set against
`weaver-gate` `src/tools/confine.rs` (657 lines at `e33ff15b`):

| | Functions and items | Lines |
| --- | --- | --- |
| **Copied as written** | Landlock's kernel structures and rights (:56-89), seccomp's structures, opcodes and `op` (:90-139), `landlock_abi` and `abi_at_floor` (:266-295), `allow` and `open_judged` (:571-604), `add_rule` (:625-655) | about 190 |
| **Copied and written again** | `apply` (:447-492) and `rlimits_within_reach` (:241-265), which read the shell's own table (:247, :464, :474) and take the launcher's. The ruleset's creation inside `build_ruleset` (about 30 of :493-570) | about 75 |
| **Written for the launcher** | its filter: the shell's jumps are offsets to its last instruction (:168-170), so its first nine instructions cannot be taken unchanged. Its limits table and ABI floor. Its faults. Reading the grant file, the environment's clear, and the null device | about 120 |
| **Left with the shell** | `SHELL_FILTER`'s file-attribute rows and its view (:140-195), `Shell` and `Layers` (:296-312), `confined` and `confined_in` with home's `.tmp` (:313-375), `bare` (:376-395), `command` (:396-446), `judge_home` (:608-624), the shell's rules in `build_ruleset` | about 275 |

- The module's header comment is :1-19.
- The file holds 10 `unsafe` sites, 7 of them in the part the launcher copies.
- The gate's tests are `src/tools/confine/tests.rs` (892 lines) and `tests/landlock.rs`
  (1,911 lines). They stay the gate's. The launcher's tests are its own (child 2).

**What the loop's grant needs that the shell's does not:**

- **Execute on two files where the shell grants one.** The busybox is one file. The
  interpreter needs its loader.
- **Read on named library files.**
- **Read and list beneath three folders:** the standard library, the runner's, and the
  loop's.
- **A grant read from a file the stack computed.** The shell's paths come from its
  declared grant.
- **Nothing the shell has that the loop lacks needs adding.** The loop takes no home, no
  write, no `/dev` file and no TCP right.

**Where the launcher's source sits: my proposal is a second binary of the harness crate,**
`weaver-loop-launch`, under `crates/weaver-harness/src/bin/weaver-loop-launch/`, with the
copy in that binary's own modules and not in the harness's library.

- **The pins it meets:**
  - the harness's manifest test pins its binaries to `["worker"]`
    (`tests/manifest.rs`, `the_only_worker_target_is_rust_and_no_python_dependency_is_carried`).
    The pin becomes `["weaver-loop-launch", "worker"]`, and its intent, no Python
    dependency and no `pyworker`, is kept.
  - the harness already takes `nix` 0.31 with `fs`, `process` and `user`
    (`Cargo.toml`:41), and already holds `unsafe` (`spawn.rs`, `channel.rs`). No new
    edge, and the floor gains nothing.
  - the documents' mirror (`DocsMirrorTests`, `deploy/test_plans.py`:14353) asks
    nothing more: the binary's rules go in the harness Spec.
  - the rules file's sentence on the floor, "every crate links at most the floor"
    (`CLAUDE.md`:129), still holds.
- **A new leaf crate instead** would need its own PRD and Spec under `docs/crates/`
  (`DocsMirrorTests`), its own manifest test, and the "ten-crate" count in `CLAUDE.md`
  (:8) changed. `CLAUDE.md` is candidate 14, which changes on the operator's word alone.
- **Either way, no floor crate gains `unsafe`.** `weaver-types` and `weaver-traits` hold
  none (0 mentions in their sources). `weaver-traits` depends on `serde` alone, and
  `weaver-types` on `serde`, `serde_json`, an optional `toml` and `weaver-traits`.
- **No measurement here speaks against a launcher.** The measurements of section 3 were
  made through one.

## 5. The end

**The loop arm.** The run holds one field for the loop, absent where none stands: the
runner's pid, the relay over the near end, and the last word's reader. Every stop below
takes it from the run, so a leave tried again finds none to stop twice.

**How the worker stops a loop, on a graceful leave.**

- After the drain, the worker shuts its end of the pair down, then closes it.
- The runner reads the end of file and exits 0 (`test_runner.py`, "the harness's end
  closing ends the runner with 0").
- **Measured, under every wall, five runs:** the runner exited 2.8 to 3.8 ms after the
  shutdown.
- **The relay's shutdown:** `LoopRelay` holds its end with no way to give it back or
  shut it (`loop_relay.rs`:72-78). Child 5 adds one that shuts the end down and returns
  it, so the stop is the relay's own act.

**The leaves that skip the drain.** A forced leave and a rollback lower at once
(`lifecycle.rs`:1548-1551, :1577-1588), and the unwind after an organ's death takes no
drain. Each stops the loop arm the same way, shutdown then close, with no drain before
it. A leave tried again from S9 finds the arm already taken.

**Who reaps it.** The worker reaps it against the leave's shared deadline: 150 s from the
lower's start (`shared_deadline`, `lifecycle.rs`:3024-3031, `LEAVE_BOUND_MS`
`wire.rs`:71). The reap runs with the other reaps, after the lower, inside that deadline.
A runner not gone by then is killed (`reap_by`, :4284-4302).

**The failed start.** Where the launcher refuses, or the runner sends no `hello`, a wrong
one, or none within 10 s:

- the worker kills the child with `SIGKILL` and reaps it at once
- it closes the near end
- the load is refused through the enter's rollback (`after_load!`), which lowers the gate
  and the SPU that stand
- the launcher's or the runner's last line, kept by the last word, names why.

**What the worker's death does to it.** The launcher arms nothing new: the organ fork
arms `PR_SET_PDEATHSIG` to `SIGKILL` before the launcher's exec, and compares `getppid`
with the worker's pid (`spawn.rs`:14-20). The signal survives both execs, measured
(section 1). So the loop dies with the worker. Its read meets the end of file in any
case.

**What an unload waits for, and for how long.** The runner's exit, which is the
milliseconds above, and at most the shared deadline. The wind-down turn does not reach
the loop (section 7), so stopping the runner after the drain takes nothing from that
turn.

**Who holds which end of the loop's pair.** A worker that forks while it holds the loop's far end
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

**Today:** a break or a refusal is one line of plain text on the runner's standard error
(`runner.py`:210, :224, :232, :254), and one line of the relay's on the worker's
(contract section 4).

**In act 2, the runner's standard error becomes the loop's last word,** handled as an
organ's (`spawn.rs` `LastWord`, :196-260).

- A reader thread copies every line into the worker's standard error, which is
  `worker.log` in the territory (`start.rs`:953-955, :987-989).
- It keeps the last line that parses as JSON.
- The runner's lines become one JSON object each (`{"loop": ..., "pass": n, "reason":
  ...}`), so the last one is kept typed. The launcher's one line takes the same form.
- **In act 2 the reason stays in `worker.log`, typed.** How it reaches a page is act
  3's. By the Weaver Web seat's report of 2026-11-26 01:23 UTC, which the Planner did not
  check in its source, Weaver Web's feeds are a verb's answer, `admin.log`'s lines and
  the trace, and nothing of it reads `worker.log`.
- **No new trace event in act 2** (ruling 8, the Planner's rulings of 2026-11-26 01:46
  UTC).

**"Test is functional: is the loop functional"** (73 e). A check of a laid loop is:

1. the owner, mode, size and digest checks of section 2
2. the runner's grammar check
3. one made pass for each path of the compiled loop, against a made harness, with no
   model touched. The paths are a first turn with nothing seated, a later turn, pressure
   with its flush and recall, a continuity line, a seated prompt, and `independent`.

The runner is started as the agent's account, through the same launcher.

- **What it answers:** whether this file, on this appliance, starts under the walls,
  passes the grammar, and makes asks the relay takes on each path, ending each pass with
  `turn` or `refuse`.
- **It does not answer** whether the turns are good. That is evaluation (73 e).
- **Act 3 builds it as `check-loop`.** Act 2 keeps the door open in three ways:
  - the walls are one program admin can run, the launcher (section 4)
  - the runner names every failure in one typed line
  - the made harness of this brief's measurements becomes a tested tool in act 3, not
    test code in act 2.

**For act 3, recorded here and planned nowhere in act 2:**

- The Weaver Web seat adopts the names `install-loop`, `check-loop` and `remove-loop`.
  It asks that each line answer one JSON object in `weaver-admin`'s shape, with an
  `event` member, within 64 KiB. A refusal carries one word from a closed list, and a
  line number where the grammar names one. It asks that a break's reason reach
  `admin.log` or `check-loop`'s answer.
- The first install also creates the gate connector, with nobody at the appliance's
  screen (the Planner's decision).
- **Act 3's laying line may read the wire file on its standard input, with root's
  authority** (73 i). The operator's words: "Yes. Ruled again when sign-in returns. Each
  install puts the digest of gatecon's destinations file in the trace." So the line is
  ruled again when sign-in returns, and each install writes the digest of gatecon's
  destinations file to the trace.
- **An open point for act 3:** "then the wire loop takes over" (73 f) is not act 2's to
  build. Which step makes today's loop as wires the standing loop of a new agent is
  open. Act 2 builds nothing for it.

## 7. Reload

**In the lifecycle's own terms,** a reload is `unload`, then `load`. Those are two fixed
lines the sudo rule grants today (`create-agent.sh`:274), with no new verb.

**The unload is a graceful leave:** drain, wind-down turn, lower, save point, unload
(`lifecycle.rs`:1540-1625).

**The wind-down turn, read and cited.**

- A graceful unload runs one wind-down turn before its save point where the run is a
  serving one, its gate not yet lowered, and its SPU standing (`lifecycle.rs`:1444-1456,
  act 1's brief, point 10).
- The turn is the harness's own, `ports.wind_down` (:1500). It never reaches the loop,
  under either loop.
- **So each graceful reload of a serving run with its SPU standing, over a record not
  latched, adds one turn and one save point to the record, then opens a new run.**
- **Where the turn is skipped:**
  - a forced leave or a rollback (:1548-1551, :1577-1588)
  - a record already latched (:1582)
  - a diagnostic run, a gate already lowered, or no SPU (:1449-1454).

**What the load does.** It reads the standing folder again (section 2) and starts a new
runner. A laid change takes effect at the next load and at no other time. Child 4 names
the test.

## 8. The identity run

73 (c) A: today's loop written as wires, run on an agent, with its trace identical to
the compiled loop's, by the contract's section 6. **It is the gate the operator named**
(73 f): "The compiled loop runs until the wire loop shows an identical trace, then the
wire loop takes over."

**What runs.** Four runs under one agent name, made in turn: three arms, then the
negative control.

- the compiled arm twice first, as the control (section 6, "A control arm comes first"),
  then the wires arm, then the wires arm once more with one byte of its wire file
  changed, as the end-to-end negative control (below)
- each run is an agent made fresh by `create-agent.sh` with the same name and
  declarations, runs the script, has its record and save points copied out, and is taken
  down by `decommission.sh` before the next is made
- `decommission.sh` takes every agent off a machine (:2), so no two runs stand at once,
  and the machine holds no other agent during the run.

The compiled arms have no loop folder. The wires arm has today's wire file laid, and the
fourth run that file with its one byte changed.

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

**What the comparison reads.** The records' canonical lines, parsed, through section 6's
three steps:

1. the clocks set aside
2. each arm's `composer` checked, then set aside
3. the minted identifiers renamed in order of first appearance.

**What shows that the wires ran,** since step 2 sets the `composer` aside:

- **the `composer`'s `sha256`** in each of the wires arm's six loads equals the laid
  file's sha256, which the relay accepted in the `hello` (section 2)
- **the passes served:** the runner writes one typed line at its end of file naming the
  passes it served, kept in `worker.log` by the last word (section 6). The count over
  the wires arm's six loads equals the script's turns that reach the loop
- **the end-to-end negative control, the fourth run:** the wires arm run once more with
  one byte of the wire file's prompt text changed. The comparison must turn red, end to
  end. A comparison that stays green there fails the run.

**There are two negative controls.** One is over recorded fixtures, on the build machine:
the comparison script's own tests (child 6). The other is end to end, on the appliance:
the fourth run above. The first shows the script can turn red. The second shows the run
can.

**The saved images.** The check line is a sha256 over the stamp line and the image
(`save_point.rs`:12-34), so it differs whenever the stamp does. The comparison reads the
image's bytes alone, after the check line. **The control decides:**

- where the two compiled arms' images are equal, the wires arm's must be, and a
  difference fails the run
- where they are not, images are not compared, and the run's result says so.

That is one function and one read per save point, with six save points in the script.
Whether two SQLite images serialized after the same operations are byte-equal is not
known. The control measures it.

**The program** is a standard-library Python script with its own tests, outside the
agent's runtime.

**Before it runs:** the review's finding 8 on #644 is fixed (section 9). The run must
stand on a wire file with no known difference from `compose` but the one the contract's
section 8 names.

**On what kind of machine, with which model.**

- **The run needs two root acts and accounts:** `create-agent.sh` and `decommission.sh`
  as root, and each arm's accounts. By the Planner's ruling 9 the build machine holds no
  agent. **So the identity run runs on an agent appliance, not here.**
- **With its cards and the declared model,** that is one seeded session each way. Where
  the control arms differ, the run reports it and does not compare the wires.
- **What it depends on there:** the worker and the launcher built from act 2, and the
  runner, the board and the grant file laid by the stack's scripts or by the carry set
  (section 1). The carry set is draft #632. So the appliance's run depends on one of
  them standing on that appliance, and act 2's code does not.
- **What this machine measures:** the comparison script's own tests, the first of the
  two negative controls, over recorded fixtures, the launcher's walls, and the start, the walls and the
  stop driven against the real runner by the harness's unprivileged tests (child 5). It
  measures no model and no agent.
- **Only an agent appliance with its cards** measures the run itself, and whether
  generation repeats bit for bit on that machine.

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
   - It already asserts `calm.broken is None` (`test_weaver_board.py`:326-328), and
     `wb.check_delta` never touches `calm`, so no row can turn it red.
   - **The fix:** a case that can fail. A `Refused` raised by a check inside a board
     method, before any frame is read, leaves the board unbroken. Its row marks every
     `Refused` as a breach and goes red.
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
   - **The appliance carries Python 3.15.2,** by the Planner's measurement on the first
     agent appliance. The sealed image's program list names 3.14 (section 3).

**The review's sixteen findings on #644.** Read at `e33ff15b`. Where they are fixed, on
#644 or in this act's child 3, is the operator's answer. The table holds either way.

| # | Place | The fix, in a line | My reading |
| --- | --- | --- | --- |
| 1 | `dev_loop/tests/wires.rs`:85 | read the child's standard error to its end before the wait | as the Planner's |
| 2 | `dev_loop/tests/wires.rs`:203 | compare the delta's blocks as a list, not joined | as the Planner's |
| 3 | `loop_relay.rs`:812 | write `BeforeHello`'s kind quoted, as `UnknownKind` does, with a case holding a newline | **read in full:** any string reaches it, since `parse` checks only that the kind is a string (:500-504) |
| 4 | `loop_relay/tests.rs`:578, :582 | `Done` asserts its value, and `Faulted` asserts the whole fault | as the Planner's |
| 5 | `loop_relay/tests.rs`:767 | read the elapsed time when the relay returns, before the peer is joined | as the Planner's |
| 6 | `loop_relay/tests.rs`:781 | two waits, each under the bound and summing past it, and the second refused | as the Planner's |
| 7 | `loop_relay/tests/cross.rs`:96 | as 1 | as the Planner's |
| 8 | `tests/fixtures/loop-wires-today.py`:84, :88 | keep every member of an object as it is read, so the surrogate and depth checks judge a member a later one of its name replaces, then let the last member stand, as serde_json's `Value` does. Two cases join the comparison: a member named twice over a lone surrogate, and one over 128 levels, each refused by both sides | as the Planner's. Refusing every member named twice would part from `compose`, which takes the last (the comparison's "the last member of a name"). Fixed in child 3, before the identity run |
| 9 | the contract, :397 and invariant 4, :246 | the runner's sentence names a raise after `turn`, and invariant 4 names the break | as the Planner's |
| 10 | the contract, :398 | "the next pass runs" becomes: "The runner and the relay stand ready if the harness opens another pass. On an agent the harness opens none: a refused pass is `ChannelLost`, and service ends." | as the Planner's. The sentence lands in the contract with the other fixes |
| 11 | act 1's brief, :211 | the sentence keeps its condition, as this brief's section 7 now does | as the Planner's |
| 12 | `runner.py`:136 | item 5 above | as the Planner's |
| 13 | `runner.py`:188 | `compile`'s `SyntaxError` is named as a grammar refusal | **measured:** a `break` in a function outside any loop passes the grammar and ends the runner with a traceback, `SyntaxError: 'break' outside loop`, exit 1, before its `hello`. A `break` at the top level is refused by the grammar |
| 14 | `test_runner.py`:440 | close the case's copy of the loop's end, then assert an end of file, not a read that timed out | as the Planner's. I did not read the pair's timeout either |
| 15 | `test_weaver_board.py`:197 | `series_refused` takes `Refused` alone where no reason is given | as the Planner's |
| 16 | `weaver_board.py`:270 | a close inside a segment series breaks the seam, and `serve` exits 1 | as the Planner's |

## 10. The children, in order

| # | Holds | Tests and perturbations | Size |
| --- | --- | --- | --- |
| 1 | This brief, text alone | none | 1 file |
| 2 | The launcher, `weaver-loop-launch`, a second binary of the harness: its copy of the plumbing (section 4), its filter, limits and Landlock from the grant file, the environment's clear, the null device. The harness's manifest pin and Spec | a probe each wall refuses (below), the grant judged and refused on a stale path or digest, the parent-death signal through both execs. A row per wall and per refusal | about 8 files, about 400 lines |
| 3 | The runner's fixes of section 9, items 1 to 5 and finding 13, typed lines, the end-of-file line naming the passes served, and the review's findings if the operator puts them here, with finding 8 in any case | a case and a row each, in `test_runner.py` and `test_weaver_board.py`, and finding 8's two cases in the comparison | about 6 files |
| 4 | The loop folder: `create-agent.sh` lays `/etc/weaver/<agent>/` and refuses a name beginning `admin` and `stack`, `decommission.sh` removes it, the stack lays `<prefix>/lib/weaver-loop/` and computes the grant file, admin's checks of section 2, the enter carrying the election, the types Spec, the admin-harness contract's sections 3 and 5, admin's Spec | `test_plans.py`, admin's tests for each refusal, `adminx` refused, the two digests of a file changed between the reads, a laid change taking effect at the next load alone, the workspace gate (`deploy/` changes) | about 12 files |
| 5 | The worker starts the loop: the fork of the launcher, the relay at the enter, the loop arm, the composer per run, the stop, the reap, the failed start, the relay's shutdown, the last word, which keeps the runner's end-of-file line in `worker.log`, the harness and trace Specs | below, driven against the real runner in the harness's tests, unprivileged | about 8 files |
| 6 | The identity run: the comparison script, its negative control, the script of section 8, and the procedure for the appliance | the comparison's own tests, the control arms, the negative control | about 5 files |
| 7 | The documents of section 12 | none | about 10 files |

**What children 2 and 5 test, so a dropped wall goes red.** A run that serves under the
walls also serves with a wall dropped, so each wall gets a probe that the shipped wall
refuses:

- a socket made
- a read outside the grant
- a write anywhere
- a limit passed
- a process made.

**And a named test for each of:**

- the cleared environment
- descriptors 0, 1, 2 and 3 under the real worker
- the runner's death with the worker
- the compiled `composer` unchanged by a byte
- the kill and the reap after a failed `hello`
- the two digests of a file changed between the reads (child 4)
- a laid change taking effect at the next load alone (child 4).

**What act 2 does not do:**

- Laying a loop from Weaver Web, `install-loop`, `remove-loop` and `check-loop` are act
  3's.
- No trace event for a loop's break, and no gentler end than `ChannelLost`.
- Nothing for "then the wire loop takes over" (section 6, for act 3).
- **Nothing on the trace's place.** The operator's words of 2026-11-26 03:07 UTC place
  the trace at `/var/lib/weaver/<agent name>/trace`. Act 2 builds nothing on that, and
  its tests keep the trace where each test puts it.

## 11. Questions for the operator

None open from this brief. The six it asked were answered by the Planner's rulings of
2026-11-26 01:46 UTC:

- a refused turn that keeps serving, the first: not in this act, kept as a candidate for
  after January
- the shipped bytecode: skipped
- the dumpable flag: no call
- the gate connector reading the loop folder: yes, with nothing to build
- a reload that skips the closing turn: no
- a loop's reason in the trace: not in this act.

The names `admin` and `stack` stay with the operator as 75 (e) (section 2).

## 12. The documents

The candidates are those of the plan of 2026-11-25, section 8, as act 1's brief amended
them at point 6. Each is re-read in the child that changes it, and each pull request's
body lists the ones it changes.

- **Made true by this act (child 7, unless a child above names it):**
  - 1, the apex PRD section 5.5. A loop laid in the agent's loop folder runs, and the
    compiled loop runs where none is laid.
  - 4 to 7, the harness Spec, with the launcher's binary (child 2).
  - 8, the harness PRD.
  - 9, `Loops/basic-inference-loop.md`.
  - 10, the store crate's PRD.
  - 11, the trace Spec's `composer`: its vocabulary stays, and the text says what `file`
    and `sha256` hold for a wires run.
  - 12, admin's Spec gains the folder check, and the types Spec and the admin-harness
    contract's sections 3 and 5 gain the enter's election (child 4).
  - 13, `deploy/REDEPLOY.md`.
  - 15, `dev_loop`'s head comment.
- **Made true by act 3:**
  - 2, the apex PRD section 9: the loop installed over the management plane.
  - 3, the vision: swapping the loop is an install and a reload.
- **Candidate 14, WeaverAgent's `CLAUDE.md`,** changes on the operator's word alone.
