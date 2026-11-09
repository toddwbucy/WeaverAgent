#!/usr/bin/env bash
# Sourced by bootstrap-stack.sh and update-stack.sh, never by an agent.
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
#   member.<name>         sha256 of each installed member, in full
#   library.<file>        sha256 of each installed engine library's versioned file
#   driver                the NVIDIA driver version
#   cudart, cublas, cublaslt  the CUDA runtime and cuBLAS files the installed SPU resolves
#   cccl                  the cccl package version
#   gpus                  how many GPUs the driver lists
#   gpu.<n>               each GPU in PCI bus order: bus id, name, memory
#
# The agent-side half (the artifact, its template, an adapter, the devices a
# declaration elects and their split) is not the install's to state.
# Driver queries and library lookups are read whole and then searched, never
# through a consumer that stops early under `pipefail`. The runtime files are
# read through `spu_ldd` (deploy/engine-libraries.sh, which the sourcing
# script sources first), as admin's start step resolves them.

# install_tuple REPO COMMIT BIN_DIR LIB_DIR FEATURES "MEMBER..." "LIBRARY..."
# COMMIT is the tree the script built from, read at its tree gate, never
# again here: the checkout may move during a long build.
install_tuple() {
  local repo=$1 commit=$2 bin_dir=$3 lib_dir=$4 features=$5 members=$6 libraries=$7
  local host metadata engine smi ldd_said m f name path n
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
  for m in $members; do
    n=$(sha256sum -- "$bin_dir/$m" 2>/dev/null | cut -d' ' -f1)
    printf 'member.%s = %s\n' "$m" "${n:-unknown}"
  done
  for f in $libraries; do
    n=$(sha256sum -- "$lib_dir/$f" 2>/dev/null | cut -d' ' -f1)
    printf 'library.%s = %s\n' "$f" "${n:-unknown}"
  done
  smi=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null || true)
  n=$(printf '%s\n' "$smi" | sed -n 1p)
  printf 'driver = %s\n' "${n:-unknown}"
  # What the installed SPU resolves, each name followed to the file it names,
  # whose versioned name carries the runtime's version. The path is read
  # whole, from the arrow to the last load address, as engine_resolved reads
  # it, since a directory may hold a space; a library not found has no load
  # address and reads `unknown`.
  ldd_said=$(spu_ldd "$bin_dir/weaver-spu" "$lib_dir" || true)
  for name in cudart cublas cublasLt; do
    path=$(printf '%s\n' "$ldd_said" | awk -v want="lib$name.so" 'index($1, want) == 1 && substr($1, length(want) + 1, 1) ~ /^(\.|)$/ && $2 == "=>" {
      line = $0
      sub(/^[[:space:]]*[^[:space:]]+[[:space:]]+=>[[:space:]]+/, "", line)
      if (sub(/[[:space:]]+\(0x[0-9a-fA-F]+\)[[:space:]]*$/, "", line)) print line
      exit
    }')
    [ -z "$path" ] || path=$(readlink -f -- "$path" 2>/dev/null || printf '%s' "$path")
    printf '%s = %s\n' "$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]')" "${path:-unknown}"
  done
  n=$(pacman -Q cccl 2>/dev/null || true)
  n=$(printf '%s\n' "$n" | awk '{print $2; exit}')
  printf 'cccl = %s\n' "${n:-unknown}"
  # The GPUs in PCI bus order, so a declaration's device ordinals can be read
  # against the box. Sorted here rather than trusted to the query's order.
  smi=$(nvidia-smi --query-gpu=pci.bus_id,name,memory.total --format=csv,noheader 2>/dev/null || true)
  smi=$(printf '%s\n' "$smi" | grep -v '^[[:space:]]*$' | LC_ALL=C sort || true)
  printf 'gpus = %s\n' "$(printf '%s\n' "$smi" | grep -c . || true)"
  n=0
  while IFS= read -r m; do
    [ -n "$m" ] || continue
    printf 'gpu.%s = %s\n' "$n" "$m"
    n=$((n + 1))
  done <<<"$smi"
}
