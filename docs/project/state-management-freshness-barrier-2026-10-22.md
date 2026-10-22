# State management selected-source freshness barrier

Date: 2026-10-22. Executor implementation record for epic #146, item B.
Branch: `code/state-freshness-barrier`, based on draft #239 at `5d6c3de`.
The initial author gates passed. The turn-boundary correction awaits fresh gates.
Planner review remains separate.
No serving activation, production qualification or epic completion is claimed.

## Authority and implemented boundary

Implements the merged [harness amendment][harness] HARNESS-BARRIERS selected-ID
filing pause and HARNESS-READ's pause-before-read ordering. HARNESS-CLOCK supplies
the loaded cut_wait and shared deadline checks. The implementation uses existing
ledger terminal proofs and adds no wire type, member query or placement grant.

prepare_cut takes ownership of an explicit required-ID vector and retains one
Cut bound using the loaded measured elapsed duration and checked arithmetic.
Pending author work, trace pressure, classifier progress and member progress do
not renew it. The shared pump continues to apply earlier Enter/Leave deadlines.
An expired Cut uses the existing CutBound first-failure path. The owner prevents
generic complete_wait from retiring its pending Cut.

The owner consumes FlowInbox through the existing flow reducer. It waits for
deferred whole copies, pending author tickets and queued flow observations before
checking Ledger::barrier against every supplied identity. That ledger's retained
history distinguishes known selected blocks from merely equal sequences, even
after the actual filed or correctly_rejected disposition released the original
content. Unknown, foreign-run and unselected IDs are protocol_failed. The owner
does not use a maximum sequence, member commit counter or capture frontier.

Receipt consumers retain their own proofs. This owner does not reserve a second
receipt subscriber, steal a SourceLease, or reconstruct SourceDisposition. An
empty required set makes no claim about unrequested filing obligations. Existing
background filings remain owned and continue through the common flow path.

## Author and cancellation custody

Admission requires no recorded open turn and refuses active explicit input, output,
filing, pair, repair, read, measurement and capture transactions. It can drain a generic
author already retained before the pause. A resumed generic author returns its exact sequence
to completed_author before another clock or flow hook; its original caller can
still retrieve that result. An initial deferred author's retry repeats every
ordinary admission check. Only that private retained retry can cross the pause;
new author input receives FreshnessOwned and remains refused evidence.

New explicit content owners also refuse the active pause. Existing owner resume
methods remain usable. Even after the Cut is disarmed, an untaken freshness
result keeps new intake, read, measurement and release admission blocked.

Cancellation suppresses the readiness proof but retains the same IDs, pending
work and original Cut. The caller still drives and takes the owner. A cancelled
operation returns its required IDs without a PreparedFreshness. It cannot erase
an admitted source obligation, rearm a missing wait or create a wire refusal.
Halt retains the owner but prevents all result transfer.

On successful completion, the final liveness check precedes an atomic transition
to completed custody and disarming Cut. No fallible hook separates them. Taking
the result also checks liveness before moving custody. PreparedFreshness retains
the context origin, exact required IDs and original bound; it is non-Clone and
has no constructor from a trace sequence. Its accessors are evidence inspection,
not a cut, snapshot, decoder or placement port.

## Instruments and results

Fourteen tests reuse the actual recorder, classifier socket and member framing
fixture. Synthetic classifier responses test mechanics, not language quality.

| Property | Instrument |
| --- | --- |
| Every selected disposition | One filed and one correctly rejected block of the same source event |
| Recorded before ready | Hold an actual member acknowledgement behind a full trace writer |
| Deferred intake | One-block backlog limit retains a later whole copy until the older source settles |
| Exact known identity | Unknown sequence, foreign run and unselected block each fail without a read |
| Existing receipt custody | Complete ordinary filing first; preserve its receipt while checking freshness |
| Pending observations | Real admitted copies queue observations before any new recorder ticket |
| Cancellation | Repeated cancellation retains the original bound and owed source, then returns IDs only |
| Bound ownership | Withheld Filed, earlier Leave, resumed original Cut and fatal Cut expiry |
| Generic author continuation | Initial deferred author, fourteen clock interruption positions and exact once-only caller result |
| Recorder ticket continuation | Real full writer retains the existing ticket across Leave and resumed drain |
| Admission pause | Completed-but-untaken owner refuses read, input, generic author and ordinary filing |
| Refused competing owner | Existing filing keeps its event; refused freshness call keeps its IDs |
| Arithmetic | Deadline overflow returns the IDs and arms no Cut |
| Turn boundary | Actual turn.started refuses Cut, then turn.closed permits it |

