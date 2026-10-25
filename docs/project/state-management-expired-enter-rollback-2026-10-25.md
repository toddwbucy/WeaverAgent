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

Two healthy cleanup paths reach the actual terminal:

- Before semantic activation, retain the unused structural Open, retire its
  channel, seal the listener, record Unload through ordinary capacity, drain, and
  answer Left on the actual Leave exchange. No initial Grants reading stood, so
  the terminal reports Unreadable and omits SPU release.
- After completed classifier admission and before SPU creation, use the accepted
  Leave to read current Grants, retain both actual readings, close the classifier
  and state, and use the same terminal. The trace is attempted before waiting for
  classifier exit. Left requires its actual reap and the completed trace drain.
  A typed Grants refusal records Unreadable without becoming a component halt.

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
actual reap, and Unload must precede the child wait.
`classifier-restored-checks/results.json` records the subsequent clean rebuild,
trace suite, all 23 expired-startup circuits, formatting, locks and diff check.
The complete suite/lint source is byte-identical to this restored code; only this
working record was expanded after those gates.

Manual source review and invoked gates are author evidence. The required
code-review and verification-before-completion skills are not available in this
session, so neither skill invocation nor a Planner grade is claimed.

## Completion boundary

This branch remains local work in progress, with no published PR and no serving
selection. In particular, the handoff and cap disposal tests for later startup
positions are not a normal SPU/gate rollback implementation. Before its cap those
positions currently remain Accepted; they must not be wired into a dispatcher as
completed cleanup.

The same slice still owes normal cleanup after SPU creation, retained Open/read/
repair/summary accounting, gate Lower and tool containment, actual SPU Release
correlation, stopped rollback recovery (including failure after a terminal
receipt), and the worker/dev_loop adapter. Successful
Ready and pre-adoption expiry need their own lifecycle continuations. The full
#146 scope additionally includes healthy Leave/save, capture cleanup, process
exit and dispatcher selection, production artifacts and prompt qualification,
measured limits, security disposition and the operator's coupled cutover.
