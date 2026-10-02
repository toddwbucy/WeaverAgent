# Redeploying the agent stack on a box

How a box that ran an earlier stack is taken down, archived, wiped and stood up
again from this tree, in an order a script can follow. Written 2026-09-30 for the
thinkpad and to be repeated on olympus; every box-specific value is a parameter or a
measured fact, and the log of each run sits at `docs/project/redeploy-<date>-<box>.md`.

The four scripts in this directory divide the work. `decommission.sh` takes down and
archives, `bootstrap-stack.sh` builds and installs on a clean box, `create-agent.sh`
makes one agent, `verify-load.sh` proves one agent loads. `update-stack.sh` is for
every later install on a box this runbook has already stood up: it needs the stack
record and at least one agent root to read and a git tree to name the build, and it
refuses without them.

## 0. Before touching anything: measure

Record in the run log, from the box, not from memory:

```sh
hostname; uname -r
nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader
nvcc --version | tail -1
pacman -Q cuda cccl postgresql
rustup show active-toolchain
sudo deploy/decommission.sh            # the plan: what stands, discovered
```

The plan lists every key root under `/etc/weaver`, the install prefixes those roots
name, every agent with its accounts and declaration, the transient
`weaver-worker@<agent>.service` units and the `system-weaver\x2dworker.slice`, the
territories, record and log directories, and the store's roles and databases.
Discovery is by rule, not by one layout: a key root is any directory under
`/etc/weaver` holding a `worker-binary` or an `allow-list` (an agent's root, the stack
record, or the box-wide configuration of the layout before 2026-10-01); the agents are
the agent roots' names, the old allow-lists' names and the old declarations; the
databases and roles are `weaver_<agent>` for those agents and whatever their
declarations name. A `weaver-*` account or database no agent found claims is printed
as left alone and never touched. If it lists something this runbook does not mention,
the runbook is what gets amended.

**What a stack consists of**, so a reader knows what the plan is enumerating:

| Piece | Where | Made by |
|---|---|---|
| Stack record: the box-wide defaults, one file per key, read by the scripts and never by admin | `/etc/weaver/stack/` | bootstrap |
| Admin base, empty until an agent is made | `/etc/weaver/admin/` (root 0755; `WEAVER_ADMIN_CONFIG` names another) | bootstrap |
| Agent root: the agent's admin configuration, one file per key, copied from the stack record, plus its own `log-path` and its declaration `agent.toml` | `/etc/weaver/admin/<name>/` (root 0755, files 0644) | create-agent |
| Members: `worker pyworker weaver-admin weaver-gate weaver-spu weaver-state` | `<prefix>/bin/` | bootstrap, update-stack |
| Engine libraries `libggml*`, `libllama*` | `<prefix>/lib/`, reached by `/etc/ld.so.conf.d/weaver.conf` and by `LD_LIBRARY_PATH` in `unit-properties` | bootstrap |
| Model artifacts, hash-pinned | `<prefix>/models/` | operator, by hand |
| The python SPU prefix and zipapp | `<prefix>/python-spu/` | `python-spu/README.md` |
| Agent account `weaver-<name>` (home `/home/weaver-<name>`, 2750) member account `weaver-<name>-state` (no home), and group `weaver-<name>-trace` | passwd | create-agent |
| Territory `weaver-<name>/` (root:weaver-<name>-state 0710, passage only) with `state/` (member 0700) and `trace.ndjson` (root:weaver-<name>-trace 0640) | the stack record's `agent-directory`, default `/var/lib/weaver-agent` | create-agent |
| Postgres election only: role and database `weaver_<name>`, a `peer map=weaver` line in `pg_hba.conf`, a `weaver` map line in `pg_ident.conf` | PostgreSQL | create-agent |
| The agent's operations log, one per agent | `/var/log/weaver/<name>/admin.log` (directory root 0750) | create-agent makes the directory, admin the file |
| Transient unit per load, under one slice | systemd | admin, at load |

## 1. Decommission and archive

> **`decommission.sh` does not yet understand the per-agent layout.** It was written for
> the box-wide layout before 2026-10-01 (one `/etc/weaver/admin` with an `allow-list`).
> **Do not run it on a box migrated to `/etc/weaver/admin/<agent>/` roots** until
> toddwbucy/WeaverAgent#35 lands; until then take agents down by hand, per
> `HowToDeployANewAgent.md` section 7. On a box still on the box-wide layout it runs as
> described below.

```sh
sudo deploy/decommission.sh --archive              # -> /mnt/bulk-store/dev-archive-<date>-<host>
```

