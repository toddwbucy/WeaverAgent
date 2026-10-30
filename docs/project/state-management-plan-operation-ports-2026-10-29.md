# Read-only plan operation ports

**Version:** 0.2, 2026-10-29.
**Parent:** Issue 146, state management implementation.

Implements: state-management-spec-07-plan-forward-flags PLAN-PORT's assess_act
and work_reference operations, CATEGORY, WORK-REFERENCE, PLAN-PORT-REFUSAL,
PLAN-PORT-RECORD and the read-only PLAN-STATES rows, weaver-harness-Spec-amendment
HARNESS-PLAN-PORTS, weaver-types-Spec-amendment TYPES-PLAN-PORT-REFUSAL, and
weaver-trace-Spec-amendment's existing refusal carrier.

This slice is on code/state-plan-operation-ports, based on draft #404 at
f832c4c565862b0cca160150c79bf224abefbf3c. Merged #384 supplies the authority.
It advances #146 section D without closing its broad plan or selection row.
The remaining activation, review, closure and break operations are separate acts.
The existing worker driver remains selected. Do not deploy or activate the
foundation dispatcher from this slice.

## Checked operations

assess_act retains the requested plan revision, step, StatementRef and exact
PlanAct. It consumes the committed plan proof and a released, recorded statement
read. It compares the held binding's verb and exact act against the active step's
operator category, and checks an external tool against the loaded grant. It
does not classify again or dispatch. AllowedAct is a private, move-owned proof.

work_reference requires the expected current revision, active step, that step's
span and the requested committed frontier. It hashes the complete retained
tagged SpanPreview bytes, including entries and terminal pairs. It does not
hash a projection or reserialize a parsed value. Public WorkRef metadata cannot
construct the private checked account used by later closure consumers.

Local foreign-seed requests refuse before a selector or cache proof is consumed.
The refusal preserves their original identities without granting authority.
A foreign or malformed read proof is a component failure. A missing locked plan,
wrong revision, inactive step or different step span is an operation refusal.
The cache must still be the same checked committed proof when success transfers.

The floor separates PlanOperationCase from the eight durable PlanBreakCase
values. The exact required-nullable read_code follows the selector/leg table.
The strict codec rejects incompatible cases, missing or extra fields, non-string
codes and a missing step for any operation except record_break. The three new
operation cases cannot enter a stored PlanEffect::Break or other break carrier.

## Ownership and continuation map

| Surface | Owners and consumers |
| --- | --- |
| PlanRequest | Explicit compiled-loop API. Admission returns the exact unaccepted request and turn on Busy. No gate or model constructor creates a private grant. |
| MarkOperation::Port | One mutable plan transaction and allocator slot. The context retains request, original Decision, consumed cache/read proofs and cancellation account. |
| ReadPurpose::PlanPort | Exact selector and accepted nullable turn under the owning stage and original bound. The common read owner retains paging, release, measurement and Recorded evidence. |
| AuthorClaim::PlanPort | Private identity, one event and exact ticket. Ordinary recording and trace-only halt binding preserve its own completion and first failure. |
| FoundationPlanOperation | Move-owned result holding the private proof/account. Public accessors expose request and outcome views. Later effect consumers must check context, current revision and exact operation again. |
| Worker progress and admission | Input drives the pending operation, while flags, root updates and saves cannot overtake its active ownership. An untaken completion stays owned. |
| Stop and Leave | Management cancels before consuming the directive, drains admitted work, and revokes a completed but untaken result. It grants no successful continuation. |
| Root admission refusal | Drives the earlier plan operation's original continuation, including when a save is already queued, before recording the competing refusal. |
| Rollback and failed exit | Retain the exact pending/completed account through their consuming handoffs. Failed cleanup records owed evidence through the existing trace-only route. |

The worker arms Decision only if its caller has no interval. It retires only
the exact interval it created. Borrowed Decision remains the caller's interval.
Read, release and refusal accounting never renew it. Cancellation drains an
already acquired snapshot, retains any chosen refusal, and creates no effect.

A read refusal becomes an operation refusal only after its finalized read
account exists. The refusing leg's measurement remains causal_parent even
after release records a later measurement. Local judgments use the latest
consumed Recorded read proof, including the held cache. Only a judgment before
any consumed proof may have no parent. The operation returns its typed refusal
only after the exact event is Recorded and its completion consumed.

## Evidence and limits

The focused owner suite covers foreign identities, unusable targets and cache
parents, all admitted initial resource/domain and acquired-next codes, refused
release, exact statement identity/category/tool checks, nonempty work previews
with terminal pairs, cancellation, Busy, expiry and retained first failure.
Compile-time pins exclude Clone for the private grants and completion. The public
result also has a compile-fail Clone example.

Twelve process cases use an entered worker and actual SQLite member: owned and
borrowed Decision, Busy, Stop under recorder pressure, Leave through actual
Lower/rollback, member EOF through failed exit, root refusal with and without a
queued save, successful active-plan assessment and work construction, wrong
category and unknown tool. The four active-plan cases create the plan through
the real root update, read its successor and marks, file activation, refresh
its summary and update native scopes before checking the public ports.

Activation is fixture setup through existing owners, not a completed public
activation port. The model/classifier and gate sides retain test adapters.
These cases do not qualify production language artifacts, hardware limits,
serve-side selection or the complete coupled cutover.

At 804eb301, author gates pass 2,257 harness tests/doctests and 262 config-feature
types tests/doctests, strict all-target lint for both affected crates, lock,
format and diff checks. Harness reports 961 ignored cases and explicitly skips
the known #331 test,
a_real_pending_trace_author_does_not_reduce_or_replace_packet_completions.
Counts exclude nested child re-execution summaries. All 59 worker-plan process
cases pass together with 64 test threads, including the twelve new cases.

