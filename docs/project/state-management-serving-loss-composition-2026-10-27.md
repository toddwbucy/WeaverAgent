# Serving loss and retained reports

Date filed: 2026-10-27. Author working record for issue #146.
Branch: code/state-refusal-close-composition. Base: main. PR: #332.

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
weaver-harness-Spec 2.2 and 6; harness amendment HARNESS-CLOCK, HARNESS-PUMP, HARNESS-CAPTURE,
HARNESS-RECORD-LIFETIME, HARNESS-HALT and HALT-BOUND; Spec 05 PAIR-END and
PAIR-LEAVE; harness-gate and harness-SPU contracts section 5.

| Section | Author assessment, pending Planner verification |
| --- | --- |
| Admin 3.0 and 6.1; Spec 05 PAIR-END | Conforms for exercised serving gate loss: actual containment before an absent sent result becomes gate_fault, a received result keeps its fate, unsent requests become no_gate, original identities and filings precede one close. S4-S7 integration remains incomplete. |
| PERSIST-SAVE; HARNESS-CAPTURE | Conforms for exercised gate loss during a healthy SQLite save and save-window expiry: retain the same capture, frontier and image, settle the original cleanup, and preserve later Leave. The SPU-death release path has a separate actual-child test. Later input remains an explicit failing case awaiting implementation of merged #358. |
| HARNESS-RECORD-LIFETIME; harness and channel contracts | Conforms for exact received reports and observation claims on the tested paths. Earlier authors keep their originals and receipts. Ordinary Leave carries owed organ observations to Recorded before exiting the gate stage. Every graceful and failed handoff of a still-owed received report remains incomplete. |
| HARNESS-CLOCK / HARNESS-PUMP | Conforms for the common observation wake, real readiness interests and measured S3 boundaries: the first fresh observation sample is retained, duplicate EOF cannot renew it, and terminal accounting is bounded before a report's author wait. Save and Leave deadlines return to their existing owners. |
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

## Planner review disposition at the revised head

The fold retains #360 and #350 through their original merge ancestry. The
[current folded review record](state-management-folded-serving-review-2026-10-27.md)
carries the additional #332 corrections and final gate scope. The validation
figures below apply to the earlier #360 head 134fbe80. Only the operator merges.

| Review row | Disposition and evidence |
| --- | --- |
| G1 / C8 | Corrected. Observation advance returns its wake to the common pump, which retains real interests and earlier terminal deadlines. Pending observation work now waits through that pump even when its caller uses advance. Separate tests pin both the observation and the common wait. |
| C1 / X1 / X4 | Corrected. Waiting also yields to the existing report, retaining a second Fault on its original port. An actual two-report process case records both in arrival order after the held original returns. Accepted, Turn, Refused, Aborted and Completed stages retain a pending report without entering the input-work panic arm. The real gate-process test exercises Accepted and Turn with an earlier held original, then records exactly one report after that original is returned. |
| C2 / X6 | Corrected for the crossed held-packet boundary. A withheld rollback gate can use only the rollback owner's retained exact port identity. Missing identity, foreign identity and an explicitly supplied foreign port still refuse. The held packet remains untouched. This does not claim every failed-report handoff. |
| C4 | Corrected. An observed closed gate with an empty receive slot and pending tail receive supplies an immediate wake through the common pump, which performs that receive. Reset/interruption cannot leave that local receive waiting only for Decision expiry. Real sources and an already expired terminal deadline keep precedence. |
| C5 | Guarded. Only an executing call can acquire a new terminal wait during gate settlement. A boundary test uses a recorded batch, a retained owner and no execution, with its wait removed to isolate that predicate. This is not a reproduction of every claimed between-call path. |
| C6 | Corrected for unstarted queued-save records. Eligibility requires Entered. Releasing yields while keeping the original request. Already-admitted save authors and the complete release handoff remain under the existing integration limit. |
| H7 | The release proof no longer depends on later-input handling. A separate actual-SPU-exit case completes its healthy SQLite save, performs the later forced Leave, reaps the child and records Unconfirmed. Removing the known-dead residency branch must fail this case. |
| Codex 4390037527 | Corrected in received Report and organ Observation. Actual Recorded evidence stays on the private claim when a recoverable deadline interrupts continuation. Each owner resumes that claim before reporting success. Deterministic post-Recorded expiry tests consume the original completion, clear the claim, record the next save and detect each restored early-success mutation. |
| X2 | Corrected. An owed rejected reply is checked before a terminal Decision wait is armed. The actual rejected packet retains its original wait and author across trace pressure and closure of the same gate; it then records exactly once. |
| X3 | Corrected for ordinary accepted Leave/rollback. The consuming owner advances the same observation after mandatory Lower transport work and cannot pass the gate stage with observation debt. Its original last-word anchor and author claim remain unchanged. Unlatched Serving containment can use only the actual gate's private, completed Leave-sweep proof. Two actual gate EOF cases enter rollback during the last-word window, with containment already latched and still unlatched, and record one serving-death account before Unload. |
| X9 | Corrected. The exact port-minted capture ticket can withdraw its wholly unsent bytes after Closed, retaining the first failure and returning the original packet. Other failures remain refusals. A disconnected-admin case revokes the original Commit before member Abort, records the capture refusal and leaves the run live. |
| X7 | Corrected in the folded #332 act. Held and unread queued SPU Fault packets enter the same report owner after observed loss; separate actual process/SQLite/Leave cases pin both orderings. The earlier H7 release-only case did not establish either packet race. |
| X11 | Retained batch in graceful conversion: an interrupted SaveQueued/CaptureWindow observation can delay setting forced until the existing drain bound. No new window or successful continuation is claimed. |
| X12 | Retained batch in accepted Stop/pair cancellation: an unsent request still uses live-port withdrawal while gate settlement yields. The exact closed-port send witness and existing Decision bound must carry through that caller. |
| X15 | Required self-check skill availability remains an explicit process carry; no invocation or waiver is claimed. |
| C3 / X5 / X8 | Retained batch work in #146's failed-handoff act. FailedRunCleanup has no tools handle to finish the earlier gate observation's containment. The gate debt can therefore hold a later SPU debt until bounded abandonment. That act must carry the owned tools evidence through Lower, preserve first-observed order under the original halt cap, and prevent observation authors from taking the slot reserved for a deferred TurnClosed. No invented containment result or claim that this path is complete. |
| C7 / X10 | Planner's decline retained. A whole send proves delivery to the gate, not the world. The original send evidence is preserved. |
| C9 / X13 | Retained consolidation batch for the close-owner act. WorldClose, input refusal and placement refusal share retain_lost_world but still reduce different duties: turn/input close, input finish, and place_response_lost. The caller census below keeps those sites visible. |
| C10 / X14 | Retained consolidation batch for the failed-report handoff act. Received reports and local observations still hold separate claimed-author state machines. Their common claim must be consolidated with the handoff, retaining exact originals and tickets. Exact-ticket capture and closed-organ withdrawal also retain duplicate transport checks; consolidation must preserve their distinct decoded types and failure rules. |

