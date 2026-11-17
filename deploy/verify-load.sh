#!/usr/bin/env bash
# Load one agent, read its load event back out of its own sink, check its
# constituents, and unload it.
#
#   sudo deploy/verify-load.sh <agent>            load, read, check, unload
#   sudo deploy/verify-load.sh <agent> --keep     load, read, check, leave it serving
#
# The verify step of update-stack.sh, standing alone so a fresh install and a
# single agent can be checked without a rebuild. Everything it reads is the
# agent's own root, `<admin base>/<agent>/` (WEAVER_ADMIN_CONFIG names the
# base, default /etc/weaver/admin), and the territory that root names, as admin
# reads them: the declaration `agent.toml` there. `weaver-admin`
# itself is taken from the stack record's `prefix` (WEAVER_STACK_RECORD,
# default /etc/weaver/stack), never from a key of the agent's root, which admin
# has not yet judged when this runs it as root. It reads the sink path out of
# the declaration through tomllib, so what it counts is the file admin opened,
# and it judges the load by events arriving in that file rather than by
# admin's exit status: a load that answers `loaded` and writes nothing is the
# failure this exists to catch.
#
# **The constituents are checked as an invoker must check them** (the operator
# contract section 2, toddwbucy/WeaverWeb#15): every pid `show` names after
# the load runs as the agent's, its member's or its relay's account and sits
# in the agent's cgroup beneath the agent's own scope,
# `/weaver.slice/weaver-<agent>.scope/weaver-<agent>/`, which admin's start
# step asks PID1 for (weaver-admin-Spec 6.1): the member and the relay in its
# `constituents/`, the worker in its `worker/run/`. A relay stands among them
# for a file sink, and after the unload none of them is left, the agent's
# cgroup is gone, and `show` reads unloaded with no constituent. The script
# runs from root's shell, in no scope of its own:
# `sudo "$PWD"/deploy/verify-load.sh <agent>`.
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

