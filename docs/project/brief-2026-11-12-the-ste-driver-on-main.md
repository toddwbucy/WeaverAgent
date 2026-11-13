# Brief: the external STE conversion driver on main, without the agent side of R0

Written by the WeaverAgent Executor on 2026-11-12 from the suite Planner's brief of that
day. Stack `act/ste-driver-on-main`, from `main` at
`6a8340a51ac1c7a621a6723632da03670f624bbf`.

## Why

The STE conversion loop (`docs/crates/weaver-harness/Loops/ste-conversion-loop.md`) has
an external driver. The driver masks the protected spans, frames each request, sends it
through the agent's gate with `deploy/turn.py`, checks the answer with the pinned
language rules, repairs, and records each attempt and its terminal class (sections 4, 6
and 7).

That driver exists only on the draft pull request #450, which consolidates the stack
#450 to #472. #450 also carries the agent side of R0. Its base is `96bbc9c4`
(2026-11-03), and `main` has taken 197 commits since. Merged whole, #450 conflicts in
two files, and most of its 170 files are crate work that `main` has since moved past.
The driver itself is new paths only and lifts onto `main` with no conflict.

The first use is the bootstrap conversion round on the operator's rulings of 2026-11-12:
Granite 4.2 30B with the trained STE100 rewriter adapter converts a seeded slice of a
public agent-trajectory dataset into STE100. That round does not wait for this act, and
this act does not wait for the round's open rulings. It does not touch an installed
agent.

## What is lifted

Exactly what a trial lift onto `main` carried, and nothing else:
- `tools/ste-driver/`: the driver, its own lock file and its tests. It is not a member
  of the workspace and links no `weaver-*` crate.
- `crates/weaver-harness/dev_ste_driver/` (`mod.rs` and `tests.rs`): the pure request
  owner. The driver compiles `mod.rs` as source, by `#[path]`, as its `policy` module.
  The harness does not compile it into its library.
- `crates/weaver-harness/tests/ste_conversion.rs`: the harness's test of that module.
- `crates/weaver-trace/tests/fixtures/ste-emission-offer.json`: the trace fixture that
  the driver's `trace_report` test reads. `main` lacks it.

Two changes from #450's head `15dc3e51`:
- **Kit uniqueness on the trajectory and the locator** (`src/prepare.rs`). #450 refused
  a kit with a repeated trajectory UUID. A trajectory has many prose segments, so a real
  slice repeats its UUID once per segment and was refused. Preparation now refuses an
  empty UUID or a repeated (UUID, locator) pair. A new test,
  `preparation_accepts_one_trajectory_at_several_locators`, covers it, and the
  existing refusal tests are kept.
- **The README's target examples** follow the private-target rule (`CLAUDE.md`, "Gates
  run with a private target directory"). They name a target owned by the checkout and a
  short temporary directory, and say why, in place of a shared target under the system
  temporary directory.

The folded prompt the driver reads,
`docs/crates/weaver-harness/Loops/ste-conversion-prompt.txt`, is already on `main`, and
it is identical to #450's.

## What stays in #450, and why

The agent side of R0 stays in #450: the harness flow and record changes, the SPU family
markers, the state, trace payload and type changes, and the admin parts. It is
incomplete, and it waits on the foundation cutover, so it is not lifted here. #450 to
#472 hold the review history of every lifted file. This act cites them and does not
repeat them.

## What the driver was graded against on main

On `main`'s tree with the lifted files and nothing else:
- the driver's 72 tests, and its ignored pinned-input test run against WeaverTrain at
  `3a619bb`.
  - The driver pins WeaverTrain `9881da6`. Between the two commits, every `ste_*`
    script, the `ste/` package, `lora_build` and the manifests are byte-identical, so
    they check identically.
- recorded Granite outputs from two earlier batches, replayed through the real driver,
  `deploy/turn.py` and the gate client against a stand-in gate. Each reproduces its
  batch's pass set exactly: 61 of 100 and 57 of 100, the same items as the batches'
  own checker.
- the driver's mask equal to WeaverTrain's Python `ste_convert.mask` on every segment of
  two seeded slices of the dataset: 664 and 648 real segments, the codes included.
- on a 20-trajectory slice run against the stand-in, every request text equal to
  WeaverTrain's request and repair layouts, and the driver's rule names equal to
  WeaverTrain's Judge on every checked output.

## The loop document's clauses

The source cites clauses 4.01, 4.02, 4.04 to 4.07, 4.11, 6.01 to 6.04 and 6.06 to
6.08, and 7.01, 7.04, 7.05 and 7.07. It names 5.02, the agent's detection rule, as a
clause it does not implement. The loop document and the folded prompt are
byte-identical between #450's head and `main`, so no cited clause has moved.

The first use found these items for the loop document's own amendment. They are not
edited here:
- 4.11 names `9881da6`. A run may use a later WeaverTrain commit that checks
  identically. The clause should say how such a commit is recorded.
- 4.02's row limit does not say which chat template and tokenizer measure it.
  WeaverTrain measures with Nemotron's. Under Granite's, more rows exceed the limit.
- 8.01 names Nemotron 3 Nano 30B for the first rewriter adapter. The trained adapter is
  Granite 4.2 30B (the operator's ruling of 2026-11-08).
- Section 9's machine plan predates the run's machine and model.
- The route that sends each request as an independent turn needs the declaration's
  `turn-context = "independent"`, which the document does not name.
- An agent laid by `create-agent.sh` admits the gate connector alone. The document does
  not say who sends the seeding turn and the requests.
- The exclusion of held-out and training rows from a selection, by UUID, with each
  set's digest recorded.

## Not in this act, and open for the operator

The caller side of the first round is Python under a pinned WeaverTrain:
- the seeded selection (4.02), with its exclusions and either row-length measure
- assembly (6.02 and 6.06), with its integrity and byte checks
- the scorer and the review sample (6.05)
- the stand-in gate the replays ran against

Each is graded against real data and perturbed check by check. Where these live is
open for the operator, since they wrap WeaverTrain's functions by pin.

The Executor's recommendation: put them beside the driver, under `tools/`, behind the
same pinned-source check the driver's `tests/reference.py` makes, since the loop
document puts selection, assembly and scoring in the driver. A later act can port the
assembly checks into the Rust driver, where 6.02 places the literal check.

## Sub-acts

1. One code pull request against this stack: the four lifted paths, the uniqueness
   change with its test, and the README fix.

## Acceptance

- The lifted tree equals the trial lift: #450's files at `15dc3e51` plus the two changes
  above, and no other path.
- Gates with a private target: `process/gates/lock.sh`. `cargo fmt --all -- --check` for
  the workspace and for the driver. Clippy at `-D warnings` for the driver with all
  targets, and for `weaver-harness` with all targets. The driver's tests, and its
  pinned-input test with the WeaverTrain paths set. `cargo test -p weaver-harness`. The
  trace crate's tests, since the fixture lands in its tree. `python3
  deploy/test_plans.py`, which holds the documentation mirror tests.
- The pull request bodies say what is lifted, what stays in #450 and why, and what the
  driver was graded against.
