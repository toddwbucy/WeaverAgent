# Explicit plan-flag observation composition

Implements: state-management-spec-07-plan-forward-flags PLAN-FLAG-READ and
PLAN-OBSERVATION's flags variant; weaver-harness-Spec-amendment HARNESS-READ,
HARNESS-PLAN-OBSERVE's explicit flag sampler, HARNESS-CLOCK and HARNESS-PUMP at
its worker join; HARNESS-SAVE-TIME at the queued-save join.

The code/state-plan-flag-observation branch is stacked on draft #381 fcbb76ef.
This act advances #146 section D's plan/forward-flag consumers and section E's
recording and lifecycle composition. It closes no broad checklist item.
The existing worker driver and admin parser remain selected. Foundation
activation still requires the reviewed, qualified coupled cutover.

## Operation and ownership

The compiled loop explicitly calls FoundationInput::observe_flags(plan, after).
Admission returns the exact attribution on Busy, WrongPhase, InvalidAttribution
or Stopped. Loaded seed, positive plan/revision and source identity checks
precede the read; the shared reader's existing phase and exclusive-owner
guards remain authoritative. This port starts no workflow or plan transition,
opens no pair, mutates no store, and grants no content placement or generation.

The existing read transaction retains its flag purpose through bounded paging,
release and the successful recall record. The result's plan/after echo and
checked count partition are validated before exposure. The final
state.plan.measurement uses plan-observation/1 and the initial read request ID,
never a next-page or release ID. Success carries the exact snapshot. A valid
typed read refusal carries unavailable and its original code, never fabricated
zero counts or a rollback. Broken seams retain the foundation failure rule.

The measurement is parented to the successful recall, or to the original
refusing operation measurement after required cursor release. Pending trace
capacity retains the immutable event under its original ticket. Only Recorded
completes the read and releases its original absolute bound. Cancellation
drains the same exchange and records the owed sample; the caller receives a
cancelled account with no usable observation.

The serving seat has one completed result slot. An untaken result makes another
explicit sample Busy. The move-owned public result retains the read and release
account and exposes its observation, initial request ID and trace sequence.
Generic read reduction cannot take this owner. Pending and failed work remain
with the recording context; failed exit and entered rollback retain any
completed account with the original serving resources.

The worker advances the same reader through its existing pump and yields when
the sample completes. Stop and Leave cancel and drain it before their own
continuation. Observe still reads the committed plan summary: sampling does
not invalidate that cache. A queued on-demand save keeps its original peer and
window while the read finishes; sampling cannot start its capture boundary.
No extra reader queue, state connection, recorder or operation clock is added.

## Author evidence and limits

Seven private reader cases cover paging, exact release, initial and next-page
refusals, cancellation, trace pressure, original identity and deadline, foreign
attribution, malformed echoes/counts, and generic-read exclusion. Case matrices
exercise multiple typed refusals and cancelled/noncancelled continuations.

Seven root process cases compose a committed plan through the real SQLite
member and sample through the worker: success, unavailable source, Stop,
complete Leave rollback, failure during the read, failure after Recorded, and
an on-demand save queued while the sample is pending. The delayed reply uses
the real blocking pump with a CPU bound. The actual SQLite success observes
zero held flags; the scripted paged reader cases check nonzero matched and
unmatched partitions. Test classifiers and template certificates are fixtures.
These tests do not qualify production language artifacts or choose sampling
policy for a loop.

Development failures remain in the evidence: test type/visibility corrections,
an absent-source assertion corrected from invalid_selector to not_found, the
gate fixture extended to acknowledge the newly exercised Lower, and a queued
save fixture given a declared real-process budget instead of its millisecond
fake-clock allowance. Strict lint caught the completed account enlarging the
serving seat; the single result slot now owns a Box, including both cleanup
handoffs. None of those development runs is presented as a final pass.

The final serial harness package passes 2,205 tests: 2,122 library, eight worker,
43 integration and 32 doctests. Nested child summaries are excluded. The package
invocation leaves 913 tests ignored and explicitly skips known #331
(a_real_pending_trace_author_does_not_reduce_or_replace_packet_completions).
It takes 1,057.12 seconds on this host. Separate actual-root runs pass 54 distinct
identities: 20 worker plan cases (13 inherited root-update and seven new flags),
14 Observe, 17 Stop and three management cases. These do not establish a pass
for all remaining ignored instruments.

All ten compiled protective mutations fail as intended: six reader checks and
four worker checks. They cover initial request identity, completion only after
the flag measurement, typed unavailable versus invented zero, original deadline,
cancelled custody, loaded seed, worker reduction, Stop cancellation, failed-result
custody and the single untaken result. The worker-reduction mutation fails the
fixture's secondary-management progress assertion after five seconds; it is not
presented as a typed runtime refusal. All 808 Rust/configuration hashes restore
exactly, and the seven reader cases pass after the restored rebuild.

Lock, formatting, strict all-target harness lint, diff checks and the three
DocsMirror tests pass. The pinned nightly-2026-02-13 toolchain and --locked use
CARGO_INCREMENTAL=0 and private target
/tmp/weaver-state-146-serving-organ-loss-target. The final gates retain the same
808 Rust/configuration hashes. This slice touches only the harness crate.

The exact source, tracked patch, untracked files, gate logs, mutation evidence
and failed development runs are deposited with verified SHA-256 hashes at:

`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-flags-composition-20261029T034838Z-ffe231b5`

The deposit names the fcbb76ef base and tested uncommitted diff; the publication
receipt names the resulting commit and draft PR. The earlier private WIP
cohort remains separately identified and is not added to final test counts.

The code-review and verification-before-completion skills are unavailable in
this session. Manual author review and executed gates are author evidence,
subject to Planner review; they are not a security or production grade.

## Remaining work and review carries

PLAN-PORT transitions, WorkRef and review/closure grants, next_ready and
fit_refused observations, serve-side selection, the complete author/retirement
map, production language artifacts and measured coupled cutover remain in #146.
The merged #377 dictionary and rule walk remain candidate inputs.

The #381 Planner review 5692337515 reports zero required fixes. Its X1 admin
identity/deadline test gaps, X2 worker busy-admission case and W1-W7/W9 batches
remain carried, with W8 declined against PLAN-PREFLIGHT-READ. This observer act
does not claim to close those unrelated root-update review items.

## After-action review

The planned slice was the explicit read/measurement owner joined to its worker
and lifecycle consumers. Real-member tests distinguished an absent source from
a stale source and exposed missing fixture support for the cleanup paths now
exercised. The queued-save check kept sampling inside the existing save policy.
Continue declaring real-process fixture budgets before timing tests, retaining
failed runs, checking exact cleanup custody, and using compiled mutations before
publishing a slice. Production activation remains a separate reviewed act.