# **held_closed PATH: admin's rule for what a root process may trust**, per
# weaver-admin-Spec section 9 (`judge_ancestors`). PATH, resolved to its
# canonical path, and every directory above it up to `/`, must be owned by uid
# 0 and writable by no group or other, unless it is a sticky directory, which
# keeps another principal from renaming an entry it does not own. Fails
# printing the first component that is not so, or the path where it does not
# resolve. Every deploy script carrying it carries this same text.
held_closed() {
  local at owner mode
  at=$(realpath -e -- "$1" 2>/dev/null) || { printf '%s' "$1"; return 1; }
  while :; do
    read -r owner mode < <(stat -c '%u %a' -- "$at" 2>/dev/null) || { printf '%s' "$at"; return 1; }
    if [ "$owner" != 0 ] || { (( 8#$mode & 8#022 )) && ! { [ -d "$at" ] && (( 8#$mode & 8#1000 )); }; }; then
      printf '%s' "$at"; return 1
    fi
    [ "$at" = / ] && return 0
    at=$(dirname -- "$at")
  done
}

[ "$(id -u)" -eq 0 ] || die "run under sudo: admin's verbs are root's"
# **Every drop to the agent's accounts is runuser's** (#590): this script is
# root's, so root's own sudo is never asked, and nothing of root's
# environment reaches the account (account-drop.sh). Each read as an account
# discards its own refusal, so a box without util-linux's runuser and setsid
# is refused here, by name, before anything loads.
ROOT_OPERATOR=1
source "$(dirname -- "$(realpath -e -- "${BASH_SOURCE[0]}")")/account-drop.sh"
command -v runuser >/dev/null && command -v setsid >/dev/null \
  || die "runuser or setsid is not on PATH, and the reads as the agent's accounts go through them: install util-linux's"
AGENT=${1:-}; [ -n "$AGENT" ] || die "name the agent"
KEEP=0; [ "${2:-}" = --keep ] && KEEP=1

# The name as admin's own check admits it, so `..` or a slash cannot walk the
# path below out of the base.
[[ "$AGENT" =~ ^[A-Za-z0-9_-]+$ ]] || die "'$AGENT' is not an agent name: ASCII letters, digits, - and _"
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
ROOT="$ADMIN_BASE/$AGENT"
[ -d "$ROOT" ] || die "no agent root at $ROOT"
# **The program run as root comes from the record and is judged first.** It was
# `$(dirname worker-binary)/weaver-admin`, a key of an agent root admin had not
# yet judged, so a root, key or directory another principal could write chose
# what this script ran as root (Codex on #45). The record's `prefix` names the
# install. The record entry and the binary must each be held closed by admin's
# rule, `held_closed`, before the binary runs.
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
bad=$(held_closed "$STACK/prefix") || die "the stack record's prefix is not held closed by root: $bad"
# Trimmed at its ends only: a prefix may hold an interior space, and the record
# keeps it as written (Codex on #45).
PREFIX=$(< "$STACK/prefix")
PREFIX=${PREFIX#"${PREFIX%%[![:space:]]*}"}; PREFIX=${PREFIX%"${PREFIX##*[![:space:]]}"}
[ -n "$PREFIX" ] || die "the stack record's prefix is empty"
# Run by the canonical path that is judged, so no link on the written path can be
# re-pointed between the judgment and the exec.
ADMIN=$(realpath -e -- "$PREFIX/bin/weaver-admin" 2>/dev/null) || die "no weaver-admin at $PREFIX/bin/weaver-admin"
[ -f "$ADMIN" ] && [ -x "$ADMIN" ] || die "no weaver-admin at $ADMIN"
bad=$(held_closed "$ADMIN") || die "weaver-admin at $ADMIN is not held closed by root: $bad"
admin() { WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$ADMIN" "$@" 2>&1 || true; }
# **answered_state ANSWER STATE: admin's answer is the state named**, read as
# JSON and never matched as text, as update-stack.sh's reader of the same
# name (the #94 survey's S22, #99 area 2 review K8).
answered_state() {
  python3 -c 'import json, sys
try:
    d = json.loads(sys.argv[1])
except ValueError:
    sys.exit(1)
sys.exit(0 if isinstance(d, dict) and d.get("kind") == "state" and d.get("state") == sys.argv[2] else 1)' "$1" "$2"
}

# **Admin judges the root before this script reads anything in it.** It read
# `agent.toml` and then counted the sink it names, as root, before admin had
# judged the root, so a writable root or declaration could name a FIFO or a
# device as the sink (Codex on #45). `validate` applies admin's root, ancestor
# and entry judgments (Spec section 9), so the root is read only once admin
# answers `validated`, and through admin's judgment rather than a copy of it.
VERDICT=$(admin validate "$AGENT" | tail -1)
[ "$VERDICT" = '{"kind":"validated"}' ] || die "admin does not validate $AGENT, so its root is not read: $VERDICT"
# The territory is the root's key, judged by the validate above, and the
# declaration stands in it (the operator's ruling of 2026-10-07 on #1).
TERRITORY=$(< "$ROOT/territory")
TERRITORY=${TERRITORY#"${TERRITORY%%[![:space:]]*}"}; TERRITORY=${TERRITORY%"${TERRITORY##*[![:space:]]}"}
DECL="$TERRITORY/agent.toml"; [ -f "$DECL" ] || die "no declaration at $DECL"

SINK=$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["trace-sink"]["path"])' "$DECL") \
  || die "the declaration names no trace-sink.path"
SINK_KIND=$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["trace-sink"].get("kind",""))' "$DECL" 2>/dev/null || true)
# **Only a regular file is counted** (#99 area 2 review, K6): a pipe or a
# socket sink is not a file a line count reads, and root opening a FIFO with
# no writer blocks for ever, so a non-file sink is never opened here and its
# load is judged by admin's answer and the constituents alone. A link or a
# FIFO standing where a file sink is declared counts as nothing.
lines() { if [ -f "$1" ] && [ ! -L "$1" ]; then wc -l < "$1"; else echo 0; fi; }

# **The adapter beside the base**, per the weaver-spu adapter binding
# amendment ADP-5: the binding as declared, `none` where the table is
# omitted. What the load measured is read from the events below.
# Admin validated the declaration above, so a binding it lacks is printed as
# absent rather than refused here.
BINDING=$(python3 -c 'import sys, tomllib
d = tomllib.load(open(sys.argv[1], "rb"))
b = d.get("spu-instruction", {}).get("decoder", {}).get("model-binding", {})
a = b.get("adapter")
print(b.get("artifact", "<no binding declared>"))
print("none" if a is None else "{} sha256 {} scale {} over base blake3 {}".format(
    a["artifact"], a["sha256"], a["scale"], a["base-blake3"]))' "$DECL" 2>/dev/null) \
  || BINDING=$'<unread>\n<unread>'
say "$AGENT"
plan "declaration $DECL"
plan "sink        $SINK ($SINK_KIND)"
plan "base        $(sed -n 1p <<<"$BINDING") (declared)"
plan "adapter     $(sed -n 2p <<<"$BINDING") (declared)"
plan "validate    $VERDICT"
# **The state member's last words**, beside a load that did not stand. Read as
# the member, whose room it is, never as root: the member can put a link at
# that name, and root's read would follow it anywhere. Admin creates the log
# root:<the member's group> 0640 so the member reads it (#99 area 2 review,
# K12); a read the member is refused prints nothing.
member_said() {
  local st said
  st="$(dirname "$SINK")/state/state.log"
  said=$(as_account "weaver-$AGENT-state" tail -n 3 "$st" 2>/dev/null || true)
  [ -n "$said" ] && { plan "the state member last said:"; printf '%s\n' "$said" | sed 's/^/     /'; }
  return 0
}
# **The first unload is read from its answer** (#99 area 2 review, K8, the
# sibling of S22): a refused unload leaves a run standing, whose state and
# constituents the checks below would then read as this load's. The count is
# taken after it, so the unload's own events are never this load's.
SAID=$(admin unload "$AGENT" | tail -1)
answered_state "$SAID" unloaded || die "$AGENT: the unload before the load answered ${SAID:-nothing}, so a load now would not be this script's"
BEFORE=0
[ "$SINK_KIND" != file ] || BEFORE=$(lines "$SINK")
LOADED=$(admin load "$AGENT" | tail -1)
plan "load        $LOADED"
# **And the load's** (N5's class): only the idle state is a load that stands.
if ! answered_state "$LOADED" idle; then
  member_said
  admin unload "$AGENT" >/dev/null
  die "$AGENT: the load answered ${LOADED:-nothing}, not the idle state"
fi
if [ "$SINK_KIND" != file ]; then
  plan "events      not counted: a ${SINK_KIND:-sink of no kind} is not a file a line count reads, so the load is judged by its answer and its constituents"
else
AFTER=$(lines "$SINK")
NEW=$((AFTER - BEFORE))
if [ "$NEW" -le 0 ]; then
  member_said
  admin unload "$AGENT" >/dev/null
  die "the load wrote no events to $SINK"
fi
plan "events      $NEW new lines"
# **The load event is `load` exactly** (K8): a match on the word let the
# unload's own `unload` stand for a load.
tail -n "$NEW" "$SINK" | python3 -c '
import sys, json, collections
kinds = collections.Counter()
load = None
events = []
for line in sys.stdin:
    try: ev = json.loads(line)
    except ValueError: kinds["<unparsed>"] += 1; continue
    events.append(ev)
    k = ev.get("kind") or ev.get("event") or ev.get("type") or "?"
    kinds[k] += 1
    if load is None and k == "load": load = ev
for k, n in kinds.most_common(): print(f"   {n:4d}  {k}")
if load is None: sys.exit("   no load event among them")
keep = {k: v for k, v in load.items() if k not in ("content",)}
# The line ends at a member boundary, after the payload stack map, and names
# the members after it, which the sink holds whole. The members before the
# stack are the elections the declaration makes and the lineage the load
# restored, 1,118 characters through the stack on the live load of
# 2026-11-09, and the stack is what verify-lifecycle reads from this line and
# the tuple below binds. The line is for reading, not a bound: a declaration
# electing a long tee prints a long line, and the sink holds the event. A cut
# at 600 characters fell inside the declaration digest on the live load of
# 2026-11-09, and never reaches the stack once a lineage stands. A load with
# no stack map prints whole.
payload = keep.get("payload")
left = []
if isinstance(payload, dict) and "stack" in payload:
    names = list(payload)
    through = names.index("stack") + 1
    keep["payload"] = {k: payload[k] for k in names[:through]}
    left = names[through:]
print("   load event:", json.dumps(keep))
if left:
    print("   load event cut after its stack map;", ", ".join(left), "is" if len(left) == 1 else "are", "in the sink")
# The Open measurement, wherever an event carries it (ADP-3): its adapter
# member beside the base identity it measured.
def opens(value):
    if isinstance(value, dict):
        evidence = value.get("open_evidence")
        if isinstance(evidence, dict) and isinstance(evidence.get("measurement"), dict):
            yield evidence["measurement"]
        for inner in value.values():
            yield from opens(inner)
    elif isinstance(value, list):
        for inner in value:
            yield from opens(inner)
found = [m for ev in events for m in opens(ev)]
for m in found[:1]:
    print("   open measurement: model", m.get("model"), "weights_hash", m.get("weights_hash"),
          "adapter", json.dumps(m.get("adapter", "<absent>")))
if not found:
    print("   open measurement: none in the new events (the live record does not carry it), so the adapter above is the declared one")
' || { admin unload "$AGENT" >/dev/null; die "$AGENT: the new events carry no load event, so the run is unloaded"; }
fi
# **The agent's deployment tuple**, the rows of the tuple survey of
# 2026-11-08 (1, 2, 3, 6, 7, 8 and 9) with the state engine and the restore
# save point, printed by `agent_tuple.py` beside this script. Each line ends
# in recorded, derivable, absent or FAULT, and a FAULT unloads the run as any
# failed check here does. Read-only. **The artifact's header is parsed as
# the agent's own account**, the one admission reads it as, never as root;
# the helper's source goes in on standard input because that account cannot
# reach the checkout. The sink is read as root only where it is a regular
# file, as the count above reads it.
TUPLE_HELPER="$(dirname -- "$(realpath -e -- "${BASH_SOURCE[0]}")")/agent_tuple.py"
[ -f "$TUPLE_HELPER" ] || { admin unload "$AGENT" >/dev/null; die "$AGENT: no tuple helper at $TUPLE_HELPER, so the run is unloaded"; }
TUPLE_HEADER=$(as_account --stdin "$TUPLE_HELPER" "weaver-$AGENT" python3 -I - header "$(sed -n 1p <<<"$BINDING")" \
  2>/dev/null || true)
[ -n "$TUPLE_HEADER" ] || TUPLE_HEADER='{"refused": "the agent account ran no header read"}'
TUPLE_ARGS=(--declaration "$DECL" --header-json "$TUPLE_HEADER"
  --live-gpus-csv "$(nvidia-smi --query-gpu=pci.bus_id,name,memory.total --format=csv,noheader 2>/dev/null || true)")
if [ "$SINK_KIND" = file ] && [ -f "$SINK" ] && [ ! -L "$SINK" ]; then
  TUPLE_ARGS+=(--sink "$SINK" --from-line "$((BEFORE + 1))")
fi
# **The hardware half, read at this load and never stored** (the operator's
# rulings of 2026-11-14: one image serves any host's cards). The cards are
# the query above. The driver is nvidia-smi's version, and its library and
# the CUDA runtime files are what the loader resolves for this agent's SPU
# now, with the library path admin hands the worker: the config root's
# `spu-binary` and `library-path` keys, which admin's `validate` above
# judged, and none of the operator's loader variables, as spu_ldd reads
# them at an install (deploy/engine-libraries.sh). Each path is resolved
# once, and the resolved path is both judged and used. Root runs the loader
# only on an SPU, and with a library directory, that admin's rule holds
# closed. Where it cannot, the reason goes to the helper as
# `--live-ldd-unread`, and a runtime file the install recorded is then a
# FAULT, never passed unbound: admin refuses such an SPU at load, so a
# loaded agent showing one is not what was installed.
TUPLE_ARGS+=("--live-driver=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null || true)")
trimmed() { local v; v=$(< "$1") || return 1; v=${v#"${v%%[![:space:]]*}"}; printf '%s' "${v%"${v##*[![:space:]]}"}"; }
LDD_ENV=(env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT -u GLIBC_TUNABLES)
LIVE_UNREAD=""
AGENT_LIBRARY_REAL=""
if ! SPU_PATH=$(trimmed "$ROOT/spu-binary" 2>/dev/null) || [ -z "$SPU_PATH" ]; then
  LIVE_UNREAD="the agent's config root names no spu-binary"
elif ! SPU_REAL=$(realpath -e -- "$SPU_PATH" 2>/dev/null); then
  LIVE_UNREAD="no SPU at $SPU_PATH"
elif ! bad=$(held_closed "$SPU_REAL"); then
  LIVE_UNREAD="the SPU at $SPU_REAL is not held closed by root at $bad"
elif [ -e "$ROOT/library-path" ] || [ -L "$ROOT/library-path" ]; then
  if ! AGENT_LIBRARY=$(trimmed "$ROOT/library-path" 2>/dev/null) || [ -z "$AGENT_LIBRARY" ]; then
    LIVE_UNREAD="the agent's library-path does not read"
  elif ! AGENT_LIBRARY_REAL=$(realpath -e -- "$AGENT_LIBRARY" 2>/dev/null); then
    LIVE_UNREAD="no library directory at $AGENT_LIBRARY"
  elif ! bad=$(held_closed "$AGENT_LIBRARY_REAL"); then
    LIVE_UNREAD="the agent's library path $AGENT_LIBRARY_REAL is not held closed by root at $bad"
  else
    LDD_ENV+=(LD_LIBRARY_PATH="$AGENT_LIBRARY_REAL")
  fi
fi
if [ -z "$LIVE_UNREAD" ]; then
  LIVE_LDD=$("${LDD_ENV[@]}" ldd "$SPU_REAL" 2>&1) || LIVE_UNREAD="ldd could not read $SPU_REAL: $(tail -n 1 <<<"$LIVE_LDD")"
fi
if [ -n "$LIVE_UNREAD" ]; then
  TUPLE_ARGS+=("--live-ldd-unread=$LIVE_UNREAD")
else
  TUPLE_ARGS+=("--live-ldd=$LIVE_LDD")
  # **Like for like**: the install read the stack record's SPU with the
  # record's library path. An agent made with `create-agent.sh --spu`, or
  # whose root names another library path, runs something the install did
  # not read, so its runtime files are not bound and the rows say which.
  # The record's keys are trusted only where admin's rule holds them closed,
  # and an untrusted one is a reason the loader's read cannot be bound.
  STACK_SPU_REAL="" STACK_LIBRARY_REAL=""
  if ! bad=$(held_closed "$STACK/spu-binary") || ! STACK_SPU_REAL=$(realpath -e -- "$(trimmed "$STACK/spu-binary")" 2>/dev/null); then
    TUPLE_ARGS+=("--live-ldd-unread=the stack record's spu-binary is not held closed by root or names no file")
  elif { [ -e "$STACK/library-path" ] || [ -L "$STACK/library-path" ]; } \
    && { ! bad=$(held_closed "$STACK/library-path") || ! STACK_LIBRARY_REAL=$(realpath -e -- "$(trimmed "$STACK/library-path")" 2>/dev/null); }; then
    TUPLE_ARGS+=("--live-ldd-unread=the stack record's library-path is not held closed by root or names no directory")
  elif [ "$SPU_REAL" != "$STACK_SPU_REAL" ]; then
    TUPLE_ARGS+=("--runtime-unbound=this agent's SPU is $SPU_REAL, not the installed $STACK_SPU_REAL whose runtime the install tuple recorded")
  elif [ "$AGENT_LIBRARY_REAL" != "$STACK_LIBRARY_REAL" ]; then
    TUPLE_ARGS+=("--runtime-unbound=this agent's library path is ${AGENT_LIBRARY_REAL:-none}, not ${STACK_LIBRARY_REAL:-none}, which the install read with")
  fi
fi
# **The box side is the stack record's install tuple**, written by
# install-tuple.sh at each install. It is trusted only where admin's rule
# holds it closed, as the record's `prefix` is above; an entry another
# principal could write is named as a FAULT rather than read. A record with no
# install tuple prints the box side as absent.
if [ -e "$STACK/install-tuple" ] || [ -L "$STACK/install-tuple" ]; then
  if bad=$(held_closed "$STACK/install-tuple"); then
    TUPLE_ARGS+=(--install-tuple "$STACK/install-tuple")
  else
    TUPLE_ARGS+=(--install-tuple-unheld "$bad")
  fi
fi
if [ -f "$STACK/library-path" ] && held_closed "$STACK/library-path" >/dev/null; then
  STACK_LIBRARY=$(trimmed "$STACK/library-path")
  TUPLE_ARGS+=(--stack-library-path "$STACK_LIBRARY")
fi
say "$AGENT deployment tuple"
TUPLE_RC=0
python3 -I "$TUPLE_HELPER" rows "${TUPLE_ARGS[@]}" || TUPLE_RC=$?
if [ "$TUPLE_RC" -ne 0 ]; then
  admin unload "$AGENT" >/dev/null
  if [ "$TUPLE_RC" -eq 3 ]; then
    die "$AGENT: the deployment tuple carries a FAULT line, so the run is unloaded"
  fi
  die "$AGENT: the tuple helper failed (status $TUPLE_RC), so the run is unloaded"
fi
# **The constituents, from `show`, judged where they run.** Each is recorded
# by pid and start time, so a pid the kernel reuses after the unload is never
# taken for a constituent that survived it.
SHOWN=$(admin show "$AGENT" | tail -1)
KIND=$(python3 -c 'import json,sys; print(json.loads(sys.argv[1]).get("kind",""))' "$SHOWN" 2>/dev/null || true)
[ "$KIND" = state ] || { admin unload "$AGENT" >/dev/null; die "$AGENT: show answers $SHOWN after the load"; }
mapfile -t PIDS < <(python3 -c 'import json,sys; print("\n".join(str(p) for p in json.loads(sys.argv[1]).get("constituents", [])))' "$SHOWN")
[ ${#PIDS[@]} -gt 0 ] && [ -n "${PIDS[0]}" ] || { admin unload "$AGENT" >/dev/null; die "$AGENT: show names no constituent of a loaded run: $SHOWN"; }
# A process's start time, which with its pid names it uniquely. A zombie, dead
# and awaiting its reaper, answers nothing: it holds no descriptor and no lock.
start_time() {
  local stat
  stat=$(cat "/proc/$1/stat" 2>/dev/null) || return 1
  stat=${stat##*) }
  set -- $stat
  [ "$1" != Z ] || return 1
  printf '%s' "${20}"
}
# **The agent's cgroup, beneath the agent's own scope** (6.1).
AGENT_CGROUP="/sys/fs/cgroup/weaver.slice/weaver-$AGENT.scope/weaver-$AGENT"
# in_leaf PID LEAF: the pid is listed in the leaf's cgroup.procs.
in_leaf() { grep -qx -- "$1" "$AGENT_CGROUP/$2/cgroup.procs" 2>/dev/null; }
declare -A ACCOUNT=()
for who in "weaver-$AGENT" "weaver-$AGENT-state" "weaver-$AGENT-relay"; do
  uid=$(id -u "$who" 2>/dev/null) && ACCOUNT["$uid"]=$who
done
RECORDED=()
RELAY_SEEN=0
FAULT=""
for pid in "${PIDS[@]}"; do
  st=$(start_time "$pid") || { FAULT="constituent $pid is not running"; break; }
  uid=$(awk '/^Uid:/ {print $2}' "/proc/$pid/status" 2>/dev/null)
  who=${ACCOUNT[$uid]:-}
  [ -n "$who" ] || { FAULT="constituent $pid runs as uid $uid, none of the agent's, its member's or its relay's accounts"; break; }
  [ "$who" = "weaver-$AGENT-relay" ] && RELAY_SEEN=1
  if [ "$who" = "weaver-$AGENT" ]; then leaf=worker/run; else leaf=constituents; fi
  in_leaf "$pid" "$leaf" \
    || { FAULT="constituent $pid ($who) is not in $AGENT_CGROUP/$leaf, the agent's cgroup beneath its scope"; break; }
  plan "constituent $pid  $(cat "/proc/$pid/comm" 2>/dev/null)  $who  in $leaf beneath the agent's scope"
  RECORDED+=("$pid:$st")
done
if [ -z "$FAULT" ] && [ "$SINK_KIND" = file ] && [ "$RELAY_SEEN" -eq 0 ]; then
  FAULT="no relay stands among the constituents of a file sink"
fi
if [ -n "$FAULT" ]; then
  # **A run that failed its verification never stays serving**, `--keep`
  # included, which keeps only a run every check passed (Codex on #79).
  admin unload "$AGENT" >/dev/null
  die "$AGENT: $FAULT, so the run is unloaded"
fi
if [ "$KEEP" -eq 1 ]; then
  plan "left serving; unload it with the unload verb of $(printf '%q' "$ADMIN") as root (deploy/HowToDeployANewAgent.md section 5)"
else
  plan "unload      $(admin unload "$AGENT" | tail -1)"
  # **Nothing of the run is left**: no recorded constituent still stands under
  # its own start time, and `show` reads unloaded with no constituent.
  for entry in "${RECORDED[@]}"; do
    pid=${entry%%:*}
    if [ "$(start_time "$pid" || true)" = "${entry#*:}" ]; then
      die "$AGENT: constituent $pid outlived the unload"
    fi
  done
  [ ! -e "$AGENT_CGROUP" ] || die "$AGENT: the agent's cgroup $AGENT_CGROUP outlived the unload"
  AFTER_SHOW=$(admin show "$AGENT" | tail -1)
  python3 -c 'import json,sys; d=json.loads(sys.argv[1]); sys.exit(0 if d.get("kind")=="state" and d.get("state")=="unloaded" and not d.get("constituents") else 1)' "$AFTER_SHOW" \
    || die "$AGENT: show after the unload answers $AFTER_SHOW"
  plan "after       every constituent gone, the agent's cgroup gone, show unloaded"
fi
say "$AGENT verified"
