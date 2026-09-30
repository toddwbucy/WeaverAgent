# Redeploy log: thinkpad, 2026-09-30

The run log `deploy/REDEPLOY.md` section 5 asks for, kept as the work went. Times are
local (CDT). The runbook and the scripts `decommission.sh`, `bootstrap-stack.sh` and
`verify-load.sh` were written during this run, from the inventory below, and this is
their first use. Where a script refused or was wrong, the entry says so and the fix went
into the script, not around it.

## 0. Box facts, measured 14:30

    host        thinkpad, Linux 7.2.8-1-cachyos
    card        NVIDIA RTX PRO 5000 Blackwell Generation Laptop GPU, driver 615.71.09, 24463 MiB
    nvcc        cuda_13.4.r13.4 (pacman cuda 13.4.2-1), cccl 3.3.4-1 (inside the 3.1.4-3.3.4 window)
    g++         16.2.1; there is no /usr/bin/g++-15, which update-stack.sh names as NVCC_CCBIN's default
    postgresql  18.6-1.1, active; the operator has no role of its own
    toolchain   nightly-2026-02-13 via rust-toolchain.toml
    bulk store  192.168.0.203:/bulk-store on /mnt/bulk-store, 9.5T free
    sudo        password required; nothing privileged runs from the session unattended

## 0. What stood, discovered 14:30

Two stacks, four agents' remains, one live database. No `weaver-worker@` unit was
active; the slice `system-weaver\x2dworker.slice` was up with zero units, 17.5 h of CPU
since 2026-09-27. The journal's last seven days show `weaver-worker@karl.service` (7.6M
lines) and `weaver-worker@karl2.service` (19 lines, ending in a triton compile failure,
`collect2: cannot find 'ld'`, at 10:39 on 2026-09-29).

Config roots:

    /etc/weaver/admin                   allow-list: karl m1      binaries /opt/weaver/bin
    /etc/weaver/admin-stageb            allow-list: karl2        binaries /opt/weaver-stageb/bin, python SPU
    /etc/weaver/admin-stageb.bak-b62812e
    /etc/weaver/agents/acorn.yaml       the August layout, nothing reads it

