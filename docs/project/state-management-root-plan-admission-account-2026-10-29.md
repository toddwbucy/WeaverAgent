# Root PlanUpdate admission-refusal account

**Version:** 0.3, 2026-10-29.
**Parent:** Issue 146, state management implementation.

Implements: state-management-spec-07-plan-forward-flags section 2,
PLAN-ADMISSION-REFUSAL, weaver-harness-Spec-amendment HARNESS-PLAN,
weaver-trace-Spec-amendment TRACE-PLAN, weaver-types-Spec-amendment
TYPES-PLAN-CONTROL and weaver-types-Spec's LifecycleAsk listing.

This implements merged #387 on code/state-root-plan-admission-account, based
on main 7cdef6196f5d988b979df9c500710c153ab137f1. It answers #386's
runtime duty and advances #146 section D. The operator's merge ratified the
proposal. It does not close the broad plan, migration or qualification rows.
The existing worker driver remains selected. This act does not deploy or
activate the foundation dispatcher.

## Refusal ownership

A complete judged PlanUpdate refused at the busy-state, root-cause or clean
inner-admission boundary retains its exact received bytes and decoded packet.
RefusedPlanPacket cannot be used by the accepted RootPlanPacket API. The
rejected document is never parsed as a plan, filed as input, or retried when
the original operation finishes. Invalid framing and descriptors keep their
existing transport disposition.

One management owner keeps the chosen answer and a private trace claim. The
existing refusal event carries RefusalRecord::Lifecycle with the exact
LifecycleAsk::PlanUpdate and refusal. The claim is tied to its recording
context, payload and harness/runtime attribution. The envelope supplies the
current run and omits turn and causal_parent. An ambient open turn is not
borrowed. The recorder remains opaque and gains no floor dependency.

The claim is created before accounting begins. Its own PlanRefusal deadline
borrows the loaded decision_wait duration and starts at the retained monotonic
reading after receipt of the complete packet. It coexists with Decision,
read, save and other deadlines. It cannot renew or retire those intervals.
The exact claim consumes its completion after Recorded, admission of a
different claim cannot substitute for that receipt.

| Boundary | Owner action |
| --- | --- |
| Earlier original or author ticket blocks admission | Run only its original input, flag, root-plan, save, wind-down or Quiesce continuation as needed to free the slot. Keep the refusal original and interval. |
| Preflight returns the unaccepted event on an owned interrupt | Retain that original for retry after the interrupt's owner settles. A ticket or fatal failure cannot take this route. |
| Recorder pressure | Resume the same claim and ticket. Stage no answer before its Recorded completion is consumed. |
| Requester EOF before Recorded | End delivery, retain the accounting duty and its original interval. |
| Accounting interval expires before Recorded | Latch DecisionBound with the original wait facts and retain the packet, answer, original or ticket and first failure through the failed handoff. |
| Interval expires after Recorded but before caller completion | End delivery and let the exact claim consume its already recorded completion. Do not create another event or halt solely for the lost answer. |
| Answer socket is full | Use only the remaining original interval. Delivery expiry ends the exchange without retry. |
| Answer was sent before expiry, reduction runs later | Retain Answered using the matching send's observed timestamp. |
| Clock fails after send | Retain the observed send and latch TraceFailed, never retry the answer. |

The management failed handoff now retains its refused packet and author account
beside the retired peer and selected answer. A save timeout is reduced by its
original owner without dropping the secondary refusal account. Completing or
dropping delivery retires only PlanRefusal. The active turn and committed plan
cache remain the original owners' state.

## Floor reconciliation and caller census

Main already contains LifecycleAsk::PlanUpdate and its strict codec from #381
at fcbb76ef. The older #387 proposal and #386 text called the variant missing.
That source claim is superseded. This act tests the existing carrier through
RefusalRecord and OrganEnvelope and aligns the base Spec's enum listing with
both existing PlanUpdate and WorldRequest. It introduces no wire variant.

