# Serving recall records

Date: 2026-10-21. Author verification on Olympus. Advances issue #146, section A's
recall representation work from merged PR #189. Branch: `code/state-recall-records`,
stacked on PR #225 at `01479f057ea54d81c81b5c5a4912986781b8ee29`.

## Behavior and authority

The serving trace now accepts its own serialize-only RecallRecord, with closed read
and boot variants. It reuses ReadSelector, ContentRef and SpanRef directly, plus
StatementRef and PlanRef inside the selector shapes. It adds no Weaver crate dependency.
Admission checks context-free selector constraints, positive snapshot IDs, source and
span identities, unique sources, increasing binding ordinals,
cited nonempty intervals and boot Message/end agreement. Repeated selected occurrences
remain valid. The licensed envelope is recall/internal/runtime/harness with no turn.

Boot carries a canonical complete system Message with one Text block. Its stored end
is the decoded UTF-8 byte length. Seed, ordinal, selected interval and end-mark
identities must agree. Trace validation makes no store query and grants no material
or placement authority. Both variants remain forbidden from partition selection and
state copies.

Implements: weaver-trace-Spec-amendment TRACE-RECALL, scoped to typed representation,
envelope and partition admission; TRACE-RECALL-CHECK; TRACE-RECALL-BOOT, scoped to the
trace's structural checks. Author grades: conforms for these clauses.

The harness traversal of completed QueryResult, Release-before-Recorded ordering,
HeldMaterial, first-content-read BootHead and serving integration are separate owners.
This slice does not claim TRACE-RECALL-SOURCES or HARNESS-RECALL implemented.

The legacy diagnostic converter accepts legacy Event, not serving Event. Compile-time
pins prove the new payload cannot enter its old RecallAccount arm, but do not implement
or test the required runtime OutsideDiagnosticVocabulary refusal. That conversion
obligation remains with author migration under issue #146. No old serving path changes.

## Evidence

Private target: `/tmp/weaver-146-recall-target`. Serial gates used `--locked`:

- `process/gates/lock.sh`: passed.
- `cargo test -p weaver-trace --locked`: 150 passed.
- `cargo test -p weaver-harness --locked`: 1173 passed, 18 inherited ignored.
- `cargo clippy -p weaver-trace --all-targets --locked -- -D warnings`: passed.
- Formatting and diff checks passed before the successor harness work began.
- After adding specific error codes to the two new compile-time pins, all 19 trace
  doctests passed again.

Fixtures cover every selector's exact bytes, both record variants, source identity,
repeated spans, structural refusals, canonical Unicode boot bytes and real Recorder
admission. Invalid records preserve sequence, and valid records produce exact sink
bytes with no state copy. Optional causal parent remains absent when unavailable.

Sixteen compiled protective changes failed their fixtures, each followed by exact source
restoration and a passing rerun. An initial adjacent-only duplicate rejection did not
affect the intentionally nonadjacent repeated-span fixture. The corrected all-prior
duplicate rejection failed as required; both probe results remain in the evidence.

[Planner review][planner-review] found that removing the ContentRef variant tag survived
the source-identity fixture. The review follow-up adds a seed and an event with
matching other key fields. Their distinct variants must remain admitted together.
The review correction passed 150 trace tests, all-target trace clippy, lock and
formatting checks using /tmp/weaver-146-trace-review-target. The tag-removal
mutation compiled and failed the intended assertion; exact restoration passed.
No production validation rule changed.

Durable evidence is deposited under the shared bulk-store path
`weaver-testing/state-management-146-olympus-2026-10-21/recall/`.
Source and evidence manifests identify tested bytes. The local staging path is
`/tmp/weaver-146-evidence/recall/`.

Independent source review found no concrete defect and explicitly left harness source
traversal and runtime diagnostic conversion ungraded. These author checks do not replace
the Planner's grade or production qualification. This PR is submitted as a draft for
the operator's review. Issue #146 remains open and only the operator merges.

[planner-review]: https://github.com/toddwbucy/WeaverAgent/pull/226#pullrequestreview-5608872660
