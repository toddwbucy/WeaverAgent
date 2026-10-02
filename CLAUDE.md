# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

`WeaverAgent` is one agent of the WeaverTools suite, singular by the operator's ruling of
2026-10-02 (it was `WeaverAgents`): a ten-crate Rust workspace that builds the deployable
proto-stateful agent, plus the documents that authorize its code,
the deploy scripts that install it on a box, and a Python prototype of one organ. It was
split out of the monorepo `toddwbucy/WeaverTools-old2` on 2026-09-30 (root commit
`a9d9827`, no history on `main`; the local `seed-history` branch holds the monorepo's
history and is never pushed).

Two sibling repositories left the monorepo in the same split and are met only across a
process or a network boundary, never linked:

- `toddwbucy/WeaverAnalysis`, the analytical tools over the trace, driver and reader of
  the diagnostic record. Out of this tree's boundary; its Spec forbids any `weaver-*`
  dependency.
- `toddwbucy/WeaverWeb`, the fleet: it connects to many agents and manages them. Its
  connectors (gate-con and admin-con) run beside each agent as their own service users,
  are WeaverWeb's and never this repository's, start under the operator's provisioning
  and never under admin, and reach the agent only through its two doors. The role map
  that bounds them is the box's (toddwbucy/WeaverTools#6).

The rule that drew the line: a crate a consumer meets across a network boundary gets its
own repository; the agent keeps everything interior. The suite-level documentation
repository (`toddwbucy/WeaverTools`) holds `experiments/` and will hold the vision, the
cross-repository contracts and the process documents. Until it does, those live here.

Issue and pull request numbers in inherited code and documents (`#689`, `#551`, ...) are
monorepo numbers. Its 31 open issues were transferred to this repository on
2026-09-30 and carry new numbers there. A `toddwbucy/WeaverTools` reference in a
document or code written before 2026-09-30 means the monorepo; this file uses the name
for the suite repository.

All of these repositories were **public** on 2026-09-30. Check rather than assume:
`gh repo view toddwbucy/WeaverAgent --json visibility`.

## The deliverable

**A deployable proto-stateful agent that completes a turn end to end against a real
local model and emits a clean, turn-bracketed, correctly-custodied trace.** The trace is
the primary artifact, not a diagnostic. `docs/crates/weaver-agent-PRD.md` is the apex:
the deliverable, the five invariants, the lifecycle and the enforcement posture every
other document answers to.

