# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## This workspace holds two separate repositories

`WeaverTools_Project/` is **not** a git repository. It is a container for two independent
clones, and the relationship between them is the single most important fact here:

| Directory | Remote | Role |
|---|---|---|
| `WeaverTools/` | `toddwbucy/WeaverTools` | **The new tree.** The ratified document corpus plus the first phase-three code: `weaver-traits`, `weaver-types`, and `weaver-trace` land first - see "Where the work stands". |
| `WeaverTools-archived/` | `toddwbucy/WeaverTools-archived` | **The quarry.** The full ~150k-line prior program, frozen. 438 PRs of history. Scheduled for deletion once G6 certifies extraction complete (checklist item 7). |

The new tree is a **one-way extraction** from the quarry, not a fork. Nothing merges back
in either direction, ever. The quarry is a **parts source you read and never edit** - no
commits, no branches, no fixes there, however tempting. Its last commit is
`d9366d5` (2026-07-28).

The old tree's `CLAUDE.md` and `docs/CLAUDE.md` load automatically when you work inside
`WeaverTools-archived/`. They are accurate about *that* tree and stale about the program's
direction - they describe a 12-crate workspace with a memory leg and a conformance graph,
all of which the extraction leaves behind. Read them for how the parts work, not for what
to build.

### The founding document is not on disk

The handoff that defines the extraction was committed and then immediately reverted
(`9dd8caf` then `d9366d5`, both 2026-07-28). Retrieve it:

```bash
git -C WeaverTools-archived show \
  9dd8caf:docs/project/HANDOFF-2026-07-28-radical-simplification-PROPOSED.md
```

**Read it before doing anything in this workspace.** It is marked PROPOSED and its own
first instruction is that the session produces a written, operator-ratified inventory
before any code is written. Whether that ratification has happened is not recorded
anywhere in either tree - ask the operator rather than inferring it from the empty
`WeaverTools/` repo existing.

## The mission and the carry rule

Deliverable: **a deployable proto-stateful agent that emits a clean, turn-bracketed,
correctly-custodied trace.** The trace is the primary artifact, not a diagnostic - that
reframing is what promotes quarry issues #340/#343/#344/#363 from debt to blockers.

**Proto-stateful, not stateless.** The human's ruling of 2026-08-01 retired "stateless" as
an overstatement, and `weaver-agents-PRD` section 2 is the authority. The agent holds
real state *within* a session and none *across* sessions. **Two things hold state across turns
inside one session, both deliberate and not two things of a kind.** The first is the working
structure, the run's trace events held in RAM in the canonical form the stream carries,
volatile by construction. The second is the hot KV cache, an optimization whose owner, flush
trigger, and forbidden touchers are named in `weaver-spu-PRD`. Lose the first and turn two
has nothing to be about. Lose the second and the agent is slow rather than absent. If you
meet "stateless" anywhere in this workspace outside a record of the rename, it is stale.

Anything crossing from quarry to new tree goes through exactly one of two doors, and you
**state which door and why at the moment you carry it**:

1. **Live code** - a proto-stateful agent provably needs it, meaning you can name the path a
   single completed turn takes through it.
2. **Stub** - a named joint the memory leg will bolt onto, *and* a written memory-leg
   design already names it. No document, no crossing. Without that constraint door two
   becomes the baggage door and every individual stub still looks principled.

Everything else stays in the quarry. **Nothing crosses because the old tree has it.**

In scope: `weaver-spu`, `weaver-harness`, `weaver-gate`, `weaver-admin`, plus
`weaver-trace`/`weaver-types`/`weaver-traits`. None come over verbatim.
`weaver-traits` and `weaver-types` are **demand-derived** - built from what the SPU and
harness turn out to need, never carried and pruned. `weaver-trace` is the exception:
**designed** against what the memory leg will later read, because demand-derivation
under-builds a deliverable.

