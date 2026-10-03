#!/usr/bin/env bash
# Install the agent stack on a box that holds none: build at the lock, test,
# install the members and their libraries, and write the stack record.
#
#   deploy/bootstrap-stack.sh              plan: preflight, test, build; installs nothing, no sudo
#   deploy/bootstrap-stack.sh --install    then install under sudo
#
# **This is the first install; `update-stack.sh` is every later one.** That
# script diffs the built members against the installed ones and reads every
# path out of the stack record, so on a box with neither it has nothing to
# read and refuses. This one writes both from the facts below and then hands
# over: after it, `create-agent.sh` makes each agent and `update-stack.sh`
# carries the box forward. It refuses to run where a stack record already
# stands, because two writers of one record is how a box drifts.
#
# **The stack record is the scripts' and admin never reads it.** Admin reads
# one agent's root, `<admin base>/<agent>/`, and nothing shared. The record at
# `/etc/weaver/stack/` holds the box-wide defaults, one file per key, which
# `create-agent.sh` copies into each new agent's root: `worker-binary`,
# `spu-binary`, `gate-binary`, `run-tool`, `control-tool`, `coordination-root`,
# `unit-properties`, and for the scripts alone `prefix`, `log-directory` and
# `agent-directory`. `headroom-bytes` and `state-store-socket` are optional;
# this script writes neither, and an operator who writes one into the record
# has it copied into every agent made after. The admin base is created empty
# here, root-owned 0755, and gains one root per agent.
#
# Box facts are environment, defaulted, printed, and never discovered from a
# directory listing (the install set is named, per update-stack.sh):
#
#   WEAVER_PREFIX        /opt/weaver            bin/ lib/ models/ python-spu/
#   WEAVER_ADMIN_CONFIG  /etc/weaver/admin      the admin base: one root per agent
#   WEAVER_STACK_RECORD  /etc/weaver/stack      the scripts' record of this install
#   WEAVER_OPERATOR      $SUDO_USER or $USER    named in the plan; owns nothing this makes
#   WEAVER_AGENT_DIR     /var/lib/weaver-agent
#   WEAVER_LOG_DIR       /var/log/weaver
#   CUDA_LIB_DIR         /opt/cuda/lib64        joins LD_LIBRARY_PATH in unit-properties
#   CARGO_TARGET_DIR     honoured; give the install its own, never a gate's
#
# The feature set and the member list are update-stack.sh's, stated again
# here rather than sourced, since that script runs its argument checks at the
# top level and cannot be sourced for its constants alone. Keep the two equal.
set -euo pipefail

say()  { printf '\n== %s\n' "$*"; }
plan() { printf '   %s\n' "$*"; }
die()  { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

# The operator's cargo, rustup and PATH do the building; root has none of
# them. Measured 2026-09-30: under sudo the toolchain read empty and nvcc
# read none on a box that has both. sudo is asked for at the install step.
[ "$(id -u)" -ne 0 ] || die "run as the operator, not under sudo; the install step asks for sudo itself"

INSTALL=0
case "${1:-}" in
  "") ;;
  --install) INSTALL=1 ;;
  *) die "unknown argument $1" ;;
esac

PREFIX=${WEAVER_PREFIX:-/opt/weaver}
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
OPERATOR=${WEAVER_OPERATOR:-${SUDO_USER:-$USER}}
AGENT_DIR=${WEAVER_AGENT_DIR:-/var/lib/weaver-agent}
LOG_DIR=${WEAVER_LOG_DIR:-/var/log/weaver}
CUDA_LIB_DIR=${CUDA_LIB_DIR:-/opt/cuda/lib64}

