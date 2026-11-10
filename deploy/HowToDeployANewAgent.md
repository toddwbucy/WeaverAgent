# How to deploy a new agent

**Subject to change.** This runbook runs ahead of the project's actual state. It is only as
authoritative as it remains functional, never above a Spec or a live run, and it stays a
moving target until the repository is locked down (the operator's word of 2026-10-15).

One agent, on a box that already runs the stack. First written 2026-09-30 from the
thinkpad redeploy, and rewritten for the start step of #50, which took the agent out of
systemd on the operator's ruling of 2026-10-03: admin stands the agent itself, holds its
run by a run lock, and starts a trace relay beside it. The run that proved this
layout is logged under `docs/project/`. Standing the stack itself up is `REDEPLOY.md`,
and this document starts where that one ends.

Every command runs from the WeaverAgent tree, as the operator, never under sudo. The
scripts ask for sudo where a step needs it. `<name>` is the agent's name: a unix user, a
group and a directory, so `create-agent.sh` takes lowercase letters and digits, 2 to 16
characters. Paths below are the defaults, and a box's real values
are in the stack record `/etc/weaver/stack/`, one file per key, which
`bootstrap-stack.sh` wrote and the scripts read, and in each agent's own root
`/etc/weaver/admin/<name>/`, which is all admin reads besides the territory that root
names (`WEAVER_ADMIN_CONFIG` names another base, but a delegated invocation under sudo
always reads `/etc/weaver/admin`). **The whole agent lives in its territory** (the
operator's ruling of 2026-10-07 on #1): the declaration, the prompt draft, the two logs
and the published save points stand under `/var/lib/weaver-agent/weaver-<name>/` beside the
state room and the trace, root's, and nothing of the agent is in your home.

## 0. What an agent is, on disk

Admin makes none of the boundary: the accounts, groups, territory, declaration and root
are the operator's. It verifies them and refuses a load where any piece is missing, so
the pieces are made first and admin is asked last; what admin does make is named in the
table.

| Piece | Path | Owner and mode |
|---|---|---|
| Agent account, the worker's uid | `weaver-<name>`, home `/home/weaver-<name>` | system user, nologin, home 2750; its group `weaver-<name>` is the gate socket's, held by the box's gate-con account |
| Member account, the state store's uid (agents with a store) | `weaver-<name>-state`, no home | system user, nologin; its group `weaver-<name>-state`, the state group, is the territory's and stands for every agent, a storeless one included |
| Trace group | `weaver-<name>-trace`, the trace's readers: the operator and the relay, never the member | system group |
| Relay account, the trace relay's uid | `weaver-<name>-relay`, no home | system user, nologin, its one group the trace group |
| Access group, the territory files' and the trace door's | `weaver-<name>-admin` | system group, held by the box's admin-con account and the operator, never the member or the agent's uid; it reads and never writes |
| The box's gate-con account, gate-con's service user, one per box | `weaver-gatecon`, no home | system user below `UID_MIN`, nologin, its own primary group shared with no other account; laid by the first create-agent run that finds it absent and judged by its shape by every later one; holds each agent's group `weaver-<name>` for the gate's socket and no state, access or trace group; the one uid each new agent's access rule names |
| The box's admin-con account, admin-con's service user, one per box | `weaver-admincon`, no home | system user below `UID_MIN`, nologin, its own primary group shared with no other account; laid and judged as gate-con's is; holds each agent's state group for passage and access group for reading, and no agent's own group or trace group; the account each agent's sudo rule and `roles.toml` name |
| Territory | `<agent-directory>/weaver-<name>/`, the stack record's `agent-directory` (default `/var/lib/weaver-agent`, root 0755) | root:weaver-<name>-state 0710, not setgid: the state group passes to what stands in it by name, which the member holds as its own and the operator and the box's admin-con account join; the agent's own uid passes not; none lists; the files' own modes are the wall beneath, and no access entry is set |
| State room (agents with a store) | `<territory>/state/`, where the member writes its save points | member 0700, unreachable by the agent's uid |
| Trace sink | `<territory>/trace.ndjson` | made by create-agent before the first load, root:weaver-<name>-trace 0640, so the member cannot read it, and admin opens it append-only at load and leaves its owner and mode alone |
| Declaration | `<territory>/agent.toml` | root:weaver-<name>-admin 0640, written by create-agent as root; you edit it with `sudoedit`, and admin refuses any other owner, group or mode |
| Prompt draft | `<territory>/system-prompt.md` | root:weaver-<name>-admin 0640, written by create-agent; `deploy/turn.py <name> --system` reads it through the access group, for an agent with a seeding sender (section 4) |
| Operations and worker logs | `<territory>/admin.log` and `worker.log` | made by admin at the first verb, root:weaver-<name>-admin 0640 |
| Published save points and their manifest | `<territory>/save-points/`: `<YYYYMMDDTHHMMSSZ>-<digest>.save-point` and `save-points.manifest` | the directory root:weaver-<name>-admin 0750; each save point root:weaver-<name>-admin 0640, the manifest root 0644, all written by admin |
| Agent root, which is the admission | `/etc/weaver/admin/<name>/`: `worker-binary`, `spu-binary`, `gate-binary`, `coordination-root` (and `library-path`, `headroom-bytes`, `load-bound-seconds`, `shell-binary` where the stack record has them), copied from the stack record, plus `territory` (the territory's path), `operator` (the operator's uid) and `roles.toml` (`trace-reader = "weaver-admincon"`) | root, directory 0755, files 0644, and admin refuses a root that is not root-owned or is group- or world-writable |
| Sudo rule | `/etc/sudoers.d/weaver-<name>` | root 0440, checked by `visudo` |
| Run directory | `<coordination-root>/weaver.run/<name>/`: `run.lock`, `admin.lock`, `trace.sock` | made by admin, root 0755 |

**The territory is the state group's and passes it; the access group reads the files**
(the operator's ruling of 2026-10-08 on #1). The operator joins three groups:
`weaver-<name>-trace` to read the trace without sudo, `weaver-<name>-state` to pass the
territory, and `weaver-<name>-admin` to read the declaration, the draft, the logs and
the save points without sudo. The operator does not join `weaver-<name>`: the gate's
caller is the box's gate-con account alone (`weaver-gate-world-contract` section 1, the
operator's rulings of 2026-11-10), and an agent laid by an earlier create-agent.sh,
whose rule names the operator and whose groups the operator holds, keeps that layout.
The member holds the state group alone and reaches its room by the territory's passage
at 0710, which only that group has; it is in neither the access group nor the trace
group, so it reads neither the logs nor the save points nor the trace, whose content it
receives only as the tee's distillate. The box's admin-con account holds the state group
for passage and the access group for reading, and no group of the agent's or the
trace's. The box's gate-con account holds the agent's group for the gate's socket and
nothing of the territory or the trace. The agent's own uid holds neither the state group
nor the access group, so it cannot pass the territory. The access group reads and never
writes: the admin-con account cannot rewrite the declaration. It reaches the trace
through the relay's door alone, by the access group, and the relay admits only the
reader `roles.toml` names. **The two box accounts reach every agent laid with them, and
the reach is the design** (`weaver-admin-Spec` section 2): WeaverWeb is the operator's
console and holds every key, so per-agent accounts would bound nothing. A session that
predates the join needs a fresh login before the groups apply: `newgrp` selects one
group in one shell.

## 1. Before you start

- The stack is installed: `ls /opt/weaver/bin` shows the seven members (with
  `weaver-trace-relay` beside the worker) and the shell's `busybox` and
  `busybox.applets` (#29), `ls /etc/weaver/stack` shows the stack
  record, and `ls /etc/weaver/admin` shows the agents already admitted, one root each.
- The artifact is on the box, under `/opt/weaver/models`, and its hash is known. An
  agent's identity is its artifact as much as its prompt, so record the sha256 in the
  run log with the declaration's.
- The name collides with nothing and is neither `gatecon` nor `admincon`, which name the
  box's accounts and refuse: `id weaver-<name>`, `id weaver-<name>-state`, `id
  weaver-<name>-relay` and `getent group weaver-<name> weaver-<name>-state
  weaver-<name>-trace weaver-<name>-admin` fail, `/home/weaver-<name>`,
  `<agent-directory>/weaver-<name>`, `/etc/weaver/admin/<name>`,
  `/etc/weaver/admin/.<name>.partial` and `/etc/sudoers.d/weaver-<name>` are absent. The
  script checks all of this and refuses rather than merging, because a half-made agent
  that looks whole is worse than an absent one.
- `weaver-gatecon` and `weaver-admincon` either are absent, and apply lays them, or
  stand in the box's shape: `getent passwd weaver-gatecon weaver-admincon` shows a uid
  above 0 and below `UID_MIN` and a nologin shell, and each one's primary group has no
  members and is no other account's. Each holds no group but what it may hold:
  `weaver-gatecon` its own group and agents' own groups `weaver-<name>`,
  `weaver-admincon` its own group and agents' `weaver-<name>-state` and
  `weaver-<name>-admin`, and neither any other group, `wheel`, `sudo` or `docker`
  among them. The plan says which it found, and refuses an account in any other shape.
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
stack record's (a separately installed Rust SPU, for instance), and without it the
agent serves from the stack's. `--connector-role operator|observer` chooses which
command lines the agent's sudo rule grants the box's admin-con account, section 5,
default `operator`.

Apply lays the box's two connector accounts where the plan found them absent and judges
them by their shape, adds `weaver-gatecon` to the agent's group and `weaver-admincon` to
its state and access groups, makes the agent's accounts and the territory, writes the
declaration and the draft into it as root, then stages the agent root under the dot-name
`/etc/weaver/admin/.<name>.partial`, which admin's name check never admits, and probes
the boundary: the member can write its state room, the agent's own uid cannot enter it
nor pass the territory, the member cannot read the trace, the relay holds the trace
group alone, the admin-con account holds the state and access groups and no group of the
agent's or the trace's, the gate-con account holds the agent's group and no state,
access or trace group, and the admin-con account reads the declaration and can write
neither it, the draft nor the save points directory. A refusal there is the boundary
being wrong, not the agent, and leaves the staged root in place to read and remove. Then
the sudo rule is rendered, checked with `visudo -cf`, and installed, and only after all
of it does the root move to `/etc/weaver/admin/<name>/`, which is the admission. The
last step runs one line through the rule as `weaver-admincon` (`validate` for the
operator role, `show` for the observer), the way admin-con will. A gate-con or admin-con
process already running takes the new agent's groups only when it restarts, since a
process's groups are fixed at its start.

The declaration the script writes is a working default: the artifact, `devices = [0]`,
an empty tool set (no tool is offered, #29), surprisal on, the sink in the territory,
and the store's engine, and its `allowed-uids` names the gate-con account's uid alone.
It carries no system prompt: the prompt is state, on the operator's ruling of
2026-10-06, and a seeding line is taken from the operator's uid alone, which this
declaration's gate does not admit. **An agent laid this way has no seeding sender**
(`weaver-gate-world-contract` section 2, the operator's ruling of 2026-11-10), and how
its prompt enters is deferred to its own act, section 4. Beside the declaration the
script still writes the draft, `<territory>/system-prompt.md`, a plain prompt that is
yours to edit. Edit `agent.toml` for a wider context before validating. Both files
are root:weaver-<name>-admin 0640: `sudoedit
/var/lib/weaver-agent/weaver-<name>/agent.toml` edits one, and you read either through
the access group. The fields are in `docs/technical/weaver-agent/agent-declaration.md`,
a snapshot of 2026-08-25 that still shows an identity table, and
`docs/crates/weaver-types/weaver-types-Spec.md` section 2 is the authority. A required
key absent, or any key misspelled, refuses the parse by name, and the optional keys
(`tool-set`, `binding-kind`, `gate-instruction`, `state-election`, `state-store`,
`restore`, `lifecycle`, `turn-context`) carry the absence their Spec rules.
`turn-context = "independent"` (#423), a top-level key, answers every turn from the
seated prompt and that turn's request alone, the decode context returned to the prompt
before each turn and the load event naming the election. Absent, it is `continuous`,
the conversation accumulating as before.

**Electing the shell** (#29). `tool-set = ["sh"]` offers the agent one tool, a POSIX `sh`
(busybox's ash) over the stack's listed commands (`<prefix>/bin/busybox.applets`), in
the agent's home, with no network, confined as `weaver-gate-Spec` section 8.1 states.
The agent's root needs `shell-binary`, which `create-agent.sh` copies from the stack
record. An agent made before the stack named it carries none and refuses at `validate`
until it is recreated. The model learns the tool from the system prompt, so describe it
in `system-prompt.md` before any seeding: its name `sh`, that it is POSIX `sh` and
not bash, the listed commands, and that it has no network.

**An adapted agent** (the weaver-spu adapter binding amendment, ADP-1, ADP-2 and
ADP-5). A LoRA adapter rides the base binding as its `adapter` table, declared with
four flags, all four or none:

```sh
deploy/create-agent.sh <name> --artifact /opt/weaver/models/<base> \
  --adapter /opt/weaver/models/<adapter>.gguf --adapter-sha256 <hex> \
  --adapter-scale 1.0 --base-blake3 <hex> --apply
```

The script computes nothing. `--adapter-sha256` is the adapter file's SHA-256, from
the conversion record, and `--base-blake3` is the BLAKE3 the base was trained against,
from the fixture report; both are 64 lowercase hexadecimal characters. `--adapter-scale`
is a finite decimal (`1.0`, `0.7`). The script refuses a partial set, a digest of
another shape and a scale that is not a finite decimal, before it makes anything. The
SPU checks both digests at admission, on the path the LoRA fixture proves: a file that
does not hash to the declared digest refuses `ArtifactUnreadable` at `adapter-hash`, and
a base that is not the declared one refuses `DeviceCannotAdmit` at
`adapter-base-identity`. The SPU's line in `worker.log` names the adapter's path, the
stage and the identity the check ran as.

**The adapter file takes the base's custody**: a regular file owned outside the agent's
uid, readable by it, and not writable by it (LoRA amendment LORA-2). A file the agent's
uid owns, or one it can write, refuses at `adapter-custody`. Admin checks only that the
path is named; the SPU resolves and judges it at admission. The adapter is part of the
binding, so adding, changing or removing it is a declaration edit followed by a reload,
like a change of base.

## 3. An agent without a store

`create-agent.sh` refuses `--engine none`, since a storeless agent has no member, no
state room and no store to verify. The pieces are made by hand: the script's `accounts`
and `territory` steps minus the member account and the state room, the relay account
and the three groups, the box accounts joined to the agent's groups, the declaration and
the draft in the territory, then the agent root from the stack record, with the root
made last because it is the admission. **A storeless agent carries the state group
`weaver-<name>-state` with no member account**: it is the territory's group, which the
operator and the box's admin-con account pass by, and admin resolves it for every verb.
**The box's two accounts must already stand in their shape** (section 0), since the
block judges nothing: a `create-agent.sh` run lays and judges them, and its plan for any
name prints what it found. The block runs as the operator, one chain, so
a step that fails stops the rest; it refuses a territory, a stage or a root that already
stands, since `install -d` would merge with it. The territory is the stack record's
`agent-directory` made canonical, as `create-agent.sh` makes it, and the 5-digit modes
clear a setgid bit the base would otherwise pass down. The block stages the root; then
install the sudo rule by the second block below, with `A=$N`, and only then move the
root into place, which is the admission: `sudo mv -T --
/etc/weaver/admin/.$N.partial /etc/weaver/admin/$N`.

```sh
N=<name>
T=$(realpath -e -- "$(cat /etc/weaver/stack/agent-directory)")/weaver-$N \
&& R=/etc/weaver/admin/.$N.partial \
&& [ ! -e "$T" ] && [ ! -L "$T" ] && [ ! -e "$R" ] && [ ! -e /etc/weaver/admin/$N ] \
&& getent passwd weaver-gatecon weaver-admincon >/dev/null \
&& sudo useradd --system --shell /usr/sbin/nologin --create-home --user-group weaver-$N \
&& sudo groupadd --system weaver-$N-state \
&& sudo groupadd --system weaver-$N-trace \
&& sudo groupadd --system weaver-$N-admin \
&& sudo useradd --system --shell /usr/sbin/nologin --no-create-home --no-user-group --gid weaver-$N-trace weaver-$N-relay \
&& sudo usermod -aG weaver-$N weaver-gatecon \
&& sudo usermod -aG weaver-$N-state,weaver-$N-admin weaver-admincon \
&& sudo usermod -aG weaver-$N-trace,weaver-$N-state,weaver-$N-admin "$(id -un)" \
&& sudo chmod 2750 /home/weaver-$N \
&& sudo install -d -o root -g weaver-$N-state -m 00710 "$T" \
&& sudo install -o root -g weaver-$N-trace -m 0640 /dev/null "$T/trace.ndjson" \
&& sudo install -d -o root -g weaver-$N-admin -m 00750 "$T/save-points" \
&& sudo install -o root -g weaver-$N-admin -m 0640 $N.toml "$T/agent.toml" \
&& sudo install -o root -g weaver-$N-admin -m 0640 $N-prompt.md "$T/system-prompt.md" \
&& sudo install -d -o root -g root -m 0755 "$R" \
&& { ok=1; for k in worker-binary spu-binary gate-binary coordination-root \
                    library-path headroom-bytes load-bound-seconds shell-binary; do
       [ ! -f /etc/weaver/stack/$k ] || sudo cp -- /etc/weaver/stack/$k "$R/$k" || { ok=0; break; }
     done; [ "$ok" = 1 ]; } \
&& printf '%s\n' "$T" | sudo tee "$R/territory" >/dev/null \
&& id -u | sudo tee "$R/operator" >/dev/null \
&& printf 'trace-reader = "%s"\n' weaver-admincon | sudo tee "$R/roles.toml" >/dev/null \
&& sudo chmod 0644 "$R"/* \
&& echo "$N: staged at $R; install the sudo rule, then: sudo mv -T -- $R /etc/weaver/admin/$N"
```

**Install the agent's sudo rule for the box's admin-con account**, staged and checked as
`create-agent.sh` stages it: under a dot-name inside `/etc/sudoers.d`, which sudo never
reads and no other principal can claim, checked by `visudo`, then moved into place.
Never stage it under `/tmp`, where any local user can make the name first.

```sh
A=<a>; ADMIN=$(realpath -e -- <prefix>/bin/weaver-admin); lines=""
for verb in show validate load unload stop save-point restore force-unload; do
  lines="$lines${lines:+, }$ADMIN $verb $A"
done
sudo grep -qE '^[#@]includedir[[:space:]]+/etc/sudoers\.d([[:space:]]|$)' /etc/sudoers \
&& rule=$(sudo mktemp "/etc/sudoers.d/.weaver-$A.XXXXXX") \
&& printf 'Defaults:%s !pam_session\n%s ALL=(root) NOPASSWD: %s\n' weaver-admincon weaver-admincon "$lines" \
   | sudo tee "$rule" >/dev/null \
&& sudo chmod 0440 "$rule" \
&& { sudo visudo -cqf "$rule" || { sudo rm -f -- "$rule"; false; }; } \
&& sudo mv -T -- "$rule" "/etc/sudoers.d/weaver-$A" \
&& sudo -l -U weaver-admincon
```

These are the operator role's eight command lines, as `create-agent.sh` grants them.
The observer role's rule grants the `show` line alone: run the loop over `show`
only. A chain that stops after `mktemp` leaves the dot-named stage, which sudo
ignores; remove it by hand.

The declaration is written by hand. karl's, which loaded on 2026-09-30, is the shape,
in the grammar of 2026-10-06, its prompt no longer in it, and its access rule moved
from the operator's uid to the gate-con account's:

```toml
session = "s-karl-1"
tool-set = []

[spu-instruction.decoder]
residual-readout-election = false
surprisal-election = true
tunable-values = { seed = 451234785645, context-capacity = 16384, max-tokens-per-turn = 1024 }

[spu-instruction.decoder.model-binding]
artifact = "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf"
devices = [0]

[gate-instruction.access-rule]
allowed-uids = [990]   # what id -u weaver-gatecon prints on this box
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
as that table's. The inline `[[spu-instruction.decoder.identity]]` table of a
declaration written before 2026-10-06, and the `identity-file` key of the days between,
each refuse by name, and an agent carrying either is recreated (section 7). **The sink's
`path` is `$T/trace.ndjson`, with `$T` the canonical territory the block derives**,
karl's being the default's: admin compares the sink's directory with the canonical
territory, so a path through a link, or under another base, refuses every `validate`,
`load` and `restore` `config_invalid` naming `trace-sink`. `allowed-uids` is who may
dial the gate, the box's gate-con account alone (`weaver-gate-world-contract` section
1). The harness takes a seeding line from the operator's uid alone, which this rule does
not admit, so an agent declared this way has no seeding sender (section 4). Check it
parses before installing it: `python3 -c 'import tomllib,sys;
tomllib.load(open(sys.argv[1],"rb"))' <name>.toml`.

Karl's prompt, the draft beside the declaration (`$N-prompt.md` above, installed as
`<territory>/system-prompt.md`):

```text
You are Karl, a small local agent running on this laptop.
You have no tools in this session. Answer plainly and
briefly, and say so when you do not know something.
```

An agent carried between boxes keeps its declaration byte for byte, with only the paths
and the access rule's uid changed, and where it has a seeding sender its prompt is
seeded on the new box from the same draft: the determinism runs compare against both,
and the load event records the declaration's sha256. **An agent without a store holds
no state across loads, its prompt included**: with no member there is nothing to ask at
the load and nothing carries the seeding to the next one, so a storeless agent with a
seeding sender is seeded again in every residency, as its first turn, and one without a
sender serves under the compiled loop's prompt.

## 4. Validate and prove the load

```sh
sudo WEAVER_ADMIN_CONFIG=/etc/weaver/admin /opt/weaver/bin/weaver-admin validate <name>
sudo "$PWD"/deploy/verify-load.sh <name>
```

**Every load runs in the agent's own scope** (`weaver-admin-Spec` section 6.1): admin's
start step asks PID1 for the transient scope `weaver-<name>.scope` in `weaver.slice`,
moves itself into it, confirms the move by its own reading, and makes the agent's cgroup
beneath it. So a load needs no wrapper: a direct load from root's shell, a script that
loads, and a load through the agent's sudo rule from admin-con's own unit stand the
agent in the same place. The load refuses `boundary_unverified` where PID1 does not move
it within ten seconds, or where the scope carries no delegate mark, or where it, a file
delegated with it, or any cgroup above it is not root's (#166's guard: no account but
root may be able to move the agent's processes out of its cgroup). A scope still holding
an ended run's leftovers refuses `agent_running`, and `unload` ends them. The host needs
systemd's `busctl` at `/usr/bin/busctl`.

`validate` answers `{"kind":"validated"}` or a refusal naming the field or the boundary
piece. `verify-load.sh` validates, first unloads the agent (taking its leave save
point where a run stands), then loads, counts the events that arrive in the sink,
prints their kinds and the load event, and prints the binding's base and adapter as
declared (`none` for a base-only agent) beside the adapter member of any Open
measurement the new events carry. The live record does not carry the Open measurement
yet, and the script says so. It then reads `show`'s constituents and checks each:
it runs as the agent's, the member's or the relay's account, it sits in the agent's
cgroup beneath the agent's scope, `weaver.slice/weaver-<name>.scope/weaver-<name>/` (the
member and the relay in `constituents/`, the worker in `worker/run/`), and a relay
stands among them for a file sink. It then unloads, and
checks that every constituent is gone, the agent's cgroup is gone, and `show` reads
unloaded. A healthy first load writes `load`,
plus `recall` where a store member stands and the agent has been seeded; a load writes
no `message.system`, the prompt being on the trace only at its seeding turn. The load event's
payload names the session, the run, the store, the declaration's hash, the composer
(`worker`), and the `stack` hashes of the installed members. A load that answers `idle`
and writes nothing is the failure this step exists to catch, and the worker's own words
are in `<territory>/worker.log`, the state member's in `<territory>/state/state.log`.

**Then it prints the agent's deployment tuple**, through `deploy/agent_tuple.py` beside
it, before the constituents are read. The rows are those of the tuple survey of
2026-11-08 (1, 2, 3, 6, 7, 8 and 9), with the state engine and the restore rows.
Each line ends in one status word: `recorded`, `derivable`, `absent` or `FAULT`.
- **Row 1, the artifact and its weights hash.** The hash is admission's BLAKE3, which no
  tool on the box computes. For an adapted agent it is the declared `base-blake3`, since
  admission refuses a base that differs. For a base-only agent it is the sink's latest
  `model.measurement` naming the path admission resolved (the declared file, or the
  container a declared directory resolves to), and `absent` where the sink holds none:
  the load itself records no hash.
- **Rows 2 and 3, the family, the chat template and the formats**, from the artifact's
  GGUF header. The header is read as the agent's own account, the one admission reads it
  as, and never as root. A declared directory is first resolved to its one container by
  admission's rule (a single `.gguf` or `.safetensors` file, or one split's first
  shard), and row 1 names that container.
- **Row 7, the devices.** Each declared ordinal names its card in the install tuple's
  PCI-order list. That order is the one the SPU sees, under PAIR-1's
  `CUDA_DEVICE_ORDER=PCI_BUS_ID` mask. The live cards are read with
  `nvidia-smi --query-gpu=pci.bus_id,name,memory.total --format=csv,noheader`, put in
  bus order. A difference from the install tuple's list in name, memory, count or order
  is a `FAULT`: the install tuple is stale.
- **Rows 8 and 9, the placement and the offload**, for a GGUF container: PAIR-3's even
  split of the header's layer count over the declared devices, and every layer
  offloaded. A container without the GGUF magic is served by the native path, and both
  rows read `absent`, as they do where no container kind is known.
- **The restore rows.** The declaration's `[restore]`, or none declared, and what the
  load restored, from the load event's `lineage`. With no `[restore]` a reload restores
  the manifest's latest save point. A declared save point the lineage does not name, by
  digest or by published name, or a declared one with no lineage, is a `FAULT`.

- **The box side and the binding.** The box side is the stack record's `install-tuple`,
  written by each install. Its own sha256 is printed as the box's identity, the one value
  a report cites for it. The record is then bound to the box:
  - The declaration's sha256 against the load event's.
  - The load event's `stack` digests against the install tuple's members, by name, with
    a name only one side carries printed as not bound.
  - Each engine library in the load event's library path against the install tuple's
    digest, hashed on the spot.

  A digest the install tuple reads `unknown` (the install could not hash that file) is
  printed as not bound, never as a `FAULT`.

A `FAULT` line unloads the run and fails the script, `--keep` included, like every other
failed check.

`deploy/verify-lifecycle.sh --agent <throwaway>`, run as the operator, verifies a whole
lifecycle end to end
(create with the box's accounts, load, an unseeded turn through the gate as gate-con,
save points, restore, a measurement with the throwaway agent's rule moved to the
operator's uid and back, in which a turn as gate-con is refused and an ordinary turn as
the operator is answered (no seeding line is sent), force-unload, crash recovery, the
refusal by name of a root of an older layout, the refusal of a load over an access
entry on the trace, take-down) on a throwaway agent it creates; it plans
unless given `--apply`, and `--keep` stops it before the take-down so the checks it
cannot make are made against the loaded agent.

`load` never ends a run that stands: it answers `agent_running` and leaves the decision
to its caller, who reads `show` and issues `unload`.

**A turn through the gate runs as the box's gate-con account**, the one caller the rule
names, with the agent loaded:

```sh
sudo -n -u weaver-gatecon python3 -I - <name> "<text>" < deploy/turn.py
```

The script is read from standard input, so `weaver-gatecon` needs no passage to the
checkout. It reads the agent's root under `/etc/weaver/admin`, dials the gate's socket
by the agent's group, and prints the model's answer.

**An agent `create-agent.sh` lays has no seeding sender** (`weaver-gate-world-contract`
section 2, the operator's ruling of 2026-11-10). The harness takes a seeding line from
the operator's uid alone, and the gate refuses every uid its rule does not name before a
byte is read, so with the rule naming gate-con alone no seeding line reaches the harness
from anyone. How such an agent's prompt enters is deferred to its own act. It serves
under the compiled loop's prompt until then, and its draft is a draft. What follows
holds for an agent whose rule names the operator, as an earlier `create-agent.sh` laid
it: such an agent keeps that layout and is not refused for it.

**Then seed the prompt**, the last step, as yourself and with the agent loaded (so
`verify-load.sh <name> --keep`, or a plain `load`, as above):

```sh
deploy/turn.py <name> --system
```

This reads `<territory>/system-prompt.md`, the root's `territory` key naming the
directory, the state group passing you through the territory and the access group
letting you read the file, and sends it through the gate
as the one line `{"role": "system", "text": ...}`. The harness takes a system line from
the operator's uid alone, the one `allowed-uids` names and admin's `operator` key
records: it writes the prompt into the record as the prefix the next load seats, the
tee carries it to the state member, and the model answers it as a turn, so what prints
is the model's first answer under its prompt and the seeding has exercised the whole
loop (gate, harness, model, trace, tee, state). The prompt is state from here: every later
load asks the member for it and seats what the member holds, and the draft on disk is
only what the next seeding would send. Seeding again in a later residency replaces the
prompt, and seeding twice in one residency seats both, in order.

**The seeding line has a bound: 49,041 octets before the delimiter**, the gate's line
bound (`weaver-gate-Spec` section 4), measured on the JSON line `turn.py` writes. The
line's wrapper takes 30 octets, so a draft of plain printable ASCII may run to 49,011
octets. Each newline, carriage return, tab, backspace, form feed, quote and backslash in
the draft counts two octets, any other character below U+0020 and DEL six, and each
non-ASCII character six (twelve above U+FFFF). `turn.py` measures the line before it
dials and refuses an oversize draft with its size; the gate itself would close the
connection unanswered. The bound is an interim, on the operator's ruling of 2026-11-08:
seeding moves off the gate to admin's seed verb at the state-management foundation
cutover.

**An agent without a store (section 3) is seeded in every residency instead**, as its
first turn: with no member, nothing holds the prompt across loads, the load seats
nothing, and a save point and unload would carry nothing, so the seeding is the first
turn of each residency and the agent is otherwise unbounded. That is the storeless
election's meaning and not a fault.

**Create, load, seed, save-point, unload; production is the next load.** Within the
seeding residency the prompt is appended context, not the decode session's prefix, so a
flush there would drop it from the context. **Under `turn-context = "independent"` every
turn is preceded by such a flush**, so a prompt seeded within a residency is in force
for the seeding turn alone: it enters state as every seeding does and the next load
seats it, but `turn.py --system` seeds under whatever the declaration elects, so seed a
new agent under `continuous` or reload it before it serves. A storeless agent under
`independent` has no seeded prompt in force past its seeding turn. Take the save point
on demand once the seeding has answered, as root or through the agent's sudo rule:

```sh
sudo /opt/weaver/bin/weaver-admin save-point <name>   # answers {"kind":"save_point_taken",...}
sudo /opt/weaver/bin/weaver-admin unload <name>
```

The first save point taken after the seeding is the agent's starting state. It holds the
prompt whole under every election, and the model's first answer as the declaration's
election admits it (`weaver-trace-Spec` section 11): its text under the
`[state-election]` `create-agent.sh` writes, which names `message.assistant`'s
`content`, its envelope without the text under an election that admits the kind and
names no `content` path for it, the default election among them, and nothing under one
with `all-kinds = false` that does not name the kind. `save-point` publishes it at once into
`<territory>/save-points/` as `<YYYYMMDDTHHMMSSZ>-<digest>.save-point`, named on one
line of `save-points.manifest` there, which is root's and which you do not edit
(section 5). The unload takes one more at the leave, and every
later load restores the latest the manifest names. A healthy seeding leaves a turnless
`message.system` in the sink followed by the turn's bracket with no `message.user` in
it. A seeding line from any other uid, or a frame the gate carried without its
dialer, answers `{"kind":"refused", ...}` naming the role, and the agent stands.

**A seeded agent's production turns carry no loop prompt.** The compiled worker loop's
`SYSTEM_PROMPT` (`crates/weaver-harness/src/bin/worker/dev_loop/mod.rs`) yields to a
seated one, per ADP-4 and section 6 of
`docs/crates/weaver-harness/Loops/basic-inference-loop.md`: where the member's identity
answer seats a prefix at the load, the loop pushes no compiled prompt and no continuity
line, on the run's first turn or at the re-entry after a flush, and the re-entry stays a
system message, the recall text alone. The compiled prompt rides only where nothing is
seated: it leads a run's first turn where the loop drives that turn, with the continuity
line beneath it only where the member's session shape reports an earlier run (none on a
session's first run, none where the shape is unavailable), and opens each re-entry
without the line. In a seeded agent's trace the prompt appears only as the turnless
`message.system` at the seeding, and no production turn carries the loop's prompt. A
seeding is appended context in its own residency, so a flush there drops it (#502): the
re-entry opens with the compiled prompt after a first seeding, and keeps the prefix the
load seated after a later one. The sequence above (load, seed, save point, unload)
avoids that, and an agent without a store, seeded in every residency, cannot.

## 5. Serving, stopping, unloading, and the agent's sudo rule

```sh
sudo /opt/weaver/bin/weaver-admin load <name>         # answers {"kind":"state","state":"idle"}
sudo /opt/weaver/bin/weaver-admin show <name>         # the state and the run's constituent pids
sudo /opt/weaver/bin/weaver-admin save-point <name>   # a save point now, published at once
sudo /opt/weaver/bin/weaver-admin unload <name>       # answers {"kind":"state","state":"unloaded"}
sudo /opt/weaver/bin/weaver-admin restore <name>      # make [restore]'s save point the latest, which the next load restores
sudo /opt/weaver/bin/weaver-admin force-unload <name> # stop the work at once, still taking the leave's save point
```

**The save points and the manifest.** Every unload takes a save point at the leave and
`save-point` takes one on demand; admin publishes each into `<territory>/save-points/`
under a name computed from its own bytes, `<YYYYMMDDTHHMMSSZ>-<digest>.save-point`,
root:weaver-<name>-admin 0640, and names it on one line of `save-points.manifest`
there, root 0644: the ordinal, the digest, the position it covers, where the trace
names it, and how it arrived. You read both through the access group and write
neither. The next load restores the latest line of the manifest, the highest ordinal;
where its file is gone or does not digest to its line, the load refuses rather than
restore an older one, and `restore` names another. A file the manifest does not name is not loadable. To start
from another state, put the file in the directory as root, root:weaver-<name>-admin
0640, its name the one its bytes compute, since `restore` refuses any other owner, group
or mode:

```sh
sudo install -o root -g weaver-<name>-admin -m 0640 <file> <territory>/save-points/<save point name>
```

Then name it in `agent.toml` as `[restore] save-point = "<name or digest>"` with
`sudoedit`, run `restore <name>`, which judges it and makes it the manifest's latest,
then `load`. A save point the manifest already names is restored the same way, without
the install. The manifest is root's; the one way a file enters it is the `restore`
verb, and removing the manifest makes every published save point unloadable until a
restore names one again.

**`[restore]` is honoured only while the save point it names is the manifest's latest**
(the operator's ruling of 2026-10-08 on #99), which the `restore` verb makes it. Once a
later unload or `save-point` publishes a newer one, a load with `[restore]` still in the
declaration refuses `config_invalid` naming `restore`, and admin's standard error names
both save points, rather than restore the older state over the newer. Remove `[restore]` with
`sudoedit` to continue from the latest, or run `restore <name>` again to continue from
the named one.

**An unload that cannot take its save point does not complete**, where the unload
stayed graceful. It answers
`{"kind":"save_point_not_taken","missed":...}` naming the leg that missed. Before the
`published` leg the run stays loaded with its lock, its gate lowered, and nothing is lost:
retry `unload`, which goes straight to the save point, if the member is alive, or `force-unload` if it is dead, which attempts the save point as
any unload does and, where it cannot be taken, records that on the trace; the next load
then restores the latest published save point with the reset recorded. The loss is your recorded choice. Where
`missed` is `published`, the run has already ended and the save point waits in the
member's room: retry `unload`, which publishes it first, and read `admin.log` if it
refuses again. An unload that turned forced, by the one definition of `forced` in `weaver-admin-Spec`
section 3 (joined by a `force-unload`, past a declared `[lifecycle]` bound, or with the
gate or SPU dead), does not stop there: it comes down without the save point,
the marker stands `Forced`, and the next load restores the latest published save point
and records `ForcedUnload`.

Or `sudo "$PWD"/deploy/verify-load.sh <name> --keep` to load with the read-back and leave
it serving. No unit file and no service is involved. The worker, the state member and
the relay are processes admin started, detached from the invoking terminal, holding the
run lock between them, and they live in the agent's cgroup, which admin makes beneath
the agent's own scope (`weaver-admin-Spec` section 6.1). So logging out, or the end of
whatever invoked the load, does not end the agent, and root stopping the scope
(`sudo systemctl stop weaver-<name>.scope`) does, an unclean stop whose next load resets
to the latest save point. `unload` kills the agent's cgroup whole before it answers
`unloaded`, so no process of the run, a tool's command that left its session included,
outlives it. `unload` asks the agent to leave,
letting it finish the turn in flight and write a wind-down summary before its save
point, which has no time limit unless the declaration's `[lifecycle]` table sets a
`drain-bound` and a `wind-down-bound`; with both set it ends the run within their sum
plus the forced unload's bound, which `weaver-admin-Spec` section 3 states. `force-unload`
stops the work at once and still saves the state, ending the run within that bound; issued while an `unload` is
in progress it joins it, waiting out one already finishing, and
beside any other command it refuses, reading which command holds the agent from the
verb that command records in `admin.lock`, one command at a time being yours to keep. Either then publishes the leave's save point, which takes as long as copying it does. Admin's
own acts on this agent are in `<territory>/admin.log`, and the worker's output in
`worker.log` beside it, both root's and yours to read through the access group.

**The agent's sudo rule** is `/etc/sudoers.d/weaver-<name>`, which create-agent writes
for the box's admin-con account:

```text
Defaults:weaver-admincon !pam_session
weaver-admincon ALL=(root) NOPASSWD: /opt/weaver/bin/weaver-admin show <name>, /opt/weaver/bin/weaver-admin validate <name>, /opt/weaver/bin/weaver-admin load <name>, /opt/weaver/bin/weaver-admin unload <name>, /opt/weaver/bin/weaver-admin stop <name>, /opt/weaver/bin/weaver-admin save-point <name>, /opt/weaver/bin/weaver-admin restore <name>, /opt/weaver/bin/weaver-admin force-unload <name>
```

The observer role's rule grants the `show` line alone. Each grant is one fixed command
line, so admin-con can pass no argument of its own, and it runs them with `sudo -n`.
**The `load` line stands as an operator's load does**: admin's start step moves it out
of admin-con's unit into the agent's own scope (`weaver-admin-Spec` section 6.1), so
admin-con's restart does not end the agent. The other lines start no process.
`!pam_session` keeps sudo from opening a session for these lines. Since #166 a session
could not break the agent's lifetime, the start step leaving any containment, and it
stays because none is needed and the rule does not change. `sudo -n -l -U
weaver-admincon` prints what every agent's rule grants it. One account holds every
agent's rule, by design (`weaver-admin-Spec` section 2). An agent laid by an earlier
`create-agent.sh` keeps a rule naming its own `weaver-<name>-admincon` and is not
refused for it. The connectors themselves, gate-con and admin-con, their services and
their containment, are WeaverWeb's packaging, and this repository provisions only their
two accounts, their places in each agent's groups, the rule and the place in
`roles.toml`.

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
Remove its sudo rule `/etc/sudoers.d/weaver-<name>` first, so admin-con can run nothing
more on it, then its root `/etc/weaver/admin/<name>/`, which ends its admission, and its
run and runtime directories, `<coordination-root>/weaver.run/<name>/` and
`<coordination-root>/weaver-<name>/`, the second holding the gate's and the harness's
sockets, which `decommission.sh` purges beside the first (both on tmpfs, so a reboot
also clears them). Archive the territory and remove it: it holds the declaration, the
prompt draft, the two logs and the published save points beside the state room and the
trace, the one record of what was done to the agent, and create-agent refuses a
territory that stands, so the archive comes first. Remove the agent's `/tmp` entries
too, while its accounts still resolve, the paths going before the accounts as in the
purge's order (`deploy/REDEPLOY.md` section 2): once they are gone, an entry's owner
reads only as a number. They are the entries directly under `/tmp` named `weaver-*` or
`torchinductor_weaver-*` and owned by one of the agent's accounts, never chosen by name
alone, since `/tmp` is anyone's to write and another agent's name can begin with this
one's. **The box's two accounts stay**: they serve every other agent, and removing the
agent's groups below takes them out of the agent's groups. `weaver-<name>-admincon` is
listed for an agent an earlier `create-agent.sh` laid with one:

```sh
for u in weaver-<name> weaver-<name>-state weaver-<name>-relay weaver-<name>-admincon; do
  getent passwd "$u" >/dev/null || continue
  sudo find /tmp -mindepth 1 -maxdepth 1 \( -name 'weaver-*' -o -name 'torchinductor_weaver-*' \) \
    -user "$u" -exec rm -rf -- {} +
done
```

Then `userdel -r` the agent's account and `userdel` the member's, the relay's and,
where it stands, the per-agent connector's, then delete the groups, which `userdel`
leaves while a member remains and which a later `useradd --user-group` of the same
name would refuse on.

The groups, once the accounts are gone:

```sh
for g in weaver-<name> weaver-<name>-state weaver-<name>-trace weaver-<name>-admin weaver-<name>-admincon; do
  getent group "$g" >/dev/null && sudo groupdel "$g"
done
```

**An agent of an older layout or an older declaration form is not migrated.** Admin and
`update-stack.sh` refuse it by name, and it is recreated with `create-agent.sh` after
its take-down by this section; nothing migrates it (the operator's ruling of 2026-10-08
on #1).
