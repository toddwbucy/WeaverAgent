# Handoff: olympus returns to the SPU lane at the new pin

**Version:** v0.5, 2026-09-27. Written from the thinkpad Planner seat for the
olympus seat, for its return on Monday 2026-09-28.

**Base:** `WeaverTools` at `07a7e5d`, after the merge of PR #716. Olympus's last
device-side work stands at `e69916a`, the commit both probe stacks were built
from, and everything between is named below.

## The claim

The pins moved under the SPU while olympus was away, and no gate has run on
olympus's hardware at either. PR #684 (2026-09-25) took `cudarc` from 0.19.8
to 0.19.10 so CUDA 13.4 builds natively, and it retired the
`CUDARC_CUDA_VERSION` override that used to sit in the operator's
`~/.cargo/config.toml`. PR #706 (2026-09-26) moved candle from the fork's
`4296151` to `aee9af9c`, the fork's integration branch carrying upstream to
2026-09-25 and a fork fix for an upstream regression that fails `rms_norm` on
every dense checkpoint without a norm bias. Both were landed and tested on
thinkpad, on the operator's ruling that olympus verifies them on Monday. The
thinkpad reading is in `crates/weaver-spu/kernels/PROVENANCE.md`'s Blackwell
row. The Ampere row still reads CUDA 13.0 from 2026-08-06, and the installed
runtime on olympus is a binary at the old pins until step 6 reinstalls it.

**bf16 output is not reproducible across the candle repin, and f32 is.** The
candle side measured Qwen2.5-0.5B at both revs: f32 logits and all 24 layer
residuals bit-identical, bf16 greedy tokens the same and bf16 logits moving by
up to 0.5, which is reduction order from upstream's kernel changes and not a
math change. Any bf16 figure olympus recorded before the repin and means to
compare after it is re-taken at `aee9af9c`. That lands on epic #698's
precision and kernel-stack arms.

## What changed, and why olympus reads it before building

The process moved. `CLAUDE.md`'s pull request path describes review as it runs: the
Executor opens a draft, Codex's GitHub review is the third-party reviewer and reviews
nothing in draft, the Planner grades every pass against a clean extract, the operator
merges, and since `83aff15` every pull request names the issues and epic items it
answers and the merging seat ticks them. Working Process section 6 carries the last of
those rules and not the loop itself, its H5 still naming the architecture seat. On the
operator's ruling of 2026-09-27 (#713) the graph is deferred to release and the census
and H6 are retired: no census runs, no new conformance header is owed, and every pull
request carries an `Implements:` line whose Spec sections the Planner grades. Fix the
class and walk every site before the next pass. Working Process v0.35 adds three rules
that #716 earned: code whose exit is a verdict has its Spec state what a pass certifies
before it is hardened, a carry into the tree is not a hardening, and more than four
review rounds is a checkpoint at which the Planner asks whether the findings converge,
relaxed from a stop on 2026-09-28. The Blackwell probe's Spec, section 5,
carries the rule that no privileged step acts on a copy of a fact held by another party
without checking it against evidence it verified itself, as a rule of that probe's
custody.

