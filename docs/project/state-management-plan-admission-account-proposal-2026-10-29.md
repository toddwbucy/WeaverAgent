# PlanUpdate admission-refusal account proposal

**Version:** 0.2, 2026-10-29.
**Parent:** Issue 146, state management implementation.

This proposes an explicit trace obligation for rejected PlanUpdate openers. It awaits
the operator's election, Planner review and merge. It changes no runtime code and
claims no implementation or production qualification. Draft PR #387 uses
`docs/state-plan-admission-account`, stacked on #384 at `cb17343e` to keep the shared
Spec headers ordered. Its election is separate from #384's non-root plan-port result
and trace representation.

## Evidence and disagreement

Draft #385 at `4d8f4a14` exercises two root PlanUpdate openers during an open turn.
Both return OutOfOrder, retain the original turn and cache, and start no plan
operation. The trace contains zero refusal records. The author read PLAN-EXCHANGE's
"The worker traces the outcome before the answer" as covering this path.

[Planner review 5692982046][review] gives #385 zero fix findings but grades AA1 a
Spec wording question. It reads that sentence as part of an admitted exchange and
observes that the general management branch answers every out-of-order directive
without a trace record. The author and Planner readings therefore differ about
pre-admission authority. AA1 is pending the operator's ruling, not an established
implementation defect or activation blocker.

The runtime experiment in `code/state-plan-admission-refusal`, based on #385, is
preserved locally and unpublished. It compiled with the new claim and tests before
the review arrived. It has not run those tests, completed its gates or passed review.
It cannot be cited as implementation evidence or selected for serving.

## Proposed election and alternatives

Elect trace-before-answer for a fully received, well-formed PlanUpdate that the
lifecycle owner judges and refuses before admitting a plan operation. Record the
existing lifecycle refusal with asked plan_update, exact refusal, runtime attribution,
turn null and causal_parent null. Busy-state and invalid-root-cause refusals both owe
this account. Invalid framing, descriptors and incomplete openers retain the current
transport treatment.

The null turn names the rejected root opener's lack of an accepted operation, even
when another caller has an open turn. Its run still comes from the trace envelope.
The non-root refusal in #384 belongs to an accepted internal operation and uses that
operation's nullable turn. Both rules attribute the event to its own work rather than
borrowing an ambient turn. This answers Planner #387 BB1 without changing either
proposed attribution.

This election reaches PlanUpdate only. Other lifecycle refusals keep their current
rules. The reason is that the operator-facing plan exchange already promises a traced
outcome, and this makes that promise explicit across its admission boundary. It does
not infer a new lifecycle-wide audit policy from the plan clause.

| Choice | Consequence |
| --- | --- |
| Proposed: trace pre-admission PlanUpdate refusals | One existing metadata record per judged refusal. A valid busy refusal can wait for earlier author custody and recorder capacity. Its own bounded accounting can fail the run if the trace cannot record. |
| Clarify that only admitted exchanges owe the trace | Preserve the current general management branch. The admin answer is the only record of a busy or invalid-cause rejection. Retire the unpublished experiment. |
| Trace every lifecycle refusal | Broader audit policy requiring each directive's admission, attribution, deadline, failure and teardown contract to be specified together. This draft does not propose or implement that change. |

The proposed rule does not reclassify a busy request as admitted work. It preserves
the original input, turn and cache, and never parses or files the rejected document
as a plan input. A later idle boundary does not retry that request.

## Owners, bounds and terminal events

| State | Event or actor | Required transition |
| --- | --- | --- |
| Complete judged opener | Management selects a refusal | Retain the exact packet and refusal. Start one interval from the retained reading immediately after receipt, using the loaded decision wait. |
| Earlier author owns the slot | Original operation continuation | Consume only its own receipt as needed to free the slot. Preserve its unchanged bounds and operation custody. No cancellation or new workflow election follows from the rejected request. |
| Refusal author pending | Recorder pressure or wake | Resume the same event and ticket. Send no answer before Recorded. |
| Refusal author pending | Root EOF | Retire delivery, retain and finish the accounting duty under the same interval. |
| Before Recorded | Own accounting interval expires | Foundation decision_bound failure. Retain the full management account and original first failure through teardown. |
| Any state | Earlier operation expires or a component fails | Existing first-failure and teardown rules. The refusal cannot renew or replace the earlier bound. |
| Recorded completion | Exact private author continuation | Consume only this claim's completion, even if delivery has expired. |
| Recorded and live delivery | Management sender | Send the unchanged refusal on the original exchange under the interval's remaining time. |
| Recorded | EOF or delivery expiry | End delivery without retry or a halt solely for the lost answer. Preserve the recorded outcome. |

There is no new timer value in the declaration or protocol. This borrows only the
loaded decision_wait elapsed_ns value, as admin does for its plan exchange. It does
not start or borrow a model decision barrier or grant its authority. This answers
Planner #387 BB2. The independent local
interval prevents this management work from resetting an active turn's Decision,
Cut, read or lifecycle bound. It includes trace accounting and send backpressure.
The new refusal record adds trace work and retained memory which the production
qualification still has to measure.

## Paired clauses and implementation plan

| Act | Owning clauses | Grade and duty |
| --- | --- | --- |
| This draft | Spec 07 PLAN-EXCHANGE and new PLAN-ADMISSION-REFUSAL | Spec gap proposed. Set scope, attribution, custody, EOF and accounting/delivery outcomes. |
| This draft | Harness amendment HARNESS-PLAN | Spec gap proposed. Retain a management-owned trace claim without taking another operation's ticket or changing its bounds. |
| This draft | Trace amendment TRACE-PLAN | Spec gap proposed. Reuse the exact existing floor refusal encoding and runtime metadata envelope. |
| After election, Planner review and merge | Issue #146 root plan management composition, after #385 | Complete and gate the refusal owner and worker adapter across outer busy, cause-refusal and inner clean-refusal branches. Reconsider the unpublished experiment against the elected text. |
| Before coupled cutover | Issue #146 production qualification | Measure the added accounting and retained memory with the approved artifacts and trace reader. |

Do not deploy from this Spec change. The implementation stack still omits these
records. The named implementation act must land the code and tests before foundation
serving is selected. No schema, admin answer shape, member protocol or cross-repository
contract changes in this draft. It does not claim an external reader has been tested.

## Verification owed by the implementing act

Use the real root listener and entered worker for two refusals during an open turn,
one unchanged answer and one exact record for each, with no new plan operation or
turn close. Pin record-before-send directly. Exercise the invalid-root-cause and inner
clean-admission refusal branches independently.

Hold a previous author's original and its admitted ticket in separate cases. Show
that its exact owner finishes it, that no competing ticket consumes its completion,
and that the rejected plan never starts when the original turn finishes. Cover an
earlier accepted root update, flag read and save as well as a turn author.

Apply recorder pressure and root socket pressure separately. Deliver EOF before
recording, after admission and after Recorded. Pin the original receive reading and
unchanged interval, accounting expiry, delivery expiry without a halt, an earlier
operation deadline, and a clock failure after send. Carry all unconsumed originals,
tickets and first failures into failed-run teardown. Remove each claimed ordering,
custody and deadline property with a compiled protective control and require its
corresponding case to fail.

DocsMirror, ASCII and diff checks are the gates for this proposal. The code-review
and verification-before-completion skills are unavailable in this session. Manual
author checks do not replace those skills or the Planner's review.

[review]: https://github.com/toddwbucy/WeaverAgent/pull/385#pullrequestreview-5692982046
