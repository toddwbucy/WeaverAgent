# Serving rollback join

Date: 2026-10-26. Author implementation and measurement record for issue #146.

## Plan and ancestry

The operator folded #340-#345 into #339 before #347 merged into #346 and #346
merged into #345's retained branch. The later tip,
`5042f827617097b7d403a5d75cfa6d2f01b2464e`, therefore still needed joining with
#339 at `80ff3ed5069319b2e50b538ef238cbb80aecb1ef`.

The automatic merge had no Git conflicts. Its tree was
`9db878ca9e53a714277ecdb914704ce9c6337697`. Composition tests found two
integration defects in the shared failed-run cleanup. This act corrects them
and preserves both branches' ancestry on `code/state-serving-rollback-join`.

During verification, the operator merged #339 into #332 at
`91cd3a17da044f3a755493c3f13ccb7eed4132cf`. That commit's tree is identical to
`80ff3ed5`, verified by GitHub comparison and local Git. The final join targets
#332 and uses ordered parents `91cd3a17` and `5042f827`.

## Authority and correction

Authority is merged admin Spec section 3, harness Spec section 6, harness-gate
contract sections 2-3, Spec 02 INPUT, Spec 05 pair cancellation/filing, Spec 06
task/placement/generation cancellation, HARNESS-ENTER-WINDOW, HARNESS-INPUT,
HARNESS-MARKS, HARNESS-CLOCK, HARNESS-RECORD-LIFETIME, HARNESS-HELD-BOUNDARY,
HARNESS-OPEN, HARNESS-HALT and HARNESS-HALT-BOUND, and the trace close rules.
The existing #339 TRACE-CAPACITY, TRACE-REJECTED-RETURN and TRACE-WAKE
corrections remain in the combined tree.

| Integration defect | Correction and boundary |
| --- | --- |
| #347's serving rollback closes its scope, then #339's shared Unload step attempts another SessionClosed from an uninitialized event. | The shared session-close step excludes the rollback scope. Rollback keeps its existing close owner; ordinary failed serving exit keeps its late close. Three actual rollback clock-failure cases reproduced the panic. |
| #339's unavailable-trace shortcut bypasses the rollback decoder account, leaving place_owed set and preventing rollback from advancing. | The shortcut applies only to ordinary failed serving exit. Rollback again advances and retains its original decoder account through its existing failed cleanup. The real broken-trace case reproduced the stall. |

The correction changes two guards, with comments identifying their owners. It
adds assertions to existing composed cases: one SessionClosed followed only by
Unload on the Ready rollback failure, and settled decoder debt on actual trace
failure. It adds no protocol, deadline, retry budget or cleanup outcome.

The author read every constructor of FailedRunCleanup. Startup, early rollback
and expired preflight rollback enter with serving false. Ordinary serving exit
enters with serving true and no rollback scope. Entered rollback enters with
serving true and its distinct rollback scope. The new guards distinguish those
last two owners; their first fatal cause, original cap and resource custody stay
with the original owner. The whole context_worker_exit.rs file remains identical
to #339's accepted H1 head.

## Verification

The automatic union passed all 2,066 ordinary harness tests/doctests but failed
four of 548 delegated worker cases: Ready rollback clock failure, active
generation clock failure, active generation trace failure and placed clock
failure. The other 544 passed. The failed source archive and complete logs are
retained separately from the correction.

All four cases passed after the correction. Three compiled protective controls
were then detected:

| Control | Observed failure |
| --- | --- |
| Restore the second session-close attempt | The Ready rollback case panics on the missing event. |
| Restore the unavailable-trace shortcut for rollback | The actual trace-failure case reaches its cleanup watchdog. |
| Omit the rollback owner's SessionClosed | The Ready rollback case fails the exact-one-close assertion. |

The controlled file was restored byte-for-byte and rebuilt before the final
gates. Final results:

- 2,066 ordinary harness tests/doctests passed: 1,996 library, 43 binary and
  integration, and 27 doctests. The ordinary invocation ignored 612 cases.
- All 548 inventoried worker cases passed in one serial delegated run. This
  includes the four regressions and #339's three rejection-pressure cases.
  Nested child summaries are excluded from the totals. The other 64 ignored
  cases were not run by this check.
- Strict harness clippy for all targets, workspace formatting, diff checks and
  first/last lock gates passed. The known #331 case was included and passed.
- All 981 tested source/configuration/document hashes matched after controls
  and gates. This new measurement record was added afterward and is deposited
  separately from the tested source archive.

The checks use pinned nightly-2026-02-13, locked dependencies and fresh private
target `/tmp/weaver-state-146-rollback-join-target`. SQLite state and admin test
prerequisites were built there. Worker cases use those actual executables under
isolated delegated scopes. Ordinary tests use four threads; worker cases use
one. Cargo, controls and Git mutations are serial. No workspace device suite,
privileged admin startup suite, maximum-concurrency stress pass or production
qualification is claimed.

The corrected source tree before this record is
`9f530938ef49002658f3355feabce7be29b42e16`. Its archive SHA-256 is
`6b758ecdac9454d5da90aac65fa3e5a0e653bfeb779ac299b4f20b50b355db7f`;
the source hash manifest is
`579efe2ada59e5b01d829a15684de30d37284a4be836724381249cba784da3ac`.
Evidence, scripts, controls and both failed and successful runs are deposited at:

```text
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-rollback-join-20261026-6b758ecd/
```

## Review and completion boundary

The Planner accepted #339's H1 and wake correction at `80ff3ed5`. This join
does not close Q1 or its carried batch/file findings, including the later B5/L2.
#346/#347's remaining findings retain their own grades, including rollback
wait behavior, terminal-answer evidence and retired transport handling. The
earlier stack's Spec questions and #348/#349 authority holds remain open.

This advances #146's rollback and failed-run teardown composition; it closes
no broad epic item. Keep the new PR draft for a Planner grade. Foundation
remains unselected; existing serving is still dev_loop::drive. Healthy
Leave/save, remaining admission/author work, dispatcher/process exit, artifact
qualification, security disposition and qualified coupled cutover remain.
These results are author evidence. The code-review and
verification-before-completion skills are unavailable in this session; the
manual reads do not claim those invocations or independent review.

## After-action review

The planned mechanical join required a behavior correction because two
independently reviewed owners shared a serving flag but different accounting
and close duties. A clean merge and the ordinary suite both missed the join
defects; the composed failure cases found them. Keep both failure routes in
the combined gate whenever these owners are consolidated. The guard comments
and constructor census above record that distinction for the next act.
