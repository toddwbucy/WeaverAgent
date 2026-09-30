# How to deploy a new agent

One agent, on a box that already runs the stack. Written 2026-09-30 from the thinkpad
redeploy, where m1 and karl were made this way and proven to load; the log of that run is
`docs/project/redeploy-2026-09-30-thinkpad.md`. Standing the stack itself up is
`REDEPLOY.md`, and this document starts where that one ends.

Every command runs from the WeaverAgents tree. `<name>` is the agent's name: a unix
user, a database role, a database and a directory, so lowercase letters, digits and
hyphens. Paths below are the thinkpad's defaults; a box's real values are in
`/etc/weaver/admin`, one file per key, and every script reads them from there.

## 0. What an agent is, on disk

Admin creates none of this. It verifies the boundary the operator built and refuses a
load where any piece is missing, so the pieces are made first and admin is asked last.

| Piece | Path | Owner and mode |
|---|---|---|
| Agent account, the worker's uid | `weaver-<name>`, home `/home/weaver-<name>` | system user, nologin, home 2750 |
| Member account, the state store's uid (agents with a store) | `weaver-<name>-state`, no home | system user, nologin |
| Territory | `~operator/.weaveragents/<name>/` (the script names it `weaver-<name>/`) | root:operator 2750 |
| State room (agents with a store) | `<territory>/state/` | member 0700, unreachable by the agent's uid |
| Trace sink | `<territory>/trace.ndjson` | opened by admin under root at load |
| Declaration | `~operator/.weaveragents/<name>.toml` | root or operator, 0644 |
| Store (postgres election) | role and database `weaver_<name>`, one `peer map=weaver` line in `pg_hba.conf`, one `weaver` map line in `pg_ident.conf` | postgres |
| Admission | `<name>` on one line of `/etc/weaver/admin/allow-list` | root |

The operator joins group `weaver-<name>` so the trace, written under root with the
territory's group, is readable without sudo. A session that predates the join needs a
new login or `newgrp weaver-<name>`.

## 1. Before you start

- The stack is installed: `ls /opt/weaver/bin` shows the six members and
  `cat /etc/weaver/admin/allow-list` shows the agents already admitted.
- The artifact is on the box, under `/opt/weaver/models`, and its hash is known. An
  agent's identity is its artifact as much as its prompt; record the sha256 in the run
  log with the declaration's.
- The name collides with nothing: `id weaver-<name>` fails, `~/.weaveragents/<name>*`
  is absent, and for a store election the role and database do not exist. The script
  checks all of this and refuses rather than merging, because a half-made agent that
  looks whole is worse than an absent one.
- PostgreSQL is active, for a store election. The script starts it if not.

## 2. An agent with a store

`create-agent.sh` does the whole of section 0 and then proves both gates. Plan first;
the plan needs no sudo and prints exactly what apply will make.

```sh
deploy/create-agent.sh <name> --artifact /opt/weaver/models/<artifact> --engine postgres
deploy/create-agent.sh <name> --artifact /opt/weaver/models/<artifact> --engine postgres --apply
```

`--engine` names the store and must be one the installed member carries. The script
provisions `postgres` only and refuses `sqlite` and `none`, telling the operator to
declare those by hand; a sqlite path beside the postgres one is #34. `--session` names the session the declaration opens,
default `<name>-001`. Apply ends with two probes: the member reaches the database as its
role, and the agent's own uid is refused. A refusal there is the boundary being wrong,
not the agent.

The declaration the script writes is a working default: the artifact, `devices = [0]`,
a plain system identity, `permission-mode = "ask"`, an empty tool set, surprisal on, the
sink in the territory, and the store's engine, database and role. Edit it before
validating if the agent wants another prompt, a wider context, or `deny`; the fields
are in `docs/technical/weaver-agents/agent-declaration.md`, and nothing defaults, so an
absent or misspelled key refuses the parse by name.

## 3. An agent without a store

`create-agent.sh` refuses `--engine none`, since its second half provisions a store and
probes it and a storeless agent has none of that to verify. The pieces are made by
hand, and they are the script's `accounts` and `territory` steps minus the member and
the state room:

```sh
sudo useradd --system --shell /usr/sbin/nologin --create-home --user-group weaver-<name>
sudo usermod -aG weaver-<name> "$USER"
sudo chmod 2750 /home/weaver-<name>
sudo install -d -o root -g "$USER" -m 2750 ~/.weaveragents/<name>
sudo install -o root -g root -m 0644 <name>.toml ~/.weaveragents/<name>.toml
echo <name> | sudo tee -a /etc/weaver/admin/allow-list
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
path = "/home/todd/.weaveragents/karl/trace.ndjson"
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

## 5. Serving, stopping, unloading

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin load <name>     # answers {"kind":"state","state":"idle"}
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin unload <name>   # answers {"kind":"state","state":"unloaded"}
```

Or `deploy/verify-load.sh <name> --keep` to load with the read-back and leave it serving.
The worker runs as the transient unit `weaver-worker@<name>.service` under the slice
`system-weaver\x2dworker.slice`; `journalctl -u weaver-worker@<name>` is its log, and
admin's own acts are in `/var/log/weaver/admin-operations.ndjson`, root's to read. Two
declarations naming one device serve one loaded agent at a time: the SPU refuses a
conflicted device and never evicts.

## 6. Record

In the run log, per agent: the declaration's sha256 (the load event carries it), the
artifact's sha256, the session, the store election, the load event's run reference,
and anything that refused and what was done about it. The next box hits the refusal
first.

## 7. Taking an agent down

`deploy/decommission.sh` takes down every agent on the box, archived first. For one
agent, the pieces of section 0 are removed in reverse: unload; drop the database, then
the role, and remove its two authentication lines; `userdel -r` both accounts; remove
the territory and the declaration; remove the allow-list line. Archive the territory
before removing it, since the trace is the one record of what the agent did.
