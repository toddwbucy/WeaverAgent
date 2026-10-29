# Review fixture scheduling

Date filed: 2026-10-27. Author record for issues #146 and #359.
Branch: code/state-review-race-fixtures.
Base: code/state-emission-refusal-budget at 9d6b72d7 (PR #364).

This act isolates descriptor lifetimes and synchronizes fixture observations.
It answers the admin lock-probe family in #359, #360 Planner T1, the placement
Decision test reported in #332, and the gate socket collision reported in #364.
All behavior changes are in tests. Foundation activation and qualification
remain outstanding under #146.

## Authority and scope

Implements: weaver-admin Spec section 3, run-lock ownership and release,
Spec 02 DECISION-BARRIER, harness amendment HARNESS-CLOCK and section 4 halt
and lifecycle. Author assessment: conforms for the tested lock, interrupt and
original-deadline properties. No public behavior, Spec, limit or wire changes.

The admin reproduction observes an actual CLOEXEC run-lock descriptor in an
unrelated fork's descriptor table. A pipe holds that child before exec. Closing
the parent's copy leaves the OFD lock held and prevents socket EOF. Exec closes
the inherited descriptors. The executed child acknowledges from user code, then
stays alive on a second pipe. At that acknowledgment the lock must be free and
the socket must report EOF. A second deliberately retained lock still reads held.
This reproduces the proposed kernel mechanism with a controlled schedule. It does
not identify the child from the Planner's earlier failed run.

Twenty-one ordinary admin cases now re-exec their exact test before opening
their locks or sockets. Each caller supplies its module and function name, checked
as a function by the compiler. The helper propagates failure and requires one
executed case. An unnamed-thread regression exercises the same explicit selector.
Its child has no other parallel fixture that can inherit those descriptors.
The immediate release, reacquisition, EOF, refusal and publication assertions
remain. Other tests may still run in parallel. Existing ignored privileged
fixtures already require isolated processes. This act does not widen their claim.

The first broad run exposed the same inheritance surface in a publication
peer-loss fixture: closing its socket answered Pending instead of Stopped.
That case now uses the same process isolation. The controlled reproduction
covers both lock and socket lifetime.

The halt fixture checks actual interrupt submission before waiting for the fake
gate's atomically published evidence. The evidence must name the original
execution and FlowFailure. The trace-loss-late child deliberately withholds its
marker until the observer waits, pinning the formerly unlucky schedule. This
observation has a separate fixture watchdog. It cannot advance or renew the halt
deadline. All four consumers use interrupt_sent before observing the evidence.
The child also stops waiting if its observer socket closes, including with an
unread packet still queued.

For ordinary placement rollback, the independent member responder sends actual
root Leave when the decoder retains the measurement request. The worker may
already be asleep inside advance(), so the fixture driver no longer has to return
before management can arrive. The decoder still withholds its answer. The
Decision test expires the original numeric bound after the retained handoff and
checks DecisionBound and the original Leave cap. No production interval changes.

Gate startup instruments now allocate distinct scratch directories per drive,
using a process-local counter. Parallel cases cannot bind the same admin socket
or overwrite one another's child launcher. The original scenario semantics stay
in their existing instruments.

## Review dispositions

| Planner row | Disposition |
| --- | --- |
| F1 | Canonicalize the expected run-lock path. The inheritance case passes under a symlinked TMPDIR. |
| F2 | Pass the exact case from every caller. No selector depends on a thread name. The unnamed-thread case fails if that dependency returns. |
| F3 | The deadline and clock-failure halt cases now use the same bounded observation as the two already corrected consumers. |
| F4 | Isolated lock scans require the exact holder set, sorting the two expected PIDs. |
| F5 | Scope independent measurement Leave to serving rollback modes, assert the marker and share one send branch with the held-member case. |
| F6 | Reuse start::tests::scratch for the inheritance reproduction. |
| F7 | Require successful child exit and the one-passed-case text without an anchored summary prefix. |
| F8 | The interrupt observer wait ends on socket hangup or error. A SOCK_SEQPACKET case closes the peer with queued data and requires that outcome. |
| T3 | Issue #369 retains the separate residency-teardown Unconfirmed/Confirmed flake, its measurements and its acceptance work. It is not fixed here. |

## Evidence and limits

Three full parallel admin package runs on the corrected source passed 555 cases
apiece, with 70 declared ignores. Three runs passed all 40 affected worker process
identities. The inheritance reproduction also passed with a symlinked TMPDIR.
Both crates passed all-target clippy with warnings denied. Formatting, lock and
diff checks passed.

The ordinary harness invocation passed 2,164 top-level cases, with 797 declared
ignores and the known #331 pending-trace-author pump flake explicitly filtered.
The #369 teardown case ran and passed in this invocation. That single pass does
not close its intermittent failure report.

Three review controls compiled and failed as intended: remove path
canonicalization under symlinked TMPDIR, restore a thread-derived selector in
an unnamed thread, and ignore observer socket departure. The last control reaches
the ten-second fixture watchdog with unread data still queued. Logs retain every
control and the restored source manifest.

After restoration and rebuilding, all 555 admin cases, all 40 affected worker
cases and the observer-departure regression passed again. Formatting, lock and
diff checks passed. All 800 source and configuration hashes match the broad run.

The first review-gate attempt passed two full admin runs and failed the third in
the new inheritance reproduction. A CLOEXEC pipe's EOF and the new /proc exe
image were visible before another inherited descriptor had finished closing.
The reproduction now requires an acknowledgment from the executed test helper.
Its pre-exec hold still proves inheritance. Its post-exec assertions still
require immediate lock reacquisition and socket EOF. The failed attempt and its
source snapshot are preserved.

At ac95d489, three original compiled controls detected a retained parent lock,
an immediate marker assertion and a withheld independent Leave. That revision
also passed three 553-case admin runs, three runs of all 40 worker and 46 gate
identities, and twenty additional runs of each originally flaky worker case.
Its broad harness run failed the known pump case with EAGAIN, which passed alone.
Those results remain evidence for that source, not additional repeats of this
review correction.

Cargo used the pinned toolchain, --locked, a private target and serial
invocations. The author self-check read every isolation caller and all four halt
marker consumers. The named review skills are unavailable here, so that check is
manual. Planner re-review remains required after this push. Source snapshots,
full logs, failed attempts and protective controls are retained for the operator.

These fixes remove specific sources of flaky failures. The records do not measure
their share of all reruns. Code changes and deliberate perturbations still require
verification. #331 and #369 remain separate reliability limitations. Synthetic
fixtures do not qualify language artifacts, the production dispatcher or cutover.

## Carries

The preceding PRs #363 and #364 received Planner grades of zero fix with
batch work retained. Draft #368 carries #364 Q1 through the repair-refused and
repair-exhausted roads, with its own tests and review carries. #365 holds the
Stop-after-completed-refusal and barrier-membership elections.
#363 Y1/Y7 hold the organ-loss/refusal trace-order question. The capture-refusal,
lifecycle, failed-report and qualification work in #146 remains open.
