# Admin refusal outcome and publication cleanup

Date filed: 2026-10-27. Author record for issue #146, section E.
Branch: code/state-refusal-outcome-custody.
Base: code/state-resting-failure-composition at 3d5dfe85 (PR #372).

This act answers #370 Planner V2 and V5 and #373 Planner X1-X4, X7 and X8.
After a matching CaptureRefused, admin accepts only the terminal refusal codes
the unwind can send, retaining the original packet. Direct retry codes and
worker claims about admin's publication are protocol failures. Failed local
publication cleanup preserves its original preparation failure and checks the
current guard before attempting another Broken write.

## Authority and consumer review

Implements: state-management foundation Spec section 5.2,
PERSIST-LEAVE-REFUSAL and PERSIST-PUBLICATION-GUARD; admin amendment section 3,
ADMIN-SAVE; types amendment section 5 TYPES-PERSIST. The initial broad refusal match
drifted from ADMIN-SAVE. The corrected match and paired Spec enumeration travel
together: Malformed, DescriptorsUnusable, OrganRefused with organ Gate, and
FailedRun naming the original run and known capture ID. The foundation table
and types amendment carry that same rule. Author assessment: conforms to the
corrected sections for these refusal and custody paths. The normal forced Left
and graceful SavePointNotTaken continuations retain their existing conditions.
No wire, timing or force policy changes. #371's deadline proposal is not
implemented and still requires the operator's election, review and merge.

| Consumer or failure path | Disposition |
| --- | --- |
| AwaitRefusedLeave | Correlation precedes the explicit terminal-code match. Other refusal codes are Protocol. Graceful SavePointNotTaken requires its exact missed leg and graceful mode. Forced Left requires no save report and no_state false. |
| AwaitCaptureRefused, ordinary receive and publication checkpoint | Once Beginning supplies a capture ID, FailedRun requires that exact ID, not None, as well as the run. Both ordinary receive and local-work checkpoint use the same predicate. Other negative-outcome classification is unchanged. |
| Standing-refusal and initial-escalation checks | Unchanged. A generic terminal refusal after capture grants neither a standing-run claim nor initial Leave escalation. |
| Public refusal projection | Only admitted post-acknowledgement lifecycle codes retain their typed projection. A nested gate refusal stays attributed to the gate. Direct StatePersistenceInvalid and standing retry codes are rejected before this projection. Worker FailedRun is evidence, not root-owned publication authority; its pre-Commit public projection remains Unanswered. |
| Publication fail, shared by seed, demand and Leave | If the judged guard already has the exact desired Broken bytes, sync its directory without rewriting it against stale Pending bytes. Other guards retain their existing replacement and mismatch checks. |
| Cancel-unconfirmed cleanup after an earlier failure | Preserve the original detail before the separate cleanup detail. Re-probe actual remaining artifacts. If guard unlink succeeded before a sync failure, restore the blocking guard through the existing missing-guard path. |
| Completed publication | The existing early return preserves the completed save; this change grants no new publication or retry. |

The gate Lower owner wraps an actual gate refusal in OrganRefused with organ Gate;
an invalid Lower reply becomes Malformed. The entered rollback terminal retains
that refusal instead of Left. The existing failed-unwind rules also name
DescriptorsUnusable. This is the producer census behind the explicit match.

The shared publication owner keeps the existing-Broken branch (X6, graded sound).
Composing an error solely from the previous failure would miss the separate case
where guard unlink succeeds and directory sync fails: the missing guard must be
restored. Existing tests cover both paths; no second failure owner is introduced.

## Reproduction and verification

The actor regression uses the actual admin channel and publisher with a scripted
worker peer. It checks forced and graceful Leave against each admitted terminal
shape and foreign run, capture and exchange identities. The foreign identities
pin correlation, not the terminal match. Separate correlated cases reject eight
disallowed lifecycle codes, including retry codes, direct publication claims and
OrganRefused from SPU. Missing FailedRun capture IDs are rejected during Image,
Hashing, Commit and AwaitRefusedLeave; Hashing exercises the local-work checkpoint.
No new frame, index entry or successful save is permitted. Existing cases retain rejection of
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

The corrected actor family passes eighteen cases. Both added review regressions
compiled and failed against the reviewed runtime. An initial test-only missing
import did not compile and is retained separately from those red results. Three
compiled controls restore the broad match or remove the exact-ID check at either
consumer; all fail the intended assertions, with source restored afterward.

The final admin package gate passes 534 main-binary cases, three relay unit cases,
six invocation cases, three manifest cases and twenty relay integration cases:
566 total, with 71 inherited ignores. Nested child summaries are not counted twice.
All-target admin clippy with warnings denied, formatting, lock and diff checks pass.
The deployment/document gate passes 209 tests. All eleven actual
admin/worker/SQLite capture-refusal cases pass again with the corrected admin
and unchanged harness in this checkout's private target. The first recheck
stopped during setup because that fresh target lacked the normal SQLite member;
building it allowed the same eleven cases to run. Both logs are retained.
Those process cases are author evidence, not production qualification.

Cargo uses the pinned toolchain, --locked and a private target. The 762 Rust,
Cargo and toolchain source hashes are identical before and after the final gates.
The unavailable code-review and verification-before-completion skills were not
invoked; manual source, caller and Spec review was performed.

The initial V2 fix generalized past the producer's actual codes. The review
correction follows the producer through its public consumer and adds correlated
negative cases, rather than treating foreign-identity tests as a match-arm pin.
That producer/consumer census is retained above for the next act.

## Remaining work

#370 V1 awaits #371; T2/T5 and V3/V4/V6-V8 remain carried. #372 received a
zero-fix Planner grade with batch findings, including W1's serving-caller
held-reply hazard and W2's send-clock witness consistency. Those are separate
from the admin corrections here. #331, #359's remaining fixture work, #369 and
the broader #146 composition, qualification and cutover obligations remain open.
Foundation remains unselected. Only the operator merges and deploys.
