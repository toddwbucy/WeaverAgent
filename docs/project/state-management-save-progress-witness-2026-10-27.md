# Serving save progress and send-clock evidence

Date filed: 2026-10-27. Author record for issue #146, section E.
Branch: code/state-save-progress-witness. Landing PR: #374.
Parents: preceding own head e04543bb and corrected #373 at 824a6e03.

This act answers #372 Planner W1, W2 and W5, then #374 A1-A5. A held
capture reply advances during organ last-word settlement in both FoundationInput
and S9. A queued save keeps its boundary behind settlement. Management dial
retirement shares the missing post-send clock check. The ordinary listener
fixture requires exactly one executed child test. A6's claimed missing fault
assertion is declined with the existing assertions identified below.

## Authority and consumer review

Implements: harness Spec section 6 coordination and Leave ownership; harness
amendment section 3 HARNESS-SAVE-TIME, section 4 HARNESS-HALT and
HARNESS-HALT-BOUND, and section 6 HARNESS-SAVE; foundation Spec section 5.2
PERSIST-SAVE and section 5.4 PERSIST-TIME. Author assessment: conforms for each
named section in the exercised save, observation and first-failure paths.
No new protocol, deadline, failure policy or dispatch election is introduced.

| Consumer | Result |
| --- | --- |
| FoundationInput | Prepare due save expiry and advance organ containment/accounting first. Before settlement, advance only a save with a held reply. Save Progress returns before an additional outer wait. |
| FoundationInput::advance_save | Retain the original save until Complete and expose Complete/Progress/Pending to its caller. |
| FoundationResting at S9 | Apply the same held-reply exception during settlement. Progress permits another reduction; a pending save waits through the original ports and deadlines. New management remains behind settlement. |
| PendingSave::reply_held | Inspect only the existing capture receive slot; consume no packet and grant no boundary or timing authority. |
| RollbackDials::prepare_dial | Call the shared failed-dial retirement helper directly after the existing live check. Preserve the accepted next connection on fatal failure. |
| RollbackDials::recover | Use the same sent/no-clock check, retaining the original dial and first TraceFailed. Ordinary peer departure still retires the dial. |
| Leave and S9 listener callers | Share preparation and recovery. Healthy retirement precedes adoption, with the original accept time and read allowance. |

The pre-settlement exception permits reduction of an owned reply, not admission
of a queued capture. Expiry and containment still precede it. The original peer,
request, capture identity and absolute intervals stay with their owners. A save's
pump can retain another reply, so Progress returns before another outer wait.

## Instruments and review disposition

The input held-reply case uses actual worker sockets and a normal SQLite member.
An SPU fixture closes decode but retains stderr until the held ImageChecked is
reduced. One controlled step supplies no readiness and no elapsed clock time.
The test requires consumption, the unchanged CaptureWindow and at most one wait
measured immediately around that step. Real monotonic time and kernel waits then
complete the save, two NoResidency requests, Leave and child reaping.

The queued-boundary case finishes the actual Requested author without electing
the boundary, observes real decode EOF with stderr held, and advances input. It
requires QueueBoundary, the original SaveQueued, no CaptureWindow and no frozen
capture. After settlement, its completed capture must include the organ-loss
record in its frozen watermark. This differs deliberately from the already-live
capture, whose original watermark precedes that later record.

The S9 case first reaches a real refused graceful Leave. Its next demand save
receives ImageChecked while a second controlled decode EOF keeps settlement
pending. It requires Progress, the consumed packet, unchanged CaptureWindow and
pending settlement, measuring waits around that step. It then completes the
save and a fresh Leave, with one SPU-loss record and Unconfirmed release. This
case uses scripted member/admin replies over real sockets; it is not the SQLite
qualification case. All model and classifier responses remain declared doubles.

A6 is declined as a missing-assertion finding: at e04543bb,
context_worker_organ_loss_tests.rs already checks exactly one
organ_death_observed after trace_through, its SPU identity, exact last_word and
sequence relative to the frozen capture. Held-reply mode reaches those checks.
The new queued case changes the expected ordering explicitly. The S9 case also
checks its actual fault and release outcome after the controlled reduction.

The clock regression performs a socket send and fails its clock observation.
Recover-first and prepare-first must preserve sent/no-clock evidence, keep the
next accepted connection and retain the first TraceFailed. Both now call one
retirement helper without wrapping and unwrapping the same failure.

The listener child runs through unshare --map-root-user. Its parent requires a
successful summary with exactly one passed test: zero matches or an ignored
match fails. This instrument requires user namespaces on its lane. An unavailable
unshare is reported as a failed prerequisite, never a silent pass or skip.

## Validation and limits

Restored-source author gates pass 2,093 library cases, seven worker cases,
43 integration cases and 31 doctests: 2,174 ordinary cases. The library has
833 ignored cases and explicitly filters known flake #331. Nested child
summaries are excluded. Strict all-target harness clippy, fmt, lock and diff
checks pass. All 111 selected process cases pass against freshly built harness,
admin and SQLite-member binaries. They include the two new cases, all selected
S9/demand-save neighbors and eleven joint admin/worker/SQLite refusal cases.
All 764 Rust/Cargo/toolchain hashes match before and after the final gates.

Six independent compiled controls fail their intended checks: advance a queued
capture before settlement; remove S9's held-reply exception; remove input's
held-reply exception; restore the second outer wait; bypass shared sent/no-clock
retirement; and make the user-namespace child match zero tests. Every source was
restored before final validation. The previous head's four controls remain
separate evidence, not additional runs on this revision.

Initial failures are retained: the S9 test first accessed a private peer instead
of the existing port accessor; the queued fixture inherited the opposite
watermark expectation; and a later build exhausted /tmp. Disposable Cargo caches
were removed and the complete gates rerun with incremental compilation disabled.
Those compile/fixture/environment failures are not protective controls.

Cargo used the pinned toolchain, --locked, a private target and serial runs after
lock.sh. Verification-before-completion was applied from its installed skill;
manual consumer/Spec review supplements the still-unavailable code-review skill
invocation. These results are author evidence, not Planner/security review or
production qualification.

#373 has a zero-fix Planner regrade with B1-B9 carried. #371 merged at 0c80e52f;
its refusal-only timing implementation is a following act and is absent here.
#369, #359's remaining fixture work and #372 T8 remain carried. Foundation stays
unselected. Regrade, operator merges, remaining composition, qualification,
security disposition and coupled cutover remain owed. No broad #146 box closes.
