# Enter response and accepted rollback composition

Date: 2026-10-23. Branch: code/state-rollback-composition.
Parent: #290, 86f21c432e3e4704bf09fe9e4ab421ad764d5efe.
Advances Issue #146 lifecycle composition. No full checklist item is closed.
Operator alone merges and deploys; existing serving stays active.

## Connected path

The actual Recorded startup refusal now stages its correlated response on an
owned LifecyclePeer. It retains the exact tracked send under socket pressure and
requires a completely sent packet observed under the original Enter window.
Busy retains its returned original without pumping another queued response.
The common pump's Admin arm now supplies its clock, as its gate and residency
arms already do. A whole send followed by a clock-observation failure remains
Sent without an invented timestamp, latches TraceFailed and cannot be resent or
become a healthy continuation.

The next owner consumes an actual peer-held Admin/Open rollback Leave, retaining
its envelope, cause, original response transport, exact author/send evidence and
same startup context/resource bag. Invalid directives and ancillary handles are
returned intact. Only the original Enter or its actual retired receipt can
transition to the caller's unchanged Leave allowance. Actual rollback may arrive
after Enter expiry; no other deadline or clock failure grants that exception.
After this handoff, the old Enter response is never pumped, including a real
queued unsent packet whose socket later becomes writable.

The rollback uses existing ports to retire the refused classifier, observe its
actual child without waiting, obtain a measured and Recorded current Grants
answer, and enter Releasing. A refused Grants answer stays retained in Entered;
it creates no unchanged-surface claim or successful release. Prepared is a
release-phase handoff, not an Unload/Left or completed process cleanup.

Authority: base harness Spec section 6 refusal and rollback, admin Spec section 3
S1 x Leave/K1, admin-harness contract section 3, Spec 02 DECLARATION, and merged
HARNESS-RECORD-LIFETIME/CLOCK/CLASSIFY. The authority check corrected a stale
source-map suggestion: rollback=true selects forced rollback even when the wire's
forced bit is false. The actual directive stays unchanged and the owner exposes
the Spec-required forced outcome separately, matching the existing lifecycle.
No new Spec, wire, event, timeout or production policy is elected.

## Author verification

Fifteen joined tests pass. They use actual startup and a forked Rust classifier
protocol double, real response SEQPACKET backpressure, the actual lifecycle receive
parser and an owned ancillary descriptor, plus scripted state answers and paired
fake time/readiness. Coverage includes complete and exactly-once response, Busy,
expiry before stage, post-send clock failure, pending-response rollback, expired
and legitimately retired Enter, substituted Enter, invalid Leave corpus, retained
clock failure/cause, original Leave expiry, typed Grants refusal, actual current
Grants before release and rollback=true/forced=false.

All 1,859 harness tests pass with 18 inherited ignored cases; all 160 trace tests
pass. Eleven nested child summaries are excluded from the top-level harness count.
All-target harness Clippy, format, lock and diff gates pass. The private target is
/tmp/weaver-146-boot-composition-target for this same isolated checkout.
Sixteen compiled protective mutations fail their named tests; every restored run
passes and all 239 harness Rust source hashes match the tested snapshot.

Initial logs retain the fixture's ChildEnd adoption compile error, its no-resend
probe initially rejecting the failed sender's expected EOF, and the large stopped
inspection lint correction. The actual inspection stays inline intentionally to
avoid allocation on its failure path. Final full gates follow all corrections and
the authority check. These are author checks, not independent, Planner, security
or production-qualification grades. The prescribed review/verification skills
remained unavailable; direct source/sequence review and executable gates were used.

## Completion boundary

The outer caller still owes judged Enter provenance, original peer selection and
listener orchestration; the fixture supplies an actual parsed Leave without
claiming that complete outer loop. The original Leave allowance is caller-owned,
not a new EnterWindow implementation. Child observations do not mean all children
reaped. Initial/current Grants comparison, bounded LastWord, full healthy/Failed
teardown, member retirement, terminal Unload and drained Left remain required.
The failed cleanup handoff must retain this actual rollback cause and the original
minimum halt/Leave cap. Generic Failed cleanup is not made to accept it by a bool.

No-SPU Unload still waits on approved but unmerged #285; neither a nonexistent SPU
nor its release outcome is fabricated. #276 and #282 also remain unmerged authority
holds. Other aggregate refusal producers, success gate/Ready, worker/dev_loop
migration, qualified artifacts/prompt/limits, real model/SQL/GPU qualification and
coupled serving cutover remain owed. Composition remains the priority; standalone
followup batches remain preserved and deferred unless needed by these joins.