MEMBERS="pyworker worker weaver-admin weaver-trace-relay weaver-gate weaver-spu weaver-state"
MEMBER_FEATURES=weaver-harness/pyworker,weaver-state/sqlite,weaver-state/postgres
SPU_FEATURES=weaver-spu/cuda
FEATURES="$SPU_FEATURES,$MEMBER_FEATURES"
# The engine's shared objects. llama-cpp-sys builds them into its own
# `out/lib` under the target's `build/` directory, with the versioned file,
# the SONAME link and the bare link side by side; the copies at the target
# root are bare links that dangle. The members carry no RUNPATH and name the
# SONAMEs, so the install writes an `ld.so.conf.d` entry for the lib
# directory and unit-properties sets LD_LIBRARY_PATH to it as well.
LIB_GLOBS='libggml*.so* libllama*.so*'
LDSO_CONF=/etc/ld.so.conf.d/weaver.conf

# ---------------------------------------------------------------- 1. box facts
say "box"
plan "host          $(hostname)"
plan "operator      $OPERATOR"
plan "prefix        $PREFIX"
plan "admin base    $ADMIN_BASE"
plan "stack record  $STACK"
plan "agent dir     $AGENT_DIR"
plan "log dir       $LOG_DIR"
plan "toolchain     $(rustup show active-toolchain 2>/dev/null | cut -d' ' -f1)"
plan "driver        $(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null | head -1 || echo none)"
plan "nvcc          $(nvcc --version 2>/dev/null | tail -1 | sed 's/^Build //' || echo none)"
CCCL=$(pacman -Q cccl 2>/dev/null | awk '{print $2}' || true)
plan "cccl          ${CCCL:-unknown}"
plan "postgresql    $(systemctl is-active postgresql 2>/dev/null || echo inactive)"
plan "will build    $FEATURES"
plan "will install  $MEMBERS"

cccl_in_window() {
  local v=${1%%-*} lo=3.1.4 hi=3.3.4
  [ "$(printf '%s\n%s\n' "$lo" "$v" | sort -V | head -1)" = "$lo" ] &&
  [ "$(printf '%s\n%s\n' "$v" "$hi" | sort -V | head -1)" = "$v" ]
}
if [ -n "$CCCL" ] && ! cccl_in_window "$CCCL"; then
  die "cccl $CCCL is outside the 3.1.4-3.3.4 window #397 measured. Fix the pin first."
fi
command -v nvcc >/dev/null || die "no nvcc on PATH and the build carries $SPU_FEATURES"
# **What stands is asked, and a question this script cannot answer refuses.**
# A path behind a directory the operator cannot traverse reads absent to `-e`,
# and a base the operator cannot list reads empty, and either would let an
# install go over a stack it never saw. `standing` answers stands or absent, or
# refuses where the look itself fails for any other reason.
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
if standing "$STACK"; then
  die "$STACK already stands. This is the first install; use update-stack.sh, or decommission.sh first."
fi
# An admin base that holds anything is a stack this script did not write: an
# agent's root, or the box-wide configuration of the layout before 2026-10-01,
# which REDEPLOY.md migrates by hand. An empty one is harmless and kept, and one
# that cannot be listed is never read as empty.
if standing "$ADMIN_BASE"; then
  listing=$(LC_ALL=C ls -A -- "$ADMIN_BASE" 2>&1) || die "cannot list $ADMIN_BASE to see whether it holds configuration: $listing"
  [ -z "$listing" ] || die "$ADMIN_BASE already holds configuration. This is the first install; use update-stack.sh, or decommission.sh first."
fi
if standing "$PREFIX/bin"; then die "$PREFIX/bin already stands. Decommission first."; fi
# **The base and the record are placed only where admin's rule holds**, judged
# at the nearest directory that stands, since this script makes the rest root
# 0755: admin refuses every verb on a root whose base or any directory above it
# another principal could write, and verify-load.sh refuses a record that is
# not closed (Codex on #45).
for placed in "$ADMIN_BASE" "$STACK" "$AGENT_DIR"; do
  at=$placed
  while ! standing "$at"; do at=$(dirname -- "$at"); done
  # Where a component is still missing, the nearest standing directory must be
  # one no other principal can make a name in, sticky or not.
  if [ "$at" = "$placed" ]; then
    bad=$(held_closed "$at") || die "$placed is not held closed by root at $bad, so admin would refuse it"
  else
    bad=$(creatable_in "$at") || die "$placed would be made under $bad, where another principal could claim the missing name first"
  fi
