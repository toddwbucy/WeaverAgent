# Diagnostic setup and rollback composition

**Version:** 0.4, 2026-10-30.
**Parent:** Issue 146, state management implementation.

Implements: weaver-harness-Spec section 6.1.1 DIAGNOSTIC-SETUP-ROLLBACK,
section 6.1's Diagnostic aggregate and section 6's silent-dial and kill/reap rules,
with weaver-admin-harness-contract section 4's pending rollback behavior.
The channel and state changes preserve the existing organ, decode, classify
and harness-state contracts.

This slice is on code/state-diagnostic-setup-rollback, based on #409 at
81e3ee5d98d21272c69fb7ae59b8a8546d47f5c4. Merge d26c2f75 also retains main's
ratified #408 at cdcc391e. Both parents remain ancestors. The merged #398
setup rule is implementation authority. It advances #146's Diagnostic
composition work and closes no broad completion item. Existing serving remains
selected. Do not select the foundation dispatcher or deploy from this slice.

## Behavior and ownership

Diagnostic setup retains management service while identity, Admit, Open or
elected classifier readiness is pending. Each peer owns its exact encoded
replies. A silent peer or a blocked reply does not hold the listener or another
peer's rollback. Every new accepted connection has the existing
two-second first-directive bound, including before rollback. Expiry closes
only that silent connection, without changing the rollback deadline. Observe
answers InTransition before Ready and while rollback is pending. Stop and JoinLeave refuse OutOfOrder
during setup rollback.

A forced rollback accepted before Ready selects NoResidency for the original
Enter exchange. An established refusal or Ready keeps its original outcome.
Requester loss retires delivery without cancelling the resource account.
No later setup work, child fork or replay grant follows accepted rollback.
Ready selection is the commitment boundary, before its delivery.

The one rollback deadline starts at acceptance. Admit settlement, channel
closure, Release, reap, record drain and reply delivery consume that interval.
A fully sent Admit drains its own attributed terminal. Only its matching
opener, ordinal, Close position and Admitted payload permit Release. A refused,
lost or unsent Admit closes without a Release claim. Pending Open ends by
closing its channel, without a turn-scoped Cancel or replacement Open.

Before Ready, confirmed Admit receives an attributed Release and must answer
Released. Children remain owned through reap. Failed setup cleanup kills and
reaps the owned children, preserves failure and permits no successful Left.
After Ready selection, cleanup uses the existing entered-run leave_after road.
A lost or invalid Release is Unconfirmed there, and the child is still closed
and reaped before Left. Ready keeps that meaning while its delivery is pending.
Admin retains its
existing escalation authority. Setup authors no diagnostic event, including
on refusal or failure. ReplayOpened remains the beginning of that record.

| Surface | Owner and continuation |
| --- | --- |
| Canonical Diagnostic admission | Projects the checked payload and reaches the same dispatch_on setup owner as legacy entry. |
| enter_with_setup | Shared Run constructor, with checkpoints before later setup work. The existing serving caller keeps its ordinary waits. |
| StateSeam | One owner retains partial identity bytes, parked deadline and existing line parser while the callback services management. |
| OrganChannel | The selected wire codec encodes once. Pending sends retain the complete packet. |
| DecodeChannel | The existing serialization, size limits and segment reassembly serve ordinary and managed waits. |
| ClassifyChannel | The elected readiness wait retains its original bound and the child for cleanup. |
| Setup peers | Exact Enter and rollback identities, first selected outcome and per-peer reply queues survive pressure and EOF. |
| Admission | Unsent, awaiting, admitted, refused and lost distinguish release authority. |
| Cleanup | The same rollback deadline covers actual channels, child release, reap, record drain and delivery. |

No wire field, trace kind, operator verb or new timeout is added. Canonical and
legacy entry use the common owner, with no fallback to Serving. The entered
replay interruption owner remains separate work under #393.

## Planner corrections and coverage

Review 5707696700 on #410 requires FX1 and FX2. FX1 now routes a Leave heard
after Ready selection through the entered-run cleanup. A newly accepted peer
also retains its first-request duty, so delivering Ready in that same poll
cannot drop the peer before receiving its queued request. FX2 is pinned at the
checkpoint itself with a queued rollback under both management wire formats.
The test retains the original exchange, cause and deadline across checkpoints.