| New surface | Complete caller/consumer set in this act |
| --- | --- |
| RefusedPlanPacket | Busy management and invalid-cause takes, inner refused RootPlanPacket conversion, management delivery identity, FailedManagement retention. It has no conversion back to an accepted packet. |
| PlanRefusal | Constructed only by the three rejected root-update branches, advanced by the original management owner, transferred intact to FailedManagement. |
| Observation::PlanRefusal | Existing serving-observation author admission, identity checks, ticket binding and exact completion consumption. |
| PlanRefusal deadline | Created once from heard_ns, returned as an owned wait by RecordingContext, reduced by management accounting/delivery expiry, retired by completion or finish. |
| step_with_peer | Ordinary input step passes no secondary peer, refusal accounting supplies its live peer while the previous input owner finishes its own author. |
| Typed lifecycle refusal | Existing PlanUpdate ask, strict RefusalRecord codec, management OrganEnvelope answer and opaque trace payload. No other lifecycle directive gains this trace duty. |

## Planner review correction

Planner review 5702693036 at 60b37c5e grades one required fix, AP1. The
repair retains the earlier owner's event, ticket and operation interval while
its continuation frees the author slot for the rejected root request.

The specific Stage::Turn scenario in the review was not reproduced. In the
actual API, start_turn enters Source. Source enters Turn only after consuming
its completed input author. FoundationTurn has no raw author access, and
into_service returns that already seated input. The task path enters Engine,
whose original continuation already runs in the blocked-refusal branch. The
new Source and Engine process cases pass before this runtime correction and
fail when that common input continuation is removed.

The wider audit reproduced two omitted joins. A queued save may itself wait
on the source input author. Advancing only the save leaves that input author
stranded. Wind-down has its own author continuation while the input stage is
Waiting. The repair advances the save without granting a boundary, checks
whether the author slot is still held and the save blocks input, then resumes
the original input, wind-down or Quiesce continuation as applicable. It checks
the actual retained operation rather than the externally masked progress.
No generic author resume, new workflow election or deadline renewal is added.

| Earlier owner | Continuation and focused process case |
| --- | --- |
| Checked source input | step_with_peer, worker_plan_refusal_prior_source_author |
| Task input | step_with_peer and Engine, worker_plan_refusal_prior_task_author |
| Explicit flags read | advance_flags, worker_plan_refusal_prior_flags_author |
| Earlier root update | advance_plan, worker_plan_refusal_prior_root_author |
| Save author | advance_save, worker_plan_refusal_prior_save_author |
| Source author with a queued save | advance_save without a boundary, then step_with_peer, worker_plan_refusal_queued_save_prior_source_author |
| Wind-down author | advance_wind_down, worker_plan_refusal_prior_wind_down_author |
| Quiesce author | advance_quiesce, worker_plan_refusal_prior_quiesce_author |

Report and organ-loss observations continue through the existing
advance_organ_faults call before refusal processing. They are not new
continuations in this repair. The Checking stage shares step_with_peer, but
this correction does not claim a separate new Checking-stage instrument.

All eight new cases use the entered worker and actual recorder and management
sockets. They retain a real earlier author under recorder pressure, then issue
the rejected root request. The controls remove flags, root, save, input,
queued-save fallthrough, wind-down and Quiesce continuations separately. All
seven compile and fail their intended assertions. The input control fails
both Source and Engine cases. All eight positive cases pass again after
restoration, with matching hashes for all 820 Rust/configuration files.

AP7 is corrected in the same act. The private observation claim constructs
one typed refusal payload and returns that RawValue for the event. Its exact
byte identity check remains, without a second independent serialization.

| Finding | Disposition |
| --- | --- |
| AP1 | Corrected joins and protective process evidence above. Planner regrade remains owed. The exact Stage::Turn allegation is not claimed as reproduced. |
| AP2 | Carry in #405. Measure sustained eager polling with a non-ready transport before changing waits, which must not sleep over a buffered completion. |
| AP3 | Carry in #405. Nested save cleanup errors still need the common expiry/transport reducer and a joined capture-timeout process case. |
| AP4 | Carry in #405. Consolidate returned-unaccepted-original recovery across refusal, report and organ-loss authors without retrying admitted or fatal work. |
| AP5 | Declined on the current source path. management enables ServingOpener on its pristine private peer. step rejects descriptors and invalid opener/position before granting DirectivePending. The alleged non-serving/descriptor handoff cannot reach queue_plan_update through that API. No new runtime case or changed transport policy is claimed. |
| AP6 | Filed in #405. The strict sent_at comparison agrees with AbsoluteBound::expired using now >= deadline. An explicit send-at-equality instrument remains owed, without a silent global boundary change. |
| AP7 | One typed serialization supplies both the claim and event. |
| AP8 | Carry in #405. The reducer's AdminDial guard needs its own protective case. The earlier EOF mutation bought a different guard. |
| AP9 | Review skills remain unavailable. Manual author reading is not a skill pass. |

