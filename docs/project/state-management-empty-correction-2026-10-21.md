# Empty correction implementation

Date: 2026-10-21. Author verification on Olympus. Advances issue #146, section A's
implementation of merged PR #213. Branch: `code/state-empty-correction-implementation`.
Base: main `c2646e293c8c1bb4376c4b9ea1d41cbebcc080b2`.

## Behavior and authority

A requested correction with no Text and no attempted call now retains its generation
while the harness records an `empty_correction` protocol refusal. The refusal names
the rejected original Text, the correction message's actual causal parent and the
current turn. Its detail is exactly:

`correction contains no Text block or attempted call`

Only the actual recorded refusal advances the existing failed-correction count. Below
the cap, the same rejected source receives its next restatement request. At the cap,
the existing exhaustion path retains the correction account. Backpressure retains the
same author ticket and Decision bound. No empty replacement or invented checker result
is produced. Empty original output, empty Text and attempted calls retain their
existing distinct paths.

Stop and Unload finish an already pending refusal and return retained cancellation
custody through the existing path. They grant no retry, successful emission or pair.

Implements: state-management-spec-02-protocol-classifier-gate EMPTY-CORRECTION and
REPAIR, scoped to this transition. Author grade: conforms.

Implements: weaver-types-Spec-amendment TYPES-INPUT, scoped to the added case and its
strict representation. Author grade: conforms.

The merged rule is authority. The malformed-recall accounting Spec gap remains, as do
the full serving caller migration and production language qualification. No serving
replacement is activated by these private harness primitives.

## Evidence

Private target: `/tmp/weaver-146-empty-target`. Gates ran serially with `--locked`:

- `process/gates/lock.sh`: passed.
- `cargo test -p weaver-types --locked`: 128 passed.
- `cargo test -p weaver-types --features config --locked`: 183 passed.
- `cargo test -p weaver-harness --locked`: 1172 passed, 18 inherited ignored.
- Both affected crates' all-target clippy gates passed with `-D warnings`. The types
  gate included its config feature.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

The new fixtures exercise strict wire bytes, actual recorded refusal attribution,
backpressure, retry and exhaustion, both cancellation causes, and the three distinct
empty-output cases. Fourteen compiled protective probes failed as intended. They cover
case/detail preservation in both crates, source/turn/parent attribution, source block
serialization, early/missing/duplicate counting, record ordering, call priority and
empty-Text classification. Every exact source restoration passed its focused tests.

Local evidence: `/tmp/weaver-146-evidence/empty/`, including the mutation logs and
SHA-256 restoration records. The source manifest beside those logs identifies every
tested Rust file. Harness gates used isolated Unix sockets, with no deployed agent.

An independent source review found no blocking defect in this transition. These
results are an author check. The Planner's grade and production qualification
remain separate. The PR stays draft and only the operator merges it. Issue #146 stays
open, and its checklist is not ticked for an unmerged draft.
