# On-demand save composition

Date filed: 2026-10-26. Author working record for issue #146.
Updated: 2026-10-27. PR #356; review carries in #357.
Branch: code/state-demand-save-composition.
Base: #354 at dad0dd6ee84aed2cc580a56640b8e46a669419c7.

A serving SavePoint now carries its original absolute window, queues behind the
current turn, and freezes new input at the next turn boundary. The same capture
owner then drives the normal SQLite member and admin publication through the
matching SavePointTaken. A queued expiry records its refusal and lets the turn
continue. A missing snapshot or finished reply requires correlated cleanup before
retry; invalid cleanup or publication enters the actual failed-run handoff. A valid
request received after admin samples its send window uses a post-read clock reading.
Pre-Commit window expiry refuses the save and cleans up its original member capture
before serving resumes; post-Commit expiry keeps its failed-run disposition.

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
| PERSIST-TIME, PERSIST-BEGIN, PERSIST-EXPIRY, HARNESS-SAVE-TIME, TYPES-SAVE-TIME | Conforms on the exercised boundaries: post-read receive clock, original loaded duration and window, queued expiry without cancelling the turn, active pre-Commit expiry without publication, null demand tail and no demand CaptureComplete. Real admin expiry reporting remains Q1 in #357. |
| PERSIST-CLEANUP and HARNESS-CONTROL-FAILURE | Conforms on actual snapshot/finished timeout and active window expiry during image checking or preparation: original Abort, matching cleanup and retry. Wrong aborted identity remains protocol_failed/state_reply with full failed teardown. Before any member request, cancellation makes no Abort or publication request. |
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

The management receive owner samples the raw clock after recvmsg has retained the
opener. It checks clock reversal and retains that reading once for either serving
projection. The original packet remains owned if this clock observation fails.
The deterministic receive case dials first, samples the worker's pre-read clock,
then sends the valid admin opener before that same receiving pump continues.

The demand capture owner retains the expired phase and exact registered window
before changing state. It never reduces an already sent Commit to a recoverable
refusal. Before Commit, it withdraws an unsent continuation, settles any original
requested/confirmed record and records the refusal. An existing member capture
uses its original correlated Abort and existing two-second cleanup bound; before
the member request, cancellation only releases the local barrier. One valid reply
already in flight remains evidence and cannot advance publication. First fatal
failure and other applicable deadlines still win. The outer worker hands this
exact timeout back to the save owner even when another pump call observes it.

No Cargo dependency or crate boundary changes. Classification remains in the SPU,
loop orchestration in worker/dev_loop, and weaver-state remains the store member.
The manual second read checked both demand and Leave callers of the shared owner,
sole-reader projection, deadline transfer, original author custody, publication
ordering, and failure-account identity. It is not a Planner or security grade.

## Instruments and controls

Thirteen worker circuits use the normal SQLite member, actual Unix connections and
actual source filing: idle, queued, expired in transit, expiry during generation,
held filing receipt, actual admin publication, wrong Published hash through failed
exit, snapshot timeout/retry, finished timeout/retry, wrong aborted identity
through failed exit, and secondary management while queued and while holding the
image, plus original-window expiry during image checking and publication
preparation. A fourteenth worker case controls receive scheduling on actual sockets.
Successful circuits continue with another turn and inspect the captured
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
| Stop immediately on local preparation error | Admin actor test fails before the required Aborted response and cleanup. |
| Convert silence to CaptureNotReady | Admin response test rejects the invented worker result. |

The nine-control table describes the original 618ea138 evidence. Its logs retain
the mutation harness's non-unique replacement preflight and the corrected scoped
run; a harness preflight is not counted as a compiled control.

## Review correction evidence

The F1 socket case and F2's private image/prepare expiry cases failed on the
original production source. The real SQLite cases completed matching cleanup, retried
with a new capture identity and completed a following turn in the same session.
Six new private capture cases cover image/prepare expiry, an unsent Commit under
socket pressure, a valid late Prepared reply, blocked Beginning before member
admission, and an admitted Confirmed author under trace pressure.

| Compiled correction control | Detecting assertion |
| --- | --- |
| Use the pre-read clock as the receive witness | The valid same-pump admin request is rejected. |
| Bypass demand window-expiry retention | Prepare expiry propagates CaptureWindow instead of cleaning up. |
| Keep an unsent Commit queued | Flushing the actual peer after expiry exposes a sent Commit. |
| Treat post-Commit expiry as recoverable | The original post-Commit failure test no longer receives its unchanged bound. |
| Skip the expired member Abort | The retained capture does not produce the required original Abort. |

