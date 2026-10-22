# Bounded state inspection owner

## Scope and authority

This slice advances issue #146's serving inspection migration with dormant Shape and
Grants ports on RecordingContext. Its parent is #243 at
3cb2430067b56a435803f36613f4916c531e94e1. The legacy engine and lifecycle callers remain
unchanged. Runtime activation still requires the coupled foundation cutover.

Implements: weaver-harness-Spec-amendment HARNESS-READ-CALLERS, HARNESS-READ,
HARNESS-PUMP and HARNESS-RECORD-LIFETIME, limited to the typed small-control owner and
its recording, deadline and admission boundaries. HARNESS-HALT applies to the existing
first-failure path. Author validation and grades are pending the checks below.

The owner admits only Shape or Grants. It retains the exact control, optional turn,
allocated request identity and original two-second StateControl bound. Existing Enter
and Leave waits cap that bound. A busy admission returns the caller's kind and turn
without allocating a request or replacing another wait. Once admitted, every lifecycle
yield retains the owner, including the interval after enqueue and before the caller
regains control.

The existing coordinator records state.store.measurement before releasing the answer.
Ordinary State completions now preserve that same Recorded timestamp, as capture
already does. An exact identity-bound timely proof ends this inspection's control
interval even if the final nonblocking channel probe or consumer scheduling is delayed.
The proof does not bypass a channel failure, grant an answer during a pending probe,
retire an outer lifecycle bound or issue another request. The one existing state reader
and common pump remain the only transport path.

Settlement retains the original typed StateFrame with its operation observation. The
shape adapter groups runs by first appearance and keeps each run's returned kind order
and full u64 counts. Grants returns the exact observed vector. Successful empty data,
a valid read-code refusal with its unchanged detail, cancelled consumption and a
latched component failure remain distinct. An incompatible response is a protocol
failure, including a capture-only refusal outside any capture owner.

Cancellation suppresses the consumer result while the admitted exchange and its exact
measurement finish under the same bound. It creates no snapshot cursor, Release,
second control or fresh interval. The completed owner blocks replacement and competing
input, read, capture, filing, pair, repair, measure, mark, template, freshness and
ordinary author admission until its final take. Generic completion access cannot steal
its answer. A final transfer has no fallible clock read after removing the owner.

## Tests and protective changes

Nineteen new tests use the existing real Unix-stream member fixture and serving trace
writer. They cover ordered populated shape with u64::MAX, exact grants, empty success
versus typed refusal, the original typed response, backpressure before measurement
Recorded, cancellation before send and while recording is blocked, one request without
cursor cleanup, original control expiry, Enter/Leave caps, timely Recorded consumer
delay,
interrupted final release, late-proof lifecycle priority, admission conflicts,
completed-owner replacement, malformed responses, EOF, post-enqueue lifecycle
interruption, final take and release custody.

The interruption fixture changes one syscall outcome on a single StateChannel after
its ordinary proof checks. It does not assign a fabricated Recorded proof, completion,
request identity or result. The existing manual coordinator proof test now preserves its
actual recording timestamp to match the ordinary State completion invariant.

Focused tests, the full harness suite, all-target lint, formatting, lock checks and
compiled protective mutations remain pending. No test total or conformance result is
claimed for this source yet. The mutation recipes will pin counts/order, refusal
mapping, cancellation, exact retained response, original deadlines, recorded-before-use,
owner replacement, competing admission and final transfer.

## Remaining integration

This owner is not Ports::session_shape or the lifecycle's final grant collector. The
coupled caller migration still must replace those legacy methods and map failed or busy
halt-time grants to the existing unreadable surface without making a fresh ask. This
owner accepts work only in the active context and offers no failed-run query escape.

These results are inspection data. They cannot construct RecordedRead, HeldMaterial,
SelectedUnit, a placement grant, a generation grant or an effective permission. No
workflow selection, classifier policy, decoder Open, private journal initializer,
failed-run teardown or production qualification is implemented here. Pending Spec
proposals and operator artifacts supply no authority to this slice.
