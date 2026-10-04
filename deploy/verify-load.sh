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
# base, default /etc/weaver/admin), and the declaration directory that root
# names, as admin reads them: the declaration `agent.toml`. `weaver-admin`
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
# in this script's own cgroup, the containment the load was invoked from, a
# relay stands among them for a file sink, and after the unload none of them
# is left and `show` reads unloaded with no constituent.
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

# **Admin judges the root before this script reads anything in it.** It read
# `agent.toml` and then counted the sink it names, as root, before admin had
# judged the root, so a writable root or declaration could name a FIFO or a
# device as the sink (Codex on #45). `validate` applies admin's root, ancestor
# and entry judgments (Spec section 9), so the root is read only once admin
# answers `validated`, and through admin's judgment rather than a copy of it.
VERDICT=$(admin validate "$AGENT" | tail -1)
[ "$VERDICT" = '{"kind":"validated"}' ] || die "admin does not validate $AGENT, so its root is not read: $VERDICT"
# The declaration directory is the root's key, judged by the validate above.
DECL_DIR=$(< "$ROOT/declaration-directory")
DECL_DIR=${DECL_DIR#"${DECL_DIR%%[![:space:]]*}"}; DECL_DIR=${DECL_DIR%"${DECL_DIR##*[![:space:]]}"}
DECL="$DECL_DIR/agent.toml"; [ -f "$DECL" ] || die "no declaration at $DECL"

SINK=$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["trace-sink"]["path"])' "$DECL") \
  || die "the declaration names no trace-sink.path"
lines() { [ -e "$1" ] && wc -l < "$1" || echo 0; }

say "$AGENT"
plan "declaration $DECL"
plan "sink        $SINK"
plan "validate    $VERDICT"
BEFORE=$(lines "$SINK")
admin unload "$AGENT" >/dev/null
plan "load        $(admin load "$AGENT" | tail -1)"
AFTER=$(lines "$SINK")
NEW=$((AFTER - BEFORE))
if [ "$NEW" -le 0 ]; then
  st="$(dirname "$SINK")/state/state.log"
  # Read as the member, whose territory it is, never as root: the member can put
  # a link at that name, and root's read would follow it anywhere.
  said=$(sudo -n -u "weaver-$AGENT-state" tail -n 3 "$st" 2>/dev/null || true)
  [ -n "$said" ] && { plan "the state member last said:"; printf '%s\n' "$said" | sed 's/^/     /'; }
  admin unload "$AGENT" >/dev/null
  die "the load wrote no events to $SINK"
fi
plan "events      $NEW new lines"
tail -n "$NEW" "$SINK" | python3 -c '
import sys, json, collections
kinds = collections.Counter()
load = None
for line in sys.stdin:
    try: ev = json.loads(line)
    except ValueError: kinds["<unparsed>"] += 1; continue
    k = ev.get("kind") or ev.get("event") or ev.get("type") or "?"
    kinds[k] += 1
    if load is None and "load" in json.dumps(k).lower(): load = ev
for k, n in kinds.most_common(): print(f"   {n:4d}  {k}")
if load is None: sys.exit("   no load event among them")
keep = {k: v for k, v in load.items() if k not in ("content",)}
print("   load event:", json.dumps(keep)[:600])
' || { admin unload "$AGENT" >/dev/null; die "$AGENT: the new events carry no load event, so the run is unloaded"; }
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
OWN_CGROUP=$(cat /proc/self/cgroup)
SINK_KIND=$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["trace-sink"].get("kind",""))' "$DECL" 2>/dev/null || true)
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
  [ "$(cat "/proc/$pid/cgroup" 2>/dev/null)" = "$OWN_CGROUP" ] \
    || { FAULT="constituent $pid ($who) sits in $(cat "/proc/$pid/cgroup" 2>/dev/null), not the invoker's $OWN_CGROUP"; break; }
  plan "constituent $pid  $(cat "/proc/$pid/comm" 2>/dev/null)  $who  in the invoker's cgroup"
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
  AFTER_SHOW=$(admin show "$AGENT" | tail -1)
  python3 -c 'import json,sys; d=json.loads(sys.argv[1]); sys.exit(0 if d.get("kind")=="state" and d.get("state")=="unloaded" and not d.get("constituents") else 1)' "$AFTER_SHOW" \
    || die "$AGENT: show after the unload answers $AFTER_SHOW"
  plan "after       every constituent gone, show unloaded"
fi
say "$AGENT verified"
