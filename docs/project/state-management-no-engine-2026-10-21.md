# State member without a compiled engine

Date: 2026-10-21. Implementation for issue #211, encountered during issue #146.
Branch: `code/state-no-engine-refusal`, stacked on draft PR #229 with the
reviewed observation, trace and capture corrections retained as merge ancestry.

## Behavior and authority

The optional SQLite feature remains optional. The legacy startup path now calls a
feature-aware store constructor, so disabling default features does not leave an
unresolved SQLite reference. A build without SQLite marks the inherited run lock
close-on-exec first, then exits unsuccessfully with the existing structured
state_fault and StoreUnavailable vocabulary. It never reads the first door, adopts
an image, opens or cleans the room, or stands a preload door. This applies to legacy
arguments and explicit serving, diagnostic and seed modes.

Default builds retain the existing dispatch, descriptor checks, room opening and
SQLite construction order. There is no replacement engine or fallback store.

Implements: weaver-state-Spec section 1, scoped to the optional engine feature;
section 2, scoped to the run-lock first act and startup refusal; section 3, scoped
to constructing the declared engine behind Store. Author assessment: conforms for
these scoped clauses. Planner review remains separate. The Spec
keeps the engine behind a named feature and elects only SQLite for this deployment.
An absent implementation follows the existing unavailable-store fault path; it
adds no control response or serving capability.

## Instruments and validation

The Linux no-engine process fixture starts the actual Cargo-built executable with
first-door fd3, absent image fd4 and run-lock fd9. For each argument mode it requires
exit status 1, the structured missing-engine fault, no stdout or seam answer, and
no preload socket. A .part-held sentinel survives, proving refusal precedes the
legacy room cleanup that would otherwise remove it. The child has a five-second
test deadline and is killed and reaped if an assertion fails.

The live-serving run-lock integration fixture now requires SQLite, as its process
must stay alive serving a store. The engine-independent run-lock unit fixture still
runs without SQLite. Shared engine-only test helpers follow their SQLite callers'
feature guard.

Serial checks on Olympus used /tmp/weaver-146-no-engine-target, --locked and the
lock gate before Cargo. The completed state matrix is:

- Default features: 203 tests passed, 8 inherited ignored; all-target clippy passed.
- No default features: all-target check, 30 tests and strict all-target clippy passed.
- No default features with sqlite explicitly enabled: check and the serving
  run-lock process fixture passed, confirming the explicit feature selects the engine.
- Formatting, lock and diff checks passed.

Protective changes for the no-engine process fixture:

- Change the early missing-engine refusal exit to success. The status check fails.
- Remove the early engine check. The legacy constructor still refuses, but
  Room::open deletes the sentinel, so the custody check fails.

Both mutations compiled and failed the named fixture; exact source restorations
passed their focused reruns. Removing the constructor's SQLite compile guard is a build
failure and is not counted as a compiled behavioral mutation.

The first no-feature lint found an SQLite-only FileFault constructor outside its
feature guard. That constructor now follows its only callers; the complete no-feature
test/lint checks passed afterward. The failed lint log is retained. Independent
source review found no concrete defect in startup ordering or the fixture.

Evidence, tested source hashes and mutation/restoration records are deposited at
`weaver-testing/state-management-146-olympus-2026-10-21/no-engine/` on the shared
bulk store. /tmp/weaver-146-evidence/no-engine is local staging.

These changes qualify neither an engine-less serving agent nor the state management
cutover. Issue #146 remains open and only the operator merges.
