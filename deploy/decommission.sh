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
# **The purge's order, root's record and its rerun are deploy/REDEPLOY.md
# section 2's** (#140).
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
# **The box's two connector accounts are not agents** (weaver-admin-Spec
# section 2, the operator's rulings of 2026-11-10): `weaver-gatecon` runs
# WeaverWeb's gate-con and `weaver-admincon` its admin-con, one of each per
# box, laid by create-agent.sh. They are listed apart, never read as agents,
# and removed after every agent's accounts.
is_box_account() {
  [ "$1" = weaver-gatecon ] || [ "$1" = weaver-admincon ]
}
box_account_role() {
  case "$1" in
    weaver-gatecon) printf "the box's gate-con account, which WeaverWeb's gate-con runs under" ;;
    weaver-admincon) printf "the box's admin-con account, which WeaverWeb's admin-con runs under" ;;
  esac
}
# **Every agent's accounts first and the box's accounts last**, so the purge,
# which removes them in this order, takes the box's accounts after every
# agent's.
# weaver_names passwd|group: the weaver- names, the box's last.
weaver_names() {
  local -a agents=() box=()
  local n
  while IFS= read -r n; do
    if is_box_account "$n"; then box+=("$n"); else agents+=("$n"); fi
  done < <(getent "$1" | awk -F: '$1 ~ /^weaver-/ {print $1}')
  [ ${#agents[@]} -eq 0 ] || printf '%s\n' "${agents[@]}"
  [ ${#box[@]} -eq 0 ] || printf '%s\n' "${box[@]}"
}
mapfile -t WEAVER_USERS < <(weaver_names passwd)
mapfile -t WEAVER_GROUPS < <(weaver_names group)
# Every account and group the agent's provisioning makes carries one of the
# reserved suffixes (weaver-admin-Spec section 4), stripped to the agent's name.
# The per-agent `-admincon` stays among them for an agent laid with one.
strip_suffix() {
  local n=${1#weaver-}
  for suffix in -admincon -state -relay -trace -admin; do
    [ "${n%"$suffix"}" != "$n" ] && { printf '%s' "${n%"$suffix"}"; return; }
  done
  printf '%s' "$n"
}
for u in "${WEAVER_USERS[@]}"; do is_box_account "$u" || AGENTS["$(strip_suffix "$u")"]=1; done
for g in "${WEAVER_GROUPS[@]}"; do is_box_account "$g" || AGENTS["$(strip_suffix "$g")"]=1; done
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
for u in weaver-gatecon weaver-admincon; do
  getent passwd "$u" >/dev/null 2>&1 && plan "$u: $(box_account_role "$u"), not an agent; the purge removes it after every agent's accounts"
done

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
  python3 -I -c '
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
  # constituent running as one of them (Codex on #79). **A process under a box
  # account is WeaverWeb's connector and not an agent's run**: it is said, and
  # the purge removes that account last, where it fails while the connector
  # still runs.
  local u
  for u in "${WEAVER_USERS[@]}"; do
    if pgrep -u "$u" >/dev/null 2>&1; then
      if is_box_account "$u"; then
        plan "$u  processes under $(box_account_role "$u"): not an agent; stop it before the purge removes the account"
        continue
      fi
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
# The directory where the trace sinks of a layout before the territories
# stood, taken whole only where it spans no base (spans_a_base, below).
LEGACY_DIR=/var/lib/weaver
# A base or a log directory under the operator's home is the layout retired
# on 2026-10-07: said, and never archived or purged.
for d in "${!TERRITORY_BASES[@]}"; do
  in_operator_home "$d" && { plan "$d  under the operator's home: not touched"; unset 'TERRITORY_BASES[$d]'; }
done
for d in "${!LOG_PATHS[@]}"; do
  in_operator_home "$d" && { plan "$d  under the operator's home: not touched"; unset 'LOG_PATHS[$d]'; }
done
# **agent_folders BASE...: each discovered agent's folder in each base**, one
# per line, canonical: `<base>/<agent>` (the operator's ruling of 2026-11-16 on
# the layout, #578) and `<base>/weaver-<agent>` (a territory laid before it),
# for each agent AGENTS names. Each is a directory, no link, and root's own by
# uid, so an account's home that shares an agent's name, under a base that is
# `/home`, is never one.
# **The purge takes these and never a base** (the Planner's ruling of
# 2026-11-16 on #578): a base is whatever the stack record names, the state
# drive's mount point on an appliance or `/home` on a host, and taking it whole
# would empty the drive or take every home on the box.
agent_folders() {
  local d a f
  for d in "$@"; do
    d=$(realpath -e -- "$d" 2>/dev/null) && [ -d "$d" ] || continue
    for a in "${!AGENTS[@]}"; do
      # **A folder of the ruled layout holds what makes it the agent's**: its
      # territory, or a home its account owns, so a root-held directory that
      # only shares a discovered name (a service account `weaver-web` beside
      # a `web/` of something else's) is never one.
      f="$d/$a"
      if [ -d "$f" ] && [ ! -L "$f" ] && [ "$(stat -c %u -- "$f" 2>/dev/null)" = 0 ] \
        && { [ -d "$f/weaver-$a" ] || [ "$(stat -c %U -- "$f/home" 2>/dev/null)" = "weaver-$a" ]; }; then
        printf '%s\n' "$f"
      fi
      # **A territory of the layout before #578 is the folder itself**, and
      # likewise holds what makes it the agent's: its declaration, or the
      # agent's state group on the directory.
      f="$d/weaver-$a"
      if [ -d "$f" ] && [ ! -L "$f" ] && [ "$(stat -c %u -- "$f" 2>/dev/null)" = 0 ] \
        && { [ -e "$f/agent.toml" ] || [ "$(stat -c %G -- "$f" 2>/dev/null)" = "weaver-$a-state" ]; }; then
        printf '%s\n' "$f"
      fi
    done
  done | sort -u
}
# **left_in_base BASE TAKEN...: each entry directly in BASE that is none of
# TAKEN**, one per line, which the plan names as left in place (the Planner's
# ruling of 2026-11-16 on #578), so the operator sees what the purge leaves.
# BASE and every TAKEN are compared canonical.
left_in_base() {
  local base e t taken=()
  base=$(realpath -e -- "$1" 2>/dev/null) || return 0
  shift
  for t in "$@"; do taken+=("$(realpath -e -- "$t" 2>/dev/null || printf '%s' "$t")"); done
  while IFS= read -r -d '' e; do
    for t in "${taken[@]}"; do [ "$t" = "$e" ] && continue 2; done
    printf '%s\n' "$e"
  done < <(find "$base" -mindepth 1 -maxdepth 1 -print0 2>/dev/null)
}
# **spans_a_base PATH: whether PATH is a base, or above a base or an agent's
# own folder** (CodeRabbit on #582), or the operator's home or above it, which
# is never touched, compared canonical: a path that does not resolve is
# compared as written. Such a path holds what is no agent's, a
# stranger's file in the base among it, so whatever fixed name it carries it
# is never archived as a path of its own or listed for the purge, in the
# archive and in purge_candidates both. The agents' folders inside it go on
# their own, and the plan names it as left.
spans_a_base() {
  local p c b
  p=$(realpath -e -- "$1" 2>/dev/null) || p=$1
  [ "$p" != / ] || return 0
  if [ -n "${OP_HOME:-}" ]; then
    case "$OP_HOME" in "$p" | "$p"/*) return 0 ;; esac
  fi
  for b in "${!TERRITORY_BASES[@]}"; do
    c=$(realpath -e -- "$b" 2>/dev/null) || continue
    case "$c" in "$p" | "$p"/*) return 0 ;; esac
  done
  for b in "${AGENT_FOLDERS[@]}" "${OWN_TERRITORIES[@]}"; do
    c=$(realpath -e -- "$b" 2>/dev/null) || continue
    case "$c" in "$p"/*) return 0 ;; esac
  done
  return 1
}
# **candidate PATH: the purge line for PATH, unless PATH spans a base**, so the
# archive's list and the purge's derivation drop the same paths.
candidate() {
  spans_a_base "$1" || printf 'path %s\n' "$1"
}
AGENT_FOLDERS=()
AGENT_LISTED=$(agent_folders "${!TERRITORY_BASES[@]}")
[ -z "$AGENT_LISTED" ] || mapfile -t AGENT_FOLDERS <<< "$AGENT_LISTED"
for d in "${AGENT_FOLDERS[@]}"; do [ -e "$d" ] && TERRITORY_PATHS+=("$d") && plan "$d  $(du -sh "$d" 2>/dev/null | cut -f1)"; done
# **home_projects FOLDER...: "<project> <seen> <mount point>" for each agent's
# home create-agent.sh capped** (#585). The project is the agent account's uid,
# read from passwd, the number create-agent.sh gives the home, and the mount is
# read off the root-owned folder. **A cap is create-agent.sh's only where the
# folder carries that project**, the mark it sets before the cap and which
# only root can change, so an agent made without --home-quota, or one refused
# because the drive carried its project already, never has a stranger's limit
# of the same number lifted. **Nothing is read off the home**: the agent owns
# it, so what it carries is no fact of root's. The gate's shell refuses every
# command the agent runs a change of its files' project (#587), but a home made
# before that, or moved by hand, can carry any number, another agent's uid
# included. What the home carries is printed as <seen>, a report and never
# acted on ("-" where it cannot be read). A folder whose account is
# gone, or whose drive has no project quotas, carries no cap. A folder whose
# mount or mark cannot be read is printed as "? mount <folder>" or "? mark
# <folder>", so the plan names it and what could not be read, rather than
# passing it over.
home_projects() {
  local f project marked seen _ options mount_point
  for f in "$@"; do
    [ -d "$f/home" ] && [ ! -L "$f/home" ] || continue
    project=$(getent passwd "weaver-${f##*/}" | cut -d: -f3) || continue
    [[ "$project" =~ ^[1-9][0-9]*$ ]] || continue
    { options=$(findmnt -n -o OPTIONS --target "$f" 2>/dev/null) \
        && mount_point=$(findmnt -n -o TARGET --target "$f" 2>/dev/null); } \
      || { printf '? mount %s\n' "$f"; continue; }
    [[ ",$options," == *,prjquota,* ]] || continue
    read -r marked _ < <(lsattr -pd -- "$f" 2>/dev/null) && [[ "$marked" =~ ^[0-9]+$ ]] \
      || { printf '? mark %s\n' "$f"; continue; }
    [ "$marked" = "$project" ] || continue
    read -r seen _ < <(lsattr -pd -- "$f/home" 2>/dev/null) || seen=-
    printf '%s %s %s\n' "$project" "${seen:--}" "$mount_point"
  done
}
# **plan_home_quotas "<project> <seen> <mount point>"...: one plan line each,
# and HOME_QUOTAS filled with "<project> <mount point>"**. A home carrying
# another project than its account's uid is named, and that project's limit
# is left as it stands. A folder whose mount or mark cannot be read is added
# to HOME_QUOTA_UNREAD, naming what could not be read, and the archive and
# the purge refuse over it.
plan_home_quotas() {
  local q project seen mount_point
  HOME_QUOTAS=() HOME_QUOTA_UNREAD=()
  for q in "$@"; do
    read -r project seen mount_point <<< "$q"
    if [ "$project" = "?" ]; then
      case $seen in
        mount) plan "home quota of $mount_point: its drive's mount cannot be read (findmnt), so whether a cap stands there is unknown, and the archive and the purge refuse until it reads"
               HOME_QUOTA_UNREAD+=("the mount of $mount_point (findmnt)") ;;
        *)     plan "home quota of $mount_point: the folder's mark cannot be read (lsattr), so whether a cap stands there is unknown, and the archive and the purge refuse until it reads"
               HOME_QUOTA_UNREAD+=("the mark of $mount_point (lsattr)") ;;
      esac
      continue
    fi
    HOME_QUOTAS+=("$project $mount_point")
    plan "home quota of project $project, the account's uid, on $mount_point: its limit is set to none at the purge, before the folder"
    if [[ "$seen" =~ ^[1-9][0-9]*$ ]] && [ "$seen" != "$project" ]; then
      plan "   the home carries project $seen, not $project: $seen's limit is not touched"
    fi
  done
}
# **remove_home_quotas "<project> <mount point>"...: each project's limit set
# to none**, one plan line each, returning 1 where any limit stands after it.
# Setting a project with no limit to none changes nothing, so an agent made
# without --home-quota on the same drive is no failure.
remove_home_quotas() {
  local q project mount_point failed=0
  for q in "$@"; do
    project=${q%% *} mount_point=${q#* }
    if ! command -v setquota >/dev/null; then
      plan "home quota of project $project on $mount_point: setquota is not on PATH, so its limit stands"; failed=1
    elif setquota -P "$project" 0 0 0 0 "$mount_point"; then
      plan "set the home quota of project $project on $mount_point to none"
    else
      plan "home quota of project $project on $mount_point: its limit was not removed"; failed=1
    fi
  done
  return "$failed"
}
# **project_limited PROJECT MOUNT: repquota's line for PROJECT on MOUNT where it
# shows any limit**, nothing where it shows none, and 1 where the quotas cannot
# be read. Usage alone is not a limit a removal could take, and the agent's own
# leftovers carry it (its folder's mark, a home a failed removal left), so it
# is not read. The fields are the ones create-agent.sh reads: the project, two
# statuses, the block counts used, soft and hard, a grace, the file counts
# used, soft and hard, and a grace.
project_limited() {
  local report line who b_soft b_hard f_soft f_hard count
  command -v repquota >/dev/null || return 1
  report=$(repquota -P -n -v -O csv "$2" 2>/dev/null) || return 1
  while IFS= read -r line; do
    IFS=, read -r who _ _ _ b_soft b_hard _ _ f_soft f_hard _ <<< "$line"
    [ "$who" = "#$1" ] || continue
    for count in "$b_soft" "$b_hard" "$f_soft" "$f_hard"; do
      [ "$count" = 0 ] || { printf '%s\n' "$line"; return 0; }
    done
  done <<< "$report"
}
HOME_PROJECTS=()
HOME_PROJECTS_LISTED=$(home_projects "${AGENT_FOLDERS[@]}")
[ -z "$HOME_PROJECTS_LISTED" ] || mapfile -t HOME_PROJECTS <<< "$HOME_PROJECTS_LISTED"
plan_home_quotas "${HOME_PROJECTS[@]}"
# **territories_outside FOLDER... -- ROOT...: each root's own territory that no
# agent's folder covers**, read from the root's `territory` key (the operator's ruling of
# 2026-10-07 on #1), one per line. Under the new layout a territory is the
# declaration, the draft, the logs and the published save points as well as
# the trace and the state room, so one the bases miss, a custom territory or
# one made under an agent-directory the stack record no longer names, would
# be the agent unarchived at its takedown (Codex on #94, round 8). A root
# without the key, the pre-ruling layout, is archived as its agent's folder
# `<base>/weaver-<agent>` where the discovery found that agent; a territory in
# an agent's folder, or that folder itself in the layout before #578, is
# archived with the folder and not twice.
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
    for b in "${bases[@]}"; do case "$v" in "$b"|"$b"/*) covered=1;; esac; done
    [ "$covered" = 0 ] && listed+=("$v")
  done
  [ ${#listed[@]} -eq 0 ] || printf '%s\n' "${listed[@]}" | sort -u
  return "$refused"
}
TERRITORIES_JUDGED=1
OWN_LISTED=$(territories_outside "${AGENT_FOLDERS[@]}" -- "${AGENT_ROOTS[@]}") || TERRITORIES_JUDGED=0
OWN_TERRITORIES=()
[ -z "$OWN_LISTED" ] || mapfile -t OWN_TERRITORIES <<< "$OWN_LISTED"
[ "$TERRITORIES_JUDGED" = 1 ] || plan "REFUSED: a territory a root names is not root:weaver-<agent>-state (see above); the archive and the purge refuse"
for d in "${OWN_TERRITORIES[@]}"; do TERRITORY_PATHS+=("$d") && plan "$d  (a root's own territory, outside every agent's folder) $(du -sh "$d" 2>/dev/null | cut -f1)"; done
# **The earlier layout's directory and each log directory are taken whole
# only where they span no base** (CodeRabbit on #582), judged once every
# agent's folder and every root's own territory is known.
for d in "$LEGACY_DIR" "${!LOG_PATHS[@]}"; do
  [ -e "$d" ] || continue
  if spans_a_base "$d"; then
    plan "$d  a base, or above a base or an agent's folder: not taken whole, left in place"
    continue
  fi
  TERRITORY_PATHS+=("$d")
  plan "$d  $(du -sh "$d" 2>/dev/null | cut -f1)"
done
# **home_within HOME FOLDER...: the folder HOME stands in**, canonical, or
# nothing. A home in an agent's folder (#578) goes with the folder.
home_within() {
  local h f c
  h=$(realpath -e -- "$1" 2>/dev/null) || return 0
  shift
  for f in "$@"; do
    c=$(realpath -e -- "$f" 2>/dev/null) || continue
    case "$h" in "$c"/*) printf '%s' "$f"; return 0 ;; esac
  done
}
# **A home is read from its account wherever it stands** (#578): one in an
# agent's folder goes with the folder, one under /home is taken as before, and
# one anywhere else is said and left in place.
HOMES=()
for u in "${WEAVER_USERS[@]}"; do
  h=$(getent passwd "$u" | cut -d: -f6)
  [ -n "$h" ] && [ -d "$h" ] || continue
  within=$(home_within "$h" "${AGENT_FOLDERS[@]}" "${OWN_TERRITORIES[@]}")
  if [ -n "$within" ]; then plan "$h  (home of $u, archived with $within)"
  else
    case "$h" in
      /home/*) HOMES+=("$h"); plan "$h  (home of $u)" ;;
      *) plan "$h  (home of $u): outside /home and every agent's folder, left in place, untouched" ;;
    esac
  fi
done
# **What a base holds beyond its agents' folders is named and left** (the
# Planner's ruling of 2026-11-16 on #578): the base stands after the purge.
for d in "${!TERRITORY_BASES[@]}"; do
  [ -d "$d" ] && [ ! -L "$d" ] || continue
  plan "$d  a base: the purge takes its agents' folders, and the base stands"
  while IFS= read -r e; do
    plan "$e  in the base, of no agent this discovery found: left in place, untouched"
  done < <(left_in_base "$d" "${TERRITORY_PATHS[@]}" "${HOMES[@]}")
done
# **tmp_paths DIR: the weaver entries directly under DIR that a weaver-
# account owns** (#98; the custody audit's G22): DIR is writable by every
# local user, so an entry another principal made under a weaver- name is
# neither read into the archive as root nor purged.
# **Read as two NUL-ended fields, and no name carrying a newline or a tab is
# taken**: a name in a world-writable directory is anyone's to choose, and
# one carrying a newline would split into an entry of the planter's choosing
# in this list and in the line-per-entry purge list, and a field separator
# a reader strips would make one entry pass for another.
tmp_paths() {
  find "$1" -mindepth 1 -maxdepth 1 \( -name 'weaver-*' -o -name 'torchinductor_weaver-*' \) -printf '%p\0%u\0' 2>/dev/null \
    | while IFS= read -r -d '' entry && IFS= read -r -d '' owner; do
        case $entry in *$'\n'* | *$'\t'*) continue ;; esac
        [ "${entry%/*}" = "$1" ] || continue
        case $owner in weaver-*) printf '%s\n' "$entry" ;; esac
      done
}
mapfile -t TMP_PATHS < <(tmp_paths /tmp)
[ ${#TMP_PATHS[@]} -gt 0 ] && plan "/tmp: ${#TMP_PATHS[@]} weaver-* entries"


[ "$MODE" = plan ] && { say "plan only. rerun with --archive DIR, then --purge DIR"; exit 0; }
[ "$TERRITORIES_JUDGED" = 1 ] || die "a territory a root names is not root:weaver-<agent>-state, named above, and only a territory standing as create-agent.sh lays it is archived and purged. Look at it by hand, then rerun"
# **A capped home's limit never outlives what names it** (Codex on #588): the
# folder's mark and the account's uid are all that tell create-agent.sh's
# limit from a stranger's, so where any folder's mount or mark cannot be read,
# nothing is archived or purged, first run or rerun. A purge that went on
# would take the folder and the account and leave the limit with nothing to
# name it, and an agent later given that uid would be refused. Root's record
# carries no unresolved limit: the operator reads or clears the mark by hand
# (deploy/HowToDeployANewAgent.md section 7) and reruns.
if [ ${#HOME_QUOTA_UNREAD[@]} -gt 0 ]; then
  unread=$(printf '%s; ' "${HOME_QUOTA_UNREAD[@]}")
  die "${unread%; } cannot be read, so whether that home carries create-agent.sh's cap is unknown, and nothing is archived or purged: a purge would take the folder and the account that name the limit and leave the limit standing. Read or clear the mark by hand by deploy/HowToDeployANewAgent.md section 7, then rerun"
fi

# **free_name NAME: a name no archive in DEST holds yet**, NAME itself or the
# first NAME-2, NAME-3 that is free, so no archive is ever written over another
# while both sources stay on the purge list (Codex on #79).
free_name() {
  local name=$1 n=2
  while op_has -e "$DEST/$name.tar.zst"; do name="$1-$n"; n=$((n + 1)); done
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
# **op_has FLAG PATH: whether the operator passes FLAG on PATH** (#577), as
# the shell's own `[` answers it as the operator, never the external `test`,
# which on the by-hand appliance walk of 2026-11-16 was uutils' and ignored
# supplementary groups. 0 is yes and 1 is no. **A drop that sudo itself
# refuses is neither**: it refuses here naming sudo, so a refused drop never
# reads as an archive absent or a directory unwritable.
op_has() {
  local said
  said=$(as_op sh -c 'if [ "$1" "$2" ]; then printf yes; else printf no; fi' sh "$1" "$2") \
    || die "sudo would not run a look as $OPERATOR at $2"
  case $said in
    yes) return 0 ;;
    no) return 1 ;;
    *) die "the look as $OPERATOR at $2 answered '$said', not yes or no" ;;
  esac
}
# stdin -> $1, owned by the operator and **readable by the operator alone**
# (#98; the custody audit's G22): the archive holds the trace and the save
# points, which the box keeps to their groups, so the invoker's umask does
# not decide who else reads them where they land.
#
# **A default access-control list on DEST outranks the umask** (#98, Codex on
# #142): a file created under one takes the list's entries and mask, the
# umask ignored, so each file is set 0600 once written, which masks any
# entry it inherited, and close_dest strips DEST's lists before anything is
# written.
to_file() { as_op sh -c 'umask 077 && cat > "$1" && chmod 0600 "$1"' _ "$1"; }
# **close_dest DIR: the archive directory made the operator's alone**: 0700,
# and its access and default access-control lists stripped, so nothing
# written in it inherits a reader. A store with no lists on DIR has none to
# strip; one carrying any, where they cannot be stripped, refuses.
#
# **A look that fails refuses** (Codex on #142, round 2): getfacl's status is
# read apart from its output, so an inspection that errs is never taken for
# a directory carrying no list.
close_dest() {
  local lists
  as_op chmod 0700 "$1" || die "the operator cannot close $1 to 0700"
  command -v getfacl >/dev/null && command -v setfacl >/dev/null \
    || die "getfacl and setfacl are needed to judge $1's access-control lists"
  lists=$(as_op getfacl -cps -- "$1") || die "$1's access-control lists cannot be read, so it cannot be vouched for"
  if [ -n "$lists" ]; then
    as_op setfacl -b -k -- "$1" || die "the operator cannot strip $1's access-control lists"
    lists=$(as_op getfacl -cps -- "$1") || die "$1's access-control lists cannot be read after the strip"
    [ -z "$lists" ] || die "$1 still carries access-control lists"
  fi
}
tarz() { # tarz OUT PATH... : preserve owners, modes, acls and xattrs
  local out=$1; shift
  tar --numeric-owner --acls --xattrs -I 'zstd -T0 -3' -cf - "$@" 2> >(grep -v 'Removing leading' >&2) | to_file "$out"
}
sha_all() { find "$1" -maxdepth 1 -type f -exec sha256sum {} \; 2>/dev/null || true; }

if [ "$MODE" = archive ]; then
  [ ${#ACTIVE_UNITS[@]} -eq 0 ] || die "units still active: ${ACTIVE_UNITS[*]}. Unload them (weaver-admin unload <agent>) or stop them, then rerun"
  [ ${#RUNNING[@]} -eq 0 ] || die "agents still run or cannot be read: ${RUNNING[*]}. Unload each (weaver-admin unload <agent>), then rerun"
  ! op_has -e "$DEST/SHA256SUMS" || die "$DEST already holds an archive; name another directory"
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
    for entry in "${MOVED[@]}"; do
      if mv -T -- "${entry%%|*}" "${entry##*|}"; then plan "restored ${entry##*|}"
      else plan "${entry##*|} could not be restored from ${entry%%|*}: move it back by hand"
      fi
    done
    die "agents run or cannot be read: ${RUNNING[*]}. The sudo rules are restored; unload each, then rerun"
  fi
  as_op sh -c 'umask 077 && mkdir -p "$1"' _ "$DEST" || die "the operator cannot create $DEST"
  op_has -w "$DEST" || die "$DEST is not writable by $OPERATOR"
  # A run that died part way leaves files here the operator may not own;
  # they are this script's and go before it writes again.
  # Removed as the operator (#98): DEST is the operator's, and root acting
  # under a path another writer controls could be led elsewhere.
  as_op find "$DEST" -maxdepth 1 -type f -exec rm -f {} \; 2>/dev/null || true
  close_dest "$DEST"
  say "archive to $DEST"

  NVCC=$(command -v nvcc || ls /opt/cuda/bin/nvcc 2>/dev/null || true)
  # Box facts first, so a reader of the archive knows what produced it.
  {
    echo "host      $HOST"
    echo "operator  $OPERATOR"
    echo "date      $(date -Iseconds)"
    echo "kernel    $(uname -r)"
    # The whole answer, then its first `<name>, <version>` line (#39): the
    # failure sentence on a box with no card, and a card in error's own line,
    # are no driver.
    driver=$(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null || true); driver=$(grep -E ', [0-9]+(\.[0-9]+)+$' <<<"$driver" || true); driver=${driver%%$'\n'*}
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
    name=$(free_name "$1") || exit 1; shift
    for pth in "$@"; do
      [ -e "$pth" ] || continue
      if spans_a_base "$pth"; then
        plan "$pth: a base, or above a base or an agent's folder, so it is not archived whole or listed for the purge"
        continue
      fi
      present+=("$pth")
    done
    [ ${#present[@]} -gt 0 ] || return 0
    tarz "$DEST/$name.tar.zst" "${present[@]}"
    # Apparent size, not blocks: on NFS the blocks are not yet accounted
    # when this line prints, and du answered 512 for a 3 G tarball.
    plan "$name.tar.zst  $(as_op stat -c %s "$DEST/$name.tar.zst" | numfmt --to=iec)  <- ${present[*]}"
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
  [ -e "$LEGACY_DIR" ] && archive_path var-lib-weaver "$LEGACY_DIR"
  # Every agent's folder, with its territory and its home, from each base
  # (Codex on #79, #578): never the base itself.
  for d in "${AGENT_FOLDERS[@]}"; do archive_path "$(archive_name agent "$d")" "$d"; done
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
      [ -d "$c/weaver.run" ] && candidate "$c/weaver.run"
      for a in "${!AGENTS[@]}"; do [ -d "$c/weaver-$a" ] && candidate "$c/weaver-$a"; done
    done
    for u in "${WEAVER_USERS[@]}";  do echo "user $u"; done
    for g in "${WEAVER_GROUPS[@]}"; do echo "group $g"; done
    # Each capped home's limit, so a purge removes only the limits this
    # archive kept, and a rerun from root's record finishes one that failed.
    for q in "${HOME_QUOTAS[@]}"; do echo "quota $q"; done
    true
  } | to_file "$DEST/PURGE-LIST"

  as_op sh -c 'umask 077 && cd "$1" && find . -type f -not -name SHA256SUMS -printf "%P\n" | sort | xargs sha256sum > SHA256SUMS && chmod 0600 SHA256SUMS' _ "$DEST"
  say "verify"
  as_op sh -c 'cd "$1" && sha256sum -c --quiet SHA256SUMS' _ "$DEST" || die "the archive does not verify"
  while IFS= read -r -d '' t; do
    as_op tar -tf "$t" >/dev/null || die "$t does not read back"
  done < <(as_op find "$DEST" -maxdepth 1 -type f -name '*.tar.zst' -print0)
  plan "$(as_op wc -l "$DEST/SHA256SUMS" | cut -d' ' -f1) files sum and every tarball reads back"
  say "archived. review $DEST/PURGE-LIST, then: sudo $0 --purge $DEST"
  exit 0
fi

# ----------------------------------------------------------------- 3. purge
# **purge_candidates: every line a purge list may hold**, derived from the box
# as the archive derives what it lists (#98; the custody audit's G22). The
# list lives in the archive directory, which the operator writes and, on an
# export where root's writes are squashed, root cannot hold as its own, so
# the purge does not take the list's word: it acts only on lines this
# derivation also finds.
purge_candidates() {
  local p sub d c a u g
  [ -d /etc/weaver ] && candidate /etc/weaver
  for p in "${SUDO_RULES[@]}" "${LDSO_CONFS[@]}"; do candidate "$p"; done
  for p in "${!PREFIXES[@]}"; do
    for sub in bin lib python-spu; do [ -d "$p/$sub" ] && candidate "$p/$sub"; done
    for sub in "$p"/backup-* "$p"/lib.backup-*; do [ -d "$sub" ] && candidate "$sub"; done
  done
  [ -e "$LEGACY_DIR" ] && candidate "$LEGACY_DIR"
  for d in "${AGENT_FOLDERS[@]}" "${OWN_TERRITORIES[@]}" "${!LOG_PATHS[@]}" "${HOMES[@]}" "${TMP_PATHS[@]}"; do
    candidate "$d"
  done
  for c in "${!COORD_ROOTS[@]}"; do
    candidate "$c/weaver.run"
    for a in "${!AGENTS[@]}"; do candidate "$c/weaver-$a"; done
  done
  for u in "${WEAVER_USERS[@]}"; do echo "user $u"; done
  for g in "${WEAVER_GROUPS[@]}"; do echo "group $g"; done
  for q in "${HOME_QUOTAS[@]}"; do echo "quota $q"; done
  true
}
# **purge_lines_outside LIST: each line of LIST naming something that still
# stands and that purge_candidates does not hold.** Judged in bash, each line
# read as the removal loops read it, so the string judged is the string acted
# on. A line naming what no longer stands is passed over, there being nothing
# to remove; any line of a kind the purge does not know is named. A rerun of a
# purge that began acts on root's record and never reaches this judgment.
#
# **A line passed over is never acted on**: the removal loops read only the
# lines purge_lines_within keeps, those the derivation holds. A line that did
# not stand when judged could stand by the removal (`/tmp/gone/../../etc`
# resolves once anyone makes `/tmp/gone`), so none is removed on the list's
# word alone.
purge_lines_outside() {
  local -A held=()
  local line kind name
  while IFS= read -r line; do
    [ -n "$line" ] && held["$line"]=1
  done < <(purge_candidates)
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    [ -n "${held["$line"]:-}" ] && continue
    kind=${line%% *} name=${line#* }
    case $kind in
      path) [ -e "$name" ] || [ -L "$name" ] || continue ;;
      user) getent passwd "$name" >/dev/null || continue ;;
      group) getent group "$name" >/dev/null || continue ;;
      # A limit the derivation does not hold is a home whose folder or account
      # is gone, or whose drive no longer carries project quotas: passed over
      # and never acted on.
      quota) continue ;;
    esac
    printf '%s\n' "$line"
  done < "$1"
}
# **purge_lines_within LIST: each line of LIST the derivation holds**, the
# only lines the removal loops act on.
purge_lines_within() {
  local -A held=()
  local line
  while IFS= read -r line; do
    [ -n "$line" ] && held["$line"]=1
  done < <(purge_candidates)
  while IFS= read -r line; do
    [ -n "$line" ] && [ -n "${held["$line"]:-}" ] && printf '%s\n' "$line"
  done < "$1"
  true
}

# **Root's record of the purge** (deploy/REDEPLOY.md section 2, #140): the
# lines purge_lines_within kept, judged while everything stood, and the prefixes
# the derivation found, kept where only root writes and the purge never reaches,
# keyed by the PURGE-LIST sum. A rerun against the same archive acts on it rather
# than deriving again from a box the purge has half removed. /var/lib is where a
# root tool keeps its state, and the name begins with no `weaver`, so no
# discovery glob of this script or another reaches it.
RECORD_PARENT=/var/lib
RECORD_DIR_NAME=decommission-weaver
# **purge_record OP KEY: read, write or remove the record named KEY**, beneath
# held directory descriptors and following no link: the parent held closed by
# root, the directory root's 0700, the file root's 0600, written whole under a
# temporary name, synced, renamed into place, and the directory synced. The
# record carries the sum of what it holds, so `read` answers its lines only
# where they are what was written: 3 where no record stands, 4 where it is
# not root's, not whole, or names another list.
read -r -d '' PURGE_RECORD_PY <<'PY' || true
import hashlib, os, stat, sys
parent, name, op, key = sys.argv[1:5]
head = b"weaver-purge-record 1\n"
def refuse(why):
    sys.stderr.write(why + "\n")
    sys.exit(4)
held = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
try:
    pfd = os.open(parent, held)
except OSError as e:
    refuse(f"{parent}: {e.strerror}")
st = os.fstat(pfd)
if st.st_uid != 0 or st.st_mode & 0o022:
    refuse(f"{parent} is not held closed by root")
try:
    dfd = os.open(name, held, dir_fd=pfd)
except FileNotFoundError:
    if op != "write":
        sys.exit(3)
    os.mkdir(name, 0o700, dir_fd=pfd)
    os.fsync(pfd)
    dfd = os.open(name, held, dir_fd=pfd)
    os.fchmod(dfd, 0o700)
except OSError as e:
    refuse(f"{parent}/{name}: {e.strerror}")
st = os.fstat(dfd)
if st.st_uid != 0 or stat.S_IMODE(st.st_mode) != 0o700:
    refuse(f"{parent}/{name} is not root's 0700")
if op == "write":
    body = sys.stdin.buffer.read()
    text = (head + b"purge-list " + key.encode() + b"\nacted "
            + hashlib.sha256(body).hexdigest().encode() + b"\n" + body)
    temporary = "." + key + ".writing"
    try:
        os.unlink(temporary, dir_fd=dfd)
    except FileNotFoundError:
        pass
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC,
                 0o600, dir_fd=dfd)
    os.fchmod(fd, 0o600)
    view = memoryview(text)
    while view:
        view = view[os.write(fd, view):]
    os.fsync(fd)
    os.close(fd)
    os.rename(temporary, key, src_dir_fd=dfd, dst_dir_fd=dfd)
    os.fsync(dfd)
elif op == "read":
    try:
        fd = os.open(key, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=dfd)
    except FileNotFoundError:
        sys.exit(3)
    except OSError as e:
        refuse(f"{parent}/{name}/{key}: {e.strerror}")
    st = os.fstat(fd)
    if not stat.S_ISREG(st.st_mode) or st.st_uid != 0 or stat.S_IMODE(st.st_mode) != 0o600:
        refuse(f"{parent}/{name}/{key} is not root's 0600 file")
    data = b""
    while chunk := os.read(fd, 65536):
        data += chunk
    lines = data.split(b"\n", 3)
    if len(lines) < 4 or lines[0] + b"\n" != head or lines[1] != b"purge-list " + key.encode():
        refuse(f"{parent}/{name}/{key} is not the record of this purge list")
    if lines[2] != b"acted " + hashlib.sha256(lines[3]).hexdigest().encode():
        refuse(f"{parent}/{name}/{key} does not hold what it summed when it was written")
    sys.stdout.buffer.write(lines[3])
elif op == "remove":
    try:
        os.unlink(key, dir_fd=dfd)
    except FileNotFoundError:
        pass
    os.fsync(dfd)
    try:
        os.rmdir(name, dir_fd=pfd)
        os.fsync(pfd)
    except OSError:
        pass
else:
    refuse(f"unknown record operation {op}")
PY
purge_record() { python3 -I -c "$PURGE_RECORD_PY" "$RECORD_PARENT" "$RECORD_DIR_NAME" "$@"; }
# **path_identity PATH: what a path is, beyond its name** (#140): its device,
# inode, owner uid and birth time (0 where the filesystem keeps none), read
# without following a final link. Device and inode alone are reused once a path
# is gone; a replacement made by another owner, or at another second, differs.
path_identity() { stat -c '%d:%i:%u:%W' -- "$1" 2>/dev/null; }
# **held_is PATH: whether PATH is still the object root's record held**, the
# identity read now equal to the one recorded when the purge judged it.
held_is() {
  local now
  now=$(path_identity "$1") || return 1
  [ -n "${HELD["$1"]:-}" ] && [ "${HELD["$1"]}" = "$now" ]
}
# **load_held: the identities root's record holds, by name** (#140): a line's
# name is everything after its identity, blanks and all. Paths and roots are
# held by their identity, a judged directory's direct entries by theirs, the
# accounts and groups by their numbers, and the legacy units by name.
load_held() {
  declare -gA HELD=() HELD_UID=() HELD_GID=() HELD_ENTRY=() HELD_UNIT=()
  local line rest
  while IFS= read -r line; do
    case $line in
      "held "*) rest=${line#held }; HELD["${rest#* }"]=${rest%% *} ;;
      "entry "*) rest=${line#entry }; HELD_ENTRY["${rest#* }"]=${rest%% *} ;;
      "uid "*) rest=${line#uid }; HELD_UID["${rest#* }"]=${rest%% *} ;;
      "gid "*) rest=${line#gid }; HELD_GID["${rest#* }"]=${rest%% *} ;;
      "unit "*) HELD_UNIT["${line#unit }"]=1 ;;
    esac
  done < "$PURGE_ACTED"
}
# **rerun_unchanged: whether everything a rerun would act on is what the first
# run judged** (Codex on #159, round 3, and its second reader): every unit,
# every root and every account still standing recorded and unchanged, and every
# listed path still standing unchanged with the same direct entries, none new
# and none replaced. Checked whole before a rerun's first act, so a rerun never
# removes, stops or changes what the first run did not judge. Names the first
# that is not, and answers 1.
# **A listed limit is still the agent's** (CodeRabbit on #582) where its
# project's number names the account this purge judged at that number, which
# still stands, or names no account and repquota shows the project with no
# limit. A limit that will not go stops the first run with its account
# standing (the home quotas step), so a limit whose account is gone was
# removed, and a limit shown on the number since is another's: a newcomer
# given the reused uid, or a project set by hand. That, a number naming an
# account this purge did not judge, or quotas that cannot be read refuse.
rerun_unchanged() {
  local u r line p entry now scan q project mount_point shown
  for u in "${UNITS[@]}"; do
    [ -n "${HELD_UNIT["$u"]:-}" ] || { printf 'the unit %s, which this purge did not judge\n' "$u"; return 1; }
  done
  for r in "${AGENT_ROOTS[@]}"; do
    held_is "$r" || { printf 'the root %s, which is not one this purge judged\n' "$r"; return 1; }
  done
  scan=$(mktemp)
  while IFS= read -r line; do
    case $line in
      "path "*)
        p=${line#path }
        [ -e "$p" ] || [ -L "$p" ] || continue
        case "$p" in */models|*/models/*) continue ;; esac
        held_is "$p" || { rm -f -- "$scan"; printf '%s, which is not the object this purge judged\n' "$p"; return 1; }
        [ -d "$p" ] && [ ! -L "$p" ] || continue
        find "$p" -mindepth 1 -maxdepth 1 -print0 > "$scan" || { rm -f -- "$scan"; printf '%s, whose entries cannot be read\n' "$p"; return 1; }
        while IFS= read -r -d '' entry; do
          now=$(path_identity "$entry" || true)
          [ -n "${HELD_ENTRY["$entry"]:-}" ] && [ "${HELD_ENTRY["$entry"]}" = "$now" ] \
            || { rm -f -- "$scan"; printf '%s in %s, which is not what this purge judged there\n' "$entry" "$p"; return 1; }
        done < "$scan" ;;
      "user "*)
        now=$(getent passwd "${line#user }" | cut -d: -f3 || true)
        [ -z "$now" ] || [ "$now" = "${HELD_UID["${line#user }"]:-}" ] \
          || { rm -f -- "$scan"; printf 'the user %s, which is not the account this purge judged\n' "${line#user }"; return 1; } ;;
      "group "*)
        now=$(getent group "${line#group }" | cut -d: -f3 || true)
        [ -z "$now" ] || [ "$now" = "${HELD_GID["${line#group }"]:-}" ] \
          || { rm -f -- "$scan"; printf 'the group %s, which is not the group this purge judged\n' "${line#group }"; return 1; } ;;
      "quota "*)
        q=${line#quota } && project=${q%% *} && mount_point=${q#* }
        now=$(getent passwd "$project" | cut -d: -f1 || true)
        if [ -n "$now" ]; then
          [ "${HELD_UID["$now"]:-}" = "$project" ] \
            || { rm -f -- "$scan"; printf 'the limit of project %s on %s, whose number now names the account %s, which this purge did not judge\n' "$project" "$mount_point" "$now"; return 1; }
        else
          shown=$(project_limited "$project" "$mount_point") \
            || { rm -f -- "$scan"; printf 'the limit of project %s on %s, whose quotas cannot be read (repquota)\n' "$project" "$mount_point"; return 1; }
          [ -z "$shown" ] \
            || { rm -f -- "$scan"; printf 'the limit of project %s on %s, which repquota shows limited (%s) while no account this purge judged holds the number, so it may be another'"'"'s (where it is the agent'"'"'s own, clear it by hand by deploy/HowToDeployANewAgent.md section 7)\n' "$project" "$mount_point" "$shown"; return 1; }
        fi ;;
    esac
  done < "$PURGE_ACTED"
  rm -f -- "$scan"
}
# **purge_began LIST: whether a purge has already acted on this box**: the
# list names /etc/weaver and it no longer stands. A purge withdraws the roots
# and removes /etc/weaver as its first removals, and nothing else removes it, so
# a box without it and with no record is one a purge completed, or left part
# way without its record, and the purge refuses rather than derive again from
# what it left. An account gone is not this sign: an agent taken down by hand
# between the archive and the purge leaves the rest of the list to purge.
purge_began() {
  grep -qxF 'path /etc/weaver' -- "$1" && [ ! -e /etc/weaver ]
}
# **record_lines_outside RECORD LIST: each line of RECORD's removals that LIST
# does not hold**, so a record and the archive it is read against agree line for
# line; a prefix line is the tail's alone and removes nothing.
record_lines_outside() {
  local -A listed=()
  local line
  while IFS= read -r line; do
    [ -n "$line" ] && listed["$line"]=1
  done < "$2"
  while IFS= read -r line; do
    case $line in "" | "prefix "* | "held "* | "entry "* | "uid "* | "gid "* | "unit "*) continue ;; esac
    [ -n "${listed["$line"]:-}" ] || printf '%s\n' "$line"
  done < "$1"
  true
}

# **Everything under DEST is read as the operator** (#98): the archive is the
# operator's alone, 0600 in a 0700 directory, and on an export where root's
# writes are squashed root could not read it either way.
op_has -f "$DEST/SHA256SUMS" && op_has -f "$DEST/PURGE-LIST" || die "$DEST holds no finished archive"
say "re-verify $DEST"
as_op sh -c 'cd "$1" && sha256sum -c --quiet SHA256SUMS' _ "$DEST" || die "the archive no longer verifies; purge refused"
# The list is read once, into root's own copy, and judged there, so what is
# judged is what the purge reads: nothing writes it between.
PURGE_LIST=$(mktemp)
PURGE_ACTED=$(mktemp)
trap 'rm -f -- "$PURGE_LIST" "$PURGE_ACTED"' EXIT
as_op cat -- "$DEST/PURGE-LIST" > "$PURGE_LIST"
# The copy agrees with the archive's sums, or nothing is purged. This
# catches a list changed by accident since the archive, not a writer who can
# rewrite the sums as well: against that writer the defence is the
# derivation below, on which every removal rests.
LISTED_SUM=$(as_op awk '$2 == "PURGE-LIST" { print $1 }' "$DEST/SHA256SUMS")
COPY_SUM=$(sha256sum < "$PURGE_LIST" | cut -d' ' -f1)
[ -n "$LISTED_SUM" ] && [ "$LISTED_SUM" = "$COPY_SUM" ] || die "the purge list read is not the one the archive summed; purge refused"
[[ "$COPY_SUM" =~ ^[0-9a-f]{64}$ ]] || die "the purge list's sum reads as $COPY_SUM; purge refused"
RERUN=0
if purge_record read "$COPY_SUM" > "$PURGE_ACTED"; then
  RERUN=1
else
  case $? in
    3) ;;
    *) die "root's record of this purge, $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM, does not read back whole (above); nothing is purged. Look at it by hand" ;;
  esac
fi
if [ "$RERUN" = 1 ]; then
  # **A rerun acts on root's record** (#140): its lines were judged against
  # the derivation while the box stood whole, and root wrote them, so they are
  # not judged again against a box the purge has half removed.
  STRAY=$(record_lines_outside "$PURGE_ACTED" "$PURGE_LIST")
  [ -z "$STRAY" ] || die "root's record names what this archive's PURGE-LIST does not, so nothing is purged: ${STRAY//$'\n'/; }"
  plan "a rerun: acting on root's record of the purge begun against this archive"
else
  purge_began "$PURGE_LIST" && die "/etc/weaver, which this archive's PURGE-LIST names, is gone, and root holds no record of a purge in progress at $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM: a purge has completed here, or one stopped without its record, and what it left is not derived again. Finish by hand from $DEST/PURGE-LIST"
  OUTSIDE=$(purge_lines_outside "$PURGE_LIST")
  [ -z "$OUTSIDE" ] || die "PURGE-LIST names what this purge would not find, so nothing is purged: ${OUTSIDE//$'\n'/; }"
  purge_lines_within "$PURGE_LIST" > "$PURGE_ACTED"
  for p in "${!PREFIXES[@]}"; do printf 'prefix %s\n' "$p"; done >> "$PURGE_ACTED"
fi
[ ${#ACTIVE_UNITS[@]} -eq 0 ] || die "units still active: ${ACTIVE_UNITS[*]}"
[ ${#RUNNING[@]} -eq 0 ] || die "agents still run or cannot be read: ${RUNNING[*]}"
plan "verified"
load_held
if [ "$RERUN" = 1 ]; then
  CHANGED=$(rerun_unchanged) || die "this rerun would act on $CHANGED, so nothing is purged: a rerun never removes what the first run did not judge. Look at it, then rerun the purge, or abandon this purge's record (deploy/REDEPLOY.md section 2)"
fi

say "the delegated door, shut"
# **No connector can start a run from here on**: the sudo rules, archived
# above, are removed before anything else, and every agent is then asked again.
# Only a root shell can still load one, which is the operator running this.
# A run found now refuses the purge with the rules already gone, and the
# archive holds them to put back.
# A rule that cannot be removed leaves the door open, so it refuses: a failure
# on the left of `&&` is exempt from `set -e` (Codex on #159, round 2).
# **Only a rule this purge judged goes** (Codex on #159, round 3): a first run
# removes a rule its PURGE-LIST names, and a rerun none, every rule judged having
# gone before the record was written. A rule restored, replaced or made for a new
# agent since is never removed unarchived: it refuses.
# Every rule is judged before any goes, so a refusal leaves the door as it stood.
for f in "${SUDO_RULES[@]}"; do
  [ "$RERUN" = 0 ] || die "the sudo rule $f stands, and every rule this purge judged went before its record was written: it was put back or made since, so nothing is purged. Look at it, then rerun the purge"
  grep -qxF "path $f" -- "$PURGE_ACTED" || die "the sudo rule $f is not on this archive's PURGE-LIST, so it is not removed and nothing is purged: archive again to take it"
done
for f in "${SUDO_RULES[@]}"; do
  rm -f -- "$f" || die "the sudo rule $f cannot be removed, so the delegated door is not shut and nothing is purged"
  plan "removed $f"
done
say "runs, asked again"
query_runs
[ ${#RUNNING[@]} -eq 0 ] || die "agents run again or cannot be read: ${RUNNING[*]}. The sudo rules are removed and archived in $DEST (sudoers-weaver.tar.zst); unload each, then rerun the purge"
# **The record is written here, before the first removal of an agent's own**:
# a stop before this line leaves a box whose next purge is a first run again,
# and a run found just above leaves no record to be reused when the box serves
# again.
if [ "$RERUN" = 0 ]; then
  # **What each removal acts on, as judged** (#140; Codex on #159, rounds 1
  # and 3): every listed path's and every root's identity, the direct entries
  # of every listed directory, every prefix's, every listed account's number
  # and every legacy unit's name, read once the door is shut and no run stands,
  # so a rerun acts on nothing the first run did not judge.
  {
    while IFS= read -r line; do
      case $line in
        "path "*)
          p=${line#path }
          held=$(path_identity "$p") || continue
          printf 'held %s %s\n' "$held" "$p"
          case "$p" in */models|*/models/*) continue ;; esac
          [ -d "$p" ] && [ ! -L "$p" ] || continue
          while IFS= read -r -d '' entry; do
            case $entry in *$'\n'*) continue ;; esac
            held=$(path_identity "$entry") && printf 'entry %s %s\n' "$held" "$entry"
          done < <(find "$p" -mindepth 1 -maxdepth 1 -print0) ;;
        "user "*) held=$(getent passwd "${line#user }" | cut -d: -f3) && [ -n "$held" ] && printf 'uid %s %s\n' "$held" "${line#user }" ;;
        "group "*) held=$(getent group "${line#group }" | cut -d: -f3) && [ -n "$held" ] && printf 'gid %s %s\n' "$held" "${line#group }" ;;
        "prefix "*) held=$(path_identity "${line#prefix }") && printf 'held %s %s\n' "$held" "${line#prefix }" ;;
      esac
    done < <(grep -E '^(path|user|group|prefix) ' -- "$PURGE_ACTED")
    for root in "${AGENT_ROOTS[@]}"; do
      held=$(path_identity "$root") && printf 'held %s %s\n' "$held" "$root"
    done
    for u in "${UNITS[@]}"; do printf 'unit %s\n' "$u"; done
    true
  } >> "$PURGE_ACTED"
  load_held
  purge_record write "$COPY_SUM" < "$PURGE_ACTED" || die "root's record of this purge cannot be written at $RECORD_PARENT/$RECORD_DIR_NAME; nothing more is purged"
  plan "root's record written: $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM"
fi
PURGE_FAILED=0

say "units"
# The legacy units (a box from before #50) this purge recorded, and no other.
for u in "${!HELD_UNIT[@]}"; do systemctl stop "$u" 2>/dev/null || true; systemctl reset-failed "$u" 2>/dev/null || true; plan "stopped $u"; done
if [ ${#HELD_UNIT[@]} -gt 0 ]; then systemctl stop "$SLICE" 2>/dev/null && plan "stopped $SLICE" || true; fi


say "the config roots"
# **The roots go first** (deploy/REDEPLOY.md section 2, #140): each is withdrawn
# whole by one rename to a name no discovery takes, so a stop leaves a root
# standing whole or not at all, never one half removed for its admin to refuse,
# and with the roots gone a rerun asks no admin: its run check is the process
# check under the weaver accounts, which go last.
# **Each is the object judged, or nothing goes** (Codex on #159, round 1):
# /etc/weaver and every root are checked against the record before a root is
# withdrawn, so a /etc/weaver put back or made again after a stop is never
# removed as the one judged.
if grep -qxF 'path /etc/weaver' -- "$PURGE_ACTED" && { [ -e /etc/weaver ] || [ -L /etc/weaver ]; }; then
  held_is /etc/weaver || die "/etc/weaver is not the object this purge judged (held ${HELD[/etc/weaver]:-none}, now $(path_identity /etc/weaver || echo unreadable)), so nothing more is removed. Look at it, then rerun the purge"
  for base in /etc/weaver/admin*; do
    [ -d "$base" ] && [ ! -L "$base" ] || continue
    for root in "$base"/*/; do
      root=${root%/}
      # The roots discovery takes, and no other: the rest go with /etc/weaver.
      [ -d "$root" ] && [ ! -L "$root" ] && [[ "${root##*/}" =~ ^[A-Za-z0-9_-]+$ ]] || continue
      held_is "$root" || die "$root is not a root this purge judged, so nothing more is removed. Look at it, then rerun the purge"
      mv -T -- "$root" "$base/.${root##*/}.purging" || die "$root cannot be withdrawn, so nothing more is removed: a root left standing with its territory gone would refuse every rerun. Look at it, then rerun the purge"
      plan "withdrew $root"
    done
  done
  if rm -rf -- /etc/weaver; then plan "removed /etc/weaver"; else plan "/etc/weaver: not removed"; PURGE_FAILED=1; fi
fi

say "home quotas"
# **Each capped home's limit goes before its folder and its account** (#585),
# so no stale limit stands in the drive's quota file for a later account of
# the same uid. Only the limits this archive's PURGE-LIST names and the
# derivation holds are removed, as with every other line, and they are on
# root's record. **A limit that will not go stops the purge here**
# (CodeRabbit on #582), its folder and its account standing and the record
# kept, so the rerun finds the agent's own account at the project's number
# and removes the limit first. A rerun that finds a listed limit's account
# gone therefore knows the first run removed it, and refuses a limit shown
# on the number since (rerun_unchanged).
PURGE_QUOTAS=()
while IFS= read -r line; do
  case $line in "quota "*) PURGE_QUOTAS+=("${line#quota }") ;; esac
done < "$PURGE_ACTED"
remove_home_quotas "${PURGE_QUOTAS[@]}" \
  || die "a home quota's limit above was not removed, so nothing more is removed, no path and no account, and root's record is kept at $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM. Make setquota answer, then rerun the purge, which removes the limit before them"

say "paths"
while IFS= read -r line; do
  kind=${line%% *} name=${line#* }
  case $kind in
    path)
      case "$name" in */models|*/models/*) plan "kept $name"; continue;; esac
      if [ ! -e "$name" ] && [ ! -L "$name" ]; then plan "$name: already gone"; continue; fi
      held_is "$name" || die "$name is not the object this purge judged (held ${HELD["$name"]:-none}, now $(path_identity "$name" || echo unreadable)), so it is not removed and nothing more is. Look at it, then rerun the purge"
      if rm -rf -- "$name"; then plan "removed $name"
      else plan "$name: not removed"; PURGE_FAILED=1
      fi ;;
  esac
done < "$PURGE_ACTED"

say "accounts"
# Each line split as it was judged, whole: `read` with its default field
# separators would strip a name's surrounding blanks and act on another path.
while IFS= read -r line; do
  kind=${line%% *} name=${line#* }
  case $kind in
    user)
      now=$(getent passwd "$name" | cut -d: -f3 || true)
      if [ -z "$now" ]; then plan "user $name: already gone"
      elif [ "$now" != "${HELD_UID["$name"]:-}" ]; then
        die "user $name is not the account this purge judged (uid ${HELD_UID["$name"]:-none}, now $now), so it is not removed and nothing more is. Look at it, then rerun the purge"
      # Without -r (Codex on #159, round 3, second reader): a home this purge
      # judged went as a path in step 2, and -r would remove one it did not,
      # a home outside /home or one made again, and the mail spool.
      elif is_box_account "$name"; then
        # **The box's accounts go last, said as what they are**: userdel
        # refuses an account a process still runs under, so a connector left
        # running keeps its account, named.
        if userdel "$name" 2>/dev/null; then plan "removed user $name, $(box_account_role "$name")"
        else plan "user $name: not removed: $(box_account_role "$name"), which may still run; stop it, then rerun the purge"; PURGE_FAILED=1
        fi
      elif userdel "$name" 2>/dev/null; then plan "removed user $name"
      else plan "user $name: not removed"; PURGE_FAILED=1
      fi ;;
  esac
done < "$PURGE_ACTED"
while IFS= read -r line; do
  kind=${line%% *} name=${line#* }
  case $kind in
    # **A group userdel already removed is done** (#140): userdel removes a
    # user's private group with the user (USERGROUPS_ENAB), so the box's
    # weaver-gatecon and weaver-admincon, the list's last groups, and an older
    # agent's weaver-<agent>-admincon are gone by now. Each is
    # said, never taken for a failure, and every arm ends in a status this
    # loop's last line can stand on, so no caller under `set -e` stops here.
    group)
      now=$(getent group "$name" | cut -d: -f3 || true)
      if [ -z "$now" ]; then plan "group $name: already gone with its user"
      elif [ "$now" != "${HELD_GID["$name"]:-}" ]; then
        die "group $name is not the group this purge judged (gid ${HELD_GID["$name"]:-none}, now $now), so it is not removed and nothing more is. Look at it, then rerun the purge"
      elif groupdel "$name" 2>/dev/null; then plan "removed group $name"
      else plan "group $name: not removed"; PURGE_FAILED=1
      fi ;;
  esac
done < "$PURGE_ACTED"

# The prefixes the record holds: a rerun derives none once /etc/weaver is gone.
mapfile -t PURGED_PREFIXES < <(sed -n 's/^prefix //p' "$PURGE_ACTED")
# A prefix left holding only models stays; one left empty goes, and one that
# will not go is a removal that failed (Codex on #159, round 2): the record is
# kept and the purge refuses below, naming it.
for p in "${PURGED_PREFIXES[@]}"; do
  [ -d "$p" ] && [ -z "$(ls -A "$p")" ] || continue
  held_is "$p" || { plan "kept $p: not the directory this purge judged"; continue; }
  if rmdir -- "$p"; then plan "removed empty $p"
  else plan "empty $p: not removed"; PURGE_FAILED=1
  fi
done
if ldconfig; then plan "ldconfig rerun: $(ldconfig -p | grep -cE 'ggml|llama') engine objects still in the cache"
else plan "ldconfig failed: its cache may still name the engine libraries removed; run ldconfig by hand"
fi

say "what remains"
for p in "${PURGED_PREFIXES[@]}"; do [ -d "$p" ] && find "$p" -maxdepth 1 -mindepth 1 -printf '   %p\n'; done
getent passwd | awk -F: '$1 ~ /^weaver-/ {print "   account still present: "$1}'
systemctl list-units 'weaver-worker@*' --all --no-legend 2>/dev/null | sed 's/^/   unit still present: /' || true
# **The record goes last**, and only once every removal it names was made: a
# removal that failed keeps it, so the rerun finds the work left.
if [ "$PURGE_FAILED" = 1 ]; then
  die "a removal above failed (\"not removed\"); root's record is kept at $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM. Look at what remains, then rerun the purge"
fi
# A record that will not go stands, and a rerun would act on it again: the
# purge is not reported done (Codex on #159, round 2). No record there (3) is
# none to remove.
purge_record remove "$COPY_SUM" || case $? in
  3) ;;
  *) die "every removal was made, but root's record $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM cannot be removed and stands, so the purge is not done. Look at what remains above, then remove it by hand: sudo rm $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM" ;;
esac
plan "removed root's record $RECORD_PARENT/$RECORD_DIR_NAME/$COPY_SUM"
say "purged. the archive is $DEST"
