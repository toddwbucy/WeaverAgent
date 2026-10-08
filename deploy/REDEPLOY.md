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
git tree to name the build, and it refuses without them.

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
| Territory `weaver-<name>/` (root:weaver-<name>-admin 0711, passage only) with `agent.toml` and `system-prompt.md` (root 0644), `admin.log` and `worker.log` once admin has run a verb (root:weaver-<name>-admin 0640), `save-points/` (root:weaver-<name>-admin 0750, the published save points 0640 and the manifest root 0644), `state/` (member 0700) and `trace.ndjson` (root:weaver-<name>-trace 0640); the whole agent, on the operator's ruling of 2026-10-07 on #1 | the stack record's `agent-directory`,  default `/var/lib/weaver-agent` | create-agent, admin for the logs and the save points |
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
   which each territory is root:weaver-<name>-admin 0711, not setgid.

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
groups, the territory (root:weaver-<name>-admin 0711 under the stack record's
`agent-directory`, passage for every uid and listing for none, with no access entries), its trace
(root:weaver-<name>-trace 0640, which the member cannot read), the declaration and the
prompt draft in the territory (root 0644; `sudoedit` edits the declaration) and
`save-points/` beside them, and the agent root staged under a dot-name: every key
copied from the stack record, `territory`, `operator` and `roles.toml`. It then proves
the boundary (the member cannot read the trace, the relay holds the trace group alone
and the connector the access group and nothing of the agent's, the member can write its
state room and the agent's own uid cannot enter it, and the connector reads the
declaration and writes nothing of the territory), installs the sudo rule once `visudo`
passes it, and only then moves the root into place, which is the admission. The
operator's three new group memberships, the agent's, the trace group and the access
group, need a fresh login before they apply (`newgrp` selects one group in one shell).

An agent electing no store (`[state-store] engine = "none"`) is made by hand, since
there is no member or store to provision or probe: `HowToDeployANewAgent.md` section 3.
Karl on the thinkpad is this case, and its declaration is carried byte for byte between
boxes, with its prompt draft beside it and seeded through the gate on the new box
(`HowToDeployANewAgent.md` section 4), because the determinism runs compare against
both.

