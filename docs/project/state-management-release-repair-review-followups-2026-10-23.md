# Release and repair review consistency

Date: 2026-10-23. Branch: code/state-release-repair-review. Parent: exact
#269 d2896bcd336b1b1ec78f370a6ff657b6ae9bcbef. Issue: #146.

## Scope and authority

This dormant followup answers #265 Planner review 5624154624 rows 1-10 and
#266 review 5624237476 rows 1-8. It preserves the placement, decoder health,
original-bound and extracted-Boot protections of its parent. #266 row 9 remains
in the separate STE100 runtime/types source batch; its map row was already fixed.

Implements: HARNESS-RECORD-LIFETIME, HARNESS-OPEN/OPEN-LIVE, HARNESS-HALT/BOUND
and TRACE-STOP/CAPACITY; Spec 02 REPAIR/RECALL-REFUSAL/EMPTY-CORRECTION custody
and Spec 05 PAIR-STATES/COMPLETED-PAIR/PAIR-LEAVE. These are implementation
consistency changes under merged authority. The approved but unmerged #263
counting election remains unimplemented. No new retry, dispatch, cancellation
policy, clock or serving activation is introduced.

## Retained release custody

The private release mark now travels with its exact AuthorTicket. Taking or
clearing the original also moves or clears the mark, including summary rollback.
Resume follows the retained release attempt rather than inferring ownership from
Releasing phase. Current scope/mark validation still checks original identity,
kind, purpose, phase and bounds. A borrowed scope is retained deliberately: a
barrier-only boolean would lose those checks at each wake.

BusyAuthored retains its exact returned original before fallible intake. A true
trace/intake failure still wins; otherwise typed Placement or BootOpen debt
returns PlaceOwned or BootOpenOwned without inventing TraceFailed. No ticket or
sequence is admitted. Placement takes precedence if defensive flags overlap,
matching public context admission. Only the unadmitted release attempt retires.
One writer retains Recorded facts; release does so before fallible continuation,
while ordinary completion preserves its earlier owner-binding boundary.

The actual placement join starts MeasureDelta and reaches Releasing with that
account owed. The context-issued private release scope exercises the inner hold,
retains the original allocation/text and unchanged Leave, drains the real reply,
then records the same original once and retires transports through Closed.
This supplements the deliberately defensive private-scope tests; it does not
claim a public caller can bypass its outer owner guard.

The companion stopped-release record carries every #265 row and surviving
Planner perturbation disposition, including retained redundant defenses and
test-module organization.

## Refused correction evidence

One validator requires an ordered PassPurpose prefix, then Stop, then Unload,
bounded by the actual retained cancellation cause. It checks exact pair identity,
terminal fate and absence of dispatch/execution/load evidence. The actual
three-call fixture preserves each earlier terminal cause and the original bound.
Defensive composed proofs separately test impossible backward/wrong-cause cases.

Outstanding correction/account loans now return named faults rather than polling
until an unrelated deadline. Pre-intake UnknownCall means a fresh unadmitted call;
other pair errors remain exact, and every error propagates when required terminal
proofs are collected. Successful proof validation retains one settled fact;
readiness polls do not reacquire the completed pairs.

Recorded refusal is written once. Shared setup owns both pending correction
accounting paths. Association traversal remains borrowed until the selected item
is cloned; owned-record allocation is linear, while iterator traversal can remain
quadratic. That allocation cleanup has source evidence, not a fabricated
behavioral mutation claim. AuthorPurpose is no longer Clone and has a separate
compile-time pin. Counting and the existing redundant extraction guard stay as
reviewed.

## Author validation

The source author added four release and six repair tests and strengthened the
actual placement/debt composition fixture. Independent source reviews found a
returned-original custody hole and one joined AuthorTicket constructor missing
its new field; both were corrected before final validation.

Author gates pass: six focused repair tests, 97 release tests, the actual
placement/debt fixture, full harness 1,678 passed with 18 inherited ignores,
trace 160, harness all-target Clippy with warnings denied, formatting, lock and
diff checks. Gates used /tmp/weaver-146-release-repair-review-target with lock
first and --locked, serially on the host socket lane.

All 19 protective mutations compiled and failed behaviorally, followed by exact
restored passes. A separate AuthorPurpose Clone perturbation failed at the
intended NotClone ambiguity pin with E0283; its restored six repair tests passed.
That compilation rejection is not counted as a behavioral mutation. All source
hashes were checked after restoration. Logs, initial source reviews and exact
published-source evidence accompany the draft. Author checks are not Planner
grades or production qualification.

## Remaining composition

The separate completed-Boot rollback slice retains pre-extraction evidence after
Stop. A proof extracted earlier still needs a consuming failed-Enter rollback
adapter. Production lifecycle/worker callers, qualified classifier artifacts,
prompts and measured limits, and the reviewed coupled serving cutover remain
open. The existing serving path stays active; this slice does not complete #146.
