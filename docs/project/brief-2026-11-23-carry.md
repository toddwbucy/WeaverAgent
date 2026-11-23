# Brief: a stack built on one box installs on another

Written by the WeaverAgent Executor on 2026-11-23, from the plan the Planner accepted
that day. A stack of draft pull requests, the base to `main` from `b74062bf`.

## Why

The stack is built and installed on one box: `deploy/bootstrap-stack.sh` compiles the
members, the engine and its kernels, then installs what it compiled.
`weaver-admin-PRD` section 7.2 says the binaries are compiled inside the image they run
in. A box that runs the agent then needs the compiler, the CUDA toolkit and the build's
whole toolchain. Two boxes that should run one build each compile their own.

A measurement installs one build on two boxes and compares their traces, so the build
has to travel as one set of bytes. Nothing the install writes may depend on which box
built the set.

Two things were measured or read while planning:

- **`ldd` passes a symbol-version mismatch.** It exits 0 and prints
  `<library>: version 'X' not found (required by <program>)`. `engine_resolved`
  (`deploy/engine-libraries.sh`) counts only a line that ends in `=> not found`. The
  resolution check therefore passes a box whose C++ runtime is older than the build's,
  and that is the box a carried stack meets.
- **Nothing reads the cccl version on a box without pacman.** `bootstrap-stack.sh` and
  `update-stack.sh` take the cccl version from `pacman -Q cccl`. Where pacman is absent,
  the version reads empty and the window check skips itself. `install-tuple.sh` falls
  back to dpkg and otherwise writes `cccl = unknown`. `NVCC_CCBIN` defaults to
  `/usr/bin/g++-15` and `CUDA_LIB_DIR` to `/opt/cuda/lib64`, which are one
  distribution's paths.

## Rulings (the operator, 2026-11-23, relayed by the Planner)

1. **The binaries are carried.** A build box compiles the stack from a named commit. A
   run box, which has no compiler and no toolkit, receives the stack and installs it.
   `weaver-admin-PRD` section 7.2 is corrected by this act.
2. **On a box an operator presents, an operator's account with sudo lays the agent.**
   This is the path `create-agent.sh` already has, without `--root-operator`. No change
   is asked there.
3. **"We copy the model into the attached storage for the VM."** The suite does the
   copy. Whether that storage is the machine's first disk or a disk that the operator
   attaches is the operator's choice. `weaver-admin-PRD` section 7.5 stands as written,
   and the models drive is out of this act.

The Planner's rulings on the plan, 2026-11-23:

- **The library path alone carries the stack on a carried box.** No loader entry is
  written and `ldconfig` is not run.
- **The CUDA runtime files that `weaver-spu` resolves on the run box equal the files the
  build resolved, by sha256, or the install refuses.** This check keeps 7.2's reason
  true: the binaries run against the libraries they were built against.
- **The build half gains `build-os`.** The load-time digests of the C and C++ runtime
  are a later act.
- **`deploy/image-programs.txt` stays** as the sealed image's record. A run box judges
  the deploy copy against its own programs when it installs.
- **`REDEPLOY.md` states facts about the licence.** It names the four runtime files
  carried, says they are carried unmodified, and says that the operator reads the
  licence text that ships with their toolkit. It makes no claim about what that licence
  allows.
- **`--carry` works on a build box with no card**, and a test holds it.
- **A box that already holds a stack refuses a second set.** The refusal names what the
  operator does to put a newer set on the box, and `REDEPLOY.md` says it.

## The stack

| Pull request | What it carries |
|---|---|
| Base | This brief. `weaver-admin-PRD` section 7.2 rewritten and 7.1's cross-reference to it. `weaver-admin-Spec` section 6: `library-path` names the CUDA runtime files the SPU resolves as well as the engine libraries. |
| First child: no silent pass without pacman | The cccl version is read from the header nvcc reads first, by no package manager, and an unreadable or out-of-window version refuses before the build in both build scripts. `NVCC_CCBIN` refuses before the build where it is not an executable file, and its path and version line join the install tuple as `ccbin`. An unset `CUDA_LIB_DIR` derives from the toolkit beside the `nvcc` on `PATH`, or refuses. `engine_resolved` refuses a version-not-found line. |
| Second child: the carry set | `bootstrap-stack.sh --carry DIR` writes the members, the shell, the engine libraries and the CUDA runtime files with their links, the deploy copy, a manifest, and the build half of the install tuple with `build-runtime.<name>` and `build-os`. It refuses a dirty tree and works with no card. |
| Third child: the install | `install-carry.sh <set> <manifest sha256> [--install]` checks the set in a staging copy that root holds and refuses on any difference. It installs and checks resolution through the library path alone. It refuses unless each runtime file equals the build's, judges the deploy copy against the box's own programs, and writes the stack record and the install tuple whole. `agent_tuple.py` binds the runtime to the build's digests. `update-stack.sh` refuses a carried box. |

## The carry set

```text
<set>/
  MANIFEST       one row per entry; its sha256 names the set
  build-tuple    the build half of the install tuple, `key = value`
  bin/           the six members, busybox, busybox.applets
  lib/           the engine libraries and the CUDA runtime files the SPU resolves,
                 each versioned file with its links
  deploy/        every file deploy/deployed-copy.txt names, the list included
```

The manifest has one entry per row, tab-separated and sorted by path:
`file<TAB><sha256><TAB><0755|0644><TAB><path>` or `link<TAB><target><TAB>-<TAB><path>`.
Every path is relative, has no `..`, and lies under `bin/`, `lib/` or `deploy/`, or is
`build-tuple`. A link's target is one name in the same directory. The set holds no
entry that the manifest does not name.

## Out

Making, powering or changing any machine; site conditions; SELinux, which waits for a
run box with cards; where the model's storage stands; a carried update
(`update-stack.sh` on a carried box); the suite repository's appliance documents.

## What waits on the operator's side

The CUDA build on a build box with a toolkit, and the install on a run box with cards.
The stack stays in draft until both have run for real.
