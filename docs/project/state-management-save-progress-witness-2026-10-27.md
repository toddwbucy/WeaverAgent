# Serving save progress and send-clock evidence

Date filed: 2026-10-27. Author record for issue #146, section E.
Branch: code/state-save-progress-witness.
Base: code/state-refusal-outcome-custody at ea2bc781 (PR #373).

This act answers #372 Planner W1, W2 and W5. A held capture reply now advances
while an observed organ's last-word settlement remains pending. A failed
management dial cannot be retired before its missing post-send clock witness
is checked. The existing two-connection listener regression runs in the ordinary
harness gate through its user-namespace root fixture.

## Authority and consumer review

Implements: harness Spec section 6 coordination and Leave ownership; harness
amendment section 3 HARNESS-SAVE-TIME, section 4 HARNESS-HALT and
HARNESS-HALT-BOUND, and section 6 HARNESS-SAVE; foundation Spec section 5.2
PERSIST-SAVE and section 5.4 PERSIST-TIME. Author assessment: conforms for the exercised
save, observation and first-failure paths. No new protocol, deadline, failure
policy or dispatch election is introduced.

| Consumer | Result |
| --- | --- |
| FoundationInput | Prepare due save expiry and advance organ containment/accounting first. Even when observation is still pending, advance the retained save. If that save reports Progress, return before an additional outer wait. |
| FoundationInput::advance_save | Return the existing Complete/Progress/Pending value. Retain the original save until Complete, including while organ observation is pending. |
| FoundationResting | Already consumes those three progress values from PendingSave; unchanged. Its retained-reader scheduling remains covered. |
| RollbackDials::prepare_dial | Route a failed active peer through the existing recovery owner before adopting an accepted next connection. |
| RollbackDials::recover | Existing sent-without-clock check latches TraceFailed at Author and retains the original dial. Ordinary transport departure still retires that dial. |
| Leave and S9 listener callers | Both use the shared preparation path. Healthy answer retirement still precedes adoption; the original accept time and read allowance remain unchanged. |

The new scheduling surface permits capture reduction during a pending organ
observation. It does not skip the prior containment/fatal check, extend either
interval, advance a turn through an active capture, or replace its peer, request
or original capture identity. A save's pump may retain another reply, so its
Progress result must return to reduction before another outer wait.

## Instruments and protective controls

The held-reply case uses the actual worker composition, admin socket and SQLite
member. The SPU fixture closes its decode socket but holds stderr open until the
save has reduced the actual ImageChecked packet. The driver observes real EOF,
retains that exact packet and proves last-word settlement is pending. One
controlled advance supplies no readiness and no elapsed clock time. It must
consume the held reply, preserve CaptureWindow and avoid a second wait while
settlement is still pending. The remainder uses the real monotonic clock and
kernel waits through save completion, two later NoResidency requests, Leave
and child reaping. This is a scheduling regression, not a latency measurement
or production-model qualification.

The missing-clock regression performs an actual socket send and then fails its
clock observation. It checks both recover-first and prepare-first entry orders.
Both must retain sent/no-clock evidence, keep the next accepted connection,
latch TraceFailed and preserve that first failure on the next preparation.
The older two-connection regression now uses the existing user-namespace root
pattern, so it no longer needs privileged selection to run.

Both new regressions compiled and failed against the original runtime. Four
independently compiled controls are detected: remove the send-clock recovery,
restore the serving caller's observation gate, remove the Progress wait guard,
and restore the older listener-adoption order. Every controlled source is
restored byte for byte before the final gates. The initial clock-fixture compile
errors are preserved separately; they are not counted as protective failures.

## Validation and limits

The final package gate passes 2,093 library cases, seven worker cases,
43 integration cases and 31 doctests: 2,174 total. The library declares
831 ignored cases and explicitly filters #331. All-target harness clippy with
warnings denied, formatting, lock and diff checks pass. All 109 selected process
cases pass with the corrected parent admin rebuilt in this checkout's target.
All 763 Rust, Cargo and toolchain source hashes match before and after these
gates. The focused listener family passed sixteen ordinary cases before the
final run. The new actual-process case passed after its compiled red reproduction,
and all four compiled controls failed their intended assertions.

The final process selection replaces the formerly ignored listener case with
the new held-reply case; the listener now runs in the ordinary package suite.
It otherwise preserves #372's demand-save, S9 and surrounding lifecycle cases,
including the eleven actual admin/worker/SQLite refusal cases. Nested child
summaries and overlapping selections are not additive distinct coverage.

Cargo uses the pinned toolchain, --locked, a private target and serial runs after
lock.sh. The known pending-author flake #331 remains explicitly filtered from
the package gate. #369 and #372 T8's scheduling-dependent wait-count assertion
remain carried. Manual consumer and Spec review was performed because the
required self-check skills are unavailable.

Foundation remains unselected. Planner review, operator merges, remaining
composition and qualification, security disposition and coupled cutover remain
owed. #371's timing proposal is not implementation authority. No broad #146
checkbox closes through this act.

The review exposed a second caller of the save owner that did not consume its
progress result, and a listener retirement entry that bypassed existing failure
recovery. Preserve the shared owners and test every changed entry order; a test
that makes all interests ready cannot prove held work progresses without a wake.
