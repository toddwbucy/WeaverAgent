# Explicit begin-task owner

Date: 2026-10-22
Branch: code/state-begin-task
PR: #242
Epic: #146

## Current scope

This slice implements the explicit begin_task port from HARNESS-MARKS and Spec 06
BOUNDARIES, MARK-FILE, MARK-STORAGE and MARK-READ. It retains one real AcceptedInput,
authors its one turn and message.user through the existing input owner, and waits for
that exact input's committed receipt. Ordinary input consumption creates no task.

The sole mark owner reserves the input receipt before source authoring. Its private
admission check uses the existing input-slot identity and revocation checks. Recorded
alone is insufficient, as are public SeatedInput fields and a copied source ID. The
receipt must name the same complete Text, turn and offered-input causal parent.

After filing, the owner reads marks through the ordinary paged protocol. The private
read purpose prevents another caller from taking or cancelling that result. Cursor
release and recall Recorded precede catalog use. Read refusal remains a typed refusal,
with its operation evidence already recorded by the read owner.

Catalog validation checks the loaded seed, revision 1, consecutive task/prompt IDs,
canonical active parent chain, allowed scope kinds, open boundary facts and current
root. An open scope cannot reuse the closed prompt's identity. These are facts carried
by Marks. The owner does not claim to inspect the closed prompt's hidden store rows.
An existing open task returns its scope without a new mark. Otherwise the next two IDs
are proposals until the store commits the effect. Checked successor exhaustion refuses
this operation. No harness allocation counter persists and no uncertain ID is retried.

The compiled consumer elects the literal-only mark.begin_task template family from the
actual recorded template certificate. Identity and source remain in typed context.
The state.mark event has one role-user Text, the current turn and internal/loop source.
The ordinary classifier and ledger construct the licensed effect after acceptance.
The owner verifies the exact selected context, source and final receipt before returning
its private BegunTask value. Correct template rejection records state_template_rejected
and returns an operation refusal without a component latch. Required unselected input
or mark content and invalid source/context associations are component failures.

The existing Decision window covers the transaction. The read keeps its ordinary
absolute control deadline. Completion retires Decision atomically, so late retrieval
cannot expire already-completed work. Cancellation returns an AcceptedInput only
before consumption. After consumption the seated Message and its receipts remain
private in TaskAccount, with no input or task-scope accessor that grants reuse.
Before an author ticket, a staged mark can be withdrawn. An already-owned author and
filing finish their obligations without a successful task result. No new retry clock is
created after a lifecycle yield.

Cancellation after settlement also suppresses the task result and retains that
private account. It does not undo a committed task or its prompt. The next marks
read can report the existing task, and no dependent generation follows the cancelled
operation. This preserves HARNESS-MARKS' stop/leave disposition rather than adding a
new post-settlement cancellation rule.

## Validation and review

The first focused run compiled and passed ten of eleven tests. Its missing-selection
fixture failed during real partition activation, before the mark owner could exist.
The corrected test now asserts that actual upstream refusal for both message.user and
state.mark. TRACE-VALIDATION requires these projections, so downstream empty-selection
guards are defensive equivalents under current admission. No fabricated active
partition or bypassed AcceptedInput is used. A second fixture attempt reached an active-
writer accessor before activation.
It now drains and reads the real untapped writer. Both failed attempts are retained.
The corrected focused run passed all thirteen tests. The focused harness filter is
context::marks_tests. Thirteen tests use the real recorder, classifier packet channel,
member stream and existing accepted-input owner with synthetic replies. They exercise:

- Delayed input filing, release and mark filing, with no earlier task result.
- Existing-task reuse, correct template rejection and upstream activation refusal
  when either required input or mark projection is missing.
- Pre-author cancellation and continued filing after an authored mark is cancelled.
- Explicit read refusal, successor exhaustion and malformed catalog identities.
- Closed-prompt reuse in the open path and exclusion of unrelated read/author calls.
- Original Decision expiry, timely completion before late retrieval, and thirteen
  nested Leave checkpoints through the original input handoff.
- Mutual mark/freshness exclusion with active and completed retained owners. A genuine
  foreign token tests Busy admission priority and unchanged return, then completes in
  its original context. No capability or owner is fabricated.

Compile-time pins reject Clone for both BegunTask and FiledTaskInput. They also reject
conversion from a public TaskScopeRef into BegunTask, or from SeatedInput into
FiledTaskInput. Four external trait-addition probes must each reach the intended
E0283 ambiguity and restore the exact source. All four probes reached that diagnostic,
and the exact-source restored check passed.

An early independent Executor reading identified closed-prompt reuse and completion
wait retirement. Both corrections and their regressions are included. That reading
was bounded and is not a Planner grade or complete independent review.

The scripted member validates the consumer protocol. It does not establish new SQL
coverage or production language qualification. Existing store begin_task implements the
atomic task/prompt effect. A final integration review must connect its existing tests
to this adapter's validated inputs without presenting scripted receipts as a real store.

## Integration boundary

This is a dormant primitive. It grants a task scope, never a placement, a decoder
operation or generation. It does not activate serving or change worker/dev_loop.
The production instruction words, language artifacts and measured limits remain owed.
Journal mark registration, Open evidence, native mark constructors, plan effects,
placement and generation confirmation remain separate work.

The freshness owner through 1c5e53f is joined. Mark admission excludes retained
freshness ownership, and freshness admission excludes retained marks ownership,
including completed results. Existing generic-author continuation and each port's
exclusions are preserved. The combined gates passed as recorded below.

Current input admission already makes a local AcceptedInput and freshness mutually
exclusive. A retained mark also retains its input snapshot. The explicit freshness-side
mark check is therefore a defensive equivalent of existing snapshot exclusion. Its
removal alone cannot be sold as a killed behavioral mutation. The completed freshness
check in mark admission has a distinct Busy-before-input-validation regression.

