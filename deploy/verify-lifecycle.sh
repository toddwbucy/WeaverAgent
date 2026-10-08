#!/usr/bin/env bash
# Verify one agent's whole lifecycle on a live box, end to end: create it,
# judge its layout, load it, seed it, take save points, reload, restore,
# force it down, crash it, optionally migrate it, and take it down again.
#
#   ./deploy/verify-lifecycle.sh --agent vtest1                          plan only
#   ./deploy/verify-lifecycle.sh --agent vtest1 --artifact <gguf> \
#       --archive <dir> --apply                                          act
#   ./deploy/verify-lifecycle.sh --agent vtest1 --cleanup                plan the take-down
#   ./deploy/verify-lifecycle.sh --agent vtest1 --cleanup --archive <dir> --apply
#
# **The agent is a throwaway this script makes.** Step 1 is create-agent.sh,
# so a name that already stands, as an account, a group, a root, a territory
# or a sudo rule, is refused before anything is done. Before step 1 acts, the
# script writes a creation marker, `verify-lifecycle-<agent>` in the stack
# record (root's, 0644; admin never reads the stack record, and every script
# that does reads its keys by name), and `--cleanup` takes down only an agent
# whose marker stands. Step 9 removes the marker last.
#
# **Plan by default.** Without `--apply` the script prints every step, the
# commands it runs and the answers it expects, acts on nothing, invokes no
# sudo, and exits 0. With `--apply` it asks for sudo itself, as create-agent.sh
# does, runs the steps in order, and prints PASS or FAIL for every check and
# every step. The first FAIL stops the run, naming the step, the check, what
# was expected and what was found, and leaves the agent as it stands for the
# operator to look at; `--cleanup` takes it down afterwards. The near misses
# (steps 1b, 6b and 8b) put back exactly what they changed, the trap included,
# whatever a check answers.
#
# **sudo runs only where admin and the deploy scripts run it**: admin's verbs,
# the root reads of the territory, the trace, the manifest and the marker, the
# near misses' changes and their restores, the worker's kill in step 7, the
# layout staged by hand in step 8, and HowToDeployANewAgent.md section 7's
# take-down in step 9. One more: the turns of steps 2 and 4 run turn.py as the
# operator's own uid through `sudo -u <operator>`, which takes the groups
# create-agent.sh just joined without a new login.
#
# **Step 9 never runs decommission.sh**, which takes every agent off the box:
# it removes this one agent by HowToDeployANewAgent.md section 7's steps,
# archiving its territory and its root first into `--archive <dir>`.
#
# **Steps 8 and 8b run only with `--with-migration`.** Step 8 runs
# `update-stack.sh --install`, which touches every agent on the box: it
# validates, loads and unloads each agent under the admin base. It is off by
# default.
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: deploy/verify-lifecycle.sh --agent <name> [options]

Verify an agent's lifecycle end to end against a throwaway agent this script
creates (step 1, create-agent.sh) and takes down (step 9). Plan only unless
--apply is given. Run it as the operator, never under sudo or as root.

  --agent <name>          the throwaway agent; refused if it already exists
  --artifact <path>       the model the agent's decoder binds (required with --apply)
  --archive <dir>         where step 9 archives the territory and the root
                          before removing them (default: ~/.weaver-archive,
                          made 0700 if absent); it must be writable by you
                          and root alone, never a shared directory like /tmp
  --checkout <dir>        the repository whose deploy/ scripts run
                          (default: the repository this script is in)
  --admin-base <dir>      the admin base (default: WEAVER_ADMIN_CONFIG, else
                          /etc/weaver/admin)
  --stack-record <dir>    the stack record (default: WEAVER_STACK_RECORD, else
                          /etc/weaver/stack)
  --prefix <dir>          the install prefix (default: the stack record's
                          prefix, else /opt/weaver)
  --with-migration        also run step 8, the layout migration, and step 8b.
                          OFF BY DEFAULT: step 8 runs update-stack.sh --install,
                          which touches every agent on the box (it validates,
                          loads and unloads each one)
  --allow-other-agents    with --apply, run on a box whose admin base holds
                          other agents
  --cleanup               only take the agent down (step 9), for a run that
                          stopped part way; refused for an agent this script
                          did not create
  --apply                 act; without it, print the plan and act on nothing
  -h, --help              this text

Steps: 0 the box; 1 create; 1b near misses; 2 load, seed, one turn;
3 save point; 4 unload and reload; 5 restore; 6 force-unload; 6b force-unload
on a drifted territory; 7 crash recovery; 8 layout migration (--with-migration);
8b an access entry on the trace (--with-migration); 9 take the agent down.
Step 0 runs update-stack.sh in plan mode, which builds the tree: set
CARGO_TARGET_DIR to a private directory first.
USAGE
}

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

AGENT=""
ARTIFACT=""
ARCHIVE=""
CHECKOUT=""
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
PREFIX_ARG=""
APPLY=0
WITH_MIGRATION=0
ALLOW_OTHERS=0
CLEANUP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --agent)        [ $# -ge 2 ] || die "--agent needs a name"; AGENT=$2; shift ;;
    --artifact)     [ $# -ge 2 ] || die "--artifact needs a path"; ARTIFACT=$2; shift ;;
    --archive)      [ $# -ge 2 ] || die "--archive needs a directory"; ARCHIVE=$2; shift ;;
    --checkout)     [ $# -ge 2 ] || die "--checkout needs a directory"; CHECKOUT=$2; shift ;;
    --admin-base)   [ $# -ge 2 ] || die "--admin-base needs a directory"; ADMIN_BASE=$2; shift ;;
    --stack-record) [ $# -ge 2 ] || die "--stack-record needs a directory"; STACK=$2; shift ;;
    --prefix)       [ $# -ge 2 ] || die "--prefix needs a directory"; PREFIX_ARG=$2; shift ;;
    --with-migration)     WITH_MIGRATION=1 ;;
    --allow-other-agents) ALLOW_OTHERS=1 ;;
    --cleanup)      CLEANUP=1 ;;
    --apply)        APPLY=1 ;;
    -h|--help)      usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

# ------------------------------------------------------------ refusals
# Every refusal below is made before anything acts and before sudo is asked
# for, except those that need root to look, which follow the credential.

# **Run as the operator**: the gate admits the operator's uid, the root's
# `operator` key names it, and create-agent.sh refuses root.
[ "$(id -u)" -ne 0 ] || die "run as the operator, not as root: create-agent.sh names this uid as the agent's operator, and --apply asks for sudo itself"
[ -z "${SUDO_USER:-}" ] || die "run as the operator, not under sudo: --apply asks for sudo itself where a step needs it"

[ -n "$AGENT" ] || die "name the throwaway agent: --agent <name> (see --help)"
# create-agent.sh's own rule, so the name step 1 makes is the name refused here.
[[ "$AGENT" =~ ^[a-z][a-z0-9]{1,15}$ ]] || die "the name is lowercase letters and digits, 2 to 16 characters, as create-agent.sh requires: '$AGENT'"

if [ -z "$CHECKOUT" ]; then
  CHECKOUT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi
for f in create-agent.sh verify-load.sh update-stack.sh turn.py; do
  [ -f "$CHECKOUT/deploy/$f" ] || die "the checkout $CHECKOUT holds no deploy/$f"
done
command -v python3 >/dev/null || die "python3 is not on PATH, and every answer is read as JSON through it"

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

