# Harness STE100 transport implementation

**Version:** 0.1, 2026-10-21.

Draft [PR #200][pr] implements the harness side of Spec 02 CHANNEL and PORT, using the
existing HARNESS-CLASSIFY trait. It is the transport part of C1, stacked on the filing
coordinator, PR #199 at 2b721285. The SPU's matching dispatcher is in PR #191. The
shared vocabulary comes from PR #184. No harness dependency on the SPU is introduced.

## Plan and boundaries

The connected Unix SEQPACKET descriptor has one owner. Its fragment channel holds one
partial receive and one pending send. A progress call advances at most one datagram
in each requested direction. An interrupted syscall yields to the caller. No call
waits for readiness or supplies a deadline. The run's common pump owns waiting.
Reason: CHANNEL defines fragmentation, and PORT forbids a second harness reader or
runtime. [Spec 02 CHANNEL, PORT, HARNESS-CLOCK]

The concrete adapter starts with the judged bootstrap inventory, sends its exact
scheme text, and requires the complete matching Ready before accepting a request.
AdmissionRefused remains a typed admission outcome. The adapter verifies the shared
representation and echoes. It does not open the artifact or qualify its language.
Reason: CUSTODY gives inventory and artifact admission to their existing owners.
[Spec 02 CUSTODY, CHANNEL, PROTOCOL-OWNER]

The adapter implements submit, try_result and readiness without changing the trait.
An inherent wants_write accessor lets the common pump poll POLLOUT only while send
bytes remain. needs_progress exposes interrupted receive work before the pump waits.
That matters when an empty packet has consumed the last readable notification.
readiness continues to borrow the real socket so the coordinator's non-consuming
terminal probe can observe peer loss while a result is held.
Reason: a queued fragment may need writable readiness, while ResultHeld must retain
the same peer's failure observation. [Spec 02 PORT, HARNESS-CLASSIFY]

| Adapter state | Event | Next state and required result |
| --- | --- | --- |
| Starting | Valid bootstrap bytes queued | Send bounded fragments, retain exact cursor |
| Starting | Matching complete Ready after bootstrap sent | Return Ready once, service becomes Idle |
| Starting | Complete AdmissionRefused | Return refusal once, retire the channel |
| Starting | Request submission or premature/malformed answer | Refuse locally or fail admission, no request can run |
| Idle | Next request with loaded scheme | Own the whole request and fragments, InFlight |
| InFlight | Another submit | Busy with the second request unchanged |
| InFlight | No complete reply | Pending, keep request and partial bytes |
| InFlight | Complete validated reply after request sent | Transfer once, release adapter request slot |
| Idle | Unsolicited reply | Fail the port, never assign it to a later request |
| Live | Bad framing, wrong identity, malformed result or peer loss | Preserve first failure, retire both directions |
| Failed or refused | Any progress or submit | No reopening or resend |

The coordinator remains responsible for ResultHeld and exact trace authoring before
reuse or another submit. A complete ComponentFailed reply is transferred as a reply,
not converted into a local EOF fault. The existing coordinator then records it and
halts with its original code. Correct NotSte100 remains an ordinary result.
[Spec 02 PORT, HARNESS-CLASSIFY]

## Verification

Tests exercise connected socket custody, exact headers, fragment size and total bounds,
offset and ID changes, duplicate or missing fragments, MSG_TRUNC, partial peer loss,
backpressure and non-inheritance. Admission tests change each echoed field and reject
wrong or duplicate JSON keys, premature replies and repeated Ready. Service tests
exercise all result variants, Busy ownership, exact byte/target/scheme correlation,
malformed bindings, extra replies and complete failure followed by peer close.
[Spec 02 CHANNEL, PORT, TYPES-STE100-ADMISSION]

A composed test must pass the concrete port through the existing Classifier and real
trace barrier. It must hold a result without another request until the exact recording
proof and retain a complete terminal reply through the required peer close. Deliberate
removal of each selected protective check must fail its named assertion. These are
transport and correlation fixtures, not production STE100 qualification.
[Spec 02 PORT, HARNESS-CLASSIFY, CLASS-7]

The private target is /tmp/weaver-state-ste100-transport-target. The focused run
passes 37 tests: 16 fragment tests, 17 adapter tests and four recording integration
tests. The complete harness run passes 540 tests with 17 ignored instruments. None
of those ignored instruments is claimed as run in this act. All-target harness lint
at -D warnings, rustfmt, the lock gate and diff checks pass.

Twelve compiled protective changes fail their named assertions. Seven cover fragment
bounds, offsets, truncation, live empty packets, interrupted receives, pending status
and atomic sends. Five cover Ready echoes, premature replies, header identity, binding
validation and terminal retirement. Exact source bytes are restored, and the focused
baseline and full suite pass afterward. Independent review verifies the log failures
and current source hashes. A script stopped on an ambiguous mutation anchor after
eight cases. The remaining four ran with a unique anchor. That script stop is not
counted as a behavioral failure.

Review closed premature final-fragment acceptance and the interrupted-receive variant
of that fault. Test enumeration also caught an unregistered adapter test module before
the final gates. These are Executor checks. The exact local code-review skill is not
available, and Planner grading remains owed. No synthetic result qualifies STE100 or
deployment throughput.

## Remaining work

This act supplies no serving cutover, worker fork/exec integration, production SPU
backend, model selection, grammar qualification or system prompt. H1b2 still owns the
run-wide pump and lifecycle. C1's remaining production duty and O02-1/O02-2
qualification remain open. Draft PR #235 proposes the exact manifest locator and
acquisition custody. It remains unapproved until the operator's ruling and merge. The
SPU open-label gap is separate. This transport implementation chooses neither
representation and supplies no artifact acquisition or qualification evidence.

The cross-end read also found that PR #191's SPU fragment helper reports a live empty
datagram as EOF. This near side distinguishes malformed empty traffic from peer close.
The corresponding C1a correction is published in PR #191 at 19e6afb9, with fresh
host and Olympus gates. It remains part of the dependency chain before cutover.

[pr]: https://github.com/toddwbucy/WeaverAgent/pull/200