Out entirely: the memory leg in any form, `weaver-memory`, `weaver-train`,
`weaver-frontend`, and `weaver-interface`. The composition root is
deliberately **new code** - it is where the session boundary gets enforced, and where quarry
issue #350 (the agent worker implements no task executor) gets solved rather than
migrated.

Order of work: SPU -> trace -> harness + new composition root -> admin and gate ->
deployable proto-stateful agent -> autonomic calculator tool -> then memory.

## Where the work stands, and what governs it

**Four process documents in `WeaverTools/process/` govern everything and outrank this
file.** Read them before acting: `WeaverTools-Working-Process` (phases, seats, gates),
`WeaverTools-Document-Format` (the document notation), `WeaverTools-Working-Rules`
(editorial), `WeaverTools-Handoff-Format`. They carry versions and change often, so read
the file rather than trusting a version remembered from a summary.

**Three phases, and the program is in the third.**

1. **Phase one, authoring.** PRDs, contracts, Specs. Closed 2026-08-04.
2. **Phase two, graph mapping.** The graph was built on the HADES server as
   `WeaverTools_v3` per `HANDOFF-2026-08-04-hades-graph-build`, and **the set was
   RATIFIED 2026-08-04** per the operator's ruling recorded at Working Process
   section 5. **Phase two and ratification are suspended until release**, on the
   operator's ruling of 2026-09-27, and HADES is not part of this project until then.
3. **Phase three, coding.** Open, gates H1-H5 in force per Working Process
   section 6, H6 and the census having retired 2026-09-27. A merged Spec
   authorizes code, and where code and a Spec disagree the act changes whichever
   is wrong and moves on. No version is pinned here because the Format moves and
   this file has carried a stale pin before.

**Assertion records are suspended until release**, per Document Format section 0 as
of 2026-09-27. The records that stand stay legal and unmaintained, and no new one is
required. The conformance check meanwhile is review against the Spec sections a pull
request names, per Working Process section 6.

**Gates G1-G7 run on every act** (mechanical, level discipline, graph facts, vocabulary,
duplication authority, extraction completeness, rulings landed). H1-H5 are phase
three's, in force per Working Process section 6, H6 having retired 2026-09-27.

**A ruling is a claim about the whole corpus.** A review finding names one sighting of its
violation, so an act that lands a ruling ends with a corpus-wide sweep for every wording
the ruling retires - and the sweep must be whitespace-normalized, because prose wraps at 88
columns and any phrase can straddle a break. **This file is in the tree so that a
corpus-wide sweep reaches it**, a rename having once swept the corpus clean and left it
behind for sitting outside.

## This machine is not the deployment box

The quarry's own `CLAUDE.md` documents runtime paths (`/opt/weavertools` source,
`/opt/weaver` installed runtime) that **do not exist here**. Consequences:

- The quarry is cloned to a home directory. Per-agent OS users cannot traverse a 0700
  home, so nothing agent-facing can actually run from this checkout - it is a reading and
  planning workspace.
- `.hades/` is gitignored and absent, so `gate-check.py` cannot run here. The quarry's
  mandatory merge-gate sequence is not executable from this machine.
- **`nvcc` is present and the device gate finishes here.** On 2026-09-25 the box
  carried `/opt/cuda/bin/nvcc`, CUDA 13.4.92, and an RTX PRO 5000 Blackwell on
  driver 615.71.09, and `weaver-spu`'s gate under `--features cuda,gguf` passed
  cold twice - build, test and clippy - per #684. The second run had no
  `CUDARC_CUDA_VERSION` override, cudarc `0.19.10` building its CUDA 13.4 API.
- **A box that can compile the feature is not a box that should gate the device.**
  The lane ruling of 2026-09-15 stands untouched by the correction above: the
  device is the olympus lane, `weaver-spu`'s gate carries `--features cuda,gguf`
  there, and `kernels/PROVENANCE.md` records what this hardware has run: the
  crate's own device tests ran on the card at the candle repin of 2026-09-26,
  and the salvaged kernels have not, nothing in the crate calling them yet.
  The earlier bullet argued the lane from a missing compiler, which
  made a standing ruling rest on a fact about one box that turned out to be wrong.
