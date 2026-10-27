# Leave and save composition

Date: 2026-10-26. Updated: 2026-10-27. Author record for issue #146.
Branch: code/state-leave-save-composition. PR: #350.
Correction parent: 6c07dcb876bbea4a0f889c5190bf390cd1cf20df.
Stack base: #332, code/state-refusal-close-composition.

## Scope and authority

Forced and graceful Leave retain their original worker, member and admin owners
through Lower, capture, publication, release and the actual final outcome.
The operator folded #351/#352/#354/#356 into #350 with their merge ancestry.
The folded parent's tree equals #356's reviewed tree. This correction answers
Planner F1 and the operator's Q1 ruling of 2026-10-27: the completed save follows
the final termination mode, including a graceful Leave converted to forced.

Implements: foundation PERSIST-LEAVE-CAUSE, PERSIST-FORMAT,
PERSIST-PUBLICATION-GUARD, PERSIST-BEGIN/TIME/EXPIRY; persistence contract
amendment section 2 PERSIST-CAPTURE/CONFIRM; ADMIN-SAVE-TIME and admin Spec
section 3 unload and force-unload; TRACE-PERSIST and TYPES-PERSIST; harness
Spec section 6 Leave, Quiesce and wind-down; HARNESS-CAPTURE,
HARNESS-CONTROL-FAILURE, HARNESS-CLOCK, HARNESS-HELD-BOUNDARY and
HARNESS-HALT-BOUND. The folded stack also retains its demand-save and held
source/author composition scopes.

**Do not deploy or select foundation.** Production still uses dev_loop::drive.
The replacement admin invocation remains dormant. #146 remains open. The
operator undrafted #350 during this correction; the new delta needs Planner
review. No CodeRabbit review was requested.

| Named boundary | Author assessment |
| --- | --- |
| PERSIST-LEAVE-CAUSE; TYPES-PERSIST; contract section 2 | Conforms to the operator's final-mode ruling. The new foundation clause and code travel together; Planner regrade is pending. |
| TRACE-PERSIST | Each save phase retains the mode when its event is first submitted. A pending author keeps that original. The terminal unload carries the settled mode. |
| ADMIN-SAVE-TIME; PERSIST-FORMAT/PUBLICATION-GUARD | The validated Left settles the pending save's index cause before guard removal. Exact prefix, tail, guard and report identity are required. Failure retains guarded evidence. |
| PERSIST-BEGIN/TIME/EXPIRY; HARNESS-CLOCK | No new window or restart. F2 and J1 are Planner-accepted at cb741ce5; their timeout and bounded trace-read cases pass on this union. |
| HARNESS-CAPTURE; PERSIST-CAPTURE | The existing frozen frontier and owed filings remain. B10's separate Confirmed/Commit/frontier controls are not claimed by this correction. |
| Harness 6; HELD-BOUNDARY/HALT-BOUND | Existing S6 conversions and S7 outcome freeze remain. Caller UID and forced_by remain separate from persistence cause. |

## Correction and caller census

| Site | Handling |
| --- | --- |
| ServingCapture / CaptureOrigin | The lifecycle owner supplies its actual current forced flag at begin and each capture step. Requested, Confirmed, Published and Refused use that mode. An already-started trace author retains its original event through backpressure. Demand remains requested. |
| FoundationEnteredRollback | Carries the current RollbackDials outcome into capture. It freezes that outcome at the existing S7 boundary. No early freeze is introduced to simplify publication. |
| Capture::finish_leave | Takes forced from the already validated, retained whole Left reply. It cannot finalize before LeftReceived and does not infer the outcome from CaptureComplete or the original command. |
| Publication::finish / finalize_cause | Requires the matching final report and Published stage. Only the owned, durable, still-pending graceful tail may change to forced. It verifies the full original prefix and tail, atomically replaces the index and syncs before removing the guard. |
| Failure accounting | An uncertain replacement leaves the save guarded; a retained cause.part file is reported as Part work. Failed or completed publications grant no revision or retry. |
| Joined force invocation | After the original publisher concludes, verifies the completed index cause against its own actual Left.forced. It never writes the other publisher's metadata. |
| Successful-Left conclusion | Existing run-end, publication completion and marker duties keep their order. L3/Q2 is not elected or changed by this correction. |

The index replacement is a narrow, explicit PERSIST-LEAVE-CAUSE exception for
an incomplete publication. It changes no previous entry, ordinal, artifact
identity, immutable binding, confirmation or codec shape. The request's cause
can be provisional while its publication guard blocks load. A late S6 force
settles the completed metadata; it cannot rewrite prior trace events.

The foundation, admin and trace Specs and the internal admin-harness contract
amendment state this rule together. No new wire field or external contract
surface is introduced.

## Verification

Pinned nightly-2026-02-13, --locked, private Cargo target, no incremental
compilation, four ordinary test threads and one delegated process-test thread.
The lock gate ran first. Cargo, Git mutations and protective controls were
serialized. Input hashes were checked after the gates and after restoration.

- Ordinary admin: 550 passed, 66 ignored. Counts: admin binary 518, relay
  binary 3, invocation 6, manifest 3 and relay integration 20.
- Ordinary harness: 2,126 passed, 749 ignored. Counts: library 2,045, worker 7,
  authorship 11, channels 7, manifest 7, service 10, start_step 1, state_flow 7,
  and doctests 2 + 29. Nested child summaries are excluded.
- All 190 affected worker_session_ and worker_graceful_ cases passed in an
  isolated root-owned delegated scope, with the actual admin test participant
  and normal SQLite member built from the same source.
- All four delegated admin-join tests passed, including graceful late joins,
  forced completion, guarded artifacts and foreign terminal evidence.
- Both changed crates passed strict all-target clippy. Formatting, diff and
  first/last lock checks passed.

Focused tests cover an unchanged graceful Leave, an original forced Leave,
conversion before confirmation, conversion after confirmation and conversion
after publication. The composed new join-published case sends real JoinLeave
packets after Published is Recorded and before S7. It retains the earlier
trace causes while the actual final unload and all joined answers are forced.
The existing join-sealed case remains graceful after the S7 boundary.

Admin tests finalize the actual validated graceful-converted Left. They verify
unchanged binding bytes and prior index entries, exact final cause, rejection
of a forced-to-graceful regression, no rewrite of completed saves, foreign or
missing guard, altered prefix/tail, extra index data, foreign temporary work,
foreign report identity, and injected failures before and after atomic rename.

All five compiled protective controls failed their focused tests: hardcode all
trace causes forced; ignore a capture's later conversion; ignore final Left's
cause; rewrite without the pending guard check; replace unchecked index bytes.
The source was restored after each control, and both focused suites passed again. Logs, inventories, source hashes,
snapshot and control results are retained in the approved evidence deposit.
These are author checks, not Planner or production qualification.

## Remaining work

F1 has an author correction and Q1 has the operator's ruling; both await the
new Planner grade. F2 and J1 retain their accepted grades. The other findings
in review 5657773404 and its regrades are not collectively closed.
Implementation of the now-merged #348/#349 and #358 rules, L3/Q2's
completed-save/run-end ordering, the remaining held-owner and failed-run
boundaries, final dispatcher and
process-exit integration, qualified artifacts, measured limits and security
remain open. #360 carries the serving organ-loss work separately. Only the
operator merges or deploys. The broad #146 goal remains paused.

The named review skills are unavailable in this environment. Manual reading
and tests claim neither their invocation nor a waiver.

The correction exposed two distinct times: a trace event describes the mode
when it is submitted, while completed save metadata must follow the settled
termination. Keep that distinction explicit, and retain the publication guard
until the final cause is durable.
