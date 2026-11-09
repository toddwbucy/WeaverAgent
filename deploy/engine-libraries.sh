#!/usr/bin/env bash
# Sourced by bootstrap-stack.sh and update-stack.sh, never by an agent: the
# engine's shared libraries as both stack scripts select, install and check
# them, stated once.

# **The engine's shared objects.** llama-cpp-sys-2 builds them into its own
# `out/lib` under the target's `build/` directory, each versioned file beside
# its SONAME link and its bare link; the copies at the target root are bare
# links that dangle. The versioned names carry the engine's build number, so
# a moved pin installs new names. The members carry no RUNPATH and name the
# SONAMEs, so bootstrap writes an `ld.so.conf.d` entry for the library
# directory and the CUDA directory, and the stack record's `library-path`
# names the library directory as well, which admin judges and sets as the
# worker's LD_LIBRARY_PATH (weaver-admin-Spec section 6).
ENGINE_CRATE=llama-cpp-sys-2
LIB_GLOBS='libggml*.so* libllama*.so*'
LDSO_CONF=/etc/ld.so.conf.d/weaver.conf
CUDA_LIB_DIR=${CUDA_LIB_DIR:-/opt/cuda/lib64}

# **Which engine libraries a build made is asked of cargo, never guessed from
# the target directory.** A shared target holds one
# `build/<engine crate>-<hash>/` per engine pin and configuration it has ever
# built (measured on 2026-11-09: three, from two fork revisions),
# so the newest by mtime is the one a build used only when nothing else built
# there since. Cargo reports every build script it ran or found fresh as a
# `build-script-executed` message carrying its `out_dir`, so each script runs
# its build with `--message-format=json-render-diagnostics`, its standard
# output (the messages) to a file it makes with mktemp, and asks here for the
# one message of the engine's package as `cargo pkgid` names it from the
# lock. The rendered diagnostics and the progress stay on standard error.
# `cargo pkgid` reads the lock alone, where `cargo metadata` with
# dependencies resolves every platform's packages and fails offline on a
# cache only a build has filled (measured on 2026-11-09). The directory a
# message names is taken only where it resolves inside the build's own
# `build/`, where cargo puts every build script's out_dir, so a message
# naming any other directory cannot choose what root installs.

# engine_out CRATE MESSAGES BUILD: prints, resolved, the out_dir the build at
# MESSAGES reported for the lock's one package named CRATE; prints nothing
# and answers 1 where the lock holds no single such package, the build
# reported no single out_dir for it, or that out_dir is not a directory
# inside BUILD once both are resolved.
engine_out() {
  local id
  id=$(cargo pkgid --offline --locked "$1" 2>/dev/null) || return 1
  [ -n "$id" ] || return 1
  python3 -c 'import json, os, sys
dirs = set()
with open(sys.argv[1]) as messages:
    for line in messages:
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if isinstance(m, dict) and m.get("reason") == "build-script-executed" and m.get("package_id") == sys.argv[2]:
            dirs.add(m.get("out_dir") or "")
if len(dirs) != 1 or "" in dirs:
    sys.exit(1)
out, build = os.path.realpath(dirs.pop()), os.path.realpath(sys.argv[3])
if out == build or os.path.commonpath([out, build]) != build or not os.path.isdir(out):
    sys.exit(1)
print(out)' "$2" "$id" "$3"
}

# install_library FROM TO: places one engine library as root's. A link is
# carried as a link with `cp -a --remove-destination`, then made root's
# without following it; a file is installed with `install -o root -g root`,
# a new file in place of the old, root's from the moment it has a name, so
# no other principal ever holds a writable descriptor to a file in the
# loader's path.
install_library() {
  if [ -L "$1" ]; then
    sudo cp -a --remove-destination -- "$1" "$2"
    sudo chown -h root:root -- "$2"
  else
    sudo install -o root -g root -m 0755 -- "$1" "$2"
  fi
}

# spu_ldd BIN LIBDIR: what the loader resolves for BIN as admin's start step
# runs the worker's organs, with LD_LIBRARY_PATH the library path
# (weaver-admin-Spec section 6) and none of the operator's loader variables.
spu_ldd() {
  env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT -u GLIBC_TUNABLES \
    LD_LIBRARY_PATH="$2" ldd "$1" 2>&1
}

# engine_resolved REPORT LIBDIR: answers 0 only where REPORT, spu_ldd's
# output from a run that exited 0, names no library not found, names at
# least one engine library, and resolves every engine library to a file in
# LIBDIR. Prints each line that fails, so a refusal can show it.
engine_resolved() {
  python3 -c 'import os, re, sys
lib = os.path.realpath(sys.argv[2])
bad, engine = [], 0
for line in sys.argv[1].splitlines():
    fields = line.split()
    if "not found" in line:
        bad.append(line.strip())
        continue
    if fields and re.match(r"lib(ggml|llama)", fields[0]):
        engine += 1
        if len(fields) < 3 or fields[1] != "=>" or os.path.dirname(os.path.realpath(fields[2])) != lib:
            bad.append(line.strip())
for line in bad:
    print(line)
if not engine:
    print("no engine library is named")
sys.exit(1 if bad or not engine else 0)' "$1" "$2"
}

# loader_dir NAME DIR: refuses, through the sourcing script's `die` and
# `held_closed`, unless DIR is one absolute path held closed by root. The
# loader's entry bootstrap writes is searched by every program on the box,
# root's included, so a directory it names that another principal could
# write would let that principal place a library root loads. NAME is the
# variable or key DIR came from, for the refusal.
loader_dir() {
  local bad
  [[ "$2" == /* && "$2" != *$'\n'* ]] \
    || die "$1 $2 is not one absolute path, so it cannot join the loader's entry"
  bad=$(held_closed "$2") \
    || die "$1 $2 is not held closed by root at $bad, so it cannot join the loader's entry every program on the box searches"
}
