# Foundation before-Enter admission composition

Version: 1.0. Date filed: 2026-10-28. Issue #146.

Branch code/state-foundation-admission-composition builds on draft #378 at
714aaa97. The dormant worker entry joins the root-only listener and original
Enter admission to the adopted-run dispatcher. The selected worker continues to
call the existing dev_loop::drive.

Implements: foundation Spec section 1 FOUNDATION-TRANSITION and section 3
FOUNDATION-WALK, harness amendment section 1 HARNESS-OWNERS and section 3
HARNESS-PUMP, HARNESS-CLOCK and HARNESS-ENTER-WINDOW, harness Spec sections 2.2,
2.3, 3 and 6, admin-harness contract sections 2, 3, 4 and 6, types amendment
TYPES-ENTER and TYPES-ENTER-WINDOW.

## Admission and ownership

The listener authenticates root before consuming a packet. One receive owner
acquires the packet and ancillary handles, samples the receipt clock before
decoding, and distinguishes canonical Enter from ordinary lifecycle requests.
It accepts no legacy Serving Enter fallback. The current connection remains
serial, with later dials in the listener backlog. After a complete answer, the
same authenticated peer returns to the sole receive owner for another directive
or EOF. Dropping it with an unread second packet can reset the socket and lose
the first answer before its caller reads it; the owner and process fixtures
exercise the retained connection.

Observe before Enter answers Unloaded without load facts or trace. Early Leave,
including rollback, Stop and JoinLeave answer OutOfOrder. A caller's departure
does not end the listener. Malformed or truncated envelopes end the service
through failed process exit, retaining custody through that exit.

Future Enter anchors answer the existing Malformed refusal. Expiry closes only
the unadmitted handles and preserves the before-load lifecycle position, without
fabricating a trace event or substitute answer. Typed construction outcomes keep
clock and invariant failures separate from expiry and descriptor refusal. The
valid original EnterWindow limits refusal delivery and admission work.

The public wait uses the retained source's readiness. The merged Spec defines no
general before-Enter silent-peer or answer deadline, so this act introduces none.
It does not borrow the two-second silent-dial rule scoped to Leave.

The constructor reuses the worker's PR_SET_DUMPABLE=0 hygiene before receiving
Enter handles. The first-act fd8/fd9 lifetime custody remains the composition
root's duty. Successful adoption moves the exact run into the existing no-return
dispatcher without a substitute recorder, state stream or interval.

The dormant entry returns a Diagnostic Enter in its original admission owner.
The caller still owes the separate Diagnostic route before production selection.
This return neither adopts a Serving context nor supplies a Diagnostic reply.

The lifecycle contract requires stderr when an answer cannot be delivered. A
retained notice returns to the worker caller before the next connection is
consumed. Its write ignores an I/O error instead of unwinding the listener. This
is synchronous stderr reporting, with no new timeout or nonblocking guarantee.

## Validation scope

The exact Rust/configuration source passes 2,190 ordinary harness cases:
2,108 library, eight worker, 43 integration and 31 doctests. This full package
run uses --test-threads=1 after the parallel fixture failure described below.
It retains 864 ignored cases and explicitly filters known #331. All 796
selected worker-process identities then pass under host-root delegated
containment, with the actual admin binary supplied to the Leave, Observe and
Stop fixtures. These are distinct counts; nested child summaries are not added.

Strict harness all-target clippy, formatting, lock, diff and three DocsMirror
cases pass. The pinned nightly, --locked and CARGO_INCREMENTAL=0 use private
target /tmp/weaver-state-146-serving-organ-loss-target. All 775 Rust/configuration
hashes remain identical through the full gate and restore after the controls.

Eight separately compiled controls are caught: omit the post-construction
window check; omit the post-staging check; lose the future refusal's original
cap; accept legacy Enter as an ordinary directive; drop the answered peer;
omit the idle wait; omit the dumpability hygiene; and report successful exit
for malformed/truncated admission. The restored source is rebuilt and all 19
focused identities pass. The control recorder initially expected the word
assertion and rejected a valid expect failure; its corrected check requires
the specific expected failing test and the matching mutated source hashes.
This instrument correction does not add a ninth control.

The requested code-review and verification-before-completion skills are not
available in this session. Manual source review and the executed author gates
remain subject to Planner verification; no independent grade is claimed here.

Seven whole-process cases start the reexecuted worker before its first Enter.
They exercise ordinary before-load directives followed by two turns and Leave,
future/expired/missing-state Enter followed by valid Serving, malformed and
truncated process failure, and return of the intact Diagnostic owner. The
controller owns the real lifecycle, trace and state peers. The tests check root
acceptance, listener and silent-peer blocking at the actual serving thread,
fd8/fd9 inheritance hygiene, and release of the original run lock at process
exit. Language, model and organ behavior are fixture doubles.

Eleven owner cases and a typed-constructor case cover actual socket pressure,
ordered replies on one peer, clock failure after physical send, expiry during
staging and after construction, exact future-window retention, lost-answer
notice custody, descriptor closure, Diagnostic preservation and rejection of
the legacy Enter shape. Deterministic owner pressure is not described as a
whole-process backpressure result.

The first compile caught a wrapper arm returning a progress value rather than
unit. The initial owner run was sandbox-denied at socket bind; the host run
exposed the answered-peer reset and a malformed legacy test fixture. Those
failures remain in the evidence. The first full gate exposed a stack overflow
from a test-only constructor compatibility wrapper copying the large context
result. The wrapper is removed and its three test callers use the typed
constructor directly, with the original failing test passing on its normal
stack.

A later parallel library run passed 2,107 cases and failed the residency teardown
case tracked by #369. Its child expected decode EOF at Release and instead read
EAGAIN; the parent's release consequently became Unconfirmed. An unchanged
isolated rerun passes. This records a concrete earlier failure in that fixture;
it does not prove why another descriptor remained live. The failed run is
preserved, and serial validation does not close #369.

## Remaining work

Production selection remains held. Root plan-update composition, the remaining
author/consumer map, Diagnostic routing, production grammar and aligned model
artifacts, prompt/template qualification, measured limits, security disposition
and coupled cutover remain open. The merged #377 dictionary and rule walk are
candidate inputs and do not qualify those artifacts.

## After-action record

The planned slice starts before Enter and joins the existing adopted-run driver.
Its constructor needed typed failure provenance because a diagnostic String
cannot grant cleanup or refusal authority. The test plan separates the actual
process entry from owner tests for deterministic send pressure and clock faults.
Keep that separation in later composition work, and report each fixture's
observed boundary rather than extending a narrower result to the entire driver.
