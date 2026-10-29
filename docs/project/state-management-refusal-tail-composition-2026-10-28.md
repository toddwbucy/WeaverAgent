# Capture refusal under the original Leave tail

Date filed: 2026-10-28. Issue #146 section E.

Branch: code/state-refusal-tail-composition, stacked on #374 at b0d7cb6c.
The act joins main's #371 merge 0c80e52f and preserves both parents. It
implements the merged timing rule without selecting the foundation dispatcher.
The foundation and admin amendment retain the higher versions and first filing
dates from #373 while incorporating #371's refusal timing clauses.

Implements: foundation Spec section 5.2 PERSIST-SAVE, PERSIST-BEGIN and
PERSIST-LEAVE-REFUSAL, section 5.4 PERSIST-TIME; admin amendment section 3
ADMIN-SAVE and ADMIN-SAVE-TIME; harness amendment section 3 HARNESS-SAVE-TIME.
Author assessment: conforms for each named section on the exercised expiry,
cleanup and final-disposition paths. This is implementation of merged authority,
not a new clock, wire shape, trace event or failure-policy election.

## Ownership and consumers

After a validated Beginning and before Commit, capture expiry withdraws all
save-success work. Admin retains the original capture and Leave windows, cancels
unconfirmed publication, carries its sole Aborted where permitted, and awaits
cleanup and the final lifecycle response. Only the earlier original outer and
shared-tail deadlines bound that refusal-only continuation. An absent graceful
outer still has the validated finite shared tail. No deadline is renewed.

| Consumer | Result |
| --- | --- |
| CaptureWindows | Separate refusal-only continuation from successful CaptureComplete. Validate the exact capture identity and monotonic clock; retain outer and shared-tail bounds at equality. |
| Capture::advance | Enter refusal-only processing on pre-Commit capture expiry or a local preparation refusal. A timely cleanup acknowledgement may be accepted after capture expiry. |
| Capture receive and publication checkpoints | Retain the whole decoded packet and original wire even when the post-decode clock check fails. Validate identity before deciding the outcome; a consumed Commit cannot disappear into a timeout. |
| Capture send | Retain whether the whole packet actually left the socket before a post-send expiry. That fact determines which image or finished result can cross cancellation. |
| Capture refusal reducer | Permit one phase-matching crossed Image or ImageFinished, never another success leg. Reject duplicate, foreign or Commit crossings. Require CaptureRefused before the final unsaved Left or matching graceful refusal. |
| Publication | A failed prepared publication with no observed Commit can be cancelled after cleanup. Retain an observed Commit even when its checkpoint fails; cancellation cannot remove that guard. |
| Existing checked transport and JoinLeave | Keep their checked receive interface and original timing behavior. No legacy lifecycle-channel policy is changed. |
| Worker/member composition | Keep existing first-refusal attribution, actual member cleanup, original tail, forced unsaved conclusion and graceful S9/retry behavior. Runtime harness code is unchanged in this slice. |

Before Beginning, on-demand saves, and post-Commit expiry retain their existing
rules. Refusal-only processing never grants late ImageChecked, Prepared, Commit,
publication, CaptureComplete or a save report. Expiry of the remaining Leave
deadline still fails the command. No cleanup is inferred from Aborted alone.

## Instruments

Seven actor tests use actual Unix packets and publication files with controlled
capture-clock observations. They cover both Leave modes at Image, Hashing,
Finished, Preparing and Commit waits; delayed cleanup after a real missing-file
preparation refusal; one crossed image/finished and its duplicate; wrong capture,
early Left, Commit and CaptureComplete; a Commit decoded across expiry; a cleanup
acknowledgement decoded across expiry; and unchanged demand/post-Commit failure.
The separate window matrix pins exact capture/tail/outer equality, absent outer,
wrong identity, clock reversal, duplicate acknowledgement and forbidden success.
Transport tests retain original Commit bytes and physical send evidence at the
post-operation deadline check. Publication tests distinguish failed preparation
from a failed checkpoint after an observed Commit.

