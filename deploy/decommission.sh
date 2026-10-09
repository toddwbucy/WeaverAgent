#!/usr/bin/env bash
# Take every weaver agent off this box, archive what stood, and remove it.
#
#   sudo deploy/decommission.sh                      plan: what stands, what would go
#   sudo deploy/decommission.sh --archive DIR        archive to DIR, which is required:
#                                                    no directory stands on every box
#   sudo deploy/decommission.sh --purge DIR          remove what DIR's PURGE-LIST names;
#                                                    refuses unless DIR verifies
#
# Box-agnostic, and discovered rather than written: the config bases are every
# `/etc/weaver/admin*`, each agent's root is a directory under one, the install
# and territory paths are read out of them, the agents are the union of every
# root, every allow-list of the box-wide layout before 2026-10-01, and every
# `weaver-*` account and group. A box fact this script
# needs and cannot find is printed
# as unknown, never guessed, and the plan is the same reads the archive and the
# purge make.
#
# **Its precondition, stated once** (deploy/REDEPLOY.md section 1): this is a
# deliberate, destructive act on the whole box by the operator at a root shell.
# (1) It first disables every agent's sudo rule, so no connector can start a
# load. (2) It then checks again for any live run, by each root's `show` and by
# any process under a `weaver-*` account, and refuses if it finds one. (3) A
# load the operator starts from a root shell while it runs is outside what it
# defends: the operator quiesces the box first.
#
# **A running agent refuses the archive and the purge.** An agent of the
# per-agent layout runs while its run lock is held, which no unit shows, so
# each root's own admin is asked with `show`, and an agent it names running,
# in transition, or that it cannot answer for refuses by name. The units of the
# layout before #50 are still checked as before.
#
# **The territory is archived whole**: since the operator's ruling of
# 2026-10-07 on #1 it holds the declaration, the prompt draft, the two logs and
# the published save points beside the state room and the trace, so they ride
# in the territories' archive and go with the agent. The operator's home is
# never touched: a box from before that ruling keeps its `~/.weaveragent/`
# directories where they stand, unarchived and unpurged, the operator's own.
# So the box-wide layout's `agent-config-directory` is not read, and a
# territory base or a root's territory under the operator's home (the stack
# record's `agent-directory` defaulted there from 2026-10-02 to 2026-10-07) is
# said and skipped, never listed, archived or purged.
#
# **A territory is archived and purged only where it stands as create-agent.sh
# lays it** (#99 area 2 review, H4): every root's territory is looked at by
# its canonical path, as root, and one that is not root's and grouped
# `weaver-<agent>-state` is named with what was found, and refuses the archive
# and the purge.
# The sudo rules `/etc/sudoers.d/weaver-*` and the run directories under each
# coordination root go with the agent.
#
# **Three modes, because the archive is verified before anything is removed.**
# `--archive` writes tarballs, dumps, a box-facts file and a SHA256SUMS, then
# reads every archive back. `--purge` re-verifies the sums and removes only
# the paths the archive's own PURGE-LIST names, so a purge can never reach
# past what was kept. Running `--purge` against a directory `--archive` did
# not finish refuses by name.
#
# **Models stay.** `/opt/weaver/models` is neither a binary nor a
# configuration; it is a hash-pinned artifact the redeploy loads again, and
# the 0.5b ggufs are already under `backups/opt-weaver-models-20260827` on the
# bulk store. The archive records each model's sha256 and copies none.
#
# **Root, because most of what stands is root's or the agents'.** Territories
# are root's, 0710 to the state group, with files beneath grouped to the access
# and trace groups and the member's own room, the record's directory is root's
# and not searchable, and the log is root's. Run it under sudo; it refuses otherwise.
#
# **No store is discovered or purged.** The service engine retired on the
# operator's ruling of 2026-10-02 on #1, and its discovery here matched every
# `weaver%` database on the box (#35), so the PostgreSQL roles and databases a
# box from before the retirement still carries are the operator's to dump and
# drop by hand (deploy/REDEPLOY.md section 2). The member's save points live
# in each territory's state room and go with it.
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

MODE=plan
DEST=""
# **No archive directory is assumed**: none stands on every box, so a mode that
# writes or reads an archive refuses without one (#39).
case "${1:-}" in
  "") ;;
  --archive) MODE=archive; DEST="${2:-}"
             [ -n "$DEST" ] || die "--archive needs a directory to write the archive to: --archive DIR. No default is assumed, since no directory stands on every box" ;;
  --purge)   MODE=purge;   DEST="${2:-}"
             [ -n "$DEST" ] || die "--purge needs the directory --archive wrote: --purge DIR" ;;
  *) die "unknown argument $1" ;;
