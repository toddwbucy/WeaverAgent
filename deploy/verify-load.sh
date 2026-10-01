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
# `agent.toml`, and `worker-binary`, beside which `weaver-admin` is installed.
# It reads the sink path out of the declaration through tomllib, so what it
# counts is the file admin opened, and it judges the load by events arriving
# in that file rather than by admin's exit status: a load that answers
# `loaded` and writes nothing is the failure this exists to catch.
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

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
ADMIN="$(dirname "$WORKER")/weaver-admin"; [ -x "$ADMIN" ] || die "no weaver-admin at $ADMIN"
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
