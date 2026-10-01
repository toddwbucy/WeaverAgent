#!/usr/bin/env bash
# Create one agent on this box: its accounts, its territory, the state store
# behind its member's seam, and its root of admin configuration.
#
#   ./deploy/create-agent.sh fred --engine sqlite --artifact /path/to.gguf            plan only
#   ./deploy/create-agent.sh fred --engine sqlite --artifact /path/to.gguf --apply    act
#
# Options: --engine sqlite|postgres (required), --session <name>
# (default <name>-001), --spu <path> (this agent's SPU, in place of the
# stack record's `spu-binary`).
#
# **The agent's root is `<admin base>/<name>/` and it is the admission.**
# Admin reads that directory and nothing shared, so this script writes every
# key admin requires into it, copied from the stack record `bootstrap-stack.sh`
# wrote at `/etc/weaver/stack/` (which admin never reads), plus the agent's own
# `log-path` and its declaration as `agent.toml`. The root is staged under a
# dot-name admin's name check refuses, and moved into place last, so a run that
# stops part way leaves no root admin would admit.
#
# **The program creates nothing and this is why the script exists.** Admin's
# inventory refuses a missing home rather than building one, per
# `weaver-admin-Spec` section 4, so every part of an agent's territory is the
# operator's to make. This file is that act written down once instead of
# remembered.
#
# **Two accounts, because the wall is two gates and one identity**, per
# `weaver-state-PRD` section 4 as ruled 2026-09-04. The service gate is
# kernel-class: the member dials the store's socket under its own account and
# the peer is verified by credential. The object gate is the database and the
# role's grants. Peer authentication welds them, the store mapping the
# member's kernel identity to its role, so the agent's own uid maps to no
# role and is refused at the second gate where it was not refused at the
# first. That refusal is verified at the end rather than assumed.
#
# **No password exists anywhere in here.** Peer authentication derives the
# object gate's identity from the kernel fact rather than asserting it a
# second time, so there is no secret to store, rotate, or leak.
#
# **The member's account is derived and no longer named**, as of 2026-09-15 and
# issue #545. `weaver-admin` resolves `weaver-<name>-state`, drops to it at the
# member's spawn, hands it the territory by chown, and asks the store's first
# gate as it, so there is one account the store must admit and this script
# cannot choose a weaker one. The `--member-identity` flag that named the
# choice while the code ran the member as root is refused rather than ignored,
# a box provisioned under it having mapped root in `pg_ident.conf`.
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

NAME=${1:-}
shift || true
APPLY=0
ARTIFACT=""
SESSION=""
# **The engine is an election and not a constant.** An earlier form wrote
# `postgres` into every declaration with nothing saying so, while
# `deploy/update-stack.sh` separately named which engines the build carries.
# Two statements of one fact from two decisions is how a declaration comes to
# elect an engine the installed member cannot serve, which is what happened on
# 2026-09-11. They are still two statements, deliberately, because a build
# serves engines no agent has elected yet; `update-stack.sh` reconciles them
# before it spends a build, and refuses by name where they disagree.
# **No engine is the default.** postgres and sqlite stand side by side, on
# the operator's ruling on #38, so the operator names one and its absence is
# refused below rather than read as either.
ENGINE=
SPU_OVERRIDE=""
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
   agent's name, and the store must admit that and nothing else. An agent made
   before this date mapped root: change its pg_ident.conf line to name
   weaver-<name>-state, give that account traversal to its territory, and chown
   the territory to it." ;;
    --engine)   [ $# -ge 2 ] || die "--engine needs a name"; ENGINE=$2; shift ;;
    --spu)      [ $# -ge 2 ] || die "--spu needs a path"; SPU_OVERRIDE=$2; shift ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done


# The name is a unix user, a role, a database and a directory, so it is
# bounded to what all four accept without quoting.
[ -n "$NAME" ] || die "name the agent: create-agent.sh <name> --engine <sqlite|postgres> --artifact <path>"
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


# **An engine this script cannot provision is refused here rather than written
# into a declaration.** `weaver-types` admits `none`, `sqlite` and `postgres`,
# and anything else fails the inventory's parse after every account, database
# and access entry has already been made. A sqlite agent is the postgres one
# minus the store half: the same two accounts, territory and state room, the
# member keeping its database file in the room, and no role, database or
# admission line, which the inventory refuses for it. `none` is a lawful
# election and not one this script serves: an agent electing no store has no
# member and no room to verify. Declare that one by hand.
case "$ENGINE" in
  "") die "name the store engine: --engine sqlite or --engine postgres. Neither is the default." ;;
  sqlite|postgres) ;;
  none) die "none is a lawful election and not one this script makes: there is no member, no state room and no store to probe. Declare it by hand, per deploy/HowToDeployANewAgent.md section 3. What this option exists for is to name the engine rather than assume it, so that deploy/update-stack.sh can reconcile the declaration against the build." ;;
  *) die "no store engine named $ENGINE. weaver-types admits none, sqlite and postgres, and this script provisions sqlite and postgres." ;;
