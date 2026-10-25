# Expired startup gate rollback composition

Date: 2026-10-25. Author working record for issue #146.

## Plan and authority

This act starts at draft #344, commit
`60ba1a8a708acacba552fb6341a82d75b78c607d`, on
`code/state-expired-gate-rollback`. It completes the expired startup owner's
gate cleanup before the existing Grants, classifier, SPU, member and terminal
sequence. The existing serving dispatcher remains selected. Foundation
qualification and coupled cutover remain separate requirements of #146.

Authority is the merged harness Spec section 2.2 stderr account and section 6
rollback and Lower rules,
harness-gate contract sections 2-3, admin Spec section 6.1's worker containment
rows, and HARNESS-ENTER-WINDOW, HARNESS-CLOCK, HARNESS-RECORD-LIFETIME,
HARNESS-HALT and HARNESS-HALT-BOUND. The summary boundary instruments also
exercise HARNESS-PLAN and HARNESS-READ.

| Boundary | Required evidence |
| --- | --- |
| Expired Enter | Retain the original Raise and actual send/reply witnesses. Never flush an unsent Raise or grant another boot step. |
| Actual rollback | Only the accepted root Leave supplies healthy cleanup authority. Its original start anchors one Lower window, capped by that Leave. |
| Lower | A fully sent original Raise permits one Lower even when GateReady is retained late. Confirmation requires the actual port, exchange and timely whole send. |
| Queued frame | Retain the received datagram while its exact Leave/Unloading refusal waits for Recorded. Do not create a turn or answer its closed world connection. |
| Lower expiry | Close transport and attempt the owned-child kill before resuming owed cleanup records under unchanged Leave. No GateBound halt or new Lower window follows from this lifecycle timeout. |
| Containment | One unwind kill or pinned fallback pass, with its actual result retained. Inside Leave, no tools-empty wait or repeated sweep. |
| Gate exit | GateStopped and kill delivery are not child exit. Reap the actual child before current Grants and subsequent releases. |
| Refused Lower | Finish cleanup and terminal drain, then carry the original wrapped refusal on the actual Leave exchange. Do not report Left instead. |
| Observed death | Retain the last parseable JSON stderr line. Freeze it at EOF or the existing 200 ms settlement limit, capped by Leave; Lower still disposes transport at its original expiry. |
| Real failure | Preserve the first cause, pending records, original receive evidence and earlier Leave/halt cap. Attempt the first fault before failed cleanup; never duplicate Lower or a completed sweep. |
| Terminal | After Recorded Unload, only terminal drain/disposal remains. No later event, second Unload or successful response can be manufactured. |

## Verification scope

The new composition cases use actual startup owners, Unix sockets, process
children and trace receipts. Gate cleanup covers normal Lower, queued frames,
silent and retained children, early and wrong replies, typed refusal, a sent
Raise without a retained Ready, concurrent silent management, Leave expiry,
trace pressure and real transport loss. The actual child must be reaped before
release can proceed.

The unsent-Raise case reaches the production Raise stage after the actual fork.
It expires Enter after staging and before sending, then preserves the original
queued packet's allocation, bytes, offset and send witness through disposal.
The separate installed-boundary case expires before the gate fork.

Failed cleanup cases cover failure before Leave, an unaccepted frame, an
accepted pending trace ticket, a stopped reply with a live child, the original
cap, release and Recorded Unload. The first fault-report attempt precedes gate
cleanup. Accepted trace work may settle under the halt owner, while no new
ordinary refusal or death record is admitted after halt.

Five actual-process stderr cases cover delayed JSON followed by garble, an
open pipe through timeout, Lower expiry, Leave expiry and failure during
settlement. Four ordinary tests pin nonconsuming typed snapshots, absence of
JSON, a held reader mutex and poisoned-mutex evidence. The snapshots use
`try_lock`; the existing blocking reader API keeps its prior 200 ms allowance.

Four summary cases expire after Recorded while the read is incomplete, after
read completion with StateControl retired, after read settlement before cache
publication, and after an actual typed read refusal. They retain actual read
and trace evidence through rollback.

All eight final checks passed: first/last lock gates, formatting, strict
harness clippy for all targets, the full harness Cargo suite, a library build,
all delegated startup cases and the diff check. The harness suite passed 2,060
tests/doctests: 1,991 library, 43 binary/integration and 26 doctests, with 564
library cases ignored in that ordinary invocation. The delegated startup run
passed 207 cases, including all 30 added here. Other ignored families and the
privileged admin-startup cases are not qualified by this run.

All 16 compiled protective mutations were detected after 13 positive
baselines. The six controlled source files were restored byte-for-byte before
the final gates. These controls remove gate cleanup, require a retained Ready,
restart Lower, skip reap, lose a pending frame or death record, drop a refusal,
skip deadline disposal, turn Lower into a fatal Gate halt, release before failed
gate exit, send Lower for an unsent Raise, flush an expired Raise, omit the typed
word, skip or renew stderr settlement, or block on the reader mutex.

The runs used the pinned nightly, locked dependencies, the default Rust test
stack and private target `/tmp/weaver-state-146-worker-session-target`.
Cargo and Git mutations were serial. These are harness/default-feature checks,
not a workspace/device-suite claim. All 718 final crate/configuration files
were hashed and matched before publication.

The evidence deposit is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-expired-gate-rollback-local-20261025/`.
Its `checkpoint.json`, `source-sha256.json` and `manifest.json` bind the committed
source archive, exact head, logs and control patches. Development failures and
excluded zero-test selectors remain beside the successful final checks.
The review audit at 2026-10-25 15:44:16 UTC found no new findings or changed
heads on drafts #332-#344. No external review pass is claimed for this act.

## Development findings and after-action review

The planned slice was gate cleanup after expired startup plus the outstanding
summary clock-boundary instruments. The first installed and raised reproductions
failed because the parent excluded every gate-bearing handoff. The completed
owner now retains its gate account through healthy rollback, deadline disposal
and a later component failure.

Reading the organ-death rule found that the observed death also owed the dying
organ's last typed stderr line. The act therefore carries bounded, nonblocking
EOF settlement and tests its transfer on failure. This uses the existing 200 ms
allowance; it introduces no new Spec rule or timing field.

The first pressure assertion compared the unassigned pre-admission sequence
with the actual Recorded sequence. The corrected instrument reads the retained
receipt and compares all other event fields unchanged. An early direct-binary
ordinary-suite run omitted Cargo's package-name environment; the affected
fixture passed when invoked with it. Final ordinary gates run through Cargo.
Five abbreviated stderr selectors selected zero tests; these are explicitly
excluded from evidence, and the corrected exact selector run passed all five.
The final mutation runner rejects zero-test baselines and detections. Keep
that selector guard in future composition evidence.

## Completion boundary

This act advances #146's expired-startup rollback composition. It closes no
broad epic checkbox. Ready rollback, healthy Leave/save, capture cleanup, the
worker adapter, dispatcher/process exit, production artifacts and measured
limits, security disposition and qualified coupled cutover remain owed. The
existing serving dispatcher remains selected.

The checks are author evidence, not a Planner grade or production qualification.
The required `code-review` and `verification-before-completion` skills are
unavailable in this session; manual checks and independent reads are disclosed
as such. The one-shot tools-sweep guard was reviewed, but no syscall-count
instrument proves a repeated completed sweep is absent. Operator-only merges
and deployment and draft publication remain in force. No CodeRabbit review is
requested by this act.