- The pinned toolchain (`nightly-2026-02-13`, rustc `47611e160`) is installed and matches
  `rust-toolchain.toml`.

## Building the new tree

From `WeaverTools/`. Nightly, edition 2024, eleven packages.

```bash
cargo build --workspace --locked
cargo test --workspace --locked
cargo test -p weaver-harness --locked   # one crate
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

**`--locked` on every command that resolves**, closing the first ask of
issue #551. Without it cargo repairs a manifest change in place and the gate
answers about a tree the repository does not record. With it the run refuses
before a single test binary is spawned, so the drift is loud rather than
silent. `fmt` resolves nothing and takes no flag.
**This is the only place the refusal fires**: the deploy's own test line
selects four crates, so the manifest instruments of `weaver-gate` and
`weaver-internal` are reached by these commands and by nothing else.

**The flag refuses only where a seat types it, so the lock has a gate of its
own**, on issue #551's third ask and as of 2026-09-15.

    process/gates/lock.sh

It runs from any directory and answers in an exit status: 0 the resolution is
in step with the lock, 1 drift, 2 the gate could not run and the lock is
unchecked rather than clean. **0 and 1 are definitive and 2 is not**: drift
whose resolution completes offline reads 1, and drift needing the network - a
package or a fork rev the local cache does not hold - reads 2 alongside a cold
cache and an unreadable manifest, because cargo fails before it reaches the
refusal that names the lock. So 2 means unchecked rather than drifted, the
message says which it might be, and a run after `cargo clean` or on a fresh
clone answers it for a tree that may be fine. It compiles nothing, so it
costs well under a second and it goes first, ahead of the commands above. An
instrument that runs inside a test binary cannot do this job at all, `cargo
test` having resolved and repaired the lock before the binary is spawned, which
is the defect #551 was filed against.

**It is not a seventh enforcement device, and no document in this corpus carries
a rule about the lock.** It is build hygiene under the resolution every command
above rests on. Whether the lock deserves a sentence in the corpus is #551's
remaining question and the operator's to answer. The script's header carries the
measurements and the reasoning and they are not copied here, per gate G5.

**Every command here was run before being written here, and none of them
touches the lock.** Counts go stale, so a later reader re-runs rather than
trusting a number.

**The quarry's command below is not this one.** It carries a
`weaver-spu/inference` flag that is correct there and errors here.

**`weaver-spu`'s gate carries `--features cuda,gguf`**, on the operator's
ruling of 2026-09-15. `default = ["gguf"]` puts the inference path on without
a flag, so a bare run lints the crate and leaves `decoder/native.rs`,
`decoder/native_pair.rs` and every test that reaches them uncompiled. They are
not small and they are where the device is touched.

    cargo clippy -p weaver-spu --all-targets --features cuda,gguf --locked \
      -- -D warnings

**A seat that cannot compile the feature does not gate the crate, and
therefore does not land acts in it.** The device is the olympus lane and the
gate follows the lane, rather than narrowing for every seat to accommodate a
box that should not be acting on the SPU at all.

## Building the quarry (read-only verification)

Nightly, edition 2024. From `WeaverTools-archived/`:

```bash
cargo build                      # workspace, no GPU
cargo test --workspace           # 12 packages
cargo test -p weaver-harness     # one crate
cargo test <name_fragment>       # one test by substring
cargo clippy --workspace --all-targets --features weaver-spu/inference -- -D warnings
cargo fmt --all -- --check
```

Cold resolution needs network: `weaver-spu` sources `candle-*` and `llama-cpp-2`/
`llama-cpp-sys-2` from `github.com/toddwbucy` forks at pinned revs. The `llama-cpp-rs`
fork pin (exposing the ggml scheduler eval callback - the only route to
per-layer activations from a GGUF model) was the stated precondition for cutting the
extraction, and it **is** in the quarry's `main`, at `277e4100`; the new tree pins
`ecce255bcb14dd6d88f184cc8776c23a85afafeb`, moved 2026-08-17, which still exposes it.

`crates/weaver-frontend` is excluded from the workspace and needs X11/Wayland/GL dev
libs, so build it from inside its own directory if at all.

## Orienting in the quarry

Sizes matter here - the carry rule is a subtraction discipline and roughly 90k lines
are in scope for consideration. `wc -l` over `crates/<name>/src` gives the current
figures when you need them.

Reading order for architecture: `docs/weavertools-HAH-v41.md` (the hypothesis this whole
apparatus tests), `docs/weavertools-primary-PRD.md` (the apparatus apex),
`docs/crate-topology-Spec.md` (the doc<->crate map). Per-crate PRDs and Specs are at
`docs/architecture/crates/<crate>/`, mirroring `crates/<crate>/` positionally.
`docs/project/handoffs/` and the dated `HANDOFF-*.md` files at `docs/project/` are the
narrative of how each subsystem reached its frozen state.

Design patterns worth carrying forward conceptually (they are the quarry's real
contribution, independent of its code): per-invocation tool safety classification
(`Tool::invocation_properties(input)` inspects the *actual* command - `ls` reads,
`rm -rf` destroys - which drives parallel-vs-serial batching), events-as-rendering-API
(`QueryEvent` over mpsc, consumed identically by CLI/TUI/tests), provider-agnostic
messages with all wire format isolated at the composition root, and `SO_PEERCRED`-verified
Unix sockets for all internal IPC.

## Enforcement, and the graph

**The graph is deferred to release**, on the operator's ruling of 2026-09-27, which
`WeaverTools-Working-Process` sections 5 and 6 carry. HADES is not part of this
project until release, when it returns as a lookup and a diff of documents against
code. No rebuild is owed on document movement, and ratification, phase two's
checklist, the census and H6's header rule are suspended with it. Existing assertion
records and `conforms:` headers stay in place and are not maintained, and no new ones
are required. The graph was a second source of truth every change had to keep true,
and it had not once helped write code.

**The conformance check is review against the Spec.** Every pull request body carries
`Implements: <Spec> <sections>`, and the Planner's grade gives each named section one
verdict: conforms, drifted (fix the code), better way (change the Spec in the same
pull request, a design-level change going to the operator), or Spec gap (extend the
Spec, a new capability being the operator's ruling). H1 is applied as documents are
written: where code and a Spec disagree, change whichever is wrong in the same act,
contracts included and reaching every party. Working Process section 6 owns the rule.

**Enforcement rests on the devices `weaver-agents-PRD` section 11 enumerates**, which
this list restates, the first suspended until release and the sixth retired:

1. Conformance trace headers in source carrying `code -> assertion -> doc`.
   **Suspended until release**: existing headers stand unmaintained and nothing
   reads them.
2. **Compile-time pins** for invariants that are type properties. A runtime test
   structurally cannot pin the *absence* of a trait impl.
3. **Perturbation-verified tests** for invariants that are behaviours. Always confirm the
   test fails when the property is removed - a test that passes either way converts
   "unenforced" into "documented as enforced", which is worse than no test.
4. Human and Codex review. Read the review **body**, not the thread count: a clean Codex
   pass edits its summary comment in place and posts no review object and no thread,
   and findings arrive as review threads, sometimes a minute after the summary row
   flips, so a thread count of zero is not a verdict until the summary row reads
   completed and a read of the threads taken after it still finds none.
5. **Clippy at `-D warnings`, per crate at the point of an act**, on the
   operator's ruling of 2026-09-06. **The gate is the crate you touched, not the
   workspace**: `cargo clippy -p <crate> --all-targets -- -D warnings` passes
   before that crate's act merges. It is the cheapest of the six, and until the
   census joined it the only one a person had to type, which is how it went
   unrun.

   **Stated per crate because the workspace did not pass when the gate
   landed, and a gate nobody can pass is a gate everyone learns to ignore.**
   The backlog clears as each crate is next touched rather than as one act
   nobody owns.

   **Issue #471 is the register and this file keeps no census.** A count
   written here is stale by the next act and then argues with the command.
   Measure rather than read:

   ```bash
   for c in $(ls crates); do
     if out=$(cargo clippy -p "$c" --all-targets --message-format=short \
                -- -D warnings 2>&1); then
       printf '%-18s %s\n' "$c" 0
     else
       n=$(printf '%s\n' "$out" | grep -cE '^crates/.*: error:') || true
       [ "$n" -eq 0 ] && n=BROKEN
       printf '%-18s %s\n' "$c" "$n"
     fi
   done
   ```

   **`BROKEN` means the run failed for a reason that is not a lint** and the
   crate's gate is unknown rather than passed. It is separated because a
   loop that counts lint lines out of a pipe reports the exit status of
   `grep` and prints a clean zero for a run that never linted. A bad flag is
   enough to cause it, erroring on cargo's first argument and reading as clean
   through the pipe. **The zero a broken run prints is the most expensive line
   in this section**, so it prints a word instead.

   **It counts the source lines the lint names**, so a finding whose path
   clippy prints relative to the crate rather than the tree is not in the
   count. That is not the case in this tree today, and it is where to look
   first if a crate you know is dirty reads zero.

   **The count is per box and this file records none.** Two seats running
   that loop on one commit have returned different answers, a cast whose lint
   fires only where it is a no-op being a property of the target's headers
   rather than of the tree. **A count is a reading taken on a box**, which is
   the second reason it does not live here, and #471 carries each reading with
   the seat that took it.

   **The workspace sweep is not the gate and under-reports it.** A crate that
   fails does not compile under deny-warnings, so its dependents are not
   linted at all and `--workspace` answers a smaller question than eleven
   per-crate runs do.

6. **The census**, H6 from the operator's ruling of 2026-09-11, **retired
   2026-09-27** with the graph. Its scripts left the tree in #713, git being the
   archive. Section 1's archive rule is held by review meanwhile, an archive
   directory in a diff being a finding. `process/ingest/chunk_plan.py` and
   `.hadesignore` are dormant until release.

Every real defect found in the quarry's final week came from items 2-4, while
`gate-check.py` returned 0 findings on four consecutive PRs and the graph returned zero
code defects while accumulating 53 dangling edges of its own. A clean automated gate is
evidence the gate did not fire, not evidence of correctness.

## The pull request path

All pull requests open as drafts, from a worktree, and the arrangement runs as the
#683 trial of 2026-09-25 settled it and the operator's word of 2026-09-27 fixed it:
the Executor seat opens the draft and never takes it out of draft, the Planner seat
grades the draft and undrafts it when it passes, Codex's GitHub review is the
third-party reviewer, and the operator merges. CodeRabbit is retired since
2026-09-22. **Edits to this file and to `AGENTS.md` go straight to `main` from the
Planning seat**, on the operator's word of 2026-09-27, so the Executor stays on code.

**A pull request in draft gets no pass.** Undrafting fires one, every push to an
undrafted pull request fires one, and `@codex review` or `@codex security review` on the
pull request requests one. A clean pass edits the summary comment in place and posts no
review object and no thread. Findings arrive as review threads, sometimes a minute after
the summary row flips. Because the rework is a push to an undrafted pull request, the
rework is always reviewed, which is the rule the sub-agent seat once carried as its
second pass: on 2026-09-11 answering fifteen findings introduced a real defect in three
pull requests of four, each found by the pass after the fixes.

**The Planner grades every pass on the pull request** against a clean extract of
the head, and verifies each fix by its own perturbation, not by the Executor's
account. The grade is what the Executor acts on: a valid finding is fixed, an
invalid one is declined with the reason, and either way the finding is answered
on the pull request, since the record carries it as it stood. **Passing means no
finding that changes behaviour or corrects a claim is unanswered.**

**Fix the class and walk every site before the next pass.** A finding names one site of
its class, and a site fix answers the finding while the reviewer finds the next site:
#683's tail was classes fixed narrowly and found again, pass after pass. So a fix greps
every consumer of the same shape in every file of the act, tables each site in the body
with its disposition, and only then takes the next pass.

**More than four review rounds is a checkpoint, not a stop**, on the operator's word of
2026-09-28. The bound was chiefly a cost control from the CodeRabbit era, and Codex's
review is a fixed cost. Past four rounds the Planner evaluates whether the findings
converge. Valid, distinct findings, each a new class, keep the loop going. The same
class found again, or findings sharing one design question, return the pull request to
design, which is how #683's twenty-eight passes ended: each finding was the next site of
a few classes. Working Process section 6 owns the rule, with the two that come before it
on the ruling of 2026-09-27: a verdict's pass is stated in its Spec before it is
hardened, and a carry is not a hardening.

**Gates before review**: a reviewer's attention on what a command can check is attention
not on "does this fix hold". The order, then, is this. The gates come first - `lock.sh`,
fmt, clippy for the touched crate, and the G1 greps where documents moved - then the
Planner's grade of the draft against a clean extract, carrying a conformance verdict for
every section the `Implements:` line names, then out of draft, which fires the Codex
pass. Every finding of the pass is graded and answered, fixed or declined with the
reason on the pull request. The fixes pass the gates and then are pushed, that push
fires a pass and the Planner grades it, and the loop repeats until a pass leaves nothing
to push, past four rounds of it the checkpoint above. Then the operator's merge.

**A pull request names what it answers, and a merge is not done until the ledger
is.** On the operator's ruling of 2026-09-26, after an audit of the eleven epics
found thirty checklist items landed and never ticked, twenty-two belonging to a
crate that had left, and every closed epic still carrying live items. Three
clauses. First, the body carries every issue the act closes, as `Closes #N`, and
every epic item it closes or moves, by epic and item number with what the act did
to it, and a pull request that answers nothing says so in one line. The Planner's
verification before undraft checks the list is there. Second, after the operator
merges, the Executor ticks each named item on its epic with the merge commit, or
annotates it as moved or declined with the reason, in the same session as the
merge notice, and the Planner's verification of main after a merge reads those
edits. Third, an epic closes only when its checklist is empty or every remaining
item is annotated with where it went. GitHub joins an issue to a pull request
only whole, and an epic is the one thing a pull request never closes whole, so
the join is made by hand in the body and kept true at the merge, and the register
of 2026-09-26 is what its absence cost.

