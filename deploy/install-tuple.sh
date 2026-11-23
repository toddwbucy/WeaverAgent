#!/usr/bin/env bash
# Sourced by bootstrap-stack.sh and update-stack.sh, never by an agent, after
# deploy/engine-libraries.sh, whose spu_ldd and toolkit_top it calls.
#
# **The install tuple: the box-side half of the deployment tuple**, stated
# after an install so every live run's report can cite one record of what
# the box was built from and runs on. It is printed under `== tuple` and
# written to the stack record as `install-tuple`, the scripts' record that
# admin never reads and create-agent.sh does not copy (it copies named keys
# only). One `key = value` line per fact, in a fixed order; a fact this box
# cannot answer reads `unknown` rather than being left out, so the keys are
# the same on every box:
#
#   commit                the tree the install was built from, in full, as the
#                         script's tree gate read it before the build (bootstrap
#                         marks a dirty tree -dirty; update-stack refuses one)
#   toolchain             rust-toolchain.toml's channel
#   features              the features the build carried
#   engine                the engine crate and version the lock pins
#   engine-source         its lock source, the fork and the commit it names
#   engine-vendored       the llama.cpp commit that fork commit vendors
#   nvcc                  the CUDA compiler release that built the kernels
#   ccbin                 the host compiler nvcc built them with, NVCC_CCBIN,
#                         as `<path> <its --version line>`
#   member.<name>         sha256 of each installed member, in full
#   library.<file>        sha256 of each installed engine library's versioned file
#   runtime.<name>        each CUDA runtime file the installed SPU resolves, as
#                         `<sha256> <path>`, the path the file the loader's name
#                         leads to: cudart, cublas, cublaslt and curand
#   cccl                  the CCCL version the build compiled against, as
#                         `<major>.<minor>.<patch>`, read from the CCCL header
#                         nvcc reads first, by no package manager (cccl_read
#                         below)
#
# **Build facts only** (the operator's rulings of 2026-11-14: one appliance
# image serves any host's cards). Nothing here reads the hardware: no driver,
# no card count, no card. An image is built with no card, and an install tuple
# that read them stated the build machine's cards, or nvidia-smi's failure
# sentence as a driver and a card, on every box made from it. The hardware
# half is read at each load by verify-load.sh through agent_tuple.py and
# never stored. A tuple of the earlier form, which carries `driver`, `gpus`,
# `gpu.<n>` and the runtime paths without digests, is read with those lines
# left unread until an install rewrites it.
#
# The agent-side half (the artifact, its template, an adapter, the devices a
# declaration elects and their split) is not the install's to state.
# Library lookups are read whole and then searched, never through a consumer
# that stops early under `pipefail`. Every digest is of the file read on
# standard input: GNU sha256sum escapes a file name holding a backslash or a
# newline and marks the digest field with a leading backslash, which a
# reader of 64 hex digits refuses. The runtime files are read through
# `spu_ldd` (deploy/engine-libraries.sh, which the sourcing script sources
# first), as admin's start step resolves them. The driver's own library,
# `libcuda`, is the driver's and not the build's, so it is read at each load
# with the driver and is not recorded here.

# The runtime files recorded, by the library name the SPU links. Their keys
# are the names lowered. agent_tuple.py's RUNTIME_LIBRARIES states the same
# names, and test_plans.py pins the two equal.
TUPLE_RUNTIME="cudart cublas cublasLt curand"

