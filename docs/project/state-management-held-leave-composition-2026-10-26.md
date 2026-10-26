# Held wind-down author custody

Date: 2026-10-26. Author working record for issue #146.

## Plan and authority

This act is stacked on #350 at
`ac5021432f90bcf4890a16e1161cfef2f87e1f39`, on
`code/state-held-leave-composition`. It gives the wind-down start and close
author their exact unadmitted event back after an existing owner holds
admission. A later explicit attempt uses that event and the original bound.

Implements: harness state management amendment HARNESS-RECORD-LIFETIME,
HARNESS-HELD-BOUNDARY and HARNESS-CLOCK; HARNESS-HALT and
HARNESS-HALT-BOUND for retained originals and first-failure precedence;
harness Spec section 6 wind-down and force joining a pending Leave.

| Section | Author assessment at the exercised boundary |
| --- | --- |
| HARNESS-RECORD-LIFETIME | Conforms: the failed attempt alone gets a private return identity. Reclamation moves the same unadmitted event, never another caller's event or a recorder ticket. The ordinary tests check foreign contexts, replacement slots and both held slots. |
| HARNESS-HELD-BOUNDARY | Conforms: reclamation removes only the exact hold. Every subsequent attempt still passes ordinary admission. An existing inspection keeps its completion and the caller keeps its event through repeated holds. |
| HARNESS-CLOCK | Conforms: the wind-down Decision bound survives each hold. An actual locally held member reply is reduced before a blocking wait; the inspection owner still takes its answer. |
| HARNESS-HALT / HALT-BOUND | Conforms for custody: taking an original in Failed preserves the first failure and every deadline. This is not a claim that all production author callers have completed failed teardown. |
| Harness Spec section 6 | Conforms at start/close conversion: force before an unadmitted start invents no trace turn. Force before an unadmitted close retains that close as evidence and authors the existing stopped-unload outcome. Admitted and Recorded closes retain their original continuation. |

Foundation remains unselected; production serving still uses `dev_loop::drive`.
The complete implementation and qualification goal remains open.

## Implementation and caller census

`AuthorFailure` and `RecordingContext` in `context.rs` share a private identity
for one held original. The identity is move-owned by the failure and is not
a trace field, queue or admission permit. An old identity cannot take a
replacement original even after the existing unkeyed taker has emptied its
slot. Taking the exact original in Failed does not revive the context.

| Existing sites | Change and remaining scope |
| --- | --- |
| Eleven ordinary hold rungs: place, plan summary, boot Open, boot pairs, marks, freshness, inspection, read, repair, measurement and pair ownership | Mint an identity only when the original enters the empty caller-held slot. An occupied slot still returns the offered event. |
| Template hold and resumed-author owed-account hold | Use the same exact-attempt identity for their existing slot. The simultaneous first-fatal transfer clears it. These complete the thirteen hold-admission sites. |
| Ordinary take, template take, first-fatal transfer | All three existing production removal paths clear the associated identity. Reclamation clears its own identity after moving the event. |
| `context_worker_wind_down_cancel.rs::record_turn` | The only new production reclamation caller. Retain start/close once, reclaim after a hold, reduce admitted local work and retry only on a later owner step. Preserve a displaced unadmitted close during force. |
| Freshness, pair and rollback-gate callers | Structural adaptation to the additional private field only. Their handling is not newly completed by this act. |

`context_worker_wind_down.rs` owns the pending and displaced turn events.
The ordinary custody cases are in `context_held_input_tests.rs`; full SQLite
cases are registered in `context_worker_startup_tests.rs` and exercised by
`context_worker_wind_down_conversion_tests.rs` with the forced-Leave tail.

## Verification

At source archive `c68e049a`, the ordinary harness suite passed 2,107 tests
and doctests, with 700 cases ignored by that invocation. The separate
delegated run passed 233 cases: graceful tail 49, wind-down 19, Quiesce 12,
drain 10, external pairs 64, sessions 59, Stop 17 and forced joint 3.
Strict harness clippy, formatting, diff and lock gates passed.

The final source changes only the wind-down conversion fixture from that
snapshot. All other 727 entries in the 728-file source manifest are
byte-identical, including every changed runtime file. The final fixture
receives the normal SQLite member's actual inspection reply before the
author step and rejects a blocking wait while that reply remains local.
It settles any preceding content filing through the common reducer and
readiness wait before admitting the inspection.

After restoration, the two custody tests and all four SQLite held-start,
held-start-resume, held-close and held-close-resume cases passed again.
Final strict harness clippy for all targets, formatting, diff and lock
checks passed. These are reruns of cases counted above, not added coverage.
The full process cases deliberately interleave inspection and wind-down
owners in the fixture; they do not establish that the selected worker
produces that interleaving. Model and classifier peers are declared doubles.

Six semantic mutations compiled and failed at inspected assertions. Four
ran at the earlier fixture: foreign identity, skipped reclamation, rebuilt
original and discarded displaced close. Removing local reduction initially
survived that fixture. The strengthened final fixture detects it before a
blocking wait; it also detects renewal of the Decision bound. A duplicate
evidence-directory setup failure executed no test and is excluded. Both
controlled runtime files were restored byte-for-byte and the original
library rebuilt before the final tests.

Runs used pinned `nightly-2026-02-13`, locked dependencies and private target
`/tmp/weaver-state-146-held-leave-target`, with `CARGO_INCREMENTAL=0` and
`CARGO_BUILD_JOBS=4`. Cargo, mutations and tests ran serially; process tests
used isolated root-owned delegated scopes. No new workspace-wide, admin,
SPU or GPU gate is claimed.

These are author checks, not Planner, security or production qualification.
The required `code-review` and `verification-before-completion` skills are
unavailable in this session; the manual author review is named as such in
the evidence.

## Evidence and remaining work

The read-only deposit is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-held-leave-composition-20261026-8149e97a/`.
All 462 regular files were copied and independently reread with zero skips.

- Final complete source archive SHA-256:
  `8149e97a6430d6ef765b145fb09113be1e8ef4e2c4b634ba01bb788be8a93fa8`.
- Final source manifest SHA-256:
  `72a78ff11b728a55ec4511791dec9b908d7807443cde3e308287bfc4850acf79`.
- Deposit manifest SHA-256:
  `5925f5187b5f9f7b090d1a2db11d2c691101ddd9697d459b302b38270313f70a`.

The archive contains 1,024 files. This review record was added afterward;
the archive and manifest identify the tested source before publication.
Development failures, the earlier survivor, final controls, source deltas
and the final manual author review remain in the deposit.

Other production author callers, including wind-down refusal authoring,
still need their own held-original integration. #348 and #349 retain their
existing operator, Planner and merge gates. On-demand save, complete
serving selection, language/prompt artifacts, measured limits, review
carries and coupled cutover remain under #146. No whole epic item closes.

The planned custody extension exposed a weak progress test: transport
readiness let a missing local reduction pass. The final fixture separates
receipt from reduction and checks that boundary directly. Its first close
setup also spun without requesting ordinary socket readiness; it now uses
the real pump. Keep source snapshots and failed instruments, publish each
verified increment, and distinguish fixture reach from selected-worker
reach when extending this composition.