esac

OPERATOR=${SUDO_USER:-$USER}
AGENT_USER="weaver-$NAME"          # the agent's own uid: the worker's identity
MEMBER_USER="weaver-$NAME-state"   # the member's uid: holds the territory
ROLE="weaver_$NAME"                # postgres spells with underscores
DATABASE="weaver_$NAME"
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
AGENT_ROOT="$ADMIN_BASE/$NAME"
STAGE="$ADMIN_BASE/.$NAME.partial"
DECLARATION="$AGENT_ROOT/agent.toml"

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
REQUIRED_KEYS="worker-binary spu-binary gate-binary run-tool control-tool coordination-root unit-properties"
OPTIONAL_KEYS="headroom-bytes state-store-socket"
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
for key in $REQUIRED_KEYS; do stack_key "$key" required >/dev/null || exit 1; done
for key in $OPTIONAL_KEYS; do stack_key "$key" optional >/dev/null || exit 1; done
LOG_DIR=$(stack_key log-directory required) || exit 1
AGENT_DIR=$(stack_key agent-directory required) || exit 1
LOG_PATH="$LOG_DIR/$NAME/admin.log"
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
# **The territory sits under the operator's home**, because the traversal the
# member is given below is a chain of access entries from that home down, and
# this script opens no passage anywhere else.
case "$AGENT_DIR/" in
  "/home/$OPERATOR/"*) ;;
  *) die "the stack record's agent-directory $AGENT_DIR is not under /home/$OPERATOR, and the member's passage to its territory is opened from there" ;;
esac
HOME_DIR="$AGENT_DIR/$AGENT_USER"
STATE_DIR="$HOME_DIR/state"

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

# **The engine, the database and the role are members of the binding**, per
# weaver-state-PRD section 4: declared here, changing only across the load
# boundary, and named on the load event like every fact that decides a
# record.
[state-store]
engine = "$ENGINE"
TOML
if [ "$ENGINE" = postgres ]; then
  printf 'database = "%s"\nrole = "%s"\n' "$DATABASE" "$ROLE"
fi
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

# **Whose identity the store admits is settled and derived.** The charter has
# the member hold a uid of its own and dial the store under it, and as of
# 2026-09-15 the code does: the account is the one this script makes, so the
# identity map, the territory's owner and the spawn's uid are one fact rather
# than three the operator keeps agreeing.

HBA=""; IDENT=""   # asked of the store itself rather than guessed from a distro path

say "plan for agent '$NAME'"
plan "agent account   $AGENT_USER      (system, nologin, the worker's uid)"
plan "member account  $MEMBER_USER     (system, nologin, owns the state territory)"
plan "operator        $OPERATOR joins group $AGENT_USER"
plan "home            /home/$AGENT_USER        the agent's own, where its tools run"
plan "directory       $HOME_DIR        root:$OPERATOR 2750"
plan "state territory $STATE_DIR       $MEMBER_USER 0700, which the agent's uid cannot enter"
if [ "$ENGINE" = postgres ]; then
  plan "role            $ROLE            postgres, no password, peer only"
  plan "database        $DATABASE        owned by $ROLE"
  plan "admission       local $DATABASE $ROLE peer map=weaver"
  plan "identity map    weaver $MEMBER_USER -> $ROLE"
