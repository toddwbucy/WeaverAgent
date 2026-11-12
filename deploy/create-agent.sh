#!/usr/bin/env bash
# Create one agent on this box: its accounts, its territory with the state
# store behind its member's seam, the declaration and the prompt draft in it,
# its root of admin configuration, and the sudo rule the box's admin-con
# account runs its verbs through, laying the box's two connector accounts
# where they are absent.
#
#   ./deploy/create-agent.sh fred --engine sqlite --artifact /path/to.gguf            plan only
#   ./deploy/create-agent.sh fred --engine sqlite --artifact /path/to.gguf --apply    act
#
# Options: --engine sqlite (the default, and the one engine since the service
# engine retired on the operator's ruling of 2026-10-02 on #1), --session <name>
# (default <name>-001), --spu <path> (this agent's SPU, in place of the
# stack record's `spu-binary`), --connector-role operator|observer (default
# operator: which command lines the agent's sudo rule grants the box's
# admin-con account), and
# --adapter <path> --adapter-sha256 <hex> --adapter-scale <number>
# --base-blake3 <hex>, all four or none, which write the binding's `adapter`
# table (weaver-spu adapter binding amendment ADP-1 and ADP-5).
#
# **The adapter's digests are the operator's declared claims**, taken from the
# conversion record and the fixture report, and this script computes nothing:
# the SPU checks both at admission (ADP-2). The adapter file takes the base's
# custody: owned outside the agent's uid, readable by it, not writable by it.
#
# Run it as the operator, never under sudo: the root's `operator` key names
# the uid this script runs as, and `--apply` asks for sudo itself for every
# root step. Nothing is written into the operator's home.
#
# **The agent's root is `<admin base>/<name>/` and it is the admission.**
# Admin reads that directory and nothing shared, so this script writes every
# key admin requires into it, copied from the stack record `bootstrap-stack.sh`
# wrote at `/etc/weaver/stack/` (which admin never reads), plus the agent's own
# `territory`, `operator` and `roles.toml`, per weaver-admin-Spec section 9.
# The root is staged under a dot-name admin's name check refuses, and moved
# into place last, so a run that stops part way leaves no root admin would
# admit.
#
# **The whole agent lives in its territory** (the operator's ruling of
# 2026-10-07 on #1): `<agent-directory>/weaver-<name>/`, root's, holds the
# declaration `agent.toml` (root:weaver-<name>-admin 0640, which the
# operator edits with `sudoedit`), the prompt draft `system-prompt.md`
# (root:weaver-<name>-admin 0640), admin's own
# `admin.log` and `worker.log` once a verb has run, `save-points/` for the
# published save points and their manifest, beside the state room and the
# trace. Root reads and writes root's files in root's directory, so no file
# admin writes stands where another principal could choose it. The territory
# itself is the state group's, `weaver-<name>-state` at 0710, which gives
# passage and lists nothing (the operator's ruling of 2026-10-08 on #1). The
# access group `weaver-<name>-admin` groups the files beneath it, the
# declaration, the draft, the logs and `save-points/` with the published save
# points, and reads them and never writes, so the admin-con account reads the
# declaration and a save point and can rewrite neither. The operator joins
# both groups to read without sudo.
#
# **The system prompt is state and not a field** (operator's ruling of
# 2026-10-06): the declaration carries no identity, admin reads no prompt, and
# the prompt enters the agent through its gate. This script still writes the
# draft, `system-prompt.md` beside the declaration, and never opens the gate.
# An agent it lays has no seeding sender: its gate admits the box's gate-con
# account alone, and the harness admits a seeding line from the operator's uid
# alone, so how its prompt enters is deferred to its own act
# (weaver-gate-world-contract section 2).
#
# **Two accounts and two groups beyond the agent's own**, per
# weaver-admin-Spec sections 4, 6 and 9: the state member
# `weaver-<name>-state`, the trace relay `weaver-<name>-relay` whose one group
# is the trace group `weaver-<name>-trace`, and the access group
# `weaver-<name>-admin` the trace door is grouped to.
#
# **The box's two connector accounts, one per plane** (weaver-admin-Spec
# section 2 and weaver-gate-world-contract section 1, the operator's rulings
# of 2026-11-10): `weaver-gatecon`, which runs WeaverWeb's gate-con and is the
# one uid the access rule names, joins the agent's own group, through which
# the gate's socket admits it. `weaver-admincon`, which runs admin-con, joins
# the state and access groups, is the boundary file's `trace-reader`, and is
# the one user the sudo rule names. Each is laid where it is absent and, where
# an account of the name stands, judged before it is used: a system account
# with no login shell, no home, a primary group of its own, and no group that
# reaches the other account's door. Neither is ever laid per agent, and the
# names `gatecon` and `admincon` are refused as agents' (weaver-admin-Spec
# section 4). The rule naming one caller is this script's convention, and
# nothing refuses a rule naming more (#70).
#
# **The sudo rule is strict** (weaver-admin-Spec section 2, the operator
# contract section 2): `/etc/sudoers.d/weaver-<name>` grants the admin-con
# account exactly this agent's fixed `weaver-admin <verb> <name>` command lines,
# as root, without a password, with no caller-chosen argument, no SETENV and
# no env_keep, and turns sudo's `pam_session` off for that user so a session
# never moves the invocation into another containment. Since #166 admin's
# start step moves a load into the agent's own scope (weaver-admin-Spec 6.1),
# leaving any session's containment, so a session would no longer break the
# agent's lifetime. It stays off because none is needed and the granted rule
# does not change (the operator contract section 2). The
# observer role grants `show`, and the operator role adds
# `validate`, `load`, `unload`, `stop`, `save-point`, `restore` and
# `force-unload`, the eight lines `VERBS` below names. The rule is checked with `visudo
# -cf` before it is installed. A delegated invocation reads admin's default
# base, sudo stripping WEAVER_ADMIN_CONFIG, so the rule drives an agent only
# under `/etc/weaver/admin`.
#
# **The program creates nothing and this is why the script exists.** Admin's
# inventory refuses a missing home rather than building one, per
# `weaver-admin-Spec` section 4, so every part of an agent's territory is the
# operator's to make. This file is that act written down once instead of
# remembered.
#
# **Two accounts, because the wall is the room and one identity**, per
# `weaver-state-PRD` section 4: the member writes its save points into a state
# room it owns, and the agent's own uid cannot enter that room. That wall is
# verified at the end rather than assumed. No password exists anywhere in here.
#
# **The member's account is derived and no longer named**, as of 2026-09-15 and
# issue #545. `weaver-admin` resolves `weaver-<name>-state`, drops to it at the
# member's spawn and hands it the territory by chown, so this script cannot
# choose a weaker account. The `--member-identity` flag that named the choice
# while the code ran the member as root is refused rather than ignored.
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

