# Diagnostic setup rollback proposal

Date: 2026-10-29. Proposed authority for #146 E, #390, #395 AI1 and the Diagnostic part of #397 AK1.
Base: main f6f707799e2b0349180582032bc909a1e021a383.
Draft proposal: #398. Branch: docs/state-diagnostic-setup-rollback.

## Problem and evidence

Draft #397 fixes replay starting after a completed setup refusal. Setup still
waits synchronously for identity, Admit, Open and elected classifier readiness.
Admin can finish its own Enter wait and send rollback while those waits continue.
The worker cannot hear that directive at those receive sites.

An actual canonical worker measurement at #397 52d189af closes the original
Enter peer while identity is held, opens a new authenticated peer and sends
forced rollback. There is no answer after 500.068332 ms. Releasing identity then
lets the worker fork the SPU; its scripted Admit refusal finally permits rollback
to finish Left/no_state. This measures the dependency on the identity answer,
not exhaustion of the production 150-second Leave or 225-second rollback bounds.
Admit/Open/classifier blocking are source-audit findings, not additional measured
process stalls in this record.

The temporary fixture changes only the test. Its exact patch, command, logs and
restored-source digest are retained in the evidence deposit named by the PR.
The implementation worktree is restored and its unmodified lib binary rebuilt.
No deployed agent or model was used.

## Proposed elections

Implements: harness Spec 6.1.1 DIAGNOSTIC-SETUP-ROLLBACK, admin Spec 3,
admin-harness contract 3, harness-SPU contract 2/3, types Spec 4.2 and Diagnostic
Spec 3.2. These are proposed Spec-gap elections and clarification, not a claim
that the current implementation conforms.

1. A forced rollback accepted before the worker selects Ready withdraws Diagnostic
   setup. The original Enter closes with existing NoResidency, unless an earlier
   outcome was already established. This is a new meaning on that aggregate:
   admission was withdrawn, not a measurement that the device is empty. Left
   remains the cleanup outcome. Ready selected after all confirmations remains
   committed even if its delivery is delayed; a later rollback uses the entered
   rule. Pending setup and refused partial Diagnostic custody report InTransition
   to Observe, not Idle; SavePoint and Stop refuse OutOfOrder. Before-load refusal
   without a retained run keeps its existing Unloaded observation. This addresses
   the Diagnostic part of Planner review 5696280646 AK1; its serving case remains
   separate. Peer loss alone changes no lifecycle outcome.
2. Setup settlement shares the existing worker Leave interval from rollback
   acceptance. The worker drains a sent Admit to its exact terminal before any
   Release. It sends no Open after rollback. A withheld Admit ends through owned
   failure cleanup at the original deadline, without a release confirmation.
   Identity and decode work channels close; all child handles remain owned
   through release/reap. No new bound or wire timing member is proposed.
3. No Diagnostic event precedes ReplayOpened, including setup faults and rollback.
   Lifecycle replies and operator diagnostics carry that pre-replay failure.
   This resolves AI1's pre-bracket question explicitly and does not change the
   vocabulary, an already-open replay or #393's proposed interruption outcome.

The alternative to election 1 is to retire Enter without an answer once rollback
is accepted, requiring an explicit exception to the aggregate contract, or to add
a dedicated setup-cancelled refusal and update both endpoints' vocabularies.
Keeping Idle for refused partial custody would preserve the current answer but
continue to report readiness the failed setup never established. A new public
refused-load state would require paired enum and reader changes. InTransition
uses the existing answer for an unfinished lifecycle. NoResidency preserves the
wire but overloads the code; that tradeoff is the
operator's to elect. The alternative to draining Admit is to end the child at
rollback acceptance. The proposal keeps the existing exchange ordering and gives
normal settlement the remainder of the already-running Leave bound.

The trace alternative is a separate setup-failure event or bracket, with paired
reader changes. The proposed empty pre-replay trace preserves the existing reader
shape, at the cost of lifecycle replies and stderr being the only setup account.
No cross-repository contract or reader payload changes in this proposal.

## Implementation plan and acceptance

Do not deploy an implementation claiming this proposal until it is approved,
Planner-reviewed, merged, implemented and qualified. This PR changes documents
only. The implementing act stacks after the current runtime root and replaces
the synchronous Diagnostic setup with an owner that retains both lifecycle
exchanges, each partial arm and the original rollback deadline. It must keep
ordinary reply backpressure and a silent new peer from starving rollback.

The harness Spec carries the state table and the actual-process acceptance
matrix. The implementing act must cover all four held setup arms, both peer
routes, late terminals, original-peer loss, reply pressure, exact Admit terminal
attribution, cleanup failure and deadline exhaustion. It must distinguish
withdrawn setup from already committed Ready and from a replay already started.
Both Diagnostic entry consumers must use the same owner or have separate process
evidence. No aggregate Ready, replay, new child or new work may follow accepted
pre-Ready rollback; Release cannot overtake Admit and failed cleanup grants no
successful Left. Compiled controls must catch removal of those properties.

This proposal closes neither #146 nor #390. Entered replay interruption still
awaits #393; #120, review carries, production artifacts, qualification and coupled
cutover remain open. The current serving dispatcher remains selected.

## Author check

The author read the merged rollback row, single-exchange rule, Enter aggregate,
SPU Admit/Release ordering and Diagnostic opening rule before drafting. They
require cleanup but leave the interrupted aggregate and pending-Admit sequence
unstated. The proposal names the new choices instead of assigning them silently
to a runtime refusal.

The useful audit rule is to hold the source answer while sending the real
rollback, rather than test only a refusal that has already settled. Keep the
measured wait separate from an unrun production-bound test. The code-review and
verification-before-completion skills are unavailable in this session; this is
a manual author check, not a Planner grade or qualification.
