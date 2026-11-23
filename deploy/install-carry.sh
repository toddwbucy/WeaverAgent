#!/usr/bin/env bash
# Install a carry set on a box that holds no stack: the stack a build box
# wrote with `bootstrap-stack.sh --carry`, checked in a copy root holds and
# placed by root, with no compiler and no CUDA toolkit on this box.
#
#   <set>/deploy/install-carry.sh <set> <manifest sha256>            plan: checks; installs nothing, no sudo
#   <set>/deploy/install-carry.sh <set> <manifest sha256> --install  then install under sudo
#
# **The manifest's sha256 is the set's identity** (weaver-admin-PRD section
# 7.2), and the operator names it as the build box printed it: a set whose
# manifest has another digest is another set and is refused. The plan checks
# the operator's copy of the set against it. The install copies the set into
# a directory root makes closed under the prefix and checks that copy again,
# so nothing the operator's account can still write is what root places.
# Root runs no code from the operator's set: it first checks, by sha256sum,
# that its copy of the manifest is the one the digest names and that its copy
# of deploy/carry_set.py is the file that manifest names, and then runs that
# copy (`python3 -I`, so nothing beside it is imported) for its `check` and
# its `tuple`. This script runs from the set itself, as the operator, so the
# operator trusts the set's deploy copy as far as the digest it names.
#
# **An install refuses unless every carried file equals its manifest line.**
# It then places the stack as root and reads the placed SPU with the stack's
# own library directory alone as its library path. It refuses unless the SPU
# resolves its engine and every runtime file there, with no library or symbol
# version missing, the driver's own library included, and unless each runtime
# file equals by sha256 the file the build box resolved
# (`build-runtime.<name>`). There is no loader entry and no ldconfig: admin
# hands the agent's processes the library path the stack record's
# `library-path` names, and create-agent.sh copies that key into each agent's
# root, so a root made by hand without it fails at exec. The deploy copy is
# judged against the programs this box carries (deploy/program_check.py), the
# names in WEAVER_PROGRAM_DIRS. On any refusal or signal after a file is
# placed, what was placed is removed, with each directory this run made.
#
# It then writes the stack record as bootstrap-stack.sh does, into a
# directory root makes beside it and renames to it last, so the record stands
# whole or not at all, with
# `library-path` the stack's library directory, and the install tuple whole:
# the build half as carried, each runtime file as it resolves here, and
# `carry = <manifest sha256>`, by which update-stack.sh knows a carried box
# and refuses it. Nothing it writes reads this box's name or address.
#
# **This is a first install only.** A box whose stack record stands takes a
# newer set after decommission.sh, which archives and removes every agent on
# the box and the stack, and then this script (deploy/REDEPLOY.md section
# 3a).
#
# Box facts are environment, defaulted and printed, as bootstrap-stack.sh
# names them:
#
#   WEAVER_PREFIX        /opt/weaver            bin/ lib/ deploy/ (and models/)
#   WEAVER_ADMIN_CONFIG  /etc/weaver/admin      the admin base: one root per agent
#   WEAVER_STACK_RECORD  /etc/weaver/stack      the scripts' record of this install
#   WEAVER_OPERATOR      $SUDO_USER or $USER    named in the plan; owns nothing this makes
#   WEAVER_AGENT_DIR     /var/lib/weaver-agent  the base of each agent's folder
#   WEAVER_PROGRAM_DIRS  /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
#                                               the directories whose programs the deploy
#                                               copy may call
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

HERE=$(dirname -- "$(realpath -e -- "${BASH_SOURCE[0]}")")
source "$(dirname -- "$(realpath -e -- "${BASH_SOURCE[0]}")")/retired-python.sh"

[ "$(id -u)" -ne 0 ] || die "run as the operator, not under sudo; the install step asks for sudo itself"

[ $# -ge 2 ] && [ $# -le 3 ] || die "usage: install-carry.sh <set> <manifest sha256> [--install]"
INSTALL=0
case "${3:-}" in
  "") ;;
  --install) INSTALL=1 ;;
  *) die "unknown argument $3" ;;