NAME=${1:-}
shift || true
APPLY=0
ARTIFACT=""
SESSION=""
# **The engine is an election and not a constant**, written into the
# declaration, which `deploy/update-stack.sh` reconciles against what the build
# carries before it spends a build. **The embedded engine is the default**: it
# is the one engine this build provides, on the operator's rulings of
# 2026-10-02 on #1 (reversing #38, whose "no engine is the default" rested on
# two engines standing side by side) and 2026-10-05 (#86).
ENGINE=sqlite
SPU_OVERRIDE=""
CONNECTOR_ROLE=operator
ADAPTER=""
ADAPTER_SHA256=""
ADAPTER_SCALE=""
BASE_BLAKE3=""
while [ $# -gt 0 ]; do
  case "$1" in
    --apply)    APPLY=1 ;;
    # **The value is required before the shift consumes it.** Without the
    # arity check the inline shift runs past the end of the list and `set -e`
    # exits with no message at all, which is a worse answer than the refusal
    # below.
    --artifact) [ $# -ge 2 ] || die "--artifact needs a path"; ARTIFACT=$2; shift ;;
    --session)  [ $# -ge 2 ] || die "--session needs a name"; SESSION=$2; shift ;;
    --member-identity) die "--member-identity is retired as of 2026-09-15, issue #545. The
   member's account is weaver-<name>-state, derived by weaver-admin from the
   agent's name. An agent made before this date is recreated with create-agent.sh
   after its take-down by deploy/HowToDeployANewAgent.md section 7." ;;
    --engine)   [ $# -ge 2 ] || die "--engine needs a name"; ENGINE=$2; shift ;;
    --spu)      [ $# -ge 2 ] || die "--spu needs a path"; SPU_OVERRIDE=$2; shift ;;
    --declaration-directory) die "--declaration-directory is retired on the operator's ruling of 2026-10-07 on #1:
   the declaration lives in the agent's territory, <agent-directory>/weaver-<name>/agent.toml,
   root's, edited with sudoedit, and nothing is written into the operator's home." ;;
    --connector-role) [ $# -ge 2 ] || die "--connector-role needs operator or observer"; CONNECTOR_ROLE=$2; shift ;;
    --adapter)        [ $# -ge 2 ] || die "--adapter needs a path"; ADAPTER=$2; shift ;;
    --adapter-sha256) [ $# -ge 2 ] || die "--adapter-sha256 needs a digest"; ADAPTER_SHA256=$2; shift ;;
    --adapter-scale)  [ $# -ge 2 ] || die "--adapter-scale needs a number"; ADAPTER_SCALE=$2; shift ;;
    --base-blake3)    [ $# -ge 2 ] || die "--base-blake3 needs a digest"; BASE_BLAKE3=$2; shift ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done


# The name is a unix user, a group and a directory, so it is bounded to what
# all three accept without quoting.
[ -n "$NAME" ] || die "name the agent: create-agent.sh <name> --artifact <path>"
[[ "$NAME" =~ ^[a-z][a-z0-9]{1,15}$ ]] || die "the name is lowercase letters and digits, 2 to 16 characters: '$NAME'"
# **The box's connector accounts' names are not agents'** (weaver-admin-Spec
# section 4): an agent named gatecon would own weaver-gatecon, the caller of
# every gate on the box, as its own identity. Admin refuses both names too.
case "$NAME" in
  gatecon|admincon) die "'$NAME' is reserved: weaver-$NAME is the box's $NAME account (weaver-admin-Spec section 4), so no agent takes the name" ;;
esac
[ -n "$ARTIFACT" ] || die "name the artifact the decoder binds: --artifact <path>"
SESSION=${SESSION:-$NAME-001}
# **The session and the artifact are written into TOML strings, so a value a
# basic string cannot carry as it stands is refused here**, before any
# account, database or file is made. A double quote ends the string early, a
# backslash begins an escape that changes the value, and a control character
# is either refused by TOML or ends the line, so `--session 'trial"2'` would
# write `session = "trial"2"`. Neither value needs any of the three. The
# control character is refused on a second ground too: weaver-types-Spec
# section 2 elects that a path in a declaration carries none, because every
# reader in the suite, admin's parser, this script's shell and the harness,
# must agree on what a path is, and admin refuses a declared path carrying
# one by name.
# **An adapter is declared whole or not at all**, per ADP-1: the table is
# closed and every member required, so a partial set is refused here, naming
# what is missing, rather than by the parse after every account is made.
ADAPTER_GIVEN=0
for given in "$ADAPTER" "$ADAPTER_SHA256" "$ADAPTER_SCALE" "$BASE_BLAKE3"; do
  [ -z "$given" ] || ADAPTER_GIVEN=$((ADAPTER_GIVEN + 1))
done
if [ "$ADAPTER_GIVEN" -ne 0 ] && [ "$ADAPTER_GIVEN" -ne 4 ]; then
  missing=""
  [ -n "$ADAPTER" ] || missing="$missing --adapter"
  [ -n "$ADAPTER_SHA256" ] || missing="$missing --adapter-sha256"
  [ -n "$ADAPTER_SCALE" ] || missing="$missing --adapter-scale"
  [ -n "$BASE_BLAKE3" ] || missing="$missing --base-blake3"
  die "an adapter takes all four of --adapter, --adapter-sha256, --adapter-scale and --base-blake3, and this run is missing:$missing"
