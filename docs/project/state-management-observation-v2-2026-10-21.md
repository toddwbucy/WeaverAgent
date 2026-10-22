# State observation version 2

Date: 2026-10-21. Author verification on Olympus. Advances issue #146, section A's
implementation of merged PR #192. Branch: `code/state-observation-v2`, stacked on
PR #224 at `fbe313d5316324be0216f0db12f3f7184fa438a9`.

## Behavior and authority

The member retains the monotonic timestamp from before operation hint parsing. The
operation interval ends after payload encoding. Snapshot CPU and disk samples have a
separate scope: after validation and capture preflight, immediately before image
serialization, through capture work and recoverable cleanup, before answer construction.
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

Copied-byte and held-byte accounting remains null with an uninstrumented reason.
Issue #221 owns its implementation. Timing instrumentation does not qualify production
resource limits, a save-window allowance or the state management cutover.

## Evidence

Private target: `/tmp/weaver-146-observation-target`. Serial gates used `--locked`:

- `process/gates/lock.sh`: passed.
- `cargo test -p weaver-types --locked`: 129 passed.
- `cargo test -p weaver-types --features config --locked`: 184 passed.
- `cargo test -p weaver-state --locked`: 200 passed, 8 inherited ignored.
- `cargo test -p weaver-harness --locked`: 1173 passed, 18 inherited ignored.
- All-target clippy passed for types with config, state and harness, using
  `-D warnings`.
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Tests drive the real parser, Session and capture paths with deterministic counter
samples. They include success, both preflight refusals, injected serialization failure,
actual room-write collision and cleanup, unavailable counters and partial answer sends.
SQLite adoption and query fixtures preserve both historical versions. The tests prove
sampling order and evidence propagation, not the host kernel's measurement accuracy.

Thirteen compiled protective mutations failed as intended. They move the wall anchor,
move or omit capture samples, sample during preflight or answer completion, accept old
or unknown current identities, relabel historical rows, or reject historical reads.
Each exact source restoration passed its focused test. Local logs and SHA-256 source
and restoration manifests are under `/tmp/weaver-146-evidence/observation/`.

An independent source review found no runtime defect. Its requested zero-capture-ID
preflight fixture was added and its protective mutation failed. The Planner's grade
remains separate. This PR stays draft, only the operator merges, and issue #146 remains
open. No deployed agent or replacement serving loop was changed.