Install prefixes:

    /opt/weaver/bin      pyworker weaver-admin weaver-gate weaver-spu weaver-state worker (2026-09-27 10:31)
                         weaver-spu-classify (2026-09-04, not in the install set)
    /opt/weaver/lib      15 objects: libggml-{base,cpu,cuda}, libggml, libllama, each .so/.so.0/.so.0.9.11
    /opt/weaver/lib.backup-20260927, seven backup-* directories from 2026-09-04 to 2026-09-27
    /opt/weaver/python-spu   cpython 3.14 prefix, three zipapps (850720e, dc3a0f7, current 07:43 today)
    /opt/weaver/models   qwen2.5-0.5b-instruct {q6_k, q8_0, bf16}.gguf, Qwen3-8B-BF16.gguf,
                         qwen2.5-0.5b-instruct-safetensors/   (about 19.7 G of the prefix's 24 G)
    /opt/weaver-stageb/bin   weaver-admin weaver-gate worker, current and .b62812e
    /etc/ld.so.conf.d/weaver.conf   one line, /opt/weaver/lib: how the members, which carry no RUNPATH,
                         resolve the engine objects. Found at 15:05 while checking the build's output,
                         after the first draft of the scripts; both now carry it.

Agents:

    acorn   user weaver-acorn (home /var/lib/weaver/agents/acorn), declared in YAML under /etc/weaver/agents
            and ~/.weaveragents; last touched 2026-08-25. Retired in all but account.
    karl    user weaver-karl (home /var/lib/weaver/agents/karl); ~/.weaveragents/karl.yaml, engine none,
            q6_k gguf; ~/.weaveragents/karl/trace.ndjson is 3.8 G. Four .pre-*-bak copies of the YAML.
    m1      users weaver-m1 (home /home/weaver-m1) and weaver-m1-state; ~/.weaveragents/m1.yaml, engine
            postgres, database and role weaver_m1; territory ~/.weaveragents/weaver-m1 with state/ (0700,
            member-owned) and a 647 K trace.
    karl2   user weaver-karl2 (home /var/lib/weaver/agents/karl2); ~/.weaveragents/karl2.toml, engine none,
            the safetensors artifact through the python SPU on the stageb root; 17 K trace.

    accounts   weaver-acorn 954, weaver-karl 952, weaver-m1 951, weaver-m1-state 950, weaver-karl2 949
    operator   todd is in groups weaver-m1 and weaver-karl (create-agent.sh's usermod), video, wheel

Record and log:

    /var/lib/weaver/agents/{acorn,karl,karl2}   0700 agent-owned; /var/lib/weaver/trace/acorn root 0700
    /var/log/weaver/admin-operations.ndjson     8.7 M, root 0640; admin-operations-stageb.ndjson 733 bytes
    /tmp   weaver-admin-{optional,spu-map,log}-<pid> from 2026-09-28, torchinductor_weaver-karl2

Store: role and database `weaver_m1`; `pg_hba.conf` and `pg_ident.conf` are postgres's
and were not read from the session, so their weaver lines are counted by the archive.

Models on the bulk store: the three 0.5b ggufs under `backups/opt-weaver-models-20260827`;
the safetensors directory under `models/Qwen--Qwen2.5-0.5B-Instruct-safetensors`; the 8B
gguf under `models/Qwen--Qwen3-8B/` is 256 bytes smaller than `/opt/weaver/models/Qwen3-8B-BF16.gguf`,
so the installed 8B is not a copy of it. Models are kept in place through the purge.

`sha256 /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf` =
`2f82233630c349ccf6b8daccf48f9a7865713d9f08a2eadfa456cebe9b97c7f5`, matching the value the
2026-09-29 handoff pins.

## Decisions this run rests on

- **Agents to stand back up: karl and m1.** acorn has been dead since August. karl2 was the
  stage-b python SPU trial and its last load failed in triton; the python SPU is its own
  install (`python-spu/README.md`) and returns when an agent needs it, not as part of the
  base stack.
- **Models stay.** See above.
- **The tree builds from `WeaverAgents/` at the suite split, with no git yet.** The build
  is named `nogit-<Cargo.lock sha>` until the repository has a root commit; the log
  records the lockfile sha so the members can be tied to a commit later.
- **Nothing is pushed and no repository is initialised by this work**, per the suite
  holds. The scripts and this log are files in the tree, to be committed when the
  operator opens WeaverAgents' history.

## 1. Decommission and archive

14:51  `sudo deploy/decommission.sh` (plan). Matched the inventory above: three config roots, two prefixes,
       four agents and five accounts, no units, the slice up and empty, role and database `weaver_m1`,
       one hba and one ident line. Two readings the unprivileged inventory could not take: `/var/lib/weaver`
       is 85M and `/opt/weaver/python-spu` is 5.0G. `/etc/ld.so.conf.d/weaver.conf` carries two lines,
       `/opt/weaver/lib` and `/opt/cuda/lib64`; bootstrap-stack.sh was writing one and now writes both.

14:55  `--archive`, first run. Two defects, both the script's. `mkdir` as root failed on the bulk store,
       whose NFS export squashes root to nobody, and the operator made the directory by hand; the one file
       root then wrote arrived owned by nobody. The run died silently in the box-facts block at the
       libraries line: `/opt/weaver-stageb` has no `lib`, `find` returned 1, and `set -e` ended the script.
       Nothing was removed. Fix: root reads and the operator writes (tar and pg_dump to stdout, the
       operator's shell writes the file), every `find` in the facts block guarded, `nvcc` found at
       `/opt/cuda/bin` when root's PATH lacks it, and a partial run's files cleared before writing.

14:58  `--archive`, second run, completed and verified: 15 files, 3.2 G on the bulk store at
       `/mnt/bulk-store/dev-archive-20260930-thinkpad`. The per-tarball size the script printed read 512 for
       most of them, NFS reporting unallocated blocks; the script now prints the apparent size. Checked
       after the purge, as the operator: SHA256SUMS verifies, every tarball lists, and inside are
       karl's 3,839,610,101-byte trace, m1's and karl2's traces, acorn's under /var/lib/weaver, the five
       declarations, 42 entries of /etc/weaver, 38,486 of /opt/weaver (bin, lib, python-spu, eight backup
       directories), `postgres/weaver_m1.dump` (11 K), the roles and both authentication files.

## 2. Purge

15:00  `--purge`, after re-verifying the archive. Stopped the slice; dropped database and role `weaver_m1`;
       removed the weaver line from `pg_hba.conf` and `pg_ident.conf` (backups `.before-decommission-20260930`
       beside them) and reloaded PostgreSQL; removed the five accounts with their homes and the three
       groups that outlived them; removed every path on the PURGE-LIST, the empty `/opt/weaver-stageb`,
       and reran ldconfig to zero engine objects. `/opt/weaver` holds `models` and nothing else.
       The operator ran the purge on the archive script's own word, before this seat had checked the
       archive; the check above came after and found it whole.

## 3. Build and install

14:45  `cargo build --release --locked --workspace --features weaver-spu/cuda,weaver-harness/pyworker,weaver-state/sqlite,weaver-state/postgres`
       started from the session, unprivileged, into `WeaverAgents/target/release`.
14:55  Finished in 9m54s wall, 175 min CPU, no errors. Tree named `nogit-eda642e70b70` (Cargo.lock sha).
       Members: pyworker 2.9M, worker 2.6M, weaver-admin 2.6M, weaver-gate 1.7M, weaver-spu 71.7M, weaver-state 5.0M.
       Engine objects at target/release/build/llama-cpp-sys-2-ac8331245f4ed19e/out/lib/: libggml-{base,cpu,cuda},
       libggml at 0.9.11 and libllama at 0.0.8783, each with its .so.0 and .so links. The .so names at the
       target root are dangling links, which is why bootstrap-stack.sh reads out/lib and not the root.
       weaver-spu NEEDED: libggml-base.so.0, libggml.so.0, libllama.so.0; no RUNPATH.

15:10  `cargo test --release --locked -p weaver-trace -p weaver-harness -p weaver-state --features weaver-harness/pyworker,weaver-state/sqlite,weaver-state/postgres`:
       228 passed, 0 failed, 24 ignored across 18 test binaries, 33 s. The ignored ones are the
       scratch-PostgreSQL and prebuilt-analysis suites and pass by not running.
15:12  `bootstrap-stack.sh` plan mode prints the box facts and refuses at `/etc/weaver/admin already stands`,
       which is the intended order.

15:05  Box checked clean from the operator's view: no weaver accounts or groups, no /etc/weaver, /var/lib/weaver,
       /var/log/weaver, ~/.weaveragents or ld.so.conf entry, zero engine objects in the loader cache, the
       slice inactive, no units, /tmp clear, PostgreSQL up. The operator's running session still names
       gids 950 and 952 that no longer exist; a fresh login clears it.
15:06  `bootstrap-stack.sh` plan: preflight passes, lock in step, 228 tests pass, build cached, engine out
       found at llama-cpp-sys's out/lib with 15 files and links. Plan printed as section 3 of the runbook says.

15:10  `sudo deploy/bootstrap-stack.sh --install` refused at `no nvcc on PATH`: the script builds with the
       operator's toolchain and asks for sudo only at install, and this seat had told the operator to
       prefix it with sudo. The script now refuses to run as root, naming the reason.

15:14  `deploy/bootstrap-stack.sh --install`, as the operator: 228 tests, cached build, six members installed
       at `nogit-eda642e70b70` (worker 2ffff352, weaver-admin 25af2f38, weaver-spu 7e06b10f, weaver-state
       f18db5f5, weaver-gate 11af5d12, pyworker baad4c6b), 15 library files and links, weaver.conf written,
       ldconfig run, weaver-spu resolving five engine objects from /opt/weaver/lib. Admin config written with
       an empty allow-list; /var/log/weaver root 0750; ~/.weaveragents todd 0755.

## 4. Agents

15:16  `create-agent.sh m1 --artifact .../qwen2.5-0.5b-instruct-q6_k.gguf --engine postgres` plan: both
       accounts, territory `~/.weaveragents/weaver-m1` with `state/`, role and database `weaver_m1`, the two
       auth lines, session `m1-001`. The script renders its own declaration, so the old m1's prompt, 32k
       context and `permission-mode = "deny"` do not carry unless edited in after creation.
15:16  karl's declaration written to the session scratchpad from the archived `karl.yaml`, as TOML, with
       the identity text byte for byte and `[state-store] engine = "none"`; the script refuses that engine by
       design, so karl's accounts and territory are made by hand per the runbook.

15:20  `create-agent.sh m1 ... --engine postgres --apply`: accounts weaver-m1 (952) and weaver-m1-state (951),
       territory, role and database, both auth lines, declaration `m1.toml` (session m1-001), allow-list.
       Both gates verified: the member reaches the database as weaver_m1, the agent's uid is refused.
15:21  karl by hand: account weaver-karl, home 2750, territory `~/.weaveragents/karl` root:todd 2750,
       `karl.toml` installed root 0644, allow-list appended.
15:09Z `verify-load.sh m1`: validated; load answered idle; 3 events landed (load, recall, message.system).
       Load event: run 2026-09-30T20:09:03.281Z-m1-0ec0dc64d96566ec, state_member true, store postgres
       weaver_m1/weaver_m1, declaration a0ef4421361a4a73, composer worker. Unloaded.
15:09Z `verify-load.sh karl`: validated; load answered idle; 2 events landed (load, message.system).
       Load event: run 2026-09-30T20:09:12.844Z-karl-f3c795ca7ff39e77, state_member false, store none,
       declaration f1557abe771a5475 (matches the file written at 15:16), composer worker, the load event's
       `stack` naming weaver-gate 11af5d12 and weaver-spu 7e06b10f, the hashes bootstrap installed. Unloaded.

## 5. The loop, the experiments move, and the overnight run

15:30  `experiments/` moved whole from WeaverAgents to `WeaverTools_Project/WeaverTools/experiments/` on the
       operator's word: its code reads WeaverAnalysis as much as WeaverAgents. 113 files, 1.4M, stdlib
       Python, nothing in it imported from either tree. Left behind in WeaverAgents: 24 references to
       `experiments/` in CLAUDE.md, .hadesignore, Document-Format, Working-Process and four project
       documents, listed for the sweep and not edited here, the process documents being the operator's.
15:31  The matrix harness's 22 self-tests run from the new location: 21 pass; `test_declaration_grammar.py`
       fails reading `crates/weaver-analysis/tests/fixtures/derived-surrogate.toml` under its own repository
       root, which is now the suite repository. The fixture is in WeaverAnalysis. The same coupling
       weaver-types' config test has. Fixed in the test: it reads the fixture from the `WeaverAnalysis`
       sibling of the suite repository, or from `WEAVER_ANALYSIS_DIR`. 22 of 22 pass.
15:32  `deploy/turn.py` written: the smallest gate client, one JSON line in, one out, per the gate-world
       contract sections 2 and 3, run as the operator with no sudo.
15:33  Deposits made on the bulk store: `weaver-testing/determinism-matrix-thinkpad-2026-09-30-eda642e7`
       and `-smoke`, each with `config.json` from run5's with the declaration now `karl.toml`, `repo` the
       WeaverAgents tree, and `build_flags` naming the bootstrap install. Journal: 2G on disk, oldest
       entry 2026-09-28T14:27:40.

15:40  Loop test, first attempt from the planning seat: `verify-load.sh karl --keep` left karl serving, and
       `turn.py` was refused at `/run/weaver-karl/gate.sock`, whose directory is `weaver-karl:weaver-karl`
       0750. The session's group list predates the account and lacks the group; a restarted terminal and a
       restarted Claude session inherit the desktop login's list, so only a fresh login or `newgrp` picks it
       up. `sg` is not on this box. `turn.py` now tries the connect and names this case instead of reporting
       the socket absent.
       **A hazard seen on the way**: the stale session carries the old login's gids 950 and 952, and the
       purge freed those numbers, so the new `weaver-m1-state` and `weaver-m1` groups were issued them.
       A session from before the purge now reads as a member of both without ever joining. Harmless
       here, the state room being 0700 under the member's uid, but a redeploy on olympus should be
       followed by a logout before anything reads group membership as a fact.

## Where it stands, 15:25

The thinkpad runs the stack built from WeaverAgents at the suite split, `nogit-eda642e70b70`, with
karl and m1 declared, validated and proven to load. Neither is loaded now. The old stack is whole in
`/mnt/bulk-store/dev-archive-20260930-thinkpad`.

Owed, for the operator to rule on before olympus:

- `create-agent.sh` has no `--engine none` path, so a storeless agent is the one hand-made step of
  the runbook. Its header refuses that engine on stated grounds; extending it is a change to the
  operator's script.
- `update-stack.sh` still tests `-p weaver-analysis`, which left the workspace, and needs a git tree
  with an origin to name the build. Neither holds in WeaverAgents until it has a repository, so
  the next install on this box is `decommission.sh` plus `bootstrap-stack.sh` again, or the script
  amended once the repository exists.
- The python SPU prefix was archived and not reinstalled; `python-spu/README.md` is its procedure,
  for an agent that elects it.
- m1's declaration is the script's default, not the archived m1's (prompt, 32k context, `deny`).
- acorn and karl2 are retired. Their traces are in the archive.
- The run-log entries above were written by the planning seat from the operator's pasted output;
  the operator typed every privileged command.

## 4. Agents

(pending)
