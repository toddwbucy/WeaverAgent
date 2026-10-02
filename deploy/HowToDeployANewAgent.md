# How to deploy a new agent

One agent, on a box that already runs the stack. Written 2026-09-30 from the thinkpad
redeploy, where m1 and karl were made this way and proven to load; the log of that run is
`docs/project/redeploy-2026-09-30-thinkpad.md`. Standing the stack itself up is
`REDEPLOY.md`, and this document starts where that one ends.

Every command runs from the WeaverAgent tree. `<name>` is the agent's name: a unix
user, a database role, a database and a directory, so `create-agent.sh` takes
lowercase letters and digits, 2 to 16 characters. Paths below are the defaults; a
box's real values are in the stack record `/etc/weaver/stack/`, one file per key,
which `bootstrap-stack.sh` wrote and the scripts read, and in each agent's own root
`/etc/weaver/admin/<name>/`, which is all admin reads (`WEAVER_ADMIN_CONFIG` names
another base).

## 0. What an agent is, on disk

Admin creates none of this. It verifies the boundary the operator built and refuses a
load where any piece is missing, so the pieces are made first and admin is asked last.

| Piece | Path | Owner and mode |
|---|---|---|
| Agent account, the worker's uid | `weaver-<name>`, home `/home/weaver-<name>` | system user, nologin, home 2750 |
| Member account, the state store's uid (agents with a store) | `weaver-<name>-state`, no home | system user, nologin |
| Territory | `<agent-directory>/weaver-<name>/`, the stack record's `agent-directory` (default `/var/lib/weaver-agent`, root 0755) | root:weaver-<name>-state 0710, not setgid: the member passes to its room, lists nothing, and no access entry is set |
| State room (agents with a store) | `<territory>/state/`, where a sqlite store keeps its file | member 0700, unreachable by the agent's uid |
| Trace group (agents with a store) | `weaver-<name>-trace`, the trace's readers: the operator, never the member | system group |
| Trace sink | `<territory>/trace.ndjson` | made by create-agent before the first load, root:weaver-<name>-trace 0640, so the member cannot read it; admin opens it append-only at load and leaves its owner and mode alone |
| Agent root, which is the admission | `/etc/weaver/admin/<name>/`: `worker-binary`, `spu-binary`, `gate-binary`, `run-tool`, `control-tool`, `coordination-root`, `unit-properties` (and `headroom-bytes`, `state-store-socket` where the stack record has them), copied from the stack record, plus `log-path` | root, directory 0755, files 0644; admin refuses a root that is not root-owned or is group- or world-writable |
| Declaration | `/etc/weaver/admin/<name>/agent.toml` | root, 0644 |
| Operations log | `/var/log/weaver/<name>/admin.log`, named by `log-path` | directory root 0750; admin writes the file |
| Store (postgres election only) | role and database `weaver_<name>`, one `peer map=weaver` line in `pg_hba.conf`, one `weaver` map line in `pg_ident.conf` | postgres |

The operator joins three groups: `weaver-<name>` for the gate's socket,
`weaver-<name>-state` for passage through the territory, and `weaver-<name>-trace` to
read the trace without sudo. The member is in none but its own, so it reaches its room
and never the trace, whose content it receives only as the tee's distillate. A session
that predates the join needs a fresh login before the groups apply: `newgrp` selects one
group in one shell.

## 1. Before you start

- The stack is installed: `ls /opt/weaver/bin` shows the six members,
  `ls /etc/weaver/stack` shows the stack record, and `ls /etc/weaver/admin` shows the
  agents already admitted, one root each.
- The artifact is on the box, under `/opt/weaver/models`, and its hash is known. An
  agent's identity is its artifact as much as its prompt; record the sha256 in the run
  log with the declaration's.
- The name collides with nothing: `id weaver-<name>` and `getent group weaver-<name>-trace`
  fail, `<agent-directory>/weaver-<name>` (`/var/lib/weaver-agent/weaver-<name>` by default),
  `/etc/weaver/admin/<name>` and `/var/log/weaver/<name>` are absent, and for a
  postgres election the role and database do not exist. The script checks all of this
  and refuses rather than merging, because a half-made agent that looks whole is worse
  than an absent one.
- PostgreSQL is active, for a postgres election. The script starts it if not.

## 2. An agent with a store

`create-agent.sh` does the whole of section 0 and then proves the boundary. Plan
first; the plan needs no sudo and prints exactly what apply will make.

```sh
deploy/create-agent.sh <name> --engine <sqlite|postgres> --artifact /opt/weaver/models/<artifact>
deploy/create-agent.sh <name> --engine <sqlite|postgres> --artifact /opt/weaver/models/<artifact> --apply
```

`--engine` names the store and is required, since neither engine is the default: `sqlite`
or `postgres`, either one the installed member carries. `none` is refused, and section 3
makes that agent by hand.
`--session` names the session the declaration opens, default `<name>-001`. `--spu
<path>` gives this agent its own SPU, written as its root's `spu-binary` in place of
the stack record's (the python SPU's zipapp, for instance); without it the agent
serves from the stack's.

Apply stages the agent root under the dot-name `/etc/weaver/admin/.<name>.partial`,
which admin's name check never admits, then probes the boundary. For sqlite: the
member can write its state room, and the agent's own uid cannot enter it. For
postgres: the member reaches the database as its role, and the agent's own uid is
refused. A refusal there is the boundary being wrong, not the agent, and leaves the
staged root in place to read and remove. Only after both probes does the root move
to `/etc/weaver/admin/<name>/`, which is the admission.

