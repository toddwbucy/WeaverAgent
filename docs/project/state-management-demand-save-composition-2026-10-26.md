# On-demand save composition

Date: 2026-10-26. Author working record for issue #146.
Branch: code/state-demand-save-composition.
Base: #354 at dad0dd6ee84aed2cc580a56640b8e46a669419c7.

A serving SavePoint now carries its original absolute window, queues behind the
current turn, and freezes new input at the next turn boundary. The same capture
owner then drives the normal SQLite member and admin publication through the
matching SavePointTaken. A queued expiry records its refusal and lets the turn
continue. A missing snapshot or finished reply requires correlated cleanup before
retry; invalid cleanup or publication enters the actual failed-run handoff.

This advances #146 section E's loaded-save consumer, author migration and failed
teardown items, and section G's composition evidence. No broad checkbox closes.
Foundation remains unselected; main.rs still serves dev_loop::drive. The paired
wire, worker and admin must cut over together after the remaining qualifications.
The operator alone merges and deploys.

Implements: state-management-foundation-Spec section 5.2 PERSIST-SAVE,
PERSIST-PUBLICATION-GUARD and section 5.4 PERSIST-TIME, PERSIST-BEGIN,
PERSIST-EXPIRY; persistence-contract-amendments PERSIST-CLEANUP;
weaver-harness-Spec-amendment
HARNESS-CAPTURE, HARNESS-SAVE-TIME, HARNESS-CONTROL-FAILURE and
HARNESS-RECORD-LIFETIME; weaver-admin-Spec-amendment ADMIN-SAVE and
ADMIN-SAVE-TIME; weaver-types-Spec-amendment TYPES-SAVE-TIME.

| Section | Author disposition, pending Planner verification |
| --- | --- |
| PERSIST-SAVE and HARNESS-CAPTURE | Conforms on the exercised demand paths: original load, ID at acceptance, requested before Beginning, whole Beginning before frontier, owed filing settled, no next turn until completion. Gate-death disposition remains part of the lifecycle work below. |
| PERSIST-TIME, PERSIST-BEGIN, PERSIST-EXPIRY, HARNESS-SAVE-TIME, TYPES-SAVE-TIME | Conforms on the exercised boundaries: required strict window, loaded duration and receive reading, no fresh deadline, queued expiry without cancelling the turn, null demand tail and no demand CaptureComplete. |
| PERSIST-CLEANUP and HARNESS-CONTROL-FAILURE | Conforms on actual snapshot/finished timeout, matching late-result discard, matching aborted and retry. Wrong aborted identity enters protocol_failed/state_reply and full failed teardown. Earlier private cleanup cases remain in the affected suite. |
| ADMIN-SAVE, ADMIN-SAVE-TIME and publication guard | Conforms on actual loaded-copy verification, locking, publication and final report; local preparation abort and first-cause retention; silence remains Unanswered. Root-only publication artifacts and guard removal are checked by the joint participant. |
| HARNESS-RECORD-LIFETIME | Conforms at the demand handoffs: original request bytes, peer, capture account, waiting author and first failure remain owned through backpressure, cancellation and failed exit. |

## Implementation and self-check

The paired capture projection adds only the serving SavePoint opener. The legacy
selected LifecycleDirective stays unchanged. A serving lifecycle connection picks
its projection before its sole recvmsg reads; a windowless SavePoint cannot fall
back to legacy. Received descriptors, raw bytes and the original exchange remain
with that owner. The existing strict codec checks direction and phase.

ServingCapture replaces the internal Leave-only name and keeps the shared capture
protocol. Its origin distinguishes demand's original received window from Leave's
existing tail. Demand allocates before the requested record, so failure reporting
can name QueueBoundary even when that record is still owed. Capture boundary
admission requires its Recorded receipt and no open turn or input snapshot. The
same registered timestamps transfer from SaveQueued to CaptureWindow.

QueuedSaveAuthor is a private claim for the exact requested/refused observation,
not general author permission. It can record while a generation or read retains
its owner, but cannot take a held caller/template, pending author or filing
original. A queued-expiry race at admission retains its exact event and claim.
Generic resume and an impostor private claim cannot consume that event.

The worker's PendingSave stays on the same serving seat while the current turn
runs. Input, generation and management lend its actual peer to the common pump.
The worker loop chooses queue_save explicitly; harness code does not select a
workflow. Secondary Observe and out-of-order requests retain their own replies
without replacing the original save. Graceful Leave and rollback cannot acquire
that seat while demand owns it. Fatal capture evidence moves through the same
halt, FoundationFailedHandoff and original FoundationRun into failed teardown.

Admin retains the real invocation lock, verifies the observed loaded artifact and
uses its parsed declaration. Its send-time window is unchanged through copying,
hashing and publication. The final demand report must equal the indexed identity;
only matching Taken can finish and remove the publication guard. A local copy
failure sends the original Aborted where allowed, removes the unlisted root part
and waits for cleanup while retaining the first cause. Silence never invents a
worker phase, even when a publication guard exists.

No Cargo dependency or crate boundary changes. Classification remains in the SPU,
loop orchestration in worker/dev_loop, and weaver-state remains the store member.
The manual second read checked both demand and Leave callers of the shared owner,
sole-reader projection, deadline transfer, original author custody, publication
ordering, and failure-account identity. It is not a Planner or security grade.

