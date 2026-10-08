# Redeploying the agent stack on a box

How a box that ran an earlier stack is taken down, archived, wiped and stood up again
from this tree, in an order a script can follow. Written 2026-09-30 for the thinkpad,
and rewritten for the start step of #50, which took the agent out of systemd on the
operator's ruling of 2026-10-03. Every box-specific value is a parameter or a measured
fact, and the log of each run sits at `docs/project/redeploy-<date>-<box>.md`.

The scripts in this directory divide the work. `decommission.sh` takes down and
archives, `bootstrap-stack.sh` builds and installs on a clean box, `create-agent.sh`
makes one agent, `verify-load.sh` proves one agent loads and its constituents sit where
they should. `update-stack.sh` is for every later install on a box this runbook has
already stood up: it needs the stack record and at least one agent root to read and a
git tree to name the build, and it refuses without them. A box on the box-wide layout
of before 2026-10-01, which it refuses by name, is taken down and stood up again by
sections 1 to 4; an agent of an older form is recreated after its take-down by
`HowToDeployANewAgent.md` section 7.

## 0. Before touching anything: measure

Record in the run log, from the box, not from memory:

```sh
hostname; uname -r
nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader
nvcc --version | tail -1
pacman -Q cuda cccl
rustup show active-toolchain
sudo deploy/decommission.sh            # the plan: what stands, discovered
```

The plan lists every configuration base under `/etc/weaver`, each agent root under a
base with its territory, the sudo rules `/etc/sudoers.d/weaver-*`, the
install prefixes the roots name, every agent with its accounts, whether each agent runs
(asked of the installed admin with `show`), any `weaver-worker@<agent>.service` unit a
box from before #50 still carries, and the territories, record and log directories.
Discovery is by rule, not by one layout. The agents are the roots' names, the names of
every `weaver-*` account and group with its reserved suffix stripped, and the
allow-lists and declarations of the box-wide layout before 2026-10-01. If it lists
something this runbook does not mention, the runbook is what gets amended.

**What a stack consists of**, so a reader knows what the plan is enumerating:

| Piece | Where | Made by |
|---|---|---|
| Stack record: the box-wide defaults, one file per key, read by the scripts and never by admin | `/etc/weaver/stack/` | bootstrap |
| Admin base, empty until an agent is made | `/etc/weaver/admin/` (root 0755, and a delegated invocation under sudo reads only this one) | bootstrap |
| Agent root: the agent's admin configuration, one file per key, copied from the stack record, plus `territory`, `operator` and `roles.toml` | `/etc/weaver/admin/<name>/` (root 0755, files 0644) | create-agent |
| Sudo rule: the connector's fixed command lines | `/etc/sudoers.d/weaver-<name>` (root 0440) | create-agent |
| Members: `worker pyworker weaver-admin weaver-trace-relay weaver-gate weaver-spu weaver-state` | `<prefix>/bin/` | bootstrap, update-stack |
| Engine libraries `libggml*`, `libllama*` | `<prefix>/lib/`, reached by `/etc/ld.so.conf.d/weaver.conf` and named by the record's `library-path`, which admin sets as the worker's `LD_LIBRARY_PATH` | bootstrap |
| Model artifacts, hash-pinned | `<prefix>/models/` | operator, by hand |
| The python SPU prefix and zipapp | `<prefix>/python-spu/` | `python-spu/README.md` |
| Accounts: the agent's `weaver-<name>` (home `/home/weaver-<name>`, 2750), the member's `weaver-<name>-state`, the relay's `weaver-<name>-relay`, the connector's `weaver-<name>-admincon`, and the groups `weaver-<name>-trace` and `weaver-<name>-admin` | passwd | create-agent |
| Territory `weaver-<name>/` (root:weaver-<name>-state 0710, passage for the state group alone) with `agent.toml` and `system-prompt.md` (root:weaver-<name>-admin 0640), `admin.log` and `worker.log` once admin has run a verb (root:weaver-<name>-admin 0640), `save-points/` (root:weaver-<name>-admin 0750, the published save points 0640 and the manifest root 0644), `state/` (member 0700) and `trace.ndjson` (root:weaver-<name>-trace 0640); the whole agent, on the operator's ruling of 2026-10-07 on #1 | the stack record's `agent-directory`,  default `/var/lib/weaver-agent` | create-agent, admin for the logs and the save points |
| Run directory: `run.lock`, `admin.lock` and the trace door `trace.sock` | `<coordination-root>/weaver.run/<name>/` (root 0755, on tmpfs under `/run`) | admin, at its first verb |

