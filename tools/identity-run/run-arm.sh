#!/usr/bin/env bash
# One arm of the identity run (act 2's brief, docs/project/brief-2026-11-26-the-loop-process.md
# section 8, by weaver-harness-loop-contract section 6), on an agent appliance, by root,
# who is the operator there. It plans by default and acts with --apply.
#
#   run-arm.sh --arm compiled --agent NAME --artifact MODEL.gguf --out DIR
#   run-arm.sh --arm wires    --agent NAME --artifact MODEL.gguf --out DIR --wires FILE --apply
#   run-arm.sh --arm negative --agent NAME --artifact MODEL.gguf --out DIR --wires FILE --apply
#
# Options: --deploy DIR (the deploy scripts, default the deploy/ folder of the tree this
# script stands in; a deployed copy holds what deploy/deployed-copy.txt lists),
# --admin PATH (default /opt/weaver/bin/weaver-admin), --context-capacity N (default
# 8192, the same in every arm, so pressure comes within a few turns).
#
# **What it does, in order**, each step printed before it is taken:
#
#  1. create-agent.sh makes the agent, continuous, with the declarations every arm shares.
#  2. The operator's uid, 0, joins the gate's access rule, so the seeding turn has a
#     sender (deploy/turn.py: it works only where the rule names the operator). Every arm
#     makes the same edit, so the declaration's digest is the same in each.
#  3. The wires and negative arms lay loop/ and wires.py by hand (the brief, section 2),
#     root:weaver-NAME 0750 and 0640, and admin's validate must answer validated.
#  4. Six loads, each answered idle, each unloaded gracefully, answered unloaded:
#     load 1, continuous, nothing seated: a first turn, a later turn, then turns until the
#       last answer's fullness passes four-fifths of capacity, and one turn more, under
#       pressure (the flush and the recall)
#     load 2: a first turn with an earlier run (the continuity line)
#     load 3, the declaration set to independent: a turn with nothing seated
#     load 4, set back to continuous: the seeding turn (deploy/turn.py NAME --system)
#     load 5: a first turn with the prompt seated
#     load 6, set to independent: a turn with the prompt seated.
#     After each load the process table is read into DIR/steps, a reading for the
#     procedure: the runner under the worker, as the agent's account.
#  5. The record, worker.log and every save point are copied to DIR, with arm.json for
#     compare.py: the arm, the agent, the loads, the turns that reach the loop (every turn
#     but the seeding one, which is the harness's own), and the laid file's sha256.
#  6. take-down.sh, beside this script, archives the arm's agent into DIR/archive as
#     root and takes that one agent down by the runbook's section 7, leaving the stack,
#     its record and the machine's own accounts standing, so the next arm is made on the
#     same install.
#
# Every turn is checked answered under its own key, t-1 onward in each load. A step that
# fails stops the arm: the agent is unloaded if it may stand, what stands is copied to
# DIR/failed, and nothing is taken down, so the operator reads it first. Keep the
# console: `--apply 2>&1 | tee DIR.console`.
set -uo pipefail

die() { printf 'run-arm: %s\n' "$*" >&2; exit 2; }
HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || die "cannot find the script's own folder"

ARM= AGENT= ARTIFACT= OUT= WIRES= APPLY=0
DEPLOY=$(cd -- "$HERE/../../deploy" 2>/dev/null && pwd) || DEPLOY="$HERE/../../deploy"
ADMIN=/opt/weaver/bin/weaver-admin
CAPACITY=8192
while [ $# -gt 0 ]; do
    case $1 in
        --arm)              [ $# -ge 2 ] || die "--arm needs compiled, wires or negative"; ARM=$2; shift ;;
        --agent)            [ $# -ge 2 ] || die "--agent needs a name"; AGENT=$2; shift ;;
        --artifact)         [ $# -ge 2 ] || die "--artifact needs the model's path"; ARTIFACT=$2; shift ;;
        --out)              [ $# -ge 2 ] || die "--out needs a folder"; OUT=$2; shift ;;
        --wires)            [ $# -ge 2 ] || die "--wires needs the wire file"; WIRES=$2; shift ;;
        --deploy)           [ $# -ge 2 ] || die "--deploy needs a folder"; DEPLOY=$2; shift ;;
        --admin)            [ $# -ge 2 ] || die "--admin needs a path"; ADMIN=$2; shift ;;
        --context-capacity) [ $# -ge 2 ] || die "--context-capacity needs a count"; CAPACITY=$2; shift ;;
        --apply)            APPLY=1 ;;
        *)                  die "unknown argument: $1" ;;
    esac
    shift
