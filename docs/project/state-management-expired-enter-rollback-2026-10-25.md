# Expired Enter rollback composition

Date: 2026-10-25. Author working record for issue #146.

## Plan and authority

The previous slice, #343 at `9cb76cc2`, consumes failure during the
classifier-refusal rollback. This act connects an original Enter expiry to root's
explicit rollback and retains the actual startup account through disposal. It
does not turn elapsed Enter time into a component failure or renew boot work.

Authority: merged HARNESS-ENTER-WINDOW, HARNESS-OPEN, HARNESS-RECORD-LIFETIME,
HARNESS-HALT and HARNESS-CLOCK, and weaver-harness-Spec section 6's rollback row.
The intended composition site is worker/dev_loop's unselected foundation adapter;
its connection to this owner is still owed.

| Boundary | Required evidence |
| --- | --- |
| Original expiry | The exact registered Enter window expires; no new boot request, Ready, refusal or halt is manufactured. |
| Custody | Every startup representation returns the same context and resource identities with its original records, requests, packets and proofs. Foreign joins return both inputs. |
| Rollback acceptance | An actual authenticated, completely received Leave with rollback true supplies the separate cleanup window. Observe and unrelated directives supply none. |
| Owed work | Preserve its own bound and disposition, settle or explicitly retain incomplete work before retirement, and never grant live boot authority. |
| Release | Release only actual children and endpoints; retain exact send/reply evidence and the load cause. No wind-down or save point. |
| Terminal | An actual load bracket owes its terminal ordering. Left requires the actual terminal and completed drain; expiry or incomplete accounting cannot fabricate success. |
| Before Load | Close unadmitted descriptors without a record, retain the lifecycle position, and answer actual rollback OutOfOrder. Admin's section 3 marker-write rule uses this answer to preserve the prior marker. |
| Failure during rollback | Preserve the first real failure and the earlier accepted Leave/halt cap. |

## Implemented local boundaries

The consuming handoff preserves the exact expired Enter, run/port identities,
startup evidence and actual resources through each retained startup representation.
It grants no boot continuation. Before Load it closes the actual unadmitted state
and trace descriptors, without a record, and keeps the lifecycle listener for
the actual OutOfOrder rollback answer. The trace writer now closes an empty,
unadmitted sink while its recorder remains owned; generic abandonment is unchanged.

After Load the owner waits on the actual root listener, with a separate original
read deadline for each accepted peer. Only the actual rollback Leave arms cleanup.
Observe, Stop and other directives do not grant that authority. A complete received
directive survives the peer's following EOF. The original Enter bytes and bound
remain retained, and no old boot request is pumped while waiting for root.

The local cleanup paths with settled startup accounts reach the actual terminal:

- Before semantic activation, retain the unused structural Open, retire its
  channel, seal the listener, record Unload through ordinary capacity, drain, and
  answer Left on the actual Leave exchange. No initial Grants reading stood, so
  the terminal reports Unreadable and omits SPU release.
- After completed classifier admission, use the accepted Leave to read current
  Grants, retain both actual readings, seal the listener, close and reap the
  classifier, then release the actual SPU and state before Unload. Left requires
  actual child reaps and the completed trace drain.
  A typed Grants refusal records Unreadable without becoming a component halt.
- An SPU child that was forked but never admitted receives its single Release;
  an exact NoResidency reply reports Unconfirmed. An admitted SPU reports
  Confirmed only for its actual matching Released after the complete Release
  send. The owner retains both the original Admit and Release exchanges,
  including any late Admit reply, without granting Open or new boot work.
  An unfinished Admit write is retained and closed, never resumed under Leave.

The unpublished checkpoint `95a6aacb` incorrectly placed the healthy classifier
reap after Unload. Harness Spec section 6 requires it before SPU or member
release. This order and the held-child circuit have been corrected locally.
The classifier is now held alive while the test proves that the member still
stands, no Unload exists and no Left is sent. The two old reap/order mutation
results below cannot establish conformance to this healthy release rule.

On an actual bad Release reply, the consuming failure handoff preserves the
first fault, original Leave, exact prior request/reply and Release owner. The
existing failed-run driver records the failure and terminal under its original
cap. It cannot acquire a second Release. A later failure after Recorded Unload
uses the shared no-record drain, retaining that terminal and appending nothing.
These exercised paths contain no gate. A caller cannot consume a progressing
healthy rollback through this failure interface.

A forked classifier still awaiting its admission is not a settled startup.
That initial account stays retained under its original Enter and accepted Leave;
it cannot start a new Grants read or enter the completed-admission release path.

The paired TRACE-STOP edit resolves its preactivation wording against the already
merged HARNESS-ENTER-WINDOW rollback rule. A recorded Load can be closed by actual
rollback without inventing a refusal or fault. This transition uses ordinary
capacity, retains the exact Unload under pressure, and spends no halt reservation.
It adds no event kind, field or timing rule. The original filed date stays fixed.