else
  plan "store           sqlite, its file in the state territory; no role, database or admission line"
fi
plan "agent root      $AGENT_ROOT      root 0755, keys 0644, copied from $STACK"
plan "spu-binary      $SPU_BINARY$( [ -n "$SPU_OVERRIDE" ] && printf '  (--spu, in place of the stack record'"'"'s)' )"
plan "log-path        $LOG_PATH        its directory root 0750"
plan "declaration     $DECLARATION     session $SESSION, artifact $ARTIFACT"
plan "store engine    $ENGINE         which the deployed member must carry"

# What must not already be there. Creation is refused rather than merged,
# because a half-made agent that looks whole is worse than an absent one.
say "checks"
for u in "$AGENT_USER" "$MEMBER_USER"; do
  if getent passwd "$u" >/dev/null; then
    die "the account $u already exists"
  else
    account_status=$?
    [ "$account_status" -eq 2 ] || die "cannot read account $u"
  fi
done
refuse_existing "/home/$AGENT_USER" "agent home"
refuse_existing "$HOME_DIR" "territory"
refuse_existing "$AGENT_ROOT" "agent root"
refuse_existing "$STAGE" "a partial agent root from an earlier run (remove it by hand)"
refuse_existing "$LOG_DIR/$NAME" "operations log directory"
[ -r "$ARTIFACT" ] || printf '   WARNING: the artifact is not readable from this shell: %s\n' "$ARTIFACT"
[ -z "$SPU_OVERRIDE" ] || [ -x "$SPU_OVERRIDE" ] || printf '   WARNING: the --spu path is not executable from this shell: %s\n' "$SPU_OVERRIDE"
if [ "$APPLY" -eq 0 ]; then
  printf '   no collision found in accounts and paths visible to this uid\n'
  printf '   PENDING --apply: privileged collision checks, service and store catalogs\n'
  printf '   PENDING --apply: authentication paths and filesystem access-entry probe\n'
  say "plan only"
  printf '   no provisioning performed; rerun with --apply to check and make it\n'
  exit 0
fi
# **Traversal is asked about here rather than discovered halfway through.** The
# member needs passage along a chain that runs through the operator's own home,
# which is 0700, and this pool answers `setfacl` with Operation not supported,
# so the need and the means are checked together before anything is made.
# **The probe sits on the filesystem that will hold the territory**, which is
# not always the operator's home: `.weaveragents` can be a mount or a dataset
# of its own, and access entries are a property of the filesystem rather than
# of the tree. Where that parent does not exist yet the home is the right
# stand-in, being where the script is about to create it. **The entry names the
# operator and not the member**, the member's account not existing until the
# apply below makes it, and what is asked here is whether the filesystem
# carries entries at all rather than which account gets one.
probe_parent="$AGENT_DIR"
[ -d "$probe_parent" ] || probe_parent="/home/$OPERATOR"
probe=$(mktemp -d "$probe_parent/.acl-probe-XXXXXX") || die "cannot write under $probe_parent"
if setfacl -m "u:$OPERATOR:x" "$probe" 2>/dev/null; then
  printf '   this filesystem carries access entries, so %s can be given passage\n' "$MEMBER_USER"
else
  rmdir "$probe"
  die "$probe_parent refuses access entries, so $MEMBER_USER cannot traverse to
   its territory there. Place the territory on a filesystem that carries them,
   or somewhere the member can reach by ownership alone."
fi
rmdir "$probe"

if [ "$ENGINE" = postgres ]; then
# **A retired agent can leave its role or database behind.** Discovering
# that at CREATE ROLE would leave both accounts and directories half-made.
# Start the store and ask its catalogues before creating anything local.
# The reversible ACL probe runs first, so its refusal starts no service.
sudo -n systemctl start postgresql || die "cannot start PostgreSQL for preflight checks"
sudo -n systemctl is-active --quiet postgresql \
  || die "cannot confirm PostgreSQL is active for preflight checks"