**Proto-stateful, not stateless** (PRD section 2, on the ruling of 2026-08-01). The
agent holds real state within a session and none across sessions. Two things hold state
across turns inside one session, deliberately and not two of a kind: the working
structure (the run's trace events held in RAM in the canonical form the stream carries)
and the hot KV cache (an optimization whose owner, flush trigger and forbidden touchers
`weaver-spu-PRD` names). "Stateless" anywhere outside a record of the rename is stale.

**Correctly custodied** means the agent cannot reach its own record: the stream lands
behind a boundary the kernel enforces.

## Architecture

A deployed agent is four processes on one machine, and its own `weaver-admin`, the
agent's lifecycle driver and management plane, which runs per verb as root and is not
resident while the agent serves. Every organ is one agent's own: a second agent gets its
own set, and managing several agents belongs to WeaverWeb or a separate application, not
to admin (operator's ruling of 2026-10-01). Every seam that crosses a process
line is a Unix domain socket, and there is no listening network socket anywhere.

```text
                weaver-admin  (the agent's management plane: loads, unloads; one unit)
                      |
   world --> weaver-gate --> [ worker: weaver-harness + weaver-trace + weaver-diagnostic ]
                                   |                 |
                              weaver-spu        weaver-state
                            (the decoder)     (session store)
```

- **`weaver-harness`** is the content-neutral switchboard: it holds the sockets, routes
  between organs, authors the trace, and holds no opinion about content. Its
  `src/bin/worker` is the composition root that becomes a systemd unit;
  `src/bin/pyworker` (feature `pyworker`, links pyo3) runs a loop written in Python.
  Loops are workflow documents under `docs/crates/weaver-harness/Loops/`, not code in
  the switchboard.
- **`weaver-trace`** writes the record, one event per line in canonical form, and tees
  it across the custody boundary. **`weaver-diagnostic`** is the harness's third member
  and writes the diagnostic-trace (residual readouts) beside it.
- **`weaver-spu`** holds the model. One binary the harness forks and execs; the seam is a
  socket, and nothing links the crate. A second bin, `weaver-spu-classify`, sits behind
  the `cuda` feature (Spec section 11). Backends: `gguf` (llama.cpp,
  on by default, builds the C++ toolchain) and `cuda` (candle plus the crate's own
  kernels under `kernels/`; `build.rs` compiles them only when the feature is on).
  `kernels/PROVENANCE.md` records what hardware has run what.
- **`weaver-gate`** is the world-facing organ: it admits a dialer by uid, relays one JSON
  line in and one out, and holds the tool hooks.
- **`weaver-state`** is the session store (`sqlite` default, `postgres` optional).
- **`weaver-admin`** is one agent's organ, invoked by the operator per verb: it reads only
  that agent's config root `<base>/<agent>/` (base from `WEAVER_ADMIN_CONFIG`, default
  `/etc/weaver/admin`), stands its unit, opens its trace sink and hands it to the worker,
  and names its SPU by that root's `spu-binary`.
- **`weaver-internal`** holds internal tools that run inside the loop (the calculator).
  A tool that binds a listening port is external and reaches the agent through the gate;
  one that does not is internal.
- **`weaver-traits`** and **`weaver-types`** are the floor: messages, providers, tools,
  permissions, identities, the wire and the agent declaration parser (`config` feature).
  They compile on stable and are demand-derived from what the organs need.

Cargo edges run downward only: every crate links at most the floor, the harness links
its two members, `weaver-state` links `weaver-trace`, and no organ links another organ.
Each crate's `tests/manifest.rs` reads its own manifest to pin its part of that shape.

### Where the documents are

`docs/crates/<crate>/` holds each crate's PRD (charter) and Spec, mirroring
`crates/<crate>/`, except that `weaver-trace`, `weaver-diagnostic` and `weaver-state`
sit under `docs/crates/weaver-harness/`. Every seam has a contract under
`docs/crates/contracts/`; four of them are cross-repository (`weaver-gate-world`,
`weaver-admin-operator`, `weaver-analysis-state`, `weaver-analysis-web`) and are
destined for the suite repository. `docs/crates/weaver-analysis/` and the two
`weaver-analysis-*` contracts are byte-identical copies of files in WeaverAnalysis;
which copy is authoritative is unruled, so edit neither without saying so.

`docs/technical/` is a dated, non-normative reader's snapshot (one page per crate,
`index.md` first). `docs/project/` holds handoffs, audits, sketches and the vision; the
most recent handoffs are the fastest way to learn where the work stands.

`docs/crates/weaver-spu/python-spu-Spec.md` governs `python-spu/`, a PyTorch prototype
of the SPU's decode role carried as a lab instrument (a carry is not a hardening). It
has its own README, lock files and an `oracle/` crate that links this workspace.

## What governs the work

**The four process documents in `process/` outrank this file**:
`WeaverTools-Working-Process` (phases, seats, gates, current position),
`WeaverTools-Document-Format`, `WeaverTools-Working-Rules` (editorial),
`WeaverTools-Handoff-Format`. They change often; read the file, not a remembered version.

The program is in **phase three, coding**, under gates H1-H5 of Working Process
section 6. **A merged Spec authorizes code** (H1), and where code and a Spec disagree
the act changes whichever is wrong in the same act, contracts included and reaching
every party. A new capability is a Spec gap and the operator's ruling.

**The knowledge graph, HADES, ratification, the census and assertion records are all
suspended until release**, on the operator's ruling of 2026-09-27. Existing
`//! conforms:` headers stay unmaintained, new code owes none, and `.hadesignore` and
`process/ingest/chunk_plan.py` are dormant. **The conformance check meanwhile is review
against the Spec sections a pull request names.**

## Commands

Toolchain `nightly-2026-02-13` with rustfmt and clippy, pinned in `rust-toolchain.toml`,
edition 2024. Run from the repository root. `--locked` goes on every cargo command that
resolves; without it cargo repairs the lock in place and the gate answers about a tree
the repository does not record. `fmt` resolves nothing and takes no flag.

```bash
process/gates/lock.sh                                    # first: 0 in step, 1 drift, 2 unchecked
cargo build --workspace --locked
cargo test -p weaver-harness --locked                    # one crate
cargo test -p weaver-harness --locked <name_fragment>    # one test by substring
cargo clippy -p <crate> --all-targets --locked -- -D warnings
cargo clippy -p weaver-spu --all-targets --features cuda,gguf --locked -- -D warnings
cargo fmt --all -- --check
```

`lock.sh` compiles nothing and runs in under a second. Its 0 and 1 are definitive; 2
means the gate could not run (cold cache, network needed, unreadable manifest), so the
lock is unchecked rather than clean. Its header carries the measurements.

**Workspace-wide `cargo test` does not compile yet.** Two path couplings reach
`weaver-analysis`, the crate that left:

