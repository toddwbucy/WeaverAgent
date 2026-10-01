#!/usr/bin/env bash
# Load one agent, read its load event back out of its own sink, and unload it.
#
#   sudo deploy/verify-load.sh <agent>            load, read, unload
#   sudo deploy/verify-load.sh <agent> --keep     load, read, leave it serving
#
# The verify step of update-stack.sh, standing alone so a fresh install and a
# single agent can be checked without a rebuild. Everything it reads is the
# agent's own root, `<admin base>/<agent>/` (WEAVER_ADMIN_CONFIG names the
# base, default /etc/weaver/admin), as admin reads it: the declaration
# `agent.toml`. `weaver-admin` itself is taken from the stack record's `prefix`
# (WEAVER_STACK_RECORD, default /etc/weaver/stack), never from a key of the
# agent's root, which admin has not yet judged when this runs it as root.
# It reads the sink path out of the declaration through tomllib, so what it
# counts is the file admin opened, and it judges the load by events arriving
# in that file rather than by admin's exit status: a load that answers
# `loaded` and writes nothing is the failure this exists to catch.
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
read_key() { cat "$ROOT/$1" 2>/dev/null || true; }
WORKER=$(read_key worker-binary); [ -n "$WORKER" ] || die "no worker-binary in $ROOT"
# **The program run as root comes from the record and is judged first.** It was
# `$(dirname worker-binary)/weaver-admin`, a key of an agent root admin had not
# yet judged, so a root, key or directory another principal could write chose
# what this script ran as root (Codex on #45). The record's `prefix` names the
# install. The record entry and the binary must each be held closed by admin's
# rule, `held_closed`, before the binary runs.
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
bad=$(held_closed "$STACK/prefix") || die "the stack record's prefix is not held closed by root: $bad"
PREFIX=$(tr -d '[:space:]' < "$STACK/prefix"); [ -n "$PREFIX" ] || die "the stack record's prefix is empty"
ADMIN="$PREFIX/bin/weaver-admin"
[ -f "$ADMIN" ] && [ -x "$ADMIN" ] || die "no weaver-admin at $ADMIN"
bad=$(held_closed "$ADMIN") || die "weaver-admin at $ADMIN is not held closed by root: $bad"
DECL="$ROOT/agent.toml"; [ -f "$DECL" ] || die "no declaration at $DECL"

SINK=$(python3 -c 'import sys,tomllib; print(tomllib.load(open(sys.argv[1],"rb"))["trace-sink"]["path"])' "$DECL") \
  || die "the declaration names no trace-sink.path"
lines() { [ -e "$1" ] && wc -l < "$1" || echo 0; }

admin() { WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$ADMIN" "$@" 2>&1 || true; }

say "$AGENT"
plan "declaration $DECL"
plan "sink        $SINK"
plan "validate    $(admin validate "$AGENT" | tail -1)"
BEFORE=$(lines "$SINK")
admin unload "$AGENT" >/dev/null
plan "load        $(admin load "$AGENT" | tail -1)"
AFTER=$(lines "$SINK")
NEW=$((AFTER - BEFORE))
if [ "$NEW" -le 0 ]; then
  st="$(dirname "$SINK")/state/state.log"
  [ -f "$st" ] && { plan "the state member last said:"; tail -n 3 "$st" | sed 's/^/     /'; }
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
' || { [ "$KEEP" -eq 1 ] || admin unload "$AGENT" >/dev/null; die "$AGENT: the new events carry no load event"; }
if [ "$KEEP" -eq 1 ]; then
  plan "left serving; unload with: sudo WEAVER_ADMIN_CONFIG=$ADMIN_BASE $ADMIN unload $AGENT"
else
  plan "unload      $(admin unload "$AGENT" | tail -1)"
fi
say "$AGENT verified"
