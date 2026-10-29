# Outbound refusal decision budget

Date filed: 2026-10-27. Author record for issue #146.
Branch: code/state-emission-refusal-budget.
Base: code/state-world-request-composition at 9a5a6764 (draft #363).

This act answers #332's Planner Z2 finding. An outbound refusal keeps the
original Decision deadline through its required terminal accounting and world
close. The foundation dispatcher remains unselected. Do not deploy the
incomplete replacement.

## Authority and behavior

Implements: Spec 02 section 5 DECISION-BARRIER and the harness amendment's
HARNESS-CLOCK. Author assessment: conforms for the three outbound refusal roads
and their continuation below. This is not a verdict on the remaining foundation.

Previously, a refused candidate, refused sort instruction or Recorded template
refusal retired Decision. Electing the refused close then armed a new full
interval for the remaining terminal receipts and world delivery. Those receipts
are intermediate progress within the same barrier. The change removes those
three early retirements. No limit, public type, message or Spec sentence changes.

The original generation and corrected identities remain with the same emission
account. Required pair terminals still precede the trace close. A held receipt,
full recorder or undrained gate consumes the remaining original budget. Expiry
halts with DecisionBound and grants no successful delivery. A completed close
retires Decision. Stop can replace an unelected operation refusal with the
existing directive outcome, retaining the original bound through that handoff.

## Owner and caller census

| Site | Disposition |
| --- | --- |
| EmissionWork BeginOutbound, invalid candidate without an owed author | Keep Decision while Refused retains the original OutboundInput. |
| EmissionWork Outbound, Refused completion | Keep Decision with the completion's original input, sort record and fault. |
| EmissionWork RefusalAuthor, Recorded | Keep Decision while moving to Refused or cancelled accounting. Held author custody is unchanged. |
| close_refusal / CancelAccount / Pairs | Consume the retained account once and await every terminal receipt under that same bound. No second interval is granted. |
| finish_accounting, ordinary refusal | Retain Decision for RefusalClose and the shared WorldClose. |
| finish_accounting, cancellation | Retire completed accounting's wait. A previously elected Stop close has already retained the original cap. |
| WorldClose begin / advance / completed delivery | Inherit the retained bound through close authoring and delivery, then retire it on completed disposition. |
| Outbound Sorted | Still retire the completed sort barrier. Successful execution and a later barrier keep their existing rules. |
| Correction placement refusal | Already carries one bound through accounting and close. The same trace/gate fixture now exercises both refusal classes. |

## Evidence

The broad harness library invocation passed 2,084 cases, with 797 declared
ignores. Its one environment-probe test requires Cargo's CARGO_PKG_NAME and failed
when invoked directly. That case passed when rerun through Cargo, for 2,085
distinct passing unit cases. All 156 selected worker process identities passed,
including the twelve new cases and the five correction refusal boundary cases.
The first worker invocation omitted its admin-artifact variable. The complete
156-case rerun supplied the current compiled admin artifact after sudo.

Harness all-target clippy at -D warnings, formatting and the combined test build
passed. Cargo used the pinned toolchain, --locked and the existing private
target. Source manifests retained the exact tested runtime and fixtures.

Three independent protective mutations restored the premature retirement at
BeginOutbound, Outbound-refused and RefusalAuthor respectively. Each compiled
and failed its matching process assertion. After restoring the source, the
combined library test build, author-hold unit case, all eighteen focused process
cases, formatting, lock and diff checks passed. The final source manifest
matches the broad-run source. Complete logs and controls are retained in the
operator's evidence deposit.

The author self-check read the changed transitions and every Decision retirement
in EmissionWork and WorldClose. The repository's named review skills were not
available in this environment, so this is a manual author check. Planner review
remains pending.

The twelve added process cases cover each of the three outbound refusal roads
with a withheld terminal receipt, expiry at that receipt, released trace pressure
and gate expiry. Further cases cover gate pressure that clears, trace expiry and
Stop after completed accounting. The fixture moves its numeric clock near the
original deadline and uses a non-ready Timeout while a receipt or full transport
still blocks progress. It checks the actual failure, withheld delivery, retained
account, terminal identities and trace ordering.

The existing author-hold test retains its original refusal through three held
owners and Stop/Unload. It now requires the original Decision interval after the
refusal record. The Observe case now treats OperationRefused as an intermediate
state with accounting owed. Completed sorted outcomes still allow observation
after their retired interval.

These are actual worker/gate/decoder/classifier process joins with synthetic
language and model replies. They are author evidence, not a Planner grade,
security disposition, language qualification or deployment approval.

## Remaining work and author note

Issue #146 stays open. The next composition work includes #348/#349's
capture-refusal S4-S7 continuation, the remaining lifecycle and failed-report
handoffs, production dispatcher integration, measured artifact qualification and
the operator's coupled cutover. #363 and this branch await review and merge.

The planned runtime change was three deadline retirements, and it stayed at
those three sites. Reusing the correction close fixture exposed a test-only
assumption: Stop after a placement refusal keeps its refusal outcome, whereas
Stop before an ordinary operation close takes the directive outcome. The fixture
now checks each. Future completion tests should distinguish an intermediate
refusal account from its completed world response before asserting a retired
clock.

Gate-runner note: invoke environment-probe cases through Cargo. Pass fixture
artifact variables explicitly after sudo, including when selecting a family
whose names include an admin/worker composition case.