## Experiment evidence lives on the share

**A run's evidence lives in its deposit on the shared bulk store, and the repository
carries only the result note and the scripts that produced its figures**, on the
operator's word of 2026-09-28. Deposits sit under `weaver-testing/` on the bulk store,
which olympus exports over NFS to the LAN and the thinkpad mounts, so each seat reads
the other's runs where they were written. The record, the log, the summary, the box
facts, the raw trace and the captured journal evidence all stay there, in the deposit
and its `evidence/` directory. A trace runs to hundreds of megabytes, and committing one
would make every clone carry it. **Every experiment's results go there by default**, and
the result report goes in two places, a copy in the deposit and the same report under
`experiments/` in the repository. The repository's copy names its deposits and copies no
data from them, and beside it sit the commands that ran the run and every script that
computed a number the report states, the harness itself cited by its commit.

## Police call

**An act picks up the litter it walks past.** A count gone stale, a doc comment
attached to the wrong item, a usage line printed twice, a claim the file next to
it already disproved. These are corrected where they are found and named in the
pull request body. They do not become issues and they do not wait for an act of
their own, because filing one costs more than fixing it and the filing is the
part that goes stale.

**The line is whether the fix needs a decision.** A ruling, a Spec election, a
new instrument, a test that does not exist yet - that is a construction site and
it is not this act's to clear. It gets an issue carrying what was measured.
Everything short of that is litter, and an act that walks past litter to file a
ticket about it has made two pieces of work out of none.

