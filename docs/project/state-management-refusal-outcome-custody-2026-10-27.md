# Admin refusal outcome and publication cleanup

Date filed: 2026-10-27. Author record for issue #146, section E.
Branch: code/state-refusal-outcome-custody.
Base: code/state-resting-failure-composition at 3d5dfe85 (PR #372).

This act answers #370 Planner V2 and V5. After a matching CaptureRefused,
admin classifies a correlated terminal lifecycle refusal or FailedRun as a
worker outcome. It retains the original packet instead of reporting a protocol
failure. Failed local publication cleanup preserves its original preparation
failure and checks the current guard before attempting another Broken write.

## Authority and consumer review

Implements: state-management foundation Spec section 5.2,
PERSIST-LEAVE-REFUSAL and PERSIST-PUBLICATION-GUARD; admin amendment section 3,
ADMIN-SAVE. Author assessment: conforms for these refusal and custody paths.
No wire, timing or force policy changes. #371's deadline proposal is not
implemented and still requires the operator's election, review and merge.

| Consumer or failure path | Disposition |
| --- | --- |
| AwaitRefusedLeave | Correlation still precedes interpretation. A terminal Refusal retains WorkerOutcome; the graceful SavePointNotTaken still requires its exact missed leg and graceful mode. Forced Left still requires no save report and no_state false. |
| AwaitCaptureRefused, ordinary receive and publication checkpoint | Existing negative-outcome classification remains. These paths already distinguish a typed refusal from malformed traffic. Their Aborted handling does not grant a successful Left. |
| Standing-refusal and initial-escalation checks | Unchanged. A generic terminal refusal after capture grants neither a standing-run claim nor initial Leave escalation. |
| Public refusal projection | Lifecycle codes retain their typed projection. Worker FailedRun is retained evidence, not authority for root-owned paths or published identity; before Commit its existing public projection remains Unanswered. |
| Publication fail, shared by seed, demand and Leave | If the judged guard already has the exact desired Broken bytes, sync its directory without rewriting it against stale Pending bytes. Other guards retain their existing replacement and mismatch checks. |
| Cancel-unconfirmed cleanup after an earlier failure | Preserve the original detail before the separate cleanup detail. Re-probe actual remaining artifacts. If guard unlink succeeded before a sync failure, restore the blocking guard through the existing missing-guard path. |
| Completed publication | The existing early return preserves the completed save; this change grants no new publication or retry. |

The new runtime surface is classification of a correlated terminal refusal after
the cleanup acknowledgement, plus an existing-Broken branch in the shared failure
owner. Both consumers and the shared publisher failure paths are listed above.

## Reproduction and verification

The actor regression uses the actual admin channel and publisher with a scripted
worker peer. It checks forced and graceful Leave against a lifecycle refusal,
FailedRun, and foreign run, capture and exchange identities. Valid refusals remain
worker outcomes; foreign identities remain protocol errors. No new frame, index
entry or successful save is permitted. Existing actor cases retain rejection of
wrong missed legs, forced SavePointNotTaken, duplicate acknowledgements, restarted
publication, save reports and invalid final shapes.

The cleanup regression first fails the real post-guard preparation checkpoint.
It then fails cancellation through foreign part material, before guard unlink,
or after unlink before directory sync. It preserves the first preparation cause,
foreign bytes and exact Broken guard, verifies every reported artifact exists,
and excludes removed parts from the retained evidence. No new index entry appears.

Both regressions failed on the original runtime. The first cleanup fixture used
the wrong phase and stopped at admission; that failed attempt is preserved. The
corrected fixture proves Preparing before exercising the reported defect.
Compiled controls removing each correction also fail their regression. The final
guard control removes the existing-Broken branch including its directory sync;
all sources are restored byte for byte before the publication-family recheck.

The final admin package gate passes 532 main-binary cases, three relay unit cases,
six invocation cases, three manifest cases and twenty relay integration cases:
564 total, with 71 inherited ignores. Nested child summaries are not counted twice.
All-target admin clippy with warnings denied, formatting, lock and diff checks pass.
The eleven actual admin/worker/SQLite capture-refusal cases also pass in a delegated
root test scope using the rebuilt admin binary. Those process cases are author
evidence and do not expand the actor regression into production qualification.

Cargo uses the pinned toolchain, --locked and a private target. The 762 Rust,
Cargo and toolchain source hashes are identical before and after the final gates.
The unavailable code-review and verification-before-completion skills were not
invoked; manual source, caller and Spec review was performed.

## Remaining work

#370 V1 awaits #371; T2/T5 and V3/V4/V6-V8 remain carried. #372 received a
zero-fix Planner grade with batch findings, including W1's serving-caller
held-reply hazard and W2's send-clock witness consistency. Those are separate
from the admin corrections here. #331, #359's remaining fixture work, #369 and
the broader #146 composition, qualification and cutover obligations remain open.
Foundation remains unselected. Only the operator merges and deploys.
