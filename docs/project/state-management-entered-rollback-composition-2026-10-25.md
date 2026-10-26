# Successful Ready rollback composition

Date: 2026-10-25. Author working record for issue #146.

## Plan and authority

This act starts at draft #345, commit
`0a7bc2ecf70c766d6370c4a31c18b9b4ff1843c4`, on
`code/state-entered-rollback-composition`. It joins an actual successful
worker Ready and a later root rollback directive to the original run's cleanup.
The selected serving dispatcher remains `dev_loop::drive`.

Authority is the merged harness Spec section 6 rollback, Lower, release, drain
and reap rules, harness-gate contract sections 2-3, HARNESS-ENTER-WINDOW,
HARNESS-CLOCK, HARNESS-HELD-BOUNDARY, HARNESS-RECORD-LIFETIME, HARNESS-HALT
and HARNESS-HALT-BOUND. The successful Ready retires Enter. Root may observe
that actual Ready after its Enter window and direct rollback while the worker
is Entered; expiry is not a prerequisite or a replacement authority.

| Boundary | Required behavior and evidence |
| --- | --- |
| Admission | Consume only actual DirectivePending rollback Leave and pristine Waiting input. Return the complete owner for a different directive, held input, or accepted input. Do not arm Leave on held-only refusal. |
| Handoff | Preserve the original Ready, Raise, Admit, source ports, retired Enter, root directive, gate process/tools and listener. End serving loans before joining only their original run. Rejected joins return both original boxes. |
| Cleanup bound | Sample one actual Leave window at consuming admission. Lower uses that same start and cannot renew Leave. |
| Gate | Reuse the same Lower/refusal/containment/reap account as expired rollback. Received socket frames owe typed Leave/Unloading records before healthy completion. |
| Release | Measure current Grants; seal management acceptance; enter healthy Releasing; retire and actually reap classifier; use the original SPU exchange and actual release outcome; retire state. |
| Terminal | One forced Unload carries the load cause, then trace drains and every actual child is reaped before the tracked Left or retained Lower refusal. |
| Failure | Retain first fatal cause, gate and residency accounts; attempt fault before failed gate work; keep min(original Leave, halt). Recorded Unload permits disposal only. |

The handoff explicitly requires empty serving FIFO and exchange history. Only
engine work populates that FIFO, while this slice admits no accepted input or
started turn. Actual pre-intake socket frames are handled by the common gate
account. This does not claim active-turn or queued serving-backlog rollback.

## Verification scope

Two baseline cases failed at the missing rollback cleanup on the parent:
actual timely Ready and actual Ready observed after its five-second Enter
window. Successful cleanup cases invoke the actual worker adapter. Refusal and edge
cases use the same real admission and handoff, injecting the named clock fault
or cap into the retained owner. All use actual Unix sockets, SCM_RIGHTS
admission, child processes, member requests and trace receipts. The organs
and member are scripted and do not qualify a real model.

Twelve new delegated cases cover timely and late Ready, two queued socket
frames, the three held-author slots and successful same-owner retry, foreign
actual admission, a nonrollback directive, already accepted input, Leave expiry
before Unload, expiry inside Unload admission, first clock failure before
Lower, clock failure after Recorded Unload, and clock failure after drain.
The failed branch retains first TraceFailed and its actual halt/Leave cap;
its first fault attempt precedes any Lower. Terminal-only cases permit no Left
or new author after the retained Unload receipt.

The original load cause is uid 73 and the rollback caller cause is uid 74, so
the recorded cause comparison distinguishes the two. Successful cleanup
requires actual zero-exit reaps, confirmed SPU release, closed member, one
Unload, the same Enter/Leave identities, and one exact Left without a turn or
save. Refused admission retains original frame allocation, acceptance token,
management owner and unconsumed directive across retries.

All nine final checks passed: first/last lock gates, formatting, strict
harness clippy for all targets, the full harness Cargo suite, library build,
all delegated startup and session cases, and diff check. The ordinary suite
passed 2,061 tests/doctests: 1,991 library, 43 binary/integration and 27
doctests, with 576 library cases ignored in that invocation. It ran with
RUST_TEST_THREADS=4. The delegated runs passed all 207 startup cases and all
28 session cases, including the twelve new cases. Other ignored families,
privileged admin startup, the full workspace/device suite and production load
are not qualified by these runs.

All eight compiled protective mutations were detected after seven positive
baselines. They bypass foreign-run, held-input or rollback-directive checks;
substitute the Leave cause for the Load cause; turn Unload-admission expiry into
a fatal error; skip the first fault attempt; admit failed cleanup after actual
Recorded Unload; or skip trace drain before Left. All three controlled source
files were restored byte-for-byte and rebuilt before the final gates.

The runs used pinned nightly-2026-02-13, locked dependencies, default harness
features and private target /tmp/weaver-state-146-worker-session-target.
Cargo and Git mutations were serial. All 723 crate/configuration files were
hashed and matched after the final checks.

The evidence deposit is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-entered-rollback-composition-local-20261025/`.
The source archive, checkpoint, hashes, logs and mutation patches identify the
exact committed source and tested files. Development failures remain included.

## Development findings and after-action review

The plan was to join successful Ready to actual rollback without fabricating
an expired or failed startup. The implementation shares existing gate and SPU
release mechanisms through explicit borrowed gate evidence and a distinct
Ready-owning release permit. Rejected release admission returns that exact
permit and its received reply; no Ready send witness is cloned.

The terminal helper retains actual Recorded Unload even when a later observer
fails. Expiry inside Unload admission maps to the original Leave timeout rather
than a new ProtocolFailed halt. The matching boundary test detects removal of
that classification. An independent read also established that a nonempty
serving FIFO cannot arise on the admitted pristine route; admission now states
that restriction explicitly instead of suggesting general active-turn cleanup.

Initial edge fixtures mixed a frozen clock with live member timestamps and
correctly triggered measurement-order refusal. They now use the live clock
through actual cleanup, injecting only the named failure or exact deadline at
the boundary. Keep each instrument's time source consistent with its peers.
Strict lint also removed three redundant borrows in the held-input instrument.
The first full ordinary run had two failures: an existing residency teardown
returned Unconfirmed and an existing trace-pressure fixture hit its five-second
write timeout. Both passed individually without source changes. The final full
suite uses four test threads; this is not a maximum-concurrency stress claim.
The failed run and isolated rechecks remain in the evidence.

## Completion boundary

This advances #146's pre-turn successful Ready rollback and worker adapter.
It checks no broad epic box. Active-input/turn rollback, healthy Leave/save,
capture cleanup, remaining admission/author composition, dispatcher/process
exit, production artifacts and measured limits, security disposition and
qualified coupled cutover remain owed. The #255 accepted-before-TurnStarted
Stop election remains open.

These checks are author evidence, not a Planner grade or production
qualification. The required `code-review` and `verification-before-completion`
skills are unavailable in this session; manual checks and independent reads
are disclosed as such. Operator-only merge/deployment and draft publication
remain in force. No CodeRabbit review is requested.
