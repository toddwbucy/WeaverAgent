# How to deploy a new agent

One agent, on a box that already runs the stack. First written 2026-09-30 from the
thinkpad redeploy, and rewritten for the start step of #50, which took the agent out of
systemd on the operator's ruling of 2026-10-03: admin stands the agent itself, holds its
run by a run lock, and starts a trace relay beside it. The olympus run that proved this
layout is logged under `docs/project/`. Standing the stack itself up is `REDEPLOY.md`,
and this document starts where that one ends.

Every command runs from the WeaverAgent tree, as the operator, never under sudo. The
scripts ask for sudo where a step needs it. `<name>` is the agent's name: a unix user, a
database role, a database and a directory, so `create-agent.sh` takes lowercase letters
and digits, 2 to 16 characters. Paths below are the defaults, and a box's real values
are in the stack record `/etc/weaver/stack/`, one file per key, which
`bootstrap-stack.sh` wrote and the scripts read, and in each agent's own root
`/etc/weaver/admin/<name>/`, which is all admin reads besides the declaration directory
that root names (`WEAVER_ADMIN_CONFIG` names another base, but a delegated invocation
under sudo always reads `/etc/weaver/admin`).

## 0. What an agent is, on disk

Admin creates none of this. It verifies the boundary the operator built and refuses a
load where any piece is missing, so the pieces are made first and admin is asked last.