**A subagent often cannot pick it up.** Parallel acts hold files, and an agent
editing outside its own extent is how two acts collide. So an agent reports what
it found and where, and the coordinating seat fixes it in the same pass. A report
is not a deferral, and the do-not-touch list an agent works under is about
collision and never about whether the thing gets fixed.

Per the operator's instruction of 2026-09-16, after a day in which six stale
counts and misattached comments were filed as owed to acts that did not exist.

## Command output is context, and the session pays for it

**On the operator's ruling of 2026-09-11.** A session can spend a fifth of a
one-million-token window on the output of its own commands: not on the work
and not on the conversation, but on `cat`, on full test runs, on `psql` dumps,
and on re-reading files already in the window. The corpus is large and a
session that reads it carelessly runs out of room to think.

**Verbose network and database output goes to a file, then the file is
queried.** A result held once on disk can be grepped ten times for nothing,
where a result printed to the session is paid for once and then paid for again
in every later turn that carries it. Write to the scratchpad, report the count,
read back only the rows that matter.

**Cargo writes its diagnostics to stderr, so a pipe without `2>&1` discards
what it claims to filter** and prints a clean nothing whether the command
succeeded or failed. The enforcement section above calls the zero it prints
the most expensive line in that section.

```text
cargo test -p <crate> 2>&1 | grep -E '^test result|FAILED'
cargo build 2>&1          | grep -E '^error' -A4
cargo clippy -p <crate> --all-targets --message-format=short -- -D warnings 2>&1 \
                          | grep -cE '^crates/.*: error:'
git diff                  --stat first; the full diff only for the hunk in hand
a listing                 aggregated - uniq -c, awk totals - never row by row
a file already read       sed -n 'X,Yp', never cat
```