# **closed_to_others PATH**: PATH's canonical path owned by root or the
# operator and writable by neither group nor other, sticky or not, and every
# component above it held the same way except that a sticky directory is
# allowed there; answers non-zero and prints the first that is not.
closed_to_others() {
  local at owner mode me leaf=1
  me=$(id -u)
  at=$(realpath -e -- "$1" 2>/dev/null) || { printf '%s' "$1"; return 1; }
  while :; do
    read -r owner mode < <(stat -c '%u %a' -- "$at" 2>/dev/null) || { printf '%s' "$at"; return 1; }
    if { [ "$owner" != 0 ] && [ "$owner" != "$me" ]; } \
      || { (( 8#$mode & 8#022 )) && { [ "$leaf" = 1 ] || ! { [ -d "$at" ] && (( 8#$mode & 8#1000 )); }; }; }; then
      printf '%s' "$at"; return 1
    fi
    leaf=0
    [ "$at" = / ] && return 0
    at=$(dirname -- "$at")
  done
}

trim() {
  local value=$1
  value=${value#"${value%%[![:space:]]*}"}
  value=${value%"${value##*[![:space:]]}"}
  printf '%s' "$value"
}

[ -d "$STACK" ] || die "no stack record at $STACK: bootstrap-stack.sh writes it"
bad=$(held_closed "$STACK") || die "the stack record $STACK is not held closed by root at $bad"
stack_key() { # stack_key KEY: the record's value, trimmed, or nothing
  [ -e "$STACK/$1" ] || return 0
  trim "$(cat -- "$STACK/$1")"
}
for key in worker-binary coordination-root agent-directory; do
  [ -n "$(stack_key "$key")" ] || die "no $key in the stack record $STACK"
done
RECORD_PREFIX=$(stack_key prefix)
# **The prefix is the record's**, as create-agent.sh and verify-load.sh read
# it, so a different `--prefix` would verify one admin and run another.
if [ -n "$PREFIX_ARG" ] && [ -n "$RECORD_PREFIX" ] && [ "$PREFIX_ARG" != "$RECORD_PREFIX" ]; then
  die "--prefix $PREFIX_ARG differs from the stack record's prefix $RECORD_PREFIX, which create-agent.sh and verify-load.sh read"
fi
PREFIX=${PREFIX_ARG:-${RECORD_PREFIX:-/opt/weaver}}
ADMIN=$(realpath -e -- "$PREFIX/bin/weaver-admin" 2>/dev/null) || die "no weaver-admin at $PREFIX/bin/weaver-admin"
bad=$(held_closed "$ADMIN") || die "weaver-admin at $ADMIN is not held closed by root at $bad, so it is not run as root"
COORD=$(stack_key coordination-root)
WORKER_COMM=$(basename -- "$(stack_key worker-binary)"); WORKER_COMM=${WORKER_COMM:0:15}
AGENT_DIR_WRITTEN=$(stack_key agent-directory)
AGENT_DIR=$(realpath -e -- "$AGENT_DIR_WRITTEN" 2>/dev/null) || die "the stack record's agent-directory $AGENT_DIR_WRITTEN does not resolve"

A=$AGENT
AU="weaver-$A"
T="$AGENT_DIR/$AU"                 # the territory, canonical
R="$ADMIN_BASE/$A"                 # the agent's root
STAGE="$ADMIN_BASE/.$A.partial"    # create-agent.sh's staged root
RULE="/etc/sudoers.d/$AU"
TRACE="$T/trace.ndjson"
DECL="$T/agent.toml"
SAVE_POINTS="$T/save-points"
MANIFEST="$SAVE_POINTS/save-points.manifest"
RUN_DIR="$COORD/weaver.run/$A"
RUNTIME_DIR="$COORD/$AU"
MARKER="$STACK/verify-lifecycle-$A"
OPERATOR=$(id -un)
OPERATOR_UID=$(id -u)
ACCOUNTS="$AU $AU-state $AU-relay $AU-admincon"
GROUPS_MADE="$AU $AU-state $AU-trace $AU-admin $AU-admincon"
OLD_DIR="${HOME:-/nonexistent}/.weaveragent/$A"   # step 8's layout from before 2026-10-07

marker_names_agent() { [ -f "$MARKER" ] && grep -qxF "agent = \"$A\"" "$MARKER"; }

# lookup DB NAME: 0 where it stands, 1 where it does not; a lookup that cannot
# answer refuses rather than reading as absence.
lookup() {
  local rc=0
  getent "$1" "$2" >/dev/null || rc=$?
  case $rc in 0) return 0 ;; 2) return 1 ;; *) die "cannot read $1 $2" ;; esac
}

if [ "$CLEANUP" -eq 1 ]; then
  # **--cleanup takes down only what this script made**: the marker it wrote
  # before step 1 must stand and name the agent.
  marker_names_agent || die "no marker at $MARKER naming $A: verify-lifecycle.sh did not create $A, and --cleanup takes down only a throwaway agent this script made. Take any other agent down by deploy/HowToDeployANewAgent.md section 7"
  if [ -f "$R/territory" ]; then
    named=$(trim "$(cat -- "$R/territory")")
    [ "$named" = "$T" ] || die "$R/territory names $named, not $T, and --cleanup removes only the territory named for its agent"
  fi
else
  # **The agent is one this run makes**: nothing of it may stand.
  marker_names_agent && die "a marker from an earlier run stands at $MARKER: take that agent down first with --cleanup"
  for u in $ACCOUNTS; do lookup passwd "$u" && die "the account $u already exists: this script verifies a throwaway agent it creates at step 1"; done
  for g in $GROUPS_MADE; do lookup group "$g" && die "the group $g already exists: this script verifies a throwaway agent it creates at step 1"; done
  for p in "$R" "$STAGE" "$T" "/home/$AU"; do
    { [ -e "$p" ] || [ -L "$p" ]; } && die "$p already exists: this script verifies a throwaway agent it creates at step 1"
  done
fi

# The other agents on the box: the roots under the base, as update-stack.sh
# counts them.
OTHERS=""
for root in "$ADMIN_BASE"/*/; do
  [ -d "$root" ] || continue
  name=$(basename -- "$root")
  [ "$name" = "$A" ] || OTHERS="$OTHERS${OTHERS:+ }$name"
done

if [ "$APPLY" -eq 1 ]; then
  # **The default archive is the operator's own, closed** (the Planner's
  # grade of c1932b5): made 0700 under the operator's home where absent.
  if [ -z "$ARCHIVE" ]; then
    ARCHIVE="$HOME/.weaver-archive"
    [ -d "$ARCHIVE" ] || ( umask 077 && mkdir -p -- "$ARCHIVE" ) \
      || die "the default archive directory $ARCHIVE could not be made"
  fi
  [ -d "$ARCHIVE" ] || die "the archive directory $ARCHIVE does not stand"
  # **The archive is written as root into a directory no other principal
  # writes** (the commit security review of 36f1374): a directory another
  # user could write would let them plant a link at the archive's name for
  # root's tar to follow, and the archive holds the territory's custodied
  # files. The directory itself is root's or the operator's and writable by
  # neither group nor other, sticky or not: a sticky directory such as /tmp
  # stops another user removing a name, never creating one. Every component
  # above it is held the same way, where a sticky one is safe, since no other
  # user can rename or replace an entry it does not own there.
  bad=$(closed_to_others "$ARCHIVE") \
    || die "the archive directory $ARCHIVE is writable by another principal at $bad: name one only root and you can write"
  ARCHIVE=$(realpath -e -- "$ARCHIVE")
  if [ "$CLEANUP" -eq 0 ]; then
    [ -n "$ARTIFACT" ] || die "--apply needs --artifact <path>: step 1's create-agent.sh binds it"
    if [ -n "$OTHERS" ] && [ "$ALLOW_OTHERS" -eq 0 ]; then
      die "the admin base $ADMIN_BASE holds other agents ($OTHERS). Step 8's update-stack.sh --install touches every agent on the box: it validates, loads and unloads each one. Run this on a box holding no other agent, or pass --allow-other-agents to accept that"
    fi
    [ -n "${CARGO_TARGET_DIR:-}" ] || die "set CARGO_TARGET_DIR to a private directory: step 0 runs update-stack.sh's plan, which builds the tree"
    if [ "$WITH_MIGRATION" -eq 1 ] && { [ -e "$OLD_DIR" ] || [ -L "$OLD_DIR" ]; }; then
      die "$OLD_DIR already stands, and step 8 stages the old layout there"
    fi
  fi
fi

# ------------------------------------------------------------ helpers
STEP_LABEL=""
RESTORE=()
CREATED=0
LOGDIR=""

begin_step() { # begin_step ID REFS TITLE
  STEP_LABEL="step $1 ($2)"
  say "$STEP_LABEL: $3"
}
end_step() { [ "$APPLY" -eq 0 ] || printf '   == PASS %s\n' "$STEP_LABEL"; }
run_line() { printf '   $ %s\n' "$*"; }
want()     { printf '     expect: %s\n' "$*"; }
skip()     { say "step $1 SKIPPED: $2"; }

fail() { # fail CHECK EXPECTED FOUND
  printf '   FAIL %s: %s\n' "$STEP_LABEL" "$1"
  printf '   == FAIL %s\n' "$STEP_LABEL"
  printf '\nFAIL %s\n   check:    %s\n   expected: %s\n   found:    %s\n' "$STEP_LABEL" "$1" "$2" "$3" >&2
  exit 1
}
pass() { printf '   PASS %s: %s\n' "$STEP_LABEL" "$1"; }
expect_eq() { # expect_eq CHECK EXPECTED FOUND
  if [ "$2" = "$3" ]; then pass "$1"; else fail "$1" "$2" "$3"; fi
}
measured() { printf '   MEASURED %s: %s\n' "$STEP_LABEL" "$*"; }

# json_at JSON PATH: the value at a dotted path, a string as itself and
# anything else as compact JSON, `absent` where the path is not there and
# `unparsed` where the text is not JSON.
json_at() {
  python3 -c 'import json, sys
try:
    v = json.loads(sys.argv[1])
except ValueError:
    print("unparsed"); sys.exit(0)
for p in [x for x in sys.argv[2].split(".") if x]:
    if isinstance(v, dict) and p in v: v = v[p]
    else: print("absent"); sys.exit(0)
print(v if isinstance(v, str) else json.dumps(v, separators=(",", ":"), sort_keys=True))' "$1" "$2"
}

# **answered_state ANSWER STATE: admin's answer is the state named**, read as
# JSON and never matched as text, as update-stack.sh's and verify-load.sh's
# reader of the same name (the #94 survey's S22, #99 area 2 review K8).
answered_state() {
  python3 -c 'import json, sys
try:
    d = json.loads(sys.argv[1])
except ValueError:
    sys.exit(1)
sys.exit(0 if isinstance(d, dict) and d.get("kind") == "state" and d.get("state") == sys.argv[2] else 1)' "$1" "$2"
}

# **ask VERB: admin's answer to one verb on this agent**, as update-stack.sh's
# `admin_answer` reads it: the last line is the answer, in ANSWER, and every
# line before it is admin's cause, kept in ASKED_CAUSE and relayed to stderr.
ANSWER=""
ASKED_CAUSE=""
ask() {
  local said
  said=$(sudo -n WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$ADMIN" "$1" "$A" 2>&1) || true
  ASKED_CAUSE=$(printf '%s\n' "$said" | sed '$d')
  [ -z "$ASKED_CAUSE" ] || printf '%s\n' "$ASKED_CAUSE" | sed 's/^/     admin: /' >&2
  ANSWER=$(printf '%s\n' "$said" | sed -n '$p')
  printf '   admin %s: %s\n' "$1" "${ANSWER:-nothing}"
}
expect_state() { # expect_state VERB STATE
  if answered_state "$ANSWER" "$2"; then pass "$1 answers the $2 state"
  else fail "$1 answers the $2 state" "{\"kind\":\"state\",\"state\":\"$2\",...}" "${ANSWER:-nothing}"; fi
}
expect_kind() { # expect_kind VERB KIND
  expect_eq "$1 answers $2" "$2" "$(json_at "$ANSWER" kind)"
}
expect_unloaded_alone() { # show reads unloaded with no constituent
  ask show
  expect_state show unloaded
  local c; c=$(json_at "$ANSWER" constituents)
  case "$c" in absent|"[]") pass "show names no constituent" ;; *) fail "show names no constituent" "no constituents" "$c" ;; esac
}

# The root's reads: layout, access entries, trace, manifest, marker.
lay() { # owner:group mode type
  local s
  s=$(sudo -n stat -c '%U:%G %a %F' -- "$1" 2>/dev/null) || { printf 'absent'; return 0; }
  printf '%s' "$s"
}
expect_lay() { # expect_lay PATH "owner:group mode type"
  local found; found=$(lay "$1")
  # An empty regular file is a regular file unless the expectation says empty.
  [[ "$2" == *empty* ]] || found=${found/regular empty file/regular file}
  expect_eq "$1 is $2" "$2" "$found"
}
expect_no_acl() {
  local p found
  for p in "$@"; do
    found=$(sudo -n getfacl -p --skip-base -- "$p" 2>&1) || true
    expect_eq "$p carries no access entry" "" "$found"
  done
}
as_can() { # as_can ACCOUNT TEST...: yes where the account passes the test
  if sudo -n -u "$1" "${@:2}" >/dev/null 2>&1; then printf yes; else printf no; fi
}
TRACE_PY='import json, sys
path, start, kinds = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
with open(path) as f:
    for n, line in enumerate(f, 1):
        if n <= start: continue
        try: e = json.loads(line)
        except ValueError: continue
        if not kinds or e.get("kind") in kinds: print(json.dumps(e, separators=(",", ":"), sort_keys=True))'
events_since() { local start=$1; shift; sudo -n python3 -c "$TRACE_PY" "$TRACE" "$start" "$@"; }
last_event() { events_since 0 "$1" | tail -n 1; }
trace_lines() { sudo -n python3 -c 'import sys; print(sum(1 for _ in open(sys.argv[1])))' "$TRACE"; }
# The manifest's lines, ordinal, arrival, name and digest, in file order.
manifest() {
  sudo -n python3 -c 'import json, os, sys
if not os.path.exists(sys.argv[1]): sys.exit(0)
for l in open(sys.argv[1]):
    if l.strip():
        d = json.loads(l); print(d["ordinal"], d["arrived"], d["name"], d["digest"])' "$MANIFEST"
}
manifest_count() { manifest | grep -c . || true; }
manifest_field() { # manifest_field LINE FIELD(1-4): LINE counted from 1, or "last"
  if [ "$1" = last ]; then manifest | tail -n 1 | cut -d' ' -f"$2"
  else manifest | sed -n "$1p" | cut -d' ' -f"$2"; fi
}
marker() { sudo -n cat -- "$R/run.marker" 2>/dev/null || printf 'absent'; }
expect_marker() { # expect_marker RUN STATE
  local m; m=$(marker)
  expect_eq "the run marker names run $1" "$1" "$(json_at "$m" run)"
  expect_eq "the run marker reads $2" "$2" "$(json_at "$m" state)"
}
file_size() { sudo -n stat -c %s -- "$1" 2>/dev/null || printf 0; }
# expect_in_order CHECK FILE NEEDLE...: each needle in FILE, after the last.
expect_in_order() {
  local check=$1 file=$2; shift 2
  local found
  found=$(python3 -c 'import sys
text = open(sys.argv[1]).read(); at = 0
for n in sys.argv[2:]:
    i = text.find(n, at)
    if i < 0: print("missing after the earlier lines: " + n); sys.exit(0)
    at = i + len(n)
print("in order")' "$file" "$@")
  expect_eq "$check" "in order" "$found"
}
# turn TEXT...: one turn through turn.py as the operator's uid with the
# groups create-agent.sh joined, its answer kept in TURN_SAID.
TURN_SAID=""
turn() {
  local rc=0 out="$LOGDIR/turn.$RANDOM"
  sudo -n -u "$OPERATOR" WEAVER_ADMIN_CONFIG="$ADMIN_BASE" python3 "$CHECKOUT/deploy/turn.py" "$A" "$@" >"$out" 2>"$out.err" || rc=$?
  TURN_SAID=$(cat "$out")
  [ "$rc" -eq 0 ] || cat "$out.err" >&2
  expect_eq "turn.py $* exits 0" 0 "$rc"
  [ -n "$TURN_SAID" ] || fail "turn.py $* answers" "an answer on stdout" "nothing"
  pass "turn.py $* answers"
  printf '%s\n' "$TURN_SAID" | sed 's/^/     answer: /'
}

# **The near misses put back what they changed.** RESTORE holds the one
# restore pending, run as soon as admin has answered and by the exit trap
# where anything stops the run between the change and that answer.
restore_mode()  { sudo -n chmod "$(printf '%05o' "$((8#$2))")" -- "$1"; }
restore_group() { sudo -n chgrp -- "$2" "$1"; }
restore_acl()   { sudo -n setfacl -b -- "$1"; }
restore_key()   { printf '%s' "$1" | base64 -d | sudo -n tee -- "$R/territory" >/dev/null && sudo -n rm -f -- "$2"; }
restore_now() {
  [ ${#RESTORE[@]} -gt 0 ] || return 0
  local -a pending=("${RESTORE[@]}")
  RESTORE=()
  if "${pending[@]}"; then
    printf '   restored: %s\n' "${pending[*]}"
  else
    printf '\nRESTORE FAILED: %s; put it back by hand\n' "${pending[*]}" >&2
    return 1
  fi
}
# near_miss LABEL RESTORE... -- CHANGE...: change, ask validate, restore, then
# judge the answer: admin refuses boundary_unverified.
near_miss() {
  local label=$1; shift
  local -a restore=() change=()
  while [ $# -gt 0 ] && [ "$1" != -- ]; do restore+=("$1"); shift; done
  shift
  change=("$@")
  RESTORE=("${restore[@]}")
  if ! "${change[@]}"; then
    restore_now || true
    fail "$label: the change applies" "it applies" "${change[*]} failed"
  fi
  ask validate
  restore_now || fail "$label: the change is put back" "${restore[*]} succeeds" "it failed"
  expect_kind "$label: validate" boundary_unverified
}

on_exit() {
  local rc=$?
  restore_now || true
  if [ "$rc" -ne 0 ] && [ "$CREATED" -eq 1 ]; then
    printf '\n   %s is left as it stands. Look, then take it down with:\n' "$A" >&2
    printf '   deploy/verify-lifecycle.sh --agent %s --cleanup --archive <dir> --apply\n' "$A" >&2
  fi
  [ -z "$LOGDIR" ] || printf '   full outputs are under %s\n' "$LOGDIR" >&2
}
trap on_exit EXIT
trap 'exit 130' INT TERM HUP

script_env() { WEAVER_ADMIN_CONFIG="$ADMIN_BASE" WEAVER_STACK_RECORD="$STACK" "$@"; }

# ------------------------------------------------------------ the steps
step_0() {
  begin_step 0 "deploy/REDEPLOY.md section 5" "the box, before anything"
  run_line "git -C $CHECKOUT log -1 --oneline"
  run_line "read every key of $STACK; realpath of its agent-directory"
  run_line "deploy/update-stack.sh   (plan mode, no sudo; it builds the tree)"
  want "the stack record holds worker-binary, spu-binary, gate-binary, coordination-root, prefix and agent-directory"
  want "the agent-directory as written is its canonical path (a difference is noted, not failed)"
  want "no account, group, root or territory of $A stands"
  want "update-stack.sh's plan exits 0 and names no NEW or CHANGED binary"
  [ "$APPLY" -eq 1 ] || return 0
  printf '   checkout at %s\n' "$(git -C "$CHECKOUT" log -1 --oneline 2>/dev/null || echo unknown)"
  local k
  for k in worker-binary spu-binary gate-binary coordination-root prefix agent-directory; do
    expect_eq "the stack record holds $k" present "$( [ -n "$(stack_key "$k")" ] && echo present || echo absent )"
    printf '     %s = %s\n' "$k" "$(stack_key "$k")"
  done
  if [ "$AGENT_DIR_WRITTEN" = "$AGENT_DIR" ]; then pass "the agent-directory is written canonical"
  else measured "the agent-directory is written $AGENT_DIR_WRITTEN and resolves to $AGENT_DIR"; fi
  pass "no account, group, root or territory of $A stands (refused before acting otherwise)"
  local rc=0
  ( cd "$CHECKOUT" && script_env bash deploy/update-stack.sh ) >"$LOGDIR/step0-update-stack.log" 2>&1 || rc=$?
  tail -n 15 "$LOGDIR/step0-update-stack.log" | sed 's/^/     /'
  expect_eq "update-stack.sh's plan exits 0" 0 "$rc"
  expect_eq "update-stack.sh's plan names no NEW or CHANGED binary" "" \
    "$(grep -E '^  [^ ]+ +(NEW|CHANGED) ' "$LOGDIR/step0-update-stack.log" || true)"
  end_step
}

step_1() {
  begin_step 1 "weaver-admin-Spec sections 2 and 9; weaver-state-PRD section 4" "create the agent and judge its layout"
  run_line "deploy/create-agent.sh $A --artifact ${ARTIFACT:-<--artifact>}"
  run_line "write the creation marker $MARKER (root, 0644)"
  run_line "deploy/create-agent.sh $A --artifact ${ARTIFACT:-<--artifact>} --apply"
  want "the plan ends PENDING --apply and == plan only"
  want "the apply prints == accounts, == territory, == declaration, the wall lines, == sudo rule, == admission, == made, in order"
  want "$OPERATOR holds $AU, $AU-trace, $AU-state and $AU-admin; $AU-relay holds $AU-trace alone; $AU-admincon holds $AU-admincon, $AU-state and $AU-admin; $AU-state holds itself alone"
  want "$T root:$AU-state 710 directory; state/ $AU-state:$AU-state 700; save-points/ root:$AU-admin 750; trace.ndjson root:$AU-trace 640 empty; agent.toml and system-prompt.md root:$AU-admin 640"
  want "/home/$AU $AU:$AU 2750; $R root:root 755 with every key root:root 644; $RULE root:root 440"
  want "no access entry on the territory, save-points/, the trace or the declaration"
  want "territory names $T, operator is $OPERATOR_UID, roles.toml is trace-reader = \"$AU-admincon\"; the rule lists the eight verbs"
  want "$AU cannot pass the territory; $AU-state reads neither the trace nor the declaration"
  [ "$APPLY" -eq 1 ] || return 0
  local rc=0 out="$LOGDIR/step1-create"
  script_env bash "$CHECKOUT/deploy/create-agent.sh" "$A" --artifact "$ARTIFACT" >"$out.plan" 2>&1 || rc=$?
  expect_eq "create-agent.sh's plan exits 0" 0 "$rc"
  expect_in_order "create-agent.sh's plan ends PENDING --apply, then == plan only" "$out.plan" "PENDING --apply" "== plan only"
  # The marker before the act, so a create that stops part way is ours to clean.
  printf 'agent = "%s"\nmade-by = "deploy/verify-lifecycle.sh"\n' "$A" | sudo -n tee -- "$MARKER" >/dev/null
  sudo -n chmod 0644 -- "$MARKER"
  CREATED=1
  rc=0
  script_env bash "$CHECKOUT/deploy/create-agent.sh" "$A" --artifact "$ARTIFACT" --apply >"$out.apply" 2>&1 || rc=$?
  sed 's/^/     /' "$out.apply"
  expect_eq "create-agent.sh --apply exits 0" 0 "$rc"
  local -a needles=("== accounts" "== territory" "== declaration"
    "$T/agent.toml and $T/system-prompt.md written, root:$AU-admin 0640"
    "can write its state room" "the agent's own uid cannot enter the state room"
    "the member cannot read the trace" "the agent's own uid cannot pass the territory"
    "reads the declaration and writes nothing of the territory"
    "== sudo rule" "== admission" "== made")
  [ "$ADMIN_BASE" != /etc/weaver/admin ] || needles+=("$AU-admincon ran validate through its rule: {\"kind\":\"validated\"}")
  expect_in_order "create-agent.sh's apply prints its sections in order" "$out.apply" "${needles[@]}"

  expect_eq "$OPERATOR holds the four groups" 4 \
    "$(id -Gn "$OPERATOR" | tr ' ' '\n' | grep -cE "^$AU(-trace|-state|-admin)?$" || true)"
  local u
  for u in $ACCOUNTS; do expect_eq "the account $u stands" yes "$(id "$u" >/dev/null 2>&1 && echo yes || echo no)"; done
  expect_eq "$AU-relay holds its trace group alone" "$AU-trace" "$(id -Gn "$AU-relay")"
  expect_eq "$AU-admincon holds its own, the state and the access groups" \
    "$(printf '%s\n' "$AU-admin" "$AU-admincon" "$AU-state" | sort | tr '\n' ' ')" \
    "$(id -Gn "$AU-admincon" | tr ' ' '\n' | sort | tr '\n' ' ')"
  expect_eq "$AU-state holds its own group alone" "$AU-state" "$(id -Gn "$AU-state")"

  expect_lay "$T" "root:$AU-state 710 directory"
  expect_lay "$T/state" "$AU-state:$AU-state 700 directory"
  expect_lay "$SAVE_POINTS" "root:$AU-admin 750 directory"
  expect_lay "$TRACE" "root:$AU-trace 640 regular empty file"
  expect_lay "$DECL" "root:$AU-admin 640 regular file"
  expect_lay "$T/system-prompt.md" "root:$AU-admin 640 regular file"
  expect_no_acl "$T" "$SAVE_POINTS" "$TRACE" "$DECL"
  expect_lay "/home/$AU" "$AU:$AU 2750 directory"
  expect_lay "$R" "root:root 755 directory"
  local key
  for key in $(ls -A -- "$R"); do expect_lay "$R/$key" "root:root 644 regular file"; done
  expect_lay "$RULE" "root:root 440 regular file"
  expect_eq "the territory key names the canonical territory" "$T" "$(trim "$(cat -- "$R/territory")")"
  expect_eq "the operator key names $OPERATOR's uid" "$OPERATOR_UID" "$(trim "$(cat -- "$R/operator")")"
  expect_eq "roles.toml names the connector the trace reader" "trace-reader = \"$AU-admincon\"" "$(trim "$(cat -- "$R/roles.toml")")"
  local listing verb
  listing=$(sudo -n -l -U "$AU-admincon" 2>&1 || true)
  for verb in show validate load unload stop save-point restore force-unload; do
    expect_eq "the rule grants $AU-admincon $verb $A" yes \
      "$(grep -qF "$ADMIN $verb $A" <<<"$listing" && echo yes || echo no)"
  done
  expect_eq "$AU cannot pass the territory" no "$(as_can "$AU" test -x "$T")"
  expect_eq "$AU-state cannot read the trace" no "$(as_can "$AU-state" test -r "$TRACE")"
  expect_eq "$AU-state cannot read the declaration" no "$(as_can "$AU-state" test -r "$DECL")"
  end_step
}

step_1b() {
  begin_step 1b "weaver-admin-Spec section 9; #99 H3" "near misses, validate only, each put back before the next"
  local link="$AGENT_DIR/.verify-lifecycle-$A-link"
  run_line "weaver-admin validate $A"
  run_line "the territory at 0711; setgid; with an access entry u:nobody:x"
  run_line "save-points/ at 0755; agent.toml at 0644; agent.toml grouped to root"
  run_line "the territory key written through $link, a link above the territory (H3)"
  run_line "each: change, weaver-admin validate $A, put back exactly what was changed (the exit trap too)"
  want "validate answers validated before and after; each near miss answers boundary_unverified"
  want "show reads unloaded; the layout is step 1's again"
  [ "$APPLY" -eq 1 ] || return 0
  ask validate; expect_kind validate validated
  local tmode smode dmode dgroup key_b64
  tmode=$(sudo -n stat -c %a -- "$T"); smode=$(sudo -n stat -c %a -- "$SAVE_POINTS")
  dmode=$(sudo -n stat -c %a -- "$DECL"); dgroup=$(sudo -n stat -c %G -- "$DECL")
  key_b64=$(base64 -w0 <"$R/territory")
  near_miss "the territory at 0711" restore_mode "$T" "$tmode" -- sudo -n chmod 0711 -- "$T"
  near_miss "the territory setgid" restore_mode "$T" "$tmode" -- sudo -n chmod g+s -- "$T"
  near_miss "the territory with an access entry" restore_acl "$T" -- sudo -n setfacl -m u:nobody:x -- "$T"
  near_miss "save-points/ at 0755" restore_mode "$SAVE_POINTS" "$smode" -- sudo -n chmod 0755 -- "$SAVE_POINTS"
  near_miss "the declaration at 0644" restore_mode "$DECL" "$dmode" -- sudo -n chmod 0644 -- "$DECL"
  near_miss "the declaration grouped to root" restore_group "$DECL" "$dgroup" -- sudo -n chgrp root -- "$DECL"
  { [ -e "$link" ] || [ -L "$link" ]; } && fail "the H3 link's name is free" "nothing at $link" "$link stands"
  near_miss "the territory key through a link above it" restore_key "$key_b64" "$link" -- \
    bash -c 'sudo -n ln -s -- "$1" "$2" && printf "%s\n" "$2/$3" | sudo -n tee -- "$4" >/dev/null' _ \
      "$AGENT_DIR" "$link" "$AU" "$R/territory"
  expect_unloaded_alone
  expect_lay "$T" "root:$AU-state 710 directory"
  expect_lay "$SAVE_POINTS" "root:$AU-admin 750 directory"
  expect_lay "$DECL" "root:$AU-admin 640 regular file"
  expect_lay "$R/territory" "root:root 644 regular file"
  expect_eq "the territory key is back" "$T" "$(trim "$(cat -- "$R/territory")")"
  expect_eq "the H3 link is gone" absent "$(lay "$link")"
  expect_no_acl "$T"
  ask validate; expect_kind validate validated
  end_step
}

RUN=""
step_2() {
  begin_step 2 "weaver-admin-Spec section 6; #99 K12" "validate, load, seed, one turn"
  run_line "weaver-admin validate $A"
  run_line "deploy/verify-load.sh $A --keep   (as root)"
  run_line "deploy/turn.py $A --system"
  run_line "deploy/turn.py $A \"Name three primary colours.\""
  want "validated; verify-load prints validate validated, load idle, N > 0 new lines, a load event naming composer, state_member and stack, a $AU-relay constituent, left serving, == $A verified"
  want "the load event carries no lineage and no reset, and names declaration, boundary and cause"
  want "a turnless message.system, a bracket with no message.user, then a bracket with message.user and message.assistant"
  want "the run marker names the load's run, open"
  want "admin.log and worker.log root:$AU-admin 640; $COORD/weaver.run and $RUN_DIR root:root 755; run.lock and admin.lock root:root 600; trace.sock root:$AU-admin 660 socket"
  want "$RUNTIME_DIR $AU:$AU 750; gate.sock $AU:$AU 770 socket; coordination.sock $AU:$AU 700 socket"
  want "state/state.log root:$AU-state 640 and readable by $AU-state (K12); $AU-state still cannot read the trace"
  [ "$APPLY" -eq 1 ] || return 0
  ask validate; expect_kind validate validated
  local rc=0 out="$LOGDIR/step2-verify-load.log" start
  start=$(trace_lines)
  sudo -n WEAVER_ADMIN_CONFIG="$ADMIN_BASE" WEAVER_STACK_RECORD="$STACK" bash "$CHECKOUT/deploy/verify-load.sh" "$A" --keep >"$out" 2>&1 || rc=$?
  sed 's/^/     /' "$out"
  expect_eq "verify-load.sh --keep exits 0" 0 "$rc"
  expect_eq "verify-load validates" yes "$(grep -qF 'validate    {"kind":"validated"}' "$out" && echo yes || echo no)"
  local loaded; loaded=$(sed -n 's/^   load        //p' "$out" | head -n 1)
  ANSWER=$loaded; expect_state "verify-load's load" idle
  local n; n=$(sed -n 's/^   events      \([0-9]*\) new lines$/\1/p' "$out")
  expect_eq "the load wrote events" yes "$( [ -n "$n" ] && [ "$n" -gt 0 ] && echo yes || echo no)"
  local ev; ev=$(grep '^   load event:' "$out" || true)
  for k in composer state_member stack; do
    expect_eq "the load event names $k" yes "$(grep -qF "\"$k\"" <<<"$ev" && echo yes || echo no)"
  done
  expect_eq "a relay stands among the constituents" yes "$(grep -qE "constituent .* $AU-relay " "$out" && echo yes || echo no)"
  expect_in_order "verify-load leaves the agent serving and verified" "$out" "left serving" "== $A verified"
  local load; load=$(last_event load)
  expect_eq "the first load carries no lineage" absent "$(json_at "$load" payload.lineage)"
  expect_eq "the first load carries no reset" absent "$(json_at "$load" payload.reset)"
  for k in declaration boundary cause; do
    expect_eq "the load event names its $k" present "$( [ "$(json_at "$load" "payload.$k")" != absent ] && echo present || echo absent)"
  done
  RUN=$(json_at "$load" payload.run)
  turn --system
  turn "Name three primary colours."
  local seq
  seq=$(events_since "$start" message.system turn.started message.user message.assistant turn.closed | python3 -c 'import json, sys
ks = [json.loads(l).get("kind") for l in sys.stdin if l.strip()]
brackets, cur = [], None
for k in ks:
    if k == "turn.started": cur = []
    elif k == "turn.closed":
        if cur is not None: brackets.append(cur)
        cur = None
    elif cur is not None: cur.append(k)
ok = (bool(ks) and ks[0] == "message.system" and len(brackets) >= 2
      and "message.user" not in brackets[0]
      and "message.user" in brackets[1] and "message.assistant" in brackets[1])
print("seeded, then one turn" if ok else " ".join(ks))')
  expect_eq "the seeding and the turn are on the record" "seeded, then one turn" "$seq"
  expect_marker "$RUN" open
  expect_lay "$T/admin.log" "root:$AU-admin 640 regular file"
  expect_lay "$T/worker.log" "root:$AU-admin 640 regular file"
  expect_lay "$COORD/weaver.run" "root:root 755 directory"
  expect_lay "$RUN_DIR" "root:root 755 directory"
  expect_lay "$RUN_DIR/run.lock" "root:root 600 regular file"
  expect_lay "$RUN_DIR/admin.lock" "root:root 600 regular file"
  expect_lay "$RUN_DIR/trace.sock" "root:$AU-admin 660 socket"
  expect_lay "$RUNTIME_DIR" "$AU:$AU 750 directory"
  expect_lay "$RUNTIME_DIR/gate.sock" "$AU:$AU 770 socket"
  expect_lay "$RUNTIME_DIR/coordination.sock" "$AU:$AU 700 socket"
  expect_lay "$T/state/state.log" "root:$AU-state 640 regular file"
  expect_eq "$AU-state reads its state.log (K12)" yes "$(as_can "$AU-state" tail -n 1 "$T/state/state.log")"
  expect_eq "$AU-state still cannot read the trace" no "$(as_can "$AU-state" test -r "$TRACE")"
  end_step
}

SP_A_NAME=""; SP_A_DIG=""
step_3() {
  begin_step 3 "weaver-admin-Spec section 6" "an on-demand save point"
  run_line "weaver-admin save-point $A"
  want "save_point_taken, its report naming the digest"
  want "the manifest holds one line: 1 demand <name> <digest>; the published file root:$AU-admin 640, the manifest root:root 644"
  want "the last save_point event names the digest and the operator's uid as its cause"
  [ "$APPLY" -eq 1 ] || return 0
  ask save-point; expect_kind save-point save_point_taken
  local dig; dig=$(json_at "$ANSWER" report.save_point)
  expect_eq "the manifest holds one line" 1 "$(manifest_count)"
  expect_eq "line 1 is ordinal 1" 1 "$(manifest_field 1 1)"
  expect_eq "line 1 arrived on demand" demand "$(manifest_field 1 2)"
  expect_eq "line 1 names the answer's digest" "$dig" "$(manifest_field 1 4)"
  SP_A_NAME=$(manifest_field 1 3); SP_A_DIG=$dig
  printf '     SP_A = %s\n' "$SP_A_NAME"
  expect_lay "$SAVE_POINTS/$SP_A_NAME" "root:$AU-admin 640 regular file"
  expect_lay "$MANIFEST" "root:root 644 regular file"
  local sp; sp=$(last_event save_point)
  expect_eq "the save_point event names the digest" "$dig" "$(json_at "$sp" payload.save_point)"
  expect_eq "the save_point event's cause is the operator" "$OPERATOR_UID" "$(json_at "$sp" payload.cause.uid)"
  end_step
}

step_4() {
  begin_step 4 "weaver-admin-Spec section 6; restore is a reload" "unload, then reload with state carried"
  run_line "weaver-admin unload $A; weaver-admin show $A; weaver-admin load $A"
  run_line "deploy/turn.py $A \"In one sentence, what were you asked to be?\""
  want "unloaded; the marker closed; a second manifest line arrived leave (SP_B); the unload event not forced; a save_point event for SP_B"
  want "show unloaded with no constituent; load idle; its lineage names SP_B and the previous run, named_at_restore false, no reset; a recall event after it"
  [ "$APPLY" -eq 1 ] || return 0
  ask unload; expect_state unload unloaded
  expect_marker "$RUN" closed
  expect_eq "the manifest holds two lines" 2 "$(manifest_count)"
  expect_eq "line 2 arrived at the leave" leave "$(manifest_field 2 2)"
  local spb; spb=$(manifest_field 2 4)
  expect_eq "the unload was not forced" false "$(json_at "$(last_event unload)" payload.forced)"
  expect_eq "the last save_point event names SP_B" "$spb" "$(json_at "$(last_event save_point)" payload.save_point)"
  expect_unloaded_alone
  local start; start=$(trace_lines)
  ask load; expect_state load idle
  local load; load=$(last_event load)
  expect_eq "the lineage names SP_B" "$spb" "$(json_at "$load" payload.lineage.save_point)"
  expect_eq "the lineage names the previous run" "$RUN" "$(json_at "$load" payload.lineage.run)"
  expect_eq "SP_B was not named at restore" false "$(json_at "$load" payload.lineage.named_at_restore)"
  expect_eq "the load carries no reset" absent "$(json_at "$load" payload.reset)"
  expect_eq "the member answered the restore (a recall event)" yes \
    "$( [ -n "$(events_since "$start" recall)" ] && echo yes || echo no)"
  RUN=$(json_at "$load" payload.run)
  turn "In one sentence, what were you asked to be?"
  end_step
}

step_5() {
  begin_step 5 "weaver-admin-Spec section 4; #99 N6" "restore an older save point, then load"
  run_line "weaver-admin unload $A"
  run_line "append [restore] save-point = \"<SP_A name>\" to $DECL (as sudoedit would; owner and mode kept)"
  run_line "weaver-admin restore $A; weaver-admin load $A"
  run_line "weaver-admin unload $A; weaver-admin load $A   ([restore] still present)"
  run_line "put $DECL back byte for byte; weaver-admin load $A"
  want "unloaded, a third line arrived leave; the declaration still root:$AU-admin 640"
  want "restore_named naming SP_A's digest and name; the manifest gains a restore line at ordinal 4 (N6)"
  want "load idle, its lineage SP_A with named_at_restore true"
  want "unload (a fifth line, leave); load with [restore] left behind answers config_invalid, admin's cause naming both save points (N6)"
  want "with [restore] removed the load is idle and continues from the newest line"
  [ "$APPLY" -eq 1 ] || return 0
  ask unload; expect_state unload unloaded
  expect_eq "the manifest holds three lines" 3 "$(manifest_count)"
  expect_eq "line 3 arrived at the leave" leave "$(manifest_field 3 2)"
  local orig; orig=$(sudo -n base64 -w0 -- "$DECL")
  { printf '%s' "$orig" | base64 -d; printf '\n[restore]\nsave-point = "%s"\n' "$SP_A_NAME"; } | sudo -n tee -- "$DECL" >/dev/null
  expect_lay "$DECL" "root:$AU-admin 640 regular file"
  ask restore; expect_kind restore restore_named
  expect_eq "restore names SP_A's digest" "$SP_A_DIG" "$(json_at "$ANSWER" save_point)"
  expect_eq "restore names SP_A's name" "$SP_A_NAME" "$(json_at "$ANSWER" name)"
  expect_eq "the manifest holds four lines" 4 "$(manifest_count)"
  expect_eq "line 4 is ordinal 4" 4 "$(manifest_field 4 1)"
  expect_eq "line 4 arrived at the restore (N6)" restore "$(manifest_field 4 2)"
  expect_eq "line 4 names SP_A" "$SP_A_DIG" "$(manifest_field 4 4)"
  ask load; expect_state load idle
  local load; load=$(last_event load)
  expect_eq "the lineage names SP_A" "$SP_A_DIG" "$(json_at "$load" payload.lineage.save_point)"
  expect_eq "SP_A was named at restore" true "$(json_at "$load" payload.lineage.named_at_restore)"
  ask unload; expect_state unload unloaded
  expect_eq "the manifest holds five lines" 5 "$(manifest_count)"
  expect_eq "line 5 arrived at the leave" leave "$(manifest_field 5 2)"
  local spd; spd=$(manifest_field 5 3)
  ask load; expect_kind "load with [restore] left behind" config_invalid
  expect_eq "admin's cause names SP_A (N6)" yes "$(grep -qF "$SP_A_NAME" <<<"$ASKED_CAUSE" && echo yes || echo no)"
  expect_eq "admin's cause names the newest save point (N6)" yes "$(grep -qF "$spd" <<<"$ASKED_CAUSE" && echo yes || echo no)"
  printf '%s' "$orig" | base64 -d | sudo -n tee -- "$DECL" >/dev/null
  expect_eq "the declaration is back byte for byte" "$orig" "$(sudo -n base64 -w0 -- "$DECL")"
  expect_lay "$DECL" "root:$AU-admin 640 regular file"
  ask load; expect_state load idle
  load=$(last_event load)
  expect_eq "the lineage names the newest line" "$(manifest_field last 4)" "$(json_at "$load" payload.lineage.save_point)"
  expect_eq "the newest was not named at restore" false "$(json_at "$load" payload.lineage.named_at_restore)"
  RUN=$(json_at "$load" payload.run)
  end_step
}

step_6() {
  begin_step 6 "weaver-admin-Spec section 3" "force-unload, then load"
  run_line "weaver-admin force-unload $A; weaver-admin load $A"
  want "unloaded; the marker forced; the unload event forced; no new manifest line"
  want "load idle; its reset {prior_run: the forced run, reason: forced-unload}; its lineage the manifest's latest"
  [ "$APPLY" -eq 1 ] || return 0
  local lines; lines=$(manifest_count)
  ask force-unload; expect_state force-unload unloaded
  expect_marker "$RUN" forced
  expect_eq "the unload event is forced" true "$(json_at "$(last_event unload)" payload.forced)"
  expect_eq "no manifest line was added" "$lines" "$(manifest_count)"
  ask load; expect_state load idle
  local load; load=$(last_event load)
  expect_eq "the reset names the forced run" "$RUN" "$(json_at "$load" payload.reset.prior_run)"
  expect_eq "the reset reason" forced-unload "$(json_at "$load" payload.reset.reason)"
  expect_eq "the lineage is the manifest's latest" "$(manifest_field last 4)" "$(json_at "$load" payload.lineage.save_point)"
  RUN=$(json_at "$load" payload.run)
  end_step
}

step_6b() {
  begin_step 6b "weaver-admin-Spec section 9; #99 K5" "force-unload on a territory that drifted while a run stands"
  run_line "with the run standing, the territory at 0711 (put back by the step and by the exit trap)"
  run_line "weaver-admin unload $A; weaver-admin force-unload $A; the territory put back"
  run_line "weaver-admin load $A"
  want "unload refuses boundary_unverified; force-unload answers unloaded; the marker forced; nothing published; no admin.log line written"
  want "show unloaded with no constituent; the territory root:$AU-state 710 again; load idle"
  [ "$APPLY" -eq 1 ] || return 0
  local tmode lines size
  tmode=$(sudo -n stat -c %a -- "$T")
  RESTORE=(restore_mode "$T" "$tmode")
  sudo -n chmod 0711 -- "$T" || { restore_now || true; fail "the territory takes 0711" "it applies" "chmod failed"; }
  ask unload
  local unload_answer=$ANSWER
  lines=$(manifest_count); size=$(file_size "$T/admin.log")
  ask force-unload
  local force_answer=$ANSWER
  restore_now || fail "the territory is put back" "chmod back to $tmode" "it failed"
  ANSWER=$unload_answer; expect_kind "unload on the drifted territory" boundary_unverified
  ANSWER=$force_answer; expect_state "force-unload on the drifted territory" unloaded
  expect_marker "$RUN" forced
  expect_eq "nothing was published" "$lines" "$(manifest_count)"
  expect_eq "no admin.log line was written" "$size" "$(file_size "$T/admin.log")"
  expect_unloaded_alone
  expect_lay "$T" "root:$AU-state 710 directory"
  ask load; expect_state load idle
  RUN=$(json_at "$(last_event load)" payload.run)
  end_step
}

step_7() {
  begin_step 7 "weaver-admin-Spec sections 3 and 4" "crash recovery"
  run_line "weaver-admin show $A; kill -9 the constituent whose comm is $WORKER_COMM and whose owner is $AU"
  run_line "weaver-admin show $A; where constituents remain: weaver-admin load $A, then unload $A"
  run_line "weaver-admin load $A"
  want "the marker open before; show after the kill unloaded, with or without constituents"
  want "where constituents remain, load answers agent_running and unload answers unloaded"
  want "the marker still open; load idle with reset {prior_run, reason: no-clean-unload} and its lineage the manifest's latest; any new manifest line arrived recovered"
  [ "$APPLY" -eq 1 ] || return 0
  expect_marker "$RUN" open
  ask show
  local pids pid worker=""
  pids=$(json_at "$ANSWER" constituents)
  for pid in $(python3 -c 'import json, sys; print(" ".join(map(str, json.loads(sys.argv[1]))))' "$pids" 2>/dev/null); do
    if [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = "$WORKER_COMM" ] && [ "$(stat -c %U "/proc/$pid" 2>/dev/null)" = "$AU" ]; then
      worker=$pid
    fi
  done
  [ -n "$worker" ] || fail "a worker stands among the constituents" "a $WORKER_COMM owned by $AU" "$pids"
  pass "the worker is $worker"
  local lines; lines=$(manifest_count)
  sudo -n kill -9 "$worker"
  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do [ -e "/proc/$worker" ] || break; sleep 1; done
  sleep 1
  ask show; expect_state "show after the kill" unloaded
  if [ "$(json_at "$ANSWER" constituents)" != absent ] && [ "$(json_at "$ANSWER" constituents)" != "[]" ]; then
    ask load; expect_kind "load while a constituent holds the lock" agent_running
    ask unload; expect_state unload unloaded
  else
    measured "no constituent outlived the worker, so load and unload were not asked"
  fi
  expect_marker "$RUN" open
  ask load; expect_state load idle
  local load; load=$(last_event load)
  expect_eq "the reset names the crashed run" "$RUN" "$(json_at "$load" payload.reset.prior_run)"
  expect_eq "the reset reason" no-clean-unload "$(json_at "$load" payload.reset.reason)"
  expect_eq "the lineage is the manifest's latest" "$(manifest_field last 4)" "$(json_at "$load" payload.lineage.save_point)"
  expect_eq "every new manifest line arrived recovered" "" \
    "$(manifest | tail -n +"$((lines + 1))" | awk '$2 != "recovered"')"
  RUN=$(json_at "$load" payload.run)
  end_step
}

step_8() {
  begin_step 8 "#99 N1, N5" "the layout migration, staged on $A"
  run_line "weaver-admin unload $A; weaver-admin show $A"
  run_line "move agent.toml, system-prompt.md, admin.log and worker.log from $T into $OLD_DIR ($OPERATOR's, 0600)"
  run_line "write $R/declaration-directory = $OLD_DIR; remove $R/territory; the territory setgid with an access entry u:nobody:x"
  run_line "weaver-admin show $A"
  run_line "deploy/update-stack.sh"
  run_line "deploy/update-stack.sh --install   (touches every agent on the box)"
  want "show refuses config_invalid with no territory key"
  want "the plan names $A's layout move; the install exits 0 through == migrate layout, the move, == verify (the load's idle answer read, N5), == the box is at"
  want "the territory root:$AU-state 710, not setgid, with no access entry; save-points/ 750; the four files root:$AU-admin 640 (N1)"
  want "$R holds territory and no declaration-directory; $OLD_DIR stands empty"
  [ "$APPLY" -eq 1 ] || return 0
  ask unload; expect_state unload unloaded
  expect_unloaded_alone
  ( umask 077; mkdir -p -- "$OLD_DIR" )
  local f
  for f in agent.toml system-prompt.md admin.log worker.log; do
    sudo -n mv -n -T -- "$T/$f" "$OLD_DIR/$f"
    sudo -n chown "$OPERATOR:$(id -gn)" -- "$OLD_DIR/$f"
    sudo -n chmod 0600 -- "$OLD_DIR/$f"
  done
  printf '%s\n' "$OLD_DIR" | sudo -n tee -- "$R/declaration-directory" >/dev/null
  sudo -n chmod 0644 -- "$R/declaration-directory"
  sudo -n rm -f -- "$R/territory"
  sudo -n chmod g+s -- "$T"
  sudo -n setfacl -m u:nobody:x -- "$T"
  ask show; expect_kind "show with no territory key" config_invalid
  local rc=0 out="$LOGDIR/step8-update-stack"
  ( cd "$CHECKOUT" && script_env bash deploy/update-stack.sh ) >"$out.plan" 2>&1 || rc=$?
  expect_eq "update-stack.sh's plan exits 0" 0 "$rc"
  expect_eq "the plan names $A's layout move" yes "$(grep -qE "^  $A +layout: " "$out.plan" && echo yes || echo no)"
  rc=0
  ( cd "$CHECKOUT" && script_env bash deploy/update-stack.sh --install ) >"$out.install" 2>&1 || rc=$?
  tail -n 40 "$out.install" | sed 's/^/     /'
  expect_eq "update-stack.sh --install exits 0" 0 "$rc"
  expect_in_order "the install migrates, verifies and finishes" "$out.install" \
    "== migrate layout" "  $A: declaration, draft and logs moved from" "== verify" "== the box is at"
  expect_eq "the reconcile validates $A" yes "$(grep -qE "^  $A +validated" "$out.install" && echo yes || echo no)"
  expect_lay "$T" "root:$AU-state 710 directory"
  expect_no_acl "$T" "$SAVE_POINTS"
  for f in agent.toml system-prompt.md admin.log worker.log; do expect_lay "$T/$f" "root:$AU-admin 640 regular file"; done
  expect_lay "$SAVE_POINTS" "root:$AU-admin 750 directory"
  expect_eq "the root's territory key names the territory" "$T" "$(trim "$(cat -- "$R/territory" 2>/dev/null)")"
  expect_eq "the root holds no declaration-directory" absent "$( [ -e "$R/declaration-directory" ] && echo present || echo absent)"
  expect_eq "$OLD_DIR stands empty" "" "$(ls -A -- "$OLD_DIR")"
  rmdir -- "$OLD_DIR"
  end_step
}

step_8b() {
  begin_step 8b "#99 H5, measured" "a read entry for the member on the trace, on the stopped agent"
  run_line "setfacl -m u:$AU-state:r $TRACE (put back with setfacl -b by the step and by the exit trap)"
  run_line "weaver-admin load $A; weaver-admin show $A; weaver-admin force-unload $A where it loaded"
  want "the trace still shows 640; the member reads it; the load is recorded as measured (today idle, H5 filed on #99, not fixed; after a fix, boundary_unverified)"
  want "the entry removed: no access entry on the trace, root:$AU-trace 640"
  [ "$APPLY" -eq 1 ] || return 0
  expect_unloaded_alone
  RESTORE=(restore_acl "$TRACE")
  sudo -n setfacl -m "u:$AU-state:r" -- "$TRACE" || { restore_now || true; fail "the entry applies" "setfacl succeeds" "it failed"; }
  local shown; shown=$(lay "$TRACE"); shown=${shown/regular empty file/regular file}
  local reads; reads=$( { [ "$(as_can "$AU-state" test -x "$T")" = yes ] && sudo -n -u "$AU-state" head -c 1 -- "$TRACE" >/dev/null 2>&1; } && echo yes || echo no)
  ask load
  local load_answer=$ANSWER
  if [ "$(json_at "$load_answer" kind)" = state ]; then
    ask show
    ask force-unload
    answered_state "$ANSWER" unloaded || { restore_now || true; fail "force-unload after the measured load" "the unloaded state" "$ANSWER"; }
  fi
  restore_now || fail "the entry is removed" "setfacl -b succeeds" "it failed"
  measured "with the entry the trace shows $shown"
  measured "the member reads the trace: $reads"
  measured "the load answered $load_answer"
  expect_no_acl "$TRACE"
  expect_lay "$TRACE" "root:$AU-trace 640 regular file"
  expect_unloaded_alone
  end_step
}

ARCHIVE_STAMP=$(date -u +%Y%m%dT%H%M%SZ)
TERRITORY_TAR="${ARCHIVE%/}/$AU-territory-$ARCHIVE_STAMP.tar"
ROOT_TAR="${ARCHIVE%/}/$AU-root-$ARCHIVE_STAMP.tar"
step_9() {
  begin_step 9 "HowToDeployANewAgent.md section 7" "take $A down alone, archived first"
  run_line "weaver-admin unload $A (force-unload where it refuses); weaver-admin show $A"
  run_line "rm -f $RULE (and any staged .$AU.* rule)"
  run_line "tar --acls --xattrs -C $AGENT_DIR -cpf ${ARCHIVE:-<--archive>}/$AU-territory-<stamp>.tar $AU"
  run_line "tar -C $ADMIN_BASE -cpf ${ARCHIVE:-<--archive>}/$AU-root-<stamp>.tar $A"
  run_line "rm -rf $R $STAGE $RUN_DIR $RUNTIME_DIR"
  run_line "userdel -r $AU; userdel $AU-state; userdel $AU-relay; userdel $AU-admincon"
  run_line "groupdel $GROUPS_MADE (each that stands)"
  run_line "rm -rf $T"
  run_line "rm -f $MARKER"
  want "show unloaded with no constituent before; afterwards show answers no_such_agent, as the operator and under sudo"
  want "the accounts, the groups, $R, $RULE and $T are absent; each archive reads back"
  want "never decommission.sh, which takes every agent off the box; no other agent is touched"
  [ "$APPLY" -eq 1 ] || return 0
  if sudo -n test -d "$R"; then
    ask unload
    if ! answered_state "$ANSWER" unloaded; then
      ask force-unload; expect_state force-unload unloaded
    else
      pass "unload answers the unloaded state"
    fi
    expect_unloaded_alone
  else
    measured "no root at $R, so no verb was asked"
  fi
  sudo -n rm -f -- "$RULE"
  sudo -n find /etc/sudoers.d -maxdepth 1 -type f -name ".$AU.*" -delete
  if sudo -n test -d "$T" && ! sudo -n test -L "$T"; then
    # Under umask 077, so the archive of the custodied territory is root's
    # 0600 and readable by no one the territory was closed to.
    sudo -n sh -c 'umask 077 && exec tar --acls --xattrs -C "$1" -cpf "$2" "$3"' sh "$AGENT_DIR" "$TERRITORY_TAR" "$AU"
    expect_eq "the territory's archive reads back" yes "$(sudo -n tar -tf "$TERRITORY_TAR" 2>/dev/null | grep -q . && echo yes || echo no)"
  else
    measured "no territory at $T to archive"
  fi
  if sudo -n test -d "$R"; then
    sudo -n sh -c 'umask 077 && exec tar -C "$1" -cpf "$2" "$3"' sh "$ADMIN_BASE" "$ROOT_TAR" "$A"
    expect_eq "the root's archive reads back" yes "$(sudo -n tar -tf "$ROOT_TAR" 2>/dev/null | grep -q . && echo yes || echo no)"
  else
    measured "no root at $R to archive"
  fi
  sudo -n rm -rf -- "$R" "$STAGE" "$RUN_DIR" "$RUNTIME_DIR"
  local u g
  if lookup passwd "$AU"; then sudo -n userdel -r "$AU"; fi
  for u in "$AU-state" "$AU-relay" "$AU-admincon"; do
    if lookup passwd "$u"; then sudo -n userdel "$u"; fi
  done
  for g in $GROUPS_MADE; do
    if lookup group "$g"; then sudo -n groupdel "$g"; fi
  done
  if sudo -n test -d "$T" && ! sudo -n test -L "$T"; then sudo -n rm -rf -- "$T"; fi
  local said
  said=$(WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$ADMIN" show "$A" 2>&1 | tail -n 1 || true)
  expect_eq "show as the operator answers no_such_agent" no_such_agent "$(json_at "$said" kind)"
  ask show; expect_kind "show under sudo" no_such_agent
  for u in $ACCOUNTS; do expect_eq "the account $u is gone" no "$(lookup passwd "$u" && echo yes || echo no)"; done
  for g in $GROUPS_MADE; do expect_eq "the group $g is gone" no "$(lookup group "$g" && echo yes || echo no)"; done
  local p
  for p in "$T" "$R" "$STAGE" "$RULE" "/home/$AU" "$RUN_DIR" "$RUNTIME_DIR"; do
    expect_eq "$p is gone" absent "$(lay "$p")"
  done
  sudo -n rm -f -- "$MARKER"
  CREATED=0
  end_step
}

manual_checks() {
  say "checks with no command today: do these by hand"
  plan "1. The member's state against the KV cache after a restore (steps 4 and 5): no command dumps the"
  plan "   member's held state or compares it with the decoder's cache, so judge the answer turn.py printed."
  plan "2. A published file's stamp position against the trace's save_point event sequence (step 3): only"
  plan "   the digest is compared here; read the stamp inside the published file and the event's sequence."
  plan "3. A loaded constituent's descriptor audit (step 2): verify-load.sh checks account and cgroup"
  plan "   only; list each constituent's /proc/<pid>/fd and confirm it holds no descriptor beyond its gifts."
}

# ------------------------------------------------------------ the run
say "verify-lifecycle for $A"
plan "checkout        $CHECKOUT"
plan "admin base      $ADMIN_BASE"
plan "stack record    $STACK"
plan "prefix          $PREFIX  (weaver-admin $ADMIN)"
plan "territory       $T"
plan "agent root      $R"
plan "creation mark   $MARKER"
plan "archive         ${ARCHIVE:-<--archive, default ~/.weaver-archive made 0700>}"
if [ "$CLEANUP" -eq 0 ]; then
  plan "artifact        ${ARTIFACT:-<--artifact, required with --apply>}"
  plan "migration       $( [ "$WITH_MIGRATION" -eq 1 ] && echo 'on: step 8 runs update-stack.sh --install, which touches every agent on the box' || echo 'off (steps 8 and 8b skipped)')"
  if [ -n "$OTHERS" ]; then
    plan "NOTE: the admin base holds other agents ($OTHERS); --apply refuses unless --allow-other-agents"
  fi
fi

if [ "$APPLY" -eq 1 ]; then
  sudo -v || die "--apply needs sudo"
  # The looks that need root, made before anything acts.
  if [ "$CLEANUP" -eq 0 ]; then
    if sudo -n test -e "$RULE"; then die "the sudo rule $RULE already exists: this script verifies a throwaway agent it creates at step 1"; fi
  fi
  for t in "$TERRITORY_TAR" "$ROOT_TAR"; do
    if sudo -n test -e "$t" || sudo -n test -L "$t"; then die "the archive $t already exists, and no archive is written over another"; fi
  done
  LOGDIR=$(mktemp -d "${TMPDIR:-/tmp}/verify-lifecycle-$A.XXXXXX")
fi

if [ "$CLEANUP" -eq 1 ]; then
  CREATED=1
  step_9
  say "$( [ "$APPLY" -eq 1 ] && echo "$A taken down" || echo "plan only: rerun with --archive <dir> --apply to take $A down")"
  exit 0
fi

step_0
step_1
step_1b
step_2
step_3
step_4
step_5
step_6
step_6b
step_7
if [ "$WITH_MIGRATION" -eq 1 ]; then
  step_8
  step_8b
else
  skip 8 "the layout migration runs update-stack.sh --install, which touches every agent on the box; pass --with-migration to run it"
  skip 8b "it measures the migrated agent with step 8; pass --with-migration to run it"
fi
step_9
manual_checks
if [ "$APPLY" -eq 1 ]; then
  say "PASS: every step of $A's lifecycle answered as expected"
else
  say "plan only"
  plan "nothing was acted on; rerun with --artifact <path> --archive <dir> --apply to act"
fi