The bulk store may squash root to nobody, as the thinkpad's mount of olympus's export does,
so the script reads as root and writes every byte under the archive directory as the
operator, through `sudo -u`. Pass a directory the operator can create or already owns.

Refuses while any `weaver-worker@` unit is active: unload each with
`sudo WEAVER_ADMIN_CONFIG=<root> <prefix>/bin/weaver-admin unload <name>` first, or
stop the unit if admin will not. The archive holds:

- `box-facts.txt`: accounts, groups, units, sha256 of every installed binary,
  library and model, the store's roles and the two authentication files' weaver
  lines, and the mode of everything archived.
- One `.tar.zst` per piece, owners, ACLs and xattrs preserved: `etc-weaver`,
  `ld-so-conf`, `opt-<prefix>` (bin, lib, python-spu, the backup-* directories;
  never models),
  `var-lib-weaver`, `log-weaver`, `agent-config-<dir>` (declarations and traces),
  `home-weaver-users`, `tmp-weaver`.
- `postgres/<db>.dump` (custom format), `roles.sql`, and copies of `pg_hba.conf` and
  `pg_ident.conf`.
- `PURGE-LIST`: the exact paths, users, groups, databases and roles the purge may
  touch. Read it.
- `SHA256SUMS`, verified before the script says archived, and again before a purge.

Models stay in `<prefix>/models` on purpose: they are artifacts, not binaries or
configuration, and the redeploy loads them again. Check they are also on the bulk
store before relying on that; on 2026-09-30 the three 0.5b ggufs were under
`backups/opt-weaver-models-20260827` and the 8B gguf on the bulk store differed from
the installed one by 256 bytes, so the installed one is the only copy of itself.

## 2. Purge

```sh
sudo deploy/decommission.sh --purge /mnt/bulk-store/dev-archive-<date>-<host>
```

Stops units and the slice, drops the databases then the roles, removes the weaver
lines from `pg_hba.conf` and `pg_ident.conf` (backups beside them) and reloads
PostgreSQL, removes the `weaver-*` users with their homes and their groups, removes
every path on the list, removes a prefix left empty, and prints what remains. After
it, `getent passwd | grep weaver-` and `ls /opt/weaver` should show nothing but
`models`.