# Judge each read's status before interpreting its answer. Failed queries
# must never become empty results, and psql startup files must not alter them.
read_store() {
  sudo -n -u postgres psql -X -v ON_ERROR_STOP=1 -tAc "$2" \
    || die "cannot read $1 from PostgreSQL"
}
role_exists=$(read_store "role catalog" "select 1 from pg_roles where rolname='$ROLE'") || exit 1
case "$role_exists" in
  1) die "the role $ROLE already exists: drop it or pick another name" ;;
  "") ;;
  *) die "unexpected role catalog answer" ;;
esac
database_exists=$(read_store "database catalog" "select 1 from pg_database where datname='$DATABASE'") || exit 1
case "$database_exists" in
  1) die "the database $DATABASE already exists: drop it or pick another name" ;;
  "") ;;
  *) die "unexpected database catalog answer" ;;
esac
HBA=$(read_store "hba_file" 'show hba_file') || exit 1
IDENT=$(read_store "ident_file" 'show ident_file') || exit 1
[ -n "$HBA" ] || die "empty hba_file from PostgreSQL"
[ -n "$IDENT" ] || die "empty ident_file from PostgreSQL"
printf '   the store is up and carries no %s\n' "$ROLE"
printf '   local collision checks completed\n'
# Both authentication files and the insertion anchor are known now. Refuse
# before accounts, directories, roles or databases are made, not at the edit.
for auth_file in "$HBA" "$IDENT"; do
  require_path -f "$auth_file" "authentication file is missing or not regular"
  require_path -r "$auth_file" "authentication file is not readable"
  require_path -w "$auth_file" "authentication file is not writable"
done
sudo -n grep -qE '^local[[:space:]]+all[[:space:]]+all[[:space:]]+peer([[:space:]]|$)' "$HBA" \
  || die "cannot find 'local all all peer' anchor in $HBA"
fi

say "accounts"
# **The agent gets a home and the member does not.** The agent's tools run
# somewhere, and the retirement of 2026-09-11 found one of three agents with
# a home and the others without, which is the drift a script ends. The member
# runs nothing and owns one room of the operator's tree instead.
sudo useradd --system --shell /usr/sbin/nologin --create-home --user-group "$AGENT_USER"
sudo useradd --system --shell /usr/sbin/nologin --no-create-home --user-group "$MEMBER_USER"
sudo usermod -aG "$AGENT_USER" "$OPERATOR"
sudo chmod 2750 "/home/$AGENT_USER"
printf '   %s uid %s, %s uid %s\n' \
  "$AGENT_USER" "$(id -u "$AGENT_USER")" "$MEMBER_USER" "$(id -u "$MEMBER_USER")"

say "territory"
# The directory is the operator's to read and the member's to own one room
# of. Setgid so the operator's group survives whatever writes here, and the
# state subdirectory closed to everyone else, which is what makes the
# charter's "one subdirectory the agent's uid cannot enter" true rather than
# stated.
sudo install -d -o root -g "$OPERATOR" -m 2750 "$HOME_DIR"
sudo install -d -o "$MEMBER_USER" -g "$MEMBER_USER" -m 0700 "$STATE_DIR"
# **Owning the room is not reaching it.** The operator's home is 0700 and
# every directory above the territory belongs to the operator, so the member
# cannot traverse to what it owns. Execute-only entries along the chain open
# passage without opening any listing, which is the narrowest thing that
# makes the ownership above true rather than stated.
steps=("/home/$OPERATOR")
IFS=/ read -ra parts <<< "${HOME_DIR#"/home/$OPERATOR/"}"
for part in "${parts[@]}"; do
  [ -n "$part" ] && steps+=("${steps[-1]}/$part")
done
for step in "${steps[@]}"; do
  sudo setfacl -m "u:$MEMBER_USER:x" "$step" \
    || die "no traversal for $MEMBER_USER at $step, and the member cannot reach its own territory"
done

if [ "$ENGINE" = postgres ]; then
say "store"
sudo -u postgres psql -X -v ON_ERROR_STOP=1 -c "CREATE ROLE $ROLE LOGIN;"
sudo -u postgres psql -X -v ON_ERROR_STOP=1 -c "CREATE DATABASE $DATABASE OWNER $ROLE;"