On the exercised healthy Accepted roads, the exact Leave cap disposes retained
startup work without a new
Unload or Left. A sent Open's incomplete account is retained before decode
retirement. Actual endpoints and child ownership survive as evidence; a kill
request is not called a reap. The owner samples the clock after inner work as
well as before it, so a drain that observes the cap cannot leave cleanup pending
behind a stale outer clock. A Recorded terminal sequence remains evidence even
when its drain expires.

## Verification at this local checkpoint

Evidence is under `/tmp/state-startup-rollback-20261025/`, with the source and
logs deposited at the approved #146 evidence root as
`state-expired-enter-local-20261025`. All Cargo
uses the pinned nightly, `--locked`, a default Rust test stack, four build jobs,
`CARGO_INCREMENTAL=0`, and private target
`/tmp/weaver-state-146-worker-session-target`. Cargo and Git mutations are serial.
The socket/process circuits run under disposable delegated root scopes; their
state member, classifier and model answers are declared doubles.

`classifier-checks/results.json` records passing lock gates, formatting, clippy
for both affected crates, the complete trace suite and the complete harness
suite with eight test threads and no filters. The harness library reports 1986
passed and 458 ignored; binary/integration targets add 45 passed and doctests 24.
The separate delegated startup run passes 101 scenarios, including 23 expired
startup cases. The trace targets pass 145 tests and 21 doctests. Ignored tests not
selected by that delegated run have not been qualified here.

The earlier default-thread harness run in `preflight-checks/` hit the existing
#331 pump fixture failure: CommitFailed at Sequence(8), OS error 11. Its isolated
rerun passed. A subsequent serial full-suite attempt was stopped after 225.98
seconds, with its partial log and exact process termination record preserved;
it is not a pass. Both subsequent complete eight-thread runs passed. No fix to
#331 is claimed.

The first blocked-drain circuit found a stale outer clock after an inner wait;
the correction is described above. The first classifier comparison run found
that the circuit fake could not model a wait without descriptors; its local
cleanup clock now advances the same fake timeline for that child wait. The
production clock and limits were unchanged. Initial failing logs are retained.

The preflight checkpoint detected 17 compiled protective changes. Its first
runner accidentally selected three similarly named tests for one control; the
intended assertion failed, but an additional blocked-sink case hung. The two
identified processes were stopped by pidfd, source bytes were verified restored,
and the final two controls passed with exact test selection. The log, timeout,
termination identities and rerun remain evidence. The updated source detected
all 22 compiled controls in `controls-classifier/results.json`, with positive
baselines first and byte-identical source restoration afterward. This runner
uses exact scenario names and adds five controls: current Grants must be read,
changed and refused readings must be distinguished, Left must wait for the
actual reap, and Unload must precede the child wait. The last two controls were
based on the incorrect healthy cleanup ordering described above; their logs
remain evidence of what was run, not evidence of conformance.
`classifier-restored-checks/results.json` records the subsequent clean rebuild,
trace suite, all 23 expired-startup circuits, formatting, locks and diff check.
Those complete suite/lint results belong to checkpoint `95a6aacb`, not to the
subsequent classifier ordering correction or SPU implementation.

Manual source review and invoked gates are author evidence. The required
code-review and verification-before-completion skills are not available in this
session, so neither skill invocation nor a Planner grade is claimed.

## SPU continuation evidence

The continuation adds actual Release/NoResidency, late and queued Admit,
wrong-exchange/position/arm and premature Release replies, silent Release at the
original Leave cap, actual SPU reap, failed release cleanup, Recorded-Unload
precedence, and unfinished classifier admission circuits. The current startup
run passes 114 cases, including 36 expired-startup cases. The model and classifier
remain declared protocol doubles; this is not GPU or production qualification.

The first expanded full startup run caught an incorrect fixture condition: its
new rollback Grants handling also matched the initial Grants read. The condition
now distinguishes the first read from the one actual rollback read, retaining
the prior request history. The failing log is preserved in
`spu-checks/startup-circuits.log`; the corrected run is
`spu-startup-all-2.log`.

The first SPU mutation run found that the seal test checked eventual unlink but
not its order. The held-classifier test now checks that the lifecycle pathname
has already been unlinked while that child remains alive and the member still
stands. The complete final control and gate results are recorded beside the
source snapshot. Older logs remain separate and do not substitute for the final
source's checks.

`spu-final-checks/results.json` records all ten final gates passing: first and
last lock gates, formatting, strict trace and harness clippy, the complete trace
and harness suites, the library build, all startup circuits, and diff check.
The harness library passes 1986 tests with 471 ignored; other targets pass 45
tests and 24 doctests. The trace suite passes 145 tests and 21 doctests. The
delegated startup run passes all 114 cases. Ignores outside that run remain
outside this qualification.

