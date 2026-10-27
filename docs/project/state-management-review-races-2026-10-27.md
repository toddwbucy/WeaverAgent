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

Implements: weaver-admin Spec section 3, run-lock ownership and release;
Spec 02 DECISION-BARRIER; harness amendment HARNESS-CLOCK and section 4 halt
and lifecycle. Author assessment: conforms for the tested lock, interrupt and
original-deadline properties. No public behavior, Spec, limit or wire changes.

The admin reproduction observes an actual CLOEXEC run-lock descriptor in an
unrelated fork's descriptor table. A pipe holds that child before exec. Closing
the parent's copy leaves the OFD lock held and prevents socket EOF. Exec closes
the inherited descriptors: the lock becomes free and the socket reports EOF,
while the executed child is still alive. A second deliberately retained lock
still reads held. This reproduces the proposed kernel mechanism with a controlled
schedule; it does not identify the child from the Planner's earlier failed run.

Twenty-one ordinary admin cases now re-exec their exact test before opening
their locks or sockets. The helper propagates failure and requires one executed
case. Its child has no other parallel fixture that can inherit those descriptors.
The immediate release, reacquisition, EOF, refusal and publication assertions
remain. Other tests may still run in parallel. Existing ignored privileged
fixtures already require isolated processes; this act does not widen their claim.

The first broad run exposed the same inheritance surface in a publication
peer-loss fixture: closing its socket answered Pending instead of Stopped.
That case now uses the same process isolation. The controlled reproduction
covers both lock and socket lifetime.

The halt fixture checks actual interrupt submission before waiting for the fake
gate's atomically published evidence. The evidence must name the original
execution and FlowFailure. The trace-loss-late child deliberately withholds its
marker until the observer waits, pinning the formerly unlucky schedule. This
observation uses a separate watchdog after the original halt deadline has been
checked; it cannot advance or renew that deadline.

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

## Evidence and limits

Three full parallel admin package runs passed 553 cases each, with 70 declared
ignores. Three runs each passed all 40 selected worker and 46 gate identities.
Both formerly flaky worker cases also passed twenty additional runs.

The broad harness library run passed 2,084 cases, with 797 declared ignores,
and failed the previously known pending-trace-author pump test with EAGAIN.
That case passed alone. This is not a claim of a clean broad harness invocation.

Three independent compiled controls failed: retaining the parent lock defeated
the release assertion; restoring the immediate marker check failed against the
deliberately delayed child; withholding the independent Leave caused the original
five-second Decision to expire before handoff. The last failure uses a custom
assertion message; the first log classifier expected the word "assertion" and
was corrected by checking the exact DecisionBound failure. Source restoration
was verified before the final build.

Both changed crates passed all-target clippy at -D warnings. Formatting, lock
and diff checks passed. Cargo used the pinned toolchain, --locked, a private
target and serial invocations. After all controls, the rebuilt source passed the
full admin package again (553 cases), all 86 affected worker/gate identities,
admin clippy, formatting, lock and diff checks. The final source manifest matches
the restored tested files.
Source snapshots, full logs, failed attempts and protective-control evidence are
retained in the operator's approved evidence deposit.

This is author evidence. The named review skills are unavailable in this session;
the self-check is manual. Planner review remains required. Synthetic fixtures do
not qualify the language artifacts, production dispatcher or coupled cutover.

These fixes remove specific sources of flaky failures. The review records do
not measure their share of all reruns, and deliberate perturbations and code
changes still require verification. A previously known pump test remains a
separate reliability limitation; the broad-run failure is retained in the logs.

## Carries

The preceding PRs #363 and #364 received Planner grades of zero fix with
batch work retained. #364 Q1 still owes one budget through the repair-refused
and repair-exhausted roads. #365 holds the Stop-after-completed-refusal election.
#363 Y1/Y7 hold the organ-loss/refusal trace-order question. The capture-refusal,
lifecycle, failed-report and qualification work in #146 remains open.