| Changed boundary | Callers and new surface |
| --- | --- |
| Observation wake | FoundationInput and FoundationManagement advance the retained observation. RecordingContext lends its wake to CommonPump, used by ordinary input, turn and lifecycle waits. Pending observation work uses the common blocking wait from advance as well as drive. |
| Pending report | FoundationInput's progress remains Pending until Recorded. Its at-rest step does no input work. Report validates a supplied port first, or the private withheld rollback identity. No public identity or new port is introduced. |
| Closed receive wake | All RecordingContext pump callers retain terminal precedence and real readiness probes. Only the exact observed gate with no held packet and an outstanding closed receive becomes immediately runnable, and the pump performs that receive. No send is re-enabled. |
| Ordinary Leave / rollback | After actual Lower transport progress, refresh the original children and advance retained observation debt. A private exact-gate sweep proof can settle earlier containment. No move to Capture/Grants until every owed observation records. Failed-run teardown remains separately limited. |
| Capture withdrawal | LifecyclePeer delegates the exact capture send ticket. Closed permits withdrawal of only the original wholly unsent datagram; no send is enabled and the failure stays latched. |
| Gate settlement | A pending rejection owns its existing wait before terminal arming. Executing pairs retain the existing terminal wait. A batch with no executing call gets no new wait from this observation. Existing waits are not cleared or renewed. |
| Queued save | ServingCapture checks Entered before starting its queued observation. Organ loss and received reports remain eligible in Releasing. No new save capability is granted there. |
| Lost world response | WorldClose::advance, FoundationInput::step(Closing), and FoundationInput's generation Closing keep their distinct completion duties after the shared exact-ticket withdrawal. |

## Merged authority awaiting implementation

Do not select or deploy the foundation dispatcher. The joined main now includes
#348/#349 and #358. Their merge removes the Spec authority holds, not the owed
code. The next INPUT-NO-RESIDENCY act in #146 implements Spec 02's WorldRequest
refusal, its types amendment and the two-request pressure/precedence tests. The
existing worker_session_demand_save_sqlite_organ_loss_spu case still reproduces
that separate gap and remains excluded. The new release-only case answers H7.
The capture-refusal composition act implements #348/#349's S4-S7 teardown and
publication outcomes. Neither act is completed by this review correction.

## Validation at #360 head 134fbe80