fi
TOML_STRINGS=("session:$SESSION" "artifact:$ARTIFACT")
[ "$ADAPTER_GIVEN" -eq 0 ] || TOML_STRINGS+=("adapter:$ADAPTER")
for pair in "${TOML_STRINGS[@]}"; do
  field=${pair%%:*} value=${pair#*:}
  if [[ "$value" == *[\"\\]* || "$value" =~ [[:cntrl:]] ]]; then
    die "the $field '$value' carries a double quote, a backslash or a control character, which the declaration's TOML string cannot hold as written"
  fi
done
if [ "$ADAPTER_GIVEN" -eq 4 ]; then
  # The digests are written as the parse requires them, 64 lowercase
  # hexadecimal characters. The scale is written bare into TOML, which would
  # take `nan` or `inf` as floats the parse then refuses with a worse account,
  # so it is a finite decimal in TOML's own number grammar (no leading zero,
  # an integer within i64) that narrows to a finite f32.
  command -v python3 >/dev/null || die "python3 is not on PATH, and the adapter's scale is checked with it"
  [[ "$ADAPTER_SHA256" =~ ^[0-9a-f]{64}$ ]] || die "--adapter-sha256 is 64 lowercase hexadecimal characters: '$ADAPTER_SHA256'"
  [[ "$BASE_BLAKE3" =~ ^[0-9a-f]{64}$ ]] || die "--base-blake3 is 64 lowercase hexadecimal characters: '$BASE_BLAKE3'"
  [[ "$ADAPTER_SCALE" =~ ^[+-]?(0|[1-9][0-9]*)(\.[0-9]+)?$ ]] \
    && python3 -c 'import sys; v = sys.argv[1]; n = float(v) if "." in v else int(v); sys.exit(0 if (isinstance(n, float) or -2**63 <= n < 2**63) and abs(float(n)) <= 3.4028234663852886e38 else 1)' "$ADAPTER_SCALE" 2>/dev/null \
    || die "--adapter-scale is a finite decimal number within f32's range, such as 1.0 or 0.7: '$ADAPTER_SCALE'"
fi
# The SPU path is one line of a key file admin reads, so it is absolute and
# carries no control character, on the ground the paragraph above gives.
if [ -n "$SPU_OVERRIDE" ]; then
  [[ "$SPU_OVERRIDE" == /* ]] || die "--spu takes an absolute path: '$SPU_OVERRIDE'"
  [[ "$SPU_OVERRIDE" =~ [[:cntrl:]] ]] && die "--spu carries a control character, which a key file cannot hold as one line"
fi
case "$CONNECTOR_ROLE" in
  operator) VERBS="show validate load unload stop save-point restore force-unload" ;;
  observer) VERBS="show" ;;
  *) die "--connector-role is operator or observer: '$CONNECTOR_ROLE'" ;;
esac
# **The operator is the account running this script**, never root: the root's
# `operator` key names their uid, and every root step below asks for sudo
# itself. The gate does not admit the operator: its rule names the box's
# gate-con account alone (weaver-gate-world-contract section 1).
[ "$(id -u)" -ne 0 ] || die "run as the operator, not under sudo: the root's operator key names this uid, and --apply asks for sudo itself"


# **An engine this script cannot provision is refused here rather than written
# into a declaration.** `weaver-types` admits `none` and `sqlite` and refuses
# any other engine at the parse, which would come after every account had been
# made. No engine is refused by its own name (#86). `none` is a lawful election and not one this script
# serves: an agent electing no store has no member and no room to verify.
# Declare that one by hand.
case "$ENGINE" in
  sqlite) ;;
  none) die "none is a lawful election and not one this script makes: there is no member, no state room and no store to probe. Declare it by hand, per deploy/HowToDeployANewAgent.md section 3." ;;
  *) die "$ENGINE is not an engine this build provides: weaver-types admits none and sqlite, and this script provisions sqlite, the default." ;;
esac

OPERATOR=$(id -un)
OPERATOR_UID=$(id -u)
AGENT_USER="weaver-$NAME"          # the agent's own uid: the worker's identity
MEMBER_USER="weaver-$NAME-state"   # the member's uid: owns the state room
STATE_GROUP="$MEMBER_USER"         # the member's primary group: the territory's, passage alone
TRACE_GROUP="weaver-$NAME-trace"   # the trace's readers: the operator and the relay, never the member
RELAY_USER="weaver-$NAME-relay"    # the trace relay's uid, its one group the trace group
ACCESS_GROUP="weaver-$NAME-admin"  # the territory's files' group and the trace door's: reads, never writes
GATECON_USER=weaver-gatecon        # the box's gate-con account: the one uid the access rule names
ADMINCON_USER=weaver-admincon      # the box's admin-con account: the reader, and the sudo rule's one user
SUDO_RULE="/etc/sudoers.d/weaver-$NAME"
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
AGENT_ROOT="$ADMIN_BASE/$NAME"
STAGE="$ADMIN_BASE/.$NAME.partial"

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

# **creatable_in DIR: where a root step may make a new name.** DIR must be
# held closed, and also writable by no group or other even when sticky: the
# sticky bit keeps another principal from renaming an entry it does not own,
# but not from claiming a name that does not exist yet, which a later root
# `install -d` or `tee` would then follow (Codex on #45). Fails printing DIR.
creatable_in() {
  local bad mode
  bad=$(held_closed "$1") || { printf '%s' "$bad"; return 1; }
  read -r _ mode < <(stat -c '%u %a' -- "$(realpath -e -- "$1")" 2>/dev/null) || { printf '%s' "$1"; return 1; }
  (( 8#$mode & 8#022 )) && { printf '%s' "$1"; return 1; }
  return 0
}

trim() {
  local value=$1
  value=${value#"${value%%[![:space:]]*}"}
  value=${value%"${value##*[![:space:]]}"}
  printf '%s' "$value"
}
# **The stack record is read without privilege in both modes.** It is
# root-owned and world-readable by construction, holds no secret, and every
# value the declaration renders from it must be known before sudo is asked
# for. A key that is absent, empty or unreadable refuses by name; only the
# optional keys may be absent.
REQUIRED_KEYS="worker-binary spu-binary gate-binary coordination-root"
OPTIONAL_KEYS="headroom-bytes library-path load-bound-seconds shell-binary"
stack_key() { # stack_key KEY required|optional
  local file="$STACK/$1" value
  [ -d "$STACK" ] || die "no stack record at $STACK: bootstrap-stack.sh writes it"
  if [ ! -e "$file" ] && [ ! -L "$file" ]; then
    [ "$2" = optional ] && return 0
    die "no $1 in the stack record $STACK"
  fi
  value=$(cat -- "$file") || die "cannot read $file"
  value=$(trim "$value")
  [ -n "$value" ] || die "empty $1 in the stack record $STACK"
  printf '%s' "$value"
}
[ -d "$STACK" ] || die "no stack record at $STACK: bootstrap-stack.sh writes it"
# **The stack record is judged before any value of it is trusted**: the record
# and every entry in it held closed by admin's rule, since a root step below
# acts on the paths it names (the walk of 2026-10-01, #45 round 11).
bad=$(held_closed "$STACK") || die "the stack record $STACK is not held closed by root at $bad"
for entry in "$STACK"/*; do
  bad=$(held_closed "$entry") || die "the stack record's $entry is not held closed by root at $bad"
done
for key in $REQUIRED_KEYS; do stack_key "$key" required >/dev/null || exit 1; done
for key in $OPTIONAL_KEYS; do stack_key "$key" optional >/dev/null || exit 1; done
AGENT_DIR=$(stack_key agent-directory required) || exit 1
# **The install prefix is a script-required key, read here and never copied
# into the root.** It was read only in the closing message, inside a command
# substitution whose refusal ended the subshell alone, so an absent prefix
# refused on stderr after everything was provisioned and the run still
# succeeded (Codex on #45).
PREFIX=$(stack_key prefix required) || exit 1
source "$(dirname "${BASH_SOURCE[0]}")/retired-python.sh"
WORKER_BINARY=$(stack_key worker-binary required) || exit 1
SPU_BINARY=${SPU_OVERRIDE:-$(stack_key spu-binary required)} || exit 1
refuse_python_binary "$WORKER_BINARY" "the stack's worker-binary"
refuse_python_binary "$SPU_BINARY" "this agent's spu-binary"
refuse_python_install "$PREFIX" "$(dirname -- "$WORKER_BINARY")"
# **The rule names the program sudo runs as root, so it is judged first**: the
# installed weaver-admin, by its canonical path, held closed by admin's rule.
ADMIN_BINARY=$(realpath -e -- "$PREFIX/bin/weaver-admin" 2>/dev/null) || die "no weaver-admin at $PREFIX/bin/weaver-admin"
bad=$(held_closed "$ADMIN_BINARY") || die "weaver-admin at $ADMIN_BINARY is not held closed by root at $bad, so no sudo rule names it"
# **Admin starts the relay from beside the worker for a file sink**
# (weaver-admin-Spec section 9), so a box without it is refused here rather
# than at the first load.
RELAY_BINARY="$(dirname -- "$(stack_key worker-binary required)")/weaver-trace-relay"
[ -x "$RELAY_BINARY" ] || die "no weaver-trace-relay beside the worker at $RELAY_BINARY: update-stack.sh installs it"
# **The stack's file names are judged as admin judges them, before anything is
# provisioned.** Admin keys the load record's stack by file name, and
# weaver-admin's `stack::judge_names` refuses every verb on a root where the
# worker, the state member (`weaver-state`), the gate and the SPU do not have
# four distinct names. An `--spu` named like any of them would otherwise be
# provisioned whole and then unusable, and this creation-only script cannot
# repair it.
judge_names() {
  local -a roles=("the worker" "the state member" "the gate" "the SPU")
  local -a names=("$(basename -- "$(stack_key worker-binary required)")" "weaver-state"
                  "$(basename -- "$(stack_key gate-binary required)")" "$(basename -- "$SPU_BINARY")")
  local i j
  for ((i = 0; i < 4; i++)); do
    for ((j = i + 1; j < 4; j++)); do
      [ "${names[i]}" = "${names[j]}" ] && die "${roles[i]} and ${roles[j]} share the file name ${names[i]}, which admin keys the stack by and refuses every verb on"
    done
  done
  return 0
}
judge_names || exit 1
# **The territory stands under a root-owned base, reached by group and never by
# access entries** (operator's ruling of 2026-10-02, #28). The base is the stack
# record's `agent-directory`, which bootstrap-stack.sh makes root 0755: it must
# stand, be held closed by admin's rule, and let no other principal make a name
# in it, since the territory is a new name there. Each territory is root-owned,
# grouped to the state group, 0710: the state group passes and none lists, the
# agent's own uid passes not, and the files' own modes are the wall beneath
# (the operator's ruling of 2026-10-08 on #1); the trace has a group of its
# own (below). No ACL is asked of the
# filesystem, so a box whose datasets carry none deploys as any other.
[ -d "$AGENT_DIR" ] || die "the stack record's agent-directory $AGENT_DIR does not stand: bootstrap-stack.sh makes it, root 0755"
bad=$(creatable_in "$AGENT_DIR") || die "the stack record's agent-directory $AGENT_DIR is not a root-held base no other principal can make a name in ($bad)"
# **The territory's path is canonical** (the #94 survey's S23): admin
# compares the declaration's sink directory with the canonical territory, so
# a link in the base's path written here would refuse every verb.
AGENT_DIR=$(realpath -e -- "$AGENT_DIR") || die "the stack record's agent-directory $AGENT_DIR does not resolve"
# **A base whose inheritance would make a territory admin refuses is refused
# here, before anything is made** (#99 area 2 review, K4): a territory made in
# a setgid base comes out setgid, and one made in a base carrying a default
# access entry carries it, and admin refuses either at every verb (exact
# mode 0710, no access entry), which the wall step below does not see. The
# look is admin's own: either POSIX ACL attribute present, read without
# following a link; a look that cannot answer refuses.
read -r base_mode < <(stat -c '%a' -- "$AGENT_DIR" 2>/dev/null) || die "cannot read the mode of the agent-directory $AGENT_DIR"
(( 8#$base_mode & 8#2000 )) && die "the stack record's agent-directory $AGENT_DIR is setgid (mode $base_mode), so a territory made in it would be setgid, which admin refuses; clear its setgid bit, then rerun"
rc=0
python3 -c '
import errno, os, sys
try:
    os.getxattr(sys.argv[1], "system.posix_acl_default", follow_symlinks=False)
except OSError as e:
    sys.exit(1 if e.errno in (errno.ENODATA, errno.EOPNOTSUPP) else 2)
' "$AGENT_DIR" || rc=$?
case $rc in
  0) die "the stack record's agent-directory $AGENT_DIR carries a default access entry, which a territory made in it would inherit and admin refuses; remove the default entry from it, then rerun" ;;
  1) ;;
  *) die "whether the agent-directory $AGENT_DIR carries a default access entry cannot be read" ;;
esac
HOME_DIR="$AGENT_DIR/$AGENT_USER"
STATE_DIR="$HOME_DIR/state"
# **The declaration, the draft and the save points stand in the territory**
# (the operator's ruling of 2026-10-07 on #1), which is refused below where it
# already stands, so no declaration is ever written over.
DECLARATION="$HOME_DIR/agent.toml"
PROMPT="$HOME_DIR/system-prompt.md"
SAVE_POINTS="$HOME_DIR/save-points"

# **account_line NAME: the account's passwd line, or nothing where it is
# absent**; a lookup that cannot answer refuses rather than reading as absence.
account_line() {
  local line rc=0
  line=$(getent passwd "$1") || rc=$?
  case $rc in
    0) printf '%s' "$line" ;;
    2) ;;
    *) die "cannot read account $1" ;;
  esac
}
# **uid_in LINE: the uid a passwd line carries**, refused where it is not a
# number.
uid_in() {
  local uid
  IFS=: read -r _ _ uid _ <<<"$1"
  [[ "$uid" =~ ^[0-9]+$ ]] || die "an account carries no numeric uid: $1"
  printf '%s' "$uid"
}
# **The box's connector accounts as they stand now**, read at the top level so
# a lookup that refuses ends the run rather than a command substitution alone.
GATECON_LINE=$(account_line "$GATECON_USER") || exit 1
ADMINCON_LINE=$(account_line "$ADMINCON_USER") || exit 1
# **The access rule names the box's gate-con account alone**
# (weaver-gate-world-contract section 1). Where the account stands its uid is
# known now. Where it does not, it is laid at --apply and the declaration is
# rendered again with the uid it takes, the parse check below having read a
# placeholder that is never written.
GATECON_UID=""
[ -z "$GATECON_LINE" ] || { GATECON_UID=$(uid_in "$GATECON_LINE") || exit 1; }
GATECON_UID_SHOWN=${GATECON_UID:-"the uid $GATECON_USER takes when --apply lays it"}

# **The declaration is rendered once, here, and parse-checked before anything
# is provisioned**, so a value that breaks it refuses before an account or a
# database exists rather than after. The check is TOML syntax through
# python3's tomllib, which reads the TOML 1.0 grammar declarations are
# written in, per weaver-types-Spec section 2, so a declaration this script
# writes is one every reader in the suite takes: the schema is admin's to judge, and its validate also
# judges the boundary this script provisions, so it can only pass once the
# script has run, which is why the closing step asks for it.
# The format is TOML, per weaver-types-Spec section 2: the top-level keys come
# first and each section is its own table after them, since TOML reads a bare
# key after a table header as that table's.
# The adapter table, written only when declared: an absent adapter is an
# omitted table (ADP-1), never an empty one.
ADAPTER_TABLE=""
if [ "$ADAPTER_GIVEN" -eq 4 ]; then
  ADAPTER_TABLE="

[spu-instruction.decoder.model-binding.adapter]
artifact = \"$ADAPTER\"
sha256 = \"$ADAPTER_SHA256\"
base-blake3 = \"$BASE_BLAKE3\"
scale = $ADAPTER_SCALE"
fi
render_declaration() {
cat <<TOML
session = "$SESSION"
tool-set = []
# **A serving binding carries a gate instruction and the inventory refuses it
# absent.** An unstated binding-kind resolves to serving, so both are written
# rather than left to a default a reader cannot see.
binding-kind = "serving"

# **No identity here**, per weaver-types-Spec section 2 on the ruling of
# 2026-10-06: the system prompt is state, seeded through the gate after the
# first load from the draft beside this file.
[spu-instruction.decoder]
residual-readout-election = false
surprisal-election = true
tunable-values = { context-capacity = 32768, max-tokens-per-turn = 4096, seed = 451234785645, temperature = 0.7, top-k = 40, top-p = 0.95, repetition-penalty = 1.1, repetition-window = 64 }

[spu-instruction.decoder.model-binding]
artifact = "$ARTIFACT"
devices = [0]$ADAPTER_TABLE

[gate-instruction.access-rule]
allowed-uids = [${GATECON_UID:-65534}]
allowed-gids = []
denied-uids = []

[trace-sink]
kind = "file"
path = "$HOME_DIR/trace.ndjson"
create = true

[state-election]
all-kinds = true
keys = [
  { kind = "message.user", paths = ["content"] },
  { kind = "message.assistant", paths = ["content"] },
]

# **The engine is a member of the binding**, per weaver-state-PRD section 4:
# declared here, changing only across the load boundary, and named on the load
# event like every fact that decides a record.
[state-store]
engine = "$ENGINE"
TOML
}
DECLARATION_TEXT=$(render_declaration)
# **The draft's bytes are what the seeding turn sends**, so the file ends in
# the newline an editor would write, and the text is the one this script wrote
# into every declaration before 2026-10-06.
PROMPT_TEXT='You are a careful assistant. Answer from what you know, say
plainly when you do not know, and keep answers as short as the
question allows.
'
command -v python3 >/dev/null || die "python3 is not on PATH, and the declaration is parse-checked with its tomllib before anything is made"
printf '%s\n' "$DECLARATION_TEXT" | python3 -c 'import sys, tomllib; tomllib.loads(sys.stdin.read())' 2>/dev/null \
  || die "the rendered declaration does not parse as TOML, so nothing was made. Check --session and --artifact"
# Plan and apply interpret the same configuration. Only the read identity
# differs. Validate all arguments above before asking for a sudo credential.
if [ "$APPLY" -eq 1 ]; then
  sudo -v || die "--apply needs sudo"
fi
read_as_owner() {
  if [ "$APPLY" -eq 1 ]; then sudo -n "$@"; else "$@"; fi
}
# Preserve test false separately from a failed privileged invocation. A sudo
# failure also returns 1, so using `sudo test ...` as a boolean loses its cause.
path_answer() {
  read_as_owner sh -c '
    if test "$1" "$2" || { [ "$1" = -e ] && test -L "$2"; }; then
      printf yes
    else
      printf no
    fi
  ' sh "$1" "$2" || die "cannot inspect $2"
}
require_path() {
  local answer
  answer=$(path_answer "$1" "$2") || exit 1
  [ "$answer" = yes ] || die "$3: $2"
}
refuse_existing() {
  local answer
  answer=$(path_answer -e "$1") || exit 1
  [ "$answer" = no ] || die "$2 already exists: $1"
}

# **The box's connector accounts are judged by their shape, not only their
# name** (weaver-admin-Spec section 4): a standing account of the name is
# adopted only where it is the account this script would lay.
UID_MIN=$(awk '$1 == "UID_MIN" { v = $2 } END { print v }' /etc/login.defs 2>/dev/null || true)
[[ "$UID_MIN" =~ ^[0-9]+$ ]] || UID_MIN=1000
# agent_name_ok N: N is a name admin's name check admits (weaver-admin-Spec
# section 4): ASCII letters, digits, `-` and `_`, no reserved suffix, and not
# a box account's name.
agent_name_ok() {
  [[ "$1" =~ ^[A-Za-z0-9_-]+$ ]] || return 1
  case "$1" in
    *-state|*-trace|*-relay|*-admin|*-admincon|gatecon|admincon) return 1 ;;
  esac
  return 0
}
# door_crossings NAME: each group NAME holds outside what the box's account
# NAME may hold, space-separated, or nothing. **Each account is judged by an
# allow-list, inside the weaver- namespace and outside it alike**, so a group
# such as wheel, sudo, disk or docker refuses as a weaver- group would:
# - weaver-admincon may hold its own group and agents' state and access
#   groups, weaver-<n>-state and weaver-<n>-admin, and nothing else;
# - weaver-gatecon may hold its own group and agents' own groups, weaver-<n>,
#   and nothing else.
# <n> is a name agent_name_ok admits, so a trace, relay, state, access or
# per-agent connector group is never read as an agent's own group.
door_crossings() {
  local held g n crossed=""
  held=$(id -Gn "$1") || die "cannot read the groups of $1"
  for g in $held; do
    n=""
    case "$1:$g" in
      "$1:$1") continue ;;
      "$ADMINCON_USER:"weaver-*-state) n=${g#weaver-}; n=${n%-state} ;;
      "$ADMINCON_USER:"weaver-*-admin) n=${g#weaver-}; n=${n%-admin} ;;
      "$GATECON_USER:"weaver-*) n=${g#weaver-} ;;
    esac
    [ -n "$n" ] && agent_name_ok "$n" && continue
    crossed="$crossed $g"
  done
  printf '%s' "${crossed# }"
}
# judge_box_account NAME: refuses by name where the standing account NAME is
# not a system account with no login shell, no home and a primary group of
# its own that no other account shares, or where it holds a group reaching
# the other account's door.
judge_box_account() {
  local name=$1 line uid gid home shell home_stands group_line gname members sharers crossed
  line=$(account_line "$name") || exit 1
  [ -n "$line" ] || die "the account $name does not stand"
  IFS=: read -r _ _ uid gid _ home shell <<<"$line"
  [[ "$uid" =~ ^[0-9]+$ ]] && [ "$uid" -lt "$UID_MIN" ] \
    || die "the account $name stands with uid $uid, not a system account (below UID_MIN $UID_MIN), so it is not adopted as the box's account"
  [ "$uid" -gt 0 ] || die "the account $name stands with uid 0, root's, so it is not adopted as the box's account"
  case "$shell" in
    /usr/sbin/nologin|/sbin/nologin|/usr/bin/nologin|/bin/false|/usr/bin/false) ;;
    *) die "the account $name stands with the login shell '$shell', so it is not adopted as the box's account" ;;
  esac
  home_stands=$(path_answer -e "$home") || exit 1
  [ "$home_stands" = no ] \
    || die "the account $name stands with a home at $home, so it is not adopted as the box's account"
  group_line=$(getent group "$gid") || die "the account $name's primary group $gid does not resolve"
  IFS=: read -r gname _ _ members <<<"$group_line"
  # **Its own is by name** (CodeRabbit on #533): a group no one else holds can
  # still be an agent's, such as weaver-<n>-admin, which the allow-list below
  # admits, and that agent's take-down could then not remove it.
  [ "$gname" = "$name" ] \
    || die "the account $name's primary group is $gname, not its own group $name, so it is not adopted as the box's account"
  [ -z "$members" ] || [ "$members" = "$name" ] \
    || die "the account $name's primary group $gname is shared with $members, so it is not adopted as the box's account"
  sharers=$(getent passwd | awk -F: -v g="$gid" -v n="$name" '$4 == g && $1 != n { printf "%s ", $1 }')
  [ -z "$sharers" ] \
    || die "the account $name's primary group $gname is the primary group of ${sharers% } too, so it is not adopted as the box's account"
  crossed=$(door_crossings "$name") || exit 1
  [ -z "$crossed" ] \
    || die "the account $name holds $crossed, outside what the box's account may hold (weaver-admin-Spec section 2), so it is not adopted as the box's account"
}
require_path -d "$ADMIN_BASE" "the admin base is missing or not a directory (bootstrap-stack.sh makes it)"
require_path -x "$ADMIN_BASE" "the admin base cannot be traversed"
# **The base is judged as admin judges every directory above a root**, before
# anything is provisioned: admin refuses every verb on a root whose base or any
# directory above it another principal could write, so a base under a home
# directory would be provisioned whole and then unusable (Codex on #45).
bad=$(held_closed "$ADMIN_BASE") || die "the admin base $ADMIN_BASE is not held closed by root at $bad, so admin would refuse every verb on the agent"
# The stage and the root are new names in the base, and the log directory is a
# new name in the record's log-directory, so neither may let another principal
# claim the name first.
bad=$(creatable_in "$ADMIN_BASE") || die "the admin base $ADMIN_BASE lets another principal make names in it ($bad), so the stage could be claimed first"
[ "$ADMIN_BASE" = /etc/weaver/admin ] || printf '   WARNING: the admin base is %s, and a delegated invocation reads only /etc/weaver/admin, so the connector'"'"'s sudo rule cannot drive this agent\n' "$ADMIN_BASE"

# **Whose identity the store admits is settled and derived.** The charter has
# the member hold a uid of its own and dial the store under it, and as of
# 2026-09-15 the code does: the account is the one this script makes, so the
# identity map, the territory's owner and the spawn's uid are one fact rather
# than three the operator keeps agreeing.

# **Which box account is laid and which is judged**, said in the plan: one
# that stands is judged in the checks below before anything is made.
box_account_plan() { # box_account_plan LINE: the account's passwd line as read above
  if [ -n "$1" ]; then
    printf 'found standing: judged below (system uid, no login shell, no home, its own primary group, no group reaching the other door)'
  else
    printf 'absent: laid at --apply (system, nologin, no home, its own group)'
  fi
}
say "plan for agent '$NAME'"
plan "agent account   $AGENT_USER      (system, nologin, the worker's uid)"
plan "member account  $MEMBER_USER     (system, nologin, owns the state room, passes the territory by its own group $STATE_GROUP)"
plan "trace group     $TRACE_GROUP     (system group: the trace's readers, never the member)"
plan "relay account   $RELAY_USER      (system, nologin, no home, its one group $TRACE_GROUP)"
plan "access group    $ACCESS_GROUP    (system group: the territory's files' and the trace door's, reads and never writes)"
plan "gate-con        $GATECON_USER  the box's, $(box_account_plan "$GATECON_LINE")"
plan "                joins $AGENT_USER, the gate socket's group: the one caller the access rule names"
plan "admin-con       $ADMINCON_USER  the box's, $(box_account_plan "$ADMINCON_LINE")"
plan "                joins $STATE_GROUP and $ACCESS_GROUP: the trace reader and the sudo rule's user"
plan "operator        $OPERATOR joins groups $TRACE_GROUP, $STATE_GROUP and $ACCESS_GROUP, never $AGENT_USER"
plan "home            /home/$AGENT_USER        the agent's own, where its tools run"
plan "territory       $HOME_DIR        root:$STATE_GROUP 0710, passage for the state group alone"
plan "trace           $HOME_DIR/trace.ndjson  root:$TRACE_GROUP 0640, made before the first load"
plan "state room      $STATE_DIR       $MEMBER_USER 0700, which the agent's uid cannot enter"
plan "store           sqlite in memory, its save points in the state room"
plan "save points     $SAVE_POINTS     root:$ACCESS_GROUP 0750, the published copies and their manifest"
plan "agent root      $AGENT_ROOT      root 0755, keys 0644, copied from $STACK"
plan "spu-binary      $SPU_BINARY$( [ -n "$SPU_OVERRIDE" ] && printf '  (--spu, in place of the stack record'"'"'s)' )"
plan "operator key    $OPERATOR_UID ($OPERATOR)"
plan "roles.toml      trace-reader = $ADMINCON_USER"
plan "access rule     allowed-uids = [$GATECON_UID_SHOWN] ($GATECON_USER alone)"
plan "declaration     $DECLARATION     root:$ACCESS_GROUP 0640, session $SESSION, artifact $ARTIFACT; edited with sudoedit"
[ "$ADAPTER_GIVEN" -eq 0 ] || plan "adapter         $ADAPTER at scale $ADAPTER_SCALE, declared sha256 $ADAPTER_SHA256 over base blake3 $BASE_BLAKE3; checked by the SPU at admission"
plan "prompt draft    $PROMPT     root:$ACCESS_GROUP 0640, a draft: the agent has no seeding sender (weaver-gate-world-contract section 2)"
plan "logs            $HOME_DIR/admin.log and worker.log, made by admin at the first verb"
plan "sudo rule       $SUDO_RULE      $ADMINCON_USER, $CONNECTOR_ROLE: $VERBS, !pam_session"
plan "store engine    $ENGINE         which the deployed member must carry"

# What must not already be there. Creation is refused rather than merged,
# because a half-made agent that looks whole is worse than an absent one.
say "checks"
for u in "$AGENT_USER" "$MEMBER_USER" "$RELAY_USER"; do
  if getent passwd "$u" >/dev/null; then
    die "the account $u already exists"
  else
    account_status=$?
    [ "$account_status" -eq 2 ] || die "cannot read account $u"
  fi
done
# The groups this run makes, the trace and access groups and the same-named
# group `useradd --user-group` makes for the agent and the member, each
# checked before anything is provisioned (Codex on #79).
for g in "$TRACE_GROUP" "$ACCESS_GROUP" "$AGENT_USER" "$MEMBER_USER"; do
  if getent group "$g" >/dev/null; then
    die "the group $g already exists"
  else
    group_status=$?
    [ "$group_status" -eq 2 ] || die "cannot read group $g"
  fi
done
# **A box account that is absent has no group of its name either**: apply
# lays it with its own group, which useradd refuses to make over a standing
# one, so the plan refuses here rather than stop part way.
for box_account in "$GATECON_USER" "$ADMINCON_USER"; do
  case "$box_account" in
    "$GATECON_USER") [ -z "$GATECON_LINE" ] || continue ;;
    *) [ -z "$ADMINCON_LINE" ] || continue ;;
  esac
  if getent group "$box_account" >/dev/null; then
    die "the group $box_account stands without its account, so the box's account cannot be laid with its own group (look at it, and remove it or lay the account by hand)"
  else
    group_status=$?
    [ "$group_status" -eq 2 ] || die "cannot read group $box_account"
  fi
done
# **A box account that stands is judged before anything is made**, so a
# standing account of the right name and the wrong shape is never adopted.
[ -z "$GATECON_LINE" ] || { judge_box_account "$GATECON_USER"; printf '   %s stands, and its shape is the box'"'"'s gate-con account\n' "$GATECON_USER"; }
[ -z "$ADMINCON_LINE" ] || { judge_box_account "$ADMINCON_USER"; printf '   %s stands, and its shape is the box'"'"'s admin-con account\n' "$ADMINCON_USER"; }
refuse_existing "/home/$AGENT_USER" "agent home"
refuse_existing "$HOME_DIR" "territory"
refuse_existing "$AGENT_ROOT" "agent root"
refuse_existing "$STAGE" "a partial agent root from an earlier run (remove it by hand)"
refuse_existing "$SUDO_RULE" "sudo rule"
[ -r "$ARTIFACT" ] || printf '   WARNING: the artifact is not readable from this shell: %s\n' "$ARTIFACT"
[ "$ADAPTER_GIVEN" -eq 0 ] || [ -r "$ADAPTER" ] || printf '   WARNING: the adapter is not readable from this shell: %s\n' "$ADAPTER"
[ -z "$SPU_OVERRIDE" ] || [ -x "$SPU_OVERRIDE" ] || printf '   WARNING: the --spu path is not executable from this shell: %s\n' "$SPU_OVERRIDE"
if [ "$APPLY" -eq 0 ]; then
  printf '   no collision found in accounts and paths visible to this uid\n'
  printf '   PENDING --apply: privileged collision checks\n'
  printf '   PENDING --apply: authentication paths, the sudoers include, visudo\n'
  say "plan only"
  printf '   no provisioning performed; rerun with --apply to check and make it\n'
  exit 0
fi

say "accounts"
# **The box's connector accounts are laid where absent, once per box**, each
# a system account with no login shell, no home and its own group, then
# judged as a standing one is. Neither is laid per agent. **They are laid
# first**, so an account that cannot be laid or judged stops the run before
# any account of the agent's stands.
for box_account in "$GATECON_USER" "$ADMINCON_USER"; do
  line=$(account_line "$box_account") || exit 1
  if [ -z "$line" ]; then
    sudo useradd --system --shell /usr/sbin/nologin --no-create-home --user-group "$box_account"
    printf '   laid %s, the box'"'"'s account\n' "$box_account"
  fi
  judge_box_account "$box_account"
done
# **The agent gets a home and the member does not.** The agent's tools run
# somewhere, and the retirement of 2026-09-11 found one of three agents with
# a home and the others without, which is the drift a script ends. The member
# runs nothing and owns one room of the operator's tree instead.
sudo useradd --system --shell /usr/sbin/nologin --create-home --user-group "$AGENT_USER"
sudo useradd --system --shell /usr/sbin/nologin --no-create-home --user-group "$MEMBER_USER"
sudo groupadd --system "$TRACE_GROUP"
sudo groupadd --system "$ACCESS_GROUP"
# **The relay's one group is the trace group**, primary and alone, which is
# what admin's start step sets at its spawn (weaver-admin-Spec section 6).
sudo useradd --system --shell /usr/sbin/nologin --no-create-home --no-user-group --gid "$TRACE_GROUP" "$RELAY_USER"
GATECON_LINE=$(account_line "$GATECON_USER") || exit 1
[ -n "$GATECON_LINE" ] || die "$GATECON_USER does not stand after it was laid"
GATECON_UID=$(uid_in "$GATECON_LINE") || exit 1
[ "$GATECON_UID" -ne 0 ] || die "$GATECON_USER carries uid 0, and no access rule names root"
ADMINCON_LINE=$(account_line "$ADMINCON_USER") || exit 1
[ -n "$ADMINCON_LINE" ] || die "$ADMINCON_USER does not stand after it was laid"
ADMINCON_UID=$(uid_in "$ADMINCON_LINE") || exit 1
# **The gate-con account joins the agent's own group**, which the gate's
# socket admits by (weaver-gate-Spec section 3). The group also reads the
# agent's tool home and lists its runtime directory, a reach that Spec states
# as the design, and gate-con joins no other group of the agent's. **The admin-con account joins the state group for passage and the
# access group for reading**, never the agent's own group or the trace group:
# it reaches the trace through the relay's door alone.
sudo usermod -aG "$AGENT_USER" "$GATECON_USER"
sudo usermod -aG "$STATE_GROUP,$ACCESS_GROUP" "$ADMINCON_USER"
# **The member holds its own group alone** (the operator's ruling of
# 2026-10-08 on #1): the territory is 0710 under that group, so the member
# passes to its own room by it, and the access group, which reads admin.log,
# worker.log, the declaration, the draft and the published save points, is
# not the member's.
# **The operator reads without sudo through its groups**: the trace by its
# group, and the declaration, the draft, the logs and the save points by the
# access group, past the territory, which the state group passes. The state
# group buys the operator passage of the territory and nothing more, the
# state room being the member's alone. The operator does not join the agent's
# own group: it does not dial the gate (weaver-gate-world-contract section 1).
sudo usermod -aG "$TRACE_GROUP,$STATE_GROUP,$ACCESS_GROUP" "$OPERATOR"
sudo chmod 2750 "/home/$AGENT_USER"
printf '   %s uid %s, %s uid %s, %s uid %s, %s uid %s, %s uid %s\n' \
  "$AGENT_USER" "$(id -u "$AGENT_USER")" "$MEMBER_USER" "$(id -u "$MEMBER_USER")" \
  "$RELAY_USER" "$(id -u "$RELAY_USER")" "$GATECON_USER" "$GATECON_UID" \
  "$ADMINCON_USER" "$ADMINCON_UID"
# **The declaration is rendered again with the uid the gate-con account
# holds**, and parse-checked again, before it is written.
DECLARATION_TEXT=$(render_declaration)
printf '%s\n' "$DECLARATION_TEXT" | python3 -c 'import sys, tomllib; tomllib.loads(sys.stdin.read())' 2>/dev/null \
  || die "the declaration rendered with $GATECON_USER's uid does not parse as TOML"
# **The rule written names the gate-con account's uid and nothing else**: the
# plan renders a placeholder, 65534, where the account is absent, so the
# rendered rule is asserted to be exactly `allowed-uids = [<its uid>]` here and
# again at the write, and refused otherwise.
rule_names_gatecon() {
  local rules
  [[ "$GATECON_UID" =~ ^[0-9]+$ ]] && [ "$GATECON_UID" -gt 0 ] && [ "$GATECON_UID" -ne 65534 ] \
    || die "$GATECON_USER's uid '$GATECON_UID' is not a uid an access rule may name"
  rules=$(grep -E '^allowed-uids' <<<"$DECLARATION_TEXT" || true)
  [ "$rules" = "allowed-uids = [$GATECON_UID]" ] \
    || die "the declaration's access rule reads '$rules', not allowed-uids = [$GATECON_UID] ($GATECON_USER alone), so it is not written"
}
rule_names_gatecon

say "territory"
# **The whole agent lives here, and the group passes and never writes** (the
# operator's ruling of 2026-10-07 on #1, refining 2026-10-02's #28 and #56).
# The territory is root's, grouped to the state group, 0710 and not setgid
# (the operator's ruling of 2026-10-08 on #1): the state group passes to what
# stands in it by name and lists nothing. The member holds that group as its
# primary group, and the operator and the admin-con account join it for
# passage. The
# agent's own uid holds it not, so it cannot pass to the trace, which is the
# denial admin's load asks of the sink's directory. Nothing written here
# takes the territory's group. Admin makes the member's room at
# `<sink directory>/state`, and the protection beneath sits on each file's
# own mode, the trace's first:
# made here, before the first load, root:$TRACE_GROUP 0640, the layout admin
# checks at every load (#62): admin opens it append-only, never through a
# link, and refuses a trace that stands otherwise, so the member, outside that
# group, cannot read it, and the operator reads it through the group. Were the
# file ever removed, admin recreates it at this same layout where the
# declaration elects creation. The agent's uid, in no group of either, cannot
# enter. The state room is the member's own. The published save points go in
# `save-points/`, root:$ACCESS_GROUP 0750, where admin writes each copy 0640
# to the group and the manifest root 0644, so the group reads a save point and
# rewrites none.
# Five-digit modes, so no setgid bit survives however the base stands (#99
# area 2 review, K4): GNU install keeps one under a four-digit mode.
sudo install -d -o root -g "$STATE_GROUP" -m 00710 "$HOME_DIR"
sudo install -o root -g "$TRACE_GROUP" -m 0640 /dev/null "$HOME_DIR/trace.ndjson"
sudo install -d -o "$MEMBER_USER" -g "$MEMBER_USER" -m 0700 "$STATE_DIR"
sudo install -d -o root -g "$ACCESS_GROUP" -m 00750 "$SAVE_POINTS"


say "declaration"
# **Written as root into root's directory**, root:$ACCESS_GROUP 0640 each:
# the declaration the operator edits from here with `sudoedit`, and the
# prompt draft, read through the access group. Neither is read
# by a uid outside the access group, and admin refuses a declaration grouped
# or moded otherwise (weaver-admin-Spec section 9). Nothing lands in the operator's
# home. The territory was refused above where it stood, so neither file is
# written over.
rule_names_gatecon
printf '%s\n' "$DECLARATION_TEXT" | sudo tee "$DECLARATION" >/dev/null
printf '%s' "$PROMPT_TEXT" | sudo tee "$PROMPT" >/dev/null
sudo chown root:"$ACCESS_GROUP" "$DECLARATION" "$PROMPT"
sudo chmod 0640 "$DECLARATION" "$PROMPT"
printf '   %s and %s written, root:%s 0640\n' "$DECLARATION" "$PROMPT" "$ACCESS_GROUP"

say "agent root, staged"
# **Root-owned and not group- or world-writable, or admin refuses it**, so it
# is made by root at 0755 with its files 0644. Every key the stack record
# holds is copied as written. `spu-binary` is this agent's `--spu` where given,
# and `territory`, `operator` and `roles.toml` are the agent's own.
sudo install -d -o root -g root -m 0755 "$STAGE"
for key in $REQUIRED_KEYS $OPTIONAL_KEYS; do
  [ -e "$STACK/$key" ] || continue
  if [ "$key" = spu-binary ] && [ -n "$SPU_OVERRIDE" ]; then
    printf '%s\n' "$SPU_OVERRIDE" | sudo tee "$STAGE/$key" >/dev/null
  else
    sudo cp -- "$STACK/$key" "$STAGE/$key"
  fi
done
printf '%s\n' "$HOME_DIR" | sudo tee "$STAGE/territory" >/dev/null
printf '%s\n' "$OPERATOR_UID" | sudo tee "$STAGE/operator" >/dev/null
# **The boundary file names the trace door's one reader**, the box's
# admin-con account, in the shape of weaver-types-Spec section 3.1 and
# nothing else.
printf 'trace-reader = "%s"\n' "$ADMINCON_USER" | sudo tee "$STAGE/roles.toml" >/dev/null
sudo chmod 0644 "$STAGE"/*
printf '   staged at %s\n' "$STAGE"

say "the wall, verified rather than assumed"
# A refusal here leaves the root staged and not admitted.
# **The store's gate is the room itself**: the member writes its save points
# into the state territory, so the member must be able to write there and the
# agent's own uid must not be able to enter it.
if sudo -u "$MEMBER_USER" test -w "$STATE_DIR" && sudo -u "$MEMBER_USER" test -x "$STATE_DIR"; then
  printf '   %s can write its state room %s\n' "$MEMBER_USER" "$STATE_DIR"
else
  die "the member cannot write its state room $STATE_DIR: its passage or ownership is wrong"
fi
if sudo -u "$AGENT_USER" test -x "$STATE_DIR"; then
  die "THE AGENT'S UID CAN ENTER THE STATE ROOM $STATE_DIR: the wall is open"
else
  printf "   the agent's own uid cannot enter the state room, which is the wall the charter asks for\n"
fi

# **The member cannot read the trace**: its ingress is the
# tee's distillate and nothing more (weaver-state-PRD).
if sudo -u "$MEMBER_USER" test -r "$HOME_DIR/trace.ndjson"; then
  die "THE MEMBER $MEMBER_USER CAN READ THE TRACE $HOME_DIR/trace.ndjson: the trace's group or mode is open"
else
  printf "   the member cannot read the trace, which is the boundary the state charter asks for\n"
fi
# **The member reads none of the access group's files**: it passes the
# territory by its own group and holds not the access group.
if sudo -u "$MEMBER_USER" test -r "$DECLARATION"; then
  die "THE MEMBER $MEMBER_USER CAN READ THE DECLARATION $DECLARATION: it holds the access group, or the file's mode is open"
fi
# **The agent's own uid cannot pass the territory** (the #94 survey's S1):
# admin's load refuses a sink whose directory the agent's uid can traverse,
# so a territory it could pass would refuse every load.
if sudo -u "$AGENT_USER" test -x "$HOME_DIR"; then
  die "THE AGENT'S UID CAN PASS THE TERRITORY $HOME_DIR: its group or mode is open, and admin would refuse every load"
else
  printf "   the agent's own uid cannot pass the territory, which is the trace's denial admin checks\n"
fi

# **The access group reads and never writes** (the operator's ruling of
# 2026-10-07 on #1): the admin-con account, which holds it, reads the
# declaration and cannot rewrite it, the draft or the save points directory,
# since an account that could rewrite the declaration would choose what admin
# loads.
sudo -u "$ADMINCON_USER" test -r "$DECLARATION" \
  || die "the admin-con account $ADMINCON_USER cannot read the declaration $DECLARATION through $STATE_GROUP and $ACCESS_GROUP: the territory's or the file's group or mode is wrong"
for f in "$DECLARATION" "$PROMPT" "$SAVE_POINTS"; do
  if sudo -u "$ADMINCON_USER" test -w "$f"; then
    die "THE ACCESS GROUP CAN WRITE $f: the admin-con account could rewrite what admin loads"
  fi
done
printf '   %s reads the declaration and writes nothing of the territory\n' "$ADMINCON_USER"

# **The relay and the two box accounts hold exactly their groups of this
# agent**: the relay the trace group alone; the admin-con account the state
# group for passage and the access group for reading, never the agent's own
# group or the trace group; the gate-con account the agent's own group, for
# the gate's socket. **Neither box account holds a group outside what it may
# hold**, of this agent, of any other or outside the weaver- namespace
# (weaver-admin-Spec section 2), by the same allow-list as the judge above.
relay_groups=$(id -Gn "$RELAY_USER")
[ "$relay_groups" = "$TRACE_GROUP" ] || die "the relay account $RELAY_USER holds '$relay_groups', not the trace group alone"
admincon_groups=" $(id -Gn "$ADMINCON_USER") "
[[ "$admincon_groups" == *" $ACCESS_GROUP "* ]] || die "the admin-con account $ADMINCON_USER does not hold $ACCESS_GROUP, so the trace door would turn it away"
[[ "$admincon_groups" == *" $STATE_GROUP "* ]] || die "the admin-con account $ADMINCON_USER does not hold $STATE_GROUP, so it cannot pass the territory"
gatecon_groups=" $(id -Gn "$GATECON_USER") "
[[ "$gatecon_groups" == *" $AGENT_USER "* ]] || die "the gate-con account $GATECON_USER does not hold $AGENT_USER, so the gate's socket would turn it away"
for box_account in "$GATECON_USER" "$ADMINCON_USER"; do
  crossed=$(door_crossings "$box_account") || exit 1
  [ -z "$crossed" ] || die "the account $box_account holds $crossed, outside what the box's account may hold (weaver-admin-Spec section 2)"
done
printf '   %s holds %s alone, %s holds%s, %s holds%s\n' "$RELAY_USER" "$TRACE_GROUP" \
  "$ADMINCON_USER" "${admincon_groups% }" "$GATECON_USER" "${gatecon_groups% }"

say "sudo rule"
# **Rendered, checked by visudo, then installed root 0440.** Each grant is one
# fixed command line with its arguments, so sudo matches it exactly and the
# admin-con account can pass none of its own.
sudo -n grep -qE '^[#@]includedir[[:space:]]+/etc/sudoers\.d([[:space:]]|$)' /etc/sudoers \
  || die "/etc/sudoers does not include /etc/sudoers.d, so a rule there would never be read"
lines=""
for verb in $VERBS; do
  lines="$lines${lines:+, }$ADMIN_BINARY $verb $NAME"
done
RULE_TEXT=$(cat <<SUDOERS
# weaver-$NAME: the box's admin-con account's fixed command lines for this
# agent, per weaver-admin-Spec section 2. Written by deploy/create-agent.sh,
# $CONNECTOR_ROLE role.
Defaults:$ADMINCON_USER !pam_session
$ADMINCON_USER ALL=(root) NOPASSWD: $lines
SUDOERS
)
# Staged under a dot-name sudo skips, checked, then moved into place.
rule_stage=$(sudo mktemp "/etc/sudoers.d/.weaver-$NAME.XXXXXX")
printf '%s\n' "$RULE_TEXT" | sudo tee "$rule_stage" >/dev/null
sudo chmod 0440 "$rule_stage"
if ! sudo visudo -cqf "$rule_stage"; then
  sudo rm -f -- "$rule_stage"
  die "visudo refuses the rendered rule, so it was not installed"
fi
sudo mv -T -- "$rule_stage" "$SUDO_RULE"
printf '   %s installed: %s\n' "$SUDO_RULE" "$VERBS"
sudo -n -l -U "$ADMINCON_USER" | sed -n '/may run the following/,$p' | sed 's/^/   /'

say "admission"
# **Moving the root into place is the admission**: before it every admin verb
# answers no_such_agent for this name, and after it the agent is admitted.
sudo mv -T -- "$STAGE" "$AGENT_ROOT"
printf '   %s admitted at %s\n' "$NAME" "$AGENT_ROOT"

say "made"
# **The group add applies to a login taken after it.** `usermod -aG` changes
# the account and not a process already running (#673, measured on the W4a
# run of 2026-09-25): a gate-con already running under the gate-con account
# cannot reach this agent's socket, nor an admin-con this agent's trace door,
# until it starts again, and an operator's shell takes its groups at a new
# login.
printf '   %s joined %s, and %s joined %s and %s: a gate-con or admin-con already running\n' "$GATECON_USER" "$AGENT_USER" "$ADMINCON_USER" "$STATE_GROUP" "$ACCESS_GROUP"
printf '   under either account takes the groups only when it starts again\n'
printf '   %s joined groups %s, %s and %s: a session that predates this run needs a new\n' "$OPERATOR" "$TRACE_GROUP" "$STATE_GROUP" "$ACCESS_GROUP"
printf '   login before the groups apply (`newgrp` selects one group in one shell)\n'
printf '   the declaration %s is root'"'"'s: edit it with sudoedit, and read it, the draft, the logs and\n' "$DECLARATION"
printf '   the save points under %s through %s\n' "$HOME_DIR" "$ACCESS_GROUP"
# **probe_connector VERB: the admin-con account reaches a line through its
# rule**, as the stand-in for admin-con: the operator's sudo becomes the account,
# and that user runs `sudo -n`, which only the rule can satisfy. It succeeds
# only on the answer the verb owes, `validated` for validate and a state for
# show, and prints what came back either way (Codex on #79).
probe_connector() {
  local said answer
  said=$(sudo -n -u "$ADMINCON_USER" sudo -n "$ADMIN_BINARY" "$1" "$NAME" 2>&1) || true
  answer=$(printf '%s\n' "$said" | tail -n 1)
  printf '%s' "$answer"
  python3 -c '
import json, sys
try:
    d = json.loads(sys.argv[2])
except ValueError:
    sys.exit(1)
sys.exit(0 if d.get("kind") == {"validate": "validated", "show": "state"}[sys.argv[1]] else 1)
' "$1" "$answer"
}
if [ "$ADMIN_BASE" = /etc/weaver/admin ]; then
  check_verb=show
  [ "$CONNECTOR_ROLE" = operator ] && check_verb=validate
  if answer=$(probe_connector "$check_verb"); then
    printf '   %s ran %s through its rule: %s\n' "$ADMINCON_USER" "$check_verb" "$answer"
  else
    die "$NAME is admitted, but the admin-con account $ADMINCON_USER could not run $check_verb through $SUDO_RULE: $answer. Another sudo policy may override the rule. Listing what it grants the account is in deploy/HowToDeployANewAgent.md section 5."
  fi
fi
printf '   validate it before loading, with the validate verb of %q for %s as root,\n' "$ADMIN_BINARY" "$NAME"
printf '   then prove its load with deploy/verify-load.sh as root (deploy/HowToDeployANewAgent.md section 4),\n'
printf '   then send a turn as %s (deploy/HowToDeployANewAgent.md section 4). Its gate admits %s\n' "$GATECON_USER" "$GATECON_USER"
printf '   alone, so it has no seeding sender: %s is a draft, and how its prompt enters is deferred\n' "$PROMPT"
printf '   (weaver-gate-world-contract section 2)\n'