**A count from a pipe is still not the command's verdict**: a crate that fails
to compile emits no `test result` line at all, so the grep prints nothing and
nothing reads like success. Check the exit status where the answer matters.

**Never `git checkout --` a file to undo an experiment.** It restores the
index, and an uncommitted rewrite in that file is gone. Copy the file aside
and copy it back.

**Grep the narrowest thing that answers the question.** `grep -c` where a
count settles it. A path rather than a tree. One section of a Spec rather than
the Spec, which at two and a half thousand lines is most of a percent of the
window each time it is opened.

**This is a discipline and not a tooling gap.** A retrieval index over the
corpus would cut the document half of it, and is wanted for other reasons -
but the command output above is the session's own doing and no index touches
it.

## Conventions carried from the quarry

- **Editorial: ASCII only, no em-dashes** (use ` - `) in docs and handoffs.
- **Dates are absolute** (`2026-07-28`). A document carries no dated banner and no
  header history, per Working Rules section 1. Where a kind elects `Landing PR`,
  and a process document does not, the field names the pull request that last
  changed what the document says.
- **Forbidden vocabulary:** no Id/Ego/SuperEgo/Freudian framing in prose or code. Canonical
  terms are `trace` / `reflection` / `substrate-state`.
- **`latency is the enemy of agency`.** Prefer the shorter abstraction. Internal traffic
  uses Unix sockets, never the network stack. Default to subprocess CLI over MCP - the
  JSON-RPC and stdio buffering cost compounds across hundreds of tool calls per session.
- **OPSEC / publish boundary.** The open-core plan extracts the SPU as a separate public
  crate, so the guard is the *publish* boundary: no commercial, GTM, or strategy material
  and no single-operator-vs-multi-tenant distinction in anything destined to be published.
  **Check visibility, never assume it.** It has changed more than once and this file
  has been wrong about it, so any statement of it here is a record rather than a
  current fact. One command settles it:
  `gh repo view toddwbucy/WeaverTools --json visibility`.
