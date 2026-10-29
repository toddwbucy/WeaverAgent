# Residency fixture after decode closure

Date: 2026-10-29. Author evidence for #146 composition checks and Planner Y1
from #383 and #396. Base: draft #397 at
52d189af58757fae7dc37ef408a526fd6699eef1.

## Scope and authority

Implements: weaver-harness Spec section 3, and weaver-harness-spu-contract
sections 2 and 3, for the test peer's residency lifetime and Release ordering.
Conforms on the exercised fixture paths. This act changes one harness test
module and this record. No runtime behavior, protocol, deadline, refusal or
production selection changes. FOUNDATION-TRANSITION remains unchanged.

The Planner's #396 review 5695982334 captured worker_plan_flags_turn_boundary_leave
stopping with an entered rollback Release completion whose fault was Closed,
detail "packet peer closed". Eight isolated repeats passed; earlier #383
leave cases also passed. Those passes did not explain the failed observation.

The normal SPU's serve loop in crates/weaver-spu/src/main.rs returns from
serve_decode to the residency loop. The shared residency_composition_child
fixture instead returned from its process on decode EOF in its default full
mode. Files::program selects full for the plan/flag cases. Rollback closes
decode before staging Release, so a child scheduled between those actions
could exit before receiving Release.

A temporary 100 ms delay at that exact boundary in
context_expired_residency.rs reproduced the Planner's complete error string
in the actual worker_plan_flags_turn_boundary_leave process case. The same
instrumented process case passed with the corrected fixture. The delay was
removed and its original file restored byte for byte after both runs.
This reproduces an ordering, not a production deadline failure.

## Correction and consumer census

The generic fixture loop records decode EOF once and continues to service its
original residency socket until Release or lifecycle EOF, under its existing
ten-second watchdog. This replaces the mode list that previously granted
only selected cases the correct lifetime. It does not reread the closed decode
socket. Release still checks decoder closure and returns the exact request's
exchange with its existing terminal outcome.

The forked child and two ordinary socket tests call the same private peer loop.
The tests use the full mode used by the failing worker, with a notification
after actual decode EOF. The parent sends no Release before that notification.
One test checks the exact Released reply; the other closes lifecycle and
checks that the peer exits. Neither test relies on a scheduling sleep to place
EOF before Release. The notification cannot block a panicking parent.

The early-answer branch now uses the passed lifecycle descriptor instead of
literal fd3, so the extracted test loop can run on an owned socket pair.
The real fork still maps lifecycle to fd3 and decode to fd4. Existing explicit
eof mode, generation/session helpers, premature or malformed reply modes,
silent Release and delayed child-exit instruments retain their own behavior.

Consumers of the generic loop are residency composition and teardown fixtures,
and the worker startup fixture's SPU producer. The producer's full fallback
includes plan/flag scenarios. The existing cleanup-ready and expired modes
already retained residency after decode EOF; the common rule keeps that
behavior. Specialized generation helpers return through their existing paths.

## Verification

The evidence deposit includes the failed baseline, the identical scheduling
instrument against the correction, the temporary compile error while extracting
the helper, the final gates and a compiled negative control. The control exits
after the EOF notification; residency_fixture_decode_eof_keeps_release_channel
fails because the Release channel closes. Source hashes verify restoration.

The restored source passed 70 ordinary affected harness cases, all 24 plan/flag
process cases, and all 207 startup process cases. The process cases used owned
root-delegated scopes and an independently built admin test binary. Cargo ran
unprivileged with the pinned toolchain, --locked, CARGO_INCREMENTAL=0 and private
target /tmp/weaver-state-146-serving-organ-loss-target. Lock, formatting, strict
all-target harness clippy, three DocsMirror cases and whitespace checks passed.
All 814 Rust/configuration hashes match the restored control and final gates.

The first plan-family attempt omitted WEAVER_ROOT_PLAN_ADMIN_TEST_BIN; 23 cases
passed and the admin case stopped at setup. The corrected invocation passed all
24 without a source change. That failed invocation remains in the evidence.

The verified deposit and source archive digest are in the PR body. The
code-review and verification-before-completion
skills were unavailable in this session. These are manual author checks, not a
Planner grade, security disposition or production qualification.

## Remaining work

Y1's reproduced cause is corrected here, subject to review and merge. #369 is
separate: another fixture fork can briefly keep a decode endpoint alive, so a
one-shot receive at Release can see EAGAIN instead of EOF. #379 recorded that
child assertion. This act does not claim to close #369 or the other timing
issues, and it does not repeat the full workspace or full foundation suite.
The pending Diagnostic and plan Spec elections, inherited review carries,
language artifacts, measured qualification and coupled cutover remain open.
