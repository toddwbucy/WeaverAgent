# State management residency composition

Date: 2026-10-23. Version: 0.1. Author implementation record, not a Spec.

## Scope and pass statement

This act starts from #288 5b9a74f0455b22b9bd5b71eb096f5622430a921e. It consumes
actual PreparedStartup, forks the decoder into the same resource bag, forwards the
original supplied SPU instruction once in Admit, and waits for the exact sent
exchange's closing Admitted before giving the context to boot. The instruction is
retained unchanged; the caller still owes its judged admin provenance. The existing
actual fork owner retains PID, stderr/LastWord, residency and decode endpoints.
No scalar PID or descriptor adoption is introduced.

The owner retains the actual send ticket, received envelope and original wire bytes
before validating or yielding again. A wrong exchange, answer arm or premature
reply grants no boot continuation. Valid typed refusal remains unchanged in the
retained envelope for the aggregate refusal clerk. EOF grants no residency.
Stopped handoff carries the original startup observations, inventory, request,
returned directive if any, send/reply and resources. It does not perform cleanup
or grant retry, Ready or Left. Final extraction rechecks the same Enter window.

The common pump now uses its clock for tracked residency sends. Only the first
wholly unsent Admit is withheld for an observed Stop/Leave; Release and other
residency packets retain their ordinary transport behavior. Direct pre-send receive
uses the same owned port, so an already-arrived reply cannot become valid merely
because the request is sent afterward. No second reader or second handle is added.

The integration instrument runs structural Open, load, activation, actual classifier
fork/admission, actual decoder fork/Admit, template checks, prompt read, decoder Open,
pair recheck, plan summary and placement. Classifier and decoder children are Rust
protocol doubles reached through temporary shell launchers. State replies and model
behavior are scripted; fake time and readiness are paired. The actual residency
and decode endpoints remain in the common boot pump. A pass proves connected
ordering/custody, not language, SQL, GPU/model or runtime qualification.

## Authority and remaining work

Implements: weaver-harness-spu-contract sections 2, 3 and 5; weaver-organ-channel
exchange correlation/terminal ordering; weaver-harness-Spec section 2.2 fork custody;
HARNESS-BOOT, HARNESS-OPEN-LIVE, HARNESS-RECORD-LIFETIME and HARNESS-CLOCK in the merged
harness state-management amendment. The author grading is limited to this dormant
startup-to-boot connection. No wire shape, event, timeout or workflow policy changes.

Actual admin entry, refusal recording and response, accepted rollback, complete
healthy/Failed teardown, gate start, lifecycle Ready, serving caller migration,
production language artifacts/prompt/limits and coupled cutover remain owed. The
current serving implementation stays active and all new PRs remain drafts. Approved
but unmerged Spec proposals remain unimplemented.

## Integration findings and validation

The first compile runs found fixture-only serializer and shared-lifetime mistakes.
The harness has no direct serde dependency, so its fixture now uses the existing
serde_json serializer through a macro. The test rejoins the same resource owner
after ending the prepared context's borrow, rather than moving a borrowed fixture.

The first executable circuit found a production integration gap: the residency
pump arm used try_send without its common clock. Tracked organ sends require that
clock, so a real Admit could not leave the transport. The corrected arm uses
try_send_observed, as the existing gate arm does. Initial logs remain retained.
Final author gates pass: 1,836 harness tests with 18 inherited ignored cases,
160 trace tests, all-target harness Clippy, format, lock and diff checks. Eleven
nested child result summaries are excluded from the harness top-level count.
The private Cargo target remains /tmp/weaver-146-boot-composition-target for this
same isolated checkout.

Ten focused named tests pass: nine parent tests (the foreign-envelope test covers
wrong ordinal, opener and non-closing position) plus the no-work child entry.
They exercise the full two-child startup/boot/placement circuit, delayed admission,
exact typed refusal, wrong terminal arm, a real already-arrived premature reply,
EOF, final expiry and actual Stop before the first Admit send. The full circuit
retains the actual residency endpoint in the boot pump and asserts the send clock
lies within the original Enter window. Child cleanup guards run on test unwinding.
One test-only unnecessary_unwrap lint was corrected with an if-let; the final full
gates follow that correction and the expanded correlation corpus.

Eight compiled protective controls fail their named tests and all exact restored
runs pass: exchange correlation, complete-send proof, terminal arm, final Enter,
stopped wire custody, first-Admit cancellation, clocked residency send and closing
position. All 230 harness source hashes match the pre-control snapshot.
These are author checks, not a Planner/security grade or production qualification.
Prescribed review/verification skills remain unavailable; direct source/sequence
self-review and executable gates were used, without an independent-review claim.

## Next composition

Connect the aggregate Enter result to the actual lifecycle exchange: Recorded
refusal before its response, actual accepted rollback and retained release/cleanup
facts, and the remaining successful Enter gate/Ready obligations. Keep all startup,
residency and boot evidence in that outer owner. The existing source-only refused
Enter handoff expects a still-held classifier completion; joining it to #288 must
explicitly preserve the already-consumed exact admission and same-context provenance,
not rebuild a grant from a matching scalar. Complete healthy/Failed cleanup remains
owed, including the original bounds and accepted Leave cause. This is necessary
composition work; unrelated followup batches remain deferred.