- 2,157 ordinary harness tests and doctests passed; 778 ignored by that invocation. Nested child summaries are excluded from this count.
- Three runs of all 703 selected non-joint worker cases passed at default parallelism. All six joint admin/SQLite cases passed serially, for 709 distinct selected worker cases. The only excluded worker case is the disclosed later-input SPU-loss case awaiting merged #358's implementation.
- Strict harness all-target clippy, workspace formatting, diff and first/last lock checks passed.
- Pinned nightly, --locked, a private target, four ordinary test threads, and serial Cargo/Git mutations. Actual worker fixtures used private delegated scopes and the built admin test participant; no deployed agent changed.
- All 796 checked crate/config source files remained unchanged across the final gates. The working record was updated afterward.


Fourteen independent protective mutations compiled and failed the assertions intended
to catch them. Eight covered the first Planner corrections, two covered the Codex continuation finding, and four covered the latest Planner round. Each original was restored before the next mutation. The final
gates ran on the restored source.

| Mutation | Detecting boundary |
| --- | --- |
| Restore the interest-free wait | Non-waiting observation refuses any wait call. |
| Drop the common observation wake | Real-interest wait loses its required wake or terminal precedence. |
| Remove the retained-report stage guard | Actual gate report at Accepted/Turn reaches the panic arm. |
| Reject the withheld rollback port | The original report cannot record beside a crossed held packet. |
| Drop the closed-receive wake | Pending receive waits for Decision expiry instead of running. |
| Arm a terminal wait with no execution | The recorded-batch boundary acquires an unowned Decision wait. |
| Admit an unstarted save in Releasing | The queued request fails instead of staying retained. |
| Ask the known-dead SPU for Release | The actual SPU-exit/SQLite/Leave circuit cannot reach its required Unconfirmed result. |
| Treat report admission as completed continuation | Post-Recorded save expiry strands the completion and claim. |
| Treat observation admission as completed continuation | The same post-Recorded boundary prevents the next save. |
| Consume a second report while the first waits | The two-report process loses the first report or consumes the second packet early. |
| Arm terminal wait before an owed rejection | The actual rejected-return wait collides with terminal arming. |
| Drop observation continuation and exit guard at Leave | The real gate-death handoff retains unrecorded debt or omits its trace account. |
| Refuse withdrawal through a Closed capture sender | The disconnected-admin expiry halts instead of completing cancellation. |

The eight earlier report/transport controls at a52ffb63 remain scoped to that
checkpoint. They are not counted as new controls here. The full-writer fixture's
earlier unexplained pre-marker failure and contaminated MSG_PEEK diagnostic are
retained in that commit's working record. No peek or extra receive was added by
this correction. The Planner's G1 reproduction exposed the interest-free wait
that a single serial pass had missed.

An initial parallel run of this correction passed 698 worker cases. Eight
fixtures failed before their admin participant ran because the invocation did
not supply its test-binary environment. The rerun builds that participant and
passes its exact path to the fixtures. The six joint fixtures share a parent
process root, so they run serially. The other cases use default parallelism; the final counts below include the new Leave and two-report fixtures.
This changes the test invocation, not their production deadlines or assertions.

The first versions of two new boundary fixtures failed on their setup: a plain
EOF had no closed-receive debt, and an empty ledger was not an admitted batch.
They now use an actual unread-request reset and a recorded checked batch. The
receive test also checks DecisionBound, the existing terminal outcome. Failed
attempts and the exact restored source remain retained with the test evidence.

The latest fixture setup corrections are retained with the evidence: the rejection's wait was already armed, so no test re-arm is needed; the two-report boundary uses the original port's actual receive while intake is backpressured, and retains the earlier template records for later assertions. The initial compile corrections changed only test paths and inaccessible-field assertions.

## Remaining integration and limits

- Carry X11 and X12 through the graceful-conversion and Stop/pair-cancellation acts, respectively. The folded review record carries X7's correction and new evidence.
- Complete C3/X5/X8 and C10/X14's still-owed observation/report handoff through graceful
  and failed teardown, including actual tools evidence, retired-port identity,
  first-observed order and the original halt cap.
- Consolidate C9's close callers while preserving their different completion
  duties. The shared withdrawal already retains exact send evidence.
- Complete S4-S7 loss composition, capture/release port lending, failed teardown
  and actual worker-process integration. Existing primitive and selected circuit
  passes do not establish these paths.
- Implement merged #358's later NoResidency input. Its actual SPU-exit fixture
  with two subsequent requests remains a known failing case.
- Implement merged #348/#349's capture-refusal outcomes, carry the inherited
  review findings, and finish retained-result and copied-byte qualification.
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
completion. The folded review record states the current review boundary.
