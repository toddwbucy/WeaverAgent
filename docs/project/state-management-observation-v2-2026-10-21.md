# State observation version 2

Date: 2026-10-21. Author verification on Olympus. Advances issue #146, section A's
implementation of merged PR #192. Branch: `code/state-observation-v2`, stacked on
PR #224 at `fbe313d5316324be0216f0db12f3f7184fa438a9`.

## Behavior and authority

The member retains the monotonic timestamp from before operation hint parsing. The
operation interval ends after payload encoding. Open and later serving replies use
one completion function, including oversized-control refusal encoding and page cleanup.
Snapshot CPU and disk samples have a separate scope: after validation and capture
preflight, immediately before image serialization, through capture work and recoverable
cleanup, before answer construction.
Preflight refusals take no resource samples. Partial sends reuse the finished evidence.

New opened responses and stored current-run identities require state-observation/2.
Historical adoption, store queries and harness validation accept the historical /1
identity or /2 without relabeling either. Unknown versions refuse. Explicit current
and historical validation methods replace the formerly ambiguous identity validator.

Implements: state-management-spec-09-qualification-costs OBSERVATION-IDENTITY,
STORE-OBSERVATION and SNAPSHOT-SAMPLING, scoped to timing and version identity.
Author grades: conforms for these implemented clauses.

Implements: weaver-types-Spec-amendment TYPES-OBSERVATION-READ, scoped to current
open and historical identity validation. Author grade: conforms.

The separate [buffer-accounting slice][byte-accounting] supplies local byte readings
where its inventory is complete. Issue #221 retains the remaining copy instrumentation.
Timing instrumentation does not qualify production resource limits, a save-window
allowance or the state management cutover.

## Evidence

Private target: `/tmp/weaver-146-observation-target`. The following baseline at
01479f057ea54d81c81b5c5a4912986781b8ee29 used serial gates with `--locked`.
The completion-path review correction is covered separately below.

- `process/gates/lock.sh`: passed.
- `cargo test -p weaver-types --locked`: 129 passed.
- `cargo test -p weaver-types --features config --locked`: 184 passed.
- `cargo test -p weaver-state --locked`: 200 passed, 8 inherited ignored.
- `cargo test -p weaver-harness --locked`: 1173 passed, 18 inherited ignored.
- All-target clippy passed for types with config, state and harness, using
  `-D warnings`.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Tests drive the real parser, Session and capture paths with deterministic counter
samples. They cover success, the Session's refusal while capture is held, the
snapshot-local zero-ID preflight refusal, injected serialization failure, an actual
room-write collision and cleanup, and unavailable counters. They do not claim to reach
Capture::snapshot's defensive held() arm through Session.

The serving-process regression checks actual payload-encoder timestamps before run()
records and sends a snapshot success and a held-capture refusal. The shared completion
fixture exercises both complete-answer bound failures and payload-encoding failures,
including initial-page discard and retained-cursor retry. The partial-send fixture
checks an already finished PendingAnswer's byte-carry property; it does not exercise
the serving send path or qualify transport timing. SQLite adoption and query fixtures
preserve both historical versions. These tests concern sampling order and evidence
propagation, not the host kernel's measurement accuracy.

Thirteen compiled protective mutations failed as intended. They move the wall anchor,
move or omit capture samples, sample during preflight or answer completion, accept old
or unknown current identities, relabel historical rows, or reject historical reads.
Each exact source restoration passed its focused test.

The completion-path correction passed 202 state tests (8 inherited ignored), 138 trace
tests, both affected all-target clippy gates, formatting and lock checks. Five compiled
protective probes failed: encoding before the Encode phase, finishing before encoding,
retaining an initial hidden page after a refusal, dropping an existing cursor, and
skipping the complete-answer bound. Each exact source restoration passed its focused
test. The real serving process now catches both formerly surviving ordering mutations.
An independent source review found no defect in the shared completion or page cleanup.

The [completion-path re-grade][completion-review] at 1537d4df confirms the two ordering
guards and identifies one surviving wiring mutation: run() passing None instead of its
CompletionBounds. A new serving-process fixture addresses that gap with real input:
a valid filing retains a turn identity larger than the answer limit but below the
request limit. Its Filed receipt succeeds. A later Position answer exceeds the encoder
bound and must produce AnswerTooLarge through run()'s fallback. A previously acquired
prompt-read cursor remains releasable after that refusal. No encoder failure is
injected. The existing child timing markers also check the fallback's observation.
The new fixture passed in a 203-test state suite (8 inherited ignored), with state
all-target clippy, lock and formatting. Disconnecting run() from CompletionBounds
compiled and failed this process fixture; exact restoration passed. An independent
source review found no defect in the new fixture. The test does not force an oversized
generated page; page_budget already reserves the complete
wrapper and maximum observation before the base64 page is built.

Logs and SHA-256 source and restoration manifests are deposited on the shared bulk
store under
`weaver-testing/state-management-146-olympus-2026-10-21/observation/`.
The local staging path is `/tmp/weaver-146-evidence/observation/`.

The [Planner review][planner] grades the scoped clauses conforms at 01479f05 and
identifies the serving completion ordering as correct by reading but unpinned by
tests. This correction gives that serving path one tested completion function and
adds the real-process timing assertions and the protective evidence above. The stale
current-load trace fixture now uses /2; historical readers
still accept explicitly historical /1 evidence. Injection-hook consolidation and
counter/phase classification retain their review dispositions. Only the operator
merges, and issue #146 remains open. No deployed agent or serving activation changed.

[planner]: https://github.com/toddwbucy/WeaverAgent/pull/225#pullrequestreview-5608802535
[completion-review]: https://github.com/toddwbucy/WeaverAgent/pull/225#pullrequestreview-5608907601

[byte-accounting]: state-management-buffer-accounting-2026-10-21.md
