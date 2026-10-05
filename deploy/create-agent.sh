#!/usr/bin/env bash
# Create one agent on this box: its accounts, its territory, the state store
# behind its member's seam, its declaration directory, its root of admin
# configuration, and the sudo rule its connector runs the verbs through.
#
#   ./deploy/create-agent.sh fred --engine sqlite --artifact /path/to.gguf            plan only
#   ./deploy/create-agent.sh fred --engine sqlite --artifact /path/to.gguf --apply    act
#
# Options: --engine sqlite (the default, and the one engine since the service
# engine retired on the operator's ruling of 2026-10-02 on #1), --session <name>
# (default <name>-001), --spu <path> (this agent's SPU, in place of the
# stack record's `spu-binary`), --declaration-directory <path> (default
# `~/.weaveragent/<name>` in the operator's home), --connector-role
# operator|observer (default operator: which command lines the connector's
# sudo rule grants).
#
# Run it as the operator, never under sudo: the declaration directory is the
# operator's own, made and written as the operator, and `--apply` asks for
# sudo itself for every root step.
#
# **The agent's root is `<admin base>/<name>/` and it is the admission.**
# Admin reads that directory and nothing shared, so this script writes every
# key admin requires into it, copied from the stack record `bootstrap-stack.sh`
# wrote at `/etc/weaver/stack/` (which admin never reads), plus the agent's own
# `declaration-directory`, `operator` and `roles.toml`, per weaver-admin-Spec
# section 9. The root is staged under a dot-name admin's name check refuses,
# and moved into place last, so a run that stops part way leaves no root admin
# would admit.
#
# **The declaration stands in the operator's directory and not in the root**
# (operator's ruling of 2026-10-02): `agent.toml` is written there, as the
# operator, in a directory only the operator can enter, and admin reads it as
# root through the judgment of weaver-admin-Spec section 9. Admin's own
# `admin.log` and `worker.log` land beside it at the first verb.
#
# **Three accounts and two groups beyond the agent's own**, per
# weaver-admin-Spec sections 4, 6 and 9 (the operator's rulings of 2026-10-03
# on #50): the state member `weaver-<name>-state`, the trace relay
# `weaver-<name>-relay` whose one group is the trace group
# `weaver-<name>-trace`, the access group `weaver-<name>-admin` the trace door
# is grouped to, and the connector's service user `weaver-<name>-admincon`,
# which holds the access group, is the boundary file's `trace-reader`, and is
# the one user the sudo rule names.
#
# **The sudo rule is strict** (weaver-admin-Spec section 2, the operator
# contract section 2): `/etc/sudoers.d/weaver-<name>` grants the connector's
# user exactly this agent's fixed `weaver-admin <verb> <name>` command lines,
# as root, without a password, with no caller-chosen argument, no SETENV and
# no env_keep, and turns sudo's `pam_session` off for that user so a session
# never moves the invocation, and the agent it starts, out of the invoker's
# containment. The observer role grants `show`, and the operator role adds
# `validate`, `load`, `unload` and `stop`. The rule is checked with `visudo
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
# `weaver-state-PRD` section 4: the member holds its store's file in a state
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
DECL_DIR=""
CONNECTOR_ROLE=operator
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
   agent's name. Lay an agent made before this date out as create-agent.sh now
   makes one: root:weaver-<name>-state 0710, its trace root:weaver-<name>-trace
   0640 (deploy/REDEPLOY.md, existing territories)." ;;
    --engine)   [ $# -ge 2 ] || die "--engine needs a name"; ENGINE=$2; shift ;;
    --spu)      [ $# -ge 2 ] || die "--spu needs a path"; SPU_OVERRIDE=$2; shift ;;
    --declaration-directory) [ $# -ge 2 ] || die "--declaration-directory needs a path"; DECL_DIR=$2; shift ;;
    --connector-role) [ $# -ge 2 ] || die "--connector-role needs operator or observer"; CONNECTOR_ROLE=$2; shift ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done


# The name is a unix user, a group and a directory, so it is bounded to what
# all three accept without quoting.
[ -n "$NAME" ] || die "name the agent: create-agent.sh <name> --artifact <path>"
[[ "$NAME" =~ ^[a-z][a-z0-9]{1,15}$ ]] || die "the name is lowercase letters and digits, 2 to 16 characters: '$NAME'"
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
for pair in "session:$SESSION" "artifact:$ARTIFACT"; do
  field=${pair%%:*} value=${pair#*:}
  if [[ "$value" == *[\"\\]* || "$value" =~ [[:cntrl:]] ]]; then
    die "the $field '$value' carries a double quote, a backslash or a control character, which the declaration's TOML string cannot hold as written"
  fi
done
# The SPU path is one line of a key file admin reads, so it is absolute and
# carries no control character, on the ground the paragraph above gives.
if [ -n "$SPU_OVERRIDE" ]; then
  [[ "$SPU_OVERRIDE" == /* ]] || die "--spu takes an absolute path: '$SPU_OVERRIDE'"
  [[ "$SPU_OVERRIDE" =~ [[:cntrl:]] ]] && die "--spu carries a control character, which a key file cannot hold as one line"
fi
case "$CONNECTOR_ROLE" in
  operator) VERBS="show validate load unload stop" ;;
  observer) VERBS="show" ;;
  *) die "--connector-role is operator or observer: '$CONNECTOR_ROLE'" ;;
esac
# **The operator is the account running this script**, never root: the
# declaration directory is theirs, made and written as them, and the root's
# `operator` key names their uid.
[ "$(id -u)" -ne 0 ] || die "run as the operator, not under sudo: the declaration directory is the operator's own, and --apply asks for sudo itself"


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
OPERATOR_HOME=$(getent passwd "$OPERATOR" | cut -d: -f6)
[ -n "$OPERATOR_HOME" ] || die "cannot read the operator $OPERATOR's home from the account database"
DECL_DIR=${DECL_DIR:-$OPERATOR_HOME/.weaveragent/$NAME}
[[ "$DECL_DIR" == /* ]] || die "--declaration-directory takes an absolute path: '$DECL_DIR'"
[[ "$DECL_DIR" =~ [[:cntrl:]] ]] && die "the declaration directory carries a control character, which a key file cannot hold as one line"
AGENT_USER="weaver-$NAME"          # the agent's own uid: the worker's identity
MEMBER_USER="weaver-$NAME-state"   # the member's uid: holds the territory
TRACE_GROUP="weaver-$NAME-trace"   # the trace's readers: the operator and the relay, never the member
RELAY_USER="weaver-$NAME-relay"    # the trace relay's uid, its one group the trace group
ACCESS_GROUP="weaver-$NAME-admin"  # the trace door's group, which the declared reader holds
CONNECTOR_USER="weaver-$NAME-admincon"  # the connector's service user: the reader, and the sudo rule's one user
SUDO_RULE="/etc/sudoers.d/weaver-$NAME"
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
AGENT_ROOT="$ADMIN_BASE/$NAME"
STAGE="$ADMIN_BASE/.$NAME.partial"
DECLARATION="$DECL_DIR/agent.toml"

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

# **held_for_operator PATH: admin's rule for the declaration directory's
# ancestors**, per weaver-admin-Spec section 9: from the nearest directory that
# stands up to `/`, each owned by uid 0 or by the operator and writable by no
# group or other unless sticky. Fails printing the first component that is
# not so. The directory itself, where it stands, is judged apart: the
# operator's, granting group and other nothing.
held_for_operator() {
  local at owner mode
  at=$1
  while [ ! -e "$at" ] && [ ! -L "$at" ]; do at=$(dirname -- "$at"); done
  at=$(realpath -e -- "$at" 2>/dev/null) || { printf '%s' "$1"; return 1; }
  while :; do
    read -r owner mode < <(stat -c '%u %a' -- "$at" 2>/dev/null) || { printf '%s' "$at"; return 1; }
    if { [ "$owner" != 0 ] && [ "$owner" != "$OPERATOR_UID" ]; } || { (( 8#$mode & 8#022 )) && ! { [ -d "$at" ] && (( 8#$mode & 8#1000 )); }; }; then
      printf '%s' "$at"; return 1
    fi
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
# **The stack record is read without privilege in both modes.** It is
# root-owned and world-readable by construction, holds no secret, and every
# value the declaration renders from it must be known before sudo is asked
# for. A key that is absent, empty or unreadable refuses by name; only the
# optional keys may be absent.
REQUIRED_KEYS="worker-binary spu-binary gate-binary coordination-root"
OPTIONAL_KEYS="headroom-bytes library-path load-bound-seconds"
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
# **The rule names the program sudo runs as root, so it is judged first**: the
# installed weaver-admin, by its canonical path, held closed by admin's rule.
ADMIN_BINARY=$(realpath -e -- "$PREFIX/bin/weaver-admin" 2>/dev/null) || die "no weaver-admin at $PREFIX/bin/weaver-admin"
bad=$(held_closed "$ADMIN_BINARY") || die "weaver-admin at $ADMIN_BINARY is not held closed by root at $bad, so no sudo rule names it"
# **Admin starts the relay from beside the worker for a file sink**
# (weaver-admin-Spec section 9), so a box without it is refused here rather
# than at the first load.
RELAY_BINARY="$(dirname -- "$(stack_key worker-binary required)")/weaver-trace-relay"
[ -x "$RELAY_BINARY" ] || die "no weaver-trace-relay beside the worker at $RELAY_BINARY: update-stack.sh installs it"
SPU_BINARY=${SPU_OVERRIDE:-$(stack_key spu-binary required)} || exit 1
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
# in it, since the territory is a new name there. Each territory is root-owned
# and grouped to its state member, 0710, so the member passes to its room by
# group and the agent's uid, in no group of it, cannot enter; the trace has a
# group of its own (below). No ACL is asked of the
# filesystem, so a box whose datasets carry none deploys as any other.
[ -d "$AGENT_DIR" ] || die "the stack record's agent-directory $AGENT_DIR does not stand: bootstrap-stack.sh makes it, root 0755"
bad=$(creatable_in "$AGENT_DIR") || die "the stack record's agent-directory $AGENT_DIR is not a root-held base no other principal can make a name in ($bad)"
HOME_DIR="$AGENT_DIR/$AGENT_USER"
STATE_DIR="$HOME_DIR/state"

# **The declaration directory is judged as admin will judge it**, before
# anything is provisioned (weaver-admin-Spec section 9): every directory above
# it root's or the operator's and closed, and the directory itself, where it
# stands, the operator's, no link, granting group and other nothing, and
# holding no `agent.toml` yet. Where it does not stand it is made 0700 as the
# operator at the apply.
bad=$(held_for_operator "$(dirname -- "$DECL_DIR")") || die "the declaration directory $DECL_DIR stands under $bad, which another principal could write, so admin would refuse it"
if [ -e "$DECL_DIR" ] || [ -L "$DECL_DIR" ]; then
  { [ ! -L "$DECL_DIR" ] && [ -d "$DECL_DIR" ]; } || die "the declaration directory $DECL_DIR is a link or not a directory"
  read -r d_owner d_mode < <(stat -c '%u %a' -- "$DECL_DIR")
  [ "$d_owner" = "$OPERATOR_UID" ] || die "the declaration directory $DECL_DIR is not $OPERATOR's"
  (( 8#$d_mode & 8#077 )) && die "the declaration directory $DECL_DIR grants group or other access (mode $d_mode), and admin requires it closed to everyone but its owner"
  { [ ! -e "$DECLARATION" ] && [ ! -L "$DECLARATION" ]; } || die "a declaration already stands at $DECLARATION"
fi

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
render_declaration() {
cat <<TOML
session = "$SESSION"
tool-set = []
permission-mode = "deny"
# **A serving binding carries a gate instruction and the inventory refuses it
# absent.** An unstated binding-kind resolves to serving, so both are written
# rather than left to a default a reader cannot see.
binding-kind = "serving"

[spu-instruction.decoder]
residual-readout-election = false
surprisal-election = true
tunable-values = { context-capacity = 32768, max-tokens-per-turn = 4096, seed = 451234785645 }

[spu-instruction.decoder.model-binding]
artifact = "$ARTIFACT"
devices = [0]

[[spu-instruction.decoder.identity]]
role = "system"

[[spu-instruction.decoder.identity.content]]
type = "text"
text = """
You are a careful assistant. Answer from what you know, say
plainly when you do not know, and keep answers as short as the
question allows.
"""

[gate-instruction.access-rule]
allowed-uids = [$(id -u "$OPERATOR")]
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

say "plan for agent '$NAME'"
plan "agent account   $AGENT_USER      (system, nologin, the worker's uid)"
plan "member account  $MEMBER_USER     (system, nologin, owns the state territory)"
plan "trace group     $TRACE_GROUP     (system group: the trace's readers, never the member)"
plan "relay account   $RELAY_USER      (system, nologin, no home, its one group $TRACE_GROUP)"
plan "access group    $ACCESS_GROUP    (system group: the trace door's, which the reader holds)"
plan "connector       $CONNECTOR_USER  (system, nologin, no home, holds $ACCESS_GROUP, the trace reader)"
plan "operator        $OPERATOR joins groups $AGENT_USER, $MEMBER_USER and $TRACE_GROUP"
plan "home            /home/$AGENT_USER        the agent's own, where its tools run"
plan "directory       $HOME_DIR        root:$MEMBER_USER 0710, passage only, no listing"
plan "trace           $HOME_DIR/trace.ndjson  root:$TRACE_GROUP 0640, made before the first load"
plan "state territory $STATE_DIR       $MEMBER_USER 0700, which the agent's uid cannot enter"
plan "store           sqlite, its file in the state territory"
plan "agent root      $AGENT_ROOT      root 0755, keys 0644, copied from $STACK"
plan "spu-binary      $SPU_BINARY$( [ -n "$SPU_OVERRIDE" ] && printf '  (--spu, in place of the stack record'"'"'s)' )"
plan "operator key    $OPERATOR_UID ($OPERATOR)"
plan "roles.toml      trace-reader = $CONNECTOR_USER"
plan "declaration dir $DECL_DIR      $OPERATOR 0700, where admin.log and worker.log land"
plan "declaration     $DECLARATION     session $SESSION, artifact $ARTIFACT"
plan "sudo rule       $SUDO_RULE      $CONNECTOR_USER, $CONNECTOR_ROLE: $VERBS, !pam_session"
plan "store engine    $ENGINE         which the deployed member must carry"

# What must not already be there. Creation is refused rather than merged,
# because a half-made agent that looks whole is worse than an absent one.
say "checks"
for u in "$AGENT_USER" "$MEMBER_USER" "$RELAY_USER" "$CONNECTOR_USER"; do
  if getent passwd "$u" >/dev/null; then
    die "the account $u already exists"
  else
    account_status=$?
    [ "$account_status" -eq 2 ] || die "cannot read account $u"
  fi
done
# The groups this run makes, the trace and access groups and the same-named
# group `useradd --user-group` makes for the agent, the member and the
# connector, each checked before anything is provisioned (Codex on #79).
for g in "$TRACE_GROUP" "$ACCESS_GROUP" "$AGENT_USER" "$MEMBER_USER" "$CONNECTOR_USER"; do
  if getent group "$g" >/dev/null; then
    die "the group $g already exists"
  else
    group_status=$?
    [ "$group_status" -eq 2 ] || die "cannot read group $g"
  fi
done
refuse_existing "/home/$AGENT_USER" "agent home"
refuse_existing "$HOME_DIR" "territory"
refuse_existing "$AGENT_ROOT" "agent root"
refuse_existing "$STAGE" "a partial agent root from an earlier run (remove it by hand)"
refuse_existing "$SUDO_RULE" "sudo rule"
[ -r "$ARTIFACT" ] || printf '   WARNING: the artifact is not readable from this shell: %s\n' "$ARTIFACT"
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
# **The connector holds the access group and nothing of the agent's**: it
# reaches the trace through the door alone, never by the trace group.
sudo useradd --system --shell /usr/sbin/nologin --no-create-home --user-group --groups "$ACCESS_GROUP" "$CONNECTOR_USER"
sudo usermod -aG "$AGENT_USER,$MEMBER_USER,$TRACE_GROUP" "$OPERATOR"
sudo chmod 2750 "/home/$AGENT_USER"
printf '   %s uid %s, %s uid %s, %s uid %s, %s uid %s\n' \
  "$AGENT_USER" "$(id -u "$AGENT_USER")" "$MEMBER_USER" "$(id -u "$MEMBER_USER")" \
  "$RELAY_USER" "$(id -u "$RELAY_USER")" "$CONNECTOR_USER" "$(id -u "$CONNECTOR_USER")"

say "territory"
# **The member passes through; the trace is not its to read** (operator's ruling
# of 2026-10-02, #28, as refined on #56). Admin makes the member's room at
# `<sink directory>/state`, so the member must traverse the directory holding
# the trace, and the protection sits on the trace file rather than on the
# directory. The territory is root's, grouped to the member, 0710 and not
# setgid: the member may pass to its room but not list, and nothing written
# here takes the member's group. The trace is made here, before the first load,
# root:$TRACE_GROUP 0640, the layout admin checks at every load (#62): admin
# opens it append-only, never through a link, and refuses a trace that stands
# otherwise, so the member, outside that group, cannot read it, and the
# operator reads it through the group. Were the file ever removed, admin
# recreates it at this same layout where the declaration elects creation. The
# agent's uid, in no group of either, cannot enter. The state subdirectory is the member's own.
sudo install -d -o root -g "$MEMBER_USER" -m 0710 "$HOME_DIR"
sudo install -o root -g "$TRACE_GROUP" -m 0640 /dev/null "$HOME_DIR/trace.ndjson"
sudo install -d -o "$MEMBER_USER" -g "$MEMBER_USER" -m 0700 "$STATE_DIR"


say "declaration"
# **Made and written as the operator**, never as root: the directory is the
# operator's own, 0700, and the declaration in it the operator's, so every
# later edit is the operator's without a privileged write.
( umask 077; mkdir -p -- "$DECL_DIR" )
chmod 0700 -- "$DECL_DIR"
( umask 077; printf '%s\n' "$DECLARATION_TEXT" > "$DECLARATION" )
printf '   %s written, %s 0700\n' "$DECLARATION" "$DECL_DIR"

say "agent root, staged"
# **Root-owned and not group- or world-writable, or admin refuses it**, so it
# is made by root at 0755 with its files 0644. Every key the stack record
# holds is copied as written. `spu-binary` is this agent's `--spu` where given,
# and `declaration-directory`, `operator` and `roles.toml` are the agent's own.
sudo install -d -o root -g root -m 0755 "$STAGE"
for key in $REQUIRED_KEYS $OPTIONAL_KEYS; do
  [ -e "$STACK/$key" ] || continue
  if [ "$key" = spu-binary ] && [ -n "$SPU_OVERRIDE" ]; then
    printf '%s\n' "$SPU_OVERRIDE" | sudo tee "$STAGE/$key" >/dev/null
  else
    sudo cp -- "$STACK/$key" "$STAGE/$key"
  fi
done
printf '%s\n' "$DECL_DIR" | sudo tee "$STAGE/declaration-directory" >/dev/null
printf '%s\n' "$OPERATOR_UID" | sudo tee "$STAGE/operator" >/dev/null
# **The boundary file names the trace door's one reader**, the connector's
# user, in the shape of weaver-types-Spec section 3.1 and nothing else.
printf 'trace-reader = "%s"\n' "$CONNECTOR_USER" | sudo tee "$STAGE/roles.toml" >/dev/null
sudo chmod 0644 "$STAGE"/*
printf '   staged at %s\n' "$STAGE"

say "the wall, verified rather than assumed"
# A refusal here leaves the root staged and not admitted.
# **The store's gate is the room itself**: the member opens its file in the
# state territory, so the member must be able to write there and the agent's
# own uid must not be able to enter it.
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

# **The relay and the connector hold exactly their groups**: the relay the
# trace group alone, the connector its own and the access group, and neither
# any group of the agent's or the member's.
relay_groups=$(id -Gn "$RELAY_USER")
[ "$relay_groups" = "$TRACE_GROUP" ] || die "the relay account $RELAY_USER holds '$relay_groups', not the trace group alone"
connector_groups=" $(id -Gn "$CONNECTOR_USER") "
[[ "$connector_groups" == *" $ACCESS_GROUP "* ]] || die "the connector $CONNECTOR_USER does not hold $ACCESS_GROUP, so the trace door would turn it away"
for g in "$AGENT_USER" "$MEMBER_USER" "$TRACE_GROUP"; do
  [[ "$connector_groups" != *" $g "* ]] || die "the connector $CONNECTOR_USER holds $g, which reaches the agent's territory or trace by group"
done
printf '   %s holds %s alone, %s holds%s\n' "$RELAY_USER" "$TRACE_GROUP" "$CONNECTOR_USER" "${connector_groups% }"

say "sudo rule"
# **Rendered, checked by visudo, then installed root 0440.** Each grant is one
# fixed command line with its arguments, so sudo matches it exactly and the
# connector can pass none of its own.
sudo -n grep -qE '^[#@]includedir[[:space:]]+/etc/sudoers\.d([[:space:]]|$)' /etc/sudoers \
  || die "/etc/sudoers does not include /etc/sudoers.d, so a rule there would never be read"
lines=""
for verb in $VERBS; do
  lines="$lines${lines:+, }$ADMIN_BINARY $verb $NAME"
done
RULE_TEXT=$(cat <<SUDOERS
# weaver-$NAME: the connector's fixed command lines, per weaver-admin-Spec
# section 2. Written by deploy/create-agent.sh, $CONNECTOR_ROLE role.
Defaults:$CONNECTOR_USER !pam_session
$CONNECTOR_USER ALL=(root) NOPASSWD: $lines
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
sudo -n -l -U "$CONNECTOR_USER" | sed -n '/may run the following/,$p' | sed 's/^/   /'

say "admission"
# **Moving the root into place is the admission**: before it every admin verb
# answers no_such_agent for this name, and after it the agent is admitted.
sudo mv -T -- "$STAGE" "$AGENT_ROOT"
printf '   %s admitted at %s\n' "$NAME" "$AGENT_ROOT"

say "made"
# **The group add applies to a login taken after it.** `usermod -aG` changes
# the account and not a session already running, so a shell that predates this
# run cannot reach the gate's socket until it takes the group (#673, measured
# on the W4a run of 2026-09-25).
printf '   %s joined groups %s, %s and %s: a session that predates this run needs a new\n' "$OPERATOR" "$AGENT_USER" "$MEMBER_USER" "$TRACE_GROUP"
printf '   login before the groups apply (`newgrp` selects one group in one shell)\n'
# **probe_connector VERB: the connector reaches a line through its rule**, as
# the stand-in for admin-con: the operator's sudo becomes the connector's user,
# and that user runs `sudo -n`, which only the rule can satisfy. It succeeds
# only on the answer the verb owes, `validated` for validate and a state for
# show, and prints what came back either way (Codex on #79).
probe_connector() {
  local said answer
  said=$(sudo -n -u "$CONNECTOR_USER" sudo -n "$ADMIN_BINARY" "$1" "$NAME" 2>&1) || true
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
    printf '   %s ran %s through its rule: %s\n' "$CONNECTOR_USER" "$check_verb" "$answer"
  else
    die "$NAME is admitted, but its connector $CONNECTOR_USER could not run $check_verb through $SUDO_RULE: $answer. Another sudo policy may override the rule. Listing what it grants the connector is in deploy/HowToDeployANewAgent.md section 5."
  fi
fi
printf '   validate it before loading, with the validate verb of %q for %s as root,\n' "$ADMIN_BINARY" "$NAME"
printf '   then prove its load with deploy/verify-load.sh as root (deploy/HowToDeployANewAgent.md section 4)\n'
