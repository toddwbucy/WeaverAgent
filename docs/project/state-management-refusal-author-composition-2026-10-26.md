# Refusal author custody through cancellation

Date: 2026-10-26. Author working record for issue #146.

## Plan and authority

This act is stacked on #351 at
`45cf1dc3f0c6eeaf6b83b6e0914decfbfbaa0217`, on
`code/state-refusal-author-composition`. It completes held-original handling
for wind-down and emission refusal authors and corrects the idle wait in
the same class of wind-down start/close authors.

Implements: harness state management amendment HARNESS-RECORD-LIFETIME,
HARNESS-HELD-BOUNDARY and HARNESS-CLOCK; HARNESS-HALT and
HARNESS-HALT-BOUND for retained originals and first-failure precedence;
harness Spec section 6 wind-down and force joining a pending Leave.

| Section | Author assessment at the exercised boundary |
| --- | --- |
| HARNESS-RECORD-LIFETIME | Conforms for the two refusal callers: the same raw event moves back to its caller after an inspection hold; an admitted author retains its existing resume path and actual receipt. |
| HARNESS-HELD-BOUNDARY | Conforms for the exercised inspection holds: the inspection keeps its answer and only the exact failed author attempt reclaims its original. This does not settle permanent holds in #351 B1. |
| HARNESS-CLOCK | Conforms for these callers: holds preserve the original Decision deadline; actual local reduction precedes a wait, and both Pending and Idle request a bounded wait. |
| HARNESS-HALT / HALT-BOUND | Conforms for unchanged failure custody: fatal author errors retain StoppedAuthor handling; no held recovery clears the first failure or gives a new halt allowance. Full failed-exit ordering remains #339 F1. |
| Harness Spec section 6 | Conforms for the exercised force conversion: an unadmitted refusal remains owed and is Recorded once before the wind-down turn closes, including force before or after its author completes. |

Foundation remains unselected. The production worker still passes
`dev_loop::drive`; no serving cutover or production qualification is claimed.

## Implementation and caller census

The two refusal authors arm Decision only if absent, retain their exact
event after `reclaim_held`, reset the unadmitted started flag and repeat
ordinary admission on a later step. Wind-down recognizes its prepared
refusal before continuing the phase. Stop and Unload keep an emission
refusal in its existing accounting phase. A successful author saves the
actual sequence and clears its started flag before continuation.

| Production caller | Original and continuation |
| --- | --- |
| `context_worker_wind_down.rs::record_refusal` | Retains the original refusal through repeated inspection holds and force; no instruction, placement or generation is admitted by reclamation. |
| `context_worker_emission.rs`, RefusalAuthor | Retains the same raw refusal through holds, Stop and Unload; its successful continuation remains Refused or CancelAccount. |
| `context_worker_wind_down_cancel.rs::record_turn` | Existing start/close reclamation now waits on Idle as well as Pending. The other two callers use the same condition. These are all three production reclamation sites. |

This addresses #351 B2, the idle part of B8, and the wind-down refusal
site in B6. Other author migration and a shared all-author helper are not
claimed. #351 B1 (permanent holds), B3 (unexposed displaced evidence), B4
(changed force cause), B5 (slot representation and allocation comments)
and B8's permanent-hold coverage remain carried before activation. For B7,
the gate prerequisites below are explicit; the Planner-owned generic
command list in CLAUDE.md is unchanged.

## Verification

The final source passed 2,108 harness tests and doctests (702 ignored by that invocation), plus 235 delegated cases: graceful tail 51, wind-down 19, Quiesce 12, drain 10, external pairs 64, sessions 59, Stop 17 and forced joint 3. Strict harness clippy for all targets, formatting, diff and lock gates passed. All 728 source manifest entries were unchanged through the final gates.

The ordinary emission case uses real inspection exchanges and recorder
receipts, with a deterministic readiness clock. It retains exact raw bytes
and allocation identity through three inspection owners, each tried before
and after its completion is ready but untaken, then admits one refusal.
It covers no cancellation, Stop and Unload. The existing fatal second-submit
case remains in the harness suite.

The six normal SQLite held-start, held-start-resume, held-close,
held-close-resume, held-refusal and held-refusal-resume cases pass. The two
refusal scenarios omit the wind_down family from the real fixture inventory,
so the runtime produces InvalidTemplate. They exercise actual Lower,
capture, release and Left after the original refusal is Recorded. The
model and classifier peers remain declared doubles.

Before the runtime change, two compiled fixtures failed with InspectionOwned.
Before the idle correction, both strengthened fixtures failed at the
idle-owner assertion. Six semantic controls at the earlier source detected
missing reclamation, skipped force accounting, renewed Decision bounds and
missing local reduction. Three further controls at the final source reverted
each caller's idle wait independently; all compiled and failed at the named
assertion. Runtime bytes were restored and the library rebuilt after each
group; the final 728-entry source manifest was then verified.

One close fixture initially assumed the flow was already Idle after the
inspection completed. It was still Progress. The final fixture reduces
that actual remaining work before probing the idle owner. This setup
failure is excluded from the semantic controls. The process idle probe
records the requested blocking wait and returns a spurious wake carrying
no transport result; it is not a CPU-usage measurement. The fixture elects
the inspection interleavings, not the selected production worker.

Runs used pinned `nightly-2026-02-13`, `--locked`, private target
`/tmp/weaver-state-146-held-leave-target`, `CARGO_INCREMENTAL=0` and
`CARGO_BUILD_JOBS=4`. The target already held the normal SQLite
`debug/weaver-state` binary and the actual admin test binary
`debug/deps/weaver_admin-5e029c23e2707887`. Those binaries are prerequisites
for the harness state-flow and joint suites. The joint tests also set
`WEAVER_LEAVE_ADMIN_TEST_BIN`; Stop sets `WEAVER_STOP_ADMIN_TEST_BIN`.
Privileged process cases ran as real root in isolated
`systemd-run --scope -p Delegate=yes` scopes. A user namespace alone
does not run the actual admin joint participant.

Cargo, tests and mutations ran serially. These are author checks, not a
Planner or security grade. The required `code-review` and
`verification-before-completion` skills are unavailable in this session;
the evidence names the manual author review as such. No new workspace,
admin, SPU or GPU gate is claimed.

## Evidence and remaining work

The read-only deposit is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-refusal-author-composition-20261026-4f8400ac/`.
All 647 regular files were copied and independently reread with zero skips.

- Complete tested source archive SHA-256:
  `4f8400ac9b1c6a654a7fefef4278aebb8ed4b04e927e65407c77c6a32025d01c`.
- Source manifest SHA-256:
  `61236c0cb7462d83829d54af46dfe4c1718730cfb033c8a6e1d90a4786b075ed`.
- Deposit manifest SHA-256: `baeffcf2f690bc2be4c5aabb7a4a10b570584682b8044bcd8a927cdf9c5af5e4`.

The source archive contains 1,025 files; this publication record was added
afterward. The deposit retains baseline failures, source snapshots, controls,
the fixture correction, final gates and manual author review.

This moves #146 section E's lifecycle and author migration work. No whole
epic item closes. #339 F1 is the next fix: settle owed turn accounts before
their closures and preserve terminal ordering. #348/#349 retain their
operator, Planner and merge gates. On-demand save, other review carries,
production artifacts, measured limits, serving selection and coupled
cutover remain required.

The planned refusal migration exposed the same idle wait that Claude
identified in #351. The final read covered all three callers, and the tests
now distinguish locally reducible work from an untaken completed owner.
Keep that distinction in later author migrations; do not infer a blocking
wait from a test that makes every descriptor ready.
