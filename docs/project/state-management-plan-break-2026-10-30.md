# Checked plan break recording

**Version:** 0.2, 2026-10-30.
**Parent:** Issue 146, state management implementation.

Implements: state-management-spec-07-plan-forward-flags PLAN-PORT's record_break,
PLAN-BREAK, PLAN-PORT-REFUSAL, PLAN-PORT-RECORD, the break/cancellation PLAN-STATES
rows and PLAN-TRACE's break carrier, and weaver-harness-Spec-amendment
HARNESS-PLAN-PORTS and HARNESS-PLACE native adoption after the effect.

This slice is on code/state-plan-break, based on corrected #407 at
a1a669ccfd126763fbe494bf5426e05d8e28d805. The merged #384/#387 text is authority.
It advances #146 section D. No broad item closes. Existing serving remains
selected. Do not deploy or select the foundation dispatcher from this slice.

## Behavior and ownership

record_break retains the exact expected plan, optional step, case and proposed
source. A checked locked plan requires the current revision and exact optional
active step. The accepted turn supplies attribution for the classified effect.
A missing turn returns original admission custody without starting an owner.
The port never chooses a recovery, dispatches, closes marks or generates text.

The compiled plan.break family supplies the effect's content. The private
PreparedPortEffect retains context origin, typed effect, causal parent, turn
and the original Decision. Activation and break share source expansion,
filing, mandatory summary, native adoption and template-fault settlement.
Complete filing checks keep the exact source, classification, disposition,
receipt identity and bound. No second author or allocator can enter this owner.

A committed effect owes a fresh checked PlanSnapshot. Its expected plan,
Broken status, optional step, case, proposed source and break source must agree
with the effect and last source. A refused or inconsistent refresh latches
foundation failure and yields no stale result. The native cache then adopts
that same committed summary through its private receipt identity. Break leaves
marks open, but that does not permit the native cache to keep an older plan.

Only after adoption does the owner author plan.break, with the effect's source
sequence as parent. It retains the exact event and AuthorClaim::PlanPort ticket
through recorder pressure. Recorded completes the report. The caller receives
the committed snapshot only after these duties settle.

With no locked plan, the same operation records only plan.break. Its plan field
is null, while step, case and proposed source retain the request's values. The
held checked cache provides the parent. No new read, effect or status change
occurs. The returned current snapshot is absent for NoPlan and remains the
actual unchanged snapshot for an existing nonlocked plan. No placeholder plan
is fabricated.

Cancellation before effect admission authors no effect. After admission the
owner settles the real disposition. A committed break still owes summary,
native adoption and its report even if the caller receives Cancelled. Correct
template rejection records the existing fault and then the exact operation
refusal, without a break report. Original deadlines and first failure remain
held through Stop, Leave and failed exit.

| Surface | Owner and continuation |
| --- | --- |
| FoundationInput::record_break | Returns the unaccepted request on admission refusal, otherwise retains it in the existing operation account. |
| PreparedPortEffect | Private context origin, effect, certified expansion family, parent, turn and original bound. |
| EffectFiling | FilingOwner::PlanPort keeps the original event and complete correlated source dispositions. |
| EffectFiled and Summary | Same owner keeps the receipt through the mandatory checked plan refresh. |
| Native | Receipt-bound native adoption installs the exact committed summary while retaining open marks. |
| BreakReport | One PlanPort author claim and event/ticket survive pressure and cancellation. No result precedes Recorded. |
| TemplateFault and Refusal | The shared effect owner records the fault, then refusal with the fault parent. No refusal implicitly requests a break. |
| Input and management | The existing shared pending predicate covers break. Stop/Leave settle it before the management handoff. |
| Save, root refusal and failed exit | Existing pending-owner continuations retain the same operation. Failed/rollback consuming handoffs retain its original account. |

The new surfaces are PlanRequest::RecordBreak, PlanPortCompletion::BreakRecorded,
FoundationPlanOutcome::BreakRecorded and Stage::BreakReport. Every request
identity/operation accessor, result accessor/account consumer, worker outcome
match, effect driver and author-claim match is updated. The existing public
take and cancellation paths remain common. No wire schema or trace kind changes.
All eight break cases are explicitly mirrored into the trace-owned carrier.

## Validation and source scope

The owner instruments use actual recorder/read/filing owners with scripted
member replies. Ten new cases cover report-only behavior, exact target and
revision, missing turn, the active/no-active committed-cancellation matrix,
pre-source cancellation, template refusal, post-commit refresh refusal and
mismatch, the actual Authorized snapshot and report ticket custody. The report
pressure matrix crosses committed/report-only paths with completion,
cancellation and expiry. Original event/ticket and Decision stay identical.

Six new process cases run the entered worker and normal SQLite member. A real
checked world turn supplies attribution. A byte-preserving proxy holds the
actual effect receipt while Stop, Leave or member EOF arrives. Stop/Leave keep
the source and world request through receipt, summary, native adoption and
report settlement. Failed exit and rollback retain their consumed operation
accounts. The active success case calls public activation, then public break,
and verifies the returned Broken plan, unchanged steps, actual open marks,
ready native cache and exact report. Gate/classifier/model sides remain test
adapters, not production artifacts.

The implementation commit dc107eb8 passes 2,286 ordinary harness tests/doctests,
all 70 worker-plan cases with 64 threads, strict all-target harness lint, lock,
format, diff and three DocsMirror checks. The ordinary suite reports 972 ignored
and explicitly skips known #331's
a_real_pending_trace_author_does_not_reduce_or_replace_packet_completions.
The restored focused suites pass 46 owner and 23 worker-port cases.