done
case $ARM in compiled|wires|negative) ;; *) die "--arm is compiled, wires or negative: '$ARM'" ;; esac
[[ "$AGENT" =~ ^[a-z][a-z0-9]{1,15}$ ]] || die "--agent is an agent's name, a lowercase letter then 1 to 15 lowercase letters or digits: '$AGENT'"
[ -n "$ARTIFACT" ] || die "--artifact is required"
[ -n "$OUT" ] || die "--out is required"
# **The arm's folder is absolute from here**: take-down.sh refuses a relative archive,
# and the take-down is the arm's last step, so a relative --out would fail only after
# every load and turn had run, with the agent left standing.
case $OUT in
    /*) ;;
    *) OUT=$PWD/$OUT ;;
esac
[[ "$CAPACITY" =~ ^[0-9]+$ ]] || die "--context-capacity is a count: '$CAPACITY'"
WIRES_SHA=
if [ "$ARM" = compiled ]; then
    [ -z "$WIRES" ] || die "a compiled arm lays no wire file"
else
    [ -r "$WIRES" ] || die "--wires names no readable file: '$WIRES'"
    WIRES_SHA=$(sha256sum <"$WIRES" | cut -d' ' -f1)
fi

BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
LOAD_BOUND=900
TURN_BOUND=900
PRESSURE_MOST=40
FULLNESS_BOUND=30

# The asks, sent in this order in every arm, one each turn but the seeding one.
ASKS=(
    "Explain how a lock and key mechanism works, in about two hundred words."
    "Now explain how a combination lock differs from it."
    "Describe the water cycle step by step."
    "Describe how a bicycle's gears change the effort of pedalling."
    "Explain what a compiler does, from source text to a running program."
    "Describe how bread rises, and what changes when it bakes."
    "Explain how a refrigerator moves heat out of its box."
    "Describe how a library catalogue helps a reader find a book."
    "Explain why the sky looks blue in the day and red at sunset."
    "Describe how a river shapes the valley it runs through."
    "Explain how a pendulum clock keeps time."
    "Describe how a seed becomes a plant."
)

STEP=0 LOADS=0 TURN_IN_LOAD=0 TURNS_TO_LOOP=0 ASKED=0 LOADED=0 TERRITORY= TRACE= TAKING_DOWN=0 TAKE_DOWN_RC=
step() { STEP=$((STEP + 1)); printf '\n== step %d: %s\n' "$STEP" "$*"; }
shown() { printf '+'; printf ' %q' "$@"; printf '\n'; }
act() { shown "$@"; [ "$APPLY" = 1 ] || return 0; "$@"; }

# take_down_said RC OUT: what a take-down that did not finish leaves, by its exit code.
# take-down.sh exits 2 where it refuses in its step 1, before anything is removed, and 1
# where it stops partway, what its earlier steps removed staying removed.
take_down_said() {
    case $1 in
        2) printf 'the take-down refused in its step 1 and removed nothing: the agent stands, unloaded, and the arm is copied to %s. Read its refusal above, then take the agent down by hand or run take-down.sh again\n' "$2" ;;
        *) printf 'the take-down stopped partway: its STOPPED line above names the step, the arm is copied to %s and the archive is %s/archive. Finish by hand by deploy/HowToDeployANewAgent.md section 7: once its step 4 has removed the root, a second take-down refuses at its step 1\n' "$2" "$2" ;;
    esac
}

fail() {
    printf '\nFAILED at step %d: %s\n' "$STEP" "$*"
    [ "$APPLY" = 1 ] || exit 1
    # The take-down refuses before anything, or removes piece by piece and stops at the
    # first step that fails: its exit code says which.
    if [ "$TAKING_DOWN" = 1 ]; then
        take_down_said "$TAKE_DOWN_RC" "$OUT"
        exit 1
    fi
    if [ "$LOADED" = 1 ]; then
        timeout "$LOAD_BOUND" "$ADMIN" unload "$AGENT" || true
    fi
    if [ -n "$TERRITORY" ]; then
        mkdir -p "$OUT/failed" && cp -a -- "$TERRITORY/worker.log" "$OUT/failed/" 2>/dev/null
        [ -z "$TRACE" ] || cp -a -- "$TRACE" "$OUT/failed/" 2>/dev/null
    fi
    printf 'nothing is taken down: read %s/failed, then take the agent down with take-down.sh\n' "$OUT"
    exit 1
}

# answered_as FILE KIND KEY: the last line of FILE is one JSON object of KIND, and, where
# KEY is given, under the turn KEY (for a state answer, KEY is the state).
answered_as() {
    python3 -I - "$@" <<'ANSWERED'
import json, sys
lines = [l for l in open(sys.argv[1], encoding="utf-8", errors="replace").read().splitlines() if l.strip()]
try:
    d = json.loads(lines[-1])
except (IndexError, ValueError):
    print("   no JSON answer"); sys.exit(1)
kind, key = sys.argv[2], sys.argv[3]
field = "state" if kind == "state" else "turn"
if not isinstance(d, dict) or d.get("kind") != kind or d.get(field) != key:
    print("   answered kind %r, %s %r; wanted %s, %s" % (d.get("kind"), field, d.get(field), kind, key)); sys.exit(1)
ANSWERED
}

admin_state() {  # VERB STATE
    shown timeout "$LOAD_BOUND" "$ADMIN" "$1" "$AGENT"
    printf '   must answer state %s\n' "$2"
    [ "$APPLY" = 1 ] || return 0
    local answer="$OUT/steps/$STEP-$1.out"
    timeout "$LOAD_BOUND" "$ADMIN" "$1" "$AGENT" >"$answer" 2>&1
    cat -- "$answer"
    answered_as "$answer" state "$2" || fail "$1 did not answer state $2"
}

load() {
    LOADS=$((LOADS + 1)); TURN_IN_LOAD=0
    step "load $LOADS: $*"
    admin_state load idle
    LOADED=1
    # A reading, not a check: the runner under the worker, as the agent's account.
    printf '   the process table read into steps/%d-processes.out\n' "$STEP"
    [ "$APPLY" = 1 ] || return 0
    ps -eo pid,ppid,user:32,args >"$OUT/steps/$STEP-processes.out" 2>&1 || true
}

unload() {
    step "unload $LOADS"
    admin_state unload unloaded
    LOADED=0
}

turn() {  # one ask through the gate as the gate connector's account
    TURN_IN_LOAD=$((TURN_IN_LOAD + 1)); TURNS_TO_LOOP=$((TURNS_TO_LOOP + 1))
    local text=${ASKS[$((ASKED % ${#ASKS[@]}))]}; ASKED=$((ASKED + 1))
    shown timeout "$TURN_BOUND" runuser -u weaver-gatecon -- python3 -I - "$AGENT" "$text" --raw
    printf '   reads %s/turn.py on its standard input, and must answer kind answered, turn t-%d\n' "$DEPLOY" "$TURN_IN_LOAD"
    [ "$APPLY" = 1 ] || return 0
    local answer="$OUT/steps/$STEP-turn-$TURN_IN_LOAD.out"
    timeout "$TURN_BOUND" runuser -u weaver-gatecon -- python3 -I - "$AGENT" "$text" --raw \
        <"$DEPLOY/turn.py" >"$answer" 2>&1
    answered_as "$answer" answered "t-$TURN_IN_LOAD" || fail "turn t-$TURN_IN_LOAD of load $LOADS was not answered"
}

seed() {  # the seeding turn, from the operator's uid, the harness's own, not the loop's
    TURN_IN_LOAD=$((TURN_IN_LOAD + 1))
    shown timeout "$TURN_BOUND" python3 -I "$DEPLOY/turn.py" "$AGENT" --system --raw
    printf '   must answer kind answered, turn t-%d\n' "$TURN_IN_LOAD"
    [ "$APPLY" = 1 ] || return 0
    local answer="$OUT/steps/$STEP-seed.out"
    timeout "$TURN_BOUND" python3 -I "$DEPLOY/turn.py" "$AGENT" --system --raw >"$answer" 2>&1
    answered_as "$answer" answered "t-$TURN_IN_LOAD" || fail "the seeding turn was not answered"
}

# declaration EDIT: one edit of the territory's agent.toml, its owner, group and mode kept.
# `turn-context=WORD` sets the election, and `operator-admitted` adds uid 0 to allowed-uids.
declaration() {
    shown declaration-edit "<territory>/agent.toml" "$1"
    [ "$APPLY" = 1 ] || return 0
    python3 -I - "$TERRITORY/agent.toml" "$1" <<'DECLARATION' || fail "the declaration edit $1"
import os, re, sys
path, edit = sys.argv[1], sys.argv[2]
text = open(path, encoding="utf-8").read()
if edit.startswith("turn-context="):
    word = edit.split("=", 1)[1]
    new, n = re.subn(r'(?m)^turn-context = "[a-z]+"$', 'turn-context = "%s"' % word, text)
elif edit == "operator-admitted":
    new, n = re.subn(r'(?m)^allowed-uids = \[(\d+)\]$', r'allowed-uids = [\1, 0]', text)
else:
    sys.exit("no such edit")
if n != 1:
    sys.exit("the line to edit stands %d times" % n)
st = os.stat(path)
part = path + ".identity-run"
with open(part, "w", encoding="utf-8") as f:
    f.write(new)
os.chown(part, st.st_uid, st.st_gid)
os.chmod(part, st.st_mode & 0o7777)
os.replace(part, path)
DECLARATION
}

# The fullness reading after the turn just answered (fullness.py beside this script): it
# waits, up to FULLNESS_BOUND seconds, until the record holds that turn's close, since the
# record lags the answer, and reads only ended lines. 0 full enough, 1 not yet; anything
# else fails the arm, which then names why.
until_pressure() {
    step "load $LOADS: turns until fullness passes four-fifths of capacity, then one under pressure (at most $PRESSURE_MOST)"
    printf '   after each turn: python3 -I %s/fullness.py <trace> t-<n> %s, waiting for the turn'"'"'s close\n' "$HERE" "$FULLNESS_BOUND"
    if [ "$APPLY" != 1 ]; then
        printf '   each a turn as above, then the reading; then one turn more\n'
        return 0
    fi
    local n=0 said
    while :; do
        python3 -I "$HERE/fullness.py" "$TRACE" "t-$TURN_IN_LOAD" "$FULLNESS_BOUND"; said=$?
        case $said in
            0) break ;;
            1) ;;
            *) fail "the fullness reading after turn t-$TURN_IN_LOAD of load $LOADS (exit $said)" ;;
        esac
        n=$((n + 1)); [ "$n" -le "$PRESSURE_MOST" ] || fail "no pressure within $PRESSURE_MOST turns"
        turn
    done
    turn
}

copy_out() {
    step "the record, worker.log and the save points copied to $OUT, with arm.json"
    shown cp "<trace>" "<territory>/worker.log" "<territory>/save-points/*.save-point" "$OUT"
    [ "$APPLY" = 1 ] || return 0
    mkdir -p "$OUT/save-points" || fail "the copy's folder"
    cp -- "$TRACE" "$OUT/trace.ndjson" && cp -- "$TERRITORY/worker.log" "$OUT/worker.log" \
        && cp -- "$TERRITORY"/save-points/*.save-point "$OUT/save-points/" || fail "the copy"
    python3 -I - "$OUT/arm.json" "$ARM" "$AGENT" "$LOADS" "$TURNS_TO_LOOP" "$WIRES_SHA" <<'ARMJSON' || fail "arm.json"
import json, sys
out, arm, agent, loads, turns, sha = sys.argv[1:]
meta = {"arm": arm, "agent": agent, "loads": int(loads), "turns_to_loop": int(turns)}
if sha:
    meta["wires_sha256"] = sha
json.dump(meta, open(out, "w"))
ARMJSON
    (cd "$OUT" && sha256sum trace.ndjson worker.log save-points/* >SHA256SUMS) || fail "the sums"
}

# ---------------------------------------------------------------- the arm
printf 'identity run, %s arm, agent %s%s: %s\n' "$ARM" "$AGENT" "${WIRES_SHA:+, wire file $WIRES_SHA}" \
    "$([ "$APPLY" = 1 ] && echo acting || echo 'plan only (--apply acts)')"
if [ "$APPLY" = 1 ]; then
    # create-agent.sh refuses --root-operator through sudo from another account, so the
    # arm refuses it first, in its words, before anything is touched.
    [ -z "${SUDO_USER:-}" ] || [ "$SUDO_USER" = root ] \
        || die "--root-operator is for root's own login on a single-purpose appliance, not sudo from $SUDO_USER: run the identity run from root's own login"
    [ "$(id -u)" = 0 ] || die "--apply is root's, the operator on an agent appliance"
    [ ! -e "$OUT" ] || die "$OUT stands: each arm writes a folder of its own"
    [ -z "$(ls -A -- "$BASE" 2>/dev/null)" ] || die "an agent stands under $BASE: the run needs an appliance holding none (take-down.sh first)"
    for f in create-agent.sh turn.py; do [ -r "$DEPLOY/$f" ] || die "$DEPLOY/$f is not there"; done
    for f in take-down.sh fullness.py; do [ -r "$HERE/$f" ] || die "$HERE/$f is not there"; done
    [ -x "$ADMIN" ] || die "$ADMIN is not there"
    mkdir -p "$OUT/steps" || die "cannot make $OUT"
fi

step "create the agent"
act "$DEPLOY/create-agent.sh" "$AGENT" --engine sqlite --artifact "$ARTIFACT" \
    --context-capacity "$CAPACITY" --turn-context continuous --root-operator --apply \
    || fail "create-agent.sh"
if [ "$APPLY" = 1 ]; then
    TERRITORY=$(tr -d '[:space:]' <"$BASE/$AGENT/territory") || fail "the root names no territory"
    TRACE=$(python3 -I -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["trace-sink"]["path"])' \
        "$TERRITORY/agent.toml") || fail "the declaration names no trace sink"
fi

step "admit the operator's uid at the gate, for the seeding turn"
declaration operator-admitted

if [ "$ARM" != compiled ]; then
    step "lay the loop by hand"
    act install -d -o root -g "weaver-$AGENT" -m 0750 "/etc/weaver/$AGENT/loop" || fail "loop/"
    act install -o root -g "weaver-$AGENT" -m 0640 "$WIRES" "/etc/weaver/$AGENT/loop/wires.py" || fail "wires.py"
    step "admin judges the loop"
    shown "$ADMIN" validate "$AGENT"
    printf '   must answer {"kind":"validated"}\n'
    if [ "$APPLY" = 1 ]; then
        [ "$("$ADMIN" validate "$AGENT" 2>&1 | tail -n 1)" = '{"kind":"validated"}' ] || fail "validate"
    fi
fi

load "continuous, nothing seated"
turn
turn
until_pressure
unload
load "a first turn with an earlier run"
turn
unload
step "the declaration set to independent"
declaration turn-context=independent
load "independent, nothing seated"
turn
unload
step "the declaration set back to continuous"
declaration turn-context=continuous
load "the seeding turn"
seed
unload
load "a first turn with the prompt seated"
turn
unload
step "the declaration set to independent"
declaration turn-context=independent
load "independent, the prompt seated"
turn
unload

copy_out
step "take the arm's agent off the appliance, the stack left standing"
TAKING_DOWN=1
act "$HERE/take-down.sh" --agent "$AGENT" --archive "$OUT/archive" --admin "$ADMIN" --apply \
    || { TAKE_DOWN_RC=$?; fail "the take-down, exit $TAKE_DOWN_RC"; }
printf '\nidentity run, %s arm: done, %d loads, %s turns to the loop\n' "$ARM" "$LOADS" \
    "$([ "$APPLY" = 1 ] && echo "$TURNS_TO_LOOP" || echo "$TURNS_TO_LOOP (plan: before the pressure turns)")"