Four joint cases run the actual admin actor, foundation worker and normal SQLite
member for forced/graceful Leave at snapshot/finished expiry. The fixture holds
the member result until the actual Abort, then releases that result before the
member's Aborted. The recorded observation order and request identity must match.
Forced Leave removes containment and closes its marker without adding an index
entry. Graceful Leave stays Open at S9 and succeeds on its actual later retry.
No model or classifier artifact is qualified; those responses remain doubles.

Either peer can observe the shared capture deadline first. The capped member
wait can also retain its timeout first. The process tests admit only those
specific expiry causes and require the actual CaptureRefused missed leg to agree
with the first recorded cause. They do not elect a cause by scheduler order or
replace it when admin's Aborted crosses. The parent examines the completed trace;
the admin child at S9 can inspect only the evidence it already owns.

## Validation and limits

The restored source passes 582 admin package cases with one test thread: 550
main-binary tests, three relay unit tests and 29 integration tests; 71 remain
ignored. The ordinary harness invocation passes 2,174 cases: 2,093 library,
seven worker, 43 integration and 31 doctests, excluding nested child summaries.
Its 837 ignored cases and the explicitly filtered known flake #331 are disclosed.
All 115 selected process cases pass, including the four new expiry cases and
15 joint admin/worker/SQLite refusal cases. These scopes overlap and are not added.

Strict all-target clippy for both changed crates, fmt, lock and diff checks pass.
The deployment/document suite passes all 209 cases. All 765 Rust/Cargo/toolchain
hashes match across the final gates. Cargo used the pinned toolchain, --locked,
a private target and serial invocations with incremental compilation disabled.
The parallel admin invocation is a failed gate; the complete serial pass does
not establish that the parallel ETXTBSY fixture race is resolved.

Evidence is under /tmp/state-refusal-tail-composition-20261028 and is deposited
under the operator-approved state-management-146-olympus-2026-10-22 root after
publication. It includes the exact source archive, source hashes, gate and
control logs, and copied joint-fixture evidence. Trace monotonic timestamps are
relative to their run; no direct comparison with absolute raw capture deadlines
is claimed. The controlled clock matrix and real protocol outcomes are the
boundary evidence.

Nine independently compiled protective controls are caught: remove the expiry
transition; keep capture limiting refusal; drop the shared tail; drop the outer
deadline; permit late success; discard a Commit consumed across expiry; cancel
after observed Commit; forget physical send; and admit a duplicate crossing.
Every changed source was restored before final gates.

Initial failures remain in the evidence: sandbox socket denial; the actor matrix
exposing failed Prepared/Confirming cancellation; a test delimiter typo; a
zero-match actor filter (rerun with all seven actual cases); scheduler-dependent
refusal-code expectations; and an admin fixture reading the worker's trace before
it was written. The parallel full admin gate also hit shell-flood ETXTBSY before
that helper could execute. These are not protective controls or successful gates.

The self-review traced all callers of the changed transport, window and
publication interfaces against the merged Spec. Verification-before-completion
was applied from its installed skill. The requested code-review skill invocation
is unavailable; manual self-review is reported as such. Planner/security review,
production qualification, activation and operator merge remain separate duties.

#374 has a zero-fix regrade with C1-C6 carried. #373's B1-B10 remain carried,
including standing joined refusal, the defensive FailedRun policy and legacy
channel wording. #369, remaining #359 fixture work and #372 T8 also remain.
No broad #146 checkbox closes. The existing serving dispatcher remains selected.

## After-action review

The plan was to implement #371 through the real three-party seam and pin the
deadline boundaries. The actor matrix found that a failed Prepared publication
lost its pre-Commit distinction. A retained Commit observation now supplies that
distinction, and the control proves a consumed Commit cannot authorize cleanup.
The real seam also exposed invalid fixture assumptions about first-cause ordering
and trace-file availability. The retained test evidence records both corrections.

Carry forward: assert the exact protocol identity and first retained cause after
the owning process has published its evidence; require an exact executed test
count before accepting a filter; preserve parallel fixture failures separately
from a serial gate. These rules guide the next composition slice and do not alter
production policy.