Twelve independently compiled controls fail their intended behavioral checks:
category, tool grant, full-preview hash, waiting-step admission, revision,
read code, cancellation, borrowed Decision retirement, Leave custody, failed
custody, root-refusal continuation and trace-only ticket binding. The waiting
and revision controls introduce a forbidden read and fail the bounded local
completion check. The others fail their result or custody assertions. Every
source is restored, and the seventeen owner tests and twelve process cases
pass afterward. All 831 Rust/configuration hashes match both control cohorts,
restored checks and final gates.

Private target: /tmp/weaver-state-plan-operation-ports-target. Toolchain:
nightly-2026-02-13, CARGO_INCREMENTAL=0, Cargo dependency operations --locked.
Process cases run in an isolated delegated host scope on Olympus. The final
source archive, scripts, manifests, logs and failed attempts are deposited at:

`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-operation-ports-20261029`

The deposit binds the full tracked/untracked source and all tested Rust/config
hashes. Its manifest checksum is recorded in the PR body. The preliminary
worker deposit at plan-operation-ports-worker-20261029 preserves the earlier
development failures. It is not the final test cohort.

Development failures are retained in the evidence. The first active-plan
fixture used an assistant-role turnless effect, which trace correctly rejected.
Its first turn-start correction incorrectly used loop origin and was also
rejected. The corrected fixture records runtime turn.started and files the
user-role activation template with that turn. Production validation was not
relaxed. Earlier worker-fixture failures and their dispositions remain in
the preliminary worker evidence deposit.

The final worker-control runner stopped after its six controls and complete
source restoration. A host process check found no live runner or build. All
source hashes matched before resuming only restored compilation/tests and
the final gates. The first broad family run passed 58 cases and failed the
admin circuit because the runner omitted its test-binary environment. Building
the admin test binary from this tree and supplying its path yields 59 passes.
Both attempts remain in the deposit, and the final script carries that setup.

The code-review and verification-before-completion skills are unavailable in
this session. Manual author reading and executed gates are not skill passes.
Planner review remains required. No CodeRabbit request or deployment occurred.

## Planner correction on PR 406

[Review 5705372425](https://github.com/toddwbucy/WeaverAgent/pull/406#pullrequestreview-5705372425)
grades 804eb301 three fix, all missing protective tests. The correction changes
test fixtures and this record only. Runtime code remains byte-identical to that
reviewed head. Merged Spec authority and the two public operations are unchanged.

| Finding | Correction and independent control |
| --- | --- |
| FX1, S2 | The loaded fixture scheme admits read and write. The active category admits read and the requested Statement act. The actual released statement binds Write file. to write, so assessment must record wrong_category. Removing only the verb comparison grants Allowed and fails this case. |
| FX1, S4 | The actual load grants sh, while the active category admits the exact external_tool curl act and the held read verb. Assessment must record unknown_tool. Removing only the tool-name check grants Allowed and fails this case. |
| FX2, S7 | The requested local span is the open identity-prefix span from boot, not the active step's span. The operation records invalid_target with null read_code and the cached read parent, with no state read or new recall. Removing only the span comparison admits a read and fails immediately. |
| FX3, S19 | The real worker completes assess_act and retains Allowed untaken. Actual Leave and Stop packets each return the same input with a Cancelled result through the public take. Removing only the completed-result rewrite fails both cases, while pending context cancellation remains intact. |

Leave uses the real deferred graceful handoff and retains its original directive
and later cleanup duty. Generic management finish does not consume a pending
Leave. The additional Stop case uses finish after the actual AtRest answer.
Before Stop, the fixture records a clean close of the turn used for activation.
That turn has no world request or turn owner. This case pins idle Stop's untaken
grant cancellation, not live-turn Stop execution.

All twenty owner cases and fourteen worker-port process cases pass before and
after four separately compiled controls. Each control fails its intended
assertion, and every source hash restores. Final affected gate results are
2,260 harness tests/doctests, 61 worker-plan process cases with 64 threads,
strict harness all-target lint, lock, format and diff checks. The ordinary
harness run reports 963 ignored and explicitly skips known #331. Counts exclude
nested child summaries. All 832 Rust/configuration hashes match controls,
restored checks and gates. The unchanged types gates remain scoped to 804eb301.

The initial compile caught two fixture field-access mistakes and an unreachable
match arm. The first process run passed Leave but correctly refused Stop on the
fixture's ownerless open turn. Recording the actual clean close before idle Stop
corrects the setup without changing runtime code or the original bounds. Those
failed attempts remain in the deposit.

The correction deposit is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-operation-ports-review-20261029`.
The PR body records its manifest checksum and exact gate counts. Earlier evidence
above remains scoped to 804eb301. Planner regrade is required before the fixes
are called review-cleared.

The review's batch findings remain open under #146 and the linked review:

| Carry | Next duty |
| --- | --- |
| B1 | Add defensive uninitialized-cache admission refusal during owner integration. Boot already initializes the worker's cache. |
| B2 | Obtain the operator's election on canonical tagged result versus released result bytes before closure consumers land. No hash-definition change is made here. |
| B3 | Preserve an already Recorded refusal on cancellation while revoking grants. |
| B4 | Resolve category/tool refusal precedence in the owning Spec and implementation. |
| B5 | Consolidate the duplicated loaded tool lookup. |
| B6 | Audit and consolidate owner-pending predicates before effectful activation and closure owners are published. |
| B7 | Simplify duplicated cancellation state and admission-enum mapping. |
| B8 | Reconcile the read-stage Progress/Pending convention. |
| B9 | Pin foreign-span-only local refusal and wrong-turn admission. |

These carries are not closed by adding FX1-FX3. Production qualification and the
remaining #146 implementation still stand outside this read-only slice.