All five controls compiled and failed their intended assertions. They were restored
by hash before the final rebuild. The nine original controls remain separate evidence
for 618ea138; this correction does not claim to have rerun those mutations.

- Final source: 2,126 ordinary harness tests/doctests passed, with 748 ignored
  in that invocation. This includes all 72 capture cases and the unskipped #331
  pending-author instrument. Counts exclude nested child summaries.
- All 684 inventoried worker cases passed in one serial delegated run, including
  fourteen demand cases and the actual admin/SQLite participants. The other 64
  ordinary ignores are outside this run. The eight privileged admin-startup cases
  are not part of this harness correction.
- Strict harness all-target clippy, formatting, first/last lock and diff checks
  passed after the final Rust edit. All 1,042 source-manifest hashes still match.
  Five compiled protective controls were restored before this rebuild and run.
- Pinned nightly-2026-02-13, --locked, private target
  /tmp/weaver-state-146-demand-save-target, CARGO_INCREMENTAL=0 and four build
  jobs. Cargo and Git mutations were serial. Ordinary tests used four threads;
  delegated families used one. State and admin prerequisites were built in this
  same target. No production or device qualification follows from these checks.
- This correction changes only harness code/tests and this record. The admin and
  types source remains the reviewed 618ea138 source; their original feature and
  lint gates remain in that head's evidence deposit.

Correction source archive SHA256: 2c3b27151619df4032ebbacdeb4e77733e5fa62cf82e095553a0f3a19078efc3.
Correction source manifest SHA256: fd651c1de3be4d7f80d26870995d55372936af8068331d0acd6ebdf130c0d9a1.
Evidence: /bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-demand-save-review-20261027/.
The deposit manifest, source tree and commit receipt bind the published correction.
The original nine-control evidence remains at state-demand-save-20261026/ under
the same approved parent directory and applies to 618ea138.

The record is updated after the final gates; source hashes cover the exact code and
tests, with the earlier version of this record. No Rust file changes after those gates.
The evidence also retains the failed baseline, development and lint runs. One partial
family run omitted WEAVER_LEAVE_ADMIN_TEST_BIN and failed only its joint participant;
the corrected runner supplies the actual admin test binary explicitly.

## Planner finding dispositions

| Finding | Disposition |
| --- | --- |
| F1 | Corrected post-read witness; actual dial-before-sample scheduling case and C1. |
| F2 | Corrected pre-Commit expiry; two SQLite retry cases, six private cases and C2-C5. First failure, original window and post-Commit policy remain. |
| Q1 | Open operator election in #357. The timeout fixtures can read after expiry; they do not prove a typed late reply reaches real admin. No reply allowance or early-expiry margin is introduced. Silence remains Unanswered. |
| B1 | Admin-disconnect coverage and the surviving peer-end control remain open in #357 with L1. |
| B2 | begin_demand_boundary is the authoritative no-open-turn/no-input-snapshot guard. The worker's stage and save_blocks_turn checks remain scheduling guards; their independent necessity is not proved by the existing controls. |
| B3 | A secondary management dial can be dropped when it yields to an outstanding original save result/window deadline. Its caller may receive Unanswered. The original save keeps its cleanup duty and bounds. |
| B4-B6 | Demand window identity, refusal leg/code consistency and mismatched author-sequence cleanup remain open in #357. No batch completion is claimed. |
| B7 | The complete ordinary harness suite and composed worker families are rerun after the last Rust edit. The named review skills remain unavailable; no invocation or operator waiver is claimed. |
| B8 | Author-admission and advance duplication remain a composition carry. Queued save currently rejects either held slot; rollback gate checks caller-held custody. Their distinct predicates are not silently unified in this correction. |
| L1 | Filed in #357. A returned original is retained in StoppedAuthor; merely translating Busy into Pending would leave that stopped attempt installed. Actual disconnect/custody retry composition is still owed. |
| L2 | Filed in #357; the live member phase versus frozen halt account still needs a reproducer and composed teardown correction. |
| L3 | Planner's two one-off test sightings are retained in #357. They are not treated as reproduced runtime defects. |

## Remaining work and after-action

This is demand-save composition, not selection of the replacement dispatcher.
Complete lifecycle policy, #357 review carries, gate/organ death during paused capture,
pending #348/#349 elections and #350's graded policy questions, parent review carries including
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
have instruments on the actual ownership and process boundaries. Receive fixtures
must also match admin's dial-before-sample order; local publication work must be
allowed to outlive its window in a test so cleanup is exercised instead of assumed.
