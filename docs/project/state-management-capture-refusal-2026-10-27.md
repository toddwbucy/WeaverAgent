# Capture refusal and the remaining Leave

Date filed: 2026-10-27. Author record for issue #146.
Branch: code/state-capture-refusal-composition.
Base: code/state-repair-refusal-budget at 2654ddda (PR #368).

A refused pre-Commit capture now completes its cleanup and trace accounting,
acknowledges that refusal on the original Leave exchange, and continues to the
accepted lifecycle outcome. Forced Leave releases and unloads without a save
report. A Leave that remains graceful keeps the entered run at S9 and accepts
its next root operation. This implements the authority merged in #348 and #349.

## Authority and implementation

Implements: state-management foundation Spec section 5.2
PERSIST-LEAVE-REFUSAL; harness amendment sections 3, 4 and 6
HARNESS-SAVE-TIME, capture cleanup, HARNESS-HALT and HARNESS-SAVE; admin
amendment section 3 ADMIN-SAVE and ADMIN-SAVE-TIME; types amendment section 5
TYPES-PERSIST; harness Spec section 6 and admin Spec section 3.0 S9.

| Section | Author assessment and behavior |
| --- | --- |
| PERSIST-LEAVE-REFUSAL | Conforms for the implemented capture and Leave owners. The first refusal keeps its missed leg and attribution. Cleanup and admitted authoring settle before refused is Recorded and the acknowledgment can leave. |
| HARNESS-SAVE-TIME and cleanup | Conforms in the tested phases. Cancellation before snapshot settles owed work without making a member holding. Pending snapshot/finished exchanges use their original Abort; a completed timely finished exchange uses a fresh ReleaseCapture. A queued confirmation settles without Commit after cancellation. |
| HARNESS-HALT | Conforms in the tested cleanup failures. Invalid cleanup latches the existing fault and preserves its capture account. No refusal acknowledgment, save report or successful Leave follows. |
| ADMIN-SAVE and ADMIN-SAVE-TIME | Conforms in the actor and invocation fixtures. A timely acknowledgment ends only the capture wait. Admin cancels its own unconfirmed publication and awaits the final Leave under the original remaining bounds. |
| TYPES-PERSIST | Conforms. SavePointCaptureRefused is a harness Continue carrying the original capture ID and Answer, Finished or Published. CaptureNotReady and wrong-direction/position frames refuse. |
| S9 | Conforms for the exercised management operations. Observe answers Idle, Stop answers AtRest, demand save retains its own window, and a later Leave starts at capture with its own original tail. A refused graceful orphan keeps the run standing. |

The capture owner retains one crossed admin Aborted without replacing an earlier
member refusal, restarting cleanup or acknowledging twice. A duplicate or foreign
continuation fails. Every lifecycle carry retains its exact envelope and send
witness through pressure. Loss of admin retains undelivered evidence; no response
is invented. A whole send whose clock observation fails remains a trace failure.

Admin's preparation error sends one Aborted and waits for cleanup acknowledgment.
A crossed Image or ImageFinished is validated and discarded. A crossed Commit
cannot become a healthy refusal. Cleanup after the acknowledgment removes only
the publisher's own unlisted part and guarded, uncommitted entry. A changed guard
or foreign part directory refuses cleanup. Retaining the acknowledgment before
that fallible cleanup prevents a second cancellation exchange.

The terminal owner distinguishes a serving Leave from possession of a save
report. A forced refused capture still owes grants, release, Unload, trace drain,
reap and normal admin conclusion. Its Left has forced true, no_state false and
no report. Joined force keeps the original Leave cause and joining force cause.

S9 keeps the loaded facts, lowered gate, context, member, SPU observations and
completed refusal evidence. It retires the ended Leave wait. A new command is
received on the actual root socket; it does not reuse the old Leave allowance
or repeat quiesce, wind-down or Lower. SPU loss is recorded while the run remains
at rest; a later Leave uses the existing NoResidency disposition. Failed-run
cleanup at S9 uses its halt allowance rather than the ended Leave's tail.

## Evidence and scope

| Instrument | What it exercises |
| --- | --- |
| Capture parent, 25 cases | Real recorder, state stream and lifecycle packets with scripted typed member results. Includes explicit image/finish refusal, all pre-Commit cancellation phases, pending confirmation under recorder pressure, original Abort identity, fresh release, late result observations, exact attribution, duplicate cancellation, acknowledgment pressure and the original Leave tail. |
| Admin capture actor | Timely acknowledgment at receive and hashing checkpoints; local preparation cancellation; crossed packets; expired capture; duplicate acknowledgment; forbidden publication restart, report and CaptureComplete; failure during owned-artifact cleanup; demand/Leave distinction. |
| Actual admin, worker and SQLite member, 11 cases | Direct forced refusal, still-graceful refusal and retry, joined force, real image-name collision, real admin image-preparation failure, delayed actual snapshot and finished replies, and deliberately corrupted cleanup replies. Successful retries reload the actual saved image. |
| S9 worker sockets | Observe, Stop, demand save, fresh graceful and forced Leave, actual SPU exit, and a graceful admin orphan. The member, recorder, lifecycle sockets and process owners remain live. |
| Surrounding lifecycle regressions | 96 selected worker cases and 21 admin invocation cases passed before the six additional joint cases. The later 11-case joint run includes those six and repeats five earlier cases. These counts overlap; they are not 128 distinct cases. |

The joint timeout cases retain the actual late result and cleanup observation
under one request identity, before the refused trace record. Their original
capture windows still permit timely refusal acknowledgment. The actor fixture
separately queues acknowledgment and Left after capture expiry and requires
Unanswered. Existing root invocation timeout fixtures preserve the Open marker.
Neither scope alone claims a deployed three-party capture-expiry qualification.

The failed-cleanup joint cases retain a protocol_failed/state_reply fault and
component-failure Unload. Admin receives no terminal wire answer and therefore
reports Unanswered, as ADMIN-SAVE requires, while preserving Open. The test
initially expected FailedRun from worker failure alone; the received bytes and
merged rule refuted that expectation. The failed run and its source are retained.

Four compiled protective controls failed as intended:

- Bypass member cleanup before refusal acknowledgment.
- Renew the original Leave tail during cancellation.
- Treat CaptureRefused as the final admin outcome.
- Accept a save report after capture refusal.

Each mutated production file was restored byte for byte. Restored capture-parent
and admin actor cases passed. Later edits expand tests and correct a comment;
runtime code exercised by those controls is unchanged. The final package gates
passed 247 types cases with config, 562 admin cases and 2,170 harness cases.
Admin declared 71 ignores; harness declared 824 and explicitly filtered one case.
All three changed crates passed all-target clippy with warnings denied; format,
lock and diff checks passed. The final comment correction also received an admin
actor and lint recheck. Counts exclude re-executed child summaries and count each
Cargo target's final libtest summary once.

The ordinary harness gate explicitly filters the recorded #331 pending-author
flake. #369 remains a separate intermittent residency-teardown report. A passing
invocation does not close either issue. All Cargo work uses the pinned toolchain,
a private target and --locked; Cargo and Git mutations are serial. Privileged
process fixtures run in private root-owned delegated scopes.

## Self-check and remaining work

The manual author check follows the new wire variant through codec, admin
checkpoints, worker acknowledgment, final Leave and joins. It follows each
cancellation phase through its member exchange and recorder owner, and S9
through new admission, SPU observation and failed cleanup. The named review
skills are unavailable in this session. Planner review remains required.

Foundation remains unselected. These fixtures use synthetic model/classifier
artifacts and do not qualify language inputs, prompts, limits, deployment or
coupled cutover. The full phase matrix is split between parent protocol fixtures
and process fixtures; it has not been qualified as a deployed agent. The broader
Issue #146 composition and qualification checklist remains open. Parent review
carries on #363, #367 and #368 and the elections in #365 remain separate work.
Only the operator undrafts, merges or deploys. No CodeRabbit review was requested.

The next S9 composition cases must cover silent or malformed root dials, an
expired S9 demand save, and state/trace failures while resting. The current S9
process family covers normal root operations, graceful orphan retention and SPU
loss. The wider gate pass is not a substitute for those specific cases.

## After-action review

The plan was to compose the merged refusal acknowledgment with the existing
capture, Leave and publication owners. Completing the graceful branch also
required a real S9 management loop: stopping after SavePointNotTaken would leave
the run standing without an owner for its next command. The implementation now
keeps that owner and its original custody through retries and organ loss.

The joined-force fixture initially released its delayed member request before
the worker had retained the join. Checking the exact refusal code exposed the
resulting timeout. The corrected trigger follows the actual retained join;
the failed source and logs remain evidence. The cleanup-failure fixture also
confused the worker's trace verdict with admin's missing wire answer. Future
composition fixtures must assert each actor's own observed evidence separately
and retain failed attempts before correcting an expectation.
