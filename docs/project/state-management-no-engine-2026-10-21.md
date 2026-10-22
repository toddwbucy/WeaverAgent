# State member without a compiled engine

Implementation for issue #211, encountered during issue #146.
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
section 2, scoped to the existing run-lock first act; section 3, scoped to
constructing the declared engine behind Store. Author assessment: conforms for
these scoped clauses. The missing-engine startup refusal is a Spec gap pending the
operator's choice on issue #211. The alternative is to require SQLite when building
the binary. The current proposal uses the existing unavailable-store fault vocabulary
and adds no control response or serving capability; that vocabulary alone does not
supply authority for the new startup disposition.

[Planner review 5609164787][planner] confirms sections 1 and 3 and reserves the
startup election to the operator. This record does not claim that election was made.
The code and test proposal remains in draft until the choice and owning Spec agree.

## Instruments and validation

The Linux no-engine process fixture starts the actual Cargo-built executable with
first-door fd3 and absent image fd4. Each argument mode runs with run-lock fd9
present and absent. The absent case requires exactly the existing state_notice line
followed by the missing-engine state_fault line. Moving the engine refusal before
keep_run_lock loses that notice and fails the ordered-record assertion. The parent
keeps fd9 occupied while spawning, so the child's intentional close cannot hit the
standard library's exec-status pipe.

Both descriptor cases require exit status 1, no stdout or seam answer, and no preload
socket. A .part-held sentinel survives, proving refusal precedes the legacy room
cleanup that would otherwise remove it. The child has a five-second test deadline
and is killed and reaped if an assertion fails.

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

## Review followups and conditional gate

The present/absent descriptor fixture answers Planner rows 1 and 4. The current
no-default-feature suite passed 30 tests, with strict all-target clippy, formatting
and lock checks passing. N1 moved the complete engine refusal before keep_run_lock,
compiled and failed the ordered-record assertion. Exact restoration passed. Two
independent source readings found no concrete defect in descriptor custody or the
ten process cases. The earlier matrix and two mutations above describe the prior
fixture. This followup changes tests and documentation only. Its evidence is in
the shared no-engine/review deposit.

If the operator elects startup refusal, add the following regular gate alongside the
existing per-crate test commands, after lock.sh, using a private target directory:

```sh
cargo test -p weaver-state --no-default-features --locked
```

This exact command already ran in the prior author and Planner checks. Making it a
regular gate answers row 3 only after the proposal is approved; no CLAUDE.md or gate
policy is changed by this test followup. The conditional owning-Spec text is prepared
outside the repository at /tmp/weaver-146-no-engine-conditional-spec.md.

Row 5's unreachable no-engine constructor refusal and row 8's shared process-test
helpers remain batch cleanup. Removing the fallback constructor arm would require a
wider feature-boundary edit and is not needed to pin startup ordering. Row 6's
missing-engine refusal before usage validation remains part of the pending election,
not an additional ruling inferred from the test. No production behavior changes in
this followup.

These changes qualify neither an engine-less serving agent nor the state management
cutover. Issue #146 remains open and only the operator merges.

[planner]: https://github.com/toddwbucy/WeaverAgent/pull/230#pullrequestreview-5609164787