| Independent compiled control at dc107eb8 | Protective failure |
| --- | --- |
| Remove the active-step guard | Exact optional target/revision case fails. |
| Refuse instead of recording with no locked plan | Report-only case fails. |
| Accept changed break metadata in the refresh | Changed case/proposed-source case fails. |
| Skip native adoption | Actual worker active success case fails. |
| Skip the report author's advance call | Report pressure/ticket case fails. |
| Drop the report's effect parent | Committed-cancellation matrix fails. |
| Skip committed settlement after cancellation | Actual active Stop case fails. |
| Omit BreakReport from the author claim | Report pressure/ticket case fails. |
| Drop the failed-exit operation account | Actual member-EOF case fails. |
| Drop the rollback operation account | Actual active Leave case fails. |

All ten controls compile independently and fail their intended assertions.
Source hashes restore exactly and match the implementation's full gates.
The initial source has 1,182 files. The join with #407's test correction changes
only the activation effect-test module declaration and parent project record,
and adds the cancellation test file. All runtime and break-test files remain
byte-identical. Both parents remain ancestors of merge 9799afb6.

The joined source passes 2,288 ordinary tests/doctests and all 70
worker-plan cases, with the same strict gates and ignored/skipped scope.
A13 and A14 are independently compiled against the renamed shared effect owner.
Each new activation cancellation case catches its own missing guard, followed
by restored full gates. These two controls use the joined source. The earlier
ten retain their dc107eb8 source scope and are not falsely claimed rerun.
The final archive adds only this record after the joined gates. The deposit's
manifest reconciles both test cohorts and the archived source.

All commands use pinned nightly-2026-02-13, CARGO_INCREMENTAL=0, --locked and
the private target /var/tmp/weaver-state-plan-break-target. Process cases run
under root-delegated transient scopes with the independently built admin test
binary and normal SQLite member. Command arrays and logs are retained.

Evidence deposit:
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-break-20261030

## Planner FX1 correction

Review 5706764287 reads the runtime correct and requires one missing test.
The earlier record-before-result control skipped the entire author advance
call. K10 preserves that call but ignores its false result when another author
owns the slot. The prior report-ticket pressure matrix did not pin this case.

The new test admits a measurement through the real recorder while its queue is
full. Both the committed-effect and report-only paths reach BreakReport with
that earlier author pending. Three advances retain the exact earlier event and
ticket, the original Decision, no report ticket or sequence, and no completion.
After pressure drains, the earlier ticket reaches Recorded. Only then does the
report reach Recorded and the operation return BreakRecorded. The test checks
the distinct tickets and the sequence order before taking the result. This is
an owner instrument using the context's recorder admission, not a new claim of
worker scheduling coverage.

K10 compiles independently and fails the new Pending assertion with Complete.
Restoring the source passes the test, 2,289 ordinary harness tests/doctests and
all 70 worker-plan process cases. The suite reports 972 ignored and skips the
same known #331 case. Strict harness lint, lock, formatting, diff and three
DocsMirror checks pass. No runtime file changes. Original controls and types
results retain their earlier source scope. Exact hashes match the control
restoration and final gates. Only this record changes after those gates.

The first test run compared the full stage description with a bare stage name.
Using the existing stage-prefix convention fixes that fixture error. Both the
failed attempt and final source snapshots remain in the supplemental deposit:

/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-break-fx1-20261030

The correction awaits Planner regrade. B1's separate Broken-status instrument
remains carried. B2-B4 require operator elections about case/state agreement,
cancellation of an unstarted report and the nonlocked report's plan reference.
Their behavior is unchanged. B5 retains the Planner's declined disposition:
the effect expectation fixes last_source to this filing's exact source, and
the break source must equal it. B6-B8 remain a batch for shared-owner cleanup
before further effect kinds. B9 retains the UTC-date disposition. All are
tracked by #146 and the linked review.

## Development findings and carries

The first compile exposed three old activation method names and an Option
accessor mistake after the shared-effect refactor. The first owner run then
failed four cases because the fixture retired Enter before taking its template
certificate. Taking the real certificate before retirement corrects the setup.
Further test-only compile failures confused PlanSummary with PlanSnapshot and
needed an explicit sequence conversion. The corrected owner, process, control
and gate runs pass. All failed attempts remain in the deposit.

#407 FX1 is carried in through its exact correction and rerun against the shared
effect owner. Planner regrade 5706589894 accepts it with zero fix. Its B1/B2/B5/B6/B7
batch items and B3 optional hardening remain carried, with B4 declined by the
Planner. #406 B1-B5/B7-B9, #402/#405 and inherited ownership, security,
measurement and qualification duties remain tracked in #146. The operator
merged #408 at cdcc391e, ratifying its WorkRef byte definition. The closure act
still owes D1's real served-preview, store recheck and dump/load verification.
This slice changes no WorkRef hashing.

Review/closure ports, next_ready/fit_refused, selection, Diagnostic interruption,
production language artifacts, prompt/template certificates, integrated limits
and qualification, and the coupled cutover remain unfinished. The code-review
and verification-before-completion skills are unavailable. Manual author
reading and executed gates are not skill passes. Planner regrade is owed.
No CodeRabbit review, deployment or full-goal completion is claimed.

## After-action review

Sharing the activation effect owner kept break's source, receipt, refresh and
cancellation sequence in one place. The committed summary still had to reach
the native cache even though break closes no mark. The active worker test and
compiled omission pin that distinction. The report is another owed author
duty, so its pressure matrix checks the actual ticket and original bound
instead of treating the filing receipt as completion of the whole operation.
FX1 adds the earlier-author wait separately from pressure on the report's own
ticket. A protective control must keep the exact conditional being claimed:
skipping a whole call does not establish that its false-result branch is pinned.