| Piece | Path | Owner and mode |
|---|---|---|
| Agent account, the worker's uid | `weaver-<name>`, home `/home/weaver-<name>` | system user, nologin, home 2750 |
| Member account, the state store's uid (agents with a store) | `weaver-<name>-state`, no home | system user, nologin |
| Trace group | `weaver-<name>-trace`, the trace's readers: the operator and the relay, never the member | system group |
| Relay account, the trace relay's uid | `weaver-<name>-relay`, no home | system user, nologin, its one group the trace group |
| Access group, the trace door's | `weaver-<name>-admin` | system group, held by the connector |
| Connector account, admin-con's service user | `weaver-<name>-admincon`, no home | system user, nologin, holds the access group and nothing of the agent's |
| Territory | `<agent-directory>/weaver-<name>/`, the stack record's `agent-directory` (default `/var/lib/weaver-agent`, root 0755) | root:weaver-<name>-state 0710, not setgid: the member passes to its room, lists nothing, and no access entry is set |
| State room (agents with a store) | `<territory>/state/`, where the member writes its save points | member 0700, unreachable by the agent's uid |
| Trace sink | `<territory>/trace.ndjson` | made by create-agent before the first load, root:weaver-<name>-trace 0640, so the member cannot read it, and admin opens it append-only at load and leaves its owner and mode alone |
| Declaration directory | `~/.weaveragent/<name>/` in the operator's home, or `--declaration-directory` | the operator's, 0700, every directory above it the operator's or root's and closed |
| Declaration | `<declaration directory>/agent.toml` | the operator's, written by create-agent as the operator |
| Operations and worker logs | `<declaration directory>/admin.log` and `worker.log` | made by admin at the first verb, the operator's, 0640 |
| Agent root, which is the admission | `/etc/weaver/admin/<name>/`: `worker-binary`, `spu-binary`, `gate-binary`, `coordination-root` (and `library-path`, `headroom-bytes`, `load-bound-seconds` where the stack record has them), copied from the stack record, plus `declaration-directory`, `operator` (the operator's uid) and `roles.toml` (`trace-reader = "weaver-<name>-admincon"`) | root, directory 0755, files 0644, and admin refuses a root that is not root-owned or is group- or world-writable |
| Sudo rule | `/etc/sudoers.d/weaver-<name>` | root 0440, checked by `visudo` |
| Run directory | `<coordination-root>/weaver.run/<name>/`: `run.lock`, `admin.lock`, `trace.sock` | made by admin, root 0755 |

The operator joins three groups: `weaver-<name>` for the gate's socket,
`weaver-<name>-state` for passage through the territory, and `weaver-<name>-trace` to
read the trace without sudo. The member is in none but its own, so it reaches its room
and never the trace, whose content it receives only as the tee's distillate. The
connector reaches the trace through the relay's door alone, by the access group, and
the relay admits only the reader `roles.toml` names. A session that predates the join
needs a fresh login before the groups apply: `newgrp` selects one group in one shell.

## 1. Before you start

- The stack is installed: `ls /opt/weaver/bin` shows the seven members (with
  `weaver-trace-relay` beside the worker), `ls /etc/weaver/stack` shows the stack
  record, and `ls /etc/weaver/admin` shows the agents already admitted, one root each.
- The artifact is on the box, under `/opt/weaver/models`, and its hash is known. An
  agent's identity is its artifact as much as its prompt, so record the sha256 in the
  run log with the declaration's.
- The name collides with nothing: `id weaver-<name>`, `id weaver-<name>-relay`, `id
  weaver-<name>-admincon` and `getent group weaver-<name>-trace weaver-<name>-admin`
  fail, `<agent-directory>/weaver-<name>`, `/etc/weaver/admin/<name>`,
  `/etc/weaver/admin/.<name>.partial` and `/etc/sudoers.d/weaver-<name>` are absent, no
  `agent.toml` stands in the declaration directory. The script checks all of this and refuses rather than
  merging, because a half-made agent that looks whole is worse than an absent one.
- `/etc/sudoers` includes `/etc/sudoers.d` (`@includedir /etc/sudoers.d`).

## 2. An agent with a store

`create-agent.sh` does the whole of section 0 and then proves the boundary. Plan
first. The plan needs no sudo and prints exactly what apply will make.

```sh
deploy/create-agent.sh <name> --artifact /opt/weaver/models/<artifact>
deploy/create-agent.sh <name> --artifact /opt/weaver/models/<artifact> --apply
```

The store is the embedded sqlite engine, `--engine sqlite` and the default, the one
engine this build provides (#1, #86), and any other engine refuses as one this build
does not provide. `--engine none` is refused too, and section 3 makes that agent by
hand. `--session` names the session the declaration opens, default `<name>-001`. `--spu
<path>` gives this agent its own SPU, written as its root's `spu-binary` in place of the
stack record's (the python SPU's zipapp, for instance), and without it the agent serves
from the stack's. `--declaration-directory <path>` places the declaration elsewhere than
`~/.weaveragent/<name>`. `--connector-role operator|observer` chooses which command
lines the connector's sudo rule grants, section 5, default `operator`.

Apply writes the declaration as the operator, then stages the agent root under the
dot-name `/etc/weaver/admin/.<name>.partial`, which admin's name check never admits, and
probes the boundary: the member can write its state room, the agent's own uid cannot
enter it, the member cannot read the trace, the relay holds the trace group alone, and
the connector holds the access group and no group of the agent's. A refusal there is the
boundary being wrong, not the agent, and leaves the staged root in place to read and
remove. Then the sudo rule is rendered, checked with `visudo -cf`, and installed, and
only after all of it does the root move to `/etc/weaver/admin/<name>/`, which is the
admission. The last step runs one line through the rule as the connector's user
(`validate` for the operator role, `show` for the observer), the way admin-con will.

The declaration the script writes is a working default: the artifact, `devices = [0]`, a
plain system identity, `permission-mode = "deny"`, an empty tool set, surprisal on, the
sink in the territory, and the store's engine. Edit `~/.weaveragent/<name>/agent.toml`
as yourself before validating if the agent wants another prompt, a wider context, or
`ask`. No sudo is needed: the file is yours. The fields are in
`docs/technical/weaver-agent/agent-declaration.md`, and nothing defaults, so an absent
or misspelled key refuses the parse by name.

## 3. An agent without a store

`create-agent.sh` refuses `--engine none`, since a storeless agent has no member, no
state room and no store to verify. The pieces are made by hand: the script's `accounts`
and `territory` steps minus the member and the state room, the relay and connector
accounts and the two groups, the declaration directory, then the agent root from the
stack record, with the root made last because it is the admission. The sudo rule is
section 5's, made the same way create-agent makes it.

```sh
N=<name>
sudo useradd --system --shell /usr/sbin/nologin --create-home --user-group weaver-$N
sudo groupadd --system weaver-$N-trace
sudo groupadd --system weaver-$N-admin
sudo useradd --system --shell /usr/sbin/nologin --no-create-home --no-user-group --gid weaver-$N-trace weaver-$N-relay
sudo useradd --system --shell /usr/sbin/nologin --no-create-home --user-group --groups weaver-$N-admin weaver-$N-admincon
sudo usermod -aG weaver-$N,weaver-$N-trace "$USER"
sudo chmod 2750 /home/weaver-$N
T=/var/lib/weaver-agent/weaver-$N      # the stack record's agent-directory, then the account
[ ! -e "$T" ] || { echo "REFUSED: $T already exists"; exit 1; }   # install -d would merge
sudo install -d -o root -g "$USER" -m 2750 "$T"
sudo install -o root -g weaver-$N-trace -m 0640 /dev/null "$T/trace.ndjson"
D=~/.weaveragent/$N
( umask 077; mkdir -p "$D" ) && install -m 0600 $N.toml "$D/agent.toml"
R=/etc/weaver/admin/.$N.partial
sudo install -d -o root -g root -m 0755 "$R"
for k in worker-binary spu-binary gate-binary coordination-root \
         library-path headroom-bytes load-bound-seconds; do
  [ ! -f /etc/weaver/stack/$k ] || sudo cp /etc/weaver/stack/$k "$R/$k"
done
echo "$D" | sudo tee "$R/declaration-directory" >/dev/null
id -u | sudo tee "$R/operator" >/dev/null
echo "trace-reader = \"weaver-$N-admincon\"" | sudo tee "$R/roles.toml" >/dev/null
sudo chmod 0644 "$R"/*
sudo mv -T "$R" /etc/weaver/admin/$N
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

Top-level keys first, then each table, since TOML reads a bare key after a table header
as that table's. Every identity message is `role = "system"`. `allowed-uids` is who may
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
prints their kinds and the load event, then reads `show`'s constituents and checks each:
it runs as the agent's, the member's or the relay's account, it sits in the invoker's own
cgroup, and a relay stands among them for a file sink. It then unloads, and checks that
every constituent is gone and `show` reads unloaded. A healthy first load writes `load`
and `message.system`, plus `recall` where a store member stands. The load event's
payload names the session, the run, the store, the declaration's hash, the composer
(`worker`), and the `stack` hashes of the installed members. A load that answers `idle`
and writes nothing is the failure this step exists to catch, and the worker's own words
are in `<declaration directory>/worker.log`, the state member's in
`<territory>/state/state.log`.

`load` never ends a run that stands: it answers `agent_running` and leaves the decision
to its caller, who reads `show` and issues `unload`.

## 5. Serving, stopping, unloading, and the connector's rule

```sh
sudo /opt/weaver/bin/weaver-admin load <name>     # answers {"kind":"state","state":"idle"}
sudo /opt/weaver/bin/weaver-admin show <name>     # the state and the run's constituent pids
sudo /opt/weaver/bin/weaver-admin unload <name>   # answers {"kind":"state","state":"unloaded"}
```

Or `sudo deploy/verify-load.sh <name> --keep` to load with the read-back and leave it
serving. No unit and no init system is involved. The worker, the state member and the
relay are processes admin started, detached from the invoking terminal, holding the run
lock between them, and they live in the containment the load was invoked from: a load
run from a login shell's scope lives in that scope. `unload` asks the agent to leave,
then ends whatever still holds the run lock, within 105 seconds. Admin's own acts on
this agent are in `<declaration directory>/admin.log`, and the worker's output in
`worker.log` beside it, both the operator's to read.

**The connector's rule** is `/etc/sudoers.d/weaver-<name>`, which create-agent writes:

```text
Defaults:weaver-<name>-admincon !pam_session
weaver-<name>-admincon ALL=(root) NOPASSWD: /opt/weaver/bin/weaver-admin show <name>, /opt/weaver/bin/weaver-admin validate <name>, /opt/weaver/bin/weaver-admin load <name>, /opt/weaver/bin/weaver-admin unload <name>, /opt/weaver/bin/weaver-admin stop <name>
```

The observer role's rule grants the `show` line alone. Each grant is one fixed command
line, so the connector can pass no argument of its own, and it runs them with `sudo -n`.
`!pam_session` keeps sudo from opening a session that would move the invocation, and the
agent it starts, out of the connector's own containment, which is what lets stopping the
connector stop its agent (toddwbucy/WeaverWeb#15). After a load the connector checks
that every constituent `show` names sits in its own cgroup. `sudo -n -l -U
weaver-<name>-admincon` prints what the rule grants. The connector itself, its service
and its containment, is WeaverWeb's packaging, and this repository provisions only its
account, its group, its rule and its place in `roles.toml`.

Two declarations naming one device serve one loaded agent at a time: the SPU refuses a
conflicted device and never evicts.

## 6. Record

In the run log, per agent: the declaration's sha256 (the load event carries it), the
artifact's sha256, the session, the store election, the load event's run reference,
and anything that refused and what was done about it. The next box hits the refusal
first.

## 7. Taking an agent down

`deploy/decommission.sh` takes every agent off a box, archiving first, and refuses while
`show` reports any of them running. To take one agent down by hand, remove the pieces of
section 0 in reverse. Unload it, and check `show` reads unloaded with no constituent.
Remove its sudo rule `/etc/sudoers.d/weaver-<name>` first, so the connector can run
nothing more, then its root `/etc/weaver/admin/<name>/`, which ends its admission, and
its run directory `<coordination-root>/weaver.run/<name>/`. `userdel -r` the agent's
account and `userdel` the member's, the relay's and the connector's, then delete the
groups, which `userdel` leaves while a member remains and which a later `useradd
--user-group` of the same name would refuse on. Archive the territory and remove it. The
declaration directory is yours and stays: it holds the declaration, the prompt and the
two logs, the one record of what was done to the agent, and create-agent refuses to
write over its `agent.toml`, so move it aside before making an agent of the same name
again.

The groups, once the accounts are gone:

```sh
for g in weaver-<name> weaver-<name>-state weaver-<name>-trace weaver-<name>-admin weaver-<name>-admincon; do
  getent group "$g" >/dev/null && sudo groupdel "$g"
done
```