The declaration the script writes is a working default: the artifact, `devices = [0]`,
a plain system identity, `permission-mode = "deny"`, an empty tool set, surprisal on,
the sink in the territory, and the store's engine (with its database and role for
postgres). Edit `/etc/weaver/admin/<name>/agent.toml` under sudo before validating if
the agent wants another prompt, a wider context, or `ask`; the fields are in
`docs/technical/weaver-agent/agent-declaration.md`, and nothing defaults, so an
absent or misspelled key refuses the parse by name.

## 3. An agent without a store

`create-agent.sh` refuses `--engine none`, since a storeless agent has no member, no
state room and no store to verify. The pieces are made by hand: the script's
`accounts` and `territory` steps minus the member and the state room, then the agent
root from the stack record, with the root made last because it is the admission:

```sh
sudo useradd --system --shell /usr/sbin/nologin --create-home --user-group weaver-<name>
sudo usermod -aG weaver-<name> "$USER"
sudo chmod 2750 /home/weaver-<name>
T=/var/lib/weaver-agent/weaver-<name>      # the stack record's agent-directory, then the account
[ ! -e "$T" ] || { echo "REFUSED: $T already exists"; exit 1; }   # install -d would merge
sudo install -d -o root -g "$USER" -m 2750 "$T"
sudo install -d -o root -g root -m 0750 /var/log/weaver/<name>
R=/etc/weaver/admin/.<name>.partial
sudo install -d -o root -g root -m 0755 "$R"
for k in worker-binary spu-binary gate-binary run-tool control-tool \
         coordination-root unit-properties headroom-bytes state-store-socket; do
  [ ! -f /etc/weaver/stack/$k ] || sudo cp /etc/weaver/stack/$k "$R/$k"
done
echo /var/log/weaver/<name>/admin.log | sudo tee "$R/log-path" >/dev/null
sudo install -o root -g root -m 0644 <name>.toml "$R/agent.toml"
sudo chmod 0644 "$R"/*
sudo mv -T "$R" /etc/weaver/admin/<name>
```

The declaration is written by hand. karl's, which loaded on 2026-09-30, is the shape:

```toml
session = "s-karl-1"
tool-set = []
permission-mode = "ask"

[spu-instruction.decoder]
residual-readout-election = false
surprisal-election = true
tunable-values = { seed = 451234785645, context-capacity = 16384, max-tokens-per-turn = 1024 }

[spu-instruction.decoder.model-binding]
artifact = "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf"
devices = [0]

[[spu-instruction.decoder.identity]]
role = "system"

[[spu-instruction.decoder.identity.content]]
type = "text"
text = """
You are Karl, a small local agent running on this laptop.
You have no tools in this session. Answer plainly and
briefly, and say so when you do not know something."""

[gate-instruction.access-rule]
allowed-uids = [1000]
allowed-gids = []
denied-uids = []

[trace-sink]
kind = "file"
path = "/var/lib/weaver-agent/weaver-karl/trace.ndjson"
create = true

[state-store]
engine = "none"
```

Top-level keys first, then each table; TOML reads a bare key after a table header as
that table's. Every identity message is `role = "system"`. `allowed-uids` is who may
dial the gate, the operator's uid here. Check it parses before installing it:
`python3 -c 'import tomllib,sys; tomllib.load(open(sys.argv[1],"rb"))' <name>.toml`.

An agent carried between boxes keeps its declaration byte for byte, prompt included,
with only the paths changed: the determinism runs compare against it, and the load
event records the declaration's sha256.

## 4. Validate and prove the load

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin validate <name>
sudo deploy/verify-load.sh <name>
```

`validate` answers `{"kind":"validated"}` or a refusal naming the field or the boundary
piece. `verify-load.sh` validates, loads, counts the events that arrive in the sink,
prints their kinds and the load event, and unloads. A healthy first load writes `load`
and `message.system`, plus `recall` where a store member stands; the load event's
payload names the session, the run, the store, the declaration's hash, the composer
(`worker`), and the `stack` hashes of the installed members. A load that answers
`idle` and writes nothing is the failure this step exists to catch, and the state
member's last words are in `<territory>/state/state.log`.

A failed load needs no clearing by hand before the next one: admin clears the failed
unit itself (`systemctl reset-failed`), so the next load is not refused as
`prior_unit_unreaped`. It does not load again on its own.

## 5. Serving, stopping, unloading

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin load <name>     # answers {"kind":"state","state":"idle"}
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin unload <name>   # answers {"kind":"state","state":"unloaded"}
```

Or `deploy/verify-load.sh <name> --keep` to load with the read-back and leave it serving.
The worker runs as the transient unit `weaver-worker@<name>.service` under the slice
`system-weaver\x2dworker.slice`; `journalctl -u weaver-worker@<name>` is its log, and
admin's own acts on this agent are in `/var/log/weaver/<name>/admin.log`, root's to
read. Two
declarations naming one device serve one loaded agent at a time: the SPU refuses a
conflicted device and never evicts.

## 6. Record

In the run log, per agent: the declaration's sha256 (the load event carries it), the
artifact's sha256, the session, the store election, the load event's run reference,
and anything that refused and what was done about it. The next box hits the refusal
first.

## 7. Taking an agent down

`deploy/decommission.sh` does not yet understand the per-agent layout and must not be
run on a box on it until toddwbucy/WeaverAgent#35 lands. Take an agent down by hand: the pieces of section 0 are removed in reverse: unload; remove its root
`/etc/weaver/admin/<name>/`, which ends its admission; for postgres, drop the database,
then the role, and remove its two authentication lines; `userdel -r` both accounts, and
`groupdel weaver-<name>-trace` (a group of its own that `userdel` leaves, the operator
still in it);
remove the territory and the log directory `/var/log/weaver/<name>/`. Archive the
territory, the root and the log before removing them, since the trace is the one
record of what the agent did and the log the one record of what was done to it.