| Site | Correction or retained responsibility |
| --- | --- |
| enter_outcome | The first selected Ready/refusal remains fixed before delivery. |
| poll accept and hear | A newly accepted peer owes a first receive. Hearing or retiring it discharges that duty. |
| replies_done and await_disposition | No handoff drops an accepted first request. Exact queued replies remain owned. |
| cleanup before Ready | Exact Release, original rollback deadline and failure custody remain unchanged. |
| cleanup after Ready | Calls leave_after with the original cause, forced mode and deadline. Unconfirmed Release follows the entered rule. |
| canonical and legacy entry | Both reach enter_diagnostic_managed and the same corrected owner. |
| checkpoint callers in enter_with_setup | The common checkpoint rejects further setup after its own poll accepts rollback. |

Two worker cases fill the reply socket before classifier readiness. A separate
Observe confirms Ready selection while its original reply remains undelivered.
Rollback then completes Left after an invalid Release ordinal or Release EOF,
with actual child reap, no replay and the original Ready delivered exactly once.
A root transport case dials the real listener with rollback already queued.
The first poll accepts that peer and delivers Ready, but cannot yet read the
new descriptor. The next poll must retain and hear that exact rollback.

B5's three gaps now have tests: forced Leave without rollback before Ready,
JoinLeave before Ready during rollback, and a refused Release. The first two
exercise both wire formats. The Release refusal uses the actual worker and
remains fatal before Ready. The JoinLeave control removes the combined Ready
and rollback condition. It does not independently pin the rollback check after
Ready. That narrower guard is covered by the B7 correction below.

Seven independent compiled controls are caught: restoring setup cleanup after
Ready, ignoring accepted rollback at checkpoint, dropping the handoff guard,
accepting force without rollback, accepting JoinLeave before Ready,
accepting a refused Release, and omitting first-request custody at real accept.
The first six are on one restored source cohort. The seventh adds only the
root listener test, with identical production source and earlier test bodies.
The restored owner tests, new listener case and strict lint pass afterward.

The full correction gates pass 2,294 ordinary harness tests/doctests, 27 setup
worker cases and seven existing Diagnostic cases. The ordinary suite reports
999 ignored and skips known #331. Its total is 2,210 library, eight worker,
43 integration and 33 doctests, excluding nested child summaries. The later
root listener test adds one ignored case, run explicitly under root. Full
process suites are not claimed rerun after that test-only addition. All
production files match across these cohorts. Lock, formatting, strict lint,
diff and three DocsMirror checks pass. The private target is unchanged, with
CARGO_INCREMENTAL=0, CARGO_PROFILE_DEV_DEBUG=0, CARGO_BUILD_JOBS=8 and private
TMPDIR /var/tmp/weaver-state-diagnostic-setup-tmp.

The first control run caught four mutations, then its JoinLeave test waited
for a refusal that the mutation withheld. That fixture hit its outer watchdog.
It is preserved, not counted as a caught assertion. The corrected fixture
uses a bounded descriptor wait, and all six controls then fail their intended
assertions. All source hashes restore after each mutation.

R1 and R2 are filed as #412 under #146: an unread root bystander can still hold
the queue-wide Ready handoff, or hold terminal reply drainage after another
peer receives Left. This correction does not close them. The entered replay
composition above #411 owns their followup. B1's non-rollback checkpoint fault
label, B2's explicit Admit-loss wording and B4's shared-owner simplification
remain carried by #146. B3 and B6 retain the Planner's dispositions: classifier
exit-on-close may spend the original deadline, and the terminal Drop path still
kills before reap. No extra timing protection is claimed for those survivors.

The correction evidence is deposited separately under the approved root in
diagnostic-setup-review-20261030. It includes the failed fixture run, both
control cohorts, final gates, listener proof, exact source hashes and published
archive. This author check does not replace the Planner's regrade.

## Silent-peer correction and review evidence

Planner regrade 5708184494 at 3c545b50 accepts FX1 and FX2 and identifies FX3.
The first-request custody guard exposed an unbounded silent peer accepted
before rollback. The accept site now arms the existing ANSWER_BOUND_MS for
every new peer. The original Enter peer has already sent its request and
retains no first-request deadline. No operation interval changes.

The real-worker case keeps the pre-rollback silent peer connected after Left.
The worker must retire it and exit successfully inside the two-second receive
interval plus fixture margin. A separate root listener case holds that peer
across Ready delivery and proves it cannot hold the handoff. Both fail against
the old runtime and pass after the correction. Restoring the leaving-only
condition is independently compiled and catches both assertions.