esac
SHA=$2
[[ "$SHA" =~ ^[0-9a-f]{64}$ ]] || die "the manifest sha256 '$SHA' is not 64 lowercase hex digits"
SET=$(realpath -e -- "$1" 2>/dev/null) && [ -d "$SET" ] || die "the carry set $1 is not a directory"
[ "$HERE" = "$SET/deploy" ] \
  || die "this script runs from $HERE, not from the deploy copy of the set it installs ($SET/deploy): run that set's own install-carry.sh"
command -v python3 >/dev/null || die "no python3 on PATH, which the set's check needs"
command -v ldd >/dev/null || die "no ldd on PATH, which the resolution check needs"

PREFIX=${WEAVER_PREFIX:-/opt/weaver}
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
OPERATOR=${WEAVER_OPERATOR:-${SUDO_USER:-$USER}}
AGENT_DIR=${WEAVER_AGENT_DIR:-/var/lib/weaver-agent}
PROGRAM_DIRS=${WEAVER_PROGRAM_DIRS:-/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}
[[ "$PREFIX" == /?* ]] || die "WEAVER_PREFIX $PREFIX is not an absolute path below /"

refuse_python_install "$PREFIX" "$PREFIX/bin"

# ---------------------------------------------------------------- 1. box facts
say "box"
plan "operator      $OPERATOR"
plan "carry set     $SET"
plan "manifest      $SHA"
plan "prefix        $PREFIX"
plan "admin base    $ADMIN_BASE"
plan "stack record  $STACK"
plan "agent dir     $AGENT_DIR"
plan "programs in   $PROGRAM_DIRS"

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

standing() { # standing PATH: 0 stands, 1 absent, refuses where it cannot tell
  local err
  if err=$(LC_ALL=C stat -- "$1" 2>&1 >/dev/null); then return 0; fi
  case "$err" in *"No such file or directory"*) return 1 ;; esac
  die "cannot tell whether $1 stands: $err"
}
# **A second set on a box that holds one** is refused: a carried update is not
# built yet, and two writers of one record is how a box drifts.
if standing "$STACK"; then
  die "$STACK already stands, so this box holds a stack. To put a newer carry set on it, take the box down with decommission.sh, which archives and removes every agent on the box and the stack, and then run this script (deploy/REDEPLOY.md section 3a)."
fi
if standing "$ADMIN_BASE"; then
  listing=$(LC_ALL=C ls -A -- "$ADMIN_BASE" 2>&1) || die "cannot list $ADMIN_BASE to see whether it holds configuration: $listing"
  [ -z "$listing" ] || die "$ADMIN_BASE already holds configuration. This is a first install; take the box down with decommission.sh first."
fi
for part in bin lib deploy; do
  if standing "$PREFIX/$part"; then die "$PREFIX/$part already stands. Decommission first."; fi
done
# Each place this script makes is judged as bootstrap-stack.sh judges it, the
# prefix among them, since its library directory joins every agent's library
# path.
for placed in "$PREFIX" "$ADMIN_BASE" "$STACK" "$AGENT_DIR"; do
  at=$placed
  while ! standing "$at"; do at=$(dirname -- "$at"); done
  if [ "$at" = "$placed" ]; then
    bad=$(held_closed "$at") || die "$placed is not held closed by root at $bad, so admin would refuse it"
  else
    bad=$(creatable_in "$at") || die "$placed would be made under $bad, where another principal could claim the missing name first"
  fi
done

# ---------------------------------------------------------------- 2. the set
say "set"
COMMIT=$(python3 "$HERE/carry_set.py" check "$SET" "$SHA") \
  || die "the carry set at $SET is not the set $SHA names (carry_set.py said why above)"
ROWS=$(python3 "$HERE/carry_set.py" rows "$SET" "$SHA") || die "the manifest's rows cannot be read"
plan "commit        $COMMIT"
plan "entries       $(wc -l <<<"$ROWS"), each equal to its manifest row"

say "plan"
plan "copy the set into a directory root makes closed under $PREFIX, and check it there"
plan "install -d $PREFIX/bin $PREFIX/lib $PREFIX/deploy  (root, 0755), and each entry from the checked copy"
plan "no loader entry and no ldconfig: library-path = $PREFIX/lib"
plan "judge the deploy copy against the programs in $PROGRAM_DIRS"
plan "write $STACK/{worker-binary,spu-binary,gate-binary,shell-binary,coordination-root,library-path,prefix,agent-directory,install-tuple}  (root, 0755 / 0644)"
plan "install -d $ADMIN_BASE and $AGENT_DIR (root:root 0755)"
[ -d "$PREFIX/models" ] && plan "$PREFIX/models stands: $(ls "$PREFIX/models" | wc -l) entries" || plan "$PREFIX/models is absent: copy the artifacts before declaring an agent"
[ "$INSTALL" -eq 1 ] || { say "plan only. rerun with --install"; exit 0; }