The positive fixture checks that Cut is absent before a new ordinary read starts
its own StateControl bound. Context identity is checked against a second real
context. Compile-time ambiguity pins reject Clone and From<Sequence> for the
prepared result. Probe recipes and their results are retained with the gate logs.

The Olympus run used private target /tmp/weaver-146-freshness-target. Lock,
formatting and strict all-target harness clippy passed. Thirteen focused tests
and the full harness suite passed: 1273 tests, with 18 inherited ignored cases.
All seventeen protective mutations compiled and failed their assertions, then
passed after exact restoration. Both compile pins failed with intended E0283
ambiguity errors, and the restored check passed. Independent source review
covered every admission path, retained authors, cancellation and proof transfer.

The first focused filter selected no tests and supplies no evidence. The first
full run found a fixture error: the caller assertion counted the measurement
records used to fill the writer. The corrected fixture identifies the caller's
exact payload while retaining the real pending-ticket check. The full suite and
correctly selected thirteen tests then passed. Both runs remain in the evidence.

Logs, recipes, results and exact source hashes are retained in the shared bulk
store at weaver-testing/state-management-146-olympus-2026-10-21/freshness. These
are author checks, not a Planner grade or integrated qualification.

## Turn-boundary correction

A later authority reading found that the admission guard omitted open_turn. An
actual recorded turn.started can exist after explicit input or output ownership
has ended, so the other owner checks do not prove the between-turn boundary.
Admission now rejects that retained open turn without arming Cut or consuming the
requested IDs. The regression authors real turn.started and turn.closed events,
then checks refusal before the close and successful preparation afterward.

The original record also incorrectly said inward recall must prepare freshness.
That claim is corrected above and below without changing the inward contract.
The earlier gate counts describe 3415405. On the corrected joined source, the
full harness suite passed 1276 tests with 18 inherited ignored cases. The new
focused test, strict all-target harness clippy, formatting and lock passed.
Removing the open-turn guard compiled and failed the new test; exact restoration
passed. The run used /tmp/weaver-146-freshness-target. Logs, recipe and hashes are
in freshness/turn in the shared evidence deposit.

## Remaining integration and assessment

This is a private between-turn primitive. The caller still supplies the actual
required source set. HARNESS-CLOCK starts Cut after the preceding turn closes and
before decode for the next turn. HARNESS-READ requires this pause before mandatory
fresh history, then releases Cut before the read's own deadline starts.

Model-elected inward recall is a separate active-turn operation. Spec 02 RECALL,
internal HARNESS-RECALL and Spec 05 PAIR-STATES require its exact request receipts
and ordinary bounded read/release contract. They do not require an active-turn Cut.
Its future pair-owned read adapter must preserve unrelated owner exclusion, but
must not turn this filing pause into an additional inward execution clock.

The current ordinary read port remains usable for selectors that do not demand
this freshness barrier. No new raw-result or material authority is introduced.
Begin-task marks, complete journal/place ownership, trusted review, boot repair,
serve selection and coupled lifecycle integration remain separate work.

The capture-frontier and OPEN-FACTS proposals provide no implementation authority
here. The existing serving path remains active until the complete coupled
cutover is ready and the operator approves it.

Preparation exposed that flow observations can be queued before pending_record
is set. The narrow has_pending_observations accessor includes both states so a
filing pause cannot skip an already-owed author. Preserve this distinction when
composing future barriers. The gate failure and its correction are recorded above.
No Planner grade is inferred from this draft.

[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
