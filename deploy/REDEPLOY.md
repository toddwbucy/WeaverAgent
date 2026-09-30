# Redeploying the agent stack on a box

How a box that ran an earlier stack is taken down, archived, wiped and stood up
again from this tree, in an order a script can follow. Written 2026-09-30 for the
thinkpad and to be repeated on olympus; every box-specific value is a parameter or a
measured fact, and the log of each run sits at `docs/project/redeploy-<date>-<box>.md`.

The four scripts in this directory divide the work. `decommission.sh` takes down and
archives, `bootstrap-stack.sh` builds and installs on a clean box, `create-agent.sh`
makes one agent, `verify-load.sh` proves one agent loads. `update-stack.sh` is for
every later install on a box this runbook has already stood up: it needs the
installed config to read and a git tree to name the build, and it refuses without
both.

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

The plan lists every config root under `/etc/weaver`, the install prefixes those
configs name, every agent by allow-list, declaration and account, the transient
`weaver-worker@<agent>.service` units and the `system-weaver\x2dworker.slice`, the
territories, record and log directories, and the store's `weaver%` roles and
databases. If it lists something this runbook does not mention, the runbook is what
gets amended.

**What a stack consists of**, so a reader knows what the plan is enumerating:

| Piece | Where | Made by |
|---|---|---|
| Admin's configuration: one file per key | `/etc/weaver/admin/` (a second root per parallel stack, e.g. `admin-stageb`) | bootstrap |
| Members: `worker pyworker weaver-admin weaver-gate weaver-spu weaver-state` | `<prefix>/bin/` | bootstrap, update-stack |
| Engine libraries `libggml*`, `libllama*` | `<prefix>/lib/`, reached by `/etc/ld.so.conf.d/weaver.conf` and by `LD_LIBRARY_PATH` in `unit-properties` | bootstrap |
| Model artifacts, hash-pinned | `<prefix>/models/` | operator, by hand |
| The python SPU prefix and zipapp | `<prefix>/python-spu/` | `python-spu/README.md` |
| Agent account `weaver-<name>` (home `/home/weaver-<name>`, 2750) and member account `weaver-<name>-state` (no home) | passwd | create-agent |
| Territory `<agent-dir>/<name>/` (root:operator 2750) with `state/` (member 0700) and `trace.ndjson` | `agent-config-directory`, default `~operator/.weaveragents` | create-agent |
| Declaration `<name>.toml` | same directory | create-agent (or by hand for `engine = "none"`) |
| Store role and database `weaver_<name>`, a `peer map=weaver` line in `pg_hba.conf`, a `weaver` map line in `pg_ident.conf` | PostgreSQL | create-agent |
| Admin's log | `/var/log/weaver/admin-operations.ndjson` | admin, at first verb |
| Transient unit per load, under one slice | systemd | admin, at load |

## 1. Decommission and archive

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

From the WeaverAgents tree, on the pinned toolchain. Give the build its own target
directory if the box shares one with a gate.

```sh
deploy/bootstrap-stack.sh              # preflight, tests, release build, plan
deploy/bootstrap-stack.sh --install    # then install under sudo
```

What it does, so the log can say which step a failure was at:

1. Prints the box facts and refuses if `cccl` is outside the 3.1.4-3.3.4 window, if
   there is no `nvcc`, or if `/etc/weaver/admin` or `<prefix>/bin` already stands.
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
6. Writes `/etc/weaver/admin`: an empty `allow-list`, `agent-config-directory`,
   the three binary paths, `run-tool`, `control-tool`, `coordination-root=/run`,
   `log-path`, and `unit-properties` carrying `UMask=0000`, the `LD_LIBRARY_PATH`,
   and the journal rate limit turned off. Creates `/var/log/weaver` (root 0750) and
   the agent config directory (operator 0755).

The python SPU is a separate install and not part of this step. Its procedure is
`python-spu/README.md`, "Installing it on a box", and an agent that elects it needs
`spu-implementations` and `agent-spu` in a config root, per the admin Spec section 9.
Stand the Rust stack up first and add it only for an agent that needs it.

## 4. Agents

The per-agent procedure in full is `HowToDeployANewAgent.md`; this is the summary.
One at a time, plan first, then `--apply`. The script refuses to merge with anything
half-made, so a name that already has an account, a directory, a role or a database
must be cleaned up first.

```sh
deploy/create-agent.sh m1 --artifact /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf --engine postgres
deploy/create-agent.sh m1 --artifact /opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf --engine postgres --apply
```

It makes both accounts, the territory with traversal ACLs along the operator's home,
the role and database, the two authentication lines, the declaration, and the
allow-list entry, then proves the member reaches the database and the agent's own uid
does not. The operator's new group membership needs a fresh login or `newgrp`.

An agent electing no store (`[state-store] engine = "none"`) is declared by hand,
since there is no store to provision or probe: make the accounts and territory the
same way (the script's `accounts` and `territory` steps, minus the state directory),
write the declaration from the surface in
`docs/technical/weaver-agents/agent-declaration.md`, and append the name to the
allow-list. Karl on the thinkpad is this case; its declaration is carried byte for
byte between boxes, system prompt included, because the determinism runs compare
against it.

Then, for each:

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin validate <name>
sudo deploy/verify-load.sh <name>              # load, read the load event from the sink, unload
```

`verify-load.sh` counts the sink's lines before and after the load and refuses a load
that wrote nothing, then prints the event kinds it saw and the load event, and unloads
unless told `--keep`.

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
  `/mnt/bulk-store`). Pass the archive path to `decommission.sh` explicitly.
- `~/.cargo/config.toml` may carry a `CUDARC_CUDA_VERSION` override; remove it
  (handoff of 2026-09-29, step 2).
- Its recorded runs used the `e69916a` stack and its CUDA user-space libraries. The
  purge removes `<prefix>/lib` and the `backup-*` directories, so if olympus still
  needs that library set for the probe's B1 re-staging, the archive is where it will
  be, and `box-facts.txt` names each library's sha256.
