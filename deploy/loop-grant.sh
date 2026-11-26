# deploy/loop-grant.sh: the loop runner's folder and its grant file, sourced by
# bootstrap-stack.sh and update-stack.sh (weaver-harness-Spec section 2.5).
#
# **The grant is computed on the machine that runs the loop**, as the operator,
# from the resolved interpreter, its loader and the libraries the loader
# resolves for it, every link resolved to its file, and the interpreter's
# standard library. The launcher reads it at each load and judges every path in
# it, so a grant written here and laid stale refuses the load rather than
# starting a runner with a wall missing. The enter carries none of it, and
# admin does not read it.
#
# The runner's folder is <prefix>/lib/weaver-loop/, root:root 0755, holding
# runner.py, weaver_board.py and grant, each root:root 0644. The pycache folder
# is <prefix>/lib/weaver-loop-pycache/, root:root 0755 and empty, outside the
# runner's folder: the runner starts with -B and writes no bytecode there. No
# __pycache__ ships beside the runner, and one standing is refused.

LOOP_FILES="runner.py weaver_board.py"

# loop_paths PREFIX: sets LOOP_DIR and LOOP_PYCACHE under PREFIX.
loop_paths() {
  LOOP_DIR="$1/lib/weaver-loop"
  LOOP_PYCACHE="$1/lib/weaver-loop-pycache"
}

# loop_grant INTERPRETER PYCACHE: the grant file's text on standard output, or
# a refusal on standard error and 1. Nothing privileged is asked.
loop_grant() {
  local named=$1 pycache=$2 interp said rc=0 line path real sha stdlib
  local -a execs=() reads=()
  local -A seen=()
  interp=$(readlink -f -- "$named") \
    || { printf 'the loop interpreter %s does not resolve\n' "$named" >&2; return 1; }
  [ -f "$interp" ] && [ -x "$interp" ] \
    || { printf 'the loop interpreter %s resolves to %s, which is not an executable file\n' "$named" "$interp" >&2; return 1; }
  # **The runner needs Python 3.12 or later**, and says so at a load; asked
  # here as well, so an install on an older interpreter refuses before it lays
  # a grant no load can use.
  "$interp" -I -S -c 'import sys; sys.exit(0 if sys.version_info >= (3, 12) else 1)' \
    || { printf 'the loop interpreter %s is older than Python 3.12, which the runner needs\n' "$interp" >&2; return 1; }
  said=$(ldd -- "$interp" 2>&1) || rc=$?
  [ "$rc" -eq 0 ] \
    || { printf 'ldd could not read the loop interpreter %s (it exited %s): %s\n' "$interp" "$rc" "$said" >&2; return 1; }
  if [[ "$said" == *"not found"* ]]; then
    printf 'the loop interpreter %s has a library the loader does not find:\n%s\n' "$interp" "$said" >&2
    return 1
  fi
  # **A library is read; the loader is executed and read** (the line that
  # starts with its own path). The vdso has no path and is not a file.
  while IFS= read -r line; do
    line=${line#"${line%%[![:space:]]*}"}
    case $line in
      *' => '*) path=${line#* => }; path=${path%% (*} ;;
      /*) path=${line%% (*} ;;
      *) continue ;;
    esac
    real=$(readlink -f -- "$path") && [ -f "$real" ] \
      || { printf 'the loop interpreter'"'"'s library %s does not resolve to a file\n' "$path" >&2; return 1; }
    # **No path stands twice** in a grant: one file reached by two names is
    # granted once, under its first role.
    [ -z "${seen[$real]+x}" ] || continue
    seen[$real]=1
    case $line in
      *' => '*) reads+=("$real") ;;
      *) execs+=("$real") ;;
    esac
  done <<<"$said"
  [ ${#execs[@]} -gt 0 ] \
    || { printf 'ldd names no loader for the loop interpreter %s\n' "$interp" >&2; return 1; }
  stdlib=$("$interp" -I -S -c 'import os; print(os.path.dirname(os.__file__))') \
    || { printf 'the loop interpreter %s does not name its standard library\n' "$interp" >&2; return 1; }
  stdlib=$(readlink -f -- "$stdlib") && [ -d "$stdlib" ] \
    || { printf 'the loop interpreter'"'"'s standard library does not resolve to a directory\n' >&2; return 1; }
  sha=$(sha256sum <"$interp") || { printf 'the loop interpreter %s does not read\n' "$interp" >&2; return 1; }
  sha=${sha%% *}
  printf 'weaver-loop-grant 1\ninterpreter %s %s\n' "$sha" "$interp"
  for path in "${execs[@]}"; do printf 'execute %s\n' "$path"; done
  for path in "${reads[@]}"; do printf 'read %s\n' "$path"; done
  printf 'beneath %s\npycache %s\n' "$stdlib" "$pycache"
}

# loop_standing_refusal: a refusal on standard output where LOOP_DIR or
# LOOP_PYCACHE stands out of its shape, nothing otherwise. Read without
# privilege: both are 0755 where they stand.
loop_standing_refusal() {
  if [ -e "$LOOP_DIR/__pycache__" ] || [ -L "$LOOP_DIR/__pycache__" ]; then
    printf '%s stands, and no bytecode ships beside the runner: remove it by hand, then rerun' "$LOOP_DIR/__pycache__"
    return 0
  fi
  if [ -L "$LOOP_DIR" ] || { [ -e "$LOOP_DIR" ] && [ ! -d "$LOOP_DIR" ]; }; then
    printf '%s stands and is not a directory' "$LOOP_DIR"
    return 0
  fi
  if [ -L "$LOOP_PYCACHE" ] || { [ -e "$LOOP_PYCACHE" ] && [ ! -d "$LOOP_PYCACHE" ]; }; then
    printf '%s stands and is not a directory' "$LOOP_PYCACHE"
    return 0
  fi
  if [ -d "$LOOP_PYCACHE" ] && [ -n "$(ls -A -- "$LOOP_PYCACHE")" ]; then
    printf '%s is not empty, and the launcher refuses a pycache folder that holds anything: empty it by hand, then rerun' "$LOOP_PYCACHE"
    return 0
  fi
}

# lay_loop SOURCE GRANT: lays LOOP_DIR and LOOP_PYCACHE, the runner's files
# from SOURCE (the repository's loop-runner/) and the grant from the text
# GRANT, which loop_grant computed. **Each file is staged beside its name
# and renamed over it**, root's from the moment it has a name, so the launcher
# reads the old file or the new one and never a part.
lay_loop() {
  local source=$1 grant=$2 f
  sudo install -d -o root -g root -m 0755 "$LOOP_DIR"
  sudo install -d -o root -g root -m 0755 "$LOOP_PYCACHE"
  for f in $LOOP_FILES; do
    sudo install -o root -g root -m 0644 "$source/$f" "$LOOP_DIR/.$f.new"
    sudo mv -T -- "$LOOP_DIR/.$f.new" "$LOOP_DIR/$f"
  done
  printf '%s\n' "$grant" | sudo tee "$LOOP_DIR/.grant.new" >/dev/null
  sudo chown root:root "$LOOP_DIR/.grant.new"
  sudo chmod 0644 "$LOOP_DIR/.grant.new"
  sudo mv -T -- "$LOOP_DIR/.grant.new" "$LOOP_DIR/grant"
}