Not touched, because they are the operator's and not the stack's: the operator's
membership of `video` and `render` (the agents' memberships go with their accounts),
`/opt/cuda`, PostgreSQL itself, the bulk-store mount.

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
   --features weaver-harness/pyworker,weaver-state/sqlite,weaver-state/postgres`.
   A workspace-wide test does not compile in this tree yet (weaver-types reads a
   fixture from the crate that left, see the suite CLAUDE.md), which is why the
   selection is named.
4. `cargo build --release --locked --workspace --features
   weaver-spu/cuda,weaver-harness/pyworker,weaver-state/sqlite,weaver-state/postgres`.
   Every engine an agent may elect is named, the default one included, or the member
   refuses that agent at load (measured 2026-09-11).
5. Installs the six members to `<prefix>/bin` (root, 0755) and the `libggml*` and
   `libllama*` objects to `<prefix>/lib`, from llama-cpp-sys's `out/lib` under the
   target's `build/` directory (the copies at the target root are bare links that
   dangle), versioned file, SONAME link and bare link together. Writes
   `/etc/ld.so.conf.d/weaver.conf`, runs `ldconfig`, and refuses if `weaver-spu`
   still has an unresolved library. The members carry no RUNPATH.
6. Writes the stack record `/etc/weaver/stack/`: the three binary paths, `run-tool`,
   `control-tool`, `coordination-root=/run`, `unit-properties` carrying `UMask=0000`,
   the `LD_LIBRARY_PATH` and the journal rate limit turned off, and for the scripts
   alone `prefix`, `log-directory` and `agent-directory`. Admin never reads it. An
   operator who wants `headroom-bytes` or `state-store-socket` on every agent writes
   it into the record before making agents. Creates the admin base `/etc/weaver/admin`
   empty (root 0755), `/var/log/weaver` (root 0750) and the agent directory
   (operator 0755).

The python SPU is a separate install and not part of this step. Its procedure is
`python-spu/README.md`, "Installing it on a box", and an agent that serves from it is
made with `create-agent.sh --spu <prefix>/python-spu/python-spu.pyz`, which writes that
path as its root's `spu-binary` in place of the stack record's. Stand the Rust stack
up first and add it only for an agent that needs it.

## 4. Agents

The per-agent procedure in full is `HowToDeployANewAgent.md`; this is the summary.
One at a time, plan first, then `--apply`. The script refuses to merge with anything
half-made, so a name that already has an account, a directory, an agent root, a log
directory, a role or a database must be cleaned up first.

```sh
deploy/create-agent.sh m1 --engine <sqlite|postgres> --artifact /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf
deploy/create-agent.sh m1 --engine <sqlite|postgres> --artifact /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf --apply
```

The store is the one `--engine` names, `sqlite` or `postgres`, and the option is
required since neither is the default. The script makes both
accounts and the trace group, the territory (root:weaver-<name>-state 0710 under the
stack record's `agent-directory`, passage by group with no access entries) and its trace
(root:weaver-<name>-trace 0640, which the member cannot read), for postgres
the role, database and two authentication lines, and the agent root staged under a
dot-name: every key copied from the stack record, `log-path`, and the declaration as
`agent.toml`. It then proves the boundary (for either engine, the member cannot read the
trace; for sqlite, the member can write its state
room and the agent's own uid cannot enter it; for postgres, the member reaches the
database and the agent's own uid does not), and only then moves the root into place,
which is the admission. The operator's three new group memberships need a fresh login
before they apply (`newgrp` selects one group in one shell).

An agent electing no store (`[state-store] engine = "none"`) is made by hand, since
there is no member or store to provision or probe: `HowToDeployANewAgent.md`
section 3. Karl on the thinkpad is this case; its declaration is carried byte for
byte between boxes, system prompt included, because the determinism runs compare
against it.

Then, for each:

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin validate <name>
sudo deploy/verify-load.sh <name>              # load, read the load event from the sink, unload
```

`verify-load.sh` reads the agent's root, counts the sink's lines before and after the
load and refuses a load that wrote nothing, then prints the event kinds it saw and the
load event, and unloads unless told `--keep`. A failed load needs no clearing by hand:
admin clears the failed unit itself (`systemctl reset-failed`), so the next load is
not refused as `prior_unit_unreaped`. It does not retry the load.

## 5. What to record in the run log

The box facts of step 0. The archive path and its `SHA256SUMS` line count. What
`--purge` printed under "what remains". The revision `bootstrap-stack.sh` named and its
test count. Per agent: the declaration's sha256, the artifact's sha256, the load
event's `composer` and device, and how long the load took. Anything the scripts
refused and what was done about it, since a refusal is the thing the next box will
hit first.

## 6. Olympus, where it differs

- Three cards. A declaration's `devices = [0]` binds whichever card CUDA numbers 0;
  record `serving_device` from the first load and pin with `CUDA_VISIBLE_DEVICES` on
  the unit if it is the wrong one. The two-card tests only olympus can run are the SPU
  gate's business, not this runbook's.
- `CARGO_TARGET_DIR` is set there and shared. Export a fresh one for the install so
  the members are built from this tree and not compared against another project's
  output.
- The bulk store is local (`/bulk-store`, exported to the thinkpad as
  `/mnt/bulk-store`). Name the archive path on the olympus side.
- `~/.cargo/config.toml` may carry a `CUDARC_CUDA_VERSION` override; remove it
  (handoff of 2026-09-29, step 2).
- Its recorded runs used the `e69916a` stack and its CUDA user-space libraries. The
  purge removes `<prefix>/lib` and the `backup-*` directories, so if olympus still
  needs that library set for the probe's B1 re-staging, the archive is where it will
  be, and `box-facts.txt` names each library's sha256.

## 7. Migrating a box on the box-wide layout

Until 2026-10-01 admin read one configuration for every agent: `/etc/weaver/admin`
held the keys for all of them, an `allow-list` admitted them, `agent-config-directory`
held their declarations as `<name>.toml`, and `spu-implementations` with `agent-spu`
chose an SPU per agent. Admin now reads only `<base>/<agent>/`, and `update-stack.sh`
refuses a box whose base still holds any of those four files. The migration is a
one-time manual step, made with every agent unloaded, before the new admin is
installed. `<prefix>` is the install prefix, `/opt/weaver` by default.

0. A box whose agent directory is still the plural `~/.weaveragents` (the default before
   2026-10-02) renames it, and every configuration key and top-level declaration that
   names the old path, never the territories' traces beneath it:

   ```sh
   mv ~/.weaveragents ~/.weaveragent
   sudo grep -l '\.weaveragents' /etc/weaver/admin/* ~/.weaveragent/*.toml ~/.weaveragent/*.yaml 2>/dev/null \
     | xargs -r sudo sed -i 's#\.weaveragents#.weaveragent#g'
   ```

   Check that `agent-config-directory` and each declaration's paths now name `~/.weaveragent`.

1. Keep the old configuration whole, and read the values below from the copy:

   ```sh
   B=/etc/weaver/admin
   sudo cp -a "$B" /etc/weaver/admin.before-migration
   OLD=/etc/weaver/admin.before-migration
   DECLS=$(cat "$OLD/agent-config-directory")
   ```

2. For each agent in `$OLD/allow-list`, make its root with every box-wide key except
   the four retired ones, its own `spu-binary`, its own log, and its declaration:

   ```sh
   for a in $(cat "$OLD/allow-list"); do
     sudo install -d -o root -g root -m 0755 "$B/$a"
     for k in worker-binary spu-binary gate-binary run-tool control-tool \
              coordination-root unit-properties headroom-bytes state-store-socket; do
       [ ! -f "$OLD/$k" ] || sudo cp "$OLD/$k" "$B/$a/$k"
     done
     # The agent's agent-spu choice, if it had one, resolved through spu-implementations.
     key=$(awk -v a="$a" '$1 == a {print $2}' "$OLD/agent-spu" 2>/dev/null)
     if [ -n "$key" ]; then
       awk -v k="$key" '$1 == k {print $2}' "$OLD/spu-implementations" | sudo tee "$B/$a/spu-binary" >/dev/null
     fi
     sudo install -d -o root -g root -m 0750 "/var/log/weaver/$a"
     echo "/var/log/weaver/$a/admin.log" | sudo tee "$B/$a/log-path" >/dev/null
     sudo mv "$DECLS/$a.toml" "$B/$a/agent.toml"
     sudo chown root:root "$B"/"$a"/*
     sudo chmod 0644 "$B"/"$a"/*
   done
   ```

   Check each `spu-binary` names the path the agent served from before.

3. Remove the old top-level files, so that the base holds only agent roots:

   ```sh
   sudo find "$B" -maxdepth 1 -type f -delete
   ```

4. Write the stack record from the old values, for `create-agent.sh` and
   `update-stack.sh`:

   ```sh
   S=/etc/weaver/stack
   sudo install -d -o root -g root -m 0755 "$S"
   for k in worker-binary spu-binary gate-binary run-tool control-tool \
            coordination-root unit-properties headroom-bytes state-store-socket; do
     [ ! -f "$OLD/$k" ] || sudo cp "$OLD/$k" "$S/$k"
   done
   echo <prefix>          | sudo tee "$S/prefix" >/dev/null
   echo /var/log/weaver   | sudo tee "$S/log-directory" >/dev/null
   sudo install -d -o root -g root -m 0755 /var/lib/weaver-agent
   echo /var/lib/weaver-agent | sudo tee "$S/agent-directory" >/dev/null
   sudo chmod 0644 "$S"/*
   ```

   `agent-directory` is where `create-agent.sh` makes new territories, and it must stand
   root-owned and writable by no group or other, so it is a new root-held base and never
   the old operator-owned declaration directory. Existing territories stay where they
   are, their sink paths unchanged in their declarations; lay each out per step 4a.

4a. **An existing territory with a state member** (made before 2026-10-02) may be
   grouped to its member and setgid, which lets the member read the trace, its ingress
   being the tee's distillate and nothing more. Re-lay it as `create-agent.sh` now makes
   one, with `<t>` the territory and `<name>` the agent, the agent unloaded:

   ```sh
   T=<t>; N=<name>
   sudo groupadd --system "weaver-$N-trace"
   sudo usermod -aG "weaver-$N-state,weaver-$N-trace" "$USER"   # passage, and the trace
   sudo chgrp "weaver-$N-state" "$T"
   sudo chmod g-s,u=rwx,g=x,o= "$T"                  # 0710, setgid cleared
   sudo find "$T" -maxdepth 1 -type f -exec chgrp "weaver-$N-trace" {} + \
     -exec chmod g-w,o= {} +                          # the trace and any copy beside it
   sudo -u "weaver-$N-state" test -r "$T/trace.ndjson" && echo "LEAK: the member reads the trace"
   ```

   A storeless agent has no member and needs none of this.

5. Install the new admin (`deploy/update-stack.sh --install`), which validates and
   loads every agent root before it reports the box current. The old box-wide log
   `/var/log/weaver/admin-operations.ndjson` stays where it is as the record of what
   came before; each agent's acts from here on are in its own `admin.log`. When the
   box has been verified, `/etc/weaver/admin.before-migration` can be archived and
   removed.
