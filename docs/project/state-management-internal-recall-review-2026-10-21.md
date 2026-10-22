# Pure recall query review followup

Date: 2026-10-21. PR #190; issue #146, slice IN1.

The operator's cutover hold remains in force: this draft stays off main until
foundation cutover. Its pure constructor tests do not establish retrieval, inward
pair accounting or serving readiness.

Implements: weaver-internal-Spec-amendment INTERNAL-QUERY and its section 3
constructor enforcement. Both conform within the original pure constructor scope.
HARNESS-RECALL and its consumer remain owed.

## Review dispositions

The followup names E0451 and E0597 on the private-field and borrowed-lifetime
compile-fail examples, moves EmptySubject's specified trait pin into normal library
compilation, extracts unchanged tests to recall/tests.rs, and updates the crate's
calling-surface description. It changes no runtime behavior or public API.

Planner review 5605464642's ten rows are answered as follows:

1. Name and directly verify the two intended compiler errors.
2. Replace the stale function-only crate documentation.
3. Preserve the operator's hold; remove the independent-merge claim from the PR.
4. Defer Display/Error additions to an actual consumer and an owning-Spec change.
5. Add no unspecified Clone/Copy implementation to RecallQuery.
6. Keep derived Debug out of future content logging; no consumer logs this value.
7. Compile the EmptySubject trait requirement in normal library builds.
8. Extract the tests without changing their bodies.
9. Replace the obsolete home-directory target/evidence claim with this host's run.
10. Distinguish the available independent source review from the earlier Planner
    grade; do not claim an unavailable named review tool ran.

No new limit, refusal or state is introduced. The same primitive remains the sole
consumer of its new module, and the harness has no dependency or activation here.

## Evidence

On Olympus, lock.sh, formatting and all-target strict clippy passed. The complete
weaver-internal suite passed: eight unit tests, two manifest tests and two
compile-fail doctests. The private target was
/tmp/weaver-146-internal-recall-target, outside the home directory.

Separate rustc invocations verified E0451 for external field construction and
E0597 for a query escaping its subject. Removing Copy from EmptySubject made a
normal cargo check --lib fail with E0277 at the module-level trait pin. Exact
source restoration then passed that check. The unchanged behavioral tests were
rerun; the original seven behavioral/type probes are historical PR evidence and
were not repeated in this documentation and compile-pin cleanup.

Independent source review found no concrete defect and confirmed the five
extracted test bodies are unchanged. This is an author check; the updated commit
still owes Planner and third-party review.

Logs, direct compiler probes, result metadata and source hashes are deposited in
weaver-testing/state-management-146-olympus-2026-10-21/internal-recall/ on the shared
bulk store. No ThinkPad-local evidence was copied or claimed available here.
