# Foundation run dispatch composition

Version: 1.0. Date filed: 2026-10-28. Issue #146.

Branch code/state-foundation-run-dispatch builds on draft #376 at a97e80dd.

The dormant worker dispatcher connects an already adopted FoundationRun to
startup, repeated turns, management, cleanup and actual worker-process exit.
Checked consuming handoffs retain the original run, input, workflow policy,
deadlines and failure evidence. A terminal cleanup owner supplies a typed
process disposition. Closed or a recorded Unload alone grants no successful exit.

Implements: foundation Spec section 1 FOUNDATION-TRANSITION and section 3
FOUNDATION-WALK, harness amendment section 1 HARNESS-OWNERS and
HARNESS-RECORD-LIFETIME, section 3 HARNESS-PUMP, HARNESS-CLOCK and
HARNESS-ENTER-WINDOW, and section 4 HARNESS-HALT and HARNESS-HALT-BOUND,
harness Spec sections 2.2, 3 and 6, admin-harness contract section 4 ordering,
basic-inference-loop sections 4 and 6.

## Authority and author assessment

These are author assessments of the exercised paths, pending Planner review.
They do not grade the whole named section or the complete replacement.

- FOUNDATION-TRANSITION: conforms. Main continues to call dev_loop::drive.
  Foundation dispatch is compiled but unselected. No serving activation occurs.
- FOUNDATION-WALK: conforms on the tested adopted-run routes through startup,
  two input turns, requested save, graceful wind-down, forced Leave, rollback,
  failed teardown and S9. Whole-driver combinations beyond those cases remain
  outside this evidence.
- HARNESS-OWNERS and HARNESS-RECORD-LIFETIME: conforms on checked handoffs.
  One retained run supplies the serving seat and cleanup owners. Failed joins
  return original custody, and process tests require complete trace EOF.
- HARNESS-PUMP and HARNESS-CLOCK: conforms on the tested management waits and
  deadline observations. Real readiness waits retain the exact peer and bounds.
  An observed expiry advances the context clock using its actual witness.
- HARNESS-ENTER-WINDOW: conforms on the adopted Enter's observed-expiry route.
  The original bound survives the wait and cleanup handoff. This slice does not
  supply pre-Enter admission or qualify the complete admin-to-Ready budget.
- HARNESS-HALT and HARNESS-HALT-BOUND: conforms on the exercised failed-startup
  and failed-serving routes, retaining their existing failure and teardown
  owners. No new failure timer or successful failed-run result is introduced.
- Harness Spec sections 2.2 and 3: conforms on retained descriptor custody,
  release, child cleanup and tested final process results. The fixture checks
  the inherited fd9 lock through actual worker death.
- Harness Spec section 6 and basic-inference-loop sections 4 and 6: conforms on
  the exercised orchestration boundary. The worker loop elects turns and
  wind-down passes. The harness continues to own protocol transitions.
- Admin-harness contract section 4: conforms on the exercised entered-state
  ordering refusals. JoinLeave without Leave, Lower and a second Enter answer
  OutOfOrder without replacing the original accepted input or active turn.

No new Spec proposal, workflow election or production-artifact authority follows
from this record. Classification remains in the SPU and the member remains
weaver-state.

## Owner and caller census

foundation_dispatch owns the adopted run until process exit. Startup first
routes a latched component failure, then the already observed Enter expiry,
then an admitted classifier-refusal rollback. A successful Ready yields the
same serving seat to Session. Input and management owners return typed serving
handoffs for entered rollback, forced Leave, graceful Leave or failed exit.

Session and Workflow preserve their original owner on a refused consuming join.
They retain round counts, after-pair elections, wind-down policy and current
input. Completed world replies return that same serving seat for another input.
S9 after graceful capture refusal remains in its existing Leave owner, which
continues management and can accept the later forced Leave.

The caller supplies the restatement family, cut method, positive wind-down pass
count and turn identities. The dispatcher supplies no prompt, placement policy
or new turn-selection policy. Its unsafe entry requires the actual serving
thread and a process dedicated to this run. The policy callback must return
promptly without unwinding. Custody retention is a checked-outcome guarantee,
not a process-wide panic-containment guarantee.

Nine owners expose process_exit. Each yields None before its own completion.
The table omits their shared Foundation prefix.

| Owner | Completed process disposition |
| --- | --- |
| Rollback | Success only with the completed ordinary rollback receipt. |
| EnteredRollback | Success only with complete, healthy, non-refused cleanup. |
| ForcedLeave | Delegates to its retained entered cleanup owner. |
| GracefulLeave | Same. S9 capture refusal supplies no process disposition. |
| ExpiredRollback | Success requires complete accounting and healthy cleanup. |
| FailedExit | Failure, including when Unload was recorded. |
| StartupExit | Failure. |
| RollbackExit | Failure. |
| ExpiredRollbackExit | Failure. |

