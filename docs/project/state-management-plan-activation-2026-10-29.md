# Checked plan activation

**Version:** 0.1, 2026-10-29.
**Parent:** Issue 146, state management implementation.

Implements: state-management-spec-07-plan-forward-flags PLAN-PORT's activate
operation, PLAN-PORT-REFUSAL, PLAN-PORT-RECORD and the activation/cancellation
PLAN-STATES rows, state-management-spec-06-marks-renderer-engine STEP-QUERY
and MARK-READ, and weaver-harness-Spec-amendment HARNESS-PLAN-PORTS and HARNESS-PLACE
native-scope adoption after a committed plan effect.

This slice is on code/state-plan-activation, based on #406 at
64b7f86f088ffda14384181ade052e47ce1b1bd8. The merged #384 and #387 documents
are authority. It advances #146 section D and answers #406 B6. It does not
close the broad plan or selection row. Review, closure, break, next_ready and
fit_refused remain separate work. The existing worker driver remains selected.
Do not deploy or select the foundation dispatcher from this slice.

## Behavior and ownership

step_successor returns a private, move-owned CheckedStepSuccessor only after
the exact paged read, release, measurement and recall complete. It retains the
RecordedRead. Public StepQueryRef metadata cannot construct the proof. A
standalone query refusal keeps its read code and detail, without an operation
refusal or a plan effect. A cancelled acquired query still releases its cursor.

activate consumes that proof and checks its recording context, current locked
plan revision, target instruction, first waiting ordinal, immediate closed
predecessor and absence of active or earlier unclosed work. Later preserved
closures do not incorrectly require a closed prefix. It reads the actual marks
catalog, checks the plan's task and open scope path, and allocates the step
and any new group from the checked counter. Allocation overflow records a
typed refusal before an effect is authored.

The loaded certificate supplies the fixed plan.activate family. Activation
retains its exact rendered event, classification and complete filing receipts.
A correctly rejected template records state_template_rejected and then the
operation refusal with the fault as causal parent. A missing certified family
takes the same fault/refusal road without inventing a source.

After a committed receipt, the owner refreshes the complete plan snapshot and
checks the committed active step and group/span identities. It then adopts the
native scopes through that same receipt identity. Only afterward can an
ActivatedStep transfer. Refused or inconsistent post-effect summary is a
foundation failure, not a pre-effect refusal or a stale successful result.

Cancellation before source admission returns custody without an effect. After
source admission it settles the actual disposition. A committed effect still
owes its summary and native scopes, even when the caller receives Cancelled.
All of this work uses the original Decision. The worker retires only a Decision
it created and preserves a borrowed interval. Fatal failure keeps the first
cause and original account for bounded failed-run cleanup.

| Surface | Owner and continuation |
| --- | --- |
| CheckedStepSuccessor | The actual released RecordedRead, consumed by Activate. Public metadata is inspection only. |
| MarkOperation::Port | One allocator and operation account, retaining request, turn, original bound, cache/read proofs and cancellation. |
| EffectFiling | FilingOwner::PlanPort admits only the driving activation with its original Decision. Unstarted withdrawal returns the exact event. |
| EffectFiled and Summary | The same owner consumes the complete filing into the mandatory effect summary, retaining its private receipt identity. |
| Native | The driving owner joins only its exact EffectPlanSummary and original bound. Native completion or cancelled custody stays in the operation account. |
| TemplateFault and Refusal | AuthorClaim::PlanPort retains each exact event and ticket. The completed fault account remains owned while its refusal is recorded. |
| Worker | Public query/activate APIs share input, management, save, root-refusal and teardown continuations. They select no workflow, reviewer or generation. |
| Stop and Leave | Management first cancels and settles the existing operation. The original input and completed cancelled account survive the consuming handoff. |
| Failed exit | Pending operation and filing identity survive halt, Lower and final teardown. No failed account yields an ordinary result. |

## Common pending-owner census

The shared plan_work_pending predicate covers root updates, explicit flags and
plan operations. Input progress, plan-at-rest admission, flag/port admission,
secondary receive setup, entered rollback, graceful handoff and the actual
save boundary use it. This answers #406 B6 before effectful consumers expand.
Per-owner drive and cancellation remain explicit. Completed untaken accounts
retain custody but have no pending I/O duty.

Capture delegation and queue_save intentionally retain their narrower root
exchange check. A save can queue behind non-root work. The actual capture
boundary uses the common predicate and cannot pass the unfinished operation.
Admission also retains each port's own untaken-result check.

## Evidence and limits

The owner cases use actual socket/trace/read owners with declared scripted
member replies. They cover released successor custody, initial/next refusal,
query cancellation, exact predecessor, foreign recording context, an already
active step, all three group allocation forms, counter refusal, pre-effect
cancellation and template-fault ordering. The committed-effect matrix crosses
ungrouped, new-group and existing-group activation with cancellation after
source, after receipt, after summary, and no cancellation.

