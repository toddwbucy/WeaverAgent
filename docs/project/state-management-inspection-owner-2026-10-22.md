# Bounded state inspection owner

## Scope and authority

Draft PR: #248.

This slice advances issue #146's serving inspection migration with dormant Shape and
Grants ports on RecordingContext. Its original parent is draft #247 at 619341c.
That ancestry carries exact #190 and its operator hold for the coupled foundation
cutover. The
legacy engine and lifecycle callers remain unchanged.

Implements: weaver-harness-Spec-amendment HARNESS-READ-CALLERS, HARNESS-READ,
HARNESS-PUMP and HARNESS-RECORD-LIFETIME, limited to the typed small-control owner and
its recording, deadline and admission boundaries. HARNESS-HALT applies to the existing
first-failure path. The author grades these scoped clauses conforms on the checks
below. This is an author check, not a Planner grade or production qualification.

The owner admits only Shape or Grants. It retains the exact control, optional turn,
allocated request identity and original two-second StateControl bound. Existing Enter
and Leave waits cap that bound. A busy admission returns the caller's kind and turn
without allocating a request or replacing another wait. Once admitted, every lifecycle
yield retains the owner, including the interval before allocation and the interval
after enqueue. A begin error's retained flag identifies whether this call installed
that owner. Returning the original kind and turn alone is not permission to retry.

The existing coordinator records state.store.measurement before releasing the answer.
Ordinary State completions now preserve that same Recorded timestamp, as capture
already does. An exact identity-bound timely proof ends this inspection's control
interval even if the final nonblocking channel probe or consumer scheduling is
delayed. The proof does not bypass a channel failure, grant an answer during a pending
probe, retire an outer lifecycle bound or issue another request. The one existing
state reader and common pump remain the only transport path.

Settlement retains the original typed StateFrame with its operation observation. The
shape adapter groups runs by first appearance and keeps each run's returned kind order
and full u64 counts. Grants returns the exact observed vector. Successful empty data,
a valid read-code refusal with its unchanged detail, cancelled consumption and a
latched component failure remain distinct. An incompatible response is a protocol
failure, including a capture-only refusal outside any capture owner.

Cancellation before request allocation withdraws the original request, turn and bound
without a member control, answer or Recorded proof. It retires only its still-owned
StateControl interval. Cancellation after allocation suppresses the consumer result
while the exchange and its exact measurement finish under the same bound. Neither path
creates a snapshot cursor, Release, second control or fresh interval. The completed
owner blocks replacement and competing
input, read, capture, filing, pair, repair, measure, mark, template, freshness and
ordinary author admission until its final take. Generic completion access cannot steal
its answer and returns InspectionOwned while this owner remains. A final transfer has
no fallible clock read after removing the owner.

## Tests and protective changes

Twenty-three new tests use the existing real Unix-stream member fixture and serving
trace writer. They cover ordered populated shape with u64::MAX, exact grants, empty
success versus typed refusal, the original typed response, backpressure before
measurement Recorded, cancellation before send and while recording is blocked, one
request without cursor cleanup, original control expiry, Enter/Leave caps, timely
Recorded consumer delay, interrupted final release, late-proof lifecycle priority,
admission conflicts, completed-owner replacement, malformed responses, EOF,
post-enqueue lifecycle interruption, final take and release custody.

The interruption fixture changes one syscall outcome on a single StateChannel after
its ordinary proof checks. It does not assign a fabricated Recorded proof, completion,
request identity or result. The existing manual coordinator proof test now preserves
its actual recording timestamp to match the ordinary State completion invariant.

Four joined-owner tests also cover boot and inward work at initial admission, between
read/filing legs and after completion before take. Reciprocal admission returns the
original PriorBootOpen or EffectiveEmission without losing its identity. The caller
can retry those exact inputs after the inspection owner is taken.

Original-slice validation at f16b6169 on 2026-10-22 used the private Cargo target
`/tmp/weaver-146-inspection-target` and locked dependency resolution:

- Focused filter: 27 passed, comprising 23 new and four inherited tests.
- Full harness suite: 1379 passed, with 18 inherited ignored cases.
- Strict all-target harness clippy, formatting and lock checks passed.
- All 21 compiled protective mutations failed their intended tests. Every exact
  source restoration passed the corresponding baseline test.

