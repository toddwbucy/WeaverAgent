# Plan-update admission and identity evidence

**Version:** 0.1, 2026-10-28.
**Parent:** Issue 146, state management implementation.

This act addresses the X1/X2 test gaps in Planner review 5692337515 of #381.
It is stacked on draft #383 at dee6c94a13053c34455fa70e0ca7cf5b5669e07c.
All implementation changes here are test code or cfg(test) observation. Runtime
behavior, the selected serving entry and production qualification do not change.

## What the tests establish

The admin exchange's existing matrix now also receives a foreign run and a nonnull
turn on PlanUpdated. Its foreign-seed, foreign-run and turn-attribution packets decode
successfully before transmission, so their expected Malformed outcome must come from
the owner rather than the strict codec. Each identity predicate is removed separately
by a compiled control, and each removal fails this matrix. The foreign-seed case was
already present on #381. The new codec assertion and isolated controls strengthen its
evidence rather than claim the case itself was absent.

The second-dial fixture re-executes the compiled test binary and binds a real
SOCK_SEQPACKET listener in that child. Observe comes from the original process and
the second dial reaches the new process. The real SO_PEERCRED/pidfd comparison refuses
BoundaryUnverified before any plan-update byte reaches that listener. Removing
require_same_worker from the exchange lets bytes cross and fails the fixture. This
covers the owner-level pin independently of root role provisioning or the healthy
actual-admin/SQLite circuit.

The deadline case fills the update socket, including its remaining one-byte capacity.
A test-only thread-local observer reports the deadline arguments at entry to the real
send and receive methods. It also signals the peer when send begins. The peer holds
that pressure for 50 ms, drains the filler, receives exactly one update and withholds
an answer until the admin closes at expiry. Both observed deadlines must be identical,
and the send must consume at least 25 ms of the original 500 ms fixture budget. Giving
receive a fresh budget still produces Unanswered but fails the equality assertion.
The observer changes neither a socket outcome nor a clock result and is absent from
production builds.

The worker process case accepts actual gate input, records a turn and then receives
two root PlanUpdate requests on separate management connections. Both answer
OutOfOrder. The original turn, committed summary and input continuation remain owned,
no root operation starts, and no plan event or turn close appears. Removing the outer
plan_at_rest admission guard makes the fixture fail its bounded answer assertion.
This pins the public management boundary. It does not separately isolate the inner
begin_root_plan_update open_turn check, which remains a defensive review carry.

## New activation finding: admission refusal is not traced

The worker case exposed an existing PLAN-EXCHANGE drift. Both OutOfOrder answers
reach their peers, but zero refusal records reach trace. The failed development probe
expected two and reported zero. The final admission fixture reports that count without
asserting that absence is correct. This act does not claim to fix this trace defect.

Spec 07 PLAN-EXCHANGE says the worker traces the outcome before answering. The ordinary
OutOfOrder branch in context_worker_management.rs sets ManagementAnswer directly and
sends it without an author owner. The accepted root transaction's later refusal path
already has trace accounting, but a busy-worker request never enters that transaction.
The two paths must not be confused in a conformance claim.

The next implementation act must retain the refused management request and exact
RefusalRecord::Lifecycle with ask plan_update, finish its Recorded boundary before
sending the answer, and preserve the suspended turn. It must also handle recorder
pressure, any earlier held author, requester EOF, existing bounds and failed-run
custody. It must neither acquire a competing root mutation grant nor cancel or advance
the suspended turn merely to answer the refusal. The tests must show one exact record
per refused request, no answer before Recorded and no lost prior author. Read the other
pre-admission refusal branches as part of that act. They were not exercised by this
busy-worker case, so no broader runtime finding is claimed here.

This is an activation blocker under existing merged authority, separate from draft
#384's proposed non-root plan-operation outcomes. No new operator election is inferred
or requested for correcting this existing root trace obligation. Issue #146 remains
open and existing serving remains selected.

## Validation

Lock, formatting, both affected all-target strict lint gates and all three DocsMirror
tests pass. The full admin package passes 606 tests with 73 ignored. The delegated
worker plan family passes all 21 cases, and the delegated root-admin plan family passes
all 13 cases. Child-test output is not counted again as a top-level package pass.

The affected crates use the pinned toolchain, --locked and the private target
/tmp/weaver-state-146-serving-organ-loss-target with CARGO_INCREMENTAL=0. Cargo runs
as the operator, never through sudo. The actual root fixtures run from the compiled
binaries under owned delegated systemd scopes. The worker plan family includes the
real SQLite/actual-admin case with WEAVER_ROOT_PLAN_ADMIN_TEST_BIN set.

The full harness library suite is not rerun for this test-only act. Its all-target
lint, fresh test-binary build and complete worker plan family are the relevant new
checks. #383's earlier 2,205-pass result belongs to its own head and is not recounted
as this head's result. The SPU, gate, state and floor runtime sources are untouched.
Fixtures are synthetic and establish no production language or deployment tuple.

| Control | Removed property | Observed failure |
| --- | --- | --- |
| A1 | Admin compares the returned seed with observed load facts | Codec-valid foreign-seed PlanUpdated is accepted instead of Malformed. |
| A2 | Admin compares the returned run with observed load facts | Codec-valid foreign-run PlanUpdated is accepted instead of Malformed. |
| A3 | Root receipt source has null turn | Turn-attributed PlanUpdated is accepted instead of Malformed. |
| A4 | Second dial uses the observed worker process | Foreign listener receives update bytes, and BoundaryUnverified is lost. |
| A5 | Receive inherits the send's deadline | Unanswered still occurs but the recorded deadline arguments differ. |
| W1 | Worker management checks plan_at_rest before delegation | A request during an open turn receives no required OutOfOrder answer within the fixture's five-second assertion. |

All six controls compile and fail their intended cases. W1's failure is the fixture's
answer-progress assertion, not a new runtime refusal code. The fixture children later
hit their cleanup watchdogs during that negative run. All 809 Rust/configuration hashes
restore after every control and remain unchanged through the final gates.

Development failures are retained. The first sandbox run denied Unix socket operations
with EPERM, and the same tests passed on the host. The first harness compile found two
test-only mutable-borrow mistakes. The first worker attempt left the gate idle because
other plan fixtures intentionally do so, and hit the child watchdog. The next attempt
reached both correct answers and exposed the zero-refusal trace count described above.
No such failure is represented as a passing production check.

Manual author review inspected the diff, the private observer's cfg(test) boundary,
actual child credentials, the guard-removal results and the pre-admission trace path.
The code-review and verification-before-completion skills are unavailable in this
session. This is a manual author check, not those skill passes or a Planner grade.

## Evidence and review scope

The evidence destination is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/plan-admission-proof-20261029T043220Z`.

The deposit retains the source archive, source hashes, gate logs, mutation scripts and
logs, and development failures. Only the operator merges or deploys. This draft does
not fix the remaining #381 W1-W7/W9 batches or earlier review carries, and it adds no
CodeRabbit request. #384 still requires its independent election, review and merge
before non-root plan-operation implementation relies on it.