The review development record retains failed attempts. The task fixture first
lacked its required mark template. The flag fixture first started its short
read interval before filling the recorder, then expected the whole read to
complete when only its blocking author had finished. The queued save first
inherited the one-millisecond fixture budget. Corrected fixtures use their
existing template and measured-wait families. The queued-save and wind-down
failures against the old runtime are separate from these fixture errors. The
Quiesce control establishes the need for its continuation after the repair.
These are correctness instruments, not throughput or retained-memory results.

### Correction gates and evidence

The correction passes 2,239 ordinary harness tests/doctests and 199 types
tests/doctests, excluding nested child summaries. The harness reports 949
ignored cases and the same explicit #331 skip. All 47 named worker-plan
process cases pass, as do the existing wind-down author-wait and graceful
save/Quiesce pending-author regression cases, 49 process cases total. Both
affected all-target strict lint gates, lock, formatting, diff and three
DocsMirror checks pass. The seven review controls and these gates share
identical hashes for all 820 Rust/configuration files. The eight original
controls below remain evidence at their initial published source revision.

The correction uses the same private target, pinned toolchain and locked
Cargo commands as the initial act. Each privileged process case runs in a
temporary delegated cgroup with actual admin-fixture paths where required.
No deployed agent is selected, restarted or changed.

The correction evidence deposit is:

`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/root-plan-admission-review-20261029`

The PR body supplies the archived commit and verified manifest. The deposit
retains the source, review patch, source hashes, all gate/process logs, all
seven mutation diffs and failures, restored checks, failed development
attempts and their runners. #405 retains the nonblocking review carries.
The Planner accepted the runtime repair in regrade 5703568602. The fixture
correction below answers its remaining AQ1 finding, with no production
qualification claim.

## Parallel prior-author fixtures

Planner regrade 5703568602 at e62e8215 closes AP1 and withdraws the
Stage::Turn allegation. Its continuation controls are caught. AQ1 remains a
required fixture correction: the flags case exceeded its two-second read
interval under the Planner's parallel family load, and the root case once
finished without its answer. Both passed five isolated runs on that seat.

The shared pressure fixture used 32 KiB of padding per record, so each case
drained and parsed roughly 32 MiB while real operation deadlines were live.
It now uses 1 KiB per record and permits enough attempts for the socket's
buffered records. The same real recorder queue must reach ORDINARY_CAPACITY
before the fixture returns. The original author's actual pending ticket and
the original Decision and StateControl intervals remain asserted. No runtime
deadline is enlarged or replaced, and no case is serialized.

The corrected worker-plan family passes three consecutive parallel runs,
47 cases each with 48 test threads. Independently compiled removals of the
flags and root-plan continuations still fail the intended ownership
assertion. The old fixture also passed one 47-case parallel run on this host,
so the Planner's repeated failures remain the reproduction evidence. These
runs do not establish reliability at every host load or qualify production
limits. Earlier controls remain scoped to their recorded source revisions.

Final author gates pass 2,239 ordinary harness tests/doctests, with 949
ignored cases and the explicit #331 skip, strict all-target harness lint,
lock, format, diff and three DocsMirror checks. All eight prior-author cases
pass after restoring the two controls. The existing wind-down and Quiesce
process regressions also pass. All 820 Rust/configuration hashes match the
final controls, restored tests, gates and three parallel family runs.

The first format check caught one wrapping change after the filler was
shortened. Its failed log and the passing pre-format tests remain separate.
The formatted source was rebuilt and the two controls, restored tests, full
affected gates and parallel runs were repeated. Unchanged types results
remain scoped to e62e8215, and no new types run is claimed.

The supplemental evidence deposit is:

`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/root-plan-aq1-20261029`

The PR body supplies the final archived commit, gate results and verified
manifest. AQ1 still requires Planner regrade. The PR remains draft.

## Initial published evidence and limits

The initial published head 60b37c5e passes 2,239 ordinary harness tests/doctests
and 199 types tests/doctests. These counts exclude nested child summaries. The harness
package run reports 941 ignored cases and explicitly skips known #331. The
separate worker-plan process run passes all 39 named cases, with both required
admin-fixture environment variables set. This does not claim all ignored
instruments were run.