B7 now has a post-Ready rollback JoinLeave case under both management wire
formats. It requires OutOfOrder and retains the first rollback's exchange,
cause, deadline and empty join set. A separate compiled control removes only
the rollback check while keeping the Ready guard. It fails the bounded
refusal assertion. B8 is addressed by narrowing the earlier control claim
above. These two controls are separate from the prior seven-control cohort.

| Site | FX3 review |
| --- | --- |
| Original Enter peer | Its request was received before Setup, so no first-request wait is owed. |
| poll accept | Every new peer starts the same existing two-second first-request interval. |
| poll wake and expiry | The earliest live peer deadline wakes the owner and retires only that peer. |
| hear | Receiving a first request clears its interval and pending duty. |
| retire and replies_done | A retired peer cannot hold reply drainage or the Ready handoff. |
| Ready/Leave outcomes | The selected outcome, accepted resource duties and original operation interval remain fixed. |

Final author gates pass 2,295 ordinary harness tests/doctests: 2,211 library,
eight worker, 43 integration and 33 doctests. The suite reports 1,002 ignored
and skips known #331. All 28 setup worker cases and seven existing Diagnostic
cases pass. Seven owner tests, including both ignored root listener cases,
pass in the explicit control lane. Strict harness lint, lock, format, diff
and three DocsMirror checks pass. Source hashes restore after both controls.
The private target is unchanged. Final full gates use /var/tmp/wd146s as TMPDIR.

The first full run failed the unchanged classifier-child EOF test with phase
Entered instead of Failed. Three isolated reruns and the repeated full suite
pass on identical source. That first failure remains in the evidence and is
not diagnosed or claimed fixed by this correction. A first fixture compile
also failed because it reached a private test module. The corrected listener
case checks its already-retired socket directly. Neither failed attempt is
counted as a pass.

R1/R2 of #412 remain open. This correction removes their silent-first-request
trigger. A peer that sends requests but does not read its replies is still the
entered replay consumer's work. The consumer has separate unpublished evidence
for Ready independence and the full 150-second completed-Left delivery case.
Those results do not close #412 from this parent PR.

The immutable evidence is under the approved root in diagnostic-setup-fx3-20261030,
including both red-test attempts, controls, both full-gate attempts, source
hashes and the published archive. Only this record changes after final code
gates. These are author results awaiting Planner regrade, not a security
review or production qualification.

## Validation and source scope

The 5e6d1b66 worker matrix has 24 cases. It holds identity, Admit, Open and
classifier readiness in turn, crossing same/new peers and original requester
EOF. It checks refused/lost Admit and wrong opener, ordinal, position and
payload. Other cases cover partial decode segments, failed Release, a child
held alive after Released, reply pressure, silent-peer expiry during Leave
and the original deadline.

The pressure case consumes 1,024 management requests without consuming their
replies. A stable partial reply queue across independent Observe cycles proves
the original peer cannot flush. Another peer receives rollback Left before
the original peer drains. All retained replies then arrive exactly once.
The deadline case spends 20 seconds in Admit settlement, then withholds Release
through the original 150-second interval. Renewing the interval fails the case.
An additional transport test fills the actual Admit socket to EAGAIN and pins
rollback before any Admit send for both wire formats. It does not claim a
worker child in that lower-level test.

The 5e6d1b66 source passes 2,291 ordinary harness tests/doctests, 24 setup
process cases and seven existing Diagnostic admission/replay cases. Strict
all-target harness lint, lock, formatting, diff and three DocsMirror checks
pass. The ordinary suite reports 996 ignored cases and explicitly skips known
#331's a_real_pending_trace_author_does_not_reduce_or_replace_packet_completions.
Counts exclude nested child summaries. The total is 2,207 library tests,
8 worker tests, 43 integration tests and 33 doctests.

| Compiled control | Protective failure |
| --- | --- |
| D1, do not receive management while waiting | Partial-identity Observe reaches the actual worker watchdog. |
| D2, abandon a fully sent Admit on rollback | Same-peer Admit case detects premature completion. |
| D3, accept a wrong Admit ordinal | Exact Admit attribution case fails. |
| D4, Release after refused Admit | The child rejects Release and the worker cannot complete the asserted rollback. |
| D5, accept a wrong Release ordinal | Failure case receives forbidden successful Left. |
| D6, skip reap before completion | Held-child case detects premature worker completion. |
| D7b, replace queued replies with a blocking send | The worker stops receiving requests under reply pressure. |
| D8, renew the deadline on entry to cleanup | The actual interval case fails at its original deadline watchdog. |
| D9, omit a silent Leave peer's receive timer | The actual worker fails to close the new silent peer within the fixture bound. |