esac

[ "$(id -u)" -eq 0 ] || die "run under sudo: territories, the record and the log are not the operator's to read"

# The operator is whoever invoked sudo, and the agent config directory is
# read out of admin's config rather than derived from the operator's home.
OPERATOR=${SUDO_USER:-$(logname 2>/dev/null || echo root)}
HOST=$(hostname)
# **The operator's home, never touched** (the operator's ruling of 2026-10-07
# on #1), canonical, or empty where it does not resolve.
OP_HOME=$(getent passwd "$OPERATOR" | cut -d: -f6)
OP_HOME=$( [ -n "$OP_HOME" ] && realpath -e -- "$OP_HOME" 2>/dev/null || true)
# in_operator_home PATH: 0 where PATH, canonical, is the operator's home or
# under it.
in_operator_home() {
  local c
  [ -n "${OP_HOME:-}" ] && [ "$OP_HOME" != / ] || return 1
  c=$(realpath -e -- "$1" 2>/dev/null) || c=$1
  case "$c" in "$OP_HOME"|"$OP_HOME"/*) return 0 ;; esac
  return 1
}

# ------------------------------------------------------------- 1. discovery
say "box"
plan "host      $HOST"
plan "operator  $OPERATOR"
plan "date      $(date -Iseconds)"
plan "mode      $MODE"
plan "archive   ${DEST:-none: plan only (--archive DIR names one)}"

CONFIG_ROOTS=()
for d in /etc/weaver/admin*; do [ -d "$d" ] && CONFIG_ROOTS+=("$d"); done

read_key() { cat "$1/$2" 2>/dev/null || true; }

# Every path admin's configs name, so the install tree is the one the box
# actually ran and not the one this script remembers.
declare -A BIN_DIRS=() LOG_PATHS=() COORD_ROOTS=()
ALLOWED=""
# **The per-agent roots**: every directory under a base, named as admin's name
# check admits it, a link never one. Each names its binaries, its territory
# and its coordination root.
AGENT_ROOTS=()
for base in "${CONFIG_ROOTS[@]}"; do
  for root in "$base"/*/; do
    root=${root%/}
    [ -d "$root" ] && [ ! -L "$root" ] || continue
    [[ "${root##*/}" =~ ^[A-Za-z0-9_-]+$ ]] || continue
    AGENT_ROOTS+=("$root")
    for k in worker-binary spu-binary gate-binary; do
      v=$(read_key "$root" "$k"); [ -n "$v" ] && BIN_DIRS["$(dirname "$v")"]=1
    done
    v=$(read_key "$root" coordination-root); [ -n "$v" ] && COORD_ROOTS["$v"]=1
    ALLOWED="$ALLOWED ${root##*/}"
  done
done
for root in "${CONFIG_ROOTS[@]}"; do
  for k in worker-binary spu-binary gate-binary; do
    v=$(read_key "$root" "$k"); [ -n "$v" ] && BIN_DIRS["$(dirname "$v")"]=1
  done
  v=$(read_key "$root" log-path); [ -n "$v" ] && LOG_PATHS["$(dirname "$v")"]=1
  ALLOWED="$ALLOWED $(read_key "$root" allow-list | tr '\n' ' ')"
  v=$(read_key "$root" spu-implementations)
  [ -n "$v" ] && BIN_DIRS["$(dirname "$(echo "$v" | awk '{print $2}')")"]=1
done
# The install prefixes: the parent of each bin dir, deduplicated.
declare -A PREFIXES=()
for b in "${!BIN_DIRS[@]}"; do PREFIXES["$(dirname "$b")"]=1; done

# Agents: roots, allow-lists, and accounts.
declare -A AGENTS=()
for a in $ALLOWED; do AGENTS["$a"]=1; done
mapfile -t WEAVER_USERS < <(getent passwd | awk -F: '$1 ~ /^weaver-/ {print $1}')
mapfile -t WEAVER_GROUPS < <(getent group | awk -F: '$1 ~ /^weaver-/ {print $1}')
# Every account and group the agent's provisioning makes carries one of the
# reserved suffixes (weaver-admin-Spec section 4), stripped to the agent's name.
strip_suffix() {
  local n=${1#weaver-}
  for suffix in -admincon -state -relay -trace -admin; do
    [ "${n%"$suffix"}" != "$n" ] && { printf '%s' "${n%"$suffix"}"; return; }
  done
  printf '%s' "$n"
}
for u in "${WEAVER_USERS[@]}"; do AGENTS["$(strip_suffix "$u")"]=1; done
for g in "${WEAVER_GROUPS[@]}"; do AGENTS["$(strip_suffix "$g")"]=1; done
# **sudo_rules DIR: the agents' sudo rules standing or held in DIR**: the
# ones create-agent.sh installs, root's to read; any an archive of this
# script disabled for its snapshot; and any an install of update-stack.sh
# held for its window and, killed inside it, never put back (#107). The held
# names are dot-names sudo never reads, archived and purged as the rest.
sudo_rules() {
  find "$1" -maxdepth 1 -type f \( -name 'weaver-*' -o -name '.weaver-*.decommissioning' \
    -o -name '.weaver-*.updating' \) 2>/dev/null | sort
}
mapfile -t SUDO_RULES < <(sudo_rules /etc/sudoers.d)

say "config roots"
for r in "${CONFIG_ROOTS[@]}"; do plan "$r  (allow-list: $(read_key "$r" allow-list | tr '\n' ' '))"; done
[ ${#CONFIG_ROOTS[@]} -gt 0 ] || plan "none under /etc/weaver"
for r in "${AGENT_ROOTS[@]}"; do plan "$r  (territory: $(read_key "$r" territory))"; done
for f in "${SUDO_RULES[@]}"; do
  case "${f##*/}" in
    *.updating) plan "$f  (sudo rule an install held and did not put back)" ;;
    *.decommissioning) plan "$f  (sudo rule an archive disabled)" ;;
    *) plan "$f  (sudo rule)" ;;
  esac
done

say "install prefixes (models excluded from every mode)"
for p in "${!PREFIXES[@]}"; do
  for sub in bin lib python-spu; do [ -d "$p/$sub" ] && plan "$p/$sub  $(du -sh "$p/$sub" 2>/dev/null | cut -f1)"; done
  for sub in "$p"/backup-* "$p"/lib.backup-*; do [ -d "$sub" ] && plan "$sub  $(du -sh "$sub" | cut -f1)"; done
  [ -d "$p/models" ] && plan "$p/models  $(du -sh "$p/models" | cut -f1)  KEPT"
done
mapfile -t LDSO_CONFS < <(grep -ls 'weaver' /etc/ld.so.conf.d/*.conf 2>/dev/null)
for c in "${LDSO_CONFS[@]}"; do plan "$c  ($(tr '\n' ' ' < "$c"))"; done

say "agents"
for a in "${!AGENTS[@]}"; do
  line="$a:"
  id "weaver-$a" >/dev/null 2>&1 && line="$line user=weaver-$a"
  id "weaver-$a-state" >/dev/null 2>&1 && line="$line member=weaver-$a-state"
  plan "$line"
done
plan "accounts: ${WEAVER_USERS[*]:-none}"
plan "groups:   ${WEAVER_GROUPS[*]:-none}"

say "units"
mapfile -t UNITS < <(systemctl list-units 'weaver-worker@*' --all --no-legend --plain 2>/dev/null | awk '{print $1}')
ACTIVE_UNITS=()
for u in "${UNITS[@]}"; do
  st=$(systemctl is-active "$u" 2>/dev/null || true)
  plan "$u  $st"
  [ "$st" = active ] && ACTIVE_UNITS+=("$u")
done
[ ${#UNITS[@]} -gt 0 ] || plan "no weaver-worker@ units"

say "runs"
# **Each root's agent asked whether it runs**, through the admin the box
# installed, by the root's own name: `show` answers the agent's state and the
# run's constituents. Running, in transition, or no answer at all refuses the
# archive and the purge below, never read as stopped.
# **run_verdict ANSWER: what one `show` says of a run**: stopped only where the
# state is absent or unloaded with no constituent, or the agent is no agent;
# running where any other state or a transition answers; unknown for a
# refusal or anything that does not parse.
run_verdict() {
  python3 -c '
import json, sys
try:
    d = json.loads(sys.argv[1])
except ValueError:
    print("unknown"); sys.exit()
if d.get("kind") == "state" and d.get("state") in ("absent", "unloaded") and not d.get("constituents"):
    print("stopped")
elif d.get("kind") in ("state", "in_transition"):
    print("running")
elif d.get("kind") == "no_such_agent":
    print("stopped")
else:
    print("unknown")
' "$1"
}
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

# **pick_admin CANDIDATE...: the first admin this script may run as root.** The
# candidates come from roots and a record another principal might have written
# (an old layout's or a drifted one), so each is resolved to its canonical path
# and held closed by admin's rule before it runs, never merely found executable
# (Codex on #79). Prints the chosen path, or nothing.
pick_admin() {
  local candidate real
  for candidate in "$@"; do
    real=$(realpath -e -- "$candidate" 2>/dev/null) || continue
    [ -f "$real" ] && [ -x "$real" ] || continue
    held_closed "$real" >/dev/null || { plan "$candidate is not held closed by root, so it is not run"; continue; }
    printf '%s' "$real"
    return 0
  done
}
ADMIN_CANDIDATES=()
v=$(read_key /etc/weaver/stack prefix); [ -n "$v" ] && ADMIN_CANDIDATES+=("$v/bin/weaver-admin")
for b in "${!BIN_DIRS[@]}"; do ADMIN_CANDIDATES+=("$b/weaver-admin"); done
ADMIN_BIN=$(pick_admin "${ADMIN_CANDIDATES[@]}" || true)
# **query_runs: asks every root's agent now**, filling RUNNING afresh. It runs
# at discovery for the plan, and again in the purge once the delegated door is
# shut, so no destructive step trusts an answer older than the last way a
# connector could have started a run (Codex on #79).
query_runs() {
  RUNNING=()
  local root agent base said verdict
  for root in "${AGENT_ROOTS[@]}"; do
    agent=${root##*/}
    base=$(dirname "$root")
    if [ -z "$ADMIN_BIN" ]; then
      plan "$agent  unknown: no weaver-admin found beside the roots' binaries"
      RUNNING+=("$agent (unknown)")
      continue
    fi
    said=$(WEAVER_ADMIN_CONFIG="$base" "$ADMIN_BIN" show "$agent" 2>/dev/null | tail -n 1 || true)
    verdict=$(run_verdict "$said")
    plan "$agent  $verdict  $said"
    [ "$verdict" = stopped ] || RUNNING+=("$agent ($verdict)")
  done
  # **A process under any agent's account is a run**, whatever its root says:
  # a run whose root was removed or lost is found by its accounts, every
  # constituent running as one of them (Codex on #79).
  local u
  for u in "${WEAVER_USERS[@]}"; do
    if pgrep -u "$u" >/dev/null 2>&1; then
      plan "$u  running: processes under the account"
      RUNNING+=("$u (processes)")
    fi
  done
}
query_runs
[ ${#AGENT_ROOTS[@]} -gt 0 ] || plan "no agent roots"
for c in "${!COORD_ROOTS[@]}"; do
  [ -d "$c/weaver.run" ] && plan "$c/weaver.run  (run directories)"
done
SLICE='system-weaver\x2dworker.slice'
systemctl is-active --quiet "$SLICE" 2>/dev/null && plan "$SLICE active ($(systemctl show "$SLICE" -p NCurrentlyActive 2>/dev/null || true))" || plan "$SLICE not active"

say "territories, record, log"
TERRITORY_PATHS=()
# The territories' bases: the per-agent layout's default and the stack record's
# own `agent-directory`, where create-agent.sh made every territory.
declare -A TERRITORY_BASES=([/var/lib/weaver-agent]=1)
v=$(read_key /etc/weaver/stack agent-directory); [ -n "$v" ] && TERRITORY_BASES["$v"]=1
# A base or a log directory under the operator's home is the layout retired
# on 2026-10-07: said, and never archived or purged.
for d in "${!TERRITORY_BASES[@]}"; do
  in_operator_home "$d" && { plan "$d  under the operator's home: not touched"; unset 'TERRITORY_BASES[$d]'; }
done
for d in "${!LOG_PATHS[@]}"; do
  in_operator_home "$d" && { plan "$d  under the operator's home: not touched"; unset 'LOG_PATHS[$d]'; }
done
for d in /var/lib/weaver "${!TERRITORY_BASES[@]}" "${!LOG_PATHS[@]}"; do [ -e "$d" ] && TERRITORY_PATHS+=("$d") && plan "$d  $(du -sh "$d" 2>/dev/null | cut -f1)"; done
# **territories_outside BASE... -- ROOT...: each root's own territory that no
# base covers**, read from the root's `territory` key (the operator's ruling of
# 2026-10-07 on #1), one per line. Under the new layout a territory is the
# declaration, the draft, the logs and the published save points as well as
# the trace and the state room, so one the bases miss, a custom territory or
# one made under an agent-directory the stack record no longer names, would
# be the agent unarchived at its takedown (Codex on #94, round 8). A root
# without the key, the pre-ruling layout, is archived by the bases as before;
# a territory under a base is archived with its base and not twice.
# Paths are compared canonical (Codex on #94, round 12): a territory written
# as `<base>/../elsewhere` is outside the base however it is spelled, and a
# base or a territory that does not resolve is left out of the comparison,
# an absent base covering nothing and an absent territory archiving nothing.
# **Every root's territory is judged, covered or not** (#99 area 2 review,
# H4 remainder): its canonical path is looked at as root, and one that is not
# uid 0's and grouped `weaver-<agent>-state` is named with what was found, on
# stderr, and the function answers 1 after listing the rest, which refuses
# the archive and the purge. One under the operator's home (OP_HOME) is said
# and skipped before it is judged, never touched.
territories_outside() {
  local bases=() b r v c covered found f_uid f_group f_mode refused=0 listed=()
  while [ $# -gt 0 ] && [ "$1" != -- ]; do
    c=$(realpath -e -- "$1" 2>/dev/null) && bases+=("$c")
    shift
  done
  [ "${1:-}" = -- ] && shift
  for r in "$@"; do
    v=$(cat "$r/territory" 2>/dev/null || true); v=${v%%$'\n'*}; [ -n "$v" ] && [ -d "$v" ] && [ ! -L "$v" ] || continue
    v=$(realpath -e -- "$v" 2>/dev/null) || continue
    # **A key is a territory only by its name** (#99 area 2 review, H4): what
    # this lists is archived and `rm -rf`'d, so a key naming anything but its
    # own agent's `weaver-<agent>` (a typo for `/var/lib`) is said and skipped.
    [ "${v##*/}" = "weaver-${r##*/}" ] || { printf 'skipped: %s names %s, not weaver-%s\n' "$r/territory" "$v" "${r##*/}" >&2; continue; }
    if [ -n "${OP_HOME:-}" ] && [ "$OP_HOME" != / ]; then
      case "$v" in "$OP_HOME"|"$OP_HOME"/*) printf "skipped: %s names %s, under the operator's home, which is never touched\n" "$r/territory" "$v" >&2; continue ;; esac
    fi
    found=$(stat -c '%u:%G:%a' -- "$v" 2>/dev/null) \
      || { printf 'refused: %s names %s, whose owner and group cannot be read\n' "$r/territory" "$v" >&2; refused=1; continue; }
    IFS=: read -r f_uid f_group f_mode <<< "$found"
    if [ "$f_uid" != 0 ] || [ "$f_group" != "weaver-${r##*/}-state" ]; then
      printf 'refused: %s names %s, which stands uid %s, group %s, mode %s, not root:weaver-%s-state\n' \
        "$r/territory" "$v" "$f_uid" "$f_group" "$f_mode" "${r##*/}" >&2
      refused=1; continue
    fi
    covered=0
    for b in "${bases[@]}"; do case "$v" in "$b"/*) covered=1;; esac; done
    [ "$covered" = 0 ] && listed+=("$v")
  done
  [ ${#listed[@]} -eq 0 ] || printf '%s\n' "${listed[@]}" | sort -u
  return "$refused"
}
TERRITORIES_JUDGED=1
OWN_LISTED=$(territories_outside "${!TERRITORY_BASES[@]}" -- "${AGENT_ROOTS[@]}") || TERRITORIES_JUDGED=0
OWN_TERRITORIES=()
[ -z "$OWN_LISTED" ] || mapfile -t OWN_TERRITORIES <<< "$OWN_LISTED"
[ "$TERRITORIES_JUDGED" = 1 ] || plan "REFUSED: a territory a root names is not root:weaver-<agent>-state (see above); the archive and the purge refuse"
for d in "${OWN_TERRITORIES[@]}"; do TERRITORY_PATHS+=("$d") && plan "$d  (a root's own territory, outside the bases) $(du -sh "$d" 2>/dev/null | cut -f1)"; done
HOMES=()
for u in "${WEAVER_USERS[@]}"; do
  h=$(getent passwd "$u" | cut -d: -f6)
  [ -d "$h" ] && case "$h" in /home/*) HOMES+=("$h"); plan "$h  (home of $u)";; esac
done
mapfile -t TMP_PATHS < <(find /tmp -maxdepth 1 \( -name 'weaver-*' -o -name 'torchinductor_weaver-*' \) 2>/dev/null)
[ ${#TMP_PATHS[@]} -gt 0 ] && plan "/tmp: ${#TMP_PATHS[@]} weaver-* entries"


[ "$MODE" = plan ] && { say "plan only. rerun with --archive DIR, then --purge DIR"; exit 0; }
[ "$TERRITORIES_JUDGED" = 1 ] || die "a territory a root names is not root:weaver-<agent>-state, named above, and only a territory standing as create-agent.sh lays it is archived and purged. Look at it by hand, then rerun"

# **free_name NAME: a name no archive in DEST holds yet**, NAME itself or the
# first NAME-2, NAME-3 that is free, so no archive is ever written over another
# while both sources stay on the purge list (Codex on #79).
free_name() {
  local name=$1 n=2
  while [ -e "$DEST/$name.tar.zst" ]; do name="$1-$n"; n=$((n + 1)); done
  printf '%s' "$name"
}

# --------------------------------------------------------------- 2. archive
# **Root reads, the operator writes.** The bulk store is an NFS export that
# squashes root to nobody, so root can neither create a directory the operator
# owns nor own what it writes there. Every read of a territory, the record or
# the store stays root's, and every byte that lands under $DEST goes through
# the operator's account: tar writes to stdout, and the operator's
# shell writes the file. Measured 2026-09-30: mkdir as root failed on the
# export, and a file root did write arrived owned by nobody.
as_op() { sudo -u "$OPERATOR" -H "$@"; }
to_file() { as_op sh -c 'cat > "$1"' _ "$1"; }   # stdin -> $1, owned by the operator
tarz() { # tarz OUT PATH... : preserve owners, modes, acls and xattrs
  local out=$1; shift
  tar --numeric-owner --acls --xattrs -I 'zstd -T0 -3' -cf - "$@" 2> >(grep -v 'Removing leading' >&2) | to_file "$out"
}
sha_all() { find "$1" -maxdepth 1 -type f -exec sha256sum {} \; 2>/dev/null || true; }

if [ "$MODE" = archive ]; then
  [ ${#ACTIVE_UNITS[@]} -eq 0 ] || die "units still active: ${ACTIVE_UNITS[*]}. Unload them (weaver-admin unload <agent>) or stop them, then rerun"
  [ ${#RUNNING[@]} -eq 0 ] || die "agents still run or cannot be read: ${RUNNING[*]}. Unload each (weaver-admin unload <agent>), then rerun"
  [ -e "$DEST/SHA256SUMS" ] && die "$DEST already holds an archive; name another directory"
  # **The delegated door shuts for the snapshot** (Codex on #79): each rule is
  # moved to a dot-name sudo never reads, so no connector can start a run
  # while tar copies, then every agent is asked again. A run found
  # puts the rules back and refuses. Otherwise they stay disabled, archived
  # under their disabled names, until the purge removes them. To serve again
  # without purging, move each `.weaver-<agent>.decommissioning` back to
  # `weaver-<agent>`.
  say "the delegated door, shut for the snapshot"
  MOVED=()
  HELD=()
  for f in "${SUDO_RULES[@]}"; do
    case "${f##*/}" in .*) HELD+=("$f"); continue ;; esac
    held="$(dirname "$f")/.${f##*/}.decommissioning"
    mv -T -- "$f" "$held" || die "cannot disable $f"
    MOVED+=("$held|$f"); HELD+=("$held")
    plan "disabled $f"
  done
  SUDO_RULES=("${HELD[@]}")
  say "runs, asked again"
  query_runs
  if [ ${#RUNNING[@]} -gt 0 ]; then
    for entry in "${MOVED[@]}"; do mv -T -- "${entry%%|*}" "${entry##*|}" && plan "restored ${entry##*|}"; done
    die "agents run or cannot be read: ${RUNNING[*]}. The sudo rules are restored; unload each, then rerun"
  fi
  as_op mkdir -p "$DEST" || die "the operator cannot create $DEST"
  as_op test -w "$DEST" || die "$DEST is not writable by $OPERATOR"
  # A run that died part way leaves files here the operator may not own;
  # they are this script's and go before it writes again.
  find "$DEST" -maxdepth 1 -type f -exec rm -f {} \; 2>/dev/null || true
  say "archive to $DEST"

  NVCC=$(command -v nvcc || ls /opt/cuda/bin/nvcc 2>/dev/null || true)
  # Box facts first, so a reader of the archive knows what produced it.
  {
    echo "host      $HOST"
    echo "operator  $OPERATOR"
    echo "date      $(date -Iseconds)"
    echo "kernel    $(uname -r)"
    driver=$(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null | head -1 || true)
    echo "driver    ${driver:-none}"
    echo "nvcc      $([ -n "$NVCC" ] && "$NVCC" --version | tail -1 || echo none)"
    echo "cccl      $(pacman -Q cccl 2>/dev/null || echo unknown)"
    echo "toolchain $(as_op rustup show active-toolchain 2>/dev/null | cut -d' ' -f1 || echo unknown)"
    echo
    echo "## accounts"; for u in "${WEAVER_USERS[@]}"; do getent passwd "$u" || true; done
    echo "## groups";   for g in "${WEAVER_GROUPS[@]}"; do getent group "$g" || true; done
    echo "## operator groups"; id "$OPERATOR"
    echo
    echo "## units"; systemctl list-units 'weaver-worker@*' --all --no-legend 2>/dev/null || true
    systemctl show "$SLICE" -p ActiveState,CPUUsageNSec,MemoryPeak 2>/dev/null || true
    echo
    echo "## installed binaries"; for p in "${!PREFIXES[@]}"; do sha_all "$p/bin"; done
    echo
    echo "## libraries"; for p in "${!PREFIXES[@]}"; do sha_all "$p/lib"; done
    echo
    echo "## ld.so.conf"; for c in "${LDSO_CONFS[@]}"; do echo "$c:"; cat "$c"; done
    echo
    echo "## models (kept in place, not copied)"
    for p in "${!PREFIXES[@]}"; do [ -d "$p/models" ] && { find "$p/models" -type f -size +1M -exec sha256sum {} \; 2>/dev/null || true; }; done
    echo
    echo "## modes of everything archived"
    for pth in "${TERRITORY_PATHS[@]}" "${HOMES[@]}" "${!PREFIXES[@]}" /etc/weaver; do
      [ -e "$pth" ] && { find "$pth" -maxdepth 2 -not -path '*/models/*' -printf '%M %u:%g %p\n' 2>/dev/null || true; }
    done
    true
  } | to_file "$DEST/box-facts.txt"
  plan "box-facts.txt"

  PURGE=()
  # **archive_name PREFIX PATH: a readable name from the whole path**, its
  # slashes made dashes, so two directories that share a last component
  # (`/srv/weaver-agent` and `/var/lib/weaver-agent`) are told apart at a
  # glance. Flattening is not injective (`/a/b-c` and `/a/b/c` meet), so the
  # name is only a proposal and `free_name` makes it unique (Codex on #79).
  archive_name() { local flat=${2#/}; printf '%s-%s' "$1" "${flat//\//-}"; }
  archive_path() { # archive_path NAME PATH...
    local name present=()
    name=$(free_name "$1"); shift
    for pth in "$@"; do [ -e "$pth" ] && present+=("$pth"); done
    [ ${#present[@]} -gt 0 ] || return 0
    tarz "$DEST/$name.tar.zst" "${present[@]}"
    # Apparent size, not blocks: on NFS the blocks are not yet accounted
    # when this line prints, and du answered 512 for a 3 G tarball.
    plan "$name.tar.zst  $(stat -c %s "$DEST/$name.tar.zst" | numfmt --to=iec)  <- ${present[*]}"
    PURGE+=("${present[@]}")
  }

  [ -d /etc/weaver ] && archive_path etc-weaver /etc/weaver
  [ ${#SUDO_RULES[@]} -gt 0 ] && archive_path sudoers-weaver "${SUDO_RULES[@]}"
  [ ${#LDSO_CONFS[@]} -gt 0 ] && archive_path ld-so-conf "${LDSO_CONFS[@]}"
  for p in "${!PREFIXES[@]}"; do
    parts=()
    for sub in bin lib python-spu; do [ -d "$p/$sub" ] && parts+=("$p/$sub"); done
    for sub in "$p"/backup-* "$p"/lib.backup-*; do [ -d "$sub" ] && parts+=("$sub"); done
    [ ${#parts[@]} -gt 0 ] && archive_path "$(archive_name opt "$p")" "${parts[@]}"
  done
  [ -e /var/lib/weaver ] && archive_path var-lib-weaver /var/lib/weaver
  # Every territory's trace and state room, under each base (Codex on #79).
  for d in "${!TERRITORY_BASES[@]}"; do
    [ -e "$d" ] && archive_path "$(archive_name territories "$d")" "$d"
  done
  # Each root's own territory the bases miss, by the path its root names.
  for d in "${OWN_TERRITORIES[@]}"; do archive_path "$(archive_name territory "$d")" "$d"; done
  for d in "${!LOG_PATHS[@]}"; do archive_path "$(archive_name log "$d")" "$d"; done
  [ ${#HOMES[@]} -gt 0 ] && archive_path home-weaver-users "${HOMES[@]}"
  [ ${#TMP_PATHS[@]} -gt 0 ] && archive_path tmp-weaver "${TMP_PATHS[@]}"

  # What --purge may touch, and nothing else. Accounts and groups are listed
  # by kind so the purge removes them by the right verb.
  {
    for pth in "${PURGE[@]}"; do echo "path $pth"; done
    # The run directories are tmpfs state, purged without an archive.
    for c in "${!COORD_ROOTS[@]}"; do
      [ -d "$c/weaver.run" ] && echo "path $c/weaver.run"
      for a in "${!AGENTS[@]}"; do [ -d "$c/weaver-$a" ] && echo "path $c/weaver-$a"; done
    done
    for u in "${WEAVER_USERS[@]}";  do echo "user $u"; done
    for g in "${WEAVER_GROUPS[@]}"; do echo "group $g"; done
    true
  } | to_file "$DEST/PURGE-LIST"

  as_op sh -c 'cd "$1" && find . -type f -not -name SHA256SUMS -printf "%P\n" | sort | xargs sha256sum > SHA256SUMS' _ "$DEST"
  say "verify"
  as_op sh -c 'cd "$1" && sha256sum -c --quiet SHA256SUMS' _ "$DEST" || die "the archive does not verify"
  for t in "$DEST"/*.tar.zst; do tar -tf "$t" >/dev/null || die "$t does not read back"; done
  plan "$(wc -l < "$DEST/SHA256SUMS") files sum and every tarball reads back"
  say "archived. review $DEST/PURGE-LIST, then: sudo $0 --purge $DEST"
  exit 0
fi

# ----------------------------------------------------------------- 3. purge
[ -f "$DEST/SHA256SUMS" ] && [ -f "$DEST/PURGE-LIST" ] || die "$DEST holds no finished archive"
say "re-verify $DEST"
( cd "$DEST" && sha256sum -c --quiet SHA256SUMS ) || die "the archive no longer verifies; purge refused"
[ ${#ACTIVE_UNITS[@]} -eq 0 ] || die "units still active: ${ACTIVE_UNITS[*]}"
[ ${#RUNNING[@]} -eq 0 ] || die "agents still run or cannot be read: ${RUNNING[*]}"
plan "verified"

say "the delegated door, shut"
# **No connector can start a run from here on**: the sudo rules, archived
# above, are removed before anything else, and every agent is then asked again.
# Only a root shell can still load one, which is the operator running this.
# A run found now refuses the purge with the rules already gone, and the
# archive holds them to put back.
for f in "${SUDO_RULES[@]}"; do rm -f -- "$f" && plan "removed $f"; done
say "runs, asked again"
query_runs
[ ${#RUNNING[@]} -eq 0 ] || die "agents run again or cannot be read: ${RUNNING[*]}. The sudo rules are removed and archived in $DEST (sudoers-weaver.tar.zst); unload each, then rerun the purge"

say "units"
for u in "${UNITS[@]}"; do systemctl stop "$u" 2>/dev/null || true; systemctl reset-failed "$u" 2>/dev/null || true; plan "stopped $u"; done
systemctl stop "$SLICE" 2>/dev/null && plan "stopped $SLICE" || true


say "accounts"
while read -r kind name; do
  case $kind in
    user)  userdel -r "$name" 2>/dev/null && plan "removed user $name (and home)" || { userdel "$name" 2>/dev/null && plan "removed user $name" || plan "user $name: not removed"; } ;;
  esac
done < "$DEST/PURGE-LIST"
while read -r kind name; do
  case $kind in
    group) getent group "$name" >/dev/null && { groupdel "$name" 2>/dev/null && plan "removed group $name" || plan "group $name: not removed"; } ;;
  esac
done < "$DEST/PURGE-LIST"

say "paths"
while read -r kind name; do
  case $kind in
    path)
      case "$name" in */models|*/models/*) plan "kept $name"; continue;; esac
      rm -rf -- "$name" && plan "removed $name" ;;
  esac
done < "$DEST/PURGE-LIST"
# A prefix left holding only models stays; one left empty goes.
for p in "${!PREFIXES[@]}"; do
  [ -d "$p" ] && [ -z "$(ls -A "$p")" ] && rmdir "$p" && plan "removed empty $p"
done
ldconfig && plan "ldconfig rerun: $(ldconfig -p | grep -cE 'ggml|llama') engine objects still in the cache"

say "what remains"
for p in "${!PREFIXES[@]}"; do [ -d "$p" ] && find "$p" -maxdepth 1 -mindepth 1 -printf '   %p\n'; done
getent passwd | awk -F: '$1 ~ /^weaver-/ {print "   account still present: "$1}'
systemctl list-units 'weaver-worker@*' --all --no-legend 2>/dev/null | sed 's/^/   unit still present: /' || true
say "purged. the archive is $DEST"
