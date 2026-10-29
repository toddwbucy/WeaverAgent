# Resting management and failure composition

Date filed: 2026-10-27. Author record for issue #146, section E.
Branch: code/state-resting-failure-composition.
Base: code/state-capture-refusal-composition at 9ac97a8c (PR #370).

This act fixes two S9 scheduling defects found while answering #370 Planner S1
and supplies the five management/failure cases left in that PR's body. The
worker first completes an actual graceful Leave's capture refusal and keeps the
entered run with its gate lowered. The cases then drive that retained owner.

## Authority and implementation

Implements: weaver-harness Spec section 6, S9 and the coordination read allowance;
weaver-admin Spec section 3.0 S9; harness amendment sections 3 and 4,
HARNESS-SAVE-TIME, HARNESS-HALT and HARNESS-HALT-BOUND; admin amendment section 3,
ADMIN-FLOW-END. Author assessment: conforms for the exercised management, capture
progress and failure rules. No wire, policy or limit changes. #370 V1 remains a
separate Spec election in #371 and is not implemented here.

The shared rollback listener now retires an answered or failed active dial
before adopting an already accepted next connection. The old order could leave
that connection in the listener slot while its caller waited with no accept or
read interest. Its original accept timestamp and read allowance are retained.
The change applies to both ongoing Leave and S9 management.

PendingSave now distinguishes Complete, Progress and Pending. Once its common
pump runs, its caller must return to capture reduction before another blocking
wait. The old boolean made an S9 owner wait again with ImageChecked already
held; neither another packet nor a new state request could wake it. The same
ordering applies to a retained state reply. The S9 caller still prepares other
root commands on that step. FoundationInput, the other caller, keeps its existing
behavior: it retains an incomplete save and advances it on its next step.

| Case | Required observation |
| --- | --- |
| Retained next connection | Finish the first actual answer, accept the second dial while that answer's owner remains, then install the second reader before waiting. |
| Held capture reply | Each S9 drive refuses to enter an outer blocking wait while its save owns a captured reply. Command timeout diagnostics name the exact command and publication stage. |
| Paced owner | A two-millisecond delay between outer owner steps still permits Observe, Stop, demand save and a new Leave. This uses actual kernel readiness. |
| Silent root dial | Close unanswered after its original two-second read allowance. Later commands still complete. |
| Malformed root dials | Invalid JSON and a lifecycle opener with Close position close unanswered without halting the run. Later commands still complete. |
| Expired demand save | Return CaptureNotReady on the same exchange. Record one refusal with the exact submitted window and requested capture identity, without a Leave tail. A later save and Leave succeed with distinct capture identities. |
| Member EOF at S9 | Latch carry_failed at state_reply, retire state/classifier ports, release and reap children, and record component-failure Unload without a new capture. |
| Trace failure at S9 | An actual idle SPU exit owes an observation; a closed sink makes the actual write fail. Preserve trace_failed as first cause, retire ports, retain writer failure and terminate children. Claim no Recorded Unload. |

Each healthy management case also checks Observe Idle, Stop AtRest, no repeated
wind-down or new turn, and unchanged Lower evidence. The member sees only the
initial refused snapshot and the two successful later snapshots; the expired
request grants none.

Both failure cases move numeric time past the previous Leave deadline before
breaking a connection. Clock readings and actual kernel waits use the same
offset. The retained first failure owns one unchanged 150-second halt interval;
the ended Leave cannot shorten it. No further member request is sent. With a
working trace, the run ends with no active Leave cause or forced Leave and an
explicit component_failure end reason.

A failed trace drain abandons the remaining record and exits failed. The test
checks actual child reap or a successful termination signal at that boundary;
it separately verifies eventual reap after the owner closes. A signal is never
reported as a reap. This does not qualify deployed process exit or admin recovery.

## Reproduction and controls

The controlled two-connection regression failed against the old listener order
and passed after correction. That repair alone was insufficient: one of twenty
parallel five-case S9 selections still stalled. A diagnostic rerun failed on its
twentieth selection, identifying demand save's wait for ImageFinished. The next
instrument observed the held capture reply immediately before the erroneous
outer wait, failing on selection twenty-one. The logs retain those failures.
The held-reply reproduction depends on scheduling; the listener case controls
its schedule directly.

With both repairs, twenty parallel and twenty serial five-case selections
passed: 200 process-case executions, not 200 distinct tests. They include the
original retry, force, SPU-loss and orphan cases plus the paced owner. This is
Olympus author evidence. It does not replace the Planner's recheck on the host
that first reported S1 or establish that every intermittent failure is removed.

Before the scheduling corrections, four compiled controls detected fatal
silent-dial expiry, acceptance of the wrong opener position, loss of queued-save
expiry and S9 cleanup capped by the ended Leave. All controlled runtime files
were restored byte for byte, and all nine then-existing S9 cases passed. Those
controls belong to that earlier source; the listener and save scheduling changes
have the separate failing regressions described above.

The initial failure-test assertions expected protocol_failed for EOF, a forced
Leave flag after that Leave had ended, and a successful trace drain with a broken
sink. The channel failure mapping and failed-cleanup contract corrected those
assertions. Those failed attempts remain with their source snapshots. The later
S1 investigation did identify runtime defects and changed code.

## Validation and limits

The final harness package gate passed 2,091 library cases, seven worker cases,
43 integration cases and 31 doctests. These are the outer executable results;
child re-exec summaries are not added again. The library declares 831 ignored
cases and explicitly filters known pending-author flake #331. The later privileged
selection passed 109 distinct process cases, including all ten S9 scenarios, the
controlled listener regression, demand-save and surrounding lifecycle cases, and
all eleven actual admin/worker/SQLite capture-refusal cases from #370. Those scopes
overlap the 200 repeated S9 executions; they are not additive distinct coverage.

All-target harness clippy at -D warnings, formatting, lock and diff checks passed.
All 762 Rust and Cargo/toolchain source hashes match before and after the package
and process gates. Cargo used the pinned toolchain, --locked, a private target and
serial operations after lock.sh. Privileged cases ran serially in an owned
delegated scope. #369 remains open even though its ordinary case passed.

The manual author review follows both PendingSave callers, both listener entry points,
the translated clock/wait pair and the test-only failure observations. The named
self-review skills are unavailable in this session. Planner review and operator
merge remain owed. #370 V1 awaits #371's election and merge; #370 T2/T5 and V2-V8,
#365's elections and the earlier stack's batch findings remain separate work.

The recorder, sockets, process owners and kernel readiness are real. Model,
classifier, member replies and publication exchange in the S9 cases are declared
fixtures. The surrounding actual admin/SQLite cases retain their separate scope;
none replaces qualification with production artifacts. Foundation remains
unselected. No broad #146 checkbox closes and no deployment permission follows.

The planned five management/failure cases are implemented. Review then required
runtime scheduling repairs. Keep command-stage diagnostics and inspect retained
work before the next blocking wait; a single green process run did not establish
S9 reliability. The failing schedules and restored-source repeats are part of the
review evidence, alongside the qualification and policy work still open.
