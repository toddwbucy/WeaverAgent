#!/usr/bin/env bash
# Install the agent stack on a box that holds none: build at the lock, test,
# install the members and their libraries, and write the stack record.
#
#   deploy/bootstrap-stack.sh              plan: preflight, test, build; installs nothing, no sudo
#   deploy/bootstrap-stack.sh --install    then install under sudo
#
# **This is the first install; `update-stack.sh` is every later one.** That
# script diffs the built members and engine libraries against the installed
# ones and reads every path out of the stack record, so on a box with neither
# it has nothing to read and refuses. This one writes both from the facts below and then hands
# over: after it, `create-agent.sh` makes each agent and `update-stack.sh`
# carries the box forward. It refuses to run where a stack record already
# stands, because two writers of one record is how a box drifts.
#
# **The stack record is the scripts' and admin never reads it.** Admin reads
# one agent's root, `<admin base>/<agent>/`, and nothing shared. The record at
# `/etc/weaver/stack/` holds the box-wide defaults, one file per key, which
# `create-agent.sh` copies into each new agent's root: `worker-binary`,
# `spu-binary`, `gate-binary`, `shell-binary`, `coordination-root` and `library-path`, and for
# the scripts alone `prefix`, `agent-directory` and `install-tuple`, the
# box-side half of the deployment tuple (deploy/install-tuple.sh), which
# update-stack.sh rewrites after each verified install. `headroom-bytes` and
# `load-bound-seconds` are optional, and this script writes neither. An
# operator who writes one into the record has it copied into every agent made
# after. The admin base is created empty here, root-owned 0755, and gains one
# root per agent. **No init system is involved**: admin's start step stands
# the agent itself, on the operator's ruling of 2026-10-03 (#50), so no unit,
# run tool or control tool is installed or recorded, and each agent's
# operations log lives in its territory rather than under a box log
# directory.
#
# Box facts are environment, defaulted, printed, and never discovered from a
# directory listing (the install set is named, per update-stack.sh):
#
#   WEAVER_PREFIX        /opt/weaver            bin/ lib/ models/
#   WEAVER_ADMIN_CONFIG  /etc/weaver/admin      the admin base: one root per agent
#   WEAVER_STACK_RECORD  /etc/weaver/stack      the scripts' record of this install
#   WEAVER_OPERATOR      $SUDO_USER or $USER    named in the plan; owns nothing this makes
#   WEAVER_AGENT_DIR     /var/lib/weaver-agent
#   CUDA_LIB_DIR         /opt/cuda/lib64        joins the ld.so.conf.d entry; held closed by root
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
source "$REPO/deploy/retired-python.sh"
source "$REPO/deploy/engine-libraries.sh"
source "$REPO/deploy/install-tuple.sh"

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

refuse_python_install "$PREFIX" "$PREFIX/bin"

MEMBERS="worker weaver-admin weaver-trace-relay weaver-gate weaver-spu weaver-state"
# The agent's shell (#29): the static busybox and its applet list, installed
# beside the members, as update-stack.sh names them.
SHELL_FILES="busybox busybox.applets"
MEMBER_FEATURES=weaver-state/sqlite
SPU_FEATURES=weaver-spu/cuda
FEATURES="$SPU_FEATURES,$MEMBER_FEATURES"
# The engine's shared objects, the crate that builds them, the loader's
# entry and the CUDA directory it names are deploy/engine-libraries.sh's,
# which update-stack.sh sources as well.

