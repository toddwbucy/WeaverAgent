# Diagnostic setup and rollback composition

**Version:** 0.1, 2026-10-30.
**Parent:** Issue 146, state management implementation.

Implements: weaver-harness-Spec section 6.1.1 DIAGNOSTIC-SETUP-ROLLBACK,
section 6.1's Diagnostic aggregate and section 6's terminal kill/reap rule,
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
peer's rollback. Observe answers InTransition before Ready and while rollback
is pending. Stop and JoinLeave refuse OutOfOrder during setup rollback.

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

Confirmed residency receives an attributed Release and must answer Released.
Children remain owned through reap. Failed cleanup kills and reaps the owned
children, preserves failure and permits no successful Left. Admin retains its
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

## Validation and source scope

The actual worker matrix has 23 cases. It holds identity, Admit, Open and
classifier readiness in turn, crossing same/new peers and original requester
EOF. It checks refused/lost Admit and wrong opener, ordinal, position and
payload. Other cases cover partial decode segments, failed Release, a child
held alive after Released, reply pressure and the original deadline.

The pressure case consumes 1,024 management requests without consuming their
replies. A stable partial reply queue across independent Observe cycles proves
the original peer cannot flush. Another peer receives rollback Left before
the original peer drains. All retained replies then arrive exactly once.
The deadline case spends 20 seconds in Admit settlement, then withholds Release
through the original 150-second interval. Renewing the interval fails the case.
An additional transport test fills the actual Admit socket to EAGAIN and pins
rollback before any Admit send for both wire formats. It does not claim a
worker child in that lower-level test.

The joined source passes 2,291 ordinary harness tests/doctests, 23 setup
process cases and seven existing Diagnostic admission/replay cases. Strict
all-target harness lint, lock, formatting, diff and three DocsMirror checks
pass. The ordinary suite reports 995 ignored cases and explicitly skips known
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

These eight controls compile independently and fail their process assertions.
A ninth, D7, removes MSG_DONTWAIT alone and survives: the retained POLLOUT check
still prevents a blocked send in this fixture. It is not counted as caught or
as proof of the flag alone. D7b removes the queued-delivery mechanism itself.
Both results remain in the evidence.
The original source restores byte-for-byte. The restored
transport test and all 23 setup process cases pass again.

The gate uses pinned nightly-2026-02-13, CARGO_INCREMENTAL=0, --locked and
the private target /var/tmp/weaver-state-diagnostic-setup-target. The normal
SQLite member is built in that target. Worker cases use root-delegated
transient scopes and the independently built admin test binary. The SPU,
classifier and preload responders are scripted test peers, not production
models or artifacts.

The source archive has 1,190 files before this record. The record is the only
addition after gates. Source checksums, command arrays, failed checks
and controls are retained with the deposit. Final archive/commit reconciliation
is stated in the PR body.

Evidence deposit:
```text
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/diagnostic-setup-20261030
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
cause, which was removed. Each failed run remains in the evidence.

Use the actual declaration and kernel preconditions in fixtures, and build
the normal member before a fresh target's integration suite. Keep failed and
restored checks tied to their exact source, and distinguish a readiness check
that remains protective from a removed blocking-wait mechanism.