done

say "tree"
if git -C "$REPO" rev-parse --short HEAD >/dev/null 2>&1; then
  REV=$(git -C "$REPO" rev-parse --short HEAD)
  [ -z "$(git -C "$REPO" status --porcelain=v1 --untracked-files=all)" ] || REV="$REV-dirty"
else
  REV="nogit-$(sha256sum Cargo.lock | cut -c1-12)"
fi
plan "revision      $REV"
BUILT=$(cargo metadata --format-version 1 --no-deps --offline --locked 2>/dev/null \
  | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("target_directory",""))
except ValueError: pass' || true)
[ -n "$BUILT" ] || die "cargo metadata names no target directory"
BUILT="$BUILT/release"
plan "built into    $BUILT"
"$REPO/process/gates/lock.sh" >/dev/null 2>&1 && plan "lock          in step" || plan "lock          lock.sh answered $? (0 in step, 1 drift, 2 unchecked)"

# ---------------------------------------------------------------- 2. test
# weaver-analysis left the workspace on 2026-09-30, so the selection is the
# three members whose tests reach the seams this install serves.
say "test"
cargo test --release --locked \
  -p weaver-trace -p weaver-harness -p weaver-state \
  --features "$MEMBER_FEATURES" 2>&1 | grep -E '^test result' | \
  awk '{p+=$4; f+=$6} END {printf "   %d passed, %d failed\n", p, f; exit (f>0)}' \
  || die "tests failed; read them with: cargo test --release --locked -p weaver-trace -p weaver-harness -p weaver-state --features $MEMBER_FEATURES"

