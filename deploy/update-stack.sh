#!/usr/bin/env bash
# Bring this box's installed agent stack to the tree's current main.
#
# Box-agnostic: every path is read from the stack record `bootstrap-stack.sh`
# wrote (`/etc/weaver/stack/`, or WEAVER_STACK_RECORD) and from each agent's
# root under the admin base (`/etc/weaver/admin/<agent>/`, or
# WEAVER_ADMIN_CONFIG), rather than written here, so the same script serves
# either seat. The agents are the roots under the base: admin admits an agent
# by its root existing, and this script serves the same set. Each agent's
# declaration is read from the directory its root's `declaration-directory`
# names, the operator's own, as the operator running this script.
#
#   ./deploy/update-stack.sh            plan only; refreshes refs, tests and builds, no install
#   ./deploy/update-stack.sh --install  plan, then install what changed
#
# Plan mode writes git refs and build artifacts. On main it also fast-forwards
# HEAD and rewrites the working tree. It invokes no sudo.
# The plan is the point. An install that swaps every binary hides which act
# actually moved, and the campaign's comparability rests on knowing that, so
# this diffs deployed against built and installs only what differs.
set -euo pipefail

ORIGINAL_ARGS=("$@")
INSTALL=0
[ "${1:-}" = "--install" ] && INSTALL=1

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ADMIN_BASE=${WEAVER_ADMIN_CONFIG:-/etc/weaver/admin}
STACK=${WEAVER_STACK_RECORD:-/etc/weaver/stack}
cd "$REPO"

say() { printf '\n== %s\n' "$*"; }
die() { printf '\nREFUSED: %s\n' "$*" >&2; exit 1; }

