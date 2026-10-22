# Recorded read provenance owner

This slice advances issue #146 section A, implementing the nonboot read part of
merged PR #189. Branch `code/state-recorded-read-owner` is stacked after PR #228.
It also carries the observation and trace review corrections by merge ancestry.

## Behavior and authority

The paged reader keeps the exact validated result until the matching Released
answer. The recording context then retains its source traversal and one recall
author ticket until Recorded. Only that completion returns a private RecordedRead
for inspection. It grants no placement, boot admission or model generation.

The original StateControl deadline covers paging, release and recall recording.
The owner saves Released and Recorded custody before recoverable lifecycle checks.
Generic author/resume and reduction ports cannot take the recall ticket. Cancellation
revokes consumption while the owned exchange and owed recording finish under the same
bound; its result exposes disposition without transferring material authority.
Refused reads preserve their refusal and release any acquired cursor. They do not
become an empty successful recall.

Traversal follows the owning table in order. It preserves original source identities,
binding ordinals, pair opening/terminal sources, step references, plan provenance,
mark endpoints and equal forward-flag boundaries. Sources are deduplicated at first
occurrence with sorted binding unions. Selected span occurrences remain ordered and
may repeat. Observation metadata contributes no content source or span.

Implements: weaver-harness-Spec-amendment HARNESS-RECALL, scoped to released
nonboot reads, exact attribution, recording and continuation ownership.
Author grade: conforms for this scope.

Implements: weaver-trace-Spec-amendment TRACE-RECALL-SOURCES, scoped to the
harness traversal and shared concrete result/record corpus. Author grade: conforms.
The trace retains its structural checks and does not access the state store.

HeldMaterial, first-content-read BootHead, pair-owned inward reads, serving consumers
and runtime diagnostic conversion remain separate issue #146 obligations. A later
system_prompt Read is not retrospectively a Boot record. Production cutover is held.

## Evidence

Author checks use Olympus, private target /tmp/weaver-146-recall-target, serialized
Cargo with --locked and the lock gate first. The joined source passed 1188 harness
tests (18 inherited ignored), 151 trace tests, both affected all-target clippy gates,
formatting and lock checks. Twelve compiled protective mutations failed; each exact
source restoration passed. They cover provenance traversal, metadata exclusion, original
deadlines, cancellation and exclusive author/reduction ownership. Compile-time pins
reject Clone for RecordedRead and CancelledRead; both deliberate implementations
failed with E0283 and exact restorations passed their focused test and lint checks.

The 18-case literal JSON corpus is consumed independently by the real harness page
decoder/release path and the trace event admission/serialization path. Fixtures cover
every selector and empty results. Real socket/recorder tests cover release-before-use,
trace backpressure, one author ticket, cancellation, original deadlines and lifecycle
interruptions on either side of Recorded. Independent read-only review found no
concrete runtime defect in the source traversal or continuation custody.

The initial corpus exposed an invalid observation fixture: it named a request source
on a read operation. It now uses a filing operation and still requires empty recall
sources. A later parallel suite run exposed an inherited classifier EOF fixture race:
another fork can temporarily retain the peer. That test now uses its existing bounded
kernel-terminal helper before its one production probe, as the neighboring tests do.
No production classifier or validation rule changed. Both failed runs are retained.

Logs and exact tested-source/restoration hashes are deposited on the shared bulk store
under `weaver-testing/state-management-146-olympus-2026-10-21/recorded-read/`.
The local staging path is /tmp/weaver-146-evidence/recorded-read. These are author
checks, not Planner verification or production qualification. Submitted as a draft;
only the operator merges and issue #146 remains open.