The mutations pin counts/order, refusal mapping, cancellation, exact retained
response, original deadlines, recorded-before-use, owner replacement, competing
admission, final transfer and reciprocal boot/inward ownership fences. Independent
author readings checked the implementation and the parent join. The code-review skill
was unavailable in this session, so no skill invocation is claimed.

Logs, source hashes, recipes and the runner are deposited under `inspection/` in
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-21/`. The deposit
records failed protective runs separately from their restored passes.

## Planner follow-up

The follow-up adds nine tests. Real socket and recorder evidence reaches a pending
ordinary completion before the inspection consumes it, so the external-take test
detects actual theft rather than observing an already empty slot. The association
tests inspect copies of that completion through the same small predicates used by
retirement and settlement. Changed request, request ID, turn, vocabulary and discard
status are defensive inputs. A separate fixture changes only the owner's expected ID
or turn before retirement and again after retirement but before settlement. Each
boundary must latch protocol failure and retain the unchanged real Recorded completion.
These probes never replace a sealed completion and do not claim those values can pass
the state channel's earlier checks.

A per-instance coordinator Busy hook pins rollback after admission without inventing
an operation or ticket. The normal synchronous admission excludes those Busy causes.
An actual independent Label wait suspends admission before allocation. After that wait
retires, cancellation returns Withdrawn and sends nothing. Repeated cancellation and
advance retain the original account, and taking it leaves no StateControl interval.
The withdrawn owner still rejects a replacement inspection, read and generic completion
take. Its final transfer preserves the original request, turn and bound, and performs
no fallible clock work after removing custody. The enclosing Leave deadline survives.
A deliberately replaced private deadline pins refusal before any foreign wait can be
disarmed. This latter case is an invariant probe, not a healthy caller path.

Both inspection and read admission now name pending_author and the coordinator's
filing/output consumers explicitly. A real pressured author test preserves its ticket
through both Busy refusals. A real filing-consumer reservation without an author
separately pins both admission guards and preserves the reservation. A healthy ticket-only
barrier is not manufactured: ordinary preparation installs the pending event in the
same synchronous act, while earlier error exits latch a failure rejected by live().
The standalone new guard is therefore redundant under the existing healthy invariant.

Shape grouping now indexes each run's first vector position with a borrowed-key hash
map, preserving the required first-appearance and per-run kind order without searching
the growing vector for each row. The existing exact-order and full-count fixture
continues to cover the observable contract.

The broader owner admission/settlement refactor is declined for this correction.
Admission predicates differ across owners; their common fields alone do not express
the exclusion contract. The passive inspection hook in live() deliberately retires
an already Recorded control before checking its expired wait, without consuming the
answer, progressing transport or changing a lifecycle deadline. Moving it to ordinary
reduction would break the timely-Recorded/delayed-consumer case already pinned here.

The current follow-up joins #247 at 162834e, preserving the corrected #245 and
main 1acf0bdf ancestry. All 36 focused inspection tests and 1,408 full harness
tests passed, with 18 inherited ignored cases. Strict all-target harness lint,
formatting and lock passed. Sixteen protective mutations compiled and failed their
named regression; every exact restored baseline passed. All 484 recorded source
hashes match the tested restored tree. Independent source review found no
remaining concrete defect. The Busy probe now names the exact Grants control
and original turn.

Current logs, recipes, restored checks and source hashes are retained at
/tmp/weaver-146-evidence/review-round2-248/ with private target
/tmp/weaver-146-inspection-target. These new records have not been deposited in
shared storage. This is author evidence, not a new Planner grade or production
qualification. Earlier counts above describe their original slice.

## Remaining integration

This owner is not Ports::session_shape or the lifecycle's final grant collector. The
coupled caller migration still must replace those legacy methods and map failed or
busy halt-time grants to the existing unreadable surface without making a fresh ask.
This owner accepts work only in the active context and offers no failed-run query
escape.

These results are inspection data. They cannot construct RecordedRead, HeldMaterial,
SelectedUnit, a placement grant, a generation grant or an effective permission. No
workflow selection, classifier policy, decoder Open, private journal initializer,
failed-run teardown or production qualification is implemented here. Pending Spec
proposals and operator artifacts supply no authority to this slice.