No unit and no init system is part of a stack. Admin's start step stands the worker, its
state member and the trace relay as processes holding one run lock, in the containment
the load was invoked from.

## 1. Decommission and archive

**Decommission's precondition**, stated once here and in `decommission.sh`'s header.
Decommission is a deliberate, destructive act on the whole box, run by the operator at a
root shell:

1. It first disables every agent's sudo rule, so no connector can start a load.
2. It then checks again for any live run, by each agent root's `show` and by any process
   under a `weaver-*` account, and refuses if it finds one.
3. A load the operator starts from a root shell while decommission runs is outside what
   it defends. The operator quiesces the box first.

```sh
sudo deploy/decommission.sh --archive              # -> /mnt/bulk-store/dev-archive-<date>-<host>
```

The bulk store may squash root to nobody, as the thinkpad's mount of olympus's export does,
so the script reads as root and writes every byte under the archive directory as the
operator, through `sudo -u`. Pass a directory the operator can create or already owns.

Refuses while any agent runs: unload each with `sudo <prefix>/bin/weaver-admin unload
<name>` first. An agent `show` cannot answer for refuses too, never read as stopped. On
a box from before #50 it also refuses while any `weaver-worker@` unit is active: unload
those with the admin that started them. **The snapshot is taken behind a shut door**:
each sudo rule is first moved to `.weaver-<agent>.decommissioning`, a name sudo never
reads, and every agent is asked again, a run found restoring the rules and refusing. The
rules stay disabled until the purge removes them, and moving one back serves its agent
again without a purge. The archive holds:

- `box-facts.txt`: accounts, groups, units, sha256 of every installed binary,
  library and model, and the mode of everything archived. It records no PostgreSQL
  facts: an old box's `weaver_*` roles, databases and authentication lines are the
  operator's to inspect and drop by hand (section 2, #35).