The first eight controls compile independently and fail their process assertions
at 59034ba9. D7 removes MSG_DONTWAIT alone and survives: the retained POLLOUT
check still prevents a blocked send in this fixture. It is not counted as
caught or as proof of the flag alone. D7b removes the queued-delivery mechanism
itself. Both results remain in the evidence. The first source restores
byte-for-byte and its transport test and 23 setup cases pass again.

The final contract sweep found the inherited two-second first-directive bound
for new silent peers accepted during Leave. The correction retains that
per-peer deadline, includes it in poll's earliest wake, clears it on the first
request and excludes retired peers. Expiry drops only that peer. The original
Enter peer has no new deadline. Both peer constructors and the request,
retirement and wait paths are included in the caller census.

D9 removes the new peer's timer and fails the actual worker's four-second EOF
watchdog. D8 is compiled and caught again against the corrected source. The
restored transport test and all 24 setup cases pass. Earlier controls remain
scoped to 59034ba9, not claimed rerun against this correction. All 1,191 tested
file hashes restore, and only this record changes after the final gates.

The gate uses pinned nightly-2026-02-13, CARGO_INCREMENTAL=0, --locked and
the private target /var/tmp/weaver-state-diagnostic-setup-target. The normal
SQLite member is built in that target. Worker cases use root-delegated
transient scopes and the independently built admin test binary. The SPU,
classifier and preload responders are scripted test peers, not production
models or artifacts.

The first archive has 1,191 committed files at 59034ba9, with this record added
after its 1,190-file gate snapshot. The correction tests all 1,191 files and
changes only this record afterward. Source checksums, command arrays, failed
checks and controls are retained with each separate deposit. Final
archive/commit reconciliation is stated in the PR body.

First evidence deposit, scoped to 59034ba9:
```text
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/diagnostic-setup-20261030
```

Final correction evidence:
```text
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/diagnostic-setup-silent-peer-20261030
```

## Review and limits

The manual self-review follows setup wait ownership, first outcome, every
release authority, deadline origin and both entry consumers. The code-review
and verification-before-completion skills are unavailable in this session.
Manual review and executed gates are not reported as those skill invocations.
Planner and subsequent reviews remain required. No CodeRabbit review was
requested, and the PR remains draft for review.

No production artifact, model, prompt/template certificate or measured limit
is qualified by these fixtures. Replay interruption, plan closure, remaining
worker composition and coupled cutover remain tracked by #146. No broad
completion or deployment follows from this slice.

## After-action

The initial worker regression reproduced blocked Observe during a partial
identity answer against the unmodified runtime. The implementation shares
the existing codecs and Run construction while adding management to their
waits. Actual process cases then exercise each setup arm and cleanup.

The first expanded fixture assumed classifier election in a declaration that
carried none. The fixture now elects it explicitly on the real Enter wire.
The first broad run exposed a classifier test assuming immediate EOF after
closing its peer. Both that test and its adapter matched the parent source.
The correction uses its existing bounded terminal precondition before the
original single-call failure assertion, covering concurrent fork inheritance.
A later run lacked the normal member binary in the new private target. The
runner now builds that prerequisite. Strict lint also caught a clone of a Copy
cause, which was removed. The silent-peer correction first named the answer
bound in the floor crate rather than its existing state-seam definition. That
compile failure and its corrected rerun remain in the separate deposit. Each
failed run remains in the evidence.

Use the actual declaration and kernel preconditions in fixtures, and build
the normal member before a fresh target's integration suite. Keep failed and
restored checks tied to their exact source, and distinguish a readiness check
that remains protective from a removed blocking-wait mechanism.

The contract sweep must include shared lifecycle wait rules as well as the
new setup section. A peer's receive deadline is a separate retirement duty
from the operation deadline, and the tests must check both together.

Test a silent peer accepted before a phase transition, not only one accepted
inside the phase. Preserve each earlier control's exact guard scope when
reporting a review result, and isolate the disputed guard in the next control.