The affected harness and types all-target strict lint gates, formatting, lock,
diff and three DocsMirror checks pass. The private target is
/tmp/weaver-state-root-plan-admission-account-target, with
CARGO_INCREMENTAL=0, nightly-2026-02-13 and --locked. All 819 Rust/configuration
hashes match the final controls, restored checks, final ordinary gates and
complete process family. The original full-gate cohort differs only in the
subsequently strengthened EOF process test. No runtime code changed between
those two cohorts.

The process instruments use the actual root listener, peer authentication,
entered worker, recorder and management sockets. The wider worker-plan family
includes real SQLite and the separate admin process. The new refusal cases
use fixture member/model behavior. They do not qualify a production model.

The new cases cover invalid cause and clean inner refusal independently, two
refusals during an open turn, original bytes and omitted envelope fields,
earlier author originals and tickets, real recorder/socket pressure, EOF
before admission and after a pending ticket, accounting/delivery expiry,
Recorded-before-completion expiry, an earlier operation deadline, timely send
followed by expiry, post-send clock failure, flags with and without a queued
save, and actual failed-run handoff. The existing SQLite update case exercises
a competing refused root update while the first root operation is pending.
Controlled clocks isolate the deadline boundaries, they are not CPU or latency
measurements. The pressure cases retain real sockets and the real recorder.

Eight distinct compiled controls fail their intended process assertions:

| Control | Detecting case |
| --- | --- |
| Bypass trace-before-send | invalid cause, send queued before Recorded |
| Discard exact received wire | invalid cause, original-byte comparison |
| Allow EOF to complete an unrecorded refusal | complete-opener EOF, Dropped instead of Pending |
| Skip pre-Recorded accounting failure | accounting expiry, Pending instead of Stopped |
| Pretend the Recorded completion was consumed | Recorded expiry, caller completion still owed |
| Drop the refusal account in FailedManagement | pending-ticket expiry, retained account absent |
| Shift the receive anchor by one | invalid cause, original began_ns mismatch |
| Forget a timely actual send on later expiry | timely-send expiry, Dropped instead of Answered |

These are assertion failures from compiled controls, not compile failures or
watchdog timeouts. The first EOF control survived on the earlier fixture and
is retained separately. The corrected source reruns every control. Source
bytes are restored after each mutation and compared against all 819
Rust/configuration hashes. The six owner checks, two packet checks and fourteen
focused process cases pass again after restoration.

Development evidence retains the failures as well as the passing runs: a
blocking trace-reader fixture, the returned-original and timely-send runtime
defects corrected before the ordinary gates, missing fixture turn attribution,
missing member/admin binaries or environment, and a new queued-save mode that
initially inherited a one-millisecond test budget. The corrected mode uses its
existing five-second family budget. An initial EOF mutation survived because
the test observed disconnect and Recorded in one reduction. The strengthened
case observes real EOF before admitting the claim, so premature completion
cannot hide behind that ordering. The source/runtime fixes and fixture fixes
are distinct in the preserved diffs and logs.

Evidence location for this slice:

`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/root-plan-admission-account-20261029`

The PR body identifies the archived commit and verified deposit manifest. The
archive includes the source, exact base-to-head patch, source manifests, gate
and process logs, compiled controls, restored checks and failed development
attempts. Earlier source cohorts remain separately identified.

The code-review and verification-before-completion skills are unavailable in
this session. Manual author review and executed gates are not skill passes,
a Planner grade, a security review or production qualification. No CodeRabbit
review was requested. Required artifact, trace-size, retained-memory and bound
measurements remain under #146 before the coupled cutover. No deployed agent
was changed by these tests.

## After-action review

The plan was to implement the merged refusal owner and protect its joins to
the original worker. Composition exposed two runtime edges in the parked
experiment: a returned unaccepted event could become permanently stopped,
and a timely actual send could be relabeled dropped at the next expired
reduction. Both were reproduced before correction. The compiled EOF control
then exposed a test-ordering gap, not another runtime defect. Keep the exact
receive, Recorded, caller-completion and actual-send boundaries separate in
future process fixtures, and observe transport loss before the accounting
step when that is the property being tested. This record carries that lesson
forward for the next plan-operation act.

AQ1 also separates recorder occupancy from artificial byte volume. A custody
fixture must prove the actual occupied slot and missing-continuation failure.
Its filler need not consume a large share of an unrelated real read interval.
Keep byte-limit qualification in its own measured instruments.