- One `.tar.zst` per piece, owners, ACLs and xattrs preserved: `etc-weaver`,
  `sudoers-weaver`, `ld-so-conf`, `opt-<prefix path>` (bin,
  lib, python-spu, the backup-* directories, never models), `territories-<base path>`
  for each territory base, `var-lib-weaver`, `log-<path>` (a box from before #50),
  `agent-config-<path>` (the box-wide layout's declarations), `home-weaver-users`,
  `tmp-weaver`. A name built from a path carries the whole path, its slashes made dashes
  (`territories-var-lib-weaver-agent`), and a name an archive already holds takes the
  first free `-2`, `-3`, so no archive is ever written over another.
- `PURGE-LIST`: the exact paths, users and groups the purge may touch. Read it.
- `SHA256SUMS`, verified before the script says archived, and again before a purge.

Models stay in `<prefix>/models` on purpose: they are artifacts, not binaries or
configuration, and the redeploy loads them again. Check they are also on the bulk
store before relying on that. On 2026-09-30 the three 0.5b ggufs were under
`backups/opt-weaver-models-20260827` and the 8B gguf on the bulk store differed from
the installed one by 256 bytes, so the installed one is the only copy of itself.

The territories' archive holds each agent whole: its declaration, prompt draft, two
logs and published save points beside its state room and trace. A box from before
2026-10-07 keeps its `~/.weaveragent/` directories where they stand, the operator's
own, neither archived nor purged.

## 2. Purge

```sh
sudo deploy/decommission.sh --purge /mnt/bulk-store/dev-archive-<date>-<host>
```

Removes the sudo rules first, so no connector can start a run, then asks every agent
again and refuses if any runs. Stops any unit of a box from before #50, removes the
`weaver-*` users with their homes and their groups, removes every path on the list (the
sudo rules and the run directories among them), removes a prefix left empty, and prints
what remains. After it, `getent passwd | grep weaver-` and `ls /opt/weaver` should show
nothing but `models`.

Not touched, because they are the operator's and not the stack's: the operator's home,
the operator's membership of `video` and `render` (the agents' memberships
go with their accounts), `/opt/cuda`, the bulk-store mount, and any PostgreSQL a box
from before the service engine's retirement still runs: its `weaver_*` roles and
databases and its authentication lines are the operator's to dump and drop by hand,
since decommission no longer discovers them (it matched every `weaver%` database,
#35).

## 3. Build and install

From the WeaverAgent tree, on the pinned toolchain. Give the build its own target
directory if the box shares one with a gate.

```sh
deploy/bootstrap-stack.sh              # preflight, tests, release build, plan
deploy/bootstrap-stack.sh --install    # then install under sudo
```

What it does, so the log can say which step a failure was at:

1. Prints the box facts and refuses if `cccl` is outside the 3.1.4-3.3.4 window, if
   there is no `nvcc`, if the stack record `/etc/weaver/stack` or `<prefix>/bin`
   already stands, or if `/etc/weaver/admin` holds anything.
2. Names the tree: the git revision, or `nogit-<lockfile sha>` on a tree without git.
3. `cargo test --release --locked -p weaver-trace -p weaver-harness -p weaver-state
   --features weaver-harness/pyworker,weaver-state/sqlite`. The selection is the
   crates the stack runs, with the store engine's feature on, so the engine an agent
   elects is tested before it is installed. A
   workspace-wide `cargo test --locked` also compiles and passes (#37).
4. `cargo build --release --locked --workspace --features
   weaver-spu/cuda,weaver-harness/pyworker,weaver-state/sqlite`. The engine is named,
   though it is the default, or the member refuses an agent electing it at load
   (measured 2026-09-11).
5. Installs the seven members to `<prefix>/bin` (root, 0755), `weaver-trace-relay`
   beside the worker where admin finds it, and the `libggml*` and `libllama*` objects
   to `<prefix>/lib`, from llama-cpp-sys's `out/lib` under the target's `build/`
   directory (the copies at the target root are bare links that dangle), versioned
   file, SONAME link and bare link together. Writes `/etc/ld.so.conf.d/weaver.conf`,
   runs `ldconfig`, and refuses if `weaver-spu` still has an unresolved library. The
   members carry no RUNPATH.
6. Writes the stack record `/etc/weaver/stack/`: the three binary paths,
   `coordination-root=/run`, `library-path=<prefix>/lib`, and for the scripts alone
   `prefix` and `agent-directory`. Admin never reads it. An operator who wants
   `headroom-bytes` or `load-bound-seconds` on every agent writes
   it into the record before making agents. Creates the admin base `/etc/weaver/admin`
   empty (root 0755) and the agent directory `/var/lib/weaver-agent` (root 0755), under
   which each territory is root:weaver-<name>-state 0710, not setgid.

The python SPU is a separate install and not part of this step. Its procedure is
`python-spu/README.md`, "Installing it on a box", and an agent that serves from it is
made with `create-agent.sh --spu <prefix>/python-spu/python-spu.pyz`, which writes that
path as its root's `spu-binary` in place of the stack record's. Stand the Rust stack
up first and add it only for an agent that needs it.

## 4. Agents

The per-agent procedure in full is `HowToDeployANewAgent.md`, and this is the summary.
One at a time, as the operator, plan first, then `--apply`. The script refuses to merge
with anything half-made, so a name that already has an account, a group, a directory,
an agent root, a sudo rule or a declaration must be cleaned up first.

```sh
deploy/create-agent.sh m1 --artifact /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf
deploy/create-agent.sh m1 --artifact /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf --apply
```

The store is the embedded sqlite engine, the default (the service engine retired on the
operator's ruling of 2026-10-02 on #1). The script makes the four accounts and the two
groups, the territory (root:weaver-<name>-state 0710 under the stack record's
`agent-directory`, made canonical, with no access entries), its trace
(root:weaver-<name>-trace 0640, which the member cannot read), the declaration and the
prompt draft in the territory (root:weaver-<name>-admin 0640; `sudoedit` edits the declaration) and
`save-points/` beside them (root:weaver-<name>-admin 0750), and the agent root staged
under a dot-name: every key copied from the stack record, `territory`, `operator` and
`roles.toml`. **The territory is the state group's and passes it; the access group reads
the files** (the operator's ruling of 2026-10-08 on #1). The member holds the state group
alone, the connector holds the state group for passage and the access group for reading,
and the agent's own uid holds neither. It then proves the boundary (the member cannot
read the trace, the relay holds the trace group alone and the connector the state and
access groups and no group of the agent's or the trace's, the member can write its
state room and the agent's own uid can pass neither it nor the territory, and the
connector reads the declaration and writes nothing of the territory), installs the sudo
rule once `visudo` passes it, and only then moves the root into place, which is the
admission. The operator joins four groups: `weaver-<name>` for the gate's socket,
`weaver-<name>-trace` for the trace, `weaver-<name>-state` to pass the territory and
`weaver-<name>-admin` to read the declaration, the draft, the logs and the save points.
They need a fresh login before they apply (`newgrp` selects one group in one shell).

An agent electing no store (`[state-store] engine = "none"`) is made by hand, since
there is no member or store to provision or probe: `HowToDeployANewAgent.md` section 3.
Karl on the thinkpad is this case, and its declaration is carried byte for byte between
boxes, with its prompt draft beside it and seeded through the gate on the new box
(`HowToDeployANewAgent.md` section 4), because the determinism runs compare against
both.

Then, for each:

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin validate <name>
sudo deploy/verify-load.sh <name>              # unload, load, read the load event, check the constituents, unload
```

`verify-load.sh` reads the agent's root, first unloads the agent (taking its leave save
point where a run stands), counts the sink's lines before and after the load and refuses a load that wrote nothing, prints the event kinds it saw and the load
event, checks every constituent `show` names (its account, its cgroup the invoker's, a
relay for a file sink), and unloads unless told `--keep`, checking that nothing of the
run is left. A failed load needs no clearing by hand: whatever it started dies with its
worker or is ended by the next `unload`. It does not retry the load.

## 5. What to record in the run log

The box facts of step 0. The archive path and its `SHA256SUMS` line count. What
`--purge` printed under "what remains". The revision `bootstrap-stack.sh` named and its
test count. Per agent: the declaration's sha256, the artifact's sha256, the load
event's `composer` and device, the constituents and their cgroup, and how long the load
took. Anything the scripts refused and what was done about it, since a refusal is the
thing the next box will hit first.

## 6. Olympus, where it differs

- Three cards. A declaration's `devices = [0]` binds whichever card CUDA numbers 0, so
  record `serving_device` from the first load and change the declaration's device if it
  is the wrong one. The two-card tests only olympus can run are the SPU gate's
  business, not this runbook's.
- `CARGO_TARGET_DIR` is set there and shared. Export a fresh one for the install so
  the members are built from this tree and not compared against another project's
  output.
- The bulk store is local (`/bulk-store`, exported to the thinkpad as
  `/mnt/bulk-store`). Name the archive path on the olympus side.
- `~/.cargo/config.toml` may carry a `CUDARC_CUDA_VERSION` override. Remove it
  (handoff of 2026-09-29, step 2).
- Its recorded runs used the `e69916a` stack and its CUDA user-space libraries. The
  purge removes `<prefix>/lib` and the `backup-*` directories, so if olympus still
  needs that library set for the probe's B1 re-staging, the archive is where it will
  be, and `box-facts.txt` names each library's sha256.
