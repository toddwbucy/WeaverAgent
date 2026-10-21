# Recording support and held completion routing

Date: 2026-10-21. Status: locally verified component implementation.

This is [draft PR #202][implementation], stacked on [PR #201][base]. It adds
recording and completion mechanics needed by the later run-wide context. The
complete legacy serving path remains active under FOUNDATION-TRANSITION in the
[foundation Spec][foundation].

## Scope and authority

The [harness amendment][harness] requires one original author ticket, one owner
per transport, and a trace record before a completion can affect a decision.
The [trace amendment][trace] supplies bounded admission, terminal reservations,
and the capacity descriptor. This slice connects those component duties without
claiming the full context or common pump.

| Component | Owning clauses | Implemented boundary |
| --- | --- | --- |
| Trace wake and drain status | TRACE-WAKE, TRACE-CAPACITY, HARNESS-PUMP | One bounded notification read and a nonblocking drain predicate |
| Preflight terminal records | TRACE-STOP, TRACE-CAPACITY | Reserved fault and unload through consumed preflight states |
| Original author custody | HARNESS-AWAIT-AUTHOR, HARNESS-RECORD | Preserve AuthoredEvent and a local ticket across preparation and capacity waits |
| Terminal author bypass | HARNESS-RECORD-LIFETIME, TRACE-STOP | Stop the tap, retain the pending author, and admit reserved terminal evidence |
| State transport progress | HARNESS-LEDGER, HARNESS-PUMP | Retry interruptions without premature sends or premature answer release |
| Typed completion routing | HARNESS-OWNERS, HARNESS-READ, TEMPLATE-CHECK | Ordinary controls and Template or UnselectedModel checks use existing sole owners |

The last row covers transport and recorded result delivery. It does not complete
the read adapter, template inventory, certificate, or caller workflow named by
those clauses.

## Trace and author custody

In [writer.rs][writer], try_capacity_wake performs one nonblocking read of at most
256 notification bytes. Interrupted is a distinct result, so the caller can retry
within the common pump's existing deadline. Empty is an observation of the wake
descriptor, not proof that the trace queue drained. drain_status checks occupied
entries, including reservations and an in-flight write, without waiting.

The existing notifier now also signals when occupancy reaches zero, either after
the final write or after cancellation of the final reservation. A small queue can
therefore wake a drain wait even when it never reached the capacity threshold.
Standing sink, notifier, and working-structure failures remain failures when the
queue is empty. Neither the status check nor notification handling resets a clock.

In [recorder.rs][recorder], record_terminal_fault consumes Loaded or Refused
preflight and returns Refused. record_terminal_unload consumes Refused and returns
Closed. Neither result can activate a tap. Invalid terminal attempts retain the
prior recorder state and original event. A fault admitted through ordinary
preflight cannot be repeated through the reserved path. Each reserved kind still
uses the writer's existing single-use permit.

In [recording.rs][recording], submit_authored_with_clock retains the complete
AuthoredEvent in AuthorTicket before preparation. The local ticket is not a trace
sequence. The prepared event remains unchanged while capacity is pending, and
resume does not repeat preparation. A second author returns its original event
without replacing the first. A preparation or clock failure retains the original
ticket. A tap or intake failure after admission retains the admitted sequence
instead of reporting an unadmitted author.

submit_terminal_authored uses the recorder's terminal path after the tap stops.
It leaves an unadmitted ordinary ticket in place. resume_trace_only can later
admit that event with the tap still stopped. It returns no completion proof or
state copy. These methods supply the mechanics for the later halt owner. They do
not implement the full halt account or teardown sequence.

## State and operation completion

In [channel.rs][channel], StateInterest::events retains raw POLLRDHUP interest
while a successful answer fills the slot. Ordinary POLLIN is absent in that
state, so a queued second answer cannot drive repeated reads. A terminal probe
does not consume bytes. Observed peer death freezes the channel while preserving
the held measurement, answer, and request progress.

A validated terminal refusal remains held for its measurement record before its
original failure takes effect. Expected closure after that refusal does not
replace the reason or spin the full slot. Successful answers cannot release
after an observed terminal failure.

An interrupted receive check leaves local progress pending and cannot authorize
the remaining request bytes. Interrupted sends and terminal probes also remain
visible through needs_progress. No interruption becomes a new deadline. The
existing request identity, byte offset, and first-send timestamp stay owned.

release consumes the exact RecordedStateMeasurement. If its non-consuming
extra-answer check is interrupted, it returns Pending through None and retains
that proof inside the held slot. resume_release retries the check without another
trace record. Only a resolved check can return StateCompletion. A duplicate
answer or EOF discovered on retry fails without discarding the held evidence.

In [coordinator.rs][coordinator], transport_one progresses the named state or
classifier owner only. The caller can check its absolute bounds between owners.
This method does not perform rotation, choose deadlines, or stand a common pump.

ControlOperation admits Read, Next, Release, Shape, Held, Position, and Grants.
It refuses capture operations because their late answer and abort cleanup need a
paired owner. begin_control uses the existing state counter and stream. begin_check
routes Template and UnselectedModel through the existing classifier owner.
Busy returns the caller's typed operation unchanged.

OperationCompletion retains the originating operation and its recorded result.
State results require state.store.measurement, and classifier results require
ste100.result. These operation results create no filing obligation. A correct
read refusal remains an operation result. A component failure freezes the owner
and retains its account. A full operation slot prevents a later result from
overwriting it. StateRelease holds an interrupted release continuation without
starting another author.

## Acceptance matrix

The following fixtures pass on this branch. They use real trace admission and
local sockets. No production language or throughput qualification follows from
their scripted classifier results.

| Property | Instrument | Gate evidence |
| --- | --- | --- |
| A small queue wakes when its final write finishes | writer_tests.rs real bounded sink and wake descriptor | Passed |
| Reservation cancellation can complete a drain | writer_tests.rs final reservation drop | Passed |
| Wake handling performs one read and preserves Interrupted | writer_tests.rs injected interruption and two notification buffers | Passed |
| Empty occupancy does not hide commit, notifier, or append failure | writer_tests.rs and recorder_tests.rs exact failure checks | Passed |
| Preflight fault and unload retain custody and cannot duplicate permits | recorder_tests.rs full ordinary capacity, malformed events, and compile-fail type checks | Passed |
| Capacity retry preserves the original author and prepared bytes | recording_owner_tests.rs retained ticket and original source fields | Passed |
| Failure before and after trace admission has distinct evidence | recording_owner_tests.rs clock, author, tap, and intake failures | Passed |
| Terminal bypass does not discard or file the pending author | recording_owner_tests.rs stopped tap and trace-only resume | Passed |
| Held state observes half-close without reading a second answer | channel_tests.rs raw interest through MonotonicIo and queued duplicate bytes | Passed |
| Receive interruption cannot permit a premature final send | channel_tests.rs injected EINTR followed by a real queued answer | Passed |
| Release interruption retains one proof and one completion | channel_tests.rs repeated EINTR, duplicate reply, and EOF cases | Passed |
| Control and nonfiling checks record before returning results | coordinator_tests.rs typed result and real trace-pressure fixtures | Passed |
| One transport choice cannot progress another owner | coordinator_tests.rs separately ready state and classifier | Passed |

The final restored tree passes 589 harness tests with 17 inherited ignored
instruments, and 137 trace tests. All new tests run normally. Each affected crate
passes all-targets clippy with -D warnings. Formatting, lock and diff checks pass.
The target is /tmp/weaver-state-recording-context-target with TMPDIR=/tmp,
CARGO_INCREMENTAL=0 and CARGO_PROFILE_DEV_DEBUG=0. Socket tests ran on the host
lane, since the sandbox refuses the fixture socket timeout operation.

Fifteen compiled perturbations fail their intended assertions. They cover original
source custody, ticket overflow, admitted-failure identity, stopped-tap gating,
terminal ticket retention, both empty-queue wake paths, bounded notification reads,
preflight fault uniqueness, interrupted receive, held-state EOF, pending release,
capture-scope refusal, Busy turn custody and release behind a later author. The
bounded-read probe replaces the bounded API with the legacy draining loop.
A compiler failure from an earlier wrong-target probe and an earlier buffer-size
probe are excluded. The corrected complete set passes its protective checks.

The final mutation record is /tmp/state-recording-support-mutations/all-final/.
Its source digests match the restored files. The command driver is
/tmp/state-recording-support-mutations.py, and the restored test log is
/tmp/state-recording-support-final-tests.log. Independent source review found and
closed lost Busy attribution and premature release behind an outstanding author.
It also corrected an idle-writer fixture race and the weak buffer-size probe.

The gates are cargo test -p weaver-trace -p weaver-harness --locked, per-crate
cargo clippy --all-targets --locked -- -D warnings, cargo fmt --all -- --check,
process/gates/lock.sh and git diff --check. These component checks do not complete
the Planner grade or production cutover.

## Integration still owed

The full RecordingContext remains to be implemented across Preflight, Entered,
Releasing, Failed, and Closed. It must preserve the same author, clock origin,
tickets, retention budget, and failure latch through load, Run, and final unload.
The serving author-call census, all caller dispositions, and diagnostic separation
remain part of that migration.

The common pump still owes lifecycle and terminal priority, rotation across ready
sources, buffered work before poll, and absolute-bound checks around each bounded
unit. It must include admin coordination and SPU lifecycle descriptors, as well as
gate, decode, label, state, STE100, and trace wake where each is standing. No new
runtime, deployment clock, or scheduling policy is supplied here.

The complete input-check transaction remains owed, including offer identity,
cancellation, accepted-input custody, and its terminal account. The ordinary
control route does not complete pagination, recall authoring, boot placement, or
capture and save orchestration. Template result routing does not qualify a
production STE100 artifact or certify an inventory. The full halt sequence,
transport retirement, and coupled foundation cutover remain later work.

[base]: https://github.com/toddwbucy/WeaverAgent/pull/201
[foundation]: ../crates/state-management/state-management-foundation-Spec.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[trace]: ../crates/state-management/weaver-trace-Spec-amendment.md
[writer]: ../../crates/weaver-trace/src/serving/writer.rs
[recorder]: ../../crates/weaver-trace/src/serving/recorder.rs
[recording]: ../../crates/weaver-harness/src/flow/recording.rs
[channel]: ../../crates/weaver-harness/src/flow/channel.rs
[coordinator]: ../../crates/weaver-harness/src/flow/coordinator.rs

[implementation]: https://github.com/toddwbucy/WeaverAgent/pull/202