Then, for each:

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin validate <name>
sudo deploy/verify-load.sh <name>              # load, read the load event, check the constituents, unload
```

`verify-load.sh` reads the agent's root, counts the sink's lines before and after the
load and refuses a load that wrote nothing, prints the event kinds it saw and the load
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

## 7. Migrating a box on the box-wide layout

Until 2026-10-01 admin read one configuration for every agent: `/etc/weaver/admin`
held the keys for all of them, an `allow-list` admitted them, `agent-config-directory`
held their declarations as `<name>.toml`, and `spu-implementations` with `agent-spu`
chose an SPU per agent. Admin now reads only `<base>/<agent>/` and the territory it
names, and `update-stack.sh` refuses a box whose base still holds any of
those four files. Such a box is best decommissioned and stood up again by sections 1 to
4, since it also lacks every account and file #50 added. Where its agents must be kept,
migrate it to the per-agent layout of section 8 by hand, with every agent unloaded and
before the new admin is installed:

1. Keep the old configuration whole: `sudo cp -a /etc/weaver/admin
   /etc/weaver/admin.before-migration`.
2. For each agent in the old `allow-list`, make its root under `/etc/weaver/admin/<a>/`
   with the binary keys and `coordination-root` copied from the old base, its own
   `spu-binary` (its `agent-spu` choice resolved through `spu-implementations`, where it
   had one), and install its declaration `<agent-config-directory>/<a>.toml` as root
   0644 at `<territory>/agent.toml` (section 8, step 7, for the territory's layout).
   Then finish it as section 8, steps 2 to 4, describe, step 2's move already made.
3. Remove the old top-level files, so that the base holds only agent roots: `sudo find
   /etc/weaver/admin -maxdepth 1 -type f -delete`.
4. Write the stack record as section 3, step 6, describes, then install with
   `deploy/update-stack.sh --install`.

## 8. Migrating a box on the per-agent layout from before #50

A box stood up between 2026-10-01 and #50 (the thinkpad's of 2026-09-30) has a root per
agent, but each root holds `agent.toml`, `run-tool`, `control-tool`, `unit-properties`
and `log-path`, its agents run as `weaver-worker@<agent>.service` units, and it has no
relay account, access group, connector account, boundary file or sudo rule.
`update-stack.sh` refuses such a box by name, and the migration is a one-time manual
step, per agent, before the new admin is installed. `<a>` is the agent, `<prefix>` the
install prefix, `/opt/weaver` by default.

1. **Unload every agent with the admin that loaded it**, while it is still installed:
   `sudo <prefix>/bin/weaver-admin unload <a>`, and check `systemctl list-units
   'weaver-worker@*'` shows none active. The new admin ends a run by its run lock, which
   a unit's worker never took, so it could not reach a worker left serving.

2. **Move the declaration into the territory** and retire the root's old keys (the
   territory's full layout is step 7, which `update-stack.sh --install` lays out for
   a box that took this step before 2026-10-07):

   ```sh
   A=<a>; R=/etc/weaver/admin/$A; T=/var/lib/weaver-agent/weaver-$A
   sudo install -o root -g root -m 0644 "$R/agent.toml" "$T/agent.toml"
   sudo cp -a "$R" "/etc/weaver/admin.before-50-$A"      # outside the base, a dot-free name admin never reads
   sudo rm -f "$R/agent.toml" "$R/run-tool" "$R/control-tool" "$R/unit-properties" "$R/log-path"
   echo "$T" | sudo tee "$R/territory" >/dev/null
   id -u | sudo tee "$R/operator" >/dev/null
   echo "trace-reader = \"weaver-$A-admincon\"" | sudo tee "$R/roles.toml" >/dev/null
   [ ! -f /etc/weaver/stack/library-path ] || sudo cp /etc/weaver/stack/library-path "$R/library-path"
   sudo chmod 0644 "$R"/*
   ```

   The copy goes beside the base, never inside it, where its name would be read as an
   agent's.

   **An agent electing `postgres` moves to the embedded engine here**, since postgres
   is not an engine this build provides (#85) and `update-stack.sh` refuses such a
   declaration before its build. In `$T/agent.toml` (with `sudoedit`), set
   `[state-store]` to `engine = "sqlite"` and delete its `database` and `role` lines.
   The new member
   starts with no save point, as an agent's first load does, and the old PostgreSQL
   database and role are the operator's to drop by hand (section 2).

   **A declaration carrying `[[spu-instruction.decoder.identity]]` loses it here**,
   since the system prompt is state (the operator's ruling of 2026-10-06) and the admin
   installed from that date refuses the table by name. `update-stack.sh --install`
   moves it for you, before the binaries: one system text message becomes
   `$T/system-prompt.md`, the draft, and the table is removed, the declaration backed
   up beside itself. A shape it cannot move losslessly (several messages, several
   blocks, a draft already standing with other text) refuses at plan time, and the
   move is then by hand: `sudo deploy/migrate-identity.py $T/agent.toml` says why, and
   `HowToDeployANewAgent.md` section 3 shows the result. Either way the prompt is not
   in the agent until it is seeded after the first load under the new stack,
   `deploy/turn.py <a> --system` (`HowToDeployANewAgent.md` section 4), and the save
   point taken after that seeding is the agent's starting state.

3. **Provision the relay, the access group and the connector**, as `create-agent.sh`
   makes them:

   ```sh
   getent group "weaver-$A-trace" >/dev/null || sudo groupadd --system "weaver-$A-trace"
   sudo groupadd --system "weaver-$A-admin"
   sudo useradd --system --shell /usr/sbin/nologin --no-create-home --no-user-group --gid "weaver-$A-trace" "weaver-$A-relay"
   sudo useradd --system --shell /usr/sbin/nologin --no-create-home --user-group --groups "weaver-$A-admin" "weaver-$A-admincon"
   ```

   A territory from before 2026-10-02 that is still grouped to its member and setgid is
   re-laid first, with its trace root:weaver-<a>-trace 0640, as `create-agent.sh` makes
   one (`HowToDeployANewAgent.md` section 0); a territory from before 2026-10-07 takes
   the access group at step 7. **Every trace must stand root's, grouped
   `weaver-<a>-trace`, 0640**, which admin checks at every load since #62. An admin
   before #62 recreated a lost trace root:root, so re-lay any such regular file with
   `sudo chown root:weaver-$A-trace <trace> && sudo chmod 0640 <trace>`, and remove a
   link or any other entry at the trace's path and provision the trace afresh, since
   chown and chmod follow a link. `update-stack.sh` refuses before its build while one
   stands otherwise, naming what it found and what is required and pointing here, and
   prints no command.

4. **Install the connector's sudo rule**, checked before it is placed:

   ```sh
   ADMIN=<prefix>/bin/weaver-admin
   cat > /tmp/weaver-$A.rule <<EOF
   Defaults:weaver-$A-admincon !pam_session
   weaver-$A-admincon ALL=(root) NOPASSWD: $ADMIN show $A, $ADMIN validate $A, $ADMIN load $A, $ADMIN unload $A, $ADMIN stop $A
   EOF
   sudo visudo -cf /tmp/weaver-$A.rule && sudo install -o root -g root -m 0440 /tmp/weaver-$A.rule /etc/sudoers.d/weaver-$A
   rm /tmp/weaver-$A.rule
   ```

   The observer role's rule grants the `show` line alone.

5. **Bring the stack record forward**, once for the box: remove `run-tool`,
   `control-tool`, `unit-properties` and `log-directory` from `/etc/weaver/stack/`, and
   write `library-path` with the install's library directory (`<prefix>/lib`), root
   0644.

6. **Pull main first** (`git pull --ff-only` on `main`), so the `update-stack.sh` that runs
   carries the checks of the stack it installs, the trace preflight of step 3 among
   them. A script from before that re-executes itself after its fast-forward would run
   its old checks. Then **install** with `deploy/update-stack.sh --install`, which now installs
   `weaver-trace-relay`, then validates, loads and reads back every agent before it
   reports the box current. Run `sudo deploy/verify-load.sh <a>` for each agent to
   check its constituents. The old per-agent logs `/var/log/weaver/<a>/admin.log` stay
   where they are as the record of what came before, and each agent's acts from here on
   are in `<territory>/admin.log`. When the box has been verified, the
   `/etc/weaver/admin.before-50-*` copies can be archived and removed.

7. **Move the agent into its territory**, for a box that took steps 2 to 6 before
   2026-10-07, when the operator ruled on #1 that the whole agent lives in its
   territory and the operator's `~/.weaveragent/<a>/` directory is retired.
   `update-stack.sh --install` makes this move for every agent whose root still names
   a `declaration-directory`, before the binaries and registered for rollback: it
   moves `agent.toml` and `system-prompt.md` into the territory as root 0644, and
   `admin.log` and `worker.log` as root:weaver-<a>-admin 0640; makes `save-points/`
   (root:weaver-<a>-admin 0750); joins the operator to the access group
   `weaver-<a>-admin`, the member never; regroups the territory to the access group
   at 0711; writes the root's `territory` key and removes `declaration-directory`. The
   plan names the move and does nothing. It refuses an agent whose territory already
   holds an `agent.toml`, since two declarations of one agent is not a state it can
   choose between: remove the one that is wrong first. After the install the
   operator's old directory stands empty but for the identity backups, yours to
   remove, and every later edit of the declaration is `sudoedit
   /var/lib/weaver-agent/weaver-<a>/agent.toml`. Published save points a box made before
   this step stand in the old directory under the operator's uid; the manifest there
   is root's. They are not moved, being an unclean mix of owners: name one at a
   `restore` only after installing it as root:weaver-<a>-admin 0640 into
   `<territory>/save-points/`, or start the agent from its first load under the new
   layout and let the next unload publish the first save point there. The operator's
   new group membership needs a fresh login before it applies.