The process cases run the entered worker and normal SQLite member. Existing
predicate cases now use public successor and activation before assessment or
work construction. New cases admit a real checked world turn, hold the actual
committed activation receipt in the byte-preserving proxy, then deliver Stop,
Leave or member EOF. Stop answers TurnAborted only after settlement and closes
the exact world exchange. Leave retains the cancelled account through actual
Lower/rollback. EOF retains the first failure, original operation and filing
identity through the complete failed exit. These are gate/classifier/model
test adapters, not production artifact or hardware qualification.

Final source gates pass 2,274 ordinary harness tests and doctests, all 64
worker-plan process cases with 64 test threads, strict all-target harness lint,
lock, formatting, diff checks and three DocsMirror tests. The ordinary harness
suite reports 966 ignored cases and explicitly skips known #331's
a_real_pending_trace_author_does_not_reduce_or_replace_packet_completions.
Counts use each outer suite's final summary and exclude child summaries.
The focused owner suite passes 36 cases and the worker-port suite passes 17.
Both pass again after the controls restore the source.

| Independent compiled control | Observed protective failure |
| --- | --- |
| Replace the immediate predecessor guard | Exact closed predecessor case fails. |
| Reuse the group counter for the step | Group allocation case fails. |
| Choose the older read parent | Ordered successor proof case fails. |
| Skip native adoption | Actual worker active assessment fails. |
| Skip native adoption after cancellation | Both actual activation Stop and Leave fail. |
| Drop the template fault parent | Classifier rejection fault/refusal case fails. |
| Accept another context's successor proof | Foreign proof case fails. |
| Allow another active step | Invalid-target refusal case fails. |
| Accept changed post-effect step state | Mandatory refresh failure case fails. |
| Omit plan operations from the common pending guard | Real Turn activation Stop case exposes Turn instead of Pending. |
| Drop the failed-exit operation account | Actual member-EOF case fails. |
| Drop the entered-rollback operation account | Actual activation Leave case fails. |

All twelve controls compile and fail their intended assertions independently.
All 1,177 baseline file hashes match before and after the controls, after the
full gates, and in the source archive. The project record is the only new file
added after those checks. No Rust source changes after the final gates.

The commands use nightly-2026-02-13, CARGO_INCREMENTAL=0, --locked and the
private target /var/tmp/weaver-state-plan-activation-target. The real worker
cases run in root-delegated transient scopes with the normal SQLite member
and independently built admin test binary. The complete command arrays and
logs are retained with the evidence.

Evidence deposit:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-activation-20261029

The deposit contains the final source, source hashes, gates, all control
attempts and their scripts. Its verified manifest is linked from the PR body.
Earlier checkpoints remain separately deposited and retain their own scope.

## Development findings and carries

The initial activation join exposed an owner-census collision: native adoption
was refused by its own allocator. The receipt-bound continuation admits only
the exact driving owner. A cancellation test then reproduced a stale native
cache after an effect committed and its summary refreshed. Cancellation now
finishes native adoption while retaining a cancelled account. The compiled
removal of that correction fails the real Stop and Leave cases.

The first worker interruption fixture compile used sibling-private fields and
the public summary's nonexistent steps view. The fixture now uses test-only
account accessors and the actual retained full snapshot. Production validation
was unchanged. The first control-runner preflight had an indentation-sensitive
anchor and stopped before mutating source. All failed attempts remain in the
evidence deposit. The first pending-owner control targeted an idle Waiting
case, which already reports Pending, and survived. The same control
fails the actual Turn-state activation case, exposing Turn when its guard is absent.
Strict lint then required boxing the fixture effect account and replacing an
is_some/unwrap pair with if let. Both changes are test-only. All twelve
controls were rerun on that final source before its gates. Earlier activation
checkpoints remain scoped to their own source snapshots.

The code-review and verification-before-completion skills are unavailable in
this session. Manual author reading and executed gates are not skill passes.
Planner review is required. No CodeRabbit review was requested.

#406 B1-B5 and B7-B9 remain carried. B2 requires the operator's hash-definition
election before closure consumers. This slice changes no WorkRef hashing.
#402/#405 and the older qualification/limit/security duties remain under #146.
No deployment, production qualification or full-goal completion is claimed.

## After-action review

The plan was to carry activation through actual receipt, summary and native
ownership before exposing the active step. The first positive cases passed,
but the admitted cancellation matrix found that the successful path's native
adoption was skipped after cancellation. The correction preserves committed
state while withholding continuation. Real worker Stop/Leave/EOF cases now
exercise those ownership joins instead of relying on predicate setup alone.
Retain that boundary-first approach for the remaining effectful plan ports.
