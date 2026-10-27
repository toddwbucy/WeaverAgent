# Serving loss and retained reports

Date filed: 2026-10-27. Author working record for issue #146.
Branch: code/state-serving-organ-loss.
Base: #356 at 7a0d2b94c8dad65ba490430b91de37e323edd16d.

A gate exit during a tool call now retains the original report and any result
already sent by that gate, contains the actual tools, files the resulting pair
accounts and records one stopped turn. An unsent request retains its packet and
becomes not dispatched. No later request is dispatched to the lost gate, and
closing the turn makes no claim that a world reply reached that gate.

Run-scoped gate and SPU observations also preserve an unrelated healthy save's
frozen frontier. A retained observation uses its own trace claim, original
observation time and existing settlement or halt bound. A known containment
failure latches before trace pressure can delay it. Failed exit advances this
observation debt while the existing Lower and release owners continue.

This advances #146's lifecycle composition, author migration and failed teardown.
It supplies part of #350 L6 and #357 N1. Neither issue nor broad epic item closes.
The remaining integration work below is required before foundation activation.
Foundation remains unselected and main.rs still calls dev_loop::drive. Do not
deploy or select the incomplete replacement. Only the operator merges and deploys.

Implements: admin Spec 3.0's gate/SPU death table and 6.1 containment;
state-management-foundation-Spec 5.2 PERSIST-SAVE's gate-death row;
weaver-harness-Spec 2.2 and 6; harness amendment HARNESS-CLOCK, HARNESS-CAPTURE,
HARNESS-RECORD-LIFETIME, HARNESS-HALT and HALT-BOUND; Spec 05 PAIR-END and
PAIR-LEAVE; harness-gate and harness-SPU contracts section 5.

| Section | Author assessment, pending Planner verification |
| --- | --- |
| Admin 3.0 and 6.1; Spec 05 PAIR-END | Conforms for exercised serving gate loss: actual containment before an absent sent result becomes gate_fault, a received result keeps its fate, unsent requests become no_gate, original identities and filings precede one close. S4-S7 integration remains incomplete. |
| PERSIST-SAVE; HARNESS-CAPTURE | Conforms for exercised gate loss during a healthy SQLite save and save-window expiry: retain the same capture, frontier and image, settle the original cleanup, and preserve later Leave. Later SPU-dead input is an explicit failing case held on #358. |
| HARNESS-RECORD-LIFETIME; harness and channel contracts | Conforms for exact received reports and observation claims on the tested paths. Earlier authors keep their originals and receipts. Graceful and failed handoff of a still-owed received report remains incomplete. |
| HARNESS-CLOCK | Conforms for the measured observation and S3 boundaries: the first fresh observation sample is retained, duplicate EOF cannot renew it, and terminal accounting is bounded before a report's author wait. Save and Leave deadlines return to their existing owners. |
| HARNESS-HALT / HALT-BOUND | Conforms for the exercised retained-observation and Lower paths: first fatal cause wins, mandatory teardown is not postponed for ordinary trace capacity, and late accounting uses the original halt cap. Actual failed kernel containment and every failed-report handoff are not qualified. |

## Owner and caller census

| Owner or caller | Disposition |
| --- | --- |
| ServingObservationAuthor | One context and one event claim. Its Recorded sequence cannot bind an ordinary output, check, mark, repair, pair or filing author. Queued saves share this private claim mechanism. |
| ObservedOrgans | Retains the actual gate/decode/residency identities and first-observed order. Last-word evidence refreshes from the owned child before authoring. Capture content position stays frozen while the live Recorded watermark advances. |
| FoundationInput / FoundationManagement | Retain socket evidence before continuing the original input, turn or management owner. Prepare an already-retained save expiry before observation work. A send without its clock witness keeps fatal precedence. |
| Received Report | Owns ReceivedOrgan, opaque account, source identity, original event, private author claim and actual sequence. It yields behind an earlier author and survives the input-to-rollback/failed-input ownership handoff. That custody is not proof that every later handoff records the debt. |
| OrganPort after Closed | Ordinary send and receive stay frozen. A private receive reads at most one queued nonempty packet into the existing sole slot, retains first failure, and yields on reset or interruption. A nonempty malformed tail remains a refusal. |
| S3 pair execution / accepted Stop | Reconcile complete send before fate. Wait for held or queued evidence and owed rejection before assigning a missing result. Preserve accepted Stop, CallIds, StatementRefs and terminal receipts. Arm the existing terminal Decision bound before report pressure. |
| World/input close | A checked lost-gate outcome retains unsent close evidence without inventing successful delivery. A completed send still requires its real timestamp. |
| Save cancellation | Both direct cancellation and observation preparation withdraw an unsent Commit before fallible member Abort setup. The expired original window cannot authorize publication. |
| Failed exit / Lower | Advance retained run-scoped observations after turn accounts retire. Include writer capacity and the original last-word settlement wake without delaying Lower or taking an earlier rejected-return claim. |
| Release | The exact previously observed SPU residency end permits only Unconfirmed. Gate loss does not create a second death observation. Actual release/reap remains the existing lifecycle owner's duty. |

