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

Without cancellation, only the actual recorded refusal advances the existing
failed-correction count. Below the cap, the same rejected source receives its next
restatement request. At the cap, the existing exhaustion path retains the correction
account. Backpressure retains the same author ticket and the Decision bound inherited
from the correction's output check. No empty replacement or invented checker result is
produced. Empty original output, empty Text and attempted calls retain their existing
distinct paths.

Stop and Unload finish a refusal already staged by the repair owner, including one
whose author has not started, as well as one waiting on a recording ticket. They then
return retained cancellation custody through the existing path. The recorded refusal
does not resume counting after cancellation; the correction remains pending evidence.
These paths grant no retry, successful emission or pair. An unstarted restatement
Instruction is suppressed because cancellation must not issue a new decode grant.

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

Durable evidence deposit:
`weaver-testing/state-management-146-olympus-2026-10-21/empty/` on the bulk store.
Its Olympus path begins `/bulk-store/`. It contains mutation logs, SHA-256 restoration
records and the source manifest identifying the tested Rust files. The deposit's
parent `SHA256.json` verifies the copied files. `/tmp/weaver-146-evidence/empty/`
is staging only. Harness gates used isolated Unix sockets, with no deployed agent.

The [Planner review][planner] grades both scoped Implements sections conforms at
fbe313d5316324be0216f0db12f3f7184fa438a9. Its gates reproduce the counts above,
and eight independent protective mutations fail as intended. No finding is fix-bucket.
That grade is distinct from the author and independent Executor checks. Cancellation
count semantics, the shared refusal helper and refusal-wait policy retain the review's
batch dispositions; the wording and evidence corrections here change no behavior.
Production qualification remains separate. Third-party review follows Planner
verification. Only the operator merges. Issue #146 remains open.

[planner]: https://github.com/toddwbucy/WeaverAgent/pull/224#pullrequestreview-5608764905