say "gates"
# The admission line precedes the catch-all, because pg_hba takes the first
# match and `local all all peer` would otherwise demand that the kernel name
# equal the role name, which is exactly what the map exists to avoid.
sudo cp -a "$HBA" "$HBA.before-$NAME"
sudo cp -a "$IDENT" "$IDENT.before-$NAME"
sudo sed -i "0,/^local\s\+all\s\+all\s\+peer/s||local   $DATABASE   $ROLE   peer map=weaver\nlocal   all             all                                     peer|" "$HBA"
printf 'weaver          %s                    %s\n' "$MEMBER_USER" "$ROLE" | sudo tee -a "$IDENT" >/dev/null
sudo systemctl reload postgresql
fi

say "agent root, staged"
# **Root-owned and not group- or world-writable, or admin refuses it**, so it
# is made by root at 0755 with its files 0644. Every key the stack record
# holds is copied as written; `spu-binary` is this agent's `--spu` where given,
# and `log-path` is the agent's own.
sudo install -d -o root -g root -m 0755 "$STAGE"
for key in $REQUIRED_KEYS $OPTIONAL_KEYS; do
  [ -e "$STACK/$key" ] || continue
  if [ "$key" = spu-binary ] && [ -n "$SPU_OVERRIDE" ]; then
    printf '%s\n' "$SPU_OVERRIDE" | sudo tee "$STAGE/$key" >/dev/null
  else
    sudo cp -- "$STACK/$key" "$STAGE/$key"
  fi
done
printf '%s\n' "$LOG_PATH" | sudo tee "$STAGE/log-path" >/dev/null
# **The sink is inside this agent's own territory and the script will not
# write it anywhere else.** A declaration of 2026-08-23 pointed one agent's
# sink at another's directory, so two agents were configured to write one
# record, and it survived three weeks because nothing checked. The path is
# derived here rather than accepted.
printf '%s\n' "$DECLARATION_TEXT" | sudo tee "$STAGE/agent.toml" >/dev/null
sudo chmod 0644 "$STAGE"/*
sudo install -d -o root -g root -m 0750 "$LOG_DIR/$NAME"
printf '   staged at %s; operations log directory %s\n' "$STAGE" "$LOG_DIR/$NAME"

say "both gates, verified rather than assumed"
# **Each probe names the role.** Without `-U` psql defaults the role to the
# connecting account's own name, so the check would ask about a role nobody
# created and fail for a reason that is not the gate.
# A refusal here leaves the root staged and not admitted.
if [ "$ENGINE" = postgres ]; then
  if sudo -u "$MEMBER_USER" psql -X -v ON_ERROR_STOP=1 -U "$ROLE" -d "$DATABASE" -c 'select 1' >/dev/null 2>&1; then
    printf '   %s reaches the database as %s\n' "$MEMBER_USER" "$ROLE"
  else
    die "the member cannot reach its database: the first gate or the map is wrong"
  fi
  if sudo -u "$AGENT_USER" psql -X -v ON_ERROR_STOP=1 -U "$ROLE" -d "$DATABASE" -c 'select 1' >/dev/null 2>&1; then
    die "THE AGENT'S UID REACHED THE DATABASE: the second gate is open"
  else
    printf "   the agent's own uid is refused, which is the gate the charter asks for\n"
  fi
else
  # **A sqlite store's gate is the room itself**: the member opens its file
  # in the state territory, so the member must be able to write there and
  # the agent's own uid must not be able to enter it.
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
fi

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
printf '   %s joined group %s: a session that predates this run needs a new\n' "$OPERATOR" "$AGENT_USER"
printf '   login, or `newgrp %s`, before the group applies\n' "$AGENT_USER"
printf '   validate it before loading:\n'
printf '     sudo WEAVER_ADMIN_CONFIG=%s %s validate %s\n' "$ADMIN_BASE" "$(dirname "$(stack_key worker-binary required)")/weaver-admin" "$NAME"