# ---------------------------------------------------------------- 3. install
say "install"
sudo -v || die "--install needs sudo"
# Every variable undo reads is set before any trap is, since undo runs
# under set -u.
STAGING=""
RECORD=""
PLACED=0
MADE=()
# **What a refused install placed is removed**, with the copy root checked,
# the record it was filling, and each directory this run made, deepest
# first and only if empty: none of the prefix's three directories and no
# stack record stood before (section 1). A further signal during the undo
# is ignored, so the undo always finishes.
undo() {
  local i
  trap '' INT TERM HUP
  if [ "$PLACED" -eq 1 ]; then
    sudo rm -rf -- "${PREFIX:?}/bin" "${PREFIX:?}/lib" "${PREFIX:?}/deploy" || true
  fi
  if [ -n "$STAGING" ]; then sudo rm -rf -- "${STAGING:?}" || true; fi
  if [ -n "$RECORD" ]; then sudo rm -rf -- "${RECORD:?}" || true; fi
  for ((i = ${#MADE[@]} - 1; i >= 0; i--)); do sudo rmdir -- "${MADE[i]}" 2>/dev/null || true; done
}
trap undo EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
# **make_dir DIR: DIR and each missing directory above it, root's, 0755**,
# each one this run made named for the undo. A directory that stands is left
# as it is, its mode included.
make_dir() {
  local d=$1 missing=()
  while ! standing "$d"; do missing=("$d" "${missing[@]}"); d=$(dirname -- "$d"); done
  [ ${#missing[@]} -gt 0 ] || return 0
  sudo install -d -o root -g root -m 0755 "${missing[@]}" || die "root could not make ${missing[-1]}"
  MADE+=("${missing[@]}")
}
make_dir "$PREFIX"
STAGING=$(sudo mktemp -d "$PREFIX/.carry-XXXXXX") || die "root could not make the directory to check the set in"
sudo install -d -o root -g root -m 0755 "$STAGING/bin" "$STAGING/lib" "$STAGING/deploy"
sudo install -o root -g root -m 0644 "$SET/MANIFEST" "$STAGING/MANIFEST"
while IFS=$'\t' read -r kind _ mode path; do
  if [ "$kind" = link ]; then
    sudo cp -a -- "$SET/$path" "$STAGING/$path"
  else
    sudo install -o root -g root -m "$mode" "$SET/$path" "$STAGING/$path"
  fi
done <<<"$ROWS"
# **Root's check runs root's copy of the checker**, once root has read, with
# this script's own code and the box's python3, that its copy of the
# manifest is the one SHA names and that its copy of carry_set.py is the file
# that manifest names.
sudo python3 -I -c 'import hashlib, sys
manifest, sha, checker = sys.argv[1:4]
digest = lambda p: hashlib.sha256(open(p, "rb").read()).hexdigest()
if digest(manifest) != sha:
    sys.exit(f"root'"'"'s copy of the manifest is not the one {sha} names")
rows = [l.split("\t") for l in open(manifest, encoding="ascii").read().splitlines()[2:]]
named = [r[1] for r in rows if len(r) == 4 and r[0] == "file" and r[3] == "deploy/carry_set.py"]
if len(named) != 1 or digest(checker) != named[0]:
    sys.exit("root'"'"'s copy of deploy/carry_set.py is not the file the manifest names")' \
  "$STAGING/MANIFEST" "$SHA" "$STAGING/deploy/carry_set.py" \
  || die "root's copy of the set is not the set $SHA names; nothing is placed"
CHECKER=$STAGING/deploy/carry_set.py
sudo python3 -I "$CHECKER" check "$STAGING" "$SHA" >/dev/null \
  || die "root's copy of the set is not the set $SHA names (carry_set.py said why above); nothing is placed"
plan "checked root's copy"
PLACED=1
sudo install -d -o root -g root -m 0755 "$PREFIX/bin" "$PREFIX/lib" "$PREFIX/deploy"
while IFS=$'\t' read -r kind _ mode path; do
  [ "$path" != build-tuple ] || continue
  if [ "$kind" = link ]; then
    sudo cp -a --remove-destination -- "$STAGING/$path" "$PREFIX/$path"
    sudo chown -h root:root -- "$PREFIX/$path"
  else
    sudo install -o root -g root -m "$mode" "$STAGING/$path" "$PREFIX/$path"
  fi
done <<<"$ROWS"
plan "placed the set's entries under $PREFIX"
# **The SPU as admin's start step runs it**: the library path alone, none of
# the operator's loader variables. Read whole, then judged.
rc=0
LDD_SAID=$(env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT -u GLIBC_TUNABLES \
  LD_LIBRARY_PATH="$PREFIX/lib" ldd "$PREFIX/bin/weaver-spu" 2>&1) || rc=$?
[ "$rc" -eq 0 ] || { printf '%s\n' "$LDD_SAID" >&2; die "ldd could not read the placed weaver-spu (it exited $rc); what was placed is removed"; }
TUPLE=$(sudo python3 -I "$CHECKER" tuple "$STAGING" "$PREFIX" "$LDD_SAID" "$SHA") \
  || die "the placed stack is not the set as carried (carry_set.py said why above); what was placed is removed"
plan "weaver-spu resolves its engine and runtime from $PREFIX/lib, each runtime file the build's"
# **The deploy copy calls only programs this box carries** (#610): the names
# in the program directories, listed by the shell alone.
box_programs() {
  local d f IFS=:
  for d in $PROGRAM_DIRS; do
    [ -d "$d" ] || continue
    for f in "$d"/*; do
      if [ -x "$f" ]; then printf '%s\n' "${f##*/}"; fi
    done
  done
}
python3 "$PREFIX/deploy/program_check.py" --programs <(box_programs) --copy "$PREFIX/deploy/deployed-copy.txt" \
  || die "the deploy copy calls a program this box does not carry (program_check.py named each above); what was placed is removed"
plan "the deploy copy calls only programs this box carries"

say "stack record"
make_dir "$(dirname -- "$STACK")"
RECORD=$(sudo mktemp -d "$(dirname -- "$STACK")/.stack-XXXXXX") || die "root could not make the directory to fill the stack record in"
w() { printf '%s\n' "$2" | sudo tee "$RECORD/$1" >/dev/null; sudo chmod 0644 "$RECORD/$1"; plan "$1 = $2"; }
w worker-binary "$PREFIX/bin/worker"
w spu-binary "$PREFIX/bin/weaver-spu"
w gate-binary "$PREFIX/bin/weaver-gate"
w shell-binary "$PREFIX/bin/busybox"
w coordination-root /run
w library-path "$PREFIX/lib"
w prefix "$PREFIX"
w agent-directory "$AGENT_DIR"

say "tuple"
printf '%s\n' "$TUPLE" | sed 's/^/   /'
printf '%s\n' "$TUPLE" | sudo tee "$RECORD/install-tuple" >/dev/null
sudo chmod 0644 "$RECORD/install-tuple"
make_dir "$ADMIN_BASE"
plan "admin base $ADMIN_BASE (empty)"
make_dir "$AGENT_DIR"
# **The record stands whole or not at all**: renamed into place last, never
# over a record that came to stand meanwhile (rename(2) replaces an empty
# directory, so no-clobber), the filled directory still standing meaning the
# rename did not happen.
# Signals are held from the rename until the run's state says the install
# stands, so no undo removes a stack the record already names. `mv -n` exits
# 0 whether or not it renamed, so the filled directory still standing is
# what says the rename did not happen.
sudo chmod 0755 "$RECORD"
trap '' INT TERM HUP
sudo mv -T -n -- "$RECORD" "$STACK" || true
[ ! -e "$RECORD" ] || die "the stack record could not be renamed to $STACK, which may have come to stand meanwhile; what was placed is removed"
RECORD=""
PLACED=0
MADE=()
undo
trap - EXIT INT TERM HUP
say "installed carry set ${SHA:0:12} at $COMMIT"