Abandoned cleanup, terminal protocol refusal, failed release and incomplete Open
accounting cannot become successful exit. A final caller's departure does not
undo completed ordinary cleanup, and success does not invent a delivered Left.
The lost-answer case checks that distinction. A checked handoff that cannot be
routed retains its custody and exits with failure. That fallback writes no
unbounded diagnostic to stderr. An unwritable-stderr subprocess checks that it
does not unwind its retained value.

## Management and waits

Pending management owns its exact peer, opener, answer ticket and accepted
input. Session and Workflow now use its blocking continuation when transport
or organ settlement must wait. A wake returns to reduction before another
wait. Held openers and unfinished Stop work remain with their existing reducers.
The common flow reducer consumes its own completed work and observation tick.
The wait cannot repeatedly poll an already due tick without reducing it.

Only admitted Leave and save/join paths become DirectivePending. Unsupported
entered-state verbs use the retained management exchange to answer OutOfOrder.
The ordering fixture covers accepted input and an active turn, including a
second Enter with actual transferred descriptors. It checks the same input
address, raw request, stage, recorded sequence and original Gate bound after
each refusal. A rejected election waits on existing lifecycle and transport
work without reconstructing the session or renewing a deadline.

## Actual expiry and retained time

The whole-process Enter-expiry case exposed a handoff defect: the pump observed
expiry after a blocking wait, but RecordingContext could retain its earlier
pre-wait reading. Startup stopped on the correct original Enter bound while
its consuming expired handoff could not prove that the bound had elapsed.

PumpStep::Deadline now carries observed_ns from the actual validated expiry
sample. RecordingContext retains it before returning the deadline to its owner.
It reconciles earlier send evidence first, so a later expiry sample cannot
invalidate the timestamp of an earlier physical send. This adds no wire field,
timer, replacement deadline or new clock read.

A related late inspection receipt carries its own Recorded timestamp. Before
returning the enclosing lifecycle expiry, inspection advances last_ns to the
maximum of that retained timestamp and any later valid context observation.
An older retained receipt is not a new reversed-clock reading. The original
completion and lifecycle bound remain available to their consuming owner.

Focused tests require actual post-wait time beyond Enter and Leave bounds, one
sample without resampling, failed and reversed-clock refusal, and the original
send-before-expiry chronology. The late-inspection proof is checked both before
and after a later pump observation with an instrument that rejects any new
clock read or wait. Once the outer bound is retired, the inspection's own
expired control bound still refuses use of its retained result.

## Composition instruments

Ten scenarios run the actual dispatcher in a reexecuted worker process. The
controller receives only fixture endpoints, drives scripted peers and judges
packets, trace bytes, release facts and process status. No live recorder thread
or adopted run crosses a fork. The worker receives the original locked open
file description at fd9. The controller cannot acquire a conflicting lock
until actual worker death. Fixture destructors do not supply that witness.

The ten routes are:

1. Two ordinary input turns followed by graceful wind-down and save.
2. Forced Leave from the entered idle state.
3. Explicit rollback after Ready.
4. Startup classifier refusal followed by explicit rollback.
5. Startup component failure and failed teardown.
6. Actual Enter-window expiry followed by explicit rollback.
7. Serving component failure and failed teardown.
8. Forced Leave while the original message.user filing remains held.
9. Graceful capture refusal at S9, Observe, Stop and later forced Leave.
10. Requested save completion followed by input turns and graceful Leave.

Three management cases cover ordering, a silent retained peer and actual answer
backpressure. The latter two withhold readiness, witness the original descriptor
in the blocking poll and compare thread CPU time with elapsed wait time. They
also require unchanged ownership and deadlines. The retained-answer-lost
rollback case completes the focused set of 14 process/management identities.
It is an owner composition case, not an eleventh whole-dispatcher scenario.

Additional terminal assertions cover failed release, wrong Published, bad
Quiesce acknowledgement, incomplete Open, pending filing, late drain and
abandonment. They distinguish process disposition from an Unload sequence and
from the successful return of a terminal reducer.

These are protocol fixtures. Language, model and member responses are scripted
where the fixtures declare them. The ten driver cases do not exercise actual
admin publication or qualify production SQLite, model or language artifacts.
They do not establish whole-driver joined save/Leave, concurrent secondary
management during capture, every repair/refusal path or every teardown race.
Existing owner cases retain their narrower evidence for those combinations.