Loop orchestration remains in worker/dev_loop, classification in the SPU, and
weaver-state remains the store member. No dependency or public wire changes.
The manual self-check read the changed callers, receipt ownership, timeout
precedence, closed-port receive, pair fate and failed-tail ordering. The required
code-review and verification-before-completion skills are unavailable in this
session. No invocation or waiver is claimed.

## Validation

The final source passed 2,148 ordinary harness tests and doctests, with 772
ignored by that invocation. Counts exclude nested child summaries. All 704
selected delegated worker cases passed in one serial run. The only excluded
worker case is worker_session_demand_save_sqlite_organ_loss_spu, which reproduces
the later-input refusal gap proposed in #358. Other ignored non-worker
instruments remain outside this run. The eight privileged admin-startup cases
and production model/device qualification are not claimed.

Strict harness all-target clippy with -D warnings, workspace formatting, diff
checks and lock.sh before and after Cargo passed. Pinned nightly-2026-02-13 and
--locked were used with a private target, no incremental compilation and four
build jobs. Ordinary tests used four threads and process cases one. The normal
SQLite member and actual admin test participant were built in that same target.
Joint process fixtures used real root in delegated scopes. Source hashes matched
before and after the final gates.

The report checks include 67 transport tests, three private report-ownership
tests and five actual gate-process cases: a received report, report under trace
pressure, accepted Stop with a report, unread Stop control at gate exit, and that
same reset boundary with a completed result already queued.

| Compiled protective control | Observed failure |
| --- | --- |
| Disable closed receive | Accepted Stop cannot settle the original exchange. |
| Remove queued-receive guard | A completed crossed result becomes Killed instead. |
| Remove terminal Decision bound | Full writer wait has no declared terminal bound. |
| Resume report before containment/bound progress | The same full-writer case loses its bound. |
| Remove source-port check | A foreign port incorrectly supplies report custody. |
| Borrow the prior author's claim | The report encounters the earlier caller's held original. |
| Read with the sole slot already full | The next receive overwrites or misuses held evidence. |
| Ignore a malformed queued tail | The required tail refusal disappears. |

All eight controls compiled and failed their intended assertions. The first four
were compiled and detected again after the final timeout correction. The other
four reach unchanged private report/transport paths. All originals were restored
before the final gates. Earlier checkpoint controls cover exact observation
claims, first-fatal ordering, original clocks, save cancellation and Lower wakes.
Those earlier controls were not all repeated at this final source.

The first broad run passed 700 worker cases and failed four save/Leave cases. An
added generic live check intercepted their deadlines before the owning operation
could reduce them. Removing that preemption restored all four cases, followed by
the complete passing run above. The full-writer fixture also had a pre-marker
failure whose cause is still unexplained. Its improved diagnostics perform no
extra socket read. Five unchanged standalone repeats, the focused regression
sequence and the final full run passed. Failed runs and exact source snapshots
remain retained as evidence.

An early diagnostic MSG_PEEK consumed ECONNRESET and contaminated a pass. Those
probes were removed. A separate socket check disproved the initial theory that
shutdown discarded the queued packets. The tested correction leaves shutdown
unchanged and permits the private receive through the port's logical freeze.

## Remaining integration and limits

- Complete still-owed received reports through graceful and failed handoff,
  including retired-port identity and bounded trace-only accounting. Retaining
  Report in FailedInput is custody only.
- Complete S4-S7 loss composition, capture/release port lending, failed teardown
  and actual worker-process integration. Existing primitive and selected circuit
  passes do not establish these paths.
- Implement later NoResidency input only after #358's operator elections, Planner
  review and merge. Its actual SPU-exit fixture remains a known failing case.
- Resolve #348/#349 and #350's authority holds, inherited review carries, and the
  separate retained-result and copied-byte qualification work.
- Qualify pinned language artifacts, classifier, separately reviewed prompt,
  measured limits and security before the coupled serving cutover.

An inherited transport limit remains: on the measured host, a zero-length
SEQPACKET and EOF both return zero bytes with zero flags. An empty record can
precede data. Existing status handling treats zero plus closed as EOF, and this
slice retains that interpretation. Preservation here covers nonempty protocol
packets and nonempty malformed tails. Empty-record differentiation remains in
the transport/security audit.

The checkpoint is a reviewable implementation slice. These author checks are
not a Planner grade, security disposition, production qualification or epic
completion. #332 stays draft until the operator's final review stopping point.
