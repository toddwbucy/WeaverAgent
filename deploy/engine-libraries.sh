#!/usr/bin/env bash
# Sourced by bootstrap-stack.sh and update-stack.sh, never by an agent: the
# engine's shared libraries as both stack scripts build, select, install and
# check them, stated once.

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
# **The CUDA directory and the host compiler are named, or derived from the
# toolkit the build uses, never one distribution's path taken on trust.**
# CUDA_LIB_DIR, where the operator sets it, is the CUDA directory the loader's
# entry names; unset, `cuda_lib_dir` derives it from the toolkit the nvcc on
# PATH belongs to. NVCC_CCBIN is the host compiler nvcc builds the engine's
# kernels with; unset, it is NVCC_CCBIN_DEFAULT, and `host_compiler` refuses
# either where it names no executable file on this box, before the long
# build. weaver-spu's own kernels are not among them: its build script hands
# nvcc the C++ compiler the cc crate chose (`-ccbin`), which overrides it.
NVCC_CCBIN_DEFAULT=/usr/bin/g++-15

# toolkit_top: the CUDA toolkit the build's nvcc belongs to. The nvcc on PATH,
# followed through its links, is `<top>/bin/nvcc`. Sets TOOLKIT_TOP and
# answers 0; else sets TOOLKIT_WHY, one clause, and answers 1. Called in the
# caller's shell, never in a substitution, so what it sets stays set.
toolkit_top() {
  local path
  TOOLKIT_TOP="" TOOLKIT_WHY=""
  path=$(command -v nvcc 2>/dev/null) || { TOOLKIT_WHY="no nvcc is on PATH"; return 1; }
  path=$(readlink -f -- "$path" 2>/dev/null) || { TOOLKIT_WHY="the nvcc on PATH does not resolve"; return 1; }
  if [[ "$path" != /*/bin/nvcc || "$path" == *$'\n'* ]]; then
    TOOLKIT_WHY="the nvcc on PATH resolves to $path, which is not <toolkit>/bin/nvcc"
    return 1
  fi
  TOOLKIT_TOP=${path%/bin/nvcc}
}

# toolkit_target: that toolkit's directory for this host's own target, the
# one nvcc compiles for and reads its includes and libraries from:
# `targets/x86_64-linux` on x86_64, and on aarch64 `targets/sbsa-linux`, else
# `targets/aarch64-linux`. A toolkit may hold other targets beside it, as a
# cross-compiling package lays them, and those are not read. Sets
# TOOLKIT_TARGET and answers 0; else sets TOOLKIT_WHY and answers 1.
toolkit_target() {
  local machine target
  local -a targets
  TOOLKIT_TARGET=""
  toolkit_top || return 1
  machine=$(uname -m 2>/dev/null) || machine=""
  case $machine in
    x86_64) targets=(x86_64-linux) ;;
    aarch64) targets=(sbsa-linux aarch64-linux) ;;
    *) TOOLKIT_WHY="this host's machine, ${machine:-unread}, names no CUDA target this script knows"
       return 1 ;;
  esac
  for target in "${targets[@]}"; do
    if [ -d "$TOOLKIT_TOP/targets/$target" ]; then
      TOOLKIT_TARGET=$TOOLKIT_TOP/targets/$target
      return 0
    fi
  done
  TOOLKIT_WHY="the toolkit at $TOOLKIT_TOP holds no targets/${targets[0]} for this host"
  return 1
}

# toolkit_lib_dir: the library directory of that toolkit for this host, its
# target's `lib`. Sets TOOLKIT_LIB and answers 0; else sets TOOLKIT_WHY and
# answers 1.
toolkit_lib_dir() {
  TOOLKIT_LIB=""
  toolkit_target || return 1
  if [ ! -d "$TOOLKIT_TARGET/lib" ]; then
    TOOLKIT_WHY="the toolkit's target $TOOLKIT_TARGET holds no lib directory"
    return 1
  fi
  TOOLKIT_LIB=$TOOLKIT_TARGET/lib
}

# cuda_lib_dir: sets CUDA_LIB_DIR, the CUDA directory the loader's entry names
# beside the library directory: the operator's value where it is set, else
# the toolkit's library directory. Refuses, through the sourcing script's
# `die`, where neither answers. The directory is judged by `loader_dir`.
cuda_lib_dir() {
  [ -z "${CUDA_LIB_DIR:-}" ] || return 0
  toolkit_lib_dir \
    || die "CUDA_LIB_DIR is unset and $TOOLKIT_WHY, so the CUDA directory the loader's entry names is unknown: set CUDA_LIB_DIR"
  CUDA_LIB_DIR=$TOOLKIT_LIB
}

# printable_path VALUE: answers 0 where VALUE is printable ASCII throughout,
# spaces included, the bytes the install tuple's reader admits.
printable_path() {
  [ -n "$1" ] && [ "$(printf '%s' "$1" | LC_ALL=C tr -cd '\40-\176')" = "$1" ]
}

# host_compiler: sets and exports NVCC_CCBIN, the operator's value or the
# default, a bare name resolved on PATH as nvcc resolves one, and refuses,
# through the sourcing script's `die`, unless it is then the absolute path of
# an executable file, in printable ASCII. nvcc also takes a directory there,
# and this script does not: the install tuple names the compiler's file.
host_compiler() {
  local named=${NVCC_CCBIN:-$NVCC_CCBIN_DEFAULT} found
  if [[ "$named" != */* ]]; then
    found=$(command -v -- "$named" 2>/dev/null) || found=""
    [[ "$found" != /* ]] || named=$found
  fi
  [[ "$named" == /* ]] && [ -f "$named" ] && [ -x "$named" ] && printable_path "$named" \
    || die "NVCC_CCBIN names the host compiler nvcc builds the engine's kernels with, and $named is not an executable file on this box (the default, $NVCC_CCBIN_DEFAULT, is one distribution's path): set NVCC_CCBIN to the absolute path, or a name on PATH, of a compiler this toolkit accepts"
  NVCC_CCBIN=$named
  export NVCC_CCBIN
}

# **The engine is built without HTTPS** (weaver-spu-Spec section 1.1). Nothing
# in the agent downloads a model, and the only user of OpenSSL in the engine's
# build is llama.cpp's bundled cpp-httplib, which serves only the model
# downloader in llama.cpp's static `common` library. The fork links that
# library into the SPU, and the linker takes neither the downloader nor
# httplib from it. On 2026-11-11 that httplib (0.40.0, in the vendored
# llama.cpp at e21cdc11) stopped compiling against the box's OpenSSL 4.1.0.
# The fork's build script passes every `CMAKE_*` variable in its environment
# to cmake. The repository's `.cargo/config.toml` forces it for every cargo
# run started inside the tree, and this export is the belt beside that file:
# it turns llama.cpp's one OpenSSL lookup off for every cargo run either
# script makes, over any value the operator's shell holds.
# The build script declares no `rerun-if-env-changed` for it, so an engine a
# target configured before this rule keeps that configuration, and each script
# judges the engine's configure cache after its build
# (`engine_without_https`).
export CMAKE_DISABLE_FIND_PACKAGE_OpenSSL=ON

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

# engine_without_https OUT: refuses, through the sourcing script's `die`,
# unless the configure cache cmake left in the engine's out_dir OUT records
# the OpenSSL lookup off, in the form cmake writes a definition: the name, a
# type (UNINITIALIZED where the definition named none) and `=ON`. A cache
# that is missing, or records the lookup any other way, is an engine a build
# before the rule configured, which cargo reuses without rerunning its build
# script. A fresh target configures the engine afresh. `cargo clean -p`
# removes every build directory and fingerprint of the engine in that target
# (measured on 2026-11-11 with `--dry-run`), every checkout's where checkouts
# share the target, so the refusal names the fresh target first.
engine_without_https() {
  grep -Eqx 'CMAKE_DISABLE_FIND_PACKAGE_OpenSSL:[A-Z]+=ON' -- "$1/build/CMakeCache.txt" 2>/dev/null \
    || die "the engine's configure cache at $1/build/CMakeCache.txt does not turn its OpenSSL lookup off, so a build in this target configured it before the stack built the engine without HTTPS, and the engine's build script does not rerun when that setting changes (deploy/engine-libraries.sh). Build into a fresh CARGO_TARGET_DIR, or remove it with 'cargo clean --release -p $ENGINE_CRATE --locked', which removes every build of the engine in this target, those of every checkout sharing it included"
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
# LIBDIR. Prints each line that fails, so a refusal can show it. ldd prints
# a resolved library as `<name> => <path> (<load address>)`, and the path
# is read whole, from the arrow to the last load address, since a library
# directory may hold a space; a line with no arrow (the vDSO, the loader)
# names no engine library, and only `=> not found` at a line's end is a
# library not found. **A symbol version not found fails too**: ldd exits 0
# for it and prints `<program>: <library>: version 'X' not found (required
# by <program>)` (measured on 2026-11-23 with a library built without the
# version its program was linked against), which is how a box whose C or C++
# runtime is older than the build's meets the stack, and the program then
# fails at exec.
engine_resolved() {
  python3 -c 'import os, re, sys
lib = os.path.realpath(sys.argv[2])
resolved = re.compile(r"\s*(\S+)\s+=>\s+(.+)\s+\(0x[0-9a-fA-F]+\)\s*")
version = re.compile(r":\s+version\s+[`\x27][^`\x27]*\x27\s+not found\s+\(required by\s")
bad, engine = [], 0
for line in sys.argv[1].splitlines():
    if re.search(r"=>\s*not found\s*$", line) or version.search(line):
        bad.append(line.strip())
        continue
    name = (line.split() or [""])[0]
    if re.match(r"lib(ggml|llama)", name):
        engine += 1
        m = resolved.fullmatch(line)
        if not m or os.path.dirname(os.path.realpath(m.group(2))) != lib:
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