## Validation

The private target is /tmp/weaver-state-146-serving-organ-loss-target, with
CARGO_INCREMENTAL=0, pinned nightly and --locked. Lock, formatting and strict
harness all-target clippy pass. The first lint attempt found the move-owned
Session refusal and a fixture unwrap. The session keeps its direct retained
return without a new allocation, as the existing consuming wrappers do. The
fixture now binds the optional held source directly. The failed lint log stays
separate from the final gate.

The ordinary harness gate passes 2,178 cases: 2,096 library, eight worker,
43 integration and 31 doctests, excluding nested child summaries. There are
857 ignored cases and the known #331 case is explicitly filtered. All 789
selected worker process identities pass across the broad run and its two
corrected environment cases. The broad invocation first passed 787 and failed
the actual-admin Observe and Stop cases because their required binary-path
variables were omitted. Both pass with the variables supplied, without source
or binary changes. The original failed invocation remains preserved. These
figures are one set of distinct identities, not a sum of nested child runs.

The 14 focused cases and the actual deadline, broken clock, exact-send,
late-inspection and fatal-custody checks pass. They overlap the broader gates
and are not added to their totals. Before/after hashes match for 770 Rust and
Cargo/toolchain files. Actual privileged process cases run under host root in
a delegated scope. No Cargo build runs under sudo.

Seven independently compiled protective controls fail as intended. They remove
or invent the expiry timestamp, advance it before send reconciliation, discard
the late inspection witness, bypass management blocking, return successful exit
for failed runs, or write an unbounded stderr diagnostic during fatal exit.
Each source is restored by hash. All 770 source/configuration files match the
broad-gate snapshot after the controls.
The restored-source recheck passes both new clock cases, the exact-send and
late-inspection cases, the fatal-custody case and all 14 focused process cases.
Final diff and three document-mirror cases pass. The record is documentation
only and does not change the 770 tested source/configuration hashes.

Earlier failures remain separate evidence: initial fixture compilation,
13-of-14 process runs, the wrong state-read-versus-Enter timing setup, and the
real post-wait expiry handoff failure. A temporary diagnostic let the failed
worker exit naturally before the controller judged its lost Leave answer.
It confirmed exit 1 and trace EOF, then was removed before the passing gates.
No failed run is counted as a passing gate.

Independent read-only review found no actionable runtime defect in the
explicit dispatcher routes and terminal APIs. It identified the callback
precondition and whole-driver coverage limits stated above. Separate clock
review verified actual expiry witnesses, send reconciliation order, both
pump consumers and late Recorded inspection proof. The repository's requested
code-review skill is unavailable in this session, as carried from the parent.
These are author checks, not a Planner grade or security disposition.

Raw logs, source manifests and failed attempts live under
/tmp/state-foundation-run-dispatch-20261028/. The published revision and
verified bulk-store evidence deposit are named in the PR body and comment.
#376 has a zero-fix Planner grade at a97e80dd, with S1-S7 and earlier batch
carries retained. This slice does not close those unrelated admin carries.

## Remaining work

The next admission act must join the pre-Enter listener, before-load Observe,
descriptor admission and public admission wait to this already-adopted-run
driver. It must retain the original Enter budget from the real caller and keep
the lock and transferred descriptors through every refusal and cleanup route.

Root plan-update dispatch and its owner must still be joined through the
serving composition, including required-read refusal and prior-cache retention.
This dispatcher does not elect plan content or turn unsupported lifecycle
directives into plan authority. Complete the remaining #146 consumer and author
migration map, inherited review carries and required composition cases before
considering activation.

Production still owes pinned STE100 scheme/dictionary/corpus and classifier
artifacts, measured limits, separately reviewed prompt and exact template
certificates, security disposition, and the retirement/reader map across the
worker and deploy tools. The new language material does not itself qualify the
remaining deployment tuple. Finish the coupled cutover review before selecting
foundation dispatch or retiring the existing serving implementation.

No broad #146 item or epic closes from these fixtures. No deployed agent changes.
Only the operator merges and deploys.

## After-action review

Whole-process composition reached a real wait boundary that earlier owner
fixtures had advanced with controlled clocks. The runtime already reported the
correct timeout, but the next owner lacked its observed time. Retaining the
actual witness repairs the handoff without sampling a replacement instant or
substituting the deadline value. The send chronology and late Recorded proof
tests constrain both directions of that change.

Continue composing from the next missing boundary and keep the present driver
unselected. Extend the same actual-process instruments for admission and
plan-update work, preserving their existing owners and explicit scope limits.