The tree moved. `weaver-web` left the repository with PR #689 for
`WeaverTools_Project/weaver-web/`, so the workspace is eleven crates and the deploy
builds the whole workspace with no exclusion. The probe's code and Spec live under
`experiments/deployment-tuple/device/blackwell/`, and the parts of its hold-lift
that needed no ruling landed with #693 and #695. The hold itself stands until the
privileged approval step lands, per the Spec's section 5. `weaver-trace` gained
`recall` (#692) and `message.restored` (#702), the diagnostic replay crosses a flush
and an elision and refuses a record that does not account for its model's input
(#694, #701), a restore from a branch carries its inherited conversation (#705), and
a task's verdict is a trace kind carried on the analysis summary (#707, #708). The
test suites stopped leaking temporary directories (#704), except the SPU's
`weaver-spu-residency` file, which is olympus's to guard.

The ledger moved. The candle chain's items left #639 for two epics on
2026-09-26: #699, the native engine's toolchain, and #698, the
deployment-tuple experiment arm by arm, each row on #639 annotated with its
new home, the annotations on #639 and #679 carrying the audit of that date.

## What olympus does on Monday, in order

1. Pull `main` at or past `07a7e5d` and read the four process documents and
   `CLAUDE.md`'s pull request path before opening anything.
2. Check `~/.cargo/config.toml` on olympus for a `CUDARC_CUDA_VERSION` override
   and remove it if present, as #684 did on thinkpad. cudarc 0.19.10 detects
   the toolkit itself, and an override left behind answers for a toolkit the
   box may not have.
3. Measure and record before building: olympus's three cards, the two A6000s
   and the RTX 2000 Ada, the driver version and the installed toolkit
   (`nvcc --version`), in the body of step 5's pull request, the way #684's
   and #706's bodies did.
4. Run `process/gates/lock.sh` and `cargo fmt --all -- --check` first, then the
   SPU gate on olympus's own target directory, never a shared one:

       cargo clippy -p weaver-spu --all-targets --features cuda,gguf --locked \
         -- -D warnings
       cargo test -p weaver-spu --features cuda,gguf --locked

   The `--locked` flag refuses if the resolution drifts from the lock, and a
   refusal is the answer, not an obstacle. Read the `test result` lines and
   the exit status, not a grep of them. **A pass counts a test that skipped
   for want of an artifact or a second card**, so name which device tests ran
   with their assertions and which passed by skipping, as #706 did. On the
   A6000s the two-card tests are the ones only olympus can run: pin them to
   the two A6000s with `CUDA_VISIBLE_DEVICES`, point the artifact variables at
   real models, and make them run.
5. Re-measure the Ampere row of `PROVENANCE.md` at the new pins, every column,
   and land it as one pull request under the rule: its body names #699's items
   on the olympus gate at 0.19.10 (the audit's 639.4) and the repin's olympus
   verification. Tick them on #699 after the merge.
6. Rebuild and reinstall the runtime with `deploy/update-stack.sh`, which
   builds the whole workspace at the lock and whose `validate` and `load`
   steps now keep admin's refusal cause (#691). A binary at the old pins on a
   box at the new lock is the mismatch this handoff exists to end. The
   operator has authorized it for Monday. The script honours
   `CARGO_TARGET_DIR`, so give it its own target directory rather than the
   gate's.
7. Prepare, but do not stage, the probe's B1 re-staging (679.4): olympus's
   `e69916a` stack as its recorded runs used it, copied by bytes, link-free,
   with its CUDA user-space libraries named by `ldd` of its engine library.
   The operator ruled the CUDA provenance on 2026-09-26, recorded on #698:
   both stacks stay at `e69916a` and each carries the libraries its runs ran
   under, so B1's toolkit version is the one step 3 measures. **If olympus no
   longer holds the library set its recorded runs used, report it rather than
   choosing**, since the ruling leaves that case to the operator. The stacks
   are not repinned: the kernel leg compares two builds of one source, so
   thinkpad's `tb-b2` worktree stayed at `e69916a` through the candle repin
   and olympus's B1 does too. The approval step (679.5) is being built on
   thinkpad.
8. Run the deployment-tuple baseline on the stack step 6 installed, with the harness
   #716 landed at `experiments/deployment-tuple/baseline/determinism-matrix/`, reading
   its `code/README.md` first. Thinkpad's smoke at `07a7e5d` passed on 2026-09-27, 31 of
   31 sessions reproduced with every load its own unit invocation, and thinkpad's
   seven-hour run3 is the comparison. **Karl is the same agent byte for byte**: copy
   thinkpad's `~/.weaveragents/karl.yaml` verbatim, system prompt included, sha256
   `a2a03d101db2240dbac48ffc394f05fa660292840ac9d6b78f128d12178ff444`, and check that
   `/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf` hashes to
   `2f82233630c349ccf6b8daccf48f9a7865713d9f08a2eadfa456cebe9b97c7f5`. The declaration's
   `devices: [0]` loads one card with no split, so the run binds whichever card CUDA
   numbers 0. Record that card from the smoke's `serving_device` in the box facts, and
   **if it is not an A6000, report it rather than choosing**. The config takes the
   README's fields with olympus's paths and carries no `spu_bin`. Smoke first with
   `--hours 0.03` and read its `summary.json`: errors zero, every held field
   `unchanged`, one device binding. Then the seven-hour run with the README's clock
   recorder and sudo renewal, into one new directory beside thinkpad's under
   `weaver-testing/` on the bulk store, named for the box, the date and the stack
   commit, with a `box-facts.txt` per the Spec's section 6.2 that records the journal's
   retention.

## Left open on purpose

The olympus queue after the gate is #699 in its own order: the RoPE watch on
the single-card path first because vendoring waits on it, then vendoring, the
fork rebase and the rest, with the retirement of the fork's
`weaver/qwen2-intermediates` branch named there as the operator's decision now
that its content is on `integration`. The measurement items are #698's. The
conformance headers are suspended until release, so the weaver-spu units without
one owe nothing now. Nothing here asks olympus to touch HADES.

    Base 07a7e5d
    Documents in this batch
      this handoff  both pins moved under the SPU, the gate is owed on the A6000s,
                    and the baseline run on the reinstalled stack
    Reached by this act from outside the crate in hand
      none  this handoff edits nothing
    Not reviewable in this batch
      none
    Gates run and their result
      none  a reading act
    Asked of the receiving seat
      steps 1 to 8 in order, step 6 on the operator's authorization, one
      pull request for step 5, one for step 6 if the deploy scripts need a
      change, and a report of the measurements and of the baseline deposit