`controls-spu-final/results.json` records 15 compiled deliberate mutations,
each detected by its named exact test after all positive baselines passed.
They cover classifier admission and reap, the lifecycle seal, exact Release
send/exchange/position/answer, NoResidency, late and unsent Admit, the original
Leave, actual SPU reap, decode retirement, no Release retry, and the Recorded
terminal's precedence. Source restoration is verified before the final gates.

The checkpoint's archive, source hashes, controls and logs are preserved under
the approved evidence root at
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-expired-spu-local-20261025`.
Its manifest identifies every deposited file. This supersedes no earlier
deposit and publishes no PR.

## Open account continuation

The consuming expired-startup owner now disposes its original Open account before
the cleanup Grants read. A private permit binds the context, original expired
Enter, exact accepted live Leave and original decode port. It moves the request,
received raw answer, account bound and any pending placement event and ticket into
the rollback owner. This detached account has no continuation or journal grant.
Its evidence survives a later failed-exit handoff.

Before the first Open byte, cancellation uses the existing unsent withdrawal.
The same helper serves the earlier stopped-account path. Withdrawal touches no
wire and creates no missing-answer obligation. A sent Open whose account has
not completed by the original Enter bound is retained incomplete. Leave supplies
no new time to send, receive or record that boot account. The cleanup can still
compare actual Grants, seal, reap the classifier, release the SPU and member,
and record and drain the original load's Unload. An incomplete Open prevents
Left even after this cleanup finishes; the actual rollback peer is closed with
its directive retained. An unsent withdrawal can finish the ordinary Left path.

Four additional delegated circuits cover unanswered Open, unsent Open, pending
placement recording under actual writer pressure, and failed Release after Open
disposal. The pressure case compares the retained event and ticket and the raw
Opened response with their originals. The unsent child rejects every Open byte.
The old cap circuit now accepts the incomplete account in its new owner as well
as in the context before detachment.

The first expanded run found that old cap assertion and a fixture deadlock: a
single test thread could not drain the writer while close_after_drain waited.
Only its disposable systemd scope was terminated. The corrected pressure case
starts its trace-reading thread after all children have been forked and joins it
after the real terminal drain. Earlier failed logs remain in `open-checks/`.

The first full harness run passed 1985 library tests and failed the existing
`rollback_halt_composition_expired_leave` test: its immediate EOF assertion saw
EAGAIN. The isolated rerun passed. A concurrent test fork can retain a socket
until exec, so the fixture now allows a bounded two-second wait for EOF while
rejecting any answer immediately. This changes no runtime deadline or release
behavior. A compiled control that leaks the retired descriptor still fails this
assertion. `open-final-checks/harness-eight.log`, the isolated rerun and
`controls-open-eof/` retain the failure, correction and sensitivity evidence.

`open-restored-checks/results.json` records all eight final gates passing after
restoration: locks before and after, formatting, strict harness clippy, the full
harness suite, the library build, all startup circuits and diff check. The library
passes 1986 tests with 475 ignored; other targets add 45 tests and 24 doctests,
for 2055 ordinary passes. The separate delegated run passes 118 startup cases,
including 40 expired-startup cases. Other ignored families and the eight
privileged admin-startup cases are not qualified by this run.

`controls-open-final/results.json` records eleven compiled controls, each detected
after its exact positive baseline passed. They cover incomplete-account Left,
account disposal, pending event/ticket and raw answer retention, original account
bound, context/Enter/Leave/decode identity, custody through failed exit and the
forbidden first Open byte after cancellation. `controls-open-eof/results.json`
adds the descriptor-leak control. Restored source hashes are verified before the
final gates. The ten-control first pass remains separate in `controls-open/`.

Manual review covered both callers of the shared unsent withdrawal, the private
detach permit and its consuming startup caller, pending author custody, the
failed-exit owner and the terminal's Left election. No new wire field, event,
runtime limit or production selection is introduced. The required review skills
remain unavailable; these results are author evidence, not Planner or production
qualification. A fresh review/thread/comment audit of drafts #332 through #343
found no new findings and is retained in `open-checks/review-audit.json`.

This checkpoint's source archive and logs are preserved at the approved evidence
root under `state-expired-open-local-20261025`; its manifest identifies every
file. Earlier deposits remain separate. This is still unpublished work.

## Completion boundary

This branch remains local work in progress, with no published PR and no serving
selection. In particular, the handoff and cap disposal tests for later startup
positions do not establish cleanup of all pending accounts or gate ownership.
Those unsupported positions remain Accepted until the original Leave cap; this
owner must not be wired into a dispatcher as completed cleanup.

The same slice still owes unfinished template, boot-read, repair and summary
accounts and other startup ownership, gate Lower and tool containment,
before-Load and gate-owned stopped cleanup, and the worker/dev_loop adapter. Successful
Ready and pre-adoption expiry need their own lifecycle continuations. The full
#146 scope additionally includes healthy Leave/save, capture cleanup, process
exit and dispatcher selection, production artifacts and prompt qualification,
measured limits, security disposition and the operator's coupled cutover.