# ---------------------------------------------------------------- 1. box facts
say "box"
plan "host          $(hostname)"
plan "operator      $OPERATOR"
plan "prefix        $PREFIX"
plan "admin base    $ADMIN_BASE"
plan "stack record  $STACK"
plan "agent dir     $AGENT_DIR"
plan "toolchain     $(rustup show active-toolchain 2>/dev/null | cut -d' ' -f1)"
# **The fallback is applied after the pipeline** (#39): under `pipefail`, a
# `head` that closes the pipe on a box with more than one card fails it, and an
# `|| echo none` inside the substitution then printed a stray `none` line.
DRIVER=$(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null | head -1 || true)
plan "driver        ${DRIVER:-none}"
plan "nvcc          $(nvcc --version 2>/dev/null | tail -1 | sed 's/^Build //' || echo none)"
CCCL=$(pacman -Q cccl 2>/dev/null | awk '{print $2}' || true)
plan "cccl          ${CCCL:-unknown}"
plan "will build    $FEATURES"
# The line names all three things this script installs: the members, the
# shell and the engine libraries. It named the members alone.
plan "will install  $MEMBERS, the shell ($SHELL_FILES), and the engine libraries ($LIB_GLOBS) built by $ENGINE_CRATE"

cccl_in_window() {
  local v=${1%%-*} lo=3.1.4 hi=3.3.4
  [ "$(printf '%s\n%s\n' "$lo" "$v" | sort -V | head -1)" = "$lo" ] &&
  [ "$(printf '%s\n%s\n' "$v" "$hi" | sort -V | head -1)" = "$v" ]
}
if [ -n "$CCCL" ] && ! cccl_in_window "$CCCL"; then
  die "cccl $CCCL is outside the 3.1.4-3.3.4 window #397 measured. Fix the pin first."
fi
command -v nvcc >/dev/null || die "no nvcc on PATH and the build carries $SPU_FEATURES"
# **The shell's build needs its tools before the long build, not after** (#29).
"$REPO/deploy/build-busybox.sh" --preflight || exit 1
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
# which is taken down, never migrated. An empty one is harmless and kept, and one
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
# The CUDA directory joins the loader's entry beside $PREFIX/lib, so it is
# judged as admin judges the library path (deploy/engine-libraries.sh).
loader_dir CUDA_LIB_DIR "$CUDA_LIB_DIR"

say "tree"
# The install tuple names the commit in full, read here and not after the
# build, since the checkout may move during it. A status that cannot be read
# refuses rather than reading as a clean tree.
if git -C "$REPO" rev-parse --short HEAD >/dev/null 2>&1; then
  REV=$(git -C "$REPO" rev-parse --short HEAD)
  COMMIT=$(git -C "$REPO" rev-parse HEAD)
  TREE_STATUS=$(git -C "$REPO" status --porcelain=v1 --untracked-files=all) \
    || die "cannot read the tree's status, so whether $REV is the tree built is unknown"
  [ -z "$TREE_STATUS" ] || { REV="$REV-dirty"; COMMIT="$COMMIT-dirty"; }