read_key() { cat "$STACK/$1" 2>/dev/null || true; }

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
[ -d "$STACK" ] || die "no stack record at $STACK: bootstrap-stack.sh writes it"
# **The stack record is judged before any value of it is trusted**: the record
# and every entry in it held closed by admin's rule, since a root step below
# acts on the paths it names (the walk of 2026-10-01, #45 round 11).
bad=$(held_closed "$STACK") || die "the stack record $STACK is not held closed by root at $bad"
for entry in "$STACK"/*; do
  bad=$(held_closed "$entry") || die "the stack record's $entry is not held closed by root at $bad"
done

# **One reader for every value this script takes from a declaration**, through
# python3's tomllib, so the script decodes what admin decodes within the TOML
# 1.0 grammar declarations are written in, per weaver-types-Spec section 2: a
# literal string, an escape, a dotted key and an inline table each read as the
# value they are, where a line-matching reader saw one spelling and missed or
# mangled the rest. It prints the string at a dotted path, or checks that a
# table stands there, and answers 3 where the path is absent. A file that does
# not parse, or a value of another kind than asked, refuses by name.
declared() {
  python3 -c '
import sys, tomllib
path, key, want = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    with open(path, "rb") as fh:
        value = tomllib.load(fh)
except (OSError, tomllib.TOMLDecodeError) as e:
    sys.exit(f"{path} is not a TOML 1.0 document, the grammar every reader in the suite shares (weaver-types-Spec section 2): {e}")
for part in key.split("."):
    if not isinstance(value, dict) or part not in value:
        sys.exit(3)
    value = value[part]
if want == "string" and isinstance(value, str):
    print(value)
elif not (want == "table" and isinstance(value, dict)):
    sys.exit(f"{path}: {key} is not a {want}")
' "$@"
}

# Reads a run's new trace lines on stdin and prints what the load event says
# about the two facts #419 put there. Non-zero where no load event names a
# composer, which is how the verify step knows the install took.
weaver_read_load() {
  python3 -c '
import sys, json
for line in sys.stdin:
    try:
        e = json.loads(line)
    except ValueError:
        continue
    if e.get("kind") == "load":
        p = e.get("payload", {})
        print("  state_member:", json.dumps(p.get("state_member")))
        print("  composer:    ", json.dumps(p.get("composer")))
        if p.get("state_store"):
            print("  state_store: ", json.dumps(p["state_store"]))
        sys.exit(0 if p.get("composer") else 1)
sys.exit(1)
'
}

# **Every box fact is required, because this script's whole purpose is that
# two boxes end up the same.** `read_key` answers empty for a key that is not
# there, so a fact read and not checked is a divergence the run carries
# silently, and the seats only find it by comparing results later.
WORKER_BINARY=$(read_key worker-binary)
[ -n "$WORKER_BINARY" ] || die "no worker-binary in the stack record $STACK"
BIN_DIR=$(dirname "$WORKER_BINARY")

# **A box still on the box-wide layout refuses before anything is built.**
# Before 2026-10-01 admin read one configuration for every agent, its
# allow-list naming them and its `agent-config-directory` holding their
# declarations. The admin this script installs reads only `<base>/<agent>/`,
# so on such a box it would find no agent at all, and every one would refuse
# after the install. The migration is a one-time manual step, REDEPLOY.md
# section 7, made while the old stack still stands.
for retired in allow-list agent-config-directory spu-implementations agent-spu; do
  [ ! -e "$ADMIN_BASE/$retired" ] || die "$ADMIN_BASE/$retired stands: this box is on the box-wide layout. \
Migrate it to one root per agent first (deploy/REDEPLOY.md section 7), then rerun."
done
# **An agent still running under a unit of the layout before #50 refuses.**
# The admin this script installs starts no unit and stops none: it ends a run
# by its run lock, which a unit's worker never took. So a worker a unit still
# serves would outlive the install, unreachable by the new `unload`. Unload
# each agent with the admin that started it, then rerun (deploy/REDEPLOY.md
# section 8).
# A box without systemd has no such unit: no `systemctl`, or none running as
# the system manager, which `/run/systemd/system` marks (sd_booted(3)), since
# a client installed beside another init cannot reach a manager (Codex on
# #79). One whose systemd cannot answer refuses, a failed look never read as
# no unit.
# **Every state in which a unit can hold a worker refuses**: active, and the
# transitions into and out of it, `activating`, `reloading` and
# `deactivating`, which systemctl lists apart from `active` (Codex on #79).
# The look runs here, before anything is built, and again just before the
# first binary is replaced, since the test and the build take minutes.
refuse_legacy_units() {
  local listing units
  command -v systemctl >/dev/null && [ -d /run/systemd/system ] || return 0
  listing=$(systemctl list-units 'weaver-worker@*' --state=active,activating,reloading,deactivating \
      --no-legend --plain 2>&1) \
    || die "cannot ask systemd whether units of the layout before #50 still serve: $listing"
  units=$(printf '%s\n' "$listing" | awk 'NF {print $1}' | tr '\n' ' ')
  [ -z "${units// /}" ] || die "units of the layout before #50 still serve: $units. Unload each agent with the installed admin first (deploy/REDEPLOY.md section 8), then rerun."
}
refuse_legacy_units

# **The agents are the roots under the base**, named as admin's name check
# admits them (ASCII letters, digits, `-` and `_`), so a staged root
# `create-agent.sh` left under a dot-name is not one. A symlink is not a root.
# A root naming a `declaration-directory` is an agent, and its declaration is
# `agent.toml` there. **A root of the layout before #50 refuses by name**: one
# holding `agent.toml` itself, or the retired `run-tool`, `control-tool`,
# `unit-properties` or `log-path`, is migrated by hand first (deploy/REDEPLOY.md
# section 8), since the admin this script installs reads none of them and would
# refuse the agent at every verb.
# **A root this user cannot read refuses, naming it, and is never left out.**
# Every step below reads each root as this user, without privilege: the key
# comparison, the store reconciliation, the plan. A root closed to this user
# answered `-f agent.toml` false and dropped out of AGENTS, so `--install`
# replaced the shared binaries, verified only the agents it could see and
# called the box current (Codex on #45). `create-agent.sh` makes every root
# 0755, so a closed one is a hand-made state, and the run stops on it.
OPERATOR_NAME=$(id -un)
[ -r "$ADMIN_BASE" ] && [ -x "$ADMIN_BASE" ] \
  || die "$ADMIN_BASE cannot be listed by $OPERATOR_NAME, so its agent roots cannot be enumerated"
AGENTS=""
for root in "$ADMIN_BASE"/*/; do
  root=${root%/}
  [ -d "$root" ] && [ ! -L "$root" ] || continue
  agent=${root##*/}
  [[ "$agent" =~ ^[A-Za-z0-9_-]+$ ]] || continue
  [ -r "$root" ] && [ -x "$root" ] \
    || die "$root is closed to $OPERATOR_NAME, so whether it is an agent, and what it names, cannot be read. Open it to 0755 as create-agent.sh makes it, then rerun."
  for retired in agent.toml run-tool control-tool unit-properties log-path; do
    [ ! -e "$root/$retired" ] || die "$root holds $retired: it is on the layout before #50. Migrate it first (deploy/REDEPLOY.md section 8), then rerun."
  done
  [ -f "$root/declaration-directory" ] || continue
  AGENTS="$AGENTS $agent"
done
[ -n "$AGENTS" ] || die "no agent root under $ADMIN_BASE: make one with create-agent.sh first"

# **Each agent's declaration, read as the operator.** The root names the
# directory, which create-agent.sh made the operator's own and closed, so the
# operator running this script reads it without privilege. One it cannot read
# refuses by name and is never left out, on the ground the root's check gives.
declaration_of() { # declaration_of AGENT: prints the path of its agent.toml
  local dir
  dir=$(cat "$ADMIN_BASE/$1/declaration-directory" 2>/dev/null) || die "$1: its root's declaration-directory does not read"
  dir=${dir#"${dir%%[![:space:]]*}"}; dir=${dir%"${dir##*[![:space:]]}"}
  [[ "$dir" == /* ]] || die "$1: its root's declaration-directory is not an absolute path"
  printf '%s/agent.toml' "$dir"
}
for agent in $AGENTS; do
  decl=$(declaration_of "$agent") || exit 1
  [ -r "$decl" ] || die "$agent: its declaration $decl cannot be read by $OPERATOR_NAME. It is the operator's own: run this script as the operator who owns it."
done

# **Where cargo builds is asked rather than assumed.** This box sets
# `CARGO_TARGET_DIR`, so `target/release` does not exist here, and every
# comparison against it silently found no file, skipped every binary, and
# reported the box current while three-week-old binaries stood installed.
# A path that can be wrong without saying so is worse than no comparison.
# **The answer is judged on whether cargo answered, not on whether the
# directory is there yet.** A clean rebuild has no release directory at this
# point, so testing for one sent a box that sets `CARGO_TARGET_DIR` back to
# `target/release` and reopened the defect above through a second door. Where
# cargo names no target directory at all there is nothing to fall back to
# that would not be a guess, so the run refuses instead.
# **The answer is read out of the JSON rather than matched out of it.** A
# regular expression does not decode what JSON escapes, so a target directory
# holding a backslash kept it doubled and one holding a quote truncated at
# the quote, both giving a non-empty path that is wrong. Measured: a real
# `/tmp/a"b` came back as `/tmp/a\`. This script already parses the trace
# with python for the same reason, so the reader is the one it has.
# **The refusal below has to be reachable.** Under `pipefail` a failing cargo
# takes the whole assignment down, and with `set -e` the run ended on cargo's
# own exit code with nothing said. Measured at 101 and silent. The failure is
# absorbed here so the empty answer reaches the line written to name it.
BUILT=$( (cargo metadata --format-version 1 --no-deps --offline --locked 2>/dev/null \
  | python3 -c 'import json, sys
try:
    print(json.load(sys.stdin).get("target_directory", ""))
except ValueError:
    pass') || true )
[ -n "$BUILT" ] || die "cargo metadata names no target directory, so where the build lands is unknown"
BUILT="$BUILT/release"

# **The features the build takes decide what an agent can elect.**
# `weaver-state` carries its engines behind features, so a build that does not
# name an engine installs a member that refuses every agent electing it.
# Measured 2026-09-11: karl's load refused with `descriptors_unusable` while
# the territory's state.log held the real fault, `no engine named "postgres"
# in this binary`, and the installed member carried none of the postgres
# symbols the pinned experiment stacks do.
#
# **Every engine the stack serves is named, the default one included.**
# `weaver-state` is `default = ["sqlite"]`, so an earlier form of this line
# shipped sqlite by inheritance while claiming to enumerate what is in.
# Narrowing that default, or passing `--no-default-features` for any reason,
# would then have refused every sqlite-electing agent: this act's own fault in
# the other engine, with this line printing a set that said nothing about it.
# Substrates arrive one at a time and each is named rather than inherited.
#
# **Two sets, because the test step selects fewer packages than the build.**
# A feature of a package the step does not select refuses with `none of the
# selected packages contains this feature`, so the members' set is stated once
# and the spu's is added for the build alone.
# **What the deployment installs is named, not discovered.** A form of the
# plan below walked the build directory to find its subject, and this box sets
# `CARGO_TARGET_DIR` to a directory another project shares: the plan proposed
# installing fifty-odd files, every `.d` and `.rlib` among them, and `hades`
# and its libraries from a workspace that has nothing to do with this one.
# Discovering the subject from a directory is how a deployment installs what
# it was never asked to. The build also makes `weaver-spu-classify`, which
# this script does not ship; `weaver-analysis` left the workspace for its own
# repository on 2026-09-30. The build below is
# the whole workspace, the frontend having left the repository on 2026-09-26.
# A member joins the installed set by being written here.
MEMBERS="pyworker worker weaver-admin weaver-trace-relay weaver-gate weaver-spu weaver-state"

MEMBER_FEATURES=weaver-harness/pyworker,weaver-state/sqlite,weaver-state/postgres
SPU_FEATURES=weaver-spu/cuda
FEATURES="$SPU_FEATURES,$MEMBER_FEATURES"

# ---------------------------------------------------------------- 1. box facts
say "box"
printf '  host          %s\n' "$(hostname)"
printf '  stack record  %s\n' "$STACK"
printf '  admin base    %s\n' "$ADMIN_BASE"
printf '  agents       %s\n' "$AGENTS"
printf '  bin dir       %s\n' "$BIN_DIR"
printf '  worker-binary %s\n' "$WORKER_BINARY"
# **Each agent's binary paths are its own and this run does not move them.**
# It installs over the files at the stack record's paths, so an agent whose
# root names those paths is updated in place and nothing in its root changes;
# one whose root names another file (an SPU chosen with `create-agent.sh
# --spu`, or a path written by hand) keeps serving that file, and is printed
# here so the reader knows this run does not reach it.
for agent in $AGENTS; do
  for key in worker-binary gate-binary spu-binary; do
    # A key that stands and does not read is not an absent one (Codex on #45).
    if [ -e "$ADMIN_BASE/$agent/$key" ] && [ ! -r "$ADMIN_BASE/$agent/$key" ]; then
      die "$ADMIN_BASE/$agent/$key stands and cannot be read by $(id -un), so whether this run updates $agent is unknown"
    fi
    own=$(cat "$ADMIN_BASE/$agent/$key" 2>/dev/null || true)
    [ "$own" = "$(read_key "$key")" ] \
      || printf '  %-12s %s = %s, not the stack record'"'"'s; this run does not update it\n' "$agent" "$key" "${own:-(absent)}"
  done
done
# **The built-from path is a box fact and is printed as one.** It differs
# between the seats, one of them setting `CARGO_TARGET_DIR`, and it was the
# difference that let this script report a box current while comparing
# nothing. A fact that decides the answer belongs where a reader of the
# output can see it.
printf '  built from    %s\n' "$BUILT"
# **The feature set is a box fact for the same reason the paths are**, and it
# is printed as what it is. This line runs before the build and before any
# install, so it is what this run would build with and never a reading of the
# member already on the box. The outcome is reported where an outcome can be
# read: step 9 prints the `state_store` the load event names, out of the sink,
# after the member has served it.
printf '  will build    %s\n' "$FEATURES"
printf '  will install  %s\n' "$MEMBERS"
# **One line, because the query answers once per device.** Three cards gave
# three lines into one `%s`, so two of them printed with no label and no
# indent. The driver is the box's, not the card's, so the first answer is the
# answer and a disagreement between cards is not a thing this can report.
printf '  driver        %s\n' "$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1 || echo none)"

# The cccl window of #397. Outside it the engine does not compile, and a
# failure here is cheaper than one twenty minutes into a build.
CCCL=$(pacman -Q cccl 2>/dev/null | awk '{print $2}' || true)
printf '  cccl          %s\n' "${CCCL:-unknown}"
# **Both ends, and by version order rather than by glob.** A `3.3.*` pattern
# waves 3.3.5 through, which is outside what #397 measured, and a `3.1.4*`
# one turns away 3.1.5, which is inside it. The pacman release suffix is
# dropped before the comparison.
cccl_in_window() {
  local v=${1%%-*} lo=3.1.4 hi=3.3.4
  [ "$(printf '%s\n%s\n' "$lo" "$v" | sort -V | head -1)" = "$lo" ] &&
  [ "$(printf '%s\n%s\n' "$v" "$hi" | sort -V | head -1)" = "$v" ]
}
if [ -z "$CCCL" ]; then
  printf '  (cccl not queryable; not gating on it)\n'
elif ! cccl_in_window "$CCCL"; then
  die "cccl $CCCL is outside the 3.1.4-3.3.4 window #397 measured. Fix the pin first."
fi

# **What the agents elect is checked against what the build will carry**, on
# the review of 2026-09-13 and for the reason the cccl gate above gives: a
# failure here is cheaper than one twenty minutes into a build, and this class
# cost a build, an install and a rollback before it named itself. `weaver-admin
# validate` cannot cover it. It asks that the binary exists and that the store
# admits the role, never that the binary carries the engine, which is why karl
# validated and then refused at load.
#
# **The two statements are separate on purpose and reconciled here.**
# `create-agent.sh` writes the engine into a declaration and this script names
# what the build carries, and neither reads the other. That is tolerable only
# while something compares them, so this is that something: edit one and not
# the other and the run refuses by name before it spends the build.
for agent in $AGENTS; do
  decl=$(declaration_of "$agent") || exit 1
  [ -f "$decl" ] || continue
  # The engine at `state-store.engine`, read by `declared` as the string
  # admin decodes, and an absent election means the crate's own default
  # rather than none.
  rc=0
  elected=$(declared "$decl" state-store.engine string) || rc=$?
  case $rc in
    0) ;;
    3) elected=sqlite ;;
    *) die "$agent: the declaration's store election does not read, see above" ;;
  esac
  # **`none` is an election and not an absence.** It is a lawful `StoreEngine`
  # and admin starts no member for it, per `inventory.rs`, which does not even
  # ask for the member binary in that case. There is no `weaver-state/none`
  # feature to look for, so comparing would refuse every agent that elects it.
  [ "$elected" = none ] && continue
  case ",$FEATURES," in
    *",weaver-state/$elected,"*) ;;
    *) die "$agent elects the $elected store and the build carries $FEATURES, so the member would refuse it at load. Name weaver-state/$elected in FEATURES, or change the declaration." ;;
  esac
done
printf '  elected store every agent under the base elects one this build carries\n'

# **Every file sink's trace stands as the territory lays it out**, root's,
# grouped `weaver-<agent>-trace`, 0640, which the admin this installs checks at
# every load (#62). An admin before #62 recreated a lost trace root:root, and
# `validate` never opens the sink, so such a trace is found here, before the
# build, with the command that re-lays it, rather than at the verify step's
# load after the install (Codex on #82). Asked of the file itself, as the
# operator, who passes the territory by its group.
for agent in $AGENTS; do
  decl=$(declaration_of "$agent") || exit 1
  rc=0
  kind=$(declared "$decl" trace-sink.kind string) || rc=$?
  [ "$rc" -eq 0 ] && [ "$kind" = file ] || continue
  sink=$(declared "$decl" trace-sink.path string) || die "$agent: the declaration's trace-sink.path does not read"
  [ -e "$sink" ] || [ -L "$sink" ] || continue
  # The type by predicate, never by `%F`'s words, which call a zero-byte file
  # "regular empty file" (Codex on #82).
  t_type="regular file"
  { [ -f "$sink" ] && [ ! -L "$sink" ]; } || t_type="not a regular file"
  read -r t_owner t_group t_mode < <(stat -c '%u %G %a' -- "$sink" 2>/dev/null) \
    || die "$agent: cannot read its trace $sink as $OPERATOR_NAME, so whether it stands as #62 requires is unknown"
  if [ "$t_type" != "regular file" ] || [ "$t_owner" != 0 ] || [ "$t_group" != "weaver-$agent-trace" ] || [ "$t_mode" != 640 ]; then
    die "$agent: its trace $sink stands as uid $t_owner, group $t_group, mode $t_mode ($t_type), and the admin this installs refuses a trace not root's, grouped weaver-$agent-trace and 0640 (#62). Re-lay it first: sudo chgrp weaver-$agent-trace $sink && sudo chmod 0640 $sink (deploy/REDEPLOY.md section 8)"
  fi
done
printf '  traces        every file sink stands as the territory lays it out\n'

# --------------------------------------------------------------- 2. update main
say "tree"
# **A failed refresh is not a stale-but-fine refresh.** Suppressing it would
# fast-forward to whatever origin/main last said and then report that commit
# as the box's, which is the kind of quiet staleness this script exists to
# remove rather than introduce.
git remote update origin >/dev/null || die "cannot refresh origin; refusing to build against a stale origin/main"
BEFORE=$(git rev-parse --short HEAD)
BRANCH=$(git branch --show-current)
if [ "$BRANCH" = "main" ]; then
  git merge --ff-only origin/main >/dev/null 2>&1 || die "main will not fast-forward; resolve by hand"
else
  printf '  on branch %s, not fast-forwarding\n' "$BRANCH"
fi
AFTER=$(git rev-parse --short HEAD)
printf '  %s -> %s\n' "$BEFORE" "$AFTER"
# **The run checks with the code it installs** (Codex on #82): a fast-forward
# that moved HEAD has changed this very script, and the checks above ran from
# the copy that started, so the fetched copy runs the whole plan again, once.
# The very first upgrade to a script carrying this re-execution runs the old
# copy; REDEPLOY.md section 8 has the operator pull main first for that reason.
if [ "$BEFORE" != "$AFTER" ] && [ -z "${WEAVER_UPDATE_REEXECUTED:-}" ]; then
  printf '  the tree moved, so the fetched update-stack.sh runs the plan again\n'
  exec env WEAVER_UPDATE_REEXECUTED=1 bash "$REPO/deploy/update-stack.sh" "${ORIGINAL_ARGS[@]}"
fi
# **A commit names what is installed only if the tree matches it.** Any
# uncommitted edit, a hand-changed source or a lock cargo repaired on its way
# past, installs under this commit's name and the closing line says the box
# is current at something it is not.
# **An untracked file is an uncommitted edit too.** `git diff-index` compares
# the commit against what git already tracks and never sees a new file, so a
# source dropped in beside the others passed this gate. The porcelain status
# reports it, leaves ignored files alone, and refreshes the index on its way
# past, which also retires the stat-dirty false refusal the old form could
# give after a checkout.
# **A gate that cannot read the tree refuses rather than passing.** Inside a
# test the substitution's own failure is not the test's status, so a git that
# answered nothing at all, a corrupt index among the reasons, read as an
# empty status and the gate said clean. Measured: a truncated `.git/index`
# exits 128 and the old form passed the run through. The answer is taken
# first, so a git that could not speak is its own refusal.
TREE_STATUS=$(git status --porcelain=v1 --untracked-files=all) \
  || die "git could not say whether the tree is clean, so $AFTER cannot be trusted to name the build"
[ -z "$TREE_STATUS" ] \
  || die "the tree is dirty, so $AFTER would name a build it did not produce; commit or stash first"
# -------------------------------------------------------------------- 3. test
say "test"
# **The test runs before the build, so it cannot overwrite what the build
# produced.** It takes a narrower feature set by necessity, `weaver-spu` not
# being among its selected packages, and a narrower set resolves features
# differently and recompiles the harness. Run after the build it rewrote both
# worker binaries, so what the plan compared and the install copied was not
# what the recorded build command made. Measured: both came back carrying
# this step's timestamp and a digest other than the build's.
# **`weaver-state` is selected here now**, on the review of 2026-09-13. It was
# not, so the shipped store member first compiled at the build below and its
# postgres engine carried no test at all against sqlite's thirteen. Selecting
# the package is also what lets the step name a `weaver-state` feature, the
# refusal that kept it out being about selection rather than about the flag.
# **The selection is bootstrap-stack.sh's and the two move together.**
# `weaver-analysis` left the workspace on 2026-09-30 and cargo refuses a
# package it does not hold, so it is not selected; a replay that needs the
# analysis binary takes it from WeaverAnalysis's own build.
cargo test --release --locked \
  -p weaver-trace -p weaver-harness -p weaver-state \
  --features "$MEMBER_FEATURES" 2>&1 | grep -E '^test result' | \
  awk '{p+=$4; f+=$6} END {printf "  %d passed, %d failed\n", p, f; exit (f>0)}'

# ------------------------------------------------------------------- 4. build
# The frontend left the repository on 2026-09-26 and ships no member here, so
# the build is the whole workspace: the seven installed members and the tools
# beside them.
say "build"
NVCC_CCBIN=${NVCC_CCBIN:-/usr/bin/g++-15} \
  cargo build --release --locked --workspace --features "$FEATURES"
printf '  ok\n'

# --------------------------------------------------------------------- 5. plan
say "plan"
CHANGED=()
for b in $MEMBERS; do
  [ -f "$BUILT/$b" ] \
    || die "the build produced no $b, so the plan cannot speak for it. Check that $b is still a bin target of this workspace."
  n=$(sha256sum "$BUILT/$b" | cut -d' ' -f1)
  # **A member the box has never held is a change, not an absence.** The
  # earlier form walked `$BIN_DIR`, so a member the build made and the box had
  # never installed was invisible: the loop skipped it, `CHANGED` never held
  # it, and the run rolled back an install that could not have contained it.
  # The member this act ships is exactly the one a box without it could not
  # have been given.
  if [ ! -f "$BIN_DIR/$b" ]; then
    printf '  %-22s NEW      %s\n' "$b" "${n:0:12}"
    CHANGED+=("$b")
    continue
  fi
  d=$(sha256sum "$BIN_DIR/$b" | cut -d' ' -f1)
  if [ "$d" = "$n" ]; then
    printf '  %-22s unchanged\n' "$b"
  else
    printf '  %-22s CHANGED  %s -> %s\n' "$b" "${d:0:12}" "${n:0:12}"
    CHANGED+=("$b")
  fi
done
# **Binaries current is not the same as the box being finished.** A run that
# installed and then died before reconciling leaves every digest matching
# and an agent that will not load, so an early exit here would refuse to
# repair exactly the state a failed run leaves behind. Only a plan run stops
# on this.
if [ ${#CHANGED[@]} -eq 0 ]; then
  printf '  every deployed binary already matches the build\n'
  if [ "$INSTALL" -eq 0 ]; then
    say "the box is current at $AFTER"
    exit 0
  fi
fi

# ------------------------------------------------ 6. what the install implies
# **The order here was wrong once and the deadlock is worth naming.** As of
# #420 an election other than `none` requires the member's binary beside the
# worker's, and an absent `state-store` key means the default, which is the
# embedded engine and not `none`. So a declaration that never mentioned the
# store stops loading the moment that admin lands. But the *old* admin
# refuses `state-store` as an unknown field, so the fix cannot be applied
# before the install that needs it.
#
# The resolution is to stop predicting. Install, then ask the admin that is
# actually running, agent by agent, and reconcile what it refuses. `validate`
# is the oracle and it is cheap, so a later schema change gets the same
# treatment without this script having to know about it in advance.
say "what the install implies"
STATE_BINARY="$BIN_DIR/weaver-state"
if [ -f "$STATE_BINARY" ]; then
  printf '  weaver-state present: declarations may elect any engine\n'
else
  printf '  weaver-state absent: a declaration electing a store will be reconciled\n'
  printf '  to `state-store: engine: none` after the install, backed up first\n'
fi

[ "$INSTALL" -eq 1 ] || { say "plan only. rerun with --install"; exit 0; }

# **Credentials first, on every install path.** Reconcile and verify run admin
# with `sudo -n` as the operator whether or not a binary changed, and no rule
# grants the operator's account those lines without a password (the
# connector's rule names its own user), so a repair run that installs nothing
# needs the credential as much as one that installs everything (Codex on #79).
sudo -v || die "--install needs sudo: reconcile and verify run admin as root"
refuse_legacy_units

# ------------------------------------------------------------------ 7. install
PATCHED=()
ADDED=()
# The agent the verify step has loaded right now, empty whenever none is. Every
# rollback from inside that step happens with a worker running, and a restore
# that leaves it running puts the old declaration and the old binaries under a
# live agent that came up on neither.
LOADED_AGENT=""
COMPLETED=0
RESTORED=0
INSTALL_DONE=0
BACKUP=""

# **A rollback that restores only the binaries leaves the box worse than it
# found it.** The old admin refuses `state-store` as an unknown field, so a
# declaration this script patched and did not un-patch is unloadable under
# the binaries the rollback just put back. Declarations go back first.
#
# **And only the binaries this run actually replaced.** A death inside the
# install loop leaves later members never backed up and never touched, so
# restoring them from a backup that does not hold them would fail the
# restore itself.
# **The restore is best-effort across every member, not until the first
# failure.** It is the last line of defence, and `set -e` abandoning it
# halfway leaves a box in a state neither the old stack nor the new one -
# which a mid-loop test produced: the first member back, the second left
# new, and the run reporting only the failure that stopped it.
restore() {
  [ "$RESTORED" -eq 0 ] || return 0
  RESTORED=1
  local failed=0
  # **The running agent goes down before the files move under it.** A load that
  # succeeded and then failed its read-back left a worker serving while the
  # declaration it came up on and the binaries it was exec'd from were both
  # put back, so the box ran an agent that matched nothing on disk. It is not
  # reloaded: a verified agent is left unloaded by the normal path too, and
  # bringing one up on restored binaries is a load this script was not asked
  # to perform.
  if [ -n "$LOADED_AGENT" ]; then
    printf '  unloading %s before the restore\n' "$LOADED_AGENT" >&2
    sudo -n WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$BIN_DIR/weaver-admin" \
      unload "$LOADED_AGENT" >/dev/null 2>&1 \
      || { printf '  %s WOULD NOT UNLOAD. It is still serving, and the files below go back under it. Unload it by hand before loading anything.\n' "$LOADED_AGENT" >&2; failed=1; }
    LOADED_AGENT=""
  fi
  if [ ${#PATCHED[@]} -gt 0 ]; then
    for entry in "${PATCHED[@]}"; do
      printf '  restoring declaration %s\n' "${entry%%|*}" >&2
      cp -a "${entry##*|}" "${entry%%|*}" \
        || { printf '  FAILED to restore %s\n' "${entry%%|*}" >&2; failed=1; }
    done
  fi
  if [ "$INSTALL_DONE" -eq 1 ] && [ -n "$BACKUP" ]; then
    printf '  rolling the binaries back from %s\n' "$BACKUP" >&2
    # **What the box never held is removed rather than restored.** Leaving it
    # would put this run's own half-finished work on a box the rollback claims
    # to have returned to where it started.
    for b in "${ADDED[@]}"; do
      printf '  removing %s, which the box did not hold before this run\n' "$b" >&2
      sudo rm -f "$BIN_DIR/$b" \
        || { printf '  FAILED to remove %s\n' "$b" >&2; failed=1; }
    done
    for b in "${CHANGED[@]}"; do
      # Never backed up is never replaced, so there is nothing to put back.
      # **Asked with the privilege that made it**: the backup directory is
      # root's, so an unprivileged test read every entry as absent and the
      # restore silently put nothing back.
      sudo test -f "$BACKUP/$b" || continue
      sudo install -o root -g root -m 0755 "$BACKUP/$b" "$BIN_DIR/$b" \
        || { printf '  FAILED to restore %s\n' "$b" >&2; failed=1; }
    done
  fi
  if [ "$failed" -ne 0 ]; then
    printf '  RESTORE INCOMPLETE - inspect %s and %s by hand\n' "$BIN_DIR" "$BACKUP" >&2
    printf '  and check with weaver-admin that no agent is still loaded\n' >&2
  fi
  return 0
}

rollback() {
  restore
  die "$1"
}

# **An abort that never reached `rollback` still has to restore.** The
# pipefail defect exited on `set -e`, so none of the rollback paths ran and
# the box was left with new binaries and an unloadable agent. A rollback
# reachable only from the failures this script predicted is not a rollback.
#
# **The trap is armed before the first binary moves**, not after the loop:
# a `sudo install` that fails on the fourth of six would otherwise exit with
# three replaced and nothing registered to put them back.
on_exit() {
  local rc=$?
  if [ "$COMPLETED" -eq 0 ] && { [ "$INSTALL_DONE" -eq 1 ] || [ ${#PATCHED[@]} -gt 0 ]; }; then
    printf '\n  the run did not complete (exit %d)\n' "$rc" >&2
    restore
  fi
}
trap on_exit EXIT

if [ ${#CHANGED[@]} -gt 0 ]; then
  say "install"
  # **Exclusive, not merely named.** A seconds-resolution name with
  # `mkdir -p` lets two runs in one second share a directory, and the
  # second run's copies would then be what the first run's rollback
  # restores. `mktemp -d` creates or fails.
  # Beside the bin directory, under the install prefix the stack record names.
  BACKUP=$(sudo mktemp -d "$(dirname "$BIN_DIR")/backup-$(date +%Y%m%dT%H%M%S)-XXXXXX")
  # State may change from the next line on, so the trap's condition is
  # armed before it does rather than after the loop closes.
  INSTALL_DONE=1
  for b in "${CHANGED[@]}"; do
    # **A binary the box has never held has nothing to back up**, and `cp`
    # refusing a source that is not there would abort the install mid-loop.
    # Its undo is removal rather than restoration, so it is recorded as such.
    if [ -f "$BIN_DIR/$b" ]; then
      sudo cp -a "$BIN_DIR/$b" "$BACKUP/$b"
    else
      ADDED+=("$b")
    fi
    sudo install -o root -g root -m 0755 "$BUILT/$b" "$BIN_DIR/$b"
    printf '  installed %s\n' "$b"
  done
  printf '  previous binaries kept at %s\n' "$BACKUP"
else
  say "install"
  printf '  nothing to install; continuing to reconcile and verify\n'
fi

# **Admin's answer on stdout and its cause on stderr, both kept.** Admin
# prints its answer as the last line on stdout and writes why it refused to
# stderr before it, as `boundary unverified: <cause>` or `config invalid:
# <cause>`. The answer is what a caller compares. The cause is what the
# operator reads beside a rollback, and a `tail -1` over the merged stream
# kept the one and dropped the other, so a refusal named no reason (#673,
# measured on the W4a run of 2026-09-25). Every line before the answer is
# relayed to stderr, marked as admin's.
# **A refusal is this function's answer, not its failure.** `weaver-admin`
# exits non-zero when it refuses, and under `set -e` a bare substitution
# would kill the script at the first agent that needed reconciling, which is
# every agent step 8 exists for. Measured on this box 2026-09-04: the run
# printed the step's header, installed binaries already in place, and
# stopped without reconciling, verifying, or rolling back.
admin_answer() {
  local said
  said=$(sudo -n WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$BIN_DIR/weaver-admin" "$1" "$2" 2>&1) || true
  [ -n "$said" ] || return 0
  printf '%s\n' "$said" | sed '$d' | sed 's/^/  admin: /' >&2
  printf '%s\n' "$said" | sed -n '$p'
}

validate() {
  admin_answer validate "$1"
}

# -------------------------------------------------------- 8. reconcile agents
say "reconcile declarations"
for agent in $AGENTS; do
  decl=$(declaration_of "$agent") || exit 1
  if [ ! -f "$decl" ]; then
    printf '  %-12s no declaration at %s\n' "$agent" "$decl"
    continue
  fi
  verdict=$(validate "$agent")
  if [ "$verdict" = '{"kind":"validated"}' ]; then
    printf '  %-12s validated\n' "$agent"
    continue
  fi
  # The one reconciliation this script knows how to make, and only where the
  # box cannot stand a leg at all. Anything else is the operator's.
  rc=0
  declared "$decl" state-store table || rc=$?
  if [ ! -f "$STATE_BINARY" ] && [ "$rc" -eq 3 ]; then
    printf '  %-12s %s\n' "$agent" "$verdict"
    # **The declaration is the operator's, so the operator patches it**,
    # without privilege, in the operator's own closed directory: no root step
    # writes a file another principal could choose.
    cp -a "$decl" "$decl.pre-$AFTER-bak"
    PATCHED+=("$decl|$decl.pre-$AFTER-bak")
    printf '\n[state-store]\nengine = "none"\n' >> "$decl"
    verdict=$(validate "$agent")
    if [ "$verdict" != '{"kind":"validated"}' ]; then
      rollback "$agent still refuses after the declaration: $verdict"
    fi
    printf '  %-12s declared `[state-store] engine = "none"`, validated (backup %s)\n' \
      "$agent" "$(basename "$decl.pre-$AFTER-bak")"
  else
    rollback "$agent refuses and this script will not guess the fix: $verdict"
  fi
done

# **An agent that has never loaded has no sink yet**, so a count taken before
# the load is a count of a file that is about to exist. The declaration says
# `create: true` and admin makes it at the load. Read as zero rather than as
# an error, the comparison below is unchanged for an agent that has run and
# is stronger for one that has not, because every line it then holds is new.
# Both sides of the comparison read through here, so a load that failed to
# make the sink at all measures no growth and is caught rather than excused.
# **A sink that is there but is not a file is a different answer from an
# absent one.** The trace sink elects one of three kinds and the other two
# name a fifo and a socket, neither of which a line count reads: counted as
# empty they would refuse a load that in fact wrote, and read with `wc` a
# fifo would block until something closed it. This answers the caller rather
# than exiting, because both calls sit inside a command substitution where an
# exit would leave only the subshell and the install standing.
sink_lines() {
  if [ ! -e "$1" ]; then printf '0\n'; return 0; fi
  [ -f "$1" ] || return 1
  wc -l < "$1"
}

# -------------------------------------------------------------------- 9. verify
# A load that is not read back is an install that was not verified. This reads
# the event out of the agent's own sink, the only place the claim can be
# checked from.
# **Every agent root under the base, not the first one.** Step 8 reconciles
# each of them, so verifying one and reporting the box current would leave
# the others' declarations changed and never loaded.
say "verify"
VERIFIED=0
for AGENT in $AGENTS; do
  decl=$(declaration_of "$AGENT") || exit 1
  if [ ! -f "$decl" ]; then
    printf '  %-12s no declaration, not verified\n' "$AGENT"
    continue
  fi
  rc=0
  SINK=$(declared "$decl" trace-sink.path string) || rc=$?
  [ "$rc" -eq 0 ] && [ -n "$SINK" ] || rollback "cannot find the trace sink for $AGENT"
  printf '  %s\n' "$AGENT"
  LINES=$(sink_lines "$SINK") || rollback "$AGENT: $SINK is not a regular file, and this step reads the load event back out of one"
  sudo -n WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$BIN_DIR/weaver-admin" unload "$AGENT" >/dev/null 2>&1 || true
  # Claimed before the load rather than after it, so a load that comes up and
  # then dies on its read-back is still a load the restore knows to undo.
  LOADED_AGENT="$AGENT"
  admin_answer load "$AGENT"
  LATER=$(sink_lines "$SINK") || rollback "$AGENT: $SINK is not a regular file, and this step reads the load event back out of one"
  NEW=$(( LATER - LINES ))
  # **A sink that gained nothing points at the sink, and the fault is rarely
  # there.** A member built without the elected engine answers
  # `descriptors_unusable` and writes no load event, so this refusal named the
  # trace sink while the real fault sat in the territory's `state.log`, which
  # is where the first look at that incident did not go. The refusal carries
  # the member's own last words rather than sending the reader to the wrong
  # subsystem. Naming the fault properly is the admin-harness contract's act,
  # not this script's; pointing at where it is already written is this one's.
  if [ "$NEW" -le 0 ]; then
    # Read as the member, whose territory it is, never as root: the member can
    # put a link at that name, and root's read would follow it anywhere.
    said=$(sudo -n -u "weaver-$AGENT-state" tail -n 3 "$(dirname "$SINK")/state/state.log" 2>/dev/null || true)
    [ -n "$said" ] && printf '  the state member last said:\n%s\n' "$said" >&2
    rollback "$AGENT: the load wrote no events to $SINK"
  fi
  if ! tail -n "$NEW" "$SINK" | weaver_read_load; then
    rollback "$AGENT: the load event does not name its composer; the install did not take"
  fi
  sudo -n WEAVER_ADMIN_CONFIG="$ADMIN_BASE" "$BIN_DIR/weaver-admin" unload "$AGENT" >/dev/null 2>&1 || true
  LOADED_AGENT=""
  VERIFIED=$((VERIFIED + 1))
done
[ "$VERIFIED" -gt 0 ] || rollback "no agent root under $ADMIN_BASE could be verified"

COMPLETED=1
say "the box is at $AFTER"