# install_tuple REPO COMMIT BIN_DIR LIB_DIR FEATURES "MEMBER..." "LIBRARY..."
# COMMIT is the tree the script built from, read at its tree gate, never
# again here: the checkout may move during a long build.
install_tuple() {
  local repo=$1 commit=$2 bin_dir=$3 lib_dir=$4 features=$5 members=$6 libraries=$7
  local host metadata engine ldd_said m f name path n digest
  printf 'commit = %s\n' "$commit"
  printf 'toolchain = %s\n' "$(python3 -c 'import sys, tomllib
try:
    print(tomllib.load(open(sys.argv[1], "rb"))["toolchain"]["channel"])
except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError):
    print("unknown")' "$repo/rust-toolchain.toml")"
  printf 'features = %s\n' "$features"
  # The engine as the lock pins it: crate, version, source, and the vendored
  # engine's commit as the fork's tree records it (its submodule gitlink).
  # Filtered to the host's platform, since an unfiltered resolve needs every
  # platform's packages and fails offline on a cache only a build has filled.
  host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)
  metadata=$( (cd "$repo" && cargo metadata --format-version 1 --locked --offline \
    ${host:+--filter-platform "$host"} 2>/dev/null) || true)
  engine=$(printf '%s' "$metadata" | python3 -c 'import json, sys
try:
    found = [p for p in json.load(sys.stdin).get("packages", []) if p.get("name") == "llama-cpp-sys-2"]
except ValueError:
    found = []
if len(found) == 1:
    p = found[0]
    print(p.get("version") or "unknown")
    print(p.get("source") or "unknown")
    print(p.get("manifest_path") or "")' || true)
  printf 'engine = llama-cpp-sys-2 %s\n' "$(printf '%s\n' "$engine" | sed -n 1p | grep . || echo unknown)"
  printf 'engine-source = %s\n' "$(printf '%s\n' "$engine" | sed -n 2p | grep . || echo unknown)"
  path=$(printf '%s\n' "$engine" | sed -n 3p)
  n=""
  [ -z "$path" ] || n=$(cd "$(dirname -- "$path")" 2>/dev/null && git ls-tree HEAD llama.cpp 2>/dev/null | awk '$2 == "commit" {print $3}' || true)
  printf 'engine-vendored = %s\n' "${n:-unknown}"
  n=$(nvcc --version 2>/dev/null || true)
  n=$(printf '%s\n' "$n" | sed -n 's/.*release //p' | sed -n 1p)
  printf 'nvcc = %s\n' "${n:-unknown}"
  # The host compiler as the build named it, and its version line read in
  # the C locale with anything but printable ASCII dropped, since the record
  # is ASCII and its reader refuses any other byte.
  n=unknown
  if [ -n "${NVCC_CCBIN:-}" ] && [ -f "$NVCC_CCBIN" ] && [ -x "$NVCC_CCBIN" ] &&
    [[ "$NVCC_CCBIN" != *$'\n'* ]]; then
    m=$(LC_ALL=C "$NVCC_CCBIN" --version 2>/dev/null </dev/null || true)
    m=$(printf '%s\n' "$m" | sed -n 1p | LC_ALL=C tr -cd '\40-\176')
    [ -z "$m" ] || n="$NVCC_CCBIN $m"
  fi
  printf 'ccbin = %s\n' "$n"
  for m in $members; do
    n=$(sha256sum 2>/dev/null < "$bin_dir/$m" | cut -d' ' -f1)
    printf 'member.%s = %s\n' "$m" "${n:-unknown}"
  done
  for f in $libraries; do
    n=$(sha256sum 2>/dev/null < "$lib_dir/$f" | cut -d' ' -f1)
    printf 'library.%s = %s\n' "$f" "${n:-unknown}"
  done
  # What the installed SPU resolves: the first line naming each library in
  # ldd's order, as agent_tuple.py's resolved_as takes it, followed to the
  # file it names, whose versioned name carries the runtime's version, and
  # that file's digest, so a load can tell the file it resolves from the one
  # installed beside. The path is read whole, from the arrow to the last load
  # address, as engine_resolved reads it, since a directory may hold a space;
  # a library not found has no load address, and it or a file that does not
  # hash reads `unknown`, as does a path that is not one absolute line. Each
  # awk reads its input to the end: under `pipefail` a consumer that stops
  # early fails its producer.
  ldd_said=$(spu_ldd "$bin_dir/weaver-spu" "$lib_dir" || true)
  for name in $TUPLE_RUNTIME; do
    path=$(printf '%s\n' "$ldd_said" | awk -v want="lib$name.so" '!seen && index($1, want) == 1 && substr($1, length(want) + 1, 1) ~ /^(\.|)$/ && $2 == "=>" {
      seen = 1
      line = $0
      sub(/^[[:space:]]*[^[:space:]]+[[:space:]]+=>[[:space:]]+/, "", line)
      if (sub(/[[:space:]]+\(0x[0-9a-fA-F]+\)[[:space:]]*$/, "", line)) print line
    }')
    n=unknown
    [ -z "$path" ] || path=$(readlink -f -- "$path" 2>/dev/null) || path=""
    if [[ "$path" == /* && "$path" != *$'\n'* ]]; then
      digest=$(sha256sum 2>/dev/null < "$path" | cut -d' ' -f1)
      [ -z "$digest" ] || n="$digest $path"
    fi
    printf 'runtime.%s = %s\n' "$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]')" "$n"
  done
  n=$(tuple_cccl)
  printf 'cccl = %s\n' "${n:-unknown}"
}

# CCCL's version header, under an include directory.
TUPLE_CCCL_HEADER=cuda/std/__cccl/version.h

# tuple_include_dirs FLAGS: each include directory FLAGS names, one per line
# and in order, as nvcc reads `-I DIR`, `-IDIR`, `--include-path DIR` and
# `--include-path=DIR`, each value a comma-separated list, the whole value
# split on blanks and newlines. Answers 1, naming nothing, where FLAGS hold
# what it does not rank: a quote or a backslash, a system, after or quote
# include, an options file (which may hold an include of its own), a
# pre-included header (which may be another toolkit's CCCL, read first), or
# options handed to the host compiler, which may carry their own.
tuple_include_dirs() {
  local -a words
  local w list i=0
  case $1 in
    *[\"\'\\]*) return 1 ;;
  esac
  read -r -d '' -a words <<<"$1" || true
  while [ "$i" -lt "${#words[@]}" ]; do
    w=${words[i]}
    i=$((i + 1))
    case $w in
      -I|--include-path) list=${words[i]:-}; i=$((i + 1)) ;;
      --include-path=*) list=${w#--include-path=} ;;
      -I*) list=${w#-I} ;;
      -isystem*|--system-include*|-idirafter*|-iquote*|-Xcompiler*|--compiler-options*) return 1 ;;
      --options-file*|-optf*|--pre-include*|-include*) return 1 ;;
      *) continue ;;
    esac
    printf '%s\n' "${list//,/$'\n'}"
  done
}

# tuple_first_header DIRS: the CCCL header in the first of DIRS, one per line,
# that holds one. Answers 1 where none does, and 2 where a relative directory
# comes first, since it is relative to a compiler's directory never seen here.
tuple_first_header() {
  local dir
  while IFS= read -r dir; do
    [ -n "$dir" ] || continue
    [[ "$dir" == /* ]] || return 2
    if [ -f "$dir/$TUPLE_CCCL_HEADER" ]; then
      printf '%s\n' "$dir/$TUPLE_CCCL_HEADER"
      return 0
    fi
  done <<<"$1"
  return 1
}

# **cccl_read: the CCCL version the build's nvcc reads, by no package
# manager.** One rule on every box: the version is the header's own, so a
# box whose package manager is neither pacman nor dpkg reads it as well, and
# a version that cannot be read refuses a build rather than passing it.
#
# nvcc reads NVCC_PREPEND_FLAGS ahead of a compile's own flags, then
# NVCC_APPEND_FLAGS, then its toolkit's `targets/<target>/include` and that
# directory's `cccl`, the second as a system include (its nvcc.profile). So
# the header is the first an include directory of NVCC_PREPEND_FLAGS holds.
# Else it is the toolkit's, beside the nvcc on PATH (`toolkit_top`,
# deploy/engine-libraries.sh). Of the compiles' own flags, weaver-spu's build
# script adds `$CUDA_PATH/include` (else `$CUDA_ROOT`, else /usr/local/cuda),
# and a header there that is not the toolkit's file is a CCCL the engine's
# compile does not read, so the version is not read past it. The engine's own
# flags are not read.
#
# The header defines `CCCL_VERSION` as one decimal number, major times a
# million, plus minor times a thousand, plus patch, and the other version
# macros from it (read in CCCL's tags v2.8.0 to v3.3.4 and on its main
# branch on 2026-11-23). Exactly one such definition is read.
#
# Sets CCCL_READ (`<major>.<minor>.<patch>`) and CCCL_HEADER and answers 0;
# else sets CCCL_WHY, one clause naming why the version cannot be read, and
# answers 1. Called in the caller's shell, never in a substitution, so what
# it sets stays set.
cccl_read() {
  local prepend append header built top value rc=0
  local -a targets
  CCCL_READ="" CCCL_HEADER="" CCCL_WHY=""
  if ! prepend=$(tuple_include_dirs "${NVCC_PREPEND_FLAGS:-}"); then
    CCCL_WHY="NVCC_PREPEND_FLAGS hold an include form this script does not rank"
    return 1
  fi
  header=$(tuple_first_header "$prepend") || rc=$?
  if [ "$rc" -eq 2 ]; then
    CCCL_WHY="a relative include directory comes first in NVCC_PREPEND_FLAGS"
    return 1
  fi
  if [ "$rc" -ne 0 ]; then
    if ! append=$(tuple_include_dirs "${NVCC_APPEND_FLAGS:-}"); then
      CCCL_WHY="NVCC_APPEND_FLAGS hold an include form this script does not rank"
      return 1
    fi
    rc=0
    tuple_first_header "$append" >/dev/null || rc=$?
    case $rc in
      0) CCCL_WHY="an include directory of NVCC_APPEND_FLAGS holds CCCL, and the build's own flags stand between it and the toolkit's"
         return 1 ;;
      2) CCCL_WHY="a relative include directory stands in NVCC_APPEND_FLAGS"
         return 1 ;;
    esac
    toolkit_top || { CCCL_WHY=$TOOLKIT_WHY; return 1; }
    top=$TOOLKIT_TOP
    targets=("$top"/targets/*/include)
    if [ "${#targets[@]}" -ne 1 ] || [ ! -d "${targets[0]}" ]; then
      CCCL_WHY="the toolkit at $top holds no single targets/*/include"
      return 1
    fi
    header=$(tuple_first_header "${targets[0]}"$'\n'"${targets[0]}/cccl") || {
      CCCL_WHY="the toolkit at $top holds no $TUPLE_CCCL_HEADER"
      return 1
    }
    built=${CUDA_PATH:-${CUDA_ROOT:-/usr/local/cuda}}/include
    if [[ "$built" != /* ]]; then
      CCCL_WHY="the SPU build's include directory $built is not an absolute path"
      return 1
    fi
    if [ -f "$built/$TUPLE_CCCL_HEADER" ] &&
      [ "$(readlink -f -- "$built/$TUPLE_CCCL_HEADER")" != "$(readlink -f -- "$header")" ]; then
      CCCL_WHY="the SPU build's include directory $built holds a CCCL header that is not the toolkit's"
      return 1
    fi
  fi
  # Every line is read: awk reads to the end and nothing stops it early.
  value=$(LC_ALL=C awk '
    /^[[:space:]]*#[[:space:]]*define[[:space:]]+CCCL_VERSION([[:space:]]|$)/ {
      n++
      v = $0
      sub(/^[[:space:]]*#[[:space:]]*define[[:space:]]+CCCL_VERSION[[:space:]]*/, "", v)
      sub(/[[:space:]]+$/, "", v)
    }
    END { if (n == 1) print v }' <"$header" 2>/dev/null) || value=""
  if [[ ! "$value" =~ ^[1-9][0-9]{6,8}$ ]]; then
    CCCL_WHY="the header $header defines no one CCCL_VERSION as a decimal number"
    return 1
  fi
  CCCL_HEADER=$header
  CCCL_READ="$((value / 1000000)).$((value / 1000 % 1000)).$((value % 1000))"
}

# tuple_cccl: the install tuple's cccl fact, CCCL_READ, or nothing where it
# cannot be read.
tuple_cccl() {
  if cccl_read; then printf '%s\n' "$CCCL_READ"; fi
  return 0
}

# cccl_in_window VERSION: answers 0 where VERSION lies inside the window #397
# measured, both ends included and by version order rather than by glob: a
# `3.3.*` pattern waves 3.3.5 through, which is outside what #397 measured,
# and a `3.1.4*` one turns away 3.1.5, which is inside it. Outside it the
# engine does not compile, and a refusal before the build is cheaper than one
# twenty minutes into it.
cccl_in_window() {
  local lo=3.1.4 hi=3.3.4
  [ "$(printf '%s\n%s\n' "$lo" "$1" | sort -V | head -1)" = "$lo" ] &&
  [ "$(printf '%s\n%s\n' "$1" "$hi" | sort -V | head -1)" = "$1" ]
}