else
  REV="nogit-$(sha256sum Cargo.lock | cut -c1-12)"
  COMMIT=$REV
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
# **Cargo's output goes to a log, never through a pipe** (#128): a crate
# that fails to compile prints no `test result` line, and the old
# `cargo | grep | awk` answered "0 passed, 0 failed", took grep's status under
# `pipefail` rather than cargo's, and discarded every diagnostic. Cargo's own
# status is kept, the counts are read from the log whole, and a failure, a
# failed test or no test at all prints the failures and the lines cargo names
# as the cause on stderr, and refuses naming the log, which is kept; a clean
# run removes it. The log is the operator's, beside cargo's own
# output in the target directory, named by this run's pid so runs sharing a
# target directory never write each other's, and named as the step begins so
# a long step can be followed.
run_tests() {
  local log="${BUILT%/release}/weaver-deploy-tests-$$.log" rc=0 totals passed failed cause
  mkdir -p -- "${BUILT%/release}" || die "cannot make the target directory ${BUILT%/release} for the test step's log"
  printf '   output: %s\n' "$log"
  cargo test --release --locked \
    -p weaver-trace -p weaver-harness -p weaver-state \
    --features "$MEMBER_FEATURES" >"$log" 2>&1 || rc=$?
  # **Only a complete summary line counts** (#507), its five counts present
  # through `filtered out` (libtest adds the duration only when it measured
  # one). Child test processes share cargo's stdout, so two summary lines can
  # interleave, and a garbled one read field by field once gave a failed
  # count of 0.29, which `%d` refused under errexit, ending the script on a
  # suite cargo passed. The verdict is cargo's status, which a failed test
  # sets, and a run that counted no test at all; the counts are read from
  # complete summary lines only and printed as text, so no total a garble
  # could make can end the step or decide it.
  totals=$(awk '
    /^test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed; [0-9]+ ignored; [0-9]+ measured; [0-9]+ filtered out/ {p+=$4; f+=$6}
    END {print p+0, f+0}' "$log") \
    || die "cannot read the test step's log $log"
  passed=${totals% *}
  failed=${totals#* }
  printf '   %s passed, %s failed\n' "$passed" "$failed"
  if [ "$rc" -ne 0 ] || [ "$passed" = 0 ]; then
    # **The failures and cargo's cause, never the run's end** (the live plan
    # of 2026-11-09): the harness's suites write queue-pressure JSON lines and
    # every passing test's line to cargo's stdout, and the log's last 60
    # carried them, and the failures block a second time, before the
    # refusal. Echoed, in the log's order: each libtest `failures:` block
    # through its `test result:` line (a FAILED summary outside a block too),
    # the block closed by a summary a child's line garbled (#507) as well;
    # then, outside those blocks, the first 12 of cargo's `error` lines with
    # the 6 after each, and of `panicked at` lines with the message after
    # each, which name a crate that did not compile or a test binary that
    # died. Where none of these stands (a run of no tests), the log's last
    # 20 lines, which then carry no test's output. The whole output stays in
    # the kept log.
    cause=$(awk '
      /^failures:$/ {held = 1}
      held || /^test result: FAILED\./ {print; if (/test result: /) held = 0; next}
      /^error(\[|:)/ && shown < 12 {after = 7; shown++}
      /panicked at / && shown < 12 {if (after < 2) after = 2; shown++}
      after > 0 {print; after--}' "$log") || true
    if [ -n "$cause" ]; then
      printf '%s\n' "$cause" >&2
    else
      tail -n 20 -- "$log" >&2
    fi
    die "the tests did not pass (cargo exited $rc, $passed passed, $failed failed); the whole output is kept in $log"
  fi
  rm -f -- "$log"
}
run_tests

# ---------------------------------------------------------------- 3. build
say "build"
# **A build that fails refuses with its diagnostics** (#128): its status was
# discarded behind `| grep ... || true`, and the binary check below then
# passed on whatever an earlier build left in the target directory. The
# output goes to a log as the test step's does; a clean build prints its
# unused-code warnings from it as before. **The CUDA host compiler is
# update-stack.sh's default too**, the two scripts building one workspace:
# a failed build no longer passing on stale binaries, a host compiler the
# kernels refuse would otherwise stop a bootstrap the update builds.
# The build's machine messages go to BUILD_MESSAGES, a file mktemp makes,
# which `engine_out` reads for the engine's output directory
# (deploy/engine-libraries.sh); its rendered diagnostics and progress go to
# the log.
run_build() {
  local log="${BUILT%/release}/weaver-deploy-build-$$.log" rc=0
  BUILD_MESSAGES=$(mktemp "${BUILT%/release}/weaver-deploy-messages-XXXXXX") \
    || die "no file for the build's messages could be made beside $BUILT"
  # No other exit trap stands, so a signal during the long build still
  # removes the file.
  trap 'rm -f -- "$BUILD_MESSAGES"' EXIT
  printf '   output: %s\n' "$log"
  NVCC_CCBIN=${NVCC_CCBIN:-/usr/bin/g++-15} \
    cargo build --release --locked --workspace --message-format=json-render-diagnostics \
      --features "$FEATURES" >"$BUILD_MESSAGES" 2>"$log" || rc=$?
  if [ "$rc" -ne 0 ]; then
    rm -f -- "$BUILD_MESSAGES"
    # The cause first, where cargo names one, then the run's end: a long
    # captured output can push the panic or the first error out of the tail.
    grep -m 12 -E -A 6 '^(error(\[|:)|failures:|---- )|panicked at' -- "$log" >&2 || true
    printf '   ...\n' >&2
    tail -n 60 -- "$log" >&2
    die "the build failed (cargo exited $rc); the whole output is kept in $log"
  fi
  grep -E '^(error|warning: unused)' -A4 -- "$log" || true
  rm -f -- "$log"
}
run_build
# **The engine's output directory is the one cargo names for this build**,
# never the newest by time (deploy/engine-libraries.sh).
SYS_OUT=$(engine_out "$ENGINE_CRATE" "$BUILD_MESSAGES" "$BUILT/build" || true)
rm -f -- "$BUILD_MESSAGES"
trap - EXIT
[ -n "$SYS_OUT" ] || die "the build reported no single output directory under $BUILT/build for $ENGINE_CRATE, the lock's one package of that name, so which engine libraries it built is unknown"
SYS_OUT="$SYS_OUT/lib"
[ -d "$SYS_OUT" ] || die "the engine's output directory names no lib/ at $SYS_OUT, so the build made no engine libraries to install"
for b in $MEMBERS; do [ -x "$BUILT/$b" ] || die "$BUILT/$b was not built"; done
# **The agent's shell, built from its pinned source and configuration** (#29,
# weaver-gate-Spec 8.1): a static busybox and its applet list, written beside
# the members and installed with them.
# BUSYBOX_TARBALL, where set, names the pinned tarball on a box without the
# network (deploy/REDEPLOY.md section 3).
build_busybox() {
  if [ -n "${BUSYBOX_TARBALL:-}" ]; then
    "$REPO/deploy/build-busybox.sh" "$BUILT" --tarball "$BUSYBOX_TARBALL"
  else
    "$REPO/deploy/build-busybox.sh" "$BUILT"
  fi
}
build_busybox || die "the shell's busybox did not build"
LIBS=()
LIB_NAMES=()
for g in $LIB_GLOBS; do for f in "$SYS_OUT"/$g; do [ -e "$f" ] && LIBS+=("$f"); done; done
# The versioned files, which the install tuple names by digest; the links
# carry no content of their own.
for f in "${LIBS[@]}"; do [ -L "$f" ] || LIB_NAMES+=("${f##*/}"); done
[ ${#LIBS[@]} -gt 0 ] || die "no engine libraries matched $LIB_GLOBS under $SYS_OUT"
plan "engine out    $SYS_OUT"
plan "members       $MEMBERS"
plan "libraries     $(printf '%s ' "${LIBS[@]##*/}")"

# ---------------------------------------------------------------- 4. plan
say "plan"
plan "install -d $PREFIX/bin $PREFIX/lib  (root, 0755)"
for b in $MEMBERS; do plan "install $b -> $PREFIX/bin/$b  $(sha256sum "$BUILT/$b" | cut -c1-12)"; done
plan "cp -a ${#LIBS[@]} library files and links -> $PREFIX/lib"
plan "write $LDSO_CONF = $PREFIX/lib and $CUDA_LIB_DIR, then ldconfig"
plan "install busybox -> $PREFIX/bin/busybox (root, 0755) and busybox.applets beside it (root, 0644)"
plan "write $STACK/{worker-binary,spu-binary,gate-binary,shell-binary,coordination-root,library-path,prefix,agent-directory,install-tuple}  (root, 0755 / 0644)"
plan "install -d $ADMIN_BASE (root:root 0755): empty; create-agent.sh adds one root per agent"
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
sudo install -o root -g root -m 0755 "$BUILT/busybox" "$PREFIX/bin/busybox"
sudo install -o root -g root -m 0644 "$BUILT/busybox.applets" "$PREFIX/bin/busybox.applets"
plan "installed busybox and busybox.applets"
# Each placed by `install_library` (deploy/engine-libraries.sh), root's from
# the moment it has a name.
for f in "${LIBS[@]}"; do install_library "$f" "$PREFIX/lib/${f##*/}"; done
plan "installed ${#LIBS[@]} library files and links"
printf '%s\n%s\n' "$PREFIX/lib" "$CUDA_LIB_DIR" | sudo tee "$LDSO_CONF" >/dev/null
sudo chmod 0644 "$LDSO_CONF"
sudo ldconfig
plan "wrote $LDSO_CONF ($PREFIX/lib, $CUDA_LIB_DIR) and ran ldconfig"
# **The SPU resolves its engine from the library directory**, as admin's
# start step will run it (deploy/engine-libraries.sh): ldd's status, no
# library not found, and every engine library a file in $PREFIX/lib. Read
# whole, then searched: `ldd | grep -q` under `pipefail` answered false where
# grep's early exit killed ldd, passing an unresolved library.
rc=0
LDD_SAID=$(spu_ldd "$PREFIX/bin/weaver-spu" "$PREFIX/lib") || rc=$?
[ "$rc" -eq 0 ] || { printf '%s\n' "$LDD_SAID" >&2; die "ldd could not read the installed weaver-spu (it exited $rc)"; }
if ! UNRESOLVED=$(engine_resolved "$LDD_SAID" "$PREFIX/lib"); then
  printf '%s\n' "$UNRESOLVED" >&2
  die "weaver-spu has unresolved libraries after ldconfig, or resolves its engine outside $PREFIX/lib"
fi
plan "weaver-spu resolves: $(grep -cE '^[[:space:]]*lib(ggml|llama)' <<<"$LDD_SAID" || true) engine objects from $PREFIX/lib"

say "stack record"
sudo install -d -o root -g root -m 0755 "$STACK"
# Each key is made 0644 whatever the umask: create-agent.sh and update-stack.sh
# read the record without privilege (Codex on #45).
w() { printf '%s\n' "$2" | sudo tee "$STACK/$1" >/dev/null; sudo chmod 0644 "$STACK/$1"; plan "$1 = $2"; }
w worker-binary "$PREFIX/bin/worker"
w spu-binary "$PREFIX/bin/weaver-spu"
w gate-binary "$PREFIX/bin/weaver-gate"
w shell-binary "$PREFIX/bin/busybox"
w coordination-root /run
w library-path "$PREFIX/lib"
w prefix "$PREFIX"
w agent-directory "$AGENT_DIR"
sudo install -d -o root -g root -m 0755 "$ADMIN_BASE"
plan "admin base $ADMIN_BASE (empty)"
# The territories' base is root's, so no other principal can make a name in it
# and each territory is reached by group, never by an access entry (#28).
sudo install -d -o root -g root -m 0755 "$AGENT_DIR"

# **The install tuple** (deploy/install-tuple.sh), read from what this run
# installed, the same record update-stack.sh writes after each install.
say "tuple"
TUPLE=$(install_tuple "$REPO" "$COMMIT" "$PREFIX/bin" "$PREFIX/lib" "$FEATURES" "$MEMBERS $SHELL_FILES" "${LIB_NAMES[*]}")
printf '%s\n' "$TUPLE" | sed 's/^/   /'
printf '%s\n' "$TUPLE" | sudo tee "$STACK/install-tuple" >/dev/null
sudo chmod 0644 "$STACK/install-tuple"
plan "written to $STACK/install-tuple"

say "installed at $REV"
plan "next: deploy/create-agent.sh <name> --artifact <path> [--apply], one agent at a time"
plan "then: validate each agent with the validate verb of $(printf '%q' "$PREFIX/bin/weaver-admin") as root, and prove its load with deploy/verify-load.sh as root in a delegated scope (deploy/HowToDeployANewAgent.md section 4)"
