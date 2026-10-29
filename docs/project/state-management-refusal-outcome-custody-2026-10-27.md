# Admin refusal outcome and publication cleanup

Date filed: 2026-10-27. Author record for issue #146, section E.
Branch: code/state-refusal-outcome-custody.
Base: code/state-resting-failure-composition at 3d5dfe85 (PR #372).

This act answers #370 Planner V2/V5 and #373 Planner X1-X4, X7/X8 and
Z1-Z4, Z6/Z8. One terminal-unwind predicate governs every paired capture
receive, publication checkpoint, final refused Leave and joined exchange.
Public Leave and demand-save projection use that same census. Failed local
publication cleanup preserves its original preparation failure and checks the
current guard before attempting another Broken write.

## Authority and consumer review

Implements: state-management foundation Spec section 5.2,
PERSIST-LEAVE-REFUSAL and PERSIST-PUBLICATION-GUARD; admin amendment section 3,
ADMIN-SAVE; types amendment section 5 TYPES-PERSIST. Author assessment: conforms
for the refusal and custody paths described below. The terminal set and its
consumer scope travel together in code and paired Specs: Malformed,
DescriptorsUnusable, OrganRefused with organ Gate, and defensive FailedRun with
the observed run and any known capture ID. No wire, deadline or force policy is
added. #371's timing proposal remains unimplemented pending the operator's
election and merge.

| Consumer or failure path | Disposition |
| --- | --- |
| Ordinary capture receives and publication checkpoints | The shared negative classifier admits the terminal set. A worker's direct publication claim or post-admission standing code is Protocol. Original packets remain retained. |
| AwaitCaptureRefused after admin Aborted | Uses the same classifier. An invalid packet remains a separate cleanup failure; it cannot overwrite the original local preparation failure or cause another continuation. |
| AwaitRefusedLeave | Uses the terminal predicate after correlation. Graceful SavePointNotTaken requires its retained missed leg and graceful mode. Forced Left requires no save report and no_state false. |
| Joined Leave | Uses the same terminal set and run comparison. Foreign or unavailable run identity cannot establish FailedRun. A join has no Beginning and cannot compare a capture ID it never received. |
| Initial admission and standing refusals | OutOfOrder, ActivityNotAtRest and Unauthorized remain initial lifecycle refusals; the paired codec excludes the legacy SavePointNotTaken shape. A started demand capture retains its own SavePointNotTaken cleanup outcome; CaptureNotReady applies before Beginning. Leave cannot skip CaptureRefused with that shape. |
| Legacy serving | follows_the_unwind still complements the same four standing cases. Its active policy is unchanged; paired capture never derives its terminal set by that complement. |
| Public Leave, joined Leave and demand-save projection | The shared projector preserves admitted typed codes, including a gate-attributed nested reason. Direct worker publication claims are Malformed. Worker FailedRun cannot supply root-owned publication identity or locations; before Commit it projects Unanswered. Post-Commit FailedRun is constructed from the local publisher's account. |
| Publication fail, shared by seed, demand and Leave | If the judged guard already has the exact desired Broken bytes, sync its directory without replacing it against stale Pending bytes. Other guards keep their replacement and mismatch checks. |
| Cancel-unconfirmed cleanup after an earlier failure | Preserve the original cause, re-probe actual remaining artifacts, and restore a missing guard if unlink succeeded before directory sync failed. |
| Completed publication | The existing early return preserves the completed save. |

The live gate Lower producer wraps its refusal in OrganRefused with organ Gate;
an invalid Lower reply becomes Malformed. The entered rollback terminal retains
that refusal instead of Left. The failed-unwind rules also name DescriptorsUnusable.
FailedRun has no current harness producer: it is admin's projection shape. Its
worker-side checks and tests are defensive boundary evidence, not a claimed live
producer. The paired Specs now say this explicitly (Z3).

The shared publication owner retains the existing-Broken branch (X6, graded sound).
Using only the previous failure would miss the separate case where guard unlink
succeeds and directory sync fails. Existing tests cover both paths.

Z5 is carried: an invalid worker refusal and a worker-reported Malformed have the
same public code. Their retained packet and internal cause differ. No new external
refusal code is invented. The first filing dates remain unchanged. This revision
uses foundation 0.27, admin amendment 0.24 and types amendment 0.36, avoiding the
version numbers currently proposed by #371; the eventual join must reconcile headers.

## Verification

The channel/publisher cases cover Image, Hashing, Finished, Preparing, Commit,
AwaitCaptureRefused and AwaitRefusedLeave. They exercise forced and graceful Leave
and demand's applicable receive/checkpoint stages. Nine named invalid lifecycle
shapes include publication claims, retry codes, an SPU refusal, Unanswered and
NoResidency. Legitimate terminal codes remain accepted. Tests retain the original
cause on cancellation, reject missing FailedRun capture identities, check root
artifact counts, and require no further packet after refusal.

One older forced-Leave test accepted SavePointNotTaken without CaptureRefused.
It now requires Protocol, consistent with merged #348. The demand and graceful
cleanup tests preserve their authorized SavePointNotTaken paths. Synthetic public
projection tests cover owner, joiner and demand, including a nested gate reason
whose identity must remain attributed to that gate.

The actual joined-invocation fixture covers the shared terminal census, direct
publication claims, SPU refusal, NoResidency, Unanswered, foreign FailedRun and a
FailedRun received while the marker cannot supply a run. It retains exact packet
bytes and verifies publication and room files are unchanged. All four joined-Leave
fixture families pass; these use private children and actual invocation locks.

Five independently compiled protective controls fail their intended assertions:
restore the broad receive census; bypass the public projection census; restore
the broad joined census; accept a foreign joined run; accept an unobserved joined
run. Runtime sources are restored byte for byte before final gates.

Final revision gates:

- 569 admin tests pass: 537 main-binary, three relay unit, six invocation, three
  manifest and twenty relay integration cases. The package retains 71 ignores.
- All four isolated joined-Leave cases pass, including the expanded scenario matrix.
- All eleven actual admin/worker/SQLite capture-refusal cases pass with this
  revision's admin and the freshly built harness executable.
- Strict admin all-target clippy, formatting, lock, diff and the three
  DocsMirrorTests pass. The earlier 209-case deployment gate belongs to ea2bc781;
  it is not represented as rerun for this revision.
- All 762 Rust, Cargo and toolchain source hashes match across the final gates.
  Cargo uses the pinned toolchain, --locked and the checkout's private target.

The first new matrix run exposed an incorrect test expectation: an unconfirmed
root part is removed on refusal, so the artifact count legitimately falls. The
fixture now checks that cleanup while retaining guards and existing published
files. Its failed log remains in the evidence. An initial joint invocation used
the older cached harness executable; its result is kept separately and the same
eleven cases were rerun using the exact executable reported by the fresh build.

The verification-before-completion skill was found and applied for this revision.
The code-review skill invocation remains unavailable; manual source, caller and
Spec review was performed. These are author checks, not Planner/security or
production qualification.

The first correction constrained only the final acknowledgement consumer. The
second review exposed the same authority leak at other receives and in the joiner.
The correction now follows the shared classifier through all capture consumers
and through both public projectors. The consumer table above is the review scope.

## Remaining work

Planner regrade and operator merge remain owed. #370 V1 awaits #371; T2/T5 and
V3/V4/V6-V8 remain carried. #374 received a zero-fix Planner grade with A1-A6
batch findings; its save-ordering and S9 work is next. #331, #359's remaining
fixture work, #369 and the broader #146 composition, qualification and cutover
obligations remain open. Foundation remains unselected. Only the operator merges
and deploys.
