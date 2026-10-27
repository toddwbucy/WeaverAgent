# Repair refusal decision budget

Date filed: 2026-10-27. Author record for issue #146.
Branch: code/state-repair-refusal-budget.
Base: code/state-review-race-fixtures at ac95d489 (draft #367).

Repair refusal and exhaustion keep the original Decision interval through their
remaining account and refused close. This answers #364 Planner Q1, adds the
caller census requested in Q2, and pins pre-election expiry from Q4. Foundation
remains unselected. Do not deploy the incomplete replacement.

## Authority and behavior

Implements: State Management Spec 02 section 5 DECISION-BARRIER and the harness
amendment's HARNESS-CLOCK. Author assessment: conforms on these two refusal
roads and their close continuation. The existing Spec governs this correction.
No public type, wire field, declared limit or policy election changes.

Exhausted readiness, exhausted extraction and refused readiness no longer call
finish_repair_wait. Returning either account transfers its original Decision
interval to the caller. Terminal accounting, trace close and world delivery
consume that same interval. They cannot obtain a fresh interval after extraction.
Successful repair, restatement handoff and cancelled settlement retain their
existing retirement points.

OperationRefused is an intermediate state. Its original received input and
emission remain owned while the caller elects the close. Observe does not renew
the deadline. Advancing at expiry before that election latches DecisionBound,
keeps the open turn and emission, and permits no turn.closed or world delivery.

## Owner and caller census

| Site | Disposition |
| --- | --- |
| RepairIntent::Exhausted | Preserve the original bound at readiness. Completed original-request accounting does not complete the world close. |
| take_repair_exhausted | Return the account once and retire the repair owner without retiring Decision. |
| advance_refused_repair / take_refused_repair | Settle every owed correction record before readiness. Extraction preserves the original bound for remaining original-request accounting and close. |
| EmissionWork::advance, Repair | The only non-test extraction consumer. Retain the exact refused or exhausted account in EmissionWork::Refused. |
| FoundationInput::close_operation_refusal | Elect that emission's existing CancelAccount, Pairs and RefusalClose continuation. Every wait uses the retained interval. |
| Workflow::advance, OperationRefused | Elect close_operation_refusal immediately. Session retains this Workflow and supplies no separate interval. |
| worker::repair / worker::generate_correction | These primitive adapters can return an intermediate OperationRefused input. The caller still owns its live deadline and must elect close, cancel or keep advancing. An accessor alone is not a clock observation. |
| EmissionWork::finish_accounting / WorldClose | Ordinary refusal retains Decision through trace close and gate delivery. Completed world delivery retires it. Cancellation keeps its existing owner transfer. |
| WindDownWork / finish_wind_down_pass | This second EmissionWork consumer elects the same refusal accounting before PassComplete. record_turn reuses Decision and retires it after the closing TurnClosed is Recorded. The independent WindDown bound also remains in force. |
| Successful repair, effective extraction, restatement grant and cancelled extraction | Keep their existing retirement rules. They do not owe this ordinary refused-close continuation. |

The new surface is a live Decision interval after repair-account extraction,
including while OperationRefused awaits its caller. This matches the outbound
roads corrected in #364. Issue #365 retains the separate Stop-after-accounting
policy question. This act makes no election for it.

## Evidence

The ordinary harness package run passed 2,163 top-level cases: 2,084 library,
7 worker binary, 43 integration and 29 documentation cases. The library declared
808 ignored process instruments. The known pending-trace-author pump flake
tracked by #331 was explicitly filtered. This is not an unfiltered suite pass.
All 740 worker process identities passed, including six root-only joint cases
run in a delegated scope with the compiled admin artifact supplied after sudo.

The eleven new process identities cover repair refusal through a withheld
terminal receipt, trace pressure and gate expiry, and exhausted repair through
trace pressure, trace expiry and gate expiry. Five cases pin expiry before close
election across refused repair, exhausted repair and all three outbound exits.
They preserve the original received-input allocation through Observe and verify
the first failure and the absence of a completed close.

The initial targeted unit run found two assertions that expected a retired
repair interval. Both now require the original interval after extraction. The
exhaustion case verifies the completed pair before expiry, then verifies the
DecisionBound refusal at the deadline. The broad run includes these corrections.

Three independent compiled mutations restored the early retirement at exhausted
readiness, exhausted extraction and refused readiness. Each failed its matching
process assertion because the expected original bound was absent. A separate
control disabled Decision expiry in both RecordingContext::live and the common
pump. All five pre-election cases then failed at OperationRefused versus Stopped.
Disabling the context check alone survived because the pump still enforced the
same deadline. That surviving control and all failed runs remain in the evidence.

After restoration and rebuilding, all 27 focused process cases and all 79 repair
unit cases passed. Formatting, lock and diff checks passed again. All 800 source
and configuration file hashes match the broad-run manifest.

The affected crate passed all-target clippy with warnings denied. Formatting,
lock and diff checks passed. Cargo used the pinned toolchain, --locked, serial
invocations and the existing private target for this checkout. Source manifests,
logs, scripts and the compiled controls are retained for the operator.

These are author checks with scripted model and language replies. They do not
provide a Planner grade, security disposition, artifact qualification or approval
to select the foundation dispatcher. The named self-check skills are unavailable
in this environment. The author manually reviewed the changed transitions,
remaining retirement sites and every non-test extraction and close consumer.

## Remaining work and author note

Issue #146 stays open. #367's fixture review batch, #365's operator election,
#348/#349's capture-refusal S4-S7 composition, remaining lifecycle and author
handoffs, production dispatcher integration, measured artifacts and qualified
coupled cutover remain separate work. No broad completion box closes here.

The planned change was to remove the two repair roads' second interval. The
implementation removes three retirement calls because exhaustion retired both
at readiness and at extraction. Reusing the close fixture required accounting
for exhausted repair having already filed its original terminals. The caller
census also includes wind-down, whose close retires the inherited wait.
Future expiry controls must account for both context and pump enforcement before
treating a surviving single-site mutation as an untested deadline.
