# Root PlanUpdate admission-refusal account

**Version:** 0.1, 2026-10-29.
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
| Earlier original or author ticket blocks admission | Run only its original input, flag, root-plan or save continuation as needed to free the slot. Keep the refusal original and interval. |
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

## Evidence and limits

The final source passes 2,239 ordinary harness tests/doctests and 199 types
tests/doctests. These counts exclude nested child summaries. The harness
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
