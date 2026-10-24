# Residency through boot and gate composition

Date: 2026-10-23. Branch: code/state-resident-boot-composition.
Parent: #297, 27368a748878d4cb83d9aac8d3a9aecf4d772574.
Advances Issue #146 startup composition; no full checklist item closes.

## Connected ownership

The actual admitted startup now has a consuming boot handoff. Its evidence carries
its private context identity and original Enter bound. Admission checks the matching
context/resource bag, original ports, actual Admitted reply, whole Admit send and
its timestamp, exchange and unchanged instruction. A rejected join returns every
input without changing the context. The existing primitive boot constructor remains
available; its output does not acquire residency evidence retroactively.

The joined boot borrows the actual residency endpoint and both SPU/classifier child
custodians beside the decode borrow. It retains the original Admit request and send
witness, Admitted response, initial Grants and startup evidence. The children retain
their existing stderr owners. Neither a copied PID nor a scalar success flag is used
for this transfer. The resource bag cannot be borrowed again while boot owns these
borrows. Completed and stopped boot both retain the bundle, as does the subsequent
gate startup. Generic completed-boot extraction now returns that bundle explicitly.

Boot and gate use this retained residency endpoint in their common waits. An
externally supplied replacement port fails before progress. Boot also compares the
live Enter bound with the one retained from actual admission. Temporary field splits
needed to borrow the pump restore the entire bundle before returning any result.
The gate still receives the actual completed boot, and its final arm keeps all of
these prerequisites and children together.

This is not original admin Enter adoption, aggregate Ready delivery, or worker-loop
activation. A gate-arm confirmation remains only that arm. The original Enter peer
and descriptor adoption, paired TYPES-ENTER carrier, accepted lifecycle reduction,
refusal recording/cleanup and final resource-owning root remain to connect. The
current lifecycle carrier still lacks the replacement Serving fields specified by
TYPES-ENTER; supplying those facts separately is not a production substitute for
the paired admin/harness cutover. Existing serving stays active.

## Authority and verification

Implements: weaver-harness Spec section 6 fan-out/resource custody; HARNESS-BOOT and
HARNESS-PLAN prerequisites; HARNESS-PUMP and HARNESS-CLOCK; harness-gate contract
sections 1-3. Author verdict: conforms for this dormant joined ownership/wait boundary,
not complete lifecycle or production qualification. No Spec election, dependency,
wire field, trace kind or duration is added.

All 24 delegated startup-through-gate scenarios pass, including nine new cases:
foreign context, changed Admit instruction, foreign admission bound, substituted
boot/gate residency ports, changed bound during boot, stopped-boot custody, and actual
SPU termination during boot and gate startup. The latter two require the Residency
transport failure, so accidentally omitting that source cannot pass by observing
only the decoder's EOF. Real child processes and delegated fixture cgroups are used;
the SPU, classifier and gate remain protocol doubles, not qualified model artifacts.

The final full harness suite passes 1,921 tests with 42 ignored: 18 inherited and all
24 delegated instruments, separately executed. The trace suite passes 160. Strict
all-target harness Clippy, formatting, lock/diff and DocsMirror checks pass. The final
full harness run follows all test additions and mutation/compile-pin restoration.
The trace suite preceded the final two test-only EOF cases. Private target:
/tmp/weaver-146-boot-composition-target. Nested child summaries are excluded.

Eleven compiled protective mutations fail their named tests; all eleven restored
cases pass. E0499 rejects reborrowing the resource bag while boot retains it, and
E0507 rejects consuming shared admitted startup. All 24 restored delegated cases pass,
and all 248 harness Rust hashes match after restoration. These are direct author
checks, not independent Planner/security grades. The prescribed review/verification
skills are unavailable; the direct sequence review is separately deposited.

## Remaining work and after-action

Original Enter/aggregate Ready and worker/dev_loop mounting remain the composition
priority, together with the specified paired lifecycle carrier. Healthy rollback,
other startup refusals, stopped gate and terminal lifecycle, LastWord, production
artifacts/prompt/limits, external qualification and coupled cutover remain open.
Only the operator merges/deploys and controls CodeRabbit.

The initial plan to send aggregate Ready exposed that the circuit discarded actual
residency evidence before boot. This act closes that ownership gap and keeps the
wire/descriptor gap visible. The initial compile found one borrow-order conflict;
its context identity is now captured before moving the mutable resource borrow.
A private-interface lint was corrected by narrowing the unused extraction method.
Sustain actual retained producers and live-source failure tests when mounting the
root; do not reconstruct aggregate readiness from summary flags or unrelated peers.