## Measured author check

Olympus used private target /tmp/weaver-146-begin-task-target. The full joined harness
suite passed 1289 tests with 18 inherited ignored cases. The focused thirteen tests,
strict all-target harness clippy, formatting and lock checks passed. The first clippy
run found a nested-if style issue and a large exact-selector refusal return. The final
source collapses the condition and documents the local lint allowance. These do not
change behavior. The focused run and strict gates passed after those edits.

All eleven protective mutations compiled, failed the intended assertion and passed
after exact-byte restoration. They cover required projections, closed-prompt reuse,
foreign catalog seed, successor overflow, atomic Decision retirement, exact input
receipt binding, unrelated-author exclusion, nested-yield custody, cancellation,
template rejection and retained freshness exclusion. Four compile probes separately
confirmed the private type properties with E0283. Defensive equivalent guards are not
counted as killed mutations.

The logs, recipes, probe results and source hashes are deposited in the shared bulk
store under weaver-testing/state-management-146-olympus-2026-10-21/begin-task. Earlier
fixture and lint failures are included. Two independent author readings covered the
owner and its admission joins. These checks are not a Planner grade or production
qualification.

## Planner follow-up at da2b445

The added fixtures cover the review's missing catalog and receipt pins. Actual
member pages carry out-of-range and nonadjacent task/prompt IDs through the real
read owner, which fails without authoring state.mark. Revision 1 is already checked
by read_validate before a RecordedRead can exist. A direct catalog inspection test
therefore pins the downstream revision defense without manufacturing such a proof.

The input and mark association checks are extracted as pure predicates over
inspection values. Two real begin_task runs supply their genuine returned receipts.
The tests alter copies of sequence, Text, source run/sequence/block, turn, parent,
context and effect values. They cannot create or replace ReceiptSet, CommittedSource,
RecordedRead or BegunTask. These are defensive predicate tests, not claims that the
current immutable ledger can produce a crossed receipt through a successful channel.
The existing full owner fixture continues to exercise the predicates' real call sites.

An actual post-input/pre-read handoff now tests ordinary-read purpose exclusion and
cancellation before any read or mark starts. Another real template transcript has
the same run, scheme and inventory but different Recorded source sequences. Its
certificate is refused before reserving input filing, and the returned input still
completes under the correct certificate. No certificate fields are forged.

The active Decision check is a pure inspection test of a real retained owner with
missing or altered bound values. Public complete_wait cannot remove this owned wait,
arm cannot replace an armed key, and competing owners are excluded. The test does
not claim to reach that corruption through a valid owner operation. The completed
Decision check has a separate real API fixture: after completion disarms the key,
arm accepts a competing window, and take refuses to transfer the retained task.

The cancellation fixture also covers a settled mark. Its private account still owns
the seated input, catalog and both receipts, while take_unconsumed_input returns None.
The withdrawn, never-recorded cancelled_event remains retained evidence in that
account. It is intentionally not an execution or replay port. The redundant
input_started flag is removed because token presence exactly represents the same
pre-consumption state, including restoration after a refused consume_input call.

Both one-element receipt checks now inspect two iterator positions without allocating
a temporary Vec. Mark authoring reuses the identity mirror functions. Read/filing Busy
still returns Pending after bounded reduction, and the caller must service the common
pump before retrying when no immediate progress is available. This matches the other
private owners and introduces no blocking wait or new timer. Consolidating all owner
admission lists is a separate structural change, not part of these test corrections.

### Counter endpoint

The near-maximum counter is a specified capacity endpoint. MARK-FILE and HARNESS-MARKS
require the next two IDs. With next_span_id equal to u64::MAX - 1, both IDs fit, and the
store's begin_task checks and inserts them in that order. The new real owner fixture
pins the proposal ending at u64::MAX. Its scripted receipt is not a real SQL test.

MARK-READ defines the next catalog value as checked MAX(local_id)+1 and explicitly
requires a CounterExhausted read refusal. The store's reads::checked_next implements
that rule, including after the final representable prompt. The existing fixture pins
the earlier begin_task refusal when only u64::MAX remains, so two IDs no longer fit.
No merged clause reserves a third representable successor. Refusing the MAX - 1
proposal to preserve later marks-read availability would change that allocation
boundary and needs a separate Spec decision. This act preserves the specified endpoint.

### Follow-up validation scope

The eleven added tests comprise ten real owner/receipt fixtures and one direct catalog
inspection. Some real-owner fixtures then inspect altered copies as described above.
All twelve review survivors have a scoped instrument. The receipt source test supplies
three distinct mutation anchors for run, sequence and block. M17 remains equivalent
under the existing input reservation/open-turn exclusions. No forged public grant is
used to make a redundant guard appear mutation-sensitive.

At joined source abdf624, the full harness suite passed 1319 tests with 18 inherited
ignored cases. The 23 begin-task tests, one catalog inspection test, strict all-target
harness clippy, formatting and lock checks passed with the same private target.
All fifteen protective mutations compiled, failed their named assertion and passed
after exact-byte restoration. They distinguish real owner transitions from private
validator defenses as described above.

CodeRabbit review 5609987832 identified two older catalog tests that accepted any
RunFailure. Both now require ProtocolFailed with the exact malformed-catalog detail.
All 23 begin-task tests, strict clippy and formatting passed after that assertion-only
change. The full-suite count belongs to abdf624 before this final assertion tightening.
The earlier 1289-test and eleven-mutation results remain historical evidence, not the
new head's count. These additions claim no production qualification.

Follow-up logs, recipes and exact source hashes are deposited in the same shared bulk
store under begin-task/review-batch. This is an author check, not a Planner grade.
