# Save progress during organ settlement

Version: 1.1. Date filed: 2026-10-28. Issue #146 section E.

Branch code/state-save-settlement-composition builds on #375 at 8e902515.
It answers #374 C1(a) and C2, the pending-author ordering found while
composing those fixes, and the closed-peer refusal race found by the joint
expiry gate. Foundation remains unselected.

Implements: foundation Spec section 5.2 PERSIST-SAVE, PERSIST-BEGIN and
PERSIST-LEAVE-REFUSAL, section 5.4 PERSIST-TIME, harness amendment section 3
HARNESS-PUMP, HARNESS-CAPTURE and HARNESS-SAVE-TIME, harness Spec section 2.2
last-word settlement, admin amendment section 3 ADMIN-SAVE and ADMIN-SAVE-TIME.
Author assessment: conforms on the exercised progress, ordering, refusal and
original-window paths. This adds no policy election, timer or wire field.

## Ownership and consumers

An in-flight save can owe member work or refusal accounting while holding no
admin reply. Input and S9 now advance that original save during organ
settlement. Only capture boundary election requires a completed organ
observation. Permission reaches the save owner's boundary check, because
Requested can become QueueBoundary inside the same call.

| Consumer | Disposition |
| --- | --- |
| FoundationInput drive | Advance owned save work during settlement. Only Complete grants boundary election. Preserve original windows and avoid a second wait after save progress. |
| EarlierAuthor | Let the original claim finish. If a previously blocked author becomes available, return to the owed observation before an unrelated input wait. An already-available author can still require a held tool reply to be consumed. |
| Input management | Retain the existing boolean progress permission, including EarlierAuthor. This is not boundary permission. Report authoring keeps its original claim and runs before returning observation status. |
| PendingSave | Keep request bytes, peer, capture identity and deadlines. Forward actual settlement readiness to boundary election. |
| S9 resting save | Advance existing capture work without requiring a held admin packet. Pass Complete-only readiness to boundary election. Return Progress when the earlier claim clears so the caller does not wait on the idle queued save. |

The review's C1(b) held-packet queued-capture example was not reproduced.
ServingCapture probes an unexpected queued continuation before its step and
retains the protocol failure. A matching Aborted enters cancellation and removes
boundary eligibility. The confirmed admission gap is Requested finishing in the
same advance call, including when its earlier recorder claim delayed the loss
observation. The fixtures distinguish those sequences.

## Instruments

Input and S9 each consume a real held ImageChecked while stderr keeps the child
loss pending. Their member responder pauses before Acknowledge. With no further
admin packet held, the responder resumes and its ImageFinished must reach admin
before Prepared is sent or stderr is released. The capture window stays exact.
Input uses the SQLite member. S9 uses the existing scripted member over sockets.

The first controlled step has no readiness and counts its waits. Subsequent
member work uses the real monotonic clock and zero-timeout kernel polling, with
the fixture's real watchdog. This preserves the member measurement chronology
and admits only real readiness. Each step retains its original deadlines and
performs at most one wait. Settlement must still be pending when Finished arrives.

Both input and S9 retain Requested and QueueBoundary across real decode EOF with
stderr held open. They require Requested accounting to finish while SaveQueued
stays exact, with no CaptureWindow or frozen image. After settlement, the input
capture's frozen watermark includes the Recorded death. S9 checks the completed
observation before admin receives the real Beginning packet.

The EarlierAuthor case fills the real recorder and closes decode only when
Requested waits for capacity. It retains the same pending ticket and author
claim, consumes the separate residency EOF, and waits for real stderr EOF. After
trace pressure is released, the same Requested claim must become Recorded with
the organ observation still owed and no capture boundary. Its wait hook fails if
the worker waits again after that claim finishes. The recorder reader parses each
completed row once and keeps incomplete rows and original bytes. The matching
S9 case retains its real Requested claim through recorder pressure and EOF,
then requires Progress and no poll through the wait wrapper after that claim
becomes Recorded.

The gate case holds a real ToolAnswer before allowing the child to exit. It
retains the observed EOF while keeping that answer in its original slot, proves
EarlierAuthor with an already-available recorder, and requires the input owner
to consume the result. The remaining pair filing and trace checks then complete.
An earlier control survived the existing crossed-result case because that case
routes the answer before EOF. That survivor prompted this missing ordering case.

C2's S9 assertion now explicitly requires the retained save request before the
later death record in the held-reply case. The other SPU-dead case retains the
opposite ordering. Every case still requires exactly one SPU-loss record.

## Refusal after peer closure

The joint graceful-expiry gate exposed an admin ordering race. The worker can
finish cleanup, carry CaptureRefused and SavePointNotTaken, and close the Leave
socket at S9 before admin sends its prepared Aborted. Admin formerly stopped at
the failed send without reading those queued packets, then escalated a standing
worker. Instrumented failure evidence identifies Channel(Closed) at that send.

Capture send distinguishes EPIPE, ECONNRESET and ENOTCONN from other Closed
failures, including a failed clock witness. Only this socket evidence on an
uncommitted Leave with an accepted Beginning admits the existing correlated
refusal reader. It proves no send, member cleanup or lifecycle result. Admin
still requires the matching CaptureRefused and final disposition under the
original shared-tail and outer bounds.

The Planner's R1 class correction covers all four capture-send sites:

| Send | Peer closure disposition |
| --- | --- |
| ImageChecked, while Hashing | Before Commit, retain the original Leave and drain its correlated refusal. |
| Prepared, while Preparing | The same pre-Commit continuation, including closure after the final publication probe. |
| Published, while Publishing | Commit already stands. Preserve the fatal send result and broken publication guard. |
| Aborted, while Aborting | Preserve the existing typed continuation through the shared pre-Commit Leave predicate. |

