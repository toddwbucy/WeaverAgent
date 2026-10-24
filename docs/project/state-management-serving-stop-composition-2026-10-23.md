# State management serving Stop composition

Date filed: 2026-10-23
Branch: `code/state-serving-stop-composition`
Issue: #146
Parent: #305 `016a469b84d68caa1c33dcac3d5a8cc92ddbe20c`
Status: completed implementation slice for draft review; goal incomplete.

## Composition and authority

This joins the original root Stop exchange to the entered worker's actual input
owner at the checked-source boundary. The worker adapter services management on the
same run, and a consumed `FoundationTurn::into_service` returns its original source
and gate exchange for management without new source authoring. Stop after the source
records the original directive cause in `turn.closed`, carries the named stopped
world response on the original gate request, and returns the same serving seat only
after complete delivery. A real separate admin process uses the actual Stop client
and the worker/dev_loop adapter in the delegated fixture.

Implements, graded by the author against merged authority:

| Authority | Grade and boundary |
| --- | --- |
| Harness Spec section 2.2 Stop/Observe; admin-harness contract sections 2-3 | Conforms for AtRest and the completed checked-source Stop; active operation cancellation and full lifecycle dispatch remain owed. |
| Spec 02 section 5 INPUT; harness amendment HARNESS-INPUT | Conforms for preserving waiting/checking/accepted/pre-source input and its original gate deadline when no turn is open. |
| Harness amendment HARNESS-OWNERS, HARNESS-AUTHOR, HARNESS-PUMP | Conforms for single-owner author resumption, actual root/gate receipts and backpressure; nested-wait lifecycle judgment remains owed. |
| Harness amendment HARNESS-OPERATION-CLOSE; harness-gate contract | Conforms for the existing named stopped close through `render_close`, the original gate exchange, and original run/turn identity. |
| Admin Spec sections 3 and 7; organ-channel section 1 | Conforms for the shared actual bounded Stop sender, authenticated dial and exact answer correlation. |
| Foundation FOUNDATION-TRANSITION | Conforms for holding replacement serving selection until qualified coupled cutover. |

No new event kind, wire field, pair, workflow election, placement grant, generation
grant or deadline is added. The approved unmerged proposals are not implemented.

## Ownership and progress

Before a recorded turn, Stop returns AtRest. It does not cancel an input check,
consume/revoke acceptance, replace the gate request, restart its bound or author a
turn close. Waiting, checking, accepted and pre-source inputs resume to one original
source. The caller's proposed turn key alone does not establish an open turn.

After the checked source, Stop uses the original turn and cause. The ordinary author
path must return its Recorded close before either response is staged. Root's answer
keeps the exact accepted exchange; the stopped world frame keeps the exact received
gate exchange. Only matching complete-send receipts grant completion. A gate write
that encountered EAGAIN continues through the existing pump's ordinary writable wait
when lifecycle traffic is exhausted. Repeated advancement never authors twice.

An accepted Stop remains owed when its root peer departs, including EOF behind a
complete request and EOF while the close waits for trace capacity. The same author
ticket resumes and the gate response finishes before returning input custody. A
management departure claims no successful admin answer. Nonmanagement failure retains
the full stopped owner. A failed clock observation after a physical send remains a
fatal trace failure. No retry, refreshed bound or successful failed-run cleanup is
inferred from custody retention.

The admin helper is reused by the existing Stop command. It preserves the original
60-second receive bound and existing UID/pidfd/OFD-lock judgment. A wrong exchange,
wrong position or unrelated answer is now Malformed; valid AtRest/TurnAborted answers
and typed refusals are relayed unchanged. This is a correction to the existing client,
not an activation of replacement serving.

## Verification

All Cargo was serial, with `--locked`, lock gate first and private target
`/tmp/weaver-146-boot-composition-target`.

- Admin: 510 passed, 45 inherited ignored.
- Harness: 1,942 passed, 129 ignored in the ordinary suite.
- All 16 new Stop root/delegated circuits pass separately: waiting, checking,
  accepted, pre-source, active, transferred turn, gate/admin pressure, early/accepted
  EOF, gate failure, Recorded-observer failure, next input, actual admin/worker,
  actual recorder pressure and EOF during blocked close admission.
- All 95 prior delegated observation/input/startup circuits pass separately. The
  total of 111 exercised delegated cases leaves 18 inherited harness ignores.
- Affected admin/harness clippy with warnings denied, formatting, initial/final lock,
  diff and three DocsMirror checks pass.
- Thirteen compiled protective controls fail as intended. They cover original cause,
  turn and gate exchange, whole gate/admin sends, driving owed delivery after EAGAIN,
  retaining accepted Stop after EOF, preserving the input token, source round trip,
  admin correlation/position/answer kind, and record-before-answer ordering.
- External compiler checks pin the public consuming handoffs and E0616/E0507/E0277
  protections. Restored source hashes are verified, rebuilt and checked after the
  mutations (336 affected Rust hashes), including the full harness suite and new Stop/previous Observe circuits.

The actual recorder queue is filled by synthetic measurement records for ordering
checks. Sockets, forks, root peer authentication and delegated cgroups are actual;
classifier/model/state responses and timing remain test doubles. The member fixture
now answers actual File requests when Stop returns to intake and the next input
continues the same run. None of these results qualifies production artifacts or limits.

Initial failure logs are retained: a real gate-write stall was fixed by continuing the
sole pump; the first fixture used `kind` instead of the close payload's `close` member;
resumed intake exposed a missing File response in the member double; accumulated-byte
rescanning made the first trace-drain instrument exceed its watchdog. Compile/lint
instrument corrections are also retained. Controls compile before their expected
failure and restore every affected Rust source hash.

The source snapshot, gate logs, protective controls, compiler pins and direct author
review accompany this slice's evidence deposit. These are author checks, not an
independent Planner/security grade. Review/verification skills are unavailable in this
environment. Publication keeps one parent and the existing stack ancestry.

## Remaining composition

Stop here reaches a completed checked source before any active generation/tool owner.
It is not general in-flight-operation cancellation. Remaining joins include lifecycle
judgment inside nested waits, generation/tool/plan operation ownership and cancellation,
Leave/SavePoint, failed-run teardown, one coupled admin-Enter/worker run, lifecycle and
author migration, active serving selection and production qualification. Existing
serving stays selected. Only the operator merges or deploys. This slice closes no
whole Issue #146 checklist item and makes no full implementation-completion claim.