## Instruments and controls

Eleven worker circuits use the normal SQLite member, actual Unix connections and
actual source filing: idle, queued, expired in transit, expiry during generation,
held filing receipt, actual admin publication, wrong Published hash through failed
exit, snapshot timeout/retry, finished timeout/retry, wrong aborted identity
through failed exit, and secondary management while queued and while holding the
image. Successful circuits continue with another turn and inspect the captured
SQLite source boundary. Retry uses the unchanged loaded identity and a new capture
ID only after matching aborted. Failure circuits require actual cleanup/reap,
one fault/unload and no healthy release or Taken.

The actual admin participant verifies the published files and index, guard removal,
Open marker, live run lock/cgroup and subsequent turn. A separate admin process
proves a second save and forced unload both refuse InvocationInFlight. The initial
contender used a thread and failed because POSIX invocation locks belong to the
process; the test now crosses the real process boundary. No runtime lock rule was
changed. Logs retain that failed fixture run.

| Compiled control | Detecting case |
| --- | --- |
| Remove the loaded save duration comparison | Invalid-window test admits the altered duration. |
| Remove the open-turn boundary guard | Boundary test begins capture during the retained turn. |
| Return the event at an expiry/admission race | Private author test loses the retained original. |
| Block the queued observation behind an active generation owner | Expiry circuit cannot answer while generation is paused; its child watchdog and missing-answer assertions fail. |
| End demand with Leave's CaptureComplete | Exact saved-report test rejects the wrong terminal form. |
| Permit Beginning to replace the original window | Admin window test accepts a changed anchor. |
| Skip indexed identity equality on Taken | Admin report test accepts the wrong image position. |
| Stop immediately on local preparation error | Admin actor test fails before the required Aborted and cleanup. |
| Convert silence to CaptureNotReady | Admin response test rejects the invented worker result. |

All nine controls compiled and failed their named tests. The mutation harness
stopped once before applying a non-unique turn-guard replacement; its next run
scoped that replacement to the boundary method. This was a harness preflight, not
a compiled control. Source bytes were restored after every control sequence.
Afterward only two fixture changes landed: the exact wrong-cleanup failure assertion
and the old management SavePoint fixture's required window. The later lint correction preserves the controlled properties and leaves every
detecting test unchanged.

## Final verification

- Ordinary tests/doctests: types default 189, types config 247, admin 546
  (66 ignored), harness 2,120 (745 ignored); zero failures. These runs precede
  only the mechanical capture-owner lint correction below.
- Final source: all 66 affected capture cases and all 681 inventoried worker
  composition cases pass, including eleven demand scenarios and the actual
  admin/SQLite participants. Nine compiled protective controls fail as intended.
- Strict all-target lint passes for each changed crate, plus types with config.
  Formatting, first/last lock and diff checks pass. Final source hashes match.
- After the ordinary suites, lint moved the existing large-error allowance back
  onto into_failed, collapsed one nested if and applied two equivalent Boolean
  rewrites. The complete before/after patch is final-restored/mechanical-lint.patch.
  No other source changed between ordinary tests and the final-source run. The
  full ordinary suites were not needlessly repeated for those equivalent forms.

Pinned nightly-2026-02-13, --locked, private target
/tmp/weaver-state-146-demand-save-target, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4. Source stayed fixed during each gate sequence; Cargo and Git mutations are
serial. Counts exclude nested child summaries. The normal member and actual admin
test executable are built in the same private target. Combined privileged tests
use the operator-authorized root-owned delegated scopes.

Final source archive SHA256: 9e948ff9ac2dbb345487879315361dc92758ca54e5f39615a0e3fadb314fe232.
Final source manifest SHA256: 9801a537241cd1fcbb82f91e4f42b9d7506ed8be5d5d9f8a62e66ee6466fcba0.
Evidence: /bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-demand-save-20261026/.
The deposit manifest and publication receipt identify its immutable contents.

The archive contains tested tracked and new source files. This record is completed
after the gates and is retained separately. Evidence includes the failed fixture
iterations, controls, exact commands, inventories and executable/source hashes.

## Remaining work and after-action

This is demand-save composition, not selection of the replacement dispatcher.
Complete lifecycle policy, gate/organ death during paused capture, pending #348/
#349 elections and #350's graded policy questions, parent review carries including
#355, author-site census, deploy/consumer migration, production language and prompt
artifacts, measured limits, security review and qualified coupled cutover remain.
The actor tests and the joint SQLite circuit use model/classifier doubles and
synthetic account lookup; they do not qualify deployment role provisioning. They do
not qualify a language model, classifier, GPU tuple or production timing limits.
No whole-workspace, device, security or production qualification is claimed.
The named code-review and verification-before-completion skills are unavailable;
manual review and executable gates are reported without claiming those invocations.

After-action: a pending save must own its observation independently of a current
generation while respecting the existing author slot. The active-generation expiry
case exposed a stall that idle capture tests missed. The full admin circuit also
requires a separate process to test its process-owned lock. Both properties now
have instruments on the actual ownership and process boundaries.