- `crates/weaver-types/tests/config.rs:646` does `include_str!` on
  `../../weaver-analysis/tests/fixtures/derived-surrogate.toml`. The test file is
  behind `#![cfg(feature = "config")]`, so `cargo test -p weaver-types --locked` passes
  alone and fails under `--all-features` or `--workspace`, where feature unification
  turns `config` on and stops the whole run before a test is spawned. The fixture is
  the analysis crate's pinned output; a copy needs a note naming the WeaverAnalysis
  commit it was taken from.
- `crates/weaver-state/src/comparison.rs:548-581` expects a built `weaver-analysis`
  binary two directories above the test executable and checks it against
  `../weaver-analysis/src`. The preload and comparison suites that call it are
  `#[ignore]` and need scratch PostgreSQL (`unshare -Ur`, no sudo), so default
  `cargo test` passes without them and they fail when run.

Until both are fixed (#37), test per crate with `-p`. The deploy scripts already select
crates: `deploy/bootstrap-stack.sh` and `deploy/update-stack.sh` test `weaver-trace`,
`weaver-harness` and `weaver-state` with `weaver-harness/pyworker,weaver-state/sqlite,
weaver-state/postgres`, then build the workspace in release with `weaver-spu/cuda`
added, because every engine an agent may elect must be compiled in or the member
refuses that agent at load.

**`weaver-spu`'s gate carries `--features cuda,gguf`.** A bare run leaves
`decoder/native.rs`, `decoder/native_pair.rs` and every test reaching them uncompiled,
and they are where the device is touched. The device is the olympus lane: a seat that
cannot compile the feature does not gate the crate and does not land acts in it. The
thinkpad has compiled it (nvcc present, CUDA 13.4, Blackwell card, 2026-09-25) and that
fact does not move the lane.

**The clippy gate is the crate touched, not the workspace.** A crate that fails under
`-D warnings` does not compile, so its dependents are not linted at all and a
`--workspace` run answers a smaller question than ten per-crate runs. Issue #471 in
the monorepo was the register of per-crate counts; counts are per box and none is
written here.

### python-spu

```bash
cd python-spu
cargo build --manifest-path oracle/Cargo.toml --locked    # the Rust oracle the suite compares against
pytest -q                                                 # CPU; the venv is held to requirements-test.lock
```

### Deploying and driving an agent

`deploy/REDEPLOY.md` (a box from scratch) and `deploy/HowToDeployANewAgent.md` (one
agent on a standing stack) are the runbooks. An agent elects sqlite or postgres (the
build carries both, on the operator's ruling of 2026-09-30, #38). `create-agent.sh` still
lays the territory out by POSIX ACLs under the operator's home, so it cannot stand an
agent on a box whose home has no ACLs (#28); small fixes are #39. The scripts are
`bootstrap-stack.sh`, `update-stack.sh`, `create-agent.sh`, `verify-load.sh`,
`decommission.sh`, and `deploy/turn.py <agent> "<text>"` sends one turn through a
loaded agent's gate as the operator's uid with no sudo. The installed stack lives under
`/etc/weaver/admin/<agent>/` (each agent's config root), `/etc/weaver/stack/` (the
scripts' record of the install, which admin never reads), `<prefix>/bin` and
`/var/log/weaver`; each agent is a systemd unit
`weaver-worker@<agent>.service` under its own OS user. Run logs of redeploys are kept
under `docs/project/redeploy-*.md`. The thinkpad runs a stack built from this tree at
the split and completes turns through the gate (2026-09-30 15:33).

### Reading command output

Cargo writes diagnostics to stderr, so a pipe without `2>&1` prints a clean nothing
for a failed run. A crate that fails to compile emits no `test result` line at all, so
check the exit status where the answer matters. Verbose output goes to the scratchpad
and is grepped there; a file already read is re-read with `sed -n`, never `cat`.

```text
cargo test -p <crate> --locked 2>&1 | grep -E '^test result|FAILED'
cargo build --locked 2>&1 | grep -E '^error' -A4
cargo clippy -p <crate> --all-targets --message-format=short --locked -- -D warnings 2>&1 \
    | grep -cE '^crates/.*: error:'     # a run that fails for a non-lint reason reads 0 here
```

**Never `git checkout -- <file>` to undo an experiment.** Copy the file aside and copy
it back.

## Seats and the pull request path

Two Claude Code sessions and the operator (Todd), who closes every loop. The
**Planner** (thinkpad seat) plans, grades drafts and handles third-party review; the
**Executor** (olympus seat, or a `-executor` session) writes code on a branch from
`main`, from a worktree, and opens a **draft** pull request. Nobody pushes to `main`
directly: it is branch-protected (a direct push was refused on 2026-09-30), so edits to
this file and `AGENTS.md`, which are the Planner's, also go through a pull request.

1. Gates first: `lock.sh`, fmt, clippy for the touched crate.
2. The Planner grades the draft against a clean extract of the head. The body carries
   `Implements: <Spec> <sections>`, and each named section gets one verdict: conforms,
   drifted (fix the code), better way (change the Spec in the same pull request, a
   design change going to the operator), or Spec gap (extend the Spec, the operator's
   ruling).
3. The Planner undrafts. That fires Codex's review; a draft gets no pass. A clean pass
   edits the summary comment in place and posts no thread; findings arrive as review
   threads, sometimes a minute after the summary row flips, so read the body, not the
   thread count.
4. Every finding is graded and answered on the pull request, fixed or declined with the
   reason. A finding names one site of its class: grep every consumer of the same
   shape in every file of the act and table each site in the body before the next
   pass. Each push to an undrafted pull request fires another pass.
5. Past four rounds the Planner checks convergence: new classes keep the loop going;
   the same class again, or findings sharing one design question, return the pull
   request to design.
6. Only the operator merges. The body names every issue it closes (`Closes #N`) and
   every epic item it closes or moves, or says in one line that it answers nothing.
   After the merge the Executor ticks each named item on its epic with the merge
   commit; an epic closes only when its checklist is empty or annotated.

Commit subjects carry `code:`, `docs:` or `process:`.

## Enforcement

Per `weaver-agent-PRD` section 11, what actually catches defects:

- **Compile-time pins** for invariants that are type properties. A runtime test cannot
  pin the absence of a trait impl.
- **Perturbation-verified tests** for invariants that are behaviours: confirm the test
  fails when the property is removed. A test that passes either way converts
  "unenforced" into "documented as enforced", which is worse than no test.
- **Human and Codex review**, read as above.
- **Clippy at `-D warnings`, per crate, at the point of an act.**

A clean automated gate is evidence the gate did not fire, not evidence of correctness.

## Police call

**An act picks up the litter it walks past**: a stale count, a doc comment on the wrong
item, a usage line printed twice, a claim the next file disproves. Fix it where found and
name it in the pull request body. The line is whether the fix needs a decision: a
ruling, a Spec election, a new instrument, a test that does not exist yet is a
construction site and becomes an issue carrying what was measured. A subagent working
under a do-not-touch list reports what it found; the coordinating seat fixes it in the
same pass.

## Experiments

`experiments/` left this tree for the suite repository on 2026-09-30, and the process
documents, `.hadesignore` and several project documents still say it is here. A run's
evidence lives in its deposit under `weaver-testing/` on the shared bulk store (olympus
exports it over NFS, the thinkpad mounts it), and a repository carries only the result
note and the scripts that produced its figures, on the operator's word of 2026-09-28.

## Conventions

- ASCII only, no em-dashes (use ` - `). Absolute dates (`2026-09-30`). No dated banners
  or header histories; git is the archive, so superseded text is removed, not struck.
- Canonical vocabulary is `trace` / `state management` / `memory`: three layers with
  authority running downward, on the operator's ruling of 2026-10-02.
  - The **trace** is what happened: append-only, and lossless for every event it
    admits (records still queued when a worker dies are forfeited, per
    `weaver-trace-PRD`), the floor under everything.
  - **State management** is what is supposed to happen, reconciled with what did, turn by
    turn. It is built from the trace's tee and rebuildable from it, and its schema,
    classifiers and re-rankers are the enforcement.
  - **Memory** is a lossy compression of state, written only by sleep-cycle
    consolidation, where patterns, strategies and the constitution are held.

  Memory proposes and state enforces. A live write or query, such as `/remember` or
  `/do-you-remember`, is state management, never memory. No Id/Ego/SuperEgo or Freudian
  framing in prose or code. Publish-destined prose uses these three words, not organ
  names.
- A ruling is a claim about the whole corpus, so an act that lands one ends with a
  whitespace-normalized sweep for every wording it retires, this file included.
- `latency is the enemy of agency`: prefer the shorter abstraction, Unix sockets over
  the network stack, subprocess CLI over MCP.
- Internal traffic never touches the network stack, and a consumer never links an
  interior crate. Internal IPC is `SO_PEERCRED`-verified.
- Publish boundary: no commercial, GTM or strategy material and no single-operator
  versus multi-tenant distinction in anything destined to be published. Check
  visibility with `gh repo view`; never assume it.
- No document edits under `docs/crates/weaver-analysis/` or the two analysis contracts
  without saying which copy you changed.