# ---------------------------------------------------------------- 3. build
say "build"
cargo build --release --locked --workspace --features "$FEATURES" 2>&1 | grep -E '^(error|warning: unused)' -A4 || true
for b in $MEMBERS; do [ -x "$BUILT/$b" ] || die "$BUILT/$b was not built"; done
SYS_OUT=$(ls -td "$BUILT"/build/llama-cpp-sys-2-*/out/lib 2>/dev/null | head -1)
[ -n "$SYS_OUT" ] || die "no llama-cpp-sys output under $BUILT/build; the engine was not built"
LIBS=()
for g in $LIB_GLOBS; do for f in "$SYS_OUT"/$g; do [ -e "$f" ] && LIBS+=("$f"); done; done
[ ${#LIBS[@]} -gt 0 ] || die "no engine libraries matched $LIB_GLOBS under $SYS_OUT"
plan "engine out     $SYS_OUT"
plan "members       $MEMBERS"
plan "libraries     $(printf '%s ' "${LIBS[@]##*/}")"

# ---------------------------------------------------------------- 4. plan
say "plan"
plan "install -d $PREFIX/bin $PREFIX/lib  (root, 0755)"
for b in $MEMBERS; do plan "install $b -> $PREFIX/bin/$b  $(sha256sum "$BUILT/$b" | cut -c1-12)"; done
plan "cp -a ${#LIBS[@]} library files and links -> $PREFIX/lib"
plan "write $LDSO_CONF = $PREFIX/lib and $CUDA_LIB_DIR, then ldconfig"
plan "write $STACK/{worker-binary,spu-binary,gate-binary,run-tool,control-tool,coordination-root,unit-properties,prefix,log-directory,agent-directory}  (root, 0755 / 0644)"
plan "install -d $ADMIN_BASE (root:root 0755): empty; create-agent.sh adds one root per agent"
plan "install -d $LOG_DIR (root:root 0750): each agent's operations log goes under it, the agent excluded by owner, group and search bit"
plan "install -d $AGENT_DIR (root:root 0755): territories, each root:weaver-<name>-state 0710, not setgid, its trace root:weaver-<name>-trace 0640"
[ -d "$PREFIX/models" ] && plan "$PREFIX/models stands: $(ls "$PREFIX/models" | wc -l) entries" || plan "$PREFIX/models is absent: copy the artifacts before declaring an agent"
[ "$INSTALL" -eq 1 ] || { say "plan only. rerun with --install"; exit 0; }

# ---------------------------------------------------------------- 5. install
say "install"
sudo -v || die "--install needs sudo"
sudo install -d -o root -g root -m 0755 "$PREFIX/bin" "$PREFIX/lib"
for b in $MEMBERS; do
  sudo install -o root -g root -m 0755 "$BUILT/$b" "$PREFIX/bin/$b"
  plan "installed $b"
done
sudo cp -a "${LIBS[@]}" "$PREFIX/lib/"
sudo chown -h root:root "$PREFIX"/lib/*
plan "installed ${#LIBS[@]} library files and links"
printf '%s\n%s\n' "$PREFIX/lib" "$CUDA_LIB_DIR" | sudo tee "$LDSO_CONF" >/dev/null
sudo chmod 0644 "$LDSO_CONF"
sudo ldconfig
plan "wrote $LDSO_CONF ($PREFIX/lib, $CUDA_LIB_DIR) and ran ldconfig"
if ldd "$PREFIX/bin/weaver-spu" 2>/dev/null | grep -q 'not found'; then
  ldd "$PREFIX/bin/weaver-spu" | grep 'not found' >&2
  die "weaver-spu has unresolved libraries after ldconfig"
fi
plan "weaver-spu resolves: $(ldd "$PREFIX/bin/weaver-spu" | grep -cE 'ggml|llama') engine objects from $PREFIX/lib"

say "stack record"
sudo install -d -o root -g root -m 0755 "$STACK"
# Each key is made 0644 whatever the umask: create-agent.sh and update-stack.sh
# read the record without privilege (Codex on #45).
w() { printf '%s\n' "$2" | sudo tee "$STACK/$1" >/dev/null; sudo chmod 0644 "$STACK/$1"; plan "$1 = $2"; }
w worker-binary "$PREFIX/bin/worker"
w spu-binary "$PREFIX/bin/weaver-spu"
w gate-binary "$PREFIX/bin/weaver-gate"
w run-tool /usr/bin/systemd-run
w control-tool /usr/bin/systemctl
w coordination-root /run
w prefix "$PREFIX"
w log-directory "$LOG_DIR"
w agent-directory "$AGENT_DIR"
printf 'UMask=0000\nEnvironment=LD_LIBRARY_PATH=%s:%s\nLogRateLimitIntervalSec=30s\nLogRateLimitBurst=1000000\n' \
  "$PREFIX/lib" "$CUDA_LIB_DIR" | sudo tee "$STACK/unit-properties" >/dev/null
sudo chmod 0644 "$STACK/unit-properties"
plan "unit-properties = UMask, LD_LIBRARY_PATH, journal rate limit off"
sudo install -d -o root -g root -m 0755 "$ADMIN_BASE"
plan "admin base $ADMIN_BASE (empty)"
sudo install -d -o root -g root -m 0750 "$LOG_DIR"
# The territories' base is root's, so no other principal can make a name in it
# and each territory is reached by group, never by an access entry (#28).
sudo install -d -o root -g root -m 0755 "$AGENT_DIR"

say "installed at $REV"
plan "next: deploy/create-agent.sh <name> --engine <sqlite|postgres> --artifact <path> [--apply], one agent at a time"
plan "then: sudo WEAVER_ADMIN_CONFIG=$ADMIN_BASE $PREFIX/bin/weaver-admin validate <name>"
plan "then: deploy/verify-load.sh <name>"
