# Returned author originals and caller custody

Date: 2026-10-24. Branch: `code/state-author-failure`.
Parent: draft #312 at `b24a260e81409c1f0d30d7128c00ab912ab7f5fc`.
Authority: merged #276, `weaver-harness-Spec-amendment`
HARNESS-RECORD-LIFETIME. This is implementation evidence for #146 and the
AuthorFailure finding in #255 review 5636484478 row 1. It is not a new
protocol or production qualification.

## Behavior and scope

The shared author and both resume roads return a move-owned AuthorFailure,
containing the existing RunFailure and an optional exact AuthoredEvent.
RunFailure remains an account of failure, wait or admission refusal; it
carries no event. AuthorFailure is not Clone, and there is no conversion
from it to RunFailure which could silently discard a returned original.

A second submit remains a caller error. With no prior fatal cause it latches
ProtocolFailed. With a prior fatal cause it returns that immutable cause.
If the existing held caller or first-fatal slot prevents retention, the new
original moves back to the caller. Neither earlier original is replaced.
The pending author's ticket, turn metadata, release identity and original
wait remain with that first attempt. The error neither creates a sequence
nor grants retry or continuation.

An absent returned original means the exact input remains in an existing
context slot or has already reached Recorded. Early owner refusals still
use the existing single held slot. A caller flag meaning that submission
has started is not proof of recorder admission. A returned original clears
that flag; the caller keeps the error and exact input in its stopped owner.

When a real CapacityPending author is withdrawn for an existing mandatory
account, only that attempt's unadmitted preparation and bookkeeping retire.
An empty held slot may retain it. An occupied held slot requires its return
through AuthorFailure. No wait is renewed, no account is skipped, and no
sequence is assigned. Both generic and private resume paths retain a fatal
cause ahead of an otherwise completed sequence, so a late observer failure
cannot be converted into successful continuation.

Each logical caller owns at most one stopped return, in an Option held by
that operation. This is not an overflow queue. Freshness is a driver of an
earlier caller, rather than its owner: it retains a withdrawn original in
the existing waiting slot, marked withdrawn, until the matching caller
takes it through resume. It never submits that original again.

## Finite caller census

Every production use of the changed shared author methods was inspected.
The census includes their private helpers, callers which already hold a
claimed AuthorRecord, and the two other consumers of the former void
retain_refused helper.

| Caller | Exact return and failure disposition |
| --- | --- |
| Shared author admission and its generic/private resume helpers | Retain the first held and first-fatal originals; return only the new unowned input. Pending and Recorded facts remain distinct. |
| InputOffered and input Refusal | InputAuthor retains the original stage and returned event before propagating the failure. |
| Input TurnStarted and MessageUser | Restore the complete InputTurn after failure; retain its accepted source, exact returned event and submitted-state flag. |
| Mark-template fault | The marks operation retains the fault return and clears its submitted-state flag; its entry still checks liveness. |
| Repair instruction, association and refusal authors | RepairAuthor retains the return, resets its internal author-driving flag before propagation, and blocks replacement or retry. Cancellation cannot discard a retained return. |
| Startup Load | Restore the returned event into the existing Load slot. Startup enters Stopped on the failure rather than constructing a replacement Load. |
| Worker organ-fault report | FoundationInput retains its original organ report and returned event. The report is cleared only after Recorded. |
| Worker Stop TurnClosed | StopWork retains the return and cannot supply a close sequence, gate close or successful Stop answer. Management checks context liveness before driving it. |
| Worker emission refusal | EmissionWork retains its exact refusal and stage through cancellation; it cannot report a recorded refusal or restart the attempt. Its engine checks liveness before advancing. |
| Measurement, placement and Enter-refusal claimed authors | AuthorRecord retains the returned original. Only that unrecorded attempt's ticket is cleared; its private identity remains. Existing entry checks precede continuation. |
| Filing and OUTPUT transactions | The transaction retains the returned original and failure before propagating; a stopped return cannot be turned into another author or a completion. |
| Outbound runtime fault | OutboundWork retains the fault and its submission disposition; no sort or dispatch result follows from the error. |
| Pair rejection | The context's existing rejection owner retains the return and submission state; acknowledgement still requires its real Recorded sequence. |
| Freshness | The previous caller's exact withdrawn input waits in the existing slot. The matching resume returns it with current failure precedence and no ticket or resubmission. |
| Read recall BusyAuthored | The existing ReadTransaction retains the exact rejected original separately from its immutable recall facts, then enters the existing record-failure path. |
| Failed pair-filing admission | The existing PairTransaction retains the exact attempted event alongside its pair; the failure cannot replace first-fatal context evidence. |

StoppedAuthor owns only returned input; a failure with no returned input
leaves the original in its existing context owner. Callers must not infer
Recorded from that absence. A private AuthorRecord never clears a Recorded
sequence to accept a return. Existing normal wait/resume and successful
recording paths continue to use their original identities and bounds.