Initial Leave, demand and join gain no continuation. A later capture expiry in
AwaitCaptureRefused reduces under the same refusal-only tail and outer deadline.
A direct or publication-checkpoint receive-clock failure stays fatal even if a
valid refusal packet is retained and the clock later recovers.

Seven new actor cases close the R1 coverage gap. A test-only one-shot hook runs
after publication checkpoints and encoding, immediately before the real send.
The worker peer then sends its actual refusal packets and closes. ImageChecked
and Prepared exercise forced and graceful results, with and without subsequent
capture expiry. Negative cases retain missing, foreign, mismatched and crossed
Commit outcomes. Separate cases exclude demand and Published, preserve the
indexed post-Commit artifact's guard, and keep pre-read and post-decode clock
failures fatal. Filesystem cleanup and the original index are checked.

R2's probe-only alternative is declined: a worker can close between the final
probe and the send syscall, which the new hook reproduces. R3 does not justify
widening every Closed into a refusal continuation, since Closed also carries
clock failure. No new full-buffer peer-close composition is claimed. Existing
channel pressure cases remain the transport evidence. R6's redundant Aborted
predicate is replaced by the shared actual-Leave admission. R4/R5's observation
precision and R7's zero-timeout fixture polling remain carried.

The deterministic positive case fails on the unchanged #375 admin runtime at
Channel(Closed). It passes on both snapshot and finished refusal with the fix.
Eight negative sequences reject missing, foreign, mismatched, duplicate and
Commit packets. A third case tests clock reversal before and after physical
Aborted transmission. The exact #375 joint-process diagnostic happened to pass
both timing cases, so it is not evidence that the baseline process run reproduced
the race. The deterministic actor failure and captured current process failure
establish the cause separately.

## Validation and limits

The private target is /tmp/weaver-state-146-serving-organ-loss-target with
CARGO_INCREMENTAL=0, pinned nightly and --locked. Lock and formatting pass.
Both affected crates pass strict all-target clippy. The R1 admin package run
passes 592 cases serially: 560 main-binary, three relay and 29 integration cases,
with 71 inherited ignored. This includes the seven new send-boundary cases and
the earlier ten expiry actor cases. No namespace skip was reported. An initial
sandbox run of the seven cases failed at its forbidden setresuid fixture call.
The compiled tests passed in a delegated root scope, then the ordinary full
package passed outside the sandbox.

The ordinary harness gate passes 2,174 tests/doctests: 2,093 library, seven worker,
43 integration and 31 doctests, excluding nested child summaries. There are
843 ignored cases and the known #331 flake is explicitly filtered. Those harness
and clippy results are retained from the first broad gate. Source hashes prove
that only admin files changed afterward, with no harness dependency or Cargo
configuration changes. They are reused results, not a second invocation.

At 703f9fb1, all 136 selected process identities passed under a root-owned
delegated scope with the rebuilt admin binary and SQLite member. For R1, all 21
joint identities are rerun against a fresh build of that exact published harness
and the corrected admin. They cover forced and graceful capture refusal, both
expiry legs, cleanup failures, demand save and joined Leave. All 21 pass. The
unpublished dispatcher work was copied aside, the parent harness installed for
this gate, and the original work restored by hash afterward. The gate snapshots
prove that its harness source is the published parent's and that its sources did
not change while the joint cases ran. The other 115 process cases are retained
baseline evidence, not rerun claims for this admin correction.

Ten separately compiled protective controls fail as intended. Eight remove the
harness's no-packet progress, boundary permission or earlier-owner handoff in input
and S9. Two remove admin's peer-close continuation or admit a clock failure as peer
closure. Each source is restored by hash. The earlier crossed-result control
survivor is retained separately and is not counted as a caught control.

Five further independently compiled R1 controls remove the send continuation,
its expiry reduction, its actual-Leave restriction, its pre-Commit restriction,
and its receive-clock failure guard. Each fails its intended actor case. The
clock control fails both direct and checkpoint cases. Source hashes are restored
and all seven send-boundary cases pass again. These controls are separate from
the ten controls on the initial head.

The early failed builds and fixture runs remain separate evidence. They exposed
test-only visibility, missing member readiness, an invalid fixed-clock/member
measurement combination, the unwanted input wait, and repeated trace parsing.
They are not counted as passing gates. The first broad process gate passed
134 of 136 cases and exposed the two graceful-expiry failures above. Its logs,
the diagnostic runs and the unchanged-runtime actor failure remain preserved.
No temporary diagnostic print or mutation is part of the submitted source.
Models and classifiers remain doubles.

#375's Planner grade is zero fix with D1-D8 carried. Its D1 physical-send actor
test gap is separate from this save-settlement act. #374 C3-C6, #373 B1-B10,
#369, remaining #359 fixture work and #372 T8 remain tracked. These author checks
do not supply production, security or artifact qualification. No broad #146
checkbox closes and no deployed agent changes.

## After-action review

The initial plan was to widen save progression and preserve boundary permission.
Composition exposed two distinct permissions inside EarlierAuthor: allowing the
earlier owner to finish, and allowing capture after the loss is Recorded. It also
exposed an idle wait after that owner finished. The resulting tests follow both
permissions through the same worker call and through the subsequent observation.
Cross-review found the same completion-to-wait gap in S9, which this act also fixes.
The broader joint gate then showed why a generic Closed error cannot authorize
draining a completed refusal. Preserve the transport error's origin so a later
valid packet cannot conceal a clock failure.

Keep exact owner claims and causal socket barriers in future composition tests.
Use real clocks when consuming measurements from real processes. Return to the
owed observation when an earlier author clears, while letting an already-ready
tool-result owner run. Apply that distinction to the complete dispatcher act.