## Instruments and validation

The focused tests use actual recorder capacity, Unix sockets and the shared
author/resume code. They cover second and third submissions, immutable
first-fatal evidence, pending ticket/turn/release preservation, both resumed
withdrawal paths with an occupied slot, caller retention through the real
halt cap, and refusal to grant Recorded after failure. Pointer plus value
checks distinguish an exact moved original from a clone.

Actual input owners are exercised at InputOffered, Refusal, TurnStarted and
MessageUser. A worker EmissionWork test exercises returned refusal custody
through cancellation. A freshness test drives an earlier caller's deferred
author and verifies that only the caller can take its exact returned input.
Negative compile pins enforce the absence of Clone on AuthorFailure and of
a lossy From<AuthorFailure> implementation on RunFailure.

The occupied-slot withdrawal fixture deliberately raises mandatory-account
debt after obtaining a real CapacityPending ticket; public owner admission
currently prevents this transition. It tests the specified defensive return
without claiming an integrated production path can cause it. The halt-cap
case retains a real caller return while advancing the actual halt reporter
to AbortRequired; it does not establish complete failed-run teardown.

Validation used `CARGO_TARGET_DIR=/var/tmp/weaver-state-146-author-failure-target`,
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4` and the pinned toolchain. Cargo,
mutations and Git mutations ran sequentially.

| Gate or instrument | Result |
| --- | --- |
| `process/gates/lock.sh`, before and after Cargo | Passed, exit 0 |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy -p weaver-harness --all-targets --locked -- -D warnings` | Passed |
| `cargo test -p weaver-harness --locked -- --test-threads=4` | 1,979 passed, including 22 doctests; 183 ignored in this ordinary run |
| Focused `author_failure_` tests | 12 passed, including the existing coordinator ignored-result test and restored nonfatal pending-owner regression |
| Delegated gate startup | 46 passed |
| Delegated worker startup, input, Observe and Stop | 17, 19, 14 and 16 passed |
| Delegated worker generation and emission | 20 and 33 passed |
| Protective runtime mutations | All 16 compiled and failed their targeted tests |
| Negative type mutations | Both adding Clone and adding a lossy failure conversion failed their intended compile pins |
| Diff and source checks | Passed; all 659 tracked/new crate and configuration files match the final gate manifest |

The 165 delegated cases ran serially through an isolated delegated scope.
Observe and Stop used a freshly built actual admin test executable from the
same private target. Eighteen inherited harness ignores and the separate
eight privileged admin-startup instruments remain outside this run. No
deployed agent was changed. The gate manifest SHA-256 is
`fe380d6664306cc0525a8e5de869d7e1a911bae2acf3085addaf412a5bdb169f`.

The first full run, before joining the corrected parent, had 1,889 passed,
nine failed and 183 ignored. It exposed owner-refusal ordering and private
continuation regressions, a failing-intake custody case, and older custody
expectations. The logs and source patch are retained. All those tests pass
in the final suite. Initial compile/lint failures are also retained.

The first 17 controls preceded restoration of the valid nonfatal pending-owner
regression; the extra ordering control and final gates include it. Evidence
retains that earlier test file, mutation source copies and both source
manifests. Every mutation was restored byte-for-byte before final gates.

## Remaining work and after-action review

This act changes the new internal recording context and all its current
production consumers. The legacy serving path remains active until the
qualified coupled cutover. Full author-site migration in worker/dev_loop,
failed-run teardown, rollback Busy/OperationRefused exits, final world close,
and the remaining #146 composition and qualification checklist are still
required. The serial delegated fixture lane does not resolve #313's
pre-existing parallel clock race.

Planner regrade 5637136323 clears parent #312 at b24a260e on the agreed
severity line (zero fix rows). Its batch items remain tracked. In particular,
row 3's worker prefix step still turns held Busy into a fatal fault, and row
7's rollback still lacks a healthy Busy exit. This signature and exact-return
migration does not claim to close either composition carry. The operator
directed on 2026-10-24 that this reviewed parent remain the stack root for
the subsequent draft corrections and composition work.

The Executor's code-review and verification-before-completion skills are
unavailable in this environment, as disclosed on #312. No invocation or
Planner grade is claimed by this record. The branch remains draft for
independent review; only the operator merges or deploys.

The initial implementation put pending-author detection ahead of existing
private owner refusals. Regression tests showed this changed established
ReadOwned/RepairOwned precedence. The check now follows genuine owner and
phase checks while a second held original is still refused before any slot
can be replaced. Review also found that freshness drives another caller's
author: keeping a withdrawn marker in that same waiting slot preserves the
owner instead of creating a second return queue. The reusable practice is
to enumerate every move-owned error consumer before changing the signature,
then test custody and acknowledgement separately at actual caller stages.
